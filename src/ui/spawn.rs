//! Dice rigid-body spawning: throw arc + per-die avian components.

use std::f32::consts::TAU;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::Rng;

use crate::dice::DieKind;

use super::arena::DiceArena;

/// Hard cap on dice entities spawned per roll. A d100 uses two entities, so
/// the cap on logical percentile rolls is half this value.
pub const MAX_DICE_PER_ROLL: usize = 40;

/// Marker for any die spawned by the plugin, plus the arena that owns it.
#[derive(Component)]
pub struct SpawnedDie {
    pub kind: DieKind,
    pub arena: Entity,
}

pub(super) struct ThrowBase {
    entry_sign: f32,
    entry_x: f32,
    base_z: f32,
    base_y: f32,
    horizontal_speed: f32,
    z_velocity: f32,
}

pub(super) fn throw_base<R: Rng + ?Sized>(arena: &DiceArena, rng: &mut R) -> ThrowBase {
    let spawn = &arena.spawn;
    let half = arena.size * 0.5;
    let entry_sign: f32 = if rng.gen_bool(0.5) { -1.0 } else { 1.0 };
    let entry_x = arena.center.x + entry_sign * (half.x + spawn.outside_box_buffer);
    let z_jitter = (rng.gen_range(0.0_f32..1.0) - 0.5) * arena.size.z * spawn.z_spread_fraction;
    let base_z = arena.center.z + z_jitter;
    let base_y = arena.center.y
        + arena.size.y
        + spawn.height_above_box
        - rng.gen_range(0.0_f32..1.0) * spawn.y_jitter;
    let speed_multiplier =
        spawn.speed_min_factor + rng.gen_range(0.0_f32..1.0) * spawn.speed_jitter;
    let horizontal_speed = spawn.speed * speed_multiplier;
    let z_velocity =
        (rng.gen_range(0.0_f32..1.0) - 0.5) * spawn.speed * spawn.z_velocity_jitter_fraction;
    ThrowBase {
        entry_sign,
        entry_x,
        base_z,
        base_y,
        horizontal_speed,
        z_velocity,
    }
}

pub(super) struct SpawnState {
    position: Vec3,
    velocity: Vec3,
    rotation: Quat,
    angular_velocity: Vec3,
}

pub(super) fn member_state<R: Rng + ?Sized>(
    base: &ThrowBase,
    arena: &DiceArena,
    z_offset: f32,
    rng: &mut R,
) -> SpawnState {
    let spawn = &arena.spawn;
    let position = Vec3::new(base.entry_x, base.base_y, base.base_z + z_offset);
    let velocity = Vec3::new(
        -base.entry_sign * base.horizontal_speed,
        -spawn.speed * spawn.vertical_velocity_fraction,
        base.z_velocity,
    );
    let rotation = random_quat(rng);
    let angular_velocity = random_unit_vec(rng) * spawn.speed * spawn.angular_speed_factor;
    SpawnState {
        position,
        velocity,
        rotation,
        angular_velocity,
    }
}

pub(super) fn spawn_die(
    commands: &mut Commands,
    asset_server: &AssetServer,
    kind: DieKind,
    arena_entity: Entity,
    arena: &DiceArena,
    state: &SpawnState,
) -> Entity {
    let mesh_index = DieKind::ALL.iter().position(|&k| k == kind).expect("known kind");
    let mesh: Handle<Mesh> = asset_server.load(
        GltfAssetLabel::Primitive { mesh: mesh_index, primitive: 0 }
            .from_asset(arena.diceset.clone()),
    );
    let material: Handle<StandardMaterial> = asset_server.load(
        GltfAssetLabel::Material { index: mesh_index, is_scale_inverted: false }
            .from_asset(arena.diceset.clone()),
    );
    let transform = Transform::from_translation(state.position).with_rotation(state.rotation);
    let params = &arena.physics;
    commands
        .spawn((
            SpawnedDie { kind, arena: arena_entity },
            Mesh3d(mesh),
            MeshMaterial3d(material),
            transform,
            RigidBody::Dynamic,
            ColliderConstructor::ConvexHullFromMesh,
            SweptCcd::default(),
            LinearVelocity(state.velocity),
            AngularVelocity(state.angular_velocity),
            Restitution::new(params.restitution),
            Friction::new(params.friction),
            AngularDamping(params.angular_damping),
            LinearDamping(params.linear_damping),
        ))
        .id()
}

fn random_quat<R: Rng + ?Sized>(rng: &mut R) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        rng.gen_range(0.0_f32..TAU),
        rng.gen_range(0.0_f32..TAU),
        rng.gen_range(0.0_f32..TAU),
    )
}

fn random_unit_vec<R: Rng + ?Sized>(rng: &mut R) -> Vec3 {
    let phi = rng.gen_range(0.0_f32..TAU);
    let cos_theta = rng.gen_range(-1.0_f32..1.0);
    let sin_theta = (1.0 - cos_theta * cos_theta).sqrt();
    Vec3::new(sin_theta * phi.cos(), cos_theta, sin_theta * phi.sin())
}
