//! Playing-area component: the box that contains tumbling dice. Each
//! [`DiceArena`] entity carries its own diceset, orientations, physics tuning,
//! and throw-arc tuning. The walls + floor are spawned as child colliders by
//! [`spawn_arena_walls`].

use avian3d::prelude::*;
use bevy::prelude::*;

use super::orientations::DiceOrientations;
#[cfg(any(
    feature = "plain_white",
    feature = "halloween",
    feature = "metal",
    feature = "clear_orange"
))]
use super::orientations::load_orientations_from_bytes;

/// Thickness of the static wall/floor colliders that contain the dice.
const WALL_THICKNESS: f32 = 0.5;

/// A playing area. Spawn one per concurrent dice region. Dice tumble inside
/// its box and report results scoped to it. Pair with [`DefaultArena`] to
/// mark the arena that [`super::DiceRoller::roll`] uses when no arena is named.
///
/// Build with [`DiceArena::default`] + chained setters:
///
/// ```ignore
/// let arena = DiceArena::default()
///     .name("user")
///     .center(-10.0, 0.0, 0.0)
///     .size(8.0, 4.0, 6.0)
///     .diceset("plain_white_diceset")
///     .orientations(load_orientations("/full/path/to/plain_white_diceset.glb"));
/// commands.spawn((arena, DefaultArena));
/// ```
#[derive(Component, Clone)]
pub struct DiceArena {
    /// Caller-supplied label for logging and debugging. Not enforced unique.
    pub name: String,
    /// World-space center of the arena's box; floor sits at `center.y`.
    pub center: Vec3,
    /// Full extents (width, height, depth) of the arena's box.
    pub size: Vec3,
    /// Asset-server path to the gltf containing this arena's diceset meshes.
    pub diceset: String,
    /// Local face-direction table for the diceset.
    pub orientations: DiceOrientations,
    pub physics: DicePhysicsConfig,
    pub spawn: SpawnConfig,
}

impl Default for DiceArena {
    fn default() -> Self {
        Self {
            name: String::new(),
            center: Vec3::ZERO,
            size: Vec3::new(12.0, 5.0, 6.0),
            diceset: String::new(),
            orientations: DiceOrientations::default(),
            physics: DicePhysicsConfig::default(),
            spawn: SpawnConfig::default(),
        }
    }
}

impl DiceArena {
    /// Sets the arena's label. Used for logs and debugging only.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Sets the arena's center as world-space (x, y, z). Floor sits at `y`.
    pub fn center(mut self, x: f32, y: f32, z: f32) -> Self {
        self.center = Vec3::new(x, y, z);
        self
    }

    /// Sets the arena's full box extents (width, height, depth).
    pub fn size(mut self, x: f32, y: f32, z: f32) -> Self {
        self.size = Vec3::new(x, y, z);
        self
    }

    /// Sets the diceset asset path. Accepts either the bare slug
    /// (`"plain_white_diceset"`) or the full filename (with `.glb` or `.gltf`);
    /// if neither extension is present, `.glb` is appended.
    pub fn diceset(mut self, path: impl Into<String>) -> Self {
        let mut value = path.into();
        if !value.ends_with(".glb") && !value.ends_with(".gltf") {
            value.push_str(".glb");
        }
        self.diceset = value;
        self
    }

    /// Sets the face-direction table the settle reader uses to map die
    /// rotations back to face labels. Required for dice to report values.
    pub fn orientations(mut self, orientations: DiceOrientations) -> Self {
        self.orientations = orientations;
        self
    }

    /// Overrides the per-die physics tuning for this arena.
    pub fn physics(mut self, physics: DicePhysicsConfig) -> Self {
        self.physics = physics;
        self
    }

    /// Overrides the throw-arc tuning for dice entering this arena.
    pub fn spawn_config(mut self, spawn: SpawnConfig) -> Self {
        self.spawn = spawn;
        self
    }
}

// === embedded diceset constructors ===

#[cfg(feature = "plain_white")]
impl DiceArena {
    /// Arena pre-configured with the embedded `plain_white` diceset. Requires
    /// the `plain_white` cargo feature.
    pub fn plain_white() -> Self {
        Self::default()
            .name("plain_white")
            .diceset("embedded://bevy_dice/plain_white_diceset.glb")
            .orientations(load_orientations_from_bytes(include_bytes!(
                "../../assets/plain_white_diceset.glb"
            )))
    }
}

#[cfg(feature = "halloween")]
impl DiceArena {
    /// Arena pre-configured with the embedded `halloween` diceset. Requires
    /// the `halloween` cargo feature.
    pub fn halloween() -> Self {
        Self::default()
            .name("halloween")
            .diceset("embedded://bevy_dice/halloween_diceset.glb")
            .orientations(load_orientations_from_bytes(include_bytes!(
                "../../assets/halloween_diceset.glb"
            )))
    }
}

#[cfg(feature = "metal")]
impl DiceArena {
    /// Arena pre-configured with the embedded `metal` diceset. Requires the
    /// `metal` cargo feature.
    pub fn metal() -> Self {
        Self::default()
            .name("metal")
            .diceset("embedded://bevy_dice/metal_diceset.glb")
            .orientations(load_orientations_from_bytes(include_bytes!(
                "../../assets/metal_diceset.glb"
            )))
    }
}

#[cfg(feature = "clear_orange")]
impl DiceArena {
    /// Arena pre-configured with the embedded `clear_orange` diceset. Requires
    /// the `clear_orange` cargo feature.
    pub fn clear_orange() -> Self {
        Self::default()
            .name("clear_orange")
            .diceset("embedded://bevy_dice/clear_orange_diceset.glb")
            .orientations(load_orientations_from_bytes(include_bytes!(
                "../../assets/clear_orange_diceset.glb"
            )))
    }
}

/// Marker for the arena [`super::DiceRoller::roll`] (no-arena form) targets.
/// Add to at most one entity; if more than one carries this marker `roll`
/// returns [`Err(NoDefaultArena::Ambiguous)`](super::NoDefaultArena).
#[derive(Component, Default)]
pub struct DefaultArena;

/// Marker for the static walls and floor of an arena, parented under the
/// arena entity itself. Field references the owning arena for queries that
/// need to filter walls by arena.
#[derive(Component)]
pub struct DiceBoxWall {
    pub arena: Entity,
}

/// Internal marker for arenas whose walls have already been spawned.
#[derive(Component)]
pub(super) struct ArenaWallsSpawned;

// === tunables ===

/// Tunables for the dice rigid bodies inside an arena. Passed straight to
/// avian's per-body components when each die spawns.
#[derive(Clone)]
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
        Self {
            restitution: 0.30,
            friction: 0.8,
            angular_damping: 1.5,
            linear_damping: 0.3,
        }
    }
}

/// Tunables for the throw arc when dice enter an arena. Each die spawns
/// just outside one X wall and is given an inward velocity plus random spin.
#[derive(Clone)]
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

// === wall spawning ===

/// Spawns the floor + four walls under each newly-added [`DiceArena`] entity
/// and tags the arena with [`ArenaWallsSpawned`] so this only runs once.
pub(super) fn spawn_arena_walls(
    mut commands: Commands,
    new_arenas: Query<(Entity, &DiceArena), (Added<DiceArena>, Without<ArenaWallsSpawned>)>,
) {
    for (arena_entity, arena) in new_arenas.iter() {
        let center = arena.center;
        let size = arena.size;
        let half = size * 0.5;
        let wall_y = center.y + half.y;
        let thickness = WALL_THICKNESS;

        let floor = commands
            .spawn((
                DiceBoxWall { arena: arena_entity },
                RigidBody::Static,
                Collider::cuboid(size.x, thickness, size.z),
                Transform::from_xyz(center.x, center.y - thickness * 0.5, center.z),
            ))
            .id();
        commands.entity(arena_entity).add_child(floor);

        for sign in [1.0_f32, -1.0] {
            let wall = commands
                .spawn((
                    DiceBoxWall { arena: arena_entity },
                    RigidBody::Static,
                    Collider::cuboid(thickness, size.y, size.z),
                    Transform::from_xyz(
                        center.x + sign * (half.x + thickness * 0.5),
                        wall_y,
                        center.z,
                    ),
                ))
                .id();
            commands.entity(arena_entity).add_child(wall);
        }
        for sign in [1.0_f32, -1.0] {
            let wall = commands
                .spawn((
                    DiceBoxWall { arena: arena_entity },
                    RigidBody::Static,
                    Collider::cuboid(size.x, size.y, thickness),
                    Transform::from_xyz(
                        center.x,
                        wall_y,
                        center.z + sign * (half.z + thickness * 0.5),
                    ),
                ))
                .id();
            commands.entity(arena_entity).add_child(wall);
        }
        commands.entity(arena_entity).insert(ArenaWallsSpawned);
    }
}
