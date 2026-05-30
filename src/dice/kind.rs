use std::fmt;

use bevy::reflect::Reflect;
use rand::Rng;

/// Standard polyhedral die. d100 rolls 1..=100; the visual layer renders
/// it as a d100 + d10 pair.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, Reflect)]
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
    /// All kinds in canonical order; index matches the gltf primitive order.
    pub const ALL: [DieKind; 7] = [
        DieKind::D4,
        DieKind::D6,
        DieKind::D8,
        DieKind::D10,
        DieKind::D12,
        DieKind::D20,
        DieKind::D100,
    ];

    /// Number of faces on the die.
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

    /// Looks up a kind by face count. Returns `None` for non-standard counts.
    pub fn from_sides(sides: u32) -> Option<DieKind> {
        DieKind::ALL.iter().copied().find(|k| k.sides() == sides)
    }

    /// Looks up a kind by gltf asset name (`"d4"`, `"d6"`, ...).
    pub fn from_asset_name(name: &str) -> Option<DieKind> {
        DieKind::ALL.iter().copied().find(|k| k.asset_name() == name)
    }

    /// Index into [`DieKind::ALL`] / the diceset gltf's primitive order.
    pub fn mesh_index(self) -> usize {
        match self {
            DieKind::D4 => 0,
            DieKind::D6 => 1,
            DieKind::D8 => 2,
            DieKind::D10 => 3,
            DieKind::D12 => 4,
            DieKind::D20 => 5,
            DieKind::D100 => 6,
        }
    }

    /// gltf basename, paired with `assets/{diceset}/{asset_name}.gltf`.
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

    /// Uniform roll in `1..=sides()`. D100 returns 1..=100 as one number.
    pub fn roll<R: Rng + ?Sized>(self, rng: &mut R) -> u32 {
        rng.gen_range(1..=self.sides())
    }

    /// Sphere-proxy radius for collision, sized to the polyhedron's visual extent.
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

    #[test]
    fn mesh_index_matches_all_order() {
        for (expected, kind) in DieKind::ALL.iter().enumerate() {
            assert_eq!(kind.mesh_index(), expected);
        }
    }
}
