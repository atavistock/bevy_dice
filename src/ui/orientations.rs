//! Per-kind face direction table loaded from a diceset gltf.

use std::collections::HashMap;
use std::path::Path;

use bevy::log::warn;
use bevy::math::{Quat, Vec3};

use crate::dice::DieKind;

/// Per-kind face directions (local space), keyed by face label.
#[derive(Default, Clone)]
pub struct DiceOrientations {
    by_kind: HashMap<DieKind, Vec<(String, Vec3)>>,
}

impl DiceOrientations {
    /// All `(label, local-direction)` pairs for `kind`.
    pub fn faces(&self, kind: DieKind) -> &[(String, Vec3)] {
        self.by_kind.get(&kind).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Label of the face that points most toward +Y after `rotation`.
    pub fn up_face(&self, kind: DieKind, rotation: Quat) -> Option<&str> {
        let faces = self.by_kind.get(&kind)?;
        let mut best: Option<(f32, &str)> = None;
        for (label, direction) in faces {
            let world_y = (rotation * *direction).y;
            if best.is_none_or(|(best_y, _)| world_y > best_y) {
                best = Some((world_y, label.as_str()));
            }
        }
        best.map(|(_, label)| label)
    }
}

/// Load orientations from a gltf file on disk.
pub fn load_orientations(path: impl AsRef<Path>) -> DiceOrientations {
    let path = path.as_ref();
    let Ok(bytes) = std::fs::read(path) else {
        warn!("could not read {}; dice will roll without face mapping", path.display());
        return DiceOrientations::default();
    };
    load_orientations_from_bytes(&bytes)
}

/// Load orientations from in-memory gltf bytes. Accepts either a `.gltf`
/// text JSON or a `.glb` binary container.
pub fn load_orientations_from_bytes(bytes: &[u8]) -> DiceOrientations {
    let json_bytes = match extract_gltf_json(bytes) {
        Some(slice) => slice,
        None => bytes,
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(json_bytes) else {
        warn!("could not parse gltf bytes");
        return DiceOrientations::default();
    };
    let Some(table) = json
        .get("extras")
        .and_then(|e| e.get("dice_orientations"))
        .and_then(|d| d.as_object())
    else {
        return DiceOrientations::default();
    };
    let mut by_kind: HashMap<DieKind, Vec<(String, Vec3)>> = HashMap::new();
    for (die_name, faces) in table {
        let Some(kind) = DieKind::from_asset_name(die_name) else { continue };
        let Some(faces) = faces.as_object() else { continue };
        for (label, value) in faces {
            let Some(array) = value.as_array() else { continue };
            if array.len() != 3 {
                continue;
            }
            let x = array[0].as_f64().unwrap_or(0.0) as f32;
            let y = array[1].as_f64().unwrap_or(0.0) as f32;
            let z = array[2].as_f64().unwrap_or(0.0) as f32;
            let raw = Vec3::new(x, y, z);
            if raw.length_squared() < 1e-6 {
                continue;
            }
            by_kind.entry(kind).or_default().push((label.clone(), raw.normalize()));
        }
    }
    DiceOrientations { by_kind }
}

fn extract_gltf_json(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return None;
    }
    let chunk_length = u32::from_le_bytes(bytes[12..16].try_into().ok()?) as usize;
    let chunk_type = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
    if chunk_type != 0x4E4F_534A {
        return None;
    }
    let start = 20usize;
    let end = start.checked_add(chunk_length)?;
    bytes.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLTF_JSON: &str = r#"{
        "extras": {
            "dice_orientations": {
                "d6": { "1": [0.0, 1.0, 0.0], "6": [0.0, -1.0, 0.0] },
                "d20": { "20": [0.0, 1.0, 0.0] }
            }
        }
    }"#;

    #[test]
    fn parses_raw_gltf_json() {
        let orientations = load_orientations_from_bytes(GLTF_JSON.as_bytes());
        assert_eq!(orientations.faces(DieKind::D6).len(), 2);
        assert_eq!(orientations.faces(DieKind::D20).len(), 1);
    }

    #[test]
    fn directions_are_normalized() {
        let json = r#"{"extras":{"dice_orientations":{"d6":{"1":[0.0,5.0,0.0]}}}}"#;
        let orientations = load_orientations_from_bytes(json.as_bytes());
        let (_, direction) = &orientations.faces(DieKind::D6)[0];
        assert!((direction.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn unknown_die_names_are_skipped() {
        let json = r#"{"extras":{"dice_orientations":{"d99":{"1":[0.0,1.0,0.0]}}}}"#;
        let orientations = load_orientations_from_bytes(json.as_bytes());
        assert!(orientations.faces(DieKind::D6).is_empty());
    }

    #[test]
    fn missing_extras_returns_empty() {
        let orientations = load_orientations_from_bytes(b"{}");
        assert!(orientations.faces(DieKind::D6).is_empty());
    }

    #[test]
    fn up_face_picks_label_pointing_to_world_y() {
        let orientations = load_orientations_from_bytes(GLTF_JSON.as_bytes());
        let identity = Quat::IDENTITY;
        assert_eq!(orientations.up_face(DieKind::D6, identity), Some("1"));
    }

    #[test]
    fn invalid_bytes_return_empty() {
        let orientations = load_orientations_from_bytes(b"not json or glb");
        assert!(orientations.faces(DieKind::D6).is_empty());
    }
}
