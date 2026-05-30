//! Headless dice math: parse, roll, apply modifiers, read totals.

pub mod kind;
pub mod options;
pub mod parser;
pub mod roll;

pub use kind::DieKind;
pub use options::{Options, OptionsError, MAX_OPTION_ITERATIONS};
pub use parser::ParseError;
pub use roll::{DiceRoll, DiceTerm, RollOutcome, RollOutcomeDisplay, RolledDie};
