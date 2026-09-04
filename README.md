# bevy_dice

Physics-driven dice for Bevy 0.18. Parse a dice expression, drop it into an
arena, watch the dice tumble and settle, read the result.

- Math-only roll API for headless use (`DiceRoll::parse`, `roll`,
  `roll_detailed`) with no Bevy dependency on the caller's side beyond the
  crate import.
- Physics layer built on `avian3d`: per-arena box collider, throw arc, settle
  detection, and a face-orientation table that turns each settled die's
  rotation back into the numeral it shows.
- Standard polyhedral set: d4, d6, d8, d10, d12, d20, d100 (rendered as a
  d100 tens + d10 ones pair).

## Add to your project

```toml
[dependencies]
bevy = "0.18"
bevy_dice = { version = "0.2", features = ["plain_white"] }
```

The diceset features ship a `.glb` baked into the binary so you do not have
to manage assets yourself:

- `plain_white` - white plastic, black labels
- `halloween` - orange and black
- `metal` - brushed metal
- `clear_orange` - translucent orange

Two larger textured sets (`frosty`, `fiery`) are not feature-gated; copy
them out of this repo's `assets/` directory and load them through the
asset server like any other gltf.

I also have a process to generate custom dice, feel free to contact me
with a request.

## Components and how they connect

- **`Diceset`** - a gltf asset path plus its face-orientation table. Cached
  mesh + material handles populate automatically once the asset loads. Spawn
  one per distinct diceset; many arenas can share one.
- **`DiceArena`** - a playing-area box. Holds an `Entity` reference to its
  `Diceset` (use `.diceset(entity)`).
- **`DefaultArena`** - marker component. Tag exactly one arena to make it
  the target of `roller.roll(...)` / `roller.roll_expr("...")`.
- **`SpawnedDie`** - marker on every die entity, plus the arena it belongs to.

## Quick start

- Add `DicePlugin::default()` to your `App` (after `DefaultPlugins`).
- Enable one diceset feature so a default arena (and its `Diceset` entity)
  auto-spawn at startup. Or spawn them yourself (see "Custom arena").
- Trigger rolls with `roller.roll_expr("3d6+2")` (a `DiceRoller` system
  param); read results from `MessageReader<RollComplete>`.

No camera setup is required out of the box - dice render on layer `0`, the
same layer as everything else in your scene.

See [`examples/simple_d20.rs`](examples/simple_d20.rs), use` cargo run 
--example simple_d20 --features plain_white` to run it.

## Dice expressions

```
3d6              three six-siders
d20+5            one d20 plus a flat 5
2d6+1d4-1        compound expression with adjustment
1d20-1d4         negated terms
```

Whitespace is ignored, `d` and `D` both work. Supported sides: 4, 6, 8, 10,
12, 20, 100. See [`DiceRoll::parse`] for the full grammar. A leading `+` or
`-` is rejected; write `3d6`, not `+3d6`.

See [`dice_expressions`](examples/dice_expressions.rs), use `cargo run --example 
dice_expressions --features plain_white` to run it.

## Reading results

```rust
use bevy::prelude::*;
use bevy_dice::ui::RollComplete;

fn report(mut completed: MessageReader<RollComplete>) {
    for event in completed.read() {
        // Pretty-print "3d6(4,2,1) + 2 = 9"
        info!("{}", event.outcome.display(&event.roll));

        // Or read the raw fields:
        info!("rolled {}", event.outcome.total);
        for die in &event.outcome.dice {
            info!("  d{}: {}", die.kind.sides(), die.value);
        }
    }
}
```

Theres an example of this in [`dice_expressions`](examples/dice_expressions.rs),
use `cargo run --example dice_expressions --features plain_white` to run it.

## Custom arenas

```rust
use bevy_dice::ui::{DefaultArena, DiceArena, Diceset};

fn setup(mut commands: Commands) {
    let diceset = commands.spawn(Diceset::embedded("halloween")).id();
    commands.spawn((
        DiceArena::default()
            .name("table")
            .center(0.0, 0.0, 0.0)
            .size(12.0, 5.0, 6.0)
            .diceset(diceset),
        DefaultArena,
    ));
}
```

`Diceset::embedded(slug)` resolves an enabled embedded feature's gltf bytes.
For a custom diceset shipped in your `assets/` directory, use
`Diceset::custom("my_diceset")` (it appends `.glb`/`.gltf` and reads
orientations from `assets/{slug}`). If orientations live elsewhere, use
`Diceset::custom_with(path, orientations)`.

Spawn multiple `DiceArena` entities to run separate rolling regions in the
same world. They can share a single `Diceset` (one mesh load, one
orientation table, one spawn). Tag at most one with `DefaultArena`; use
`DiceRoller::roll_in(arena_entity, roll)` to target a specific one.

## Render layers

By default, every dice entity and its overhead light spawn on render layer
`0`, so any camera that renders your scene sees them with no extra setup.

You can override the layer per arena (or globally on the plugin) to isolate
the dice from the rest of your scene. This is useful when:

- Your game already has its own lights on layer 0 that double-light the dice.
- You want a HUD-style camera that only renders dice.
- Split-screen: each player's arena draws on a different layer.

To isolate, pick a non-zero layer, set it on the arena, and make sure your
camera includes both the scene layer and the dice layer:

```rust
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_dice::ui::{DefaultArena, DiceArena, DicePlugin, Diceset};

const DICE_LAYER: u8 = 1;

App::new()
    .add_plugins((
        DefaultPlugins,
        // Don't auto-spawn the default arena; we want to pin it to DICE_LAYER.
        DicePlugin { spawn_default_arena: false, ..default() },
    ))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 8.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
            RenderLayers::from_layers(&[0, DICE_LAYER as usize]),
        ));
        let diceset = commands.spawn(Diceset::embedded("plain_white")).id();
        commands.spawn((
            DiceArena::default().diceset(diceset).render_layer(DICE_LAYER),
            DefaultArena,
        ));
    });
```

`DicePlugin::render_layer` sets the default for arenas that don't override
it. `DiceArena::render_layer(N)` overrides per arena. The plugin spawns one
overhead `DirectionalLight` per arena, on the arena's resolved layer.

See [`examples/isolated_layer.rs`](examples/isolated_layer.rs), use `cargo run 
--example isolated_layer --features plain_white` to run it.

## Roll options

```rust
use bevy_dice::dice::{DiceRoll, Options};

let roll = DiceRoll::parse("4d6").unwrap()
    .with_options(Options::default().with_keep_highest(3));
```

- `keep_highest(n)` / `keep_lowest(n)` - drop dice after rolling.
- `reroll_at_or_below(threshold)` - replace low rolls until above the
  threshold (capped by `MAX_OPTION_ITERATIONS` per term).
- `explode_at_or_above(threshold)` - rolls hitting the threshold spawn an
  extra die (same cap).

`DiceRoll::with_explode()` and `with_reroll_ones()` are shorthands.
`DiceRoll::advantage(modifier)` / `disadvantage(modifier)` build a
standard 5e `2d20kh1` / `2d20kl1` roll plus the modifier.

In the physics path, reroll and explode are re-simulated: a die that lands
in the trigger range is despawned and a fresh one is thrown, until it
clears or the budget runs out.

## Determinism

Physics systems pull from a `DiceRng` resource (a boxed `RngCore`). Default
wraps `StdRng::from_entropy()`. For deterministic playback in tests, insert
a seeded one:

```rust
use bevy_dice::ui::DiceRng;
app.insert_resource(DiceRng::from_seed(0xD1CE));
```

## Headless math

If you only need the numbers, skip `DicePlugin` entirely:

```rust
use bevy_dice::dice::DiceRoll;

let total = DiceRoll::parse("3d6+2").unwrap().roll(&mut rand::thread_rng());
```

`roll_detailed` additionally returns each individual die value.

## Inspector support

All public components and messages derive `Reflect` and are registered with
the type registry, so they show up in `bevy-inspector-egui` and similar
tools.

## License

Everything here is available under an MIT license. See [`LICENSE.txt`](LICENSE.txt).
