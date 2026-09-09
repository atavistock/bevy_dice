//! Asset snapshots and validated geometry for background trajectory preparation.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, Weak},
};

use avian3d::prelude::Collider;
use bevy::{
    asset::{AssetId, LoadState, RecursiveDependencyLoadState, UntypedAssetId},
    mesh::VertexAttributeValues,
    prelude::*,
};

use crate::dice::DieKind;
use crate::sim::{DiceOrientations, FaceSymmetries, RecordedThrow};

use super::{arena::DiceArena, cached_roll::RollFailure, diceset::Diceset};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CacheKey {
    pub arena: Entity,
    pub kinds: Vec<DieKind>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArenaStamp {
    configuration: Vec<u32>,
    diceset: Entity,
    meshes: Vec<AssetId<Mesh>>,
    faces: Vec<Vec<(String, [u32; 3])>>,
}

#[derive(Clone)]
pub struct SimulationTemplate {
    pub arena: DiceArena,
    pub kinds: Vec<DieKind>,
    pub gravity: Vec3,
    pub geometry: Vec<Arc<SharedGeometry>>,
    pub orientations: DiceOrientations,
    pub stamp: ArenaStamp,
}

#[derive(Clone)]
pub struct PreparedGeometry {
    pub colliders: Vec<Collider>,
    pub symmetries: Vec<Arc<FaceSymmetries>>,
}

/// Immutable mesh data with lazily prepared collision and face geometry.
pub struct SharedGeometry {
    vertices: Box<[Vec3]>,
    prepared: OnceLock<Result<(Collider, Arc<FaceSymmetries>), RollFailure>>,
}

#[derive(Clone, Eq, PartialEq, Hash)]
struct GeometryKey {
    mesh: AssetId<Mesh>,
    kind: DieKind,
    faces: Vec<(String, [u32; 3])>,
}

/// Weak entries share geometry without extending the lifetime of unused meshes.
#[derive(Default)]
pub struct GeometryCache {
    entries: HashMap<GeometryKey, Weak<SharedGeometry>>,
}

impl GeometryCache {
    pub fn invalidate(&mut self, modified: &HashSet<AssetId<Mesh>>) {
        self.entries.retain(|key, geometry| !modified.contains(&key.mesh) && geometry.strong_count() > 0);
    }

    fn get(&mut self, key: GeometryKey, meshes: &Assets<Mesh>) -> Result<Arc<SharedGeometry>, RollFailure> {
        if let Some(geometry) = self.entries.get(&key).and_then(Weak::upgrade) {
            return Ok(geometry);
        }
        let mesh = meshes.get(key.mesh).ok_or(RollFailure::AssetUnavailable)?;
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            return Err(RollFailure::InvalidGeometry);
        };
        let vertices: Box<[Vec3]> = positions.iter().copied().map(Vec3::from_array).collect();
        if vertices.len() < 4 || vertices.iter().any(|position| !position.is_finite()) {
            return Err(RollFailure::InvalidGeometry);
        }
        let geometry = Arc::new(SharedGeometry { vertices, prepared: OnceLock::new() });
        self.entries.insert(key, Arc::downgrade(&geometry));
        Ok(geometry)
    }
}

/// Capture configuration and asset identities without copying mesh vertices.
pub fn current_stamp(world: &World, key: &CacheKey) -> Result<Option<ArenaStamp>, RollFailure> {
    let arena = world.get::<DiceArena>(key.arena).ok_or(RollFailure::MissingArena)?;
    let diceset = world.get::<Diceset>(arena.diceset).ok_or(RollFailure::MissingDiceset)?;
    let Some(handles) = diceset.handles() else { return Ok(None) };
    let meshes = world.get_resource::<Assets<Mesh>>();
    let materials = world.get_resource::<Assets<StandardMaterial>>();
    let asset_server = world.get_resource::<AssetServer>();
    let mut mesh_ids = Vec::with_capacity(key.kinds.len());
    let mut ready = true;
    for kind in key.kinds.iter() {
        let mesh = handles.mesh(kind.mesh_index());
        let material = handles.material(kind.mesh_index());
        let mesh_loaded = asset_ready(asset_server, mesh.id().untyped())?;
        let material_loaded = asset_ready(asset_server, material.id().untyped())?;
        if !mesh_loaded
            || !material_loaded
            || meshes.is_none_or(|assets| assets.get(&mesh).is_none())
            || materials.is_none_or(|assets| assets.get(&material).is_none())
        {
            ready = false;
        }
        mesh_ids.push(mesh.id());
    }
    Ok(ready.then(|| make_stamp(arena, gravity(world), mesh_ids, &key.kinds, &diceset.orientations)))
}

/// Copy ready mesh positions and arena inputs for use outside the main world.
pub fn snapshot(
    world: &World,
    key: &CacheKey,
    shared: &mut GeometryCache,
) -> Result<Option<SimulationTemplate>, RollFailure> {
    let Some(stamp) = current_stamp(world, key)? else { return Ok(None) };
    let arena = world.get::<DiceArena>(key.arena).ok_or(RollFailure::MissingArena)?;
    let diceset = world.get::<Diceset>(arena.diceset).ok_or(RollFailure::MissingDiceset)?;
    let meshes = world.get_resource::<Assets<Mesh>>().ok_or(RollFailure::AssetUnavailable)?;
    let mut geometry = Vec::with_capacity(key.kinds.len());
    for (index, kind) in key.kinds.iter().enumerate() {
        geometry.push(
            shared.get(
                GeometryKey { mesh: stamp.meshes[index], kind: *kind, faces: stamp.faces[index].clone() },
                meshes,
            )?,
        );
    }
    Ok(Some(SimulationTemplate {
        arena: arena.clone(),
        kinds: key.kinds.clone(),
        gravity: gravity(world),
        geometry,
        orientations: diceset.orientations.clone(),
        stamp,
    }))
}

/// Construct each distinct collider and symmetry lookup once in the background.
pub fn prepare_geometry(template: &SimulationTemplate) -> Result<PreparedGeometry, RollFailure> {
    if template.kinds.len() != template.geometry.len() {
        return Err(RollFailure::InvalidGeometry);
    }
    let mut colliders = Vec::with_capacity(template.kinds.len());
    let mut symmetries = Vec::with_capacity(template.kinds.len());
    for (kind, geometry) in template.kinds.iter().zip(template.geometry.iter()) {
        let prepared = geometry.prepared.get_or_init(|| {
            let symmetry = FaceSymmetries::new(*kind, &template.orientations, &geometry.vertices)
                .map_err(|_| RollFailure::InvalidGeometry)?;
            let collider = Collider::convex_hull(geometry.vertices.to_vec()).ok_or(RollFailure::InvalidGeometry)?;
            Ok((collider, Arc::new(symmetry)))
        });
        let (collider, symmetry) = prepared.as_ref().map_err(Clone::clone)?;
        colliders.push(collider.clone());
        symmetries.push(Arc::clone(symmetry));
    }
    Ok(PreparedGeometry { colliders, symmetries })
}

/// Map raw numeric face labels onto the settled directions of a recording.
pub fn corrections(
    template: &SimulationTemplate,
    geometry: &PreparedGeometry,
    recording: &RecordedThrow,
    values: &[u32],
) -> Result<Vec<Quat>, RollFailure> {
    if values.len() != template.kinds.len()
        || geometry.symmetries.len() != template.kinds.len()
        || recording.kinds != template.kinds
    {
        return Err(RollFailure::InvalidGeometry);
    }
    template
        .kinds
        .iter()
        .zip(values.iter())
        .enumerate()
        .map(|(index, (kind, value))| {
            let label = template
                .orientations
                .faces(*kind)
                .iter()
                .find(|(label, _)| label.parse::<u32>() == Ok(*value))
                .map(|(label, _)| label.as_str())
                .ok_or(RollFailure::InvalidGeometry)?;
            let rotation = recording.final_rotation(index).ok_or(RollFailure::SimulationFailed)?;
            geometry.symmetries[index].correction(label, rotation).ok_or(RollFailure::InvalidGeometry)
        })
        .collect()
}

fn gravity(world: &World) -> Vec3 {
    world
        .get_resource::<super::plugin::DiceSimulationGravity>()
        .map_or(Vec3::NEG_Y * 23.1, |gravity| gravity.acceleration)
}

fn asset_ready(asset_server: Option<&AssetServer>, id: UntypedAssetId) -> Result<bool, RollFailure> {
    let Some(asset_server) = asset_server else { return Ok(true) };
    let state = asset_server.get_load_state(id);
    let dependencies = asset_server.get_recursive_dependency_load_state(id);
    if matches!(state, Some(LoadState::Failed(_)))
        || matches!(dependencies, Some(RecursiveDependencyLoadState::Failed(_)))
    {
        return Err(RollFailure::AssetUnavailable);
    }
    Ok(!matches!(state, Some(LoadState::NotLoaded | LoadState::Loading))
        && !matches!(
            dependencies,
            Some(RecursiveDependencyLoadState::NotLoaded | RecursiveDependencyLoadState::Loading)
        ))
}

fn make_stamp(
    arena: &DiceArena,
    gravity: Vec3,
    meshes: Vec<AssetId<Mesh>>,
    kinds: &[DieKind],
    orientations: &DiceOrientations,
) -> ArenaStamp {
    let mut configuration = Vec::with_capacity(24);
    for vector in [arena.center, arena.size, gravity] {
        configuration.extend(vector.to_array().map(f32::to_bits));
    }
    configuration.extend(
        [
            arena.physics.restitution,
            arena.physics.friction,
            arena.physics.angular_damping,
            arena.physics.linear_damping,
            arena.spawn.height_above_box,
            arena.spawn.speed,
            arena.spawn.outside_box_buffer,
            arena.spawn.z_spread_fraction,
            arena.spawn.y_jitter,
            arena.spawn.speed_min_factor,
            arena.spawn.speed_jitter,
            arena.spawn.vertical_velocity_fraction,
            arena.spawn.z_velocity_jitter_fraction,
            arena.spawn.angular_speed_factor,
            arena.spawn.pair_z_offset,
        ]
        .map(f32::to_bits),
    );
    let faces = kinds
        .iter()
        .map(|kind| {
            orientations
                .faces(*kind)
                .iter()
                .map(|(label, direction)| (label.clone(), direction.to_array().map(f32::to_bits)))
                .collect()
        })
        .collect();
    ArenaStamp { configuration, diceset: arena.diceset, meshes, faces }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::load_orientations_from_bytes;

    fn template() -> SimulationTemplate {
        let arena = DiceArena::default();
        let orientations = load_orientations_from_bytes(
            br#"{"extras":{"dice_orientations":{"d6":{
            "1":[1,0,0],"2":[-1,0,0],"3":[0,1,0],"4":[0,-1,0],"5":[0,0,1],"6":[0,0,-1]
        }}}}"#,
        );
        let mut vertices = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    vertices.push(Vec3::new(x, y, z));
                }
            }
        }
        let kinds = vec![DieKind::D6, DieKind::D6];
        let gravity = Vec3::NEG_Y * 23.1;
        let stamp = make_stamp(&arena, gravity, Vec::new(), &kinds, &orientations);
        let geometry = Arc::new(SharedGeometry { vertices: vertices.into_boxed_slice(), prepared: OnceLock::new() });
        SimulationTemplate { arena, kinds, gravity, geometry: vec![geometry.clone(), geometry], orientations, stamp }
    }

    #[test]
    fn stamp_tracks_physics_but_ignores_visual_arena_properties() {
        let mut template = template();
        template.arena.name = "renamed".into();
        template.arena.render_layer = Some(3);
        let stamp = make_stamp(&template.arena, template.gravity, Vec::new(), &template.kinds, &template.orientations);
        assert_eq!(stamp, template.stamp);
        template.arena.spawn.speed += 1.0;
        let stamp = make_stamp(&template.arena, template.gravity, Vec::new(), &template.kinds, &template.orientations);
        assert_ne!(stamp, template.stamp);
    }

    #[test]
    fn preparation_shares_symmetries_and_rejects_nonfinite_vertices() {
        let mut template = template();
        let geometry = prepare_geometry(&template).unwrap();
        assert_eq!(geometry.colliders.len(), 2);
        assert!(Arc::ptr_eq(&geometry.symmetries[0], &geometry.symmetries[1]));
        template.geometry[0] =
            Arc::new(SharedGeometry { vertices: vec![Vec3::NAN; 8].into_boxed_slice(), prepared: OnceLock::new() });
        assert!(matches!(prepare_geometry(&template), Err(RollFailure::InvalidGeometry)));
    }

    #[test]
    fn geometry_registry_separates_metadata_and_releases_unused_meshes() {
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Cuboid::new(2.0, 2.0, 2.0));
        let template = template();
        let mut key = GeometryKey { mesh: mesh.id(), kind: DieKind::D6, faces: template.stamp.faces[0].clone() };
        let mut shared = GeometryCache::default();
        let first = shared.get(key.clone(), &meshes).expect("geometry");
        let again = shared.get(key.clone(), &meshes).expect("same geometry");
        assert!(Arc::ptr_eq(&first, &again));
        key.faces[0].0 = "changed".into();
        let changed = shared.get(key, &meshes).expect("changed metadata");
        assert!(!Arc::ptr_eq(&first, &changed));
        drop(first);
        drop(again);
        drop(changed);
        shared.invalidate(&HashSet::new());
        assert!(shared.entries.is_empty());
    }

    #[test]
    fn missing_entities_fail_without_waiting_for_assets() {
        let mut world = World::new();
        let mut key = CacheKey { arena: Entity::PLACEHOLDER, kinds: vec![DieKind::D6] };
        assert!(matches!(snapshot(&world, &key, &mut GeometryCache::default()), Err(RollFailure::MissingArena)));
        key.arena = world.spawn(DiceArena::default()).id();
        assert!(matches!(snapshot(&world, &key, &mut GeometryCache::default()), Err(RollFailure::MissingDiceset)));
    }

    #[test]
    fn corrections_match_raw_labels_and_reject_recording_mismatches() {
        let template = template();
        let geometry = prepare_geometry(&template).unwrap();
        let recording = RecordedThrow {
            kinds: template.kinds.clone(),
            positions: vec![[0.0; 3]; 2].into_boxed_slice(),
            rotations: vec![Quat::IDENTITY.to_array(); 2].into_boxed_slice(),
        };
        let rotations = corrections(&template, &geometry, &recording, &[1, 6]).unwrap();
        for (rotation, label) in rotations.iter().zip(["1", "6"]) {
            assert_eq!(template.orientations.up_face(DieKind::D6, *rotation), Some(label));
        }
        assert!(matches!(corrections(&template, &geometry, &recording, &[1]), Err(RollFailure::InvalidGeometry)));
        assert!(matches!(corrections(&template, &geometry, &recording, &[0, 6]), Err(RollFailure::InvalidGeometry)));
    }
}
