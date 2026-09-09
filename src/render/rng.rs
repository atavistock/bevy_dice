//! Independent random streams for gameplay outcomes and simulated throws.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

/// Random stream used only to generate gameplay outcomes.
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

/// Independent random stream used only for background simulation seeds.
#[derive(Resource)]
pub struct DiceSimulationRng {
    inner: StdRng,
}

impl DiceSimulationRng {
    /// Seeds simulated trajectories without changing gameplay outcomes.
    pub fn from_seed(seed: u64) -> Self {
        Self { inner: StdRng::seed_from_u64(seed) }
    }

    pub fn next_seed(&mut self) -> u64 {
        self.inner.next_u64()
    }
}

impl Default for DiceSimulationRng {
    fn default() -> Self {
        Self { inner: StdRng::from_entropy() }
    }
}
