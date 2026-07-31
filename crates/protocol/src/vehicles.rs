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
    /// Empty means no faction owner.
    pub owner_faction_id: String,
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

fn valid_variant(variant: &WorldgenVehiclePartVariantV1) -> bool {
    valid_optional_id(&variant.variant_id)
        && valid_text(&variant.symbols, false, MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES)
        && valid_text(
            &variant.broken_symbols,
            false,
            MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES,
        )
}

fn valid_part_type(part: &WorldgenVehiclePartTypeV1) -> bool {
    valid_id(&part.part_type_id)
        && valid_text(&part.name, false, MAX_WORLDGEN_VEHICLE_TEXT_BYTES)
        && valid_id(&part.item_type_id)
        && valid_id(&part.location)
        && part.durability > 0
        && part.cargo_capacity_milliliters <= MAX_VEHICLE_CARGO_VOLUME_MILLILITERS
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
        && part.position.x.abs_diff(origin.x) <= u32::from(u16::MAX)
        && part.position.y.abs_diff(origin.y) <= u32::from(u16::MAX)
        && part.hp <= part_type.durability
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
    actors: &[(ActorId, WorldPosition)],
) -> bool {
    if vehicles.len() > MAX_LIVE_VEHICLES
        || !vehicles.windows(2).all(|pair| pair[0].id < pair[1].id)
    {
        return false;
    }
    let mut actor_positions = BTreeMap::new();
    let mut stable_counters = BTreeSet::new();
    for (actor_id, position) in actors {
        if actor_id.counter() == 0
            || actor_id.world_namespace() != world_namespace
            || !stable_counters.insert(actor_id.counter())
            || actor_positions.insert(*actor_id, *position).is_some()
        {
            return false;
        }
    }
    let mut passengers = BTreeSet::new();
    for vehicle in vehicles {
        let Some(prototype) = prototypes.get(usize::from(vehicle.prototype_index)) else {
            return false;
        };
        if vehicle.id.counter() == 0
            || vehicle.id.world_namespace() != world_namespace
            || !stable_counters.insert(vehicle.id.counter())
            || !(0..360).contains(&vehicle.facing_degrees)
            || !valid_optional_id(&vehicle.owner_faction_id)
            || vehicle.parts.len() != prototype.parts.len()
            || vehicle.parts.len() > MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE
        {
            return false;
        }
        let mut mount_positions = BTreeMap::new();
        for (index, part) in vehicle.parts.iter().enumerate() {
            if !valid_live_part(part, index, prototype, part_types, vehicle.origin) {
                return false;
            }
            let prototype_part = &prototype.parts[index];
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
                    || !passengers.insert(passenger)
                    || actor_positions.get(&passenger) != Some(&part.position)
                {
                    return false;
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
                tile.position.z == vehicle.origin.z
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
