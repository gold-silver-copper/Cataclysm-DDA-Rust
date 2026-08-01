//! Authoritative non-EOC item use actions.

use cdda_protocol::{
    ACTION_POINTS_PER_UPSTREAM_MOVE, ActorId, CommandRejection, CommandSequence, ItemId,
    ItemPlaceMonsterTypeV1, ItemTransformTypeV1, SimTick, WorldEvent, WorldEventKind,
    WorldPosition, item_place_monster_catalog_is_valid, item_transform_catalog_is_valid,
};
use rand_core::Rng;

use crate::{
    ItemInstance, SimError, WorldState, actor_effective_intelligence, actor_skill_level, mapgen,
    validate_item_snapshot,
};

impl WorldState {
    pub fn register_item_place_monster_types(
        &mut self,
        catalog: Vec<ItemPlaceMonsterTypeV1>,
    ) -> Result<(), SimError> {
        if self.tick != SimTick(0)
            || !self.actors.is_empty()
            || !self.item_place_monster_types.is_empty()
            || !item_place_monster_catalog_is_valid(&catalog)
            || catalog.iter().any(|profile| {
                self.worldgen.as_ref().is_none_or(|worldgen| {
                    worldgen.monster_prototypes.iter().all(|prototype| {
                        prototype.base.monster_type_id != profile.monster_type_id
                            || !prototype.runtime_spawnable
                    })
                })
            })
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
        activation_origin: Option<WorldPosition>,
    ) -> Result<Option<i64>, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(item) = actor.inventory.get(&item_id) else {
            return Ok(None);
        };
        let Some(profile) = self.item_place_monster_types.get(&item.type_id) else {
            return Ok(None);
        };
        if (profile.maximum_raw_damage > 0 && item.raw_damage >= profile.maximum_raw_damage)
            || item.available_tool_charges()
                < i32::try_from(profile.required_charges).map_err(|_| SimError::InvalidItem)?
            || item.available_tool_charges()
                < i32::try_from(profile.activation_charges).map_err(|_| SimError::InvalidItem)?
        {
            return Ok(Some(0));
        }
        let has_target = if profile.place_randomly {
            choice_id.is_none()
                && !self
                    .place_monster_candidate_positions(actor.position, &profile.monster_type_id)
                    .is_empty()
        } else {
            activation_origin.is_none_or(|origin| origin == actor.position)
                && choice_id
                    .and_then(parse_place_monster_choice)
                    .and_then(|(dx, dy)| {
                        activation_origin
                            .unwrap_or(actor.position)
                            .checked_offset(dx, dy, 0)
                    })
                    .is_some_and(|position| {
                        self.can_place_deployed_creature(position, &profile.monster_type_id)
                    })
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
        activation_origin: Option<WorldPosition>,
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
        if (profile.maximum_raw_damage > 0 && item.raw_damage >= profile.maximum_raw_damage)
            || item.available_tool_charges()
                < i32::try_from(profile.required_charges).map_err(|_| SimError::InvalidItem)?
            || item.available_tool_charges()
                < i32::try_from(profile.activation_charges).map_err(|_| SimError::InvalidItem)?
        {
            return Ok(false);
        }
        if !profile.place_randomly && choice_id.is_none() {
            self.request_place_monster_position(
                actor_id,
                activation_sequence,
                item_id,
                expected_item_type_id.to_owned(),
                &prototype_display_name(self, &profile.monster_type_id)
                    .unwrap_or_else(|| profile.monster_type_id.clone()),
                actor.position,
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
            // Pinned `random_point` performs ten independent trials in the
            // bounding 3x3 range, then enumerates all suitable points and
            // selects one uniformly as a fallback.
            let mut selected = None;
            for _ in 0..10 {
                let dx = i8::try_from(unbiased_inclusive_u64(&mut rng, 0, 2))
                    .map_err(|_| SimError::NumericOverflow)?
                    - 1;
                let dy = i8::try_from(unbiased_inclusive_u64(&mut rng, 0, 2))
                    .map_err(|_| SimError::NumericOverflow)?
                    - 1;
                let _z_roll = unbiased_inclusive_u64(&mut rng, 0, 0);
                if let Some(candidate) = actor.position.checked_offset(dx, dy, 0)
                    && self.can_place_deployed_creature(candidate, &profile.monster_type_id)
                {
                    selected = Some(candidate);
                    break;
                }
            }
            selected.unwrap_or_else(|| actor.position)
        } else {
            let Some((dx, dy)) = choice_id.and_then(parse_place_monster_choice) else {
                return Ok(false);
            };
            let Some(position) = activation_origin
                .unwrap_or(actor.position)
                .checked_offset(dx, dy, 0)
            else {
                return Ok(false);
            };
            // A pending prompt is tied to its activation origin. Movement
            // invalidates it instead of shifting the eight relative choices.
            if activation_origin.is_some_and(|origin| actor.position != origin)
                || !self.can_place_deployed_creature(position, &profile.monster_type_id)
            {
                return Ok(false);
            }
            position
        };

        let position = if profile.place_randomly && position == actor.position {
            let candidates =
                self.place_monster_candidate_positions(actor.position, &profile.monster_type_id);
            if candidates.is_empty() {
                return Ok(false);
            }
            let selected = unbiased_inclusive_u64(
                &mut rng,
                0,
                u64::try_from(candidates.len() - 1).map_err(|_| SimError::NumericOverflow)?,
            );
            candidates[usize::try_from(selected).map_err(|_| SimError::NumericOverflow)?]
        } else {
            position
        };

        let item_raw_damage = item.raw_damage;
        let mut inventory = actor.inventory.clone();
        let mut traversal_priority = actor.wielded.into_iter().collect::<Vec<_>>();
        traversal_priority.extend(actor.worn.iter().copied());
        let mut ammunition = prototype.starting_ammunition.clone();
        let mut ammunition_feedback = Vec::new();
        if !prototype.interior_ammunition {
            for (ammunition_type_id, loaded) in &mut ammunition {
                *loaded = crate::items::debit_inventory_type(
                    &mut inventory,
                    &traversal_priority,
                    ammunition_type_id,
                    *loaded,
                )?;
                if *loaded == 0 {
                    ammunition_feedback.push(format!(
                        "No {ammunition_type_id} ammunition was available for the {}.",
                        prototype.display_name
                    ));
                } else {
                    ammunition_feedback.push(format!(
                        "You load {loaded} x {ammunition_type_id} into the {}.",
                        prototype.display_name
                    ));
                }
            }
        }
        let friendly = deployment_is_friendly(actor, &profile, &mut rng)?;
        if profile.activation_charges > 0 {
            let Some(item) = inventory.get_mut(&item_id) else {
                return Ok(false);
            };
            item.debit_tool_charges(
                i32::try_from(profile.activation_charges).map_err(|_| SimError::InvalidItem)?,
            )?;
        }
        if profile.single_use {
            inventory.remove(&item_id);
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
        let message = if ammunition_feedback.is_empty() {
            message
        } else {
            format!("{} {message}", ammunition_feedback.join(" "))
        };

        // Deployment consumes an item, allocates a creature identity, mutates
        // two canonical owners, and emits one lifecycle event. Stage all of it
        // so allocator/event exhaustion cannot leave a creature without its
        // corresponding item debit or event.
        let mut staged = self.clone();
        let creature_id = staged.spawn_creature(spawn)?;
        let creature = staged
            .creatures
            .get_mut(&creature_id)
            .ok_or(SimError::UnknownCreature)?;
        creature.friendly = if friendly { -1 } else { 0 };
        creature.pet = friendly && profile.is_pet;
        creature.deploying_owner = friendly.then_some(actor_id);
        if friendly {
            creature.faction_id = String::from("player");
        }
        creature.hp = deployed_hp;
        creature.ammunition = ammunition;
        let actor = staged
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?;
        actor.inventory = inventory;
        if actor
            .wielded
            .is_some_and(|id| !actor.inventory.contains_key(&id))
        {
            actor.wielded = None;
        }
        actor.worn.retain(|id| actor.inventory.contains_key(id));
        let event = staged.make_event(WorldEventKind::CreatureDeployed {
            actor_id,
            item_id,
            creature_id,
            position,
            friendly,
            pet: friendly && profile.is_pet,
            message,
        })?;
        *self = staged;
        events.push(event);
        Ok(true)
    }

    fn place_monster_candidate_positions(
        &self,
        center: WorldPosition,
        monster_type_id: &str,
    ) -> Vec<WorldPosition> {
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
        .filter(|position| self.can_place_deployed_creature(*position, monster_type_id))
        .collect()
    }

    fn can_place_deployed_creature(&self, position: WorldPosition, monster_type_id: &str) -> bool {
        let Some(prototype) = self.worldgen.as_ref().and_then(|catalog| {
            catalog.monster_prototypes.iter().find(|prototype| {
                prototype.base.monster_type_id == monster_type_id && prototype.runtime_spawnable
            })
        }) else {
            return false;
        };
        if prototype.base.size > cdda_protocol::CreatureSizeV1::Medium {
            // SMALL_PASSAGE is not yet canonical. This is intentionally
            // stricter than the pinned placement kernel rather than allowing
            // a large deployment to bypass `will_move_to`.
            return false;
        }
        self.is_passable(position)
            && self.actor_at(position).is_none()
            && self.creature_at(position).is_none()
            && self.npc_at(position).is_none()
            && !self.vehicle_blocks_actor_at(position)
            && (!prototype.base.path_settings.avoid_dangerous_fields
                || !self.fields_at(position).is_some_and(|fields| {
                    fields.iter().any(|field| {
                        self.field_types
                            .get(&field.field_type_id)
                            .and_then(|kind| {
                                kind.intensity_levels.get(usize::from(field.intensity - 1))
                            })
                            .is_some_and(|level| level.dangerous)
                    })
                }))
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
        u32::try_from(unbiased_inclusive_u64(
            rng,
            0,
            u64::from(intelligence_maximum),
        ))
        .map_err(|_| SimError::NumericOverflow)?
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
        u32::try_from(unbiased_inclusive_u64(
            rng,
            0,
            u64::from(difficulty_maximum),
        ))
        .map_err(|_| SimError::NumericOverflow)?
    };
    Ok(intelligence_roll
        .checked_mul(2)
        .and_then(|value| value.checked_add(skill_sum))
        .ok_or(SimError::NumericOverflow)?
        >= difficulty_roll
            .checked_mul(2)
            .ok_or(SimError::NumericOverflow)?)
}

fn unbiased_inclusive_u64(rng: &mut rand_chacha::ChaCha8Rng, minimum: u64, maximum: u64) -> u64 {
    if minimum >= maximum {
        return minimum;
    }
    let width = maximum - minimum + 1;
    let threshold = width.wrapping_neg() % width;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return minimum + value % width;
        }
    }
}

fn prototype_display_name(world: &WorldState, monster_type_id: &str) -> Option<String> {
    world
        .worldgen
        .as_ref()?
        .monster_prototypes
        .iter()
        .find_map(|prototype| {
            (prototype.base.monster_type_id == monster_type_id)
                .then(|| prototype.display_name.clone())
        })
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
