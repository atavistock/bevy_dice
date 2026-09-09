//! Requests and validation for cached trajectory playback.

use std::fmt;

use bevy::prelude::*;

use crate::dice::{DiceRoll, DieKind, MAX_OPTION_ITERATIONS, RollOutcome};

use crate::sim::MAX_DICE_PER_ROLL;

/// Requests a cached throw, optionally with externally supplied final faces.
#[derive(Message, Clone, Reflect)]
pub struct RollRequest {
    pub roll_id: u64,
    pub roll: DiceRoll,
    pub arena: Entity,
    pub outcome: Option<RollOutcome>,
}

/// Warms the two-entry trajectory queue for an arena and expression.
#[derive(Message, Clone)]
pub struct PrecomputeRequest {
    pub roll: DiceRoll,
    pub arena: Entity,
}

/// Emitted when a requested cached roll cannot complete.
#[derive(Message, Clone, Debug)]
pub struct RollFailed {
    pub roll_id: u64,
    pub arena: Entity,
    pub reason: RollFailure,
}

/// Failures encountered while preparing or playing a cached roll.
#[derive(Clone, Debug, PartialEq)]
pub enum RollFailure {
    InvalidOutcome(OutcomeError),
    MissingArena,
    MissingDiceset,
    AssetUnavailable,
    InvalidGeometry,
    SimulationFailed,
    ArenaChanged,
    DiceRemoved,
}

impl fmt::Display for RollFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOutcome(err) => write!(formatter, "invalid outcome: {err}"),
            Self::MissingArena => formatter.write_str("arena no longer exists"),
            Self::MissingDiceset => formatter.write_str("arena diceset no longer exists"),
            Self::AssetUnavailable => formatter.write_str("diceset asset is unavailable"),
            Self::InvalidGeometry => formatter.write_str("dice geometry cannot map the requested faces"),
            Self::SimulationFailed => formatter.write_str("simulation did not produce a usable throw"),
            Self::ArenaChanged => formatter.write_str("arena changed during playback"),
            Self::DiceRemoved => formatter.write_str("dice were removed during playback"),
        }
    }
}

impl std::error::Error for RollFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidOutcome(err) => Some(err),
            _ => None,
        }
    }
}

/// Invalid expression settings or externally supplied outcome data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutcomeError {
    NoDice,
    InvalidOptions,
    TermCount,
    DiceCount,
    DieKind,
    DieValue,
    DieSign,
    Total,
    TooManyDice,
}

impl fmt::Display for OutcomeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoDice => "expression must contain at least one dice term",
            Self::InvalidOptions => "invalid dice options",
            Self::TermCount => "outcome term count does not match the expression",
            Self::DiceCount => "outcome dice count does not match its terms",
            Self::DieKind => "outcome die kind does not match its term",
            Self::DieValue => "outcome die value is outside its face range",
            Self::DieSign => "outcome die sign does not match its term",
            Self::Total => "outcome total does not match its dice and adjustment",
            Self::TooManyDice => "roll exceeds the physical dice limit",
        })
    }
}

impl std::error::Error for OutcomeError {}

/// Validates expression options and its initial physical body count.
pub fn validate_roll(roll: &DiceRoll) -> Result<(), OutcomeError> {
    if roll.terms.is_empty() {
        return Err(OutcomeError::NoDice);
    }
    let mut body_count = 0u64;
    for term in roll.terms.iter() {
        term.options.validate_for(term.kind).map_err(|_| OutcomeError::InvalidOptions)?;
        if term.count == 0 {
            return Err(OutcomeError::DiceCount);
        }
        body_count += u64::from(term.count) * if term.kind == DieKind::D100 { 2 } else { 1 };
        if body_count > MAX_DICE_PER_ROLL as u64 {
            return Err(OutcomeError::TooManyDice);
        }
    }
    Ok(())
}

/// Canonical base composition after keep modifiers, excluding additional explosions.
pub fn base_composition(roll: &DiceRoll) -> Result<Vec<DieKind>, OutcomeError> {
    validate_roll(roll)?;
    let mut kinds = Vec::new();
    for term in roll.terms.iter() {
        let mut count = term.count;
        for limit in [term.options.keep_highest, term.options.keep_lowest].into_iter().flatten() {
            count = count.min(limit);
        }
        for _ in 0..count {
            kinds.push(term.kind);
            if term.kind == DieKind::D100 {
                kinds.push(DieKind::D10);
            }
        }
    }
    kinds.sort_by_key(|kind| kind.mesh_index());
    Ok(kinds)
}

/// Validates untrusted final dice without indexing through unchecked term lengths.
pub fn validate_outcome(roll: &DiceRoll, outcome: &RollOutcome) -> Result<(), OutcomeError> {
    validate_roll(roll)?;
    if outcome.term_lengths.len() != roll.terms.len() {
        return Err(OutcomeError::TermCount);
    }
    let length_sum = outcome.term_lengths.iter().try_fold(0usize, |acc, &len| acc.checked_add(len as usize));
    if length_sum != Some(outcome.dice.len()) {
        return Err(OutcomeError::DiceCount);
    }
    let mut offset = 0;
    let mut body_count = 0;
    let mut total = roll.adjustment;
    for (term, &len) in roll.terms.iter().zip(outcome.term_lengths.iter()) {
        let mut minimum_count = term.count;
        let mut maximum_count = term.count;
        if term.options.explode_at_or_above.is_some_and(|threshold| threshold <= term.kind.sides()) {
            maximum_count = maximum_count.saturating_add(MAX_OPTION_ITERATIONS);
        }
        for limit in [term.options.keep_highest, term.options.keep_lowest].into_iter().flatten() {
            minimum_count = minimum_count.min(limit);
            maximum_count = maximum_count.min(limit);
        }
        if len < minimum_count || len > maximum_count {
            return Err(OutcomeError::DiceCount);
        }
        body_count += len as usize * if term.kind == DieKind::D100 { 2 } else { 1 };
        if body_count > MAX_DICE_PER_ROLL {
            return Err(OutcomeError::TooManyDice);
        }
        let end = offset + len as usize;
        let mut term_total = 0i32;
        for die in outcome.dice[offset..end].iter() {
            if die.kind != term.kind {
                return Err(OutcomeError::DieKind);
            }
            if !(1..=term.kind.sides()).contains(&die.value) {
                return Err(OutcomeError::DieValue);
            }
            if die.negate != term.negate {
                return Err(OutcomeError::DieSign);
            }
            term_total = term_total.saturating_add(die.value as i32);
        }
        total = total.saturating_add(if term.negate { -term_total } else { term_total });
        offset = end;
    }
    if total != outcome.total {
        return Err(OutcomeError::Total);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::dice::{Options, RolledDie};

    fn outcome(roll: &DiceRoll) -> RollOutcome {
        roll.roll_detailed(&mut StdRng::seed_from_u64(42))
    }

    #[test]
    fn mixed_signs_and_saturating_totals_match_math() {
        let mut roll = DiceRoll::parse("2d20-1d4+5").expect("valid expression");
        for adjustment in [5, i32::MAX, i32::MIN] {
            roll.adjustment = adjustment;
            assert_eq!(validate_outcome(&roll, &outcome(&roll)), Ok(()));
        }
    }

    #[test]
    fn malformed_term_lengths_are_rejected_without_slicing() {
        let roll = DiceRoll::parse("1d6+1d4").expect("valid expression");
        let mut result = outcome(&roll);
        result.term_lengths = vec![u32::MAX, u32::MAX];
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::DiceCount));
        result.term_lengths.clear();
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::TermCount));
    }

    #[test]
    fn rejects_inconsistent_faces_signs_kinds_and_totals() {
        let roll = DiceRoll::parse("1d6").expect("valid expression");
        let valid = outcome(&roll);
        for value in [0, 7, u32::MAX] {
            let mut result = valid.clone();
            result.dice[0].value = value;
            assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::DieValue));
        }
        let mut result = valid.clone();
        result.dice[0].negate = true;
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::DieSign));
        result = valid.clone();
        result.dice[0].kind = DieKind::D4;
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::DieKind));
        result = valid;
        result.total += 1;
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::Total));
    }

    #[test]
    fn percentile_pairs_count_as_two_bodies() {
        let roll = DiceRoll::parse("10d100").expect("valid expression");
        assert_eq!(validate_outcome(&roll, &outcome(&roll)), Ok(()));
        assert_eq!(
            validate_roll(&DiceRoll::parse("11d100").expect("valid expression")),
            Err(OutcomeError::TooManyDice)
        );
        assert_eq!(
            validate_roll(&DiceRoll::parse("10d100+1d4").expect("valid expression")),
            Err(OutcomeError::TooManyDice)
        );
    }

    #[test]
    fn keep_and_explode_counts_follow_final_contributions() {
        let mut roll = DiceRoll::parse("4d6").expect("valid expression");
        roll.terms[0].options = Options::default().with_keep_highest(3).with_keep_lowest(2);
        assert_eq!(validate_outcome(&roll, &outcome(&roll)), Ok(()));
        roll.terms[0].options = Options::default().with_explode_at_or_above(6).with_keep_highest(6);
        for count in [4, 5, 6] {
            let result = RollOutcome {
                dice: vec![RolledDie { kind: DieKind::D6, value: 6, negate: false }; count],
                term_lengths: vec![count as u32],
                total: count as i32 * 6,
            };
            assert_eq!(validate_outcome(&roll, &result), Ok(()));
        }
        let mut result = outcome(&roll);
        result.dice.truncate(3);
        result.term_lengths = vec![3];
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::DiceCount));
    }

    #[test]
    fn exploded_outcomes_cannot_exceed_body_limit() {
        let roll = DiceRoll::parse("10d100").expect("valid expression").with_explode();
        let result = RollOutcome {
            dice: vec![RolledDie { kind: DieKind::D100, value: 100, negate: false }; 11],
            term_lengths: vec![11],
            total: 1100,
        };
        assert_eq!(validate_outcome(&roll, &result), Err(OutcomeError::TooManyDice));
    }

    #[test]
    fn invalid_options_are_rejected_before_rolling() {
        let roll = DiceRoll::parse("1d6")
            .expect("valid expression")
            .with_options(Options::default().with_reroll_at_or_below(6));
        assert_eq!(validate_roll(&roll), Err(OutcomeError::InvalidOptions));
    }
    #[test]
    fn constant_only_requests_and_warming_are_rejected() {
        for adjustment in [0, 5, -5] {
            let roll = DiceRoll { terms: vec![], adjustment };
            let outcome = RollOutcome { total: adjustment, dice: vec![], term_lengths: vec![] };
            assert_eq!(validate_roll(&roll), Err(OutcomeError::NoDice));
            assert_eq!(base_composition(&roll), Err(OutcomeError::NoDice));
            assert_eq!(validate_outcome(&roll, &outcome), Err(OutcomeError::NoDice));
        }
    }
}
