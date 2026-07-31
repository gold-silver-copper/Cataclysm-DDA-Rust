use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    CitySettingsDefinition, DefaultRegionTerrainFurnitureRegistry, FurnitureRegistry,
    MapgenCoordinateRange, MapgenIdChoice, MapgenRegistry, MonsterGroupRegistry,
    MonsterGroupTarget, MonsterRegistry, OvermapSpecialDefinition, OvermapSpecialRegistry,
    OvermapTerrainMatchType, OvermapTerrainRegistry, RiverSettingsDefinition,
    StartLocationDefinition, StrictMapgenAreaItemPlacement, StrictMapgenChunkChoice,
    StrictMapgenDefinition, StrictMapgenIndividualMonsterPlacement,
    StrictMapgenIndividualMonsterTarget, StrictMapgenMonsterPlacement, StrictMapgenNeighborFlags,
    StrictMapgenNeighborMatch, StrictMapgenNestedPlacement, StrictNestedMapgenDefinition,
    TerrainRegistry,
};
use cdda_protocol::{
    ItemGroupDefinitionV1, WorldgenAreaItemPlacementV1, WorldgenBuiltinMapgenV1, WorldgenCatalogV1,
    WorldgenCellV1, WorldgenCityV1, WorldgenCoordinateRangeV1, WorldgenFurniturePrototypeTargetV1,
    WorldgenFurnitureTargetV1, WorldgenIndividualMonsterPlacementV1,
    WorldgenIndividualMonsterTargetV1, WorldgenItemGroupPlacementV1, WorldgenMonsterGroupEntryV1,
    WorldgenMonsterGroupTargetV1, WorldgenMonsterGroupV1, WorldgenMonsterPlacementV1,
    WorldgenMonsterPrototypeV1, WorldgenNeighborConditionV1, WorldgenNestedChoiceV1,
    WorldgenNestedConditionsV1, WorldgenNestedGeneratorV1, WorldgenNestedPlacementV1,
    WorldgenNestedTemplateV1, WorldgenOmtGeneratorV1, WorldgenOmtIdentityV1,
    WorldgenOmtMatchTypeV1, WorldgenOvermapLayerV1, WorldgenOvermapLayoutV1, WorldgenOvermapRunV1,
    WorldgenRegionalFurnitureTableV1, WorldgenRegionalTerrainTableV1, WorldgenRiverNodeV1,
    WorldgenSpecialPlacementV1, WorldgenSpecialPopulationV1, WorldgenSpecialUniquenessV1,
    WorldgenStartLocationV1, WorldgenStartTargetV1, WorldgenTemplateV1, WorldgenTerrainTargetV1,
    WorldgenU16RangeV1, WorldgenU32RangeV1, WorldgenWeightedFurniturePrototypeV1,
    WorldgenWeightedFurnitureTargetV1, WorldgenWeightedPrototypeV1,
    WorldgenWeightedTerrainTargetV1, worldgen_catalog_is_valid, worldgen_catalog_shape_is_valid,
    worldgen_omt_matches,
};
use cdda_sim::{
    OVERMAP_BRIDGE_IDS, OVERMAP_RIVER_IDS, OVERMAP_ROAD_MASK_IDS, OvermapCitySettings,
    OvermapFixedSpecial, OvermapFixedSpecialConnection, OvermapFixedSpecialTerrain,
    OvermapRiverSettings, OvermapRoadExit, OvermapSpecialInterval, connect_overmap_special_roads,
    place_overmap_cities, place_overmap_rivers, place_overmap_roads_with_bridges,
    place_overmap_specials,
};

use super::{
    furniture_tile, monster_attack_cost, monster_blood_field_type, monster_path_settings,
    monster_size, terrain_tile,
};

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
    river_settings: &RiverSettingsDefinition,
) -> Result<
    (
        WorldgenOvermapLayoutV1,
        Vec<WorldgenCityV1>,
        Vec<WorldgenRiverNodeV1>,
    ),
    Box<dyn std::error::Error>,
> {
    let (field, rivers) = bootstrap_regional_river_overmap(terrain, world_seed, river_settings)?;
    let source = terrain
        .get_identity("road_nesw")
        .ok_or("pinned overmap-terrain catalog is missing road_nesw")?;
    let center = WorldgenOmtIdentityV1 {
        full_id: source.full_id.clone(),
        type_id: source.type_id.clone(),
        subtype_id: source.subtype_id.clone(),
        generator_id: source.generator_id.clone(),
        rotation: source.rotation,
    };
    let (layout, cities) = place_overmap_cities(
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
    )?;
    Ok((layout, cities, rivers))
}

/// Production regional layout after major-river topology, bounded branches,
/// and complete shore polishing. The returned nodes are canonical continuity
/// data rather than transient generation scratch state.
pub(super) fn bootstrap_regional_river_overmap(
    terrain: &OvermapTerrainRegistry,
    world_seed: [u8; 32],
    settings: &RiverSettingsDefinition,
) -> Result<(WorldgenOvermapLayoutV1, Vec<WorldgenRiverNodeV1>), Box<dyn std::error::Error>> {
    let field = bootstrap_regional_field_overmap(terrain)?;
    let identities = runtime_omt_identities(terrain, &OVERMAP_RIVER_IDS)?;
    place_overmap_rivers(
        world_seed,
        cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        field,
        OvermapRiverSettings {
            river_scale: settings.river_scale,
            river_frequency_millis: settings.river_frequency_millis,
            branch_chance_millis: settings.branch_chance_millis,
            branch_remerge_chance_millis: settings.branch_remerge_chance_millis,
            branch_scale_decrease_millis: settings.branch_scale_decrease_millis,
        },
        &[],
        0,
        &identities,
    )
    .map_err(Into::into)
}

/// Production regional layout after the inter-city road-topology family.
type RegionalRoadOvermap = (
    WorldgenOvermapLayoutV1,
    Vec<WorldgenCityV1>,
    Vec<WorldgenRiverNodeV1>,
    Vec<OvermapRoadExit>,
);

pub(super) fn bootstrap_regional_road_overmap(
    terrain: &OvermapTerrainRegistry,
    world_seed: [u8; 32],
    city_settings: &CitySettingsDefinition,
    river_settings: &RiverSettingsDefinition,
) -> Result<RegionalRoadOvermap, Box<dyn std::error::Error>> {
    let (city_layout, cities, rivers) =
        bootstrap_regional_city_overmap(terrain, world_seed, city_settings, river_settings)?;
    let road_identities = runtime_omt_identities(terrain, &OVERMAP_ROAD_MASK_IDS)?;
    let bridge_identities = runtime_omt_identities(terrain, &OVERMAP_BRIDGE_IDS)?;
    let (layout, exits) = place_overmap_roads_with_bridges(
        world_seed,
        cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        city_layout,
        &cities,
        &[],
        &road_identities,
        &bridge_identities,
    )?;
    Ok((layout, cities, rivers, exits))
}

/// Production regional layout after deterministic fixed-special placement and
/// generalized local-road connection routing.
type RegionalSpecialOvermap = (
    WorldgenOvermapLayoutV1,
    Vec<WorldgenCityV1>,
    Vec<WorldgenRiverNodeV1>,
    Vec<OvermapRoadExit>,
    Vec<WorldgenSpecialPlacementV1>,
);

pub(super) fn bootstrap_regional_special_overmap(
    terrain: &OvermapTerrainRegistry,
    specials: &OvermapSpecialRegistry,
    mapgen: &MapgenRegistry,
    world_seed: [u8; 32],
    city_settings: &CitySettingsDefinition,
    river_settings: &RiverSettingsDefinition,
) -> Result<RegionalSpecialOvermap, Box<dyn std::error::Error>> {
    let (layout, cities, rivers, exits) =
        bootstrap_regional_road_overmap(terrain, world_seed, city_settings, river_settings)?;
    let definitions = runtime_fixed_specials(specials, terrain, mapgen, &layout)?;
    let default_below = runtime_omt_identity(terrain, "solid_earth")?;
    let default_above = runtime_omt_identity(terrain, "open_air")?;
    let placed = place_overmap_specials(
        world_seed,
        cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        layout,
        &cities,
        &definitions,
        default_below,
        default_above,
        &BTreeSet::new(),
    )?;
    let road_identities = runtime_omt_identities(terrain, &OVERMAP_ROAD_MASK_IDS)?;
    let bridge_identities = runtime_omt_identities(terrain, &OVERMAP_BRIDGE_IDS)?;
    let layout = connect_overmap_special_roads(
        placed.layout,
        &placed.road_anchors,
        &road_identities,
        &bridge_identities,
    )?;
    Ok((layout, cities, rivers, exits, placed.placements))
}

fn runtime_fixed_specials(
    specials: &OvermapSpecialRegistry,
    terrain: &OvermapTerrainRegistry,
    mapgen: &MapgenRegistry,
    layout: &WorldgenOvermapLayoutV1,
) -> Result<Vec<OvermapFixedSpecial>, Box<dyn std::error::Error>> {
    let present_types = layout
        .identities
        .iter()
        .map(|identity| identity.type_id.as_str())
        .collect::<BTreeSet<_>>();
    specials
        .iter()
        .filter_map(|(_, definition)| {
            if !definition.placement_semantics_are_supported()
                || definition.flags.contains("EXTRADIMENSIONAL")
                || definition.flags.contains("LAKE") && !present_types.contains("lake_surface")
                || definition.flags.contains("OCEAN") && !present_types.contains("ocean_surface")
            {
                None
            } else {
                Some(compile_fixed_special(definition, specials, terrain, mapgen))
            }
        })
        .filter_map(|result| match result {
            Ok(Some(definition)) => Some(Ok(definition)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn compile_fixed_special(
    definition: &OvermapSpecialDefinition,
    specials: &OvermapSpecialRegistry,
    terrain: &OvermapTerrainRegistry,
    mapgen: &MapgenRegistry,
) -> Result<Option<OvermapFixedSpecial>, Box<dyn std::error::Error>> {
    let mut terrains = Vec::with_capacity(definition.terrains.len());
    for part in &definition.terrains {
        let mut allowed_location_types = BTreeSet::new();
        for location in &part.locations {
            let Some(location) = specials.location(location) else {
                return Ok(None);
            };
            allowed_location_types.extend(location.terrain_types.iter().cloned());
        }
        let rotated_identities = if let Some(overmap) = &part.overmap {
            let Some(peers) = terrain.rotated_peers(overmap) else {
                return Ok(None);
            };
            if peers
                .iter()
                .any(|identity| !runtime_generator_is_available(&identity.generator_id, mapgen))
            {
                return Ok(None);
            }
            peers.into_iter().map(runtime_protocol_identity).collect()
        } else {
            Vec::new()
        };
        terrains.push(OvermapFixedSpecialTerrain {
            offset: cdda_protocol::ChunkCoord {
                x: part.point[0],
                y: part.point[1],
                z: part.point[2],
            },
            rotated_identities,
            allowed_location_types,
        });
    }
    if terrains
        .iter()
        .all(|terrain| terrain.rotated_identities.is_empty())
    {
        return Ok(None);
    }
    let connection_location_ids = [
        "road", "stream", "field", "forest", "swamp", "highway", "water",
    ];
    let mut road_location_types = BTreeSet::new();
    for location in connection_location_ids {
        if let Some(location) = specials.location(location) {
            road_location_types.extend(location.terrain_types.iter().cloned());
        }
    }
    let mut connections = Vec::with_capacity(definition.connections.len());
    for connection in &definition.connections {
        let terrain_type = connection
            .terrain
            .as_deref()
            .or_else(|| {
                definition
                    .terrains
                    .iter()
                    .find(|part| part.point == connection.point)
                    .and_then(|part| part.overmap.as_deref())
                    .and_then(|id| terrain.get_identity(id))
                    .map(|identity| identity.type_id.as_str())
            })
            .unwrap_or("");
        let connection_id = connection.connection.as_deref().unwrap_or_else(|| {
            if terrain_type == "road" {
                "local_road"
            } else {
                ""
            }
        });
        if terrain_type != "road" || connection_id != "local_road" {
            return Ok(None);
        }
        connections.push(OvermapFixedSpecialConnection {
            offset: cdda_protocol::ChunkCoord {
                x: connection.point[0],
                y: connection.point[1],
                z: connection.point[2],
            },
            from: connection.from.map(|point| cdda_protocol::ChunkCoord {
                x: point[0],
                y: point[1],
                z: point[2],
            }),
            terrain_type: terrain_type.to_owned(),
            connection_id: connection_id.to_owned(),
            existing: connection.existing,
            allowed_location_types: road_location_types.clone(),
        });
    }
    let uniqueness = match (
        definition.flags.contains("OVERMAP_UNIQUE"),
        definition.flags.contains("GLOBALLY_UNIQUE"),
    ) {
        (false, false) => WorldgenSpecialUniquenessV1::None,
        (true, false) => WorldgenSpecialUniquenessV1::Overmap,
        (false, true) => WorldgenSpecialUniquenessV1::Global,
        (true, true) => return Ok(None),
    };
    let population = definition
        .monster_spawn
        .as_ref()
        .map(|spawn| -> Result<_, Box<dyn std::error::Error>> {
            Ok(WorldgenSpecialPopulationV1 {
                group_id: spawn.monster_group.clone(),
                population: WorldgenU32RangeV1 {
                    minimum: u32::try_from(spawn.population.minimum)?,
                    maximum: u32::try_from(spawn.population.maximum)?,
                },
                radius: WorldgenU16RangeV1 {
                    minimum: u16::try_from(spawn.radius.minimum)?,
                    maximum: u16::try_from(spawn.radius.maximum)?,
                },
            })
        })
        .transpose()?;
    Ok(Some(OvermapFixedSpecial {
        special_id: definition.id.clone(),
        terrains,
        connections,
        city_sizes: OvermapSpecialInterval {
            minimum: definition.city_sizes.minimum,
            maximum: definition.city_sizes.maximum,
        },
        city_distance: OvermapSpecialInterval {
            minimum: definition.city_distance.minimum,
            maximum: definition.city_distance.maximum,
        },
        occurrences: OvermapSpecialInterval {
            minimum: definition.occurrences.minimum,
            maximum: definition.occurrences.maximum,
        },
        priority: definition.priority,
        rotate: definition.rotate,
        uniqueness,
        population,
    }))
}

fn runtime_generator_is_available(generator_id: &str, mapgen: &MapgenRegistry) -> bool {
    runtime_generator_is_available_inner(generator_id, mapgen, &mut BTreeSet::new())
}

fn runtime_generator_is_available_inner(
    generator_id: &str,
    mapgen: &MapgenRegistry,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if runtime_builtin_mapgen(generator_id).is_some() {
        return true;
    }
    if !visiting.insert(generator_id.to_owned()) {
        return false;
    }
    let available = mapgen.get(generator_id).is_some_and(|definitions| {
        !definitions.is_empty()
            && definitions.iter().all(|definition| {
                definition.deferred_fields.is_empty()
                    && definition
                        .fallback_predecessor_mapgen
                        .as_deref()
                        .is_none_or(|predecessor| {
                            runtime_generator_is_available_inner(predecessor, mapgen, visiting)
                        })
            })
            && mapgen
                .strict_nested_closure(generator_id)
                .is_ok_and(|closure| {
                    closure.iter().all(|nested_id| {
                        mapgen.nested(nested_id).is_some_and(|nested| {
                            !nested.is_empty()
                                && nested
                                    .iter()
                                    .all(|definition| definition.deferred_fields.is_empty())
                        })
                    })
                })
    });
    visiting.remove(generator_id);
    available
}

fn runtime_protocol_identity(
    source: &cdda_content::OvermapTerrainIdentity,
) -> WorldgenOmtIdentityV1 {
    WorldgenOmtIdentityV1 {
        full_id: source.full_id.clone(),
        type_id: source.type_id.clone(),
        subtype_id: source.subtype_id.clone(),
        generator_id: source.generator_id.clone(),
        rotation: source.rotation,
    }
}

fn runtime_omt_identity(
    terrain: &OvermapTerrainRegistry,
    full_id: &str,
) -> Result<WorldgenOmtIdentityV1, Box<dyn std::error::Error>> {
    terrain
        .get_identity(full_id)
        .map(runtime_protocol_identity)
        .ok_or_else(|| format!("pinned overmap-terrain catalog is missing {full_id}").into())
}

fn runtime_omt_identities(
    terrain: &OvermapTerrainRegistry,
    full_ids: &[&str],
) -> Result<Vec<WorldgenOmtIdentityV1>, Box<dyn std::error::Error>> {
    full_ids
        .iter()
        .map(|full_id| {
            let source = terrain
                .get_identity(full_id)
                .ok_or_else(|| format!("pinned overmap-terrain catalog is missing {full_id}"))?;
            Ok(runtime_protocol_identity(source))
        })
        .collect()
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
    pub overmap_terrain: &'a OvermapTerrainRegistry,
    pub regions: &'a DefaultRegionTerrainFurnitureRegistry,
    pub terrain: &'a TerrainRegistry,
    pub furniture: &'a FurnitureRegistry,
    pub item_groups: &'a [ItemGroupDefinitionV1],
    pub monsters: &'a MonsterRegistry,
    pub monster_groups: &'a MonsterGroupRegistry,
}

fn runtime_monster_catalog(
    roots: BTreeSet<String>,
    mut monster_ids: BTreeSet<String>,
    groups: &MonsterGroupRegistry,
    monsters: &MonsterRegistry,
) -> Result<
    (
        Vec<WorldgenMonsterPrototypeV1>,
        Vec<WorldgenMonsterGroupV1>,
        BTreeMap<String, u16>,
        BTreeMap<String, u16>,
    ),
    Box<dyn std::error::Error>,
> {
    fn visit(
        id: &str,
        groups: &MonsterGroupRegistry,
        visiting: &mut BTreeSet<String>,
        resolved: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if resolved.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_owned()) {
            return Err(format!("monster-group dependency cycle reaches {id}"));
        }
        let group = groups
            .get(id)
            .ok_or_else(|| format!("mapgen references unknown monster group {id}"))?;
        if !group.is_runtime_static() {
            return Err(format!(
                "mapgen monster group {id} requires time, event, condition, replacement, ammo, or unknown semantics"
            ));
        }
        for entry in &group.entries {
            if let MonsterGroupTarget::Group(child) = &entry.target {
                visit(child, groups, visiting, resolved)?;
            }
        }
        visiting.remove(id);
        resolved.insert(id.to_owned());
        Ok(())
    }

    let mut group_ids = BTreeSet::new();
    for root in roots {
        visit(&root, groups, &mut BTreeSet::new(), &mut group_ids)?;
    }
    for group_id in &group_ids {
        let group = groups
            .get(group_id)
            .ok_or("resolved monster group disappeared")?;
        monster_ids.extend(group.default_monster.iter().cloned());
        monster_ids.extend(
            group
                .entries
                .iter()
                .filter_map(|entry| match &entry.target {
                    MonsterGroupTarget::Monster(id) => Some(id.clone()),
                    MonsterGroupTarget::Group(_) => None,
                }),
        );
    }
    monster_ids.remove("mon_null");
    let monster_indices = monster_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let monster_prototypes = monster_ids
        .iter()
        .map(|id| {
            let monster = monsters
                .get(id)
                .ok_or_else(|| format!("monster group references unknown MONSTER {id}"))?;
            let base = cdda_protocol::CreatureCorpsePrototypeV1 {
                monster_type_id: monster.id.clone(),
                max_hp: monster.hp,
                speed: u16::try_from(monster.speed)?,
                attack_cost_moves: monster_attack_cost(monster)?,
                aggression: i16::try_from(monster.aggression)?,
                melee_skill: u16::try_from(monster.melee_skill)?,
                dodge: u16::try_from(monster.dodge)?,
                size: monster_size(monster),
                melee_dice: u16::try_from(monster.melee_dice)?,
                melee_dice_sides: u16::try_from(monster.melee_dice_sides)?,
                can_see: monster.flags.contains("SEES"),
                vision_day: u16::try_from(monster.vision_day)?,
                vision_night: u16::try_from(monster.vision_night)?,
                stumbles: monster.flags.contains("STUMBLES"),
                bashes: monster.flags.contains("BASHES"),
                group_bash: monster.flags.contains("GROUP_BASH"),
                hears: monster.flags.contains("HEARS"),
                good_hearing: monster.flags.contains("GOODHEARING"),
                clumsy_attacks: monster.flags.contains("CLUMSY_ATTACKS"),
                immobile: monster.flags.contains("IMMOBILE"),
                pacifist: monster.flags.contains("PACIFIST"),
                can_open_doors: monster.flags.contains("CAN_OPEN_DOORS"),
                path_settings: monster_path_settings(monster)?,
                blood_field_type_id: monster_blood_field_type(monster).to_owned(),
                revives: monster.flags.contains("REVIVES"),
            };
            Ok(WorldgenMonsterPrototypeV1 {
                base,
                armor_milli: monster.finalized_armor_milli(),
                leaves_corpse: !monster.flags.contains("NO_CORPSE"),
                deferred_behavior_fields: monster.unsupported_fields.iter().cloned().collect(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let group_indices = group_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let runtime_groups = group_ids
        .iter()
        .map(|id| {
            let group = groups.get(id).ok_or("resolved monster group disappeared")?;
            Ok(WorldgenMonsterGroupV1 {
                group_id: id.clone(),
                default_prototype_index: group
                    .default_monster
                    .as_ref()
                    .filter(|id| id.as_str() != "mon_null")
                    .map(|id| {
                        monster_indices
                            .get(id)
                            .copied()
                            .ok_or("monster-group default prototype disappeared")
                    })
                    .transpose()?,
                frequency_total: group.frequency_total,
                is_animal: group.is_animal,
                is_safe: group.is_safe,
                entries: group
                    .entries
                    .iter()
                    .map(|entry| {
                        let target = match &entry.target {
                            MonsterGroupTarget::Monster(id) => {
                                WorldgenMonsterGroupTargetV1::Monster {
                                    prototype_index: *monster_indices
                                        .get(id)
                                        .ok_or("monster-group prototype disappeared")?,
                                }
                            }
                            MonsterGroupTarget::Group(id) => WorldgenMonsterGroupTargetV1::Group {
                                group_index: *group_indices
                                    .get(id)
                                    .ok_or("monster subgroup disappeared")?,
                            },
                        };
                        Ok(WorldgenMonsterGroupEntryV1 {
                            target,
                            weight: entry.weight,
                            cost_multiplier: entry.cost_multiplier,
                            pack_size: WorldgenU16RangeV1 {
                                minimum: entry.pack_minimum,
                                maximum: entry.pack_maximum,
                            },
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    Ok((
        monster_prototypes,
        runtime_groups,
        group_indices,
        monster_indices,
    ))
}

fn runtime_monster_placement(
    placement: &StrictMapgenMonsterPlacement,
    group_indices: &BTreeMap<String, u16>,
) -> Result<WorldgenMonsterPlacementV1, Box<dyn std::error::Error>> {
    Ok(WorldgenMonsterPlacementV1 {
        group_index: *group_indices
            .get(&placement.monster_group)
            .ok_or("mapgen monster-group closure disappeared")?,
        chance: WorldgenU16RangeV1 {
            minimum: placement.chance.minimum,
            maximum: placement.chance.maximum,
        },
        density_millionths: placement.density_millionths,
        repeat: WorldgenU16RangeV1 {
            minimum: placement.repeat.minimum,
            maximum: placement.repeat.maximum,
        },
        x: runtime_coordinate_range(placement.x),
        y: runtime_coordinate_range(placement.y),
    })
}

fn runtime_individual_monster_placement(
    placement: &StrictMapgenIndividualMonsterPlacement,
    group_indices: &BTreeMap<String, u16>,
    monster_indices: &BTreeMap<String, u16>,
) -> Result<WorldgenIndividualMonsterPlacementV1, Box<dyn std::error::Error>> {
    let target = match &placement.target {
        StrictMapgenIndividualMonsterTarget::Monster(id) => {
            WorldgenIndividualMonsterTargetV1::Monster {
                prototype_index: *monster_indices
                    .get(id)
                    .ok_or("individual mapgen monster prototype disappeared")?,
            }
        }
        StrictMapgenIndividualMonsterTarget::Group(id) => {
            WorldgenIndividualMonsterTargetV1::Group {
                group_index: *group_indices
                    .get(id)
                    .ok_or("individual mapgen monster group disappeared")?,
            }
        }
    };
    Ok(WorldgenIndividualMonsterPlacementV1 {
        target,
        chance_percent: WorldgenU16RangeV1 {
            minimum: placement.chance_percent.minimum,
            maximum: placement.chance_percent.maximum,
        },
        pack_size: WorldgenU16RangeV1 {
            minimum: placement.pack_size.minimum,
            maximum: placement.pack_size.maximum,
        },
        repeat: WorldgenU16RangeV1 {
            minimum: placement.repeat.minimum,
            maximum: placement.repeat.maximum,
        },
        x: runtime_coordinate_range(placement.x),
        y: runtime_coordinate_range(placement.y),
    })
}

#[derive(Clone, Copy)]
enum RuntimeBuiltinMapgen {
    Algorithm(WorldgenBuiltinMapgenV1),
    UniformTerrain(&'static str),
}

fn runtime_builtin_mapgen(generator_id: &str) -> Option<RuntimeBuiltinMapgen> {
    let algorithm = match generator_id {
        "river" => WorldgenBuiltinMapgenV1::RiverStraight,
        "river_ne" => WorldgenBuiltinMapgenV1::RiverCurved { rotation: 0 },
        "river_se" => WorldgenBuiltinMapgenV1::RiverCurved { rotation: 1 },
        "river_sw" => WorldgenBuiltinMapgenV1::RiverCurved { rotation: 2 },
        "river_nw" => WorldgenBuiltinMapgenV1::RiverCurved { rotation: 3 },
        "river_c_not_ne" => WorldgenBuiltinMapgenV1::RiverCurvedNot { rotation: 0 },
        "river_c_not_se" => WorldgenBuiltinMapgenV1::RiverCurvedNot { rotation: 1 },
        "river_c_not_sw" => WorldgenBuiltinMapgenV1::RiverCurvedNot { rotation: 2 },
        "river_c_not_nw" => WorldgenBuiltinMapgenV1::RiverCurvedNot { rotation: 3 },
        "forest_water" => WorldgenBuiltinMapgenV1::ForestWater,
        "river_center" => return Some(RuntimeBuiltinMapgen::UniformTerrain("t_water_moving_dp")),
        "solid_earth" => return Some(RuntimeBuiltinMapgen::UniformTerrain("t_soil")),
        "open_air" => return Some(RuntimeBuiltinMapgen::UniformTerrain("t_open_air")),
        "empty_rock" | "deep_rock" => {
            return Some(RuntimeBuiltinMapgen::UniformTerrain("t_rock"));
        }
        _ => return None,
    };
    Some(RuntimeBuiltinMapgen::Algorithm(algorithm))
}

fn builtin_terrain_ids(builtin: RuntimeBuiltinMapgen) -> &'static [&'static str] {
    match builtin {
        RuntimeBuiltinMapgen::UniformTerrain(id) => match id {
            "t_water_moving_dp" => &["t_water_moving_dp"],
            "t_soil" => &["t_soil"],
            "t_open_air" => &["t_open_air"],
            "t_rock" => &["t_rock"],
            _ => &[],
        },
        RuntimeBuiltinMapgen::Algorithm(WorldgenBuiltinMapgenV1::ForestWater) => {
            &["t_water_dp", "t_water_murky", "t_water_sh"]
        }
        RuntimeBuiltinMapgen::Algorithm(_) => &[
            "t_clay",
            "t_dirt",
            "t_grass",
            "t_sand",
            "t_water_moving_dp",
            "t_water_moving_sh",
        ],
    }
}

/// Exact named item-group roots referenced by every mapgen template reachable
/// from this overmap, including predecessor and nested-mapgen closures. This
/// keeps production admission driven by the selected world rather than by a
/// hand-maintained aggregate group such as `road`.
pub(super) fn runtime_mapgen_item_group_roots(
    overmap: &WorldgenOvermapLayoutV1,
    mapgen: &MapgenRegistry,
) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut generator_ids = overmap
        .identities
        .iter()
        .map(|identity| identity.generator_id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let mut predecessors = BTreeSet::new();
        for generator_id in &generator_ids {
            if runtime_builtin_mapgen(generator_id).is_some() {
                continue;
            }
            let definitions = mapgen.get(generator_id).ok_or_else(|| {
                format!("pinned mapgen {generator_id} is unavailable while collecting item groups")
            })?;
            predecessors.extend(
                definitions
                    .iter()
                    .filter_map(|definition| definition.fallback_predecessor_mapgen.clone()),
            );
        }
        let prior_len = generator_ids.len();
        generator_ids.extend(predecessors);
        if generator_ids.len() == prior_len {
            break;
        }
    }
    let mut roots = BTreeSet::new();
    for generator_id in generator_ids {
        if runtime_builtin_mapgen(&generator_id).is_some() {
            continue;
        }
        let definitions = mapgen
            .get(&generator_id)
            .ok_or_else(|| format!("pinned mapgen {generator_id} disappeared"))?;
        for definition in definitions {
            roots.extend(
                definition
                    .items
                    .values()
                    .map(|placement| placement.item_group.clone()),
            );
            roots.extend(
                definition
                    .area_items
                    .iter()
                    .map(|placement| placement.item_group.clone()),
            );
        }
        for nested_id in mapgen
            .strict_nested_closure(&generator_id)
            .map_err(|error| format!("pinned mapgen {generator_id} nested closure: {error}"))?
        {
            let definitions = mapgen
                .nested(&nested_id)
                .ok_or_else(|| format!("nested mapgen {nested_id} disappeared"))?;
            for definition in definitions {
                roots.extend(
                    definition
                        .items
                        .values()
                        .map(|placement| placement.item_group.clone()),
                );
                roots.extend(
                    definition
                        .area_items
                        .iter()
                        .map(|placement| placement.item_group.clone()),
                );
            }
        }
    }
    Ok(roots)
}

pub(super) fn runtime_mapgen_worldgen(
    overmap: WorldgenOvermapLayoutV1,
    cities: Vec<WorldgenCityV1>,
    rivers: Vec<WorldgenRiverNodeV1>,
    specials: Vec<WorldgenSpecialPlacementV1>,
    start_location: &StartLocationDefinition,
    content: RuntimeMapgenContent<'_>,
) -> Result<WorldgenCatalogV1, Box<dyn std::error::Error>> {
    let RuntimeMapgenContent {
        mapgen,
        overmap_terrain,
        regions,
        terrain,
        furniture,
        item_groups,
        monsters,
        monster_groups,
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
    let mut generator_ids = overmap
        .identities
        .iter()
        .map(|identity| identity.generator_id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let mut predecessors = BTreeSet::new();
        for omt_id in &generator_ids {
            if runtime_builtin_mapgen(omt_id).is_some() {
                continue;
            }
            let variants = mapgen.get(omt_id).ok_or_else(|| {
                let reason = mapgen
                    .unavailable_reports(omt_id)
                    .and_then(|reports| reports.first())
                    .and_then(|report| report.rejection_reason.as_deref())
                    .unwrap_or("no selected definition");
                format!("pinned mapgen {omt_id} is unavailable: {reason}")
            })?;
            predecessors.extend(
                variants
                    .iter()
                    .filter_map(|definition| definition.fallback_predecessor_mapgen.clone()),
            );
        }
        let old_len = generator_ids.len();
        generator_ids.extend(predecessors);
        if generator_ids.len() == old_len {
            break;
        }
    }
    let definitions = generator_ids
        .iter()
        .filter(|omt_id| runtime_builtin_mapgen(omt_id).is_none())
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
    let nested_closures = definitions
        .iter()
        .map(|(omt_id, _)| {
            Ok((
                (*omt_id).to_owned(),
                mapgen
                    .strict_nested_closure(omt_id)
                    .map_err(|error| format!("pinned mapgen {omt_id} nested closure: {error}"))?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;

    let mut monster_group_roots = BTreeSet::new();
    let mut individual_monster_roots = BTreeSet::new();
    monster_group_roots.extend(
        specials
            .iter()
            .filter_map(|special| special.population.as_ref())
            .map(|population| population.group_id.clone()),
    );
    for (omt_id, variants) in &definitions {
        for definition in *variants {
            monster_group_roots.extend(
                definition
                    .monster_placements
                    .iter()
                    .map(|placement| placement.monster_group.clone()),
            );
            for placement in &definition.individual_monster_placements {
                match &placement.target {
                    StrictMapgenIndividualMonsterTarget::Monster(id) => {
                        individual_monster_roots.insert(id.clone());
                    }
                    StrictMapgenIndividualMonsterTarget::Group(id) => {
                        monster_group_roots.insert(id.clone());
                    }
                }
            }
        }
        for nested_id in nested_closures
            .get(*omt_id)
            .ok_or("nested mapgen closure disappeared")?
        {
            let variants = mapgen
                .nested(nested_id)
                .ok_or_else(|| format!("nested mapgen {nested_id} disappeared"))?;
            for definition in variants {
                monster_group_roots.extend(
                    definition
                        .monster_placements
                        .iter()
                        .map(|placement| placement.monster_group.clone()),
                );
                for placement in &definition.individual_monster_placements {
                    match &placement.target {
                        StrictMapgenIndividualMonsterTarget::Monster(id) => {
                            individual_monster_roots.insert(id.clone());
                        }
                        StrictMapgenIndividualMonsterTarget::Group(id) => {
                            monster_group_roots.insert(id.clone());
                        }
                    }
                }
            }
        }
    }
    let (monster_prototypes, runtime_monster_groups, monster_group_indices, monster_indices) =
        runtime_monster_catalog(
            monster_group_roots,
            individual_monster_roots,
            monster_groups,
            monsters,
        )?;

    let mut terrain_ids = BTreeSet::new();
    let mut furniture_ids = BTreeSet::new();
    let mut regional_terrain_ids = BTreeSet::new();
    let mut regional_furniture_ids = BTreeSet::new();
    for builtin in generator_ids
        .iter()
        .filter_map(|generator_id| runtime_builtin_mapgen(generator_id))
    {
        if matches!(
            builtin,
            RuntimeBuiltinMapgen::Algorithm(WorldgenBuiltinMapgenV1::ForestWater)
        ) {
            collect_runtime_terrain_choice(
                &MapgenIdChoice::Fixed(String::from("t_region_groundcover_swamp")),
                regions,
                terrain,
                &mut terrain_ids,
                &mut regional_terrain_ids,
            )?;
        }
        for terrain_id in builtin_terrain_ids(builtin) {
            if terrain.get(terrain_id).is_none() {
                return Err(
                    format!("builtin mapgen references missing terrain {terrain_id}").into(),
                );
            }
            terrain_ids.insert((*terrain_id).to_owned());
        }
    }
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
        for nested_id in nested_closures
            .get(*omt_id)
            .ok_or("nested mapgen closure disappeared")?
        {
            let variants = mapgen
                .nested(nested_id)
                .ok_or_else(|| format!("nested mapgen {nested_id} disappeared"))?;
            for definition in variants {
                if matches!(definition.fill_terrain, Some(MapgenIdChoice::Weighted(_))) {
                    return Err(format!(
                        "nested mapgen {nested_id} uses a weighted one-time fill that worldgen v2 cannot encode"
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

    let mut omt_generators = definitions
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
                            &monster_group_indices,
                            &monster_indices,
                            &overmap,
                            overmap_terrain,
                        )
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
                nested_generators: nested_closures
                    .get(*omt_id)
                    .ok_or("nested mapgen closure disappeared")?
                    .iter()
                    .map(|nested_id| {
                        let definitions = mapgen
                            .nested(nested_id)
                            .ok_or_else(|| format!("nested mapgen {nested_id} disappeared"))?;
                        Ok(WorldgenNestedGeneratorV1 {
                            nested_id: nested_id.clone(),
                            templates: definitions
                                .iter()
                                .map(|definition| {
                                    runtime_nested_mapgen_template(
                                        definition,
                                        &terrain_indices,
                                        &furniture_indices,
                                        &regional_terrain_indices,
                                        &regional_furniture_indices,
                                        &monster_group_indices,
                                        &monster_indices,
                                        &overmap,
                                        overmap_terrain,
                                    )
                                })
                                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    omt_generators.extend(
        generator_ids
            .iter()
            .filter_map(|generator_id| {
                runtime_builtin_mapgen(generator_id).map(|builtin| (generator_id.as_str(), builtin))
            })
            .map(|(generator_id, builtin)| {
                runtime_builtin_generator(generator_id, builtin, &terrain_indices)
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
    );
    omt_generators.sort_by(|left, right| left.omt_id.cmp(&right.omt_id));
    let catalog = WorldgenCatalogV1 {
        generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        overmap,
        cities,
        rivers,
        specials,
        start_location: Some(start_location),
        terrain_prototypes,
        furniture_prototypes,
        monster_prototypes,
        monster_groups: runtime_monster_groups,
        regional_terrain,
        regional_furniture,
        omt_generators,
    };
    if !worldgen_catalog_shape_is_valid(&catalog) {
        return Err(
            "pinned mapgens produced an invalid coordinate-owned worldgen catalog shape".into(),
        );
    }
    if !worldgen_catalog_is_valid(&catalog, item_groups) {
        let available = item_groups
            .iter()
            .map(|definition| definition.group_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut required = BTreeSet::new();
        for generator in &catalog.omt_generators {
            for template in &generator.templates {
                required.extend(
                    template
                        .cells
                        .iter()
                        .filter_map(|cell| cell.item_group.as_ref())
                        .map(|placement| placement.group_id.as_str()),
                );
                required.extend(
                    template
                        .area_items
                        .iter()
                        .map(|placement| placement.item_group.group_id.as_str()),
                );
            }
            for nested in &generator.nested_generators {
                for template in &nested.templates {
                    required.extend(
                        template
                            .cells
                            .iter()
                            .filter_map(|cell| cell.item_group.as_ref())
                            .map(|placement| placement.group_id.as_str()),
                    );
                    required.extend(
                        template
                            .area_items
                            .iter()
                            .map(|placement| placement.item_group.group_id.as_str()),
                    );
                }
            }
        }
        let missing = required.difference(&available).copied().collect::<Vec<_>>();
        return Err(format!(
            "pinned mapgens reference an invalid or incomplete item-group catalog; missing {missing:?}"
        )
        .into());
    }
    Ok(catalog)
}

fn runtime_builtin_generator(
    generator_id: &str,
    builtin: RuntimeBuiltinMapgen,
    terrain_indices: &BTreeMap<String, u16>,
) -> Result<WorldgenOmtGeneratorV1, Box<dyn std::error::Error>> {
    let (builtin, cells) = match builtin {
        RuntimeBuiltinMapgen::Algorithm(builtin) => (Some(builtin), Vec::new()),
        RuntimeBuiltinMapgen::UniformTerrain(terrain_id) => {
            let prototype = *terrain_indices
                .get(terrain_id)
                .ok_or("builtin terrain disappeared from prototype closure")?;
            let cell = WorldgenCellV1 {
                terrain: vec![vec![WorldgenWeightedTerrainTargetV1 {
                    target: WorldgenTerrainTargetV1::Prototype(prototype),
                    weight: 1,
                }]],
                furniture: vec![vec![WorldgenWeightedFurnitureTargetV1 {
                    target: WorldgenFurnitureTargetV1::None,
                    weight: 1,
                }]],
                item_group: None,
            };
            (None, vec![cell; cdda_protocol::WORLDGEN_CELLS_PER_OMT])
        }
    };
    let deferred_fields = if builtin == Some(WorldgenBuiltinMapgenV1::ForestWater) {
        vec![String::from("forest_components")]
    } else {
        Vec::new()
    };
    Ok(WorldgenOmtGeneratorV1 {
        omt_id: generator_id.to_owned(),
        templates: vec![WorldgenTemplateV1 {
            weight: 1_000,
            predecessor_id: None,
            builtin,
            cells,
            nested: Vec::new(),
            area_items: Vec::new(),
            monster_placements: Vec::new(),
            individual_monster_placements: Vec::new(),
            erase_all_before_placing_terrain: false,
            deferred_fields,
        }],
        nested_generators: Vec::new(),
    })
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
    monster_groups: &BTreeMap<String, u16>,
    monsters: &BTreeMap<String, u16>,
    overmap: &WorldgenOvermapLayoutV1,
    overmap_terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenTemplateV1, Box<dyn std::error::Error>> {
    Ok(WorldgenTemplateV1 {
        weight: definition.weight,
        predecessor_id: definition.fallback_predecessor_mapgen.clone(),
        builtin: None,
        cells: runtime_mapgen_cells(
            &definition.source,
            &definition.cells,
            definition.fill_terrain.as_ref(),
            &definition.terrain,
            &definition.furniture,
            &definition.items,
            definition.fallback_predecessor_mapgen.is_some(),
            terrain,
            furniture,
            regional_terrain,
            regional_furniture,
        )?,
        nested: definition
            .nested
            .iter()
            .map(|placement| runtime_nested_placement(placement, overmap, overmap_terrain))
            .collect::<Result<Vec<_>, _>>()?,
        area_items: definition
            .area_items
            .iter()
            .map(runtime_area_item_placement)
            .collect(),
        monster_placements: definition
            .monster_placements
            .iter()
            .map(|placement| runtime_monster_placement(placement, monster_groups))
            .collect::<Result<Vec<_>, _>>()?,
        individual_monster_placements: definition
            .individual_monster_placements
            .iter()
            .map(|placement| {
                runtime_individual_monster_placement(placement, monster_groups, monsters)
            })
            .collect::<Result<Vec<_>, _>>()?,
        erase_all_before_placing_terrain: definition.erase_all_before_placing_terrain,
        deferred_fields: definition.deferred_fields.iter().cloned().collect(),
    })
}

fn runtime_nested_mapgen_template(
    definition: &StrictNestedMapgenDefinition,
    terrain: &BTreeMap<String, u16>,
    furniture: &BTreeMap<String, u16>,
    regional_terrain: &BTreeMap<String, u16>,
    regional_furniture: &BTreeMap<String, u16>,
    monster_groups: &BTreeMap<String, u16>,
    monsters: &BTreeMap<String, u16>,
    overmap: &WorldgenOvermapLayoutV1,
    overmap_terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenNestedTemplateV1, Box<dyn std::error::Error>> {
    Ok(WorldgenNestedTemplateV1 {
        weight: definition.weight,
        width: definition.width,
        height: definition.height,
        cells: runtime_mapgen_cells(
            &definition.source,
            &definition.cells,
            definition.fill_terrain.as_ref(),
            &definition.terrain,
            &definition.furniture,
            &definition.items,
            true,
            terrain,
            furniture,
            regional_terrain,
            regional_furniture,
        )?,
        nested: definition
            .nested
            .iter()
            .map(|placement| runtime_nested_placement(placement, overmap, overmap_terrain))
            .collect::<Result<Vec<_>, _>>()?,
        area_items: definition
            .area_items
            .iter()
            .map(runtime_area_item_placement)
            .collect(),
        monster_placements: definition
            .monster_placements
            .iter()
            .map(|placement| runtime_monster_placement(placement, monster_groups))
            .collect::<Result<Vec<_>, _>>()?,
        individual_monster_placements: definition
            .individual_monster_placements
            .iter()
            .map(|placement| {
                runtime_individual_monster_placement(placement, monster_groups, monsters)
            })
            .collect::<Result<Vec<_>, _>>()?,
        erase_all_before_placing_terrain: definition.erase_all_before_placing_terrain,
        deferred_fields: definition.deferred_fields.iter().cloned().collect(),
    })
}

#[allow(clippy::too_many_arguments)]
fn runtime_mapgen_cells(
    source: &str,
    glyphs: &[String],
    fill: Option<&MapgenIdChoice>,
    terrain_bindings: &BTreeMap<String, Vec<MapgenIdChoice>>,
    furniture_bindings: &BTreeMap<String, Vec<MapgenIdChoice>>,
    item_bindings: &BTreeMap<String, cdda_content::StrictMapgenItemPlacement>,
    overlay: bool,
    terrain: &BTreeMap<String, u16>,
    furniture: &BTreeMap<String, u16>,
    regional_terrain: &BTreeMap<String, u16>,
    regional_furniture: &BTreeMap<String, u16>,
) -> Result<Vec<WorldgenCellV1>, Box<dyn std::error::Error>> {
    glyphs
        .iter()
        .map(|glyph| {
            let mut terrain_layers = fill
                .iter()
                .map(|choice| runtime_mapgen_terrain_choice(choice, terrain, regional_terrain))
                .collect::<Result<Vec<_>, _>>()?;
            terrain_layers.extend(
                terrain_bindings
                    .get(glyph)
                    .into_iter()
                    .flatten()
                    .map(|choice| runtime_mapgen_terrain_choice(choice, terrain, regional_terrain))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            if terrain_layers.is_empty() && !overlay {
                return Err(format!("mapgen {source} has no terrain for glyph {glyph:?}").into());
            }
            let mut furniture_layers = furniture_bindings
                .get(glyph)
                .into_iter()
                .flatten()
                .map(|choice| {
                    runtime_mapgen_furniture_choice(choice, furniture, regional_furniture)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if furniture_layers.is_empty() && !overlay {
                furniture_layers.push(vec![WorldgenWeightedFurnitureTargetV1 {
                    target: WorldgenFurnitureTargetV1::None,
                    weight: 1,
                }]);
            }
            Ok(WorldgenCellV1 {
                terrain: terrain_layers,
                furniture: furniture_layers,
                item_group: item_bindings.get(glyph).map(|placement| {
                    WorldgenItemGroupPlacementV1 {
                        group_id: placement.item_group.clone(),
                        chance: placement.chance,
                        repeat_minimum: placement.repeat_minimum,
                        repeat_maximum: placement.repeat_maximum,
                    }
                }),
            })
        })
        .collect()
}

fn runtime_area_item_placement(
    placement: &StrictMapgenAreaItemPlacement,
) -> WorldgenAreaItemPlacementV1 {
    WorldgenAreaItemPlacementV1 {
        item_group: WorldgenItemGroupPlacementV1 {
            group_id: placement.item_group.clone(),
            chance: placement.chance,
            repeat_minimum: 1,
            repeat_maximum: 1,
        },
        x: runtime_coordinate_range(placement.x),
        y: runtime_coordinate_range(placement.y),
    }
}

const fn runtime_coordinate_range(range: MapgenCoordinateRange) -> WorldgenCoordinateRangeV1 {
    WorldgenCoordinateRangeV1 {
        minimum: range.minimum,
        maximum: range.maximum,
    }
}

fn runtime_nested_placement(
    placement: &StrictMapgenNestedPlacement,
    overmap: &WorldgenOvermapLayoutV1,
    overmap_terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenNestedPlacementV1, Box<dyn std::error::Error>> {
    let mut predecessor_ids = placement.conditions.predecessors.clone();
    predecessor_ids.sort();
    Ok(WorldgenNestedPlacementV1 {
        chunks: runtime_nested_choices(&placement.chunks),
        else_chunks: runtime_nested_choices(&placement.else_chunks),
        x: runtime_coordinate_range(placement.x),
        y: runtime_coordinate_range(placement.y),
        conditions: WorldgenNestedConditionsV1 {
            all_neighbors: placement
                .conditions
                .neighbors
                .iter()
                .map(|condition| runtime_neighbor_match(condition, overmap))
                .chain(
                    placement.conditions.flags.iter().map(|condition| {
                        runtime_neighbor_flags(condition, overmap, overmap_terrain)
                    }),
                )
                .collect::<Result<Vec<_>, _>>()?,
            any_neighbors: placement
                .conditions
                .flags_any
                .iter()
                .map(|condition| runtime_neighbor_flags(condition, overmap, overmap_terrain))
                .collect::<Result<Vec<_>, _>>()?,
            predecessor_ids,
        },
    })
}

fn runtime_nested_choices(choices: &[StrictMapgenChunkChoice]) -> Vec<WorldgenNestedChoiceV1> {
    choices
        .iter()
        .map(|choice| WorldgenNestedChoiceV1 {
            nested_id: choice.nested_id.clone(),
            weight: choice.weight,
        })
        .collect()
}

fn runtime_neighbor_match(
    condition: &StrictMapgenNeighborMatch,
    overmap: &WorldgenOvermapLayoutV1,
) -> Result<WorldgenNeighborConditionV1, Box<dyn std::error::Error>> {
    let (offset_x, offset_y) = runtime_neighbor_offset(&condition.direction)?;
    let allowed_identity_ids = overmap
        .identities
        .iter()
        .filter(|identity| {
            condition.alternatives.iter().any(|alternative| {
                worldgen_omt_matches(
                    &alternative.omt,
                    runtime_match_type(alternative.match_type),
                    identity,
                )
            })
        })
        .map(|identity| identity.full_id.clone())
        .collect();
    Ok(WorldgenNeighborConditionV1 {
        offset_x,
        offset_y,
        allowed_identity_ids,
    })
}

fn runtime_neighbor_flags(
    condition: &StrictMapgenNeighborFlags,
    overmap: &WorldgenOvermapLayoutV1,
    overmap_terrain: &OvermapTerrainRegistry,
) -> Result<WorldgenNeighborConditionV1, Box<dyn std::error::Error>> {
    let (offset_x, offset_y) = runtime_neighbor_offset(&condition.direction)?;
    let allowed_identity_ids = overmap
        .identities
        .iter()
        .map(|identity| {
            let definition = overmap_terrain
                .get_type(&identity.type_id)
                .ok_or_else(|| format!("overmap terrain type {} disappeared", identity.type_id))?;
            Ok(condition
                .flags
                .iter()
                .any(|flag| definition.flags.contains(flag))
                .then(|| identity.full_id.clone()))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(WorldgenNeighborConditionV1 {
        offset_x,
        offset_y,
        allowed_identity_ids,
    })
}

const fn runtime_match_type(match_type: OvermapTerrainMatchType) -> WorldgenOmtMatchTypeV1 {
    match match_type {
        OvermapTerrainMatchType::Exact => WorldgenOmtMatchTypeV1::Exact,
        OvermapTerrainMatchType::Type => WorldgenOmtMatchTypeV1::Type,
        OvermapTerrainMatchType::Subtype => WorldgenOmtMatchTypeV1::Subtype,
        OvermapTerrainMatchType::Prefix => WorldgenOmtMatchTypeV1::Prefix,
        OvermapTerrainMatchType::Contains => WorldgenOmtMatchTypeV1::Contains,
    }
}

fn runtime_neighbor_offset(direction: &str) -> Result<(i8, i8), Box<dyn std::error::Error>> {
    match direction {
        "north" => Ok((0, -1)),
        "north_east" => Ok((1, -1)),
        "east" => Ok((1, 0)),
        "south_east" => Ok((1, 1)),
        "south" => Ok((0, 1)),
        "south_west" => Ok((-1, 1)),
        "west" => Ok((-1, 0)),
        "north_west" => Ok((-1, -1)),
        _ => Err(format!("invalid retained mapgen direction {direction:?}").into()),
    }
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
        CitySettingsRegistry, ContentManifest, DEFAULT_CITY_SETTINGS_ID, DEFAULT_RIVER_SETTINGS_ID,
        DefaultRegionTerrainFurnitureRegistry, FurnitureRegistry, ItemGroupRegistry,
        MapgenRegistry, ModCatalog, MonsterGroupRegistry, MonsterRegistry, OvermapTerrainRegistry,
        RiverSettingsRegistry, StartLocationRegistry, TerrainRegistry,
    };
    use cdda_protocol::{
        ItemGroupDefinitionV1, ItemGroupGraphV1, ItemGroupKindV1, ItemGroupNodeV1, WorldPosition,
        worldgen_omt_identity_at,
    };
    use cdda_sim::{ReservedIdBlock, WorldState};

    use super::{RuntimeMapgenContent, bootstrap_regional_road_overmap, runtime_mapgen_worldgen};

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
        let monsters =
            MonsterRegistry::load_selected(&manifest, root, &mods, &enabled).expect("monsters");
        let monster_groups = MonsterGroupRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("monster groups");
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
        for root_id in [
            "road_end",
            "road_straight",
            "road_curved",
            "road_tee",
            "road_four_way",
        ] {
            let closure = mapgen
                .strict_nested_closure(root_id)
                .unwrap_or_else(|error| panic!("{root_id} nested closure is blocked: {error}"));
            assert!(!closure.is_empty(), "{root_id} should use named mapgen");
        }
        let overmap = OvermapTerrainRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("overmap terrain");
        let settings = CitySettingsRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("city settings");
        let river_settings = RiverSettingsRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("river settings");
        let (layout, cities, rivers, exits) = bootstrap_regional_road_overmap(
            &overmap,
            [37; 32],
            settings
                .get(DEFAULT_CITY_SETTINGS_ID)
                .expect("default city settings"),
            river_settings
                .get(DEFAULT_RIVER_SETTINGS_ID)
                .expect("default river settings"),
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
            "production city-center generator {:?} is blocked: {:?}",
            center.generator_id,
            mapgen.unavailable_reports(&center.generator_id)
        );

        let regions = DefaultRegionTerrainFurnitureRegistry::load_selected(
            &manifest, root, &mods, &enabled, &terrain, &furniture,
        )
        .expect("regional substitutions");
        let starts = StartLocationRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("start locations");
        let empty_group = |group_id: &str| ItemGroupDefinitionV1 {
            group_id: group_id.to_owned(),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: Vec::new(),
                }],
                wrapper: None,
            },
        };
        let runtime_groups = [
            "SUS_trash_floor",
            "SUS_trash_trashcan_public",
            "fast_food",
            "field",
            "road",
        ]
        .into_iter()
        .map(empty_group)
        .collect::<Vec<_>>();
        let catalog = runtime_mapgen_worldgen(
            layout,
            cities,
            rivers,
            Vec::new(),
            starts.get("sloc_field").expect("field start"),
            RuntimeMapgenContent {
                mapgen: &mapgen,
                overmap_terrain: &overmap,
                regions: &regions,
                terrain: &terrain,
                furniture: &furniture,
                item_groups: &runtime_groups,
                monsters: &monsters,
                monster_groups: &monster_groups,
            },
        )
        .expect("production road mapgen should compile");
        let road_omt = (-86..86)
            .flat_map(|y| (-86..86).map(move |x| cdda_protocol::ChunkCoord { x, y, z: 0 }))
            .find(|coord| {
                worldgen_omt_identity_at(&catalog, *coord)
                    .is_some_and(|identity| identity.type_id == "road")
            })
            .expect("road coordinate");
        let mut world = WorldState::new(1, [37; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("ID block"))
            .expect("install ID block");
        world
            .register_item_group_catalog(runtime_groups)
            .expect("register empty production group identities");
        world
            .configure_worldgen(catalog)
            .expect("configure production road worldgen");
        world
            .generate_initial_bubble(WorldPosition {
                x: road_omt.x * 24 + 12,
                y: road_omt.y * 24 + 12,
                z: 0,
            })
            .expect("materialize a road-centered active bubble");
        let snapshot = world.snapshot();
        assert!(snapshot.chunks.iter().any(|chunk| {
            chunk
                .tiles
                .iter()
                .any(|tile| tile.terrain_id.starts_with("t_pavement"))
        }));
    }
}
