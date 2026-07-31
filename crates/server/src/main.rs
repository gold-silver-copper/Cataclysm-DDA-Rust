use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::TryRecvError;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use cdda_content::MapgenIdChoice;
use cdda_content::{
    AmmunitionRegistry, BashDamageProfileRegistry, BashFieldEffectDefinition, ConstructionRegistry,
    ContentManifest, DEFAULT_MANIFEST_PATH, DefaultRegionTerrainFurnitureRegistry,
    DescriptionSnippetRegistry, FieldTypeDefinition, FieldTypeRegistry, FurnitureDefinition,
    FurnitureRegistry, ItemDefinition, ItemGroupRegistry, ItemRegistry, MapgenRegistry,
    MaterialRegistry, ModCatalog, MonsterDefinition, MonsterRegistry, OvermapTerrainRegistry,
    ProficiencyRegistry, RecipeRegistry, SkillRegistry, StartLocationRegistry, TerrainDefinition,
    TerrainRegistry,
};
#[cfg(test)]
use cdda_content::{
    ItemGroupEntryWrapper, ItemGroupEvent, ItemGroupOverflow, ItemGroupSubtype, ItemGroupWrapper,
    StrictItemGroupDefinition, StrictItemGroupNode, StrictItemGroupNodeKind,
};
use cdda_persistence::{
    AllocatorInputV1, DatabaseBackupMetadata, JournalBatchV1, JournalTickV1,
    MIN_RECOVERABLE_SCHEMA_VERSION, PreparedReplayArchive, REPLAY_ARCHIVE_INTERVAL_SECONDS,
    ReplayArchiveCursor, ReplayBundleV1, SCHEMA_VERSION, SnapshotObjectV1, WorldStore,
};
#[cfg(test)]
use cdda_protocol::ChunkCoord;
#[cfg(test)]
use cdda_protocol::worldgen_catalog_is_valid;
use cdda_protocol::{
    ACTION_POINTS_PER_UPSTREAM_MOVE, ADMIN_ALPN, ActorConnectionUpdateV1, BASELINE_COMMIT,
    BashFieldEffectV1, BookStudyV1, ConstructionRecipeV1, ConstructionResultV1, ContentIdentity,
    CraftBookRequirementV1, CraftByproductV1, CraftComponentRequirementV1, CraftItemPrototypeV1,
    CraftProficiencyV1, CraftQualityProviderV1, CraftQualityRequirementV1, CraftRecipeV1,
    CraftSkillRequirementV1, CraftToolRequirementV1, CreatureCorpsePrototypeV1,
    CreaturePathSettingsV1, CreatureSizeV1, DisassemblyComponentV1, DisassemblyRecipeV1,
    ENROLL_ALPN, FieldIntensityLevelV1, FieldTypeSnapshotV1, FurnitureBashTypeV1,
    FurnitureTileSnapshot, GAME_ALPN, IntegralMagazinePocketPrototypeV1, MAX_ACTOR_BASE_STAT,
    MAX_BOOK_STUDY_MOVES, MAX_CRAFT_QUALITY_PROVIDERS, MAX_CRAFT_SUPPORT_ALTERNATIVES,
    MAX_CRAFT_SUPPORT_GROUPS, MAX_SKILL_LEVEL, MagazineWellPrototypeV1, PROTOCOL_VERSION,
    PoweredToolStateV1, RangedWeaponSnapshot, SimTick, SmashItemTypeV1, TerrainBashTypeV1,
    TerrainTileSnapshot, adjusted_book_study_time_moves,
};
#[cfg(test)]
use cdda_protocol::{
    AmmunitionCapacityV1, AmmunitionContainerPocketPrototypeV1, ItemGroupChargeRangeV1,
    ItemGroupContentsSourceV1, ItemGroupEventV1, ItemGroupSourceV1, ItemGroupTargetV1,
    encode_item_group_dressing_marker, item_group_source_max_outputs,
};
use cdda_server::{
    AuthorizationChangeHub, CharacterCreationError, CharacterCreationRequest, ChatHub,
    CommittedEventBatch, CommittedEventHub, ConstructionCatalog, CraftingCatalog,
    DisassemblyCatalog, DurabilityAcknowledgement, MAX_CONNECTION_TASKS, PersistenceHandle,
    PersistenceHost, ReadingCatalog, SessionRegistry, SimulationHandle, SimulationHost,
    SimulationOutput, SnapshotReceipt, SnapshotWriteOutcome, bind_iroh_endpoint,
    character_creation_channel, handle_admin_connection, handle_enrollment_connection,
    handle_game_connection_with_sessions, load_or_create_secret_key,
};
#[cfg(test)]
use cdda_sim::Chunk;
use cdda_sim::{CreatureSpawn, ItemSpawn, WorldState, canonical_events_hash};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod item_groups;
#[cfg(test)]
mod regional_field_acceptance;
mod worldgen;

use item_groups::{
    RuntimeItemGroupContent, merge_item_group_catalogs, runtime_ammunition_containers,
    runtime_bash_item_group_catalog, runtime_bash_item_group_source,
    runtime_item_temperature_capability, runtime_item_tracks_temperature,
    runtime_named_item_group_catalog,
};
#[cfg(test)]
use item_groups::{
    assert_custom_freezing_item_admission, assert_custom_freezing_recipe_admission,
    assert_regional_field_item_group_closure, runtime_item_group_charges, runtime_item_group_graph,
    runtime_item_group_item,
};
use worldgen::{RuntimeMapgenContent, bootstrap_regional_field_overmap, runtime_mapgen_worldgen};
#[cfg(test)]
use worldgen::{
    bootstrap_lmoe_overmap, runtime_mapgen_furniture_choice, runtime_mapgen_terrain_choice,
};

#[derive(Default)]
struct PendingJournal {
    ticks: Vec<JournalTickV1>,
    durability: Vec<DurabilityAcknowledgement>,
    event_batches: Vec<CommittedEventBatch>,
    event_hub: CommittedEventHub,
}

struct OpenedWorld {
    store: WorldStore,
    world: WorldState,
    journal_sequence: u64,
    recovery_connection_updates: Vec<ActorConnectionUpdateV1>,
}

struct RuntimeWorldContent<'a> {
    ammunition: &'a AmmunitionRegistry,
    snippets: &'a DescriptionSnippetRegistry,
    items: &'a ItemRegistry,
    materials: &'a MaterialRegistry,
    item_groups: &'a ItemGroupRegistry,
    monsters: &'a MonsterRegistry,
    fields: &'a FieldTypeRegistry,
    bash_profiles: &'a BashDamageProfileRegistry,
    terrain: &'a TerrainRegistry,
    furniture: &'a FurnitureRegistry,
    regions: &'a DefaultRegionTerrainFurnitureRegistry,
    mapgen: &'a MapgenRegistry,
    overmap_terrain: &'a OvermapTerrainRegistry,
    start_locations: &'a StartLocationRegistry,
}

const ID_REFILL_THRESHOLD: u64 = 512;
const REPLAY_ARCHIVE_POLL_SECONDS: u64 = 60;
const REPLAY_ARCHIVE_RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_REPLAY_ARCHIVE_DECODED: usize = 256 * 1024 * 1024;
const MAX_REPLAY_ARCHIVE_ENCODED: u64 = 256 * 1024 * 1024;
const MAX_SNAPSHOT_OBJECT_DECODED: u64 = 64 * 1024 * 1024;
const MAX_SNAPSHOT_OBJECT_ENCODED: u64 = 64 * 1024 * 1024;
const BACKUP_FORMAT_VERSION: u16 = 1;
const BACKUP_INTERVAL_SECONDS: i64 = 60 * 60;
const BACKUP_POLL_SECONDS: u64 = 60;
const BACKUP_HOURLY_RETENTION: usize = 24;
const BACKUP_DAILY_RETENTION: usize = 30;
const MAX_BACKUP_MANIFEST_BYTES: u64 = 1024 * 1024;
const RESTORE_PROVENANCE_FILE: &str = "restore-provenance.json";

type ReplayArchiveError = Box<dyn std::error::Error + Send + Sync>;

struct ReplayArchiveWrite {
    start: ReplayArchiveCursor,
    end: ReplayArchiveCursor,
    path: PathBuf,
    encoded_bytes: usize,
    checksum: [u8; 32],
    snapshot_object_path: PathBuf,
    snapshot_object_hash: [u8; 32],
    snapshot_gc: SnapshotObjectGc,
    final_tick: u64,
}

struct ReplayArchiveTask {
    thread: JoinHandle<Result<ReplayArchiveWrite, ReplayArchiveError>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SnapshotObjectGc {
    retained_archives: usize,
    retained_objects: usize,
    removed_objects: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct BackupManifestV1 {
    format_version: u16,
    created_utc_seconds: i64,
    baseline_commit: String,
    protocol_version: u16,
    schema_version: i64,
    content: ContentIdentity,
    server_endpoint_id: String,
    database_checksum: String,
    identity_checksum: String,
    world_namespace: u64,
    journal_sequence: u64,
    tick: u64,
    state_hash: String,
}

struct BackupWrite {
    created_utc_seconds: i64,
    path: PathBuf,
    metadata: DatabaseBackupMetadata,
    database_checksum: [u8; 32],
}

struct BackupTask {
    thread: JoinHandle<Result<BackupWrite, ReplayArchiveError>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut arguments = std::env::args_os().skip(1);
    let first_argument = arguments.next();
    if first_argument.as_deref() == Some(std::ffi::OsStr::new("--restore")) {
        let backup = arguments
            .next()
            .map(PathBuf::from)
            .ok_or("restore requires a backup generation directory")?;
        let world = arguments
            .next()
            .map(PathBuf::from)
            .ok_or("restore requires a new world directory")?;
        let manifest = arguments
            .next()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST_PATH));
        if arguments.next().is_some() {
            return Err(
                "usage: cdda-server --restore <backup-generation> <new-world-directory> [content-manifest.json]"
                    .into(),
            );
        }
        let content = load_content_identity(&manifest)?;
        let restored = restore_backup_generation(&backup, &world, &content)
            .map_err(|error| error.to_string())?;
        println!("Restored world: {}", world.display());
        println!("Server endpoint: {}", restored.server_endpoint_id);
        println!("Backup UTC: {}", restored.created_utc_seconds);
        return Ok(());
    }
    let world_directory = first_argument
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("world"));
    let content_manifest_path = arguments
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST_PATH));
    if arguments.next().is_some() {
        return Err("usage: cdda-server [world-directory] [content-manifest.json]".into());
    }
    let content_manifest = ContentManifest::load(&content_manifest_path)?;
    let content_root = content_manifest_path
        .parent()
        .ok_or("content manifest has no parent directory")?;
    let mod_catalog = ModCatalog::load(&content_manifest, content_root)?;
    let enabled_mods = mod_catalog.recommended_new_world()?;
    let ammunition = AmmunitionRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let snippets = DescriptionSnippetRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let items =
        ItemRegistry::load_selected(&content_manifest, content_root, &mod_catalog, &enabled_mods)?;
    let materials = MaterialRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let item_groups = ItemGroupRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let monsters = MonsterRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    validate_monster_attack_costs(&monsters)?;
    let fields = FieldTypeRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let bash_profiles = BashDamageProfileRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let terrain = TerrainRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let furniture = FurnitureRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let regions = DefaultRegionTerrainFurnitureRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
        &terrain,
        &furniture,
    )?;
    let mapgen = MapgenRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
        &terrain,
        &furniture,
        &item_groups,
    )?;
    let overmap_terrain = OvermapTerrainRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let start_locations = StartLocationRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let skills =
        SkillRegistry::load_selected(&content_manifest, content_root, &mod_catalog, &enabled_mods)?;
    let proficiencies = ProficiencyRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let recipes = RecipeRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
        &items,
        &skills,
        &proficiencies,
    )?;
    let constructions = ConstructionRegistry::load_selected(
        &content_manifest,
        content_root,
        &mod_catalog,
        &enabled_mods,
    )?;
    let crafting = build_crafting_catalog(&recipes, &items, &materials, &proficiencies)?;
    let reading = build_reading_catalog(&items, &skills)?;
    let disassembly =
        build_disassembly_catalog(&recipes, &items, &materials, &ammunition, &crafting)?;
    let construction = build_construction_catalog(
        &constructions,
        &recipes,
        &items,
        &skills,
        &terrain,
        &furniture,
    )?;
    ammunition.validate_item_references(&items)?;
    let content = ContentIdentity {
        baseline_commit: BASELINE_COMMIT.to_owned(),
        manifest_hash: content_manifest.canonical_hash()?,
        enabled_mods,
    };
    std::fs::create_dir_all(&world_directory)?;
    let identity_path = world_directory.join("server-identity.key");
    let database_path = world_directory.join("world.db");
    let replay_directory = world_directory.join("replays");
    let snapshot_object_directory = world_directory.join("snapshot-objects");
    let backup_directory = world_directory.join("backups");
    let secret_key = load_or_create_secret_key(&identity_path)?;
    let secret_key_bytes = secret_key.to_bytes();
    let endpoint_id = secret_key.public();
    verify_restored_world_identity(
        &world_directory,
        &content,
        secret_key_bytes,
        &endpoint_id.to_string(),
    )
    .map_err(|error| error.to_string())?;
    let OpenedWorld {
        store,
        world,
        mut journal_sequence,
        recovery_connection_updates,
    } = open_world(
        &database_path,
        &RuntimeWorldContent {
            ammunition: &ammunition,
            snippets: &snippets,
            items: &items,
            materials: &materials,
            item_groups: &item_groups,
            monsters: &monsters,
            fields: &fields,
            bash_profiles: &bash_profiles,
            terrain: &terrain,
            furniture: &furniture,
            regions: &regions,
            mapgen: &mapgen,
            overmap_terrain: &overmap_terrain,
            start_locations: &start_locations,
        },
    )?;
    let persistence_host = PersistenceHost::start(store)?;
    let persistence = persistence_host.handle();
    let host = SimulationHost::start_with_all_gameplay_catalogs_and_recovery_inputs(
        world,
        crafting.clone(),
        reading.clone(),
        disassembly.clone(),
        construction.clone(),
        recovery_connection_updates,
    )?;
    let simulation = host.handle();
    let endpoint = bind_iroh_endpoint(secret_key).await?;
    let endpoint_address_path = world_directory.join("endpoint-address.json");
    std::fs::write(
        &endpoint_address_path,
        serde_json::to_vec_pretty(&endpoint.addr())?,
    )?;
    info!(%endpoint_id, world = %world_directory.display(), "server endpoint is ready");
    println!("CDDA Rust server endpoint: {endpoint_id}");
    println!("Endpoint address: {}", endpoint_address_path.display());
    println!("World directory: {}", world_directory.display());
    println!(
        "Content manifest: {} ({})",
        content_manifest_path.display(),
        blake3::Hash::from_bytes(content.manifest_hash)
    );
    println!(
        "Content mods: {} catalog entries; enabled {}",
        mod_catalog.len(),
        content.enabled_mods.join(",")
    );
    println!("Ammunition types: {}", ammunition.len());
    println!("Description snippet categories: {}", snippets.len());
    println!(
        "Items: {} concrete definitions; {} abstracts",
        items.len(),
        items.abstract_count()
    );
    println!("Item groups: {} definitions", item_groups.len());
    println!(
        "Monsters: {} concrete definitions; {} abstracts",
        monsters.len(),
        monsters.abstract_count()
    );
    println!("Field types: {} map definitions", fields.len());
    println!(
        "Terrain: {} concrete definitions; {} abstracts",
        terrain.len(),
        terrain.abstract_count()
    );
    println!(
        "Furniture: {} concrete definitions; {} abstracts",
        furniture.len(),
        furniture.abstract_count()
    );
    println!(
        "Recipes: {} concrete definitions; {} runnable; {} explicit uncraft definitions",
        recipes.len(),
        crafting.len(),
        recipes.uncraft_count()
    );
    println!("Readable skill books: {}", reading.len());
    println!(
        "Strict reversible disassembly recipes: {}",
        disassembly.len()
    );
    println!("Strict constructions: {}", construction.len());

    let mut drain_timer = tokio::time::interval(Duration::from_millis(25));
    let mut snapshot_timer = tokio::time::interval(Duration::from_secs(5));
    snapshot_timer.tick().await;
    let mut replay_archive_timer =
        tokio::time::interval(Duration::from_secs(REPLAY_ARCHIVE_POLL_SECONDS));
    replay_archive_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    replay_archive_timer.tick().await;
    let mut backup_timer = tokio::time::interval(Duration::from_secs(BACKUP_POLL_SECONDS));
    backup_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut connections = tokio::task::JoinSet::new();
    let sessions = SessionRegistry::default();
    let authorization_changes = AuthorizationChangeHub::default();
    let committed_event_hub = CommittedEventHub::default();
    let chat_hub = ChatHub::default();
    let (character_creator, mut character_creation_requests) = character_creation_channel();
    let mut pending_journal = PendingJournal {
        event_hub: committed_event_hub.clone(),
        ..PendingJournal::default()
    };
    let mut pending_snapshots = VecDeque::new();
    let mut replay_archive_task = None;
    let mut backup_task = None;
    let mut last_backup_utc =
        latest_backup_utc(&backup_directory, &content, &endpoint_id.to_string())
            .map_err(|error| error.to_string())?;
    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal?;
                break;
            }
            _ = drain_timer.tick() => {
                drain_outputs(
                    &host,
                    &persistence,
                    &mut pending_journal,
                    &mut journal_sequence,
                )?;
                drain_snapshot_results(&mut pending_snapshots)?;
                poll_replay_archive(&mut replay_archive_task, &persistence)?;
                poll_backup(&mut backup_task, &mut last_backup_utc)?;
            }
            _ = snapshot_timer.tick() => {
                let receipt = queue_checkpoint_world(
                    &host,
                    &simulation,
                    &persistence,
                    &mut pending_journal,
                    &mut journal_sequence,
                )?;
                pending_snapshots.push_back(receipt);
            }
            _ = replay_archive_timer.tick(), if replay_archive_task.is_none() => {
                let now = utc_now_seconds()?;
                let cursor = persistence.replay_archive_cursor()?;
                let elapsed = now
                    .checked_sub(cursor.archived_utc_seconds)
                    .ok_or("replay archive clock moved backwards")?;
                if elapsed >= REPLAY_ARCHIVE_INTERVAL_SECONDS {
                    let final_snapshot = queue_checkpoint_world(
                        &host,
                        &simulation,
                        &persistence,
                        &mut pending_journal,
                        &mut journal_sequence,
                    )?;
                    if final_snapshot.wait()? != SnapshotWriteOutcome::Written {
                        return Err("hourly replay snapshot was superseded".into());
                    }
                    drain_snapshot_results(&mut pending_snapshots)?;
                    if let Some(prepared) = persistence.prepare_replay_archive(
                        journal_sequence,
                        now,
                        content.clone(),
                    )? {
                        replay_archive_task = Some(start_replay_archive(
                            replay_directory.clone(),
                            snapshot_object_directory.clone(),
                            prepared,
                        )?);
                    }
                }
            }
            _ = backup_timer.tick(), if backup_task.is_none() => {
                let now = utc_now_seconds()?;
                let due = match last_backup_utc {
                    Some(previous) => {
                        let elapsed = now
                            .checked_sub(previous)
                            .ok_or("backup clock moved backwards")?;
                        if elapsed < 0 {
                            return Err("backup clock moved backwards".into());
                        }
                        elapsed >= BACKUP_INTERVAL_SECONDS
                    }
                    None => true,
                };
                if due {
                    backup_task = Some(start_backup(
                        backup_directory.clone(),
                        persistence.clone(),
                        secret_key_bytes,
                        content.clone(),
                        now,
                    )?);
                }
            }
            Some(request) = character_creation_requests.recv() => {
                process_character_creation(
                    request,
                    &host,
                    &simulation,
                    &persistence,
                    &mut pending_journal,
                    &mut journal_sequence,
                ).await?;
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else {
                    break;
                };
                if connections.len() >= MAX_CONNECTION_TASKS {
                    warn!(
                        limit = MAX_CONNECTION_TASKS,
                        "dropping incoming connection at task limit"
                    );
                    continue;
                }
                let connection_persistence = persistence.clone();
                let connection_simulation = simulation.clone();
                let connection_content = content.clone();
                let connection_sessions = sessions.clone();
                let connection_authorization_changes = authorization_changes.clone();
                let connection_character_creator = character_creator.clone();
                let connection_committed_events = committed_event_hub.clone();
                let connection_chat = chat_hub.clone();
                connections.spawn(async move {
                    match incoming.await {
                        Ok(connection) if connection.alpn() == ENROLL_ALPN => {
                            let result = handle_enrollment_connection(
                                &connection,
                                connection_persistence,
                            )
                            .await;
                            connection.close(0_u32.into(), b"enrollment complete");
                            result.map(|_| ()).map_err(|error| error.to_string())
                        }
                        Ok(connection) if connection.alpn() == GAME_ALPN => {
                            let result = handle_game_connection_with_sessions(
                                &connection,
                                connection_persistence,
                                connection_simulation,
                                connection_content,
                                connection_sessions,
                                connection_authorization_changes,
                                connection_character_creator,
                                connection_committed_events,
                                connection_chat,
                            )
                            .await;
                            connection.close(0_u32.into(), b"gameplay session ended");
                            result.map_err(|error| error.to_string())
                        }
                        Ok(connection) if connection.alpn() == ADMIN_ALPN => {
                            let result = handle_admin_connection(
                                &connection,
                                connection_persistence,
                                connection_authorization_changes,
                                connection_sessions,
                                connection_simulation,
                            )
                            .await;
                            connection.close(0_u32.into(), b"administration session ended");
                            result.map_err(|error| error.to_string())
                        }
                        Ok(connection) => {
                            connection.close(1_u32.into(), b"unsupported protocol");
                            Err(String::from("unsupported connection ALPN"))
                        }
                        Err(error) => Err(error.to_string()),
                    }
                });
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => warn!(%error, "connection ended with an error"),
                    Err(error) => warn!(%error, "connection task failed"),
                }
            }
        }
    }

    endpoint.close().await;
    let graceful_shutdown = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(result) = connections.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => warn!(%error, "connection ended during shutdown"),
                Err(error) => warn!(%error, "connection task failed during shutdown"),
            }
        }
    })
    .await;
    if graceful_shutdown.is_err() {
        warn!(
            remaining = connections.len(),
            "forcing connection tasks to stop after graceful shutdown deadline"
        );
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    finish_replay_archive(&mut replay_archive_task, &persistence)?;
    finish_backup(&mut backup_task, &mut last_backup_utc)?;
    let disconnect_boundary = disconnect_all_actors(&simulation)?;
    drain_through_next_tick(
        &host,
        &persistence,
        &mut pending_journal,
        &mut journal_sequence,
        disconnect_boundary,
    )?;
    let final_snapshot = queue_checkpoint_world(
        &host,
        &simulation,
        &persistence,
        &mut pending_journal,
        &mut journal_sequence,
    )?;
    if final_snapshot.wait()? != SnapshotWriteOutcome::Written {
        return Err("final persistence snapshot was superseded".into());
    }
    drain_snapshot_results(&mut pending_snapshots)?;
    persistence.finish_runtime(utc_now_seconds()?)?;
    persistence.checkpoint()?;
    let reason = host.shutdown();
    persistence_host.shutdown();
    info!(?reason, "server stopped cleanly");
    Ok(())
}

fn open_world(
    path: &Path,
    content: &RuntimeWorldContent<'_>,
) -> Result<OpenedWorld, Box<dyn std::error::Error>> {
    let item_group_content = RuntimeItemGroupContent {
        items: content.items,
        materials: content.materials,
        ammunition: content.ammunition,
        snippets: content.snippets,
        monsters: content.monsters,
    };
    let items = content.items;
    let item_groups = content.item_groups;
    let monsters = content.monsters;
    let fields = content.fields;
    let bash_profiles = content.bash_profiles;
    let terrain = content.terrain;
    let furniture = content.furniture;
    let regions = content.regions;
    let mapgen = content.mapgen;
    let overmap_terrain = content.overmap_terrain;
    let start_locations = content.start_locations;
    let mut store = WorldStore::open(path)?;
    let metadata = match store.metadata_optional()? {
        Some(metadata) => metadata,
        None => {
            let mut namespace = rand::random::<u64>();
            if namespace == 0 {
                namespace = 1;
            }
            store.initialize_world(namespace, rand::random::<[u8; 32]>())?;
            store.metadata()?
        }
    };
    let has_snapshot = store.latest_snapshot()?.is_some();
    let mut initial = WorldState::new(metadata.world_namespace, metadata.world_seed);
    for field_type_id in [
        "fd_acid",
        "fd_bile",
        "fd_blood",
        "fd_blood_insect",
        "fd_blood_invertebrate",
        "fd_blood_veggy",
        "fd_dust",
        "fd_splinters",
    ] {
        let definition = fields
            .get(field_type_id)
            .ok_or("pinned default content is missing a creature blood field")?;
        initial.register_field_type(runtime_field_type(definition)?)?;
    }
    let terrain_bash_definitions = ["t_wall", "t_door_b", "t_door_c", "t_door_frame"]
        .into_iter()
        .map(|terrain_id| {
            terrain.get(terrain_id).ok_or_else(|| {
                format!("pinned default content is missing structural terrain {terrain_id}")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let furniture_bashes = runtime_furniture_bash_types(
        furniture,
        bash_profiles,
        fields,
        item_group_content,
        item_groups,
    )?;
    let bash_item_group_catalog = runtime_bash_item_group_catalog(
        terrain_bash_definitions
            .iter()
            .filter_map(|definition| definition.bash.as_ref())
            .chain(furniture_bashes.iter().filter_map(|runtime| {
                furniture
                    .get(&runtime.furniture_id)
                    .and_then(|definition| definition.bash.as_ref())
            })),
        item_groups,
        item_group_content,
    )?;
    let field_graph = item_groups.strict_graph("field")?;
    let item_group_catalog = merge_item_group_catalogs([
        bash_item_group_catalog,
        runtime_named_item_group_catalog(&field_graph, item_group_content)?,
    ])?;
    let worldgen = runtime_mapgen_worldgen(
        bootstrap_regional_field_overmap(overmap_terrain)?,
        start_locations
            .get("sloc_field")
            .ok_or("pinned default content is missing sloc_field")?,
        RuntimeMapgenContent {
            mapgen,
            regions,
            terrain,
            furniture,
            item_groups: &item_group_catalog,
        },
    )?;
    initial.register_item_group_catalog(item_group_catalog)?;
    for definition in terrain_bash_definitions {
        let dynamic_floor_result =
            matches!(definition.id.as_str(), "t_wall" | "t_door_frame").then_some("t_floor");
        initial.register_terrain_bash_type(runtime_terrain_bash_type(
            definition,
            bash_profiles,
            fields,
            terrain,
            item_group_content,
            item_groups,
            dynamic_floor_result,
        )?)?;
    }
    for profile in runtime_smash_item_types(items) {
        initial.register_smash_item_type(profile)?;
    }
    for definition in furniture
        .iter()
        .filter(|definition| definition.bash.is_some())
    {
        initial.register_furniture_bash_presence(definition.id.clone())?;
    }
    for bash in furniture_bashes {
        initial.register_furniture_bash_type(bash)?;
    }
    if !has_snapshot {
        initial.configure_worldgen(worldgen)?;
        initial.generate_initial_bubble(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 })?;
    }
    let (journal_sequence, mut world) = store.recover_latest(initial)?;
    let mut recovery_connection_updates = world.disconnect_all_for_recovery();
    let runtime_start = store.begin_runtime(utc_now_seconds()?)?;
    let mut journal_sequence = journal_sequence;
    apply_unexpected_downtime(
        &mut store,
        &mut world,
        &mut journal_sequence,
        runtime_start,
        &mut recovery_connection_updates,
    )?;
    let allocator_tick = world.tick();
    world.advance_allocator_high_water(metadata.id_high_water)?;
    let block = store.reserve_id_block()?;
    world.install_reserved_block(block)?;
    journal_sequence = store.append_journal_batch_at(
        &JournalBatchV1 {
            ticks: Vec::new(),
            allocator_inputs: vec![
                AllocatorInputV1::IdBlockAbandoned {
                    at_tick: allocator_tick,
                    high_water: metadata.id_high_water,
                },
                AllocatorInputV1::IdBlockReserved {
                    at_tick: allocator_tick,
                    block,
                },
            ],
        },
        utc_now_seconds()?,
    )?;
    if !has_snapshot {
        let rock = items
            .get("rock")
            .ok_or("pinned default content has no rock item")?;
        let spawn_position = cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 };
        world.spawn_ground_item(ItemSpawn {
            position: spawn_position,
            type_id: rock.id.clone(),
            charges: default_instance_charges(rock),
            melee_damage_milli: rock.melee_damage_milli()?,
            calories: rock.calories,
            quench: rock.quench,
            comestible_type: rock.comestible_type.clone(),
            ammunition_type: String::new(),
            ranged_weapon: None,
        })?;
        for (item_id, charge_override) in [
            ("water_clean", None),
            ("meat_cooked", None),
            ("socks", None),
            ("stick", None),
            ("knife_small", None),
            ("hammer", None),
            ("toasterpastryfrozen", None),
            // Until pockets and power grids land, aggregate loaded/connected
            // tool energy is represented directly by the tool's charge field.
            ("toaster", Some(20)),
            ("manual_pistol", None),
        ] {
            let item = items
                .get(item_id)
                .ok_or("pinned default content has no survival starter item")?;
            world.spawn_ground_item(ItemSpawn {
                position: spawn_position,
                type_id: item.id.clone(),
                charges: charge_override.unwrap_or_else(|| default_instance_charges(item)),
                melee_damage_milli: item.melee_damage_milli()?,
                calories: item.calories,
                quench: item.quench,
                comestible_type: item.comestible_type.clone(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })?;
        }
        let quiver = items
            .get("quiver")
            .ok_or("pinned default content has no starter quiver")?;
        let quiver_containers = runtime_ammunition_containers(quiver)?;
        if quiver_containers.is_empty() {
            return Err("pinned starter quiver has no strict ammunition container".into());
        }
        world.spawn_ground_item_with_ammunition_containers(
            ItemSpawn {
                position: spawn_position,
                type_id: quiver.id.clone(),
                charges: default_instance_charges(quiver),
                melee_damage_milli: quiver.melee_damage_milli()?,
                calories: quiver.calories,
                quench: quiver.quench,
                comestible_type: quiver.comestible_type.clone(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            },
            quiver_containers,
        )?;
        let arrow = items
            .get("arrow_wood")
            .ok_or("pinned default content has no starter wooden arrow")?;
        world.spawn_ground_item(ItemSpawn {
            position: spawn_position,
            type_id: arrow.id.clone(),
            charges: arrow.default_charges().max(1),
            melee_damage_milli: arrow.melee_damage_milli()?,
            calories: arrow.calories,
            quench: arrow.quench,
            comestible_type: arrow.comestible_type.clone(),
            ammunition_type: single_ammunition_type(arrow)?,
            ranged_weapon: None,
        })?;
        let flashlight = items
            .get("flashlight")
            .ok_or("pinned default content has no starter flashlight")?;
        let (flashlight_capacity, flashlight_well) = runtime_magazine_storage(flashlight, items)?;
        world.spawn_ground_item_with_powered_magazine_wells(
            ItemSpawn {
                position: spawn_position,
                type_id: flashlight.id.clone(),
                charges: 0,
                melee_damage_milli: flashlight.melee_damage_milli()?,
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            },
            flashlight_capacity,
            flashlight_well,
            0,
            runtime_powered_tool(flashlight, items)?,
        )?;
        let medium_battery = items
            .get("medium_battery_cell")
            .ok_or("pinned default content has no starter medium battery")?;
        let battery_ammunition = items
            .get("battery")
            .ok_or("pinned default content has no loose battery ammunition")?;
        let (_, battery_wells) = runtime_magazine_storage(medium_battery, items)?;
        let battery_integral_magazines = runtime_integral_magazines(medium_battery);
        let starter_battery_pocket = battery_integral_magazines
            .first()
            .ok_or("pinned starter medium battery has no strict magazine pocket")?;
        let starter_battery_pocket_index = starter_battery_pocket.pocket_index;
        let starter_battery_charges = i32::try_from(starter_battery_pocket.capacity)?;
        world.spawn_ground_item_with_preloaded_item_backed_magazines(
            ItemSpawn {
                position: spawn_position,
                type_id: medium_battery.id.clone(),
                charges: 0,
                melee_damage_milli: medium_battery.melee_damage_milli()?,
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            },
            battery_integral_magazines,
            battery_wells,
            None,
            vec![(
                starter_battery_pocket_index,
                ItemSpawn {
                    position: spawn_position,
                    type_id: battery_ammunition.id.clone(),
                    charges: starter_battery_charges,
                    melee_damage_milli: battery_ammunition.melee_damage_milli()?,
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: single_ammunition_type(battery_ammunition)?,
                    ranged_weapon: None,
                },
            )],
        )?;
        let revolver = items
            .get("model_10_revolver")
            .ok_or("pinned default content has no starter revolver")?;
        let ammunition = items
            .get("38_special")
            .ok_or("pinned default content has no starter revolver ammunition")?;
        if !revolver.ammo.contains("38") || !ammunition.ammo_types.contains("38") {
            return Err("starter revolver and ammunition are incompatible".into());
        }
        let ranged_damage = revolver
            .ranged_damage
            .values()
            .chain(ammunition.damage.values())
            .try_fold(0.0, |total, damage| {
                let total = total + damage.amount;
                total
                    .is_finite()
                    .then_some(total)
                    .ok_or("starter ranged damage is not finite")
            })?;
        let range = revolver
            .range
            .checked_add(ammunition.range)
            .ok_or("starter ranged range overflow")?;
        let dispersion = revolver
            .dispersion
            .checked_add(ammunition.dispersion)
            .ok_or("starter ranged dispersion overflow")?;
        let sound_volume = firearm_sound_volume(revolver, ammunition)?;
        let ranged_weapon = RangedWeaponSnapshot {
            ammunition_type: String::from("38"),
            ammunition_remaining: u16::try_from(revolver.clip_size)?,
            ammunition_capacity: u16::try_from(revolver.clip_size)?,
            range: u16::try_from(range)?,
            damage: ranged_stat_u16(ranged_damage, "damage")?,
            dispersion: u16::try_from(dispersion)?,
            sound_volume,
        };
        world.spawn_ground_item(ItemSpawn {
            position: spawn_position,
            type_id: revolver.id.clone(),
            charges: 1,
            melee_damage_milli: revolver.melee_damage_milli()?,
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::new(),
            ranged_weapon: Some(ranged_weapon),
        })?;
        world.spawn_ground_item(ItemSpawn {
            position: spawn_position,
            type_id: ammunition.id.clone(),
            charges: ammunition.count.max(1),
            melee_damage_milli: ammunition.melee_damage_milli()?,
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::from("38"),
            ranged_weapon: None,
        })?;
        let zombie = monsters
            .get("mon_zombie")
            .ok_or("pinned default content has no classic zombie")?;
        let blood_field_type_id = monster_blood_field_type(zombie).to_owned();
        let path_settings = monster_path_settings(zombie)?;
        let size = monster_size(zombie);
        world.spawn_creature(CreatureSpawn {
            type_id: zombie.id.clone(),
            position: cdda_protocol::WorldPosition { x: 5, y: 5, z: 0 },
            hp: zombie.hp,
            speed: u16::try_from(zombie.speed)?,
            attack_cost_moves: monster_attack_cost(zombie)?,
            aggression: i16::try_from(zombie.aggression)?,
            melee_skill: u16::try_from(zombie.melee_skill)?,
            dodge: u16::try_from(zombie.dodge)?,
            size,
            melee_dice: u16::try_from(zombie.melee_dice)?,
            melee_dice_sides: u16::try_from(zombie.melee_dice_sides)?,
            can_see: zombie.flags.contains("SEES"),
            vision_day: u16::try_from(zombie.vision_day)?,
            vision_night: u16::try_from(zombie.vision_night)?,
            stumbles: zombie.flags.contains("STUMBLES"),
            bashes: zombie.flags.contains("BASHES"),
            group_bash: zombie.flags.contains("GROUP_BASH"),
            hears: zombie.flags.contains("HEARS"),
            good_hearing: zombie.flags.contains("GOODHEARING"),
            clumsy_attacks: zombie.flags.contains("CLUMSY_ATTACKS"),
            immobile: zombie.flags.contains("IMMOBILE"),
            pacifist: zombie.flags.contains("PACIFIST"),
            can_open_doors: zombie.flags.contains("CAN_OPEN_DOORS"),
            path_settings,
            blood_field_type_id: blood_field_type_id.clone(),
            corpse: Some(CreatureCorpsePrototypeV1 {
                monster_type_id: zombie.id.clone(),
                max_hp: zombie.hp,
                speed: u16::try_from(zombie.speed)?,
                attack_cost_moves: monster_attack_cost(zombie)?,
                aggression: i16::try_from(zombie.aggression)?,
                melee_skill: u16::try_from(zombie.melee_skill)?,
                dodge: u16::try_from(zombie.dodge)?,
                size,
                melee_dice: u16::try_from(zombie.melee_dice)?,
                melee_dice_sides: u16::try_from(zombie.melee_dice_sides)?,
                can_see: zombie.flags.contains("SEES"),
                vision_day: u16::try_from(zombie.vision_day)?,
                vision_night: u16::try_from(zombie.vision_night)?,
                stumbles: zombie.flags.contains("STUMBLES"),
                bashes: zombie.flags.contains("BASHES"),
                group_bash: zombie.flags.contains("GROUP_BASH"),
                hears: zombie.flags.contains("HEARS"),
                good_hearing: zombie.flags.contains("GOODHEARING"),
                clumsy_attacks: zombie.flags.contains("CLUMSY_ATTACKS"),
                immobile: zombie.flags.contains("IMMOBILE"),
                pacifist: zombie.flags.contains("PACIFIST"),
                can_open_doors: zombie.flags.contains("CAN_OPEN_DOORS"),
                path_settings,
                blood_field_type_id,
                revives: zombie.flags.contains("REVIVES"),
            }),
        })?;
    }
    store.write_snapshot(journal_sequence, &world)?;
    store.initialize_replay_archive_cursor(journal_sequence, utc_now_seconds()?)?;
    Ok(OpenedWorld {
        store,
        world,
        journal_sequence,
        recovery_connection_updates,
    })
}

fn apply_unexpected_downtime(
    store: &mut WorldStore,
    world: &mut WorldState,
    journal_sequence: &mut u64,
    runtime_start: cdda_persistence::RuntimeStart,
    recovery_connection_updates: &mut Vec<ActorConnectionUpdateV1>,
) -> Result<(), Box<dyn std::error::Error>> {
    let elapsed_seconds = runtime_start.elapsed_seconds()?;
    if elapsed_seconds == 0 {
        return Ok(());
    }
    let total_ticks = elapsed_seconds
        .checked_mul(cdda_protocol::SimTick::HZ)
        .ok_or("unexpected-downtime tick count overflow")?;
    let mut completed_ticks = 0_u64;
    while completed_ticks < total_ticks {
        let batch_ticks = (total_ticks - completed_ticks).min(4_000);
        let mut ticks = Vec::with_capacity(usize::try_from(batch_ticks)?);
        for _ in 0..batch_ticks {
            let connection_updates = if completed_ticks == 0 && ticks.is_empty() {
                std::mem::take(recovery_connection_updates)
            } else {
                Vec::new()
            };
            let outcome = world.advance_tick_with_recovery_inputs(
                Vec::new(),
                Vec::new(),
                connection_updates.clone(),
            )?;
            ticks.push(JournalTickV1 {
                tick: outcome.tick,
                commands: Vec::new(),
                held_movement: Vec::new(),
                connection_updates,
                events_hash: canonical_events_hash(&outcome.events)?,
                state_hash: outcome.canonical_hash,
            });
        }
        completed_ticks = completed_ticks
            .checked_add(batch_ticks)
            .ok_or("unexpected-downtime progress overflow")?;
        let completed_seconds = completed_ticks / cdda_protocol::SimTick::HZ;
        let committed_utc = runtime_start
            .from_utc_seconds
            .checked_add(i64::try_from(completed_seconds)?)
            .ok_or("unexpected-downtime UTC overflow")?;
        *journal_sequence = store.append_journal_batch_at(
            &JournalBatchV1 {
                ticks,
                allocator_inputs: Vec::new(),
            },
            committed_utc,
        )?;
    }
    info!(
        elapsed_seconds,
        total_ticks,
        final_tick = world.tick().0,
        "applied and journaled unexpected-downtime catch-up"
    );
    Ok(())
}

fn utc_now_seconds() -> Result<i64, Box<dyn std::error::Error>> {
    Ok(i64::try_from(
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
    )?)
}

fn build_crafting_catalog(
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    materials: &MaterialRegistry,
    proficiencies: &ProficiencyRegistry,
) -> Result<CraftingCatalog, Box<dyn std::error::Error>> {
    let mut catalog = BTreeMap::new();
    for recipe in recipes.craftable_with_knowledge_source() {
        let result = items
            .get(&recipe.result)
            .ok_or("runnable recipe result disappeared from the item registry")?;
        if runtime_item_temperature_capability(result, materials)
            .map_or(true, |capability| capability.rot_shelf_life_turns.is_some())
            || recipe.byproducts.keys().any(|type_id| {
                items.get(type_id).is_none_or(|item| {
                    runtime_item_temperature_capability(item, materials)
                        .map_or(true, |capability| capability.rot_shelf_life_turns.is_some())
                })
            })
        {
            continue;
        }
        let amount = recipe
            .charges
            .unwrap_or(1)
            .checked_mul(recipe.result_mult)
            .ok_or("recipe output count overflow")?;
        let (output_instances, output_charges) = if result.count_by_charges() {
            (1, i32::try_from(amount)?)
        } else {
            (u16::try_from(amount)?, default_instance_charges(result))
        };
        let byproducts = recipe
            .byproducts
            .iter()
            .map(
                |(type_id, count)| -> Result<_, Box<dyn std::error::Error>> {
                    let item = items.get(type_id).ok_or_else(|| {
                        format!("runnable recipe {} lost byproduct {type_id}", recipe.id)
                    })?;
                    let (output_instances, charges) = if item.count_by_charges() {
                        let charges = item
                            .default_charges()
                            .checked_mul(i32::try_from(*count)?)
                            .ok_or("byproduct charge overflow")?;
                        (1, charges)
                    } else {
                        (u16::try_from(*count)?, default_instance_charges(item))
                    };
                    Ok(CraftByproductV1 {
                        output_instances,
                        output: craft_item_prototype(item, charges, items, materials)?,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        let components = recipes
            .resolved_components(recipe)?
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|component| {
                        let item = items.get(&component.type_id).ok_or_else(|| {
                            format!(
                                "runnable recipe {} has missing component {}",
                                recipe.id, component.type_id
                            )
                        })?;
                        Ok(CraftComponentRequirementV1 {
                            type_id: component.type_id,
                            count: component.count,
                            count_by_charges: item.count_by_charges(),
                            recoverable: !item.flags.contains("UNRECOVERABLE"),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tools = recipes
            .resolved_tools(recipe)?
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|tool| -> Result<_, Box<dyn std::error::Error>> {
                        if tool.requirement_list {
                            return Err(format!(
                                "runnable recipe {} retained unsupported tool semantics",
                                recipe.id
                            )
                            .into());
                        }
                        let amount = u16::try_from(tool.count.unsigned_abs())?;
                        Ok(CraftToolRequirementV1 {
                            type_id: tool.type_id,
                            amount,
                            consumes_charges: tool.count > 0,
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let qualities = recipes
            .resolved_qualities(recipe)?
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|quality| {
                        let providers = items
                            .iter()
                            .filter_map(|(type_id, item)| {
                                let inherent = (!item.unsupported_fields.contains("qualities")
                                    && item
                                        .qualities
                                        .get(&quality.quality_id)
                                        .is_some_and(|provided| provided.level >= quality.level))
                                .then_some(0);
                                let charged = (!item
                                    .unsupported_fields
                                    .contains("charged_qualities")
                                    && item
                                        .charged_qualities
                                        .get(&quality.quality_id)
                                        .is_some_and(|provided| provided.level >= quality.level))
                                .then(|| u16::try_from(item.charges_per_use).ok())
                                .flatten()
                                .filter(|charges| *charges > 0);
                                inherent
                                    .or(charged)
                                    .map(|minimum_charges| CraftQualityProviderV1 {
                                        type_id: type_id.to_owned(),
                                        minimum_charges,
                                    })
                            })
                            .collect::<Vec<_>>();
                        if providers.is_empty() {
                            return Err(format!(
                                "runnable recipe {} lost quality providers for {}",
                                recipe.id, quality.quality_id
                            )
                            .into());
                        }
                        Ok(CraftQualityRequirementV1 {
                            quality_id: quality.quality_id,
                            level: quality.level,
                            amount: u16::try_from(quality.amount)?,
                            providers,
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ammunition_type = single_ammunition_type(result)?;
        let primary_skill = (!recipe.skill_used.is_empty()).then(|| CraftSkillRequirementV1 {
            skill_id: recipe.skill_used.clone(),
            level: recipe.difficulty,
        });
        let required_skills = recipe
            .skills_required
            .iter()
            .map(|(skill_id, level)| CraftSkillRequirementV1 {
                skill_id: skill_id.clone(),
                level: *level,
            })
            .collect();
        let autolearn = recipe.autolearn;
        let autolearn_skills = if autolearn {
            recipe
                .resolved_autolearn_skills()
                .into_iter()
                .map(|(skill_id, level)| CraftSkillRequirementV1 { skill_id, level })
                .collect()
        } else {
            Vec::new()
        };
        let book_requirements = recipe
            .book_learn
            .iter()
            .map(|(book_type_id, metadata)| {
                let book = items.get(book_type_id).ok_or_else(|| {
                    format!(
                        "runnable recipe {} lost BOOK item {book_type_id}",
                        recipe.id
                    )
                })?;
                let required = if metadata.skill_level > 0 {
                    metadata.skill_level
                } else {
                    book.book_required_level
                        .max(i32::from(recipe.difficulty))
                };
                let required_skill_level = u8::try_from(required).map_err(|_| {
                    format!(
                        "runnable recipe {} has invalid BOOK threshold {required}",
                        recipe.id
                    )
                })?;
                if required_skill_level > MAX_SKILL_LEVEL {
                    return Err(format!(
                        "runnable recipe {} BOOK threshold {required_skill_level} exceeds canonical maximum {MAX_SKILL_LEVEL}",
                        recipe.id
                    ));
                }
                Ok(CraftBookRequirementV1 {
                    book_type_id: book_type_id.clone(),
                    required_skill_level,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut normalized_proficiencies = recipe
            .proficiencies
            .iter()
            .map(|recipe_proficiency| {
                let proficiency = proficiencies
                    .get(&recipe_proficiency.proficiency_id)
                    .ok_or_else(|| {
                        format!(
                            "runnable recipe {} lost proficiency {}",
                            recipe.id, recipe_proficiency.proficiency_id
                        )
                    })?;
                let time_multiplier_millionths = if recipe_proficiency.required {
                    0
                } else {
                    recipe_proficiency
                        .time_multiplier_millionths
                        .filter(|multiplier| *multiplier > 0)
                        .unwrap_or(proficiency.default_time_multiplier_millionths)
                };
                let skill_penalty_millionths = if recipe_proficiency.required {
                    0
                } else {
                    recipe_proficiency
                        .skill_penalty_millionths
                        .unwrap_or(proficiency.default_skill_penalty_millionths)
                };
                let to_action_points = |moves: u64| {
                    moves
                        .checked_mul(
                            u64::try_from(ACTION_POINTS_PER_UPSTREAM_MOVE)
                                .expect("positive action-point scale"),
                        )
                        .ok_or_else(|| String::from("proficiency action-point overflow"))
                };
                Ok::<_, String>(CraftProficiencyV1 {
                    proficiency_id: recipe_proficiency.proficiency_id.clone(),
                    required: recipe_proficiency.required,
                    time_multiplier_millionths,
                    skill_penalty_millionths,
                    learning_time_multiplier_millionths: recipe_proficiency
                        .learning_time_multiplier_millionths,
                    max_experience_action_points: recipe_proficiency
                        .max_experience_moves
                        .map(to_action_points)
                        .transpose()?,
                    time_to_learn_action_points: to_action_points(proficiency.time_to_learn_moves)?,
                    can_learn: proficiency.can_learn,
                    required_proficiencies: proficiency
                        .required_proficiencies
                        .iter()
                        .cloned()
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        normalized_proficiencies
            .sort_by(|left, right| left.proficiency_id.cmp(&right.proficiency_id));
        let definition = CraftRecipeV1 {
            recipe_id: recipe.id.clone(),
            time_moves: recipe.time_moves,
            output_instances,
            output: craft_item_prototype_with_ammunition(
                result,
                output_charges,
                ammunition_type,
                items,
                materials,
                false,
            )?,
            retain_components: recipe.reversible && !result.count_by_charges(),
            byproducts,
            components,
            tools,
            qualities,
            proficiencies: normalized_proficiencies,
            primary_skill,
            required_skills,
            autolearn,
            autolearn_skills,
            book_requirements,
            can_be_learned: !recipe.never_learn && !recipe.learn_by_disassembly.is_empty(),
        };
        if catalog.insert(recipe.id.clone(), definition).is_some() {
            return Err(format!("duplicate runnable recipe {}", recipe.id).into());
        }
    }
    Ok(CraftingCatalog::new(catalog))
}

fn build_reading_catalog(
    items: &ItemRegistry,
    skills: &SkillRegistry,
) -> Result<ReadingCatalog, Box<dyn std::error::Error>> {
    const MOVES_PER_MINUTE: u64 = 60 * 100;

    let mut catalog = BTreeMap::new();
    for (book_type_id, book) in items.iter().filter(|(_, item)| {
        item.subtypes.contains("BOOK")
            && !item.book_skill.is_empty()
            && item.book_max_level > item.book_required_level
            && item.book_time_moves > 0
    }) {
        let skill = skills.get(&book.book_skill).ok_or_else(|| {
            format!(
                "skill BOOK {book_type_id} references missing skill {}",
                book.book_skill
            )
        })?;
        if skill.tags.contains("contextual_skill") {
            return Err(format!(
                "skill BOOK {book_type_id} references contextual skill {}",
                book.book_skill
            )
            .into());
        }
        let required_skill_level = u8::try_from(book.book_required_level)
            .map_err(|_| format!("skill BOOK {book_type_id} has invalid required level"))?;
        let maximum_skill_level = u8::try_from(book.book_max_level)
            .map_err(|_| format!("skill BOOK {book_type_id} has invalid maximum level"))?;
        let intelligence_requirement = u16::try_from(book.book_intelligence)
            .map_err(|_| format!("skill BOOK {book_type_id} has invalid intelligence"))?;
        if maximum_skill_level > MAX_SKILL_LEVEL || book.book_time_moves % MOVES_PER_MINUTE != 0 {
            return Err(format!("skill BOOK {book_type_id} has unsupported study bounds").into());
        }
        if intelligence_requirement > MAX_ACTOR_BASE_STAT
            || adjusted_book_study_time_moves(book.book_time_moves, intelligence_requirement, 1)
                .is_none_or(|moves| moves > MAX_BOOK_STUDY_MOVES)
        {
            return Err(format!("skill BOOK {book_type_id} exceeds the study-time bound").into());
        }
        let source_time_minutes = u32::try_from(book.book_time_moves / MOVES_PER_MINUTE)?;
        let study = BookStudyV1 {
            book_type_id: book_type_id.to_owned(),
            skill_id: book.book_skill.clone(),
            required_skill_level,
            maximum_skill_level,
            intelligence_requirement,
            time_moves: book.book_time_moves,
            source_time_minutes,
        };
        if catalog.insert(book_type_id.to_owned(), study).is_some() {
            return Err(format!("duplicate readable BOOK {book_type_id}").into());
        }
    }
    Ok(ReadingCatalog::new(catalog))
}

fn build_construction_catalog(
    constructions: &ConstructionRegistry,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    skills: &SkillRegistry,
    terrain: &TerrainRegistry,
    furniture: &FurnitureRegistry,
) -> Result<ConstructionCatalog, Box<dyn std::error::Error>> {
    let mut catalog = BTreeMap::new();
    for (construction_id, construction) in constructions.iter() {
        let Some(group) = constructions.group(&construction.group) else {
            return Err(format!(
                "construction {construction_id} lost group {}",
                construction.group
            )
            .into());
        };
        let result = if let Some(result) = furniture.get(&construction.post_terrain) {
            ConstructionResultV1::Furniture(furniture_tile(result))
        } else if let Some(result) = terrain.get(&construction.post_terrain) {
            ConstructionResultV1::Terrain(terrain_tile(result, terrain)?)
        } else {
            continue;
        };
        let supported_target_predicate = match (
            construction.pre_terrain.is_empty(),
            construction.pre_special.as_slice(),
        ) {
            (true, [special]) => special == "check_empty",
            (false, []) => construction
                .pre_terrain
                .iter()
                .all(|id| terrain.get(id).is_some() || furniture.get(id).is_some()),
            _ => false,
        };
        let Ok(resolved_components) =
            recipes.resolved_component_groups(construction_id, &construction.components)
        else {
            continue;
        };
        if !construction.unsupported_fields.is_empty()
            || !group.unsupported_fields.is_empty()
            || construction.time_moves == 0
            || construction.activity_level != "LIGHT_EXERCISE"
            || resolved_components.is_empty()
            || !supported_target_predicate
            || construction
                .required_skills
                .iter()
                .any(|(skill_id, level)| {
                    *level > MAX_SKILL_LEVEL
                        || skills
                            .get(skill_id)
                            .is_none_or(|skill| skill.tags.contains("contextual_skill"))
                })
            || resolved_components.iter().any(|group| {
                group.is_empty()
                    || group.iter().any(|component| {
                        component.requirement_list
                            || component.count == 0
                            || items.get(&component.type_id).is_none()
                    })
            })
        {
            continue;
        }
        let components = resolved_components
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|component| {
                        let item = items
                            .get(&component.type_id)
                            .expect("strict construction component was checked above");
                        CraftComponentRequirementV1 {
                            type_id: component.type_id.clone(),
                            count: component.count,
                            count_by_charges: item.count_by_charges(),
                            recoverable: component.recoverable,
                        }
                    })
                    .collect()
            })
            .collect();
        let Some(qualities) = normalize_construction_qualities(&construction.qualities, items)
        else {
            continue;
        };
        let required_skills = construction
            .required_skills
            .iter()
            .map(|(skill_id, level)| CraftSkillRequirementV1 {
                skill_id: skill_id.clone(),
                level: *level,
            })
            .collect();
        let mut pre_terrain = construction.pre_terrain.clone();
        pre_terrain.sort();
        pre_terrain.dedup();
        let recipe = ConstructionRecipeV1 {
            construction_id: construction_id.to_owned(),
            name: group.name.clone(),
            time_moves: construction.time_moves,
            required_skills,
            components,
            qualities,
            pre_terrain,
            requires_empty: construction.pre_special.as_slice() == ["check_empty"],
            result,
        };
        if catalog.insert(construction_id.to_owned(), recipe).is_some() {
            return Err(format!("duplicate strict construction {construction_id}").into());
        }
    }
    Ok(ConstructionCatalog::new(catalog))
}

fn normalize_construction_qualities(
    groups: &[Vec<cdda_content::QualityRequirement>],
    items: &ItemRegistry,
) -> Option<Vec<Vec<CraftQualityRequirementV1>>> {
    if groups.len() > MAX_CRAFT_SUPPORT_GROUPS {
        return None;
    }
    groups
        .iter()
        .map(|group| {
            if group.is_empty() || group.len() > MAX_CRAFT_SUPPORT_ALTERNATIVES {
                return None;
            }
            group
                .iter()
                .map(|quality| {
                    let amount = u16::try_from(quality.amount).ok()?;
                    if amount == 0 || amount > 256 {
                        return None;
                    }
                    let providers = items
                        .iter()
                        .filter_map(|(type_id, item)| {
                            let inherent = (!item.unsupported_fields.contains("qualities")
                                && item
                                    .qualities
                                    .get(&quality.quality_id)
                                    .is_some_and(|provided| provided.level >= quality.level))
                            .then_some(0);
                            let charged = (!item.unsupported_fields.contains("charged_qualities")
                                && item
                                    .charged_qualities
                                    .get(&quality.quality_id)
                                    .is_some_and(|provided| provided.level >= quality.level))
                            .then(|| u16::try_from(item.charges_per_use).ok())
                            .flatten()
                            .filter(|charges| *charges > 0);
                            inherent
                                .or(charged)
                                .map(|minimum_charges| CraftQualityProviderV1 {
                                    type_id: type_id.to_owned(),
                                    minimum_charges,
                                })
                        })
                        .take(MAX_CRAFT_QUALITY_PROVIDERS + 1)
                        .collect::<Vec<_>>();
                    if providers.is_empty() || providers.len() > MAX_CRAFT_QUALITY_PROVIDERS {
                        return None;
                    }
                    Some(CraftQualityRequirementV1 {
                        quality_id: quality.quality_id.clone(),
                        level: quality.level,
                        amount,
                        providers,
                    })
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
}

fn build_disassembly_catalog(
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    materials: &MaterialRegistry,
    ammunition: &AmmunitionRegistry,
    crafting: &CraftingCatalog,
) -> Result<DisassemblyCatalog, Box<dyn std::error::Error>> {
    const SPECIAL_TOOL_SUBSTITUTIONS: [&str; 8] = [
        "welder",
        "welder_crude",
        "oxy_torch",
        "forge",
        "char_forge",
        "crucible",
        "press",
        "fire",
    ];
    const SPECIAL_QUALITY_SUBSTITUTIONS: [&str; 3] = ["SEW", "GLARE", "KNIT"];

    let mut catalog = BTreeMap::new();
    for (target_type_id, result) in items.iter() {
        let Some(recipe) =
            recipes.strict_disassembly_recipe_for_result(target_type_id, items, ammunition)
        else {
            continue;
        };
        if result.count_by_charges() || runtime_item_tracks_temperature(result, materials).is_err()
        {
            continue;
        }
        let unload_category = if result.subtypes.contains("GUN") {
            Some(
                result
                    .ammo
                    .first()
                    .ok_or_else(|| format!("gun {target_type_id} lost its ammunition type"))?,
            )
        } else if result.subtypes.contains("TOOL")
            && !result.tool_ammunition.is_empty()
            && result.default_charges() > 0
        {
            Some(
                result
                    .tool_ammunition
                    .first()
                    .ok_or_else(|| format!("tool {target_type_id} lost its charge-carrier type"))?,
            )
        } else {
            None
        };
        let unload_charges_as = if let Some(ammunition_type) = unload_category {
            let default_ammunition = ammunition
                .get(ammunition_type)
                .and_then(|definition| items.get(&definition.default_item))
                .ok_or_else(|| {
                    format!("item {target_type_id} lost its default charge-carrier item")
                })?;
            if runtime_item_tracks_temperature(default_ammunition, materials).is_err() {
                continue;
            }
            Some(craft_item_prototype(
                default_ammunition,
                default_ammunition.default_charges(),
                items,
                materials,
            )?)
        } else {
            None
        };
        let tools = normalize_recipe_tools(recipes, recipe)?;
        let qualities = normalize_recipe_qualities(recipes, recipe, items)?;
        if tools.iter().flatten().any(|tool| {
            tool.consumes_charges || SPECIAL_TOOL_SUBSTITUTIONS.contains(&tool.type_id.as_str())
        }) || qualities
            .iter()
            .flatten()
            .any(|quality| SPECIAL_QUALITY_SUBSTITUTIONS.contains(&quality.quality_id.as_str()))
        {
            continue;
        }
        let resolved = recipes.resolved_components(recipe)?;
        if resolved.is_empty() || resolved.iter().any(Vec::is_empty) {
            continue;
        }
        let mut components = Vec::new();
        let mut total_instances = 0_u16;
        let mut supported = true;
        for component in resolved
            .into_iter()
            .filter_map(|group| group.into_iter().next())
        {
            let Some(item) = items.get(&component.type_id) else {
                supported = false;
                break;
            };
            if !component.recoverable || item.flags.contains("UNRECOVERABLE") {
                continue;
            }
            if runtime_item_tracks_temperature(item, materials).is_err() {
                supported = false;
                break;
            }
            let (output_instances, charges) = if item.count_by_charges() {
                let Ok(charges) = i32::try_from(component.count) else {
                    supported = false;
                    break;
                };
                (1_u16, charges)
            } else {
                let Ok(instances) = u16::try_from(component.count) else {
                    supported = false;
                    break;
                };
                (instances, default_instance_charges(item))
            };
            let Some(next_total) = total_instances.checked_add(output_instances) else {
                supported = false;
                break;
            };
            if next_total > cdda_protocol::MAX_CRAFT_OUTPUT_INSTANCES {
                supported = false;
                break;
            }
            total_instances = next_total;
            components.push(DisassemblyComponentV1 {
                output_instances,
                count_by_charges: item.count_by_charges(),
                output: craft_item_prototype(item, charges, items, materials)?,
                output_state: None,
            });
        }
        if !supported {
            continue;
        }
        let learn_requirements = if recipe.never_learn {
            Vec::new()
        } else {
            recipe
                .learn_by_disassembly
                .iter()
                .map(|(skill_id, level)| {
                    Ok(CraftSkillRequirementV1 {
                        skill_id: skill_id.clone(),
                        level: u8::try_from(*level)?,
                    })
                })
                .collect::<Result<Vec<_>, std::num::TryFromIntError>>()?
        };
        if learn_requirements
            .iter()
            .any(|requirement| requirement.level > MAX_SKILL_LEVEL)
        {
            return Err(format!("disassembly recipe {} exceeds skill bounds", recipe.id).into());
        }
        let (autolearn, autolearn_requirements) = crafting
            .get(&recipe.id)
            .map(|craft| (craft.autolearn, craft.autolearn_skills.clone()))
            .unwrap_or((false, Vec::new()));
        let definition = DisassemblyRecipeV1 {
            recipe_id: recipe.id.clone(),
            target_type_id: recipe.result.clone(),
            time_moves: if recipe.uncraft_time_moves > 0 {
                recipe.uncraft_time_moves
            } else {
                recipe.time_moves
            },
            difficulty: recipe.difficulty,
            primary_skill_id: (!recipe.skill_used.is_empty()).then(|| recipe.skill_used.clone()),
            learn_requirements,
            autolearn,
            autolearn_requirements,
            unload_charges_as,
            requires_empty_charges: result.subtypes.contains("TOOL")
                && !result.subtypes.contains("GUN")
                && !result.tool_ammunition.is_empty()
                && result.default_charges() == 0
                && runtime_magazine_storage(result, items)?.1.is_empty(),
            components,
            tools,
            qualities,
        };
        if catalog
            .insert(target_type_id.to_owned(), definition)
            .is_some()
        {
            return Err(format!("duplicate disassembly target {target_type_id}").into());
        }
    }
    Ok(DisassemblyCatalog::new(catalog))
}

fn normalize_recipe_tools(
    recipes: &RecipeRegistry,
    recipe: &cdda_content::RecipeDefinition,
) -> Result<Vec<Vec<CraftToolRequirementV1>>, Box<dyn std::error::Error>> {
    recipes
        .resolved_tools(recipe)?
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|tool| -> Result<_, Box<dyn std::error::Error>> {
                    if tool.requirement_list {
                        return Err(format!(
                            "recipe {} retained unsupported tool semantics",
                            recipe.id
                        )
                        .into());
                    }
                    Ok(CraftToolRequirementV1 {
                        type_id: tool.type_id,
                        amount: u16::try_from(tool.count.unsigned_abs())?,
                        consumes_charges: tool.count > 0,
                    })
                })
                .collect()
        })
        .collect()
}

fn normalize_recipe_qualities(
    recipes: &RecipeRegistry,
    recipe: &cdda_content::RecipeDefinition,
    items: &ItemRegistry,
) -> Result<Vec<Vec<CraftQualityRequirementV1>>, Box<dyn std::error::Error>> {
    recipes
        .resolved_qualities(recipe)?
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|quality| -> Result<_, Box<dyn std::error::Error>> {
                    let providers = items
                        .iter()
                        .filter_map(|(type_id, item)| {
                            let inherent = (!item.unsupported_fields.contains("qualities")
                                && item
                                    .qualities
                                    .get(&quality.quality_id)
                                    .is_some_and(|provided| provided.level >= quality.level))
                            .then_some(0);
                            let charged = (!item.unsupported_fields.contains("charged_qualities")
                                && item
                                    .charged_qualities
                                    .get(&quality.quality_id)
                                    .is_some_and(|provided| provided.level >= quality.level))
                            .then(|| u16::try_from(item.charges_per_use).ok())
                            .flatten()
                            .filter(|charges| *charges > 0);
                            inherent
                                .or(charged)
                                .map(|minimum_charges| CraftQualityProviderV1 {
                                    type_id: type_id.to_owned(),
                                    minimum_charges,
                                })
                        })
                        .collect::<Vec<_>>();
                    if providers.is_empty() {
                        return Err(format!(
                            "recipe {} lost quality providers for {}",
                            recipe.id, quality.quality_id
                        )
                        .into());
                    }
                    Ok(CraftQualityRequirementV1 {
                        quality_id: quality.quality_id,
                        level: quality.level,
                        amount: u16::try_from(quality.amount)?,
                        providers,
                    })
                })
                .collect()
        })
        .collect()
}

fn craft_item_prototype(
    item: &ItemDefinition,
    charges: i32,
    items: &ItemRegistry,
    materials: &MaterialRegistry,
) -> Result<CraftItemPrototypeV1, Box<dyn std::error::Error>> {
    craft_item_prototype_with_ammunition(
        item,
        charges,
        single_ammunition_type(item)?,
        items,
        materials,
        false,
    )
}

fn craft_item_group_prototype(
    item: &ItemDefinition,
    charges: i32,
    items: &ItemRegistry,
    materials: &MaterialRegistry,
) -> Result<CraftItemPrototypeV1, Box<dyn std::error::Error>> {
    craft_item_prototype_with_ammunition(
        item,
        charges,
        single_ammunition_type(item)?,
        items,
        materials,
        true,
    )
}

fn default_instance_charges(item: &ItemDefinition) -> i32 {
    if item.strict_magazine().is_some() {
        0
    } else if item.subtypes.contains("TOOL") && !item.tool_ammunition.is_empty() {
        item.default_charges()
    } else {
        item.charges.max(1)
    }
}

fn craft_item_prototype_with_ammunition(
    item: &ItemDefinition,
    charges: i32,
    ammunition_type: String,
    items: &ItemRegistry,
    materials: &MaterialRegistry,
    allow_item_group_state: bool,
) -> Result<CraftItemPrototypeV1, Box<dyn std::error::Error>> {
    let (magazine_capacity, magazine_wells) = runtime_magazine_storage(item, items)?;
    let integral_magazines = runtime_integral_magazines(item);
    let ammunition_type = if integral_magazines.is_empty() {
        ammunition_type
    } else {
        String::new()
    };
    let charges = if integral_magazines.is_empty() && magazine_wells.is_empty() {
        charges
    } else {
        // Pocketed ammunition is represented only by the explicit integral or
        // detachable storage snapshots. Keeping the ordinary one-instance
        // sentinel here would describe aggregate charges beside that storage
        // and cannot round-trip through canonical item validation. This also
        // covers inherited wells on definitions whose derived subtype list no
        // longer includes TOOL.
        0
    };
    let temperature = runtime_item_temperature_capability(item, materials)?;
    if temperature.rot_shelf_life_turns.is_some() && !allow_item_group_state {
        return Err(format!(
            "item {} requires rot metadata unavailable at this constructor boundary",
            item.id
        )
        .into());
    }
    if !allow_item_group_state
        && item
            .spawn_pockets
            .iter()
            .any(|pocket| pocket.insulation_f32_bits != 1.0_f32.to_bits())
    {
        return Err(format!(
            "item {} requires pocket insulation metadata unavailable at this constructor boundary",
            item.id
        )
        .into());
    }
    Ok(CraftItemPrototypeV1 {
        type_id: item.id.clone(),
        charges,
        melee_damage_milli: item.melee_damage_milli()?,
        calories: item.calories,
        quench: item.quench,
        comestible_type: item.comestible_type.clone(),
        tracks_temperature: temperature.tracks_temperature,
        thermal_properties: temperature.thermal_properties,
        ammunition_type,
        ranged_weapon: None,
        magazine_capacity,
        integral_magazines,
        magazine_wells,
        ammunition_containers: runtime_ammunition_containers(item)?,
        residual_energy_millijoules: 0,
        powered_tool: runtime_powered_tool(item, items)?,
        containment: cdda_protocol::ItemContainmentProfileV1 {
            weight_milligrams: u64::try_from(item.weight_milligrams)?,
            volume_milliliters: u64::try_from(item.volume_milliliters)?,
            longest_side_millimeters: item
                .finalized_longest_side_millimeters()
                .ok_or("item longest-side derivation overflowed")?,
            flags: item.flags.iter().cloned().collect(),
            estorable: item.subtypes.contains("BOOK")
                || item.unsupported_fields.contains("ememory_size"),
            phase: match item.phase.to_ascii_lowercase().as_str() {
                "" | "solid" => cdda_protocol::ItemPhaseV1::Solid,
                "liquid" => cdda_protocol::ItemPhaseV1::Liquid,
                "gas" => cdda_protocol::ItemPhaseV1::Gas,
                phase => {
                    return Err(format!("item {} has unsupported phase {phase}", item.id).into());
                }
            },
            count_by_charges: item.count_by_charges(),
            stack_size: u32::try_from(item.stack_size)?,
        },
    })
}

fn runtime_magazine_storage(
    item: &ItemDefinition,
    items: &ItemRegistry,
) -> Result<(u32, Vec<MagazineWellPrototypeV1>), Box<dyn std::error::Error>> {
    if item.strict_magazine().is_some() {
        return Ok((0, Vec::new()));
    }
    if let Some(projection) = strict_detachable_battery_light(item, items)? {
        return Ok((
            0,
            vec![MagazineWellPrototypeV1 {
                pocket_index: projection.pocket_index,
                pocket_id: projection.pocket_id,
                compatible_magazine_type_ids: projection.compatible_magazine_type_ids,
                rigid: projection.rigid,
                unloadable: !item.flags.contains("NO_UNLOAD"),
            }],
        ));
    }
    let wells = strict_detachable_magazine_wells(item, items);
    if !wells.is_empty() {
        return Ok((0, wells));
    }
    Ok((0, Vec::new()))
}

fn runtime_integral_magazines(item: &ItemDefinition) -> Vec<IntegralMagazinePocketPrototypeV1> {
    item.pockets
        .iter()
        .filter_map(|pocket| {
            let restrictions = pocket.strict_integral_magazine()?;
            if restrictions.len() != 1 {
                return None;
            }
            let (ammunition_type, capacity) = restrictions.first_key_value()?;
            Some(IntegralMagazinePocketPrototypeV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                ammunition_type: ammunition_type.clone(),
                capacity: u32::try_from(*capacity).ok()?,
                rigid: pocket
                    .raw_fields
                    .get("rigid")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                reloadable: !item.flags.contains("NO_RELOAD"),
                unloadable: !item.flags.contains("NO_UNLOAD"),
            })
        })
        .collect()
}

#[cfg(test)]
fn strict_detachable_magazine_well(
    item: &ItemDefinition,
    items: &ItemRegistry,
) -> Option<MagazineWellPrototypeV1> {
    let wells = strict_detachable_magazine_wells(item, items);
    let [well] = wells.as_slice() else {
        return None;
    };
    Some(well.clone())
}

fn strict_detachable_magazine_wells(
    item: &ItemDefinition,
    items: &ItemRegistry,
) -> Vec<MagazineWellPrototypeV1> {
    if item.magazine_wells.is_empty()
        || !item.integral_magazines.is_empty()
        || item.magazine_capacity != 0
        || item.subtypes.contains("GUN")
        || item.subtypes.contains("MAGAZINE")
        || item.tool_ammunition.len() != 1
    {
        return Vec::new();
    }
    let Some(ammunition_type) = item.tool_ammunition.first() else {
        return Vec::new();
    };
    let mut normalized = Vec::with_capacity(item.magazine_wells.len());
    for well in &item.magazine_wells {
        let Some(pocket) = item
            .pockets
            .iter()
            .find(|pocket| pocket.pocket_index == well.pocket_index)
        else {
            return Vec::new();
        };
        let compatible = items.compatible_magazines(well);
        if !pocket.strict_magazine_well()
            || compatible.is_empty()
            || (!well.default_magazine.is_empty()
                && !compatible
                    .iter()
                    .any(|type_id| *type_id == well.default_magazine))
            || compatible.iter().any(|type_id| {
                items
                    .get(type_id)
                    .and_then(ItemDefinition::strict_magazine)
                    .is_none_or(|magazine| magazine.ammunition_type != *ammunition_type)
            })
        {
            return Vec::new();
        }
        normalized.push(MagazineWellPrototypeV1 {
            pocket_index: well.pocket_index,
            pocket_id: well.pocket_id.clone(),
            compatible_magazine_type_ids: compatible.into_iter().map(str::to_owned).collect(),
            rigid: pocket
                .raw_fields
                .get("rigid")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            unloadable: !item.flags.contains("NO_UNLOAD"),
        });
    }
    normalized
}

fn runtime_powered_tool(
    item: &ItemDefinition,
    items: &ItemRegistry,
) -> Result<Option<PoweredToolStateV1>, Box<dyn std::error::Error>> {
    Ok(strict_detachable_battery_light(item, items)?.map(|projection| projection.powered_tool))
}

#[derive(Debug, Eq, PartialEq)]
struct DetachableBatteryLightProjection {
    powered_tool: PoweredToolStateV1,
    pocket_index: u16,
    pocket_id: String,
    compatible_magazine_type_ids: Vec<String>,
    rigid: bool,
}

fn strict_battery_magazine_capacity(item: &ItemDefinition) -> Option<u32> {
    item.strict_magazine()
        .filter(|magazine| magazine.ammunition_type == "battery")
        .map(|magazine| magazine.capacity)
}

fn strict_detachable_battery_light(
    item: &ItemDefinition,
    items: &ItemRegistry,
) -> Result<Option<DetachableBatteryLightProjection>, Box<dyn std::error::Error>> {
    let (inactive, active) = if item.revert_to.is_empty() {
        let [action] = item.transform_actions.as_slice() else {
            return Ok(None);
        };
        let Some(active) = items.get(&action.target) else {
            return Ok(None);
        };
        (item, active)
    } else {
        let Some(inactive) = items.get(&item.revert_to) else {
            return Ok(None);
        };
        (inactive, item)
    };
    let [activate] = inactive.transform_actions.as_slice() else {
        return Ok(None);
    };
    let [deactivate] = active.transform_actions.as_slice() else {
        return Ok(None);
    };
    let [well] = inactive.magazine_wells.as_slice() else {
        return Ok(None);
    };
    let activation_charges = inactive
        .charges_per_use
        .checked_mul(activate.ammo_scale)
        .and_then(|charges| u16::try_from(charges).ok());
    let power_draw_milliwatts = u32::try_from(active.power_draw_milliwatts).ok();
    let light_emission = u16::try_from(active.light_emission).ok();
    let light_behavior_supported = light_emission.is_some_and(|emission| {
        cdda_sim::powered_light_is_personal_detail(emission)
            && cdda_sim::powered_light_sight_radius(emission) > 0
    });
    let runtime_properties_match = inactive.melee_damage_milli()? == active.melee_damage_milli()?
        && inactive.calories == active.calories
        && inactive.quench == active.quench
        && inactive.comestible_type == active.comestible_type
        && inactive.ammo_types == active.ammo_types
        && inactive.magazine_capacity == active.magazine_capacity;
    if !inactive.subtypes.contains("TOOL")
        || inactive.subtypes.contains("GUN")
        || !active.subtypes.contains("TOOL")
        || active.subtypes.contains("GUN")
        || inactive.tool_ammunition != BTreeSet::from([String::from("battery")])
        || active.tool_ammunition != inactive.tool_ammunition
        || !inactive.integral_magazines.is_empty()
        || active.integral_magazines != inactive.integral_magazines
        || active.magazine_wells != inactive.magazine_wells
        || inactive.has_non_transform_use_actions
        || active.has_non_transform_use_actions
        || inactive.has_unsupported_transform_action_fields
        || active.has_unsupported_transform_action_fields
        || !runtime_properties_match
        || inactive.power_draw_milliwatts != 0
        || inactive.light_emission != 0
        || !inactive.revert_to.is_empty()
        || activate.target != active.id
        || activate.need_charges <= 0
        || activate.need_charges != inactive.charges_per_use
        || activate.ammo_scale != 1
        || activate.moves != 0
        || active.revert_to != inactive.id
        || !light_behavior_supported
        || deactivate.target != inactive.id
        || deactivate.need_charges != 0
        || deactivate.ammo_scale != 0
        || deactivate.moves != 0
        || activation_charges.is_none_or(|charges| charges == 0)
        || power_draw_milliwatts.is_none_or(|draw| draw == 0)
        || light_emission.is_none_or(|light| light == 0)
    {
        return Ok(None);
    }
    let compatible = items.compatible_magazines(well);
    if compatible.is_empty()
        || !compatible
            .iter()
            .any(|type_id| *type_id == well.default_magazine)
        || compatible.iter().any(|type_id| {
            items
                .get(type_id)
                .and_then(strict_battery_magazine_capacity)
                .is_none()
        })
    {
        return Ok(None);
    }
    Ok(Some(DetachableBatteryLightProjection {
        powered_tool: PoweredToolStateV1 {
            inactive_type_id: inactive.id.clone(),
            active_type_id: active.id.clone(),
            activation_charges: activation_charges.expect("positive charge count was checked"),
            power_draw_milliwatts: power_draw_milliwatts.expect("positive draw was checked"),
            light_emission: light_emission.expect("positive light was checked"),
            dims_with_charge: active.flags.contains("CHARGEDIM"),
            power_pocket_index: well.pocket_index,
            active: item.id == active.id,
        },
        pocket_index: well.pocket_index,
        pocket_id: well.pocket_id.clone(),
        compatible_magazine_type_ids: compatible.into_iter().map(str::to_owned).collect(),
        rigid: inactive
            .pockets
            .iter()
            .find(|pocket| pocket.pocket_index == well.pocket_index)
            .and_then(|pocket| pocket.raw_fields.get("rigid"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }))
}

fn single_ammunition_type(item: &ItemDefinition) -> Result<String, Box<dyn std::error::Error>> {
    match item.ammo_types.len() {
        0 => Ok(String::new()),
        1 => item
            .ammo_types
            .first()
            .cloned()
            .ok_or_else(|| format!("item {} lost its ammunition type", item.id).into()),
        _ => Err(format!("item {} has ambiguous ammunition types", item.id).into()),
    }
}

fn ranged_stat_u16(value: f64, field: &str) -> Result<u16, Box<dyn std::error::Error>> {
    if !value.is_finite() || value <= 0.0 || value > f64::from(u16::MAX) {
        return Err(format!("starter ranged {field} is outside u16 bounds").into());
    }
    Ok(value.round() as u16)
}

fn firearm_sound_volume(
    gun: &ItemDefinition,
    ammunition: &ItemDefinition,
) -> Result<u16, Box<dyn std::error::Error>> {
    if !gun.subtypes.contains("GUN") || !ammunition.subtypes.contains("AMMO") {
        return Err("ranged loudness requires a gun and ammunition".into());
    }
    let gun_loudness = gun.loudness.unwrap_or(0);
    let ammunition_loudness = if let Some(loudness) = ammunition.loudness {
        loudness
    } else {
        let damage_loudness = ammunition.damage.values().try_fold(
            0.0,
            |total, damage| -> Result<_, Box<dyn std::error::Error>> {
                let total = total + (damage.amount + damage.armor_penetration) * 2.0;
                total
                    .is_finite()
                    .then_some(total)
                    .ok_or_else(|| "ammunition loudness is not finite".into())
            },
        )?;
        let derived = f64::from(
            ammunition
                .range
                .checked_mul(2)
                .ok_or("ammunition loudness range overflow")?,
        ) + damage_loudness;
        if !derived.is_finite() || derived < 0.0 || derived > f64::from(i32::MAX) {
            return Err("derived ammunition loudness is out of range".into());
        }
        derived.trunc() as i32
    };
    let total = gun_loudness
        .checked_add(ammunition_loudness)
        .ok_or("firearm loudness overflow")?
        .max(0);
    Ok(u16::try_from(total)?)
}

fn terrain_tile(
    definition: &TerrainDefinition,
    registry: &TerrainRegistry,
) -> Result<TerrainTileSnapshot, Box<dyn std::error::Error>> {
    type TransformBehavior = Option<(i32, bool, bool)>;
    let transform = |target: &str| -> Result<TransformBehavior, Box<dyn std::error::Error>> {
        if target.is_empty() {
            Ok(None)
        } else {
            let target = registry
                .get(target)
                .ok_or("terrain transform target disappeared after validation")?;
            Ok(Some((
                target.move_cost,
                target.flags.contains("TRANSPARENT"),
                target.flags.contains("FLAT"),
            )))
        }
    };
    // Interior-only transforms require canonical indoor/outdoor topology,
    // which the current generated-world slice does not yet model. Retain them
    // in content, but omit the runtime transform rather than opening a locked
    // door from the wrong side.
    let transform_is_admitted = !definition.flags.contains("OPENCLOSE_INSIDE");
    let open_id = if transform_is_admitted {
        definition.open.as_str()
    } else {
        ""
    };
    let close_id = if transform_is_admitted {
        definition.close.as_str()
    } else {
        ""
    };
    let open = transform(open_id)?;
    let close = transform(close_id)?;
    Ok(TerrainTileSnapshot {
        terrain_id: definition.id.clone(),
        move_cost: definition.move_cost,
        transparent: definition.flags.contains("TRANSPARENT"),
        flat: definition.flags.contains("FLAT"),
        open: open_id.to_owned(),
        open_move_cost: open.map(|transform| transform.0),
        open_transparent: open.map(|transform| transform.1),
        open_flat: open.map(|transform| transform.2),
        close: close_id.to_owned(),
        close_move_cost: close.map(|transform| transform.0),
        close_transparent: close.map(|transform| transform.1),
        close_flat: close.map(|transform| transform.2),
    })
}

fn runtime_terrain_bash_type(
    definition: &TerrainDefinition,
    profiles: &BashDamageProfileRegistry,
    fields: &FieldTypeRegistry,
    terrain: &TerrainRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
    item_groups: &ItemGroupRegistry,
    dynamic_floor_result: Option<&str>,
) -> Result<TerrainBashTypeV1, Box<dyn std::error::Error>> {
    let bash = definition
        .bash
        .as_ref()
        .ok_or_else(|| format!("terrain {} has no bash definition", definition.id))?;
    if !bash.is_fully_supported()
        || !bash.furniture_result.is_empty() && bash.furniture_result != "f_null"
    {
        return Err(format!(
            "terrain {} retains unsupported bash semantics",
            definition.id
        )
        .into());
    }
    let profile = profiles.get(&bash.profile).ok_or_else(|| {
        format!(
            "terrain {} has missing bash profile {}",
            definition.id, bash.profile
        )
    })?;
    let bash_multiplier_millionths = profile
        .multipliers_millionths
        .get("bash")
        .copied()
        .ok_or_else(|| format!("bash profile {} has no bash multiplier", bash.profile))?;
    let result_id = if bash.terrain_result == "t_null" {
        dynamic_floor_result.ok_or_else(|| {
            format!(
                "terrain {} needs unresolved dynamic floor repair",
                definition.id
            )
        })?
    } else {
        if dynamic_floor_result.is_some() {
            return Err(format!(
                "terrain {} supplied an unnecessary dynamic floor result",
                definition.id
            )
            .into());
        }
        &bash.terrain_result
    };
    let result_definition = terrain.get(result_id).ok_or_else(|| {
        format!(
            "terrain {} has missing bash result {}",
            definition.id, result_id
        )
    })?;
    Ok(TerrainBashTypeV1 {
        terrain_id: definition.id.clone(),
        str_min: bash.str_min,
        str_max: bash.str_max,
        str_min_blocked: bash.str_min_blocked,
        str_max_blocked: bash.str_max_blocked,
        str_min_supported: bash.str_min_supported,
        str_max_supported: bash.str_max_supported,
        bash_multiplier_millionths,
        result: terrain_tile(result_definition, terrain)?,
        drop_source: runtime_bash_item_group_source(
            bash,
            item_groups,
            item_group_content,
            "terrain",
            &definition.id,
        )?,
        hit_field: runtime_bash_field(bash.hit_field.as_ref(), fields, &definition.id)?,
        destroyed_field: runtime_bash_field(bash.destroyed_field.as_ref(), fields, &definition.id)?,
        sound: bash.sound.clone(),
        failure_sound: bash.failure_sound.clone(),
        sound_volume: bash.sound_volume,
        failure_sound_volume: bash.failure_sound_volume,
    })
}

fn runtime_furniture_bash_type(
    definition: &FurnitureDefinition,
    profiles: &BashDamageProfileRegistry,
    fields: &FieldTypeRegistry,
    furniture: &FurnitureRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
    item_groups: &ItemGroupRegistry,
) -> Result<FurnitureBashTypeV1, Box<dyn std::error::Error>> {
    let bash = definition
        .bash
        .as_ref()
        .ok_or_else(|| format!("furniture {} has no bash definition", definition.id))?;
    if !bash.is_fully_supported() || bash.terrain_result != "t_null" {
        return Err(format!(
            "furniture {} retains unsupported bash semantics",
            definition.id
        )
        .into());
    }
    let profile = profiles.get(&bash.profile).ok_or_else(|| {
        format!(
            "furniture {} has missing bash profile {}",
            definition.id, bash.profile
        )
    })?;
    let bash_multiplier_millionths = profile
        .multipliers_millionths
        .get("bash")
        .copied()
        .ok_or_else(|| format!("bash profile {} has no bash multiplier", bash.profile))?;
    let result = if bash.furniture_result == "f_null" {
        None
    } else {
        Some(furniture_tile(
            furniture.get(&bash.furniture_result).ok_or_else(|| {
                format!(
                    "furniture {} has missing bash result {}",
                    definition.id, bash.furniture_result
                )
            })?,
        ))
    };
    Ok(FurnitureBashTypeV1 {
        furniture_id: definition.id.clone(),
        str_min: bash.str_min,
        str_max: bash.str_max,
        str_min_blocked: bash.str_min_blocked,
        str_max_blocked: bash.str_max_blocked,
        str_min_supported: bash.str_min_supported,
        str_max_supported: bash.str_max_supported,
        bash_multiplier_millionths,
        result,
        drop_source: runtime_bash_item_group_source(
            bash,
            item_groups,
            item_group_content,
            "furniture",
            &definition.id,
        )?,
        hit_field: runtime_bash_field(bash.hit_field.as_ref(), fields, &definition.id)?,
        destroyed_field: runtime_bash_field(bash.destroyed_field.as_ref(), fields, &definition.id)?,
        sound: bash.sound.clone(),
        failure_sound: bash.failure_sound.clone(),
        sound_volume: bash.sound_volume,
        failure_sound_volume: bash.failure_sound_volume,
    })
}

fn runtime_furniture_bash_types(
    furniture: &FurnitureRegistry,
    profiles: &BashDamageProfileRegistry,
    fields: &FieldTypeRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
    item_groups: &ItemGroupRegistry,
) -> Result<Vec<FurnitureBashTypeV1>, Box<dyn std::error::Error>> {
    let mut admitted = furniture
        .iter()
        .filter(|definition| {
            furniture_bash_is_runtime_admitted(definition, item_groups, item_group_content)
        })
        .map(|definition| definition.id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let previous = admitted.clone();
        admitted.retain(|id| {
            let Some(bash) = furniture
                .get(id)
                .and_then(|definition| definition.bash.as_ref())
            else {
                return false;
            };
            bash.furniture_result == "f_null"
                || furniture
                    .get(&bash.furniture_result)
                    .is_some_and(|result| result.bash.is_none() || previous.contains(&result.id))
        });
        if admitted == previous {
            break;
        }
    }
    admitted
        .iter()
        .map(|id| {
            runtime_furniture_bash_type(
                furniture
                    .get(id)
                    .ok_or("admitted furniture bash definition disappeared")?,
                profiles,
                fields,
                furniture,
                item_group_content,
                item_groups,
            )
        })
        .collect()
}

fn runtime_smash_item_types(items: &ItemRegistry) -> Vec<SmashItemTypeV1> {
    items
        .iter()
        .filter_map(|(_id, definition)| {
            if definition.flags.contains("REDUCED_BASHING")
                || definition.subtypes.contains("GUN")
                || definition.magazine_capacity != 0
                || !definition.magazine_wells.is_empty()
                || !definition.integral_magazines.is_empty()
                || !definition.tool_ammunition.is_empty()
                || definition.charges > 0
                || definition.charges_per_use > 0
                || definition.power_draw_milliwatts != 0
            {
                // `item::attack_time` uses live weight and volume. Loaded
                // ammunition, detachable/integral magazines, and powered-tool
                // state therefore need an instance-derived profile rather than
                // this immutable empty ordinary-item projection. Guns are also
                // excluded because condition can alter their melee damage.
                return None;
            }
            let damage = definition.melee_damage_milli().ok()?;
            if damage
                .iter()
                .any(|(damage_type, amount)| damage_type != "bash" && *amount > 0)
            {
                return None;
            }
            let bash_milli = damage
                .get("bash")
                .copied()
                .filter(|amount| *amount >= 1_000 && *amount % 1_000 == 0)?;
            Some(SmashItemTypeV1 {
                item_type_id: definition.id.clone(),
                bash_damage: u16::try_from(bash_milli / 1_000).ok()?,
                attack_time_moves: definition.ordinary_attack_time_moves()?,
                melee_to_hit: i16::try_from(definition.melee_to_hit()).ok()?,
            })
        })
        .collect()
}

fn furniture_bash_is_runtime_admitted(
    definition: &FurnitureDefinition,
    item_groups: &ItemGroupRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
) -> bool {
    let Some(bash) = definition.bash.as_ref() else {
        return false;
    };
    let blocked_bounds = (bash.str_min_blocked == -1 && bash.str_max_blocked == -1)
        || (bash.str_min_blocked >= 0
            && bash.str_max_blocked >= bash.str_min_blocked
            && bash.str_max_blocked <= i32::from(u16::MAX));
    let ordinary_bounds = bash.str_min >= 0
        && bash.str_max >= bash.str_min
        && bash.str_max <= i32::from(u16::MAX)
        && blocked_bounds
        && bash.str_min_supported == -1
        && bash.str_max_supported == -1;
    let modeled_fields = bash
        .hit_field
        .iter()
        .chain(bash.destroyed_field.iter())
        .all(|effect| matches!(effect.field_type_id.as_str(), "fd_dust" | "fd_splinters"));
    let bounded_drops = runtime_bash_item_group_source(
        bash,
        item_groups,
        item_group_content,
        "furniture",
        &definition.id,
    )
    .is_ok();
    bash.is_fully_supported()
        && ordinary_bounds
        && modeled_fields
        && bounded_drops
        && bash.terrain_result == "t_null"
        && bash.sound_volume <= i32::from(u16::MAX)
        && bash.failure_sound_volume <= i32::from(u16::MAX)
}

fn runtime_bash_field(
    effect: Option<&BashFieldEffectDefinition>,
    fields: &FieldTypeRegistry,
    owner_id: &str,
) -> Result<Option<BashFieldEffectV1>, Box<dyn std::error::Error>> {
    effect
        .map(|effect| {
            let definition = fields.get(&effect.field_type_id).ok_or_else(|| {
                format!(
                    "bash definition {owner_id} references missing field {}",
                    effect.field_type_id
                )
            })?;
            if usize::from(effect.intensity) > definition.intensity_levels.len() {
                return Err(format!(
                    "bash definition {owner_id} exceeds field {} intensity",
                    effect.field_type_id
                )
                .into());
            }
            Ok(BashFieldEffectV1 {
                field_type_id: effect.field_type_id.clone(),
                intensity: effect.intensity,
            })
        })
        .transpose()
}

fn runtime_field_type(
    definition: &FieldTypeDefinition,
) -> Result<FieldTypeSnapshotV1, Box<dyn std::error::Error>> {
    if definition.intensity_levels.is_empty() || definition.intensity_levels.len() > 16 {
        return Err(format!(
            "field type {} has an unsupported intensity count",
            definition.id
        )
        .into());
    }
    Ok(FieldTypeSnapshotV1 {
        field_type_id: definition.id.clone(),
        intensity_levels: definition
            .intensity_levels
            .iter()
            .map(|level| FieldIntensityLevelV1 {
                name: level.name.clone(),
                symbol: level.symbol.clone(),
                color: level.color.clone(),
                dangerous: level.dangerous,
                transparent: level.transparent,
            })
            .collect(),
        priority: definition.priority,
        half_life_seconds: definition.half_life_seconds,
        linear_half_life: definition.linear_half_life,
        is_splattering: definition.is_splattering,
        display_field: definition.display_field,
    })
}

fn monster_blood_field_type(monster: &MonsterDefinition) -> &'static str {
    if monster.flags.contains("ACID_BLOOD") {
        "fd_acid"
    } else if monster.flags.contains("BILE_BLOOD") {
        "fd_bile"
    } else if monster.flags.contains("ARTHROPOD_BLOOD") {
        "fd_blood_invertebrate"
    } else if monster.materials.contains("veggy") || monster.flags.contains("PLANT_BLOOD") {
        "fd_blood_veggy"
    } else if monster.materials.contains("iflesh") {
        "fd_blood_insect"
    } else if monster.flags.contains("WARM") && monster.materials.contains("flesh") {
        "fd_blood"
    } else {
        ""
    }
}

const fn monster_size_from_volume(volume_milliliters: i64) -> CreatureSizeV1 {
    if volume_milliliters <= 7_500 {
        CreatureSizeV1::Tiny
    } else if volume_milliliters <= 46_250 {
        CreatureSizeV1::Small
    } else if volume_milliliters <= 108_000 {
        CreatureSizeV1::Medium
    } else if volume_milliliters <= 483_750 {
        CreatureSizeV1::Large
    } else {
        CreatureSizeV1::Huge
    }
}

fn monster_size(monster: &MonsterDefinition) -> CreatureSizeV1 {
    monster_size_from_volume(monster.volume_milliliters)
}

fn monster_attack_cost(monster: &MonsterDefinition) -> Result<u16, Box<dyn std::error::Error>> {
    let attack_cost = u16::try_from(monster.attack_cost_moves).map_err(|_| {
        format!(
            "MONSTER {} has attack_cost outside the authoritative runtime range",
            monster.id
        )
    })?;
    if attack_cost == 0 {
        return Err(format!("MONSTER {} has zero attack_cost", monster.id).into());
    }
    Ok(attack_cost)
}

fn validate_monster_attack_costs(
    monsters: &MonsterRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    for (_id, monster) in monsters.iter() {
        monster_attack_cost(monster)?;
    }
    Ok(())
}

fn monster_path_settings(
    monster: &MonsterDefinition,
) -> Result<CreaturePathSettingsV1, Box<dyn std::error::Error>> {
    let max_distance = u16::try_from(monster.path_settings.max_distance)?;
    if max_distance > 400 {
        return Err(format!("MONSTER {} has unsupported path settings", monster.id).into());
    }
    Ok(CreaturePathSettingsV1 {
        max_distance,
        allow_open_doors: monster.path_settings.allow_open_doors,
        avoid_traps: monster.path_settings.avoid_traps,
        avoid_sharp: monster.path_settings.avoid_sharp,
        avoid_dangerous_fields: monster.path_settings.avoid_dangerous_fields,
        allow_climb_stairs: monster.path_settings.allow_climb_stairs,
    })
}

fn furniture_tile(definition: &FurnitureDefinition) -> FurnitureTileSnapshot {
    FurnitureTileSnapshot {
        furniture_id: definition.id.clone(),
        move_cost_mod: definition.move_cost_mod,
        transparent: definition.is_transparent(),
        blocks_door: definition.flags.contains("BLOCKSDOOR"),
        comfort: definition.comfort,
        floor_bedding_warmth: definition.floor_bedding_warmth,
    }
}

async fn process_character_creation(
    request: CharacterCreationRequest,
    host: &SimulationHost,
    simulation: &SimulationHandle,
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let account_id = request.account_id();
    let name = request.name().to_owned();
    let base_stats = request.base_stats();
    let creation_simulation = simulation.clone();
    let spawned = match tokio::task::spawn_blocking(move || {
        creation_simulation.begin_actor_creation(base_stats, Duration::from_secs(1))
    })
    .await
    {
        Ok(Ok(spawned)) => spawned,
        Ok(Err(error)) => {
            request.complete(Err(CharacterCreationError::Simulation(error.to_string())));
            return Ok(());
        }
        Err(error) => {
            request.complete(Err(CharacterCreationError::Simulation(error.to_string())));
            return Ok(());
        }
    };

    if let Err(error) = drain_outputs(host, persistence, pending, journal_sequence)
        .and_then(|()| flush_journal(persistence, pending, journal_sequence))
    {
        let rollback_simulation = simulation.clone();
        let actor_id = spawned.actor.id;
        let _rollback = tokio::task::spawn_blocking(move || {
            rollback_simulation.complete_actor_creation(actor_id, false, Duration::from_secs(1))
        })
        .await;
        request.complete(Err(CharacterCreationError::Simulation(error.to_string())));
        return Err(error);
    }

    let character_persistence = persistence.clone();
    let created_tick = spawned.created_tick;
    let created_after_journal_sequence = *journal_sequence;
    let actor = spawned.actor.clone();
    let result = tokio::task::spawn_blocking(move || {
        character_persistence.create_character(
            account_id,
            name,
            created_tick,
            created_after_journal_sequence,
            actor,
        )
    })
    .await
    .map_err(|error| error.to_string())?;
    let committed = result.is_ok();
    let actor_id = spawned.actor.id;
    let completion_simulation = simulation.clone();
    let completion = tokio::task::spawn_blocking(move || {
        completion_simulation.complete_actor_creation(actor_id, committed, Duration::from_secs(1))
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result.map_err(|error| error.to_string()));
    if let Err(error) = completion {
        request.complete(Err(CharacterCreationError::Simulation(error.clone())));
        return Err(error.into());
    }
    request.complete(match result {
        Ok(_character) => Ok(actor_id),
        Err(error) => Err(CharacterCreationError::Persistence(error)),
    });
    Ok(())
}

fn capture_checkpoint(
    host: &SimulationHost,
    simulation: &SimulationHandle,
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
) -> Result<cdda_protocol::WorldSnapshotV1, Box<dyn std::error::Error>> {
    let mut snapshot = simulation.begin_checkpoint(Duration::from_secs(1))?;
    let capture = (|| {
        drain_outputs(host, persistence, pending, journal_sequence)?;
        flush_journal(persistence, pending, journal_sequence)?;
        let remaining = snapshot
            .allocator_reserved_end
            .saturating_sub(snapshot.allocator_next)
            .saturating_add(1);
        if remaining <= ID_REFILL_THRESHOLD {
            let at_tick = snapshot.tick;
            let prior_high_water = snapshot.allocator_high_water;
            let block = persistence.reserve_id_block()?;
            simulation.install_reserved_block(block, Duration::from_secs(1))?;
            snapshot = simulation.snapshot(Duration::from_secs(1))?;
            *journal_sequence = persistence.append_journal_batch_at(
                JournalBatchV1 {
                    ticks: Vec::new(),
                    allocator_inputs: vec![
                        AllocatorInputV1::IdBlockAbandoned {
                            at_tick,
                            high_water: prior_high_water,
                        },
                        AllocatorInputV1::IdBlockReserved { at_tick, block },
                    ],
                },
                utc_now_seconds()?,
            )?;
        }
        Ok(snapshot)
    })();
    let resume = simulation.complete_checkpoint(Duration::from_secs(1));
    match (capture, resume) {
        (Ok(snapshot), Ok(())) => Ok(snapshot),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error.into()),
    }
}

fn queue_checkpoint_world(
    host: &SimulationHost,
    simulation: &SimulationHandle,
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
) -> Result<SnapshotReceipt, Box<dyn std::error::Error>> {
    let snapshot = capture_checkpoint(host, simulation, persistence, pending, journal_sequence)?;
    Ok(persistence.queue_snapshot(*journal_sequence, snapshot)?)
}

fn drain_snapshot_results(
    snapshots: &mut VecDeque<SnapshotReceipt>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = 0;
    while index < snapshots.len() {
        if snapshots[index].try_result()?.is_some() {
            snapshots.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn start_replay_archive(
    replay_directory: PathBuf,
    snapshot_object_directory: PathBuf,
    prepared: PreparedReplayArchive,
) -> Result<ReplayArchiveTask, std::io::Error> {
    let thread = thread::Builder::new()
        .name(String::from("cdda-replay-archive"))
        .spawn(move || {
            write_replay_archive(&replay_directory, &snapshot_object_directory, prepared)
        })?;
    Ok(ReplayArchiveTask { thread })
}

fn poll_replay_archive(
    task: &mut Option<ReplayArchiveTask>,
    persistence: &PersistenceHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    if task.as_ref().is_some_and(|task| task.thread.is_finished()) {
        finish_replay_archive(task, persistence)?;
    }
    Ok(())
}

fn finish_replay_archive(
    task: &mut Option<ReplayArchiveTask>,
    persistence: &PersistenceHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(task) = task.take() else {
        return Ok(());
    };
    match task.thread.join() {
        Ok(Ok(archive)) => {
            persistence.commit_replay_archive(archive.start, archive.end)?;
            if let Some(compaction) =
                persistence.compact_recovery_history(archive.end.archived_utc_seconds)?
            {
                info!(
                    through_sequence = compaction.through_journal_sequence,
                    deleted_journal_batches = compaction.deleted_journal_batches,
                    deleted_snapshots = compaction.deleted_snapshots,
                    "compacted recovery history behind verified archive snapshot"
                );
            }
            info!(
                path = %archive.path.display(),
                snapshot_object_path = %archive.snapshot_object_path.display(),
                snapshot_object_hash = %blake3::Hash::from_bytes(archive.snapshot_object_hash),
                retained_replay_archives = archive.snapshot_gc.retained_archives,
                retained_snapshot_objects = archive.snapshot_gc.retained_objects,
                removed_snapshot_objects = archive.snapshot_gc.removed_objects,
                start_sequence = archive.start.journal_sequence,
                end_sequence = archive.end.journal_sequence,
                final_tick = archive.final_tick,
                encoded_bytes = archive.encoded_bytes,
                checksum = %blake3::Hash::from_bytes(archive.checksum),
                "verified hourly replay archive committed"
            );
        }
        Ok(Err(error)) => {
            warn!(%error, "hourly replay archive failed; the durable cursor was not advanced");
        }
        Err(_panic) => {
            return Err("hourly replay archive worker panicked".into());
        }
    }
    Ok(())
}

fn start_backup(
    directory: PathBuf,
    persistence: PersistenceHandle,
    secret_key_bytes: [u8; 32],
    content: ContentIdentity,
    now_utc_seconds: i64,
) -> Result<BackupTask, std::io::Error> {
    let thread = thread::Builder::new()
        .name(String::from("cdda-online-backup"))
        .spawn(move || {
            write_backup_generation(
                &directory,
                &persistence,
                secret_key_bytes,
                &content,
                now_utc_seconds,
            )
        })?;
    Ok(BackupTask { thread })
}

fn poll_backup(
    task: &mut Option<BackupTask>,
    last_backup_utc: &mut Option<i64>,
) -> Result<(), Box<dyn std::error::Error>> {
    if task.as_ref().is_some_and(|task| task.thread.is_finished()) {
        finish_backup(task, last_backup_utc)?;
    }
    Ok(())
}

fn finish_backup(
    task: &mut Option<BackupTask>,
    last_backup_utc: &mut Option<i64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(task) = task.take() else {
        return Ok(());
    };
    match task.thread.join() {
        Ok(Ok(backup)) => {
            *last_backup_utc = Some(backup.created_utc_seconds);
            info!(
                path = %backup.path.display(),
                schema_version = backup.metadata.schema_version,
                journal_sequence = backup.metadata.journal_sequence,
                tick = backup.metadata.tick.0,
                database_checksum = %blake3::Hash::from_bytes(backup.database_checksum),
                "verified online backup committed"
            );
        }
        Ok(Err(error)) => {
            warn!(%error, "online backup failed; no generation was committed");
        }
        Err(_panic) => return Err("online backup worker panicked".into()),
    }
    Ok(())
}

fn write_backup_generation(
    directory: &Path,
    persistence: &PersistenceHandle,
    secret_key_bytes: [u8; 32],
    content: &ContentIdentity,
    now_utc_seconds: i64,
) -> Result<BackupWrite, ReplayArchiveError> {
    if now_utc_seconds <= 0 {
        return Err("backup UTC must be positive".into());
    }
    prepare_private_directory(directory)?;
    let temporary_directory = create_private_backup_temp(directory, now_utc_seconds)?;
    let result = (|| -> Result<BackupWrite, ReplayArchiveError> {
        let database_path = temporary_directory.join("world.db");
        let metadata = persistence.backup_to(database_path.clone())?;
        let identity_path = temporary_directory.join("server-identity.key");
        write_private_file(&identity_path, &secret_key_bytes)?;

        let secret_key = iroh::SecretKey::from_bytes(&secret_key_bytes);
        let database_checksum = hash_regular_file(&database_path)?;
        let identity_checksum = *blake3::hash(&secret_key_bytes).as_bytes();
        let manifest = BackupManifestV1 {
            format_version: BACKUP_FORMAT_VERSION,
            created_utc_seconds: now_utc_seconds,
            baseline_commit: BASELINE_COMMIT.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            schema_version: metadata.schema_version,
            content: content.clone(),
            server_endpoint_id: secret_key.public().to_string(),
            database_checksum: blake3::Hash::from_bytes(database_checksum).to_string(),
            identity_checksum: blake3::Hash::from_bytes(identity_checksum).to_string(),
            world_namespace: metadata.world_namespace,
            journal_sequence: metadata.journal_sequence,
            tick: metadata.tick.0,
            state_hash: blake3::Hash::from_bytes(metadata.state_hash).to_string(),
        };
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
        if manifest_bytes.len() as u64 > MAX_BACKUP_MANIFEST_BYTES {
            return Err("backup manifest exceeds 1 MiB".into());
        }
        write_private_file(&temporary_directory.join("manifest.json"), &manifest_bytes)?;
        sync_directory(&temporary_directory)?;
        let verified = verify_backup_generation(
            &temporary_directory,
            content,
            &secret_key.public().to_string(),
        )?;
        if verified != manifest {
            return Err("backup manifest changed during verification".into());
        }

        let final_path = directory.join(backup_generation_name(
            now_utc_seconds,
            metadata.journal_sequence,
        ));
        publish_backup_generation(&temporary_directory, &final_path)?;
        prune_backup_generations(
            directory,
            &final_path,
            content,
            &secret_key.public().to_string(),
        )?;
        Ok(BackupWrite {
            created_utc_seconds: now_utc_seconds,
            path: final_path,
            metadata,
            database_checksum,
        })
    })();
    if result.is_err() {
        let _cleanup = fs::remove_dir_all(&temporary_directory);
    }
    result
}

fn publish_backup_generation(
    temporary_directory: &Path,
    final_path: &Path,
) -> Result<(), std::io::Error> {
    fs::create_dir(final_path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(final_path, fs::Permissions::from_mode(0o700))?;
    }
    let result = (|| -> Result<(), std::io::Error> {
        for member in ["world.db", "server-identity.key"] {
            fs::rename(temporary_directory.join(member), final_path.join(member))?;
        }
        sync_directory(final_path)?;
        fs::rename(
            temporary_directory.join("manifest.json"),
            final_path.join("manifest.json"),
        )?;
        sync_directory(final_path)?;
        fs::remove_dir(temporary_directory)?;
        let parent = final_path
            .parent()
            .ok_or_else(|| std::io::Error::other("backup generation has no parent"))?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _cleanup = fs::remove_dir_all(final_path);
    }
    result
}

fn load_content_identity(
    manifest_path: &Path,
) -> Result<ContentIdentity, Box<dyn std::error::Error>> {
    let manifest = ContentManifest::load(manifest_path)?;
    let root = manifest_path
        .parent()
        .ok_or("content manifest has no parent directory")?;
    manifest.verify_files(root)?;
    let catalog = ModCatalog::load(&manifest, root)?;
    Ok(ContentIdentity {
        baseline_commit: BASELINE_COMMIT.to_owned(),
        manifest_hash: manifest.canonical_hash()?,
        enabled_mods: catalog.recommended_new_world()?,
    })
}

fn restore_backup_generation(
    backup_directory: &Path,
    world_directory: &Path,
    expected_content: &ContentIdentity,
) -> Result<BackupManifestV1, ReplayArchiveError> {
    if fs::symlink_metadata(world_directory).is_ok() {
        return Err("restore destination already exists".into());
    }
    let untrusted_manifest = read_backup_manifest(backup_directory)?;
    let manifest = verify_backup_generation(
        backup_directory,
        expected_content,
        &untrusted_manifest.server_endpoint_id,
    )?;
    let parent = world_directory
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary_directory = create_private_restore_temp(parent)?;
    let result = (|| -> Result<(), ReplayArchiveError> {
        copy_private_file(
            &backup_directory.join("world.db"),
            &temporary_directory.join("world.db"),
        )?;
        copy_private_file(
            &backup_directory.join("server-identity.key"),
            &temporary_directory.join("server-identity.key"),
        )?;
        copy_private_file(
            &backup_directory.join("manifest.json"),
            &temporary_directory.join("manifest.json"),
        )?;
        sync_directory(&temporary_directory)?;
        let copied = verify_backup_generation(
            &temporary_directory,
            expected_content,
            &manifest.server_endpoint_id,
        )?;
        if copied != manifest {
            return Err("copied restore generation differs from its source".into());
        }
        if fs::symlink_metadata(world_directory).is_ok() {
            return Err("restore destination appeared concurrently".into());
        }
        fs::rename(&temporary_directory, world_directory)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _cleanup = fs::remove_dir_all(&temporary_directory);
    }
    result?;
    Ok(manifest)
}

fn verify_restored_world_identity(
    world_directory: &Path,
    expected_content: &ContentIdentity,
    secret_key_bytes: [u8; 32],
    expected_endpoint_id: &str,
) -> Result<(), ReplayArchiveError> {
    let restore_manifest = world_directory.join("manifest.json");
    let provenance_path = world_directory.join(RESTORE_PROVENANCE_FILE);
    match fs::symlink_metadata(&restore_manifest) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err("restore manifest is not a regular file".into());
            }
            let verified =
                verify_backup_generation(world_directory, expected_content, expected_endpoint_id)?;
            match fs::symlink_metadata(&provenance_path) {
                Ok(metadata) => {
                    if !metadata.file_type().is_file()
                        || read_backup_manifest_file(&provenance_path)? != verified
                    {
                        return Err("restore provenance conflicts with its manifest".into());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::hard_link(&restore_manifest, &provenance_path)?;
                }
                Err(error) => return Err(Box::new(error)),
            }
            fs::remove_file(&restore_manifest)?;
            sync_directory(world_directory)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Box::new(error)),
    }
    let provenance = match fs::symlink_metadata(&provenance_path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err("restore provenance is not a regular file".into());
            }
            read_backup_manifest_file(&provenance_path)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Box::new(error)),
    };
    if provenance.format_version != BACKUP_FORMAT_VERSION
        || provenance.baseline_commit != BASELINE_COMMIT
        || provenance.protocol_version == 0
        || provenance.protocol_version > PROTOCOL_VERSION
        || provenance.schema_version < MIN_RECOVERABLE_SCHEMA_VERSION
        || provenance.schema_version > SCHEMA_VERSION
        || &provenance.content != expected_content
        || provenance.server_endpoint_id != expected_endpoint_id
        || provenance.identity_checksum != blake3::hash(&secret_key_bytes).to_string()
        || iroh::SecretKey::from_bytes(&secret_key_bytes)
            .public()
            .to_string()
            != expected_endpoint_id
    {
        return Err("restored world provenance does not match its server identity".into());
    }
    Ok(())
}

fn create_private_restore_temp(parent: &Path) -> Result<PathBuf, std::io::Error> {
    for _attempt in 0..16 {
        let path = parent.join(format!(
            ".cdda-restore.tmp-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate restore temp directory",
    ))
}

fn copy_private_file(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::other(
            "restore source is not a regular file",
        ));
    }
    let mut source_file = fs::File::open(source)?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut destination_file = options.open(destination)?;
    std::io::copy(&mut source_file, &mut destination_file)?;
    destination_file.sync_all()
}

fn create_private_backup_temp(
    directory: &Path,
    now_utc_seconds: i64,
) -> Result<PathBuf, std::io::Error> {
    for _attempt in 0..16 {
        let path = directory.join(format!(
            ".backup-{now_utc_seconds:020}.tmp-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate backup temp directory",
    ))
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn hash_regular_file(path: &Path) -> Result<[u8; 32], std::io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::other("backup member is not a regular file"));
    }
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn verify_backup_generation(
    directory: &Path,
    expected_content: &ContentIdentity,
    expected_endpoint_id: &str,
) -> Result<BackupManifestV1, ReplayArchiveError> {
    let manifest =
        verify_backup_generation_header(directory, expected_content, expected_endpoint_id)?;
    let database_path = directory.join("world.db");
    let database_checksum = hash_regular_file(&database_path)?;
    if blake3::Hash::from_bytes(database_checksum).to_string() != manifest.database_checksum {
        return Err("backup database checksum does not match its manifest".into());
    }
    let state_hash = parse_blake3_hash(&manifest.state_hash)?;
    let metadata = DatabaseBackupMetadata {
        schema_version: manifest.schema_version,
        world_namespace: manifest.world_namespace,
        journal_sequence: manifest.journal_sequence,
        tick: SimTick(manifest.tick),
        state_hash,
    };
    WorldStore::verify_backup(&database_path, metadata)?;
    Ok(manifest)
}

fn verify_backup_generation_header(
    directory: &Path,
    expected_content: &ContentIdentity,
    expected_endpoint_id: &str,
) -> Result<BackupManifestV1, ReplayArchiveError> {
    let directory_metadata = fs::symlink_metadata(directory)?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err("backup generation is not a real directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if directory_metadata.permissions().mode() & 0o077 != 0 {
            return Err("backup generation is accessible to group or other users".into());
        }
    }
    let manifest = read_backup_manifest(directory)?;
    if let Some(name) = directory
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .and_then(parse_backup_generation_name)
        && name != (manifest.created_utc_seconds, manifest.journal_sequence)
    {
        return Err("backup generation name disagrees with its manifest".into());
    }
    if manifest.format_version != BACKUP_FORMAT_VERSION
        || manifest.created_utc_seconds <= 0
        || manifest.baseline_commit != BASELINE_COMMIT
        || manifest.protocol_version == 0
        || manifest.protocol_version > PROTOCOL_VERSION
        || manifest.schema_version < MIN_RECOVERABLE_SCHEMA_VERSION
        || manifest.schema_version > SCHEMA_VERSION
        || &manifest.content != expected_content
        || manifest.server_endpoint_id != expected_endpoint_id
    {
        return Err("backup manifest identity does not match this server".into());
    }

    let identity_path = directory.join("server-identity.key");
    let identity_metadata = fs::symlink_metadata(&identity_path)?;
    if !identity_metadata.file_type().is_file() || identity_metadata.len() != 32 {
        return Err("backup identity bundle is not an exact regular key file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if identity_metadata.permissions().mode() & 0o077 != 0 {
            return Err("backup identity bundle is accessible to group or other users".into());
        }
    }
    let identity_bytes: [u8; 32] = fs::read(&identity_path)?
        .try_into()
        .map_err(|_| "backup identity key length changed")?;
    let identity_checksum = blake3::hash(&identity_bytes).to_string();
    let derived_endpoint = iroh::SecretKey::from_bytes(&identity_bytes)
        .public()
        .to_string();
    if identity_checksum != manifest.identity_checksum
        || derived_endpoint != manifest.server_endpoint_id
        || derived_endpoint != expected_endpoint_id
    {
        return Err("backup identity bundle does not match its endpoint manifest".into());
    }
    let database_path = directory.join("world.db");
    if !fs::symlink_metadata(database_path)?.file_type().is_file() {
        return Err("backup database is not a regular file".into());
    }
    Ok(manifest)
}

fn read_backup_manifest(directory: &Path) -> Result<BackupManifestV1, ReplayArchiveError> {
    read_backup_manifest_file(&directory.join("manifest.json"))
}

fn read_backup_manifest_file(path: &Path) -> Result<BackupManifestV1, ReplayArchiveError> {
    let manifest_metadata = fs::symlink_metadata(path)?;
    if !manifest_metadata.file_type().is_file()
        || manifest_metadata.len() > MAX_BACKUP_MANIFEST_BYTES
    {
        return Err("backup manifest is not a bounded regular file".into());
    }
    let mut manifest_bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BACKUP_MANIFEST_BYTES + 1)
        .read_to_end(&mut manifest_bytes)?;
    if manifest_bytes.len() as u64 > MAX_BACKUP_MANIFEST_BYTES {
        return Err("backup manifest exceeds 1 MiB".into());
    }
    Ok(serde_json::from_slice(&manifest_bytes)?)
}

fn parse_blake3_hash(value: &str) -> Result<[u8; 32], ReplayArchiveError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("backup manifest contains an invalid BLAKE3 hash".into());
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)?;
    }
    Ok(bytes)
}

fn backup_generation_name(created_utc_seconds: i64, journal_sequence: u64) -> String {
    format!("backup-{created_utc_seconds:020}-{journal_sequence:020}")
}

fn parse_backup_generation_name(name: &str) -> Option<(i64, u64)> {
    let fields = name.strip_prefix("backup-")?.split('-').collect::<Vec<_>>();
    if fields.len() != 2
        || fields
            .iter()
            .any(|field| field.len() != 20 || !field.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    Some((fields[0].parse().ok()?, fields[1].parse().ok()?))
}

fn latest_backup_utc(
    directory: &Path,
    expected_content: &ContentIdentity,
    expected_endpoint_id: &str,
) -> Result<Option<i64>, ReplayArchiveError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Box::new(error)),
    };
    let mut generations = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some((utc, _sequence)) = entry
            .file_name()
            .to_str()
            .and_then(parse_backup_generation_name)
        else {
            continue;
        };
        generations.push((utc, entry.path()));
    }
    generations.sort_by_key(|generation| std::cmp::Reverse(generation.0));
    for (utc, path) in generations {
        match verify_backup_generation(&path, expected_content, expected_endpoint_id) {
            Ok(_) => return Ok(Some(utc)),
            Err(error) => {
                warn!(path = %path.display(), %error, "ignoring invalid backup generation");
            }
        }
    }
    Ok(None)
}

fn prune_backup_generations(
    directory: &Path,
    current: &Path,
    expected_content: &ContentIdentity,
    expected_endpoint_id: &str,
) -> Result<(), ReplayArchiveError> {
    let mut generations = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let Some((utc, sequence)) = entry
            .file_name()
            .to_str()
            .and_then(parse_backup_generation_name)
        else {
            continue;
        };
        let path = entry.path();
        match verify_backup_generation_header(&path, expected_content, expected_endpoint_id) {
            Ok(_) => generations.push((utc, sequence, path)),
            Err(error) => {
                warn!(path = %path.display(), %error, "excluding invalid generation from backup retention");
            }
        }
    }
    generations.sort_by_key(|generation| std::cmp::Reverse((generation.0, generation.1)));
    let keep = retained_backup_paths(&generations, current);
    let mut removed_any = false;
    for (_, _, path) in generations {
        if !keep.contains(&path) {
            fs::remove_dir_all(path)?;
            removed_any = true;
        }
    }
    if removed_any {
        sync_directory(directory)?;
    }
    Ok(())
}

fn retained_backup_paths(
    generations: &[(i64, u64, PathBuf)],
    current: &Path,
) -> std::collections::BTreeSet<PathBuf> {
    let mut keep = std::collections::BTreeSet::new();
    for (_, _, path) in generations.iter().take(BACKUP_HOURLY_RETENTION) {
        keep.insert(path.clone());
    }
    let mut daily = std::collections::BTreeSet::new();
    for (utc, _, path) in generations.iter().skip(BACKUP_HOURLY_RETENTION) {
        let day = utc.div_euclid(24 * 60 * 60);
        if daily.len() < BACKUP_DAILY_RETENTION && daily.insert(day) {
            keep.insert(path.clone());
        }
    }
    keep.insert(current.to_path_buf());
    keep
}

fn write_replay_archive(
    replay_directory: &Path,
    snapshot_object_directory: &Path,
    prepared: PreparedReplayArchive,
) -> Result<ReplayArchiveWrite, ReplayArchiveError> {
    let content = prepared.bundle.content.clone();
    let verified = prepared.bundle.verify(&content)?;
    let final_tick = verified.tick().0;
    let snapshot_object = prepared.bundle.snapshot_object()?;
    let snapshot_object_hash = snapshot_object.canonical_hash()?;
    if snapshot_object_hash != prepared.bundle.initial_snapshot_object_hash {
        return Err("replay snapshot-object hash does not match its header".into());
    }
    let snapshot_object_decoded = postcard::to_stdvec(&snapshot_object)?;
    if snapshot_object_decoded.len() as u64 > MAX_SNAPSHOT_OBJECT_DECODED {
        return Err("decoded snapshot object exceeds 64 MiB".into());
    }
    let snapshot_object_encoded = zstd::stream::encode_all(snapshot_object_decoded.as_slice(), 3)?;
    if snapshot_object_encoded.len() as u64 > MAX_SNAPSHOT_OBJECT_ENCODED {
        return Err("encoded snapshot object exceeds 64 MiB".into());
    }
    prepare_private_directory(snapshot_object_directory)?;
    let snapshot_object_path = publish_replay_archive(
        snapshot_object_directory,
        &snapshot_object_filename(snapshot_object_hash),
        &snapshot_object_encoded,
    )?;
    verify_snapshot_object_file(&snapshot_object_path, snapshot_object_hash, &content)?;

    let decoded = postcard::to_stdvec(&prepared.bundle)?;
    if decoded.len() > MAX_REPLAY_ARCHIVE_DECODED {
        return Err("decoded replay archive exceeds 256 MiB".into());
    }
    let encoded = zstd::stream::encode_all(decoded.as_slice(), 3)?;
    if encoded.len() as u64 > MAX_REPLAY_ARCHIVE_ENCODED {
        return Err("encoded replay archive exceeds 256 MiB".into());
    }
    let checksum = *blake3::hash(&encoded).as_bytes();
    prepare_private_directory(replay_directory)?;
    let filename = replay_archive_filename(prepared.start, prepared.end);
    let path = publish_replay_archive(replay_directory, &filename, &encoded)?;
    prune_replay_archives(replay_directory, prepared.end.archived_utc_seconds, &path)?;
    let snapshot_gc =
        garbage_collect_snapshot_objects(replay_directory, snapshot_object_directory, &content)?;
    Ok(ReplayArchiveWrite {
        start: prepared.start,
        end: prepared.end,
        path,
        encoded_bytes: encoded.len(),
        checksum,
        snapshot_object_path,
        snapshot_object_hash,
        snapshot_gc,
        final_tick,
    })
}

fn snapshot_object_filename(hash: [u8; 32]) -> String {
    format!("snapshot-{}.cddasnap", blake3::Hash::from_bytes(hash))
}

fn verify_snapshot_object_file(
    path: &Path,
    expected_hash: [u8; 32],
    expected_content: &ContentIdentity,
) -> Result<(), ReplayArchiveError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_SNAPSHOT_OBJECT_ENCODED {
        return Err("snapshot object is not a bounded regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("snapshot object is accessible to group or other users".into());
        }
    }
    let file = fs::File::open(path)?;
    let decoder = zstd::stream::read::Decoder::new(file)?;
    let mut decoded = Vec::new();
    decoder
        .take(MAX_SNAPSHOT_OBJECT_DECODED + 1)
        .read_to_end(&mut decoded)?;
    if decoded.len() as u64 > MAX_SNAPSHOT_OBJECT_DECODED {
        return Err("decoded snapshot object exceeds 64 MiB".into());
    }
    let object: SnapshotObjectV1 = postcard::from_bytes(&decoded)?;
    if object.canonical_hash()? != expected_hash {
        return Err("snapshot object content address does not match".into());
    }
    object.verify(expected_content)?;
    Ok(())
}

fn garbage_collect_snapshot_objects(
    replay_directory: &Path,
    snapshot_object_directory: &Path,
    expected_content: &ContentIdentity,
) -> Result<SnapshotObjectGc, ReplayArchiveError> {
    let mut referenced = BTreeSet::new();
    let mut retained_archives = 0_usize;
    for entry in fs::read_dir(replay_directory)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if replay_archive_end_utc(&name).is_none() {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "recognized replay archive is not a regular file: {}",
                path.display()
            )
            .into());
        }
        let bundle = read_replay_archive(&path, expected_content)?;
        referenced.insert(bundle.initial_snapshot_object_hash);
        retained_archives = retained_archives.saturating_add(1);
    }

    prepare_private_directory(snapshot_object_directory)?;
    for hash in &referenced {
        verify_snapshot_object_file(
            &snapshot_object_directory.join(snapshot_object_filename(*hash)),
            *hash,
            expected_content,
        )?;
    }

    let mut removable = Vec::new();
    for entry in fs::read_dir(snapshot_object_directory)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(hash) = parse_snapshot_object_filename(&name) else {
            continue;
        };
        let path = entry.path();
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(format!(
                "recognized snapshot object is not a regular file: {}",
                path.display()
            )
            .into());
        }
        if !referenced.contains(&hash) {
            removable.push(path);
        }
    }

    for path in &removable {
        fs::remove_file(path)?;
    }
    if !removable.is_empty() {
        sync_directory(snapshot_object_directory)?;
    }
    Ok(SnapshotObjectGc {
        retained_archives,
        retained_objects: referenced.len(),
        removed_objects: removable.len(),
    })
}

fn read_replay_archive(
    path: &Path,
    expected_content: &ContentIdentity,
) -> Result<ReplayBundleV1, ReplayArchiveError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_REPLAY_ARCHIVE_ENCODED {
        return Err(format!(
            "replay archive is not a bounded regular file: {}",
            path.display()
        )
        .into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(format!(
                "replay archive is accessible to group or other users: {}",
                path.display()
            )
            .into());
        }
    }
    let file = fs::File::open(path)?;
    let decoder = zstd::stream::read::Decoder::new(file)?;
    let mut decoded = Vec::new();
    decoder
        .take(MAX_REPLAY_ARCHIVE_DECODED as u64 + 1)
        .read_to_end(&mut decoded)?;
    if decoded.len() > MAX_REPLAY_ARCHIVE_DECODED {
        return Err("decoded replay archive exceeds 256 MiB".into());
    }
    let bundle: ReplayBundleV1 = postcard::from_bytes(&decoded)?;
    bundle.verify(expected_content)?;
    Ok(bundle)
}

fn parse_snapshot_object_filename(filename: &str) -> Option<[u8; 32]> {
    let hash = filename
        .strip_prefix("snapshot-")?
        .strip_suffix(".cddasnap")?;
    let hash = parse_blake3_hash(hash).ok()?;
    (snapshot_object_filename(hash) == filename).then_some(hash)
}

fn replay_archive_filename(start: ReplayArchiveCursor, end: ReplayArchiveCursor) -> String {
    format!(
        "replay-{:020}-{:020}-{:020}-{:020}.cddar",
        start.archived_utc_seconds,
        end.archived_utc_seconds,
        start.journal_sequence,
        end.journal_sequence
    )
}

fn prepare_private_directory(directory: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(directory)?;
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(std::io::Error::other(
            "replay archive path is not a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn publish_replay_archive(
    directory: &Path,
    filename: &str,
    encoded: &[u8],
) -> Result<PathBuf, ReplayArchiveError> {
    let final_path = directory.join(filename);
    match existing_archive_matches(&final_path, encoded) {
        Ok(Some(true)) => return Ok(final_path),
        Ok(Some(false)) => {
            return Err(format!(
                "existing replay archive differs from deterministic retry: {}",
                final_path.display()
            )
            .into());
        }
        Ok(None) => {}
        Err(error) => return Err(error.into()),
    }
    let mut temporary_path = None;
    let mut temporary_file = None;
    for _attempt in 0..16 {
        let candidate = directory.join(format!(
            ".{filename}.tmp-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => {
                temporary_path = Some(candidate);
                temporary_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let temporary_path = temporary_path.ok_or("could not allocate a replay archive temp file")?;
    let mut temporary_file = temporary_file.ok_or("replay archive temp file disappeared")?;
    let write_result = (|| -> Result<(), std::io::Error> {
        temporary_file.write_all(encoded)?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        fs::hard_link(&temporary_path, &final_path)?;
        fs::remove_file(&temporary_path)?;
        sync_directory(directory)
    })();
    if let Err(error) = write_result {
        let _cleanup = fs::remove_file(&temporary_path);
        if existing_archive_matches(&final_path, encoded)? == Some(true) {
            return Ok(final_path);
        }
        return Err(error.into());
    }
    Ok(final_path)
}

fn existing_archive_matches(path: &Path, encoded: &[u8]) -> Result<Option<bool>, std::io::Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_file()
        || metadata.len() != u64::try_from(encoded.len()).unwrap_or(u64::MAX)
    {
        return Ok(Some(false));
    }
    Ok(Some(fs::read(path)? == encoded))
}

fn prune_replay_archives(
    directory: &Path,
    now_utc_seconds: i64,
    current: &Path,
) -> Result<(), std::io::Error> {
    let cutoff = now_utc_seconds.saturating_sub(REPLAY_ARCHIVE_RETENTION_SECONDS);
    let mut removed_any = false;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path == current || !entry.file_type()?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if replay_archive_end_utc(&name).is_some_and(|end_utc| end_utc < cutoff) {
            fs::remove_file(path)?;
            removed_any = true;
        }
    }
    if removed_any {
        sync_directory(directory)?;
    }
    Ok(())
}

fn replay_archive_end_utc(filename: &str) -> Option<i64> {
    let fields = filename
        .strip_prefix("replay-")?
        .strip_suffix(".cddar")?
        .split('-')
        .collect::<Vec<_>>();
    if fields.len() != 4
        || fields
            .iter()
            .any(|field| field.len() != 20 || !field.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    fields[1].parse().ok()
}

fn sync_directory(directory: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        fs::File::open(directory)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _unused = directory;
    Ok(())
}

fn disconnect_all_actors(
    simulation: &SimulationHandle,
) -> Result<SimTick, Box<dyn std::error::Error>> {
    let snapshot = simulation.snapshot(Duration::from_secs(1))?;
    for actor in snapshot.actors.into_iter().filter(|actor| actor.connected) {
        simulation.set_connected(actor.id, false, Duration::from_secs(1))?;
    }
    Ok(simulation.snapshot(Duration::from_secs(1))?.tick)
}

fn drain_through_next_tick(
    host: &SimulationHost,
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
    boundary: SimTick,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let output = host.recv_timeout(Duration::from_secs(1))?;
        let tick = record_simulation_output(output, persistence, pending, journal_sequence)?;
        if tick.0 > boundary.0 {
            break;
        }
    }
    flush_journal(persistence, pending, journal_sequence)
}

fn drain_outputs(
    host: &SimulationHost,
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match host.try_recv() {
            Ok(output) => {
                record_simulation_output(output, persistence, pending, journal_sequence)?;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err("authoritative simulation stopped unexpectedly".into());
            }
        }
    }
    Ok(())
}

fn record_simulation_output(
    output: SimulationOutput,
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
) -> Result<SimTick, Box<dyn std::error::Error>> {
    let (outcome, commands, held_movement, connection_updates, durability) = match output {
        SimulationOutput::Tick {
            outcome,
            commands,
            held_movement,
            connection_updates,
            durability,
        } => (
            outcome,
            commands,
            held_movement,
            connection_updates,
            durability,
        ),
        SimulationOutput::Failed(error) => {
            return Err(format!("authoritative simulation failed: {error}").into());
        }
    };
    let tick = outcome.tick;
    if !outcome.events.is_empty() {
        pending.event_batches.push(CommittedEventBatch {
            tick,
            events: outcome.events.clone(),
        });
    }
    pending.ticks.push(JournalTickV1 {
        tick,
        commands,
        held_movement,
        connection_updates,
        events_hash: canonical_events_hash(&outcome.events)?,
        state_hash: outcome.canonical_hash,
    });
    pending.durability.extend(durability);
    if pending.ticks.len() >= 2 {
        flush_journal(persistence, pending, journal_sequence)?;
    }
    Ok(tick)
}

fn flush_journal(
    persistence: &PersistenceHandle,
    pending: &mut PendingJournal,
    journal_sequence: &mut u64,
) -> Result<(), Box<dyn std::error::Error>> {
    if pending.ticks.is_empty() {
        return Ok(());
    }
    let batch = JournalBatchV1 {
        ticks: std::mem::take(&mut pending.ticks),
        allocator_inputs: Vec::new(),
    };
    match persistence.append_journal_batch_at(batch, utc_now_seconds()?) {
        Ok(sequence) => {
            *journal_sequence = sequence;
            for batch in std::mem::take(&mut pending.event_batches) {
                pending.event_hub.publish(batch);
            }
            for acknowledgement in std::mem::take(&mut pending.durability) {
                let tick = acknowledgement.tick();
                acknowledgement.acknowledge(Ok(tick));
            }
        }
        Err(error) => {
            let detail = error.to_string();
            for acknowledgement in std::mem::take(&mut pending.durability) {
                acknowledgement.acknowledge(Err(detail.clone()));
            }
            return Err(error.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use cdda_protocol::{
        ActorId, ClientCommand, CommandKind, CommandSequence, ControlMessage, SimTick,
        WorldPosition, encode_control,
    };

    use super::*;

    #[test]
    fn pinned_monster_volume_thresholds_map_to_exact_base_sizes() {
        for (volume, expected) in [
            (0, CreatureSizeV1::Tiny),
            (7_500, CreatureSizeV1::Tiny),
            (7_501, CreatureSizeV1::Small),
            (46_250, CreatureSizeV1::Small),
            (46_251, CreatureSizeV1::Medium),
            (108_000, CreatureSizeV1::Medium),
            (108_001, CreatureSizeV1::Large),
            (483_750, CreatureSizeV1::Large),
            (483_751, CreatureSizeV1::Huge),
        ] {
            assert_eq!(monster_size_from_volume(volume), expected);
        }
    }

    #[test]
    fn runtime_mapgen_rejects_weighted_singletons_that_would_lose_rng_phase() {
        let weighted = MapgenIdChoice::Weighted(vec![cdda_content::WeightedMapgenId {
            id: String::from("only"),
            weight: 7,
        }]);
        let concrete = BTreeMap::from([(String::from("only"), 0)]);
        let regional = BTreeMap::new();

        assert!(runtime_mapgen_terrain_choice(&weighted, &concrete, &regional).is_err());
        assert!(runtime_mapgen_furniture_choice(&weighted, &concrete, &regional).is_err());
    }

    #[test]
    fn item_group_charge_overrides_follow_pinned_item_categories() {
        let range = cdda_content::ItemGroupChargesRange {
            minimum: 0,
            maximum: 7,
        };

        let mut ordinary = ItemDefinition {
            id: String::from("ordinary"),
            ..ItemDefinition::default()
        };
        assert!(
            runtime_item_group_charges(&ordinary, Some(range)).is_err(),
            "a ranged ignored modifier would still consume pinned RNG"
        );
        assert_eq!(
            runtime_item_group_charges(
                &ordinary,
                Some(cdda_content::ItemGroupChargesRange {
                    minimum: 30,
                    maximum: -1,
                }),
            )
            .expect("an unresolved upper sentinel is an exact no-op"),
            (None, false)
        );
        assert_eq!(
            runtime_item_group_charges(
                &ordinary,
                Some(cdda_content::ItemGroupChargesRange {
                    minimum: -1,
                    maximum: -1,
                }),
            )
            .expect("an explicit default sentinel is a no-op after marker retention"),
            (None, false)
        );
        assert_eq!(
            runtime_item_group_charges(
                &ordinary,
                Some(cdda_content::ItemGroupChargesRange {
                    minimum: 2,
                    maximum: 2,
                }),
            )
            .expect("fixed ignored modifier consumes no RNG"),
            (None, false)
        );
        assert_eq!(
            runtime_item_group_charges(
                &ordinary,
                Some(cdda_content::ItemGroupChargesRange {
                    minimum: 7,
                    maximum: 2,
                }),
            )
            .expect("pinned charge normalization clamps a reversed range to its maximum"),
            (None, false)
        );

        ordinary.stackable = true;
        assert_eq!(
            runtime_item_group_charges(&ordinary, Some(range))
                .expect("count-by-charges item should admit"),
            (
                Some(ItemGroupChargeRangeV1 {
                    minimum: 0,
                    maximum: 7,
                }),
                true,
            )
        );

        ordinary.stackable = false;
        ordinary.phase = String::from("LIQUID");
        assert_eq!(
            runtime_item_group_charges(&ordinary, Some(range))
                .expect("every liquid charge modifier should clamp"),
            (
                Some(ItemGroupChargeRangeV1 {
                    minimum: 0,
                    maximum: 7,
                }),
                true,
            )
        );

        ordinary.phase.clear();
        ordinary.flags.insert(String::from("CAN_HAVE_CHARGES"));
        assert_eq!(
            runtime_item_group_charges(&ordinary, Some(range))
                .expect("explicit charge-bearing item should admit"),
            (
                Some(ItemGroupChargeRangeV1 {
                    minimum: 0,
                    maximum: 7,
                }),
                false,
            )
        );

        ordinary.subtypes.insert(String::from("TOOL"));
        assert_eq!(
            runtime_item_group_charges(&ordinary, Some(range))
                .expect("ammunition owners retain the range for later storage normalization"),
            (
                Some(ItemGroupChargeRangeV1 {
                    minimum: 0,
                    maximum: 7,
                }),
                false,
            ),
            "range parsing must stay separate from fail-closed storage resolution"
        );
    }

    #[test]
    fn pinned_default_content_builds_authoritative_rock_sock_recipe() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(cdda_content::DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest should load");
        let content_root = manifest_path
            .parent()
            .expect("manifest should have a parent");
        let mods = ModCatalog::load(&manifest, content_root).expect("mods should load");
        let enabled = mods
            .recommended_new_world()
            .expect("default mods should resolve");
        let items = ItemRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("items should load");
        let materials = MaterialRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("materials should load");
        let mut materialless_temperature_items = Vec::new();
        let mut material_temperature_items = Vec::new();
        for (id, item) in items.iter() {
            let Ok(capability) = runtime_item_temperature_capability(item, &materials) else {
                continue;
            };
            if !capability.tracks_temperature {
                continue;
            }
            if capability.rot_shelf_life_turns.is_some() {
                continue;
            }
            if capability.thermal_properties.is_some() {
                material_temperature_items.push(id);
            } else {
                materialless_temperature_items.push(id);
            }
        }
        materialless_temperature_items.sort_unstable();
        material_temperature_items.sort_unstable();
        assert_eq!(
            materialless_temperature_items,
            [
                "brew_rootbeer",
                "chaw",
                "chem_DMSO",
                "chem_chloroform",
                "chem_glycerol",
                "chem_hydrogen_peroxide",
                "chem_phenol",
                "cocaine_topical",
                "dayquil",
                "ecig",
                "ether",
                "eyedrops",
                "fermentable_fish_mixture",
                "fermentable_fish_mixture_active",
                "fermentable_liquid_mixture",
                "fermented_fertilizer_liquid",
                "fert_supplement",
                "fertilizer_liquid",
                "gelatin_extracted",
                "gum",
                "hi_q_distillate",
                "hi_q_shatter",
                "hi_q_wax",
                "latex",
                "liquid_soap",
                "lye",
                "lye_potassium",
                "nectar",
                "nic_gum",
                "nyquil",
                "pine_resin",
                "poppysyrup",
                "royal_jelly",
                "skunk_spray_neutralizing_solution",
                "slime_scrap",
                "steroid_eyedrops",
            ],
            "the fixed selected-content snapshot should admit the complete materialless/nonperishable temperature class"
        );
        assert_custom_freezing_item_admission(&items, &material_temperature_items);
        assert!(material_temperature_items.contains(&"caff_gum"));
        let caff_thermal = runtime_item_temperature_capability(
            items.get("caff_gum").expect("caffeine gum should load"),
            &materials,
        )
        .expect("caffeine gum thermodynamics should normalize")
        .thermal_properties
        .expect("caffeine gum should be material-backed");
        assert_eq!(
            caff_thermal.normal_ambient_specific_energy_millijoules_per_gram(),
            Some(367_780)
        );
        let ammunition =
            AmmunitionRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("ammunition should load");
        let snippets =
            DescriptionSnippetRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("description snippets should load");
        let monsters = MonsterRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("monsters should load");
        let item_group_content = RuntimeItemGroupContent {
            items: &items,
            materials: &materials,
            ammunition: &ammunition,
            snippets: &snippets,
            monsters: &monsters,
        };
        let item_groups =
            ItemGroupRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("item groups should load");
        let field_graph = item_groups
            .strict_graph("field")
            .expect("the complete field loot closure should retain every supported definition");
        assert_eq!(field_graph.maximum_output, 760);
        let child_accessories = runtime_item_group_graph(
            field_graph
                .groups
                .get("accesories_personal_unisex_child")
                .expect("field closure should retain child accessories"),
            item_group_content,
        )
        .expect("detachable tool charges should admit the child-accessory definition");
        let wearable_light = child_accessories
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .find_map(|entry| match &entry.target {
                ItemGroupTargetV1::Item(item) if item.prototype.type_id == "wearable_light" => {
                    Some(item)
                }
                ItemGroupTargetV1::Item(_)
                | ItemGroupTargetV1::Group(_)
                | ItemGroupTargetV1::Node(_) => None,
            })
            .expect("child accessories should retain the charged headlamp");
        assert_eq!(
            wearable_light.charges,
            Some(ItemGroupChargeRangeV1 {
                minimum: 0,
                maximum: 100,
            })
        );
        let Some(cdda_protocol::ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index,
            magazine,
            ammunition: charge_ammunition,
        }) = &wearable_light.tool_charge_storage
        else {
            panic!("headlamp charges should resolve detachable storage")
        };
        assert_eq!(*well_pocket_index, 0);
        assert!(wearable_light.prototype.magazine_wells[0].rigid);
        assert_eq!(magazine.type_id, "medium_battery_cell");
        assert_eq!(magazine.integral_magazines[0].capacity, 56);
        assert_eq!(charge_ammunition.type_id, "battery");
        let light_batteries = runtime_item_group_graph(
            field_graph
                .groups
                .get("ammo_light_batteries")
                .expect("field closure should retain light batteries"),
            item_group_content,
        )
        .expect("the generalized ammunition-loading engine should admit light batteries");
        let light_battery = light_batteries
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .find_map(|entry| match &entry.target {
                ItemGroupTargetV1::Item(item) if item.prototype.type_id == "light_battery_cell" => {
                    Some(item)
                }
                ItemGroupTargetV1::Item(_)
                | ItemGroupTargetV1::Group(_)
                | ItemGroupTargetV1::Node(_) => None,
            })
            .expect("light batteries should retain their integral ammunition storage");
        let Some(cdda_protocol::ItemGroupToolChargeStorageV1::Integral {
            ammunition: light_charge_ammunition,
        }) = &light_battery.tool_charge_storage
        else {
            panic!("light battery charges should resolve integral storage")
        };
        assert_eq!(light_battery.prototype.integral_magazines[0].capacity, 16);
        assert_eq!(light_charge_ammunition.type_id, "battery");
        let bbgun = items
            .get("bbgun")
            .expect("the pinned registry should retain the integral BB gun");
        assert_eq!(
            runtime_item_group_item(
                bbgun,
                Some(cdda_content::ItemGroupChargesRange {
                    minimum: 0,
                    maximum: 150,
                }),
                item_group_content,
            )
            .expect_err("gun charge modifiers require their distinct owner-local engine")
            .to_string(),
            "item group item bbgun cannot retain charge modifiers"
        );
        let necklaces = runtime_item_group_graph(
            field_graph
                .groups
                .get("accessory_necklace")
                .expect("field closure should retain necklaces"),
            item_group_content,
        )
        .expect("description snippet expansion should admit necklaces");
        let saint = necklaces
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .find_map(|entry| match &entry.target {
                ItemGroupTargetV1::Item(item) if item.prototype.type_id == "holy_symbol" => item
                    .variants
                    .iter()
                    .find(|variant| variant.variant.id == "saint_necklace"),
                ItemGroupTargetV1::Item(_)
                | ItemGroupTargetV1::Group(_)
                | ItemGroupTargetV1::Node(_) => None,
            })
            .expect("holy symbols should retain the saint necklace variant");
        let saint_expansion = saint
            .description_expansion
            .as_ref()
            .expect("saint description should retain its expansion closure");
        assert_eq!(saint_expansion.categories.len(), 1);
        assert_eq!(saint_expansion.categories[0].category, "<catholic_saints>");
        assert_eq!(saint_expansion.categories[0].choices.len(), 14);
        let dog_tag = runtime_item_group_item(
            items.get("dog_tag").expect("dog tags should be loaded"),
            None,
            item_group_content,
        )
        .expect("English name categories should admit the dog-tag variant");
        let dog_tag_expansion = dog_tag
            .variants
            .iter()
            .find(|variant| variant.variant.id == "dog_tag_id")
            .and_then(|variant| variant.description_expansion.as_ref())
            .expect("dog-tag identification should retain its complete name closure");
        assert_eq!(dog_tag_expansion.categories.len(), 7);
        assert_eq!(
            dog_tag_expansion
                .categories
                .iter()
                .find(|category| category.category == "<family_name>")
                .expect("family names should be reachable")
                .choices
                .len(),
            3_045
        );
        assert_eq!(
            dog_tag_expansion
                .categories
                .iter()
                .find(|category| category.category == "<female_given_name>")
                .expect("female given names should be reachable")
                .choices
                .len(),
            4_275
        );
        assert_eq!(
            dog_tag_expansion
                .categories
                .iter()
                .find(|category| category.category == "<male_given_name>")
                .expect("male given names should be reachable")
                .choices
                .len(),
            1_219
        );
        let weapon_carry = runtime_item_group_graph(
            field_graph
                .groups
                .get("accessory_weaponcarry")
                .expect("field closure should retain weapon-carry accessories"),
            item_group_content,
        )
        .expect("the generalized variable-size constructor should admit weapon-carry accessories");
        let leg_sheath = weapon_carry
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .find_map(|entry| match &entry.target {
                ItemGroupTargetV1::Item(item) if item.prototype.type_id == "leg_sheath6" => {
                    Some(item)
                }
                ItemGroupTargetV1::Item(_)
                | ItemGroupTargetV1::Group(_)
                | ItemGroupTargetV1::Node(_) => None,
            })
            .expect("weapon-carry accessories should retain the six-knife leg sheath");
        assert!(
            leg_sheath
                .prototype
                .containment
                .flags
                .binary_search_by(|flag| flag.as_str().cmp("VARSIZE"))
                .is_ok()
        );
        assert_regional_field_item_group_closure(&field_graph, item_group_content);
        let painkillers = runtime_item_group_graph(
            field_graph
                .groups
                .get("bottle_otc_painkiller_1_20")
                .expect("field closure should retain the painkiller bottle"),
            item_group_content,
        )
        .expect("explicit null containers should suppress, not erase, item defaults");
        assert_eq!(
            painkillers
                .wrapper
                .as_ref()
                .map(|wrapper| wrapper.item.prototype.type_id.as_str()),
            Some("bottle_plastic_pill_painkiller")
        );
        assert!(
            painkillers
                .nodes
                .iter()
                .flat_map(|node| &node.entries)
                .all(|entry| {
                    entry.modifier_container.is_none()
                        && entry.modifier_default_container_sealed.is_none()
                        && matches!(
                            &entry.target,
                            ItemGroupTargetV1::Item(item)
                                if item.default_container.as_ref().is_some_and(|container| {
                                    container.item.prototype.type_id
                                        == "bottle_plastic_pill_painkiller"
                                })
                        )
                })
        );
        let chaw_wrapper = runtime_item_group_graph(
            field_graph
                .groups
                .get("chaw_wrapper_1_20")
                .expect("field closure should retain chewing tobacco wrappers"),
            item_group_content,
        )
        .expect("flexible reserved-volume wrappers should normalize");
        let chaw_wrapper = chaw_wrapper
            .wrapper
            .as_ref()
            .expect("chewing tobacco should retain its group wrapper");
        assert_eq!(chaw_wrapper.item.prototype.type_id, "wrapper");
        let [chaw_pocket] = chaw_wrapper.item.prototype.ammunition_containers.as_slice() else {
            panic!("paper wrapper should retain one physical pocket")
        };
        let chaw_rules = chaw_pocket
            .spawn_rules
            .as_ref()
            .expect("paper wrapper should retain spawn rules");
        assert!(!chaw_rules.rigid);
        assert_eq!(chaw_rules.magazine_well_volume_milliliters, 45);
        assert!(!chaw_rules.contents_collapsed_by_default);

        let chewing_gum = runtime_item_group_graph(
            field_graph
                .groups
                .get("chewing_gum_full")
                .expect("field closure should retain ordinary chewing gum"),
            item_group_content,
        )
        .expect("COLLAPSE_CONTENTS wrappers should normalize");
        let gum_wrapper = chewing_gum
            .wrapper
            .as_ref()
            .expect("chewing gum should retain its blister pack");
        assert_eq!(gum_wrapper.variant_id.as_deref(), Some("blister_pack_gum"));
        assert!(
            gum_wrapper.item.prototype.ammunition_containers[0]
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.contents_collapsed_by_default)
        );
        let eink_tablets = runtime_item_group_graph(
            field_graph
                .groups
                .get("civilian_eink_tablet_pcs")
                .expect("field closure should retain civilian e-ink tablets"),
            item_group_content,
        )
        .expect("integral-tool capacity sentinels should normalize");
        let eink_tablet = eink_tablets
            .nodes
            .iter()
            .flat_map(|node| &node.entries)
            .find_map(|entry| match &entry.target {
                ItemGroupTargetV1::Item(item) if item.prototype.type_id == "eink_tablet_pc" => {
                    Some(item.as_ref())
                }
                ItemGroupTargetV1::Item(_)
                | ItemGroupTargetV1::Group(_)
                | ItemGroupTargetV1::Node(_) => None,
            })
            .expect("civilian tablet group should retain its direct tablet leaf");
        assert_eq!(
            eink_tablet.charges,
            Some(cdda_protocol::ItemGroupChargeRangeV1 {
                minimum: 0,
                maximum: -1,
            })
        );
        assert_eq!(eink_tablet.prototype.charges, 0);
        assert_eq!(
            eink_tablet.charge_capacity,
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            "upstream is_magazine includes integral MAGAZINE pockets"
        );
        assert!(matches!(
            &eink_tablet.tool_charge_storage,
            Some(cdda_protocol::ItemGroupToolChargeStorageV1::Integral { ammunition })
                if ammunition.type_id == "battery"
                    && eink_tablet.prototype.integral_magazines[0].capacity == 85
        ));
        let phone_case_graph = item_groups
            .strict_graph("civilian_phones_case")
            .expect("the phone-case containment family should parse as one strict closure");
        assert_eq!(phone_case_graph.maximum_output, 11);
        runtime_item_group_graph(&phone_case_graph.root, item_group_content)
            .expect("the phone-case root should normalize for the authoritative runtime");
        let phone_case_runtime_errors = phone_case_graph
            .groups
            .values()
            .filter_map(|definition| {
                runtime_item_group_graph(definition, item_group_content)
                    .err()
                    .map(|error| (definition.id.as_str(), error.to_string()))
            })
            .collect::<Vec<_>>();
        assert!(
            phone_case_runtime_errors.is_empty(),
            "phone-case closure should normalize for the authoritative runtime: {phone_case_runtime_errors:#?}"
        );
        let content_events = [
            ItemGroupEvent::NewYear,
            ItemGroupEvent::Easter,
            ItemGroupEvent::IndependenceDay,
            ItemGroupEvent::Halloween,
            ItemGroupEvent::Thanksgiving,
            ItemGroupEvent::Christmas,
        ];
        let event_definition = StrictItemGroupDefinition {
            id: String::from("event_projection"),
            subtype: ItemGroupSubtype::Distribution,
            ammo_chance: 0,
            magazine_chance: 0,
            wrapper: None,
            roots: (0..content_events.len())
                .map(|index| u32::try_from(index).expect("event index fits"))
                .collect(),
            nodes: content_events
                .into_iter()
                .map(|event| StrictItemGroupNode {
                    kind: StrictItemGroupNodeKind::Item(String::from("rock")),
                    probability: 1,
                    count: cdda_content::ItemGroupRange::ONE,
                    charges: None,
                    damage: None,
                    variant: None,
                    direct_wrapper: None,
                    modifier_container: None,
                    modifier_sealed: None,
                    contents: Vec::new(),
                    event: Some(event),
                })
                .collect(),
        };
        assert_eq!(
            runtime_item_group_graph(&event_definition, item_group_content)
                .expect("every holiday qualifier should project")
                .nodes[0]
                .entries
                .iter()
                .map(|entry| entry.event)
                .collect::<Vec<_>>(),
            [
                Some(ItemGroupEventV1::NewYear),
                Some(ItemGroupEventV1::Easter),
                Some(ItemGroupEventV1::IndependenceDay),
                Some(ItemGroupEventV1::Halloween),
                Some(ItemGroupEventV1::Thanksgiving),
                Some(ItemGroupEventV1::Christmas),
            ]
        );
        let mut damage_definition = event_definition.clone();
        damage_definition.nodes[0].damage = Some(cdda_content::ItemGroupRange {
            minimum: 1,
            maximum: 4,
        });
        let damage_graph = runtime_item_group_graph(&damage_definition, item_group_content)
            .expect("raw damage should project exactly");
        assert_eq!(
            damage_graph.nodes[0].entries[0].raw_damage,
            Some(cdda_protocol::InclusiveU16RangeV1 {
                minimum: 1_000,
                maximum: 4_000,
            })
        );
        let mut entry_wrapper_definition = event_definition.clone();
        entry_wrapper_definition.nodes[0].direct_wrapper = Some(ItemGroupEntryWrapper {
            item: String::from("waterproof_smart_phone_case"),
            variant: None,
        });
        entry_wrapper_definition.nodes[0].modifier_sealed = Some(false);
        let entry_wrapper_graph =
            runtime_item_group_graph(&entry_wrapper_definition, item_group_content).expect(
                "entry containment should normalize through the generalized wrapper engine",
            );
        let entry_wrapper = entry_wrapper_graph.nodes[0].entries[0]
            .direct_wrapper
            .as_ref()
            .expect("entry wrapper should be retained");
        assert_eq!(
            entry_wrapper.item.prototype.type_id,
            "waterproof_smart_phone_case"
        );
        assert_eq!(
            entry_wrapper.overflow,
            cdda_protocol::ItemGroupOverflowV1::None
        );
        assert!(
            entry_wrapper.sealed,
            "entry-wrapper sealing is independent from modifier sealing"
        );
        assert!(!entry_wrapper_graph.nodes[0].entries[0].seal_contents);
        let mut contents_definition = entry_wrapper_definition.clone();
        contents_definition.nodes[0].direct_wrapper = None;
        contents_definition.nodes[0].contents = vec![cdda_content::ItemGroupContentsSource::Item(
            String::from("rock"),
        )];
        let contents_graph = runtime_item_group_graph(&contents_definition, item_group_content)
            .expect("explicit unsealed modifier contents should normalize");
        assert!(!contents_graph.nodes[0].entries[0].seal_contents);
        contents_definition.nodes[0].modifier_sealed = None;
        let default_sealed_contents =
            runtime_item_group_graph(&contents_definition, item_group_content)
                .expect("default-sealed modifier contents should normalize");
        assert!(default_sealed_contents.nodes[0].entries[0].seal_contents);
        let mut variant_definition = event_definition.clone();
        variant_definition.nodes[0].variant = Some(String::from("oracle_variant"));
        let variant_graph = runtime_item_group_graph(&variant_definition, item_group_content)
            .expect("explicit variants should project");
        assert_eq!(
            variant_graph.nodes[0].entries[0].variant_id.as_deref(),
            Some("oracle_variant")
        );
        assert_eq!(
            variant_graph.nodes[0].entries[0].raw_damage,
            Some(cdda_protocol::InclusiveU16RangeV1 {
                minimum: 0,
                maximum: 0,
            })
        );
        let mut wrapper_definition = event_definition;
        wrapper_definition.wrapper = Some(ItemGroupWrapper {
            item: String::from("waterproof_smart_phone_case"),
            variant: None,
            sealed: true,
            overflow: ItemGroupOverflow::Discard,
        });
        let wrapper_graph = runtime_item_group_graph(&wrapper_definition, item_group_content)
            .expect("whole-group containment should normalize through the generalized engine");
        let wrapper = wrapper_graph
            .wrapper
            .as_ref()
            .expect("whole-group wrapper should be retained");
        assert_eq!(
            wrapper.item.prototype.type_id,
            "waterproof_smart_phone_case"
        );
        assert_eq!(
            wrapper.overflow,
            cdda_protocol::ItemGroupOverflowV1::Discard
        );
        let skills = SkillRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("skills should load");
        let fields = FieldTypeRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("field types should load");
        let bash_profiles =
            BashDamageProfileRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("bash profiles should load");
        let zombie = monsters
            .get("mon_zombie")
            .expect("classic zombie should load");
        validate_monster_attack_costs(&monsters)
            .expect("every selected monster attack cost should enter the runtime exactly");
        assert_eq!(monster_attack_cost(zombie).expect("zombie cost fits"), 100);
        assert_eq!(
            monster_attack_cost(
                monsters
                    .get("mon_skeleton_slasher")
                    .expect("skeletal slasher should load")
            )
            .expect("skeletal slasher cost fits"),
            70
        );
        assert_eq!(
            monster_attack_cost(
                monsters
                    .get("mon_dog_zombie_brute")
                    .expect("zombie dog brute should load")
            )
            .expect("zombie dog brute cost fits"),
            150
        );
        assert_eq!(
            monster_attack_cost(
                monsters
                    .get("mon_dog_zombie_hulk")
                    .expect("zombie dog hulk should load")
            )
            .expect("zombie dog hulk cost fits"),
            187,
            "inherited 150 * 1.25 truncates like pinned int compound assignment"
        );
        let mut invalid = zombie.clone();
        invalid.attack_cost_moves = 0;
        assert!(monster_attack_cost(&invalid).is_err());
        invalid.attack_cost_moves = i32::from(u16::MAX) + 1;
        assert!(monster_attack_cost(&invalid).is_err());
        assert_eq!(monster_blood_field_type(zombie), "fd_blood");
        assert!(zombie.flags.contains("REVIVES"));
        assert!(zombie.flags.contains("SEES"));
        assert!(zombie.flags.contains("STUMBLES"));
        assert!(zombie.flags.contains("BASHES"));
        assert!(zombie.flags.contains("GROUP_BASH"));
        assert!(zombie.flags.contains("HEARS"));
        assert!(!zombie.flags.contains("GOODHEARING"));
        assert!(zombie.flags.contains("CLUMSY_ATTACKS"));
        assert!(!zombie.flags.contains("IMMOBILE"));
        assert!(!zombie.flags.contains("PACIFIST"));
        assert!(!zombie.flags.contains("CAN_OPEN_DOORS"));
        assert_eq!(zombie.volume_milliliters, 62_500);
        assert_eq!(monster_size(zombie), CreatureSizeV1::Medium);
        assert!(
            monsters
                .get("mon_turret")
                .expect("pinned improvised turret should load")
                .flags
                .contains("IMMOBILE")
        );
        assert!(
            monsters
                .get("mon_grocerybot")
                .expect("pinned grocery bot should load")
                .flags
                .contains("PACIFIST")
        );
        assert_eq!(
            monster_path_settings(zombie).expect("zombie path settings should normalize"),
            CreaturePathSettingsV1::default()
        );
        let feral = monsters
            .get("mon_feral_human_pipe")
            .expect("door-opening feral human should load");
        assert!(feral.flags.contains("CAN_OPEN_DOORS"));
        assert!(!feral.unsupported_fields.contains("path_settings"));
        assert_eq!(feral.path_settings.max_distance, 45);
        assert!(feral.path_settings.allow_open_doors);
        assert!(feral.path_settings.avoid_traps);
        assert!(feral.path_settings.avoid_sharp);
        let feral_path =
            monster_path_settings(feral).expect("feral path settings should normalize");
        assert_eq!(feral_path.max_distance, 45);
        assert!(feral_path.allow_open_doors);
        assert_eq!(zombie.vision_day, 40);
        assert_eq!(zombie.vision_night, 3);
        assert_eq!(
            firearm_sound_volume(
                items.get("sw_619").expect("starter revolver should load"),
                items
                    .get("38_special")
                    .expect("starter ammunition should load"),
            )
            .expect("starter firearm loudness should finalize"),
            70
        );
        let blood = runtime_field_type(fields.get("fd_blood").expect("blood should load"))
            .expect("blood should normalize");
        assert_eq!(blood.field_type_id, "fd_blood");
        assert_eq!(blood.half_life_seconds, 2 * 24 * 60 * 60);
        assert_eq!(blood.intensity_levels.len(), 3);
        let terrain = TerrainRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("terrain should load");
        let furniture = FurnitureRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("furniture should load");
        assert!(furniture_tile(furniture.get("f_dresser").expect("dresser")).blocks_door);
        assert_eq!(
            terrain_tile(terrain.get("t_door_c").expect("closed door"), &terrain)
                .expect("ordinary door should normalize")
                .open,
            "t_door_o"
        );
        assert!(
            terrain_tile(
                terrain
                    .get("t_door_elocked")
                    .expect("inside-only locked door"),
                &terrain,
            )
            .expect("inside-only door should normalize fail-closed")
            .open
            .is_empty()
        );
        let wall_definition = terrain.get("t_wall").expect("wall should load");
        let wall_bash = runtime_terrain_bash_type(
            wall_definition,
            &bash_profiles,
            &fields,
            &terrain,
            item_group_content,
            &item_groups,
            Some("t_floor"),
        )
        .expect("wall bash should normalize");
        assert_eq!(wall_bash.result.terrain_id, "t_floor");
        assert_eq!(
            wall_bash.drop_source,
            Some(ItemGroupSourceV1::Group(String::from("wall_bash_results")))
        );
        let wall_catalog = runtime_bash_item_group_catalog(
            [wall_definition.bash.as_ref().expect("wall bash")],
            &item_groups,
            item_group_content,
        )
        .expect("wall item-group closure should normalize");
        assert_eq!(wall_catalog.len(), 1);
        assert_eq!(wall_catalog[0].group_id, "wall_bash_results");
        let regions = DefaultRegionTerrainFurnitureRegistry::load_selected(
            &manifest,
            content_root,
            &mods,
            &enabled,
            &terrain,
            &furniture,
        )
        .expect("default regional substitutions should load");
        let mapgen = MapgenRegistry::load_selected(
            &manifest,
            content_root,
            &mods,
            &enabled,
            &terrain,
            &furniture,
            &item_groups,
        )
        .expect("strict mapgen should load");
        let overmap_terrain =
            OvermapTerrainRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("overmap terrain should load");
        let start_locations =
            StartLocationRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("start locations should load");
        let wilderness = runtime_mapgen_worldgen(
            bootstrap_lmoe_overmap(&overmap_terrain).expect("LMOE layer should normalize"),
            start_locations
                .get("sloc_lmoe")
                .expect("LMOE start location should load"),
            RuntimeMapgenContent {
                mapgen: &mapgen,
                regions: &regions,
                terrain: &terrain,
                furniture: &furniture,
                item_groups: &wall_catalog,
            },
        )
        .expect("pinned surface mapgen should normalize");
        assert_eq!(wilderness.overmap.identities[0].full_id, "lmoe_north");
        assert_eq!(wilderness.overmap.identities[0].generator_id, "lmoe");
        assert_eq!(
            wilderness
                .start_location
                .as_ref()
                .expect("start location should normalize")
                .start_location_id,
            "sloc_lmoe"
        );
        assert!(overmap_terrain.get_identity("field").is_some());
        assert!(
            runtime_mapgen_worldgen(
                bootstrap_lmoe_overmap(&overmap_terrain).expect("LMOE layer should normalize"),
                start_locations
                    .get("sloc_shelter_safe")
                    .expect("parameterized shelter start should load"),
                RuntimeMapgenContent {
                    mapgen: &mapgen,
                    regions: &regions,
                    terrain: &terrain,
                    furniture: &furniture,
                    item_groups: &wall_catalog,
                },
            )
            .is_err(),
            "parameterized start locations must fail closed"
        );
        super::regional_field_acceptance::assert_production_regional_field_gameplay(
            &field_graph,
            item_group_content,
            &overmap_terrain,
            &start_locations,
            &mapgen,
            &regions,
            &terrain,
            &furniture,
        );
        assert_eq!(wilderness.omt_generators[0].templates.len(), 1);
        assert_eq!(wilderness.regional_terrain.len(), 1);
        assert_eq!(
            wilderness.regional_terrain[0].regional_id,
            "t_region_groundcover"
        );
        assert!(worldgen_catalog_is_valid(&wilderness, &wall_catalog));
        let wall_entries = &wall_catalog[0].graph.nodes[0].entries;
        assert_eq!(wall_entries.len(), 8);
        assert_eq!(
            (wall_entries[0].count_min, wall_entries[0].count_max),
            (0, 2)
        );
        let ItemGroupTargetV1::Item(nails) = &wall_entries[2].target else {
            panic!("nail drop should be a direct item")
        };
        assert_eq!(nails.prototype.type_id, "nail");
        assert_eq!(
            nails.charges,
            Some(ItemGroupChargeRangeV1 {
                minimum: 4,
                maximum: 16,
            })
        );
        assert!(nails.minimum_one_charge);
        assert_eq!(wall_entries[4].probability, 25);
        assert_eq!(
            item_group_source_max_outputs(
                wall_bash.drop_source.as_ref().expect("wall drops"),
                &wall_catalog,
            ),
            Some(82)
        );
        let mut dressed_wall = item_groups
            .strict_graph("wall_bash_results")
            .expect("wall group should strictly normalize")
            .root;
        dressed_wall.ammo_chance = 1;
        let dressed_wall = runtime_item_group_graph(&dressed_wall, item_group_content)
            .expect("generalized ammunition dressing should normalize");
        let dressing_marker = encode_item_group_dressing_marker(1, 0)
            .expect("nonzero synthetic dressing should encode");
        assert!(
            dressed_wall
                .nodes
                .iter()
                .flat_map(|node| &node.entries)
                .filter(|entry| matches!(
                    entry.target,
                    ItemGroupTargetV1::Item(_) | ItemGroupTargetV1::Group(_)
                ))
                .all(|entry| entry.contents.iter().any(|contents| matches!(
                    contents,
                    ItemGroupContentsSourceV1::Group(group_id) if group_id == &dressing_marker
                ))),
            "every concrete/named leaf should carry inherited dressing"
        );
        for (terrain_id, dynamic_floor_result, multiplier) in [
            ("t_door_b", None, 950_000),
            ("t_door_c", None, 950_000),
            ("t_door_frame", Some("t_floor"), 1_000_000),
        ] {
            let bash = runtime_terrain_bash_type(
                terrain.get(terrain_id).expect("door stage should load"),
                &bash_profiles,
                &fields,
                &terrain,
                item_group_content,
                &item_groups,
                dynamic_floor_result,
            )
            .expect("door bash should normalize");
            assert_eq!(bash.terrain_id, terrain_id);
            assert_eq!(bash.bash_multiplier_millionths, multiplier);
            assert_eq!(
                bash.hit_field.as_ref().expect("dust").field_type_id,
                "fd_dust"
            );
            assert_eq!(
                bash.destroyed_field
                    .as_ref()
                    .expect("splinters")
                    .field_type_id,
                "fd_splinters"
            );
            assert!(bash.drop_source.is_some());
            if terrain_id == "t_door_frame" {
                assert_eq!(bash.result.terrain_id, "t_floor");
            }
        }
        let furniture_bashes = runtime_furniture_bash_types(
            &furniture,
            &bash_profiles,
            &fields,
            item_group_content,
            &item_groups,
        )
        .expect("admitted furniture bashes should normalize");
        let no_materials = MaterialRegistry::default();
        let pre_material_content = RuntimeItemGroupContent {
            materials: &no_materials,
            ..item_group_content
        };
        let pre_material_bashes = runtime_furniture_bash_types(
            &furniture,
            &bash_profiles,
            &fields,
            pre_material_content,
            &item_groups,
        )
        .expect("the prior materialless boundary should remain measurable");
        let pre_material_ids = pre_material_bashes
            .iter()
            .map(|bash| bash.furniture_id.as_str())
            .collect::<BTreeSet<_>>();
        let newly_admitted_material_bashes = furniture_bashes
            .iter()
            .filter(|bash| !pre_material_ids.contains(bash.furniture_id.as_str()))
            .map(|bash| bash.furniture_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(pre_material_bashes.len(), 530);
        assert_eq!(
            newly_admitted_material_bashes,
            [
                "f_archery_target_bale",
                "f_beach_seaweed",
                "f_firefly_terrarium",
                "f_hay",
                "f_straw_bed",
                "f_tatami",
            ],
            "the changed aggregate must remain explained by exact material-backed and perishable drop owners"
        );
        assert_eq!(
            furniture_bashes.len(),
            536,
            "material thermodynamics and rot should admit exactly six additional furniture bashes"
        );
        for furniture_id in [
            "f_cardboard_door_o",
            "f_cardboard_roof",
            "f_pallet_brick_adobe",
        ] {
            assert!(
                furniture_bashes
                    .iter()
                    .any(|bash| bash.furniture_id == furniture_id),
                "the audited damage/variant family should admit {furniture_id}"
            );
        }
        for furniture_id in [
            "f_earthbag_half",
            "f_earthbag_wall",
            "f_exodii_charger",
            "f_exodii_pump",
            "f_pillow_fort",
            "f_string_dimension_pump",
        ] {
            assert!(
                furniture_bashes
                    .iter()
                    .any(|bash| bash.furniture_id == furniture_id),
                "the audited flexible/collapsed containment family should admit {furniture_id}"
            );
        }
        let furniture_bash_ids = furniture
            .iter()
            .filter(|definition| definition.bash.is_some())
            .map(|definition| definition.id.clone())
            .collect::<BTreeSet<_>>();
        assert!(
            furniture_bash_ids.len() > furniture_bashes.len(),
            "pinned content should retain unsupported bash layers separately"
        );
        for furniture_id in ["f_bed", "f_chair", "f_dresser", "f_table"] {
            assert!(
                furniture_bashes
                    .iter()
                    .any(|bash| bash.furniture_id == furniture_id),
                "fresh-cabin furniture {furniture_id} should be admitted"
            );
        }
        for furniture_id in [
            "f_clothing_rail",
            "f_drophammer",
            "f_dumpster",
            "f_exodii_charger_cheap",
            "f_power_hammer",
            "f_treadmill",
        ] {
            assert!(
                !furniture_bashes
                    .iter()
                    .any(|bash| bash.furniture_id == furniture_id),
                "{furniture_id} must remain unavailable until all generated item state and RNG side effects are represented"
            );
        }
        assert!(furniture_bashes.iter().any(|bash| bash.result.is_some()));
        let dresser_bash = furniture_bashes
            .iter()
            .find(|bash| bash.furniture_id == "f_dresser")
            .expect("dresser bash should be admitted");
        assert_eq!(dresser_bash.furniture_id, "f_dresser");
        assert!(dresser_bash.result.is_none());
        assert_eq!(dresser_bash.bash_multiplier_millionths, 1_000_000);
        assert!(dresser_bash.drop_source.is_some());
        let smash_items = runtime_smash_item_types(&items);
        let hammer_smash = smash_items
            .iter()
            .find(|profile| profile.item_type_id == "hammer")
            .expect("pinned hammer should be in the strict smash-item subset");
        assert_eq!(hammer_smash.bash_damage, 9);
        assert_eq!(hammer_smash.attack_time_moves, 79);
        assert_eq!(hammer_smash.melee_to_hit, -1);
        let starter_revolver = items
            .get("model_10_revolver")
            .expect("starter revolver should load");
        let starter_round = items
            .get("38_special")
            .expect("starter ammunition should load");
        let static_revolver_time = starter_revolver
            .ordinary_attack_time_moves()
            .expect("unloaded revolver base time should be representable");
        let loaded_revolver_time = 65
            + starter_revolver.volume_milliliters * 2 / 125
            + (starter_revolver.weight_milligrams
                + starter_round.weight_milligrams * i64::from(starter_revolver.clip_size))
                / 60_000;
        assert_eq!(static_revolver_time, 87);
        assert_eq!(loaded_revolver_time, 88);
        assert_eq!(u32::from(static_revolver_time) * 4 / 5, 69);
        assert_eq!(loaded_revolver_time * 4 / 5, 70);
        assert!(
            !smash_items
                .iter()
                .any(|profile| profile.item_type_id == starter_revolver.id),
            "the reachable loaded starter revolver must not use its cheaper unloaded type timing"
        );
        for dynamic_type in ["flashlight", "medium_battery_cell", "toaster"] {
            assert!(
                !smash_items
                    .iter()
                    .any(|profile| profile.item_type_id == dynamic_type),
                "{dynamic_type} has live ranged, magazine, or powered weight state"
            );
        }
        let mut bash_validation = WorldState::new(60, [60; 32]);
        let item_group_catalog = runtime_bash_item_group_catalog(
            furniture_bashes.iter().filter_map(|runtime| {
                furniture
                    .get(&runtime.furniture_id)
                    .and_then(|definition| definition.bash.as_ref())
            }),
            &item_groups,
            item_group_content,
        )
        .expect("admitted furniture group closure should normalize");
        bash_validation
            .register_item_group_catalog(item_group_catalog)
            .expect("admitted furniture group closure should register");
        for field_type_id in ["fd_dust", "fd_splinters"] {
            bash_validation
                .register_field_type(
                    runtime_field_type(fields.get(field_type_id).expect("bash field should load"))
                        .expect("bash field should normalize"),
                )
                .expect("bash field should register");
        }
        for furniture_id in &furniture_bash_ids {
            bash_validation
                .register_furniture_bash_presence(furniture_id.clone())
                .expect("pinned furniture bash presence should validate");
        }
        for profile in smash_items {
            bash_validation
                .register_smash_item_type(profile)
                .expect("strict smash-item profile should validate");
        }
        for bash in furniture_bashes {
            let furniture_id = bash.furniture_id.clone();
            bash_validation
                .register_furniture_bash_type(bash)
                .unwrap_or_else(|error| {
                    panic!("admitted furniture bash {furniture_id} should validate: {error}")
                });
        }
        let bash_snapshot = bash_validation.snapshot();
        let bash_snapshot_bytes = postcard::to_stdvec(&bash_snapshot)
            .expect("admitted furniture bash snapshot should encode");
        assert!(
            // Protocol 87's generalized containment and constructor state add
            // bounded fields to every retained item-group prototype. Against
            // the pinned snapshot, the unchanged 524-definition catalog grows
            // from 124_929 to 166_941 bytes.
            bash_snapshot_bytes.len() <= 192 * 1024,
            "admitted bash snapshot grew to {} bytes",
            bash_snapshot_bytes.len()
        );
        assert_eq!(
            bash_snapshot.furniture_bash_ids,
            furniture_bash_ids.into_iter().collect::<Vec<_>>()
        );
        WorldState::from_snapshot(&bash_snapshot)
            .expect("admitted furniture bash snapshot should restore");
        let mut missing_presence = bash_snapshot.clone();
        let admitted_id = missing_presence.furniture_bash_types[0]
            .furniture_id
            .clone();
        missing_presence
            .furniture_bash_ids
            .retain(|furniture_id| furniture_id != &admitted_id);
        assert!(
            WorldState::from_snapshot(&missing_presence).is_err(),
            "an admitted definition without canonical upstream presence must reject"
        );
        let proficiencies =
            ProficiencyRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("proficiencies should load");
        let recipes = RecipeRegistry::load_selected(
            &manifest,
            content_root,
            &mods,
            &enabled,
            &items,
            &skills,
            &proficiencies,
        )
        .expect("recipes should load");
        let constructions =
            ConstructionRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("constructions should load");
        assert_eq!(constructions.len(), 776);
        assert_eq!(constructions.group_count(), 438);
        let place_table = constructions
            .get("constr_place_table")
            .expect("pinned table placement should exist");
        assert_eq!(place_table.time_moves, 6_000);
        assert_eq!(place_table.pre_special, ["check_empty"]);
        assert_eq!(place_table.post_terrain, "f_table");
        assert!(place_table.unsupported_fields.is_empty());
        let construction = build_construction_catalog(
            &constructions,
            &recipes,
            &items,
            &skills,
            &terrain,
            &furniture,
        )
        .expect("strict constructions should normalize");
        let normalized_table = construction
            .get("constr_place_table")
            .expect("place-table construction should be runnable");
        assert_eq!(normalized_table.name, "Place Table");
        assert_eq!(normalized_table.time_moves, 6_000);
        assert_eq!(
            normalized_table.components,
            [vec![CraftComponentRequirementV1 {
                type_id: String::from("w_table"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }]]
        );
        let carpet = construction
            .get("constr_carpet_conc_green")
            .expect("exact-floor carpeting should be runnable");
        assert_eq!(carpet.pre_terrain, ["t_thconc_floor"]);
        assert!(!carpet.requires_empty);
        assert!(matches!(
            &carpet.result,
            ConstructionResultV1::Terrain(tile)
                if tile.terrain_id == "t_carpet_concrete_green"
        ));
        let brick_oven = construction
            .get("constr_brick_oven_finisher")
            .expect("quality-backed brick-oven finishing should be runnable");
        assert_eq!(brick_oven.pre_terrain, ["t_brick_oven_struct"]);
        assert_eq!(brick_oven.qualities.len(), 2);
        assert_eq!(brick_oven.qualities[0][0].quality_id, "AXE");
        assert_eq!(brick_oven.qualities[0][0].level, 2);
        assert!(!brick_oven.qualities[0][0].providers.is_empty());
        assert_eq!(brick_oven.qualities[1][0].quality_id, "CHISEL_WOOD");
        assert_eq!(brick_oven.qualities[1][0].level, 1);
        assert!(!brick_oven.qualities[1][0].providers.is_empty());
        let hammered_carpet = construction
            .get("constr_carpet_green")
            .expect("LIST-expanded carpeting should be runnable");
        assert_eq!(
            hammered_carpet.components[0]
                .iter()
                .map(|component| (component.type_id.as_str(), component.count))
                .collect::<Vec<_>>(),
            [("nail", 5), ("bronze_nail", 5)]
        );
        assert_eq!(hammered_carpet.qualities[0][0].quality_id, "HAMMER");
        assert!(construction.get("constr_hay").is_some());
        assert_eq!(construction.len(), 55);
        let crafting = build_crafting_catalog(&recipes, &items, &materials, &proficiencies)
            .expect("runnable recipes should normalize");
        let disassembly =
            build_disassembly_catalog(&recipes, &items, &materials, &ammunition, &crafting)
                .expect("strict reversible recipes should normalize");
        let powered_light_items = items
            .iter()
            .filter_map(|(item_id, item)| {
                strict_detachable_battery_light(item, &items)
                    .expect("powered projection should not fail")
                    .map(|projection| (item_id, projection))
            })
            .collect::<Vec<_>>();
        let runtime_battery_magazines = items
            .iter()
            .filter_map(|(item_id, item)| {
                strict_battery_magazine_capacity(item).map(|capacity| (item_id, capacity))
            })
            .collect::<Vec<_>>();
        let quiver = items
            .get("quiver")
            .expect("pinned default content should contain the starter quiver");
        let quiver_containers =
            runtime_ammunition_containers(quiver).expect("starter quiver pockets should normalize");
        assert_eq!(
            quiver_containers,
            [AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::new(),
                capacities: vec![
                    AmmunitionCapacityV1 {
                        ammunition_type: String::from("arrow"),
                        capacity: 20,
                    },
                    AmmunitionCapacityV1 {
                        ammunition_type: String::from("bolt"),
                        capacity: 20,
                    },
                ],
                rigid: false,
                access_moves: 20,
                reloadable: true,
                unloadable: true,
                spawn_rules: None,
            }]
        );
        let mut sealed_quiver = quiver.clone();
        sealed_quiver.flags.insert(String::from("NO_RELOAD"));
        sealed_quiver.flags.insert(String::from("NO_UNLOAD"));
        let sealed_projection = runtime_ammunition_containers(&sealed_quiver)
            .expect("sealed quiver pockets should normalize");
        assert!(
            sealed_projection
                .iter()
                .all(|pocket| !pocket.reloadable && !pocket.unloadable),
            "container access flags must project into authoritative runtime policy"
        );
        assert_eq!(
            craft_item_prototype(quiver, default_instance_charges(quiver), &items, &materials,)
                .expect("quiver craft prototype should normalize")
                .ammunition_containers,
            quiver_containers,
            "crafted and spawned quivers must use the same strict pocket projection"
        );
        let arrow = items
            .get("arrow_wood")
            .expect("pinned default content should contain starter arrows");
        let mut container_world = WorldState::new(59, [59; 32]);
        container_world
            .install_reserved_block(cdda_sim::ReservedIdBlock { start: 1, end: 8 })
            .expect("container fixture ID block should install");
        container_world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let container_position = WorldPosition { x: 1, y: 1, z: 0 };
        let quiver_id = container_world
            .spawn_ground_item_with_ammunition_containers(
                ItemSpawn {
                    position: container_position,
                    type_id: quiver.id.clone(),
                    charges: default_instance_charges(quiver),
                    melee_damage_milli: quiver
                        .melee_damage_milli()
                        .expect("quiver melee damage should normalize"),
                    calories: quiver.calories,
                    quench: quiver.quench,
                    comestible_type: quiver.comestible_type.clone(),
                    ammunition_type: String::new(),
                    ranged_weapon: None,
                },
                runtime_ammunition_containers(quiver)
                    .expect("starter quiver pockets should normalize"),
            )
            .expect("strict starter quiver should spawn");
        let arrow_id = container_world
            .spawn_ground_item(ItemSpawn {
                position: container_position,
                type_id: arrow.id.clone(),
                charges: arrow.default_charges().max(1),
                melee_damage_milli: arrow
                    .melee_damage_milli()
                    .expect("arrow melee damage should normalize"),
                calories: arrow.calories,
                quench: arrow.quench,
                comestible_type: arrow.comestible_type.clone(),
                ammunition_type: single_ammunition_type(arrow)
                    .expect("wooden arrow should have one ammunition type"),
                ranged_weapon: None,
            })
            .expect("starter arrow stack should spawn");
        let container_snapshot = container_world.snapshot();
        let spawned_quiver = container_snapshot
            .ground_items
            .iter()
            .find(|ground| ground.item.id == quiver_id)
            .expect("spawned quiver should remain on the ground");
        assert_eq!(spawned_quiver.item.ammunition_containers.len(), 1);
        assert!(
            spawned_quiver.item.ammunition_containers[0]
                .contents
                .is_empty()
        );
        let spawned_arrow = container_snapshot
            .ground_items
            .iter()
            .find(|ground| ground.item.id == arrow_id)
            .expect("spawned arrow should remain on the ground");
        assert_eq!(spawned_arrow.item.type_id, "arrow_wood");
        assert_eq!(spawned_arrow.item.ammunition_type, "arrow");
        assert_eq!(spawned_arrow.item.charges, arrow.default_charges().max(1));
        assert_eq!(
            runtime_battery_magazines,
            [
                ("battery_car", 3_000),
                ("battery_motorbike", 450),
                ("battery_motorbike_small", 225),
                ("cell_phone", 17),
                ("cell_phone_flashlight", 17),
                ("diving_flashlight_variable", 35),
                ("diving_flashlight_variable_on_hi", 35),
                ("diving_flashlight_variable_on_low", 35),
                ("diving_flashlight_variable_on_med", 35),
                ("elec_jackhammer", 7_920),
                ("electric_lantern", 58),
                ("electric_lantern_on", 58),
                ("electric_lighter", 30),
                ("electric_masonrysaw_off", 2_970),
                ("electric_masonrysaw_on", 2_970),
                ("folding_solar_panel_deployed", 1),
                ("folding_solar_panel_v2_deployed", 1),
                ("heavy_atomic_battery_cell", 4_800),
                ("heavy_battery_cell", 259),
                ("heavy_plus_battery_cell", 503),
                ("huge_atomic_battery_cell", 100_000),
                ("large_storage_battery", 100_000),
                ("light_battery_cell", 16),
                ("light_cell_rechargeable", 10),
                ("light_minus_battery_cell", 2),
                ("light_minus_disposable_cell", 2),
                ("medium_battery_cell", 56),
                ("medium_storage_battery", 10_000),
                ("mobile_weather_station", 1_000),
                ("mp3", 50),
                ("mp3_on", 50),
                ("nl_safehouse_boiler", 10_000),
                ("phase_immersion_suit", 7_000),
                ("phase_immersion_suit_on", 7_000),
                ("portable_game", 48),
                ("powered_earplugs", 10),
                ("powered_earplugs_on", 10),
                ("reading_light", 13),
                ("reading_light_on", 13),
                ("rm13_armor", 5_000),
                ("rm13_armor_on", 5_000),
                ("robofac_mobile_weather_station", 1_000),
                ("small_storage_battery", 1_000),
                ("smart_lamp", 33),
                ("smart_lamp_on", 33),
                ("smart_watch", 20),
                ("smart_watch_music", 20),
                ("storage_battery", 50_000),
                ("vibrator", 58),
                ("xedra_mobile_weather_station", 1_000),
            ]
        );
        assert_eq!(
            powered_light_items
                .iter()
                .map(|(item_id, _)| *item_id)
                .collect::<Vec<_>>(),
            [
                "diving_flashlight_small",
                "diving_flashlight_small_hipower",
                "diving_flashlight_small_hipower_on",
                "diving_flashlight_small_on",
                "flashlight",
                "flashlight_on",
                "mipim",
                "mipim_on",
                "mounted_flashlight",
                "mounted_flashlight_on",
                "wearable_big_light",
                "wearable_big_light_on",
                "wearable_light",
                "wearable_light_on",
                "wizard_cane",
                "wizard_cane_cheap",
                "wizard_cane_cheap_on",
                "wizard_cane_on",
            ]
        );
        assert_eq!(
            powered_light_items
                .iter()
                .filter(|(_, projection)| projection.powered_tool.dims_with_charge)
                .map(|(_, projection)| projection.powered_tool.inactive_type_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "diving_flashlight_small",
                "diving_flashlight_small_hipower",
                "flashlight",
                "mipim",
                "mounted_flashlight",
                "wearable_big_light",
                "wearable_light",
                "wizard_cane",
                "wizard_cane_cheap",
            ])
        );
        assert_eq!(recipes.uncraft_count(), 1_428);
        assert_eq!(recipes.uncraft_abstract_count(), 1);
        let ranged_targets = disassembly
            .iter()
            .filter(|(item_type_id, _)| {
                items
                    .get(item_type_id)
                    .is_some_and(|item| item.subtypes.contains("GUN"))
            })
            .filter_map(|(item_type_id, recipe)| {
                recipe.unload_charges_as.as_ref().map(|ammunition| {
                    (
                        item_type_id,
                        ammunition.type_id.as_str(),
                        ammunition.ammunition_type.as_str(),
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ranged_targets,
            [
                ("coilgun", "nail", "nail"),
                ("compositebow", "arrow_wood", "arrow"),
                ("compositecrossbow", "bolt_wood", "bolt"),
            ]
        );
        let integral_tool_targets = disassembly
            .iter()
            .filter(|(item_type_id, recipe)| {
                recipe.unload_charges_as.is_some()
                    && items.get(item_type_id).is_some_and(|item| {
                        item.subtypes.contains("TOOL") && !item.subtypes.contains("GUN")
                    })
            })
            .map(|(item_type_id, recipe)| {
                let output = recipe
                    .unload_charges_as
                    .as_ref()
                    .expect("filtered tool recipe has an unload output");
                (
                    item_type_id,
                    output.type_id.as_str(),
                    output.ammunition_type.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert!(
            integral_tool_targets.is_empty(),
            "the pinned corpus has no default-charged non-pocket tool targets"
        );
        let empty_charge_targets = disassembly
            .iter()
            .filter(|(_, recipe)| recipe.requires_empty_charges)
            .map(|(item_type_id, _)| item_type_id)
            .collect::<Vec<_>>();
        let generalized_detachable_targets = disassembly
            .iter()
            .filter(|(item_type_id, recipe)| {
                !recipe.requires_empty_charges
                    && items.get(item_type_id).is_some_and(|item| {
                        strict_detachable_magazine_well(item, &items).is_some()
                            && strict_detachable_battery_light(item, &items)
                                .expect("power projection should remain valid")
                                .is_none()
                    })
            })
            .map(|(item_type_id, _)| item_type_id)
            .collect::<Vec<_>>();
        let generalized_multi_well_targets = disassembly
            .iter()
            .filter(|(item_type_id, recipe)| {
                !recipe.requires_empty_charges
                    && items.get(item_type_id).is_some_and(|item| {
                        strict_detachable_magazine_wells(item, &items).len() > 1
                    })
            })
            .map(|(item_type_id, _)| item_type_id)
            .collect::<Vec<_>>();
        assert_eq!(
            disassembly
                .iter()
                .filter(|(item_id, _)| {
                    items.get(item_id).is_some_and(|item| {
                        strict_detachable_battery_light(item, &items)
                            .expect("power projection should remain valid")
                            .is_some()
                    })
                })
                .map(|(item_id, recipe)| (item_id, recipe.requires_empty_charges))
                .collect::<Vec<_>>(),
            [
                ("flashlight", false),
                ("wearable_big_light", false),
                ("wearable_light", false),
            ]
        );
        assert_eq!(empty_charge_targets.len(), 51);
        for item_id in ["matches", "ref_matches"] {
            assert!(
                !disassembly
                    .get(item_id)
                    .unwrap_or_else(|| panic!("{item_id} disassembly should remain admitted"))
                    .requires_empty_charges,
                "strict holster-integral storage should make {item_id} charge state recoverable"
            );
        }
        assert_eq!(
            generalized_detachable_targets,
            [
                "acetylene_cooker",
                "circsaw_off",
                "cordless_drill",
                "creepy_doll",
                "crude_firestarter",
                "elec_chainsaw_off",
                "elec_hairtrimmer",
                "electric_blanket",
                "game_watch",
                "heavy_flashlight",
                "mask_filter",
                "mask_gas",
                "oxy_torch",
                "ph_meter",
                "radio_car",
                "radiocontrol",
                "ref_lighter_butane",
                "small_repairkit",
                "soldering_iron_portable",
                "spectrophotometer",
                "talking_doll",
                "two_way_radio",
            ]
        );
        assert_eq!(generalized_multi_well_targets, Vec::<&str>::new());
        assert_eq!(
            empty_charge_targets.len() + generalized_detachable_targets.len(),
            73,
            "the generalized wells must replace, not discard, empty-charge admission"
        );
        assert_eq!(
            disassembly.len(),
            1_161,
            "66 formerly admitted recipes depend on material-backed or perishable temperature state and must remain fail-closed"
        );
        let flashlight_disassembly = disassembly
            .get("flashlight")
            .expect("the canonical detachable-battery tool should remain reversible");
        assert!(!flashlight_disassembly.requires_empty_charges);
        assert!(flashlight_disassembly.unload_charges_as.is_none());
        let (capacity, well) = runtime_magazine_storage(
            items.get("flashlight").expect("flashlight should exist"),
            &items,
        )
        .expect("flashlight storage should normalize");
        assert_eq!(capacity, 0);
        assert_eq!(
            well.first()
                .expect("flashlight should have a canonical battery well")
                .compatible_magazine_type_ids,
            ["medium_battery_cell"]
        );
        assert_eq!(
            runtime_powered_tool(
                items.get("flashlight").expect("flashlight should exist"),
                &items,
            )
            .expect("flashlight power should normalize")
            .expect("flashlight should be a powered tool"),
            PoweredToolStateV1 {
                inactive_type_id: String::from("flashlight"),
                active_type_id: String::from("flashlight_on"),
                activation_charges: 1,
                power_draw_milliwatts: 1_560,
                light_emission: 300,
                dims_with_charge: true,
                power_pocket_index: 0,
                active: false,
            }
        );
        let high_power_well = runtime_magazine_storage(
            items
                .get("diving_flashlight_small_hipower")
                .expect("high-power diving light should exist"),
            &items,
        )
        .expect("high-power diving light storage should normalize")
        .1
        .into_iter()
        .next()
        .expect("high-power diving light should have a magazine well");
        assert_eq!(
            high_power_well.compatible_magazine_type_ids,
            ["light_battery_cell", "light_cell_rechargeable"]
        );
        assert_eq!(
            runtime_magazine_storage(
                items
                    .get("light_battery_cell")
                    .expect("light battery should exist"),
                &items,
            )
            .expect("light battery storage should normalize"),
            (0, Vec::new())
        );
        assert_eq!(
            runtime_integral_magazines(
                items
                    .get("light_battery_cell")
                    .expect("light battery should exist")
            ),
            [IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::new(),
                ammunition_type: String::from("battery"),
                capacity: 16,
                rigid: true,
                reloadable: false,
                unloadable: false,
            }]
        );
        assert!(
            runtime_powered_tool(
                items
                    .get("flashlight_on")
                    .expect("active flashlight should exist"),
                &items,
            )
            .expect("active flashlight power should normalize")
            .is_some_and(|powered| powered.active)
        );
        assert_eq!(
            runtime_powered_tool(
                items
                    .get("diving_flashlight_small_hipower")
                    .expect("high-power diving light should exist"),
                &items,
            )
            .expect("high-power diving light should normalize")
            .expect("high-power diving light should be admitted"),
            PoweredToolStateV1 {
                inactive_type_id: String::from("diving_flashlight_small_hipower"),
                active_type_id: String::from("diving_flashlight_small_hipower_on"),
                activation_charges: 1,
                power_draw_milliwatts: 3_000,
                light_emission: 450,
                dims_with_charge: true,
                power_pocket_index: 0,
                active: false,
            }
        );
        assert_eq!(
            runtime_powered_tool(
                items
                    .get("wizard_cane_cheap")
                    .expect("cheap wizard cane should exist"),
                &items,
            )
            .expect("cheap wizard cane projection should not fail")
            .expect("low-output attenuation should admit the cheap wizard cane"),
            PoweredToolStateV1 {
                inactive_type_id: String::from("wizard_cane_cheap"),
                active_type_id: String::from("wizard_cane_cheap_on"),
                activation_charges: 1,
                power_draw_milliwatts: 1_000,
                light_emission: 4,
                dims_with_charge: true,
                power_pocket_index: 0,
                active: false,
            }
        );
        let wearable_recipe = crafting
            .get("wearable_light")
            .expect("wearable light should remain craftable");
        assert_eq!(
            wearable_recipe.output.powered_tool,
            runtime_powered_tool(
                items
                    .get("wearable_light")
                    .expect("wearable light should exist"),
                &items,
            )
            .expect("wearable light should normalize")
        );
        assert_eq!(
            wearable_recipe
                .output
                .magazine_wells
                .first()
                .expect("crafted wearable light should retain its well")
                .compatible_magazine_type_ids,
            ["medium_battery_cell"]
        );
        let (capacity, well) = runtime_magazine_storage(
            items
                .get("medium_battery_cell")
                .expect("medium battery should exist"),
            &items,
        )
        .expect("medium battery storage should normalize");
        assert_eq!(capacity, 0);
        assert!(well.is_empty());
        assert_eq!(
            runtime_integral_magazines(
                items
                    .get("medium_battery_cell")
                    .expect("medium battery should exist")
            ),
            [IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::new(),
                ammunition_type: String::from("battery"),
                capacity: 56,
                rigid: true,
                reloadable: false,
                unloadable: false,
            }]
        );
        assert_eq!(
            disassembly
                .get("cordage_36")
                .expect("explicit uncraft must override reversible craft")
                .recipe_id,
            "cordage_36"
        );
        let scythe = disassembly
            .get("makeshift_scythe_war")
            .expect("pinned reversible singleton-component recipe should normalize");
        assert_eq!(scythe.recipe_id, "makeshift_scythe_war");
        assert_eq!(scythe.target_type_id, "makeshift_scythe_war");
        assert!(!scythe.components.is_empty());
        let rock_sock = disassembly
            .get("rock_sock")
            .expect("a reversible craft with alternatives should use pinned defaults");
        assert_eq!(rock_sock.components.len(), 2);
        assert_eq!(rock_sock.components[0].output.type_id, "rock");
        assert_eq!(rock_sock.components[1].output.type_id, "socks");
        let rock_sock_craft = crafting
            .get("rock_sock")
            .expect("pinned reversible craft should normalize");
        assert!(rock_sock_craft.retain_components);
        assert_eq!(rock_sock_craft.components[1].len(), 2);
        let crossbow = items.get("crossbow").expect("crossbow should load");
        let crossbow_prototype = craft_item_prototype(
            crossbow,
            default_instance_charges(crossbow),
            &items,
            &materials,
        )
        .expect("integral crossbow should normalize");
        assert_eq!(crossbow_prototype.charges, 0);
        assert_eq!(crossbow_prototype.integral_magazines.len(), 1);
        for (item_type_id, recipe) in disassembly.iter() {
            let target = items
                .get(item_type_id)
                .expect("disassembly target must be a selected item");
            let unload_category = if target.subtypes.contains("GUN") {
                target.ammo.first()
            } else if target.subtypes.contains("TOOL") && target.default_charges() > 0 {
                target.tool_ammunition.first()
            } else {
                None
            };
            assert_eq!(
                recipe.unload_charges_as.is_some(),
                unload_category.is_some(),
                "only supported bare ranged or integral-tool targets carry an unload prototype"
            );
            if let Some(unloaded) = &recipe.unload_charges_as {
                assert!(!recipe.requires_empty_charges);
                assert_eq!(unload_category, Some(&unloaded.ammunition_type));
                assert!(unloaded.charges > 0);
                assert!(unloaded.ranged_weapon.is_none());
            } else {
                assert_eq!(
                    target.default_charges(),
                    0,
                    "charged catalog targets require canonical unloading"
                );
            }
            if recipe.requires_empty_charges {
                assert!(target.subtypes.contains("TOOL"));
                assert!(!target.subtypes.contains("GUN"));
                assert!(!target.tool_ammunition.is_empty());
            }
            assert_eq!(
                recipes
                    .strict_disassembly_recipe_for_result(item_type_id, &items, &ammunition)
                    .map(|candidate| candidate.id.as_str()),
                Some(recipe.recipe_id.as_str()),
                "client-side strict eligibility must match the authoritative catalog"
            );
            encode_control(&ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(1, 1),
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::Disassemble {
                    item_id: cdda_protocol::ItemId::new(1, 2),
                    item_type_id: item_type_id.to_owned(),
                    recipe: Some(Box::new(recipe.clone())),
                },
            }))
            .unwrap_or_else(|error| {
                panic!(
                    "disassembly recipe {item_type_id} must fit a control frame: {error}; {recipe:#?}"
                )
            });
        }
        let client_visible = items
            .iter()
            .filter_map(|(item_type_id, _)| {
                recipes
                    .strict_disassembly_recipe_for_result(item_type_id, &items, &ammunition)
                    .map(|recipe| (item_type_id, recipe.id.as_str()))
            })
            .collect::<Vec<_>>();
        assert_eq!(client_visible.len(), disassembly.len());
        for (item_type_id, recipe_id) in client_visible {
            assert_eq!(
                disassembly
                    .get(item_type_id)
                    .map(|recipe| recipe.recipe_id.as_str()),
                Some(recipe_id),
                "the client must not expose a recipe absent from the server catalog"
            );
        }
        let reading = build_reading_catalog(&items, &skills).expect("skill books should normalize");
        assert_eq!(reading.len(), 197);
        assert_eq!(
            reading.get("manual_pistol"),
            Some(&BookStudyV1 {
                book_type_id: String::from("manual_pistol"),
                skill_id: String::from("pistol"),
                required_skill_level: 0,
                maximum_skill_level: 3,
                intelligence_requirement: 3,
                time_moves: 15 * 60 * 100,
                source_time_minutes: 15,
            })
        );
        let no_materials = MaterialRegistry::default();
        let pre_material_crafting =
            build_crafting_catalog(&recipes, &items, &no_materials, &proficiencies)
                .expect("the prior materialless crafting boundary should remain measurable");
        assert_eq!(pre_material_crafting.len(), 2_629);
        let pre_material_recipe_ids = pre_material_crafting
            .iter()
            .map(|(recipe_id, _)| recipe_id)
            .collect::<BTreeSet<_>>();
        let newly_admitted_material_recipes = crafting
            .iter()
            .filter(|(recipe_id, _)| !pre_material_recipe_ids.contains(recipe_id))
            .map(|(recipe_id, recipe)| {
                assert!(
                    recipe.output.thermal_properties.is_some()
                        || recipe
                            .byproducts
                            .iter()
                            .any(|byproduct| byproduct.output.thermal_properties.is_some()),
                    "new recipe {recipe_id} must be attributable to represented material thermodynamics"
                );
                recipe_id
            })
            .collect::<Vec<_>>();
        assert_custom_freezing_recipe_admission(&crafting, &pre_material_recipe_ids);
        assert_eq!(newly_admitted_material_recipes.len(), 220);
        assert_eq!(
            crafting.len(),
            2_849,
            "exactly 197 default-freezing and 23 custom-freezing recipes should cross the represented material-thermodynamics boundary; rot and overlapping unsupported semantics remain closed"
        );
        let mut maximum_encoded_recipe = (0_usize, "");
        for (recipe_id, recipe) in crafting.iter() {
            let encoded = encode_control(&ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(1, 1),
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::Craft {
                    recipe_id: recipe_id.to_owned(),
                    recipe: Some(Box::new(recipe.clone())),
                },
            }))
            .unwrap_or_else(|error| {
                panic!("normalized recipe {recipe_id} is invalid: {error}; {recipe:#?}")
            });
            if encoded.len() > maximum_encoded_recipe.0 {
                maximum_encoded_recipe = (encoded.len(), recipe_id);
            }
        }
        assert!(
            maximum_encoded_recipe.0 <= cdda_protocol::MAX_CONTROL_ENCODED / 2,
            "largest recipe {} uses {} bytes and leaves insufficient private-inspection headroom",
            maximum_encoded_recipe.1,
            maximum_encoded_recipe.0
        );
        let rock_sock = crafting
            .get("rock_sock")
            .expect("foundational rock-in-a-sock recipe should be runnable");
        assert_eq!(rock_sock.time_moves, 500);
        assert_eq!(rock_sock.output.type_id, "rock_sock");
        assert_eq!(
            rock_sock.primary_skill,
            Some(CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 0,
            })
        );
        let anvil = crafting
            .get("anvil_bronze")
            .expect("pinned bronze anvil proficiency recipe should be runnable");
        assert_eq!(
            anvil
                .proficiencies
                .iter()
                .map(|proficiency| (
                    proficiency.proficiency_id.as_str(),
                    proficiency.required,
                    proficiency.time_multiplier_millionths,
                    proficiency.skill_penalty_millionths,
                    proficiency.time_to_learn_action_points,
                    proficiency.required_proficiencies.as_slice(),
                ))
                .collect::<Vec<_>>(),
            vec![
                ("prof_metalworking", true, 0, 0, 14_400_000, &[][..]),
                (
                    "prof_redsmithing",
                    true,
                    0,
                    0,
                    28_800_000,
                    &[String::from("prof_metalworking")][..],
                ),
                (
                    "prof_redsmithing_adv",
                    false,
                    2_000_000,
                    250_000,
                    43_200_000,
                    &[String::from("prof_redsmithing")][..],
                ),
            ]
        );
        assert_eq!(
            rock_sock.autolearn_skills,
            vec![CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 0,
            }]
        );
        assert!(rock_sock.autolearn);
        let paper_cartridge = crafting
            .get("36navy")
            .expect("pinned book-only recipe should be server-authorized");
        assert!(!paper_cartridge.autolearn);
        assert!(paper_cartridge.autolearn_skills.is_empty());
        assert!(
            !paper_cartridge.book_requirements.is_empty(),
            "book-only knowledge must be journaled in the normalized recipe"
        );
        assert!(
            paper_cartridge
                .book_requirements
                .windows(2)
                .all(|pair| pair[0].book_type_id < pair[1].book_type_id)
        );
        assert_eq!(
            paper_cartridge
                .book_requirements
                .iter()
                .map(|requirement| (
                    requirement.book_type_id.as_str(),
                    requirement.required_skill_level,
                ))
                .collect::<Vec<_>>(),
            vec![("manual_pistol", 1), ("recipe_bullets", 3)]
        );
        assert_eq!(rock_sock.output_instances, 1);
        assert_eq!(rock_sock.components.len(), 2);
        assert_eq!(rock_sock.components[0][0].type_id, "rock");
        assert_eq!(
            rock_sock.components[1]
                .iter()
                .map(|component| component.type_id.as_str())
                .collect::<Vec<_>>(),
            vec!["socks", "socks_wool"]
        );
        assert!(
            crafting.get("V8").is_none(),
            "the parser retains V8's exact LIST alternatives, but its perishable material-backed result must not enter the runtime catalog"
        );
        let makeshift_cards = crafting
            .get("deck_of_cards_deck_of_cards_makeshift")
            .expect("pinned tool LIST recipe should normalize into concrete alternatives");
        assert_eq!(
            makeshift_cards.tools[0]
                .iter()
                .map(|tool| (tool.type_id.as_str(), tool.amount, tool.consumes_charges))
                .collect::<Vec<_>>(),
            vec![
                ("pen", 5, true),
                ("black_pen", 5, true),
                ("blue_pen", 5, true),
                ("green_pen", 5, true),
                ("red_pen", 5, true),
                ("pencil", 5, true),
                ("permanent_marker", 5, true),
                ("survival_marker", 5, true),
            ]
        );
        let sawn_lumber = crafting
            .get("2x4_from logs")
            .expect("pinned legacy byproduct recipe should normalize");
        assert_eq!(sawn_lumber.byproducts.len(), 1);
        assert_eq!(sawn_lumber.byproducts[0].output.type_id, "splinter");
        assert_eq!(sawn_lumber.byproducts[0].output_instances, 10);
        assert_eq!(sawn_lumber.byproducts[0].output.charges, 1);
        assert!(
            crafting.get("milk_cream").is_none(),
            "cream and its charged buttermilk byproduct require the later material/rot engine"
        );
        let pointy_stick = crafting
            .get("pointy_stick")
            .expect("inherent CUT quality recipe should be runnable");
        assert_eq!(pointy_stick.qualities.len(), 1);
        assert_eq!(pointy_stick.qualities[0][0].quality_id, "CUT");
        assert_eq!(pointy_stick.qualities[0][0].level, 1);
        assert!(
            pointy_stick.qualities[0][0]
                .providers
                .binary_search_by(|provider| provider.type_id.as_str().cmp("knife_small"))
                .is_ok()
        );
        assert!(
            pointy_stick.qualities[0][0]
                .providers
                .binary_search_by(|provider| provider.type_id.as_str().cmp("circsaw_on"))
                .is_ok(),
            "circsaw_on CUT 1 at speed 0.5 remains valid for a legacy recipe"
        );
        assert!(
            crafting.get("toasterpastry_with_toaster").is_none(),
            "charged-tool recipe parsing remains characterized, but the pastry result requires material/rot state"
        );
        let suppressor = crafting
            .get("crafted_suppressor")
            .expect("charged DRILL quality should expose the pinned suppressor recipe");
        let drill = suppressor
            .qualities
            .iter()
            .flatten()
            .find(|quality| quality.quality_id == "DRILL")
            .expect("pinned suppressor should retain DRILL 3");
        let cordless_drill = drill
            .providers
            .iter()
            .find(|provider| provider.type_id == "cordless_drill")
            .expect("cordless drill should provide charged DRILL 3");
        assert_eq!(cordless_drill.minimum_charges, 5);
    }

    #[test]
    fn replay_archive_publication_is_verified_idempotent_private_and_retained() {
        let directory = std::env::temp_dir().join(format!(
            "cdda-rust-replay-archive-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let snapshot_object_directory = directory.join("snapshot-objects");
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(95, [7; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(95, [7; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("anchor snapshot should write");
        let start_utc = REPLAY_ARCHIVE_RETENTION_SECONDS + 100;
        store
            .initialize_replay_archive_cursor(0, start_utc)
            .expect("archive cursor should initialize");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("world should advance");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("journal should append");
        store
            .write_snapshot(sequence, &world)
            .expect("end snapshot should write");
        let end_utc = start_utc + REPLAY_ARCHIVE_INTERVAL_SECONDS;
        let content = cdda_protocol::ContentIdentity {
            baseline_commit: cdda_protocol::BASELINE_COMMIT.to_owned(),
            manifest_hash: [25; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let prepared = store
            .prepare_replay_archive(sequence, end_utc, content.clone())
            .expect("archive should prepare")
            .expect("archive should be due");
        prepare_private_directory(&directory).expect("archive directory should create");
        let old_name = replay_archive_filename(
            ReplayArchiveCursor {
                journal_sequence: 0,
                security_audit_sequence: 0,
                archived_utc_seconds: 0,
            },
            ReplayArchiveCursor {
                journal_sequence: 1,
                security_audit_sequence: 0,
                archived_utc_seconds: 1,
            },
        );
        let old_path = directory.join(old_name);
        fs::write(&old_path, b"expired").expect("old archive fixture should write");
        let unrelated = directory.join("keep-me.txt");
        fs::write(&unrelated, b"unrelated").expect("unrelated fixture should write");
        prepare_private_directory(&snapshot_object_directory)
            .expect("snapshot-object directory should create");
        let orphan = snapshot_object_directory.join(snapshot_object_filename([91; 32]));
        write_private_file(&orphan, b"unreferenced object")
            .expect("orphan object fixture should write");
        let unrelated_object = snapshot_object_directory.join("keep-me-too.txt");
        write_private_file(&unrelated_object, b"unrelated")
            .expect("unrelated object fixture should write");
        let noncanonical_object = snapshot_object_directory.join(format!(
            "snapshot-{}.cddasnap",
            blake3::Hash::from_bytes([0xab; 32])
                .to_string()
                .to_ascii_uppercase()
        ));
        write_private_file(&noncanonical_object, b"noncanonical lookalike")
            .expect("noncanonical object fixture should write");

        let first = write_replay_archive(&directory, &snapshot_object_directory, prepared.clone())
            .expect("archive should verify and publish");
        let second = write_replay_archive(&directory, &snapshot_object_directory, prepared)
            .expect("deterministic retry should accept identical bytes");
        assert_eq!(first.path, second.path);
        assert_eq!(first.checksum, second.checksum);
        assert_eq!(first.snapshot_object_path, second.snapshot_object_path);
        assert!(first.snapshot_object_path.is_file());
        assert_eq!(
            first.snapshot_gc,
            SnapshotObjectGc {
                retained_archives: 1,
                retained_objects: 1,
                removed_objects: 1,
            }
        );
        assert_eq!(second.snapshot_gc.removed_objects, 0);
        assert!(!orphan.exists());
        assert!(unrelated_object.exists());
        assert!(noncanonical_object.exists());
        assert!(!old_path.exists());
        assert!(unrelated.exists());
        assert_eq!(
            *blake3::hash(&fs::read(&first.path).expect("archive should read")).as_bytes(),
            first.checksum
        );
        let collision_name = "replay-00000000000000000001-00000000000000000002-00000000000000000003-00000000000000000004.cddar";
        write_private_file(&directory.join(collision_name), &[0_u8; 4_096])
            .expect("collision fixture should write");
        assert!(publish_replay_archive(&directory, collision_name, b"small").is_err());
        let fail_closed_orphan = snapshot_object_directory.join(snapshot_object_filename([92; 32]));
        write_private_file(
            &fail_closed_orphan,
            b"preserve when references cannot be proven",
        )
        .expect("fail-closed orphan fixture should write");
        assert!(
            garbage_collect_snapshot_objects(&directory, &snapshot_object_directory, &content,)
                .is_err()
        );
        assert!(fail_closed_orphan.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            use std::os::unix::fs::symlink;
            assert_eq!(
                fs::metadata(&first.path)
                    .expect("archive metadata should read")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&first.snapshot_object_path)
                    .expect("snapshot-object metadata should read")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            let symlink_name = "replay-00000000000000000005-00000000000000000006-00000000000000000007-00000000000000000008.cddar";
            symlink(&unrelated, directory.join(symlink_name))
                .expect("symlink fixture should create");
            assert!(publish_replay_archive(&directory, symlink_name, b"unrelated").is_err());
        }
        fs::remove_dir_all(directory).expect("archive fixtures should clean up");
    }

    #[test]
    fn online_backup_binds_database_content_and_private_iroh_identity() {
        let directory = std::env::temp_dir().join(format!(
            "cdda-rust-backup-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        prepare_private_directory(&directory).expect("backup fixture directory should create");
        let mut store =
            WorldStore::open(directory.join("source-world.db")).expect("on-disk store should open");
        store
            .initialize_world(96, [8; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(96, [8; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("snapshot should write");
        let persistence_host = PersistenceHost::start(store).expect("worker should start");
        let persistence = persistence_host.handle();
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [28; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let secret_key_bytes = [42; 32];
        let endpoint = iroh::SecretKey::from_bytes(&secret_key_bytes)
            .public()
            .to_string();
        let backup =
            write_backup_generation(&directory, &persistence, secret_key_bytes, &content, 10_000)
                .expect("backup should publish");
        let manifest = verify_backup_generation(&backup.path, &content, &endpoint)
            .expect("published backup should verify");
        assert_eq!(manifest.journal_sequence, backup.metadata.journal_sequence);
        assert_eq!(manifest.tick, backup.metadata.tick.0);
        assert_eq!(manifest.server_endpoint_id, endpoint);
        let fake_future = directory.join(backup_generation_name(99_999, u64::MAX));
        fs::create_dir(&fake_future).expect("fake future generation should create");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fake_future, fs::Permissions::from_mode(0o700))
                .expect("fake future permissions should set");
        }
        for member in ["world.db", "server-identity.key", "manifest.json"] {
            fs::copy(backup.path.join(member), fake_future.join(member))
                .expect("fake future member should copy");
        }
        assert_eq!(
            latest_backup_utc(&directory, &content, &endpoint)
                .expect("valid backup discovery should work"),
            Some(10_000),
            "an unverified future-looking directory must not postpone backups"
        );
        let restored_world = directory.join("restored-world");
        let restored = restore_backup_generation(&backup.path, &restored_world, &content)
            .expect("verified backup should restore into a new world");
        assert_eq!(restored, manifest);
        assert_eq!(
            iroh::SecretKey::from_bytes(
                &fs::read(restored_world.join("server-identity.key"))
                    .expect("restored identity should read")
                    .try_into()
                    .expect("restored identity should be exact")
            )
            .public()
            .to_string(),
            endpoint
        );
        fs::hard_link(
            restored_world.join("manifest.json"),
            restored_world.join(RESTORE_PROVENANCE_FILE),
        )
        .expect("provenance crash-window fixture should link");
        verify_restored_world_identity(&restored_world, &content, secret_key_bytes, &endpoint)
            .expect("first restored startup should consume verified provenance");
        assert!(!restored_world.join("manifest.json").exists());
        assert!(restored_world.join(RESTORE_PROVENANCE_FILE).is_file());
        verify_restored_world_identity(&restored_world, &content, secret_key_bytes, &endpoint)
            .expect("later startup should verify durable provenance");
        assert!(
            verify_restored_world_identity(&restored_world, &content, [44; 32], &endpoint).is_err(),
            "a restored world must reject a different server key"
        );
        assert!(
            restore_backup_generation(&backup.path, &restored_world, &content).is_err(),
            "restore must not overwrite an existing world"
        );

        let legacy_parent = directory.join("legacy-source");
        let legacy_generation = legacy_parent.join(backup_generation_name(
            manifest.created_utc_seconds,
            manifest.journal_sequence,
        ));
        prepare_private_directory(&legacy_generation)
            .expect("legacy generation fixture should create");
        for member in ["world.db", "server-identity.key", "manifest.json"] {
            fs::copy(backup.path.join(member), legacy_generation.join(member))
                .expect("legacy generation member should copy");
        }
        {
            let legacy_database = rusqlite::Connection::open(legacy_generation.join("world.db"))
                .expect("legacy database should open");
            legacy_database
                .execute(
                    "DELETE FROM schema_migrations WHERE version = ?1",
                    [SCHEMA_VERSION],
                )
                .expect("legacy schema marker should downgrade");
            legacy_database
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .expect("legacy fixture WAL should checkpoint");
        }
        let mut legacy_manifest = manifest.clone();
        legacy_manifest.schema_version = SCHEMA_VERSION - 1;
        legacy_manifest.protocol_version = PROTOCOL_VERSION - 1;
        legacy_manifest.database_checksum = blake3::Hash::from_bytes(
            hash_regular_file(&legacy_generation.join("world.db"))
                .expect("legacy database should hash"),
        )
        .to_string();
        fs::write(
            legacy_generation.join("manifest.json"),
            serde_json::to_vec_pretty(&legacy_manifest).expect("legacy manifest should serialize"),
        )
        .expect("legacy manifest should write");
        let legacy_restored_world = directory.join("legacy-restored-world");
        assert!(
            restore_backup_generation(&legacy_generation, &legacy_restored_world, &content)
                .is_err(),
            "a backup with an incompatible Postcard schema must fail before restore"
        );
        assert!(!legacy_restored_world.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(backup.path.join("server-identity.key"))
                    .expect("identity metadata should read")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&backup.path)
                    .expect("generation metadata should read")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        fs::write(backup.path.join("server-identity.key"), [43_u8; 32])
            .expect("identity tamper fixture should write");
        assert!(verify_backup_generation(&backup.path, &content, &endpoint).is_err());
        persistence_host.shutdown();
        fs::remove_dir_all(directory).expect("backup fixtures should clean up");
    }

    #[test]
    fn backup_retention_keeps_24_newest_and_30_older_daily_generations() {
        let generations = (1_i64..=60)
            .rev()
            .map(|day| {
                (
                    day * 86_400,
                    day as u64,
                    PathBuf::from(backup_generation_name(day * 86_400, day as u64)),
                )
            })
            .collect::<Vec<_>>();
        let current = generations[0].2.clone();
        let kept = retained_backup_paths(&generations, &current);
        assert_eq!(kept.len(), BACKUP_HOURLY_RETENTION + BACKUP_DAILY_RETENTION);
        assert!(kept.contains(&current));
        assert!(kept.contains(&generations[BACKUP_HOURLY_RETENTION].2));
        assert!(!kept.contains(&generations.last().expect("oldest exists").2));
    }

    #[test]
    fn durable_command_receipt_is_released_after_sqlite_commit() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(91, [3; 32])
            .expect("world should initialize");
        store
            .begin_runtime(utc_now_seconds().expect("clock should work"))
            .expect("runtime should begin");
        let block = store.reserve_id_block().expect("block should reserve");
        let persistence_host =
            PersistenceHost::start(store).expect("persistence host should start");
        let persistence = persistence_host.handle();
        let mut world = WorldState::new(91, [3; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let host = SimulationHost::start(world).expect("host should start");
        let receipt = host
            .handle()
            .submit_durable(ClientCommand {
                actor_id,
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::Move {
                    dx: 1,
                    dy: 0,
                    dz: 0,
                },
            })
            .expect("command should submit");

        std::thread::sleep(Duration::from_millis(130));
        let mut pending = PendingJournal::default();
        let mut sequence = 0;
        drain_outputs(&host, &persistence, &mut pending, &mut sequence)
            .expect("outputs should persist");
        flush_journal(&persistence, &mut pending, &mut sequence)
            .expect("partial batch should flush");
        let committed_tick = receipt
            .wait(Duration::from_secs(1))
            .expect("receipt should be released");
        assert!(committed_tick.0 >= 1);
        assert_eq!(
            persistence
                .journal_after(0)
                .expect("journal should query")
                .len(),
            1
        );
        assert_eq!(host.shutdown(), cdda_server::SimulationExit::Requested);
        persistence_host.shutdown();
    }

    #[test]
    fn clean_shutdown_disconnects_every_actor_before_snapshot() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(92, [4; 32])
            .expect("world should initialize");
        store
            .begin_runtime(utc_now_seconds().expect("clock should work"))
            .expect("runtime should begin");
        let persistence_host =
            PersistenceHost::start(store).expect("persistence host should start");
        let persistence = persistence_host.handle();
        let mut world = WorldState::new(92, [4; 32]);
        world
            .install_reserved_block(cdda_sim::ReservedIdBlock { start: 1, end: 16 })
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("first actor should spawn");
        world
            .spawn_actor(WorldPosition { x: 2, y: 1, z: 0 }, true)
            .expect("second actor should spawn");
        let host = SimulationHost::start(world).expect("host should start");

        let boundary = disconnect_all_actors(&host.handle()).expect("actors should disconnect");
        let mut pending = PendingJournal::default();
        let mut sequence = 0;
        drain_through_next_tick(&host, &persistence, &mut pending, &mut sequence, boundary)
            .expect("disconnect boundary should persist");
        let snapshot = host
            .handle()
            .snapshot(Duration::from_secs(1))
            .expect("snapshot should arrive");
        assert!(snapshot.actors.iter().all(|actor| !actor.connected));
        let connection_updates = persistence
            .journal_after(0)
            .expect("journal should query")
            .into_iter()
            .flat_map(|(_sequence, batch)| batch.ticks)
            .flat_map(|tick| tick.connection_updates)
            .collect::<Vec<_>>();
        assert_eq!(connection_updates.len(), 2);
        assert!(connection_updates.iter().all(|update| !update.connected));
        assert_eq!(host.shutdown(), cdda_server::SimulationExit::Requested);
        persistence_host.shutdown();
    }

    #[test]
    fn unexpected_downtime_is_journaled_as_replayable_commandless_ticks() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(94, [6; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(94, [6; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 }, true)
            .expect("connected actor should spawn");
        world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_slow_test"),
                position: cdda_protocol::WorldPosition { x: 1, y: 0, z: 0 },
                hp: 20,
                speed: 1,
                attack_cost_moves: 100,
                aggression: 100,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 1,
                melee_dice_sides: 1,
                can_see: true,
                vision_day: 60,
                vision_night: 60,
                stumbles: false,
                bashes: false,
                group_bash: false,
                hears: false,
                good_hearing: false,
                clumsy_attacks: false,
                immobile: false,
                pacifist: false,
                can_open_doors: false,
                path_settings: Default::default(),
                blood_field_type_id: String::new(),
                corpse: None,
            })
            .expect("hostile creature should spawn");
        store
            .write_snapshot(0, &world)
            .expect("connected pre-crash snapshot should write");
        store
            .begin_runtime(100)
            .expect("first runtime should begin");
        let restart = store
            .begin_runtime(105)
            .expect("unclean restart should be detected");
        assert!(restart.previous_exit_was_unclean);
        let mut recovery_connection_updates = world.disconnect_all_for_recovery();
        assert_eq!(
            recovery_connection_updates,
            vec![ActorConnectionUpdateV1 {
                actor_id,
                connected: false,
            }]
        );
        let mut journal_sequence = 0;
        apply_unexpected_downtime(
            &mut store,
            &mut world,
            &mut journal_sequence,
            restart,
            &mut recovery_connection_updates,
        )
        .expect("catch-up should apply");
        assert!(recovery_connection_updates.is_empty());
        assert_eq!(world.tick(), SimTick(100));
        assert_eq!(journal_sequence, 1);
        let (_sequence, replayed) = store
            .recover_latest(WorldState::new(94, [6; 32]))
            .expect("catch-up journal should replay");
        assert_eq!(
            replayed.canonical_hash().expect("replayed hash"),
            world.canonical_hash().expect("catch-up hash")
        );
        assert_eq!(
            replayed
                .actor_snapshot(actor_id)
                .expect("actor should recover")
                .position,
            world
                .actor_snapshot(actor_id)
                .expect("live actor should remain")
                .position
        );
    }

    #[test]
    fn checkpoint_snapshot_sequence_matches_its_exact_tick_boundary() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(93, [5; 32])
            .expect("world should initialize");
        store
            .begin_runtime(utc_now_seconds().expect("clock should work"))
            .expect("runtime should begin");
        let block = store.reserve_id_block().expect("block should reserve");
        let persistence_host =
            PersistenceHost::start(store).expect("persistence host should start");
        let persistence = persistence_host.handle();
        let mut world = WorldState::new(93, [5; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let host = SimulationHost::start(world).expect("host should start");
        std::thread::sleep(Duration::from_millis(180));
        let mut pending = PendingJournal::default();
        let mut sequence = 0;
        let snapshot = queue_checkpoint_world(
            &host,
            &host.handle(),
            &persistence,
            &mut pending,
            &mut sequence,
        )
        .expect("checkpoint should commit");
        assert_eq!(
            snapshot.wait().expect("snapshot should write"),
            SnapshotWriteOutcome::Written
        );
        let journals = persistence.journal_after(0).expect("journal should read");
        let (stored_sequence, stored_world) = persistence
            .latest_snapshot()
            .expect("snapshot should read")
            .expect("snapshot should exist");
        let (last_sequence, last_batch) = journals.last().expect("journal should contain ticks");
        assert_eq!(stored_sequence, *last_sequence);
        assert_eq!(stored_sequence, sequence);
        assert_eq!(
            stored_world.tick(),
            last_batch.ticks.last().expect("batch contains a tick").tick
        );
        assert_eq!(host.shutdown(), cdda_server::SimulationExit::Requested);
        persistence_host.shutdown();
    }
}
