//! Headless physics recording, face orientations, and geometry-preserving result corrections.

mod config;
mod orientations;
mod precompute;
mod symmetry;

pub use config::{DicePhysicsConfig, MAX_DICE_PER_ROLL, SimulationArena, SpawnConfig};
pub use orientations::{DiceOrientations, load_orientations, load_orientations_from_bytes};
pub use precompute::{RecordedThrow, SimulationError, SimulationInput, simulate_throw};
pub use symmetry::{FaceSymmetries, SymmetryError};
