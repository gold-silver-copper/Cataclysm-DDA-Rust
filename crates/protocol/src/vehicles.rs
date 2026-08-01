use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ActorId, ItemGroupItemPrototypeV1, ItemSnapshot, VehicleId, WORLDGEN_OMT_SIZE, WorldPosition,
    WorldgenCoordinateRangeV1, WorldgenU16RangeV1, item_snapshot_containment_volume_milliliters,
    valid_item_snapshot,
};

pub const MAX_WORLDGEN_VEHICLE_PART_TYPES: usize = 16_384;
pub const MAX_WORLDGEN_VEHICLE_PROTOTYPES: usize = 8_192;
pub const MAX_WORLDGEN_VEHICLE_GROUPS: usize = 8_192;
pub const MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE: usize = 4_096;
pub const MAX_WORLDGEN_VEHICLE_PROTOTYPE_PARTS_TOTAL: usize = 262_144;
pub const MAX_WORLDGEN_VEHICLE_GROUP_ENTRIES: usize = 4_096;
pub const MAX_WORLDGEN_VEHICLE_GROUP_ENTRIES_TOTAL: usize = 65_536;
pub const MAX_WORLDGEN_VEHICLE_PLACEMENTS: usize = 65_536;
pub const MAX_WORLDGEN_VEHICLE_ROTATIONS: usize = 64;
pub const MAX_WORLDGEN_VEHICLE_REPEAT: u16 = 1_024;
pub const MAX_WORLDGEN_VEHICLE_PART_FLAGS: usize = 256;
pub const MAX_WORLDGEN_VEHICLE_PART_VARIANTS: usize = 256;
pub const MAX_WORLDGEN_VEHICLE_PART_AMMO_TYPES: usize = 256;
pub const MAX_WORLDGEN_VEHICLE_PART_TOOLS: usize = 256;
pub const MAX_WORLDGEN_VEHICLE_TEXT_BYTES: usize = 512;
pub const MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES: usize = 32;
pub const MAX_LIVE_VEHICLES: usize = 65_536;
pub const MAX_WORLDGEN_VEHICLE_ITEM_SPAWNS: usize = 4_096;
pub const MAX_WORLDGEN_VEHICLE_ITEMS_PER_SPAWN: usize = 4_096;
pub const MAX_VEHICLE_CARGO_ITEMS_PER_PART: usize = 4_096;
pub const MAX_VEHICLE_CARGO_VOLUME_MILLILITERS: u64 = 10_000_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehiclePartVariantV1 {
    /// Empty is the pinned default variant ID.
    pub variant_id: String,
    /// Exact pinned directional symbol string. Runtime rendering selects the
    /// direction; canonical content must not collapse it to one glyph.
    pub symbols: String,
    pub broken_symbols: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehiclePartTypeV1 {
    pub part_type_id: String,
    pub name: String,
    pub item_type_id: String,
    pub location: String,
    pub durability: u32,
    pub cargo_capacity_milliliters: u64,
    pub power_milliwatts: i64,
    /// Empty means no fuel type. The first ordinary movement family admits
    /// only exact `muscle` engines and retains other types fail-closed.
    pub fuel_type: String,
    pub muscle_power_factor_milliwatts: i64,
    pub rolling_resistance_millionths: u32,
    pub contact_area: u32,
    pub wheel_offroad_rating_millionths: u32,
    pub wheel_terrain_modifiers_json: String,
    /// Finalized, sorted upstream flags. Runtime families admit only the flags
    /// whose behavior they implement, while canonical content retains the
    /// exact set needed by later families.
    pub flags: Vec<String>,
    /// Exact finalized source order; the empty ID is the default variant.
    pub variants: Vec<WorldgenVehiclePartVariantV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehiclePrototypePartV1 {
    pub mount_x: i16,
    pub mount_y: i16,
    pub part_type_index: u16,
    pub variant_id: String,
    /// Empty means the prototype did not explicitly initialize tank contents.
    pub fuel_item_type_id: String,
    pub with_ammo_percent: u8,
    /// Exact source order is retained because a future spawn-state kernel may
    /// consume randomness while selecting from this list.
    pub ammo_type_ids: Vec<String>,
    /// `(-1, -1)` is pinned unspecified quantity; otherwise both bounds are
    /// nonnegative and inclusive.
    pub ammo_quantity_minimum: i32,
    pub ammo_quantity_maximum: i32,
    pub tool_item_type_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehicleDirectItemSpawnV1 {
    pub item: ItemGroupItemPrototypeV1,
    /// Empty means the pinned default constructor selection.
    pub variant_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehicleItemSpawnV1 {
    pub mount_x: i16,
    pub mount_y: i16,
    /// First pinned prototype part at the mount with the `CARGO` feature.
    pub cargo_prototype_part_index: u16,
    pub chance_percent: u8,
    pub with_magazine_percent: u8,
    pub with_ammo_percent: u8,
    /// Exact source order. Pinned spawning emits these before item groups.
    pub direct_items: Vec<WorldgenVehicleDirectItemSpawnV1>,
    pub item_group_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehiclePrototypeV1 {
    pub prototype_id: String,
    pub name: String,
    /// Exact pinned installation order. Vehicle initialization consumes RNG in
    /// this order, so this collection must never be sorted by mount or type.
    pub parts: Vec<WorldgenVehiclePrototypePartV1>,
    pub item_spawns: Vec<WorldgenVehicleItemSpawnV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehicleGroupEntryV1 {
    pub prototype_index: u16,
    pub weight: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehicleGroupV1 {
    pub group_id: String,
    /// Exact pinned append order, including each prototype's implicit same-ID
    /// group entry before explicit vehicle-group entries.
    pub entries: Vec<WorldgenVehicleGroupEntryV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum VehicleSpawnStatusV1 {
    DefaultLightDamage,
    Undamaged,
    Disabled,
    Pristine,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenVehiclePlacementV1 {
    pub group_index: u16,
    pub chance_percent: u8,
    /// Exact normalized source order. Production data contains non-cardinal
    /// angles, so the representation must not narrow this to four facings.
    pub rotations_degrees: Vec<i16>,
    /// Pinned `-1` means randomized initial fuel; 0 through 100 is explicit.
    pub fuel_percent: i16,
    pub status: VehicleSpawnStatusV1,
    /// Empty means no faction owner.
    pub faction_id: String,
    pub repeat: WorldgenU16RangeV1,
    pub x: WorldgenCoordinateRangeV1,
    pub y: WorldgenCoordinateRangeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehiclePartSnapshotV1 {
    /// Index into the owning prototype's exact ordered part list.
    pub prototype_part_index: u16,
    /// Authoritative transformed tile. This is retained because pinned
    /// tileray placement supports arbitrary angles and is recovery-critical.
    pub position: WorldPosition,
    pub hp: u32,
    pub enabled: bool,
    pub open: bool,
    pub locked: bool,
    pub passenger: Option<ActorId>,
    /// Source-ordered vehicle-owned cargo with stable nested item identities.
    pub cargo: Vec<ItemSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehicleSnapshotV1 {
    pub id: VehicleId,
    pub prototype_index: u16,
    pub origin: WorldPosition,
    pub facing_degrees: i16,
    /// Pinned 15-degree steering target. This remains distinct from the
    /// vehicle's current facing until an authoritative movement succeeds.
    pub turn_direction_degrees: i16,
    /// Error accumulator for the persistent pinned `tileray` used to advance
    /// non-cardinal movement without platform floating-point drift.
    pub movement_ray_leftover: u16,
    /// Empty means no faction owner.
    pub owner_faction_id: String,
    /// Pinned global ignition state, distinct from per-part enablement.
    pub engine_on: bool,
    /// Pinned global security state set by an available SECURITY part.
    pub security_locked: bool,
    /// Exact prototype order. Removed-part compaction is outside this version;
    /// broken parts remain present with zero HP.
    pub parts: Vec<VehiclePartSnapshotV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleVehicleTileV1 {
    pub prototype_part_index: u16,
    pub position: WorldPosition,
    pub name: String,
    pub symbol: String,
    pub hp: u32,
    pub maximum_hp: u32,
    pub open: bool,
    /// Authoritative boardable part at this displayed tile, if one is live.
    pub boardable_prototype_part_index: Option<u16>,
    /// Server-selected live `OPENABLE` part at this displayed tile.
    pub openable_prototype_part_index: Option<u16>,
    /// Adjacent, unlocked cargo boundary selected by the server, if present.
    pub cargo_prototype_part_index: Option<u16>,
    pub passenger: Option<ActorId>,
    pub cargo: Vec<ItemSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleVehicleSnapshotV1 {
    pub id: VehicleId,
    pub prototype_id: String,
    pub name: String,
    pub origin: WorldPosition,
    pub facing_degrees: i16,
    /// Position-sorted, one server-selected displayed part per occupied tile.
    /// Hidden parts and private mechanical state are omitted.
    pub tiles: Vec<VisibleVehicleTileV1>,
}

fn valid_text(value: &str, allow_empty: bool, maximum_bytes: usize) -> bool {
    (allow_empty || !value.is_empty())
        && value.len() <= maximum_bytes
        && !value.chars().any(char::is_control)
}

fn valid_id(value: &str) -> bool {
    valid_text(value, false, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
}

fn valid_optional_id(value: &str) -> bool {
    valid_text(value, true, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
}

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

/// Advances the pinned infinite vehicle `tileray` by one tile. The returned
/// leftover must be persisted because repeated 15-degree steps deliberately
/// alternate between cardinal and diagonal deltas.
#[must_use]
pub fn advance_vehicle_tileray(
    direction_degrees: i16,
    leftover: u16,
    reverse: bool,
) -> Option<(i8, i8, u16)> {
    let direction = direction_degrees.rem_euclid(360);
    let delta_x = i32::from(sin_degrees_times_100((90_i16 + direction).rem_euclid(360)));
    let delta_y = i32::from(sin_degrees_times_100(direction));
    let abs_x = delta_x.unsigned_abs();
    let abs_y = delta_y.unsigned_abs();
    let major = abs_x.max(abs_y);
    if major == 0 || u32::from(leftover) >= major {
        return None;
    }
    let mut next_leftover = u32::from(leftover);
    let (mut dx, mut dy) = if abs_x <= abs_y {
        next_leftover = next_leftover.checked_add(abs_x)?;
        let minor = u8::from(next_leftover >= abs_y);
        if minor != 0 {
            next_leftover = next_leftover.checked_sub(abs_y)?;
        }
        (i8::try_from(minor).ok()?, 1_i8)
    } else {
        next_leftover = next_leftover.checked_add(abs_y)?;
        let minor = u8::from(next_leftover >= abs_x);
        if minor != 0 {
            next_leftover = next_leftover.checked_sub(abs_x)?;
        }
        (1_i8, i8::try_from(minor).ok()?)
    };
    let quadrant = usize::try_from(direction / 90).ok()?;
    const SX: [i8; 4] = [1, -1, -1, 1];
    const SY: [i8; 4] = [1, 1, -1, -1];
    dx = dx.checked_mul(SX[quadrant])?;
    dy = dy.checked_mul(SY[quadrant])?;
    if reverse {
        dx = dx.checked_neg()?;
        dy = dy.checked_neg()?;
    }
    Some((dx, dy, u16::try_from(next_leftover).ok()?))
}

fn valid_vehicle_tileray_leftover(direction_degrees: i16, leftover: u16) -> bool {
    let direction = direction_degrees.rem_euclid(360);
    let abs_x =
        i32::from(sin_degrees_times_100((90_i16 + direction).rem_euclid(360))).unsigned_abs();
    let abs_y = i32::from(sin_degrees_times_100(direction)).unsigned_abs();
    u32::from(leftover) < abs_x.max(abs_y)
}

/// Canonical zero-pivot `vehicle::coord_translate` used both while creating a
/// vehicle and while validating recovered authoritative geometry.
#[must_use]
pub fn expected_vehicle_part_position(
    origin: WorldPosition,
    mount_x: i16,
    mount_y: i16,
    facing_degrees: i16,
) -> Option<WorldPosition> {
    let facing = facing_degrees.rem_euclid(360);
    let delta_x = i32::from(sin_degrees_times_100((90_i16 + facing).rem_euclid(360)));
    let delta_y = i32::from(sin_degrees_times_100(facing));
    let (abs_x, abs_y) = (delta_x.abs(), delta_y.abs());
    let mostly_vertical = abs_x <= abs_y;
    let advance = i32::from(mount_x).unsigned_abs();
    let advance = i32::try_from(advance).ok()?;
    let (mut x, mut y) = if abs_x != 0 && abs_y != 0 {
        if mostly_vertical {
            (advance.checked_mul(abs_x)?.checked_div(abs_y)?, advance)
        } else {
            (advance, advance.checked_mul(abs_y)?.checked_div(abs_x)?)
        }
    } else if mostly_vertical {
        (0, advance)
    } else {
        (advance, 0)
    };
    const SX: [i32; 4] = [1, -1, -1, 1];
    const SY: [i32; 4] = [1, 1, -1, -1];
    let quadrant = usize::try_from(facing / 90).ok()?;
    x = x.checked_mul(SX[quadrant])?;
    y = y.checked_mul(SY[quadrant])?;
    if mount_x < 0 {
        x = x.checked_neg()?;
        y = y.checked_neg()?;
    }
    let orthogonal = i32::from(mount_y);
    if mostly_vertical {
        x = x.checked_add(orthogonal.checked_mul(-SY[quadrant])?)?;
    } else {
        y = y.checked_add(orthogonal.checked_mul(SX[quadrant])?)?;
    }
    Some(WorldPosition {
        x: origin.x.checked_add(x)?,
        y: origin.y.checked_add(y)?,
        z: origin.z,
    })
}

fn valid_variant(variant: &WorldgenVehiclePartVariantV1) -> bool {
    valid_optional_id(&variant.variant_id)
        && valid_text(&variant.symbols, false, MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES)
        && matches!(variant.symbols.chars().count(), 1 | 8)
        && valid_text(
            &variant.broken_symbols,
            false,
            MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES,
        )
        && matches!(variant.broken_symbols.chars().count(), 1 | 8)
}

fn valid_part_type(part: &WorldgenVehiclePartTypeV1) -> bool {
    valid_id(&part.part_type_id)
        && valid_text(&part.name, false, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
        && valid_id(&part.item_type_id)
        && valid_id(&part.location)
        && part.durability > 0
        && part.cargo_capacity_milliliters <= MAX_VEHICLE_CARGO_VOLUME_MILLILITERS
        && part.power_milliwatts.unsigned_abs() <= 1_000_000_000_000
        && valid_optional_id(&part.fuel_type)
        && part.muscle_power_factor_milliwatts >= 0
        && part.muscle_power_factor_milliwatts <= 1_000_000_000_000
        && part.rolling_resistance_millionths > 0
        && part.contact_area > 0
        && part.wheel_offroad_rating_millionths <= 1_000_000
        && part.wheel_terrain_modifiers_json.len() <= 16_384
        && !part
            .wheel_terrain_modifiers_json
            .chars()
            .any(char::is_control)
        && (part.cargo_capacity_milliliters == 0
            || part
                .flags
                .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
                .is_ok())
        && part.flags.len() <= MAX_WORLDGEN_VEHICLE_PART_FLAGS
        && part.flags.iter().all(|flag| valid_id(flag))
        && part.flags.windows(2).all(|pair| pair[0] < pair[1])
        && !part.variants.is_empty()
        && part.variants.len() <= MAX_WORLDGEN_VEHICLE_PART_VARIANTS
        && part.variants.iter().all(valid_variant)
        && {
            let mut ids = BTreeSet::new();
            part.variants
                .iter()
                .all(|variant| ids.insert(variant.variant_id.as_str()))
        }
}

fn valid_prototype_part(
    part: &WorldgenVehiclePrototypePartV1,
    part_types: &[WorldgenVehiclePartTypeV1],
) -> bool {
    let Some(part_type) = part_types.get(usize::from(part.part_type_index)) else {
        return false;
    };
    part_type
        .variants
        .iter()
        .any(|variant| variant.variant_id == part.variant_id)
        && valid_optional_id(&part.fuel_item_type_id)
        && part.with_ammo_percent <= 100
        && part.ammo_type_ids.len() <= MAX_WORLDGEN_VEHICLE_PART_AMMO_TYPES
        && part.ammo_type_ids.iter().all(|id| valid_id(id))
        && part.tool_item_type_ids.len() <= MAX_WORLDGEN_VEHICLE_PART_TOOLS
        && part.tool_item_type_ids.iter().all(|id| valid_id(id))
        && ((part.ammo_quantity_minimum == -1 && part.ammo_quantity_maximum == -1)
            || (part.ammo_quantity_minimum >= 0
                && part.ammo_quantity_minimum <= part.ammo_quantity_maximum))
}

fn valid_item_spawn(
    spawn: &WorldgenVehicleItemSpawnV1,
    prototype: &WorldgenVehiclePrototypeV1,
    part_types: &[WorldgenVehiclePartTypeV1],
) -> bool {
    let Some(cargo_part) = prototype
        .parts
        .get(usize::from(spawn.cargo_prototype_part_index))
    else {
        return false;
    };
    let Some(cargo_type) = part_types.get(usize::from(cargo_part.part_type_index)) else {
        return false;
    };
    cargo_part.mount_x == spawn.mount_x
        && cargo_part.mount_y == spawn.mount_y
        && cargo_type
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
            .is_ok()
        && prototype
            .parts
            .iter()
            .take(usize::from(spawn.cargo_prototype_part_index))
            .all(|part| {
                part.mount_x != spawn.mount_x
                    || part.mount_y != spawn.mount_y
                    || part_types
                        .get(usize::from(part.part_type_index))
                        .is_none_or(|part_type| {
                            part_type
                                .flags
                                .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
                                .is_err()
                        })
            })
        && spawn.chance_percent <= 100
        && spawn.with_magazine_percent <= 100
        && spawn.with_ammo_percent <= 100
        && spawn.direct_items.len() <= MAX_WORLDGEN_VEHICLE_ITEMS_PER_SPAWN
        && spawn.item_group_ids.len() <= MAX_WORLDGEN_VEHICLE_ITEMS_PER_SPAWN
        && spawn
            .direct_items
            .len()
            .saturating_add(spawn.item_group_ids.len())
            <= MAX_WORLDGEN_VEHICLE_ITEMS_PER_SPAWN
        && spawn.direct_items.iter().all(|direct| {
            crate::item_groups::item_group_item_prototype_is_valid(&direct.item)
                && valid_optional_id(&direct.variant_id)
                && (direct.variant_id.is_empty()
                    || direct
                        .item
                        .variants
                        .iter()
                        .any(|variant| variant.variant.id == direct.variant_id))
        })
        && spawn.item_group_ids.iter().all(|id| valid_id(id))
}

fn checked_positive_weight_sum(entries: &[WorldgenVehicleGroupEntryV1]) -> bool {
    entries
        .iter()
        .try_fold(0_u32, |total, entry| {
            (entry.weight > 0).then_some(())?;
            total.checked_add(entry.weight)
        })
        .is_some()
}

#[must_use]
pub fn worldgen_vehicle_catalog_is_valid(
    part_types: &[WorldgenVehiclePartTypeV1],
    prototypes: &[WorldgenVehiclePrototypeV1],
    groups: &[WorldgenVehicleGroupV1],
) -> bool {
    if part_types.len() > MAX_WORLDGEN_VEHICLE_PART_TYPES
        || prototypes.len() > MAX_WORLDGEN_VEHICLE_PROTOTYPES
        || groups.len() > MAX_WORLDGEN_VEHICLE_GROUPS
        || !part_types.iter().all(valid_part_type)
        || !part_types
            .windows(2)
            .all(|pair| pair[0].part_type_id < pair[1].part_type_id)
        || !prototypes
            .windows(2)
            .all(|pair| pair[0].prototype_id < pair[1].prototype_id)
        || !groups
            .windows(2)
            .all(|pair| pair[0].group_id < pair[1].group_id)
    {
        return false;
    }

    let mut total_prototype_parts = 0_usize;
    for prototype in prototypes {
        if !valid_id(&prototype.prototype_id)
            || !valid_text(&prototype.name, false, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
            || prototype.parts.is_empty()
            || prototype.parts.len() > MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
            || !prototype
                .parts
                .iter()
                .all(|part| valid_prototype_part(part, part_types))
            || !prototype
                .parts
                .iter()
                .any(|part| part_types[usize::from(part.part_type_index)].location == "structure")
            || prototype.item_spawns.len() > MAX_WORLDGEN_VEHICLE_ITEM_SPAWNS
            || !prototype
                .item_spawns
                .iter()
                .all(|spawn| valid_item_spawn(spawn, prototype, part_types))
        {
            return false;
        }
        let Some(total) = total_prototype_parts.checked_add(prototype.parts.len()) else {
            return false;
        };
        total_prototype_parts = total;
        if total_prototype_parts > MAX_WORLDGEN_VEHICLE_PROTOTYPE_PARTS_TOTAL {
            return false;
        }
    }

    let mut total_entries = 0_usize;
    for group in groups {
        if !valid_id(&group.group_id)
            || group.entries.is_empty()
            || group.entries.len() > MAX_WORLDGEN_VEHICLE_GROUP_ENTRIES
            || !checked_positive_weight_sum(&group.entries)
            || !group
                .entries
                .iter()
                .all(|entry| usize::from(entry.prototype_index) < prototypes.len())
        {
            return false;
        }
        let Some(total) = total_entries.checked_add(group.entries.len()) else {
            return false;
        };
        total_entries = total;
        if total_entries > MAX_WORLDGEN_VEHICLE_GROUP_ENTRIES_TOTAL {
            return false;
        }
    }
    true
}

#[must_use]
pub fn worldgen_vehicle_placement_is_valid(
    placement: &WorldgenVehiclePlacementV1,
    group_count: usize,
) -> bool {
    usize::from(placement.group_index) < group_count
        && placement.chance_percent <= 100
        && !placement.rotations_degrees.is_empty()
        && placement.rotations_degrees.len() <= MAX_WORLDGEN_VEHICLE_ROTATIONS
        && placement
            .rotations_degrees
            .iter()
            .all(|degrees| (0..360).contains(degrees))
        && (-1..=100).contains(&placement.fuel_percent)
        && valid_optional_id(&placement.faction_id)
        && placement.repeat.minimum <= placement.repeat.maximum
        && placement.repeat.maximum <= MAX_WORLDGEN_VEHICLE_REPEAT
        && placement.x.minimum <= placement.x.maximum
        && i16::from(placement.x.minimum).unsigned_abs() < WORLDGEN_OMT_SIZE as u16
        && i16::from(placement.x.maximum).unsigned_abs() < WORLDGEN_OMT_SIZE as u16
        && placement.y.minimum <= placement.y.maximum
        && i16::from(placement.y.minimum).unsigned_abs() < WORLDGEN_OMT_SIZE as u16
        && i16::from(placement.y.maximum).unsigned_abs() < WORLDGEN_OMT_SIZE as u16
}

fn valid_live_part(
    part: &VehiclePartSnapshotV1,
    expected_index: usize,
    prototype: &WorldgenVehiclePrototypeV1,
    part_types: &[WorldgenVehiclePartTypeV1],
    origin: WorldPosition,
) -> bool {
    let Some(prototype_part) = prototype.parts.get(expected_index) else {
        return false;
    };
    let Some(part_type) = part_types.get(usize::from(prototype_part.part_type_index)) else {
        return false;
    };
    usize::from(part.prototype_part_index) == expected_index
        && part.position.z == origin.z
        && part.hp <= part_type.durability
        && (!part.enabled
            || [
                "ENGINE",
                "CONE_LIGHT",
                "WIDE_CONE_LIGHT",
                "DOME_LIGHT",
                "AISLE_LIGHT",
                "HALF_CIRCLE_LIGHT",
                "CIRCLE_LIGHT",
                "ATOMIC_LIGHT",
                "FRIDGE",
                "FREEZER",
                "WATER_PURIFIER",
                "REACTOR",
            ]
            .into_iter()
            .any(|flag| {
                part_type
                    .flags
                    .binary_search_by(|candidate| candidate.as_str().cmp(flag))
                    .is_ok()
            }))
        && (!part.open
            || part_type
                .flags
                .binary_search_by(|flag| flag.as_str().cmp("OPENABLE"))
                .is_ok())
        && (!part.locked
            || (!part.open
                && (part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("LOCKABLE_DOOR"))
                    .is_ok()
                    || part_type
                        .flags
                        .binary_search_by(|flag| flag.as_str().cmp("LOCKABLE_CARGO"))
                        .is_ok())))
        && part.passenger.is_none_or(|_| {
            part.hp > 0
                && part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("BOARDABLE"))
                    .is_ok()
        })
        && part.cargo.len() <= MAX_VEHICLE_CARGO_ITEMS_PER_PART
        && part.cargo.iter().all(valid_item_snapshot)
        && (part.cargo.is_empty()
            || (part.hp > 0
                && part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
                    .is_ok()))
        && part
            .cargo
            .iter()
            .try_fold(0_u64, |total, item| {
                total.checked_add(item_snapshot_containment_volume_milliliters(item)?)
            })
            .is_some_and(|volume| volume <= part_type.cargo_capacity_milliliters)
}

#[must_use]
pub fn vehicle_snapshots_are_valid(
    world_namespace: u64,
    part_types: &[WorldgenVehiclePartTypeV1],
    prototypes: &[WorldgenVehiclePrototypeV1],
    vehicles: &[VehicleSnapshotV1],
    actors: &[(ActorId, WorldPosition, bool)],
) -> bool {
    if vehicles.len() > MAX_LIVE_VEHICLES
        || !vehicles.windows(2).all(|pair| pair[0].id < pair[1].id)
    {
        return false;
    }
    let mut actor_positions = BTreeMap::new();
    let mut stable_counters = BTreeSet::new();
    let mut live_actors = BTreeSet::new();
    for (actor_id, position, alive) in actors {
        if actor_id.counter() == 0
            || actor_id.world_namespace() != world_namespace
            || !stable_counters.insert(actor_id.counter())
            || actor_positions.insert(*actor_id, *position).is_some()
        {
            return false;
        }
        if *alive {
            live_actors.insert(*actor_id);
        }
    }
    let mut passengers = BTreeSet::new();
    let mut structural_positions = BTreeMap::new();
    for vehicle in vehicles {
        let Some(prototype) = prototypes.get(usize::from(vehicle.prototype_index)) else {
            return false;
        };
        if vehicle.id.counter() == 0
            || vehicle.id.world_namespace() != world_namespace
            || !stable_counters.insert(vehicle.id.counter())
            || !(0..360).contains(&vehicle.facing_degrees)
            || !(0..360).contains(&vehicle.turn_direction_degrees)
            || !valid_vehicle_tileray_leftover(
                vehicle.facing_degrees,
                vehicle.movement_ray_leftover,
            )
            || !valid_optional_id(&vehicle.owner_faction_id)
            || vehicle.parts.len() != prototype.parts.len()
            || vehicle.parts.len() > MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
        {
            return false;
        }
        let live_flag = |flag: &str| {
            vehicle.parts.iter().enumerate().any(|(index, part)| {
                part.hp > 0
                    && prototype.parts.get(index).is_some_and(|prototype_part| {
                        part_types
                            .get(usize::from(prototype_part.part_type_index))
                            .is_some_and(|part_type| {
                                part_type
                                    .flags
                                    .binary_search_by(|candidate| candidate.as_str().cmp(flag))
                                    .is_ok()
                            })
                    })
            })
        };
        if (vehicle.engine_on && !live_flag("ENGINE"))
            || (vehicle.security_locked && !live_flag("SECURITY"))
        {
            return false;
        }
        let mut mount_positions = BTreeMap::new();
        for (index, part) in vehicle.parts.iter().enumerate() {
            if !valid_live_part(part, index, prototype, part_types, vehicle.origin) {
                return false;
            }
            let prototype_part = &prototype.parts[index];
            if expected_vehicle_part_position(
                vehicle.origin,
                prototype_part.mount_x,
                prototype_part.mount_y,
                vehicle.facing_degrees,
            ) != Some(part.position)
            {
                return false;
            }
            let mount = (prototype_part.mount_x, prototype_part.mount_y);
            if mount_positions
                .insert(mount, part.position)
                .is_some_and(|position| position != part.position)
            {
                return false;
            }
            if let Some(passenger) = part.passenger {
                if passenger.counter() == 0
                    || passenger.world_namespace() != world_namespace
                    || !live_actors.contains(&passenger)
                    || !passengers.insert(passenger)
                    || actor_positions.get(&passenger) != Some(&part.position)
                {
                    return false;
                }
            }
            let Some(part_type) = part_types.get(usize::from(prototype_part.part_type_index))
            else {
                return false;
            };
            if part.hp > 0 && part_type.location == "structure" {
                match structural_positions.insert(part.position, (vehicle.id, mount)) {
                    Some((other_vehicle, other_mount))
                        if other_vehicle != vehicle.id || other_mount != mount =>
                    {
                        return false;
                    }
                    _ => {}
                }
            }
        }
    }
    true
}

/// Adds vehicle counters to the caller's shared allocator-namespace set. This
/// lets full recovery reject a counter reused by any other stable-ID domain.
pub fn insert_vehicle_stable_counters(
    world_namespace: u64,
    vehicles: &[VehicleSnapshotV1],
    counters: &mut BTreeSet<u64>,
) -> bool {
    vehicles.iter().all(|vehicle| {
        vehicle.id.counter() > 0
            && vehicle.id.world_namespace() == world_namespace
            && counters.insert(vehicle.id.counter())
    })
}

#[must_use]
pub fn visible_vehicle_snapshots_are_valid(
    world_namespace: u64,
    vehicles: &[VisibleVehicleSnapshotV1],
    visible_actor_ids: &BTreeSet<ActorId>,
) -> bool {
    if vehicles.len() > MAX_LIVE_VEHICLES
        || !vehicles.windows(2).all(|pair| pair[0].id < pair[1].id)
    {
        return false;
    }
    let mut passengers = BTreeSet::new();
    vehicles.iter().all(|vehicle| {
        let mut part_indices = BTreeSet::new();
        let mut boardable_indices = BTreeSet::new();
        let mut openable_indices = BTreeSet::new();
        let mut cargo_indices = BTreeSet::new();
        vehicle.id.counter() > 0
            && vehicle.id.world_namespace() == world_namespace
            && valid_id(&vehicle.prototype_id)
            && valid_text(&vehicle.name, false, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
            && (0..360).contains(&vehicle.facing_degrees)
            && !vehicle.tiles.is_empty()
            && vehicle.tiles.len() <= MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
            && vehicle
                .tiles
                .windows(2)
                .all(|pair| pair[0].position < pair[1].position)
            && vehicle.tiles.iter().all(|tile| {
                usize::from(tile.prototype_part_index) < MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
                    && part_indices.insert(tile.prototype_part_index)
                    && tile.boardable_prototype_part_index.is_none_or(|index| {
                        usize::from(index) < MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
                            && boardable_indices.insert(index)
                    })
                    && tile.openable_prototype_part_index.is_none_or(|index| {
                        usize::from(index) < MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
                            && openable_indices.insert(index)
                    })
                    && tile.cargo_prototype_part_index.is_none_or(|index| {
                        usize::from(index) < MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
                            && cargo_indices.insert(index)
                    })
                    && tile.position.z == vehicle.origin.z
                    && tile.position.x.abs_diff(vehicle.origin.x) <= u32::from(u16::MAX)
                    && tile.position.y.abs_diff(vehicle.origin.y) <= u32::from(u16::MAX)
                    && valid_text(&tile.name, false, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
                    && valid_text(&tile.symbol, false, MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES)
                    && tile.maximum_hp > 0
                    && tile.hp <= tile.maximum_hp
                    && tile.passenger.is_none_or(|passenger| {
                        passenger.counter() > 0
                            && passenger.world_namespace() == world_namespace
                            && visible_actor_ids.contains(&passenger)
                            && passengers.insert(passenger)
                    })
                    && tile.cargo.len() <= MAX_VEHICLE_CARGO_ITEMS_PER_PART
                    && tile.cargo.iter().all(valid_item_snapshot)
            })
    })
}
