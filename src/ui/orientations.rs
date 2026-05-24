//! Per-kind face direction table loaded from a diceset gltf.

use std::collections::HashMap;
use std::path::Path;

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
        eprintln!(
            "warning: could not read {}; dice will roll without face mapping",
            path.display()
        );
        return DiceOrientations::default();
    };
    load_orientations_from_bytes(&bytes)
}

/// Load orientations from in-memory gltf bytes.
pub fn load_orientations_from_bytes(bytes: &[u8]) -> DiceOrientations {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        eprintln!("warning: could not parse gltf bytes");
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
