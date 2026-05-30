//! Same setup as `simple_d20`, but the arena uses render layer 1 so the
//! plugin's overhead light and the dice are isolated from the host scene's
//! lighting. The camera must include layer 1 to see them.
//!
//! `cargo run --example isolated_layer --features plain_white`

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_dice::dice::DiceRoll;
use bevy_dice::ui::{DefaultArena, DiceArena, DicePlugin, DiceRoller, Diceset};

const DICE_LAYER: u8 = 1;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            // Tell the plugin not to auto-spawn its default arena; we'll
            // spawn our own so we can pin it to a non-default render layer.
            DicePlugin { spawn_default_arena: false, ..default() },
        ))
        .add_systems(Startup, setup_scene)
        .add_systems(PostStartup, roll_once)
        .run();
}

fn setup_scene(mut commands: Commands) {
    // Camera explicitly renders both layers: the scene (0) and the dice (1).
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::from_layers(&[0, DICE_LAYER as usize]),
    ));

    // Spawn the diceset entity once; arenas reference it by Entity id.
    let diceset = commands.spawn(Diceset::embedded("plain_white")).id();

    // Arena pinned to DICE_LAYER. Its overhead light spawns on the same layer.
    commands.spawn((
        DiceArena::default().diceset(diceset).render_layer(DICE_LAYER),
        DefaultArena,
    ));
}

fn roll_once(mut roller: DiceRoller) {
    let _ = roller.roll(DiceRoll::parse("1d20").unwrap());
}
