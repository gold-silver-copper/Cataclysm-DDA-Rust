use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use cdda_content::{
    AmmunitionRegistry, ConstructionRegistry, ContentManifest, DEFAULT_MANIFEST_PATH,
    FurnitureRegistry, ItemRegistry, ModCatalog, MonsterRegistry, ProficiencyRegistry,
    RecipeRegistry, SkillRegistry, TerrainRegistry,
};
use cdda_net::{
    load_or_create_secret_key, read_control_frame, read_snapshot_stream, write_control_frame,
};
use cdda_protocol::{
    ACTION_POINT_THRESHOLD, ActorId, BASELINE_COMMIT, BashTargetKindV1, CharacterCreationStatsV1,
    CharacterRequest, CharacterSummary, ChatMessage, ChatRejection, ClientCommand,
    ClientDatagramV1, ClientHello, CommandKind, CommandRejection, ContentIdentity, ControlMessage,
    CreatureId, ENROLL_ALPN, EnrollmentRejection, GAME_ALPN, GameplayRejection,
    HeldMovementInputV1, HorizontalDirection, IntegralMagazinePocketSnapshotV1,
    InteractionCancellationReasonV1, ItemId, ItemSnapshot, MAX_CHARACTER_CREATION_STAT,
    MAX_CHARACTERS_PER_ACCOUNT, MAX_CHAT_BYTES, MAX_DATAGRAM_SIZE, MAX_REPORT_BYTES,
    MAX_REPORT_CHARACTERS, MIN_CHARACTER_CREATION_STAT, PROTOCOL_VERSION, PlayerReport,
    REQUIRED_DATAGRAM_SIZE, ReplicationSnapshotV1, ReportReason, ReportRejection, ReportResponse,
    SkyPhase, VehicleId, WorldEvent, WorldEventKind, encode_client_datagram,
};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, endpoint::presets};

mod operator;

use operator::{OneShotOperation, parse_account_key_operation, parse_admin_operation};

#[derive(Resource)]
struct BootstrapStatus {
    identity: EndpointId,
    detail: String,
}

struct Options {
    profile: PathBuf,
    enrollment_address: Option<PathBuf>,
    identity_only: bool,
    enroll_only: bool,
    play_address: Option<PathBuf>,
    admin_address: Option<PathBuf>,
    one_shot: Option<OneShotOperation>,
    character_name: Option<String>,
    content_manifest: PathBuf,
}

enum ClientAction {
    ChooseCharacter(CharacterRequest),
    HeldMovement {
        direction: Option<HorizontalDirection>,
    },
    PickUp {
        item_id: ItemId,
    },
    Drop {
        item_id: ItemId,
    },
    TakeVehicleCargo {
        vehicle_id: cdda_protocol::VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
    },
    StoreVehicleCargo {
        vehicle_id: cdda_protocol::VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
    },
    SetVehiclePartOpen {
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        open: bool,
    },
    Wield {
        item_id: ItemId,
    },
    Wear {
        item_id: ItemId,
    },
    TakeOff {
        item_id: ItemId,
    },
    Unwield,
    Consume {
        item_id: ItemId,
    },
    Activate {
        item_id: ItemId,
    },
    TalkToNpc {
        target: cdda_protocol::NpcId,
    },
    BoardVehicle {
        vehicle_id: VehicleId,
        prototype_part_index: u16,
    },
    UnboardVehicle {
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        dx: i8,
        dy: i8,
    },
    RespondInteraction {
        interaction_id: cdda_protocol::InteractionId,
        choice_id: String,
    },
    CancelInteraction {
        interaction_id: cdda_protocol::InteractionId,
    },
    Craft {
        recipe_id: String,
    },
    ResumeCraft,
    CancelCraft,
    Read {
        item_id: ItemId,
        book_type_id: String,
    },
    ResumeRead,
    CancelRead,
    Disassemble {
        item_id: ItemId,
        item_type_id: String,
    },
    ResumeDisassembly,
    CancelDisassembly,
    Construct {
        target: cdda_protocol::WorldPosition,
        construction_id: String,
    },
    ResumeConstruction,
    CancelConstruction,
    Open {
        dx: i8,
        dy: i8,
    },
    Close {
        dx: i8,
        dy: i8,
    },
    Smash {
        dx: i8,
        dy: i8,
    },
    Attack {
        target: ActorId,
    },
    AttackCreature {
        target: CreatureId,
    },
    ShootActor {
        target: ActorId,
    },
    ShootCreature {
        target: CreatureId,
    },
    Reload {
        ammunition_item: ItemId,
        target_pocket_index: Option<u16>,
    },
    RemovePocketItem {
        owner_item: ItemId,
        pocket_index: u16,
        contained_item: ItemId,
    },
    InsertPocketItem {
        owner_item: ItemId,
        pocket_index: u16,
        source_item: ItemId,
    },
    Sleep,
    Wake,
    Wait,
    Chat {
        text: String,
    },
    Shutdown,
}

enum ClientUpdate {
    CharacterList(Vec<CharacterSummary>),
    CharacterSelectionRejected(GameplayRejection),
    Snapshot {
        controlled_actor: ActorId,
        snapshot: Box<ReplicationSnapshotV1>,
    },
    Events(Vec<WorldEvent>),
    Chat(ChatMessage),
    Status(String),
}

#[derive(Resource)]
struct GameClient {
    actions: tokio::sync::mpsc::Sender<ClientAction>,
    updates: Mutex<Receiver<ClientUpdate>>,
    thread: Option<JoinHandle<()>>,
    controlled_actor: Option<ActorId>,
    snapshot: Option<ReplicationSnapshotV1>,
    status: String,
    notice: String,
    chat_messages: VecDeque<String>,
}

#[derive(Default, Resource)]
struct ChatComposer {
    active: bool,
    text: String,
}

#[derive(Default, Resource)]
struct HeldMovementSender {
    last: Option<HorizontalDirection>,
    since_send: Duration,
}

#[derive(Default, Resource)]
struct CharacterMenu {
    characters: Option<Vec<CharacterSummary>>,
    selected: usize,
    creating: bool,
    name: String,
    base_stats: CharacterCreationStatsV1,
    selected_stat: usize,
    waiting: bool,
    notice: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemMenuAction {
    PickUp,
    Drop,
    Wield,
    Wear,
    TakeOff,
    Reload,
    Consume,
    Activate,
    Read,
    Disassemble,
}

#[derive(Clone)]
struct ItemMenuEntry {
    item_id: ItemId,
    label: String,
    vehicle_cargo: Option<(cdda_protocol::VehicleId, u16)>,
}

#[derive(Default, Resource)]
struct ItemMenu {
    action: Option<ItemMenuAction>,
    entries: Vec<ItemMenuEntry>,
    selected: usize,
}

#[derive(Default, Resource)]
struct InteractionMenu {
    interaction_id: Option<cdda_protocol::InteractionId>,
    selected: usize,
    waiting: bool,
}

impl InteractionMenu {
    const fn is_active(&self) -> bool {
        self.interaction_id.is_some()
    }
}

#[derive(Clone)]
struct CraftMenuEntry {
    recipe_id: String,
    label: String,
}

#[derive(Clone)]
struct ConstructionTargetMenuEntry {
    target: cdda_protocol::WorldPosition,
    label: String,
}

#[derive(Default, Resource)]
struct CraftMenu {
    open: bool,
    construction: bool,
    target_construction: Option<String>,
    entries: Vec<CraftMenuEntry>,
    targets: Vec<ConstructionTargetMenuEntry>,
    selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetMenuAction {
    Melee,
    Shoot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetChoice {
    Actor(ActorId),
    Creature(CreatureId),
}

#[derive(Clone)]
struct TargetMenuEntry {
    target: TargetChoice,
    label: String,
}

#[derive(Default, Resource)]
struct TargetMenu {
    action: Option<TargetMenuAction>,
    entries: Vec<TargetMenuEntry>,
    selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerrainMenuAction {
    Open,
    Close,
    Smash,
}

#[derive(Clone)]
struct TerrainMenuEntry {
    dx: i8,
    dy: i8,
    label: String,
}

#[derive(Default, Resource)]
struct TerrainMenu {
    action: Option<TerrainMenuAction>,
    entries: Vec<TerrainMenuEntry>,
    selected: usize,
}

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct ActorVisual(ActorId);

#[derive(Component)]
struct ItemVisual(ItemId);

#[derive(Component)]
struct CreatureVisual(CreatureId);

#[derive(Component)]
struct VehicleVisual(VehicleId, u16);

#[derive(Component)]
struct TileVisual {
    x: u8,
    y: u8,
}

#[derive(Resource)]
struct ContentItems(ItemRegistry);

#[derive(Resource)]
struct ContentAmmunition(AmmunitionRegistry);

#[derive(Resource)]
struct ContentMonsters(MonsterRegistry);

#[derive(Resource)]
struct ContentTerrain(TerrainRegistry);

#[derive(Resource)]
struct ContentFurniture(FurnitureRegistry);

#[derive(Resource)]
struct ContentRecipes(RecipeRegistry);

#[derive(Resource)]
struct ContentProficiencies(ProficiencyRegistry);

#[derive(Resource)]
struct ContentConstructions(ConstructionRegistry);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options()?;
    std::fs::create_dir_all(&options.profile)?;
    let secret_key = load_or_create_secret_key(options.profile.join("client-identity.key"))?;
    let identity = secret_key.public();
    println!("CDDA Rust client endpoint: {identity}");
    if options.identity_only {
        return Ok(());
    }

    let detail = if let Some(address_path) = &options.enrollment_address {
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(enroll(secret_key.clone(), &options.profile, address_path))?
    } else if options.play_address.is_some() {
        String::from("Connecting to the authoritative server…")
    } else {
        String::from(
            "Identity ready. Ask the server operator to create a pending account for this endpoint, then restart with --enroll-address <endpoint-address.json>.",
        )
    };
    if options.enroll_only {
        println!("{detail}");
        return Ok(());
    }

    if let Some(operation) = options.one_shot {
        let runtime = tokio::runtime::Runtime::new()?;
        let output = match operation {
            OneShotOperation::AccountKey(request) => {
                let address = options
                    .play_address
                    .as_deref()
                    .ok_or("--account-key requires --play-address")?;
                let content = load_content_identity(&options.content_manifest)?;
                runtime.block_on(operator::run_account_key_operation(
                    secret_key,
                    &options.profile,
                    address,
                    content,
                    request,
                ))?
            }
            OneShotOperation::Admin(request) => {
                let address = options
                    .admin_address
                    .as_deref()
                    .ok_or("--admin requires --admin-address")?;
                runtime.block_on(operator::run_admin_operation(
                    secret_key,
                    &options.profile,
                    address,
                    request,
                ))?
            }
        };
        println!("{output}");
        return Ok(());
    }

    let (
        game_client,
        content_items,
        content_ammunition,
        content_monsters,
        content_terrain,
        content_furniture,
        content_recipes,
        content_proficiencies,
        content_constructions,
    ) = if let Some(address) = &options.play_address {
        let content_manifest = ContentManifest::load(&options.content_manifest)?;
        let content_root = options
            .content_manifest
            .parent()
            .ok_or("content manifest has no parent directory")?;
        let mod_catalog = ModCatalog::load(&content_manifest, content_root)?;
        let enabled_mods = mod_catalog.recommended_new_world()?;
        let items = ItemRegistry::load_selected(
            &content_manifest,
            content_root,
            &mod_catalog,
            &enabled_mods,
        )?;
        let ammunition = AmmunitionRegistry::load_selected(
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
        let skills = SkillRegistry::load_selected(
            &content_manifest,
            content_root,
            &mod_catalog,
            &enabled_mods,
        )?;
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
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: content_manifest.canonical_hash()?,
            enabled_mods,
        };
        (
            Some(GameClient::start(
                secret_key,
                options.profile.clone(),
                address.clone(),
                options.character_name.clone(),
                content,
            )?),
            Some(ContentItems(items)),
            Some(ContentAmmunition(ammunition)),
            Some(ContentMonsters(monsters)),
            Some(ContentTerrain(terrain)),
            Some(ContentFurniture(furniture)),
            Some(ContentRecipes(recipes)),
            Some(ContentProficiencies(proficiencies)),
            Some(ContentConstructions(constructions)),
        )
    } else {
        (None, None, None, None, None, None, None, None, None)
    };
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.035, 0.043, 0.055)))
        .insert_resource(BootstrapStatus { identity, detail })
        .insert_resource(ChatComposer::default())
        .insert_resource(CharacterMenu::default())
        .insert_resource(ItemMenu::default())
        .insert_resource(InteractionMenu::default())
        .insert_resource(CraftMenu::default())
        .insert_resource(TargetMenu::default())
        .insert_resource(TerrainMenu::default())
        .insert_resource(HeldMovementSender::default());
    if let Some(game_client) = game_client {
        app.insert_resource(game_client);
    }
    if let Some(content_items) = content_items {
        app.insert_resource(content_items);
    }
    if let Some(content_ammunition) = content_ammunition {
        app.insert_resource(content_ammunition);
    }
    if let Some(content_monsters) = content_monsters {
        app.insert_resource(content_monsters);
    }
    if let Some(content_terrain) = content_terrain {
        app.insert_resource(content_terrain);
    }
    if let Some(content_furniture) = content_furniture {
        app.insert_resource(content_furniture);
    }
    if let Some(content_recipes) = content_recipes {
        app.insert_resource(content_recipes);
    }
    if let Some(content_proficiencies) = content_proficiencies {
        app.insert_resource(content_proficiencies);
    }
    if let Some(content_constructions) = content_constructions {
        app.insert_resource(content_constructions);
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: String::from("Cataclysm: Dark Days Ahead — Rust Multiplayer"),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    }))
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            handle_character_menu,
            handle_interaction_menu,
            handle_chat_input,
            handle_item_menu,
            handle_craft_menu,
            handle_target_menu,
            handle_terrain_menu,
            handle_movement_input,
            poll_game_updates,
            render_status_text,
        )
            .chain(),
    )
    .run();
    Ok(())
}

fn parse_options() -> Result<Options, Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut profile = PathBuf::from("client-profile");
    let mut enrollment_address = None;
    let mut identity_only = false;
    let mut enroll_only = false;
    let mut play_address = None;
    let mut admin_address = None;
    let mut one_shot = None;
    let mut character_name = None;
    let mut content_manifest = PathBuf::from(DEFAULT_MANIFEST_PATH);
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--profile") => {
                profile = arguments
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--profile requires a directory")?;
            }
            Some("--enroll-address") => {
                enrollment_address = Some(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--enroll-address requires a JSON file")?,
                );
            }
            Some("--identity-only") => identity_only = true,
            Some("--enroll-only") => enroll_only = true,
            Some("--play-address") => {
                play_address = Some(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--play-address requires a JSON file")?,
                );
            }
            Some("--admin-address") => {
                admin_address = Some(
                    arguments
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--admin-address requires a JSON file")?,
                );
            }
            Some("--account-key") => {
                if one_shot.is_some() {
                    return Err("only one one-shot operation may be requested".into());
                }
                let command = arguments
                    .next()
                    .and_then(|value| value.into_string().ok())
                    .ok_or("--account-key requires list, add, or revoke")?;
                let endpoint = if command == "add" || command == "revoke" {
                    Some(
                        arguments
                            .next()
                            .and_then(|value| value.into_string().ok())
                            .ok_or("account-key add/revoke requires an endpoint ID")?,
                    )
                } else {
                    None
                };
                one_shot = Some(OneShotOperation::AccountKey(parse_account_key_operation(
                    &command,
                    endpoint.as_deref(),
                )?));
            }
            Some("--admin") => {
                if one_shot.is_some() {
                    return Err("only one one-shot operation may be requested".into());
                }
                let command_arguments = arguments
                    .map(|value| {
                        value
                            .into_string()
                            .map_err(|_| "admin arguments must be valid UTF-8")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                one_shot = Some(OneShotOperation::Admin(parse_admin_operation(
                    &command_arguments,
                )?));
                break;
            }
            Some("--character") => {
                character_name = Some(
                    arguments
                        .next()
                        .and_then(|name| name.into_string().ok())
                        .ok_or("--character requires a UTF-8 name")?,
                );
            }
            Some("--content-manifest") => {
                content_manifest = arguments
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--content-manifest requires a JSON file")?;
            }
            Some(other) => return Err(format!("unknown client argument: {other}").into()),
            None => return Err("client arguments must be valid UTF-8".into()),
        }
    }
    if enroll_only && enrollment_address.is_none() {
        return Err("--enroll-only requires --enroll-address".into());
    }
    if enrollment_address.is_some() && play_address.is_some() {
        return Err("enrollment and gameplay cannot run in the same invocation".into());
    }
    if identity_only
        && (enroll_only
            || enrollment_address.is_some()
            || play_address.is_some()
            || admin_address.is_some()
            || one_shot.is_some())
    {
        return Err("--identity-only cannot be combined with network options".into());
    }
    if enrollment_address.is_some() && (admin_address.is_some() || one_shot.is_some()) {
        return Err("enrollment cannot be combined with a one-shot operation".into());
    }
    match &one_shot {
        Some(OneShotOperation::AccountKey(_)) if play_address.is_none() => {
            return Err("--account-key requires --play-address".into());
        }
        Some(OneShotOperation::AccountKey(_)) if admin_address.is_some() => {
            return Err("account-key operations cannot use --admin-address".into());
        }
        Some(OneShotOperation::Admin(_)) if admin_address.is_none() => {
            return Err("--admin requires --admin-address".into());
        }
        Some(OneShotOperation::Admin(_)) if play_address.is_some() => {
            return Err("admin operations cannot use --play-address".into());
        }
        None if admin_address.is_some() => {
            return Err("--admin-address requires --admin".into());
        }
        _ => {}
    }
    Ok(Options {
        profile,
        enrollment_address,
        identity_only,
        enroll_only,
        play_address,
        admin_address,
        one_shot,
        character_name,
        content_manifest,
    })
}

fn load_content_identity(
    manifest_path: &Path,
) -> Result<ContentIdentity, Box<dyn std::error::Error>> {
    let manifest = ContentManifest::load(manifest_path)?;
    let content_root = manifest_path
        .parent()
        .ok_or("content manifest has no parent directory")?;
    let mod_catalog = ModCatalog::load(&manifest, content_root)?;
    Ok(ContentIdentity {
        baseline_commit: BASELINE_COMMIT.to_owned(),
        manifest_hash: manifest.canonical_hash()?,
        enabled_mods: mod_catalog.recommended_new_world()?,
    })
}

impl GameClient {
    fn start(
        secret_key: SecretKey,
        profile: PathBuf,
        address_path: PathBuf,
        character_name: Option<String>,
        content: ContentIdentity,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (actions, action_receiver) = tokio::sync::mpsc::channel(128);
        let (update_sender, updates) = mpsc::sync_channel(128);
        let thread = thread::Builder::new()
            .name(String::from("cdda-client-network"))
            .spawn(move || {
                let result = tokio::runtime::Runtime::new().and_then(|runtime| {
                    runtime
                        .block_on(run_game_session(
                            secret_key,
                            profile,
                            address_path,
                            character_name,
                            content,
                            action_receiver,
                            update_sender.clone(),
                        ))
                        .map_err(std::io::Error::other)
                });
                if let Err(error) = result {
                    let _send_result = update_sender
                        .try_send(ClientUpdate::Status(format!("Disconnected: {error}")));
                }
            })?;
        Ok(Self {
            actions,
            updates: Mutex::new(updates),
            thread: Some(thread),
            controlled_actor: None,
            snapshot: None,
            status: String::from("Connecting to the authoritative server…"),
            notice: String::new(),
            chat_messages: VecDeque::new(),
        })
    }
}

impl Drop for GameClient {
    fn drop(&mut self) {
        let _send_result = self.actions.try_send(ClientAction::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
    }
}

async fn run_game_session(
    secret_key: SecretKey,
    profile: PathBuf,
    address_path: PathBuf,
    character_name: Option<String>,
    content: ContentIdentity,
    mut actions: tokio::sync::mpsc::Receiver<ClientAction>,
    updates: SyncSender<ClientUpdate>,
) -> Result<(), String> {
    let server_address: EndpointAddr =
        serde_json::from_slice(&std::fs::read(&address_path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    verify_existing_pin(&profile, server_address.id).map_err(|error| error.to_string())?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .bind()
        .await
        .map_err(|error| error.to_string())?;
    let connection = endpoint
        .connect(server_address.clone(), GAME_ALPN)
        .await
        .map_err(|error| error.to_string())?;
    require_datagram_support(&connection)?;
    if connection.remote_id() != server_address.id {
        return Err(String::from(
            "iroh server identity did not match the pinned address",
        ));
    }
    pin_server(&profile, server_address.id).map_err(|error| error.to_string())?;
    let (mut send, mut receive) = connection
        .open_bi()
        .await
        .map_err(|error| error.to_string())?;
    write_control_frame(
        &mut send,
        &ControlMessage::ClientHello(ClientHello {
            protocol_version: PROTOCOL_VERSION,
            content: content.clone(),
        }),
    )
    .await
    .map_err(|error| error.to_string())?;
    let selection_tick =
        match tokio::time::timeout(Duration::from_secs(5), read_control_frame(&mut receive))
            .await
            .map_err(|_| String::from("gameplay hello made no progress"))?
            .map_err(|error| error.to_string())?
        {
            ControlMessage::ServerHello(hello)
                if hello.protocol_version == PROTOCOL_VERSION && hello.content == content =>
            {
                hello.tick
            }
            ControlMessage::GameplayRejected(reason) => {
                return Err(format!("gameplay handshake rejected: {reason:?}"));
            }
            _ => return Err(String::from("server returned an invalid gameplay hello")),
        };
    let characters =
        match tokio::time::timeout(Duration::from_secs(5), read_control_frame(&mut receive))
            .await
            .map_err(|_| String::from("character list made no progress"))?
            .map_err(|error| error.to_string())?
        {
            ControlMessage::CharacterList(characters) => characters,
            _ => return Err(String::from("server omitted the character list")),
        };
    let interactive = character_name.is_none();
    let mut automatic_request = character_name.map(|name| {
        characters
            .iter()
            .find(|character| character.name == name)
            .map_or_else(
                || CharacterRequest::Create {
                    name,
                    base_stats: CharacterCreationStatsV1::default(),
                },
                |character| CharacterRequest::Select {
                    actor_id: character.actor_id,
                },
            )
    });
    if interactive {
        updates
            .try_send(ClientUpdate::CharacterList(characters))
            .map_err(|_| String::from("client character menu is unavailable"))?;
    }
    let mut selection_heartbeat = tokio::time::interval(Duration::from_secs(5));
    selection_heartbeat.tick().await;
    let controlled_actor = loop {
        let request = if let Some(request) = automatic_request.take() {
            request
        } else {
            loop {
                tokio::select! {
                    action = actions.recv() => match action {
                        Some(ClientAction::ChooseCharacter(request))
                            if !matches!(request, CharacterRequest::List) => break request,
                        Some(ClientAction::Shutdown) | None => {
                            return Err(String::from("client closed during character selection"));
                        }
                        _ => {}
                    },
                    _ = selection_heartbeat.tick() => {
                        write_control_frame(
                            &mut send,
                            &ControlMessage::Heartbeat { tick: selection_tick },
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                }
            }
        };
        write_control_frame(&mut send, &ControlMessage::CharacterRequest(request))
            .await
            .map_err(|error| error.to_string())?;
        match tokio::time::timeout(Duration::from_secs(5), read_control_frame(&mut receive))
            .await
            .map_err(|_| String::from("character response made no progress"))?
            .map_err(|error| error.to_string())?
        {
            ControlMessage::CharacterReady { actor_id } => break actor_id,
            ControlMessage::GameplayRejected(reason) if interactive => {
                updates
                    .try_send(ClientUpdate::CharacterSelectionRejected(reason))
                    .map_err(|_| String::from("client character menu is unavailable"))?;
            }
            ControlMessage::GameplayRejected(reason) => {
                return Err(format!("character selection rejected: {reason:?}"));
            }
            _ => {
                return Err(String::from(
                    "server returned an invalid character response",
                ));
            }
        }
    };
    let mut event_receive = tokio::time::timeout(Duration::from_secs(15), connection.accept_uni())
        .await
        .map_err(|_| String::from("timed out waiting for the authoritative event stream"))?
        .map_err(|error| error.to_string())?;
    match read_control_frame(&mut event_receive)
        .await
        .map_err(|error| error.to_string())?
    {
        ControlMessage::EventStreamReady { actor_id } if actor_id == controlled_actor => {}
        _ => return Err(String::from("server opened an invalid event stream")),
    }
    let mut initial_receive =
        tokio::time::timeout(Duration::from_secs(15), connection.accept_uni())
            .await
            .map_err(|_| String::from("timed out waiting for the initial snapshot stream"))?
            .map_err(|error| error.to_string())?;
    let (initial_actor, mut last_snapshot_sequence, initial_snapshot) = tokio::time::timeout(
        Duration::from_secs(5),
        read_snapshot_stream(&mut initial_receive),
    )
    .await
    .map_err(|_| String::from("initial snapshot stream made no progress"))?
    .map_err(|error| error.to_string())?;
    if initial_actor != controlled_actor || initial_snapshot.controlled_actor.id != controlled_actor
    {
        return Err(String::from(
            "initial snapshot belongs to a different actor",
        ));
    }
    let mut latest_tick = initial_snapshot.tick;
    let mut sequence = initial_snapshot.controlled_actor.last_command_sequence;
    let mut held_sequence = initial_snapshot.controlled_actor.last_held_input_sequence;
    let mut last_reportable_actor = None;
    let _send_result = updates.try_send(ClientUpdate::Snapshot {
        controlled_actor,
        snapshot: Box::new(initial_snapshot),
    });
    let _send_result = updates.try_send(ClientUpdate::Status(String::from(
        "Connected. Move with WASD or the arrow keys.",
    )));
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    heartbeat.tick().await;
    loop {
        tokio::select! {
            action = actions.recv() => {
                let kind = match action {
                    Some(ClientAction::ChooseCharacter(_)) => None,
                    Some(ClientAction::HeldMovement { direction }) => {
                        held_sequence.0 = held_sequence.0.checked_add(1)
                            .ok_or_else(|| String::from("held-input sequence overflow"))?;
                        send_held_movement_datagram(
                            &connection,
                            HeldMovementInputV1 {
                                actor_id: controlled_actor,
                                sequence: held_sequence,
                                client_tick: latest_tick,
                                direction,
                            },
                        )?;
                        None
                    }
                    Some(ClientAction::PickUp { item_id }) => {
                        Some(CommandKind::PickUp { item_id })
                    }
                    Some(ClientAction::Drop { item_id }) => {
                        Some(CommandKind::Drop { item_id })
                    }
                    Some(ClientAction::TakeVehicleCargo {
                        vehicle_id,
                        prototype_part_index,
                        item_id,
                    }) => Some(CommandKind::TakeVehicleCargo {
                        vehicle_id,
                        prototype_part_index,
                        item_id,
                    }),
                    Some(ClientAction::StoreVehicleCargo {
                        vehicle_id,
                        prototype_part_index,
                        item_id,
                    }) => Some(CommandKind::StoreVehicleCargo {
                        vehicle_id,
                        prototype_part_index,
                        item_id,
                    }),
                    Some(ClientAction::SetVehiclePartOpen {
                        vehicle_id,
                        prototype_part_index,
                        open,
                    }) => Some(CommandKind::SetVehiclePartOpen {
                        vehicle_id,
                        prototype_part_index,
                        open,
                    }),
                    Some(ClientAction::Wield { item_id }) => {
                        Some(CommandKind::Wield { item_id })
                    }
                    Some(ClientAction::Wear { item_id }) => Some(CommandKind::Wear { item_id }),
                    Some(ClientAction::TakeOff { item_id }) => {
                        Some(CommandKind::TakeOff { item_id })
                    }
                    Some(ClientAction::Unwield) => Some(CommandKind::Unwield),
                    Some(ClientAction::Consume { item_id }) => {
                        Some(CommandKind::Consume { item_id })
                    }
                    Some(ClientAction::Activate { item_id }) => {
                        Some(CommandKind::Activate { item_id })
                    }
                    Some(ClientAction::TalkToNpc { target }) => {
                        Some(CommandKind::TalkToNpc { target })
                    }
                    Some(ClientAction::BoardVehicle {
                        vehicle_id,
                        prototype_part_index,
                    }) => Some(CommandKind::BoardVehicle {
                        vehicle_id,
                        prototype_part_index,
                    }),
                    Some(ClientAction::UnboardVehicle {
                        vehicle_id,
                        prototype_part_index,
                        dx,
                        dy,
                    }) => Some(CommandKind::UnboardVehicle {
                        vehicle_id,
                        prototype_part_index,
                        dx,
                        dy,
                    }),
                    Some(ClientAction::RespondInteraction {
                        interaction_id,
                        choice_id,
                    }) => Some(CommandKind::RespondInteraction {
                        interaction_id,
                        choice_id,
                    }),
                    Some(ClientAction::CancelInteraction { interaction_id }) => {
                        Some(CommandKind::CancelInteraction { interaction_id })
                    }
                    Some(ClientAction::Craft { recipe_id }) => Some(CommandKind::Craft {
                        recipe_id,
                        recipe: None,
                    }),
                    Some(ClientAction::ResumeCraft) => Some(CommandKind::ResumeCraft),
                    Some(ClientAction::CancelCraft) => Some(CommandKind::CancelCraft),
                    Some(ClientAction::Read {
                        item_id,
                        book_type_id,
                    }) => Some(CommandKind::ReadBook {
                        item_id,
                        book_type_id,
                        study: None,
                    }),
                    Some(ClientAction::ResumeRead) => Some(CommandKind::ResumeRead),
                    Some(ClientAction::CancelRead) => Some(CommandKind::CancelRead),
                    Some(ClientAction::Disassemble {
                        item_id,
                        item_type_id,
                    }) => Some(CommandKind::Disassemble {
                        item_id,
                        item_type_id,
                        recipe: None,
                    }),
                    Some(ClientAction::ResumeDisassembly) => {
                        Some(CommandKind::ResumeDisassembly)
                    }
                    Some(ClientAction::CancelDisassembly) => {
                        Some(CommandKind::CancelDisassembly)
                    }
                    Some(ClientAction::Construct {
                        target,
                        construction_id,
                    }) => Some(CommandKind::Construct {
                        target,
                        construction_id,
                        construction: None,
                    }),
                    Some(ClientAction::ResumeConstruction) => {
                        Some(CommandKind::ResumeConstruction)
                    }
                    Some(ClientAction::CancelConstruction) => {
                        Some(CommandKind::CancelConstruction)
                    }
                    Some(ClientAction::Open { dx, dy }) => Some(CommandKind::Open { dx, dy }),
                    Some(ClientAction::Close { dx, dy }) => Some(CommandKind::Close { dx, dy }),
                    Some(ClientAction::Smash { dx, dy }) => Some(CommandKind::Smash { dx, dy }),
                    Some(ClientAction::Attack { target }) => {
                        Some(CommandKind::Attack { target })
                    }
                    Some(ClientAction::AttackCreature { target }) => {
                        Some(CommandKind::AttackCreature { target })
                    }
                    Some(ClientAction::ShootActor { target }) => {
                        Some(CommandKind::ShootActor { target })
                    }
                    Some(ClientAction::ShootCreature { target }) => {
                        Some(CommandKind::ShootCreature { target })
                    }
                    Some(ClientAction::Reload {
                        ammunition_item,
                        target_pocket_index,
                    }) => {
                        Some(CommandKind::Reload {
                            ammunition_item,
                            target_pocket_index,
                        })
                    }
                    Some(ClientAction::RemovePocketItem {
                        owner_item,
                        pocket_index,
                        contained_item,
                    }) => Some(CommandKind::RemovePocketItem {
                        owner_item,
                        pocket_index,
                        contained_item,
                    }),
                    Some(ClientAction::InsertPocketItem {
                        owner_item,
                        pocket_index,
                        source_item,
                    }) => Some(CommandKind::InsertPocketItem {
                        owner_item,
                        pocket_index,
                        source_item,
                    }),
                    Some(ClientAction::Sleep) => Some(CommandKind::Sleep),
                    Some(ClientAction::Wake) => Some(CommandKind::Wake),
                    Some(ClientAction::Wait) => Some(CommandKind::Wait),
                    Some(ClientAction::Chat { text }) => {
                        if let Some(details) = text.strip_prefix("/report-last ") {
                            let details = details.trim();
                            if details.is_empty()
                                || details.len() > MAX_REPORT_BYTES
                                || details.chars().count() > MAX_REPORT_CHARACTERS
                            {
                                let _send_result = updates.try_send(ClientUpdate::Status(format!(
                                    "Report details must be 1-{MAX_REPORT_BYTES} UTF-8 bytes and at most {MAX_REPORT_CHARACTERS} characters."
                                )));
                            } else if let Some(target_actor) = last_reportable_actor {
                                write_control_frame(
                                    &mut send,
                                    &ControlMessage::ReportSubmit(PlayerReport {
                                        target_actor,
                                        reason: ReportReason::Chat,
                                        details: details.to_owned(),
                                    }),
                                )
                                .await
                                .map_err(|error| error.to_string())?;
                            } else {
                                let _send_result = updates.try_send(ClientUpdate::Status(
                                    String::from("No other chat participant is available to report."),
                                ));
                            }
                        } else {
                            write_control_frame(&mut send, &ControlMessage::ChatSend { text })
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                        None
                    }
                    Some(ClientAction::Shutdown) | None => break,
                };
                if let Some(kind) = kind {
                        sequence.0 = sequence.0.checked_add(1)
                            .ok_or_else(|| String::from("command sequence overflow"))?;
                        write_control_frame(
                            &mut send,
                            &ControlMessage::Command(ClientCommand {
                                actor_id: controlled_actor,
                                sequence,
                                client_tick: latest_tick,
                                kind,
                            }),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                }
            }
            response = read_control_frame(&mut receive) => {
                match response.map_err(|error| error.to_string())? {
                    ControlMessage::Events(_) => {
                        return Err(String::from("server sent events on the control stream"));
                    }
                    ControlMessage::GameplayRejected(reason) => {
                        let _send_result = updates.try_send(ClientUpdate::Status(
                            format!("Command rejected: {}", gameplay_rejection_message(reason)),
                        ));
                    }
                    ControlMessage::ChatReceived(message) => {
                        if message.from_actor != controlled_actor {
                            last_reportable_actor = Some(message.from_actor);
                        }
                        let _send_result = updates.try_send(ClientUpdate::Chat(message));
                    }
                    ControlMessage::ChatRejected(ChatRejection::Muted { until_utc }) => {
                        let _send_result = updates.try_send(ClientUpdate::Status(format!(
                            "Chat is muted until UTC timestamp {until_utc}."
                        )));
                    }
                    ControlMessage::ChatRejected(ChatRejection::ServerBusy) => {
                        let _send_result = updates.try_send(ClientUpdate::Status(String::from(
                            "Chat is temporarily unavailable; try again shortly.",
                        )));
                    }
                    ControlMessage::ReportResponse(ReportResponse::Accepted { report_id }) => {
                        let _send_result = updates.try_send(ClientUpdate::Status(format!(
                            "Report {} was recorded for moderator review.", report_id.0
                        )));
                    }
                    ControlMessage::ReportResponse(ReportResponse::Rejected(reason)) => {
                        let detail = match reason {
                            ReportRejection::CannotReportSelf => {
                                "You cannot report your own account."
                            }
                            ReportRejection::TargetUnavailable => {
                                "That report target is unavailable."
                            }
                            ReportRejection::InvalidReport => "The report details are invalid.",
                            ReportRejection::RateLimited => {
                                "The hourly report limit has been reached."
                            }
                            ReportRejection::ServerBusy => {
                                "Reports are temporarily unavailable; try again shortly."
                            }
                        };
                        let _send_result =
                            updates.try_send(ClientUpdate::Status(detail.to_owned()));
                    }
                    _ => {}
                }
            }
            stream = connection.accept_uni() => {
                let mut stream = stream.map_err(|error| error.to_string())?;
                let (actor_id, snapshot_sequence, snapshot) = tokio::time::timeout(
                    Duration::from_secs(5),
                    read_snapshot_stream(&mut stream),
                )
                .await
                .map_err(|_| String::from("snapshot stream made no progress"))?
                .map_err(|error| error.to_string())?;
                if actor_id != controlled_actor
                    || snapshot.controlled_actor.id != controlled_actor
                    || snapshot_sequence <= last_snapshot_sequence
                {
                    return Err(String::from("server sent an invalid snapshot stream"));
                }
                last_snapshot_sequence = snapshot_sequence;
                latest_tick = snapshot.tick;
                let _send_result = updates.try_send(ClientUpdate::Snapshot {
                    controlled_actor,
                    snapshot: Box::new(snapshot),
                });
            }
            event = read_control_frame(&mut event_receive) => {
                match event.map_err(|error| error.to_string())? {
                    ControlMessage::Events(events) => {
                        let _send_result = updates.try_send(ClientUpdate::Events(events));
                    }
                    _ => return Err(String::from("server sent an invalid event-stream frame")),
                }
            }
            _ = heartbeat.tick() => {
                write_control_frame(
                    &mut send,
                    &ControlMessage::Heartbeat { tick: latest_tick },
                )
                .await
                .map_err(|error| error.to_string())?;
            }
        }
    }
    connection.close(0_u32.into(), b"client exiting");
    endpoint.close().await;
    Ok(())
}

pub(crate) fn require_datagram_support(
    connection: &iroh::endpoint::Connection,
) -> Result<usize, String> {
    match connection.max_datagram_size() {
        Some(maximum) if maximum >= REQUIRED_DATAGRAM_SIZE => Ok(maximum.min(MAX_DATAGRAM_SIZE)),
        _ => Err(String::from(
            "iroh connection does not support the required 1,024-byte datagrams",
        )),
    }
}

fn send_held_movement_datagram(
    connection: &iroh::endpoint::Connection,
    input: HeldMovementInputV1,
) -> Result<(), String> {
    let maximum = require_datagram_support(connection)?;
    let encoded = encode_client_datagram(&ClientDatagramV1::HeldMovement(input))
        .map_err(|error| error.to_string())?;
    if encoded.len() > maximum {
        return Err(String::from(
            "held-input datagram exceeds the current iroh path limit",
        ));
    }
    connection
        .send_datagram(encoded.into())
        .map_err(|error| error.to_string())
}

const fn gameplay_rejection_message(reason: GameplayRejection) -> &'static str {
    match reason {
        GameplayRejection::AuthenticationRequired => "authentication required",
        GameplayRejection::ContentMismatch => "content mismatch",
        GameplayRejection::InvalidCharacterName => "invalid character name",
        GameplayRejection::CharacterNotOwned => "character is not owned by this account",
        GameplayRejection::CharacterAlreadyExists => "character already exists",
        GameplayRejection::NoSpawnLocation => "no spawn location is available",
        GameplayRejection::SessionAlreadyActive => "account or character is already connected",
        GameplayRejection::ServerFull => "server has reached its player limit",
        GameplayRejection::ServerBusy => "server busy",
        GameplayRejection::UnexpectedMessage => "unexpected command",
    }
}

async fn enroll(
    secret_key: SecretKey,
    profile: &Path,
    address_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let server_address: EndpointAddr = serde_json::from_slice(&std::fs::read(address_path)?)?;
    verify_existing_pin(profile, server_address.id)?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .bind()
        .await?;
    let connection = endpoint
        .connect(server_address.clone(), ENROLL_ALPN)
        .await?;
    if connection.remote_id() != server_address.id {
        return Err("iroh connected to an unexpected server identity".into());
    }
    let (mut send, mut receive) = connection.open_bi().await?;
    write_control_frame(
        &mut send,
        &ControlMessage::EnrollmentRequest {
            protocol_version: PROTOCOL_VERSION,
        },
    )
    .await?;
    send.finish()?;
    let response = read_control_frame(&mut receive).await?;
    pin_server(profile, server_address.id)?;
    connection.close(0_u32.into(), b"enrollment response received");
    endpoint.close().await;
    match response {
        ControlMessage::EnrollmentAccepted(accepted) => Ok(format!(
            "Enrolled as {} ({:?}). Account {} is ready for gameplay.",
            accepted.display_name, accepted.role, accepted.account_id
        )),
        ControlMessage::EnrollmentRejected(reason) => {
            Err(enrollment_rejection_message(reason).into())
        }
        _ => Err("server returned an unexpected enrollment response".into()),
    }
}

pub(crate) fn verify_existing_pin(
    profile: &Path,
    expected: EndpointId,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = profile.join("server-endpoint-id");
    match std::fs::read_to_string(path) {
        Ok(stored) if EndpointId::from_str(stored.trim())? == expected => Ok(()),
        Ok(_) => Err("server identity changed; refusing silent replacement".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn pin_server(
    profile: &Path,
    endpoint_id: EndpointId,
) -> Result<(), Box<dyn std::error::Error>> {
    verify_existing_pin(profile, endpoint_id)?;
    let path = profile.join("server-endpoint-id");
    if path.exists() {
        return Ok(());
    }
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    writeln!(file, "{endpoint_id}")?;
    file.sync_all()?;
    Ok(())
}

const fn enrollment_rejection_message(reason: EnrollmentRejection) -> &'static str {
    match reason {
        EnrollmentRejection::UnknownIdentity => {
            "the server has no pending enrollment for this exact endpoint"
        }
        EnrollmentRejection::Expired => "the server's pending enrollment expired",
        EnrollmentRejection::AccountUnavailable => "the account is unavailable",
        EnrollmentRejection::ServerBusy => "the server is busy; retry later",
        EnrollmentRejection::ProtocolMismatch => "client and server protocol versions differ",
    }
}

fn setup(mut commands: Commands, status: Res<BootstrapStatus>, game: Option<Res<GameClient>>) {
    commands.spawn(Camera2d);
    const TILE_SIZE: f32 = 36.0;
    for y in 0..12 {
        for x in 0..12 {
            let shade = if (x + y) % 2 == 0 { 0.10 } else { 0.12 };
            commands.spawn((
                Sprite::from_color(
                    Color::srgb(shade, shade + 0.01, shade + 0.02),
                    Vec2::splat(34.0),
                ),
                Transform::from_xyz(
                    (x as f32 - 5.5) * TILE_SIZE,
                    (5.5 - y as f32) * TILE_SIZE,
                    -1.0,
                ),
                TileVisual { x, y },
            ));
        }
    }
    let detail = game
        .as_ref()
        .map_or(status.detail.as_str(), |game| game.status.as_str());
    commands.spawn((
        Text::new(format!(
            "Cataclysm: Dark Days Ahead — Rust Multiplayer\n\nClient endpoint\n{}\n\n{}",
            status.identity, detail
        )),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.9, 0.92)),
        Node {
            position_type: PositionType::Absolute,
            top: px(32),
            left: px(32),
            max_width: percent(90),
            ..default()
        },
        StatusText,
    ));
}

fn handle_character_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut keyboard_inputs: MessageReader<KeyboardInput>,
    mut menu: ResMut<CharacterMenu>,
    game: Option<Res<GameClient>>,
) {
    if !menu.is_active() || menu.waiting {
        for _event in keyboard_inputs.read() {}
        return;
    }
    let Some(game) = game else {
        return;
    };
    if menu.creating {
        if keys.just_pressed(KeyCode::ArrowUp) {
            menu.selected_stat = menu.selected_stat.checked_sub(1).unwrap_or(3);
        } else if keys.just_pressed(KeyCode::ArrowDown) {
            menu.selected_stat = (menu.selected_stat + 1) % 4;
        } else if keys.just_pressed(KeyCode::ArrowLeft) {
            menu.adjust_selected_stat(-1);
        } else if keys.just_pressed(KeyCode::ArrowRight) {
            menu.adjust_selected_stat(1);
        }
        for event in keyboard_inputs.read() {
            if !event.state.is_pressed() {
                continue;
            }
            match &event.logical_key {
                Key::Enter => {
                    let name = menu.name.trim().to_owned();
                    if !valid_character_name_input(&name) {
                        menu.notice = String::from(
                            "Name must be 1-64 characters, at most 256 UTF-8 bytes, with no controls.",
                        );
                    } else if game
                        .actions
                        .try_send(ClientAction::ChooseCharacter(CharacterRequest::Create {
                            name,
                            base_stats: menu.base_stats,
                        }))
                        .is_ok()
                    {
                        menu.waiting = true;
                        menu.notice = String::from("Creating character…");
                    } else {
                        menu.notice = String::from("The client command queue is busy; try again.");
                    }
                }
                Key::Escape => {
                    menu.creating = false;
                    menu.name.clear();
                    menu.base_stats = CharacterCreationStatsV1::default();
                    menu.selected_stat = 0;
                    menu.notice.clear();
                }
                Key::Backspace => {
                    menu.name.pop();
                }
                Key::Character(character)
                    if !character.chars().any(char::is_control)
                        && menu.name.len().saturating_add(character.len()) <= 256
                        && menu
                            .name
                            .chars()
                            .count()
                            .saturating_add(character.chars().count())
                            <= 64 =>
                {
                    menu.name.push_str(character);
                }
                _ => {}
            }
        }
        return;
    }
    for _event in keyboard_inputs.read() {}
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
        menu.select_previous();
    } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
        menu.select_next();
    } else if keys.just_pressed(KeyCode::Enter) {
        let character_count = menu.characters.as_ref().map_or(0, Vec::len);
        if menu.selected == character_count {
            menu.creating = true;
            menu.name.clear();
            menu.base_stats = CharacterCreationStatsV1::default();
            menu.selected_stat = 0;
            menu.notice.clear();
        } else if let Some(actor_id) = menu
            .characters
            .as_ref()
            .and_then(|characters| characters.get(menu.selected))
            .map(|character| character.actor_id)
        {
            if game
                .actions
                .try_send(ClientAction::ChooseCharacter(CharacterRequest::Select {
                    actor_id,
                }))
                .is_ok()
            {
                menu.waiting = true;
                menu.notice = String::from("Selecting character…");
            } else {
                menu.notice = String::from("The client command queue is busy; try again.");
            }
        }
    }
}

impl CharacterMenu {
    fn is_active(&self) -> bool {
        self.characters.is_some()
    }

    fn show(&mut self, characters: Vec<CharacterSummary>) {
        self.characters = Some(characters);
        self.selected = 0;
        self.creating = false;
        self.name.clear();
        self.base_stats = CharacterCreationStatsV1::default();
        self.selected_stat = 0;
        self.waiting = false;
        self.notice.clear();
    }

    fn reject(&mut self, reason: GameplayRejection) {
        self.waiting = false;
        self.notice = format!(
            "Character request rejected: {}.",
            gameplay_rejection_message(reason)
        );
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn choice_count(&self) -> usize {
        self.characters.as_ref().map_or(1, |characters| {
            characters.len() + usize::from(characters.len() < MAX_CHARACTERS_PER_ACCOUNT)
        })
    }

    fn select_previous(&mut self) {
        let count = self.choice_count();
        self.selected = self.selected.checked_sub(1).unwrap_or(count - 1);
    }

    fn select_next(&mut self) {
        self.selected = (self.selected + 1) % self.choice_count();
    }

    fn display(&self) -> String {
        let Some(characters) = &self.characters else {
            return String::new();
        };
        if self.creating {
            let stats = [
                ("STR", self.base_stats.strength),
                ("DEX", self.base_stats.dexterity),
                ("INT", self.base_stats.intelligence),
                ("PER", self.base_stats.perception),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (name, value))| {
                format!(
                    "{} {name} {value}",
                    if index == self.selected_stat {
                        ">"
                    } else {
                        " "
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
            return format!(
                "\n\nCreate character — type a name; arrows select and adjust stats ({MIN_CHARACTER_CREATION_STAT}-{MAX_CHARACTER_CREATION_STAT}); Enter confirms; Escape returns\nName> {}\n{}\n{}",
                self.name, stats, self.notice
            );
        }
        const VISIBLE_ENTRIES: usize = 9;
        let count = self.choice_count();
        let half = VISIBLE_ENTRIES / 2;
        let start = self
            .selected
            .saturating_sub(half)
            .min(count.saturating_sub(VISIBLE_ENTRIES));
        let end = start.saturating_add(VISIBLE_ENTRIES).min(count);
        let choices = (start..end)
            .map(|index| {
                let label = characters.get(index).map_or_else(
                    || String::from("Create new character"),
                    |character| format!("{} [{}]", character.name, character.actor_id),
                );
                format!(
                    "{} {}",
                    if index == self.selected { ">" } else { " " },
                    label
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\nChoose character ({}/{}) — arrows or J/K select, Enter confirms\n{}\n{}",
            self.selected + 1,
            count,
            choices,
            self.notice
        )
    }

    fn adjust_selected_stat(&mut self, delta: i16) {
        let stat = match self.selected_stat {
            0 => &mut self.base_stats.strength,
            1 => &mut self.base_stats.dexterity,
            2 => &mut self.base_stats.intelligence,
            _ => &mut self.base_stats.perception,
        };
        *stat = u16::try_from(i32::from(*stat) + i32::from(delta))
            .unwrap_or(MIN_CHARACTER_CREATION_STAT)
            .clamp(MIN_CHARACTER_CREATION_STAT, MAX_CHARACTER_CREATION_STAT);
    }
}

fn valid_character_name_input(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}

fn handle_interaction_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<InteractionMenu>,
    game: Option<ResMut<GameClient>>,
) {
    let Some(mut game) = game else {
        return;
    };
    let pending = game
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.controlled_actor.pending_interaction.as_ref())
        .cloned();
    let Some(pending) = pending else {
        menu.interaction_id = None;
        menu.selected = 0;
        menu.waiting = false;
        return;
    };
    if menu.interaction_id != Some(pending.interaction_id) {
        menu.interaction_id = Some(pending.interaction_id);
        menu.selected = 0;
        menu.waiting = false;
    }
    if menu.waiting || pending.choices.is_empty() {
        return;
    }
    if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
        menu.selected = menu
            .selected
            .checked_sub(1)
            .unwrap_or(pending.choices.len() - 1);
    } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
        menu.selected = (menu.selected + 1) % pending.choices.len();
    } else if keys.just_pressed(KeyCode::Escape) {
        if game
            .actions
            .try_send(ClientAction::CancelInteraction {
                interaction_id: pending.interaction_id,
            })
            .is_ok()
        {
            menu.waiting = true;
            game.notice = String::from("Canceling interaction…");
        }
    } else if keys.just_pressed(KeyCode::Enter)
        && let Some(choice) = pending.choices.get(menu.selected)
        && game
            .actions
            .try_send(ClientAction::RespondInteraction {
                interaction_id: pending.interaction_id,
                choice_id: choice.choice_id.clone(),
            })
            .is_ok()
    {
        menu.waiting = true;
        game.notice = format!("Selected {}…", choice.label);
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems declare disjoint resource access as parameters.
fn handle_chat_input(
    mut keyboard_inputs: MessageReader<KeyboardInput>,
    mut composer: ResMut<ChatComposer>,
    character_menu: Res<CharacterMenu>,
    menu: Res<ItemMenu>,
    craft_menu: Res<CraftMenu>,
    target_menu: Res<TargetMenu>,
    terrain_menu: Res<TerrainMenu>,
    interaction_menu: Res<InteractionMenu>,
    game: Option<Res<GameClient>>,
) {
    let Some(game) = game else {
        return;
    };
    for event in keyboard_inputs.read() {
        if !event.state.is_pressed() {
            continue;
        }
        if !composer.active {
            if character_menu.is_active()
                || menu.action.is_some()
                || craft_menu.open
                || target_menu.action.is_some()
                || terrain_menu.action.is_some()
                || interaction_menu.is_active()
            {
                continue;
            }
            if event.logical_key == Key::Enter {
                composer.active = true;
            }
            continue;
        }
        match &event.logical_key {
            Key::Enter => {
                let text = composer.text.trim().to_owned();
                if !text.is_empty() {
                    let _send_result = game.actions.try_send(ClientAction::Chat { text });
                }
                composer.text.clear();
                composer.active = false;
            }
            Key::Escape => {
                composer.text.clear();
                composer.active = false;
            }
            Key::Backspace => {
                composer.text.pop();
            }
            Key::Character(character)
                if !character.chars().any(char::is_control)
                    && composer.text.len().saturating_add(character.len()) <= MAX_CHAT_BYTES =>
            {
                composer.text.push_str(character);
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems declare disjoint resource access as parameters.
fn handle_item_menu(
    keys: Res<ButtonInput<KeyCode>>,
    composer: Res<ChatComposer>,
    interaction_menu: Res<InteractionMenu>,
    target_menu: Res<TargetMenu>,
    terrain_menu: Res<TerrainMenu>,
    craft_menu: Res<CraftMenu>,
    mut menu: ResMut<ItemMenu>,
    game: Option<ResMut<GameClient>>,
    content_items: Option<Res<ContentItems>>,
    content_ammunition: Option<Res<ContentAmmunition>>,
    content_recipes: Option<Res<ContentRecipes>>,
) {
    if composer.active
        || interaction_menu.is_active()
        || target_menu.action.is_some()
        || terrain_menu.action.is_some()
        || craft_menu.open
    {
        return;
    }
    let Some(mut game) = game else {
        return;
    };
    if let Some(action) = menu.action {
        if keys.just_pressed(KeyCode::Escape) {
            menu.clear();
        } else if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
            menu.select_previous();
        } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
            menu.select_next();
        } else if keys.just_pressed(KeyCode::Enter)
            && let Some(entry) = menu.entries.get(menu.selected)
        {
            if let Some(client_action) =
                client_action_for_item_menu(action, entry, game.snapshot.as_ref())
            {
                let _send_result = game.actions.try_send(client_action);
            }
            menu.clear();
        }
        return;
    }
    let Some(snapshot) = &game.snapshot else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyX) && snapshot.controlled_actor.disassembly_activity.is_some()
    {
        let _send_result = game.actions.try_send(ClientAction::CancelDisassembly);
        return;
    }
    if keys.just_pressed(KeyCode::KeyN)
        && let Some(activity) = &snapshot.controlled_actor.disassembly_activity
    {
        if activity.interrupted {
            let _send_result = game.actions.try_send(ClientAction::ResumeDisassembly);
        } else {
            game.notice = String::from("Disassembly is already in progress; X cancels it.");
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyX) && snapshot.controlled_actor.read_activity.is_some() {
        let _send_result = game.actions.try_send(ClientAction::CancelRead);
        return;
    }
    if keys.just_pressed(KeyCode::KeyV)
        && let Some(activity) = &snapshot.controlled_actor.read_activity
    {
        if activity.interrupted {
            let _send_result = game.actions.try_send(ClientAction::ResumeRead);
        } else {
            game.notice = String::from("Reading is already in progress; X cancels it.");
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyY) {
        if snapshot.controlled_actor.craft_activity.is_some()
            || snapshot.controlled_actor.read_activity.is_some()
            || snapshot.controlled_actor.disassembly_activity.is_some()
            || snapshot.controlled_actor.construction_activity.is_some()
        {
            game.notice = String::from("Finish or cancel the current activity first.");
        } else if let Some(action) = first_pocket_item_removal(snapshot) {
            let _send_result = game.actions.try_send(action);
        } else {
            game.notice = String::from("No carried pocket item can be removed.");
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyI) {
        if snapshot.controlled_actor.craft_activity.is_some()
            || snapshot.controlled_actor.read_activity.is_some()
            || snapshot.controlled_actor.disassembly_activity.is_some()
            || snapshot.controlled_actor.construction_activity.is_some()
        {
            game.notice = String::from("Finish or cancel the current activity first.");
        } else if let Some(action) = first_pocket_item_insertion(snapshot) {
            let _send_result = game.actions.try_send(action);
        } else {
            game.notice = String::from("No carried ammunition fits a carried container pocket.");
        }
        return;
    }
    let action = if keys.just_pressed(KeyCode::KeyG) {
        Some(ItemMenuAction::PickUp)
    } else if keys.just_pressed(KeyCode::KeyQ) {
        Some(ItemMenuAction::Drop)
    } else if keys.just_pressed(KeyCode::KeyE) {
        Some(ItemMenuAction::Wield)
    } else if keys.just_pressed(KeyCode::KeyW) {
        Some(ItemMenuAction::Wear)
    } else if keys.just_pressed(KeyCode::KeyD) {
        Some(ItemMenuAction::TakeOff)
    } else if keys.just_pressed(KeyCode::KeyU) {
        Some(ItemMenuAction::Reload)
    } else if keys.just_pressed(KeyCode::KeyC) {
        Some(ItemMenuAction::Consume)
    } else if keys.just_pressed(KeyCode::KeyP) {
        Some(ItemMenuAction::Activate)
    } else if keys.just_pressed(KeyCode::KeyV) {
        Some(ItemMenuAction::Read)
    } else if keys.just_pressed(KeyCode::KeyN) {
        Some(ItemMenuAction::Disassemble)
    } else {
        None
    };
    let Some(action) = action else {
        return;
    };
    if snapshot.controlled_actor.craft_activity.is_some()
        || snapshot.controlled_actor.read_activity.is_some()
        || snapshot.controlled_actor.disassembly_activity.is_some()
        || snapshot.controlled_actor.construction_activity.is_some()
    {
        game.notice = String::from("Finish or cancel the current activity first.");
        return;
    }
    let entries = item_menu_entries(
        action,
        snapshot,
        content_items.as_deref(),
        content_ammunition.as_deref(),
        content_recipes.as_deref(),
    );
    match entries.as_slice() {
        [] => {
            game.notice = format!("No item is available to {}.", action.verb());
        }
        [entry] => {
            if let Some(client_action) = client_action_for_item_menu(action, entry, Some(snapshot))
            {
                let _send_result = game.actions.try_send(client_action);
            }
        }
        _ => {
            menu.action = Some(action);
            menu.entries = entries;
            menu.selected = 0;
        }
    }
}

impl ItemMenuAction {
    const fn title(self) -> &'static str {
        match self {
            Self::PickUp => "Pick up",
            Self::Drop => "Drop",
            Self::Wield => "Wield",
            Self::Wear => "Wear",
            Self::TakeOff => "Take off",
            Self::Reload => "Reload with",
            Self::Consume => "Consume",
            Self::Activate => "Activate/deactivate",
            Self::Read => "Read",
            Self::Disassemble => "Disassemble",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::PickUp => "pick up",
            Self::Drop => "drop",
            Self::Wield => "wield",
            Self::Wear => "wear",
            Self::TakeOff => "take off",
            Self::Reload => "reload with",
            Self::Consume => "consume",
            Self::Activate => "activate or deactivate",
            Self::Read => "read",
            Self::Disassemble => "disassemble",
        }
    }
}

impl ItemMenu {
    fn clear(&mut self) {
        self.action = None;
        self.entries.clear();
        self.selected = 0;
    }

    fn select_previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or(self.entries.len() - 1);
    }

    fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
    }

    fn display(&self) -> String {
        let Some(action) = self.action else {
            return String::new();
        };
        const VISIBLE_ENTRIES: usize = 9;
        let half = VISIBLE_ENTRIES / 2;
        let start = self
            .selected
            .saturating_sub(half)
            .min(self.entries.len().saturating_sub(VISIBLE_ENTRIES));
        let end = start
            .saturating_add(VISIBLE_ENTRIES)
            .min(self.entries.len());
        let choices = self.entries[start..end]
            .iter()
            .enumerate()
            .map(|(offset, entry)| {
                let index = start + offset;
                format!(
                    "{} {}",
                    if index == self.selected { ">" } else { " " },
                    entry.label
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\n{} ({}/{}) — arrows or J/K select, Enter confirms, Escape cancels\n{}",
            action.title(),
            self.selected + 1,
            self.entries.len(),
            choices
        )
    }
}

impl CraftMenu {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn select_previous(&mut self) {
        let len = if self.target_construction.is_some() {
            self.targets.len()
        } else {
            self.entries.len()
        };
        if len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(len - 1);
        }
    }

    fn select_next(&mut self) {
        let len = if self.target_construction.is_some() {
            self.targets.len()
        } else {
            self.entries.len()
        };
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    fn display(&self) -> String {
        if !self.open {
            return String::new();
        }
        const VISIBLE_ENTRIES: usize = 9;
        let (title, labels) = if self.target_construction.is_some() {
            (
                "Construction target",
                self.targets
                    .iter()
                    .map(|entry| entry.label.as_str())
                    .collect::<Vec<_>>(),
            )
        } else {
            (
                if self.construction {
                    "Construct"
                } else {
                    "Craft"
                },
                self.entries
                    .iter()
                    .map(|entry| entry.label.as_str())
                    .collect::<Vec<_>>(),
            )
        };
        let half = VISIBLE_ENTRIES / 2;
        let start = self
            .selected
            .saturating_sub(half)
            .min(labels.len().saturating_sub(VISIBLE_ENTRIES));
        let end = start.saturating_add(VISIBLE_ENTRIES).min(labels.len());
        let choices = labels[start..end]
            .iter()
            .enumerate()
            .map(|(offset, label)| {
                let index = start + offset;
                format!(
                    "{} {}",
                    if index == self.selected { ">" } else { " " },
                    label
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\n{title} ({}/{}) — arrows or J/K select, Enter confirms, Escape cancels\n{}",
            self.selected + 1,
            labels.len(),
            choices
        )
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems declare disjoint resource access as parameters.
fn handle_craft_menu(
    keys: Res<ButtonInput<KeyCode>>,
    composer: Res<ChatComposer>,
    interaction_menu: Res<InteractionMenu>,
    item_menu: Res<ItemMenu>,
    target_menu: Res<TargetMenu>,
    terrain_menu: Res<TerrainMenu>,
    mut menu: ResMut<CraftMenu>,
    game: Option<ResMut<GameClient>>,
    recipes: Option<Res<ContentRecipes>>,
    items: Option<Res<ContentItems>>,
    constructions: Option<Res<ContentConstructions>>,
    terrain: Option<Res<ContentTerrain>>,
    furniture: Option<Res<ContentFurniture>>,
) {
    if composer.active
        || interaction_menu.is_active()
        || item_menu.action.is_some()
        || target_menu.action.is_some()
        || terrain_menu.action.is_some()
    {
        return;
    }
    let Some(mut game) = game else {
        return;
    };
    if menu.open {
        if keys.just_pressed(KeyCode::Escape) {
            menu.clear();
        } else if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
            menu.select_previous();
        } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
            menu.select_next();
        } else if keys.just_pressed(KeyCode::Enter) {
            if let Some(construction_id) = menu.target_construction.clone() {
                if let Some(entry) = menu.targets.get(menu.selected) {
                    let _send_result = game.actions.try_send(ClientAction::Construct {
                        target: entry.target,
                        construction_id,
                    });
                    menu.clear();
                }
            } else if let Some(entry) = menu.entries.get(menu.selected) {
                let entry_id = entry.recipe_id.clone();
                if menu.construction {
                    let Some(snapshot) = &game.snapshot else {
                        menu.clear();
                        return;
                    };
                    let Some(construction) = constructions
                        .as_ref()
                        .and_then(|content| content.0.get(&entry_id))
                    else {
                        game.notice = String::from("Pinned construction content is unavailable.");
                        menu.clear();
                        return;
                    };
                    let targets = construction_target_menu_entries(
                        snapshot,
                        &construction.pre_terrain,
                        construction.pre_special.as_slice() == ["check_empty"],
                        furniture.as_ref().is_some_and(|content| {
                            content.0.get(&construction.post_terrain).is_some()
                        }),
                    );
                    if targets.is_empty() {
                        game.notice =
                            String::from("No visible adjacent tile satisfies this construction.");
                        menu.clear();
                    } else {
                        menu.target_construction = Some(entry_id);
                        menu.targets = targets;
                        menu.selected = 0;
                    }
                } else {
                    let _send_result = game.actions.try_send(ClientAction::Craft {
                        recipe_id: entry_id,
                    });
                    menu.clear();
                }
            }
        }
        return;
    }
    let Some(snapshot) = &game.snapshot else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyX) && snapshot.controlled_actor.craft_activity.is_some() {
        let _send_result = game.actions.try_send(ClientAction::CancelCraft);
        return;
    }
    if keys.just_pressed(KeyCode::KeyX) && snapshot.controlled_actor.construction_activity.is_some()
    {
        let _send_result = game.actions.try_send(ClientAction::CancelConstruction);
        return;
    }
    let wants_craft = keys.just_pressed(KeyCode::KeyB);
    let wants_construction = keys.just_pressed(KeyCode::KeyM);
    if !wants_craft && !wants_construction {
        return;
    }
    if wants_craft && let Some(activity) = &snapshot.controlled_actor.craft_activity {
        if activity.interrupted {
            let _send_result = game.actions.try_send(ClientAction::ResumeCraft);
        } else {
            game.notice = String::from("Crafting is already in progress; X cancels it.");
        }
        return;
    }
    if wants_construction && let Some(activity) = &snapshot.controlled_actor.construction_activity {
        if activity.interrupted {
            let _send_result = game.actions.try_send(ClientAction::ResumeConstruction);
        } else {
            game.notice = String::from("Construction is already in progress; X cancels it.");
        }
        return;
    }
    if snapshot.controlled_actor.craft_activity.is_some()
        || snapshot.controlled_actor.read_activity.is_some()
        || snapshot.controlled_actor.disassembly_activity.is_some()
        || snapshot.controlled_actor.construction_activity.is_some()
    {
        game.notice = String::from("Finish or cancel the current activity first.");
        return;
    }
    if wants_construction {
        let (Some(constructions), Some(recipes), Some(items), Some(terrain), Some(furniture)) =
            (constructions, recipes, items, terrain, furniture)
        else {
            game.notice = String::from("Pinned construction content is unavailable.");
            return;
        };
        let entries = construction_menu_entries(
            snapshot,
            &constructions.0,
            &recipes.0,
            &items.0,
            &terrain.0,
            &furniture.0,
        );
        if entries.is_empty() {
            game.notice = String::from("No supported construction can use the carried components.");
            return;
        }
        menu.open = true;
        menu.construction = true;
        menu.entries = entries;
        menu.selected = 0;
        return;
    }
    let (Some(recipes), Some(items)) = (recipes, items) else {
        game.notice = String::from("Pinned recipe content is unavailable.");
        return;
    };
    let entries = craft_menu_entries(snapshot, &recipes.0, &items.0);
    match entries.as_slice() {
        [] => game.notice = String::from("No known recipe can use the carried components."),
        [entry] => {
            let _send_result = game.actions.try_send(ClientAction::Craft {
                recipe_id: entry.recipe_id.clone(),
            });
        }
        _ => {
            menu.open = true;
            menu.construction = false;
            menu.entries = entries;
            menu.selected = 0;
        }
    }
}

fn construction_menu_entries(
    snapshot: &ReplicationSnapshotV1,
    constructions: &ConstructionRegistry,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    terrain: &TerrainRegistry,
    furniture: &FurnitureRegistry,
) -> Vec<CraftMenuEntry> {
    let mut entries = constructions
        .iter()
        .filter_map(|(construction_id, construction)| {
            let group = constructions.group(&construction.group)?;
            let quality_items = client_construction_quality_items(snapshot, construction, items)?;
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
            if !construction.unsupported_fields.is_empty()
                || !group.unsupported_fields.is_empty()
                || construction.activity_level != "LIGHT_EXERCISE"
                || construction.components.is_empty()
                || !supported_target_predicate
                || (furniture.get(&construction.post_terrain).is_none()
                    && terrain.get(&construction.post_terrain).is_none())
                || construction
                    .required_skills
                    .iter()
                    .any(|(skill_id, level)| {
                        client_skill_level(&snapshot.controlled_actor, skill_id, false) < *level
                    })
                || !client_construction_components_available(
                    snapshot,
                    construction,
                    recipes,
                    items,
                    &quality_items,
                )
            {
                return None;
            }
            Some(CraftMenuEntry {
                recipe_id: construction_id.to_owned(),
                label: format!(
                    "{} — {} [{}]",
                    group.name,
                    craft_duration_label(construction.time_moves),
                    construction_id
                ),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.recipe_id.cmp(&right.recipe_id));
    entries
}

fn client_construction_components_available(
    snapshot: &ReplicationSnapshotV1,
    construction: &cdda_content::ConstructionDefinition,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    protected: &BTreeSet<ItemId>,
) -> bool {
    let Ok(component_groups) =
        recipes.resolved_component_groups(&construction.id, &construction.components)
    else {
        return false;
    };
    let mut planned = BTreeMap::<ItemId, u32>::new();
    for group in &component_groups {
        let selected = group.iter().find(|component| {
            let Some(definition) = items.get(&component.type_id) else {
                return false;
            };
            let count_by_charges = definition.count_by_charges();
            let available = snapshot
                .controlled_actor
                .inventory
                .iter()
                .filter(|item| {
                    item.type_id == component.type_id
                        && item.residual_energy_millijoules == 0
                        && !protected.contains(&item.id)
                        && item
                            .magazine_wells
                            .iter()
                            .all(|well| well.installed_magazine.is_none())
                        && item
                            .integral_magazines
                            .iter()
                            .all(|pocket| pocket.loaded_ammunition.is_none())
                })
                .fold(0_u32, |total, item| {
                    let units = if count_by_charges {
                        u32::try_from(item.charges).unwrap_or(0)
                    } else {
                        1
                    };
                    total.saturating_add(
                        units.saturating_sub(planned.get(&item.id).copied().unwrap_or(0)),
                    )
                });
            available >= component.count
        });
        let Some(selected) = selected else {
            return false;
        };
        let Some(definition) = items.get(&selected.type_id) else {
            return false;
        };
        let count_by_charges = definition.count_by_charges();
        let mut remaining = selected.count;
        for item in snapshot.controlled_actor.inventory.iter().filter(|item| {
            item.type_id == selected.type_id
                && item.residual_energy_millijoules == 0
                && !protected.contains(&item.id)
                && item
                    .magazine_wells
                    .iter()
                    .all(|well| well.installed_magazine.is_none())
                && item
                    .integral_magazines
                    .iter()
                    .all(|pocket| pocket.loaded_ammunition.is_none())
        }) {
            let units = if count_by_charges {
                u32::try_from(item.charges).unwrap_or(0)
            } else {
                1
            };
            let entry = planned.entry(item.id).or_default();
            let take = units.saturating_sub(*entry).min(remaining);
            *entry = entry.saturating_add(take);
            remaining -= take;
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return false;
        }
    }
    true
}

fn client_construction_quality_items(
    snapshot: &ReplicationSnapshotV1,
    construction: &cdda_content::ConstructionDefinition,
    items: &ItemRegistry,
) -> Option<BTreeSet<ItemId>> {
    let mut selected = BTreeSet::new();
    for group in &construction.qualities {
        let choice = group.iter().find_map(|quality| {
            let ids = snapshot
                .controlled_actor
                .inventory
                .iter()
                .filter(|carried| {
                    items.get(&carried.type_id).is_some_and(|definition| {
                        let inherent = !definition.unsupported_fields.contains("qualities")
                            && definition
                                .qualities
                                .get(&quality.quality_id)
                                .is_some_and(|provided| provided.level >= quality.level);
                        let charged = !definition.unsupported_fields.contains("charged_qualities")
                            && definition
                                .charged_qualities
                                .get(&quality.quality_id)
                                .is_some_and(|provided| provided.level >= quality.level)
                            && definition.charges_per_use > 0
                            && item_tool_charges(carried) >= definition.charges_per_use;
                        inherent || charged
                    })
                })
                .take(usize::try_from(quality.amount).ok()?)
                .map(|item| item.id)
                .collect::<Vec<_>>();
            (ids.len() == usize::try_from(quality.amount).ok()?).then_some(ids)
        })?;
        selected.extend(choice);
    }
    Some(selected)
}

fn construction_target_menu_entries(
    snapshot: &ReplicationSnapshotV1,
    pre_terrain: &[String],
    requires_empty: bool,
    result_is_furniture: bool,
) -> Vec<ConstructionTargetMenuEntry> {
    let position = snapshot.controlled_actor.position;
    [
        (-1, -1, "northwest"),
        (0, -1, "north"),
        (1, -1, "northeast"),
        (1, 0, "east"),
        (1, 1, "southeast"),
        (0, 1, "south"),
        (-1, 1, "southwest"),
        (-1, 0, "west"),
    ]
    .into_iter()
    .filter_map(|(dx, dy, direction)| {
        let target = position.checked_offset(dx, dy, 0)?;
        let (chunk_coord, local) = target.chunk_and_local();
        let tile = snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coord == chunk_coord)?
            .tiles
            .get(usize::from(local.y) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(local.x))?
            .as_ref()
            .filter(|tile| tile.currently_visible)?;
        let occupied = snapshot
            .visible_actors
            .iter()
            .any(|actor| actor.hp > 0 && actor.position == target)
            || snapshot.npcs.iter().any(|npc| npc.position == target)
            || snapshot
                .creatures
                .iter()
                .any(|creature| creature.hp > 0 && creature.position == target)
            || snapshot
                .ground_items
                .iter()
                .any(|ground| ground.position == target);
        let exact_prerequisite_matches = !pre_terrain.is_empty()
            && (pre_terrain.contains(&tile.terrain.terrain_id)
                || tile
                    .furniture
                    .as_ref()
                    .is_some_and(|placed| pre_terrain.contains(&placed.furniture_id)));
        let target_is_valid = if requires_empty {
            tile.terrain.flat && tile.furniture.is_none() && !occupied
        } else {
            exact_prerequisite_matches && (!result_is_furniture || tile.furniture.is_none())
        };
        target_is_valid.then(|| ConstructionTargetMenuEntry {
            target,
            label: format!(
                "{direction} — {} ({}, {}, {})",
                tile.terrain.terrain_id, target.x, target.y, target.z
            ),
        })
    })
    .collect()
}

fn craft_menu_entries(
    snapshot: &ReplicationSnapshotV1,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
) -> Vec<CraftMenuEntry> {
    let mut entries = recipes
        .craftable_with_knowledge_source()
        .filter(|recipe| can_craft_recipe(snapshot, recipes, items, recipe))
        .map(|recipe| {
            let result_name = items
                .get(&recipe.result)
                .map_or(recipe.result.as_str(), |item| item.name.as_str());
            let skill = if recipe.skill_used.is_empty() {
                String::new()
            } else {
                format!(" — {} {}", recipe.skill_used, recipe.difficulty)
            };
            CraftMenuEntry {
                recipe_id: recipe.id.clone(),
                label: format!(
                    "{} — {}{} [{}]",
                    result_name,
                    craft_duration_label(recipe.time_moves),
                    skill,
                    recipe.id
                ),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.recipe_id.cmp(&right.recipe_id));
    entries
}

fn can_craft_recipe(
    snapshot: &ReplicationSnapshotV1,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    recipe: &cdda_content::RecipeDefinition,
) -> bool {
    if !client_recipe_skills_allow(&snapshot.controlled_actor, items, recipe) {
        return false;
    }
    if !client_recipe_proficiencies_allow(&snapshot.controlled_actor, recipe) {
        return false;
    }
    let Some(protected) = client_craft_support_items(snapshot, recipes, items, recipe, None) else {
        return false;
    };
    let Ok(groups) = recipes.resolved_components(recipe) else {
        return false;
    };
    let mut planned = BTreeMap::<ItemId, u32>::new();
    for group in groups {
        let selected = group.into_iter().find(|component| {
            let Some(definition) = items.get(&component.type_id) else {
                return false;
            };
            let count_by_charges = definition.count_by_charges();
            let available = snapshot
                .controlled_actor
                .inventory
                .iter()
                .filter(|item| {
                    item.type_id == component.type_id
                        && item.residual_energy_millijoules == 0
                        && !protected.contains(&item.id)
                        && item
                            .magazine_wells
                            .iter()
                            .all(|well| well.installed_magazine.is_none())
                        && item
                            .integral_magazines
                            .iter()
                            .all(|pocket| pocket.loaded_ammunition.is_none())
                })
                .fold(0_u32, |total, item| {
                    let units = if count_by_charges {
                        u32::try_from(item.charges).unwrap_or(0)
                    } else {
                        1
                    };
                    total.saturating_add(
                        units.saturating_sub(planned.get(&item.id).copied().unwrap_or(0)),
                    )
                });
            available >= component.count
        });
        let Some(selected) = selected else {
            return false;
        };
        let Some(definition) = items.get(&selected.type_id) else {
            return false;
        };
        let count_by_charges = definition.count_by_charges();
        let mut remaining = selected.count;
        for item in snapshot.controlled_actor.inventory.iter().filter(|item| {
            item.type_id == selected.type_id
                && item.residual_energy_millijoules == 0
                && !protected.contains(&item.id)
                && item
                    .magazine_wells
                    .iter()
                    .all(|well| well.installed_magazine.is_none())
                && item
                    .integral_magazines
                    .iter()
                    .all(|pocket| pocket.loaded_ammunition.is_none())
        }) {
            let units = if count_by_charges {
                u32::try_from(item.charges).unwrap_or(0)
            } else {
                1
            };
            let entry = planned.entry(item.id).or_default();
            let take = units.saturating_sub(*entry).min(remaining);
            *entry = entry.saturating_add(take);
            remaining -= take;
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return false;
        }
    }
    true
}

fn client_recipe_proficiencies_allow(
    actor: &cdda_protocol::ActorSnapshot,
    recipe: &cdda_content::RecipeDefinition,
) -> bool {
    recipe.proficiencies.iter().all(|proficiency| {
        !proficiency.required
            || actor
                .proficiencies
                .iter()
                .any(|state| state.proficiency_id == proficiency.proficiency_id && state.learned)
    })
}

fn client_skill_level(
    actor: &cdda_protocol::ActorSnapshot,
    skill_id: &str,
    theoretical: bool,
) -> u8 {
    actor
        .skills
        .iter()
        .find(|skill| skill.skill_id == skill_id)
        .map_or(0, |skill| {
            if theoretical {
                skill.theoretical_level
            } else {
                skill.practical_level
            }
        })
}

fn client_recipe_skills_allow(
    actor: &cdda_protocol::ActorSnapshot,
    items: &ItemRegistry,
    recipe: &cdda_content::RecipeDefinition,
) -> bool {
    if !client_recipe_knowledge_allows(actor, items, recipe) {
        return false;
    }
    let requirements = recipe
        .skills_required
        .iter()
        .map(|(skill_id, level)| (skill_id.as_str(), *level))
        .chain(
            (!recipe.skill_used.is_empty())
                .then_some((recipe.skill_used.as_str(), recipe.difficulty)),
        )
        .collect::<Vec<_>>();
    requirements
        .iter()
        .all(|(skill_id, level)| client_skill_level(actor, skill_id, false) >= *level)
        || requirements
            .iter()
            .all(|(skill_id, level)| client_skill_level(actor, skill_id, true) >= *level)
}

fn client_recipe_knowledge_allows(
    actor: &cdda_protocol::ActorSnapshot,
    items: &ItemRegistry,
    recipe: &cdda_content::RecipeDefinition,
) -> bool {
    let autolearned = recipe.autolearn
        && recipe
            .resolved_autolearn_skills()
            .iter()
            .all(|(skill_id, level)| client_skill_level(actor, skill_id, true) >= *level);
    (!recipe.never_learn
        && !recipe.learn_by_disassembly.is_empty()
        && actor.learned_recipes.binary_search(&recipe.id).is_ok())
        || autolearned
        || recipe.book_learn.iter().any(|(book_type_id, metadata)| {
            let Some(book) = items.get(book_type_id) else {
                return false;
            };
            // Item identification is not yet separate canonical state; every
            // replicated concrete type is therefore treated as identified.
            let required = if metadata.skill_level > 0 {
                metadata.skill_level
            } else {
                book.book_required_level.max(i32::from(recipe.difficulty))
            };
            i32::from(client_skill_level(actor, &recipe.skill_used, true)) >= required
                && actor
                    .inventory
                    .iter()
                    .any(|item| item.type_id == *book_type_id)
        })
}

fn client_craft_support_items(
    snapshot: &ReplicationSnapshotV1,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    recipe: &cdda_content::RecipeDefinition,
    excluded_item: Option<ItemId>,
) -> Option<BTreeSet<ItemId>> {
    let mut selected = BTreeSet::new();
    let inventory = &snapshot.controlled_actor.inventory;
    let mut required_presence = BTreeMap::<String, usize>::new();
    let mut required_charges = BTreeMap::<String, u64>::new();
    for group in recipes.resolved_tools(recipe).ok()? {
        let choice = group.into_iter().find(|tool| {
            if tool.requirement_list {
                return false;
            }
            if tool.count > 0 {
                let Ok(total) = u64::try_from(tool.count) else {
                    return false;
                };
                let required = total / 20 + total % 20;
                let already = required_charges.get(&tool.type_id).copied().unwrap_or(0);
                inventory
                    .iter()
                    .filter(|item| {
                        Some(item.id) != excluded_item
                            && item.type_id == tool.type_id
                            && item_tool_charges(item) > 0
                    })
                    .map(|item| u64::try_from(item_tool_charges(item)).unwrap_or(0))
                    .sum::<u64>()
                    >= already + required
            } else {
                let Ok(required) = usize::try_from(tool.count.unsigned_abs()) else {
                    return false;
                };
                let already = required_presence.get(&tool.type_id).copied().unwrap_or(0);
                inventory
                    .iter()
                    .filter(|item| Some(item.id) != excluded_item && item.type_id == tool.type_id)
                    .count()
                    >= already + required
            }
        })?;
        if choice.count > 0 {
            let total = u64::try_from(choice.count).ok()?;
            *required_charges.entry(choice.type_id).or_default() += total / 20 + total % 20;
        } else {
            *required_presence.entry(choice.type_id).or_default() +=
                usize::try_from(choice.count.unsigned_abs()).ok()?;
        }
    }
    for (type_id, amount) in required_presence {
        selected.extend(
            inventory
                .iter()
                .filter(|item| Some(item.id) != excluded_item && item.type_id == type_id)
                .take(amount)
                .map(|item| item.id),
        );
    }
    for (type_id, mut remaining) in required_charges {
        for item in inventory.iter().filter(|item| {
            Some(item.id) != excluded_item && item.type_id == type_id && item_tool_charges(item) > 0
        }) {
            selected.insert(item.id);
            remaining = remaining.saturating_sub(u64::try_from(item_tool_charges(item)).ok()?);
            if remaining == 0 {
                break;
            }
        }
        if remaining != 0 {
            return None;
        }
    }
    for group in recipes.resolved_qualities(recipe).ok()? {
        let choice = group.into_iter().find_map(|quality| {
            let ids = snapshot
                .controlled_actor
                .inventory
                .iter()
                .filter(|carried| {
                    Some(carried.id) != excluded_item
                        && items.get(&carried.type_id).is_some_and(|definition| {
                            let inherent = !definition.unsupported_fields.contains("qualities")
                                && definition
                                    .qualities
                                    .get(&quality.quality_id)
                                    .is_some_and(|provided| provided.level >= quality.level);
                            let charged =
                                !definition.unsupported_fields.contains("charged_qualities")
                                    && definition
                                        .charged_qualities
                                        .get(&quality.quality_id)
                                        .is_some_and(|provided| provided.level >= quality.level)
                                    && definition.charges_per_use > 0
                                    && item_tool_charges(carried) >= definition.charges_per_use;
                            inherent || charged
                        })
                })
                .take(usize::try_from(quality.amount).ok()?)
                .map(|item| item.id)
                .collect::<Vec<_>>();
            (ids.len() == usize::try_from(quality.amount).ok()?).then_some(ids)
        })?;
        selected.extend(choice);
    }
    Some(selected)
}

fn craft_duration_label(time_moves: u64) -> String {
    let seconds = time_moves / 100;
    if seconds >= 3_600 {
        format!("{}h {}m", seconds / 3_600, seconds % 3_600 / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

fn can_disassemble_item(
    snapshot: &ReplicationSnapshotV1,
    recipes: &RecipeRegistry,
    items: &ItemRegistry,
    ammunition: &AmmunitionRegistry,
    target: &ItemSnapshot,
) -> bool {
    if target.damage > cdda_protocol::MAX_ITEM_DAMAGE_LEVEL
        || target.residual_energy_millijoules != 0
    {
        return false;
    }
    if target
        .integral_magazines
        .iter()
        .any(|pocket| pocket.loaded_ammunition.is_some())
    {
        return false;
    }
    let Some(recipe) =
        recipes.strict_disassembly_recipe_for_result(&target.type_id, items, ammunition)
    else {
        return false;
    };
    if target.ranged_weapon.is_some()
        != items
            .get(&target.type_id)
            .is_some_and(|item| item.subtypes.contains("GUN"))
    {
        return false;
    }
    if items.get(&target.type_id).is_some_and(|item| {
        item.subtypes.contains("TOOL")
            && !item.subtypes.contains("GUN")
            && !item.tool_ammunition.is_empty()
            && item.default_charges() == 0
            && target.charges != 0
    }) {
        return false;
    }
    client_craft_support_items(snapshot, recipes, items, recipe, Some(target.id)).is_some()
}

fn item_menu_entries(
    action: ItemMenuAction,
    snapshot: &ReplicationSnapshotV1,
    content: Option<&ContentItems>,
    ammunition: Option<&ContentAmmunition>,
    recipes: Option<&ContentRecipes>,
) -> Vec<ItemMenuEntry> {
    let actor = &snapshot.controlled_actor;
    let items =
        match action {
            ItemMenuAction::PickUp => snapshot
                .ground_items
                .iter()
                .filter(|ground| ground.position == actor.position)
                .map(|ground| &ground.item)
                .collect::<Vec<_>>(),
            ItemMenuAction::Drop => actor
                .inventory
                .iter()
                .filter(|item| !actor.worn.contains(&item.id))
                .collect(),
            ItemMenuAction::Wield => actor
                .inventory
                .iter()
                .filter(|item| Some(item.id) != actor.wielded && !actor.worn.contains(&item.id))
                .collect(),
            ItemMenuAction::Wear => actor
                .inventory
                .iter()
                .filter(|item| {
                    Some(item.id) != actor.wielded
                        && !actor.worn.contains(&item.id)
                        && content.is_some_and(|content| {
                            content.0.get(&item.type_id).is_some_and(|definition| {
                                definition.subtypes.contains("ARMOR")
                                    && !definition.armor.is_empty()
                                    && !definition.unsupported_fields.contains("armor")
                                    && definition.armor.iter().all(|portion| {
                                        !portion.materials.is_empty()
                                            && portion.deferred_fields.is_empty()
                                    })
                            })
                        })
                })
                .collect(),
            ItemMenuAction::TakeOff => actor
                .worn
                .iter()
                .filter_map(|item_id| actor.inventory.iter().find(|item| item.id == *item_id))
                .collect(),
            ItemMenuAction::Reload => {
                let wielded = actor
                    .wielded
                    .and_then(|item_id| actor.inventory.iter().find(|item| item.id == item_id));
                actor
                    .inventory
                    .iter()
                    .filter(|item| {
                        wielded.is_some_and(|wielded| {
                            wielded.ranged_weapon.as_ref().is_some_and(|weapon| {
                                weapon.ammunition_remaining < weapon.ammunition_capacity
                                    && item.ammunition_type == weapon.ammunition_type
                                    && item.charges > 0
                                    && item.residual_energy_millijoules == 0
                            }) || wielded.magazine_wells.iter().any(|well| {
                                item.id != wielded.id
                                    && (item.magazine_capacity > 0
                                        || !item.integral_magazines.is_empty())
                                    && well
                                        .compatible_magazine_type_ids
                                        .binary_search(&item.type_id)
                                        .is_ok()
                            }) || wielded.integral_magazines.iter().any(|pocket| {
                                item.id != wielded.id
                                    && pocket.reloadable
                                    && item.ammunition_type == pocket.ammunition_type
                                    && (item.charges > 0 || item.residual_energy_millijoules > 0)
                                    && integral_pocket_has_free_charge_slot(pocket)
                                    && pocket.loaded_ammunition.as_deref().is_none_or(
                                        |ammunition| same_item_stack_state(ammunition, item),
                                    )
                            })
                        })
                    })
                    .collect()
            }
            ItemMenuAction::Consume => actor
                .inventory
                .iter()
                .filter(|item| !actor.worn.contains(&item.id) && !item.comestible_type.is_empty())
                .collect(),
            ItemMenuAction::Activate => actor
                .inventory
                .iter()
                .filter(|item| {
                    let definition = content.and_then(|content| content.0.get(&item.type_id));
                    let healing = definition.filter(|definition| {
                        definition.healing_actions.len() == 1
                            && !definition.has_unsupported_use_actions
                            && definition.transform_actions.is_empty()
                            && definition.eoc_actions.is_empty()
                            && definition.healing_actions[0].deferred_fields.is_empty()
                            && u16::try_from(definition.charges_per_use).is_ok()
                    });
                    if let Some(definition) = healing {
                        !actor.worn.contains(&item.id)
                            && item.charges >= definition.charges_per_use.max(1)
                    } else if let Some(action) = definition.and_then(|definition| {
                        let [action] = definition.eoc_actions.as_slice() else {
                            return None;
                        };
                        (!definition.has_unsupported_use_actions
                            && definition.transform_actions.is_empty()
                            && definition.healing_actions.is_empty()
                            && definition.comestible_type.is_empty()
                            && action.deferred_fields.is_empty()
                            && !action.eoc_ids.is_empty())
                        .then_some(action)
                    }) {
                        (!action.need_worn || actor.worn.contains(&item.id))
                            && (!action.need_wielding || actor.wielded == Some(item.id))
                            && (!action.consume || item.charges > 0)
                    } else if let Some(required_charges) = definition.and_then(|definition| {
                        let [action] = definition.transform_actions.as_slice() else {
                            return None;
                        };
                        if definition.has_non_transform_use_actions
                            || definition.has_unsupported_use_actions
                            || definition.has_unsupported_transform_action_fields
                            || !definition.healing_actions.is_empty()
                            || !definition.eoc_actions.is_empty()
                            || definition.subtypes.contains("ARMOR")
                            || action.target == definition.id
                        {
                            return None;
                        }
                        let consumed = if definition.subtypes.contains("TOOL") {
                            definition.charges_per_use.checked_mul(action.ammo_scale)?
                        } else {
                            0
                        };
                        Some(action.need_charges.max(consumed))
                    }) {
                        item_tool_charges(item) >= required_charges
                    } else {
                        item.powered_tool.as_ref().is_some_and(|powered| {
                            powered.active
                                || item
                                    .magazine_wells
                                    .iter()
                                    .find(|well| well.pocket_index == powered.power_pocket_index)
                                    .and_then(|well| well.installed_magazine.as_deref())
                                    .is_some_and(|magazine| {
                                        item_stored_ammunition_charges(magazine)
                                            >= i32::from(powered.activation_charges)
                                    })
                        })
                    }
                })
                .collect(),
            ItemMenuAction::Read => actor
                .inventory
                .iter()
                .filter(|item| {
                    snapshot.detail_vision_available
                        && content.is_some_and(|content| {
                            content.0.get(&item.type_id).is_some_and(|definition| {
                                let theory = actor
                                    .skills
                                    .iter()
                                    .find(|skill| skill.skill_id == definition.book_skill)
                                    .map_or(0, |skill| skill.theoretical_level);
                                definition.subtypes.contains("BOOK")
                                    && !definition.book_skill.is_empty()
                                    && definition.book_time_moves > 0
                                    && definition.book_required_level >= 0
                                    && definition.book_max_level > definition.book_required_level
                                    && theory
                                        >= u8::try_from(definition.book_required_level)
                                            .unwrap_or(u8::MAX)
                                    && theory
                                        < u8::try_from(definition.book_max_level)
                                            .unwrap_or_default()
                            })
                        })
                })
                .collect(),
            ItemMenuAction::Disassemble => actor
                .inventory
                .iter()
                .filter(|item| {
                    !actor.worn.contains(&item.id)
                        && snapshot.detail_vision_available
                        && item.damage <= cdda_protocol::MAX_ITEM_DAMAGE_LEVEL
                        && content.is_some_and(|content| {
                            ammunition.is_some_and(|ammunition| {
                                recipes.is_some_and(|recipes| {
                                    can_disassemble_item(
                                        snapshot,
                                        &recipes.0,
                                        &content.0,
                                        &ammunition.0,
                                        item,
                                    )
                                })
                            })
                        })
                })
                .collect(),
        };
    let mut entries = items
        .into_iter()
        .map(|item| ItemMenuEntry {
            item_id: item.id,
            label: item_menu_label(item, content),
            vehicle_cargo: None,
        })
        .collect::<Vec<_>>();
    if action == ItemMenuAction::PickUp {
        entries.extend(snapshot.vehicles.iter().flat_map(|vehicle| {
            vehicle.tiles.iter().flat_map(move |tile| {
                tile.cargo.iter().filter_map(move |item| {
                    tile.cargo_prototype_part_index
                        .map(|part_index| ItemMenuEntry {
                            item_id: item.id,
                            label: format!("{} [vehicle]", item_menu_label(item, content)),
                            vehicle_cargo: Some((vehicle.id, part_index)),
                        })
                })
            })
        }));
    }
    entries.sort_by_key(|entry| (entry.item_id, entry.vehicle_cargo));
    entries
}

fn item_menu_label(item: &ItemSnapshot, content: Option<&ContentItems>) -> String {
    let name = item
        .variant
        .as_ref()
        .map(|variant| variant.name.as_str())
        .unwrap_or_else(|| {
            content
                .and_then(|content| content.0.get(&item.type_id))
                .map_or(item.type_id.as_str(), |definition| definition.name.as_str())
        });
    let name = if !item.fitted
        && item
            .containment
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("VARSIZE"))
            .is_ok()
    {
        format!("{name} (poor fit)")
    } else {
        name.to_owned()
    };
    let name = match &item.snippet {
        Some(snippet) => format!("{name} — {}", snippet.text),
        None => name,
    };
    let name = match item.variables.get("description") {
        Some(cdda_protocol::ItemVariableValueV1::String(description)) => {
            format!("{name} — {description}")
        }
        _ => name,
    };
    let charges = if !item.ammunition_containers.is_empty() {
        Some(format!(
            " [{}]",
            item.ammunition_containers
                .iter()
                .map(|pocket| {
                    if let Some(state) = &pocket.spawn_state {
                        let kind = match state.rules.kind {
                            cdda_protocol::SpawnPocketKindV1::Container => "items",
                            cdda_protocol::SpawnPocketKindV1::EFileStorage => "e-files",
                        };
                        let sealed = if state.sealed { ", sealed" } else { "" };
                        let collapsed = if state.contents_collapsed {
                            ", collapsed"
                        } else {
                            ""
                        };
                        return format!(
                            "p{} {} {}{}{}",
                            pocket.pocket_index,
                            pocket.contents.len(),
                            kind,
                            sealed,
                            collapsed
                        );
                    }
                    let stored = pocket
                        .contents
                        .iter()
                        .fold(0_i32, |total, item| total.saturating_add(item.charges));
                    if let Some(active) = pocket.contents.first() {
                        let capacity = pocket
                            .capacities
                            .iter()
                            .find(|capacity| capacity.ammunition_type == active.ammunition_type)
                            .map(|capacity| capacity.capacity)
                            .unwrap_or_default();
                        format!(
                            "p{} {} {stored}/{capacity}",
                            pocket.pocket_index, active.ammunition_type
                        )
                    } else {
                        let capacities = pocket
                            .capacities
                            .iter()
                            .map(|capacity| {
                                format!("{}:{}", capacity.ammunition_type, capacity.capacity)
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        format!("p{} empty 0 [{capacities}]", pocket.pocket_index)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        ))
    } else if !item.integral_magazines.is_empty() {
        Some(format!(
            " [{}]",
            item.integral_magazines
                .iter()
                .map(|pocket| format!(
                    "p{} {}/{} {}{}",
                    pocket.pocket_index,
                    pocket
                        .loaded_ammunition
                        .as_deref()
                        .map(|ammunition| ammunition.charges)
                        .unwrap_or(0),
                    pocket.capacity,
                    pocket.ammunition_type,
                    residual_power_suffix(pocket.residual_energy_millijoules)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    } else if item.magazine_capacity > 0 {
        Some(format!(
            " {}/{}{}",
            item.charges,
            item.magazine_capacity,
            residual_power_suffix(item.residual_energy_millijoules)
        ))
    } else if let Some(installed) = item.powered_tool.as_ref().and_then(|powered| {
        item.magazine_wells
            .iter()
            .find(|well| well.pocket_index == powered.power_pocket_index)
            .and_then(|well| well.installed_magazine.as_deref())
    }) {
        Some(format!(
            " [power {}{}]",
            item_stored_ammunition_charges(installed),
            residual_power_suffix(item_residual_power_millijoules(installed))
        ))
    } else {
        (item.charges > 1).then(|| format!(" x{}", item.charges))
    };
    let temperature = item_temperature_suffix(item);
    format!(
        "{}{}{} [{}]",
        name,
        charges.as_deref().unwrap_or_default(),
        temperature,
        item.id
    )
}

fn item_temperature_suffix(item: &ItemSnapshot) -> &'static str {
    match item.temperature {
        Some(state)
            if state.temperature_millikelvin
                == cdda_protocol::ITEM_TEMPERATURE_UNPROCESSED_MILLIKELVIN =>
        {
            " [temperature pending]"
        }
        Some(_) => " [20.0 °C]",
        None => "",
    }
}

fn residual_power_suffix(residual_energy_millijoules: u32) -> String {
    if residual_energy_millijoules == 0 {
        String::new()
    } else {
        format!(" + {residual_energy_millijoules} mJ")
    }
}

fn item_tool_charges(item: &ItemSnapshot) -> i32 {
    if item.magazine_wells.is_empty() {
        return item_stored_ammunition_charges(item);
    }
    item.magazine_wells
        .iter()
        .filter_map(|well| well.installed_magazine.as_deref())
        .fold(0, |total, magazine| {
            total.saturating_add(item_stored_ammunition_charges(magazine))
        })
}

fn item_stored_ammunition_charges(item: &ItemSnapshot) -> i32 {
    if item.integral_magazines.is_empty() {
        item.charges
    } else {
        item.integral_magazines
            .iter()
            .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
            .fold(0_i32, |total, ammunition| {
                total.saturating_add(ammunition.charges)
            })
    }
}

fn same_item_stack_state(left: &ItemSnapshot, right: &ItemSnapshot) -> bool {
    left.type_id == right.type_id
        && left.damage == right.damage
        && left.raw_damage == right.raw_damage
        && left.fitted == right.fitted
        && left.variant == right.variant
        && left.snippet == right.snippet
        && left.variables == right.variables
        && left.melee_damage_milli == right.melee_damage_milli
        && left.calories == right.calories
        && left.quench == right.quench
        && left.comestible_type == right.comestible_type
        && left.temperature == right.temperature
        && left.ammunition_type == right.ammunition_type
        && left.ranged_weapon == right.ranged_weapon
        && left.component_provenance == right.component_provenance
        && left.magazine_capacity == right.magazine_capacity
        && left.integral_magazines == right.integral_magazines
        && left.magazine_wells == right.magazine_wells
        && left.ammunition_containers == right.ammunition_containers
        && left.residual_energy_millijoules == right.residual_energy_millijoules
        && left.powered_tool == right.powered_tool
        && left.creature_corpse == right.creature_corpse
        && left.containment == right.containment
}

fn item_residual_power_millijoules(item: &ItemSnapshot) -> u32 {
    if item.integral_magazines.is_empty() {
        item.residual_energy_millijoules
    } else {
        item.integral_magazines.iter().fold(0_u32, |total, pocket| {
            total.saturating_add(pocket.residual_energy_millijoules)
        })
    }
}

fn first_pocket_item_removal(snapshot: &ReplicationSnapshotV1) -> Option<ClientAction> {
    let actor = &snapshot.controlled_actor;
    actor
        .inventory
        .iter()
        .flat_map(|owner| {
            let integral = owner.integral_magazines.iter().filter_map(|pocket| {
                pocket
                    .unloadable
                    .then_some(pocket.loaded_ammunition.as_deref())
                    .flatten()
                    .map(|contained| {
                        (
                            owner.id,
                            pocket.pocket_index,
                            contained.id,
                            ClientAction::RemovePocketItem {
                                owner_item: owner.id,
                                pocket_index: pocket.pocket_index,
                                contained_item: contained.id,
                            },
                        )
                    })
            });
            let wells = owner.magazine_wells.iter().filter_map(|well| {
                let power_locked = owner.powered_tool.as_ref().is_some_and(|powered| {
                    powered.active && powered.power_pocket_index == well.pocket_index
                });
                (well.unloadable && !power_locked)
                    .then_some(well.installed_magazine.as_deref())
                    .flatten()
                    .map(|contained| {
                        (
                            owner.id,
                            well.pocket_index,
                            contained.id,
                            ClientAction::RemovePocketItem {
                                owner_item: owner.id,
                                pocket_index: well.pocket_index,
                                contained_item: contained.id,
                            },
                        )
                    })
            });
            let containers = owner
                .ammunition_containers
                .iter()
                .filter(|pocket| pocket.unloadable)
                .flat_map(|pocket| {
                    pocket.contents.iter().map(|contained| {
                        (
                            owner.id,
                            pocket.pocket_index,
                            contained.id,
                            ClientAction::RemovePocketItem {
                                owner_item: owner.id,
                                pocket_index: pocket.pocket_index,
                                contained_item: contained.id,
                            },
                        )
                    })
                });
            integral.chain(wells).chain(containers)
        })
        .min_by_key(|(owner, pocket, contained, _)| (*owner, *pocket, *contained))
        .map(|(_, _, _, action)| action)
}

fn first_pocket_item_insertion(snapshot: &ReplicationSnapshotV1) -> Option<ClientAction> {
    let actor = &snapshot.controlled_actor;
    actor
        .inventory
        .iter()
        .flat_map(|owner| {
            owner
                .ammunition_containers
                .iter()
                .filter(|pocket| pocket.reloadable)
                .flat_map(move |pocket| {
                    actor.inventory.iter().filter_map(move |source| {
                        if source.id == owner.id
                            || source.charges <= 0
                            || source.ammunition_type.is_empty()
                            || !source.comestible_type.is_empty()
                            || source.ranged_weapon.is_some()
                            || source.component_provenance.is_some()
                            || source.magazine_capacity != 0
                            || !source.integral_magazines.is_empty()
                            || !source.magazine_wells.is_empty()
                            || !source.ammunition_containers.is_empty()
                            || source.residual_energy_millijoules != 0
                            || source.powered_tool.is_some()
                            || source.creature_corpse.is_some()
                        {
                            return None;
                        }
                        let capacity = pocket
                            .capacities
                            .iter()
                            .find(|capacity| capacity.ammunition_type == source.ammunition_type)?
                            .capacity;
                        if pocket.contents.first().is_some_and(|contained| {
                            contained.ammunition_type != source.ammunition_type
                        }) {
                            return None;
                        }
                        let occupied = pocket.contents.iter().try_fold(0_u32, |total, item| {
                            u32::try_from(item.charges)
                                .ok()
                                .and_then(|charges| total.checked_add(charges))
                        })?;
                        (occupied < capacity).then_some((
                            owner.id,
                            pocket.pocket_index,
                            source.id,
                            ClientAction::InsertPocketItem {
                                owner_item: owner.id,
                                pocket_index: pocket.pocket_index,
                                source_item: source.id,
                            },
                        ))
                    })
                })
        })
        .min_by_key(|(owner, pocket, source, _)| (*owner, *pocket, *source))
        .map(|(_, _, _, action)| action)
}

fn integral_pocket_has_free_charge_slot(pocket: &IntegralMagazinePocketSnapshotV1) -> bool {
    let loaded = pocket
        .loaded_ammunition
        .as_deref()
        .map(|ammunition| ammunition.charges)
        .unwrap_or(0);
    u32::try_from(loaded).is_ok_and(|loaded| {
        loaded.saturating_add(u32::from(pocket.residual_energy_millijoules > 0)) < pocket.capacity
    })
}

fn client_action_for_item_menu(
    action: ItemMenuAction,
    entry: &ItemMenuEntry,
    snapshot: Option<&ReplicationSnapshotV1>,
) -> Option<ClientAction> {
    let item_id = entry.item_id;
    Some(match action {
        ItemMenuAction::PickUp => match entry.vehicle_cargo {
            Some((vehicle_id, prototype_part_index)) => ClientAction::TakeVehicleCargo {
                vehicle_id,
                prototype_part_index,
                item_id,
            },
            None => ClientAction::PickUp { item_id },
        },
        ItemMenuAction::Drop => snapshot
            .and_then(|snapshot| {
                snapshot
                    .vehicles
                    .iter()
                    .flat_map(|vehicle| {
                        vehicle.tiles.iter().filter_map(move |tile| {
                            tile.cargo_prototype_part_index
                                .map(|part_index| (vehicle.id, part_index, tile.position))
                        })
                    })
                    .min_by_key(|(vehicle_id, part_index, position)| {
                        (*vehicle_id, *part_index, *position)
                    })
            })
            .map_or(ClientAction::Drop { item_id }, |(vehicle_id, part, _)| {
                ClientAction::StoreVehicleCargo {
                    vehicle_id,
                    prototype_part_index: part,
                    item_id,
                }
            }),
        ItemMenuAction::Wield => ClientAction::Wield { item_id },
        ItemMenuAction::Wear => ClientAction::Wear { item_id },
        ItemMenuAction::TakeOff => ClientAction::TakeOff { item_id },
        ItemMenuAction::Reload => {
            let actor = &snapshot?.controlled_actor;
            let ammunition = actor.inventory.iter().find(|item| item.id == item_id)?;
            let wielded = actor
                .wielded
                .and_then(|wielded| actor.inventory.iter().find(|item| item.id == wielded))?;
            let target_pocket_index = if wielded.ranged_weapon.is_some() {
                None
            } else if let Some(pocket) = wielded.integral_magazines.iter().find(|pocket| {
                pocket.reloadable
                    && ammunition.ammunition_type == pocket.ammunition_type
                    && integral_pocket_has_free_charge_slot(pocket)
                    && pocket
                        .loaded_ammunition
                        .as_deref()
                        .is_none_or(|loaded| same_item_stack_state(loaded, ammunition))
            }) {
                Some(pocket.pocket_index)
            } else {
                Some(
                    wielded
                        .magazine_wells
                        .iter()
                        .find(|well| {
                            well.compatible_magazine_type_ids
                                .binary_search(&ammunition.type_id)
                                .is_ok()
                        })?
                        .pocket_index,
                )
            };
            ClientAction::Reload {
                ammunition_item: item_id,
                target_pocket_index,
            }
        }
        ItemMenuAction::Consume => ClientAction::Consume { item_id },
        ItemMenuAction::Activate => ClientAction::Activate { item_id },
        ItemMenuAction::Read => ClientAction::Read {
            item_id,
            book_type_id: snapshot?
                .controlled_actor
                .inventory
                .iter()
                .find(|item| item.id == item_id)?
                .type_id
                .clone(),
        },
        ItemMenuAction::Disassemble => ClientAction::Disassemble {
            item_id,
            item_type_id: snapshot?
                .controlled_actor
                .inventory
                .iter()
                .find(|item| item.id == item_id)?
                .type_id
                .clone(),
        },
    })
}

#[allow(clippy::too_many_arguments)] // Bevy systems declare disjoint resource access as parameters.
fn handle_target_menu(
    keys: Res<ButtonInput<KeyCode>>,
    composer: Res<ChatComposer>,
    interaction_menu: Res<InteractionMenu>,
    item_menu: Res<ItemMenu>,
    terrain_menu: Res<TerrainMenu>,
    craft_menu: Res<CraftMenu>,
    mut menu: ResMut<TargetMenu>,
    game: Option<ResMut<GameClient>>,
    content_monsters: Option<Res<ContentMonsters>>,
) {
    if composer.active
        || interaction_menu.is_active()
        || item_menu.action.is_some()
        || terrain_menu.action.is_some()
        || craft_menu.open
    {
        return;
    }
    let Some(mut game) = game else {
        return;
    };
    if let Some(action) = menu.action {
        if keys.just_pressed(KeyCode::Escape) {
            menu.clear();
        } else if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
            menu.select_previous();
        } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
            menu.select_next();
        } else if keys.just_pressed(KeyCode::Enter)
            && let Some(entry) = menu.entries.get(menu.selected)
        {
            let _send_result = game
                .actions
                .try_send(client_action_for_target_menu(action, entry.target));
            menu.clear();
        }
        return;
    }
    let action = if keys.just_pressed(KeyCode::KeyF) {
        Some(TargetMenuAction::Melee)
    } else if keys.just_pressed(KeyCode::KeyT) {
        Some(TargetMenuAction::Shoot)
    } else {
        None
    };
    let Some(action) = action else {
        return;
    };
    let Some(snapshot) = &game.snapshot else {
        return;
    };
    let entries = target_menu_entries(action, snapshot, content_monsters.as_deref());
    match entries.as_slice() {
        [] => game.notice = format!("No valid target is available to {}.", action.verb()),
        [entry] => {
            let _send_result = game
                .actions
                .try_send(client_action_for_target_menu(action, entry.target));
        }
        _ => {
            menu.action = Some(action);
            menu.entries = entries;
            menu.selected = 0;
        }
    }
}

impl TargetMenuAction {
    const fn title(self) -> &'static str {
        match self {
            Self::Melee => "Melee target",
            Self::Shoot => "Ranged target",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Melee => "attack",
            Self::Shoot => "shoot",
        }
    }
}

impl TargetMenu {
    fn clear(&mut self) {
        self.action = None;
        self.entries.clear();
        self.selected = 0;
    }

    fn select_previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or(self.entries.len() - 1);
    }

    fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
    }

    fn display(&self) -> String {
        let Some(action) = self.action else {
            return String::new();
        };
        const VISIBLE_ENTRIES: usize = 9;
        let half = VISIBLE_ENTRIES / 2;
        let start = self
            .selected
            .saturating_sub(half)
            .min(self.entries.len().saturating_sub(VISIBLE_ENTRIES));
        let end = start
            .saturating_add(VISIBLE_ENTRIES)
            .min(self.entries.len());
        let choices = self.entries[start..end]
            .iter()
            .enumerate()
            .map(|(offset, entry)| {
                let index = start + offset;
                format!(
                    "{} {}",
                    if index == self.selected { ">" } else { " " },
                    entry.label
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\n{} ({}/{}) — arrows or J/K select, Enter confirms, Escape cancels\n{}",
            action.title(),
            self.selected + 1,
            self.entries.len(),
            choices
        )
    }
}

fn target_menu_entries(
    action: TargetMenuAction,
    snapshot: &ReplicationSnapshotV1,
    content: Option<&ContentMonsters>,
) -> Vec<TargetMenuEntry> {
    let actor = &snapshot.controlled_actor;
    let maximum_distance = match action {
        TargetMenuAction::Melee => 1,
        TargetMenuAction::Shoot => {
            let Some(range) = actor
                .wielded
                .and_then(|item_id| actor.inventory.iter().find(|item| item.id == item_id))
                .and_then(|item| item.ranged_weapon.as_ref())
                .filter(|weapon| weapon.ammunition_remaining > 0)
                .map(|weapon| u32::from(weapon.range))
            else {
                return Vec::new();
            };
            range
        }
    };
    let distance = |position: cdda_protocol::WorldPosition| {
        position
            .x
            .abs_diff(actor.position.x)
            .max(position.y.abs_diff(actor.position.y))
            .max(position.z.abs_diff(actor.position.z))
    };
    let in_range = |position: cdda_protocol::WorldPosition| {
        position != actor.position
            && distance(position) <= maximum_distance
            && (action == TargetMenuAction::Shoot || position.z == actor.position.z)
    };
    let mut entries = snapshot
        .creatures
        .iter()
        .filter(|target| target.hp > 0 && in_range(target.position))
        .map(|target| {
            let name = content
                .and_then(|content| content.0.get(&target.type_id))
                .map_or(target.type_id.as_str(), |definition| {
                    definition.name.as_str()
                });
            (
                distance(target.position),
                0_u8,
                target.id.as_u128(),
                TargetMenuEntry {
                    target: TargetChoice::Creature(target.id),
                    label: format!(
                        "{} — {} HP, distance {} [{}]",
                        name,
                        target.hp,
                        distance(target.position),
                        target.id
                    ),
                },
            )
        })
        .chain(
            snapshot
                .visible_actors
                .iter()
                .filter(|target| target.hp > 0 && in_range(target.position))
                .map(|target| {
                    (
                        distance(target.position),
                        1_u8,
                        target.id.as_u128(),
                        TargetMenuEntry {
                            target: TargetChoice::Actor(target.id),
                            label: format!(
                                "survivor — {} HP, distance {} [{}]",
                                target.hp,
                                distance(target.position),
                                target.id
                            ),
                        },
                    )
                }),
        )
        .collect::<Vec<_>>();
    entries.sort_by_key(|(distance, kind, stable_id, _)| (*distance, *kind, *stable_id));
    entries.into_iter().map(|(_, _, _, entry)| entry).collect()
}

const fn client_action_for_target_menu(
    action: TargetMenuAction,
    target: TargetChoice,
) -> ClientAction {
    match (action, target) {
        (TargetMenuAction::Melee, TargetChoice::Actor(target)) => ClientAction::Attack { target },
        (TargetMenuAction::Melee, TargetChoice::Creature(target)) => {
            ClientAction::AttackCreature { target }
        }
        (TargetMenuAction::Shoot, TargetChoice::Actor(target)) => {
            ClientAction::ShootActor { target }
        }
        (TargetMenuAction::Shoot, TargetChoice::Creature(target)) => {
            ClientAction::ShootCreature { target }
        }
    }
}

#[allow(clippy::too_many_arguments)] // Bevy systems declare disjoint resource access as parameters.
fn handle_terrain_menu(
    keys: Res<ButtonInput<KeyCode>>,
    composer: Res<ChatComposer>,
    interaction_menu: Res<InteractionMenu>,
    item_menu: Res<ItemMenu>,
    target_menu: Res<TargetMenu>,
    craft_menu: Res<CraftMenu>,
    mut menu: ResMut<TerrainMenu>,
    game: Option<ResMut<GameClient>>,
    content_terrain: Option<Res<ContentTerrain>>,
    content_furniture: Option<Res<ContentFurniture>>,
) {
    if composer.active
        || interaction_menu.is_active()
        || item_menu.action.is_some()
        || target_menu.action.is_some()
        || craft_menu.open
    {
        return;
    }
    let Some(mut game) = game else {
        return;
    };
    if let Some(action) = menu.action {
        if keys.just_pressed(KeyCode::Escape) {
            menu.clear();
        } else if keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyK) {
            menu.select_previous();
        } else if keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyJ) {
            menu.select_next();
        } else if keys.just_pressed(KeyCode::Enter)
            && let Some(entry) = menu.entries.get(menu.selected)
        {
            let _send_result = game
                .actions
                .try_send(client_action_for_terrain_menu(action, entry.dx, entry.dy));
            menu.clear();
        }
        return;
    }
    let action = if keys.just_pressed(KeyCode::KeyO) {
        Some(TerrainMenuAction::Open)
    } else if keys.just_pressed(KeyCode::KeyL) {
        Some(TerrainMenuAction::Close)
    } else if keys.just_pressed(KeyCode::KeyH) {
        Some(TerrainMenuAction::Smash)
    } else {
        None
    };
    let Some(action) = action else {
        return;
    };
    let Some(snapshot) = &game.snapshot else {
        return;
    };
    let entries = terrain_menu_entries(
        action,
        snapshot,
        content_terrain.as_deref(),
        content_furniture.as_deref(),
    );
    match entries.as_slice() {
        [] => game.notice = format!("There is nothing adjacent to {}.", action.verb()),
        [entry] => {
            let _send_result = game
                .actions
                .try_send(client_action_for_terrain_menu(action, entry.dx, entry.dy));
        }
        _ => {
            menu.action = Some(action);
            menu.entries = entries;
            menu.selected = 0;
        }
    }
}

impl TerrainMenuAction {
    const fn title(self) -> &'static str {
        match self {
            Self::Open => "Open adjacent terrain",
            Self::Close => "Close adjacent terrain",
            Self::Smash => "Smash adjacent structure",
        }
    }

    const fn verb(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Close => "close",
            Self::Smash => "smash",
        }
    }
}

impl TerrainMenu {
    fn clear(&mut self) {
        self.action = None;
        self.entries.clear();
        self.selected = 0;
    }

    fn select_previous(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .checked_sub(1)
            .unwrap_or(self.entries.len() - 1);
    }

    fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
    }

    fn display(&self) -> String {
        let Some(action) = self.action else {
            return String::new();
        };
        let choices = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                format!(
                    "{} {}",
                    if index == self.selected { ">" } else { " " },
                    entry.label
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "\n\n{} ({}/{}) — arrows or J/K select, Enter confirms, Escape cancels\n{}",
            action.title(),
            self.selected + 1,
            self.entries.len(),
            choices
        )
    }
}

fn terrain_menu_entries(
    action: TerrainMenuAction,
    snapshot: &ReplicationSnapshotV1,
    terrain_content: Option<&ContentTerrain>,
    furniture_content: Option<&ContentFurniture>,
) -> Vec<TerrainMenuEntry> {
    let position = snapshot.controlled_actor.position;
    [
        (0, -1, "north"),
        (1, -1, "northeast"),
        (1, 0, "east"),
        (1, 1, "southeast"),
        (0, 1, "south"),
        (-1, 1, "southwest"),
        (-1, 0, "west"),
        (-1, -1, "northwest"),
    ]
    .into_iter()
    .filter_map(|(dx, dy, direction)| {
        if action != TerrainMenuAction::Smash && dx != 0 && dy != 0 {
            return None;
        }
        let target = position.checked_offset(dx, dy, 0)?;
        let (chunk_coord, local) = target.chunk_and_local();
        let tile = snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coord == chunk_coord)?
            .tiles
            .get(usize::from(local.y) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(local.x))?
            .as_ref()
            .filter(|tile| tile.currently_visible)?;
        let terrain_name = |terrain_id: &str| {
            terrain_content
                .and_then(|content| content.0.get(terrain_id))
                .map_or_else(
                    || terrain_id.to_owned(),
                    |definition| definition.name.clone(),
                )
        };
        let furniture_name = |furniture_id: &str| {
            furniture_content
                .and_then(|content| content.0.get(furniture_id))
                .map_or_else(
                    || furniture_id.to_owned(),
                    |definition| definition.name.clone(),
                )
        };
        let interaction = match action {
            TerrainMenuAction::Open => {
                let result_id = &tile.terrain.open;
                if result_id.is_empty() {
                    return None;
                }
                format!(
                    "{} → {}",
                    terrain_name(&tile.terrain.terrain_id),
                    terrain_name(result_id)
                )
            }
            TerrainMenuAction::Close => {
                let result_id = &tile.terrain.close;
                if result_id.is_empty() {
                    return None;
                }
                format!(
                    "{} → {}",
                    terrain_name(&tile.terrain.terrain_id),
                    terrain_name(result_id)
                )
            }
            TerrainMenuAction::Smash => match tile.bash_target? {
                BashTargetKindV1::Terrain => {
                    format!("{} (terrain)", terrain_name(&tile.terrain.terrain_id))
                }
                BashTargetKindV1::Furniture => {
                    let furniture = tile.furniture.as_ref()?;
                    format!("{} (furniture)", furniture_name(&furniture.furniture_id))
                }
            },
        };
        Some(TerrainMenuEntry {
            dx,
            dy,
            label: format!(
                "{} — {} at ({}, {}, {})",
                direction, interaction, target.x, target.y, target.z
            ),
        })
    })
    .collect()
}

const fn client_action_for_terrain_menu(action: TerrainMenuAction, dx: i8, dy: i8) -> ClientAction {
    match action {
        TerrainMenuAction::Open => ClientAction::Open { dx, dy },
        TerrainMenuAction::Close => ClientAction::Close { dx, dy },
        TerrainMenuAction::Smash => ClientAction::Smash { dx, dy },
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy systems receive independent input and modal resources as typed parameters"
)]
fn handle_movement_input(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    composer: Res<ChatComposer>,
    character_menu: Res<CharacterMenu>,
    interaction_menu: Res<InteractionMenu>,
    menu: Res<ItemMenu>,
    craft_menu: Res<CraftMenu>,
    target_menu: Res<TargetMenu>,
    terrain_menu: Res<TerrainMenu>,
    mut held_sender: ResMut<HeldMovementSender>,
    game: Option<Res<GameClient>>,
) {
    held_sender.since_send = held_sender.since_send.saturating_add(time.delta());
    let direction = if game
        .as_ref()
        .is_none_or(|game| game.controlled_actor.is_none())
        || character_menu.is_active()
        || interaction_menu.is_active()
        || composer.active
        || menu.action.is_some()
        || craft_menu.open
        || game.as_ref().is_some_and(|game| {
            game.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.controlled_actor.craft_activity.is_some()
                    || snapshot.controlled_actor.read_activity.is_some()
                    || snapshot.controlled_actor.disassembly_activity.is_some()
                    || snapshot.controlled_actor.construction_activity.is_some()
            })
        })
        || target_menu.action.is_some()
        || terrain_menu.action.is_some()
    {
        None
    } else {
        current_held_direction(&keys)
    };
    let changed = direction != held_sender.last;
    let refresh_due = direction.is_some() && held_sender.since_send >= Duration::from_millis(100);
    if (changed || refresh_due)
        && let Some(game) = &game
        && game
            .actions
            .try_send(ClientAction::HeldMovement { direction })
            .is_ok()
    {
        held_sender.last = direction;
        held_sender.since_send = Duration::ZERO;
    }
    if character_menu.is_active()
        || interaction_menu.is_active()
        || composer.active
        || menu.action.is_some()
        || craft_menu.open
        || target_menu.action.is_some()
        || terrain_menu.action.is_some()
    {
        return;
    }
    let Some(game) = game else {
        return;
    };
    let Some(controlled_actor) = game.controlled_actor else {
        return;
    };
    let Some(snapshot) = &game.snapshot else {
        return;
    };
    let actor = &snapshot.controlled_actor;
    if actor.id != controlled_actor {
        return;
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        let action = if actor.sleeping {
            ClientAction::Wake
        } else {
            ClientAction::Sleep
        };
        let _send_result = game.actions.try_send(action);
    } else if keys.just_pressed(KeyCode::Period) || keys.just_pressed(KeyCode::Numpad5) {
        let _send_result = game.actions.try_send(ClientAction::Wait);
    } else if keys.just_pressed(KeyCode::KeyR) && actor.wielded.is_some() {
        let _send_result = game.actions.try_send(ClientAction::Unwield);
    } else if keys.just_pressed(KeyCode::KeyO) {
        if let Some((vehicle, tile, prototype_part_index)) = snapshot
            .vehicles
            .iter()
            .flat_map(|vehicle| {
                vehicle.tiles.iter().filter_map(move |tile| {
                    tile.openable_prototype_part_index
                        .map(|part_index| (vehicle, tile, part_index))
                })
            })
            .filter(|(_, tile, _)| {
                tile.position.z == actor.position.z
                    && tile.position.x.abs_diff(actor.position.x) <= 1
                    && tile.position.y.abs_diff(actor.position.y) <= 1
            })
            .min_by_key(|(vehicle, tile, part_index)| {
                (
                    actor.position.x.abs_diff(tile.position.x)
                        + actor.position.y.abs_diff(tile.position.y),
                    vehicle.id,
                    *part_index,
                )
            })
        {
            let _send_result = game.actions.try_send(ClientAction::SetVehiclePartOpen {
                vehicle_id: vehicle.id,
                prototype_part_index,
                open: !tile.open,
            });
        }
    } else if keys.just_pressed(KeyCode::KeyK) {
        if let Some((vehicle, tile)) = snapshot.vehicles.iter().find_map(|vehicle| {
            vehicle
                .tiles
                .iter()
                .find(|tile| tile.passenger == Some(actor.id))
                .map(|tile| (vehicle, tile))
        }) {
            if let (Some(prototype_part_index), Some(direction)) = (
                tile.boardable_prototype_part_index,
                client_unboard_direction(snapshot, actor.position),
            ) {
                let _send_result = game.actions.try_send(ClientAction::UnboardVehicle {
                    vehicle_id: vehicle.id,
                    prototype_part_index,
                    dx: direction.dx,
                    dy: direction.dy,
                });
            }
            return;
        }
        let adjacent = snapshot
            .npcs
            .iter()
            .filter(|npc| {
                npc.position.z == actor.position.z
                    && npc.position.x.abs_diff(actor.position.x) <= 1
                    && npc.position.y.abs_diff(actor.position.y) <= 1
            })
            .min_by_key(|npc| npc.id);
        if let Some(npc) = adjacent {
            let _send_result = game
                .actions
                .try_send(ClientAction::TalkToNpc { target: npc.id });
        } else if let Some((vehicle, _tile, prototype_part_index)) = snapshot
            .vehicles
            .iter()
            .flat_map(|vehicle| {
                vehicle.tiles.iter().filter_map(move |tile| {
                    tile.boardable_prototype_part_index
                        .filter(|_| tile.passenger.is_none())
                        .map(|part_index| (vehicle, tile, part_index))
                })
            })
            .filter(|(_, tile, _)| {
                tile.position.z == actor.position.z
                    && tile.position.x.abs_diff(actor.position.x) <= 1
                    && tile.position.y.abs_diff(actor.position.y) <= 1
                    && tile.position != actor.position
            })
            .min_by_key(|(vehicle, _tile, part_index)| (vehicle.id, *part_index))
        {
            let _send_result = game.actions.try_send(ClientAction::BoardVehicle {
                vehicle_id: vehicle.id,
                prototype_part_index,
            });
        }
    }
}

fn client_unboard_direction(
    snapshot: &ReplicationSnapshotV1,
    from: cdda_protocol::WorldPosition,
) -> Option<HorizontalDirection> {
    [
        HorizontalDirection { dx: 0, dy: -1 },
        HorizontalDirection { dx: 1, dy: 0 },
        HorizontalDirection { dx: 0, dy: 1 },
        HorizontalDirection { dx: -1, dy: 0 },
        HorizontalDirection { dx: 1, dy: -1 },
        HorizontalDirection { dx: 1, dy: 1 },
        HorizontalDirection { dx: -1, dy: 1 },
        HorizontalDirection { dx: -1, dy: -1 },
    ]
    .into_iter()
    .find(|direction| {
        let Some(target) = from.checked_offset(direction.dx, direction.dy, 0) else {
            return false;
        };
        let (coord, local) = target.chunk_and_local();
        let passable = snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coord == coord)
            .and_then(|chunk| {
                chunk.tiles.get(
                    usize::from(local.y) * cdda_protocol::SUBMAP_SIZE as usize
                        + usize::from(local.x),
                )
            })
            .and_then(Option::as_ref)
            .is_some_and(|tile| {
                tile.currently_visible
                    && tile.terrain.move_cost > 0
                    && tile
                        .furniture
                        .as_ref()
                        .is_none_or(|furniture| furniture.move_cost_mod >= 0)
            });
        passable
            && !snapshot
                .visible_actors
                .iter()
                .any(|actor| actor.hp > 0 && actor.position == target)
            && !snapshot.npcs.iter().any(|npc| npc.position == target)
            && !snapshot
                .creatures
                .iter()
                .any(|creature| creature.hp > 0 && creature.position == target)
            && !snapshot
                .vehicles
                .iter()
                .flat_map(|vehicle| &vehicle.tiles)
                .any(|tile| tile.position == target)
    })
}

fn current_held_direction(keys: &ButtonInput<KeyCode>) -> Option<HorizontalDirection> {
    for (key, direction) in [
        (KeyCode::Numpad7, HorizontalDirection { dx: -1, dy: -1 }),
        (KeyCode::Home, HorizontalDirection { dx: -1, dy: -1 }),
        (KeyCode::Numpad9, HorizontalDirection { dx: 1, dy: -1 }),
        (KeyCode::PageUp, HorizontalDirection { dx: 1, dy: -1 }),
        (KeyCode::Numpad1, HorizontalDirection { dx: -1, dy: 1 }),
        (KeyCode::End, HorizontalDirection { dx: -1, dy: 1 }),
        (KeyCode::Numpad3, HorizontalDirection { dx: 1, dy: 1 }),
        (KeyCode::PageDown, HorizontalDirection { dx: 1, dy: 1 }),
    ] {
        if keys.pressed(key) {
            return Some(direction);
        }
    }
    let left = keys.pressed(KeyCode::KeyA)
        || keys.pressed(KeyCode::ArrowLeft)
        || keys.pressed(KeyCode::Numpad4);
    let right = keys.pressed(KeyCode::KeyD)
        || keys.pressed(KeyCode::ArrowRight)
        || keys.pressed(KeyCode::Numpad6);
    let up = keys.pressed(KeyCode::KeyW)
        || keys.pressed(KeyCode::ArrowUp)
        || keys.pressed(KeyCode::Numpad8);
    let down = keys.pressed(KeyCode::KeyS)
        || keys.pressed(KeyCode::ArrowDown)
        || keys.pressed(KeyCode::Numpad2);
    let direction = HorizontalDirection {
        dx: i8::from(right) - i8::from(left),
        dy: i8::from(down) - i8::from(up),
    };
    direction.is_valid().then_some(direction)
}

#[expect(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "Bevy systems express disjoint ECS access through typed query parameters"
)]
fn poll_game_updates(
    mut commands: Commands,
    game: Option<ResMut<GameClient>>,
    mut visuals: Query<(Entity, &ActorVisual, &mut Transform, &mut Sprite)>,
    mut item_visuals: Query<(Entity, &ItemVisual, &mut Transform), Without<ActorVisual>>,
    mut creature_visuals: Query<
        (Entity, &CreatureVisual, &mut Transform, &mut Sprite),
        (
            Without<ActorVisual>,
            Without<ItemVisual>,
            Without<TileVisual>,
        ),
    >,
    mut vehicle_visuals: Query<
        (Entity, &VehicleVisual, &mut Transform, &mut Sprite),
        (
            Without<ActorVisual>,
            Without<ItemVisual>,
            Without<CreatureVisual>,
            Without<TileVisual>,
        ),
    >,
    mut tile_visuals: Query<
        (&TileVisual, &mut Sprite),
        (
            Without<ActorVisual>,
            Without<ItemVisual>,
            Without<CreatureVisual>,
            Without<VehicleVisual>,
        ),
    >,
    content_items: Option<Res<ContentItems>>,
    content_monsters: Option<Res<ContentMonsters>>,
    content_terrain: Option<Res<ContentTerrain>>,
    content_furniture: Option<Res<ContentFurniture>>,
    content_proficiencies: Option<Res<ContentProficiencies>>,
    mut character_menu: ResMut<CharacterMenu>,
) {
    let Some(mut game) = game else {
        return;
    };
    let mut received = Vec::new();
    {
        let receiver = match game.updates.lock() {
            Ok(receiver) => receiver,
            Err(poisoned) => poisoned.into_inner(),
        };
        while let Ok(update) = receiver.try_recv() {
            received.push(update);
        }
    }
    for update in received {
        match update {
            ClientUpdate::Status(status) => game.status = status,
            ClientUpdate::CharacterList(characters) => {
                character_menu.show(characters);
                game.status = String::from("Choose an existing character or create a new one.");
            }
            ClientUpdate::CharacterSelectionRejected(reason) => {
                character_menu.reject(reason);
                game.status = format!(
                    "Character request rejected: {}.",
                    gameplay_rejection_message(reason)
                );
            }
            ClientUpdate::Events(events) => {
                game.notice = events
                    .iter()
                    .map(event_message)
                    .collect::<Vec<_>>()
                    .join(" ");
                if !game.notice.is_empty() {
                    game.status = format!("{}\n{}", game.status, game.notice);
                }
            }
            ClientUpdate::Chat(message) => {
                game.chat_messages.push_back(format!(
                    "[{}] {}: {}",
                    message.tick.0, message.from_character, message.text
                ));
                while game.chat_messages.len() > 5 {
                    game.chat_messages.pop_front();
                }
            }
            ClientUpdate::Snapshot {
                controlled_actor,
                snapshot,
            } => {
                character_menu.clear();
                game.controlled_actor = Some(controlled_actor);
                sync_actor_visuals(&mut commands, &snapshot, controlled_actor, &mut visuals);
                sync_item_visuals(
                    &mut commands,
                    &snapshot,
                    controlled_actor,
                    &mut item_visuals,
                );
                sync_creature_visuals(
                    &mut commands,
                    &snapshot,
                    controlled_actor,
                    &mut creature_visuals,
                );
                sync_vehicle_visuals(
                    &mut commands,
                    &snapshot,
                    controlled_actor,
                    &mut vehicle_visuals,
                );
                sync_terrain_visuals(
                    &snapshot,
                    controlled_actor,
                    content_furniture.as_deref(),
                    &mut tile_visuals,
                );
                let status = gameplay_status(
                    &snapshot,
                    controlled_actor,
                    content_items.as_deref(),
                    content_monsters.as_deref(),
                    content_terrain.as_deref(),
                    content_furniture.as_deref(),
                    content_proficiencies.as_deref(),
                );
                game.status = if game.notice.is_empty() {
                    status
                } else {
                    format!("{status}\nLast authoritative outcome: {}", game.notice)
                };
                game.snapshot = Some(*snapshot);
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the Bevy HUD renders independent modal resources without owning domain state"
)]
fn render_status_text(
    mut status_text: Query<&mut Text, With<StatusText>>,
    bootstrap: Res<BootstrapStatus>,
    game: Option<Res<GameClient>>,
    character_menu: Res<CharacterMenu>,
    composer: Res<ChatComposer>,
    menu: Res<ItemMenu>,
    craft_menu: Res<CraftMenu>,
    target_menu: Res<TargetMenu>,
    terrain_menu: Res<TerrainMenu>,
    interaction_menu: Res<InteractionMenu>,
) {
    let Some(game) = game else {
        return;
    };
    let Ok(mut text) = status_text.single_mut() else {
        return;
    };
    let chat_log = game
        .chat_messages
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let chat = if character_menu.is_active()
        || menu.action.is_some()
        || craft_menu.open
        || target_menu.action.is_some()
        || terrain_menu.action.is_some()
        || interaction_menu.is_active()
    {
        String::new()
    } else if composer.active {
        format!("\n\nChat (Enter sends, Escape cancels)> {}", composer.text)
    } else if chat_log.is_empty() {
        String::from(
            "\n\nPress Enter to chat. Use /report-last <details> after another player chats.",
        )
    } else {
        format!(
            "\n\nChat:\n{chat_log}\nPress Enter to chat; /report-last <details> reports the latest other speaker."
        )
    };
    let interaction = game
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.controlled_actor.pending_interaction.as_ref())
        .map_or_else(String::new, |pending| {
            let choices = pending
                .choices
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    let marker = if index == interaction_menu.selected {
                        ">"
                    } else {
                        " "
                    };
                    format!("{marker} {}", choice.label)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let instructions = if interaction_menu.waiting {
                "Waiting for the server…"
            } else {
                "Arrows or J/K select, Enter confirms, Escape cancels"
            };
            format!("\n\n{}\n{choices}\n{instructions}", pending.prompt)
        });
    text.0 = format!(
        "Cataclysm: Dark Days Ahead — Rust Multiplayer\n\nClient endpoint\n{}\n\n{}{}{}{}{}{}{}{}",
        bootstrap.identity,
        game.status,
        character_menu.display(),
        menu.display(),
        craft_menu.display(),
        target_menu.display(),
        terrain_menu.display(),
        interaction,
        chat
    );
}

fn event_message(event: &WorldEvent) -> String {
    match &event.kind {
        WorldEventKind::VehicleSpawned { prototype_id, .. } => {
            format!("A {prototype_id} vehicle entered the world.")
        }
        WorldEventKind::ActorBoardedVehicle { .. } => String::from("Boarded the vehicle."),
        WorldEventKind::ActorUnboardedVehicle { .. } => String::from("Left the vehicle."),
        WorldEventKind::VehicleCargoTaken { .. } => String::from("Took the vehicle cargo."),
        WorldEventKind::VehicleCargoStored { .. } => {
            String::from("Stored the item in the vehicle.")
        }
        WorldEventKind::VehiclePartOpenChanged { open, .. } => {
            if *open {
                String::from("Opened the vehicle part.")
            } else {
                String::from("Closed the vehicle part.")
            }
        }
        WorldEventKind::ActorMoved { .. } => String::from("Moved."),
        WorldEventKind::DamageApplied {
            body_part_id,
            amount,
            remaining_part_hp,
            remaining_hp,
            ..
        } => format!(
            "Hit a survivor's {body_part_id} for {amount}; {remaining_part_hp} part HP and {remaining_hp} vital HP remain."
        ),
        WorldEventKind::ActorMissedActor { .. } => String::from("Missed the survivor."),
        WorldEventKind::ActorDied { .. } => String::from("A survivor died."),
        WorldEventKind::CreatureMoved { .. } => String::from("A creature moved."),
        WorldEventKind::CreatureDamaged {
            amount,
            remaining_hp,
            ..
        } => format!("Hit a creature for {amount}; {remaining_hp} HP remains."),
        WorldEventKind::ActorMissedCreature { .. } => String::from("Missed the creature."),
        WorldEventKind::CreatureDied { .. } => String::from("The creature died."),
        WorldEventKind::MissionAssigned {
            mission_type_id, ..
        } => format!("Mission assigned: {mission_type_id}."),
        WorldEventKind::MissionFinished {
            mission_type_id,
            success,
            ..
        } => format!(
            "Mission {mission_type_id} {}.",
            if *success { "completed" } else { "failed" }
        ),
        WorldEventKind::CreatureCorpseCreated { .. } => String::from("The creature left a corpse."),
        WorldEventKind::CreatureRevived { .. } => String::from("A corpse rose again."),
        WorldEventKind::CreaturePolymorphed {
            from_type_id,
            to_type_id,
            ..
        } => format!("A creature changed from {from_type_id} into {to_type_id}."),
        WorldEventKind::CreatureSummoned {
            monster_type_id, ..
        } => format!("A creature summoned {monster_type_id}."),
        WorldEventKind::CreatureBashed {
            target_type_id,
            success,
            sound,
            volume,
            ..
        } => {
            let outcome = if *success { "gave way" } else { "held" };
            format!("{sound} ({volume}) The {target_type_id} {outcome}.")
        }
        WorldEventKind::ActorBashed {
            target_type_id,
            success,
            sound,
            volume,
            ..
        } => {
            let outcome = if *success { "gave way" } else { "held" };
            format!("{sound} ({volume}) You smashed {target_type_id}; it {outcome}.")
        }
        WorldEventKind::CreatureOpenedTerrain { from, to, .. } => {
            format!("A creature opened {from} into {to}.")
        }
        WorldEventKind::FieldIntensityChanged {
            field_type_id,
            intensity,
            ..
        } => {
            if *intensity == 0 {
                format!("The {field_type_id} field dissipated.")
            } else {
                format!("The {field_type_id} field changed to intensity {intensity}.")
            }
        }
        WorldEventKind::ActorDamagedByCreature {
            body_part_id,
            amount,
            remaining_part_hp,
            remaining_hp,
            ..
        } => format!(
            "A creature hit your {body_part_id} for {amount}; {remaining_part_hp} part HP and {remaining_hp} vital HP remain."
        ),
        WorldEventKind::CreatureMissedActor { stumbled, .. } => {
            if *stumbled {
                String::from("A creature missed you and fell.")
            } else {
                String::from("A creature missed you.")
            }
        }
        WorldEventKind::ActorKilledByCreature { .. } => {
            String::from("A creature killed your character.")
        }
        WorldEventKind::CommandRejected { reason, .. } => {
            format!("Command rejected: {}.", command_rejection_message(reason))
        }
        WorldEventKind::ConnectionChanged { connected, .. } => {
            format!("Connection state changed to {connected}.")
        }
        WorldEventKind::ItemPickedUp { .. } => String::from("Picked up the item."),
        WorldEventKind::ItemDropped { .. } => String::from("Dropped the item."),
        WorldEventKind::ItemWorn { .. } => String::from("Put on the item."),
        WorldEventKind::ItemTakenOff { .. } => String::from("Took off the item."),
        WorldEventKind::ItemWielded { item_id, .. } => item_id.map_or_else(
            || String::from("Stopped wielding."),
            |_| String::from("Wielded the item."),
        ),
        WorldEventKind::ItemConsumed {
            remaining_charges, ..
        } => format!("Consumed the item; {remaining_charges} charge(s) remain."),
        WorldEventKind::MedicalItemApplied {
            body_part_id,
            healed_hp,
            remaining_charges,
            ..
        } => format!(
            "Treated {body_part_id}; restored {healed_hp} HP and left {remaining_charges} charge(s)."
        ),
        WorldEventKind::EocMessage { text, .. } => text.clone(),
        WorldEventKind::EocItemActivated {
            remaining_charges, ..
        } => format!("Activated item; {remaining_charges} charge(s) remain."),
        WorldEventKind::ItemTransformed {
            from_type_id,
            to_type_id,
            remaining_charges,
            ..
        } => format!(
            "Transformed {from_type_id} into {to_type_id}; {remaining_charges} charge(s) remain."
        ),
        WorldEventKind::InteractionRequested { interaction, .. } => {
            format!("Interaction requested: {}", interaction.prompt)
        }
        WorldEventKind::InteractionCanceled { reason, .. } => match reason {
            InteractionCancellationReasonV1::Replaced => {
                String::from("The previous interaction was replaced.")
            }
            InteractionCancellationReasonV1::Expired => String::from("The interaction expired."),
            InteractionCancellationReasonV1::ClientCanceled => {
                String::from("Canceled the interaction.")
            }
            InteractionCancellationReasonV1::Invalidated => {
                String::from("The interaction is no longer valid.")
            }
            InteractionCancellationReasonV1::Completed => String::from("The conversation ended."),
        },
        WorldEventKind::ActorNeedsUpdated { .. } => String::from("Needs advanced."),
        WorldEventKind::ActorDiedFromNeeds { .. } => {
            String::from("Your character died from unmet needs.")
        }
        WorldEventKind::ActorAffectedByField {
            field_type_id,
            effect_id,
            message,
            ..
        } => {
            if message.is_empty() {
                format!("The {field_type_id} field applied {effect_id}.")
            } else {
                message.clone()
            }
        }
        WorldEventKind::ActorDamagedByEffect {
            effect_id,
            body_part_id,
            amount,
            remaining_part_hp,
            ..
        } => format!(
            "The {effect_id} effect damaged your {body_part_id} for {amount}; {remaining_part_hp} part HP remains."
        ),
        WorldEventKind::ActorDiedFromEffect { effect_id, .. } => {
            format!("Your character died from {effect_id}.")
        }
        WorldEventKind::TerrainChanged { to, .. } => {
            format!("Changed the terrain to {to}.")
        }
        WorldEventKind::RangedAttackResolved {
            hit,
            remaining_ammunition,
            ..
        } => format!(
            "Shot {}; {remaining_ammunition} round(s) remain.",
            if *hit { "hit" } else { "missed" }
        ),
        WorldEventKind::CreatureRangedAttackResolved {
            gun_type_id, hit, ..
        } => format!(
            "A creature fired {gun_type_id} and {} you.",
            if *hit { "hit" } else { "missed" }
        ),
        WorldEventKind::CreatureTargetedActor {
            sound, laser_lock, ..
        } => {
            if *laser_lock {
                String::from("A creature paints you with a targeting laser.")
            } else if sound.is_empty() {
                String::from("A creature targets you.")
            } else {
                format!("A creature targets you: {sound}")
            }
        }
        WorldEventKind::WeaponReloaded {
            loaded,
            ammunition_remaining,
            ..
        } => format!("Reloaded {loaded} round(s); {ammunition_remaining} now loaded."),
        WorldEventKind::MagazineReloaded {
            charges,
            ejected_magazine,
            ..
        } => format!(
            "Installed a {charges}-charge power cell{}.",
            if ejected_magazine.is_some() {
                " and kept the old cell"
            } else {
                ""
            }
        ),
        WorldEventKind::AmmunitionLoadedIntoPocket {
            loaded,
            pocket_ammunition,
            pocket_index,
            ..
        } => format!(
            "Loaded {loaded} charge(s) into pocket {pocket_index}; {pocket_ammunition} now loaded."
        ),
        WorldEventKind::AmmunitionInsertedIntoContainer {
            transferred,
            pocket_ammunition,
            pocket_index,
            ammunition_type,
            ..
        } => format!(
            "Stored {transferred} {ammunition_type} charge(s) in pocket {pocket_index}; {pocket_ammunition} now stored."
        ),
        WorldEventKind::PocketItemRemoved {
            pocket_index,
            charges,
            residual_energy_millijoules,
            ..
        } => format!(
            "Removed a {charges}-charge item{} from pocket {pocket_index}.",
            residual_power_suffix(*residual_energy_millijoules)
        ),
        WorldEventKind::PoweredToolChanged {
            active,
            reason,
            available_energy_millijoules,
            ..
        } => format!(
            "Powered tool {} ({reason:?}); {} J remain.",
            if *active { "activated" } else { "deactivated" },
            available_energy_millijoules / 1_000
        ),
        WorldEventKind::ActorFellAsleep { reason, .. } => {
            format!("Your character fell asleep ({reason:?}).")
        }
        WorldEventKind::ActorWokeUp { reason, .. } => {
            format!("Your character woke up ({reason:?}).")
        }
        WorldEventKind::CraftStarted { recipe_id, .. } => {
            format!("Started crafting {recipe_id}.")
        }
        WorldEventKind::CraftInterrupted { recipe_id, .. } => {
            format!("Crafting {recipe_id} was interrupted.")
        }
        WorldEventKind::CraftResumed { recipe_id, .. } => {
            format!("Resumed crafting {recipe_id}.")
        }
        WorldEventKind::CraftCanceled { recipe_id, .. } => {
            format!("Canceled crafting {recipe_id}.")
        }
        WorldEventKind::CraftCompleted { recipe_id, .. } => {
            format!("Finished crafting {recipe_id}.")
        }
        WorldEventKind::BookStudyStarted { skill_id, .. } => {
            format!("Started studying {skill_id}.")
        }
        WorldEventKind::BookStudyInterrupted { reason, .. } => {
            format!("Book study was interrupted ({reason:?}).")
        }
        WorldEventKind::BookStudyResumed { .. } => String::from("Resumed book study."),
        WorldEventKind::BookStudyCanceled { .. } => String::from("Canceled book study."),
        WorldEventKind::BookStudyCompleted {
            skill_id,
            experience_gained,
            theoretical_level,
            ..
        } => format!(
            "Finished studying {skill_id}; gained {experience_gained} theory XP (level {theoretical_level})."
        ),
        WorldEventKind::DisassemblyStarted { recipe_id, .. } => {
            format!("Started disassembling {recipe_id}.")
        }
        WorldEventKind::DisassemblyInterrupted { reason, .. } => {
            format!("Disassembly was interrupted ({reason:?}).")
        }
        WorldEventKind::DisassemblyResumed { .. } => String::from("Resumed disassembly."),
        WorldEventKind::DisassemblyCanceled { .. } => String::from("Canceled disassembly."),
        WorldEventKind::DisassemblyCompleted {
            recipe_id,
            recovered_items,
            destroyed_components,
            ..
        } => format!(
            "Finished disassembling {recipe_id}; recovered {} item(s), lost {} component unit(s).",
            recovered_items.len(),
            destroyed_components
                .iter()
                .map(|component| u64::from(component.count))
                .sum::<u64>()
        ),
        WorldEventKind::ConstructionStarted {
            construction_id, ..
        } => format!("Started construction {construction_id}."),
        WorldEventKind::ConstructionInterrupted {
            construction_id,
            reason,
            ..
        } => format!("Construction {construction_id} was interrupted ({reason:?})."),
        WorldEventKind::ConstructionResumed {
            construction_id, ..
        } => format!("Resumed construction {construction_id}."),
        WorldEventKind::ConstructionCanceled {
            construction_id, ..
        } => format!("Canceled construction {construction_id}."),
        WorldEventKind::ConstructionCompleted {
            construction_id, ..
        } => format!("Finished construction {construction_id}."),
        WorldEventKind::RecipeLearned { recipe_id, .. } => {
            format!("You learned the {recipe_id} recipe from disassembly.")
        }
        WorldEventKind::CraftToolChargesConsumed {
            charges,
            remaining_charges,
            ..
        } => format!(
            "Crafting used {charges} tool charge(s); {remaining_charges} remain in that tool."
        ),
        WorldEventKind::SkillLevelGained {
            skill_id,
            practical_level,
            theoretical_level,
            ..
        } => format!(
            "Your {skill_id} skill reached practical {practical_level}, theory {theoretical_level}."
        ),
        WorldEventKind::ProficiencyLearned { proficiency_id, .. } => {
            format!("You learned the {proficiency_id} proficiency.")
        }
    }
}

const fn command_rejection_message(reason: &CommandRejection) -> &'static str {
    match reason {
        CommandRejection::UnknownActor => "unknown actor",
        CommandRejection::ActorDead => "your character is dead",
        CommandRejection::StaleSequence => "stale command sequence",
        CommandRejection::InvalidMovement => "invalid movement",
        CommandRejection::Blocked => "the way is blocked",
        CommandRejection::TargetMissing => "target is missing",
        CommandRejection::TargetOutOfRange => "target is out of range",
        CommandRejection::ItemMissing => "item is missing",
        CommandRejection::ItemNotHere => "item is not here",
        CommandRejection::ItemNotOwned => "item is not carried",
        CommandRejection::ItemNotWearable => "item cannot be worn",
        CommandRejection::ItemAlreadyWorn => "item is already worn",
        CommandRejection::ItemNotWorn => "item is not worn",
        CommandRejection::ItemWorn => "take the item off first",
        CommandRejection::PocketMissing => "the selected item pocket is unavailable",
        CommandRejection::InventoryFull => "inventory is full",
        CommandRejection::ItemNotConsumable => "item cannot be consumed",
        CommandRejection::ItemNotActivatable => "item cannot be activated",
        CommandRejection::NoInteractionPending => "there is no interaction to answer",
        CommandRejection::StaleInteraction => "that interaction is no longer current",
        CommandRejection::InvalidInteractionChoice => "that choice is not available",
        CommandRejection::NpcRefusedDialogue => "the NPC refuses to talk",
        CommandRejection::VehicleMissing => "the vehicle is missing",
        CommandRejection::VehiclePartMissing => "the vehicle part is missing",
        CommandRejection::VehiclePartBroken => "the vehicle part is broken",
        CommandRejection::VehiclePartNotBoardable => "that vehicle part cannot be boarded",
        CommandRejection::VehiclePartNotCargo => "that vehicle part cannot hold cargo",
        CommandRejection::VehiclePartNotOpenable => "that vehicle part cannot be opened",
        CommandRejection::VehicleCargoLocked => "that vehicle cargo is locked",
        CommandRejection::VehiclePartObstructed => "something is blocking that vehicle part",
        CommandRejection::VehiclePartOccupied => "that vehicle seat is occupied",
        CommandRejection::ActorAlreadyBoarded => "your character is already aboard a vehicle",
        CommandRejection::ActorNotBoarded => "your character is not aboard that vehicle",
        CommandRejection::InvalidUnboardDestination => "there is nowhere safe to leave the vehicle",
        CommandRejection::ItemHasNoPower => "item has no usable battery charge",
        CommandRejection::PoweredToolActive => "turn the powered tool off first",
        CommandRejection::InvalidTerrainInteraction => "terrain cannot be changed",
        CommandRejection::InvalidBashInteraction => "that tile cannot be smashed",
        CommandRejection::InvalidBashTool => "wield a supported bash-only tool first",
        CommandRejection::ActionQueueFull => "action queue is full",
        CommandRejection::WeaponNotMelee => "wielded item has no admitted melee profile",
        CommandRejection::WeaponNotRanged => "wielded item is not a ranged weapon",
        CommandRejection::WeaponEmpty => "weapon is empty",
        CommandRejection::NoClearShot => "no clear shot",
        CommandRejection::WeaponFull => "weapon is already full",
        CommandRejection::IncompatibleAmmunition => "ammunition is incompatible",
        CommandRejection::PocketNotReloadable => "that pocket cannot be reloaded",
        CommandRejection::PocketNotUnloadable => "that pocket cannot be unloaded",
        CommandRejection::PocketItemMissing => "that item is not in the selected pocket",
        CommandRejection::PocketFull => "that pocket is full",
        CommandRejection::ActorSleeping => "your character is sleeping",
        CommandRejection::ActorAwake => "your character is already awake",
        CommandRejection::NotTired => "your character is not tired enough to sleep",
        CommandRejection::RecipeUnavailable => "the recipe is unavailable",
        CommandRejection::RecipeNotKnown => "your character has not learned the recipe",
        CommandRejection::InsufficientSkills => "your character lacks the required skills",
        CommandRejection::MissingProficiencies => "your character lacks a mandatory proficiency",
        CommandRejection::MissingComponents => "required components are missing",
        CommandRejection::MissingTools => "required tools are missing",
        CommandRejection::MissingQualities => "required tool qualities are missing",
        CommandRejection::ActorBusy => "your character is busy",
        CommandRejection::NoCraftInProgress => "no craft is in progress",
        CommandRejection::CraftNotInterrupted => "the craft is already running",
        CommandRejection::BookUnavailable => "the book cannot be studied",
        CommandRejection::BookMastered => "the book cannot raise your theory further",
        CommandRejection::TooDarkToRead => "natural daylight is required to read",
        CommandRejection::NoReadInProgress => "no book study is in progress",
        CommandRejection::ReadNotInterrupted => "the book study is already running",
        CommandRejection::DisassemblyUnavailable => "the item cannot be disassembled",
        CommandRejection::ItemDamaged => "the item has an invalid damage level",
        CommandRejection::TooDarkToDisassemble => "natural daylight is required to disassemble",
        CommandRejection::NoDisassemblyInProgress => "no disassembly is in progress",
        CommandRejection::DisassemblyNotInterrupted => "the disassembly is already running",
        CommandRejection::ConstructionUnavailable => "the construction is unavailable",
        CommandRejection::InvalidConstructionTarget => "the construction target is invalid",
        CommandRejection::TooDarkToConstruct => "more light is required to construct",
        CommandRejection::NoConstructionInProgress => "no construction is in progress",
        CommandRejection::ConstructionNotInterrupted => "the construction is already running",
        CommandRejection::StableIdsUnavailable => "stable IDs are temporarily unavailable",
    }
}

fn sync_actor_visuals(
    commands: &mut Commands,
    snapshot: &ReplicationSnapshotV1,
    controlled_actor: ActorId,
    visuals: &mut Query<(Entity, &ActorVisual, &mut Transform, &mut Sprite)>,
) {
    let Some((origin_x, origin_y)) = view_origin(snapshot, controlled_actor) else {
        return;
    };
    for (entity, visual, _transform, _sprite) in visuals.iter_mut() {
        if snapshot.controlled_actor.id != visual.0
            && !snapshot
                .visible_actors
                .iter()
                .any(|actor| actor.id == visual.0)
        {
            commands.entity(entity).despawn();
        }
    }
    let actors = std::iter::once((
        snapshot.controlled_actor.id,
        snapshot.controlled_actor.position,
        snapshot.controlled_actor.connected,
        snapshot.controlled_actor.sleeping,
        true,
    ))
    .chain(snapshot.visible_actors.iter().map(|actor| {
        (
            actor.id,
            actor.position,
            actor.connected,
            actor.sleeping,
            false,
        )
    }));
    for (actor_id, position, connected, sleeping, controlled) in actors {
        let x = ((position.x - origin_x) as f32 - 5.5) * 36.0;
        let y = (5.5 - (position.y - origin_y) as f32) * 36.0;
        let color = if sleeping {
            Color::srgb(0.48, 0.34, 0.78)
        } else if controlled {
            Color::srgb(0.20, 0.85, 0.45)
        } else if connected {
            Color::srgb(0.30, 0.55, 0.95)
        } else {
            Color::srgb(0.45, 0.45, 0.48)
        };
        if let Some((_entity, _visual, mut transform, mut sprite)) = visuals
            .iter_mut()
            .find(|(_entity, visual, _transform, _sprite)| visual.0 == actor_id)
        {
            transform.translation = Vec3::new(x, y, 1.0);
            sprite.color = color;
        } else {
            commands.spawn((
                Sprite::from_color(color, Vec2::splat(28.0)),
                Transform::from_xyz(x, y, 1.0),
                ActorVisual(actor_id),
            ));
        }
    }
}

fn sync_item_visuals(
    commands: &mut Commands,
    snapshot: &ReplicationSnapshotV1,
    controlled_actor: ActorId,
    visuals: &mut Query<(Entity, &ItemVisual, &mut Transform), Without<ActorVisual>>,
) {
    let Some((origin_x, origin_y)) = view_origin(snapshot, controlled_actor) else {
        return;
    };
    for (entity, visual, _transform) in visuals.iter_mut() {
        if !snapshot
            .ground_items
            .iter()
            .any(|ground| ground.item.id == visual.0)
        {
            commands.entity(entity).despawn();
        }
    }
    for ground in &snapshot.ground_items {
        let x = ((ground.position.x - origin_x) as f32 - 5.5) * 36.0;
        let y = (5.5 - (ground.position.y - origin_y) as f32) * 36.0;
        if let Some((_entity, _visual, mut transform)) = visuals
            .iter_mut()
            .find(|(_entity, visual, _transform)| visual.0 == ground.item.id)
        {
            transform.translation = Vec3::new(x, y, 0.5);
        } else {
            commands.spawn((
                Sprite::from_color(Color::srgb(0.82, 0.62, 0.28), Vec2::splat(14.0)),
                Transform::from_xyz(x, y, 0.5),
                ItemVisual(ground.item.id),
            ));
        }
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the filter proves creature visuals are disjoint from actor and item queries"
)]
fn sync_creature_visuals(
    commands: &mut Commands,
    snapshot: &ReplicationSnapshotV1,
    controlled_actor: ActorId,
    visuals: &mut Query<
        (Entity, &CreatureVisual, &mut Transform, &mut Sprite),
        (
            Without<ActorVisual>,
            Without<ItemVisual>,
            Without<TileVisual>,
        ),
    >,
) {
    let Some((origin_x, origin_y)) = view_origin(snapshot, controlled_actor) else {
        return;
    };
    for (entity, visual, _transform, _sprite) in visuals.iter_mut() {
        if !snapshot
            .creatures
            .iter()
            .any(|creature| creature.id == visual.0)
        {
            commands.entity(entity).despawn();
        }
    }
    for creature in &snapshot.creatures {
        let x = ((creature.position.x - origin_x) as f32 - 5.5) * 36.0;
        let y = (5.5 - (creature.position.y - origin_y) as f32) * 36.0;
        let color = if creature.hp > 0 {
            Color::srgb(0.82, 0.18, 0.18)
        } else {
            Color::srgb(0.28, 0.12, 0.12)
        };
        if let Some((_entity, _visual, mut transform, mut sprite)) = visuals
            .iter_mut()
            .find(|(_entity, visual, _transform, _sprite)| visual.0 == creature.id)
        {
            transform.translation = Vec3::new(x, y, 0.9);
            sprite.color = color;
        } else {
            commands.spawn((
                Sprite::from_color(color, Vec2::splat(26.0)),
                Transform::from_xyz(x, y, 0.9),
                CreatureVisual(creature.id),
            ));
        }
    }
}

#[expect(
    clippy::type_complexity,
    reason = "the filter proves vehicle visuals are disjoint from other dynamic sprite queries"
)]
fn sync_vehicle_visuals(
    commands: &mut Commands,
    snapshot: &ReplicationSnapshotV1,
    controlled_actor: ActorId,
    visuals: &mut Query<
        (Entity, &VehicleVisual, &mut Transform, &mut Sprite),
        (
            Without<ActorVisual>,
            Without<ItemVisual>,
            Without<CreatureVisual>,
            Without<TileVisual>,
        ),
    >,
) {
    let Some((origin_x, origin_y)) = view_origin(snapshot, controlled_actor) else {
        return;
    };
    for (entity, visual, _transform, _sprite) in visuals.iter_mut() {
        if !snapshot.vehicles.iter().any(|vehicle| {
            vehicle.id == visual.0
                && vehicle
                    .tiles
                    .iter()
                    .any(|tile| tile.prototype_part_index == visual.1)
        }) {
            commands.entity(entity).despawn();
        }
    }
    for vehicle in &snapshot.vehicles {
        for tile in &vehicle.tiles {
            let x = ((tile.position.x - origin_x) as f32 - 5.5) * 36.0;
            let y = (5.5 - (tile.position.y - origin_y) as f32) * 36.0;
            let health = tile.hp as f32 / tile.maximum_hp as f32;
            let color = if tile.passenger == Some(controlled_actor) {
                Color::srgb(0.28, 0.78, 0.88)
            } else if tile.open {
                Color::srgb(0.62, 0.55, 0.28)
            } else {
                Color::srgb(0.32 + 0.28 * health, 0.33 + 0.25 * health, 0.36)
            };
            if let Some((_entity, _visual, mut transform, mut sprite)) =
                visuals
                    .iter_mut()
                    .find(|(_entity, visual, _transform, _sprite)| {
                        visual.0 == vehicle.id && visual.1 == tile.prototype_part_index
                    })
            {
                transform.translation = Vec3::new(x, y, 0.82);
                sprite.color = color;
            } else {
                commands.spawn((
                    Sprite::from_color(color, Vec2::splat(30.0)),
                    Transform::from_xyz(x, y, 0.82),
                    VehicleVisual(vehicle.id, tile.prototype_part_index),
                ));
            }
        }
    }
}

fn view_origin(snapshot: &ReplicationSnapshotV1, controlled_actor: ActorId) -> Option<(i32, i32)> {
    let actor = &snapshot.controlled_actor;
    if actor.id != controlled_actor {
        return None;
    }
    let (chunk, _local) = actor.position.chunk_and_local();
    Some((
        chunk.x.checked_mul(cdda_protocol::SUBMAP_SIZE)?,
        chunk.y.checked_mul(cdda_protocol::SUBMAP_SIZE)?,
    ))
}

#[expect(
    clippy::type_complexity,
    reason = "the filter proves map tiles are disjoint from dynamic sprite queries"
)]
fn sync_terrain_visuals(
    snapshot: &ReplicationSnapshotV1,
    controlled_actor: ActorId,
    furniture: Option<&ContentFurniture>,
    visuals: &mut Query<
        (&TileVisual, &mut Sprite),
        (
            Without<ActorVisual>,
            Without<ItemVisual>,
            Without<CreatureVisual>,
            Without<VehicleVisual>,
        ),
    >,
) {
    let actor = &snapshot.controlled_actor;
    if actor.id != controlled_actor {
        return;
    }
    let (chunk_coord, _local) = actor.position.chunk_and_local();
    let Some(chunk) = snapshot
        .chunks
        .iter()
        .find(|chunk| chunk.coord == chunk_coord)
    else {
        return;
    };
    for (visual, mut sprite) in visuals.iter_mut() {
        let index =
            usize::from(visual.y) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(visual.x);
        let Some(tile) = chunk.tiles.get(index).and_then(Option::as_ref) else {
            sprite.color = Color::srgb(0.015, 0.015, 0.02);
            continue;
        };
        if tile.currently_visible
            && let Some(field) = tile
                .fields
                .iter()
                .filter(|field| field.display_field)
                .max_by_key(|field| (field.priority, field.display_sequence))
        {
            sprite.color = content_color(
                &field.color,
                snapshot.natural_light.phase,
                tile.currently_visible,
            );
            continue;
        }
        if let Some(furniture_tile) = &tile.furniture {
            sprite.color = furniture_color(
                &furniture_tile.furniture_id,
                furniture,
                snapshot.natural_light.phase,
                tile.currently_visible,
            );
            continue;
        }
        sprite.color = if !tile.currently_visible && !tile.terrain.open.is_empty() {
            Color::srgb(0.20, 0.12, 0.05)
        } else if !tile.currently_visible && tile.terrain.move_cost > 0 {
            Color::srgb(0.04, 0.045, 0.055)
        } else if !tile.currently_visible {
            Color::srgb(0.13, 0.14, 0.16)
        } else {
            let (floor, wall, door) = match snapshot.natural_light.phase {
                SkyPhase::Day => (0.11, 0.33, (0.45, 0.28, 0.12)),
                SkyPhase::CivilTwilight => (0.065, 0.21, (0.29, 0.18, 0.09)),
                SkyPhase::Night => (0.025, 0.10, (0.15, 0.09, 0.05)),
            };
            if !tile.terrain.open.is_empty() {
                Color::srgb(door.0, door.1, door.2)
            } else if tile.terrain.move_cost > 0 {
                let shade = if (visual.x + visual.y) % 2 == 0 {
                    floor
                } else {
                    floor + 0.01
                };
                Color::srgb(shade, shade + 0.005, shade + 0.01)
            } else {
                Color::srgb(wall, wall + 0.01, wall + 0.03)
            }
        };
    }
}

fn furniture_color(
    furniture_id: &str,
    furniture: Option<&ContentFurniture>,
    phase: SkyPhase,
    currently_visible: bool,
) -> Color {
    let color_name = furniture
        .and_then(|content| content.0.get(furniture_id))
        .and_then(|definition| definition.colors.first())
        .map(String::as_str)
        .unwrap_or("brown");
    content_color(color_name, phase, currently_visible)
}

fn content_color(color_name: &str, phase: SkyPhase, currently_visible: bool) -> Color {
    let (red, green, blue) = match color_name {
        "black" | "dark_gray" => (0.20, 0.21, 0.23),
        "white" | "light_gray" => (0.80, 0.81, 0.83),
        "red" | "light_red" => (0.72, 0.20, 0.16),
        "green" | "light_green" => (0.20, 0.62, 0.24),
        "blue" | "light_blue" => (0.22, 0.38, 0.78),
        "cyan" | "light_cyan" => (0.18, 0.68, 0.72),
        "magenta" | "pink" => (0.68, 0.25, 0.64),
        "yellow" | "light_yellow" => (0.78, 0.68, 0.18),
        "brown" | "light_brown" => (0.52, 0.31, 0.14),
        _ => (0.48, 0.34, 0.22),
    };
    let brightness = if !currently_visible {
        0.38
    } else {
        match phase {
            SkyPhase::Day => 1.0,
            SkyPhase::CivilTwilight => 0.66,
            SkyPhase::Night => 0.38,
        }
    };
    Color::srgb(red * brightness, green * brightness, blue * brightness)
}

fn gameplay_status(
    snapshot: &ReplicationSnapshotV1,
    controlled_actor: ActorId,
    content: Option<&ContentItems>,
    monsters: Option<&ContentMonsters>,
    terrain: Option<&ContentTerrain>,
    furniture: Option<&ContentFurniture>,
    proficiencies: Option<&ContentProficiencies>,
) -> String {
    let actor = &snapshot.controlled_actor;
    if actor.id != controlled_actor {
        return format!(
            "Connected at tick {}; controlled actor is absent.",
            snapshot.tick.0
        );
    }
    let item_name = |item: &ItemSnapshot| {
        item.variant.as_ref().map_or_else(
            || {
                content
                    .and_then(|content| content.0.get(&item.type_id))
                    .map_or_else(
                        || item.type_id.clone(),
                        |definition| definition.name.clone(),
                    )
            },
            |variant| variant.name.clone(),
        )
    };
    let inventory = actor.inventory.iter().map(item_name).collect::<Vec<_>>();
    let wielded = actor
        .wielded
        .and_then(|item_id| actor.inventory.iter().find(|item| item.id == item_id))
        .map_or_else(
            || String::from("nothing"),
            |item| {
                let name = item_name(item);
                item.ranged_weapon.as_ref().map_or(name.clone(), |weapon| {
                    format!(
                        "{name} [{}/{} {}]",
                        weapon.ammunition_remaining,
                        weapon.ammunition_capacity,
                        weapon.ammunition_type
                    )
                })
            },
        );
    let worn = actor
        .worn
        .iter()
        .filter_map(|item_id| actor.inventory.iter().find(|item| item.id == *item_id))
        .map(item_name)
        .collect::<Vec<_>>()
        .join(", ");
    let body_parts = actor
        .body_parts
        .iter()
        .map(|part| {
            format!(
                "{} {}/{}",
                part.body_part_id, part.current_hp, part.maximum_hp
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let effects = actor
        .effects
        .iter()
        .map(|effect| {
            let location = effect
                .body_part_id
                .as_ref()
                .map_or(String::new(), |body_part| format!(" on {body_part}"));
            let remaining_seconds = effect
                .expires_at_tick
                .0
                .saturating_sub(snapshot.tick.0)
                .div_ceil(cdda_protocol::SimTick::HZ);
            format!(
                "{}{} x{} ({}s)",
                effect.effect_id, location, effect.intensity, remaining_seconds
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ground_here = snapshot
        .ground_items
        .iter()
        .filter(|item| item.position == actor.position)
        .count();
    let nearest_hostile = snapshot
        .creatures
        .iter()
        .filter(|creature| creature.hp > 0)
        .min_by_key(|creature| {
            creature.position.x.abs_diff(actor.position.x)
                + creature.position.y.abs_diff(actor.position.y)
                + creature.position.z.abs_diff(actor.position.z)
        })
        .map(|creature| {
            let name = monsters
                .and_then(|content| content.0.get(&creature.type_id))
                .map_or(creature.type_id.as_str(), |definition| {
                    definition.name.as_str()
                });
            format!("{name} ({}/{})", creature.hp.max(0), creature.max_hp)
        })
        .unwrap_or_else(|| String::from("none"));
    let nearest_npc = snapshot
        .npcs
        .iter()
        .min_by_key(|npc| {
            npc.position.x.abs_diff(actor.position.x)
                + npc.position.y.abs_diff(actor.position.y)
                + npc.position.z.abs_diff(actor.position.z)
        })
        .map(|npc| {
            let opinion =
                npc.opinion_of_controlled_actor
                    .as_ref()
                    .map_or_else(String::new, |opinion| {
                        format!(
                            " (trust {}, fear {}, value {}, anger {}, owed {})",
                            opinion.trust, opinion.fear, opinion.value, opinion.anger, opinion.owed,
                        )
                    });
            format!(
                "{} [{}; {}] at ({}, {}, {}){}",
                npc.name,
                npc.faction_name,
                if npc.hostile_to_controlled_actor {
                    "hostile"
                } else {
                    "not hostile"
                },
                npc.position.x,
                npc.position.y,
                npc.position.z,
                opinion,
            )
        })
        .unwrap_or_else(|| String::from("none"));
    let nearest_vehicle = snapshot
        .vehicles
        .iter()
        .min_by_key(|vehicle| {
            vehicle
                .tiles
                .iter()
                .map(|tile| {
                    tile.position.x.abs_diff(actor.position.x)
                        + tile.position.y.abs_diff(actor.position.y)
                        + tile.position.z.abs_diff(actor.position.z)
                })
                .min()
                .unwrap_or(u32::MAX)
        })
        .map(|vehicle| {
            let boarded = vehicle
                .tiles
                .iter()
                .any(|tile| tile.passenger == Some(actor.id));
            format!(
                "{} at ({}, {}, {}){}",
                vehicle.name,
                vehicle.origin.x,
                vehicle.origin.y,
                vehicle.origin.z,
                if boarded { " (aboard)" } else { "" },
            )
        })
        .unwrap_or_else(|| String::from("none"));
    let current_observation = {
        let (chunk_coord, local) = actor.position.chunk_and_local();
        snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coord == chunk_coord)
            .and_then(|chunk| {
                let index = usize::from(local.y) * cdda_protocol::SUBMAP_SIZE as usize
                    + usize::from(local.x);
                chunk.tiles.get(index).and_then(Option::as_ref)
            })
    };
    let current_terrain = current_observation.map_or_else(
        || String::from("unknown"),
        |tile| {
            terrain
                .and_then(|content| content.0.get(&tile.terrain.terrain_id))
                .map_or_else(
                    || tile.terrain.terrain_id.clone(),
                    |definition| definition.name.clone(),
                )
        },
    );
    let current_furniture = current_observation
        .and_then(|tile| tile.furniture.as_ref())
        .map_or_else(
            || String::from("none"),
            |tile| {
                let name = furniture
                    .and_then(|content| content.0.get(&tile.furniture_id))
                    .map_or(tile.furniture_id.as_str(), |definition| {
                        definition.name.as_str()
                    });
                format!(
                    "{name} (comfort {}, bedding warmth {})",
                    tile.comfort, tile.floor_bedding_warmth
                )
            },
        );
    let craft = actor.craft_activity.as_ref().map_or_else(
        || String::from("none"),
        |activity| {
            let total = activity
                .recipe
                .time_moves
                .saturating_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
            let completed = total.saturating_sub(activity.remaining_action_points);
            let percent = completed
                .saturating_mul(100)
                .checked_div(total)
                .unwrap_or(0);
            format!(
                "{} ({}%, {})",
                activity.recipe.recipe_id,
                percent,
                if activity.interrupted {
                    "interrupted — B resumes, X cancels"
                } else {
                    "active — X cancels"
                }
            )
        },
    );
    let reading = actor.read_activity.as_ref().map_or_else(
        || String::from("none"),
        |activity| {
            let total = activity
                .study
                .time_moves
                .saturating_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
            let completed = total.saturating_sub(activity.remaining_action_points);
            let percent = completed
                .saturating_mul(100)
                .checked_div(total)
                .unwrap_or(0);
            format!(
                "{} via {} ({}%, {})",
                activity.study.skill_id,
                activity.study.book_type_id,
                percent,
                if activity.interrupted {
                    "interrupted — V resumes, X cancels"
                } else {
                    "active — X cancels"
                }
            )
        },
    );
    let disassembly = actor.disassembly_activity.as_ref().map_or_else(
        || String::from("none"),
        |activity| {
            let total = activity
                .recipe
                .time_moves
                .saturating_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
            let completed = total.saturating_sub(activity.remaining_action_points);
            let percent = completed
                .saturating_mul(100)
                .checked_div(total)
                .unwrap_or(0);
            format!(
                "{} ({}%, {})",
                activity.recipe.recipe_id,
                percent,
                if activity.interrupted {
                    "interrupted — N resumes, X cancels"
                } else {
                    "active — X cancels"
                }
            )
        },
    );
    let construction = actor.construction_activity.as_ref().map_or_else(
        || String::from("none"),
        |activity| {
            let total = activity
                .recipe
                .time_moves
                .saturating_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
            let completed = total.saturating_sub(activity.remaining_action_points);
            let percent = completed
                .saturating_mul(100)
                .checked_div(total)
                .unwrap_or(0);
            format!(
                "{} at ({}, {}, {}) ({}%, {})",
                activity.recipe.construction_id,
                activity.target.x,
                activity.target.y,
                activity.target.z,
                percent,
                if activity.interrupted {
                    "interrupted — M resumes, X cancels"
                } else {
                    "active — X cancels"
                }
            )
        },
    );
    let skills = actor
        .skills
        .iter()
        .map(|skill| {
            let next = u64::from(skill.practical_level) + 1;
            let threshold = 10_000_u64.saturating_mul(next).saturating_mul(next);
            let percent = u64::from(skill.practical_experience)
                .saturating_mul(100)
                .checked_div(threshold)
                .unwrap_or(0);
            format!(
                "{} {} ({}%; theory {})",
                skill.skill_id, skill.practical_level, percent, skill.theoretical_level
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let proficiency_progress = actor
        .proficiencies
        .iter()
        .map(|state| {
            let definition = proficiencies.and_then(|content| content.0.get(&state.proficiency_id));
            let name = definition.map_or(state.proficiency_id.as_str(), |definition| {
                definition.name.as_str()
            });
            if state.learned {
                format!("{name} (learned)")
            } else {
                let threshold = definition.map_or(0, |definition| {
                    definition.time_to_learn_moves.saturating_mul(
                        u64::try_from(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
                            .expect("positive action-point scale"),
                    )
                });
                let percent = state
                    .practiced_action_points
                    .saturating_mul(100)
                    .checked_div(threshold)
                    .unwrap_or(0);
                format!("{name} ({percent}%)")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mission_definitions = snapshot
        .mission_definitions
        .iter()
        .map(|definition| (definition.mission_type_id.as_str(), definition))
        .collect::<std::collections::BTreeMap<_, _>>();
    let missions = actor
        .missions
        .iter()
        .map(|mission| {
            let definition = mission_definitions
                .get(mission.mission_type_id.as_str())
                .copied();
            let name = definition.map_or(mission.mission_type_id.as_str(), |definition| {
                definition.name.as_str()
            });
            let status = match mission.status {
                cdda_protocol::MissionStatusV1::InProgress => "active",
                cdda_protocol::MissionStatusV1::Success => "completed",
                cdda_protocol::MissionStatusV1::Failure => "failed",
            };
            let objective = definition.map_or_else(
                || String::from("objective unavailable"),
                |definition| mission_objective(actor, mission, definition),
            );
            format!("{name}: {objective} ({status})")
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Connected at tick {} — Year {}, {:?}, day {} {:02}:{:02}:{:02}. Sky: {:?}; moon phase {}; sight radius {}. Move: WASD/arrows/numpad (Home/PageUp/End/PageDown diagonals); wait: ./numpad 5; sleep/wake: Z; interact/board/unboard: K; open/close adjacent: O/L; smash adjacent: H; pick up: G; drop: Q; wield/unwield: E/R; wear/take off: W/D; reload: U; insert first fitting container item: I; remove first pocket item: Y; consume: C; craft/resume: B; construct/resume: M; read/resume: V; disassemble/resume: N; cancel activity: X; select melee target: F; select ranged target: T.\nHP: {}. Body parts: [{}]. Effects: [{}]. Stamina: {}/{}; dodges: {}. Stats: STR {} DEX {} INT {} PER {}. Stored kcal: {}. Thirst: {}. Sleepiness: {} ({}). Readiness: {}/{}; queued actions: {}. Craft: {}. Reading: {}. Disassembly: {}. Construction: {}. Missions: [{}]. Learned recipes: {}. Skills: [{}]. Proficiencies: [{}]. Terrain: {}. Furniture: {}. Wielding: {}. Wearing: [{}]. Inventory: [{}]. Ground here: {} item(s). Nearest hostile: {}. Nearest NPC: {}. Nearest vehicle: {}.",
        snapshot.tick.0,
        snapshot.calendar.year,
        snapshot.calendar.season,
        snapshot.calendar.day_of_season,
        snapshot.calendar.hour,
        snapshot.calendar.minute,
        snapshot.calendar.second,
        snapshot.natural_light.phase,
        snapshot.natural_light.moon_phase,
        snapshot.natural_light.sight_radius,
        actor.hp,
        body_parts,
        effects,
        actor.stamina,
        actor.maximum_stamina,
        actor.dodge_attempts_remaining,
        actor.base_strength,
        actor.base_dexterity,
        actor.base_intelligence,
        actor.base_perception,
        actor.stored_kcal,
        actor.thirst,
        actor.sleepiness,
        if actor.sleeping { "sleeping" } else { "awake" },
        actor.action_points,
        ACTION_POINT_THRESHOLD,
        actor.queued_actions.len(),
        craft,
        reading,
        disassembly,
        construction,
        missions,
        actor.learned_recipes.len(),
        skills,
        proficiency_progress,
        current_terrain,
        current_furniture,
        wielded,
        worn,
        inventory.join(", "),
        ground_here,
        nearest_hostile,
        nearest_npc,
        nearest_vehicle,
    )
}

fn mission_objective(
    actor: &cdda_protocol::ActorSnapshot,
    mission: &cdda_protocol::MissionSnapshotV1,
    definition: &cdda_protocol::MissionDefinitionV1,
) -> String {
    match &definition.goal {
        cdda_protocol::MissionGoalV1::Null => {
            if definition.description.is_empty() {
                String::from("await the mission objective")
            } else {
                definition.description.clone()
            }
        }
        cdda_protocol::MissionGoalV1::FindItem {
            item_type_id,
            count,
            count_by_charges,
        } => {
            let progress = actor.inventory.iter().fold(0_u64, |total, item| {
                total.saturating_add(mission_item_quantity(item, item_type_id, *count_by_charges))
            });
            format!(
                "find {item_type_id} ({}/{count})",
                progress.min(u64::from(*count))
            )
        }
        cdda_protocol::MissionGoalV1::KillMonsterType {
            monster_type_id,
            count,
        } => mission_kill_objective(
            actor,
            mission,
            std::slice::from_ref(monster_type_id),
            *count,
            format!("kill {monster_type_id}"),
        ),
        cdda_protocol::MissionGoalV1::KillMonsterSpecies {
            monster_species_id,
            monster_type_ids,
            count,
        } => mission_kill_objective(
            actor,
            mission,
            monster_type_ids,
            *count,
            format!("kill {monster_species_id} creatures"),
        ),
    }
}

fn mission_kill_objective(
    actor: &cdda_protocol::ActorSnapshot,
    mission: &cdda_protocol::MissionSnapshotV1,
    monster_type_ids: &[String],
    required: u32,
    label: String,
) -> String {
    let current = monster_type_ids.iter().fold(0_u64, |total, id| {
        total.saturating_add(actor.creature_kill_counts.get(id).copied().unwrap_or(0))
    });
    let threshold = mission.kill_count_to_reach.unwrap_or(u64::from(required));
    let baseline = threshold.saturating_sub(u64::from(required));
    let progress = current.saturating_sub(baseline).min(u64::from(required));
    format!("{label} ({progress}/{required})")
}

fn mission_item_quantity(
    item: &cdda_protocol::ItemSnapshot,
    sought_type_id: &str,
    count_by_charges: bool,
) -> u64 {
    let own = if item.type_id == sought_type_id {
        if count_by_charges {
            u64::try_from(item.charges.max(0)).unwrap_or(0)
        } else {
            1
        }
    } else {
        0
    };
    let integral = item
        .integral_magazines
        .iter()
        .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
        .fold(0_u64, |total, nested| {
            total.saturating_add(mission_item_quantity(
                nested,
                sought_type_id,
                count_by_charges,
            ))
        });
    let wells = item
        .magazine_wells
        .iter()
        .filter_map(|well| well.installed_magazine.as_deref())
        .fold(0_u64, |total, nested| {
            total.saturating_add(mission_item_quantity(
                nested,
                sought_type_id,
                count_by_charges,
            ))
        });
    let containers = item
        .ammunition_containers
        .iter()
        .flat_map(|pocket| &pocket.contents)
        .fold(0_u64, |total, nested| {
            total.saturating_add(mission_item_quantity(
                nested,
                sought_type_id,
                count_by_charges,
            ))
        });
    own.saturating_add(integral)
        .saturating_add(wells)
        .saturating_add(containers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creature_miss_message_reports_the_clumsy_fall() {
        let event = |stumbled| WorldEvent {
            id: cdda_protocol::EventId::new(1, 1),
            tick: cdda_protocol::SimTick(1),
            kind: WorldEventKind::CreatureMissedActor {
                source: CreatureId::new(1, 2),
                target: ActorId::new(1, 3),
                stumbled,
                target_was_sleeping: false,
            },
        };
        assert_eq!(event_message(&event(false)), "A creature missed you.");
        assert_eq!(
            event_message(&event(true)),
            "A creature missed you and fell."
        );
    }

    #[test]
    fn character_menu_is_bounded_wraps_and_respects_account_capacity() {
        let characters = (1..=12)
            .map(|counter| CharacterSummary {
                actor_id: ActorId::new(1, counter),
                name: format!("survivor {counter}"),
            })
            .collect::<Vec<_>>();
        let mut menu = CharacterMenu::default();
        menu.show(characters);
        menu.select_previous();
        assert_eq!(menu.selected, 12);
        let display = menu.display();
        assert!(display.contains("Choose character (13/13)"));
        assert!(display.contains("Create new character"));
        assert_eq!(
            display
                .lines()
                .filter(|line| line.starts_with("> ") || line.starts_with("  "))
                .count(),
            9
        );
        menu.creating = true;
        menu.name = String::from("Ada");
        assert!(menu.display().contains("Name> Ada"));
        assert!(menu.display().contains("> STR 8"));
        for _ in 0..32 {
            menu.adjust_selected_stat(1);
        }
        assert_eq!(menu.base_stats.strength, MAX_CHARACTER_CREATION_STAT);
        menu.selected_stat = 2;
        for _ in 0..32 {
            menu.adjust_selected_stat(-1);
        }
        assert_eq!(menu.base_stats.intelligence, MIN_CHARACTER_CREATION_STAT);
        menu.creating = false;
        menu.reject(GameplayRejection::CharacterAlreadyExists);
        assert!(!menu.waiting);
        assert!(menu.notice.contains("character already exists"));

        menu.show(
            (1..=MAX_CHARACTERS_PER_ACCOUNT)
                .map(|counter| CharacterSummary {
                    actor_id: ActorId::new(1, counter as u64),
                    name: format!("survivor {counter}"),
                })
                .collect(),
        );
        assert_eq!(menu.choice_count(), MAX_CHARACTERS_PER_ACCOUNT);
        assert!(!menu.display().contains("Create new character"));
    }

    #[test]
    fn character_name_input_matches_protocol_bounds() {
        assert!(valid_character_name_input("Survivor"));
        assert!(valid_character_name_input(&"é".repeat(64)));
        assert!(!valid_character_name_input(""));
        assert!(!valid_character_name_input(&"a".repeat(65)));
        assert!(!valid_character_name_input("bad\nname"));
    }

    #[test]
    fn held_direction_combines_cardinal_keys_and_cancels_opposites() {
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        assert_eq!(
            current_held_direction(&keys),
            Some(HorizontalDirection { dx: 1, dy: -1 })
        );
        keys.press(KeyCode::KeyA);
        assert_eq!(
            current_held_direction(&keys),
            Some(HorizontalDirection { dx: 0, dy: -1 })
        );
        keys.press(KeyCode::KeyS);
        assert_eq!(current_held_direction(&keys), None);
    }

    #[test]
    fn explicit_diagonal_key_has_deterministic_precedence() {
        let mut keys = ButtonInput::default();
        keys.press(KeyCode::Numpad1);
        keys.press(KeyCode::KeyW);
        assert_eq!(
            current_held_direction(&keys),
            Some(HorizontalDirection { dx: -1, dy: 1 })
        );
    }

    #[test]
    fn item_menu_wraps_and_renders_a_bounded_window() {
        let mut menu = ItemMenu {
            action: Some(ItemMenuAction::Drop),
            entries: (1..=12)
                .map(|counter| ItemMenuEntry {
                    item_id: ItemId::new(1, counter),
                    label: format!("item {counter}"),
                })
                .collect(),
            selected: 0,
        };
        menu.select_previous();
        assert_eq!(menu.selected, 11);
        menu.select_next();
        assert_eq!(menu.selected, 0);
        menu.selected = 6;
        let display = menu.display();
        assert!(display.contains("Drop (7/12)"));
        assert_eq!(
            display.lines().filter(|line| line.starts_with('>')).count(),
            1
        );
        assert_eq!(
            display
                .lines()
                .filter(|line| line.starts_with("> ") || line.starts_with("  "))
                .count(),
            9
        );
    }

    #[test]
    fn item_menu_filters_actions_by_authoritative_item_state() {
        fn item(
            counter: u64,
            ammunition_type: &str,
            comestible_type: &str,
            ranged_weapon: Option<cdda_protocol::RangedWeaponSnapshot>,
        ) -> ItemSnapshot {
            ItemSnapshot {
                id: ItemId::new(1, counter),
                type_id: format!("item_{counter}"),
                charges: 5,
                damage: 0,
                raw_damage: 0,
                fitted: false,
                variant: None,
                snippet: None,
                variables: std::collections::BTreeMap::new(),
                melee_damage_milli: std::collections::BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::from(comestible_type),
                temperature: None,
                ammunition_type: String::from(ammunition_type),
                ranged_weapon,
                component_provenance: None,
                magazine_capacity: 0,
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
                creature_corpse: None,
                containment: Default::default(),
            }
        }

        let position = cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 };
        let food = item(1, "", "FOOD", None);
        let ammunition = item(2, "38", "", None);
        let gun = item(
            3,
            "",
            "",
            Some(cdda_protocol::RangedWeaponSnapshot {
                ammunition_type: String::from("38"),
                ammunition_remaining: 0,
                ammunition_capacity: 6,
                range: 6,
                damage: 10,
                dispersion: 100,
                sound_volume: 0,
            }),
        );
        let ground = item(4, "", "", None);
        let elsewhere = item(5, "", "", None);
        let mut temperature_item = item(8, "", "FOOD", None);
        temperature_item.temperature = Some(cdda_protocol::initial_item_temperature_state(
            cdda_protocol::SimTick(123),
            cdda_protocol::ItemPhaseV1::Solid,
            None,
        ));
        assert!(
            item_menu_label(&temperature_item, None).contains("[temperature pending]"),
            "normal client item menus should expose unprocessed authoritative temperature state"
        );
        let unprocessed_temperature_item = temperature_item.clone();
        let state = temperature_item
            .temperature
            .as_mut()
            .expect("temperature state should exist");
        state.temperature_millikelvin = cdda_protocol::ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN;
        state.specific_energy_millijoules_per_gram = None;
        assert!(item_menu_label(&temperature_item, None).contains("[20.0 °C]"));
        assert!(!same_item_stack_state(
            &unprocessed_temperature_item,
            &temperature_item
        ));
        let water = cdda_protocol::ItemThermalPropertiesV1 {
            specific_heat_liquid_microjoules_per_gram_kelvin: 4_186_000,
            specific_heat_solid_microjoules_per_gram_kelvin: 2_108_000,
            latent_heat_microjoules_per_gram: 333_000_000,
            freezing_point_millikelvin: 273_150,
        };
        let material_state = temperature_item
            .temperature
            .as_mut()
            .expect("temperature state should exist");
        material_state.thermal_properties = Some(water);
        material_state.specific_energy_millijoules_per_gram =
            water.normal_ambient_specific_energy_millijoules_per_gram();
        assert!(
            item_menu_label(&temperature_item, None).contains("[20.0 °C]"),
            "normal client item menus must not mistake numeric material energy for pending state"
        );
        let mut variant_item = item(6, "", "", None);
        variant_item.variant = Some(cdda_protocol::ItemVariantV1 {
            id: String::from("weathered"),
            name: String::from("weathered splinter"),
            description: String::from("A weathered splinter."),
            symbol: String::from(";"),
            color: String::from("brown"),
            ascii_picture: String::new(),
        });
        assert!(
            item_menu_label(&variant_item, None).starts_with("weathered splinter x5"),
            "the authoritative selected variant should be visible in normal item menus"
        );
        let mut variable_size = item(7, "", "", None);
        variable_size.type_id = String::from("throwing knives leg sheath");
        variable_size.containment.flags = vec![String::from("VARSIZE")];
        let unfitted_variable_size = variable_size.clone();
        assert!(
            item_menu_label(&variable_size, None)
                .starts_with("throwing knives leg sheath (poor fit) x5"),
            "replicated unfitted variable-size state should be visible in the normal client menu"
        );
        variable_size.fitted = true;
        assert!(item_menu_label(&variable_size, None).starts_with("throwing knives leg sheath x5"));
        assert!(!item_menu_label(&variable_size, None).contains("poor fit"));
        assert!(!same_item_stack_state(
            &unfitted_variable_size,
            &variable_size
        ));
        variant_item.snippet = Some(cdda_protocol::ItemSnippetV1 {
            id: String::from("provenance"),
            text: String::from("Found near the river"),
        });
        assert!(
            item_menu_label(&variant_item, None)
                .starts_with("weathered splinter — Found near the river x5"),
            "authoritative snippet text should be visible without consulting live content"
        );
        variant_item.variables.insert(
            String::from("description"),
            cdda_protocol::ItemVariableValueV1::String(String::from("A weathered splinter.")),
        );
        assert!(
            item_menu_label(&variant_item, None).starts_with(
                "weathered splinter — Found near the river — A weathered splinter. x5"
            ),
            "authoritative expanded descriptions should be visible in normal item menus"
        );
        let mut distinct_variables = variant_item.clone();
        distinct_variables.variables.insert(
            String::from("browsed"),
            cdda_protocol::ItemVariableValueV1::String(String::from("false")),
        );
        assert!(!same_item_stack_state(&variant_item, &distinct_variables));
        let tick = cdda_protocol::SimTick(0);
        let snapshot = ReplicationSnapshotV1 {
            tick,
            calendar: cdda_protocol::CalendarSnapshot::at_tick(tick),
            natural_light: cdda_protocol::NaturalLightSnapshot::at_tick(tick),
            detail_vision_available: true,
            controlled_actor: cdda_protocol::ActorSnapshot {
                id: ActorId::new(1, 10),
                position,
                hp: 100,
                body_parts: vec![cdda_protocol::ActorBodyPartSnapshotV1 {
                    body_part_id: String::from("torso"),
                    current_hp: 100,
                    maximum_hp: 100,
                }],
                effects: Vec::new(),
                eoc_variables: std::collections::BTreeMap::new(),
                next_eoc_schedule_sequence: 0,
                scheduled_eocs: Vec::new(),
                inactive_recurring_eocs: Vec::new(),
                base_strength: 8,
                base_dexterity: 8,
                base_intelligence: 8,
                base_perception: 8,
                connected: true,
                last_command_sequence: cdda_protocol::CommandSequence(0),
                last_held_input_sequence: cdda_protocol::HeldInputSequence(0),
                held_movement: None,
                inventory: vec![gun, ammunition, food],
                wielded: Some(ItemId::new(1, 3)),
                worn: Vec::new(),
                stored_kcal: 55_000,
                thirst: 0,
                sleepiness: 0,
                sleeping: false,
                sleep_intervals: 0,
                stamina: 8_500,
                maximum_stamina: 8_500,
                dodge_attempts_remaining: 1,
                speed: 100,
                action_points: 0,
                queued_actions: Vec::new(),
                craft_activity: None,
                read_activity: None,
                disassembly_activity: None,
                construction_activity: None,
                pending_interaction: None,
                missions: Vec::new(),
                learned_recipes: Vec::new(),
                skills: Vec::new(),
                proficiencies: Vec::new(),
                map_memory: Vec::new(),
            },
            visible_actors: Vec::new(),
            npcs: Vec::new(),
            mission_definitions: Vec::new(),
            creatures: Vec::new(),
            ground_items: vec![
                cdda_protocol::GroundItemSnapshot {
                    item: elsewhere,
                    position: cdda_protocol::WorldPosition { x: 1, y: 0, z: 0 },
                },
                cdda_protocol::GroundItemSnapshot {
                    item: ground,
                    position,
                },
            ],
            chunks: Vec::new(),
        };
        let ids = |action| {
            item_menu_entries(action, &snapshot, None, None, None)
                .into_iter()
                .map(|entry| entry.item_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(ItemMenuAction::PickUp), vec![ItemId::new(1, 4)]);
        assert_eq!(
            ids(ItemMenuAction::Drop),
            vec![ItemId::new(1, 1), ItemId::new(1, 2), ItemId::new(1, 3)]
        );
        assert_eq!(
            ids(ItemMenuAction::Wield),
            vec![ItemId::new(1, 1), ItemId::new(1, 2)]
        );
        assert_eq!(ids(ItemMenuAction::Reload), vec![ItemId::new(1, 2)]);
        assert_eq!(ids(ItemMenuAction::Consume), vec![ItemId::new(1, 1)]);

        let mut tool = item(6, "", "", None);
        tool.type_id = String::from("flashlight");
        tool.charges = 0;
        tool.magazine_wells = vec![
            cdda_protocol::MagazineWellSnapshotV1 {
                pocket_index: 1,
                pocket_id: String::from("AUXILIARY"),
                compatible_magazine_type_ids: vec![String::from("heavy_battery")],
                rigid: true,
                unloadable: true,
                installed_magazine: None,
            },
            cdda_protocol::MagazineWellSnapshotV1 {
                pocket_index: 4,
                pocket_id: String::from("POWER"),
                compatible_magazine_type_ids: vec![String::from("medium_battery")],
                rigid: true,
                unloadable: true,
                installed_magazine: None,
            },
        ];
        let mut battery = item(7, "battery", "", None);
        battery.type_id = String::from("medium_battery");
        battery.charges = 3;
        battery.magazine_capacity = 5;
        let mut incompatible_battery = item(8, "battery", "", None);
        incompatible_battery.type_id = String::from("heavy_battery");
        incompatible_battery.charges = 9;
        incompatible_battery.magazine_capacity = 10;
        let mut battery_snapshot = snapshot;
        battery_snapshot.controlled_actor.inventory =
            vec![tool.clone(), battery.clone(), incompatible_battery.clone()];
        battery_snapshot.controlled_actor.wielded = Some(tool.id);
        battery_snapshot.ground_items.clear();
        assert_eq!(
            item_menu_entries(ItemMenuAction::Reload, &battery_snapshot, None, None, None)
                .into_iter()
                .map(|entry| entry.item_id)
                .collect::<Vec<_>>(),
            vec![battery.id, incompatible_battery.id]
        );
        assert!(matches!(
            client_action_for_item_menu(
                ItemMenuAction::Reload,
                incompatible_battery.id,
                Some(&battery_snapshot),
            ),
            Some(ClientAction::Reload {
                ammunition_item,
                target_pocket_index: Some(1),
            }) if ammunition_item == incompatible_battery.id
        ));
        assert!(item_menu_label(&battery, None).contains("3/5"));
        battery.residual_energy_millijoules = 998_440;
        assert!(item_menu_label(&battery, None).contains("+ 998440 mJ"));

        tool.magazine_wells
            .iter_mut()
            .find(|well| well.pocket_index == 4)
            .expect("flashlight well exists")
            .installed_magazine = Some(Box::new(battery));
        tool.magazine_wells
            .iter_mut()
            .find(|well| well.pocket_index == 1)
            .expect("auxiliary well exists")
            .installed_magazine = Some(Box::new(incompatible_battery));
        tool.powered_tool = Some(cdda_protocol::PoweredToolStateV1 {
            inactive_type_id: String::from("flashlight"),
            active_type_id: String::from("flashlight_on"),
            activation_charges: 1,
            power_draw_milliwatts: 1_560,
            light_emission: 300,
            dims_with_charge: true,
            power_pocket_index: 4,
            active: false,
        });
        assert!(item_menu_label(&tool, None).contains("[power 3 + 998440 mJ]"));
        assert!(!item_menu_label(&tool, None).contains("[power 9"));
        battery_snapshot.controlled_actor.inventory = vec![tool.clone()];
        assert!(matches!(
            first_pocket_item_removal(&battery_snapshot),
            Some(ClientAction::RemovePocketItem {
                owner_item,
                pocket_index: 1,
                contained_item,
            }) if owner_item == tool.id && contained_item == ItemId::new(1, 8)
        ));
        let mut policy_locked_tool = tool.clone();
        policy_locked_tool
            .powered_tool
            .as_mut()
            .expect("powered state should exist")
            .active = true;
        policy_locked_tool
            .magazine_wells
            .iter_mut()
            .find(|well| well.pocket_index == 1)
            .expect("auxiliary well should exist")
            .unloadable = false;
        battery_snapshot.controlled_actor.inventory = vec![policy_locked_tool];
        assert!(first_pocket_item_removal(&battery_snapshot).is_none());

        let mut integral_owner = item(9, "", "", None);
        integral_owner.type_id = String::from("fractional_cell");
        let mut fractional_ammunition = item(10, "battery", "", None);
        fractional_ammunition.charges = 0;
        integral_owner.integral_magazines = vec![cdda_protocol::IntegralMagazinePocketSnapshotV1 {
            pocket_index: 2,
            pocket_id: String::from("RESERVE"),
            ammunition_type: String::from("battery"),
            capacity: 1,
            rigid: true,
            reloadable: true,
            unloadable: true,
            loaded_ammunition: Some(Box::new(fractional_ammunition)),
            residual_energy_millijoules: 123_456,
        }];
        battery_snapshot.controlled_actor.inventory = vec![integral_owner.clone()];
        assert!(item_menu_label(&integral_owner, None).contains("p2 0/1 battery + 123456 mJ"));
        assert!(matches!(
            first_pocket_item_removal(&battery_snapshot),
            Some(ClientAction::RemovePocketItem {
                owner_item,
                pocket_index: 2,
                contained_item,
            }) if owner_item == integral_owner.id && contained_item == ItemId::new(1, 10)
        ));

        let mut light_battery = item(15, "", "", None);
        light_battery.type_id = String::from("light_battery_cell");
        let mut loaded_battery = item(16, "battery", "", None);
        loaded_battery.charges = 16;
        light_battery.integral_magazines = vec![cdda_protocol::IntegralMagazinePocketSnapshotV1 {
            pocket_index: 0,
            pocket_id: String::from("MAGAZINE"),
            ammunition_type: String::from("battery"),
            capacity: 16,
            rigid: true,
            reloadable: false,
            unloadable: false,
            loaded_ammunition: Some(Box::new(loaded_battery)),
            residual_energy_millijoules: 0,
        }];
        assert!(
            item_menu_label(&light_battery, None).contains("p0 16/16 battery"),
            "the normal Bevy item menu must expose generated integral ammunition"
        );

        let mut quiver = item(11, "", "", None);
        quiver.type_id = String::from("quiver");
        quiver.charges = 1;
        quiver.ammunition_containers = vec![cdda_protocol::AmmunitionContainerPocketSnapshotV1 {
            pocket_index: 3,
            pocket_id: String::from("QUIVER"),
            capacities: vec![
                cdda_protocol::AmmunitionCapacityV1 {
                    ammunition_type: String::from("arrow"),
                    capacity: 20,
                },
                cdda_protocol::AmmunitionCapacityV1 {
                    ammunition_type: String::from("bolt"),
                    capacity: 20,
                },
            ],
            rigid: false,
            access_moves: 20,
            reloadable: true,
            unloadable: true,
            contents: Vec::new(),
            spawn_state: None,
        }];
        let mut arrows = item(12, "arrow", "", None);
        arrows.type_id = String::from("arrow_wood");
        arrows.charges = 10;
        let mut bolts = item(13, "bolt", "", None);
        bolts.type_id = String::from("bolt_wood");
        bolts.charges = 10;
        battery_snapshot.controlled_actor.inventory =
            vec![bolts.clone(), arrows.clone(), quiver.clone()];
        assert!(matches!(
            first_pocket_item_insertion(&battery_snapshot),
            Some(ClientAction::InsertPocketItem {
                owner_item,
                pocket_index: 3,
                source_item,
            }) if owner_item == quiver.id && source_item == arrows.id
        ));
        assert!(item_menu_label(&quiver, None).contains("p3 empty"));
        let mut contained = arrows.clone();
        contained.id = ItemId::new(1, 14);
        contained.charges = 6;
        quiver.ammunition_containers[0].contents.push(contained);
        battery_snapshot.controlled_actor.inventory = vec![bolts, arrows, quiver.clone()];
        let quiver_snapshot = battery_snapshot.clone();
        let mut phone_case = item(15, "", "", None);
        phone_case.type_id = String::from("waterproof_smart_phone_case");
        let mut phone = item(16, "", "", None);
        phone.type_id = String::from("smart_phone");
        phone_case.ammunition_containers =
            vec![cdda_protocol::AmmunitionContainerPocketSnapshotV1 {
                pocket_index: 4,
                pocket_id: String::from("PHONE"),
                capacities: Vec::new(),
                rigid: true,
                access_moves: 100,
                reloadable: false,
                unloadable: true,
                contents: vec![phone.clone()],
                spawn_state: Some(cdda_protocol::SpawnPocketStateV1 {
                    rules: cdda_protocol::SpawnPocketRulesV1 {
                        kind: cdda_protocol::SpawnPocketKindV1::Container,
                        max_contains_volume_milliliters: 111,
                        magazine_well_volume_milliliters: 0,
                        contents_collapsed_by_default: false,
                        max_contains_weight_milligrams: 233_000,
                        max_item_volume_milliliters: 111,
                        min_item_volume_milliliters: 0,
                        max_item_length_millimeters: 150,
                        item_restrictions: vec![String::from("smart_phone")],
                        flag_restrictions: Vec::new(),
                        access_moves: 100,
                        rigid: true,
                        watertight: true,
                        transparent: true,
                        forbidden: false,
                        sealable: false,
                    },
                    contents_collapsed: false,
                    sealed: false,
                }),
            }];
        assert!(item_menu_label(&phone_case, None).contains("p4 1 items"));
        battery_snapshot.controlled_actor.inventory = vec![phone_case.clone()];
        assert!(matches!(
            first_pocket_item_removal(&battery_snapshot),
            Some(ClientAction::RemovePocketItem {
                owner_item,
                pocket_index: 4,
                contained_item,
            }) if owner_item == phone_case.id && contained_item == phone.id
        ));
        assert!(first_pocket_item_insertion(&battery_snapshot).is_none());
        let mut painkiller_bottle = item(17, "", "", None);
        painkiller_bottle.type_id = String::from("bottle_plastic_pill_painkiller");
        let mut aspirin = item(18, "", "MED", None);
        aspirin.type_id = String::from("aspirin");
        aspirin.charges = 1;
        painkiller_bottle.ammunition_containers =
            vec![cdda_protocol::AmmunitionContainerPocketSnapshotV1 {
                pocket_index: 0,
                pocket_id: String::from("CONTAINER"),
                capacities: Vec::new(),
                rigid: true,
                access_moves: 400,
                reloadable: false,
                unloadable: true,
                contents: vec![aspirin.clone()],
                spawn_state: Some(cdda_protocol::SpawnPocketStateV1 {
                    rules: cdda_protocol::SpawnPocketRulesV1 {
                        kind: cdda_protocol::SpawnPocketKindV1::Container,
                        max_contains_volume_milliliters: 250,
                        magazine_well_volume_milliliters: 0,
                        contents_collapsed_by_default: false,
                        max_contains_weight_milligrams: 1_000_000,
                        max_item_volume_milliliters: 17,
                        min_item_volume_milliliters: 0,
                        max_item_length_millimeters: 170,
                        item_restrictions: Vec::new(),
                        flag_restrictions: Vec::new(),
                        access_moves: 400,
                        rigid: true,
                        watertight: true,
                        transparent: true,
                        forbidden: false,
                        sealable: true,
                    },
                    contents_collapsed: true,
                    sealed: false,
                }),
            }];
        assert!(
            item_menu_label(&painkiller_bottle, None).contains("p0 1 items, collapsed"),
            "the normal Bevy item menu must expose auto-collapsed contained loot"
        );
        battery_snapshot.controlled_actor.inventory = vec![painkiller_bottle.clone()];
        assert!(matches!(
            first_pocket_item_removal(&battery_snapshot),
            Some(ClientAction::RemovePocketItem {
                owner_item,
                pocket_index: 0,
                contained_item,
            }) if owner_item == painkiller_bottle.id && contained_item == aspirin.id
        ));
        battery_snapshot = quiver_snapshot;
        assert!(item_menu_label(&quiver, None).contains("p3 arrow 6/20"));
        assert!(matches!(
            first_pocket_item_removal(&battery_snapshot),
            Some(ClientAction::RemovePocketItem {
                owner_item,
                pocket_index: 3,
                contained_item,
            }) if owner_item == quiver.id && contained_item == ItemId::new(1, 14)
        ));
        assert!(matches!(
            first_pocket_item_insertion(&battery_snapshot),
            Some(ClientAction::InsertPocketItem { source_item, .. }) if source_item == ItemId::new(1, 12)
        ));
        battery_snapshot.controlled_actor.inventory = vec![tool.clone()];
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Activate,
                &battery_snapshot,
                None,
                None,
                None,
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![tool.id]
        );
        tool.magazine_wells
            .iter_mut()
            .find(|well| well.pocket_index == 4)
            .and_then(|well| well.installed_magazine.as_deref_mut())
            .expect("battery is installed")
            .charges = 0;
        battery_snapshot.controlled_actor.inventory = vec![tool.clone()];
        assert!(
            item_menu_entries(
                ItemMenuAction::Activate,
                &battery_snapshot,
                None,
                None,
                None,
            )
            .is_empty()
        );
        tool.type_id = String::from("flashlight_on");
        tool.powered_tool
            .as_mut()
            .expect("powered state exists")
            .active = true;
        battery_snapshot.controlled_actor.inventory = vec![tool.clone()];
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Activate,
                &battery_snapshot,
                None,
                None,
                None,
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![tool.id],
            "an active tool remains selectable so it can be turned off"
        );
    }

    #[test]
    fn craft_menu_uses_pinned_recipe_alternatives_and_carried_components() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(DEFAULT_MANIFEST_PATH);
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
        let skills = SkillRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("skills should load");
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
        let terrain = TerrainRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("terrain should load");
        let furniture = FurnitureRegistry::load_selected(&manifest, content_root, &mods, &enabled)
            .expect("furniture should load");
        let item = |counter: u64, type_id: &str| ItemSnapshot {
            id: ItemId::new(1, counter),
            type_id: type_id.to_owned(),
            charges: 1,
            damage: 0,
            raw_damage: 0,
            fitted: false,
            variant: None,
            snippet: None,
            variables: BTreeMap::new(),
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            temperature: None,
            ammunition_type: String::new(),
            ranged_weapon: None,
            component_provenance: None,
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: Default::default(),
        };
        let tick = cdda_protocol::SimTick(0);
        let mut snapshot = ReplicationSnapshotV1 {
            tick,
            calendar: cdda_protocol::CalendarSnapshot::at_tick(tick),
            natural_light: cdda_protocol::NaturalLightSnapshot::at_tick(tick),
            detail_vision_available: true,
            controlled_actor: cdda_protocol::ActorSnapshot {
                id: ActorId::new(1, 10),
                position: cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 },
                hp: 100,
                body_parts: vec![cdda_protocol::ActorBodyPartSnapshotV1 {
                    body_part_id: String::from("torso"),
                    current_hp: 100,
                    maximum_hp: 100,
                }],
                effects: Vec::new(),
                eoc_variables: BTreeMap::new(),
                next_eoc_schedule_sequence: 0,
                scheduled_eocs: Vec::new(),
                inactive_recurring_eocs: Vec::new(),
                base_strength: 8,
                base_dexterity: 8,
                base_intelligence: 8,
                base_perception: 8,
                connected: true,
                last_command_sequence: cdda_protocol::CommandSequence(0),
                last_held_input_sequence: cdda_protocol::HeldInputSequence(0),
                held_movement: None,
                inventory: vec![item(1, "rock"), item(2, "socks_wool")],
                wielded: None,
                worn: Vec::new(),
                stored_kcal: 55_000,
                thirst: 0,
                sleepiness: 0,
                sleeping: false,
                sleep_intervals: 0,
                stamina: 8_500,
                maximum_stamina: 8_500,
                dodge_attempts_remaining: 1,
                speed: 100,
                action_points: 0,
                queued_actions: Vec::new(),
                craft_activity: None,
                read_activity: None,
                disassembly_activity: None,
                construction_activity: None,
                pending_interaction: None,
                missions: Vec::new(),
                learned_recipes: Vec::new(),
                skills: Vec::new(),
                proficiencies: Vec::new(),
                map_memory: Vec::new(),
            },
            visible_actors: Vec::new(),
            npcs: Vec::new(),
            mission_definitions: Vec::new(),
            creatures: Vec::new(),
            ground_items: Vec::new(),
            chunks: Vec::new(),
        };
        snapshot.controlled_actor.inventory.push(item(8, "w_table"));
        assert!(
            construction_menu_entries(
                &snapshot,
                &constructions,
                &recipes,
                &items,
                &terrain,
                &furniture,
            )
            .iter()
            .any(|entry| entry.recipe_id == "constr_place_table")
        );
        let mut duct_tape = item(10, "duct_tape");
        duct_tape.charges = 5;
        let prior_inventory = std::mem::replace(
            &mut snapshot.controlled_actor.inventory,
            vec![duct_tape, item(11, "g_carpet")],
        );
        assert!(
            construction_menu_entries(
                &snapshot,
                &constructions,
                &recipes,
                &items,
                &terrain,
                &furniture,
            )
            .iter()
            .any(|entry| entry.recipe_id == "constr_carpet_conc_green")
        );
        let mut nails = item(12, "nail");
        nails.charges = 5;
        snapshot.controlled_actor.inventory = vec![nails, item(13, "g_carpet"), item(14, "hammer")];
        assert!(
            construction_menu_entries(
                &snapshot,
                &constructions,
                &recipes,
                &items,
                &terrain,
                &furniture,
            )
            .iter()
            .any(|entry| entry.recipe_id == "constr_carpet_green"),
            "the client must expand nails LIST to pinned nail alternatives"
        );
        let brick_oven = constructions
            .get("constr_brick_oven_finisher")
            .expect("quality construction should load");
        let mut provider_types = BTreeSet::new();
        for group in &brick_oven.qualities {
            let quality = &group[0];
            let provider = items
                .iter()
                .find_map(|(type_id, definition)| {
                    (!definition.unsupported_fields.contains("qualities")
                        && definition
                            .qualities
                            .get(&quality.quality_id)
                            .is_some_and(|provided| provided.level >= quality.level))
                    .then(|| type_id.to_owned())
                })
                .expect("pinned construction quality should have an inherent provider");
            provider_types.insert(provider);
        }
        snapshot.controlled_actor.inventory = vec![item(12, "log")];
        snapshot
            .controlled_actor
            .inventory
            .extend(provider_types.iter().enumerate().map(|(index, type_id)| {
                item(
                    13 + u64::try_from(index).expect("provider count fits u64"),
                    type_id,
                )
            }));
        let prior_skills = std::mem::replace(
            &mut snapshot.controlled_actor.skills,
            vec![cdda_protocol::SkillLevelSnapshot {
                skill_id: String::from("fabrication"),
                practical_level: 3,
                practical_experience: 0,
                theoretical_level: 3,
                theoretical_experience: 0,
                last_practiced: tick,
            }],
        );
        assert!(
            construction_menu_entries(
                &snapshot,
                &constructions,
                &recipes,
                &items,
                &terrain,
                &furniture,
            )
            .iter()
            .any(|entry| entry.recipe_id == "constr_brick_oven_finisher")
        );
        snapshot.controlled_actor.inventory = prior_inventory;
        snapshot.controlled_actor.skills = prior_skills;
        snapshot.controlled_actor.inventory.pop();
        snapshot
            .controlled_actor
            .inventory
            .push(item(9, "manual_pistol"));
        let content_items = ContentItems(items.clone());
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Read,
                &snapshot,
                Some(&content_items),
                None,
                None,
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![ItemId::new(1, 9)]
        );
        snapshot.natural_light.phase = SkyPhase::Night;
        snapshot.detail_vision_available = false;
        assert!(
            item_menu_entries(
                ItemMenuAction::Read,
                &snapshot,
                Some(&content_items),
                None,
                None,
            )
            .is_empty()
        );
        snapshot.natural_light.phase = SkyPhase::Day;
        snapshot.detail_vision_available = true;
        snapshot.controlled_actor.inventory.pop();
        snapshot
            .controlled_actor
            .inventory
            .extend([item(10, "makeshift_scythe_war"), item(11, "hammer")]);
        let content_recipes = ContentRecipes(recipes.clone());
        let ammunition =
            AmmunitionRegistry::load_selected(&manifest, content_root, &mods, &enabled)
                .expect("ammunition should load");
        let content_ammunition = ContentAmmunition(ammunition);
        let empty_only_tool_type_id = items
            .iter()
            .find_map(|(type_id, definition)| {
                (definition.subtypes.contains("TOOL")
                    && !definition.subtypes.contains("GUN")
                    && !definition.tool_ammunition.is_empty()
                    && definition.default_charges() == 0
                    && recipes
                        .strict_disassembly_recipe_for_result(
                            type_id,
                            &items,
                            &content_ammunition.0,
                        )
                        .is_some())
                .then(|| type_id.to_owned())
            })
            .expect("pinned content should expose an empty-only powered-tool recipe");
        let mut empty_only_tool = item(13, &empty_only_tool_type_id);
        empty_only_tool.charges = 0;
        snapshot.controlled_actor.inventory.push(empty_only_tool);
        snapshot
            .controlled_actor
            .inventory
            .extend(items.iter().enumerate().map(|(index, (type_id, _))| {
                let mut support = item(
                    1_000 + u64::try_from(index).expect("content count fits u64"),
                    type_id,
                );
                support.charges = 1_000_000;
                support
            }));
        let empty_tool = snapshot
            .controlled_actor
            .inventory
            .iter()
            .find(|carried| carried.id == ItemId::new(1, 13))
            .expect("empty-only powered tool is carried");
        assert!(can_disassemble_item(
            &snapshot,
            &recipes,
            &items,
            &content_ammunition.0,
            empty_tool,
        ));
        snapshot
            .controlled_actor
            .inventory
            .iter_mut()
            .find(|carried| carried.id == ItemId::new(1, 13))
            .expect("empty-only powered tool is carried")
            .charges = 20;
        let charged_empty_only_tool = snapshot
            .controlled_actor
            .inventory
            .iter()
            .find(|carried| carried.id == ItemId::new(1, 13))
            .expect("empty-only powered tool is carried");
        assert!(
            !can_disassemble_item(
                &snapshot,
                &recipes,
                &items,
                &content_ammunition.0,
                charged_empty_only_tool,
            ),
            "the client must hide a powered tool with unmodeled stored energy"
        );
        snapshot
            .controlled_actor
            .inventory
            .retain(|carried| carried.id.counter() < 13);
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Disassemble,
                &snapshot,
                Some(&content_items),
                Some(&content_ammunition),
                Some(&content_recipes),
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![ItemId::new(1, 10)]
        );
        snapshot
            .controlled_actor
            .inventory
            .iter_mut()
            .find(|item| item.id == ItemId::new(1, 10))
            .expect("scythe is carried")
            .damage = cdda_protocol::MAX_ITEM_DAMAGE_LEVEL;
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Disassemble,
                &snapshot,
                Some(&content_items),
                Some(&content_ammunition),
                Some(&content_recipes),
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![ItemId::new(1, 10)],
            "damaged targets remain eligible with reduced recovery odds"
        );
        let bow_ammunition_type = content_items
            .0
            .get("compositebow")
            .and_then(|definition| definition.ammo.first())
            .expect("pinned composite bow should have one ammunition type")
            .clone();
        let mut composite_bow = item(12, "compositebow");
        composite_bow.ranged_weapon = Some(cdda_protocol::RangedWeaponSnapshot {
            ammunition_type: bow_ammunition_type,
            ammunition_remaining: 1,
            ammunition_capacity: 1,
            range: 10,
            damage: 10,
            dispersion: 100,
            sound_volume: 0,
        });
        snapshot.controlled_actor.inventory.push(composite_bow);
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Disassemble,
                &snapshot,
                Some(&content_items),
                Some(&content_ammunition),
                Some(&content_recipes),
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![ItemId::new(1, 10), ItemId::new(1, 12)],
            "a loaded pinned bare ranged target should be selectable"
        );
        snapshot
            .controlled_actor
            .inventory
            .iter_mut()
            .find(|item| item.id == ItemId::new(1, 12))
            .expect("composite bow is carried")
            .ranged_weapon = None;
        assert_eq!(
            item_menu_entries(
                ItemMenuAction::Disassemble,
                &snapshot,
                Some(&content_items),
                Some(&content_ammunition),
                Some(&content_recipes),
            )
            .into_iter()
            .map(|entry| entry.item_id)
            .collect::<Vec<_>>(),
            vec![ItemId::new(1, 10)],
            "a gun definition without canonical ranged state must stay hidden"
        );
        snapshot
            .controlled_actor
            .inventory
            .retain(|carried| carried.id.counter() < 10);
        let entries = craft_menu_entries(&snapshot, &recipes, &items);
        let rock_sock = entries
            .iter()
            .find(|entry| entry.recipe_id == "rock_sock")
            .expect("wool socks should satisfy the pinned alternative");
        assert!(rock_sock.label.contains("5s"));
        assert!(rock_sock.label.contains("[rock_sock]"));
        let learned_only = recipes
            .get("flashlight")
            .expect("pinned disassembly-learnable recipe should load");
        assert!(!client_recipe_knowledge_allows(
            &snapshot.controlled_actor,
            &items,
            learned_only,
        ));
        snapshot
            .controlled_actor
            .learned_recipes
            .push(String::from("flashlight"));
        assert!(client_recipe_knowledge_allows(
            &snapshot.controlled_actor,
            &items,
            learned_only,
        ));

        snapshot.controlled_actor.inventory.pop();
        assert!(
            !craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "rock_sock")
        );

        snapshot.controlled_actor.inventory = vec![item(3, "stick")];
        assert!(
            !craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "pointy_stick")
        );
        snapshot
            .controlled_actor
            .inventory
            .push(item(4, "knife_small"));
        assert!(
            craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "pointy_stick")
        );
        snapshot.controlled_actor.inventory = vec![item(5, "stick"), item(6, "circsaw_on")];
        assert!(
            craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "pointy_stick"),
            "non-unit-speed inherent quality should remain valid for a legacy recipe"
        );
        let charged_quality_recipe = cdda_content::RecipeDefinition {
            qualities: vec![vec![cdda_content::QualityRequirement {
                quality_id: String::from("DRILL"),
                level: 3,
                amount: 1,
            }]],
            ..cdda_content::RecipeDefinition::default()
        };
        snapshot.controlled_actor.inventory = vec![item(7, "cordless_drill")];
        snapshot.controlled_actor.inventory[0].charges = 4;
        assert!(
            client_craft_support_items(&snapshot, &recipes, &items, &charged_quality_recipe, None,)
                .is_none()
        );
        snapshot.controlled_actor.inventory[0].charges = 5;
        assert!(
            client_craft_support_items(&snapshot, &recipes, &items, &charged_quality_recipe, None,)
                .is_some(),
            "charged quality should require its pinned per-use threshold"
        );

        let gated = recipes
            .available()
            .find(|recipe| recipe.difficulty > 0)
            .expect("expanded catalog should contain a skill-gated recipe");
        assert!(!client_recipe_skills_allow(
            &snapshot.controlled_actor,
            &items,
            gated
        ));
        let skill_ids = gated
            .resolved_autolearn_skills()
            .into_keys()
            .chain(gated.skills_required.keys().cloned())
            .chain((!gated.skill_used.is_empty()).then_some(gated.skill_used.clone()))
            .collect::<BTreeSet<_>>();
        snapshot.controlled_actor.skills = skill_ids
            .into_iter()
            .map(|skill_id| cdda_protocol::SkillLevelSnapshot {
                skill_id,
                practical_level: cdda_protocol::MAX_SKILL_LEVEL,
                practical_experience: 0,
                theoretical_level: cdda_protocol::MAX_SKILL_LEVEL,
                theoretical_experience: 0,
                last_practiced: tick,
            })
            .collect();
        assert!(client_recipe_skills_allow(
            &snapshot.controlled_actor,
            &items,
            gated
        ));

        let cottage_cheese = recipes
            .get("cottage_cheese")
            .expect("pinned book-only recipe exists");
        let book_type_id = cottage_cheese
            .book_learn
            .keys()
            .next()
            .expect("pinned recipe has a book source")
            .clone();
        snapshot
            .controlled_actor
            .skills
            .retain(|skill| skill.skill_id != cottage_cheese.skill_used);
        snapshot
            .controlled_actor
            .skills
            .push(cdda_protocol::SkillLevelSnapshot {
                skill_id: cottage_cheese.skill_used.clone(),
                practical_level: cdda_protocol::MAX_SKILL_LEVEL,
                practical_experience: 0,
                theoretical_level: cdda_protocol::MAX_SKILL_LEVEL,
                theoretical_experience: 0,
                last_practiced: tick,
            });
        assert!(!client_recipe_knowledge_allows(
            &snapshot.controlled_actor,
            &items,
            cottage_cheese
        ));
        snapshot
            .controlled_actor
            .inventory
            .push(item(8, &book_type_id));
        assert!(client_recipe_knowledge_allows(
            &snapshot.controlled_actor,
            &items,
            cottage_cheese
        ));

        let proficiency_gated = cdda_content::RecipeDefinition {
            proficiencies: vec![cdda_content::RecipeProficiency {
                proficiency_id: String::from("prof_metalworking"),
                required: true,
                time_multiplier_millionths: None,
                skill_penalty_millionths: None,
                learning_time_multiplier_millionths: cdda_content::PROFICIENCY_MULTIPLIER_SCALE,
                max_experience_moves: None,
            }],
            ..cdda_content::RecipeDefinition::default()
        };
        assert!(!client_recipe_proficiencies_allow(
            &snapshot.controlled_actor,
            &proficiency_gated
        ));
        snapshot.controlled_actor.proficiencies = vec![cdda_protocol::ProficiencyLevelSnapshot {
            proficiency_id: String::from("prof_metalworking"),
            practiced_action_points: 14_400_000,
            practice_remainder_millionths: 0,
            learned: true,
        }];
        assert!(client_recipe_proficiencies_allow(
            &snapshot.controlled_actor,
            &proficiency_gated
        ));

        snapshot.controlled_actor.skills = skills
            .iter()
            .map(|(skill_id, _)| cdda_protocol::SkillLevelSnapshot {
                skill_id: skill_id.to_owned(),
                practical_level: cdda_protocol::MAX_SKILL_LEVEL,
                practical_experience: 0,
                theoretical_level: cdda_protocol::MAX_SKILL_LEVEL,
                theoretical_experience: 0,
                last_practiced: tick,
            })
            .collect();
        let charged = recipes
            .get("toasterpastry_with_toaster")
            .expect("charged-toaster recipe should be exposed");
        let mut counter = 20_u64;
        let mut charged_inventory = Vec::new();
        for group in recipes
            .resolved_components(charged)
            .expect("charged recipe components should resolve")
        {
            let component = &group[0];
            let definition = items
                .get(&component.type_id)
                .expect("component item should resolve");
            let instances = if definition.count_by_charges() {
                1
            } else {
                component.count
            };
            for _ in 0..instances {
                let mut carried = item(counter, &component.type_id);
                if definition.count_by_charges() {
                    carried.charges = i32::try_from(component.count).expect("bounded component");
                }
                charged_inventory.push(carried);
                counter += 1;
            }
        }
        let mut toaster = item(counter, "toaster");
        toaster.charges = 4;
        charged_inventory.push(toaster);
        snapshot.controlled_actor.inventory = charged_inventory;
        assert!(
            !craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == charged.id)
        );
        snapshot
            .controlled_actor
            .inventory
            .last_mut()
            .expect("toaster remains")
            .charges = 5;
        assert!(
            craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == charged.id)
        );

        let mut water = item(counter + 1, "water_clean");
        water.charges = 2;
        snapshot.controlled_actor.inventory = vec![
            water,
            item(counter + 2, "can_tomato"),
            item(counter + 3, "broccoli"),
            item(counter + 4, "zucchini_cut"),
        ];
        assert!(
            craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "V8"),
            "concrete alternatives expanded from pinned LIST requirements should be craftable"
        );
        snapshot.controlled_actor.inventory.pop();
        assert!(
            !craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "V8")
        );

        snapshot.controlled_actor.inventory = (0_u64..10)
            .map(|index| item(counter + 20 + index, "cardboard"))
            .collect();
        let mut drawing_tool = item(counter + 40, "black_pen");
        drawing_tool.charges = 5;
        snapshot.controlled_actor.inventory.push(drawing_tool);
        assert!(
            craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "deck_of_cards_deck_of_cards_makeshift"),
            "a concrete charged tool expanded from a pinned tool LIST should be usable"
        );
        snapshot
            .controlled_actor
            .inventory
            .last_mut()
            .expect("drawing tool remains")
            .charges = 4;
        assert!(
            !craft_menu_entries(&snapshot, &recipes, &items)
                .iter()
                .any(|entry| entry.recipe_id == "deck_of_cards_deck_of_cards_makeshift")
        );

        let mut menu = CraftMenu {
            open: true,
            entries: (0..12)
                .map(|index| CraftMenuEntry {
                    recipe_id: format!("recipe_{index}"),
                    label: format!("recipe {index}"),
                })
                .collect(),
            selected: 0,
            ..default()
        };
        menu.select_previous();
        assert_eq!(menu.selected, 11);
        assert_eq!(
            menu.display()
                .lines()
                .filter(|line| line.starts_with("> ") || line.starts_with("  "))
                .count(),
            9
        );
    }

    #[test]
    fn target_menu_filters_and_orders_authoritative_targets() {
        let position = cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 };
        let gun_id = ItemId::new(1, 40);
        let gun = ItemSnapshot {
            id: gun_id,
            type_id: String::from("test_revolver"),
            charges: 1,
            damage: 0,
            raw_damage: 0,
            fitted: false,
            variant: None,
            snippet: None,
            variables: std::collections::BTreeMap::new(),
            melee_damage_milli: std::collections::BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            temperature: None,
            ammunition_type: String::new(),
            ranged_weapon: Some(cdda_protocol::RangedWeaponSnapshot {
                ammunition_type: String::from("38"),
                ammunition_remaining: 6,
                ammunition_capacity: 6,
                range: 6,
                damage: 10,
                dispersion: 100,
                sound_volume: 0,
            }),
            component_provenance: None,
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: Default::default(),
        };
        let creature = |counter, x, hp| cdda_protocol::VisibleCreatureSnapshot {
            id: CreatureId::new(1, counter),
            type_id: String::from("mon_zombie"),
            position: cdda_protocol::WorldPosition { x, y: 0, z: 0 },
            hp,
            max_hp: 80,
        };
        let tick = cdda_protocol::SimTick(0);
        let snapshot = ReplicationSnapshotV1 {
            tick,
            calendar: cdda_protocol::CalendarSnapshot::at_tick(tick),
            natural_light: cdda_protocol::NaturalLightSnapshot::at_tick(tick),
            detail_vision_available: true,
            controlled_actor: cdda_protocol::ActorSnapshot {
                id: ActorId::new(1, 10),
                position,
                hp: 100,
                body_parts: vec![cdda_protocol::ActorBodyPartSnapshotV1 {
                    body_part_id: String::from("torso"),
                    current_hp: 100,
                    maximum_hp: 100,
                }],
                effects: Vec::new(),
                eoc_variables: std::collections::BTreeMap::new(),
                next_eoc_schedule_sequence: 0,
                scheduled_eocs: Vec::new(),
                inactive_recurring_eocs: Vec::new(),
                base_strength: 8,
                base_dexterity: 8,
                base_intelligence: 8,
                base_perception: 8,
                connected: true,
                last_command_sequence: cdda_protocol::CommandSequence(0),
                last_held_input_sequence: cdda_protocol::HeldInputSequence(0),
                held_movement: None,
                inventory: vec![gun],
                wielded: Some(gun_id),
                worn: Vec::new(),
                stored_kcal: 55_000,
                thirst: 0,
                sleepiness: 0,
                sleeping: false,
                sleep_intervals: 0,
                stamina: 8_500,
                maximum_stamina: 8_500,
                dodge_attempts_remaining: 1,
                speed: 100,
                action_points: 0,
                queued_actions: Vec::new(),
                craft_activity: None,
                read_activity: None,
                disassembly_activity: None,
                construction_activity: None,
                pending_interaction: None,
                missions: Vec::new(),
                learned_recipes: Vec::new(),
                skills: Vec::new(),
                proficiencies: Vec::new(),
                map_memory: Vec::new(),
            },
            visible_actors: vec![cdda_protocol::VisibleActorSnapshot {
                id: ActorId::new(1, 30),
                position: cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 },
                hp: 100,
                connected: true,
                sleeping: false,
            }],
            npcs: Vec::new(),
            mission_definitions: Vec::new(),
            creatures: vec![creature(21, 5, 80), creature(20, 1, 80), creature(22, 1, 0)],
            ground_items: Vec::new(),
            chunks: Vec::new(),
        };
        let targets = |action, snapshot: &ReplicationSnapshotV1| {
            target_menu_entries(action, snapshot, None)
                .into_iter()
                .map(|entry| entry.target)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            targets(TargetMenuAction::Melee, &snapshot),
            vec![
                TargetChoice::Creature(CreatureId::new(1, 20)),
                TargetChoice::Actor(ActorId::new(1, 30)),
            ]
        );
        assert_eq!(
            targets(TargetMenuAction::Shoot, &snapshot),
            vec![
                TargetChoice::Creature(CreatureId::new(1, 20)),
                TargetChoice::Actor(ActorId::new(1, 30)),
                TargetChoice::Creature(CreatureId::new(1, 21)),
            ]
        );

        let mut unloaded = snapshot;
        unloaded.controlled_actor.inventory[0]
            .ranged_weapon
            .as_mut()
            .expect("test gun has ranged data")
            .ammunition_remaining = 0;
        assert!(targets(TargetMenuAction::Shoot, &unloaded).is_empty());
    }

    #[test]
    fn terrain_menu_uses_authoritative_visible_interactions_and_diagonal_smashing() {
        let terrain = |terrain_id: &str,
                       open: &str,
                       close: &str,
                       currently_visible: bool,
                       flat: bool,
                       bash_target| {
            Some(cdda_protocol::ObservedTerrainSnapshot {
                terrain: cdda_protocol::TerrainTileSnapshot {
                    terrain_id: String::from(terrain_id),
                    move_cost: 0,
                    transparent: false,
                    flat,
                    open: String::from(open),
                    open_move_cost: (!open.is_empty()).then_some(2),
                    open_transparent: (!open.is_empty()).then_some(true),
                    open_flat: (!open.is_empty()).then_some(true),
                    close: String::from(close),
                    close_move_cost: (!close.is_empty()).then_some(0),
                    close_transparent: (!close.is_empty()).then_some(false),
                    close_flat: (!close.is_empty()).then_some(false),
                },
                furniture: None,
                bash_target,
                fields: Vec::new(),
                currently_visible,
            })
        };
        let mut tiles = vec![None; (cdda_protocol::SUBMAP_SIZE.pow(2)) as usize];
        let index = |x: usize, y: usize| y * cdda_protocol::SUBMAP_SIZE as usize + x;
        tiles[index(5, 4)] = terrain("t_door_c", "t_door_o", "", true, true, None);
        tiles[index(6, 5)] = terrain("t_door_c", "t_door_o", "", true, true, None);
        tiles[index(5, 6)] = terrain("t_door_c", "t_door_o", "", false, true, None);
        tiles[index(4, 5)] = terrain("t_door_o", "", "t_door_c", true, true, None);
        tiles[index(4, 4)] = terrain(
            "t_nonflat",
            "",
            "",
            true,
            false,
            Some(BashTargetKindV1::Terrain),
        );
        let tick = cdda_protocol::SimTick(0);
        let snapshot = ReplicationSnapshotV1 {
            tick,
            calendar: cdda_protocol::CalendarSnapshot::at_tick(tick),
            natural_light: cdda_protocol::NaturalLightSnapshot::at_tick(tick),
            detail_vision_available: true,
            controlled_actor: cdda_protocol::ActorSnapshot {
                id: ActorId::new(1, 10),
                position: cdda_protocol::WorldPosition { x: 5, y: 5, z: 0 },
                hp: 100,
                body_parts: vec![cdda_protocol::ActorBodyPartSnapshotV1 {
                    body_part_id: String::from("torso"),
                    current_hp: 100,
                    maximum_hp: 100,
                }],
                effects: Vec::new(),
                eoc_variables: std::collections::BTreeMap::new(),
                next_eoc_schedule_sequence: 0,
                scheduled_eocs: Vec::new(),
                inactive_recurring_eocs: Vec::new(),
                base_strength: 8,
                base_dexterity: 8,
                base_intelligence: 8,
                base_perception: 8,
                connected: true,
                last_command_sequence: cdda_protocol::CommandSequence(0),
                last_held_input_sequence: cdda_protocol::HeldInputSequence(0),
                held_movement: None,
                inventory: Vec::new(),
                wielded: None,
                worn: Vec::new(),
                stored_kcal: 55_000,
                thirst: 0,
                sleepiness: 0,
                sleeping: false,
                sleep_intervals: 0,
                stamina: 8_500,
                maximum_stamina: 8_500,
                dodge_attempts_remaining: 1,
                speed: 100,
                action_points: 0,
                queued_actions: Vec::new(),
                craft_activity: None,
                read_activity: None,
                disassembly_activity: None,
                construction_activity: None,
                pending_interaction: None,
                missions: Vec::new(),
                learned_recipes: Vec::new(),
                skills: Vec::new(),
                proficiencies: Vec::new(),
                map_memory: Vec::new(),
            },
            visible_actors: Vec::new(),
            npcs: Vec::new(),
            mission_definitions: Vec::new(),
            creatures: Vec::new(),
            ground_items: Vec::new(),
            chunks: vec![cdda_protocol::VisibleChunkSnapshot {
                coord: cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 },
                tiles,
            }],
        };
        let directions = |action| {
            terrain_menu_entries(action, &snapshot, None, None)
                .into_iter()
                .map(|entry| (entry.dx, entry.dy, entry.label))
                .collect::<Vec<_>>()
        };
        let open = directions(TerrainMenuAction::Open);
        assert_eq!(
            open.iter()
                .map(|(dx, dy, _label)| (*dx, *dy))
                .collect::<Vec<_>>(),
            vec![(0, -1), (1, 0)]
        );
        assert!(open[0].2.contains("north — t_door_c → t_door_o"));
        assert_eq!(
            directions(TerrainMenuAction::Close)
                .into_iter()
                .map(|(dx, dy, _label)| (dx, dy))
                .collect::<Vec<_>>(),
            vec![(-1, 0)]
        );
        assert_eq!(
            directions(TerrainMenuAction::Smash)
                .into_iter()
                .map(|(dx, dy, _label)| (dx, dy))
                .collect::<Vec<_>>(),
            vec![(-1, -1)]
        );
        assert_eq!(
            construction_target_menu_entries(&snapshot, &[], true, true)
                .into_iter()
                .map(|entry| entry.target)
                .collect::<Vec<_>>(),
            vec![
                cdda_protocol::WorldPosition { x: 5, y: 4, z: 0 },
                cdda_protocol::WorldPosition { x: 6, y: 5, z: 0 },
                cdda_protocol::WorldPosition { x: 4, y: 5, z: 0 },
            ],
            "construction targets must be current, flat, and furniture-free"
        );
        assert_eq!(
            construction_target_menu_entries(&snapshot, &[String::from("t_door_c")], false, false,)
                .into_iter()
                .map(|entry| entry.target)
                .collect::<Vec<_>>(),
            vec![
                cdda_protocol::WorldPosition { x: 5, y: 4, z: 0 },
                cdda_protocol::WorldPosition { x: 6, y: 5, z: 0 },
            ],
            "exact-prerequisite construction must use current terrain identity"
        );
    }
}
