use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cdda_content::{
    ContentManifest, ModCatalog, OvermapTerrainMatchType, OvermapTerrainRegistry,
    StartLocationRegistry,
};
use cdda_protocol::{
    BASELINE_COMMIT, ChunkCoord, CraftItemPrototypeV1, FurnitureTileSnapshot,
    IntegralMagazinePocketPrototypeV1, ItemContainmentProfileV1, ItemDescriptionExpansionV1,
    ItemDescriptionSnippetCategoryV1, ItemDescriptionSnippetChoiceV1, ItemGroupDefinitionV1,
    ItemGroupEntryV1, ItemGroupGraphV1, ItemGroupItemPrototypeV1, ItemGroupKindV1, ItemGroupNodeV1,
    ItemGroupTargetV1, ItemGroupToolChargeStorageV1, MagazineWellPrototypeV1, TerrainTileSnapshot,
    WORLDGEN_CELLS_PER_OMT, WORLDGEN_OMT_SIZE, WORLDGEN_OVERMAP_HEIGHT, WORLDGEN_OVERMAP_WIDTH,
    WorldPosition, WorldSnapshotV1, WorldgenCatalogV1, WorldgenCellV1, WorldgenFurnitureTargetV1,
    WorldgenItemGroupPlacementV1, WorldgenOmtGeneratorV1, WorldgenOmtIdentityV1,
    WorldgenOmtMatchTypeV1, WorldgenOvermapLayerV1, WorldgenOvermapLayoutV1, WorldgenOvermapRunV1,
    WorldgenTemplateV1, WorldgenTerrainTargetV1, WorldgenWeightedFurnitureTargetV1,
    WorldgenWeightedTerrainTargetV1, worldgen_omt_matches,
};
use cdda_sim::{ReservedIdBlock, WorldState};
use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};

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
    repeated_tool_charges: ItemGroupRepeatedToolChargeObservationV1,
    modifier_rng_phase: ItemGroupModifierRngPhaseObservationV1,
    constructor_variants: Vec<ItemGroupConstructorVariantTraceV1>,
    description_expansion: ItemGroupDescriptionExpansionObservationV1,
    nested: ItemGroupNestedObservationV1,
    modifiers: ItemGroupModifierObservationV1,
    modifier_container_capacity: ItemGroupModifierContainerCapacityObservationV1,
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
    content_types: Vec<String>,
    content_raw_damage: Vec<i32>,
    content_damage_levels: Vec<i32>,
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

#[derive(Debug, Eq, PartialEq, Serialize)]
struct MapgenDirectObservationV1 {
    matching: Vec<MapgenMatchObservationV1>,
    rotatable: Vec<MapgenRotationObservationV1>,
    linear: Vec<MapgenRotationObservationV1>,
    static_template: MapgenStaticTemplateObservationV1,
    start_location: MapgenStartLocationObservationV1,
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
                "item-group repeated detachable tool charges",
                &observation.repeated_tool_charges.direct_projection(),
                &rust_repeated_item_group_tool_charge_observation()?,
            )?;
            compare_direct_observation(
                "item description snippet expansion",
                &observation.description_expansion.direct_projection(),
                &rust_item_group_description_expansion_observation()?,
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
        modifier_side_effects_supported: true,
        charges: Some(cdda_protocol::InclusiveI32RangeV1 {
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
        modifier_container_capacity_applies: false,
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
                            modifier_charges: Some(cdda_protocol::InclusiveI32RangeV1 {
                                minimum: replacement_requested,
                                maximum: replacement_requested,
                            }),
                            contents: Vec::new(),
                            seal_contents: false,
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
            terrain: vec![WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(0),
                weight: 1,
            }],
            furniture: vec![WorldgenWeightedFurnitureTargetV1 {
                target: WorldgenFurnitureTargetV1::None,
                weight: 1,
            }],
            item_group: None,
        };
        WORLDGEN_CELLS_PER_OMT
    ];
    cells[0].item_group = Some(WorldgenItemGroupPlacementV1 {
        group_id: placement_group_id,
        chance: 100,
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
        start_location: None,
        terrain_prototypes: vec![terrain],
        furniture_prototypes: Vec::new(),
        regional_terrain: Vec::new(),
        regional_furniture: Vec::new(),
        omt_generators: vec![WorldgenOmtGeneratorV1 {
            omt_id: identity.generator_id,
            templates: vec![WorldgenTemplateV1 { weight: 1, cells }],
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
            terrain: vec![WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(0),
                weight: 1,
            }],
            furniture: vec![WorldgenWeightedFurnitureTargetV1 {
                target: WorldgenFurnitureTargetV1::None,
                weight: 1,
            }],
            item_group: None,
        };
        WORLDGEN_CELLS_PER_OMT
    ];
    let source_marker = 5 * WORLDGEN_OMT_SIZE + 2;
    cells[source_marker] = WorldgenCellV1 {
        terrain: vec![WorldgenWeightedTerrainTargetV1 {
            target: WorldgenTerrainTargetV1::Prototype(1),
            weight: 1,
        }],
        furniture: vec![WorldgenWeightedFurnitureTargetV1 {
            target: WorldgenFurnitureTargetV1::Prototype(0),
            weight: 1,
        }],
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
        regional_terrain: Vec::new(),
        regional_furniture: Vec::new(),
        omt_generators: vec![WorldgenOmtGeneratorV1 {
            omt_id: identity.generator_id.clone(),
            templates: vec![WorldgenTemplateV1 { weight: 1, cells }],
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
