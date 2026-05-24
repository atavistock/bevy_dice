//! Caller-facing roll API: the [`DiceRoller`] system param, the
//! [`RollRequest`]/[`RollComplete`] messages, and the shared roll-id counter
//! they use to correlate dispatch with completion.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::dice::{DiceRoll, RollOutcome};

use super::arena::DefaultArena;

/// Request a physics-driven roll in `arena`. The visual layer spawns dice
/// for the parsed expression, runs physics, and emits [`RollComplete`] once
/// all dice settle.
#[derive(Message, Clone)]
pub struct RollRequest {
    pub roll_id: u64,
    pub roll: DiceRoll,
    pub arena: Entity,
}

/// Fires when every die for `roll_id` has come to rest. `outcome.dice` lists
/// the per-die values in term-order; `outcome.total` includes the roll's
/// adjustment and per-term negation.
#[derive(Message, Clone)]
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

/// Monotonic roll-id counter shared by [`DiceRoller`].
#[derive(Resource, Default)]
pub(super) struct NextRollId(pub u64);

/// Ergonomic system param for triggering rolls. Use [`DiceRoller::roll`] for
/// the simple "drop a roll into the default arena" case, or
/// [`DiceRoller::roll_in`] to target a specific arena.
///
/// # Examples
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_dice::dice::DiceRoll;
/// use bevy_dice::ui::DiceRoller;
///
/// fn roll_three_d_six(mut roller: DiceRoller) {
///     let roll = DiceRoll::parse("3d6").unwrap();
///     let _ = roller.roll(roll);
/// }
/// ```
#[derive(SystemParam)]
pub struct DiceRoller<'w, 's> {
    writer: MessageWriter<'w, RollRequest>,
    default_arena: Query<'w, 's, Entity, With<DefaultArena>>,
    next_roll_id: ResMut<'w, NextRollId>,
}

impl<'w, 's> DiceRoller<'w, 's> {
    /// Submits `roll` to the arena tagged with [`DefaultArena`]. Returns the
    /// assigned roll id, or [`NoDefaultArena`] if zero or more than one
    /// arena is tagged. Accepts either an owned [`DiceRoll`] or a `&DiceRoll`
    /// (which is cloned).
    pub fn roll(&mut self, roll: impl Into<DiceRoll>) -> Result<u64, NoDefaultArena> {
        let mut iter = self.default_arena.iter();
        let arena = iter.next().ok_or(NoDefaultArena::None)?;
        if iter.next().is_some() {
            return Err(NoDefaultArena::Ambiguous);
        }
        Ok(self.roll_in(arena, roll))
    }

    /// Submits `roll` to `arena`. Returns the assigned roll id. Accepts
    /// either an owned [`DiceRoll`] or a `&DiceRoll` (which is cloned).
    pub fn roll_in(&mut self, arena: Entity, roll: impl Into<DiceRoll>) -> u64 {
        self.next_roll_id.0 = self.next_roll_id.0.wrapping_add(1);
        let roll_id = self.next_roll_id.0;
        self.writer.write(RollRequest { roll_id, roll: roll.into(), arena });
        roll_id
    }
}
