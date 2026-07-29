use std::collections::BTreeMap;

use cdda_protocol::{
    CraftItemPrototypeV1, ItemGroupDefinitionV1, ItemGroupEntryV1, ItemGroupGraphV1,
    ItemGroupKindV1, ItemGroupSourceV1, ItemGroupTargetV1, MAX_ITEM_GROUP_DEPTH,
    MAX_ITEM_GROUP_OUTPUTS,
};
use rand_chacha::ChaCha8Rng;
use rand_core::Rng;

use super::{SimError, inclusive_rng_u64, validate_craft_item_prototype};

pub(super) fn plan_item_group_source(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<CraftItemPrototypeV1>, SimError> {
    let mut output = Vec::new();
    plan_item_group_source_into(source, item_groups, rng, &mut output, 0)?;
    Ok(output)
}

fn plan_item_group_source_into(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    let graph = match source {
        ItemGroupSourceV1::Group(group_id) => {
            &item_groups
                .get(group_id)
                .ok_or(SimError::InvalidItem)?
                .graph
        }
        ItemGroupSourceV1::Inline(graph) => graph,
    };
    plan_item_group_node(graph, graph.root_node, item_groups, rng, output, depth)
}

fn plan_item_group_node(
    graph: &ItemGroupGraphV1,
    node_id: u16,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    if depth > MAX_ITEM_GROUP_DEPTH {
        return Err(SimError::InvalidItem);
    }
    let node = graph
        .nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .ok_or(SimError::InvalidItem)?;
    match node.kind {
        ItemGroupKindV1::Collection => {
            for entry in &node.entries {
                // The pinned implementation rolls even for guaranteed entries.
                if rng.next_u64() % 100 < u64::from(entry.probability) {
                    plan_item_group_entry(graph, entry, item_groups, rng, output, depth)?;
                }
            }
        }
        ItemGroupKindV1::Distribution => {
            let total = node.entries.iter().try_fold(0_u64, |total, entry| {
                total.checked_add(u64::from(entry.probability))
            });
            let Some(total) = total.filter(|total| *total > 0) else {
                return Ok(());
            };
            let ticket = inclusive_rng_u64(rng, 1, total);
            let mut accumulated = 0_u64;
            let entry = node
                .entries
                .iter()
                .find(|entry| {
                    accumulated = accumulated.saturating_add(u64::from(entry.probability));
                    ticket <= accumulated
                })
                .ok_or(SimError::InvalidItem)?;
            plan_item_group_entry(graph, entry, item_groups, rng, output, depth)?;
        }
    }
    Ok(())
}

fn plan_item_group_entry(
    graph: &ItemGroupGraphV1,
    entry: &ItemGroupEntryV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    let count = if entry.count_min == entry.count_max {
        u64::from(entry.count_min)
    } else {
        inclusive_rng_u64(rng, u64::from(entry.count_min), u64::from(entry.count_max))
    };
    for _ in 0..count {
        plan_item_group_target(
            graph,
            &entry.target,
            item_groups,
            rng,
            output,
            depth.checked_add(1).ok_or(SimError::NumericOverflow)?,
        )?;
    }
    Ok(())
}

fn plan_item_group_target(
    graph: &ItemGroupGraphV1,
    target: &ItemGroupTargetV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    if depth > MAX_ITEM_GROUP_DEPTH {
        return Err(SimError::InvalidItem);
    }
    match target {
        ItemGroupTargetV1::Item(item) => {
            let mut prototype = item.prototype.clone();
            if let Some(charges) = item.charges {
                let rolled = if charges.minimum == charges.maximum {
                    charges.minimum
                } else {
                    i32::try_from(inclusive_rng_u64(
                        rng,
                        u64::try_from(charges.minimum).map_err(|_| SimError::InvalidItem)?,
                        u64::try_from(charges.maximum).map_err(|_| SimError::InvalidItem)?,
                    ))
                    .map_err(|_| SimError::NumericOverflow)?
                };
                prototype.charges = if item.minimum_one_charge {
                    rolled.max(1)
                } else {
                    rolled
                };
            }
            if output.len()
                >= usize::try_from(MAX_ITEM_GROUP_OUTPUTS).map_err(|_| SimError::NumericOverflow)?
            {
                return Err(SimError::InvalidItem);
            }
            if validate_craft_item_prototype(&prototype).is_err() {
                return Err(SimError::InvalidItem);
            }
            output.push(prototype);
        }
        ItemGroupTargetV1::Group(group_id) => plan_item_group_source_into(
            &ItemGroupSourceV1::Group(group_id.clone()),
            item_groups,
            rng,
            output,
            depth,
        )?,
        ItemGroupTargetV1::Node(node_id) => {
            plan_item_group_node(graph, *node_id, item_groups, rng, output, depth)?;
        }
    }
    Ok(())
}
