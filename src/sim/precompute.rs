//! Isolated physics simulation and packed, interpolated throw recordings.

use std::{f32::consts::TAU, fmt, time::Duration};

use avian3d::prelude::*;
use bevy::{
    ecs::schedule::{Schedules, SingleThreadedExecutor},
    prelude::*,
    time::TimeUpdateStrategy,
};
use rand::{Rng, SeedableRng, rngs::StdRng};

use super::{DiceOrientations, MAX_DICE_PER_ROLL, SimulationArena};
use crate::dice::DieKind;

const PHYSICS_HZ: f64 = 64.0;
const RECORDING_HZ: f32 = 32.0;
const MAX_STEPS: usize = 15 * 64;
const MAX_ATTEMPTS: u64 = 3;

pub struct SimulationInput {
    pub arena: SimulationArena,
    pub kinds: Vec<DieKind>,
    pub colliders: Vec<Collider>,
    pub orientations: DiceOrientations,
    pub gravity: Vec3,
    pub seed: u64,
}

pub struct RecordedThrow {
    pub kinds: Vec<DieKind>,
    pub positions: Box<[[f32; 3]]>,
    pub rotations: Box<[[f32; 4]]>,
}

impl RecordedThrow {
    pub fn frame_count(&self) -> usize {
        if self.kinds.is_empty() { 0 } else { self.positions.len().min(self.rotations.len()) / self.kinds.len() }
    }

    pub fn duration(&self) -> f32 {
        self.frame_count().saturating_sub(1) as f32 / RECORDING_HZ
    }

    pub fn pose(&self, body: usize, elapsed: f32) -> Option<(Vec3, Quat)> {
        let frame_count = self.frame_count();
        if body >= self.kinds.len() || frame_count == 0 || !elapsed.is_finite() {
            return None;
        }
        let sample = (elapsed.max(0.0) * RECORDING_HZ).min((frame_count - 1) as f32);
        let first = sample.floor() as usize;
        let second = (first + 1).min(frame_count - 1);
        let first_idx = first * self.kinds.len() + body;
        let second_idx = second * self.kinds.len() + body;
        let fraction = sample - first as f32;
        Some((
            Vec3::from_array(self.positions[first_idx]).lerp(Vec3::from_array(self.positions[second_idx]), fraction),
            Quat::from_array(self.rotations[first_idx]).slerp(Quat::from_array(self.rotations[second_idx]), fraction),
        ))
    }

    pub fn final_rotation(&self, body: usize) -> Option<Quat> {
        self.pose(body, self.duration()).map(|(_, rotation)| rotation)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationError {
    InvalidInput,
    DidNotSettle,
}

impl fmt::Display for SimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "invalid dice simulation input",
            Self::DidNotSettle => "dice did not settle inside the arena within the simulation budget",
        })
    }
}

impl std::error::Error for SimulationError {}

pub fn simulate_throw(input: SimulationInput) -> Result<RecordedThrow, SimulationError> {
    let radius = validate_input(&input)?;
    for attempt in 0..MAX_ATTEMPTS {
        if let Some(recording) = simulate_attempt(&input, radius, input.seed.wrapping_add(attempt)) {
            return Ok(recording);
        }
    }
    Err(SimulationError::DidNotSettle)
}

fn validate_input(input: &SimulationInput) -> Result<f32, SimulationError> {
    let arena = &input.arena;
    let tuning = [
        arena.physics.restitution,
        arena.physics.friction,
        arena.physics.angular_damping,
        arena.physics.linear_damping,
        arena.spawn.speed,
        arena.spawn.angular_speed_factor,
        arena.spawn.speed_min_factor,
        arena.spawn.speed_jitter,
        arena.spawn.vertical_velocity_fraction,
        arena.spawn.z_velocity_jitter_fraction,
    ];
    if input.kinds.is_empty()
        || input.kinds.len() > MAX_DICE_PER_ROLL
        || input.kinds.len() != input.colliders.len()
        || !arena.center.is_finite()
        || !arena.size.is_finite()
        || arena.size.min_element() <= 0.0
        || !input.gravity.is_finite()
        || input.gravity.y >= 0.0
        || tuning.iter().any(|value| !value.is_finite() || *value < 0.0)
        || input.kinds.iter().any(|kind| input.orientations.faces(*kind).is_empty())
    {
        return Err(SimulationError::InvalidInput);
    }
    let mut radius = 0.0_f32;
    for collider in &input.colliders {
        let bounds = collider.aabb(Vec3::ZERO, Quat::IDENTITY);
        let extent = bounds.min.abs().max(bounds.max.abs());
        if !extent.is_finite() || extent.min_element() <= 0.0 {
            return Err(SimulationError::InvalidInput);
        }
        let sphere = collider.shape_scaled().compute_local_bounding_sphere();
        let collider_radius = sphere.center.length() + sphere.radius;
        if !collider_radius.is_finite() || collider_radius <= 0.0 {
            return Err(SimulationError::InvalidInput);
        }
        radius = radius.max(collider_radius);
    }
    if arena.size.x <= radius * 2.1 || arena.size.z <= radius * 2.1 || arena.size.y <= radius * 2.1 {
        return Err(SimulationError::InvalidInput);
    }
    Ok(radius)
}

fn simulation_app(input: &SimulationInput) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, PhysicsPlugins::default()));
    app.add_message::<AssetEvent<Mesh>>();
    app.init_resource::<Assets<Mesh>>();
    app.insert_resource(Gravity(input.gravity));
    app.insert_resource(Time::<Fixed>::from_hz(PHYSICS_HZ));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / PHYSICS_HZ)));
    let arena = &input.arena;
    let half = arena.size * 0.5;
    let mut walls = vec![(Vec3::new(0.0, -0.25, 0.0), Vec3::new(arena.size.x, 0.5, arena.size.z))];
    for sign in [-1.0, 1.0] {
        walls.push((Vec3::new(sign * (half.x + 0.25), half.y, 0.0), Vec3::new(0.5, arena.size.y, arena.size.z)));
        walls.push((Vec3::new(0.0, half.y, sign * (half.z + 0.25)), Vec3::new(arena.size.x, arena.size.y, 0.5)));
    }
    for (position, size) in walls {
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Transform::from_translation(arena.center + position),
        ));
    }
    app.finish();
    app.cleanup();
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
    app.update();
    app
}

fn simulate_attempt(input: &SimulationInput, radius: f32, seed: u64) -> Option<RecordedThrow> {
    let mut app = simulation_app(input);
    let mut random = StdRng::seed_from_u64(seed);
    let arena = &input.arena;
    let spacing = radius * 2.001;
    let columns = ((arena.size.x / spacing).floor() as usize).min(input.kinds.len());
    let rows = ((arena.size.z / spacing).floor() as usize).min(input.kinds.len().div_ceil(columns));
    let mut entities = Vec::with_capacity(input.kinds.len());
    let mut positions = Vec::new();
    let mut rotations = Vec::new();
    let mut quiet_steps = vec![0usize; input.kinds.len()];
    for (idx, collider) in input.colliders.iter().enumerate() {
        let col = idx % columns;
        let row = (idx / columns) % rows;
        let layer = idx / (columns * rows);
        let position = arena.center
            + Vec3::new(
                (col as f32 - (columns - 1) as f32 * 0.5) * spacing,
                arena.size.y - radius + layer as f32 * spacing,
                (row as f32 - (rows - 1) as f32 * 0.5) * spacing,
            );
        let rotation = Quat::from_euler(
            EulerRot::XYZ,
            random.gen_range(0.0..TAU),
            random.gen_range(0.0..TAU),
            random.gen_range(0.0..TAU),
        );
        let inward = (arena.center - position).with_y(0.0).normalize_or_zero();
        let speed =
            arena.spawn.speed * (arena.spawn.speed_min_factor + random.gen_range(0.0..1.0) * arena.spawn.speed_jitter);
        let velocity = inward * speed * (0.15 / (input.kinds.len() as f32).sqrt())
            + Vec3::NEG_Y * arena.spawn.speed * arena.spawn.vertical_velocity_fraction;
        let angular = random_unit(&mut random) * arena.spawn.speed * arena.spawn.angular_speed_factor;
        entities.push(
            app.world_mut()
                .spawn((
                    RigidBody::Dynamic,
                    collider.clone(),
                    SweptCcd::default(),
                    Transform::from_translation(position).with_rotation(rotation),
                    LinearVelocity(velocity),
                    AngularVelocity(angular),
                    Restitution::new(arena.physics.restitution),
                    Friction::new(arena.physics.friction),
                    AngularDamping(arena.physics.angular_damping),
                    LinearDamping(arena.physics.linear_damping),
                ))
                .id(),
        );
        positions.push(position.to_array());
        rotations.push(rotation.to_array());
    }
    for step in 1..=MAX_STEPS {
        app.update();
        let mut settled = true;
        for (idx, entity) in entities.iter().enumerate() {
            let body = app.world().entity(*entity);
            let position = body.get::<Position>()?.0;
            let rotation = body.get::<Rotation>()?.0;
            if !position.is_finite() || !rotation.is_finite() || position.y < arena.center.y - radius {
                return None;
            }
            if step % 2 == 0 {
                positions.push(position.to_array());
                rotations.push(rotation.to_array());
            }
            let quiet = body.get::<LinearVelocity>()?.0.length_squared() < 0.0025
                && body.get::<AngularVelocity>()?.0.length_squared() < 0.01;
            quiet_steps[idx] = if quiet { quiet_steps[idx] + 1 } else { 0 };
            if !body.contains::<Sleeping>() && quiet_steps[idx] < 32 {
                settled = false;
                continue;
            }
            let bounds = input.colliders[idx].aabb(position, rotation);
            let half = arena.size * 0.5;
            if bounds.min.x < arena.center.x - half.x - 0.05
                || bounds.max.x > arena.center.x + half.x + 0.05
                || bounds.min.z < arena.center.z - half.z - 0.05
                || bounds.max.z > arena.center.z + half.z + 0.05
                || bounds.min.y < arena.center.y - 0.05
                || bounds.max.y > arena.center.y + arena.size.y
            {
                return None;
            }
            let flatness = input
                .orientations
                .faces(input.kinds[idx])
                .iter()
                .map(|(_, direction)| (rotation * *direction).y)
                .fold(f32::NEG_INFINITY, f32::max);
            if flatness < 0.985 {
                settled = false;
                quiet_steps[idx] = 0;
                app.world_mut().entity_mut(*entity).remove::<Sleeping>().insert((
                    AngularVelocity(random_unit(&mut random) * 2.0),
                    LinearVelocity(Vec3::Y * 2.0 + random_unit(&mut random).with_y(0.0)),
                ));
            }
        }
        // Keep the final pose on the fixed recording grid.
        if settled && step % 2 == 0 {
            return Some(RecordedThrow {
                kinds: input.kinds.clone(),
                positions: positions.into_boxed_slice(),
                rotations: rotations.into_boxed_slice(),
            });
        }
    }
    None
}

fn random_unit(random: &mut StdRng) -> Vec3 {
    let angle = random.gen_range(0.0..TAU);
    let cosine: f32 = random.gen_range(-1.0..1.0);
    let sine = (1.0 - cosine * cosine).sqrt();
    Vec3::new(sine * angle.cos(), cosine, sine * angle.sin())
}

#[cfg(test)]
mod tests {
    use super::super::orientations::load_orientations_from_bytes;
    use super::*;

    fn cube_input() -> SimulationInput {
        SimulationInput { arena: SimulationArena::default(), kinds: vec![DieKind::D6],
            colliders: vec![Collider::cuboid(1.0, 1.0, 1.0)],
            orientations: load_orientations_from_bytes(br#"{"extras":{"dice_orientations":{"d6":{"1":[0,1,0],"2":[1,0,0],"3":[0,0,1],"4":[0,0,-1],"5":[-1,0,0],"6":[0,-1,0]}}}}"#),
            gravity: Vec3::new(0.0, -23.1, 0.0), seed: 1234 }
    }

    #[test]
    fn packed_samples_have_no_padding() {
        assert_eq!(std::mem::size_of::<[f32; 3]>() + std::mem::size_of::<[f32; 4]>(), 28);
    }

    #[test]
    fn playback_interpolates_and_clamps_endpoints() {
        let recording = RecordedThrow {
            kinds: vec![DieKind::D6],
            positions: vec![[0.0; 3], [2.0, 0.0, 0.0]].into_boxed_slice(),
            rotations: vec![Quat::IDENTITY.to_array(), Quat::from_rotation_y(1.0).to_array()].into_boxed_slice(),
        };
        assert_eq!(recording.frame_count(), 2);
        assert_eq!(recording.duration(), 1.0 / 32.0);
        let (position, rotation) = recording.pose(0, 1.0 / 64.0).expect("interpolated pose");
        assert_eq!(position, Vec3::X);
        assert!(rotation.angle_between(Quat::from_rotation_y(0.5)) < 0.001);
        assert_eq!(recording.pose(0, -1.0).expect("initial pose").0, Vec3::ZERO);
        assert_eq!(recording.pose(0, 100.0).expect("final pose").0, Vec3::X * 2.0);
        assert!(recording.pose(1, 0.0).is_none());
        assert!(recording.pose(0, f32::NAN).is_none());
    }

    #[test]
    fn rejects_empty_and_invalid_input() {
        let mut input = cube_input();
        input.kinds.clear();
        assert!(matches!(simulate_throw(input), Err(SimulationError::InvalidInput)));
        let mut input = cube_input();
        input.arena.size.x = f32::NAN;
        assert!(matches!(simulate_throw(input), Err(SimulationError::InvalidInput)));
    }

    #[test]
    fn packed_frames_preserve_body_order() {
        let recording = RecordedThrow {
            kinds: vec![DieKind::D6, DieKind::D20],
            positions: vec![[0.0; 3], [10.0, 0.0, 0.0], [2.0, 0.0, 0.0], [14.0, 0.0, 0.0]].into_boxed_slice(),
            rotations: vec![Quat::IDENTITY.to_array(); 4].into_boxed_slice(),
        };
        assert_eq!(recording.frame_count(), 2);
        assert_eq!(recording.pose(0, 1.0 / 64.0).expect("first body").0.x, 1.0);
        assert_eq!(recording.pose(1, 1.0 / 64.0).expect("second body").0.x, 12.0);
    }

    #[test]
    fn headless_cube_settles_and_records_inside_arena() {
        let recording = simulate_throw(cube_input()).expect("cube settles");
        assert!(recording.frame_count() > 1);
        assert_eq!(recording.positions.len(), recording.rotations.len());
        let (position, rotation) = recording.pose(0, recording.duration()).expect("final pose");
        assert!((position.y - 0.5).abs() < 0.05);
        assert!(
            cube_input().orientations.faces(DieKind::D6).iter().any(|(_, direction)| (rotation * *direction).y > 0.985)
        );
    }
}
