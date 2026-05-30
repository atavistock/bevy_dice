//! [`Diceset`] entity: the gltf asset path, face orientations, and cached
//! mesh + material handles. Multiple [`super::DiceArena`]s can share one.

use bevy::prelude::*;

use crate::dice::DieKind;

use super::orientations::{load_orientations, load_orientations_from_bytes, DiceOrientations};

/// Single source of truth for embedded dicesets. Each entry is
/// `(feature_name, gltf_bytes)`; add a row to include a new embedded diceset.
pub(super) fn embedded_table() -> &'static [(&'static str, &'static [u8])] {
    &[
        #[cfg(feature = "plain_white")]
        ("plain_white", include_bytes!("../../assets/plain_white_diceset.glb")),
        #[cfg(feature = "halloween")]
        ("halloween", include_bytes!("../../assets/halloween_diceset.glb")),
        #[cfg(feature = "metal")]
        ("metal", include_bytes!("../../assets/metal_diceset.glb")),
        #[cfg(feature = "clear_orange")]
        ("clear_orange", include_bytes!("../../assets/clear_orange_diceset.glb")),
    ]
}

/// A diceset: gltf asset path, face orientations, and cached mesh + material
/// handles. Spawn one entity per distinct diceset; many [`super::DiceArena`]s
/// can reference the same one.
#[derive(Component, Clone, Reflect)]
#[reflect(Component)]
pub struct Diceset {
    /// Bevy asset-server path for the gltf containing the diceset meshes.
    pub asset_path: String,
    /// Per-face direction table, loaded from the gltf's `extras.dice_orientations`.
    #[reflect(ignore)]
    pub orientations: DiceOrientations,
    /// Cached mesh + material handles, populated by [`load_diceset_handles`].
    #[reflect(ignore)]
    pub(super) handles: Option<GltfAssetHandles>,
}

impl Diceset {
    /// Loads an embedded diceset by feature name (`"plain_white"`,
    /// `"halloween"`, `"metal"`, `"clear_orange"`). Panics if the matching
    /// cargo feature isn't enabled.
    pub fn embedded(slug: &str) -> Self {
        let (name, bytes) = embedded_table()
            .iter()
            .find(|(name, _)| *name == slug)
            .copied()
            .unwrap_or_else(|| panic!("embedded diceset '{slug}' not available - enable the matching cargo feature"));
        Diceset {
            asset_path: format!("embedded://bevy_dice/{name}_diceset.glb"),
            orientations: load_orientations_from_bytes(bytes),
            handles: None,
        }
    }

    /// Diceset whose gltf lives at `assets/{path}`; orientations come from
    /// the same file. Appends `.glb` if `path` has no extension.
    pub fn custom(path: impl Into<String>) -> Self {
        let mut asset_path = path.into();
        if !asset_path.ends_with(".glb") && !asset_path.ends_with(".gltf") {
            asset_path.push_str(".glb");
        }
        let orientations = load_orientations(format!("assets/{asset_path}"));
        Diceset { asset_path, orientations, handles: None }
    }

    /// Custom diceset with caller-supplied orientations (use when the
    /// orientations file lives outside `assets/`).
    pub fn custom_with(path: impl Into<String>, orientations: DiceOrientations) -> Self {
        let mut asset_path = path.into();
        if !asset_path.ends_with(".glb") && !asset_path.ends_with(".gltf") {
            asset_path.push_str(".glb");
        }
        Diceset { asset_path, orientations, handles: None }
    }

    /// Cached handles for this diceset, populated once [`load_diceset_handles`]
    /// has run. Returns `None` if loading hasn't happened yet.
    pub(super) fn handles(&self) -> Option<&GltfAssetHandles> {
        self.handles.as_ref()
    }
}

/// Mesh + material handles for every [`DieKind`], indexed by [`DieKind::mesh_index`].
#[derive(Clone)]
pub(super) struct GltfAssetHandles {
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
}

impl GltfAssetHandles {
    pub(super) fn mesh(&self, idx: usize) -> Handle<Mesh> {
        self.meshes[idx].clone()
    }

    pub(super) fn material(&self, idx: usize) -> Handle<StandardMaterial> {
        self.materials[idx].clone()
    }
}

/// Populates [`Diceset::handles`] for each newly-spawned [`Diceset`] entity.
pub(super) fn load_diceset_handles(
    mut new_dicesets: Query<&mut Diceset, Added<Diceset>>,
    asset_server: Res<AssetServer>,
) {
    for mut diceset in new_dicesets.iter_mut() {
        let mut meshes = Vec::with_capacity(DieKind::ALL.len());
        let mut materials = Vec::with_capacity(DieKind::ALL.len());
        for mesh_index in 0..DieKind::ALL.len() {
            meshes.push(asset_server.load(
                GltfAssetLabel::Primitive { mesh: mesh_index, primitive: 0 }
                    .from_asset(diceset.asset_path.clone()),
            ));
            materials.push(asset_server.load(
                GltfAssetLabel::Material { index: mesh_index, is_scale_inverted: false }
                    .from_asset(diceset.asset_path.clone()),
            ));
        }
        diceset.handles = Some(GltfAssetHandles { meshes, materials });
    }
}
