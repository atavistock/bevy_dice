//! Physics and throw configuration shared by simulation and presentation.

use bevy::prelude::*;

/// Maximum physical dice per throw; percentile dice consume two bodies each.
pub const MAX_DICE_PER_ROLL: usize = 20;

/// Arena geometry and tuning without rendering or asset references.
#[derive(Clone)]
pub struct SimulationArena {
    pub center: Vec3,
    pub size: Vec3,
    pub physics: DicePhysicsConfig,
    pub spawn: SpawnConfig,
}

impl Default for SimulationArena {
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            size: Vec3::new(12.0, 5.0, 6.0),
            physics: DicePhysicsConfig::default(),
            spawn: SpawnConfig::default(),
        }
    }
}

/// Tunables for the dice rigid bodies inside an arena. Passed straight to
/// avian's per-body components when each die spawns.
#[derive(Clone, Reflect)]
pub struct DicePhysicsConfig {
    /// Bounciness on collision (0 = no bounce, 1 = elastic).
    pub restitution: f32,
    /// Surface friction coefficient against the box walls and other dice.
    pub friction: f32,
    /// Per-second decay applied to angular velocity; higher = settles faster.
    pub angular_damping: f32,
    /// Per-second decay applied to linear velocity.
    pub linear_damping: f32,
}

impl Default for DicePhysicsConfig {
    fn default() -> Self {
        Self { restitution: 0.30, friction: 0.8, angular_damping: 1.5, linear_damping: 0.3 }
    }
}

/// Tunables for the throw arc when dice enter an arena. Each die spawns
/// just outside one X wall and is given an inward velocity plus random spin.
#[derive(Clone, Reflect)]
pub struct SpawnConfig {
    /// Height above the box ceiling where dice begin their arc.
    pub height_above_box: f32,
    /// Magnitude of the inward horizontal velocity at entry.
    pub speed: f32,
    /// Distance beyond the box wall where dice spawn.
    pub outside_box_buffer: f32,
    /// Random z position spread as a fraction of `size.z` (centered range).
    pub z_spread_fraction: f32,
    /// Random vertical jitter applied below `height_above_box`.
    pub y_jitter: f32,
    /// Minimum horizontal-speed multiplier on `speed`.
    pub speed_min_factor: f32,
    /// Additional random range added to the speed multiplier.
    pub speed_jitter: f32,
    /// Initial downward velocity as a fraction of `speed`.
    pub vertical_velocity_fraction: f32,
    /// Random z-velocity spread as a fraction of `speed` (centered range).
    pub z_velocity_jitter_fraction: f32,
    /// Angular speed magnitude relative to `speed`.
    pub angular_speed_factor: f32,
    /// Z-offset between the two members of a d100 pair so they don't spawn
    /// inside each other.
    pub pair_z_offset: f32,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            height_above_box: 2.5,
            speed: 8.0,
            outside_box_buffer: 1.2,
            z_spread_fraction: 0.6,
            y_jitter: 0.8,
            speed_min_factor: 0.85,
            speed_jitter: 0.3,
            vertical_velocity_fraction: 0.15,
            z_velocity_jitter_fraction: 0.35,
            angular_speed_factor: 0.9,
            pair_z_offset: 0.5,
        }
    }
}
