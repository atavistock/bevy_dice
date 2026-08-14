## Lighting

- **DirectionalLight**: `shadows_enabled` renamed to `shadow_maps_enabled`.

## Text

- **TextFont.font_size**: Changed from `f32` to the `FontSize` enum. Use `FontSize::Px(64.0)` for the old pixel-size behavior (also has `Vw`, `Vh`, `VMin`, `VMax` variants).

## Scenes (not used by this crate, noted for reference)

- **bevy_scene crate renamed to bevy_world_serialization**: `Scene` -> `WorldAsset`, `SceneRoot` -> `WorldAssetRoot`, `DynamicScene` -> `DynamicWorld`, `SceneSpawner` -> `WorldInstanceSpawner`.
- **gltf material sub-assets**: Now load as `GltfMaterial` by default instead of `StandardMaterial`; use a `/std` label suffix to get `StandardMaterial` when the `bevy_pbr` feature is enabled. This crate loads materials via `GltfAssetLabel::Material`, which still resolves fine.

## Notes

- avian3d 0.7.0 targets bevy 0.19; no API changes needed in this crate's physics usage.
- No other API surface used by this crate changed between 0.18 and 0.19.
