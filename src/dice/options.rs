use bevy::reflect::Reflect;
use rand::Rng;

use crate::dice::kind::DieKind;

/// Shared per-call cap on reroll-replacements + explode-additions.
pub const MAX_OPTION_ITERATIONS: u32 = 100;

/// Per-term roll modifiers, applied in order: reroll, explode, keep.
#[derive(Clone, Debug, Default, Eq, PartialEq, Reflect)]
pub struct Options {
    /// Keep the top `n` dice after rolling, discarding the rest.
    pub keep_highest: Option<u32>,
    /// Keep the bottom `n` dice after rolling, discarding the rest.
    pub keep_lowest: Option<u32>,
    /// Reroll any die showing this value or lower until it exceeds it.
    pub reroll_at_or_below: Option<u32>,
    /// Spawn an extra die whenever a roll meets or exceeds this value.
    pub explode_at_or_above: Option<u32>,
}

/// Settings that would loop forever or never trigger for a given [`DieKind`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionsError {
    /// `keep_highest` or `keep_lowest` was set to zero.
    KeepZero,
    /// `reroll_at_or_below` >= the die's max face, so every roll loops forever.
    RerollNotBelowMax { reroll: u32, sides: u32 },
    /// `explode_at_or_above` <= 1, so every roll explodes forever.
    ExplodeNotAboveMin { explode: u32 },
}

impl Options {
    /// Sets [`keep_highest`](Self::keep_highest).
    pub fn with_keep_highest(mut self, count: u32) -> Self {
        self.keep_highest = Some(count);
        self
    }

    /// Sets [`keep_lowest`](Self::keep_lowest).
    pub fn with_keep_lowest(mut self, count: u32) -> Self {
        self.keep_lowest = Some(count);
        self
    }

    /// Sets [`reroll_at_or_below`](Self::reroll_at_or_below).
    pub fn with_reroll_at_or_below(mut self, threshold: u32) -> Self {
        self.reroll_at_or_below = Some(threshold);
        self
    }

    /// Sets [`explode_at_or_above`](Self::explode_at_or_above).
    pub fn with_explode_at_or_above(mut self, threshold: u32) -> Self {
        self.explode_at_or_above = Some(threshold);
        self
    }

    /// Rejects settings that would loop forever or never trigger for `kind`.
    pub fn validate_for(&self, kind: DieKind) -> Result<(), OptionsError> {
        if matches!(self.keep_highest, Some(0)) || matches!(self.keep_lowest, Some(0)) {
            return Err(OptionsError::KeepZero);
        }
        if let Some(threshold) = self.reroll_at_or_below
            && threshold >= kind.sides()
        {
            return Err(OptionsError::RerollNotBelowMax { reroll: threshold, sides: kind.sides() });
        }
        if let Some(threshold) = self.explode_at_or_above
            && threshold <= 1
        {
            return Err(OptionsError::ExplodeNotAboveMin { explode: threshold });
        }
        Ok(())
    }

    /// What a die showing `value` asks for; reroll wins over explode, and a die explodes once.
    /// Thresholds outside the die's range never trigger.
    pub fn modifier_for(&self, kind: DieKind, value: u32, exploded: bool) -> Option<Modifier> {
        if let Some(threshold) = self.reroll_at_or_below
            && threshold < kind.sides()
            && value <= threshold
        {
            return Some(Modifier::Reroll);
        }
        if let Some(threshold) = self.explode_at_or_above
            && threshold <= kind.sides()
            && !exploded
            && value >= threshold
        {
            return Some(Modifier::Explode);
        }
        None
    }

    /// Runs reroll + explode (sharing one [`MAX_OPTION_ITERATIONS`] budget) then keep.
    /// Exploded dice are checked too, so the physics path yields the same distribution.
    pub fn apply<R: Rng + ?Sized>(&self, kind: DieKind, rolls: &mut Vec<u32>, rng: &mut R) {
        let mut budget = MAX_OPTION_ITERATIONS;
        let mut idx = 0;
        while idx < rolls.len() {
            let mut exploded = false;
            while budget > 0 {
                match self.modifier_for(kind, rolls[idx], exploded) {
                    Some(Modifier::Reroll) => rolls[idx] = kind.roll(rng),
                    Some(Modifier::Explode) => {
                        rolls.push(kind.roll(rng));
                        exploded = true;
                    }
                    None => break,
                }
                budget -= 1;
            }
            idx += 1;
        }
        self.apply_keep(rolls);
    }

    /// RNG-free trim of `rolls` in place; the physics layer reuses this.
    pub fn apply_keep(&self, rolls: &mut Vec<u32>) {
        if let Some(count) = self.keep_highest {
            keep(rolls, count, true);
        }
        if let Some(count) = self.keep_lowest {
            keep(rolls, count, false);
        }
    }
}

/// Action a settled die value requests under [`Options::modifier_for`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Modifier {
    /// Replace the die with a fresh roll.
    Reroll,
    /// Add one extra die to the term.
    Explode,
}

/// Sorts `rolls` (descending when `highest`) and truncates to `count`.
fn keep(rolls: &mut Vec<u32>, count: u32, highest: bool) {
    let count = count as usize;
    if count >= rolls.len() {
        return;
    }
    if highest {
        rolls.sort_unstable_by(|a, b| b.cmp(a));
    } else {
        rolls.sort_unstable();
    }
    rolls.truncate(count);
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
    fn default_apply_is_identity() {
        let mut rolls = vec![1, 2, 3, 4, 5];
        Options::default().apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn apply_keep_only_trims_without_rng() {
        let mut rolls = vec![3, 1, 5, 4, 2];
        Options::default().with_keep_highest(2).apply_keep(&mut rolls);
        assert_eq!(rolls, vec![5, 4]);
    }

    #[test]
    fn keep_highest_trims_to_top_count() {
        let mut rolls = vec![3, 1, 5, 4, 2];
        Options::default().with_keep_highest(2).apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls, vec![5, 4]);
    }

    #[test]
    fn keep_lowest_trims_to_bottom_count() {
        let mut rolls = vec![3, 1, 5, 4, 2];
        Options::default().with_keep_lowest(2).apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls, vec![1, 2]);
    }

    #[test]
    fn keep_count_at_or_above_len_is_noop() {
        let mut rolls = vec![3, 1, 5];
        Options::default().with_keep_highest(5).apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls.len(), 3);
    }

    #[test]
    fn reroll_replaces_low_values_until_above_threshold() {
        let mut rolls = vec![1, 1, 6];
        Options::default().with_reroll_at_or_below(1).apply(DieKind::D6, &mut rolls, &mut rng());
        assert!(rolls.iter().all(|&r| r > 1));
        assert_eq!(rolls.len(), 3);
    }

    #[test]
    fn explode_adds_dice_for_max_rolls() {
        let mut rolls = vec![6, 3, 6];
        let before = rolls.len();
        Options::default().with_explode_at_or_above(6).apply(DieKind::D6, &mut rolls, &mut rng());
        assert!(rolls.len() >= before + 2);
    }

    #[test]
    fn reroll_also_applies_to_exploded_dice() {
        let mut rolls = vec![6, 6, 6, 6];
        Options::default().with_reroll_at_or_below(3).with_explode_at_or_above(6).apply(
            DieKind::D6,
            &mut rolls,
            &mut rng(),
        );
        assert!(rolls.len() > 4);
        assert!(rolls.iter().all(|&r| r > 3), "{rolls:?}");
    }

    #[test]
    fn modifier_for_prefers_reroll_and_explodes_once() {
        let options = Options::default().with_reroll_at_or_below(5).with_explode_at_or_above(5);
        assert_eq!(options.modifier_for(DieKind::D6, 5, false), Some(Modifier::Reroll));
        assert_eq!(options.modifier_for(DieKind::D6, 5, true), Some(Modifier::Reroll));
        assert_eq!(options.modifier_for(DieKind::D6, 6, false), Some(Modifier::Explode));
        assert_eq!(Options::default().with_reroll_at_or_below(6).modifier_for(DieKind::D6, 6, false), None);
        let options = Options::default().with_explode_at_or_above(5);
        assert_eq!(options.modifier_for(DieKind::D6, 5, false), Some(Modifier::Explode));
        assert_eq!(options.modifier_for(DieKind::D6, 5, true), None);
        assert_eq!(options.modifier_for(DieKind::D4, 4, false), None);
    }

    #[test]
    fn iterations_capped_to_max() {
        let original_count = (MAX_OPTION_ITERATIONS as usize) * 2;
        let mut rolls = vec![1; original_count];
        Options::default().with_explode_at_or_above(1).apply(DieKind::D6, &mut rolls, &mut rng());
        let added = rolls.len() - original_count;
        assert_eq!(added, MAX_OPTION_ITERATIONS as usize);
    }

    #[test]
    fn validate_rejects_keep_zero() {
        let result = Options::default().with_keep_highest(0).validate_for(DieKind::D6);
        assert_eq!(result, Err(OptionsError::KeepZero));
    }

    #[test]
    fn validate_rejects_reroll_at_or_above_sides() {
        let result = Options::default().with_reroll_at_or_below(6).validate_for(DieKind::D6);
        assert!(matches!(result, Err(OptionsError::RerollNotBelowMax { .. })));
    }

    #[test]
    fn validate_accepts_sensible_settings() {
        let result = Options::default()
            .with_keep_highest(1)
            .with_reroll_at_or_below(1)
            .with_explode_at_or_above(6)
            .validate_for(DieKind::D6);
        assert_eq!(result, Ok(()));
    }
}
