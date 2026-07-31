//! SQLite persistence boundary for canonical world state and recovery inputs.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cdda_protocol::{
    AccountId, AccountRole, AccountStatus, ActorConnectionUpdateV1, ActorId, ActorSnapshot,
    AdminRequest, BASELINE_COMMIT, CharacterSummary, ClientCommand, ContentIdentity,
    EndpointBindingState, EndpointBindingSummary, EndpointIdentity, HeldMovementUpdateV1, ItemId,
    MAX_ADMIN_ACCOUNTS_PER_PAGE, MAX_ADMIN_INVENTORY_PER_PAGE, MAX_CHARACTERS_PER_ACCOUNT,
    MAX_MODERATION_DURATION_SECONDS, MAX_MODERATION_HISTORY_PER_PAGE, MAX_REPORT_BYTES,
    MAX_REPORT_CHARACTERS, MAX_REPORTS_PER_PAGE, ModerationHistoryEntry, ModerationKind,
    PROTOCOL_VERSION, ReportId, ReportReason, ReportState, ReportSummary, SimTick, WorldSnapshotV1,
};
use cdda_sim::{ID_RESERVATION_SIZE, ReservedIdBlock, SimError, WorldState, canonical_events_hash};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: i64 = 85;
/// Old Postcard snapshots and journals cannot be decoded after Protocol 107
/// retained canonical EOC programs and item activation profiles.
/// Metadata-only databases may still migrate.
pub const MIN_RECOVERABLE_SCHEMA_VERSION: i64 = 85;
const MAX_SNAPSHOT_DECODED: u64 = 32 * 1024 * 1024;
// A newly created character retains the same bounded 60-tile terrain memory
// that enters canonical snapshots. Production regional terrain exceeds the
// former 4 KiB fixture-era cap, so use the existing canonical snapshot bound.
const MAX_CHARACTER_SPAWN_DECODED: usize = 32 * 1024 * 1024;
const PRE_MIGRATION_BACKUP_FORMAT_VERSION: u16 = 1;
const PRE_MIGRATION_MANIFEST_FILE: &str = "manifest.postcard";
const PRE_MIGRATION_DATABASE_FILE: &str = "world.db";
const PRE_MIGRATION_IDENTITY_FILE: &str = "server-identity.key";
const MAX_PRE_MIGRATION_MANIFEST_BYTES: u64 = 64 * 1024;
pub const REPLAY_FORMAT_VERSION: u16 = 3;
pub const SNAPSHOT_OBJECT_FORMAT_VERSION: u16 = 1;
pub const SECURITY_AUDIT_FORMAT_VERSION: u16 = 1;
pub const ENROLLMENT_LIFETIME_SECONDS: i64 = 10 * 60;
pub const REPLAY_ARCHIVE_INTERVAL_SECONDS: i64 = 60 * 60;
pub const RECOVERY_COMPACTION_INTERVAL_SECONDS: i64 = 24 * 60 * 60;
pub const MAX_ENDPOINT_BINDINGS_PER_ACCOUNT: usize = 256;
pub const MAX_REPORTS_PER_ACCOUNT_PER_HOUR: u16 = 5;
const REPORT_RATE_WINDOW_SECONDS: i64 = 60 * 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalTickV1 {
    pub tick: SimTick,
    pub commands: Vec<ClientCommand>,
    pub held_movement: Vec<HeldMovementUpdateV1>,
    #[serde(default)]
    pub connection_updates: Vec<ActorConnectionUpdateV1>,
    pub events_hash: [u8; 32],
    pub state_hash: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JournalBatchV1 {
    pub ticks: Vec<JournalTickV1>,
    pub allocator_inputs: Vec<AllocatorInputV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AllocatorInputV1 {
    IdBlockAbandoned {
        at_tick: SimTick,
        high_water: u64,
    },
    IdBlockReserved {
        at_tick: SimTick,
        block: ReservedIdBlock,
    },
}

impl AllocatorInputV1 {
    const fn at_tick(self) -> SimTick {
        match self {
            Self::IdBlockAbandoned { at_tick, .. } | Self::IdBlockReserved { at_tick, .. } => {
                at_tick
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CharacterSpawnV1 {
    pub created_tick: SimTick,
    pub created_after_journal_sequence: u64,
    pub actor: ActorSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReplayBundleV1 {
    pub format_version: u16,
    pub baseline_commit: String,
    pub protocol_version: u16,
    pub content: ContentIdentity,
    pub initial_journal_sequence: u64,
    pub initial_snapshot: WorldSnapshotV1,
    pub initial_snapshot_object_hash: [u8; 32],
    pub character_spawns: Vec<CharacterSpawnV1>,
    pub journal_batches: Vec<(u64, JournalBatchV1)>,
    pub initial_security_audit_sequence: u64,
    pub final_security_audit_sequence: u64,
    pub security_audit_records: Vec<(u64, SecurityAuditRecordV1)>,
    pub final_state_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SecurityAuditActorV1 {
    LocalOperator,
    EndpointProof {
        endpoint: EndpointIdentity,
        account_id: Option<AccountId>,
        role: Option<AccountRole>,
    },
    AuthenticatedAccount {
        account_id: AccountId,
        endpoint: EndpointIdentity,
        role: AccountRole,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SecurityAuditActionV1 {
    ListEndpoints {
        account_id: AccountId,
    },
    CreateAccount {
        account_id: AccountId,
        role: AccountRole,
        endpoint: EndpointIdentity,
    },
    EnrollEndpoint {
        endpoint: EndpointIdentity,
    },
    AddEndpoint {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
    RevokeEndpoint {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
    RecoverEndpoint {
        account_id: AccountId,
        replacement: EndpointIdentity,
    },
    OpenAdmin,
    ListAccounts {
        after: Option<AccountId>,
        limit: u16,
    },
    SetAccountRole {
        account_id: AccountId,
        role: AccountRole,
    },
    SetAccountStatus {
        account_id: AccountId,
        status: AccountStatus,
    },
    RejectAdminMessage,
    ListCharacters {
        account_id: AccountId,
    },
    KickAccount {
        account_id: AccountId,
    },
    SetSuspension {
        account_id: AccountId,
        duration_seconds: Option<u32>,
    },
    SetMute {
        account_id: AccountId,
        duration_seconds: Option<u32>,
    },
    TransferCharacter {
        actor_id: ActorId,
        new_owner: AccountId,
    },
    SubmitReport {
        report_id: Option<ReportId>,
        target_actor: ActorId,
        reason: ReportReason,
    },
    ListReports {
        state: Option<ReportState>,
        after: Option<ReportId>,
        limit: u16,
    },
    ListModerationHistory {
        account_id: AccountId,
        after: Option<u64>,
        limit: u16,
    },
    SetReportState {
        report_id: ReportId,
        state: ReportState,
    },
    AdminCreateAccount {
        account_id: Option<AccountId>,
        role: AccountRole,
        endpoint: EndpointIdentity,
    },
    AdminListEndpoints {
        account_id: AccountId,
    },
    AdminAddEndpoint {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
    AdminRevokeEndpoint {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
    InspectPrivateCharacter {
        actor_id: ActorId,
        inventory_after: Option<ItemId>,
        inventory_limit: u16,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SecurityAuditRejectionV1 {
    AccountUnavailable,
    EndpointAlreadyBound,
    EndpointNotRevocable,
    LastActiveEndpoint,
    TooManyBindings,
    UnknownEndpoint,
    EndpointNotPending,
    EnrollmentExpired,
    InvalidRequest,
    AdministratorRequired,
    CannotTargetSelf,
    InvalidTransition,
    LastAdministrator,
    RateLimited,
    ModeratorRequired,
    TargetRoleNotAllowed,
    CharacterUnavailable,
    CharacterNameConflict,
    TooManyCharacters,
    CannotReportSelf,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SecurityAuditOutcomeV1 {
    Allowed,
    Rejected(SecurityAuditRejectionV1),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SecurityAuditRecordV1 {
    pub format_version: u16,
    pub occurred_utc_seconds: i64,
    pub observed_tick: SimTick,
    pub actor: SecurityAuditActorV1,
    pub action: SecurityAuditActionV1,
    pub outcome: SecurityAuditOutcomeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SnapshotObjectV1 {
    pub format_version: u16,
    pub baseline_commit: String,
    pub protocol_version: u16,
    pub content: ContentIdentity,
    pub journal_sequence: u64,
    pub snapshot: WorldSnapshotV1,
    pub state_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayArchiveCursor {
    pub journal_sequence: u64,
    pub security_audit_sequence: u64,
    pub archived_utc_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedReplayArchive {
    pub start: ReplayArchiveCursor,
    pub end: ReplayArchiveCursor,
    pub bundle: ReplayBundleV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryCompaction {
    pub through_journal_sequence: u64,
    pub deleted_journal_batches: usize,
    pub deleted_snapshots: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatabaseBackupMetadata {
    pub schema_version: i64,
    pub world_namespace: u64,
    pub journal_sequence: u64,
    pub tick: SimTick,
    pub state_hash: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PreMigrationBackupMemberV1 {
    filename: String,
    length: u64,
    checksum: [u8; 32],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PreMigrationBackupManifestV1 {
    format_version: u16,
    source_schema_version: i64,
    created_utc_seconds: u64,
    created_utc_nanoseconds: u32,
    database_checksum: [u8; 32],
    protected_members: Vec<PreMigrationBackupMemberV1>,
}

impl JournalBatchV1 {
    pub fn validate(&self) -> Result<(), StoreError> {
        let valid_allocator_boundary = self.allocator_inputs.is_empty()
            || matches!(
                self.allocator_inputs.as_slice(),
                [
                    AllocatorInputV1::IdBlockAbandoned {
                        at_tick: abandoned_tick,
                        high_water,
                    },
                    AllocatorInputV1::IdBlockReserved {
                        at_tick: reserved_tick,
                        block,
                    },
                ] if abandoned_tick == reserved_tick
                    && high_water.checked_add(1) == Some(block.start)
                    && block.start <= block.end
            );
        if (self.ticks.is_empty() && self.allocator_inputs.is_empty())
            || (!self.ticks.is_empty() && !self.allocator_inputs.is_empty())
            || !valid_allocator_boundary
            || self.ticks.len() > 4_096
            || self.allocator_inputs.len() > 4_096
            || self
                .ticks
                .iter()
                .map(|tick| tick.commands.len())
                .sum::<usize>()
                > 4_096
            || self
                .ticks
                .iter()
                .map(|tick| tick.connection_updates.len())
                .sum::<usize>()
                > 4_096
            || self
                .ticks
                .windows(2)
                .any(|pair| pair[0].tick.0.checked_add(1) != Some(pair[1].tick.0))
            || self
                .allocator_inputs
                .windows(2)
                .any(|pair| pair[0].at_tick() > pair[1].at_tick())
        {
            return Err(StoreError::InvalidRecord);
        }
        Ok(())
    }

    fn first_tick(&self) -> Result<SimTick, StoreError> {
        self.ticks
            .first()
            .map(|tick| tick.tick)
            .or_else(|| self.allocator_inputs.first().map(|input| input.at_tick()))
            .ok_or(StoreError::InvalidRecord)
    }

    fn last_tick(&self) -> Result<SimTick, StoreError> {
        self.ticks
            .last()
            .map(|tick| tick.tick)
            .or_else(|| self.allocator_inputs.last().map(|input| input.at_tick()))
            .ok_or(StoreError::InvalidRecord)
    }

    fn canonical_hash(&self) -> Result<[u8; 32], StoreError> {
        self.validate()?;
        let encoded = postcard::to_stdvec(self).map_err(StoreError::Postcard)?;
        let mut hasher = blake3::Hasher::new_derive_key("cdda-rust JournalBatchV1");
        hasher.update(&encoded);
        Ok(*hasher.finalize().as_bytes())
    }
}

impl SecurityAuditRecordV1 {
    pub fn validate(&self) -> Result<(), StoreError> {
        if self.format_version != SECURITY_AUDIT_FORMAT_VERSION || self.occurred_utc_seconds <= 0 {
            return Err(StoreError::InvalidRecord);
        }
        let action_is_bounded = match self.action {
            SecurityAuditActionV1::SubmitReport {
                report_id,
                target_actor,
                ..
            } => {
                (match self.outcome {
                    SecurityAuditOutcomeV1::Allowed => report_id
                        .is_some_and(|report_id| report_id.0 > 0 && report_id.0 <= i64::MAX as u64),
                    SecurityAuditOutcomeV1::Rejected(_) => report_id.is_none(),
                }) && target_actor.counter() > 0
            }
            SecurityAuditActionV1::ListReports { after, limit, .. } => {
                after.is_none_or(|report_id| report_id.0 > 0 && report_id.0 <= i64::MAX as u64)
                    && limit > 0
                    && limit <= MAX_REPORTS_PER_PAGE
            }
            SecurityAuditActionV1::SetReportState { report_id, .. } => {
                report_id.0 > 0 && report_id.0 <= i64::MAX as u64
            }
            SecurityAuditActionV1::AdminCreateAccount { account_id, .. } => match self.outcome {
                SecurityAuditOutcomeV1::Allowed => {
                    account_id.is_some_and(|account_id| account_id.counter() > 0)
                }
                SecurityAuditOutcomeV1::Rejected(_) => account_id.is_none(),
            },
            SecurityAuditActionV1::AdminListEndpoints { account_id }
            | SecurityAuditActionV1::AdminAddEndpoint { account_id, .. }
            | SecurityAuditActionV1::AdminRevokeEndpoint { account_id, .. } => {
                account_id.counter() > 0
            }
            SecurityAuditActionV1::ListModerationHistory {
                account_id,
                after,
                limit,
            } => {
                account_id.counter() > 0
                    && after
                        .is_none_or(|history_id| history_id > 0 && history_id <= i64::MAX as u64)
                    && limit > 0
                    && limit <= MAX_MODERATION_HISTORY_PER_PAGE
            }
            SecurityAuditActionV1::InspectPrivateCharacter {
                actor_id,
                inventory_after,
                inventory_limit,
            } => {
                actor_id.counter() > 0
                    && inventory_after.is_none_or(|item_id| {
                        item_id.counter() > 0
                            && item_id.world_namespace() == actor_id.world_namespace()
                    })
                    && inventory_limit > 0
                    && inventory_limit <= MAX_ADMIN_INVENTORY_PER_PAGE
            }
            _ => true,
        };
        if !action_is_bounded {
            return Err(StoreError::InvalidRecord);
        }
        let actor_matches_action = match (self.actor, self.action) {
            (SecurityAuditActorV1::LocalOperator, SecurityAuditActionV1::CreateAccount { .. })
            | (
                SecurityAuditActorV1::LocalOperator,
                SecurityAuditActionV1::RecoverEndpoint { .. },
            ) => true,
            (
                SecurityAuditActorV1::EndpointProof {
                    endpoint: actor,
                    account_id,
                    role,
                },
                SecurityAuditActionV1::EnrollEndpoint { endpoint },
            ) => actor == endpoint && account_id.is_some() == role.is_some(),
            (
                SecurityAuditActorV1::AuthenticatedAccount { .. },
                SecurityAuditActionV1::ListEndpoints { .. }
                | SecurityAuditActionV1::AddEndpoint { .. }
                | SecurityAuditActionV1::RevokeEndpoint { .. }
                | SecurityAuditActionV1::SubmitReport { .. },
            ) => true,
            (
                SecurityAuditActorV1::AuthenticatedAccount {
                    role: AccountRole::Moderator | AccountRole::Administrator,
                    ..
                },
                SecurityAuditActionV1::OpenAdmin
                | SecurityAuditActionV1::ListAccounts { .. }
                | SecurityAuditActionV1::ListCharacters { .. }
                | SecurityAuditActionV1::ListReports { .. }
                | SecurityAuditActionV1::ListModerationHistory { .. }
                | SecurityAuditActionV1::SetReportState { .. }
                | SecurityAuditActionV1::KickAccount { .. }
                | SecurityAuditActionV1::SetSuspension { .. }
                | SecurityAuditActionV1::SetMute { .. },
            ) => true,
            (
                SecurityAuditActorV1::AuthenticatedAccount {
                    role: AccountRole::Administrator,
                    ..
                },
                SecurityAuditActionV1::SetAccountRole { .. }
                | SecurityAuditActionV1::SetAccountStatus { .. }
                | SecurityAuditActionV1::TransferCharacter { .. }
                | SecurityAuditActionV1::AdminCreateAccount { .. }
                | SecurityAuditActionV1::AdminListEndpoints { .. }
                | SecurityAuditActionV1::AdminAddEndpoint { .. }
                | SecurityAuditActionV1::AdminRevokeEndpoint { .. }
                | SecurityAuditActionV1::InspectPrivateCharacter { .. },
            ) => true,
            (
                SecurityAuditActorV1::EndpointProof {
                    account_id, role, ..
                },
                SecurityAuditActionV1::ListEndpoints { .. }
                | SecurityAuditActionV1::AddEndpoint { .. }
                | SecurityAuditActionV1::RevokeEndpoint { .. },
            ) => {
                matches!(self.outcome, SecurityAuditOutcomeV1::Rejected(_))
                    && account_id.is_some() == role.is_some()
            }
            (
                SecurityAuditActorV1::AuthenticatedAccount { .. }
                | SecurityAuditActorV1::EndpointProof { .. },
                SecurityAuditActionV1::OpenAdmin
                | SecurityAuditActionV1::ListAccounts { .. }
                | SecurityAuditActionV1::SetAccountRole { .. }
                | SecurityAuditActionV1::SetAccountStatus { .. }
                | SecurityAuditActionV1::ListCharacters { .. }
                | SecurityAuditActionV1::KickAccount { .. }
                | SecurityAuditActionV1::SetSuspension { .. }
                | SecurityAuditActionV1::SetMute { .. }
                | SecurityAuditActionV1::TransferCharacter { .. }
                | SecurityAuditActionV1::SubmitReport { .. }
                | SecurityAuditActionV1::ListReports { .. }
                | SecurityAuditActionV1::ListModerationHistory { .. }
                | SecurityAuditActionV1::SetReportState { .. }
                | SecurityAuditActionV1::AdminCreateAccount { .. }
                | SecurityAuditActionV1::AdminListEndpoints { .. }
                | SecurityAuditActionV1::AdminAddEndpoint { .. }
                | SecurityAuditActionV1::AdminRevokeEndpoint { .. }
                | SecurityAuditActionV1::InspectPrivateCharacter { .. }
                | SecurityAuditActionV1::RejectAdminMessage,
            ) => matches!(self.outcome, SecurityAuditOutcomeV1::Rejected(_)),
            _ => false,
        };
        if !actor_matches_action {
            return Err(StoreError::InvalidRecord);
        }
        Ok(())
    }

    fn canonical_hash(&self) -> Result<[u8; 32], StoreError> {
        self.validate()?;
        let encoded = postcard::to_stdvec(self).map_err(StoreError::Postcard)?;
        let mut hasher = blake3::Hasher::new_derive_key("cdda-rust SecurityAuditRecordV1");
        hasher.update(&encoded);
        Ok(*hasher.finalize().as_bytes())
    }
}

impl ReplayBundleV1 {
    pub fn verify(&self, expected_content: &ContentIdentity) -> Result<WorldState, StoreError> {
        if self.format_version != REPLAY_FORMAT_VERSION
            || self.baseline_commit != BASELINE_COMMIT
            || self.protocol_version != PROTOCOL_VERSION
            || &self.content != expected_content
        {
            return Err(StoreError::InvalidRecord);
        }
        let snapshot_object = self.snapshot_object()?;
        if snapshot_object.canonical_hash()? != self.initial_snapshot_object_hash {
            return Err(StoreError::StateHashMismatch);
        }
        validate_security_audit_range(
            self.initial_security_audit_sequence,
            self.final_security_audit_sequence,
            &self.security_audit_records,
        )?;
        let world = snapshot_object.verify(expected_content)?;
        let (_sequence, world) = replay_parts(
            self.initial_journal_sequence,
            world,
            &self.character_spawns,
            &self.journal_batches,
        )?;
        if world.canonical_hash().map_err(StoreError::Simulation)? != self.final_state_hash {
            return Err(StoreError::ReplayHashMismatch);
        }
        Ok(world)
    }

    pub fn snapshot_object(&self) -> Result<SnapshotObjectV1, StoreError> {
        SnapshotObjectV1::new(
            self.content.clone(),
            self.initial_journal_sequence,
            self.initial_snapshot.clone(),
        )
    }
}

impl SnapshotObjectV1 {
    pub fn new(
        content: ContentIdentity,
        journal_sequence: u64,
        snapshot: WorldSnapshotV1,
    ) -> Result<Self, StoreError> {
        let world = WorldState::from_snapshot(&snapshot).map_err(StoreError::Simulation)?;
        Ok(Self {
            format_version: SNAPSHOT_OBJECT_FORMAT_VERSION,
            baseline_commit: BASELINE_COMMIT.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            content,
            journal_sequence,
            state_hash: world.canonical_hash().map_err(StoreError::Simulation)?,
            snapshot,
        })
    }

    pub fn canonical_hash(&self) -> Result<[u8; 32], StoreError> {
        let encoded = postcard::to_stdvec(self).map_err(StoreError::Postcard)?;
        let mut hasher = blake3::Hasher::new_derive_key("cdda-rust SnapshotObjectV1");
        hasher.update(&encoded);
        Ok(*hasher.finalize().as_bytes())
    }

    pub fn verify(&self, expected_content: &ContentIdentity) -> Result<WorldState, StoreError> {
        if self.format_version != SNAPSHOT_OBJECT_FORMAT_VERSION
            || self.baseline_commit != BASELINE_COMMIT
            || self.protocol_version != PROTOCOL_VERSION
            || &self.content != expected_content
        {
            return Err(StoreError::InvalidRecord);
        }
        let world = WorldState::from_snapshot(&self.snapshot).map_err(StoreError::Simulation)?;
        if world.canonical_hash().map_err(StoreError::Simulation)? != self.state_hash {
            return Err(StoreError::StateHashMismatch);
        }
        Ok(world)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldMetadata {
    pub world_namespace: u64,
    pub world_seed: [u8; 32],
    pub id_high_water: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeStart {
    pub previous_exit_was_unclean: bool,
    pub from_utc_seconds: i64,
    pub to_utc_seconds: i64,
}

impl RuntimeStart {
    pub fn elapsed_seconds(self) -> Result<u64, StoreError> {
        if !self.previous_exit_was_unclean {
            return Ok(0);
        }
        u64::try_from(
            self.to_utc_seconds
                .checked_sub(self.from_utc_seconds)
                .ok_or(StoreError::ClockRegression)?,
        )
        .map_err(|_| StoreError::ClockRegression)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountRecord {
    pub id: AccountId,
    pub display_name: String,
    pub role: AccountRole,
    pub status: AccountStatus,
    pub suspended_until_utc: Option<i64>,
    pub muted_until_utc: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountPage {
    pub accounts: Vec<AccountRecord>,
    pub next_after: Option<AccountId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountMutation {
    pub account: AccountRecord,
    pub changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAccountCreation {
    pub account: AccountRecord,
    pub pending_endpoint: EndpointBindingSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterTransfer {
    pub actor_id: ActorId,
    pub previous_owner: AccountId,
    pub new_owner: AccountId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminCharacterIdentity {
    pub account_id: AccountId,
    pub actor_id: ActorId,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportPage {
    pub reports: Vec<ReportSummary>,
    pub next_after: Option<ReportId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModerationHistoryPage {
    pub entries: Vec<ModerationHistoryEntry>,
    pub next_after: Option<u64>,
}

pub struct WorldStore {
    connection: Connection,
}

impl WorldStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        let connection = Connection::open(path).map_err(StoreError::Sqlite)?;
        let mut store = Self { connection };
        store.configure()?;
        if let Some(existing_schema) = existing_schema_version(&store.connection)? {
            if existing_schema > SCHEMA_VERSION {
                return Err(StoreError::UnsupportedSchema(existing_schema));
            }
            if existing_schema < MIN_RECOVERABLE_SCHEMA_VERSION
                && serialized_world_state_present(&store.connection)?
            {
                return Err(StoreError::UnsupportedSchema(existing_schema));
            }
            if existing_schema < SCHEMA_VERSION {
                create_pre_migration_backup(&store.connection, path, existing_schema)?;
            }
        }
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory().map_err(StoreError::Sqlite)?;
        let mut store = Self { connection };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    fn configure(&mut self) -> Result<(), StoreError> {
        self.connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(StoreError::Sqlite)?;
        self.connection
            .pragma_update(None, "synchronous", "FULL")
            .map_err(StoreError::Sqlite)?;
        self.connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY NOT NULL,
                    applied_utc TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS world_metadata (
                    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
                    world_namespace BLOB NOT NULL CHECK (length(world_namespace) = 8),
                    world_seed BLOB NOT NULL CHECK (length(world_seed) = 32),
                    id_high_water BLOB NOT NULL CHECK (length(id_high_water) = 8),
                    account_high_water BLOB NOT NULL DEFAULT X'0000000000000000'
                        CHECK (length(account_high_water) = 8),
                    runtime_state INTEGER NOT NULL DEFAULT 0 CHECK (runtime_state BETWEEN 0 AND 1),
                    last_committed_utc INTEGER NOT NULL DEFAULT 0,
                    replay_archive_sequence INTEGER NOT NULL DEFAULT 0,
                    replay_archive_utc INTEGER NOT NULL DEFAULT 0,
                    replay_pending_sequence INTEGER NOT NULL DEFAULT 0,
                    replay_pending_utc INTEGER NOT NULL DEFAULT 0,
                    replay_archive_security_sequence INTEGER NOT NULL DEFAULT 0,
                    replay_pending_security_sequence INTEGER NOT NULL DEFAULT 0,
                    last_compacted_utc INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS journal_batches (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    first_tick BLOB NOT NULL CHECK (length(first_tick) = 8),
                    last_tick BLOB NOT NULL CHECK (length(last_tick) = 8),
                    payload BLOB NOT NULL,
                    events_hash BLOB NOT NULL CHECK (length(events_hash) = 32)
                );
                CREATE TABLE IF NOT EXISTS snapshots (
                    sequence INTEGER PRIMARY KEY NOT NULL,
                    tick BLOB NOT NULL CHECK (length(tick) = 8),
                    state_hash BLOB NOT NULL CHECK (length(state_hash) = 32),
                    compressed_state BLOB NOT NULL
                );
                CREATE TABLE IF NOT EXISTS security_audit (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    occurred_utc INTEGER NOT NULL CHECK (occurred_utc > 0),
                    payload BLOB NOT NULL CHECK (length(payload) BETWEEN 1 AND 65536),
                    record_hash BLOB NOT NULL CHECK (length(record_hash) = 32)
                );
                CREATE TABLE IF NOT EXISTS accounts (
                    account_id BLOB PRIMARY KEY NOT NULL CHECK (length(account_id) = 16),
                    display_name TEXT NOT NULL CHECK (
                        length(display_name) BETWEEN 1 AND 64
                        AND length(CAST(display_name AS BLOB)) <= 256
                    ),
                    role INTEGER NOT NULL CHECK (role BETWEEN 0 AND 2),
                    status INTEGER NOT NULL CHECK (status BETWEEN 0 AND 4),
                    suspended_until_utc INTEGER CHECK (
                        suspended_until_utc IS NULL OR suspended_until_utc > 0
                    ),
                    muted_until_utc INTEGER CHECK (
                        muted_until_utc IS NULL OR muted_until_utc > 0
                    )
                );
                CREATE TABLE IF NOT EXISTS endpoint_bindings (
                    endpoint_id BLOB PRIMARY KEY NOT NULL CHECK (length(endpoint_id) = 32),
                    account_id BLOB NOT NULL REFERENCES accounts(account_id),
                    state INTEGER NOT NULL CHECK (state BETWEEN 0 AND 2),
                    pending_expires_utc INTEGER,
                    CHECK (
                        (state = 0 AND pending_expires_utc IS NOT NULL)
                        OR (state != 0 AND pending_expires_utc IS NULL)
                    )
                );
                CREATE INDEX IF NOT EXISTS endpoint_bindings_account
                    ON endpoint_bindings(account_id, state);
                CREATE TABLE IF NOT EXISTS characters (
                    actor_id BLOB PRIMARY KEY NOT NULL CHECK (length(actor_id) = 16),
                    account_id BLOB NOT NULL REFERENCES accounts(account_id),
                    name TEXT NOT NULL CHECK (
                        length(name) BETWEEN 1 AND 64
                        AND length(CAST(name AS BLOB)) <= 256
                    ),
                    spawn_state BLOB NOT NULL,
                    spawn_journal_sequence INTEGER NOT NULL CHECK (spawn_journal_sequence >= 0),
                    UNIQUE(account_id, name)
                );
                CREATE INDEX IF NOT EXISTS characters_account
                    ON characters(account_id, actor_id);
                CREATE TABLE IF NOT EXISTS player_reports (
                    report_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    created_utc INTEGER NOT NULL CHECK (created_utc > 0),
                    reporter_account_id BLOB NOT NULL REFERENCES accounts(account_id),
                    reporter_actor_id BLOB NOT NULL REFERENCES characters(actor_id),
                    reporter_character TEXT NOT NULL CHECK (
                        length(reporter_character) BETWEEN 1 AND 64
                        AND length(CAST(reporter_character AS BLOB)) <= 256
                    ),
                    target_account_id BLOB NOT NULL REFERENCES accounts(account_id),
                    target_actor_id BLOB NOT NULL REFERENCES characters(actor_id),
                    target_character TEXT NOT NULL CHECK (
                        length(target_character) BETWEEN 1 AND 64
                        AND length(CAST(target_character AS BLOB)) <= 256
                    ),
                    reason INTEGER NOT NULL CHECK (reason BETWEEN 0 AND 3),
                    details TEXT NOT NULL CHECK (
                        length(details) BETWEEN 1 AND 512
                        AND length(CAST(details AS BLOB)) <= 1024
                    ),
                    state INTEGER NOT NULL DEFAULT 0 CHECK (state BETWEEN 0 AND 2),
                    resolved_utc INTEGER CHECK (resolved_utc IS NULL OR resolved_utc > 0),
                    resolved_by_account_id BLOB REFERENCES accounts(account_id),
                    resolution_audit_sequence INTEGER REFERENCES security_audit(sequence),
                    CHECK (
                        (state = 0 AND resolved_utc IS NULL
                            AND resolved_by_account_id IS NULL
                            AND resolution_audit_sequence IS NULL)
                        OR (state BETWEEN 1 AND 2 AND resolved_utc IS NOT NULL
                            AND resolved_by_account_id IS NOT NULL
                            AND resolution_audit_sequence IS NOT NULL)
                    ),
                    CHECK (reporter_account_id != target_account_id)
                );
                CREATE INDEX IF NOT EXISTS player_reports_target
                    ON player_reports(target_account_id, report_id);
                CREATE TABLE IF NOT EXISTS moderation_history (
                    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    security_audit_sequence INTEGER NOT NULL UNIQUE
                        REFERENCES security_audit(sequence),
                    occurred_utc INTEGER NOT NULL CHECK (occurred_utc > 0),
                    operator_account_id BLOB NOT NULL REFERENCES accounts(account_id),
                    target_account_id BLOB NOT NULL REFERENCES accounts(account_id),
                    kind INTEGER NOT NULL CHECK (kind BETWEEN 0 AND 2),
                    until_utc INTEGER CHECK (until_utc IS NULL OR until_utc > 0)
                );
                CREATE INDEX IF NOT EXISTS moderation_history_target
                    ON moderation_history(target_account_id, history_id);
                ",
            )
            .map_err(StoreError::Sqlite)?;
        let account_columns = {
            let mut statement = transaction
                .prepare("PRAGMA table_info(accounts)")
                .map_err(StoreError::Sqlite)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(StoreError::Sqlite)?;
            let mut names = Vec::new();
            for column in columns {
                names.push(column.map_err(StoreError::Sqlite)?);
            }
            names
        };
        if !account_columns
            .iter()
            .any(|column| column == "suspended_until_utc")
        {
            transaction
                .execute(
                    "ALTER TABLE accounts ADD COLUMN suspended_until_utc INTEGER
                     CHECK (suspended_until_utc IS NULL OR suspended_until_utc > 0)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !account_columns
            .iter()
            .any(|column| column == "muted_until_utc")
        {
            transaction
                .execute(
                    "ALTER TABLE accounts ADD COLUMN muted_until_utc INTEGER
                     CHECK (muted_until_utc IS NULL OR muted_until_utc > 0)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        let report_columns = {
            let mut statement = transaction
                .prepare("PRAGMA table_info(player_reports)")
                .map_err(StoreError::Sqlite)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(StoreError::Sqlite)?;
            let mut names = Vec::new();
            for column in columns {
                names.push(column.map_err(StoreError::Sqlite)?);
            }
            names
        };
        if !report_columns.iter().any(|column| column == "state") {
            transaction
                .execute(
                    "ALTER TABLE player_reports ADD COLUMN state INTEGER NOT NULL
                     DEFAULT 0 CHECK (state BETWEEN 0 AND 2)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !report_columns.iter().any(|column| column == "resolved_utc") {
            transaction
                .execute(
                    "ALTER TABLE player_reports ADD COLUMN resolved_utc INTEGER
                     CHECK (resolved_utc IS NULL OR resolved_utc > 0)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !report_columns
            .iter()
            .any(|column| column == "resolved_by_account_id")
        {
            transaction
                .execute(
                    "ALTER TABLE player_reports ADD COLUMN resolved_by_account_id BLOB
                     REFERENCES accounts(account_id)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !report_columns
            .iter()
            .any(|column| column == "resolution_audit_sequence")
        {
            transaction
                .execute(
                    "ALTER TABLE player_reports ADD COLUMN resolution_audit_sequence INTEGER
                     REFERENCES security_audit(sequence)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        transaction
            .execute(
                "CREATE INDEX IF NOT EXISTS player_reports_state
                 ON player_reports(state, report_id)",
                [],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS player_reports_resolution_audit
                 ON player_reports(resolution_audit_sequence)
                 WHERE resolution_audit_sequence IS NOT NULL",
                [],
            )
            .map_err(StoreError::Sqlite)?;
        let has_spawn_state = {
            let mut statement = transaction
                .prepare("PRAGMA table_info(characters)")
                .map_err(StoreError::Sqlite)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(StoreError::Sqlite)?;
            let mut found = false;
            for column in columns {
                if column.map_err(StoreError::Sqlite)? == "spawn_state" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_spawn_state {
            transaction
                .execute("ALTER TABLE characters ADD COLUMN spawn_state BLOB", [])
                .map_err(StoreError::Sqlite)?;
        }
        let has_spawn_journal_sequence = {
            let mut statement = transaction
                .prepare("PRAGMA table_info(characters)")
                .map_err(StoreError::Sqlite)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(StoreError::Sqlite)?;
            let mut found = false;
            for column in columns {
                if column.map_err(StoreError::Sqlite)? == "spawn_journal_sequence" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_spawn_journal_sequence {
            transaction
                .execute(
                    "ALTER TABLE characters ADD COLUMN spawn_journal_sequence INTEGER NOT NULL DEFAULT 0 CHECK (spawn_journal_sequence >= 0)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        let metadata_columns = {
            let mut statement = transaction
                .prepare("PRAGMA table_info(world_metadata)")
                .map_err(StoreError::Sqlite)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(StoreError::Sqlite)?;
            let mut names = Vec::new();
            for column in columns {
                names.push(column.map_err(StoreError::Sqlite)?);
            }
            names
        };
        if !metadata_columns
            .iter()
            .any(|column| column == "runtime_state")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN runtime_state INTEGER NOT NULL DEFAULT 0 CHECK (runtime_state BETWEEN 0 AND 1)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "replay_archive_sequence")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN replay_archive_sequence INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "replay_archive_utc")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN replay_archive_utc INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "replay_pending_sequence")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN replay_pending_sequence INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "replay_pending_utc")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN replay_pending_utc INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "last_compacted_utc")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN last_compacted_utc INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "replay_archive_security_sequence")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN replay_archive_security_sequence INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "replay_pending_security_sequence")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN replay_pending_security_sequence INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "last_committed_utc")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN last_committed_utc INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        if !metadata_columns
            .iter()
            .any(|column| column == "account_high_water")
        {
            transaction
                .execute(
                    "ALTER TABLE world_metadata ADD COLUMN account_high_water BLOB NOT NULL
                     DEFAULT X'0000000000000000' CHECK (length(account_high_water) = 8)",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
            transaction
                .execute(
                    "UPDATE world_metadata SET account_high_water = id_high_water",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
        }
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [4_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [1_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [2_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [3_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [5_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [6_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [7_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [8_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [10_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [11_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [12_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [13_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [14_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [15_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [16_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [17_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [18_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [19_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [20_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [21_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [22_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [23_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [24_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [25_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [26_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [27_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [28_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [29_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [30_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [31_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [32_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [33_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [34_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [35_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [36_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [37_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [38_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [39_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [40_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [41_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [42_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [43_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [44_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [45_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [46_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [47_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [48_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [49_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [50_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [51_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [52_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [53_i64],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version) VALUES (?1)",
                [SCHEMA_VERSION],
            )
            .map_err(StoreError::Sqlite)?;
        let newest: i64 = transaction
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(StoreError::Sqlite)?;
        if newest != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema(newest));
        }
        transaction.commit().map_err(StoreError::Sqlite)
    }

    pub fn initialize_world(
        &mut self,
        world_namespace: u64,
        world_seed: [u8; 32],
    ) -> Result<(), StoreError> {
        let namespace = world_namespace.to_be_bytes();
        let high_water = 0_u64.to_be_bytes();
        let changed = self
            .connection
            .execute(
                "INSERT OR IGNORE INTO world_metadata(
                    singleton, world_namespace, world_seed, id_high_water
                 ) VALUES (1, ?1, ?2, ?3)",
                params![
                    namespace.as_slice(),
                    world_seed.as_slice(),
                    high_water.as_slice()
                ],
            )
            .map_err(StoreError::Sqlite)?;
        if changed == 0 {
            let existing = self.metadata()?;
            if existing.world_namespace != world_namespace || existing.world_seed != world_seed {
                return Err(StoreError::WorldIdentityMismatch);
            }
        }
        Ok(())
    }

    pub fn metadata(&self) -> Result<WorldMetadata, StoreError> {
        self.metadata_optional()?
            .ok_or(StoreError::WorldUninitialized)
    }

    pub fn metadata_optional(&self) -> Result<Option<WorldMetadata>, StoreError> {
        let row: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> = self
            .connection
            .query_row(
                "SELECT world_namespace, world_seed, id_high_water
                 FROM world_metadata WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let Some((namespace, seed, high_water)) = row else {
            return Ok(None);
        };
        Ok(Some(WorldMetadata {
            world_namespace: decode_u64(&namespace)?,
            world_seed: decode_array(&seed)?,
            id_high_water: decode_u64(&high_water)?,
        }))
    }

    pub fn begin_runtime(&mut self, now_utc_seconds: i64) -> Result<RuntimeStart, StoreError> {
        if now_utc_seconds < 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let (state, anchor): (i64, i64) = transaction
            .query_row(
                "SELECT runtime_state, last_committed_utc FROM world_metadata WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
                other => StoreError::Sqlite(other),
            })?;
        if !matches!(state, 0 | 1) || anchor < 0 || (state == 1 && anchor == 0) {
            return Err(StoreError::CorruptRecord);
        }
        if now_utc_seconds < anchor {
            return Err(StoreError::ClockRegression);
        }
        let unclean = state == 1;
        let from = if unclean { anchor } else { now_utc_seconds };
        let next_anchor = if unclean { anchor } else { now_utc_seconds };
        transaction
            .execute(
                "UPDATE world_metadata SET runtime_state = 1, last_committed_utc = ?1 WHERE singleton = 1",
                [next_anchor],
            )
            .map_err(StoreError::Sqlite)?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(RuntimeStart {
            previous_exit_was_unclean: unclean,
            from_utc_seconds: from,
            to_utc_seconds: now_utc_seconds,
        })
    }

    pub fn finish_runtime(&mut self, now_utc_seconds: i64) -> Result<(), StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        validate_runtime_anchor(&transaction, now_utc_seconds)?;
        transaction
            .execute(
                "UPDATE world_metadata SET runtime_state = 0, last_committed_utc = ?1 WHERE singleton = 1",
                [now_utc_seconds],
            )
            .map_err(StoreError::Sqlite)?;
        transaction.commit().map_err(StoreError::Sqlite)
    }

    pub fn require_runtime_inactive(&self) -> Result<(), StoreError> {
        let state: i64 = self
            .connection
            .query_row(
                "SELECT runtime_state FROM world_metadata WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
                other => StoreError::Sqlite(other),
            })?;
        match state {
            0 => Ok(()),
            1 => Err(StoreError::RuntimeActive),
            _ => Err(StoreError::CorruptRecord),
        }
    }

    pub fn reserve_id_block(&mut self) -> Result<ReservedIdBlock, StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let current_blob: Vec<u8> = transaction
            .query_row(
                "SELECT id_high_water FROM world_metadata WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        let current = decode_u64(&current_blob)?;
        let start = current.checked_add(1).ok_or(StoreError::NumericOverflow)?;
        let end = current
            .checked_add(ID_RESERVATION_SIZE)
            .ok_or(StoreError::NumericOverflow)?;
        let end_blob = end.to_be_bytes();
        transaction
            .execute(
                "UPDATE world_metadata SET id_high_water = ?1 WHERE singleton = 1",
                [end_blob.as_slice()],
            )
            .map_err(StoreError::Sqlite)?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        ReservedIdBlock::new(start, end).map_err(StoreError::Simulation)
    }

    pub fn reserve_account_id(&mut self) -> Result<AccountId, StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let account_id = reserve_account_id_in_transaction(&transaction)?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(account_id)
    }

    pub fn create_pending_account(
        &mut self,
        account_id: AccountId,
        display_name: &str,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        let action = SecurityAuditActionV1::CreateAccount {
            account_id,
            role,
            endpoint,
        };
        let result = self.create_pending_account_allowed(
            account_id,
            display_name,
            role,
            endpoint,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(
            now_utc_seconds,
            SecurityAuditActorV1::LocalOperator,
            action,
            result,
        )
    }

    fn create_pending_account_allowed(
        &mut self,
        account_id: AccountId,
        display_name: &str,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        validate_display_name(display_name)?;
        let metadata = self.metadata()?;
        if account_id.world_namespace() != metadata.world_namespace || account_id.counter() == 0 {
            return Err(StoreError::InvalidStableId);
        }
        let expires = now_utc_seconds
            .checked_add(ENROLLMENT_LIFETIME_SECONDS)
            .ok_or(StoreError::NumericOverflow)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let endpoint_exists = transaction
            .query_row(
                "SELECT 1 FROM endpoint_bindings WHERE endpoint_id = ?1",
                [endpoint.0.as_slice()],
                |_| Ok(()),
            )
            .optional()
            .map_err(StoreError::Sqlite)?
            .is_some();
        if endpoint_exists {
            return Err(StoreError::EndpointAlreadyBound);
        }
        let account_bytes = account_id.as_u128().to_be_bytes();
        transaction
            .execute(
                "INSERT INTO accounts(account_id, display_name, role, status)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    account_bytes.as_slice(),
                    display_name,
                    encode_role(role),
                    encode_status(AccountStatus::InitialEnrollment)
                ],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT INTO endpoint_bindings(endpoint_id, account_id, state, pending_expires_utc)
                 VALUES (?1, ?2, 0, ?3)",
                params![endpoint.0.as_slice(), account_bytes.as_slice(), expires],
            )
            .map_err(StoreError::Sqlite)?;
        let observed_tick = current_persisted_tick(&transaction)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick,
                actor: SecurityAuditActorV1::LocalOperator,
                action: SecurityAuditActionV1::CreateAccount {
                    account_id,
                    role,
                    endpoint,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountRecord {
            id: account_id,
            display_name: display_name.to_owned(),
            role,
            status: AccountStatus::InitialEnrollment,
            suspended_until_utc: None,
            muted_until_utc: None,
        })
    }

    pub fn enroll_endpoint(
        &mut self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, endpoint)?;
        let action = SecurityAuditActionV1::EnrollEndpoint { endpoint };
        let result = self.enroll_endpoint_allowed(endpoint, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn enroll_endpoint_allowed(
        &mut self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let pending: Option<(Vec<u8>, i64, i64)> = transaction
            .query_row(
                "SELECT b.account_id, b.state, b.pending_expires_utc
                 FROM endpoint_bindings b WHERE b.endpoint_id = ?1",
                [endpoint.0.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let Some((account_bytes, state, expires)) = pending else {
            return Err(StoreError::UnknownEndpoint);
        };
        if state != 0 {
            return Err(StoreError::EndpointNotPending);
        }
        if expires < now_utc_seconds {
            return Err(StoreError::EnrollmentExpired);
        }
        let account = query_account(&transaction, &account_bytes)?;
        if !matches!(
            account.status,
            AccountStatus::InitialEnrollment
                | AccountStatus::Enabled
                | AccountStatus::RecoveryLocked
        ) {
            return Err(StoreError::AccountUnavailable);
        }
        transaction
            .execute(
                "UPDATE endpoint_bindings
                 SET state = 1, pending_expires_utc = NULL
                 WHERE endpoint_id = ?1 AND state = 0",
                [endpoint.0.as_slice()],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "UPDATE accounts SET status = ?1 WHERE account_id = ?2",
                params![
                    encode_status(AccountStatus::Enabled),
                    account_bytes.as_slice()
                ],
            )
            .map_err(StoreError::Sqlite)?;
        let observed_tick = current_persisted_tick(&transaction)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick,
                actor: SecurityAuditActorV1::EndpointProof {
                    endpoint,
                    account_id: Some(account.id),
                    role: Some(account.role),
                },
                action: SecurityAuditActionV1::EnrollEndpoint { endpoint },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountRecord {
            status: AccountStatus::Enabled,
            ..account
        })
    }

    pub fn authorize_endpoint(
        &self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let row: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT account_id FROM endpoint_bindings
                 WHERE endpoint_id = ?1 AND state = 1",
                [endpoint.0.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let account_bytes = row.ok_or(StoreError::UnauthorizedEndpoint)?;
        let account = query_account(&self.connection, &account_bytes)?;
        if !account_is_available(&account, now_utc_seconds) {
            return Err(StoreError::AccountUnavailable);
        }
        Ok(account)
    }

    pub fn authorize_admin_endpoint(
        &mut self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, endpoint)?;
        let result = (|| {
            let account = self.authorize_endpoint(endpoint, now_utc_seconds)?;
            if account.role == AccountRole::Player {
                return Err(StoreError::ModeratorRequired);
            }
            Ok(account)
        })();
        match result {
            Ok(account) => {
                self.append_security_audit_attempt(
                    now_utc_seconds,
                    SecurityAuditActorV1::AuthenticatedAccount {
                        account_id: account.id,
                        endpoint,
                        role: account.role,
                    },
                    SecurityAuditActionV1::OpenAdmin,
                    SecurityAuditOutcomeV1::Allowed,
                )?;
                Ok(account)
            }
            Err(error) => self.audit_rejected_security_result(
                now_utc_seconds,
                actor,
                SecurityAuditActionV1::OpenAdmin,
                Err(error),
            ),
        }
    }

    pub fn audit_invalid_admin_message(
        &mut self,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, endpoint)?;
        self.append_security_audit_attempt(
            now_utc_seconds,
            actor,
            SecurityAuditActionV1::RejectAdminMessage,
            SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::InvalidRequest),
        )?;
        Ok(())
    }

    pub fn audit_rate_limited_admin_request(
        &mut self,
        endpoint: EndpointIdentity,
        request: AdminRequest,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, endpoint)?;
        let action = match request {
            AdminRequest::ListAccounts { after, limit } => {
                SecurityAuditActionV1::ListAccounts { after, limit }
            }
            AdminRequest::SetRole { account_id, role } => {
                SecurityAuditActionV1::SetAccountRole { account_id, role }
            }
            AdminRequest::SetStatus { account_id, status } => {
                SecurityAuditActionV1::SetAccountStatus { account_id, status }
            }
            AdminRequest::ListCharacters { account_id } => {
                SecurityAuditActionV1::ListCharacters { account_id }
            }
            AdminRequest::InspectCharacter {
                actor_id,
                inventory_after,
                inventory_limit,
            } => SecurityAuditActionV1::InspectPrivateCharacter {
                actor_id,
                inventory_after,
                inventory_limit,
            },
            AdminRequest::ListReports {
                state,
                after,
                limit,
            } => SecurityAuditActionV1::ListReports {
                state,
                after,
                limit,
            },
            AdminRequest::ListModerationHistory {
                account_id,
                after,
                limit,
            } => SecurityAuditActionV1::ListModerationHistory {
                account_id,
                after,
                limit,
            },
            AdminRequest::Kick { account_id } => SecurityAuditActionV1::KickAccount { account_id },
            AdminRequest::SetSuspension {
                account_id,
                duration_seconds,
            } => SecurityAuditActionV1::SetSuspension {
                account_id,
                duration_seconds,
            },
            AdminRequest::SetMute {
                account_id,
                duration_seconds,
            } => SecurityAuditActionV1::SetMute {
                account_id,
                duration_seconds,
            },
            AdminRequest::TransferCharacter {
                actor_id,
                new_owner,
            } => SecurityAuditActionV1::TransferCharacter {
                actor_id,
                new_owner,
            },
            AdminRequest::SetReportState { report_id, state } => {
                SecurityAuditActionV1::SetReportState { report_id, state }
            }
            AdminRequest::CreateAccount { role, endpoint, .. } => {
                SecurityAuditActionV1::AdminCreateAccount {
                    account_id: None,
                    role,
                    endpoint,
                }
            }
            AdminRequest::ListEndpoints { account_id } => {
                SecurityAuditActionV1::AdminListEndpoints { account_id }
            }
            AdminRequest::AddEndpoint {
                account_id,
                endpoint,
            } => SecurityAuditActionV1::AdminAddEndpoint {
                account_id,
                endpoint,
            },
            AdminRequest::RevokeEndpoint {
                account_id,
                endpoint,
            } => SecurityAuditActionV1::AdminRevokeEndpoint {
                account_id,
                endpoint,
            },
        };
        self.append_security_audit_attempt(
            now_utc_seconds,
            actor,
            action,
            SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::RateLimited),
        )?;
        Ok(())
    }

    pub fn admin_accounts(
        &mut self,
        actor_endpoint: EndpointIdentity,
        after: Option<AccountId>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<AccountPage, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::ListAccounts { after, limit };
        let result = self.admin_accounts_allowed(actor_endpoint, after, limit, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_accounts_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        after: Option<AccountId>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<AccountPage, StoreError> {
        if now_utc_seconds <= 0 || limit == 0 || limit > MAX_ADMIN_ACCOUNTS_PER_PAGE {
            return Err(StoreError::InvalidRecord);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator =
            require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        if let Some(after) = after
            && (after.world_namespace() != administrator.id.world_namespace()
                || after.counter() == 0)
        {
            return Err(StoreError::InvalidStableId);
        }
        let after_bytes = after.map(|account_id| account_id.as_u128().to_be_bytes());
        let row_limit = i64::from(limit) + 1;
        let mut statement = transaction
            .prepare(
                "SELECT account_id, display_name, role, status,
                        suspended_until_utc, muted_until_utc FROM accounts
                 WHERE (?1 IS NULL OR account_id > ?1)
                 ORDER BY account_id ASC LIMIT ?2",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = statement
            .query_map(
                params![after_bytes.as_ref().map(<[u8; 16]>::as_slice), row_limit],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                    ))
                },
            )
            .map_err(StoreError::Sqlite)?;
        let mut accounts = Vec::new();
        for row in rows {
            let (account_id, display_name, role, status, suspended_until_utc, muted_until_utc) =
                row.map_err(StoreError::Sqlite)?;
            let raw = u128::from_be_bytes(decode_array(&account_id)?);
            accounts.push(AccountRecord {
                id: AccountId::new((raw >> 64) as u64, raw as u64),
                display_name,
                role: decode_role(role)?,
                status: decode_status(status)?,
                suspended_until_utc,
                muted_until_utc,
            });
        }
        drop(statement);
        let has_more = accounts.len() > usize::from(limit);
        accounts.truncate(usize::from(limit));
        let next_after = has_more.then(|| {
            accounts
                .last()
                .expect("a page with an overflow row has a returned row")
                .id
        });
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::ListAccounts { after, limit },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountPage {
            accounts,
            next_after,
        })
    }

    pub fn admin_characters(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<Vec<CharacterSummary>, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::ListCharacters { account_id };
        let result = self.admin_characters_allowed(actor_endpoint, account_id, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_characters_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<Vec<CharacterSummary>, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        query_account(&transaction, &account_bytes)?;
        let characters = query_characters(&transaction, account_id)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::ListCharacters { account_id },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(characters)
    }

    pub fn admin_private_character(
        &mut self,
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        inventory_after: Option<ItemId>,
        inventory_limit: u16,
        now_utc_seconds: i64,
    ) -> Result<AdminCharacterIdentity, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::InspectPrivateCharacter {
            actor_id,
            inventory_after,
            inventory_limit,
        };
        let result = self.admin_private_character_allowed(
            actor_endpoint,
            actor_id,
            inventory_after,
            inventory_limit,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_private_character_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        inventory_after: Option<ItemId>,
        inventory_limit: u16,
        now_utc_seconds: i64,
    ) -> Result<AdminCharacterIdentity, StoreError> {
        if now_utc_seconds <= 0
            || actor_id.counter() == 0
            || inventory_limit == 0
            || inventory_limit > MAX_ADMIN_INVENTORY_PER_PAGE
            || inventory_after.is_some_and(|item_id| {
                item_id.counter() == 0 || item_id.world_namespace() != actor_id.world_namespace()
            })
        {
            return Err(StoreError::InvalidRecord);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        let actor_bytes = actor_id.as_u128().to_be_bytes();
        let (account_bytes, name): (Vec<u8>, String) = transaction
            .query_row(
                "SELECT account_id, name FROM characters WHERE actor_id = ?1",
                [actor_bytes.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?
            .ok_or(StoreError::CharacterUnavailable)?;
        let account_raw = u128::from_be_bytes(decode_array(&account_bytes)?);
        let account_id = AccountId::new((account_raw >> 64) as u64, account_raw as u64);
        if account_id.world_namespace() != actor_id.world_namespace() {
            return Err(StoreError::CorruptRecord);
        }
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::InspectPrivateCharacter {
                    actor_id,
                    inventory_after,
                    inventory_limit,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AdminCharacterIdentity {
            account_id,
            actor_id,
            name,
        })
    }

    pub fn admin_create_account(
        &mut self,
        actor_endpoint: EndpointIdentity,
        display_name: &str,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AdminAccountCreation, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::AdminCreateAccount {
            account_id: None,
            role,
            endpoint,
        };
        let result = self.admin_create_account_allowed(
            actor_endpoint,
            display_name,
            role,
            endpoint,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    pub fn audit_invalid_admin_account_create(
        &mut self,
        actor_endpoint: EndpointIdentity,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        self.append_security_audit_attempt(
            now_utc_seconds,
            actor,
            SecurityAuditActionV1::AdminCreateAccount {
                account_id: None,
                role,
                endpoint,
            },
            SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::InvalidRequest),
        )?;
        Ok(())
    }

    fn admin_create_account_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        display_name: &str,
        role: AccountRole,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<AdminAccountCreation, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        validate_display_name(display_name)?;
        let expires = now_utc_seconds
            .checked_add(ENROLLMENT_LIFETIME_SECONDS)
            .ok_or(StoreError::NumericOverflow)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        ensure_endpoint_is_new(&transaction, endpoint)?;
        let account_id = reserve_account_id_in_transaction(&transaction)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        transaction
            .execute(
                "INSERT INTO accounts(account_id, display_name, role, status)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    account_bytes.as_slice(),
                    display_name,
                    encode_role(role),
                    encode_status(AccountStatus::InitialEnrollment),
                ],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT INTO endpoint_bindings(endpoint_id, account_id, state, pending_expires_utc)
                 VALUES (?1, ?2, 0, ?3)",
                params![endpoint.0.as_slice(), account_bytes.as_slice(), expires],
            )
            .map_err(StoreError::Sqlite)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::AdminCreateAccount {
                    account_id: Some(account_id),
                    role,
                    endpoint,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AdminAccountCreation {
            account: AccountRecord {
                id: account_id,
                display_name: display_name.to_owned(),
                role,
                status: AccountStatus::InitialEnrollment,
                suspended_until_utc: None,
                muted_until_utc: None,
            },
            pending_endpoint: EndpointBindingSummary {
                endpoint,
                state: EndpointBindingState::Pending,
                pending_expires_utc: Some(expires),
            },
        })
    }

    pub fn admin_endpoint_bindings(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<Vec<EndpointBindingSummary>, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::AdminListEndpoints { account_id };
        let result =
            self.admin_endpoint_bindings_allowed(actor_endpoint, account_id, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_endpoint_bindings_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<Vec<EndpointBindingSummary>, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        let bindings = query_endpoint_bindings(&transaction, account_id)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::AdminListEndpoints { account_id },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(bindings)
    }

    pub fn admin_add_pending_endpoint(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::AdminAddEndpoint {
            account_id,
            endpoint,
        };
        let result = self.admin_add_pending_endpoint_allowed(
            actor_endpoint,
            account_id,
            endpoint,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    pub fn audit_invalid_admin_endpoint_add(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        self.append_security_audit_attempt(
            now_utc_seconds,
            actor,
            SecurityAuditActionV1::AdminAddEndpoint {
                account_id,
                endpoint,
            },
            SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::InvalidRequest),
        )?;
        Ok(())
    }

    fn admin_add_pending_endpoint_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let expires = now_utc_seconds
            .checked_add(ENROLLMENT_LIFETIME_SECONDS)
            .ok_or(StoreError::NumericOverflow)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        let target = query_account(&transaction, &account_bytes)?;
        if !matches!(
            target.status,
            AccountStatus::InitialEnrollment | AccountStatus::Enabled
        ) {
            return Err(StoreError::AccountUnavailable);
        }
        ensure_endpoint_is_new(&transaction, endpoint)?;
        ensure_binding_capacity(&transaction, &account_bytes)?;
        transaction
            .execute(
                "INSERT INTO endpoint_bindings(endpoint_id, account_id, state, pending_expires_utc)
                 VALUES (?1, ?2, 0, ?3)",
                params![endpoint.0.as_slice(), account_bytes.as_slice(), expires],
            )
            .map_err(StoreError::Sqlite)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::AdminAddEndpoint {
                    account_id,
                    endpoint,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(EndpointBindingSummary {
            endpoint,
            state: EndpointBindingState::Pending,
            pending_expires_utc: Some(expires),
        })
    }

    pub fn admin_revoke_endpoint(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::AdminRevokeEndpoint {
            account_id,
            endpoint,
        };
        let result = self.admin_revoke_endpoint_allowed(
            actor_endpoint,
            account_id,
            endpoint,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_revoke_endpoint_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        query_account(&transaction, &account_bytes)?;
        let state: Option<i64> = transaction
            .query_row(
                "SELECT state FROM endpoint_bindings
                 WHERE endpoint_id = ?1 AND account_id = ?2",
                params![endpoint.0.as_slice(), account_bytes.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        match state.map(decode_endpoint_state).transpose()? {
            Some(EndpointBindingState::Pending) => {}
            Some(EndpointBindingState::Active) => {
                let active: i64 = transaction
                    .query_row(
                        "SELECT COUNT(*) FROM endpoint_bindings
                         WHERE account_id = ?1 AND state = 1",
                        [account_bytes.as_slice()],
                        |row| row.get(0),
                    )
                    .map_err(StoreError::Sqlite)?;
                if active <= 1 {
                    return Err(StoreError::CannotRevokeLastEndpoint);
                }
            }
            Some(EndpointBindingState::Revoked) | None => {
                return Err(StoreError::EndpointNotRevocable);
            }
        }
        let changed = transaction
            .execute(
                "UPDATE endpoint_bindings
                 SET state = 2, pending_expires_utc = NULL
                 WHERE endpoint_id = ?1 AND account_id = ?2 AND state IN (0, 1)",
                params![endpoint.0.as_slice(), account_bytes.as_slice()],
            )
            .map_err(StoreError::Sqlite)?;
        if changed != 1 {
            return Err(StoreError::EndpointNotRevocable);
        }
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::AdminRevokeEndpoint {
                    account_id,
                    endpoint,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the durable report boundary keeps authenticated subjects and targets explicit"
    )]
    pub fn submit_report(
        &mut self,
        reporter_account: AccountId,
        reporter_endpoint: EndpointIdentity,
        reporter_actor: ActorId,
        target_actor: ActorId,
        reason: ReportReason,
        details: &str,
        now_utc_seconds: i64,
    ) -> Result<ReportId, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, reporter_endpoint)?;
        let action = SecurityAuditActionV1::SubmitReport {
            report_id: None,
            target_actor,
            reason,
        };
        let result = self.submit_report_allowed(
            reporter_account,
            reporter_endpoint,
            reporter_actor,
            target_actor,
            reason,
            details,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the durable report records every authenticated subject and target explicitly"
    )]
    fn submit_report_allowed(
        &mut self,
        reporter_account: AccountId,
        reporter_endpoint: EndpointIdentity,
        reporter_actor: ActorId,
        target_actor: ActorId,
        reason: ReportReason,
        details: &str,
        now_utc_seconds: i64,
    ) -> Result<ReportId, StoreError> {
        if now_utc_seconds <= 0 || !valid_report_details(details) {
            return Err(StoreError::InvalidReport);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let reporter =
            require_authenticated_account(&transaction, reporter_endpoint, now_utc_seconds)?;
        if reporter.id != reporter_account {
            return Err(StoreError::UnauthorizedEndpoint);
        }
        let reporter_actor_bytes = reporter_actor.as_u128().to_be_bytes();
        let reporter_character: Option<(Vec<u8>, String)> = transaction
            .query_row(
                "SELECT account_id, name FROM characters WHERE actor_id = ?1",
                [reporter_actor_bytes.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let Some((reporter_owner_bytes, reporter_character)) = reporter_character else {
            return Err(StoreError::CharacterUnavailable);
        };
        if reporter_owner_bytes.as_slice() != reporter_account.as_u128().to_be_bytes().as_slice() {
            return Err(StoreError::UnauthorizedEndpoint);
        }
        let report_window_start = now_utc_seconds
            .checked_sub(REPORT_RATE_WINDOW_SECONDS)
            .ok_or(StoreError::ClockRegression)?;
        let recent_reports: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM player_reports
                 WHERE reporter_account_id = ?1 AND created_utc > ?2",
                params![reporter_owner_bytes.as_slice(), report_window_start],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        if recent_reports >= i64::from(MAX_REPORTS_PER_ACCOUNT_PER_HOUR) {
            return Err(StoreError::ReportRateLimited);
        }
        let target_actor_bytes = target_actor.as_u128().to_be_bytes();
        let target_character: Option<(Vec<u8>, String)> = transaction
            .query_row(
                "SELECT account_id, name FROM characters WHERE actor_id = ?1",
                [target_actor_bytes.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let Some((target_owner_bytes, target_character)) = target_character else {
            return Err(StoreError::CharacterUnavailable);
        };
        if target_owner_bytes == reporter_owner_bytes {
            return Err(StoreError::CannotReportSelf);
        }
        let changed = transaction
            .execute(
                "INSERT INTO player_reports(
                    created_utc, reporter_account_id, reporter_actor_id,
                    reporter_character, target_account_id, target_actor_id,
                    target_character, reason, details
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    now_utc_seconds,
                    reporter_owner_bytes,
                    reporter_actor_bytes.as_slice(),
                    reporter_character,
                    target_owner_bytes,
                    target_actor_bytes.as_slice(),
                    target_character,
                    encode_report_reason(reason),
                    details,
                ],
            )
            .map_err(StoreError::Sqlite)?;
        if changed != 1 {
            return Err(StoreError::InvalidReport);
        }
        let report_id = ReportId(
            u64::try_from(transaction.last_insert_rowid())
                .map_err(|_| StoreError::NumericOverflow)?,
        );
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: reporter.id,
                    endpoint: reporter_endpoint,
                    role: reporter.role,
                },
                action: SecurityAuditActionV1::SubmitReport {
                    report_id: Some(report_id),
                    target_actor,
                    reason,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(report_id)
    }

    pub fn admin_reports(
        &mut self,
        actor_endpoint: EndpointIdentity,
        state: Option<ReportState>,
        after: Option<ReportId>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<ReportPage, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::ListReports {
            state,
            after,
            limit,
        };
        let result =
            self.admin_reports_allowed(actor_endpoint, state, after, limit, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_reports_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        state: Option<ReportState>,
        after: Option<ReportId>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<ReportPage, StoreError> {
        if now_utc_seconds <= 0
            || limit == 0
            || limit > MAX_REPORTS_PER_PAGE
            || after.is_some_and(|report_id| report_id.0 == 0)
        {
            return Err(StoreError::InvalidReport);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let after = after
            .map(|report_id| i64::try_from(report_id.0).map_err(|_| StoreError::NumericOverflow))
            .transpose()?;
        let row_limit = i64::from(limit) + 1;
        let mut reports = query_reports_page(&transaction, state, after, row_limit)?;
        let has_more = reports.len() > usize::from(limit);
        reports.truncate(usize::from(limit));
        let next_after = has_more.then(|| {
            reports
                .last()
                .expect("an overflowing report page returns at least one row")
                .report_id
        });
        let after = after
            .map(|report_id| u64::try_from(report_id).map(ReportId))
            .transpose()
            .map_err(|_| StoreError::CorruptRecord)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::ListReports {
                    state,
                    after,
                    limit,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(ReportPage {
            reports,
            next_after,
        })
    }

    pub fn set_report_state(
        &mut self,
        actor_endpoint: EndpointIdentity,
        report_id: ReportId,
        state: ReportState,
        now_utc_seconds: i64,
    ) -> Result<ReportSummary, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::SetReportState { report_id, state };
        let result =
            self.set_report_state_allowed(actor_endpoint, report_id, state, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn set_report_state_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        report_id: ReportId,
        state: ReportState,
        now_utc_seconds: i64,
    ) -> Result<ReportSummary, StoreError> {
        if now_utc_seconds <= 0 || report_id.0 == 0 || state == ReportState::Open {
            return Err(StoreError::InvalidReport);
        }
        let report_id_i64 = i64::try_from(report_id.0).map_err(|_| StoreError::NumericOverflow)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let mut report = query_report(&transaction, report_id)?;
        if report.state != ReportState::Open {
            return Err(StoreError::InvalidReport);
        }
        let security_audit_sequence = insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::SetReportState { report_id, state },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        let security_audit_sequence_i64 =
            i64::try_from(security_audit_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let operator_bytes = operator.id.as_u128().to_be_bytes();
        let changed = transaction
            .execute(
                "UPDATE player_reports
                 SET state = ?1, resolved_utc = ?2, resolved_by_account_id = ?3,
                     resolution_audit_sequence = ?4
                 WHERE report_id = ?5 AND state = 0",
                params![
                    encode_report_state(state),
                    now_utc_seconds,
                    operator_bytes.as_slice(),
                    security_audit_sequence_i64,
                    report_id_i64,
                ],
            )
            .map_err(StoreError::Sqlite)?;
        if changed != 1 {
            return Err(StoreError::InvalidReport);
        }
        report.state = state;
        report.resolved_utc = Some(now_utc_seconds);
        report.resolved_by_account = Some(operator.id);
        report.resolution_audit_sequence = Some(security_audit_sequence);
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(report)
    }

    pub fn admin_moderation_history(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        after: Option<u64>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<ModerationHistoryPage, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::ListModerationHistory {
            account_id,
            after,
            limit,
        };
        let result = self.admin_moderation_history_allowed(
            actor_endpoint,
            account_id,
            after,
            limit,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn admin_moderation_history_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        after: Option<u64>,
        limit: u16,
        now_utc_seconds: i64,
    ) -> Result<ModerationHistoryPage, StoreError> {
        if now_utc_seconds <= 0
            || account_id.counter() == 0
            || after == Some(0)
            || limit == 0
            || limit > MAX_MODERATION_HISTORY_PER_PAGE
        {
            return Err(StoreError::InvalidReport);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        query_account(&transaction, &account_bytes)?;
        let after_i64 = after
            .map(|value| i64::try_from(value).map_err(|_| StoreError::NumericOverflow))
            .transpose()?;
        let row_limit = i64::from(limit) + 1;
        let mut statement = transaction
            .prepare(
                "SELECT history_id, security_audit_sequence, occurred_utc,
                        operator_account_id, target_account_id, kind, until_utc
                 FROM moderation_history
                 WHERE target_account_id = ?1 AND (?2 IS NULL OR history_id > ?2)
                 ORDER BY history_id ASC LIMIT ?3",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = statement
            .query_map(
                params![account_bytes.as_slice(), after_i64, row_limit],
                decode_moderation_history_row,
            )
            .map_err(StoreError::Sqlite)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(StoreError::Sqlite)??);
        }
        drop(statement);
        let has_more = entries.len() > usize::from(limit);
        entries.truncate(usize::from(limit));
        let next_after = has_more.then(|| {
            entries
                .last()
                .expect("an overflowing moderation page returns at least one row")
                .history_id
        });
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::ListModerationHistory {
                    account_id,
                    after,
                    limit,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(ModerationHistoryPage {
            entries,
            next_after,
        })
    }

    pub fn set_account_role(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        role: AccountRole,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::SetAccountRole { account_id, role };
        let result =
            self.set_account_role_allowed(actor_endpoint, account_id, role, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn set_account_role_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        role: AccountRole,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        if administrator.id == account_id {
            return Err(StoreError::CannotTargetSelf);
        }
        let account_bytes = account_id.as_u128().to_be_bytes();
        let mut target = query_account(&transaction, &account_bytes)?;
        if !matches!(
            target.status,
            AccountStatus::Enabled | AccountStatus::Disabled
        ) {
            return Err(StoreError::InvalidAccountTransition);
        }
        let changed = target.role != role;
        if changed
            && target.role == AccountRole::Administrator
            && target.status == AccountStatus::Enabled
        {
            ensure_another_enabled_administrator(&transaction, account_id, now_utc_seconds)?;
        }
        if changed {
            transaction
                .execute(
                    "UPDATE accounts SET role = ?1 WHERE account_id = ?2",
                    params![encode_role(role), account_bytes.as_slice()],
                )
                .map_err(StoreError::Sqlite)?;
            target.role = role;
        }
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::SetAccountRole { account_id, role },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountMutation {
            account: target,
            changed,
        })
    }

    pub fn set_account_status(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        status: AccountStatus,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::SetAccountStatus { account_id, status };
        let result =
            self.set_account_status_allowed(actor_endpoint, account_id, status, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn set_account_status_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        status: AccountStatus,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        if !matches!(
            status,
            AccountStatus::Enabled | AccountStatus::Disabled | AccountStatus::Banned
        ) {
            return Err(StoreError::InvalidAccountTransition);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        if administrator.id == account_id {
            return Err(StoreError::CannotTargetSelf);
        }
        let account_bytes = account_id.as_u128().to_be_bytes();
        let mut target = query_account(&transaction, &account_bytes)?;
        let transition_allowed = matches!(
            (target.status, status),
            (AccountStatus::Enabled | AccountStatus::Disabled, _)
                | (AccountStatus::Banned, AccountStatus::Banned)
        );
        if !transition_allowed {
            return Err(StoreError::InvalidAccountTransition);
        }
        let changed = target.status != status;
        if changed
            && target.role == AccountRole::Administrator
            && target.status == AccountStatus::Enabled
            && status != AccountStatus::Enabled
        {
            ensure_another_enabled_administrator(&transaction, account_id, now_utc_seconds)?;
        }
        if changed && status == AccountStatus::Enabled {
            let active: i64 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM endpoint_bindings
                     WHERE account_id = ?1 AND state = 1",
                    [account_bytes.as_slice()],
                    |row| row.get(0),
                )
                .map_err(StoreError::Sqlite)?;
            if active == 0 {
                return Err(StoreError::InvalidAccountTransition);
            }
        }
        if changed {
            transaction
                .execute(
                    "UPDATE accounts SET status = ?1 WHERE account_id = ?2",
                    params![encode_status(status), account_bytes.as_slice()],
                )
                .map_err(StoreError::Sqlite)?;
            target.status = status;
        }
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::SetAccountStatus { account_id, status },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountMutation {
            account: target,
            changed,
        })
    }

    pub fn kick_account(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::KickAccount { account_id };
        let result = self.kick_account_allowed(actor_endpoint, account_id, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn kick_account_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        now_utc_seconds: i64,
    ) -> Result<AccountRecord, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        let target = query_account(&transaction, &account_bytes)?;
        ensure_moderation_target(&operator, &target)?;
        if target.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        let security_audit_sequence = insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::KickAccount { account_id },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        insert_moderation_history(
            &transaction,
            security_audit_sequence,
            now_utc_seconds,
            operator.id,
            target.id,
            ModerationKind::Kick,
            None,
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(target)
    }

    pub fn set_account_suspension(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::SetSuspension {
            account_id,
            duration_seconds,
        };
        let result = self.set_account_suspension_allowed(
            actor_endpoint,
            account_id,
            duration_seconds,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn set_account_suspension_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        let until = moderation_until(duration_seconds, now_utc_seconds)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        let mut target = query_account(&transaction, &account_bytes)?;
        ensure_moderation_target(&operator, &target)?;
        if target.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        if target.role == AccountRole::Administrator && until.is_some() {
            ensure_another_enabled_administrator(&transaction, account_id, now_utc_seconds)?;
        }
        let changed = target.suspended_until_utc != until;
        if changed {
            transaction
                .execute(
                    "UPDATE accounts SET suspended_until_utc = ?1 WHERE account_id = ?2",
                    params![until, account_bytes.as_slice()],
                )
                .map_err(StoreError::Sqlite)?;
            target.suspended_until_utc = until;
        }
        let security_audit_sequence = insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::SetSuspension {
                    account_id,
                    duration_seconds,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        insert_moderation_history(
            &transaction,
            security_audit_sequence,
            now_utc_seconds,
            operator.id,
            target.id,
            ModerationKind::Suspension,
            until,
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountMutation {
            account: target,
            changed,
        })
    }

    pub fn set_account_mute(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::SetMute {
            account_id,
            duration_seconds,
        };
        let result = self.set_account_mute_allowed(
            actor_endpoint,
            account_id,
            duration_seconds,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn set_account_mute_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        account_id: AccountId,
        duration_seconds: Option<u32>,
        now_utc_seconds: i64,
    ) -> Result<AccountMutation, StoreError> {
        let until = moderation_until(duration_seconds, now_utc_seconds)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let operator = require_management_operator(&transaction, actor_endpoint, now_utc_seconds)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        let mut target = query_account(&transaction, &account_bytes)?;
        ensure_moderation_target(&operator, &target)?;
        if target.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        let changed = target.muted_until_utc != until;
        if changed {
            transaction
                .execute(
                    "UPDATE accounts SET muted_until_utc = ?1 WHERE account_id = ?2",
                    params![until, account_bytes.as_slice()],
                )
                .map_err(StoreError::Sqlite)?;
            target.muted_until_utc = until;
        }
        let security_audit_sequence = insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: operator.id,
                    endpoint: actor_endpoint,
                    role: operator.role,
                },
                action: SecurityAuditActionV1::SetMute {
                    account_id,
                    duration_seconds,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        insert_moderation_history(
            &transaction,
            security_audit_sequence,
            now_utc_seconds,
            operator.id,
            target.id,
            ModerationKind::Mute,
            until,
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(AccountMutation {
            account: target,
            changed,
        })
    }

    pub fn transfer_character(
        &mut self,
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        new_owner: AccountId,
        now_utc_seconds: i64,
    ) -> Result<CharacterTransfer, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::TransferCharacter {
            actor_id,
            new_owner,
        };
        let result =
            self.transfer_character_allowed(actor_endpoint, actor_id, new_owner, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn transfer_character_allowed(
        &mut self,
        actor_endpoint: EndpointIdentity,
        actor_id: ActorId,
        new_owner: AccountId,
        now_utc_seconds: i64,
    ) -> Result<CharacterTransfer, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let administrator = require_administrator(&transaction, actor_endpoint, now_utc_seconds)?;
        let new_owner_bytes = new_owner.as_u128().to_be_bytes();
        let destination = query_account(&transaction, &new_owner_bytes)?;
        if destination.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        let actor_bytes = actor_id.as_u128().to_be_bytes();
        let character: Option<(Vec<u8>, String)> = transaction
            .query_row(
                "SELECT account_id, name FROM characters WHERE actor_id = ?1",
                [actor_bytes.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let Some((previous_owner_bytes, character_name)) = character else {
            return Err(StoreError::CharacterUnavailable);
        };
        let previous_raw = u128::from_be_bytes(decode_array(&previous_owner_bytes)?);
        let previous_owner = AccountId::new((previous_raw >> 64) as u64, previous_raw as u64);
        if previous_owner == new_owner {
            return Err(StoreError::InvalidAccountTransition);
        }
        let destination_character_count: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM characters WHERE account_id = ?1",
                [new_owner_bytes.as_slice()],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        if destination_character_count
            >= i64::try_from(MAX_CHARACTERS_PER_ACCOUNT).map_err(|_| StoreError::NumericOverflow)?
        {
            return Err(StoreError::TooManyCharacters);
        }
        let name_conflict = transaction
            .query_row(
                "SELECT 1 FROM characters WHERE account_id = ?1 AND name = ?2",
                params![new_owner_bytes.as_slice(), character_name],
                |_| Ok(()),
            )
            .optional()
            .map_err(StoreError::Sqlite)?
            .is_some();
        if name_conflict {
            return Err(StoreError::CharacterNameConflict);
        }
        let changed = transaction
            .execute(
                "UPDATE characters SET account_id = ?1 WHERE actor_id = ?2",
                params![new_owner_bytes.as_slice(), actor_bytes.as_slice()],
            )
            .map_err(StoreError::Sqlite)?;
        if changed != 1 {
            return Err(StoreError::CharacterUnavailable);
        }
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick: current_persisted_tick(&transaction)?,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id: administrator.id,
                    endpoint: actor_endpoint,
                    role: administrator.role,
                },
                action: SecurityAuditActionV1::TransferCharacter {
                    actor_id,
                    new_owner,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(CharacterTransfer {
            actor_id,
            previous_owner,
            new_owner,
        })
    }

    pub fn authorize_chat(
        &self,
        account_id: AccountId,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let account = self.authorize_endpoint(endpoint, now_utc_seconds)?;
        if account.id != account_id {
            return Err(StoreError::UnauthorizedEndpoint);
        }
        if let Some(until) = account.muted_until_utc
            && until > now_utc_seconds
        {
            return Err(StoreError::AccountMuted(until));
        }
        Ok(())
    }

    pub fn endpoint_bindings(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<EndpointBindingSummary>, StoreError> {
        query_endpoint_bindings(&self.connection, account_id)
    }

    pub fn audited_endpoint_bindings(
        &mut self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<Vec<EndpointBindingSummary>, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::ListEndpoints { account_id };
        let account_bytes = account_id.as_u128().to_be_bytes();
        let result = (|| {
            let account = query_account(&self.connection, &account_bytes)?;
            if account.status != AccountStatus::Enabled {
                return Err(StoreError::AccountUnavailable);
            }
            ensure_active_account_endpoint(&self.connection, &account_bytes, actor_endpoint)?;
            self.endpoint_bindings(account_id)
        })();
        match result {
            Ok(bindings) => {
                self.append_security_audit_attempt(
                    now_utc_seconds,
                    actor,
                    action,
                    SecurityAuditOutcomeV1::Allowed,
                )?;
                Ok(bindings)
            }
            Err(error) => {
                self.audit_rejected_security_result(now_utc_seconds, actor, action, Err(error))
            }
        }
    }

    pub fn add_pending_endpoint(
        &mut self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::AddEndpoint {
            account_id,
            endpoint,
        };
        let result = self.add_pending_endpoint_allowed(
            account_id,
            actor_endpoint,
            endpoint,
            now_utc_seconds,
        );
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    pub fn audit_invalid_endpoint_add(
        &mut self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        self.append_security_audit_attempt(
            now_utc_seconds,
            actor,
            SecurityAuditActionV1::AddEndpoint {
                account_id,
                endpoint,
            },
            SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::InvalidRequest),
        )?;
        Ok(())
    }

    fn add_pending_endpoint_allowed(
        &mut self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let expires = now_utc_seconds
            .checked_add(ENROLLMENT_LIFETIME_SECONDS)
            .ok_or(StoreError::NumericOverflow)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let account = query_account(&transaction, &account_bytes)?;
        if account.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        ensure_active_account_endpoint(&transaction, &account_bytes, actor_endpoint)?;
        ensure_endpoint_is_new(&transaction, endpoint)?;
        ensure_binding_capacity(&transaction, &account_bytes)?;
        transaction
            .execute(
                "INSERT INTO endpoint_bindings(endpoint_id, account_id, state, pending_expires_utc)
                 VALUES (?1, ?2, 0, ?3)",
                params![endpoint.0.as_slice(), account_bytes.as_slice(), expires],
            )
            .map_err(StoreError::Sqlite)?;
        let observed_tick = current_persisted_tick(&transaction)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id,
                    endpoint: actor_endpoint,
                    role: account.role,
                },
                action: SecurityAuditActionV1::AddEndpoint {
                    account_id,
                    endpoint,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(EndpointBindingSummary {
            endpoint,
            state: EndpointBindingState::Pending,
            pending_expires_utc: Some(expires),
        })
    }

    pub fn revoke_endpoint(
        &mut self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        let actor = security_actor_for_endpoint(&self.connection, actor_endpoint)?;
        let action = SecurityAuditActionV1::RevokeEndpoint {
            account_id,
            endpoint,
        };
        let result =
            self.revoke_endpoint_allowed(account_id, actor_endpoint, endpoint, now_utc_seconds);
        self.audit_rejected_security_result(now_utc_seconds, actor, action, result)
    }

    fn revoke_endpoint_allowed(
        &mut self,
        account_id: AccountId,
        actor_endpoint: EndpointIdentity,
        endpoint: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<(), StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let account_bytes = account_id.as_u128().to_be_bytes();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let account = query_account(&transaction, &account_bytes)?;
        if account.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        ensure_active_account_endpoint(&transaction, &account_bytes, actor_endpoint)?;
        let state: Option<i64> = transaction
            .query_row(
                "SELECT state FROM endpoint_bindings
                 WHERE endpoint_id = ?1 AND account_id = ?2",
                params![endpoint.0.as_slice(), account_bytes.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        let state = state.map(decode_endpoint_state).transpose()?;
        match state {
            Some(EndpointBindingState::Pending) => {}
            Some(EndpointBindingState::Active) => {
                let active: i64 = transaction
                    .query_row(
                        "SELECT COUNT(*) FROM endpoint_bindings
                         WHERE account_id = ?1 AND state = 1",
                        [account_bytes.as_slice()],
                        |row| row.get(0),
                    )
                    .map_err(StoreError::Sqlite)?;
                if active <= 1 {
                    return Err(StoreError::CannotRevokeLastEndpoint);
                }
            }
            Some(EndpointBindingState::Revoked) | None => {
                return Err(StoreError::EndpointNotRevocable);
            }
        }
        transaction
            .execute(
                "UPDATE endpoint_bindings
                 SET state = 2, pending_expires_utc = NULL
                 WHERE endpoint_id = ?1 AND account_id = ?2 AND state IN (0, 1)",
                params![endpoint.0.as_slice(), account_bytes.as_slice()],
            )
            .map_err(StoreError::Sqlite)?;
        let observed_tick = current_persisted_tick(&transaction)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick,
                actor: SecurityAuditActorV1::AuthenticatedAccount {
                    account_id,
                    endpoint: actor_endpoint,
                    role: account.role,
                },
                action: SecurityAuditActionV1::RevokeEndpoint {
                    account_id,
                    endpoint,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)
    }

    pub fn recover_account_endpoint(
        &mut self,
        account_id: AccountId,
        replacement: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        let action = SecurityAuditActionV1::RecoverEndpoint {
            account_id,
            replacement,
        };
        let result =
            self.recover_account_endpoint_allowed(account_id, replacement, now_utc_seconds);
        self.audit_rejected_security_result(
            now_utc_seconds,
            SecurityAuditActorV1::LocalOperator,
            action,
            result,
        )
    }

    fn recover_account_endpoint_allowed(
        &mut self,
        account_id: AccountId,
        replacement: EndpointIdentity,
        now_utc_seconds: i64,
    ) -> Result<EndpointBindingSummary, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::ClockRegression);
        }
        let expires = now_utc_seconds
            .checked_add(ENROLLMENT_LIFETIME_SECONDS)
            .ok_or(StoreError::NumericOverflow)?;
        let account_bytes = account_id.as_u128().to_be_bytes();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let account = query_account(&transaction, &account_bytes)?;
        if account.status == AccountStatus::Banned {
            return Err(StoreError::AccountUnavailable);
        }
        ensure_endpoint_is_new(&transaction, replacement)?;
        ensure_binding_capacity(&transaction, &account_bytes)?;
        transaction
            .execute(
                "UPDATE endpoint_bindings SET state = 2, pending_expires_utc = NULL
                 WHERE account_id = ?1 AND state IN (0, 1)",
                [account_bytes.as_slice()],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "UPDATE accounts SET status = ?1 WHERE account_id = ?2",
                params![
                    encode_status(AccountStatus::RecoveryLocked),
                    account_bytes.as_slice()
                ],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT INTO endpoint_bindings(endpoint_id, account_id, state, pending_expires_utc)
                 VALUES (?1, ?2, 0, ?3)",
                params![replacement.0.as_slice(), account_bytes.as_slice(), expires],
            )
            .map_err(StoreError::Sqlite)?;
        let observed_tick = current_persisted_tick(&transaction)?;
        insert_security_audit(
            &transaction,
            &SecurityAuditRecordV1 {
                format_version: SECURITY_AUDIT_FORMAT_VERSION,
                occurred_utc_seconds: now_utc_seconds,
                observed_tick,
                actor: SecurityAuditActorV1::LocalOperator,
                action: SecurityAuditActionV1::RecoverEndpoint {
                    account_id,
                    replacement,
                },
                outcome: SecurityAuditOutcomeV1::Allowed,
            },
        )?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(EndpointBindingSummary {
            endpoint: replacement,
            state: EndpointBindingState::Pending,
            pending_expires_utc: Some(expires),
        })
    }

    pub fn create_character(
        &mut self,
        account_id: AccountId,
        name: &str,
        created_tick: SimTick,
        created_after_journal_sequence: u64,
        actor: &ActorSnapshot,
    ) -> Result<CharacterSummary, StoreError> {
        validate_character_name(name)?;
        let actor_id = actor.id;
        if account_id.world_namespace() != actor_id.world_namespace()
            || account_id.world_namespace() != self.metadata()?.world_namespace
            || actor_id.counter() == 0
        {
            return Err(StoreError::InvalidStableId);
        }
        let account_bytes = account_id.as_u128().to_be_bytes();
        let actor_bytes = actor_id.as_u128().to_be_bytes();
        let spawn_journal_sequence = i64::try_from(created_after_journal_sequence)
            .map_err(|_| StoreError::NumericOverflow)?;
        let spawn_state = postcard::to_stdvec(&CharacterSpawnV1 {
            created_tick,
            created_after_journal_sequence,
            actor: actor.clone(),
        })
        .map_err(StoreError::Postcard)?;
        if spawn_state.len() > MAX_CHARACTER_SPAWN_DECODED {
            return Err(StoreError::InvalidRecord);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let account = query_account(&transaction, &account_bytes)?;
        if account.status != AccountStatus::Enabled {
            return Err(StoreError::AccountUnavailable);
        }
        let latest_journal_sequence: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) FROM journal_batches",
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        if latest_journal_sequence != spawn_journal_sequence {
            return Err(StoreError::InvalidRecord);
        }
        let count: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM characters WHERE account_id = ?1",
                [account_bytes.as_slice()],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        if count
            >= i64::try_from(MAX_CHARACTERS_PER_ACCOUNT).map_err(|_| StoreError::NumericOverflow)?
        {
            return Err(StoreError::TooManyCharacters);
        }
        let changed = transaction
            .execute(
                "INSERT OR IGNORE INTO characters(
                    actor_id, account_id, name, spawn_state, spawn_journal_sequence
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    actor_bytes.as_slice(),
                    account_bytes.as_slice(),
                    name,
                    spawn_state,
                    spawn_journal_sequence
                ],
            )
            .map_err(StoreError::Sqlite)?;
        if changed != 1 {
            return Err(StoreError::CharacterAlreadyExists);
        }
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(CharacterSummary {
            actor_id,
            name: name.to_owned(),
        })
    }

    pub fn characters_for_account(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<CharacterSummary>, StoreError> {
        query_characters(&self.connection, account_id)
    }

    pub fn account_owns_actor(
        &self,
        account_id: AccountId,
        actor_id: ActorId,
    ) -> Result<bool, StoreError> {
        let account_bytes = account_id.as_u128().to_be_bytes();
        let actor_bytes = actor_id.as_u128().to_be_bytes();
        self.connection
            .query_row(
                "SELECT 1 FROM characters WHERE account_id = ?1 AND actor_id = ?2",
                params![account_bytes.as_slice(), actor_bytes.as_slice()],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(StoreError::Sqlite)
    }

    pub fn security_audit_after(
        &self,
        sequence: u64,
    ) -> Result<Vec<(u64, SecurityAuditRecordV1)>, StoreError> {
        let sequence = i64::try_from(sequence).map_err(|_| StoreError::NumericOverflow)?;
        self.security_audit_between_sql(sequence, None)
    }

    fn security_audit_between(
        &self,
        start_sequence: u64,
        end_sequence: u64,
    ) -> Result<Vec<(u64, SecurityAuditRecordV1)>, StoreError> {
        let start = i64::try_from(start_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let end = i64::try_from(end_sequence).map_err(|_| StoreError::NumericOverflow)?;
        self.security_audit_between_sql(start, Some(end))
    }

    fn security_audit_between_sql(
        &self,
        start_sequence: i64,
        end_sequence: Option<i64>,
    ) -> Result<Vec<(u64, SecurityAuditRecordV1)>, StoreError> {
        let sql = if end_sequence.is_some() {
            "SELECT sequence, payload, record_hash FROM security_audit
             WHERE sequence > ?1 AND sequence <= ?2 ORDER BY sequence ASC"
        } else {
            "SELECT sequence, payload, record_hash FROM security_audit
             WHERE sequence > ?1 ORDER BY sequence ASC"
        };
        let mut statement = self.connection.prepare(sql).map_err(StoreError::Sqlite)?;
        let mut rows = if let Some(end_sequence) = end_sequence {
            statement
                .query(params![start_sequence, end_sequence])
                .map_err(StoreError::Sqlite)?
        } else {
            statement
                .query([start_sequence])
                .map_err(StoreError::Sqlite)?
        };
        let mut records = Vec::new();
        while let Some(row) = rows.next().map_err(StoreError::Sqlite)? {
            let raw_sequence: i64 = row.get(0).map_err(StoreError::Sqlite)?;
            let payload: Vec<u8> = row.get(1).map_err(StoreError::Sqlite)?;
            let expected_hash: Vec<u8> = row.get(2).map_err(StoreError::Sqlite)?;
            if payload.is_empty() || payload.len() > 65_536 {
                return Err(StoreError::CorruptRecord);
            }
            let record: SecurityAuditRecordV1 =
                postcard::from_bytes(&payload).map_err(StoreError::Postcard)?;
            if record.canonical_hash()? != decode_array::<32>(&expected_hash)? {
                return Err(StoreError::StateHashMismatch);
            }
            records.push((
                u64::try_from(raw_sequence).map_err(|_| StoreError::CorruptRecord)?,
                record,
            ));
        }
        let initial_sequence =
            u64::try_from(start_sequence).map_err(|_| StoreError::CorruptRecord)?;
        let final_sequence = if let Some(end_sequence) = end_sequence {
            u64::try_from(end_sequence).map_err(|_| StoreError::CorruptRecord)?
        } else {
            records
                .last()
                .map_or(initial_sequence, |(sequence, _)| *sequence)
        };
        validate_security_audit_range(initial_sequence, final_sequence, &records)?;
        Ok(records)
    }

    fn append_security_audit_attempt(
        &mut self,
        occurred_utc_seconds: i64,
        actor: SecurityAuditActorV1,
        action: SecurityAuditActionV1,
        outcome: SecurityAuditOutcomeV1,
    ) -> Result<u64, StoreError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let record = SecurityAuditRecordV1 {
            format_version: SECURITY_AUDIT_FORMAT_VERSION,
            occurred_utc_seconds,
            observed_tick: current_persisted_tick(&transaction)?,
            actor,
            action,
            outcome,
        };
        let sequence = insert_security_audit(&transaction, &record)?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(sequence)
    }

    fn audit_rejected_security_result<T>(
        &mut self,
        occurred_utc_seconds: i64,
        actor: SecurityAuditActorV1,
        action: SecurityAuditActionV1,
        result: Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                if occurred_utc_seconds > 0
                    && let Some(rejection) = security_audit_rejection(&error)
                {
                    self.append_security_audit_attempt(
                        occurred_utc_seconds,
                        actor,
                        action,
                        SecurityAuditOutcomeV1::Rejected(rejection),
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn append_journal_batch(&mut self, batch: &JournalBatchV1) -> Result<u64, StoreError> {
        self.append_journal_batch_internal(batch, None)
    }

    pub fn append_journal_batch_at(
        &mut self,
        batch: &JournalBatchV1,
        committed_utc_seconds: i64,
    ) -> Result<u64, StoreError> {
        self.append_journal_batch_internal(batch, Some(committed_utc_seconds))
    }

    fn append_journal_batch_internal(
        &mut self,
        batch: &JournalBatchV1,
        committed_utc_seconds: Option<i64>,
    ) -> Result<u64, StoreError> {
        batch.validate()?;
        let payload = postcard::to_stdvec(batch).map_err(StoreError::Postcard)?;
        let first_tick = batch.first_tick()?.0.to_be_bytes();
        let last_tick = batch.last_tick()?.0.to_be_bytes();
        let batch_hash = batch.canonical_hash()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "INSERT INTO journal_batches(first_tick, last_tick, payload, events_hash)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    first_tick.as_slice(),
                    last_tick.as_slice(),
                    payload,
                    batch_hash.as_slice()
                ],
            )
            .map_err(StoreError::Sqlite)?;
        let sequence = u64::try_from(transaction.last_insert_rowid())
            .map_err(|_| StoreError::NumericOverflow)?;
        if let Some(committed_utc_seconds) = committed_utc_seconds {
            validate_runtime_anchor(&transaction, committed_utc_seconds)?;
            transaction
                .execute(
                    "UPDATE world_metadata SET last_committed_utc = ?1 WHERE singleton = 1",
                    [committed_utc_seconds],
                )
                .map_err(StoreError::Sqlite)?;
        }
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(sequence)
    }

    pub fn journal_after(&self, sequence: u64) -> Result<Vec<(u64, JournalBatchV1)>, StoreError> {
        let sequence = i64::try_from(sequence).map_err(|_| StoreError::NumericOverflow)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT sequence, payload, events_hash FROM journal_batches
                 WHERE sequence > ?1 ORDER BY sequence ASC",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = statement
            .query_map([sequence], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(StoreError::Sqlite)?;
        let mut batches = Vec::new();
        for row in rows {
            let (sequence, payload, stored_hash) = row.map_err(StoreError::Sqlite)?;
            let batch: JournalBatchV1 =
                postcard::from_bytes(&payload).map_err(StoreError::Postcard)?;
            batch.validate()?;
            if decode_array::<32>(&stored_hash)? != batch.canonical_hash()? {
                return Err(StoreError::CorruptRecord);
            }
            batches.push((
                u64::try_from(sequence).map_err(|_| StoreError::CorruptRecord)?,
                batch,
            ));
        }
        Ok(batches)
    }

    fn journal_between(
        &self,
        first_exclusive: u64,
        last_inclusive: u64,
    ) -> Result<Vec<(u64, JournalBatchV1)>, StoreError> {
        let first = i64::try_from(first_exclusive).map_err(|_| StoreError::NumericOverflow)?;
        let last = i64::try_from(last_inclusive).map_err(|_| StoreError::NumericOverflow)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT sequence, payload, events_hash FROM journal_batches
                 WHERE sequence > ?1 AND sequence <= ?2 ORDER BY sequence ASC",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = statement
            .query_map(params![first, last], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(StoreError::Sqlite)?;
        let mut batches = Vec::new();
        for row in rows {
            let (sequence, payload, stored_hash) = row.map_err(StoreError::Sqlite)?;
            let batch: JournalBatchV1 =
                postcard::from_bytes(&payload).map_err(StoreError::Postcard)?;
            batch.validate()?;
            if decode_array::<32>(&stored_hash)? != batch.canonical_hash()? {
                return Err(StoreError::CorruptRecord);
            }
            batches.push((
                u64::try_from(sequence).map_err(|_| StoreError::CorruptRecord)?,
                batch,
            ));
        }
        Ok(batches)
    }

    pub fn replay_after(
        &self,
        journal_sequence: u64,
        world: WorldState,
    ) -> Result<(u64, WorldState), StoreError> {
        let batches = self.journal_after(journal_sequence)?;
        replay_parts(journal_sequence, world, &[], &batches)
    }

    pub fn recover_latest(
        &self,
        initial_world: WorldState,
    ) -> Result<(u64, WorldState), StoreError> {
        let (sequence, world) = self.latest_snapshot()?.unwrap_or((0, initial_world));
        let spawns = self.character_spawns()?;
        let batches = self.journal_after(sequence)?;
        replay_parts(sequence, world, &spawns, &batches)
    }

    pub fn export_replay(&self, content: ContentIdentity) -> Result<ReplayBundleV1, StoreError> {
        let (initial_journal_sequence, initial_world) = self
            .previous_snapshot()?
            .or(self.latest_snapshot()?)
            .ok_or(StoreError::MissingSnapshot)?;
        let initial_snapshot = initial_world.snapshot();
        let character_spawns = self.character_spawns()?;
        let journal_batches = self.journal_after(initial_journal_sequence)?;
        let security_audit_records = self.security_audit_after(0)?;
        let final_security_audit_sequence = security_audit_records
            .last()
            .map_or(0, |(sequence, _)| *sequence);
        let (_final_sequence, final_world) = replay_parts(
            initial_journal_sequence,
            initial_world,
            &character_spawns,
            &journal_batches,
        )?;
        let initial_snapshot_object_hash = SnapshotObjectV1::new(
            content.clone(),
            initial_journal_sequence,
            initial_snapshot.clone(),
        )?
        .canonical_hash()?;
        Ok(ReplayBundleV1 {
            format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: BASELINE_COMMIT.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            content,
            initial_journal_sequence,
            initial_snapshot,
            initial_snapshot_object_hash,
            character_spawns,
            journal_batches,
            initial_security_audit_sequence: 0,
            final_security_audit_sequence,
            security_audit_records,
            final_state_hash: final_world
                .canonical_hash()
                .map_err(StoreError::Simulation)?,
        })
    }

    pub fn initialize_replay_archive_cursor(
        &mut self,
        journal_sequence: u64,
        now_utc_seconds: i64,
    ) -> Result<ReplayArchiveCursor, StoreError> {
        if now_utc_seconds <= 0 || self.snapshot_at(journal_sequence)?.is_none() {
            return Err(StoreError::InvalidRecord);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let (stored_sequence, stored_security_sequence, stored_utc): (i64, i64, i64) = transaction
            .query_row(
                "SELECT replay_archive_sequence, replay_archive_security_sequence,
                        replay_archive_utc
                 FROM world_metadata WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
                other => StoreError::Sqlite(other),
            })?;
        let cursor = if stored_utc == 0 {
            let sequence =
                i64::try_from(journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
            let security_sequence: i64 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(sequence), 0) FROM security_audit",
                    [],
                    |row| row.get(0),
                )
                .map_err(StoreError::Sqlite)?;
            transaction
                .execute(
                    "UPDATE world_metadata
                     SET replay_archive_sequence = ?1,
                         replay_archive_security_sequence = ?2,
                         replay_archive_utc = ?3
                     WHERE singleton = 1",
                    params![sequence, security_sequence, now_utc_seconds],
                )
                .map_err(StoreError::Sqlite)?;
            ReplayArchiveCursor {
                journal_sequence,
                security_audit_sequence: u64::try_from(security_sequence)
                    .map_err(|_| StoreError::CorruptRecord)?,
                archived_utc_seconds: now_utc_seconds,
            }
        } else {
            if stored_sequence < 0 || stored_security_sequence < 0 || stored_utc < 0 {
                return Err(StoreError::CorruptRecord);
            }
            ReplayArchiveCursor {
                journal_sequence: u64::try_from(stored_sequence)
                    .map_err(|_| StoreError::CorruptRecord)?,
                security_audit_sequence: u64::try_from(stored_security_sequence)
                    .map_err(|_| StoreError::CorruptRecord)?,
                archived_utc_seconds: stored_utc,
            }
        };
        transaction
            .execute(
                "UPDATE world_metadata SET last_compacted_utc = ?1
                 WHERE singleton = 1 AND last_compacted_utc = 0",
                [now_utc_seconds],
            )
            .map_err(StoreError::Sqlite)?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        if self.snapshot_at(cursor.journal_sequence)?.is_none() {
            return Err(StoreError::MissingSnapshot);
        }
        Ok(cursor)
    }

    pub fn replay_archive_cursor(&self) -> Result<ReplayArchiveCursor, StoreError> {
        let (sequence, security_sequence, archived_utc_seconds): (i64, i64, i64) = self
            .connection
            .query_row(
                "SELECT replay_archive_sequence, replay_archive_security_sequence,
                        replay_archive_utc
                 FROM world_metadata WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
                other => StoreError::Sqlite(other),
            })?;
        if sequence < 0 || security_sequence < 0 || archived_utc_seconds <= 0 {
            return Err(StoreError::CorruptRecord);
        }
        Ok(ReplayArchiveCursor {
            journal_sequence: u64::try_from(sequence).map_err(|_| StoreError::CorruptRecord)?,
            security_audit_sequence: u64::try_from(security_sequence)
                .map_err(|_| StoreError::CorruptRecord)?,
            archived_utc_seconds,
        })
    }

    pub fn prepare_replay_archive(
        &mut self,
        end_journal_sequence: u64,
        now_utc_seconds: i64,
        content: ContentIdentity,
    ) -> Result<Option<PreparedReplayArchive>, StoreError> {
        let start = self.replay_archive_cursor()?;
        let (pending_sequence, pending_security_sequence, pending_utc): (i64, i64, i64) = self
            .connection
            .query_row(
                "SELECT replay_pending_sequence, replay_pending_security_sequence,
                        replay_pending_utc
                 FROM world_metadata WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
                other => StoreError::Sqlite(other),
            })?;
        let end = if pending_sequence == 0 && pending_security_sequence == 0 && pending_utc == 0 {
            let elapsed = now_utc_seconds
                .checked_sub(start.archived_utc_seconds)
                .ok_or(StoreError::ClockRegression)?;
            if elapsed < 0 {
                return Err(StoreError::ClockRegression);
            }
            if elapsed < REPLAY_ARCHIVE_INTERVAL_SECONDS {
                return Ok(None);
            }
            if end_journal_sequence <= start.journal_sequence {
                return Err(StoreError::InvalidRecord);
            }
            if self.snapshot_at(end_journal_sequence)?.is_none() {
                return Err(StoreError::MissingSnapshot);
            }
            let end_sequence =
                i64::try_from(end_journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
            let start_sequence =
                i64::try_from(start.journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
            let end_security_sequence: i64 = self
                .connection
                .query_row(
                    "SELECT COALESCE(MAX(sequence), 0) FROM security_audit",
                    [],
                    |row| row.get(0),
                )
                .map_err(StoreError::Sqlite)?;
            let start_security_sequence = i64::try_from(start.security_audit_sequence)
                .map_err(|_| StoreError::NumericOverflow)?;
            let changed = self
                .connection
                .execute(
                    "UPDATE world_metadata
                     SET replay_pending_sequence = ?1,
                         replay_pending_security_sequence = ?2,
                         replay_pending_utc = ?3
                     WHERE singleton = 1
                       AND replay_archive_sequence = ?4
                       AND replay_archive_security_sequence = ?5
                       AND replay_archive_utc = ?6
                       AND replay_pending_sequence = 0
                       AND replay_pending_security_sequence = 0
                       AND replay_pending_utc = 0",
                    params![
                        end_sequence,
                        end_security_sequence,
                        now_utc_seconds,
                        start_sequence,
                        start_security_sequence,
                        start.archived_utc_seconds
                    ],
                )
                .map_err(StoreError::Sqlite)?;
            if changed != 1 {
                return Err(StoreError::ReplayArchiveCursorChanged);
            }
            ReplayArchiveCursor {
                journal_sequence: end_journal_sequence,
                security_audit_sequence: u64::try_from(end_security_sequence)
                    .map_err(|_| StoreError::CorruptRecord)?,
                archived_utc_seconds: now_utc_seconds,
            }
        } else if pending_sequence > 0 && pending_security_sequence >= 0 && pending_utc > 0 {
            ReplayArchiveCursor {
                journal_sequence: u64::try_from(pending_sequence)
                    .map_err(|_| StoreError::CorruptRecord)?,
                security_audit_sequence: u64::try_from(pending_security_sequence)
                    .map_err(|_| StoreError::CorruptRecord)?,
                archived_utc_seconds: pending_utc,
            }
        } else {
            return Err(StoreError::CorruptRecord);
        };
        if end.journal_sequence <= start.journal_sequence
            || end.security_audit_sequence < start.security_audit_sequence
            || end.archived_utc_seconds < start.archived_utc_seconds
        {
            return Err(StoreError::CorruptRecord);
        }
        let bundle = self.export_replay_range(
            start.journal_sequence,
            end.journal_sequence,
            start.security_audit_sequence,
            end.security_audit_sequence,
            content,
        )?;
        Ok(Some(PreparedReplayArchive { start, end, bundle }))
    }

    pub fn commit_replay_archive(
        &mut self,
        start: ReplayArchiveCursor,
        end: ReplayArchiveCursor,
    ) -> Result<(), StoreError> {
        if end.journal_sequence <= start.journal_sequence
            || end.security_audit_sequence < start.security_audit_sequence
            || end.archived_utc_seconds < start.archived_utc_seconds
            || self.snapshot_at(end.journal_sequence)?.is_none()
        {
            return Err(StoreError::InvalidRecord);
        }
        let start_sequence =
            i64::try_from(start.journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let end_sequence =
            i64::try_from(end.journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let start_security_sequence = i64::try_from(start.security_audit_sequence)
            .map_err(|_| StoreError::NumericOverflow)?;
        let end_security_sequence =
            i64::try_from(end.security_audit_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let changed = self
            .connection
            .execute(
                "UPDATE world_metadata
                 SET replay_archive_sequence = ?1,
                     replay_archive_security_sequence = ?2,
                     replay_archive_utc = ?3,
                     replay_pending_sequence = 0,
                     replay_pending_security_sequence = 0,
                     replay_pending_utc = 0
                 WHERE singleton = 1
                   AND replay_archive_sequence = ?4
                   AND replay_archive_security_sequence = ?5
                   AND replay_archive_utc = ?6
                   AND replay_pending_sequence = ?1
                   AND replay_pending_security_sequence = ?2
                   AND replay_pending_utc = ?3",
                params![
                    end_sequence,
                    end_security_sequence,
                    end.archived_utc_seconds,
                    start_sequence,
                    start_security_sequence,
                    start.archived_utc_seconds
                ],
            )
            .map_err(StoreError::Sqlite)?;
        if changed != 1 {
            return Err(StoreError::ReplayArchiveCursorChanged);
        }
        Ok(())
    }

    pub fn compact_recovery_history(
        &mut self,
        now_utc_seconds: i64,
    ) -> Result<Option<RecoveryCompaction>, StoreError> {
        if now_utc_seconds <= 0 {
            return Err(StoreError::InvalidRecord);
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::Sqlite)?;
        let (
            cursor_sequence,
            cursor_security_sequence,
            cursor_utc,
            pending_sequence,
            pending_security_sequence,
            pending_utc,
            last_compacted_utc,
        ): (i64, i64, i64, i64, i64, i64, i64) = transaction
            .query_row(
                "SELECT replay_archive_sequence, replay_archive_security_sequence,
                        replay_archive_utc, replay_pending_sequence,
                        replay_pending_security_sequence, replay_pending_utc,
                        last_compacted_utc
                 FROM world_metadata WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
                other => StoreError::Sqlite(other),
            })?;
        if cursor_sequence < 0
            || cursor_security_sequence < 0
            || cursor_utc <= 0
            || pending_sequence != 0
            || pending_security_sequence != 0
            || pending_utc != 0
            || last_compacted_utc <= 0
        {
            return Err(StoreError::CorruptRecord);
        }
        let elapsed = now_utc_seconds
            .checked_sub(last_compacted_utc)
            .ok_or(StoreError::ClockRegression)?;
        if elapsed < 0 {
            return Err(StoreError::ClockRegression);
        }
        if elapsed < RECOVERY_COMPACTION_INTERVAL_SECONDS {
            return Ok(None);
        }
        let snapshot_exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM snapshots WHERE sequence = ?1)",
                [cursor_sequence],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        if !snapshot_exists {
            return Err(StoreError::MissingSnapshot);
        }
        let deleted_journal_batches = transaction
            .execute(
                "DELETE FROM journal_batches WHERE sequence <= ?1",
                [cursor_sequence],
            )
            .map_err(StoreError::Sqlite)?;
        let deleted_snapshots = transaction
            .execute(
                "DELETE FROM snapshots WHERE sequence < ?1",
                [cursor_sequence],
            )
            .map_err(StoreError::Sqlite)?;
        transaction
            .execute(
                "UPDATE world_metadata SET last_compacted_utc = ?1
                 WHERE singleton = 1 AND last_compacted_utc = ?2",
                params![now_utc_seconds, last_compacted_utc],
            )
            .map_err(StoreError::Sqlite)?;
        transaction.commit().map_err(StoreError::Sqlite)?;
        Ok(Some(RecoveryCompaction {
            through_journal_sequence: u64::try_from(cursor_sequence)
                .map_err(|_| StoreError::CorruptRecord)?,
            deleted_journal_batches,
            deleted_snapshots,
        }))
    }

    fn export_replay_range(
        &self,
        initial_journal_sequence: u64,
        final_journal_sequence: u64,
        initial_security_audit_sequence: u64,
        final_security_audit_sequence: u64,
        content: ContentIdentity,
    ) -> Result<ReplayBundleV1, StoreError> {
        let (_, initial_world) = self
            .snapshot_at(initial_journal_sequence)?
            .ok_or(StoreError::MissingSnapshot)?;
        let (_, final_world) = self
            .snapshot_at(final_journal_sequence)?
            .ok_or(StoreError::MissingSnapshot)?;
        let initial_snapshot = initial_world.snapshot();
        let final_tick = final_world.tick();
        let character_spawns = self
            .character_spawns()?
            .into_iter()
            .filter(|spawn| {
                spawn.created_after_journal_sequence <= final_journal_sequence
                    && spawn.created_tick <= final_tick
            })
            .collect();
        let journal_batches =
            self.journal_between(initial_journal_sequence, final_journal_sequence)?;
        let security_audit_records = self.security_audit_between(
            initial_security_audit_sequence,
            final_security_audit_sequence,
        )?;
        if journal_batches.last().map(|(sequence, _)| *sequence) != Some(final_journal_sequence) {
            return Err(StoreError::InvalidRecord);
        }
        let initial_snapshot_object_hash = SnapshotObjectV1::new(
            content.clone(),
            initial_journal_sequence,
            initial_snapshot.clone(),
        )?
        .canonical_hash()?;
        Ok(ReplayBundleV1 {
            format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: BASELINE_COMMIT.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            content,
            initial_journal_sequence,
            initial_snapshot,
            initial_snapshot_object_hash,
            character_spawns,
            journal_batches,
            initial_security_audit_sequence,
            final_security_audit_sequence,
            security_audit_records,
            final_state_hash: final_world
                .canonical_hash()
                .map_err(StoreError::Simulation)?,
        })
    }

    fn character_spawns(&self) -> Result<Vec<CharacterSpawnV1>, StoreError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT actor_id, spawn_state, spawn_journal_sequence
                 FROM characters ORDER BY actor_id ASC",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(StoreError::Sqlite)?;
        let mut spawns = Vec::new();
        for row in rows {
            let (actor_bytes, encoded, spawn_journal_sequence) = row.map_err(StoreError::Sqlite)?;
            let encoded = encoded.ok_or(StoreError::MissingCharacterSpawnState)?;
            if encoded.is_empty() || encoded.len() > MAX_CHARACTER_SPAWN_DECODED {
                return Err(StoreError::InvalidRecord);
            }
            let spawn: CharacterSpawnV1 =
                postcard::from_bytes(&encoded).map_err(StoreError::Postcard)?;
            let raw = u128::from_be_bytes(decode_array(&actor_bytes)?);
            let row_actor = ActorId::new((raw >> 64) as u64, raw as u64);
            if spawn_journal_sequence < 0
                || spawn.created_after_journal_sequence
                    != u64::try_from(spawn_journal_sequence)
                        .map_err(|_| StoreError::CorruptRecord)?
                || spawn.actor.id != row_actor
            {
                return Err(StoreError::CorruptRecord);
            }
            spawns.push(spawn);
        }
        spawns.sort_by_key(|spawn| {
            (
                spawn.created_after_journal_sequence,
                spawn.created_tick,
                spawn.actor.id,
            )
        });
        Ok(spawns)
    }

    pub fn write_snapshot(
        &mut self,
        journal_sequence: u64,
        world: &WorldState,
    ) -> Result<(), StoreError> {
        let sequence = i64::try_from(journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let snapshot = world.snapshot();
        let state_hash = world.canonical_hash().map_err(StoreError::Simulation)?;
        let encoded = postcard::to_stdvec(&snapshot).map_err(StoreError::Postcard)?;
        let compressed = zstd::stream::encode_all(encoded.as_slice(), 3).map_err(StoreError::Io)?;
        let tick = snapshot.tick.0.to_be_bytes();
        self.connection
            .execute(
                "INSERT OR REPLACE INTO snapshots(sequence, tick, state_hash, compressed_state)
                 VALUES (?1, ?2, ?3, ?4)",
                params![sequence, tick.as_slice(), state_hash.as_slice(), compressed],
            )
            .map_err(StoreError::Sqlite)?;
        Ok(())
    }

    pub fn latest_snapshot(&self) -> Result<Option<(u64, WorldState)>, StoreError> {
        self.snapshot_by_order("DESC")
    }

    pub fn oldest_snapshot(&self) -> Result<Option<(u64, WorldState)>, StoreError> {
        self.snapshot_by_order("ASC")
    }

    pub fn previous_snapshot(&self) -> Result<Option<(u64, WorldState)>, StoreError> {
        self.snapshot_by_order("PREVIOUS")
    }

    pub fn snapshot_at(
        &self,
        journal_sequence: u64,
    ) -> Result<Option<(u64, WorldState)>, StoreError> {
        let sequence = i64::try_from(journal_sequence).map_err(|_| StoreError::NumericOverflow)?;
        let row: Option<(i64, Vec<u8>, Vec<u8>)> = self
            .connection
            .query_row(
                "SELECT sequence, state_hash, compressed_state
                 FROM snapshots WHERE sequence = ?1",
                [sequence],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(StoreError::Sqlite)?;
        row.map(decode_snapshot_row).transpose()
    }

    fn snapshot_by_order(&self, order: &str) -> Result<Option<(u64, WorldState)>, StoreError> {
        let query = match order {
            "ASC" => {
                "SELECT sequence, state_hash, compressed_state
                 FROM snapshots ORDER BY sequence ASC LIMIT 1"
            }
            "DESC" => {
                "SELECT sequence, state_hash, compressed_state
                 FROM snapshots ORDER BY sequence DESC LIMIT 1"
            }
            "PREVIOUS" => {
                "SELECT sequence, state_hash, compressed_state
                 FROM snapshots ORDER BY sequence DESC LIMIT 1 OFFSET 1"
            }
            _ => return Err(StoreError::InvalidRecord),
        };
        let row: Option<(i64, Vec<u8>, Vec<u8>)> = self
            .connection
            .query_row(query, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .optional()
            .map_err(StoreError::Sqlite)?;
        row.map(decode_snapshot_row).transpose()
    }

    pub fn checkpoint(&self) -> Result<(), StoreError> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(StoreError::Sqlite)
    }

    pub fn database_path(&self) -> Result<Option<PathBuf>, StoreError> {
        let path: String = self
            .connection
            .query_row("PRAGMA database_list", [], |row| row.get(2))
            .map_err(StoreError::Sqlite)?;
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    pub fn backup_to(&self, path: impl AsRef<Path>) -> Result<DatabaseBackupMetadata, StoreError> {
        backup_connection_to(&self.connection, path.as_ref())
    }

    pub fn backup_from_path(
        source_path: impl AsRef<Path>,
        destination_path: impl AsRef<Path>,
    ) -> Result<DatabaseBackupMetadata, StoreError> {
        let source_metadata = fs::symlink_metadata(source_path.as_ref()).map_err(StoreError::Io)?;
        if !source_metadata.file_type().is_file() {
            return Err(StoreError::InvalidRecord);
        }
        let source = Connection::open_with_flags(
            source_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(StoreError::Sqlite)?;
        source
            .busy_timeout(Duration::from_secs(5))
            .map_err(StoreError::Sqlite)?;
        backup_connection_to(&source, destination_path.as_ref())
    }

    pub fn verify_backup(
        path: impl AsRef<Path>,
        expected: DatabaseBackupMetadata,
    ) -> Result<(), StoreError> {
        let path = path.as_ref();
        let metadata = fs::symlink_metadata(path).map_err(StoreError::Io)?;
        if !metadata.file_type().is_file() {
            return Err(StoreError::InvalidRecord);
        }
        let connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(StoreError::Sqlite)?;
        let actual = inspect_backup_connection_for_schema(connection, expected.schema_version)?;
        if actual != expected {
            return Err(StoreError::BackupVerificationMismatch);
        }
        Ok(())
    }
}

fn backup_connection_to(
    source: &Connection,
    path: &Path,
) -> Result<DatabaseBackupMetadata, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            return Err(StoreError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "backup destination already exists",
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(StoreError::Io(error)),
    }
    create_private_file(path)?;
    let result = (|| {
        let mut destination = Connection::open(path).map_err(StoreError::Sqlite)?;
        {
            let backup = rusqlite::backup::Backup::new(source, &mut destination)
                .map_err(StoreError::Sqlite)?;
            let started = Instant::now();
            loop {
                match backup.step(256).map_err(StoreError::Sqlite)? {
                    rusqlite::backup::StepResult::Done => break,
                    rusqlite::backup::StepResult::More
                    | rusqlite::backup::StepResult::Busy
                    | rusqlite::backup::StepResult::Locked => {
                        if started.elapsed() >= Duration::from_secs(10 * 60) {
                            return Err(StoreError::PersistenceTimeout);
                        }
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    _ => return Err(StoreError::BackupVerificationMismatch),
                }
            }
        }
        destination
            .pragma_update(None, "journal_mode", "DELETE")
            .map_err(StoreError::Sqlite)?;
        let metadata = inspect_backup_connection(destination)?;
        fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(StoreError::Io)?;
        Ok(metadata)
    })();
    if result.is_err() {
        for suffix in ["", "-shm", "-wal"] {
            let mut candidate = path.as_os_str().to_os_string();
            candidate.push(suffix);
            if let Err(error) = fs::remove_file(candidate)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(StoreError::Io(error));
            }
        }
    }
    result
}

fn inspect_backup_connection(connection: Connection) -> Result<DatabaseBackupMetadata, StoreError> {
    inspect_backup_connection_for_schema(connection, SCHEMA_VERSION)
}

fn inspect_backup_connection_for_schema(
    connection: Connection,
    expected_schema_version: i64,
) -> Result<DatabaseBackupMetadata, StoreError> {
    if expected_schema_version <= 0 || expected_schema_version > SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema(expected_schema_version));
    }
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(StoreError::Sqlite)?;
    if integrity != "ok" {
        return Err(StoreError::BackupVerificationMismatch);
    }
    let mut foreign_keys = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(StoreError::Sqlite)?;
    if foreign_keys
        .query([])
        .map_err(StoreError::Sqlite)?
        .next()
        .map_err(StoreError::Sqlite)?
        .is_some()
    {
        return Err(StoreError::BackupVerificationMismatch);
    }
    drop(foreign_keys);
    let schema_version: i64 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(StoreError::Sqlite)?;
    if schema_version != expected_schema_version {
        return Err(StoreError::UnsupportedSchema(schema_version));
    }
    let store = WorldStore { connection };
    let world_metadata = store.metadata()?;
    if schema_version >= 11 {
        let _verified_security_audit = store.security_audit_after(0)?;
    }
    let (journal_sequence, world) = store.recover_latest(WorldState::new(
        world_metadata.world_namespace,
        world_metadata.world_seed,
    ))?;
    Ok(DatabaseBackupMetadata {
        schema_version,
        world_namespace: world_metadata.world_namespace,
        journal_sequence,
        tick: world.tick(),
        state_hash: world.canonical_hash().map_err(StoreError::Simulation)?,
    })
}

fn existing_schema_version(connection: &Connection) -> Result<Option<i64>, StoreError> {
    let has_migrations: bool = connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master
                WHERE type = 'table' AND name = 'schema_migrations'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(StoreError::Sqlite)?;
    if !has_migrations {
        return Ok(None);
    }
    connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .map_err(StoreError::Sqlite)
}

fn serialized_world_state_present(connection: &Connection) -> Result<bool, StoreError> {
    for (table, query) in [
        (
            "snapshots",
            "SELECT EXISTS(SELECT 1 FROM snapshots LIMIT 1)",
        ),
        (
            "journal_batches",
            "SELECT EXISTS(SELECT 1 FROM journal_batches LIMIT 1)",
        ),
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(StoreError::Sqlite)?;
        if exists
            && connection
                .query_row(query, [], |row| row.get(0))
                .map_err(StoreError::Sqlite)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn create_pre_migration_backup(
    source: &Connection,
    database_path: &Path,
    existing_schema: i64,
) -> Result<(), StoreError> {
    if existing_schema <= 0 {
        return Err(StoreError::UnsupportedSchema(existing_schema));
    }
    let parent = database_path.parent().unwrap_or_else(|| Path::new("."));
    let directory = parent.join("pre-migration-backups");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => return Err(StoreError::InvalidRecord),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&directory).map_err(StoreError::Io)?;
        }
        Err(error) => return Err(StoreError::Io(error)),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(StoreError::Io)?;
    }
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StoreError::ClockRegression)?;
    let stem = database_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("world");
    let unique = format!(
        "{stem}-schema-{existing_schema}-{:020}-{:09}-{}",
        elapsed.as_secs(),
        elapsed.subsec_nanos(),
        std::process::id()
    );
    let temporary = directory.join(format!(".{unique}.tmp"));
    let final_path = directory.join(unique);
    fs::create_dir(&temporary).map_err(StoreError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700))
            .map_err(StoreError::Io)?;
    }
    let result = (|| {
        let database_backup = temporary.join(PRE_MIGRATION_DATABASE_FILE);
        create_private_file(&database_backup)?;
        let mut destination = Connection::open(&database_backup).map_err(StoreError::Sqlite)?;
        {
            let backup = rusqlite::backup::Backup::new(source, &mut destination)
                .map_err(StoreError::Sqlite)?;
            backup
                .run_to_completion(256, Duration::from_millis(2), None)
                .map_err(StoreError::Sqlite)?;
        }
        let integrity: String = destination
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(StoreError::Sqlite)?;
        if integrity != "ok" || existing_schema_version(&destination)? != Some(existing_schema) {
            return Err(StoreError::BackupVerificationMismatch);
        }
        let mut foreign_keys = destination
            .prepare("PRAGMA foreign_key_check")
            .map_err(StoreError::Sqlite)?;
        if foreign_keys
            .query([])
            .map_err(StoreError::Sqlite)?
            .next()
            .map_err(StoreError::Sqlite)?
            .is_some()
        {
            return Err(StoreError::BackupVerificationMismatch);
        }
        drop(foreign_keys);
        destination
            .pragma_update(None, "journal_mode", "DELETE")
            .map_err(StoreError::Sqlite)?;
        drop(destination);
        fs::File::open(&database_backup)
            .and_then(|file| file.sync_all())
            .map_err(StoreError::Io)?;

        let protected_members = collect_pre_migration_protected_members(database_path)?;
        let mut member_manifests = Vec::with_capacity(protected_members.len());
        for (filename, bytes) in protected_members {
            let member_path = temporary.join(&filename);
            write_private_file(&member_path, &bytes)?;
            member_manifests.push(PreMigrationBackupMemberV1 {
                filename,
                length: u64::try_from(bytes.len()).map_err(|_| StoreError::NumericOverflow)?,
                checksum: *blake3::hash(&bytes).as_bytes(),
            });
        }
        member_manifests.sort_by(|left, right| left.filename.cmp(&right.filename));
        let manifest = PreMigrationBackupManifestV1 {
            format_version: PRE_MIGRATION_BACKUP_FORMAT_VERSION,
            source_schema_version: existing_schema,
            created_utc_seconds: elapsed.as_secs(),
            created_utc_nanoseconds: elapsed.subsec_nanos(),
            database_checksum: hash_private_regular_file(&database_backup)?,
            protected_members: member_manifests,
        };
        let manifest_bytes = postcard::to_stdvec(&manifest).map_err(StoreError::Postcard)?;
        if manifest_bytes.len() as u64 > MAX_PRE_MIGRATION_MANIFEST_BYTES {
            return Err(StoreError::InvalidRecord);
        }
        write_private_file(
            &temporary.join(PRE_MIGRATION_MANIFEST_FILE),
            &manifest_bytes,
        )?;
        verify_pre_migration_backup_generation(&temporary, existing_schema)?;
        sync_private_directory(&temporary)?;
        fs::rename(&temporary, &final_path).map_err(StoreError::Io)?;
        sync_private_directory(&directory)
    })();
    if result.is_err() {
        let _cleanup = fs::remove_dir_all(&temporary);
    }
    result
}

fn collect_pre_migration_protected_members(
    database_path: &Path,
) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
    let identity_path = database_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PRE_MIGRATION_IDENTITY_FILE);
    let metadata = match fs::symlink_metadata(&identity_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(StoreError::Io(error)),
    };
    if !metadata.file_type().is_file() || metadata.len() != 32 {
        return Err(StoreError::InvalidRecord);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::InvalidRecord);
        }
    }
    let bytes = fs::read(identity_path).map_err(StoreError::Io)?;
    if bytes.len() != 32 {
        return Err(StoreError::InvalidRecord);
    }
    Ok(vec![(String::from(PRE_MIGRATION_IDENTITY_FILE), bytes)])
}

fn create_private_file(path: &Path) -> Result<(), StoreError> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map(|_| ()).map_err(StoreError::Io)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(StoreError::Io)?;
    file.write_all(bytes).map_err(StoreError::Io)?;
    file.sync_all().map_err(StoreError::Io)
}

fn hash_private_regular_file(path: &Path) -> Result<[u8; 32], StoreError> {
    let metadata = fs::symlink_metadata(path).map_err(StoreError::Io)?;
    if !metadata.file_type().is_file() {
        return Err(StoreError::InvalidRecord);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::InvalidRecord);
        }
    }
    let mut file = fs::File::open(path).map_err(StoreError::Io)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(StoreError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn read_pre_migration_manifest(
    generation: &Path,
) -> Result<PreMigrationBackupManifestV1, StoreError> {
    let path = generation.join(PRE_MIGRATION_MANIFEST_FILE);
    let metadata = fs::symlink_metadata(&path).map_err(StoreError::Io)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_PRE_MIGRATION_MANIFEST_BYTES {
        return Err(StoreError::InvalidRecord);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::InvalidRecord);
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(StoreError::Io)?
        .take(MAX_PRE_MIGRATION_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(StoreError::Io)?;
    if bytes.len() as u64 > MAX_PRE_MIGRATION_MANIFEST_BYTES {
        return Err(StoreError::InvalidRecord);
    }
    postcard::from_bytes(&bytes).map_err(StoreError::Postcard)
}

fn verify_pre_migration_backup_generation(
    generation: &Path,
    expected_schema: i64,
) -> Result<PreMigrationBackupManifestV1, StoreError> {
    let directory_metadata = fs::symlink_metadata(generation).map_err(StoreError::Io)?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(StoreError::InvalidRecord);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if directory_metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::InvalidRecord);
        }
    }
    let manifest = read_pre_migration_manifest(generation)?;
    if manifest.format_version != PRE_MIGRATION_BACKUP_FORMAT_VERSION
        || manifest.source_schema_version != expected_schema
        || manifest.created_utc_nanoseconds >= 1_000_000_000
        || manifest.protected_members.len() > 1
        || manifest
            .protected_members
            .windows(2)
            .any(|pair| pair[0].filename >= pair[1].filename)
    {
        return Err(StoreError::InvalidRecord);
    }

    let database_path = generation.join(PRE_MIGRATION_DATABASE_FILE);
    if hash_private_regular_file(&database_path)? != manifest.database_checksum {
        return Err(StoreError::BackupVerificationMismatch);
    }
    let connection = Connection::open_with_flags(
        &database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(StoreError::Sqlite)?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(StoreError::Sqlite)?;
    if integrity != "ok" || existing_schema_version(&connection)? != Some(expected_schema) {
        return Err(StoreError::BackupVerificationMismatch);
    }
    let mut foreign_keys = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(StoreError::Sqlite)?;
    if foreign_keys
        .query([])
        .map_err(StoreError::Sqlite)?
        .next()
        .map_err(StoreError::Sqlite)?
        .is_some()
    {
        return Err(StoreError::BackupVerificationMismatch);
    }
    drop(foreign_keys);
    drop(connection);

    let mut expected_files = BTreeSet::from([
        String::from(PRE_MIGRATION_DATABASE_FILE),
        String::from(PRE_MIGRATION_MANIFEST_FILE),
    ]);
    for member in &manifest.protected_members {
        if member.filename != PRE_MIGRATION_IDENTITY_FILE
            || member.length != 32
            || !expected_files.insert(member.filename.clone())
            || hash_private_regular_file(&generation.join(&member.filename))? != member.checksum
            || fs::symlink_metadata(generation.join(&member.filename))
                .map_err(StoreError::Io)?
                .len()
                != member.length
        {
            return Err(StoreError::BackupVerificationMismatch);
        }
    }
    for entry in fs::read_dir(generation).map_err(StoreError::Io)? {
        let entry = entry.map_err(StoreError::Io)?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or(StoreError::InvalidRecord)?;
        if !expected_files.remove(&name) || !entry.file_type().map_err(StoreError::Io)?.is_file() {
            return Err(StoreError::InvalidRecord);
        }
    }
    if !expected_files.is_empty() {
        return Err(StoreError::InvalidRecord);
    }
    Ok(manifest)
}

fn sync_private_directory(directory: &Path) -> Result<(), StoreError> {
    #[cfg(unix)]
    {
        fs::File::open(directory)
            .and_then(|directory| directory.sync_all())
            .map_err(StoreError::Io)?;
    }
    #[cfg(not(unix))]
    let _unused = directory;
    Ok(())
}

fn insert_security_audit(
    transaction: &rusqlite::Transaction<'_>,
    record: &SecurityAuditRecordV1,
) -> Result<u64, StoreError> {
    record.validate()?;
    let payload = postcard::to_stdvec(record).map_err(StoreError::Postcard)?;
    if payload.is_empty() || payload.len() > 65_536 {
        return Err(StoreError::InvalidRecord);
    }
    let record_hash = record.canonical_hash()?;
    transaction
        .execute(
            "INSERT INTO security_audit(occurred_utc, payload, record_hash)
             VALUES (?1, ?2, ?3)",
            params![record.occurred_utc_seconds, payload, record_hash.as_slice()],
        )
        .map_err(StoreError::Sqlite)?;
    u64::try_from(transaction.last_insert_rowid()).map_err(|_| StoreError::NumericOverflow)
}

fn current_persisted_tick(connection: &Connection) -> Result<SimTick, StoreError> {
    let tick: Option<Vec<u8>> = connection
        .query_row(
            "SELECT COALESCE(
                (SELECT last_tick FROM journal_batches ORDER BY sequence DESC LIMIT 1),
                (SELECT tick FROM snapshots ORDER BY sequence DESC LIMIT 1)
             )",
            [],
            |row| row.get(0),
        )
        .map_err(StoreError::Sqlite)?;
    tick.map_or(Ok(SimTick(0)), |tick| decode_u64(&tick).map(SimTick))
}

fn validate_security_audit_records(
    records: &[(u64, SecurityAuditRecordV1)],
) -> Result<(), StoreError> {
    let mut previous = None;
    for (sequence, record) in records {
        if *sequence == 0 || previous.is_some_and(|previous| *sequence <= previous) {
            return Err(StoreError::InvalidRecord);
        }
        record.validate()?;
        previous = Some(*sequence);
    }
    Ok(())
}

fn validate_security_audit_range(
    initial_sequence: u64,
    final_sequence: u64,
    records: &[(u64, SecurityAuditRecordV1)],
) -> Result<(), StoreError> {
    if final_sequence < initial_sequence {
        return Err(StoreError::InvalidRecord);
    }
    validate_security_audit_records(records)?;
    if records.is_empty() {
        return (initial_sequence == final_sequence)
            .then_some(())
            .ok_or(StoreError::InvalidRecord);
    }
    let expected_first = initial_sequence
        .checked_add(1)
        .ok_or(StoreError::NumericOverflow)?;
    if records.first().map(|(sequence, _)| *sequence) != Some(expected_first)
        || records.last().map(|(sequence, _)| *sequence) != Some(final_sequence)
        || records
            .windows(2)
            .any(|pair| pair[0].0.checked_add(1) != Some(pair[1].0))
    {
        return Err(StoreError::InvalidRecord);
    }
    Ok(())
}

fn security_audit_rejection(error: &StoreError) -> Option<SecurityAuditRejectionV1> {
    match error {
        StoreError::AccountUnavailable | StoreError::UnauthorizedEndpoint => {
            Some(SecurityAuditRejectionV1::AccountUnavailable)
        }
        StoreError::EndpointAlreadyBound => Some(SecurityAuditRejectionV1::EndpointAlreadyBound),
        StoreError::EndpointNotRevocable => Some(SecurityAuditRejectionV1::EndpointNotRevocable),
        StoreError::CannotRevokeLastEndpoint => Some(SecurityAuditRejectionV1::LastActiveEndpoint),
        StoreError::TooManyEndpointBindings => Some(SecurityAuditRejectionV1::TooManyBindings),
        StoreError::UnknownEndpoint => Some(SecurityAuditRejectionV1::UnknownEndpoint),
        StoreError::EndpointNotPending => Some(SecurityAuditRejectionV1::EndpointNotPending),
        StoreError::EnrollmentExpired => Some(SecurityAuditRejectionV1::EnrollmentExpired),
        StoreError::AdministratorRequired => Some(SecurityAuditRejectionV1::AdministratorRequired),
        StoreError::CannotTargetSelf => Some(SecurityAuditRejectionV1::CannotTargetSelf),
        StoreError::InvalidAccountTransition => Some(SecurityAuditRejectionV1::InvalidTransition),
        StoreError::CannotRemoveLastAdministrator => {
            Some(SecurityAuditRejectionV1::LastAdministrator)
        }
        StoreError::CannotReportSelf => Some(SecurityAuditRejectionV1::CannotReportSelf),
        StoreError::ModeratorRequired => Some(SecurityAuditRejectionV1::ModeratorRequired),
        StoreError::TargetRoleNotAllowed => Some(SecurityAuditRejectionV1::TargetRoleNotAllowed),
        StoreError::CharacterUnavailable => Some(SecurityAuditRejectionV1::CharacterUnavailable),
        StoreError::CharacterNameConflict => Some(SecurityAuditRejectionV1::CharacterNameConflict),
        StoreError::TooManyCharacters => Some(SecurityAuditRejectionV1::TooManyCharacters),
        StoreError::InvalidModerationDuration | StoreError::InvalidReport => {
            Some(SecurityAuditRejectionV1::InvalidRequest)
        }
        StoreError::ReportRateLimited => Some(SecurityAuditRejectionV1::RateLimited),
        StoreError::InvalidDisplayName
        | StoreError::InvalidEndpointIdentity
        | StoreError::InvalidStableId
        | StoreError::ClockRegression => Some(SecurityAuditRejectionV1::InvalidRequest),
        _ => None,
    }
}

fn decode_u64(bytes: &[u8]) -> Result<u64, StoreError> {
    Ok(u64::from_be_bytes(decode_array(bytes)?))
}

fn decode_snapshot_row(
    (sequence, expected_hash, compressed): (i64, Vec<u8>, Vec<u8>),
) -> Result<(u64, WorldState), StoreError> {
    let decoder =
        zstd::stream::read::Decoder::new(compressed.as_slice()).map_err(StoreError::Io)?;
    let mut decoded = Vec::new();
    decoder
        .take(MAX_SNAPSHOT_DECODED + 1)
        .read_to_end(&mut decoded)
        .map_err(StoreError::Io)?;
    if decoded.len() as u64 > MAX_SNAPSHOT_DECODED {
        return Err(StoreError::SnapshotTooLarge);
    }
    let snapshot: WorldSnapshotV1 = postcard::from_bytes(&decoded).map_err(StoreError::Postcard)?;
    let world = WorldState::from_snapshot(&snapshot).map_err(StoreError::Simulation)?;
    let actual_hash = world.canonical_hash().map_err(StoreError::Simulation)?;
    if actual_hash != decode_array::<32>(&expected_hash)? {
        return Err(StoreError::StateHashMismatch);
    }
    Ok((
        u64::try_from(sequence).map_err(|_| StoreError::CorruptRecord)?,
        world,
    ))
}

fn replay_parts(
    initial_sequence: u64,
    mut world: WorldState,
    spawns: &[CharacterSpawnV1],
    batches: &[(u64, JournalBatchV1)],
) -> Result<(u64, WorldState), StoreError> {
    if spawns.windows(2).any(|pair| {
        (
            pair[0].created_after_journal_sequence,
            pair[0].created_tick,
            pair[0].actor.id,
        ) > (
            pair[1].created_after_journal_sequence,
            pair[1].created_tick,
            pair[1].actor.id,
        )
    }) {
        return Err(StoreError::InvalidRecord);
    }
    let mut next_spawn = 0;
    while next_spawn < spawns.len()
        && spawns[next_spawn].created_after_journal_sequence <= initial_sequence
    {
        restore_character_at_boundary(&mut world, &spawns[next_spawn])?;
        next_spawn += 1;
    }
    let mut last_sequence = initial_sequence;
    for (batch_sequence, batch) in batches {
        if *batch_sequence <= last_sequence {
            return Err(StoreError::CorruptRecord);
        }
        while next_spawn < spawns.len()
            && spawns[next_spawn].created_after_journal_sequence < *batch_sequence
        {
            restore_character_at_boundary(&mut world, &spawns[next_spawn])?;
            next_spawn += 1;
        }
        batch.validate()?;
        let mut next_allocator_input = 0;
        for tick in &batch.ticks {
            apply_allocator_inputs_at_boundary(
                &mut world,
                &batch.allocator_inputs,
                &mut next_allocator_input,
            )?;
            let expected_tick = world
                .tick()
                .0
                .checked_add(1)
                .ok_or(StoreError::NumericOverflow)?;
            if tick.tick.0 != expected_tick {
                return Err(StoreError::ReplayTickMismatch);
            }
            let outcome = world
                .advance_tick_with_recovery_inputs(
                    tick.commands.clone(),
                    tick.held_movement.clone(),
                    tick.connection_updates.clone(),
                )
                .map_err(StoreError::Simulation)?;
            let events_hash =
                canonical_events_hash(&outcome.events).map_err(StoreError::Simulation)?;
            if events_hash != tick.events_hash || outcome.canonical_hash != tick.state_hash {
                return Err(StoreError::ReplayHashMismatch);
            }
        }
        apply_allocator_inputs_at_boundary(
            &mut world,
            &batch.allocator_inputs,
            &mut next_allocator_input,
        )?;
        if next_allocator_input != batch.allocator_inputs.len() {
            return Err(StoreError::ReplayTickMismatch);
        }
        last_sequence = *batch_sequence;
    }
    while next_spawn < spawns.len() {
        if spawns[next_spawn].created_after_journal_sequence > last_sequence {
            return Err(StoreError::ReplayTickMismatch);
        }
        restore_character_at_boundary(&mut world, &spawns[next_spawn])?;
        next_spawn += 1;
    }
    Ok((last_sequence, world))
}

fn apply_allocator_inputs_at_boundary(
    world: &mut WorldState,
    inputs: &[AllocatorInputV1],
    next: &mut usize,
) -> Result<(), StoreError> {
    while let Some(input) = inputs.get(*next).copied() {
        if input.at_tick() < world.tick() {
            return Err(StoreError::ReplayTickMismatch);
        }
        if input.at_tick() > world.tick() {
            break;
        }
        match input {
            AllocatorInputV1::IdBlockAbandoned { high_water, .. } => world
                .advance_allocator_high_water(high_water)
                .map_err(StoreError::Simulation)?,
            AllocatorInputV1::IdBlockReserved { block, .. } => world
                .install_reserved_block(block)
                .map_err(StoreError::Simulation)?,
        }
        *next = (*next).checked_add(1).ok_or(StoreError::NumericOverflow)?;
    }
    Ok(())
}

fn restore_character_at_boundary(
    world: &mut WorldState,
    spawn: &CharacterSpawnV1,
) -> Result<(), StoreError> {
    if spawn.created_tick != world.tick() && world.actor_snapshot(spawn.actor.id).is_none() {
        return Err(StoreError::ReplayTickMismatch);
    }
    if world.actor_snapshot(spawn.actor.id).is_none() {
        world
            .restore_actor(spawn.actor.clone())
            .map_err(StoreError::Simulation)?;
    }
    Ok(())
}

fn validate_display_name(display_name: &str) -> Result<(), StoreError> {
    if display_name.is_empty()
        || display_name.len() > 256
        || display_name.chars().count() > 64
        || display_name.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidDisplayName);
    }
    Ok(())
}

fn validate_character_name(name: &str) -> Result<(), StoreError> {
    if name.is_empty()
        || name.len() > 256
        || name.chars().count() > 64
        || name.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidCharacterName);
    }
    Ok(())
}

const fn encode_role(role: AccountRole) -> i64 {
    match role {
        AccountRole::Player => 0,
        AccountRole::Moderator => 1,
        AccountRole::Administrator => 2,
    }
}

fn decode_role(value: i64) -> Result<AccountRole, StoreError> {
    match value {
        0 => Ok(AccountRole::Player),
        1 => Ok(AccountRole::Moderator),
        2 => Ok(AccountRole::Administrator),
        _ => Err(StoreError::CorruptRecord),
    }
}

const fn encode_status(status: AccountStatus) -> i64 {
    match status {
        AccountStatus::InitialEnrollment => 0,
        AccountStatus::Enabled => 1,
        AccountStatus::Disabled => 2,
        AccountStatus::Banned => 3,
        AccountStatus::RecoveryLocked => 4,
    }
}

fn decode_status(value: i64) -> Result<AccountStatus, StoreError> {
    match value {
        0 => Ok(AccountStatus::InitialEnrollment),
        1 => Ok(AccountStatus::Enabled),
        2 => Ok(AccountStatus::Disabled),
        3 => Ok(AccountStatus::Banned),
        4 => Ok(AccountStatus::RecoveryLocked),
        _ => Err(StoreError::CorruptRecord),
    }
}

fn decode_endpoint_state(value: i64) -> Result<EndpointBindingState, StoreError> {
    match value {
        0 => Ok(EndpointBindingState::Pending),
        1 => Ok(EndpointBindingState::Active),
        2 => Ok(EndpointBindingState::Revoked),
        _ => Err(StoreError::CorruptRecord),
    }
}

fn ensure_endpoint_is_new(
    connection: &Connection,
    endpoint: EndpointIdentity,
) -> Result<(), StoreError> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM endpoint_bindings WHERE endpoint_id = ?1",
            [endpoint.0.as_slice()],
            |_| Ok(()),
        )
        .optional()
        .map_err(StoreError::Sqlite)?
        .is_some();
    if exists {
        return Err(StoreError::EndpointAlreadyBound);
    }
    Ok(())
}

fn ensure_active_account_endpoint(
    connection: &Connection,
    account_bytes: &[u8],
    endpoint: EndpointIdentity,
) -> Result<(), StoreError> {
    let active = connection
        .query_row(
            "SELECT 1 FROM endpoint_bindings
             WHERE endpoint_id = ?1 AND account_id = ?2 AND state = 1",
            params![endpoint.0.as_slice(), account_bytes],
            |_| Ok(()),
        )
        .optional()
        .map_err(StoreError::Sqlite)?
        .is_some();
    if !active {
        return Err(StoreError::UnauthorizedEndpoint);
    }
    Ok(())
}

fn require_management_operator(
    connection: &Connection,
    endpoint: EndpointIdentity,
    now_utc_seconds: i64,
) -> Result<AccountRecord, StoreError> {
    let account = require_authenticated_account(connection, endpoint, now_utc_seconds)?;
    if account.role == AccountRole::Player {
        return Err(StoreError::ModeratorRequired);
    }
    Ok(account)
}

fn require_authenticated_account(
    connection: &Connection,
    endpoint: EndpointIdentity,
    now_utc_seconds: i64,
) -> Result<AccountRecord, StoreError> {
    let account_bytes: Vec<u8> = connection
        .query_row(
            "SELECT account_id FROM endpoint_bindings
             WHERE endpoint_id = ?1 AND state = 1",
            [endpoint.0.as_slice()],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::Sqlite)?
        .ok_or(StoreError::UnauthorizedEndpoint)?;
    let account = query_account(connection, &account_bytes)?;
    if !account_is_available(&account, now_utc_seconds) {
        return Err(StoreError::AccountUnavailable);
    }
    Ok(account)
}

fn require_administrator(
    connection: &Connection,
    endpoint: EndpointIdentity,
    now_utc_seconds: i64,
) -> Result<AccountRecord, StoreError> {
    let account = require_management_operator(connection, endpoint, now_utc_seconds)?;
    if account.role != AccountRole::Administrator {
        return Err(StoreError::AdministratorRequired);
    }
    Ok(account)
}

fn ensure_another_enabled_administrator(
    connection: &Connection,
    excluded: AccountId,
    now_utc_seconds: i64,
) -> Result<(), StoreError> {
    let excluded_bytes = excluded.as_u128().to_be_bytes();
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM accounts
             WHERE role = ?1 AND status = ?2 AND account_id != ?3
               AND (suspended_until_utc IS NULL OR suspended_until_utc <= ?4)",
            params![
                encode_role(AccountRole::Administrator),
                encode_status(AccountStatus::Enabled),
                excluded_bytes.as_slice(),
                now_utc_seconds,
            ],
            |row| row.get(0),
        )
        .map_err(StoreError::Sqlite)?;
    if count == 0 {
        return Err(StoreError::CannotRemoveLastAdministrator);
    }
    Ok(())
}

fn ensure_moderation_target(
    operator: &AccountRecord,
    target: &AccountRecord,
) -> Result<(), StoreError> {
    if operator.id == target.id {
        return Err(StoreError::CannotTargetSelf);
    }
    if operator.role == AccountRole::Moderator && target.role != AccountRole::Player {
        return Err(StoreError::TargetRoleNotAllowed);
    }
    Ok(())
}

fn moderation_until(
    duration_seconds: Option<u32>,
    now_utc_seconds: i64,
) -> Result<Option<i64>, StoreError> {
    if now_utc_seconds <= 0 {
        return Err(StoreError::ClockRegression);
    }
    duration_seconds
        .map(|duration| {
            if duration == 0 || duration > MAX_MODERATION_DURATION_SECONDS {
                return Err(StoreError::InvalidModerationDuration);
            }
            now_utc_seconds
                .checked_add(i64::from(duration))
                .ok_or(StoreError::NumericOverflow)
        })
        .transpose()
}

fn query_characters(
    connection: &Connection,
    account_id: AccountId,
) -> Result<Vec<CharacterSummary>, StoreError> {
    let account_bytes = account_id.as_u128().to_be_bytes();
    let mut statement = connection
        .prepare(
            "SELECT actor_id, name FROM characters
             WHERE account_id = ?1 ORDER BY actor_id ASC",
        )
        .map_err(StoreError::Sqlite)?;
    let rows = statement
        .query_map([account_bytes.as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(StoreError::Sqlite)?;
    let mut characters = Vec::new();
    for row in rows {
        let (actor_bytes, name) = row.map_err(StoreError::Sqlite)?;
        let raw = u128::from_be_bytes(decode_array(&actor_bytes)?);
        characters.push(CharacterSummary {
            actor_id: ActorId::new((raw >> 64) as u64, raw as u64),
            name,
        });
    }
    Ok(characters)
}

fn query_endpoint_bindings(
    connection: &Connection,
    account_id: AccountId,
) -> Result<Vec<EndpointBindingSummary>, StoreError> {
    let account_bytes = account_id.as_u128().to_be_bytes();
    query_account(connection, &account_bytes)?;
    let mut statement = connection
        .prepare(
            "SELECT endpoint_id, state, pending_expires_utc
             FROM endpoint_bindings
             WHERE account_id = ?1 ORDER BY endpoint_id ASC",
        )
        .map_err(StoreError::Sqlite)?;
    let rows = statement
        .query_map([account_bytes.as_slice()], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(StoreError::Sqlite)?;
    let mut bindings = Vec::new();
    for row in rows {
        let (endpoint, state, pending_expires_utc) = row.map_err(StoreError::Sqlite)?;
        let state = decode_endpoint_state(state)?;
        if matches!(state, EndpointBindingState::Pending) != pending_expires_utc.is_some() {
            return Err(StoreError::CorruptRecord);
        }
        bindings.push(EndpointBindingSummary {
            endpoint: EndpointIdentity(decode_array(&endpoint)?),
            state,
            pending_expires_utc,
        });
    }
    if bindings.len() > MAX_ENDPOINT_BINDINGS_PER_ACCOUNT {
        return Err(StoreError::CorruptRecord);
    }
    Ok(bindings)
}

fn valid_report_details(details: &str) -> bool {
    !details.is_empty()
        && details.len() <= MAX_REPORT_BYTES
        && details.chars().count() <= MAX_REPORT_CHARACTERS
        && !details.chars().any(char::is_control)
}

const fn encode_report_reason(reason: ReportReason) -> i64 {
    match reason {
        ReportReason::Chat => 0,
        ReportReason::Harassment => 1,
        ReportReason::Exploit => 2,
        ReportReason::Other => 3,
    }
}

fn decode_report_reason(value: i64) -> Result<ReportReason, StoreError> {
    match value {
        0 => Ok(ReportReason::Chat),
        1 => Ok(ReportReason::Harassment),
        2 => Ok(ReportReason::Exploit),
        3 => Ok(ReportReason::Other),
        _ => Err(StoreError::CorruptRecord),
    }
}

const fn encode_report_state(state: ReportState) -> i64 {
    match state {
        ReportState::Open => 0,
        ReportState::Actioned => 1,
        ReportState::Dismissed => 2,
    }
}

fn decode_report_state(value: i64) -> Result<ReportState, StoreError> {
    match value {
        0 => Ok(ReportState::Open),
        1 => Ok(ReportState::Actioned),
        2 => Ok(ReportState::Dismissed),
        _ => Err(StoreError::CorruptRecord),
    }
}

const fn encode_moderation_kind(kind: ModerationKind) -> i64 {
    match kind {
        ModerationKind::Kick => 0,
        ModerationKind::Suspension => 1,
        ModerationKind::Mute => 2,
    }
}

fn decode_moderation_kind(value: i64) -> Result<ModerationKind, StoreError> {
    match value {
        0 => Ok(ModerationKind::Kick),
        1 => Ok(ModerationKind::Suspension),
        2 => Ok(ModerationKind::Mute),
        _ => Err(StoreError::CorruptRecord),
    }
}

fn query_report(connection: &Connection, report_id: ReportId) -> Result<ReportSummary, StoreError> {
    let report_id = i64::try_from(report_id.0).map_err(|_| StoreError::NumericOverflow)?;
    connection
        .query_row(
            "SELECT report_id, created_utc, reporter_account_id, reporter_actor_id,
                    reporter_character, target_account_id, target_actor_id,
                    target_character, reason, details, state, resolved_utc,
                    resolved_by_account_id, resolution_audit_sequence
             FROM player_reports WHERE report_id = ?1",
            [report_id],
            decode_report_row,
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => StoreError::InvalidReport,
            other => StoreError::Sqlite(other),
        })?
}

fn query_reports_page(
    connection: &Connection,
    state: Option<ReportState>,
    after: Option<i64>,
    row_limit: i64,
) -> Result<Vec<ReportSummary>, StoreError> {
    let mut reports = Vec::new();
    match (state, after) {
        (Some(state), Some(after)) => {
            let mut statement = connection
                .prepare(
                    "SELECT report_id, created_utc, reporter_account_id, reporter_actor_id,
                            reporter_character, target_account_id, target_actor_id,
                            target_character, reason, details, state, resolved_utc,
                            resolved_by_account_id, resolution_audit_sequence
                     FROM player_reports
                     WHERE state = ?1 AND report_id > ?2
                     ORDER BY report_id ASC LIMIT ?3",
                )
                .map_err(StoreError::Sqlite)?;
            let rows = statement
                .query_map(
                    params![encode_report_state(state), after, row_limit],
                    decode_report_row,
                )
                .map_err(StoreError::Sqlite)?;
            for row in rows {
                reports.push(row.map_err(StoreError::Sqlite)??);
            }
        }
        (Some(state), None) => {
            let mut statement = connection
                .prepare(
                    "SELECT report_id, created_utc, reporter_account_id, reporter_actor_id,
                            reporter_character, target_account_id, target_actor_id,
                            target_character, reason, details, state, resolved_utc,
                            resolved_by_account_id, resolution_audit_sequence
                     FROM player_reports WHERE state = ?1
                     ORDER BY report_id ASC LIMIT ?2",
                )
                .map_err(StoreError::Sqlite)?;
            let rows = statement
                .query_map(
                    params![encode_report_state(state), row_limit],
                    decode_report_row,
                )
                .map_err(StoreError::Sqlite)?;
            for row in rows {
                reports.push(row.map_err(StoreError::Sqlite)??);
            }
        }
        (None, Some(after)) => {
            let mut statement = connection
                .prepare(
                    "SELECT report_id, created_utc, reporter_account_id, reporter_actor_id,
                            reporter_character, target_account_id, target_actor_id,
                            target_character, reason, details, state, resolved_utc,
                            resolved_by_account_id, resolution_audit_sequence
                     FROM player_reports WHERE report_id > ?1
                     ORDER BY report_id ASC LIMIT ?2",
                )
                .map_err(StoreError::Sqlite)?;
            let rows = statement
                .query_map(params![after, row_limit], decode_report_row)
                .map_err(StoreError::Sqlite)?;
            for row in rows {
                reports.push(row.map_err(StoreError::Sqlite)??);
            }
        }
        (None, None) => {
            let mut statement = connection
                .prepare(
                    "SELECT report_id, created_utc, reporter_account_id, reporter_actor_id,
                            reporter_character, target_account_id, target_actor_id,
                            target_character, reason, details, state, resolved_utc,
                            resolved_by_account_id, resolution_audit_sequence
                     FROM player_reports ORDER BY report_id ASC LIMIT ?1",
                )
                .map_err(StoreError::Sqlite)?;
            let rows = statement
                .query_map([row_limit], decode_report_row)
                .map_err(StoreError::Sqlite)?;
            for row in rows {
                reports.push(row.map_err(StoreError::Sqlite)??);
            }
        }
    }
    Ok(reports)
}

fn decode_report_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<ReportSummary, StoreError>> {
    let report_id = row.get::<_, i64>(0)?;
    let created_utc = row.get::<_, i64>(1)?;
    let reporter_account = row.get::<_, Vec<u8>>(2)?;
    let reporter_actor = row.get::<_, Vec<u8>>(3)?;
    let reporter_character = row.get::<_, String>(4)?;
    let target_account = row.get::<_, Vec<u8>>(5)?;
    let target_actor = row.get::<_, Vec<u8>>(6)?;
    let target_character = row.get::<_, String>(7)?;
    let reason = row.get::<_, i64>(8)?;
    let details = row.get::<_, String>(9)?;
    let state = row.get::<_, i64>(10)?;
    let resolved_utc = row.get::<_, Option<i64>>(11)?;
    let resolved_by_account = row.get::<_, Option<Vec<u8>>>(12)?;
    let resolution_audit_sequence = row.get::<_, Option<i64>>(13)?;
    Ok((|| {
        if report_id <= 0
            || created_utc <= 0
            || !valid_report_details(&details)
            || reporter_character.is_empty()
            || target_character.is_empty()
        {
            return Err(StoreError::CorruptRecord);
        }
        let reporter_account = u128::from_be_bytes(decode_array(&reporter_account)?);
        let reporter_actor = u128::from_be_bytes(decode_array(&reporter_actor)?);
        let target_account = u128::from_be_bytes(decode_array(&target_account)?);
        let target_actor = u128::from_be_bytes(decode_array(&target_actor)?);
        let state = decode_report_state(state)?;
        let resolved_by_account = resolved_by_account
            .map(|bytes| decode_array(&bytes).map(u128::from_be_bytes))
            .transpose()?;
        let resolution_audit_sequence = resolution_audit_sequence
            .map(|sequence| u64::try_from(sequence).map_err(|_| StoreError::CorruptRecord))
            .transpose()?;
        let resolution_is_valid = match state {
            ReportState::Open => {
                resolved_utc.is_none()
                    && resolved_by_account.is_none()
                    && resolution_audit_sequence.is_none()
            }
            ReportState::Actioned | ReportState::Dismissed => {
                resolved_utc.is_some_and(|resolved| resolved > 0)
                    && resolved_by_account.is_some()
                    && resolution_audit_sequence.is_some_and(|sequence| sequence > 0)
            }
        };
        if !resolution_is_valid {
            return Err(StoreError::CorruptRecord);
        }
        Ok(ReportSummary {
            report_id: ReportId(u64::try_from(report_id).map_err(|_| StoreError::CorruptRecord)?),
            created_utc,
            reporter_account: AccountId::new(
                (reporter_account >> 64) as u64,
                reporter_account as u64,
            ),
            reporter_actor: ActorId::new((reporter_actor >> 64) as u64, reporter_actor as u64),
            reporter_character,
            target_account: AccountId::new((target_account >> 64) as u64, target_account as u64),
            target_actor: ActorId::new((target_actor >> 64) as u64, target_actor as u64),
            target_character,
            reason: decode_report_reason(reason)?,
            details,
            state,
            resolved_utc,
            resolved_by_account: resolved_by_account
                .map(|account| AccountId::new((account >> 64) as u64, account as u64)),
            resolution_audit_sequence,
        })
    })())
}

fn decode_moderation_history_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<ModerationHistoryEntry, StoreError>> {
    let history_id = row.get::<_, i64>(0)?;
    let security_audit_sequence = row.get::<_, i64>(1)?;
    let occurred_utc = row.get::<_, i64>(2)?;
    let operator_account = row.get::<_, Vec<u8>>(3)?;
    let target_account = row.get::<_, Vec<u8>>(4)?;
    let kind = row.get::<_, i64>(5)?;
    let until_utc = row.get::<_, Option<i64>>(6)?;
    Ok((|| {
        if history_id <= 0
            || security_audit_sequence <= 0
            || occurred_utc <= 0
            || until_utc.is_some_and(|until| until <= 0)
        {
            return Err(StoreError::CorruptRecord);
        }
        let operator_account = u128::from_be_bytes(decode_array(&operator_account)?);
        let target_account = u128::from_be_bytes(decode_array(&target_account)?);
        Ok(ModerationHistoryEntry {
            history_id: u64::try_from(history_id).map_err(|_| StoreError::CorruptRecord)?,
            security_audit_sequence: u64::try_from(security_audit_sequence)
                .map_err(|_| StoreError::CorruptRecord)?,
            occurred_utc,
            operator_account: AccountId::new(
                (operator_account >> 64) as u64,
                operator_account as u64,
            ),
            target_account: AccountId::new((target_account >> 64) as u64, target_account as u64),
            kind: decode_moderation_kind(kind)?,
            until_utc,
        })
    })())
}

fn insert_moderation_history(
    transaction: &rusqlite::Transaction<'_>,
    security_audit_sequence: u64,
    occurred_utc: i64,
    operator_account: AccountId,
    target_account: AccountId,
    kind: ModerationKind,
    until_utc: Option<i64>,
) -> Result<(), StoreError> {
    let security_audit_sequence =
        i64::try_from(security_audit_sequence).map_err(|_| StoreError::NumericOverflow)?;
    let operator_account = operator_account.as_u128().to_be_bytes();
    let target_account = target_account.as_u128().to_be_bytes();
    let changed = transaction
        .execute(
            "INSERT INTO moderation_history(
                security_audit_sequence, occurred_utc, operator_account_id,
                target_account_id, kind, until_utc
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                security_audit_sequence,
                occurred_utc,
                operator_account.as_slice(),
                target_account.as_slice(),
                encode_moderation_kind(kind),
                until_utc,
            ],
        )
        .map_err(StoreError::Sqlite)?;
    if changed != 1 {
        return Err(StoreError::InvalidRecord);
    }
    Ok(())
}

fn ensure_binding_capacity(
    connection: &Connection,
    account_bytes: &[u8],
) -> Result<(), StoreError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM endpoint_bindings WHERE account_id = ?1",
            [account_bytes],
            |row| row.get(0),
        )
        .map_err(StoreError::Sqlite)?;
    if count
        >= i64::try_from(MAX_ENDPOINT_BINDINGS_PER_ACCOUNT)
            .map_err(|_| StoreError::NumericOverflow)?
    {
        return Err(StoreError::TooManyEndpointBindings);
    }
    Ok(())
}

fn security_actor_for_endpoint(
    connection: &Connection,
    endpoint: EndpointIdentity,
) -> Result<SecurityAuditActorV1, StoreError> {
    let binding: Option<(Vec<u8>, i64)> = connection
        .query_row(
            "SELECT account_id, state FROM endpoint_bindings WHERE endpoint_id = ?1",
            [endpoint.0.as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(StoreError::Sqlite)?;
    let Some((account_bytes, state)) = binding else {
        return Ok(SecurityAuditActorV1::EndpointProof {
            endpoint,
            account_id: None,
            role: None,
        });
    };
    let account = query_account(connection, &account_bytes)?;
    if decode_endpoint_state(state)? == EndpointBindingState::Active
        && account.status == AccountStatus::Enabled
    {
        Ok(SecurityAuditActorV1::AuthenticatedAccount {
            account_id: account.id,
            endpoint,
            role: account.role,
        })
    } else {
        Ok(SecurityAuditActorV1::EndpointProof {
            endpoint,
            account_id: Some(account.id),
            role: Some(account.role),
        })
    }
}

fn query_account(
    connection: &Connection,
    account_bytes: &[u8],
) -> Result<AccountRecord, StoreError> {
    let (display_name, role, status, suspended_until_utc, muted_until_utc): (
        String,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
    ) = connection
        .query_row(
            "SELECT display_name, role, status, suspended_until_utc, muted_until_utc
             FROM accounts WHERE account_id = ?1",
            [account_bytes],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => StoreError::AccountUnavailable,
            other => StoreError::Sqlite(other),
        })?;
    let raw = u128::from_be_bytes(decode_array(account_bytes)?);
    Ok(AccountRecord {
        id: AccountId::new((raw >> 64) as u64, raw as u64),
        display_name,
        role: decode_role(role)?,
        status: decode_status(status)?,
        suspended_until_utc,
        muted_until_utc,
    })
}

fn reserve_account_id_in_transaction(connection: &Connection) -> Result<AccountId, StoreError> {
    let (namespace_bytes, account_high_water_bytes): (Vec<u8>, Vec<u8>) = connection
        .query_row(
            "SELECT world_namespace, account_high_water
             FROM world_metadata WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
            other => StoreError::Sqlite(other),
        })?;
    let namespace = decode_u64(&namespace_bytes)?;
    if namespace == 0 {
        return Err(StoreError::CorruptRecord);
    }
    let account_high_water = decode_u64(&account_high_water_bytes)?;
    let latest_account: Option<Vec<u8>> = connection
        .query_row(
            "SELECT account_id FROM accounts ORDER BY account_id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::Sqlite)?;
    let latest_counter = latest_account
        .map(|bytes| {
            let raw = u128::from_be_bytes(decode_array(&bytes)?);
            let account = AccountId::new((raw >> 64) as u64, raw as u64);
            if account.world_namespace() != namespace || account.counter() == 0 {
                return Err(StoreError::CorruptRecord);
            }
            Ok(account.counter())
        })
        .transpose()?
        .unwrap_or(0);
    let next_counter = account_high_water
        .max(latest_counter)
        .checked_add(1)
        .ok_or(StoreError::NumericOverflow)?;
    let next_counter_bytes = next_counter.to_be_bytes();
    let changed = connection
        .execute(
            "UPDATE world_metadata SET account_high_water = ?1 WHERE singleton = 1",
            [next_counter_bytes.as_slice()],
        )
        .map_err(StoreError::Sqlite)?;
    if changed != 1 {
        return Err(StoreError::WorldUninitialized);
    }
    Ok(AccountId::new(namespace, next_counter))
}

fn account_is_available(account: &AccountRecord, now_utc_seconds: i64) -> bool {
    account.status == AccountStatus::Enabled
        && account
            .suspended_until_utc
            .is_none_or(|until| until <= now_utc_seconds)
}

fn validate_runtime_anchor(
    connection: &Connection,
    now_utc_seconds: i64,
) -> Result<(), StoreError> {
    if now_utc_seconds < 0 {
        return Err(StoreError::ClockRegression);
    }
    let (state, anchor): (i64, i64) = connection
        .query_row(
            "SELECT runtime_state, last_committed_utc FROM world_metadata WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => StoreError::WorldUninitialized,
            other => StoreError::Sqlite(other),
        })?;
    if state != 1 {
        return Err(StoreError::RuntimeInactive);
    }
    if anchor < 0 || now_utc_seconds < anchor {
        return Err(StoreError::ClockRegression);
    }
    Ok(())
}

fn decode_array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], StoreError> {
    bytes.try_into().map_err(|_| StoreError::CorruptRecord)
}

#[derive(Debug)]
pub enum StoreError {
    AccountMuted(i64),
    AccountUnavailable,
    AdministratorRequired,
    BackupVerificationMismatch,
    CannotRevokeLastEndpoint,
    CannotRemoveLastAdministrator,
    CannotReportSelf,
    CannotTargetSelf,
    CharacterAlreadyExists,
    CharacterNameConflict,
    CharacterUnavailable,
    ClockRegression,
    CorruptRecord,
    EndpointAlreadyBound,
    EndpointNotRevocable,
    EndpointNotPending,
    EnrollmentExpired,
    InvalidDisplayName,
    InvalidEndpointIdentity,
    InvalidCharacterName,
    InvalidAccountTransition,
    InvalidRecord,
    InvalidStableId,
    InvalidModerationDuration,
    InvalidReport,
    Io(std::io::Error),
    MissingCharacterSpawnState,
    MissingSnapshot,
    ModeratorRequired,
    NumericOverflow,
    Postcard(postcard::Error),
    PersistenceBusy,
    PersistenceTimeout,
    PersistenceUnavailable,
    ReplayHashMismatch,
    ReplayArchiveCursorChanged,
    ReplayTickMismatch,
    ReportRateLimited,
    RuntimeInactive,
    RuntimeActive,
    Simulation(SimError),
    SnapshotTooLarge,
    Sqlite(rusqlite::Error),
    StateHashMismatch,
    TooManyCharacters,
    TooManyEndpointBindings,
    TargetRoleNotAllowed,
    UnauthorizedEndpoint,
    UnknownEndpoint,
    UnsupportedSchema(i64),
    WorldIdentityMismatch,
    WorldUninitialized,
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountMuted(until) => write!(formatter, "account is muted until UTC {until}"),
            Self::AccountUnavailable => formatter.write_str("account is unavailable"),
            Self::AdministratorRequired => formatter.write_str("administrator role is required"),
            Self::BackupVerificationMismatch => {
                formatter.write_str("database backup verification mismatch")
            }
            Self::CannotRevokeLastEndpoint => {
                formatter.write_str("cannot revoke an account's last active endpoint")
            }
            Self::CannotRemoveLastAdministrator => {
                formatter.write_str("cannot remove the last enabled administrator")
            }
            Self::CannotReportSelf => formatter.write_str("an account cannot report itself"),
            Self::CannotTargetSelf => {
                formatter.write_str("administrators cannot target themselves")
            }
            Self::CharacterAlreadyExists => formatter.write_str("character already exists"),
            Self::CharacterNameConflict => {
                formatter.write_str("destination account already has that character name")
            }
            Self::CharacterUnavailable => formatter.write_str("character is unavailable"),
            Self::ClockRegression => formatter.write_str("runtime UTC clock moved backwards"),
            Self::CorruptRecord => formatter.write_str("corrupt persistence record"),
            Self::EndpointAlreadyBound => {
                formatter.write_str("endpoint identity is already permanently bound")
            }
            Self::EndpointNotRevocable => {
                formatter.write_str("endpoint identity is neither active nor pending")
            }
            Self::EndpointNotPending => formatter.write_str("endpoint identity is not pending"),
            Self::EnrollmentExpired => formatter.write_str("pending enrollment has expired"),
            Self::InvalidDisplayName => formatter.write_str("invalid account display name"),
            Self::InvalidEndpointIdentity => formatter.write_str("invalid iroh endpoint identity"),
            Self::InvalidCharacterName => formatter.write_str("invalid character name"),
            Self::InvalidAccountTransition => {
                formatter.write_str("invalid account role or status transition")
            }
            Self::InvalidRecord => formatter.write_str("invalid persistence record"),
            Self::InvalidStableId => formatter.write_str("stable ID does not belong to this world"),
            Self::InvalidModerationDuration => {
                formatter.write_str("moderation duration must be between one second and 24 hours")
            }
            Self::InvalidReport => formatter.write_str("invalid player report"),
            Self::Io(error) => write!(formatter, "persistence I/O error: {error}"),
            Self::MissingCharacterSpawnState => {
                formatter.write_str("character is missing its durable spawn state")
            }
            Self::MissingSnapshot => formatter.write_str("world has no replayable snapshot"),
            Self::ModeratorRequired => {
                formatter.write_str("moderator or administrator role is required")
            }
            Self::NumericOverflow => formatter.write_str("persistence numeric overflow"),
            Self::Postcard(error) => write!(formatter, "persistence Postcard error: {error}"),
            Self::PersistenceBusy => formatter.write_str("persistence worker is busy"),
            Self::PersistenceTimeout => formatter.write_str("persistence worker timed out"),
            Self::PersistenceUnavailable => {
                formatter.write_str("persistence worker is unavailable")
            }
            Self::ReplayHashMismatch => formatter.write_str("journal replay hash mismatch"),
            Self::ReplayArchiveCursorChanged => {
                formatter.write_str("replay archive cursor changed concurrently")
            }
            Self::ReplayTickMismatch => formatter.write_str("journal replay tick mismatch"),
            Self::ReportRateLimited => {
                formatter.write_str("account report rate limit is exhausted")
            }
            Self::RuntimeInactive => formatter.write_str("world runtime is not marked active"),
            Self::RuntimeActive => {
                formatter.write_str("world runtime is active; stop the server first")
            }
            Self::Simulation(error) => write!(formatter, "invalid simulation state: {error}"),
            Self::SnapshotTooLarge => formatter.write_str("snapshot exceeds decoded size limit"),
            Self::Sqlite(error) => write!(formatter, "SQLite error: {error}"),
            Self::StateHashMismatch => formatter.write_str("snapshot state hash mismatch"),
            Self::TooManyCharacters => formatter.write_str("account has too many characters"),
            Self::TooManyEndpointBindings => {
                formatter.write_str("account has too many endpoint bindings")
            }
            Self::TargetRoleNotAllowed => {
                formatter.write_str("operator may not moderate the target role")
            }
            Self::UnauthorizedEndpoint => {
                formatter.write_str("endpoint identity is not authorized")
            }
            Self::UnknownEndpoint => formatter.write_str("endpoint identity is unknown"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported schema version {version}")
            }
            Self::WorldIdentityMismatch => {
                formatter.write_str("world identity does not match database")
            }
            Self::WorldUninitialized => formatter.write_str("world database is not initialized"),
        }
    }
}

impl std::error::Error for StoreError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use cdda_protocol::{
        BashTargetKindV1, BookStudyV1, ChunkCoord, CommandKind, CommandSequence,
        CraftComponentRequirementV1, CraftItemPrototypeV1, CraftRecipeV1, DisassemblyComponentV1,
        DisassemblyRecipeV1, FieldIntensityLevelV1, FieldTypeSnapshotV1, FurnitureBashTypeV1,
        FurnitureTileSnapshot, HeldInputSequence, HeldMovementUpdateSource, HorizontalDirection,
        ItemId, LocalTileCoord, MagazineWellPrototypeV1, PoweredToolStateV1, RangedWeaponSnapshot,
        SleepReason, TerrainBashTypeV1, TerrainTileSnapshot, WorldEvent, WorldEventKind,
        WorldPosition,
    };
    use cdda_sim::{Chunk, CreatureSpawn, ItemSpawn, ReservedIdBlock};

    use super::*;

    static NEXT_TEST_DB: AtomicU64 = AtomicU64::new(1);

    fn test_database_path() -> PathBuf {
        let id = NEXT_TEST_DB.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("cdda-rust-{}-{id}.db", std::process::id()))
    }

    fn remove_database(path: &Path) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = PathBuf::from(format!("{}{suffix}", path.display()));
            if let Err(error) = std::fs::remove_file(candidate)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                panic!("failed to clean test database: {error}");
            }
        }
    }

    #[test]
    fn atomic_mapgen_catalog_and_chunks_recover_from_sqlite_snapshot() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(63, [19; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let terrain = TerrainTileSnapshot {
            terrain_id: String::from("t_grass"),
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
        let cell = cdda_protocol::WorldgenCellV1 {
            terrain: vec![vec![cdda_protocol::WorldgenWeightedTerrainTargetV1 {
                target: cdda_protocol::WorldgenTerrainTargetV1::Prototype(0),
                weight: 1,
            }]],
            furniture: vec![vec![cdda_protocol::WorldgenWeightedFurnitureTargetV1 {
                target: cdda_protocol::WorldgenFurnitureTargetV1::None,
                weight: 1,
            }]],
            item_group: None,
        };
        let catalog = cdda_protocol::WorldgenCatalogV1 {
            generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
            overmap: cdda_protocol::WorldgenOvermapLayoutV1 {
                origin_x: -90,
                origin_y: -90,
                identities: vec![cdda_protocol::WorldgenOmtIdentityV1 {
                    full_id: String::from("sqlite_field"),
                    type_id: String::from("sqlite_field"),
                    subtype_id: String::from("sqlite_field"),
                    generator_id: String::from("sqlite_field"),
                    rotation: 0,
                }],
                layers: vec![cdda_protocol::WorldgenOvermapLayerV1 {
                    z: 0,
                    runs: vec![cdda_protocol::WorldgenOvermapRunV1 {
                        identity_index: 0,
                        length: u32::from(cdda_protocol::WORLDGEN_OVERMAP_WIDTH)
                            * u32::from(cdda_protocol::WORLDGEN_OVERMAP_HEIGHT),
                    }],
                }],
            },
            cities: vec![cdda_protocol::WorldgenCityV1 {
                city_id: cdda_protocol::WorldgenCityId(1),
                center: cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 },
                size: 8,
            }],
            rivers: Vec::new(),
            specials: Vec::new(),
            start_location: None,
            terrain_prototypes: vec![terrain],
            furniture_prototypes: Vec::new(),
            monster_prototypes: Vec::new(),
            monster_groups: Vec::new(),
            regional_terrain: Vec::new(),
            regional_furniture: Vec::new(),
            omt_generators: vec![cdda_protocol::WorldgenOmtGeneratorV1 {
                omt_id: String::from("sqlite_field"),
                templates: vec![cdda_protocol::WorldgenTemplateV1 {
                    weight: 1,
                    predecessor_id: None,
                    builtin: None,
                    cells: vec![cell; cdda_protocol::WORLDGEN_CELLS_PER_OMT],
                    nested: Vec::new(),
                    area_items: Vec::new(),
                    monster_placements: Vec::new(),
                    individual_monster_placements: Vec::new(),
                    erase_all_before_placing_terrain: false,
                    deferred_fields: Vec::new(),
                }],
                nested_generators: Vec::new(),
            }],
        };
        let expected_overmap = catalog.overmap.clone();
        let expected_cities = catalog.cities.clone();
        let mut world = WorldState::new(63, [19; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world
            .configure_worldgen(catalog)
            .expect("catalog should configure");
        world
            .generate_initial_bubble(WorldPosition { x: 0, y: 0, z: 0 })
            .expect("initial cells should generate");
        assert_eq!(world.snapshot().chunks.len(), 144);
        let expected_hash = world.canonical_hash().expect("world should hash");
        store
            .write_snapshot(0, &world)
            .expect("mapgen snapshot should write");

        let (sequence, recovered) = store
            .recover_latest(WorldState::new(63, [19; 32]))
            .expect("mapgen snapshot should recover");
        assert_eq!(sequence, 0);
        let recovered_snapshot = recovered.snapshot();
        assert_eq!(recovered_snapshot.chunks.len(), 144);
        assert_eq!(
            recovered_snapshot
                .worldgen
                .as_ref()
                .expect("catalog should recover")
                .overmap
                .clone(),
            expected_overmap
        );
        assert_eq!(
            recovered_snapshot
                .worldgen
                .as_ref()
                .expect("catalog should recover")
                .cities,
            expected_cities
        );
        assert_eq!(
            recovered.canonical_hash().expect("recovered hash"),
            expected_hash
        );
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [71; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("mapgen replay should export")
            .verify(&content)
            .expect("mapgen replay should verify");
        assert_eq!(replayed.snapshot().chunks.len(), 144);
        assert_eq!(
            replayed
                .snapshot()
                .worldgen
                .as_ref()
                .expect("catalog should replay")
                .cities,
            expected_cities
        );
        assert_eq!(
            replayed.canonical_hash().expect("replayed hash"),
            expected_hash
        );
    }

    #[test]
    fn runtime_marker_distinguishes_clean_stop_and_crash_anchor() {
        let path = test_database_path();
        remove_database(&path);
        let mut store = WorldStore::open(&path).expect("store should open");
        store
            .initialize_world(61, [9; 32])
            .expect("world should initialize");
        store
            .require_runtime_inactive()
            .expect("local tools may operate before runtime starts");
        let clean_start = store.begin_runtime(100).expect("runtime should begin");
        assert!(matches!(
            store.require_runtime_inactive(),
            Err(StoreError::RuntimeActive)
        ));
        assert!(!clean_start.previous_exit_was_unclean);
        assert_eq!(
            clean_start.elapsed_seconds().expect("elapsed should fit"),
            0
        );

        let mut world = WorldState::new(61, [9; 32]);
        let outcome = world
            .advance_tick(Vec::new())
            .expect("commandless tick should advance");
        store
            .append_journal_batch_at(
                &JournalBatchV1 {
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
                },
                101,
            )
            .expect("journal and runtime anchor should commit atomically");
        drop(store);

        let mut recovered = WorldStore::open(&path).expect("store should reopen after crash");
        let crash_start = recovered
            .begin_runtime(106)
            .expect("crash recovery runtime should begin");
        assert!(crash_start.previous_exit_was_unclean);
        assert_eq!(crash_start.from_utc_seconds, 101);
        assert_eq!(
            crash_start.elapsed_seconds().expect("elapsed should fit"),
            5
        );
        recovered
            .finish_runtime(106)
            .expect("clean stop should persist");
        drop(recovered);

        let mut clean = WorldStore::open(&path).expect("store should reopen after clean stop");
        let clean_restart = clean
            .begin_runtime(1_000)
            .expect("clean restart should begin");
        assert!(!clean_restart.previous_exit_was_unclean);
        assert_eq!(
            clean_restart.elapsed_seconds().expect("elapsed should fit"),
            0
        );
        clean
            .finish_runtime(1_000)
            .expect("test should stop cleanly");
        clean
            .require_runtime_inactive()
            .expect("local tools may operate after a clean stop");
        drop(clean);
        remove_database(&path);
    }

    #[test]
    fn id_blocks_are_reserved_transactionally_across_reopen() {
        let path = test_database_path();
        remove_database(&path);
        {
            let mut store = WorldStore::open(&path).expect("database should open");
            store
                .initialize_world(9, [4; 32])
                .expect("world should initialize");
            assert_eq!(
                store
                    .reserve_account_id()
                    .expect("first account ID should reserve"),
                AccountId::new(9, 1)
            );
            assert_eq!(
                store.reserve_id_block().expect("first reservation"),
                ReservedIdBlock::new(1, ID_RESERVATION_SIZE).expect("first block should fit")
            );
            store.checkpoint().expect("checkpoint should succeed");
        }
        {
            let mut store = WorldStore::open(&path).expect("database should reopen");
            assert_eq!(
                store
                    .reserve_account_id()
                    .expect("second account ID should reserve"),
                AccountId::new(9, 2)
            );
            assert_eq!(
                store.reserve_id_block().expect("second reservation"),
                ReservedIdBlock::new(ID_RESERVATION_SIZE + 1, ID_RESERVATION_SIZE * 2)
                    .expect("second block should fit")
            );
        }
        remove_database(&path);
    }

    #[test]
    fn breaking_schema_rejects_old_serialized_world_state_before_mutation() {
        let path = test_database_path();
        let mut store = WorldStore::open(&path).expect("current database should open");
        store
            .initialize_world(54, [54; 32])
            .expect("world should initialize");
        store
            .write_snapshot(0, &WorldState::new(54, [54; 32]))
            .expect("snapshot should write");
        drop(store);

        let connection = Connection::open(&path).expect("fixture database should open");
        connection
            .execute(
                "DELETE FROM schema_migrations WHERE version = ?1",
                [SCHEMA_VERSION],
            )
            .expect("fixture should expose the previous schema marker");
        connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("fixture WAL should checkpoint");
        drop(connection);

        assert!(matches!(
            WorldStore::open(&path),
            Err(StoreError::UnsupportedSchema(53))
        ));
        let connection = Connection::open(&path).expect("rejected database should remain intact");
        assert_eq!(
            existing_schema_version(&connection).expect("schema should remain readable"),
            Some(53)
        );
        assert!(
            serialized_world_state_present(&connection)
                .expect("serialized state should remain present")
        );
        drop(connection);
        remove_database(&path);
    }

    #[test]
    fn on_disk_migration_publishes_verified_private_backup_first() {
        let directory = std::env::temp_dir().join(format!(
            "cdda-rust-migration-{}-{:016x}",
            std::process::id(),
            NEXT_TEST_DB.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("migration fixture directory should create");
        let path = directory.join("world.db");
        {
            let connection = Connection::open(&path).expect("schema-13 fixture should open");
            connection
                .execute_batch(
                    "CREATE TABLE schema_migrations(
                        version INTEGER PRIMARY KEY NOT NULL,
                        applied_utc TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                     );
                     INSERT INTO schema_migrations(version) VALUES (13);
                     CREATE TABLE world_metadata (
                        singleton INTEGER PRIMARY KEY NOT NULL,
                        world_namespace BLOB NOT NULL,
                        world_seed BLOB NOT NULL,
                        id_high_water BLOB NOT NULL
                     );
                     INSERT INTO world_metadata(
                        singleton, world_namespace, world_seed, id_high_water
                     ) VALUES (
                        1,
                        X'0000000000000036',
                        X'0000000000000000000000000000000000000000000000000000000000000000',
                        X'0000000000001000'
                     );
                     CREATE TABLE player_reports (
                        report_id INTEGER PRIMARY KEY AUTOINCREMENT,
                        created_utc INTEGER NOT NULL,
                        reporter_account_id BLOB NOT NULL,
                        reporter_actor_id BLOB NOT NULL,
                        reporter_character TEXT NOT NULL,
                        target_account_id BLOB NOT NULL,
                        target_actor_id BLOB NOT NULL,
                        target_character TEXT NOT NULL,
                        reason INTEGER NOT NULL,
                        details TEXT NOT NULL
                     );",
                )
                .expect("schema-13 report fixture should create");
        }
        let identity_bytes = [42_u8; 32];
        write_private_file(
            &directory.join(PRE_MIGRATION_IDENTITY_FILE),
            &identity_bytes,
        )
        .expect("protected identity fixture should write");

        let store = WorldStore::open(&path).expect("migration should succeed after backup");
        assert_eq!(
            existing_schema_version(&store.connection).expect("schema should query"),
            Some(SCHEMA_VERSION)
        );
        let report_columns = store
            .connection
            .prepare("PRAGMA table_info(player_reports)")
            .expect("report columns should prepare")
            .query_map([], |row| row.get::<_, String>(1))
            .expect("report columns should query")
            .collect::<Result<Vec<_>, _>>()
            .expect("report columns should decode");
        for expected in [
            "state",
            "resolved_utc",
            "resolved_by_account_id",
            "resolution_audit_sequence",
        ] {
            assert!(report_columns.iter().any(|column| column == expected));
        }
        let account_high_water: Vec<u8> = store
            .connection
            .query_row(
                "SELECT account_high_water FROM world_metadata WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .expect("account allocator should migrate");
        assert_eq!(
            decode_u64(&account_high_water).expect("allocator should decode"),
            4_096
        );
        let backup_directory = directory.join("pre-migration-backups");
        let backups = fs::read_dir(&backup_directory)
            .expect("backup directory should list")
            .map(|entry| entry.expect("backup entry should read").path())
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let backup_metadata = fs::symlink_metadata(&backups[0]).expect("backup should stat");
        assert!(backup_metadata.file_type().is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(backup_metadata.permissions().mode() & 0o077, 0);
        }
        let manifest = verify_pre_migration_backup_generation(&backups[0], 13)
            .expect("pre-migration generation should verify");
        assert_eq!(manifest.protected_members.len(), 1);
        assert_eq!(
            manifest.protected_members[0],
            PreMigrationBackupMemberV1 {
                filename: String::from(PRE_MIGRATION_IDENTITY_FILE),
                length: 32,
                checksum: *blake3::hash(&identity_bytes).as_bytes(),
            }
        );
        assert_eq!(
            fs::read(backups[0].join(PRE_MIGRATION_IDENTITY_FILE))
                .expect("protected identity should read"),
            identity_bytes
        );
        let backup = Connection::open_with_flags(
            backups[0].join(PRE_MIGRATION_DATABASE_FILE),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("pre-migration backup should reopen read-only");
        assert_eq!(
            existing_schema_version(&backup).expect("backup schema should query"),
            Some(13)
        );
        assert_eq!(
            backup
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .expect("backup integrity should query"),
            "ok"
        );
        drop(backup);
        fs::write(backups[0].join(PRE_MIGRATION_IDENTITY_FILE), [43_u8; 32])
            .expect("identity tamper fixture should write");
        assert!(verify_pre_migration_backup_generation(&backups[0], 13).is_err());
        drop(store);
        remove_database(&path);
        fs::remove_dir_all(&directory).expect("migration fixtures should remove");
    }

    #[test]
    fn snapshot_round_trip_verifies_the_canonical_hash() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(11, [8; 32])
            .expect("world should initialize");
        let block = store
            .reserve_id_block()
            .expect("reservation should succeed");
        let mut world = WorldState::new(11, [8; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("commandless tick should advance");
        let batch = JournalBatchV1 {
            ticks: vec![JournalTickV1 {
                tick: outcome.tick,
                commands: Vec::new(),
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events).expect("events should hash"),
                state_hash: outcome.canonical_hash,
            }],
            allocator_inputs: Vec::new(),
        };
        let sequence = store
            .append_journal_batch(&batch)
            .expect("journal should append");
        store
            .write_snapshot(sequence, &world)
            .expect("snapshot should write");
        let (loaded_sequence, loaded) = store
            .latest_snapshot()
            .expect("snapshot query should succeed")
            .expect("snapshot should exist");
        assert_eq!(loaded_sequence, sequence);
        assert_eq!(
            loaded.canonical_hash().expect("loaded hash"),
            world.canonical_hash().expect("original hash")
        );
    }

    #[test]
    fn in_progress_craft_recovers_and_replays_to_the_same_stable_output() {
        fn record_tick(world: &mut WorldState, commands: Vec<ClientCommand>) -> JournalTickV1 {
            let outcome = world
                .advance_tick(commands.clone())
                .expect("craft replay tick should advance");
            JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("craft events should hash"),
                state_hash: outcome.canonical_hash,
            }
        }

        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(35, [19; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(35, [19; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let actor_id = world
            .spawn_actor(position, true)
            .expect("actor should spawn");
        let spawn_component = |world: &mut WorldState, type_id: &str| {
            world
                .spawn_ground_item(cdda_sim::ItemSpawn {
                    position,
                    type_id: type_id.to_owned(),
                    charges: 1,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::new(),
                    ranged_weapon: None,
                })
                .expect("component should spawn")
        };
        let rock = spawn_component(&mut world, "rock");
        let socks = spawn_component(&mut world, "socks");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");

        let recipe = CraftRecipeV1 {
            recipe_id: String::from("rock_sock"),
            time_moves: 500,
            output_instances: 1,
            output: CraftItemPrototypeV1 {
                type_id: String::from("rock_sock"),
                charges: 1,
                melee_damage_milli: BTreeMap::from([(String::from("bash"), 8_000)]),
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
            retain_components: true,
            byproducts: vec![cdda_protocol::CraftByproductV1 {
                output_instances: 2,
                output: CraftItemPrototypeV1 {
                    type_id: String::from("splinter"),
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
            }],
            components: vec![
                vec![CraftComponentRequirementV1 {
                    type_id: String::from("rock"),
                    count: 1,
                    count_by_charges: false,
                    recoverable: true,
                }],
                vec![CraftComponentRequirementV1 {
                    type_id: String::from("socks"),
                    count: 1,
                    count_by_charges: false,
                    recoverable: true,
                }],
            ],
            tools: Vec::new(),
            qualities: Vec::new(),
            proficiencies: Vec::new(),
            primary_skill: None,
            required_skills: Vec::new(),
            can_be_learned: false,
            autolearn: true,
            autolearn_skills: Vec::new(),
            book_requirements: Vec::new(),
        };
        let mut before_snapshot = vec![
            record_tick(
                &mut world,
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(1),
                    client_tick: SimTick(0),
                    kind: CommandKind::PickUp { item_id: rock },
                }],
            ),
            record_tick(
                &mut world,
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(2),
                    client_tick: SimTick(1),
                    kind: CommandKind::PickUp { item_id: socks },
                }],
            ),
            record_tick(
                &mut world,
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(3),
                    client_tick: SimTick(2),
                    kind: CommandKind::Craft {
                        recipe_id: recipe.recipe_id.clone(),
                        recipe: Some(Box::new(recipe)),
                    },
                }],
            ),
        ];
        while world
            .actor_snapshot(actor_id)
            .and_then(|actor| actor.craft_activity)
            .is_none_or(|activity| activity.remaining_action_points > 6_000)
        {
            assert!(
                before_snapshot.len() < 100,
                "craft should start and progress"
            );
            before_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let mid_activity = world
            .actor_snapshot(actor_id)
            .expect("actor remains")
            .craft_activity
            .expect("craft is active at checkpoint");
        assert_eq!(mid_activity.remaining_action_points, 6_000);
        let output_ids = mid_activity.reserved_output_items.clone();
        assert_eq!(output_ids.len(), 3);
        let first_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: before_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("pre-checkpoint journal should append");
        store
            .write_snapshot(first_sequence, &world)
            .expect("in-progress craft snapshot should write");

        let mut after_snapshot = Vec::new();
        while world
            .actor_snapshot(actor_id)
            .is_some_and(|actor| actor.craft_activity.is_some())
        {
            assert!(
                after_snapshot.len() < 100,
                "craft should finish after recovery point"
            );
            after_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let final_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: after_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("post-checkpoint journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let actor = world.actor_snapshot(actor_id).expect("actor remains");
        assert_eq!(
            actor
                .inventory
                .iter()
                .map(|item| item.id)
                .collect::<Vec<ItemId>>(),
            output_ids
        );
        assert_eq!(
            actor
                .inventory
                .iter()
                .map(|item| item.type_id.as_str())
                .collect::<Vec<_>>(),
            vec!["rock_sock", "splinter", "splinter"]
        );
        assert_eq!(
            actor.inventory[0]
                .component_provenance
                .as_ref()
                .expect("reversible result retains exact components")
                .iter()
                .map(|component| component.type_id.as_str())
                .collect::<Vec<_>>(),
            vec!["rock", "socks"]
        );
        assert!(
            actor.inventory[1..]
                .iter()
                .all(|item| item.component_provenance.is_none()),
            "byproducts do not inherit the primary result's components"
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(35, [19; 32]))
            .expect("in-progress snapshot and tail should recover");
        assert_eq!(recovered_sequence, final_sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        let recovered_actor = recovered
            .actor_snapshot(actor_id)
            .expect("recovered actor exists");
        assert_eq!(
            recovered_actor
                .inventory
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            output_ids
        );
        assert_eq!(
            recovered_actor
                .inventory
                .iter()
                .map(|item| item.type_id.as_str())
                .collect::<Vec<_>>(),
            vec!["rock_sock", "splinter", "splinter"]
        );
        assert_eq!(
            recovered_actor.inventory[0].component_provenance,
            actor.inventory[0].component_provenance
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [29; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("craft replay should export")
            .verify(&content)
            .expect("self-contained craft replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replay hashes"),
            expected_hash
        );
    }

    #[test]
    fn installed_battery_identity_recovers_and_replays_exactly() {
        fn record_tick(world: &mut WorldState, commands: Vec<ClientCommand>) -> JournalTickV1 {
            let outcome = world
                .advance_tick(commands.clone())
                .expect("battery replay tick should advance");
            JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("battery events should hash"),
                state_hash: outcome.canonical_hash,
            }
        }

        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(39, [23; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(39, [23; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let actor_id = world
            .spawn_actor(position, false)
            .expect("actor should spawn");
        let tool_id = world
            .spawn_ground_item_with_powered_storage(
                cdda_sim::ItemSpawn {
                    position,
                    type_id: String::from("flashlight"),
                    charges: 0,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::new(),
                    ranged_weapon: None,
                },
                0,
                Some(MagazineWellPrototypeV1 {
                    pocket_index: 0,
                    pocket_id: String::new(),
                    compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
                    rigid: true,
                    unloadable: true,
                }),
                0,
                Some(PoweredToolStateV1 {
                    inactive_type_id: String::from("flashlight"),
                    active_type_id: String::from("flashlight_on"),
                    activation_charges: 1,
                    power_draw_milliwatts: 1_560,
                    light_emission: 300,
                    dims_with_charge: true,
                    power_pocket_index: 0,
                    active: false,
                }),
            )
            .expect("flashlight should spawn");
        let battery_id = world
            .spawn_ground_item_with_magazine_storage(
                cdda_sim::ItemSpawn {
                    position,
                    type_id: String::from("medium_battery_cell"),
                    charges: 37,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::from("battery"),
                    ranged_weapon: None,
                },
                56,
                None,
            )
            .expect("battery should spawn");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");

        let mut ticks = Vec::new();
        for (sequence, kind) in [
            CommandKind::PickUp { item_id: tool_id },
            CommandKind::PickUp {
                item_id: battery_id,
            },
            CommandKind::Wield { item_id: tool_id },
            CommandKind::Reload {
                ammunition_item: battery_id,
                target_pocket_index: Some(0),
            },
            CommandKind::Activate { item_id: tool_id },
        ]
        .into_iter()
        .enumerate()
        {
            let client_tick = world.tick();
            ticks.push(record_tick(
                &mut world,
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(u64::try_from(sequence + 1).expect("small index")),
                    client_tick,
                    kind,
                }],
            ));
            for _ in 0..19 {
                ticks.push(record_tick(&mut world, Vec::new()));
            }
        }
        let expected_hash = world.canonical_hash().expect("world should hash");
        let installed = world
            .actor_snapshot(actor_id)
            .expect("actor remains")
            .inventory
            .iter()
            .find(|item| item.id == tool_id)
            .and_then(|item| item.magazine_wells.first())
            .and_then(|well| well.installed_magazine.as_deref())
            .map(|battery| {
                (
                    battery.id,
                    battery.charges,
                    battery.residual_energy_millijoules,
                )
            });
        assert_eq!(installed, Some((battery_id, 35, 998_440)));
        assert!(
            world
                .actor_snapshot(actor_id)
                .expect("actor remains")
                .inventory
                .iter()
                .find(|item| item.id == tool_id)
                .and_then(|item| item.powered_tool.as_ref())
                .is_some_and(|powered| powered.active)
        );

        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks,
                allocator_inputs: Vec::new(),
            })
            .expect("battery journal should append");
        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(39, [23; 32]))
            .expect("battery journal should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered state hashes"),
            expected_hash
        );
        let recovered_battery = recovered
            .actor_snapshot(actor_id)
            .expect("actor should recover")
            .inventory
            .iter()
            .find(|item| item.id == tool_id)
            .and_then(|item| item.magazine_wells.first())
            .and_then(|well| well.installed_magazine.as_deref())
            .map(|battery| {
                (
                    battery.id,
                    battery.charges,
                    battery.residual_energy_millijoules,
                )
            });
        assert_eq!(recovered_battery, Some((battery_id, 35, 998_440)));

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [32; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("battery replay should export")
            .verify(&content)
            .expect("battery replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed state hashes"),
            expected_hash
        );
        let replayed_battery = replayed
            .actor_snapshot(actor_id)
            .expect("actor should replay")
            .inventory
            .iter()
            .find(|item| item.id == tool_id)
            .and_then(|item| item.magazine_wells.first())
            .and_then(|well| well.installed_magazine.as_deref())
            .map(|battery| {
                (
                    battery.id,
                    battery.charges,
                    battery.residual_energy_millijoules,
                )
            });
        assert_eq!(replayed_battery, Some((battery_id, 35, 998_440)));
    }

    #[test]
    fn disconnected_autopilot_recovery_and_portable_replay_choose_the_same_flight() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(41, [25; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(41, [25; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 0, y: 0, z: 0 }, true)
            .expect("connected actor should spawn");
        world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_slow_test"),
                position: WorldPosition { x: 1, y: 0, z: 0 },
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
            .expect("pre-flight snapshot should write");

        let connection_updates = vec![ActorConnectionUpdateV1 {
            actor_id,
            connected: false,
        }];
        let outcome = world
            .advance_tick_with_recovery_inputs(Vec::new(), Vec::new(), connection_updates.clone())
            .expect("disconnect boundary and autopilot tick should advance");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates,
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("autopilot events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("autopilot journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let expected_position = WorldPosition { x: 0, y: 1, z: 0 };
        assert_eq!(
            world
                .actor_snapshot(actor_id)
                .expect("actor should remain")
                .position,
            expected_position
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(41, [25; 32]))
            .expect("autopilot tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .actor_snapshot(actor_id)
                .expect("recovered actor should remain")
                .position,
            expected_position
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [34; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("autopilot replay should export")
            .verify(&content)
            .expect("autopilot replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .actor_snapshot(actor_id)
                .expect("replayed actor should remain")
                .position,
            expected_position
        );
    }

    #[test]
    fn creature_movement_debt_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(52, [42; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(52, [42; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let mut chunk = Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 });
        let wall = TerrainTileSnapshot {
            terrain_id: String::from("t_wall"),
            move_cost: 0,
            transparent: false,
            flat: false,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        for y in [0, 2] {
            chunk
                .set_terrain(LocalTileCoord { x: 3, y }, wall.clone())
                .expect("flanking wall should install");
        }
        chunk
            .set_furniture(
                LocalTileCoord { x: 3, y: 1 },
                Some(FurnitureTileSnapshot {
                    furniture_id: String::from("f_bed"),
                    move_cost_mod: 3,
                    transparent: true,
                    blocks_door: false,
                    comfort: 5,
                    floor_bedding_warmth: 1_000,
                }),
            )
            .expect("bed should install");
        world.insert_chunk(chunk);
        world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_zombie"),
                position: WorldPosition { x: 4, y: 1, z: 0 },
                hp: 80,
                speed: 2_000,
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
            .expect("zombie should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-movement snapshot should write");

        let outcome = world
            .advance_tick(Vec::new())
            .expect("terrain-costed movement should advance");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("movement events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("movement journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let creature = world
            .creature_snapshot(creature_id)
            .expect("creature should remain");
        assert_eq!(creature.position, WorldPosition { x: 3, y: 1, z: 0 });
        assert_eq!(creature.action_points, -1_500);
        assert!(creature.stumbles);
        assert_eq!(
            creature.goal,
            Some(WorldPosition { x: 1, y: 1, z: 0 }),
            "the acquired target destination is canonical persisted AI state"
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(52, [42; 32]))
            .expect("movement tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        let recovered_creature = recovered
            .creature_snapshot(creature_id)
            .expect("recovered creature should remain");
        assert_eq!(recovered_creature.action_points, -1_500);
        assert!(recovered_creature.stumbles);
        assert_eq!(
            recovered_creature.goal,
            Some(WorldPosition { x: 1, y: 1, z: 0 })
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [52; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("movement replay should export")
            .verify(&content)
            .expect("movement replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        let replayed_creature = replayed
            .creature_snapshot(creature_id)
            .expect("replayed creature should remain");
        assert_eq!(replayed_creature.action_points, -1_500);
        assert!(replayed_creature.stumbles);
        assert_eq!(
            replayed_creature.goal,
            Some(WorldPosition { x: 1, y: 1, z: 0 })
        );
    }

    #[test]
    fn sleeping_target_clumsy_miss_recovers_and_verifies_in_portable_replay() {
        let selected = (0..=u8::MAX).find_map(|seed_byte| {
            let mut world = WorldState::new(70, [seed_byte; 32]);
            world
                .install_reserved_block(
                    ReservedIdBlock::new(1, ID_RESERVATION_SIZE).expect("valid block"),
                )
                .expect("block should install");
            world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
            let actor_id = world
                .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
                .expect("actor should spawn");
            let mut sleeping_snapshot = world.snapshot();
            sleeping_snapshot.actors[0].sleeping = true;
            sleeping_snapshot.actors[0].sleep_intervals = 1;
            world = WorldState::from_snapshot(&sleeping_snapshot)
                .expect("canonical sleeping actor should restore");
            let creature_id = world
                .spawn_creature(CreatureSpawn {
                    type_id: String::from("mon_zombie"),
                    position: WorldPosition { x: 2, y: 1, z: 0 },
                    hp: 80,
                    speed: 2_000,
                    attack_cost_moves: 100,
                    aggression: 100,
                    melee_skill: 4,
                    dodge: 0,
                    size: Default::default(),
                    melee_dice: 2,
                    melee_dice_sides: 3,
                    can_see: true,
                    vision_day: 60,
                    vision_night: 60,
                    stumbles: true,
                    bashes: true,
                    group_bash: true,
                    hears: true,
                    good_hearing: false,
                    clumsy_attacks: true,
                    immobile: false,
                    pacifist: false,
                    can_open_doors: false,
                    path_settings: Default::default(),
                    blood_field_type_id: String::new(),
                    corpse: None,
                })
                .expect("classic zombie should spawn");
            let before_attack = world.clone();
            let outcome = world
                .advance_tick(Vec::new())
                .expect("candidate attack should resolve");
            outcome
                .events
                .iter()
                .any(|event| {
                    matches!(
                        event.kind,
                        WorldEventKind::CreatureMissedActor {
                            source,
                            target,
                            stumbled: true,
                            ..
                        } if source == creature_id && target == actor_id
                    )
                })
                .then_some((
                    seed_byte,
                    before_attack,
                    world,
                    outcome,
                    actor_id,
                    creature_id,
                ))
        });
        let (seed_byte, before_attack, world, outcome, actor_id, creature_id) =
            selected.expect("deterministic streams should contain a clumsy miss");

        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(70, [seed_byte; 32])
            .expect("world should initialize");
        assert_eq!(
            store.reserve_id_block().expect("block should reserve"),
            ReservedIdBlock::new(1, ID_RESERVATION_SIZE).expect("valid block")
        );
        store
            .write_snapshot(0, &before_attack)
            .expect("pre-attack snapshot should write");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("miss events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("miss journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let expected_creature = world
            .creature_snapshot(creature_id)
            .expect("creature should remain");
        assert!(expected_creature.clumsy_attacks);
        assert_eq!(
            expected_creature.downed_until_tick,
            Some(SimTick(outcome.tick.0 + 2 * SimTick::HZ))
        );
        let actor = world.actor_snapshot(actor_id).expect("actor should remain");
        assert!(actor.sleeping);
        assert_eq!(
            actor.hp,
            before_attack
                .actor_snapshot(actor_id)
                .expect("pre-attack actor should exist")
                .hp
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(70, [seed_byte; 32]))
            .expect("miss should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("recovered creature should remain"),
            expected_creature
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [70; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("miss replay should export")
            .verify(&content)
            .expect("miss replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed creature should remain"),
            expected_creature
        );
    }

    #[test]
    fn structural_bash_damage_and_capability_recover_and_verify_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(53, [43; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(53, [43; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let floor = TerrainTileSnapshot {
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
        };
        let mut damaged_door = floor.clone();
        damaged_door.terrain_id = String::from("t_door_b");
        damaged_door.move_cost = 0;
        world
            .register_terrain_bash_type(TerrainBashTypeV1 {
                terrain_id: String::from("t_door_c"),
                str_min: 8,
                str_max: 80,
                str_min_blocked: 15,
                str_max_blocked: 100,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: damaged_door,
                drop_source: None,
                hit_field: None,
                destroyed_field: None,
                sound: String::from("smash!"),
                failure_sound: String::from("whump!"),
                sound_volume: -1,
                failure_sound_volume: -1,
            })
            .expect("door bash should register");
        let mut chunk = Chunk::filled(ChunkCoord { x: 0, y: 0, z: 0 }, floor)
            .expect("floor chunk should build");
        let mut wall = TerrainTileSnapshot {
            terrain_id: String::from("t_wall"),
            move_cost: 0,
            transparent: false,
            flat: false,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        for x in 0..cdda_protocol::SUBMAP_SIZE as u8 {
            for y in [0, 2] {
                chunk
                    .set_terrain(LocalTileCoord { x, y }, wall.clone())
                    .expect("corridor should build");
            }
        }
        wall.terrain_id = String::from("t_door_c");
        wall.transparent = true;
        chunk
            .set_terrain(LocalTileCoord { x: 4, y: 1 }, wall)
            .expect("door should install");
        world.insert_chunk(chunk);
        world
            .spawn_actor(WorldPosition { x: 5, y: 1, z: 0 }, true)
            .expect("target should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_bashing_test"),
                position: WorldPosition { x: 3, y: 1, z: 0 },
                hp: 20,
                speed: 2_000,
                attack_cost_moves: 100,
                aggression: 100,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 10,
                melee_dice_sides: 1,
                can_see: true,
                vision_day: 60,
                vision_night: 60,
                stumbles: false,
                bashes: true,
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
            .expect("basher should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-bash snapshot should write");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("structural bash should advance");
        assert!(outcome.events.iter().any(|event| matches!(
            event.kind,
            WorldEventKind::CreatureBashed {
                creature_id: event_creature,
                damage: 2,
                accumulated_damage: 2,
                success: false,
                ..
            } if event_creature == creature_id
        )));
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("bash events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("bash journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let door_index =
            usize::from(1_u8) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(4_u8);
        assert_eq!(world.snapshot().chunks[0].map_damage[door_index], 2);

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(53, [43; 32]))
            .expect("bash tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered hash"),
            expected_hash
        );
        assert_eq!(recovered.snapshot().chunks[0].map_damage[door_index], 2);
        let recovered_creature = recovered
            .creature_snapshot(creature_id)
            .expect("basher should recover");
        assert!(recovered_creature.bashes);
        assert!(!recovered_creature.group_bash);

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [53; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("bash replay should export")
            .verify(&content)
            .expect("bash replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed hash"),
            expected_hash
        );
        assert_eq!(replayed.snapshot().chunks[0].map_damage[door_index], 2);
    }

    #[test]
    fn route_planned_bash_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(61, [61; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(61, [61; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let floor = TerrainTileSnapshot {
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
        };
        world
            .register_terrain_bash_type(TerrainBashTypeV1 {
                terrain_id: String::from("t_route_door"),
                str_min: 6,
                str_max: 26,
                str_min_blocked: -1,
                str_max_blocked: -1,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: floor.clone(),
                drop_source: None,
                hit_field: None,
                destroyed_field: None,
                sound: String::from("smash!"),
                failure_sound: String::from("whump!"),
                sound_volume: 20,
                failure_sound_volume: 12,
            })
            .expect("route door bash should register");
        let mut chunk =
            Chunk::filled(ChunkCoord { x: 0, y: 0, z: 0 }, floor).expect("chunk should build");
        let wall = TerrainTileSnapshot {
            terrain_id: String::from("t_route_wall"),
            move_cost: 0,
            transparent: true,
            flat: false,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        for y in 0..=10 {
            chunk
                .set_terrain(LocalTileCoord { x: 4, y }, wall.clone())
                .expect("barrier should install");
        }
        chunk
            .set_terrain(
                LocalTileCoord { x: 4, y: 7 },
                TerrainTileSnapshot {
                    terrain_id: String::from("t_route_door"),
                    ..wall
                },
            )
            .expect("route door should install");
        world.insert_chunk(chunk);
        world
            .spawn_actor(WorldPosition { x: 5, y: 5, z: 0 }, true)
            .expect("target should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_route_basher"),
                position: WorldPosition { x: 3, y: 5, z: 0 },
                hp: 20,
                speed: 2_000,
                attack_cost_moves: 100,
                aggression: 100,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 10,
                melee_dice_sides: 10,
                can_see: true,
                vision_day: 60,
                vision_night: 60,
                stumbles: false,
                bashes: true,
                group_bash: false,
                hears: false,
                good_hearing: false,
                clumsy_attacks: false,
                immobile: false,
                pacifist: false,
                can_open_doors: false,
                path_settings: cdda_protocol::CreaturePathSettingsV1 {
                    max_distance: 20,
                    ..cdda_protocol::CreaturePathSettingsV1::default()
                },
                blood_field_type_id: String::new(),
                corpse: None,
            })
            .expect("route basher should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-route snapshot should write");

        let mut ticks = Vec::new();
        let mut outcomes = Vec::new();
        for _ in 0..3 {
            let outcome = world
                .advance_tick(Vec::new())
                .expect("route bash tick should advance");
            ticks.push(JournalTickV1 {
                tick: outcome.tick,
                commands: Vec::new(),
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("route events should hash"),
                state_hash: outcome.canonical_hash,
            });
            outcomes.push(outcome);
        }
        assert!(matches!(
            outcomes[0].events.as_slice(),
            [WorldEvent {
                kind: WorldEventKind::CreatureMoved {
                    creature_id: event_creature,
                    from: WorldPosition { x: 3, y: 5, z: 0 },
                    to: WorldPosition { x: 3, y: 6, z: 0 },
                },
                ..
            }] if *event_creature == creature_id
        ));
        assert!(
            outcomes
                .iter()
                .skip(1)
                .any(|outcome| outcome.events.iter().any(|event| matches!(
                    &event.kind,
                    WorldEventKind::CreatureBashed {
                        creature_id: event_creature,
                        target: WorldPosition { x: 4, y: 7, z: 0 },
                        target_type_id,
                        success: true,
                        ..
                    } if *event_creature == creature_id && target_type_id == "t_route_door"
                )))
        );
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks,
                allocator_inputs: Vec::new(),
            })
            .expect("route journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let door_index =
            usize::from(7_u8) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(4_u8);
        assert_eq!(
            world.snapshot().chunks[0].tiles[door_index].terrain_id,
            "t_floor"
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(61, [61; 32]))
            .expect("route ticks should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered hash"),
            expected_hash
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("route basher should recover")
                .path_settings
                .max_distance,
            20
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [61; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("route replay should export")
            .verify(&content)
            .expect("route replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed hash"),
            expected_hash
        );
        assert_eq!(
            replayed.snapshot().chunks[0].tiles[door_index].terrain_id,
            "t_floor"
        );
    }

    #[test]
    fn furniture_bash_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(59, [59; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(59, [59; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world
            .register_furniture_bash_type(FurnitureBashTypeV1 {
                furniture_id: String::from("f_dresser"),
                str_min: 12,
                str_max: 40,
                str_min_blocked: -1,
                str_max_blocked: -1,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: None,
                drop_source: None,
                hit_field: None,
                destroyed_field: None,
                sound: String::from("smash!"),
                failure_sound: String::from("whump."),
                sound_volume: -1,
                failure_sound_volume: -1,
            })
            .expect("dresser bash should register");
        let mut chunk = Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 });
        chunk
            .set_furniture(
                LocalTileCoord { x: 4, y: 1 },
                Some(FurnitureTileSnapshot {
                    furniture_id: String::from("f_dresser"),
                    move_cost_mod: -1,
                    transparent: true,
                    blocks_door: true,
                    comfort: 0,
                    floor_bedding_warmth: 0,
                }),
            )
            .expect("dresser should install");
        world.insert_chunk(chunk);
        world
            .spawn_actor(WorldPosition { x: 5, y: 1, z: 0 }, true)
            .expect("target should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_furniture_basher"),
                position: WorldPosition { x: 3, y: 1, z: 0 },
                hp: 20,
                speed: 2_000,
                attack_cost_moves: 100,
                aggression: 100,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 10,
                melee_dice_sides: 10,
                can_see: true,
                vision_day: 60,
                vision_night: 60,
                stumbles: false,
                bashes: true,
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
            .expect("basher should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-bash snapshot should write");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("furniture bash should advance");
        assert!(outcome.events.iter().any(|event| matches!(
            &event.kind,
            WorldEventKind::CreatureBashed {
                creature_id: event_creature,
                target_kind: BashTargetKindV1::Furniture,
                target_type_id,
                success: true,
                ..
            } if *event_creature == creature_id && target_type_id == "f_dresser"
        )));
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("furniture-bash events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("furniture-bash journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let dresser_index =
            usize::from(1_u8) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(4_u8);
        assert!(world.snapshot().chunks[0].furniture[dresser_index].is_none());

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(59, [59; 32]))
            .expect("furniture-bash tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered hash"),
            expected_hash
        );
        assert!(recovered.snapshot().chunks[0].furniture[dresser_index].is_none());

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [59; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("furniture-bash replay should export")
            .verify(&content)
            .expect("furniture-bash replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed hash"),
            expected_hash
        );
        assert!(replayed.snapshot().chunks[0].furniture[dresser_index].is_none());
    }

    #[test]
    fn actor_smash_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(62, [62; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(62, [62; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let floor = TerrainTileSnapshot {
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
        };
        world
            .register_terrain_bash_type(TerrainBashTypeV1 {
                terrain_id: String::from("t_actor_door"),
                str_min: 8,
                str_max: 9,
                str_min_blocked: -1,
                str_max_blocked: -1,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: floor.clone(),
                drop_source: None,
                hit_field: None,
                destroyed_field: None,
                sound: String::from("crash!"),
                failure_sound: String::from("thump!"),
                sound_volume: 15,
                failure_sound_volume: 12,
            })
            .expect("actor door bash should register");
        world
            .register_smash_item_type(cdda_protocol::SmashItemTypeV1 {
                item_type_id: String::from("hammer"),
                bash_damage: 9,
                attack_time_moves: 79,
                melee_to_hit: -1,
            })
            .expect("hammer smash profile should register");
        let mut chunk =
            Chunk::filled(ChunkCoord { x: 0, y: 0, z: 0 }, floor).expect("chunk should build");
        chunk
            .set_terrain(
                LocalTileCoord { x: 2, y: 2 },
                TerrainTileSnapshot {
                    terrain_id: String::from("t_actor_door"),
                    move_cost: 0,
                    transparent: false,
                    flat: false,
                    open: String::new(),
                    open_move_cost: None,
                    open_transparent: None,
                    open_flat: None,
                    close: String::new(),
                    close_move_cost: None,
                    close_transparent: None,
                    close_flat: None,
                },
            )
            .expect("actor door should install");
        world.insert_chunk(chunk);
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let hammer_id = world
            .spawn_ground_item(ItemSpawn {
                position: WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("hammer"),
                charges: 1,
                melee_damage_milli: BTreeMap::from([(String::from("bash"), 9_000)]),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("hammer should spawn");
        for (sequence, kind) in [
            CommandKind::PickUp { item_id: hammer_id },
            CommandKind::Wield { item_id: hammer_id },
        ]
        .into_iter()
        .enumerate()
        {
            world
                .advance_tick(vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(sequence as u64 + 1),
                    client_tick: world.tick(),
                    kind,
                }])
                .expect("hammer setup should queue");
            for _ in 0..20 {
                let actor = world.actor_snapshot(actor_id).expect("actor remains");
                let complete = if sequence == 0 {
                    actor.inventory.iter().any(|item| item.id == hammer_id)
                } else {
                    actor.wielded == Some(hammer_id)
                };
                if complete {
                    break;
                }
                world
                    .advance_tick(Vec::new())
                    .expect("hammer setup should advance");
            }
        }
        assert_eq!(
            world
                .actor_snapshot(actor_id)
                .expect("actor remains")
                .wielded,
            Some(hammer_id)
        );
        store
            .write_snapshot(0, &world)
            .expect("pre-smash snapshot should write");

        let smash = ClientCommand {
            actor_id,
            sequence: CommandSequence(3),
            client_tick: world.tick(),
            kind: CommandKind::Smash { dx: 1, dy: 1 },
        };
        let mut ticks = Vec::new();
        let mut smashed = false;
        for attempt in 0..20 {
            let commands = if attempt == 0 {
                vec![smash.clone()]
            } else {
                Vec::new()
            };
            let outcome = world
                .advance_tick(commands.clone())
                .expect("smash tick should advance");
            smashed |= outcome.events.iter().any(|event| {
                matches!(
                    &event.kind,
                    WorldEventKind::ActorBashed {
                        actor_id: event_actor,
                        target: WorldPosition { x: 2, y: 2, z: 0 },
                        target_kind: BashTargetKindV1::Terrain,
                        target_type_id,
                        success: true,
                        damage: 9,
                        ..
                    } if *event_actor == actor_id && target_type_id == "t_actor_door"
                )
            });
            ticks.push(JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("actor smash events should hash"),
                state_hash: outcome.canonical_hash,
            });
            if smashed {
                break;
            }
        }
        assert!(smashed, "queued actor smash should execute within one turn");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks,
                allocator_inputs: Vec::new(),
            })
            .expect("actor-smash journal should append");
        let expected_hash = world.canonical_hash().expect("live hash");
        let door_index =
            usize::from(2_u8) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(2_u8);
        assert_eq!(
            world.snapshot().chunks[0].tiles[door_index].terrain_id,
            "t_floor"
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(62, [62; 32]))
            .expect("actor smash should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered hash"),
            expected_hash
        );
        assert_eq!(
            recovered.snapshot().chunks[0].tiles[door_index].terrain_id,
            "t_floor"
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [62; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("actor smash replay should export")
            .verify(&content)
            .expect("actor smash replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed hash"),
            expected_hash
        );
        assert_eq!(
            replayed.snapshot().chunks[0].tiles[door_index].terrain_id,
            "t_floor"
        );
    }

    #[test]
    fn gunfire_hearing_goal_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(56, [56; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(56, [56; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let weapon_id = world
            .spawn_ground_item(ItemSpawn {
                position: WorldPosition { x: 1, y: 1, z: 0 },
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
                    damage: 1,
                    dispersion: 0,
                    sound_volume: 70,
                }),
            })
            .expect("weapon should spawn");
        let target_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_target"),
                position: WorldPosition { x: 2, y: 1, z: 0 },
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
            .expect("target should spawn");
        let listener_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_listener"),
                position: WorldPosition { x: 8, y: 1, z: 0 },
                hp: 20,
                speed: 2_000,
                attack_cost_moves: 100,
                aggression: 100,
                melee_skill: 0,
                dodge: 0,
                size: Default::default(),
                melee_dice: 1,
                melee_dice_sides: 1,
                can_see: false,
                vision_day: 60,
                vision_night: 60,
                stumbles: false,
                bashes: false,
                group_bash: false,
                hears: true,
                good_hearing: false,
                clumsy_attacks: false,
                immobile: false,
                pacifist: false,
                can_open_doors: false,
                path_settings: Default::default(),
                blood_field_type_id: String::new(),
                corpse: None,
            })
            .expect("listener should spawn");

        let pickup = ClientCommand {
            actor_id,
            sequence: CommandSequence(1),
            client_tick: world.tick(),
            kind: CommandKind::PickUp { item_id: weapon_id },
        };
        world
            .advance_tick(vec![pickup])
            .expect("pickup should queue");
        for _ in 0..20 {
            if world
                .actor_snapshot(actor_id)
                .expect("actor should remain")
                .inventory
                .iter()
                .any(|item| item.id == weapon_id)
            {
                break;
            }
            world
                .advance_tick(Vec::new())
                .expect("pickup preparation should advance");
        }
        assert!(
            world
                .actor_snapshot(actor_id)
                .expect("actor should remain")
                .inventory
                .iter()
                .any(|item| item.id == weapon_id),
            "weapon pickup must finish before the replay snapshot"
        );
        let wield = ClientCommand {
            actor_id,
            sequence: CommandSequence(2),
            client_tick: world.tick(),
            kind: CommandKind::Wield { item_id: weapon_id },
        };
        world.advance_tick(vec![wield]).expect("wield should queue");
        for _ in 0..20 {
            if world
                .actor_snapshot(actor_id)
                .expect("actor should remain")
                .wielded
                == Some(weapon_id)
            {
                break;
            }
            world
                .advance_tick(Vec::new())
                .expect("wield preparation should advance");
        }
        assert_eq!(
            world
                .actor_snapshot(actor_id)
                .expect("actor should remain")
                .wielded,
            Some(weapon_id)
        );
        store
            .write_snapshot(0, &world)
            .expect("pre-shot snapshot should write");

        let mut ticks = Vec::new();
        let mut shot_seen = false;
        for attempt in 0..20 {
            let commands = if attempt == 0 {
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(3),
                    client_tick: world.tick(),
                    kind: CommandKind::ShootCreature { target: target_id },
                }]
            } else {
                Vec::new()
            };
            let outcome = world
                .advance_tick(commands.clone())
                .expect("shot replay tick should advance");
            shot_seen |= outcome.events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::RangedAttackResolved {
                        sound_volume: 70,
                        ..
                    }
                )
            });
            ticks.push(JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("shot events should hash"),
                state_hash: outcome.canonical_hash,
            });
            if shot_seen {
                break;
            }
        }
        assert!(
            shot_seen,
            "queued shot should execute within the bounded fixture"
        );
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks,
                allocator_inputs: Vec::new(),
            })
            .expect("shot journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let expected_listener = world
            .creature_snapshot(listener_id)
            .expect("listener should remain");
        assert!(expected_listener.position.x < 8);
        assert_eq!(
            expected_listener
                .sound_goal
                .expect("gunfire should create a canonical private goal")
                .remaining_actions,
            62
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(56, [56; 32]))
            .expect("gunfire tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .creature_snapshot(listener_id)
                .expect("listener should recover"),
            expected_listener
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [56; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("gunfire replay should export")
            .verify(&content)
            .expect("gunfire replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .creature_snapshot(listener_id)
                .expect("listener should replay"),
            expected_listener
        );
    }

    #[test]
    fn creature_door_opening_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(57, [57; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(57, [57; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let mut chunk = Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 });
        let wall = TerrainTileSnapshot {
            terrain_id: String::from("t_wall"),
            move_cost: 0,
            transparent: false,
            flat: false,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        for x in 0..cdda_protocol::SUBMAP_SIZE as u8 {
            for y in [0, 2] {
                chunk
                    .set_terrain(LocalTileCoord { x, y }, wall.clone())
                    .expect("corridor wall should install");
            }
        }
        chunk
            .set_terrain(
                LocalTileCoord { x: 4, y: 1 },
                TerrainTileSnapshot {
                    terrain_id: String::from("t_door_c"),
                    move_cost: 0,
                    transparent: true,
                    flat: false,
                    open: String::from("t_door_o"),
                    open_move_cost: Some(2),
                    open_transparent: Some(true),
                    open_flat: Some(true),
                    close: String::new(),
                    close_move_cost: None,
                    close_transparent: None,
                    close_flat: None,
                },
            )
            .expect("closed door should install");
        world.insert_chunk(chunk);
        world
            .spawn_actor(WorldPosition { x: 5, y: 1, z: 0 }, true)
            .expect("target should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_door_opener"),
                position: WorldPosition { x: 3, y: 1, z: 0 },
                hp: 20,
                speed: 2_000,
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
                hears: true,
                good_hearing: false,
                clumsy_attacks: false,
                immobile: false,
                pacifist: false,
                can_open_doors: true,
                path_settings: cdda_protocol::CreaturePathSettingsV1 {
                    max_distance: 45,
                    allow_open_doors: true,
                    avoid_traps: true,
                    avoid_sharp: true,
                    ..cdda_protocol::CreaturePathSettingsV1::default()
                },
                blood_field_type_id: String::new(),
                corpse: None,
            })
            .expect("door opener should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-open snapshot should write");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("door-opening tick should advance");
        assert!(matches!(
            outcome.events.first().map(|event| &event.kind),
            Some(WorldEventKind::CreatureOpenedTerrain {
                creature_id: event_creature,
                from,
                to,
                sound,
                volume: 6,
                ..
            }) if *event_creature == creature_id
                && from == "t_door_c"
                && to == "t_door_o"
                && sound == "swish"
        ));
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("door-opening events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("door-opening journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let expected_creature = world
            .creature_snapshot(creature_id)
            .expect("door opener should remain");
        assert!(expected_creature.can_open_doors);
        assert_eq!(expected_creature.path_settings.max_distance, 45);
        assert!(expected_creature.path_settings.allow_open_doors);
        assert_eq!(
            expected_creature.position,
            WorldPosition { x: 4, y: 1, z: 0 }
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(57, [57; 32]))
            .expect("door-opening tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("door opener should recover"),
            expected_creature
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [57; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("door-opening replay should export")
            .verify(&content)
            .expect("door-opening replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .creature_snapshot(creature_id)
                .expect("door opener should replay"),
            expected_creature
        );
    }

    #[test]
    fn creature_blood_field_recovers_and_verifies_in_portable_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(49, [39; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(49, [39; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world
            .register_field_type(FieldTypeSnapshotV1 {
                field_type_id: String::from("fd_blood"),
                intensity_levels: vec![FieldIntensityLevelV1 {
                    name: String::from("blood splatter"),
                    symbol: String::from("%"),
                    color: String::from("red"),
                    dangerous: false,
                    transparent: true,
                }],
                priority: 0,
                half_life_seconds: 2 * 24 * 60 * 60,
                linear_half_life: false,
                is_splattering: true,
                display_field: true,
            })
            .expect("blood field should register");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let death_position = WorldPosition { x: 2, y: 1, z: 0 };
        let corpse_prototype = cdda_protocol::CreatureCorpsePrototypeV1 {
            monster_type_id: String::from("mon_test"),
            max_hp: 8,
            speed: 1,
            attack_cost_moves: 100,
            aggression: 0,
            melee_skill: 4,
            dodge: 2,
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
            blood_field_type_id: String::from("fd_blood"),
            revives: true,
        };
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_test"),
                position: death_position,
                hp: 8,
                speed: 1,
                attack_cost_moves: 100,
                aggression: 0,
                melee_skill: corpse_prototype.melee_skill,
                dodge: corpse_prototype.dodge,
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
                blood_field_type_id: String::from("fd_blood"),
                corpse: Some(corpse_prototype.clone()),
            })
            .expect("creature should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-death snapshot should write");
        let command = (1..=1_000)
            .find_map(|sequence| {
                let command = ClientCommand {
                    actor_id,
                    sequence: CommandSequence(sequence),
                    client_tick: SimTick(0),
                    kind: CommandKind::AttackCreature {
                        target: creature_id,
                    },
                };
                let mut candidate = world.clone();
                let events = candidate
                    .advance_tick(vec![command.clone()])
                    .expect("candidate fatal attack should resolve")
                    .events;
                let remaining_hp = events.iter().find_map(|event| match event.kind {
                    WorldEventKind::CreatureDamaged {
                        target,
                        remaining_hp,
                        ..
                    } if target == creature_id && remaining_hp <= 0 => Some(remaining_hp),
                    _ => None,
                })?;
                let raw_damage = u16::try_from(
                    (i64::from(remaining_hp).checked_neg()? * 2_500
                        / i64::from(corpse_prototype.max_hp))
                    .min(4_000),
                )
                .ok()?;
                (raw_damage > 0
                    && raw_damage < cdda_protocol::MAX_ITEM_RAW_DAMAGE
                    && cdda_protocol::minimum_raw_damage_for_level(
                        cdda_protocol::item_damage_level(raw_damage),
                    ) != Some(raw_damage))
                .then_some(command)
            })
            .expect("named stream should contain a non-boundary fatal hit");
        let outcome = world
            .advance_tick(vec![command.clone()])
            .expect("fatal attack should advance");
        assert!(outcome.events.iter().any(|event| matches!(
            &event.kind,
            WorldEventKind::FieldIntensityChanged {
                position,
                field_type_id,
                intensity: 1,
            } if *position == death_position && field_type_id == "fd_blood"
        )));
        let corpse_item_id = outcome
            .events
            .iter()
            .find_map(|event| match event.kind {
                WorldEventKind::CreatureCorpseCreated { corpse_item_id, .. } => {
                    Some(corpse_item_id)
                }
                _ => None,
            })
            .expect("death should create a corpse");
        let remaining_hp = outcome
            .events
            .iter()
            .find_map(|event| match event.kind {
                WorldEventKind::CreatureDamaged {
                    target,
                    remaining_hp,
                    ..
                } if target == creature_id => Some(remaining_hp),
                _ => None,
            })
            .expect("fatal damage should be recorded");
        let expected_raw_damage = u16::try_from(
            (i64::from(remaining_hp).abs() * 2_500 / i64::from(corpse_prototype.max_hp)).min(4_000),
        )
        .expect("bounded corpse damage");
        let expected_condition = (
            expected_raw_damage,
            cdda_protocol::item_damage_level(expected_raw_damage),
        );
        assert_eq!(
            world
                .ground_item_snapshot(corpse_item_id)
                .map(|ground| (ground.item.raw_damage, ground.item.damage)),
            Some(expected_condition)
        );
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: vec![command],
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("field events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("field journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(49, [39; 32]))
            .expect("field tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .fields_at(death_position)
                .and_then(|fields| fields.first())
                .map(|field| (field.field_type_id.as_str(), field.intensity)),
            Some(("fd_blood", 1))
        );
        assert_eq!(
            recovered
                .ground_item_snapshot(corpse_item_id)
                .and_then(|ground| ground.item.creature_corpse)
                .map(|corpse| corpse.prototype),
            Some(corpse_prototype.clone())
        );
        assert_eq!(
            recovered
                .ground_item_snapshot(corpse_item_id)
                .map(|ground| (ground.item.raw_damage, ground.item.damage)),
            Some(expected_condition)
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [49; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("field replay should export")
            .verify(&content)
            .expect("field replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .fields_at(death_position)
                .and_then(|fields| fields.first())
                .map(|field| (field.field_type_id.as_str(), field.intensity)),
            Some(("fd_blood", 1))
        );
        assert_eq!(
            replayed
                .ground_item_snapshot(corpse_item_id)
                .and_then(|ground| ground.item.creature_corpse)
                .map(|corpse| corpse.prototype),
            Some(corpse_prototype)
        );
        assert_eq!(
            replayed
                .ground_item_snapshot(corpse_item_id)
                .map(|ground| (ground.item.raw_damage, ground.item.damage)),
            Some(expected_condition)
        );
    }

    #[test]
    fn trapped_autopilot_defense_recovers_and_replays_the_same_melee_hit() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(42, [26; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(42, [26; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let mut chunk = Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 });
        let wall = TerrainTileSnapshot {
            terrain_id: String::from("t_wall"),
            move_cost: 0,
            transparent: false,
            flat: false,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        for (x, y) in [(0, 0), (1, 0), (2, 0), (0, 1), (0, 2), (1, 2), (2, 2)] {
            chunk
                .set_terrain(LocalTileCoord { x, y }, wall.clone())
                .expect("wall should install");
        }
        world.insert_chunk(chunk);
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("connected actor should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_slow_test"),
                position: WorldPosition { x: 2, y: 1, z: 0 },
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
            .expect("adjacent hostile creature should spawn");
        store
            .write_snapshot(0, &world)
            .expect("pre-defense snapshot should write");

        let connection_updates = vec![ActorConnectionUpdateV1 {
            actor_id,
            connected: false,
        }];
        let outcome = world
            .advance_tick_with_recovery_inputs(Vec::new(), Vec::new(), connection_updates.clone())
            .expect("disconnect boundary and defense should advance");
        assert!(matches!(
            outcome.events.as_slice(),
            [WorldEvent {
                kind: WorldEventKind::CreatureDamaged {
                    source,
                    target,
                    amount: 10,
                    remaining_hp: 10,
                },
                ..
            }] if *source == actor_id && *target == creature_id
        ));
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates,
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("defense events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("defense journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(42, [26; 32]))
            .expect("defense tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("recovered creature should remain")
                .hp,
            10
        );
        assert_eq!(
            recovered
                .actor_snapshot(actor_id)
                .expect("recovered defender should remain")
                .action_points,
            800
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [35; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("defense replay should export")
            .verify(&content)
            .expect("defense replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed creature should remain")
                .hp,
            10
        );
        assert_eq!(
            replayed
                .actor_snapshot(actor_id)
                .expect("replayed defender should remain")
                .action_points,
            800
        );
    }

    #[test]
    fn unarmed_immobile_pacifist_tiny_monster_private_state_recovers_and_replays() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(66, [66; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(66, [66; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_arbitrary_immobile_target"),
                position: WorldPosition { x: 2, y: 1, z: 0 },
                hp: 20,
                speed: 1,
                attack_cost_moves: 37,
                aggression: 0,
                melee_skill: 4,
                dodge: 0,
                size: cdda_protocol::CreatureSizeV1::Tiny,
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
                immobile: true,
                pacifist: true,
                can_open_doors: false,
                path_settings: Default::default(),
                blood_field_type_id: String::new(),
                corpse: None,
            })
            .expect("arbitrary immobile monster should spawn");
        let mut ready = world.snapshot();
        ready
            .actors
            .iter_mut()
            .find(|actor| actor.id == actor_id)
            .expect("actor should be canonical")
            .speed = 2_000;
        world = WorldState::from_snapshot(&ready).expect("ready actor should restore");
        store
            .write_snapshot(0, &world)
            .expect("pre-miss snapshot should write");

        let (command, outcome, missed_world) = (1..=1_000)
            .find_map(|sequence| {
                let command = ClientCommand {
                    actor_id,
                    sequence: CommandSequence(sequence),
                    client_tick: world.tick(),
                    kind: CommandKind::AttackCreature {
                        target: creature_id,
                    },
                };
                let mut candidate = world.clone();
                let outcome = candidate
                    .advance_tick(vec![command.clone()])
                    .expect("candidate attack should resolve");
                outcome
                    .events
                    .iter()
                    .any(|event| {
                        matches!(
                            event.kind,
                            WorldEventKind::ActorMissedCreature { source, target }
                                if source == actor_id && target == creature_id
                        )
                    })
                    .then_some((command, outcome, candidate))
            })
            .expect("named stream should provide a deterministic miss");
        let expected_hash = missed_world
            .canonical_hash()
            .expect("missed world should hash");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: vec![command],
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("miss event should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("miss journal should append");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(66, [66; 32]))
            .expect("miss should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("missed immobile monster should remain")
                .hp,
            20
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("missed immobile monster should remain")
                .size,
            cdda_protocol::CreatureSizeV1::Tiny
        );
        assert!(
            recovered
                .creature_snapshot(creature_id)
                .expect("missed immobile monster should remain")
                .immobile
        );
        assert!(
            recovered
                .creature_snapshot(creature_id)
                .expect("missed pacifist monster should remain")
                .pacifist
        );
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("variable-cost monster should remain")
                .attack_cost_moves,
            37
        );
        assert_eq!(
            recovered
                .actor_snapshot(actor_id)
                .expect("attacker should remain")
                .action_points,
            800
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [66; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("miss replay should export")
            .verify(&content)
            .expect("miss replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed missed immobile monster should remain")
                .hp,
            20
        );
        assert_eq!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed missed immobile monster should remain")
                .size,
            cdda_protocol::CreatureSizeV1::Tiny
        );
        assert!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed immobile monster should remain")
                .immobile
        );
        assert!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed pacifist monster should remain")
                .pacifist
        );
        assert_eq!(
            replayed
                .creature_snapshot(creature_id)
                .expect("replayed variable-cost monster should remain")
                .attack_cost_moves,
            37
        );
    }

    #[test]
    fn strict_hammer_miss_retains_canonical_accuracy_through_recovery_and_replay() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(69, [69; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(69, [69; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        world
            .register_smash_item_type(cdda_protocol::SmashItemTypeV1 {
                item_type_id: String::from("hammer"),
                bash_damage: 9,
                attack_time_moves: 79,
                melee_to_hit: -1,
            })
            .expect("hammer profile should register");
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let hammer_id = world
            .spawn_ground_item(ItemSpawn {
                position: WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("hammer"),
                charges: 1,
                melee_damage_milli: BTreeMap::from([(String::from("bash"), 9_000)]),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("hammer should spawn");
        let creature_id = world
            .spawn_creature(CreatureSpawn {
                type_id: String::from("mon_zombie"),
                position: WorldPosition { x: 2, y: 1, z: 0 },
                hp: 20,
                speed: 1,
                attack_cost_moves: 100,
                aggression: 0,
                melee_skill: 4,
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
                blood_field_type_id: String::new(),
                corpse: None,
            })
            .expect("classic zombie should spawn");
        let mut ready = world.snapshot();
        ready
            .actors
            .iter_mut()
            .find(|actor| actor.id == actor_id)
            .expect("actor should be canonical")
            .speed = 2_000;
        world = WorldState::from_snapshot(&ready).expect("ready actor should restore");
        for (sequence, kind) in [
            (
                CommandSequence(1),
                CommandKind::PickUp { item_id: hammer_id },
            ),
            (
                CommandSequence(2),
                CommandKind::Wield { item_id: hammer_id },
            ),
        ] {
            let client_tick = world.tick();
            world
                .advance_tick(vec![ClientCommand {
                    actor_id,
                    sequence,
                    client_tick,
                    kind,
                }])
                .expect("hammer setup should resolve");
        }
        store
            .write_snapshot(0, &world)
            .expect("armed pre-miss snapshot should write");

        let (command, outcome, missed_world) = (3..=1_000)
            .find_map(|sequence| {
                let command = ClientCommand {
                    actor_id,
                    sequence: CommandSequence(sequence),
                    client_tick: world.tick(),
                    kind: CommandKind::AttackCreature {
                        target: creature_id,
                    },
                };
                let mut candidate = world.clone();
                let outcome = candidate
                    .advance_tick(vec![command.clone()])
                    .expect("candidate hammer attack should resolve");
                outcome
                    .events
                    .iter()
                    .any(|event| {
                        matches!(
                            event.kind,
                            WorldEventKind::ActorMissedCreature { source, target }
                                if source == actor_id && target == creature_id
                        )
                    })
                    .then_some((command, outcome, candidate))
            })
            .expect("named hammer stream should provide a deterministic miss");
        let expected_hash = missed_world
            .canonical_hash()
            .expect("armed missed world should hash");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: vec![command],
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("armed miss event should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("armed miss journal should append");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(69, [69; 32]))
            .expect("armed miss should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(recovered.snapshot().smash_item_types[0].melee_to_hit, -1);
        assert_eq!(
            recovered
                .creature_snapshot(creature_id)
                .expect("missed zombie should remain")
                .hp,
            20
        );
        assert_eq!(
            recovered
                .actor_snapshot(actor_id)
                .expect("hammer attacker should remain")
                .action_points,
            520
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [69; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("armed miss replay should export")
            .verify(&content)
            .expect("armed miss replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(replayed.snapshot().smash_item_types[0].melee_to_hit, -1);
    }

    #[test]
    fn emergency_autopilot_drink_recovers_and_replays_the_same_stable_item() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(43, [27; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(43, [27; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("connected actor should spawn");
        let drink_id = world
            .spawn_ground_item(ItemSpawn {
                position: WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("ordinary_water"),
                charges: 2,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 100,
                comestible_type: String::from("DRINK"),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("drink should spawn");
        world
            .advance_tick(vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(1),
                client_tick: world.tick(),
                kind: CommandKind::PickUp { item_id: drink_id },
            }])
            .expect("drink should be picked up");
        let mut snapshot = world.snapshot();
        let actor = snapshot
            .actors
            .iter_mut()
            .find(|actor| actor.id == actor_id)
            .expect("actor should snapshot");
        actor.connected = true;
        actor.action_points = i64::from(cdda_protocol::ACTION_POINT_THRESHOLD);
        actor.stored_kcal = 0;
        actor.thirst = cdda_sim::THIRST_DEATH_THRESHOLD;
        let mut world = WorldState::from_snapshot(&snapshot).expect("fixture should restore");
        store
            .write_snapshot(0, &world)
            .expect("pre-drink snapshot should write");

        let connection_updates = vec![ActorConnectionUpdateV1 {
            actor_id,
            connected: false,
        }];
        let outcome = world
            .advance_tick_with_recovery_inputs(Vec::new(), Vec::new(), connection_updates.clone())
            .expect("disconnect boundary and emergency drink should advance");
        assert!(matches!(
            outcome.events.as_slice(),
            [WorldEvent {
                kind: WorldEventKind::ItemConsumed {
                    actor_id: event_actor,
                    item_id,
                    remaining_charges: 1,
                    stored_kcal: 0,
                    thirst,
                },
                ..
            }] if *event_actor == actor_id
                && *item_id == drink_id
                && *thirst == cdda_sim::THIRST_DEATH_THRESHOLD - 100
        ));
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates,
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("drink events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("drink journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(43, [27; 32]))
            .expect("drink tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        let recovered_drink = recovered
            .actor_snapshot(actor_id)
            .expect("actor should recover")
            .inventory
            .into_iter()
            .find(|item| item.id == drink_id)
            .expect("drink should recover");
        assert_eq!(recovered_drink.charges, 1);

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [36; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("drink replay should export")
            .verify(&content)
            .expect("drink replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .actor_snapshot(actor_id)
                .expect("actor should replay")
                .inventory
                .into_iter()
                .find(|item| item.id == drink_id)
                .expect("drink should replay")
                .charges,
            1
        );
    }

    #[test]
    fn safe_autopilot_sleep_recovers_and_replays_on_the_same_bed() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(44, [28; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(44, [28; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        let mut chunk = Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 });
        chunk
            .set_furniture(
                LocalTileCoord { x: 1, y: 1 },
                Some(cdda_protocol::FurnitureTileSnapshot {
                    furniture_id: String::from("f_bed"),
                    move_cost_mod: 0,
                    transparent: true,
                    blocks_door: false,
                    comfort: 5,
                    floor_bedding_warmth: 1_000,
                }),
            )
            .expect("bed should install");
        world.insert_chunk(chunk);
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("connected actor should spawn");
        let mut snapshot = world.snapshot();
        let actor = snapshot
            .actors
            .iter_mut()
            .find(|actor| actor.id == actor_id)
            .expect("actor should snapshot");
        actor.action_points = i64::from(cdda_protocol::ACTION_POINT_THRESHOLD);
        actor.sleepiness = cdda_sim::SLEEPINESS_TIRED;
        let mut world = WorldState::from_snapshot(&snapshot).expect("fixture should restore");
        store
            .write_snapshot(0, &world)
            .expect("pre-sleep snapshot should write");

        let connection_updates = vec![ActorConnectionUpdateV1 {
            actor_id,
            connected: false,
        }];
        let outcome = world
            .advance_tick_with_recovery_inputs(Vec::new(), Vec::new(), connection_updates.clone())
            .expect("disconnect boundary and safe sleep should advance");
        assert!(matches!(
            outcome.events.as_slice(),
            [WorldEvent {
                kind: WorldEventKind::ActorFellAsleep {
                    actor_id: event_actor,
                    reason: SleepReason::Autopilot,
                },
                ..
            }] if *event_actor == actor_id
        ));
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates,
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("sleep events should hash"),
                    state_hash: outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("sleep journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(44, [28; 32]))
            .expect("sleep tick should recover");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert!(
            recovered
                .actor_snapshot(actor_id)
                .expect("actor should recover")
                .sleeping
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [37; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("sleep replay should export")
            .verify(&content)
            .expect("sleep replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replayed world hashes"),
            expected_hash
        );
        assert!(
            replayed
                .actor_snapshot(actor_id)
                .expect("actor should replay")
                .sleeping
        );
    }

    #[test]
    fn in_progress_book_study_recovers_and_replays_the_same_theory_gain() {
        fn record_tick(world: &mut WorldState, commands: Vec<ClientCommand>) -> JournalTickV1 {
            let outcome = world
                .advance_tick(commands.clone())
                .expect("book-study replay tick should advance");
            JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("book-study events should hash"),
                state_hash: outcome.canonical_hash,
            }
        }

        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(36, [20; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(36, [20; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let actor_id = world
            .spawn_actor(position, false)
            .expect("reader should spawn disconnected");
        let book_item_id = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position,
                type_id: String::from("manual_pistol"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("book should spawn");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");
        let study = BookStudyV1 {
            book_type_id: String::from("manual_pistol"),
            skill_id: String::from("pistol"),
            required_skill_level: 0,
            maximum_skill_level: 3,
            intelligence_requirement: 3,
            time_moves: 100,
            source_time_minutes: 15,
        };
        let mut before_snapshot = vec![record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::PickUp {
                    item_id: book_item_id,
                },
            }],
        )];
        for _ in 0..19 {
            before_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let read_client_tick = world.tick();
        before_snapshot.push(record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(2),
                client_tick: read_client_tick,
                kind: CommandKind::ReadBook {
                    item_id: book_item_id,
                    book_type_id: study.book_type_id.clone(),
                    study: Some(Box::new(study)),
                },
            }],
        ));
        assert!(
            world
                .actor_snapshot(actor_id)
                .is_some_and(|actor| actor.read_activity.is_some() && !actor.connected)
        );
        let first_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: before_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("study-start journal should append");
        store
            .write_snapshot(first_sequence, &world)
            .expect("in-progress study snapshot should write");

        let mut after_snapshot = Vec::new();
        while world
            .actor_snapshot(actor_id)
            .is_some_and(|actor| actor.read_activity.is_some())
        {
            assert!(after_snapshot.len() < 25, "book study should complete");
            after_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let final_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: after_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("study-completion journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let expected_skill = world
            .actor_snapshot(actor_id)
            .and_then(|actor| {
                actor
                    .skills
                    .into_iter()
                    .find(|skill| skill.skill_id == "pistol")
            })
            .expect("study should create pistol theory");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(36, [20; 32]))
            .expect("in-progress study and tail should recover");
        assert_eq!(recovered_sequence, final_sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(
            recovered.actor_snapshot(actor_id).and_then(|actor| actor
                .skills
                .into_iter()
                .find(|skill| skill.skill_id == "pistol")),
            Some(expected_skill)
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [30; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("study replay should export")
            .verify(&content)
            .expect("self-contained study replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replay hashes"),
            expected_hash
        );
    }

    #[test]
    fn in_progress_terrain_construction_recovers_and_replays_exactly() {
        fn record_tick(world: &mut WorldState, commands: Vec<ClientCommand>) -> JournalTickV1 {
            let outcome = world
                .advance_tick(commands.clone())
                .expect("construction replay tick should advance");
            JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("construction events should hash"),
                state_hash: outcome.canonical_hash,
            }
        }

        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(40, [24; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(40, [24; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let target = WorldPosition { x: 2, y: 1, z: 0 };
        let actor_id = world
            .spawn_actor(position, false)
            .expect("builder should spawn disconnected");
        let component_id = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position,
                type_id: String::from("g_carpet"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("carpet item should spawn");
        let hammer_id = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position,
                type_id: String::from("hammer"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("quality provider should spawn");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");
        let recipe = cdda_protocol::ConstructionRecipeV1 {
            construction_id: String::from("constr_carpet_green"),
            name: String::from("Lay green carpet"),
            time_moves: 100,
            required_skills: vec![cdda_protocol::CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 0,
            }],
            components: vec![vec![cdda_protocol::CraftComponentRequirementV1 {
                type_id: String::from("g_carpet"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }]],
            qualities: vec![vec![cdda_protocol::CraftQualityRequirementV1 {
                quality_id: String::from("HAMMER"),
                level: 2,
                amount: 1,
                providers: vec![cdda_protocol::CraftQualityProviderV1 {
                    type_id: String::from("hammer"),
                    minimum_charges: 0,
                }],
            }]],
            pre_terrain: vec![String::from("t_floor")],
            requires_empty: false,
            result: cdda_protocol::ConstructionResultV1::Terrain(
                cdda_protocol::TerrainTileSnapshot {
                    terrain_id: String::from("t_carpet_green"),
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
            ),
        };
        let mut before_snapshot = vec![record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::PickUp {
                    item_id: component_id,
                },
            }],
        )];
        for _ in 0..19 {
            before_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let hammer_pickup_tick = world.tick();
        before_snapshot.push(record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(2),
                client_tick: hammer_pickup_tick,
                kind: CommandKind::PickUp { item_id: hammer_id },
            }],
        ));
        for _ in 0..19 {
            before_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let start_tick = world.tick();
        before_snapshot.push(record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(3),
                client_tick: start_tick,
                kind: CommandKind::Construct {
                    target,
                    construction_id: recipe.construction_id.clone(),
                    construction: Some(Box::new(recipe)),
                },
            }],
        ));
        assert!(
            world
                .actor_snapshot(actor_id)
                .is_some_and(|actor| { actor.construction_activity.is_some() && !actor.connected })
        );
        let first_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: before_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("construction-start journal should append");
        store
            .write_snapshot(first_sequence, &world)
            .expect("in-progress construction snapshot should write");

        let mut after_snapshot = Vec::new();
        while world
            .actor_snapshot(actor_id)
            .is_some_and(|actor| actor.construction_activity.is_some())
        {
            assert!(after_snapshot.len() < 25, "construction should complete");
            after_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let final_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: after_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("construction-completion journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let built_terrain = |world: &WorldState| {
            let (coord, local) = target.chunk_and_local();
            let snapshot = world.snapshot();
            let chunk = snapshot
                .chunks
                .into_iter()
                .find(|chunk| chunk.coord == coord)
                .expect("target chunk should exist");
            chunk.tiles
                [usize::from(local.y) * cdda_protocol::SUBMAP_SIZE as usize + usize::from(local.x)]
            .clone()
        };
        assert_eq!(built_terrain(&world).terrain_id, "t_carpet_green");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(40, [24; 32]))
            .expect("construction snapshot and tail should recover");
        assert_eq!(recovered_sequence, final_sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        assert_eq!(built_terrain(&recovered), built_terrain(&world));

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [33; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("construction replay should export")
            .verify(&content)
            .expect("self-contained construction replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replay hashes"),
            expected_hash
        );
        assert_eq!(built_terrain(&replayed), built_terrain(&world));
    }

    #[test]
    fn in_progress_disassembly_recovers_and_replays_to_stable_ground_outputs() {
        fn record_tick(world: &mut WorldState, commands: Vec<ClientCommand>) -> JournalTickV1 {
            let outcome = world
                .advance_tick(commands.clone())
                .expect("disassembly replay tick should advance");
            JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events)
                    .expect("disassembly events should hash"),
                state_hash: outcome.canonical_hash,
            }
        }

        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(37, [21; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(37, [21; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let actor_id = world
            .spawn_actor(position, false)
            .expect("actor should spawn disconnected");
        let target_item_id = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position,
                type_id: String::from("assembled_test_item"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: Some(RangedWeaponSnapshot {
                    ammunition_type: String::from("test_ammo"),
                    ammunition_remaining: 3,
                    ammunition_capacity: 6,
                    range: 8,
                    damage: 10,
                    dispersion: 100,
                    sound_volume: 0,
                }),
            })
            .expect("target should spawn");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");
        let recipe = DisassemblyRecipeV1 {
            recipe_id: String::from("assembled_test_item"),
            target_type_id: String::from("assembled_test_item"),
            time_moves: 100,
            difficulty: 0,
            primary_skill_id: None,
            learn_requirements: Vec::new(),
            autolearn: false,
            autolearn_requirements: Vec::new(),
            unload_charges_as: Some(CraftItemPrototypeV1 {
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
            }),
            requires_empty_charges: false,
            components: vec![DisassemblyComponentV1 {
                output_instances: 2,
                count_by_charges: false,
                output: CraftItemPrototypeV1 {
                    type_id: String::from("recovered_test_component"),
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
        };
        let mut before_snapshot = vec![record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::PickUp {
                    item_id: target_item_id,
                },
            }],
        )];
        for _ in 0..19 {
            before_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let start_tick = world.tick();
        before_snapshot.push(record_tick(
            &mut world,
            vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(2),
                client_tick: start_tick,
                kind: CommandKind::Disassemble {
                    item_id: target_item_id,
                    item_type_id: recipe.target_type_id.clone(),
                    recipe: Some(Box::new(recipe)),
                },
            }],
        ));
        let activity = world
            .actor_snapshot(actor_id)
            .and_then(|actor| actor.disassembly_activity)
            .expect("disassembly should be active at checkpoint");
        assert_eq!(
            activity
                .target_item
                .ranged_weapon
                .expect("reserved target should remain ranged")
                .ammunition_remaining,
            0
        );
        let reserved = activity.reserved_component_items;
        assert_eq!(reserved.len(), 2);
        let unloaded_item_id = world
            .snapshot()
            .ground_items
            .into_iter()
            .find(|ground| ground.item.type_id == "test_round")
            .expect("disassembly start should unload ammunition")
            .item
            .id;
        assert_eq!(
            world
                .ground_item_snapshot(unloaded_item_id)
                .expect("unloaded ammunition should remain on the ground")
                .item
                .charges,
            3
        );
        let first_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: before_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("disassembly-start journal should append");
        store
            .write_snapshot(first_sequence, &world)
            .expect("in-progress disassembly snapshot should write");

        let mut after_snapshot = Vec::new();
        while world
            .actor_snapshot(actor_id)
            .is_some_and(|actor| actor.disassembly_activity.is_some())
        {
            assert!(after_snapshot.len() < 25, "disassembly should complete");
            after_snapshot.push(record_tick(&mut world, Vec::new()));
        }
        let final_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: after_snapshot,
                allocator_inputs: Vec::new(),
            })
            .expect("disassembly-completion journal should append");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        for item_id in &reserved {
            let ground = world
                .ground_item_snapshot(*item_id)
                .expect("recovered component should be on the ground");
            assert_eq!(ground.position, position);
            assert_eq!(ground.item.type_id, "recovered_test_component");
        }
        assert_eq!(
            world
                .ground_item_snapshot(unloaded_item_id)
                .expect("completion should retain unloaded ammunition")
                .item
                .charges,
            3
        );

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(37, [21; 32]))
            .expect("in-progress disassembly and tail should recover");
        assert_eq!(recovered_sequence, final_sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered world hashes"),
            expected_hash
        );
        for item_id in &reserved {
            assert!(recovered.ground_item_snapshot(*item_id).is_some());
        }
        assert_eq!(
            recovered
                .ground_item_snapshot(unloaded_item_id)
                .expect("recovery should retain unloaded ammunition")
                .item
                .charges,
            3
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [31; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("disassembly replay should export")
            .verify(&content)
            .expect("self-contained disassembly replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replay hashes"),
            expected_hash
        );
        assert_eq!(
            replayed
                .ground_item_snapshot(unloaded_item_id)
                .expect("replay should retain unloaded ammunition")
                .item
                .charges,
            3
        );
    }

    #[test]
    fn integral_tool_unload_recovers_and_replays_exact_charges() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(38, [22; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(38, [22; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let actor_id = world
            .spawn_actor(position, false)
            .expect("actor should spawn");
        let target_item_id = world
            .spawn_ground_item(cdda_sim::ItemSpawn {
                position,
                type_id: String::from("integral_test_tool"),
                charges: 7,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
            })
            .expect("tool should spawn");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");
        let recipe = DisassemblyRecipeV1 {
            recipe_id: String::from("integral_test_tool"),
            target_type_id: String::from("integral_test_tool"),
            time_moves: 100,
            difficulty: 0,
            primary_skill_id: None,
            learn_requirements: Vec::new(),
            autolearn: false,
            autolearn_requirements: Vec::new(),
            unload_charges_as: Some(CraftItemPrototypeV1 {
                type_id: String::from("battery"),
                charges: 100,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                tracks_temperature: false,
                thermal_properties: None,
                ammunition_type: String::from("battery"),
                ranged_weapon: None,
                magazine_capacity: 0,
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
                containment: Default::default(),
            }),
            requires_empty_charges: false,
            components: vec![DisassemblyComponentV1 {
                output_instances: 1,
                count_by_charges: false,
                output: CraftItemPrototypeV1 {
                    type_id: String::from("test_component"),
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
        };
        let mut ticks = Vec::new();
        {
            let mut record = |world: &mut WorldState, commands: Vec<ClientCommand>| {
                let outcome = world
                    .advance_tick(commands.clone())
                    .expect("tool replay tick should advance");
                ticks.push(JournalTickV1 {
                    tick: outcome.tick,
                    commands,
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&outcome.events)
                        .expect("tool events should hash"),
                    state_hash: outcome.canonical_hash,
                });
            };
            record(
                &mut world,
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(1),
                    client_tick: SimTick(0),
                    kind: CommandKind::PickUp {
                        item_id: target_item_id,
                    },
                }],
            );
            for _ in 0..19 {
                record(&mut world, Vec::new());
            }
            let start_tick = world.tick();
            record(
                &mut world,
                vec![ClientCommand {
                    actor_id,
                    sequence: CommandSequence(2),
                    client_tick: start_tick,
                    kind: CommandKind::Disassemble {
                        item_id: target_item_id,
                        item_type_id: recipe.target_type_id.clone(),
                        recipe: Some(Box::new(recipe)),
                    },
                }],
            );
        }
        let unloaded = world
            .snapshot()
            .ground_items
            .into_iter()
            .find(|ground| ground.item.type_id == "battery")
            .expect("tool charges should unload exactly once");
        assert_eq!(unloaded.item.charges, 7);
        assert_eq!(
            world
                .actor_snapshot(actor_id)
                .and_then(|actor| actor.disassembly_activity)
                .expect("tool disassembly should be active")
                .target_item
                .charges,
            0
        );
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks,
                allocator_inputs: Vec::new(),
            })
            .expect("tool journal should append");
        store
            .write_snapshot(sequence, &world)
            .expect("active tool snapshot should write");
        let expected_hash = world.canonical_hash().expect("live world should hash");
        let (_, recovered) = store
            .recover_latest(WorldState::new(38, [22; 32]))
            .expect("tool activity should recover");
        assert_eq!(
            recovered.canonical_hash().expect("recovery should hash"),
            expected_hash
        );
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [32; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let replayed = store
            .export_replay(content.clone())
            .expect("tool replay should export")
            .verify(&content)
            .expect("tool replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replay should hash"),
            expected_hash
        );
        assert_eq!(
            replayed
                .ground_item_snapshot(unloaded.item.id)
                .expect("replay retains unloaded charges")
                .item
                .charges,
            7
        );
    }

    #[test]
    fn replay_archive_range_is_exact_due_and_commit_guarded() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(12, [9; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(12, [9; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("archive anchor snapshot should write");
        assert!(matches!(
            store.initialize_replay_archive_cursor(0, 0),
            Err(StoreError::InvalidRecord)
        ));
        let start = store
            .initialize_replay_archive_cursor(0, 100)
            .expect("archive cursor should initialize");
        let audit_endpoint = EndpointIdentity([42; 32]);
        store
            .create_pending_account(
                AccountId::new(12, 1),
                "Archive Audit",
                AccountRole::Player,
                audit_endpoint,
                200,
            )
            .expect("audited account creation should commit");
        store
            .enroll_endpoint(audit_endpoint, 201)
            .expect("audited enrollment should commit");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("commandless tick should advance");
        let batch = JournalBatchV1 {
            ticks: vec![JournalTickV1 {
                tick: outcome.tick,
                commands: Vec::new(),
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events).expect("events should hash"),
                state_hash: outcome.canonical_hash,
            }],
            allocator_inputs: Vec::new(),
        };
        let end_sequence = store
            .append_journal_batch(&batch)
            .expect("journal should append");
        store
            .write_snapshot(end_sequence, &world)
            .expect("archive end snapshot should write");
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [24; 32],
            enabled_mods: vec![String::from("dda")],
        };
        assert!(
            store
                .prepare_replay_archive(end_sequence, 3_699, content.clone())
                .expect("not-due query should work")
                .is_none()
        );
        let prepared = store
            .prepare_replay_archive(end_sequence, 3_700, content.clone())
            .expect("due archive should prepare")
            .expect("archive should be due");
        assert_eq!(prepared.start, start);
        assert_eq!(prepared.end.journal_sequence, end_sequence);
        assert_eq!(prepared.end.security_audit_sequence, 2);
        assert_eq!(prepared.bundle.journal_batches.len(), 1);
        assert_eq!(prepared.bundle.security_audit_records.len(), 2);
        let replayed = prepared
            .bundle
            .verify(&content)
            .expect("prepared replay should verify");
        assert_eq!(
            replayed.canonical_hash().expect("replay should hash"),
            world.canonical_hash().expect("live world should hash")
        );
        store
            .commit_replay_archive(prepared.start, prepared.end)
            .expect("cursor should commit once");
        assert_eq!(
            store.replay_archive_cursor().expect("cursor should read"),
            prepared.end
        );
        assert!(matches!(
            store.commit_replay_archive(prepared.start, prepared.end),
            Err(StoreError::ReplayArchiveCursorChanged)
        ));
    }

    #[test]
    fn pending_replay_archive_survives_restart_and_retries_exact_range() {
        let path = test_database_path();
        remove_database(&path);
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [25; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let mut world = WorldState::new(15, [12; 32]);
        let first_prepared;

        {
            let mut store = WorldStore::open(&path).expect("database should open");
            store
                .initialize_world(15, [12; 32])
                .expect("world should initialize");
            let block = store.reserve_id_block().expect("block should reserve");
            world
                .install_reserved_block(block)
                .expect("block should install");
            world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
            store
                .write_snapshot(0, &world)
                .expect("archive anchor should write");
            store
                .initialize_replay_archive_cursor(0, 100)
                .expect("archive cursor should initialize");

            let outcome = world
                .advance_tick(Vec::new())
                .expect("first tick should advance");
            let first_sequence = store
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
                .expect("first journal should append");
            store
                .write_snapshot(first_sequence, &world)
                .expect("first endpoint should write");
            first_prepared = store
                .prepare_replay_archive(first_sequence, 3_700, content.clone())
                .expect("first archive should prepare")
                .expect("archive should be due");
            store
                .create_pending_account(
                    AccountId::new(15, 1),
                    "After Prepare",
                    AccountRole::Player,
                    EndpointIdentity([90; 32]),
                    3_701,
                )
                .expect("later security input should commit outside the pending range");
        }

        {
            let mut store = WorldStore::open(&path).expect("database should reopen");
            let outcome = world
                .advance_tick(Vec::new())
                .expect("later tick should advance");
            let later_sequence = store
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
                .expect("later journal should append");
            store
                .write_snapshot(later_sequence, &world)
                .expect("later endpoint should write");

            let retried = store
                .prepare_replay_archive(later_sequence, 3_800, content)
                .expect("pending archive should prepare")
                .expect("pending archive should be retried");
            assert_eq!(retried, first_prepared);
            store
                .commit_replay_archive(retried.start, retried.end)
                .expect("exact pending cursor should commit");
            let pending: (i64, i64, i64) = store
                .connection
                .query_row(
                    "SELECT replay_pending_sequence, replay_pending_security_sequence,
                            replay_pending_utc
                     FROM world_metadata WHERE singleton = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("pending cursor should query");
            assert_eq!(pending, (0, 0, 0));
            assert_eq!(
                store.replay_archive_cursor().expect("cursor should read"),
                retried.end
            );
            assert_eq!(retried.end.security_audit_sequence, 0);
            assert_eq!(
                store
                    .security_audit_after(retried.end.security_audit_sequence)
                    .expect("later security input should remain for the next archive")
                    .len(),
                1
            );
        }
        remove_database(&path);
    }

    #[test]
    fn daily_compaction_keeps_archive_anchor_and_newer_recovery_history() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(16, [13; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(16, [13; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("initial snapshot should write");
        let started_utc = 100;
        store
            .initialize_replay_archive_cursor(0, started_utc)
            .expect("archive cursor should initialize");

        let first_outcome = world
            .advance_tick(Vec::new())
            .expect("first tick should advance");
        let first_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: first_outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&first_outcome.events)
                        .expect("events should hash"),
                    state_hash: first_outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("first journal should append");
        store
            .write_snapshot(first_sequence, &world)
            .expect("archive anchor should write");
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [27; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let prepared = store
            .prepare_replay_archive(
                first_sequence,
                started_utc + REPLAY_ARCHIVE_INTERVAL_SECONDS,
                content,
            )
            .expect("archive should prepare")
            .expect("archive should be due");
        store
            .commit_replay_archive(prepared.start, prepared.end)
            .expect("archive cursor should commit");
        assert!(
            store
                .compact_recovery_history(started_utc + REPLAY_ARCHIVE_INTERVAL_SECONDS)
                .expect("early compaction query should work")
                .is_none()
        );

        let second_outcome = world
            .advance_tick(Vec::new())
            .expect("second tick should advance");
        let second_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: second_outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&second_outcome.events)
                        .expect("events should hash"),
                    state_hash: second_outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("second journal should append");
        store
            .write_snapshot(second_sequence, &world)
            .expect("newer snapshot should write");

        let compacted = store
            .compact_recovery_history(started_utc + RECOVERY_COMPACTION_INTERVAL_SECONDS)
            .expect("due compaction should work")
            .expect("compaction should be due");
        assert_eq!(
            compacted,
            RecoveryCompaction {
                through_journal_sequence: first_sequence,
                deleted_journal_batches: 1,
                deleted_snapshots: 1,
            }
        );
        assert!(
            store
                .snapshot_at(0)
                .expect("old snapshot should query")
                .is_none()
        );
        assert!(
            store
                .snapshot_at(first_sequence)
                .expect("anchor should query")
                .is_some()
        );
        assert_eq!(
            store
                .journal_after(0)
                .expect("remaining journal should query")
                .iter()
                .map(|(sequence, _)| *sequence)
                .collect::<Vec<_>>(),
            vec![second_sequence]
        );
        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(16, [13; 32]))
            .expect("compacted world should recover");
        assert_eq!(recovered_sequence, second_sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered should hash"),
            world.canonical_hash().expect("live world should hash")
        );
    }

    #[test]
    fn online_backup_is_private_integrity_checked_and_replay_verified() {
        let source_path = test_database_path();
        let backup_path = test_database_path();
        remove_database(&source_path);
        remove_database(&backup_path);
        let mut store = WorldStore::open(&source_path).expect("source should open");
        store
            .initialize_world(17, [14; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(17, [14; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");
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
        let first_tick = world.tick();
        let first_hash = world.canonical_hash().expect("first world should hash");
        store
            .connection
            .execute_batch(
                "CREATE TABLE backup_test_padding(bytes BLOB NOT NULL);
                 INSERT INTO backup_test_padding(bytes) VALUES(zeroblob(16777216));",
            )
            .expect("backup fixture should allocate multiple copy steps");
        let backup_source = source_path.clone();
        let backup_destination = backup_path.clone();
        let backup_thread = std::thread::spawn(move || {
            WorldStore::backup_from_path(backup_source, backup_destination)
        });
        let second_outcome = world
            .advance_tick(Vec::new())
            .expect("world should continue while backup reads");
        let second_sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: vec![JournalTickV1 {
                    tick: second_outcome.tick,
                    commands: Vec::new(),
                    held_movement: Vec::new(),
                    connection_updates: Vec::new(),
                    events_hash: canonical_events_hash(&second_outcome.events)
                        .expect("second events should hash"),
                    state_hash: second_outcome.canonical_hash,
                }],
                allocator_inputs: Vec::new(),
            })
            .expect("live journal should interleave with online copy");
        let backup = backup_thread
            .join()
            .expect("backup thread should not panic")
            .expect("backup should verify");
        assert_eq!(backup.schema_version, SCHEMA_VERSION);
        assert_eq!(backup.world_namespace, 17);
        match backup.journal_sequence {
            captured if captured == sequence => {
                assert_eq!(backup.tick, first_tick);
                assert_eq!(backup.state_hash, first_hash);
            }
            captured if captured == second_sequence => {
                assert_eq!(backup.tick, world.tick());
                assert_eq!(
                    backup.state_hash,
                    world.canonical_hash().expect("second world should hash")
                );
            }
            other => panic!("backup captured an impossible journal sequence {other}"),
        }
        WorldStore::verify_backup(&backup_path, backup)
            .expect("copied database should verify independently");
        assert!(matches!(
            WorldStore::verify_backup(
                &backup_path,
                DatabaseBackupMetadata {
                    state_hash: [0; 32],
                    ..backup
                }
            ),
            Err(StoreError::BackupVerificationMismatch)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&backup_path)
                    .expect("backup metadata should read")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(store);
        remove_database(&source_path);
        remove_database(&backup_path);
    }

    #[test]
    fn allocator_boundary_inputs_replay_across_snapshot_ranges() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(13, [10; 32])
            .expect("world should initialize");
        let first = store
            .reserve_id_block()
            .expect("first block should reserve");
        let mut world = WorldState::new(13, [10; 32]);
        world
            .install_reserved_block(first)
            .expect("first block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("initial snapshot should write");
        store
            .initialize_replay_archive_cursor(0, 1_000)
            .expect("cursor should initialize");

        world
            .advance_allocator_high_water(first.end)
            .expect("unused first block should burn");
        let second = store
            .reserve_id_block()
            .expect("second block should reserve");
        world
            .install_reserved_block(second)
            .expect("second block should install");
        let sequence = store
            .append_journal_batch(&JournalBatchV1 {
                ticks: Vec::new(),
                allocator_inputs: vec![
                    AllocatorInputV1::IdBlockAbandoned {
                        at_tick: world.tick(),
                        high_water: first.end,
                    },
                    AllocatorInputV1::IdBlockReserved {
                        at_tick: world.tick(),
                        block: second,
                    },
                ],
            })
            .expect("allocator boundary should journal");
        store
            .write_snapshot(sequence, &world)
            .expect("final snapshot should write");
        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [26; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let prepared = store
            .prepare_replay_archive(
                sequence,
                1_000 + REPLAY_ARCHIVE_INTERVAL_SECONDS,
                content.clone(),
            )
            .expect("archive should prepare")
            .expect("archive should be due");
        assert!(prepared.bundle.journal_batches[0].1.ticks.is_empty());
        let replayed = prepared
            .bundle
            .verify(&content)
            .expect("allocator range should replay");
        assert_eq!(
            replayed.canonical_hash().expect("replay should hash"),
            world.canonical_hash().expect("live world should hash")
        );
    }

    #[test]
    fn spawn_journal_sequence_orders_both_sides_of_allocator_boundary() {
        let build_world = || {
            let mut world = WorldState::new(14, [11; 32]);
            world
                .install_reserved_block(ReservedIdBlock::new(1, 4_096).expect("valid block"))
                .expect("block should install");
            world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
            world
        };
        let boundary = JournalBatchV1 {
            ticks: Vec::new(),
            allocator_inputs: vec![
                AllocatorInputV1::IdBlockAbandoned {
                    at_tick: SimTick(0),
                    high_water: 4_096,
                },
                AllocatorInputV1::IdBlockReserved {
                    at_tick: SimTick(0),
                    block: ReservedIdBlock::new(4_097, 8_192).expect("valid next block"),
                },
            ],
        };

        let initial_before = build_world();
        let mut live_before = WorldState::from_snapshot(&initial_before.snapshot())
            .expect("initial state should clone");
        let actor_before = live_before
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn before boundary");
        let spawn_before = CharacterSpawnV1 {
            created_tick: SimTick(0),
            created_after_journal_sequence: 0,
            actor: live_before
                .actor_snapshot(actor_before)
                .expect("actor snapshot should exist"),
        };
        live_before
            .advance_allocator_high_water(4_096)
            .expect("old remainder should burn");
        live_before
            .install_reserved_block(ReservedIdBlock::new(4_097, 8_192).expect("valid next block"))
            .expect("next block should install");
        let (_, replayed_before) =
            replay_parts(0, initial_before, &[spawn_before], &[(1, boundary.clone())])
                .expect("spawn-before-boundary should replay in order");
        assert_eq!(
            replayed_before
                .canonical_hash()
                .expect("replay should hash"),
            live_before
                .canonical_hash()
                .expect("live state should hash")
        );

        let initial_after = build_world();
        let mut live_after = WorldState::from_snapshot(&initial_after.snapshot())
            .expect("initial state should clone");
        live_after
            .advance_allocator_high_water(4_096)
            .expect("old remainder should burn");
        live_after
            .install_reserved_block(ReservedIdBlock::new(4_097, 8_192).expect("valid next block"))
            .expect("next block should install");
        let actor_after = live_after
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn after boundary");
        let spawn_after = CharacterSpawnV1 {
            created_tick: SimTick(0),
            created_after_journal_sequence: 1,
            actor: live_after
                .actor_snapshot(actor_after)
                .expect("actor snapshot should exist"),
        };
        let (_, replayed_after) = replay_parts(0, initial_after, &[spawn_after], &[(1, boundary)])
            .expect("spawn-after-boundary should replay in order");
        assert_eq!(
            replayed_after.canonical_hash().expect("replay should hash"),
            live_after.canonical_hash().expect("live state should hash")
        );
    }

    #[test]
    fn journal_replay_reproduces_state_and_rejects_hash_mismatch() {
        let mut store = WorldStore::open_in_memory().expect("database should open");
        store
            .initialize_world(31, [7; 32])
            .expect("world should initialize");
        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(31, [7; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        let actor_id = world
            .spawn_actor(WorldPosition { x: 1, y: 1, z: 0 }, true)
            .expect("actor should spawn");
        let mut sleep_ready = world.snapshot();
        let actor = sleep_ready
            .actors
            .iter_mut()
            .find(|actor| actor.id == actor_id)
            .expect("actor should be present");
        actor.sleepiness = cdda_sim::SLEEPINESS_TIRED;
        actor.speed =
            u16::try_from(cdda_sim::ACTOR_ACTION_THRESHOLD).expect("action threshold should fit");
        world = WorldState::from_snapshot(&sleep_ready).expect("sleep-ready state should restore");
        store
            .write_snapshot(0, &world)
            .expect("base snapshot should write");

        let mut ticks = Vec::new();
        for (sequence, kind) in [
            (
                1,
                CommandKind::Move {
                    dx: 1,
                    dy: 0,
                    dz: 0,
                },
            ),
            (2, CommandKind::Sleep),
            (3, CommandKind::Wake),
        ] {
            let commands = vec![ClientCommand {
                actor_id,
                sequence: CommandSequence(sequence),
                client_tick: world.tick(),
                kind,
            }];
            let outcome = world
                .advance_tick(commands.clone())
                .expect("recorded tick should advance");
            ticks.push(JournalTickV1 {
                tick: outcome.tick,
                commands,
                held_movement: Vec::new(),
                connection_updates: Vec::new(),
                events_hash: canonical_events_hash(&outcome.events).expect("events should hash"),
                state_hash: outcome.canonical_hash,
            });
        }
        let held_movement = vec![HeldMovementUpdateV1 {
            actor_id,
            sequence: HeldInputSequence(1),
            client_tick: world.tick(),
            direction: Some(HorizontalDirection { dx: 0, dy: 1 }),
            source: HeldMovementUpdateSource::Client,
        }];
        let outcome = world
            .advance_tick_with_inputs(Vec::new(), held_movement.clone())
            .expect("held movement tick should advance");
        ticks.push(JournalTickV1 {
            tick: outcome.tick,
            commands: Vec::new(),
            held_movement,
            connection_updates: Vec::new(),
            events_hash: canonical_events_hash(&outcome.events).expect("events should hash"),
            state_hash: outcome.canonical_hash,
        });
        let batch = JournalBatchV1 {
            ticks,
            allocator_inputs: Vec::new(),
        };
        let sequence = store
            .append_journal_batch(&batch)
            .expect("journal should append");
        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(31, [7; 32]))
            .expect("journal should replay");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovered hash"),
            world.canonical_hash().expect("expected hash")
        );
        assert!(
            !recovered
                .actor_snapshot(actor_id)
                .expect("actor exists")
                .sleeping
        );

        let content = ContentIdentity {
            baseline_commit: BASELINE_COMMIT.to_owned(),
            manifest_hash: [23; 32],
            enabled_mods: vec![String::from("dda")],
        };
        let bundle = store
            .export_replay(content.clone())
            .expect("replay should export");
        let encoded = postcard::to_stdvec(&bundle).expect("replay should encode");
        let decoded: ReplayBundleV1 = postcard::from_bytes(&encoded).expect("replay should decode");
        let replayed = decoded.verify(&content).expect("bundle should verify");
        assert_eq!(
            replayed.canonical_hash().expect("bundle should hash"),
            world.canonical_hash().expect("live world should hash")
        );
        let mut snapshot_address_tampered = decoded.clone();
        snapshot_address_tampered.initial_snapshot_object_hash[0] ^= 1;
        assert!(matches!(
            snapshot_address_tampered.verify(&content),
            Err(StoreError::StateHashMismatch)
        ));
        let mut tampered = decoded;
        tampered.final_state_hash[0] ^= 1;
        assert!(matches!(
            tampered.verify(&content),
            Err(StoreError::ReplayHashMismatch)
        ));

        let mut corrupt = batch;
        corrupt.ticks[0].state_hash[0] ^= 1;
        let mut corrupt_store = WorldStore::open_in_memory().expect("corrupt store should open");
        corrupt_store
            .initialize_world(31, [7; 32])
            .expect("corrupt world should initialize");
        corrupt_store
            .write_snapshot(
                0,
                &store
                    .latest_snapshot()
                    .expect("snapshot should query")
                    .expect("snapshot should exist")
                    .1,
            )
            .expect("corrupt base snapshot should write");
        corrupt_store
            .append_journal_batch(&corrupt)
            .expect("internally consistent corrupt record should append");
        assert!(matches!(
            corrupt_store.recover_latest(WorldState::new(31, [7; 32])),
            Err(StoreError::ReplayHashMismatch)
        ));
    }

    #[test]
    fn exact_endpoint_enrollment_enables_account() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(41, [6; 32])
            .expect("world should initialize");
        let account_id = AccountId::new(41, 1);
        let endpoint = EndpointIdentity([7; 32]);
        let created = store
            .create_pending_account(
                account_id,
                "Ada",
                AccountRole::Administrator,
                endpoint,
                1_000,
            )
            .expect("pending account should be created");
        assert_eq!(created.status, AccountStatus::InitialEnrollment);
        assert!(matches!(
            store.authorize_endpoint(endpoint, 1_001),
            Err(StoreError::UnauthorizedEndpoint)
        ));

        let enrolled = store
            .enroll_endpoint(endpoint, 1_599)
            .expect("exact identity should enroll before expiry");
        assert_eq!(enrolled.status, AccountStatus::Enabled);
        assert_eq!(
            store
                .authorize_endpoint(endpoint, 1_599)
                .expect("active endpoint should authorize"),
            enrolled
        );
        let actor_id = ActorId::new(41, 2);
        let actor = ActorSnapshot {
            id: actor_id,
            position: WorldPosition { x: 0, y: 0, z: 0 },
            hp: cdda_sim::DEFAULT_ACTOR_HP,
            body_parts: vec![cdda_protocol::ActorBodyPartSnapshotV1 {
                body_part_id: String::from("torso"),
                current_hp: cdda_sim::DEFAULT_ACTOR_HP,
                maximum_hp: cdda_sim::DEFAULT_ACTOR_HP,
            }],
            effects: Vec::new(),
            base_strength: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            base_dexterity: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            base_intelligence: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            base_perception: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            connected: true,
            last_command_sequence: CommandSequence(0),
            last_held_input_sequence: cdda_protocol::HeldInputSequence(0),
            held_movement: None,
            inventory: Vec::new(),
            wielded: None,
            worn: Vec::new(),
            stored_kcal: cdda_sim::DEFAULT_STORED_KCAL,
            thirst: 0,
            sleepiness: 0,
            sleeping: false,
            sleep_intervals: 0,
            stamina: cdda_sim::DEFAULT_ACTOR_MAXIMUM_STAMINA,
            maximum_stamina: cdda_sim::DEFAULT_ACTOR_MAXIMUM_STAMINA,
            dodge_attempts_remaining: 1,
            speed: cdda_sim::DEFAULT_ACTOR_SPEED,
            action_points: i64::from(cdda_sim::ACTOR_ACTION_THRESHOLD),
            queued_actions: Vec::new(),
            craft_activity: None,
            read_activity: None,
            disassembly_activity: None,
            construction_activity: None,
            pending_interaction: None,
            learned_recipes: Vec::new(),
            skills: Vec::new(),
            proficiencies: Vec::new(),
            map_memory: Vec::new(),
        };
        let character = store
            .create_character(account_id, "Survivor Ada", SimTick(0), 0, &actor)
            .expect("character should be created");
        assert_eq!(
            store
                .characters_for_account(account_id)
                .expect("characters should list"),
            vec![character]
        );
        assert!(
            store
                .account_owns_actor(account_id, actor_id)
                .expect("ownership should query")
        );
        assert!(
            !store
                .account_owns_actor(AccountId::new(41, 99), actor_id)
                .expect("cross-account ownership should query")
        );
    }

    #[test]
    fn remote_account_administration_is_paginated_audited_and_lockout_safe() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(54, [5; 32])
            .expect("world should initialize");
        let administrator_id = AccountId::new(54, 1);
        let player_id = AccountId::new(54, 2);
        let second_administrator_id = AccountId::new(54, 3);
        let pending_id = AccountId::new(54, 4);
        let administrator_endpoint = EndpointIdentity([1; 32]);
        let player_endpoint = EndpointIdentity([2; 32]);
        let second_administrator_endpoint = EndpointIdentity([3; 32]);

        for (account_id, name, role, endpoint) in [
            (
                administrator_id,
                "Primary Administrator",
                AccountRole::Administrator,
                administrator_endpoint,
            ),
            (player_id, "Player", AccountRole::Player, player_endpoint),
        ] {
            store
                .create_pending_account(account_id, name, role, endpoint, 1_000)
                .expect("account should create");
            store
                .enroll_endpoint(endpoint, 1_001)
                .expect("account should enroll");
        }

        assert!(matches!(
            store.authorize_admin_endpoint(player_endpoint, 1_002),
            Err(StoreError::ModeratorRequired)
        ));
        assert_eq!(
            store
                .authorize_admin_endpoint(administrator_endpoint, 1_003)
                .expect("administrator endpoint should authorize")
                .id,
            administrator_id
        );
        assert!(matches!(
            store.set_account_role(
                administrator_endpoint,
                administrator_id,
                AccountRole::Player,
                1_004,
            ),
            Err(StoreError::CannotTargetSelf)
        ));
        assert!(matches!(
            store.set_account_role(
                administrator_endpoint,
                AccountId::new(54, 99),
                AccountRole::Player,
                1_005,
            ),
            Err(StoreError::AccountUnavailable)
        ));

        store
            .create_pending_account(
                second_administrator_id,
                "Secondary Administrator",
                AccountRole::Administrator,
                second_administrator_endpoint,
                1_006,
            )
            .expect("second administrator should create");
        assert!(matches!(
            store.set_account_role(
                administrator_endpoint,
                second_administrator_id,
                AccountRole::Player,
                1_007,
            ),
            Err(StoreError::InvalidAccountTransition)
        ));
        store
            .enroll_endpoint(second_administrator_endpoint, 1_008)
            .expect("second administrator should enroll");
        store
            .create_pending_account(
                pending_id,
                "Pending",
                AccountRole::Player,
                EndpointIdentity([4; 32]),
                1_009,
            )
            .expect("pending account should create");

        let first_page = store
            .admin_accounts(administrator_endpoint, None, 2, 1_010)
            .expect("first account page should list");
        assert_eq!(
            first_page
                .accounts
                .iter()
                .map(|account| account.id)
                .collect::<Vec<_>>(),
            vec![administrator_id, player_id]
        );
        assert_eq!(first_page.next_after, Some(player_id));
        let second_page = store
            .admin_accounts(administrator_endpoint, first_page.next_after, 2, 1_011)
            .expect("second account page should list");
        assert_eq!(
            second_page
                .accounts
                .iter()
                .map(|account| account.id)
                .collect::<Vec<_>>(),
            vec![second_administrator_id, pending_id]
        );
        assert_eq!(second_page.next_after, None);

        let promoted = store
            .set_account_role(
                administrator_endpoint,
                player_id,
                AccountRole::Moderator,
                1_012,
            )
            .expect("enabled player should promote");
        assert!(promoted.changed);
        assert_eq!(promoted.account.role, AccountRole::Moderator);
        assert!(matches!(
            store.admin_create_account(
                player_endpoint,
                "Forbidden Account",
                AccountRole::Player,
                EndpointIdentity([8; 32]),
                1_013,
            ),
            Err(StoreError::AdministratorRequired)
        ));
        let disabled = store
            .set_account_status(
                administrator_endpoint,
                player_id,
                AccountStatus::Disabled,
                1_013,
            )
            .expect("player should disable");
        assert!(disabled.changed);
        assert!(matches!(
            store.authorize_endpoint(player_endpoint, 1_013),
            Err(StoreError::AccountUnavailable)
        ));
        store
            .set_account_status(
                administrator_endpoint,
                player_id,
                AccountStatus::Enabled,
                1_014,
            )
            .expect("disabled account with an active endpoint should enable");
        store
            .set_account_status(
                administrator_endpoint,
                player_id,
                AccountStatus::Banned,
                1_015,
            )
            .expect("account should become terminally banned");
        assert!(matches!(
            store.set_account_status(
                administrator_endpoint,
                player_id,
                AccountStatus::Enabled,
                1_016,
            ),
            Err(StoreError::InvalidAccountTransition)
        ));
        assert!(matches!(
            store.set_account_status(
                administrator_endpoint,
                pending_id,
                AccountStatus::Enabled,
                1_017,
            ),
            Err(StoreError::InvalidAccountTransition)
        ));

        store
            .set_account_status(
                administrator_endpoint,
                second_administrator_id,
                AccountStatus::Disabled,
                1_018,
            )
            .expect("one of two enabled administrators may disable");
        assert!(matches!(
            store.set_account_status(
                second_administrator_endpoint,
                administrator_id,
                AccountStatus::Disabled,
                1_019,
            ),
            Err(StoreError::AccountUnavailable)
        ));

        let initial_endpoint = EndpointIdentity([9; 32]);
        let discarded_pending = EndpointIdentity([10; 32]);
        let replacement_endpoint = EndpointIdentity([11; 32]);
        let created = store
            .admin_create_account(
                administrator_endpoint,
                "Remote Account",
                AccountRole::Player,
                initial_endpoint,
                1_020,
            )
            .expect("administrator should create a pending account");
        assert_eq!(created.account.id, AccountId::new(54, 5));
        assert_eq!(created.account.status, AccountStatus::InitialEnrollment);
        assert_eq!(created.pending_endpoint.endpoint, initial_endpoint);
        assert_eq!(
            store
                .admin_endpoint_bindings(administrator_endpoint, created.account.id, 1_021)
                .expect("administrator should list a target's endpoints"),
            vec![created.pending_endpoint]
        );
        store
            .admin_add_pending_endpoint(
                administrator_endpoint,
                created.account.id,
                discarded_pending,
                1_022,
            )
            .expect("administrator should stage exact iroh proof without bypassing it");
        store
            .admin_revoke_endpoint(
                administrator_endpoint,
                created.account.id,
                discarded_pending,
                1_023,
            )
            .expect("administrator should revoke a pending endpoint");
        store
            .enroll_endpoint(initial_endpoint, 1_024)
            .expect("the exact created endpoint must still prove itself");
        store
            .admin_add_pending_endpoint(
                administrator_endpoint,
                created.account.id,
                replacement_endpoint,
                1_025,
            )
            .expect("administrator should stage a replacement");
        store
            .enroll_endpoint(replacement_endpoint, 1_026)
            .expect("the replacement endpoint must prove itself");
        store
            .admin_revoke_endpoint(
                administrator_endpoint,
                created.account.id,
                initial_endpoint,
                1_027,
            )
            .expect("administrator should revoke one of two active endpoints");
        assert!(matches!(
            store.authorize_endpoint(initial_endpoint, 1_028),
            Err(StoreError::UnauthorizedEndpoint)
        ));
        assert!(matches!(
            store.admin_revoke_endpoint(
                administrator_endpoint,
                created.account.id,
                replacement_endpoint,
                1_029,
            ),
            Err(StoreError::CannotRevokeLastEndpoint)
        ));

        let audit = store
            .security_audit_after(0)
            .expect("administration audit should verify");
        assert!(audit.iter().any(|(_, record)| {
            matches!(record.action, SecurityAuditActionV1::OpenAdmin)
                && record.outcome
                    == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::ModeratorRequired)
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                SecurityAuditActionV1::SetAccountRole { account_id, .. }
                    if account_id == administrator_id
            ) && record.outcome
                == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::CannotTargetSelf)
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(record.action, SecurityAuditActionV1::ListAccounts { .. })
                && record.outcome == SecurityAuditOutcomeV1::Allowed
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                SecurityAuditActionV1::AdminCreateAccount {
                    account_id: Some(account_id),
                    ..
                } if account_id == created.account.id
            ) && record.outcome == SecurityAuditOutcomeV1::Allowed
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                SecurityAuditActionV1::AdminRevokeEndpoint { account_id, endpoint }
                    if account_id == created.account.id && endpoint == replacement_endpoint
            ) && record.outcome
                == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::LastActiveEndpoint)
        }));
    }

    #[test]
    fn moderation_and_character_transfer_enforce_the_role_matrix() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(55, [6; 32])
            .expect("world should initialize");
        let administrator_id = AccountId::new(55, 1);
        let moderator_id = AccountId::new(55, 2);
        let player_id = AccountId::new(55, 3);
        let destination_id = AccountId::new(55, 4);
        let other_moderator_id = AccountId::new(55, 5);
        let administrator_endpoint = EndpointIdentity([1; 32]);
        let moderator_endpoint = EndpointIdentity([2; 32]);
        let player_endpoint = EndpointIdentity([3; 32]);
        let destination_endpoint = EndpointIdentity([4; 32]);
        let other_moderator_endpoint = EndpointIdentity([5; 32]);
        for (account_id, name, role, endpoint) in [
            (
                administrator_id,
                "Administrator",
                AccountRole::Administrator,
                administrator_endpoint,
            ),
            (
                moderator_id,
                "Moderator",
                AccountRole::Moderator,
                moderator_endpoint,
            ),
            (player_id, "Player", AccountRole::Player, player_endpoint),
            (
                destination_id,
                "Destination",
                AccountRole::Player,
                destination_endpoint,
            ),
            (
                other_moderator_id,
                "Other Moderator",
                AccountRole::Moderator,
                other_moderator_endpoint,
            ),
        ] {
            store
                .create_pending_account(account_id, name, role, endpoint, 1_000)
                .expect("account should create");
            store
                .enroll_endpoint(endpoint, 1_001)
                .expect("account should enroll");
        }
        assert_eq!(
            store
                .authorize_admin_endpoint(moderator_endpoint, 1_002)
                .expect("moderator should authorize on the management ALPN")
                .id,
            moderator_id
        );
        assert!(matches!(
            store.authorize_admin_endpoint(player_endpoint, 1_002),
            Err(StoreError::ModeratorRequired)
        ));
        assert!(matches!(
            store.set_account_role(moderator_endpoint, player_id, AccountRole::Moderator, 1_003,),
            Err(StoreError::AdministratorRequired)
        ));
        store
            .kick_account(moderator_endpoint, player_id, 1_004)
            .expect("moderator should kick a player");
        assert!(matches!(
            store.kick_account(moderator_endpoint, moderator_id, 1_005),
            Err(StoreError::CannotTargetSelf)
        ));
        assert!(matches!(
            store.kick_account(moderator_endpoint, other_moderator_id, 1_006),
            Err(StoreError::TargetRoleNotAllowed)
        ));
        store
            .kick_account(administrator_endpoint, moderator_id, 1_007)
            .expect("administrator should kick another moderator");

        let suspended = store
            .set_account_suspension(moderator_endpoint, player_id, Some(3_600), 2_000)
            .expect("moderator should suspend a player");
        assert_eq!(suspended.account.suspended_until_utc, Some(5_600));
        assert!(matches!(
            store.authorize_endpoint(player_endpoint, 2_001),
            Err(StoreError::AccountUnavailable)
        ));
        assert_eq!(
            store
                .authorize_endpoint(player_endpoint, 5_600)
                .expect("suspension should expire at its exact UTC boundary")
                .id,
            player_id
        );
        assert!(matches!(
            store.set_account_suspension(
                moderator_endpoint,
                player_id,
                Some(MAX_MODERATION_DURATION_SECONDS + 1),
                5_601,
            ),
            Err(StoreError::InvalidModerationDuration)
        ));
        let cleared = store
            .set_account_suspension(moderator_endpoint, player_id, None, 5_602)
            .expect("moderator should clear a player suspension");
        assert_eq!(cleared.account.suspended_until_utc, None);

        let muted = store
            .set_account_mute(moderator_endpoint, player_id, Some(60), 6_000)
            .expect("moderator should mute a player");
        assert_eq!(muted.account.muted_until_utc, Some(6_060));
        assert!(matches!(
            store.authorize_chat(player_id, player_endpoint, 6_001),
            Err(StoreError::AccountMuted(6_060))
        ));
        store
            .authorize_chat(player_id, player_endpoint, 6_060)
            .expect("mute should expire at its exact UTC boundary");

        let actor = |actor_id| ActorSnapshot {
            id: actor_id,
            position: WorldPosition { x: 0, y: 0, z: 0 },
            hp: cdda_sim::DEFAULT_ACTOR_HP,
            body_parts: vec![cdda_protocol::ActorBodyPartSnapshotV1 {
                body_part_id: String::from("torso"),
                current_hp: cdda_sim::DEFAULT_ACTOR_HP,
                maximum_hp: cdda_sim::DEFAULT_ACTOR_HP,
            }],
            effects: Vec::new(),
            base_strength: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            base_dexterity: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            base_intelligence: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            base_perception: cdda_sim::DEFAULT_ACTOR_BASE_STAT,
            connected: false,
            last_command_sequence: CommandSequence(0),
            last_held_input_sequence: HeldInputSequence(0),
            held_movement: None,
            inventory: Vec::new(),
            wielded: None,
            worn: Vec::new(),
            stored_kcal: cdda_sim::DEFAULT_STORED_KCAL,
            thirst: 0,
            sleepiness: 0,
            sleeping: false,
            sleep_intervals: 0,
            stamina: cdda_sim::DEFAULT_ACTOR_MAXIMUM_STAMINA,
            maximum_stamina: cdda_sim::DEFAULT_ACTOR_MAXIMUM_STAMINA,
            dodge_attempts_remaining: 1,
            speed: cdda_sim::DEFAULT_ACTOR_SPEED,
            action_points: i64::from(cdda_sim::ACTOR_ACTION_THRESHOLD),
            queued_actions: Vec::new(),
            craft_activity: None,
            read_activity: None,
            disassembly_activity: None,
            construction_activity: None,
            pending_interaction: None,
            learned_recipes: Vec::new(),
            skills: Vec::new(),
            proficiencies: Vec::new(),
            map_memory: Vec::new(),
        };
        let unique_actor = ActorId::new(55, 10);
        let conflicting_actor = ActorId::new(55, 11);
        let destination_actor = ActorId::new(55, 12);
        store
            .create_character(player_id, "Unique", SimTick(0), 0, &actor(unique_actor))
            .expect("unique character should create");
        store
            .create_character(
                player_id,
                "Conflict",
                SimTick(0),
                0,
                &actor(conflicting_actor),
            )
            .expect("conflicting source character should create");
        store
            .create_character(
                destination_id,
                "Conflict",
                SimTick(0),
                0,
                &actor(destination_actor),
            )
            .expect("conflicting destination character should create");
        assert!(matches!(
            store.admin_private_character(
                moderator_endpoint,
                unique_actor,
                None,
                MAX_ADMIN_INVENTORY_PER_PAGE,
                6_088,
            ),
            Err(StoreError::AdministratorRequired)
        ));
        assert_eq!(
            store
                .admin_private_character(
                    administrator_endpoint,
                    unique_actor,
                    None,
                    MAX_ADMIN_INVENTORY_PER_PAGE,
                    6_089,
                )
                .expect("administrator should inspect private character identity"),
            AdminCharacterIdentity {
                account_id: player_id,
                actor_id: unique_actor,
                name: String::from("Unique"),
            }
        );
        let first_report = store
            .submit_report(
                player_id,
                player_endpoint,
                unique_actor,
                destination_actor,
                ReportReason::Chat,
                "Repeated abusive chat",
                6_090,
            )
            .expect("an authenticated character should submit a report");
        assert_eq!(first_report, ReportId(1));
        store
            .submit_report(
                destination_id,
                destination_endpoint,
                destination_actor,
                unique_actor,
                ReportReason::Other,
                "Reciprocal test report",
                6_091,
            )
            .expect("a second account should submit a report");
        assert!(matches!(
            store.submit_report(
                player_id,
                player_endpoint,
                unique_actor,
                conflicting_actor,
                ReportReason::Other,
                "self report",
                6_092,
            ),
            Err(StoreError::CannotReportSelf)
        ));
        assert!(matches!(
            store.submit_report(
                player_id,
                player_endpoint,
                unique_actor,
                ActorId::new(55, 9_999),
                ReportReason::Exploit,
                "unknown target",
                6_093,
            ),
            Err(StoreError::CharacterUnavailable)
        ));
        let first_report_page = store
            .admin_reports(moderator_endpoint, Some(ReportState::Open), None, 1, 6_094)
            .expect("moderator should page reports");
        assert_eq!(first_report_page.reports.len(), 1);
        assert_eq!(first_report_page.reports[0].report_id, first_report);
        assert_eq!(
            first_report_page.reports[0].details,
            "Repeated abusive chat"
        );
        assert_eq!(first_report_page.next_after, Some(first_report));
        assert_eq!(
            store
                .admin_reports(
                    moderator_endpoint,
                    Some(ReportState::Open),
                    first_report_page.next_after,
                    1,
                    6_095,
                )
                .expect("the second report page should query")
                .reports
                .len(),
            1
        );
        let resolved_report = store
            .set_report_state(
                moderator_endpoint,
                first_report,
                ReportState::Actioned,
                6_096,
            )
            .expect("moderator should resolve an open report");
        assert_eq!(resolved_report.state, ReportState::Actioned);
        assert_eq!(resolved_report.resolved_by_account, Some(moderator_id));
        assert!(resolved_report.resolution_audit_sequence.is_some());
        assert!(
            store
                .admin_reports(moderator_endpoint, Some(ReportState::Open), None, 32, 6_097,)
                .expect("open report filter should query")
                .reports
                .iter()
                .all(|report| report.report_id != first_report)
        );
        let actioned_reports = store
            .admin_reports(
                moderator_endpoint,
                Some(ReportState::Actioned),
                None,
                32,
                6_098,
            )
            .expect("actioned report filter should query");
        assert_eq!(actioned_reports.reports, vec![resolved_report]);
        assert!(matches!(
            store.set_report_state(
                moderator_endpoint,
                first_report,
                ReportState::Dismissed,
                6_099,
            ),
            Err(StoreError::InvalidReport)
        ));
        let history_page = store
            .admin_moderation_history(moderator_endpoint, player_id, None, 2, 6_100)
            .expect("moderator should read target moderation history");
        assert_eq!(history_page.entries.len(), 2);
        assert!(history_page.next_after.is_some());
        assert!(
            history_page
                .entries
                .iter()
                .all(|entry| entry.target_account == player_id)
        );
        for index in 0..4_u16 {
            store
                .submit_report(
                    player_id,
                    player_endpoint,
                    unique_actor,
                    destination_actor,
                    ReportReason::Other,
                    &format!("bounded report {index}"),
                    6_100 + i64::from(index),
                )
                .expect("the hourly report allowance should admit five reports");
        }
        assert!(matches!(
            store.submit_report(
                player_id,
                player_endpoint,
                unique_actor,
                destination_actor,
                ReportReason::Other,
                "rate limit overflow",
                6_104,
            ),
            Err(StoreError::ReportRateLimited)
        ));
        assert_eq!(
            store
                .admin_characters(moderator_endpoint, player_id, 6_100)
                .expect("moderator should list character names")
                .len(),
            2
        );
        for index in 0..MAX_CHARACTERS_PER_ACCOUNT {
            let counter = 100_u64
                .checked_add(u64::try_from(index).expect("fixture index should fit"))
                .expect("fixture counter should fit");
            store
                .create_character(
                    other_moderator_id,
                    &format!("Capacity {index}"),
                    SimTick(0),
                    0,
                    &actor(ActorId::new(55, counter)),
                )
                .expect("destination capacity fixture should create");
        }
        assert!(matches!(
            store.transfer_character(
                administrator_endpoint,
                conflicting_actor,
                other_moderator_id,
                6_101,
            ),
            Err(StoreError::TooManyCharacters)
        ));
        assert!(matches!(
            store.transfer_character(
                administrator_endpoint,
                conflicting_actor,
                destination_id,
                6_102,
            ),
            Err(StoreError::CharacterNameConflict)
        ));
        assert!(matches!(
            store.transfer_character(moderator_endpoint, unique_actor, destination_id, 6_103),
            Err(StoreError::AdministratorRequired)
        ));
        let transfer = store
            .transfer_character(administrator_endpoint, unique_actor, destination_id, 6_104)
            .expect("administrator should transfer unique character ownership");
        assert_eq!(transfer.previous_owner, player_id);
        assert_eq!(transfer.new_owner, destination_id);
        assert!(
            !store
                .account_owns_actor(player_id, unique_actor)
                .expect("old ownership should query")
        );
        assert!(
            store
                .account_owns_actor(destination_id, unique_actor)
                .expect("new ownership should query")
        );

        let audit = store
            .security_audit_after(0)
            .expect("moderation audit should verify");
        let private_report_text = b"Repeated abusive chat";
        assert!(audit.iter().all(|(_, record)| {
            let encoded = postcard::to_stdvec(record).expect("audit should encode");
            !encoded
                .windows(private_report_text.len())
                .any(|window| window == private_report_text)
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                SecurityAuditActionV1::TransferCharacter {
                    actor_id,
                    new_owner,
                } if actor_id == conflicting_actor && new_owner == other_moderator_id
            ) && record.outcome
                == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::TooManyCharacters)
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(record.action, SecurityAuditActionV1::SubmitReport { .. })
                && record.outcome
                    == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::RateLimited)
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                SecurityAuditActionV1::InspectPrivateCharacter {
                    actor_id,
                    inventory_after: None,
                    inventory_limit: MAX_ADMIN_INVENTORY_PER_PAGE,
                } if actor_id == unique_actor
            ) && record.outcome == SecurityAuditOutcomeV1::Allowed
        }));
        assert!(audit.iter().any(|(_, record)| {
            matches!(
                record.action,
                SecurityAuditActionV1::InspectPrivateCharacter { actor_id, .. }
                    if actor_id == unique_actor
            ) && record.outcome
                == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::AdministratorRequired)
        }));
        for action in ["kick", "suspend", "mute", "transfer"] {
            assert!(audit.iter().any(|(_, record)| match action {
                "kick" => matches!(record.action, SecurityAuditActionV1::KickAccount { .. }),
                "suspend" => {
                    matches!(record.action, SecurityAuditActionV1::SetSuspension { .. })
                }
                "mute" => matches!(record.action, SecurityAuditActionV1::SetMute { .. }),
                "transfer" => {
                    matches!(
                        record.action,
                        SecurityAuditActionV1::TransferCharacter { .. }
                    )
                }
                _ => false,
            }));
        }
    }

    #[test]
    fn endpoint_rotation_and_local_recovery_are_permanent_and_fail_closed() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(42, [7; 32])
            .expect("world should initialize");
        let account_id = AccountId::new(42, 1);
        let first = EndpointIdentity([1; 32]);
        let second = EndpointIdentity([2; 32]);
        let replacement = EndpointIdentity([3; 32]);
        store
            .create_pending_account(
                account_id,
                "Rotation Tester",
                AccountRole::Player,
                first,
                1_000,
            )
            .expect("initial account should create");
        store
            .enroll_endpoint(first, 1_001)
            .expect("first endpoint should enroll");
        assert!(matches!(
            store.revoke_endpoint(account_id, first, first, 1_002),
            Err(StoreError::CannotRevokeLastEndpoint)
        ));

        let pending = store
            .add_pending_endpoint(account_id, first, second, 2_000)
            .expect("rotation should add an exact pending endpoint");
        assert_eq!(pending.endpoint, second);
        assert_eq!(pending.state, EndpointBindingState::Pending);
        assert_eq!(pending.pending_expires_utc, Some(2_600));
        assert_eq!(
            store
                .endpoint_bindings(account_id)
                .expect("bindings should list")
                .len(),
            2
        );
        store
            .enroll_endpoint(second, 2_001)
            .expect("enabled account should enroll its added endpoint");
        store
            .revoke_endpoint(account_id, second, first, 2_002)
            .expect("one of two active endpoints should revoke");
        assert!(matches!(
            store.authorize_endpoint(first, 2_002),
            Err(StoreError::UnauthorizedEndpoint)
        ));
        assert!(matches!(
            store.add_pending_endpoint(account_id, second, first, 2_100),
            Err(StoreError::EndpointAlreadyBound)
        ));

        store
            .recover_account_endpoint(account_id, replacement, 3_000)
            .expect("local recovery should lock around an exact replacement");
        assert!(matches!(
            store.authorize_endpoint(second, 3_000),
            Err(StoreError::UnauthorizedEndpoint)
        ));
        assert!(matches!(
            store.add_pending_endpoint(account_id, second, EndpointIdentity([4; 32]), 3_001),
            Err(StoreError::AccountUnavailable)
        ));
        let recovered = store
            .enroll_endpoint(replacement, 3_001)
            .expect("replacement proof should unlock the account");
        assert_eq!(recovered.status, AccountStatus::Enabled);
        assert_eq!(
            store
                .authorize_endpoint(replacement, 3_001)
                .expect("replacement should authorize")
                .id,
            account_id
        );
        let bindings = store
            .endpoint_bindings(account_id)
            .expect("bindings should list");
        assert_eq!(
            bindings
                .iter()
                .filter(|binding| binding.state == EndpointBindingState::Revoked)
                .count(),
            2
        );
        assert_eq!(
            bindings
                .iter()
                .filter(|binding| binding.state == EndpointBindingState::Active)
                .count(),
            1
        );
        let disposable_pending = EndpointIdentity([4; 32]);
        store
            .add_pending_endpoint(account_id, replacement, disposable_pending, 3_100)
            .expect("an active replacement should stage another exact endpoint");
        store
            .revoke_endpoint(account_id, replacement, disposable_pending, 3_101)
            .expect("pending endpoints should be revocable without affecting the last active key");
        assert_eq!(
            store
                .endpoint_bindings(account_id)
                .expect("bindings should list")
                .into_iter()
                .find(|binding| binding.endpoint == disposable_pending)
                .expect("revoked pending binding should remain permanently bound")
                .state,
            EndpointBindingState::Revoked
        );
        let audit = store
            .security_audit_after(0)
            .expect("security audit should verify and list");
        assert_eq!(audit.len(), 12);
        assert!(
            audit
                .iter()
                .enumerate()
                .all(|(index, (sequence, _))| *sequence == index as u64 + 1)
        );
        assert_eq!(
            audit
                .iter()
                .filter(|(_, record)| matches!(record.outcome, SecurityAuditOutcomeV1::Rejected(_)))
                .count(),
            3
        );
        assert!(audit.iter().any(|(_, record)| {
            record.outcome
                == SecurityAuditOutcomeV1::Rejected(SecurityAuditRejectionV1::LastActiveEndpoint)
        }));
        assert!(audit.iter().all(|(_, record)| {
            record.observed_tick == SimTick(0)
                && !matches!(
                    record.actor,
                    SecurityAuditActorV1::AuthenticatedAccount {
                        role: AccountRole::Moderator,
                        ..
                    } | SecurityAuditActorV1::AuthenticatedAccount {
                        role: AccountRole::Administrator,
                        ..
                    }
                )
        }));
        store
            .connection
            .execute(
                "UPDATE security_audit SET record_hash = zeroblob(32) WHERE sequence = 1",
                [],
            )
            .expect("audit corruption fixture should apply");
        assert!(matches!(
            store.security_audit_after(0),
            Err(StoreError::StateHashMismatch)
        ));
    }

    #[test]
    fn recovery_inserts_durable_character_at_its_recorded_tick() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(53, [4; 32])
            .expect("world should initialize");
        let account_id = AccountId::new(53, 900);
        let endpoint = EndpointIdentity([5; 32]);
        store
            .create_pending_account(
                account_id,
                "Recovery Ada",
                AccountRole::Player,
                endpoint,
                10,
            )
            .expect("pending account should be created");
        store
            .enroll_endpoint(endpoint, 11)
            .expect("account should enroll");

        let block = store.reserve_id_block().expect("block should reserve");
        let mut world = WorldState::new(53, [4; 32]);
        world
            .install_reserved_block(block)
            .expect("block should install");
        world.insert_chunk(Chunk::floor(ChunkCoord { x: 0, y: 0, z: 0 }));
        store
            .write_snapshot(0, &world)
            .expect("pre-character snapshot should persist");

        let actor_id = world
            .spawn_actor_with_base_stats(
                WorldPosition { x: 2, y: 3, z: 0 },
                true,
                cdda_protocol::CharacterCreationStatsV1 {
                    strength: 12,
                    dexterity: 11,
                    intelligence: 10,
                    perception: 9,
                },
            )
            .expect("actor should spawn");
        let actor = world
            .actor_snapshot(actor_id)
            .expect("spawn should have a snapshot");
        store
            .create_character(account_id, "Crash Safe Ada", world.tick(), 0, &actor)
            .expect("spawn record should persist");
        let outcome = world
            .advance_tick(Vec::new())
            .expect("post-creation tick should advance");
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
            .expect("post-creation journal should persist");

        let (recovered_sequence, recovered) = store
            .recover_latest(WorldState::new(53, [4; 32]))
            .expect("character should be restored before replay");
        assert_eq!(recovered_sequence, sequence);
        assert_eq!(
            recovered.canonical_hash().expect("recovery should hash"),
            world.canonical_hash().expect("live world should hash")
        );
        let recovered_actor = recovered
            .actor_snapshot(actor_id)
            .expect("durable character should recover");
        assert_eq!(recovered_actor.base_strength, 12);
        assert_eq!(recovered_actor.base_dexterity, 11);
        assert_eq!(recovered_actor.base_intelligence, 10);
        assert_eq!(recovered_actor.base_perception, 9);
    }

    #[test]
    fn expired_or_reused_endpoint_cannot_enroll() {
        let mut store = WorldStore::open_in_memory().expect("store should open");
        store
            .initialize_world(42, [8; 32])
            .expect("world should initialize");
        let endpoint = EndpointIdentity([9; 32]);
        store
            .create_pending_account(
                AccountId::new(42, 1),
                "Grace",
                AccountRole::Player,
                endpoint,
                2_000,
            )
            .expect("pending account should be created");
        assert!(matches!(
            store.enroll_endpoint(endpoint, 2_601),
            Err(StoreError::EnrollmentExpired)
        ));
        assert!(matches!(
            store.create_pending_account(
                AccountId::new(42, 2),
                "Linus",
                AccountRole::Player,
                endpoint,
                3_000,
            ),
            Err(StoreError::EndpointAlreadyBound)
        ));
    }
}
