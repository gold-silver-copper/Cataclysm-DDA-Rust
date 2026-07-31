//! Mechanical item-source selection and traversal for `MGOAL_FIND_ITEM`.

use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ItemId, ItemPhaseV1, ItemSnapshot, MissionGoalV1, PLAYER_FACTION_ID, VehicleId,
    VehicleSnapshotV1, WorldPosition,
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
    source_positions: &[WorldPosition],
    goal: &MissionGoalV1,
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
        source_positions,
        item_type_id,
        *count_by_charges,
        *count,
    )
}

pub(super) fn consume_mission_items_from_sources(
    inventory: &mut BTreeMap<ItemId, ItemInstance>,
    worn: &mut Vec<ItemId>,
    wielded: &mut Option<ItemId>,
    vehicles: &mut BTreeMap<VehicleId, VehicleSnapshotV1>,
    source_positions: &[WorldPosition],
    item_type_id: &str,
    count_by_charges: bool,
    count: u32,
) -> Result<(), SimError> {
    let wanted = u64::from(count);
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
    let selected = selected.ok_or(SimError::InvalidMission)?;
    let mut remaining = i64::from(count);
    // `Character::consume_items` performs a preferred pass and then an
    // unrestricted pass. Each mixed pass draws from map before character.
    for preferred_only in [true, false] {
        if matches!(selected, MissionItemSource::Map | MissionItemSource::Both) {
            consume_vehicle_mission_items(
                vehicles,
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
    exact_consumption_result(remaining)
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
        if found >= limit {
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
    source_positions: &[WorldPosition],
    item_type_id: &str,
    count_by_charges: bool,
    limit: u64,
    preferred_only: bool,
) -> Result<u64, SimError> {
    let mut found = 0_u64;
    for position in source_positions {
        let Some(cargo) = selected_vehicle_cargo(vehicles, *position) else {
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ItemPocketCursor {
    Integral(usize),
    MagazineWell(usize),
    Container(usize),
}

fn charge_pocket_order(item: &ItemSnapshot) -> Vec<(u16, ItemPocketCursor)> {
    let mut order = item
        .integral_magazines
        .iter()
        .enumerate()
        .map(|(index, pocket)| (pocket.pocket_index, ItemPocketCursor::Integral(index)))
        .chain(
            item.magazine_wells
                .iter()
                .enumerate()
                .map(|(index, pocket)| {
                    (pocket.pocket_index, ItemPocketCursor::MagazineWell(index))
                }),
        )
        .chain(
            item.ammunition_containers
                .iter()
                .enumerate()
                .map(|(index, pocket)| (pocket.pocket_index, ItemPocketCursor::Container(index))),
        )
        .collect::<Vec<_>>();
    order.sort();
    order
}

fn consume_charge_nested_items(
    item: &mut ItemSnapshot,
    item_type_id: &str,
    preferred_only: bool,
    remaining: &mut i64,
) -> Result<(), SimError> {
    for (_pocket_index, cursor) in charge_pocket_order(item) {
        if *remaining == 0 {
            break;
        }
        match cursor {
            ItemPocketCursor::Integral(index) => {
                let pocket = item
                    .integral_magazines
                    .get_mut(index)
                    .ok_or(SimError::InvalidItem)?;
                let remove = pocket
                    .loaded_ammunition
                    .as_deref_mut()
                    .map(|nested| {
                        consume_item_snapshot(nested, item_type_id, true, preferred_only, remaining)
                    })
                    .transpose()?
                    .unwrap_or(false);
                if remove {
                    pocket.loaded_ammunition = None;
                }
            }
            ItemPocketCursor::MagazineWell(index) => {
                let well = item
                    .magazine_wells
                    .get_mut(index)
                    .ok_or(SimError::InvalidItem)?;
                let remove = well
                    .installed_magazine
                    .as_deref_mut()
                    .map(|nested| {
                        consume_item_snapshot(nested, item_type_id, true, preferred_only, remaining)
                    })
                    .transpose()?
                    .unwrap_or(false);
                if remove {
                    well.installed_magazine = None;
                }
            }
            ItemPocketCursor::Container(index) => {
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
            }
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
    let mut pockets = item
        .ammunition_containers
        .iter()
        .enumerate()
        .map(|(index, pocket)| (pocket.pocket_index, index))
        .collect::<Vec<_>>();
    pockets.sort();
    // Pinned `item::use_amount` reverses its top-down CONTAINER list so
    // removals happen bottom-up without invalidating parent references.
    for (_pocket_index, pocket_index) in pockets.into_iter().rev() {
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
        let Some(cargo) = selected_vehicle_cargo_mut(vehicles, *position) else {
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

pub(super) fn selected_vehicle_cargo(
    vehicles: &BTreeMap<VehicleId, VehicleSnapshotV1>,
    position: WorldPosition,
) -> Option<&[ItemSnapshot]> {
    // Pinned `veh_at(p).cargo()` selects one cargo feature. Vehicle stable-ID
    // order plus prototype part order is the explicit multiplayer tie-breaker
    // if corrupt/overlapping vehicle state exposes more than one candidate.
    vehicles.values().find_map(|vehicle| {
        vehicle
            .parts
            .iter()
            .find(|part| part.position == position && !part.cargo.is_empty())
            .map(|part| part.cargo.as_slice())
    })
}

fn selected_vehicle_cargo_mut(
    vehicles: &mut BTreeMap<VehicleId, VehicleSnapshotV1>,
    position: WorldPosition,
) -> Option<&mut Vec<ItemSnapshot>> {
    vehicles.values_mut().find_map(|vehicle| {
        vehicle
            .parts
            .iter_mut()
            .find(|part| part.position == position && !part.cargo.is_empty())
            .map(|part| &mut part.cargo)
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
        for (_pocket_index, cursor) in charge_pocket_order(item) {
            match cursor {
                ItemPocketCursor::Integral(index) => {
                    if let Some(nested) = item
                        .integral_magazines
                        .get(index)
                        .and_then(|pocket| pocket.loaded_ammunition.as_deref())
                    {
                        found = checked_nested_quantity(
                            found,
                            nested,
                            item_type_id,
                            true,
                            limit,
                            preferred_only,
                        )?;
                    }
                }
                ItemPocketCursor::MagazineWell(index) => {
                    if let Some(nested) = item
                        .magazine_wells
                        .get(index)
                        .and_then(|well| well.installed_magazine.as_deref())
                    {
                        found = checked_nested_quantity(
                            found,
                            nested,
                            item_type_id,
                            true,
                            limit,
                            preferred_only,
                        )?;
                    }
                }
                ItemPocketCursor::Container(index) => {
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
                }
            }
            if found >= limit {
                return Ok(found);
            }
        }
    } else {
        let mut pockets = item.ammunition_containers.iter().collect::<Vec<_>>();
        pockets.sort_by_key(|pocket| pocket.pocket_index);
        for pocket in pockets.into_iter().rev() {
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
