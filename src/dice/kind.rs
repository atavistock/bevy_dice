use std::fmt;

use rand::Rng;

/// One of the standard polyhedral dice. d100 is the percentile die; the
/// visual layer renders it as a d100 tens + d10 ones pair, but its `roll`
/// returns one number in 1..=100.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum DieKind {
    D4,
    D6,
    D8,
    D10,
    D12,
    D20,
    D100,
}

impl DieKind {
    /// All supported kinds in canonical order; the index matches the gltf
    /// primitive order written by the asset generator.
    pub const ALL: [DieKind; 7] = [
        DieKind::D4,
        DieKind::D6,
        DieKind::D8,
        DieKind::D10,
        DieKind::D12,
        DieKind::D20,
        DieKind::D100,
    ];

    pub fn sides(self) -> u32 {
        match self {
            DieKind::D4 => 4,
            DieKind::D6 => 6,
            DieKind::D8 => 8,
            DieKind::D10 => 10,
            DieKind::D12 => 12,
            DieKind::D20 => 20,
            DieKind::D100 => 100,
        }
    }

    pub fn from_sides(sides: u32) -> Option<DieKind> {
        DieKind::ALL.iter().copied().find(|k| k.sides() == sides)
    }

    pub fn from_asset_name(name: &str) -> Option<DieKind> {
        DieKind::ALL.iter().copied().find(|k| k.asset_name() == name)
    }

    /// gltf basename emitted by the asset generator. Pairs with
    /// `assets/{diceset}/{asset_name}.gltf`.
    pub fn asset_name(self) -> &'static str {
        match self {
            DieKind::D4 => "d4",
            DieKind::D6 => "d6",
            DieKind::D8 => "d8",
            DieKind::D10 => "d10",
            DieKind::D12 => "d12",
            DieKind::D20 => "d20",
            DieKind::D100 => "d100",
        }
    }

    /// Uniform roll in 1..=sides(). D100 returns the full 1-100 percentile;
    /// the runtime renders it as a d100+d10 pair but the math is one number.
    pub fn roll<R: Rng + ?Sized>(self, rng: &mut R) -> u32 {
        rng.gen_range(1..=self.sides())
    }

    /// Sphere-proxy radius sized to each polyhedron's visual extent. Scaled
    /// globally by `DicePhysicsConfig::radius_scale`.
    pub fn collision_radius(self) -> f32 {
        match self {
            DieKind::D4 => 0.55,
            DieKind::D6 => 0.65,
            DieKind::D8 => 0.65,
            DieKind::D10 => 0.75,
            DieKind::D12 => 0.85,
            DieKind::D20 => 0.85,
            DieKind::D100 => 0.75,
        }
    }
}

impl fmt::Display for DieKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "d{}", self.sides())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn sides_match_canonical_values() {
        let expected = [4, 6, 8, 10, 12, 20, 100];
        for (kind, sides) in DieKind::ALL.iter().zip(expected) {
            assert_eq!(kind.sides(), sides);
        }
    }

    #[test]
    fn from_sides_round_trips() {
        for kind in DieKind::ALL {
            assert_eq!(DieKind::from_sides(kind.sides()), Some(kind));
        }
        assert_eq!(DieKind::from_sides(7), None);
        assert_eq!(DieKind::from_sides(0), None);
    }

    #[test]
    fn roll_stays_within_bounds() {
        let mut rng = StdRng::seed_from_u64(0xD1CE);
        for kind in DieKind::ALL {
            for _ in 0..1000 {
                let value = kind.roll(&mut rng);
                assert!(value >= 1 && value <= kind.sides(), "{kind} produced {value}");
            }
        }
    }

    #[test]
    fn display_matches_asset_name() {
        for kind in DieKind::ALL {
            assert_eq!(format!("{kind}"), kind.asset_name());
        }
    }
}
