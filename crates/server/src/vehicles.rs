use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{VehiclePrototypeDefinition, VehicleRegistry};
use cdda_protocol::{
    WorldgenVehicleDirectItemSpawnV1, WorldgenVehicleGroupEntryV1, WorldgenVehicleGroupV1,
    WorldgenVehicleItemSpawnV1, WorldgenVehiclePartTypeV1, WorldgenVehiclePartVariantV1,
    WorldgenVehiclePrototypePartV1, WorldgenVehiclePrototypeV1,
};

use super::item_groups::{RuntimeItemGroupContent, runtime_item_group_item};

pub(super) struct RuntimeVehicleCatalog {
    pub part_types: Vec<WorldgenVehiclePartTypeV1>,
    pub prototypes: Vec<WorldgenVehiclePrototypeV1>,
    pub groups: Vec<WorldgenVehicleGroupV1>,
    pub group_indices: BTreeMap<String, u16>,
}

fn unsupported_part_field_changes_runtime(field: &str) -> bool {
    // These finalized fields affect installation, repair, disassembly, UI, or
    // collision damage, none of which the ordinary clear-terrain manual-drive
    // boundary exposes. Movement-affecting fields are deliberately absent and
    // therefore continue to fail admission.
    !matches!(
        field,
        "bonus"
            | "breaks_into"
            | "broken_color"
            | "categories"
            | "color"
            | "comfort"
            | "damage_modifier"
            | "damage_reduction"
            | "description"
            | "exclusions"
            | "folded_volume"
            | "folding_time"
            | "floor_bedding_warmth"
            | "looks_like"
            | "m2c"
            | "noise_factor"
            | "qualities"
            | "requirements"
            | "unfolding_time"
    )
}

fn ensure_static_prototype_is_supported(
    prototype: &VehiclePrototypeDefinition,
    vehicles: &VehicleRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    if prototype.abstract_definition
        || !prototype.unsupported_fields.is_empty()
        || prototype.parts.is_empty()
    {
        return Err(format!(
            "reachable vehicle {} uses unsupported items, zones, inheritance, or an empty body ({})",
            prototype.id, prototype.source
        )
        .into());
    }
    for part in &prototype.parts {
        if !part.fuel_item_id.is_empty()
            || part.with_ammo_percent != 0
            || !part.ammo_type_ids.is_empty()
            || part.ammo_quantity_minimum != -1
            || part.ammo_quantity_maximum != -1
            || !part.tool_item_ids.is_empty()
        {
            return Err(format!(
                "reachable vehicle {} part {} requires unrepresented fuel, turret ammunition, or tool state",
                prototype.id, part.part_id
            )
            .into());
        }
        let definition = vehicles.part(&part.part_id).ok_or_else(|| {
            format!(
                "reachable vehicle {} references missing part {}",
                prototype.id, part.part_id
            )
        })?;
        if definition.abstract_definition
            || definition
                .unsupported_fields
                .iter()
                .any(|field| unsupported_part_field_changes_runtime(field))
            || definition.durability == 0
            || definition.variants.is_empty()
        {
            return Err(format!(
                "reachable vehicle {} part {} has unsupported static semantics ({})",
                prototype.id, definition.id, definition.source
            )
            .into());
        }
        if !definition
            .variants
            .iter()
            .any(|variant| variant.variant_id == part.variant_id)
        {
            return Err(format!(
                "reachable vehicle {} selects missing variant {:?} for part {}",
                prototype.id, part.variant_id, definition.id
            )
            .into());
        }
        if definition.flags.contains("ENGINE")
            && (definition.fuel_type != "muscle"
                || definition.power_milliwatts <= 0
                || definition.muscle_power_factor_milliwatts < 0
                || !definition.flags.contains("CONTROLS")
                || !(definition.flags.contains("MUSCLE_ARMS")
                    || definition.flags.contains("MUSCLE_LEGS")))
        {
            return Err(format!(
                "reachable vehicle {} part {} is not an admitted manual engine",
                prototype.id, definition.id
            )
            .into());
        }
        const UNREPRESENTED_INITIAL_STATE_FLAGS: [&str; 17] = [
            "AISLE_LIGHT",
            "ATOMIC_LIGHT",
            "BATTERY",
            "CIRCLE_LIGHT",
            "CONE_LIGHT",
            "DOME_LIGHT",
            "FREEZER",
            "FRIDGE",
            "FUEL_STORE",
            "FUEL_TANK",
            "HALF_CIRCLE_LIGHT",
            "REACTOR",
            "SECURITY",
            "TURRET",
            "WATER_PURIFIER",
            "WIDE_CONE_LIGHT",
            "WIND_TURBINE",
        ];
        if let Some(flag) = UNREPRESENTED_INITIAL_STATE_FLAGS
            .into_iter()
            .find(|flag| definition.flags.contains(*flag))
        {
            return Err(format!(
                "reachable vehicle {} part {} requires unrepresented {flag} initialization state",
                prototype.id, definition.id
            )
            .into());
        }
    }
    Ok(())
}

pub(super) fn runtime_vehicle_item_group_roots(vehicles: &VehicleRegistry) -> BTreeSet<String> {
    vehicles
        .prototypes()
        .filter(|(_, prototype)| {
            !prototype.abstract_definition
                && prototype.unsupported_fields.is_empty()
                && !prototype.parts.is_empty()
        })
        .flat_map(|(_, prototype)| &prototype.item_spawns)
        .flat_map(|spawn| spawn.item_group_ids.iter().cloned())
        .collect()
}

pub(super) fn runtime_vehicle_catalog(
    root_group_ids: BTreeSet<String>,
    vehicles: &VehicleRegistry,
    item_content: RuntimeItemGroupContent<'_>,
) -> Result<RuntimeVehicleCatalog, Box<dyn std::error::Error>> {
    let mut prototype_ids = BTreeSet::new();
    for group_id in &root_group_ids {
        let group = vehicles
            .group(group_id)
            .ok_or_else(|| format!("mapgen references missing vehicle group {group_id}"))?;
        if group.entries.is_empty() {
            return Err(format!("reachable vehicle group {group_id} is empty").into());
        }
        let mut total_weight = 0_u32;
        for entry in &group.entries {
            if entry.weight == 0 {
                return Err(format!("vehicle group {group_id} contains zero weight").into());
            }
            total_weight = total_weight
                .checked_add(entry.weight)
                .ok_or_else(|| format!("vehicle group {group_id} weight total overflows u32"))?;
            let prototype = vehicles.prototype(&entry.prototype_id).ok_or_else(|| {
                format!(
                    "vehicle group {group_id} references missing prototype {}",
                    entry.prototype_id
                )
            })?;
            ensure_static_prototype_is_supported(prototype, vehicles)?;
            prototype_ids.insert(entry.prototype_id.clone());
        }
    }

    let mut part_ids = BTreeSet::new();
    for prototype_id in &prototype_ids {
        let prototype = vehicles
            .prototype(prototype_id)
            .ok_or("reachable vehicle prototype disappeared")?;
        part_ids.extend(prototype.parts.iter().map(|part| part.part_id.clone()));
    }

    let part_indices = part_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let prototype_indices = prototype_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let group_indices = root_group_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;

    let part_types = part_ids
        .iter()
        .map(|id| {
            let part = vehicles
                .part(id)
                .ok_or("reachable vehicle part disappeared")?;
            Ok(WorldgenVehiclePartTypeV1 {
                part_type_id: part.id.clone(),
                name: part.name.clone(),
                item_type_id: part.item_id.clone(),
                location: part.location.clone(),
                durability: part.durability,
                cargo_capacity_milliliters: part
                    .cargo_capacity_milliliters
                    .min(cdda_protocol::MAX_VEHICLE_CARGO_VOLUME_MILLILITERS),
                power_milliwatts: part.power_milliwatts,
                fuel_type: part.fuel_type.clone(),
                muscle_power_factor_milliwatts: part.muscle_power_factor_milliwatts,
                rolling_resistance_millionths: part.rolling_resistance_millionths,
                contact_area: part.contact_area,
                wheel_offroad_rating_millionths: part.wheel_offroad_rating_millionths,
                wheel_terrain_modifiers_json: part.wheel_terrain_modifiers_json.clone(),
                flags: part.flags.iter().cloned().collect(),
                variants: part
                    .variants
                    .iter()
                    .map(|variant| WorldgenVehiclePartVariantV1 {
                        variant_id: variant.variant_id.clone(),
                        symbols: variant.symbols.clone(),
                        broken_symbols: variant.broken_symbols.clone(),
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let prototypes = prototype_ids
        .iter()
        .map(|id| {
            let prototype = vehicles
                .prototype(id)
                .ok_or("reachable vehicle prototype disappeared")?;
            Ok(WorldgenVehiclePrototypeV1 {
                prototype_id: prototype.id.clone(),
                name: prototype.name.clone(),
                parts: prototype
                    .parts
                    .iter()
                    .map(|part| {
                        Ok(WorldgenVehiclePrototypePartV1 {
                            mount_x: part.mount_x,
                            mount_y: part.mount_y,
                            part_type_index: *part_indices
                                .get(&part.part_id)
                                .ok_or("reachable vehicle part index disappeared")?,
                            variant_id: part.variant_id.clone(),
                            fuel_item_type_id: part.fuel_item_id.clone(),
                            with_ammo_percent: part.with_ammo_percent,
                            ammo_type_ids: part.ammo_type_ids.clone(),
                            ammo_quantity_minimum: part.ammo_quantity_minimum,
                            ammo_quantity_maximum: part.ammo_quantity_maximum,
                            tool_item_type_ids: part.tool_item_ids.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
                item_spawns: prototype
                    .item_spawns
                    .iter()
                    .map(|spawn| {
                        let cargo_prototype_part_index = prototype
                            .parts
                            .iter()
                            .enumerate()
                            .find_map(|(index, candidate)| {
                                if candidate.mount_x != spawn.mount_x
                                    || candidate.mount_y != spawn.mount_y
                                {
                                    return None;
                                }
                                vehicles.part(&candidate.part_id).and_then(|part| {
                                    part.flags
                                        .contains("CARGO")
                                        .then(|| u16::try_from(index).ok())
                                        .flatten()
                                })
                            })
                            .ok_or_else(|| {
                                format!(
                                    "vehicle {} has an item spawn without CARGO at ({}, {})",
                                    prototype.id, spawn.mount_x, spawn.mount_y
                                )
                            })?;
                        let direct_items = spawn
                            .direct_items
                            .iter()
                            .map(|direct| {
                                let item =
                                    item_content.items.get(&direct.item_id).ok_or_else(|| {
                                        format!(
                                            "vehicle {} item spawn references missing item {}",
                                            prototype.id, direct.item_id
                                        )
                                    })?;
                                let item = runtime_item_group_item(item, None, item_content)?;
                                if !direct.variant_id.is_empty()
                                    && !item
                                        .variants
                                        .iter()
                                        .any(|variant| variant.variant.id == direct.variant_id)
                                {
                                    return Err(format!(
                                        "vehicle {} item {} references missing variant {}",
                                        prototype.id, direct.item_id, direct.variant_id
                                    )
                                    .into());
                                }
                                Ok(WorldgenVehicleDirectItemSpawnV1 {
                                    item,
                                    variant_id: direct.variant_id.clone(),
                                })
                            })
                            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
                        Ok(WorldgenVehicleItemSpawnV1 {
                            mount_x: spawn.mount_x,
                            mount_y: spawn.mount_y,
                            cargo_prototype_part_index,
                            chance_percent: spawn.chance_percent,
                            with_magazine_percent: spawn.with_magazine_percent,
                            with_ammo_percent: spawn.with_ammo_percent,
                            direct_items,
                            item_group_ids: spawn.item_group_ids.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let groups = root_group_ids
        .iter()
        .map(|id| {
            let group = vehicles
                .group(id)
                .ok_or("reachable vehicle group disappeared")?;
            Ok(WorldgenVehicleGroupV1 {
                group_id: group.id.clone(),
                entries: group
                    .entries
                    .iter()
                    .map(|entry| {
                        Ok(WorldgenVehicleGroupEntryV1 {
                            prototype_index: *prototype_indices
                                .get(&entry.prototype_id)
                                .ok_or("reachable vehicle prototype index disappeared")?,
                            weight: entry.weight,
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    Ok(RuntimeVehicleCatalog {
        part_types,
        prototypes,
        groups,
        group_indices,
    })
}
