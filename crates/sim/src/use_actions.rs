//! Authoritative non-EOC item use actions.

use cdda_protocol::{
    ACTION_POINTS_PER_UPSTREAM_MOVE, ActorId, CommandRejection, CommandSequence, ItemId,
    ItemPlaceMonsterTypeV1, ItemTransformTypeV1, SimTick, WorldEvent, WorldEventKind,
    WorldPosition, item_place_monster_catalog_is_valid, item_transform_catalog_is_valid,
};
use rand_core::Rng;

use crate::{
    ItemInstance, SimError, WorldState, actor_effective_intelligence, actor_skill_level,
    inclusive_rng_u64, mapgen, validate_item_snapshot,
};

impl WorldState {
    pub fn register_item_place_monster_types(
        &mut self,
        catalog: Vec<ItemPlaceMonsterTypeV1>,
    ) -> Result<(), SimError> {
        if self.tick != SimTick(0)
            || !self.actors.is_empty()
            || !item_place_monster_catalog_is_valid(&catalog)
        {
            return Err(SimError::InvalidItem);
        }
        self.item_place_monster_types = catalog
            .into_iter()
            .map(|profile| (profile.source_type_id.clone(), profile))
            .collect();
        Ok(())
    }

    pub(super) fn place_monster_action_cost(
        &self,
        actor_id: ActorId,
        item_id: ItemId,
        choice_id: Option<&str>,
    ) -> Result<Option<i64>, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(item) = actor.inventory.get(&item_id) else {
            return Ok(None);
        };
        let Some(profile) = self.item_place_monster_types.get(&item.type_id) else {
            return Ok(None);
        };
        if item.available_tool_charges()
            < i32::try_from(profile.required_charges).map_err(|_| SimError::InvalidItem)?
        {
            return Ok(Some(0));
        }
        let has_target = if profile.place_randomly {
            choice_id.is_none()
                && !self
                    .place_monster_candidate_positions(actor.position)
                    .is_empty()
        } else {
            choice_id
                .and_then(parse_place_monster_choice)
                .and_then(|(dx, dy)| actor.position.checked_offset(dx, dy, 0))
                .is_some_and(|position| self.can_place_deployed_creature(position))
        };
        if !has_target {
            return Ok(Some(0));
        }
        Ok(Some(
            i64::from(profile.move_cost_moves)
                .checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE)
                .ok_or(SimError::NumericOverflow)?,
        ))
    }

    pub(super) fn apply_place_monster_item(
        &mut self,
        actor_id: ActorId,
        activation_sequence: CommandSequence,
        item_id: ItemId,
        expected_item_type_id: &str,
        choice_id: Option<&str>,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(item) = actor.inventory.get(&item_id) else {
            return Ok(false);
        };
        if item.type_id != expected_item_type_id {
            return Ok(false);
        }
        let Some(profile) = self
            .item_place_monster_types
            .get(expected_item_type_id)
            .cloned()
        else {
            return Ok(false);
        };
        if item.available_tool_charges()
            < i32::try_from(profile.required_charges).map_err(|_| SimError::InvalidItem)?
        {
            return Ok(false);
        }
        if !profile.place_randomly && choice_id.is_none() {
            self.request_place_monster_position(
                actor_id,
                activation_sequence,
                item_id,
                expected_item_type_id.to_owned(),
                &profile.monster_type_id,
                events,
            )?;
            return Ok(true);
        }
        let prototype = self
            .worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog.monster_prototypes.iter().find(|prototype| {
                    prototype.base.monster_type_id == profile.monster_type_id
                        && prototype.runtime_spawnable
                })
            })
            .cloned();
        let Some(prototype) = prototype else {
            return Ok(false);
        };
        let mut rng = self.named_rng(
            b"place-monster-item",
            &[actor_id.as_u128(), item_id.as_u128()],
            activation_sequence.0,
        );
        let position = if profile.place_randomly {
            if choice_id.is_some() {
                return Ok(false);
            }
            let candidates = self.place_monster_candidate_positions(actor.position);
            if candidates.is_empty() {
                return Ok(false);
            }
            let selected = inclusive_rng_u64(
                &mut rng,
                0,
                u64::try_from(candidates.len() - 1).map_err(|_| SimError::NumericOverflow)?,
            );
            candidates[usize::try_from(selected).map_err(|_| SimError::NumericOverflow)?]
        } else {
            let Some((dx, dy)) = choice_id.and_then(parse_place_monster_choice) else {
                return Ok(false);
            };
            let Some(position) = actor.position.checked_offset(dx, dy, 0) else {
                return Ok(false);
            };
            if !self.can_place_deployed_creature(position) {
                return Ok(false);
            }
            position
        };

        let friendly = deployment_is_friendly(actor, &profile, &mut rng)?;
        let item_raw_damage = item.raw_damage;
        let mut inventory = actor.inventory.clone();
        let mut ammunition = prototype.starting_ammunition.clone();
        if !prototype.interior_ammunition {
            for (ammunition_type_id, loaded) in &mut ammunition {
                *loaded = crate::items::debit_inventory_type(
                    &mut inventory,
                    ammunition_type_id,
                    *loaded,
                )?;
            }
        }
        if profile.single_use {
            inventory.remove(&item_id);
        } else {
            let Some(item) = inventory.get_mut(&item_id) else {
                return Ok(false);
            };
            if item.debit_tool_charges(1).is_err() {
                return Ok(false);
            }
        }

        let spawn = mapgen::creature_spawn_from_worldgen(&prototype, position);
        let maximum_damage = i64::from(profile.maximum_raw_damage);
        let damage_factor = maximum_damage
            .checked_sub(i64::from(item_raw_damage))
            .and_then(|value| value.checked_add(1))
            .ok_or(SimError::NumericOverflow)?;
        let deployed_hp = i32::try_from(
            i64::from(spawn.hp)
                .checked_mul(damage_factor)
                .and_then(|value| value.checked_div(maximum_damage + 1))
                .ok_or(SimError::NumericOverflow)?
                .max(1),
        )
        .map_err(|_| SimError::NumericOverflow)?;
        let creature_id = self.spawn_creature(spawn)?;
        let creature = self
            .creatures
            .get_mut(&creature_id)
            .ok_or(SimError::UnknownCreature)?;
        creature.friendly = if friendly { -1 } else { 0 };
        creature.pet = friendly && profile.is_pet;
        creature.hp = deployed_hp;
        creature.ammunition = ammunition;
        let actor = self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?;
        actor.inventory = inventory;
        if !actor.inventory.contains_key(&item_id) && actor.wielded == Some(item_id) {
            actor.wielded = None;
        }
        let message = if friendly {
            if profile.friendly_message.is_empty() {
                format!("You deploy the {}.", prototype.display_name)
            } else {
                profile.friendly_message
            }
        } else if profile.hostile_message.is_empty() {
            format!(
                "You deploy the {} wrong.  It is hostile!",
                prototype.display_name
            )
        } else {
            profile.hostile_message
        };
        events.push(self.make_event(WorldEventKind::CreatureDeployed {
            actor_id,
            item_id,
            creature_id,
            position,
            friendly,
            pet: friendly && profile.is_pet,
            message,
        })?);
        Ok(true)
    }

    fn place_monster_candidate_positions(&self, center: WorldPosition) -> Vec<WorldPosition> {
        [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ]
        .into_iter()
        .filter_map(|(dx, dy)| center.checked_offset(dx, dy, 0))
        .filter(|position| self.can_place_deployed_creature(*position))
        .collect()
    }

    fn can_place_deployed_creature(&self, position: WorldPosition) -> bool {
        self.is_passable(position)
            && self.actor_at(position).is_none()
            && self.creature_at(position).is_none()
            && self.npc_at(position).is_none()
            && !self.vehicle_blocks_actor_at(position)
    }

    pub fn register_item_transform_types(
        &mut self,
        catalog: Vec<ItemTransformTypeV1>,
    ) -> Result<(), SimError> {
        if self.tick != SimTick(0)
            || !self.actors.is_empty()
            || !item_transform_catalog_is_valid(&catalog)
        {
            return Err(SimError::InvalidItem);
        }
        self.item_transform_types = catalog
            .into_iter()
            .map(|profile| (profile.source_type_id.clone(), profile))
            .collect();
        Ok(())
    }

    pub(super) fn item_transform_action_cost(
        &self,
        actor_id: ActorId,
        item_id: ItemId,
    ) -> Result<Option<i64>, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(item) = actor.inventory.get(&item_id) else {
            return Ok(None);
        };
        self.item_transform_types
            .get(&item.type_id)
            .map(|profile| {
                i64::from(profile.move_cost_moves)
                    .checked_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
                    .ok_or(SimError::NumericOverflow)
            })
            .transpose()
    }

    pub(super) fn apply_item_transform(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        item_id: ItemId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(item) = actor.inventory.get(&item_id) else {
            return Ok(false);
        };
        let Some(profile) = self.item_transform_types.get(&item.type_id).cloned() else {
            return Ok(false);
        };
        let required = i32::try_from(profile.required_charges)
            .map_err(|_| SimError::InvalidItem)?
            .max(i32::try_from(profile.consumed_charges).map_err(|_| SimError::InvalidItem)?);
        if item.available_tool_charges() < required {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemHasNoPower)?);
            return Ok(true);
        }

        let target_type_id = profile.target.type_id.clone();
        let mut transformed = item.clone();
        let transformation = (|| {
            let remaining_charges = transformed.debit_tool_charges(
                i32::try_from(profile.consumed_charges).map_err(|_| SimError::InvalidItem)?,
            )?;
            apply_transform_target(&mut transformed, &profile.target)?;
            validate_item_snapshot(&transformed.snapshot())?;
            Ok::<_, SimError>(remaining_charges)
        })();
        let remaining_charges = match transformation {
            Ok(remaining_charges) => remaining_charges,
            Err(_) => {
                events.push(self.rejection(
                    actor_id,
                    sequence,
                    CommandRejection::ItemNotActivatable,
                )?);
                return Ok(true);
            }
        };
        let replaced = self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .inventory
            .insert(item_id, transformed);
        if replaced.is_none() {
            return Err(SimError::UnknownItem);
        }
        events.push(self.make_event(WorldEventKind::ItemTransformed {
            actor_id,
            item_id,
            from_type_id: profile.source_type_id,
            to_type_id: target_type_id,
            remaining_charges,
        })?);
        Ok(true)
    }
}

fn parse_place_monster_choice(choice_id: &str) -> Option<(i8, i8)> {
    match choice_id {
        "-1,-1" => Some((-1, -1)),
        "0,-1" => Some((0, -1)),
        "1,-1" => Some((1, -1)),
        "-1,0" => Some((-1, 0)),
        "1,0" => Some((1, 0)),
        "-1,1" => Some((-1, 1)),
        "0,1" => Some((0, 1)),
        "1,1" => Some((1, 1)),
        _ => None,
    }
}

fn deployment_is_friendly(
    actor: &crate::Actor,
    profile: &ItemPlaceMonsterTypeV1,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Result<bool, SimError> {
    if profile.difficulty < 0 {
        return Ok(false);
    }
    let intelligence_maximum = u32::from(actor_effective_intelligence(actor) / 2);
    let intelligence_roll = if intelligence_maximum == 0 {
        0
    } else {
        rng.next_u32() % (intelligence_maximum + 1)
    };
    let skill_sum = profile.skills.iter().try_fold(0_u32, |total, skill_id| {
        total
            .checked_add(u32::from(actor_skill_level(actor, skill_id, false)))
            .ok_or(SimError::NumericOverflow)
    })?;
    let difficulty_maximum =
        u32::try_from(profile.difficulty).map_err(|_| SimError::InvalidItem)?;
    let difficulty_roll = if difficulty_maximum == 0 {
        0
    } else {
        rng.next_u32() % (difficulty_maximum + 1)
    };
    Ok(intelligence_roll
        .checked_mul(2)
        .and_then(|value| value.checked_add(skill_sum))
        .ok_or(SimError::NumericOverflow)?
        >= difficulty_roll
            .checked_mul(2)
            .ok_or(SimError::NumericOverflow)?)
}

fn apply_transform_target(
    item: &mut ItemInstance,
    target: &cdda_protocol::CraftItemPrototypeV1,
) -> Result<(), SimError> {
    let integral_layout_matches = item
        .integral_magazines
        .iter()
        .zip(&target.integral_magazines)
        .all(|(current, target)| {
            current.pocket_index == target.pocket_index
                && current.pocket_id == target.pocket_id
                && current.ammunition_type == target.ammunition_type
                && current.capacity == target.capacity
                && current.rigid == target.rigid
                && current.reloadable == target.reloadable
                && current.unloadable == target.unloadable
        })
        && item.integral_magazines.len() == target.integral_magazines.len();
    let well_layout_matches =
        item.magazine_wells
            .iter()
            .zip(&target.magazine_wells)
            .all(|(current, target)| {
                current.pocket_index == target.pocket_index
                    && current.pocket_id == target.pocket_id
                    && current.compatible_magazine_type_ids == target.compatible_magazine_type_ids
                    && current.rigid == target.rigid
                    && current.unloadable == target.unloadable
            })
            && item.magazine_wells.len() == target.magazine_wells.len();
    let container_layout_matches = item
        .ammunition_containers
        .iter()
        .zip(&target.ammunition_containers)
        .all(|(current, target)| {
            current.pocket_index == target.pocket_index
                && current.pocket_id == target.pocket_id
                && current.capacities == target.capacities
                && current.access_moves == target.access_moves
                && current.rigid == target.rigid
                && current.reloadable == target.reloadable
                && current.unloadable == target.unloadable
                && current.spawn_state.as_ref().map(|state| &state.rules)
                    == target.spawn_rules.as_ref()
        })
        && item.ammunition_containers.len() == target.ammunition_containers.len();
    let temperature_layout_matches = item
        .temperature
        .as_ref()
        .map(|temperature| (temperature.current_phase, temperature.thermal_properties))
        == target
            .tracks_temperature
            .then_some((target.containment.phase, target.thermal_properties));
    if target.powered_tool.is_some()
        || item.powered_tool.is_some()
        || item.ammunition_type != target.ammunition_type
        || item.magazine_capacity != target.magazine_capacity
        || item.containment.count_by_charges != target.containment.count_by_charges
        || !integral_layout_matches
        || !well_layout_matches
        || !container_layout_matches
        || !temperature_layout_matches
    {
        return Err(SimError::InvalidItem);
    }
    item.type_id.clone_from(&target.type_id);
    item.melee_damage_milli
        .clone_from(&target.melee_damage_milli);
    item.calories = target.calories;
    item.quench = target.quench;
    item.comestible_type.clone_from(&target.comestible_type);
    item.ranged_weapon.clone_from(&target.ranged_weapon);
    item.containment.clone_from(&target.containment);
    Ok(())
}
