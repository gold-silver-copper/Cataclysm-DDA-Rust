//! Stable domain identifiers and versioned network messages shared by every
//! runtime. This crate deliberately has no transport, persistence, or renderer
//! dependency.

use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod astronomy_table;

pub const PROTOCOL_VERSION: u16 = 75;
pub const BASELINE_COMMIT: &str = "4dfd36038b16650dc1b5cb9d79a3e42363174b05";
pub const GAME_ALPN: &[u8] = b"cdda-rust/game/1";
pub const ENROLL_ALPN: &[u8] = b"cdda-rust/enroll/1";
pub const ADMIN_ALPN: &[u8] = b"cdda-rust/admin/1";
pub const SUBMAP_SIZE: i32 = 12;
pub const ACTION_POINT_THRESHOLD: u32 = 2_000;
pub const ACTION_POINTS_PER_UPSTREAM_MOVE: i64 = 20;
pub const CRAFT_PRACTICE_MOVES: u64 = 100;
pub const CRAFT_PRACTICE_ACTION_POINTS: u64 =
    CRAFT_PRACTICE_MOVES * ACTION_POINTS_PER_UPSTREAM_MOVE as u64;
const MAX_COMBINED_TILE_COST: i64 = 4 * i32::MAX as i64;
/// Lowest possible readiness after charging one diagonal move between two
/// maximally expensive canonical terrain/furniture tiles.
pub const MIN_ACTION_POINTS: i64 = ACTION_POINT_THRESHOLD as i64
    - (MAX_COMBINED_TILE_COST * 71 / 2) * ACTION_POINTS_PER_UPSTREAM_MOVE;
pub const MAX_CONTROL_ENCODED: usize = 64 * 1024;
pub const MAX_CONTROL_DECODED: usize = 256 * 1024;
pub const MAX_BULK_ENCODED: usize = 8 * 1024 * 1024;
pub const MAX_BULK_DECODED: usize = 32 * 1024 * 1024;
pub const REQUIRED_DATAGRAM_SIZE: usize = 1_024;
pub const MAX_DATAGRAM_SIZE: usize = 1_200;
pub const MAX_CHAT_BYTES: usize = 4 * 1024;
pub const MAX_REPORT_BYTES: usize = 1024;
pub const MAX_REPORT_CHARACTERS: usize = 512;
pub const MAX_ENABLED_MODS: usize = 256;
pub const MAX_CRAFT_RECIPE_ID_BYTES: usize = 512;
pub const MAX_CRAFT_COMPONENT_GROUPS: usize = 128;
pub const MAX_CRAFT_COMPONENT_ALTERNATIVES: usize = 128;
pub const MAX_CRAFT_SUPPORT_GROUPS: usize = 128;
pub const MAX_CRAFT_SUPPORT_ALTERNATIVES: usize = 128;
pub const MAX_CRAFT_QUALITY_PROVIDERS: usize = 512;
pub const MAX_CRAFT_PROFICIENCIES: usize = 64;
pub const MAX_CRAFT_BOOK_REQUIREMENTS: usize = 64;
pub const MAX_BOOK_STUDY_MOVES: u64 = 100 * 60 * 60 * 24 * 7;
pub const CRAFT_PROFICIENCY_SCALE: u32 = 1_000_000;
pub const MAX_CRAFT_PROFICIENCY_MULTIPLIER: u32 = 100 * CRAFT_PROFICIENCY_SCALE;
pub const MAX_CRAFT_OUTPUT_INSTANCES: u16 = 256;
pub const MAX_CRAFT_BYPRODUCT_TYPES: usize = 64;
pub const MAX_DISASSEMBLY_COMPONENT_TYPES: usize = 256;
pub const MAX_LEARNED_RECIPES: usize = 8_192;
/// Canonical item condition uses pinned CDDA's coarse damage level, 0 through 4.
pub const MAX_ITEM_DAMAGE_LEVEL: u16 = 4;
pub const MAX_ACTOR_BASE_STAT: u16 = 100;
/// Pinned freeform character-creator bounds from `CHARACTER_STAT_MIN/MAX`.
pub const MIN_CHARACTER_CREATION_STAT: u16 = 4;
pub const MAX_CHARACTER_CREATION_STAT: u16 = 20;
pub const DEFAULT_CHARACTER_CREATION_STAT: u16 = 8;
pub const MAX_ITEM_COMPONENTS: usize = 256;
pub const MAX_ITEM_COMPONENT_DEPTH: usize = 8;
pub const MAX_MAGAZINE_COMPATIBLE_TYPES: usize = 256;
pub const MILLIJOULES_PER_BATTERY_CHARGE: u32 = 1_000_000;

const fn default_true() -> bool {
    true
}
pub const MAX_SKILLS: usize = 64;
pub const MAX_SKILL_ID_BYTES: usize = 64;
pub const MAX_SKILL_LEVEL: u8 = 10;
pub const MAX_PROFICIENCIES: usize = 512;
pub const MAX_PROFICIENCY_ID_BYTES: usize = 128;
pub const MAX_PROFICIENCY_PRACTICE_ACTION_POINTS: u64 = 100_000_000_000_000;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
        )]
        #[repr(transparent)]
        pub struct $name(u128);

        impl $name {
            #[must_use]
            pub const fn new(world_namespace: u64, counter: u64) -> Self {
                Self(((world_namespace as u128) << 64) | counter as u128)
            }

            #[must_use]
            pub const fn world_namespace(self) -> u64 {
                (self.0 >> 64) as u64
            }

            #[must_use]
            pub const fn counter(self) -> u64 {
                self.0 as u64
            }

            #[must_use]
            pub const fn as_u128(self) -> u128 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(
                    formatter,
                    "{:016x}:{:016x}",
                    self.world_namespace(),
                    self.counter()
                )
            }
        }
    };
}

stable_id!(WorldId);
stable_id!(AccountId);
stable_id!(ActorId);
stable_id!(CreatureId);
stable_id!(ItemId);
stable_id!(VehicleId);
stable_id!(MissionId);
stable_id!(EventId);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
pub struct EndpointIdentity(pub [u8; 32]);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountRole {
    Player,
    Moderator,
    Administrator,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountStatus {
    InitialEnrollment,
    Enabled,
    Disabled,
    Banned,
    RecoveryLocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EndpointBindingState {
    Pending,
    Active,
    Revoked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EndpointBindingSummary {
    pub endpoint: EndpointIdentity,
    pub state: EndpointBindingState,
    pub pending_expires_utc: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountKeyRequest {
    List,
    Add { endpoint: EndpointIdentity },
    Revoke { endpoint: EndpointIdentity },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountKeyRejection {
    AccountUnavailable,
    EndpointAlreadyBound,
    InvalidEndpoint,
    EndpointNotRevocable,
    LastActiveEndpoint,
    TooManyBindings,
    ServerBusy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountKeyResponse {
    Bindings(Vec<EndpointBindingSummary>),
    Pending(EndpointBindingSummary),
    Revoked { endpoint: EndpointIdentity },
    Rejected(AccountKeyRejection),
}

pub const MAX_ADMIN_ACCOUNTS_PER_PAGE: u16 = 128;
pub const MAX_CHARACTERS_PER_ACCOUNT: usize = 64;
pub const MAX_ADMIN_INVENTORY_PER_PAGE: u16 = 8;
pub const MAX_REPORTS_PER_PAGE: u16 = 32;
pub const MAX_MODERATION_HISTORY_PER_PAGE: u16 = 128;
pub const MAX_MODERATION_DURATION_SECONDS: u32 = 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[repr(transparent)]
pub struct ReportId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReportReason {
    Chat,
    Harassment,
    Exploit,
    Other,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReportState {
    Open,
    Actioned,
    Dismissed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlayerReport {
    pub target_actor: ActorId,
    pub reason: ReportReason,
    pub details: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReportRejection {
    CannotReportSelf,
    TargetUnavailable,
    InvalidReport,
    RateLimited,
    ServerBusy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReportResponse {
    Accepted { report_id: ReportId },
    Rejected(ReportRejection),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReportSummary {
    pub report_id: ReportId,
    pub created_utc: i64,
    pub reporter_account: AccountId,
    pub reporter_actor: ActorId,
    pub reporter_character: String,
    pub target_account: AccountId,
    pub target_actor: ActorId,
    pub target_character: String,
    pub reason: ReportReason,
    pub details: String,
    pub state: ReportState,
    pub resolved_utc: Option<i64>,
    pub resolved_by_account: Option<AccountId>,
    pub resolution_audit_sequence: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdminHello {
    pub protocol_version: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AdminAccountSummary {
    pub account_id: AccountId,
    pub display_name: String,
    pub role: AccountRole,
    pub status: AccountStatus,
    pub suspended_until_utc: Option<i64>,
    pub muted_until_utc: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ModerationKind {
    Kick,
    Suspension,
    Mute,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdminRequest {
    ListAccounts {
        after: Option<AccountId>,
        limit: u16,
    },
    ListCharacters {
        account_id: AccountId,
    },
    InspectCharacter {
        actor_id: ActorId,
        inventory_after: Option<ItemId>,
        inventory_limit: u16,
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
    SetRole {
        account_id: AccountId,
        role: AccountRole,
    },
    SetStatus {
        account_id: AccountId,
        status: AccountStatus,
    },
    SetSuspension {
        account_id: AccountId,
        duration_seconds: Option<u32>,
    },
    SetMute {
        account_id: AccountId,
        duration_seconds: Option<u32>,
    },
    Kick {
        account_id: AccountId,
    },
    TransferCharacter {
        actor_id: ActorId,
        new_owner: AccountId,
    },
    SetReportState {
        report_id: ReportId,
        state: ReportState,
    },
    CreateAccount {
        display_name: String,
        role: AccountRole,
        endpoint: EndpointIdentity,
    },
    ListEndpoints {
        account_id: AccountId,
    },
    AddEndpoint {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
    RevokeEndpoint {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdminRejection {
    AuthenticationRequired,
    AdministratorRequired,
    ModeratorRequired,
    ProtocolMismatch,
    AccountUnavailable,
    CannotTargetSelf,
    InvalidTransition,
    LastAdministrator,
    TargetRoleNotAllowed,
    CharacterUnavailable,
    CharacterNameConflict,
    TooManyCharacters,
    InvalidDisplayName,
    InvalidEndpoint,
    EndpointAlreadyBound,
    EndpointNotRevocable,
    LastActiveEndpoint,
    TooManyBindings,
    ServerBusy,
    UnexpectedMessage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdminResponse {
    Ready {
        account_id: AccountId,
        role: AccountRole,
    },
    Accounts {
        accounts: Vec<AdminAccountSummary>,
        next_after: Option<AccountId>,
    },
    AccountUpdated(AdminAccountSummary),
    Characters {
        account_id: AccountId,
        characters: Vec<CharacterSummary>,
        gameplay_session_active: bool,
        controlled_actor: Option<ActorId>,
    },
    PrivateCharacter(Box<PrivateCharacterInspection>),
    Reports {
        reports: Vec<ReportSummary>,
        next_after: Option<ReportId>,
    },
    ReportUpdated(ReportSummary),
    AccountCreated {
        account: AdminAccountSummary,
        pending_endpoint: EndpointBindingSummary,
    },
    Endpoints {
        account_id: AccountId,
        bindings: Vec<EndpointBindingSummary>,
    },
    EndpointPending {
        account_id: AccountId,
        binding: EndpointBindingSummary,
    },
    EndpointRevoked {
        account_id: AccountId,
        endpoint: EndpointIdentity,
    },
    ModerationHistory {
        account_id: AccountId,
        entries: Vec<ModerationHistoryEntry>,
        next_after: Option<u64>,
    },
    ModerationApplied {
        account: AdminAccountSummary,
        kind: ModerationKind,
        until_utc: Option<i64>,
    },
    CharacterTransferred {
        actor_id: ActorId,
        previous_owner: AccountId,
        new_owner: AccountId,
    },
    Rejected(AdminRejection),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModerationHistoryEntry {
    pub history_id: u64,
    pub security_audit_sequence: u64,
    pub occurred_utc: i64,
    pub operator_account: AccountId,
    pub target_account: AccountId,
    pub kind: ModerationKind,
    pub until_utc: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChatRejection {
    Muted { until_utc: i64 },
    ServerBusy,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SimTick(pub u64);

impl SimTick {
    pub const HZ: u64 = 20;

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

pub const SEASON_LENGTH_DAYS: u64 = 91;
pub const DAYS_PER_YEAR: u64 = SEASON_LENGTH_DAYS * 4;
pub const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
pub const DEFAULT_START_DAY_OF_SPRING: u16 = 61;
pub const DEFAULT_START_HOUR: u8 = 8;
pub const LUNAR_MONTH_SECONDS: u64 = 2_551_442;

#[must_use]
pub fn solar_boundaries_seconds(day_of_year: u16) -> Option<[u32; 4]> {
    astronomy_table::SOLAR_BOUNDARIES_SECONDS
        .get(usize::from(day_of_year))
        .copied()
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarSnapshot {
    pub year: u64,
    pub season: Season,
    pub day_of_season: u16,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl CalendarSnapshot {
    /// Converts the 20 Hz real-time simulation clock to CDDA's default
    /// 91-day-season calendar. The generic scenario starts on Spring day 61
    /// at 08:00; sub-second simulation ticks intentionally do not cross the
    /// wire because ordinary CDDA calendar turns are seconds.
    #[must_use]
    pub fn at_tick(tick: SimTick) -> Self {
        let start_seconds = (u64::from(DEFAULT_START_DAY_OF_SPRING) - 1) * SECONDS_PER_DAY
            + u64::from(DEFAULT_START_HOUR) * 60 * 60;
        let total_seconds = start_seconds + tick.0 / SimTick::HZ;
        let total_days = total_seconds / SECONDS_PER_DAY;
        let second_of_day = total_seconds % SECONDS_PER_DAY;
        let day_of_year = total_days % DAYS_PER_YEAR;
        let season_index = day_of_year / SEASON_LENGTH_DAYS;
        Self {
            year: total_days / DAYS_PER_YEAR + 1,
            season: match season_index {
                0 => Season::Spring,
                1 => Season::Summer,
                2 => Season::Autumn,
                _ => Season::Winter,
            },
            day_of_season: u16::try_from(day_of_year % SEASON_LENGTH_DAYS + 1)
                .expect("a 91-day season always fits u16"),
            hour: u8::try_from(second_of_day / 3_600).expect("hours always fit u8"),
            minute: u8::try_from(second_of_day % 3_600 / 60).expect("minutes always fit u8"),
            second: u8::try_from(second_of_day % 60).expect("seconds always fit u8"),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SkyPhase {
    Night,
    CivilTwilight,
    Day,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NaturalLightSnapshot {
    pub phase: SkyPhase,
    pub moon_phase: u8,
    pub sight_radius: u16,
}

impl NaturalLightSnapshot {
    #[must_use]
    pub fn at_tick(tick: SimTick) -> Self {
        let start_seconds = (u64::from(DEFAULT_START_DAY_OF_SPRING) - 1) * SECONDS_PER_DAY
            + u64::from(DEFAULT_START_HOUR) * 60 * 60;
        let total_seconds = start_seconds + tick.0 / SimTick::HZ;
        let day_of_year = usize::try_from(total_seconds / SECONDS_PER_DAY % DAYS_PER_YEAR)
            .expect("a day in the 364-day year always fits usize");
        let second_of_day = u32::try_from(total_seconds % SECONDS_PER_DAY)
            .expect("a second within one day always fits u32");
        let [civil_dawn, sunrise, sunset, civil_dusk] =
            astronomy_table::SOLAR_BOUNDARIES_SECONDS[day_of_year];
        let phase = if second_of_day < civil_dawn || second_of_day > civil_dusk {
            SkyPhase::Night
        } else if second_of_day < sunrise || second_of_day > sunset {
            SkyPhase::CivilTwilight
        } else {
            SkyPhase::Day
        };
        let nearest_midday = u128::from((total_seconds + SECONDS_PER_DAY / 2) / SECONDS_PER_DAY);
        let phase_numerator = nearest_midday * u128::from(SECONDS_PER_DAY) * 8;
        let rounded_phase = (phase_numerator + u128::from(LUNAR_MONTH_SECONDS / 2))
            / u128::from(LUNAR_MONTH_SECONDS);
        let moon_phase = u8::try_from(rounded_phase % 8).expect("moon phase always fits u8");
        let folded_moon = moon_phase.min(8 - moon_phase);
        let sight_radius = match phase {
            SkyPhase::Day => 60,
            SkyPhase::CivilTwilight => 8,
            SkyPhase::Night => [2, 2, 3, 11, 12][usize::from(folded_moon)],
        };
        Self {
            phase,
            moon_phase,
            sight_radius,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CommandSequence(pub u64);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct HeldInputSequence(pub u64);

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct LocalTileCoord {
    pub x: u8,
    pub y: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct WorldPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl WorldPosition {
    #[must_use]
    pub fn chunk_and_local(self) -> (ChunkCoord, LocalTileCoord) {
        let chunk = ChunkCoord {
            x: self.x.div_euclid(SUBMAP_SIZE),
            y: self.y.div_euclid(SUBMAP_SIZE),
            z: self.z,
        };
        let local = LocalTileCoord {
            x: self.x.rem_euclid(SUBMAP_SIZE) as u8,
            y: self.y.rem_euclid(SUBMAP_SIZE) as u8,
        };
        (chunk, local)
    }

    pub fn checked_offset(self, dx: i8, dy: i8, dz: i8) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(i32::from(dx))?,
            y: self.y.checked_add(i32::from(dy))?,
            z: self.z.checked_add(i32::from(dz))?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentIdentity {
    pub baseline_commit: String,
    pub manifest_hash: [u8; 32],
    pub enabled_mods: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientHello {
    pub protocol_version: u16,
    pub content: ContentIdentity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServerHello {
    pub protocol_version: u16,
    pub content: ContentIdentity,
    pub tick: SimTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnrollmentAccepted {
    pub account_id: AccountId,
    pub display_name: String,
    pub role: AccountRole,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EnrollmentRejection {
    UnknownIdentity,
    Expired,
    AccountUnavailable,
    ServerBusy,
    ProtocolMismatch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CharacterSummary {
    pub actor_id: ActorId,
    pub name: String,
}

/// The pinned baseline's freeform character creator independently clamps each
/// selected base stat to 4 through 20; legacy point pools are not active.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CharacterCreationStatsV1 {
    pub strength: u16,
    pub dexterity: u16,
    pub intelligence: u16,
    pub perception: u16,
}

impl CharacterCreationStatsV1 {
    #[must_use]
    pub fn is_valid(self) -> bool {
        [
            self.strength,
            self.dexterity,
            self.intelligence,
            self.perception,
        ]
        .into_iter()
        .all(|stat| (MIN_CHARACTER_CREATION_STAT..=MAX_CHARACTER_CREATION_STAT).contains(&stat))
    }
}

impl Default for CharacterCreationStatsV1 {
    fn default() -> Self {
        Self {
            strength: DEFAULT_CHARACTER_CREATION_STAT,
            dexterity: DEFAULT_CHARACTER_CREATION_STAT,
            intelligence: DEFAULT_CHARACTER_CREATION_STAT,
            perception: DEFAULT_CHARACTER_CREATION_STAT,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CharacterRequest {
    List,
    Create {
        name: String,
        base_stats: CharacterCreationStatsV1,
    },
    Select {
        actor_id: ActorId,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum GameplayRejection {
    AuthenticationRequired,
    ContentMismatch,
    InvalidCharacterName,
    CharacterNotOwned,
    CharacterAlreadyExists,
    NoSpawnLocation,
    SessionAlreadyActive,
    ServerFull,
    ServerBusy,
    UnexpectedMessage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientCommand {
    pub actor_id: ActorId,
    pub sequence: CommandSequence,
    pub client_tick: SimTick,
    pub kind: CommandKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HorizontalDirection {
    pub dx: i8,
    pub dy: i8,
}

impl HorizontalDirection {
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.dx >= -1
            && self.dx <= 1
            && self.dy >= -1
            && self.dy <= 1
            && (self.dx != 0 || self.dy != 0)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeldMovementInputV1 {
    pub actor_id: ActorId,
    pub sequence: HeldInputSequence,
    pub client_tick: SimTick,
    pub direction: Option<HorizontalDirection>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientDatagramV1 {
    HeldMovement(HeldMovementInputV1),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum HeldMovementUpdateSource {
    Client,
    LeaseExpired,
    Disconnected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HeldMovementUpdateV1 {
    pub actor_id: ActorId,
    pub sequence: HeldInputSequence,
    pub client_tick: SimTick,
    pub direction: Option<HorizontalDirection>,
    pub source: HeldMovementUpdateSource,
}

/// Recovery input recording whether an actor had a live gameplay session at a
/// simulation boundary. Presence is not itself canonical world state, but it
/// can affect canonical behavior such as disconnected survival autopilot.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorConnectionUpdateV1 {
    pub actor_id: ActorId,
    pub connected: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftComponentRequirementV1 {
    pub type_id: String,
    pub count: u32,
    pub count_by_charges: bool,
    /// Whether an exact consumed component object can later be recovered.
    /// Pinned CDDA filters stored objects only for the ITEM-level
    /// `UNRECOVERABLE` flag; recipe-local `NO_RECOVER` applies only when
    /// ordinary disassembly constructs components from recipe defaults.
    #[serde(default = "default_true")]
    pub recoverable: bool,
}

/// A presence tool remains carried; a charge tool spends `amount` aggregate
/// charges in the pinned twenty-bucket schedule.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftToolRequirementV1 {
    pub type_id: String,
    pub amount: u16,
    pub consumes_charges: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftQualityProviderV1 {
    pub type_id: String,
    /// Zero for an inherent quality; otherwise the pinned per-use charge
    /// threshold that must be present on this individual provider.
    pub minimum_charges: u16,
}

/// Providers are server-derived from pinned inherent and charged ITEM
/// qualities. Quality speed is relevant only to unsupported step recipes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftQualityRequirementV1 {
    pub quality_id: String,
    pub level: i32,
    pub amount: u16,
    pub providers: Vec<CraftQualityProviderV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftSkillRequirementV1 {
    pub skill_id: String,
    pub level: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftBookRequirementV1 {
    pub book_type_id: String,
    /// The recipe primary skill's theoretical level required to understand
    /// this identified carried book's recipe entry.
    pub required_skill_level: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftProficiencyV1 {
    pub proficiency_id: String,
    /// Required proficiencies gate the craft and have a zero time multiplier.
    pub required: bool,
    /// Fixed-point multiplier in millionths for an unlearned proficiency.
    pub time_multiplier_millionths: u32,
    /// Retained for the future stochastic-failure boundary.
    pub skill_penalty_millionths: i32,
    pub learning_time_multiplier_millionths: u32,
    pub max_experience_action_points: Option<u64>,
    pub time_to_learn_action_points: u64,
    pub can_learn: bool,
    /// Sorted direct prerequisites from the pinned proficiency definition.
    pub required_proficiencies: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MagazineWellPrototypeV1 {
    /// Concrete compatible MAGAZINE item type IDs in stable order.
    pub compatible_magazine_type_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PoweredToolStateV1 {
    pub inactive_type_id: String,
    pub active_type_id: String,
    pub activation_charges: u16,
    pub power_draw_milliwatts: u32,
    pub light_emission: u16,
    /// Whether light output scales linearly below one fifth of installed
    /// battery energy, matching pinned `CHARGEDIM` behavior.
    #[serde(default)]
    pub dims_with_charge: bool,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftItemPrototypeV1 {
    pub type_id: String,
    pub charges: i32,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    pub ammunition_type: String,
    pub ranged_weapon: Option<RangedWeaponSnapshot>,
    #[serde(default)]
    pub magazine_capacity: u32,
    #[serde(default)]
    pub magazine_well: Option<MagazineWellPrototypeV1>,
    #[serde(default)]
    pub residual_energy_millijoules: u32,
    #[serde(default)]
    pub powered_tool: Option<PoweredToolStateV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftByproductV1 {
    pub output_instances: u16,
    pub output: CraftItemPrototypeV1,
}

/// Server-normalized immutable recipe input stored in the command journal.
/// Gameplay clients submit only `recipe_id`; the server replaces any supplied
/// definition with the exact pinned definition before simulation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftRecipeV1 {
    pub recipe_id: String,
    pub time_moves: u64,
    pub output_instances: u16,
    pub output: CraftItemPrototypeV1,
    /// Pinned reversible non-charge crafts retain the exact consumed component
    /// objects on their primary results for later disassembly.
    #[serde(default)]
    pub retain_components: bool,
    /// Stable type-ID-sorted legacy byproducts. Their output IDs are reserved
    /// after the main result IDs when the craft starts.
    pub byproducts: Vec<CraftByproductV1>,
    pub components: Vec<Vec<CraftComponentRequirementV1>>,
    pub tools: Vec<Vec<CraftToolRequirementV1>>,
    pub qualities: Vec<Vec<CraftQualityRequirementV1>>,
    pub proficiencies: Vec<CraftProficiencyV1>,
    pub primary_skill: Option<CraftSkillRequirementV1>,
    pub required_skills: Vec<CraftSkillRequirementV1>,
    /// Whether theoretical skill requirements can independently supply recipe
    /// knowledge. Pinned `never_learn` affects explicit permanent learning,
    /// not this autolearn path.
    pub autolearn: bool,
    pub autolearn_skills: Vec<CraftSkillRequirementV1>,
    /// Stable BOOK-type-ID-sorted live knowledge sources. Books are checked at
    /// craft start; the knowledge check itself neither reserves nor consumes
    /// them.
    pub book_requirements: Vec<CraftBookRequirementV1>,
    /// Whether this definition may become permanent knowledge through the
    /// authoritative disassembly path.
    pub can_be_learned: bool,
}

impl CraftRecipeV1 {
    #[must_use]
    pub fn total_output_instances(&self) -> Option<u16> {
        self.byproducts
            .iter()
            .try_fold(self.output_instances, |total, byproduct| {
                total.checked_add(byproduct.output_instances)
            })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftConsumedItemV1 {
    pub item: ItemSnapshot,
    /// A charge split preserves the carried stack's identity and gives the
    /// reserved portion a new stable ID. Cancellation merges it back here.
    pub split_from: Option<ItemId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftActivitySnapshotV1 {
    pub recipe: CraftRecipeV1,
    /// One server-selected alternative index per recipe tool group, in group
    /// order. Interrupted crafts may replace these selections on resume.
    pub selected_tool_alternatives: Vec<u16>,
    pub remaining_action_points: u64,
    pub consumed_items: Vec<CraftConsumedItemV1>,
    pub reserved_output_items: Vec<ItemId>,
    pub previously_wielded: Option<ItemId>,
    /// Nominal one-second CDDA practice boundaries already awarded. Earned
    /// practice survives interruption and cancellation.
    pub practice_ticks_awarded: u64,
    /// Base-progress sub-action-points in fixed millionths, retained across
    /// ticks and recovery while proficiency speed changes.
    pub proficiency_progress_millionths: u32,
    /// Completed five-percent practice boundaries already awarded.
    pub proficiency_buckets_awarded: u8,
    pub interrupted: bool,
}

/// Exact tile mutation produced by a strict construction definition. Terrain
/// and furniture are independent layers, matching upstream `ter_set` and
/// `furn_set`; changing one preserves the other.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConstructionResultV1 {
    Terrain(TerrainTileSnapshot),
    Furniture(FurnitureTileSnapshot),
}

/// Server-normalized immutable construction input stored in the journal.
/// Gameplay clients submit only the stable construction ID and target tile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConstructionRecipeV1 {
    pub construction_id: String,
    pub name: String,
    pub time_moves: u64,
    pub required_skills: Vec<CraftSkillRequirementV1>,
    pub components: Vec<Vec<CraftComponentRequirementV1>>,
    /// Non-consuming item qualities that must remain available while work
    /// progresses. Providers are normalized from the pinned item catalog.
    #[serde(default)]
    pub qualities: Vec<Vec<CraftQualityRequirementV1>>,
    /// Exact terrain or furniture IDs accepted before work starts. Empty means
    /// no identity predicate beyond `requires_empty`.
    pub pre_terrain: Vec<String>,
    pub requires_empty: bool,
    pub result: ConstructionResultV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConstructionActivitySnapshotV1 {
    pub recipe: ConstructionRecipeV1,
    pub target: WorldPosition,
    pub remaining_action_points: u64,
    pub consumed_items: Vec<CraftConsumedItemV1>,
    pub previously_wielded: Option<ItemId>,
    pub interrupted: bool,
}

/// Server-normalized immutable skill-book input stored in the command journal.
/// The initial port models the pinned canonical-stat, default-focus, identified
/// physical-book path; clients never author these values.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BookStudyV1 {
    pub book_type_id: String,
    pub skill_id: String,
    pub required_skill_level: u8,
    pub maximum_skill_level: u8,
    /// Pinned BOOK intelligence threshold used by reading-time adjustment.
    pub intelligence_requirement: u16,
    /// Unadjusted pinned reading duration in upstream moves.
    pub time_moves: u64,
    /// Unadjusted whole minutes used by pinned reading-XP arithmetic.
    pub source_time_minutes: u32,
}

/// Applies pinned CDDA's low-Intelligence reading-time penalty using checked,
/// deterministic integer arithmetic. The input is the book's unadjusted time.
pub fn adjusted_book_study_time_moves(
    time_moves: u64,
    intelligence_requirement: u16,
    intelligence: u16,
) -> Option<u64> {
    if time_moves == 0
        || intelligence == 0
        || intelligence > MAX_ACTOR_BASE_STAT
        || intelligence_requirement > MAX_ACTOR_BASE_STAT
    {
        return None;
    }
    let penalty = u64::from(intelligence_requirement.saturating_sub(intelligence));
    time_moves
        .checked_mul(penalty)
        .map(|penalty_moves| penalty_moves / 60)
        .and_then(|penalty_moves| time_moves.checked_add(penalty_moves))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BookStudyActivitySnapshotV1 {
    pub study: BookStudyV1,
    pub book_item_id: ItemId,
    pub remaining_action_points: u64,
    /// The accepted start-command sequence names this study session's RNG.
    pub rng_sequence: CommandSequence,
    pub interrupted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisassemblyComponentV1 {
    pub output_instances: u16,
    pub count_by_charges: bool,
    pub output: CraftItemPrototypeV1,
    /// Exact crafted-component state. `None` uses the immutable default
    /// prototype above and keeps pre-provenance durable activities readable.
    #[serde(default)]
    pub output_state: Option<ItemComponentSnapshotV1>,
}

/// Server-normalized reversible recipe used by authoritative disassembly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisassemblyRecipeV1 {
    pub recipe_id: String,
    pub target_type_id: String,
    pub time_moves: u64,
    pub difficulty: u8,
    pub primary_skill_id: Option<String>,
    /// Stable skill-ID-sorted requirements for the one-in-four permanent
    /// learning roll. An empty set means the recipe cannot be learned here.
    pub learn_requirements: Vec<CraftSkillRequirementV1>,
    /// Whether this recipe becomes known automatically once its stable,
    /// skill-ID-sorted requirements are met. Disassembly learning must not
    /// create a redundant explicit learned-recipe entry in that case.
    pub autolearn: bool,
    pub autolearn_requirements: Vec<CraftSkillRequirementV1>,
    /// One deterministic default component per original recipe group.
    pub components: Vec<DisassemblyComponentV1>,
    pub tools: Vec<Vec<CraftToolRequirementV1>>,
    pub qualities: Vec<Vec<CraftQualityRequirementV1>>,
    /// Pinned charge-carrier item emitted before a supported bare ranged weapon
    /// or integral-charge tool is reserved. Its charges are replaced by the
    /// target's exact loaded count. `None` requires a target without modeled
    /// unloadable charges.
    #[serde(default)]
    pub unload_charges_as: Option<CraftItemPrototypeV1>,
    /// True when the target uses a charge-storage model that is not represented
    /// by this protocol version. Such a target may still be disassembled when
    /// its aggregate charge count is exactly zero, but the server must reject a
    /// charged instance instead of silently destroying its contents.
    #[serde(default)]
    pub requires_empty_charges: bool,
}

impl DisassemblyRecipeV1 {
    #[must_use]
    pub fn total_component_instances(&self) -> Option<u16> {
        self.components.iter().try_fold(0_u16, |total, component| {
            total.checked_add(component.output_instances)
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisassemblyActivitySnapshotV1 {
    pub recipe: DisassemblyRecipeV1,
    pub target_item: ItemSnapshot,
    pub selected_tool_alternatives: Vec<u16>,
    pub remaining_action_points: u64,
    pub reserved_component_items: Vec<ItemId>,
    pub previously_wielded: bool,
    /// The accepted start-command sequence names this disassembly session's
    /// recovery and learning RNG streams.
    pub rng_sequence: CommandSequence,
    pub interrupted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CommandKind {
    Move {
        dx: i8,
        dy: i8,
        dz: i8,
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
    },
    Wield {
        item_id: ItemId,
    },
    Unwield,
    PickUp {
        item_id: ItemId,
    },
    Drop {
        item_id: ItemId,
    },
    Consume {
        item_id: ItemId,
    },
    Activate {
        item_id: ItemId,
    },
    Craft {
        recipe_id: String,
        /// `None` is the untrusted client request shape. The authoritative
        /// server writes `Some` before the command enters simulation/journal.
        recipe: Option<Box<CraftRecipeV1>>,
    },
    ResumeCraft,
    CancelCraft,
    ReadBook {
        item_id: ItemId,
        book_type_id: String,
        /// `None` is the untrusted client request shape. The authoritative
        /// server writes `Some` before simulation and journaling.
        study: Option<Box<BookStudyV1>>,
    },
    ResumeRead,
    CancelRead,
    Disassemble {
        item_id: ItemId,
        item_type_id: String,
        /// `None` is the untrusted client request shape. The authoritative
        /// server replaces it before simulation and journaling.
        recipe: Option<Box<DisassemblyRecipeV1>>,
    },
    ResumeDisassembly,
    CancelDisassembly,
    Construct {
        target: WorldPosition,
        construction_id: String,
        /// `None` is the untrusted client request shape. The authoritative
        /// server replaces it before simulation and journaling.
        construction: Option<Box<ConstructionRecipeV1>>,
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
    Sleep,
    Wake,
    Wait,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QueuedActionSnapshot {
    pub sequence: CommandSequence,
    pub kind: CommandKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CommandRejection {
    UnknownActor,
    ActorDead,
    StaleSequence,
    InvalidMovement,
    Blocked,
    TargetMissing,
    TargetOutOfRange,
    ItemMissing,
    ItemNotHere,
    ItemNotOwned,
    InventoryFull,
    ItemNotConsumable,
    ItemNotActivatable,
    ItemHasNoPower,
    PoweredToolActive,
    InvalidTerrainInteraction,
    InvalidBashInteraction,
    InvalidBashTool,
    ActionQueueFull,
    WeaponNotRanged,
    WeaponEmpty,
    NoClearShot,
    WeaponFull,
    IncompatibleAmmunition,
    ActorSleeping,
    ActorAwake,
    NotTired,
    RecipeUnavailable,
    RecipeNotKnown,
    InsufficientSkills,
    MissingProficiencies,
    MissingComponents,
    MissingTools,
    MissingQualities,
    ActorBusy,
    NoCraftInProgress,
    CraftNotInterrupted,
    BookUnavailable,
    BookMastered,
    TooDarkToRead,
    NoReadInProgress,
    ReadNotInterrupted,
    DisassemblyUnavailable,
    ItemDamaged,
    TooDarkToDisassemble,
    NoDisassemblyInProgress,
    DisassemblyNotInterrupted,
    ConstructionUnavailable,
    InvalidConstructionTarget,
    TooDarkToConstruct,
    NoConstructionInProgress,
    ConstructionNotInterrupted,
    StableIdsUnavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SleepReason {
    Voluntary,
    Exhaustion,
    Autopilot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WakeReason {
    Voluntary,
    Rested,
    Damage,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BookStudyInterruptionReason {
    Damage,
    Needs,
    Exhaustion,
    Darkness,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DisassemblyInterruptionReason {
    Damage,
    Needs,
    Exhaustion,
    Darkness,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConstructionInterruptionReason {
    Damage,
    Needs,
    Exhaustion,
    Darkness,
    TargetChanged,
    MissingQualities,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PoweredToolTransitionReason {
    Activated,
    Deactivated,
    EnergyDepleted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldEvent {
    pub id: EventId,
    pub tick: SimTick,
    pub kind: WorldEventKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BashTargetKindV1 {
    Terrain,
    Furniture,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldEventKind {
    ActorMoved {
        actor_id: ActorId,
        from: WorldPosition,
        to: WorldPosition,
    },
    DamageApplied {
        source: ActorId,
        target: ActorId,
        amount: u16,
        remaining_hp: i32,
    },
    ActorDied {
        actor_id: ActorId,
        killer: ActorId,
    },
    CreatureMoved {
        creature_id: CreatureId,
        from: WorldPosition,
        to: WorldPosition,
    },
    CreatureDamaged {
        source: ActorId,
        target: CreatureId,
        amount: u16,
        remaining_hp: i32,
    },
    /// A fully resolved authoritative melee attempt that dealt no damage.
    ActorMissedCreature {
        source: ActorId,
        target: CreatureId,
    },
    CreatureDied {
        creature_id: CreatureId,
        killer: ActorId,
    },
    CreatureCorpseCreated {
        creature_id: CreatureId,
        corpse_item_id: ItemId,
        position: WorldPosition,
    },
    CreatureRevived {
        creature_id: CreatureId,
        corpse_item_id: ItemId,
        position: WorldPosition,
    },
    CreatureBashed {
        creature_id: CreatureId,
        target: WorldPosition,
        target_kind: BashTargetKindV1,
        target_type_id: String,
        success: bool,
        damage: u16,
        accumulated_damage: u16,
        sound: String,
        volume: u16,
    },
    ActorBashed {
        actor_id: ActorId,
        target: WorldPosition,
        target_kind: BashTargetKindV1,
        target_type_id: String,
        success: bool,
        damage: u16,
        accumulated_damage: u16,
        sound: String,
        volume: u16,
    },
    CreatureOpenedTerrain {
        creature_id: CreatureId,
        position: WorldPosition,
        from: String,
        to: String,
        sound: String,
        volume: u16,
    },
    FieldIntensityChanged {
        position: WorldPosition,
        field_type_id: String,
        intensity: u8,
    },
    ActorDamagedByCreature {
        source: CreatureId,
        target: ActorId,
        amount: u16,
        remaining_hp: i32,
    },
    CreatureMissedActor {
        source: CreatureId,
        target: ActorId,
        stumbled: bool,
    },
    ActorKilledByCreature {
        actor_id: ActorId,
        killer: CreatureId,
    },
    CommandRejected {
        actor_id: ActorId,
        sequence: CommandSequence,
        reason: CommandRejection,
    },
    ConnectionChanged {
        actor_id: ActorId,
        connected: bool,
    },
    ItemPickedUp {
        actor_id: ActorId,
        item_id: ItemId,
        position: WorldPosition,
    },
    ItemDropped {
        actor_id: ActorId,
        item_id: ItemId,
        position: WorldPosition,
    },
    ItemWielded {
        actor_id: ActorId,
        item_id: Option<ItemId>,
    },
    ItemConsumed {
        actor_id: ActorId,
        item_id: ItemId,
        remaining_charges: i32,
        stored_kcal: i32,
        thirst: i32,
    },
    ActorNeedsUpdated {
        actor_id: ActorId,
        stored_kcal: i32,
        thirst: i32,
        sleepiness: i32,
        sleeping: bool,
    },
    ActorDiedFromNeeds {
        actor_id: ActorId,
    },
    TerrainChanged {
        actor_id: ActorId,
        position: WorldPosition,
        from: String,
        to: String,
    },
    RangedAttackResolved {
        source: ActorId,
        weapon: ItemId,
        origin: WorldPosition,
        target: RangedTarget,
        hit: bool,
        remaining_ammunition: u16,
        sound: String,
        sound_volume: u16,
    },
    WeaponReloaded {
        actor_id: ActorId,
        weapon: ItemId,
        ammunition_item: ItemId,
        loaded: u16,
        ammunition_remaining: u16,
        source_charges_remaining: i32,
    },
    MagazineReloaded {
        actor_id: ActorId,
        tool: ItemId,
        magazine: ItemId,
        ejected_magazine: Option<ItemId>,
        charges: i32,
    },
    PoweredToolChanged {
        actor_id: Option<ActorId>,
        item_id: ItemId,
        active: bool,
        reason: PoweredToolTransitionReason,
        available_energy_millijoules: u64,
    },
    ActorFellAsleep {
        actor_id: ActorId,
        reason: SleepReason,
    },
    ActorWokeUp {
        actor_id: ActorId,
        reason: WakeReason,
    },
    CraftStarted {
        actor_id: ActorId,
        recipe_id: String,
        total_action_points: u64,
    },
    CraftInterrupted {
        actor_id: ActorId,
        recipe_id: String,
    },
    CraftResumed {
        actor_id: ActorId,
        recipe_id: String,
    },
    CraftCanceled {
        actor_id: ActorId,
        recipe_id: String,
    },
    CraftCompleted {
        actor_id: ActorId,
        recipe_id: String,
        output_items: Vec<ItemId>,
    },
    BookStudyStarted {
        actor_id: ActorId,
        book_item_id: ItemId,
        skill_id: String,
        total_action_points: u64,
    },
    BookStudyInterrupted {
        actor_id: ActorId,
        book_item_id: ItemId,
        reason: BookStudyInterruptionReason,
    },
    BookStudyResumed {
        actor_id: ActorId,
        book_item_id: ItemId,
    },
    BookStudyCanceled {
        actor_id: ActorId,
        book_item_id: ItemId,
    },
    BookStudyCompleted {
        actor_id: ActorId,
        book_item_id: ItemId,
        skill_id: String,
        experience_gained: u32,
        theoretical_level: u8,
        theoretical_experience: u32,
    },
    DisassemblyStarted {
        actor_id: ActorId,
        target_item_id: ItemId,
        recipe_id: String,
        total_action_points: u64,
    },
    DisassemblyInterrupted {
        actor_id: ActorId,
        target_item_id: ItemId,
        reason: DisassemblyInterruptionReason,
    },
    DisassemblyResumed {
        actor_id: ActorId,
        target_item_id: ItemId,
    },
    DisassemblyCanceled {
        actor_id: ActorId,
        target_item_id: ItemId,
    },
    DisassemblyCompleted {
        actor_id: ActorId,
        target_item_id: ItemId,
        recipe_id: String,
        recovered_items: Vec<ItemId>,
        destroyed_components: Vec<DisassemblyDestroyedComponentV1>,
    },
    ConstructionStarted {
        actor_id: ActorId,
        construction_id: String,
        target: WorldPosition,
        total_action_points: u64,
    },
    ConstructionInterrupted {
        actor_id: ActorId,
        construction_id: String,
        target: WorldPosition,
        reason: ConstructionInterruptionReason,
    },
    ConstructionResumed {
        actor_id: ActorId,
        construction_id: String,
        target: WorldPosition,
    },
    ConstructionCanceled {
        actor_id: ActorId,
        construction_id: String,
        target: WorldPosition,
    },
    ConstructionCompleted {
        actor_id: ActorId,
        construction_id: String,
        target: WorldPosition,
    },
    RecipeLearned {
        actor_id: ActorId,
        recipe_id: String,
    },
    CraftToolChargesConsumed {
        actor_id: ActorId,
        item_id: ItemId,
        charges: u32,
        remaining_charges: i32,
    },
    SkillLevelGained {
        actor_id: ActorId,
        skill_id: String,
        practical_level: u8,
        theoretical_level: u8,
    },
    ProficiencyLearned {
        actor_id: ActorId,
        proficiency_id: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RangedTarget {
    Actor(ActorId),
    Creature(CreatureId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RangedWeaponSnapshot {
    pub ammunition_type: String,
    pub ammunition_remaining: u16,
    pub ammunition_capacity: u16,
    pub range: u16,
    pub damage: u16,
    pub dispersion: u16,
    /// Authoritative single-shot volume after pinned gun and ammunition
    /// loudness finalization. Zero is silent.
    pub sound_volume: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreatureSoundGoalV1 {
    pub position: WorldPosition,
    /// Remaining creature actions, matching upstream's decrement-per-move
    /// `wandf` behavior rather than wall-clock expiration.
    pub remaining_actions: u32,
}

/// Strict projection of the pathfinding settings observed in the pinned
/// MONSTER corpus. A zero distance disables route search. The current corpus
/// has no explicit `max_length`, so runtime uses upstream's finalized
/// `max_distance * 5` budget.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreaturePathSettingsV1 {
    pub max_distance: u16,
    pub allow_open_doors: bool,
    pub avoid_traps: bool,
    pub avoid_sharp: bool,
    pub avoid_dangerous_fields: bool,
    pub allow_climb_stairs: bool,
}

impl Default for CreaturePathSettingsV1 {
    fn default() -> Self {
        Self {
            max_distance: 0,
            allow_open_doors: false,
            avoid_traps: false,
            avoid_sharp: false,
            avoid_dangerous_fields: false,
            allow_climb_stairs: true,
        }
    }
}

/// Final base size derived from a pinned monster type's inherited volume.
/// Runtime size-changing effects are not yet admitted by the canonical model.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum CreatureSizeV1 {
    Tiny,
    Small,
    #[default]
    Medium,
    Large,
    Huge,
}

const fn default_creature_attack_cost_moves() -> u16 {
    100
}

/// Static ordinary-corpse and revival data copied from the pinned monster type.
/// Keeping it with the runtime object lets replay revive a corpse without
/// consulting process-local content registries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreatureCorpsePrototypeV1 {
    pub monster_type_id: String,
    pub max_hp: i32,
    pub speed: u16,
    /// Final inherited moves spent by an ordinary melee attack.
    #[serde(default = "default_creature_attack_cost_moves")]
    pub attack_cost_moves: u16,
    pub aggression: i16,
    /// Final inherited monster accuracy stat from pinned content.
    #[serde(default)]
    pub melee_skill: u16,
    /// Final inherited monster dodge stat from pinned content.
    #[serde(default)]
    pub dodge: u16,
    /// Private immutable base size derived from final inherited volume.
    #[serde(default)]
    pub size: CreatureSizeV1,
    pub melee_dice: u16,
    pub melee_dice_sides: u16,
    pub can_see: bool,
    pub vision_day: u16,
    pub vision_night: u16,
    pub stumbles: bool,
    pub bashes: bool,
    pub group_bash: bool,
    pub hears: bool,
    pub good_hearing: bool,
    /// Whether a failed ordinary melee attack can knock this monster down.
    #[serde(default)]
    pub clumsy_attacks: bool,
    /// Static pinned `IMMOBILE` capability. Dynamic `CANNOT_MOVE` effects are
    /// outside this version's canonical effect model.
    #[serde(default)]
    pub immobile: bool,
    /// Static pinned `PACIFIST` capability. It suppresses ordinary melee but
    /// does not imply immobility or suppress special attacks.
    #[serde(default)]
    pub pacifist: bool,
    pub can_open_doors: bool,
    pub path_settings: CreaturePathSettingsV1,
    pub blood_field_type_id: String,
    pub revives: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreatureCorpseSnapshotV1 {
    pub prototype: CreatureCorpsePrototypeV1,
    pub death_tick: SimTick,
    /// Upstream gives one in twenty reviving corpses a proximity-gated rise.
    pub revive_special: bool,
    /// False for a corpse damaged enough to be incapable of revival.
    pub revivable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemSnapshot {
    pub id: ItemId,
    pub type_id: String,
    pub charges: i32,
    pub damage: u16,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    pub ammunition_type: String,
    pub ranged_weapon: Option<RangedWeaponSnapshot>,
    /// `None` means an ordinary item whose disassembly uses recipe defaults.
    /// `Some` retains the exact components of a reversible crafted item;
    /// `Some([])` deliberately means it was crafted from no recoverable output.
    #[serde(default)]
    pub component_provenance: Option<Vec<ItemComponentSnapshotV1>>,
    /// Capacity of a concrete MAGAZINE item. Its charge category is
    /// `ammunition_type`; unlike loose ammunition, an empty magazine remains a
    /// real stable item.
    #[serde(default)]
    pub magazine_capacity: u32,
    /// The first canonical detachable-magazine boundary. Installed contents
    /// retain their own stable item identity.
    #[serde(default)]
    pub magazine_well: Option<MagazineWellSnapshotV1>,
    /// Sub-charge battery energy retained after continuous draw. One integer
    /// battery charge is exactly one kilojoule.
    #[serde(default)]
    pub residual_energy_millijoules: u32,
    #[serde(default)]
    pub powered_tool: Option<PoweredToolStateV1>,
    /// `Some` turns the ordinary `corpse` item into a creature-specific corpse.
    #[serde(default)]
    pub creature_corpse: Option<CreatureCorpseSnapshotV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MagazineWellSnapshotV1 {
    pub compatible_magazine_type_ids: Vec<String>,
    pub installed_magazine: Option<Box<ItemSnapshot>>,
}

/// A component item retained inside a crafted result. It intentionally has no
/// world-stable ID while nested; recovery allocates a new stable ID before the
/// parent disassembly starts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemComponentSnapshotV1 {
    pub type_id: String,
    pub charges: i32,
    pub damage: u16,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    pub ammunition_type: String,
    pub ranged_weapon: Option<RangedWeaponSnapshot>,
    pub count_by_charges: bool,
    pub recoverable: bool,
    pub component_provenance: Option<Vec<ItemComponentSnapshotV1>>,
    #[serde(default)]
    pub magazine_capacity: u32,
    /// Retains an empty detachable-magazine well in crafted provenance.
    /// Crafting admission rejects installed contents until general nested
    /// component containment is implemented.
    #[serde(default)]
    pub magazine_well: Option<MagazineWellPrototypeV1>,
    #[serde(default)]
    pub residual_energy_millijoules: u32,
    #[serde(default)]
    pub powered_tool: Option<PoweredToolStateV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DisassemblyDestroyedComponentV1 {
    pub type_id: String,
    pub count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GroundItemSnapshot {
    pub item: ItemSnapshot,
    pub position: WorldPosition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SkillLevelSnapshot {
    pub skill_id: String,
    pub practical_level: u8,
    /// Raw CDDA exercise points, not the displayed percentage.
    pub practical_experience: u32,
    pub theoretical_level: u8,
    /// Raw CDDA knowledge experience points.
    pub theoretical_experience: u32,
    pub last_practiced: SimTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProficiencyLevelSnapshot {
    pub proficiency_id: String,
    pub practiced_action_points: u64,
    pub practice_remainder_millionths: u32,
    pub learned: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorSnapshot {
    pub id: ActorId,
    pub position: WorldPosition,
    pub hp: i32,
    /// Base Strength. Until limb anatomy lands, a healthy actor's arm-strength
    /// modifier is exactly one and structural smashing derives from this.
    pub base_strength: u16,
    pub base_dexterity: u16,
    pub base_intelligence: u16,
    pub base_perception: u16,
    pub connected: bool,
    pub last_command_sequence: CommandSequence,
    pub last_held_input_sequence: HeldInputSequence,
    pub held_movement: Option<HorizontalDirection>,
    pub inventory: Vec<ItemSnapshot>,
    pub wielded: Option<ItemId>,
    pub stored_kcal: i32,
    pub thirst: i32,
    pub sleepiness: i32,
    pub sleeping: bool,
    pub sleep_intervals: u16,
    pub speed: u16,
    pub action_points: i64,
    pub queued_actions: Vec<QueuedActionSnapshot>,
    pub craft_activity: Option<CraftActivitySnapshotV1>,
    pub read_activity: Option<BookStudyActivitySnapshotV1>,
    pub disassembly_activity: Option<DisassemblyActivitySnapshotV1>,
    #[serde(default)]
    pub construction_activity: Option<ConstructionActivitySnapshotV1>,
    /// Stable recipe-ID-sorted permanent knowledge. Autolearn and carried-book
    /// availability remain derived rather than duplicated here.
    pub learned_recipes: Vec<String>,
    /// Sorted by stable skill ID; absent entries are canonical level zero.
    pub skills: Vec<SkillLevelSnapshot>,
    /// Sorted by stable proficiency ID; absent entries are unpracticed.
    pub proficiencies: Vec<ProficiencyLevelSnapshot>,
    pub map_memory: Vec<MemorizedChunkSnapshot>,
}

/// Administrator-only canonical character state. Terrain memory is represented
/// only by its chunk count and inventory is paginated so this always fits the
/// bounded control stream.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivateCharacterInspection {
    pub tick: SimTick,
    pub account_id: AccountId,
    pub actor_id: ActorId,
    pub name: String,
    pub position: WorldPosition,
    pub hp: i32,
    pub base_strength: u16,
    pub base_dexterity: u16,
    pub base_intelligence: u16,
    pub base_perception: u16,
    pub connected: bool,
    pub last_command_sequence: CommandSequence,
    pub last_held_input_sequence: HeldInputSequence,
    pub held_movement: Option<HorizontalDirection>,
    pub wielded: Option<ItemId>,
    pub stored_kcal: i32,
    pub thirst: i32,
    pub sleepiness: i32,
    pub sleeping: bool,
    pub sleep_intervals: u16,
    pub speed: u16,
    pub action_points: i64,
    pub queued_actions: Vec<QueuedActionSnapshot>,
    pub craft_activity: Option<CraftActivitySnapshotV1>,
    pub read_activity: Option<BookStudyActivitySnapshotV1>,
    pub disassembly_activity: Option<DisassemblyActivitySnapshotV1>,
    #[serde(default)]
    pub construction_activity: Option<ConstructionActivitySnapshotV1>,
    pub learned_recipe_count: u16,
    pub skills: Vec<SkillLevelSnapshot>,
    pub proficiencies: Vec<ProficiencyLevelSnapshot>,
    pub inventory_total: u16,
    pub inventory: Vec<ItemSnapshot>,
    pub next_inventory_after: Option<ItemId>,
    pub map_memory_chunks: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreatureSnapshot {
    pub id: CreatureId,
    pub type_id: String,
    pub position: WorldPosition,
    pub hp: i32,
    pub max_hp: i32,
    pub speed: u16,
    /// Private authoritative ordinary melee move cost.
    #[serde(default = "default_creature_attack_cost_moves")]
    pub attack_cost_moves: u16,
    pub aggression: i16,
    /// Private authoritative monster accuracy; omitted from visible DTOs.
    #[serde(default)]
    pub melee_skill: u16,
    /// Private authoritative monster dodge; omitted from visible DTOs.
    #[serde(default)]
    pub dodge: u16,
    /// Private authoritative base size; omitted from visible DTOs.
    #[serde(default)]
    pub size: CreatureSizeV1,
    pub melee_dice: u16,
    pub melee_dice_sides: u16,
    pub can_see: bool,
    pub vision_day: u16,
    pub vision_night: u16,
    pub stumbles: bool,
    pub bashes: bool,
    pub group_bash: bool,
    pub hears: bool,
    pub good_hearing: bool,
    /// Private authoritative failed-attack consequence capability.
    #[serde(default)]
    pub clumsy_attacks: bool,
    /// Private authoritative static `IMMOBILE` capability.
    #[serde(default)]
    pub immobile: bool,
    /// Private authoritative static `PACIFIST` capability.
    #[serde(default)]
    pub pacifist: bool,
    pub can_open_doors: bool,
    pub path_settings: CreaturePathSettingsV1,
    /// Last currently or previously seen destination. This is canonical AI
    /// state and is deliberately absent from the public creature DTO.
    pub goal: Option<WorldPosition>,
    /// Private imprecise destination inferred from a recent authoritative
    /// sound. This never appears in `VisibleCreatureSnapshot`.
    pub sound_goal: Option<CreatureSoundGoalV1>,
    /// Signed readiness. Expensive movement leaves deterministic debt that must
    /// be recovered before the creature can act again.
    pub action_points: i64,
    /// Private down-effect deadline. Revival uses five seconds; an admitted
    /// clumsy attack miss uses two seconds.
    pub downed_until_tick: Option<SimTick>,
    /// Empty means this creature leaves no splatter on ordinary death.
    pub blood_field_type_id: String,
    /// `None` means this runtime creature has no modeled ordinary corpse.
    pub corpse: Option<CreatureCorpsePrototypeV1>,
}

/// Public state for a currently visible creature. AI intent, action debt,
/// combat internals, blood, and corpse reconstruction data are omitted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleCreatureSnapshot {
    pub id: CreatureId,
    pub type_id: String,
    pub position: WorldPosition,
    pub hp: i32,
    pub max_hp: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldIntensityLevelV1 {
    pub name: String,
    pub symbol: String,
    pub color: String,
    pub dangerous: bool,
    pub transparent: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldTypeSnapshotV1 {
    pub field_type_id: String,
    pub intensity_levels: Vec<FieldIntensityLevelV1>,
    pub priority: i32,
    pub half_life_seconds: u64,
    pub linear_half_life: bool,
    pub is_splattering: bool,
    pub display_field: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldSnapshotV1 {
    pub field_type_id: String,
    pub intensity: u8,
    pub age_seconds: u64,
    pub display_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldObservationV1 {
    pub field_type_id: String,
    pub intensity: u8,
    pub name: String,
    pub symbol: String,
    pub color: String,
    pub dangerous: bool,
    pub transparent: bool,
    pub priority: i32,
    pub display_field: bool,
    pub display_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChunkSnapshot {
    pub coord: ChunkCoord,
    pub revision: u64,
    pub tiles: Vec<TerrainTileSnapshot>,
    pub furniture: Vec<Option<FurnitureTileSnapshot>>,
    /// One field-type-ID-sorted collection per tile.
    pub fields: Vec<Vec<FieldSnapshotV1>>,
    /// Semi-persistent structural bash damage, one integer value per tile.
    pub map_damage: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerrainTileSnapshot {
    pub terrain_id: String,
    pub move_cost: i32,
    pub transparent: bool,
    pub flat: bool,
    pub open: String,
    pub open_move_cost: Option<i32>,
    pub open_transparent: Option<bool>,
    pub open_flat: Option<bool>,
    pub close: String,
    pub close_move_cost: Option<i32>,
    pub close_transparent: Option<bool>,
    pub close_flat: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FurnitureTileSnapshot {
    pub furniture_id: String,
    pub move_cost_mod: i32,
    pub transparent: bool,
    pub blocks_door: bool,
    pub comfort: i32,
    pub floor_bedding_warmth: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BashFieldEffectV1 {
    pub field_type_id: String,
    pub intensity: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BashDropPrototypeV1 {
    pub prototype: CraftItemPrototypeV1,
    pub probability_percent: u8,
    pub count_min: u16,
    pub count_max: u16,
    pub charges_min: Option<i32>,
    pub charges_max: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerrainBashTypeV1 {
    pub terrain_id: String,
    pub str_min: i32,
    pub str_max: i32,
    pub str_min_blocked: i32,
    pub str_max_blocked: i32,
    pub str_min_supported: i32,
    pub str_max_supported: i32,
    pub bash_multiplier_millionths: u32,
    pub result: TerrainTileSnapshot,
    pub drops: Vec<BashDropPrototypeV1>,
    pub hit_field: Option<BashFieldEffectV1>,
    pub destroyed_field: Option<BashFieldEffectV1>,
    pub sound: String,
    pub failure_sound: String,
    pub sound_volume: i32,
    pub failure_sound_volume: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FurnitureBashTypeV1 {
    pub furniture_id: String,
    pub str_min: i32,
    pub str_max: i32,
    pub str_min_blocked: i32,
    pub str_max_blocked: i32,
    pub str_min_supported: i32,
    pub str_max_supported: i32,
    pub bash_multiplier_millionths: u32,
    pub result: Option<FurnitureTileSnapshot>,
    pub drops: Vec<BashDropPrototypeV1>,
    pub hit_field: Option<BashFieldEffectV1>,
    pub destroyed_field: Option<BashFieldEffectV1>,
    pub sound: String,
    pub failure_sound: String,
    pub sound_volume: i32,
    pub failure_sound_volume: i32,
}

/// Strict pinned item subset that can participate in player structural
/// smashing without consulting mutable client content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SmashItemTypeV1 {
    pub item_type_id: String,
    pub bash_damage: u16,
    /// Pinned `item::attack_time` before the smash action's 80% multiplier.
    pub attack_time_moves: u16,
    /// Finalized pinned `item::get_to_hit` for ordinary unmodified instances.
    #[serde(default = "default_smash_item_melee_to_hit")]
    pub melee_to_hit: i16,
}

const fn default_smash_item_melee_to_hit() -> i16 {
    -2
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemorizedTileSnapshot {
    pub terrain: TerrainTileSnapshot,
    pub furniture: Option<FurnitureTileSnapshot>,
}

/// Sparse per-character terrain memory for one CDDA submap. `None` means the
/// character has never perceived that tile; dynamic entities are deliberately
/// not remembered here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemorizedChunkSnapshot {
    pub coord: ChunkCoord,
    pub tiles: Vec<Option<MemorizedTileSnapshot>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ObservedTerrainSnapshot {
    pub terrain: TerrainTileSnapshot,
    pub furniture: Option<FurnitureTileSnapshot>,
    /// The authoritative structural layer a smash would currently target.
    /// Furniture takes precedence over terrain. Hidden remembered tiles never
    /// expose this live interaction metadata.
    pub bash_target: Option<BashTargetKindV1>,
    /// Dynamic fields are present only while the tile is currently visible.
    pub fields: Vec<FieldObservationV1>,
    pub currently_visible: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldSnapshotV1 {
    pub world_namespace: u64,
    pub world_seed: [u8; 32],
    pub tick: SimTick,
    pub allocator_high_water: u64,
    pub allocator_next: u64,
    pub allocator_reserved_end: u64,
    pub next_event_counter: u64,
    pub next_field_sequence: u64,
    /// Field-type-ID-sorted simulation definitions admitted from pinned data.
    pub field_types: Vec<FieldTypeSnapshotV1>,
    /// Stable terrain/furniture-ID-sorted authoritative bash definitions.
    pub terrain_bash_types: Vec<TerrainBashTypeV1>,
    /// Furniture-ID-sorted set of every pinned furniture definition with an
    /// upstream bash body. Runtime-admitted definitions are a strict subset;
    /// an unsupported body still blocks a smash from reaching the terrain.
    pub furniture_bash_ids: Vec<String>,
    pub furniture_bash_types: Vec<FurnitureBashTypeV1>,
    /// Item-type-ID-sorted strict player-smashing profiles.
    pub smash_item_types: Vec<SmashItemTypeV1>,
    pub worldgen_default_terrain: Option<TerrainTileSnapshot>,
    pub actors: Vec<ActorSnapshot>,
    pub creatures: Vec<CreatureSnapshot>,
    pub ground_items: Vec<GroundItemSnapshot>,
    pub chunks: Vec<ChunkSnapshot>,
}

/// Public information about another actor. Private inventory, needs, command,
/// and equipment state is deliberately absent from client replication.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleActorSnapshot {
    pub id: ActorId,
    pub position: WorldPosition,
    pub hp: i32,
    pub connected: bool,
    pub sleeping: bool,
}

/// One interest-managed chunk with an authoritative visibility mask. A `None`
/// tile is not currently perceived and must not be inferred by the client.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleChunkSnapshot {
    pub coord: ChunkCoord,
    pub tiles: Vec<Option<ObservedTerrainSnapshot>>,
}

/// The client-facing state DTO. Canonical persistence metadata such as the
/// world seed, namespace, allocator, and event sequence never crosses this
/// boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReplicationSnapshotV1 {
    pub tick: SimTick,
    pub calendar: CalendarSnapshot,
    pub natural_light: NaturalLightSnapshot,
    /// Server-derived fine-detail light at the controlled actor's tile.
    pub detail_vision_available: bool,
    pub controlled_actor: ActorSnapshot,
    pub visible_actors: Vec<VisibleActorSnapshot>,
    pub creatures: Vec<VisibleCreatureSnapshot>,
    pub ground_items: Vec<GroundItemSnapshot>,
    pub chunks: Vec<VisibleChunkSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChatMessage {
    pub from_actor: ActorId,
    pub from_character: String,
    pub text: String,
    pub tick: SimTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ControlMessage {
    EnrollmentRequest {
        protocol_version: u16,
    },
    EnrollmentAccepted(EnrollmentAccepted),
    EnrollmentRejected(EnrollmentRejection),
    CharacterRequest(CharacterRequest),
    CharacterList(Vec<CharacterSummary>),
    CharacterReady {
        actor_id: ActorId,
    },
    GameplayRejected(GameplayRejection),
    AccountKeyRequest(AccountKeyRequest),
    AccountKeyResponse(AccountKeyResponse),
    AdminHello(AdminHello),
    AdminRequest(AdminRequest),
    AdminResponse(AdminResponse),
    ReportSubmit(PlayerReport),
    ReportResponse(ReportResponse),
    ClientHello(ClientHello),
    ServerHello(ServerHello),
    Command(ClientCommand),
    EventStreamReady {
        actor_id: ActorId,
    },
    SnapshotStreamReady {
        actor_id: ActorId,
        sequence: u64,
        tick: SimTick,
        encoded_length: u32,
        decoded_length: u32,
    },
    Events(Vec<WorldEvent>),
    ChatSend {
        text: String,
    },
    ChatReceived(ChatMessage),
    ChatRejected(ChatRejection),
    Heartbeat {
        tick: SimTick,
    },
}

impl ControlMessage {
    fn validate(&self) -> Result<(), FrameError> {
        match self {
            Self::EnrollmentAccepted(accepted) if !valid_display_name(&accepted.display_name) => {
                Err(FrameError::InvalidBounds)
            }
            Self::CharacterRequest(CharacterRequest::Create { name, base_stats })
                if !valid_character_name(name) || !base_stats.is_valid() =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::CharacterList(characters)
                if characters.len() > MAX_CHARACTERS_PER_ACCOUNT
                    || characters
                        .iter()
                        .any(|character| !valid_character_name(&character.name)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::ReportSubmit(report)
                if report.target_actor.counter() == 0 || !valid_report_details(&report.details) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::ReportResponse(ReportResponse::Accepted { report_id })
                if !valid_db_sequence(report_id.0) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AccountKeyResponse(AccountKeyResponse::Bindings(bindings))
                if bindings.len() > 256
                    || bindings.iter().any(|binding| !valid_binding(binding)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AccountKeyResponse(AccountKeyResponse::Pending(binding))
                if binding.state != EndpointBindingState::Pending
                    || !binding
                        .pending_expires_utc
                        .is_some_and(|expires| expires > 0) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(AdminRequest::ListAccounts { limit, .. })
                if *limit == 0 || *limit > MAX_ADMIN_ACCOUNTS_PER_PAGE =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(AdminRequest::InspectCharacter {
                actor_id,
                inventory_after,
                inventory_limit,
            }) if actor_id.counter() == 0
                || *inventory_limit == 0
                || *inventory_limit > MAX_ADMIN_INVENTORY_PER_PAGE
                || inventory_after.is_some_and(|item_id| {
                    item_id.counter() == 0
                        || item_id.world_namespace() != actor_id.world_namespace()
                }) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(AdminRequest::ListReports { after, limit, .. })
                if *limit == 0
                    || *limit > MAX_REPORTS_PER_PAGE
                    || after.is_some_and(|report_id| !valid_db_sequence(report_id.0)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(AdminRequest::SetReportState { report_id, state })
                if !valid_db_sequence(report_id.0) || *state == ReportState::Open =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(AdminRequest::CreateAccount { display_name, .. })
                if !valid_display_name(display_name) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(
                AdminRequest::ListEndpoints { account_id }
                | AdminRequest::AddEndpoint { account_id, .. }
                | AdminRequest::RevokeEndpoint { account_id, .. },
            ) if account_id.counter() == 0 => Err(FrameError::InvalidBounds),
            Self::AdminRequest(AdminRequest::ListModerationHistory { after, limit, .. })
                if *limit == 0
                    || *limit > MAX_MODERATION_HISTORY_PER_PAGE
                    || after.is_some_and(|history_id| !valid_db_sequence(history_id)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(AdminRequest::SetStatus { status, .. })
                if !matches!(
                    status,
                    AccountStatus::Enabled | AccountStatus::Disabled | AccountStatus::Banned
                ) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminRequest(
                AdminRequest::SetSuspension {
                    duration_seconds: Some(duration),
                    ..
                }
                | AdminRequest::SetMute {
                    duration_seconds: Some(duration),
                    ..
                },
            ) if *duration == 0 || *duration > MAX_MODERATION_DURATION_SECONDS => {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::Accounts { accounts, .. })
                if accounts.len() > usize::from(MAX_ADMIN_ACCOUNTS_PER_PAGE)
                    || accounts.iter().any(|account| !valid_admin_account(account)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::AccountUpdated(account))
                if !valid_admin_account(account) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::PrivateCharacter(character))
                if !valid_private_character_inspection(character) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::AccountCreated {
                account,
                pending_endpoint,
            }) if !valid_admin_account(account)
                || account.status != AccountStatus::InitialEnrollment
                || account.suspended_until_utc.is_some()
                || account.muted_until_utc.is_some()
                || pending_endpoint.state != EndpointBindingState::Pending
                || !valid_binding(pending_endpoint) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::Endpoints {
                account_id,
                bindings,
            }) if account_id.counter() == 0
                || bindings.len() > 256
                || bindings.iter().any(|binding| !valid_binding(binding)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::EndpointPending {
                account_id,
                binding,
            }) if account_id.counter() == 0
                || binding.state != EndpointBindingState::Pending
                || !valid_binding(binding) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::EndpointRevoked { account_id, .. })
                if account_id.counter() == 0 =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::ModerationApplied {
                account,
                kind,
                until_utc,
            }) if !valid_admin_account(account)
                || match kind {
                    ModerationKind::Kick => until_utc.is_some(),
                    ModerationKind::Suspension | ModerationKind::Mute => {
                        until_utc.is_some_and(|until| until <= 0)
                    }
                } =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::Characters {
                account_id,
                characters,
                gameplay_session_active,
                controlled_actor,
            }) if characters.len() > MAX_CHARACTERS_PER_ACCOUNT
                || account_id.counter() == 0
                || characters.iter().any(|character| {
                    character.actor_id.counter() == 0
                        || character.actor_id.world_namespace() != account_id.world_namespace()
                        || !valid_character_name(&character.name)
                })
                || (!gameplay_session_active && controlled_actor.is_some())
                || controlled_actor.is_some_and(|actor_id| {
                    !characters
                        .iter()
                        .any(|character| character.actor_id == actor_id)
                }) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::Reports {
                reports,
                next_after,
            }) if reports.len() > usize::from(MAX_REPORTS_PER_PAGE)
                || reports.iter().any(|report| !valid_report_summary(report))
                || next_after.is_some_and(|report_id| !valid_db_sequence(report_id.0)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::ReportUpdated(report))
                if !valid_report_summary(report) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::ModerationHistory {
                entries,
                next_after,
                ..
            }) if entries.len() > usize::from(MAX_MODERATION_HISTORY_PER_PAGE)
                || entries.iter().any(|entry| !valid_moderation_history(entry))
                || next_after.is_some_and(|history_id| !valid_db_sequence(history_id)) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::AdminResponse(AdminResponse::Ready { role, .. })
                if *role == AccountRole::Player =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::ChatRejected(ChatRejection::Muted { until_utc }) if *until_utc <= 0 => {
                Err(FrameError::InvalidBounds)
            }
            Self::ClientHello(hello) => validate_content(&hello.content),
            Self::ServerHello(hello) => validate_content(&hello.content),
            Self::Command(command) if !valid_client_command(command) => {
                Err(FrameError::InvalidBounds)
            }
            Self::ChatSend { text } if !valid_chat_text(text) => Err(FrameError::InvalidBounds),
            Self::ChatReceived(message)
                if !valid_character_name(&message.from_character)
                    || !valid_chat_text(&message.text) =>
            {
                Err(FrameError::InvalidBounds)
            }
            Self::Events(events) if events.len() > 4_096 => Err(FrameError::InvalidBounds),
            Self::SnapshotStreamReady {
                encoded_length,
                decoded_length,
                ..
            } if *encoded_length as usize > MAX_BULK_ENCODED
                || *decoded_length as usize > MAX_BULK_DECODED =>
            {
                Err(FrameError::InvalidBounds)
            }
            _ => Ok(()),
        }
    }
}

fn valid_admin_account(account: &AdminAccountSummary) -> bool {
    account.account_id.counter() > 0
        && valid_display_name(&account.display_name)
        && account.suspended_until_utc.is_none_or(|until| until > 0)
        && account.muted_until_utc.is_none_or(|until| until > 0)
}

fn valid_private_character_inspection(character: &PrivateCharacterInspection) -> bool {
    let inventory_is_ordered = character
        .inventory
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id);
    let namespace = character.actor_id.world_namespace();
    let mut item_ids = BTreeSet::new();
    let stable_item_ids_are_valid =
        character
            .inventory
            .iter()
            .all(|item| collect_stable_item_ids(item, namespace, &mut item_ids))
            && character.craft_activity.as_ref().is_none_or(|activity| {
                activity.consumed_items.iter().all(|consumed| {
                    collect_stable_item_ids(&consumed.item, namespace, &mut item_ids)
                }) && activity.reserved_output_items.iter().all(|item_id| {
                    item_id.counter() > 0
                        && item_id.world_namespace() == namespace
                        && item_ids.insert(*item_id)
                })
            })
            && character
                .disassembly_activity
                .as_ref()
                .is_none_or(|activity| {
                    collect_stable_item_ids(&activity.target_item, namespace, &mut item_ids)
                        && activity.reserved_component_items.iter().all(|item_id| {
                            item_id.counter() > 0
                                && item_id.world_namespace() == namespace
                                && item_ids.insert(*item_id)
                        })
                })
            && character
                .construction_activity
                .as_ref()
                .is_none_or(|activity| {
                    activity.consumed_items.iter().all(|consumed| {
                        collect_stable_item_ids(&consumed.item, namespace, &mut item_ids)
                    })
                });
    stable_item_ids_are_valid
        && character.account_id.counter() > 0
        && character.actor_id.counter() > 0
        && character.account_id.world_namespace() == character.actor_id.world_namespace()
        && valid_character_name(&character.name)
        && valid_base_stats(
            character.base_strength,
            character.base_dexterity,
            character.base_intelligence,
            character.base_perception,
        )
        && character
            .held_movement
            .is_none_or(HorizontalDirection::is_valid)
        && character.wielded.is_none_or(|item_id| {
            item_id.counter() > 0
                && item_id.world_namespace() == character.actor_id.world_namespace()
        })
        && (-1_000..=1_000).contains(&character.sleepiness)
        && (character.sleeping || character.sleep_intervals == 0)
        && character.sleep_intervals <= 24
        && character.speed > 0
        && u32::from(character.speed) <= ACTION_POINT_THRESHOLD
        && character.action_points <= i64::from(ACTION_POINT_THRESHOLD)
        && character.action_points >= MIN_ACTION_POINTS
        && character.queued_actions.len() <= 2
        && character
            .queued_actions
            .iter()
            .all(|action| action.sequence <= character.last_command_sequence)
        && character
            .queued_actions
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
        && character
            .craft_activity
            .as_ref()
            .is_none_or(|activity| valid_craft_activity(activity, character.actor_id))
        && character.read_activity.as_ref().is_none_or(|activity| {
            valid_book_study_activity(activity, character.actor_id, character.base_intelligence)
        })
        && character
            .disassembly_activity
            .as_ref()
            .is_none_or(|activity| valid_disassembly_activity(activity, character.actor_id))
        && character
            .construction_activity
            .as_ref()
            .is_none_or(|activity| valid_construction_activity(activity, character.actor_id))
        && usize::from(character.craft_activity.is_some())
            + usize::from(character.read_activity.is_some())
            + usize::from(character.disassembly_activity.is_some())
            + usize::from(character.construction_activity.is_some())
            <= 1
        && usize::from(character.learned_recipe_count) <= MAX_LEARNED_RECIPES
        && valid_skill_levels(&character.skills, character.tick)
        && valid_proficiency_levels(&character.proficiencies)
        && character.inventory_total <= 256
        && character.inventory.len() <= usize::from(MAX_ADMIN_INVENTORY_PER_PAGE)
        && character.inventory.len() <= usize::from(character.inventory_total)
        && inventory_is_ordered
        && character.inventory.iter().all(|item| {
            item.id.counter() > 0
                && item.id.world_namespace() == character.actor_id.world_namespace()
                && valid_item_snapshot(item)
        })
        && character.next_inventory_after.is_none_or(|next| {
            !character.inventory.is_empty()
                && character.inventory.last().map(|item| item.id) == Some(next)
        })
}

fn valid_client_command(command: &ClientCommand) -> bool {
    if command.actor_id.counter() == 0 || command.sequence.0 == 0 {
        return false;
    }
    match &command.kind {
        CommandKind::Craft { recipe_id, recipe } => {
            valid_recipe_id(recipe_id)
                && recipe.as_ref().is_none_or(|recipe| {
                    recipe.recipe_id == *recipe_id && valid_craft_recipe(recipe)
                })
        }
        CommandKind::ReadBook {
            item_id,
            book_type_id,
            study,
        } => {
            item_id.counter() > 0
                && item_id.world_namespace() == command.actor_id.world_namespace()
                && valid_recipe_id(book_type_id)
                && study.as_ref().is_none_or(|study| {
                    study.book_type_id == *book_type_id && valid_book_study(study)
                })
        }
        CommandKind::Disassemble {
            item_id,
            item_type_id,
            recipe,
        } => {
            item_id.counter() > 0
                && item_id.world_namespace() == command.actor_id.world_namespace()
                && valid_recipe_id(item_type_id)
                && recipe.as_ref().is_none_or(|recipe| {
                    recipe.target_type_id == *item_type_id && valid_disassembly_recipe(recipe)
                })
        }
        CommandKind::Construct {
            target: _,
            construction_id,
            construction,
        } => {
            valid_recipe_id(construction_id)
                && construction.as_ref().is_none_or(|construction| {
                    construction.construction_id == *construction_id
                        && valid_construction_recipe(construction)
                })
        }
        CommandKind::Activate { item_id } => {
            item_id.counter() > 0 && item_id.world_namespace() == command.actor_id.world_namespace()
        }
        _ => true,
    }
}

fn valid_recipe_id(recipe_id: &str) -> bool {
    !recipe_id.is_empty()
        && recipe_id.len() <= MAX_CRAFT_RECIPE_ID_BYTES
        && recipe_id.chars().all(|character| !character.is_control())
}

fn valid_skill_id(skill_id: &str) -> bool {
    !skill_id.is_empty()
        && skill_id.len() <= MAX_SKILL_ID_BYTES
        && skill_id.chars().all(|character| !character.is_control())
}

fn valid_proficiency_id(proficiency_id: &str) -> bool {
    !proficiency_id.is_empty()
        && proficiency_id.len() <= MAX_PROFICIENCY_ID_BYTES
        && proficiency_id
            .chars()
            .all(|character| !character.is_control())
}

fn valid_craft_proficiency(proficiency: &CraftProficiencyV1) -> bool {
    valid_proficiency_id(&proficiency.proficiency_id)
        && if proficiency.required {
            proficiency.time_multiplier_millionths == 0 && proficiency.skill_penalty_millionths == 0
        } else {
            (CRAFT_PROFICIENCY_SCALE..=MAX_CRAFT_PROFICIENCY_MULTIPLIER)
                .contains(&proficiency.time_multiplier_millionths)
        }
        && proficiency.skill_penalty_millionths.unsigned_abs() <= MAX_CRAFT_PROFICIENCY_MULTIPLIER
        && proficiency.learning_time_multiplier_millionths <= MAX_CRAFT_PROFICIENCY_MULTIPLIER
        && proficiency.time_to_learn_action_points > 0
        && proficiency.time_to_learn_action_points <= MAX_PROFICIENCY_PRACTICE_ACTION_POINTS
        && proficiency
            .max_experience_action_points
            .is_none_or(|maximum| maximum > 0 && maximum <= MAX_PROFICIENCY_PRACTICE_ACTION_POINTS)
        && proficiency.required_proficiencies.len() <= MAX_CRAFT_PROFICIENCIES
        && proficiency
            .required_proficiencies
            .iter()
            .all(|id| valid_proficiency_id(id))
        && proficiency
            .required_proficiencies
            .windows(2)
            .all(|pair| pair[0] < pair[1])
}

fn valid_skill_requirement(requirement: &CraftSkillRequirementV1) -> bool {
    valid_skill_id(&requirement.skill_id) && requirement.level <= MAX_SKILL_LEVEL
}

fn valid_skill_requirements(requirements: &[CraftSkillRequirementV1]) -> bool {
    requirements.len() <= MAX_SKILLS
        && requirements.iter().all(valid_skill_requirement)
        && requirements
            .windows(2)
            .all(|pair| pair[0].skill_id < pair[1].skill_id)
}

fn skill_experience_threshold(level: u8) -> u64 {
    let next = u64::from(level) + 1;
    10_000 * next * next
}

fn valid_skill_levels(skills: &[SkillLevelSnapshot], current_tick: SimTick) -> bool {
    skills.len() <= MAX_SKILLS
        && skills
            .windows(2)
            .all(|pair| pair[0].skill_id < pair[1].skill_id)
        && skills.iter().all(|skill| {
            valid_skill_id(&skill.skill_id)
                && skill.practical_level <= MAX_SKILL_LEVEL
                && skill.theoretical_level <= MAX_SKILL_LEVEL
                && skill.theoretical_level >= skill.practical_level
                && (skill.practical_level == MAX_SKILL_LEVEL
                    || u64::from(skill.practical_experience)
                        < skill_experience_threshold(skill.practical_level))
                && (skill.theoretical_level == MAX_SKILL_LEVEL
                    || u64::from(skill.theoretical_experience)
                        < skill_experience_threshold(skill.theoretical_level))
                && (skill.theoretical_level != skill.practical_level
                    || skill.theoretical_experience >= skill.practical_experience)
                && skill.last_practiced <= current_tick
        })
}

fn valid_proficiency_levels(proficiencies: &[ProficiencyLevelSnapshot]) -> bool {
    proficiencies.len() <= MAX_PROFICIENCIES
        && proficiencies
            .windows(2)
            .all(|pair| pair[0].proficiency_id < pair[1].proficiency_id)
        && proficiencies.iter().all(|proficiency| {
            valid_proficiency_id(&proficiency.proficiency_id)
                && proficiency.practiced_action_points <= MAX_PROFICIENCY_PRACTICE_ACTION_POINTS
                && proficiency.practice_remainder_millionths < CRAFT_PROFICIENCY_SCALE
                && (proficiency.learned
                    || proficiency.practiced_action_points > 0
                    || proficiency.practice_remainder_millionths > 0)
        })
}

fn valid_craft_recipe(recipe: &CraftRecipeV1) -> bool {
    let shape_is_valid = valid_recipe_id(&recipe.recipe_id)
        && recipe.time_moves > 0
        && recipe.output_instances > 0
        && recipe.output_instances <= MAX_CRAFT_OUTPUT_INSTANCES
        && valid_craft_item_prototype(&recipe.output)
        && recipe.byproducts.len() <= MAX_CRAFT_BYPRODUCT_TYPES
        && recipe.byproducts.iter().all(|byproduct| {
            byproduct.output_instances > 0
                && byproduct.output_instances <= MAX_CRAFT_OUTPUT_INSTANCES
                && valid_craft_item_prototype(&byproduct.output)
        })
        && recipe
            .byproducts
            .windows(2)
            .all(|pair| pair[0].output.type_id < pair[1].output.type_id)
        && recipe
            .total_output_instances()
            .is_some_and(|total| total <= MAX_CRAFT_OUTPUT_INSTANCES)
        && !recipe.components.is_empty()
        && recipe.components.len() <= MAX_CRAFT_COMPONENT_GROUPS
        && recipe.components.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_COMPONENT_ALTERNATIVES
                && group
                    .iter()
                    .all(|component| valid_recipe_id(&component.type_id) && component.count > 0)
        })
        && recipe.tools.len() <= MAX_CRAFT_SUPPORT_GROUPS
        && recipe.tools.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_SUPPORT_ALTERNATIVES
                && group.iter().all(|tool| {
                    valid_recipe_id(&tool.type_id)
                        && tool.amount > 0
                        && (tool.consumes_charges || tool.amount <= 256)
                })
        })
        && recipe.qualities.len() <= MAX_CRAFT_SUPPORT_GROUPS
        && recipe.qualities.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_SUPPORT_ALTERNATIVES
                && group.iter().all(|quality| {
                    valid_recipe_id(&quality.quality_id)
                        && quality.amount > 0
                        && quality.amount <= 256
                        && !quality.providers.is_empty()
                        && quality.providers.len() <= MAX_CRAFT_QUALITY_PROVIDERS
                        && quality
                            .providers
                            .iter()
                            .all(|provider| valid_recipe_id(&provider.type_id))
                        && quality
                            .providers
                            .windows(2)
                            .all(|pair| pair[0].type_id < pair[1].type_id)
                })
        })
        && recipe.proficiencies.len() <= MAX_CRAFT_PROFICIENCIES
        && recipe.proficiencies.iter().all(valid_craft_proficiency)
        && recipe
            .proficiencies
            .windows(2)
            .all(|pair| pair[0].proficiency_id < pair[1].proficiency_id)
        && recipe
            .primary_skill
            .as_ref()
            .is_none_or(valid_skill_requirement)
        && valid_skill_requirements(&recipe.required_skills)
        && valid_skill_requirements(&recipe.autolearn_skills)
        && (recipe.autolearn || !recipe.book_requirements.is_empty() || recipe.can_be_learned)
        && (recipe.autolearn || recipe.autolearn_skills.is_empty())
        && recipe.book_requirements.len() <= MAX_CRAFT_BOOK_REQUIREMENTS
        && recipe.book_requirements.iter().all(|requirement| {
            valid_recipe_id(&requirement.book_type_id)
                && requirement.required_skill_level <= MAX_SKILL_LEVEL
        })
        && recipe
            .book_requirements
            .windows(2)
            .all(|pair| pair[0].book_type_id < pair[1].book_type_id);
    if !shape_is_valid {
        return false;
    }
    let mut charge_modes = BTreeMap::new();
    recipe.components.iter().flatten().all(|component| {
        charge_modes
            .insert(component.type_id.as_str(), component.count_by_charges)
            .is_none_or(|mode| mode == component.count_by_charges)
    })
}

fn valid_construction_recipe(recipe: &ConstructionRecipeV1) -> bool {
    valid_recipe_id(&recipe.construction_id)
        && !recipe.name.is_empty()
        && recipe.name.len() <= 512
        && recipe.name.chars().all(|character| !character.is_control())
        && recipe.time_moves > 0
        && recipe
            .time_moves
            .checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE as u64)
            .is_some()
        && valid_skill_requirements(&recipe.required_skills)
        && !recipe.components.is_empty()
        && recipe.components.len() <= MAX_CRAFT_COMPONENT_GROUPS
        && recipe.components.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_COMPONENT_ALTERNATIVES
                && group
                    .iter()
                    .all(|component| valid_recipe_id(&component.type_id) && component.count > 0)
        })
        && recipe.qualities.len() <= MAX_CRAFT_SUPPORT_GROUPS
        && recipe.qualities.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_SUPPORT_ALTERNATIVES
                && group.iter().all(|quality| {
                    valid_recipe_id(&quality.quality_id)
                        && quality.amount > 0
                        && quality.amount <= 256
                        && !quality.providers.is_empty()
                        && quality.providers.len() <= MAX_CRAFT_QUALITY_PROVIDERS
                        && quality
                            .providers
                            .iter()
                            .all(|provider| valid_recipe_id(&provider.type_id))
                        && quality
                            .providers
                            .windows(2)
                            .all(|pair| pair[0].type_id < pair[1].type_id)
                })
        })
        && recipe.pre_terrain.len() <= 64
        && recipe.pre_terrain.iter().all(|id| valid_recipe_id(id))
        && recipe.pre_terrain.windows(2).all(|pair| pair[0] < pair[1])
        && match &recipe.result {
            ConstructionResultV1::Terrain(tile) => valid_terrain_tile(tile),
            ConstructionResultV1::Furniture(furniture) => valid_furniture_tile(furniture),
        }
}

fn valid_disassembly_recipe(recipe: &DisassemblyRecipeV1) -> bool {
    valid_recipe_id(&recipe.recipe_id)
        && valid_recipe_id(&recipe.target_type_id)
        && recipe.time_moves > 0
        && recipe
            .time_moves
            .checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE as u64)
            .is_some()
        && recipe.difficulty <= MAX_SKILL_LEVEL
        && recipe
            .primary_skill_id
            .as_ref()
            .is_none_or(|skill_id| valid_skill_id(skill_id))
        && valid_skill_requirements(&recipe.learn_requirements)
        && valid_skill_requirements(&recipe.autolearn_requirements)
        && (recipe.autolearn || recipe.autolearn_requirements.is_empty())
        && !(recipe.requires_empty_charges && recipe.unload_charges_as.is_some())
        && recipe.unload_charges_as.as_ref().is_none_or(|ammunition| {
            valid_craft_item_prototype(ammunition)
                && ammunition.charges > 0
                && !ammunition.ammunition_type.is_empty()
                && ammunition.ranged_weapon.is_none()
        })
        && recipe.components.len() <= MAX_DISASSEMBLY_COMPONENT_TYPES
        && recipe.components.iter().all(|component| {
            component.output_instances > 0
                && component.output_instances <= MAX_CRAFT_OUTPUT_INSTANCES
                && (!component.count_by_charges || component.output_instances == 1)
                && valid_craft_item_prototype(&component.output)
                && component.output_state.as_ref().is_none_or(|state| {
                    state.recoverable
                        && state.count_by_charges == component.count_by_charges
                        && valid_item_component_root(state)
                        && component_state_matches_prototype(state, &component.output)
                })
        })
        && recipe
            .total_component_instances()
            .is_some_and(|total| total <= MAX_CRAFT_OUTPUT_INSTANCES)
        && recipe.tools.len() <= MAX_CRAFT_SUPPORT_GROUPS
        && recipe.tools.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_SUPPORT_ALTERNATIVES
                && group.iter().all(|tool| {
                    valid_recipe_id(&tool.type_id)
                        && tool.amount > 0
                        && !tool.consumes_charges
                        && tool.amount <= 256
                })
        })
        && recipe.qualities.len() <= MAX_CRAFT_SUPPORT_GROUPS
        && recipe.qualities.iter().all(|group| {
            !group.is_empty()
                && group.len() <= MAX_CRAFT_SUPPORT_ALTERNATIVES
                && group.iter().all(|quality| {
                    valid_recipe_id(&quality.quality_id)
                        && quality.amount > 0
                        && quality.amount <= 256
                        && !quality.providers.is_empty()
                        && quality.providers.len() <= MAX_CRAFT_QUALITY_PROVIDERS
                        && quality
                            .providers
                            .iter()
                            .all(|provider| valid_recipe_id(&provider.type_id))
                        && quality
                            .providers
                            .windows(2)
                            .all(|pair| pair[0].type_id < pair[1].type_id)
                })
        })
}

fn valid_disassembly_activity(activity: &DisassemblyActivitySnapshotV1, actor_id: ActorId) -> bool {
    let maximum = activity
        .recipe
        .time_moves
        .checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
    let mut ids = BTreeSet::new();
    valid_disassembly_recipe(&activity.recipe)
        && activity.target_item.id.counter() > 0
        && activity.target_item.id.world_namespace() == actor_id.world_namespace()
        && activity.target_item.type_id == activity.recipe.target_type_id
        && activity.target_item.damage <= MAX_ITEM_DAMAGE_LEVEL
        && activity
            .target_item
            .magazine_well
            .as_ref()
            .is_none_or(|well| well.installed_magazine.is_none())
        && match (
            &activity.target_item.ranged_weapon,
            &activity.recipe.unload_charges_as,
        ) {
            (None, None) => {
                !activity.recipe.requires_empty_charges || activity.target_item.charges == 0
            }
            (Some(weapon), Some(ammunition)) => {
                !activity.recipe.requires_empty_charges
                    && weapon.ammunition_remaining == 0
                    && weapon.ammunition_type == ammunition.ammunition_type
            }
            (None, Some(_)) => {
                !activity.recipe.requires_empty_charges && activity.target_item.charges == 0
            }
            _ => false,
        }
        && valid_item_snapshot(&activity.target_item)
        && activity.selected_tool_alternatives.len() == activity.recipe.tools.len()
        && activity
            .selected_tool_alternatives
            .iter()
            .zip(&activity.recipe.tools)
            .all(|(selected, group)| usize::from(*selected) < group.len())
        && activity.remaining_action_points > 0
        && maximum.is_some_and(|maximum| activity.remaining_action_points <= maximum)
        && activity.rng_sequence.0 > 0
        && activity.reserved_component_items.len()
            == usize::from(
                activity
                    .recipe
                    .total_component_instances()
                    .unwrap_or_default(),
            )
        && activity
            .reserved_component_items
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && activity.reserved_component_items.iter().all(|item_id| {
            item_id.counter() > 0
                && item_id.world_namespace() == actor_id.world_namespace()
                && item_id != &activity.target_item.id
                && ids.insert(*item_id)
        })
}

fn valid_learned_recipes(recipes: &[String]) -> bool {
    recipes.len() <= MAX_LEARNED_RECIPES
        && recipes.iter().all(|recipe| valid_recipe_id(recipe))
        && recipes.windows(2).all(|pair| pair[0] < pair[1])
}

fn valid_craft_item_prototype(item: &CraftItemPrototypeV1) -> bool {
    let snapshot = ItemSnapshot {
        id: ItemId::new(1, 1),
        type_id: item.type_id.clone(),
        charges: item.charges,
        damage: 0,
        melee_damage_milli: item.melee_damage_milli.clone(),
        calories: item.calories,
        quench: item.quench,
        comestible_type: item.comestible_type.clone(),
        ammunition_type: item.ammunition_type.clone(),
        ranged_weapon: item.ranged_weapon.clone(),
        component_provenance: None,
        magazine_capacity: item.magazine_capacity,
        magazine_well: item
            .magazine_well
            .as_ref()
            .map(|well| MagazineWellSnapshotV1 {
                compatible_magazine_type_ids: well.compatible_magazine_type_ids.clone(),
                installed_magazine: None,
            }),
        residual_energy_millijoules: item.residual_energy_millijoules,
        powered_tool: item.powered_tool.clone(),
        creature_corpse: None,
    };
    valid_item_snapshot(&snapshot)
}

fn valid_craft_activity(activity: &CraftActivitySnapshotV1, actor_id: ActorId) -> bool {
    let maximum = activity
        .recipe
        .time_moves
        .checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
    let mut ids = BTreeSet::new();
    let maximum_practice_ticks = maximum.and_then(|maximum| {
        maximum
            .checked_sub(activity.remaining_action_points)
            .map(|completed| completed / CRAFT_PRACTICE_ACTION_POINTS)
    });
    let maximum_proficiency_buckets = maximum.and_then(|maximum| {
        maximum
            .checked_sub(activity.remaining_action_points)
            .map(|completed| {
                u8::try_from(u128::from(completed) * 20 / u128::from(maximum))
                    .expect("an incomplete craft has fewer than twenty buckets")
            })
    });
    valid_craft_recipe(&activity.recipe)
        && activity.selected_tool_alternatives.len() == activity.recipe.tools.len()
        && activity
            .selected_tool_alternatives
            .iter()
            .zip(&activity.recipe.tools)
            .all(|(selected, group)| usize::from(*selected) < group.len())
        && activity.remaining_action_points > 0
        && maximum.is_some_and(|maximum| activity.remaining_action_points <= maximum)
        && maximum_practice_ticks.is_some_and(|maximum| {
            if activity.recipe.primary_skill.is_some() {
                activity.practice_ticks_awarded == maximum
            } else {
                activity.practice_ticks_awarded == 0
            }
        })
        && activity.proficiency_progress_millionths < CRAFT_PROFICIENCY_SCALE
        && maximum_proficiency_buckets.is_some_and(|maximum| {
            if activity.recipe.proficiencies.is_empty() {
                activity.proficiency_buckets_awarded == 0
                    && activity.proficiency_progress_millionths == 0
            } else {
                activity.proficiency_buckets_awarded == maximum
            }
        })
        && !activity.consumed_items.is_empty()
        && activity.consumed_items.len() <= 256
        && activity.consumed_items.iter().all(|consumed| {
            consumed.item.id.counter() > 0
                && consumed.item.id.world_namespace() == actor_id.world_namespace()
                && ids.insert(consumed.item.id)
                && valid_item_snapshot(&consumed.item)
                && consumed.split_from.is_none_or(|item_id| {
                    item_id.counter() > 0
                        && item_id.world_namespace() == actor_id.world_namespace()
                        && item_id != consumed.item.id
                })
        })
        && activity.reserved_output_items.len()
            == activity
                .recipe
                .total_output_instances()
                .map_or(usize::MAX, usize::from)
        && activity.reserved_output_items.iter().all(|item_id| {
            item_id.counter() > 0
                && item_id.world_namespace() == actor_id.world_namespace()
                && ids.insert(*item_id)
        })
        && activity
            .reserved_output_items
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && activity.previously_wielded.is_none_or(|item_id| {
            activity
                .consumed_items
                .iter()
                .any(|consumed| consumed.item.id == item_id && consumed.split_from.is_none())
        })
}

fn valid_construction_activity(
    activity: &ConstructionActivitySnapshotV1,
    actor_id: ActorId,
) -> bool {
    let maximum = activity
        .recipe
        .time_moves
        .checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE as u64);
    let mut ids = BTreeSet::new();
    valid_construction_recipe(&activity.recipe)
        && activity.remaining_action_points > 0
        && maximum.is_some_and(|maximum| activity.remaining_action_points <= maximum)
        && !activity.consumed_items.is_empty()
        && activity.consumed_items.len() <= 256
        && activity.consumed_items.iter().all(|consumed| {
            consumed.item.id.counter() > 0
                && consumed.item.id.world_namespace() == actor_id.world_namespace()
                && ids.insert(consumed.item.id)
                && valid_item_snapshot(&consumed.item)
                && consumed.split_from.is_none_or(|item_id| {
                    item_id.counter() > 0
                        && item_id.world_namespace() == actor_id.world_namespace()
                        && item_id != consumed.item.id
                })
        })
        && activity.previously_wielded.is_none_or(|item_id| {
            activity
                .consumed_items
                .iter()
                .any(|consumed| consumed.item.id == item_id && consumed.split_from.is_none())
        })
}

fn valid_book_study(study: &BookStudyV1) -> bool {
    valid_recipe_id(&study.book_type_id)
        && valid_skill_id(&study.skill_id)
        && study.required_skill_level < study.maximum_skill_level
        && study.maximum_skill_level <= MAX_SKILL_LEVEL
        && study.intelligence_requirement <= MAX_ACTOR_BASE_STAT
        && study.time_moves > 0
        && study.time_moves <= MAX_BOOK_STUDY_MOVES
        && adjusted_book_study_time_moves(study.time_moves, study.intelligence_requirement, 1)
            .is_some_and(|moves| moves <= MAX_BOOK_STUDY_MOVES)
        && study.source_time_minutes > 0
        && u64::from(study.source_time_minutes)
            .checked_mul(60 * 100)
            .is_some_and(|moves| moves <= MAX_BOOK_STUDY_MOVES)
}

fn valid_book_study_activity(
    activity: &BookStudyActivitySnapshotV1,
    actor_id: ActorId,
    intelligence: u16,
) -> bool {
    let maximum = adjusted_book_study_time_moves(
        activity.study.time_moves,
        activity.study.intelligence_requirement,
        intelligence,
    )
    .and_then(|moves| moves.checked_mul(ACTION_POINTS_PER_UPSTREAM_MOVE as u64));
    valid_book_study(&activity.study)
        && activity.book_item_id.counter() > 0
        && activity.book_item_id.world_namespace() == actor_id.world_namespace()
        && activity.rng_sequence.0 > 0
        && activity.remaining_action_points > 0
        && maximum.is_some_and(|maximum| activity.remaining_action_points <= maximum)
}

fn valid_base_stats(strength: u16, dexterity: u16, intelligence: u16, perception: u16) -> bool {
    [strength, dexterity, intelligence, perception]
        .into_iter()
        .all(|stat| (1..=MAX_ACTOR_BASE_STAT).contains(&stat))
}

fn valid_display_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}

fn valid_binding(binding: &EndpointBindingSummary) -> bool {
    if binding.state == EndpointBindingState::Pending {
        binding
            .pending_expires_utc
            .is_some_and(|expires| expires > 0)
    } else {
        binding.pending_expires_utc.is_none()
    }
}

fn valid_item_snapshot(item: &ItemSnapshot) -> bool {
    !item.type_id.is_empty()
        && item.type_id.len() <= 512
        && item.damage <= MAX_ITEM_DAMAGE_LEVEL
        && item
            .type_id
            .chars()
            .all(|character| !character.is_control())
        && item.melee_damage_milli.len() <= 32
        && item.melee_damage_milli.iter().all(|(damage_type, value)| {
            !damage_type.is_empty()
                && damage_type.len() <= 64
                && damage_type.chars().all(|character| !character.is_control())
                && *value >= 0
        })
        && item.comestible_type.len() <= 32
        && item
            .comestible_type
            .chars()
            .all(|character| !character.is_control())
        && (item.comestible_type.is_empty() || item.charges > 0)
        && item.ammunition_type.len() <= 64
        && item
            .ammunition_type
            .chars()
            .all(|character| !character.is_control())
        && (item.ammunition_type.is_empty()
            || item.charges > 0
            || (item.magazine_capacity > 0 && item.charges >= 0))
        && (item.magazine_capacity == 0
            || (item.charges >= 0
                && u32::try_from(item.charges)
                    .is_ok_and(|charges| charges <= item.magazine_capacity)
                && !item.ammunition_type.is_empty()
                && item.ranged_weapon.is_none()
                && item.magazine_well.is_none()))
        && (item.residual_energy_millijoules == 0
            || (item.magazine_capacity > 0
                && u32::try_from(item.charges)
                    .is_ok_and(|charges| charges < item.magazine_capacity)
                && item.residual_energy_millijoules < MILLIJOULES_PER_BATTERY_CHARGE))
        && item.magazine_well.as_ref().is_none_or(|_| {
            item.charges == 0
                && item.magazine_capacity == 0
                && item.ammunition_type.is_empty()
                && item.ranged_weapon.is_none()
        })
        && item.ranged_weapon.as_ref().is_none_or(|weapon| {
            !weapon.ammunition_type.is_empty()
                && weapon.ammunition_type.len() <= 64
                && weapon
                    .ammunition_type
                    .chars()
                    .all(|character| !character.is_control())
                && weapon.ammunition_capacity > 0
                && weapon.ammunition_remaining <= weapon.ammunition_capacity
                && weapon.range > 0
                && weapon.damage > 0
        })
        && valid_component_provenance(&item.component_provenance)
        && item
            .magazine_well
            .as_ref()
            .is_none_or(valid_magazine_well_snapshot)
        && item.powered_tool.as_ref().is_none_or(|powered| {
            item.magazine_well.is_some()
                && item.residual_energy_millijoules == 0
                && valid_powered_tool_state(&item.type_id, powered)
        })
        && item.creature_corpse.as_ref().is_none_or(|corpse| {
            item.type_id == "corpse"
                && item.charges == 1
                && item.melee_damage_milli.is_empty()
                && item.calories == 0
                && item.quench == 0
                && item.comestible_type.is_empty()
                && item.ammunition_type.is_empty()
                && item.ranged_weapon.is_none()
                && item.component_provenance.is_none()
                && item.magazine_capacity == 0
                && item.magazine_well.is_none()
                && item.residual_energy_millijoules == 0
                && item.powered_tool.is_none()
                && valid_creature_corpse_prototype(&corpse.prototype)
                && (!corpse.revivable || corpse.prototype.revives)
                && (!corpse.revive_special || corpse.prototype.revives)
        })
}

fn valid_creature_corpse_prototype(prototype: &CreatureCorpsePrototypeV1) -> bool {
    valid_recipe_id(&prototype.monster_type_id)
        && prototype.max_hp > 0
        && prototype.speed > 0
        && prototype.attack_cost_moves > 0
        && prototype.melee_dice_sides > 0
        && (!prototype.group_bash || prototype.bashes)
        && (!prototype.good_hearing || prototype.hears)
        && valid_creature_path_settings(prototype.path_settings)
        && (!prototype.can_see || (prototype.vision_day > 0 || prototype.vision_night > 0))
        && (prototype.blood_field_type_id.is_empty()
            || valid_recipe_id(&prototype.blood_field_type_id))
}

fn valid_creature_path_settings(settings: CreaturePathSettingsV1) -> bool {
    settings.max_distance <= 400
}

fn valid_powered_tool_state(type_id: &str, powered: &PoweredToolStateV1) -> bool {
    valid_recipe_id(&powered.inactive_type_id)
        && valid_recipe_id(&powered.active_type_id)
        && powered.inactive_type_id != powered.active_type_id
        && powered.activation_charges > 0
        && powered.power_draw_milliwatts > 0
        && powered.light_emission > 0
        && if powered.active {
            type_id == powered.active_type_id
        } else {
            type_id == powered.inactive_type_id
        }
}

fn valid_magazine_well_prototype(well: &MagazineWellPrototypeV1) -> bool {
    !well.compatible_magazine_type_ids.is_empty()
        && well.compatible_magazine_type_ids.len() <= MAX_MAGAZINE_COMPATIBLE_TYPES
        && well
            .compatible_magazine_type_ids
            .iter()
            .all(|type_id| valid_recipe_id(type_id))
        && well
            .compatible_magazine_type_ids
            .windows(2)
            .all(|pair| pair[0] < pair[1])
}

fn valid_magazine_well_snapshot(well: &MagazineWellSnapshotV1) -> bool {
    let prototype = MagazineWellPrototypeV1 {
        compatible_magazine_type_ids: well.compatible_magazine_type_ids.clone(),
    };
    valid_magazine_well_prototype(&prototype)
        && well.installed_magazine.as_ref().is_none_or(|installed| {
            well.compatible_magazine_type_ids
                .binary_search(&installed.type_id)
                .is_ok()
                && installed.magazine_capacity > 0
                && installed.magazine_well.is_none()
                && valid_item_snapshot(installed)
        })
}

fn collect_stable_item_ids(
    item: &ItemSnapshot,
    world_namespace: u64,
    ids: &mut BTreeSet<ItemId>,
) -> bool {
    item.id.counter() > 0
        && item.id.world_namespace() == world_namespace
        && ids.insert(item.id)
        && item
            .magazine_well
            .as_ref()
            .and_then(|well| well.installed_magazine.as_deref())
            .is_none_or(|installed| collect_stable_item_ids(installed, world_namespace, ids))
}

fn valid_item_component_root(component: &ItemComponentSnapshotV1) -> bool {
    let mut remaining = MAX_ITEM_COMPONENTS;
    valid_item_component(component, 1, &mut remaining)
}

fn component_state_matches_prototype(
    state: &ItemComponentSnapshotV1,
    prototype: &CraftItemPrototypeV1,
) -> bool {
    state.type_id == prototype.type_id
        && state.charges == prototype.charges
        && state.melee_damage_milli == prototype.melee_damage_milli
        && state.calories == prototype.calories
        && state.quench == prototype.quench
        && state.comestible_type == prototype.comestible_type
        && state.ammunition_type == prototype.ammunition_type
        && state.ranged_weapon == prototype.ranged_weapon
        && state.magazine_capacity == prototype.magazine_capacity
        && state.magazine_well == prototype.magazine_well
        && state.residual_energy_millijoules == prototype.residual_energy_millijoules
        && state.powered_tool == prototype.powered_tool
}

fn valid_component_provenance(provenance: &Option<Vec<ItemComponentSnapshotV1>>) -> bool {
    let Some(components) = provenance else {
        return true;
    };
    if components.len() > MAX_ITEM_COMPONENTS {
        return false;
    }
    let mut remaining = MAX_ITEM_COMPONENTS;
    components
        .iter()
        .all(|component| valid_item_component(component, 1, &mut remaining))
}

fn valid_item_component(
    component: &ItemComponentSnapshotV1,
    depth: usize,
    remaining: &mut usize,
) -> bool {
    if depth > MAX_ITEM_COMPONENT_DEPTH || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    !component.type_id.is_empty()
        && component.type_id.len() <= 512
        && component
            .type_id
            .chars()
            .all(|character| !character.is_control())
        && component.damage <= MAX_ITEM_DAMAGE_LEVEL
        && (!component.count_by_charges || component.charges > 0)
        && component.melee_damage_milli.len() <= 32
        && component
            .melee_damage_milli
            .iter()
            .all(|(damage_type, value)| {
                !damage_type.is_empty()
                    && damage_type.len() <= 64
                    && damage_type.chars().all(|character| !character.is_control())
                    && *value >= 0
            })
        && component.comestible_type.len() <= 32
        && component
            .comestible_type
            .chars()
            .all(|character| !character.is_control())
        && (component.comestible_type.is_empty() || component.charges > 0)
        && component.ammunition_type.len() <= 64
        && component
            .ammunition_type
            .chars()
            .all(|character| !character.is_control())
        && (component.ammunition_type.is_empty()
            || component.charges > 0
            || (component.magazine_capacity > 0 && component.charges >= 0))
        && (component.magazine_capacity == 0
            || (component.charges >= 0
                && u32::try_from(component.charges)
                    .is_ok_and(|charges| charges <= component.magazine_capacity)
                && !component.ammunition_type.is_empty()
                && component.ranged_weapon.is_none()
                && component.magazine_well.is_none()))
        && (component.residual_energy_millijoules == 0
            || (component.magazine_capacity > 0
                && u32::try_from(component.charges)
                    .is_ok_and(|charges| charges < component.magazine_capacity)
                && component.residual_energy_millijoules < MILLIJOULES_PER_BATTERY_CHARGE))
        && component.ranged_weapon.as_ref().is_none_or(|weapon| {
            !weapon.ammunition_type.is_empty()
                && weapon.ammunition_type.len() <= 64
                && weapon
                    .ammunition_type
                    .chars()
                    .all(|character| !character.is_control())
                && weapon.ammunition_capacity > 0
                && weapon.ammunition_remaining <= weapon.ammunition_capacity
                && weapon.range > 0
                && weapon.damage > 0
        })
        && component
            .magazine_well
            .as_ref()
            .is_none_or(valid_magazine_well_prototype)
        && component.powered_tool.as_ref().is_none_or(|powered| {
            component.magazine_well.is_some()
                && component.residual_energy_millijoules == 0
                && valid_powered_tool_state(&component.type_id, powered)
        })
        && component
            .component_provenance
            .as_ref()
            .is_none_or(|children| {
                children.len() <= MAX_ITEM_COMPONENTS
                    && children
                        .iter()
                        .all(|child| valid_item_component(child, depth + 1, remaining))
            })
}

fn valid_chat_text(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= MAX_CHAT_BYTES && !text.chars().any(char::is_control)
}

fn valid_terrain_tile(tile: &TerrainTileSnapshot) -> bool {
    !tile.terrain_id.is_empty()
        && tile.terrain_id.len() <= 512
        && tile.move_cost >= -1
        && [
            tile.terrain_id.as_str(),
            tile.open.as_str(),
            tile.close.as_str(),
        ]
        .into_iter()
        .all(|value| value.len() <= 512 && value.chars().all(|character| !character.is_control()))
        && matches!(
            (
                tile.open.is_empty(),
                tile.open_move_cost,
                tile.open_transparent,
                tile.open_flat
            ),
            (true, None, None, None) | (false, Some(-1..), Some(_), Some(_))
        )
        && matches!(
            (
                tile.close.is_empty(),
                tile.close_move_cost,
                tile.close_transparent,
                tile.close_flat
            ),
            (true, None, None, None) | (false, Some(-1..), Some(_), Some(_))
        )
}

fn valid_furniture_tile(furniture: &FurnitureTileSnapshot) -> bool {
    !furniture.furniture_id.is_empty()
        && furniture.furniture_id.len() <= 512
        && furniture
            .furniture_id
            .chars()
            .all(|character| !character.is_control())
}

fn valid_replication_snapshot(snapshot: &ReplicationSnapshotV1) -> bool {
    let namespace = snapshot.controlled_actor.id.world_namespace();
    let mut item_ids = BTreeSet::new();
    let stable_item_ids_are_valid = snapshot
        .controlled_actor
        .inventory
        .iter()
        .all(|item| collect_stable_item_ids(item, namespace, &mut item_ids))
        && snapshot
            .controlled_actor
            .craft_activity
            .as_ref()
            .is_none_or(|activity| {
                activity.consumed_items.iter().all(|consumed| {
                    collect_stable_item_ids(&consumed.item, namespace, &mut item_ids)
                }) && activity.reserved_output_items.iter().all(|item_id| {
                    item_id.counter() > 0
                        && item_id.world_namespace() == namespace
                        && item_ids.insert(*item_id)
                })
            })
        && snapshot
            .controlled_actor
            .disassembly_activity
            .as_ref()
            .is_none_or(|activity| {
                collect_stable_item_ids(&activity.target_item, namespace, &mut item_ids)
                    && activity.reserved_component_items.iter().all(|item_id| {
                        item_id.counter() > 0
                            && item_id.world_namespace() == namespace
                            && item_ids.insert(*item_id)
                    })
            })
        && snapshot
            .controlled_actor
            .construction_activity
            .as_ref()
            .is_none_or(|activity| {
                activity.consumed_items.iter().all(|consumed| {
                    collect_stable_item_ids(&consumed.item, namespace, &mut item_ids)
                })
            })
        && snapshot
            .ground_items
            .iter()
            .all(|ground| collect_stable_item_ids(&ground.item, namespace, &mut item_ids));
    stable_item_ids_are_valid
        && snapshot.calendar == CalendarSnapshot::at_tick(snapshot.tick)
        && snapshot.natural_light == NaturalLightSnapshot::at_tick(snapshot.tick)
        && snapshot.visible_actors.len() <= 65_536
        && snapshot.creatures.len() <= 65_536
        && snapshot.ground_items.len() <= 65_536
        && snapshot.chunks.len() <= 16_384
        && snapshot.chunks.iter().all(|chunk| {
            chunk.tiles.len() == (SUBMAP_SIZE * SUBMAP_SIZE) as usize
                && chunk.tiles.iter().flatten().all(|observed| {
                    valid_terrain_tile(&observed.terrain)
                        && observed.furniture.as_ref().is_none_or(valid_furniture_tile)
                        && (observed.bash_target != Some(BashTargetKindV1::Furniture)
                            || observed.furniture.is_some())
                        && (observed.currently_visible
                            || (observed.fields.is_empty() && observed.bash_target.is_none()))
                        && observed.fields.len() <= 16
                        && observed
                            .fields
                            .windows(2)
                            .all(|pair| pair[0].field_type_id < pair[1].field_type_id)
                        && observed.fields.iter().all(|field| {
                            !field.field_type_id.is_empty()
                                && field.field_type_id.len() <= 512
                                && field
                                    .field_type_id
                                    .chars()
                                    .all(|character| !character.is_control())
                                && (1..=16).contains(&field.intensity)
                                && !field.name.is_empty()
                                && field.name.len() <= 512
                                && field.name.chars().all(|character| !character.is_control())
                                && !field.symbol.is_empty()
                                && field.symbol.len() <= 16
                                && field
                                    .symbol
                                    .chars()
                                    .all(|character| !character.is_control())
                                && !field.color.is_empty()
                                && field.color.len() <= 64
                                && field.color.chars().all(|character| !character.is_control())
                                && field.display_sequence > 0
                        })
                })
        })
        && snapshot.controlled_actor.inventory.len() <= 4_096
        && snapshot.controlled_actor.map_memory.is_empty()
        && snapshot
            .controlled_actor
            .held_movement
            .is_none_or(HorizontalDirection::is_valid)
        && valid_base_stats(
            snapshot.controlled_actor.base_strength,
            snapshot.controlled_actor.base_dexterity,
            snapshot.controlled_actor.base_intelligence,
            snapshot.controlled_actor.base_perception,
        )
        && snapshot.controlled_actor.speed > 0
        && u32::from(snapshot.controlled_actor.speed) <= ACTION_POINT_THRESHOLD
        && snapshot.controlled_actor.action_points <= i64::from(ACTION_POINT_THRESHOLD)
        && snapshot.controlled_actor.action_points >= MIN_ACTION_POINTS
        && snapshot.controlled_actor.queued_actions.len() <= 2
        && snapshot
            .controlled_actor
            .queued_actions
            .iter()
            .all(|action| action.sequence <= snapshot.controlled_actor.last_command_sequence)
        && snapshot
            .controlled_actor
            .queued_actions
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
        && snapshot
            .controlled_actor
            .craft_activity
            .as_ref()
            .is_none_or(|activity| valid_craft_activity(activity, snapshot.controlled_actor.id))
        && snapshot
            .controlled_actor
            .read_activity
            .as_ref()
            .is_none_or(|activity| {
                valid_book_study_activity(
                    activity,
                    snapshot.controlled_actor.id,
                    snapshot.controlled_actor.base_intelligence,
                ) && (activity.interrupted
                    || snapshot.controlled_actor.inventory.iter().any(|item| {
                        item.id == activity.book_item_id
                            && item.type_id == activity.study.book_type_id
                    }))
            })
        && snapshot
            .controlled_actor
            .disassembly_activity
            .as_ref()
            .is_none_or(|activity| {
                valid_disassembly_activity(activity, snapshot.controlled_actor.id)
                    && !snapshot
                        .controlled_actor
                        .inventory
                        .iter()
                        .any(|item| item.id == activity.target_item.id)
            })
        && snapshot
            .controlled_actor
            .construction_activity
            .as_ref()
            .is_none_or(|activity| {
                valid_construction_activity(activity, snapshot.controlled_actor.id)
            })
        && usize::from(snapshot.controlled_actor.craft_activity.is_some())
            + usize::from(snapshot.controlled_actor.read_activity.is_some())
            + usize::from(snapshot.controlled_actor.disassembly_activity.is_some())
            + usize::from(snapshot.controlled_actor.construction_activity.is_some())
            <= 1
        && valid_learned_recipes(&snapshot.controlled_actor.learned_recipes)
        && valid_skill_levels(&snapshot.controlled_actor.skills, snapshot.tick)
        && valid_proficiency_levels(&snapshot.controlled_actor.proficiencies)
        && snapshot
            .controlled_actor
            .inventory
            .iter()
            .all(valid_item_snapshot)
        && snapshot
            .ground_items
            .iter()
            .all(|item| valid_item_snapshot(&item.item))
        && snapshot.creatures.iter().all(|creature| {
            creature.id.counter() > 0
                && creature.id.world_namespace() == namespace
                && !creature.type_id.is_empty()
                && creature.type_id.len() <= 512
                && creature
                    .type_id
                    .chars()
                    .all(|character| !character.is_control())
                && creature.max_hp > 0
                && creature.hp > 0
                && creature.hp <= creature.max_hp
        })
}

fn valid_character_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}

fn valid_report_details(details: &str) -> bool {
    !details.is_empty()
        && details.len() <= MAX_REPORT_BYTES
        && details.chars().count() <= MAX_REPORT_CHARACTERS
        && !details.chars().any(char::is_control)
}

fn valid_report_summary(report: &ReportSummary) -> bool {
    valid_db_sequence(report.report_id.0)
        && report.created_utc > 0
        && report.reporter_account.counter() > 0
        && report.reporter_actor.counter() > 0
        && valid_character_name(&report.reporter_character)
        && report.target_account.counter() > 0
        && report.target_actor.counter() > 0
        && valid_character_name(&report.target_character)
        && valid_report_details(&report.details)
        && match report.state {
            ReportState::Open => {
                report.resolved_utc.is_none()
                    && report.resolved_by_account.is_none()
                    && report.resolution_audit_sequence.is_none()
            }
            ReportState::Actioned | ReportState::Dismissed => {
                report.resolved_utc.is_some_and(|resolved| resolved > 0)
                    && report
                        .resolved_by_account
                        .is_some_and(|account| account.counter() > 0)
                    && report
                        .resolution_audit_sequence
                        .is_some_and(valid_db_sequence)
            }
        }
}

fn valid_moderation_history(entry: &ModerationHistoryEntry) -> bool {
    valid_db_sequence(entry.history_id)
        && valid_db_sequence(entry.security_audit_sequence)
        && entry.occurred_utc > 0
        && entry.operator_account.counter() > 0
        && entry.target_account.counter() > 0
        && match entry.kind {
            ModerationKind::Kick => entry.until_utc.is_none(),
            ModerationKind::Suspension | ModerationKind::Mute => {
                entry.until_utc.is_none_or(|until| until > 0)
            }
        }
}

fn valid_db_sequence(sequence: u64) -> bool {
    sequence > 0 && sequence <= i64::MAX as u64
}

fn validate_content(content: &ContentIdentity) -> Result<(), FrameError> {
    if content.baseline_commit.len() != 40 || content.enabled_mods.len() > MAX_ENABLED_MODS {
        return Err(FrameError::InvalidBounds);
    }
    if content.enabled_mods.iter().any(|entry| entry.len() > 512) {
        return Err(FrameError::InvalidBounds);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrameError {
    EncodedTooLarge { actual: usize, maximum: usize },
    Decode(postcard::Error),
    InvalidBounds,
}

impl fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "encoded frame is {actual} bytes; maximum is {maximum}"
                )
            }
            Self::Decode(error) => write!(formatter, "invalid Postcard frame: {error}"),
            Self::InvalidBounds => formatter.write_str("decoded message exceeds protocol bounds"),
        }
    }
}

impl std::error::Error for FrameError {}

#[must_use = "encoded frames must be sent or handled"]
pub fn encode_control(message: &ControlMessage) -> Result<Vec<u8>, FrameError> {
    message.validate()?;
    let encoded = postcard::to_stdvec(message).map_err(FrameError::Decode)?;
    if encoded.len() > MAX_CONTROL_ENCODED {
        return Err(FrameError::EncodedTooLarge {
            actual: encoded.len(),
            maximum: MAX_CONTROL_ENCODED,
        });
    }
    Ok(encoded)
}

pub fn decode_control(encoded: &[u8]) -> Result<ControlMessage, FrameError> {
    if encoded.len() > MAX_CONTROL_ENCODED {
        return Err(FrameError::EncodedTooLarge {
            actual: encoded.len(),
            maximum: MAX_CONTROL_ENCODED,
        });
    }
    let decoded: ControlMessage = postcard::from_bytes(encoded).map_err(FrameError::Decode)?;
    decoded.validate()?;
    Ok(decoded)
}

#[must_use = "encoded datagrams must be sent or handled"]
pub fn encode_client_datagram(message: &ClientDatagramV1) -> Result<Vec<u8>, FrameError> {
    match message {
        ClientDatagramV1::HeldMovement(input)
            if input
                .direction
                .is_some_and(|direction| !direction.is_valid()) =>
        {
            return Err(FrameError::InvalidBounds);
        }
        ClientDatagramV1::HeldMovement(_) => {}
    }
    let encoded = postcard::to_stdvec(message).map_err(FrameError::Decode)?;
    if encoded.len() > MAX_DATAGRAM_SIZE {
        return Err(FrameError::EncodedTooLarge {
            actual: encoded.len(),
            maximum: MAX_DATAGRAM_SIZE,
        });
    }
    Ok(encoded)
}

pub fn decode_client_datagram(encoded: &[u8]) -> Result<ClientDatagramV1, FrameError> {
    if encoded.len() > MAX_DATAGRAM_SIZE {
        return Err(FrameError::EncodedTooLarge {
            actual: encoded.len(),
            maximum: MAX_DATAGRAM_SIZE,
        });
    }
    let decoded: ClientDatagramV1 = postcard::from_bytes(encoded).map_err(FrameError::Decode)?;
    match decoded {
        ClientDatagramV1::HeldMovement(input)
            if input
                .direction
                .is_some_and(|direction| !direction.is_valid()) =>
        {
            Err(FrameError::InvalidBounds)
        }
        _ => Ok(decoded),
    }
}

#[must_use = "encoded replication snapshots must be sent or handled"]
pub fn encode_replication_snapshot(
    snapshot: &ReplicationSnapshotV1,
) -> Result<Vec<u8>, FrameError> {
    if !valid_replication_snapshot(snapshot) {
        return Err(FrameError::InvalidBounds);
    }
    let encoded = postcard::to_stdvec(snapshot).map_err(FrameError::Decode)?;
    if encoded.len() > MAX_BULK_DECODED {
        return Err(FrameError::EncodedTooLarge {
            actual: encoded.len(),
            maximum: MAX_BULK_DECODED,
        });
    }
    Ok(encoded)
}

pub fn decode_replication_snapshot(encoded: &[u8]) -> Result<ReplicationSnapshotV1, FrameError> {
    if encoded.len() > MAX_BULK_DECODED {
        return Err(FrameError::EncodedTooLarge {
            actual: encoded.len(),
            maximum: MAX_BULK_DECODED,
        });
    }
    let snapshot: ReplicationSnapshotV1 =
        postcard::from_bytes(encoded).map_err(FrameError::Decode)?;
    if !valid_replication_snapshot(&snapshot) {
        return Err(FrameError::InvalidBounds);
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn protocol_test_recipe() -> CraftRecipeV1 {
        CraftRecipeV1 {
            recipe_id: String::from("rock_sock"),
            time_moves: 500,
            output_instances: 1,
            output: CraftItemPrototypeV1 {
                type_id: String::from("rock_sock"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
                magazine_capacity: 0,
                magazine_well: None,
                residual_energy_millijoules: 0,
                powered_tool: None,
            },
            retain_components: true,
            byproducts: Vec::new(),
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
            primary_skill: Some(CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 0,
            }),
            required_skills: Vec::new(),
            autolearn: true,
            autolearn_skills: vec![CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 0,
            }],
            book_requirements: Vec::new(),
            can_be_learned: false,
        }
    }

    fn protocol_test_construction() -> ConstructionRecipeV1 {
        ConstructionRecipeV1 {
            construction_id: String::from("constr_place_table"),
            name: String::from("Place Table"),
            time_moves: 6_000,
            required_skills: vec![CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 0,
            }],
            components: vec![vec![CraftComponentRequirementV1 {
                type_id: String::from("w_table"),
                count: 1,
                count_by_charges: false,
                recoverable: true,
            }]],
            qualities: vec![vec![CraftQualityRequirementV1 {
                quality_id: String::from("HAMMER"),
                level: 2,
                amount: 1,
                providers: vec![CraftQualityProviderV1 {
                    type_id: String::from("hammer"),
                    minimum_charges: 0,
                }],
            }]],
            pre_terrain: Vec::new(),
            requires_empty: true,
            result: ConstructionResultV1::Furniture(FurnitureTileSnapshot {
                furniture_id: String::from("f_table"),
                move_cost_mod: 0,
                transparent: true,
                blocks_door: false,
                comfort: 0,
                floor_bedding_warmth: 0,
            }),
        }
    }

    fn protocol_test_disassembly_recipe() -> DisassemblyRecipeV1 {
        DisassemblyRecipeV1 {
            recipe_id: String::from("makeshift_scythe_war"),
            target_type_id: String::from("makeshift_scythe_war"),
            time_moves: 500,
            difficulty: 2,
            primary_skill_id: Some(String::from("fabrication")),
            learn_requirements: vec![CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 2,
            }],
            autolearn: false,
            autolearn_requirements: Vec::new(),
            unload_charges_as: None,
            requires_empty_charges: false,
            components: vec![DisassemblyComponentV1 {
                output_instances: 2,
                count_by_charges: false,
                output: CraftItemPrototypeV1 {
                    type_id: String::from("scrap"),
                    charges: 1,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::new(),
                    ranged_weapon: None,
                    magazine_capacity: 0,
                    magazine_well: None,
                    residual_energy_millijoules: 0,
                    powered_tool: None,
                },
                output_state: Some(ItemComponentSnapshotV1 {
                    type_id: String::from("scrap"),
                    charges: 1,
                    damage: 0,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::new(),
                    ranged_weapon: None,
                    count_by_charges: false,
                    recoverable: true,
                    component_provenance: None,
                    magazine_capacity: 0,
                    magazine_well: None,
                    residual_energy_millijoules: 0,
                    powered_tool: None,
                }),
            }],
            tools: Vec::new(),
            qualities: Vec::new(),
        }
    }

    #[test]
    fn stable_id_round_trips_its_parts() {
        let id = ActorId::new(0xfedc_ba98_7654_3210, 42);
        assert_eq!(id.world_namespace(), 0xfedc_ba98_7654_3210);
        assert_eq!(id.counter(), 42);
        assert_eq!(id.to_string(), "fedcba9876543210:000000000000002a");
    }

    #[test]
    fn character_creation_stats_use_pinned_freeform_bounds() {
        let request = |base_stats| {
            ControlMessage::CharacterRequest(CharacterRequest::Create {
                name: String::from("Survivor"),
                base_stats,
            })
        };
        let defaults = CharacterCreationStatsV1::default();
        assert_eq!(
            defaults,
            CharacterCreationStatsV1 {
                strength: 8,
                dexterity: 8,
                intelligence: 8,
                perception: 8,
            }
        );
        let encoded = encode_control(&request(defaults)).expect("default stats should encode");
        assert_eq!(
            decode_control(&encoded).expect("creation request should decode"),
            request(defaults)
        );
        for invalid in [
            CharacterCreationStatsV1 {
                strength: MIN_CHARACTER_CREATION_STAT - 1,
                ..defaults
            },
            CharacterCreationStatsV1 {
                perception: MAX_CHARACTER_CREATION_STAT + 1,
                ..defaults
            },
        ] {
            assert_eq!(
                encode_control(&request(invalid)),
                Err(FrameError::InvalidBounds)
            );
        }
    }

    #[test]
    fn held_movement_datagram_is_versioned_bounded_and_strict() {
        let input = HeldMovementInputV1 {
            actor_id: ActorId::new(1, 2),
            sequence: HeldInputSequence(7),
            client_tick: SimTick(11),
            direction: Some(HorizontalDirection { dx: -1, dy: 1 }),
        };
        let encoded = encode_client_datagram(&ClientDatagramV1::HeldMovement(input))
            .expect("valid held movement should encode");
        assert!(encoded.len() < REQUIRED_DATAGRAM_SIZE);
        assert_eq!(
            decode_client_datagram(&encoded).expect("held movement should decode"),
            ClientDatagramV1::HeldMovement(input)
        );
        let invalid = ClientDatagramV1::HeldMovement(HeldMovementInputV1 {
            direction: Some(HorizontalDirection { dx: 2, dy: 0 }),
            ..input
        });
        assert_eq!(
            encode_client_datagram(&invalid),
            Err(FrameError::InvalidBounds)
        );
    }

    #[test]
    fn negative_positions_use_euclidean_chunks() {
        let position = WorldPosition {
            x: -1,
            y: -13,
            z: 2,
        };
        let (chunk, local) = position.chunk_and_local();
        assert_eq!(chunk, ChunkCoord { x: -1, y: -2, z: 2 });
        assert_eq!(local, LocalTileCoord { x: 11, y: 11 });
    }

    #[test]
    fn control_frame_round_trip_is_versioned_and_bounded() {
        let message = ControlMessage::Heartbeat { tick: SimTick(50) };
        let encoded = encode_control(&message).expect("heartbeat should encode");
        let decoded = decode_control(&encoded).expect("heartbeat should decode");
        assert_eq!(decoded, message);

        let activate = |item_id| {
            ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(7, 1),
                sequence: CommandSequence(1),
                client_tick: SimTick(50),
                kind: CommandKind::Activate { item_id },
            })
        };
        let valid = activate(ItemId::new(7, 2));
        assert_eq!(
            decode_control(&encode_control(&valid).expect("activate should encode"))
                .expect("activate should decode"),
            valid
        );
        assert_eq!(
            encode_control(&activate(ItemId::new(8, 2))),
            Err(FrameError::InvalidBounds),
            "item commands cannot cross a world namespace"
        );
        assert_eq!(
            encode_control(&activate(ItemId::new(7, 0))),
            Err(FrameError::InvalidBounds),
            "stable item ID zero is invalid"
        );
    }

    #[test]
    fn craft_request_placeholder_and_normalized_definition_round_trip_strictly() {
        let command = |recipe| {
            ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(1, 2),
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::Craft {
                    recipe_id: String::from("rock_sock"),
                    recipe,
                },
            })
        };
        for message in [
            command(None),
            command(Some(Box::new(protocol_test_recipe()))),
        ] {
            let encoded = encode_control(&message).expect("valid craft should encode");
            assert_eq!(
                decode_control(&encoded).expect("valid craft should decode"),
                message
            );
        }

        let byproduct = |type_id: &str, output_instances| CraftByproductV1 {
            output_instances,
            output: CraftItemPrototypeV1 {
                type_id: type_id.to_owned(),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
                magazine_capacity: 0,
                magazine_well: None,
                residual_energy_millijoules: 0,
                powered_tool: None,
            },
        };
        let mut with_byproducts = protocol_test_recipe();
        with_byproducts.byproducts = vec![byproduct("dust", 2), byproduct("splinter", 3)];
        assert!(encode_control(&command(Some(Box::new(with_byproducts)))).is_ok());

        let mut unsorted_byproducts = protocol_test_recipe();
        unsorted_byproducts.byproducts = vec![byproduct("splinter", 1), byproduct("dust", 1)];
        assert_eq!(
            encode_control(&command(Some(Box::new(unsorted_byproducts)))),
            Err(FrameError::InvalidBounds)
        );
        let mut duplicate_byproducts = protocol_test_recipe();
        duplicate_byproducts.byproducts = vec![byproduct("dust", 1), byproduct("dust", 1)];
        assert_eq!(
            encode_control(&command(Some(Box::new(duplicate_byproducts)))),
            Err(FrameError::InvalidBounds)
        );
        let mut zero_byproduct = protocol_test_recipe();
        zero_byproduct.byproducts = vec![byproduct("dust", 0)];
        assert_eq!(
            encode_control(&command(Some(Box::new(zero_byproduct)))),
            Err(FrameError::InvalidBounds)
        );
        let mut excessive_total_output = protocol_test_recipe();
        excessive_total_output.byproducts = vec![byproduct("dust", MAX_CRAFT_OUTPUT_INSTANCES)];
        assert_eq!(
            encode_control(&command(Some(Box::new(excessive_total_output)))),
            Err(FrameError::InvalidBounds)
        );

        let mut mismatched = protocol_test_recipe();
        mismatched.recipe_id = String::from("different_recipe");
        assert_eq!(
            encode_control(&command(Some(Box::new(mismatched)))),
            Err(FrameError::InvalidBounds)
        );
        let mut inconsistent_charge_mode = protocol_test_recipe();
        inconsistent_charge_mode
            .components
            .push(vec![CraftComponentRequirementV1 {
                type_id: String::from("rock"),
                count: 1,
                count_by_charges: true,
                recoverable: true,
            }]);
        assert_eq!(
            encode_control(&command(Some(Box::new(inconsistent_charge_mode)))),
            Err(FrameError::InvalidBounds)
        );
        let mut invalid_support = protocol_test_recipe();
        invalid_support.tools = vec![vec![CraftToolRequirementV1 {
            type_id: String::from("hammer"),
            amount: 0,
            consumes_charges: false,
        }]];
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid_support)))),
            Err(FrameError::InvalidBounds)
        );
        let mut excessive_presence = protocol_test_recipe();
        excessive_presence.tools = vec![vec![CraftToolRequirementV1 {
            type_id: String::from("hammer"),
            amount: 257,
            consumes_charges: false,
        }]];
        assert_eq!(
            encode_control(&command(Some(Box::new(excessive_presence)))),
            Err(FrameError::InvalidBounds)
        );
        let mut maximum_charged_tool = protocol_test_recipe();
        maximum_charged_tool.tools = vec![vec![CraftToolRequirementV1 {
            type_id: String::from("welder"),
            amount: u16::MAX,
            consumes_charges: true,
        }]];
        assert!(encode_control(&command(Some(Box::new(maximum_charged_tool)))).is_ok());
        let mut unsorted_providers = protocol_test_recipe();
        unsorted_providers.qualities = vec![vec![CraftQualityRequirementV1 {
            quality_id: String::from("CUT"),
            level: 1,
            amount: 1,
            providers: vec![
                CraftQualityProviderV1 {
                    type_id: String::from("knife"),
                    minimum_charges: 0,
                },
                CraftQualityProviderV1 {
                    type_id: String::from("blade"),
                    minimum_charges: 5,
                },
            ],
        }]];
        assert_eq!(
            encode_control(&command(Some(Box::new(unsorted_providers)))),
            Err(FrameError::InvalidBounds)
        );
        let mut unsorted_skills = protocol_test_recipe();
        unsorted_skills.required_skills = vec![
            CraftSkillRequirementV1 {
                skill_id: String::from("survival"),
                level: 1,
            },
            CraftSkillRequirementV1 {
                skill_id: String::from("fabrication"),
                level: 1,
            },
        ];
        assert_eq!(
            encode_control(&command(Some(Box::new(unsorted_skills)))),
            Err(FrameError::InvalidBounds)
        );
        let mut excessive_skill = protocol_test_recipe();
        excessive_skill.primary_skill = Some(CraftSkillRequirementV1 {
            skill_id: String::from("fabrication"),
            level: MAX_SKILL_LEVEL + 1,
        });
        assert_eq!(
            encode_control(&command(Some(Box::new(excessive_skill)))),
            Err(FrameError::InvalidBounds)
        );
        let mut book_only = protocol_test_recipe();
        book_only.autolearn = false;
        book_only.autolearn_skills.clear();
        book_only.book_requirements = vec![CraftBookRequirementV1 {
            book_type_id: String::from("manual_fabrication"),
            required_skill_level: 2,
        }];
        assert!(encode_control(&command(Some(Box::new(book_only.clone())))).is_ok());
        let mut no_knowledge_source = book_only.clone();
        no_knowledge_source.book_requirements.clear();
        assert_eq!(
            encode_control(&command(Some(Box::new(no_knowledge_source)))),
            Err(FrameError::InvalidBounds)
        );
        let mut unsorted_books = book_only.clone();
        unsorted_books.book_requirements = vec![
            CraftBookRequirementV1 {
                book_type_id: String::from("manual_survival"),
                required_skill_level: 1,
            },
            CraftBookRequirementV1 {
                book_type_id: String::from("manual_fabrication"),
                required_skill_level: 1,
            },
        ];
        assert_eq!(
            encode_control(&command(Some(Box::new(unsorted_books)))),
            Err(FrameError::InvalidBounds)
        );
        let mut excessive_book_skill = book_only;
        excessive_book_skill.book_requirements[0].required_skill_level = MAX_SKILL_LEVEL + 1;
        assert_eq!(
            encode_control(&command(Some(Box::new(excessive_book_skill)))),
            Err(FrameError::InvalidBounds)
        );
        let valid_proficiency = CraftProficiencyV1 {
            proficiency_id: String::from("prof_metalworking"),
            required: false,
            time_multiplier_millionths: 1_500_000,
            skill_penalty_millionths: 500_000,
            learning_time_multiplier_millionths: CRAFT_PROFICIENCY_SCALE,
            max_experience_action_points: None,
            time_to_learn_action_points: 14_400_000,
            can_learn: true,
            required_proficiencies: Vec::new(),
        };
        let mut with_proficiency = protocol_test_recipe();
        with_proficiency.proficiencies = vec![valid_proficiency.clone()];
        assert!(encode_control(&command(Some(Box::new(with_proficiency)))).is_ok());

        let mut required_with_time = protocol_test_recipe();
        required_with_time.proficiencies = vec![CraftProficiencyV1 {
            required: true,
            ..valid_proficiency.clone()
        }];
        assert_eq!(
            encode_control(&command(Some(Box::new(required_with_time)))),
            Err(FrameError::InvalidBounds)
        );
        let mut required_with_penalty = protocol_test_recipe();
        required_with_penalty.proficiencies = vec![CraftProficiencyV1 {
            required: true,
            time_multiplier_millionths: 0,
            ..valid_proficiency.clone()
        }];
        assert_eq!(
            encode_control(&command(Some(Box::new(required_with_penalty)))),
            Err(FrameError::InvalidBounds)
        );
        let mut unsorted_proficiencies = protocol_test_recipe();
        unsorted_proficiencies.proficiencies = vec![
            CraftProficiencyV1 {
                proficiency_id: String::from("prof_welding"),
                ..valid_proficiency.clone()
            },
            valid_proficiency,
        ];
        assert_eq!(
            encode_control(&command(Some(Box::new(unsorted_proficiencies)))),
            Err(FrameError::InvalidBounds)
        );
    }

    #[test]
    fn construction_request_placeholder_and_normalized_definition_round_trip_strictly() {
        let command = |construction| {
            ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(1, 2),
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::Construct {
                    target: WorldPosition { x: 2, y: 2, z: 0 },
                    construction_id: String::from("constr_place_table"),
                    construction,
                },
            })
        };
        for message in [
            command(None),
            command(Some(Box::new(protocol_test_construction()))),
        ] {
            let encoded = encode_control(&message).expect("valid construction should encode");
            assert_eq!(
                decode_control(&encoded).expect("valid construction should decode"),
                message
            );
        }
        let mut invalid = protocol_test_construction();
        invalid.components.clear();
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid)))),
            Err(FrameError::InvalidBounds)
        );
        let mut invalid_quality = protocol_test_construction();
        invalid_quality.qualities[0][0].providers.clear();
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid_quality)))),
            Err(FrameError::InvalidBounds)
        );
    }

    #[test]
    fn book_study_request_is_placeholder_or_strict_server_definition() {
        assert_eq!(adjusted_book_study_time_moves(90_000, 12, 8), Some(96_000));
        assert_eq!(adjusted_book_study_time_moves(90_000, 12, 12), Some(90_000));
        assert_eq!(adjusted_book_study_time_moves(90_000, 12, 0), None);
        let study = BookStudyV1 {
            book_type_id: String::from("manual_pistol"),
            skill_id: String::from("pistol"),
            required_skill_level: 0,
            maximum_skill_level: 3,
            intelligence_requirement: 3,
            time_moves: 90_000,
            source_time_minutes: 15,
        };
        let command = |study| {
            ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(1, 2),
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::ReadBook {
                    item_id: ItemId::new(1, 3),
                    book_type_id: String::from("manual_pistol"),
                    study,
                },
            })
        };
        for message in [command(None), command(Some(Box::new(study.clone())))] {
            let encoded = encode_control(&message).expect("valid study request should encode");
            assert_eq!(
                decode_control(&encoded).expect("study should decode"),
                message
            );
        }
        let mut mismatched = study.clone();
        mismatched.book_type_id = String::from("other_book");
        assert_eq!(
            encode_control(&command(Some(Box::new(mismatched)))),
            Err(FrameError::InvalidBounds)
        );
        let mut invalid_levels = study.clone();
        invalid_levels.maximum_skill_level = invalid_levels.required_skill_level;
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid_levels)))),
            Err(FrameError::InvalidBounds)
        );
        let mut invalid_intelligence = study.clone();
        invalid_intelligence.intelligence_requirement = MAX_ACTOR_BASE_STAT + 1;
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid_intelligence)))),
            Err(FrameError::InvalidBounds)
        );
        let mut excessive_time = study;
        excessive_time.time_moves = MAX_BOOK_STUDY_MOVES + 1;
        assert_eq!(
            encode_control(&command(Some(Box::new(excessive_time)))),
            Err(FrameError::InvalidBounds)
        );
    }

    #[test]
    fn disassembly_request_is_placeholder_or_strict_server_definition() {
        let recipe = protocol_test_disassembly_recipe();
        let command = |recipe| {
            ControlMessage::Command(ClientCommand {
                actor_id: ActorId::new(1, 2),
                sequence: CommandSequence(1),
                client_tick: SimTick(0),
                kind: CommandKind::Disassemble {
                    item_id: ItemId::new(1, 3),
                    item_type_id: String::from("makeshift_scythe_war"),
                    recipe,
                },
            })
        };
        for message in [command(None), command(Some(Box::new(recipe.clone())))] {
            let encoded = encode_control(&message).expect("valid disassembly should encode");
            assert_eq!(
                decode_control(&encoded).expect("disassembly should decode"),
                message
            );
        }
        let mut mismatched = recipe.clone();
        mismatched.target_type_id = String::from("other_item");
        assert_eq!(
            encode_control(&command(Some(Box::new(mismatched)))),
            Err(FrameError::InvalidBounds)
        );
        let mut charged_tool = recipe.clone();
        charged_tool.tools = vec![vec![CraftToolRequirementV1 {
            type_id: String::from("welder"),
            amount: 1,
            consumes_charges: true,
        }]];
        assert_eq!(
            encode_control(&command(Some(Box::new(charged_tool)))),
            Err(FrameError::InvalidBounds)
        );
        let mut invalid_charged_component = recipe;
        invalid_charged_component.components[0].count_by_charges = true;
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid_charged_component)))),
            Err(FrameError::InvalidBounds)
        );

        let mut invalid_unload = protocol_test_disassembly_recipe();
        invalid_unload.unload_charges_as = Some(CraftItemPrototypeV1 {
            type_id: String::from("test_round"),
            charges: 1,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::new(),
            ranged_weapon: None,
            magazine_capacity: 0,
            magazine_well: None,
            residual_energy_millijoules: 0,
            powered_tool: None,
        });
        assert_eq!(
            encode_control(&command(Some(Box::new(invalid_unload)))),
            Err(FrameError::InvalidBounds)
        );
        let mut contradictory = protocol_test_disassembly_recipe();
        contradictory.unload_charges_as = Some(CraftItemPrototypeV1 {
            type_id: String::from("battery"),
            charges: 100,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::from("battery"),
            ranged_weapon: None,
            magazine_capacity: 0,
            magazine_well: None,
            residual_energy_millijoules: 0,
            powered_tool: None,
        });
        contradictory.requires_empty_charges = true;
        assert_eq!(
            encode_control(&command(Some(Box::new(contradictory)))),
            Err(FrameError::InvalidBounds),
            "a recipe cannot both unload modeled charges and require an empty target"
        );
    }

    #[test]
    fn craft_activity_rejects_duplicate_or_unowned_reserved_ids() {
        let actor_id = ActorId::new(3, 1);
        let consumed = CraftConsumedItemV1 {
            item: ItemSnapshot {
                id: ItemId::new(3, 2),
                type_id: String::from("rock"),
                charges: 1,
                damage: 0,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
                component_provenance: None,
                magazine_capacity: 0,
                magazine_well: None,
                residual_energy_millijoules: 0,
                powered_tool: None,
                creature_corpse: None,
            },
            split_from: None,
        };
        let mut activity = CraftActivitySnapshotV1 {
            recipe: protocol_test_recipe(),
            selected_tool_alternatives: Vec::new(),
            remaining_action_points: 10_000,
            consumed_items: vec![consumed.clone()],
            reserved_output_items: vec![ItemId::new(3, 3)],
            previously_wielded: Some(consumed.item.id),
            practice_ticks_awarded: 0,
            proficiency_progress_millionths: 0,
            proficiency_buckets_awarded: 0,
            interrupted: false,
        };
        assert!(valid_craft_activity(&activity, actor_id));
        activity.recipe.byproducts = vec![CraftByproductV1 {
            output_instances: 2,
            output: CraftItemPrototypeV1 {
                type_id: String::from("splinter"),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
                magazine_capacity: 0,
                magazine_well: None,
                residual_energy_millijoules: 0,
                powered_tool: None,
            },
        }];
        assert!(!valid_craft_activity(&activity, actor_id));
        activity
            .reserved_output_items
            .extend([ItemId::new(3, 4), ItemId::new(3, 5)]);
        assert!(valid_craft_activity(&activity, actor_id));
        activity.recipe.byproducts.clear();
        activity.reserved_output_items.truncate(1);
        activity.practice_ticks_awarded = 1;
        assert!(!valid_craft_activity(&activity, actor_id));
        activity.remaining_action_points = 8_000;
        assert!(valid_craft_activity(&activity, actor_id));
        activity.practice_ticks_awarded = 0;
        activity.remaining_action_points = 10_000;
        activity.recipe.tools = vec![vec![CraftToolRequirementV1 {
            type_id: String::from("welder"),
            amount: 20,
            consumes_charges: true,
        }]];
        assert!(!valid_craft_activity(&activity, actor_id));
        activity.selected_tool_alternatives = vec![0];
        assert!(valid_craft_activity(&activity, actor_id));
        activity.selected_tool_alternatives[0] = 1;
        assert!(!valid_craft_activity(&activity, actor_id));
        activity.recipe.tools.clear();
        activity.selected_tool_alternatives.clear();
        activity.reserved_output_items[0] = consumed.item.id;
        assert!(!valid_craft_activity(&activity, actor_id));
        activity.reserved_output_items[0] = ItemId::new(4, 3);
        assert!(!valid_craft_activity(&activity, actor_id));
    }

    #[test]
    fn account_key_responses_enforce_binding_bounds_and_pending_shape() {
        let pending = EndpointBindingSummary {
            endpoint: EndpointIdentity([1; 32]),
            state: EndpointBindingState::Pending,
            pending_expires_utc: Some(1),
        };
        assert!(
            encode_control(&ControlMessage::AccountKeyResponse(
                AccountKeyResponse::Pending(pending)
            ))
            .is_ok()
        );
        let invalid_expiry = EndpointBindingSummary {
            pending_expires_utc: Some(0),
            ..pending
        };
        assert_eq!(
            encode_control(&ControlMessage::AccountKeyResponse(
                AccountKeyResponse::Pending(invalid_expiry)
            )),
            Err(FrameError::InvalidBounds)
        );
        let invalid_active = EndpointBindingSummary {
            state: EndpointBindingState::Active,
            ..pending
        };
        assert_eq!(
            encode_control(&ControlMessage::AccountKeyResponse(
                AccountKeyResponse::Bindings(vec![invalid_active])
            )),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AccountKeyResponse(
                AccountKeyResponse::Bindings(vec![pending; 257])
            )),
            Err(FrameError::InvalidBounds)
        );
    }

    #[test]
    fn admin_messages_enforce_pages_and_public_status_transitions() {
        assert!(
            encode_control(&ControlMessage::AdminRequest(AdminRequest::ListAccounts {
                after: None,
                limit: MAX_ADMIN_ACCOUNTS_PER_PAGE,
            }))
            .is_ok()
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminRequest(AdminRequest::ListAccounts {
                after: None,
                limit: 0,
            })),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminRequest(AdminRequest::SetStatus {
                account_id: AccountId::new(1, 1),
                status: AccountStatus::RecoveryLocked,
            })),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminRequest(AdminRequest::CreateAccount {
                display_name: String::from("invalid\nname"),
                role: AccountRole::Player,
                endpoint: EndpointIdentity([1; 32]),
            })),
            Err(FrameError::InvalidBounds)
        );
        let account = AdminAccountSummary {
            account_id: AccountId::new(1, 2),
            display_name: "🦀".repeat(64),
            role: AccountRole::Player,
            status: AccountStatus::Enabled,
            suspended_until_utc: None,
            muted_until_utc: Some(1),
        };
        let largest_page =
            encode_control(&ControlMessage::AdminResponse(AdminResponse::Accounts {
                accounts: vec![account; usize::from(MAX_ADMIN_ACCOUNTS_PER_PAGE)],
                next_after: Some(AccountId::new(1, 2)),
            }))
            .expect("a worst-case bounded account page should fit the control frame");
        assert!(largest_page.len() <= MAX_CONTROL_ENCODED);
        let pending_binding = EndpointBindingSummary {
            endpoint: EndpointIdentity([2; 32]),
            state: EndpointBindingState::Pending,
            pending_expires_utc: Some(i64::MAX),
        };
        let endpoint_page =
            encode_control(&ControlMessage::AdminResponse(AdminResponse::Endpoints {
                account_id: AccountId::new(1, 2),
                bindings: vec![pending_binding; 256],
            }))
            .expect("the maximum endpoint page should fit a control frame");
        assert!(endpoint_page.len() <= MAX_CONTROL_ENCODED);
        assert_eq!(
            encode_control(&ControlMessage::AdminResponse(AdminResponse::Characters {
                account_id: AccountId::new(1, 2),
                characters: vec![CharacterSummary {
                    actor_id: ActorId::new(1, 3),
                    name: String::from("Survivor"),
                }],
                gameplay_session_active: false,
                controlled_actor: Some(ActorId::new(1, 3)),
            })),
            Err(FrameError::InvalidBounds)
        );
        encode_control(&ControlMessage::AdminResponse(AdminResponse::Characters {
            account_id: AccountId::new(1, 2),
            characters: vec![CharacterSummary {
                actor_id: ActorId::new(1, 3),
                name: String::from("Survivor"),
            }],
            gameplay_session_active: true,
            controlled_actor: Some(ActorId::new(1, 3)),
        }))
        .expect("active session metadata should encode");
        let private_character = PrivateCharacterInspection {
            tick: SimTick(10),
            account_id: AccountId::new(1, 2),
            actor_id: ActorId::new(1, 3),
            name: String::from("Survivor"),
            position: WorldPosition { x: 1, y: 2, z: 0 },
            hp: 100,
            base_strength: 8,
            base_dexterity: 8,
            base_intelligence: 8,
            base_perception: 8,
            connected: true,
            last_command_sequence: CommandSequence(0),
            last_held_input_sequence: HeldInputSequence(0),
            held_movement: None,
            wielded: None,
            stored_kcal: 55_000,
            thirst: 0,
            sleepiness: 0,
            sleeping: false,
            sleep_intervals: 0,
            speed: 100,
            action_points: 0,
            queued_actions: Vec::new(),
            craft_activity: None,
            read_activity: None,
            disassembly_activity: None,
            construction_activity: None,
            learned_recipe_count: 0,
            skills: Vec::new(),
            proficiencies: Vec::new(),
            inventory_total: 0,
            inventory: Vec::new(),
            next_inventory_after: None,
            map_memory_chunks: 1,
        };
        encode_control(&ControlMessage::AdminResponse(
            AdminResponse::PrivateCharacter(Box::new(private_character.clone())),
        ))
        .expect("bounded private character inspection should encode");
        assert_eq!(
            encode_control(&ControlMessage::AdminResponse(
                AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                    base_strength: 0,
                    ..private_character.clone()
                }))
            )),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminResponse(
                AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                    base_intelligence: 0,
                    ..private_character.clone()
                }))
            )),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminResponse(
                AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                    skills: vec![SkillLevelSnapshot {
                        skill_id: String::from("fabrication"),
                        practical_level: 2,
                        practical_experience: 0,
                        theoretical_level: 1,
                        theoretical_experience: 0,
                        last_practiced: SimTick(10),
                    }],
                    ..private_character.clone()
                }))
            )),
            Err(FrameError::InvalidBounds)
        );
        let melee_damage_milli: BTreeMap<String, i32> = (0..32)
            .map(|index| (format!("damage-{index:02}-{}", "x".repeat(54)), i32::MAX))
            .collect();
        let maximum_inventory = (0..MAX_ADMIN_INVENTORY_PER_PAGE)
            .map(|index| ItemSnapshot {
                id: ItemId::new(1, u64::from(index) + 4),
                type_id: "x".repeat(512),
                charges: i32::MAX,
                damage: MAX_ITEM_DAMAGE_LEVEL,
                melee_damage_milli: melee_damage_milli.clone(),
                calories: i32::MAX,
                quench: i32::MAX,
                comestible_type: "x".repeat(32),
                ammunition_type: "x".repeat(64),
                ranged_weapon: Some(RangedWeaponSnapshot {
                    ammunition_type: "x".repeat(64),
                    ammunition_remaining: u16::MAX,
                    ammunition_capacity: u16::MAX,
                    range: u16::MAX,
                    damage: u16::MAX,
                    dispersion: u16::MAX,
                    sound_volume: u16::MAX,
                }),
                component_provenance: None,
                magazine_capacity: 0,
                magazine_well: None,
                residual_energy_millijoules: 0,
                powered_tool: None,
                creature_corpse: None,
            })
            .collect::<Vec<_>>();
        encode_control(&ControlMessage::AdminResponse(
            AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                inventory_total: 2,
                next_inventory_after: maximum_inventory.first().map(|item| item.id),
                inventory: maximum_inventory.iter().take(1).cloned().collect(),
                ..private_character.clone()
            })),
        ))
        .expect("a requested one-item page should carry a continuation cursor");
        let mut invalid_damage = maximum_inventory[0].clone();
        invalid_damage.damage = MAX_ITEM_DAMAGE_LEVEL + 1;
        assert_eq!(
            encode_control(&ControlMessage::AdminResponse(
                AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                    inventory_total: 1,
                    next_inventory_after: None,
                    inventory: vec![invalid_damage],
                    ..private_character.clone()
                }))
            )),
            Err(FrameError::InvalidBounds)
        );
        let maximum_skills = (0..MAX_SKILLS)
            .map(|index| SkillLevelSnapshot {
                skill_id: format!("{index:02}{}", "x".repeat(62)),
                practical_level: MAX_SKILL_LEVEL,
                practical_experience: u32::MAX,
                theoretical_level: MAX_SKILL_LEVEL,
                theoretical_experience: u32::MAX,
                last_practiced: SimTick(10),
            })
            .collect();
        let largest_private = encode_control(&ControlMessage::AdminResponse(
            AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                inventory_total: 256,
                next_inventory_after: maximum_inventory.last().map(|item| item.id),
                inventory: maximum_inventory,
                skills: maximum_skills,
                ..private_character.clone()
            })),
        ))
        .expect("the maximum private inventory page should encode");
        assert!(largest_private.len() <= MAX_CONTROL_ENCODED);
        assert_eq!(
            encode_control(&ControlMessage::AdminResponse(
                AdminResponse::PrivateCharacter(Box::new(PrivateCharacterInspection {
                    next_inventory_after: Some(ItemId::new(1, 4)),
                    ..private_character
                }))
            )),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminRequest(AdminRequest::SetSuspension {
                account_id: AccountId::new(1, 2),
                duration_seconds: Some(MAX_MODERATION_DURATION_SECONDS + 1),
            })),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::ReportSubmit(PlayerReport {
                target_actor: ActorId::new(1, 3),
                reason: ReportReason::Chat,
                details: String::new(),
            })),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::ReportResponse(ReportResponse::Accepted {
                report_id: ReportId(0),
            })),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminRequest(AdminRequest::ListReports {
                state: Some(ReportState::Open),
                after: Some(ReportId(i64::MAX as u64 + 1)),
                limit: 1,
            })),
            Err(FrameError::InvalidBounds)
        );
        assert_eq!(
            encode_control(&ControlMessage::AdminRequest(
                AdminRequest::SetReportState {
                    report_id: ReportId(1),
                    state: ReportState::Open,
                }
            )),
            Err(FrameError::InvalidBounds)
        );
        let report = ReportSummary {
            report_id: ReportId(1),
            created_utc: 1,
            reporter_account: AccountId::new(1, 1),
            reporter_actor: ActorId::new(1, 1),
            reporter_character: "🦀".repeat(64),
            target_account: AccountId::new(1, 2),
            target_actor: ActorId::new(1, 2),
            target_character: "🦀".repeat(64),
            reason: ReportReason::Other,
            details: "é".repeat(MAX_REPORT_CHARACTERS),
            state: ReportState::Actioned,
            resolved_utc: Some(i64::MAX),
            resolved_by_account: Some(AccountId::new(u64::MAX, u64::MAX)),
            resolution_audit_sequence: Some(i64::MAX as u64),
        };
        let largest_reports =
            encode_control(&ControlMessage::AdminResponse(AdminResponse::Reports {
                reports: vec![report; usize::from(MAX_REPORTS_PER_PAGE)],
                next_after: Some(ReportId(1)),
            }))
            .expect("a worst-case bounded report page should fit the control frame");
        assert!(largest_reports.len() <= MAX_CONTROL_ENCODED);
        let history = ModerationHistoryEntry {
            history_id: 1,
            security_audit_sequence: 1,
            occurred_utc: 1,
            operator_account: AccountId::new(1, 1),
            target_account: AccountId::new(1, 2),
            kind: ModerationKind::Suspension,
            until_utc: Some(2),
        };
        encode_control(&ControlMessage::AdminResponse(
            AdminResponse::ModerationHistory {
                account_id: AccountId::new(1, 2),
                entries: vec![history; usize::from(MAX_MODERATION_HISTORY_PER_PAGE)],
                next_after: Some(1),
            },
        ))
        .expect("a maximum moderation-history page should fit the control frame");
    }

    #[test]
    fn oversized_chat_is_rejected_before_encoding() {
        let message = ControlMessage::ChatSend {
            text: "x".repeat(MAX_CHAT_BYTES + 1),
        };
        assert_eq!(encode_control(&message), Err(FrameError::InvalidBounds));
    }

    #[test]
    fn detachable_magazine_snapshots_are_bounded_and_keep_unique_stable_ids() {
        let magazine = ItemSnapshot {
            id: ItemId::new(1, 2),
            type_id: String::from("medium_battery"),
            charges: 0,
            damage: 0,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::from("battery"),
            ranged_weapon: None,
            component_provenance: None,
            magazine_capacity: 10,
            magazine_well: None,
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
        };
        assert!(valid_item_snapshot(&magazine));
        let tool = ItemSnapshot {
            id: ItemId::new(1, 1),
            type_id: String::from("flashlight"),
            charges: 0,
            damage: 0,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::new(),
            ranged_weapon: None,
            component_provenance: None,
            magazine_capacity: 0,
            magazine_well: Some(MagazineWellSnapshotV1 {
                compatible_magazine_type_ids: vec![String::from("medium_battery")],
                installed_magazine: Some(Box::new(magazine.clone())),
            }),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
        };
        assert!(valid_item_snapshot(&tool));
        let mut ids = BTreeSet::new();
        assert!(collect_stable_item_ids(&tool, 1, &mut ids));
        assert_eq!(ids, BTreeSet::from([tool.id, magazine.id]));
        let duplicate_root = ItemSnapshot {
            id: ItemId::new(1, 3),
            ..tool.clone()
        };
        assert!(!collect_stable_item_ids(&duplicate_root, 1, &mut ids));

        let mut hidden_parent_charges = tool.clone();
        hidden_parent_charges.charges = 1;
        assert!(!valid_item_snapshot(&hidden_parent_charges));
        let mut incompatible = tool.clone();
        incompatible
            .magazine_well
            .as_mut()
            .expect("well exists")
            .compatible_magazine_type_ids = vec![String::from("other_battery")];
        assert!(!valid_item_snapshot(&incompatible));
        let mut overfilled = magazine;
        overfilled.charges = 11;
        assert!(!valid_item_snapshot(&overfilled));

        let mut powered = tool;
        powered.powered_tool = Some(PoweredToolStateV1 {
            inactive_type_id: String::from("flashlight"),
            active_type_id: String::from("flashlight_on"),
            activation_charges: 1,
            power_draw_milliwatts: 1_560,
            light_emission: 300,
            dims_with_charge: true,
            active: false,
        });
        assert!(valid_item_snapshot(&powered));
        powered.type_id = String::from("flashlight_on");
        powered
            .powered_tool
            .as_mut()
            .expect("powered state exists")
            .active = true;
        assert!(valid_item_snapshot(&powered));
        powered.type_id = String::from("flashlight");
        assert!(!valid_item_snapshot(&powered));

        let mut fractional_battery = powered
            .magazine_well
            .as_ref()
            .and_then(|well| well.installed_magazine.as_deref())
            .expect("installed magazine exists")
            .clone();
        fractional_battery.residual_energy_millijoules = MILLIJOULES_PER_BATTERY_CHARGE - 1;
        assert!(valid_item_snapshot(&fractional_battery));
        fractional_battery.charges = fractional_battery.magazine_capacity as i32;
        assert!(!valid_item_snapshot(&fractional_battery));
        fractional_battery.charges = 0;
        fractional_battery.residual_energy_millijoules = MILLIJOULES_PER_BATTERY_CHARGE;
        assert!(!valid_item_snapshot(&fractional_battery));
    }

    #[test]
    fn creature_corpse_snapshot_is_strict_and_self_contained() {
        let prototype = CreatureCorpsePrototypeV1 {
            monster_type_id: String::from("mon_zombie"),
            max_hp: 80,
            speed: 70,
            attack_cost_moves: 100,
            aggression: 100,
            melee_skill: 4,
            dodge: 1,
            size: CreatureSizeV1::Medium,
            melee_dice: 2,
            melee_dice_sides: 3,
            can_see: true,
            vision_day: 40,
            vision_night: 3,
            stumbles: false,
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
            revives: true,
        };
        let corpse = ItemSnapshot {
            id: ItemId::new(1, 9),
            type_id: String::from("corpse"),
            charges: 1,
            damage: 1,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::new(),
            ranged_weapon: None,
            component_provenance: None,
            magazine_capacity: 0,
            magazine_well: None,
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: Some(CreatureCorpseSnapshotV1 {
                prototype,
                death_tick: SimTick(20),
                revive_special: false,
                revivable: true,
            }),
        };
        assert!(valid_item_snapshot(&corpse));
        let mut disguised = corpse.clone();
        disguised.type_id = String::from("rock");
        assert!(!valid_item_snapshot(&disguised));
        let mut contradictory = corpse.clone();
        contradictory
            .creature_corpse
            .as_mut()
            .expect("corpse metadata should exist")
            .prototype
            .revives = false;
        assert!(!valid_item_snapshot(&contradictory));
        let metadata = contradictory
            .creature_corpse
            .as_mut()
            .expect("corpse metadata should exist");
        metadata.revivable = false;
        metadata.revive_special = true;
        assert!(!valid_item_snapshot(&contradictory));
        let mut zero_cost = corpse.clone();
        zero_cost
            .creature_corpse
            .as_mut()
            .expect("corpse metadata should exist")
            .prototype
            .attack_cost_moves = 0;
        assert!(!valid_item_snapshot(&zero_cost));
        let mut over_routed = corpse.clone();
        over_routed
            .creature_corpse
            .as_mut()
            .expect("corpse metadata should exist")
            .prototype
            .path_settings
            .max_distance = 401;
        assert!(!valid_item_snapshot(&over_routed));
        let mut blind = corpse;
        let prototype = &mut blind
            .creature_corpse
            .as_mut()
            .expect("corpse metadata should exist")
            .prototype;
        prototype.vision_day = 0;
        prototype.vision_night = 0;
        assert!(!valid_item_snapshot(&blind));
    }

    #[test]
    fn item_component_provenance_is_recursively_bounded() {
        let component = || ItemComponentSnapshotV1 {
            type_id: String::from("component"),
            charges: 1,
            damage: 0,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: String::new(),
            ranged_weapon: None,
            count_by_charges: false,
            recoverable: true,
            component_provenance: None,
            magazine_capacity: 0,
            magazine_well: None,
            residual_energy_millijoules: 0,
            powered_tool: None,
        };
        let mut deepest = component();
        for _ in 1..MAX_ITEM_COMPONENT_DEPTH {
            let mut parent = component();
            parent.component_provenance = Some(vec![deepest]);
            deepest = parent;
        }
        assert!(valid_item_component_root(&deepest));
        let mut too_deep = component();
        too_deep.component_provenance = Some(vec![deepest]);
        assert!(!valid_item_component_root(&too_deep));
        assert!(!valid_component_provenance(&Some(vec![
            component();
            MAX_ITEM_COMPONENTS
                + 1
        ])));
    }

    #[test]
    fn default_calendar_starts_on_spring_day_sixty_one_at_eight() {
        assert_eq!(
            CalendarSnapshot::at_tick(SimTick(0)),
            CalendarSnapshot {
                year: 1,
                season: Season::Spring,
                day_of_season: 61,
                hour: 8,
                minute: 0,
                second: 0,
            }
        );
        assert_eq!(
            CalendarSnapshot::at_tick(SimTick(SimTick::HZ * 31 * SECONDS_PER_DAY)),
            CalendarSnapshot {
                year: 1,
                season: Season::Summer,
                day_of_season: 1,
                hour: 8,
                minute: 0,
                second: 0,
            }
        );
    }

    #[test]
    fn calendar_advances_only_after_twenty_subsecond_ticks() {
        assert_eq!(
            CalendarSnapshot::at_tick(SimTick(SimTick::HZ - 1)).second,
            0
        );
        assert_eq!(CalendarSnapshot::at_tick(SimTick(SimTick::HZ)).second, 1);
    }

    #[test]
    fn pinned_boston_solar_boundaries_drive_light_phase() {
        let day = 61_u64;
        let [civil_dawn, sunrise, sunset, civil_dusk] =
            astronomy_table::SOLAR_BOUNDARIES_SECONDS[day as usize];
        let start_seconds = (u64::from(DEFAULT_START_DAY_OF_SPRING) - 1) * SECONDS_PER_DAY
            + u64::from(DEFAULT_START_HOUR) * 60 * 60;
        let at = |second_of_day: u32| {
            let absolute = day * SECONDS_PER_DAY + u64::from(second_of_day);
            NaturalLightSnapshot::at_tick(SimTick((absolute - start_seconds) * SimTick::HZ))
        };
        assert_eq!(at(civil_dawn - 1).phase, SkyPhase::Night);
        assert_eq!(at(civil_dawn).phase, SkyPhase::CivilTwilight);
        assert_eq!(at(sunrise).phase, SkyPhase::Day);
        assert_eq!(at(sunset).phase, SkyPhase::Day);
        assert_eq!(at(sunset + 1).phase, SkyPhase::CivilTwilight);
        assert_eq!(at(civil_dusk + 1).phase, SkyPhase::Night);
    }

    #[test]
    fn default_start_uses_new_moon_daylight_sight_cap() {
        assert_eq!(
            NaturalLightSnapshot::at_tick(SimTick(0)),
            NaturalLightSnapshot {
                phase: SkyPhase::Day,
                moon_phase: 0,
                sight_radius: 60,
            }
        );
    }
}
