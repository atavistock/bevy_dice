//! Physics-driven dice for Bevy 0.18.
//!
//! - [`dice`] - headless parse/roll/modifier math, no Bevy types.
//! - [`ui`] - [`ui::DicePlugin`], [`ui::DiceArena`] + [`ui::Diceset`],
//!   [`ui::DiceRoller`], and the [`ui::RollRequest`] / [`ui::RollComplete`]
//!   messages.
//!
//! Dice render on render layer `0` by default; override per arena with
//! [`ui::DiceArena::render_layer`] or globally with
//! [`ui::DicePlugin::render_layer`]. Optional embedded dicesets:
//! `plain_white`, `halloween`, `metal`, `clear_orange`.

pub mod dice;
pub mod ui;
