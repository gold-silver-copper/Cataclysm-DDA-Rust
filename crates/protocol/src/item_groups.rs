//! Canonical item-group wire DTOs, bounds, and graph validation.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{CraftItemPrototypeV1, valid_craft_item_prototype, valid_recipe_id};

/// Maximum number of named item groups retained by one canonical world.
/// Runtime catalogs contain only the transitive closure used by canonical
/// consumers rather than every group present in the selected content set.
pub const MAX_ITEM_GROUP_DEFINITIONS: usize = 512;
/// Maximum number of flat local nodes across the canonical named catalog and
/// every inline consumer graph retained by one world.
pub const MAX_ITEM_GROUP_NODES: usize = 2_048;
/// Maximum number of entries across the canonical named catalog and every
/// inline consumer graph retained by one world.
pub const MAX_ITEM_GROUP_ENTRIES: usize = 8_192;
/// Maximum number of local-node and named-group edges on a generated path.
pub const MAX_ITEM_GROUP_DEPTH: usize = 32;
/// One item-group invocation cannot require more than one reserved ID block.
pub const MAX_ITEM_GROUP_OUTPUTS: u64 = 4_096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupKindV1 {
    Collection,
    Distribution,
}

/// Pinned real-world holiday qualifier on an item-group entry. Authoritative
/// server policy decides whether the qualifier is active; inactive
/// distribution entries retain their weight and can deliberately yield no
/// output.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupEventV1 {
    NewYear,
    Easter,
    IndependenceDay,
    Halloween,
    Thanksgiving,
    Christmas,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InclusiveI32RangeV1 {
    pub minimum: i32,
    pub maximum: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InclusiveU16RangeV1 {
    pub minimum: u16,
    pub maximum: u16,
}

/// A direct item leaf in a normalized item-group graph. The server resolves
/// immutable content into the same prototype used by crafting before the
/// graph enters canonical simulation state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupItemPrototypeV1 {
    pub prototype: CraftItemPrototypeV1,
    pub charges: Option<InclusiveI32RangeV1>,
    /// Count-by-charge items clamp an explicit zero charge roll to one in the
    /// pinned implementation. Non-charge items leave this false.
    pub minimum_one_charge: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupTargetV1 {
    Item(Box<ItemGroupItemPrototypeV1>),
    Group(String),
    /// Reference to a node in the containing graph.
    Node(u16),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupEntryV1 {
    /// Collection entries use a normalized percentage in 1..=100;
    /// distribution entries use an arbitrary positive weight.
    pub probability: u32,
    pub count_min: u16,
    pub count_max: u16,
    /// Presence preserves upstream `Item_modifier` construction even for the
    /// fixed-zero damage range, whose RNG evaluation precedes charge dressing.
    /// Protocol 85 admits only `0..=0`; nonzero raw damage remains fail-closed
    /// until item snapshots retain the exact raw value.
    pub raw_damage: Option<InclusiveU16RangeV1>,
    pub event: Option<ItemGroupEventV1>,
    pub target: ItemGroupTargetV1,
}

/// One node in a flat local item-group graph. Nodes are ID-sorted in their
/// containing graph while each node's entries retain source order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupNodeV1 {
    pub node_id: u16,
    pub kind: ItemGroupKindV1,
    pub entries: Vec<ItemGroupEntryV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupGraphV1 {
    pub root_node: u16,
    pub nodes: Vec<ItemGroupNodeV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupDefinitionV1 {
    pub group_id: String,
    pub graph: ItemGroupGraphV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupSourceV1 {
    Group(String),
    Inline(ItemGroupGraphV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ItemGroupMetrics {
    outputs: u64,
    depth: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemGroupEvaluationState {
    Visiting,
    Complete(ItemGroupMetrics),
}

struct ItemGroupEvaluator<'a> {
    definitions: &'a [ItemGroupDefinitionV1],
    lookup: BTreeMap<&'a str, usize>,
    group_states: Vec<Option<ItemGroupEvaluationState>>,
    node_states: BTreeMap<(usize, u16), ItemGroupEvaluationState>,
}

impl<'a> ItemGroupEvaluator<'a> {
    fn new(definitions: &'a [ItemGroupDefinitionV1]) -> Option<Self> {
        if definitions.len() > MAX_ITEM_GROUP_DEFINITIONS
            || definitions
                .windows(2)
                .any(|pair| pair[0].group_id >= pair[1].group_id)
        {
            return None;
        }
        let total_nodes = definitions.iter().try_fold(0_usize, |total, definition| {
            total.checked_add(definition.graph.nodes.len())
        })?;
        let total_entries = definitions.iter().try_fold(0_usize, |total, definition| {
            definition
                .graph
                .nodes
                .iter()
                .try_fold(total, |total, node| total.checked_add(node.entries.len()))
        })?;
        if total_nodes > MAX_ITEM_GROUP_NODES || total_entries > MAX_ITEM_GROUP_ENTRIES {
            return None;
        }
        let mut lookup = BTreeMap::new();
        for (index, definition) in definitions.iter().enumerate() {
            if !valid_recipe_id(&definition.group_id)
                || !valid_item_group_graph_shape(&definition.graph)
                || lookup.insert(definition.group_id.as_str(), index).is_some()
            {
                return None;
            }
        }
        Some(Self {
            definitions,
            lookup,
            group_states: vec![None; definitions.len()],
            node_states: BTreeMap::new(),
        })
    }

    fn validate_catalog(&mut self) -> bool {
        (0..self.definitions.len()).all(|index| self.evaluate_group(index, 0).is_some())
    }

    fn evaluate_group(&mut self, index: usize, recursion_depth: usize) -> Option<ItemGroupMetrics> {
        if recursion_depth > MAX_ITEM_GROUP_DEPTH {
            return None;
        }
        match self.group_states.get(index).copied().flatten() {
            Some(ItemGroupEvaluationState::Visiting) => return None,
            Some(ItemGroupEvaluationState::Complete(metrics)) => return Some(metrics),
            None => {}
        }
        *self.group_states.get_mut(index)? = Some(ItemGroupEvaluationState::Visiting);
        let root_node = self.definitions.get(index)?.graph.root_node;
        let metrics = self.evaluate_named_node(index, root_node, recursion_depth)?;
        if metrics.outputs > MAX_ITEM_GROUP_OUTPUTS || metrics.depth > MAX_ITEM_GROUP_DEPTH {
            return None;
        }
        *self.group_states.get_mut(index)? = Some(ItemGroupEvaluationState::Complete(metrics));
        Some(metrics)
    }

    fn evaluate_named_node(
        &mut self,
        owner: usize,
        node_id: u16,
        recursion_depth: usize,
    ) -> Option<ItemGroupMetrics> {
        if recursion_depth > MAX_ITEM_GROUP_DEPTH {
            return None;
        }
        let key = (owner, node_id);
        match self.node_states.get(&key).copied() {
            Some(ItemGroupEvaluationState::Visiting) => return None,
            Some(ItemGroupEvaluationState::Complete(metrics)) => return Some(metrics),
            None => {}
        }
        self.node_states
            .insert(key, ItemGroupEvaluationState::Visiting);
        let node = self
            .definitions
            .get(owner)?
            .graph
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)?
            .clone();
        let metrics =
            self.evaluate_entries(node.kind, &node.entries, |evaluator, target| match target {
                ItemGroupTargetV1::Item(_) => Some(ItemGroupMetrics {
                    outputs: 1,
                    depth: 0,
                }),
                ItemGroupTargetV1::Group(group_id) => {
                    let index = *evaluator.lookup.get(group_id.as_str())?;
                    evaluator.evaluate_group(index, recursion_depth.checked_add(1)?)
                }
                ItemGroupTargetV1::Node(node_id) => {
                    evaluator.evaluate_named_node(owner, *node_id, recursion_depth.checked_add(1)?)
                }
            })?;
        self.node_states
            .insert(key, ItemGroupEvaluationState::Complete(metrics));
        Some(metrics)
    }

    fn evaluate_inline_graph(&mut self, graph: &ItemGroupGraphV1) -> Option<ItemGroupMetrics> {
        if !valid_item_group_graph_shape(graph) {
            return None;
        }
        let mut states = BTreeMap::new();
        let metrics = self.evaluate_inline_node(graph, graph.root_node, &mut states, 0)?;
        (metrics.outputs <= MAX_ITEM_GROUP_OUTPUTS && metrics.depth <= MAX_ITEM_GROUP_DEPTH)
            .then_some(metrics)
    }

    fn evaluate_inline_node(
        &mut self,
        graph: &ItemGroupGraphV1,
        node_id: u16,
        states: &mut BTreeMap<u16, ItemGroupEvaluationState>,
        recursion_depth: usize,
    ) -> Option<ItemGroupMetrics> {
        if recursion_depth > MAX_ITEM_GROUP_DEPTH {
            return None;
        }
        match states.get(&node_id).copied() {
            Some(ItemGroupEvaluationState::Visiting) => return None,
            Some(ItemGroupEvaluationState::Complete(metrics)) => return Some(metrics),
            None => {}
        }
        states.insert(node_id, ItemGroupEvaluationState::Visiting);
        let node = graph
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)?
            .clone();
        let metrics =
            self.evaluate_entries(node.kind, &node.entries, |evaluator, target| match target {
                ItemGroupTargetV1::Item(_) => Some(ItemGroupMetrics {
                    outputs: 1,
                    depth: 0,
                }),
                ItemGroupTargetV1::Group(group_id) => {
                    let index = *evaluator.lookup.get(group_id.as_str())?;
                    evaluator.evaluate_group(index, recursion_depth.checked_add(1)?)
                }
                ItemGroupTargetV1::Node(node_id) => evaluator.evaluate_inline_node(
                    graph,
                    *node_id,
                    states,
                    recursion_depth.checked_add(1)?,
                ),
            })?;
        states.insert(node_id, ItemGroupEvaluationState::Complete(metrics));
        Some(metrics)
    }

    fn evaluate_entries(
        &mut self,
        kind: ItemGroupKindV1,
        entries: &[ItemGroupEntryV1],
        mut target_metrics: impl FnMut(&mut Self, &ItemGroupTargetV1) -> Option<ItemGroupMetrics>,
    ) -> Option<ItemGroupMetrics> {
        let mut outputs = 0_u64;
        let mut depth = 0_usize;
        for entry in entries {
            let target = target_metrics(self, &entry.target)?;
            let entry_outputs = target.outputs.checked_mul(u64::from(entry.count_max))?;
            let entry_depth = target.depth.checked_add(1)?;
            if entry_outputs > MAX_ITEM_GROUP_OUTPUTS || entry_depth > MAX_ITEM_GROUP_DEPTH {
                return None;
            }
            match kind {
                ItemGroupKindV1::Collection => {
                    outputs = outputs.checked_add(entry_outputs)?;
                }
                ItemGroupKindV1::Distribution => outputs = outputs.max(entry_outputs),
            }
            if outputs > MAX_ITEM_GROUP_OUTPUTS {
                return None;
            }
            depth = depth.max(entry_depth);
        }
        Some(ItemGroupMetrics { outputs, depth })
    }

    fn evaluate_source(&mut self, source: &ItemGroupSourceV1) -> Option<ItemGroupMetrics> {
        match source {
            ItemGroupSourceV1::Group(group_id) => {
                let index = *self.lookup.get(group_id.as_str())?;
                self.evaluate_group(index, 0)
            }
            ItemGroupSourceV1::Inline(graph) => self.evaluate_inline_graph(graph),
        }
    }
}

fn valid_item_group_graph_shape(graph: &ItemGroupGraphV1) -> bool {
    if graph.nodes.is_empty()
        || graph.nodes.len() > MAX_ITEM_GROUP_NODES
        || graph
            .nodes
            .windows(2)
            .any(|pair| pair[0].node_id >= pair[1].node_id)
        || !graph
            .nodes
            .iter()
            .any(|node| node.node_id == graph.root_node)
    {
        return false;
    }
    let entry_count = graph
        .nodes
        .iter()
        .try_fold(0_usize, |total, node| total.checked_add(node.entries.len()));
    if entry_count.is_none_or(|count| count > MAX_ITEM_GROUP_ENTRIES) {
        return false;
    }
    let node_ids = graph
        .nodes
        .iter()
        .map(|node| node.node_id)
        .collect::<BTreeSet<_>>();
    if graph.nodes.iter().any(|node| {
        let weight_sum = node
            .entries
            .iter()
            .try_fold(0_u32, |total, entry| total.checked_add(entry.probability));
        node.entries.iter().any(|entry| {
            let modifier_requires_marker = entry.count_min != 1
                || entry.count_max != 1
                || matches!(
                    &entry.target,
                    ItemGroupTargetV1::Item(item) if item.charges.is_some()
                );
            entry.probability == 0
                || (node.kind == ItemGroupKindV1::Collection && entry.probability > 100)
                || entry.count_min > entry.count_max
                || (modifier_requires_marker && entry.raw_damage.is_none())
                || matches!(
                    &entry.target,
                    ItemGroupTargetV1::Item(item) if item.prototype.type_id == "null"
                )
                || entry
                    .raw_damage
                    .is_some_and(|damage| damage.minimum != 0 || damage.maximum != 0)
                || (entry.raw_damage.is_some()
                    && !matches!(&entry.target, ItemGroupTargetV1::Item(_)))
                || !valid_item_group_target(&entry.target, &node_ids)
        }) || (node.kind == ItemGroupKindV1::Distribution && weight_sum.is_none())
    }) {
        return false;
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![graph.root_node];
    while let Some(node_id) = pending.pop() {
        if !reachable.insert(node_id) {
            continue;
        }
        let Some(node) = graph.nodes.iter().find(|node| node.node_id == node_id) else {
            return false;
        };
        pending.extend(node.entries.iter().filter_map(|entry| match entry.target {
            ItemGroupTargetV1::Node(node_id) => Some(node_id),
            ItemGroupTargetV1::Item(_) | ItemGroupTargetV1::Group(_) => None,
        }));
    }
    reachable.len() == graph.nodes.len()
}

fn valid_item_group_target(target: &ItemGroupTargetV1, node_ids: &BTreeSet<u16>) -> bool {
    match target {
        ItemGroupTargetV1::Item(item) => {
            valid_craft_item_prototype(&item.prototype)
                && item.charges.is_none_or(|charges| {
                    if charges.minimum < 0 || charges.maximum < charges.minimum {
                        return false;
                    }
                    [charges.minimum, charges.maximum].into_iter().all(|value| {
                        let mut effective = item.prototype.clone();
                        effective.charges = if item.minimum_one_charge {
                            value.max(1)
                        } else {
                            value
                        };
                        valid_craft_item_prototype(&effective)
                    })
                })
                && (!item.minimum_one_charge || item.charges.is_some())
        }
        ItemGroupTargetV1::Group(group_id) => valid_recipe_id(group_id),
        ItemGroupTargetV1::Node(node_id) => node_ids.contains(node_id),
    }
}

/// Validates the complete named group catalog, including local and global
/// references, cycles, depth, weight sums, direct prototypes, and maximum
/// generated output counts.
#[must_use]
pub fn item_group_catalog_is_valid(definitions: &[ItemGroupDefinitionV1]) -> bool {
    item_group_sources_are_valid(definitions, std::iter::empty())
}

/// Validates a named catalog and all retained consumer sources against one
/// aggregate graph budget. Repeated named references do not duplicate storage;
/// each inline graph does.
#[must_use]
pub fn item_group_sources_are_valid<'a>(
    definitions: &[ItemGroupDefinitionV1],
    sources: impl IntoIterator<Item = &'a ItemGroupSourceV1>,
) -> bool {
    let Some(mut evaluator) = ItemGroupEvaluator::new(definitions) else {
        return false;
    };
    if !evaluator.validate_catalog() {
        return false;
    }
    let Some(mut total_nodes) = definitions.iter().try_fold(0_usize, |total, definition| {
        total.checked_add(definition.graph.nodes.len())
    }) else {
        return false;
    };
    let Some(mut total_entries) = definitions.iter().try_fold(0_usize, |total, definition| {
        definition
            .graph
            .nodes
            .iter()
            .try_fold(total, |total, node| total.checked_add(node.entries.len()))
    }) else {
        return false;
    };
    for source in sources {
        if let ItemGroupSourceV1::Inline(graph) = source {
            let Some(nodes) = total_nodes.checked_add(graph.nodes.len()) else {
                return false;
            };
            let Some(entries) = graph.nodes.iter().try_fold(total_entries, |total, node| {
                total.checked_add(node.entries.len())
            }) else {
                return false;
            };
            total_nodes = nodes;
            total_entries = entries;
            if total_nodes > MAX_ITEM_GROUP_NODES || total_entries > MAX_ITEM_GROUP_ENTRIES {
                return false;
            }
        }
        if evaluator.evaluate_source(source).is_none() {
            return false;
        }
    }
    true
}

/// Returns the maximum number of stable top-level item objects produced by a
/// source when both the source and its named catalog are valid.
#[must_use]
pub fn item_group_source_max_outputs(
    source: &ItemGroupSourceV1,
    definitions: &[ItemGroupDefinitionV1],
) -> Option<u64> {
    item_group_sources_are_valid(definitions, std::iter::once(source)).then_some(())?;
    let mut evaluator = ItemGroupEvaluator::new(definitions)?;
    Some(evaluator.evaluate_source(source)?.outputs)
}

pub(super) fn item_group_sources_have_exact_named_closure(
    definitions: &[ItemGroupDefinitionV1],
    sources: &[&ItemGroupSourceV1],
) -> bool {
    let lookup = definitions
        .iter()
        .map(|definition| (definition.group_id.as_str(), definition))
        .collect::<BTreeMap<_, _>>();
    let mut pending = Vec::new();
    for source in sources {
        match source {
            ItemGroupSourceV1::Group(group_id) => pending.push(group_id.as_str()),
            ItemGroupSourceV1::Inline(graph) => pending.extend(item_group_graph_references(graph)),
        }
    }
    let mut reachable = BTreeSet::new();
    while let Some(group_id) = pending.pop() {
        if !reachable.insert(group_id) {
            continue;
        }
        let Some(definition) = lookup.get(group_id) else {
            return false;
        };
        pending.extend(item_group_graph_references(&definition.graph));
    }
    reachable.len() == definitions.len()
}

fn item_group_graph_references(graph: &ItemGroupGraphV1) -> impl Iterator<Item = &str> {
    graph
        .nodes
        .iter()
        .flat_map(|node| &node.entries)
        .filter_map(|entry| match &entry.target {
            ItemGroupTargetV1::Group(group_id) => Some(group_id.as_str()),
            ItemGroupTargetV1::Item(_) | ItemGroupTargetV1::Node(_) => None,
        })
}
