//! The smallest possible bevy_dice setup: add the plugin, spawn a camera,
//! roll once. With the `plain_white` feature enabled, `DicePlugin` auto-
//! spawns both a `Diceset` entity and a `DiceArena` tagged as the default,
//! so `roller.roll_expr(...)` has somewhere to throw the dice.
//!
//! Run: `cargo run --example simple_d20 --features plain_white`

use bevy::prelude::*;
use bevy_dice::ui::{DicePlugin, DiceRoller};

#[path = "common/mod.rs"]
mod common;
use common::top_down_camera;

// === boilerplate ===

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, DicePlugin::default()))
        .add_systems(Startup, setup_scene)
        .add_systems(PostStartup, roll_once)
        .run();
}

fn setup_scene(mut commands: Commands) {
    commands.spawn(top_down_camera(14.0));
}

// === example ===

/// Fires one d20 into the default arena. `roll_expr` parses the expression
/// and submits it in a single call; the returned `Result` carries the
/// roll id (we ignore it) or a `RollError` if parsing or arena lookup fails.
fn roll_once(mut roller: DiceRoller) {
    let _ = roller.roll_expr("1d20");
}
