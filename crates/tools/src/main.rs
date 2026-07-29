use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use cdda_content::{
    ContentManifest, ModCatalog, PINNED_UPSTREAM_COMMIT, ProvenanceEntry, REQUIRED_CONTENT_ROOTS,
    SchemaInventory,
};
use cdda_persistence::{
    ENROLLMENT_LIFETIME_SECONDS, REPLAY_FORMAT_VERSION, ReplayBundleV1, SCHEMA_VERSION, WorldStore,
};
use cdda_protocol::{
    AccountId, AccountRole, BASELINE_COMMIT, ContentIdentity, EndpointIdentity, PROTOCOL_VERSION,
};
use iroh::EndpointId;
use serde::Deserialize;

mod cpp_oracle;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    resolve: Option<Resolve>,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    dependencies: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParityLedger {
    format_version: u16,
    baseline_commit: String,
    protocol_version: u16,
    persistence_schema_version: i64,
    replay_format_version: u16,
    active_milestone: String,
    completion_gate: Vec<String>,
    completed_family_evidence: Vec<CompletedFamilyEvidence>,
    milestones: Vec<ParityMilestone>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletedFamilyEvidence {
    milestone_id: String,
    pinned_characterization: Vec<String>,
    generalized_rust_engine: Vec<String>,
    direct_rust_cpp_comparison: Vec<String>,
    four_mode_conformance: Vec<String>,
    runtime_content_admission: Vec<String>,
    authoritative_client_path: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParityMilestone {
    id: String,
    title: String,
    state: ParityState,
    priority: u16,
    depends_on: Vec<String>,
    upstream_sources: Vec<String>,
    rust_paths: Vec<String>,
    scenario_families: Vec<String>,
    differential_oracle: DifferentialState,
    multiplayer_adaptation: String,
    unlocks: Vec<String>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RuntimeProgress {
    format_version: u16,
    baseline_commit: String,
    green_parent_commit: String,
    verified_commit: Option<String>,
    protocol_version: u16,
    persistence_schema_version: i64,
    replay_format_version: u16,
    evidence_weights: RuntimeEvidenceWeights,
    parser_inventory: Vec<ParserInventoryCategory>,
    ordinary_gameplay_targets: Vec<RuntimeTargetScope>,
    categories: Vec<RuntimeProgressCategory>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RuntimeEvidenceWeights {
    generated: u64,
    authoritative_interaction: u64,
    persisted: u64,
    client_accessible: u64,
    four_mode_conformance: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ParserInventoryCategory {
    id: String,
    inventoried_definitions: u64,
    evidence_paths: Vec<String>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RuntimeTargetScope {
    id: String,
    parser_targets: Vec<RuntimeParserTarget>,
    target_definitions: u64,
    maximum_weighted_points: u64,
    earned_weighted_points: u64,
    evidence_paths: Vec<String>,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RuntimeParserTarget {
    id: String,
    target_definitions: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RuntimeProgressCategory {
    id: String,
    target_scope: String,
    generated_definitions: u64,
    authoritatively_interacted_definitions: u64,
    persisted_definitions: u64,
    client_accessible_definitions: u64,
    four_mode_definitions: u64,
    weighted_points: u64,
    ordinary_gameplay_loops: Vec<String>,
    evidence_paths: Vec<String>,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ParityState {
    Complete,
    OraclePending,
    InProgress,
    Planned,
    Pending,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DifferentialState {
    Verified,
    Active,
    Planned,
    NotApplicable,
}

fn runtime_target_inventory(
    workspace: &Path,
    parsers: &[ParserInventoryCategory],
) -> Result<BTreeMap<(String, String), u64>, Box<dyn std::error::Error>> {
    let manifest: ContentManifest = serde_json::from_slice(&fs::read(
        workspace.join("vendor/cdda-content-manifest.json"),
    )?)?;
    if manifest.upstream_commit != BASELINE_COMMIT {
        return Err("runtime target inventory manifest baseline mismatch".into());
    }
    let content_root = workspace.join("vendor");
    let mods = ModCatalog::load(&manifest, &content_root)?;
    let core_ids = mods
        .iter()
        .filter(|(_, information)| information.core && !information.obsolete)
        .map(|(id, _)| id.to_owned())
        .collect::<Vec<_>>();
    let mut selectable_mod_files = BTreeSet::new();
    for (id, information) in mods.iter().filter(|(_, information)| !information.obsolete) {
        let mut requests = vec![vec![id.to_owned()]];
        if !information.core {
            requests.extend(
                core_ids
                    .iter()
                    .map(|core| vec![core.clone(), id.to_owned()]),
            );
        }
        let selected = requests.into_iter().find_map(|request| {
            mods.resolve_new_world(&request)
                .ok()
                .and_then(|enabled| mods.selected_json_files(&manifest, &enabled).ok())
        });
        let Some(selected) = selected else {
            continue;
        };
        selectable_mod_files.extend(
            selected
                .into_iter()
                .filter(|file| file.upstream_path.starts_with("data/mods/"))
                .map(|file| file.upstream_path),
        );
    }
    let parser_ids = parsers
        .iter()
        .map(|parser| parser.id.as_str())
        .collect::<BTreeSet<_>>();
    let scopes = [
        "bundled-mod-ordinary-gameplay",
        "core-dda-ordinary-gameplay",
    ];
    let mut counts = scopes
        .iter()
        .flat_map(|scope| {
            parser_ids
                .iter()
                .map(move |parser| ((*scope).to_owned(), (*parser).to_owned()))
        })
        .map(|key| (key, 0_u64))
        .collect::<BTreeMap<_, _>>();
    for entry in manifest
        .entries
        .iter()
        .filter(|entry| entry.destination.ends_with(".json"))
    {
        if entry.upstream_path.starts_with("data/mods/")
            && !selectable_mod_files.contains(&entry.upstream_path)
        {
            continue;
        }
        let bytes = fs::read(workspace.join("vendor").join(&entry.destination))?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        let objects = match &value {
            serde_json::Value::Array(values) => values.as_slice(),
            serde_json::Value::Object(_) => std::slice::from_ref(&value),
            _ => &[],
        };
        let scope = if entry.upstream_path.starts_with("data/mods/") {
            "bundled-mod-ordinary-gameplay"
        } else {
            "core-dda-ordinary-gameplay"
        };
        for object in objects {
            let Some(kind) = object.get("type").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if parser_ids.contains(kind) {
                let count = counts
                    .get_mut(&(scope.to_owned(), kind.to_owned()))
                    .ok_or("runtime target inventory lost a scoped parser")?;
                *count = count
                    .checked_add(1)
                    .ok_or("runtime target inventory count overflow")?;
            }
        }
    }
    Ok(counts)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("verify-dependency-boundaries") => verify_dependency_boundaries(),
        Some("parity-ledger-check") => parity_ledger_check(),
        Some("runtime-progress-check") => runtime_progress_check(),
        Some("cpp-oracle-check") => cpp_oracle::check(std::env::args().skip(2).collect()),
        Some("astronomy-table-check") => astronomy_table_check(),
        Some("account-create") => create_account(std::env::args().skip(2).collect()),
        Some("account-recover") => recover_account(std::env::args().skip(2).collect()),
        Some("content-import") => import_content(std::env::args().skip(2).collect()),
        Some("content-validate") => validate_content(std::env::args().skip(2).collect()),
        Some("content-inventory") => content_inventory(std::env::args().skip(2).collect(), false),
        Some("content-inventory-check") => {
            content_inventory(std::env::args().skip(2).collect(), true)
        }
        Some("replay-export") => replay_export(std::env::args().skip(2).collect()),
        Some("replay-verify") => replay_verify(std::env::args().skip(2).collect()),
        Some(other) => Err(format!("unknown xtask command: {other}").into()),
        None => Err(
            "usage: cargo xtask <verify-dependency-boundaries|parity-ledger-check|runtime-progress-check|cpp-oracle-check ...|astronomy-table-check|account-create ...|account-recover ...|content-import ...|content-validate ...|content-inventory ...|content-inventory-check ...|replay-export ...|replay-verify ...>"
                .into(),
        ),
    }
}

fn runtime_progress_check() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("tools crate is not nested beneath the workspace")?;
    let progress: RuntimeProgress =
        serde_json::from_slice(&fs::read(workspace.join("docs/runtime-progress.json"))?)?;
    if progress.format_version != 2
        || progress.baseline_commit != BASELINE_COMMIT
        || progress.protocol_version != PROTOCOL_VERSION
        || progress.persistence_schema_version != SCHEMA_VERSION
        || progress.replay_format_version != REPLAY_FORMAT_VERSION
    {
        return Err("runtime progress version gates do not match the runtime".into());
    }
    let validate_commit = |commit: &str, label: &str| -> Result<(), Box<dyn std::error::Error>> {
        if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("runtime progress {label} is not a full Git object ID").into());
        }
        let available = Command::new("git")
            .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
            .current_dir(workspace)
            .status()?;
        let ancestor = Command::new("git")
            .args(["merge-base", "--is-ancestor", commit, "HEAD"])
            .current_dir(workspace)
            .status()?;
        if !available.success() || !ancestor.success() {
            return Err(
                format!("runtime progress {label} is unavailable or not an ancestor").into(),
            );
        }
        Ok(())
    };
    validate_commit(&progress.green_parent_commit, "green parent commit")?;
    if let Some(verified_commit) = &progress.verified_commit {
        validate_commit(verified_commit, "verified commit")?;
    }
    let weights = &progress.evidence_weights;
    if !(weights.generated > 0
        && weights.generated < weights.authoritative_interaction
        && weights.authoritative_interaction < weights.persisted
        && weights.persisted < weights.client_accessible
        && weights.client_accessible < weights.four_mode_conformance)
    {
        return Err("runtime progress evidence weights must be strictly increasing".into());
    }
    if progress.parser_inventory.is_empty() || progress.categories.is_empty() {
        return Err("runtime progress must contain categories".into());
    }
    let schema_inventory: serde_json::Value = serde_json::from_slice(&fs::read(
        workspace.join("docs/content-schema-inventory.json"),
    )?)?;
    let mut previous = None;
    for parser in &progress.parser_inventory {
        let actual = schema_inventory
            .pointer(&format!("/definitions/{}/objects", parser.id))
            .and_then(serde_json::Value::as_u64);
        if parser.id.is_empty()
            || previous.is_some_and(|previous| previous >= parser.id.as_str())
            || parser.inventoried_definitions == 0
            || actual != Some(parser.inventoried_definitions)
            || parser.evidence_paths.is_empty()
            || parser.evidence_paths.iter().any(|path| {
                path.starts_with('/') || path.contains("..") || !workspace.join(path).is_file()
            })
        {
            return Err(
                format!("runtime progress parser category {} is invalid", parser.id).into(),
            );
        }
        previous = Some(parser.id.as_str());
    }
    let maximum_points_per_definition = weights
        .generated
        .checked_add(weights.authoritative_interaction)
        .and_then(|points| points.checked_add(weights.persisted))
        .and_then(|points| points.checked_add(weights.client_accessible))
        .and_then(|points| points.checked_add(weights.four_mode_conformance))
        .ok_or("runtime progress maximum evidence weight overflow")?;
    let scoped_inventory = runtime_target_inventory(workspace, &progress.parser_inventory)?;
    let expected_scopes = [
        "bundled-mod-ordinary-gameplay",
        "core-dda-ordinary-gameplay",
    ];
    if progress.ordinary_gameplay_targets.len() != expected_scopes.len() {
        return Err("runtime progress must separate core DDA and bundled-mod targets".into());
    }
    let mut target_maximums = BTreeMap::new();
    let mut target_recorded_earned = BTreeMap::new();
    for (target, expected_scope) in progress
        .ordinary_gameplay_targets
        .iter()
        .zip(expected_scopes)
    {
        let expected_parsers = progress
            .parser_inventory
            .iter()
            .map(|parser| parser.id.as_str())
            .collect::<Vec<_>>();
        let actual_parsers = target
            .parser_targets
            .iter()
            .map(|parser| parser.id.as_str())
            .collect::<Vec<_>>();
        let target_definitions = target
            .parser_targets
            .iter()
            .try_fold(0_u64, |total, parser| {
                let expected = scoped_inventory
                    .get(&(target.id.clone(), parser.id.clone()))
                    .copied();
                (expected == Some(parser.target_definitions))
                    .then(|| total.checked_add(parser.target_definitions))
                    .flatten()
            });
        let maximum = target_definitions
            .and_then(|definitions| definitions.checked_mul(maximum_points_per_definition));
        if target.id != expected_scope
            || actual_parsers != expected_parsers
            || target_definitions != Some(target.target_definitions)
            || maximum != Some(target.maximum_weighted_points)
            || target.earned_weighted_points > target.maximum_weighted_points
            || target.evidence_paths.is_empty()
            || target.evidence_paths.iter().any(|path| {
                path.starts_with('/') || path.contains("..") || !workspace.join(path).is_file()
            })
        {
            return Err(format!(
                "runtime progress target scope {} is invalid: derived parser targets {:?}, derived definitions {:?}, recorded definitions {}, derived maximum {:?}, recorded maximum {}",
                target.id,
                target
                    .parser_targets
                    .iter()
                    .map(|parser| (
                        parser.id.as_str(),
                        scoped_inventory
                            .get(&(target.id.clone(), parser.id.clone()))
                            .copied(),
                    ))
                    .collect::<Vec<_>>(),
                target_definitions,
                target.target_definitions,
                maximum,
                target.maximum_weighted_points,
            )
            .into());
        }
        target_maximums.insert(target.id.as_str(), target.maximum_weighted_points);
        target_recorded_earned.insert(target.id.as_str(), target.earned_weighted_points);
    }
    let mut previous = None;
    let mut total_definitions = 0_u64;
    let mut total_points = 0_u64;
    let mut target_actual_earned = expected_scopes
        .into_iter()
        .map(|scope| (scope, 0_u64))
        .collect::<BTreeMap<_, _>>();
    for category in &progress.categories {
        if category.id.is_empty()
            || previous.is_some_and(|previous| previous >= category.id.as_str())
            || !target_maximums.contains_key(category.target_scope.as_str())
            || category.generated_definitions == 0
            || category.authoritatively_interacted_definitions > category.generated_definitions
            || category.persisted_definitions > category.generated_definitions
            || category.client_accessible_definitions
                > category.authoritatively_interacted_definitions
            || category.four_mode_definitions > category.authoritatively_interacted_definitions
            || category.four_mode_definitions > category.persisted_definitions
            || category.ordinary_gameplay_loops.is_empty()
            || category.evidence_paths.is_empty()
        {
            return Err(format!("runtime progress category {} is invalid", category.id).into());
        }
        if category.evidence_paths.iter().any(|path| {
            path.starts_with('/') || path.contains("..") || !workspace.join(path).is_file()
        }) {
            return Err(format!(
                "runtime progress category {} has invalid evidence",
                category.id
            )
            .into());
        }
        let expected = category
            .generated_definitions
            .checked_mul(weights.generated)
            .and_then(|value| {
                category
                    .authoritatively_interacted_definitions
                    .checked_mul(weights.authoritative_interaction)
                    .and_then(|points| value.checked_add(points))
            })
            .and_then(|value| {
                category
                    .persisted_definitions
                    .checked_mul(weights.persisted)
                    .and_then(|points| value.checked_add(points))
            })
            .and_then(|value| {
                category
                    .client_accessible_definitions
                    .checked_mul(weights.client_accessible)
                    .and_then(|points| value.checked_add(points))
            })
            .and_then(|value| {
                category
                    .four_mode_definitions
                    .checked_mul(weights.four_mode_conformance)
                    .and_then(|points| value.checked_add(points))
            })
            .ok_or("runtime progress weighted points overflow")?;
        if category.weighted_points != expected {
            return Err(
                format!("runtime progress category {} has stale points", category.id).into(),
            );
        }
        total_definitions = total_definitions
            .checked_add(category.generated_definitions)
            .ok_or("runtime progress definition total overflow")?;
        total_points = total_points
            .checked_add(category.weighted_points)
            .ok_or("runtime progress point total overflow")?;
        let earned = target_actual_earned
            .get_mut(category.target_scope.as_str())
            .ok_or("runtime progress category names an unknown target scope")?;
        *earned = earned
            .checked_add(category.weighted_points)
            .ok_or("runtime progress target earned points overflow")?;
        previous = Some(category.id.as_str());
    }
    for (scope, actual) in &target_actual_earned {
        if target_recorded_earned.get(scope).copied() != Some(*actual) {
            return Err(
                format!("runtime progress target scope {scope} has stale earned points").into(),
            );
        }
    }
    if let Some(verified_commit) = &progress.verified_commit {
        let git_show = |path: &str| -> Result<String, Box<dyn std::error::Error>> {
            let output = Command::new("git")
                .args(["show", &format!("{verified_commit}:{path}")])
                .current_dir(workspace)
                .output()?;
            if !output.status.success() {
                return Err(format!("verified commit is missing {path}").into());
            }
            Ok(String::from_utf8(output.stdout)?)
        };
        let recorded_progress: RuntimeProgress =
            serde_json::from_str(&git_show("docs/runtime-progress.json")?)?;
        let mut normalized_progress = progress.clone();
        normalized_progress.verified_commit = None;
        if recorded_progress.verified_commit.is_some() || recorded_progress != normalized_progress {
            return Err(
                "runtime progress data differs from the unbound artifact at the verified commit"
                    .into(),
            );
        }
        let protocol_source = git_show("crates/protocol/src/lib.rs")?;
        let persistence_source = git_show("crates/persistence/src/lib.rs")?;
        if !protocol_source.contains(&format!(
            "pub const PROTOCOL_VERSION: u16 = {};",
            progress.protocol_version
        )) || !persistence_source.contains(&format!(
            "pub const SCHEMA_VERSION: i64 = {};",
            progress.persistence_schema_version
        )) || !persistence_source.contains(&format!(
            "pub const REPLAY_FORMAT_VERSION: u16 = {};",
            progress.replay_format_version
        )) {
            return Err("verified commit runtime versions do not match progress data".into());
        }

        let mut evidence_paths = BTreeSet::from([
            "crates/protocol/src/lib.rs",
            "crates/persistence/src/lib.rs",
            "crates/tools/src/main.rs",
            "docs/content-schema-inventory.json",
        ]);
        for path in progress
            .parser_inventory
            .iter()
            .flat_map(|category| &category.evidence_paths)
            .chain(
                progress
                    .ordinary_gameplay_targets
                    .iter()
                    .flat_map(|target| &target.evidence_paths),
            )
            .chain(
                progress
                    .categories
                    .iter()
                    .flat_map(|category| &category.evidence_paths),
            )
        {
            evidence_paths.insert(path.as_str());
        }
        for path in &evidence_paths {
            let tracked = Command::new("git")
                .args(["cat-file", "-e", &format!("{verified_commit}:{path}")])
                .current_dir(workspace)
                .status()?;
            if !tracked.success() {
                return Err(format!(
                    "runtime progress evidence {path} is absent from the verified commit"
                )
                .into());
            }
        }
        let mut unchanged = Command::new("git");
        unchanged
            .args(["diff", "--quiet", verified_commit, "--"])
            .args(evidence_paths)
            .current_dir(workspace);
        if !unchanged.status()?.success() {
            return Err("runtime progress evidence differs from the verified commit".into());
        }
        let status = fs::read_to_string(workspace.join("IMPLEMENTATION_STATUS.md"))?;
        if !status.contains(&format!("Verified green commit: `{verified_commit}`")) {
            return Err("implementation status does not name the exact verified commit".into());
        }
        println!(
            "runtime progress verified at {verified_commit}: {total_definitions} generated definitions, {total_points} weighted evidence points"
        );
    } else {
        let target_summary = progress
            .ordinary_gameplay_targets
            .iter()
            .map(|target| {
                let percent = 100.0 * target.earned_weighted_points as f64
                    / target.maximum_weighted_points as f64;
                format!(
                    "{} {}/{} ({percent:.4}%)",
                    target.id, target.earned_weighted_points, target.maximum_weighted_points
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "runtime progress measured in the active worktree above green parent {}: {} generated definitions, {} weighted evidence points; {}; checkpoint binding pending",
            progress.green_parent_commit, total_definitions, total_points, target_summary
        );
    }
    Ok(())
}

fn parity_ledger_check() -> Result<(), Box<dyn std::error::Error>> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("tools crate is not nested beneath the workspace")?;
    let ledger_path = workspace.join("docs/parity-ledger.json");
    let ledger: ParityLedger = serde_json::from_slice(&fs::read(&ledger_path)?)?;
    validate_parity_ledger(&ledger, workspace)?;
    println!(
        "parity ledger verified: {} milestones, active {}",
        ledger.milestones.len(),
        ledger.active_milestone
    );
    Ok(())
}

fn validate_parity_ledger(
    ledger: &ParityLedger,
    workspace: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if ledger.format_version != 1 {
        return Err("unsupported parity ledger format version".into());
    }
    const COMPLETION_GATE: [&str; 6] = [
        "pinned C++ characterization with exact boundary traces where applicable",
        "generalized Rust engine",
        "direct Rust-to-C++ comparison",
        "direct snapshot SQLite and portable-replay conformance",
        "runtime content admission",
        "normal authoritative server/client path or explicit not-applicable rationale",
    ];
    if ledger.completion_gate != COMPLETION_GATE {
        return Err("parity ledger completion gate is incomplete or reordered".into());
    }
    if ledger.baseline_commit != BASELINE_COMMIT
        || ledger.protocol_version != PROTOCOL_VERSION
        || ledger.persistence_schema_version != SCHEMA_VERSION
        || ledger.replay_format_version != REPLAY_FORMAT_VERSION
    {
        return Err("parity ledger version gates do not match the runtime".into());
    }
    if ledger.milestones.is_empty() {
        return Err("parity ledger must contain milestones".into());
    }

    let mut by_id = BTreeMap::new();
    let mut priorities = BTreeSet::new();
    let upstream_root = workspace.join("../Cataclysm-DDA");
    let canonical_upstream_root = upstream_root
        .is_dir()
        .then(|| fs::canonicalize(&upstream_root))
        .transpose()?;
    if canonical_upstream_root.is_some() {
        let output = Command::new("git")
            .args(["-C", "../Cataclysm-DDA", "rev-parse", "HEAD"])
            .current_dir(workspace)
            .output()?;
        if !output.status.success()
            || String::from_utf8(output.stdout)?.trim() != ledger.baseline_commit
        {
            return Err("available upstream checkout is not at the ledger baseline".into());
        }
    }
    for milestone in &ledger.milestones {
        if milestone.id.is_empty()
            || milestone.title.is_empty()
            || milestone.multiplayer_adaptation.is_empty()
            || milestone.upstream_sources.is_empty()
            || milestone.rust_paths.is_empty()
            || milestone.unlocks.is_empty()
        {
            return Err(format!("milestone {} has an empty required field", milestone.id).into());
        }
        if by_id.insert(milestone.id.as_str(), milestone).is_some() {
            return Err(format!("duplicate milestone ID: {}", milestone.id).into());
        }
        if !priorities.insert(milestone.priority) {
            return Err(format!("duplicate milestone priority: {}", milestone.priority).into());
        }
        for path in &milestone.rust_paths {
            if path.starts_with('/') || path.contains("..") || !workspace.join(path).exists() {
                return Err(format!(
                    "milestone {} references missing or nonlocal Rust path {path}",
                    milestone.id
                )
                .into());
            }
        }
        if milestone.upstream_sources.iter().any(|source| {
            source
                .strip_prefix("../Cataclysm-DDA/")
                .is_none_or(|relative| {
                    relative.is_empty()
                        || Path::new(relative)
                            .components()
                            .any(|component| !matches!(component, std::path::Component::Normal(_)))
                })
        }) {
            return Err(format!(
                "milestone {} has an unpinned upstream source path",
                milestone.id
            )
            .into());
        }
        if let Some(root) = &canonical_upstream_root {
            for source in &milestone.upstream_sources {
                let canonical_source =
                    fs::canonicalize(workspace.join(source)).map_err(|error| {
                        format!(
                            "milestone {} references missing upstream source {source}: {error}",
                            milestone.id
                        )
                    })?;
                if !canonical_source.starts_with(root) || !canonical_source.is_file() {
                    return Err(format!(
                        "milestone {} upstream source escapes the pinned checkout: {source}",
                        milestone.id
                    )
                    .into());
                }
            }
        }
        if matches!(milestone.state, ParityState::Complete)
            && (milestone.scenario_families.is_empty()
                || !matches!(
                    milestone.differential_oracle,
                    DifferentialState::Verified | DifferentialState::NotApplicable
                ))
        {
            return Err(format!(
                "completed milestone {} lacks scenarios or oracle disposition",
                milestone.id
            )
            .into());
        }
    }

    let completed_ids = ledger
        .milestones
        .iter()
        .filter(|milestone| milestone.state == ParityState::Complete)
        .map(|milestone| milestone.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut evidence_ids = BTreeSet::new();
    for evidence in &ledger.completed_family_evidence {
        let evidence_paths = [
            &evidence.pinned_characterization,
            &evidence.generalized_rust_engine,
            &evidence.direct_rust_cpp_comparison,
            &evidence.four_mode_conformance,
            &evidence.runtime_content_admission,
            &evidence.authoritative_client_path,
        ];
        if evidence.milestone_id.is_empty()
            || !evidence_ids.insert(evidence.milestone_id.as_str())
            || evidence_paths.iter().any(|paths| paths.is_empty())
            || evidence_paths.into_iter().flatten().any(|path| {
                path.starts_with('/') || path.contains("..") || !workspace.join(path).is_file()
            })
        {
            return Err(format!(
                "completed milestone {} has invalid completion evidence",
                evidence.milestone_id
            )
            .into());
        }
    }
    if evidence_ids != completed_ids {
        return Err(
            "completed milestone evidence does not exactly match completed families".into(),
        );
    }

    let active = by_id
        .get(ledger.active_milestone.as_str())
        .ok_or("active milestone is absent from the ledger")?;
    if active.state != ParityState::InProgress {
        return Err("active milestone must be in_progress".into());
    }
    for milestone in &ledger.milestones {
        if milestone.state == ParityState::Complete
            && milestone.depends_on.iter().any(|dependency| {
                by_id
                    .get(dependency.as_str())
                    .is_none_or(|dependency| dependency.state != ParityState::Complete)
            })
        {
            return Err(format!(
                "completed milestone {} has an incomplete prerequisite",
                milestone.id
            )
            .into());
        }
    }

    let mut indegree = BTreeMap::<&str, usize>::new();
    let mut dependents = BTreeMap::<&str, Vec<&str>>::new();
    for milestone in &ledger.milestones {
        indegree.insert(milestone.id.as_str(), milestone.depends_on.len());
        let mut unique_dependencies = BTreeSet::new();
        for dependency in &milestone.depends_on {
            if dependency == &milestone.id
                || !unique_dependencies.insert(dependency.as_str())
                || !by_id.contains_key(dependency.as_str())
            {
                return Err(format!(
                    "milestone {} has an invalid dependency {dependency}",
                    milestone.id
                )
                .into());
            }
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(milestone.id.as_str());
        }
    }
    let mut ready = indegree
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<Vec<_>>();
    let mut visited = 0_usize;
    while let Some(id) = ready.pop() {
        visited += 1;
        for dependent in dependents.get(id).into_iter().flatten() {
            let count = indegree
                .get_mut(dependent)
                .ok_or("parity ledger dependency index is inconsistent")?;
            *count = count
                .checked_sub(1)
                .ok_or("parity ledger dependency count underflow")?;
            if *count == 0 {
                ready.push(dependent);
            }
        }
    }
    if visited != ledger.milestones.len() {
        return Err("parity milestone dependency graph contains a cycle".into());
    }
    Ok(())
}

fn astronomy_table_check() -> Result<(), Box<dyn std::error::Error>> {
    for day in 0_u16..364 {
        let generated = generated_solar_boundaries(day);
        let committed = cdda_protocol::solar_boundaries_seconds(day)
            .ok_or("committed astronomy table is incomplete")?;
        for (kind, (generated, committed)) in ["civil dawn", "sunrise", "sunset", "civil dusk"]
            .into_iter()
            .zip(generated.into_iter().zip(committed))
        {
            if generated.abs_diff(committed) > 1 {
                return Err(format!(
                    "astronomy table differs on day {day} {kind}: generated {generated}, committed {committed}"
                )
                .into());
            }
        }
    }
    println!("astronomy table verified for 364 pinned Boston days");
    Ok(())
}

fn generated_solar_boundaries(day: u16) -> [u32; 4] {
    const DEGREE: f64 = std::f64::consts::PI / 180.0;
    [
        generated_sun_at(day, -6.0 * DEGREE, false),
        generated_sun_at(day, -DEGREE, false),
        generated_sun_at(day, -DEGREE, true),
        generated_sun_at(day, -6.0 * DEGREE, true),
    ]
}

fn generated_sun_at(day: u16, altitude: f64, evening: bool) -> u32 {
    const SECONDS_PER_DAY: f64 = 86_400.0;
    const DEGREE: f64 = std::f64::consts::PI / 180.0;
    let midnight = f64::from(day) * SECONDS_PER_DAY;
    let noon = midnight + SECONDS_PER_DAY / 2.0;
    let mut initial = generated_solar_offset(altitude, noon, evening);
    if !evening {
        initial -= 2.0 * std::f64::consts::PI;
    }
    let approximation = noon + (initial / (15.0 * DEGREE) * 3_600.0).trunc();
    let mut correction = generated_solar_offset(altitude, approximation, evening);
    if correction > std::f64::consts::PI {
        correction -= 2.0 * std::f64::consts::PI;
    }
    u32::try_from(
        (approximation + (correction / (15.0 * DEGREE) * 3_600.0).trunc() - midnight) as i64,
    )
    .expect("pinned Boston solar boundary fits one day")
}

fn generated_solar_offset(altitude: f64, time: f64, evening: bool) -> f64 {
    const LATITUDE: f64 = 42.36 * std::f64::consts::PI / 180.0;
    let (right_ascension, declination) = generated_sun_ra_declination(time);
    let mut cosine_hour_angle = (altitude.sin() - LATITUDE.sin() * declination.sin())
        / (LATITUDE.cos() * declination.cos());
    cosine_hour_angle = cosine_hour_angle.clamp(-1.0, 1.0);
    let mut hour_angle = cosine_hour_angle.acos();
    if !evening {
        hour_angle = -hour_angle;
    }
    (hour_angle + right_ascension - generated_sidereal_time(time))
        .rem_euclid(2.0 * std::f64::consts::PI)
}

fn generated_sun_ra_declination(time: f64) -> (f64, f64) {
    const DEGREE: f64 = std::f64::consts::PI / 180.0;
    let days = (time - generated_timezone_seconds()) / 86_400.0;
    let mean_longitude = 2.0 * std::f64::consts::PI / 364.0 * days;
    let mean_anomaly = 77.0 * DEGREE + mean_longitude;
    let ecliptic_longitude = mean_longitude
        + 1.915 * DEGREE * mean_anomaly.sin()
        + 0.020 * DEGREE * (2.0 * mean_anomaly).sin();
    let obliquity = 23.439_279 * DEGREE;
    let x = ecliptic_longitude.cos();
    let y = ecliptic_longitude.sin() * obliquity.cos();
    let z = ecliptic_longitude.sin() * obliquity.sin();
    (y.atan2(x), z.asin())
}

fn generated_sidereal_time(time: f64) -> f64 {
    const LONGITUDE: f64 = -71.06 * std::f64::consts::PI / 180.0;
    let days = (time - generated_timezone_seconds()) / 86_400.0;
    std::f64::consts::PI
        + (2.0 * std::f64::consts::PI + 2.0 * std::f64::consts::PI / 364.0) * days
        + LONGITUDE
}

fn generated_timezone_seconds() -> f64 {
    const LONGITUDE: f64 = -71.06 * std::f64::consts::PI / 180.0;
    LONGITUDE / (15.0 * std::f64::consts::PI / 180.0) * 3_600.0
}

const MAX_REPLAY_DECODED: u64 = 256 * 1024 * 1024;

fn replay_content_identity(
    manifest_path: &Path,
) -> Result<ContentIdentity, Box<dyn std::error::Error>> {
    let root = manifest_path
        .parent()
        .ok_or("content manifest has no parent directory")?;
    let manifest = ContentManifest::load(manifest_path)?;
    manifest.verify_files(root)?;
    let catalog = ModCatalog::load(&manifest, root)?;
    Ok(ContentIdentity {
        baseline_commit: BASELINE_COMMIT.to_owned(),
        manifest_hash: manifest.canonical_hash()?,
        enabled_mods: catalog.recommended_new_world()?,
    })
}

fn replay_export(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if !(2..=3).contains(&arguments.len()) {
        return Err(
            "usage: cargo xtask replay-export <world.db> <output.cddar> [content-manifest.json]"
                .into(),
        );
    }
    let manifest_path = arguments
        .get(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(cdda_content::DEFAULT_MANIFEST_PATH));
    let content = replay_content_identity(&manifest_path)?;
    let store = WorldStore::open(&arguments[0])?;
    let bundle = store.export_replay(content.clone())?;
    let verified = bundle.verify(&content)?;
    let encoded = postcard::to_stdvec(&bundle)?;
    let compressed = zstd::stream::encode_all(encoded.as_slice(), 9)?;
    let output = Path::new(&arguments[1]);
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)?;
    file.write_all(&compressed)?;
    file.sync_all()?;
    println!(
        "exported replay through tick {} to {} ({} batches, {} bytes, BLAKE3 {})",
        verified.tick().0,
        output.display(),
        bundle.journal_batches.len(),
        compressed.len(),
        blake3::hash(&compressed)
    );
    Ok(())
}

fn replay_verify(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if !(1..=2).contains(&arguments.len()) {
        return Err(
            "usage: cargo xtask replay-verify <replay.cddar> [content-manifest.json]".into(),
        );
    }
    let manifest_path = arguments
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(cdda_content::DEFAULT_MANIFEST_PATH));
    let content = replay_content_identity(&manifest_path)?;
    let compressed = fs::read(&arguments[0])?;
    let mut decoder = zstd::stream::read::Decoder::new(compressed.as_slice())?;
    let mut decoded = Vec::new();
    decoder
        .by_ref()
        .take(MAX_REPLAY_DECODED + 1)
        .read_to_end(&mut decoded)?;
    if decoded.len() as u64 > MAX_REPLAY_DECODED {
        return Err("decoded replay exceeds 256 MiB".into());
    }
    let bundle: ReplayBundleV1 = postcard::from_bytes(&decoded)?;
    let world = bundle.verify(&content)?;
    println!(
        "verified replay through tick {} ({} batches, state BLAKE3 {})",
        world.tick().0,
        bundle.journal_batches.len(),
        blake3::Hash::from_bytes(world.canonical_hash()?)
    );
    Ok(())
}

fn content_inventory(
    arguments: Vec<String>,
    check: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() > 2 {
        return Err(
            "usage: cargo xtask content-inventory[-check] [content-manifest.json] [inventory.json]"
                .into(),
        );
    }
    let manifest_path = arguments
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(cdda_content::DEFAULT_MANIFEST_PATH));
    let inventory_path = arguments
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("docs/content-schema-inventory.json"));
    let root = manifest_path
        .parent()
        .ok_or("content manifest has no parent directory")?;
    let manifest = ContentManifest::load(&manifest_path)?;
    let inventory = SchemaInventory::build(&manifest, root)?;
    let mut encoded = serde_json::to_vec_pretty(&inventory)?;
    encoded.push(b'\n');
    if check {
        let existing = fs::read(&inventory_path)?;
        if existing != encoded {
            return Err(format!(
                "content schema inventory is stale: regenerate {}",
                inventory_path.display()
            )
            .into());
        }
        println!(
            "content schema inventory is current: {} JSON files, {} top-level objects, {} definition types",
            inventory.json_files,
            inventory.top_level_objects,
            inventory.definitions.len()
        );
    } else {
        if let Some(parent) = inventory_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&inventory_path, encoded)?;
        println!(
            "wrote {}: {} JSON files, {} top-level objects, {} definition types",
            inventory_path.display(),
            inventory.json_files,
            inventory.top_level_objects,
            inventory.definitions.len()
        );
    }
    Ok(())
}

fn import_content(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() != 1 {
        return Err("usage: cargo xtask content-import <pinned-cdda-checkout>".into());
    }
    let checkout = Path::new(&arguments[0]);
    verify_pinned_checkout(checkout)?;
    let paths = tracked_content_paths(checkout)?;
    if paths.is_empty() {
        return Err("pinned checkout contains no tracked content files".into());
    }

    let vendor_root = Path::new("vendor");
    let content_root = vendor_root.join("cdda");
    if content_root.exists() && !fs::symlink_metadata(&content_root)?.file_type().is_dir() {
        return Err("vendor/cdda exists but is not a directory".into());
    }
    let mut entries = Vec::with_capacity(paths.len());
    for path in paths {
        let source = checkout.join(&path);
        if !fs::symlink_metadata(&source)?.file_type().is_file() {
            return Err(format!("tracked content is not a regular file: {path}").into());
        }
        let bytes = fs::read(&source)?;
        let destination = content_root.join(&path);
        let parent = destination
            .parent()
            .ok_or("content destination has no parent directory")?;
        fs::create_dir_all(parent)?;
        fs::copy(&source, &destination)?;
        entries.push(ProvenanceEntry {
            upstream_path: path.clone(),
            destination: format!("cdda/{path}"),
            blake3: *blake3::hash(&bytes).as_bytes(),
            license: String::from("CC-BY-SA-3.0 (upstream LICENSE.txt)"),
        });
    }

    let manifest = ContentManifest::new(entries)?;
    fs::create_dir_all(vendor_root)?;
    let manifest_path = vendor_root.join("cdda-content-manifest.json");
    let mut encoded = serde_json::to_vec_pretty(&manifest)?;
    encoded.push(b'\n');
    fs::write(&manifest_path, encoded)?;
    validate_manifest_package(&manifest, vendor_root)?;
    println!(
        "imported {} pinned content files; manifest hash {}",
        manifest.entries.len(),
        blake3::Hash::from_bytes(manifest.canonical_hash()?)
    );
    Ok(())
}

fn validate_content(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() > 1 {
        return Err("usage: cargo xtask content-validate [content-manifest.json]".into());
    }
    let manifest_path = arguments
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(cdda_content::DEFAULT_MANIFEST_PATH));
    let root = manifest_path
        .parent()
        .ok_or("content manifest has no parent directory")?;
    let manifest = ContentManifest::load(&manifest_path)?;
    validate_manifest_package(&manifest, root)?;
    println!(
        "validated {} content files; manifest hash {}",
        manifest.entries.len(),
        blake3::Hash::from_bytes(manifest.canonical_hash()?)
    );
    Ok(())
}

fn verify_pinned_checkout(checkout: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let commit = git_stdout(checkout, &["rev-parse", "HEAD"])?;
    if commit.trim() != PINNED_UPSTREAM_COMMIT {
        return Err(format!(
            "CDDA checkout is at {}, expected {PINNED_UPSTREAM_COMMIT}",
            commit.trim()
        )
        .into());
    }
    let mut arguments = vec!["status", "--porcelain=v1", "--untracked-files=no", "--"];
    arguments.extend(REQUIRED_CONTENT_ROOTS.iter().copied());
    if !git_stdout(checkout, &arguments)?.is_empty() {
        return Err("pinned CDDA content paths contain tracked working-tree changes".into());
    }
    Ok(())
}

fn tracked_content_paths(checkout: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut arguments = vec!["ls-files", "-z", "--"];
    arguments.extend(REQUIRED_CONTENT_ROOTS.iter().copied());
    let output = git_output(checkout, &arguments)?;
    let mut paths = Vec::new();
    for path in output
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path)?.to_owned();
        if path.starts_with('/')
            || path.contains('\\')
            || path
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
        {
            return Err(format!("git returned an unsafe content path: {path}").into());
        }
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn git_stdout(checkout: &Path, arguments: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    Ok(String::from_utf8(git_output(checkout, arguments)?)?)
}

fn git_output(checkout: &Path, arguments: &[&str]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(arguments)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}

fn validate_manifest_package(
    manifest: &ContentManifest,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    manifest.verify_files(root)?;
    let expected: BTreeSet<_> = manifest
        .entries
        .iter()
        .map(|entry| PathBuf::from(&entry.destination))
        .collect();
    let mut actual = BTreeSet::new();
    collect_files(root, &root.join("cdda"), &mut actual)?;
    if actual != expected {
        let missing = expected.difference(&actual).next();
        let unexpected = actual.difference(&expected).next();
        return Err(format!(
            "vendored content set differs from manifest (missing: {}, unexpected: {})",
            missing.map_or_else(|| String::from("none"), |path| path.display().to_string()),
            unexpected.map_or_else(|| String::from("none"), |path| path.display().to_string())
        )
        .into());
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            files.insert(entry.path().strip_prefix(root)?.to_owned());
        } else {
            return Err(format!(
                "vendored content contains a symlink or special file: {}",
                entry.path().display()
            )
            .into());
        }
    }
    Ok(())
}

fn create_account(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if !(3..=4).contains(&arguments.len()) {
        return Err(
            "usage: cargo xtask account-create <world.db> <endpoint-id> <display-name> [player|moderator|administrator]"
                .into(),
        );
    }
    let endpoint = EndpointId::from_str(&arguments[1])?;
    let role = match arguments.get(3).map(String::as_str).unwrap_or("player") {
        "player" => AccountRole::Player,
        "moderator" => AccountRole::Moderator,
        "administrator" => AccountRole::Administrator,
        other => return Err(format!("unknown account role: {other}").into()),
    };
    let mut store = WorldStore::open(&arguments[0])?;
    match store.metadata_optional()? {
        Some(_) => {}
        None => {
            let mut namespace = rand::random::<u64>();
            if namespace == 0 {
                namespace = 1;
            }
            store.initialize_world(namespace, rand::random::<[u8; 32]>())?;
        }
    }
    store.require_runtime_inactive()?;
    let account_id = store.reserve_account_id()?;
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    let account = store.create_pending_account(
        account_id,
        &arguments[2],
        role,
        EndpointIdentity(*endpoint.as_bytes()),
        now,
    )?;
    println!(
        "created pending account {} ({})",
        account.display_name, account.id
    );
    println!(
        "enrollment expires at Unix time {}",
        now.checked_add(ENROLLMENT_LIFETIME_SECONDS)
            .ok_or("enrollment expiry overflow")?
    );
    Ok(())
}

fn recover_account(arguments: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if arguments.len() != 3 {
        return Err(
            "usage: cargo xtask account-recover <world.db> <account-id> <replacement-endpoint-id>"
                .into(),
        );
    }
    let account_id = parse_account_id(&arguments[1])?;
    let endpoint = EndpointId::from_str(&arguments[2])?;
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    let mut store = WorldStore::open(&arguments[0])?;
    store.require_runtime_inactive()?;
    let binding =
        store.recover_account_endpoint(account_id, EndpointIdentity(*endpoint.as_bytes()), now)?;
    println!("recovery-locked account {account_id}");
    println!("pending replacement endpoint: {endpoint}");
    println!(
        "enrollment expires at Unix time {}",
        binding
            .pending_expires_utc
            .ok_or("replacement binding has no expiry")?
    );
    Ok(())
}

fn parse_account_id(value: &str) -> Result<AccountId, Box<dyn std::error::Error>> {
    let (namespace, counter) = value
        .split_once(':')
        .ok_or("account ID must use NAMESPACE:COUNTER hexadecimal form")?;
    if namespace.len() != 16 || counter.len() != 16 {
        return Err("account ID components must each contain 16 hexadecimal digits".into());
    }
    Ok(AccountId::new(
        u64::from_str_radix(namespace, 16)?,
        u64::from_str_radix(counter, 16)?,
    ))
}

fn verify_dependency_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let resolve = metadata
        .resolve
        .ok_or("cargo metadata omitted resolve graph")?;
    let names: BTreeMap<_, _> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package.name.as_str()))
        .collect();
    let edges: BTreeMap<_, _> = resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.dependencies.as_slice()))
        .collect();

    let mut violations = Vec::new();
    for root in &metadata.workspace_members {
        let root_name = names
            .get(root.as_str())
            .copied()
            .ok_or("workspace member missing from package list")?;
        if root_name == "cdda-client" {
            continue;
        }
        let mut seen = BTreeSet::new();
        let mut pending = vec![root.as_str()];
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let name = names
                .get(id)
                .copied()
                .ok_or("resolve node has no package")?;
            if name == "bevy" || name.starts_with("bevy_") {
                violations.push(format!("{root_name} reaches forbidden dependency {name}"));
            }
            if let Some(dependencies) = edges.get(id) {
                pending.extend(dependencies.iter().map(String::as_str));
            }
        }
    }
    if violations.is_empty() {
        println!("dependency boundaries verified: Bevy is client-only");
        Ok(())
    } else {
        Err(violations.join("\n").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("tools crate should be nested beneath the workspace")
    }

    fn ledger() -> ParityLedger {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/parity-ledger.json"
        )))
        .expect("committed parity ledger should decode")
    }

    #[test]
    fn parity_ledger_rejects_unknown_fields() {
        let mut value: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/parity-ledger.json"
        )))
        .expect("committed parity ledger should decode as JSON");
        value
            .as_object_mut()
            .expect("parity ledger should be an object")
            .insert(String::from("protcol_version"), serde_json::json!(75));
        assert!(serde_json::from_value::<ParityLedger>(value).is_err());
    }

    #[test]
    fn completed_milestone_requires_completed_dependencies() {
        let mut ledger = ledger();
        ledger.active_milestone = String::from("conformance-foundation");
        let item = ledger
            .milestones
            .iter_mut()
            .find(|milestone| milestone.id == "item-containment")
            .expect("item milestone should exist");
        item.state = ParityState::Complete;
        item.differential_oracle = DifferentialState::NotApplicable;
        ledger
            .completed_family_evidence
            .push(CompletedFamilyEvidence {
                milestone_id: String::from("item-containment"),
                pinned_characterization: vec![String::from("README.md")],
                generalized_rust_engine: vec![String::from("README.md")],
                direct_rust_cpp_comparison: vec![String::from("README.md")],
                four_mode_conformance: vec![String::from("README.md")],
                runtime_content_admission: vec![String::from("README.md")],
                authoritative_client_path: vec![String::from("README.md")],
            });
        let result = validate_parity_ledger(&ledger, workspace());
        assert!(result.is_err_and(|error| error.to_string().contains("incomplete prerequisite")));
    }

    #[test]
    fn completed_milestone_requires_six_part_evidence() {
        let mut ledger = ledger();
        let foundation = ledger
            .milestones
            .iter_mut()
            .find(|milestone| milestone.id == "conformance-foundation")
            .expect("foundation milestone should exist");
        foundation.state = ParityState::Complete;
        foundation.differential_oracle = DifferentialState::NotApplicable;
        let result = validate_parity_ledger(&ledger, workspace());
        assert!(result.is_err_and(|error| {
            error
                .to_string()
                .contains("evidence does not exactly match")
        }));
    }
}
