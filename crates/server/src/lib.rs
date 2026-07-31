//! Bevy-free dedicated-server runtime boundaries.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cdda_net::FrameIoError;
pub use cdda_net::{
    load_or_create_secret_key, read_control_frame, read_snapshot_stream, write_control_frame,
    write_snapshot_stream,
};
use cdda_persistence::{
    AccountMutation, AccountPage, AccountRecord, AdminAccountCreation, AdminCharacterIdentity,
    CharacterTransfer, DatabaseBackupMetadata, JournalBatchV1, ModerationHistoryPage,
    PreparedReplayArchive, RecoveryCompaction, ReplayArchiveCursor, ReportPage, StoreError,
    WorldStore,
};
use cdda_protocol::{
    ADMIN_ALPN, AccountId, AccountKeyRejection, AccountKeyRequest, AccountKeyResponse, AccountRole,
    AccountStatus, ActorConnectionUpdateV1, ActorId, ActorSnapshot, AdminAccountSummary,
    AdminRejection, AdminRequest, AdminResponse, BashTargetKindV1, BookStudyV1, CalendarSnapshot,
    CharacterCreationStatsV1, CharacterRequest, CharacterSummary, ChatMessage, ChatRejection,
    ClientCommand, ClientDatagramV1, CommandKind, ConstructionRecipeV1, ContentIdentity,
    ControlMessage, CraftRecipeV1, DisassemblyRecipeV1, ENROLL_ALPN, EndpointBindingSummary,
    EndpointIdentity, EnrollmentAccepted, EnrollmentRejection, FrameError, GAME_ALPN,
    GameplayRejection, HeldInputSequence, HeldMovementUpdateSource, HeldMovementUpdateV1, ItemId,
    MAX_DATAGRAM_SIZE, ModerationKind, NaturalLightSnapshot, ObservedTerrainSnapshot,
    PROTOCOL_VERSION, PlayerReport, PrivateCharacterInspection, REQUIRED_DATAGRAM_SIZE,
    ReplicationSnapshotV1, ReportId, ReportRejection, ReportResponse, ReportState, ReportSummary,
    ServerHello, SimTick, VisibleActorSnapshot, VisibleChunkSnapshot, VisibleCreatureSnapshot,
    VisibleNpcSnapshotV1, VisibleVehicleSnapshotV1, VisibleVehicleTileV1, WorldEvent,
    WorldEventKind, WorldPosition, WorldSnapshotV1, actor_body_part_summary_hp,
    decode_client_datagram, encode_control,
};
use cdda_sim::{ActorSpawn, ReservedIdBlock, TickOutcome, WorldState};
use iroh::{
    Endpoint, EndpointId, SecretKey,
    endpoint::{Connection, presets},
};

mod npc_faction;

pub const INPUT_QUEUE_CAPACITY: usize = 4_096;
pub const OUTPUT_QUEUE_CAPACITY: usize = 256;
pub const SIMULATION_INTERVAL: Duration = Duration::from_millis(50);
pub const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_GAMEPLAY_SESSIONS: usize = 16;
pub const MAX_CONNECTION_TASKS: usize = 64;
pub const CHARACTER_CREATION_QUEUE_CAPACITY: usize = 64;
pub const COMMITTED_EVENT_BATCH_CAPACITY: usize = 256;
pub const CHAT_MESSAGE_CAPACITY: usize = 128;
pub const PERSISTENCE_QUEUE_CAPACITY: usize = 64;
pub const PERSISTENCE_BYTE_CAPACITY: usize = 32 * 1024 * 1024;
pub const PERSISTENCE_CALL_TIMEOUT: Duration = Duration::from_secs(5);
pub const PERSISTENCE_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(30);
pub const CURRENT_INTEREST_RADIUS_SUBMAPS: u32 = 5;
pub const CURRENT_VISION_RADIUS_TILES: u32 = cdda_sim::TERRAIN_MEMORY_RADIUS_TILES;
pub const INBOUND_TRAFFIC_TIMEOUT: Duration = Duration::from_secs(15);
pub const ADMIN_CONNECTION_LIFETIME: Duration = Duration::from_secs(5 * 60);
const CONTROL_MESSAGES_PER_SECOND: u128 = 40;
const CONTROL_MESSAGE_BURST: u128 = 80;
const DATAGRAMS_PER_SECOND: u128 = 60;
const DATAGRAM_BURST: u128 = 120;
const HELD_INPUT_LEASE: Duration = Duration::from_millis(250);
const RATE_LIMIT_VIOLATIONS_BEFORE_CLOSE: u8 = 3;
const TOKEN_SCALE: u128 = 1_000_000_000;

#[derive(Clone, Default)]
pub struct CraftingCatalog {
    recipes: Arc<BTreeMap<String, CraftRecipeV1>>,
}

impl CraftingCatalog {
    #[must_use]
    pub fn new(recipes: BTreeMap<String, CraftRecipeV1>) -> Self {
        Self {
            recipes: Arc::new(recipes),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    #[must_use]
    pub fn get(&self, recipe_id: &str) -> Option<&CraftRecipeV1> {
        self.recipes.get(recipe_id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &CraftRecipeV1)> {
        self.recipes
            .iter()
            .map(|(recipe_id, recipe)| (recipe_id.as_str(), recipe))
    }

    fn normalize(&self, command: &mut ClientCommand) {
        if let CommandKind::Craft { recipe_id, recipe } = &mut command.kind {
            *recipe = self.recipes.get(recipe_id).cloned().map(Box::new);
        }
    }
}

#[derive(Clone, Default)]
pub struct ReadingCatalog {
    books: Arc<BTreeMap<String, BookStudyV1>>,
}

impl ReadingCatalog {
    #[must_use]
    pub fn new(books: BTreeMap<String, BookStudyV1>) -> Self {
        Self {
            books: Arc::new(books),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.books.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    #[must_use]
    pub fn get(&self, book_type_id: &str) -> Option<&BookStudyV1> {
        self.books.get(book_type_id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &BookStudyV1)> {
        self.books
            .iter()
            .map(|(book_type_id, study)| (book_type_id.as_str(), study))
    }

    fn normalize(&self, command: &mut ClientCommand) {
        if let CommandKind::ReadBook {
            book_type_id,
            study,
            ..
        } = &mut command.kind
        {
            *study = self.books.get(book_type_id).cloned().map(Box::new);
        }
    }
}

#[derive(Clone, Default)]
pub struct DisassemblyCatalog {
    recipes: Arc<BTreeMap<String, DisassemblyRecipeV1>>,
}

impl DisassemblyCatalog {
    #[must_use]
    pub fn new(recipes: BTreeMap<String, DisassemblyRecipeV1>) -> Self {
        Self {
            recipes: Arc::new(recipes),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    #[must_use]
    pub fn get(&self, item_type_id: &str) -> Option<&DisassemblyRecipeV1> {
        self.recipes.get(item_type_id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &DisassemblyRecipeV1)> {
        self.recipes
            .iter()
            .map(|(item_type_id, recipe)| (item_type_id.as_str(), recipe))
    }

    fn normalize(&self, command: &mut ClientCommand) {
        if let CommandKind::Disassemble {
            item_type_id,
            recipe,
            ..
        } = &mut command.kind
        {
            *recipe = self.recipes.get(item_type_id).cloned().map(Box::new);
        }
    }
}

#[derive(Clone, Default)]
pub struct ConstructionCatalog {
    recipes: Arc<BTreeMap<String, ConstructionRecipeV1>>,
}

impl ConstructionCatalog {
    #[must_use]
    pub fn new(recipes: BTreeMap<String, ConstructionRecipeV1>) -> Self {
        Self {
            recipes: Arc::new(recipes),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    #[must_use]
    pub fn get(&self, construction_id: &str) -> Option<&ConstructionRecipeV1> {
        self.recipes.get(construction_id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ConstructionRecipeV1)> {
        self.recipes
            .iter()
            .map(|(construction_id, recipe)| (construction_id.as_str(), recipe))
    }

    fn normalize(&self, command: &mut ClientCommand) {
        if let CommandKind::Construct {
            construction_id,
            construction,
            ..
        } = &mut command.kind
        {
            *construction = self.recipes.get(construction_id).cloned().map(Box::new);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedEventBatch {
    pub tick: SimTick,
    pub events: Vec<WorldEvent>,
}

#[derive(Clone)]
pub struct CommittedEventHub {
    sender: tokio::sync::broadcast::Sender<CommittedEventBatch>,
}

impl Default for CommittedEventHub {
    fn default() -> Self {
        let (sender, _receiver) = tokio::sync::broadcast::channel(COMMITTED_EVENT_BATCH_CAPACITY);
        Self { sender }
    }
}

impl CommittedEventHub {
    pub fn publish(&self, batch: CommittedEventBatch) {
        let _subscriber_count = self.sender.send(batch);
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<CommittedEventBatch> {
        self.sender.subscribe()
    }
}

#[derive(Clone)]
pub struct ChatHub {
    sender: tokio::sync::broadcast::Sender<ChatMessage>,
}

impl Default for ChatHub {
    fn default() -> Self {
        let (sender, _receiver) = tokio::sync::broadcast::channel(CHAT_MESSAGE_CAPACITY);
        Self { sender }
    }
}

impl ChatHub {
    fn publish(&self, message: ChatMessage) {
        let _subscriber_count = self.sender.send(message);
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ChatMessage> {
        self.sender.subscribe()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorizationChange {
    pub account_id: AccountId,
    pub endpoint: Option<EndpointIdentity>,
}

#[derive(Clone)]
pub struct AuthorizationChangeHub {
    sender: tokio::sync::broadcast::Sender<AuthorizationChange>,
}

impl Default for AuthorizationChangeHub {
    fn default() -> Self {
        let (sender, _receiver) = tokio::sync::broadcast::channel(128);
        Self { sender }
    }
}

impl AuthorizationChangeHub {
    pub fn publish_account(&self, account_id: AccountId) {
        let _subscriber_count = self.sender.send(AuthorizationChange {
            account_id,
            endpoint: None,
        });
    }

    pub fn publish_endpoint(&self, account_id: AccountId, endpoint: EndpointIdentity) {
        let _subscriber_count = self.sender.send(AuthorizationChange {
            account_id,
            endpoint: Some(endpoint),
        });
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AuthorizationChange> {
        self.sender.subscribe()
    }
}

enum PersistenceRequest {
    AppendJournal {
        batch: JournalBatchV1,
        committed_utc_seconds: i64,
        response: SyncSender<Result<u64, StoreError>>,
    },
    WriteSnapshot {
        sequence: u64,
        snapshot: Box<WorldSnapshotV1>,
        response: SyncSender<Result<(), StoreError>>,
    },
    ReserveIdBlock(SyncSender<Result<ReservedIdBlock, StoreError>>),
    Checkpoint(SyncSender<Result<(), StoreError>>),
    FinishRuntime {
        now_utc_seconds: i64,
        response: SyncSender<Result<(), StoreError>>,
    },
    EnrollEndpoint {
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountRecord, StoreError>>,
    },
    AuthorizeEndpoint {
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountRecord, StoreError>>,
    },
    AuthorizeAdminEndpoint {
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountRecord, StoreError>>,
    },
    AuditInvalidAdminMessage {
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<(), StoreError>>,
    },
    AuditRateLimitedAdminRequest {
        endpoint: EndpointIdentity,
        request: AdminRequest,
        now_utc_seconds: i64,
        response: SyncSender<Result<(), StoreError>>,
    },
    AdminAccounts {
        actor_endpoint: EndpointIdentity,
        after: Option<AccountId>,
        limit: u16,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountPage, StoreError>>,
    },
    AdminCharacters {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
        response: SyncSender<Result<Vec<CharacterSummary>, StoreError>>,
    },
    AdminPrivateCharacter {
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        inventory_after: Option<ItemId>,
        inventory_limit: u16,
        now_utc_seconds: i64,
        response: SyncSender<Result<AdminCharacterIdentity, StoreError>>,
    },
    AdminReports {
        actor_endpoint: EndpointIdentity,
        state: Option<ReportState>,
        after: Option<ReportId>,
        limit: u16,
        now_utc_seconds: i64,
        response: SyncSender<Result<ReportPage, StoreError>>,
    },
    SetReportState {
        actor_endpoint: EndpointIdentity,
        report_id: ReportId,
        state: ReportState,
        now_utc_seconds: i64,
        response: SyncSender<Result<ReportSummary, StoreError>>,
    },
    AdminCreateAccount {
        actor_endpoint: EndpointIdentity,
        display_name: String,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<AdminAccountCreation, StoreError>>,
    },
    AdminEndpointBindings {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
        response: SyncSender<Result<Vec<EndpointBindingSummary>, StoreError>>,
    },
    AdminAddEndpoint {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<EndpointBindingSummary, StoreError>>,
    },
    AdminRevokeEndpoint {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<(), StoreError>>,
    },
    AdminModerationHistory {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        after: Option<u64>,
        limit: u16,
        now_utc_seconds: i64,
        response: SyncSender<Result<ModerationHistoryPage, StoreError>>,
    },
    SetAccountRole {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        role: AccountRole,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountMutation, StoreError>>,
    },
    SetAccountStatus {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        status: AccountStatus,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountMutation, StoreError>>,
    },
    KickAccount {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountRecord, StoreError>>,
    },
    SetAccountSuspension {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountMutation, StoreError>>,
    },
    SetAccountMute {
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
        response: SyncSender<Result<AccountMutation, StoreError>>,
    },
    TransferCharacter {
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        new_owner: AccountId,
        now_utc_seconds: i64,
        response: SyncSender<Result<CharacterTransfer, StoreError>>,
    },
    AuthorizeChat {
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<(), StoreError>>,
    },
    SubmitReport {
        reporter_account: AccountId,
        reporter_endpoint: EndpointIdentity,
        reporter_actor: ActorId,
        report: PlayerReport,
        now_utc_seconds: i64,
        response: SyncSender<Result<ReportId, StoreError>>,
    },
    EndpointBindings {
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<Vec<EndpointBindingSummary>, StoreError>>,
    },
    AddPendingEndpoint {
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<EndpointBindingSummary, StoreError>>,
    },
    RevokeEndpoint {
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<(), StoreError>>,
    },
    RecoverAccountEndpoint {
        account_id: AccountId,
        replacement: EndpointIdentity,
        now_utc_seconds: i64,
        response: SyncSender<Result<EndpointBindingSummary, StoreError>>,
    },
    CharactersForAccount {
        account_id: AccountId,
        response: SyncSender<Result<Vec<CharacterSummary>, StoreError>>,
    },
    CreateCharacter {
        account_id: AccountId,
        name: String,
        created_tick: SimTick,
        created_after_journal_sequence: u64,
        actor: Box<ActorSnapshot>,
        response: SyncSender<Result<CharacterSummary, StoreError>>,
    },
    JournalAfter {
        sequence: u64,
        response: SyncSender<Result<Vec<(u64, JournalBatchV1)>, StoreError>>,
    },
    LatestSnapshot(SyncSender<Result<Option<(u64, WorldState)>, StoreError>>),
    ReplayArchiveCursor(SyncSender<Result<ReplayArchiveCursor, StoreError>>),
    PrepareReplayArchive {
        end_journal_sequence: u64,
        now_utc_seconds: i64,
        content: ContentIdentity,
        response: SyncSender<Result<Option<PreparedReplayArchive>, StoreError>>,
    },
    CommitReplayArchive {
        start: ReplayArchiveCursor,
        end: ReplayArchiveCursor,
        response: SyncSender<Result<(), StoreError>>,
    },
    CompactRecoveryHistory {
        now_utc_seconds: i64,
        response: SyncSender<Result<Option<RecoveryCompaction>, StoreError>>,
    },
    Shutdown,
}

impl PersistenceRequest {
    fn queued_bytes(&self) -> Result<usize, StoreError> {
        match self {
            Self::AppendJournal { batch, .. } => Ok(postcard::to_stdvec(batch)
                .map_err(StoreError::Postcard)?
                .len()),
            Self::WriteSnapshot { snapshot, .. } => Ok(postcard::to_stdvec(snapshot)
                .map_err(StoreError::Postcard)?
                .len()),
            Self::CreateCharacter { name, actor, .. } => postcard::to_stdvec(actor)
                .map_err(StoreError::Postcard)?
                .len()
                .checked_add(name.len())
                .ok_or(StoreError::NumericOverflow),
            Self::AdminCreateAccount { display_name, .. } => Ok(display_name.len().max(1)),
            Self::SubmitReport { report, .. } => Ok(report.details.len().max(1)),
            Self::PrepareReplayArchive { content, .. } => {
                let mod_bytes = content
                    .enabled_mods
                    .iter()
                    .try_fold(0_usize, |total, entry| {
                        total
                            .checked_add(entry.len())
                            .ok_or(StoreError::NumericOverflow)
                    })?;
                content
                    .baseline_commit
                    .len()
                    .checked_add(mod_bytes)
                    .and_then(|bytes| bytes.checked_add(content.manifest_hash.len()))
                    .ok_or(StoreError::NumericOverflow)
            }
            Self::ReserveIdBlock(_)
            | Self::Checkpoint(_)
            | Self::FinishRuntime { .. }
            | Self::EnrollEndpoint { .. }
            | Self::AuthorizeEndpoint { .. }
            | Self::EndpointBindings { .. }
            | Self::AddPendingEndpoint { .. }
            | Self::RevokeEndpoint { .. }
            | Self::AuthorizeAdminEndpoint { .. }
            | Self::AuditInvalidAdminMessage { .. }
            | Self::AuditRateLimitedAdminRequest { .. }
            | Self::AdminAccounts { .. }
            | Self::AdminCharacters { .. }
            | Self::AdminPrivateCharacter { .. }
            | Self::AdminReports { .. }
            | Self::SetReportState { .. }
            | Self::AdminEndpointBindings { .. }
            | Self::AdminAddEndpoint { .. }
            | Self::AdminRevokeEndpoint { .. }
            | Self::AdminModerationHistory { .. }
            | Self::SetAccountRole { .. }
            | Self::SetAccountStatus { .. }
            | Self::KickAccount { .. }
            | Self::SetAccountSuspension { .. }
            | Self::SetAccountMute { .. }
            | Self::TransferCharacter { .. }
            | Self::AuthorizeChat { .. }
            | Self::RecoverAccountEndpoint { .. }
            | Self::CharactersForAccount { .. }
            | Self::JournalAfter { .. }
            | Self::LatestSnapshot(_)
            | Self::ReplayArchiveCursor(_)
            | Self::CommitReplayArchive { .. }
            | Self::CompactRecoveryHistory { .. }
            | Self::Shutdown => Ok(1),
        }
    }
}

#[derive(Default)]
struct PersistenceBudget {
    used: Mutex<usize>,
}

impl PersistenceBudget {
    fn try_reserve(self: &Arc<Self>, bytes: usize) -> Result<PersistencePermit, StoreError> {
        let mut used = lock_unpoisoned(&self.used);
        let next = used.checked_add(bytes).ok_or(StoreError::NumericOverflow)?;
        if next > PERSISTENCE_BYTE_CAPACITY {
            return Err(StoreError::PersistenceBusy);
        }
        *used = next;
        Ok(PersistencePermit {
            budget: Arc::clone(self),
            bytes,
        })
    }

    #[cfg(test)]
    fn used(&self) -> usize {
        *lock_unpoisoned(&self.used)
    }
}

struct PersistencePermit {
    budget: Arc<PersistenceBudget>,
    bytes: usize,
}

impl PersistencePermit {
    fn resize(&mut self, bytes: usize) -> Result<(), StoreError> {
        let mut used = lock_unpoisoned(&self.budget.used);
        let without_self = used
            .checked_sub(self.bytes)
            .ok_or(StoreError::NumericOverflow)?;
        let next = without_self
            .checked_add(bytes)
            .ok_or(StoreError::NumericOverflow)?;
        if next > PERSISTENCE_BYTE_CAPACITY {
            return Err(StoreError::PersistenceBusy);
        }
        *used = next;
        self.bytes = bytes;
        Ok(())
    }
}

impl Drop for PersistencePermit {
    fn drop(&mut self) {
        let mut used = lock_unpoisoned(&self.budget.used);
        *used = used.saturating_sub(self.bytes);
    }
}

struct PersistenceEnvelope {
    request: PersistenceRequest,
    _permit: Option<PersistencePermit>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotWriteOutcome {
    Written,
    Superseded,
}

struct SnapshotJob {
    sequence: u64,
    snapshot: WorldSnapshotV1,
    response: SyncSender<Result<SnapshotWriteOutcome, StoreError>>,
    _permit: PersistencePermit,
}

#[derive(Default)]
struct SnapshotSlot {
    pending: Mutex<Option<SnapshotJob>>,
}

pub struct SnapshotReceipt {
    response: Receiver<Result<SnapshotWriteOutcome, StoreError>>,
}

impl SnapshotReceipt {
    pub fn try_result(&self) -> Result<Option<SnapshotWriteOutcome>, StoreError> {
        match self.response.try_recv() {
            Ok(result) => result.map(Some),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(StoreError::PersistenceUnavailable),
        }
    }

    pub fn wait(self) -> Result<SnapshotWriteOutcome, StoreError> {
        self.response
            .recv_timeout(PERSISTENCE_SNAPSHOT_TIMEOUT)
            .map_err(|error| match error {
                RecvTimeoutError::Timeout => StoreError::PersistenceTimeout,
                RecvTimeoutError::Disconnected => StoreError::PersistenceUnavailable,
            })?
    }
}

pub struct PersistenceHost {
    handle: PersistenceHandle,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone)]
pub struct PersistenceHandle {
    requests: SyncSender<PersistenceEnvelope>,
    snapshots: Arc<SnapshotSlot>,
    budget: Arc<PersistenceBudget>,
    database_path: Option<PathBuf>,
}

impl PersistenceHost {
    pub fn start(mut store: WorldStore) -> Result<Self, std::io::Error> {
        let database_path = store
            .database_path()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let (requests, receiver) = mpsc::sync_channel(PERSISTENCE_QUEUE_CAPACITY);
        let snapshots = Arc::new(SnapshotSlot::default());
        let worker_snapshots = Arc::clone(&snapshots);
        let budget = Arc::new(PersistenceBudget::default());
        let thread = thread::Builder::new()
            .name(String::from("cdda-persistence"))
            .spawn(move || {
                loop {
                    let envelope = match receiver.recv_timeout(Duration::from_millis(10)) {
                        Ok(envelope) => Some(envelope),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    let mut shutdown = false;
                    if let Some(PersistenceEnvelope { request, _permit }) = envelope {
                        match request {
                            PersistenceRequest::AppendJournal {
                                batch,
                                committed_utc_seconds,
                                response,
                            } => {
                                let result =
                                    store.append_journal_batch_at(&batch, committed_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::WriteSnapshot {
                                sequence,
                                snapshot,
                                response,
                            } => {
                                let result = WorldState::from_snapshot(&snapshot)
                                    .map_err(StoreError::Simulation)
                                    .and_then(|world| store.write_snapshot(sequence, &world));
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::ReserveIdBlock(response) => {
                                let result = store.reserve_id_block();
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::Checkpoint(response) => {
                                let result = store.checkpoint();
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::FinishRuntime {
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.finish_runtime(now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::EnrollEndpoint {
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.enroll_endpoint(endpoint, now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AuthorizeEndpoint {
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.authorize_endpoint(endpoint, now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AuthorizeAdminEndpoint {
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result =
                                    store.authorize_admin_endpoint(endpoint, now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AuditInvalidAdminMessage {
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result =
                                    store.audit_invalid_admin_message(endpoint, now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AuditRateLimitedAdminRequest {
                                endpoint,
                                request,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.audit_rate_limited_admin_request(
                                    endpoint,
                                    request,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminAccounts {
                                actor_endpoint,
                                after,
                                limit,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_accounts(
                                    actor_endpoint,
                                    after,
                                    limit,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminCharacters {
                                actor_endpoint,
                                account_id,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_characters(
                                    actor_endpoint,
                                    account_id,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminPrivateCharacter {
                                actor_endpoint,
                                actor_id,
                                inventory_after,
                                inventory_limit,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_private_character(
                                    actor_endpoint,
                                    actor_id,
                                    inventory_after,
                                    inventory_limit,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminReports {
                                actor_endpoint,
                                state,
                                after,
                                limit,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_reports(
                                    actor_endpoint,
                                    state,
                                    after,
                                    limit,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::SetReportState {
                                actor_endpoint,
                                report_id,
                                state,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.set_report_state(
                                    actor_endpoint,
                                    report_id,
                                    state,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminCreateAccount {
                                actor_endpoint,
                                display_name,
                                role,
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = if EndpointId::from_bytes(&endpoint.0).is_ok() {
                                    store.admin_create_account(
                                        actor_endpoint,
                                        &display_name,
                                        role,
                                        endpoint,
                                        now_utc_seconds,
                                    )
                                } else {
                                    match store.audit_invalid_admin_account_create(
                                        actor_endpoint,
                                        role,
                                        endpoint,
                                        now_utc_seconds,
                                    ) {
                                        Ok(()) => Err(StoreError::InvalidEndpointIdentity),
                                        Err(error) => Err(error),
                                    }
                                };
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminEndpointBindings {
                                actor_endpoint,
                                account_id,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_endpoint_bindings(
                                    actor_endpoint,
                                    account_id,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminAddEndpoint {
                                actor_endpoint,
                                account_id,
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = if EndpointId::from_bytes(&endpoint.0).is_ok() {
                                    store.admin_add_pending_endpoint(
                                        actor_endpoint,
                                        account_id,
                                        endpoint,
                                        now_utc_seconds,
                                    )
                                } else {
                                    match store.audit_invalid_admin_endpoint_add(
                                        actor_endpoint,
                                        account_id,
                                        endpoint,
                                        now_utc_seconds,
                                    ) {
                                        Ok(()) => Err(StoreError::InvalidEndpointIdentity),
                                        Err(error) => Err(error),
                                    }
                                };
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminRevokeEndpoint {
                                actor_endpoint,
                                account_id,
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_revoke_endpoint(
                                    actor_endpoint,
                                    account_id,
                                    endpoint,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AdminModerationHistory {
                                actor_endpoint,
                                account_id,
                                after,
                                limit,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.admin_moderation_history(
                                    actor_endpoint,
                                    account_id,
                                    after,
                                    limit,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::SetAccountRole {
                                actor_endpoint,
                                account_id,
                                role,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.set_account_role(
                                    actor_endpoint,
                                    account_id,
                                    role,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::SetAccountStatus {
                                actor_endpoint,
                                account_id,
                                status,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.set_account_status(
                                    actor_endpoint,
                                    account_id,
                                    status,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::KickAccount {
                                actor_endpoint,
                                account_id,
                                now_utc_seconds,
                                response,
                            } => {
                                let result =
                                    store.kick_account(actor_endpoint, account_id, now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::SetAccountSuspension {
                                actor_endpoint,
                                account_id,
                                duration_seconds,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.set_account_suspension(
                                    actor_endpoint,
                                    account_id,
                                    duration_seconds,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::SetAccountMute {
                                actor_endpoint,
                                account_id,
                                duration_seconds,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.set_account_mute(
                                    actor_endpoint,
                                    account_id,
                                    duration_seconds,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::TransferCharacter {
                                actor_endpoint,
                                actor_id,
                                new_owner,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.transfer_character(
                                    actor_endpoint,
                                    actor_id,
                                    new_owner,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AuthorizeChat {
                                account_id,
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result =
                                    store.authorize_chat(account_id, endpoint, now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::SubmitReport {
                                reporter_account,
                                reporter_endpoint,
                                reporter_actor,
                                report,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.submit_report(
                                    reporter_account,
                                    reporter_endpoint,
                                    reporter_actor,
                                    report.target_actor,
                                    report.reason,
                                    &report.details,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::EndpointBindings {
                                account_id,
                                actor_endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.audited_endpoint_bindings(
                                    account_id,
                                    actor_endpoint,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::AddPendingEndpoint {
                                account_id,
                                actor_endpoint,
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = if EndpointId::from_bytes(&endpoint.0).is_ok() {
                                    store.add_pending_endpoint(
                                        account_id,
                                        actor_endpoint,
                                        endpoint,
                                        now_utc_seconds,
                                    )
                                } else {
                                    match store.audit_invalid_endpoint_add(
                                        account_id,
                                        actor_endpoint,
                                        endpoint,
                                        now_utc_seconds,
                                    ) {
                                        Ok(()) => Err(StoreError::InvalidEndpointIdentity),
                                        Err(error) => Err(error),
                                    }
                                };
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::RevokeEndpoint {
                                account_id,
                                actor_endpoint,
                                endpoint,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.revoke_endpoint(
                                    account_id,
                                    actor_endpoint,
                                    endpoint,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::RecoverAccountEndpoint {
                                account_id,
                                replacement,
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.recover_account_endpoint(
                                    account_id,
                                    replacement,
                                    now_utc_seconds,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::CharactersForAccount {
                                account_id,
                                response,
                            } => {
                                let result = store.characters_for_account(account_id);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::CreateCharacter {
                                account_id,
                                name,
                                created_tick,
                                created_after_journal_sequence,
                                actor,
                                response,
                            } => {
                                let result = store.create_character(
                                    account_id,
                                    &name,
                                    created_tick,
                                    created_after_journal_sequence,
                                    &actor,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::JournalAfter { sequence, response } => {
                                let result = store.journal_after(sequence);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::LatestSnapshot(response) => {
                                let result = store.latest_snapshot();
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::ReplayArchiveCursor(response) => {
                                let result = store.replay_archive_cursor();
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::PrepareReplayArchive {
                                end_journal_sequence,
                                now_utc_seconds,
                                content,
                                response,
                            } => {
                                let result = store.prepare_replay_archive(
                                    end_journal_sequence,
                                    now_utc_seconds,
                                    content,
                                );
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::CommitReplayArchive {
                                start,
                                end,
                                response,
                            } => {
                                let result = store.commit_replay_archive(start, end);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::CompactRecoveryHistory {
                                now_utc_seconds,
                                response,
                            } => {
                                let result = store.compact_recovery_history(now_utc_seconds);
                                let _send_result = response.try_send(result);
                            }
                            PersistenceRequest::Shutdown => shutdown = true,
                        }
                    }
                    if shutdown {
                        let pending_snapshot = {
                            let mut pending = lock_unpoisoned(&worker_snapshots.pending);
                            pending.take()
                        };
                        if let Some(job) = pending_snapshot {
                            let _send_result =
                                job.response.try_send(Ok(SnapshotWriteOutcome::Superseded));
                        }
                        break;
                    }
                    let pending_snapshot = {
                        let mut pending = lock_unpoisoned(&worker_snapshots.pending);
                        pending.take()
                    };
                    if let Some(job) = pending_snapshot {
                        let result = WorldState::from_snapshot(&job.snapshot)
                            .map_err(StoreError::Simulation)
                            .and_then(|world| store.write_snapshot(job.sequence, &world))
                            .map(|()| SnapshotWriteOutcome::Written);
                        let _send_result = job.response.try_send(result);
                    }
                }
            })?;
        Ok(Self {
            handle: PersistenceHandle {
                requests,
                snapshots,
                budget,
                database_path,
            },
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn handle(&self) -> PersistenceHandle {
        self.handle.clone()
    }

    pub fn shutdown(mut self) {
        let _send_result = self.handle.requests.send(PersistenceEnvelope {
            request: PersistenceRequest::Shutdown,
            _permit: None,
        });
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
    }
}

impl Drop for PersistenceHost {
    fn drop(&mut self) {
        // A full queue must not make Drop join a worker that was never told to
        // stop. The worker continuously drains this bounded channel, so the
        // blocking send establishes an eventual shutdown request.
        let _send_result = self.handle.requests.send(PersistenceEnvelope {
            request: PersistenceRequest::Shutdown,
            _permit: None,
        });
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
    }
}

impl PersistenceHandle {
    fn request<T>(
        &self,
        request: impl FnOnce(SyncSender<Result<T, StoreError>>) -> PersistenceRequest,
    ) -> Result<T, StoreError> {
        self.request_with_timeout(PERSISTENCE_CALL_TIMEOUT, request)
    }

    fn request_with_timeout<T>(
        &self,
        timeout: Duration,
        request: impl FnOnce(SyncSender<Result<T, StoreError>>) -> PersistenceRequest,
    ) -> Result<T, StoreError> {
        let (response, receive) = mpsc::sync_channel(1);
        let request = request(response);
        let permit = self.budget.try_reserve(request.queued_bytes()?)?;
        self.requests
            .try_send(PersistenceEnvelope {
                request,
                _permit: Some(permit),
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => StoreError::PersistenceBusy,
                TrySendError::Disconnected(_) => StoreError::PersistenceUnavailable,
            })?;
        receive.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => StoreError::PersistenceTimeout,
            RecvTimeoutError::Disconnected => StoreError::PersistenceUnavailable,
        })?
    }

    pub fn append_journal_batch_at(
        &self,
        batch: JournalBatchV1,
        committed_utc_seconds: i64,
    ) -> Result<u64, StoreError> {
        self.request(|response| PersistenceRequest::AppendJournal {
            batch,
            committed_utc_seconds,
            response,
        })
    }

    pub fn write_snapshot(
        &self,
        sequence: u64,
        snapshot: WorldSnapshotV1,
    ) -> Result<(), StoreError> {
        self.request_with_timeout(PERSISTENCE_SNAPSHOT_TIMEOUT, |response| {
            PersistenceRequest::WriteSnapshot {
                sequence,
                snapshot: Box::new(snapshot),
                response,
            }
        })
    }

    pub fn queue_snapshot(
        &self,
        sequence: u64,
        snapshot: WorldSnapshotV1,
    ) -> Result<SnapshotReceipt, StoreError> {
        let bytes = postcard::to_stdvec(&snapshot)
            .map_err(StoreError::Postcard)?
            .len();
        let (response, receive) = mpsc::sync_channel(1);
        let mut pending = lock_unpoisoned(&self.snapshots.pending);
        let permit = if let Some(mut old) = pending.take() {
            if let Err(error) = old._permit.resize(bytes) {
                *pending = Some(old);
                return Err(error);
            }
            let _send_result = old.response.try_send(Ok(SnapshotWriteOutcome::Superseded));
            old._permit
        } else {
            self.budget.try_reserve(bytes)?
        };
        *pending = Some(SnapshotJob {
            sequence,
            snapshot,
            response,
            _permit: permit,
        });
        Ok(SnapshotReceipt { response: receive })
    }

    pub fn reserve_id_block(&self) -> Result<ReservedIdBlock, StoreError> {
        self.request(PersistenceRequest::ReserveIdBlock)
    }

    pub fn checkpoint(&self) -> Result<(), StoreError> {
        self.request(PersistenceRequest::Checkpoint)
    }

    pub fn finish_runtime(&self, now_utc_seconds: i64) -> Result<(), StoreError> {
        self.request(|response| PersistenceRequest::FinishRuntime {
            now_utc_seconds,
            response,
        })
    }

    pub fn enroll_endpoint(
        &self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        self.request(|response| PersistenceRequest::EnrollEndpoint {
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn authorize_endpoint(
        &self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        self.request(|response| PersistenceRequest::AuthorizeEndpoint {
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn authorize_admin_endpoint(
        &self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        self.request(|response| PersistenceRequest::AuthorizeAdminEndpoint {
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn audit_invalid_admin_message(
        &self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        self.request(|response| PersistenceRequest::AuditInvalidAdminMessage {
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn audit_rate_limited_admin_request(
        &self,
        endpoint: EndpointIdentity,
        request: AdminRequest,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        self.request(
            |response| PersistenceRequest::AuditRateLimitedAdminRequest {
                endpoint,
                request,
                now_utc_seconds,
                response,
            },
        )
    }

    pub fn admin_accounts(
        &self,
        actor_endpoint: EndpointIdentity,
        after: Option<AccountId>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<AccountPage, StoreError> {
        self.request(|response| PersistenceRequest::AdminAccounts {
            actor_endpoint,
            after,
            limit,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_characters(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<Vec<CharacterSummary>, StoreError> {
        self.request(|response| PersistenceRequest::AdminCharacters {
            actor_endpoint,
            account_id,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_private_character(
        &self,
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        inventory_after: Option<ItemId>,
        inventory_limit: u16,
        now_utc_seconds: i64,
    ) -> Result<AdminCharacterIdentity, StoreError> {
        self.request(|response| PersistenceRequest::AdminPrivateCharacter {
            actor_endpoint,
            actor_id,
            inventory_after,
            inventory_limit,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_reports(
        &self,
        actor_endpoint: EndpointIdentity,
        state: Option<ReportState>,
        after: Option<ReportId>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<ReportPage, StoreError> {
        self.request(|response| PersistenceRequest::AdminReports {
            actor_endpoint,
            state,
            after,
            limit,
            now_utc_seconds,
            response,
        })
    }

    pub fn set_report_state(
        &self,
        actor_endpoint: EndpointIdentity,
        report_id: ReportId,
        state: ReportState,
        now_utc_seconds: i64,
    ) -> Result<ReportSummary, StoreError> {
        self.request(|response| PersistenceRequest::SetReportState {
            actor_endpoint,
            report_id,
            state,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_create_account(
        &self,
        actor_endpoint: EndpointIdentity,
        display_name: String,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AdminAccountCreation, StoreError> {
        self.request(|response| PersistenceRequest::AdminCreateAccount {
            actor_endpoint,
            display_name,
            role,
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_endpoint_bindings(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<Vec<EndpointBindingSummary>, StoreError> {
        self.request(|response| PersistenceRequest::AdminEndpointBindings {
            actor_endpoint,
            account_id,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_add_pending_endpoint(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        self.request(|response| PersistenceRequest::AdminAddEndpoint {
            actor_endpoint,
            account_id,
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_revoke_endpoint(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        self.request(|response| PersistenceRequest::AdminRevokeEndpoint {
            actor_endpoint,
            account_id,
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn admin_moderation_history(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        after: Option<u64>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<ModerationHistoryPage, StoreError> {
        self.request(|response| PersistenceRequest::AdminModerationHistory {
            actor_endpoint,
            account_id,
            after,
            limit,
            now_utc_seconds,
            response,
        })
    }

    pub fn set_account_role(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        role: AccountRole,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        self.request(|response| PersistenceRequest::SetAccountRole {
            actor_endpoint,
            account_id,
            role,
            now_utc_seconds,
            response,
        })
    }

    pub fn set_account_status(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        status: AccountStatus,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        self.request(|response| PersistenceRequest::SetAccountStatus {
            actor_endpoint,
            account_id,
            status,
            now_utc_seconds,
            response,
        })
    }

    pub fn kick_account(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        self.request(|response| PersistenceRequest::KickAccount {
            actor_endpoint,
            account_id,
            now_utc_seconds,
            response,
        })
    }

    pub fn set_account_suspension(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        self.request(|response| PersistenceRequest::SetAccountSuspension {
            actor_endpoint,
            account_id,
            duration_seconds,
            now_utc_seconds,
            response,
        })
    }

    pub fn set_account_mute(
        &self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        self.request(|response| PersistenceRequest::SetAccountMute {
            actor_endpoint,
            account_id,
            duration_seconds,
            now_utc_seconds,
            response,
        })
    }

    pub fn transfer_character(
        &self,
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        new_owner: AccountId,
        now_utc_seconds: i64,
    ) -> Result<CharacterTransfer, StoreError> {
        self.request(|response| PersistenceRequest::TransferCharacter {
            actor_endpoint,
            actor_id,
            new_owner,
            now_utc_seconds,
            response,
        })
    }

    pub fn authorize_chat(
        &self,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        self.request(|response| PersistenceRequest::AuthorizeChat {
            account_id,
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn submit_report(
        &self,
        reporter_account: AccountId,
        reporter_endpoint: EndpointIdentity,
        reporter_actor: ActorId,
        report: PlayerReport,
        now_utc_seconds: i64,
    ) -> Result<ReportId, StoreError> {
        self.request(|response| PersistenceRequest::SubmitReport {
            reporter_account,
            reporter_endpoint,
            reporter_actor,
            report,
            now_utc_seconds,
            response,
        })
    }

    pub fn endpoint_bindings(
        &self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<Vec<EndpointBindingSummary>, StoreError> {
        self.request(|response| PersistenceRequest::EndpointBindings {
            account_id,
            actor_endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn add_pending_endpoint(
        &self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        self.request(|response| PersistenceRequest::AddPendingEndpoint {
            account_id,
            actor_endpoint,
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn revoke_endpoint(
        &self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        self.request(|response| PersistenceRequest::RevokeEndpoint {
            account_id,
            actor_endpoint,
            endpoint,
            now_utc_seconds,
            response,
        })
    }

    pub fn recover_account_endpoint(
        &self,
        account_id: AccountId,
        replacement: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        self.request(|response| PersistenceRequest::RecoverAccountEndpoint {
            account_id,
            replacement,
            now_utc_seconds,
            response,
        })
    }

    pub fn characters_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<CharacterSummary>, StoreError> {
        self.request(|response| PersistenceRequest::CharactersForAccount {
            account_id,
            response,
        })
    }

    pub fn create_character(
        &self,
        account_id: AccountId,
        name: String,
        created_tick: SimTick,
        created_after_journal_sequence: u64,
        actor: ActorSnapshot,
    ) -> Result<CharacterSummary, StoreError> {
        self.request(|response| PersistenceRequest::CreateCharacter {
            account_id,
            name,
            created_tick,
            created_after_journal_sequence,
            actor: Box::new(actor),
            response,
        })
    }

    pub fn journal_after(&self, sequence: u64) -> Result<Vec<(u64, JournalBatchV1)>, StoreError> {
        self.request(|response| PersistenceRequest::JournalAfter { sequence, response })
    }

    pub fn latest_snapshot(&self) -> Result<Option<(u64, WorldState)>, StoreError> {
        self.request(PersistenceRequest::LatestSnapshot)
    }

    pub fn replay_archive_cursor(&self) -> Result<ReplayArchiveCursor, StoreError> {
        self.request(PersistenceRequest::ReplayArchiveCursor)
    }

    pub fn prepare_replay_archive(
        &self,
        end_journal_sequence: u64,
        now_utc_seconds: i64,
        content: ContentIdentity,
    ) -> Result<Option<PreparedReplayArchive>, StoreError> {
        self.request(|response| PersistenceRequest::PrepareReplayArchive {
            end_journal_sequence,
            now_utc_seconds,
            content,
            response,
        })
    }

    pub fn commit_replay_archive(
        &self,
        start: ReplayArchiveCursor,
        end: ReplayArchiveCursor,
    ) -> Result<(), StoreError> {
        self.request(|response| PersistenceRequest::CommitReplayArchive {
            start,
            end,
            response,
        })
    }

    pub fn compact_recovery_history(
        &self,
        now_utc_seconds: i64,
    ) -> Result<Option<RecoveryCompaction>, StoreError> {
        self.request(|response| PersistenceRequest::CompactRecoveryHistory {
            now_utc_seconds,
            response,
        })
    }

    pub fn backup_to(&self, path: PathBuf) -> Result<DatabaseBackupMetadata, StoreError> {
        let source = self
            .database_path
            .as_ref()
            .ok_or(StoreError::InvalidRecord)?;
        WorldStore::backup_from_path(source, path)
    }
}

struct ControlIngressLimiter {
    available: u128,
    last_refill: Instant,
}

struct DatagramIngressLimiter {
    available: u128,
    last_refill: Instant,
}

impl DatagramIngressLimiter {
    fn new(now: Instant) -> Self {
        Self {
            available: DATAGRAM_BURST * TOKEN_SCALE,
            last_refill: now,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        let replenished = now
            .saturating_duration_since(self.last_refill)
            .as_nanos()
            .saturating_mul(DATAGRAMS_PER_SECOND);
        self.available = self
            .available
            .saturating_add(replenished)
            .min(DATAGRAM_BURST * TOKEN_SCALE);
        self.last_refill = now;
        if self.available < TOKEN_SCALE {
            return false;
        }
        self.available -= TOKEN_SCALE;
        true
    }
}

impl ControlIngressLimiter {
    fn new(now: Instant) -> Self {
        Self {
            available: CONTROL_MESSAGE_BURST * TOKEN_SCALE,
            last_refill: now,
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        let replenished = now
            .saturating_duration_since(self.last_refill)
            .as_nanos()
            .saturating_mul(CONTROL_MESSAGES_PER_SECOND);
        self.available = self
            .available
            .saturating_add(replenished)
            .min(CONTROL_MESSAGE_BURST * TOKEN_SCALE);
        self.last_refill = now;
        if self.available < TOKEN_SCALE {
            return false;
        }
        self.available -= TOKEN_SCALE;
        true
    }
}

#[derive(Clone)]
pub struct SessionRegistry {
    inner: Arc<Mutex<SessionState>>,
    maximum: usize,
}

#[derive(Default)]
struct SessionState {
    accounts: BTreeSet<AccountId>,
    actors: BTreeSet<ActorId>,
    controlled_actors: BTreeMap<AccountId, ActorId>,
}

pub struct SessionLease {
    registry: SessionRegistry,
    account_id: AccountId,
    actor_id: Option<ActorId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionClaimError {
    AlreadyActive,
    Full,
    Unavailable,
}

impl SessionRegistry {
    #[must_use]
    pub fn new(maximum: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionState::default())),
            maximum,
        }
    }

    pub fn claim_account(&self, account_id: AccountId) -> Result<SessionLease, SessionClaimError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| SessionClaimError::Unavailable)?;
        if state.accounts.contains(&account_id) {
            return Err(SessionClaimError::AlreadyActive);
        }
        if state.accounts.len() >= self.maximum {
            return Err(SessionClaimError::Full);
        }
        state.accounts.insert(account_id);
        Ok(SessionLease {
            registry: self.clone(),
            account_id,
            actor_id: None,
        })
    }

    pub fn inspect_account(
        &self,
        account_id: AccountId,
    ) -> Result<(bool, Option<ActorId>), SessionClaimError> {
        let state = self
            .inner
            .lock()
            .map_err(|_| SessionClaimError::Unavailable)?;
        Ok((
            state.accounts.contains(&account_id),
            state.controlled_actors.get(&account_id).copied(),
        ))
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new(MAX_GAMEPLAY_SESSIONS)
    }
}

impl SessionLease {
    pub fn claim_actor(&mut self, actor_id: ActorId) -> Result<(), SessionClaimError> {
        if self.actor_id == Some(actor_id) {
            return Ok(());
        }
        let mut state = self
            .registry
            .inner
            .lock()
            .map_err(|_| SessionClaimError::Unavailable)?;
        if state.actors.contains(&actor_id) {
            return Err(SessionClaimError::AlreadyActive);
        }
        if let Some(previous) = self.actor_id.replace(actor_id) {
            state.actors.remove(&previous);
        }
        state.actors.insert(actor_id);
        state.controlled_actors.insert(self.account_id, actor_id);
        Ok(())
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.registry.inner.lock() {
            state.accounts.remove(&self.account_id);
            state.controlled_actors.remove(&self.account_id);
            if let Some(actor_id) = self.actor_id {
                state.actors.remove(&actor_id);
            }
        }
    }
}

#[derive(Clone)]
pub struct CharacterCreationHandle {
    sender: tokio::sync::mpsc::Sender<CharacterCreationRequest>,
}

pub struct CharacterCreationRequest {
    account_id: AccountId,
    name: String,
    base_stats: CharacterCreationStatsV1,
    response: tokio::sync::oneshot::Sender<Result<ActorId, CharacterCreationError>>,
}

#[derive(Debug)]
pub enum CharacterCreationError {
    Persistence(StoreError),
    Simulation(String),
    Unavailable,
}

#[must_use]
pub fn character_creation_channel() -> (
    CharacterCreationHandle,
    tokio::sync::mpsc::Receiver<CharacterCreationRequest>,
) {
    let (sender, receiver) = tokio::sync::mpsc::channel(CHARACTER_CREATION_QUEUE_CAPACITY);
    (CharacterCreationHandle { sender }, receiver)
}

impl CharacterCreationHandle {
    pub async fn create(
        &self,
        account_id: AccountId,
        name: String,
        base_stats: CharacterCreationStatsV1,
    ) -> Result<ActorId, CharacterCreationError> {
        let (response, receive) = tokio::sync::oneshot::channel();
        self.sender
            .send(CharacterCreationRequest {
                account_id,
                name,
                base_stats,
                response,
            })
            .await
            .map_err(|_| CharacterCreationError::Unavailable)?;
        receive
            .await
            .map_err(|_| CharacterCreationError::Unavailable)?
    }
}

impl CharacterCreationRequest {
    #[must_use]
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn base_stats(&self) -> CharacterCreationStatsV1 {
        self.base_stats
    }

    pub fn complete(self, result: Result<ActorId, CharacterCreationError>) {
        let _send_result = self.response.send(result);
    }
}

enum SimulationInput {
    Command(QueuedCommand),
    HeldMovement(HeldMovementUpdateV1),
    BeginActorCreation {
        base_stats: CharacterCreationStatsV1,
        response: SyncSender<Result<ActorSpawn, String>>,
    },
    CompleteActorCreation {
        actor_id: ActorId,
        committed: bool,
        response: SyncSender<Result<(), String>>,
    },
    DespawnActor {
        actor_id: ActorId,
        response: SyncSender<Result<(), String>>,
    },
    SetConnected {
        actor_id: ActorId,
        connected: bool,
        response: SyncSender<Result<(), String>>,
    },
    Snapshot(SyncSender<WorldSnapshotV1>),
    BeginCheckpoint(SyncSender<Result<WorldSnapshotV1, String>>),
    CompleteCheckpoint(SyncSender<Result<(), String>>),
    InstallReservedBlock {
        block: ReservedIdBlock,
        response: SyncSender<Result<(), String>>,
    },
    Shutdown,
}

struct QueuedCommand {
    command: ClientCommand,
    durability: Option<SyncSender<Result<SimTick, String>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimulationExit {
    Requested,
    InputDisconnected,
    OutputDisconnected,
    OutputBackpressure,
    SimulationFailed(String),
}

#[derive(Debug)]
pub enum SimulationOutput {
    Tick {
        outcome: TickOutcome,
        commands: Vec<ClientCommand>,
        held_movement: Vec<HeldMovementUpdateV1>,
        connection_updates: Vec<ActorConnectionUpdateV1>,
        durability: Vec<DurabilityAcknowledgement>,
    },
    Failed(String),
}

#[derive(Debug)]
pub struct DurabilityAcknowledgement {
    tick: SimTick,
    response: SyncSender<Result<SimTick, String>>,
}

impl DurabilityAcknowledgement {
    #[must_use]
    pub const fn tick(&self) -> SimTick {
        self.tick
    }

    pub fn acknowledge(self, result: Result<SimTick, String>) {
        let _send_result = self.response.try_send(result);
    }
}

pub struct DurabilityReceipt {
    response: Receiver<Result<SimTick, String>>,
}

impl DurabilityReceipt {
    pub fn wait(self, timeout: Duration) -> Result<SimTick, SimulationCallError> {
        self.response
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Persistence)
    }
}

/// Owns the canonical simulation thread. Network tasks can submit commands,
/// but they never mutate world state directly.
pub struct SimulationHost {
    input: SyncSender<SimulationInput>,
    output: Receiver<SimulationOutput>,
    exit: Arc<Mutex<Option<SimulationExit>>>,
    thread: Option<JoinHandle<()>>,
    crafting: CraftingCatalog,
    reading: ReadingCatalog,
    disassembly: DisassemblyCatalog,
    construction: ConstructionCatalog,
}

#[derive(Clone)]
pub struct SimulationHandle {
    input: SyncSender<SimulationInput>,
    crafting: CraftingCatalog,
    reading: ReadingCatalog,
    disassembly: DisassemblyCatalog,
    construction: ConstructionCatalog,
}

impl SimulationHost {
    pub fn start(world: WorldState) -> Result<Self, std::io::Error> {
        Self::start_with_catalogs(world, CraftingCatalog::default(), ReadingCatalog::default())
    }

    pub fn start_with_crafting(
        world: WorldState,
        crafting: CraftingCatalog,
    ) -> Result<Self, std::io::Error> {
        Self::start_with_catalogs(world, crafting, ReadingCatalog::default())
    }

    pub fn start_with_catalogs(
        world: WorldState,
        crafting: CraftingCatalog,
        reading: ReadingCatalog,
    ) -> Result<Self, std::io::Error> {
        Self::start_with_gameplay_catalogs(world, crafting, reading, DisassemblyCatalog::default())
    }

    pub fn start_with_gameplay_catalogs(
        world: WorldState,
        crafting: CraftingCatalog,
        reading: ReadingCatalog,
        disassembly: DisassemblyCatalog,
    ) -> Result<Self, std::io::Error> {
        Self::start_with_all_gameplay_catalogs(
            world,
            crafting,
            reading,
            disassembly,
            ConstructionCatalog::default(),
        )
    }

    pub fn start_with_all_gameplay_catalogs(
        world: WorldState,
        crafting: CraftingCatalog,
        reading: ReadingCatalog,
        disassembly: DisassemblyCatalog,
        construction: ConstructionCatalog,
    ) -> Result<Self, std::io::Error> {
        Self::start_with_all_gameplay_catalogs_and_recovery_inputs(
            world,
            crafting,
            reading,
            disassembly,
            construction,
            Vec::new(),
        )
    }

    pub fn start_with_all_gameplay_catalogs_and_recovery_inputs(
        world: WorldState,
        crafting: CraftingCatalog,
        reading: ReadingCatalog,
        disassembly: DisassemblyCatalog,
        construction: ConstructionCatalog,
        connection_updates: Vec<ActorConnectionUpdateV1>,
    ) -> Result<Self, std::io::Error> {
        let (input, input_receiver) = mpsc::sync_channel(INPUT_QUEUE_CAPACITY);
        let (output_sender, output) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
        let exit = Arc::new(Mutex::new(None));
        let thread_exit = Arc::clone(&exit);
        let thread = thread::Builder::new()
            .name(String::from("cdda-simulation"))
            .spawn(move || {
                run_simulation(
                    world,
                    input_receiver,
                    output_sender,
                    &thread_exit,
                    connection_updates,
                );
            })?;
        Ok(Self {
            input,
            output,
            exit,
            thread: Some(thread),
            crafting,
            reading,
            disassembly,
            construction,
        })
    }

    pub fn submit(&self, command: ClientCommand) -> Result<(), SubmitError> {
        self.handle().submit(command)
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<SimulationOutput, RecvTimeoutError> {
        self.output.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Result<SimulationOutput, TryRecvError> {
        self.output.try_recv()
    }

    #[must_use]
    pub fn handle(&self) -> SimulationHandle {
        SimulationHandle {
            input: self.input.clone(),
            crafting: self.crafting.clone(),
            reading: self.reading.clone(),
            disassembly: self.disassembly.clone(),
            construction: self.construction.clone(),
        }
    }

    #[must_use]
    pub fn exit_reason(&self) -> Option<SimulationExit> {
        lock_unpoisoned(&self.exit).clone()
    }

    pub fn shutdown(mut self) -> SimulationExit {
        let _send_result = self.input.send(SimulationInput::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
        self.exit_reason()
            .unwrap_or(SimulationExit::InputDisconnected)
    }
}

impl SimulationHandle {
    pub fn submit(&self, command: ClientCommand) -> Result<(), SubmitError> {
        self.send_command(QueuedCommand {
            command,
            durability: None,
        })
    }

    pub fn submit_durable(&self, command: ClientCommand) -> Result<DurabilityReceipt, SubmitError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.send_command(QueuedCommand {
            command,
            durability: Some(response),
        })?;
        Ok(DurabilityReceipt { response: receive })
    }

    fn send_command(&self, mut command: QueuedCommand) -> Result<(), SubmitError> {
        self.crafting.normalize(&mut command.command);
        self.reading.normalize(&mut command.command);
        self.disassembly.normalize(&mut command.command);
        self.construction.normalize(&mut command.command);
        self.input
            .try_send(SimulationInput::Command(command))
            .map_err(|error| match error {
                TrySendError::Full(_) => SubmitError::Backpressure,
                TrySendError::Disconnected(_) => SubmitError::Stopped,
            })
    }

    pub fn submit_held_movement(&self, input: HeldMovementUpdateV1) -> Result<(), SubmitError> {
        self.input
            .try_send(SimulationInput::HeldMovement(input))
            .map_err(|error| match error {
                TrySendError::Full(_) => SubmitError::Backpressure,
                TrySendError::Disconnected(_) => SubmitError::Stopped,
            })
    }

    pub fn snapshot(&self, timeout: Duration) -> Result<WorldSnapshotV1, SnapshotError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::Snapshot(response))
            .map_err(|error| match error {
                TrySendError::Full(_) => SnapshotError::Backpressure,
                TrySendError::Disconnected(_) => SnapshotError::Stopped,
            })?;
        receive.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => SnapshotError::Timeout,
            RecvTimeoutError::Disconnected => SnapshotError::Stopped,
        })
    }

    /// Spawns a provisional actor and pauses canonical tick advancement until
    /// `complete_actor_creation` commits or rolls it back.
    pub fn begin_actor_creation(
        &self,
        base_stats: CharacterCreationStatsV1,
        timeout: Duration,
    ) -> Result<ActorSpawn, SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::BeginActorCreation {
                base_stats,
                response,
            })
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }

    pub fn complete_actor_creation(
        &self,
        actor_id: ActorId,
        committed: bool,
        timeout: Duration,
    ) -> Result<(), SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::CompleteActorCreation {
                actor_id,
                committed,
                response,
            })
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }

    pub fn despawn_actor(
        &self,
        actor_id: ActorId,
        timeout: Duration,
    ) -> Result<(), SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::DespawnActor { actor_id, response })
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }

    pub fn set_connected(
        &self,
        actor_id: ActorId,
        connected: bool,
        timeout: Duration,
    ) -> Result<(), SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::SetConnected {
                actor_id,
                connected,
                response,
            })
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }

    /// Pauses tick advancement at an exact snapshot boundary. Commands remain
    /// queued until `complete_checkpoint` resumes the simulation.
    pub fn begin_checkpoint(
        &self,
        timeout: Duration,
    ) -> Result<WorldSnapshotV1, SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::BeginCheckpoint(response))
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }

    pub fn complete_checkpoint(&self, timeout: Duration) -> Result<(), SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::CompleteCheckpoint(response))
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }

    pub fn install_reserved_block(
        &self,
        block: ReservedIdBlock,
        timeout: Duration,
    ) -> Result<(), SimulationCallError> {
        let (response, receive) = mpsc::sync_channel(1);
        self.input
            .try_send(SimulationInput::InstallReservedBlock { block, response })
            .map_err(map_call_send_error)?;
        receive
            .recv_timeout(timeout)
            .map_err(map_call_receive_error)?
            .map_err(SimulationCallError::Simulation)
    }
}

fn map_call_send_error(error: TrySendError<SimulationInput>) -> SimulationCallError {
    match error {
        TrySendError::Full(_) => SimulationCallError::Backpressure,
        TrySendError::Disconnected(_) => SimulationCallError::Stopped,
    }
}

fn map_call_receive_error(error: RecvTimeoutError) -> SimulationCallError {
    match error {
        RecvTimeoutError::Timeout => SimulationCallError::Timeout,
        RecvTimeoutError::Disconnected => SimulationCallError::Stopped,
    }
}

impl Drop for SimulationHost {
    fn drop(&mut self) {
        let _send_result = self.input.try_send(SimulationInput::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
    }
}

fn run_simulation(
    mut world: WorldState,
    input: Receiver<SimulationInput>,
    output: SyncSender<SimulationOutput>,
    exit: &Mutex<Option<SimulationExit>>,
    mut connection_updates: Vec<ActorConnectionUpdateV1>,
) {
    let mut commands: Vec<QueuedCommand> = Vec::new();
    let mut held_movement = Vec::new();
    let mut deadline = Instant::now() + SIMULATION_INTERVAL;
    let mut actor_creation = None;
    let mut checkpoint_paused = false;
    loop {
        let wait = if actor_creation.is_some() || checkpoint_paused {
            Duration::from_secs(60)
        } else {
            deadline.saturating_duration_since(Instant::now())
        };
        match input.recv_timeout(wait) {
            Ok(SimulationInput::Command(command)) => {
                commands.push(command);
                continue;
            }
            Ok(SimulationInput::HeldMovement(input)) => {
                held_movement.push(input);
                continue;
            }
            Ok(SimulationInput::BeginActorCreation {
                base_stats,
                response,
            }) => {
                let result = if actor_creation.is_some() || checkpoint_paused {
                    Err(String::from(
                        "another actor creation is already in progress",
                    ))
                } else {
                    world
                        .spawn_actor_first_available_with_stats(true, base_stats)
                        .and_then(|actor_id| {
                            let actor = world
                                .actor_snapshot(actor_id)
                                .ok_or(cdda_sim::SimError::UnknownActor)?;
                            actor_creation = Some(actor_id);
                            Ok(ActorSpawn {
                                actor,
                                created_tick: world.tick(),
                            })
                        })
                        .map_err(|error| error.to_string())
                };
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::CompleteActorCreation {
                actor_id,
                committed,
                response,
            }) => {
                let result = if actor_creation != Some(actor_id) {
                    Err(String::from("actor creation completion does not match"))
                } else if committed {
                    Ok(())
                } else {
                    world
                        .despawn_actor(actor_id)
                        .map_err(|error| error.to_string())
                };
                if result.is_ok() {
                    actor_creation = None;
                    deadline = Instant::now() + SIMULATION_INTERVAL;
                }
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::DespawnActor { actor_id, response }) => {
                let result = world
                    .despawn_actor(actor_id)
                    .map_err(|error| error.to_string());
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::SetConnected {
                actor_id,
                connected,
                response,
            }) => {
                let result = world
                    .set_connected(actor_id, connected)
                    .map_err(|error| error.to_string());
                if result.is_ok() {
                    connection_updates.push(ActorConnectionUpdateV1 {
                        actor_id,
                        connected,
                    });
                }
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::Snapshot(response)) => {
                let _send_result = response.try_send(world.snapshot());
                continue;
            }
            Ok(SimulationInput::BeginCheckpoint(response)) => {
                let result = if actor_creation.is_some() || checkpoint_paused {
                    Err(String::from(
                        "simulation is already paused for a transaction",
                    ))
                } else {
                    checkpoint_paused = true;
                    Ok(world.snapshot())
                };
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::CompleteCheckpoint(response)) => {
                let result = if checkpoint_paused {
                    checkpoint_paused = false;
                    deadline = Instant::now() + SIMULATION_INTERVAL;
                    Ok(())
                } else {
                    Err(String::from("no checkpoint transaction is in progress"))
                };
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::InstallReservedBlock { block, response }) => {
                let result = if checkpoint_paused {
                    world
                        .install_reserved_block(block)
                        .map_err(|error| error.to_string())
                } else {
                    Err(String::from(
                        "ID blocks may install only at a checkpoint boundary",
                    ))
                };
                let _send_result = response.try_send(result);
                continue;
            }
            Ok(SimulationInput::Shutdown) => {
                set_exit(exit, SimulationExit::Requested);
                return;
            }
            Err(RecvTimeoutError::Disconnected) => {
                set_exit(exit, SimulationExit::InputDisconnected);
                return;
            }
            Err(RecvTimeoutError::Timeout) if actor_creation.is_some() || checkpoint_paused => {
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let pending = std::mem::take(&mut commands);
        let journal_held_movement = std::mem::take(&mut held_movement);
        let journal_connection_updates = std::mem::take(&mut connection_updates);
        let journal_commands = pending
            .iter()
            .map(|queued| queued.command.clone())
            .collect::<Vec<_>>();
        let durability = pending
            .into_iter()
            .filter_map(|queued| queued.durability)
            .collect::<Vec<_>>();
        let message = match world.advance_tick_with_recovery_inputs(
            journal_commands.clone(),
            journal_held_movement.clone(),
            journal_connection_updates.clone(),
        ) {
            Ok(outcome) => SimulationOutput::Tick {
                outcome,
                commands: journal_commands,
                held_movement: journal_held_movement,
                connection_updates: journal_connection_updates,
                durability: durability
                    .into_iter()
                    .map(|response| DurabilityAcknowledgement {
                        tick: world.tick(),
                        response,
                    })
                    .collect(),
            },
            Err(error) => {
                let detail = error.to_string();
                for response in durability {
                    let _send_result = response.try_send(Err(detail.clone()));
                }
                let _send_result = output.try_send(SimulationOutput::Failed(detail.clone()));
                set_exit(exit, SimulationExit::SimulationFailed(detail));
                return;
            }
        };
        match output.try_send(message) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                set_exit(exit, SimulationExit::OutputBackpressure);
                return;
            }
            Err(TrySendError::Disconnected(_)) => {
                set_exit(exit, SimulationExit::OutputDisconnected);
                return;
            }
        }
        deadline += SIMULATION_INTERVAL;
    }
}

fn set_exit(exit: &Mutex<Option<SimulationExit>>, reason: SimulationExit) {
    *lock_unpoisoned(exit) = Some(reason);
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmitError {
    Backpressure,
    Stopped,
}

impl fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backpressure => formatter.write_str("simulation input queue is full"),
            Self::Stopped => formatter.write_str("simulation has stopped"),
        }
    }
}

impl std::error::Error for SubmitError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    Backpressure,
    Stopped,
    Timeout,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backpressure => formatter.write_str("simulation input queue is full"),
            Self::Stopped => formatter.write_str("simulation has stopped"),
            Self::Timeout => formatter.write_str("simulation snapshot request timed out"),
        }
    }
}

impl std::error::Error for SnapshotError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SimulationCallError {
    Backpressure,
    Persistence(String),
    Simulation(String),
    Stopped,
    Timeout,
}

impl fmt::Display for SimulationCallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backpressure => formatter.write_str("simulation input queue is full"),
            Self::Persistence(error) => write!(formatter, "persistence failed: {error}"),
            Self::Simulation(error) => write!(formatter, "simulation operation failed: {error}"),
            Self::Stopped => formatter.write_str("simulation has stopped"),
            Self::Timeout => formatter.write_str("simulation operation timed out"),
        }
    }
}

impl std::error::Error for SimulationCallError {}

pub async fn bind_iroh_endpoint(
    secret_key: SecretKey,
) -> Result<Endpoint, iroh::endpoint::BindError> {
    Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![
            GAME_ALPN.to_vec(),
            ENROLL_ALPN.to_vec(),
            ADMIN_ALPN.to_vec(),
        ])
        .bind()
        .await
}

/// Handles the sole control stream for a fully handshaken enrollment
/// connection. Authentication is the connection's iroh endpoint identity; the
/// request contains no credential or bearer secret.
pub async fn handle_enrollment_connection(
    connection: &Connection,
    persistence: PersistenceHandle,
) -> Result<Option<AccountRecord>, NetworkError> {
    if connection.alpn() != ENROLL_ALPN {
        return Err(NetworkError::WrongAlpn);
    }
    let remote = EndpointIdentity(*connection.remote_id().as_bytes());
    let (mut send, mut receive) =
        tokio::time::timeout(AUTHORIZATION_TIMEOUT, connection.accept_bi())
            .await
            .map_err(|_| NetworkError::AuthorizationTimeout)?
            .map_err(|error| NetworkError::Transport(error.to_string()))?;
    let request = tokio::time::timeout(AUTHORIZATION_TIMEOUT, read_control_frame(&mut receive))
        .await
        .map_err(|_| NetworkError::AuthorizationTimeout)??;
    let response = match request {
        ControlMessage::EnrollmentRequest { protocol_version }
            if protocol_version == PROTOCOL_VERSION =>
        {
            let now = utc_now_seconds()?;
            match persistence_call(move || persistence.enroll_endpoint(remote, now)).await {
                Ok(account) => {
                    let message = ControlMessage::EnrollmentAccepted(EnrollmentAccepted {
                        account_id: account.id,
                        display_name: account.display_name.clone(),
                        role: account.role,
                    });
                    (message, Some(account))
                }
                Err(NetworkError::Persistence(error)) => (
                    ControlMessage::EnrollmentRejected(map_enrollment_rejection(&error)),
                    None,
                ),
                Err(error) => return Err(error),
            }
        }
        ControlMessage::EnrollmentRequest { .. } => (
            ControlMessage::EnrollmentRejected(EnrollmentRejection::ProtocolMismatch),
            None,
        ),
        _ => return Err(NetworkError::UnexpectedMessage),
    };
    write_control_frame(&mut send, &response.0).await?;
    send.finish()
        .map_err(|error| NetworkError::Transport(error.to_string()))?;
    let stopped = tokio::time::timeout(AUTHORIZATION_TIMEOUT, send.stopped())
        .await
        .map_err(|_| NetworkError::AuthorizationTimeout)?
        .map_err(|error| NetworkError::Transport(error.to_string()))?;
    if stopped.is_some() {
        return Err(NetworkError::Transport(String::from(
            "peer rejected the enrollment response stream",
        )));
    }
    Ok(response.1)
}

/// Runs a short-lived, fully authenticated administration control stream.
/// Every request is authorized again inside its persistence transaction, and
/// any authorization change for the operator ends the connection.
pub async fn handle_admin_connection(
    connection: &Connection,
    persistence: PersistenceHandle,
    authorization_change_hub: AuthorizationChangeHub,
    sessions: SessionRegistry,
    simulation: SimulationHandle,
) -> Result<(), NetworkError> {
    if connection.alpn() != ADMIN_ALPN {
        return Err(NetworkError::WrongAlpn);
    }
    let started = Instant::now();
    let remote = EndpointIdentity(*connection.remote_id().as_bytes());
    let mut authorization_changes = authorization_change_hub.subscribe();
    let authorization = persistence.clone();
    let now = utc_now_seconds()?;
    let operator =
        match persistence_call(move || authorization.authorize_admin_endpoint(remote, now)).await {
            Ok(account) => account,
            Err(NetworkError::Persistence(StoreError::UnauthorizedEndpoint))
            | Err(NetworkError::Persistence(StoreError::AccountUnavailable)) => {
                return Err(NetworkError::UnauthorizedIdentity);
            }
            Err(NetworkError::Persistence(StoreError::AdministratorRequired)) => {
                return Err(NetworkError::AdministratorRequired);
            }
            Err(NetworkError::Persistence(StoreError::ModeratorRequired)) => {
                return Err(NetworkError::ModeratorRequired);
            }
            Err(error) => return Err(error),
        };
    let (mut send, mut receive) =
        tokio::time::timeout(AUTHORIZATION_TIMEOUT, connection.accept_bi())
            .await
            .map_err(|_| NetworkError::AuthorizationTimeout)?
            .map_err(|error| NetworkError::Transport(error.to_string()))?;
    let hello =
        match tokio::time::timeout(AUTHORIZATION_TIMEOUT, read_control_frame(&mut receive)).await {
            Err(_) => return Err(NetworkError::AuthorizationTimeout),
            Ok(Ok(message)) => message,
            Ok(Err(error @ (FrameIoError::Codec(_) | FrameIoError::TooLarge))) => {
                audit_invalid_admin_frame(&persistence, remote).await?;
                return Err(NetworkError::from(error));
            }
            Ok(Err(error)) => return Err(NetworkError::from(error)),
        };
    let valid_hello = matches!(
        hello,
        ControlMessage::AdminHello(cdda_protocol::AdminHello { protocol_version })
            if protocol_version == PROTOCOL_VERSION
    );
    if !valid_hello {
        let audit = persistence.clone();
        let now = utc_now_seconds()?;
        persistence_call(move || audit.audit_invalid_admin_message(remote, now)).await?;
        let rejection = if matches!(hello, ControlMessage::AdminHello(_)) {
            AdminRejection::ProtocolMismatch
        } else {
            AdminRejection::UnexpectedMessage
        };
        write_control_frame(
            &mut send,
            &ControlMessage::AdminResponse(AdminResponse::Rejected(rejection)),
        )
        .await?;
        finish_gameplay_response(&mut send).await?;
        return Ok(());
    }
    write_control_frame(
        &mut send,
        &ControlMessage::AdminResponse(AdminResponse::Ready {
            account_id: operator.id,
            role: operator.role,
        }),
    )
    .await?;
    let mut ingress = ControlIngressLimiter::new(Instant::now());
    let mut rate_limit_violations = 0_u8;

    loop {
        let Some(freshness_remaining) = ADMIN_CONNECTION_LIFETIME.checked_sub(started.elapsed())
        else {
            return Err(NetworkError::AdminConnectionStale);
        };
        let read_timeout = freshness_remaining.min(INBOUND_TRAFFIC_TIMEOUT);
        let message = tokio::select! {
            authorization_change = authorization_changes.recv() => {
                match authorization_change {
                    Ok(change)
                        if change.account_id == operator.id
                            && change.endpoint.is_none_or(|endpoint| endpoint == remote) =>
                    {
                        break;
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
                    | Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err(NetworkError::AuthorizationRevoked);
                    }
                }
            }
            result = tokio::time::timeout(read_timeout, read_control_frame(&mut receive)) => {
                match result {
                    Ok(Ok(message)) => message,
                    Ok(Err(FrameIoError::Transport(_))) => break,
                    Ok(Err(error @ (FrameIoError::Codec(_) | FrameIoError::TooLarge))) => {
                        audit_invalid_admin_frame(&persistence, remote).await?;
                        return Err(NetworkError::from(error));
                    }
                    Ok(Err(error)) => return Err(NetworkError::from(error)),
                    Err(_) if started.elapsed() >= ADMIN_CONNECTION_LIFETIME => {
                        return Err(NetworkError::AdminConnectionStale);
                    }
                    Err(_) => return Err(NetworkError::HeartbeatTimeout),
                }
            }
        };
        let ControlMessage::AdminRequest(request) = message else {
            let audit = persistence.clone();
            let now = utc_now_seconds()?;
            persistence_call(move || audit.audit_invalid_admin_message(remote, now)).await?;
            write_control_frame(
                &mut send,
                &ControlMessage::AdminResponse(AdminResponse::Rejected(
                    AdminRejection::UnexpectedMessage,
                )),
            )
            .await?;
            finish_gameplay_response(&mut send).await?;
            return Ok(());
        };
        if !ingress.allow(Instant::now()) {
            rate_limit_violations = rate_limit_violations.saturating_add(1);
            let audit = persistence.clone();
            let now = utc_now_seconds()?;
            persistence_call(move || audit.audit_rate_limited_admin_request(remote, request, now))
                .await?;
            write_control_frame(
                &mut send,
                &ControlMessage::AdminResponse(AdminResponse::Rejected(AdminRejection::ServerBusy)),
            )
            .await?;
            if rate_limit_violations >= RATE_LIMIT_VIOLATIONS_BEFORE_CLOSE {
                finish_gameplay_response(&mut send).await?;
                return Err(NetworkError::RateLimited);
            }
            continue;
        }
        rate_limit_violations = 0;
        let (response, changed_accounts) = match request {
            AdminRequest::ListAccounts { after, limit } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_accounts(remote, after, limit, now)
                })
                .await
                {
                    Ok(page) => (
                        AdminResponse::Accounts {
                            accounts: page
                                .accounts
                                .into_iter()
                                .map(admin_account_summary)
                                .collect(),
                            next_after: page.next_after,
                        },
                        Vec::new(),
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::ListCharacters { account_id } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_characters(remote, account_id, now)
                })
                .await
                {
                    Ok(characters) => match sessions.inspect_account(account_id) {
                        Ok((gameplay_session_active, controlled_actor)) => (
                            AdminResponse::Characters {
                                account_id,
                                characters,
                                gameplay_session_active,
                                controlled_actor,
                            },
                            Vec::new(),
                        ),
                        Err(_) => (
                            AdminResponse::Rejected(AdminRejection::ServerBusy),
                            Vec::new(),
                        ),
                    },
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::InspectCharacter {
                actor_id,
                inventory_after,
                inventory_limit,
            } => {
                let inspection_persistence = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    inspection_persistence.admin_private_character(
                        remote,
                        actor_id,
                        inventory_after,
                        inventory_limit,
                        now,
                    )
                })
                .await
                {
                    Ok(identity) => {
                        let snapshot = simulation_snapshot(&simulation).await?;
                        match private_character_inspection(
                            &snapshot,
                            identity,
                            inventory_after,
                            inventory_limit,
                        ) {
                            Some(inspection) => (
                                AdminResponse::PrivateCharacter(Box::new(inspection)),
                                Vec::new(),
                            ),
                            None => (
                                AdminResponse::Rejected(AdminRejection::ServerBusy),
                                Vec::new(),
                            ),
                        }
                    }
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::ListReports {
                state,
                after,
                limit,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_reports(remote, state, after, limit, now)
                })
                .await
                {
                    Ok(page) => (
                        AdminResponse::Reports {
                            reports: page.reports,
                            next_after: page.next_after,
                        },
                        Vec::new(),
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::SetReportState { report_id, state } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.set_report_state(remote, report_id, state, now)
                })
                .await
                {
                    Ok(report) => (AdminResponse::ReportUpdated(report), Vec::new()),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::CreateAccount {
                display_name,
                role,
                endpoint,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_create_account(remote, display_name, role, endpoint, now)
                })
                .await
                {
                    Ok(created) => (
                        AdminResponse::AccountCreated {
                            account: admin_account_summary(created.account),
                            pending_endpoint: created.pending_endpoint,
                        },
                        Vec::new(),
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::ListEndpoints { account_id } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_endpoint_bindings(remote, account_id, now)
                })
                .await
                {
                    Ok(bindings) => (
                        AdminResponse::Endpoints {
                            account_id,
                            bindings,
                        },
                        Vec::new(),
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::AddEndpoint {
                account_id,
                endpoint,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_add_pending_endpoint(remote, account_id, endpoint, now)
                })
                .await
                {
                    Ok(binding) => (
                        AdminResponse::EndpointPending {
                            account_id,
                            binding,
                        },
                        Vec::new(),
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::RevokeEndpoint {
                account_id,
                endpoint,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_revoke_endpoint(remote, account_id, endpoint, now)
                })
                .await
                {
                    Ok(()) => {
                        authorization_change_hub.publish_endpoint(account_id, endpoint);
                        (
                            AdminResponse::EndpointRevoked {
                                account_id,
                                endpoint,
                            },
                            Vec::new(),
                        )
                    }
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::ListModerationHistory {
                account_id,
                after,
                limit,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.admin_moderation_history(remote, account_id, after, limit, now)
                })
                .await
                {
                    Ok(page) => (
                        AdminResponse::ModerationHistory {
                            account_id,
                            entries: page.entries,
                            next_after: page.next_after,
                        },
                        Vec::new(),
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::SetRole { account_id, role } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.set_account_role(remote, account_id, role, now)
                })
                .await
                {
                    Ok(mutation) => {
                        let changed = mutation.changed.then_some(mutation.account.id);
                        (
                            AdminResponse::AccountUpdated(admin_account_summary(mutation.account)),
                            changed.map_or_else(Vec::new, |account_id| vec![account_id]),
                        )
                    }
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::SetStatus { account_id, status } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.set_account_status(remote, account_id, status, now)
                })
                .await
                {
                    Ok(mutation) => {
                        let changed = mutation.changed.then_some(mutation.account.id);
                        (
                            AdminResponse::AccountUpdated(admin_account_summary(mutation.account)),
                            changed.map_or_else(Vec::new, |account_id| vec![account_id]),
                        )
                    }
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::Kick { account_id } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || administration.kick_account(remote, account_id, now))
                    .await
                {
                    Ok(account) => (
                        AdminResponse::ModerationApplied {
                            account: admin_account_summary(account),
                            kind: ModerationKind::Kick,
                            until_utc: None,
                        },
                        vec![account_id],
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::SetSuspension {
                account_id,
                duration_seconds,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.set_account_suspension(remote, account_id, duration_seconds, now)
                })
                .await
                {
                    Ok(mutation) => {
                        let until_utc = mutation.account.suspended_until_utc;
                        let changed_accounts = if mutation.changed && until_utc.is_some() {
                            vec![account_id]
                        } else {
                            Vec::new()
                        };
                        (
                            AdminResponse::ModerationApplied {
                                account: admin_account_summary(mutation.account),
                                kind: ModerationKind::Suspension,
                                until_utc,
                            },
                            changed_accounts,
                        )
                    }
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::SetMute {
                account_id,
                duration_seconds,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.set_account_mute(remote, account_id, duration_seconds, now)
                })
                .await
                {
                    Ok(mutation) => {
                        let until_utc = mutation.account.muted_until_utc;
                        (
                            AdminResponse::ModerationApplied {
                                account: admin_account_summary(mutation.account),
                                kind: ModerationKind::Mute,
                                until_utc,
                            },
                            Vec::new(),
                        )
                    }
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
            AdminRequest::TransferCharacter {
                actor_id,
                new_owner,
            } => {
                let administration = persistence.clone();
                let now = utc_now_seconds()?;
                match persistence_call(move || {
                    administration.transfer_character(remote, actor_id, new_owner, now)
                })
                .await
                {
                    Ok(transfer) => (
                        AdminResponse::CharacterTransferred {
                            actor_id: transfer.actor_id,
                            previous_owner: transfer.previous_owner,
                            new_owner: transfer.new_owner,
                        },
                        vec![transfer.previous_owner, transfer.new_owner],
                    ),
                    Err(NetworkError::Persistence(error)) => (
                        AdminResponse::Rejected(map_admin_rejection(error)?),
                        Vec::new(),
                    ),
                    Err(error) => return Err(error),
                }
            }
        };
        for account_id in changed_accounts {
            authorization_change_hub.publish_account(account_id);
        }
        write_control_frame(&mut send, &ControlMessage::AdminResponse(response)).await?;
    }
    Ok(())
}

async fn audit_invalid_admin_frame(
    persistence: &PersistenceHandle,
    remote: EndpointIdentity,
) -> Result<(), NetworkError> {
    let audit = persistence.clone();
    let now = utc_now_seconds()?;
    persistence_call(move || audit.audit_invalid_admin_message(remote, now)).await
}

/// Runs one authenticated gameplay control stream. The iroh peer identity is
/// authorized before any character data is disclosed or command is accepted.
pub async fn handle_game_connection(
    connection: &Connection,
    persistence: PersistenceHandle,
    simulation: SimulationHandle,
    content: ContentIdentity,
    characters: CharacterCreationHandle,
    committed_events: CommittedEventHub,
    chat: ChatHub,
) -> Result<(), NetworkError> {
    handle_game_connection_with_sessions(
        connection,
        persistence,
        simulation,
        content,
        SessionRegistry::default(),
        AuthorizationChangeHub::default(),
        characters,
        committed_events,
        chat,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "the handler receives explicit bounded server services for test substitution"
)]
pub async fn handle_game_connection_with_sessions(
    connection: &Connection,
    persistence: PersistenceHandle,
    simulation: SimulationHandle,
    content: ContentIdentity,
    sessions: SessionRegistry,
    authorization_change_hub: AuthorizationChangeHub,
    characters: CharacterCreationHandle,
    committed_event_hub: CommittedEventHub,
    chat_hub: ChatHub,
) -> Result<(), NetworkError> {
    if connection.alpn() != GAME_ALPN {
        return Err(NetworkError::WrongAlpn);
    }
    require_datagram_support(connection)?;
    let remote = EndpointIdentity(*connection.remote_id().as_bytes());
    let mut authorization_changes = authorization_change_hub.subscribe();
    let authorization = persistence.clone();
    let now = utc_now_seconds()?;
    let account =
        match persistence_call(move || authorization.authorize_endpoint(remote, now)).await {
            Ok(account) => account,
            Err(NetworkError::Persistence(StoreError::UnauthorizedEndpoint)) => {
                return Err(NetworkError::UnauthorizedIdentity);
            }
            Err(error) => return Err(error),
        };
    let (mut send, mut receive) =
        tokio::time::timeout(AUTHORIZATION_TIMEOUT, connection.accept_bi())
            .await
            .map_err(|_| NetworkError::AuthorizationTimeout)?
            .map_err(|error| NetworkError::Transport(error.to_string()))?;
    let hello = tokio::time::timeout(AUTHORIZATION_TIMEOUT, read_control_frame(&mut receive))
        .await
        .map_err(|_| NetworkError::AuthorizationTimeout)??;
    let ControlMessage::ClientHello(hello) = hello else {
        return Err(NetworkError::UnexpectedMessage);
    };
    if hello.protocol_version != PROTOCOL_VERSION || hello.content != content {
        write_control_frame(
            &mut send,
            &ControlMessage::GameplayRejected(GameplayRejection::ContentMismatch),
        )
        .await?;
        finish_gameplay_response(&mut send).await?;
        return Ok(());
    }
    let mut session = match sessions.claim_account(account.id) {
        Ok(session) => session,
        Err(error) => {
            write_control_frame(
                &mut send,
                &ControlMessage::GameplayRejected(map_session_rejection(error)),
            )
            .await?;
            finish_gameplay_response(&mut send).await?;
            return Ok(());
        }
    };
    let initial = simulation_snapshot(&simulation).await?;
    write_control_frame(
        &mut send,
        &ControlMessage::ServerHello(ServerHello {
            protocol_version: PROTOCOL_VERSION,
            content: content.clone(),
            tick: initial.tick,
        }),
    )
    .await?;
    send_character_list(&mut send, &persistence, account.id).await?;

    let mut selected_actor = None;
    let mut selected_character = String::new();
    let mut event_stream = None;
    let mut committed_events = committed_event_hub.subscribe();
    let mut chat_messages = chat_hub.subscribe();
    let mut ingress = ControlIngressLimiter::new(Instant::now());
    let mut datagram_ingress = DatagramIngressLimiter::new(Instant::now());
    let mut rate_limit_violations = 0_u8;
    let mut malformed_datagrams = 0_u8;
    let mut last_held_sequence = HeldInputSequence(0);
    let mut held_input_active = false;
    let mut last_held_input_at = Instant::now();
    let mut snapshot_updates = None;
    let mut snapshot_output_task = None;
    let (snapshot_failure_sender, mut snapshot_failures) = tokio::sync::mpsc::channel(1);
    let mut held_input_watchdog = tokio::time::interval(Duration::from_millis(50));
    held_input_watchdog.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    held_input_watchdog.tick().await;
    let mut replication = tokio::time::interval(Duration::from_millis(100));
    replication.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    replication.tick().await;
    let (inbound_sender, mut inbound_messages) = tokio::sync::mpsc::channel(128);
    let inbound_reader = tokio::spawn(async move {
        loop {
            let result = match tokio::time::timeout(
                INBOUND_TRAFFIC_TIMEOUT,
                read_control_frame(&mut receive),
            )
            .await
            {
                Err(_) => Err(NetworkError::HeartbeatTimeout),
                Ok(Err(FrameIoError::Transport(_))) => break,
                Ok(Err(error)) => Err(NetworkError::Frame(error.to_string())),
                Ok(Ok(message)) => Ok(message),
            };
            let failed = result.is_err();
            if inbound_sender.send(result).await.is_err() || failed {
                break;
            }
        }
    });
    let session_result = async {
        loop {
            let message = tokio::select! {
                result = inbound_messages.recv() => {
                    match result {
                        Some(Ok(message)) => Some(message),
                        Some(Err(error)) => return Err(error),
                        None => break,
                    }
                }
                authorization_change = authorization_changes.recv() => {
                    match authorization_change {
                        Ok(change)
                            if change.account_id == account.id
                                && change.endpoint.is_none_or(|endpoint| endpoint == remote) =>
                        {
                            break;
                        }
                        Ok(_) => None,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
                        | Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            return Err(NetworkError::AuthorizationRevoked);
                        }
                    }
                }
                _ = replication.tick(), if selected_actor.is_some() => {
                    let actor_id = selected_actor.ok_or(NetworkError::ServerBusy)?;
                    let snapshot = interest_snapshot(
                        simulation_snapshot(&simulation).await?,
                        actor_id,
                    )?;
                    queue_snapshot(&snapshot_updates, snapshot)?;
                    None
                }
                Some(error) = snapshot_failures.recv() => {
                    return Err(NetworkError::SnapshotOutput(error));
                }
                datagram = connection.read_datagram() => {
                    let encoded = match datagram {
                        Ok(encoded) => encoded,
                        Err(_connection_closed) => break,
                    };
                    if encoded.len() > MAX_DATAGRAM_SIZE || !datagram_ingress.allow(Instant::now()) {
                        None
                    } else {
                        match decode_client_datagram(&encoded) {
                            Ok(ClientDatagramV1::HeldMovement(input))
                                if selected_actor == Some(input.actor_id)
                                    && input.sequence > last_held_sequence =>
                            {
                                let update = HeldMovementUpdateV1 {
                                    actor_id: input.actor_id,
                                    sequence: input.sequence,
                                    client_tick: input.client_tick,
                                    direction: input.direction,
                                    source: HeldMovementUpdateSource::Client,
                                };
                                if simulation.submit_held_movement(update).is_ok() {
                                    last_held_sequence = input.sequence;
                                    held_input_active = input.direction.is_some();
                                    last_held_input_at = Instant::now();
                                }
                                malformed_datagrams = 0;
                                None
                            }
                            Ok(ClientDatagramV1::HeldMovement(input))
                                if selected_actor == Some(input.actor_id) =>
                            {
                                None
                            }
                            _ => {
                                malformed_datagrams = malformed_datagrams.saturating_add(1);
                                if malformed_datagrams >= RATE_LIMIT_VIOLATIONS_BEFORE_CLOSE {
                                    return Err(NetworkError::InvalidDatagram);
                                }
                                None
                            }
                        }
                    }
                }
                _ = held_input_watchdog.tick(), if held_input_active => {
                    if last_held_input_at.elapsed() >= HELD_INPUT_LEASE {
                        let actor_id = selected_actor.ok_or(NetworkError::ServerBusy)?;
                        let update = HeldMovementUpdateV1 {
                            actor_id,
                            sequence: last_held_sequence,
                            client_tick: simulation_snapshot(&simulation).await?.tick,
                            direction: None,
                            source: HeldMovementUpdateSource::LeaseExpired,
                        };
                        if simulation.submit_held_movement(update).is_ok() {
                            held_input_active = false;
                        }
                    }
                    None
                }
                committed = committed_events.recv(), if selected_actor.is_some() => {
                    let batch = match committed {
                        Ok(batch) => batch,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            return Err(NetworkError::EventBackpressure);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            return Err(NetworkError::EventStreamClosed);
                        }
                    };
                    let actor_id = selected_actor.ok_or(NetworkError::ServerBusy)?;
                    let visible_effect_npcs = if batch.events.iter().any(|event| {
                        matches!(
                            &event.kind,
                            WorldEventKind::NpcDamagedByEffect { .. }
                                | WorldEventKind::NpcKilledByEffect { .. }
                        )
                    }) {
                        Some(
                            interest_snapshot(simulation_snapshot(&simulation).await?, actor_id)?
                                .npcs
                                .into_iter()
                                .map(|npc| npc.id)
                                .collect::<BTreeSet<_>>(),
                        )
                    } else {
                        None
                    };
                    let events = batch
                        .events
                        .into_iter()
                        .filter(|event| {
                            event_involves_actor(event, actor_id)
                                || npc_effect_event_target(event).is_some_and(|npc_id| {
                                    visible_effect_npcs
                                        .as_ref()
                                        .is_some_and(|visible| visible.contains(&npc_id))
                                })
                        })
                        .collect::<Vec<_>>();
                    if !events.is_empty() {
                        let stream = event_stream
                            .as_mut()
                            .ok_or(NetworkError::EventStreamClosed)?;
                        write_control_frame(stream, &ControlMessage::Events(events)).await?;
                    }
                    None
                }
                chat = chat_messages.recv(), if selected_actor.is_some() => {
                    let message = match chat {
                        Ok(message) => message,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            return Err(NetworkError::ChatBackpressure);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            return Err(NetworkError::ChatStreamClosed);
                        }
                    };
                    write_control_frame(&mut send, &ControlMessage::ChatReceived(message)).await?;
                    None
                }
            };
            let Some(message) = message else {
                continue;
            };
            if !ingress.allow(Instant::now()) {
                rate_limit_violations = rate_limit_violations.saturating_add(1);
                if rate_limit_violations >= RATE_LIMIT_VIOLATIONS_BEFORE_CLOSE {
                    return Err(NetworkError::RateLimited);
                }
                continue;
            }
            rate_limit_violations = 0;
            match message {
                ControlMessage::AccountKeyRequest(AccountKeyRequest::List) => {
                    let key_persistence = persistence.clone();
                    let now = utc_now_seconds()?;
                    let response = match persistence_call(move || {
                        key_persistence.endpoint_bindings(account.id, remote, now)
                    })
                    .await
                    {
                        Ok(bindings) => AccountKeyResponse::Bindings(bindings),
                        Err(NetworkError::Persistence(error)) => {
                            AccountKeyResponse::Rejected(map_account_key_rejection(error)?)
                        }
                        Err(error) => return Err(error),
                    };
                    write_control_frame(&mut send, &ControlMessage::AccountKeyResponse(response))
                        .await?;
                }
                ControlMessage::AccountKeyRequest(AccountKeyRequest::Add { endpoint }) => {
                    let key_persistence = persistence.clone();
                    let now = utc_now_seconds()?;
                    let response = match persistence_call(move || {
                        key_persistence.add_pending_endpoint(account.id, remote, endpoint, now)
                    })
                    .await
                    {
                        Ok(binding) => AccountKeyResponse::Pending(binding),
                        Err(NetworkError::Persistence(error)) => {
                            AccountKeyResponse::Rejected(map_account_key_rejection(error)?)
                        }
                        Err(error) => return Err(error),
                    };
                    write_control_frame(&mut send, &ControlMessage::AccountKeyResponse(response))
                        .await?;
                }
                ControlMessage::AccountKeyRequest(AccountKeyRequest::Revoke { endpoint }) => {
                    let key_persistence = persistence.clone();
                    let now = utc_now_seconds()?;
                    let result = persistence_call(move || {
                        key_persistence.revoke_endpoint(account.id, remote, endpoint, now)
                    })
                    .await;
                    let succeeded = result.is_ok();
                    let revoked_current = succeeded && endpoint == remote;
                    let response = match result {
                        Ok(()) => AccountKeyResponse::Revoked { endpoint },
                        Err(NetworkError::Persistence(error)) => {
                            AccountKeyResponse::Rejected(map_account_key_rejection(error)?)
                        }
                        Err(error) => return Err(error),
                    };
                    if succeeded {
                        authorization_change_hub.publish_endpoint(account.id, endpoint);
                    }
                    write_control_frame(&mut send, &ControlMessage::AccountKeyResponse(response))
                        .await?;
                    if revoked_current {
                        break;
                    }
                }
                ControlMessage::CharacterRequest(CharacterRequest::List) => {
                    send_character_list(&mut send, &persistence, account.id).await?;
                }
                ControlMessage::CharacterRequest(CharacterRequest::Create { name, base_stats })
                    if selected_actor.is_none() =>
                {
                    match characters.create(account.id, name.clone(), base_stats).await {
                        Ok(actor_id) => {
                            session
                                .claim_actor(actor_id)
                                .map_err(|_| NetworkError::ServerBusy)?;
                            selected_actor = Some(actor_id);
                            selected_character = name;
                            committed_events = committed_event_hub.subscribe();
                            chat_messages = chat_hub.subscribe();
                            let snapshot = send_character_ready(&mut send, &simulation, actor_id).await?;
                            last_held_sequence = snapshot.controlled_actor.last_held_input_sequence;
                            held_input_active = snapshot.controlled_actor.held_movement.is_some();
                            last_held_input_at = Instant::now();
                            event_stream = Some(open_event_stream(connection, actor_id).await?);
                            send_snapshot_stream(connection, actor_id, 0, &snapshot).await?;
                            let (updates, task) = start_snapshot_output(
                                connection.clone(),
                                actor_id,
                                snapshot_failure_sender.clone(),
                            );
                            snapshot_updates = Some(updates);
                            snapshot_output_task = Some(task);
                        }
                        Err(CharacterCreationError::Persistence(error)) => {
                            write_control_frame(
                                &mut send,
                                &ControlMessage::GameplayRejected(map_gameplay_rejection(&error)),
                            )
                            .await?;
                        }
                        Err(CharacterCreationError::Simulation(error)) => {
                            return Err(NetworkError::Simulation(error));
                        }
                        Err(CharacterCreationError::Unavailable) => {
                            return Err(NetworkError::ServerBusy);
                        }
                    }
                }
                ControlMessage::CharacterRequest(CharacterRequest::Select { actor_id })
                    if selected_actor.is_none() =>
                {
                    let character_persistence = persistence.clone();
                    let character_name = persistence_call(move || {
                        character_persistence.characters_for_account(account.id)
                    })
                    .await
                    .map(|characters| {
                        characters
                            .into_iter()
                            .find(|character| character.actor_id == actor_id)
                            .map(|character| character.name)
                    })?;
                    let Some(character_name) = character_name else {
                        write_control_frame(
                            &mut send,
                            &ControlMessage::GameplayRejected(GameplayRejection::CharacterNotOwned),
                        )
                        .await?;
                        continue;
                    };
                    if let Err(error) = session.claim_actor(actor_id) {
                        write_control_frame(
                            &mut send,
                            &ControlMessage::GameplayRejected(map_session_rejection(error)),
                        )
                        .await?;
                        continue;
                    }
                    simulation_set_connected(&simulation, actor_id, true).await?;
                    selected_actor = Some(actor_id);
                    selected_character = character_name;
                    committed_events = committed_event_hub.subscribe();
                    chat_messages = chat_hub.subscribe();
                    let snapshot = send_character_ready(&mut send, &simulation, actor_id).await?;
                    last_held_sequence = snapshot.controlled_actor.last_held_input_sequence;
                    held_input_active = snapshot.controlled_actor.held_movement.is_some();
                    last_held_input_at = Instant::now();
                    event_stream = Some(open_event_stream(connection, actor_id).await?);
                    send_snapshot_stream(connection, actor_id, 0, &snapshot).await?;
                    let (updates, task) = start_snapshot_output(
                        connection.clone(),
                        actor_id,
                        snapshot_failure_sender.clone(),
                    );
                    snapshot_updates = Some(updates);
                    snapshot_output_task = Some(task);
                }
                ControlMessage::Command(command)
                    if selected_actor.is_some_and(|actor_id| actor_id == command.actor_id) =>
                {
                    let actor_id = selected_actor.ok_or(NetworkError::ServerBusy)?;
                    simulation_submit_durable(&simulation, command).await?;
                    let snapshot =
                        interest_snapshot(simulation_snapshot(&simulation).await?, actor_id)?;
                    queue_snapshot(&snapshot_updates, snapshot)?;
                }
                ControlMessage::Heartbeat { .. } => {
                    if let Some(actor_id) = selected_actor {
                        let snapshot = interest_snapshot(
                            simulation_snapshot(&simulation).await?,
                            actor_id,
                        )?;
                        queue_snapshot(&snapshot_updates, snapshot)?;
                    }
                }
                ControlMessage::ChatSend { text } if selected_actor.is_some() => {
                    let chat_authorization = persistence.clone();
                    let now = utc_now_seconds()?;
                    match persistence_call(move || {
                        chat_authorization.authorize_chat(account.id, remote, now)
                    })
                    .await
                    {
                        Ok(()) => {
                            let actor_id = selected_actor.ok_or(NetworkError::ServerBusy)?;
                            let tick = simulation_snapshot(&simulation).await?.tick;
                            chat_hub.publish(ChatMessage {
                                from_actor: actor_id,
                                from_character: selected_character.clone(),
                                text,
                                tick,
                            });
                        }
                        Err(NetworkError::Persistence(StoreError::AccountMuted(until_utc))) => {
                            write_control_frame(
                                &mut send,
                                &ControlMessage::ChatRejected(ChatRejection::Muted { until_utc }),
                            )
                            .await?;
                        }
                        Err(NetworkError::Persistence(
                            StoreError::PersistenceBusy
                            | StoreError::PersistenceTimeout
                            | StoreError::PersistenceUnavailable,
                        )) => {
                            write_control_frame(
                                &mut send,
                                &ControlMessage::ChatRejected(ChatRejection::ServerBusy),
                            )
                            .await?;
                        }
                        Err(NetworkError::Persistence(
                            StoreError::AccountUnavailable | StoreError::UnauthorizedEndpoint,
                        )) => return Err(NetworkError::AuthorizationRevoked),
                        Err(error) => return Err(error),
                    }
                }
                ControlMessage::ReportSubmit(report) if selected_actor.is_some() => {
                    let reporter_actor = selected_actor.ok_or(NetworkError::ServerBusy)?;
                    let reporting = persistence.clone();
                    let now = utc_now_seconds()?;
                    let response = match persistence_call(move || {
                        reporting.submit_report(account.id, remote, reporter_actor, report, now)
                    })
                    .await
                    {
                        Ok(report_id) => ReportResponse::Accepted { report_id },
                        Err(NetworkError::Persistence(StoreError::CannotReportSelf)) => {
                            ReportResponse::Rejected(ReportRejection::CannotReportSelf)
                        }
                        Err(NetworkError::Persistence(StoreError::CharacterUnavailable)) => {
                            ReportResponse::Rejected(ReportRejection::TargetUnavailable)
                        }
                        Err(NetworkError::Persistence(StoreError::InvalidReport)) => {
                            ReportResponse::Rejected(ReportRejection::InvalidReport)
                        }
                        Err(NetworkError::Persistence(StoreError::ReportRateLimited)) => {
                            ReportResponse::Rejected(ReportRejection::RateLimited)
                        }
                        Err(NetworkError::Persistence(
                            StoreError::PersistenceBusy
                            | StoreError::PersistenceTimeout
                            | StoreError::PersistenceUnavailable,
                        )) => ReportResponse::Rejected(ReportRejection::ServerBusy),
                        Err(NetworkError::Persistence(
                            StoreError::AccountUnavailable | StoreError::UnauthorizedEndpoint,
                        )) => return Err(NetworkError::AuthorizationRevoked),
                        Err(error) => return Err(error),
                    };
                    write_control_frame(&mut send, &ControlMessage::ReportResponse(response))
                        .await?;
                }
                _ => {
                    write_control_frame(
                        &mut send,
                        &ControlMessage::GameplayRejected(GameplayRejection::UnexpectedMessage),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
    .await;
    inbound_reader.abort();
    let _reader_result = inbound_reader.await;
    drop(snapshot_updates);
    if let Some(task) = snapshot_output_task {
        task.abort();
        let _task_result = task.await;
    }
    let disconnect_result = if let Some(actor_id) = selected_actor {
        let clear_result = simulation
            .submit_held_movement(HeldMovementUpdateV1 {
                actor_id,
                sequence: last_held_sequence,
                client_tick: simulation_snapshot(&simulation).await?.tick,
                direction: None,
                source: HeldMovementUpdateSource::Disconnected,
            })
            .map_err(|error| NetworkError::Simulation(error.to_string()));
        clear_result.and(simulation_set_connected(&simulation, actor_id, false).await)
    } else {
        Ok(())
    };
    session_result.and(disconnect_result)
}

async fn finish_gameplay_response(
    send: &mut iroh::endpoint::SendStream,
) -> Result<(), NetworkError> {
    send.finish()
        .map_err(|error| NetworkError::Transport(error.to_string()))?;
    tokio::time::timeout(AUTHORIZATION_TIMEOUT, send.stopped())
        .await
        .map_err(|_| NetworkError::AuthorizationTimeout)?
        .map_err(|error| NetworkError::Transport(error.to_string()))?;
    Ok(())
}

async fn open_event_stream(
    connection: &Connection,
    actor_id: ActorId,
) -> Result<iroh::endpoint::SendStream, NetworkError> {
    let mut stream = tokio::time::timeout(AUTHORIZATION_TIMEOUT, connection.open_uni())
        .await
        .map_err(|_| NetworkError::AuthorizationTimeout)?
        .map_err(|error| NetworkError::Transport(error.to_string()))?;
    write_control_frame(&mut stream, &ControlMessage::EventStreamReady { actor_id }).await?;
    Ok(stream)
}

async fn send_character_list(
    send: &mut iroh::endpoint::SendStream,
    persistence: &PersistenceHandle,
    account_id: cdda_protocol::AccountId,
) -> Result<(), NetworkError> {
    let persistence = persistence.clone();
    let characters =
        persistence_call(move || persistence.characters_for_account(account_id)).await?;
    write_control_frame(send, &ControlMessage::CharacterList(characters)).await?;
    Ok(())
}

async fn persistence_call<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, NetworkError> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| NetworkError::Persistence(StoreError::Io(std::io::Error::other(error))))?
        .map_err(NetworkError::Persistence)
}

async fn send_character_ready(
    send: &mut iroh::endpoint::SendStream,
    simulation: &SimulationHandle,
    actor_id: ActorId,
) -> Result<ReplicationSnapshotV1, NetworkError> {
    let snapshot = interest_snapshot(simulation_snapshot(simulation).await?, actor_id)?;
    write_control_frame(send, &ControlMessage::CharacterReady { actor_id }).await?;
    Ok(snapshot)
}

async fn send_snapshot_stream(
    connection: &Connection,
    actor_id: ActorId,
    sequence: u64,
    snapshot: &ReplicationSnapshotV1,
) -> Result<(), NetworkError> {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), connection.open_uni())
        .await
        .map_err(|_| NetworkError::SnapshotTimeout)?
        .map_err(|error| NetworkError::Transport(error.to_string()))?;
    tokio::time::timeout(
        Duration::from_secs(5),
        write_snapshot_stream(&mut stream, actor_id, sequence, snapshot),
    )
    .await
    .map_err(|_| NetworkError::SnapshotTimeout)??;
    Ok(())
}

fn start_snapshot_output(
    connection: Connection,
    actor_id: ActorId,
    failures: tokio::sync::mpsc::Sender<String>,
) -> (
    tokio::sync::watch::Sender<Option<ReplicationSnapshotV1>>,
    tokio::task::JoinHandle<()>,
) {
    let (updates, mut receiver) = tokio::sync::watch::channel(None);
    let task = tokio::spawn(async move {
        let mut sequence = 1_u64;
        while receiver.changed().await.is_ok() {
            let Some(snapshot) = receiver.borrow_and_update().clone() else {
                continue;
            };
            if let Err(error) =
                send_snapshot_stream(&connection, actor_id, sequence, &snapshot).await
            {
                let _send_result = failures.try_send(error.to_string());
                return;
            }
            let Some(next) = sequence.checked_add(1) else {
                let _send_result = failures.try_send(String::from("snapshot sequence overflow"));
                return;
            };
            sequence = next;
        }
    });
    (updates, task)
}

fn queue_snapshot(
    updates: &Option<tokio::sync::watch::Sender<Option<ReplicationSnapshotV1>>>,
    snapshot: ReplicationSnapshotV1,
) -> Result<(), NetworkError> {
    let updates = updates.as_ref().ok_or(NetworkError::ServerBusy)?;
    if updates.receiver_count() == 0 {
        return Err(NetworkError::SnapshotOutput(String::from(
            "snapshot output worker stopped",
        )));
    }
    let _superseded = updates.send_replace(Some(snapshot));
    Ok(())
}

fn require_datagram_support(connection: &Connection) -> Result<usize, NetworkError> {
    match connection.max_datagram_size() {
        Some(maximum) if maximum >= REQUIRED_DATAGRAM_SIZE => Ok(maximum.min(MAX_DATAGRAM_SIZE)),
        _ => Err(NetworkError::DatagramUnsupported),
    }
}

fn interest_snapshot(
    snapshot: WorldSnapshotV1,
    actor_id: ActorId,
) -> Result<ReplicationSnapshotV1, NetworkError> {
    let mut controlled_actor = snapshot
        .actors
        .iter()
        .find(|actor| actor.id == actor_id)
        .cloned()
        .ok_or_else(|| NetworkError::Simulation(String::from("controlled actor is absent")))?;
    let map_memory = std::mem::take(&mut controlled_actor.map_memory);
    let origin = controlled_actor.position;
    let actor_sleeping = controlled_actor.sleeping;
    let personal_detail_light = controlled_actor
        .inventory
        .iter()
        .any(|item| cdda_sim::powered_light_is_personal_detail(item_powered_light_emission(item)));
    let light_sources = active_light_sources(&snapshot);
    let center = origin.chunk_and_local().0;
    let relevant = |position: cdda_protocol::WorldPosition| {
        let chunk = position.chunk_and_local().0;
        chunk.x.abs_diff(center.x) <= CURRENT_INTEREST_RADIUS_SUBMAPS
            && chunk.y.abs_diff(center.y) <= CURRENT_INTEREST_RADIUS_SUBMAPS
            && chunk.z.abs_diff(center.z) <= 1
    };
    let visible = |position| {
        !actor_sleeping
            && relevant(position)
            && can_see(&snapshot, origin, position, &light_sources)
    };
    let visible_actors = snapshot
        .actors
        .iter()
        .filter(|actor| actor.id != actor_id && visible(actor.position))
        .map(|actor| VisibleActorSnapshot {
            id: actor.id,
            position: actor.position,
            hp: actor.hp,
            connected: actor.connected,
            sleeping: actor.sleeping,
        })
        .collect();
    let dialogue_npc_id = controlled_actor
        .pending_interaction
        .as_ref()
        .and_then(|interaction| match &interaction.context {
            cdda_protocol::InteractionContextV1::NpcDialogue { npc_id, .. } => Some(*npc_id),
            _ => None,
        });
    let npcs = snapshot
        .npcs
        .iter()
        .filter(|npc| visible(npc.position))
        .map(|npc| {
            let (faction_name, hostile_to_controlled_actor) =
                npc_faction::visible_npc_faction(&snapshot, npc);
            let maximum_hp = {
                let mut body_parts = npc.body_parts.clone();
                for part in &mut body_parts {
                    part.current_hp = part.maximum_hp;
                }
                actor_body_part_summary_hp(&snapshot.actor_anatomy, &body_parts).unwrap_or_default()
            };
            VisibleNpcSnapshotV1 {
                id: npc.id,
                template_id: npc.template_id.clone(),
                name: npc.name.clone(),
                faction_id: npc.faction_id.clone(),
                faction_name,
                hostile_to_controlled_actor,
                position: npc.position,
                hp: npc.hp,
                maximum_hp,
                profession: npc.profession.clone(),
                opinion_of_controlled_actor: (dialogue_npc_id == Some(npc.id)).then(|| {
                    npc.social
                        .iter()
                        .find(|social| social.actor_id == actor_id)
                        .map(|social| social.opinion.clone())
                        .unwrap_or_default()
                }),
            }
        })
        .collect();
    let creatures = snapshot
        .creatures
        .iter()
        .filter(|creature| creature.hp > 0 && visible(creature.position))
        .map(|creature| VisibleCreatureSnapshot {
            id: creature.id,
            type_id: creature.type_id.clone(),
            position: creature.position,
            hp: creature.hp,
            max_hp: creature.max_hp,
            friendly: creature.friendly == -1,
            pet: creature.pet,
            deploying_owner: creature.deploying_owner,
            faction_id: creature.faction_id.clone(),
        })
        .collect();
    let vehicles = visible_vehicles(&snapshot, origin, &visible)?;
    let ground_items = snapshot
        .ground_items
        .iter()
        .filter(|ground| visible(ground.position))
        .cloned()
        .collect();
    let chunks = snapshot
        .chunks
        .iter()
        .filter(|chunk| {
            chunk.coord.x.abs_diff(center.x) <= CURRENT_INTEREST_RADIUS_SUBMAPS
                && chunk.coord.y.abs_diff(center.y) <= CURRENT_INTEREST_RADIUS_SUBMAPS
                && chunk.coord.z.abs_diff(center.z) <= 1
        })
        .map(|chunk| {
            let tiles = chunk
                .tiles
                .iter()
                .enumerate()
                .map(|(index, tile)| {
                    let local_x = i32::try_from(index % cdda_protocol::SUBMAP_SIZE as usize)
                        .expect("submap x index always fits i32");
                    let local_y = i32::try_from(index / cdda_protocol::SUBMAP_SIZE as usize)
                        .expect("submap y index always fits i32");
                    let position = chunk
                        .coord
                        .x
                        .checked_mul(cdda_protocol::SUBMAP_SIZE)
                        .and_then(|x| x.checked_add(local_x))
                        .zip(
                            chunk
                                .coord
                                .y
                                .checked_mul(cdda_protocol::SUBMAP_SIZE)
                                .and_then(|y| y.checked_add(local_y)),
                        )
                        .map(|(x, y)| WorldPosition {
                            x,
                            y,
                            z: chunk.coord.z,
                        });
                    let currently_visible = !actor_sleeping
                        && position.is_some_and(|position| {
                            can_see(&snapshot, origin, position, &light_sources)
                        });
                    if currently_visible {
                        let furniture = chunk.furniture.get(index).cloned().flatten();
                        let admitted_furniture = furniture.as_ref().is_some_and(|furniture| {
                            snapshot
                                .furniture_bash_types
                                .binary_search_by(|candidate| {
                                    candidate.furniture_id.as_str().cmp(&furniture.furniture_id)
                                })
                                .is_ok()
                        });
                        let unsupported_furniture = furniture.as_ref().is_some_and(|furniture| {
                            snapshot
                                .furniture_bash_ids
                                .binary_search_by(|candidate| {
                                    candidate.as_str().cmp(&furniture.furniture_id)
                                })
                                .is_ok()
                                && !admitted_furniture
                        });
                        let bash_target = admitted_furniture
                            .then_some(BashTargetKindV1::Furniture)
                            .or_else(|| {
                                if unsupported_furniture {
                                    return None;
                                }
                                snapshot
                                    .terrain_bash_types
                                    .binary_search_by(|candidate| {
                                        candidate.terrain_id.as_str().cmp(&tile.terrain_id)
                                    })
                                    .is_ok()
                                    .then_some(BashTargetKindV1::Terrain)
                            });
                        let fields = chunk
                            .fields
                            .get(index)
                            .into_iter()
                            .flatten()
                            .filter_map(|field| {
                                let field_type = snapshot.field_types.iter().find(|candidate| {
                                    candidate.field_type_id == field.field_type_id
                                })?;
                                let level = field_type
                                    .intensity_levels
                                    .get(usize::from(field.intensity.checked_sub(1)?))?;
                                Some(cdda_protocol::FieldObservationV1 {
                                    field_type_id: field.field_type_id.clone(),
                                    intensity: field.intensity,
                                    name: level.name.clone(),
                                    symbol: level.symbol.clone(),
                                    color: level.color.clone(),
                                    dangerous: level.dangerous,
                                    transparent: level.transparent,
                                    priority: field_type.priority,
                                    display_field: field_type.display_field,
                                    display_sequence: field.display_sequence,
                                })
                            })
                            .collect();
                        Some(ObservedTerrainSnapshot {
                            terrain: tile.clone(),
                            furniture,
                            bash_target,
                            fields,
                            currently_visible: true,
                        })
                    } else {
                        map_memory
                            .iter()
                            .find(|memory| memory.coord == chunk.coord)
                            .and_then(|memory| memory.tiles.get(index))
                            .and_then(Option::as_ref)
                            .cloned()
                            .map(|remembered| ObservedTerrainSnapshot {
                                terrain: remembered.terrain,
                                furniture: remembered.furniture,
                                bash_target: None,
                                fields: Vec::new(),
                                currently_visible: false,
                            })
                    }
                })
                .collect();
            VisibleChunkSnapshot {
                coord: chunk.coord,
                tiles,
            }
        })
        .collect();
    let active_mission_types = controlled_actor
        .missions
        .iter()
        .map(|mission| mission.mission_type_id.as_str())
        .collect::<BTreeSet<_>>();
    let mission_definitions = snapshot
        .mission_definitions
        .iter()
        .filter(|definition| active_mission_types.contains(definition.mission_type_id.as_str()))
        .cloned()
        .collect();
    Ok(ReplicationSnapshotV1 {
        tick: snapshot.tick,
        calendar: CalendarSnapshot::at_tick(snapshot.tick),
        natural_light: NaturalLightSnapshot::at_tick(snapshot.tick),
        weather: cdda_sim::weather_observation_from_snapshot(&snapshot),
        detail_vision_available: cdda_sim::weather_adjusted_natural_sight_radius(&snapshot) >= 18
            || personal_detail_light
            || position_has_external_detail_light(&snapshot, origin, &light_sources),
        controlled_actor,
        item_place_monster_types: snapshot.item_place_monster_types.clone(),
        visible_actors,
        npcs,
        mission_definitions,
        creatures,
        vehicles,
        ground_items,
        chunks,
    })
}

fn visible_vehicles(
    snapshot: &WorldSnapshotV1,
    controlled_position: WorldPosition,
    visible: &impl Fn(WorldPosition) -> bool,
) -> Result<Vec<VisibleVehicleSnapshotV1>, NetworkError> {
    let Some(catalog) = snapshot.worldgen.as_ref() else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    for vehicle in &snapshot.vehicles {
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or_else(|| NetworkError::Simulation(String::from("vehicle prototype missing")))?;
        let mut displayed = BTreeMap::<WorldPosition, (i16, VisibleVehicleTileV1)>::new();
        let mut passengers = BTreeMap::new();
        let mut boardable_parts = BTreeMap::new();
        let mut openable_parts = BTreeMap::new();
        let mut cargo_parts = BTreeMap::new();
        let mut cargo = BTreeMap::<WorldPosition, Vec<cdda_protocol::ItemSnapshot>>::new();
        for (index, part) in vehicle.parts.iter().enumerate() {
            if let Some(passenger) = part.passenger {
                passengers.insert(part.position, passenger);
            }
            let prototype_part = prototype.parts.get(index).ok_or_else(|| {
                NetworkError::Simulation(String::from("vehicle prototype part missing"))
            })?;
            let part_type = catalog
                .vehicle_part_types
                .get(usize::from(prototype_part.part_type_index))
                .ok_or_else(|| {
                    NetworkError::Simulation(String::from("vehicle part type missing"))
                })?;
            if part.hp > 0
                && part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("BOARDABLE"))
                    .is_ok()
            {
                boardable_parts
                    .entry(part.position)
                    .or_insert(part.prototype_part_index);
            }
            if part.hp > 0
                && part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("OPENABLE"))
                    .is_ok()
            {
                openable_parts
                    .entry(part.position)
                    .or_insert(part.prototype_part_index);
            }
            if !part.locked
                && part.position.z == controlled_position.z
                && part.position.x.abs_diff(controlled_position.x) <= 1
                && part.position.y.abs_diff(controlled_position.y) <= 1
            {
                if part_type
                    .flags
                    .binary_search_by(|flag| flag.as_str().cmp("CARGO"))
                    .is_ok()
                {
                    cargo_parts
                        .entry(part.position)
                        .or_insert(part.prototype_part_index);
                }
                cargo
                    .entry(part.position)
                    .or_default()
                    .extend(part.cargo.iter().cloned());
            }
        }
        for (index, part) in vehicle.parts.iter().enumerate() {
            if !visible(part.position) {
                continue;
            }
            let prototype_part = prototype.parts.get(index).ok_or_else(|| {
                NetworkError::Simulation(String::from("vehicle prototype part missing"))
            })?;
            let part_type = catalog
                .vehicle_part_types
                .get(usize::from(prototype_part.part_type_index))
                .ok_or_else(|| {
                    NetworkError::Simulation(String::from("vehicle part type missing"))
                })?;
            let z_order = vehicle_part_location_z_order(&part_type.location);
            if z_order < 0
                || displayed
                    .get(&part.position)
                    .is_some_and(|(current, _)| *current >= z_order)
            {
                continue;
            }
            let variant = part_type
                .variants
                .iter()
                .find(|variant| variant.variant_id == prototype_part.variant_id)
                .ok_or_else(|| {
                    NetworkError::Simulation(String::from("vehicle part variant missing"))
                })?;
            let symbol = if part.open {
                String::from("'")
            } else {
                vehicle_part_symbol(
                    if part.hp == 0 {
                        &variant.broken_symbols
                    } else {
                        &variant.symbols
                    },
                    vehicle.facing_degrees,
                )?
            };
            displayed.insert(
                part.position,
                (
                    z_order,
                    VisibleVehicleTileV1 {
                        prototype_part_index: part.prototype_part_index,
                        position: part.position,
                        name: part_type.name.clone(),
                        symbol,
                        hp: part.hp,
                        maximum_hp: part_type.durability,
                        open: part.open,
                        boardable_prototype_part_index: boardable_parts
                            .get(&part.position)
                            .copied(),
                        openable_prototype_part_index: openable_parts.get(&part.position).copied(),
                        cargo_prototype_part_index: cargo_parts.get(&part.position).copied(),
                        passenger: passengers.get(&part.position).copied(),
                        cargo: cargo.remove(&part.position).unwrap_or_default(),
                    },
                ),
            );
        }
        let tiles = displayed
            .into_values()
            .map(|(_, tile)| tile)
            .collect::<Vec<_>>();
        if !tiles.is_empty() {
            output.push(VisibleVehicleSnapshotV1 {
                id: vehicle.id,
                prototype_id: prototype.prototype_id.clone(),
                name: prototype.name.clone(),
                origin: vehicle.origin,
                facing_degrees: vehicle.facing_degrees,
                tiles,
            });
        }
    }
    Ok(output)
}

fn vehicle_part_location_z_order(location: &str) -> i16 {
    match location {
        "armor" => -2,
        "roof" | "on_frame" | "axle" | "on_ceiling" | "on_controls" | "on_lockable_cargo"
        | "on_seat" | "on_windshield" => -1,
        "fuel_source" | "on_battery_mount" => 3,
        "engine_block" => 4,
        "structure" => 5,
        "under" => 6,
        "center" => 7,
        "on_cargo" => 8,
        "on_roof" => 9,
        _ => -1,
    }
}

fn vehicle_part_symbol(symbols: &str, facing_degrees: i16) -> Result<String, NetworkError> {
    let symbols = symbols.chars().collect::<Vec<_>>();
    let index = if symbols.len() == 1 {
        0
    } else if symbols.len() == 8 {
        let display_degrees = (270_i16 - facing_degrees).rem_euclid(360);
        usize::try_from((display_degrees + 22).rem_euclid(360) / 45)
            .expect("normalized vehicle direction fits usize")
    } else {
        return Err(NetworkError::Simulation(String::from(
            "vehicle directional symbol count is unsupported",
        )));
    };
    symbols
        .get(index)
        .map(char::to_string)
        .ok_or_else(|| NetworkError::Simulation(String::from("vehicle symbol missing")))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveLightSource {
    position: WorldPosition,
    sight_radius: u32,
    external_detail_radius: Option<u32>,
}

fn active_light_sources(snapshot: &WorldSnapshotV1) -> Vec<ActiveLightSource> {
    let mut sources = BTreeMap::<WorldPosition, (u32, Option<u32>)>::new();
    let mut add_source = |position: WorldPosition, item: &cdda_protocol::ItemSnapshot| {
        let light_emission = item_powered_light_emission(item);
        if light_emission == 0 {
            return;
        }
        let sight_radius = cdda_sim::powered_light_sight_radius(light_emission);
        let external_detail_radius = cdda_sim::powered_light_external_detail_radius(light_emission);
        if sight_radius == 0 && external_detail_radius.is_none() {
            return;
        }
        let entry = sources.entry(position).or_insert((0, None));
        entry.0 = entry.0.max(sight_radius);
        entry.1 = match (entry.1, external_detail_radius) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (left @ Some(_), None) => left,
            (None, right) => right,
        };
    };
    for actor in &snapshot.actors {
        for item in &actor.inventory {
            add_source(actor.position, item);
        }
    }
    for ground in &snapshot.ground_items {
        add_source(ground.position, &ground.item);
    }
    sources
        .into_iter()
        .map(
            |(position, (sight_radius, external_detail_radius))| ActiveLightSource {
                position,
                sight_radius,
                external_detail_radius,
            },
        )
        .collect()
}

fn item_powered_light_emission(item: &cdda_protocol::ItemSnapshot) -> u16 {
    let Some(powered) = item.powered_tool.as_ref().filter(|powered| powered.active) else {
        return 0;
    };
    let Some(magazine) = item
        .magazine_wells
        .iter()
        .find(|well| well.pocket_index == powered.power_pocket_index)
        .and_then(|well| well.installed_magazine.as_deref())
    else {
        return 0;
    };
    let Some(stored_energy) = u64::try_from(magazine.charges)
        .ok()
        .and_then(|charges| {
            charges.checked_mul(u64::from(cdda_protocol::MILLIJOULES_PER_BATTERY_CHARGE))
        })
        .and_then(|energy| energy.checked_add(u64::from(magazine.residual_energy_millijoules)))
    else {
        return 0;
    };
    cdda_sim::powered_light_effective_emission(
        powered.light_emission,
        powered.dims_with_charge,
        stored_energy,
        magazine.magazine_capacity,
    )
}

fn position_is_lit(
    snapshot: &WorldSnapshotV1,
    target: WorldPosition,
    light_sources: &[ActiveLightSource],
) -> bool {
    light_sources.iter().copied().any(|source| {
        source.position.z == target.z
            && source
                .position
                .x
                .abs_diff(target.x)
                .max(source.position.y.abs_diff(target.y))
                <= source.sight_radius
            && has_clear_line(snapshot, source.position, target)
    })
}

fn position_has_external_detail_light(
    snapshot: &WorldSnapshotV1,
    target: WorldPosition,
    light_sources: &[ActiveLightSource],
) -> bool {
    light_sources.iter().copied().any(|source| {
        source.external_detail_radius.is_some_and(|radius| {
            source.position.z == target.z
                && source
                    .position
                    .x
                    .abs_diff(target.x)
                    .max(source.position.y.abs_diff(target.y))
                    <= radius
                && has_clear_line(snapshot, source.position, target)
        })
    })
}

fn can_see(
    snapshot: &WorldSnapshotV1,
    origin: WorldPosition,
    target: WorldPosition,
    light_sources: &[ActiveLightSource],
) -> bool {
    let sight_radius = u32::from(cdda_sim::weather_adjusted_natural_sight_radius(snapshot));
    if origin.z != target.z
        || origin.x.abs_diff(target.x) > CURRENT_VISION_RADIUS_TILES
        || origin.y.abs_diff(target.y) > CURRENT_VISION_RADIUS_TILES
    {
        return false;
    }
    if !has_clear_line(snapshot, origin, target) {
        return false;
    }
    (origin.x.abs_diff(target.x) <= sight_radius && origin.y.abs_diff(target.y) <= sight_radius)
        || position_is_lit(snapshot, target, light_sources)
}

fn has_clear_line(
    snapshot: &WorldSnapshotV1,
    origin: WorldPosition,
    target: WorldPosition,
) -> bool {
    if origin.z != target.z {
        return false;
    }
    let (mut x, mut y) = (origin.x, origin.y);
    let dx = i64::from(target.x).saturating_sub(i64::from(x)).abs();
    let sx = if x < target.x { 1 } else { -1 };
    let dy = -i64::from(target.y).saturating_sub(i64::from(y)).abs();
    let sy = if y < target.y { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        if x == target.x && y == target.y {
            return true;
        }
        let doubled = error.saturating_mul(2);
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
        if x == target.x && y == target.y {
            return true;
        }
        let position = WorldPosition { x, y, z: origin.z };
        let (chunk_coord, local) = position.chunk_and_local();
        let index =
            usize::from(local.y) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(local.x);
        let Some(chunk) = snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coord == chunk_coord)
        else {
            return false;
        };
        let Some(tile) = chunk.tiles.get(index) else {
            return false;
        };
        let furniture_transparent = chunk
            .furniture
            .get(index)
            .and_then(Option::as_ref)
            .is_none_or(|furniture| furniture.transparent);
        if !tile.transparent || !furniture_transparent {
            return false;
        }
    }
}

fn map_gameplay_rejection(error: &StoreError) -> GameplayRejection {
    match error {
        StoreError::InvalidCharacterName => GameplayRejection::InvalidCharacterName,
        StoreError::CharacterAlreadyExists => GameplayRejection::CharacterAlreadyExists,
        StoreError::AccountUnavailable => GameplayRejection::AuthenticationRequired,
        _ => GameplayRejection::ServerBusy,
    }
}

fn map_account_key_rejection(error: StoreError) -> Result<AccountKeyRejection, NetworkError> {
    match error {
        StoreError::AccountUnavailable => Ok(AccountKeyRejection::AccountUnavailable),
        StoreError::EndpointAlreadyBound => Ok(AccountKeyRejection::EndpointAlreadyBound),
        StoreError::InvalidEndpointIdentity => Ok(AccountKeyRejection::InvalidEndpoint),
        StoreError::EndpointNotRevocable => Ok(AccountKeyRejection::EndpointNotRevocable),
        StoreError::CannotRevokeLastEndpoint => Ok(AccountKeyRejection::LastActiveEndpoint),
        StoreError::TooManyEndpointBindings => Ok(AccountKeyRejection::TooManyBindings),
        StoreError::PersistenceBusy
        | StoreError::PersistenceTimeout
        | StoreError::PersistenceUnavailable => Ok(AccountKeyRejection::ServerBusy),
        other => Err(NetworkError::Persistence(other)),
    }
}

fn admin_account_summary(account: AccountRecord) -> AdminAccountSummary {
    AdminAccountSummary {
        account_id: account.id,
        display_name: account.display_name,
        role: account.role,
        status: account.status,
        suspended_until_utc: account.suspended_until_utc,
        muted_until_utc: account.muted_until_utc,
    }
}

fn private_character_inspection(
    snapshot: &WorldSnapshotV1,
    identity: AdminCharacterIdentity,
    inventory_after: Option<ItemId>,
    inventory_limit: u16,
) -> Option<PrivateCharacterInspection> {
    let actor = snapshot
        .actors
        .iter()
        .find(|actor| actor.id == identity.actor_id)?;
    let inventory_total = u16::try_from(actor.inventory.len()).ok()?;
    let map_memory_chunks = u32::try_from(actor.map_memory.len()).ok()?;
    let learned_recipe_count = u16::try_from(actor.learned_recipes.len()).ok()?;
    let mut inventory = actor
        .inventory
        .iter()
        .filter(|item| inventory_after.is_none_or(|after| item.id > after))
        .cloned()
        .collect::<Vec<_>>();
    inventory.sort_by_key(|item| item.id);
    let has_more = inventory.len() > usize::from(inventory_limit);
    inventory.truncate(usize::from(inventory_limit));
    let next_inventory_after = has_more.then(|| {
        inventory
            .last()
            .expect("an overflowing inventory page has a returned item")
            .id
    });
    let mut inspection = PrivateCharacterInspection {
        tick: snapshot.tick,
        account_id: identity.account_id,
        actor_id: identity.actor_id,
        name: identity.name,
        position: actor.position,
        hp: actor.hp,
        base_strength: actor.base_strength,
        base_dexterity: actor.base_dexterity,
        base_intelligence: actor.base_intelligence,
        base_perception: actor.base_perception,
        connected: actor.connected,
        last_command_sequence: actor.last_command_sequence,
        last_held_input_sequence: actor.last_held_input_sequence,
        held_movement: actor.held_movement,
        wielded: actor.wielded,
        stored_kcal: actor.stored_kcal,
        thirst: actor.thirst,
        sleepiness: actor.sleepiness,
        sleeping: actor.sleeping,
        sleep_intervals: actor.sleep_intervals,
        speed: actor.speed,
        action_points: actor.action_points,
        queued_actions: actor.queued_actions.clone(),
        craft_activity: actor.craft_activity.clone(),
        read_activity: actor.read_activity.clone(),
        disassembly_activity: actor.disassembly_activity.clone(),
        construction_activity: actor.construction_activity.clone(),
        learned_recipe_count,
        skills: actor.skills.clone(),
        proficiencies: actor.proficiencies.clone(),
        inventory_total,
        inventory,
        next_inventory_after,
        map_memory_chunks,
    };
    loop {
        let response = ControlMessage::AdminResponse(AdminResponse::PrivateCharacter(Box::new(
            inspection.clone(),
        )));
        match encode_control(&response) {
            Ok(_) => return Some(inspection),
            Err(FrameError::EncodedTooLarge { .. }) => {
                inspection.inventory.pop()?;
                inspection.next_inventory_after = inspection.inventory.last().map(|item| item.id);
            }
            Err(_) => return None,
        }
    }
}

fn map_admin_rejection(error: StoreError) -> Result<AdminRejection, NetworkError> {
    match error {
        StoreError::UnauthorizedEndpoint => Ok(AdminRejection::AuthenticationRequired),
        StoreError::AdministratorRequired => Ok(AdminRejection::AdministratorRequired),
        StoreError::ModeratorRequired => Ok(AdminRejection::ModeratorRequired),
        StoreError::AccountUnavailable | StoreError::InvalidStableId => {
            Ok(AdminRejection::AccountUnavailable)
        }
        StoreError::CannotTargetSelf => Ok(AdminRejection::CannotTargetSelf),
        StoreError::InvalidAccountTransition => Ok(AdminRejection::InvalidTransition),
        StoreError::CannotRemoveLastAdministrator => Ok(AdminRejection::LastAdministrator),
        StoreError::TargetRoleNotAllowed => Ok(AdminRejection::TargetRoleNotAllowed),
        StoreError::CharacterUnavailable => Ok(AdminRejection::CharacterUnavailable),
        StoreError::CharacterNameConflict => Ok(AdminRejection::CharacterNameConflict),
        StoreError::TooManyCharacters => Ok(AdminRejection::TooManyCharacters),
        StoreError::InvalidDisplayName => Ok(AdminRejection::InvalidDisplayName),
        StoreError::InvalidEndpointIdentity => Ok(AdminRejection::InvalidEndpoint),
        StoreError::EndpointAlreadyBound => Ok(AdminRejection::EndpointAlreadyBound),
        StoreError::EndpointNotRevocable => Ok(AdminRejection::EndpointNotRevocable),
        StoreError::CannotRevokeLastEndpoint => Ok(AdminRejection::LastActiveEndpoint),
        StoreError::TooManyEndpointBindings => Ok(AdminRejection::TooManyBindings),
        StoreError::InvalidModerationDuration | StoreError::InvalidReport => {
            Ok(AdminRejection::InvalidTransition)
        }
        StoreError::PersistenceBusy
        | StoreError::PersistenceTimeout
        | StoreError::PersistenceUnavailable => Ok(AdminRejection::ServerBusy),
        other => Err(NetworkError::Persistence(other)),
    }
}

const fn map_session_rejection(error: SessionClaimError) -> GameplayRejection {
    match error {
        SessionClaimError::AlreadyActive => GameplayRejection::SessionAlreadyActive,
        SessionClaimError::Full => GameplayRejection::ServerFull,
        SessionClaimError::Unavailable => GameplayRejection::ServerBusy,
    }
}

async fn simulation_snapshot(
    simulation: &SimulationHandle,
) -> Result<WorldSnapshotV1, NetworkError> {
    let simulation = simulation.clone();
    tokio::task::spawn_blocking(move || simulation.snapshot(Duration::from_secs(1)))
        .await
        .map_err(|error| NetworkError::Simulation(error.to_string()))?
        .map_err(|error| NetworkError::Simulation(error.to_string()))
}

async fn simulation_submit_durable(
    simulation: &SimulationHandle,
    command: ClientCommand,
) -> Result<SimTick, NetworkError> {
    let receipt = simulation.submit_durable(command).map_err(|error| {
        if error == SubmitError::Backpressure {
            NetworkError::ServerBusy
        } else {
            NetworkError::Simulation(error.to_string())
        }
    })?;
    tokio::task::spawn_blocking(move || receipt.wait(Duration::from_secs(2)))
        .await
        .map_err(|error| NetworkError::Simulation(error.to_string()))?
        .map_err(|error| NetworkError::Simulation(error.to_string()))
}

fn event_involves_actor(event: &WorldEvent, actor_id: ActorId) -> bool {
    match event.kind {
        WorldEventKind::ActorBoardedVehicle {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorUnboardedVehicle {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::VehicleCargoTaken {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::VehicleCargoStored {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::VehiclePartOpenChanged {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorMoved {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CommandRejected {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ConnectionChanged {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemPickedUp {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemDropped {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemWielded {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemWorn {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemTakenOff {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemConsumed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::MedicalItemApplied {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::EocMessage {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::EocItemActivated {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ItemTransformed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CreatureDeployed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::InteractionRequested {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::InteractionCanceled {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorNeedsUpdated {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorDiedFromNeeds {
            actor_id: event_actor,
        }
        | WorldEventKind::ActorAffectedByField {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorDamagedByEffect {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorDiedFromEffect {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::TerrainChanged {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorBashed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::WeaponReloaded {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::MagazineReloaded {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::AmmunitionLoadedIntoPocket {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::AmmunitionInsertedIntoContainer {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::PocketItemRemoved {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorFellAsleep {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ActorWokeUp {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CraftStarted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CraftInterrupted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CraftResumed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CraftCanceled {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CraftCompleted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::CraftToolChargesConsumed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::BookStudyStarted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::BookStudyInterrupted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::BookStudyResumed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::BookStudyCanceled {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::BookStudyCompleted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::DisassemblyStarted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::DisassemblyInterrupted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::DisassemblyResumed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::DisassemblyCanceled {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::DisassemblyCompleted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ConstructionStarted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ConstructionInterrupted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ConstructionResumed {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ConstructionCanceled {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ConstructionCompleted {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::RecipeLearned {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::SkillLevelGained {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::ProficiencyLearned {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::MissionAssigned {
            actor_id: event_actor,
            ..
        }
        | WorldEventKind::MissionFinished {
            actor_id: event_actor,
            ..
        } => event_actor == actor_id,
        WorldEventKind::PoweredToolChanged {
            actor_id: event_actor,
            ..
        } => event_actor == Some(actor_id),
        WorldEventKind::DamageApplied { source, target, .. }
        | WorldEventKind::ActorMissedActor { source, target }
        | WorldEventKind::ActorDied {
            actor_id: target,
            killer: source,
        } => source == actor_id || target == actor_id,
        WorldEventKind::CreatureDamaged { source, .. }
        | WorldEventKind::ActorMissedCreature { source, .. }
        | WorldEventKind::CreatureDied { killer: source, .. }
        | WorldEventKind::NpcDamaged { source, .. }
        | WorldEventKind::ActorMissedNpc { source, .. }
        | WorldEventKind::NpcDied { killer: source, .. } => source == actor_id,
        WorldEventKind::ActorDamagedByCreature { target, .. }
        | WorldEventKind::ActorKilledByCreature {
            actor_id: target, ..
        }
        | WorldEventKind::ActorDamagedByNpc { target, .. }
        | WorldEventKind::NpcMissedActor { target, .. }
        | WorldEventKind::ActorKilledByNpc {
            actor_id: target, ..
        } => target == actor_id,
        WorldEventKind::CreatureMissedActor {
            target,
            target_was_sleeping,
            ..
        } => target == actor_id && !target_was_sleeping,
        WorldEventKind::RangedAttackResolved { source, target, .. } => {
            source == actor_id
                || matches!(target, cdda_protocol::RangedTarget::Actor(target) if target == actor_id)
        }
        WorldEventKind::CreatureRangedAttackResolved { target, .. } => target == actor_id,
        WorldEventKind::CreatureTargetedActor { target, .. } => target == actor_id,
        WorldEventKind::VehicleSpawned { .. }
        | WorldEventKind::CreatureMoved { .. }
        | WorldEventKind::CreatureDamagedByCreature { .. }
        | WorldEventKind::CreatureKilledByCreature { .. }
        | WorldEventKind::CreatureCorpseCreated { .. }
        | WorldEventKind::CreatureRevived { .. }
        | WorldEventKind::CreaturePolymorphed { .. }
        | WorldEventKind::CreatureSummoned { .. }
        | WorldEventKind::CreatureBashed { .. }
        | WorldEventKind::CreatureOpenedTerrain { .. }
        | WorldEventKind::NpcMoved { .. }
        | WorldEventKind::NpcDamagedByEffect { .. }
        | WorldEventKind::NpcKilledByEffect { .. }
        | WorldEventKind::FieldIntensityChanged { .. } => false,
    }
}

fn npc_effect_event_target(event: &WorldEvent) -> Option<cdda_protocol::NpcId> {
    match &event.kind {
        WorldEventKind::NpcDamagedByEffect { npc_id, .. }
        | WorldEventKind::NpcKilledByEffect { npc_id, .. } => Some(*npc_id),
        _ => None,
    }
}

async fn simulation_set_connected(
    simulation: &SimulationHandle,
    actor_id: ActorId,
    connected: bool,
) -> Result<(), NetworkError> {
    let simulation = simulation.clone();
    tokio::task::spawn_blocking(move || {
        simulation.set_connected(actor_id, connected, Duration::from_secs(1))
    })
    .await
    .map_err(|error| NetworkError::Simulation(error.to_string()))?
    .map_err(|error| NetworkError::Simulation(error.to_string()))
}

fn map_enrollment_rejection(error: &StoreError) -> EnrollmentRejection {
    match error {
        StoreError::UnknownEndpoint | StoreError::UnauthorizedEndpoint => {
            EnrollmentRejection::UnknownIdentity
        }
        StoreError::EnrollmentExpired => EnrollmentRejection::Expired,
        _ => EnrollmentRejection::AccountUnavailable,
    }
}

fn utc_now_seconds() -> Result<i64, NetworkError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| NetworkError::ClockBeforeEpoch)?;
    i64::try_from(elapsed.as_secs()).map_err(|_| NetworkError::ClockOverflow)
}

#[derive(Debug)]
pub enum NetworkError {
    AdminConnectionStale,
    AdministratorRequired,
    AuthorizationRevoked,
    AuthorizationTimeout,
    ClockBeforeEpoch,
    ClockOverflow,
    ChatBackpressure,
    ChatStreamClosed,
    DatagramUnsupported,
    EventBackpressure,
    EventStreamClosed,
    Frame(String),
    FrameTooLarge,
    HeartbeatTimeout,
    InvalidDatagram,
    ModeratorRequired,
    Persistence(StoreError),
    RateLimited,
    ServerBusy,
    Simulation(String),
    SnapshotOutput(String),
    SnapshotTimeout,
    Transport(String),
    UnauthorizedIdentity,
    UnexpectedMessage,
    WrongAlpn,
}

impl fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AdminConnectionStale => {
                formatter.write_str("administration connection exceeded its five-minute lifetime")
            }
            Self::AdministratorRequired => {
                formatter.write_str("iroh identity does not hold the administrator role")
            }
            Self::AuthorizationRevoked => {
                formatter.write_str("connection authorization changed or was revoked")
            }
            Self::AuthorizationTimeout => formatter.write_str("connection authorization timed out"),
            Self::ClockBeforeEpoch => formatter.write_str("system clock is before the Unix epoch"),
            Self::ClockOverflow => formatter.write_str("system clock exceeds supported range"),
            Self::ChatBackpressure => formatter.write_str("client fell behind the chat stream"),
            Self::ChatStreamClosed => formatter.write_str("server chat stream closed"),
            Self::DatagramUnsupported => formatter
                .write_str("iroh connection does not support the required 1,024-byte datagrams"),
            Self::EventBackpressure => {
                formatter.write_str("client fell behind the committed event stream")
            }
            Self::EventStreamClosed => formatter.write_str("committed event stream closed"),
            Self::Frame(error) => write!(formatter, "invalid control frame: {error}"),
            Self::FrameTooLarge => formatter.write_str("control frame exceeds its size limit"),
            Self::HeartbeatTimeout => formatter.write_str("gameplay heartbeat timed out"),
            Self::InvalidDatagram => formatter.write_str("invalid gameplay datagram"),
            Self::ModeratorRequired => {
                formatter.write_str("iroh identity does not hold a moderation role")
            }
            Self::Persistence(error) => {
                write!(formatter, "authorization persistence error: {error}")
            }
            Self::RateLimited => formatter.write_str("control ingress rate limit exceeded"),
            Self::ServerBusy => formatter.write_str("server is busy"),
            Self::Simulation(error) => write!(formatter, "simulation runtime error: {error}"),
            Self::SnapshotOutput(error) => write!(formatter, "snapshot output failed: {error}"),
            Self::SnapshotTimeout => formatter.write_str("snapshot stream made no progress"),
            Self::Transport(error) => write!(formatter, "iroh transport error: {error}"),
            Self::UnauthorizedIdentity => formatter.write_str("iroh identity is not authorized"),
            Self::UnexpectedMessage => formatter.write_str("unexpected authorization message"),
            Self::WrongAlpn => formatter.write_str("connection used the wrong ALPN"),
        }
    }
}

impl std::error::Error for NetworkError {}

impl From<FrameIoError> for NetworkError {
    fn from(error: FrameIoError) -> Self {
        Self::Frame(error.to_string())
    }
}

#[must_use]
pub const fn ticks_per_second() -> u64 {
    SimTick::HZ
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use cdda_protocol::{
        AccountId, AccountRole, BASELINE_COMMIT, CharacterRequest, ClientDatagramV1, ClientHello,
        CommandKind, CommandSequence, ContentIdentity, HeldInputSequence, HeldMovementInputV1,
        HorizontalDirection, LocalTileCoord, RangedWeaponSnapshot, ReportReason,
        TerrainTileSnapshot, encode_client_datagram,
    };
    use cdda_sim::{Chunk, CreatureSpawn, ReservedIdBlock};

    use super::*;

    #[test]
    fn creature_miss_event_respects_sleeping_message_boundary() {
        let target = ActorId::new(1, 1);
        let other = ActorId::new(1, 2);
        let event = WorldEvent {
            id: cdda_protocol::EventId::new(1, 3),
            tick: SimTick(4),
            kind: WorldEventKind::CreatureMissedActor {
                source: cdda_protocol::CreatureId::new(1, 5),
                target,
                stumbled: true,
                target_was_sleeping: true,
            },
        };
        assert!(!event_involves_actor(&event, target));
        assert!(!event_involves_actor(&event, other));
        let mut awake_event = event;
        if let WorldEventKind::CreatureMissedActor {
            target_was_sleeping,
            ..
        } = &mut awake_event.kind
        {
            *target_was_sleeping = false;
        }
        assert!(event_involves_actor(&awake_event, target));
        assert!(!event_involves_actor(&awake_event, other));
    }

    fn server_test_recipe(recipe_id: &str, output: &str) -> CraftRecipeV1 {
        CraftRecipeV1 {
            recipe_id: recipe_id.to_owned(),
            time_moves: 100,
            output_instances: 1,
            output: cdda_protocol::CraftItemPrototypeV1 {
                type_id: output.to_owned(),
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
                containment: Default::default(),
            },
            retain_components: false,
            byproducts: Vec::new(),
            components: vec![vec![cdda_protocol::CraftComponentRequirementV1 {
                type_id: String::from("rock"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }]],
            tools: Vec::new(),
            qualities: Vec::new(),
            proficiencies: Vec::new(),
            primary_skill: None,
            required_skills: Vec::new(),
            can_be_learned: false,
            autolearn: true,
            autolearn_skills: Vec::new(),
            book_requirements: Vec::new(),
        }
    }

    fn server_test_disassembly_recipe(
        recipe_id: &str,
        target_type_id: &str,
        component_type_id: &str,
    ) -> DisassemblyRecipeV1 {
        DisassemblyRecipeV1 {
            recipe_id: recipe_id.to_owned(),
            target_type_id: target_type_id.to_owned(),
            time_moves: 100,
            difficulty: 0,
            primary_skill_id: None,
            learn_requirements: Vec::new(),
            autolearn: false,
            autolearn_requirements: Vec::new(),
            unload_charges_as: None,
            requires_empty_charges: false,
            components: vec![cdda_protocol::DisassemblyComponentV1 {
                output_instances: 1,
                count_by_charges: false,
                output: cdda_protocol::CraftItemPrototypeV1 {
                    type_id: component_type_id.to_owned(),
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
                    containment: Default::default(),
                },
                output_state: None,
            }],
            tools: Vec::new(),
            qualities: Vec::new(),
        }
    }

    fn server_test_construction(construction_id: &str, furniture_id: &str) -> ConstructionRecipeV1 {
        ConstructionRecipeV1 {
            construction_id: construction_id.to_owned(),
            name: String::from("Place test furniture"),
            time_moves: 100,
            required_skills: Vec::new(),
            components: vec![vec![cdda_protocol::CraftComponentRequirementV1 {
                type_id: String::from("test_component"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }]],
            qualities: Vec::new(),
            pre_terrain: Vec::new(),
            requires_empty: true,
            result: cdda_protocol::ConstructionResultV1::Furniture(
                cdda_protocol::FurnitureTileSnapshot {
                    furniture_id: furniture_id.to_owned(),
                    move_cost_mod: 0,
                    transparent: true,
                    blocks_door: false,
                    comfort: 0,
                    floor_bedding_warmth: 0,
                },
            ),
        }
    }

    #[test]
    fn crafting_catalog_replaces_untrusted_payload_and_clears_unknown_recipe() {
        let authoritative = server_test_recipe("known", "authoritative_output");
        let catalog = CraftingCatalog::new(BTreeMap::from([(
            String::from("known"),
            authoritative.clone(),
        )]));
        let mut command = ClientCommand {
            actor_id: ActorId::new(1, 1),
            sequence: CommandSequence(1),
            client_tick: SimTick(0),
            kind: CommandKind::Craft {
                recipe_id: String::from("known"),
                recipe: Some(Box::new(server_test_recipe("known", "attacker_output"))),
            },
        };
        catalog.normalize(&mut command);
        assert!(matches!(
            command.kind,
            CommandKind::Craft { recipe: Some(recipe), .. }
                if *recipe == authoritative
        ));

        let mut unknown = ClientCommand {
            kind: CommandKind::Craft {
                recipe_id: String::from("unknown"),
                recipe: Some(Box::new(server_test_recipe("unknown", "attacker_output"))),
            },
            ..command
        };
        catalog.normalize(&mut unknown);
        assert!(matches!(
            unknown.kind,
            CommandKind::Craft { recipe: None, .. }
        ));
    }

    #[test]
    fn construction_catalog_replaces_untrusted_payload_and_clears_unknown_id() {
        let authoritative = server_test_construction("known", "f_table");
        let catalog = ConstructionCatalog::new(BTreeMap::from([(
            String::from("known"),
            authoritative.clone(),
        )]));
        let mut command = ClientCommand {
            actor_id: ActorId::new(1, 1),
            sequence: CommandSequence(1),
            client_tick: SimTick(0),
            kind: CommandKind::Construct {
                target: WorldPosition { x: 2, y: 1, z: 0 },
                construction_id: String::from("known"),
                construction: Some(Box::new(server_test_construction("known", "f_forged"))),
            },
        };
        catalog.normalize(&mut command);
        assert!(matches!(
            command.kind,
            CommandKind::Construct {
                construction: Some(recipe),
                ..
            } if *recipe == authoritative
        ));

        let mut unknown = ClientCommand {
            kind: CommandKind::Construct {
                target: WorldPosition { x: 2, y: 1, z: 0 },
                construction_id: String::from("unknown"),
                construction: Some(Box::new(server_test_construction("unknown", "f_forged"))),
            },
            ..command
        };
        catalog.normalize(&mut unknown);
        assert!(matches!(
            unknown.kind,
            CommandKind::Construct {
                construction: None,
                ..
            }
        ));
    }

    #[test]
    fn disassembly_catalog_replaces_untrusted_payload_and_clears_unknown_item() {
        let mut authoritative =
            server_test_disassembly_recipe("known_recipe", "known_item", "rock");
        authoritative.unload_charges_as = Some(cdda_protocol::CraftItemPrototypeV1 {
            type_id: String::from("test_round"),
            charges: 1,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            tracks_temperature: false,
            thermal_properties: None,
            ammunition_type: String::from("test_ammo"),
            ranged_weapon: None,
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            containment: Default::default(),
        });
        let catalog = DisassemblyCatalog::new(BTreeMap::from([(
            String::from("known_item"),
            authoritative.clone(),
        )]));
        let mut command = ClientCommand {
            actor_id: ActorId::new(1, 1),
            sequence: CommandSequence(1),
            client_tick: SimTick(0),
            kind: CommandKind::Disassemble {
                item_id: ItemId::new(1, 2),
                item_type_id: String::from("known_item"),
                recipe: Some(Box::new(server_test_disassembly_recipe(
                    "forged_recipe",
                    "known_item",
                    "plutonium",
                ))),
            },
        };
        catalog.normalize(&mut command);
        assert!(matches!(
            &command.kind,
            CommandKind::Disassemble {
                recipe: Some(recipe),
                ..
            } if recipe.as_ref() == &authoritative
        ));
        if let CommandKind::Disassemble {
            item_type_id,
            recipe,
            ..
        } = &mut command.kind
        {
            *item_type_id = String::from("unknown_item");
            *recipe = Some(Box::new(authoritative));
        }
        catalog.normalize(&mut command);
        assert!(matches!(
            command.kind,
            CommandKind::Disassemble { recipe: None, .. }
        ));
    }

    #[test]
    fn reading_catalog_replaces_untrusted_payload_and_clears_unknown_book() {
        let authoritative = BookStudyV1 {
            book_type_id: String::from("manual_pistol"),
            skill_id: String::from("pistol"),
            required_skill_level: 0,
            maximum_skill_level: 3,
            intelligence_requirement: 3,
            time_moves: 90_000,
            source_time_minutes: 15,
        };
        let catalog = ReadingCatalog::new(BTreeMap::from([(
            authoritative.book_type_id.clone(),
            authoritative.clone(),
        )]));
        let mut forged = authoritative.clone();
        forged.skill_id = String::from("fabrication");
        let mut command = ClientCommand {
            actor_id: ActorId::new(1, 1),
            sequence: CommandSequence(1),
            client_tick: SimTick(0),
            kind: CommandKind::ReadBook {
                item_id: ItemId::new(1, 2),
                book_type_id: String::from("manual_pistol"),
                study: Some(Box::new(forged)),
            },
        };
        catalog.normalize(&mut command);
        assert!(matches!(
            command.kind,
            CommandKind::ReadBook { study: Some(study), .. } if *study == authoritative
        ));

        let mut unknown = ClientCommand {
            kind: CommandKind::ReadBook {
                item_id: ItemId::new(1, 2),
                book_type_id: String::from("unknown_book"),
                study: Some(Box::new(authoritative)),
            },
            ..command
        };
        catalog.normalize(&mut unknown);
        assert!(matches!(
            unknown.kind,
            CommandKind::ReadBook { study: None, .. }
        ));
    }

    #[test]
    fn persistence_byte_budget_counts_queued_and_in_flight_payloads() {
        let budget = Arc::new(PersistenceBudget::default());
        let first = budget
            .try_reserve(PERSISTENCE_BYTE_CAPACITY - 1)
            .expect("the first payload should fit");
        assert_eq!(budget.used(), PERSISTENCE_BYTE_CAPACITY - 1);
        assert!(matches!(
            budget.try_reserve(2),
            Err(StoreError::PersistenceBusy)
        ));
        drop(first);
        assert_eq!(budget.used(), 0);
        let entire_budget = budget
            .try_reserve(PERSISTENCE_BYTE_CAPACITY)
            .expect("the exact byte capacity should fit");
        assert_eq!(budget.used(), PERSISTENCE_BYTE_CAPACITY);
        drop(entire_budget);
    }

    #[test]
    fn queued_persistence_snapshot_is_replaced_explicitly() {
        let (requests, _receiver) = mpsc::sync_channel(PERSISTENCE_QUEUE_CAPACITY);
        let snapshots = Arc::new(SnapshotSlot::default());
        let budget = Arc::new(PersistenceBudget::default());
        let handle = PersistenceHandle {
            requests,
            snapshots: Arc::clone(&snapshots),
            budget: Arc::clone(&budget),
            database_path: None,
        };
        let first_snapshot = WorldState::new(1, [1; 32]).snapshot();
        let mut second_snapshot = first_snapshot.clone();
        second_snapshot.tick = SimTick(1);
        let first = handle
            .queue_snapshot(1, first_snapshot)
            .expect("first snapshot should queue");
        let second = handle
            .queue_snapshot(2, second_snapshot)
            .expect("newer snapshot should replace the pending one");
        assert_eq!(
            first.wait().expect("replacement should be reported"),
            SnapshotWriteOutcome::Superseded
        );
        assert_eq!(
            lock_unpoisoned(&snapshots.pending)
                .as_ref()
                .expect("newest snapshot should remain")
                .sequence,
            2
        );
        assert_eq!(
            second.try_result().expect("worker is still available"),
            None
        );
        assert!(budget.used() > 0);
        drop(lock_unpoisoned(&snapshots.pending).take());
        assert_eq!(budget.used(), 0);
    }

    async fn read_snapshot_until(
        connection: &Connection,
        actor_id: ActorId,
        sequence: CommandSequence,
        predicate: impl Fn(&ReplicationSnapshotV1) -> bool,
    ) -> ReplicationSnapshotV1 {
        loop {
            let mut receive = connection
                .accept_uni()
                .await
                .expect("snapshot stream should arrive");
            let (stream_actor, _snapshot_sequence, snapshot) = read_snapshot_stream(&mut receive)
                .await
                .expect("snapshot stream should decode");
            if snapshot.controlled_actor.id == actor_id
                && stream_actor == actor_id
                && snapshot.controlled_actor.last_command_sequence >= sequence
                && predicate(&snapshot)
            {
                return snapshot;
            }
        }
    }

    async fn read_events_until(
        receive: &mut iroh::endpoint::RecvStream,
        predicate: impl Fn(&[WorldEvent]) -> bool,
    ) -> Vec<WorldEvent> {
        loop {
            let message = read_control_frame(receive)
                .await
                .expect("event frame should arrive");
            match message {
                ControlMessage::Events(events) if predicate(&events) => return events,
                ControlMessage::Events(_) => {}
                other => panic!("expected authoritative events, got {other:?}"),
            }
        }
    }

    static NEXT_PATH: AtomicU64 = AtomicU64::new(1);

    fn temporary_key_path() -> PathBuf {
        let number = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "cdda-rust-identity-{}-{number}",
            std::process::id()
        ))
    }

    fn remove_database(path: &Path) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = PathBuf::from(format!("{}{suffix}", path.display()));
            if let Err(error) = fs::remove_file(candidate)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                panic!("failed to remove test database: {error}");
            }
        }
    }

    fn start_test_acknowledger(
        host: SimulationHost,
        committed_events: CommittedEventHub,
    ) -> (SyncSender<()>, JoinHandle<SimulationHost>) {
        let (stop, stop_receiver) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            loop {
                if stop_receiver.try_recv().is_ok() {
                    break;
                }
                match host.recv_timeout(Duration::from_millis(20)) {
                    Ok(SimulationOutput::Tick {
                        outcome,
                        durability,
                        ..
                    }) => {
                        if !outcome.events.is_empty() {
                            committed_events.publish(CommittedEventBatch {
                                tick: outcome.tick,
                                events: outcome.events,
                            });
                        }
                        for acknowledgement in durability {
                            let tick = acknowledgement.tick();
                            acknowledgement.acknowledge(Ok(tick));
                        }
                    }
                    Ok(SimulationOutput::Failed(_)) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
            host
        });
        (stop, thread)
    }

    fn start_test_character_creator(
        persistence: PersistenceHandle,
        simulation: SimulationHandle,
    ) -> CharacterCreationHandle {
        let (handle, mut requests) = character_creation_channel();
        tokio::spawn(async move {
            while let Some(request) = requests.recv().await {
                let begin_simulation = simulation.clone();
                let base_stats = request.base_stats();
                let spawned = tokio::task::spawn_blocking(move || {
                    begin_simulation.begin_actor_creation(base_stats, Duration::from_secs(1))
                })
                .await;
                let spawned = match spawned {
                    Ok(Ok(spawned)) => spawned,
                    Ok(Err(error)) => {
                        request
                            .complete(Err(CharacterCreationError::Simulation(error.to_string())));
                        continue;
                    }
                    Err(error) => {
                        request
                            .complete(Err(CharacterCreationError::Simulation(error.to_string())));
                        continue;
                    }
                };
                let result = persistence.create_character(
                    request.account_id(),
                    request.name().to_owned(),
                    spawned.created_tick,
                    0,
                    spawned.actor.clone(),
                );
                let committed = result.is_ok();
                let actor_id = spawned.actor.id;
                let complete_simulation = simulation.clone();
                let completion = tokio::task::spawn_blocking(move || {
                    complete_simulation.complete_actor_creation(
                        actor_id,
                        committed,
                        Duration::from_secs(1),
                    )
                })
                .await;
                match completion {
                    Ok(Ok(())) => request.complete(match result {
                        Ok(_character) => Ok(actor_id),
                        Err(error) => Err(CharacterCreationError::Persistence(error)),
                    }),
                    Ok(Err(error)) => {
                        request.complete(Err(CharacterCreationError::Simulation(error.to_string())))
                    }
                    Err(error) => {
                        request.complete(Err(CharacterCreationError::Simulation(error.to_string())))
                    }
                }
            }
        });
        handle
    }

    #[test]
    fn identity_is_stable_across_restarts() {
        let path = temporary_key_path();
        let first = load_or_create_secret_key(&path).expect("identity should be created");
        let second = load_or_create_secret_key(&path).expect("identity should be reloaded");
        assert_eq!(first.public(), second.public());
        fs::remove_file(path).expect("temporary identity should be removable");
    }

    #[test]
    fn world_advances_without_players() {
        let world = WorldState::new(9, [4; 32]);
        let host = SimulationHost::start(world).expect("simulation thread should start");
        let output = host
            .recv_timeout(Duration::from_secs(1))
            .expect("simulation should publish a tick");
        let SimulationOutput::Tick {
            outcome,
            commands,
            durability,
            ..
        } = output
        else {
            panic!("simulation unexpectedly failed");
        };
        assert!(commands.is_empty());
        assert!(durability.is_empty());
        assert_eq!(outcome.tick, SimTick(1));
        assert_eq!(
            host.handle()
                .snapshot(Duration::from_secs(1))
                .expect("snapshot request should complete")
                .tick,
            SimTick(1)
        );
        assert_eq!(host.shutdown(), SimulationExit::Requested);
    }

    #[test]
    fn actor_creation_pauses_ticks_until_commit_or_rollback() {
        let mut world = WorldState::new(17, [8; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 }));
        let host = SimulationHost::start(world).expect("simulation thread should start");
        let simulation = host.handle();
        let base_stats = CharacterCreationStatsV1 {
            strength: 12,
            dexterity: 11,
            intelligence: 10,
            perception: 9,
        };
        let spawned = simulation
            .begin_actor_creation(base_stats, Duration::from_secs(1))
            .expect("provisional actor should spawn");
        assert_eq!(spawned.actor.base_strength, 12);
        assert_eq!(spawned.actor.base_dexterity, 11);
        assert_eq!(spawned.actor.base_intelligence, 10);
        assert_eq!(spawned.actor.base_perception, 9);
        while host.try_recv().is_ok() {}
        std::thread::sleep(SIMULATION_INTERVAL * 3);
        assert!(matches!(host.try_recv(), Err(TryRecvError::Empty)));

        simulation
            .complete_actor_creation(spawned.actor.id, false, Duration::from_secs(1))
            .expect("rollback should resume ticks");
        let output = host
            .recv_timeout(Duration::from_secs(1))
            .expect("simulation should resume after rollback");
        assert!(matches!(output, SimulationOutput::Tick { .. }));
        assert!(
            simulation
                .snapshot(Duration::from_secs(1))
                .expect("snapshot should complete")
                .actors
                .is_empty()
        );
        assert_eq!(host.shutdown(), SimulationExit::Requested);
    }

    #[test]
    fn connection_transitions_are_emitted_with_the_next_tick_for_recovery() {
        let mut world = WorldState::new(18, [9; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let host = SimulationHost::start(world).expect("simulation thread should start");
        let simulation = host.handle();

        simulation
            .begin_checkpoint(Duration::from_secs(1))
            .expect("checkpoint should pause");
        while host.try_recv().is_ok() {}
        simulation
            .set_connected(actor_id, false, Duration::from_secs(1))
            .expect("disconnect should apply immediately");
        assert!(
            !simulation
                .snapshot(Duration::from_secs(1))
                .expect("snapshot should arrive")
                .actors
                .into_iter()
                .find(|actor| actor.id == actor_id)
                .expect("actor should remain")
                .connected
        );
        simulation
            .complete_checkpoint(Duration::from_secs(1))
            .expect("checkpoint should resume");

        let output = host
            .recv_timeout(Duration::from_secs(1))
            .expect("simulation should publish the recovery boundary");
        let SimulationOutput::Tick {
            connection_updates, ..
        } = output
        else {
            panic!("simulation unexpectedly failed");
        };
        assert_eq!(
            connection_updates,
            vec![ActorConnectionUpdateV1 {
                actor_id,
                connected: false,
            }]
        );
        assert_eq!(host.shutdown(), SimulationExit::Requested);
    }

    #[test]
    fn startup_recovery_transitions_seed_the_first_tick() {
        let mut world = WorldState::new(19, [10; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let connection_updates = world.disconnect_all_for_recovery();

        let host = SimulationHost::start_with_all_gameplay_catalogs_and_recovery_inputs(
            world,
            CraftingCatalog::default(),
            ReadingCatalog::default(),
            DisassemblyCatalog::default(),
            ConstructionCatalog::default(),
            connection_updates.clone(),
        )
        .expect("simulation thread should start");
        let output = host
            .recv_timeout(Duration::from_secs(1))
            .expect("simulation should publish its first tick");
        let SimulationOutput::Tick {
            connection_updates: journaled,
            ..
        } = output
        else {
            panic!("simulation unexpectedly failed");
        };
        assert_eq!(
            connection_updates,
            vec![ActorConnectionUpdateV1 {
                actor_id,
                connected: false,
            }]
        );
        assert_eq!(journaled, connection_updates);
        assert_eq!(host.shutdown(), SimulationExit::Requested);
    }

    #[test]
    fn checkpoint_barrier_pauses_ticks_and_installs_the_next_id_block() {
        let mut world = WorldState::new(21, [10; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 }));
        let host = SimulationHost::start(world).expect("simulation thread should start");
        let simulation = host.handle();
        let boundary = simulation
            .begin_checkpoint(Duration::from_secs(1))
            .expect("checkpoint should pause");
        while host.try_recv().is_ok() {}
        std::thread::sleep(SIMULATION_INTERVAL * 3);
        assert!(matches!(host.try_recv(), Err(TryRecvError::Empty)));
        simulation
            .install_reserved_block(
                ReservedIdBlock::new(4_097, 8_192).expect("next block should be valid"),
                Duration::from_secs(1),
            )
            .expect("paused simulation should accept the durable block");
        let reserved = simulation
            .snapshot(Duration::from_secs(1))
            .expect("reserved state should snapshot");
        assert_eq!(reserved.tick, boundary.tick);
        assert_eq!(reserved.allocator_next, 4_097);
        assert_eq!(reserved.allocator_reserved_end, 8_192);
        simulation
            .complete_checkpoint(Duration::from_secs(1))
            .expect("checkpoint should resume");
        assert!(matches!(
            host.recv_timeout(Duration::from_secs(1)),
            Ok(SimulationOutput::Tick { .. })
        ));
        assert_eq!(host.shutdown(), SimulationExit::Requested);
    }

    #[test]
    fn session_registry_enforces_capacity_and_unique_claims() {
        let registry = SessionRegistry::new(2);
        let account_one = AccountId::new(5, 1);
        let account_two = AccountId::new(5, 2);
        let actor = ActorId::new(5, 9);
        let mut first = registry
            .claim_account(account_one)
            .expect("first account should claim");
        assert!(matches!(
            registry.claim_account(account_one),
            Err(SessionClaimError::AlreadyActive)
        ));
        let mut second = registry
            .claim_account(account_two)
            .expect("second account should claim");
        assert_eq!(
            registry
                .inspect_account(account_one)
                .expect("account session should inspect"),
            (true, None)
        );
        first.claim_actor(actor).expect("actor should claim");
        assert_eq!(
            registry
                .inspect_account(account_one)
                .expect("controlled actor should inspect"),
            (true, Some(actor))
        );
        assert_eq!(
            second.claim_actor(actor),
            Err(SessionClaimError::AlreadyActive)
        );
        assert!(matches!(
            registry.claim_account(AccountId::new(5, 3)),
            Err(SessionClaimError::Full)
        ));
        drop(first);
        assert_eq!(
            registry
                .inspect_account(account_one)
                .expect("released account should inspect"),
            (false, None)
        );
        second
            .claim_actor(actor)
            .expect("dropped lease should release actor");
        drop(second);
        assert!(registry.claim_account(account_one).is_ok());
    }

    #[test]
    fn replication_snapshot_excludes_entities_and_chunks_outside_interest() {
        let mut world = WorldState::new(31, [22; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        for x in 0..=6 {
            world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x, y: 0, z: 0 }));
        }
        let mut center = Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 });
        center
            .set_furniture(
                LocalTileCoord { x: 2, y: 1 },
                Some(cdda_protocol::FurnitureTileSnapshot {
                    furniture_id: String::from("f_opaque_test"),
                    move_cost_mod: 0,
                    transparent: false,
                    blocks_door: false,
                    comfort: 0,
                    floor_bedding_warmth: 0,
                }),
            )
            .expect("opaque furniture should be valid");
        let visible_bed = cdda_protocol::FurnitureTileSnapshot {
            furniture_id: String::from("f_bed"),
            move_cost_mod: 3,
            transparent: true,
            blocks_door: false,
            comfort: 5,
            floor_bedding_warmth: 1_000,
        };
        center
            .set_furniture(LocalTileCoord { x: 1, y: 2 }, Some(visible_bed.clone()))
            .expect("bed should be valid");
        center
            .set_furniture(
                LocalTileCoord { x: 1, y: 0 },
                Some(cdda_protocol::FurnitureTileSnapshot {
                    furniture_id: String::from("f_unsupported_bash_test"),
                    move_cost_mod: 0,
                    transparent: true,
                    blocks_door: false,
                    comfort: 0,
                    floor_bedding_warmth: 0,
                }),
            )
            .expect("unsupported bash furniture should be valid");
        world.insert_chunk(center);
        world
            .register_terrain_bash_type(cdda_protocol::TerrainBashTypeV1 {
                terrain_id: String::from("t_floor"),
                str_min: 1,
                str_max: 2,
                str_min_blocked: -1,
                str_max_blocked: -1,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: cdda_protocol::TerrainTileSnapshot {
                    terrain_id: String::from("t_dirt"),
                    ..cdda_protocol::TerrainTileSnapshot {
                        terrain_id: String::from("t_floor"),
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
                    }
                },
                drop_source: None,
                hit_field: None,
                destroyed_field: None,
                sound: String::from("crunch!"),
                failure_sound: String::from("thump!"),
                sound_volume: 2,
                failure_sound_volume: 1,
            })
            .expect("floor bash should register");
        world
            .register_furniture_bash_type(cdda_protocol::FurnitureBashTypeV1 {
                furniture_id: String::from("f_bed"),
                str_min: 1,
                str_max: 2,
                str_min_blocked: -1,
                str_max_blocked: -1,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: None,
                drop_source: None,
                hit_field: None,
                destroyed_field: None,
                sound: String::from("crash!"),
                failure_sound: String::from("thump!"),
                sound_volume: 2,
                failure_sound_volume: 1,
            })
            .expect("bed bash should register");
        world
            .register_furniture_bash_presence(String::from("f_unsupported_bash_test"))
            .expect("unsupported furniture bash presence should register");
        world
            .register_field_type(cdda_protocol::FieldTypeSnapshotV1 {
                field_type_id: String::from("fd_blood"),
                intensity_levels: vec![cdda_protocol::FieldIntensityLevelV1 {
                    name: String::from("blood splatter"),
                    symbol: String::from("%"),
                    color: String::from("red"),
                    dangerous: false,
                    transparent: true,
                    contact_effects: Vec::new(),
                    contact_effects_supported: true,
                }],
                priority: 0,
                half_life_seconds: 172_800,
                linear_half_life: false,
                contact_damage: None,
                is_splattering: true,
                display_field: true,
                decrease_intensity_on_contact: false,
            })
            .expect("blood field type should register");
        world
            .add_field(WorldPosition { x: 1, y: 2, z: 0 }, "fd_blood", 1)
            .expect("visible blood should place");
        world
            .add_field(WorldPosition { x: 3, y: 1, z: 0 }, "fd_blood", 1)
            .expect("occluded blood should place");
        let controlled = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("controlled actor should spawn");
        let visible = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 3, z: 0 }, true)
            .expect("near actor should spawn");
        let daylight_only = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 4, z: 0 }, true)
            .expect("daylight-only actor should spawn");
        let beyond_low_light = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 5, z: 0 }, true)
            .expect("farther daylight-only actor should spawn");
        let occluded = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 3, y: 1, z: 0 }, true)
            .expect("occluded actor should spawn");
        let distant = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 24, y: 1, z: 0 }, true)
            .expect("distant actor should spawn");
        let visible_creature = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_zombie"),
                position: WorldPosition { x: 1, y: 2, z: 0 },
                hp: 80,
                speed: 100,
                attack_cost_moves: 100,
                aggression: 100,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 2,
                melee_dice_sides: 3,
                can_see: true,
                vision_day: 40,
                vision_night: 3,
                stumbles: true,
                bashes: true,
                group_bash: true,
                hears: true,
                good_hearing: false,
                clumsy_attacks: false,
                immobile: false,
                pacifist: false,
                can_open_doors: false,
                path_settings: Default::default(),
                blood_field_type_id: String::from("fd_blood"),
                corpse: None,
            })
            .expect("visible creature should spawn");
        let mut source = world.snapshot();
        let controlled_source = source
            .actors
            .iter_mut()
            .find(|actor| actor.id == controlled)
            .expect("controlled actor exists");
        controlled_source.sleepiness = 123;
        controlled_source.action_points = -1_500;
        let visible_source = source
            .actors
            .iter_mut()
            .find(|actor| actor.id == visible)
            .expect("visible actor exists");
        visible_source.sleepiness = cdda_sim::SLEEPINESS_TIRED;
        visible_source.sleeping = true;
        visible_source.sleep_intervals = 1;
        let creature_source = source
            .creatures
            .iter_mut()
            .find(|creature| creature.id == visible_creature)
            .expect("visible creature exists");
        creature_source.goal = Some(WorldPosition { x: 10, y: 10, z: 0 });
        creature_source.sound_goal = Some(cdda_protocol::CreatureSoundGoalV1 {
            position: WorldPosition { x: 9, y: 9, z: 0 },
            remaining_actions: 42,
        });
        creature_source.can_open_doors = true;
        creature_source.path_settings.max_distance = 45;
        creature_source.path_settings.allow_open_doors = true;
        creature_source.path_settings.avoid_traps = true;
        creature_source.action_points = -1_500;
        let filtered =
            interest_snapshot(source, controlled).expect("controlled interest should derive");
        assert_eq!(filtered.controlled_actor.id, controlled);
        assert_eq!(filtered.controlled_actor.sleepiness, 123);
        assert_eq!(filtered.controlled_actor.action_points, -1_500);
        cdda_protocol::encode_replication_snapshot(&filtered)
            .expect("signed movement debt should remain a valid bounded bulk snapshot");
        assert_eq!(
            filtered.creatures,
            vec![VisibleCreatureSnapshot {
                id: visible_creature,
                type_id: String::from("mon_zombie"),
                position: WorldPosition { x: 1, y: 2, z: 0 },
                hp: 80,
                max_hp: 80,
            }],
            "replication projects creatures into a DTO without visual or sound goals, hearing/door-opening/bash/routing capabilities, debt, or combat internals"
        );
        assert!(
            filtered
                .visible_actors
                .iter()
                .any(|actor| actor.id == visible && actor.sleeping)
        );
        assert!(
            filtered
                .visible_actors
                .iter()
                .any(|actor| actor.id == daylight_only)
        );
        assert!(
            filtered
                .visible_actors
                .iter()
                .any(|actor| actor.id == beyond_low_light)
        );
        assert!(
            !filtered
                .visible_actors
                .iter()
                .any(|actor| actor.id == occluded)
        );
        assert!(
            !filtered
                .visible_actors
                .iter()
                .any(|actor| actor.id == distant)
        );
        assert_eq!(filtered.chunks.len(), 6);
        assert!(filtered.chunks.iter().all(|chunk| chunk.coord.x <= 5));
        let center = filtered
            .chunks
            .iter()
            .find(|chunk| chunk.coord == cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 })
            .expect("center chunk should replicate");
        let index = |x: usize, y: usize| y * cdda_protocol::SUBMAP_SIZE as usize + x;
        assert!(
            center.tiles[index(2, 1)].is_some(),
            "the occluding furniture tile is visible"
        );
        assert!(
            center.tiles[index(2, 1)]
                .as_ref()
                .is_some_and(|tile| tile.currently_visible),
            "current terrain is explicitly marked"
        );
        assert_eq!(
            center.tiles[index(1, 2)]
                .as_ref()
                .and_then(|tile| tile.furniture.as_ref()),
            Some(&visible_bed),
            "currently visible furniture is replicated"
        );
        assert_eq!(
            center.tiles[index(1, 2)]
                .as_ref()
                .and_then(|tile| tile.bash_target),
            Some(BashTargetKindV1::Furniture),
            "registered furniture takes authoritative smash precedence"
        );
        assert_eq!(
            center.tiles[index(2, 1)]
                .as_ref()
                .and_then(|tile| tile.bash_target),
            Some(BashTargetKindV1::Terrain),
            "unregistered furniture does not hide a registered terrain smash target"
        );
        assert_eq!(
            center.tiles[index(1, 0)]
                .as_ref()
                .and_then(|tile| tile.bash_target),
            None,
            "unsupported upstream furniture bash behavior blocks terrain metadata"
        );
        assert_eq!(
            center.tiles[index(1, 2)]
                .as_ref()
                .map(|tile| tile.fields.as_slice()),
            Some(
                [cdda_protocol::FieldObservationV1 {
                    field_type_id: String::from("fd_blood"),
                    intensity: 1,
                    name: String::from("blood splatter"),
                    symbol: String::from("%"),
                    color: String::from("red"),
                    dangerous: false,
                    transparent: true,
                    priority: 0,
                    display_field: true,
                    display_sequence: 1,
                }]
                .as_slice()
            ),
            "currently visible dynamic fields are replicated"
        );
        assert!(
            center.tiles[index(3, 1)].is_none(),
            "terrain behind the wall is hidden"
        );
        assert!(filtered.controlled_actor.map_memory.is_empty());

        let mut sleeping_source = world.snapshot();
        let sleeping_actor = sleeping_source
            .actors
            .iter_mut()
            .find(|actor| actor.id == controlled)
            .expect("controlled actor exists");
        sleeping_actor.sleepiness = cdda_sim::SLEEPINESS_TIRED;
        sleeping_actor.sleeping = true;
        sleeping_actor.sleep_intervals = 1;
        let sleeping =
            interest_snapshot(sleeping_source, controlled).expect("sleep interest should derive");
        assert!(sleeping.visible_actors.is_empty());
        assert!(sleeping.creatures.is_empty());
        assert!(sleeping.ground_items.is_empty());
        assert!(
            sleeping
                .chunks
                .iter()
                .flat_map(|chunk| &chunk.tiles)
                .all(|tile| tile
                    .as_ref()
                    .is_none_or(|observation| !observation.currently_visible))
        );

        let mut canonical = world.snapshot();
        let actor = canonical
            .actors
            .iter_mut()
            .find(|actor| actor.id == controlled)
            .expect("controlled actor exists");
        let memory = actor
            .map_memory
            .iter_mut()
            .find(|memory| memory.coord == cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 })
            .expect("spawn initialized center memory");
        memory.tiles[index(3, 1)] = Some(cdda_protocol::MemorizedTileSnapshot {
            terrain: TerrainTileSnapshot {
                terrain_id: String::from("t_remembered_floor"),
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
            },
            furniture: Some(cdda_protocol::FurnitureTileSnapshot {
                furniture_id: String::from("f_remembered_bed"),
                move_cost_mod: 3,
                transparent: true,
                blocks_door: false,
                comfort: 5,
                floor_bedding_warmth: 1_000,
            }),
        });
        let remembered =
            interest_snapshot(canonical, controlled).expect("remembered terrain should derive");
        let center = remembered
            .chunks
            .iter()
            .find(|chunk| chunk.coord == cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 })
            .expect("center chunk should replicate");
        let remembered_tile = center.tiles[index(3, 1)]
            .as_ref()
            .expect("memory should fill currently hidden terrain");
        assert!(!remembered_tile.currently_visible);
        assert_eq!(remembered_tile.bash_target, None);
        assert_eq!(remembered_tile.terrain.terrain_id, "t_remembered_floor");
        assert!(
            remembered_tile.fields.is_empty(),
            "dynamic fields never leak through stale terrain memory"
        );
        assert_eq!(
            remembered_tile
                .furniture
                .as_ref()
                .map(|furniture| furniture.furniture_id.as_str()),
            Some("f_remembered_bed"),
            "only the character's stale remembered furniture is exposed"
        );

        let mut night = world.snapshot();
        night.tick = SimTick(13 * 60 * 60 * SimTick::HZ);
        let night = interest_snapshot(night, controlled).expect("night interest should derive");
        assert_eq!(night.natural_light.sight_radius, 2);
        assert!(!night.detail_vision_available);
        assert!(night.visible_actors.iter().any(|actor| actor.id == visible));
        assert!(
            !night
                .visible_actors
                .iter()
                .any(|actor| actor.id == daylight_only)
        );

        let low_output_item = cdda_protocol::ItemSnapshot {
            id: cdda_protocol::ItemId::new(31, 110),
            type_id: String::from("wizard_cane_cheap_on"),
            owner_faction_id: String::new(),
            charges: 0,
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
            magazine_wells: vec![cdda_protocol::MagazineWellSnapshotV1 {
                pocket_index: 0,
                pocket_id: String::new(),
                compatible_magazine_type_ids: vec![String::from("light_minus_battery_cell")],
                rigid: true,
                unloadable: true,
                installed_magazine: Some(Box::new(cdda_protocol::ItemSnapshot {
                    id: cdda_protocol::ItemId::new(31, 111),
                    type_id: String::from("light_minus_battery_cell"),
                    owner_faction_id: String::new(),
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
                    ammunition_type: String::from("battery"),
                    ranged_weapon: None,
                    component_provenance: None,
                    magazine_capacity: 2,
                    integral_magazines: Vec::new(),
                    magazine_wells: Vec::new(),
                    ammunition_containers: Vec::new(),
                    residual_energy_millijoules: 0,
                    powered_tool: None,
                    creature_corpse: None,
                    containment: Default::default(),
                })),
            }],
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: Some(cdda_protocol::PoweredToolStateV1 {
                inactive_type_id: String::from("wizard_cane_cheap"),
                active_type_id: String::from("wizard_cane_cheap_on"),
                activation_charges: 1,
                power_draw_milliwatts: 1_000,
                light_emission: 4,
                dims_with_charge: true,
                power_pocket_index: 0,
                active: true,
            }),
            creature_corpse: None,
            containment: Default::default(),
        };
        let mut personally_lit = world.snapshot();
        personally_lit.tick = SimTick(13 * 60 * 60 * SimTick::HZ);
        personally_lit
            .actors
            .iter_mut()
            .find(|actor| actor.id == controlled)
            .expect("controlled actor exists")
            .inventory
            .push(low_output_item.clone());
        let personally_lit = interest_snapshot(personally_lit, controlled)
            .expect("personal low-output light should derive");
        assert!(personally_lit.detail_vision_available);
        assert!(
            personally_lit
                .visible_actors
                .iter()
                .any(|actor| actor.id == daylight_only),
            "luminance four reaches exactly three open-air tiles"
        );
        assert!(
            !personally_lit
                .visible_actors
                .iter()
                .any(|actor| actor.id == beyond_low_light)
        );

        let mut ground_lit = world.snapshot();
        ground_lit.tick = SimTick(13 * 60 * 60 * SimTick::HZ);
        ground_lit
            .ground_items
            .push(cdda_protocol::GroundItemSnapshot {
                item: low_output_item,
                position: cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 },
            });
        let ground_lit = interest_snapshot(ground_lit, controlled)
            .expect("external low-output light should derive");
        assert!(
            !ground_lit.detail_vision_available,
            "luminance four is below the external detail-work threshold"
        );
        assert!(
            ground_lit
                .visible_actors
                .iter()
                .any(|actor| actor.id == daylight_only)
        );
        assert!(
            !ground_lit
                .visible_actors
                .iter()
                .any(|actor| actor.id == beyond_low_light)
        );

        let mut externally_lit = world.snapshot();
        externally_lit.tick = SimTick(13 * 60 * 60 * SimTick::HZ);
        externally_lit
            .actors
            .iter_mut()
            .find(|actor| actor.id == visible)
            .expect("near actor exists")
            .inventory
            .push(cdda_protocol::ItemSnapshot {
                id: cdda_protocol::ItemId::new(31, 100),
                type_id: String::from("flashlight_on"),
                owner_faction_id: String::new(),
                charges: 0,
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
                magazine_wells: vec![cdda_protocol::MagazineWellSnapshotV1 {
                    pocket_index: 0,
                    pocket_id: String::new(),
                    compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
                    rigid: true,
                    unloadable: true,
                    installed_magazine: Some(Box::new(cdda_protocol::ItemSnapshot {
                        id: cdda_protocol::ItemId::new(31, 101),
                        type_id: String::from("medium_battery_cell"),
                        owner_faction_id: String::new(),
                        charges: 0,
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
                        ammunition_type: String::from("battery"),
                        ranged_weapon: None,
                        component_provenance: None,
                        magazine_capacity: 56,
                        integral_magazines: Vec::new(),
                        magazine_wells: Vec::new(),
                        ammunition_containers: Vec::new(),
                        residual_energy_millijoules: 998_440,
                        powered_tool: None,
                        creature_corpse: None,
                        containment: Default::default(),
                    })),
                }],
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: Some(cdda_protocol::PoweredToolStateV1 {
                    inactive_type_id: String::from("flashlight"),
                    active_type_id: String::from("flashlight_on"),
                    activation_charges: 1,
                    power_draw_milliwatts: 1_560,
                    light_emission: 300,
                    dims_with_charge: true,
                    power_pocket_index: 0,
                    active: true,
                }),
                creature_corpse: None,
                containment: Default::default(),
            });
        assert_eq!(
            externally_lit
                .actors
                .iter()
                .find(|actor| actor.id == visible)
                .and_then(|actor| actor.inventory.last())
                .map(item_powered_light_emission),
            Some(26),
            "a nearly spent CHARGEDIM flashlight uses exact residual energy"
        );
        let externally_lit = interest_snapshot(externally_lit, controlled)
            .expect("another actor's flashlight should derive visibility");
        assert!(externally_lit.detail_vision_available);
        assert!(
            externally_lit
                .visible_actors
                .iter()
                .any(|actor| actor.id == daylight_only),
            "an authoritative external light exposes illuminated targets"
        );
        assert!(
            !externally_lit
                .visible_actors
                .iter()
                .any(|actor| actor.id == occluded),
            "local light does not bypass line of sight"
        );
    }

    #[test]
    fn eleven_by_eleven_daylight_snapshot_uses_the_bounded_bulk_path() {
        let mut world = WorldState::new(32, [23; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        for y in -5..=5 {
            for x in -5..=5 {
                world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x, y, z: 0 }));
            }
        }
        let controlled = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 }, true)
            .expect("controlled actor should spawn");
        let snapshot = interest_snapshot(world.snapshot(), controlled)
            .expect("replication snapshot should derive");
        assert_eq!(snapshot.chunks.len(), 121);
        let encoded = cdda_protocol::encode_replication_snapshot(&snapshot)
            .expect("121 visibility-masked chunks must fit the bounded bulk payload");
        assert!(encoded.len() > cdda_protocol::MAX_CONTROL_ENCODED);
        assert!(encoded.len() <= cdda_protocol::MAX_BULK_DECODED);

        let (updates, receiver) = tokio::sync::watch::channel(None);
        let updates = Some(updates);
        queue_snapshot(&updates, snapshot.clone()).expect("first snapshot should queue");
        let mut newest = snapshot;
        newest.controlled_actor.action_points = -1;
        queue_snapshot(&updates, newest).expect("newest snapshot should supersede the old one");
        assert_eq!(
            receiver
                .borrow()
                .as_ref()
                .expect("one replaceable snapshot should remain")
                .controlled_actor
                .action_points,
            -1
        );
    }

    #[test]
    fn control_ingress_bucket_enforces_rate_and_burst() {
        let start = Instant::now();
        let mut limiter = ControlIngressLimiter::new(start);
        for _ in 0..CONTROL_MESSAGE_BURST {
            assert!(limiter.allow(start));
        }
        assert!(!limiter.allow(start));
        assert!(!limiter.allow(start + Duration::from_millis(24)));
        assert!(limiter.allow(start + Duration::from_millis(25)));
        assert!(!limiter.allow(start + Duration::from_millis(25)));
        for _ in 0..CONTROL_MESSAGE_BURST {
            assert!(limiter.allow(start + Duration::from_secs(10)));
        }
        assert!(!limiter.allow(start + Duration::from_secs(10)));
    }

    #[test]
    fn datagram_ingress_bucket_enforces_rate_and_burst() {
        let start = Instant::now();
        let mut limiter = DatagramIngressLimiter::new(start);
        for _ in 0..DATAGRAM_BURST {
            assert!(limiter.allow(start));
        }
        assert!(!limiter.allow(start));
        assert!(!limiter.allow(start + Duration::from_millis(16)));
        assert!(limiter.allow(start + Duration::from_millis(17)));
        assert!(!limiter.allow(start + Duration::from_millis(17)));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pending_identity_enrolls_over_real_iroh_connection() {
        let database_path = temporary_key_path().with_extension("db");
        remove_database(&database_path);
        let client_secret = SecretKey::generate();
        let client_identity = EndpointIdentity(*client_secret.public().as_bytes());
        let now = utc_now_seconds().expect("wall clock should be supported");
        {
            let mut store = WorldStore::open(&database_path).expect("store should open");
            store
                .initialize_world(71, [5; 32])
                .expect("world should initialize");
            store
                .create_pending_account(
                    AccountId::new(71, 1),
                    "Network Ada",
                    AccountRole::Player,
                    client_identity,
                    now,
                )
                .expect("pending account should be created");
        }
        let persistence_host = PersistenceHost::start(
            WorldStore::open(&database_path).expect("worker store should open"),
        )
        .expect("persistence worker should start");
        let persistence = persistence_host.handle();

        let server = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(SecretKey::generate())
            .alpns(vec![ENROLL_ALPN.to_vec()])
            .bind()
            .await
            .expect("server endpoint should bind");
        let server_address = server.addr();
        let serving_endpoint = server.clone();
        let serving_persistence = persistence.clone();
        let server_task = tokio::spawn(async move {
            let incoming = serving_endpoint
                .accept()
                .await
                .expect("server should receive one connection");
            let connection = incoming.await.expect("handshake should complete");
            handle_enrollment_connection(&connection, serving_persistence)
                .await
                .expect("enrollment handler should complete")
        });

        let client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(client_secret)
            .bind()
            .await
            .expect("client endpoint should bind");
        let connection = client
            .connect(server_address, ENROLL_ALPN)
            .await
            .expect("client should connect");
        assert_eq!(connection.remote_id(), server.id());
        let (mut send, mut receive) = connection
            .open_bi()
            .await
            .expect("control stream should open");
        write_control_frame(
            &mut send,
            &ControlMessage::EnrollmentRequest {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await
        .expect("request should send");
        send.finish().expect("request stream should finish");
        let response = read_control_frame(&mut receive)
            .await
            .expect("response should decode");
        let ControlMessage::EnrollmentAccepted(accepted) = response else {
            panic!("pending identity should be accepted");
        };
        assert_eq!(accepted.account_id, AccountId::new(71, 1));

        let server_account = server_task.await.expect("server task should join");
        assert_eq!(
            server_account
                .expect("server should return the enrolled account")
                .id,
            accepted.account_id
        );
        client.close().await;
        server.close().await;
        persistence_host.shutdown();
        let store = WorldStore::open(&database_path).expect("store should reopen");
        assert_eq!(
            store
                .authorize_endpoint(
                    client_identity,
                    utc_now_seconds().expect("clock should work"),
                )
                .expect("enrolled endpoint should authorize")
                .id,
            accepted.account_id
        );
        remove_database(&database_path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn moderation_reports_and_account_management_apply_over_real_iroh() {
        let database_path = temporary_key_path().with_extension("admin.db");
        remove_database(&database_path);
        let moderator_secret = SecretKey::generate();
        let moderator_endpoint = EndpointIdentity(*moderator_secret.public().as_bytes());
        let administrator_secret = SecretKey::generate();
        let administrator_endpoint = EndpointIdentity(*administrator_secret.public().as_bytes());
        let player_secret = SecretKey::generate();
        let player_endpoint = EndpointIdentity(*player_secret.public().as_bytes());
        let created_account_secret = SecretKey::generate();
        let created_account_endpoint =
            EndpointIdentity(*created_account_secret.public().as_bytes());
        let discarded_secret = SecretKey::generate();
        let discarded_endpoint = EndpointIdentity(*discarded_secret.public().as_bytes());
        let moderator_id = AccountId::new(83, 1);
        let player_id = AccountId::new(83, 2);
        let administrator_id = AccountId::new(83, 3);
        let now = utc_now_seconds().expect("wall clock should be supported");
        let mut world = WorldState::new(83, [9; 32]);
        world
            .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
            .expect("block should install");
        world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 }, false)
            .expect("moderated actor should spawn");
        let reported_actor_id = world
            .spawn_actor(cdda_protocol::WorldPosition { x: 1, y: 0, z: 0 }, false)
            .expect("reported actor should spawn");
        {
            let mut store = WorldStore::open(&database_path).expect("store should open");
            store
                .initialize_world(83, [9; 32])
                .expect("world should initialize");
            for (account_id, name, role, endpoint) in [
                (
                    moderator_id,
                    "Network Moderator",
                    AccountRole::Moderator,
                    moderator_endpoint,
                ),
                (
                    player_id,
                    "Network Player",
                    AccountRole::Player,
                    player_endpoint,
                ),
                (
                    administrator_id,
                    "Network Administrator",
                    AccountRole::Administrator,
                    administrator_endpoint,
                ),
            ] {
                store
                    .create_pending_account(account_id, name, role, endpoint, now)
                    .expect("account should create");
                store
                    .enroll_endpoint(endpoint, now)
                    .expect("account should enroll");
            }
            store
                .create_character(
                    player_id,
                    "Moderated Survivor",
                    SimTick(0),
                    0,
                    &world.actor_snapshot(actor_id).expect("actor should exist"),
                )
                .expect("moderated character should persist");
            store
                .create_character(
                    moderator_id,
                    "Reported Moderator",
                    SimTick(0),
                    0,
                    &world
                        .actor_snapshot(reported_actor_id)
                        .expect("reported actor should exist"),
                )
                .expect("reported character should persist");
        }
        let persistence_host = PersistenceHost::start(
            WorldStore::open(&database_path).expect("worker store should open"),
        )
        .expect("persistence worker should start");
        let persistence = persistence_host.handle();
        let host = SimulationHost::start(world).expect("simulation should start");
        let simulation = host.handle();
        let (stop_acknowledger, acknowledger) =
            start_test_acknowledger(host, CommittedEventHub::default());
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [9; 32],
            enabled_mods: Vec::new(),
        };
        let sessions = SessionRegistry::default();
        let authorization_changes = AuthorizationChangeHub::default();
        let (character_creator, _character_creation_requests) = character_creation_channel();
        let server = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(SecretKey::generate())
            .alpns(vec![
                GAME_ALPN.to_vec(),
                ADMIN_ALPN.to_vec(),
                ENROLL_ALPN.to_vec(),
            ])
            .bind()
            .await
            .expect("server endpoint should bind");
        let server_address = server.addr();
        let serving_endpoint = server.clone();
        let serving_persistence = persistence.clone();
        let serving_simulation = simulation.clone();
        let serving_content = content.clone();
        let serving_sessions = sessions.clone();
        let serving_authorization_changes = authorization_changes.clone();
        let serving_character_creator = character_creator.clone();
        let server_task = tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            for _ in 0..4 {
                let incoming = serving_endpoint
                    .accept()
                    .await
                    .expect("server should receive connection");
                let connection = incoming.await.expect("handshake should complete");
                let persistence = serving_persistence.clone();
                let authorization_changes = serving_authorization_changes.clone();
                if connection.alpn() == GAME_ALPN {
                    let simulation = serving_simulation.clone();
                    let content = serving_content.clone();
                    let sessions = serving_sessions.clone();
                    let character_creator = serving_character_creator.clone();
                    handlers.spawn(async move {
                        let result = handle_game_connection_with_sessions(
                            &connection,
                            persistence,
                            simulation,
                            content,
                            sessions,
                            authorization_changes,
                            character_creator,
                            CommittedEventHub::default(),
                            ChatHub::default(),
                        )
                        .await;
                        connection.close(0_u32.into(), b"game handler complete");
                        result
                    });
                } else if connection.alpn() == ADMIN_ALPN {
                    let sessions = serving_sessions.clone();
                    let simulation = serving_simulation.clone();
                    handlers.spawn(async move {
                        let result = handle_admin_connection(
                            &connection,
                            persistence,
                            authorization_changes,
                            sessions,
                            simulation,
                        )
                        .await;
                        connection.close(0_u32.into(), b"admin handler complete");
                        result
                    });
                } else {
                    handlers.spawn(async move {
                        let result = handle_enrollment_connection(&connection, persistence)
                            .await
                            .map(|_| ());
                        connection.close(0_u32.into(), b"enrollment handler complete");
                        result
                    });
                }
            }
            let mut results = Vec::new();
            while let Some(result) = handlers.join_next().await {
                results.push(result.expect("handler should join"));
            }
            results
        });

        let player_client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(player_secret)
            .bind()
            .await
            .expect("player endpoint should bind");
        let player_connection = player_client
            .connect(server_address.clone(), GAME_ALPN)
            .await
            .expect("player should connect");
        let (mut player_send, mut player_receive) = player_connection
            .open_bi()
            .await
            .expect("player control stream should open");
        write_control_frame(
            &mut player_send,
            &ControlMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                content,
            }),
        )
        .await
        .expect("player hello should send");
        assert!(matches!(
            read_control_frame(&mut player_receive)
                .await
                .expect("server hello should arrive"),
            ControlMessage::ServerHello(_)
        ));
        assert!(matches!(
            read_control_frame(&mut player_receive)
                .await
                .expect("character list should arrive"),
            ControlMessage::CharacterList(_)
        ));
        write_control_frame(
            &mut player_send,
            &ControlMessage::CharacterRequest(CharacterRequest::Select { actor_id }),
        )
        .await
        .expect("character selection should send");
        assert_eq!(
            read_control_frame(&mut player_receive)
                .await
                .expect("selected character should become ready"),
            ControlMessage::CharacterReady { actor_id }
        );
        let mut player_events = player_connection
            .accept_uni()
            .await
            .expect("event stream should open");
        assert_eq!(
            read_control_frame(&mut player_events)
                .await
                .expect("event stream header should decode"),
            ControlMessage::EventStreamReady { actor_id }
        );
        let mut initial_snapshot = player_connection
            .accept_uni()
            .await
            .expect("initial snapshot stream should open");
        let (snapshot_actor, snapshot_sequence, snapshot) =
            read_snapshot_stream(&mut initial_snapshot)
                .await
                .expect("initial snapshot should decode");
        assert_eq!(snapshot_actor, actor_id);
        assert_eq!(snapshot_sequence, 0);
        assert_eq!(snapshot.controlled_actor.id, actor_id);
        write_control_frame(
            &mut player_send,
            &ControlMessage::ReportSubmit(PlayerReport {
                target_actor: reported_actor_id,
                reason: ReportReason::Chat,
                details: String::from("transport-level report fixture"),
            }),
        )
        .await
        .expect("player report should send");
        assert_eq!(
            read_control_frame(&mut player_receive)
                .await
                .expect("report acceptance should arrive"),
            ControlMessage::ReportResponse(ReportResponse::Accepted {
                report_id: ReportId(1),
            })
        );

        let moderator_client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(moderator_secret)
            .bind()
            .await
            .expect("moderator endpoint should bind");
        let moderator_connection = moderator_client
            .connect(server_address.clone(), ADMIN_ALPN)
            .await
            .expect("moderator should connect");
        let (mut moderator_send, mut moderator_receive) = moderator_connection
            .open_bi()
            .await
            .expect("moderator control stream should open");
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminHello(cdda_protocol::AdminHello {
                protocol_version: PROTOCOL_VERSION,
            }),
        )
        .await
        .expect("moderator hello should send");
        assert_eq!(
            read_control_frame(&mut moderator_receive)
                .await
                .expect("moderator ready should arrive"),
            ControlMessage::AdminResponse(AdminResponse::Ready {
                account_id: moderator_id,
                role: AccountRole::Moderator,
            })
        );
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::ListAccounts {
                after: None,
                limit: 1,
            }),
        )
        .await
        .expect("account page request should send");
        let ControlMessage::AdminResponse(AdminResponse::Accounts {
            accounts,
            next_after,
        }) = read_control_frame(&mut moderator_receive)
            .await
            .expect("account page should arrive")
        else {
            panic!("expected an account page");
        };
        assert_eq!(accounts[0].account_id, moderator_id);
        assert_eq!(next_after, Some(moderator_id));
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::ListCharacters {
                account_id: player_id,
            }),
        )
        .await
        .expect("live character inspection should send");
        let ControlMessage::AdminResponse(AdminResponse::Characters {
            account_id,
            characters,
            gameplay_session_active,
            controlled_actor,
        }) = read_control_frame(&mut moderator_receive)
            .await
            .expect("live character inspection should arrive")
        else {
            panic!("expected live character inspection");
        };
        assert_eq!(account_id, player_id);
        assert_eq!(characters.len(), 1);
        assert_eq!(characters[0].actor_id, actor_id);
        assert!(gameplay_session_active);
        assert_eq!(controlled_actor, Some(actor_id));
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::InspectCharacter {
                actor_id,
                inventory_after: None,
                inventory_limit: cdda_protocol::MAX_ADMIN_INVENTORY_PER_PAGE,
            }),
        )
        .await
        .expect("moderator private-inspection attempt should send");
        assert_eq!(
            read_control_frame(&mut moderator_receive)
                .await
                .expect("private-inspection denial should arrive"),
            ControlMessage::AdminResponse(AdminResponse::Rejected(
                AdminRejection::AdministratorRequired,
            ))
        );
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::ListReports {
                state: Some(ReportState::Open),
                after: None,
                limit: 1,
            }),
        )
        .await
        .expect("report page request should send");
        let ControlMessage::AdminResponse(AdminResponse::Reports {
            reports,
            next_after,
        }) = read_control_frame(&mut moderator_receive)
            .await
            .expect("report page should arrive")
        else {
            panic!("expected a report page");
        };
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].report_id, ReportId(1));
        assert_eq!(reports[0].target_actor, reported_actor_id);
        assert_eq!(reports[0].details, "transport-level report fixture");
        assert_eq!(next_after, None);
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::SetReportState {
                report_id: ReportId(1),
                state: ReportState::Actioned,
            }),
        )
        .await
        .expect("report resolution should send");
        let ControlMessage::AdminResponse(AdminResponse::ReportUpdated(resolved_report)) =
            read_control_frame(&mut moderator_receive)
                .await
                .expect("resolved report should arrive")
        else {
            panic!("expected an updated report");
        };
        assert_eq!(resolved_report.state, ReportState::Actioned);
        assert_eq!(resolved_report.resolved_by_account, Some(moderator_id));
        assert!(resolved_report.resolution_audit_sequence.is_some());

        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::SetMute {
                account_id: player_id,
                duration_seconds: Some(60),
            }),
        )
        .await
        .expect("mute request should send");
        let ControlMessage::AdminResponse(AdminResponse::ModerationApplied {
            account: muted,
            kind: ModerationKind::Mute,
            until_utc: Some(muted_until),
        }) = read_control_frame(&mut moderator_receive)
            .await
            .expect("durable mute response should arrive")
        else {
            panic!("expected mute response");
        };
        assert_eq!(muted.account_id, player_id);
        assert_eq!(muted.muted_until_utc, Some(muted_until));
        assert!(muted_until > now);
        write_control_frame(
            &mut player_send,
            &ControlMessage::ChatSend {
                text: String::from("this must not be broadcast"),
            },
        )
        .await
        .expect("muted chat attempt should send");
        assert_eq!(
            read_control_frame(&mut player_receive)
                .await
                .expect("mute rejection should arrive"),
            ControlMessage::ChatRejected(ChatRejection::Muted {
                until_utc: muted_until,
            })
        );

        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::SetSuspension {
                account_id: player_id,
                duration_seconds: Some(60),
            }),
        )
        .await
        .expect("suspension request should send");
        let ControlMessage::AdminResponse(AdminResponse::ModerationApplied {
            account: suspended,
            kind: ModerationKind::Suspension,
            until_utc: Some(suspended_until),
        }) = read_control_frame(&mut moderator_receive)
            .await
            .expect("durable suspension response should arrive")
        else {
            panic!("expected suspension response");
        };
        assert_eq!(suspended.account_id, player_id);
        assert_eq!(suspended.suspended_until_utc, Some(suspended_until));
        assert!(suspended_until > now);
        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::ListModerationHistory {
                account_id: player_id,
                after: None,
                limit: 8,
            }),
        )
        .await
        .expect("moderation history request should send");
        let ControlMessage::AdminResponse(AdminResponse::ModerationHistory {
            account_id,
            entries,
            next_after,
        }) = read_control_frame(&mut moderator_receive)
            .await
            .expect("moderation history should arrive")
        else {
            panic!("expected moderation history");
        };
        assert_eq!(account_id, player_id);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].kind, ModerationKind::Mute);
        assert_eq!(entries[1].kind, ModerationKind::Suspension);
        assert_eq!(next_after, None);
        tokio::time::timeout(Duration::from_secs(2), player_connection.closed())
            .await
            .expect("suspended gameplay connection should close promptly");

        write_control_frame(
            &mut moderator_send,
            &ControlMessage::AdminRequest(AdminRequest::CreateAccount {
                display_name: String::from("Created Through Iroh"),
                role: AccountRole::Player,
                endpoint: created_account_endpoint,
            }),
        )
        .await
        .expect("moderator account-creation attempt should send");
        assert_eq!(
            read_control_frame(&mut moderator_receive)
                .await
                .expect("administrator-required rejection should arrive"),
            ControlMessage::AdminResponse(AdminResponse::Rejected(
                AdminRejection::AdministratorRequired,
            ))
        );

        write_control_frame(
            &mut moderator_send,
            &ControlMessage::Heartbeat { tick: SimTick(0) },
        )
        .await
        .expect("unexpected admin message should send");
        assert_eq!(
            read_control_frame(&mut moderator_receive)
                .await
                .expect("unexpected-message rejection should arrive"),
            ControlMessage::AdminResponse(AdminResponse::Rejected(
                AdminRejection::UnexpectedMessage,
            ))
        );
        tokio::time::timeout(Duration::from_secs(2), moderator_connection.closed())
            .await
            .expect("invalid admin stream should close promptly");

        let administrator_client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(administrator_secret)
            .bind()
            .await
            .expect("administrator endpoint should bind");
        let administrator_connection = administrator_client
            .connect(server_address.clone(), ADMIN_ALPN)
            .await
            .expect("administrator should connect");
        let (mut administrator_send, mut administrator_receive) = administrator_connection
            .open_bi()
            .await
            .expect("administrator control stream should open");
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminHello(cdda_protocol::AdminHello {
                protocol_version: PROTOCOL_VERSION,
            }),
        )
        .await
        .expect("administrator hello should send");
        assert_eq!(
            read_control_frame(&mut administrator_receive)
                .await
                .expect("administrator ready should arrive"),
            ControlMessage::AdminResponse(AdminResponse::Ready {
                account_id: administrator_id,
                role: AccountRole::Administrator,
            })
        );
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminRequest(AdminRequest::InspectCharacter {
                actor_id,
                inventory_after: None,
                inventory_limit: cdda_protocol::MAX_ADMIN_INVENTORY_PER_PAGE,
            }),
        )
        .await
        .expect("administrator private inspection should send");
        let ControlMessage::AdminResponse(AdminResponse::PrivateCharacter(private_character)) =
            read_control_frame(&mut administrator_receive)
                .await
                .expect("private character inspection should arrive")
        else {
            panic!("expected private character inspection");
        };
        assert_eq!(private_character.account_id, player_id);
        assert_eq!(private_character.actor_id, actor_id);
        assert_eq!(private_character.name, "Moderated Survivor");
        assert!(!private_character.connected);
        assert_eq!(private_character.inventory_total, 0);
        assert!(private_character.inventory.is_empty());
        assert_eq!(private_character.next_inventory_after, None);
        let invalid_endpoint_bytes = (0_u32..1_024)
            .find_map(|candidate| {
                let mut bytes = [0_u8; 32];
                bytes[..4].copy_from_slice(&candidate.to_le_bytes());
                EndpointId::from_bytes(&bytes).is_err().then_some(bytes)
            })
            .expect("the bounded fixture search should find a non-curve encoding");
        let invalid_admin_endpoint = EndpointIdentity(invalid_endpoint_bytes);
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminRequest(AdminRequest::CreateAccount {
                display_name: String::from("Invalid Endpoint"),
                role: AccountRole::Player,
                endpoint: invalid_admin_endpoint,
            }),
        )
        .await
        .expect("invalid administrator endpoint should send");
        assert_eq!(
            read_control_frame(&mut administrator_receive)
                .await
                .expect("invalid endpoint rejection should arrive"),
            ControlMessage::AdminResponse(
                AdminResponse::Rejected(AdminRejection::InvalidEndpoint,)
            )
        );
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminRequest(AdminRequest::CreateAccount {
                display_name: String::from("Created Through Iroh"),
                role: AccountRole::Player,
                endpoint: created_account_endpoint,
            }),
        )
        .await
        .expect("administrator account creation should send");
        let ControlMessage::AdminResponse(AdminResponse::AccountCreated {
            account: created_account,
            pending_endpoint,
        }) = read_control_frame(&mut administrator_receive)
            .await
            .expect("created account should arrive")
        else {
            panic!("expected a created account");
        };
        assert_eq!(created_account.account_id, AccountId::new(83, 4));
        assert_eq!(created_account.status, AccountStatus::InitialEnrollment);
        assert_eq!(pending_endpoint.endpoint, created_account_endpoint);
        assert_eq!(
            pending_endpoint.state,
            cdda_protocol::EndpointBindingState::Pending
        );

        let created_account_client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(created_account_secret)
            .bind()
            .await
            .expect("created account endpoint should bind");
        let created_account_connection = created_account_client
            .connect(server_address, ENROLL_ALPN)
            .await
            .expect("created account should connect for proof");
        let (mut created_send, mut created_receive) = created_account_connection
            .open_bi()
            .await
            .expect("created account enrollment stream should open");
        write_control_frame(
            &mut created_send,
            &ControlMessage::EnrollmentRequest {
                protocol_version: PROTOCOL_VERSION,
            },
        )
        .await
        .expect("created account proof should send");
        created_send.finish().expect("proof stream should finish");
        let ControlMessage::EnrollmentAccepted(enrolled) = read_control_frame(&mut created_receive)
            .await
            .expect("created account enrollment should arrive")
        else {
            panic!("created endpoint should enroll");
        };
        assert_eq!(enrolled.account_id, created_account.account_id);

        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminRequest(AdminRequest::AddEndpoint {
                account_id: created_account.account_id,
                endpoint: discarded_endpoint,
            }),
        )
        .await
        .expect("administrator endpoint staging should send");
        let ControlMessage::AdminResponse(AdminResponse::EndpointPending {
            account_id,
            binding,
        }) = read_control_frame(&mut administrator_receive)
            .await
            .expect("pending endpoint should arrive")
        else {
            panic!("expected a pending endpoint");
        };
        assert_eq!(account_id, created_account.account_id);
        assert_eq!(binding.endpoint, discarded_endpoint);
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminRequest(AdminRequest::RevokeEndpoint {
                account_id: created_account.account_id,
                endpoint: discarded_endpoint,
            }),
        )
        .await
        .expect("administrator pending-endpoint revocation should send");
        assert_eq!(
            read_control_frame(&mut administrator_receive)
                .await
                .expect("endpoint revocation should arrive"),
            ControlMessage::AdminResponse(AdminResponse::EndpointRevoked {
                account_id: created_account.account_id,
                endpoint: discarded_endpoint,
            })
        );
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::AdminRequest(AdminRequest::ListEndpoints {
                account_id: created_account.account_id,
            }),
        )
        .await
        .expect("administrator endpoint list should send");
        let ControlMessage::AdminResponse(AdminResponse::Endpoints {
            account_id,
            bindings,
        }) = read_control_frame(&mut administrator_receive)
            .await
            .expect("administrator endpoint list should arrive")
        else {
            panic!("expected endpoint bindings");
        };
        assert_eq!(account_id, created_account.account_id);
        assert_eq!(bindings.len(), 2);
        assert!(bindings.iter().any(|binding| {
            binding.endpoint == created_account_endpoint
                && binding.state == cdda_protocol::EndpointBindingState::Active
        }));
        assert!(bindings.iter().any(|binding| {
            binding.endpoint == discarded_endpoint
                && binding.state == cdda_protocol::EndpointBindingState::Revoked
        }));
        write_control_frame(
            &mut administrator_send,
            &ControlMessage::Heartbeat { tick: SimTick(0) },
        )
        .await
        .expect("administrator close message should send");
        assert!(matches!(
            read_control_frame(&mut administrator_receive)
                .await
                .expect("administrator close rejection should arrive"),
            ControlMessage::AdminResponse(AdminResponse::Rejected(
                AdminRejection::UnexpectedMessage
            ))
        ));
        tokio::time::timeout(Duration::from_secs(2), administrator_connection.closed())
            .await
            .expect("administrator stream should close promptly");
        let results = tokio::time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("handlers should stop")
            .expect("server task should join");
        assert!(results.into_iter().all(|result| result.is_ok()));
        player_client.close().await;
        moderator_client.close().await;
        administrator_client.close().await;
        created_account_client.close().await;
        server.close().await;
        stop_acknowledger
            .send(())
            .expect("acknowledger should still be running");
        let host = acknowledger.join().expect("acknowledger should join");
        assert_eq!(host.shutdown(), SimulationExit::Requested);
        persistence_host.shutdown();
        let store = WorldStore::open(&database_path).expect("audited store should reopen");
        assert!(matches!(
            store.authorize_endpoint(
                player_endpoint,
                utc_now_seconds().expect("clock should work"),
            ),
            Err(StoreError::AccountUnavailable)
        ));
        assert_eq!(
            store
                .authorize_endpoint(
                    created_account_endpoint,
                    utc_now_seconds().expect("clock should work"),
                )
                .expect("administrator-created identity should remain enrolled")
                .id,
            created_account.account_id
        );
        let audit = store
            .security_audit_after(0)
            .expect("security audit should verify");
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::OpenAdmin
            ) && record.outcome == cdda_persistence::SecurityAuditOutcomeV1::Allowed
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::AdminCreateAccount {
                    account_id: Some(account_id),
                    endpoint,
                    ..
                } if account_id == created_account.account_id
                    && endpoint == created_account_endpoint
            ) && record.outcome == cdda_persistence::SecurityAuditOutcomeV1::Allowed
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::AdminCreateAccount {
                    account_id: None,
                    endpoint,
                    ..
                } if endpoint == created_account_endpoint
            ) && record.outcome
                == cdda_persistence::SecurityAuditOutcomeV1::Rejected(
                    cdda_persistence::SecurityAuditRejectionV1::AdministratorRequired,
                )
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::AdminCreateAccount {
                    account_id: None,
                    endpoint,
                    ..
                } if endpoint == invalid_admin_endpoint
            ) && record.outcome
                == cdda_persistence::SecurityAuditOutcomeV1::Rejected(
                    cdda_persistence::SecurityAuditRejectionV1::InvalidRequest,
                )
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::RejectAdminMessage
            ) && record.outcome
                == cdda_persistence::SecurityAuditOutcomeV1::Rejected(
                    cdda_persistence::SecurityAuditRejectionV1::InvalidRequest,
                )
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::SetMute {
                    account_id,
                    duration_seconds: Some(60),
                } if account_id == player_id
            ) && record.outcome == cdda_persistence::SecurityAuditOutcomeV1::Allowed
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::SetSuspension {
                    account_id,
                    duration_seconds: Some(60),
                } if account_id == player_id
            ) && record.outcome == cdda_persistence::SecurityAuditOutcomeV1::Allowed
        }));
        drop(store);
        remove_database(&database_path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn authorized_client_creates_character_and_moves_over_iroh() {
        let database_path = temporary_key_path().with_extension("game.db");
        remove_database(&database_path);
        let client_secret = SecretKey::generate();
        let client_identity = EndpointIdentity(*client_secret.public().as_bytes());
        let replacement_secret = SecretKey::generate();
        let replacement_identity = EndpointIdentity(*replacement_secret.public().as_bytes());
        let account_id;
        let simulation_block;
        {
            let mut store = WorldStore::open(&database_path).expect("store should open");
            store
                .initialize_world(81, [6; 32])
                .expect("world should initialize");
            let account_block = store
                .reserve_id_block()
                .expect("account block should reserve");
            account_id = AccountId::new(81, account_block.start);
            store
                .create_pending_account(
                    account_id,
                    "Game Ada",
                    AccountRole::Player,
                    client_identity,
                    utc_now_seconds().expect("clock should work"),
                )
                .expect("pending account should be created");
            store
                .enroll_endpoint(
                    client_identity,
                    utc_now_seconds().expect("clock should work"),
                )
                .expect("account should enroll");
            simulation_block = store
                .reserve_id_block()
                .expect("simulation block should reserve");
        }
        let persistence_host = PersistenceHost::start(
            WorldStore::open(&database_path).expect("worker store should open"),
        )
        .expect("persistence worker should start");
        let persistence = persistence_host.handle();
        let mut world = WorldState::new(81, [6; 32]);
        world
            .advance_allocator_high_water(simulation_block.start - 1)
            .expect("account reservation should burn");
        world
            .install_reserved_block(
                ReservedIdBlock::new(simulation_block.start, simulation_block.end)
                    .expect("block should be valid"),
            )
            .expect("simulation block should install");
        world.insert_chunk(Chunk::floor(cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 }));
        let ground_item = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position: cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 },
                type_id: String::from("test_meal"),
                charges: 1,
                melee_damage_milli: BTreeMap::from([(String::from("bash"), 7_000)]),
                calories: 100,
                quench: 10,
                comestible_type: String::from("FOOD"),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("test item should spawn");
        let ranged_weapon = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position: cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 },
                type_id: String::from("test_revolver"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: Some(RangedWeaponSnapshot {
                    ammunition_type: String::from("38"),
                    ammunition_remaining: 2,
                    ammunition_capacity: 6,
                    range: 10,
                    damage: 10,
                    dispersion: 0,
                    sound_volume: 0,
                }),
            })
            .expect("test ranged weapon should spawn");
        let ranged_ammunition = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position: cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 },
                type_id: String::from("test_38_special"),
                charges: 5,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::from("38"),
                ranged_weapon: None,
            })
            .expect("test ranged ammunition should spawn");
        let ranged_target = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_target"),
                position: cdda_protocol::WorldPosition { x: 4, y: 0, z: 0 },
                hp: 10,
                speed: 1,
                attack_cost_moves: 100,
                aggression: 0,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 0,
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
            .expect("ranged target should spawn");
        let craft_rock = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position: cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("craft_rock"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("craft rock should spawn");
        let craft_socks = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position: cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("craft_socks"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("craft socks should spawn");
        let study_book = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position: cdda_protocol::WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("manual_pistol"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("study book should spawn");
        let mut authoritative_recipe = server_test_recipe("network_craft", "authoritative_output");
        authoritative_recipe.time_moves = 1;
        authoritative_recipe.components = vec![
            vec![cdda_protocol::CraftComponentRequirementV1 {
                type_id: String::from("craft_rock"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }],
            vec![cdda_protocol::CraftComponentRequirementV1 {
                type_id: String::from("craft_socks"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }],
        ];
        let crafting = CraftingCatalog::new(BTreeMap::from([(
            authoritative_recipe.recipe_id.clone(),
            authoritative_recipe,
        )]));
        let reading = ReadingCatalog::new(BTreeMap::from([(
            String::from("manual_pistol"),
            BookStudyV1 {
                book_type_id: String::from("manual_pistol"),
                skill_id: String::from("pistol"),
                required_skill_level: 0,
                maximum_skill_level: 3,
                intelligence_requirement: 3,
                time_moves: 1,
                source_time_minutes: 15,
            },
        )]));
        let host = SimulationHost::start_with_catalogs(world, crafting, reading)
            .expect("simulation should start with content catalogs");
        let simulation = host.handle();
        let committed_events = CommittedEventHub::default();
        let (stop_acknowledger, acknowledger) =
            start_test_acknowledger(host, committed_events.clone());
        let character_creator =
            start_test_character_creator(persistence.clone(), simulation.clone());
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [4; 32],
            enabled_mods: Vec::new(),
        };

        let server = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(SecretKey::generate())
            .alpns(vec![GAME_ALPN.to_vec()])
            .bind()
            .await
            .expect("server endpoint should bind");
        let server_address = server.addr();
        let serving_endpoint = server.clone();
        let serving_persistence = persistence.clone();
        let serving_simulation = simulation.clone();
        let serving_content = content.clone();
        let serving_character_creator = character_creator.clone();
        let server_task = tokio::spawn(async move {
            let incoming = serving_endpoint
                .accept()
                .await
                .expect("server should receive connection");
            let connection = incoming.await.expect("handshake should complete");
            handle_game_connection(
                &connection,
                serving_persistence,
                serving_simulation,
                serving_content,
                serving_character_creator,
                committed_events,
                ChatHub::default(),
            )
            .await
        });

        let client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(client_secret)
            .bind()
            .await
            .expect("client endpoint should bind");
        let connection = client
            .connect(server_address, GAME_ALPN)
            .await
            .expect("game connection should establish");
        let (mut send, mut receive) = connection
            .open_bi()
            .await
            .expect("control stream should open");
        write_control_frame(
            &mut send,
            &ControlMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                content,
            }),
        )
        .await
        .expect("hello should send");
        assert!(matches!(
            read_control_frame(&mut receive)
                .await
                .expect("server hello"),
            ControlMessage::ServerHello(_)
        ));
        assert_eq!(
            read_control_frame(&mut receive)
                .await
                .expect("character list"),
            ControlMessage::CharacterList(Vec::new())
        );
        write_control_frame(
            &mut send,
            &ControlMessage::AccountKeyRequest(AccountKeyRequest::List),
        )
        .await
        .expect("endpoint list request should send");
        let ControlMessage::AccountKeyResponse(AccountKeyResponse::Bindings(bindings)) =
            read_control_frame(&mut receive)
                .await
                .expect("endpoint list should decode")
        else {
            panic!("expected endpoint bindings");
        };
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].endpoint, client_identity);
        assert_eq!(
            bindings[0].state,
            cdda_protocol::EndpointBindingState::Active
        );
        let invalid_endpoint_bytes = (0_u32..1_024)
            .find_map(|candidate| {
                let mut bytes = [0_u8; 32];
                bytes[..4].copy_from_slice(&candidate.to_le_bytes());
                EndpointId::from_bytes(&bytes).is_err().then_some(bytes)
            })
            .expect("the bounded fixture search should find a non-curve encoding");
        let invalid_identity = EndpointIdentity(invalid_endpoint_bytes);
        assert!(EndpointId::from_bytes(&invalid_identity.0).is_err());
        write_control_frame(
            &mut send,
            &ControlMessage::AccountKeyRequest(AccountKeyRequest::Add {
                endpoint: invalid_identity,
            }),
        )
        .await
        .expect("invalid endpoint add request should send");
        assert_eq!(
            read_control_frame(&mut receive)
                .await
                .expect("invalid endpoint rejection should decode"),
            ControlMessage::AccountKeyResponse(AccountKeyResponse::Rejected(
                AccountKeyRejection::InvalidEndpoint,
            ))
        );
        write_control_frame(
            &mut send,
            &ControlMessage::AccountKeyRequest(AccountKeyRequest::Add {
                endpoint: replacement_identity,
            }),
        )
        .await
        .expect("endpoint add request should send");
        let ControlMessage::AccountKeyResponse(AccountKeyResponse::Pending(pending)) =
            read_control_frame(&mut receive)
                .await
                .expect("pending endpoint should decode")
        else {
            panic!("expected pending endpoint response");
        };
        assert_eq!(pending.endpoint, replacement_identity);
        assert_eq!(pending.state, cdda_protocol::EndpointBindingState::Pending);
        assert!(pending.pending_expires_utc.is_some());
        write_control_frame(
            &mut send,
            &ControlMessage::AccountKeyRequest(AccountKeyRequest::Revoke {
                endpoint: client_identity,
            }),
        )
        .await
        .expect("last-active endpoint revocation should send");
        assert_eq!(
            read_control_frame(&mut receive)
                .await
                .expect("revocation rejection should decode"),
            ControlMessage::AccountKeyResponse(AccountKeyResponse::Rejected(
                AccountKeyRejection::LastActiveEndpoint,
            ))
        );
        write_control_frame(&mut send, &ControlMessage::Heartbeat { tick: SimTick(0) })
            .await
            .expect("pre-selection heartbeat should send");
        write_control_frame(
            &mut send,
            &ControlMessage::CharacterRequest(CharacterRequest::Create {
                name: String::from("Survivor"),
                base_stats: CharacterCreationStatsV1 {
                    strength: 12,
                    dexterity: 11,
                    intelligence: 10,
                    perception: 9,
                },
            }),
        )
        .await
        .expect("create request should send");
        let ready = read_control_frame(&mut receive)
            .await
            .expect("character should become ready");
        let ControlMessage::CharacterReady { actor_id } = ready else {
            panic!("expected ready character");
        };
        let mut event_receive = connection
            .accept_uni()
            .await
            .expect("server should open an event stream");
        assert_eq!(
            read_control_frame(&mut event_receive)
                .await
                .expect("event stream header should decode"),
            ControlMessage::EventStreamReady { actor_id }
        );
        let mut initial_snapshot_stream = connection
            .accept_uni()
            .await
            .expect("server should open the initial snapshot stream");
        let (snapshot_actor, snapshot_sequence, snapshot) =
            read_snapshot_stream(&mut initial_snapshot_stream)
                .await
                .expect("initial snapshot should decode");
        assert_eq!(snapshot_actor, actor_id);
        assert_eq!(snapshot_sequence, 0);
        assert!(snapshot.visible_actors.is_empty());
        assert_eq!(snapshot.controlled_actor.position.x, 0);
        assert_eq!(snapshot.controlled_actor.base_strength, 12);
        assert_eq!(snapshot.controlled_actor.base_dexterity, 11);
        assert_eq!(snapshot.controlled_actor.base_intelligence, 10);
        assert_eq!(snapshot.controlled_actor.base_perception, 9);
        assert_eq!(snapshot.ground_items[0].item.id, ground_item);
        let pushed = tokio::time::timeout(
            Duration::from_secs(1),
            read_snapshot_until(&connection, actor_id, CommandSequence(0), |_| true),
        )
        .await
        .expect("server should push state without a client heartbeat");
        assert_eq!(pushed.controlled_actor.id, actor_id);
        write_control_frame(
            &mut send,
            &ControlMessage::ChatSend {
                text: String::from("still alive"),
            },
        )
        .await
        .expect("chat should send");
        let ControlMessage::ChatReceived(message) = read_control_frame(&mut receive)
            .await
            .expect("chat response should decode")
        else {
            panic!("expected routed chat");
        };
        assert_eq!(message.from_actor, actor_id);
        assert_eq!(message.from_character, "Survivor");
        assert_eq!(message.text, "still alive");
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(1),
                client_tick: snapshot.tick,
                kind: CommandKind::PickUp {
                    item_id: ground_item,
                },
            }),
        )
        .await
        .expect("pickup should send");
        let picked_up =
            read_snapshot_until(&connection, actor_id, CommandSequence(1), |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == ground_item)
            })
            .await;
        assert!(
            !picked_up
                .ground_items
                .iter()
                .any(|ground| ground.item.id == ground_item)
        );
        assert_eq!(picked_up.controlled_actor.inventory[0].id, ground_item);
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(2),
                client_tick: picked_up.tick,
                kind: CommandKind::Consume {
                    item_id: ground_item,
                },
            }),
        )
        .await
        .expect("consume should send");
        let consumed = read_snapshot_until(&connection, actor_id, CommandSequence(2), |snapshot| {
            snapshot.controlled_actor.inventory.is_empty()
        })
        .await;
        assert!(consumed.controlled_actor.inventory.is_empty());
        assert_eq!(
            consumed.controlled_actor.stored_kcal,
            cdda_sim::DEFAULT_STORED_KCAL + 100
        );
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(3),
                client_tick: consumed.tick,
                kind: CommandKind::PickUp {
                    item_id: ranged_weapon,
                },
            }),
        )
        .await
        .expect("ranged weapon pickup should send");
        let armed = read_snapshot_until(&connection, actor_id, CommandSequence(3), |snapshot| {
            snapshot
                .controlled_actor
                .inventory
                .iter()
                .any(|item| item.id == ranged_weapon)
        })
        .await;
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(4),
                client_tick: armed.tick,
                kind: CommandKind::Wield {
                    item_id: ranged_weapon,
                },
            }),
        )
        .await
        .expect("wield should send");
        let wielded = read_snapshot_until(&connection, actor_id, CommandSequence(4), |snapshot| {
            snapshot.controlled_actor.wielded == Some(ranged_weapon)
        })
        .await;
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(5),
                client_tick: wielded.tick,
                kind: CommandKind::ShootCreature {
                    target: ranged_target,
                },
            }),
        )
        .await
        .expect("shot should send");
        let shot_events = read_events_until(&mut event_receive, |events| {
            events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::RangedAttackResolved {
                        source,
                        target: cdda_protocol::RangedTarget::Creature(target),
                        hit: true,
                        ..
                    } if source == actor_id && target == ranged_target
                )
            })
        })
        .await;
        assert!(shot_events.iter().any(|event| matches!(
            event.kind,
            WorldEventKind::CreatureDied {
                creature_id,
                killer,
            } if creature_id == ranged_target && killer == actor_id
        )));
        let shot = read_snapshot_until(&connection, actor_id, CommandSequence(5), |snapshot| {
            snapshot
                .creatures
                .iter()
                .all(|creature| creature.id != ranged_target)
        })
        .await;
        assert_eq!(
            shot.controlled_actor.inventory[0]
                .ranged_weapon
                .as_ref()
                .expect("ranged stats should replicate")
                .ammunition_remaining,
            1
        );
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(6),
                client_tick: shot.tick,
                kind: CommandKind::PickUp {
                    item_id: ranged_ammunition,
                },
            }),
        )
        .await
        .expect("ammunition pickup should send");
        let ammunition_picked_up =
            read_snapshot_until(&connection, actor_id, CommandSequence(6), |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == ranged_ammunition)
            })
            .await;
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(7),
                client_tick: ammunition_picked_up.tick,
                kind: CommandKind::Reload {
                    ammunition_item: ranged_ammunition,
                    target_pocket_index: None,
                },
            }),
        )
        .await
        .expect("reload should send");
        let reloaded = read_snapshot_until(&connection, actor_id, CommandSequence(7), |snapshot| {
            snapshot
                .controlled_actor
                .inventory
                .iter()
                .find(|item| item.id == ranged_weapon)
                .and_then(|item| item.ranged_weapon.as_ref())
                .is_some_and(|weapon| weapon.ammunition_remaining == 6)
        })
        .await;
        assert!(
            !reloaded
                .controlled_actor
                .inventory
                .iter()
                .any(|item| item.id == ranged_ammunition)
        );
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(8),
                client_tick: reloaded.tick,
                kind: CommandKind::Move {
                    dx: 1,
                    dy: 0,
                    dz: 0,
                },
            }),
        )
        .await
        .expect("move should send");
        let moved = read_snapshot_until(&connection, actor_id, CommandSequence(8), |snapshot| {
            snapshot.controlled_actor.position.x == 1
        })
        .await;
        assert_eq!(moved.controlled_actor.position.x, 1);

        for sequence in 1..=22 {
            let encoded =
                encode_client_datagram(&ClientDatagramV1::HeldMovement(HeldMovementInputV1 {
                    actor_id,
                    sequence: HeldInputSequence(sequence),
                    client_tick: moved.tick,
                    direction: Some(HorizontalDirection { dx: 0, dy: 1 }),
                }))
                .expect("held movement should encode");
            assert!(encoded.len() <= require_datagram_support(&connection).expect("datagrams"));
            connection
                .send_datagram(encoded.into())
                .expect("held movement should send over real iroh");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let stop = encode_client_datagram(&ClientDatagramV1::HeldMovement(HeldMovementInputV1 {
            actor_id,
            sequence: HeldInputSequence(23),
            client_tick: moved.tick,
            direction: None,
        }))
        .expect("held movement release should encode");
        connection
            .send_datagram(stop.into())
            .expect("held movement release should send");
        let held_move =
            read_snapshot_until(&connection, actor_id, CommandSequence(8), |snapshot| {
                snapshot.controlled_actor.position.y == 1
                    && snapshot.controlled_actor.last_held_input_sequence >= HeldInputSequence(23)
                    && snapshot.controlled_actor.held_movement.is_none()
            })
            .await;
        assert_eq!(held_move.controlled_actor.position.y, 1);

        let abandoned =
            encode_client_datagram(&ClientDatagramV1::HeldMovement(HeldMovementInputV1 {
                actor_id,
                sequence: HeldInputSequence(24),
                client_tick: held_move.tick,
                direction: Some(HorizontalDirection { dx: 1, dy: 0 }),
            }))
            .expect("abandoned held movement should encode");
        connection
            .send_datagram(abandoned.into())
            .expect("abandoned held movement should send");
        let lease_cleared =
            read_snapshot_until(&connection, actor_id, CommandSequence(8), |snapshot| {
                snapshot.controlled_actor.last_held_input_sequence == HeldInputSequence(24)
                    && snapshot.controlled_actor.held_movement.is_none()
            })
            .await;
        assert_eq!(lease_cleared.controlled_actor.position.x, 1);
        assert_eq!(lease_cleared.controlled_actor.position.y, 1);
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(9),
                client_tick: lease_cleared.tick,
                kind: CommandKind::Sleep,
            }),
        )
        .await
        .expect("sleep command should send");
        let sleep_rejection = read_events_until(&mut event_receive, |events| {
            events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::CommandRejected {
                        actor_id: event_actor,
                        sequence: CommandSequence(9),
                        reason: cdda_protocol::CommandRejection::NotTired,
                    } if event_actor == actor_id
                )
            })
        })
        .await;
        assert!(sleep_rejection.iter().any(|event| matches!(
            event.kind,
            WorldEventKind::CommandRejected {
                actor_id: event_actor,
                sequence: CommandSequence(9),
                reason: cdda_protocol::CommandRejection::NotTired,
            } if event_actor == actor_id
        )));

        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(10),
                client_tick: lease_cleared.tick,
                kind: CommandKind::PickUp {
                    item_id: craft_rock,
                },
            }),
        )
        .await
        .expect("craft rock pickup should send");
        let has_rock =
            read_snapshot_until(&connection, actor_id, CommandSequence(10), |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == craft_rock)
            })
            .await;
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(11),
                client_tick: has_rock.tick,
                kind: CommandKind::PickUp {
                    item_id: craft_socks,
                },
            }),
        )
        .await
        .expect("craft socks pickup should send");
        let has_components =
            read_snapshot_until(&connection, actor_id, CommandSequence(11), |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == craft_socks)
            })
            .await;
        let mut attacker_recipe = server_test_recipe("network_craft", "attacker_output");
        attacker_recipe.time_moves = 1;
        attacker_recipe.components[0][0].type_id = String::from("craft_rock");
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(12),
                client_tick: has_components.tick,
                kind: CommandKind::Craft {
                    recipe_id: String::from("network_craft"),
                    recipe: Some(Box::new(attacker_recipe)),
                },
            }),
        )
        .await
        .expect("untrusted craft request should send");
        let craft_events = read_events_until(&mut event_receive, |events| {
            events.iter().any(|event| {
                matches!(
                    &event.kind,
                    WorldEventKind::CraftCompleted {
                        actor_id: event_actor,
                        recipe_id,
                        ..
                    } if *event_actor == actor_id && recipe_id == "network_craft"
                )
            })
        })
        .await;
        assert!(craft_events.iter().any(|event| matches!(
            &event.kind,
            WorldEventKind::CraftCompleted { actor_id: event_actor, .. }
                if *event_actor == actor_id
        )));
        let crafted = read_snapshot_until(&connection, actor_id, CommandSequence(12), |snapshot| {
            snapshot
                .controlled_actor
                .inventory
                .iter()
                .any(|item| item.type_id == "authoritative_output")
        })
        .await;
        assert!(
            !crafted
                .controlled_actor
                .inventory
                .iter()
                .any(|item| item.id == craft_rock
                    || item.id == craft_socks
                    || item.type_id == "attacker_output")
        );

        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(13),
                client_tick: crafted.tick,
                kind: CommandKind::PickUp {
                    item_id: study_book,
                },
            }),
        )
        .await
        .expect("study-book pickup should send");
        let has_book =
            read_snapshot_until(&connection, actor_id, CommandSequence(13), |snapshot| {
                snapshot
                    .controlled_actor
                    .inventory
                    .iter()
                    .any(|item| item.id == study_book)
            })
            .await;
        write_control_frame(
            &mut send,
            &ControlMessage::Command(ClientCommand {
                actor_id,
                sequence: CommandSequence(14),
                client_tick: has_book.tick,
                kind: CommandKind::ReadBook {
                    item_id: study_book,
                    book_type_id: String::from("manual_pistol"),
                    study: Some(Box::new(BookStudyV1 {
                        book_type_id: String::from("manual_pistol"),
                        skill_id: String::from("fabrication"),
                        required_skill_level: 0,
                        maximum_skill_level: 10,
                        intelligence_requirement: 3,
                        time_moves: 100,
                        source_time_minutes: 60,
                    })),
                },
            }),
        )
        .await
        .expect("untrusted study request should send");
        let study_events = read_events_until(&mut event_receive, |events| {
            events.iter().any(|event| {
                matches!(
                    &event.kind,
                    WorldEventKind::BookStudyCompleted {
                        actor_id: event_actor,
                        skill_id,
                        ..
                    } if *event_actor == actor_id && skill_id == "pistol"
                )
            })
        })
        .await;
        assert!(study_events.iter().any(|event| matches!(
            &event.kind,
            WorldEventKind::BookStudyCompleted { skill_id, .. } if skill_id == "pistol"
        )));
        let studied = read_snapshot_until(&connection, actor_id, CommandSequence(14), |snapshot| {
            snapshot.controlled_actor.read_activity.is_none()
                && snapshot
                    .controlled_actor
                    .skills
                    .iter()
                    .any(|skill| skill.skill_id == "pistol")
        })
        .await;
        assert!(
            studied
                .controlled_actor
                .skills
                .iter()
                .all(|skill| skill.skill_id != "fabrication")
        );

        connection.close(0_u32.into(), b"test complete");
        server_task
            .await
            .expect("server task should join")
            .expect("game handler should finish cleanly");
        let final_snapshot = simulation
            .snapshot(Duration::from_secs(1))
            .expect("final snapshot should arrive");
        assert!(!final_snapshot.actors[0].connected);
        assert_eq!(
            persistence
                .characters_for_account(account_id)
                .expect("character should persist")[0]
                .actor_id,
            actor_id
        );
        client.close().await;
        server.close().await;
        stop_acknowledger
            .send(())
            .expect("acknowledger should still be running");
        let host = acknowledger.join().expect("acknowledger should join");
        assert_eq!(host.shutdown(), SimulationExit::Requested);
        persistence_host.shutdown();
        let store = WorldStore::open(&database_path).expect("audited store should reopen");
        let security_audit = store
            .security_audit_after(0)
            .expect("security audit should verify after the real connection closes");
        assert_eq!(security_audit.len(), 6);
        assert!(security_audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::ListEndpoints {
                    account_id: audited_account,
                } if audited_account == account_id
            ) && record.outcome == cdda_persistence::SecurityAuditOutcomeV1::Allowed
        }));
        assert!(security_audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::AddEndpoint {
                    account_id: audited_account,
                    endpoint,
                } if audited_account == account_id && endpoint == invalid_identity
            ) && record.outcome
                == cdda_persistence::SecurityAuditOutcomeV1::Rejected(
                    cdda_persistence::SecurityAuditRejectionV1::InvalidRequest,
                )
        }));
        assert!(security_audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::AddEndpoint {
                    account_id: audited_account,
                    endpoint,
                } if audited_account == account_id && endpoint == replacement_identity
            ) && record.outcome == cdda_persistence::SecurityAuditOutcomeV1::Allowed
        }));
        assert!(security_audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                cdda_persistence::SecurityAuditActionV1::RevokeEndpoint {
                    account_id: audited_account,
                    endpoint,
                } if audited_account == account_id && endpoint == client_identity
            ) && record.outcome
                == cdda_persistence::SecurityAuditOutcomeV1::Rejected(
                    cdda_persistence::SecurityAuditRejectionV1::LastActiveEndpoint,
                )
        }));
        drop(store);
        remove_database(&database_path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn duplicate_account_session_is_rejected_over_iroh() {
        let database_path = temporary_key_path().with_extension("sessions.db");
        remove_database(&database_path);
        let client_secret = SecretKey::generate();
        let client_identity = EndpointIdentity(*client_secret.public().as_bytes());
        {
            let mut store = WorldStore::open(&database_path).expect("store should open");
            store
                .initialize_world(82, [7; 32])
                .expect("world should initialize");
            store
                .create_pending_account(
                    AccountId::new(82, 1),
                    "Session Ada",
                    AccountRole::Player,
                    client_identity,
                    utc_now_seconds().expect("clock should work"),
                )
                .expect("pending account should be created");
            store
                .enroll_endpoint(
                    client_identity,
                    utc_now_seconds().expect("clock should work"),
                )
                .expect("account should enroll");
        }
        let persistence_host = PersistenceHost::start(
            WorldStore::open(&database_path).expect("worker store should open"),
        )
        .expect("persistence worker should start");
        let persistence = persistence_host.handle();
        let world = WorldState::new(82, [7; 32]);
        let host = SimulationHost::start(world).expect("simulation should start");
        let simulation = host.handle();
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [8; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let sessions = SessionRegistry::default();
        let committed_events = CommittedEventHub::default();
        let (stop_acknowledger, acknowledger) =
            start_test_acknowledger(host, committed_events.clone());
        let (character_creator, _character_creation_requests) = character_creation_channel();
        let server = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(SecretKey::generate())
            .alpns(vec![GAME_ALPN.to_vec()])
            .bind()
            .await
            .expect("server endpoint should bind");
        let server_address = server.addr();
        let serving_endpoint = server.clone();
        let serving_persistence = persistence.clone();
        let serving_simulation = simulation.clone();
        let serving_content = content.clone();
        let serving_sessions = sessions.clone();
        let serving_character_creator = character_creator.clone();
        let server_task = tokio::spawn(async move {
            let mut handlers = tokio::task::JoinSet::new();
            for _ in 0..2 {
                let incoming = serving_endpoint
                    .accept()
                    .await
                    .expect("server should receive connection");
                let connection = incoming.await.expect("handshake should complete");
                let persistence = serving_persistence.clone();
                let simulation = serving_simulation.clone();
                let content = serving_content.clone();
                let sessions = serving_sessions.clone();
                let character_creator = serving_character_creator.clone();
                let committed_events = committed_events.clone();
                handlers.spawn(async move {
                    handle_game_connection_with_sessions(
                        &connection,
                        persistence,
                        simulation,
                        content,
                        sessions,
                        AuthorizationChangeHub::default(),
                        character_creator,
                        committed_events,
                        ChatHub::default(),
                    )
                    .await
                });
            }
            let mut results = Vec::new();
            while let Some(result) = handlers.join_next().await {
                results.push(result.expect("handler should join"));
            }
            results
        });

        let client = Endpoint::builder(presets::N0DisableRelay)
            .secret_key(client_secret)
            .bind()
            .await
            .expect("client endpoint should bind");
        let first = client
            .connect(server_address.clone(), GAME_ALPN)
            .await
            .expect("first connection should establish");
        let (mut first_send, mut first_receive) =
            first.open_bi().await.expect("first stream should open");
        write_control_frame(
            &mut first_send,
            &ControlMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                content: content.clone(),
            }),
        )
        .await
        .expect("first hello should send");
        assert!(matches!(
            read_control_frame(&mut first_receive)
                .await
                .expect("first server hello"),
            ControlMessage::ServerHello(_)
        ));
        assert!(matches!(
            read_control_frame(&mut first_receive)
                .await
                .expect("first character list"),
            ControlMessage::CharacterList(_)
        ));

        let second = client
            .connect(server_address, GAME_ALPN)
            .await
            .expect("second connection should establish");
        let (mut second_send, mut second_receive) =
            second.open_bi().await.expect("second stream should open");
        write_control_frame(
            &mut second_send,
            &ControlMessage::ClientHello(ClientHello {
                protocol_version: PROTOCOL_VERSION,
                content,
            }),
        )
        .await
        .expect("second hello should send");
        assert_eq!(
            read_control_frame(&mut second_receive)
                .await
                .expect("duplicate rejection should arrive"),
            ControlMessage::GameplayRejected(GameplayRejection::SessionAlreadyActive)
        );

        second.close(0_u32.into(), b"duplicate rejected");
        first.close(0_u32.into(), b"test complete");
        let results = tokio::time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("handlers should stop")
            .expect("server task should join");
        assert!(results.into_iter().all(|result| result.is_ok()));
        client.close().await;
        server.close().await;
        stop_acknowledger
            .send(())
            .expect("acknowledger should still be running");
        let host = acknowledger.join().expect("acknowledger should join");
        assert_eq!(host.shutdown(), SimulationExit::Requested);
        persistence_host.shutdown();
        remove_database(&database_path);
    }
}
