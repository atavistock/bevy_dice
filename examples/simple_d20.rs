//! `cargo run --example simple_d20 --features plain_white`

use bevy::prelude::*;
use bevy_dice::ui::{DiceRoller, DicePlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, DicePlugin::default()))
        .add_systems(Startup, setup_scene)
        .add_systems(PostStartup, roll_once)
        .run();
}

fn roll_once(mut roller: DiceRoller) {
    let _ = roller.roll_expr("1d20");
}

fn setup_scene(mut commands: Commands) {
    // Default render_layer is 0, so a plain camera renders the dice too.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
