use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cdda_protocol::BASELINE_COMMIT;
use serde::{Deserialize, Serialize};

const ORACLE_FORMAT_VERSION: u16 = 1;
const UPSTREAM_TREE: &str = "210f31db2e8b2f0caed1809f1a66781859f9d129";
const KERNEL: &str = "item_pocket_max_length_v1";
const MAX_JSON_BYTES: u64 = 1024 * 1024;
const MAX_CASES: usize = 8;
const DEFAULT_SCENARIO: &str = "docs/oracles/item-pocket-max-length-v1.json";
const ADAPTER_SOURCE: &str = include_str!("../../../tools/cpp-oracle/item_pocket_oracle_test.cpp");
const ADAPTER_MAKEFILE: &str = include_str!("../../../tools/cpp-oracle/oracle.mk");

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
    let scenario_path = arguments
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join(DEFAULT_SCENARIO));
    let upstream = arguments
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_upstream(workspace));
    let scenario = load_scenario(&scenario_path)?;
    validate_upstream(&upstream)?;

    let binary = prepare_binary(workspace, &upstream)?;
    let observation = run_binary(workspace, &binary)?;
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
    let export_stamp = root.join(".rust-cpp-oracle-export");
    let adapter_stamp = root.join(".rust-cpp-oracle-adapter");
    let export_identity = format!("{BASELINE_COMMIT}\n{UPSTREAM_TREE}");
    let adapter_hash = blake3::hash(
        [ADAPTER_SOURCE.as_bytes(), ADAPTER_MAKEFILE.as_bytes()]
            .concat()
            .as_slice(),
    )
    .to_hex()
    .to_string();
    let reusable_export =
        fs::read_to_string(&export_stamp).is_ok_and(|contents| contents.trim() == export_identity);
    if !reusable_export {
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(&root)?;
        export_upstream(upstream, &root)?;
        let mut stamp_file = fs::File::create(&export_stamp)?;
        writeln!(stamp_file, "{export_identity}")?;
        stamp_file.sync_all()?;
    }
    let adapter_current =
        fs::read_to_string(&adapter_stamp).is_ok_and(|contents| contents.trim() == adapter_hash);
    if binary.is_file() && adapter_current {
        return Ok(binary);
    }
    fs::write(
        root.join("tests/rust_cpp_oracle_item_pocket_test.cpp"),
        ADAPTER_SOURCE,
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
    let mut stamp_file = fs::File::create(adapter_stamp)?;
    writeln!(stamp_file, "{adapter_hash}")?;
    stamp_file.sync_all()?;
    Ok(binary)
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
    let mut archive = Command::new("git")
        .arg("-C")
        .arg(upstream)
        .args(["archive", "--format=tar", BASELINE_COMMIT])
        .stdout(Stdio::piped())
        .spawn()?;
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
    binary: &Path,
) -> Result<OracleObservationV1, Box<dyn std::error::Error>> {
    let process = std::process::id();
    let output_path = workspace.join(format!("target/cpp-oracle/observation-{process}.json"));
    let user_dir = workspace.join(format!("target/cpp-oracle/test-user-{process}"));
    if output_path.exists() {
        fs::remove_file(&output_path)?;
    }
    if user_dir.exists() {
        fs::remove_dir_all(&user_dir)?;
    }
    let upstream_root = binary
        .parent()
        .and_then(Path::parent)
        .ok_or("C++ oracle binary is outside its exported upstream root")?;
    let status = Command::new(binary)
        .arg("rust_cpp_oracle_item_pocket_max_length")
        .args(["--rng-seed", "1", "--order", "lex", "--drop-world"])
        .arg("--user-dir")
        .arg(&user_dir)
        .env("CDDA_RUST_CPP_ORACLE_OUTPUT", &output_path)
        .env("LANGUAGE", "en")
        .env("LC_ALL", "C")
        .current_dir(upstream_root)
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

fn read_bounded(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.len() > MAX_JSON_BYTES {
        return Err(format!("JSON input {} is absent or exceeds 1 MiB", path.display()).into());
    }
    Ok(fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked_scenario() -> OracleScenarioV1 {
        serde_json::from_str(include_str!(
            "../../../docs/oracles/item-pocket-max-length-v1.json"
        ))
        .expect("checked oracle scenario should decode")
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
    fn comparison_is_exact() {
        let scenario = checked_scenario();
        compare(&scenario, &scenario.expected_observation)
            .expect("identical observation should compare");

        let mut changed = checked_scenario().expected_observation;
        changed.cases[2].reason = String::from("changed");
        assert!(compare(&scenario, &changed).is_err());
    }
}
