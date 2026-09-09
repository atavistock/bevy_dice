//! Two-entry trajectory queues and nonblocking background refill.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use bevy::{
    asset::AssetEvent,
    camera::visibility::RenderLayers,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
};
use rand::RngCore;

use crate::dice::{DiceRoll, DieKind, RollOutcome};
use crate::sim::{RecordedThrow, SimulationError, SimulationInput, simulate_throw};

use super::{
    cache_assets::{
        CacheKey, PreparedGeometry, SimulationTemplate, corrections, current_stamp, prepare_geometry, snapshot,
    },
    cached_roll::{CachedRollRequest, PrecomputeRequest, RollFailed, RollFailure, validate_outcome, validate_roll},
    diceset::Diceset,
    plugin::DiceRenderLayer,
    rng::DiceRng,
    roller::RollComplete,
    spawn::SpawnedDie,
};

/// Number of ready recordings retained for each arena and physical dice composition.
pub const PRECOMPUTE_QUEUE_DEPTH: usize = 2;

#[derive(Default)]
struct CacheEntry {
    last_refill: u64,
    template: Option<Arc<SimulationTemplate>>,
    geometry: Option<Arc<PreparedGeometry>>,
    ready: VecDeque<RecordedThrow>,
    failure: Option<RollFailure>,
}

struct PendingPresentation {
    roll_id: u64,
    arena: Entity,
    roll: DiceRoll,
    outcome: RollOutcome,
    key: CacheKey,
    values: Vec<u32>,
}

struct JobOutput {
    recording: RecordedThrow,
    geometry: Arc<PreparedGeometry>,
}

struct CacheJob {
    key: CacheKey,
    template: Arc<SimulationTemplate>,
    task: Task<Result<JobOutput, RollFailure>>,
}

struct Playback {
    request: PendingPresentation,
    template: Arc<SimulationTemplate>,
    recording: RecordedThrow,
    rotations: Vec<Quat>,
    entities: Vec<Entity>,
    elapsed: f32,
}

#[derive(Component)]
struct CachedDie {
    arena: Entity,
}

/// Holds two ready throws per observed composition and one background refill job.
#[derive(Resource, Default)]
pub struct PrecomputeCache {
    entries: HashMap<CacheKey, CacheEntry>,
    pending: VecDeque<PendingPresentation>,
    active: HashMap<Entity, Playback>,
    job: Option<CacheJob>,
    refill_serial: u64,
    modified_meshes: HashSet<bevy::asset::AssetId<Mesh>>,
}

impl PrecomputeCache {
    /// Ready recordings for the expression's base dice after keep modifiers.
    pub fn ready_count(&self, arena: Entity, roll: &DiceRoll) -> usize {
        let mut kinds = Vec::new();
        for term in roll.terms.iter() {
            let mut count = term.count;
            for limit in [term.options.keep_highest, term.options.keep_lowest].into_iter().flatten() {
                count = count.min(limit);
            }
            if count > crate::sim::MAX_DICE_PER_ROLL as u32 {
                return 0;
            }
            for _ in 0..count {
                kinds.push(term.kind);
                if term.kind == DieKind::D100 {
                    kinds.push(DieKind::D10);
                }
            }
        }
        kinds.sort_by_key(|kind| kind.mesh_index());
        self.entries.get(&CacheKey { arena, kinds }).map_or(0, |entry| entry.ready.len())
    }

    /// Number of arena/composition queues currently observed.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    fn request(&mut self, request: CachedRollRequest, rng: &mut DiceRng) -> Result<(), RollFailure> {
        validate_roll(&request.roll).map_err(RollFailure::InvalidOutcome)?;
        let outcome = request.outcome.unwrap_or_else(|| request.roll.roll_detailed(rng));
        validate_outcome(&request.roll, &outcome).map_err(RollFailure::InvalidOutcome)?;
        let (kinds, values) = physical_dice(&outcome);
        let key = CacheKey { arena: request.arena, kinds };
        if !key.kinds.is_empty() {
            self.entries.entry(key.clone()).or_default().failure = None;
        }
        self.pending.push_back(PendingPresentation {
            roll_id: request.roll_id,
            arena: request.arena,
            roll: request.roll,
            outcome,
            key,
            values,
        });
        Ok(())
    }

    fn refresh(&mut self, world: &World) {
        self.entries.retain(|key, _| world.get::<super::arena::DiceArena>(key.arena).is_some());
        for (key, entry) in self.entries.iter_mut() {
            let changed_mesh = entry.template.as_ref().is_some_and(|template| {
                let Some(diceset) = world.get::<Diceset>(template.arena.diceset) else { return true };
                let Some(handles) = diceset.handles() else { return true };
                key.kinds.iter().any(|kind| self.modified_meshes.contains(&handles.mesh(kind.mesh_index()).id()))
            });
            let stamp = current_stamp(world, key);
            let unchanged = matches!(&stamp, Ok(Some(stamp)) if !changed_mesh && entry.template.as_ref().is_some_and(|template| &template.stamp == stamp));
            if unchanged {
                continue;
            }
            entry.ready.clear();
            entry.geometry = None;
            entry.template = None;
            match stamp {
                Err(err) => {
                    entry.failure = Some(err);
                }
                Ok(None) => {
                    entry.failure = None;
                }
                Ok(Some(_)) => match snapshot(world, key) {
                    Ok(Some(template)) => {
                        entry.template = Some(Arc::new(template));
                        entry.failure = None;
                    }
                    Ok(None) => {
                        entry.failure = None;
                    }
                    Err(err) => {
                        entry.failure = Some(err);
                    }
                },
            }
        }
        self.modified_meshes.clear();
    }

    fn collect_job(&mut self) {
        let Some(job) = self.job.as_mut() else { return };
        let Some(result) = check_ready(&mut job.task) else { return };
        let Some(job) = self.job.take() else { return };
        let Some(entry) = self.entries.get_mut(&job.key) else { return };
        if !entry.template.as_ref().is_some_and(|template| Arc::ptr_eq(template, &job.template)) {
            return;
        }
        match result {
            Ok(output) => {
                entry.geometry = Some(output.geometry);
                if entry.ready.len() < PRECOMPUTE_QUEUE_DEPTH {
                    entry.ready.push_back(output.recording);
                }
            }
            Err(err) => {
                warn!("could not precompute dice for {:?}: {err}", job.key.arena);
                if err != RollFailure::SimulationFailed {
                    entry.failure = Some(err);
                }
            }
        }
    }

    fn refill(&mut self, world: &mut World) {
        if self.job.is_some() {
            return;
        }
        let needs_refill = |entry: &CacheEntry| {
            entry.template.is_some() && entry.failure.is_none() && entry.ready.len() < PRECOMPUTE_QUEUE_DEPTH
        };
        let key = self
            .entries
            .iter()
            .filter(|(_, entry)| needs_refill(entry))
            .min_by_key(|(key, entry)| {
                let demanded = self.pending.iter().any(|request| &request.key == *key);
                (entry.last_refill, !demanded, entry.ready.len())
            })
            .map(|(key, _)| key.clone());
        let Some(key) = key else { return };
        let Some(entry) = self.entries.get_mut(&key) else { return };
        let Some(template) = entry.template.clone() else { return };
        let geometry = entry.geometry.clone();
        self.refill_serial = self.refill_serial.wrapping_add(1);
        entry.last_refill = self.refill_serial;
        let seed = world.resource_mut::<DiceRng>().next_u64();
        let input = template.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let geometry = match geometry {
                Some(geometry) => geometry,
                None => Arc::new(prepare_geometry(&input)?),
            };
            let recording = simulate_throw(SimulationInput {
                arena: (&input.arena).into(),
                kinds: input.kinds.clone(),
                colliders: geometry.colliders.clone(),
                orientations: input.orientations.clone(),
                gravity: input.gravity,
                seed,
            })
            .map_err(|err| match err {
                SimulationError::InvalidInput => RollFailure::InvalidGeometry,
                SimulationError::DidNotSettle => RollFailure::SimulationFailed,
            })?;
            Ok(JobOutput { recording, geometry })
        });
        self.job = Some(CacheJob { key, template, task });
    }

    fn play(&mut self, world: &mut World) {
        let delta = world.resource::<Time>().delta_secs();
        let mut finished = Vec::new();
        for (arena, playback) in self.active.iter_mut() {
            let failure = if world.get::<super::arena::DiceArena>(*arena).is_none() {
                Some(RollFailure::MissingArena)
            } else if !self
                .entries
                .get(&playback.request.key)
                .and_then(|entry| entry.template.as_ref())
                .is_some_and(|template| Arc::ptr_eq(template, &playback.template))
            {
                Some(RollFailure::ArenaChanged)
            } else if playback.entities.iter().any(|entity| world.get::<Transform>(*entity).is_none()) {
                Some(RollFailure::DiceRemoved)
            } else {
                None
            };
            if let Some(failure) = failure {
                finished.push((*arena, Some(failure)));
                continue;
            }
            playback.elapsed = (playback.elapsed + delta).min(playback.recording.duration());
            for (body, entity) in playback.entities.iter().enumerate() {
                let Some((position, rotation)) = playback.recording.pose(body, playback.elapsed) else { continue };
                if let Some(mut transform) = world.get_mut::<Transform>(*entity) {
                    transform.translation = position;
                    transform.rotation = rotation * playback.rotations[body];
                }
            }
            if playback.elapsed >= playback.recording.duration() {
                finished.push((*arena, None));
            }
        }
        for (arena, failure) in finished {
            let Some(playback) = self.active.remove(&arena) else { continue };
            if let Some(reason) = failure {
                for entity in playback.entities {
                    world.despawn(entity);
                }
                fail(world, &playback.request, reason);
            } else {
                complete(world, playback.request);
            }
        }
    }

    fn start_ready(&mut self, world: &mut World) {
        let mut blocked: HashSet<Entity> = self.active.keys().copied().collect();
        let count = self.pending.len();
        for _ in 0..count {
            let Some(request) = self.pending.pop_front() else { break };
            if blocked.contains(&request.arena) {
                self.pending.push_back(request);
                continue;
            }
            if world.get::<super::arena::DiceArena>(request.arena).is_none() {
                fail(world, &request, RollFailure::MissingArena);
                continue;
            }
            if request.key.kinds.is_empty() {
                complete(world, request);
                continue;
            }
            let Some(entry) = self.entries.get_mut(&request.key) else {
                fail(world, &request, RollFailure::MissingArena);
                continue;
            };
            if let Some(reason) = entry.failure.clone()
                && entry.ready.is_empty()
            {
                fail(world, &request, reason);
                continue;
            }
            let prepared = entry.template.clone().zip(entry.geometry.clone());
            let Some((template, geometry)) = prepared else {
                blocked.insert(request.arena);
                self.pending.push_back(request);
                continue;
            };
            let Some(recording) = entry.ready.pop_front() else {
                blocked.insert(request.arena);
                self.pending.push_back(request);
                continue;
            };
            let rotations = match corrections(&template, &geometry, &recording, &request.values) {
                Ok(rotations) => rotations,
                Err(reason) => {
                    fail(world, &request, reason);
                    continue;
                }
            };
            let Some(diceset) = world.get::<Diceset>(template.arena.diceset) else {
                fail(world, &request, RollFailure::MissingDiceset);
                continue;
            };
            let Some(handles) = diceset.handles().cloned() else {
                fail(world, &request, RollFailure::AssetUnavailable);
                continue;
            };
            let layer = world
                .get::<super::arena::DiceArena>(request.arena)
                .and_then(|arena| arena.render_layer)
                .unwrap_or(world.resource::<DiceRenderLayer>().layer);
            let previous: Vec<Entity> = world
                .query::<(Entity, &CachedDie)>()
                .iter(world)
                .filter(|(_, die)| die.arena == request.arena)
                .map(|(entity, _)| entity)
                .collect();
            for entity in previous {
                world.despawn(entity);
            }
            let mut entities = Vec::with_capacity(recording.kinds.len());
            for (body, kind) in recording.kinds.iter().enumerate() {
                let Some((position, rotation)) = recording.pose(body, 0.0) else { continue };
                entities.push(
                    world
                        .spawn((
                            CachedDie { arena: request.arena },
                            SpawnedDie { kind: *kind, arena: request.arena },
                            Mesh3d(handles.mesh(kind.mesh_index())),
                            MeshMaterial3d(handles.material(kind.mesh_index())),
                            Transform::from_translation(position).with_rotation(rotation * rotations[body]),
                            RenderLayers::layer(layer as usize),
                        ))
                        .id(),
                );
            }
            blocked.insert(request.arena);
            self.active
                .insert(request.arena, Playback { request, template, recording, rotations, entities, elapsed: 0.0 });
        }
    }
}

fn physical_dice(outcome: &RollOutcome) -> (Vec<DieKind>, Vec<u32>) {
    let mut dice = Vec::new();
    for die in outcome.dice.iter() {
        match die.kind {
            DieKind::D100 => {
                dice.push((DieKind::D100, die.value % 100 / 10 * 10));
                dice.push((DieKind::D10, die.value % 10));
            }
            DieKind::D10 => dice.push((DieKind::D10, die.value % 10)),
            _ => dice.push((die.kind, die.value)),
        }
    }
    dice.sort_by_key(|(kind, _)| kind.mesh_index());
    dice.into_iter().unzip()
}

fn fail(world: &mut World, request: &PendingPresentation, reason: RollFailure) {
    world.write_message(RollFailed { roll_id: request.roll_id, arena: request.arena, reason });
}

fn complete(world: &mut World, request: PendingPresentation) {
    world.write_message(RollComplete {
        roll_id: request.roll_id,
        arena: request.arena,
        roll: request.roll,
        outcome: request.outcome,
    });
}

pub fn queue_cached_rolls(
    mut requests: MessageReader<CachedRollRequest>,
    mut warming: MessageReader<PrecomputeRequest>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut cache: ResMut<PrecomputeCache>,
    mut rng: ResMut<DiceRng>,
    mut failed: MessageWriter<RollFailed>,
) {
    for event in mesh_events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            cache.modified_meshes.insert(*id);
        }
    }
    for request in requests.read() {
        if let Err(reason) = cache.request(request.clone(), &mut rng) {
            failed.write(RollFailed { roll_id: request.roll_id, arena: request.arena, reason });
        }
    }
    for request in warming.read() {
        if let Err(err) = validate_roll(&request.roll) {
            warn!("cannot precompute expression: {err}");
            continue;
        }
        let outcome = request.roll.roll_detailed(&mut *rng);
        if let Err(err) = validate_outcome(&request.roll, &outcome) {
            warn!("cannot precompute expression: {err}");
            continue;
        }
        let (kinds, _) = physical_dice(&outcome);
        if !kinds.is_empty() {
            cache.entries.entry(CacheKey { arena: request.arena, kinds }).or_default().failure = None;
        }
    }
}

pub fn update_precompute_cache(world: &mut World) {
    world.resource_scope(|world, mut cache: Mut<PrecomputeCache>| {
        cache.refresh(world);
        cache.collect_job();
        cache.play(world);
        cache.start_ready(world);
        cache.refill(world);
    });
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::super::diceset::GltfAssetHandles;
    use super::*;
    use crate::dice::RolledDie;
    use crate::render::DiceArena;
    use crate::sim::load_orientations_from_bytes;

    fn fixture() -> (App, Entity, DiceRoll) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<PrecomputeCache>();
        app.insert_resource(DiceRng::from_seed(1234));
        app.insert_resource(DiceRenderLayer { layer: 0 });
        app.add_message::<RollComplete>();
        app.add_message::<RollFailed>();
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Cuboid::new(1.0, 1.0, 1.0));
        let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
        let orientations = load_orientations_from_bytes(br#"{"extras":{"dice_orientations":{"d6":{"1":[0,1,0],"2":[1,0,0],"3":[0,0,1],"4":[0,0,-1],"5":[-1,0,0],"6":[0,-1,0]}}}}"#);
        let mut diceset = Diceset::custom_with("cube", orientations);
        diceset.handles = Some(GltfAssetHandles::for_test(mesh, material));
        let diceset = world.spawn(diceset).id();
        let arena = world.spawn(DiceArena::default().diceset(diceset)).id();
        (app, arena, DiceRoll::parse("1d6").expect("valid expression"))
    }

    fn submit(world: &mut World, arena: Entity, roll: &DiceRoll, roll_id: u64) {
        let request = CachedRollRequest {
            arena,
            roll_id,
            roll: roll.clone(),
            outcome: Some(RollOutcome {
                total: 6,
                term_lengths: vec![1],
                dice: vec![RolledDie { kind: DieKind::D6, value: 6, negate: false }],
            }),
        };
        world.resource_scope(|world, mut cache: Mut<PrecomputeCache>| {
            cache.request(request, &mut world.resource_mut::<DiceRng>()).expect("valid request");
            cache.refresh(world);
        });
    }

    fn recording() -> RecordedThrow {
        RecordedThrow {
            kinds: vec![DieKind::D6],
            positions: vec![[0.0, 0.5, 0.0]; 2].into_boxed_slice(),
            rotations: vec![Quat::IDENTITY.to_array(); 2].into_boxed_slice(),
        }
    }

    fn fill(world: &mut World, arena: Entity, count: usize) {
        let mut cache = world.resource_mut::<PrecomputeCache>();
        let entry = cache.entries.get_mut(&CacheKey { arena, kinds: vec![DieKind::D6] }).expect("registered queue");
        entry.geometry =
            Some(Arc::new(prepare_geometry(entry.template.as_ref().expect("ready assets")).expect("cube geometry")));
        for _ in 0..count {
            entry.ready.push_back(recording());
        }
    }

    fn present(world: &mut World) {
        world.resource_mut::<Time>().advance_by(Duration::from_secs(1));
        world.resource_scope(|world, mut cache: Mut<PrecomputeCache>| {
            cache.play(world);
            cache.start_ready(world);
        });
    }

    #[test]
    fn cold_cache_waits_without_failure_then_plays_filled_entry() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        submit(world, arena, &roll, 1);
        world.resource_mut::<Time<Real>>().advance_by(Duration::from_secs(120));
        for _ in 0..3 {
            present(world);
        }
        assert_eq!(world.resource::<PrecomputeCache>().pending.len(), 1);
        assert!(world.resource::<PrecomputeCache>().active.is_empty());
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
        assert!(world.resource::<Messages<RollComplete>>().is_empty());
        fill(world, arena, 1);
        present(world);
        assert_eq!(world.resource::<PrecomputeCache>().ready_count(arena, &roll), 0);
        assert_eq!(world.resource::<PrecomputeCache>().active.len(), 1);
        present(world);
        present(world);
        let completed: Vec<_> = world.resource_mut::<Messages<RollComplete>>().drain().collect();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].roll_id, 1);
        assert_eq!(completed[0].outcome.total, 6);
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
        let diceset = world.get::<DiceArena>(arena).expect("arena").diceset;
        let orientations = world.get::<Diceset>(diceset).expect("diceset").orientations.clone();
        let mut dice = world.query::<(&CachedDie, &Transform, Option<&avian3d::prelude::RigidBody>)>();
        let (_, transform, body) = dice.iter(world).next().expect("rendered die");
        assert_eq!(orientations.up_face(DieKind::D6, transform.rotation), Some("6"));
        assert!(body.is_none());
    }

    #[test]
    fn exhausted_queue_preserves_fifo_until_next_fill() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        for roll_id in 1..=3 {
            submit(world, arena, &roll, roll_id);
        }
        fill(world, arena, PRECOMPUTE_QUEUE_DEPTH);
        for _ in 0..4 {
            present(world);
        }
        assert_eq!(world.resource::<PrecomputeCache>().pending.len(), 1);
        assert!(world.resource::<PrecomputeCache>().active.is_empty());
        assert_eq!(world.resource::<PrecomputeCache>().ready_count(arena, &roll), 0);
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
        world.resource_mut::<Time<Real>>().advance_by(Duration::from_secs(120));
        present(world);
        assert_eq!(world.resource::<Messages<RollComplete>>().len(), 2);
        fill(world, arena, 1);
        present(world);
        present(world);
        let completed: Vec<_> =
            world.resource_mut::<Messages<RollComplete>>().drain().map(|event| event.roll_id).collect();
        assert_eq!(completed, vec![1, 2, 3]);
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
    }

    #[test]
    fn cold_cache_waits_for_asset_loading_without_a_timeout() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        let diceset = world.get::<DiceArena>(arena).expect("arena").diceset;
        let handles = world.get_mut::<Diceset>(diceset).expect("diceset").handles.take();
        submit(world, arena, &roll, 1);
        world.resource_mut::<Time<Real>>().advance_by(Duration::from_secs(120));
        present(world);
        assert_eq!(world.resource::<PrecomputeCache>().pending.len(), 1);
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
        world.get_mut::<Diceset>(diceset).expect("diceset").handles = handles;
        world.resource_scope(|world, mut cache: Mut<PrecomputeCache>| cache.refresh(world));
        fill(world, arena, 1);
        present(world);
        present(world);
        assert_eq!(world.resource::<Messages<RollComplete>>().len(), 1);
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
    }

    #[test]
    fn background_refill_serves_cold_request_and_restores_two_entries() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        submit(world, arena, &roll, 1);
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            world.resource_mut::<Time>().advance_by(Duration::from_millis(100));
            update_precompute_cache(world);
            assert!(world.resource::<Messages<RollFailed>>().is_empty());
            let cache = world.resource::<PrecomputeCache>();
            assert!(cache.ready_count(arena, &roll) <= PRECOMPUTE_QUEUE_DEPTH);
            if cache.ready_count(arena, &roll) == PRECOMPUTE_QUEUE_DEPTH
                && cache.active.is_empty()
                && cache.pending.is_empty()
            {
                assert!(cache.job.is_none());
                break;
            }
            assert!(Instant::now() < deadline, "background queue did not fill");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(world.resource::<Messages<RollComplete>>().len(), 1);
        submit(world, arena, &roll, 2);
        update_precompute_cache(world);
        let cache = world.resource::<PrecomputeCache>();
        assert_eq!(cache.ready_count(arena, &roll), 1);
        assert!(cache.job.is_some(), "consuming a recording schedules replacement immediately");
    }

    #[test]
    fn arena_changes_discard_precomputed_trajectories() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        submit(world, arena, &roll, 1);
        fill(world, arena, 2);
        world.get_mut::<DiceArena>(arena).expect("arena").size.x += 1.0;
        world.resource_scope(|world, mut cache: Mut<PrecomputeCache>| cache.refresh(world));
        assert_eq!(world.resource::<PrecomputeCache>().ready_count(arena, &roll), 0);
        present(world);
        assert_eq!(world.resource::<PrecomputeCache>().pending.len(), 1);
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
    }

    #[test]
    fn failed_simulation_keeps_waiters_and_schedules_another_attempt() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        submit(world, arena, &roll, 1);
        let key = CacheKey { arena, kinds: vec![DieKind::D6] };
        let mut cache = world.resource_mut::<PrecomputeCache>();
        let template = cache.entries[&key].template.clone().expect("ready template");
        cache.job = Some(CacheJob {
            key,
            template,
            task: AsyncComputeTaskPool::get().spawn(async { Err(RollFailure::SimulationFailed) }),
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while cache.job.is_some() {
            cache.collect_job();
            assert!(Instant::now() < deadline, "completed task was not collected");
            std::thread::yield_now();
        }
        update_precompute_cache(world);
        let cache = world.resource::<PrecomputeCache>();
        assert_eq!(cache.pending.len(), 1);
        assert!(cache.job.is_some());
        assert!(world.resource::<Messages<RollFailed>>().is_empty());
    }

    #[test]
    fn stale_background_result_cannot_repopulate_changed_arena() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        submit(world, arena, &roll, 1);
        let key = CacheKey { arena, kinds: vec![DieKind::D6] };
        {
            let mut cache = world.resource_mut::<PrecomputeCache>();
            let template = cache.entries[&key].template.clone().expect("ready template");
            let geometry = Arc::new(prepare_geometry(&template).expect("cube geometry"));
            cache.job = Some(CacheJob {
                key,
                template,
                task: AsyncComputeTaskPool::get().spawn(async { Ok(JobOutput { recording: recording(), geometry }) }),
            });
        }
        world.get_mut::<DiceArena>(arena).expect("arena").center.x += 10.0;
        world.resource_scope(|world, mut cache: Mut<PrecomputeCache>| cache.refresh(world));
        let mut cache = world.resource_mut::<PrecomputeCache>();
        let deadline = Instant::now() + Duration::from_secs(5);
        while cache.job.is_some() {
            cache.collect_job();
            assert!(Instant::now() < deadline, "completed task was not collected");
            std::thread::yield_now();
        }
        assert_eq!(cache.ready_count(arena, &roll), 0);
        assert_eq!(cache.pending.len(), 1);
    }

    #[test]
    fn root_composition_shares_adjustments_but_separates_arenas() {
        let (mut app, arena, roll) = fixture();
        let world = app.world_mut();
        submit(world, arena, &roll, 1);
        fill(world, arena, 2);
        let cache = world.resource::<PrecomputeCache>();
        assert_eq!(cache.ready_count(arena, &DiceRoll::parse("d6+10").expect("expression")), 2);
        assert_eq!(cache.ready_count(arena, &DiceRoll::parse("2d6").expect("expression")), 0);
        assert_eq!(cache.ready_count(Entity::PLACEHOLDER, &roll), 0);
    }

    #[test]
    fn percentile_faces_preserve_zero_and_hundred_conventions() {
        for value in [1, 10, 99, 100] {
            let outcome = RollOutcome {
                total: value as i32,
                term_lengths: vec![1],
                dice: vec![RolledDie { kind: DieKind::D100, value, negate: false }],
            };
            let (kinds, values) = physical_dice(&outcome);
            assert_eq!(kinds, vec![DieKind::D10, DieKind::D100]);
            let combined = values.iter().sum::<u32>();
            assert_eq!(if combined == 0 { 100 } else { combined }, value);
        }
    }
}
