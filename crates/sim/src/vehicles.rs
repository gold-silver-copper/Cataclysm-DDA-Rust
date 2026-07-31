use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, ItemId, VehicleId, WorldEvent, WorldEventKind,
    WorldPosition, item_snapshot_containment_volume_milliliters,
};

use super::{
    ACTOR_ACTION_THRESHOLD, ItemInstance, MAX_ACTOR_INVENTORY_ITEMS, SimError, WorldState,
    construction_reserved_inventory_slots, craft_reserved_inventory_slots, roll_dice,
};

// Pinned `tileray` rounds `sin(angle) * 100` before stepping. Keeping the
// quarter-wave as integer canonical data avoids platform libm differences for
// arbitrary mapgen angles while reproducing the C++ 100-unit raster ray.
const SIN_DEGREES_TIMES_100: [i16; 91] = [
    0, 2, 3, 5, 7, 9, 10, 12, 14, 16, 17, 19, 21, 22, 24, 26, 28, 29, 31, 33, 34, 36, 37, 39, 41,
    42, 44, 45, 47, 48, 50, 52, 53, 54, 56, 57, 59, 60, 62, 63, 64, 66, 67, 68, 69, 71, 72, 73, 74,
    75, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 87, 88, 89, 90, 91, 91, 92, 93, 93, 94, 95, 95,
    96, 96, 97, 97, 97, 98, 98, 98, 99, 99, 99, 99, 100, 100, 100, 100, 100, 100,
];

fn sin_degrees_times_100(degrees: i16) -> i16 {
    let normalized = degrees.rem_euclid(360);
    let quadrant = normalized / 90;
    let within = usize::try_from(normalized % 90).expect("normalized degree index fits usize");
    match quadrant {
        0 => SIN_DEGREES_TIMES_100[within],
        1 => SIN_DEGREES_TIMES_100[90 - within],
        2 => -SIN_DEGREES_TIMES_100[within],
        _ => -SIN_DEGREES_TIMES_100[90 - within],
    }
}

/// Mechanical port of `vehicle::coord_translate` with a zero pivot. The C++
/// ray is cleared for each distinct mount, so the closed-form quotient is the
/// same as its repeated leftover accumulator without state crossing parts.
pub(crate) fn rotate_vehicle_mount(
    mount_x: i16,
    mount_y: i16,
    facing_degrees: i16,
) -> Result<(i32, i32), SimError> {
    let facing = facing_degrees.rem_euclid(360);
    let delta_x = i32::from(sin_degrees_times_100((90_i16 + facing).rem_euclid(360)));
    let delta_y = i32::from(sin_degrees_times_100(facing));
    let abs_x = delta_x.abs();
    let abs_y = delta_y.abs();
    let mostly_vertical = abs_x <= abs_y;
    let advance = i32::from(mount_x).unsigned_abs();
    let advance = i32::try_from(advance).map_err(|_| SimError::NumericOverflow)?;
    let (mut x, mut y) = if abs_x != 0 && abs_y != 0 {
        if mostly_vertical {
            (
                advance
                    .checked_mul(abs_x)
                    .and_then(|value| value.checked_div(abs_y))
                    .ok_or(SimError::NumericOverflow)?,
                advance,
            )
        } else {
            (
                advance,
                advance
                    .checked_mul(abs_y)
                    .and_then(|value| value.checked_div(abs_x))
                    .ok_or(SimError::NumericOverflow)?,
            )
        }
    } else if mostly_vertical {
        (0, advance)
    } else {
        (advance, 0)
    };
    const SX: [i32; 4] = [1, -1, -1, 1];
    const SY: [i32; 4] = [1, 1, -1, -1];
    let quadrant = usize::try_from(facing / 90).map_err(|_| SimError::NumericOverflow)?;
    x = x
        .checked_mul(SX[quadrant])
        .ok_or(SimError::NumericOverflow)?;
    y = y
        .checked_mul(SY[quadrant])
        .ok_or(SimError::NumericOverflow)?;
    if mount_x < 0 {
        x = x.checked_neg().ok_or(SimError::NumericOverflow)?;
        y = y.checked_neg().ok_or(SimError::NumericOverflow)?;
    }
    let orthogonal = i32::from(mount_y);
    if mostly_vertical {
        x = x
            .checked_add(
                orthogonal
                    .checked_mul(-SY[quadrant])
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)?;
    } else {
        y = y
            .checked_add(
                orthogonal
                    .checked_mul(SX[quadrant])
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)?;
    }
    Ok((x, y))
}

pub(crate) fn vehicle_part_position(
    origin: WorldPosition,
    mount_x: i16,
    mount_y: i16,
    facing_degrees: i16,
) -> Result<WorldPosition, SimError> {
    let (dx, dy) = rotate_vehicle_mount(mount_x, mount_y, facing_degrees)?;
    Ok(WorldPosition {
        x: origin.x.checked_add(dx).ok_or(SimError::NumericOverflow)?,
        y: origin.y.checked_add(dy).ok_or(SimError::NumericOverflow)?,
        z: origin.z,
    })
}

/// Pinned light-damage part HP branch from `vehicle::init_state`. Disabled
/// follow-up destruction is applied by the caller after this ordered roll.
pub(crate) fn initial_vehicle_part_hp(
    durability: u32,
    undamaged: bool,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> Result<u32, SimError> {
    if durability == 0 {
        return Err(SimError::NumericOverflow);
    }
    if undamaged {
        return Ok(durability);
    }
    let roll = roll_dice(rng, 4, 8)?;
    match roll {
        0..=8 => Ok(0),
        9..=19 => roll
            .checked_sub(8)
            .and_then(|numerator| numerator.checked_mul(durability))
            .map(|value| value / 12)
            .ok_or(SimError::NumericOverflow),
        _ => Ok(durability),
    }
}

fn contiguous_vehicle_axis(candidates: &mut [(usize, i16)], target_index: usize) -> Vec<usize> {
    candidates.sort_unstable_by(|(left_index, left), (right_index, right)| {
        right.cmp(left).then(left_index.cmp(right_index))
    });
    let Some(target_position) = candidates
        .iter()
        .position(|(index, _)| *index == target_index)
    else {
        return Vec::new();
    };
    let mut first = target_position;
    while first > 0 && candidates[first - 1].1.abs_diff(candidates[first].1) <= 1 {
        first -= 1;
    }
    let mut end = target_position + 1;
    while end < candidates.len() && candidates[end - 1].1.abs_diff(candidates[end].1) <= 1 {
        end += 1;
    }
    candidates[first..end]
        .iter()
        .map(|(index, _)| *index)
        .collect()
}

fn connected_openable_vehicle_parts(
    prototype: &cdda_protocol::WorldgenVehiclePrototypeV1,
    part_types: &[cdda_protocol::WorldgenVehiclePartTypeV1],
    target_index: usize,
) -> Result<Vec<usize>, SimError> {
    let target = prototype
        .parts
        .get(target_index)
        .ok_or(SimError::InvalidTerrain)?;
    let target_type = part_types
        .get(usize::from(target.part_type_index))
        .ok_or(SimError::InvalidTerrain)?;
    let is_multisquare = target_type
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("MULTISQUARE"))
        .is_ok();
    let mut connected = std::collections::BTreeSet::from([target_index]);
    if !is_multisquare {
        return Ok(connected.into_iter().collect());
    }
    let mut same_x = Vec::new();
    let mut same_y = Vec::new();
    for (index, candidate) in prototype.parts.iter().enumerate() {
        if candidate.part_type_index != target.part_type_index {
            continue;
        }
        let candidate_type = part_types
            .get(usize::from(candidate.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        if candidate_type
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("OPENABLE"))
            .is_err()
            || candidate_type
                .flags
                .binary_search_by(|flag| flag.as_str().cmp("MULTISQUARE"))
                .is_err()
        {
            continue;
        }
        if candidate.mount_x == target.mount_x {
            same_x.push((index, candidate.mount_y));
        }
        if candidate.mount_y == target.mount_y {
            same_y.push((index, candidate.mount_x));
        }
    }
    connected.extend(contiguous_vehicle_axis(&mut same_x, target_index));
    connected.extend(contiguous_vehicle_axis(&mut same_y, target_index));
    Ok(connected.into_iter().collect())
}

impl WorldState {
    pub(super) fn actor_is_boarded(&self, actor_id: ActorId) -> bool {
        self.vehicles.values().any(|vehicle| {
            vehicle
                .parts
                .iter()
                .any(|part| part.passenger == Some(actor_id))
        })
    }

    pub(super) fn vehicle_blocks_actor_at(&self, position: WorldPosition) -> bool {
        self.vehicles.values().any(|vehicle| {
            vehicle
                .parts
                .iter()
                .any(|part| part.hp > 0 && part.position == position)
        })
    }

    pub(super) fn vehicle_board_action_cost(&self) -> i64 {
        i64::from(ACTOR_ACTION_THRESHOLD)
    }

    pub(super) fn apply_set_vehicle_part_open(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        open: bool,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let Some(vehicle) = self.vehicles.get(&vehicle_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehicleMissing)?);
            return Ok(());
        };
        let target_index = usize::from(prototype_part_index);
        let Some(target_part) = vehicle.parts.get(target_index) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartMissing,
            )?);
            return Ok(());
        };
        if actor_position.z != target_part.position.z
            || actor_position.x.abs_diff(target_part.position.x) > 1
            || actor_position.y.abs_diff(target_part.position.y) > 1
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotHere)?);
            return Ok(());
        }
        let catalog = self.worldgen.as_ref().ok_or(SimError::InvalidTerrain)?;
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let target_prototype_part = prototype
            .parts
            .get(target_index)
            .ok_or(SimError::InvalidTerrain)?;
        let target_type = catalog
            .vehicle_part_types
            .get(usize::from(target_prototype_part.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        if target_part.hp == 0 {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehiclePartBroken)?);
            return Ok(());
        }
        if target_type
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("OPENABLE"))
            .is_err()
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartNotOpenable,
            )?);
            return Ok(());
        }
        let connected =
            connected_openable_vehicle_parts(prototype, &catalog.vehicle_part_types, target_index)?;
        if !open
            && connected.iter().any(|index| {
                let position = vehicle.parts[*index].position;
                self.actors
                    .values()
                    .any(|actor| actor.hp > 0 && actor.position == position)
                    || self
                        .creatures
                        .values()
                        .any(|creature| creature.hp > 0 && creature.position == position)
                    || self.npcs.values().any(|npc| npc.position == position)
            })
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartObstructed,
            )?);
            return Ok(());
        }
        let mut changed = Vec::new();
        let vehicle = self
            .vehicles
            .get_mut(&vehicle_id)
            .ok_or(SimError::InvalidTerrain)?;
        for index in connected {
            let part = vehicle
                .parts
                .get_mut(index)
                .ok_or(SimError::InvalidTerrain)?;
            if part.open == open {
                continue;
            }
            part.open = open;
            if open {
                part.locked = false;
            }
            changed.push((part.prototype_part_index, part.position));
        }
        for (prototype_part_index, position) in changed {
            events.push(self.make_event(WorldEventKind::VehiclePartOpenChanged {
                actor_id,
                vehicle_id,
                prototype_part_index,
                position,
                open,
            })?);
        }
        Ok(())
    }

    pub(super) fn try_open_vehicle_at_from_movement(
        &mut self,
        actor_id: ActorId,
        position: WorldPosition,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let Some(catalog) = self.worldgen.as_ref() else {
            return Ok(false);
        };
        let target = self.vehicles.iter().find_map(|(vehicle_id, vehicle)| {
            if !vehicle.owner_faction_id.is_empty() {
                return None;
            }
            let prototype = catalog
                .vehicle_prototypes
                .get(usize::from(vehicle.prototype_index))?;
            vehicle.parts.iter().enumerate().find_map(|(index, part)| {
                let prototype_part = prototype.parts.get(index)?;
                let part_type = catalog
                    .vehicle_part_types
                    .get(usize::from(prototype_part.part_type_index))?;
                (part.position == position
                    && part.hp > 0
                    && !part.open
                    && part_type
                        .flags
                        .binary_search_by(|flag| flag.as_str().cmp("OPENABLE"))
                        .is_ok())
                .then_some((*vehicle_id, index))
            })
        });
        let Some((vehicle_id, target_index)) = target else {
            return Ok(false);
        };
        let vehicle = self
            .vehicles
            .get(&vehicle_id)
            .ok_or(SimError::InvalidTerrain)?;
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let connected =
            connected_openable_vehicle_parts(prototype, &catalog.vehicle_part_types, target_index)?;
        let mut changed = Vec::new();
        let vehicle = self
            .vehicles
            .get_mut(&vehicle_id)
            .ok_or(SimError::InvalidTerrain)?;
        for index in connected {
            let part = vehicle
                .parts
                .get_mut(index)
                .ok_or(SimError::InvalidTerrain)?;
            if part.open {
                continue;
            }
            part.open = true;
            part.locked = false;
            changed.push((part.prototype_part_index, part.position));
        }
        for (prototype_part_index, position) in changed {
            events.push(self.make_event(WorldEventKind::VehiclePartOpenChanged {
                actor_id,
                vehicle_id,
                prototype_part_index,
                position,
                open: true,
            })?);
        }
        Ok(true)
    }

    pub(super) fn apply_board_vehicle(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        if self.actor_is_boarded(actor_id) {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::ActorAlreadyBoarded,
            )?);
            return Ok(());
        }
        let Some(vehicle) = self.vehicles.get(&vehicle_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehicleMissing)?);
            return Ok(());
        };
        let Some(part) = vehicle.parts.get(usize::from(prototype_part_index)) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartMissing,
            )?);
            return Ok(());
        };
        let Some(catalog) = self.worldgen.as_ref() else {
            return Err(SimError::InvalidTerrain);
        };
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let prototype_part = prototype
            .parts
            .get(usize::from(prototype_part_index))
            .ok_or(SimError::InvalidTerrain)?;
        let part_type = catalog
            .vehicle_part_types
            .get(usize::from(prototype_part.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        if part.hp == 0 {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehiclePartBroken)?);
            return Ok(());
        }
        if part_type
            .flags
            .binary_search_by(|candidate| candidate.as_str().cmp("BOARDABLE"))
            .is_err()
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartNotBoardable,
            )?);
            return Ok(());
        }
        if part.passenger.is_some() {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartOccupied,
            )?);
            return Ok(());
        }
        let from = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let to = part.position;
        if from.z != to.z
            || from.x.abs_diff(to.x) > 1
            || from.y.abs_diff(to.y) > 1
            || (from == to)
            || self
                .actors
                .iter()
                .any(|(id, actor)| *id != actor_id && actor.hp > 0 && actor.position == to)
            || self
                .creatures
                .values()
                .any(|creature| creature.hp > 0 && creature.position == to)
            || self.npcs.values().any(|npc| npc.position == to)
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
            return Ok(());
        }
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position = to;
        self.vehicles
            .get_mut(&vehicle_id)
            .and_then(|vehicle| vehicle.parts.get_mut(usize::from(prototype_part_index)))
            .ok_or(SimError::InvalidTerrain)?
            .passenger = Some(actor_id);
        events.push(self.make_event(WorldEventKind::ActorBoardedVehicle {
            actor_id,
            vehicle_id,
            prototype_part_index,
            position: to,
        })?);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_unboard_vehicle(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        dx: i8,
        dy: i8,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let Some(vehicle) = self.vehicles.get(&vehicle_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehicleMissing)?);
            return Ok(());
        };
        let Some(part) = vehicle.parts.get(usize::from(prototype_part_index)) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartMissing,
            )?);
            return Ok(());
        };
        if part.passenger != Some(actor_id) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ActorNotBoarded)?);
            return Ok(());
        }
        let from = part.position;
        let Some(to) = from.checked_offset(dx, dy, 0) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::InvalidUnboardDestination,
            )?);
            return Ok(());
        };
        if dx == 0 && dy == 0
            || dx.unsigned_abs() > 1
            || dy.unsigned_abs() > 1
            || !self.ensure_active_bubble_generated(to)?
            || !self.is_passable(to)
            || self.vehicle_blocks_actor_at(to)
            || self
                .actors
                .iter()
                .any(|(id, actor)| *id != actor_id && actor.hp > 0 && actor.position == to)
            || self.creature_at(to).is_some()
            || self.npc_at(to).is_some()
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::InvalidUnboardDestination,
            )?);
            return Ok(());
        }
        self.vehicles
            .get_mut(&vehicle_id)
            .and_then(|vehicle| vehicle.parts.get_mut(usize::from(prototype_part_index)))
            .ok_or(SimError::InvalidTerrain)?
            .passenger = None;
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position = to;
        events.push(self.make_event(WorldEventKind::ActorUnboardedVehicle {
            actor_id,
            vehicle_id,
            prototype_part_index,
            from,
            to,
        })?);
        Ok(())
    }

    pub(super) fn apply_take_vehicle_cargo(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let reserved_slots = actor
            .craft_activity
            .as_ref()
            .map_or(0, craft_reserved_inventory_slots)
            + usize::from(actor.disassembly_activity.is_some())
            + actor
                .construction_activity
                .as_ref()
                .map_or(0, construction_reserved_inventory_slots);
        if actor
            .inventory
            .len()
            .checked_add(reserved_slots)
            .is_none_or(|count| count >= MAX_ACTOR_INVENTORY_ITEMS)
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::InventoryFull)?);
            return Ok(());
        }
        let Some(vehicle) = self.vehicles.get(&vehicle_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehicleMissing)?);
            return Ok(());
        };
        let Some(part) = vehicle.parts.get(usize::from(prototype_part_index)) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartMissing,
            )?);
            return Ok(());
        };
        let Some(catalog) = self.worldgen.as_ref() else {
            return Err(SimError::InvalidTerrain);
        };
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let prototype_part = prototype
            .parts
            .get(usize::from(prototype_part_index))
            .ok_or(SimError::InvalidTerrain)?;
        let part_type = catalog
            .vehicle_part_types
            .get(usize::from(prototype_part.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        if part_type
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
            .is_err()
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartNotCargo,
            )?);
            return Ok(());
        }
        if part.locked {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehicleCargoLocked,
            )?);
            return Ok(());
        }
        if actor.position.z != part.position.z
            || actor.position.x.abs_diff(part.position.x) > 1
            || actor.position.y.abs_diff(part.position.y) > 1
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotHere)?);
            return Ok(());
        }
        let Some(cargo_index) = part.cargo.iter().position(|item| item.id == item_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemMissing)?);
            return Ok(());
        };
        let position = part.position;
        let snapshot = self
            .vehicles
            .get_mut(&vehicle_id)
            .and_then(|vehicle| vehicle.parts.get_mut(usize::from(prototype_part_index)))
            .ok_or(SimError::InvalidTerrain)?
            .cargo
            .remove(cargo_index);
        let item = ItemInstance::from_snapshot(&snapshot)?;
        if self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .inventory
            .insert(item_id, item)
            .is_some()
        {
            return Err(SimError::InvalidItem);
        }
        events.push(self.make_event(WorldEventKind::VehicleCargoTaken {
            actor_id,
            vehicle_id,
            prototype_part_index,
            item_id,
            position,
        })?);
        Ok(())
    }

    pub(super) fn apply_store_vehicle_cargo(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let Some(vehicle) = self.vehicles.get(&vehicle_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::VehicleMissing)?);
            return Ok(());
        };
        let Some(part) = vehicle.parts.get(usize::from(prototype_part_index)) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartMissing,
            )?);
            return Ok(());
        };
        let Some(catalog) = self.worldgen.as_ref() else {
            return Err(SimError::InvalidTerrain);
        };
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let prototype_part = prototype
            .parts
            .get(usize::from(prototype_part_index))
            .ok_or(SimError::InvalidTerrain)?;
        let part_type = catalog
            .vehicle_part_types
            .get(usize::from(prototype_part.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        if part_type
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
            .is_err()
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehiclePartNotCargo,
            )?);
            return Ok(());
        }
        if part.locked {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::VehicleCargoLocked,
            )?);
            return Ok(());
        }
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        if actor_position.z != part.position.z
            || actor_position.x.abs_diff(part.position.x) > 1
            || actor_position.y.abs_diff(part.position.y) > 1
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotHere)?);
            return Ok(());
        }
        if self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .worn
            .contains(&item_id)
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemWorn)?);
            return Ok(());
        }
        if part.cargo.len() >= cdda_protocol::MAX_VEHICLE_CARGO_ITEMS_PER_PART {
            events.push(self.rejection(actor_id, sequence, CommandRejection::InventoryFull)?);
            return Ok(());
        }
        let item_snapshot = self
            .actors
            .get(&actor_id)
            .and_then(|actor| actor.inventory.get(&item_id))
            .map(ItemInstance::snapshot);
        let Some(item_snapshot) = item_snapshot else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotOwned)?);
            return Ok(());
        };
        let used_volume = part.cargo.iter().try_fold(0_u64, |total, item| {
            total.checked_add(item_snapshot_containment_volume_milliliters(item)?)
        });
        let added_volume = item_snapshot_containment_volume_milliliters(&item_snapshot);
        if used_volume
            .zip(added_volume)
            .and_then(|(used, added)| used.checked_add(added))
            .is_none_or(|total| total > part_type.cargo_capacity_milliliters)
        {
            events.push(self.rejection(actor_id, sequence, CommandRejection::InventoryFull)?);
            return Ok(());
        }
        let actor = self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?;
        let Some(_item) = actor.inventory.remove(&item_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotOwned)?);
            return Ok(());
        };
        if actor.wielded == Some(item_id) {
            actor.wielded = None;
        }
        let position = part.position;
        self.vehicles
            .get_mut(&vehicle_id)
            .and_then(|vehicle| vehicle.parts.get_mut(usize::from(prototype_part_index)))
            .ok_or(SimError::InvalidTerrain)?
            .cargo
            .push(item_snapshot);
        events.push(self.make_event(WorldEventKind::VehicleCargoStored {
            actor_id,
            vehicle_id,
            prototype_part_index,
            item_id,
            position,
        })?);
        Ok(())
    }
}
