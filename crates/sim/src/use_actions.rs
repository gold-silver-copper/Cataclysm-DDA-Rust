//! Authoritative non-EOC item use actions.

use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, ItemId, ItemTransformTypeV1, SimTick, WorldEvent,
    WorldEventKind, item_transform_catalog_is_valid,
};

use crate::{ItemInstance, SimError, WorldState, validate_item_snapshot};

impl WorldState {
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
