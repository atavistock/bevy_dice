//! Dice arenas, asset loading, roll requests, and trajectory playback.

mod arena;
mod cache;
mod cache_assets;
mod cached_roll;
mod diceset;
mod pending;
mod plugin;
mod resolve;
mod rng;
mod roller;
mod spawn;

pub use arena::{DefaultArena, DiceArena, DiceBoxWall};
pub use cache::{PRECOMPUTE_QUEUE_DEPTH, PrecomputeCache};
pub use cached_roll::{CachedRollRequest, OutcomeError, PrecomputeRequest, RollFailed, RollFailure};
pub use diceset::Diceset;
pub use plugin::DicePlugin;
pub use rng::DiceRng;
pub use roller::{DiceRoller, NoDefaultArena, RollComplete, RollError, RollRequest};
pub use spawn::SpawnedDie;
