//! Interactive tester: type a dice expression, click Roll, see the dice and total.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy_dice::dice::{DiceRoll, RollOutcome};
use bevy_dice::ui::{
    load_orientations, DefaultArena, DiceArena, DicePlugin, DiceRoller, RollComplete, SpawnedDie,
};
use bevy_material_ui::button::{spawn_material_button, ButtonClickEvent, ButtonVariant};
use bevy_material_ui::prelude::*;
use bevy_material_ui::select::{SelectBuilder, SelectChangeEvent, SelectOption, SpawnSelectChild};
use bevy_material_ui::text_field::{
    spawn_text_field_control_with, MaterialTextField, TextFieldBuilder, TextFieldSubmitEvent,
};

/// (display label, asset-folder slug). First entry is the startup default.
const DICESETS: &[(&str, &str)] = &[
    ("Plain White", "plain_white_diceset"),
    ("Frosty", "frosty_diceset"),
    ("Clear Orange", "clear_orange_diceset"),
    ("Fiery", "fiery_diceset"),
    ("Halloween", "halloween_diceset"),
    ("Metal", "metal_diceset"),
];

const CAMERA_POS: Vec3 = Vec3::new(0.0, 12.0, 0.0);

#[derive(Component)]
struct DiceInput;

#[derive(Component)]
struct RollButton;

#[derive(Component)]
struct TotalDisplay;

#[derive(Component)]
struct BreakdownDisplay;

#[derive(Message)]
struct RollRequested;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(MaterialUiPlugin)
        .add_plugins(DicePlugin {
            spawn_default_arena: false,
            ..default()
        })
        .add_message::<RollRequested>()
        .add_systems(Startup, (setup_arena, setup_scene, setup_ui))
        .add_systems(
            Update,
            (
                button_click_to_roll,
                text_submit_to_roll,
                handle_diceset_change,
                process_roll,
                update_display_on_complete,
            ),
        )
        .run();
}

fn setup_arena(mut commands: Commands) {
    let (_, slug) = DICESETS[0];
    // Box at ~76% of the visible area from the orthographic top-down camera.
    let arena = DiceArena::default()
        .name("tester")
        .center(0.0, 0.0, 0.0)
        .size(13.4, 5.0, 7.5)
        .diceset(slug)
        .orientations(load_orientations(asset_full_path(slug)));
    commands.spawn((arena, DefaultArena));
}

fn asset_full_path(slug: &str) -> String {
    format!("{}/assets/{slug}.glb", env!("CARGO_MANIFEST_DIR"))
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        // Orthographic top-down: each die's rolled face reads square-on
        // regardless of its position in the box; world +Z becomes screen up.
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical { viewport_height: 10.0 },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_translation(CAMERA_POS).looking_at(Vec3::ZERO, Vec3::Z),
        AmbientLight {
            color: Color::WHITE,
            brightness: 25.0,
            ..default()
        },
    ));

    // Key light straight down: rolled face is brightest, sides fall to shadow.
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 10.0, 0.0).looking_at(Vec3::ZERO, Vec3::Z),
    ));
}

fn setup_ui(mut commands: Commands, theme: Res<MaterialTheme>) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            right: Val::Px(16.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexEnd,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                TotalDisplay,
                Text::new("Total: -"),
                TextFont {
                    font_size: 28.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            parent.spawn((
                BreakdownDisplay,
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.75, 0.75, 0.75)),
            ));
        });

    let input_row = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            left: Val::Px(16.0),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(24.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .id();

    commands.entity(input_row).with_children(|row| {
        spawn_text_field_control_with(
            row,
            &theme,
            TextFieldBuilder::new()
                .label("Dice Term")
                .outlined()
                .width(Val::Px(180.0)),
            DiceInput,
        );
    });

    let button = spawn_material_button(&mut commands, &theme, "Roll", ButtonVariant::Elevated);
    commands.entity(button).insert((
        RollButton,
        Node {
            padding: UiRect::axes(Val::Px(28.0), Val::Px(14.0)),
            min_width: Val::Px(96.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
    ));
    commands.entity(input_row).add_child(button);

    // Centered diceset selector at the top.
    let options: Vec<SelectOption> = DICESETS
        .iter()
        .map(|(label, _)| SelectOption::new(*label))
        .collect();
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|center| {
            center.spawn_select_with(
                &theme,
                SelectBuilder::new(options)
                    .label("Diceset")
                    .outlined()
                    .selected(0)
                    .width(Val::Px(220.0)),
            );
        });
}

fn handle_diceset_change(
    mut events: MessageReader<SelectChangeEvent>,
    mut arenas: Query<&mut DiceArena, With<DefaultArena>>,
) {
    for event in events.read() {
        let Some(&(_, slug)) = DICESETS.get(event.index) else {
            continue;
        };
        let Ok(mut arena) = arenas.single_mut() else {
            continue;
        };
        arena.diceset = format!("{slug}.glb");
        arena.orientations = load_orientations(asset_full_path(slug));
    }
}

fn button_click_to_roll(
    mut clicks: MessageReader<ButtonClickEvent>,
    buttons: Query<Entity, With<RollButton>>,
    mut writer: MessageWriter<RollRequested>,
) {
    let our_button = buttons.iter().next();
    for click in clicks.read() {
        if Some(click.entity) == our_button {
            writer.write(RollRequested);
        }
    }
}

fn text_submit_to_roll(
    mut submits: MessageReader<TextFieldSubmitEvent>,
    fields: Query<Entity, With<DiceInput>>,
    mut writer: MessageWriter<RollRequested>,
) {
    let our_field = fields.iter().next();
    for submit in submits.read() {
        if Some(submit.entity) == our_field {
            writer.write(RollRequested);
        }
    }
}

fn process_roll(
    mut requested: MessageReader<RollRequested>,
    field: Single<&MaterialTextField, With<DiceInput>>,
    old_dice: Query<Entity, With<SpawnedDie>>,
    mut total_text: Single<&mut Text, With<TotalDisplay>>,
    mut breakdown_text: Single<&mut Text, (With<BreakdownDisplay>, Without<TotalDisplay>)>,
    mut roller: DiceRoller,
    mut commands: Commands,
) {
    if requested.read().next().is_none() {
        return;
    }

    let input = field.value.trim();
    if input.is_empty() {
        return;
    }

    for entity in old_dice.iter() {
        commands.entity(entity).despawn();
    }

    match DiceRoll::parse(input) {
        Ok(roll) => {
            ***total_text = "Rolling...".to_string();
            ***breakdown_text = String::new();
            let _ = roller.roll(roll);
        }
        Err(err) => {
            ***total_text = format!("Error: {err}");
            ***breakdown_text = String::new();
        }
    }
}

fn update_display_on_complete(
    mut events: MessageReader<RollComplete>,
    mut total_text: Single<&mut Text, With<TotalDisplay>>,
    mut breakdown_text: Single<&mut Text, (With<BreakdownDisplay>, Without<TotalDisplay>)>,
) {
    for event in events.read() {
        ***total_text = format!("Total: {}", event.outcome.total);
        ***breakdown_text = format_breakdown(&event.roll, &event.outcome);
    }
}

/// Renders "3d6(1,2,3) + 2d4(4,3) - 1" from a parsed roll plus its outcome.
fn format_breakdown(roll: &DiceRoll, outcome: &RollOutcome) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut die_index = 0;
    for term in &roll.terms {
        let count = term.count as usize;
        let end = (die_index + count).min(outcome.dice.len());
        let values: Vec<String> = outcome.dice[die_index..end]
            .iter()
            .map(|d| d.value.to_string())
            .collect();
        die_index = end;
        let chunk = format!("{}d{}({})", term.count, term.kind.sides(), values.join(","));
        let sign = if term.negate { "-" } else { "+" };
        if parts.is_empty() && !term.negate {
            parts.push(chunk);
        } else {
            parts.push(format!("{sign} {chunk}"));
        }
    }
    match roll.adjustment.cmp(&0) {
        std::cmp::Ordering::Greater => parts.push(format!("+ {}", roll.adjustment)),
        std::cmp::Ordering::Less => parts.push(format!("- {}", -roll.adjustment)),
        std::cmp::Ordering::Equal => {}
    }
    if parts.is_empty() {
        format!("{}", roll.adjustment)
    } else {
        parts.join(" ")
    }
}
