//! Roll API: [`DiceRoller`] system param plus [`RollRequest`]/[`RollComplete`] messages.

use std::fmt;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::dice::{DiceRoll, ParseError, RollOutcome};

use super::arena::DefaultArena;

/// Request a physics-driven roll in `arena`; a [`RollComplete`] fires once all dice settle.
#[derive(Message, Clone, Reflect)]
pub struct RollRequest {
    /// Matches the eventual [`RollComplete::roll_id`].
    pub roll_id: u64,
    pub roll: DiceRoll,
    pub arena: Entity,
}

/// Emitted once every die for `roll_id` has settled.
#[derive(Message, Clone, Reflect)]
pub struct RollComplete {
    pub roll_id: u64,
    pub roll: DiceRoll,
    pub outcome: RollOutcome,
    pub arena: Entity,
}

/// Reasons [`DiceRoller::roll`] (no-arena form) can fail.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NoDefaultArena {
    /// Zero entities carry the [`DefaultArena`] marker.
    None,
    /// More than one entity carries the [`DefaultArena`] marker.
    Ambiguous,
}

/// Reasons [`DiceRoller::roll_expr`] can fail.
#[derive(Debug, Clone, PartialEq)]
pub enum RollError {
    /// The expression failed to parse.
    Parse(ParseError),
    /// No (or multiple) [`DefaultArena`] entities exist.
    Arena(NoDefaultArena),
}

impl fmt::Display for RollError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RollError::Parse(err) => write!(f, "parse error: {err}"),
            RollError::Arena(NoDefaultArena::None) => write!(f, "no DefaultArena in the world"),
            RollError::Arena(NoDefaultArena::Ambiguous) => write!(f, "multiple DefaultArenas in the world"),
        }
    }
}

impl std::error::Error for RollError {}

impl From<ParseError> for RollError {
    fn from(err: ParseError) -> Self {
        RollError::Parse(err)
    }
}

impl From<NoDefaultArena> for RollError {
    fn from(err: NoDefaultArena) -> Self {
        RollError::Arena(err)
    }
}

/// Monotonic roll-id counter shared by [`DiceRoller`].
#[derive(Resource, Default)]
pub(super) struct NextRollId(pub u64);

/// System param for triggering rolls. [`roll`](Self::roll) targets the
/// [`DefaultArena`]; [`roll_in`](Self::roll_in) targets a specific arena.
///
/// ```no_run
/// # use bevy_dice::{dice::DiceRoll, ui::DiceRoller};
/// fn cast(mut roller: DiceRoller) {
///     let _ = roller.roll(DiceRoll::parse("3d6").unwrap());
/// }
/// ```
#[derive(SystemParam)]
pub struct DiceRoller<'w, 's> {
    writer: MessageWriter<'w, RollRequest>,
    default_arena: Query<'w, 's, Entity, With<DefaultArena>>,
    next_roll_id: ResMut<'w, NextRollId>,
}

impl<'w, 's> DiceRoller<'w, 's> {
    /// Submits `roll` to the [`DefaultArena`]; errors if zero or multiple
    /// arenas carry that marker. Accepts [`DiceRoll`] or `&DiceRoll`.
    pub fn roll(&mut self, roll: impl Into<DiceRoll>) -> Result<u64, NoDefaultArena> {
        let mut iter = self.default_arena.iter();
        let arena = iter.next().ok_or(NoDefaultArena::None)?;
        if iter.next().is_some() {
            return Err(NoDefaultArena::Ambiguous);
        }
        Ok(self.roll_in(arena, roll))
    }

    /// Submits `roll` to `arena`; returns the roll id. Accepts [`DiceRoll`] or `&DiceRoll`.
    pub fn roll_in(&mut self, arena: Entity, roll: impl Into<DiceRoll>) -> u64 {
        self.next_roll_id.0 = self.next_roll_id.0.wrapping_add(1);
        let roll_id = self.next_roll_id.0;
        self.writer.write(RollRequest { roll_id, roll: roll.into(), arena });
        roll_id
    }

    /// Parses `expr` and submits to the [`DefaultArena`]. Combined parse +
    /// roll for one-liner call sites.
    ///
    /// ```no_run
    /// # use bevy_dice::ui::DiceRoller;
    /// fn cast(mut roller: DiceRoller) {
    ///     let _ = roller.roll_expr("3d6+2");
    /// }
    /// ```
    pub fn roll_expr(&mut self, expr: &str) -> Result<u64, RollError> {
        let roll = DiceRoll::parse(expr)?;
        Ok(self.roll(roll)?)
    }

    /// Parses `expr` and submits to `arena`.
    pub fn roll_expr_in(&mut self, arena: Entity, expr: &str) -> Result<u64, RollError> {
        let roll = DiceRoll::parse(expr)?;
        Ok(self.roll_in(arena, roll))
    }
}
