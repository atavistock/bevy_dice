//! Settle detection: reads each die's up-face and emits RollComplete.

use std::collections::HashMap;
use std::ops::Range;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::dice::{DieKind, Modifier, Options, RollOutcome, RolledDie};

use super::arena::DiceArena;
use super::diceset::Diceset;
use super::orientations::DiceOrientations;
use super::pending::{DieRef, PendingRoll, PendingRolls, spawn_term_member};
use super::plugin::DiceRenderLayer;
use super::rng::DiceRng;
use super::roller::RollComplete;
use super::spawn::SpawnedDie;

pub(super) fn despawn_orphaned_dice(
    mut commands: Commands,
    mut removed: RemovedComponents<DiceArena>,
    dice: Query<(Entity, &SpawnedDie)>,
) {
    for arena_entity in removed.read() {
        for (die_entity, spawned) in dice.iter() {
            if spawned.arena == arena_entity {
                commands.entity(die_entity).despawn();
            }
        }
    }
}

pub(super) fn prune_orphaned_pending_rolls(
    mut pending: ResMut<PendingRolls>,
    arenas: Query<(), With<DiceArena>>,
    dice: Query<(), With<SpawnedDie>>,
) {
    pending.rolls.retain(|roll_id, roll| {
        if arenas.get(roll.arena).is_err() {
            warn!("dropping pending roll {} - arena despawned", roll_id);
            return false;
        }
        for die_ref in roll.terms.iter().flatten() {
            if dice.get(die_ref.entity).is_err() {
                warn!("dropping pending roll {} - die despawned", roll_id);
                return false;
            }
        }
        true
    });
}

// cos(~10deg): up-face flatness needed to count as settled.
const FLAT_FACE_THRESHOLD: f32 = 0.985;

const PERTURB_ANGULAR_SPEED: f32 = 3.0;

pub(super) fn resolve_pending_rolls(
    mut pending: ResMut<PendingRolls>,
    sleeping_dice: Query<(&SpawnedDie, &Transform), With<Sleeping>>,
    mut wake_dice: Query<&mut AngularVelocity, With<Sleeping>>,
    arenas: Query<&DiceArena>,
    dicesets: Query<&Diceset>,
    default_layer: Res<DiceRenderLayer>,
    mut rng: ResMut<DiceRng>,
    mut commands: Commands,
    mut completed: MessageWriter<RollComplete>,
) {
    let (ready, to_perturb) = detect_settled_rolls(&pending, &sleeping_dice, &arenas, &dicesets);
    perturb_stuck_dice(&mut wake_dice, &to_perturb, &mut *rng);
    let truly_ready = apply_physics_modifiers(&mut pending, ready, default_layer.layer, &mut commands, &mut *rng);
    emit_completed_rolls(&mut pending, truly_ready, &mut completed);
}

/// A pending roll whose dice have all settled flat, with its arena and diceset resolved.
struct ReadyRoll<'a> {
    roll_id: u64,
    arena: &'a DiceArena,
    diceset: &'a Diceset,
    rotations: HashMap<Entity, Quat>,
}

/// Scans `pending` for rolls whose dice have all settled flat. Returns
/// `(ready_rolls, stuck_dice_to_perturb)`. Read-only.
fn detect_settled_rolls<'a>(
    pending: &PendingRolls,
    sleeping_dice: &Query<(&SpawnedDie, &Transform), With<Sleeping>>,
    arenas: &'a Query<&DiceArena>,
    dicesets: &'a Query<&Diceset>,
) -> (Vec<ReadyRoll<'a>>, Vec<Entity>) {
    let mut ready = Vec::new();
    let mut to_perturb = Vec::new();

    for (id, roll) in pending.rolls.iter() {
        let Ok(arena) = arenas.get(roll.arena) else { continue };
        let Ok(diceset) = dicesets.get(arena.diceset) else { continue };
        let mut all_flat = true;
        let mut rotations: HashMap<Entity, Quat> = HashMap::new();
        let mut roll_perturbs: Vec<Entity> = Vec::new();

        for die_ref in roll.terms.iter().flatten() {
            let Ok((spawned, transform)) = sleeping_dice.get(die_ref.entity) else {
                all_flat = false;
                continue;
            };
            let max_y = diceset
                .orientations
                .faces(spawned.kind)
                .iter()
                .map(|(_, direction)| (transform.rotation * *direction).y)
                .fold(f32::NEG_INFINITY, f32::max);
            if max_y < FLAT_FACE_THRESHOLD {
                all_flat = false;
                roll_perturbs.push(die_ref.entity);
            } else {
                rotations.insert(die_ref.entity, transform.rotation);
            }
        }

        if all_flat {
            ready.push(ReadyRoll { roll_id: *id, arena, diceset, rotations });
        } else {
            to_perturb.extend(roll_perturbs);
        }
    }

    (ready, to_perturb)
}

/// Applies a small random angular kick to each sleeping die that has settled
/// on an edge or corner so it tips onto a flat face.
fn perturb_stuck_dice<R: rand::Rng + ?Sized>(
    wake_dice: &mut Query<&mut AngularVelocity, With<Sleeping>>,
    to_perturb: &[Entity],
    rng: &mut R,
) {
    for &entity in to_perturb {
        if let Ok(mut angular_velocity) = wake_dice.get_mut(entity) {
            let direction =
                Vec3::new(rng.gen_range(-1.0..1.0_f32), rng.gen_range(-0.2..0.2_f32), rng.gen_range(-1.0..1.0_f32))
                    .normalize_or_zero();
            angular_velocity.0 = direction * PERTURB_ANGULAR_SPEED;
        }
    }
}

/// For each settled-flat roll, applies reroll/explode by despawning + spawning
/// fresh dice. Returns the roll ids that need no further dice (ready to emit).
/// Rolls that still have pending modifications stay in `pending` for the next frame.
fn apply_physics_modifiers<'a, R: rand::Rng + ?Sized>(
    pending: &mut PendingRolls,
    ready: Vec<ReadyRoll<'a>>,
    default_layer: u8,
    commands: &mut Commands,
    rng: &mut R,
) -> Vec<ReadyRoll<'a>> {
    let mut truly_ready = Vec::new();
    for ready_roll in ready {
        let Some(roll) = pending.rolls.get_mut(&ready_roll.roll_id) else { continue };
        let Some(handles) = ready_roll.diceset.handles() else { continue };
        let layer = ready_roll.arena.render_layer.unwrap_or(default_layer);
        let modified = trigger_term_modifiers(
            roll,
            ready_roll.arena,
            ready_roll.diceset,
            handles,
            layer,
            &ready_roll.rotations,
            commands,
            rng,
        );
        if !modified {
            truly_ready.push(ready_roll);
        }
    }
    truly_ready
}

/// Scans every term for reroll/explode triggers and applies them in place,
/// respecting [`PendingRoll::budgets`]. Returns true if any term was modified.
fn trigger_term_modifiers<R: rand::Rng + ?Sized>(
    roll: &mut PendingRoll,
    arena: &DiceArena,
    diceset: &Diceset,
    handles: &super::diceset::GltfAssetHandles,
    layer: u8,
    rotations: &HashMap<Entity, Quat>,
    commands: &mut Commands,
    rng: &mut R,
) -> bool {
    let mut modified = false;
    for term_index in 0..roll.terms.len() {
        let term = &roll.parsed.terms[term_index];
        if term.options.reroll_at_or_below.is_none() && term.options.explode_at_or_above.is_none() {
            continue;
        }
        let triggers =
            find_triggers(&roll.terms[term_index], term.kind, &term.options, rotations, &diceset.orientations);
        if triggers.is_empty() {
            continue;
        }
        for trigger in triggers {
            if roll.budgets[term_index] == 0 {
                break;
            }
            match trigger {
                Trigger::Reroll(range) => {
                    for i in range.clone() {
                        commands.entity(roll.terms[term_index][i].entity).despawn();
                    }
                    let replacement = spawn_term_member(commands, arena, roll.arena, term.kind, handles, layer, rng);
                    debug_assert_eq!(replacement.len(), range.len());
                    for (i, new_ref) in range.zip(replacement.into_iter()) {
                        roll.terms[term_index][i] = new_ref;
                    }
                    roll.budgets[term_index] -= 1;
                    modified = true;
                }
                Trigger::Explode(source_index) => {
                    let extra = spawn_term_member(commands, arena, roll.arena, term.kind, handles, layer, rng);
                    roll.terms[term_index][source_index].exploded = true;
                    roll.terms[term_index].extend(extra);
                    roll.budgets[term_index] -= 1;
                    modified = true;
                }
            }
        }
    }
    modified
}

/// What to do with a settled die that triggered an option.
enum Trigger {
    /// Despawn the dice at the given range and respawn one fresh member.
    Reroll(Range<usize>),
    /// Spawn one extra member for the term; carries the index of the member that triggered.
    Explode(usize),
}

fn find_triggers(
    term_refs: &[DieRef],
    kind: DieKind,
    options: &Options,
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> Vec<Trigger> {
    let mut triggers = Vec::new();
    let mut index = 0;
    while index < term_refs.len() {
        let (value, range) = read_member_value(term_refs, index, rotations, orientations);
        match options.modifier_for(kind, value, term_refs[index].exploded) {
            Some(Modifier::Reroll) => triggers.push(Trigger::Reroll(range.clone())),
            Some(Modifier::Explode) => triggers.push(Trigger::Explode(index)),
            None => {}
        }
        index = range.end;
    }
    triggers
}

/// Reads the logical value of the die member starting at `index` plus the
/// term_refs range it occupies (1 for a normal die, 2 for a d100 pair).
fn read_member_value(
    term_refs: &[DieRef],
    index: usize,
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> (u32, Range<usize>) {
    let reference = &term_refs[index];
    if reference.pair_slot == Some(0) && index + 1 < term_refs.len() {
        let tens = read_raw_value(reference, rotations, orientations);
        let ones = read_raw_value(&term_refs[index + 1], rotations, orientations);
        let combined = if tens == 0 && ones == 0 { 100 } else { tens + ones };
        (combined, index..index + 2)
    } else {
        (read_die_value(reference, rotations, orientations), index..index + 1)
    }
}

/// Removes each ready roll from `pending`, computes its outcome, and writes a
/// [`RollComplete`] message.
fn emit_completed_rolls(
    pending: &mut PendingRolls,
    ready: Vec<ReadyRoll<'_>>,
    completed: &mut MessageWriter<RollComplete>,
) {
    for ready_roll in ready {
        let Some(roll) = pending.rolls.remove(&ready_roll.roll_id) else { continue };
        let outcome = compute_outcome(&roll, &ready_roll.rotations, &ready_roll.diceset.orientations);
        completed.write(RollComplete { roll_id: ready_roll.roll_id, roll: roll.parsed, outcome, arena: roll.arena });
    }
}

fn compute_outcome(
    roll: &PendingRoll,
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> RollOutcome {
    let mut dice = Vec::new();
    let mut term_lengths = Vec::with_capacity(roll.terms.len());
    let mut total = roll.parsed.adjustment;

    for (term_index, term_refs) in roll.terms.iter().enumerate() {
        let term = &roll.parsed.terms[term_index];
        let mut term_values = collect_term_values(term_refs, rotations, orientations);
        term.options.apply_keep(&mut term_values);

        term_lengths.push(term_values.len() as u32);
        for value in term_values {
            dice.push(RolledDie { kind: term.kind, value, negate: term.negate });
            total += value as i32 * if term.negate { -1 } else { 1 };
        }
    }

    RollOutcome { total, dice, term_lengths }
}

fn collect_term_values(
    term_refs: &[DieRef],
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> Vec<u32> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < term_refs.len() {
        let (value, range) = read_member_value(term_refs, index, rotations, orientations);
        values.push(value);
        index = range.end;
    }
    values
}

fn read_die_value(reference: &DieRef, rotations: &HashMap<Entity, Quat>, orientations: &DiceOrientations) -> u32 {
    let raw = read_raw_value(reference, rotations, orientations);
    match reference.kind {
        DieKind::D10 if reference.pair_slot.is_none() && raw == 0 => 10,
        _ => raw,
    }
}

fn read_raw_value(reference: &DieRef, rotations: &HashMap<Entity, Quat>, orientations: &DiceOrientations) -> u32 {
    let Some(rotation) = rotations.get(&reference.entity).copied() else { return 0 };
    let Some(label) = orientations.up_face(reference.kind, rotation) else { return 0 };
    label.parse().unwrap_or(0)
}
