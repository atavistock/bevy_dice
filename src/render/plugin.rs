//! Recorded dice playback, background precomputation, and arena lighting.

use bevy::prelude::*;

use crate::dice::{DiceRoll, DiceTerm, DieKind, Options, RollOutcome, RolledDie};

use super::arena::{DefaultArena, DiceArena, spawn_arena_lights};
use super::cache::{PrecomputeCache, queue_cached_rolls, update_precompute_cache};
use super::cached_roll::{PrecomputeRequest, RollFailed, RollRequest};
use super::diceset::{Diceset, embedded_table, load_diceset_handles};
use super::rng::{DiceRng, DiceSimulationRng};
use super::roller::{NextRollId, RollComplete};
use super::spawn::{SpawnedDie, despawn_orphaned_dice};
use crate::sim::{DicePhysicsConfig, SpawnConfig};

/// Adds cached roll playback and arena lighting, optionally spawning a default arena.
pub struct DicePlugin {
    /// Simulation gravity along Y in m/s^2; `None` defaults to `-23.1`.
    pub gravity: Option<f32>,
    /// Auto-spawn a [`DiceArena`] + [`DefaultArena`] for the enabled embedded
    /// diceset feature. Set `false` when supplying your own arena.
    pub spawn_default_arena: bool,
    /// Default render layer for spawned dice and their overhead light.
    /// Defaults to `0` (shares the host scene). Set higher (e.g. `1`) and
    /// add that layer to your camera to isolate dice lighting.
    pub render_layer: u8,
}

impl Default for DicePlugin {
    fn default() -> Self {
        Self { gravity: None, spawn_default_arena: true, render_layer: 0 }
    }
}

/// Default background simulation gravity along Y.
const DEFAULT_GRAVITY: f32 = -23.1;

/// Gravity used only by background dice simulations.
#[derive(Resource, Clone, Copy, Reflect)]
#[reflect(Resource)]
pub struct DiceSimulationGravity {
    pub acceleration: Vec3,
}

impl Default for DiceSimulationGravity {
    fn default() -> Self {
        Self { acceleration: Vec3::new(0.0, DEFAULT_GRAVITY, 0.0) }
    }
}

/// Resource holding the active [`DicePlugin::render_layer`] for spawn-time use.
#[derive(Resource, Clone, Copy)]
pub struct DiceRenderLayer {
    pub layer: u8,
}

impl Plugin for DicePlugin {
    fn build(&self, app: &mut App) {
        register_embedded_dicesets(app);

        let render_layer = self.render_layer;
        app.init_resource::<DiceSimulationGravity>();
        if let Some(gravity) = self.gravity {
            app.insert_resource(DiceSimulationGravity { acceleration: Vec3::new(0.0, gravity, 0.0) });
        }
        app.init_resource::<PrecomputeCache>()
            .init_resource::<NextRollId>()
            .init_resource::<DiceRng>()
            .init_resource::<DiceSimulationRng>()
            .insert_resource(DiceRenderLayer { layer: render_layer })
            .add_message::<RollRequest>()
            .add_message::<RollComplete>()
            .add_message::<PrecomputeRequest>()
            .add_message::<RollFailed>()
            .register_type::<DiceArena>()
            .register_type::<DefaultArena>()
            .register_type::<DiceSimulationGravity>()
            .register_type::<DicePhysicsConfig>()
            .register_type::<SpawnConfig>()
            .register_type::<Diceset>()
            .register_type::<SpawnedDie>()
            .register_type::<DieKind>()
            .register_type::<Options>()
            .register_type::<DiceTerm>()
            .register_type::<DiceRoll>()
            .register_type::<RolledDie>()
            .register_type::<RollOutcome>()
            .register_type::<RollRequest>()
            .register_type::<RollComplete>()
            .add_systems(
                Update,
                (
                    spawn_arena_lights,
                    (load_diceset_handles, queue_cached_rolls, update_precompute_cache).chain(),
                    despawn_orphaned_dice,
                ),
            );

        #[cfg(any(feature = "plain_white", feature = "halloween", feature = "metal", feature = "clear_orange"))]
        if self.spawn_default_arena {
            app.add_systems(Startup, spawn_default_arena_system);
        }
    }
}

/// Registers every enabled diceset feature with Bevy's embedded asset source.
fn register_embedded_dicesets(app: &mut App) {
    let table = embedded_table();
    if table.is_empty() {
        return;
    }
    use bevy::asset::io::embedded::EmbeddedAssetRegistry;
    let embedded = app.world().get_resource::<EmbeddedAssetRegistry>().expect(
        "DicePlugin requires bevy::asset::AssetPlugin (provided by DefaultPlugins). \
             Add DefaultPlugins before DicePlugin.",
    );
    for (name, bytes) in table {
        let asset_path = format!("bevy_dice/{name}_diceset.glb");
        embedded.insert_asset(std::path::PathBuf::new(), std::path::Path::new(&asset_path), *bytes);
    }
}

/// Auto-spawns a [`Diceset`] + [`DiceArena`] for the first entry in [`embedded_table`].
#[cfg(any(feature = "plain_white", feature = "halloween", feature = "metal", feature = "clear_orange"))]
fn spawn_default_arena_system(mut commands: Commands) {
    if let Some((name, _)) = embedded_table().first() {
        let diceset = commands.spawn(Diceset::embedded(name)).id();
        commands.spawn((DiceArena::default().diceset(diceset), DefaultArena));
    }
}

#[cfg(test)]
mod tests {
    use avian3d::prelude::{Collider, Gravity, PhysicsSchedulePlugin, RigidBody};
    use bevy::asset::AssetPlugin;

    use super::*;

    fn app_with_assets() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app
    }

    #[test]
    fn presentation_does_not_install_host_physics() {
        let mut app = app_with_assets();
        app.add_plugins(DicePlugin { spawn_default_arena: false, ..default() });
        assert!(!app.is_plugin_added::<PhysicsSchedulePlugin>());
        assert!(!app.world().contains_resource::<Gravity>());
        assert_eq!(app.world().resource::<DiceSimulationGravity>().acceleration, Vec3::new(0.0, -23.1, 0.0));
    }

    #[test]
    fn simulation_gravity_is_independent_of_host_gravity() {
        let mut app = app_with_assets();
        let host_gravity = Vec3::new(1.0, -9.81, 2.0);
        app.insert_resource(Gravity(host_gravity));
        app.add_plugins(DicePlugin { gravity: Some(-15.0), spawn_default_arena: false, ..default() });
        assert_eq!(app.world().resource::<Gravity>().0, host_gravity);
        assert_eq!(app.world().resource::<DiceSimulationGravity>().acceleration, Vec3::new(0.0, -15.0, 0.0));
        assert!(!app.is_plugin_added::<PhysicsSchedulePlugin>());
    }

    #[test]
    fn arenas_spawn_lighting_without_host_colliders() {
        let mut world = World::new();
        world.insert_resource(DiceRenderLayer { layer: 0 });
        let arena = world.spawn(DiceArena::default()).id();
        let mut schedule = Schedule::default();
        schedule.add_systems(spawn_arena_lights);
        schedule.run(&mut world);
        assert_eq!(world.query::<&DirectionalLight>().iter(&world).count(), 1);
        assert_eq!(world.query::<&Collider>().iter(&world).count(), 0);
        assert_eq!(world.query::<&RigidBody>().iter(&world).count(), 0);
        assert!(world.get::<Transform>(arena).is_some());
    }
}
