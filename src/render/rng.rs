//! [`DiceRng`] resource. Physics systems pull randomness from here so tests
//! can install a seeded RNG for deterministic playback.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

/// Resource owning the RNG used by every physics system (throw arc, stuck-die
/// perturb, reroll/explode replacements). Replace with [`DiceRng::from_seed`]
/// in tests to make rolls reproducible.
#[derive(Resource)]
pub struct DiceRng {
    inner: Box<dyn RngCore + Send + Sync>,
}

impl DiceRng {
    /// Wraps any existing [`RngCore`] implementation.
    pub fn new<R: RngCore + Send + Sync + 'static>(rng: R) -> Self {
        Self { inner: Box::new(rng) }
    }

    /// Deterministic RNG seeded with `seed` (uses [`StdRng`] internally).
    pub fn from_seed(seed: u64) -> Self {
        Self::new(StdRng::seed_from_u64(seed))
    }
}

impl Default for DiceRng {
    fn default() -> Self {
        Self::new(StdRng::from_entropy())
    }
}

impl RngCore for DiceRng {
    fn next_u32(&mut self) -> u32 {
        self.inner.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.inner.fill_bytes(dest);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.inner.try_fill_bytes(dest)
    }
}
