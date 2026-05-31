//! Boilerplate shared by every example. Each example pulls this in with
//!
//! ```ignore
//! #[path = "common/mod.rs"]
//! mod common;
//! use common::*;
//! ```
//!
//! Anything that genuinely repeats across examples lives here so the
//! example-specific code can stand on its own.

// Each example compiles this module independently and may use only a
// subset of the helpers; silence the resulting dead-code warnings.
#![allow(dead_code)]

use bevy::camera::visibility::RenderLayers;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy_dice::ui::DiceArena;

// === Cameras ===

/// Bundle for a perspective camera that looks exactly straight down at the
/// world origin from `height` units up. The up-vector is `Vec3::NEG_Z` so
/// that world +X maps to screen-right and world +Z maps to screen-down,
/// matching the usual top-down game convention.
pub fn top_down_camera(height: f32) -> impl Bundle {
    (
        Camera3d::default(),
        Transform::from_xyz(0.0, height, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
    )
}

/// Same as [`top_down_camera`] but the camera renders the scene's default
/// layer (`0`) plus every layer in `extra_layers`. Use when an arena pins
/// its dice to a non-zero render layer; the camera has to opt in to that
/// layer or the dice won't appear.
pub fn top_down_camera_on_layers(height: f32, extra_layers: &[u8]) -> impl Bundle {
    let mut layers = vec![0usize];
    layers.extend(extra_layers.iter().map(|&n| n as usize));
    (
        Camera3d::default(),
        Transform::from_xyz(0.0, height, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
        RenderLayers::from_layers(&layers),
    )
}

// === Visual helpers ===

/// Draws a flat rectangle outline around every [`DiceArena`]'s floor
/// every frame using Bevy gizmos. Add to `Update` to make the arena
/// footprint visible from a top-down camera (the walls themselves are
/// invisible static colliders, and a 3D cube wireframe just clutters the
/// top-down view).
pub fn draw_arena_borders(mut gizmos: Gizmos, arenas: Query<&DiceArena>) {
    for arena in arenas.iter() {
        let center = arena.center;
        let half = arena.size * 0.5;
        let y = center.y;
        gizmos.linestrip(
            [
                Vec3::new(center.x - half.x, y, center.z - half.z),
                Vec3::new(center.x + half.x, y, center.z - half.z),
                Vec3::new(center.x + half.x, y, center.z + half.z),
                Vec3::new(center.x - half.x, y, center.z + half.z),
                Vec3::new(center.x - half.x, y, center.z - half.z),
            ],
            Color::WHITE,
        );
    }
}

// === Keyboard helpers ===

/// True when at least one key was pressed (not released) this frame. Use
/// inside an `Update` system for "any key" triggers.
pub fn any_key_pressed(events: &mut MessageReader<KeyboardInput>) -> bool {
    events.read().any(|e| e.state.is_pressed())
}

/// Attach to a UI [`Text`] entity to turn it into a single-line input
/// field driven by [`drive_text_input`]. The text always renders as
/// `"{prompt}{current}"`; pressing Enter publishes a
/// [`TextInputSubmitted`] message with `current` (trimmed).
#[derive(Component)]
pub struct TextInput {
    pub prompt: String,
}

/// Emitted by [`drive_text_input`] whenever the user presses Enter on a
/// [`TextInput`] field.
#[derive(Message)]
pub struct TextInputSubmitted {
    /// Which `TextInput` entity submitted (lets multiple inputs coexist).
    pub entity: Entity,
    /// The user's text, with leading/trailing whitespace removed.
    pub value: String,
}

/// System that drives [`TextInput`] widgets: Enter submits + clears,
/// Backspace deletes one character, printable characters append. Add it
/// to `Update` and register the message type:
///
/// ```ignore
/// app.add_message::<common::TextInputSubmitted>()
///    .add_systems(Update, common::drive_text_input);
/// ```
pub fn drive_text_input(
    mut events: MessageReader<KeyboardInput>,
    mut inputs: Query<(Entity, &TextInput, &mut Text)>,
    mut submitted: MessageWriter<TextInputSubmitted>,
) {
    for event in events.read().filter(|e| e.state.is_pressed()) {
        for (entity, input, mut text) in inputs.iter_mut() {
            let current = text.0.strip_prefix(&input.prompt).unwrap_or("").to_string();
            match &event.logical_key {
                Key::Enter => {
                    submitted.write(TextInputSubmitted {
                        entity,
                        value: current.trim().to_string(),
                    });
                    text.0 = input.prompt.clone();
                }
                Key::Backspace => {
                    let mut next = current;
                    next.pop();
                    text.0 = format!("{}{}", input.prompt, next);
                }
                Key::Character(string) => {
                    text.0 = format!("{}{}{}", input.prompt, current, string);
                }
                _ => {}
            }
        }
    }
}
