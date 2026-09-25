# bevy_dice

Physics-driven dice for Bevy 0.19. Parse a dice expression, roll it in an arena, watch the dice tumble and settle, read the result.

- `dice`: expression parsing and roll math, usable headless.
- `sim`: background physics on `avian3d`, recorded as packed trajectories. The host world needs no physics plugin.
- `render`: dicesets, arenas, roll queues, and playback.
- Standard polyhedral set: d4, d6, d8, d10, d12, d20, d100 (a tens die plus a d10).

## Add to your project

```toml
[dependencies]
bevy = "0.19"
bevy_dice = { version = "0.4", features = ["plain_white"] }
```

Each diceset feature bakes a `.glb` into the binary:

- `plain_white` - white plastic, black labels
- `halloween` - orange and black
- `metal` - brushed metal
- `clear_orange` - translucent orange

Two larger textured sets (`frosty`, `fiery`) are not feature-gated; copy them from this repo's `assets/` directory and load them as a custom diceset. I can also generate custom dice on request.

## Quick start

- Add `DicePlugin::default()` after `DefaultPlugins`.
- Enable one diceset feature and a default arena spawns at startup, or spawn your own (see "Custom arenas").
- Roll with the `DiceRoller` system param, such as `roller.roll_expr("3d6+2")`, and read `MessageReader<RollComplete>`.

Dice render on layer `0` with the rest of your scene, so no camera setup is needed.

Examples run with `cargo run --example <name> --features plain_white`: [`simple_d20`](examples/simple_d20.rs), [`dice_expressions`](examples/dice_expressions.rs), [`two_arenas`](examples/two_arenas.rs), [`isolated_layer`](examples/isolated_layer.rs).

## Components

- `Diceset`: a gltf asset path plus its face-orientation table. Spawn one per distinct diceset; arenas can share it.
- `DiceArena`: a playing-area box that references its `Diceset` entity.
- `DefaultArena`: marker for the arena targeted by `roller.roll(...)` and `roller.roll_expr(...)`. Tag at most one.
- `SpawnedDie`: marker on every die entity, with its kind and arena.

## Dice expressions

- `3d6`: three six-sided dice.
- `d20+5`: one d20 plus five.
- `2d6+1d4-1`: compound expression with an adjustment.
- `1d20-1d4`: negated term.
- `2d20kh1+5`: keep the highest die, then add five.
- `4d6kl3r1e6`: reroll ones, explode sixes, then keep the lowest three.

Whitespace is ignored and `d`/`D` both work. Sides: 4, 6, 8, 10, 12, 20, 100. Modifiers `khN`, `klN`, `rN`, and `eN` are case-insensitive and combine in any order.

Parse errors: a leading `+` or `-`, no dice term (`ParseError::NoDice`), an adjustment outside +/-2147483647 (`ParseError::Overflow`), and duplicate, incomplete, or invalid modifiers.

## Reading results

```rust
use bevy::prelude::*;
use bevy_dice::render::RollComplete;

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

`RollFailed` reports rolls that cannot complete: invalid input, unavailable assets, or interrupted playback.

## Forcing a result

Decide the faces yourself (a server roll, a replay, a scripted moment) and the dice land on them. The throw is still a recorded physics trajectory; each die is pre-rotated by a symmetry of its mesh so the requested face settles on top.

```rust
use bevy_dice::dice::{DiceRoll, DieKind, RollOutcome, RolledDie};
use bevy_dice::render::DiceRoller;

fn cast(mut roller: DiceRoller) {
    let roll = DiceRoll::parse("2d6+3").unwrap();
    let outcome = RollOutcome {
        total: 14,
        dice: vec![
            RolledDie { kind: DieKind::D6, value: 6, negate: false },
            RolledDie { kind: DieKind::D6, value: 5, negate: false },
        ],
        // Dice per term, in expression order.
        term_lengths: vec![2],
    };
    let _roll_id = roller.roll_with_outcome(&roll, outcome).unwrap();
}
```

- `roll_with_outcome` targets the `DefaultArena`; `roll_in_with_outcome(arena, &roll, outcome)` targets a specific one.
- An outcome that does not fit the expression returns an `OutcomeError` and consumes no roll id. Checked: term count, dice per term, die kind, face range, sign, total, and the 20 physical dice limit.
- List only dice that count toward the total: the kept dice for `khN`/`klN`, plus any exploded extras for `eN`. `roll.roll_detailed(&mut rng)` builds a valid outcome from your own generator.
- A d100 value is `1..=100`; `100` lands as `00` and `0`. A d10 value of `10` lands on `0`.

Generated rolls play the same way, so every diceset needs numeric face labels in `extras.dice_orientations` and rotationally symmetric meshes. The embedded dicesets qualify; otherwise rolls fail with `RollFailure::InvalidGeometry`.

## Custom arenas

```rust
use bevy_dice::render::{DefaultArena, DiceArena, Diceset};

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

- `Diceset::embedded(slug)` loads an enabled diceset feature.
- `Diceset::custom("my_diceset")` loads `assets/my_diceset.glb` and reads its orientations; use `Diceset::custom_with(path, orientations)` when they live elsewhere.
- Spawn several `DiceArena` entities for separate rolling regions; they can share one `Diceset`. Target one with `roller.roll_in(arena, roll)`.
- `DicePlugin::gravity` sets simulation gravity without touching host physics; change `DiceSimulationGravity.acceleration` at runtime. Arena floor and walls exist only in the background simulation.

## How playback works

Rolls play precomputed throws, so results appear without a physics step in your world.

- Each arena keeps a queue of two ready throws per physical dice composition. Adjustments and term order share a queue.
- Queues refill concurrently on Bevy's async compute task pool, and waiting rolls take priority over top-ups. Browser worker support is not implemented.
- A roll on an empty queue waits for its refill; rolls in one arena play in submission order.
- `roller.precompute_in(arena, &roll)` warms a queue before the first throw, and `PrecomputeCache::ready_count(arena, &roll)` reports it. Both use the dice left after keep modifiers; exploded extras create their own queues on demand.
- A throw that does not settle retries after 250 ms, doubling up to 8 seconds. Crowded arenas settle less often, so give large throws floor space, such as 12 by 10 for 20 d20s.
- Changing an arena's geometry, physics, gravity, or meshes discards its recordings and fails any playback in progress.
- Physics runs at 64 Hz and records at 32 Hz, about 0.9 KB per die per second; playback interpolates.
- `RollRequest` messages use the same queue: `outcome: None` generates a result, `Some(outcome)` forces one.

## Render layers

Dice and their overhead light spawn on render layer `0` by default. Move them to another layer to avoid double lighting from your scene, to draw dice with a HUD camera, or to split arenas across split-screen views. Set the layer on the arena and include it on your camera:

```rust
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy_dice::render::{DefaultArena, DiceArena, DicePlugin, Diceset};

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

`DicePlugin::render_layer` sets the default and `DiceArena::render_layer(N)` overrides it per arena. Each arena gets one overhead `DirectionalLight` on its layer.

## Roll options

```rust
use bevy_dice::dice::{DiceRoll, Options};

let roll = DiceRoll::parse("4d6").unwrap()
    .with_options(Options::default().with_keep_highest(3));
```

- `keep_highest(n)` / `keep_lowest(n)`: drop dice after rolling.
- `reroll_at_or_below(threshold)`: replace low rolls, capped by `MAX_OPTION_ITERATIONS` per term.
- `explode_at_or_above(threshold)`: rolls at the threshold add an extra die (same cap).

Shorthands: `DiceRoll::with_explode()`, `with_reroll_ones()`, and `DiceRoll::advantage(modifier)` / `disadvantage(modifier)` for 5e `2d20kh1` / `2d20kl1`. Modifiers resolve before playback, so only the contributing dice are thrown.

## Determinism

Gameplay outcomes draw from `DiceRng` (any boxed `RngCore`); simulation seeds draw from the independent `DiceSimulationRng`, so warming and refills never advance the gameplay stream. Both default to entropy. Seed either one for repeatable results:

```rust
use bevy_dice::render::{DiceRng, DiceSimulationRng};
app.insert_resource(DiceRng::from_seed(0xD1CE));
app.insert_resource(DiceSimulationRng::from_seed(0xD1CE));
```

## Headless math

If you only need the numbers, skip `DicePlugin`:

```rust
use bevy_dice::dice::DiceRoll;

let total = DiceRoll::parse("3d6+2").unwrap().roll(&mut rand::thread_rng());
```

`roll_detailed` also returns each die value.

## Testing

Set `BEVY_DICE_SMOKE_SCREENSHOT=/tmp/dice.png` when running an example to submit its demonstration roll, verify completion, save a screenshot, and exit.

## Inspector support

Components, `RollRequest`, and `RollComplete` derive `Reflect` and are registered, so they show up in `bevy-inspector-egui` and similar tools.

## License

BSD-2-Clause-Patent. See [`LICENSE.txt`](LICENSE.txt).
