use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cdda_conformance::{
    ScenarioActorSpawnV1, ScenarioActorV1, ScenarioCommandV1, ScenarioExpectationV1,
    ScenarioGeneratedItemSelectorV1, ScenarioGeneratedItemV1, ScenarioMode, ScenarioStepV1,
    ScenarioV1, run_scenario,
};
use cdda_content::{
    CitySettingsDefinition, DefaultRegionTerrainFurnitureRegistry, FurnitureRegistry,
    ItemGroupRegistry, MapgenRegistry, OvermapTerrainRegistry, StartLocationRegistry,
    TerrainRegistry,
};
use cdda_persistence::{ReplayBundleV1, WorldStore};
use cdda_protocol::{
    AccountId, AccountRole, ActorId, CharacterRequest, ClientCommand, ClientHello, CommandKind,
    CommandSequence, ContentIdentity, ControlMessage, EndpointIdentity, GAME_ALPN,
    ReplicationSnapshotV1, SUBMAP_SIZE, SimTick, WorldPosition, WorldSnapshotV1,
};
use cdda_server::{
    AuthorizationChangeHub, ChatHub, CommittedEventHub, PersistenceHandle, PersistenceHost,
    SessionRegistry, SimulationHost, character_creation_channel,
    handle_game_connection_with_sessions, read_control_frame, read_snapshot_stream,
    write_control_frame,
};
use cdda_sim::WorldState;
use iroh::{Endpoint, EndpointAddr, SecretKey, endpoint::presets};

use super::item_groups::{RuntimeItemGroupContent, runtime_named_item_group_catalogs};
use super::worldgen::{
    RuntimeMapgenContent, bootstrap_regional_road_overmap, runtime_mapgen_item_group_roots,
    runtime_mapgen_worldgen,
};
use super::{PendingJournal, flush_journal, record_simulation_output, utc_now_seconds};

static NEXT_FIELD_DATABASE: AtomicU64 = AtomicU64::new(1);

struct TemporaryFieldDatabase(PathBuf);

impl TemporaryFieldDatabase {
    fn new() -> Self {
        let sequence = NEXT_FIELD_DATABASE.fetch_add(1, Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "cdda-regional-field-{}-{sequence}.db",
            std::process::id()
        )))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryFieldDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let path = PathBuf::from(format!("{}{suffix}", self.0.display()));
            if let Err(error) = fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                panic!("failed to remove regional-field database: {error}");
            }
        }
    }
}

struct ConnectedFieldClient {
    endpoint: Endpoint,
    connection: iroh::endpoint::Connection,
    send: iroh::endpoint::SendStream,
    _control_receive: iroh::endpoint::RecvStream,
    _event_receive: iroh::endpoint::RecvStream,
    snapshot: ReplicationSnapshotV1,
}

async fn connect_field_client(
    secret: SecretKey,
    server_address: EndpointAddr,
    content: ContentIdentity,
    actor_id: ActorId,
) -> ConnectedFieldClient {
    let endpoint = Endpoint::builder(presets::N0DisableRelay)
        .secret_key(secret)
        .bind()
        .await
        .expect("field client endpoint should bind");
    let connection = endpoint
        .connect(server_address, GAME_ALPN)
        .await
        .expect("field client should connect");
    let (mut send, mut control_receive) = connection
        .open_bi()
        .await
        .expect("field client control stream should open");
    write_control_frame(
        &mut send,
        &ControlMessage::ClientHello(ClientHello {
            protocol_version: cdda_protocol::PROTOCOL_VERSION,
            content,
        }),
    )
    .await
    .expect("field client hello should send");
    assert!(matches!(
        read_control_frame(&mut control_receive)
            .await
            .expect("field server hello should decode"),
        ControlMessage::ServerHello(_)
    ));
    let ControlMessage::CharacterList(characters) = read_control_frame(&mut control_receive)
        .await
        .expect("field character list should decode")
    else {
        panic!("field server must send a character list");
    };
    assert_eq!(characters.len(), 1);
    assert_eq!(characters[0].actor_id, actor_id);
    write_control_frame(
        &mut send,
        &ControlMessage::CharacterRequest(CharacterRequest::Select { actor_id }),
    )
    .await
    .expect("field character selection should send");
    assert_eq!(
        read_control_frame(&mut control_receive)
            .await
            .expect("field character should become ready"),
        ControlMessage::CharacterReady { actor_id }
    );
    let mut event_receive = connection
        .accept_uni()
        .await
        .expect("field event stream should open");
    assert_eq!(
        read_control_frame(&mut event_receive)
            .await
            .expect("field event stream header should decode"),
        ControlMessage::EventStreamReady { actor_id }
    );
    let mut snapshot_receive = connection
        .accept_uni()
        .await
        .expect("field snapshot stream should open");
    let (snapshot_actor, snapshot_sequence, snapshot) = read_snapshot_stream(&mut snapshot_receive)
        .await
        .expect("field snapshot should decode");
    assert_eq!(snapshot_actor, actor_id);
    assert_eq!(snapshot_sequence, 0);
    assert_eq!(snapshot.controlled_actor.id, actor_id);
    ConnectedFieldClient {
        endpoint,
        connection,
        send,
        _control_receive: control_receive,
        _event_receive: event_receive,
        snapshot,
    }
}

async fn read_field_snapshot_until(
    connection: &iroh::endpoint::Connection,
    actor_id: ActorId,
    sequence: CommandSequence,
    predicate: impl Fn(&ReplicationSnapshotV1) -> bool,
) -> ReplicationSnapshotV1 {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let mut receive = connection
                .accept_uni()
                .await
                .expect("field snapshot update should arrive");
            let (stream_actor, _snapshot_sequence, snapshot) = read_snapshot_stream(&mut receive)
                .await
                .expect("field snapshot update should decode");
            if stream_actor == actor_id
                && snapshot.controlled_actor.id == actor_id
                && snapshot.controlled_actor.last_command_sequence >= sequence
                && predicate(&snapshot)
            {
                break snapshot;
            }
        }
    })
    .await
    .expect("field snapshot predicate should become true")
}

fn start_field_persistence_pump(
    host: SimulationHost,
    persistence: PersistenceHandle,
    event_hub: CommittedEventHub,
) -> (SyncSender<()>, JoinHandle<(SimulationHost, u64)>) {
    let (stop, stop_receiver) = mpsc::sync_channel(1);
    let pump = thread::spawn(move || {
        let mut pending = PendingJournal {
            event_hub,
            ..PendingJournal::default()
        };
        let mut journal_sequence = 0;
        loop {
            if stop_receiver.try_recv().is_ok() {
                break;
            }
            match host.recv_timeout(Duration::from_millis(20)) {
                Ok(output) => {
                    record_simulation_output(
                        output,
                        &persistence,
                        &mut pending,
                        &mut journal_sequence,
                    )
                    .expect("field simulation output should persist");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("field simulation stopped before its checkpoint")
                }
            }
        }
        loop {
            match host.try_recv() {
                Ok(output) => {
                    record_simulation_output(
                        output,
                        &persistence,
                        &mut pending,
                        &mut journal_sequence,
                    )
                    .expect("remaining field simulation output should persist");
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("field simulation stopped while draining")
                }
            }
        }
        flush_journal(&persistence, &mut pending, &mut journal_sequence)
            .expect("field journal tail should persist");
        (host, journal_sequence)
    });
    (stop, pump)
}

fn is_passable_pavement(snapshot: &WorldSnapshotV1, position: WorldPosition) -> bool {
    let (chunk_coord, local) = position.chunk_and_local();
    let Some(chunk) = snapshot
        .chunks
        .iter()
        .find(|chunk| chunk.coord == chunk_coord)
    else {
        return false;
    };
    let index = usize::from(local.y) * SUBMAP_SIZE as usize + usize::from(local.x);
    chunk.tiles.get(index).is_some_and(|terrain| {
        terrain.terrain_id.starts_with("t_pavement") && terrain.move_cost > 0
    }) && chunk.furniture.get(index).is_some_and(|furniture| {
        furniture
            .as_ref()
            .is_none_or(|furniture| furniture.move_cost_mod >= 0)
    })
}

fn production_road_exploration_step(
    snapshot: &WorldSnapshotV1,
    occupied: Option<WorldPosition>,
) -> Option<(WorldPosition, WorldPosition, i8, i8)> {
    for chunk in &snapshot.chunks {
        for local_y in 0..SUBMAP_SIZE {
            for local_x in 0..SUBMAP_SIZE {
                let start = WorldPosition {
                    x: chunk
                        .coord
                        .x
                        .checked_mul(SUBMAP_SIZE)?
                        .checked_add(local_x)?,
                    y: chunk
                        .coord
                        .y
                        .checked_mul(SUBMAP_SIZE)?
                        .checked_add(local_y)?,
                    z: chunk.coord.z,
                };
                if occupied == Some(start) || !is_passable_pavement(snapshot, start) {
                    continue;
                }
                for (dx, dy) in [(1_i8, 0_i8), (0, 1), (-1, 0), (0, -1)] {
                    let target = start.checked_offset(dx, dy, 0)?;
                    if occupied != Some(target) && is_passable_pavement(snapshot, target) {
                        return Some((start, target, dx, dy));
                    }
                }
            }
        }
    }
    None
}

fn assert_two_client_field_path(
    item_groups: &[cdda_protocol::ItemGroupDefinitionV1],
    worldgen: &cdda_protocol::WorldgenCatalogV1,
) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("field network runtime should build");
    runtime.block_on(async {
        let database = TemporaryFieldDatabase::new();
        let world_namespace = 911;
        let world_seed = [31; 32];
        let content = ContentIdentity {
            baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
            manifest_hash: [33; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let alpha_secret = SecretKey::generate();
        let beta_secret = SecretKey::generate();
        let alpha_endpoint = EndpointIdentity(*alpha_secret.public().as_bytes());
        let beta_endpoint = EndpointIdentity(*beta_secret.public().as_bytes());

        let mut store = WorldStore::open(database.path()).expect("field store should open");
        store
            .initialize_world(world_namespace, world_seed)
            .expect("field world should initialize");
        let account_block = store
            .reserve_id_block()
            .expect("field account IDs should reserve");
        let alpha_account = AccountId::new(world_namespace, account_block.start);
        let beta_account = AccountId::new(world_namespace, account_block.start + 1);
        let now = utc_now_seconds().expect("field acceptance clock should work");
        for (account_id, display_name, endpoint) in [
            (alpha_account, "Field Alpha", alpha_endpoint),
            (beta_account, "Field Beta", beta_endpoint),
        ] {
            store
                .create_pending_account(
                    account_id,
                    display_name,
                    AccountRole::Player,
                    endpoint,
                    now,
                )
                .expect("field account should be created");
            store
                .enroll_endpoint(endpoint, now)
                .expect("field endpoint should enroll");
        }
        let simulation_block = store
            .reserve_id_block()
            .expect("field simulation IDs should reserve");

        let mut world = WorldState::new(world_namespace, world_seed);
        world
            .advance_allocator_high_water(simulation_block.start - 1)
            .expect("field account reservation should remain burned");
        world
            .install_reserved_block(simulation_block)
            .expect("field simulation IDs should install");
        world
            .register_item_group_catalog(item_groups.to_vec())
            .expect("field item groups should register for network play");
        world
            .configure_worldgen(worldgen.clone())
            .expect("field worldgen should configure for network play");
        world
            .generate_initial_bubble(WorldPosition { x: 0, y: 0, z: 0 })
            .expect("field bubble should generate for network play");
        let initial_snapshot = world.snapshot();
        let corpse = initial_snapshot
            .ground_items
            .iter()
            .find(|ground| ground.item.type_id == "corpse_generic_male")
            .expect("production field should contain its characterized corpse");
        let corpse_id = corpse.item.id;
        let corpse_position = corpse.position;
        let nested_loot = corpse.item.ammunition_containers[0].contents[0].id;
        let (road_start, road_target, road_dx, road_dy) =
            production_road_exploration_step(&initial_snapshot, Some(corpse_position))
                .expect("production world should expose adjacent passable pavement tiles");
        let alpha_actor = world
            .spawn_actor(corpse_position, false)
            .expect("alpha should spawn on the field corpse");
        let beta_actor = world
            .spawn_actor(road_start, false)
            .expect("beta should spawn on the production road");
        let beta_initial_position = world
            .actor_snapshot(beta_actor)
            .expect("beta should exist")
            .position;
        assert_eq!(beta_initial_position, road_start);
        store
            .create_character(
                alpha_account,
                "Alpha",
                SimTick(0),
                0,
                &world
                    .actor_snapshot(alpha_actor)
                    .expect("alpha should have a spawn snapshot"),
            )
            .expect("alpha character should persist");
        store
            .create_character(
                beta_account,
                "Beta",
                SimTick(0),
                0,
                &world
                    .actor_snapshot(beta_actor)
                    .expect("beta should have a spawn snapshot"),
            )
            .expect("beta character should persist");
        store
            .write_snapshot(0, &world)
            .expect("initial field snapshot should persist");
        store
            .begin_runtime(now)
            .expect("field runtime should begin");

        let persistence_host =
            PersistenceHost::start(store).expect("field persistence worker should start");
        let persistence = persistence_host.handle();
        let host = SimulationHost::start(world).expect("field simulation should start");
        let simulation = host.handle();
        let committed_events = CommittedEventHub::default();
        let (stop_pump, pump) =
            start_field_persistence_pump(host, persistence.clone(), committed_events.clone());
        let (character_creator, _character_requests) = character_creation_channel();
        let server = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(SecretKey::generate())
            .alpns(vec![GAME_ALPN.to_vec()])
            .bind()
            .await
            .expect("field server endpoint should bind");
        let server_address = server.addr();
        let serving_endpoint = server.clone();
        let serving_persistence = persistence.clone();
        let serving_simulation = simulation.clone();
        let serving_content = content.clone();
        let serving_character_creator = character_creator.clone();
        let serving_events = committed_events.clone();
        let serving_sessions = SessionRegistry::default();
        let server_task = tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            for _ in 0..2 {
                let incoming = serving_endpoint
                    .accept()
                    .await
                    .expect("field server should accept both clients");
                let connection = incoming.await.expect("field handshake should complete");
                let persistence = serving_persistence.clone();
                let simulation = serving_simulation.clone();
                let content = serving_content.clone();
                let character_creator = serving_character_creator.clone();
                let events = serving_events.clone();
                let sessions = serving_sessions.clone();
                handlers.spawn(async move {
                    handle_game_connection_with_sessions(
                        &connection,
                        persistence,
                        simulation,
                        content,
                        sessions,
                        AuthorizationChangeHub::default(),
                        character_creator,
                        events,
                        ChatHub::default(),
                    )
                    .await
                });
            }
            let mut results = Vec::new();
            while let Some(result) = handlers.join_next().await {
                results.push(result.expect("field session task should join"));
            }
            results
        });

        let mut alpha = connect_field_client(
            alpha_secret,
            server_address.clone(),
            content.clone(),
            alpha_actor,
        )
        .await;
        let mut beta =
            connect_field_client(beta_secret, server_address, content.clone(), beta_actor).await;
        assert!(
            alpha
                .snapshot
                .ground_items
                .iter()
                .any(|ground| ground.item.id == corpse_id),
            "alpha should receive the production corpse through normal replication"
        );
        assert!(
            beta.snapshot.controlled_actor.position == beta_initial_position,
            "beta should enter on production pavement"
        );

        write_control_frame(
            &mut alpha.send,
            &ControlMessage::Command(ClientCommand {
                actor_id: alpha_actor,
                sequence: CommandSequence(1),
                client_tick: alpha.snapshot.tick,
                kind: CommandKind::PickUp { item_id: corpse_id },
            }),
        )
        .await
        .expect("alpha corpse pickup should send");
        let picked_up = read_field_snapshot_until(
            &alpha.connection,
            alpha_actor,
            CommandSequence(1),
            |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == corpse_id)
            },
        )
        .await;
        let removal_tick = SimTick(
            picked_up
                .tick
                .0
                .checked_add(25)
                .expect("field wait tick should fit"),
        );
        let ready_to_remove = read_field_snapshot_until(
            &alpha.connection,
            alpha_actor,
            CommandSequence(1),
            |snapshot| snapshot.tick >= removal_tick,
        )
        .await;
        write_control_frame(
            &mut alpha.send,
            &ControlMessage::Command(ClientCommand {
                actor_id: alpha_actor,
                sequence: CommandSequence(2),
                client_tick: ready_to_remove.tick,
                kind: CommandKind::RemovePocketItem {
                    owner_item: corpse_id,
                    pocket_index: 0,
                    contained_item: nested_loot,
                },
            }),
        )
        .await
        .expect("alpha nested-loot removal should send");
        let nested_removed = read_field_snapshot_until(
            &alpha.connection,
            alpha_actor,
            CommandSequence(2),
            |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == nested_loot)
            },
        )
        .await;

        write_control_frame(
            &mut beta.send,
            &ControlMessage::Command(ClientCommand {
                actor_id: beta_actor,
                sequence: CommandSequence(1),
                client_tick: beta.snapshot.tick,
                kind: CommandKind::Move {
                    dx: road_dx,
                    dy: road_dy,
                    dz: 0,
                },
            }),
        )
        .await
        .expect("beta exploration move should send");
        let explored = read_field_snapshot_until(
            &beta.connection,
            beta_actor,
            CommandSequence(1),
            |snapshot| snapshot.controlled_actor.position != beta_initial_position,
        )
        .await;
        assert_eq!(
            explored.controlled_actor.position, road_target,
            "the normal client command should traverse adjacent production pavement"
        );
        assert!(
            nested_removed
                .controlled_actor
                .inventory
                .iter()
                .any(|item| item.id == corpse_id)
        );

        alpha
            .send
            .finish()
            .expect("alpha control stream should finish cleanly");
        beta.send
            .finish()
            .expect("beta control stream should finish cleanly");
        let results = tokio::time::timeout(Duration::from_secs(3), server_task)
            .await
            .expect("field sessions should stop after disconnect")
            .expect("field server task should join");
        assert!(
            results.iter().all(Result::is_ok),
            "field session results were {results:?}"
        );
        alpha
            .connection
            .close(0_u32.into(), b"field acceptance complete");
        beta.connection
            .close(0_u32.into(), b"field acceptance complete");
        let disconnected = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let snapshot = simulation
                    .snapshot(Duration::from_secs(1))
                    .expect("field simulation snapshot should respond");
                if snapshot.actors.iter().all(|actor| !actor.connected) {
                    break snapshot;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("both field actors should become disconnected");
        assert_eq!(disconnected.actors.len(), 2);
        assert_eq!(
            disconnected
                .actors
                .iter()
                .find(|actor| actor.id == beta_actor)
                .expect("disconnected beta should remain physically present")
                .position,
            road_target
        );
        alpha.endpoint.close().await;
        beta.endpoint.close().await;
        server.close().await;

        let checkpoint = simulation
            .begin_checkpoint(Duration::from_secs(1))
            .expect("field simulation should pause for its audited checkpoint");
        stop_pump
            .send(())
            .expect("field persistence pump should still be running");
        let (host, journal_sequence) = pump.join().expect("field persistence pump should join");
        persistence
            .write_snapshot(journal_sequence, checkpoint.clone())
            .expect("final field snapshot should persist");
        simulation
            .complete_checkpoint(Duration::from_secs(1))
            .expect("field simulation should resume for shutdown");
        assert_eq!(
            host.shutdown(),
            cdda_server::SimulationExit::Requested,
            "field simulation should stop cleanly"
        );
        persistence
            .finish_runtime(utc_now_seconds().expect("field shutdown clock should work"))
            .expect("field runtime should finish cleanly");
        persistence
            .checkpoint()
            .expect("field SQLite WAL should checkpoint");
        persistence_host.shutdown();

        let recovered_store =
            WorldStore::open(database.path()).expect("field store should reopen after restart");
        let (_sequence, recovered) = recovered_store
            .recover_latest(WorldState::new(world_namespace, world_seed))
            .expect("field SQLite state should recover");
        assert_eq!(
            recovered
                .canonical_hash()
                .expect("recovered field should hash"),
            WorldState::from_snapshot(&checkpoint)
                .expect("checkpoint should remain canonical")
                .canonical_hash()
                .expect("checkpoint should hash")
        );
        assert_eq!(
            recovered
                .actor_snapshot(beta_actor)
                .expect("recovered beta should remain present")
                .position,
            road_target,
            "SQLite recovery should preserve the production-road exploration step"
        );
        let encoded = postcard::to_stdvec(
            &recovered_store
                .export_replay(content.clone())
                .expect("field portable replay should export"),
        )
        .expect("field portable replay should encode");
        let replayed = postcard::from_bytes::<ReplayBundleV1>(&encoded)
            .expect("field portable replay should decode")
            .verify(&content)
            .expect("field portable replay should verify");
        assert_eq!(
            replayed
                .canonical_hash()
                .expect("replayed field should hash"),
            recovered
                .canonical_hash()
                .expect("recovered field should hash again")
        );
        assert_eq!(
            replayed
                .actor_snapshot(beta_actor)
                .expect("replayed beta should remain present")
                .position,
            road_target,
            "portable replay should preserve the production-road exploration step"
        );
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn assert_production_regional_field_gameplay(
    item_groups: &ItemGroupRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
    overmap_terrain: &OvermapTerrainRegistry,
    start_locations: &StartLocationRegistry,
    city_settings: &CitySettingsDefinition,
    mapgen: &MapgenRegistry,
    regions: &DefaultRegionTerrainFurnitureRegistry,
    terrain: &TerrainRegistry,
    furniture: &FurnitureRegistry,
) {
    let (production_overmap, cities, road_exits) =
        bootstrap_regional_road_overmap(overmap_terrain, [31; 32], city_settings)
            .expect("regional road overmap should normalize");
    assert_eq!(road_exits.len(), 3);
    let mapgen_item_group_roots = runtime_mapgen_item_group_roots(&production_overmap, mapgen)
        .expect("production mapgen item-group roots should resolve");
    let production_field_catalog = runtime_named_item_group_catalogs(
        mapgen_item_group_roots.iter().map(String::as_str),
        item_groups,
        item_group_content,
    )
    .expect("production mapgen item-group closures should normalize");
    let production_field_worldgen = runtime_mapgen_worldgen(
        production_overmap,
        cities,
        start_locations
            .get("sloc_field")
            .expect("field start should exist"),
        RuntimeMapgenContent {
            mapgen,
            overmap_terrain,
            regions,
            terrain,
            furniture,
            item_groups: &production_field_catalog,
        },
    )
    .expect("production regional field should normalize");
    assert!(
        production_field_worldgen
            .overmap
            .identities
            .iter()
            .any(|identity| identity.full_id == "field")
    );
    assert!(
        production_field_worldgen
            .overmap
            .identities
            .iter()
            .any(|identity| identity.full_id == "road_nesw")
    );
    assert!(!production_field_worldgen.cities.is_empty());
    assert_eq!(
        production_field_worldgen
            .start_location
            .as_ref()
            .expect("field start should remain explicit")
            .start_location_id,
        "sloc_field"
    );

    let mut production_field = WorldState::new(909, [31; 32]);
    production_field
        .install_reserved_block(
            cdda_sim::ReservedIdBlock::new(1, cdda_sim::ID_RESERVATION_SIZE)
                .expect("field ID block should be valid"),
        )
        .expect("field IDs should reserve");
    production_field
        .register_item_group_catalog(production_field_catalog.clone())
        .expect("field item groups should register");
    production_field
        .configure_worldgen(production_field_worldgen.clone())
        .expect("field worldgen should configure");
    production_field
        .generate_initial_bubble(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 })
        .expect("field bubble should generate");
    let first_field_actor = production_field
        .spawn_actor_first_available(true)
        .expect("first field actor should spawn");
    let second_field_actor = production_field
        .spawn_actor_first_available(true)
        .expect("second field actor should spawn");
    let production_field_snapshot = production_field.snapshot();
    let (_, production_road_target, _, _) =
        production_road_exploration_step(&production_field_snapshot, None)
            .expect("production initial bubble should contain traversable road pavement");
    assert!(is_passable_pavement(
        &production_field_snapshot,
        production_road_target
    ));
    fn collect_item_types(
        item: &cdda_protocol::ItemSnapshot,
        item_types: &mut std::collections::BTreeSet<String>,
    ) {
        item_types.insert(item.type_id.clone());
        for ammunition in item
            .integral_magazines
            .iter()
            .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
        {
            collect_item_types(ammunition, item_types);
        }
        for magazine in item
            .magazine_wells
            .iter()
            .filter_map(|pocket| pocket.installed_magazine.as_deref())
        {
            collect_item_types(magazine, item_types);
        }
        for content in item
            .ammunition_containers
            .iter()
            .flat_map(|pocket| &pocket.contents)
        {
            collect_item_types(content, item_types);
        }
    }
    let mut production_field_item_types = std::collections::BTreeSet::new();
    for ground in &production_field_snapshot.ground_items {
        collect_item_types(&ground.item, &mut production_field_item_types);
    }
    assert_eq!(
        production_field_item_types.len(),
        45,
        "production field reached {production_field_item_types:?}"
    );
    let nested_ground_items = production_field_snapshot
        .ground_items
        .iter()
        .filter(|ground| {
            ground
                .item
                .ammunition_containers
                .iter()
                .any(|pocket| !pocket.contents.is_empty())
        })
        .map(|ground| ground.item.type_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        production_field_snapshot.ground_items.len(),
        48,
        "production ground items were {:?}",
        production_field_snapshot
            .ground_items
            .iter()
            .map(|ground| (
                ground.position.x,
                ground.position.y,
                ground.item.type_id.as_str(),
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(nested_ground_items, ["corpse_generic_male"]);
    assert_eq!(
        second_field_actor.counter(),
        first_field_actor.counter() + 1,
        "the two production-field actors should receive consecutive stable IDs"
    );
    assert_eq!(
        second_field_actor.world_namespace(),
        first_field_actor.world_namespace()
    );

    let production_field_scenario = ScenarioV1 {
        format_version: cdda_conformance::SCENARIO_FORMAT_VERSION,
        protocol_version: cdda_protocol::PROTOCOL_VERSION,
        persistence_schema_version: cdda_persistence::SCHEMA_VERSION,
        replay_format_version: cdda_persistence::REPLAY_FORMAT_VERSION,
        baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
        world_namespace: 910,
        world_seed: [31; 32],
        content_manifest_hash: [33; 32],
        enabled_mods: vec![String::from("dda")],
        item_groups: production_field_catalog,
        terrain_bash_types: Vec::new(),
        smash_item_types: Vec::new(),
        worldgen: Some(production_field_worldgen),
        chunks: Vec::new(),
        terrain: Vec::new(),
        actors: vec![
            ScenarioActorV1 {
                alias: String::from("alpha"),
                spawn: ScenarioActorSpawnV1::AtGeneratedGroundItem {
                    item: String::from("field_corpse"),
                },
                connected: true,
                stats: cdda_protocol::CharacterCreationStatsV1::default(),
            },
            ScenarioActorV1 {
                alias: String::from("beta"),
                spawn: ScenarioActorSpawnV1::StartLocation,
                connected: true,
                stats: cdda_protocol::CharacterCreationStatsV1::default(),
            },
        ],
        ground_items: Vec::new(),
        generated_items: vec![
            ScenarioGeneratedItemV1 {
                alias: String::from("field_corpse"),
                selector: ScenarioGeneratedItemSelectorV1::Ground {
                    type_id: String::from("corpse_generic_male"),
                    ordinal: 0,
                },
            },
            ScenarioGeneratedItemV1 {
                alias: String::from("nested_loot"),
                selector: ScenarioGeneratedItemSelectorV1::Pocket {
                    owner: String::from("field_corpse"),
                    pocket_index: 0,
                    ordinal: 0,
                },
            },
        ],
        steps: vec![
            ScenarioStepV1::Command {
                actor: String::from("alpha"),
                command: ScenarioCommandV1::PickUp {
                    item: String::from("field_corpse"),
                },
            },
            ScenarioStepV1::AdvanceBatch { ticks: 25 },
            ScenarioStepV1::Command {
                actor: String::from("alpha"),
                command: ScenarioCommandV1::RemovePocketItem {
                    owner_item: String::from("field_corpse"),
                    pocket_index: 0,
                    contained_item: String::from("nested_loot"),
                },
            },
            ScenarioStepV1::Command {
                actor: String::from("beta"),
                command: ScenarioCommandV1::Move { dx: 0, dy: 1 },
            },
            ScenarioStepV1::AdvanceBatch { ticks: 105 },
            ScenarioStepV1::Connection {
                actor: String::from("alpha"),
                connected: false,
            },
            ScenarioStepV1::Connection {
                actor: String::from("beta"),
                connected: false,
            },
            ScenarioStepV1::Advance { ticks: 3 },
        ],
        expected: ScenarioExpectationV1 {
            final_tick: SimTick(0),
            final_state_hash: [0; 32],
            event_trace_hash: [0; 32],
            actors: Vec::new(),
            ground_items: Vec::new(),
            event_batches: None,
        },
    };
    let direct_field = run_scenario(&production_field_scenario, ScenarioMode::Direct)
        .expect("production field gameplay should run directly");
    assert_eq!(
        (direct_field.final_state_hash, direct_field.event_trace_hash),
        (
            [
                0xb8, 0x1e, 0xf5, 0x02, 0xd8, 0xde, 0xdb, 0xf9, 0xd6, 0xe5, 0xcc, 0x87, 0xb4, 0xd3,
                0x41, 0x5d, 0xe4, 0x2e, 0xdf, 0xd5, 0x3b, 0x2a, 0xa6, 0x1c, 0xd5, 0x78, 0x56, 0x11,
                0x8f, 0x44, 0xd9, 0xba,
            ],
            [
                0x54, 0x08, 0x4d, 0x59, 0xe3, 0x67, 0xee, 0x8d, 0xcd, 0xfb, 0xa8, 0x4f, 0xe4, 0x2e,
                0x46, 0xb2, 0x64, 0x63, 0x03, 0xcc, 0x41, 0x16, 0x8b, 0xe0, 0x62, 0x59, 0xdd, 0xab,
                0xe8, 0x6d, 0xce, 0xfd,
            ],
        )
    );
    assert_eq!(
        direct_field.final_snapshot.chunks.len(),
        168,
        "production chunk coordinates were {:?}",
        direct_field
            .final_snapshot
            .chunks
            .iter()
            .map(|chunk| chunk.coord)
            .collect::<Vec<_>>()
    );
    assert_eq!(direct_field.final_snapshot.actors.len(), 2);
    assert!(
        direct_field
            .final_snapshot
            .actors
            .iter()
            .all(|actor| !actor.connected)
    );
    let field_event_kinds = direct_field
        .event_batches
        .iter()
        .flat_map(|batch| &batch.events)
        .map(|event| &event.kind)
        .collect::<Vec<_>>();
    assert!(
        field_event_kinds.iter().any(|kind| matches!(
            kind,
            cdda_protocol::WorldEventKind::PocketItemRemoved { .. }
        )),
        "production nested-loot command trace was {field_event_kinds:?}"
    );
    assert!(
        field_event_kinds
            .iter()
            .any(|kind| matches!(kind, cdda_protocol::WorldEventKind::ActorMoved { .. })),
        "production exploration command trace was {field_event_kinds:?}"
    );
    for mode in [
        ScenarioMode::SnapshotEachTick,
        ScenarioMode::SqliteRecovery,
        ScenarioMode::PortableReplay,
    ] {
        assert_eq!(
            run_scenario(&production_field_scenario, mode)
                .expect("production field gameplay should recover"),
            direct_field,
            "{mode} must preserve the complete production field gameplay trace"
        );
    }
    assert_two_client_field_path(
        &production_field_scenario.item_groups,
        production_field_scenario
            .worldgen
            .as_ref()
            .expect("production field scenario should retain worldgen"),
    );
}
