//! Two side-by-side arenas with different dicesets.
//!
//! - Left arena uses `plain_white`; right arena uses `halloween`.
//! - Each arena gets its own d20 thrown at startup.
//! - Press any key to despawn the dice and re-roll both.
//! - Once the dice settle, the result for each arena is displayed in big
//!   text centered above that arena's half of the screen.
//!
//! Run: `cargo run --example two_arenas --features "plain_white halloween"`

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy_dice::ui::{
    DiceArena, DicePlugin, DiceRoller, Diceset, RollComplete, SpawnConfig, SpawnedDie,
};

#[path = "common/mod.rs"]
mod common;
use common::{any_key_pressed, draw_arena_borders, top_down_camera};

// === boilerplate ===

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            // The plugin would otherwise auto-spawn a default arena for the
            // first enabled embedded diceset. We're spawning two custom
            // arenas ourselves, so opt out.
            DicePlugin { spawn_default_arena: false, ..default() },
        ))
        // Scene + arenas in Startup; UI + initial roll in PostStartup so
        // the Arenas resource is visible (Commands from Startup have
        // applied by then).
        .add_systems(Startup, setup_scene)
        .add_systems(PostStartup, (setup_ui, roll_in_both))
        .add_systems(
            Update,
            (reroll_on_key, update_result_text, draw_arena_borders),
        )
        .run();
}

// === example ===

/// Holds the entity ids of the two arenas so other systems can target them
/// (e.g. to fire a roll into a specific arena, or to update the matching
/// text widget).
#[derive(Resource)]
struct Arenas {
    left: Entity,
    right: Entity,
}

/// Tag on a UI `Text` so [`update_result_text`] knows which arena the
/// text widget belongs to (`ArenaLabel(arenas.left)` -> the left-side text).
#[derive(Component)]
struct ArenaLabel(Entity);

/// Spawns the camera, two `Diceset` entities (one per visual style), and
/// two `DiceArena`s positioned left and right of world origin. Inserts the
/// [`Arenas`] resource so later systems can target each arena by name.
fn setup_scene(mut commands: Commands) {
    // Pulled-back top-down camera so both arenas fit in frame.
    commands.spawn(top_down_camera(24.0));

    // One Diceset entity per visual style. Each arena references its own;
    // both share the default render layer (0), so the camera renders both.
    // (DicePlugin auto-spawns one angled overhead DirectionalLight per
    // arena to make the dice faces read clearly from above.)
    let plain_white = commands.spawn(Diceset::embedded("plain_white")).id();
    let halloween = commands.spawn(Diceset::embedded("halloween")).id();

    // Bigger arenas + a punchier throw so the dice cover the whole floor
    // instead of clustering near the entry wall.
    let throw = SpawnConfig {
        speed: 16.0,
        angular_speed_factor: 1.2,
        ..default()
    };

    let left = commands
        .spawn(
            DiceArena::default()
                .name("left")
                .center(-7.0, 0.0, 0.0)
                .size(10.0, 4.0, 8.0)
                .diceset(plain_white)
                .spawn_config(throw.clone()),
        )
        .id();
    let right = commands
        .spawn(
            DiceArena::default()
                .name("right")
                .center(7.0, 0.0, 0.0)
                .size(10.0, 4.0, 8.0)
                .diceset(halloween)
                .spawn_config(throw),
        )
        .id();

    commands.insert_resource(Arenas { left, right });
}

/// Spawns one text widget per arena, each pinned to the top of its half of
/// the screen. The text starts as "Rolling..." and is replaced by
/// [`update_result_text`] when a roll settles.
fn setup_ui(mut commands: Commands, arenas: Res<Arenas>) {
    spawn_arena_text(&mut commands, arenas.left, Val::Px(0.0));
    spawn_arena_text(&mut commands, arenas.right, Val::Percent(50.0));
}

/// Spawns a 50%-wide row at the top of the screen with one big centered
/// `Text` inside, tagged with `ArenaLabel(arena)` so updates can find it.
fn spawn_arena_text(commands: &mut Commands, arena: Entity, left: Val) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(20.0),
            left,
            width: Val::Percent(50.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                Text::new("Rolling..."),
                TextFont { font_size: 64.0, ..default() },
                TextColor(Color::WHITE),
                ArenaLabel(arena),
            ));
        });
}

/// Fires one d20 into each arena. Called at startup and again after every
/// keypress (see [`reroll_on_key`]).
fn roll_in_both(mut roller: DiceRoller, arenas: Res<Arenas>) {
    // `roll_expr_in` submits the parsed expression to a specific arena
    // entity instead of the (nonexistent) DefaultArena.
    let _ = roller.roll_expr_in(arenas.left, "1d20");
    let _ = roller.roll_expr_in(arenas.right, "1d20");
}

/// On any key press, clears the existing dice and triggers two fresh rolls.
/// We despawn every `SpawnedDie` rather than waiting for the old roll to
/// resolve, so the user gets instant feedback that something is happening.
fn reroll_on_key(
    mut events: MessageReader<KeyboardInput>,
    dice: Query<Entity, With<SpawnedDie>>,
    mut texts: Query<&mut Text, With<ArenaLabel>>,
    mut roller: DiceRoller,
    arenas: Res<Arenas>,
    mut commands: Commands,
) {
    if !any_key_pressed(&mut events) {
        return;
    }

    // Despawn every die in the world. The plugin's orphan-cleanup system
    // also prunes the matching PendingRolls if any were in flight.
    for entity in dice.iter() {
        commands.entity(entity).despawn();
    }

    // Reset both text widgets to the "Rolling..." state.
    for mut text in texts.iter_mut() {
        text.0 = "Rolling...".to_string();
    }

    // Fire the new rolls. RollComplete will arrive once the dice settle.
    let _ = roller.roll_expr_in(arenas.left, "1d20");
    let _ = roller.roll_expr_in(arenas.right, "1d20");
}

/// Listens for `RollComplete` messages and updates the matching arena's
/// text widget. The arena entity carried on the message tells us which
/// side to update (via `ArenaLabel`).
fn update_result_text(
    mut completed: MessageReader<RollComplete>,
    mut labels: Query<(&ArenaLabel, &mut Text)>,
) {
    for event in completed.read() {
        for (label, mut text) in labels.iter_mut() {
            if label.0 == event.arena {
                text.0 = format!("{}", event.outcome.total);
            }
        }
    }
}
