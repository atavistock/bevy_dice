//! [`DicePlugin`]: avian + roll messages + pipeline systems, optionally
//! auto-spawning a default arena for the enabled embedded diceset.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::dice::{DiceRoll, DiceTerm, DieKind, Options, RollOutcome, RolledDie};

use super::arena::{DefaultArena, DiceArena, DiceBoxWall, DicePhysicsConfig, SpawnConfig, spawn_arena_walls};
use super::diceset::{Diceset, embedded_table, load_diceset_handles};
use super::pending::{PendingRolls, handle_roll_requests};
use super::resolve::{despawn_orphaned_dice, prune_orphaned_pending_rolls, resolve_pending_rolls};
use super::rng::DiceRng;
use super::roller::{NextRollId, RollComplete, RollRequest};
use super::spawn::SpawnedDie;

/// Adds avian3d physics (unless the host already did), the roll request/complete messages, the
/// tumble/settle/emit pipeline, and an overhead [`DirectionalLight`] for the
/// dice. With an embedded diceset feature enabled, auto-spawns a [`DiceArena`]
/// tagged [`DefaultArena`] unless
/// [`spawn_default_arena`](Self::spawn_default_arena) is `false`.
///
/// Dice render on layer `0` (the default Bevy layer) by default, so any
/// camera that renders your scene also renders the dice with no extra setup.
/// Set [`render_layer`](Self::render_layer) to a non-zero layer to isolate
/// dice (the camera then needs `RenderLayers::from_layers(&[0, N])`).
pub struct DicePlugin {
    /// World-space gravity along -Y in m/s^2. `None` uses `-23.1` (tabletop feel)
    /// when this plugin adds avian, and leaves the host's `Gravity` alone otherwise.
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

/// Gravity applied when this plugin owns the avian install; feels like a tabletop.
const DEFAULT_GRAVITY: f32 = -23.1;

/// Resource holding the active [`DicePlugin::render_layer`] for spawn-time use.
#[derive(Resource, Clone, Copy)]
pub(super) struct DiceRenderLayer {
    pub layer: u8,
}

impl Plugin for DicePlugin {
    fn build(&self, app: &mut App) {
        register_embedded_dicesets(app);

        let render_layer = self.render_layer;
        let host_has_physics = app.is_plugin_added::<PhysicsSchedulePlugin>();
        if !host_has_physics {
            app.add_plugins(PhysicsPlugins::default());
        }
        let gravity = self.gravity.or((!host_has_physics).then_some(DEFAULT_GRAVITY));
        if let Some(gravity) = gravity {
            app.insert_resource(Gravity(Vec3::new(0.0, gravity, 0.0)));
        }
        app.init_resource::<PendingRolls>()
            .init_resource::<NextRollId>()
            .init_resource::<DiceRng>()
            .insert_resource(DiceRenderLayer { layer: render_layer })
            .add_message::<RollRequest>()
            .add_message::<RollComplete>()
            .register_type::<DiceArena>()
            .register_type::<DefaultArena>()
            .register_type::<DiceBoxWall>()
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
                    spawn_arena_walls,
                    (load_diceset_handles, handle_roll_requests).chain(),
                    resolve_pending_rolls,
                    despawn_orphaned_dice,
                    prune_orphaned_pending_rolls,
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
