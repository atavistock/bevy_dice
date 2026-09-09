//! Roll API: [`DiceRoller`] system param plus [`RollRequest`]/[`RollComplete`] messages.

use std::fmt;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::dice::{DiceRoll, ParseError, RollOutcome};

use super::arena::DefaultArena;
use super::cached_roll::{CachedRollRequest, OutcomeError, PrecomputeRequest, validate_outcome, validate_roll};

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
    /// The supplied outcome is inconsistent with the expression.
    Outcome(OutcomeError),
}

impl fmt::Display for RollError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RollError::Outcome(err) => write!(f, "invalid outcome: {err}"),
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

impl From<OutcomeError> for RollError {
    fn from(err: OutcomeError) -> Self {
        RollError::Outcome(err)
    }
}

/// Monotonic roll-id counter shared by [`DiceRoller`].
#[derive(Resource, Default)]
pub(super) struct NextRollId {
    pub value: u64,
}

/// System param for triggering rolls. [`roll`](Self::roll) targets the
/// [`DefaultArena`]; [`roll_in`](Self::roll_in) targets a specific arena.
///
/// ```no_run
/// # use bevy_dice::{dice::DiceRoll, render::DiceRoller};
/// fn cast(mut roller: DiceRoller) {
///     let _ = roller.roll(DiceRoll::parse("3d6").unwrap());
/// }
/// ```
#[derive(SystemParam)]
pub struct DiceRoller<'w, 's> {
    writer: MessageWriter<'w, CachedRollRequest>,
    warm_writer: MessageWriter<'w, PrecomputeRequest>,
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

    /// Queues a cached throw in `arena`, waiting for refill when empty; returns its roll id.
    pub fn roll_in(&mut self, arena: Entity, roll: impl Into<DiceRoll>) -> u64 {
        self.next_roll_id.value = self.next_roll_id.value.wrapping_add(1);
        let roll_id = self.next_roll_id.value;
        self.writer.write(CachedRollRequest { roll_id, roll: roll.into(), arena, outcome: None });
        roll_id
    }

    /// Submits validated final faces to `arena` for cached trajectory playback.
    pub fn roll_in_with_outcome(
        &mut self,
        arena: Entity,
        roll: impl Into<DiceRoll>,
        outcome: RollOutcome,
    ) -> Result<u64, OutcomeError> {
        let roll = roll.into();
        validate_outcome(&roll, &outcome)?;
        self.next_roll_id.value = self.next_roll_id.value.wrapping_add(1);
        let roll_id = self.next_roll_id.value;
        self.writer.write(CachedRollRequest { roll_id, roll, arena, outcome: Some(outcome) });
        Ok(roll_id)
    }

    /// Submits validated final faces to the default arena.
    pub fn roll_with_outcome(&mut self, roll: impl Into<DiceRoll>, outcome: RollOutcome) -> Result<u64, RollError> {
        let mut arenas = self.default_arena.iter();
        let arena = arenas.next().ok_or(NoDefaultArena::None)?;
        if arenas.next().is_some() {
            return Err(NoDefaultArena::Ambiguous.into());
        }
        Ok(self.roll_in_with_outcome(arena, roll, outcome)?)
    }

    /// Fills the arena and expression queue without starting playback.
    pub fn precompute_in(&mut self, arena: Entity, roll: impl Into<DiceRoll>) -> Result<(), OutcomeError> {
        let roll = roll.into();
        validate_roll(&roll)?;
        self.warm_writer.write(PrecomputeRequest { roll, arena });
        Ok(())
    }

    /// Parses `expr` and submits to the [`DefaultArena`]. Combined parse +
    /// roll for one-liner call sites.
    ///
    /// ```no_run
    /// # use bevy_dice::render::DiceRoller;
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

#[cfg(test)]
mod tests {
    use bevy::ecs::system::SystemState;

    use super::*;
    use crate::dice::{DieKind, RolledDie};

    #[test]
    fn rejected_outcome_does_not_consume_id_or_queue_request() {
        let mut world = World::new();
        world.init_resource::<Messages<CachedRollRequest>>();
        world.init_resource::<Messages<PrecomputeRequest>>();
        world.init_resource::<NextRollId>();
        let arena = world.spawn(DefaultArena).id();
        let roll = DiceRoll::parse("1d6").expect("valid expression");
        let mut state = SystemState::<DiceRoller>::new(&mut world);
        let invalid = RollOutcome {
            total: 7,
            dice: vec![RolledDie { kind: DieKind::D6, value: 7, negate: false }],
            term_lengths: vec![1],
        };
        assert_eq!(
            state.get_mut(&mut world).expect("roller parameters").roll_in_with_outcome(arena, &roll, invalid),
            Err(OutcomeError::DieValue)
        );
        assert_eq!(world.resource::<NextRollId>().value, 0);
        assert!(world.resource::<Messages<CachedRollRequest>>().is_empty());
        let valid = RollOutcome {
            total: 6,
            dice: vec![RolledDie { kind: DieKind::D6, value: 6, negate: false }],
            term_lengths: vec![1],
        };
        assert_eq!(
            state.get_mut(&mut world).expect("roller parameters").roll_with_outcome(&roll, valid.clone()),
            Ok(1)
        );
        let queued = world.resource_mut::<Messages<CachedRollRequest>>().drain().collect::<Vec<_>>();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].arena, arena);
        assert_eq!(queued[0].outcome, Some(valid));
        state.get_mut(&mut world).expect("roller parameters").precompute_in(arena, roll).expect("valid warm request");
        assert_eq!(world.resource::<NextRollId>().value, 1);
        assert_eq!(world.resource::<Messages<PrecomputeRequest>>().len(), 1);
    }
}
