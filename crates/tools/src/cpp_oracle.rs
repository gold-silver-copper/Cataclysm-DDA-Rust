use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cdda_protocol::BASELINE_COMMIT;
use serde::{Deserialize, Serialize};

const ORACLE_FORMAT_VERSION: u16 = 1;
const CACHE_FORMAT_VERSION: u16 = 1;
const UPSTREAM_TREE: &str = "210f31db2e8b2f0caed1809f1a66781859f9d129";
const KERNEL: &str = "item_pocket_max_length_v1";
const ITEM_GROUP_KERNEL: &str = "item_group_generation_v1";
const MAX_JSON_BYTES: u64 = 1024 * 1024;
const MAX_CASES: usize = 8;
const DEFAULT_SCENARIO: &str = "docs/oracles/item-pocket-max-length-v1.json";
const ADAPTER_SOURCE: &str = include_str!("../../../tools/cpp-oracle/item_pocket_oracle_test.cpp");
const ITEM_GROUP_ADAPTER_SOURCE: &str =
    include_str!("../../../tools/cpp-oracle/item_group_oracle_test.cpp");
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
    nested: ItemGroupNestedObservationV1,
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
struct ItemGroupNestedObservationV1 {
    rolls_consumed: u16,
    expected_trace: Vec<String>,
    actual_trace: Vec<String>,
    downstream_draw_matches: bool,
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
            println!(
                "{}",
                serde_json::to_string_pretty(&observation)
                    .map_err(|error| format!("could not encode oracle observation: {error}"))?
            );
            eprintln!(
                "C++ oracle verified bounded item-group generation against pinned {}",
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
    let nested_trace = ["child_conditional", "child_always", "root_last"];
    if observation.nested.rolls_consumed != 4
        || !observation.nested.downstream_draw_matches
        || observation.nested.expected_trace != nested_trace
        || observation.nested.actual_trace != nested_trace
    {
        return Err("item-group nested observation does not preserve the shared RNG stream".into());
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
    }
}
