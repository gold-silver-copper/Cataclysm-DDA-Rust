use std::collections::{BTreeMap, BTreeSet, VecDeque};

use cdda_content::{
    AmmunitionEffectRegistry, CitySettingsDefinition, DefaultRegionTerrainFurnitureRegistry,
    DescriptionSnippetRegistry, EffectTypeRegistry, FieldTypeRegistry, FurnitureRegistry,
    ItemRegistry, MapgenCoordinateRange, MapgenIdChoice, MapgenRegistry,
    MonsterAttackEffectDefinition, MonsterGroupRegistry, MonsterGroupTarget, MonsterRegistry,
    MonsterSpecialAttackKind, OvermapSpecialDefinition, OvermapSpecialRegistry,
    OvermapTerrainMatchType, OvermapTerrainRegistry, RiverSettingsDefinition, SpellRegistry,
    StartLocationDefinition, StrictMapgenAreaItemPlacement, StrictMapgenChunkChoice,
    StrictMapgenDefinition, StrictMapgenIndividualMonsterPlacement,
    StrictMapgenIndividualMonsterTarget, StrictMapgenMonsterPlacement, StrictMapgenNeighborFlags,
    StrictMapgenNeighborMatch, StrictMapgenNestedPlacement, StrictMapgenNpcPlacement,
    StrictNestedMapgenDefinition, TerrainRegistry,
};
use cdda_protocol::{
    ActorEffectLimbScoreModifierV1, ActorEffectModifiersV1, EocDefinitionV1, ItemGroupDefinitionV1,
    NpcTemplateV1, WorldgenAreaItemPlacementV1, WorldgenBuiltinMapgenV1, WorldgenCatalogV1,
    WorldgenCellV1, WorldgenCityV1, WorldgenCoordinateRangeV1, WorldgenFurniturePrototypeTargetV1,
    WorldgenFurnitureTargetV1, WorldgenIndividualMonsterPlacementV1,
    WorldgenIndividualMonsterTargetV1, WorldgenItemGroupPlacementV1, WorldgenMonsterAttackEffectV1,
    WorldgenMonsterEffectIntensityApplicationV1, WorldgenMonsterGroupEntryV1,
    WorldgenMonsterGroupTargetV1, WorldgenMonsterGroupV1, WorldgenMonsterGunRangeV1,
    WorldgenMonsterMeleeDamageUnitV1, WorldgenMonsterPlacementV1,
    WorldgenMonsterProjectileEffectV1, WorldgenMonsterProjectileFieldEffectV1,
    WorldgenMonsterProjectileOnHitEffectV1, WorldgenMonsterPrototypeV1,
    WorldgenMonsterSpecialAttackKindV1, WorldgenMonsterSpecialAttackV1,
    WorldgenNeighborConditionV1, WorldgenNestedChoiceV1, WorldgenNestedConditionsV1,
    WorldgenNestedGeneratorV1, WorldgenNestedPlacementV1, WorldgenNestedTemplateV1,
    WorldgenNpcNameCategoryV1, WorldgenNpcNameChoiceV1, WorldgenNpcPlacementV1,
    WorldgenOmtGeneratorV1, WorldgenOmtIdentityV1, WorldgenOmtMatchTypeV1, WorldgenOvermapLayerV1,
    WorldgenOvermapLayoutV1, WorldgenOvermapRunV1, WorldgenRegionalFurnitureTableV1,
    WorldgenRegionalTerrainTableV1, WorldgenRiverNodeV1, WorldgenSpecialPlacementV1,
    WorldgenSpecialPopulationV1, WorldgenSpecialUniquenessV1, WorldgenStartLocationV1,
    WorldgenStartTargetV1, WorldgenTemplateV1, WorldgenTerrainTargetV1, WorldgenU16RangeV1,
    WorldgenU32RangeV1, WorldgenWeightedFurniturePrototypeV1, WorldgenWeightedFurnitureTargetV1,
    WorldgenWeightedPrototypeV1, WorldgenWeightedTerrainTargetV1, worldgen_catalog_is_valid,
    worldgen_catalog_shape_is_valid, worldgen_omt_matches,
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

const MALE_FULL_NAME_CATEGORY: &str = "<male_full_name>";
const FEMALE_FULL_NAME_CATEGORY: &str = "<female_full_name>";

fn runtime_npc_generated_name_categories(template: &NpcTemplateV1) -> Option<Vec<String>> {
    if template.name_unique.is_some() {
        return Some(Vec::new());
    }
    match template.gender.as_deref() {
        Some("male") => Some(vec![MALE_FULL_NAME_CATEGORY.to_owned()]),
        Some("female") => Some(vec![FEMALE_FULL_NAME_CATEGORY.to_owned()]),
        None => Some(vec![
            MALE_FULL_NAME_CATEGORY.to_owned(),
            FEMALE_FULL_NAME_CATEGORY.to_owned(),
        ]),
        Some(_) => None,
    }
}

pub(super) fn runtime_mapgen_npc_template_ids(
    templates: &[NpcTemplateV1],
    snippets: &DescriptionSnippetRegistry,
) -> BTreeSet<String> {
    if snippets.get(MALE_FULL_NAME_CATEGORY).is_none()
        || snippets.get(FEMALE_FULL_NAME_CATEGORY).is_none()
    {
        return templates
            .iter()
            .filter(|template| template.name_unique.is_some())
            .map(|template| template.template_id.clone())
            .collect();
    }
    templates
        .iter()
        .filter(|template| runtime_npc_generated_name_categories(template).is_some())
        .map(|template| template.template_id.clone())
        .collect()
}

pub(super) fn bootstrap_regional_special_overmap(
    terrain: &OvermapTerrainRegistry,
    specials: &OvermapSpecialRegistry,
    mapgen: &MapgenRegistry,
    npc_template_ids: &BTreeSet<String>,
    world_seed: [u8; 32],
    city_settings: &CitySettingsDefinition,
    river_settings: &RiverSettingsDefinition,
) -> Result<RegionalSpecialOvermap, Box<dyn std::error::Error>> {
    let (layout, cities, rivers, exits) =
        bootstrap_regional_road_overmap(terrain, world_seed, city_settings, river_settings)?;
    let definitions = runtime_fixed_specials(specials, terrain, mapgen, npc_template_ids, &layout)?;
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
    npc_template_ids: &BTreeSet<String>,
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
                Some(compile_fixed_special(
                    definition,
                    specials,
                    terrain,
                    mapgen,
                    npc_template_ids,
                ))
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
    npc_template_ids: &BTreeSet<String>,
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
            if peers.iter().any(|identity| {
                !runtime_generator_is_available(&identity.generator_id, mapgen, npc_template_ids)
            }) {
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

fn runtime_generator_is_available(
    generator_id: &str,
    mapgen: &MapgenRegistry,
    npc_template_ids: &BTreeSet<String>,
) -> bool {
    runtime_generator_is_available_inner(
        generator_id,
        mapgen,
        npc_template_ids,
        &mut BTreeSet::new(),
    )
}

fn runtime_generator_is_available_inner(
    generator_id: &str,
    mapgen: &MapgenRegistry,
    npc_template_ids: &BTreeSet<String>,
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
                        .npc_placements
                        .iter()
                        .all(|placement| npc_template_ids.contains(&placement.template_id))
                    && definition
                        .fallback_predecessor_mapgen
                        .as_deref()
                        .is_none_or(|predecessor| {
                            runtime_generator_is_available_inner(
                                predecessor,
                                mapgen,
                                npc_template_ids,
                                visiting,
                            )
                        })
            })
            && mapgen
                .strict_nested_closure(generator_id)
                .is_ok_and(|closure| {
                    closure.iter().all(|nested_id| {
                        mapgen.nested(nested_id).is_some_and(|nested| {
                            !nested.is_empty()
                                && nested.iter().all(|definition| {
                                    definition.deferred_fields.is_empty()
                                        && definition.npc_placements.iter().all(|placement| {
                                            npc_template_ids.contains(&placement.template_id)
                                        })
                                })
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
    pub snippets: &'a DescriptionSnippetRegistry,
    pub npc_templates: &'a [NpcTemplateV1],
    pub items: &'a ItemRegistry,
    pub ammunition_effects: &'a AmmunitionEffectRegistry,
    pub effect_types: &'a EffectTypeRegistry,
    pub fields: &'a FieldTypeRegistry,
    pub monsters: &'a MonsterRegistry,
    pub spells: &'a SpellRegistry,
    pub monster_groups: &'a MonsterGroupRegistry,
    pub eoc_definitions: &'a [EocDefinitionV1],
}

#[derive(Clone)]
struct RuntimeMonsterGunProfile {
    damage: Vec<WorldgenMonsterMeleeDamageUnitV1>,
    ammunition_type_id: String,
    ranges: Vec<WorldgenMonsterGunRangeV1>,
    item_range: u32,
    dispersion: u32,
    sound_volume: u16,
    targeting_cost_moves: u32,
    require_targeting_player: bool,
    targeting_timeout_turns: u32,
    targeting_timeout_extend_turns: i32,
    targeting_sound: String,
    targeting_volume: u16,
    laser_lock: bool,
    projectile_effects: Vec<WorldgenMonsterProjectileEffectV1>,
    no_damage_scaling: bool,
    blinds_eyes: bool,
    target_moving_vehicles: bool,
}

fn runtime_monster_attack_effect(
    effect: &MonsterAttackEffectDefinition,
    effect_types: &EffectTypeRegistry,
    requires_cut_or_stab_damage: bool,
) -> Option<WorldgenMonsterAttackEffectV1> {
    if !effect.permanent && effect.duration_turns.0 == 0 {
        return None;
    }
    let count = effect
        .intensity
        .1
        .checked_sub(effect.intensity.0)?
        .checked_add(1)?;
    if count > 64 {
        return None;
    }
    let mut applications = Vec::with_capacity(usize::try_from(count).ok()?);
    let mut maximum_accumulated_duration_turns = None;
    let mut duration_add_percent = None;
    let mut blocked_by_effect_ids = None;
    for requested_intensity in effect.intensity.0..=effect.intensity.1 {
        let resolved = effect_types.resolve_application(&effect.effect_id, requested_intensity)?;
        let maximum_duration = u32::try_from(resolved.maximum_duration_seconds).ok()?;
        if maximum_duration == 0
            || resolved.duration_add_percent > 1_000
            || resolved.intensity == 0
            || resolved.intensity > 1_000_000
        {
            return None;
        }
        if maximum_accumulated_duration_turns
            .replace(maximum_duration)
            .is_some_and(|previous| previous != maximum_duration)
            || duration_add_percent
                .replace(resolved.duration_add_percent)
                .is_some_and(|previous| previous != resolved.duration_add_percent)
            || blocked_by_effect_ids
                .replace(resolved.blocked_by_effect_ids.clone())
                .is_some_and(|previous| previous != resolved.blocked_by_effect_ids)
        {
            return None;
        }
        let modifiers = ActorEffectModifiersV1 {
            strength: resolved.strength_modifier,
            dexterity: resolved.dexterity_modifier,
            intelligence: resolved.intelligence_modifier,
            perception: resolved.perception_modifier,
            speed: resolved.speed_modifier,
            limb_scores: resolved
                .limb_score_multipliers
                .into_iter()
                .map(
                    |(score_id, multiplier_millionths)| ActorEffectLimbScoreModifierV1 {
                        score_id,
                        multiplier_millionths,
                    },
                )
                .collect(),
        };
        if !cdda_protocol::actor_effect_modifiers_are_valid(&modifiers) {
            return None;
        }
        applications.push(WorldgenMonsterEffectIntensityApplicationV1 {
            intensity: resolved.intensity,
            modifiers,
        });
    }
    let blocked_by_effect_ids = blocked_by_effect_ids?;
    if blocked_by_effect_ids.len() > 64
        || blocked_by_effect_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || blocked_by_effect_ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 512 || id.chars().any(char::is_control))
    {
        return None;
    }
    Some(WorldgenMonsterAttackEffectV1 {
        effect_id: effect.effect_id.clone(),
        chance_millionths: effect.chance_millionths,
        permanent: effect.permanent,
        affect_hit_body_part: effect.affect_hit_body_part,
        requires_cut_or_stab_damage,
        body_part_id: effect.body_part_id.clone(),
        duration_minimum_turns: effect.duration_turns.0,
        duration_maximum_turns: effect.duration_turns.1,
        intensity_minimum: effect.intensity.0,
        intensity_maximum: effect.intensity.1,
        maximum_accumulated_duration_turns: maximum_accumulated_duration_turns?,
        duration_add_percent: duration_add_percent?,
        blocked_by_effect_ids,
        intensity_applications: applications,
    })
}

#[derive(Clone)]
struct RuntimeMonsterSpellProfile {
    summoned_monster_type_id: String,
    damage_type_id: String,
    minimum_damage: u16,
    maximum_damage: u16,
    status_effect_id: String,
    minimum_duration_turns: u32,
    maximum_duration_turns: u32,
    eoc_id: String,
    target_self: bool,
    no_projectile: bool,
    minimum_summons: u16,
    maximum_summons: u16,
    random_summons: bool,
    range: u32,
    aoe: u8,
    casting_time_moves: u32,
    field_type_id: String,
    field_chance: u32,
    field_intensity: u8,
    field_intensity_variance_millionths: u32,
    field_duration_turns: u32,
    targets_hostile: bool,
    targets_ground: bool,
}

fn runtime_monster_gun_profile(
    attack: &cdda_content::MonsterSpecialAttackDefinition,
    items: &ItemRegistry,
    starting_ammunition: &BTreeMap<String, u32>,
    ammunition_effects: &AmmunitionEffectRegistry,
    effect_types: &EffectTypeRegistry,
    fields: &FieldTypeRegistry,
) -> Result<Option<RuntimeMonsterGunProfile>, Box<dyn std::error::Error>> {
    if attack.kind != MonsterSpecialAttackKind::Gun {
        return Ok(None);
    }
    let gun = items.get(&attack.gun_type_id).ok_or_else(|| {
        format!(
            "monster gun actor references unknown item {}",
            attack.gun_type_id
        )
    })?;
    let ranges = attack
        .gun_ranges
        .iter()
        .map(|range| {
            let mode_id = if range.mode_id.is_empty() {
                "DEFAULT"
            } else {
                &range.mode_id
            };
            let mode = gun.gun_modes.get(mode_id)?;
            if mode.shot_count == 0
                || mode.shot_count > 100
                || !mode.flags.iter().all(|flag| flag == "NPC_AVOID")
            {
                return None;
            }
            Some(WorldgenMonsterGunRangeV1 {
                minimum: range.minimum,
                maximum: range.maximum,
                mode_id: mode_id.to_owned(),
                shot_count: mode.shot_count,
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(ranges) = ranges.filter(|ranges| !ranges.is_empty()) else {
        return Ok(None);
    };
    let ammunition = if attack.gun_ammunition_type_id.is_empty() {
        None
    } else {
        let ammunition = items.get(&attack.gun_ammunition_type_id).ok_or_else(|| {
            format!(
                "monster gun actor references unknown ammunition item {}",
                attack.gun_ammunition_type_id
            )
        })?;
        let compatible = ammunition.subtypes.contains("AMMO")
            && ammunition.ammo_types.len() == 1
            && ammunition
                .ammo_types
                .iter()
                .next()
                .is_some_and(|ammunition_type| gun.ammo.contains(ammunition_type))
            && starting_ammunition
                .get(&attack.gun_ammunition_type_id)
                .is_some_and(|amount| *amount > 0)
            && !ammunition
                .unsupported_fields
                .iter()
                .any(|field| field == "damage" || field.starts_with("damage."));
        if !compatible {
            return Ok(None);
        }
        Some(ammunition)
    };
    let projectile_effects = gun
        .ammunition_effects
        .iter()
        .chain(
            ammunition
                .into_iter()
                .flat_map(|ammunition| ammunition.ammunition_effects.iter()),
        )
        .cloned()
        .collect::<BTreeSet<_>>();
    let Some(projectile_effect_profiles) = runtime_monster_projectile_effects(
        &projectile_effects,
        ammunition_effects,
        effect_types,
        fields,
    ) else {
        return Ok(None);
    };
    let has_fixed_damage = projectile_effects.contains("NO_DAMAGE_SCALING");
    let stops_at_obstacles = projectile_effects.contains("NO_PENETRATE_OBSTACLES");
    let ammunition_shape_is_supported = ammunition.is_none() == gun.ammo.is_empty();
    let combined_range = gun
        .range
        .checked_add(ammunition.map_or(0, |ammunition| ammunition.range))
        .ok_or("monster gun range overflow")?;
    let combined_dispersion = gun
        .dispersion
        .checked_add(ammunition.map_or(0, |ammunition| ammunition.dispersion))
        .ok_or("monster gun dispersion overflow")?;
    let gun_is_supported = gun.subtypes.contains("GUN")
        && gun.flags.contains("NEVER_JAMS")
        && ammunition_shape_is_supported
        && !attack.gun_require_sunlight
        && !attack.gun_require_targeting_npc
        && !attack.gun_require_targeting_monster
        && attack.gun_targeting_sound.len() <= 512
        && !attack.gun_targeting_sound.chars().any(char::is_control)
        && !attack.gun_fake_skills.contains_key("throw")
        && !gun.gun_skill.is_empty()
        && combined_range > 0
        && combined_dispersion >= 0
        && !gun
            .unsupported_fields
            .iter()
            .any(|field| field.starts_with("ranged_damage."))
        && !gun.unsupported_fields.contains("modes")
        && !gun.unsupported_fields.contains("energy_drain")
        && stops_at_obstacles;
    if !gun_is_supported {
        return Ok(None);
    }
    let gun_skill = attack
        .gun_fake_skills
        .get("gun")
        .copied()
        .zip(attack.gun_fake_skills.get(&gun.gun_skill).copied());
    let Some((generic_skill, weapon_skill)) = gun_skill else {
        return Ok(None);
    };
    let item_range = u32::try_from(combined_range)?;
    if attack.range > item_range
        || item_range > 1_000
        || !runtime_monster_gun_work_is_bounded(&ranges, item_range, &projectile_effect_profiles)
    {
        return Ok(None);
    }
    let raw_dispersion = u32::try_from(combined_dispersion)?;
    let weapon_dispersion = raw_dispersion
        .saturating_add(9)
        .checked_div(18)
        .unwrap_or(0)
        .max(1);
    let dexterity_penalty = 20_u32.saturating_sub(u32::from(attack.gun_fake_dexterity)) / 2;
    let skill_twice = u32::from(generic_skill)
        .saturating_add(u32::from(weapon_skill))
        .min(20);
    let shortfall_twice = 20 - skill_twice;
    let base_skill_penalty = shortfall_twice.saturating_mul(5);
    let (skill_numerator, skill_denominator) = if skill_twice >= 10 {
        (25_u32.saturating_mul(shortfall_twice), 12_u32)
    } else {
        (
            25_u32.saturating_mul(45_u32.saturating_sub(skill_twice.saturating_mul(4))),
            6_u32,
        )
    };
    let scaled_skill_penalty = skill_numerator
        .saturating_add(skill_denominator / 2)
        .checked_div(skill_denominator)
        .unwrap_or(0);
    let dispersion = weapon_dispersion
        .checked_add(dexterity_penalty)
        .and_then(|value| value.checked_add(base_skill_penalty))
        .and_then(|value| value.checked_add(scaled_skill_penalty))
        .ok_or("monster gun dispersion overflow")?;
    let mut combined_damage = gun
        .ranged_damage
        .iter()
        .map(|(damage_type_id, unit)| (damage_type_id.clone(), unit.clone()))
        .collect::<BTreeMap<_, _>>();
    if let Some(ammunition) = ammunition {
        for (damage_type_id, ammunition_unit) in &ammunition.damage {
            let unit = combined_damage.entry(damage_type_id.clone()).or_default();
            unit.amount += ammunition_unit.amount;
            unit.armor_penetration += ammunition_unit.armor_penetration;
        }
    }
    let damage = combined_damage
        .iter()
        .map(|(damage_type_id, unit)| {
            if !unit.amount.is_finite()
                || !unit.armor_penetration.is_finite()
                || unit.amount < 0.0
                || unit.armor_penetration < 0.0
                || unit.amount > f64::from(i32::MAX) / 1_000.0
                || unit.armor_penetration > f64::from(i32::MAX) / 1_000.0
            {
                return Err("monster gun damage is outside fixed-point bounds".into());
            }
            Ok(WorldgenMonsterMeleeDamageUnitV1 {
                damage_type_id: damage_type_id.clone(),
                amount_milli: (unit.amount * 1_000.0).round() as i32,
                armor_penetration_milli: (unit.armor_penetration * 1_000.0).round() as i32,
                armor_multiplier_millionths: 1_000_000,
                damage_multiplier_millionths: 1_000_000,
                constant_armor_multiplier_millionths: 1_000_000,
                constant_damage_multiplier_millionths: 1_000_000,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    if damage.iter().all(|unit| unit.amount_milli == 0) {
        if projectile_effect_profiles.iter().all(|effect| {
            effect.area_fields.is_empty()
                && effect.trail_fields.is_empty()
                && effect.on_hit_effects.is_empty()
        }) {
            return Ok(None);
        }
    } else if !has_fixed_damage {
        return Ok(None);
    }
    Ok(Some(RuntimeMonsterGunProfile {
        damage,
        ammunition_type_id: attack.gun_ammunition_type_id.clone(),
        ranges,
        item_range,
        dispersion,
        sound_volume: monster_firearm_sound_volume(gun, ammunition)?,
        targeting_cost_moves: attack.gun_targeting_cost_moves,
        require_targeting_player: attack.gun_require_targeting_player,
        targeting_timeout_turns: attack.gun_targeting_timeout_turns,
        targeting_timeout_extend_turns: attack.gun_targeting_timeout_extend_turns,
        targeting_sound: attack.gun_targeting_sound.clone(),
        targeting_volume: u16::try_from(attack.gun_targeting_volume)?,
        laser_lock: attack.gun_laser_lock,
        projectile_effects: projectile_effect_profiles,
        no_damage_scaling: has_fixed_damage,
        blinds_eyes: projectile_effects.contains("BLINDS_EYES"),
        target_moving_vehicles: attack.gun_target_moving_vehicles,
    }))
}

fn runtime_monster_gun_work_is_bounded(
    ranges: &[WorldgenMonsterGunRangeV1],
    item_range: u32,
    effects: &[WorldgenMonsterProjectileEffectV1],
) -> bool {
    let Some(maximum_shots) = ranges.iter().map(|range| u64::from(range.shot_count)).max() else {
        return false;
    };
    let Some(per_shot) = effects.iter().try_fold(1_u64, |total, effect| {
        let trails = u64::try_from(effect.trail_fields.len())
            .ok()?
            .checked_mul(u64::from(item_range))?;
        let area = effect.area_fields.iter().try_fold(0_u64, |area, field| {
            let diameter = u64::from(field.radius).checked_mul(2)?.checked_add(1)?;
            area.checked_add(diameter.checked_mul(diameter)?)
        })?;
        total
            .checked_add(trails)?
            .checked_add(area)?
            .checked_add(u64::try_from(effect.on_hit_effects.len()).ok()?)
    }) else {
        return false;
    };
    per_shot
        .checked_mul(maximum_shots)
        .is_some_and(|work| work <= 100_000)
}

fn runtime_monster_projectile_effects(
    effect_ids: &BTreeSet<String>,
    registry: &AmmunitionEffectRegistry,
    effect_types: &EffectTypeRegistry,
    fields: &FieldTypeRegistry,
) -> Option<Vec<WorldgenMonsterProjectileEffectV1>> {
    const SUPPORTED_HARDCODED: &[&str] =
        &["BLINDS_EYES", "NO_DAMAGE_SCALING", "NO_PENETRATE_OBSTACLES"];
    let mut profiles = Vec::new();
    for effect_id in effect_ids {
        if SUPPORTED_HARDCODED.contains(&effect_id.as_str()) {
            continue;
        }
        let definition = registry.get(effect_id)?;
        if !definition.unsupported_fields.is_empty()
            || definition.area_fields.len() > 64
            || definition.trail_fields.len() > 64
            || definition.on_hit_effects.len() > 64
            || definition.on_hit_effects.iter().any(|effect| {
                effect.need_touch_skin
                    || !(1..=1_000_000_000).contains(&effect.duration_seconds)
                    || !(1..=1_000_000).contains(&effect.intensity)
            })
            || definition
                .area_fields
                .iter()
                .any(|field| field.radius_z > 0)
            || (definition.area_fields.is_empty()
                && definition.trail_fields.is_empty()
                && definition.on_hit_effects.is_empty())
        {
            return None;
        }
        for field in definition
            .area_fields
            .iter()
            .chain(&definition.trail_fields)
        {
            let field_type = fields.get(&field.field_type_id)?;
            if field_type.intensity_levels.is_empty()
                || field_type.intensity_levels.len() > 16
                || field.intensity_minimum == 0
                || field.intensity_minimum > field.intensity_maximum
                || (field.intensity_minimum..=field.intensity_maximum)
                    .any(|intensity| !field_type.contact_effects_supported_at(intensity))
            {
                return None;
            }
        }
        let field = |field: &cdda_content::AmmunitionFieldEffectDefinition, radius| {
            WorldgenMonsterProjectileFieldEffectV1 {
                field_type_id: field.field_type_id.clone(),
                intensity_minimum: field.intensity_minimum,
                intensity_maximum: field.intensity_maximum,
                chance_percent: field.chance_percent,
                radius,
                check_passable: field.check_passable,
            }
        };
        let on_hit_effects = definition
            .on_hit_effects
            .iter()
            .map(|effect| {
                let resolved =
                    effect_types.resolve_application(&effect.effect_id, effect.intensity)?;
                let maximum_accumulated_duration_seconds =
                    u32::try_from(resolved.maximum_duration_seconds).ok()?;
                let modifiers = ActorEffectModifiersV1 {
                    strength: resolved.strength_modifier,
                    dexterity: resolved.dexterity_modifier,
                    intelligence: resolved.intelligence_modifier,
                    perception: resolved.perception_modifier,
                    speed: resolved.speed_modifier,
                    limb_scores: resolved
                        .limb_score_multipliers
                        .into_iter()
                        .map(
                            |(score_id, multiplier_millionths)| ActorEffectLimbScoreModifierV1 {
                                score_id,
                                multiplier_millionths,
                            },
                        )
                        .collect(),
                };
                (maximum_accumulated_duration_seconds > 0
                    && resolved.duration_add_percent <= 1_000
                    && resolved.blocked_by_effect_ids.len() <= 64
                    && resolved
                        .blocked_by_effect_ids
                        .windows(2)
                        .all(|pair| pair[0] < pair[1])
                    && resolved.blocked_by_effect_ids.iter().all(|id| {
                        !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
                    })
                    && cdda_protocol::actor_effect_modifiers_are_valid(&modifiers))
                .then_some(WorldgenMonsterProjectileOnHitEffectV1 {
                    effect_id: effect.effect_id.clone(),
                    duration_seconds: effect.duration_seconds,
                    intensity: resolved.intensity,
                    maximum_accumulated_duration_seconds,
                    duration_add_percent: resolved.duration_add_percent,
                    blocked_by_effect_ids: resolved.blocked_by_effect_ids,
                    modifiers,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        profiles.push(WorldgenMonsterProjectileEffectV1 {
            effect_id: effect_id.clone(),
            trigger_chance_percent: definition.trigger_chance_percent,
            area_fields: definition
                .area_fields
                .iter()
                .map(|definition| field(definition, definition.radius))
                .collect(),
            trail_fields: definition
                .trail_fields
                .iter()
                .map(|definition| field(definition, 0))
                .collect(),
            on_hit_effects,
        });
        if profiles.len() > 64 {
            return None;
        }
    }
    Some(profiles)
}

fn monster_firearm_sound_volume(
    gun: &cdda_content::ItemDefinition,
    ammunition: Option<&cdda_content::ItemDefinition>,
) -> Result<u16, Box<dyn std::error::Error>> {
    let ammunition_loudness = if let Some(ammunition) = ammunition {
        if let Some(loudness) = ammunition.loudness {
            loudness
        } else {
            let damage_loudness = ammunition.damage.values().try_fold(
                0.0,
                |total, damage| -> Result<_, Box<dyn std::error::Error>> {
                    let total = total + (damage.amount + damage.armor_penetration) * 2.0;
                    total
                        .is_finite()
                        .then_some(total)
                        .ok_or_else(|| "monster ammunition loudness is not finite".into())
                },
            )?;
            let derived = f64::from(
                ammunition
                    .range
                    .checked_mul(2)
                    .ok_or("monster ammunition loudness range overflow")?,
            ) + damage_loudness;
            if !derived.is_finite() || derived < 0.0 || derived > f64::from(i32::MAX) {
                return Err("derived monster ammunition loudness is out of range".into());
            }
            derived.trunc() as i32
        }
    } else {
        0
    };
    let volume = gun
        .loudness
        .unwrap_or(0)
        .checked_add(ammunition_loudness)
        .ok_or("monster firearm loudness overflow")?
        .max(0);
    Ok(u16::try_from(volume)?)
}

fn rounded_millionths_product(level: u32, increment_millionths: i64) -> Option<i64> {
    let product = i128::from(level).checked_mul(i128::from(increment_millionths))?;
    let quotient = product / 1_000_000;
    let remainder = product % 1_000_000;
    let rounded = if remainder.unsigned_abs() >= 500_000 {
        quotient.checked_add(if product.is_negative() { -1 } else { 1 })?
    } else {
        quotient
    };
    i64::try_from(rounded).ok()
}

fn finalized_spell_stat(
    minimum: i32,
    maximum: i32,
    increment_millionths: i64,
    level: u32,
) -> Option<i32> {
    let value =
        i64::from(minimum).checked_add(rounded_millionths_product(level, increment_millionths)?)?;
    let clamped = if minimum <= maximum {
        value.min(i64::from(maximum))
    } else {
        value.max(i64::from(maximum))
    };
    i32::try_from(clamped).ok()
}

fn runtime_monster_spell_profile(
    attack: &cdda_content::MonsterSpecialAttackDefinition,
    spells: &SpellRegistry,
    fields: &FieldTypeRegistry,
    creature_eoc_ids: &BTreeSet<String>,
) -> Result<Option<RuntimeMonsterSpellProfile>, Box<dyn std::error::Error>> {
    if attack.kind != MonsterSpecialAttackKind::Spell {
        return Ok(None);
    }
    let spell = spells.get(&attack.spell_id).ok_or_else(|| {
        format!(
            "monster spell actor references unknown SPELL {}",
            attack.spell_id
        )
    })?;
    let summon = spell.supports_hostile_permanent_summoning();
    let typed_damage = false;
    let status_effect = spell.supports_hostile_status_effect();
    let effect_on_condition = spell.supports_hostile_effect_on_condition()
        && creature_eoc_ids.contains(&spell.effect_str)
        && !spell.valid_targets.contains("ground");
    if (!summon && !typed_damage && !status_effect && !effect_on_condition)
        || !spell.flags.contains("SILENT")
        || !spell.flags.contains("NO_EXPLOSION_SFX")
        || spell.flags.contains("RANDOM_DAMAGE")
        || spell.flags.contains("RANDOM_DURATION")
        || attack.spell_min_level > 1_000
        || attack
            .spell_max_level
            .is_some_and(|maximum| maximum > 1_000)
        || attack
            .spell_max_level
            .is_some_and(|maximum| maximum != attack.spell_min_level)
        || attack.spell_min_level > u32::try_from(spell.maximum_level.max(0))?
    {
        return Ok(None);
    }
    let target_self = attack.spell_hit_self || attack.spell_allow_no_target;
    if (target_self && !spell.valid_targets.contains("self"))
        || (!target_self
            && !spell.valid_targets.contains("hostile")
            && !spell.valid_targets.contains("ground"))
        || (target_self
            && attack.condition.as_ref().is_some_and(|condition| {
                cdda_protocol::eoc_condition_requires_target_context(
                    &crate::eocs::runtime_condition(condition),
                )
            }))
    {
        return Ok(None);
    }
    let level = attack.spell_min_level;
    let leveled_damage = finalized_spell_stat(
        spell.minimum_damage,
        spell.maximum_damage,
        spell.damage_increment_millionths,
        level,
    );
    let range = finalized_spell_stat(
        spell.minimum_range,
        spell.maximum_range,
        spell.range_increment_millionths,
        level,
    );
    let aoe = finalized_spell_stat(
        spell.minimum_aoe,
        spell.maximum_aoe,
        spell.aoe_increment_millionths,
        level,
    );
    let casting_time = finalized_spell_stat(
        spell.base_casting_time_moves,
        spell.final_casting_time_moves,
        spell.casting_time_increment_millionths,
        level,
    );
    let leveled_duration = finalized_spell_stat(
        spell.minimum_duration_moves,
        spell.maximum_duration_moves,
        spell.duration_increment_millionths,
        level,
    );
    let field_intensity = finalized_spell_stat(
        spell.minimum_field_intensity,
        spell.maximum_field_intensity,
        spell.field_intensity_increment_millionths,
        level,
    );
    let Some((leveled_damage, range, aoe, casting_time, leveled_duration, field_intensity)) =
        leveled_damage
            .zip(range)
            .zip(aoe)
            .zip(casting_time)
            .zip(leveled_duration)
            .zip(field_intensity)
            .map(
                |(((((damage, range), aoe), casting_time), duration), field_intensity)| {
                    (damage, range, aoe, casting_time, duration, field_intensity)
                },
            )
    else {
        return Ok(None);
    };
    let random_damage = spell.flags.contains("RANDOM_DAMAGE");
    let (minimum_damage, maximum_damage) = if random_damage {
        (
            leveled_damage.min(spell.maximum_damage),
            leveled_damage.max(spell.maximum_damage),
        )
    } else {
        (leveled_damage, leveled_damage)
    };
    let amount_is_bounded = if summon {
        (1..=64).contains(&minimum_damage) && (minimum_damage..=64).contains(&maximum_damage)
    } else if typed_damage {
        (1..=1_000).contains(&minimum_damage) && (minimum_damage..=1_000).contains(&maximum_damage)
    } else {
        minimum_damage == 0 && maximum_damage == 0
    };
    let random_duration = spell.flags.contains("RANDOM_DURATION");
    let (minimum_duration_moves, maximum_duration_moves) = if random_duration {
        (
            leveled_duration.min(spell.maximum_duration_moves),
            leveled_duration.max(spell.maximum_duration_moves),
        )
    } else {
        (leveled_duration, leveled_duration)
    };
    let duration_is_bounded = if status_effect {
        (1..=1_000_000_000).contains(&minimum_duration_moves)
            && (minimum_duration_moves..=1_000_000_000).contains(&maximum_duration_moves)
            && minimum_duration_moves % 100 == 0
            && maximum_duration_moves % 100 == 0
            && (!random_duration || minimum_duration_moves == maximum_duration_moves)
    } else if spell.field_type_id.is_empty() {
        minimum_duration_moves == 0 && maximum_duration_moves == 0
    } else {
        (0..=1_000_000_000).contains(&minimum_duration_moves)
            && (minimum_duration_moves..=1_000_000_000).contains(&maximum_duration_moves)
            && minimum_duration_moves == maximum_duration_moves
    };
    let field_profile = if spell.field_type_id.is_empty() {
        None
    } else {
        let field_intensity = u8::try_from(field_intensity).ok();
        let variance = u32::try_from(spell.field_intensity_variance_millionths).ok();
        field_intensity
            .zip(variance)
            .and_then(|(field_intensity, variance)| {
                let definition = fields.get(&spell.field_type_id)?;
                let maximum_registered_intensity =
                    u16::try_from(definition.intensity_levels.len()).ok()?;
                let variance_delta = u32::from(field_intensity)
                    .checked_mul(variance)?
                    .checked_div(1_000_000)?;
                let maximum_rolled_intensity =
                    u16::from(field_intensity).checked_add(u16::try_from(variance_delta).ok()?)?;
                let maximum_effective_intensity =
                    u8::try_from(maximum_rolled_intensity.min(maximum_registered_intensity))
                        .ok()?;
                (definition.unsupported_fields.is_empty()
                    && definition.contact_effects_supported_at(maximum_effective_intensity))
                .then_some((field_intensity, variance))
            })
    };
    if !spell.field_type_id.is_empty() && field_profile.is_none() {
        return Ok(None);
    }
    if !amount_is_bounded
        || !duration_is_bounded
        || !(0..=1_000).contains(&range)
        || !(0..=32).contains(&aoe)
        || !(0..=1_000_000_000).contains(&casting_time)
        || (target_self && range != 0)
        || (!target_self && range == 0)
        || (summon && aoe == 0)
        || ((typed_damage || status_effect || effect_on_condition) && target_self)
    {
        return Ok(None);
    }
    Ok(Some(RuntimeMonsterSpellProfile {
        summoned_monster_type_id: summon.then(|| spell.effect_str.clone()).unwrap_or_default(),
        damage_type_id: typed_damage
            .then(|| spell.damage_type_id.clone())
            .unwrap_or_default(),
        minimum_damage: typed_damage
            .then(|| u16::try_from(minimum_damage))
            .transpose()?
            .unwrap_or(0),
        maximum_damage: typed_damage
            .then(|| u16::try_from(maximum_damage))
            .transpose()?
            .unwrap_or(0),
        status_effect_id: status_effect
            .then(|| spell.effect_str.clone())
            .unwrap_or_default(),
        minimum_duration_turns: status_effect
            .then(|| u32::try_from(minimum_duration_moves).map(|moves| moves.div_ceil(100)))
            .transpose()?
            .unwrap_or(0),
        maximum_duration_turns: status_effect
            .then(|| u32::try_from(maximum_duration_moves).map(|moves| moves.div_ceil(100)))
            .transpose()?
            .unwrap_or(0),
        eoc_id: effect_on_condition
            .then(|| spell.effect_str.clone())
            .unwrap_or_default(),
        target_self,
        no_projectile: spell.flags.contains("NO_PROJECTILE"),
        minimum_summons: summon
            .then(|| u16::try_from(minimum_damage))
            .transpose()?
            .unwrap_or(0),
        maximum_summons: summon
            .then(|| u16::try_from(maximum_damage))
            .transpose()?
            .unwrap_or(0),
        random_summons: summon && random_damage,
        range: u32::try_from(range)?,
        aoe: u8::try_from(aoe)?,
        casting_time_moves: u32::try_from(casting_time)?,
        field_type_id: spell.field_type_id.clone(),
        field_chance: field_profile.map_or(0, |_| spell.field_chance),
        field_intensity: field_profile.map_or(0, |profile| profile.0),
        field_intensity_variance_millionths: field_profile.map_or(0, |profile| profile.1),
        field_duration_turns: field_profile
            .map(|_| u32::try_from(leveled_duration).map(|moves| moves / 100))
            .transpose()?
            .unwrap_or(0),
        targets_hostile: !target_self && spell.valid_targets.contains("hostile"),
        targets_ground: !target_self && spell.valid_targets.contains("ground"),
    }))
}

fn runtime_monster_catalog(
    roots: BTreeSet<String>,
    mut monster_ids: BTreeSet<String>,
    groups: &MonsterGroupRegistry,
    monsters: &MonsterRegistry,
    items: &ItemRegistry,
    ammunition_effects: &AmmunitionEffectRegistry,
    effect_types: &EffectTypeRegistry,
    fields: &FieldTypeRegistry,
    spells: &SpellRegistry,
    creature_eoc_ids: &BTreeSet<String>,
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
    let mut monster_dependencies = monster_ids.iter().cloned().collect::<VecDeque<_>>();
    while let Some(monster_id) = monster_dependencies.pop_front() {
        let monster = monsters
            .get(&monster_id)
            .ok_or_else(|| format!("monster group references unknown MONSTER {monster_id}"))?;
        for attack in monster
            .special_attacks
            .values()
            .filter(|attack| attack.is_fully_supported())
        {
            let target_id = match attack.kind {
                MonsterSpecialAttackKind::Polymorph => {
                    Some(attack.polymorph_monster_type_id.clone())
                }
                MonsterSpecialAttackKind::Spell => {
                    runtime_monster_spell_profile(attack, spells, fields, creature_eoc_ids)?
                        .and_then(|profile| {
                            (!profile.summoned_monster_type_id.is_empty())
                                .then_some(profile.summoned_monster_type_id)
                        })
                }
                MonsterSpecialAttackKind::Melee
                | MonsterSpecialAttackKind::Bite
                | MonsterSpecialAttackKind::Leap
                | MonsterSpecialAttackKind::Eoc
                | MonsterSpecialAttackKind::Gun
                | MonsterSpecialAttackKind::Unsupported => None,
            };
            let Some(target_id) = target_id else {
                continue;
            };
            if monsters.get(&target_id).is_none() {
                return Err(format!(
                    "monster {} special actor references unknown MONSTER {target_id}",
                    monster.id
                )
                .into());
            }
            if monster_ids.insert(target_id.clone()) {
                monster_dependencies.push_back(target_id);
            }
        }
    }
    let monster_indices = monster_ids
        .iter()
        .enumerate()
        .map(|(index, id)| Ok((id.clone(), u16::try_from(index)?)))
        .collect::<Result<BTreeMap<_, _>, Box<dyn std::error::Error>>>()?;
    let mut monster_prototypes = monster_ids
        .iter()
        .map(|id| {
            let monster = monsters
                .get(id)
                .ok_or_else(|| format!("monster group references unknown MONSTER {id}"))?;
            for ammunition_type_id in monster.starting_ammunition.keys() {
                if items.get(ammunition_type_id).is_none() {
                    return Err(format!(
                        "monster {} starting_ammo references unknown item {ammunition_type_id}",
                        monster.id
                    )
                    .into());
                }
            }
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
            let melee_damage_supported = monster.melee_damage_is_fully_supported()
                && monster
                    .melee_damage
                    .iter()
                    .all(|unit| unit.damage_multiplier_millionths > 0);
            let mut deferred_behavior_fields = monster
                .unsupported_fields
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            deferred_behavior_fields.extend(
                monster
                    .special_attacks
                    .values()
                    .filter(|attack| !attack.is_fully_supported())
                    .map(|attack| format!("special_attacks.{}", attack.id)),
            );
            if !melee_damage_supported {
                deferred_behavior_fields.insert(String::from("melee_damage"));
            }
            let attack_effects_supported = monster.attack_effects_are_fully_supported();
            if !attack_effects_supported {
                deferred_behavior_fields.insert(String::from("attack_effs"));
            }
            let mut attack_effect_definitions = attack_effects_supported
                .then(|| monster.attack_effects.clone())
                .unwrap_or_default();
            for (flag, effect_id, duration_turns) in [
                ("VENOM", "poison", 180),
                ("BADVENOM", "badpoison", 240),
                ("PARALYZEVENOM", "paralyzepoison", 600),
            ] {
                if monster.flags.contains(flag) {
                    attack_effect_definitions.push(MonsterAttackEffectDefinition {
                        effect_id: effect_id.to_owned(),
                        chance_millionths: 1_000_000,
                        permanent: false,
                        affect_hit_body_part: false,
                        body_part_id: None,
                        duration_turns: (duration_turns, duration_turns),
                        intensity: (1, 1),
                    });
                }
            }
            let attack_effects = attack_effects_supported
                .then(|| {
                    attack_effect_definitions
                        .iter()
                        .enumerate()
                        .map(|(index, effect)| {
                            runtime_monster_attack_effect(
                                effect,
                                effect_types,
                                index >= monster.attack_effects.len(),
                            )
                        })
                        .collect::<Option<Vec<_>>>()
                })
                .flatten();
            if attack_effects.is_none() {
                deferred_behavior_fields.insert(String::from("attack_effs.effect_type"));
            }
            let attack_effects = attack_effects.unwrap_or_default();
            let creature_eoc_context_is_supported =
                |attack: &cdda_content::MonsterSpecialAttackDefinition| {
                    attack
                        .eoc_ids
                        .iter()
                        .all(|eoc_id| creature_eoc_ids.contains(eoc_id))
                        && attack.condition.as_ref().is_none_or(|condition| {
                            cdda_protocol::creature_eoc_condition_is_supported(
                                &crate::eocs::runtime_condition(condition),
                            )
                        })
                };
            for attack in monster.special_attacks.values().filter(|attack| {
                attack.is_fully_supported() && !creature_eoc_context_is_supported(attack)
            }) {
                deferred_behavior_fields.insert(format!(
                    "special_attacks.{}.creature_eoc_context",
                    attack.id
                ));
            }
            let mut gun_profiles = BTreeMap::new();
            for attack in monster.special_attacks.values().filter(|attack| {
                attack.is_fully_supported()
                    && creature_eoc_context_is_supported(attack)
                    && attack.kind == MonsterSpecialAttackKind::Gun
            }) {
                if let Some(profile) = runtime_monster_gun_profile(
                    attack,
                    items,
                    &monster.starting_ammunition,
                    ammunition_effects,
                    effect_types,
                    fields,
                )? {
                    gun_profiles.insert(attack.id.clone(), profile);
                } else {
                    deferred_behavior_fields
                        .insert(format!("special_attacks.{}.gun_runtime_context", attack.id));
                }
            }
            let mut spell_profiles = BTreeMap::new();
            for attack in monster.special_attacks.values().filter(|attack| {
                attack.is_fully_supported()
                    && creature_eoc_context_is_supported(attack)
                    && attack.kind == MonsterSpecialAttackKind::Spell
            }) {
                if let Some(profile) =
                    runtime_monster_spell_profile(attack, spells, fields, creature_eoc_ids)?
                {
                    spell_profiles.insert(attack.id.clone(), profile);
                } else {
                    deferred_behavior_fields.insert(format!(
                        "special_attacks.{}.spell_runtime_context",
                        attack.id
                    ));
                }
            }
            let mut special_effect_profiles = BTreeMap::new();
            for attack in monster.special_attacks.values().filter(|attack| {
                attack.is_fully_supported() && creature_eoc_context_is_supported(attack)
            }) {
                let spell_status = spell_profiles.get(&attack.id).and_then(|profile| {
                    (!profile.status_effect_id.is_empty()).then(|| MonsterAttackEffectDefinition {
                        effect_id: profile.status_effect_id.clone(),
                        chance_millionths: 1_000_000,
                        permanent: false,
                        affect_hit_body_part: false,
                        body_part_id: None,
                        duration_turns: (
                            profile.minimum_duration_turns,
                            profile.maximum_duration_turns,
                        ),
                        intensity: (1, 1),
                    })
                });
                let compiled = spell_status.as_ref().map_or_else(
                    || {
                        attack
                            .effects
                            .iter()
                            .map(|effect| {
                                runtime_monster_attack_effect(effect, effect_types, false)
                            })
                            .collect::<Option<Vec<_>>>()
                    },
                    |effect| {
                        runtime_monster_attack_effect(effect, effect_types, false)
                            .map(|effect| vec![effect])
                    },
                );
                if let Some(compiled) = compiled {
                    special_effect_profiles.insert(attack.id.clone(), compiled);
                } else {
                    deferred_behavior_fields
                        .insert(format!("special_attacks.{}.effect_type", attack.id));
                }
            }
            let special_attacks = monster
                .special_attacks
                .values()
                .filter(|attack| {
                    attack.is_fully_supported()
                        && creature_eoc_context_is_supported(attack)
                        && (attack.kind != MonsterSpecialAttackKind::Gun
                            || gun_profiles.contains_key(&attack.id))
                        && (attack.kind != MonsterSpecialAttackKind::Spell
                            || spell_profiles.contains_key(&attack.id))
                        && special_effect_profiles.contains_key(&attack.id)
                })
                .map(|attack| {
                    let kind = match attack.kind {
                        MonsterSpecialAttackKind::Melee => {
                            WorldgenMonsterSpecialAttackKindV1::Melee
                        }
                        MonsterSpecialAttackKind::Bite => WorldgenMonsterSpecialAttackKindV1::Bite,
                        MonsterSpecialAttackKind::Leap => WorldgenMonsterSpecialAttackKindV1::Leap,
                        MonsterSpecialAttackKind::Eoc => WorldgenMonsterSpecialAttackKindV1::Eoc,
                        MonsterSpecialAttackKind::Gun => WorldgenMonsterSpecialAttackKindV1::Gun,
                        MonsterSpecialAttackKind::Polymorph => {
                            WorldgenMonsterSpecialAttackKindV1::Polymorph
                        }
                        MonsterSpecialAttackKind::Spell => {
                            WorldgenMonsterSpecialAttackKindV1::Spell
                        }
                        MonsterSpecialAttackKind::Unsupported => {
                            return Err(
                                "unsupported special attack reached runtime admission".into()
                            );
                        }
                    };
                    let gun_profile = gun_profiles.get(&attack.id);
                    let spell_profile = spell_profiles.get(&attack.id);
                    Ok(WorldgenMonsterSpecialAttackV1 {
                        attack_id: attack.id.clone(),
                        kind,
                        cooldown_turns: attack.cooldown_turns,
                        move_cost_moves: spell_profile
                            .map_or(attack.move_cost_moves, |profile| profile.casting_time_moves),
                        accuracy: attack.accuracy,
                        range: spell_profile.map_or(attack.range, |profile| profile.range),
                        no_adjacent: attack.no_adjacent,
                        dodgeable: attack.dodgeable,
                        minimum_damage_multiplier_millionths: spell_profile
                            .map_or(attack.minimum_damage_multiplier_millionths, |profile| {
                                i32::from(profile.minimum_damage) * 1_000_000
                            }),
                        maximum_damage_multiplier_millionths: spell_profile
                            .map_or(attack.maximum_damage_multiplier_millionths, |profile| {
                                i32::from(profile.maximum_damage) * 1_000_000
                            }),
                        damage: spell_profile.map_or_else(
                            || {
                                gun_profile.map_or_else(
                                    || {
                                        attack
                                            .damage
                                            .iter()
                                            .map(|unit| WorldgenMonsterMeleeDamageUnitV1 {
                                                damage_type_id: unit.damage_type_id.clone(),
                                                amount_milli: unit.amount_milli,
                                                armor_penetration_milli: unit
                                                    .armor_penetration_milli,
                                                armor_multiplier_millionths: unit
                                                    .armor_multiplier_millionths,
                                                damage_multiplier_millionths: unit
                                                    .damage_multiplier_millionths,
                                                constant_armor_multiplier_millionths: unit
                                                    .constant_armor_multiplier_millionths,
                                                constant_damage_multiplier_millionths: unit
                                                    .constant_damage_multiplier_millionths,
                                            })
                                            .collect()
                                    },
                                    |profile| profile.damage.clone(),
                                )
                            },
                            |profile| {
                                if profile.damage_type_id.is_empty() {
                                    Vec::new()
                                } else {
                                    vec![WorldgenMonsterMeleeDamageUnitV1 {
                                        damage_type_id: profile.damage_type_id.clone(),
                                        amount_milli: 1_000,
                                        armor_penetration_milli: 0,
                                        armor_multiplier_millionths: 1_000_000,
                                        damage_multiplier_millionths: 1_000_000,
                                        constant_armor_multiplier_millionths: 1_000_000,
                                        constant_damage_multiplier_millionths: 1_000_000,
                                    }]
                                }
                            },
                        ),
                        effects: special_effect_profiles
                            .get(&attack.id)
                            .cloned()
                            .unwrap_or_default(),
                        effects_require_damage: attack.effects_require_damage,
                        attack_amount_minimum: attack.attack_amount_minimum,
                        attack_amount_maximum: attack.attack_amount_maximum,
                        spread_damage: attack.spread_damage,
                        infection_chance_millionths: attack.infection_chance_millionths,
                        leap_minimum_range_milli: attack.leap_minimum_range_milli,
                        leap_maximum_range_milli: attack.leap_maximum_range_milli,
                        leap_minimum_consider_range_milli: attack.leap_minimum_consider_range_milli,
                        leap_maximum_consider_range_milli: attack.leap_maximum_consider_range_milli,
                        leap_allow_no_target: attack.leap_allow_no_target,
                        leap_prefer: attack.leap_prefer,
                        leap_random: attack.leap_random,
                        leap_ignore_destination_danger: attack.leap_ignore_destination_danger,
                        condition: attack
                            .condition
                            .as_ref()
                            .map(crate::eocs::runtime_condition),
                        eoc_ids: spell_profile.map_or_else(
                            || attack.eoc_ids.clone(),
                            |profile| {
                                (!profile.eoc_id.is_empty())
                                    .then(|| vec![profile.eoc_id.clone()])
                                    .unwrap_or_default()
                            },
                        ),
                        polymorph_monster_type_id: if attack.kind
                            == MonsterSpecialAttackKind::Polymorph
                        {
                            attack.polymorph_monster_type_id.clone()
                        } else {
                            String::new()
                        },
                        polymorph_keep_speed: attack.kind == MonsterSpecialAttackKind::Polymorph
                            && attack.polymorph_keep_speed,
                        polymorph_keep_hp: attack.kind == MonsterSpecialAttackKind::Polymorph
                            && attack.polymorph_keep_hp,
                        polymorph_keep_aggression: attack.kind
                            == MonsterSpecialAttackKind::Polymorph
                            && attack.polymorph_keep_aggression,
                        spell_summoned_monster_type_id: spell_profile
                            .map(|profile| profile.summoned_monster_type_id.clone())
                            .unwrap_or_default(),
                        spell_target_self: spell_profile.is_some_and(|profile| profile.target_self),
                        spell_no_projectile: spell_profile
                            .is_some_and(|profile| profile.no_projectile),
                        spell_minimum_summons: spell_profile
                            .map_or(0, |profile| profile.minimum_summons),
                        spell_maximum_summons: spell_profile
                            .map_or(0, |profile| profile.maximum_summons),
                        spell_random_summons: spell_profile
                            .is_some_and(|profile| profile.random_summons),
                        spell_aoe: spell_profile.map_or(0, |profile| profile.aoe),
                        spell_field_type_id: spell_profile
                            .map(|profile| profile.field_type_id.clone())
                            .unwrap_or_default(),
                        spell_field_chance: spell_profile.map_or(0, |profile| profile.field_chance),
                        spell_field_intensity: spell_profile
                            .map_or(0, |profile| profile.field_intensity),
                        spell_field_intensity_variance_millionths: spell_profile
                            .map_or(0, |profile| profile.field_intensity_variance_millionths),
                        spell_field_duration_turns: spell_profile
                            .map_or(0, |profile| profile.field_duration_turns),
                        spell_targets_hostile: spell_profile
                            .is_some_and(|profile| profile.targets_hostile),
                        spell_targets_ground: spell_profile
                            .is_some_and(|profile| profile.targets_ground),
                        gun_type_id: gun_profile
                            .map(|_| attack.gun_type_id.clone())
                            .unwrap_or_default(),
                        gun_ammunition_type_id: gun_profile
                            .map(|profile| profile.ammunition_type_id.clone())
                            .unwrap_or_default(),
                        gun_ranges: gun_profile
                            .map(|profile| profile.ranges.clone())
                            .unwrap_or_default(),
                        gun_item_range: gun_profile.map_or(0, |profile| profile.item_range),
                        gun_dispersion: gun_profile.map_or(0, |profile| profile.dispersion),
                        gun_sound_volume: gun_profile.map_or(0, |profile| profile.sound_volume),
                        gun_targeting_cost_moves: gun_profile
                            .map_or(0, |profile| profile.targeting_cost_moves),
                        gun_require_targeting_player: gun_profile
                            .is_some_and(|profile| profile.require_targeting_player),
                        gun_targeting_timeout_turns: gun_profile
                            .map_or(0, |profile| profile.targeting_timeout_turns),
                        gun_targeting_timeout_extend_turns: gun_profile
                            .map_or(0, |profile| profile.targeting_timeout_extend_turns),
                        gun_targeting_sound: gun_profile
                            .map(|profile| profile.targeting_sound.clone())
                            .unwrap_or_default(),
                        gun_targeting_volume: gun_profile
                            .map_or(0, |profile| profile.targeting_volume),
                        gun_laser_lock: gun_profile.is_some_and(|profile| profile.laser_lock),
                        gun_projectile_effects: gun_profile
                            .map(|profile| profile.projectile_effects.clone())
                            .unwrap_or_default(),
                        gun_no_damage_scaling: gun_profile
                            .is_some_and(|profile| profile.no_damage_scaling),
                        gun_blinds_eyes: gun_profile.is_some_and(|profile| profile.blinds_eyes),
                        gun_target_moving_vehicles: gun_profile
                            .is_some_and(|profile| profile.target_moving_vehicles),
                    })
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            Ok(WorldgenMonsterPrototypeV1 {
                base,
                runtime_spawnable: deferred_behavior_fields.is_empty(),
                starting_ammunition: monster.starting_ammunition.clone(),
                armor_milli: monster.finalized_armor_milli(),
                melee_dice_armor_penetration_milli: monster.melee_dice_armor_penetration_milli,
                melee_damage: melee_damage_supported
                    .then(|| {
                        monster
                            .melee_damage
                            .iter()
                            .map(|unit| WorldgenMonsterMeleeDamageUnitV1 {
                                damage_type_id: unit.damage_type_id.clone(),
                                amount_milli: unit.amount_milli,
                                armor_penetration_milli: unit.armor_penetration_milli,
                                armor_multiplier_millionths: unit.armor_multiplier_millionths,
                                damage_multiplier_millionths: unit.damage_multiplier_millionths,
                                constant_armor_multiplier_millionths: unit
                                    .constant_armor_multiplier_millionths,
                                constant_damage_multiplier_millionths: unit
                                    .constant_damage_multiplier_millionths,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                attack_effects,
                special_attacks,
                leaves_corpse: !monster.flags.contains("NO_CORPSE"),
                deferred_behavior_fields: deferred_behavior_fields.into_iter().collect(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    loop {
        let spawnability = monster_prototypes
            .iter()
            .map(|prototype| {
                (
                    prototype.base.monster_type_id.as_str(),
                    prototype.runtime_spawnable,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let blocked = monster_prototypes
            .iter()
            .enumerate()
            .filter_map(|(index, prototype)| {
                if !prototype.runtime_spawnable {
                    return None;
                }
                prototype.special_attacks.iter().find_map(|attack| {
                    let dependency = match attack.kind {
                        WorldgenMonsterSpecialAttackKindV1::Polymorph => {
                            Some(attack.polymorph_monster_type_id.as_str())
                        }
                        WorldgenMonsterSpecialAttackKindV1::Spell
                            if !attack.spell_summoned_monster_type_id.is_empty() =>
                        {
                            Some(attack.spell_summoned_monster_type_id.as_str())
                        }
                        WorldgenMonsterSpecialAttackKindV1::Melee
                        | WorldgenMonsterSpecialAttackKindV1::Bite
                        | WorldgenMonsterSpecialAttackKindV1::Leap
                        | WorldgenMonsterSpecialAttackKindV1::Eoc
                        | WorldgenMonsterSpecialAttackKindV1::Gun
                        | WorldgenMonsterSpecialAttackKindV1::Spell => None,
                    }?;
                    spawnability
                        .get(dependency)
                        .is_some_and(|spawnable| !*spawnable)
                        .then(|| {
                            (
                                index,
                                format!("special_attacks.{}.runtime_dependency", attack.attack_id),
                            )
                        })
                })
            })
            .collect::<Vec<_>>();
        if blocked.is_empty() {
            break;
        }
        for (index, field) in blocked {
            let prototype = monster_prototypes
                .get_mut(index)
                .ok_or("monster dependency propagation lost its prototype")?;
            prototype.runtime_spawnable = false;
            prototype.deferred_behavior_fields.push(field);
            prototype.deferred_behavior_fields.sort();
            prototype.deferred_behavior_fields.dedup();
        }
    }
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

fn npc_name_references(text: &str) -> Result<Vec<&str>, Box<dyn std::error::Error>> {
    let mut references = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(['<', '>']) {
        if rest.as_bytes()[start] == b'>' {
            return Err(format!("malformed NPC name snippet {text:?}").into());
        }
        let suffix = &rest[start..];
        let end = suffix
            .find('>')
            .ok_or_else(|| format!("unterminated NPC name snippet {text:?}"))?;
        references.push(&suffix[..=end]);
        rest = &suffix[end + 1..];
    }
    Ok(references)
}

fn runtime_npc_name_categories(
    snippets: &DescriptionSnippetRegistry,
) -> Result<Vec<WorldgenNpcNameCategoryV1>, Box<dyn std::error::Error>> {
    let mut pending = VecDeque::from([
        MALE_FULL_NAME_CATEGORY.to_owned(),
        FEMALE_FULL_NAME_CATEGORY.to_owned(),
    ]);
    let mut categories = BTreeMap::new();
    while let Some(category_id) = pending.pop_front() {
        if categories.contains_key(&category_id) {
            continue;
        }
        let category = snippets
            .get(&category_id)
            .ok_or_else(|| format!("missing reachable NPC name category {category_id}"))?;
        let choices = category
            .choices()
            .map(|choice| {
                for reference in npc_name_references(&choice.text)? {
                    if !categories.contains_key(reference)
                        && !pending.iter().any(|pending| pending == reference)
                    {
                        pending.push_back(reference.to_owned());
                    }
                }
                Ok(WorldgenNpcNameChoiceV1 {
                    text: choice.text.clone(),
                    weight: choice.weight,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        categories.insert(
            category_id.clone(),
            WorldgenNpcNameCategoryV1 {
                category_id,
                choices,
            },
        );
        if categories.len() > cdda_protocol::MAX_WORLDGEN_NPC_NAME_CATEGORIES {
            return Err("NPC name snippet closure exceeds its canonical category bound".into());
        }
    }
    let categories = categories.into_values().collect::<Vec<_>>();
    let choice_count = categories
        .iter()
        .try_fold(0_usize, |total, category| {
            total.checked_add(category.choices.len())
        })
        .ok_or("NPC name snippet choice count overflow")?;
    if choice_count > cdda_protocol::MAX_WORLDGEN_NPC_NAME_CHOICES {
        return Err("NPC name snippet closure exceeds its canonical choice bound".into());
    }
    Ok(categories)
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
        snippets,
        npc_templates,
        items,
        ammunition_effects,
        effect_types,
        fields,
        monsters,
        spells,
        monster_groups,
        eoc_definitions,
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
            items,
            ammunition_effects,
            effect_types,
            fields,
            spells,
            &cdda_protocol::creature_eoc_supported_ids(eoc_definitions),
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
                            npc_templates,
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
                                        npc_templates,
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
    let npc_name_categories = if npc_templates.is_empty() {
        Vec::new()
    } else {
        runtime_npc_name_categories(snippets)?
    };
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
        npc_name_categories,
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
            npc_placements: Vec::new(),
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
    npc_templates: &[NpcTemplateV1],
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
        npc_placements: definition
            .npc_placements
            .iter()
            .map(|placement| runtime_npc_placement(placement, npc_templates))
            .collect::<Result<Vec<_>, _>>()?,
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
    npc_templates: &[NpcTemplateV1],
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
        npc_placements: definition
            .npc_placements
            .iter()
            .map(|placement| runtime_npc_placement(placement, npc_templates))
            .collect::<Result<Vec<_>, _>>()?,
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

fn runtime_npc_placement(
    placement: &StrictMapgenNpcPlacement,
    templates: &[NpcTemplateV1],
) -> Result<WorldgenNpcPlacementV1, Box<dyn std::error::Error>> {
    let template = templates
        .binary_search_by(|template| template.template_id.as_str().cmp(&placement.template_id))
        .ok()
        .and_then(|index| templates.get(index))
        .ok_or_else(|| {
            format!(
                "mapgen NPC placement references unsupported template {}",
                placement.template_id
            )
        })?;
    let generated_name_category_ids =
        runtime_npc_generated_name_categories(template).ok_or_else(|| {
            format!(
                "NPC template {} has unsupported gender",
                template.template_id
            )
        })?;
    Ok(WorldgenNpcPlacementV1 {
        template_id: placement.template_id.clone(),
        generated_name_category_ids,
        repeat: WorldgenU16RangeV1 {
            minimum: placement.repeat.minimum,
            maximum: placement.repeat.maximum,
        },
        x: runtime_coordinate_range(placement.x),
        y: runtime_coordinate_range(placement.y),
    })
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
        AmmunitionEffectRegistry, CitySettingsRegistry, ContentManifest, DEFAULT_CITY_SETTINGS_ID,
        DEFAULT_RIVER_SETTINGS_ID, DefaultRegionTerrainFurnitureRegistry,
        DescriptionSnippetRegistry, FieldTypeRegistry, FurnitureRegistry, ItemGroupRegistry,
        ItemRegistry, MapgenRegistry, ModCatalog, MonsterGroupRegistry, MonsterRegistry,
        OvermapTerrainRegistry, RiverSettingsRegistry, SpellRegistry, StartLocationRegistry,
        TerrainRegistry,
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
        let items = ItemRegistry::load_selected(&manifest, root, &mods, &enabled).expect("items");
        let monsters =
            MonsterRegistry::load_selected(&manifest, root, &mods, &enabled).expect("monsters");
        let spells =
            SpellRegistry::load_selected(&manifest, root, &mods, &enabled).expect("spells");
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
                snippets: &DescriptionSnippetRegistry::default(),
                npc_templates: &[],
                items: &items,
                ammunition_effects: &AmmunitionEffectRegistry::default(),
                effect_types: &EffectTypeRegistry::default(),
                fields: &FieldTypeRegistry::default(),
                monsters: &monsters,
                spells: &spells,
                monster_groups: &monster_groups,
                eoc_definitions: &[],
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
