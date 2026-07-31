use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, VehicleId, WorldEvent, WorldEventKind,
    WorldPosition,
};

use super::{ACTOR_ACTION_THRESHOLD, SimError, WorldState, roll_dice};

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
}
