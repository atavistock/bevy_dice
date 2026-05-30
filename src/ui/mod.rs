//! Physics-driven dice presentation. Dice tumble inside a [`DiceArena`]
//! (which references a [`Diceset`] entity for its visuals) and report
//! results via [`RollComplete`] once they settle.
//!
//! Render layer is `0` by default. Override per arena with
//! [`DiceArena::render_layer`], or globally with [`DicePlugin::render_layer`];
//! either way, the camera must include the chosen layer.

mod arena;
mod diceset;
mod orientations;
mod pending;
mod plugin;
mod resolve;
mod rng;
mod roller;
mod spawn;

pub use arena::{DefaultArena, DiceArena, DiceBoxWall, DicePhysicsConfig, SpawnConfig};
pub use diceset::Diceset;
pub use orientations::{load_orientations, load_orientations_from_bytes, DiceOrientations};
pub use plugin::DicePlugin;
pub use rng::DiceRng;
pub use roller::{DiceRoller, NoDefaultArena, RollComplete, RollError, RollRequest};
pub use spawn::{SpawnedDie, MAX_DICE_PER_ROLL};
