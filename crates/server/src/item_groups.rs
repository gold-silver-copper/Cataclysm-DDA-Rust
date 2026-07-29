use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    BashDefinition, BashItemGroupSource, ItemDefinition, ItemGroupEvent, ItemGroupRegistry,
    ItemGroupSubtype, ItemRegistry, PocketTypeDefinition, StrictItemGroupDefinition,
    StrictItemGroupGraph, StrictItemGroupNode, StrictItemGroupNodeKind,
};
use cdda_protocol::{
    CraftItemPrototypeV1, InclusiveI32RangeV1, InclusiveU16RangeV1, ItemGroupDefinitionV1,
    ItemGroupEntryV1, ItemGroupEventV1, ItemGroupGraphV1, ItemGroupItemPrototypeV1,
    ItemGroupKindV1, ItemGroupNodeV1, ItemGroupSourceV1, ItemGroupTargetV1,
    item_group_catalog_is_valid, item_group_source_max_outputs,
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
        return Err("reachable bash item-group catalog is invalid for the current protocol".into());
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
            "item-group closure rooted at {} is invalid for the current protocol",
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
    if definition.wrapper.is_some() {
        return Err(format!(
            "item group {} requires unimplemented group containment",
            definition.id
        )
        .into());
    }
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
    if node.direct_wrapper.is_some() || node.modifier_container.is_some() {
        return Err(format!(
            "item group {} requires unimplemented entry containment",
            definition.id
        )
        .into());
    }
    if node.variant.is_some() {
        return Err(format!(
            "item group {} requires unimplemented item variants",
            definition.id
        )
        .into());
    }
    if node.modifier_sealed.is_some() {
        return Err(format!(
            "item group {} requires unimplemented modifier sealing",
            definition.id
        )
        .into());
    }
    let raw_damage = node
        .damage
        .map(
            |damage| -> Result<InclusiveU16RangeV1, Box<dyn std::error::Error>> {
                if damage.minimum != 0 || damage.maximum != 0 {
                    return Err(format!(
                        "item group {} requires unimplemented raw-damage modifiers",
                        definition.id
                    )
                    .into());
                }
                Ok(InclusiveU16RangeV1 {
                    minimum: 0,
                    maximum: 0,
                })
            },
        )
        .transpose()?;
    let target = match &node.kind {
        StrictItemGroupNodeKind::Item(item_id) => {
            let item = items.get(item_id).ok_or_else(|| {
                format!(
                    "item group {} references missing concrete item {item_id}",
                    definition.id
                )
            })?;
            let (charges, minimum_one_charge) = runtime_item_group_charges(item, node.charges)?;
            let prototype = craft_item_prototype(item, default_instance_charges(item), items)?;
            validate_item_group_item_spawn(item, &prototype, raw_damage.is_some())?;
            ItemGroupTargetV1::Item(Box::new(ItemGroupItemPrototypeV1 {
                prototype,
                charges,
                minimum_one_charge,
            }))
        }
        StrictItemGroupNodeKind::Group(group_id) => {
            if raw_damage.is_some() {
                return Err(format!(
                    "item group {} applies a modifier to nested group {group_id}; recursive modifier-side-effect admission is not implemented",
                    definition.id
                )
                .into());
            }
            if node.charges.is_some() {
                return Err(format!(
                    "item group {} applies charges to nested group {group_id}, which the current protocol cannot represent",
                    definition.id
                )
                .into());
            }
            ItemGroupTargetV1::Group(group_id.clone())
        }
        StrictItemGroupNodeKind::Collection(_) | StrictItemGroupNodeKind::Distribution(_) => {
            if node.charges.is_some() {
                return Err(format!(
                    "item group {} applies charges to a local nested group, which the current protocol cannot represent",
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
        event: node.event.map(runtime_item_group_event),
        target,
    })
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
    const CONSTRUCTOR_STATE_FIELDS: &[&str] = &["countdown_interval", "variables", "relic_data"];
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
        "variants",
        "nanofab_template_group",
        "trait_group",
        "built_in_mods",
        "default_mods",
        "snippet_category",
        "expand_snippets",
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
            minimum: charges.minimum,
            maximum: charges.maximum,
        }),
        item.count_by_charges() || liquid,
    ))
}
