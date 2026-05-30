//! Keyboard-driven dice tester. Type a dice expression (e.g. `3d6+2`), press
//! Enter to roll, Backspace to delete.
//!
//! `cargo run --example tester --features plain_white`

use bevy::camera::visibility::RenderLayers;
use bevy::camera::ScalingMode;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy_dice::dice::{DiceRoll, RollOutcome};
use bevy_dice::ui::{DicePlugin, DiceRoller, RollComplete, SpawnedDie};

#[derive(Component)]
struct InputDisplay;

#[derive(Component)]
struct ResultDisplay;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, DicePlugin::default()))
        .add_systems(Startup, (setup_scene, setup_ui))
        .add_systems(Update, (handle_keys, show_result))
        .run();
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 10.0 },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::ZERO, Vec3::Z),
        RenderLayers::from_layers(&[0, 1]),
    ));
}

fn setup_ui(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            left: Val::Px(16.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|root| {
            root.spawn((InputDisplay, Text::new("> ")));
            root.spawn((ResultDisplay, Text::new("")));
        });
}

fn handle_keys(
    mut events: MessageReader<KeyboardInput>,
    mut input_text: Single<&mut Text, With<InputDisplay>>,
    mut result_text: Single<&mut Text, (With<ResultDisplay>, Without<InputDisplay>)>,
    old_dice: Query<Entity, With<SpawnedDie>>,
    mut roller: DiceRoller,
    mut commands: Commands,
) {
    for event in events.read().filter(|e| e.state.is_pressed()) {
        let expression = input_text.0.strip_prefix("> ").unwrap_or("").to_string();
        match &event.logical_key {
            Key::Enter => {
                for entity in old_dice.iter() {
                    commands.entity(entity).despawn();
                }
                match DiceRoll::parse(expression.trim()) {
                    Ok(roll) => {
                        ***result_text = "Rolling...".into();
                        let _ = roller.roll(roll);
                    }
                    Err(err) => ***result_text = format!("Error: {err}"),
                }
            }
            Key::Backspace => {
                let mut next = expression;
                next.pop();
                ***input_text = format!("> {next}");
            }
            Key::Character(string) => ***input_text = format!("> {expression}{string}"),
            _ => {}
        }
    }
}

fn show_result(
    mut events: MessageReader<RollComplete>,
    mut text: Single<&mut Text, With<ResultDisplay>>,
) {
    for event in events.read() {
        ***text = format!("Total: {}\n{}", event.outcome.total, breakdown(&event.roll, &event.outcome));
    }
}

fn breakdown(roll: &DiceRoll, outcome: &RollOutcome) -> String {
    let mut parts = Vec::new();
    for (term_index, term) in roll.terms.iter().enumerate() {
        let values: Vec<_> = outcome.term_dice(term_index).iter().map(|d| d.value.to_string()).collect();
        parts.push(format!("{term}({})", values.join(",")));
    }
    if roll.adjustment != 0 {
        parts.push(format!("{:+}", roll.adjustment));
    }
    parts.join(" ")
}
