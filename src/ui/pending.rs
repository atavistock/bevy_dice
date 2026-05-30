//! In-flight roll bookkeeping.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::dice::{DiceRoll, DieKind, MAX_OPTION_ITERATIONS};

use super::arena::DiceArena;
use super::diceset::{Diceset, GltfAssetHandles};
use super::plugin::DiceRenderLayer;
use super::rng::DiceRng;
use super::roller::RollRequest;
use super::spawn::{member_state, spawn_die, throw_base, MAX_DICE_PER_ROLL};

pub(super) struct PendingRoll {
    pub arena: Entity,
    pub parsed: DiceRoll,
    pub terms: Vec<Vec<DieRef>>,
    /// Per-term remaining reroll+explode budget, capped at [`crate::dice::MAX_OPTION_ITERATIONS`].
    pub budgets: Vec<u32>,
}

pub(super) struct DieRef {
    pub entity: Entity,
    pub kind: DieKind,
    // Some(0)/Some(1) = d100 tens/ones pair; None = standalone die.
    pub pair_slot: Option<u8>,
}

#[derive(Resource, Default)]
pub(super) struct PendingRolls(pub HashMap<u64, PendingRoll>);

pub(super) fn handle_roll_requests(
    mut requests: MessageReader<RollRequest>,
    mut pending: ResMut<PendingRolls>,
    arenas: Query<&DiceArena>,
    dicesets: Query<&Diceset>,
    default_layer: Res<DiceRenderLayer>,
    mut rng: ResMut<DiceRng>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok(arena) = arenas.get(request.arena) else {
            warn!("RollRequest {} targets unknown arena entity", request.roll_id);
            continue;
        };
        let Ok(diceset) = dicesets.get(arena.diceset) else {
            warn!("RollRequest {} arena's diceset entity is missing", request.roll_id);
            continue;
        };
        let Some(handles) = diceset.handles() else {
            warn!("RollRequest {} diceset handles not yet loaded", request.roll_id);
            continue;
        };
        let layer = arena.render_layer.unwrap_or(default_layer.0);
        let terms = spawn_roll_dice(&mut commands, arena, request.arena, &request.roll, handles, layer, &mut *rng);
        let budgets = vec![MAX_OPTION_ITERATIONS; request.roll.terms.len()];
        pending.0.insert(
            request.roll_id,
            PendingRoll { arena: request.arena, parsed: request.roll.clone(), terms, budgets },
        );
    }
}

/// Spawns every die for `roll` in `arena`, capped at [`MAX_DICE_PER_ROLL`].
/// Returns one [`DieRef`] vec per term (in source order).
fn spawn_roll_dice<R: rand::Rng + ?Sized>(
    commands: &mut Commands,
    arena: &DiceArena,
    arena_entity: Entity,
    roll: &DiceRoll,
    handles: &GltfAssetHandles,
    layer: u8,
    rng: &mut R,
) -> Vec<Vec<DieRef>> {
    let mut terms: Vec<Vec<DieRef>> = Vec::with_capacity(roll.terms.len());
    let mut spawned = 0usize;
    for term in &roll.terms {
        let mut term_refs = Vec::new();
        for _ in 0..term.count {
            let needed = if term.kind == DieKind::D100 { 2 } else { 1 };
            if spawned + needed > MAX_DICE_PER_ROLL {
                warn!(
                    "roll exceeds MAX_DICE_PER_ROLL ({}); remaining dice dropped",
                    MAX_DICE_PER_ROLL
                );
                terms.push(term_refs);
                return terms;
            }
            let member_refs = spawn_term_member(commands, arena, arena_entity, term.kind, handles, layer, rng);
            spawned += member_refs.len();
            term_refs.extend(member_refs);
        }
        terms.push(term_refs);
    }
    terms
}

/// Spawns one logical die member of a term (a d100 produces two entities).
pub(super) fn spawn_term_member<R: rand::Rng + ?Sized>(
    commands: &mut Commands,
    arena: &DiceArena,
    arena_entity: Entity,
    kind: DieKind,
    handles: &GltfAssetHandles,
    layer: u8,
    rng: &mut R,
) -> Vec<DieRef> {
    let base = throw_base(arena, rng);
    if kind == DieKind::D100 {
        let pair_offset = arena.spawn.pair_z_offset;
        let tens_state = member_state(&base, arena, -pair_offset, rng);
        let ones_state = member_state(&base, arena, pair_offset, rng);
        let tens = spawn_die(commands, handles, DieKind::D100, arena_entity, arena, &tens_state, layer);
        let ones = spawn_die(commands, handles, DieKind::D10, arena_entity, arena, &ones_state, layer);
        vec![
            DieRef { entity: tens, kind: DieKind::D100, pair_slot: Some(0) },
            DieRef { entity: ones, kind: DieKind::D10, pair_slot: Some(1) },
        ]
    } else {
        let state = member_state(&base, arena, 0.0, rng);
        let entity = spawn_die(commands, handles, kind, arena_entity, arena, &state, layer);
        vec![DieRef { entity, kind, pair_slot: None }]
    }
}
