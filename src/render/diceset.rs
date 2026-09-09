//! [`Diceset`] entity: the gltf asset path, face orientations, and cached
//! mesh + material handles. Multiple [`super::DiceArena`]s can share one.

use bevy::asset::AssetPath;
use bevy::prelude::*;

use crate::dice::DieKind;

use crate::sim::{DiceOrientations, load_orientations, load_orientations_from_bytes};

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
        let (name, bytes) =
            embedded_table().iter().find(|(name, _)| *name == slug).copied().unwrap_or_else(|| {
                panic!("embedded diceset '{slug}' not available - enable the matching cargo feature")
            });
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
    asset_path: String,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
}

impl GltfAssetHandles {
    #[cfg(test)]
    pub fn for_test(mesh: Handle<Mesh>, material: Handle<StandardMaterial>) -> Self {
        Self {
            asset_path: String::new(),
            meshes: vec![mesh; DieKind::ALL.len()],
            materials: vec![material; DieKind::ALL.len()],
        }
    }

    pub(super) fn mesh(&self, idx: usize) -> Handle<Mesh> {
        self.meshes[idx].clone()
    }

    pub(super) fn material(&self, idx: usize) -> Handle<StandardMaterial> {
        self.materials[idx].clone()
    }
}

/// Loads handles when a diceset is initialized, replaced, or changes asset paths.
pub(super) fn load_diceset_handles(
    mut dicesets: Query<&mut Diceset, Changed<Diceset>>,
    asset_server: Res<AssetServer>,
) {
    for mut diceset in dicesets.iter_mut() {
        if diceset.handles.as_ref().is_some_and(|handles| handles.asset_path == diceset.asset_path) {
            continue;
        }
        let mut meshes = Vec::with_capacity(DieKind::ALL.len());
        let mut materials = Vec::with_capacity(DieKind::ALL.len());
        for mesh_index in 0..DieKind::ALL.len() {
            meshes.push(asset_server.load(
                GltfAssetLabel::Primitive { mesh: mesh_index, primitive: 0 }.from_asset(diceset.asset_path.clone()),
            ));
            // The bare material label is a `GltfMaterial`; `/std` selects the `StandardMaterial`.
            let material_label =
                format!("{}/std", GltfAssetLabel::Material { index: mesh_index, is_scale_inverted: false });
            materials.push(asset_server.load(AssetPath::from(diceset.asset_path.clone()).with_label(material_label)));
        }
        diceset.handles = Some(GltfAssetHandles { asset_path: diceset.asset_path.clone(), meshes, materials });
    }
}

#[cfg(test)]
mod tests {
    use bevy::asset::AssetPlugin;

    use super::*;

    fn app_with_diceset() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .add_systems(Update, load_diceset_handles);
        let entity = app.world_mut().spawn(Diceset::custom_with("first.glb", DiceOrientations::default())).id();
        app.update();
        (app, entity)
    }

    fn assert_asset_path(app: &App, entity: Entity, expected: &str) {
        let handles = app.world().get::<Diceset>(entity).unwrap().handles().unwrap();
        assert_eq!(handles.asset_path, expected);
        for mesh_index in 0..DieKind::ALL.len() {
            assert_eq!(handles.mesh(mesh_index).path().unwrap().path().to_str(), Some(expected));
            assert_eq!(handles.material(mesh_index).path().unwrap().path().to_str(), Some(expected));
        }
    }

    #[test]
    fn replacing_existing_diceset_initializes_handles() {
        let (mut app, entity) = app_with_diceset();
        assert_asset_path(&app, entity, "first.glb");
        app.world_mut().entity_mut(entity).insert(Diceset::custom_with("second.glb", DiceOrientations::default()));
        app.update();
        assert_asset_path(&app, entity, "second.glb");
    }

    #[test]
    fn asset_path_mutation_reloads_meshes_and_materials() {
        let (mut app, entity) = app_with_diceset();
        app.world_mut().get_mut::<Diceset>(entity).unwrap().asset_path = "second.glb".into();
        app.update();
        assert_asset_path(&app, entity, "second.glb");
    }

    #[test]
    fn orientation_changes_and_updates_preserve_handles_and_change_tick() {
        let (mut app, entity) = app_with_diceset();
        let mesh = app.world().get::<Diceset>(entity).unwrap().handles().unwrap().mesh(0);
        app.world_mut().get_mut::<Diceset>(entity).unwrap().orientations = DiceOrientations::default();
        let changed = app.world_mut().get_mut::<Diceset>(entity).unwrap().last_changed();
        for _ in 0..3 {
            app.update();
            let diceset = app.world_mut().get_mut::<Diceset>(entity).unwrap();
            assert_eq!(diceset.handles().unwrap().mesh(0), mesh);
            assert_eq!(diceset.last_changed(), changed);
        }
    }
}
