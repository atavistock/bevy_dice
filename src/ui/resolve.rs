//! Settle detection: reads each die's up-face and emits RollComplete.

use std::collections::HashMap;

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::Rng as _;

use crate::dice::{DieKind, RollOutcome, RolledDie};

use super::arena::DiceArena;
use super::orientations::DiceOrientations;
use super::pending::{DieRef, PendingRoll, PendingRolls};
use super::roller::RollComplete;
use super::spawn::SpawnedDie;

// cos(~10deg): up-face flatness needed to count as settled.
const FLAT_FACE_THRESHOLD: f32 = 0.985;

// Angular nudge for sleeping dice balanced on an edge or corner.
const PERTURB_ANGULAR_SPEED: f32 = 3.0;

pub(super) fn resolve_pending_rolls(
    mut pending: ResMut<PendingRolls>,
    sleeping_dice: Query<(&SpawnedDie, &Transform), With<Sleeping>>,
    mut wake_dice: Query<&mut AngularVelocity, With<Sleeping>>,
    arenas: Query<&DiceArena>,
    mut completed: MessageWriter<RollComplete>,
) {
    let mut rng = rand::thread_rng();
    let mut ready: Vec<(u64, HashMap<Entity, Quat>)> = Vec::new();
    let mut to_perturb: Vec<Entity> = Vec::new();

    for (id, roll) in pending.0.iter() {
        let Ok(arena) = arenas.get(roll.arena) else { continue };
        let mut all_flat = true;
        let mut rotations: HashMap<Entity, Quat> = HashMap::new();
        let mut roll_perturbs: Vec<Entity> = Vec::new();

        for die_ref in roll.terms.iter().flatten() {
            let Ok((spawned, transform)) = sleeping_dice.get(die_ref.entity) else {
                all_flat = false;
                continue;
            };
            let max_y = arena
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
            ready.push((*id, rotations));
        } else {
            to_perturb.extend(roll_perturbs);
        }
    }

    for entity in to_perturb {
        if let Ok(mut angular_velocity) = wake_dice.get_mut(entity) {
            let direction = Vec3::new(
                rng.gen_range(-1.0..1.0_f32),
                rng.gen_range(-0.2..0.2_f32),
                rng.gen_range(-1.0..1.0_f32),
            )
            .normalize_or_zero();
            angular_velocity.0 = direction * PERTURB_ANGULAR_SPEED;
        }
    }

    for (roll_id, rotations) in ready {
        let Some(roll) = pending.0.remove(&roll_id) else { continue };
        let Ok(arena) = arenas.get(roll.arena) else { continue };
        let outcome = compute_outcome(&roll, &rotations, &arena.orientations);
        completed.write(RollComplete {
            roll_id,
            roll: roll.parsed,
            outcome,
            arena: roll.arena,
        });
    }
}

fn compute_outcome(
    roll: &PendingRoll,
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> RollOutcome {
    let mut dice = Vec::new();
    let mut total = roll.parsed.adjustment;

    for (term_index, term_refs) in roll.terms.iter().enumerate() {
        let term = &roll.parsed.terms[term_index];
        let mut term_values = collect_term_values(term_refs, rotations, orientations);

        if let Some(keep_high) = term.options.keep_highest {
            term_values.sort_by(|a, b| b.cmp(a));
            term_values.truncate(keep_high as usize);
        }
        if let Some(keep_low) = term.options.keep_lowest {
            term_values.sort();
            term_values.truncate(keep_low as usize);
        }

        for value in term_values {
            dice.push(RolledDie { kind: term.kind, value, negate: term.negate });
            total += value as i32 * if term.negate { -1 } else { 1 };
        }
    }

    RollOutcome { total, dice }
}

fn collect_term_values(
    term_refs: &[DieRef],
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> Vec<u32> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < term_refs.len() {
        let reference = &term_refs[index];
        if reference.pair_slot == Some(0) && index + 1 < term_refs.len() {
            let ones_reference = &term_refs[index + 1];
            let tens = read_raw_value(reference, rotations, orientations);
            let ones = read_raw_value(ones_reference, rotations, orientations);
            let combined = if tens == 0 && ones == 0 { 100 } else { tens + ones };
            values.push(combined);
            index += 2;
        } else {
            values.push(read_die_value(reference, rotations, orientations));
            index += 1;
        }
    }
    values
}

fn read_die_value(
    reference: &DieRef,
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> u32 {
    let raw = read_raw_value(reference, rotations, orientations);
    match reference.kind {
        DieKind::D10 if reference.pair_slot.is_none() && raw == 0 => 10,
        _ => raw,
    }
}

fn read_raw_value(
    reference: &DieRef,
    rotations: &HashMap<Entity, Quat>,
    orientations: &DiceOrientations,
) -> u32 {
    let Some(rotation) = rotations.get(&reference.entity).copied() else { return 0 };
    let Some(label) = orientations.up_face(reference.kind, rotation) else { return 0 };
    label.parse().unwrap_or(0)
}
