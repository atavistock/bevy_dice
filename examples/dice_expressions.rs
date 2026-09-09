//! Interactive dice-expression tester. Type an expression at the prompt
//! (e.g. `3d6+2`, `1d20-1d4`, `2d20kh1+5`), press Enter to roll, Backspace
//! to delete. The dice tumble in the auto-spawned default arena and the
//! result + per-die breakdown appears in the result line.
//!
//! What this example actually demonstrates: how to wire `roller.roll_expr`
//! to user-supplied text and how `outcome.display(&roll)` produces the
//! human-readable breakdown. Text-input mechanics (Enter/Backspace/typing)
//! live in `common::TextInput` so they don't clutter the example.
//!
//! Run: `cargo run --example dice_expressions --features plain_white`

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy_dice::render::{DicePlugin, DiceRoller, RollComplete, RollError, RollFailed, SpawnedDie};

#[path = "common/mod.rs"]
mod common;
use common::{TextInput, TextInputSubmitted, drive_text_input};

// === boilerplate ===

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, DicePlugin::default()))
        .add_message::<TextInputSubmitted>()
        .init_resource::<LatestRoll>()
        .add_systems(Startup, (setup_scene, setup_ui))
        .add_systems(Update, (drive_text_input, on_expression_submitted, show_result))
        .add_plugins(common::SmokeTestPlugin)
        .run();
}

/// Top-down orthographic camera. We use orthographic + `FixedVertical` so
/// the arena reads the same regardless of window aspect ratio; the shared
/// `top_down_camera` helper is perspective, so we hand-roll the camera here.
fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 10.0 },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
    ));
}

// === example ===

/// Marker on the text node that shows the most recent roll result.
#[derive(Component)]
struct ResultDisplay;

#[derive(Resource, Default)]
struct LatestRoll {
    roll_id: Option<u64>,
}

/// Two stacked text widgets in the top-left corner: the live input line
/// (driven by [`TextInput`]) and the most recent result line.
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
            // The TextInput component turns this Text into an editable
            // input field; common::drive_text_input handles keystrokes.
            root.spawn((Text::new("> "), TextInput { prompt: "> ".to_string() }));
            root.spawn((ResultDisplay, Text::new("")));
        });
}

/// When the user presses Enter on the input field, despawn any leftover
/// dice and submit the typed expression. `roll_expr` returns
/// `Result<u64, RollError>` covering both parse failures and missing-arena
/// errors in one type.
fn on_expression_submitted(
    mut submitted: MessageReader<TextInputSubmitted>,
    mut result_text: Single<&mut Text, With<ResultDisplay>>,
    old_dice: Query<Entity, With<SpawnedDie>>,
    mut roller: DiceRoller,
    mut commands: Commands,
    mut latest: ResMut<LatestRoll>,
) {
    for event in submitted.read() {
        latest.roll_id = None;
        for entity in old_dice.iter() {
            commands.entity(entity).despawn();
        }
        match roller.roll_expr(&event.value) {
            Ok(roll_id) => {
                latest.roll_id = Some(roll_id);
                ***result_text = "Rolling...".into();
            }
            Err(RollError::Parse(err)) => ***result_text = format!("Parse error: {err}"),
            Err(RollError::Arena(err)) => ***result_text = format!("Arena error: {err:?}"),
            Err(RollError::Outcome(err)) => ***result_text = format!("Outcome error: {err}"),
        }
    }
}

/// When dice settle, the plugin emits a `RollComplete`; we format it via
/// `outcome.display(&roll)` which already prints a clean
/// `"3d6(4,2,1) + 2 = 9"`-style breakdown.
fn show_result(
    mut events: MessageReader<RollComplete>,
    mut failed: MessageReader<RollFailed>,
    latest: Res<LatestRoll>,
    mut text: Single<&mut Text, With<ResultDisplay>>,
) {
    for event in events.read() {
        if latest.roll_id == Some(event.roll_id) {
            ***text = event.outcome.display(&event.roll).to_string();
        }
    }
    for event in failed.read() {
        if latest.roll_id == Some(event.roll_id) {
            ***text = format!("Roll error: {}", event.reason);
        }
    }
}
