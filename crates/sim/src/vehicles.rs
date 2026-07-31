use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, ItemId, VehicleId, WorldEvent, WorldEventKind,
    WorldPosition, item_snapshot_containment_volume_milliliters,
};

use super::{
    ACTOR_ACTION_THRESHOLD, ItemInstance, MAX_ACTOR_INVENTORY_ITEMS, SimError, WorldState,
    construction_reserved_inventory_slots, craft_reserved_inventory_slots, roll_dice,
};

fn vehicle_part_has_flag(part: &cdda_protocol::WorldgenVehiclePartTypeV1, flag: &str) -> bool {
    part.flags
        .binary_search_by(|candidate| candidate.as_str().cmp(flag))
        .is_ok()
}

pub(crate) fn vehicle_part_position(
    origin: WorldPosition,
    mount_x: i16,
    mount_y: i16,
    facing_degrees: i16,
) -> Result<WorldPosition, SimError> {
    cdda_protocol::expected_vehicle_part_position(origin, mount_x, mount_y, facing_degrees)
        .ok_or(SimError::NumericOverflow)
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
        9..=19 => u64::from(roll.checked_sub(8).ok_or(SimError::NumericOverflow)?)
            .checked_mul(u64::from(durability))
            .and_then(|value| u32::try_from(value / 12).ok())
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

pub(crate) fn connected_openable_vehicle_parts(
    prototype: &cdda_protocol::WorldgenVehiclePrototypeV1,
    part_types: &[cdda_protocol::WorldgenVehiclePartTypeV1],
    live_parts: Option<&[cdda_protocol::VehiclePartSnapshotV1]>,
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
        if live_parts.is_some_and(|parts| parts.get(index).is_none_or(|part| part.hp == 0)) {
            continue;
        }
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

    fn vehicle_allows_player_use(vehicle: &cdda_protocol::VehicleSnapshotV1) -> bool {
        vehicle.owner_faction_id == cdda_protocol::PLAYER_FACTION_ID
            || (vehicle.owner_faction_id.is_empty() && !vehicle.security_locked)
    }

    pub(super) fn clear_actor_passenger(&mut self, actor_id: ActorId) {
        for vehicle in self.vehicles.values_mut() {
            for part in &mut vehicle.parts {
                if part.passenger == Some(actor_id) {
                    part.passenger = None;
                }
            }
        }
    }

    /// Returns whether a live vehicle occupies the tile and its pinned vehicle
    /// movement cost. A live closed OBSTACLE is impassable, an AISLE costs 2,
    /// and another non-obstacle vehicle tile costs 8.
    pub(super) fn vehicle_movement_at(&self, position: WorldPosition) -> (bool, Option<i64>) {
        let Some(catalog) = self.worldgen.as_ref() else {
            return (false, None);
        };
        let mut occupied = false;
        let mut aisle = false;
        for vehicle in self.vehicles.values() {
            let Some(prototype) = catalog
                .vehicle_prototypes
                .get(usize::from(vehicle.prototype_index))
            else {
                return (true, None);
            };
            for (index, part) in vehicle.parts.iter().enumerate() {
                if part.hp == 0 || part.position != position {
                    continue;
                }
                occupied = true;
                let Some(prototype_part) = prototype.parts.get(index) else {
                    return (true, None);
                };
                let Some(part_type) = catalog
                    .vehicle_part_types
                    .get(usize::from(prototype_part.part_type_index))
                else {
                    return (true, None);
                };
                let obstacle = vehicle_part_has_flag(part_type, "OBSTACLE")
                    && !(vehicle_part_has_flag(part_type, "OPENABLE") && part.open);
                if obstacle {
                    return (true, None);
                }
                aisle |= vehicle_part_has_flag(part_type, "AISLE");
            }
        }
        (occupied, occupied.then_some(if aisle { 2 } else { 8 }))
    }

    pub(super) fn vehicle_blocks_actor_at(&self, position: WorldPosition) -> bool {
        matches!(self.vehicle_movement_at(position), (true, None))
    }

    pub(super) fn vehicle_is_opaque_at(&self, position: WorldPosition) -> bool {
        let Some(catalog) = self.worldgen.as_ref() else {
            return false;
        };
        self.vehicles.values().any(|vehicle| {
            let Some(prototype) = catalog
                .vehicle_prototypes
                .get(usize::from(vehicle.prototype_index))
            else {
                return true;
            };
            vehicle.parts.iter().enumerate().any(|(index, part)| {
                let Some(prototype_part) = prototype.parts.get(index) else {
                    return true;
                };
                let Some(part_type) = catalog
                    .vehicle_part_types
                    .get(usize::from(prototype_part.part_type_index))
                else {
                    return true;
                };
                if part.position != position
                    || part.hp == 0
                    || !vehicle_part_has_flag(part_type, "OPAQUE")
                {
                    return false;
                }
                !vehicle.parts.iter().enumerate().any(|(door_index, door)| {
                    let Some(door_prototype) = prototype.parts.get(door_index) else {
                        return false;
                    };
                    let Some(door_type) = catalog
                        .vehicle_part_types
                        .get(usize::from(door_prototype.part_type_index))
                    else {
                        return false;
                    };
                    door.hp > 0
                        && door.open
                        && door_prototype.mount_x == prototype_part.mount_x
                        && door_prototype.mount_y == prototype_part.mount_y
                        && vehicle_part_has_flag(door_type, "OPENABLE")
                })
            })
        })
    }

    /// Pinned field-contact vehicle predicates for the actor's exact tile.
    pub(super) fn actor_vehicle_context(&self, actor_id: ActorId) -> (bool, bool) {
        let Some(actor) = self.actors.get(&actor_id) else {
            return (false, false);
        };
        let Some(catalog) = self.worldgen.as_ref() else {
            return (false, false);
        };
        for vehicle in self.vehicles.values() {
            let Some(prototype) = catalog
                .vehicle_prototypes
                .get(usize::from(vehicle.prototype_index))
            else {
                continue;
            };
            let selected = vehicle
                .parts
                .iter()
                .position(|part| part.passenger == Some(actor_id))
                .or_else(|| {
                    vehicle
                        .parts
                        .iter()
                        .position(|part| part.hp > 0 && part.position == actor.position)
                })
                .or_else(|| {
                    vehicle
                        .parts
                        .iter()
                        .position(|part| part.position == actor.position)
                });
            let Some(selected) = selected else {
                continue;
            };
            let Some(selected_part) = vehicle.parts.get(selected) else {
                return (true, false);
            };
            let Some(selected_prototype) = prototype.parts.get(selected) else {
                return (true, false);
            };
            if selected_part.hp == 0 {
                return (true, false);
            }
            let mount = (selected_prototype.mount_x, selected_prototype.mount_y);
            let live_part_with_flag = |target: (i16, i16), flag: &str| {
                vehicle.parts.iter().enumerate().any(|(index, part)| {
                    let Some(prototype_part) = prototype.parts.get(index) else {
                        return false;
                    };
                    let Some(part_type) = catalog
                        .vehicle_part_types
                        .get(usize::from(prototype_part.part_type_index))
                    else {
                        return false;
                    };
                    part.hp > 0
                        && (prototype_part.mount_x, prototype_part.mount_y) == target
                        && vehicle_part_has_flag(part_type, flag)
                })
            };
            if !live_part_with_flag(mount, "ROOF") {
                return (true, false);
            }
            let inside = [(0_i16, -1_i16), (1, 0), (0, 1), (-1, 0)]
                .into_iter()
                .all(|(dx, dy)| {
                    let Some(target_x) = mount.0.checked_add(dx) else {
                        return false;
                    };
                    let Some(target_y) = mount.1.checked_add(dy) else {
                        return false;
                    };
                    let target = (target_x, target_y);
                    live_part_with_flag(target, "ROOF")
                        || vehicle.parts.iter().enumerate().any(|(index, part)| {
                            let Some(prototype_part) = prototype.parts.get(index) else {
                                return false;
                            };
                            let Some(part_type) = catalog
                                .vehicle_part_types
                                .get(usize::from(prototype_part.part_type_index))
                            else {
                                return false;
                            };
                            part.hp > 0
                                && (prototype_part.mount_x, prototype_part.mount_y) == target
                                && vehicle_part_has_flag(part_type, "OBSTACLE")
                                && !(vehicle_part_has_flag(part_type, "OPENABLE") && part.open)
                        })
                });
            return (true, inside);
        }
        (false, false)
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
        if !Self::vehicle_allows_player_use(vehicle) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
            return Ok(());
        }
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
        if open && target_part.locked {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
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
        let connected = connected_openable_vehicle_parts(
            prototype,
            &catalog.vehicle_part_types,
            Some(&vehicle.parts),
            target_index,
        )?;
        if open && connected.iter().any(|index| vehicle.parts[*index].locked) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
            return Ok(());
        }
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
            if !Self::vehicle_allows_player_use(vehicle) {
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
                    && !part.locked
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
        let connected = connected_openable_vehicle_parts(
            prototype,
            &catalog.vehicle_part_types,
            Some(&vehicle.parts),
            target_index,
        )?;
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
        if !Self::vehicle_allows_player_use(vehicle) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
            return Ok(());
        }
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
        // Pinned boarding attaches a character already standing on the
        // boardable tile; it is not a second movement/teleport primitive.
        if from != to
            || self.vehicle_blocks_actor_at(to)
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
        if !Self::vehicle_allows_player_use(vehicle) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
            return Ok(());
        }
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
        if part.hp == 0
            || part_type
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
        if !part.cargo[cargo_index].owner_faction_id.is_empty()
            && part.cargo[cargo_index].owner_faction_id != cdda_protocol::PLAYER_FACTION_ID
        {
            // Multiplayer adaptation shared with ordinary ground pickup:
            // witness and theft-consequence state is not represented, so a
            // foreign root cannot cross the authoritative inventory boundary.
            events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotOwned)?);
            return Ok(());
        }
        let position = part.position;
        let snapshot = self
            .vehicles
            .get_mut(&vehicle_id)
            .and_then(|vehicle| vehicle.parts.get_mut(usize::from(prototype_part_index)))
            .ok_or(SimError::InvalidTerrain)?
            .cargo
            .remove(cargo_index);
        let mut item = ItemInstance::from_snapshot(&snapshot)?;
        if item.owner_faction_id.is_empty() {
            item.set_owner_recursive(cdda_protocol::PLAYER_FACTION_ID);
        }
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
        if !Self::vehicle_allows_player_use(vehicle) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::Blocked)?);
            return Ok(());
        }
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
