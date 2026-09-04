//! Headless dice math: parse, roll, apply modifiers, read totals.

pub mod kind;
pub mod options;
pub mod parser;
pub mod roll;

pub use kind::DieKind;
pub use options::{MAX_OPTION_ITERATIONS, Modifier, Options, OptionsError};
pub use parser::{MAX_DICE_PER_TERM, ParseError};
pub use roll::{DiceRoll, DiceTerm, RollOutcome, RollOutcomeDisplay, RolledDie};
