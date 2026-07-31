//! Authoritative monster-specific combat behavior.

use cdda_protocol::{ActorId, CreatureId};

use crate::{SimError, UNARMED_DAMAGE, WorldState};

impl WorldState {
    fn creature_armor_milli(&self, target: CreatureId, damage_type: &str) -> Result<i32, SimError> {
        let creature = self
            .creatures
            .get(&target)
            .ok_or(SimError::UnknownCreature)?;
        Ok(self
            .worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|prototype| {
                        prototype
                            .base
                            .monster_type_id
                            .as_str()
                            .cmp(&creature.type_id)
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
            })
            .and_then(|prototype| prototype.armor_milli.get(damage_type))
            .copied()
            .unwrap_or_default())
    }

    pub(super) fn creature_damage_after_armor(
        &self,
        target: CreatureId,
        damage_type: &str,
        damage_milli: u32,
    ) -> Result<u16, SimError> {
        let armor = i64::from(self.creature_armor_milli(target, damage_type)?);
        let remaining = i64::from(damage_milli).saturating_sub(armor).max(0);
        let rounded = remaining
            .checked_add(500)
            .ok_or(SimError::NumericOverflow)?
            / 1_000;
        u16::try_from(rounded).map_err(|_| SimError::NumericOverflow)
    }

    pub(super) fn actor_melee_damage_against_creature(
        &self,
        actor_id: ActorId,
        target: CreatureId,
    ) -> Result<u16, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let components = actor
            .wielded
            .and_then(|item_id| actor.inventory.get(&item_id))
            .map(|item| item.melee_damage_milli.clone())
            .unwrap_or_else(|| {
                std::collections::BTreeMap::from([(
                    String::from("bash"),
                    i32::from(UNARMED_DAMAGE) * 1_000,
                )])
            });
        let mut total_milli = 0_i64;
        for (damage_type, damage_milli) in components {
            let armor = i64::from(self.creature_armor_milli(target, &damage_type)?);
            total_milli = total_milli
                .checked_add(i64::from(damage_milli).saturating_sub(armor).max(0))
                .ok_or(SimError::NumericOverflow)?;
        }
        let rounded = total_milli
            .checked_add(500)
            .ok_or(SimError::NumericOverflow)?
            / 1_000;
        u16::try_from(rounded).map_err(|_| SimError::NumericOverflow)
    }
}
