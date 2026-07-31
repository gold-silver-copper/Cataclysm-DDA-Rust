use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cdda_content::{
    CitySettingsRegistry, ContentManifest, DEFAULT_CITY_SETTINGS_ID, DescriptionSnippetRegistry,
    ItemRegistry, MaterialRegistry, ModCatalog, OvermapTerrainMatchType, OvermapTerrainRegistry,
    StartLocationRegistry,
};
use cdda_protocol::{
    AmmunitionContainerPocketPrototypeV1, BASELINE_COMMIT, ChunkCoord, CraftItemPrototypeV1,
    FurnitureTileSnapshot, ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE,
    IntegralMagazinePocketPrototypeV1, ItemContainmentProfileV1, ItemDescriptionExpansionV1,
    ItemDescriptionSnippetCategoryV1, ItemDescriptionSnippetChoiceV1, ItemGroupContainerV1,
    ItemGroupDefinitionV1, ItemGroupEntryV1, ItemGroupGraphV1, ItemGroupItemPrototypeV1,
    ItemGroupKindV1, ItemGroupNodeV1, ItemGroupOverflowV1, ItemGroupTargetV1,
    ItemGroupToolChargeStorageV1, ItemGroupVariantOptionV1, ItemPhaseV1, ItemSnippetV1,
    ItemVariableValueV1, ItemVariantV1, MagazineWellPrototypeV1, SimTick, SpawnPocketKindV1,
    SpawnPocketRulesV1, TerrainTileSnapshot, WORLDGEN_CELLS_PER_OMT, WORLDGEN_OMT_SIZE,
    WORLDGEN_OVERMAP_HEIGHT, WORLDGEN_OVERMAP_WIDTH, WorldPosition, WorldSnapshotV1,
    WorldgenCatalogV1, WorldgenCellV1, WorldgenCityId, WorldgenCityV1, WorldgenFurnitureTargetV1,
    WorldgenItemGroupPlacementV1, WorldgenOmtGeneratorV1, WorldgenOmtIdentityV1,
    WorldgenOmtMatchTypeV1, WorldgenOvermapLayerV1, WorldgenOvermapLayoutV1, WorldgenOvermapRunV1,
    WorldgenTemplateV1, WorldgenTerrainTargetV1, WorldgenWeightedFurnitureTargetV1,
    WorldgenWeightedTerrainTargetV1, initial_item_temperature_state, worldgen_city_start_distance,
    worldgen_omt_matches,
};
use cdda_sim::{ReservedIdBlock, WorldState, overmap_road_mst_edges};
use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};

mod item_group_dressing;

use item_group_dressing::ItemGroupDressingObservationV1;

const ORACLE_FORMAT_VERSION: u16 = 1;
const CACHE_FORMAT_VERSION: u16 = 1;
const UPSTREAM_TREE: &str = "210f31db2e8b2f0caed1809f1a66781859f9d129";
const KERNEL: &str = "item_pocket_max_length_v1";
const ITEM_GROUP_KERNEL: &str = "item_group_generation_v1";
const MAPGEN_KERNEL: &str = "mapgen_static_semantics_v1";
const MAX_JSON_BYTES: u64 = 1024 * 1024;
const MAX_CASES: usize = 8;
const DEFAULT_SCENARIO: &str = "docs/oracles/item-pocket-max-length-v1.json";
const ADAPTER_SOURCE: &str = include_str!("../../../tools/cpp-oracle/item_pocket_oracle_test.cpp");
const ITEM_GROUP_ADAPTER_SOURCE: &str =
    include_str!("../../../tools/cpp-oracle/item_group_oracle_test.cpp");
const MAPGEN_ADAPTER_SOURCE: &str =
    include_str!("../../../tools/cpp-oracle/mapgen_oracle_test.cpp");
const ADAPTER_MAKEFILE: &str = include_str!("../../../tools/cpp-oracle/oracle.mk");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleScenarioHeader {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    expected_observation: serde_json::Value,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct OracleScenarioV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    expected_observation: OracleObservationV1,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct OracleObservationV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    pocket: PocketObservationV1,
    cases: Vec<PocketCaseObservationV1>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PocketObservationV1 {
    pocket_type: String,
    max_item_length_mm: i64,
    volume_capacity_ml: i64,
    weight_capacity_g: i64,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PocketCaseObservationV1 {
    case_id: String,
    item_id: String,
    item_length_mm: i64,
    success: bool,
    contain_code: i32,
    contain_code_name: String,
    reason: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ItemGroupOracleScenarioV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    expected_observation: ItemGroupOracleObservationV1,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupOracleObservationV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    collection: ItemGroupTraceObservationV1,
    distribution: Vec<ItemGroupDistributionObservationV1>,
    counts: Vec<ItemGroupRangeObservationV1>,
    charges: Vec<ItemGroupRangeObservationV1>,
    tool_charges: Vec<ItemGroupToolChargeObservationV1>,
    magazine_charges: ItemGroupMagazineChargeObservationV1,
    repeated_tool_charges: ItemGroupRepeatedToolChargeObservationV1,
    modifier_rng_phase: ItemGroupModifierRngPhaseObservationV1,
    constructor_variants: Vec<ItemGroupConstructorVariantTraceV1>,
    description_expansion: ItemGroupDescriptionExpansionObservationV1,
    variable_size_fit: ItemGroupVariableSizeFitObservationV1,
    nested: ItemGroupNestedObservationV1,
    modifiers: ItemGroupModifierObservationV1,
    dressing: ItemGroupDressingObservationV1,
    modifier_container_capacity: ItemGroupModifierContainerCapacityObservationV1,
    #[serde(default)]
    charge_capacity_sentinels: Vec<ItemGroupChargeCapacitySentinelTraceV1>,
    default_containers: Vec<ItemGroupDefaultContainerTraceV1>,
    flexible_wrappers: Vec<ItemGroupFlexibleWrapperTraceV1>,
    temperature_constructors: Vec<ItemGroupTemperatureConstructorTraceV1>,
    #[serde(default)]
    rot_family: Vec<ItemGroupRotTraceV1>,
    insulated_container: ItemGroupInsulatedContainerTraceV1,
    named_snippet_categories: Vec<ItemGroupNamedSnippetCategoryTraceV1>,
    multi_pocket_wrappers: Vec<ItemGroupMultiPocketTraceV1>,
    containers: Vec<ItemGroupContainerObservationV1>,
    everyday_corpse: ItemGroupCorpseObservationV1,
    civilian_phone_case: ItemGroupPhoneCaseObservationV1,
    nonholiday_event_types: Vec<String>,
    event_distribution: Vec<ItemGroupDistributionObservationV1>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupTraceObservationV1 {
    entry_probability: u16,
    rolls_consumed: u16,
    expected_trace: Vec<String>,
    actual_trace: Vec<String>,
    downstream_draw_matches: bool,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupDistributionObservationV1 {
    ticket: u16,
    selected: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupRangeObservationV1 {
    case_id: String,
    minimum: i32,
    maximum: i32,
    target: i32,
    observed: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupModifierRngPhaseObservationV1 {
    case_id: String,
    rolls_consumed: u16,
    expected_downstream: i32,
    actual_downstream: i32,
    downstream_draw_matches: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupToolChargeObservationV1 {
    requested_charges: i32,
    tool_type: String,
    magazine_present: bool,
    magazine_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
    remaining_capacity: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupMagazineChargeTraceV1 {
    case_id: String,
    seed: u32,
    requested_charges: i32,
    item_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
    remaining_capacity: i32,
    downstream_draw: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupMagazineChargeObservationV1 {
    production_group: String,
    direct: Vec<ItemGroupMagazineChargeTraceV1>,
    production: Vec<ItemGroupMagazineChargeTraceV1>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupMagazineChargeDirectV1 {
    case_id: String,
    requested_charges: i32,
    item_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
    remaining_capacity: i32,
}

impl ItemGroupMagazineChargeObservationV1 {
    fn direct_projection(&self) -> Vec<ItemGroupMagazineChargeDirectV1> {
        self.direct
            .iter()
            .map(|trace| ItemGroupMagazineChargeDirectV1 {
                case_id: trace.case_id.clone(),
                requested_charges: trace.requested_charges,
                item_type: trace.item_type.clone(),
                ammunition_type: trace.ammunition_type.clone(),
                ammunition_remaining: trace.ammunition_remaining,
                remaining_capacity: trace.remaining_capacity,
            })
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupRepeatedToolChargeObservationV1 {
    source_group: String,
    seed: u32,
    leaf_minimum: i32,
    leaf_maximum: i32,
    replacement_requested: i32,
    tool_type: String,
    magazine_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupRepeatedToolChargeDirectV1 {
    leaf_minimum: i32,
    leaf_maximum: i32,
    replacement_requested: i32,
    tool_type: String,
    magazine_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
}

impl ItemGroupRepeatedToolChargeObservationV1 {
    fn direct_projection(&self) -> ItemGroupRepeatedToolChargeDirectV1 {
        ItemGroupRepeatedToolChargeDirectV1 {
            leaf_minimum: self.leaf_minimum,
            leaf_maximum: self.leaf_maximum,
            replacement_requested: self.replacement_requested,
            tool_type: self.tool_type.clone(),
            magazine_type: self.magazine_type.clone(),
            ammunition_type: self.ammunition_type.clone(),
            ammunition_remaining: self.ammunition_remaining,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupConstructorVariantTraceV1 {
    seed: u32,
    selected: String,
    name: String,
    description: String,
    downstream_draw: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupDescriptionExpansionObservationV1 {
    direct_input: String,
    direct_output: String,
    direct_downstream_draw: i32,
    source_group: String,
    seed: u32,
    item_type: String,
    variant_id: String,
    expanded_description: String,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupDescriptionExpansionDirectV1 {
    direct_input: String,
    direct_output: String,
}

impl ItemGroupDescriptionExpansionObservationV1 {
    fn direct_projection(&self) -> ItemGroupDescriptionExpansionDirectV1 {
        ItemGroupDescriptionExpansionDirectV1 {
            direct_input: self.direct_input.clone(),
            direct_output: self.direct_output.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupVariableSizeFitTraceV1 {
    case_id: String,
    seed: u32,
    item_type: String,
    variable_size: bool,
    fitted: bool,
    name: String,
    downstream_draw: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupVariableSizeFitObservationV1 {
    production_group: String,
    direct: Vec<ItemGroupVariableSizeFitTraceV1>,
    production: Vec<ItemGroupVariableSizeFitTraceV1>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupVariableSizeFitDirectV1 {
    case_id: String,
    variable_size: bool,
    fitted: bool,
}

impl ItemGroupVariableSizeFitObservationV1 {
    fn direct_projection(&self) -> Vec<ItemGroupVariableSizeFitDirectV1> {
        self.direct
            .iter()
            .map(|trace| ItemGroupVariableSizeFitDirectV1 {
                case_id: trace.case_id.clone(),
                variable_size: trace.variable_size,
                fitted: trace.fitted,
            })
            .collect()
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupNestedObservationV1 {
    rolls_consumed: u16,
    expected_trace: Vec<String>,
    actual_trace: Vec<String>,
    downstream_draw_matches: bool,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupModifierObservationV1 {
    damageable_raw_damage: i32,
    damageable_damage_level: i32,
    undamageable_raw_damage: i32,
    explicit_variant: String,
    detachable_magazine_present: bool,
    detachable_magazine_type: String,
    detachable_ammunition_type: String,
    detachable_ammo_remaining: i32,
    detachable_remaining_capacity: i32,
    integral_ammo_remaining: i32,
    integral_ammunition_type: String,
    integral_remaining_capacity: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupModifierContainerCapacityObservationV1 {
    seed: u32,
    container_type: String,
    payload_type: String,
    explicit_minimum: i32,
    explicit_maximum: i32,
    explicit_charges: i32,
    default_charges: i32,
    explicit_downstream_draw: i32,
    fixed_downstream_draw: i32,
    downstream_draw_matches: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupChargeCapacitySentinelTraceV1 {
    case_id: String,
    seed: u32,
    minimum: i32,
    maximum: i32,
    effective_minimum: i32,
    effective_maximum: i32,
    item_type: String,
    item_charges: i32,
    ammunition_type: String,
    ammunition_remaining: i32,
    remaining_capacity: i32,
    magazine_present: bool,
    magazine_type: String,
    wrapper_type: String,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupChargeCapacitySentinelDirectV1 {
    case_id: String,
    minimum: i32,
    maximum: i32,
    effective_minimum: i32,
    effective_maximum: i32,
}

impl ItemGroupChargeCapacitySentinelTraceV1 {
    fn direct_projection(&self) -> ItemGroupChargeCapacitySentinelDirectV1 {
        ItemGroupChargeCapacitySentinelDirectV1 {
            case_id: self.case_id.clone(),
            minimum: self.minimum,
            maximum: self.maximum,
            effective_minimum: self.effective_minimum,
            effective_maximum: self.effective_maximum,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupTemperatureConstructorTraceV1 {
    case_id: String,
    item_type: String,
    birth_turn: i32,
    has_temperature: bool,
    active: bool,
    processing_speed: i32,
    temperature_millikelvin: i32,
    specific_energy_millijoules_per_gram: i32,
    thermal_properties_present: bool,
    specific_heat_liquid_microjoules_per_gram_kelvin: i64,
    specific_heat_solid_microjoules_per_gram_kelvin: i64,
    latent_heat_microjoules_per_gram: i64,
    freezing_point_millikelvin: i32,
    ambient_specific_energy_millijoules_per_gram: i32,
    serialized_last_temp_check_present: bool,
    serialized_last_temp_check: i32,
    solid: bool,
    liquid: bool,
    hot: bool,
    cold: bool,
    frozen: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupRotTraceV1 {
    case_id: String,
    item_type: String,
    corpse: bool,
    goes_bad: bool,
    shelf_life_turns: i64,
    rot_after_ten_minutes: i64,
    rot_after_one_hour: i64,
    removal_threshold_turns: i64,
    removed_at_threshold: bool,
    removed_after_threshold: bool,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupInsulatedContainerTraceV1 {
    item_type: String,
    pocket_index: u16,
    insulation_milli: i32,
}

const ITEM_GROUP_ROT_CASES: [(&str, &str, bool, i64); 22] = [
    ("food_apple", "apple", false, 576_000),
    ("food_banana", "banana", false, 432_000),
    ("food_cheeseburger", "cheeseburger", false, 129_600),
    ("food_fish_sandwich", "fish_sandwich", false, 86_400),
    ("food_hamburger", "hamburger", false, 129_600),
    ("food_orange", "orange", false, 1_814_400),
    ("food_sandwich_cheese", "sandwich_cheese", false, 129_600),
    (
        "food_sandwich_cucumber",
        "sandwich_cucumber",
        false,
        129_600,
    ),
    ("food_sandwich_deluxe", "sandwich_deluxe", false, 115_200),
    ("food_sandwich_jam", "sandwich_jam", false, 133_200),
    (
        "food_sandwich_jam_butter",
        "sandwich_jam_butter",
        false,
        133_200,
    ),
    ("food_sandwich_pb", "sandwich_pb", false, 129_600),
    ("food_sandwich_pbf", "sandwich_pbf", false, 129_600),
    ("food_sandwich_pbh", "sandwich_pbh", false, 129_600),
    ("food_sandwich_pbj", "sandwich_pbj", false, 129_600),
    ("food_sandwich_pbm", "sandwich_pbm", false, 129_600),
    ("food_sandwich_reuben", "sandwich_reuben", false, 115_200),
    ("food_sandwich_t", "sandwich_t", false, 129_600),
    ("food_sandwich_veggy", "sandwich_veggy", false, 172_800),
    ("corpse_child_calm", "corpse_child_calm", true, 86_400),
    (
        "corpse_generic_female",
        "corpse_generic_female",
        true,
        86_400,
    ),
    ("corpse_generic_male", "corpse_generic_male", true, 86_400),
];

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupDefaultContainerTraceV1 {
    case_id: String,
    seed: u32,
    outer_type: String,
    content_types: Vec<String>,
    payload_charges: i32,
    sealed: bool,
    pocket_collapsed: bool,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupDefaultContainerDirectV1 {
    case_id: String,
    outer_type: String,
    content_types: Vec<String>,
    payload_charges: Option<i32>,
    sealed: bool,
    pocket_collapsed: bool,
}

impl ItemGroupDefaultContainerTraceV1 {
    fn direct_projection(&self) -> ItemGroupDefaultContainerDirectV1 {
        ItemGroupDefaultContainerDirectV1 {
            case_id: self.case_id.clone(),
            outer_type: self.outer_type.clone(),
            content_types: self.content_types.clone(),
            payload_charges: (self.payload_charges >= 0).then_some(self.payload_charges),
            sealed: self.sealed,
            pocket_collapsed: self.pocket_collapsed,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupFlexibleWrapperTraceV1 {
    case_id: String,
    seed: u32,
    outer_type: String,
    outer_variant: String,
    pocket_rigid: bool,
    pocket_collapsed_by_default: bool,
    pocket_collapsed: bool,
    content_types: Vec<String>,
    content_variants: Vec<String>,
    content_charges: Vec<i32>,
    outer_volume_ml: u64,
    outer_weight_g: u64,
    pocket_capacity_volume_ml: u64,
    pocket_remaining_volume_ml: u64,
    pocket_remaining_weight_g: u64,
    sealed: bool,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupFlexibleWrapperDirectV1 {
    case_id: String,
    outer_type: String,
    outer_variant: String,
    pocket_rigid: bool,
    pocket_collapsed_by_default: bool,
    pocket_collapsed: bool,
    content_types: Vec<String>,
    content_variants: Vec<String>,
    content_charges: Vec<i32>,
    outer_volume_ml: u64,
    outer_weight_g: u64,
    pocket_capacity_volume_ml: u64,
    pocket_remaining_volume_ml: u64,
    pocket_remaining_weight_g: u64,
    sealed: bool,
}

impl ItemGroupFlexibleWrapperTraceV1 {
    fn direct_projection(&self) -> ItemGroupFlexibleWrapperDirectV1 {
        ItemGroupFlexibleWrapperDirectV1 {
            case_id: self.case_id.clone(),
            outer_type: self.outer_type.clone(),
            outer_variant: self.outer_variant.clone(),
            pocket_rigid: self.pocket_rigid,
            pocket_collapsed_by_default: self.pocket_collapsed_by_default,
            pocket_collapsed: self.pocket_collapsed,
            content_types: self.content_types.clone(),
            content_variants: self.content_variants.clone(),
            content_charges: self.content_charges.clone(),
            outer_volume_ml: self.outer_volume_ml,
            outer_weight_g: self.outer_weight_g,
            pocket_capacity_volume_ml: self.pocket_capacity_volume_ml,
            pocket_remaining_volume_ml: self.pocket_remaining_volume_ml,
            pocket_remaining_weight_g: self.pocket_remaining_weight_g,
            sealed: self.sealed,
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupContainerObservationV1 {
    case_id: String,
    seed_search_limit: u32,
    valid_shapes: bool,
    minimum_top_level: u16,
    maximum_top_level: u16,
    minimum_contents: u16,
    maximum_contents: u16,
    content_orders: Vec<String>,
    outside_types: Vec<String>,
    exact_traces: Vec<ItemGroupContainerTraceV1>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupNamedSnippetSelectionTraceV1 {
    seed: u32,
    snippet_id: String,
    text: String,
    downstream_draw: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupNamedSnippetCategoryTraceV1 {
    case_id: String,
    item_type: String,
    category: String,
    choice_ids: Vec<String>,
    first_text: String,
    last_text: String,
    first_selection: ItemGroupNamedSnippetSelectionTraceV1,
    last_selection: ItemGroupNamedSnippetSelectionTraceV1,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupNamedSnippetDirectV1 {
    case_id: String,
    item_type: String,
    category: String,
    choice_ids: Vec<String>,
    first_text: String,
    last_text: String,
    first_selected_id: String,
    first_selected_text: String,
    last_selected_id: String,
    last_selected_text: String,
}

impl ItemGroupNamedSnippetCategoryTraceV1 {
    fn direct_projection(&self) -> ItemGroupNamedSnippetDirectV1 {
        ItemGroupNamedSnippetDirectV1 {
            case_id: self.case_id.clone(),
            item_type: self.item_type.clone(),
            category: self.category.clone(),
            choice_ids: self.choice_ids.clone(),
            first_text: self.first_text.clone(),
            last_text: self.last_text.clone(),
            first_selected_id: self.first_selection.snippet_id.clone(),
            first_selected_text: self.first_selection.text.clone(),
            last_selected_id: self.last_selection.snippet_id.clone(),
            last_selected_text: self.last_selection.text.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupMultiPocketTraceV1 {
    case_id: String,
    seed: u32,
    wrapper_type: String,
    payload_type: String,
    pocket_contents: Vec<Vec<String>>,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupMultiPocketDirectV1 {
    case_id: String,
    wrapper_type: String,
    payload_type: String,
    pocket_contents: Vec<Vec<String>>,
}

impl ItemGroupMultiPocketTraceV1 {
    fn direct_projection(&self) -> ItemGroupMultiPocketDirectV1 {
        ItemGroupMultiPocketDirectV1 {
            case_id: self.case_id.clone(),
            wrapper_type: self.wrapper_type.clone(),
            payload_type: self.payload_type.clone(),
            pocket_contents: self.pocket_contents.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupContainerTraceV1 {
    witness: String,
    seed: u32,
    top_level_types: Vec<String>,
    content_types: Vec<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupCorpseObservationV1 {
    seed_search_limit: u32,
    valid_shapes: bool,
    wrapper_types: Vec<String>,
    wrapper_raw_damage: Vec<i32>,
    wrapper_damage_levels: Vec<i32>,
    multiple_content_counts: bool,
    observed_pristine_content: bool,
    observed_damage_four_content: bool,
    exact_traces: Vec<ItemGroupCorpseTraceV1>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupCorpseTraceV1 {
    witness: String,
    seed: u32,
    wrapper_type: String,
    wrapper_raw_damage: i32,
    wrapper_damage_level: i32,
    wrapper_pocket_forbidden: bool,
    wrapper_pocket_no_unload: bool,
    unloadable_content_count: usize,
    content_types: Vec<String>,
    content_raw_damage: Vec<i32>,
    content_damage_levels: Vec<i32>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct ItemGroupCorpseDirectTraceV1 {
    wrapper_type: String,
    wrapper_raw_damage: i32,
    wrapper_damage_level: i32,
    wrapper_pocket_forbidden: bool,
    wrapper_pocket_no_unload: bool,
    unloadable_content_count: usize,
    content_types: Vec<String>,
    content_raw_damage: Vec<i32>,
    content_damage_levels: Vec<i32>,
}

impl ItemGroupCorpseTraceV1 {
    fn direct_projection(&self) -> ItemGroupCorpseDirectTraceV1 {
        ItemGroupCorpseDirectTraceV1 {
            wrapper_type: self.wrapper_type.clone(),
            wrapper_raw_damage: self.wrapper_raw_damage,
            wrapper_damage_level: self.wrapper_damage_level,
            wrapper_pocket_forbidden: self.wrapper_pocket_forbidden,
            wrapper_pocket_no_unload: self.wrapper_pocket_no_unload,
            unloadable_content_count: self.unloadable_content_count,
            content_types: self.content_types.clone(),
            content_raw_damage: self.content_raw_damage.clone(),
            content_damage_levels: self.content_damage_levels.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupPhoneCaseObservationV1 {
    seed_search_limit: u32,
    valid_shapes: bool,
    phone_types: Vec<String>,
    observed_empty_efiles: bool,
    observed_many_efiles: bool,
    exact_traces: Vec<ItemGroupPhoneCaseTraceV1>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupPhoneCaseTraceV1 {
    witness: String,
    seed: u32,
    wrapper_type: String,
    wrapper_variant: String,
    wrapper_any_pocket_sealed: bool,
    wrapper_remaining_volume_ml: i64,
    wrapper_remaining_weight_g: i64,
    phone_type: String,
    phone_charges: i32,
    phone_ammo_remaining: i32,
    phone_ammunition_type: String,
    phone_raw_damage: i32,
    efile_types: Vec<String>,
    efile_raw_damage: Vec<i32>,
    downstream_draw: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct MapgenOracleScenarioV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    expected_observation: MapgenOracleObservationV1,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenOracleObservationV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    kernel: String,
    matching: Vec<MapgenMatchObservationV1>,
    rotatable: Vec<MapgenRotationObservationV1>,
    linear: Vec<MapgenRotationObservationV1>,
    palette: MapgenPaletteObservationV1,
    static_template: MapgenStaticTemplateObservationV1,
    start_location: MapgenStartLocationObservationV1,
    city: MapgenCityObservationV1,
    road: MapgenRoadObservationV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenMatchObservationV1 {
    case_id: String,
    query: String,
    terrain_id: String,
    match_type: String,
    matches: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenRotationObservationV1 {
    direction: String,
    terrain_id: String,
    mapgen_id: String,
    rotation: i32,
    marker_x: i32,
    marker_y: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenPaletteObservationV1 {
    palette_id: String,
    key: String,
    key_has_terrain: bool,
    piece_phases: Vec<String>,
    mapgen_size_x: i32,
    mapgen_size_y: i32,
    setup_completed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenStaticTemplateObservationV1 {
    width_tiles: i32,
    height_tiles: i32,
    source_marker_x: i32,
    source_marker_y: i32,
    background_terrain_id: String,
    marker_terrain_id: String,
    marker_furniture_id: String,
    generated_background_terrain_id: String,
    generated_marker_terrain_id: String,
    generated_marker_furniture_id: String,
    generated_rows: Vec<String>,
    piece_phases: Vec<String>,
    setup_completed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenStartLocationObservationV1 {
    start_location_id: String,
    target_count: i32,
    chosen_target_index: i32,
    chosen_target_omt: String,
    chosen_target_match_type: String,
    chosen_target_parameter_count: i32,
    requires_city: bool,
    city_size_minimum: i32,
    city_size_maximum: i32,
    city_distance_minimum: i32,
    city_distance_maximum: i32,
    allowed_z_minimum: i32,
    allowed_z_maximum: i32,
    flags: Vec<String>,
    runtime_selectable_without_cities: bool,
    candidate_identity_ids: Vec<String>,
    matching_candidate_ids: Vec<String>,
    selected_candidate_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenCityObservationV1 {
    settings_id: String,
    city_size: i32,
    city_spacing: i32,
    is_megacity: bool,
    center_x: i32,
    center_y: i32,
    size: i32,
    point_x: Vec<i32>,
    point_y: Vec<i32>,
    edge_distances: Vec<i32>,
    start_distances: Vec<i32>,
    random_count_floor: i32,
    random_count_ceiling: i32,
    minimum_generated_size: i32,
    maximum_generated_size: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MapgenRoadObservationV1 {
    point_x: Vec<i32>,
    point_y: Vec<i32>,
    mst_left: Vec<i32>,
    mst_right: Vec<i32>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct MapgenDirectObservationV1 {
    matching: Vec<MapgenMatchObservationV1>,
    rotatable: Vec<MapgenRotationObservationV1>,
    linear: Vec<MapgenRotationObservationV1>,
    static_template: MapgenStaticTemplateObservationV1,
    start_location: MapgenStartLocationObservationV1,
    city: MapgenCityObservationV1,
    road: MapgenRoadObservationV1,
}

struct RustStaticTemplateTiles {
    background: String,
    marker_terrain: String,
    marker_furniture: String,
    generated_rows: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OracleCacheV1 {
    format_version: u16,
    baseline_commit: String,
    upstream_tree: String,
    adapter_hash: String,
    binary_hash: String,
}

struct OracleRunArtifacts {
    root: PathBuf,
}

impl Drop for OracleRunArtifacts {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub(crate) fn check(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() > 2 {
        return Err(
            "usage: cargo xtask cpp-oracle-check [scenario.json] [upstream-checkout]".into(),
        );
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("tools crate is not nested beneath the workspace")?;
    let oracle_root = workspace.join("target/cpp-oracle");
    fs::create_dir_all(&oracle_root)?;
    let oracle_lock = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(oracle_root.join(".lock"))?;
    oracle_lock.lock()?;
    let scenario_path = arguments
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join(DEFAULT_SCENARIO));
    let upstream = arguments
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_upstream(workspace));
    let kernel = load_kernel(&scenario_path)?;
    validate_upstream(&upstream)?;

    let binary = prepare_binary(workspace, &upstream)?;
    match kernel.as_str() {
        KERNEL => {
            let scenario = load_scenario(&scenario_path)?;
            let observation = run_binary(workspace, &upstream, &binary)?;
            compare(&scenario, &observation)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&observation)
                    .map_err(|error| format!("could not encode oracle observation: {error}"))?
            );
            eprintln!(
                "C++ oracle verified {} cases against pinned {}",
                observation.cases.len(),
                BASELINE_COMMIT
            );
        }
        ITEM_GROUP_KERNEL => {
            let scenario = load_item_group_scenario(&scenario_path)?;
            let observation = run_item_group_binary(workspace, &upstream, &binary)?;
            compare_item_group(&scenario, &observation)?;
            let rust_tool_charges = rust_item_group_tool_charge_observation()?;
            compare_direct_observation(
                "item-group detachable tool charges",
                &observation.tool_charges,
                &rust_tool_charges,
            )?;
            compare_direct_observation(
                "item-group integral magazine charges",
                &observation.magazine_charges.direct_projection(),
                &rust_item_group_magazine_charge_observation()?,
            )?;
            compare_direct_observation(
                "item-group repeated detachable tool charges",
                &observation.repeated_tool_charges.direct_projection(),
                &rust_repeated_item_group_tool_charge_observation()?,
            )?;
            compare_direct_observation(
                "item-group ammunition and magazine dressing",
                &observation.dressing.direct_projection(),
                &item_group_dressing::rust_observation()?,
            )?;
            compare_direct_observation(
                "item description snippet expansion",
                &observation.description_expansion.direct_projection(),
                &rust_item_group_description_expansion_observation()?,
            )?;
            compare_direct_observation(
                "item-group variable-size FIT transition",
                &observation.variable_size_fit.direct_projection(),
                &rust_item_group_variable_size_fit_observation(),
            )?;
            compare_direct_observation(
                "item-group default-container ownership",
                &observation
                    .default_containers
                    .iter()
                    .map(ItemGroupDefaultContainerTraceV1::direct_projection)
                    .collect::<Vec<_>>(),
                &rust_item_group_default_container_observation()?,
            )?;
            compare_direct_observation(
                "item-group flexible-wrapper containment",
                &observation
                    .flexible_wrappers
                    .iter()
                    .map(ItemGroupFlexibleWrapperTraceV1::direct_projection)
                    .collect::<Vec<_>>(),
                &rust_item_group_flexible_wrapper_observation()?,
            )?;
            compare_direct_observation(
                "item constructor temperature state",
                &observation.temperature_constructors,
                &rust_item_group_temperature_constructor_observation(workspace)?,
            )?;
            compare_direct_observation(
                "item rot shelf life, ambient increments, and removal boundaries",
                &observation.rot_family,
                &rust_item_group_rot_observation(workspace)?,
            )?;
            compare_direct_observation(
                "item-group insulated container metadata",
                &observation.insulated_container,
                &rust_item_group_insulated_container_observation(workspace)?,
            )?;
            compare_direct_observation(
                "item-group charge-capacity sentinels",
                &observation
                    .charge_capacity_sentinels
                    .iter()
                    .map(ItemGroupChargeCapacitySentinelTraceV1::direct_projection)
                    .collect::<Vec<_>>(),
                &rust_item_group_charge_capacity_sentinel_observation()?,
            )?;
            compare_direct_observation(
                "item-group named snippet category selection",
                &observation
                    .named_snippet_categories
                    .iter()
                    .map(ItemGroupNamedSnippetCategoryTraceV1::direct_projection)
                    .collect::<Vec<_>>(),
                &rust_item_group_named_snippet_observation(workspace)?,
            )?;
            compare_direct_observation(
                "item-group multi-pocket first-compatible selection",
                &observation
                    .multi_pocket_wrappers
                    .iter()
                    .map(ItemGroupMultiPocketTraceV1::direct_projection)
                    .collect::<Vec<_>>(),
                &rust_item_group_multi_pocket_observation()?,
            )?;
            compare_direct_observation(
                "item-group static corpse ownership",
                &observation
                    .everyday_corpse
                    .exact_traces
                    .iter()
                    .map(ItemGroupCorpseTraceV1::direct_projection)
                    .collect::<Vec<_>>(),
                &rust_item_group_static_corpse_observation(
                    &observation.everyday_corpse.exact_traces,
                )?,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&observation)
                    .map_err(|error| format!("could not encode oracle observation: {error}"))?
            );
            eprintln!(
                "C++ oracle and direct Rust comparison verified bounded item-group generation against pinned {}",
                BASELINE_COMMIT
            );
        }
        MAPGEN_KERNEL => {
            let scenario = load_mapgen_scenario(&scenario_path)?;
            let observation = run_mapgen_binary(workspace, &upstream, &binary)?;
            compare_mapgen(&scenario, &observation)?;
            let rust_observation = rust_mapgen_direct_observation(workspace)?;
            compare_direct_observation(
                "mapgen/OMT/start-location",
                &direct_mapgen_projection(&observation),
                &rust_observation,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&observation)
                    .map_err(|error| format!("could not encode oracle observation: {error}"))?
            );
            eprintln!(
                "C++ oracle and direct Rust comparison verified bounded static mapgen semantics against pinned {}",
                BASELINE_COMMIT
            );
        }
        _ => return Err(format!("unsupported C++ oracle kernel: {kernel}").into()),
    }
    Ok(())
}

fn default_upstream(workspace: &Path) -> PathBuf {
    let sibling = workspace.join("../Cataclysm-DDA");
    if sibling.is_dir() {
        return sibling;
    }
    workspace
        .ancestors()
        .map(|ancestor| ancestor.join("Cataclysm-DDA"))
        .find(|candidate| candidate.is_dir())
        .unwrap_or(sibling)
}

fn load_scenario(path: &Path) -> Result<OracleScenarioV1, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path)?;
    let scenario: OracleScenarioV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid oracle scenario {}: {error}", path.display()))?;
    validate_scenario(&scenario)?;
    Ok(scenario)
}

fn load_kernel(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path)?;
    let header: OracleScenarioHeader = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid oracle scenario {}: {error}", path.display()))?;
    if header.format_version != ORACLE_FORMAT_VERSION
        || header.baseline_commit != BASELINE_COMMIT
        || header.upstream_tree != UPSTREAM_TREE
        || header.expected_observation.is_null()
    {
        return Err("oracle scenario version, baseline, or content tree mismatch".into());
    }
    Ok(header.kernel)
}

fn load_item_group_scenario(
    path: &Path,
) -> Result<ItemGroupOracleScenarioV1, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path)?;
    let scenario: ItemGroupOracleScenarioV1 = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "invalid item-group oracle scenario {}: {error}",
            path.display()
        )
    })?;
    validate_item_group_scenario(&scenario)?;
    Ok(scenario)
}

fn load_mapgen_scenario(path: &Path) -> Result<MapgenOracleScenarioV1, Box<dyn std::error::Error>> {
    let bytes = read_bounded(path)?;
    let scenario: MapgenOracleScenarioV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid mapgen oracle scenario {}: {error}", path.display()))?;
    validate_mapgen_scenario(&scenario)?;
    Ok(scenario)
}

fn validate_scenario(scenario: &OracleScenarioV1) -> Result<(), Box<dyn std::error::Error>> {
    if scenario.format_version != ORACLE_FORMAT_VERSION
        || scenario.baseline_commit != BASELINE_COMMIT
        || scenario.upstream_tree != UPSTREAM_TREE
        || scenario.kernel != KERNEL
    {
        return Err("oracle scenario version, baseline, content tree, or kernel mismatch".into());
    }
    validate_observation(&scenario.expected_observation)?;
    let expected_cases = [
        ("shorter", "test_screwdriver"),
        ("equal", "test_sonic_screwdriver"),
        ("longer", "test_clumsy_sword"),
    ];
    if scenario.expected_observation.cases.len() != expected_cases.len()
        || scenario
            .expected_observation
            .cases
            .iter()
            .zip(expected_cases)
            .any(|(actual, expected)| actual.case_id != expected.0 || actual.item_id != expected.1)
    {
        return Err("oracle scenario must contain the complete ordered kernel case set".into());
    }
    Ok(())
}

fn validate_observation(
    observation: &OracleObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if observation.format_version != ORACLE_FORMAT_VERSION
        || observation.baseline_commit != BASELINE_COMMIT
        || observation.upstream_tree != UPSTREAM_TREE
        || observation.kernel != KERNEL
    {
        return Err(
            "oracle observation version, baseline, content tree, or kernel mismatch".into(),
        );
    }
    if observation.pocket.pocket_type != "CONTAINER"
        || observation.pocket.max_item_length_mm <= 0
        || observation.pocket.volume_capacity_ml <= 0
        || observation.pocket.weight_capacity_g <= 0
        || observation.cases.is_empty()
        || observation.cases.len() > MAX_CASES
    {
        return Err("oracle observation has invalid pocket metadata or case bounds".into());
    }
    let mut case_ids = BTreeSet::new();
    for case in &observation.cases {
        if case.case_id.is_empty()
            || case.case_id.len() > 64
            || case.item_id.is_empty()
            || case.item_id.len() > 128
            || case.item_length_mm <= 0
            || case.contain_code < 0
            || case.contain_code > 10
            || case.contain_code_name.is_empty()
            || case.contain_code_name.len() > 64
            || case.reason.len() > 256
            || !case_ids.insert(case.case_id.as_str())
            || (case.success && (case.contain_code != 0 || !case.reason.is_empty()))
            || (!case.success && case.contain_code == 0)
        {
            return Err(format!("invalid oracle case observation: {}", case.case_id).into());
        }
    }
    Ok(())
}

fn validate_item_group_scenario(
    scenario: &ItemGroupOracleScenarioV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if scenario.format_version != ORACLE_FORMAT_VERSION
        || scenario.baseline_commit != BASELINE_COMMIT
        || scenario.upstream_tree != UPSTREAM_TREE
        || scenario.kernel != ITEM_GROUP_KERNEL
    {
        return Err("item-group oracle scenario identity mismatch".into());
    }
    validate_item_group_observation(&scenario.expected_observation)
}

fn validate_item_group_observation(
    observation: &ItemGroupOracleObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if observation.format_version != ORACLE_FORMAT_VERSION
        || observation.baseline_commit != BASELINE_COMMIT
        || observation.upstream_tree != UPSTREAM_TREE
        || observation.kernel != ITEM_GROUP_KERNEL
    {
        return Err("item-group oracle observation identity mismatch".into());
    }
    let collection_trace = ["first", "conditional", "last"];
    if observation.collection.entry_probability != 50
        || observation.collection.rolls_consumed != 3
        || !observation.collection.downstream_draw_matches
        || observation.collection.expected_trace != collection_trace
        || observation.collection.actual_trace != collection_trace
    {
        return Err("item-group collection observation is not the complete ordered case".into());
    }
    let distribution = [
        (1, "low"),
        (2, "low"),
        (3, "middle"),
        (5, "middle"),
        (6, "high"),
        (10, "high"),
    ];
    if observation.distribution.len() != distribution.len()
        || observation
            .distribution
            .iter()
            .zip(distribution)
            .any(|(actual, expected)| actual.ticket != expected.0 || actual.selected != expected.1)
    {
        return Err("item-group distribution observation omits an interval boundary".into());
    }
    let expected_counts = [
        ("fixed", 3, 3, 3),
        ("range_minimum", 2, 4, 2),
        ("range_maximum", 2, 4, 4),
    ];
    let expected_charges = [
        ("fixed", 4, 4, 4),
        ("zero_clamped_to_one", 0, 0, 1),
        ("range_minimum", 1, 4, 1),
        ("range_maximum", 1, 4, 4),
    ];
    if !ranges_match(&observation.counts, &expected_counts)
        || !ranges_match(&observation.charges, &expected_charges)
    {
        return Err("item-group count or charges observation is incomplete".into());
    }
    let expected_tool_charges = [
        (0, "null", 0),
        (1, "battery", 1),
        (56, "battery", 56),
        (100, "battery", 56),
    ];
    if observation.tool_charges.len() != expected_tool_charges.len()
        || observation
            .tool_charges
            .iter()
            .zip(expected_tool_charges)
            .any(|(trace, (requested, ammunition_type, remaining))| {
                trace.requested_charges != requested
                    || trace.tool_type != "wearable_light"
                    || !trace.magazine_present
                    || trace.magazine_type != "medium_battery_cell"
                    || trace.ammunition_type != ammunition_type
                    || trace.ammunition_remaining != remaining
                    || trace.remaining_capacity != 0
            })
    {
        return Err(format!(
            "item-group detachable tool-charge traces are incomplete: {}",
            serde_json::to_string(&observation.tool_charges)?
        )
        .into());
    }
    let expected_magazine_charges = [
        (
            "light_0",
            8_675_309,
            0,
            "light_battery_cell",
            "null",
            0,
            16,
            8_054,
        ),
        (
            "light_1",
            8_675_309,
            1,
            "light_battery_cell",
            "battery",
            1,
            15,
            8_012,
        ),
        (
            "light_16",
            8_675_309,
            16,
            "light_battery_cell",
            "battery",
            16,
            0,
            8_012,
        ),
        (
            "light_100",
            8_675_309,
            100,
            "light_battery_cell",
            "battery",
            16,
            0,
            8_012,
        ),
        (
            "ultralight_overflow",
            8_675_309,
            100,
            "light_minus_battery_cell",
            "battery",
            2,
            0,
            8_012,
        ),
        (
            "production_empty_light",
            378,
            -1,
            "light_battery_cell",
            "null",
            0,
            16,
            4_351,
        ),
        (
            "production_partial_light",
            19,
            -1,
            "light_battery_cell",
            "battery",
            4,
            12,
            6_734,
        ),
        (
            "production_full_light",
            1,
            -1,
            "light_battery_cell",
            "battery",
            16,
            0,
            272,
        ),
        (
            "production_full_ultralight",
            4,
            -1,
            "light_minus_battery_cell",
            "battery",
            2,
            0,
            7_453,
        ),
    ];
    if observation.magazine_charges.production_group != "ammo_light_batteries"
        || observation.magazine_charges.direct.len() != 5
        || observation.magazine_charges.production.len() != 4
        || observation
            .magazine_charges
            .direct
            .iter()
            .chain(&observation.magazine_charges.production)
            .zip(expected_magazine_charges)
            .any(|(trace, expected)| {
                trace.case_id != expected.0
                    || trace.seed != expected.1
                    || trace.requested_charges != expected.2
                    || trace.item_type != expected.3
                    || trace.ammunition_type != expected.4
                    || trace.ammunition_remaining != expected.5
                    || trace.remaining_capacity != expected.6
                    || trace.downstream_draw != expected.7
            })
    {
        return Err("item-group integral-magazine charge traces are incomplete".into());
    }
    if observation.repeated_tool_charges.source_group != "accesories_personal_unisex_child"
        || observation.repeated_tool_charges.seed == 0
        || observation.repeated_tool_charges.leaf_minimum != 0
        || observation.repeated_tool_charges.leaf_maximum != 100
        || observation.repeated_tool_charges.replacement_requested != 1
        || observation.repeated_tool_charges.tool_type != "wearable_light"
        || observation.repeated_tool_charges.magazine_type != "medium_battery_cell"
        || observation.repeated_tool_charges.ammunition_type != "battery"
        || observation.repeated_tool_charges.ammunition_remaining != 1
    {
        return Err(format!(
            "item-group repeated detachable tool-charge trace is incomplete: {}",
            serde_json::to_string(&observation.repeated_tool_charges)?
        )
        .into());
    }
    if observation.modifier_rng_phase.case_id != "direct_fixed_count"
        || observation.modifier_rng_phase.rolls_consumed != 4
        || observation.modifier_rng_phase.expected_downstream
            != observation.modifier_rng_phase.actual_downstream
        || !observation.modifier_rng_phase.downstream_draw_matches
    {
        return Err(format!(
            "item-group modifier RNG phase is incomplete: expected downstream {}, actual {}",
            observation.modifier_rng_phase.expected_downstream,
            observation.modifier_rng_phase.actual_downstream
        )
        .into());
    }
    let base_description = "A rock the size of a baseball.  Makes a decent melee weapon, and is also good for throwing at enemies.";
    let expected_constructor_variants = [
        (
            3,
            "test_rock_blue",
            "blue test_rock",
            format!("{base_description}  It's a blue test rock"),
            9_862,
        ),
        (
            1,
            "test_rock_green",
            "green test_rock",
            format!("{base_description}  It's a green test rock"),
            6_855,
        ),
    ];
    if observation.constructor_variants.len() != expected_constructor_variants.len()
        || observation
            .constructor_variants
            .iter()
            .zip(expected_constructor_variants)
            .any(|(trace, expected)| {
                trace.seed != expected.0
                    || trace.selected != expected.1
                    || trace.name != expected.2
                    || trace.description != expected.3
                    || trace.downstream_draw != expected.4
            })
    {
        return Err("item-group constructor variant traces are incomplete".into());
    }
    let description_expansion = &observation.description_expansion;
    if description_expansion.direct_input != "Foo <lt>lt<gt> <unknown>"
        || description_expansion.direct_output != "Foo <lt> <unknown>"
        || !(0..=9_999).contains(&description_expansion.direct_downstream_draw)
        || description_expansion.source_group != "accessory_necklace"
        || description_expansion.seed == 0
        || description_expansion.item_type != "holy_symbol"
        || description_expansion.variant_id != "saint_necklace"
        || !description_expansion
            .expanded_description
            .starts_with("A necklace made of a fine gold chain")
        || description_expansion
            .expanded_description
            .contains("<catholic_saints>")
        || !(0..=9_999).contains(&description_expansion.downstream_draw)
    {
        return Err("item-group description expansion characterization is incomplete".into());
    }
    let expected_fit_traces = [
        (
            "non_variable_control",
            2,
            "test_pipe",
            false,
            false,
            "TEST pipe",
            7_599,
        ),
        (
            "variable_unfitted",
            1,
            "leg_sheath6",
            true,
            false,
            "throwing knives leg sheath (poor fit)",
            6_855,
        ),
        (
            "variable_fitted",
            2,
            "leg_sheath6",
            true,
            true,
            "throwing knives leg sheath",
            7_599,
        ),
        (
            "production_unfitted",
            219,
            "leg_sheath6",
            true,
            false,
            "throwing knives leg sheath (poor fit)",
            3_293,
        ),
        (
            "production_fitted",
            97,
            "leg_sheath6",
            true,
            true,
            "throwing knives leg sheath",
            8_155,
        ),
    ];
    if observation.variable_size_fit.production_group != "accessory_weaponcarry"
        || observation.variable_size_fit.direct.len() != 3
        || observation.variable_size_fit.production.len() != 2
        || observation
            .variable_size_fit
            .direct
            .iter()
            .chain(&observation.variable_size_fit.production)
            .zip(expected_fit_traces)
            .any(|(trace, expected)| {
                trace.case_id != expected.0
                    || trace.seed != expected.1
                    || trace.item_type != expected.2
                    || trace.variable_size != expected.3
                    || trace.fitted != expected.4
                    || trace.name != expected.5
                    || trace.downstream_draw != expected.6
            })
    {
        return Err("item-group variable-size FIT characterization is incomplete".into());
    }
    let nested_trace = ["child_conditional", "child_always", "root_last"];
    if observation.nested.rolls_consumed != 4
        || !observation.nested.downstream_draw_matches
        || observation.nested.expected_trace != nested_trace
        || observation.nested.actual_trace != nested_trace
    {
        return Err("item-group nested observation does not preserve the shared RNG stream".into());
    }
    if observation.modifiers.damageable_raw_damage != 1_000
        || observation.modifiers.damageable_damage_level != 2
        || observation.modifiers.undamageable_raw_damage != 0
        || observation.modifiers.explicit_variant != "flag_shirt"
        || !observation.modifiers.detachable_magazine_present
        || observation.modifiers.detachable_magazine_type != "glockmag_10"
        || observation.modifiers.detachable_ammunition_type != "9mm"
        || observation.modifiers.detachable_ammo_remaining != 10
        || observation.modifiers.detachable_remaining_capacity != 0
        || observation.modifiers.integral_ammo_remaining != 20
        || observation.modifiers.integral_ammunition_type != "match"
        || observation.modifiers.integral_remaining_capacity != 0
    {
        return Err("item-group modifier characterization is incomplete".into());
    }
    item_group_dressing::validate(&observation.dressing)?;
    if observation.modifier_container_capacity.seed != 31_415
        || observation.modifier_container_capacity.container_type != "bottle_plastic"
        || observation.modifier_container_capacity.payload_type != "water_clean"
        || observation.modifier_container_capacity.explicit_minimum != 50
        || observation.modifier_container_capacity.explicit_maximum != 80
        || observation.modifier_container_capacity.explicit_charges <= 0
        || observation.modifier_container_capacity.explicit_charges
            != observation.modifier_container_capacity.default_charges
        || observation
            .modifier_container_capacity
            .explicit_downstream_draw
            != 8_831
        || observation
            .modifier_container_capacity
            .explicit_downstream_draw
            != observation
                .modifier_container_capacity
                .fixed_downstream_draw
        || !observation
            .modifier_container_capacity
            .downstream_draw_matches
    {
        return Err("modifier-container capacity characterization is incomplete".into());
    }
    let expected_charge_sentinels = [
        (
            "integral_tool_minimum",
            78,
            0,
            -1,
            0,
            85,
            "eink_tablet_pc",
            0,
            "null",
            0,
            85,
            false,
            "",
            "",
            9_885,
        ),
        (
            "integral_tool_maximum",
            31_415,
            0,
            -1,
            0,
            85,
            "eink_tablet_pc",
            0,
            "battery",
            85,
            0,
            false,
            "",
            "",
            6_092,
        ),
        (
            "ordinary_unresolved",
            31_415,
            4,
            -1,
            -1,
            -1,
            "rock",
            1,
            "rock",
            1,
            0,
            false,
            "",
            "",
            4_053,
        ),
        (
            "detachable_tool_minimum",
            24,
            0,
            -1,
            0,
            56,
            "wearable_light",
            0,
            "null",
            0,
            0,
            true,
            "medium_battery_cell",
            "",
            7_471,
        ),
        (
            "detachable_tool_maximum",
            7,
            0,
            -1,
            0,
            56,
            "wearable_light",
            0,
            "battery",
            56,
            0,
            true,
            "medium_battery_cell",
            "",
            331,
        ),
        (
            "detachable_explicit_over_capacity",
            31_415,
            0,
            100,
            0,
            100,
            "wearable_light",
            0,
            "battery",
            56,
            0,
            true,
            "medium_battery_cell",
            "",
            6_092,
        ),
        (
            "magazine_minimum",
            24,
            0,
            -1,
            0,
            16,
            "light_battery_cell",
            0,
            "null",
            0,
            16,
            false,
            "",
            "",
            3_656,
        ),
        (
            "magazine_maximum",
            6,
            0,
            -1,
            0,
            16,
            "light_battery_cell",
            0,
            "battery",
            16,
            0,
            false,
            "",
            "",
            2_988,
        ),
        (
            "container_minimum",
            3,
            1,
            -1,
            1,
            2,
            "water_clean",
            1,
            "water_clean",
            1,
            0,
            false,
            "",
            "bottle_plastic",
            5_029,
        ),
        (
            "container_maximum",
            1,
            1,
            -1,
            1,
            2,
            "water_clean",
            2,
            "water_clean",
            2,
            0,
            false,
            "",
            "bottle_plastic",
            4_747,
        ),
        (
            "lower_sentinel_minimum",
            4,
            -1,
            4,
            0,
            4,
            "40x46mm_m1006",
            1,
            "40x46mm_m1006",
            1,
            0,
            false,
            "",
            "",
            4_406,
        ),
        (
            "lower_sentinel_maximum",
            2,
            -1,
            4,
            0,
            4,
            "40x46mm_m1006",
            4,
            "40x46mm_m1006",
            4,
            0,
            false,
            "",
            "",
            7_814,
        ),
    ];
    if observation.charge_capacity_sentinels.len() != expected_charge_sentinels.len()
        || observation
            .charge_capacity_sentinels
            .iter()
            .zip(expected_charge_sentinels)
            .any(|(actual, expected)| {
                let (
                    case_id,
                    seed,
                    minimum,
                    maximum,
                    effective_minimum,
                    effective_maximum,
                    item_type,
                    item_charges,
                    ammunition_type,
                    ammunition_remaining,
                    remaining_capacity,
                    magazine_present,
                    magazine_type,
                    wrapper_type,
                    downstream_draw,
                ) = expected;
                actual.case_id != case_id
                    || actual.seed != seed
                    || actual.minimum != minimum
                    || actual.maximum != maximum
                    || actual.effective_minimum != effective_minimum
                    || actual.effective_maximum != effective_maximum
                    || actual.item_type != item_type
                    || actual.item_charges != item_charges
                    || actual.ammunition_type != ammunition_type
                    || actual.ammunition_remaining != ammunition_remaining
                    || actual.remaining_capacity != remaining_capacity
                    || actual.magazine_present != magazine_present
                    || actual.magazine_type != magazine_type
                    || actual.wrapper_type != wrapper_type
                    || actual.downstream_draw != downstream_draw
            })
    {
        return Err(format!(
            "charge-capacity sentinel characterization is incomplete: {:?}",
            observation.charge_capacity_sentinels
        )
        .into());
    }
    let expected_default_containers = [
        (
            "direct_water",
            31_415,
            "bottle_plastic",
            vec!["water_clean"],
            2,
            true,
            true,
            8_831,
        ),
        (
            "direct_aspirin",
            31_415,
            "bottle_plastic_pill_painkiller",
            vec!["aspirin"],
            0,
            false,
            true,
            8_831,
        ),
        (
            "modifier_aspirin",
            31_415,
            "bottle_plastic_pill_painkiller",
            vec!["aspirin"],
            0,
            false,
            true,
            8_831,
        ),
        (
            "suppressed_aspirin",
            31_415,
            "aspirin",
            vec![],
            -1,
            false,
            false,
            8_831,
        ),
        (
            "explicit_container_default",
            31_415,
            "bottle_plastic_pill_painkiller",
            vec!["ibuprofen", "aspirin"],
            0,
            false,
            true,
            8_323,
        ),
        (
            "production_aspirin_minimum",
            86,
            "bottle_plastic_pill_painkiller",
            vec!["aspirin"],
            0,
            false,
            true,
            7_093,
        ),
        (
            "production_aspirin_maximum",
            5,
            "bottle_plastic_pill_painkiller",
            vec!["aspirin"; 20],
            0,
            false,
            true,
            6_790,
        ),
    ];
    if observation.default_containers.len() != expected_default_containers.len()
        || observation
            .default_containers
            .iter()
            .zip(expected_default_containers)
            .any(
                |(
                    actual,
                    (
                        case_id,
                        seed,
                        outer_type,
                        content_types,
                        payload_charges,
                        sealed,
                        pocket_collapsed,
                        downstream_draw,
                    ),
                )| {
                    actual.case_id != case_id
                        || actual.seed != seed
                        || actual.outer_type != outer_type
                        || actual
                            .content_types
                            .iter()
                            .map(String::as_str)
                            .ne(content_types)
                        || actual.payload_charges != payload_charges
                        || actual.sealed != sealed
                        || actual.pocket_collapsed != pocket_collapsed
                        || actual.downstream_draw != downstream_draw
                },
            )
    {
        return Err(format!(
            "item-group default-container characterization is incomplete: {:?}",
            observation.default_containers
        )
        .into());
    }
    let [chaw_minimum, chaw_maximum, chewing_gum] = observation.flexible_wrappers.as_slice() else {
        return Err("flexible-wrapper characterization must retain both boundaries and the collapsed production case".into());
    };
    if chaw_minimum.case_id != "production_chaw_minimum"
        || chaw_minimum.seed != 30
        || chaw_minimum.outer_type != "wrapper"
        || !chaw_minimum.outer_variant.is_empty()
        || chaw_minimum.pocket_rigid
        || chaw_minimum.pocket_collapsed_by_default
        || !chaw_minimum.pocket_collapsed
        || chaw_minimum.content_types != ["chaw"]
        || chaw_minimum.content_variants != [""]
        || chaw_minimum.content_charges != [0]
        || chaw_minimum.outer_volume_ml != 50
        || chaw_minimum.outer_weight_g != 7
        || chaw_minimum.pocket_capacity_volume_ml != 2_500
        || chaw_minimum.pocket_remaining_volume_ml != 2_496
        || chaw_minimum.pocket_remaining_weight_g != 5_996
        || chaw_minimum.sealed
        || chaw_minimum.downstream_draw != 8_189
        || chaw_maximum.case_id != "production_chaw_maximum"
        || chaw_maximum.seed != 5
        || chaw_maximum.outer_type != "wrapper"
        || !chaw_maximum.outer_variant.is_empty()
        || chaw_maximum.pocket_rigid
        || chaw_maximum.pocket_collapsed_by_default
        || !chaw_maximum.pocket_collapsed
        || chaw_maximum.content_types != vec![String::from("chaw"); 20]
        || chaw_maximum.content_variants != vec![String::new(); 20]
        || chaw_maximum.content_charges != vec![0; 20]
        || chaw_maximum.outer_volume_ml != 85
        || chaw_maximum.outer_weight_g != 83
        || chaw_maximum.pocket_capacity_volume_ml != 2_500
        || chaw_maximum.pocket_remaining_volume_ml != 2_420
        || chaw_maximum.pocket_remaining_weight_g != 5_920
        || chaw_maximum.sealed
        || chaw_maximum.downstream_draw != 6_790
        || chewing_gum.case_id != "production_chewing_gum"
        || chewing_gum.seed != 1
        || chewing_gum.outer_type != "blister_pack_small"
        || chewing_gum.outer_variant != "blister_pack_gum"
        || chewing_gum.pocket_rigid
        || !chewing_gum.pocket_collapsed_by_default
        || !chewing_gum.pocket_collapsed
        || chewing_gum.content_types != vec![String::from("gum"); 12]
        || chewing_gum.content_variants != vec![String::from("gum_watermelon"); 12]
        || chewing_gum.content_charges != vec![0; 12]
        || chewing_gum.outer_volume_ml != 31
        || chewing_gum.outer_weight_g != 41
        || chewing_gum.pocket_capacity_volume_ml != 50
        || chewing_gum.pocket_remaining_volume_ml != 26
        || chewing_gum.pocket_remaining_weight_g != 14
        || chewing_gum.sealed
        || chewing_gum.downstream_draw != 6_872
    {
        return Err(format!(
            "flexible-wrapper characterization is incomplete: {:?}",
            observation.flexible_wrappers
        )
        .into());
    }
    let expected_temperature_constructors = [
        (
            "materialless_comestible",
            "chaw",
            true,
            true,
            600,
            true,
            true,
            false,
        ),
        (
            "material_comestible",
            "water_clean",
            true,
            true,
            600,
            true,
            false,
            true,
        ),
        (
            "field_blocker_material",
            "caff_gum",
            true,
            true,
            600,
            true,
            true,
            false,
        ),
        (
            "weighted_material",
            "saline",
            true,
            true,
            600,
            true,
            false,
            true,
        ),
        (
            "custom_freezing_comestible",
            "whiskey",
            true,
            true,
            600,
            true,
            false,
            true,
        ),
        (
            "never_freeze_sentinel",
            "powder_eggs",
            true,
            true,
            600,
            true,
            true,
            false,
        ),
        (
            "positive_freezing_comestible",
            "chem_benzene",
            true,
            true,
            600,
            true,
            false,
            true,
        ),
        (
            "no_temp_comestible",
            "caffeine",
            false,
            false,
            600,
            false,
            true,
            false,
        ),
        (
            "ordinary_control",
            "rock",
            false,
            false,
            10_000,
            false,
            true,
            false,
        ),
    ];
    if observation.temperature_constructors.len() != expected_temperature_constructors.len()
        || observation
            .temperature_constructors
            .iter()
            .zip(expected_temperature_constructors)
            .any(
                |(
                    actual,
                    (
                        case_id,
                        item_type,
                        has_temperature,
                        active,
                        processing_speed,
                        has_last_temp_check,
                        solid,
                        liquid,
                    ),
                )| {
                    actual.case_id != case_id
                        || actual.item_type != item_type
                        || actual.birth_turn != 123
                        || actual.has_temperature != has_temperature
                        || actual.active != active
                        || actual.processing_speed != processing_speed
                        || actual.temperature_millikelvin != 0
                        || actual.specific_energy_millijoules_per_gram != -10_000
                        || actual.serialized_last_temp_check_present != has_last_temp_check
                        || actual.serialized_last_temp_check
                            != if has_last_temp_check { 123 } else { 0 }
                        || actual.solid != solid
                        || actual.liquid != liquid
                        || actual.hot
                        || actual.cold
                        || actual.frozen
                },
            )
    {
        return Err(format!(
            "item temperature-constructor characterization is incomplete: {:?}",
            observation.temperature_constructors
        )
        .into());
    }
    let expected_thermal_properties = [
        (false, 0, 0, 0, 0, 0),
        (true, 4_186_000, 2_108_000, 333_000_000, 273_150, 992_520),
        (true, 1_500_000, 1_200_000, 10_000_000, 273_150, 367_780),
        (true, 4_156_246, 2_097_308, 330_092_987, 273_150, 986_098),
        (true, 4_000_000, 2_000_000, 310_000_000, 243_150, 996_300),
        (true, 1_693_636, 1_268_182, 32_000_000, -850, 528_851),
        (true, 2_000_000, 1_800_000, 200_000_000, 278_150, 730_670),
        (false, 0, 0, 0, 0, 0),
        (false, 0, 0, 0, 0, 0),
    ];
    if observation
        .temperature_constructors
        .iter()
        .zip(expected_thermal_properties)
        .any(
            |(actual, (present, liquid, solid, latent, freezing_point, ambient_energy))| {
                actual.thermal_properties_present != present
                    || actual.specific_heat_liquid_microjoules_per_gram_kelvin != liquid
                    || actual.specific_heat_solid_microjoules_per_gram_kelvin != solid
                    || actual.latent_heat_microjoules_per_gram != latent
                    || actual.freezing_point_millikelvin != freezing_point
                    || actual.ambient_specific_energy_millijoules_per_gram != ambient_energy
            },
        )
    {
        return Err(format!(
            "item thermal-property characterization is incomplete: {:?}",
            observation.temperature_constructors
        )
        .into());
    }
    if observation.rot_family.len() != ITEM_GROUP_ROT_CASES.len()
        || observation.rot_family.iter().zip(ITEM_GROUP_ROT_CASES).any(
            |(actual, (case_id, item_type, corpse, shelf_life_turns))| {
                let removal_threshold_turns = if corpse {
                    10 * 24 * 60 * 60
                } else {
                    shelf_life_turns * 2
                };
                actual.case_id != case_id
                    || actual.item_type != item_type
                    || actual.corpse != corpse
                    || !actual.goes_bad
                    || actual.shelf_life_turns != shelf_life_turns
                    || actual.rot_after_ten_minutes != 683
                    || actual.rot_after_one_hour != 4_099
                    || actual.removal_threshold_turns != removal_threshold_turns
                    || actual.removed_at_threshold
                    || !actual.removed_after_threshold
            },
        )
    {
        return Err(format!(
            "item rot characterization is incomplete: {:?}",
            observation.rot_family
        )
        .into());
    }
    if observation.insulated_container
        != (ItemGroupInsulatedContainerTraceV1 {
            item_type: String::from("thermos"),
            pocket_index: 0,
            insulation_milli: 10_000,
        })
    {
        return Err("item-group insulated-container characterization is incomplete".into());
    }
    let expected_named_snippets = [
        (
            "months_old_news",
            "months_old_newspaper",
            "months_old_news",
            24,
            "months_old_news_1",
            "months_old_news_25",
        ),
        (
            "wallet_photos",
            "wallet_photo",
            "wallet_photos",
            38,
            "wallet_picture_1",
            "wallet_picture_38",
        ),
    ];
    if observation.named_snippet_categories.len() != expected_named_snippets.len()
        || observation
            .named_snippet_categories
            .iter()
            .zip(expected_named_snippets)
            .any(
                |(actual, (case_id, item_type, category, count, first_id, last_id))| {
                    actual.case_id != case_id
                        || actual.item_type != item_type
                        || actual.category != category
                        || actual.choice_ids.len() != count
                        || actual.choice_ids.first().map(String::as_str) != Some(first_id)
                        || actual.choice_ids.last().map(String::as_str) != Some(last_id)
                        || actual.first_text.is_empty()
                        || actual.last_text.is_empty()
                        || actual.first_selection.seed == 0
                        || actual.first_selection.snippet_id != first_id
                        || actual.first_selection.text != actual.first_text
                        || !(0..10_000).contains(&actual.first_selection.downstream_draw)
                        || actual.last_selection.seed == 0
                        || actual.last_selection.snippet_id != last_id
                        || actual.last_selection.text != actual.last_text
                        || !(0..10_000).contains(&actual.last_selection.downstream_draw)
                },
            )
    {
        return Err("item-group named snippet characterization is incomplete".into());
    }
    let expected_multi_pocket = [
        (
            "leg_sheath_minimum",
            "leg_sheath6",
            "throwing_knife",
            vec![1, 0, 0, 0, 0, 0],
        ),
        (
            "leg_sheath_maximum",
            "leg_sheath6",
            "throwing_knife",
            vec![1, 1, 1, 1, 1, 1],
        ),
        (
            "hard_hat_mandible",
            "hat_hard",
            "plastic_mandible_guard",
            vec![0, 0, 0, 1, 0, 0],
        ),
    ];
    if observation.multi_pocket_wrappers.len() != expected_multi_pocket.len()
        || observation
            .multi_pocket_wrappers
            .iter()
            .zip(expected_multi_pocket)
            .any(
                |(actual, (case_id, wrapper_type, payload_type, pocket_counts))| {
                    actual.case_id != case_id
                        || actual.seed == 0
                        || actual.wrapper_type != wrapper_type
                        || actual.payload_type != payload_type
                        || !(0..10_000).contains(&actual.downstream_draw)
                        || actual
                            .pocket_contents
                            .iter()
                            .map(Vec::len)
                            .ne(pocket_counts)
                        || actual
                            .pocket_contents
                            .iter()
                            .flatten()
                            .any(|payload| payload != payload_type)
                },
            )
    {
        return Err("item-group multi-pocket characterization is incomplete".into());
    }
    let expected_containers = [
        ("discard", 1, 1, Vec::<String>::new()),
        (
            "spill",
            2,
            2,
            vec![
                String::from("test_nuclear_carafe"),
                String::from("test_pants_fur"),
                String::from("test_utility_belt"),
            ],
        ),
    ];
    let expected_content_orders = [
        "test_nuclear_carafe,test_pants_fur",
        "test_nuclear_carafe,test_utility_belt",
        "test_pants_fur,test_nuclear_carafe",
        "test_pants_fur,test_utility_belt",
        "test_utility_belt,test_nuclear_carafe",
        "test_utility_belt,test_pants_fur",
    ];
    if observation.containers.len() != expected_containers.len()
        || observation.containers.iter().zip(expected_containers).any(
            |(actual, (case_id, minimum_top_level, maximum_top_level, outside_types))| {
                actual.case_id != case_id
                    || actual.seed_search_limit != 100_000
                    || !actual.valid_shapes
                    || actual.minimum_top_level != minimum_top_level
                    || actual.maximum_top_level != maximum_top_level
                    || actual.minimum_contents != 2
                    || actual.maximum_contents != 2
                    || actual.content_orders != expected_content_orders
                    || actual.outside_types != outside_types
                    || actual.exact_traces.len() != 6
                    || actual.exact_traces.iter().any(|trace| {
                        trace.witness.is_empty()
                            || trace.seed == 0
                            || trace.top_level_types.len() != usize::from(minimum_top_level)
                            || trace.content_types.len() != 2
                    })
            },
        )
    {
        return Err("item-group container overflow characterization is incomplete".into());
    }
    if observation.everyday_corpse.seed_search_limit != 100_000
        || !observation.everyday_corpse.valid_shapes
        || observation.everyday_corpse.wrapper_types
            != [
                "corpse_child_calm",
                "corpse_generic_female",
                "corpse_generic_male",
            ]
        || observation.everyday_corpse.wrapper_raw_damage != [4_000]
        || observation.everyday_corpse.wrapper_damage_levels != [5]
        || !observation.everyday_corpse.multiple_content_counts
        || !observation.everyday_corpse.observed_pristine_content
        || !observation.everyday_corpse.observed_damage_four_content
        || observation.everyday_corpse.exact_traces.len() != 2
        || observation.everyday_corpse.exact_traces[0].witness != "fixed_seed:1"
        || observation.everyday_corpse.exact_traces[0].seed != 1
        || observation.everyday_corpse.exact_traces[1].witness != "first_damage_four_content"
        || observation
            .everyday_corpse
            .exact_traces
            .iter()
            .any(|trace| {
                trace.witness.is_empty()
                    || trace.seed == 0
                    || trace.wrapper_type.is_empty()
                    || trace.wrapper_raw_damage != 4_000
                    || trace.wrapper_damage_level != 5
                    || !trace.wrapper_pocket_forbidden
                    || trace.wrapper_pocket_no_unload
                    || trace.unloadable_content_count != trace.content_types.len()
                    || trace.content_types.is_empty()
                    || trace.content_types.len() != trace.content_raw_damage.len()
                    || trace.content_types.len() != trace.content_damage_levels.len()
            })
        || observation.nonholiday_event_types != ["test_rock"]
    {
        return Err("item-group corpse or event characterization is incomplete".into());
    }
    struct ExpectedPhoneTrace<'a> {
        witness: &'a str,
        seed: u32,
        wrapper_variant: &'a str,
        phone_type: &'a str,
        phone_ammo_remaining: i32,
        efile_types: &'a [&'a str],
        downstream_draw: i32,
    }
    let expected_phone_traces = [
        ExpectedPhoneTrace {
            witness: "fixed_seed:1",
            seed: 1,
            wrapper_variant: "hello_kitty_case",
            phone_type: "smart_phone_locked",
            phone_ammo_remaining: 3,
            efile_types: &["efile_recipes", "efile_lore", "efile_map"],
            downstream_draw: 9_907,
        },
        ExpectedPhoneTrace {
            witness: "first_phone_type:smart_phone_locked",
            seed: 1,
            wrapper_variant: "hello_kitty_case",
            phone_type: "smart_phone_locked",
            phone_ammo_remaining: 3,
            efile_types: &["efile_recipes", "efile_lore", "efile_map"],
            downstream_draw: 9_907,
        },
        ExpectedPhoneTrace {
            witness: "first_five_or_more_efiles",
            seed: 2,
            wrapper_variant: "violet_smart_phone_case",
            phone_type: "smart_phone_locked",
            phone_ammo_remaining: 7,
            efile_types: &[
                "essay_book",
                "essay_book",
                "essay_book",
                "novel_swash",
                "novel_satire",
                "book_fict_hard_sports_omni",
                "efile_lore",
                "efile_map",
            ],
            downstream_draw: 9_375,
        },
        ExpectedPhoneTrace {
            witness: "first_phone_type:smart_phone",
            seed: 3,
            wrapper_variant: "brown_smart_phone_case",
            phone_type: "smart_phone",
            phone_ammo_remaining: 7,
            efile_types: &[
                "plays_book",
                "essay_book",
                "poetry_book",
                "novel_coa",
                "novel_war2",
                "novel_road",
                "efile_lore",
                "efile_map",
            ],
            downstream_draw: 4_342,
        },
        ExpectedPhoneTrace {
            witness: "first_empty_efiles",
            seed: 5,
            wrapper_variant: "black_smart_phone_case",
            phone_type: "smart_phone_locked",
            phone_ammo_remaining: 12,
            efile_types: &[],
            downstream_draw: 4_586,
        },
    ];
    if observation.civilian_phone_case.seed_search_limit != 100_000
        || !observation.civilian_phone_case.valid_shapes
        || observation.civilian_phone_case.phone_types != ["smart_phone", "smart_phone_locked"]
        || !observation.civilian_phone_case.observed_empty_efiles
        || !observation.civilian_phone_case.observed_many_efiles
        || observation.civilian_phone_case.exact_traces.len() != expected_phone_traces.len()
        || observation
            .civilian_phone_case
            .exact_traces
            .iter()
            .zip(expected_phone_traces)
            .any(|(trace, expected)| {
                trace.witness != expected.witness
                    || trace.seed != expected.seed
                    || trace.wrapper_type != "waterproof_smart_phone_case"
                    || trace.wrapper_variant != expected.wrapper_variant
                    || trace.wrapper_any_pocket_sealed
                    || trace.wrapper_remaining_volume_ml != 0
                    || trace.wrapper_remaining_weight_g != 0
                    || trace.phone_type != expected.phone_type
                    || trace.phone_charges != 0
                    || trace.phone_ammo_remaining != expected.phone_ammo_remaining
                    || trace.phone_ammunition_type != "battery"
                    || trace.phone_raw_damage != 0
                    || trace
                        .efile_types
                        .iter()
                        .map(String::as_str)
                        .ne(expected.efile_types.iter().copied())
                    || trace.efile_raw_damage.len() != expected.efile_types.len()
                    || trace.efile_raw_damage.iter().any(|damage| *damage != 0)
                    || trace.downstream_draw != expected.downstream_draw
            })
    {
        return Err("civilian phone nested containment characterization is incomplete".into());
    }
    let event_distribution = [(1, "none"), (3, "none"), (4, "ordinary"), (5, "ordinary")];
    if observation.event_distribution.len() != event_distribution.len()
        || observation
            .event_distribution
            .iter()
            .zip(event_distribution)
            .any(|(actual, expected)| actual.ticket != expected.0 || actual.selected != expected.1)
    {
        return Err("inactive event entries do not preserve distribution ticket weight".into());
    }
    Ok(())
}

fn validate_mapgen_scenario(
    scenario: &MapgenOracleScenarioV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if scenario.format_version != ORACLE_FORMAT_VERSION
        || scenario.baseline_commit != BASELINE_COMMIT
        || scenario.upstream_tree != UPSTREAM_TREE
        || scenario.kernel != MAPGEN_KERNEL
    {
        return Err("mapgen oracle scenario identity mismatch".into());
    }
    validate_mapgen_observation(&scenario.expected_observation)
}

fn validate_mapgen_observation(
    observation: &MapgenOracleObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if observation.format_version != ORACLE_FORMAT_VERSION
        || observation.baseline_commit != BASELINE_COMMIT
        || observation.upstream_tree != UPSTREAM_TREE
        || observation.kernel != MAPGEN_KERNEL
    {
        return Err("mapgen oracle observation identity mismatch".into());
    }
    let matching = [
        (
            "exact_full",
            "shelter_north",
            "shelter_north",
            "EXACT",
            true,
        ),
        (
            "exact_base_rejected",
            "shelter",
            "shelter_north",
            "EXACT",
            false,
        ),
        ("rotatable_type", "shelter", "shelter_east", "TYPE", true),
        (
            "linear_subtype",
            "road_straight",
            "road_ew",
            "SUBTYPE",
            true,
        ),
        (
            "wrong_linear_subtype",
            "road_curved",
            "road_ew",
            "SUBTYPE",
            false,
        ),
        ("prefix_separator", "forest", "forest_thick", "PREFIX", true),
        (
            "partial_prefix_rejected",
            "fore",
            "forest_thick",
            "PREFIX",
            false,
        ),
        (
            "contains_substring",
            "rest_t",
            "forest_thick",
            "CONTAINS",
            true,
        ),
    ];
    if observation.matching.len() != matching.len()
        || observation
            .matching
            .iter()
            .zip(matching)
            .any(|(actual, expected)| {
                actual.case_id != expected.0
                    || actual.query != expected.1
                    || actual.terrain_id != expected.2
                    || actual.match_type != expected.3
                    || actual.matches != expected.4
            })
    {
        return Err("mapgen matching observation is incomplete or out of order".into());
    }
    validate_rotations(&observation.rotatable, "shelter")?;
    validate_rotations(&observation.linear, "road")?;
    if observation.palette.palette_id != "rust_cpp_oracle_mapgen_palette_v1"
        || observation.palette.key != "X"
        || !observation.palette.key_has_terrain
        || observation.palette.piece_phases != ["terrain", "furniture", "removal", "nested_mapgen"]
        || observation.palette.mapgen_size_x != 1
        || observation.palette.mapgen_size_y != 1
        || !observation.palette.setup_completed
    {
        return Err("mapgen palette observation is incomplete".into());
    }
    let template = &observation.static_template;
    if template.width_tiles != 24
        || template.height_tiles != 24
        || template.source_marker_x != 2
        || template.source_marker_y != 5
        || template.background_terrain_id != "t_dirt"
        || template.marker_terrain_id != "t_floor"
        || template.marker_furniture_id != "f_table"
        || template.generated_background_terrain_id != template.background_terrain_id
        || template.generated_marker_terrain_id != template.marker_terrain_id
        || template.generated_marker_furniture_id != template.marker_furniture_id
        || template.generated_rows.len() != 24
        || template.generated_rows.iter().enumerate().any(|(y, row)| {
            row.len() != 24
                || row
                    .bytes()
                    .enumerate()
                    .any(|(x, tile)| tile != if (x, y) == (2, 5) { b'X' } else { b'.' })
        })
        || template.piece_phases != ["terrain", "furniture"]
        || !template.setup_completed
    {
        return Err("mapgen admitted static-template observation is incomplete".into());
    }
    let start = &observation.start_location;
    if start.start_location_id != "sloc_lmoe"
        || start.target_count != 1
        || start.chosen_target_index != 0
        || start.chosen_target_omt != "lmoe"
        || start.chosen_target_match_type != "TYPE"
        || start.chosen_target_parameter_count != 0
        || start.requires_city
        || start.city_size_minimum != 0
        || start.city_size_maximum != i32::MAX
        || start.city_distance_minimum != 0
        || start.city_distance_maximum != i32::MAX
        || start.allowed_z_minimum != -10
        || start.allowed_z_maximum != 10
        || !start.flags.is_empty()
        || !start.runtime_selectable_without_cities
        || start.candidate_identity_ids
            != ["shelter_north", "lmoe_north", "road_ew", "forest_thick"]
        || start.matching_candidate_ids != ["lmoe_north"]
        || start.selected_candidate_id != "lmoe_north"
    {
        return Err("mapgen production start-location observation is incomplete".into());
    }
    let city = &observation.city;
    if city.settings_id != "default"
        || city.city_size != 8
        || city.city_spacing != 4
        || city.is_megacity
        || (city.center_x, city.center_y, city.size) != (90, 90, 8)
        || city.point_x != [90, 98, 98, 106]
        || city.point_y != [90, 90, 98, 90]
        || city.edge_distances != [0, 0, 3, 8]
        || city.start_distances != [-8, -8, -5, 0]
        || (city.random_count_floor, city.random_count_ceiling) != (9, 10)
        || (city.minimum_generated_size, city.maximum_generated_size) != (2, 55)
    {
        return Err("mapgen city placement characterization is incomplete".into());
    }
    let road = &observation.road;
    if road.point_x != [10, 179, 100, 70, 110, 90]
        || road.point_y != [0, 40, 179, 70, 75, 115]
        || road.mst_left != [3, 4, 2, 1, 0]
        || road.mst_right != [4, 5, 5, 4, 3]
    {
        return Err("mapgen road MST characterization is incomplete".into());
    }
    Ok(())
}

fn rust_item_group_tool_charge_observation()
-> Result<Vec<ItemGroupToolChargeObservationV1>, Box<dyn std::error::Error>> {
    [0, 1, 56, 100]
        .into_iter()
        .map(rust_item_group_tool_charge_case)
        .collect()
}

fn rust_item_group_tool_charge_case(
    requested_charges: i32,
) -> Result<ItemGroupToolChargeObservationV1, Box<dyn std::error::Error>> {
    rust_item_group_tool_charge_case_with_replacement(requested_charges, requested_charges, None)
}

fn rust_item_group_magazine_charge_observation()
-> Result<Vec<ItemGroupMagazineChargeDirectV1>, Box<dyn std::error::Error>> {
    [
        ("light_0", "light_battery_cell", 16, 0),
        ("light_1", "light_battery_cell", 16, 1),
        ("light_16", "light_battery_cell", 16, 16),
        ("light_100", "light_battery_cell", 16, 100),
        ("ultralight_overflow", "light_minus_battery_cell", 2, 100),
    ]
    .into_iter()
    .map(|(case_id, item_type, capacity, requested_charges)| {
        rust_item_group_magazine_charge_case(case_id, item_type, capacity, requested_charges)
    })
    .collect()
}

fn rust_item_group_charge_capacity_sentinel_observation()
-> Result<Vec<ItemGroupChargeCapacitySentinelDirectV1>, Box<dyn std::error::Error>> {
    [
        (
            "integral_tool_minimum",
            0,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(85),
        ),
        (
            "integral_tool_maximum",
            0,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(85),
        ),
        (
            "ordinary_unresolved",
            4,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::None,
            None,
        ),
        (
            "detachable_tool_minimum",
            0,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(56),
        ),
        (
            "detachable_tool_maximum",
            0,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(56),
        ),
        (
            "detachable_explicit_over_capacity",
            0,
            100,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(56),
        ),
        (
            "magazine_minimum",
            0,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(16),
        ),
        (
            "magazine_maximum",
            0,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            Some(16),
        ),
        (
            "container_minimum",
            1,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
            Some(2),
        ),
        (
            "container_maximum",
            1,
            -1,
            cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
            Some(2),
        ),
        (
            "lower_sentinel_minimum",
            -1,
            4,
            cdda_protocol::ItemGroupChargeCapacityV1::None,
            None,
        ),
        (
            "lower_sentinel_maximum",
            -1,
            4,
            cdda_protocol::ItemGroupChargeCapacityV1::None,
            None,
        ),
    ]
    .into_iter()
    .map(|(case_id, minimum, maximum, owner, capacity)| {
        let effective = cdda_sim::resolve_item_group_charge_range(
            cdda_protocol::ItemGroupChargeRangeV1 { minimum, maximum },
            owner,
            capacity,
        )?;
        let (effective_minimum, effective_maximum) = effective
            .map(|range| (range.minimum, range.maximum))
            .unwrap_or((-1, -1));
        Ok(ItemGroupChargeCapacitySentinelDirectV1 {
            case_id: case_id.to_owned(),
            minimum,
            maximum,
            effective_minimum,
            effective_maximum,
        })
    })
    .collect()
}

fn rust_item_group_magazine_charge_case(
    case_id: &str,
    item_type: &str,
    capacity: u32,
    requested_charges: i32,
) -> Result<ItemGroupMagazineChargeDirectV1, Box<dyn std::error::Error>> {
    let mut owner = CraftItemPrototypeV1 {
        type_id: item_type.to_owned(),
        charges: 0,
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
        containment: ItemContainmentProfileV1::default(),
    };
    owner.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
        pocket_index: 0,
        pocket_id: String::from("MAGAZINE"),
        ammunition_type: String::from("battery"),
        capacity,
        rigid: true,
        reloadable: false,
        unloadable: false,
    }];
    let mut ammunition = owner.clone();
    ammunition.type_id = String::from("battery");
    ammunition.charges = 1;
    ammunition.ammunition_type = String::from("battery");
    ammunition.integral_magazines.clear();
    ammunition.containment = ItemContainmentProfileV1 {
        count_by_charges: true,
        stack_size: 100,
        ..ItemContainmentProfileV1::default()
    };
    let item = ItemGroupItemPrototypeV1 {
        prototype: owner,
        maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
        variants: Vec::new(),
        description_expansion: None,
        snippets: Vec::new(),
        initial_variables: BTreeMap::new(),
        default_container: None,
        modifier_side_effects_supported: true,
        charges: Some(cdda_protocol::ItemGroupChargeRangeV1 {
            minimum: requested_charges,
            maximum: requested_charges,
        }),
        minimum_one_charge: false,
        tool_charge_storage: Some(ItemGroupToolChargeStorageV1::Integral { ammunition }),
        charges_supported: true,
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
        contents_insertion_supported: true,
    };
    let projection = cdda_sim::item_group_integral_charge_projection(&item, requested_charges)
        .map_err(|error| format!("Rust integral-magazine charge projection failed: {error}"))?;
    Ok(ItemGroupMagazineChargeDirectV1 {
        case_id: case_id.to_owned(),
        requested_charges,
        item_type: projection.item_type,
        ammunition_type: projection
            .ammunition_type
            .unwrap_or_else(|| String::from("null")),
        ammunition_remaining: projection.ammunition_remaining,
        remaining_capacity: i32::try_from(projection.remaining_capacity)?,
    })
}

fn rust_repeated_item_group_tool_charge_observation()
-> Result<ItemGroupRepeatedToolChargeDirectV1, Box<dyn std::error::Error>> {
    let observed = rust_item_group_tool_charge_case_with_replacement(0, 100, Some(1))?;
    Ok(ItemGroupRepeatedToolChargeDirectV1 {
        leaf_minimum: 0,
        leaf_maximum: 100,
        replacement_requested: 1,
        tool_type: observed.tool_type,
        magazine_type: observed.magazine_type,
        ammunition_type: observed.ammunition_type,
        ammunition_remaining: observed.ammunition_remaining,
    })
}

fn rust_item_group_description_expansion_observation()
-> Result<ItemGroupDescriptionExpansionDirectV1, Box<dyn std::error::Error>> {
    let direct_input = String::from("Foo <lt>lt<gt> <unknown>");
    let expansion = ItemDescriptionExpansionV1 {
        template: direct_input.clone(),
        categories: vec![
            ItemDescriptionSnippetCategoryV1 {
                category: String::from("<gt>"),
                choices: vec![ItemDescriptionSnippetChoiceV1 {
                    text: String::from(">"),
                    weight: 1,
                }],
            },
            ItemDescriptionSnippetCategoryV1 {
                category: String::from("<lt>"),
                choices: vec![ItemDescriptionSnippetChoiceV1 {
                    text: String::from("<"),
                    weight: 1,
                }],
            },
        ],
    };
    let mut rng = StdRng::seed_from_u64(113);
    let direct_output = cdda_sim::expand_item_description(&expansion, &mut rng)
        .map_err(|error| format!("Rust direct description expansion failed: {error}"))?;
    Ok(ItemGroupDescriptionExpansionDirectV1 {
        direct_input,
        direct_output,
    })
}

fn rust_item_group_variable_size_fit_observation() -> Vec<ItemGroupVariableSizeFitDirectV1> {
    [
        ("non_variable_control", false, true),
        ("variable_unfitted", true, false),
        ("variable_fitted", true, true),
    ]
    .into_iter()
    .map(
        |(case_id, variable_size, one_in_three_succeeded)| ItemGroupVariableSizeFitDirectV1 {
            case_id: case_id.to_owned(),
            variable_size,
            fitted: cdda_sim::item_group_fitted_after_phase(
                variable_size,
                false,
                one_in_three_succeeded,
            ),
        },
    )
    .collect()
}

fn rust_item_group_default_container_observation()
-> Result<Vec<ItemGroupDefaultContainerDirectV1>, Box<dyn std::error::Error>> {
    let plain = |type_id: &str| ItemGroupItemPrototypeV1 {
        prototype: CraftItemPrototypeV1 {
            type_id: type_id.to_owned(),
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
            containment: ItemContainmentProfileV1::default(),
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
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
        contents_insertion_supported: true,
    };
    let container = |type_id: &str,
                     maximum_volume: u64,
                     maximum_item_volume: u64,
                     maximum_weight: u64,
                     watertight: bool| {
        let mut container = plain(type_id);
        container.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("CONTAINER"),
            capacities: Vec::new(),
            access_moves: 400,
            rigid: true,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind: SpawnPocketKindV1::Container,
                max_contains_volume_milliliters: maximum_volume,
                magazine_well_volume_milliliters: 0,
                contents_collapsed_by_default: false,
                max_contains_weight_milligrams: maximum_weight,
                max_item_volume_milliliters: maximum_item_volume,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: 1_000,
                item_restrictions: Vec::new(),
                flag_restrictions: Vec::new(),
                access_moves: 400,
                rigid: true,
                watertight,
                transparent: true,
                forbidden: false,
                sealable: true,
            }),
        }];
        ItemGroupContainerV1 {
            item: Box::new(container),
            variant_id: None,
            sealed: true,
            overflow: ItemGroupOverflowV1::None,
        }
    };
    let mut water = plain("water_clean");
    water.prototype.containment = ItemContainmentProfileV1 {
        weight_milligrams: 250_000,
        volume_milliliters: 250,
        count_by_charges: true,
        stack_size: 1,
        phase: ItemPhaseV1::Liquid,
        ..ItemContainmentProfileV1::default()
    };
    water.default_container = Some(container("bottle_plastic", 500, 500, 1_000_000, true));
    let mut aspirin = plain("aspirin");
    aspirin.prototype.charges = 0;
    aspirin.prototype.containment = ItemContainmentProfileV1 {
        weight_milligrams: 1_000,
        volume_milliliters: 1,
        stack_size: 1,
        phase: ItemPhaseV1::Solid,
        ..ItemContainmentProfileV1::default()
    };
    aspirin.default_container = Some(container(
        "bottle_plastic_pill_painkiller",
        250,
        17,
        1_000_000,
        true,
    ));
    let mut explicit_target = plain("ibuprofen");
    explicit_target.prototype.charges = 0;
    explicit_target.prototype.containment.volume_milliliters = 1;
    explicit_target.prototype.containment.weight_milligrams = 1_000;
    let explicit_aspirin_container = ItemGroupContainerV1 {
        item: Box::new(aspirin.clone()),
        variant_id: None,
        sealed: true,
        overflow: ItemGroupOverflowV1::None,
    };
    let painkiller_group_wrapper =
        container("bottle_plastic_pill_painkiller", 250, 17, 1_000_000, true);
    [
        (
            "direct_water",
            &water,
            cdda_sim::ItemGroupDefaultContainerMode::Unmodified,
        ),
        (
            "direct_aspirin",
            &aspirin,
            cdda_sim::ItemGroupDefaultContainerMode::Unmodified,
        ),
        (
            "modifier_aspirin",
            &aspirin,
            cdda_sim::ItemGroupDefaultContainerMode::ModifierFallback { sealed: true },
        ),
        (
            "suppressed_aspirin",
            &aspirin,
            cdda_sim::ItemGroupDefaultContainerMode::ModifierSuppressed,
        ),
        (
            "explicit_container_default",
            &explicit_target,
            cdda_sim::ItemGroupDefaultContainerMode::ModifierExplicit {
                container: explicit_aspirin_container,
            },
        ),
        (
            "production_aspirin_minimum",
            &aspirin,
            cdda_sim::ItemGroupDefaultContainerMode::GroupWrapperExplicitNull {
                container: painkiller_group_wrapper.clone(),
                count: 1,
            },
        ),
        (
            "production_aspirin_maximum",
            &aspirin,
            cdda_sim::ItemGroupDefaultContainerMode::GroupWrapperExplicitNull {
                container: painkiller_group_wrapper,
                count: 20,
            },
        ),
    ]
    .into_iter()
    .map(|(case_id, item, mode)| {
        let projection = cdda_sim::item_group_default_container_projection(item, mode)
            .map_err(|error| format!("Rust default-container projection failed: {error}"))?;
        Ok(ItemGroupDefaultContainerDirectV1 {
            case_id: case_id.to_owned(),
            outer_type: projection.outer_type,
            content_types: projection.content_types,
            payload_charges: projection.payload_charges,
            sealed: projection.sealed,
            pocket_collapsed: projection.pocket_collapsed,
        })
    })
    .collect()
}

fn rust_item_group_flexible_wrapper_observation()
-> Result<Vec<ItemGroupFlexibleWrapperDirectV1>, Box<dyn std::error::Error>> {
    let plain = |type_id: &str| ItemGroupItemPrototypeV1 {
        prototype: CraftItemPrototypeV1 {
            type_id: type_id.to_owned(),
            charges: 0,
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
            containment: ItemContainmentProfileV1::default(),
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
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
        contents_insertion_supported: true,
    };
    let variant = |id: &str| ItemGroupVariantOptionV1 {
        variant: ItemVariantV1 {
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            symbol: String::new(),
            color: String::new(),
            ascii_picture: String::new(),
        },
        weight: 1,
        description_expansion: None,
    };
    let wrapper = |type_id: &str,
                   own_volume: u64,
                   own_weight: u64,
                   capacity_volume: u64,
                   reserved_volume: u64,
                   capacity_weight: u64,
                   collapsed: bool,
                   variant_id: Option<&str>| {
        let mut item = plain(type_id);
        item.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: own_weight,
            volume_milliliters: own_volume,
            longest_side_millimeters: 102,
            stack_size: 1,
            phase: ItemPhaseV1::Solid,
            ..ItemContainmentProfileV1::default()
        };
        if let Some(variant_id) = variant_id {
            item.variants.push(variant(variant_id));
        }
        item.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::new(),
            capacities: Vec::new(),
            access_moves: 400,
            rigid: false,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind: SpawnPocketKindV1::Container,
                max_contains_volume_milliliters: capacity_volume,
                magazine_well_volume_milliliters: reserved_volume,
                contents_collapsed_by_default: collapsed,
                max_contains_weight_milligrams: capacity_weight,
                max_item_volume_milliliters: capacity_volume,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: 1_000,
                item_restrictions: Vec::new(),
                flag_restrictions: Vec::new(),
                access_moves: 400,
                rigid: false,
                watertight: false,
                transparent: true,
                forbidden: false,
                sealable: false,
            }),
        }];
        ItemGroupContainerV1 {
            item: Box::new(item),
            variant_id: variant_id.map(str::to_owned),
            sealed: false,
            overflow: ItemGroupOverflowV1::None,
        }
    };
    let mut chaw = plain("chaw");
    chaw.prototype.containment = ItemContainmentProfileV1 {
        weight_milligrams: 4_000,
        volume_milliliters: 4,
        stack_size: 1,
        phase: ItemPhaseV1::Solid,
        ..ItemContainmentProfileV1::default()
    };
    let mut gum = plain("gum");
    gum.prototype.containment = ItemContainmentProfileV1 {
        weight_milligrams: 3_000,
        volume_milliliters: 2,
        stack_size: 1,
        phase: ItemPhaseV1::Solid,
        ..ItemContainmentProfileV1::default()
    };
    let gum_variant = "gum_watermelon";
    gum.variants.push(variant(gum_variant));
    [
        (
            "production_chaw_minimum",
            &chaw,
            wrapper("wrapper", 50, 3_000, 2_500, 45, 6_000_000, false, None),
            1,
            None,
        ),
        (
            "production_chaw_maximum",
            &chaw,
            wrapper("wrapper", 50, 3_000, 2_500, 45, 6_000_000, false, None),
            20,
            None,
        ),
        (
            "production_chewing_gum",
            &gum,
            wrapper(
                "blister_pack_small",
                7,
                5_000,
                50,
                0,
                50_000,
                true,
                Some("blister_pack_gum"),
            ),
            12,
            Some(gum_variant),
        ),
    ]
    .into_iter()
    .map(|(case_id, item, wrapper, count, content_variant)| {
        let projection =
            cdda_sim::item_group_flexible_wrapper_projection(item, wrapper, count, content_variant)
                .map_err(|error| format!("Rust flexible-wrapper projection failed: {error}"))?;
        Ok(ItemGroupFlexibleWrapperDirectV1 {
            case_id: case_id.to_owned(),
            outer_type: projection.outer_type,
            outer_variant: projection.outer_variant,
            pocket_rigid: projection.pocket_rigid,
            pocket_collapsed_by_default: projection.pocket_collapsed_by_default,
            pocket_collapsed: projection.pocket_collapsed,
            content_types: projection.content_types,
            content_variants: projection.content_variants,
            content_charges: projection.content_charges,
            outer_volume_ml: projection.outer_volume_milliliters,
            outer_weight_g: projection.outer_weight_grams,
            pocket_capacity_volume_ml: projection.pocket_capacity_volume_milliliters,
            pocket_remaining_volume_ml: projection.pocket_remaining_volume_milliliters,
            pocket_remaining_weight_g: projection.pocket_remaining_weight_grams,
            sealed: projection.sealed,
        })
    })
    .collect()
}

fn rust_item_group_named_snippet_observation(
    workspace: &Path,
) -> Result<Vec<ItemGroupNamedSnippetDirectV1>, Box<dyn std::error::Error>> {
    let manifest_path = workspace.join("vendor/cdda-content-manifest.json");
    let manifest = ContentManifest::load(&manifest_path)?;
    let content_root = manifest_path
        .parent()
        .ok_or("pinned content manifest has no parent directory")?;
    let mods = ModCatalog::load(&manifest, content_root)?;
    let enabled = mods.recommended_new_world()?;
    let items = ItemRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    let snippets =
        DescriptionSnippetRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;

    [
        ("months_old_news", "months_old_newspaper", "months_old_news"),
        ("wallet_photos", "wallet_photo", "wallet_photos"),
    ]
    .into_iter()
    .map(|(case_id, item_type, category_id)| {
        let item = items
            .get(item_type)
            .ok_or_else(|| format!("pinned item {item_type} disappeared"))?;
        if item.snippet_category != category_id || !item.snippets.is_empty() {
            return Err(format!(
                "pinned item {item_type} no longer uses named snippet category {category_id}"
            )
            .into());
        }
        let category = snippets
            .get(category_id)
            .ok_or_else(|| format!("pinned snippet category {category_id} disappeared"))?;
        let first_weight = category
            .identified
            .first()
            .ok_or_else(|| format!("pinned snippet category {category_id} became empty"))?
            .weight;
        if first_weight == 0
            || category
                .identified
                .iter()
                .any(|choice| choice.weight != first_weight)
            || category.identified.len() > cdda_protocol::MAX_ITEM_SNIPPETS
        {
            return Err(format!(
                "pinned snippet category {category_id} no longer has bounded uniform weights"
            )
            .into());
        }
        let choices = category
            .identified
            .iter()
            .map(|choice| {
                Ok(ItemSnippetV1 {
                    id: choice.id.clone().ok_or_else(|| {
                        format!("pinned snippet category {category_id} lost an identified ID")
                    })?,
                    text: choice.text.clone(),
                })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        let first = cdda_sim::item_group_snippet_projection(&choices, 0)
            .map_err(|error| format!("Rust named-snippet projection failed: {error}"))?
            .ok_or("nonempty snippet category produced no first choice")?;
        let last_ticket =
            u64::try_from(choices.len() - 1).map_err(|_| "snippet category length exceeded u64")?;
        let last = cdda_sim::item_group_snippet_projection(&choices, last_ticket)
            .map_err(|error| format!("Rust named-snippet projection failed: {error}"))?
            .ok_or("nonempty snippet category produced no last choice")?;
        Ok(ItemGroupNamedSnippetDirectV1 {
            case_id: case_id.to_owned(),
            item_type: item_type.to_owned(),
            category: category_id.to_owned(),
            choice_ids: choices.iter().map(|choice| choice.id.clone()).collect(),
            first_text: choices
                .first()
                .ok_or("snippet first choice disappeared")?
                .text
                .clone(),
            last_text: choices
                .last()
                .ok_or("snippet last choice disappeared")?
                .text
                .clone(),
            first_selected_id: first.id,
            first_selected_text: first.text,
            last_selected_id: last.id,
            last_selected_text: last.text,
        })
    })
    .collect()
}

fn rust_item_group_multi_pocket_observation()
-> Result<Vec<ItemGroupMultiPocketDirectV1>, Box<dyn std::error::Error>> {
    let plain = |type_id: &str| ItemGroupItemPrototypeV1 {
        prototype: CraftItemPrototypeV1 {
            type_id: type_id.to_owned(),
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
            containment: ItemContainmentProfileV1::default(),
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
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
        contents_insertion_supported: true,
    };
    let pocket = |index: u16, volume: u64, weight: u64, length: u64, accepted_flag: &str| {
        AmmunitionContainerPocketPrototypeV1 {
            pocket_index: index,
            pocket_id: String::new(),
            capacities: Vec::new(),
            rigid: false,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind: SpawnPocketKindV1::Container,
                max_contains_volume_milliliters: volume,
                magazine_well_volume_milliliters: 0,
                contents_collapsed_by_default: false,
                max_contains_weight_milligrams: weight,
                max_item_volume_milliliters: u64::MAX,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: length,
                item_restrictions: vec![String::from(
                    cdda_protocol::SPAWN_POCKET_SINGLE_ITEM_MARKER,
                )],
                flag_restrictions: vec![accepted_flag.to_owned()],
                access_moves: 100,
                rigid: false,
                watertight: false,
                transparent: false,
                forbidden: false,
                sealable: false,
            }),
        }
    };
    let wrapper = |type_id: &str, pockets| {
        let mut item = plain(type_id);
        item.prototype.ammunition_containers = pockets;
        ItemGroupContainerV1 {
            item: Box::new(item),
            variant_id: None,
            sealed: false,
            overflow: ItemGroupOverflowV1::None,
        }
    };

    let mut knife = plain("throwing_knife");
    knife.prototype.containment = ItemContainmentProfileV1 {
        weight_milligrams: 200_000,
        volume_milliliters: 56,
        longest_side_millimeters: 350,
        flags: vec![String::from("SHEATH_KNIFE")],
        ..ItemContainmentProfileV1::default()
    };
    let sheath = || {
        wrapper(
            "leg_sheath6",
            (0..6)
                .map(|index| pocket(index, 100, 500_000, 350, "SHEATH_KNIFE"))
                .collect(),
        )
    };

    let mut guard = plain("plastic_mandible_guard");
    guard.prototype.containment = ItemContainmentProfileV1 {
        weight_milligrams: 250_000,
        volume_milliliters: 200,
        longest_side_millimeters: 100,
        flags: vec![String::from("HELMET_MANDIBLE_GUARD_STRAPPED")],
        ..ItemContainmentProfileV1::default()
    };
    let hard_hat = wrapper(
        "hat_hard",
        [
            (500, 500_000, u64::MAX, "HELMET_FACE_SHIELD"),
            (500, 400_000, u64::MAX, "HELMET_EAR_ATTACHMENT"),
            (500, 250_000, u64::MAX, "HELMET_NAPE_PROTECTOR"),
            (500, 400_000, u64::MAX, "HELMET_MANDIBLE_GUARD_STRAPPED"),
            (400, 400_000, u64::MAX, "HELMET_BACK_POUCH"),
            (300, 500_000, 140, "HELMET_HEAD_ATTACHMENT"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (volume, weight, length, flag))| {
            pocket(index as u16, volume, weight, length, flag)
        })
        .collect(),
    );

    [
        ("leg_sheath_minimum", &knife, sheath(), 1_u16),
        ("leg_sheath_maximum", &knife, sheath(), 6_u16),
        ("hard_hat_mandible", &guard, hard_hat, 1_u16),
    ]
    .into_iter()
    .map(|(case_id, payload, wrapper, count)| {
        let projection = cdda_sim::item_group_multi_pocket_projection(payload, wrapper, count)
            .map_err(|error| format!("Rust multi-pocket projection failed: {error}"))?;
        Ok(ItemGroupMultiPocketDirectV1 {
            case_id: case_id.to_owned(),
            wrapper_type: projection.outer_type,
            payload_type: payload.prototype.type_id.clone(),
            pocket_contents: projection
                .pocket_contents
                .into_iter()
                .map(|(_, contents)| contents)
                .collect(),
        })
    })
    .collect()
}

fn rust_item_group_static_corpse_observation(
    traces: &[ItemGroupCorpseTraceV1],
) -> Result<Vec<ItemGroupCorpseDirectTraceV1>, Box<dyn std::error::Error>> {
    let plain = |type_id: &str| ItemGroupItemPrototypeV1 {
        prototype: CraftItemPrototypeV1 {
            type_id: type_id.to_owned(),
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
            containment: ItemContainmentProfileV1 {
                weight_milligrams: 1,
                volume_milliliters: 2,
                longest_side_millimeters: 1,
                ..ItemContainmentProfileV1::default()
            },
        },
        maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
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
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
        contents_insertion_supported: true,
    };
    traces
        .iter()
        .map(|trace| {
            if trace.content_types.len() != trace.content_raw_damage.len() {
                return Err("corpse trace content identity/damage lengths diverged".into());
            }
            let mut wrapper = plain(&trace.wrapper_type);
            wrapper
                .prototype
                .containment
                .flags
                .push(String::from("CORPSE"));
            wrapper.initial_variables.insert(
                ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE.to_owned(),
                ItemVariableValueV1::String(String::from(
                    if trace.wrapper_type == "corpse_child_calm" {
                        "mon_child"
                    } else {
                        "mon_null"
                    },
                )),
            );
            wrapper.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("CORPSE_CONTENTS"),
                capacities: Vec::new(),
                rigid: false,
                access_moves: 100,
                reloadable: false,
                unloadable: true,
                spawn_rules: Some(SpawnPocketRulesV1 {
                    kind: SpawnPocketKindV1::Container,
                    max_contains_volume_milliliters: 1,
                    magazine_well_volume_milliliters: 0,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: u64::MAX,
                    max_item_volume_milliliters: 1,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: 1,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 100,
                    rigid: false,
                    watertight: false,
                    transparent: true,
                    forbidden: true,
                    sealable: false,
                }),
            }];
            let contents = trace
                .content_types
                .iter()
                .zip(&trace.content_raw_damage)
                .rev()
                .map(|(type_id, raw_damage)| Ok((plain(type_id), u16::try_from(*raw_damage)?)))
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            let projection = cdda_sim::item_group_static_corpse_projection(
                &wrapper,
                u16::try_from(trace.wrapper_raw_damage)?,
                &contents,
            )?;
            Ok(ItemGroupCorpseDirectTraceV1 {
                wrapper_type: projection.wrapper_type,
                wrapper_raw_damage: i32::from(projection.wrapper_raw_damage),
                wrapper_damage_level: i32::from(projection.wrapper_damage_level),
                wrapper_pocket_forbidden: projection.wrapper_pocket_forbidden,
                wrapper_pocket_no_unload: !projection.wrapper_pocket_unloadable,
                unloadable_content_count: projection.unloadable_content_count,
                content_types: projection.content_types,
                content_raw_damage: projection
                    .content_raw_damage
                    .into_iter()
                    .map(i32::from)
                    .collect(),
                content_damage_levels: projection
                    .content_damage_levels
                    .into_iter()
                    .map(i32::from)
                    .collect(),
            })
        })
        .collect()
}

fn rust_item_group_temperature_constructor_observation(
    workspace: &Path,
) -> Result<Vec<ItemGroupTemperatureConstructorTraceV1>, Box<dyn std::error::Error>> {
    let manifest_path = workspace.join("vendor/cdda-content-manifest.json");
    let manifest = ContentManifest::load(&manifest_path)?;
    let content_root = manifest_path
        .parent()
        .ok_or("pinned content manifest has no parent directory")?;
    let mods = ModCatalog::load(&manifest, content_root)?;
    let enabled = mods.recommended_new_world()?;
    let items = ItemRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    let materials = MaterialRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    [
        ("materialless_comestible", "chaw"),
        ("material_comestible", "water_clean"),
        ("field_blocker_material", "caff_gum"),
        ("weighted_material", "saline"),
        ("custom_freezing_comestible", "whiskey"),
        ("never_freeze_sentinel", "powder_eggs"),
        ("positive_freezing_comestible", "chem_benzene"),
        ("no_temp_comestible", "caffeine"),
        ("ordinary_control", "rock"),
    ]
    .into_iter()
    .map(|(case_id, item_type)| {
        let item = items
            .get(item_type)
            .ok_or_else(|| format!("pinned item {item_type} disappeared"))?;
        let has_temperature =
            item.subtypes.contains("COMESTIBLE") && !item.flags.contains("NO_TEMP");
        let current_phase = match item.phase.to_ascii_lowercase().as_str() {
            "" | "solid" => ItemPhaseV1::Solid,
            "liquid" => ItemPhaseV1::Liquid,
            phase => {
                return Err(format!(
                    "oracle item {item_type} has unsupported phase {phase}"
                ));
            }
        };
        let thermal_properties = if has_temperature {
            match materials
                .comestible_thermal_properties(item)
                .map_err(|error| error.to_string())?
            {
                Some(properties) => Some(cdda_protocol::ItemThermalPropertiesV1 {
                    specific_heat_liquid_microjoules_per_gram_kelvin: properties
                        .specific_heat_liquid_microjoules_per_gram_kelvin,
                    specific_heat_solid_microjoules_per_gram_kelvin: properties
                        .specific_heat_solid_microjoules_per_gram_kelvin,
                    latent_heat_microjoules_per_gram: properties.latent_heat_microjoules_per_gram,
                    freezing_point_millikelvin: item.freezing_point_millikelvin().ok_or_else(
                        || format!("oracle item {item_type} has an overflowing freezing point"),
                    )?,
                }),
                None => None,
            }
        } else {
            None
        };
        let state = initial_item_temperature_state(SimTick(123), current_phase, thermal_properties);
        let ambient_specific_energy_millijoules_per_gram = thermal_properties
            .and_then(|properties| properties.normal_ambient_specific_energy_millijoules_per_gram())
            .unwrap_or_default();
        Ok(ItemGroupTemperatureConstructorTraceV1 {
            case_id: case_id.to_owned(),
            item_type: item_type.to_owned(),
            birth_turn: 123,
            has_temperature,
            active: has_temperature,
            processing_speed: if item.subtypes.contains("COMESTIBLE") {
                600
            } else {
                10_000
            },
            temperature_millikelvin: state.temperature_millikelvin,
            specific_energy_millijoules_per_gram: state
                .specific_energy_millijoules_per_gram
                .ok_or("initial temperature energy disappeared")?,
            thermal_properties_present: thermal_properties.is_some(),
            specific_heat_liquid_microjoules_per_gram_kelvin: thermal_properties
                .map_or(0, |properties| {
                    properties.specific_heat_liquid_microjoules_per_gram_kelvin
                }),
            specific_heat_solid_microjoules_per_gram_kelvin: thermal_properties
                .map_or(0, |properties| {
                    properties.specific_heat_solid_microjoules_per_gram_kelvin
                }),
            latent_heat_microjoules_per_gram: thermal_properties
                .map_or(0, |properties| properties.latent_heat_microjoules_per_gram),
            freezing_point_millikelvin: thermal_properties
                .map_or(0, |properties| properties.freezing_point_millikelvin),
            ambient_specific_energy_millijoules_per_gram,
            serialized_last_temp_check_present: has_temperature,
            serialized_last_temp_check: if has_temperature {
                i32::try_from(state.last_check_tick.0)
                    .map_err(|_| "temperature tick exceeded oracle range")?
            } else {
                0
            },
            solid: current_phase == ItemPhaseV1::Solid,
            liquid: current_phase == ItemPhaseV1::Liquid,
            hot: state.hot,
            cold: state.cold,
            frozen: state.frozen,
        })
    })
    .collect::<Result<Vec<_>, String>>()
    .map_err(Into::into)
}

fn rust_item_group_rot_observation(
    workspace: &Path,
) -> Result<Vec<ItemGroupRotTraceV1>, Box<dyn std::error::Error>> {
    let manifest_path = workspace.join("vendor/cdda-content-manifest.json");
    let manifest = ContentManifest::load(&manifest_path)?;
    let content_root = manifest_path
        .parent()
        .ok_or("pinned content manifest has no parent directory")?;
    let mods = ModCatalog::load(&manifest, content_root)?;
    let enabled = mods.recommended_new_world()?;
    let items = ItemRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    ITEM_GROUP_ROT_CASES
        .into_iter()
        .map(
            |(case_id, item_type, corpse, expected_shelf_life)| -> Result<
                _,
                Box<dyn std::error::Error>,
            > {
            let item = items
                .get(item_type)
                .ok_or_else(|| format!("pinned rot item {item_type} disappeared"))?;
            let actual_corpse = item.flags.contains("CORPSE");
            let shelf_life_turns = if actual_corpse {
                i64::try_from(cdda_protocol::ITEM_STATIC_CORPSE_SHELF_LIFE_TURNS)?
            } else {
                i64::try_from(item.spoilage_lifetime_seconds)?
            };
            if actual_corpse != corpse || shelf_life_turns != expected_shelf_life {
                return Err(format!(
                    "pinned rot item {item_type} changed family or shelf life"
                )
                .into());
            }
            let removal_threshold_turns = if corpse {
                10 * 24 * 60 * 60
            } else {
                shelf_life_turns
                    .checked_mul(2)
                    .ok_or("rot threshold overflow")?
            };
            let removal_threshold_u64 = u64::try_from(removal_threshold_turns)?;
            Ok(ItemGroupRotTraceV1 {
                case_id: case_id.to_owned(),
                item_type: item_type.to_owned(),
                corpse,
                goes_bad: true,
                shelf_life_turns,
                rot_after_ten_minutes: i64::try_from(
                    cdda_sim::normal_ambient_rot_increment_turns(10 * 60)
                        .ok_or("ten-minute rot overflow")?,
                )?,
                rot_after_one_hour: i64::try_from(
                    cdda_sim::normal_ambient_rot_increment_turns(60 * 60)
                        .ok_or("one-hour rot overflow")?,
                )?,
                removal_threshold_turns,
                removed_at_threshold: cdda_sim::rot_has_rotten_away(
                    u64::try_from(shelf_life_turns)?,
                    removal_threshold_u64,
                    corpse,
                )
                .ok_or("rot threshold overflow")?,
                removed_after_threshold: cdda_sim::rot_has_rotten_away(
                    u64::try_from(shelf_life_turns)?,
                    removal_threshold_u64
                        .checked_add(1)
                        .ok_or("rot threshold overflow")?,
                    corpse,
                )
                .ok_or("rot threshold overflow")?,
            })
            },
        )
        .collect()
}

fn rust_item_group_insulated_container_observation(
    workspace: &Path,
) -> Result<ItemGroupInsulatedContainerTraceV1, Box<dyn std::error::Error>> {
    let manifest_path = workspace.join("vendor/cdda-content-manifest.json");
    let manifest = ContentManifest::load(&manifest_path)?;
    let content_root = manifest_path
        .parent()
        .ok_or("pinned content manifest has no parent directory")?;
    let mods = ModCatalog::load(&manifest, content_root)?;
    let enabled = mods.recommended_new_world()?;
    let items = ItemRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    let thermos = items
        .get("thermos")
        .ok_or("pinned item catalog has no thermos")?;
    let [pocket] = thermos.spawn_pockets.as_slice() else {
        return Err("pinned thermos must retain exactly one strict spawn pocket".into());
    };
    let mut variables = BTreeMap::new();
    variables.insert(
        cdda_protocol::item_pocket_insulation_variable_key(pocket.pocket_index),
        ItemVariableValueV1::Integer(i64::from(pocket.insulation_f32_bits)),
    );
    let insulation = cdda_protocol::item_pocket_insulation(&variables, pocket.pocket_index)
        .ok_or("thermos insulation did not round-trip through typed variables")?;
    Ok(ItemGroupInsulatedContainerTraceV1 {
        item_type: thermos.id.clone(),
        pocket_index: pocket.pocket_index,
        insulation_milli: (insulation * 1_000.0).round() as i32,
    })
}

fn rust_item_group_tool_charge_case_with_replacement(
    leaf_minimum: i32,
    leaf_maximum: i32,
    replacement_requested: Option<i32>,
) -> Result<ItemGroupToolChargeObservationV1, Box<dyn std::error::Error>> {
    let plain = |type_id: &str| CraftItemPrototypeV1 {
        type_id: type_id.to_owned(),
        charges: 0,
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
        containment: ItemContainmentProfileV1::default(),
    };
    let mut ammunition = plain("battery");
    ammunition.charges = 1;
    ammunition.ammunition_type = String::from("battery");
    ammunition.containment = ItemContainmentProfileV1 {
        volume_milliliters: 100,
        count_by_charges: true,
        stack_size: 100,
        ..ItemContainmentProfileV1::default()
    };
    let mut magazine = plain("medium_battery_cell");
    magazine.containment = ItemContainmentProfileV1 {
        weight_milligrams: 85_000,
        volume_milliliters: 17,
        longest_side_millimeters: 65,
        ..ItemContainmentProfileV1::default()
    };
    magazine.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
        pocket_index: 0,
        pocket_id: String::from("MAGAZINE"),
        ammunition_type: String::from("battery"),
        capacity: 56,
        rigid: true,
        reloadable: false,
        unloadable: false,
    }];
    let mut tool = plain("wearable_light");
    tool.magazine_wells = vec![MagazineWellPrototypeV1 {
        pocket_index: 0,
        pocket_id: String::from("MAGAZINE_WELL"),
        compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
        rigid: true,
        unloadable: true,
    }];
    let item = ItemGroupItemPrototypeV1 {
        prototype: tool,
        maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
        variants: Vec::new(),
        description_expansion: None,
        snippets: Vec::new(),
        initial_variables: BTreeMap::new(),
        default_container: None,
        modifier_side_effects_supported: true,
        charges: Some(cdda_protocol::ItemGroupChargeRangeV1 {
            minimum: leaf_minimum,
            maximum: leaf_maximum,
        }),
        minimum_one_charge: false,
        tool_charge_storage: Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index: 0,
            magazine,
            ammunition: Box::new(ammunition),
        }),
        charges_supported: true,
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
        contents_insertion_supported: true,
    };
    let definition = ItemGroupDefinitionV1 {
        group_id: String::from("direct_tool_charge"),
        graph: ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                        minimum: 0,
                        maximum: 0,
                    }),
                    variant_id: None,
                    event: None,
                    target: ItemGroupTargetV1::Item(Box::new(item)),
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
        },
    };
    let (definitions, placement_group_id) =
        if let Some(replacement_requested) = replacement_requested {
            let outer = ItemGroupDefinitionV1 {
                group_id: String::from("repeated_tool_charge"),
                graph: ItemGroupGraphV1 {
                    root_node: 0,
                    nodes: vec![ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: None,
                            event: None,
                            target: ItemGroupTargetV1::Group(definition.group_id.clone()),
                            modifier_charges: Some(cdda_protocol::ItemGroupChargeRangeV1 {
                                minimum: replacement_requested,
                                maximum: replacement_requested,
                            }),
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        }],
                    }],
                    wrapper: None,
                },
            };
            (
                vec![definition, outer],
                String::from("repeated_tool_charge"),
            )
        } else {
            (vec![definition], String::from("direct_tool_charge"))
        };
    let terrain = TerrainTileSnapshot {
        terrain_id: String::from("t_dirt"),
        move_cost: 2,
        transparent: true,
        flat: true,
        open: String::new(),
        open_move_cost: None,
        open_transparent: None,
        open_flat: None,
        close: String::new(),
        close_move_cost: None,
        close_transparent: None,
        close_flat: None,
    };
    let mut cells = vec![
        WorldgenCellV1 {
            terrain: vec![vec![WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(0),
                weight: 1,
            }]],
            furniture: vec![vec![WorldgenWeightedFurnitureTargetV1 {
                target: WorldgenFurnitureTargetV1::None,
                weight: 1,
            }]],
            item_group: None,
        };
        WORLDGEN_CELLS_PER_OMT
    ];
    cells[0].item_group = Some(WorldgenItemGroupPlacementV1 {
        group_id: placement_group_id,
        chance: 100,
        repeat_minimum: 1,
        repeat_maximum: 1,
    });
    let identity = WorldgenOmtIdentityV1 {
        full_id: String::from("direct_tool_charge_north"),
        type_id: String::from("direct_tool_charge"),
        subtype_id: String::from("direct_tool_charge"),
        generator_id: String::from("direct_tool_charge"),
        rotation: 0,
    };
    let catalog = WorldgenCatalogV1 {
        generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        overmap: WorldgenOvermapLayoutV1 {
            origin_x: -90,
            origin_y: -90,
            identities: vec![identity.clone()],
            layers: vec![WorldgenOvermapLayerV1 {
                z: 0,
                runs: vec![WorldgenOvermapRunV1 {
                    identity_index: 0,
                    length: u32::from(WORLDGEN_OVERMAP_WIDTH) * u32::from(WORLDGEN_OVERMAP_HEIGHT),
                }],
            }],
        },
        cities: Vec::new(),
        rivers: Vec::new(),
        specials: Vec::new(),
        start_location: None,
        terrain_prototypes: vec![terrain],
        furniture_prototypes: Vec::new(),
        monster_prototypes: Vec::new(),
        monster_groups: Vec::new(),
        regional_terrain: Vec::new(),
        regional_furniture: Vec::new(),
        npc_name_categories: Vec::new(),
        omt_generators: vec![WorldgenOmtGeneratorV1 {
            omt_id: identity.generator_id,
            templates: vec![WorldgenTemplateV1 {
                weight: 1,
                predecessor_id: None,
                builtin: None,
                cells,
                nested: Vec::new(),
                area_items: Vec::new(),
                npc_placements: Vec::new(),
                monster_placements: Vec::new(),
                individual_monster_placements: Vec::new(),
                erase_all_before_placing_terrain: false,
                deferred_fields: Vec::new(),
            }],
            nested_generators: Vec::new(),
        }],
    };
    let mut world = WorldState::new(5, [u8::try_from(leaf_maximum.min(255))?; 32]);
    world.install_reserved_block(ReservedIdBlock::new(1, 4_096)?)?;
    world
        .register_item_group_catalog(definitions)
        .map_err(|error| format!("Rust direct tool-charge catalog failed: {error}"))?;
    world
        .configure_worldgen(catalog)
        .map_err(|error| format!("Rust direct tool-charge worldgen failed: {error}"))?;
    world
        .generate_initial_bubble(WorldPosition { x: 0, y: 0, z: 0 })
        .map_err(|error| format!("Rust direct tool-charge generation failed: {error}"))?;
    let snapshot = world.snapshot();
    let ground = snapshot
        .ground_items
        .first()
        .ok_or("Rust direct tool-charge world generated no ground item")?;
    if snapshot
        .ground_items
        .iter()
        .any(|ground| ground.item.type_id != "wearable_light")
    {
        return Err("Rust direct tool-charge world generated a heterogeneous item trace".into());
    }
    let [well] = ground.item.magazine_wells.as_slice() else {
        return Err("Rust direct tool-charge item lost its magazine well".into());
    };
    let magazine = well
        .installed_magazine
        .as_deref()
        .ok_or("Rust direct tool-charge item did not install a magazine")?;
    let ammunition = magazine
        .integral_magazines
        .first()
        .and_then(|pocket| pocket.loaded_ammunition.as_deref());
    Ok(ItemGroupToolChargeObservationV1 {
        requested_charges: replacement_requested.unwrap_or(leaf_minimum),
        tool_type: ground.item.type_id.clone(),
        magazine_present: true,
        magazine_type: magazine.type_id.clone(),
        ammunition_type: ammunition
            .map_or_else(|| String::from("null"), |item| item.ammunition_type.clone()),
        ammunition_remaining: ammunition.map_or(0, |item| item.charges),
        // Pinned `item::remaining_ammo_capacity()` reports zero for the
        // detachable tool itself; magazine capacity is asserted separately by
        // server normalization and the Rust clamp result.
        remaining_capacity: 0,
    })
}

fn direct_mapgen_projection(observation: &MapgenOracleObservationV1) -> MapgenDirectObservationV1 {
    MapgenDirectObservationV1 {
        matching: observation.matching.clone(),
        rotatable: observation.rotatable.clone(),
        linear: observation.linear.clone(),
        static_template: observation.static_template.clone(),
        start_location: observation.start_location.clone(),
        city: observation.city.clone(),
        road: observation.road.clone(),
    }
}

fn rust_mapgen_direct_observation(
    workspace: &Path,
) -> Result<MapgenDirectObservationV1, Box<dyn std::error::Error>> {
    let manifest_path = workspace.join("vendor/cdda-content-manifest.json");
    let manifest = ContentManifest::load(&manifest_path)?;
    let content_root = manifest_path
        .parent()
        .ok_or("pinned content manifest has no parent directory")?;
    let mods = ModCatalog::load(&manifest, content_root)?;
    let enabled = mods.recommended_new_world()?;
    let terrain = OvermapTerrainRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    let start_locations =
        StartLocationRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;
    let city_settings =
        CitySettingsRegistry::load_selected(&manifest, content_root, &mods, &enabled)?;

    let match_cases = [
        (
            "exact_full",
            "shelter_north",
            "shelter_north",
            "EXACT",
            WorldgenOmtMatchTypeV1::Exact,
        ),
        (
            "exact_base_rejected",
            "shelter",
            "shelter_north",
            "EXACT",
            WorldgenOmtMatchTypeV1::Exact,
        ),
        (
            "rotatable_type",
            "shelter",
            "shelter_east",
            "TYPE",
            WorldgenOmtMatchTypeV1::Type,
        ),
        (
            "linear_subtype",
            "road_straight",
            "road_ew",
            "SUBTYPE",
            WorldgenOmtMatchTypeV1::Subtype,
        ),
        (
            "wrong_linear_subtype",
            "road_curved",
            "road_ew",
            "SUBTYPE",
            WorldgenOmtMatchTypeV1::Subtype,
        ),
        (
            "prefix_separator",
            "forest",
            "forest_thick",
            "PREFIX",
            WorldgenOmtMatchTypeV1::Prefix,
        ),
        (
            "partial_prefix_rejected",
            "fore",
            "forest_thick",
            "PREFIX",
            WorldgenOmtMatchTypeV1::Prefix,
        ),
        (
            "contains_substring",
            "rest_t",
            "forest_thick",
            "CONTAINS",
            WorldgenOmtMatchTypeV1::Contains,
        ),
    ];
    let matching = match_cases
        .into_iter()
        .map(|(case_id, query, terrain_id, match_name, match_type)| {
            let identity = protocol_omt_identity(&terrain, terrain_id)?;
            Ok(MapgenMatchObservationV1 {
                case_id: case_id.to_owned(),
                query: query.to_owned(),
                terrain_id: terrain_id.to_owned(),
                match_type: match_name.to_owned(),
                matches: worldgen_omt_matches(query, match_type, &identity),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

    let directions = ["north", "east", "south", "west"];
    let rotatable_ids = [
        "shelter_north",
        "shelter_east",
        "shelter_south",
        "shelter_west",
    ];
    let linear_ids = ["road_ns", "road_ew", "road_ns", "road_ew"];
    let rotatable = rust_rotation_observations(&terrain, directions, rotatable_ids)?;
    let linear = rust_rotation_observations(&terrain, directions, linear_ids)?;
    let north = rotatable
        .first()
        .ok_or("rotatable Rust mapgen observation is empty")?;
    let template_tiles =
        rust_static_template_tiles(&protocol_omt_identity(&terrain, "shelter_north")?)?;
    if (north.marker_x, north.marker_y) != (2, 5) {
        return Err("north static-template marker did not remain at its source coordinate".into());
    }
    Ok(MapgenDirectObservationV1 {
        matching,
        rotatable,
        linear,
        static_template: MapgenStaticTemplateObservationV1 {
            width_tiles: i32::try_from(WORLDGEN_OMT_SIZE)?,
            height_tiles: i32::try_from(WORLDGEN_OMT_SIZE)?,
            source_marker_x: 2,
            source_marker_y: 5,
            background_terrain_id: String::from("t_dirt"),
            marker_terrain_id: String::from("t_floor"),
            marker_furniture_id: String::from("f_table"),
            generated_background_terrain_id: template_tiles.background,
            generated_marker_terrain_id: template_tiles.marker_terrain,
            generated_marker_furniture_id: template_tiles.marker_furniture,
            generated_rows: template_tiles.generated_rows,
            piece_phases: vec![String::from("terrain"), String::from("furniture")],
            setup_completed: true,
        },
        start_location: rust_start_location_observation(&start_locations, &terrain)?,
        city: rust_city_observation(&city_settings)?,
        road: rust_road_observation()?,
    })
}

fn rust_road_observation() -> Result<MapgenRoadObservationV1, Box<dyn std::error::Error>> {
    let points = [
        ChunkCoord { x: 10, y: 0, z: 0 },
        ChunkCoord {
            x: 179,
            y: 40,
            z: 0,
        },
        ChunkCoord {
            x: 100,
            y: 179,
            z: 0,
        },
        ChunkCoord { x: 70, y: 70, z: 0 },
        ChunkCoord {
            x: 110,
            y: 75,
            z: 0,
        },
        ChunkCoord {
            x: 90,
            y: 115,
            z: 0,
        },
    ];
    let edges = overmap_road_mst_edges(&points)?;
    Ok(MapgenRoadObservationV1 {
        point_x: points.iter().map(|point| point.x).collect(),
        point_y: points.iter().map(|point| point.y).collect(),
        mst_left: edges.iter().map(|edge| i32::from(edge.0)).collect(),
        mst_right: edges.iter().map(|edge| i32::from(edge.1)).collect(),
    })
}

fn rust_city_observation(
    registry: &CitySettingsRegistry,
) -> Result<MapgenCityObservationV1, Box<dyn std::error::Error>> {
    let settings = registry
        .get(DEFAULT_CITY_SETTINGS_ID)
        .ok_or("pinned Rust content is missing default city settings")?;
    let city = WorldgenCityV1 {
        city_id: WorldgenCityId(1),
        center: ChunkCoord { x: 90, y: 90, z: 0 },
        size: 8,
    };
    let points = [
        ChunkCoord { x: 90, y: 90, z: 0 },
        ChunkCoord { x: 98, y: 90, z: 0 },
        ChunkCoord { x: 98, y: 98, z: 0 },
        ChunkCoord {
            x: 106,
            y: 90,
            z: 0,
        },
    ];
    let start_distances = points
        .iter()
        .map(|point| worldgen_city_start_distance(&city, *point))
        .collect::<Vec<_>>();
    Ok(MapgenCityObservationV1 {
        settings_id: settings.id.clone(),
        city_size: i32::from(settings.city_size),
        city_spacing: i32::from(settings.city_spacing),
        is_megacity: settings.is_megacity,
        center_x: city.center.x,
        center_y: city.center.y,
        size: i32::from(city.size),
        point_x: points.iter().map(|point| point.x).collect(),
        point_y: points.iter().map(|point| point.y).collect(),
        edge_distances: start_distances
            .iter()
            .map(|distance| distance.saturating_add(i32::from(city.size)))
            .collect(),
        start_distances,
        random_count_floor: 9,
        random_count_ceiling: 10,
        minimum_generated_size: 2,
        maximum_generated_size: 55,
    })
}

fn rust_start_location_observation(
    registry: &StartLocationRegistry,
    terrain: &OvermapTerrainRegistry,
) -> Result<MapgenStartLocationObservationV1, Box<dyn std::error::Error>> {
    let start = registry
        .get("sloc_lmoe")
        .ok_or("pinned Rust content is missing start location sloc_lmoe")?;
    if start.targets.len() != 1 {
        return Err("pinned sloc_lmoe no longer has exactly one target".into());
    }
    let chosen_target = start
        .targets
        .first()
        .ok_or("pinned sloc_lmoe has no selectable target")?;
    let (match_type_name, match_type) = protocol_match_type(chosen_target.match_type);
    let candidate_identity_ids = ["shelter_north", "lmoe_north", "road_ew", "forest_thick"];
    let mut matching_candidate_ids = Vec::new();
    for candidate in candidate_identity_ids {
        let identity = protocol_omt_identity(terrain, candidate)?;
        if worldgen_omt_matches(&chosen_target.overmap_terrain, match_type, &identity) {
            matching_candidate_ids.push(candidate.to_owned());
        }
    }
    let selected_candidate_id = matching_candidate_ids
        .first()
        .ok_or("pinned sloc_lmoe target matched no normalized candidate")?
        .clone();
    Ok(MapgenStartLocationObservationV1 {
        start_location_id: start.id.clone(),
        target_count: i32::try_from(start.targets.len())?,
        chosen_target_index: 0,
        chosen_target_omt: chosen_target.overmap_terrain.clone(),
        chosen_target_match_type: match_type_name.to_owned(),
        chosen_target_parameter_count: i32::try_from(chosen_target.parameters.len())?,
        requires_city: start.requires_city(),
        city_size_minimum: start.city_sizes.minimum,
        city_size_maximum: start.city_sizes.maximum,
        city_distance_minimum: start.city_distance.minimum,
        city_distance_maximum: start.city_distance.maximum,
        allowed_z_minimum: start.allowed_z_levels.minimum,
        allowed_z_maximum: start.allowed_z_levels.maximum,
        flags: start.flags.iter().cloned().collect(),
        runtime_selectable_without_cities: start.is_runtime_selectable_without_cities(),
        candidate_identity_ids: candidate_identity_ids
            .into_iter()
            .map(str::to_owned)
            .collect(),
        matching_candidate_ids,
        selected_candidate_id,
    })
}

fn protocol_match_type(
    match_type: OvermapTerrainMatchType,
) -> (&'static str, WorldgenOmtMatchTypeV1) {
    match match_type {
        OvermapTerrainMatchType::Exact => ("EXACT", WorldgenOmtMatchTypeV1::Exact),
        OvermapTerrainMatchType::Type => ("TYPE", WorldgenOmtMatchTypeV1::Type),
        OvermapTerrainMatchType::Subtype => ("SUBTYPE", WorldgenOmtMatchTypeV1::Subtype),
        OvermapTerrainMatchType::Prefix => ("PREFIX", WorldgenOmtMatchTypeV1::Prefix),
        OvermapTerrainMatchType::Contains => ("CONTAINS", WorldgenOmtMatchTypeV1::Contains),
    }
}

fn protocol_omt_identity(
    registry: &OvermapTerrainRegistry,
    full_id: &str,
) -> Result<WorldgenOmtIdentityV1, Box<dyn std::error::Error>> {
    let identity = registry
        .get_identity(full_id)
        .ok_or_else(|| format!("pinned Rust content is missing OMT identity {full_id}"))?;
    Ok(WorldgenOmtIdentityV1 {
        full_id: identity.full_id.clone(),
        type_id: identity.type_id.clone(),
        subtype_id: identity.subtype_id.clone(),
        generator_id: identity.generator_id.clone(),
        rotation: identity.rotation,
    })
}

fn rust_rotation_observations<const N: usize>(
    registry: &OvermapTerrainRegistry,
    directions: [&str; N],
    full_ids: [&str; N],
) -> Result<Vec<MapgenRotationObservationV1>, Box<dyn std::error::Error>> {
    directions
        .into_iter()
        .zip(full_ids)
        .map(|(direction, full_id)| {
            let identity = protocol_omt_identity(registry, full_id)?;
            let (marker_x, marker_y) = rust_static_template_marker(&identity)?;
            Ok(MapgenRotationObservationV1 {
                direction: direction.to_owned(),
                terrain_id: identity.full_id,
                mapgen_id: identity.generator_id,
                rotation: i32::from(identity.rotation),
                marker_x,
                marker_y,
            })
        })
        .collect()
}

fn rust_static_template_marker(
    identity: &WorldgenOmtIdentityV1,
) -> Result<(i32, i32), Box<dyn std::error::Error>> {
    let snapshot = rust_static_template_snapshot(identity)?;
    rust_static_template_marker_in(&snapshot)
}

fn rust_static_template_marker_in(
    snapshot: &WorldSnapshotV1,
) -> Result<(i32, i32), Box<dyn std::error::Error>> {
    let mut marker = None;
    for y in 0..i32::try_from(WORLDGEN_OMT_SIZE)? {
        for x in 0..i32::try_from(WORLDGEN_OMT_SIZE)? {
            let (terrain, furniture) = generated_tile(snapshot, x, y)?;
            if terrain == "t_floor" {
                if furniture.as_deref() != Some("f_table") || marker.replace((x, y)).is_some() {
                    return Err("Rust static mapgen produced an invalid marker shape".into());
                }
            } else if terrain != "t_dirt" || furniture.is_some() {
                return Err("Rust static mapgen produced an unexpected background tile".into());
            }
        }
    }
    marker.ok_or_else(|| "Rust static mapgen did not produce its marker".into())
}

fn rust_static_template_tiles(
    identity: &WorldgenOmtIdentityV1,
) -> Result<RustStaticTemplateTiles, Box<dyn std::error::Error>> {
    let snapshot = rust_static_template_snapshot(identity)?;
    let (background, background_furniture) = generated_tile(&snapshot, 0, 0)?;
    if background_furniture.is_some() {
        return Err("Rust static mapgen background unexpectedly has furniture".into());
    }
    let (marker_x, marker_y) = rust_static_template_marker_in(&snapshot)?;
    let (marker, furniture) = generated_tile(&snapshot, marker_x, marker_y)?;
    let mut generated_rows = Vec::with_capacity(WORLDGEN_OMT_SIZE);
    for y in 0..i32::try_from(WORLDGEN_OMT_SIZE)? {
        let mut row = String::with_capacity(WORLDGEN_OMT_SIZE);
        for x in 0..i32::try_from(WORLDGEN_OMT_SIZE)? {
            let (terrain, furniture) = generated_tile(&snapshot, x, y)?;
            row.push(
                if terrain == "t_floor" && furniture.as_deref() == Some("f_table") {
                    'X'
                } else if terrain == "t_dirt" && furniture.is_none() {
                    '.'
                } else {
                    return Err(
                        "Rust static mapgen cannot encode an unexpected generated tile".into(),
                    );
                },
            );
        }
        generated_rows.push(row);
    }
    Ok(RustStaticTemplateTiles {
        background,
        marker_terrain: marker,
        marker_furniture: furniture.ok_or("Rust static mapgen marker has no furniture")?,
        generated_rows,
    })
}

fn rust_static_template_snapshot(
    identity: &WorldgenOmtIdentityV1,
) -> Result<WorldSnapshotV1, Box<dyn std::error::Error>> {
    let background = TerrainTileSnapshot {
        terrain_id: String::from("t_dirt"),
        move_cost: 2,
        transparent: true,
        flat: true,
        open: String::new(),
        open_move_cost: None,
        open_transparent: None,
        open_flat: None,
        close: String::new(),
        close_move_cost: None,
        close_transparent: None,
        close_flat: None,
    };
    let marker = TerrainTileSnapshot {
        terrain_id: String::from("t_floor"),
        ..background.clone()
    };
    let mut cells = vec![
        WorldgenCellV1 {
            terrain: vec![vec![WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(0),
                weight: 1,
            }]],
            furniture: vec![vec![WorldgenWeightedFurnitureTargetV1 {
                target: WorldgenFurnitureTargetV1::None,
                weight: 1,
            }]],
            item_group: None,
        };
        WORLDGEN_CELLS_PER_OMT
    ];
    let source_marker = 5 * WORLDGEN_OMT_SIZE + 2;
    cells[source_marker] = WorldgenCellV1 {
        terrain: vec![vec![WorldgenWeightedTerrainTargetV1 {
            target: WorldgenTerrainTargetV1::Prototype(1),
            weight: 1,
        }]],
        furniture: vec![vec![WorldgenWeightedFurnitureTargetV1 {
            target: WorldgenFurnitureTargetV1::Prototype(0),
            weight: 1,
        }]],
        item_group: None,
    };
    let catalog = WorldgenCatalogV1 {
        generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
        overmap: WorldgenOvermapLayoutV1 {
            origin_x: -90,
            origin_y: -90,
            identities: vec![identity.clone()],
            layers: vec![WorldgenOvermapLayerV1 {
                z: 0,
                runs: vec![WorldgenOvermapRunV1 {
                    identity_index: 0,
                    length: u32::from(WORLDGEN_OVERMAP_WIDTH) * u32::from(WORLDGEN_OVERMAP_HEIGHT),
                }],
            }],
        },
        cities: Vec::new(),
        rivers: Vec::new(),
        specials: Vec::new(),
        start_location: None,
        terrain_prototypes: vec![background, marker],
        furniture_prototypes: vec![FurnitureTileSnapshot {
            furniture_id: String::from("f_table"),
            move_cost_mod: 0,
            transparent: true,
            blocks_door: false,
            comfort: 0,
            floor_bedding_warmth: 0,
        }],
        monster_prototypes: Vec::new(),
        monster_groups: Vec::new(),
        regional_terrain: Vec::new(),
        regional_furniture: Vec::new(),
        npc_name_categories: Vec::new(),
        omt_generators: vec![WorldgenOmtGeneratorV1 {
            omt_id: identity.generator_id.clone(),
            templates: vec![WorldgenTemplateV1 {
                weight: 1,
                predecessor_id: None,
                builtin: None,
                cells,
                nested: Vec::new(),
                area_items: Vec::new(),
                npc_placements: Vec::new(),
                monster_placements: Vec::new(),
                individual_monster_placements: Vec::new(),
                erase_all_before_placing_terrain: false,
                deferred_fields: Vec::new(),
            }],
            nested_generators: Vec::new(),
        }],
    };
    let mut world = WorldState::new(1, [47; 32]);
    world.configure_worldgen(catalog)?;
    world.generate_initial_bubble(WorldPosition { x: 0, y: 0, z: 0 })?;
    Ok(world.snapshot())
}

fn generated_tile(
    snapshot: &WorldSnapshotV1,
    x: i32,
    y: i32,
) -> Result<(String, Option<String>), Box<dyn std::error::Error>> {
    let submap_size = cdda_protocol::SUBMAP_SIZE;
    let coord = ChunkCoord {
        x: x.div_euclid(submap_size),
        y: y.div_euclid(submap_size),
        z: 0,
    };
    let local_x = usize::try_from(x.rem_euclid(submap_size))?;
    let local_y = usize::try_from(y.rem_euclid(submap_size))?;
    let width = usize::try_from(submap_size)?;
    let index = local_y
        .checked_mul(width)
        .and_then(|row| row.checked_add(local_x))
        .ok_or("Rust static mapgen tile index overflow")?;
    let chunk = snapshot
        .chunks
        .iter()
        .find(|chunk| chunk.coord == coord)
        .ok_or("Rust static mapgen omitted an expected chunk")?;
    let terrain = chunk
        .tiles
        .get(index)
        .ok_or("Rust static mapgen omitted an expected terrain tile")?
        .terrain_id
        .clone();
    let furniture = chunk
        .furniture
        .get(index)
        .ok_or("Rust static mapgen omitted an expected furniture tile")?
        .as_ref()
        .map(|furniture| furniture.furniture_id.clone());
    Ok((terrain, furniture))
}

fn validate_rotations(
    observations: &[MapgenRotationObservationV1],
    expected_family: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let directions = ["north", "east", "south", "west"];
    if observations.len() != directions.len()
        || observations
            .iter()
            .zip(directions)
            .any(|(observation, direction)| {
                observation.direction != direction
                    || observation.terrain_id.is_empty()
                    || observation.mapgen_id.is_empty()
                    || !observation.terrain_id.starts_with(expected_family)
                    || !observation.mapgen_id.starts_with(expected_family)
                    || !(0..=3).contains(&observation.rotation)
                    || !(0..24).contains(&observation.marker_x)
                    || !(0..24).contains(&observation.marker_y)
            })
    {
        return Err(format!("invalid {expected_family} rotation observation").into());
    }
    Ok(())
}

fn ranges_match(
    actual: &[ItemGroupRangeObservationV1],
    expected: &[(&str, i32, i32, i32)],
) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            actual.case_id == expected.0
                && actual.minimum == expected.1
                && actual.maximum == expected.2
                && actual.target == expected.3
                && actual.observed == actual.target
        })
}

fn validate_upstream(upstream: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let canonical = fs::canonicalize(upstream).map_err(|error| {
        format!(
            "could not resolve upstream checkout {}: {error}",
            upstream.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err("upstream checkout is not a directory".into());
    }
    let head = git_output(&canonical, &["rev-parse", "HEAD"])?;
    let tree_spec = format!("{BASELINE_COMMIT}^{{tree}}");
    let tree = git_output(&canonical, &["rev-parse", tree_spec.as_str()])?;
    if head != BASELINE_COMMIT || tree != UPSTREAM_TREE {
        return Err(format!(
            "upstream checkout identity mismatch: expected commit {BASELINE_COMMIT} tree {UPSTREAM_TREE}, got commit {head} tree {tree}"
        )
        .into());
    }
    Ok(())
}

fn git_output(upstream: &Path, arguments: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(upstream)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed for {}: {}",
            arguments.join(" "),
            upstream.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn prepare_binary(
    workspace: &Path,
    upstream: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = workspace.join("target/cpp-oracle").join(BASELINE_COMMIT);
    let binary = root.join("tests/cata_test");
    let cache_path = root.join(".rust-cpp-oracle-cache.json");
    let adapter_hash = blake3::hash(
        [
            ADAPTER_SOURCE.as_bytes(),
            ITEM_GROUP_ADAPTER_SOURCE.as_bytes(),
            MAPGEN_ADAPTER_SOURCE.as_bytes(),
            ADAPTER_MAKEFILE.as_bytes(),
        ]
        .concat()
        .as_slice(),
    )
    .to_hex()
    .to_string();
    if reusable_binary(&cache_path, &binary, &adapter_hash)? {
        return Ok(binary);
    }
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;
    export_upstream(upstream, &root)?;
    fs::write(
        root.join("tests/rust_cpp_oracle_item_pocket_test.cpp"),
        ADAPTER_SOURCE,
    )?;
    fs::write(
        root.join("tests/rust_cpp_oracle_item_group_test.cpp"),
        ITEM_GROUP_ADAPTER_SOURCE,
    )?;
    fs::write(
        root.join("tests/rust_cpp_oracle_mapgen_test.cpp"),
        MAPGEN_ADAPTER_SOURCE,
    )?;
    fs::write(root.join("rust-cpp-oracle.mk"), ADAPTER_MAKEFILE)?;

    let parallelism = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .clamp(1, 32);
    let mut build = Command::new("make");
    build
        .arg("--silent")
        .arg("-f")
        .arg("Makefile")
        .arg("-f")
        .arg("rust-cpp-oracle.mk")
        .arg(format!("-j{parallelism}"))
        .arg("rust-cpp-oracle")
        .args([
            "RELEASE=1",
            "LOCALIZE=0",
            "BACKTRACE=0",
            "TILES=0",
            "SOUND=0",
            "USE_HOME_DIR=0",
        ])
        .current_dir(&root);
    if let Some(pkg_config_path) = macos_ncurses_pkg_config_path()? {
        build.env("PKG_CONFIG_PATH", pkg_config_path);
    }
    let status = build.status()?;
    if !status.success() || !binary.is_file() {
        return Err(format!(
            "pinned C++ oracle build failed in {} with status {status}",
            root.display()
        )
        .into());
    }
    let cache = OracleCacheV1 {
        format_version: CACHE_FORMAT_VERSION,
        baseline_commit: BASELINE_COMMIT.to_owned(),
        upstream_tree: UPSTREAM_TREE.to_owned(),
        adapter_hash,
        binary_hash: blake3_file(&binary)?,
    };
    let mut stamp_file = fs::File::create(cache_path)?;
    serde_json::to_writer(&mut stamp_file, &cache)?;
    writeln!(stamp_file)?;
    stamp_file.sync_all()?;
    Ok(binary)
}

fn reusable_binary(
    cache_path: &Path,
    binary: &Path,
    expected_adapter_hash: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    if !binary.is_file() || !cache_path.is_file() {
        return Ok(false);
    }
    let cache_bytes = match read_bounded(cache_path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(false),
    };
    let cache = match serde_json::from_slice::<OracleCacheV1>(&cache_bytes) {
        Ok(cache) => cache,
        Err(_) => return Ok(false),
    };
    if cache.format_version != CACHE_FORMAT_VERSION
        || cache.baseline_commit != BASELINE_COMMIT
        || cache.upstream_tree != UPSTREAM_TREE
        || cache.adapter_hash != expected_adapter_hash
        || cache.binary_hash.parse::<blake3::Hash>().is_err()
    {
        return Ok(false);
    }
    Ok(blake3_file(binary)? == cache.binary_hash)
}

fn blake3_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(format!("cache executable is not a regular file: {}", path.display()).into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn macos_ncurses_pkg_config_path() -> Result<Option<String>, Box<dyn std::error::Error>> {
    if !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let output = Command::new("brew")
        .args(["--prefix", "ncurses"])
        .output()
        .map_err(|error| {
            format!(
                "the pinned C++ build requires Homebrew ncursesw on macOS; could not run `brew --prefix ncurses`: {error}"
            )
        })?;
    if !output.status.success() {
        return Err(
            "the pinned C++ build requires Homebrew ncursesw on macOS; install it with `brew install ncurses`"
                .into(),
        );
    }
    let prefix = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let pkg_config = prefix.join("lib/pkgconfig");
    if !pkg_config.is_dir() {
        return Err(format!(
            "Homebrew ncurses pkg-config directory is missing: {}",
            pkg_config.display()
        )
        .into());
    }
    let mut combined = pkg_config.into_os_string().into_string().map_err(
        |_| "Homebrew ncurses pkg-config path cannot be represented as UTF-8 on this host",
    )?;
    if let Some(existing) = std::env::var_os("PKG_CONFIG_PATH") {
        let existing = existing
            .into_string()
            .map_err(|_| "PKG_CONFIG_PATH cannot be represented as UTF-8 on this host")?;
        if !existing.is_empty() {
            combined.push(':');
            combined.push_str(&existing);
        }
    }
    Ok(Some(combined))
}

fn export_upstream(upstream: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    export_upstream_paths(upstream, destination, &[])
}

fn export_upstream_paths(
    upstream: &Path,
    destination: &Path,
    paths: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive_command = Command::new("git");
    archive_command
        .arg("-C")
        .arg(upstream)
        .args(["archive", "--format=tar", BASELINE_COMMIT])
        .args(paths)
        .stdout(Stdio::piped());
    let mut archive = archive_command.spawn()?;
    let archive_stdout = archive.stdout.take().ok_or("git archive has no stdout")?;
    let extract_status = Command::new("tar")
        .args(["-xf", "-", "-C"])
        .arg(destination)
        .stdin(Stdio::from(archive_stdout))
        .status()?;
    let archive_status = archive.wait()?;
    if !archive_status.success() || !extract_status.success() {
        return Err(format!(
            "could not export pinned upstream: git {archive_status}, tar {extract_status}"
        )
        .into());
    }
    Ok(())
}

fn run_binary(
    workspace: &Path,
    upstream: &Path,
    binary: &Path,
) -> Result<OracleObservationV1, Box<dyn std::error::Error>> {
    cleanup_legacy_run_artifacts(workspace)?;
    let run_root = workspace.join("target/cpp-oracle/runtime");
    if run_root.exists() {
        fs::remove_dir_all(&run_root)?;
    }
    fs::create_dir_all(&run_root)?;
    let _artifacts = OracleRunArtifacts {
        root: run_root.clone(),
    };
    let output_path = run_root.join("observation.json");
    let user_dir = run_root.join("user");
    export_upstream_paths(upstream, &run_root, &["data"])?;
    let status = Command::new(binary)
        .arg("rust_cpp_oracle_item_pocket_max_length")
        .args(["--rng-seed", "1", "--order", "lex", "--drop-world"])
        .arg("--user-dir")
        .arg(&user_dir)
        .env("CDDA_RUST_CPP_ORACLE_OUTPUT", &output_path)
        .env("LANGUAGE", "en")
        .env("LC_ALL", "C")
        .current_dir(&run_root)
        .status()?;
    if !status.success() {
        return Err(format!("pinned C++ oracle execution failed with status {status}").into());
    }
    let bytes = read_bounded(&output_path)?;
    let observation: OracleObservationV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("C++ oracle emitted invalid observation JSON: {error}"))?;
    validate_observation(&observation)?;
    Ok(observation)
}

fn run_item_group_binary(
    workspace: &Path,
    upstream: &Path,
    binary: &Path,
) -> Result<ItemGroupOracleObservationV1, Box<dyn std::error::Error>> {
    cleanup_legacy_run_artifacts(workspace)?;
    let run_root = workspace.join("target/cpp-oracle/runtime");
    if run_root.exists() {
        fs::remove_dir_all(&run_root)?;
    }
    fs::create_dir_all(&run_root)?;
    let _artifacts = OracleRunArtifacts {
        root: run_root.clone(),
    };
    let output_path = run_root.join("observation.json");
    let user_dir = run_root.join("user");
    export_upstream_paths(upstream, &run_root, &["data"])?;
    let status = Command::new(binary)
        .arg("rust_cpp_oracle_item_group_generation")
        .args(["--rng-seed", "1", "--order", "lex", "--drop-world"])
        .arg("--user-dir")
        .arg(&user_dir)
        .env("CDDA_RUST_CPP_ORACLE_OUTPUT", &output_path)
        .env("LANGUAGE", "en")
        .env("LC_ALL", "C")
        .current_dir(&run_root)
        .status()?;
    if !status.success() {
        return Err(
            format!("pinned C++ item-group oracle execution failed with status {status}").into(),
        );
    }
    let bytes = read_bounded(&output_path)?;
    let observation: ItemGroupOracleObservationV1 =
        serde_json::from_slice(&bytes).map_err(|error| {
            format!("C++ item-group oracle emitted invalid observation JSON: {error}")
        })?;
    validate_item_group_observation(&observation)?;
    Ok(observation)
}

fn run_mapgen_binary(
    workspace: &Path,
    upstream: &Path,
    binary: &Path,
) -> Result<MapgenOracleObservationV1, Box<dyn std::error::Error>> {
    cleanup_legacy_run_artifacts(workspace)?;
    let run_root = workspace.join("target/cpp-oracle/runtime");
    if run_root.exists() {
        fs::remove_dir_all(&run_root)?;
    }
    fs::create_dir_all(&run_root)?;
    let _artifacts = OracleRunArtifacts {
        root: run_root.clone(),
    };
    let output_path = run_root.join("observation.json");
    let user_dir = run_root.join("user");
    export_upstream_paths(upstream, &run_root, &["data"])?;
    let status = Command::new(binary)
        .arg("rust_cpp_oracle_mapgen_static_semantics")
        .args(["--rng-seed", "1", "--order", "lex", "--drop-world"])
        .arg("--user-dir")
        .arg(&user_dir)
        .env("CDDA_RUST_CPP_ORACLE_OUTPUT", &output_path)
        .env("LANGUAGE", "en")
        .env("LC_ALL", "C")
        .current_dir(&run_root)
        .status()?;
    if !status.success() {
        return Err(
            format!("pinned C++ mapgen oracle execution failed with status {status}").into(),
        );
    }
    let bytes = read_bounded(&output_path)?;
    let observation: MapgenOracleObservationV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("C++ mapgen oracle emitted invalid observation JSON: {error}"))?;
    validate_mapgen_observation(&observation)?;
    Ok(observation)
}

fn cleanup_legacy_run_artifacts(workspace: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root = workspace.join("target/cpp-oracle");
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let observation = name
            .strip_prefix("observation-")
            .and_then(|suffix| suffix.strip_suffix(".json"));
        let test_user = name.strip_prefix("test-user-");
        if observation.is_some_and(|process| {
            !process.is_empty() && process.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            let file_type = entry.file_type()?;
            if file_type.is_file() || file_type.is_symlink() {
                fs::remove_file(entry.path())?;
            }
        } else if test_user.is_some_and(|process| {
            !process.is_empty() && process.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else if file_type.is_file() || file_type.is_symlink() {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn compare(
    scenario: &OracleScenarioV1,
    observation: &OracleObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if &scenario.expected_observation == observation {
        return Ok(());
    }
    Err(format!(
        "C++ oracle diverged from the checked scenario\nexpected: {}\nactual: {}",
        serde_json::to_string_pretty(&scenario.expected_observation)?,
        serde_json::to_string_pretty(observation)?
    )
    .into())
}

fn compare_item_group(
    scenario: &ItemGroupOracleScenarioV1,
    observation: &ItemGroupOracleObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if &scenario.expected_observation == observation {
        return Ok(());
    }
    Err(format!(
        "C++ item-group oracle diverged from the checked scenario\nexpected: {}\nactual: {}",
        serde_json::to_string_pretty(&scenario.expected_observation)?,
        serde_json::to_string_pretty(observation)?
    )
    .into())
}

fn compare_mapgen(
    scenario: &MapgenOracleScenarioV1,
    observation: &MapgenOracleObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    if &scenario.expected_observation == observation {
        return Ok(());
    }
    Err(format!(
        "C++ mapgen oracle diverged from the checked scenario\nexpected: {}\nactual: {}",
        serde_json::to_string_pretty(&scenario.expected_observation)?,
        serde_json::to_string_pretty(observation)?
    )
    .into())
}

fn compare_direct_observation<T>(
    family: &str,
    cpp: &T,
    rust: &T,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: PartialEq + Serialize,
{
    if cpp == rust {
        return Ok(());
    }
    Err(format!(
        "direct Rust-to-C++ {family} comparison diverged\nC++: {}\nRust: {}",
        serde_json::to_string_pretty(cpp)?,
        serde_json::to_string_pretty(rust)?
    )
    .into())
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = fs::File::open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    if !file.metadata()?.is_file() {
        return Err(format!("JSON input {} is absent or exceeds 1 MiB", path.display()).into());
    }
    read_bounded_from(file, MAX_JSON_BYTES).map_err(|error| {
        format!(
            "JSON input {} is absent or exceeds 1 MiB: {error}",
            path.display()
        )
        .into()
    })
}

fn read_bounded_from(
    reader: impl Read,
    maximum_bytes: u64,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    reader
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > maximum_bytes) {
        return Err("input exceeds its byte limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(1);

    fn checked_scenario() -> OracleScenarioV1 {
        serde_json::from_str(include_str!(
            "../../../docs/oracles/item-pocket-max-length-v1.json"
        ))
        .expect("checked oracle scenario should decode")
    }

    fn checked_item_group_scenario() -> ItemGroupOracleScenarioV1 {
        serde_json::from_str(include_str!(
            "../../../docs/oracles/item-group-generation-v1.json"
        ))
        .expect("checked item-group oracle scenario should decode")
    }

    fn checked_mapgen_scenario() -> MapgenOracleScenarioV1 {
        serde_json::from_str(include_str!(
            "../../../docs/oracles/mapgen-static-semantics-v1.json"
        ))
        .expect("checked mapgen oracle scenario should decode")
    }

    #[test]
    fn bounded_reader_enforces_the_limit_while_reading() {
        assert_eq!(
            read_bounded_from(std::io::Cursor::new(b"1234"), 4)
                .expect("input at the limit should read"),
            b"1234"
        );
        assert!(read_bounded_from(std::io::Cursor::new(b"12345"), 4).is_err());
    }

    #[test]
    fn cached_binary_is_reused_only_while_its_digest_matches() {
        let unique = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "cdda-rust-cpp-oracle-cache-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary cache should create");
        let binary = root.join("cata_test");
        let cache_path = root.join("cache.json");
        let adapter_hash = "adapter";
        fs::write(&binary, b"exact binary").expect("binary fixture should write");
        let cache = OracleCacheV1 {
            format_version: CACHE_FORMAT_VERSION,
            baseline_commit: BASELINE_COMMIT.to_owned(),
            upstream_tree: UPSTREAM_TREE.to_owned(),
            adapter_hash: adapter_hash.to_owned(),
            binary_hash: blake3_file(&binary).expect("binary fixture should hash"),
        };
        fs::write(
            &cache_path,
            serde_json::to_vec(&cache).expect("cache fixture should encode"),
        )
        .expect("cache fixture should write");
        assert!(
            reusable_binary(&cache_path, &binary, adapter_hash)
                .expect("matching cache should validate")
        );
        fs::write(&binary, b"polluted binary").expect("binary fixture should mutate");
        assert!(
            !reusable_binary(&cache_path, &binary, adapter_hash)
                .expect("mismatched cache should validate as unusable")
        );
        fs::remove_dir_all(root).expect("temporary cache should clean up");
    }

    #[test]
    fn checked_scenario_is_strict_and_version_bound() {
        let scenario = checked_scenario();
        validate_scenario(&scenario).expect("checked oracle scenario should validate");

        let mut wrong_baseline = checked_scenario();
        wrong_baseline.baseline_commit = String::from("wrong");
        assert!(validate_scenario(&wrong_baseline).is_err());

        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/oracles/item-pocket-max-length-v1.json"
        ))
        .expect("checked oracle scenario should decode as a value");
        value
            .as_object_mut()
            .expect("scenario is an object")
            .insert(String::from("unknown"), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<OracleScenarioV1>(value).is_err());

        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/oracles/item-pocket-max-length-v1.json"
        ))
        .expect("checked oracle scenario should decode as a value");
        value["expected_observation"]["pocket"]["unknown"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<OracleScenarioV1>(value).is_err());
    }

    #[test]
    fn observation_validation_rejects_duplicates_and_inconsistent_success() {
        let mut scenario = checked_scenario();
        scenario.expected_observation.cases[1].case_id = String::from("shorter");
        assert!(validate_observation(&scenario.expected_observation).is_err());

        let mut scenario = checked_scenario();
        scenario.expected_observation.cases[0].contain_code = 4;
        assert!(validate_observation(&scenario.expected_observation).is_err());
    }

    #[test]
    fn checked_item_group_scenario_is_complete_and_version_bound() {
        let scenario = checked_item_group_scenario();
        validate_item_group_scenario(&scenario)
            .expect("checked item-group oracle scenario should validate");

        let mut incomplete = checked_item_group_scenario();
        incomplete.expected_observation.distribution.pop();
        assert!(validate_item_group_scenario(&incomplete).is_err());

        let mut bad_stream = checked_item_group_scenario();
        bad_stream
            .expected_observation
            .nested
            .downstream_draw_matches = false;
        assert!(validate_item_group_scenario(&bad_stream).is_err());
    }

    #[test]
    fn checked_mapgen_scenario_is_complete_and_version_bound() {
        let scenario = checked_mapgen_scenario();
        validate_mapgen_scenario(&scenario)
            .expect("checked mapgen oracle scenario should validate");

        let mut incomplete = checked_mapgen_scenario();
        incomplete.expected_observation.matching.pop();
        assert!(validate_mapgen_scenario(&incomplete).is_err());

        let mut bad_palette = checked_mapgen_scenario();
        bad_palette.expected_observation.palette.setup_completed = false;
        assert!(validate_mapgen_scenario(&bad_palette).is_err());

        let mut bad_template = checked_mapgen_scenario();
        bad_template
            .expected_observation
            .static_template
            .generated_marker_furniture_id = String::from("f_null");
        assert!(validate_mapgen_scenario(&bad_template).is_err());

        let mut bad_trace = checked_mapgen_scenario();
        bad_trace
            .expected_observation
            .static_template
            .generated_rows[0]
            .replace_range(0..1, "X");
        assert!(validate_mapgen_scenario(&bad_trace).is_err());

        let mut bad_start = checked_mapgen_scenario();
        bad_start
            .expected_observation
            .start_location
            .selected_candidate_id = String::from("shelter_north");
        assert!(validate_mapgen_scenario(&bad_start).is_err());
    }

    #[test]
    fn comparison_is_exact() {
        let scenario = checked_scenario();
        compare(&scenario, &scenario.expected_observation)
            .expect("identical observation should compare");

        let mut changed = checked_scenario().expected_observation;
        changed.cases[2].reason = String::from("changed");
        assert!(compare(&scenario, &changed).is_err());

        let item_group = checked_item_group_scenario();
        compare_item_group(&item_group, &item_group.expected_observation)
            .expect("identical item-group observation should compare");
        let corpse_expected = item_group
            .expected_observation
            .everyday_corpse
            .exact_traces
            .iter()
            .map(ItemGroupCorpseTraceV1::direct_projection)
            .collect::<Vec<_>>();
        let corpse_actual = rust_item_group_static_corpse_observation(
            &item_group.expected_observation.everyday_corpse.exact_traces,
        )
        .expect("Rust corpse projection should execute the production transition");
        compare_direct_observation("static corpse", &corpse_expected, &corpse_actual)
            .expect("representative corpse traces should compare exactly");
        let mut changed_corpse = corpse_actual;
        changed_corpse[0].content_raw_damage[0] = 1;
        assert!(
            compare_direct_observation("static corpse", &corpse_expected, &changed_corpse).is_err()
        );

        let mapgen = checked_mapgen_scenario();
        compare_mapgen(&mapgen, &mapgen.expected_observation)
            .expect("identical mapgen observation should compare");

        let direct = direct_mapgen_projection(&mapgen.expected_observation);
        compare_direct_observation("mapgen", &direct, &direct)
            .expect("identical direct observations should compare");
        let mut changed = direct_mapgen_projection(&mapgen.expected_observation);
        changed.linear[1].marker_x = 18;
        assert!(compare_direct_observation("mapgen", &direct, &changed).is_err());
        let mut changed = direct_mapgen_projection(&mapgen.expected_observation);
        changed.static_template.generated_rows[5].replace_range(2..3, ".");
        assert!(compare_direct_observation("mapgen", &direct, &changed).is_err());
        let mut changed = direct_mapgen_projection(&mapgen.expected_observation);
        changed.start_location.chosen_target_omt = String::from("shelter");
        assert!(compare_direct_observation("mapgen", &direct, &changed).is_err());
    }
}
