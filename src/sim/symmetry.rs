//! Geometry-preserving rotations for selecting the visible result of a recorded throw.

use std::fmt;

use bevy::math::{Mat3, Quat, Vec3};

use crate::dice::DieKind;

use super::orientations::DiceOrientations;

const SYMMETRY_TOLERANCE: f32 = 1e-5;

/// A mesh or orientation table cannot support arbitrary forced results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymmetryError {
    MissingMetadata,
    InvalidMetadata,
    InvalidGeometry,
    AsymmetricGeometry,
}

impl fmt::Display for SymmetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingMetadata => "dice face orientations are missing",
            Self::InvalidMetadata => "dice face orientations are incomplete or invalid",
            Self::InvalidGeometry => "dice mesh vertices are empty or invalid",
            Self::AsymmetricGeometry => "dice mesh does not support every face permutation by rotation",
        })
    }
}

impl std::error::Error for SymmetryError {}

/// Validated local rotations indexed by requested and settled face directions.
#[derive(Clone, Debug)]
pub struct FaceSymmetries {
    faces: Vec<(String, Vec3)>,
    corrections: Vec<Quat>,
}

impl FaceSymmetries {
    /// Validate geometry and cache a proper rotation for every pair of face labels.
    pub fn new(kind: DieKind, orientations: &DiceOrientations, vertices: &[Vec3]) -> Result<Self, SymmetryError> {
        let faces = orientations.faces(kind);
        if faces.is_empty() {
            return Err(SymmetryError::MissingMetadata);
        }
        let expected_faces = if kind == DieKind::D100 { 10 } else { kind.sides() as usize };
        if faces.len() != expected_faces
            || faces.iter().any(|(_, direction)| !direction.is_finite() || !direction.is_normalized())
        {
            return Err(SymmetryError::InvalidMetadata);
        }
        let directions: Vec<Vec3> = faces.iter().map(|(_, direction)| *direction).collect();
        let tolerance_squared = SYMMETRY_TOLERANCE * SYMMETRY_TOLERANCE;
        for (index, direction) in directions.iter().enumerate() {
            if directions[..index].iter().any(|other| direction.distance_squared(*other) <= tolerance_squared) {
                return Err(SymmetryError::InvalidMetadata);
            }
        }
        if vertices.len() < 4 || vertices.iter().any(|vertex| !vertex.is_finite()) {
            return Err(SymmetryError::InvalidGeometry);
        }
        let radius = vertices.iter().map(|vertex| vertex.length()).fold(0.0_f32, f32::max);
        if !radius.is_finite() || radius <= 0.0 {
            return Err(SymmetryError::InvalidGeometry);
        }
        let mut positions: Vec<Vec3> = vertices.iter().map(|vertex| *vertex / radius).collect();
        positions.sort_unstable_by(|first, second| {
            first.x.total_cmp(&second.x).then(first.y.total_cmp(&second.y)).then(first.z.total_cmp(&second.z))
        });
        positions.dedup();
        if positions.len() < 4 {
            return Err(SymmetryError::InvalidGeometry);
        }
        let reference = directions[0];
        let Some(second) = directions.iter().copied().find(|direction| reference.dot(*direction).abs() < 0.99) else {
            return Err(SymmetryError::InvalidMetadata);
        };
        let reference_frame = direction_frame(reference, second);
        let reference_dot = reference.dot(second);
        let mut corrections = vec![None; faces.len() * faces.len()];
        for target in directions.iter().copied() {
            for companion in directions.iter().copied() {
                if (target.dot(companion) - reference_dot).abs() > SYMMETRY_TOLERANCE {
                    continue;
                }
                let rotation =
                    Quat::from_mat3(&(direction_frame(target, companion) * reference_frame.transpose())).normalize();
                if !preserves_positions(rotation, &directions, tolerance_squared)
                    || !preserves_positions(rotation, &positions, tolerance_squared)
                {
                    continue;
                }
                for (source_index, source) in directions.iter().enumerate() {
                    let rotated = rotation * *source;
                    for (target_index, target) in directions.iter().enumerate() {
                        if rotated.distance_squared(*target) <= tolerance_squared {
                            corrections[source_index * faces.len() + target_index].get_or_insert(rotation);
                        }
                    }
                }
            }
        }
        let corrections =
            corrections.into_iter().collect::<Option<Vec<_>>>().ok_or(SymmetryError::AsymmetricGeometry)?;
        Ok(Self { faces: faces.to_vec(), corrections })
    }

    /// Return the local correction used as `recorded_rotation * correction` during playback.
    pub fn correction(&self, desired_label: &str, settled_rotation: Quat) -> Option<Quat> {
        if !settled_rotation.is_finite() || !settled_rotation.is_normalized() {
            return None;
        }
        let source_index = self.faces.iter().position(|(label, _)| label == desired_label)?;
        let mut target_index = 0;
        let mut highest = f32::NEG_INFINITY;
        for (index, (_, direction)) in self.faces.iter().enumerate() {
            let height = (settled_rotation * *direction).y;
            if height > highest {
                highest = height;
                target_index = index;
            }
        }
        Some(self.corrections[source_index * self.faces.len() + target_index])
    }
}

fn direction_frame(first: Vec3, second: Vec3) -> Mat3 {
    let tangent = (second - first.dot(second) * first).normalize();
    Mat3::from_cols(first, tangent, first.cross(tangent))
}

fn preserves_positions(rotation: Quat, positions: &[Vec3], tolerance_squared: f32) -> bool {
    positions.iter().all(|position| {
        let rotated = rotation * *position;
        positions.iter().any(|candidate| rotated.distance_squared(*candidate) <= tolerance_squared)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::load_orientations_from_bytes;

    fn cube() -> (DiceOrientations, Vec<Vec3>) {
        let orientations = load_orientations_from_bytes(
            br#"{"extras":{"dice_orientations":{"d6":{
            "1":[1,0,0],"2":[-1,0,0],"3":[0,1,0],"4":[0,-1,0],"5":[0,0,1],"6":[0,0,-1]
        }}}}"#,
        );
        let mut vertices = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    vertices.push(Vec3::new(x, y, z));
                }
            }
        }
        (orientations, vertices)
    }

    fn check_all_pairs(kind: DieKind, orientations: &DiceOrientations, vertices: &[Vec3]) {
        let symmetries = FaceSymmetries::new(kind, orientations, vertices).unwrap();
        for (_, settled_direction) in orientations.faces(kind) {
            let settled_rotation = Quat::from_rotation_arc(*settled_direction, Vec3::Y);
            for (desired_label, _) in orientations.faces(kind) {
                let correction = symmetries.correction(desired_label, settled_rotation).unwrap();
                assert_eq!(orientations.up_face(kind, settled_rotation * correction), Some(desired_label.as_str()));
                assert!(preserves_positions(correction, vertices, 1e-8));
            }
        }
        assert!(symmetries.correction("unknown", Quat::IDENTITY).is_none());
    }

    #[test]
    fn cube_supports_every_pair_including_opposite_faces() {
        let (orientations, vertices) = cube();
        check_all_pairs(DieKind::D6, &orientations, &vertices);
    }

    #[test]
    fn asymmetric_mesh_is_rejected() {
        let (orientations, mut vertices) = cube();
        vertices[0].x -= 0.2;
        assert_eq!(
            FaceSymmetries::new(DieKind::D6, &orientations, &vertices).unwrap_err(),
            SymmetryError::AsymmetricGeometry
        );
    }

    #[test]
    fn missing_metadata_and_invalid_geometry_are_rejected() {
        let (orientations, vertices) = cube();
        assert_eq!(
            FaceSymmetries::new(DieKind::D4, &orientations, &vertices).unwrap_err(),
            SymmetryError::MissingMetadata
        );
        assert_eq!(FaceSymmetries::new(DieKind::D6, &orientations, &[]).unwrap_err(), SymmetryError::InvalidGeometry);
    }

    #[test]
    fn bundled_dice_support_forced_trajectories() {
        let bytes = include_bytes!("../../assets/plain_white_diceset.glb");
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let document: serde_json::Value = serde_json::from_slice(&bytes[20..20 + json_length]).unwrap();
        let binary = &bytes[28 + json_length..];
        let orientations = load_orientations_from_bytes(bytes);
        for kind in DieKind::ALL {
            let primitive = &document["meshes"][kind.mesh_index()]["primitives"][0];
            let accessor = &document["accessors"][primitive["attributes"]["POSITION"].as_u64().unwrap() as usize];
            assert_eq!(accessor["componentType"], 5126);
            assert_eq!(accessor["type"], "VEC3");
            let view = &document["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
            let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize
                + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
            let stride = view["byteStride"].as_u64().unwrap_or(12) as usize;
            let vertices: Vec<Vec3> = (0..accessor["count"].as_u64().unwrap() as usize)
                .map(|index| {
                    let components = std::array::from_fn(|component| {
                        let start = offset + index * stride + component * 4;
                        f32::from_le_bytes(binary[start..start + 4].try_into().unwrap())
                    });
                    Vec3::from_array(components)
                })
                .collect();
            check_all_pairs(kind, &orientations, &vertices);
            let collider = avian3d::prelude::Collider::convex_hull(vertices).expect("bundled hull");
            for count in if kind == DieKind::D20 { vec![1, 4, 20] } else { vec![1] } {
                let recording = crate::sim::simulate_throw(crate::sim::SimulationInput {
                    arena: crate::sim::SimulationArena {
                        size: Vec3::new(12.0, 5.0, if count > 4 { 10.0 } else { 6.0 }),
                        ..Default::default()
                    },
                    kinds: vec![kind; count],
                    colliders: vec![collider.clone(); count],
                    orientations: orientations.clone(),
                    gravity: Vec3::NEG_Y * 23.1,
                    seed: 1234,
                })
                .unwrap_or_else(|err| panic!("{count} {kind} simulation failed: {err}"));
                assert_eq!(recording.kinds.len(), count);
            }
        }
    }
}
