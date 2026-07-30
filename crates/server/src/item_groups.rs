use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    AmmunitionRegistry, BashDefinition, BashItemGroupSource, DescriptionSnippetRegistry,
    ItemDefinition, ItemGroupContentsSource, ItemGroupEvent, ItemGroupOverflow, ItemGroupRegistry,
    ItemGroupSubtype, ItemRegistry, ItemVariableValueDefinition, PocketTypeDefinition,
    StrictItemGroupDefinition, StrictItemGroupGraph, StrictItemGroupNode, StrictItemGroupNodeKind,
};
use cdda_protocol::{
    CraftItemPrototypeV1, InclusiveI32RangeV1, InclusiveU16RangeV1, ItemDescriptionExpansionV1,
    ItemDescriptionSnippetCategoryV1, ItemDescriptionSnippetChoiceV1, ItemGroupContainerV1,
    ItemGroupContentsSourceV1, ItemGroupDefinitionV1, ItemGroupEntryV1, ItemGroupEventV1,
    ItemGroupGraphV1, ItemGroupItemPrototypeV1, ItemGroupKindV1, ItemGroupNodeV1,
    ItemGroupOverflowV1, ItemGroupSourceV1, ItemGroupTargetV1, ItemGroupToolChargeStorageV1,
    ItemGroupVariantOptionV1, ItemSnippetV1, ItemVariableValueV1, ItemVariantV1,
    MAX_DESCRIPTION_SNIPPET_DEPTH, MAX_ITEM_RAW_DAMAGE, item_description_expansion_is_valid,
    item_group_catalog_is_valid, item_group_source_max_outputs,
};

use super::{craft_item_prototype, default_instance_charges};

#[derive(Clone, Copy)]
pub(super) struct RuntimeItemGroupContent<'a> {
    pub(super) items: &'a ItemRegistry,
    pub(super) ammunition: &'a AmmunitionRegistry,
    pub(super) snippets: &'a DescriptionSnippetRegistry,
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
    if definition.ammo_chance != 0 || definition.magazine_chance != 0 {
        return Err(format!(
            "item group {} requires unimplemented ammunition or magazine dressing",
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
            node.variant.as_ref().map(|_| InclusiveU16RangeV1 {
                minimum: 0,
                maximum: 0,
            })
        });
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
        contents: node
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
                    Ok(ItemGroupContentsSourceV1::Group(group_id.clone()))
                }
            })
            .collect::<Result<_, Box<dyn std::error::Error>>>()?,
        seal_contents: !node.contents.is_empty() && node.modifier_sealed.unwrap_or(true),
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
                runtime_item_group_container(
                    item,
                    None,
                    node.modifier_sealed.unwrap_or(true),
                    ItemGroupOverflow::None,
                    content,
                )
            })
            .transpose()?,
        target,
    })
}

pub(super) fn runtime_item_group_item(
    item: &ItemDefinition,
    charges: Option<cdda_content::ItemGroupChargesRange>,
    content: RuntimeItemGroupContent<'_>,
) -> Result<ItemGroupItemPrototypeV1, Box<dyn std::error::Error>> {
    let (charges, minimum_one_charge) = runtime_item_group_charges(item, charges)?;
    let prototype = craft_item_prototype(item, default_instance_charges(item), content.items)?;
    validate_item_group_item_spawn(item, &prototype, false)?;
    let modifier_side_effects_supported =
        validate_item_group_item_spawn(item, &prototype, true).is_ok();
    let tool_charge_storage = runtime_tool_charge_storage(item, &prototype, content)?;
    let charges_supported = if item.subtypes.contains("TOOL") {
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
        snippets: item
            .snippets
            .iter()
            .map(|snippet| ItemSnippetV1 {
                id: snippet.id.clone(),
                text: snippet.text.clone(),
            })
            .collect(),
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
        modifier_side_effects_supported,
        charges,
        minimum_one_charge,
        tool_charge_storage,
        charges_supported,
        modifier_container_capacity_applies: matches!(item.phase.as_str(), "LIQUID" | "liquid")
            || !item
                .subtypes
                .iter()
                .any(|subtype| matches!(subtype.as_str(), "TOOL" | "GUN" | "MAGAZINE")),
        contents_insertion_supported,
    })
}

fn runtime_tool_charge_storage(
    item: &ItemDefinition,
    prototype: &CraftItemPrototypeV1,
    content: RuntimeItemGroupContent<'_>,
) -> Result<Option<ItemGroupToolChargeStorageV1>, Box<dyn std::error::Error>> {
    if !item.subtypes.contains("TOOL") {
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
                "item-group tool {} references missing default magazine {}",
                item.id, raw_well.default_magazine
            )
        })?;
    let magazine_shape = magazine_definition.strict_magazine().ok_or_else(|| {
        format!(
            "item-group tool {} default magazine {} is not a strict single-pocket magazine",
            item.id, magazine_definition.id
        )
    })?;
    if well
        .compatible_magazine_type_ids
        .binary_search(&magazine_definition.id)
        .is_err()
    {
        return Err(format!(
            "item-group tool {} default magazine {} is incompatible with well {}",
            item.id, magazine_definition.id, well.pocket_index
        )
        .into());
    }
    validate_charge_item_constructor_state(magazine_definition)?;
    let magazine = craft_item_prototype(magazine_definition, 0, content.items)?;
    validate_item_group_item_spawn(magazine_definition, &magazine, false)?;
    let ammunition =
        runtime_default_ammunition_prototype(item, &magazine_shape.ammunition_type, content)?;
    Ok(Some(ItemGroupToolChargeStorageV1::Detachable {
        well_pocket_index: well.pocket_index,
        magazine,
        ammunition: Box::new(ammunition),
    }))
}

fn runtime_default_ammunition_prototype(
    tool: &ItemDefinition,
    ammunition_type: &str,
    content: RuntimeItemGroupContent<'_>,
) -> Result<CraftItemPrototypeV1, Box<dyn std::error::Error>> {
    let default_id = &content
        .ammunition
        .get(ammunition_type)
        .ok_or_else(|| {
            format!(
                "item-group tool {} references missing ammunition type {ammunition_type}",
                tool.id
            )
        })?
        .default_item;
    let definition = content.items.get(default_id).ok_or_else(|| {
        format!(
            "item-group tool {} has no concrete default ammunition {} for {ammunition_type}",
            tool.id, default_id
        )
    })?;
    validate_charge_item_constructor_state(definition)?;
    let mut ammunition = craft_item_prototype(definition, 1, content.items)?;
    validate_item_group_item_spawn(definition, &ammunition, false)?;
    if ammunition.ammunition_type != ammunition_type {
        return Err(format!(
            "item-group tool {} default ammunition {} does not match {ammunition_type}",
            tool.id, definition.id
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
        || !item.variables.is_empty()
    {
        return Err(format!(
            "tool-charge item {} has constructor variant, snippet, or variable state that is not represented by ItemGroupToolChargeStorageV1",
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
    let item = runtime_item_group_item(item, None, content)?;
    let physical_pockets = item
        .prototype
        .ammunition_containers
        .iter()
        .filter_map(|pocket| pocket.spawn_rules.as_ref())
        .filter(|rules| rules.kind == cdda_protocol::SpawnPocketKindV1::Container)
        .collect::<Vec<_>>();
    if physical_pockets.len() != 1 || !physical_pockets[0].rigid {
        return Err(
            "item-group wrappers require exactly one rigid physical container pocket".into(),
        );
    }
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
    if item.flags.contains("VARSIZE") {
        return Err(format!(
            "item group item {} requires unimplemented variable-size FIT state",
            item.id
        )
        .into());
    }
    if item.unsupported_fields.contains("container") {
        return Err(format!(
            "item group item {} requires unimplemented default containment",
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
    const CONSTRUCTOR_STATE_FLAGS: &[&str] = &[
        "COLLAPSE_CONTENTS",
        "ENERGY_SHIELD",
        "NANOFAB_TEMPLATE",
        "SPAWN_ACTIVE",
    ];
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
    if item.subtypes.contains("COMESTIBLE") && !item.flags.contains("NO_TEMP") {
        return Err(format!(
            "item group comestible {} requires unimplemented temperature state",
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
    if item.unsupported_fields.contains("snippet_category") && item.snippets.is_empty() {
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
) -> Result<(Option<InclusiveI32RangeV1>, bool), Box<dyn std::error::Error>> {
    let Some(charges) = charges else {
        return Ok((None, false));
    };
    if charges.minimum == -1 && charges.maximum == -1 {
        return Ok((None, false));
    }
    if charges.minimum < 0 || charges.maximum < 0 {
        return Err(format!(
            "item-group charges for {} require unimplemented capacity sentinels",
            item.id
        )
        .into());
    }
    let charges = cdda_content::ItemGroupChargesRange {
        minimum: charges.minimum.min(charges.maximum),
        maximum: charges.maximum,
    };
    if (item.subtypes.contains("GUN") || item.subtypes.contains("MAGAZINE"))
        || (item.subtypes.contains("TOOL") && !item_group_charges_supported(item))
    {
        return Err(format!(
            "item-group charges for {} require unimplemented ammunition loading",
            item.id
        )
        .into());
    }
    let liquid = matches!(item.phase.as_str(), "LIQUID" | "liquid");
    if !item_group_charges_supported(item) {
        if charges.minimum == charges.maximum {
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
        Some(InclusiveI32RangeV1 {
            minimum: charges.minimum,
            maximum: charges.maximum,
        }),
        item.count_by_charges() || liquid,
    ))
}

fn normalize_item_group_charges(
    charges: Option<cdda_content::ItemGroupChargesRange>,
) -> Result<Option<InclusiveI32RangeV1>, Box<dyn std::error::Error>> {
    let Some(charges) = charges else {
        return Ok(None);
    };
    if charges.minimum == -1 && charges.maximum == -1 {
        return Ok(None);
    }
    if charges.minimum < 0 || charges.maximum < 0 {
        return Err("nested item-group charges require unimplemented capacity sentinels".into());
    }
    Ok(Some(InclusiveI32RangeV1 {
        minimum: charges.minimum.min(charges.maximum),
        maximum: charges.maximum,
    }))
}

fn item_group_charges_supported(item: &ItemDefinition) -> bool {
    if item.count_by_charges() || matches!(item.phase.as_str(), "LIQUID" | "liquid") {
        return true;
    }
    if item.subtypes.contains("TOOL") {
        let integral = item
            .pockets
            .iter()
            .filter(|pocket| pocket.strict_integral_magazine().is_some())
            .count();
        let detachable = item
            .magazine_wells
            .iter()
            .filter(|well| !well.default_magazine.is_empty())
            .count();
        return (integral == 1 && detachable == 0) || (integral == 0 && detachable == 1);
    }
    item.flags.contains("CAN_HAVE_CHARGES")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_content::{ItemVariantDefinition, PocketDefinition};

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

        let mut variable = plain;
        variable.variables.insert(
            String::from("lot"),
            cdda_content::ItemVariableValueDefinition::Integer(7),
        );
        assert!(validate_charge_item_constructor_state(&variable).is_err());
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
