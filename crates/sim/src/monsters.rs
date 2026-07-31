//! Authoritative monster-specific combat behavior.

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorId, BookStudyInterruptionReason, ConstructionInterruptionReason,
    CreatureId, CreatureSnapshot, CreatureSpecialAttackStateV1, DisassemblyInterruptionReason,
    SimTick, WorldEvent, WorldEventKind, WorldPosition, WorldgenCatalogV1,
    WorldgenMonsterAttackEffectV1, WorldgenMonsterPrototypeV1, WorldgenMonsterSpecialAttackKindV1,
    WorldgenMonsterSpecialAttackV1,
};
use rand_core::Rng;

use crate::{
    SimError, UNARMED_DAMAGE, WorldState, combat::ActorDamageUnit, horizontal_distance_squared,
    horizontally_adjacent, ranged_distance,
};

pub(super) fn special_state_matches_catalog(
    catalog: Option<&WorldgenCatalogV1>,
    snapshot: &CreatureSnapshot,
) -> bool {
    let expected = catalog
        .and_then(|catalog| {
            catalog
                .monster_prototypes
                .binary_search_by(|prototype| {
                    prototype
                        .base
                        .monster_type_id
                        .as_str()
                        .cmp(&snapshot.type_id)
                })
                .ok()
                .and_then(|index| catalog.monster_prototypes.get(index))
        })
        .map(|prototype| {
            prototype
                .special_attacks
                .iter()
                .map(|attack| attack.attack_id.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    snapshot
        .special_attacks
        .iter()
        .map(|attack| attack.attack_id.as_str())
        .eq(expected)
}

impl WorldState {
    pub(super) fn initial_creature_special_attacks(
        &self,
        type_id: &str,
        creature_id: CreatureId,
    ) -> Result<Vec<CreatureSpecialAttackStateV1>, SimError> {
        let profiles = self
            .worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|prototype| {
                        prototype.base.monster_type_id.as_str().cmp(type_id)
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
            })
            .map(|prototype| prototype.special_attacks.clone())
            .unwrap_or_default();
        let mut rng = self.named_rng(
            b"creature-special-initial-cooldown",
            &[creature_id.as_u128()],
            0,
        );
        profiles
            .into_iter()
            .map(|attack| {
                let cooldown_turns = if attack.cooldown_turns == 0 {
                    0
                } else {
                    u32::try_from(rng.next_u64() % (u64::from(attack.cooldown_turns) + 1))
                        .map_err(|_| SimError::NumericOverflow)?
                };
                Ok(CreatureSpecialAttackStateV1 {
                    attack_id: attack.attack_id,
                    cooldown_turns,
                    enabled: true,
                })
            })
            .collect()
    }

    pub(super) fn advance_creature_special_cooldowns(&mut self) {
        if !self.tick.0.is_multiple_of(SimTick::HZ) {
            return;
        }
        for creature in self.creatures.values_mut() {
            for attack in &mut creature.special_attacks {
                if attack.enabled {
                    attack.cooldown_turns = attack.cooldown_turns.saturating_sub(1);
                }
            }
        }
    }

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
        self.apply_monster_attack_effects(
            target,
            hit_body_part_id,
            dealt_cut_or_stab_damage,
            &effects,
            rng,
        )
    }

    fn apply_monster_attack_effects(
        &mut self,
        target: ActorId,
        hit_body_part_id: &str,
        dealt_cut_or_stab_damage: bool,
        effects: &[WorldgenMonsterAttackEffectV1],
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
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
                effect.body_part_id.clone()
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
                    effect_id: effect.effect_id.clone(),
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

    pub(super) fn try_creature_special_attacks(
        &mut self,
        source: CreatureId,
        visible_target: Option<(ActorId, WorldPosition)>,
        destination: Option<WorldPosition>,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<i64, SimError> {
        let profiles = self
            .creature_prototype(source)?
            .map(|prototype| prototype.special_attacks.clone())
            .unwrap_or_default();
        let states = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .special_attacks
            .clone();
        let mut total_cost = 0_i64;
        for (index, profile) in profiles.iter().enumerate() {
            let Some(state) = states
                .iter()
                .find(|state| state.attack_id == profile.attack_id)
            else {
                return Err(SimError::InvalidCreature);
            };
            if !state.enabled || state.cooldown_turns > 0 {
                continue;
            }
            if profile.condition.as_ref().is_some_and(|condition| {
                !self
                    .creature_eoc_condition_matches(source, condition)
                    .unwrap_or(false)
            }) {
                continue;
            }
            let sequence = turn_sequence
                .checked_mul(64)
                .and_then(|sequence| sequence.checked_add(index as u64))
                .ok_or(SimError::NumericOverflow)?;
            let used = match profile.kind {
                WorldgenMonsterSpecialAttackKindV1::Melee
                | WorldgenMonsterSpecialAttackKindV1::Bite => {
                    let Some((target, target_position)) = visible_target else {
                        continue;
                    };
                    let source_position = self
                        .creatures
                        .get(&source)
                        .ok_or(SimError::UnknownCreature)?
                        .position;
                    let adjacent = horizontally_adjacent(source_position, target_position);
                    let distance = ranged_distance(source_position, target_position);
                    if (profile.no_adjacent && adjacent)
                        || (profile.range == 1 && !adjacent)
                        || distance > profile.range
                        || (profile.range > 1
                            && !self.has_clear_shot(source_position, target_position))
                        || self.actors.get(&target).is_none_or(|actor| actor.hp <= 0)
                    {
                        continue;
                    }
                    self.execute_creature_special_attack(
                        source, target, profile, sequence, events,
                    )?;
                    if !profile.eoc_ids.is_empty() {
                        let _ =
                            self.apply_creature_eocs(source, target, &profile.eoc_ids, sequence)?;
                    }
                    true
                }
                WorldgenMonsterSpecialAttackKindV1::Leap => {
                    let Some(destination) = destination else {
                        continue;
                    };
                    let has_live_target = visible_target.is_some_and(|(target, _position)| {
                        self.actors.get(&target).is_some_and(|actor| actor.hp > 0)
                    });
                    if !has_live_target && !profile.leap_allow_no_target {
                        continue;
                    }
                    self.execute_creature_leap(source, destination, profile, sequence, events)?
                }
                WorldgenMonsterSpecialAttackKindV1::Eoc => {
                    let Some((target, target_position)) = visible_target else {
                        continue;
                    };
                    let source_position = self
                        .creatures
                        .get(&source)
                        .ok_or(SimError::UnknownCreature)?
                        .position;
                    let distance = ranged_distance(source_position, target_position);
                    if distance == 0
                        || distance > profile.range
                        || (profile.range == 1
                            && !horizontally_adjacent(source_position, target_position))
                        || (profile.range > 1
                            && !self.has_clear_shot(source_position, target_position))
                        || self.actors.get(&target).is_none_or(|actor| actor.hp <= 0)
                    {
                        continue;
                    }
                    let _ = self.apply_creature_eocs(source, target, &profile.eoc_ids, sequence)?;
                    true
                }
            };
            if !used {
                continue;
            }
            self.creatures
                .get_mut(&source)
                .and_then(|creature| {
                    creature
                        .special_attacks
                        .iter_mut()
                        .find(|state| state.attack_id == profile.attack_id)
                })
                .ok_or(SimError::InvalidCreature)?
                .cooldown_turns = profile.cooldown_turns;
            total_cost = total_cost
                .checked_add(
                    i64::from(profile.move_cost_moves)
                        .checked_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .ok_or(SimError::NumericOverflow)?;
        }
        Ok(total_cost)
    }

    fn execute_creature_leap(
        &mut self,
        source: CreatureId,
        destination: WorldPosition,
        profile: &WorldgenMonsterSpecialAttackV1,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let origin = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .position;
        if origin.z != destination.z || origin == destination {
            return Ok(false);
        }
        // The pinned default enables circular distance. `rl_dist` still
        // truncates that Euclidean result to an integer before the actor
        // compares it with the configured floating-point consider bounds.
        let destination_distance_milli = horizontal_euclidean_distance_floor(origin, destination)?
            .checked_mul(1_000)
            .ok_or(SimError::NumericOverflow)?;
        let origin_destination_distance = destination_distance_milli / 1_000;
        if destination_distance_milli < profile.leap_minimum_consider_range_milli
            || destination_distance_milli > profile.leap_maximum_consider_range_milli
        {
            return Ok(false);
        }
        let radius = i32::try_from(profile.leap_maximum_range_milli.div_ceil(1_000))
            .map_err(|_| SimError::NumericOverflow)?;
        let minimum_squared = u128::from(profile.leap_minimum_range_milli).pow(2);
        let maximum_squared = u128::from(profile.leap_maximum_range_milli).pow(2);
        let light_sources = self.active_light_sources();
        let mut candidates = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let offset_x = i8::try_from(dx).map_err(|_| SimError::NumericOverflow)?;
                let offset_y = i8::try_from(dy).map_err(|_| SimError::NumericOverflow)?;
                let Some(candidate) = origin.checked_offset(offset_x, offset_y, 0) else {
                    continue;
                };
                let dx_milli = i128::from(dx) * 1_000;
                let dy_milli = i128::from(dy) * 1_000;
                let squared = u128::try_from(dx_milli * dx_milli + dy_milli * dy_milli)
                    .map_err(|_| SimError::NumericOverflow)?;
                if squared < minimum_squared || squared > maximum_squared {
                    continue;
                }
                let candidate_distance =
                    horizontal_euclidean_distance_floor(candidate, destination)?;
                if candidate_distance >= origin_destination_distance
                    && !(profile.leap_prefer || profile.leap_random)
                {
                    continue;
                }
                if !self.is_passable(candidate)
                    || self.actor_at(candidate).is_some()
                    || self.creature_at(candidate).is_some()
                    || !self.has_clear_shot(origin, candidate)
                    || !self.creature_can_see_position(source, candidate, &light_sources)?
                    || (!profile.leap_ignore_destination_danger
                        && self
                            .creatures
                            .get(&source)
                            .ok_or(SimError::UnknownCreature)?
                            .path_settings
                            .avoid_dangerous_fields
                        && self.fields_at(candidate).is_some_and(|fields| {
                            fields.iter().any(|field| {
                                self.field_types
                                    .get(&field.field_type_id)
                                    .and_then(|field_type| {
                                        field_type
                                            .intensity_levels
                                            .get(usize::from(field.intensity - 1))
                                    })
                                    .is_some_and(|level| level.dangerous)
                            })
                        }))
                {
                    continue;
                }
                candidates.push((candidate_distance, candidate));
            }
        }
        if candidates.is_empty() {
            return Ok(false);
        }
        candidates
            .sort_by_key(|(distance, position)| (*distance, position.z, position.y, position.x));
        if !profile.leap_random {
            let best = candidates[0].0;
            candidates.retain(|(distance, _position)| *distance == best);
        }
        let mut rng = self.named_rng(b"creature-special-leap", &[source.as_u128()], turn_sequence);
        let index = usize::try_from(rng.next_u64() % candidates.len() as u64)
            .map_err(|_| SimError::NumericOverflow)?;
        let to = candidates[index].1;
        self.creatures
            .get_mut(&source)
            .ok_or(SimError::UnknownCreature)?
            .position = to;
        events.push(self.make_event(WorldEventKind::CreatureMoved {
            creature_id: source,
            from: origin,
            to,
        })?);
        Ok(true)
    }

    fn execute_creature_special_attack(
        &mut self,
        source: CreatureId,
        target: ActorId,
        profile: &WorldgenMonsterSpecialAttackV1,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let accuracy = profile.accuracy.unwrap_or(i32::from(
            self.creatures
                .get(&source)
                .ok_or(SimError::UnknownCreature)?
                .melee_skill,
        ));
        let (spread, dodge_attempted) = self.creature_actor_special_attack_roll(
            source,
            target,
            turn_sequence,
            accuracy,
            profile.dodgeable,
        )?;
        if dodge_attempted {
            self.consume_actor_dodge_attempt(target)?;
        }
        if spread < 0 {
            let target_was_sleeping = self
                .actors
                .get(&target)
                .ok_or(SimError::UnknownActor)?
                .sleeping;
            events.push(self.make_event(WorldEventKind::CreatureMissedActor {
                source,
                target,
                stumbled: false,
                target_was_sleeping,
            })?);
            return Ok(());
        }
        let mut rng = self.named_rng(
            b"creature-special-melee-damage",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let multiplier = roll_inclusive_i32(
            profile.minimum_damage_multiplier_millionths,
            profile.maximum_damage_multiplier_millionths,
            &mut rng,
        )?;
        let damage = profile
            .damage
            .iter()
            .map(|unit| {
                Ok(ActorDamageUnit {
                    damage_type_id: unit.damage_type_id.clone(),
                    amount_milli: unit.amount_milli,
                    armor_penetration_milli: unit.armor_penetration_milli,
                    armor_multiplier_millionths: unit.armor_multiplier_millionths,
                    damage_multiplier_millionths: i128::from(unit.damage_multiplier_millionths)
                        .checked_mul(i128::from(multiplier))
                        .and_then(|value| value.checked_div(1_000_000))
                        .and_then(|value| i32::try_from(value).ok())
                        .ok_or(SimError::NumericOverflow)?,
                    constant_armor_multiplier_millionths: unit.constant_armor_multiplier_millionths,
                    constant_damage_multiplier_millionths: unit
                        .constant_damage_multiplier_millionths,
                })
            })
            .collect::<Result<Vec<_>, SimError>>()?;
        let (outcome, was_sleeping, cut_or_stab_damage) =
            self.damage_actor_components(target, &damage, &mut rng)?;
        if outcome.amount > 0 || !profile.effects_require_damage {
            self.apply_monster_attack_effects(
                target,
                &outcome.body_part_id,
                cut_or_stab_damage > 0,
                &profile.effects,
                &mut rng,
            )?;
        }
        if outcome.amount > 0 && matches!(profile.kind, WorldgenMonsterSpecialAttackKindV1::Bite) {
            self.apply_bite_infection(
                target,
                &outcome.body_part_id,
                profile.infection_chance_millionths,
                &mut rng,
            )?;
        }
        events.push(self.make_event(WorldEventKind::ActorDamagedByCreature {
            source,
            target,
            body_part_id: outcome.body_part_id,
            amount: outcome.amount,
            remaining_part_hp: outcome.remaining_part_hp,
            remaining_hp: outcome.remaining_hp,
        })?);
        if outcome.amount > 0 {
            self.interrupt_craft(target, events)?;
            self.interrupt_book_study(target, BookStudyInterruptionReason::Damage, events)?;
            self.interrupt_disassembly(target, DisassemblyInterruptionReason::Damage, events)?;
            self.interrupt_construction(target, ConstructionInterruptionReason::Damage, events)?;
            if was_sleeping && outcome.remaining_hp > 0 {
                self.wake_actor(target, cdda_protocol::WakeReason::Damage, events)?;
            }
            if outcome.remaining_hp <= 0 {
                events.push(self.make_event(WorldEventKind::ActorKilledByCreature {
                    actor_id: target,
                    killer: source,
                })?);
            }
        }
        Ok(())
    }

    fn apply_bite_infection(
        &mut self,
        target: ActorId,
        body_part_id: &str,
        chance_millionths: u32,
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        if rng.next_u32() % 1_000_000 >= chance_millionths {
            return Ok(());
        }
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        if let Some(existing) = actor.effects.iter_mut().find(|effect| {
            matches!(effect.effect_id.as_str(), "bite" | "infected")
                && effect.body_part_id.as_deref() == Some(body_part_id)
        }) {
            existing.expires_at_tick = SimTick(u64::MAX);
            return Ok(());
        }
        if actor.effects.len() < 1_024 {
            actor.effects.push(ActorEffectSnapshotV1 {
                effect_id: String::from("bite"),
                body_part_id: Some(body_part_id.to_owned()),
                intensity: 1,
                expires_at_tick: SimTick(u64::MAX),
            });
            actor.effects.sort_by(|left, right| {
                (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
            });
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

fn horizontal_euclidean_distance_floor(
    left: WorldPosition,
    right: WorldPosition,
) -> Result<u32, SimError> {
    u32::try_from(horizontal_distance_squared(left, right)?.isqrt())
        .map_err(|_| SimError::NumericOverflow)
}

fn roll_inclusive_u32(minimum: u32, maximum: u32, rng: &mut impl Rng) -> Result<u32, SimError> {
    let span = u64::from(maximum)
        .checked_sub(u64::from(minimum))
        .and_then(|span| span.checked_add(1))
        .ok_or(SimError::NumericOverflow)?;
    let offset = rng.next_u64() % span;
    u32::try_from(u64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
}

fn roll_inclusive_i32(minimum: i32, maximum: i32, rng: &mut impl Rng) -> Result<i32, SimError> {
    let span = i64::from(maximum)
        .checked_sub(i64::from(minimum))
        .and_then(|span| span.checked_add(1))
        .and_then(|span| u64::try_from(span).ok())
        .ok_or(SimError::NumericOverflow)?;
    let offset = i64::try_from(rng.next_u64() % span).map_err(|_| SimError::NumericOverflow)?;
    i32::try_from(i64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
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
