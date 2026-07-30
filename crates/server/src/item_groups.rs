use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    AmmunitionRegistry, BashDefinition, BashItemGroupSource, DescriptionSnippetRegistry,
    ItemDefinition, ItemGroupContentsSource, ItemGroupEvent, ItemGroupOverflow, ItemGroupRegistry,
    ItemGroupSubtype, ItemRegistry, ItemTemperatureRuntimeClass, ItemVariableValueDefinition,
    MaterialRegistry, PocketTypeDefinition, StrictItemGroupDefinition, StrictItemGroupGraph,
    StrictItemGroupNode, StrictItemGroupNodeKind, StrictSpawnPocketDefinition,
};
use cdda_protocol::{
    AmmunitionCapacityV1, AmmunitionContainerPocketPrototypeV1, CraftItemPrototypeV1,
    InclusiveU16RangeV1, ItemDescriptionExpansionV1, ItemDescriptionSnippetCategoryV1,
    ItemDescriptionSnippetChoiceV1, ItemGroupChargeCapacityV1, ItemGroupChargeRangeV1,
    ItemGroupContainerV1, ItemGroupContentsSourceV1, ItemGroupDefinitionV1, ItemGroupEntryV1,
    ItemGroupEventV1, ItemGroupGraphV1, ItemGroupItemPrototypeV1, ItemGroupKindV1, ItemGroupNodeV1,
    ItemGroupOverflowV1, ItemGroupSourceV1, ItemGroupTargetV1, ItemGroupToolChargeStorageV1,
    ItemGroupVariantOptionV1, ItemSnippetV1, ItemThermalPropertiesV1, ItemVariableValueV1,
    ItemVariantV1, MAX_DESCRIPTION_SNIPPET_DEPTH, MAX_ITEM_RAW_DAMAGE,
    SPAWN_POCKET_SINGLE_ITEM_MARKER, SpawnPocketKindV1, SpawnPocketRulesV1,
    encode_item_group_dressing_marker, is_reserved_item_group_dressing_marker,
    item_description_expansion_is_valid, item_group_catalog_is_valid,
    item_group_source_max_outputs,
};

use super::{craft_item_prototype, default_instance_charges};

#[derive(Clone, Copy)]
pub(super) struct RuntimeItemGroupContent<'a> {
    pub(super) items: &'a ItemRegistry,
    pub(super) materials: &'a MaterialRegistry,
    pub(super) ammunition: &'a AmmunitionRegistry,
    pub(super) snippets: &'a DescriptionSnippetRegistry,
}

#[cfg(test)]
pub(super) fn assert_regional_field_item_group_closure(
    field_graph: &StrictItemGroupGraph,
    content: RuntimeItemGroupContent<'_>,
) {
    let costume_accessories = runtime_item_group_graph(
        field_graph
            .groups
            .get("costume_accessories")
            .expect("field closure should retain costume accessories"),
        content,
    )
    .expect("multi-pocket holster wrappers should normalize");
    let (throwing_knife, knife_sheath) = costume_accessories
        .nodes
        .iter()
        .flat_map(|node| &node.entries)
        .find_map(|entry| match (&entry.target, &entry.direct_wrapper) {
            (ItemGroupTargetV1::Item(item), Some(wrapper))
                if item.prototype.type_id == "throwing_knife"
                    && wrapper.item.prototype.type_id == "leg_sheath6" =>
            {
                Some((item.as_ref(), wrapper.clone()))
            }
            _ => None,
        })
        .expect("costume accessories should retain the six-knife wrapper entry");
    assert_eq!(knife_sheath.item.prototype.ammunition_containers.len(), 6);
    assert!(
        knife_sheath
            .item
            .prototype
            .ammunition_containers
            .iter()
            .all(|pocket| pocket
                .spawn_rules
                .as_ref()
                .is_some_and(cdda_protocol::spawn_pocket_is_single_item))
    );
    assert_eq!(
        cdda_sim::item_group_multi_pocket_projection(throwing_knife, knife_sheath, 6)
            .expect("six knives should fill six sheath pockets")
            .pocket_contents
            .iter()
            .map(|(index, contents)| (*index, contents.len()))
            .collect::<Vec<_>>(),
        [(0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]
    );

    let costume_hats = runtime_item_group_graph(
        field_graph
            .groups
            .get("costume_hats_hoods")
            .expect("field closure should retain costume hats"),
        content,
    )
    .expect("multi-pocket ablative wrappers should normalize");
    let (mandible_guard, hard_hat) = costume_hats
        .nodes
        .iter()
        .flat_map(|node| &node.entries)
        .find_map(|entry| match (&entry.target, &entry.modifier_container) {
            (ItemGroupTargetV1::Item(item), Some(wrapper))
                if item.prototype.type_id == "plastic_mandible_guard"
                    && wrapper.item.prototype.type_id == "hat_hard" =>
            {
                Some((item.as_ref(), wrapper.clone()))
            }
            _ => None,
        })
        .expect("costume hats should retain the hard-hat mandible guard entry");
    assert_eq!(
        cdda_sim::item_group_multi_pocket_projection(mandible_guard, hard_hat, 1)
            .expect("the mandible guard should select its declared compatible slot")
            .pocket_contents
            .iter()
            .map(|(index, contents)| (*index, contents.len()))
            .collect::<Vec<_>>(),
        [(0, 0), (1, 0), (2, 0), (3, 1), (4, 0), (5, 0)]
    );

    for (item_id, expected_count, first_id, last_id) in [
        (
            "months_old_newspaper",
            24,
            "months_old_news_1",
            "months_old_news_25",
        ),
        ("wallet_photo", 38, "wallet_picture_1", "wallet_picture_38"),
    ] {
        let item = runtime_item_group_item(
            content
                .items
                .get(item_id)
                .unwrap_or_else(|| panic!("field closure should retain {item_id}")),
            None,
            content,
        )
        .unwrap_or_else(|error| panic!("named snippets for {item_id} should normalize: {error}"));
        assert_eq!(item.snippets.len(), expected_count);
        assert_eq!(
            item.snippets.first().map(|snippet| snippet.id.as_str()),
            Some(first_id)
        );
        assert_eq!(
            item.snippets.last().map(|snippet| snippet.id.as_str()),
            Some(last_id)
        );
    }

    let lighter = runtime_item_group_graph(
        field_graph
            .groups
            .get("everyday_lighter")
            .expect("field closure should retain everyday lighters"),
        content,
    )
    .expect("integral match storage should normalize");
    for (item_id, maximum) in [("matches", 20), ("ref_matches", 32)] {
        let item = lighter
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .find_map(|entry| match &entry.target {
                ItemGroupTargetV1::Item(item) if item.prototype.type_id == item_id => Some(item),
                ItemGroupTargetV1::Item(_)
                | ItemGroupTargetV1::Group(_)
                | ItemGroupTargetV1::Node(_) => None,
            })
            .unwrap_or_else(|| panic!("everyday lighter should retain {item_id}"));
        assert_eq!(
            item.charges,
            Some(ItemGroupChargeRangeV1 {
                minimum: 0,
                maximum,
            })
        );
        assert!(matches!(
            item.tool_charge_storage,
            Some(ItemGroupToolChargeStorageV1::Integral { .. })
        ));
    }

    let everyday_gear = runtime_item_group_graph(
        field_graph
            .groups
            .get("everyday_gear")
            .expect("field closure should retain everyday gear"),
        content,
    )
    .expect("group ammunition and magazine dressing should normalize");
    let dressing_marker = encode_item_group_dressing_marker(75, 100)
        .expect("production dressing policy should encode");
    assert!(
        everyday_gear
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .filter(|entry| matches!(entry.target, ItemGroupTargetV1::Item(_) | ItemGroupTargetV1::Group(_)))
            .all(|entry| entry.contents.iter().filter(|contents| {
                matches!(contents, ItemGroupContentsSourceV1::Group(group_id) if group_id == &dressing_marker)
            }).count() == 1),
        "every concrete/named leaf should inherit exactly one dressing policy"
    );
    let marker = everyday_gear
        .nodes
        .iter()
        .flat_map(|node| &node.entries)
        .find_map(|entry| match &entry.target {
            ItemGroupTargetV1::Item(item) if item.prototype.type_id == "permanent_marker" => {
                Some(item)
            }
            ItemGroupTargetV1::Item(_)
            | ItemGroupTargetV1::Group(_)
            | ItemGroupTargetV1::Node(_) => None,
        })
        .expect("everyday gear should retain its permanent marker");
    assert_eq!(
        marker.charges,
        Some(ItemGroupChargeRangeV1 {
            minimum: 0,
            maximum: -1,
        })
    );
    assert!(matches!(
        marker.tool_charge_storage,
        Some(ItemGroupToolChargeStorageV1::Integral { .. })
    ));

    let field_runtime_errors = field_graph
        .groups
        .values()
        .filter_map(|definition| {
            runtime_item_group_graph(definition, content)
                .err()
                .map(|error| (definition.id.as_str(), error.to_string()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        field_runtime_errors
            .iter()
            .map(|(group, error)| (*group, error.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                "everyday_corpse_child",
                "item group item corpse_child_calm requires unimplemented corpse construction",
            ),
            (
                "everyday_corpse_female",
                "item group item corpse_generic_female requires unimplemented corpse construction",
            ),
            (
                "everyday_corpse_male",
                "item group item corpse_generic_male requires unimplemented corpse construction",
            ),
            (
                "flask_liquor",
                "temperature-tracked item whiskey requires a custom freezing point",
            ),
            (
                "lunchbox_food",
                "temperature-tracked item cheeseburger requires unimplemented rot state",
            ),
            (
                "lunchbox_fruit",
                "temperature-tracked item banana requires unimplemented rot state",
            ),
            (
                "sandwich_deluxe_wrapper_2",
                "temperature-tracked item sandwich_deluxe requires unimplemented rot state",
            ),
            (
                "sandwich_reuben_wrapper_2",
                "temperature-tracked item sandwich_reuben requires unimplemented rot state",
            ),
            (
                "sandwich_t_wrapper_2",
                "temperature-tracked item sandwich_t requires unimplemented rot state",
            ),
            (
                "sandwiches",
                "temperature-tracked item sandwich_cucumber requires unimplemented rot state",
            ),
        ],
        "every production-field blocker must remain classified until its generalized family is implemented"
    );
}

fn runtime_spawn_pocket_item_restrictions(
    pocket: &StrictSpawnPocketDefinition,
) -> Option<Vec<String>> {
    if pocket
        .item_restrictions
        .contains(SPAWN_POCKET_SINGLE_ITEM_MARKER)
        || pocket
            .flag_restrictions
            .contains(SPAWN_POCKET_SINGLE_ITEM_MARKER)
    {
        return None;
    }
    let mut restrictions = pocket.item_restrictions.iter().cloned().collect::<Vec<_>>();
    if pocket.single_item {
        restrictions.push(SPAWN_POCKET_SINGLE_ITEM_MARKER.to_owned());
        restrictions.sort_unstable();
    }
    Some(restrictions)
}

pub(super) fn runtime_ammunition_containers(
    item: &ItemDefinition,
) -> Result<Vec<AmmunitionContainerPocketPrototypeV1>, Box<dyn std::error::Error>> {
    let mut pockets = item
        .ammunition_containers
        .iter()
        .map(|pocket| AmmunitionContainerPocketPrototypeV1 {
            pocket_index: pocket.pocket_index,
            pocket_id: pocket.pocket_id.clone(),
            capacities: pocket
                .capacities
                .iter()
                .map(|(ammunition_type, capacity)| AmmunitionCapacityV1 {
                    ammunition_type: ammunition_type.clone(),
                    capacity: *capacity,
                })
                .collect(),
            rigid: pocket.rigid,
            access_moves: pocket.access_moves,
            reloadable: !item.flags.contains("NO_RELOAD"),
            unloadable: !item.flags.contains("NO_UNLOAD"),
            spawn_rules: None,
        })
        .collect::<Vec<_>>();
    let spawn_pockets = item
        .spawn_pockets
        .iter()
        .map(|pocket| {
            let item_restrictions =
                runtime_spawn_pocket_item_restrictions(pocket).ok_or_else(|| {
                    format!(
                        "item {} spawn pocket {} collides with reserved single-item marker",
                        item.id, pocket.pocket_index
                    )
                })?;
            Ok(AmmunitionContainerPocketPrototypeV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                capacities: Vec::new(),
                rigid: pocket.rigid,
                access_moves: pocket.access_moves,
                reloadable: false,
                unloadable: !pocket.forbidden && !item.flags.contains("NO_UNLOAD"),
                spawn_rules: Some(SpawnPocketRulesV1 {
                    kind: match pocket.kind {
                        cdda_content::SpawnPocketKindDefinition::Container => {
                            SpawnPocketKindV1::Container
                        }
                        cdda_content::SpawnPocketKindDefinition::EFileStorage => {
                            SpawnPocketKindV1::EFileStorage
                        }
                    },
                    max_contains_volume_milliliters: pocket.max_contains_volume_milliliters,
                    magazine_well_volume_milliliters: pocket.magazine_well_volume_milliliters,
                    contents_collapsed_by_default: matches!(
                        pocket.kind,
                        cdda_content::SpawnPocketKindDefinition::Container
                    ) && item.flags.contains("COLLAPSE_CONTENTS"),
                    max_contains_weight_milligrams: pocket.max_contains_weight_milligrams,
                    max_item_volume_milliliters: pocket.max_item_volume_milliliters,
                    min_item_volume_milliliters: pocket.min_item_volume_milliliters,
                    max_item_length_millimeters: pocket.max_item_length_millimeters,
                    item_restrictions,
                    flag_restrictions: pocket.flag_restrictions.iter().cloned().collect(),
                    access_moves: pocket.access_moves,
                    rigid: pocket.rigid,
                    watertight: pocket.watertight,
                    transparent: pocket.transparent,
                    forbidden: pocket.forbidden,
                    sealable: pocket.sealable,
                }),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    pockets.extend(spawn_pockets);
    pockets.sort_by_key(|pocket| pocket.pocket_index);
    Ok(pockets)
}

pub(super) fn runtime_bash_item_group_source(
    bash: &BashDefinition,
    item_groups: &ItemGroupRegistry,
    content: RuntimeItemGroupContent<'_>,
    owner_kind: &str,
    owner_id: &str,
) -> Result<Option<ItemGroupSourceV1>, Box<dyn std::error::Error>> {
    let Some(graph) = strict_bash_item_group_graph(bash, item_groups, owner_id)? else {
        return Ok(None);
    };
    if graph.maximum_output > cdda_sim::ID_RESERVATION_SIZE {
        return Err(format!(
            "{owner_kind} {owner_id} bash item group can generate {} objects, exceeding the stable-ID reservation",
            graph.maximum_output
        )
        .into());
    }
    let catalog = runtime_strict_item_group_catalog(&graph, content)?;
    let source = match bash.item_group.as_ref() {
        Some(BashItemGroupSource::Named(group_id)) => ItemGroupSourceV1::Group(group_id.clone()),
        Some(BashItemGroupSource::InlineCollection(_)) => {
            ItemGroupSourceV1::Inline(runtime_item_group_graph(&graph.root, content)?)
        }
        None => return Err("bash item-group source disappeared during normalization".into()),
    };
    let maximum_output = item_group_source_max_outputs(&source, &catalog)
        .ok_or_else(|| format!("{owner_kind} {owner_id} produced an invalid item-group graph"))?;
    if maximum_output > cdda_sim::ID_RESERVATION_SIZE {
        return Err(format!(
            "{owner_kind} {owner_id} item group can generate {maximum_output} objects, exceeding the stable-ID reservation"
        )
        .into());
    }
    Ok(Some(source))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RuntimeItemTemperatureCapability {
    pub(super) tracks_temperature: bool,
    pub(super) thermal_properties: Option<ItemThermalPropertiesV1>,
}

/// Resolves the complete strict constructor capability for nonperishable
/// temperature-tracked items. Rot, custom freezing points, and unsupported
/// phases remain fail closed.
pub(super) fn runtime_item_temperature_capability(
    item: &ItemDefinition,
    materials: &MaterialRegistry,
) -> Result<RuntimeItemTemperatureCapability, Box<dyn std::error::Error>> {
    match item.temperature_runtime_class() {
        ItemTemperatureRuntimeClass::NotTracked => Ok(RuntimeItemTemperatureCapability {
            tracks_temperature: false,
            thermal_properties: None,
        }),
        ItemTemperatureRuntimeClass::MateriallessNonperishable => {
            Ok(RuntimeItemTemperatureCapability {
                tracks_temperature: true,
                thermal_properties: None,
            })
        }
        ItemTemperatureRuntimeClass::RequiresRot => Err(format!(
            "temperature-tracked item {} requires unimplemented rot state",
            item.id
        )
        .into()),
        ItemTemperatureRuntimeClass::RequiresCustomFreezing => Err(format!(
            "temperature-tracked item {} requires a custom freezing point",
            item.id
        )
        .into()),
        ItemTemperatureRuntimeClass::RequiresMaterialThermodynamics => {
            if !matches!(
                item.phase.to_ascii_lowercase().as_str(),
                "" | "solid" | "liquid"
            ) {
                return Err(format!(
                    "temperature-tracked item {} has unsupported phase {}",
                    item.id, item.phase
                )
                .into());
            }
            let properties = materials
                .comestible_thermal_properties(item)?
                .ok_or_else(|| {
                    format!(
                        "material-backed temperature item {} lost its material profile",
                        item.id
                    )
                })?;
            Ok(RuntimeItemTemperatureCapability {
                tracks_temperature: true,
                thermal_properties: Some(ItemThermalPropertiesV1 {
                    specific_heat_liquid_microjoules_per_gram_kelvin: properties
                        .specific_heat_liquid_microjoules_per_gram_kelvin,
                    specific_heat_solid_microjoules_per_gram_kelvin: properties
                        .specific_heat_solid_microjoules_per_gram_kelvin,
                    latent_heat_microjoules_per_gram: properties.latent_heat_microjoules_per_gram,
                    freezing_point_millikelvin: 273_150,
                }),
            })
        }
        ItemTemperatureRuntimeClass::UnsupportedPhase => Err(format!(
            "temperature-tracked item {} has unsupported phase {}",
            item.id, item.phase
        )
        .into()),
    }
}

pub(super) fn runtime_item_tracks_temperature(
    item: &ItemDefinition,
    materials: &MaterialRegistry,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(runtime_item_temperature_capability(item, materials)?.tracks_temperature)
}

fn strict_bash_item_group_graph(
    bash: &BashDefinition,
    item_groups: &ItemGroupRegistry,
    owner_id: &str,
) -> Result<Option<StrictItemGroupGraph>, Box<dyn std::error::Error>> {
    match bash.item_group.as_ref() {
        None => Ok(None),
        Some(BashItemGroupSource::Named(group_id)) => Ok(Some(item_groups.strict_graph(group_id)?)),
        Some(BashItemGroupSource::InlineCollection(entries)) => Ok(Some(
            item_groups.strict_inline_collection(entries, &format!("bash:{owner_id}"))?,
        )),
    }
}

pub(super) fn runtime_bash_item_group_catalog<'a>(
    bashes: impl IntoIterator<Item = &'a BashDefinition>,
    item_groups: &ItemGroupRegistry,
    content: RuntimeItemGroupContent<'_>,
) -> Result<Vec<ItemGroupDefinitionV1>, Box<dyn std::error::Error>> {
    let mut reachable = BTreeMap::<String, StrictItemGroupDefinition>::new();
    for bash in bashes {
        let Some(graph) = strict_bash_item_group_graph(bash, item_groups, "catalog")? else {
            continue;
        };
        if graph.maximum_output > cdda_sim::ID_RESERVATION_SIZE {
            return Err(format!(
                "admitted bash item group can generate {} objects, exceeding the stable-ID reservation",
                graph.maximum_output
            )
            .into());
        }
        for (group_id, definition) in graph.groups {
            match reachable.get(&group_id) {
                Some(existing) if existing != &definition => {
                    return Err(format!(
                        "reachable item group {group_id} normalized inconsistently"
                    )
                    .into());
                }
                Some(_) => {}
                None => {
                    reachable.insert(group_id, definition);
                }
            }
        }
    }
    let catalog = reachable
        .values()
        .map(|definition| runtime_item_group_definition(definition, content))
        .collect::<Result<Vec<_>, _>>()?;
    if !item_group_catalog_is_valid(&catalog) {
        return Err("reachable bash item-group catalog is invalid for the current protocol".into());
    }
    Ok(catalog)
}

fn runtime_strict_item_group_catalog(
    graph: &StrictItemGroupGraph,
    content: RuntimeItemGroupContent<'_>,
) -> Result<Vec<ItemGroupDefinitionV1>, Box<dyn std::error::Error>> {
    let catalog = graph
        .groups
        .values()
        .map(|definition| runtime_item_group_definition(definition, content))
        .collect::<Result<Vec<_>, _>>()?;
    if !item_group_catalog_is_valid(&catalog) {
        return Err(format!(
            "item-group closure rooted at {} is invalid for the current protocol",
            graph.root.id
        )
        .into());
    }
    Ok(catalog)
}

fn runtime_item_group_definition(
    definition: &StrictItemGroupDefinition,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupDefinitionV1, Box<dyn std::error::Error>> {
    Ok(ItemGroupDefinitionV1 {
        group_id: definition.id.clone(),
        graph: runtime_item_group_graph(definition, content)?,
    })
}

pub(super) fn runtime_item_group_graph(
    definition: &StrictItemGroupDefinition,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupGraphV1, Box<dyn std::error::Error>> {
    if is_reserved_item_group_dressing_marker(&definition.id) {
        return Err(format!(
            "item group {} collides with the reserved dressing namespace",
            definition.id
        )
        .into());
    }
    let reachable = strict_item_group_reachable_nodes(definition)?;
    let mut composite_ids = BTreeMap::new();
    for content_node_id in &reachable {
        let node = strict_item_group_node(definition, *content_node_id)?;
        if matches!(
            node.kind,
            StrictItemGroupNodeKind::Collection(_) | StrictItemGroupNodeKind::Distribution(_)
        ) {
            let protocol_node_id = u16::try_from(composite_ids.len() + 1)
                .map_err(|_| format!("item group {} has too many local nodes", definition.id))?;
            composite_ids.insert(*content_node_id, protocol_node_id);
        }
    }

    let mut nodes = vec![ItemGroupNodeV1 {
        node_id: 0,
        kind: runtime_item_group_kind(definition.subtype),
        entries: definition
            .roots
            .iter()
            .map(|node_id| runtime_item_group_entry(definition, *node_id, &composite_ids, content))
            .collect::<Result<Vec<_>, _>>()?,
    }];
    for (content_node_id, protocol_node_id) in &composite_ids {
        let node = strict_item_group_node(definition, *content_node_id)?;
        let (kind, children) = match &node.kind {
            StrictItemGroupNodeKind::Collection(children) => {
                (ItemGroupKindV1::Collection, children)
            }
            StrictItemGroupNodeKind::Distribution(children) => {
                (ItemGroupKindV1::Distribution, children)
            }
            StrictItemGroupNodeKind::Item(_) | StrictItemGroupNodeKind::Group(_) => {
                return Err(format!(
                    "item group {} changed local node shape during normalization",
                    definition.id
                )
                .into());
            }
        };
        nodes.push(ItemGroupNodeV1 {
            node_id: *protocol_node_id,
            kind,
            entries: children
                .iter()
                .map(|node_id| {
                    runtime_item_group_entry(definition, *node_id, &composite_ids, content)
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
        });
    }
    Ok(ItemGroupGraphV1 {
        root_node: 0,
        nodes,
        wrapper: definition
            .wrapper
            .as_ref()
            .map(|wrapper| {
                let item = content.items.get(&wrapper.item).ok_or_else(|| {
                    format!(
                        "item group {} references missing wrapper item {}",
                        definition.id, wrapper.item
                    )
                })?;
                runtime_item_group_container(
                    item,
                    wrapper.variant.clone(),
                    wrapper.sealed,
                    wrapper.overflow,
                    content,
                )
            })
            .transpose()?,
    })
}

fn strict_item_group_reachable_nodes(
    definition: &StrictItemGroupDefinition,
) -> Result<BTreeSet<u32>, Box<dyn std::error::Error>> {
    let mut reachable = BTreeSet::new();
    let mut pending = definition.roots.clone();
    while let Some(node_id) = pending.pop() {
        if !reachable.insert(node_id) {
            continue;
        }
        match &strict_item_group_node(definition, node_id)?.kind {
            StrictItemGroupNodeKind::Collection(children)
            | StrictItemGroupNodeKind::Distribution(children) => {
                pending.extend(children.iter().copied());
            }
            StrictItemGroupNodeKind::Item(_) | StrictItemGroupNodeKind::Group(_) => {}
        }
    }
    Ok(reachable)
}

fn strict_item_group_node(
    definition: &StrictItemGroupDefinition,
    node_id: u32,
) -> Result<&StrictItemGroupNode, Box<dyn std::error::Error>> {
    definition
        .nodes
        .get(usize::try_from(node_id)?)
        .ok_or_else(|| format!("item group {} has invalid node {node_id}", definition.id).into())
}

fn runtime_item_group_kind(subtype: ItemGroupSubtype) -> ItemGroupKindV1 {
    match subtype {
        ItemGroupSubtype::Collection => ItemGroupKindV1::Collection,
        ItemGroupSubtype::Distribution => ItemGroupKindV1::Distribution,
    }
}

fn runtime_item_group_event(event: ItemGroupEvent) -> ItemGroupEventV1 {
    match event {
        ItemGroupEvent::NewYear => ItemGroupEventV1::NewYear,
        ItemGroupEvent::Easter => ItemGroupEventV1::Easter,
        ItemGroupEvent::IndependenceDay => ItemGroupEventV1::IndependenceDay,
        ItemGroupEvent::Halloween => ItemGroupEventV1::Halloween,
        ItemGroupEvent::Thanksgiving => ItemGroupEventV1::Thanksgiving,
        ItemGroupEvent::Christmas => ItemGroupEventV1::Christmas,
    }
}

fn runtime_item_group_entry(
    definition: &StrictItemGroupDefinition,
    node_id: u32,
    composite_ids: &BTreeMap<u32, u16>,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupEntryV1, Box<dyn std::error::Error>> {
    let node = strict_item_group_node(definition, node_id)?;
    let dressing_marker = runtime_item_group_dressing_marker(definition, node);
    let raw_damage = node
        .damage
        .map(
            |damage| -> Result<InclusiveU16RangeV1, Box<dyn std::error::Error>> {
                Ok(InclusiveU16RangeV1 {
                    minimum: u16::try_from(i64::from(damage.minimum) * 1_000)?,
                    maximum: u16::try_from(i64::from(damage.maximum) * 1_000)?,
                })
            },
        )
        .transpose()?
        .or_else(|| {
            (node.variant.is_some() || dressing_marker.is_some()).then_some(InclusiveU16RangeV1 {
                minimum: 0,
                maximum: 0,
            })
        });
    let modifier_present = raw_damage.is_some()
        || node.variant.is_some()
        || node.charges.is_some()
        || node.modifier_container.is_some()
        || node.modifier_sealed.is_some()
        || !node.contents.is_empty()
        || dressing_marker.is_some();
    let target = match &node.kind {
        StrictItemGroupNodeKind::Item(item_id) => {
            let item = content.items.get(item_id).ok_or_else(|| {
                format!(
                    "item group {} references missing concrete item {item_id}",
                    definition.id
                )
            })?;
            ItemGroupTargetV1::Item(Box::new(runtime_item_group_item(
                item,
                node.charges,
                content,
            )?))
        }
        StrictItemGroupNodeKind::Group(group_id) => ItemGroupTargetV1::Group(group_id.clone()),
        StrictItemGroupNodeKind::Collection(_) | StrictItemGroupNodeKind::Distribution(_) => {
            if raw_damage.is_some() || node.variant.is_some() {
                return Err(format!(
                    "item group {} applies a modifier to a local composite whose upstream modifier is not evaluated",
                    definition.id
                )
                .into());
            }
            ItemGroupTargetV1::Node(*composite_ids.get(&node_id).ok_or_else(|| {
                format!(
                    "item group {} lost reachable local node {node_id}",
                    definition.id
                )
            })?)
        }
    };
    let mut contents = node
        .contents
        .iter()
        .map(|contents| match contents {
            ItemGroupContentsSource::Item(item_id) => {
                let item = content.items.get(item_id).ok_or_else(|| {
                    format!(
                        "item group {} references missing contents item {item_id}",
                        definition.id
                    )
                })?;
                Ok(ItemGroupContentsSourceV1::Item(Box::new(
                    runtime_item_group_item(item, None, content)?,
                )))
            }
            ItemGroupContentsSource::Group(group_id) => {
                if is_reserved_item_group_dressing_marker(group_id) {
                    return Err(format!(
                        "item group {} contents reference {} collides with the reserved dressing namespace",
                        definition.id, group_id
                    )
                    .into());
                }
                Ok(ItemGroupContentsSourceV1::Group(group_id.clone()))
            }
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    if let Some(marker) = dressing_marker {
        contents.push(ItemGroupContentsSourceV1::Group(marker));
    }
    Ok(ItemGroupEntryV1 {
        probability: node.probability,
        count_min: u16::try_from(node.count.minimum)?,
        count_max: u16::try_from(node.count.maximum)?,
        raw_damage,
        variant_id: node.variant.clone(),
        event: node.event.map(runtime_item_group_event),
        modifier_charges: match &node.kind {
            StrictItemGroupNodeKind::Group(_) => normalize_item_group_charges(node.charges)?,
            StrictItemGroupNodeKind::Item(_)
            | StrictItemGroupNodeKind::Collection(_)
            | StrictItemGroupNodeKind::Distribution(_) => None,
        },
        contents,
        seal_contents: !node.contents.is_empty() && node.modifier_sealed.unwrap_or(true),
        modifier_default_container_sealed: (modifier_present && node.modifier_container.is_none())
            .then(|| node.modifier_sealed.unwrap_or(true)),
        direct_wrapper: node
            .direct_wrapper
            .as_ref()
            .map(|wrapper| {
                let item = content.items.get(&wrapper.item).ok_or_else(|| {
                    format!(
                        "item group {} references missing entry wrapper {}",
                        definition.id, wrapper.item
                    )
                })?;
                runtime_item_group_container(
                    item,
                    wrapper.variant.clone(),
                    // `Single_item_creator::sealed` is independent from the
                    // modifier's JSON `sealed` member and defaults to true in
                    // the pinned implementation.
                    true,
                    ItemGroupOverflow::None,
                    content,
                )
            })
            .transpose()?,
        modifier_container: node
            .modifier_container
            .as_ref()
            .filter(|item_id| item_id.as_str() != "null")
            .map(|item_id| {
                let item = content.items.get(item_id).ok_or_else(|| {
                    format!(
                        "item group {} references missing modifier container {item_id}",
                        definition.id
                    )
                })?;
                runtime_item_group_creator_container(
                    item,
                    node.modifier_sealed.unwrap_or(true),
                    content,
                )
            })
            .transpose()?,
        target,
    })
}

fn runtime_item_group_dressing_marker(
    definition: &StrictItemGroupDefinition,
    node: &StrictItemGroupNode,
) -> Option<String> {
    matches!(
        node.kind,
        StrictItemGroupNodeKind::Item(_) | StrictItemGroupNodeKind::Group(_)
    )
    .then(|| encode_item_group_dressing_marker(definition.ammo_chance, definition.magazine_chance))
    .flatten()
}

pub(super) fn runtime_item_group_item(
    item: &ItemDefinition,
    charges: Option<cdda_content::ItemGroupChargesRange>,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupItemPrototypeV1, Box<dyn std::error::Error>> {
    runtime_item_group_item_inner(item, charges, content, &mut Vec::new())
}

fn runtime_item_group_item_inner(
    item: &ItemDefinition,
    charges: Option<cdda_content::ItemGroupChargesRange>,
    content: RuntimeItemGroupContent<'_>,
    default_container_stack: &mut Vec<String>,
) -> Result<ItemGroupItemPrototypeV1, Box<dyn std::error::Error>> {
    let (charges, minimum_one_charge) = runtime_item_group_charges(item, charges)?;
    let prototype = craft_item_prototype(
        item,
        default_instance_charges(item),
        content.items,
        content.materials,
    )?;
    validate_item_group_item_spawn(item, &prototype, false)?;
    let modifier_side_effects_supported =
        validate_item_group_item_spawn(item, &prototype, true).is_ok();
    let tool_charge_storage = runtime_item_charge_storage(item, &prototype, content)?;
    let uses_ammunition_loading = item
        .subtypes
        .iter()
        .any(|subtype| matches!(subtype.as_str(), "TOOL" | "GUN" | "MAGAZINE"));
    let charges_supported = if uses_ammunition_loading {
        tool_charge_storage.is_some()
    } else {
        item_group_charges_supported(item)
    };
    if charges.is_some() && !charges_supported {
        return Err(format!("item group item {} cannot retain charge modifiers", item.id).into());
    }
    let contents_insertion_supported = item_group_contents_insertion_supported(item, &prototype);
    Ok(ItemGroupItemPrototypeV1 {
        prototype,
        maximum_raw_damage: if item.count_by_charges() {
            0
        } else {
            MAX_ITEM_RAW_DAMAGE
        },
        variants: runtime_item_variants(item, content.snippets)?,
        description_expansion: item
            .expand_description_snippets
            .then(|| runtime_description_expansion(&item.description, content.snippets))
            .transpose()?,
        snippets: runtime_item_snippets(item, content.snippets)?,
        initial_variables: item
            .variables
            .iter()
            .map(|(key, value)| {
                let value = match value {
                    ItemVariableValueDefinition::Integer(value) => {
                        ItemVariableValueV1::Integer(*value)
                    }
                    ItemVariableValueDefinition::String(value) => {
                        ItemVariableValueV1::String(value.clone())
                    }
                };
                (key.clone(), value)
            })
            .collect(),
        default_container: runtime_default_item_container(item, content, default_container_stack)?,
        modifier_side_effects_supported,
        charges,
        minimum_one_charge,
        tool_charge_storage,
        charges_supported,
        charge_capacity: runtime_item_group_charge_capacity(item),
        contents_insertion_supported,
    })
}

fn runtime_item_snippets(
    item: &ItemDefinition,
    snippets: &DescriptionSnippetRegistry,
) -> Result<Vec<ItemSnippetV1>, Box<dyn std::error::Error>> {
    if !item.snippets.is_empty() {
        if !item.snippet_category.is_empty() {
            return Err(format!(
                "item {} has both inline and named snippet constructors",
                item.id
            )
            .into());
        }
        return Ok(item
            .snippets
            .iter()
            .map(|snippet| ItemSnippetV1 {
                id: snippet.id.clone(),
                text: snippet.text.clone(),
            })
            .collect());
    }
    if item.snippet_category.is_empty() {
        return Ok(Vec::new());
    }
    let category = snippets.get(&item.snippet_category).ok_or_else(|| {
        format!(
            "item {} references missing snippet category {}",
            item.id, item.snippet_category
        )
    })?;
    let Some(first) = category.identified.first() else {
        return Err(format!(
            "item {} snippet category {} has no identified choices",
            item.id, item.snippet_category
        )
        .into());
    };
    if first.weight == 0
        || category
            .identified
            .iter()
            .any(|choice| choice.weight != first.weight)
    {
        return Err(format!(
            "item {} snippet category {} requires weighted named selection",
            item.id, item.snippet_category
        )
        .into());
    }
    if category.identified.len() > cdda_protocol::MAX_ITEM_SNIPPETS {
        return Err(format!(
            "item {} snippet category {} exceeds the runtime choice bound",
            item.id, item.snippet_category
        )
        .into());
    }
    category
        .identified
        .iter()
        .map(|choice| {
            Ok(ItemSnippetV1 {
                id: choice.id.clone().ok_or_else(|| {
                    format!(
                        "item {} snippet category {} contains an unidentified choice",
                        item.id, item.snippet_category
                    )
                })?,
                text: choice.text.clone(),
            })
        })
        .collect()
}

fn runtime_item_charge_storage(
    item: &ItemDefinition,
    prototype: &CraftItemPrototypeV1,
    content: RuntimeItemGroupContent<'_>,
) -> Result<Option<ItemGroupToolChargeStorageV1>, Box<dyn std::error::Error>> {
    let uses_ammunition_loading = item
        .subtypes
        .iter()
        .any(|subtype| matches!(subtype.as_str(), "TOOL" | "GUN" | "MAGAZINE"));
    if !uses_ammunition_loading {
        return Ok(None);
    }
    if !item_group_charge_storage_owner_supported(item) {
        // Pinned guns retain a separate owner-local/ammo_set transition and
        // RNG schedule. Neither integral nor detachable gun charges can reuse
        // the magazine/tool planner, so the complete gun family stays closed.
        return Ok(None);
    }
    if let [pocket] = prototype.integral_magazines.as_slice()
        && prototype.magazine_wells.is_empty()
    {
        let ammunition =
            runtime_default_ammunition_prototype(item, &pocket.ammunition_type, content)?;
        return Ok(Some(ItemGroupToolChargeStorageV1::Integral { ammunition }));
    }
    let ([well], [raw_well]) = (
        prototype.magazine_wells.as_slice(),
        item.magazine_wells.as_slice(),
    ) else {
        return Ok(None);
    };
    if !prototype.integral_magazines.is_empty()
        || raw_well.pocket_index != well.pocket_index
        || raw_well.default_magazine.is_empty()
    {
        return Ok(None);
    }
    let magazine_definition = content
        .items
        .get(&raw_well.default_magazine)
        .ok_or_else(|| {
            format!(
                "item-group charge owner {} references missing default magazine {}",
                item.id, raw_well.default_magazine
            )
        })?;
    let magazine_shape = magazine_definition.strict_magazine().ok_or_else(|| {
        format!(
            "item-group charge owner {} default magazine {} is not a strict single-pocket magazine",
            item.id, magazine_definition.id
        )
    })?;
    if well
        .compatible_magazine_type_ids
        .binary_search(&magazine_definition.id)
        .is_err()
    {
        return Err(format!(
            "item-group charge owner {} default magazine {} is incompatible with well {}",
            item.id, magazine_definition.id, well.pocket_index
        )
        .into());
    }
    validate_charge_item_constructor_state(magazine_definition)?;
    let magazine = craft_item_prototype(magazine_definition, 0, content.items, content.materials)?;
    validate_item_group_item_spawn(magazine_definition, &magazine, false)?;
    let ammunition =
        runtime_default_ammunition_prototype(item, &magazine_shape.ammunition_type, content)?;
    Ok(Some(ItemGroupToolChargeStorageV1::Detachable {
        well_pocket_index: well.pocket_index,
        magazine,
        ammunition: Box::new(ammunition),
    }))
}

fn item_group_charge_storage_owner_supported(item: &ItemDefinition) -> bool {
    !item.subtypes.contains("GUN")
}

fn runtime_default_ammunition_prototype(
    owner: &ItemDefinition,
    ammunition_type: &str,
    content: RuntimeItemGroupContent<'_>,
) -> Result<CraftItemPrototypeV1, Box<dyn std::error::Error>> {
    let default_id = &content
        .ammunition
        .get(ammunition_type)
        .ok_or_else(|| {
            format!(
                "item-group charge owner {} references missing ammunition type {ammunition_type}",
                owner.id
            )
        })?
        .default_item;
    let definition = content.items.get(default_id).ok_or_else(|| {
        format!(
            "item-group charge owner {} has no concrete default ammunition {} for {ammunition_type}",
            owner.id, default_id
        )
    })?;
    validate_charge_item_constructor_state(definition)?;
    let mut ammunition = craft_item_prototype(definition, 1, content.items, content.materials)?;
    validate_item_group_item_spawn(definition, &ammunition, false)?;
    if ammunition.ammunition_type != ammunition_type {
        return Err(format!(
            "item-group charge owner {} default ammunition {} does not match {ammunition_type}",
            owner.id, definition.id
        )
        .into());
    }
    ammunition.charges = 1;
    Ok(ammunition)
}

fn item_group_contents_insertion_supported(
    item: &ItemDefinition,
    prototype: &CraftItemPrototypeV1,
) -> bool {
    item.pockets.iter().all(|raw| {
        if raw.strict_integral_magazine().is_some() {
            return prototype
                .integral_magazines
                .iter()
                .any(|pocket| pocket.pocket_index == raw.pocket_index);
        }
        if raw.strict_magazine_well() {
            return prototype
                .magazine_wells
                .iter()
                .any(|pocket| pocket.pocket_index == raw.pocket_index);
        }
        if raw.strict_ammunition_container().is_some() {
            return prototype.ammunition_containers.iter().any(|pocket| {
                pocket.pocket_index == raw.pocket_index && pocket.spawn_rules.is_none()
            });
        }
        if raw.strict_spawn_pocket().is_some() {
            return prototype.ammunition_containers.iter().any(|pocket| {
                pocket.pocket_index == raw.pocket_index && pocket.spawn_rules.is_some()
            });
        }
        false
    })
}

fn validate_charge_item_constructor_state(
    item: &ItemDefinition,
) -> Result<(), Box<dyn std::error::Error>> {
    if !item.variants.is_empty()
        || item.expand_description_snippets
        || !item.snippets.is_empty()
        || !item.snippet_category.is_empty()
        || !item.variables.is_empty()
    {
        return Err(format!(
            "ammunition-loading item {} has constructor variant, snippet, or variable state that is not represented by ItemGroupToolChargeStorageV1",
            item.id
        )
        .into());
    }
    Ok(())
}

fn runtime_item_group_container(
    item: &ItemDefinition,
    variant_id: Option<String>,
    sealed: bool,
    overflow: ItemGroupOverflow,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupContainerV1, Box<dyn std::error::Error>> {
    runtime_item_group_container_inner(item, variant_id, sealed, overflow, content, &mut Vec::new())
}

fn runtime_item_group_creator_container(
    item: &ItemDefinition,
    sealed: bool,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupContainerV1, Box<dyn std::error::Error>> {
    let item = runtime_item_group_item_inner(item, None, content, &mut Vec::new())?;
    let container = ItemGroupContainerV1 {
        item: Box::new(item),
        variant_id: None,
        sealed,
        overflow: ItemGroupOverflowV1::None,
    };
    let effective = container
        .item
        .default_container
        .as_ref()
        .unwrap_or(&container);
    require_physical_container(effective)?;
    Ok(container)
}

fn runtime_item_group_container_inner(
    item: &ItemDefinition,
    variant_id: Option<String>,
    sealed: bool,
    overflow: ItemGroupOverflow,
    content: RuntimeItemGroupContent<'_>,
    default_container_stack: &mut Vec<String>,
) -> Result<ItemGroupContainerV1, Box<dyn std::error::Error>> {
    let item = runtime_item_group_item_inner(item, None, content, default_container_stack)?;
    require_physical_item(&item)?;
    if variant_id.as_ref().is_some_and(|variant_id| {
        !item
            .variants
            .iter()
            .any(|option| option.variant.id == *variant_id)
    }) {
        return Err("item-group wrapper references an unavailable variant".into());
    }
    Ok(ItemGroupContainerV1 {
        item: Box::new(item),
        variant_id,
        sealed,
        overflow: match overflow {
            ItemGroupOverflow::None => ItemGroupOverflowV1::None,
            ItemGroupOverflow::Spill => ItemGroupOverflowV1::Spill,
            ItemGroupOverflow::Discard => ItemGroupOverflowV1::Discard,
        },
    })
}

fn require_physical_container(
    container: &ItemGroupContainerV1,
) -> Result<(), Box<dyn std::error::Error>> {
    require_physical_item(&container.item)
}

fn require_physical_item(
    item: &ItemGroupItemPrototypeV1,
) -> Result<(), Box<dyn std::error::Error>> {
    let physical_pockets = item
        .prototype
        .ammunition_containers
        .iter()
        .filter_map(|pocket| pocket.spawn_rules.as_ref())
        .filter(|rules| rules.kind == cdda_protocol::SpawnPocketKindV1::Container)
        .collect::<Vec<_>>();
    if physical_pockets.is_empty() {
        return Err(format!(
            "item-group wrapper {} requires at least one supported physical container pocket",
            item.prototype.type_id
        )
        .into());
    }
    Ok(())
}

fn runtime_default_item_container(
    item: &ItemDefinition,
    content: RuntimeItemGroupContent<'_>,
    default_container_stack: &mut Vec<String>,
) -> Result<Option<ItemGroupContainerV1>, Box<dyn std::error::Error>> {
    if item.default_container.is_empty() || item.default_container == "null" {
        return Ok(None);
    }
    if default_container_stack.len() >= cdda_protocol::MAX_ITEM_COMPONENT_DEPTH
        || default_container_stack.contains(&item.id)
    {
        return Err(format!(
            "item group item {} has cyclic or excessively deep default containment",
            item.id
        )
        .into());
    }
    let container = content.items.get(&item.default_container).ok_or_else(|| {
        format!(
            "item group item {} references missing default container {}",
            item.id, item.default_container
        )
    })?;
    default_container_stack.push(item.id.clone());
    let normalized = runtime_item_group_container_inner(
        container,
        (!item.default_container_variant.is_empty())
            .then(|| item.default_container_variant.clone()),
        item.default_container_sealed.unwrap_or(true),
        ItemGroupOverflow::None,
        content,
        default_container_stack,
    );
    default_container_stack.pop();
    normalized.map(Some)
}

fn runtime_item_variants(
    item: &ItemDefinition,
    snippets: &DescriptionSnippetRegistry,
) -> Result<Vec<ItemGroupVariantOptionV1>, Box<dyn std::error::Error>> {
    if item.variants.is_empty() {
        return Ok(Vec::new());
    }
    if item.variant_type != "generic" {
        return Err(format!(
            "item-group item {} uses unsupported {} variant visibility policy",
            item.id, item.variant_type
        )
        .into());
    }
    item.variants
        .iter()
        .map(|variant| {
            if !variant.unsupported_fields.is_empty() {
                return Err(format!(
                    "item-group item {} variant {} requires unsupported fields {:?}",
                    item.id, variant.id, variant.unsupported_fields
                )
                .into());
            }
            // Pinned item-type finalization fills missing variant text from
            // the finalized base item before instances select a variant.
            let name = variant
                .name
                .clone()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| item.name.clone());
            let alternate_description = variant
                .description
                .clone()
                .filter(|description| !description.is_empty())
                .unwrap_or_else(|| item.description.clone());
            let description = if variant.append {
                format!("{}  {alternate_description}", item.description)
            } else {
                alternate_description
            };
            let description_expansion = variant
                .expand_description_snippets
                .then(|| runtime_description_expansion(&description, snippets))
                .transpose()?;
            Ok(ItemGroupVariantOptionV1 {
                variant: ItemVariantV1 {
                    id: variant.id.clone(),
                    name,
                    description,
                    symbol: variant
                        .symbol
                        .clone()
                        .unwrap_or_else(|| item.symbol.clone()),
                    color: variant.color.clone().unwrap_or_else(|| item.color.clone()),
                    ascii_picture: variant
                        .ascii_picture
                        .clone()
                        .filter(|ascii_picture| !ascii_picture.is_empty())
                        .unwrap_or_else(|| item.ascii_picture.clone()),
                },
                weight: variant.weight,
                description_expansion,
            })
        })
        .collect()
}

fn runtime_description_expansion(
    template: &str,
    snippets: &DescriptionSnippetRegistry,
) -> Result<ItemDescriptionExpansionV1, Box<dyn std::error::Error>> {
    let mut reachable = BTreeMap::new();
    let mut visiting = BTreeSet::new();
    for tag in description_tags(template) {
        collect_description_category(tag, snippets, &mut reachable, &mut visiting, 0)?;
    }
    let expansion = ItemDescriptionExpansionV1 {
        template: template.to_owned(),
        categories: reachable.into_values().collect(),
    };
    if !item_description_expansion_is_valid(&expansion) {
        return Err("description snippet closure exceeds canonical bounds or is cyclic".into());
    }
    Ok(expansion)
}

fn collect_description_category(
    category_id: &str,
    snippets: &DescriptionSnippetRegistry,
    reachable: &mut BTreeMap<String, ItemDescriptionSnippetCategoryV1>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if reachable.contains_key(category_id) || snippets.get(category_id).is_none() {
        return Ok(());
    }
    if depth > MAX_DESCRIPTION_SNIPPET_DEPTH || !visiting.insert(category_id.to_owned()) {
        return Err(
            format!("cyclic or oversized description snippet category {category_id}").into(),
        );
    }
    let category = snippets
        .get(category_id)
        .ok_or("description snippet category disappeared")?;
    let choices = category
        .choices()
        .map(|choice| ItemDescriptionSnippetChoiceV1 {
            text: choice.text.clone(),
            weight: choice.weight,
        })
        .collect::<Vec<_>>();
    for choice in choices.iter().filter(|choice| choice.weight > 0) {
        for tag in description_tags(&choice.text) {
            collect_description_category(
                tag,
                snippets,
                reachable,
                visiting,
                depth.checked_add(1).ok_or("snippet depth overflow")?,
            )?;
        }
    }
    visiting.remove(category_id);
    reachable.insert(
        category_id.to_owned(),
        ItemDescriptionSnippetCategoryV1 {
            category: category_id.to_owned(),
            choices,
        },
    );
    Ok(())
}

fn description_tags(text: &str) -> Vec<&str> {
    let mut tags = Vec::new();
    let mut offset = 0_usize;
    while let Some(relative_begin) = text[offset..].find('<') {
        let begin = offset + relative_begin;
        let Some(relative_end) = text[begin + 1..].find('>') else {
            break;
        };
        let end = begin + relative_end + 2;
        tags.push(&text[begin..end]);
        offset = end;
    }
    tags
}

fn validate_item_group_item_spawn(
    item: &ItemDefinition,
    prototype: &CraftItemPrototypeV1,
    modifier_present: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if item.id == "null" {
        return Err("item-group null leaves do not materialize an item upstream".into());
    }
    if item.id == "corpse" || item.flags.contains("CORPSE") {
        return Err(format!(
            "item group item {} requires unimplemented corpse construction",
            item.id
        )
        .into());
    }
    const CONSTRUCTOR_STATE_FIELDS: &[&str] = &["countdown_interval", "relic_data"];
    if let Some(field) = CONSTRUCTOR_STATE_FIELDS
        .iter()
        .find(|field| item.unsupported_fields.contains(**field))
    {
        return Err(format!(
            "item group item {} requires unimplemented constructor state field {field}",
            item.id
        )
        .into());
    }
    const CONSTRUCTOR_STATE_FLAGS: &[&str] = &["ENERGY_SHIELD", "NANOFAB_TEMPLATE", "SPAWN_ACTIVE"];
    if let Some(flag) = CONSTRUCTOR_STATE_FLAGS
        .iter()
        .find(|flag| item.flags.contains(**flag))
    {
        return Err(format!(
            "item group item {} requires unimplemented constructor state flag {flag}",
            item.id
        )
        .into());
    }
    if item.subtypes.contains("MAGAZINE") && item.count > 0 {
        return Err(format!(
            "item group magazine {} requires unimplemented preloaded ammunition",
            item.id
        )
        .into());
    }
    const CONSTRUCTOR_RNG_FIELDS: &[&str] = &[
        "nanofab_template_group",
        "trait_group",
        "built_in_mods",
        "default_mods",
    ];
    if let Some(field) = CONSTRUCTOR_RNG_FIELDS
        .iter()
        .find(|field| item.unsupported_fields.contains(**field))
    {
        return Err(format!(
            "item group item {} requires unimplemented constructor RNG field {field}",
            item.id
        )
        .into());
    }
    if item.unsupported_fields.contains("variables") && item.variables.is_empty() {
        return Err(format!(
            "item group item {} has an unsupported empty variables field",
            item.id
        )
        .into());
    }
    if let Some(variable) = item
        .variables
        .keys()
        .find(|key| matches!(key.as_str(), "weight" | "integral_weight" | "volume"))
    {
        return Err(format!(
            "item group item {} variable {variable} changes unimplemented physical dimensions",
            item.id
        )
        .into());
    }
    if item.unsupported_fields.contains("snippet_category")
        && item.snippets.is_empty()
        && item.snippet_category.is_empty()
    {
        return Err(format!(
            "item group item {} requires an unsupported named or empty snippet category",
            item.id
        )
        .into());
    }
    if modifier_present && item.category == "veh_parts" && !item.count_by_charges() {
        return Err(format!(
            "item group modifier for {} requires unimplemented degradation state",
            item.id
        )
        .into());
    }
    if modifier_present
        && item.subtypes.contains("GUN")
        && !item.flags.contains("PRIMITIVE_RANGED_WEAPON")
        && !item.flags.contains("NON_FOULING")
    {
        return Err(format!(
            "item group modifier for {} requires unimplemented gun dirt and fault state",
            item.id
        )
        .into());
    }
    let upstream_uses_magazine_dressing = item.subtypes.contains("MAGAZINE")
        || item
            .pockets
            .iter()
            .any(|pocket| pocket.pocket_type == PocketTypeDefinition::MagazineWell);
    let projected_uses_magazine_dressing = prototype.magazine_capacity > 0
        || !prototype.integral_magazines.is_empty()
        || !prototype.magazine_wells.is_empty();
    if modifier_present && upstream_uses_magazine_dressing && !projected_uses_magazine_dressing {
        return Err(format!(
            "item group modifier for {} has unrepresented magazine dressing draws",
            item.id
        )
        .into());
    }
    Ok(())
}

pub(super) fn runtime_item_group_charges(
    item: &ItemDefinition,
    charges: Option<cdda_content::ItemGroupChargesRange>,
) -> Result<(Option<ItemGroupChargeRangeV1>, bool), Box<dyn std::error::Error>> {
    let Some(charges) = charges else {
        return Ok((None, false));
    };
    let liquid = matches!(item.phase.as_str(), "LIQUID" | "liquid");
    if item
        .subtypes
        .iter()
        .any(|subtype| matches!(subtype.as_str(), "TOOL" | "GUN" | "MAGAZINE"))
    {
        return Ok((
            Some(ItemGroupChargeRangeV1 {
                minimum: charges.minimum,
                maximum: charges.maximum,
            }),
            false,
        ));
    }
    if !item_group_charges_supported(item) {
        let resolved = cdda_sim::resolve_item_group_charge_range(
            ItemGroupChargeRangeV1 {
                minimum: charges.minimum,
                maximum: charges.maximum,
            },
            ItemGroupChargeCapacityV1::None,
            None,
        )?;
        if resolved.is_none_or(|charges| charges.minimum == charges.maximum) {
            // Pinned Item_modifier computes the fixed value without consuming
            // RNG, then ignores it for an ordinary item.
            return Ok((None, false));
        }
        return Err(format!(
            "ranged item-group charges for ordinary item {} require consume-only RNG semantics",
            item.id
        )
        .into());
    }
    Ok((
        Some(ItemGroupChargeRangeV1 {
            minimum: charges.minimum,
            maximum: charges.maximum,
        }),
        item.count_by_charges() || liquid,
    ))
}

fn normalize_item_group_charges(
    charges: Option<cdda_content::ItemGroupChargesRange>,
) -> Result<Option<ItemGroupChargeRangeV1>, Box<dyn std::error::Error>> {
    let Some(charges) = charges else {
        return Ok(None);
    };
    Ok(Some(ItemGroupChargeRangeV1 {
        minimum: charges.minimum,
        maximum: charges.maximum,
    }))
}

fn runtime_item_group_charge_capacity(item: &ItemDefinition) -> ItemGroupChargeCapacityV1 {
    if !item.integral_magazines.is_empty()
        || item.subtypes.contains("MAGAZINE")
        || item
            .pockets
            .iter()
            .any(|pocket| pocket.pocket_type == PocketTypeDefinition::MagazineWell)
    {
        ItemGroupChargeCapacityV1::AmmunitionStorage
    } else if matches!(item.phase.as_str(), "LIQUID" | "liquid")
        || !item
            .subtypes
            .iter()
            .any(|subtype| matches!(subtype.as_str(), "TOOL" | "GUN" | "MAGAZINE"))
    {
        ItemGroupChargeCapacityV1::ModifierContainer
    } else {
        ItemGroupChargeCapacityV1::None
    }
}

fn item_group_charges_supported(item: &ItemDefinition) -> bool {
    if item.count_by_charges() || matches!(item.phase.as_str(), "LIQUID" | "liquid") {
        return true;
    }
    item.flags.contains("CAN_HAVE_CHARGES")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_content::{
        ItemGroupRange, ItemVariantDefinition, PocketDefinition, SpawnPocketKindDefinition,
    };

    fn strict_group_node(kind: StrictItemGroupNodeKind) -> StrictItemGroupNode {
        StrictItemGroupNode {
            kind,
            probability: 100,
            count: ItemGroupRange::ONE,
            charges: None,
            damage: None,
            variant: None,
            direct_wrapper: None,
            modifier_container: None,
            modifier_sealed: None,
            contents: Vec::new(),
            event: None,
        }
    }

    #[test]
    fn inherited_dressing_marks_only_concrete_and_named_leaves() {
        let definition = StrictItemGroupDefinition {
            id: String::from("dressed_group"),
            subtype: ItemGroupSubtype::Collection,
            ammo_chance: 75,
            magazine_chance: 100,
            wrapper: None,
            roots: vec![0],
            nodes: vec![
                strict_group_node(StrictItemGroupNodeKind::Collection(vec![1])),
                strict_group_node(StrictItemGroupNodeKind::Group(String::from("inner"))),
            ],
        };
        assert_eq!(
            runtime_item_group_dressing_marker(&definition, &definition.nodes[0]),
            None
        );
        assert_eq!(
            runtime_item_group_dressing_marker(&definition, &definition.nodes[1]),
            Some(String::from("__CDDA_ITEM_GROUP_DRESSING_V1:75:100"))
        );
    }

    fn materialless_temperature_item() -> ItemDefinition {
        ItemDefinition {
            id: String::from("chaw"),
            phase: String::from("solid"),
            subtypes: BTreeSet::from([String::from("COMESTIBLE")]),
            flags: BTreeSet::from([String::from("CAN_HAVE_CHARGES")]),
            ..ItemDefinition::default()
        }
    }

    #[test]
    fn reserved_single_item_marker_collision_fails_closed() {
        let item = ItemDefinition {
            id: String::from("hostile_marker_collision"),
            spawn_pockets: vec![StrictSpawnPocketDefinition {
                pocket_index: 0,
                pocket_id: String::new(),
                kind: SpawnPocketKindDefinition::Container,
                max_contains_volume_milliliters: 1,
                magazine_well_volume_milliliters: 0,
                max_contains_weight_milligrams: 1,
                max_item_volume_milliliters: 1,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: 1,
                item_restrictions: BTreeSet::from([String::from(SPAWN_POCKET_SINGLE_ITEM_MARKER)]),
                flag_restrictions: BTreeSet::new(),
                access_moves: 100,
                rigid: true,
                watertight: false,
                transparent: false,
                forbidden: false,
                sealable: false,
                single_item: false,
            }],
            ..ItemDefinition::default()
        };
        assert_eq!(
            runtime_ammunition_containers(&item)
                .expect_err("reserved marker collisions must not erase canonical pockets")
                .to_string(),
            "item hostile_marker_collision spawn pocket 0 collides with reserved single-item marker"
        );
    }

    #[test]
    fn named_snippet_projection_rejects_missing_and_conflicting_categories() {
        let missing = ItemDefinition {
            id: String::from("missing_named_snippets"),
            snippet_category: String::from("not_loaded"),
            ..ItemDefinition::default()
        };
        assert_eq!(
            runtime_item_snippets(&missing, &DescriptionSnippetRegistry::default())
                .expect_err("missing named categories must fail closed")
                .to_string(),
            "item missing_named_snippets references missing snippet category not_loaded"
        );

        let conflicting = ItemDefinition {
            id: String::from("conflicting_snippets"),
            snippet_category: String::from("named"),
            snippets: vec![cdda_content::ItemSnippetDefinition {
                id: String::from("inline"),
                text: String::from("inline text"),
            }],
            ..ItemDefinition::default()
        };
        assert_eq!(
            runtime_item_snippets(&conflicting, &DescriptionSnippetRegistry::default())
                .expect_err("mixed snippet constructors must fail closed")
                .to_string(),
            "item conflicting_snippets has both inline and named snippet constructors"
        );
    }

    #[test]
    fn temperature_admission_is_generalized_and_fail_closed() {
        let materials = MaterialRegistry::default();
        let supported = materialless_temperature_item();
        assert_eq!(
            runtime_item_tracks_temperature(&supported, &materials).ok(),
            Some(true)
        );

        let mut no_temp = supported.clone();
        no_temp.flags.insert(String::from("NO_TEMP"));
        assert_eq!(
            runtime_item_tracks_temperature(&no_temp, &materials).ok(),
            Some(false)
        );

        let mut material_backed = supported.clone();
        material_backed.materials.insert(String::from("water"), 1);
        assert!(runtime_item_tracks_temperature(&material_backed, &materials).is_err());

        let mut perishable = supported.clone();
        perishable
            .unsupported_fields
            .insert(String::from("spoils_in"));
        assert!(runtime_item_tracks_temperature(&perishable, &materials).is_err());

        let mut custom_freezing = supported;
        custom_freezing
            .unsupported_fields
            .insert(String::from("freezing_point"));
        assert!(runtime_item_tracks_temperature(&custom_freezing, &materials).is_err());
    }

    #[test]
    fn finalized_variants_fall_back_to_base_text_and_art_before_append() {
        let item = ItemDefinition {
            id: String::from("variant_item"),
            name: String::from("base name"),
            description: String::from("base description"),
            symbol: String::from("?"),
            color: String::from("white"),
            ascii_picture: String::from("base_art"),
            variant_type: String::from("generic"),
            variants: vec![
                ItemVariantDefinition {
                    id: String::from("fallback"),
                    ..ItemVariantDefinition::default()
                },
                ItemVariantDefinition {
                    id: String::from("append_fallback"),
                    name: Some(String::from("alternate name")),
                    append: true,
                    ..ItemVariantDefinition::default()
                },
                ItemVariantDefinition {
                    id: String::from("append_explicit"),
                    description: Some(String::from("alternate description")),
                    ascii_picture: Some(String::from("alternate_art")),
                    append: true,
                    ..ItemVariantDefinition::default()
                },
            ],
            ..ItemDefinition::default()
        };

        let variants = runtime_item_variants(&item, &DescriptionSnippetRegistry::default())
            .expect("generic variants should project");
        assert_eq!(variants[0].variant.name, "base name");
        assert_eq!(variants[0].variant.description, "base description");
        assert_eq!(variants[0].variant.ascii_picture, "base_art");
        assert_eq!(variants[1].variant.name, "alternate name");
        assert_eq!(
            variants[1].variant.description,
            "base description  base description"
        );
        assert_eq!(
            variants[2].variant.description,
            "base description  alternate description"
        );
        assert_eq!(variants[2].variant.ascii_picture, "alternate_art");
    }

    #[test]
    fn charge_item_constructor_state_fails_closed() {
        let plain = ItemDefinition {
            id: String::from("plain_ammunition"),
            ..ItemDefinition::default()
        };
        assert!(validate_charge_item_constructor_state(&plain).is_ok());

        let mut variant = plain.clone();
        variant.variants = vec![ItemVariantDefinition {
            id: String::from("tracer"),
            ..ItemVariantDefinition::default()
        }];
        assert!(validate_charge_item_constructor_state(&variant).is_err());

        let mut snippet = plain.clone();
        snippet.snippets = vec![cdda_content::ItemSnippetDefinition {
            id: String::from("marked"),
            text: String::from("Marked lot"),
        }];
        assert!(validate_charge_item_constructor_state(&snippet).is_err());

        let mut named_snippet = plain.clone();
        named_snippet.snippet_category = String::from("marked_lots");
        assert!(validate_charge_item_constructor_state(&named_snippet).is_err());

        let mut variable = plain;
        variable.variables.insert(
            String::from("lot"),
            cdda_content::ItemVariableValueDefinition::Integer(7),
        );
        assert!(validate_charge_item_constructor_state(&variable).is_err());
    }

    #[test]
    fn gun_charge_storage_stays_fail_closed_for_every_pocket_shape() {
        let gun = ItemDefinition {
            id: String::from("test_detachable_gun"),
            subtypes: BTreeSet::from([String::from("GUN")]),
            ..ItemDefinition::default()
        };
        assert!(
            !item_group_charge_storage_owner_supported(&gun),
            "integral and detachable guns use distinct owner-local/ammo_set state and RNG semantics"
        );

        let tool = ItemDefinition {
            id: String::from("test_detachable_tool"),
            subtypes: BTreeSet::from([String::from("TOOL")]),
            ..ItemDefinition::default()
        };
        assert!(item_group_charge_storage_owner_supported(&tool));
    }

    #[test]
    fn item_group_spawn_rejects_physical_override_variables() {
        let prototype = CraftItemPrototypeV1 {
            type_id: String::from("physical_override"),
            charges: 1,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            tracks_temperature: false,
            thermal_properties: None,
            ammunition_type: String::new(),
            ranged_weapon: None,
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            containment: Default::default(),
        };
        for reserved in ["weight", "integral_weight", "volume"] {
            let mut item = ItemDefinition {
                id: String::from("physical_override"),
                ..ItemDefinition::default()
            };
            item.variables.insert(
                reserved.to_owned(),
                cdda_content::ItemVariableValueDefinition::Integer(1),
            );
            assert!(
                validate_item_group_item_spawn(&item, &prototype, false).is_err(),
                "reserved variable {reserved} must fail closed"
            );
        }
    }

    #[test]
    fn contents_projection_distinguishes_no_pockets_from_lost_pockets() {
        let empty_prototype = || CraftItemPrototypeV1 {
            type_id: String::from("projection_target"),
            charges: 1,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            tracks_temperature: false,
            thermal_properties: None,
            ammunition_type: String::new(),
            ranged_weapon: None,
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            containment: Default::default(),
        };
        let no_pockets = ItemDefinition::default();
        assert!(item_group_contents_insertion_supported(
            &no_pockets,
            &empty_prototype(),
        ));

        let unsupported = ItemDefinition {
            pockets: vec![PocketDefinition {
                pocket_index: 0,
                pocket_type: PocketTypeDefinition::Corpse,
                pocket_id: String::new(),
                ammo_restrictions: BTreeMap::new(),
                item_restrictions: BTreeSet::new(),
                flag_restrictions: BTreeSet::new(),
                default_magazine: String::new(),
                raw_fields: BTreeMap::new(),
            }],
            ..ItemDefinition::default()
        };
        assert!(!item_group_contents_insertion_supported(
            &unsupported,
            &empty_prototype(),
        ));

        let multi_ammo_integral = ItemDefinition {
            pockets: vec![PocketDefinition {
                pocket_index: 0,
                pocket_type: PocketTypeDefinition::Magazine,
                pocket_id: String::from("POWER"),
                ammo_restrictions: BTreeMap::from([
                    (String::from("battery"), 10),
                    (String::from("plutonium"), 5),
                ]),
                item_restrictions: BTreeSet::new(),
                flag_restrictions: BTreeSet::new(),
                default_magazine: String::new(),
                raw_fields: BTreeMap::new(),
            }],
            ..ItemDefinition::default()
        };
        assert!(!item_group_contents_insertion_supported(
            &multi_ammo_integral,
            &empty_prototype(),
        ));

        let lost_well = ItemDefinition {
            pockets: vec![PocketDefinition {
                pocket_index: 3,
                pocket_type: PocketTypeDefinition::MagazineWell,
                pocket_id: String::from("MAGAZINE"),
                ammo_restrictions: BTreeMap::new(),
                item_restrictions: BTreeSet::from([String::from("test_magazine")]),
                flag_restrictions: BTreeSet::new(),
                default_magazine: String::new(),
                raw_fields: BTreeMap::new(),
            }],
            ..ItemDefinition::default()
        };
        assert!(!item_group_contents_insertion_supported(
            &lost_well,
            &empty_prototype(),
        ));

        let one_ammo_integral = ItemDefinition {
            pockets: vec![PocketDefinition {
                pocket_index: 0,
                pocket_type: PocketTypeDefinition::Magazine,
                pocket_id: String::from("POWER"),
                ammo_restrictions: BTreeMap::from([(String::from("battery"), 10)]),
                item_restrictions: BTreeSet::new(),
                flag_restrictions: BTreeSet::new(),
                default_magazine: String::new(),
                raw_fields: BTreeMap::new(),
            }],
            ..ItemDefinition::default()
        };
        let mut projected = empty_prototype();
        projected.charges = 0;
        projected.integral_magazines = vec![cdda_protocol::IntegralMagazinePocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("POWER"),
            ammunition_type: String::from("battery"),
            capacity: 10,
            rigid: false,
            reloadable: true,
            unloadable: true,
        }];
        assert!(item_group_contents_insertion_supported(
            &one_ammo_integral,
            &projected,
        ));
    }
}
