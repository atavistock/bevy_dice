//! Rendered dice ownership and cleanup.

use bevy::prelude::*;

use crate::dice::DieKind;

use super::arena::DiceArena;

/// Marker for a rendered die and its owning arena.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct SpawnedDie {
    pub kind: DieKind,
    pub arena: Entity,
}

pub fn despawn_orphaned_dice(
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
