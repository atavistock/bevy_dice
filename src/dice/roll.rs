use rand::Rng;

use crate::dice::kind::DieKind;
use crate::dice::options::Options;

/// One `NdK` chunk inside a [`DiceRoll`]. `negate` flips its sign in the
/// total; `options` carries per-term modifiers (keep, reroll, explode).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiceTerm {
    pub count: u32,
    pub kind: DieKind,
    pub negate: bool,
    pub options: Options,
}

/// A parsed dice expression: zero or more [`DiceTerm`]s plus a flat
/// `adjustment` summed into the total. Build with [`DiceRoll::parse`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiceRoll {
    pub terms: Vec<DiceTerm>,
    pub adjustment: i32,
}

impl From<&DiceRoll> for DiceRoll {
    fn from(roll: &DiceRoll) -> Self {
        roll.clone()
    }
}

/// One die's contribution to a [`RollOutcome`]. `negate` is copied from the
/// originating term so callers can render `-` prefixes when displaying.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolledDie {
    pub kind: DieKind,
    pub value: u32,
    pub negate: bool,
}

/// Result of [`DiceRoll::roll_detailed`]. `total` already includes the
/// expression's adjustment and per-term negation; `dice` lists the
/// individual values in term order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollOutcome {
    pub total: i32,
    pub dice: Vec<RolledDie>,
}

impl DiceRoll {
    /// Applies `options` to every term. Useful for single-term rolls; for
    /// mixed-kind rolls, mutate `terms[i].options` directly instead.
    pub fn with_options(mut self, options: Options) -> Self {
        for term in &mut self.terms {
            term.options = options.clone();
        }
        self
    }

    /// Causes any die showing its max value to spawn an additional die,
    /// recursively. Applied to every term in the roll.
    pub fn with_explode(mut self) -> Self {
        for term in &mut self.terms {
            term.options.explode_at_or_above = Some(term.kind.sides());
        }
        self
    }

    /// Re-rolls any die showing 1 until it shows a higher value or the
    /// iteration cap fires. Applied to every term in the roll.
    pub fn with_reroll_ones(mut self) -> Self {
        for term in &mut self.terms {
            term.options.reroll_at_or_below = Some(1);
        }
        self
    }

    pub fn roll<R: Rng + ?Sized>(&self, rng: &mut R) -> i32 {
        self.roll_detailed(rng).total
    }

    /// Same as `roll` but also returns each rolled die's kind, value, and sign
    /// contribution. Useful when callers need to display the individual results.
    pub fn roll_detailed<R: Rng + ?Sized>(&self, rng: &mut R) -> RollOutcome {
        let mut dice = Vec::new();
        let mut total: i32 = self.adjustment;
        for term in &self.terms {
            let mut rolls: Vec<u32> = (0..term.count).map(|_| term.kind.roll(rng)).collect();
            term.options.apply(term.kind, &mut rolls, rng);
            let term_total: i32 = rolls.iter().map(|&r| r as i32).sum();
            total += if term.negate { -term_total } else { term_total };
            for value in rolls {
                dice.push(RolledDie { kind: term.kind, value, negate: term.negate });
            }
        }
        RollOutcome { total, dice }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(0xD1CE)
    }

    #[test]
    fn roll_total_stays_in_expected_range() {
        let roll = DiceRoll::parse("3d6+2").unwrap();
        let mut rng = rng();
        for _ in 0..200 {
            let total = roll.roll(&mut rng);
            assert!(total >= 3 + 2 && total <= 18 + 2);
        }
    }

    #[test]
    fn roll_applies_adjustment_when_no_dice() {
        let roll = DiceRoll::parse("d4-5").unwrap();
        let mut rng = rng();
        for _ in 0..50 {
            let total = roll.roll(&mut rng);
            assert!(total >= -4 && total <= -1);
        }
    }

    #[test]
    fn roll_detailed_returns_one_entry_per_rolled_die() {
        let roll = DiceRoll::parse("3d6").unwrap();
        let outcome = roll.roll_detailed(&mut rng());
        assert_eq!(outcome.dice.len(), 3);
        assert!(outcome.dice.iter().all(|d| d.kind == DieKind::D6));
        assert!(outcome.dice.iter().all(|d| !d.negate));
    }

    #[test]
    fn negated_term_marks_dice_and_subtracts_from_total() {
        let roll = DiceRoll::parse("1d20-1d4").unwrap();
        let outcome = roll.roll_detailed(&mut rng());
        let positive: i32 = outcome.dice.iter().filter(|d| !d.negate).map(|d| d.value as i32).sum();
        let negative: i32 = outcome.dice.iter().filter(|d| d.negate).map(|d| d.value as i32).sum();
        assert_eq!(outcome.total, positive - negative);
    }

    #[test]
    fn with_options_applies_to_all_terms() {
        let roll = DiceRoll::parse("2d6+1d4")
            .unwrap()
            .with_options(Options::default().with_keep_highest(1));
        assert_eq!(roll.terms[0].options.keep_highest, Some(1));
        assert_eq!(roll.terms[1].options.keep_highest, Some(1));
    }
}
