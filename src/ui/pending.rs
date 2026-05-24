//! In-flight roll bookkeeping and asset handle cache.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::dice::{DiceRoll, DieKind};

use super::arena::DiceArena;
use super::roller::RollRequest;
use super::spawn::{member_state, spawn_die, throw_base, MAX_DICE_PER_ROLL};

pub(super) struct GltfAssetHandles {
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
pub(super) struct DiceAssetHandles {
    by_path: HashMap<String, GltfAssetHandles>,
}

pub(super) struct PendingRoll {
    pub arena: Entity,
    pub parsed: DiceRoll,
    pub terms: Vec<Vec<DieRef>>,
}

pub(super) struct DieRef {
    pub entity: Entity,
    pub kind: DieKind,
    // Some(0)/Some(1) = d100 tens/ones pair; None = standalone die.
    pub pair_slot: Option<u8>,
}

#[derive(Resource, Default)]
pub(super) struct PendingRolls(pub HashMap<u64, PendingRoll>);

pub(super) fn preload_dice_assets(
    arenas: Query<&DiceArena>,
    asset_server: Res<AssetServer>,
    mut handles: ResMut<DiceAssetHandles>,
) {
    for arena in arenas.iter() {
        if handles.by_path.contains_key(&arena.diceset) {
            continue;
        }
        let mut bundle = GltfAssetHandles {
            meshes: Vec::with_capacity(DieKind::ALL.len()),
            materials: Vec::with_capacity(DieKind::ALL.len()),
        };
        for mesh_index in 0..DieKind::ALL.len() {
            bundle.meshes.push(asset_server.load(
                GltfAssetLabel::Primitive { mesh: mesh_index, primitive: 0 }
                    .from_asset(arena.diceset.clone()),
            ));
            bundle.materials.push(asset_server.load(
                GltfAssetLabel::Material { index: mesh_index, is_scale_inverted: false }
                    .from_asset(arena.diceset.clone()),
            ));
        }
        handles.by_path.insert(arena.diceset.clone(), bundle);
    }
}

pub(super) fn handle_roll_requests(
    mut requests: MessageReader<RollRequest>,
    mut pending: ResMut<PendingRolls>,
    arenas: Query<&DiceArena>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(arena) = arenas.get(request.arena) else {
            eprintln!(
                "warning: RollRequest {} targets unknown arena entity",
                request.roll_id
            );
            continue;
        };
        let mut rng = rand::thread_rng();
        let mut terms: Vec<Vec<DieRef>> = Vec::with_capacity(request.roll.terms.len());
        let mut spawned = 0usize;
        let cap = MAX_DICE_PER_ROLL;

        'terms: for term in &request.roll.terms {
            let mut term_refs = Vec::new();
            for _ in 0..term.count {
                let base = throw_base(arena, &mut rng);
                match term.kind {
                    DieKind::D100 => {
                        if spawned + 2 > cap {
                            break 'terms;
                        }
                        let pair_offset = arena.spawn.pair_z_offset;
                        let tens_state = member_state(&base, arena, -pair_offset, &mut rng);
                        let ones_state = member_state(&base, arena, pair_offset, &mut rng);
                        let tens = spawn_die(
                            &mut commands, &asset_server, DieKind::D100, request.arena, arena,
                            &tens_state,
                        );
                        let ones = spawn_die(
                            &mut commands, &asset_server, DieKind::D10, request.arena, arena,
                            &ones_state,
                        );
                        term_refs.push(DieRef { entity: tens, kind: DieKind::D100, pair_slot: Some(0) });
                        term_refs.push(DieRef { entity: ones, kind: DieKind::D10, pair_slot: Some(1) });
                        spawned += 2;
                    }
                    _ => {
                        if spawned + 1 > cap {
                            break 'terms;
                        }
                        let state = member_state(&base, arena, 0.0, &mut rng);
                        let entity = spawn_die(
                            &mut commands, &asset_server, term.kind, request.arena, arena, &state,
                        );
                        term_refs.push(DieRef { entity, kind: term.kind, pair_slot: None });
                        spawned += 1;
                    }
                }
            }
            terms.push(term_refs);
        }

        pending.0.insert(
            request.roll_id,
            PendingRoll { arena: request.arena, parsed: request.roll.clone(), terms },
        );
    }
}
