//! Mechanical item-source selection and traversal for `MGOAL_FIND_ITEM`.

use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ItemId, ItemPhaseV1, ItemSnapshot, MissionGoalV1, PLAYER_FACTION_ID, SpawnPocketKindV1,
    VehicleId, VehicleSnapshotV1, WorldPosition, WorldgenCatalogV1,
};

use crate::{SimError, items::ItemInstance};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MissionItemSource {
    Player,
    Map,
    Both,
}

pub(super) fn consume_mission_items_from_preview(
    inventory: &mut BTreeMap<ItemId, ItemInstance>,
    worn: &mut Vec<ItemId>,
    wielded: &mut Option<ItemId>,
    vehicles: &mut BTreeMap<VehicleId, VehicleSnapshotV1>,
    worldgen: Option<&WorldgenCatalogV1>,
    source_positions: &[WorldPosition],
    goal: &MissionGoalV1,
    require_exact: bool,
) -> Result<(), SimError> {
    let MissionGoalV1::FindItem {
        item_type_id,
        count,
        count_by_charges,
    } = goal
    else {
        return Ok(());
    };
    consume_mission_items_from_sources(
        inventory,
        worn,
        wielded,
        vehicles,
        worldgen,
        source_positions,
        item_type_id,
        *count_by_charges,
        *count,
        require_exact,
    )
}

pub(super) fn consume_mission_items_from_sources(
    inventory: &mut BTreeMap<ItemId, ItemInstance>,
    worn: &mut Vec<ItemId>,
    wielded: &mut Option<ItemId>,
    vehicles: &mut BTreeMap<VehicleId, VehicleSnapshotV1>,
    worldgen: Option<&WorldgenCatalogV1>,
    source_positions: &[WorldPosition],
    item_type_id: &str,
    count_by_charges: bool,
    count: u32,
    require_exact: bool,
) -> Result<(), SimError> {
    let wanted = u64::from(count);
    let selected = if require_exact {
        let mut selected = None;
        for preferred_only in [true, false] {
            let player = actor_mission_item_quantity(
                inventory,
                item_type_id,
                count_by_charges,
                wanted,
                preferred_only,
            )?;
            let map = vehicle_mission_item_quantity(
                vehicles,
                worldgen,
                source_positions,
                item_type_id,
                count_by_charges,
                wanted,
                preferred_only,
            )?;
            selected = if player >= wanted {
                Some(MissionItemSource::Player)
            } else if map >= wanted {
                Some(MissionItemSource::Map)
            } else if player.checked_add(map).is_some_and(|total| total >= wanted) {
                Some(MissionItemSource::Both)
            } else {
                None
            };
            if selected.is_some() {
                break;
            }
        }
        selected.ok_or(SimError::InvalidMission)?
    } else {
        // Dynamic `finish_mission` calls pinned `mission::wrap_up` directly.
        // That path does not re-check completion and `consume_items` consumes
        // whatever is present, from map before character, even when short.
        MissionItemSource::Both
    };
    let mut remaining = i64::from(count);
    // `Character::consume_items` performs a preferred pass and then an
    // unrestricted pass. Each mixed pass draws from map before character.
    for preferred_only in [true, false] {
        if matches!(selected, MissionItemSource::Map | MissionItemSource::Both) {
            consume_vehicle_mission_items(
                vehicles,
                worldgen,
                source_positions,
                item_type_id,
                count_by_charges,
                preferred_only,
                &mut remaining,
            )?;
        }
        if remaining > 0
            && matches!(
                selected,
                MissionItemSource::Player | MissionItemSource::Both
            )
        {
            consume_actor_mission_items(
                inventory,
                worn,
                wielded,
                item_type_id,
                count_by_charges,
                preferred_only,
                &mut remaining,
            )?;
        }
    }
    if require_exact {
        exact_consumption_result(remaining)
    } else {
        Ok(())
    }
}

fn exact_consumption_result(remaining: i64) -> Result<(), SimError> {
    (remaining == 0)
        .then_some(())
        .ok_or(SimError::InvalidMission)
}

fn actor_mission_item_quantity(
    inventory: &BTreeMap<ItemId, ItemInstance>,
    item_type_id: &str,
    count_by_charges: bool,
    limit: u64,
    preferred_only: bool,
) -> Result<u64, SimError> {
    inventory.values().try_fold(0_u64, |found, item| {
        if found >= limit || !mission_item_is_available_to_player(&item.owner_faction_id) {
            return Ok(found);
        }
        found
            .checked_add(item_instance_quantity(
                item,
                item_type_id,
                count_by_charges,
                limit - found,
                preferred_only,
            )?)
            .ok_or(SimError::NumericOverflow)
    })
}

fn vehicle_mission_item_quantity(
    vehicles: &BTreeMap<VehicleId, VehicleSnapshotV1>,
    worldgen: Option<&WorldgenCatalogV1>,
    source_positions: &[WorldPosition],
    item_type_id: &str,
    count_by_charges: bool,
    limit: u64,
    preferred_only: bool,
) -> Result<u64, SimError> {
    let mut found = 0_u64;
    for position in source_positions {
        let Some(cargo) = selected_vehicle_cargo(vehicles, worldgen, *position) else {
            continue;
        };
        for item in cargo {
            if !mission_item_is_available_to_player(&item.owner_faction_id) {
                continue;
            }
            found = found
                .checked_add(item_snapshot_quantity(
                    item,
                    item_type_id,
                    count_by_charges,
                    limit - found,
                    preferred_only,
                )?)
                .ok_or(SimError::NumericOverflow)?;
            if found >= limit {
                return Ok(found);
            }
        }
    }
    Ok(found)
}

fn ordered_actor_item_ids(
    inventory: &BTreeMap<ItemId, ItemInstance>,
    worn: &[ItemId],
    wielded: Option<ItemId>,
) -> Vec<ItemId> {
    let mut ordered = Vec::with_capacity(inventory.len());
    let mut seen = BTreeSet::new();
    if let Some(wielded) = wielded.filter(|id| inventory.contains_key(id)) {
        ordered.push(wielded);
        seen.insert(wielded);
    }
    for id in worn {
        if inventory.contains_key(id) && seen.insert(*id) {
            ordered.push(*id);
        }
    }
    // The remaining canonical inventory tier has no independent upstream
    // insertion-order representation, so its stable ItemId order is retained.
    ordered.extend(inventory.keys().filter(|id| seen.insert(**id)).copied());
    ordered
}

fn consume_actor_mission_items(
    inventory: &mut BTreeMap<ItemId, ItemInstance>,
    worn: &mut Vec<ItemId>,
    wielded: &mut Option<ItemId>,
    item_type_id: &str,
    count_by_charges: bool,
    preferred_only: bool,
    remaining: &mut i64,
) -> Result<(), SimError> {
    for id in ordered_actor_item_ids(inventory, worn, *wielded) {
        if *remaining == 0 {
            break;
        }
        if inventory
            .get(&id)
            .is_none_or(|item| !mission_item_is_available_to_player(&item.owner_faction_id))
        {
            continue;
        }
        let mut snapshot = inventory.get(&id).ok_or(SimError::InvalidItem)?.snapshot();
        let remove = consume_item_snapshot(
            &mut snapshot,
            item_type_id,
            count_by_charges,
            preferred_only,
            remaining,
        )?;
        if remove {
            inventory.remove(&id).ok_or(SimError::InvalidItem)?;
            worn.retain(|worn| *worn != id);
            if *wielded == Some(id) {
                *wielded = None;
            }
        } else {
            inventory.insert(id, ItemInstance::from_snapshot(&snapshot)?);
        }
    }
    Ok(())
}

fn consume_item_snapshot(
    item: &mut ItemSnapshot,
    item_type_id: &str,
    count_by_charges: bool,
    preferred_only: bool,
    remaining: &mut i64,
) -> Result<bool, SimError> {
    if count_by_charges {
        if mission_item_candidate(item, item_type_id, true, preferred_only)
            && consume_matching_item(&mut item.charges, true, remaining)?
        {
            return Ok(true);
        }
        consume_charge_nested_items(item, item_type_id, preferred_only, remaining)?;
        Ok(false)
    } else {
        // Preserve the hostile/recovered-instance fail-closed decision from
        // before traversal. Removing a nested target must not make its former
        // container newly eligible during the same consumption pass.
        let consume_root = mission_item_candidate(item, item_type_id, false, preferred_only);
        consume_amount_container_items(item, item_type_id, preferred_only, remaining)?;
        if consume_root {
            consume_matching_item(&mut item.charges, false, remaining)
        } else {
            Ok(false)
        }
    }
}

fn consume_matching_item(
    charges: &mut i32,
    count_by_charges: bool,
    remaining: &mut i64,
) -> Result<bool, SimError> {
    if *remaining == 0 {
        return Ok(false);
    }
    if !count_by_charges {
        *remaining -= 1;
        return Ok(true);
    }
    let available = i64::from((*charges).max(0));
    if available == 0 {
        return Ok(false);
    }
    let consumed = (*remaining).min(available);
    *charges = i32::try_from(available - consumed).map_err(|_| SimError::NumericOverflow)?;
    *remaining -= consumed;
    Ok(*charges == 0)
}

fn container_pocket_order(item: &ItemSnapshot) -> Vec<(u16, usize)> {
    let mut order = item
        .ammunition_containers
        .iter()
        .enumerate()
        .filter(|(_index, pocket)| {
            pocket
                .spawn_state
                .as_ref()
                .is_some_and(|state| state.rules.kind == SpawnPocketKindV1::Container)
        })
        .map(|(index, pocket)| (pocket.pocket_index, index))
        .collect::<Vec<_>>();
    order.sort();
    order
}

fn unseal_changed_container_pocket(item: &mut ItemSnapshot, index: usize) {
    if let Some(state) = item
        .ammunition_containers
        .get_mut(index)
        .and_then(|pocket| pocket.spawn_state.as_mut())
        .filter(|state| state.rules.kind == SpawnPocketKindV1::Container)
    {
        state.sealed = false;
    }
    // The current canonical invariant represents upstream's all-pockets seal
    // state. If one changed pocket makes that aggregate non-full, retain no
    // impossible sealed sibling that recovery would reject.
    if !cdda_protocol::item_snapshot_sealing_is_valid(item) {
        for pocket in &mut item.ammunition_containers {
            if let Some(state) = pocket
                .spawn_state
                .as_mut()
                .filter(|state| state.rules.kind == SpawnPocketKindV1::Container)
            {
                state.sealed = false;
            }
        }
    }
}

fn consume_charge_nested_items(
    item: &mut ItemSnapshot,
    item_type_id: &str,
    preferred_only: bool,
    remaining: &mut i64,
) -> Result<(), SimError> {
    for (_pocket_index, index) in container_pocket_order(item) {
        if *remaining == 0 {
            break;
        }
        let before = *remaining;
        let contents = &mut item
            .ammunition_containers
            .get_mut(index)
            .ok_or(SimError::InvalidItem)?
            .contents;
        let mut content_index = 0;
        while content_index < contents.len() && *remaining > 0 {
            if consume_item_snapshot(
                &mut contents[content_index],
                item_type_id,
                true,
                preferred_only,
                remaining,
            )? {
                contents.remove(content_index);
            } else {
                content_index += 1;
            }
        }
        if *remaining != before {
            unseal_changed_container_pocket(item, index);
        }
    }
    Ok(())
}

fn consume_amount_container_items(
    item: &mut ItemSnapshot,
    item_type_id: &str,
    preferred_only: bool,
    remaining: &mut i64,
) -> Result<(), SimError> {
    let pockets = container_pocket_order(item);
    // Pinned `item::use_amount` reverses its top-down CONTAINER list so
    // removals happen bottom-up without invalidating parent references.
    for (_pocket_index, pocket_index) in pockets.into_iter().rev() {
        let before = *remaining;
        let contents = &mut item
            .ammunition_containers
            .get_mut(pocket_index)
            .ok_or(SimError::InvalidItem)?
            .contents;
        let mut index = contents.len();
        while index > 0 && *remaining > 0 {
            index -= 1;
            if consume_item_snapshot(
                &mut contents[index],
                item_type_id,
                false,
                preferred_only,
                remaining,
            )? {
                contents.remove(index);
            }
        }
        if *remaining != before {
            unseal_changed_container_pocket(item, pocket_index);
        }
    }
    Ok(())
}

fn mission_item_candidate(
    item: &ItemSnapshot,
    item_type_id: &str,
    count_by_charges: bool,
    preferred_only: bool,
) -> bool {
    item.type_id == item_type_id
        && item.containment.count_by_charges == count_by_charges
        && item.containment.phase == ItemPhaseV1::Solid
        // Runtime mission admission limits goals to simple target types. Keep
        // hostile/recovered instances fail-closed too: consuming a target with
        // contents would require remove_ammo/spill branches not yet modeled.
        && cdda_protocol::item_snapshot_has_no_contained_items(item)
        && (!preferred_only || item.ammunition_containers.iter().all(|p| p.contents.is_empty()))
}

fn consume_vehicle_mission_items(
    vehicles: &mut BTreeMap<VehicleId, VehicleSnapshotV1>,
    worldgen: Option<&WorldgenCatalogV1>,
    source_positions: &[WorldPosition],
    item_type_id: &str,
    count_by_charges: bool,
    preferred_only: bool,
    remaining: &mut i64,
) -> Result<(), SimError> {
    for position in source_positions {
        if *remaining == 0 {
            break;
        }
        let Some(cargo) = selected_vehicle_cargo_mut(vehicles, worldgen, *position) else {
            continue;
        };
        let mut index = 0;
        while index < cargo.len() && *remaining > 0 {
            if !mission_item_is_available_to_player(&cargo[index].owner_faction_id) {
                index += 1;
                continue;
            }
            if consume_item_snapshot(
                &mut cargo[index],
                item_type_id,
                count_by_charges,
                preferred_only,
                remaining,
            )? {
                cargo.remove(index);
            } else {
                index += 1;
            }
        }
    }
    Ok(())
}

pub(super) fn selected_vehicle_cargo<'a>(
    vehicles: &'a BTreeMap<VehicleId, VehicleSnapshotV1>,
    worldgen: Option<&WorldgenCatalogV1>,
    position: WorldPosition,
) -> Option<&'a [ItemSnapshot]> {
    let worldgen = worldgen?;
    // Pinned `veh_at(p).cargo()` selects one cargo feature. Vehicle stable-ID
    // order plus prototype part order is the explicit multiplayer tie-breaker
    // if corrupt/overlapping vehicle state exposes more than one candidate.
    vehicles.values().find_map(|vehicle| {
        let prototype = worldgen
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))?;
        vehicle.parts.iter().find_map(|part| {
            let prototype_part = prototype
                .parts
                .get(usize::from(part.prototype_part_index))?;
            let part_type = worldgen
                .vehicle_part_types
                .get(usize::from(prototype_part.part_type_index))?;
            (part.position == position
                && part.hp > 0
                && part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
                    .is_ok())
            .then(|| part.cargo.as_slice())
        })
    })
}

fn selected_vehicle_cargo_mut<'a>(
    vehicles: &'a mut BTreeMap<VehicleId, VehicleSnapshotV1>,
    worldgen: Option<&WorldgenCatalogV1>,
    position: WorldPosition,
) -> Option<&'a mut Vec<ItemSnapshot>> {
    let worldgen = worldgen?;
    vehicles.values_mut().find_map(|vehicle| {
        let prototype = worldgen
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))?;
        vehicle.parts.iter_mut().find_map(|part| {
            let prototype_part = prototype
                .parts
                .get(usize::from(part.prototype_part_index))?;
            let part_type = worldgen
                .vehicle_part_types
                .get(usize::from(prototype_part.part_type_index))?;
            (part.position == position
                && part.hp > 0
                && part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
                    .is_ok())
            .then_some(&mut part.cargo)
        })
    })
}

fn mission_item_is_available_to_player(owner_faction_id: &str) -> bool {
    owner_faction_id.is_empty() || owner_faction_id == PLAYER_FACTION_ID
}

pub(super) fn item_instance_quantity(
    item: &ItemInstance,
    item_type_id: &str,
    count_by_charges: bool,
    limit: u64,
    preferred_only: bool,
) -> Result<u64, SimError> {
    item_snapshot_quantity(
        &item.snapshot(),
        item_type_id,
        count_by_charges,
        limit,
        preferred_only,
    )
}

pub(super) fn item_snapshot_quantity(
    item: &ItemSnapshot,
    item_type_id: &str,
    count_by_charges: bool,
    limit: u64,
    preferred_only: bool,
) -> Result<u64, SimError> {
    if limit == 0 {
        return Ok(0);
    }
    let mut found = 0_u64;
    if count_by_charges {
        if mission_item_candidate(item, item_type_id, true, preferred_only) {
            found = u64::try_from(item.charges.max(0)).map_err(|_| SimError::NumericOverflow)?;
            found = found.min(limit);
        }
        for (_pocket_index, index) in container_pocket_order(item) {
            if let Some(pocket) = item.ammunition_containers.get(index) {
                for nested in &pocket.contents {
                    found = checked_nested_quantity(
                        found,
                        nested,
                        item_type_id,
                        true,
                        limit,
                        preferred_only,
                    )?;
                    if found >= limit {
                        break;
                    }
                }
            }
            if found >= limit {
                return Ok(found);
            }
        }
    } else {
        for (_pocket_index, pocket_index) in container_pocket_order(item).into_iter().rev() {
            let pocket = item
                .ammunition_containers
                .get(pocket_index)
                .ok_or(SimError::InvalidItem)?;
            for nested in pocket.contents.iter().rev() {
                found = checked_nested_quantity(
                    found,
                    nested,
                    item_type_id,
                    false,
                    limit,
                    preferred_only,
                )?;
                if found >= limit {
                    return Ok(found);
                }
            }
        }
        if mission_item_candidate(item, item_type_id, false, preferred_only) {
            found = found.checked_add(1).ok_or(SimError::NumericOverflow)?;
        }
    }
    Ok(found.min(limit))
}

fn checked_nested_quantity(
    found: u64,
    nested: &ItemSnapshot,
    item_type_id: &str,
    count_by_charges: bool,
    limit: u64,
    preferred_only: bool,
) -> Result<u64, SimError> {
    found
        .checked_add(item_snapshot_quantity(
            nested,
            item_type_id,
            count_by_charges,
            limit - found,
            preferred_only,
        )?)
        .ok_or(SimError::NumericOverflow)
}
