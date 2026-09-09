//! Dice arenas, asset loading, roll requests, and trajectory playback.

mod arena;
mod cache;
mod cache_assets;
mod cached_roll;
mod diceset;
mod plugin;
mod rng;
mod roller;
mod spawn;

pub use arena::{DefaultArena, DiceArena};
pub use cache::{PRECOMPUTE_QUEUE_DEPTH, PrecomputeCache};
pub use cached_roll::{OutcomeError, PrecomputeRequest, RollFailed, RollFailure, RollRequest};
pub use diceset::Diceset;
pub use plugin::{DicePlugin, DiceSimulationGravity};
pub use rng::{DiceRng, DiceSimulationRng};
pub use roller::{DiceRoller, NoDefaultArena, RollComplete, RollError};
pub use spawn::SpawnedDie;
