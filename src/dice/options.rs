use rand::Rng;

use crate::dice::kind::DieKind;

/// Shared budget across reroll-replacements and explode-additions per
/// [`Options::apply`] call. Prevents pathological configs from looping or ballooning.
pub const MAX_OPTION_ITERATIONS: u32 = 100;

/// Per-term roll modifiers. All fields default to `None` (no modification).
/// Build with the `with_*` setters; apply order is reroll, explode, then the
/// keep filters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Options {
    pub keep_highest: Option<u32>,
    pub keep_lowest: Option<u32>,
    pub reroll_at_or_below: Option<u32>,
    pub explode_at_or_above: Option<u32>,
}

/// Failures from [`Options::validate_for`]. Settings that would loop forever
/// or never trigger for the target [`DieKind`] are rejected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionsError {
    KeepZero,
    RerollNotBelowMax { reroll: u32, sides: u32 },
    ExplodeNotAboveMin { explode: u32 },
}

impl Options {
    pub fn with_keep_highest(mut self, count: u32) -> Self {
        self.keep_highest = Some(count);
        self
    }

    pub fn with_keep_lowest(mut self, count: u32) -> Self {
        self.keep_lowest = Some(count);
        self
    }

    pub fn with_reroll_at_or_below(mut self, threshold: u32) -> Self {
        self.reroll_at_or_below = Some(threshold);
        self
    }

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

    /// Applies reroll, explode, then keep-highest/keep-lowest. Reroll and explode
    /// share one [`MAX_OPTION_ITERATIONS`] budget; both keep options compose.
    pub fn apply<R: Rng + ?Sized>(&self, kind: DieKind, rolls: &mut Vec<u32>, rng: &mut R) {
        let mut budget = MAX_OPTION_ITERATIONS;
        self.apply_reroll(kind, rolls, rng, &mut budget);
        self.apply_explode(kind, rolls, rng, &mut budget);
        self.apply_keep_highest(rolls);
        self.apply_keep_lowest(rolls);
    }

    fn apply_reroll<R: Rng + ?Sized>(
        &self,
        kind: DieKind,
        rolls: &mut Vec<u32>,
        rng: &mut R,
        budget: &mut u32,
    ) {
        let Some(threshold) = self.reroll_at_or_below else {
            return;
        };
        if threshold >= kind.sides() {
            return;
        }
        for roll in rolls.iter_mut() {
            while *roll <= threshold && *budget > 0 {
                *roll = kind.roll(rng);
                *budget -= 1;
            }
        }
    }

    fn apply_explode<R: Rng + ?Sized>(
        &self,
        kind: DieKind,
        rolls: &mut Vec<u32>,
        rng: &mut R,
        budget: &mut u32,
    ) {
        let Some(threshold) = self.explode_at_or_above else {
            return;
        };
        if threshold > kind.sides() {
            return;
        }
        let mut idx = 0;
        while idx < rolls.len() {
            if rolls[idx] >= threshold && *budget > 0 {
                let extra = kind.roll(rng);
                rolls.push(extra);
                *budget -= 1;
            }
            idx += 1;
        }
    }

    fn apply_keep_highest(&self, rolls: &mut Vec<u32>) {
        let Some(count) = self.keep_highest else {
            return;
        };
        let count = count as usize;
        if count >= rolls.len() {
            return;
        }
        rolls.sort_unstable_by(|a, b| b.cmp(a));
        rolls.truncate(count);
    }

    fn apply_keep_lowest(&self, rolls: &mut Vec<u32>) {
        let Some(count) = self.keep_lowest else {
            return;
        };
        let count = count as usize;
        if count >= rolls.len() {
            return;
        }
        rolls.sort_unstable();
        rolls.truncate(count);
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
    fn default_apply_is_identity() {
        let mut rolls = vec![1, 2, 3, 4, 5];
        Options::default().apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn keep_highest_trims_to_top_count() {
        let mut rolls = vec![3, 1, 5, 4, 2];
        Options::default()
            .with_keep_highest(2)
            .apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls, vec![5, 4]);
    }

    #[test]
    fn keep_lowest_trims_to_bottom_count() {
        let mut rolls = vec![3, 1, 5, 4, 2];
        Options::default()
            .with_keep_lowest(2)
            .apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls, vec![1, 2]);
    }

    #[test]
    fn keep_count_at_or_above_len_is_noop() {
        let mut rolls = vec![3, 1, 5];
        Options::default()
            .with_keep_highest(5)
            .apply(DieKind::D6, &mut rolls, &mut rng());
        assert_eq!(rolls.len(), 3);
    }

    #[test]
    fn reroll_replaces_low_values_until_above_threshold() {
        let mut rolls = vec![1, 1, 6];
        Options::default()
            .with_reroll_at_or_below(1)
            .apply(DieKind::D6, &mut rolls, &mut rng());
        assert!(rolls.iter().all(|&r| r > 1));
        assert_eq!(rolls.len(), 3);
    }

    #[test]
    fn explode_adds_dice_for_max_rolls() {
        let mut rolls = vec![6, 3, 6];
        let before = rolls.len();
        Options::default()
            .with_explode_at_or_above(6)
            .apply(DieKind::D6, &mut rolls, &mut rng());
        assert!(rolls.len() >= before + 2);
    }

    #[test]
    fn iterations_capped_to_max() {
        let original_count = (MAX_OPTION_ITERATIONS as usize) * 2;
        let mut rolls = vec![1; original_count];
        Options::default()
            .with_explode_at_or_above(1)
            .apply(DieKind::D6, &mut rolls, &mut rng());
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
        let result = Options::default()
            .with_reroll_at_or_below(6)
            .validate_for(DieKind::D6);
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
