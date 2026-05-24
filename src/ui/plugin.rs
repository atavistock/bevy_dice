//! Top-level wiring: [`DicePlugin`] adds avian, the roll messages, the
//! pipeline systems, and (optionally) auto-spawns a default arena using
//! whatever embedded diceset feature was selected at compile time.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::arena::spawn_arena_walls;
#[cfg(any(
    feature = "plain_white",
    feature = "halloween",
    feature = "metal",
    feature = "clear_orange"
))]
use super::arena::{DefaultArena, DiceArena};
use super::pending::{handle_roll_requests, preload_dice_assets, DiceAssetHandles, PendingRolls};
use super::resolve::resolve_pending_rolls;
use super::roller::{NextRollId, RollComplete, RollRequest};

/// Adds avian3d physics, the avian picking backend, the roll request/complete
/// messages, and the systems that drive dice through their tumble + settle
/// + result-emit lifecycle.
///
/// When any embedded diceset feature is enabled (`plain_white`, `halloween`,
/// `metal`, `clear_orange`) and [`DicePlugin::spawn_default_arena`] is `true`
/// (the default), a [`DiceArena`] is spawned at startup and tagged with
/// [`DefaultArena`] so [`super::DiceRoller::roll`] works with zero caller
/// setup. Disable by setting `spawn_default_arena: false` if you want to
/// spawn your own arenas (custom diceset, custom box geometry, multiple
/// arenas).
///
/// # Examples
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_dice::ui::DicePlugin;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(DicePlugin::default())
///     .run();
/// ```
pub struct DicePlugin {
    /// World-space gravity along -Y in m/s^2. Default `-23.1` produces a
    /// snappy tabletop feel; lower it for slower, floatier dice.
    pub gravity: f32,
    /// If true and an embedded diceset feature is enabled, a [`DiceArena`]
    /// tagged with [`DefaultArena`] is spawned at startup. Set false when
    /// providing your own arena entities.
    pub spawn_default_arena: bool,
}

impl Default for DicePlugin {
    fn default() -> Self {
        Self {
            gravity: -23.1,
            spawn_default_arena: true,
        }
    }
}

impl Plugin for DicePlugin {
    fn build(&self, app: &mut App) {
        register_embedded_dicesets(app);

        let gravity = self.gravity;
        app.add_plugins((PhysicsPlugins::default(), PhysicsPickingPlugin))
            .init_resource::<PendingRolls>()
            .init_resource::<DiceAssetHandles>()
            .init_resource::<NextRollId>()
            .add_message::<RollRequest>()
            .add_message::<RollComplete>()
            .insert_resource(Gravity(Vec3::new(0.0, gravity, 0.0)))
            .add_systems(
                Update,
                (
                    spawn_arena_walls,
                    (preload_dice_assets, handle_roll_requests).chain(),
                    resolve_pending_rolls,
                ),
            );

        #[cfg(any(
            feature = "plain_white",
            feature = "halloween",
            feature = "metal",
            feature = "clear_orange"
        ))]
        if self.spawn_default_arena {
            app.add_systems(Startup, spawn_default_arena_system);
        }
    }
}

/// Registers each enabled diceset feature's gltf bytes with Bevy's embedded
/// asset source. URLs become `embedded://bevy_dice/<slug>_diceset.glb`.
fn register_embedded_dicesets(app: &mut App) {
    #[cfg(any(
        feature = "plain_white",
        feature = "halloween",
        feature = "metal",
        feature = "clear_orange"
    ))]
    {
        use bevy::asset::io::embedded::EmbeddedAssetRegistry;
        use std::path::{Path, PathBuf};

        let embedded = app
            .world()
            .get_resource::<EmbeddedAssetRegistry>()
            .expect(
                "DicePlugin requires bevy::asset::AssetPlugin (provided by DefaultPlugins). \
                 Add DefaultPlugins before DicePlugin.",
            );

        #[cfg(feature = "plain_white")]
        embedded.insert_asset(
            PathBuf::new(),
            Path::new("bevy_dice/plain_white_diceset.glb"),
            include_bytes!("../../assets/plain_white_diceset.glb").as_slice(),
        );
        #[cfg(feature = "halloween")]
        embedded.insert_asset(
            PathBuf::new(),
            Path::new("bevy_dice/halloween_diceset.glb"),
            include_bytes!("../../assets/halloween_diceset.glb").as_slice(),
        );
        #[cfg(feature = "metal")]
        embedded.insert_asset(
            PathBuf::new(),
            Path::new("bevy_dice/metal_diceset.glb"),
            include_bytes!("../../assets/metal_diceset.glb").as_slice(),
        );
        #[cfg(feature = "clear_orange")]
        embedded.insert_asset(
            PathBuf::new(),
            Path::new("bevy_dice/clear_orange_diceset.glb"),
            include_bytes!("../../assets/clear_orange_diceset.glb").as_slice(),
        );
    }
    let _ = app;
}

/// Auto-spawns a [`DiceArena`] using the highest-priority embedded diceset
/// (plain_white > halloween > metal > clear_orange) so the zero-config case
/// of "add the plugin and roll" works. Only compiled in when at least one
/// embedded diceset feature is enabled.
#[cfg(feature = "plain_white")]
fn spawn_default_arena_system(mut commands: Commands) {
    commands.spawn((DiceArena::plain_white(), DefaultArena));
}

#[cfg(all(not(feature = "plain_white"), feature = "halloween"))]
fn spawn_default_arena_system(mut commands: Commands) {
    commands.spawn((DiceArena::halloween(), DefaultArena));
}

#[cfg(all(not(feature = "plain_white"), not(feature = "halloween"), feature = "metal"))]
fn spawn_default_arena_system(mut commands: Commands) {
    commands.spawn((DiceArena::metal(), DefaultArena));
}

#[cfg(all(
    not(feature = "plain_white"),
    not(feature = "halloween"),
    not(feature = "metal"),
    feature = "clear_orange"
))]
fn spawn_default_arena_system(mut commands: Commands) {
    commands.spawn((DiceArena::clear_orange(), DefaultArena));
}
