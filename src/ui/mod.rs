//! Physics-driven dice presentation. Each playing area is a [`DiceArena`]
//! entity; dice spawned via [`RollRequest`] or the [`DiceRoller`] system
//! param tumble inside that arena and report rolled values once they settle.
//! Math-only roll calculations stay in `src/dice/` and remain available via
//! `DiceRoll::roll` / `DiceRoll::roll_detailed`.

mod arena;
mod orientations;
mod pending;
mod plugin;
mod resolve;
mod roller;
mod spawn;

pub use arena::{DefaultArena, DiceArena, DiceBoxWall, DicePhysicsConfig, SpawnConfig};
pub use orientations::{load_orientations, load_orientations_from_bytes, DiceOrientations};
pub use plugin::DicePlugin;
pub use roller::{DiceRoller, NoDefaultArena, RollComplete, RollRequest};
pub use spawn::{SpawnedDie, MAX_DICE_PER_ROLL};
