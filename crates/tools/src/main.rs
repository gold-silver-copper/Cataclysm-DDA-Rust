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
use cdda_persistence::{ENROLLMENT_LIFETIME_SECONDS, ReplayBundleV1, WorldStore};
use cdda_protocol::{AccountId, AccountRole, BASELINE_COMMIT, ContentIdentity, EndpointIdentity};
use iroh::EndpointId;
use serde::Deserialize;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("verify-dependency-boundaries") => verify_dependency_boundaries(),
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
            "usage: cargo xtask <verify-dependency-boundaries|astronomy-table-check|account-create ...|account-recover ...|content-import ...|content-validate ...|content-inventory ...|content-inventory-check ...|replay-export ...|replay-verify ...>"
                .into(),
        ),
    }
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
