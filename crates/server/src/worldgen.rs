use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    CitySettingsDefinition, DefaultRegionTerrainFurnitureRegistry, FurnitureRegistry,
    MapgenIdChoice, MapgenRegistry, OvermapTerrainMatchType, OvermapTerrainRegistry,
    StartLocationDefinition, StrictMapgenDefinition, TerrainRegistry,
};
use cdda_protocol::{
    ItemGroupDefinitionV1, WorldgenCatalogV1, WorldgenCellV1, WorldgenCityV1,
    WorldgenFurniturePrototypeTargetV1, WorldgenFurnitureTargetV1, WorldgenItemGroupPlacementV1,
    WorldgenOmtGeneratorV1, WorldgenOmtIdentityV1, WorldgenOmtMatchTypeV1, WorldgenOvermapLayerV1,
    WorldgenOvermapLayoutV1, WorldgenOvermapRunV1, WorldgenRegionalFurnitureTableV1,
    WorldgenRegionalTerrainTableV1, WorldgenStartLocationV1, WorldgenStartTargetV1,
    WorldgenTemplateV1, WorldgenTerrainTargetV1, WorldgenWeightedFurniturePrototypeV1,
    WorldgenWeightedFurnitureTargetV1, WorldgenWeightedPrototypeV1,
    WorldgenWeightedTerrainTargetV1, worldgen_catalog_is_valid,
};
use cdda_sim::{
    OVERMAP_ROAD_MASK_IDS, OvermapCitySettings, OvermapRoadExit, place_overmap_cities,
    place_overmap_roads,
};

use super::{furniture_tile, terrain_tile};

/// Retains the characterized LMOE layout for direct mapgen/start-selection
/// tests. Production worlds use `bootstrap_regional_field_overmap`.
#[cfg(test)]
pub(super) fn bootstrap_lmoe_overmap(
    terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenOvermapLayoutV1, Box<dyn std::error::Error>> {
    bootstrap_uniform_overmap(terrain, "lmoe_north")
}

/// Production pre-city layout for the regional-field milestone. Every OMT is
/// a real pinned `field` identity and therefore executes the production field
/// mapgen, regional terrain/furniture tables, and field item-group closure.
pub(super) fn bootstrap_regional_field_overmap(
    terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenOvermapLayoutV1, Box<dyn std::error::Error>> {
    bootstrap_uniform_overmap(terrain, "field")
}

/// Production regional layout after the city-placement family. City seeds are
/// deterministic world data; later roads/buildings expand from them without
/// rerolling ownership or size.
pub(super) fn bootstrap_regional_city_overmap(
    terrain: &OvermapTerrainRegistry,
    world_seed: [u8; 32],
    city_settings: &CitySettingsDefinition,
) -> Result<(WorldgenOvermapLayoutV1, Vec<WorldgenCityV1>), Box<dyn std::error::Error>> {
    let field = bootstrap_regional_field_overmap(terrain)?;
    let source = terrain
        .get_identity("road_nesw")
        .ok_or("pinned overmap-terrain catalog is missing road_nesw")?;
    let center = WorldgenOmtIdentityV1 {
        full_id: source.full_id.clone(),
        type_id: source.type_id.clone(),
        subtype_id: source.subtype_id.clone(),
        // `place_cities` owns the road OMT identity, but its nested local-map
        // construction belongs to the following roads family. Use the exact
        // pinned `fallback_predecessor_mapgen` (`field`) until that family is
        // admitted; no unsupported road collision or loot is fabricated.
        generator_id: String::from("field"),
        rotation: source.rotation,
    };
    place_overmap_cities(
        world_seed,
        cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        field,
        OvermapCitySettings::core_default(
            city_settings.city_size,
            city_settings.city_spacing,
            city_settings.is_megacity,
        ),
        "field",
        center,
    )
    .map_err(Into::into)
}

/// Production regional layout after the inter-city road-topology family.
/// Road OMT ownership is exact; local road rendering continues to use the
/// pinned field predecessor until nested road mapgen is admitted.
type RegionalRoadOvermap = (
    WorldgenOvermapLayoutV1,
    Vec<WorldgenCityV1>,
    Vec<OvermapRoadExit>,
);

pub(super) fn bootstrap_regional_road_overmap(
    terrain: &OvermapTerrainRegistry,
    world_seed: [u8; 32],
    city_settings: &CitySettingsDefinition,
) -> Result<RegionalRoadOvermap, Box<dyn std::error::Error>> {
    let (city_layout, cities) =
        bootstrap_regional_city_overmap(terrain, world_seed, city_settings)?;
    let road_identities = OVERMAP_ROAD_MASK_IDS
        .iter()
        .map(|full_id| {
            let source = terrain
                .get_identity(full_id)
                .ok_or_else(|| format!("pinned overmap-terrain catalog is missing {full_id}"))?;
            Ok(WorldgenOmtIdentityV1 {
                full_id: source.full_id.clone(),
                type_id: source.type_id.clone(),
                subtype_id: source.subtype_id.clone(),
                generator_id: String::from("field"),
                rotation: source.rotation,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let (layout, exits) = place_overmap_roads(
        world_seed,
        cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        city_layout,
        &cities,
        &[],
        &road_identities,
    )?;
    Ok((layout, cities, exits))
}

fn bootstrap_uniform_overmap(
    terrain: &OvermapTerrainRegistry,
    identity_id: &str,
) -> Result<WorldgenOvermapLayoutV1, Box<dyn std::error::Error>> {
    let source = terrain
        .get_identity(identity_id)
        .ok_or_else(|| format!("pinned overmap-terrain catalog is missing {identity_id}"))?;
    let identity = WorldgenOmtIdentityV1 {
        full_id: source.full_id.clone(),
        type_id: source.type_id.clone(),
        subtype_id: source.subtype_id.clone(),
        generator_id: source.generator_id.clone(),
        rotation: source.rotation,
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
    cities: Vec<WorldgenCityV1>,
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
    if !start_location.is_runtime_selectable_with_cities() {
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
        city_sizes: cdda_protocol::WorldgenI32IntervalV1 {
            minimum: start_location.city_sizes.minimum,
            maximum: start_location.city_sizes.maximum,
        },
        city_distance: cdda_protocol::WorldgenI32IntervalV1 {
            minimum: start_location.city_distance.minimum,
            maximum: start_location.city_distance.maximum,
        },
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
                                "reachable regional terrain target disappeared from the prototype closure",
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
                                    "reachable regional furniture target disappeared from the prototype closure",
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
        cities,
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
        if regions.terrain_table(id).is_some() {
            collect_runtime_regional_terrain_closure(
                id,
                regions,
                terrain,
                concrete,
                regional,
                &mut BTreeSet::new(),
            )?;
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

fn collect_runtime_regional_terrain_closure(
    id: &str,
    regions: &DefaultRegionTerrainFurnitureRegistry,
    terrain: &TerrainRegistry,
    concrete: &mut BTreeSet<String>,
    regional: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if regional.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(format!("recursive regional terrain cycle reached {id}").into());
    }
    let table = regions
        .terrain_table(id)
        .ok_or_else(|| format!("regional terrain table {id} disappeared"))?;
    for replacement in &table.choices {
        let definition = terrain.get(&replacement.id).ok_or_else(|| {
            format!(
                "regional terrain {} references missing terrain {}",
                id, replacement.id
            )
        })?;
        concrete.insert(replacement.id.clone());
        if regions.terrain_table(&replacement.id).is_some() {
            collect_runtime_regional_terrain_closure(
                &replacement.id,
                regions,
                terrain,
                concrete,
                regional,
                visiting,
            )?;
        } else if definition.flags.contains("REGION_PSEUDO") {
            return Err(format!(
                "regional terrain {} reaches pseudo terrain {} without a default table",
                id, replacement.id
            )
            .into());
        }
    }
    visiting.remove(id);
    regional.insert(id.to_owned());
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
        if regions.furniture_table(id).is_some() {
            collect_runtime_regional_furniture_closure(
                id,
                regions,
                furniture,
                concrete,
                regional,
                &mut BTreeSet::new(),
            )?;
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

fn collect_runtime_regional_furniture_closure(
    id: &str,
    regions: &DefaultRegionTerrainFurnitureRegistry,
    furniture: &FurnitureRegistry,
    concrete: &mut BTreeSet<String>,
    regional: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if regional.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_owned()) {
        return Err(format!("recursive regional furniture cycle reached {id}").into());
    }
    let table = regions
        .furniture_table(id)
        .ok_or_else(|| format!("regional furniture table {id} disappeared"))?;
    for replacement in &table.choices {
        if replacement.id == "f_null" {
            continue;
        }
        let definition = furniture.get(&replacement.id).ok_or_else(|| {
            format!(
                "regional furniture {} references missing furniture {}",
                id, replacement.id
            )
        })?;
        concrete.insert(replacement.id.clone());
        if regions.furniture_table(&replacement.id).is_some() {
            collect_runtime_regional_furniture_closure(
                &replacement.id,
                regions,
                furniture,
                concrete,
                regional,
                visiting,
            )?;
        } else if definition.flags.contains("REGION_PSEUDO") {
            return Err(format!(
                "regional furniture {} reaches pseudo furniture {} without a default table",
                id, replacement.id
            )
            .into());
        }
    }
    visiting.remove(id);
    regional.insert(id.to_owned());
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cdda_content::{
        CitySettingsRegistry, ContentManifest, DEFAULT_CITY_SETTINGS_ID, FurnitureRegistry,
        ItemGroupRegistry, MapgenRegistry, ModCatalog, OvermapTerrainRegistry, TerrainRegistry,
    };

    use super::bootstrap_regional_road_overmap;

    #[test]
    fn pinned_city_and_road_topology_reach_production_content() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(cdda_content::DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest");
        let root = manifest_path.parent().expect("manifest parent");
        let mods = ModCatalog::load(&manifest, root).expect("mods");
        let enabled = mods.recommended_new_world().expect("recommended mods");
        let terrain =
            TerrainRegistry::load_selected(&manifest, root, &mods, &enabled).expect("terrain");
        let furniture =
            FurnitureRegistry::load_selected(&manifest, root, &mods, &enabled).expect("furniture");
        let item_groups = ItemGroupRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("item groups");
        let mapgen = MapgenRegistry::load_selected(
            &manifest,
            root,
            &mods,
            &enabled,
            &terrain,
            &furniture,
            &item_groups,
        )
        .expect("mapgen");
        let overmap = OvermapTerrainRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("overmap terrain");
        let settings = CitySettingsRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("city settings");
        let (layout, cities, exits) = bootstrap_regional_road_overmap(
            &overmap,
            [37; 32],
            settings
                .get(DEFAULT_CITY_SETTINGS_ID)
                .expect("default city settings"),
        )
        .expect("city overmap");
        assert!(!cities.is_empty());
        assert_eq!(exits.len(), 3);
        let center = layout
            .identities
            .iter()
            .find(|identity| identity.full_id == "road_nesw")
            .expect("city center identity");
        assert_eq!(center.type_id, "road");
        assert_eq!(center.subtype_id, "road_four_way");
        assert!(
            layout
                .identities
                .iter()
                .any(|identity| identity.type_id == "road" && identity.full_id != "road_nesw")
        );
        assert!(
            mapgen.get(&center.generator_id).is_some(),
            "production city-center predecessor {:?} is blocked: {:?}",
            center.generator_id,
            mapgen.unavailable_reports(&center.generator_id)
        );
    }
}
