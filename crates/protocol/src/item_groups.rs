//! Canonical item-group wire DTOs, bounds, and graph validation.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    CraftItemPrototypeV1, ItemContainmentProfileV1, ItemPhaseV1, MAX_ITEM_COMPONENT_DEPTH,
    MAX_ITEM_RAW_DAMAGE, SimTick, valid_craft_item_prototype, valid_recipe_id,
};

/// Strict current boundary for nonperishable temperature-tracked items whose
/// finalized material mix is empty. Pinned C++ constructs these items active
/// with zero kelvin and -10 J/g sentinel energy; the first ten-minute check
/// initializes them to the canonical normal ambient. `None` energy retains
/// the pinned indeterminate materialless result without serializing a float or
/// platform-dependent NaN payload.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemTemperatureStateV1 {
    pub temperature_millikelvin: i32,
    pub specific_energy_millijoules_per_gram: Option<i32>,
    pub last_check_tick: SimTick,
    pub current_phase: ItemPhaseV1,
    pub hot: bool,
    pub cold: bool,
    pub frozen: bool,
}

pub const ITEM_TEMPERATURE_UNPROCESSED_MILLIKELVIN: i32 = 0;
pub const ITEM_TEMPERATURE_UNPROCESSED_ENERGY_MJ_PER_G: i32 = -10_000;
pub const ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN: i32 = 293_150;
pub const ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS: u64 = 10 * 60 * SimTick::HZ;

#[must_use]
pub fn initial_item_temperature_state(
    birth_tick: SimTick,
    current_phase: ItemPhaseV1,
) -> ItemTemperatureStateV1 {
    ItemTemperatureStateV1 {
        temperature_millikelvin: ITEM_TEMPERATURE_UNPROCESSED_MILLIKELVIN,
        specific_energy_millijoules_per_gram: Some(ITEM_TEMPERATURE_UNPROCESSED_ENERGY_MJ_PER_G),
        last_check_tick: birth_tick,
        current_phase,
        hot: false,
        cold: false,
        frozen: false,
    }
}

pub(super) fn valid_item_temperature_state(state: &ItemTemperatureStateV1) -> bool {
    matches!(
        state.current_phase,
        ItemPhaseV1::Solid | ItemPhaseV1::Liquid
    ) && !state.hot
        && !state.cold
        && !state.frozen
        && matches!(
            (
                state.temperature_millikelvin,
                state.specific_energy_millijoules_per_gram,
            ),
            (
                ITEM_TEMPERATURE_UNPROCESSED_MILLIKELVIN,
                Some(ITEM_TEMPERATURE_UNPROCESSED_ENERGY_MJ_PER_G),
            ) | (ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN, None)
        )
}

pub(super) fn valid_item_fit_state(fitted: bool, containment: &ItemContainmentProfileV1) -> bool {
    let immutable_fit = initial_item_fit_state(containment);
    let variable_size = item_profile_has_flag(containment, "VARSIZE");
    (!immutable_fit || fitted) && (!fitted || immutable_fit || variable_size)
}

pub(super) fn initial_item_fit_state(containment: &ItemContainmentProfileV1) -> bool {
    item_profile_has_flag(containment, "FIT")
}

fn item_profile_has_flag(containment: &ItemContainmentProfileV1, expected: &str) -> bool {
    containment
        .flags
        .binary_search_by(|candidate| candidate.as_str().cmp(expected))
        .is_ok()
}

pub const MAX_ITEM_VARIANTS: usize = 256;
pub const MAX_ITEM_SNIPPETS: usize = 256;
pub const MAX_ITEM_VARIABLES: usize = 64;
pub const MAX_DESCRIPTION_SNIPPET_CATEGORIES: usize = 256;
/// Includes the pinned 20,900-choice English `<world_name>` category while
/// keeping one normalized item's self-contained closure explicitly bounded.
pub const MAX_DESCRIPTION_SNIPPET_CHOICES: usize = 32_768;
pub const MAX_DESCRIPTION_SNIPPET_DEPTH: usize = 32;
pub const MAX_EXPANDED_DESCRIPTION_BYTES: usize = 16_384;

/// A selected cosmetic item variant retained in canonical state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemVariantV1 {
    pub id: String,
    pub name: String,
    pub description: String,
    pub symbol: String,
    pub color: String,
    pub ascii_picture: String,
}

/// One source-ordered constructor choice. Zero-weight variants remain
/// addressable by explicit item-group modifiers but are never selected at
/// construction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupVariantOptionV1 {
    pub variant: ItemVariantV1,
    pub weight: u32,
    /// Recursive description expansion performed when this variant becomes
    /// active. The plan is self-contained and never consults live content.
    pub description_expansion: Option<ItemDescriptionExpansionV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemDescriptionSnippetChoiceV1 {
    pub text: String,
    pub weight: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemDescriptionSnippetCategoryV1 {
    pub category: String,
    /// Upstream selection order: identified entries followed by anonymous
    /// entries, with source order retained inside each partition.
    pub choices: Vec<ItemDescriptionSnippetChoiceV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemDescriptionExpansionV1 {
    pub template: String,
    /// Sorted reachable closure. Categories not present here remain literal
    /// tags and consume no simulation RNG.
    pub categories: Vec<ItemDescriptionSnippetCategoryV1>,
}

/// Self-contained selected snippet presentation. Constructor choices use the
/// same shape and snapshots retain the selected value without live content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemSnippetV1 {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemVariableValueV1 {
    Integer(i64),
    String(String),
}

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

/// Canonical storage selected by pinned `Item_modifier::charges` for any
/// supported tool or magazine. The historical type name is retained so
/// this generalized admission does not alter the Postcard representation. The
/// server resolves content defaults before simulation, which never guesses a
/// magazine or ammunition item.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupToolChargeStorageV1 {
    Integral {
        ammunition: CraftItemPrototypeV1,
    },
    Detachable {
        well_pocket_index: u16,
        magazine: CraftItemPrototypeV1,
        ammunition: Box<CraftItemPrototypeV1>,
    },
}

/// A direct item leaf in a normalized item-group graph. The server resolves
/// immutable content into the same prototype used by crafting before the
/// graph enters canonical simulation state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupItemPrototypeV1 {
    pub prototype: CraftItemPrototypeV1,
    /// Zero for count-by-charge/otherwise undamageable leaves; otherwise the
    /// pinned exact maximum raw damage.
    pub maximum_raw_damage: u16,
    /// Source-ordered finalized generic variants.
    pub variants: Vec<ItemGroupVariantOptionV1>,
    /// Base-type description expansion. Variant expansion, when selected,
    /// occurs later and overwrites the same canonical description variable.
    pub description_expansion: Option<ItemDescriptionExpansionV1>,
    /// Source-ordered inline snippet choices. Named categories remain closed.
    pub snippets: Vec<ItemSnippetV1>,
    /// Typed variables copied from the finalized item type before modifiers.
    pub initial_variables: BTreeMap<String, ItemVariableValueV1>,
    /// Finalized item-type default containment used by the direct constructor
    /// path and as `Item_modifier`'s fallback when no explicit container was
    /// supplied. The container prototype is self-contained and is constructed
    /// without an item-group FIT phase, matching pinned `item(...)` behavior.
    pub default_container: Option<ItemGroupContainerV1>,
    /// Whether applying an upstream item-group modifier to this leaf has no
    /// authoritative side effects beyond the raw damage, selected variant,
    /// and magazine-dressing RNG phases represented by the protocol.
    pub modifier_side_effects_supported: bool,
    pub charges: Option<InclusiveI32RangeV1>,
    /// Count-by-charge items clamp an explicit zero charge roll to one in the
    /// pinned implementation. Non-charge items leave this false.
    pub minimum_one_charge: bool,
    /// Fully resolved nested storage used when `charges` targets ammunition.
    /// `None` means charges remain on the owning instance.
    pub tool_charge_storage: Option<ItemGroupToolChargeStorageV1>,
    pub charges_supported: bool,
    /// Whether pinned `Item_modifier` derives a charge ceiling from a
    /// modifier-owned container for this finalized item type. This is true for
    /// liquids and non-tool/non-gun/non-magazine items.
    pub modifier_container_capacity_applies: bool,
    /// Whether every retained pocket shape needed by generalized item-group
    /// contents insertion is represented by the normalized prototype. This
    /// distinguishes a true no-pocket item from an item whose unsupported raw
    /// pocket caused all strict projections to be cleared.
    pub contents_insertion_supported: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupOverflowV1 {
    None,
    Spill,
    Discard,
}

/// One item type used as an entry, modifier, or whole-group wrapper.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemGroupContainerV1 {
    pub item: Box<ItemGroupItemPrototypeV1>,
    pub variant_id: Option<String>,
    pub sealed: bool,
    pub overflow: ItemGroupOverflowV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemGroupContentsSourceV1 {
    Item(Box<ItemGroupItemPrototypeV1>),
    Group(String),
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
    pub raw_damage: Option<InclusiveU16RangeV1>,
    /// Explicit variant applied after construction. On named-group targets it
    /// is applied to every generated child item in output order.
    pub variant_id: Option<String>,
    pub event: Option<ItemGroupEventV1>,
    /// Modifier charges applied to every completed child of a named group.
    /// Direct item leaves retain their range on `ItemGroupItemPrototypeV1` for
    /// compatibility with earlier fixtures.
    pub modifier_charges: Option<InclusiveI32RangeV1>,
    pub contents: Vec<ItemGroupContentsSourceV1>,
    /// Upstream seals the modified item after adding `contents-*` when it is a
    /// comestible. Entry wrappers are a separate always-sealed spawn layer.
    pub seal_contents: bool,
    /// Sealing policy for the modifier's item-type default-container fallback.
    /// For a direct leaf the descriptor is carried by that leaf; for a named
    /// group the generated top-level type is inspected dynamically. `None`
    /// means the entry has no modifier-owned fallback phase; explicit modifier
    /// containers carry their policy in their descriptor instead.
    pub modifier_default_container_sealed: Option<bool>,
    pub direct_wrapper: Option<ItemGroupContainerV1>,
    pub modifier_container: Option<ItemGroupContainerV1>,
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
    pub wrapper: Option<ItemGroupContainerV1>,
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
    containment_depth: usize,
    /// Upper bound on top-level items whose charge modifier can materialize a
    /// detachable magazine even for an explicit zero-charge result.
    charge_magazine_candidates: u64,
    /// Upper bound on top-level items whose positive charge modifier can
    /// materialize one nested ammunition object.
    charge_ammunition_candidates: u64,
    modifier_side_effects_supported: bool,
    charges_supported: bool,
    estorable_contents_supported: bool,
    non_estorable_contents_supported: bool,
    all_top_level_estorable: bool,
    /// Conservative proof input for named-group modifiers. A modifier applied
    /// after a named group must re-evaluate the generated top-level type's
    /// default container. The current catalog is self-contained for direct
    /// leaves, but deliberately rejects that dynamic composite case whenever
    /// it may occur.
    top_level_default_container_possible: bool,
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
        let mut metrics = self.evaluate_named_node(index, root_node, recursion_depth)?;
        if let Some(wrapper) = &self.definitions.get(index)?.graph.wrapper {
            metrics.outputs = metrics.outputs.checked_add(1)?;
            metrics.depth = metrics.depth.checked_add(1)?;
            metrics.containment_depth = metrics.containment_depth.checked_add(1)?;
            apply_output_wrapper_metrics(&mut metrics, wrapper);
        }
        if metrics.outputs > MAX_ITEM_GROUP_OUTPUTS
            || metrics.depth > MAX_ITEM_GROUP_DEPTH
            || metrics.containment_depth > MAX_ITEM_COMPONENT_DEPTH
        {
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
                ItemGroupTargetV1::Item(item) => Some(item_group_item_metrics(item)),
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
        let mut metrics = self.evaluate_inline_node(graph, graph.root_node, &mut states, 0)?;
        if let Some(wrapper) = &graph.wrapper {
            metrics.outputs = metrics.outputs.checked_add(1)?;
            metrics.depth = metrics.depth.checked_add(1)?;
            metrics.containment_depth = metrics.containment_depth.checked_add(1)?;
            apply_output_wrapper_metrics(&mut metrics, wrapper);
        }
        (metrics.outputs <= MAX_ITEM_GROUP_OUTPUTS
            && metrics.depth <= MAX_ITEM_GROUP_DEPTH
            && metrics.containment_depth <= MAX_ITEM_COMPONENT_DEPTH)
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
                ItemGroupTargetV1::Item(item) => Some(item_group_item_metrics(item)),
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
        let mut containment_depth = 0_usize;
        let mut charge_magazine_candidates = 0_u64;
        let mut charge_ammunition_candidates = 0_u64;
        let mut modifier_side_effects_supported = true;
        let mut charges_supported = true;
        let mut estorable_contents_supported = true;
        let mut non_estorable_contents_supported = true;
        let mut all_top_level_estorable = true;
        let mut top_level_default_container_possible = false;
        for entry in entries {
            let mut target = target_metrics(self, &entry.target)?;
            let modifier_present = entry.raw_damage.is_some()
                || entry.variant_id.is_some()
                || entry.modifier_charges.is_some()
                || !entry.contents.is_empty()
                || entry.seal_contents
                || entry.modifier_default_container_sealed.is_some()
                || entry.modifier_container.is_some();
            if modifier_present
                && entry.modifier_default_container_sealed.is_none()
                && let ItemGroupTargetV1::Item(item) = &entry.target
            {
                // An explicit modifier container, including the upstream null
                // sentinel normalized as no container, suppresses the item
                // type's default fallback. Metrics must therefore start from
                // the raw constructed item rather than its ordinary default-
                // contained result.
                target = raw_item_group_item_metrics(item);
            }
            if entry.modifier_default_container_sealed.is_some()
                && !matches!(&entry.target, ItemGroupTargetV1::Item(_))
                && target.top_level_default_container_possible
            {
                // A named-group modifier sees already-generated objects and
                // may therefore fall back through several possible top-level
                // type defaults. Retain those definitions, but fail closed
                // until the protocol carries their aggregate wrapper closure.
                return None;
            }
            if modifier_present && !target.modifier_side_effects_supported {
                return None;
            }
            if entry.modifier_charges.is_some() && !target.charges_supported {
                return None;
            }
            let modifier_creator_metrics = entry
                .modifier_container
                .as_ref()
                .map(item_group_item_creator_metrics);
            let modified_contents_support = modifier_creator_metrics
                .map(|metrics| {
                    (
                        metrics.estorable_contents_supported,
                        metrics.non_estorable_contents_supported,
                    )
                })
                .unwrap_or((
                    target.estorable_contents_supported,
                    target.non_estorable_contents_supported,
                ));
            let count = u64::from(entry.count_max);
            let mut entry_outputs = target.outputs.checked_mul(count)?;
            let (contents_outputs, contents_depth, all_contents_estorable) =
                entry.contents.iter().try_fold(
                    (0_u64, 0_usize, true),
                    |(total, depth, all_estorable), contents| {
                        let metrics = match contents {
                            ItemGroupContentsSourceV1::Item(item) => item_group_item_metrics(item),
                            ItemGroupContentsSourceV1::Group(group_id) => {
                                let index = *self.lookup.get(group_id.as_str())?;
                                self.evaluate_group(index, 0)?
                            }
                        };
                        Some((
                            total.checked_add(metrics.outputs)?,
                            depth.max(metrics.containment_depth),
                            all_estorable && metrics.all_top_level_estorable,
                        ))
                    },
                )?;
            if !entry.contents.is_empty()
                && !(if all_contents_estorable {
                    modified_contents_support.0
                } else {
                    modified_contents_support.1
                })
            {
                return None;
            }
            entry_outputs =
                entry_outputs.checked_add(entry_outputs.checked_mul(contents_outputs)?)?;
            let positive_modifier_charges = entry
                .modifier_charges
                .is_some_and(|charges| charges.maximum > 0);
            if entry.modifier_charges.is_some() {
                entry_outputs = entry_outputs
                    .checked_add(target.charge_magazine_candidates.checked_mul(count)?)?;
            }
            if positive_modifier_charges {
                entry_outputs = entry_outputs
                    .checked_add(target.charge_ammunition_candidates.checked_mul(count)?)?;
            }
            if let Some(container) = modifier_creator_metrics {
                // Each modified target retains its own subtree and gains the
                // explicit creator's complete subtree. A creator whose item
                // has a type default therefore contributes both that item and
                // its effective outer container; doubling the target count
                // would undercount that three-node shape.
                entry_outputs = entry_outputs.checked_add(container.outputs.checked_mul(count)?)?;
            }
            if entry.direct_wrapper.is_some() {
                // One direct container wraps the entire count result. Spill
                // overflow can retain every payload beside that container;
                // count zero still produces the one empty container.
                entry_outputs = entry_outputs.checked_add(1)?;
            }
            let mut entry_result = target;
            entry_result.charge_magazine_candidates =
                entry_result.charge_magazine_candidates.checked_mul(count)?;
            entry_result.charge_ammunition_candidates = entry_result
                .charge_ammunition_candidates
                .checked_mul(count)?;
            if let Some(container) = modifier_creator_metrics {
                // Modifier containers replace every modified top-level item;
                // unlike whole-output wrappers, their overflow policy is not
                // part of the pinned Item_modifier behavior.
                entry_result = container;
            }
            if let Some(wrapper) = &entry.direct_wrapper {
                apply_output_wrapper_metrics(&mut entry_result, wrapper);
            }
            let entry_depth = target.depth.checked_add(1)?;
            let mut entry_containment_depth = target.containment_depth;
            if entry.modifier_charges.is_some() && target.charge_magazine_candidates > 0 {
                entry_containment_depth = entry_containment_depth.max(1);
            }
            if positive_modifier_charges
                && target.charge_magazine_candidates > 0
                && target.charge_ammunition_candidates > 0
            {
                entry_containment_depth = entry_containment_depth.max(2);
            } else if positive_modifier_charges && target.charge_ammunition_candidates > 0 {
                entry_containment_depth = entry_containment_depth.max(1);
            }
            if entry.modifier_container.is_some() {
                entry_containment_depth = entry_containment_depth.checked_add(1)?;
                if let Some(container) = modifier_creator_metrics {
                    entry_containment_depth =
                        entry_containment_depth.max(container.containment_depth);
                }
            }
            if !entry.contents.is_empty() {
                entry_containment_depth =
                    entry_containment_depth.max(contents_depth.checked_add(1)?);
            }
            if entry.direct_wrapper.is_some() {
                entry_containment_depth = entry_containment_depth.checked_add(1)?;
            }
            if entry_outputs > MAX_ITEM_GROUP_OUTPUTS
                || entry_depth > MAX_ITEM_GROUP_DEPTH
                || entry_containment_depth > MAX_ITEM_COMPONENT_DEPTH
            {
                return None;
            }
            match kind {
                ItemGroupKindV1::Collection => {
                    outputs = outputs.checked_add(entry_outputs)?;
                    charge_magazine_candidates = charge_magazine_candidates
                        .checked_add(entry_result.charge_magazine_candidates)?;
                    charge_ammunition_candidates = charge_ammunition_candidates
                        .checked_add(entry_result.charge_ammunition_candidates)?;
                }
                ItemGroupKindV1::Distribution => {
                    outputs = outputs.max(entry_outputs);
                    charge_magazine_candidates =
                        charge_magazine_candidates.max(entry_result.charge_magazine_candidates);
                    charge_ammunition_candidates =
                        charge_ammunition_candidates.max(entry_result.charge_ammunition_candidates);
                }
            }
            if outputs > MAX_ITEM_GROUP_OUTPUTS {
                return None;
            }
            depth = depth.max(entry_depth);
            containment_depth = containment_depth.max(entry_containment_depth);
            modifier_side_effects_supported &= entry_result.modifier_side_effects_supported;
            charges_supported &= entry_result.charges_supported;
            estorable_contents_supported &= entry_result.estorable_contents_supported;
            non_estorable_contents_supported &= entry_result.non_estorable_contents_supported;
            all_top_level_estorable &= entry_result.all_top_level_estorable;
            top_level_default_container_possible |=
                entry_result.top_level_default_container_possible;
        }
        Some(ItemGroupMetrics {
            outputs,
            depth,
            containment_depth,
            charge_magazine_candidates,
            charge_ammunition_candidates,
            modifier_side_effects_supported,
            charges_supported,
            estorable_contents_supported,
            non_estorable_contents_supported,
            all_top_level_estorable,
            top_level_default_container_possible,
        })
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

fn apply_output_wrapper_metrics(metrics: &mut ItemGroupMetrics, wrapper: &ItemGroupContainerV1) {
    let retains_spilled_payloads = wrapper.overflow == ItemGroupOverflowV1::Spill;
    metrics.modifier_side_effects_supported = wrapper.item.modifier_side_effects_supported
        && (!retains_spilled_payloads || metrics.modifier_side_effects_supported);
    metrics.charges_supported =
        wrapper.item.charges_supported && (!retains_spilled_payloads || metrics.charges_supported);
    let contents_support = item_group_container_insertion_support(wrapper);
    metrics.estorable_contents_supported =
        contents_support.0 && (!retains_spilled_payloads || metrics.estorable_contents_supported);
    metrics.non_estorable_contents_supported = contents_support.1
        && (!retains_spilled_payloads || metrics.non_estorable_contents_supported);
    metrics.all_top_level_estorable = wrapper.item.prototype.containment.estorable
        && (!retains_spilled_payloads || metrics.all_top_level_estorable);
    metrics.top_level_default_container_possible = wrapper.item.default_container.is_some()
        || (retains_spilled_payloads && metrics.top_level_default_container_possible);
    if !retains_spilled_payloads {
        metrics.charge_magazine_candidates = 0;
        metrics.charge_ammunition_candidates = 0;
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
        || graph
            .wrapper
            .as_ref()
            .is_some_and(|wrapper| !valid_item_group_container(wrapper))
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
            let modifier_present = entry.raw_damage.is_some()
                || entry.variant_id.is_some()
                || entry.modifier_charges.is_some()
                || !entry.contents.is_empty()
                || entry.seal_contents
                || entry.modifier_default_container_sealed.is_some()
                || entry.modifier_container.is_some();
            let modifier_requires_marker = entry.count_min != 1
                || entry.count_max != 1
                || entry.variant_id.is_some()
                || entry.modifier_charges.is_some()
                || !entry.contents.is_empty()
                || entry.seal_contents
                || entry.modifier_default_container_sealed.is_some()
                || entry.modifier_container.is_some()
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
                || entry.raw_damage.is_some_and(|damage| {
                    damage.minimum > damage.maximum || damage.maximum > MAX_ITEM_RAW_DAMAGE
                })
                || entry.variant_id.as_ref().is_some_and(|variant| {
                    variant.is_empty()
                        || variant.len() > 512
                        || variant.chars().any(char::is_control)
                })
                || (modifier_present && matches!(&entry.target, ItemGroupTargetV1::Node(_)))
                || entry
                    .modifier_charges
                    .is_some_and(|charges| charges.minimum < 0 || charges.maximum < charges.minimum)
                || (entry.modifier_charges.is_some()
                    && !matches!(&entry.target, ItemGroupTargetV1::Group(_)))
                || (entry.modifier_container.is_some()
                    && !matches!(&entry.target, ItemGroupTargetV1::Item(_)))
                || entry.modifier_default_container_sealed.is_some_and(|_| {
                    entry.modifier_container.is_some()
                        || matches!(&entry.target, ItemGroupTargetV1::Node(_))
                })
                || matches!(
                    &entry.target,
                    ItemGroupTargetV1::Item(item)
                        if entry.modifier_container.is_some()
                            && item.prototype.containment.phase == super::ItemPhaseV1::Liquid
                            && !item.charges_supported
                )
                || entry
                    .contents
                    .iter()
                    .any(|contents| !valid_item_group_contents(contents))
                || (entry.seal_contents && entry.contents.is_empty())
                || entry
                    .direct_wrapper
                    .as_ref()
                    .is_some_and(|wrapper| !valid_item_group_container(wrapper))
                || entry.modifier_container.as_ref().is_some_and(|wrapper| {
                    wrapper.overflow != ItemGroupOverflowV1::None
                        || !valid_item_group_creator_container(wrapper)
                })
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
        ItemGroupTargetV1::Item(item) => valid_item_group_item(item),
        ItemGroupTargetV1::Group(group_id) => valid_recipe_id(group_id),
        ItemGroupTargetV1::Node(node_id) => node_ids.contains(node_id),
    }
}

fn valid_item_group_item(item: &ItemGroupItemPrototypeV1) -> bool {
    valid_item_group_item_at_depth(item, 0)
}

fn valid_item_group_item_at_depth(item: &ItemGroupItemPrototypeV1, depth: usize) -> bool {
    let generates_description = item.description_expansion.is_some()
        || item
            .variants
            .iter()
            .any(|variant| variant.description_expansion.is_some());
    valid_craft_item_prototype(&item.prototype)
        && matches!(item.maximum_raw_damage, 0 | MAX_ITEM_RAW_DAMAGE)
        && valid_item_group_variants(&item.variants)
        && item
            .description_expansion
            .as_ref()
            .is_none_or(item_description_expansion_is_valid)
        && valid_item_snippets(&item.snippets)
        && valid_item_variables(&item.initial_variables)
        && (!generates_description
            || item.initial_variables.contains_key("description")
            || item.initial_variables.len() < MAX_ITEM_VARIABLES)
        && item
            .initial_variables
            .keys()
            .all(|key| !matches!(key.as_str(), "weight" | "integral_weight" | "volume"))
        && item.prototype.containment.flags.len() <= 256
        && item
            .prototype
            .containment
            .flags
            .iter()
            .all(|flag| valid_recipe_id(flag))
        && item
            .prototype
            .containment
            .flags
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && item
            .tool_charge_storage
            .as_ref()
            .is_none_or(|storage| valid_tool_charge_storage(&item.prototype, storage))
        && item
            .tool_charge_storage
            .as_ref()
            .is_none_or(|_| item.charges_supported)
        && item.default_container.as_ref().is_none_or(|container| {
            depth < MAX_ITEM_COMPONENT_DEPTH
                && container.overflow == ItemGroupOverflowV1::None
                && valid_item_group_container_at_depth(container, depth + 1)
        })
        && (!item.modifier_container_capacity_applies
            || (item.tool_charge_storage.is_none()
                && item.prototype.ranged_weapon.is_none()
                && item.prototype.magazine_capacity == 0))
        && (item.prototype.containment.phase != super::ItemPhaseV1::Liquid
            || item.modifier_container_capacity_applies)
        && item.charges.is_none_or(|charges| {
            if charges.minimum < 0 || charges.maximum < charges.minimum {
                return false;
            }
            if let Some(storage) = &item.tool_charge_storage {
                let Some((ammunition, capacity)) =
                    tool_charge_ammunition_and_capacity(&item.prototype, storage)
                else {
                    return false;
                };
                return [charges.minimum, charges.maximum].into_iter().all(|value| {
                    let mut effective = ammunition.clone();
                    effective.charges = value.min(i32::try_from(capacity).unwrap_or(i32::MAX));
                    value == 0 || valid_craft_item_prototype(&effective)
                });
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
        && (item.charges.is_none() || item.charges_supported)
}

fn valid_tool_charge_storage(
    owner: &CraftItemPrototypeV1,
    storage: &ItemGroupToolChargeStorageV1,
) -> bool {
    match storage {
        ItemGroupToolChargeStorageV1::Integral { ammunition } => {
            let [pocket] = owner.integral_magazines.as_slice() else {
                return false;
            };
            owner.magazine_wells.is_empty()
                && valid_craft_item_prototype(ammunition)
                && valid_charge_ammunition_for_integral_pocket(ammunition, pocket)
        }
        ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index,
            magazine,
            ammunition,
            ..
        } => {
            let [well] = owner.magazine_wells.as_slice() else {
                return false;
            };
            let [magazine_pocket] = magazine.integral_magazines.as_slice() else {
                return false;
            };
            owner.integral_magazines.is_empty()
                && well.pocket_index == *well_pocket_index
                && well
                    .compatible_magazine_type_ids
                    .binary_search(&magazine.type_id)
                    .is_ok()
                && magazine.charges == 0
                && magazine.magazine_capacity == 0
                && magazine.magazine_wells.is_empty()
                && magazine.ammunition_containers.is_empty()
                && magazine.residual_energy_millijoules == 0
                && magazine.powered_tool.is_none()
                && valid_craft_item_prototype(magazine)
                && valid_craft_item_prototype(ammunition)
                && valid_charge_ammunition_for_integral_pocket(ammunition, magazine_pocket)
        }
    }
}

fn tool_charge_ammunition_and_capacity<'a>(
    owner: &'a CraftItemPrototypeV1,
    storage: &'a ItemGroupToolChargeStorageV1,
) -> Option<(&'a CraftItemPrototypeV1, u32)> {
    match storage {
        ItemGroupToolChargeStorageV1::Integral { ammunition } => owner
            .integral_magazines
            .first()
            .map(|pocket| (ammunition, pocket.capacity)),
        ItemGroupToolChargeStorageV1::Detachable {
            magazine,
            ammunition,
            ..
        } => magazine
            .integral_magazines
            .first()
            .map(|pocket| (ammunition.as_ref(), pocket.capacity)),
    }
}

fn valid_charge_ammunition_for_integral_pocket(
    ammunition: &CraftItemPrototypeV1,
    pocket: &super::IntegralMagazinePocketPrototypeV1,
) -> bool {
    ammunition.ammunition_type == pocket.ammunition_type
        && ammunition.comestible_type.is_empty()
        && ammunition.ranged_weapon.is_none()
        && ammunition.magazine_capacity == 0
        && ammunition.integral_magazines.is_empty()
        && ammunition.magazine_wells.is_empty()
        && ammunition.ammunition_containers.is_empty()
        && ammunition.residual_energy_millijoules == 0
        && ammunition.powered_tool.is_none()
}

fn item_group_item_containment_depth(item: &ItemGroupItemPrototypeV1) -> usize {
    let Some(storage) = &item.tool_charge_storage else {
        return 0;
    };
    match storage {
        ItemGroupToolChargeStorageV1::Integral { .. } => {
            usize::from(item.charges.is_some_and(|charges| charges.maximum > 0))
        }
        ItemGroupToolChargeStorageV1::Detachable { .. } => {
            usize::from(item.charges.is_some())
                + usize::from(item.charges.is_some_and(|charges| charges.maximum > 0))
        }
    }
}

fn item_group_item_metrics(item: &ItemGroupItemPrototypeV1) -> ItemGroupMetrics {
    let mut metrics = raw_item_group_item_metrics(item);
    if let Some(container) = &item.default_container {
        metrics.outputs = metrics.outputs.saturating_add(1);
        metrics.containment_depth = metrics.containment_depth.saturating_add(1);
        apply_output_wrapper_metrics(&mut metrics, container);
    }
    metrics
}

fn item_group_item_creator_metrics(container: &ItemGroupContainerV1) -> ItemGroupMetrics {
    item_group_item_metrics(&container.item)
}

fn raw_item_group_item_metrics(item: &ItemGroupItemPrototypeV1) -> ItemGroupMetrics {
    let contents_support = item_contents_insertion_support(item);
    ItemGroupMetrics {
        outputs: item_group_item_max_outputs(item),
        depth: 0,
        containment_depth: item_group_item_containment_depth(item),
        charge_magazine_candidates: u64::from(matches!(
            &item.tool_charge_storage,
            Some(ItemGroupToolChargeStorageV1::Detachable { .. })
        )),
        charge_ammunition_candidates: u64::from(item.tool_charge_storage.is_some()),
        modifier_side_effects_supported: item.modifier_side_effects_supported,
        charges_supported: item.charges_supported,
        estorable_contents_supported: contents_support.0,
        non_estorable_contents_supported: contents_support.1,
        all_top_level_estorable: item.prototype.containment.estorable,
        top_level_default_container_possible: item.default_container.is_some(),
    }
}

fn item_group_item_max_outputs(item: &ItemGroupItemPrototypeV1) -> u64 {
    1 + u64::try_from(item_group_item_containment_depth(item)).unwrap_or(1)
}

#[must_use]
pub fn valid_item_variables(variables: &BTreeMap<String, ItemVariableValueV1>) -> bool {
    variables.len() <= MAX_ITEM_VARIABLES
        && variables.iter().all(|(key, value)| {
            !key.is_empty()
                && key.len() <= 128
                && !key.chars().any(char::is_control)
                && match value {
                    ItemVariableValueV1::Integer(_) => true,
                    ItemVariableValueV1::String(value) => {
                        value.len() <= 16_384
                            && !value.chars().any(|character| {
                                character.is_control() && !matches!(character, '\n' | '\r' | '\t')
                            })
                    }
                }
        })
}

#[must_use]
pub fn item_snippet_is_valid(snippet: &ItemSnippetV1) -> bool {
    !snippet.id.is_empty()
        && snippet.id.len() <= 512
        && !snippet.id.chars().any(char::is_control)
        && !snippet.text.is_empty()
        && snippet.text.len() <= 16_384
        && !snippet
            .text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn valid_item_snippets(snippets: &[ItemSnippetV1]) -> bool {
    snippets.len() <= MAX_ITEM_SNIPPETS
        && snippets.iter().all(item_snippet_is_valid)
        && snippets
            .iter()
            .map(|snippet| snippet.id.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == snippets.len()
}

fn valid_item_group_contents(contents: &ItemGroupContentsSourceV1) -> bool {
    match contents {
        ItemGroupContentsSourceV1::Item(item) => valid_item_group_item(item),
        ItemGroupContentsSourceV1::Group(group_id) => valid_recipe_id(group_id),
    }
}

fn valid_item_group_container(container: &ItemGroupContainerV1) -> bool {
    valid_item_group_container_at_depth(container, 0)
}

fn valid_item_group_creator_container(container: &ItemGroupContainerV1) -> bool {
    valid_item_group_item(&container.item)
        && container.item.charges.is_none()
        && container.item.tool_charge_storage.is_none()
        // Upstream `container-item` is a string-only creator reference; unlike
        // direct wrappers, it has no independent variant selector.
        && container.variant_id.is_none()
        && container
            .item
            .default_container
            .as_ref()
            .map_or_else(
                || item_group_container_insertion_supported(container),
                item_group_container_insertion_supported,
            )
}

fn valid_item_group_container_at_depth(container: &ItemGroupContainerV1, depth: usize) -> bool {
    valid_item_group_item_at_depth(&container.item, depth)
        && container.item.charges.is_none()
        && container.item.tool_charge_storage.is_none()
        && item_group_container_insertion_supported(container)
        && container.variant_id.as_ref().is_none_or(|variant| {
            variant != "<any>"
                && container
                    .item
                    .variants
                    .iter()
                    .any(|option| option.variant.id == *variant)
        })
}

fn item_group_container_insertion_supported(container: &ItemGroupContainerV1) -> bool {
    item_group_container_insertion_support(container).1
}

fn item_group_container_insertion_support(container: &ItemGroupContainerV1) -> (bool, bool) {
    let physical = container
        .item
        .prototype
        .ammunition_containers
        .iter()
        .filter_map(|pocket| pocket.spawn_rules.as_ref())
        .filter(|rules| rules.kind == super::SpawnPocketKindV1::Container && rules.rigid)
        .count();
    if physical != 1 {
        return (false, false);
    }
    item_contents_insertion_support(&container.item)
}

fn item_contents_insertion_support(item: &ItemGroupItemPrototypeV1) -> (bool, bool) {
    if !item.contents_insertion_supported {
        return (false, false);
    }
    let mut physical = 0_usize;
    let mut efiles = 0_usize;
    for rules in item
        .prototype
        .ammunition_containers
        .iter()
        .filter_map(|pocket| pocket.spawn_rules.as_ref())
    {
        match rules.kind {
            super::SpawnPocketKindV1::Container => {
                if !rules.rigid {
                    return (false, false);
                }
                physical += 1;
            }
            super::SpawnPocketKindV1::EFileStorage => {
                if !rules.rigid {
                    return (false, false);
                }
                efiles += 1;
            }
        }
    }
    if physical > 1 || efiles > 1 {
        return (false, false);
    }
    let has_reload_only_pocket = !item.prototype.integral_magazines.is_empty()
        || !item.prototype.magazine_wells.is_empty()
        || item
            .prototype
            .ammunition_containers
            .iter()
            .any(|pocket| pocket.spawn_rules.is_none());
    // Estorable payloads choose EFILE first and otherwise follow an exactly
    // represented general-container/drop path. A non-estorable payload can
    // select an integral magazine, magazine well, or ammunition container;
    // those generalized spawn-insertion branches remain fail-closed.
    (true, !has_reload_only_pocket)
}

fn valid_item_group_variants(variants: &[ItemGroupVariantOptionV1]) -> bool {
    if variants.len() > MAX_ITEM_VARIANTS {
        return false;
    }
    let mut ids = BTreeSet::new();
    let mut total_weight = 0_u32;
    for option in variants {
        if !item_variant_is_valid(&option.variant)
            || option
                .description_expansion
                .as_ref()
                .is_some_and(|expansion| !item_description_expansion_is_valid(expansion))
            || option.weight > i32::MAX as u32
            || !ids.insert(option.variant.id.as_str())
        {
            return false;
        }
        let Some(total) = total_weight.checked_add(option.weight) else {
            return false;
        };
        total_weight = total;
        if total_weight > i32::MAX as u32 {
            return false;
        }
    }
    true
}

#[must_use]
pub fn item_description_expansion_is_valid(expansion: &ItemDescriptionExpansionV1) -> bool {
    if expansion.template.len() > MAX_EXPANDED_DESCRIPTION_BYTES
        || invalid_description_text(&expansion.template)
        || expansion.categories.len() > MAX_DESCRIPTION_SNIPPET_CATEGORIES
    {
        return false;
    }
    let mut categories = BTreeMap::new();
    let mut choice_count = 0_usize;
    for category in &expansion.categories {
        if category.category.is_empty()
            || category.category.len() > 512
            || !category.category.starts_with('<')
            || !category.category.ends_with('>')
            || category.category.chars().any(char::is_control)
            || categories
                .insert(category.category.as_str(), category)
                .is_some()
        {
            return false;
        }
        let Some(next_count) = choice_count.checked_add(category.choices.len()) else {
            return false;
        };
        choice_count = next_count;
        if choice_count > MAX_DESCRIPTION_SNIPPET_CHOICES
            || category.choices.iter().any(|choice| {
                choice.text.len() > MAX_EXPANDED_DESCRIPTION_BYTES
                    || invalid_description_text(&choice.text)
            })
            || category
                .choices
                .iter()
                .try_fold(0_u64, |total, choice| total.checked_add(choice.weight))
                .is_none()
        {
            return false;
        }
    }
    if expansion
        .categories
        .windows(2)
        .any(|pair| pair[0].category >= pair[1].category)
    {
        return false;
    }
    let mut visiting = BTreeSet::new();
    let mut reachable = BTreeSet::new();
    let mut memoized_lengths = BTreeMap::new();
    let Some(maximum) = maximum_expanded_description_len(
        &expansion.template,
        &categories,
        &mut visiting,
        &mut reachable,
        &mut memoized_lengths,
        0,
    ) else {
        return false;
    };
    maximum <= MAX_EXPANDED_DESCRIPTION_BYTES && reachable.len() == categories.len()
}

fn invalid_description_text(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

fn maximum_expanded_description_len(
    text: &str,
    categories: &BTreeMap<&str, &ItemDescriptionSnippetCategoryV1>,
    visiting: &mut BTreeSet<String>,
    reachable: &mut BTreeSet<String>,
    memoized_lengths: &mut BTreeMap<(String, usize), usize>,
    depth: usize,
) -> Option<usize> {
    if depth > MAX_DESCRIPTION_SNIPPET_DEPTH {
        return None;
    }
    let mut total = 0_usize;
    let mut remaining = text;
    while let Some(begin) = remaining.find('<') {
        let after_begin = &remaining[begin + 1..];
        let Some(relative_end) = after_begin.find('>') else {
            return total.checked_add(remaining.len());
        };
        let end = begin.checked_add(relative_end)?.checked_add(2)?;
        total = total.checked_add(begin)?;
        let tag = &remaining[begin..end];
        let Some(category) = categories.get(tag) else {
            total = total.checked_add(tag.len())?;
            remaining = &remaining[end..];
            continue;
        };
        reachable.insert(tag.to_owned());
        if let Some(length) = memoized_lengths.get(&(tag.to_owned(), depth)) {
            total = total.checked_add(*length)?;
            if total > MAX_EXPANDED_DESCRIPTION_BYTES {
                return None;
            }
            remaining = &remaining[end..];
            continue;
        }
        if !visiting.insert(tag.to_owned()) {
            return None;
        }
        let mut replacement_maximum = None;
        for choice in category.choices.iter().filter(|choice| choice.weight > 0) {
            let length = maximum_expanded_description_len(
                &choice.text,
                categories,
                visiting,
                reachable,
                memoized_lengths,
                depth.checked_add(1)?,
            )?;
            replacement_maximum =
                Some(replacement_maximum.map_or(length, |current: usize| current.max(length)));
        }
        visiting.remove(tag);
        let replacement_maximum = replacement_maximum.unwrap_or(tag.len());
        memoized_lengths.insert((tag.to_owned(), depth), replacement_maximum);
        total = total.checked_add(replacement_maximum)?;
        if total > MAX_EXPANDED_DESCRIPTION_BYTES {
            return None;
        }
        remaining = &remaining[end..];
    }
    total.checked_add(remaining.len())
}

#[must_use]
pub fn item_variant_is_valid(variant: &ItemVariantV1) -> bool {
    !variant.id.is_empty()
        && variant.id != "<any>"
        && variant.id.len() <= 512
        && !variant.id.chars().any(char::is_control)
        && !variant.name.is_empty()
        && variant.name.len() <= 1_024
        && !variant.name.chars().any(char::is_control)
        && variant.description.len() <= 16_384
        && !variant.description.chars().any(char::is_control)
        && variant.symbol.len() <= 16
        && !variant.symbol.chars().any(char::is_control)
        && variant.color.len() <= 64
        && !variant.color.chars().any(char::is_control)
        && variant.ascii_picture.len() <= 512
        && !variant.ascii_picture.chars().any(char::is_control)
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

/// Returns the maximum number of stable item objects produced by a source,
/// including recursively contained wrappers, contents, and integral
/// ammunition, when both the source and its named catalog are valid.
#[must_use]
pub fn item_group_source_max_outputs(
    source: &ItemGroupSourceV1,
    definitions: &[ItemGroupDefinitionV1],
) -> Option<u64> {
    item_group_sources_are_valid(definitions, std::iter::once(source)).then_some(())?;
    let mut evaluator = ItemGroupEvaluator::new(definitions)?;
    Some(evaluator.evaluate_source(source)?.outputs)
}

#[cfg(test)]
pub(super) fn item_group_source_metrics_for_test(
    source: &ItemGroupSourceV1,
    definitions: &[ItemGroupDefinitionV1],
) -> Option<(u64, usize, u64)> {
    item_group_sources_are_valid(definitions, std::iter::once(source)).then_some(())?;
    let mut evaluator = ItemGroupEvaluator::new(definitions)?;
    let metrics = evaluator.evaluate_source(source)?;
    Some((
        metrics.outputs,
        metrics.containment_depth,
        metrics.charge_ammunition_candidates,
    ))
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
        .flat_map(|entry| {
            let target = match &entry.target {
                ItemGroupTargetV1::Group(group_id) => Some(group_id.as_str()),
                ItemGroupTargetV1::Item(_) | ItemGroupTargetV1::Node(_) => None,
            };
            target
                .into_iter()
                .chain(entry.contents.iter().filter_map(|contents| match contents {
                    ItemGroupContentsSourceV1::Group(group_id) => Some(group_id.as_str()),
                    ItemGroupContentsSourceV1::Item(_) => None,
                }))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AmmunitionContainerPocketPrototypeV1, SpawnPocketKindV1, SpawnPocketRulesV1};

    #[test]
    fn temperature_state_accepts_only_the_bounded_constructor_lifecycle() {
        let birth = SimTick(123);
        let initial = initial_item_temperature_state(birth, ItemPhaseV1::Solid);
        assert!(valid_item_temperature_state(&initial));

        let mut initialized = initial;
        initialized.temperature_millikelvin = ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN;
        initialized.specific_energy_millijoules_per_gram = None;
        initialized.last_check_tick = SimTick(12_123);
        assert!(valid_item_temperature_state(&initialized));

        let mut invented_energy = initialized;
        invented_energy.specific_energy_millijoules_per_gram = Some(0);
        assert!(!valid_item_temperature_state(&invented_energy));

        let mut invented_flag = initialized;
        invented_flag.hot = true;
        assert!(!valid_item_temperature_state(&invented_flag));

        let mut unsupported_phase = initialized;
        unsupported_phase.current_phase = ItemPhaseV1::Gas;
        assert!(!valid_item_temperature_state(&unsupported_phase));
    }

    fn valid_test_item() -> ItemGroupItemPrototypeV1 {
        ItemGroupItemPrototypeV1 {
            prototype: CraftItemPrototypeV1 {
                type_id: String::from("test_item"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                tracks_temperature: false,
                ammunition_type: String::new(),
                ranged_weapon: None,
                magazine_capacity: 0,
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
                containment: Default::default(),
            },
            maximum_raw_damage: 0,
            variants: Vec::new(),
            description_expansion: None,
            snippets: Vec::new(),
            initial_variables: BTreeMap::new(),
            default_container: None,
            modifier_side_effects_supported: true,
            charges: None,
            minimum_one_charge: false,
            tool_charge_storage: None,
            charges_supported: true,
            modifier_container_capacity_applies: true,
            contents_insertion_supported: true,
        }
    }

    fn valid_test_container(type_id: &str) -> ItemGroupContainerV1 {
        let mut item = valid_test_item();
        item.prototype.type_id = type_id.to_owned();
        item.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("CONTAINER"),
            capacities: Vec::new(),
            rigid: true,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind: SpawnPocketKindV1::Container,
                max_contains_volume_milliliters: 1_000,
                max_contains_weight_milligrams: 1_000_000,
                max_item_volume_milliliters: 1_000,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: 1_000,
                item_restrictions: Vec::new(),
                flag_restrictions: Vec::new(),
                access_moves: 100,
                rigid: true,
                watertight: true,
                transparent: true,
                forbidden: false,
                sealable: true,
            }),
        }];
        ItemGroupContainerV1 {
            item: Box::new(item),
            variant_id: None,
            sealed: true,
            overflow: ItemGroupOverflowV1::None,
        }
    }

    fn test_group(group_id: &str, entry: ItemGroupEntryV1) -> ItemGroupDefinitionV1 {
        ItemGroupDefinitionV1 {
            group_id: group_id.to_owned(),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![entry],
                }],
                wrapper: None,
            },
        }
    }

    fn test_entry(target: ItemGroupTargetV1) -> ItemGroupEntryV1 {
        ItemGroupEntryV1 {
            probability: 100,
            count_min: 1,
            count_max: 1,
            raw_damage: None,
            variant_id: None,
            event: None,
            modifier_charges: None,
            contents: Vec::new(),
            seal_contents: false,
            modifier_default_container_sealed: None,
            direct_wrapper: None,
            modifier_container: None,
            target,
        }
    }

    #[test]
    fn default_container_fallback_and_explicit_null_have_distinct_valid_shapes() {
        let mut item = valid_test_item();
        item.default_container = Some(valid_test_container("default_bottle"));

        let direct = test_group(
            "direct_default",
            test_entry(ItemGroupTargetV1::Item(Box::new(item.clone()))),
        );
        assert!(item_group_catalog_is_valid(std::slice::from_ref(&direct)));

        let mut fallback = test_entry(ItemGroupTargetV1::Item(Box::new(item.clone())));
        fallback.raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        fallback.modifier_default_container_sealed = Some(false);
        assert!(item_group_catalog_is_valid(&[test_group(
            "modifier_fallback",
            fallback,
        )]));

        let mut suppressed = test_entry(ItemGroupTargetV1::Item(Box::new(item)));
        suppressed.raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        assert!(
            item_group_catalog_is_valid(&[test_group("explicit_null_suppression", suppressed)]),
            "an explicit null modifier container is represented by a modifier marker without a fallback"
        );
    }

    #[test]
    fn explicit_modifier_container_creator_may_apply_its_own_default_wrapper() {
        let mut creator_item = valid_test_item();
        creator_item.prototype.type_id = String::from("creator_payload");
        creator_item.default_container = Some(valid_test_container("effective_wrapper"));
        let creator = ItemGroupContainerV1 {
            item: Box::new(creator_item),
            variant_id: None,
            sealed: true,
            overflow: ItemGroupOverflowV1::None,
        };
        let mut entry = test_entry(ItemGroupTargetV1::Item(Box::new(valid_test_item())));
        entry.raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        entry.modifier_container = Some(creator.clone());
        assert!(item_group_catalog_is_valid(&[test_group(
            "creator_default_wrapper",
            entry,
        )]));

        let mut direct = test_entry(ItemGroupTargetV1::Item(Box::new(valid_test_item())));
        direct.direct_wrapper = Some(creator);
        assert!(
            !item_group_catalog_is_valid(&[test_group("raw_wrapper_stays_raw", direct)]),
            "whole-group wrappers use the raw item constructor and cannot borrow creator semantics"
        );
    }

    #[test]
    fn dynamic_named_group_default_fallback_is_retained_but_fails_closed() {
        let mut item = valid_test_item();
        let mut default_bottle = valid_test_container("default_bottle");
        default_bottle.item.default_container =
            Some(valid_test_container("second_level_default_bottle"));
        item.default_container = Some(default_bottle);
        let inner = test_group(
            "inner_default",
            test_entry(ItemGroupTargetV1::Item(Box::new(item))),
        );
        let mut outer_entry = test_entry(ItemGroupTargetV1::Group(inner.group_id.clone()));
        outer_entry.raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        outer_entry.modifier_default_container_sealed = Some(true);
        let outer = test_group("outer_modifier", outer_entry);
        assert!(!item_group_catalog_is_valid(&[inner, outer]));

        let plain_inner = test_group(
            "plain_inner",
            test_entry(ItemGroupTargetV1::Item(Box::new(valid_test_item()))),
        );
        let mut proven_noop = test_entry(ItemGroupTargetV1::Group(plain_inner.group_id.clone()));
        proven_noop.raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        proven_noop.modifier_default_container_sealed = Some(true);
        let plain_outer = test_group("plain_outer", proven_noop);
        assert!(item_group_catalog_is_valid(&[plain_inner, plain_outer]));
    }

    #[test]
    fn graph_shape_rejects_contents_modifiers_on_local_node_targets() {
        let graph = ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![
                ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![ItemGroupEntryV1 {
                        probability: 100,
                        count_min: 1,
                        count_max: 1,
                        raw_damage: Some(InclusiveU16RangeV1 {
                            minimum: 0,
                            maximum: 0,
                        }),
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Node(1),
                        modifier_charges: None,
                        contents: vec![ItemGroupContentsSourceV1::Group(String::from("contents"))],
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                },
                ItemGroupNodeV1 {
                    node_id: 1,
                    kind: ItemGroupKindV1::Collection,
                    entries: Vec::new(),
                },
            ],
            wrapper: None,
        };

        assert!(
            !valid_item_group_graph_shape(&graph),
            "canonical-valid Node entries must not reach simulator-rejected modifier shapes"
        );
    }

    #[test]
    fn description_expansion_requires_exact_acyclic_reachable_closure() {
        let choice = |text: &str, weight| ItemDescriptionSnippetChoiceV1 {
            text: text.to_owned(),
            weight,
        };
        let category = |category: &str, choices| ItemDescriptionSnippetCategoryV1 {
            category: category.to_owned(),
            choices,
        };
        let valid = ItemDescriptionExpansionV1 {
            template: String::from("Before <outer> <unknown>"),
            categories: vec![
                category("<inner>", vec![choice("done", 1)]),
                category(
                    "<outer>",
                    vec![choice("nested <inner>", 2), choice("plain", 0)],
                ),
            ],
        };
        assert!(item_description_expansion_is_valid(&valid));

        let mut unsorted = valid.clone();
        unsorted.categories.reverse();
        assert!(!item_description_expansion_is_valid(&unsorted));

        let mut unreachable = valid.clone();
        unreachable
            .categories
            .insert(0, category("<extra>", vec![choice("unused", 1)]));
        assert!(!item_description_expansion_is_valid(&unreachable));

        let cyclic = ItemDescriptionExpansionV1 {
            template: String::from("<cycle_a>"),
            categories: vec![
                category("<cycle_a>", vec![choice("<cycle_b>", 1)]),
                category("<cycle_b>", vec![choice("<cycle_a>", 1)]),
            ],
        };
        assert!(!item_description_expansion_is_valid(&cyclic));

        let literal_zero_weight = ItemDescriptionExpansionV1 {
            template: String::from("<zero>"),
            categories: vec![category("<zero>", vec![choice("never", 0)])],
        };
        assert!(item_description_expansion_is_valid(&literal_zero_weight));
    }

    #[test]
    fn description_expansion_memoizes_repeated_dag_branches() {
        let mut categories = Vec::new();
        for index in 0..MAX_DESCRIPTION_SNIPPET_DEPTH {
            let replacement = if index + 1 == MAX_DESCRIPTION_SNIPPET_DEPTH {
                String::from("done")
            } else {
                format!("<dag_{:02}>", index + 1)
            };
            categories.push(ItemDescriptionSnippetCategoryV1 {
                category: format!("<dag_{index:02}>"),
                choices: vec![
                    ItemDescriptionSnippetChoiceV1 {
                        text: replacement.clone(),
                        weight: 1,
                    },
                    ItemDescriptionSnippetChoiceV1 {
                        text: replacement,
                        weight: 1,
                    },
                ],
            });
        }
        let expansion = ItemDescriptionExpansionV1 {
            template: String::from("<dag_00>"),
            categories,
        };
        assert!(item_description_expansion_is_valid(&expansion));
    }

    #[test]
    fn generated_description_reserves_variable_capacity() {
        let mut item = valid_test_item();
        item.description_expansion = Some(ItemDescriptionExpansionV1 {
            template: String::from("expanded"),
            categories: Vec::new(),
        });
        item.initial_variables = (0..MAX_ITEM_VARIABLES)
            .map(|index| {
                (
                    format!("variable_{index}"),
                    ItemVariableValueV1::Integer(index as i64),
                )
            })
            .collect();
        assert!(!valid_item_group_item(&item));

        item.initial_variables.remove("variable_0");
        item.initial_variables.insert(
            String::from("description"),
            ItemVariableValueV1::String(String::from("old")),
        );
        assert!(valid_item_group_item(&item));
    }
}
