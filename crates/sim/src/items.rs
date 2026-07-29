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
                // Pinned collection generation rolls even for guaranteed and
                // inactive event entries. The server fixes EVENT_SPAWNS to its
                // upstream default `off`, so a qualified entry never spawns.
                let roll = rng.next_u64() % 100;
                if entry.event.is_none() && roll < u64::from(entry.probability) {
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
            // Distribution tickets retain the original event entry weight.
            // Under the deterministic disabled policy, landing on one yields
            // no item instead of selecting another entry.
            if entry.event.is_some() {
                return Ok(());
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_protocol::{ItemGroupEventV1, ItemGroupItemPrototypeV1, ItemGroupNodeV1};
    use rand_core::SeedableRng;

    fn leaf(type_id: &str) -> ItemGroupTargetV1 {
        ItemGroupTargetV1::Item(Box::new(ItemGroupItemPrototypeV1 {
            prototype: CraftItemPrototypeV1 {
                type_id: type_id.to_owned(),
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
            },
            charges: None,
            minimum_one_charge: false,
        }))
    }

    fn entry(probability: u32, event: Option<ItemGroupEventV1>, type_id: &str) -> ItemGroupEntryV1 {
        ItemGroupEntryV1 {
            probability,
            count_min: 1,
            count_max: 1,
            event,
            target: leaf(type_id),
        }
    }

    #[test]
    fn disabled_event_collection_still_consumes_its_probability_roll() {
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![
                    entry(100, Some(ItemGroupEventV1::Christmas), "holiday_token"),
                    entry(100, None, "ordinary"),
                ],
            }],
        });
        let mut actual_rng = ChaCha8Rng::seed_from_u64(19);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("valid event collection should plan");
        assert_eq!(
            planned
                .iter()
                .map(|prototype| prototype.type_id.as_str())
                .collect::<Vec<_>>(),
            ["ordinary"]
        );

        let mut expected_rng = ChaCha8Rng::seed_from_u64(19);
        let _ = expected_rng.next_u64();
        let _ = expected_rng.next_u64();
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn disabled_event_distribution_retains_empty_ticket_intervals() {
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Distribution,
                entries: vec![
                    entry(3, Some(ItemGroupEventV1::Halloween), "holiday_token"),
                    entry(2, None, "ordinary"),
                ],
            }],
        });
        for ticket in 1..=5 {
            let seed = (0..100_000)
                .find(|seed| {
                    let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                    inclusive_rng_u64(&mut rng, 1, 5) == ticket
                })
                .expect("every bounded ticket should have a witness seed");
            let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
            let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
                .expect("valid event distribution should plan");
            assert_eq!(
                planned
                    .iter()
                    .map(|prototype| prototype.type_id.as_str())
                    .collect::<Vec<_>>(),
                if ticket <= 3 {
                    Vec::<&str>::new()
                } else {
                    vec!["ordinary"]
                },
                "ticket {ticket} must retain its pinned interval"
            );

            let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
            assert_eq!(inclusive_rng_u64(&mut expected_rng, 1, 5), ticket);
            assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
        }
    }
}
