//! Headless dice math. Parse expressions, roll, apply option modifiers, and
//! read totals - all without spawning anything in the Bevy world.

pub mod kind;
pub mod options;
pub mod parser;
pub mod roll;

pub use kind::DieKind;
pub use options::{Options, OptionsError, MAX_OPTION_ITERATIONS};
pub use parser::ParseError;
pub use roll::{DiceRoll, DiceTerm, RollOutcome, RolledDie};
