//! [`DiceArena`] entity: a box that contains tumbling dice. Each arena
//! references a [`super::Diceset`] entity by [`Entity`], plus its own
//! physics and throw tuning.

use avian3d::prelude::*;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::plugin::DiceRenderLayer;

/// Thickness of the static wall/floor colliders that contain the dice.
const WALL_THICKNESS: f32 = 0.5;

/// A playing area; spawn one per concurrent dice region. Tag with
/// [`DefaultArena`] to make it the target of [`super::DiceRoller::roll`].
/// Holds a reference to a [`super::Diceset`] entity for its visuals; many
/// arenas can share one diceset.
///
/// ```ignore
/// let diceset = commands.spawn(Diceset::embedded("plain_white")).id();
/// commands.spawn((DiceArena::default().diceset(diceset), DefaultArena));
/// ```
#[derive(Component, Clone, Reflect)]
#[reflect(Component)]
pub struct DiceArena {
    /// Caller-supplied label for logging and debugging. Not enforced unique.
    pub name: String,
    /// World-space center of the arena's box; floor sits at `center.y`.
    pub center: Vec3,
    /// Full extents (width, height, depth) of the arena's box.
    pub size: Vec3,
    /// Entity carrying the [`super::Diceset`] component this arena draws from.
    /// `Entity::PLACEHOLDER` until [`DiceArena::diceset`] is called.
    pub diceset: Entity,
    /// Render layer for this arena's dice and overhead light. `None` falls
    /// back to [`super::DicePlugin::render_layer`].
    pub render_layer: Option<u8>,
    pub physics: DicePhysicsConfig,
    pub spawn: SpawnConfig,
}

impl Default for DiceArena {
    fn default() -> Self {
        Self {
            name: String::new(),
            center: Vec3::ZERO,
            size: Vec3::new(12.0, 5.0, 6.0),
            diceset: Entity::PLACEHOLDER,
            render_layer: None,
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

    /// Sets the [`super::Diceset`] entity this arena draws from. Spawn the
    /// Diceset first (e.g. `commands.spawn(Diceset::embedded("halloween")).id()`).
    pub fn diceset(mut self, diceset: Entity) -> Self {
        self.diceset = diceset;
        self
    }

    /// Overrides the render layer used for this arena's dice and overhead
    /// light. Cameras that should see the dice must include this layer.
    pub fn render_layer(mut self, layer: u8) -> Self {
        self.render_layer = Some(layer);
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

/// Marker for the arena targeted by [`super::DiceRoller::roll`]; tag at most
/// one entity or `roll` returns [`super::NoDefaultArena::Ambiguous`].
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct DefaultArena;

/// Marker for the static walls and floor of an arena; the owning arena is the
/// `ChildOf` parent.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct DiceBoxWall;

// === tunables ===

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

// === wall + light spawning ===

/// Spawns the floor + four walls + overhead light under each newly-added
/// [`DiceArena`] entity.
pub(super) fn spawn_arena_walls(
    mut commands: Commands,
    new_arenas: Query<(Entity, &DiceArena), Added<DiceArena>>,
    default_layer: Res<DiceRenderLayer>,
) {
    for (arena_entity, arena) in new_arenas.iter() {
        let center = arena.center;
        let half = arena.size * 0.5;
        let wall_y = center.y + half.y;
        let t = WALL_THICKNESS;

        spawn_wall(
            &mut commands, arena_entity,
            Vec3::new(center.x, center.y - t * 0.5, center.z),
            Vec3::new(arena.size.x, t, arena.size.z),
        );
        for sign in [1.0_f32, -1.0] {
            spawn_wall(
                &mut commands, arena_entity,
                Vec3::new(center.x + sign * (half.x + t * 0.5), wall_y, center.z),
                Vec3::new(t, arena.size.y, arena.size.z),
            );
            spawn_wall(
                &mut commands, arena_entity,
                Vec3::new(center.x, wall_y, center.z + sign * (half.z + t * 0.5)),
                Vec3::new(arena.size.x, arena.size.y, t),
            );
        }
        spawn_arena_light(
            &mut commands, arena_entity, center,
            arena.render_layer.unwrap_or(default_layer.0),
        );
        // Children only get a `GlobalTransform` when the parent has a `Transform`.
        commands.entity(arena_entity).insert_if_new(Transform::default());
    }
}

/// Spawns one static collider as a child of `arena_entity`.
fn spawn_wall(commands: &mut Commands, arena_entity: Entity, position: Vec3, size: Vec3) {
    let wall = commands
        .spawn((
            DiceBoxWall,
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Transform::from_translation(position),
        ))
        .id();
    commands.entity(arena_entity).add_child(wall);
}

/// Spawns an overhead directional light on `layer`, parented to the arena.
/// The light is angled ~30deg off vertical with shadows on so the die's
/// polyhedral facets read clearly from above (a purely vertical light
/// would flatten the top face into a featureless silhouette).
fn spawn_arena_light(commands: &mut Commands, arena_entity: Entity, center: Vec3, layer: u8) {
    let light = commands
        .spawn((
            DirectionalLight {
                illuminance: 10_000.0,
                shadow_maps_enabled: true,
                ..default()
            },
            // Offset on X+Z so the light comes from above-corner; the dice
            // pick up varying shading on each face as they tumble.
            Transform::from_xyz(center.x + 5.0, center.y + 10.0, center.z + 5.0)
                .looking_at(center, Vec3::Y),
            RenderLayers::layer(layer as usize),
        ))
        .id();
    commands.entity(arena_entity).add_child(light);
}
