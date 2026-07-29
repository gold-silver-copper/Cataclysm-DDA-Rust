use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    BashDefinition, BashItemGroupSource, ItemDefinition, ItemGroupEvent, ItemGroupRegistry,
    ItemGroupSubtype, ItemRegistry, StrictItemGroupDefinition, StrictItemGroupGraph,
    StrictItemGroupNode, StrictItemGroupNodeKind,
};
use cdda_protocol::{
    InclusiveI32RangeV1, ItemGroupDefinitionV1, ItemGroupEntryV1, ItemGroupEventV1,
    ItemGroupGraphV1, ItemGroupItemPrototypeV1, ItemGroupKindV1, ItemGroupNodeV1,
    ItemGroupSourceV1, ItemGroupTargetV1, item_group_catalog_is_valid,
    item_group_source_max_outputs,
};

use super::{craft_item_prototype, default_instance_charges};

pub(super) fn runtime_bash_item_group_source(
    bash: &BashDefinition,
    item_groups: &ItemGroupRegistry,
    items: &ItemRegistry,
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
    let catalog = runtime_strict_item_group_catalog(&graph, items)?;
    let source = match bash.item_group.as_ref() {
        Some(BashItemGroupSource::Named(group_id)) => ItemGroupSourceV1::Group(group_id.clone()),
        Some(BashItemGroupSource::InlineCollection(_)) => {
            ItemGroupSourceV1::Inline(runtime_item_group_graph(&graph.root, items)?)
        }
        None => return Err("bash item-group source disappeared during normalization".into()),
    };
    let maximum_output = item_group_source_max_outputs(&source, &catalog).ok_or_else(|| {
        format!("{owner_kind} {owner_id} produced an invalid Protocol 80 item-group graph")
    })?;
    if maximum_output > cdda_sim::ID_RESERVATION_SIZE {
        return Err(format!(
            "{owner_kind} {owner_id} Protocol 80 item group can generate {maximum_output} objects, exceeding the stable-ID reservation"
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
    items: &ItemRegistry,
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
        .map(|definition| runtime_item_group_definition(definition, items))
        .collect::<Result<Vec<_>, _>>()?;
    if !item_group_catalog_is_valid(&catalog) {
        return Err("reachable bash item-group catalog is invalid for Protocol 80".into());
    }
    Ok(catalog)
}

fn runtime_strict_item_group_catalog(
    graph: &StrictItemGroupGraph,
    items: &ItemRegistry,
) -> Result<Vec<ItemGroupDefinitionV1>, Box<dyn std::error::Error>> {
    let catalog = graph
        .groups
        .values()
        .map(|definition| runtime_item_group_definition(definition, items))
        .collect::<Result<Vec<_>, _>>()?;
    if !item_group_catalog_is_valid(&catalog) {
        return Err(format!(
            "item-group closure rooted at {} is invalid for Protocol 80",
            graph.root.id
        )
        .into());
    }
    Ok(catalog)
}

fn runtime_item_group_definition(
    definition: &StrictItemGroupDefinition,
    items: &ItemRegistry,
) -> Result<ItemGroupDefinitionV1, Box<dyn std::error::Error>> {
    Ok(ItemGroupDefinitionV1 {
        group_id: definition.id.clone(),
        graph: runtime_item_group_graph(definition, items)?,
    })
}

pub(super) fn runtime_item_group_graph(
    definition: &StrictItemGroupDefinition,
    items: &ItemRegistry,
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
            .map(|node_id| runtime_item_group_entry(definition, *node_id, &composite_ids, items))
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
                    runtime_item_group_entry(definition, *node_id, &composite_ids, items)
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
        });
    }
    Ok(ItemGroupGraphV1 {
        root_node: 0,
        nodes,
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
    items: &ItemRegistry,
) -> Result<ItemGroupEntryV1, Box<dyn std::error::Error>> {
    let node = strict_item_group_node(definition, node_id)?;
    let target = match &node.kind {
        StrictItemGroupNodeKind::Item(item_id) => {
            let item = items.get(item_id).ok_or_else(|| {
                format!(
                    "item group {} references missing concrete item {item_id}",
                    definition.id
                )
            })?;
            let (charges, minimum_one_charge) = runtime_item_group_charges(item, node.charges)?;
            ItemGroupTargetV1::Item(Box::new(ItemGroupItemPrototypeV1 {
                prototype: craft_item_prototype(item, default_instance_charges(item), items)?,
                charges,
                minimum_one_charge,
            }))
        }
        StrictItemGroupNodeKind::Group(group_id) => {
            if node.charges.is_some() {
                return Err(format!(
                    "item group {} applies charges to nested group {group_id}, which Protocol 80 cannot represent",
                    definition.id
                )
                .into());
            }
            ItemGroupTargetV1::Group(group_id.clone())
        }
        StrictItemGroupNodeKind::Collection(_) | StrictItemGroupNodeKind::Distribution(_) => {
            if node.charges.is_some() {
                return Err(format!(
                    "item group {} applies charges to a local nested group, which Protocol 80 cannot represent",
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
        event: node.event.map(runtime_item_group_event),
        target,
    })
}

pub(super) fn runtime_item_group_charges(
    item: &ItemDefinition,
    charges: Option<cdda_content::ItemGroupRange>,
) -> Result<(Option<InclusiveI32RangeV1>, bool), Box<dyn std::error::Error>> {
    let Some(charges) = charges else {
        return Ok((None, false));
    };
    if item.subtypes.contains("TOOL")
        || item.subtypes.contains("GUN")
        || item.subtypes.contains("MAGAZINE")
    {
        return Err(format!(
            "item-group charges for {} require unimplemented ammunition loading",
            item.id
        )
        .into());
    }
    let liquid = matches!(item.phase.as_str(), "LIQUID" | "liquid");
    if !item.count_by_charges() && !liquid && !item.flags.contains("CAN_HAVE_CHARGES") {
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
            minimum: i32::try_from(charges.minimum)?,
            maximum: i32::try_from(charges.maximum)?,
        }),
        item.count_by_charges() || liquid,
    ))
}
