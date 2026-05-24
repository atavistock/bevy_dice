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

| feature         | size     | look                        |
|-----------------|----------|-----------------------------|
| `plain_white`   | small    | white plastic, black labels |
| `halloween`     | small    | orange and black            |
| `metal`         | small    | brushed metal               |
| `clear_orange`  | small    | translucent orange          |

Two larger textured sets (`frosty`, `fiery`) are not feature-gated; copy
them out of this repo's `assets/` directory and load them through the
asset server like any other gltf.

## Quick start

```rust
use bevy::prelude::*;
use bevy_dice::dice::DiceRoll;
use bevy_dice::ui::{DicePlugin, DiceRoller, RollComplete};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DicePlugin::default())
        .add_systems(Startup, roll_once)
        .add_systems(Update, log_results)
        .run();
}

fn roll_once(mut roller: DiceRoller) {
    let roll = DiceRoll::parse("3d6+2").unwrap();
    let _ = roller.roll(roll);
}

fn log_results(mut completed: MessageReader<RollComplete>) {
    for event in completed.read() {
        info!("rolled {}", event.outcome.total);
    }
}
```

With a diceset feature enabled, `DicePlugin::default()` auto-spawns a
[`DiceArena`] tagged with [`DefaultArena`] so `roller.roll(...)` works with
zero setup. Disable that with `DicePlugin { spawn_default_arena: false,
..default() }` if you want to place arenas yourself.

## Custom arena

```rust
use bevy_dice::ui::{DefaultArena, DiceArena, load_orientations};

fn setup(mut commands: Commands) {
    let arena = DiceArena::default()
        .name("table")
        .center(0.0, 0.0, 0.0)
        .size(12.0, 5.0, 6.0)
        .diceset("my_diceset.glb")
        .orientations(load_orientations("assets/my_diceset.glb"));
    commands.spawn((arena, DefaultArena));
}
```

Spawn multiple `DiceArena` entities to run separate rolling regions in the
same world. Tag at most one with `DefaultArena`; use
`DiceRoller::roll_in(arena_entity, roll)` to target a specific one.

## Dice expressions

```
3d6              three six-siders
d20+5            one d20 plus a flat 5
2d6+1d4-1        compound expression with adjustment
1d20-1d4         negated terms
```

Whitespace is ignored, `d` and `D` both work. Supported sides: 4, 6, 8, 10,
12, 20, 100. See [`DiceRoll::parse`] for the full grammar.

## Roll options

```rust
use bevy_dice::dice::{DiceRoll, Options};

let roll = DiceRoll::parse("4d6").unwrap()
    .with_options(Options::default().with_keep_highest(3));
```

- `keep_highest(n)` / `keep_lowest(n)` - drop dice after rolling.
- `reroll_at_or_below(threshold)` - replace low rolls until above the
  threshold (capped by `MAX_OPTION_ITERATIONS`).
- `explode_at_or_above(threshold)` - rolls hitting the threshold spawn an
  extra die (same cap).

`DiceRoll::with_explode()` and `with_reroll_ones()` are shorthands for the
common cases.

## Headless math

If you only need the numbers, skip [`DicePlugin`] entirely:

```rust
use bevy_dice::dice::DiceRoll;

let total = DiceRoll::parse("3d6+2").unwrap().roll(&mut rand::thread_rng());
```

`roll_detailed` additionally returns each individual die value.

## Example

```
cargo run --example tester --features plain_white
```

An interactive window with a text field, a diceset selector, and a top-down
view of the arena.

## License

See repository.
