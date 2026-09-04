//! Same one-die-rolled setup as `simple_d20`, but the arena is pinned to
//! render layer 1 so its dice and its overhead light are isolated from the
//! host scene's lighting. The camera explicitly lists both layers so it can
//! see your scene (layer 0) and the dice (layer 1) at the same time.
//!
//! Useful when:
//! - Your game already has its own lights on layer 0 that would over-light the dice.
//! - You want a HUD-style camera that only renders the dice.
//! - Split-screen: each player's arena draws on a different layer.
//!
//! Run: `cargo run --example isolated_layer --features plain_white`

use bevy::prelude::*;
use bevy_dice::ui::{DefaultArena, DiceArena, DicePlugin, DiceRoller, Diceset};

#[path = "common/mod.rs"]
mod common;
use common::top_down_camera_on_layers;

// === boilerplate ===

/// Non-zero render layer the dice (and their light) will live on. Must
/// match the layer included on the camera's `RenderLayers` component.
const DICE_LAYER: u8 = 1;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            // Disable the auto-spawned default arena: it would land on the
            // plugin's default layer (0), and we want everything on DICE_LAYER.
            DicePlugin { spawn_default_arena: false, ..default() },
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(PostStartup, roll_once)
        .run();
}

// === example ===

fn setup_scene(mut commands: Commands) {
    // Camera renders both layer 0 (whatever else might be in the scene) and
    // DICE_LAYER (the arena). Drop layer 0 for a dice-only HUD camera.
    commands.spawn(top_down_camera_on_layers(14.0, &[DICE_LAYER]));

    // Spawn the Diceset entity once and remember its id; the arena will
    // reference it by Entity, so many arenas could share one Diceset.
    let diceset = commands.spawn(Diceset::embedded("plain_white")).id();

    // The arena pins itself to DICE_LAYER. When `spawn_arena_walls` runs,
    // it also spawns a top-down DirectionalLight on this layer so the dice
    // are lit no matter what the host scene's lighting looks like.
    commands.spawn((DiceArena::default().diceset(diceset).render_layer(DICE_LAYER), DefaultArena));
}

/// Roll a single d20 into the default arena.
fn roll_once(mut roller: DiceRoller) {
    let _ = roller.roll_expr("1d20");
}
