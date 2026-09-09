use std::fmt;

use bevy::reflect::Reflect;
use rand::Rng;

use crate::dice::kind::DieKind;
use crate::dice::options::Options;

/// One `NdK` chunk of a [`DiceRoll`] with optional sign and modifiers.
#[derive(Clone, Debug, Eq, PartialEq, Reflect)]
pub struct DiceTerm {
    /// `N` in `NdK`. The parser rejects `0`; `count == 0` produces an empty roll.
    pub count: u32,
    /// `K` in `NdK`.
    pub kind: DieKind,
    /// Subtract this term's total from the roll.
    pub negate: bool,
    /// Per-term modifiers (keep, reroll, explode).
    pub options: Options,
}

impl DiceTerm {
    /// Writes `NdK` plus option suffixes (`kh2`, `kl1`, `r1`, `e6`) without a sign.
    fn fmt_unsigned(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}d{}", self.count, self.kind.sides())?;
        if let Some(count) = self.options.keep_highest {
            write!(f, "kh{count}")?;
        }
        if let Some(count) = self.options.keep_lowest {
            write!(f, "kl{count}")?;
        }
        if let Some(threshold) = self.options.reroll_at_or_below {
            write!(f, "r{threshold}")?;
        }
        if let Some(threshold) = self.options.explode_at_or_above {
            write!(f, "e{threshold}")?;
        }
        Ok(())
    }
}

/// Renders as `"+3d6"` / `"-1d4"` / `"+4d6kh3"`; the leading sign is always present.
impl fmt::Display for DiceTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", if self.negate { '-' } else { '+' })?;
        self.fmt_unsigned(f)
    }
}

/// Parsed dice expression: terms plus a flat adjustment. Build via [`DiceRoll::parse`].
#[derive(Clone, Debug, Eq, PartialEq, Reflect)]
pub struct DiceRoll {
    /// Terms in source order.
    pub terms: Vec<DiceTerm>,
    /// Flat integer added to the total.
    pub adjustment: i32,
}

impl From<&DiceRoll> for DiceRoll {
    fn from(roll: &DiceRoll) -> Self {
        roll.clone()
    }
}

/// Renders as `"3d6+2"` / `"1d20-1d4+5"` / `"2d20kh1+5"`; no sign on the first term.
impl fmt::Display for DiceRoll {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, term) in self.terms.iter().enumerate() {
            if index == 0 {
                if term.negate {
                    write!(f, "-")?;
                }
                term.fmt_unsigned(f)?;
            } else {
                write!(f, "{term}")?;
            }
        }
        if self.terms.is_empty() {
            write!(f, "{}", self.adjustment)
        } else if self.adjustment != 0 {
            write!(f, "{:+}", self.adjustment)
        } else {
            Ok(())
        }
    }
}

/// One die's contribution to a [`RollOutcome`].
#[derive(Clone, Debug, Eq, PartialEq, Reflect)]
pub struct RolledDie {
    /// Die that produced this value.
    pub kind: DieKind,
    /// Face value in `1..=kind.sides()`.
    pub value: u32,
    /// Copied from the source term; true means the value subtracts from the total.
    pub negate: bool,
}

/// Result of [`DiceRoll::roll_detailed`]: final `total`, per-die values in
/// term order, and how many of those came from each term (after keep trims).
#[derive(Clone, Debug, Eq, PartialEq, Reflect)]
pub struct RollOutcome {
    /// Sum of all dice with adjustment and negation applied.
    pub total: i32,
    /// Each rolled die in term order.
    pub dice: Vec<RolledDie>,
    /// Count of dice in `dice` contributed by each term.
    pub term_lengths: Vec<u32>,
}

impl RollOutcome {
    /// Dice contributed by a term; returns an empty slice for missing or malformed ranges.
    pub fn term_dice(&self, term_index: usize) -> &[RolledDie] {
        let Some(&len) = self.term_lengths.get(term_index) else { return &[] };
        let Some(start) = self
            .term_lengths
            .iter()
            .take(term_index)
            .try_fold(0usize, |offset, &count| offset.checked_add(count as usize))
        else {
            return &[];
        };
        let Some(end) = start.checked_add(len as usize) else { return &[] };
        self.dice.get(start..end).unwrap_or(&[])
    }

    /// Human-readable breakdown like `"3d6(4,2,1)+2 = 9"`. Pair with the
    /// originating [`DiceRoll`] so term signs and adjustments render.
    pub fn display<'a>(&'a self, roll: &'a DiceRoll) -> RollOutcomeDisplay<'a> {
        RollOutcomeDisplay { roll, outcome: self }
    }
}

/// `Display` adapter returned by [`RollOutcome::display`].
pub struct RollOutcomeDisplay<'a> {
    roll: &'a DiceRoll,
    outcome: &'a RollOutcome,
}

impl<'a> fmt::Display for RollOutcomeDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (term_index, term) in self.roll.terms.iter().enumerate() {
            let values: Vec<String> = self.outcome.term_dice(term_index).iter().map(|d| d.value.to_string()).collect();
            if first {
                if term.negate {
                    write!(f, "-")?;
                }
                first = false;
            } else {
                write!(f, "{}", if term.negate { " - " } else { " + " })?;
            }
            write!(f, "{}d{}({})", term.count, term.kind.sides(), values.join(","))?;
        }
        if self.roll.adjustment != 0 {
            if first {
                write!(f, "{}", self.roll.adjustment)?;
            } else if self.roll.adjustment > 0 {
                write!(f, " + {}", self.roll.adjustment)?;
            } else {
                write!(f, " - {}", self.roll.adjustment.unsigned_abs())?;
            }
        }
        write!(f, " = {}", self.outcome.total)
    }
}

impl DiceRoll {
    /// Sets `options` on every term; for mixed rolls, mutate `terms[i].options` instead.
    pub fn with_options(mut self, options: Options) -> Self {
        for term in &mut self.terms {
            term.options = options.clone();
        }
        self
    }

    /// Every term explodes on its max face (one extra die, recursively).
    pub fn with_explode(mut self) -> Self {
        for term in &mut self.terms {
            term.options.explode_at_or_above = Some(term.kind.sides());
        }
        self
    }

    /// Every term re-rolls 1s until they clear (capped by [`Options::apply`]).
    pub fn with_reroll_ones(mut self) -> Self {
        for term in &mut self.terms {
            term.options.reroll_at_or_below = Some(1);
        }
        self
    }

    /// `2d20` keep-highest plus `modifier`. Standard 5e advantage roll.
    pub fn advantage(modifier: i32) -> Self {
        Self::two_d20(Options::default().with_keep_highest(1), modifier)
    }

    /// `2d20` keep-lowest plus `modifier`. Standard 5e disadvantage roll.
    pub fn disadvantage(modifier: i32) -> Self {
        Self::two_d20(Options::default().with_keep_lowest(1), modifier)
    }

    fn two_d20(options: Options, modifier: i32) -> Self {
        DiceRoll {
            terms: vec![DiceTerm { count: 2, kind: DieKind::D20, negate: false, options }],
            adjustment: modifier,
        }
    }

    /// Rolls and returns the final total; see [`roll_detailed`](Self::roll_detailed) for per-die values.
    pub fn roll<R: Rng + ?Sized>(&self, rng: &mut R) -> i32 {
        self.roll_detailed(rng).total
    }

    /// Like [`roll`](Self::roll) but also returns each die's kind, value, and sign.
    pub fn roll_detailed<R: Rng + ?Sized>(&self, rng: &mut R) -> RollOutcome {
        let mut dice = Vec::new();
        let mut term_lengths = Vec::with_capacity(self.terms.len());
        let mut total: i32 = self.adjustment;
        for term in &self.terms {
            let mut rolls: Vec<u32> = (0..term.count).map(|_| term.kind.roll(rng)).collect();
            term.options.apply(term.kind, &mut rolls, rng);
            let term_total = rolls.iter().fold(0i32, |acc, &value| acc.saturating_add(value as i32));
            total = total.saturating_add(if term.negate { -term_total } else { term_total });
            term_lengths.push(rolls.len() as u32);
            for value in rolls {
                dice.push(RolledDie { kind: term.kind, value, negate: term.negate });
            }
        }
        RollOutcome { total, dice, term_lengths }
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
            assert!((3 + 2..=18 + 2).contains(&total));
        }
    }

    #[test]
    fn roll_applies_adjustment_when_no_dice() {
        let roll = DiceRoll::parse("d4-5").unwrap();
        let mut rng = rng();
        for _ in 0..50 {
            let total = roll.roll(&mut rng);
            assert!((-4..=-1).contains(&total));
        }
    }

    #[test]
    fn term_dice_slices_by_term() {
        let roll = DiceRoll::parse("2d6+3d4").unwrap();
        let outcome = roll.roll_detailed(&mut rng());
        assert_eq!(outcome.term_lengths, vec![2, 3]);
        assert_eq!(outcome.term_dice(0).len(), 2);
        assert_eq!(outcome.term_dice(1).len(), 3);
        assert!(outcome.term_dice(0).iter().all(|d| d.kind == DieKind::D6));
        assert!(outcome.term_dice(1).iter().all(|d| d.kind == DieKind::D4));
    }

    #[test]
    fn term_dice_reflects_keep_modifier() {
        let roll = DiceRoll::parse("4d6").unwrap().with_options(Options::default().with_keep_highest(2));
        let outcome = roll.roll_detailed(&mut rng());
        assert_eq!(outcome.term_lengths, vec![2]);
        assert_eq!(outcome.term_dice(0).len(), 2);
        assert_eq!(outcome.dice.len(), 2);
    }

    #[test]
    fn term_dice_out_of_range_returns_empty() {
        let roll = DiceRoll::parse("1d6").unwrap();
        let outcome = roll.roll_detailed(&mut rng());
        assert!(outcome.term_dice(5).is_empty());
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
    fn display_positive_term() {
        let term = DiceTerm { count: 3, kind: DieKind::D6, negate: false, options: Options::default() };
        assert_eq!(term.to_string(), "+3d6");
    }

    #[test]
    fn display_negative_term() {
        let term = DiceTerm { count: 1, kind: DieKind::D4, negate: true, options: Options::default() };
        assert_eq!(term.to_string(), "-1d4");
    }

    #[test]
    fn display_term_includes_options() {
        let options = Options::default().with_keep_highest(2).with_reroll_at_or_below(1).with_explode_at_or_above(6);
        let term = DiceTerm { count: 4, kind: DieKind::D6, negate: false, options };
        assert_eq!(term.to_string(), "+4d6kh2r1e6");
    }

    #[test]
    fn display_roll_round_trips_parser_input() {
        for input in ["1d20", "3d6+2", "1d20-1d4+5", "2d6-3"] {
            assert_eq!(DiceRoll::parse(input).unwrap().to_string(), input);
        }
    }

    #[test]
    fn display_roll_with_options_and_adjustment_only() {
        assert_eq!(DiceRoll::advantage(5).to_string(), "2d20kh1+5");
        assert_eq!(DiceRoll::disadvantage(-2).to_string(), "2d20kl1-2");
        assert_eq!(DiceRoll { terms: vec![], adjustment: 7 }.to_string(), "7");
    }

    #[test]
    fn display_uses_parsed_term() {
        let roll = DiceRoll::parse("2d20-1d100").unwrap();
        assert_eq!(roll.terms[0].to_string(), "+2d20");
        assert_eq!(roll.terms[1].to_string(), "-1d100");
    }

    #[test]
    fn with_options_applies_to_all_terms() {
        let roll = DiceRoll::parse("2d6+1d4").unwrap().with_options(Options::default().with_keep_highest(1));
        assert_eq!(roll.terms[0].options.keep_highest, Some(1));
        assert_eq!(roll.terms[1].options.keep_highest, Some(1));
    }

    #[test]
    fn display_renders_simple_roll() {
        let roll = DiceRoll::parse("3d6+2").unwrap();
        let outcome = RollOutcome {
            total: 9,
            dice: vec![
                RolledDie { kind: DieKind::D6, value: 4, negate: false },
                RolledDie { kind: DieKind::D6, value: 2, negate: false },
                RolledDie { kind: DieKind::D6, value: 1, negate: false },
            ],
            term_lengths: vec![3],
        };
        assert_eq!(outcome.display(&roll).to_string(), "3d6(4,2,1) + 2 = 9");
    }

    #[test]
    fn display_renders_negated_compound() {
        let roll = DiceRoll::parse("1d20-1d4").unwrap();
        let outcome = RollOutcome {
            total: 10,
            dice: vec![
                RolledDie { kind: DieKind::D20, value: 12, negate: false },
                RolledDie { kind: DieKind::D4, value: 2, negate: true },
            ],
            term_lengths: vec![1, 1],
        };
        assert_eq!(outcome.display(&roll).to_string(), "1d20(12) - 1d4(2) = 10");
    }
    #[test]
    fn malformed_term_ranges_return_empty_without_panicking() {
        let die = RolledDie { kind: DieKind::D6, value: 1, negate: false };
        for lengths in [vec![1, 1], vec![u32::MAX, u32::MAX, 1], vec![0, 5], vec![]] {
            for dice in [vec![], vec![die.clone()]] {
                let outcome = RollOutcome { total: 0, dice, term_lengths: lengths.clone() };
                for term_index in [0, 1, 2, usize::MAX] {
                    let values = outcome.term_dice(term_index);
                    assert!(values.len() <= outcome.dice.len());
                }
                assert!(outcome.term_dice(usize::MAX).is_empty());
            }
        }
        let outcome = RollOutcome { total: 1, dice: vec![die], term_lengths: vec![2] };
        assert!(outcome.term_dice(0).is_empty());
        let roll = DiceRoll::parse("2d6").expect("roll");
        assert_eq!(outcome.display(&roll).to_string(), "2d6() = 1");
    }

    #[test]
    fn hand_built_minimum_adjustment_formats_without_overflow() {
        let mut roll = DiceRoll::parse("1d6").expect("roll");
        roll.adjustment = i32::MIN;
        let outcome = RollOutcome {
            total: i32::MIN + 1,
            dice: vec![RolledDie { kind: DieKind::D6, value: 1, negate: false }],
            term_lengths: vec![1],
        };
        assert_eq!(outcome.display(&roll).to_string(), "1d6(1) - 2147483648 = -2147483647");
    }
}
