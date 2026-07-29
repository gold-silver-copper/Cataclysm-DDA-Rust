use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    DefaultRegionTerrainFurnitureRegistry, FurnitureRegistry, MapgenIdChoice, MapgenRegistry,
    OvermapTerrainMatchType, OvermapTerrainRegistry, StartLocationDefinition,
    StrictMapgenDefinition, TerrainRegistry,
};
use cdda_protocol::{
    ItemGroupDefinitionV1, WorldgenCatalogV1, WorldgenCellV1, WorldgenFurniturePrototypeTargetV1,
    WorldgenFurnitureTargetV1, WorldgenItemGroupPlacementV1, WorldgenOmtGeneratorV1,
    WorldgenOmtIdentityV1, WorldgenOmtMatchTypeV1, WorldgenOvermapLayerV1, WorldgenOvermapLayoutV1,
    WorldgenOvermapRunV1, WorldgenRegionalFurnitureTableV1, WorldgenRegionalTerrainTableV1,
    WorldgenStartLocationV1, WorldgenStartTargetV1, WorldgenTemplateV1, WorldgenTerrainTargetV1,
    WorldgenWeightedFurniturePrototypeV1, WorldgenWeightedFurnitureTargetV1,
    WorldgenWeightedPrototypeV1, WorldgenWeightedTerrainTargetV1, worldgen_catalog_is_valid,
};

use super::{furniture_tile, terrain_tile};

/// Retains the previously runnable LMOE bootstrap inside the new bounded,
/// coordinate-owned representation. Real regional field population cannot be
/// admitted until its complete item-group modifier closure is supported.
pub(super) fn bootstrap_lmoe_overmap(
    terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenOvermapLayoutV1, Box<dyn std::error::Error>> {
    let lmoe = terrain
        .get_identity("lmoe_north")
        .ok_or("pinned overmap-terrain catalog is missing lmoe_north")?;
    let identity = WorldgenOmtIdentityV1 {
        full_id: lmoe.full_id.clone(),
        type_id: lmoe.type_id.clone(),
        subtype_id: lmoe.subtype_id.clone(),
        generator_id: lmoe.generator_id.clone(),
        rotation: lmoe.rotation,
    };
    Ok(WorldgenOvermapLayoutV1 {
        origin_x: -90,
        origin_y: -90,
        identities: vec![identity],
        layers: vec![WorldgenOvermapLayerV1 {
            z: 0,
            runs: vec![WorldgenOvermapRunV1 {
                identity_index: 0,
                length: u32::from(cdda_protocol::WORLDGEN_OVERMAP_WIDTH)
                    * u32::from(cdda_protocol::WORLDGEN_OVERMAP_HEIGHT),
            }],
        }],
    })
}

pub(super) struct RuntimeMapgenContent<'a> {
    pub mapgen: &'a MapgenRegistry,
    pub regions: &'a DefaultRegionTerrainFurnitureRegistry,
    pub terrain: &'a TerrainRegistry,
    pub furniture: &'a FurnitureRegistry,
    pub item_groups: &'a [ItemGroupDefinitionV1],
}

pub(super) fn runtime_mapgen_worldgen(
    overmap: WorldgenOvermapLayoutV1,
    start_location: &StartLocationDefinition,
    content: RuntimeMapgenContent<'_>,
) -> Result<WorldgenCatalogV1, Box<dyn std::error::Error>> {
    let RuntimeMapgenContent {
        mapgen,
        regions,
        terrain,
        furniture,
        item_groups,
    } = content;
    if !start_location.is_runtime_selectable_without_cities() {
        return Err(format!(
            "start location {} requires unsupported city, parameter, flag, or z-level semantics",
            start_location.id
        )
        .into());
    }
    let start_location = WorldgenStartLocationV1 {
        start_location_id: start_location.id.clone(),
        targets: start_location
            .targets
            .iter()
            .map(|target| WorldgenStartTargetV1 {
                omt: target.overmap_terrain.clone(),
                match_type: match target.match_type {
                    OvermapTerrainMatchType::Exact => WorldgenOmtMatchTypeV1::Exact,
                    OvermapTerrainMatchType::Type => WorldgenOmtMatchTypeV1::Type,
                    OvermapTerrainMatchType::Subtype => WorldgenOmtMatchTypeV1::Subtype,
                    OvermapTerrainMatchType::Prefix => WorldgenOmtMatchTypeV1::Prefix,
                    OvermapTerrainMatchType::Contains => WorldgenOmtMatchTypeV1::Contains,
                },
            })
            .collect(),
    };
    let generator_ids = overmap
        .identities
        .iter()
        .map(|identity| identity.generator_id.clone())
        .collect::<BTreeSet<_>>();
    let definitions = generator_ids
        .iter()
        .map(|omt_id| {
            let definitions = mapgen.get(omt_id).ok_or_else(|| {
                let reason = mapgen
                    .unavailable_reports(omt_id)
                    .and_then(|reports| reports.first())
                    .and_then(|report| report.rejection_reason.as_deref())
                    .unwrap_or("no selected definition");
                format!("pinned mapgen {omt_id} is unavailable: {reason}")
            })?;
            if definitions.is_empty() {
                return Err(format!("pinned mapgen {omt_id} has no variants"));
            }
            Ok((omt_id.as_str(), definitions))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut terrain_ids = BTreeSet::new();
    let mut furniture_ids = BTreeSet::new();
    let mut regional_terrain_ids = BTreeSet::new();
    let mut regional_furniture_ids = BTreeSet::new();
    for (omt_id, variants) in &definitions {
        for definition in *variants {
            if matches!(definition.fill_terrain, Some(MapgenIdChoice::Weighted(_))) {
                return Err(format!(
                    "mapgen {omt_id} uses a weighted one-time fill that worldgen v2 cannot encode"
                )
                .into());
            }
            for choice in definition
                .fill_terrain
                .iter()
                .chain(definition.terrain.values().flatten())
            {
                collect_runtime_terrain_choice(
                    choice,
                    regions,
                    terrain,
                    &mut terrain_ids,
                    &mut regional_terrain_ids,
                )?;
            }
            for choice in definition.furniture.values().flatten() {
                collect_runtime_furniture_choice(
                    choice,
                    regions,
                    furniture,
                    &mut furniture_ids,
                    &mut regional_furniture_ids,
                )?;
            }
        }
    }

    let terrain_prototypes = terrain_ids
        .iter()
        .map(|id| {
            terrain_tile(
                terrain
                    .get(id)
                    .ok_or("mapgen terrain prototype disappeared")?,
                terrain,
            )
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let furniture_prototypes = furniture_ids
        .iter()
        .map(|id| {
            furniture
                .get(id)
                .map(furniture_tile)
                .ok_or_else(|| "mapgen furniture prototype disappeared".into())
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let terrain_indices = terrain_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let furniture_indices = furniture_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let regional_terrain_indices = regional_terrain_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let regional_furniture_indices = regional_furniture_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;

    let regional_terrain = regional_terrain_ids
        .iter()
        .map(|id| {
            let table = regions
                .terrain_table(id)
                .ok_or("reachable regional terrain table disappeared")?;
            Ok(WorldgenRegionalTerrainTableV1 {
                regional_id: id.clone(),
                choices: table
                    .choices
                    .iter()
                    .map(|choice| {
                        Ok(WorldgenWeightedPrototypeV1 {
                            prototype_index: *terrain_indices.get(&choice.id).ok_or(
                                "worldgen v2 cannot encode recursive regional terrain choices",
                            )?,
                            weight: choice.weight,
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let regional_furniture = regional_furniture_ids
        .iter()
        .map(|id| {
            let table = regions
                .furniture_table(id)
                .ok_or("reachable regional furniture table disappeared")?;
            Ok(WorldgenRegionalFurnitureTableV1 {
                regional_id: id.clone(),
                choices: table
                    .choices
                    .iter()
                    .map(|choice| {
                        let target = if choice.id == "f_null" {
                            WorldgenFurniturePrototypeTargetV1::None
                        } else {
                            WorldgenFurniturePrototypeTargetV1::Prototype(
                                *furniture_indices.get(&choice.id).ok_or(
                                    "worldgen v2 cannot encode recursive regional furniture choices",
                                )?,
                            )
                        };
                        Ok(WorldgenWeightedFurniturePrototypeV1 {
                            target,
                            weight: choice.weight,
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    let omt_generators = definitions
        .iter()
        .map(|(omt_id, definitions)| {
            Ok(WorldgenOmtGeneratorV1 {
                omt_id: (*omt_id).to_owned(),
                templates: definitions
                    .iter()
                    .map(|definition| {
                        runtime_mapgen_template(
                            definition,
                            &terrain_indices,
                            &furniture_indices,
                            &regional_terrain_indices,
                            &regional_furniture_indices,
                        )
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let catalog = WorldgenCatalogV1 {
        generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        overmap,
        start_location: Some(start_location),
        terrain_prototypes,
        furniture_prototypes,
        regional_terrain,
        regional_furniture,
        omt_generators,
    };
    if !worldgen_catalog_is_valid(&catalog, item_groups) {
        return Err("pinned mapgens produced an invalid coordinate-owned worldgen catalog".into());
    }
    Ok(catalog)
}

fn collect_runtime_terrain_choice(
    choice: &MapgenIdChoice,
    regions: &DefaultRegionTerrainFurnitureRegistry,
    terrain: &TerrainRegistry,
    concrete: &mut BTreeSet<String>,
    regional: &mut BTreeSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for id in choice.ids() {
        if let Some(table) = regions.terrain_table(id) {
            regional.insert(id.to_owned());
            for replacement in &table.choices {
                if regions.terrain_table(&replacement.id).is_some() {
                    return Err(format!(
                        "worldgen v2 cannot encode recursive regional terrain {} -> {}",
                        id, replacement.id
                    )
                    .into());
                }
                concrete.insert(replacement.id.clone());
            }
        } else {
            let definition = terrain
                .get(id)
                .ok_or_else(|| format!("mapgen references missing terrain {id}"))?;
            if definition.flags.contains("REGION_PSEUDO") {
                return Err(format!("mapgen terrain {id} has no default regional table").into());
            }
            concrete.insert(id.to_owned());
        }
    }
    Ok(())
}

fn collect_runtime_furniture_choice(
    choice: &MapgenIdChoice,
    regions: &DefaultRegionTerrainFurnitureRegistry,
    furniture: &FurnitureRegistry,
    concrete: &mut BTreeSet<String>,
    regional: &mut BTreeSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for id in choice.ids() {
        if id == "f_null" {
            continue;
        }
        if let Some(table) = regions.furniture_table(id) {
            regional.insert(id.to_owned());
            for replacement in &table.choices {
                if replacement.id == "f_null" {
                    continue;
                }
                if regions.furniture_table(&replacement.id).is_some() {
                    return Err(format!(
                        "worldgen v2 cannot encode recursive regional furniture {} -> {}",
                        id, replacement.id
                    )
                    .into());
                }
                concrete.insert(replacement.id.clone());
            }
        } else {
            let definition = furniture
                .get(id)
                .ok_or_else(|| format!("mapgen references missing furniture {id}"))?;
            if definition.flags.contains("REGION_PSEUDO") {
                return Err(format!("mapgen furniture {id} has no default regional table").into());
            }
            concrete.insert(id.to_owned());
        }
    }
    Ok(())
}

fn runtime_mapgen_template(
    definition: &StrictMapgenDefinition,
    terrain: &BTreeMap<String, u16>,
    furniture: &BTreeMap<String, u16>,
    regional_terrain: &BTreeMap<String, u16>,
    regional_furniture: &BTreeMap<String, u16>,
) -> Result<WorldgenTemplateV1, Box<dyn std::error::Error>> {
    let fill = definition.fill_terrain.as_ref();
    let cells = definition
        .cells
        .iter()
        .map(|glyph| {
            let terrain_layers = definition.terrain.get(glyph);
            if terrain_layers.is_some_and(|layers| layers.len() != 1) {
                return Err(format!(
                    "mapgen {} has multiple terrain layers for glyph {glyph:?}",
                    definition.source
                )
                .into());
            }
            let terrain_choice = terrain_layers
                .and_then(|layers| layers.first())
                .or(fill)
                .ok_or_else(|| {
                    format!(
                        "mapgen {} has no terrain for glyph {glyph:?}",
                        definition.source
                    )
                })?;
            let furniture_layers = definition.furniture.get(glyph);
            if furniture_layers.is_some_and(|layers| layers.len() != 1) {
                return Err(format!(
                    "mapgen {} has multiple furniture layers for glyph {glyph:?}",
                    definition.source
                )
                .into());
            }
            Ok(WorldgenCellV1 {
                terrain: runtime_mapgen_terrain_choice(terrain_choice, terrain, regional_terrain)?,
                furniture: furniture_layers
                    .and_then(|layers| layers.first())
                    .map_or_else(
                        || {
                            Ok(vec![WorldgenWeightedFurnitureTargetV1 {
                                target: WorldgenFurnitureTargetV1::None,
                                weight: 1,
                            }])
                        },
                        |choice| {
                            runtime_mapgen_furniture_choice(choice, furniture, regional_furniture)
                        },
                    )?,
                item_group: definition.items.get(glyph).map(|placement| {
                    WorldgenItemGroupPlacementV1 {
                        group_id: placement.item_group.clone(),
                        chance: placement.chance,
                    }
                }),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    Ok(WorldgenTemplateV1 {
        weight: definition.weight,
        cells,
    })
}

pub(super) fn runtime_mapgen_terrain_choice(
    choice: &MapgenIdChoice,
    concrete: &BTreeMap<String, u16>,
    regional: &BTreeMap<String, u16>,
) -> Result<Vec<WorldgenWeightedTerrainTargetV1>, Box<dyn std::error::Error>> {
    let target = |id: &str| {
        regional.get(id).map_or_else(
            || {
                concrete
                    .get(id)
                    .copied()
                    .map(WorldgenTerrainTargetV1::Prototype)
                    .ok_or_else(|| format!("terrain target {id} disappeared"))
            },
            |index| Ok(WorldgenTerrainTargetV1::Regional(*index)),
        )
    };
    match choice {
        MapgenIdChoice::Fixed(id) => Ok(vec![WorldgenWeightedTerrainTargetV1 {
            target: target(id)?,
            weight: 1,
        }]),
        MapgenIdChoice::Weighted(entries) => {
            if entries.len() == 1 {
                return Err(
                    "worldgen v2 cannot retain a one-entry weighted terrain RNG phase".into(),
                );
            }
            entries
                .iter()
                .map(|entry| {
                    Ok(WorldgenWeightedTerrainTargetV1 {
                        target: target(&entry.id)?,
                        weight: entry.weight,
                    })
                })
                .collect::<Result<_, String>>()
                .map_err(Into::into)
        }
    }
}

pub(super) fn runtime_mapgen_furniture_choice(
    choice: &MapgenIdChoice,
    concrete: &BTreeMap<String, u16>,
    regional: &BTreeMap<String, u16>,
) -> Result<Vec<WorldgenWeightedFurnitureTargetV1>, Box<dyn std::error::Error>> {
    let target = |id: &str| {
        if id == "f_null" {
            Ok(WorldgenFurnitureTargetV1::None)
        } else {
            regional.get(id).map_or_else(
                || {
                    concrete
                        .get(id)
                        .copied()
                        .map(WorldgenFurnitureTargetV1::Prototype)
                        .ok_or_else(|| format!("furniture target {id} disappeared"))
                },
                |index| Ok(WorldgenFurnitureTargetV1::Regional(*index)),
            )
        }
    };
    match choice {
        MapgenIdChoice::Fixed(id) => Ok(vec![WorldgenWeightedFurnitureTargetV1 {
            target: target(id)?,
            weight: 1,
        }]),
        MapgenIdChoice::Weighted(entries) => {
            if entries.len() == 1 {
                return Err(
                    "worldgen v2 cannot retain a one-entry weighted furniture RNG phase".into(),
                );
            }
            entries
                .iter()
                .map(|entry| {
                    Ok(WorldgenWeightedFurnitureTargetV1 {
                        target: target(&entry.id)?,
                        weight: entry.weight,
                    })
                })
                .collect::<Result<_, String>>()
                .map_err(Into::into)
        }
    }
}
