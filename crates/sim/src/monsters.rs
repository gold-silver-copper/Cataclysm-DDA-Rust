//! Authoritative monster-specific combat behavior.

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorId, CreatureId, SimTick, WorldgenMonsterPrototypeV1,
};
use rand_core::Rng;

use crate::{SimError, UNARMED_DAMAGE, WorldState, combat::ActorDamageUnit};

impl WorldState {
    fn creature_prototype(
        &self,
        target: CreatureId,
    ) -> Result<Option<&WorldgenMonsterPrototypeV1>, SimError> {
        let creature = self
            .creatures
            .get(&target)
            .ok_or(SimError::UnknownCreature)?;
        Ok(self.worldgen.as_ref().and_then(|catalog| {
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
        }))
    }

    fn creature_armor_milli(&self, target: CreatureId, damage_type: &str) -> Result<i32, SimError> {
        Ok(self
            .creature_prototype(target)?
            .and_then(|prototype| prototype.armor_milli.get(damage_type))
            .copied()
            .unwrap_or_default())
    }

    pub(super) fn creature_melee_damage_units(
        &self,
        source: CreatureId,
        rolled_bash_damage: u16,
    ) -> Result<Vec<ActorDamageUnit>, SimError> {
        let Some(prototype) = self.creature_prototype(source)? else {
            return Ok(vec![ActorDamageUnit::ordinary("bash", rolled_bash_damage)]);
        };
        let mut units = prototype
            .melee_damage
            .iter()
            .map(|unit| ActorDamageUnit {
                damage_type_id: unit.damage_type_id.clone(),
                amount_milli: unit.amount_milli,
                armor_penetration_milli: unit.armor_penetration_milli,
                armor_multiplier_millionths: unit.armor_multiplier_millionths,
                damage_multiplier_millionths: unit.damage_multiplier_millionths,
                constant_armor_multiplier_millionths: unit.constant_armor_multiplier_millionths,
                constant_damage_multiplier_millionths: unit.constant_damage_multiplier_millionths,
            })
            .collect::<Vec<_>>();
        let rolled_milli = i32::from(rolled_bash_damage)
            .checked_mul(1_000)
            .ok_or(SimError::NumericOverflow)?;
        if let Some(bash) = units.iter_mut().find(|unit| unit.damage_type_id == "bash") {
            merge_rolled_bash_damage(
                bash,
                rolled_milli,
                prototype.melee_dice_armor_penetration_milli,
            )?;
        } else {
            units.push(ActorDamageUnit {
                damage_type_id: String::from("bash"),
                amount_milli: rolled_milli,
                armor_penetration_milli: prototype.melee_dice_armor_penetration_milli,
                armor_multiplier_millionths: 1_000_000,
                damage_multiplier_millionths: 1_000_000,
                constant_armor_multiplier_millionths: 1_000_000,
                constant_damage_multiplier_millionths: 1_000_000,
            });
        }
        Ok(units)
    }

    pub(super) fn apply_creature_attack_effects(
        &mut self,
        source: CreatureId,
        target: ActorId,
        hit_body_part_id: &str,
        dealt_cut_or_stab_damage: bool,
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        let effects = self
            .creature_prototype(source)?
            .map(|prototype| prototype.attack_effects.clone())
            .unwrap_or_default();
        for effect in effects {
            if effect.requires_cut_or_stab_damage && !dealt_cut_or_stab_damage {
                continue;
            }
            let chance_roll = rng.next_u32() % 1_000_000;
            if chance_roll >= effect.chance_millionths {
                continue;
            }
            let duration_turns = roll_inclusive_u32(
                effect.duration_minimum_turns,
                effect.duration_maximum_turns,
                rng,
            )?;
            let intensity =
                roll_inclusive_u32(effect.intensity_minimum, effect.intensity_maximum, rng)?;
            if duration_turns == 0 || intensity == 0 {
                continue;
            }
            let body_part_id = if effect.affect_hit_body_part {
                Some(hit_body_part_id.to_owned())
            } else {
                effect.body_part_id
            };
            if body_part_id.as_ref().is_some_and(|body_part_id| {
                !self
                    .actor_anatomy
                    .parts
                    .iter()
                    .any(|part| part.body_part_id == *body_part_id)
            }) {
                continue;
            }
            let duration_ticks = u64::from(duration_turns)
                .checked_mul(SimTick::HZ)
                .ok_or(SimError::NumericOverflow)?;
            let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
            if let Some(existing) = actor.effects.iter_mut().find(|existing| {
                existing.effect_id == effect.effect_id && existing.body_part_id == body_part_id
            }) {
                existing.intensity = intensity;
                existing.expires_at_tick =
                    if effect.permanent || existing.expires_at_tick == SimTick(u64::MAX) {
                        SimTick(u64::MAX)
                    } else {
                        SimTick(
                            existing
                                .expires_at_tick
                                .0
                                .saturating_add(duration_ticks)
                                .min(u64::MAX - 1),
                        )
                    };
            } else if actor.effects.len() < 1_024 {
                actor.effects.push(ActorEffectSnapshotV1 {
                    effect_id: effect.effect_id,
                    body_part_id,
                    intensity,
                    expires_at_tick: if effect.permanent {
                        SimTick(u64::MAX)
                    } else {
                        SimTick(self.tick.0.saturating_add(duration_ticks).min(u64::MAX - 1))
                    },
                });
                actor.effects.sort_by(|left, right| {
                    (&left.effect_id, &left.body_part_id)
                        .cmp(&(&right.effect_id, &right.body_part_id))
                });
            }
        }
        Ok(())
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

fn roll_inclusive_u32(minimum: u32, maximum: u32, rng: &mut impl Rng) -> Result<u32, SimError> {
    let span = u64::from(maximum)
        .checked_sub(u64::from(minimum))
        .and_then(|span| span.checked_add(1))
        .ok_or(SimError::NumericOverflow)?;
    let offset = rng.next_u64() % span;
    u32::try_from(u64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
}

fn merge_rolled_bash_damage(
    fixed: &mut ActorDamageUnit,
    rolled_amount_milli: i32,
    rolled_armor_penetration_milli: i32,
) -> Result<(), SimError> {
    let existing_multiplier = i128::from(fixed.damage_multiplier_millionths);
    if existing_multiplier <= 0 {
        return Err(SimError::NumericOverflow);
    }
    // Pinned damage_instance::add normalizes a same-type unit by the ratio of
    // the incoming and existing damage multipliers before adding amount/AP.
    let ratio_millionths = 1_000_000_i128
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(existing_multiplier))
        .ok_or(SimError::NumericOverflow)?;
    let scaled_addition = |value: i32| -> Result<i32, SimError> {
        i128::from(value)
            .checked_mul(ratio_millionths)
            .and_then(|value| value.checked_div(1_000_000))
            .and_then(|value| i32::try_from(value).ok())
            .ok_or(SimError::NumericOverflow)
    };
    fixed.amount_milli = fixed
        .amount_milli
        .checked_add(scaled_addition(rolled_amount_milli)?)
        .ok_or(SimError::NumericOverflow)?;
    fixed.armor_penetration_milli = fixed
        .armor_penetration_milli
        .checked_add(scaled_addition(rolled_armor_penetration_milli)?)
        .ok_or(SimError::NumericOverflow)?;
    // The pinned implementation interpolates toward the incoming damage
    // multiplier (1.0) for the merged armor multiplier.
    let interpolation_millionths = 1_000_000_i128
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(existing_multiplier + 1_000_000))
        .ok_or(SimError::NumericOverflow)?;
    fixed.armor_multiplier_millionths = i128::from(fixed.armor_multiplier_millionths)
        .checked_add(
            (1_000_000_i128 - i128::from(fixed.armor_multiplier_millionths))
                .checked_mul(interpolation_millionths)
                .and_then(|value| value.checked_div(1_000_000))
                .ok_or(SimError::NumericOverflow)?,
        )
        .and_then(|value| i32::try_from(value).ok())
        .ok_or(SimError::NumericOverflow)?;
    Ok(())
}
