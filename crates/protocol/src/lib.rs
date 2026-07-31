//! Stable domain identifiers and versioned network messages shared by every
//! runtime. This crate deliberately has no transport, persistence, or renderer
//! dependency.

use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod anatomy;
mod astronomy_table;
mod eocs;
mod interactions;
mod item_groups;
mod missions;
mod npc_dialogue;
mod npc_faction;
mod use_actions;
mod vehicles;

pub use anatomy::{
    ANATOMY_SCALE, ActorBodyPartSnapshotV1, ActorEffectLimbScoreModifierV1, ActorEffectModifiersV1,
    ActorEffectSnapshotV1, AnatomyDefinitionV1, ArmorMaterialProtectionV1, BodyPartHpModifiersV1,
    BodyPartOnHitEffectV1, BodyPartPrototypeV1, MAX_ANATOMY_PARTS, MAX_ARMOR_DAMAGE_TYPES,
    MAX_ARMOR_PORTIONS, MAX_BODY_PART_DEFERRED_FIELDS, MAX_BODY_PART_HIT_DIFFICULTY_MILLIONTHS,
    MAX_BODY_PART_HIT_SIZE_MILLIONTHS, MAX_BODY_PART_ID_BYTES, MAX_WEARABLE_ARMOR_TYPES,
    WearableArmorPortionV1, WearableArmorTypeV1, actor_body_part_summary_hp,
    actor_body_parts_are_valid, actor_effect_modifiers_are_valid, actor_effects_are_valid,
    anatomy_definition_is_valid, body_part_prototype_is_valid, wearable_armor_catalog_is_valid,
    wearable_armor_type_is_valid,
};
pub use eocs::{
    EocActorStatV1, EocActorValueV1, EocConditionV1, EocDefinitionV1, EocDelayV1, EocEffectV1,
    EocEventTriggerV1, EocItemUseTypeV1, EocMathAssignmentOperationV1, EocMathAssignmentTargetV1,
    EocMathExpressionV1, EocStringValueV1, MAX_ACTOR_SCHEDULED_EOCS, MAX_EOC_ACTOR_VARIABLES,
    MAX_EOC_DEFINITIONS, MAX_EOC_EFFECTS, MAX_EOC_ITEM_USE_TYPES, MAX_EOC_MATH_NODES,
    MAX_EOC_MESSAGE_BYTES, MAX_EOC_REFERENCES, MAX_EOC_SAFE_INTEGER, MAX_EOC_TREE_DEPTH,
    MAX_EOC_TREE_NODES, MAX_EOC_VARIABLE_VALUE_BYTES, ScheduledEocV1, actor_eoc_schedule_is_valid,
    actor_eoc_variables_are_valid, actor_inactive_recurring_eocs_are_valid,
    creature_eoc_condition_is_supported, creature_eoc_supported_ids,
    creature_spell_eoc_supported_ids, eoc_catalog_is_valid, eoc_condition_is_valid,
    eoc_condition_requires_target_context, eoc_confirmation_branches_are_valid,
    eoc_definition_requires_target_context, eoc_effect_referenced_ids, eoc_effects_are_valid,
    eoc_effects_contain_confirmation, eoc_effects_require_target_context,
};
pub use interactions::{
    InteractionCancellationReasonV1, InteractionChoiceV1, InteractionContextV1,
    MAX_INTERACTION_CHOICE_ID_BYTES, MAX_INTERACTION_CHOICE_LABEL_BYTES, MAX_INTERACTION_CHOICES,
    MAX_INTERACTION_LIFETIME_TICKS, MAX_INTERACTION_PROMPT_BYTES, PendingInteractionV1,
    pending_interaction_is_valid,
};
pub use missions::{
    MAX_ACTOR_MISSIONS, MAX_CREATURE_KILL_COUNT_TYPES, MAX_MISSION_DEFINITIONS,
    MAX_MISSION_TEXT_BYTES, MissionDefinitionV1, MissionGoalV1, MissionSnapshotV1, MissionStatusV1,
    actor_missions_are_valid, creature_kill_counts_are_valid, mission_catalog_is_valid,
    mission_definition_is_valid, mission_snapshot_is_valid,
    mission_snapshot_is_valid_for_definition,
};
pub use npc_dialogue::{
    DialogueResponseV1, DialogueTopicV1, MAX_DIALOGUE_ID_BYTES, MAX_DIALOGUE_RESPONSES,
    MAX_DIALOGUE_TEXT_BYTES, MAX_DIALOGUE_TOPIC_STACK, MAX_NPC_NAME_BYTES, MAX_NPC_OPINION_ABS,
    MAX_NPC_TEMPLATES, NpcOpinionV1, NpcSnapshotV1, NpcSocialStateV1, NpcTemplateV1,
    VisibleNpcSnapshotV1, npc_dialogue_catalog_is_valid, npc_snapshot_is_valid,
    npc_template_attitude_is_supported, npc_template_attitude_will_talk,
    opinion_delta_cannot_trigger_hostility, opinion_is_valid,
};
pub use npc_faction::{
    FactionFoodSupplyV1, FactionRelationFlagsV1, FactionRelationshipV1, FactionStateV1,
    FactionTemplateV1, MAX_FACTION_DESCRIPTION_BYTES, MAX_FACTION_FOOD_SUPPLY_ENTRIES,
    MAX_FACTION_ID_BYTES, MAX_FACTION_NAME_BYTES, MAX_FACTION_RELATIONS, MAX_FACTION_TEMPLATES,
    NO_FACTION_ID, PLAYER_FACTION_ID, faction_catalog_is_valid, faction_template_is_valid,
};

pub use item_groups::{
    ITEM_DEGRADATION_INCREMENTS_VARIABLE, ITEM_DEGRADATION_VARIABLE,
    ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE, ITEM_GROUP_CUSTOM_FLAG_MARKER_PREFIX,
    ITEM_GROUP_DRESSING_MARKER_PREFIX, ITEM_GROUP_GUN_FOULING_VARIABLE,
    ITEM_GUN_DIRT_FAULT_VARIABLE, ITEM_GUN_UNLUBRICATED_FAULT_VARIABLE,
    ITEM_POCKET_INSULATION_VARIABLE_PREFIX, ITEM_POCKET_VOLUME_MULTIPLIER_VARIABLE_PREFIX,
    ITEM_POCKET_WEIGHT_MULTIPLIER_VARIABLE_PREFIX, ITEM_ROT_SHELF_LIFE_TURNS_VARIABLE,
    ITEM_ROT_TURNS_VARIABLE, ITEM_STATIC_CORPSE_SHELF_LIFE_TURNS,
    ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN, ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS,
    ITEM_TEMPERATURE_UNPROCESSED_ENERGY_MJ_PER_G, ITEM_TEMPERATURE_UNPROCESSED_MILLIKELVIN,
    InclusiveI32RangeV1, InclusiveU16RangeV1, ItemDescriptionExpansionV1,
    ItemDescriptionSnippetCategoryV1, ItemDescriptionSnippetChoiceV1, ItemGroupChargeCapacityV1,
    ItemGroupChargeRangeV1, ItemGroupContainerV1, ItemGroupContentsSourceV1, ItemGroupDefinitionV1,
    ItemGroupDetachableStorageV1, ItemGroupEntryV1, ItemGroupEventV1, ItemGroupGraphV1,
    ItemGroupItemPrototypeV1, ItemGroupKindV1, ItemGroupNodeV1, ItemGroupOverflowV1,
    ItemGroupSourceV1, ItemGroupTargetV1, ItemGroupToolChargeStorageV1, ItemGroupVariantOptionV1,
    ItemSnippetV1, ItemTemperatureStateV1, ItemThermalPropertiesV1, ItemVariableValueV1,
    ItemVariantV1, MAX_DESCRIPTION_SNIPPET_CATEGORIES, MAX_DESCRIPTION_SNIPPET_CHOICES,
    MAX_DESCRIPTION_SNIPPET_DEPTH, MAX_EXPANDED_DESCRIPTION_BYTES,
    MAX_ITEM_GROUP_CUSTOM_FLAG_BYTES, MAX_ITEM_GROUP_CUSTOM_FLAGS, MAX_ITEM_GROUP_DEFINITIONS,
    MAX_ITEM_GROUP_DEPTH, MAX_ITEM_GROUP_ENTRIES, MAX_ITEM_GROUP_NODES, MAX_ITEM_GROUP_OUTPUTS,
    MAX_ITEM_SNIPPETS, MAX_ITEM_VARIABLES, MAX_ITEM_VARIANTS, SPAWN_POCKET_OPEN_CONTAINER_MARKER,
    SPAWN_POCKET_SINGLE_ITEM_MARKER, decode_item_group_custom_flag_marker,
    decode_item_group_dressing_marker, encode_item_group_custom_flag_marker,
    encode_item_group_dressing_marker, initial_item_temperature_state,
    is_reserved_item_group_custom_flag_marker, is_reserved_item_group_dressing_marker,
    is_reserved_item_group_internal_marker, is_reserved_spawn_pocket_marker,
    item_degradation_matches_damage, item_degradation_state, item_degradation_variables_are_valid,
    item_description_expansion_is_valid, item_group_catalog_is_valid,
    item_group_source_max_outputs, item_group_sources_are_valid, item_pocket_insulation,
    item_pocket_insulation_variable_key, item_pocket_insulation_variables_are_valid,
    item_pocket_multiplier_variables_are_valid, item_pocket_volume_multiplier,
    item_pocket_volume_multiplier_variable_key, item_pocket_weight_multiplier,
    item_pocket_weight_multiplier_variable_key, item_rot_state, item_rot_variables_are_valid,
    item_snippet_is_valid, item_temperature_state_matches_phase, item_variant_is_valid,
    spawn_pocket_content_weight_with_multiplier_milligrams,
    spawn_pocket_external_volume_milliliters,
    spawn_pocket_external_volume_with_multiplier_milliliters, spawn_pocket_has_item_restrictions,
    spawn_pocket_is_open_container, spawn_pocket_is_single_item, valid_item_variables,
};
use item_groups::{
    initial_item_fit_state, item_group_sources_have_exact_named_closure, valid_item_fit_state,
    valid_item_temperature_state,
};
pub use use_actions::{
    ItemTransformTypeV1, MAX_ITEM_TRANSFORM_MOVES, MAX_ITEM_TRANSFORM_TYPES,
    item_transform_catalog_is_valid,
};
pub use vehicles::{
    MAX_LIVE_VEHICLES, MAX_VEHICLE_CARGO_ITEMS_PER_PART, MAX_VEHICLE_CARGO_VOLUME_MILLILITERS,
    MAX_WORLDGEN_VEHICLE_GROUP_ENTRIES, MAX_WORLDGEN_VEHICLE_GROUP_ENTRIES_TOTAL,
    MAX_WORLDGEN_VEHICLE_GROUPS, MAX_WORLDGEN_VEHICLE_ITEM_SPAWNS,
    MAX_WORLDGEN_VEHICLE_ITEMS_PER_SPAWN, MAX_WORLDGEN_VEHICLE_PART_AMMO_TYPES,
    MAX_WORLDGEN_VEHICLE_PART_FLAGS, MAX_WORLDGEN_VEHICLE_PART_TOOLS,
    MAX_WORLDGEN_VEHICLE_PART_TYPES, MAX_WORLDGEN_VEHICLE_PART_VARIANTS,
    MAX_WORLDGEN_VEHICLE_PARTS_PER_PROTOTYPE, MAX_WORLDGEN_VEHICLE_PLACEMENTS,
    MAX_WORLDGEN_VEHICLE_PROTOTYPE_PARTS_TOTAL, MAX_WORLDGEN_VEHICLE_PROTOTYPES,
    MAX_WORLDGEN_VEHICLE_REPEAT, MAX_WORLDGEN_VEHICLE_ROTATIONS, MAX_WORLDGEN_VEHICLE_SYMBOL_BYTES,
    MAX_WORLDGEN_VEHICLE_TEXT_BYTES, VehiclePartSnapshotV1, VehicleSnapshotV1,
    VehicleSpawnStatusV1, VisibleVehicleSnapshotV1, VisibleVehicleTileV1,
    WorldgenVehicleDirectItemSpawnV1, WorldgenVehicleGroupEntryV1, WorldgenVehicleGroupV1,
    WorldgenVehicleItemSpawnV1, WorldgenVehiclePartTypeV1, WorldgenVehiclePartVariantV1,
    WorldgenVehiclePlacementV1, WorldgenVehiclePrototypePartV1, WorldgenVehiclePrototypeV1,
    insert_vehicle_stable_counters, vehicle_snapshots_are_valid,
    visible_vehicle_snapshots_are_valid, worldgen_vehicle_catalog_is_valid,
    worldgen_vehicle_placement_is_valid,
};

pub const PROTOCOL_VERSION: u16 = 132;
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
/// Canonical item condition uses pinned CDDA's display damage level, 0 through 5.
pub const MAX_ITEM_DAMAGE_LEVEL: u16 = 5;
/// Pinned `itype::damage_scale * 4` maximum for ordinary damageable items.
pub const MAX_ITEM_RAW_DAMAGE: u16 = 4_000;

#[must_use]
pub const fn item_damage_level(raw_damage: u16) -> u16 {
    if raw_damage == 0 {
        0
    } else {
        1 + (4 * raw_damage / MAX_ITEM_RAW_DAMAGE)
    }
}

#[must_use]
pub const fn minimum_raw_damage_for_level(level: u16) -> Option<u16> {
    match level {
        0 => Some(0),
        1 => Some(1),
        2 => Some(1_000),
        3 => Some(2_000),
        4 => Some(3_000),
        5 => Some(4_000),
        _ => None,
    }
}
pub const MAX_ACTOR_BASE_STAT: u16 = 100;
/// Pinned freeform character-creator bounds from `CHARACTER_STAT_MIN/MAX`.
pub const MIN_CHARACTER_CREATION_STAT: u16 = 4;
pub const MAX_CHARACTER_CREATION_STAT: u16 = 20;
pub const DEFAULT_CHARACTER_CREATION_STAT: u16 = 8;
pub const MAX_ITEM_COMPONENTS: usize = 256;
pub const MAX_ITEM_COMPONENT_DEPTH: usize = 8;
pub const MAX_MAGAZINE_COMPATIBLE_TYPES: usize = 256;
pub const MAX_ITEM_MAGAZINE_WELLS: usize = 16;
pub const MAX_ITEM_INTEGRAL_MAGAZINES: usize = 16;
pub const MAX_ITEM_AMMUNITION_CONTAINER_POCKETS: usize = 16;
pub const MAX_AMMUNITION_CONTAINER_TYPES: usize = 256;
pub const MAX_AMMUNITION_CONTAINER_CONTENTS: usize = 256;
pub const MILLIJOULES_PER_BATTERY_CHARGE: u32 = 1_000_000;
/// Canonical implementation version for deterministic local-map generation.
pub const WORLDGEN_GENERATOR_VERSION_V2: u16 = 2;
/// One overmap-terrain tile is exactly two 12x12 submaps on each axis.
pub const WORLDGEN_SUBMAPS_PER_OMT_AXIS: usize = 2;
pub const WORLDGEN_OMT_SIZE: usize = SUBMAP_SIZE as usize * WORLDGEN_SUBMAPS_PER_OMT_AXIS;
pub const WORLDGEN_CELLS_PER_OMT: usize = WORLDGEN_OMT_SIZE * WORLDGEN_OMT_SIZE;
pub const MAX_WORLDGEN_TERRAIN_PROTOTYPES: usize = 2_048;
pub const MAX_WORLDGEN_FURNITURE_PROTOTYPES: usize = 1_024;
pub const MAX_WORLDGEN_REGIONAL_TABLES: usize = 256;
pub const MAX_WORLDGEN_REGIONAL_CHOICES: usize = 256;
pub const MAX_WORLDGEN_REGIONAL_CHOICES_TOTAL: usize = 8_192;
pub const MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH: usize = 32;
pub const MAX_WORLDGEN_OMT_GENERATORS: usize = 512;
pub const MAX_WORLDGEN_TEMPLATES_PER_OMT: usize = 32;
pub const MAX_WORLDGEN_TEMPLATES: usize = 512;
pub const MAX_WORLDGEN_NESTED_GENERATORS_PER_OMT: usize = 4_096;
pub const MAX_WORLDGEN_NESTED_TEMPLATES_PER_GENERATOR: usize = 32;
pub const MAX_WORLDGEN_NESTED_TEMPLATES: usize = 16_384;
pub const MAX_WORLDGEN_NESTED_PLACEMENTS_PER_TEMPLATE: usize = 1_024;
pub const MAX_WORLDGEN_NESTED_PLACEMENTS: usize = 65_536;
pub const MAX_WORLDGEN_NESTED_DEPTH: usize = 32;
pub const MAX_WORLDGEN_DEFERRED_FIELDS: usize = 8;
pub const MAX_WORLDGEN_ITEM_PLACEMENT_REPEAT: u16 = 256;
pub const MAX_WORLDGEN_CELL_CHOICES: usize = 32;
pub const MAX_WORLDGEN_CELL_LAYERS: usize = 32;
pub const MAX_WORLDGEN_WEIGHTED_CELL_TARGETS: usize = 1_048_576;
pub const MAX_WORLDGEN_ID_BYTES: usize = 512;
pub const MAX_WORLDGEN_START_TARGETS: usize = 256;
pub const MAX_WORLDGEN_CITIES: usize = 4_096;
pub const MAX_WORLDGEN_CITY_SIZE: u8 = 55;
pub const MAX_WORLDGEN_RIVER_NODES: usize = 64;
pub const MAX_WORLDGEN_SPECIAL_PLACEMENTS: usize = 4_096;
pub const MAX_WORLDGEN_SPECIAL_OMTS: usize = 65_536;
pub const MAX_WORLDGEN_MONSTER_PROTOTYPES: usize = 16_384;
pub const MAX_WORLDGEN_MONSTER_GROUPS: usize = 16_384;
pub const MAX_WORLDGEN_MONSTER_GROUP_ENTRIES: usize = 65_536;
pub const MAX_WORLDGEN_MONSTER_GROUP_DEPTH: usize = 32;
pub const MAX_WORLDGEN_MONSTER_PLACEMENTS: usize = 65_536;
pub const MAX_WORLDGEN_MONSTER_REPEAT: u16 = 1_024;
pub const MAX_WORLDGEN_NPC_NAME_CATEGORIES: usize = 64;
pub const MAX_WORLDGEN_NPC_NAME_CHOICES: usize = 32_768;
pub const MAX_WORLDGEN_NPC_NAME_TEXT_BYTES: usize = 256;
pub const MAX_WORLDGEN_NPC_NAME_EXPANSION_DEPTH: usize = 16;
pub const MAX_WORLDGEN_MONSTER_PACK_SIZE: u16 = 1_024;
pub const MAX_WORLDGEN_MONSTER_DENSITY_MILLIONTHS: u32 = 81_920_000;
/// Pinned overmaps own 180x180 overmap-terrain coordinates per z-level.
pub const WORLDGEN_OVERMAP_WIDTH: u16 = 180;
pub const WORLDGEN_OVERMAP_HEIGHT: u16 = 180;
pub const MAX_WORLDGEN_OMT_IDENTITIES: usize = 512;
pub const MAX_WORLDGEN_OVERMAP_LAYERS: usize = 21;
pub const MAX_WORLDGEN_OVERMAP_RUNS: usize =
    WORLDGEN_OVERMAP_WIDTH as usize * WORLDGEN_OVERMAP_HEIGHT as usize;

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
stable_id!(NpcId);
stable_id!(ItemId);
stable_id!(InteractionId);
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
    /// Canonical zero-based index in the item's inherited `pocket_data` list.
    pub pocket_index: u16,
    /// Optional upstream pocket ID. Empty means the source pocket had no ID;
    /// `pocket_index` remains its stable identity in either case.
    pub pocket_id: String,
    /// Concrete compatible MAGAZINE item type IDs in stable order.
    pub compatible_magazine_type_ids: Vec<String>,
    /// Whether installed magazine volume is already included by the owner.
    pub rigid: bool,
    /// False when the owning item carries pinned `NO_UNLOAD`.
    pub unloadable: bool,
}

/// A canonical item-owned `MAGAZINE` pocket. The first runtime slice admits
/// one ammunition category per pocket; source order and identity remain
/// explicit so multiple integral magazines can be added without another
/// ownership migration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IntegralMagazinePocketPrototypeV1 {
    /// Canonical zero-based index in the item's inherited `pocket_data` list.
    pub pocket_index: u16,
    /// Optional upstream pocket ID. Empty means the source pocket had no ID.
    pub pocket_id: String,
    pub ammunition_type: String,
    pub capacity: u32,
    /// Whether contents occupy no additional external volume.
    pub rigid: bool,
    /// False when the owning item carries pinned `NO_RELOAD`.
    pub reloadable: bool,
    /// False when the owning item carries pinned `NO_UNLOAD`.
    pub unloadable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AmmunitionCapacityV1 {
    pub ammunition_type: String,
    pub capacity: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SpawnPocketKindV1 {
    Container,
    EFileStorage,
}

/// Immutable spawn-time containment rules retained in canonical prototypes.
/// Runtime reload-only ammunition pockets use `None` and continue to be
/// interpreted by their category capacities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpawnPocketRulesV1 {
    pub kind: SpawnPocketKindV1,
    pub max_contains_volume_milliliters: u64,
    /// Flexible-pocket content volume already included in the owner's base
    /// volume. Zero for rigid and E-file pockets.
    pub magazine_well_volume_milliliters: u64,
    /// Pinned `COLLAPSE_CONTENTS` constructor default for standard pockets.
    /// This is presentation state only and never changes insertion access.
    pub contents_collapsed_by_default: bool,
    pub max_contains_weight_milligrams: u64,
    pub max_item_volume_milliliters: u64,
    pub min_item_volume_milliliters: u64,
    pub max_item_length_millimeters: u64,
    pub item_restrictions: Vec<String>,
    pub flag_restrictions: Vec<String>,
    pub access_moves: u16,
    pub rigid: bool,
    pub watertight: bool,
    pub transparent: bool,
    pub forbidden: bool,
    pub sealable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpawnPocketStateV1 {
    pub rules: SpawnPocketRulesV1,
    /// Current inventory presentation state. Constructors start from the
    /// immutable rule default; homogeneous auto-wrapped contents also set it.
    pub contents_collapsed: bool,
    pub sealed: bool,
}

/// An item-owned `CONTAINER` pocket whose admitted contents are restricted by
/// upstream ammunition category and per-category capacity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AmmunitionContainerPocketPrototypeV1 {
    /// Canonical zero-based index in the item's inherited `pocket_data` list.
    pub pocket_index: u16,
    /// Optional upstream pocket ID. Empty means the source pocket had no ID.
    pub pocket_id: String,
    /// Stable ammunition-category-sorted capacity limits.
    pub capacities: Vec<AmmunitionCapacityV1>,
    pub rigid: bool,
    pub access_moves: u16,
    pub reloadable: bool,
    pub unloadable: bool,
    /// `Some` turns this existing stable pocket boundary into a generalized
    /// physical or E-file spawn pocket. `None` retains ammunition-container
    /// behavior for older canonical fixtures and reload gameplay.
    #[serde(default)]
    pub spawn_rules: Option<SpawnPocketRulesV1>,
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
    /// Canonical pocket index of the detachable magazine that supplies power.
    pub power_pocket_index: u16,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ItemPhaseV1 {
    #[default]
    Solid,
    Liquid,
    Gas,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemContainmentProfileV1 {
    /// Finalized type weight. Upstream multiplies this by live charges only
    /// for count-by-charges items.
    pub weight_milligrams: u64,
    /// Finalized type volume. Charge-scaled volume divides this value by
    /// `stack_size` with integer ceiling, matching pinned `item::volume`.
    pub volume_milliliters: u64,
    pub longest_side_millimeters: u64,
    pub flags: Vec<String>,
    pub estorable: bool,
    pub phase: ItemPhaseV1,
    pub count_by_charges: bool,
    pub stack_size: u32,
}

#[must_use]
pub fn item_containment_weight_milligrams(
    profile: &ItemContainmentProfileV1,
    charges: i32,
) -> Option<u64> {
    if profile
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("NO_DROP"))
        .is_ok()
    {
        return Some(0);
    }
    let weight = if profile.count_by_charges {
        profile
            .weight_milligrams
            .checked_mul(u64::try_from(charges).ok()?)
    } else {
        Some(profile.weight_milligrams)
    }?;
    if profile
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("REDUCED_WEIGHT"))
        .is_ok()
    {
        weight.checked_mul(3).map(|weight| weight / 4)
    } else {
        Some(weight)
    }
}

#[must_use]
pub fn item_containment_volume_milliliters(
    profile: &ItemContainmentProfileV1,
    charges: i32,
) -> Option<u64> {
    if !profile.count_by_charges && profile.phase != ItemPhaseV1::Liquid {
        return Some(profile.volume_milliliters);
    }
    let numerator = profile
        .volume_milliliters
        .checked_mul(u64::try_from(charges).ok()?)?;
    if profile.stack_size == 0 {
        return Some(numerator);
    }
    numerator
        .checked_add(u64::from(profile.stack_size) - 1)
        .map(|rounded| rounded / u64::from(profile.stack_size))
}

#[must_use]
pub fn item_containment_single_charge_volume_milliliters(
    profile: &ItemContainmentProfileV1,
) -> Option<u64> {
    item_containment_volume_milliliters(profile, 1)
}

#[must_use]
pub fn item_snapshot_has_no_contained_items(item: &ItemSnapshot) -> bool {
    item.integral_magazines
        .iter()
        .all(|pocket| pocket.loaded_ammunition.is_none())
        && item
            .magazine_wells
            .iter()
            .all(|pocket| pocket.installed_magazine.is_none())
        && item
            .ammunition_containers
            .iter()
            .all(|pocket| pocket.contents.is_empty())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemSoftnessProjection {
    Soft,
    Hard,
    Unknown,
}

fn item_softness_projection(profile: &ItemContainmentProfileV1) -> ItemSoftnessProjection {
    if profile
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("SOFT"))
        .is_ok()
    {
        ItemSoftnessProjection::Soft
    } else if profile
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("HARD"))
        .is_ok()
    {
        ItemSoftnessProjection::Hard
    } else {
        // Pinned `item::is_soft` falls back to material definitions. Material
        // softness is not canonical state yet, so compatibility below requires
        // both the soft and hard interpretations whenever neither override is
        // present.
        ItemSoftnessProjection::Unknown
    }
}

fn item_snapshot_standard_contents(item: &ItemSnapshot) -> Vec<&ItemSnapshot> {
    item.integral_magazines
        .iter()
        .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
        .chain(
            item.magazine_wells
                .iter()
                .filter_map(|pocket| pocket.installed_magazine.as_deref()),
        )
        .chain(item.ammunition_containers.iter().flat_map(|pocket| {
            let is_e_file = pocket
                .spawn_state
                .as_ref()
                .is_some_and(|state| state.rules.kind == SpawnPocketKindV1::EFileStorage);
            (!is_e_file)
                .then_some(pocket.contents.iter())
                .into_iter()
                .flatten()
        }))
        .collect()
}

/// Exact physical length projection for the represented canonical pocket
/// family. E-file children do not contribute to upstream item length.
#[must_use]
pub fn item_snapshot_containment_length_millimeters(item: &ItemSnapshot) -> Option<u64> {
    if item.containment.phase == ItemPhaseV1::Liquid
        || (item_softness_projection(&item.containment) == ItemSoftnessProjection::Soft
            && item_snapshot_has_no_contained_items(item))
    {
        return Some(0);
    }
    let own = if item_softness_projection(&item.containment) == ItemSoftnessProjection::Soft {
        0
    } else {
        item.containment.longest_side_millimeters
    };
    item_snapshot_standard_contents(item)
        .into_iter()
        .try_fold(own, |longest, child| {
            Some(longest.max(item_snapshot_containment_length_millimeters(child)?))
        })
}

fn item_snapshot_soft_volume_fits(item: &ItemSnapshot, maximum: u64) -> Option<bool> {
    item_snapshot_standard_contents(item)
        .into_iter()
        .try_fold(true, |fits, child| {
            Some(fits && item_snapshot_max_item_volume_fits(child, maximum)?)
        })
}

fn item_snapshot_max_item_volume_fits(item: &ItemSnapshot, maximum: u64) -> Option<bool> {
    if matches!(
        item.containment.phase,
        ItemPhaseV1::Liquid | ItemPhaseV1::Gas
    ) {
        return Some(true);
    }
    let hard_fits = if item.containment.count_by_charges {
        item_containment_single_charge_volume_milliliters(&item.containment)
    } else {
        item_snapshot_containment_volume_milliliters(item)
    }? <= maximum;
    let soft_fits = item_snapshot_soft_volume_fits(item, maximum)?;
    Some(match item_softness_projection(&item.containment) {
        ItemSoftnessProjection::Soft => soft_fits,
        ItemSoftnessProjection::Hard => hard_fits,
        ItemSoftnessProjection::Unknown => hard_fits && soft_fits,
    })
}

/// Pinned `item_pocket::is_compatible` projection for generalized spawn
/// pockets. Callers validate the snapshot structure separately.
#[must_use]
pub fn item_snapshot_is_compatible_with_spawn_rules(
    rules: &SpawnPocketRulesV1,
    content: &ItemSnapshot,
) -> bool {
    if rules.kind == SpawnPocketKindV1::EFileStorage {
        return content.containment.estorable;
    }
    let profile = &content.containment;
    let restricted =
        spawn_pocket_has_item_restrictions(rules) || !rules.flag_restrictions.is_empty();
    let accepted_restriction = rules.item_restrictions.iter().any(|restriction| {
        !is_reserved_spawn_pocket_marker(restriction) && restriction == &content.type_id
    }) || rules
        .flag_restrictions
        .iter()
        .any(|flag| profile.flags.binary_search(flag).is_ok());
    let compatibility_volume = if profile.count_by_charges {
        item_containment_single_charge_volume_milliliters(profile)
    } else {
        item_snapshot_containment_volume_milliliters(content)
    };
    profile.phase != ItemPhaseV1::Gas
        && profile
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("NO_UNWIELD"))
            .is_err()
        && (!profile.count_by_charges || item_snapshot_has_no_contained_items(content))
        && (profile.phase != ItemPhaseV1::Liquid || rules.watertight)
        && (!restricted || accepted_restriction)
        && compatibility_volume.is_some_and(|volume| volume >= rules.min_item_volume_milliliters)
        && item_snapshot_max_item_volume_fits(content, rules.max_item_volume_milliliters)
            == Some(true)
        && item_snapshot_containment_length_millimeters(content)
            .is_some_and(|length| length <= rules.max_item_length_millimeters)
}

/// Conservative, self-contained projection of pinned `item::can_combine` for
/// canonical containment. Every represented item property must match except
/// the stable identity and charge count, and neither item may own contents.
#[must_use]
pub fn item_snapshots_can_combine_for_containment(
    left: &ItemSnapshot,
    right: &ItemSnapshot,
) -> bool {
    if !left.containment.count_by_charges
        || !right.containment.count_by_charges
        || !item_snapshot_has_no_contained_items(left)
        || !item_snapshot_has_no_contained_items(right)
    {
        return false;
    }
    let mut left = left.clone();
    let mut right = right.clone();
    left.id = right.id;
    left.charges = 0;
    right.charges = 0;
    left == right
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CraftItemPrototypeV1 {
    pub type_id: String,
    pub charges: i32,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    /// Whether this strict nonperishable constructor owns active temperature
    /// state. Rot remains a separate fail-closed family.
    #[serde(default)]
    pub tracks_temperature: bool,
    /// Finalized material thermodynamics. `None` on a tracked item represents
    /// the characterized materialless constructor; it is also required when
    /// `tracks_temperature` is false.
    #[serde(default)]
    pub thermal_properties: Option<ItemThermalPropertiesV1>,
    pub ammunition_type: String,
    pub ranged_weapon: Option<RangedWeaponSnapshot>,
    #[serde(default)]
    pub magazine_capacity: u32,
    /// Ordered item-owned `MAGAZINE` pockets. Newly normalized content uses
    /// this item-backed form instead of `magazine_capacity` plus aggregate
    /// charges.
    #[serde(default)]
    pub integral_magazines: Vec<IntegralMagazinePocketPrototypeV1>,
    #[serde(default)]
    pub magazine_wells: Vec<MagazineWellPrototypeV1>,
    #[serde(default)]
    pub ammunition_containers: Vec<AmmunitionContainerPocketPrototypeV1>,
    #[serde(default)]
    pub residual_energy_millijoules: u32,
    #[serde(default)]
    pub powered_tool: Option<PoweredToolStateV1>,
    /// Self-contained physical identity used by authoritative pocket fit
    /// checks. Zero dimensions retain older fixtures that never participate in
    /// generalized containment.
    #[serde(default)]
    pub containment: ItemContainmentProfileV1,
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
    TalkToNpc {
        target: NpcId,
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
    TakeVehicleCargo {
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
    },
    StoreVehicleCargo {
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
    },
    ShootActor {
        target: ActorId,
    },
    ShootCreature {
        target: CreatureId,
    },
    Reload {
        ammunition_item: ItemId,
        /// `Some` selects an integral magazine or detachable magazine well by
        /// canonical pocket index.
        /// `None` selects the temporary built-in ranged-ammunition store.
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
    Wield {
        item_id: ItemId,
    },
    Unwield,
    Wear {
        item_id: ItemId,
    },
    TakeOff {
        item_id: ItemId,
    },
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
    RespondInteraction {
        interaction_id: InteractionId,
        choice_id: String,
    },
    CancelInteraction {
        interaction_id: InteractionId,
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
    ItemNotWearable,
    ItemAlreadyWorn,
    ItemNotWorn,
    ItemWorn,
    PocketMissing,
    InventoryFull,
    ItemNotConsumable,
    ItemNotActivatable,
    NoInteractionPending,
    StaleInteraction,
    InvalidInteractionChoice,
    NpcRefusedDialogue,
    VehicleMissing,
    VehiclePartMissing,
    VehiclePartBroken,
    VehiclePartNotBoardable,
    VehiclePartNotCargo,
    VehicleCargoLocked,
    VehiclePartOccupied,
    ActorAlreadyBoarded,
    ActorNotBoarded,
    InvalidUnboardDestination,
    ItemHasNoPower,
    PoweredToolActive,
    InvalidTerrainInteraction,
    InvalidBashInteraction,
    InvalidBashTool,
    ActionQueueFull,
    WeaponNotMelee,
    WeaponNotRanged,
    WeaponEmpty,
    NoClearShot,
    WeaponFull,
    IncompatibleAmmunition,
    PocketNotReloadable,
    PocketNotUnloadable,
    PocketItemMissing,
    PocketFull,
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
    VehicleSpawned {
        vehicle_id: VehicleId,
        prototype_id: String,
        position: WorldPosition,
        facing_degrees: i16,
    },
    ActorBoardedVehicle {
        actor_id: ActorId,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        position: WorldPosition,
    },
    ActorUnboardedVehicle {
        actor_id: ActorId,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        from: WorldPosition,
        to: WorldPosition,
    },
    VehicleCargoTaken {
        actor_id: ActorId,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
        position: WorldPosition,
    },
    VehicleCargoStored {
        actor_id: ActorId,
        vehicle_id: VehicleId,
        prototype_part_index: u16,
        item_id: ItemId,
        position: WorldPosition,
    },
    ActorMoved {
        actor_id: ActorId,
        from: WorldPosition,
        to: WorldPosition,
    },
    DamageApplied {
        source: ActorId,
        target: ActorId,
        body_part_id: String,
        amount: u16,
        remaining_part_hp: i32,
        remaining_hp: i32,
    },
    /// A fully resolved authoritative survivor-on-survivor melee miss.
    ActorMissedActor {
        source: ActorId,
        target: ActorId,
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
    MissionAssigned {
        actor_id: ActorId,
        mission_id: MissionId,
        mission_type_id: String,
    },
    MissionFinished {
        actor_id: ActorId,
        mission_id: MissionId,
        mission_type_id: String,
        success: bool,
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
    CreaturePolymorphed {
        creature_id: CreatureId,
        from_type_id: String,
        to_type_id: String,
        position: WorldPosition,
    },
    CreatureSummoned {
        caster: CreatureId,
        creature_id: CreatureId,
        monster_type_id: String,
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
        body_part_id: String,
        amount: u32,
        remaining_part_hp: i32,
        remaining_hp: i32,
    },
    CreatureMissedActor {
        source: CreatureId,
        target: ActorId,
        stumbled: bool,
        /// Sleeping-target misses are canonical for replay but intentionally
        /// remain private, matching the pinned upstream message boundary.
        target_was_sleeping: bool,
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
    ItemWorn {
        actor_id: ActorId,
        item_id: ItemId,
    },
    ItemTakenOff {
        actor_id: ActorId,
        item_id: ItemId,
    },
    ItemConsumed {
        actor_id: ActorId,
        item_id: ItemId,
        remaining_charges: i32,
        stored_kcal: i32,
        thirst: i32,
    },
    MedicalItemApplied {
        actor_id: ActorId,
        item_id: ItemId,
        body_part_id: String,
        healed_hp: i32,
        remaining_charges: i32,
    },
    EocMessage {
        actor_id: ActorId,
        text: String,
    },
    EocItemActivated {
        actor_id: ActorId,
        item_id: ItemId,
        remaining_charges: i32,
    },
    ItemTransformed {
        actor_id: ActorId,
        item_id: ItemId,
        from_type_id: String,
        to_type_id: String,
        remaining_charges: i32,
    },
    InteractionRequested {
        actor_id: ActorId,
        interaction: PendingInteractionV1,
    },
    InteractionCanceled {
        actor_id: ActorId,
        interaction_id: InteractionId,
        reason: InteractionCancellationReasonV1,
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
    ActorAffectedByField {
        actor_id: ActorId,
        field_type_id: String,
        effect_id: String,
        body_part_id: Option<String>,
        intensity: u32,
        duration_turns: u32,
        message: String,
        message_type: String,
    },
    ActorDamagedByEffect {
        actor_id: ActorId,
        effect_id: String,
        body_part_id: String,
        amount: u16,
        remaining_part_hp: i32,
        remaining_hp: i32,
    },
    ActorDiedFromEffect {
        actor_id: ActorId,
        effect_id: String,
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
    CreatureRangedAttackResolved {
        source: CreatureId,
        target: ActorId,
        origin: WorldPosition,
        gun_type_id: String,
        hit: bool,
        sound: String,
        sound_volume: u16,
    },
    CreatureTargetedActor {
        source: CreatureId,
        target: ActorId,
        origin: WorldPosition,
        sound: String,
        sound_volume: u16,
        laser_lock: bool,
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
        magazine_well_index: u16,
        magazine: ItemId,
        ejected_magazine: Option<ItemId>,
        charges: i32,
    },
    AmmunitionLoadedIntoPocket {
        actor_id: ActorId,
        item: ItemId,
        pocket_index: u16,
        source_ammunition: ItemId,
        /// Stable identity of the stack retained inside the pocket. This is
        /// the source identity on a full transfer into an empty pocket, an
        /// allocated split identity on a partial transfer into an empty
        /// pocket, or the existing nested identity when stacks combine.
        nested_ammunition: ItemId,
        loaded: u32,
        pocket_ammunition: u32,
        /// Fractional battery energy retained by the destination pocket after
        /// this transfer. This is zero for non-battery ammunition.
        pocket_residual_energy_millijoules: u32,
        source_charges_remaining: i32,
        /// Fractional battery energy retained by the loose source stack. A
        /// whole transfer moves this value into the destination pocket.
        source_residual_energy_millijoules_remaining: u32,
    },
    AmmunitionInsertedIntoContainer {
        actor_id: ActorId,
        owner_item: ItemId,
        pocket_index: u16,
        source_item: ItemId,
        contained_item: ItemId,
        ammunition_type: String,
        transferred: u32,
        pocket_ammunition: u32,
        source_charges_remaining: i32,
    },
    PocketItemRemoved {
        actor_id: ActorId,
        owner_item: ItemId,
        pocket_index: u16,
        contained_item: ItemId,
        charges: i32,
        residual_energy_millijoules: u32,
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
    /// Final inherited base morale restored by construction, polymorph, and revival.
    #[serde(default)]
    pub morale: i32,
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
    /// Exact upstream item damage. `damage` is the derived display level.
    #[serde(default)]
    pub raw_damage: u16,
    /// Per-instance upstream `FIT` state. A fitted item must have a finalized
    /// `VARSIZE` or `FIT` capability in its immutable containment flags.
    #[serde(default)]
    pub fitted: bool,
    /// Selected immutable appearance variant, if any. The state is
    /// self-contained so snapshot/replay recovery never consults live content.
    #[serde(default)]
    pub variant: Option<ItemVariantV1>,
    /// Selected inline snippet, retained independently of live content.
    #[serde(default)]
    pub snippet: Option<ItemSnippetV1>,
    /// Typed per-instance variables initialized from content and later owned
    /// by canonical simulation state.
    #[serde(default)]
    pub variables: BTreeMap<String, ItemVariableValueV1>,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    #[serde(default)]
    pub temperature: Option<ItemTemperatureStateV1>,
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
    /// Ordered item-owned `MAGAZINE` pockets and their stable nested ammo
    /// stacks. This is the canonical runtime representation for admitted
    /// magazines; `magazine_capacity` is retained only for older internal
    /// fixtures during the containment migration.
    #[serde(default)]
    pub integral_magazines: Vec<IntegralMagazinePocketSnapshotV1>,
    /// Ordered detachable-magazine boundaries. Installed contents retain their
    /// own stable item identities.
    #[serde(default)]
    pub magazine_wells: Vec<MagazineWellSnapshotV1>,
    /// Ordered ammunition-restricted `CONTAINER` pockets. Each nested content
    /// item keeps its stable identity and exact item state.
    #[serde(default)]
    pub ammunition_containers: Vec<AmmunitionContainerPocketSnapshotV1>,
    /// Sub-charge battery energy retained after continuous draw. This belongs
    /// either to an aggregate magazine or to a loose battery-ammunition item;
    /// integral storage retains it on the owning pocket instead. One integer
    /// battery charge is exactly one kilojoule.
    #[serde(default)]
    pub residual_energy_millijoules: u32,
    #[serde(default)]
    pub powered_tool: Option<PoweredToolStateV1>,
    /// `Some` turns the ordinary `corpse` item into a creature-specific corpse.
    #[serde(default)]
    pub creature_corpse: Option<CreatureCorpseSnapshotV1>,
    #[serde(default)]
    pub containment: ItemContainmentProfileV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MagazineWellSnapshotV1 {
    pub pocket_index: u16,
    pub pocket_id: String,
    pub compatible_magazine_type_ids: Vec<String>,
    pub rigid: bool,
    pub unloadable: bool,
    pub installed_magazine: Option<Box<ItemSnapshot>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IntegralMagazinePocketSnapshotV1 {
    pub pocket_index: u16,
    pub pocket_id: String,
    pub ammunition_type: String,
    pub capacity: u32,
    pub rigid: bool,
    pub reloadable: bool,
    pub unloadable: bool,
    pub loaded_ammunition: Option<Box<ItemSnapshot>>,
    /// Sub-charge energy retained after deterministic continuous battery draw.
    /// One whole battery charge remains exactly one kilojoule.
    #[serde(default)]
    pub residual_energy_millijoules: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AmmunitionContainerPocketSnapshotV1 {
    pub pocket_index: u16,
    pub pocket_id: String,
    pub capacities: Vec<AmmunitionCapacityV1>,
    pub rigid: bool,
    pub access_moves: u16,
    pub reloadable: bool,
    pub unloadable: bool,
    pub contents: Vec<ItemSnapshot>,
    #[serde(default)]
    pub spawn_state: Option<SpawnPocketStateV1>,
}

/// A component item retained inside a crafted result. It intentionally has no
/// world-stable ID while nested; recovery allocates a new stable ID before the
/// parent disassembly starts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemComponentSnapshotV1 {
    pub type_id: String,
    pub charges: i32,
    pub damage: u16,
    #[serde(default)]
    pub raw_damage: u16,
    #[serde(default)]
    pub fitted: bool,
    #[serde(default)]
    pub variant: Option<ItemVariantV1>,
    #[serde(default)]
    pub snippet: Option<ItemSnippetV1>,
    #[serde(default)]
    pub variables: BTreeMap<String, ItemVariableValueV1>,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    #[serde(default)]
    pub temperature: Option<ItemTemperatureStateV1>,
    pub ammunition_type: String,
    pub ranged_weapon: Option<RangedWeaponSnapshot>,
    pub count_by_charges: bool,
    pub recoverable: bool,
    pub component_provenance: Option<Vec<ItemComponentSnapshotV1>>,
    #[serde(default)]
    pub magazine_capacity: u32,
    /// Retains empty integral magazine pockets in crafted provenance. Loaded
    /// contents remain unsupported at this provenance boundary.
    #[serde(default)]
    pub integral_magazines: Vec<IntegralMagazinePocketPrototypeV1>,
    /// Retains an empty detachable-magazine well in crafted provenance.
    /// Crafting admission rejects installed contents until general nested
    /// component containment is implemented.
    #[serde(default)]
    pub magazine_wells: Vec<MagazineWellPrototypeV1>,
    #[serde(default)]
    pub ammunition_containers: Vec<AmmunitionContainerPocketPrototypeV1>,
    #[serde(default)]
    pub residual_energy_millijoules: u32,
    #[serde(default)]
    pub powered_tool: Option<PoweredToolStateV1>,
    #[serde(default)]
    pub containment: ItemContainmentProfileV1,
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
    /// Source-ordered human anatomy state. `hp` is the minimum current HP of
    /// vital parts and is retained as the compact public/death summary.
    pub body_parts: Vec<ActorBodyPartSnapshotV1>,
    /// Effect/body-part sorted canonical active effects.
    pub effects: Vec<ActorEffectSnapshotV1>,
    /// Stable actor-scoped CDDA dialogue/EOC string variables.
    pub eoc_variables: BTreeMap<String, String>,
    /// Monotonic high-water sequence for stable delayed-EOC ordering.
    pub next_eoc_schedule_sequence: u64,
    /// Due-tick/sequence sorted server-authoritative delayed activations.
    pub scheduled_eocs: Vec<ScheduledEocV1>,
    /// ID-sorted recurring EOCs paused by their deactivation conditions.
    pub inactive_recurring_eocs: Vec<String>,
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
    /// Inner-to-outer stable item order. Armor resolution walks this in
    /// reverse so the last worn layer is struck first.
    pub worn: Vec<ItemId>,
    pub stored_kcal: i32,
    pub thirst: i32,
    pub sleepiness: i32,
    pub sleeping: bool,
    pub sleep_intervals: u16,
    /// Current whole-point stamina. Combat resource changes are authoritative
    /// and persist across disconnect, recovery, and replay.
    pub stamina: u32,
    pub maximum_stamina: u32,
    /// Ordinary defensive reactions remaining before the next one-second
    /// actor turn refresh.
    pub dodge_attempts_remaining: u8,
    pub speed: u16,
    pub action_points: i64,
    pub queued_actions: Vec<QueuedActionSnapshot>,
    pub craft_activity: Option<CraftActivitySnapshotV1>,
    pub read_activity: Option<BookStudyActivitySnapshotV1>,
    pub disassembly_activity: Option<DisassemblyActivitySnapshotV1>,
    #[serde(default)]
    pub construction_activity: Option<ConstructionActivitySnapshotV1>,
    /// Server-owned prompt retained across snapshots, reconnects, and recovery.
    pub pending_interaction: Option<PendingInteractionV1>,
    /// Stable-ID-sorted canonical mission history, including active missions.
    pub missions: Vec<MissionSnapshotV1>,
    /// Per-character server-attributed lifetime kills keyed by concrete
    /// monster type. Sharing the upstream single-player tracker would let one
    /// multiplayer character advance another character's mission.
    pub creature_kill_counts: BTreeMap<String, u64>,
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
    /// Private authoritative live morale used by pinned attitude selection.
    #[serde(default)]
    pub morale: i32,
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
    /// ID-sorted private runtime state for immutable special-attack profiles.
    #[serde(default)]
    pub special_attacks: Vec<CreatureSpecialAttackStateV1>,
    /// Private authoritative monster-alpha EOC effects. Monster effects are
    /// whole-creature state and therefore never carry body-part IDs.
    #[serde(default)]
    pub effects: Vec<ActorEffectSnapshotV1>,
    /// Private bounded monster-alpha EOC variables.
    #[serde(default)]
    pub eoc_variables: BTreeMap<String, String>,
    /// Private authoritative concrete item-ID ammunition pools used by
    /// monster attack actors.
    #[serde(default)]
    pub ammunition: BTreeMap<String, u32>,
    /// Empty means this creature leaves no splatter on ordinary death.
    pub blood_field_type_id: String,
    /// `None` means this runtime creature has no modeled ordinary corpse.
    pub corpse: Option<CreatureCorpsePrototypeV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreatureSpecialAttackStateV1 {
    pub attack_id: String,
    pub cooldown_turns: u32,
    pub enabled: bool,
}

/// Public state for a currently visible creature. AI intent, action debt,
/// combat internals, blood, and corpse reconstruction data are omitted.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleCreatureSnapshot {
    pub id: CreatureId,
    pub type_id: String,
    pub position: WorldPosition,
    pub hp: i32,
    /// Current HP may exceed the new type's maximum after a pinned
    /// `poly_keep_hp` transformation.
    pub max_hp: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldIntensityLevelV1 {
    pub name: String,
    pub symbol: String,
    pub color: String,
    pub dangerous: bool,
    pub transparent: bool,
    /// Strict, source-ordered effects for this intensity. Empty when the
    /// source level has no effects or when `contact_effects_supported` is
    /// false.
    pub contact_effects: Vec<FieldContactEffectV1>,
    pub contact_effects_supported: bool,
}

/// A field effect applied once per simulation turn while a character occupies
/// the tile. Vehicle predicates are retained even before vehicle actors are
/// admitted so the contract does not silently lose source semantics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldContactEffectV1 {
    pub effect_id: String,
    pub minimum_duration_turns: u32,
    pub maximum_duration_turns: u32,
    /// Effect-type application defaults. The content compiler can replace
    /// these with source-specific values without another wire change.
    pub maximum_accumulated_duration_turns: u32,
    pub duration_add_percent: u16,
    pub intensity: u32,
    pub body_part_id: Option<String>,
    pub environmental: bool,
    pub immune_in_vehicle: bool,
    pub immune_inside_vehicle: bool,
    pub immune_outside_vehicle: bool,
    pub chance_in_vehicle: u32,
    pub chance_inside_vehicle: u32,
    pub chance_outside_vehicle: u32,
    pub message: String,
    pub message_npc: String,
    pub message_type: String,
    pub blocked_by_effect_ids: Vec<String>,
    pub modifiers: ActorEffectModifiersV1,
}

/// Data-normalized damage and status behavior applied to every matching body
/// part while a character occupies a field tile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldContactDamageV1 {
    pub body_part_type_id: String,
    pub damage_type_id: String,
    pub minimum_damage: u16,
    pub maximum_damage_base: u16,
    pub maximum_damage_per_intensity: u16,
    pub maximum_damage_divisor: u16,
    pub status_effect_id: String,
    pub status_intensity_base: u16,
    pub status_intensity_per_field_intensity: u16,
    pub status_duration_minimum_turns: u16,
    pub status_duration_maximum_base_turns: u16,
    pub status_duration_maximum_per_field_intensity: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldTypeSnapshotV1 {
    pub field_type_id: String,
    pub intensity_levels: Vec<FieldIntensityLevelV1>,
    pub priority: i32,
    pub half_life_seconds: u64,
    pub linear_half_life: bool,
    pub contact_damage: Option<FieldContactDamageV1>,
    pub is_splattering: bool,
    pub display_field: bool,
    pub decrease_intensity_on_contact: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldSnapshotV1 {
    pub field_type_id: String,
    pub intensity: u8,
    /// Pinned field age. Spell-created fields begin negative so their spell
    /// duration delays normal field decay.
    pub age_seconds: i64,
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

/// A weighted prototype reference used by a regional terrain table. A
/// prototype whose terrain ID names another regional table is a retained
/// pseudo-terrain edge and causes a fresh weighted roll. Choice order is
/// canonical because it determines deterministic ticket intervals.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenWeightedPrototypeV1 {
    pub prototype_index: u16,
    pub weight: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenFurniturePrototypeTargetV1 {
    None,
    Prototype(u16),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenWeightedFurniturePrototypeV1 {
    pub target: WorldgenFurniturePrototypeTargetV1,
    pub weight: u32,
}

/// One ID-sorted regional substitution table. Selecting a regional cell
/// target consumes a separate weighted roll against this table.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenRegionalTerrainTableV1 {
    pub regional_id: String,
    pub choices: Vec<WorldgenWeightedPrototypeV1>,
}

/// Furniture regional substitutions can explicitly produce no furniture.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenRegionalFurnitureTableV1 {
    pub regional_id: String,
    pub choices: Vec<WorldgenWeightedFurniturePrototypeV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenTerrainTargetV1 {
    Prototype(u16),
    Regional(u16),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenWeightedTerrainTargetV1 {
    pub target: WorldgenTerrainTargetV1,
    pub weight: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenFurnitureTargetV1 {
    None,
    Prototype(u16),
    Regional(u16),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenWeightedFurnitureTargetV1 {
    pub target: WorldgenFurnitureTargetV1,
    pub weight: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenItemGroupPlacementV1 {
    pub group_id: String,
    /// Independent pinned collection-style chance in 1..=100.
    pub chance: u8,
    /// Inclusive pinned repetition interval. Zero retains a deterministic
    /// no-placement branch without inventing an item-group roll.
    pub repeat_minimum: u16,
    pub repeat_maximum: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenCoordinateRangeV1 {
    pub minimum: i8,
    pub maximum: i8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenAreaItemPlacementV1 {
    pub item_group: WorldgenItemGroupPlacementV1,
    pub x: WorldgenCoordinateRangeV1,
    pub y: WorldgenCoordinateRangeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNpcPlacementV1 {
    pub template_id: String,
    /// Empty for a template with a unique name, one category for a fixed
    /// gender, or male/female categories in draw order for random gender.
    pub generated_name_category_ids: Vec<String>,
    /// One pinned repeat interval is sampled before applying the placement.
    pub repeat: WorldgenU16RangeV1,
    pub x: WorldgenCoordinateRangeV1,
    pub y: WorldgenCoordinateRangeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNpcNameChoiceV1 {
    pub text: String,
    pub weight: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNpcNameCategoryV1 {
    pub category_id: String,
    /// Pinned identified choices followed by anonymous choices, preserving
    /// the weighted snippet library's selection order.
    pub choices: Vec<WorldgenNpcNameChoiceV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNestedChoiceV1 {
    /// `null` is the pinned explicit no-op branch.
    pub nested_id: String,
    pub weight: u32,
}

/// One precompiled adjacent-OMT predicate. Keeping the exact allowed full IDs
/// in canonical world data makes runtime mapgen independent of live content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNeighborConditionV1 {
    pub offset_x: i8,
    pub offset_y: i8,
    /// Full overmap-terrain IDs, sorted and unique.
    pub allowed_identity_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNestedConditionsV1 {
    /// Every predicate must match.
    pub all_neighbors: Vec<WorldgenNeighborConditionV1>,
    /// Empty means no disjunction; otherwise at least one predicate must match.
    pub any_neighbors: Vec<WorldgenNeighborConditionV1>,
    /// Root predecessor generator IDs, sorted and unique.
    pub predecessor_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNestedPlacementV1 {
    pub chunks: Vec<WorldgenNestedChoiceV1>,
    pub else_chunks: Vec<WorldgenNestedChoiceV1>,
    pub x: WorldgenCoordinateRangeV1,
    pub y: WorldgenCoordinateRangeV1,
    pub conditions: WorldgenNestedConditionsV1,
}

/// One row-major local-map cell. A multi-entry target consumes one weighted
/// roll; a one-entry target is fixed. A selected regional target always
/// consumes one additional weighted-table roll, even with one candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenCellV1 {
    /// Ordered mapgen phases; every inner vector is one weighted choice.
    pub terrain: Vec<Vec<WorldgenWeightedTerrainTargetV1>>,
    pub furniture: Vec<Vec<WorldgenWeightedFurnitureTargetV1>>,
    pub item_group: Option<WorldgenItemGroupPlacementV1>,
}

/// Bounded pinned mapgen algorithms whose random choices cannot be expressed
/// as independent JSON cell layers. The discriminant is canonical world data;
/// unsupported upstream built-ins never enter a runtime catalog.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenBuiltinMapgenV1 {
    RiverStraight,
    RiverCurved { rotation: u8 },
    RiverCurvedNot { rotation: u8 },
    ForestWater,
}

impl WorldgenBuiltinMapgenV1 {
    #[must_use]
    pub const fn is_valid(self) -> bool {
        match self {
            Self::RiverStraight | Self::ForestWater => true,
            Self::RiverCurved { rotation } | Self::RiverCurvedNot { rotation } => rotation < 4,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenTemplateV1 {
    pub weight: u32,
    /// Optional ordinary mapgen run before this overlay.
    pub predecessor_id: Option<String>,
    /// Exact bounded algorithm used instead of independent cell layers.
    pub builtin: Option<WorldgenBuiltinMapgenV1>,
    /// Exactly 576 row-major cells: one 24x24 OMT or four 12x12 submaps.
    /// Empty target vectors are explicit overlay no-ops.
    pub cells: Vec<WorldgenCellV1>,
    pub nested: Vec<WorldgenNestedPlacementV1>,
    pub area_items: Vec<WorldgenAreaItemPlacementV1>,
    pub npc_placements: Vec<WorldgenNpcPlacementV1>,
    pub vehicle_placements: Vec<WorldgenVehiclePlacementV1>,
    pub monster_placements: Vec<WorldgenMonsterPlacementV1>,
    pub individual_monster_placements: Vec<WorldgenIndividualMonsterPlacementV1>,
    pub erase_all_before_placing_terrain: bool,
    /// Sorted semantic phases owned by later generalized families.
    pub deferred_fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNestedTemplateV1 {
    pub weight: u32,
    pub width: u8,
    pub height: u8,
    /// Exactly `width * height` row-major overlay cells.
    pub cells: Vec<WorldgenCellV1>,
    pub nested: Vec<WorldgenNestedPlacementV1>,
    pub area_items: Vec<WorldgenAreaItemPlacementV1>,
    pub npc_placements: Vec<WorldgenNpcPlacementV1>,
    pub vehicle_placements: Vec<WorldgenVehiclePlacementV1>,
    pub monster_placements: Vec<WorldgenMonsterPlacementV1>,
    pub individual_monster_placements: Vec<WorldgenIndividualMonsterPlacementV1>,
    pub erase_all_before_placing_terrain: bool,
    pub deferred_fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenNestedGeneratorV1 {
    pub nested_id: String,
    pub templates: Vec<WorldgenNestedTemplateV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenU16RangeV1 {
    pub minimum: u16,
    pub maximum: u16,
}

/// One immutable monster type admitted into deterministic generation. The
/// ordinary corpse prototype already contains every modeled base combat and
/// movement field, so generation reuses it as the live-creature prototype.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterPrototypeV1 {
    pub base: CreatureCorpsePrototypeV1,
    /// Whether the complete inherited behavior is admitted for ordinary
    /// runtime creation rather than retained only for fail-closed inspection.
    pub runtime_spawnable: bool,
    pub leaves_corpse: bool,
    /// Final inherited concrete item-ID ammunition assigned to each new
    /// creature of this type.
    pub starting_ammunition: BTreeMap<String, u32>,
    /// Final inherited flat resistances in thousandths of one damage point.
    pub armor_milli: BTreeMap<String, i32>,
    /// Armor penetration applied to the ordinary rolled bash dice, in
    /// thousandths of one damage point.
    pub melee_dice_armor_penetration_milli: i32,
    /// Unique typed components added to the rolled ordinary melee hit, in the
    /// pinned damage-instance order that controls armor and effect RNG draws.
    pub melee_damage: Vec<WorldgenMonsterMeleeDamageUnitV1>,
    /// Source-ordered effects applied only after an ordinary melee hit deals
    /// positive damage.
    pub attack_effects: Vec<WorldgenMonsterAttackEffectV1>,
    /// ID-sorted generic special-attack actor profiles attempted before
    /// ordinary attacks and movement.
    pub special_attacks: Vec<WorldgenMonsterSpecialAttackV1>,
    /// Sorted pinned MONSTER fields not yet consumed by the ordinary runtime
    /// creature model. They remain canonical instead of being silently lost.
    pub deferred_behavior_fields: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenMonsterSpecialAttackKindV1 {
    Melee,
    Bite,
    Leap,
    Eoc,
    Gun,
    Polymorph,
    Spell,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterGunRangeV1 {
    pub minimum: u32,
    pub maximum: u32,
    pub mode_id: String,
    pub shot_count: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterProjectileFieldEffectV1 {
    pub field_type_id: String,
    pub intensity_minimum: u8,
    pub intensity_maximum: u8,
    pub chance_percent: u8,
    pub radius: u8,
    pub check_passable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterProjectileOnHitEffectV1 {
    pub effect_id: String,
    pub duration_seconds: u64,
    pub intensity: u32,
    pub maximum_accumulated_duration_seconds: u32,
    pub duration_add_percent: u16,
    pub blocked_by_effect_ids: Vec<String>,
    pub modifiers: ActorEffectModifiersV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterProjectileEffectV1 {
    pub effect_id: String,
    pub trigger_chance_percent: u8,
    pub area_fields: Vec<WorldgenMonsterProjectileFieldEffectV1>,
    pub trail_fields: Vec<WorldgenMonsterProjectileFieldEffectV1>,
    pub on_hit_effects: Vec<WorldgenMonsterProjectileOnHitEffectV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterSpecialAttackV1 {
    pub attack_id: String,
    pub kind: WorldgenMonsterSpecialAttackKindV1,
    pub cooldown_turns: u32,
    pub move_cost_moves: u32,
    /// `None` uses the owning monster's ordinary melee skill.
    pub accuracy: Option<i32>,
    pub range: u32,
    pub no_adjacent: bool,
    pub dodgeable: bool,
    pub minimum_damage_multiplier_millionths: i32,
    pub maximum_damage_multiplier_millionths: i32,
    pub damage: Vec<WorldgenMonsterMeleeDamageUnitV1>,
    pub effects: Vec<WorldgenMonsterAttackEffectV1>,
    pub effects_require_damage: bool,
    pub attack_amount_minimum: u16,
    pub attack_amount_maximum: u16,
    pub spread_damage: bool,
    pub infection_chance_millionths: u32,
    /// Leap distances are thousandths of one map tile.
    pub leap_minimum_range_milli: u32,
    pub leap_maximum_range_milli: u32,
    pub leap_minimum_consider_range_milli: u32,
    pub leap_maximum_consider_range_milli: u32,
    pub leap_allow_no_target: bool,
    pub leap_prefer: bool,
    pub leap_random: bool,
    pub leap_ignore_destination_danger: bool,
    /// Monster-alpha condition. Runtime admission permits only semantics
    /// represented by canonical creature state.
    pub condition: Option<EocConditionV1>,
    /// Source-ordered immediate monster-alpha EOC activations.
    pub eoc_ids: Vec<String>,
    /// Concrete target prototype for a polymorph actor; empty for every other
    /// special kind.
    pub polymorph_monster_type_id: String,
    pub polymorph_keep_speed: bool,
    pub polymorph_keep_hp: bool,
    pub polymorph_keep_aggression: bool,
    /// Concrete hostile monster prototype emitted by a compiled permanent
    /// summon spell. Empty for every other special kind.
    pub spell_summoned_monster_type_id: String,
    /// Self-centered spells do not require an actor target or line of sight.
    pub spell_target_self: bool,
    /// Pinned `NO_PROJECTILE`: blast targeting does not move the epicenter to
    /// the first impassable tile on the projectile line.
    pub spell_no_projectile: bool,
    /// Pinned `IGNORE_WALLS`: blast propagation ignores impassable terrain.
    pub spell_ignore_walls: bool,
    pub spell_minimum_summons: u16,
    pub spell_maximum_summons: u16,
    pub spell_random_summons: bool,
    pub spell_aoe: u8,
    /// Optional field created on every valid tile in the spell area.
    pub spell_field_type_id: String,
    pub spell_field_chance: u32,
    pub spell_field_intensity: u8,
    pub spell_field_intensity_variance_millionths: u32,
    pub spell_field_duration_turns: u32,
    pub spell_targets_hostile: bool,
    pub spell_targets_ground: bool,
    pub spell_targets_self: bool,
    /// Empty outside strict content-derived gun actors.
    pub gun_type_id: String,
    /// Empty for ammo-free pseudo guns; otherwise the concrete item ID whose
    /// owning creature pool loses one charge per emitted projectile.
    pub gun_ammunition_type_id: String,
    /// Range-sorted engagement bands with their finalized firing mode and
    /// bounded projectile count. The first matching band wins.
    pub gun_ranges: Vec<WorldgenMonsterGunRangeV1>,
    pub gun_item_range: u32,
    /// Finalized fake-shooter dispersion in pinned engine dispersion units.
    pub gun_dispersion: u32,
    pub gun_sound_volume: u16,
    pub gun_targeting_cost_moves: u32,
    pub gun_require_targeting_player: bool,
    pub gun_targeting_timeout_turns: u32,
    pub gun_targeting_timeout_extend_turns: i32,
    pub gun_targeting_sound: String,
    pub gun_targeting_volume: u16,
    pub gun_laser_lock: bool,
    /// ID-sorted data-driven projectile behavior whose complete definitions
    /// are supported by the authoritative runtime.
    pub gun_projectile_effects: Vec<WorldgenMonsterProjectileEffectV1>,
    pub gun_no_damage_scaling: bool,
    pub gun_blinds_eyes: bool,
    /// Retained even before vehicles exist so later vehicle admission cannot
    /// silently change an already-canonical gun actor.
    pub gun_target_moving_vehicles: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterMeleeDamageUnitV1 {
    pub damage_type_id: String,
    pub amount_milli: i32,
    pub armor_penetration_milli: i32,
    pub armor_multiplier_millionths: i32,
    pub damage_multiplier_millionths: i32,
    pub constant_armor_multiplier_millionths: i32,
    pub constant_damage_multiplier_millionths: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterAttackEffectV1 {
    pub effect_id: String,
    pub chance_millionths: u32,
    pub permanent: bool,
    pub affect_hit_body_part: bool,
    /// Used by intrinsic venom effects, which require dealt cut or stab damage.
    pub requires_cut_or_stab_damage: bool,
    pub body_part_id: Option<String>,
    pub duration_minimum_turns: u32,
    pub duration_maximum_turns: u32,
    pub intensity_minimum: u32,
    pub intensity_maximum: u32,
    pub maximum_accumulated_duration_turns: u32,
    pub duration_add_percent: u16,
    /// IDs of active effects that block this application, sorted uniquely.
    pub blocked_by_effect_ids: Vec<String>,
    /// One entry for every source-rollable intensity, in ascending requested
    /// intensity order. Each entry retains the clamped effect intensity and
    /// its resolved modifiers.
    pub intensity_applications: Vec<WorldgenMonsterEffectIntensityApplicationV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterEffectIntensityApplicationV1 {
    pub intensity: u32,
    pub modifiers: ActorEffectModifiersV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenMonsterGroupTargetV1 {
    Monster { prototype_index: u16 },
    Group { group_index: u16 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterGroupEntryV1 {
    pub target: WorldgenMonsterGroupTargetV1,
    pub weight: u32,
    pub cost_multiplier: i32,
    pub pack_size: WorldgenU16RangeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterGroupV1 {
    pub group_id: String,
    pub default_prototype_index: Option<u16>,
    pub frequency_total: u32,
    pub is_animal: bool,
    pub is_safe: bool,
    pub entries: Vec<WorldgenMonsterGroupEntryV1>,
}

/// One mapgen monster-group placement. Chance is an upstream one-in divisor;
/// density is a pinned multiplier, and repeat is rolled once per application.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenMonsterPlacementV1 {
    pub group_index: u16,
    pub chance: WorldgenU16RangeV1,
    pub density_millionths: u32,
    pub repeat: WorldgenU16RangeV1,
    pub x: WorldgenCoordinateRangeV1,
    pub y: WorldgenCoordinateRangeV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenIndividualMonsterTargetV1 {
    Monster { prototype_index: u16 },
    Group { group_index: u16 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenIndividualMonsterPlacementV1 {
    pub target: WorldgenIndividualMonsterTargetV1,
    pub chance_percent: WorldgenU16RangeV1,
    pub pack_size: WorldgenU16RangeV1,
    pub repeat: WorldgenU16RangeV1,
    pub x: WorldgenCoordinateRangeV1,
    pub y: WorldgenCoordinateRangeV1,
}

/// One OMT-ID-sorted generator. Template order is source order because it is
/// observable through deterministic weighted selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenOmtGeneratorV1 {
    pub omt_id: String,
    pub templates: Vec<WorldgenTemplateV1>,
    /// ID-sorted exact closure reachable from this root generator.
    pub nested_generators: Vec<WorldgenNestedGeneratorV1>,
}

/// The three upstream identities used by `is_ot_match`, plus the normalized
/// local-map generator selected for this terrain. For an ordinary rotatable
/// `lmoe_north`, these are `lmoe_north`, `lmoe`, `lmoe`, and `lmoe`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenOmtIdentityV1 {
    pub full_id: String,
    pub type_id: String,
    pub subtype_id: String,
    pub generator_id: String,
    /// Pinned clockwise local-map quarter turns for this concrete OMT peer.
    pub rotation: u8,
}

/// One canonical row-major run in a coordinate-owned overmap layer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenOvermapRunV1 {
    pub identity_index: u16,
    pub length: u32,
}

/// One z-level of a pinned-size overmap. Runs expand to exactly 180x180 cells.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenOvermapLayerV1 {
    pub z: i32,
    pub runs: Vec<WorldgenOvermapRunV1>,
}

/// A bounded coordinate-owned overmap layout. Coordinates outside this region
/// fail closed until adjacent-overmap generation is implemented.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenOvermapLayoutV1 {
    pub origin_x: i32,
    pub origin_y: i32,
    /// Full-ID-sorted concrete identities referenced by compact run indices.
    pub identities: Vec<WorldgenOmtIdentityV1>,
    /// Strictly z-sorted layers, including z=0.
    pub layers: Vec<WorldgenOvermapLayerV1>,
}

/// Stable immutable identity for a city seed inside one worldgen catalog.
/// IDs are dense placement-order values beginning at one, independent of the
/// runtime object allocator and therefore reproducible from the world seed.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct WorldgenCityId(pub u32);

/// One authoritative city seed placed by the pinned overmap family. The road
/// family later expands from this center; city ownership and starts use the
/// retained center and size without rediscovering them from terrain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenCityV1 {
    pub city_id: WorldgenCityId,
    /// Absolute OMT coordinate in the catalog's coordinate-owned overmap.
    pub center: ChunkCoord,
    pub size: u8,
}

/// One retained river curve. Major nodes carry cross-overmap endpoint and
/// tangent continuity; bounded branch nodes make the generated topology fully
/// reproducible without consulting mutable neighboring worlds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenRiverNodeV1 {
    pub start: ChunkCoord,
    pub end: ChunkCoord,
    pub control_start: ChunkCoord,
    pub control_end: ChunkCoord,
    pub size: u32,
    pub major: bool,
}

/// Stable immutable identity for an overmap-special placement. IDs are dense
/// placement-order values so persistence and replay never infer ownership from
/// mutable terrain names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct WorldgenSpecialId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenSpecialUniquenessV1 {
    None,
    Overmap,
    Global,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenU32RangeV1 {
    pub minimum: u32,
    pub maximum: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenSpecialPopulationV1 {
    pub group_id: String,
    pub population: WorldgenU32RangeV1,
    pub radius: WorldgenU16RangeV1,
}

/// One atomically placed fixed overmap special. `terrain_omts` contains only
/// OMTs actually replaced by the special; predicate-only connection anchors
/// remain represented by the final overmap layout.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenSpecialPlacementV1 {
    pub placement_id: WorldgenSpecialId,
    pub special_id: String,
    pub origin: ChunkCoord,
    pub rotation: u8,
    pub uniqueness: WorldgenSpecialUniquenessV1,
    pub terrain_omts: Vec<ChunkCoord>,
    pub population: Option<WorldgenSpecialPopulationV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenI32IntervalV1 {
    pub minimum: i32,
    pub maximum: i32,
}

impl WorldgenI32IntervalV1 {
    #[must_use]
    pub const fn contains(self, value: i32) -> bool {
        value >= self.minimum && value <= self.maximum
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WorldgenOmtMatchTypeV1 {
    Exact,
    Type,
    Subtype,
    Prefix,
    Contains,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenStartTargetV1 {
    pub omt: String,
    pub match_type: WorldgenOmtMatchTypeV1,
}

/// One parameter-free, map-preparation-free starting location admitted by the
/// current runtime. Target order remains source order for point-origin starts;
/// city-origin starts retain upstream's city size and edge-distance filters.
/// Runtime admission requires a matching coordinate in the durable initial
/// bubble so character creation never generates terrain.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenStartLocationV1 {
    pub start_location_id: String,
    pub targets: Vec<WorldgenStartTargetV1>,
    pub city_sizes: WorldgenI32IntervalV1,
    pub city_distance: WorldgenI32IntervalV1,
}

impl WorldgenStartLocationV1 {
    #[must_use]
    pub const fn requires_city(&self) -> bool {
        self.city_sizes.minimum > 0 || self.city_distance.maximum < WORLDGEN_OVERMAP_WIDTH as i32
    }
}

/// Pinned `start_location::can_belong_to_city` distance projection. Upstream
/// first clamps radial distance to the city edge, then subtracts city size a
/// second time; negative inner-city values are therefore observable.
#[must_use]
pub fn worldgen_city_start_distance(city: &WorldgenCityV1, omt: ChunkCoord) -> i32 {
    let dx = i64::from(omt.x) - i64::from(city.center.x);
    let dy = i64::from(omt.y) - i64::from(city.center.y);
    let squared = u64::try_from(dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy)))
        .unwrap_or(u64::MAX);
    i32::try_from(integer_sqrt_u64(squared))
        .unwrap_or(i32::MAX)
        .saturating_sub(i32::from(city.size))
        .max(0)
        .saturating_sub(i32::from(city.size))
}

fn integer_sqrt_u64(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut lower = 1_u64;
    let mut upper = value.min(u64::from(u32::MAX)).saturating_add(1);
    while lower + 1 < upper {
        let middle = lower + (upper - lower) / 2;
        if middle <= value / middle {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    lower
}

/// Immutable deterministic generation definitions retained by one world.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorldgenCatalogV1 {
    pub generator_version: u16,
    /// Immutable coordinate-owned terrain selection retained with the world.
    pub overmap: WorldgenOvermapLayoutV1,
    /// Stable placement-order city seeds retained with their authoritative
    /// center and radius. Strictly sorted by `city_id`.
    pub cities: Vec<WorldgenCityV1>,
    /// Deterministic placement-order river curves retained for adjacent-map
    /// continuity and recovery.
    pub rivers: Vec<WorldgenRiverNodeV1>,
    /// Deterministic placement-order fixed specials and their stable OMT
    /// ownership. Strictly sorted by `placement_id`.
    pub specials: Vec<WorldgenSpecialPlacementV1>,
    /// Server-authoritative spawn selector for new characters.
    pub start_location: Option<WorldgenStartLocationV1>,
    /// Prototype-ID-sorted, unique catalogs referenced by compact indices.
    pub terrain_prototypes: Vec<TerrainTileSnapshot>,
    pub furniture_prototypes: Vec<FurnitureTileSnapshot>,
    /// Monster-type-ID and group-ID sorted immutable spawn catalogs.
    pub monster_prototypes: Vec<WorldgenMonsterPrototypeV1>,
    pub monster_groups: Vec<WorldgenMonsterGroupV1>,
    /// Part-type/prototype/group sorted immutable vehicle spawn catalogs.
    pub vehicle_part_types: Vec<WorldgenVehiclePartTypeV1>,
    pub vehicle_prototypes: Vec<WorldgenVehiclePrototypeV1>,
    pub vehicle_groups: Vec<WorldgenVehicleGroupV1>,
    /// Regional-table-ID-sorted, unique catalogs referenced by compact indices.
    pub regional_terrain: Vec<WorldgenRegionalTerrainTableV1>,
    pub regional_furniture: Vec<WorldgenRegionalFurnitureTableV1>,
    /// Reachable pinned full-name snippet graph used by mapgen NPCs.
    pub npc_name_categories: Vec<WorldgenNpcNameCategoryV1>,
    /// OMT-ID-sorted, unique generators.
    pub omt_generators: Vec<WorldgenOmtGeneratorV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BashFieldEffectV1 {
    pub field_type_id: String,
    pub intensity: u8,
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
    pub drop_source: Option<ItemGroupSourceV1>,
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
    pub drop_source: Option<ItemGroupSourceV1>,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HealingItemTypeV1 {
    pub item_type_id: String,
    pub move_cost_moves: u32,
    pub charges_per_use: u16,
    pub limb_power_milli: i32,
    pub head_power_milli: i32,
    pub torso_power_milli: i32,
    pub limb_scaling_milli: i32,
    pub head_scaling_milli: i32,
    pub torso_scaling_milli: i32,
    pub bandages_power_milli: i32,
    pub bandages_scaling_milli: i32,
    pub disinfectant_power_milli: i32,
    pub disinfectant_scaling_milli: i32,
    pub bleed: u16,
    pub bite_chance_millionths: u32,
    pub infect_chance_millionths: u32,
}

#[must_use]
pub fn healing_item_catalog_is_valid(catalog: &[HealingItemTypeV1]) -> bool {
    catalog.len() <= 65_536
        && catalog
            .windows(2)
            .all(|pair| pair[0].item_type_id < pair[1].item_type_id)
        && catalog.iter().all(|healing| {
            !healing.item_type_id.is_empty()
                && healing.item_type_id.len() <= 512
                && healing
                    .item_type_id
                    .chars()
                    .all(|character| !character.is_control())
                && healing.move_cost_moves > 0
                && healing.bite_chance_millionths <= 1_000_000
                && healing.infect_chance_millionths <= 1_000_000
                && [
                    healing.limb_power_milli,
                    healing.head_power_milli,
                    healing.torso_power_milli,
                    healing.limb_scaling_milli,
                    healing.head_scaling_milli,
                    healing.torso_scaling_milli,
                    healing.bandages_power_milli,
                    healing.bandages_scaling_milli,
                    healing.disinfectant_power_milli,
                    healing.disinfectant_scaling_milli,
                ]
                .into_iter()
                .all(|value| value >= 0)
                && (healing.limb_power_milli > 0
                    || healing.head_power_milli > 0
                    || healing.torso_power_milli > 0
                    || healing.bandages_power_milli > 0
                    || healing.disinfectant_power_milli > 0
                    || healing.bleed > 0
                    || healing.bite_chance_millionths > 0
                    || healing.infect_chance_millionths > 0)
        })
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
    /// Immutable anatomy used for new and recovered player characters.
    pub actor_anatomy: AnatomyDefinitionV1,
    pub wearable_armor_types: Vec<WearableArmorTypeV1>,
    /// Field-type-ID-sorted simulation definitions admitted from pinned data.
    pub field_types: Vec<FieldTypeSnapshotV1>,
    /// Group-ID-sorted transitive closure of normalized item groups referenced
    /// by canonical simulation definitions.
    pub item_groups: Vec<ItemGroupDefinitionV1>,
    /// Stable terrain/furniture-ID-sorted authoritative bash definitions.
    pub terrain_bash_types: Vec<TerrainBashTypeV1>,
    /// Furniture-ID-sorted set of every pinned furniture definition with an
    /// upstream bash body. Runtime-admitted definitions are a strict subset;
    /// an unsupported body still blocks a smash from reaching the terrain.
    pub furniture_bash_ids: Vec<String>,
    pub furniture_bash_types: Vec<FurnitureBashTypeV1>,
    /// Item-type-ID-sorted strict player-smashing profiles.
    pub smash_item_types: Vec<SmashItemTypeV1>,
    /// Item-type-ID-sorted strict authoritative medical-use profiles.
    pub healing_item_types: Vec<HealingItemTypeV1>,
    /// Immutable EOC interpreter programs and item activation profiles. Both
    /// catalogs are ID-sorted and retain only closed, supported semantics.
    pub eoc_definitions: Vec<EocDefinitionV1>,
    pub eoc_item_use_types: Vec<EocItemUseTypeV1>,
    /// Strict conversions with compatible storage/temperature layouts and a
    /// complete validated target static prototype.
    pub item_transform_types: Vec<ItemTransformTypeV1>,
    /// Immutable coordinate-owned layout and normalized mapgen definitions,
    /// plus the server-authoritative start selector.
    /// Generated four-submap cells live in `chunks`; the catalog is retained
    /// so recovery never rereads mutable external content.
    pub worldgen: Option<WorldgenCatalogV1>,
    /// Immutable admitted faction defaults and their mutable canonical state.
    pub faction_templates: Vec<FactionTemplateV1>,
    pub factions: Vec<FactionStateV1>,
    pub npc_templates: Vec<NpcTemplateV1>,
    pub dialogue_topics: Vec<DialogueTopicV1>,
    pub mission_definitions: Vec<MissionDefinitionV1>,
    pub actors: Vec<ActorSnapshot>,
    pub npcs: Vec<NpcSnapshotV1>,
    pub creatures: Vec<CreatureSnapshot>,
    /// Stable-ID-sorted canonical vehicles and exact ordered part state.
    pub vehicles: Vec<VehicleSnapshotV1>,
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
    pub npcs: Vec<VisibleNpcSnapshotV1>,
    /// Immutable definitions needed to render the controlled actor's missions.
    pub mission_definitions: Vec<MissionDefinitionV1>,
    pub creatures: Vec<VisibleCreatureSnapshot>,
    pub vehicles: Vec<VisibleVehicleSnapshotV1>,
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
        CommandKind::TalkToNpc { target } => {
            target.counter() > 0 && target.world_namespace() == command.actor_id.world_namespace()
        }
        CommandKind::BoardVehicle { vehicle_id, .. } => {
            vehicle_id.counter() > 0
                && vehicle_id.world_namespace() == command.actor_id.world_namespace()
        }
        CommandKind::UnboardVehicle {
            vehicle_id, dx, dy, ..
        } => {
            vehicle_id.counter() > 0
                && vehicle_id.world_namespace() == command.actor_id.world_namespace()
                && HorizontalDirection { dx: *dx, dy: *dy }.is_valid()
        }
        CommandKind::TakeVehicleCargo {
            vehicle_id,
            item_id,
            ..
        }
        | CommandKind::StoreVehicleCargo {
            vehicle_id,
            item_id,
            ..
        } => {
            vehicle_id.counter() > 0
                && vehicle_id.world_namespace() == command.actor_id.world_namespace()
                && item_id.counter() > 0
                && item_id.world_namespace() == command.actor_id.world_namespace()
        }
        CommandKind::RespondInteraction {
            interaction_id,
            choice_id,
        } => {
            interaction_id.counter() > 0
                && interaction_id.world_namespace() == command.actor_id.world_namespace()
                && !choice_id.is_empty()
                && choice_id.len() <= MAX_INTERACTION_CHOICE_ID_BYTES
                && !choice_id.chars().any(char::is_control)
        }
        CommandKind::CancelInteraction { interaction_id } => {
            interaction_id.counter() > 0
                && interaction_id.world_namespace() == command.actor_id.world_namespace()
        }
        CommandKind::RemovePocketItem {
            owner_item,
            contained_item,
            ..
        } => {
            owner_item.counter() > 0
                && owner_item.world_namespace() == command.actor_id.world_namespace()
                && contained_item.counter() > 0
                && contained_item.world_namespace() == command.actor_id.world_namespace()
                && owner_item != contained_item
        }
        CommandKind::InsertPocketItem {
            owner_item,
            source_item,
            ..
        } => {
            owner_item.counter() > 0
                && owner_item.world_namespace() == command.actor_id.world_namespace()
                && source_item.counter() > 0
                && source_item.world_namespace() == command.actor_id.world_namespace()
                && owner_item != source_item
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
            .magazine_wells
            .iter()
            .all(|well| well.installed_magazine.is_none())
        && activity
            .target_item
            .integral_magazines
            .iter()
            .all(|pocket| pocket.loaded_ammunition.is_none())
        && activity
            .target_item
            .ammunition_containers
            .iter()
            .all(|pocket| pocket.contents.is_empty())
        && activity.target_item.residual_energy_millijoules == 0
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
    if !item.tracks_temperature && item.thermal_properties.is_some() {
        return false;
    }
    let static_corpse = item.tracks_temperature
        && item
            .containment
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("CORPSE"))
            .is_ok();
    let mut variables = BTreeMap::new();
    if static_corpse {
        variables.insert(
            ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE.to_owned(),
            ItemVariableValueV1::String(String::from("prototype_corpse")),
        );
        variables.insert(
            ITEM_ROT_SHELF_LIFE_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(
                i64::try_from(ITEM_STATIC_CORPSE_SHELF_LIFE_TURNS).unwrap_or(i64::MAX),
            ),
        );
        variables.insert(
            ITEM_ROT_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(0),
        );
    }
    let raw_damage = if static_corpse {
        MAX_ITEM_RAW_DAMAGE
    } else {
        0
    };
    let snapshot = ItemSnapshot {
        id: ItemId::new(1, 1),
        type_id: item.type_id.clone(),
        charges: item.charges,
        damage: item_damage_level(raw_damage),
        raw_damage,
        fitted: initial_item_fit_state(&item.containment),
        variant: None,
        snippet: None,
        variables,
        melee_damage_milli: item.melee_damage_milli.clone(),
        calories: item.calories,
        quench: item.quench,
        comestible_type: item.comestible_type.clone(),
        temperature: item.tracks_temperature.then(|| {
            initial_item_temperature_state(
                SimTick(0),
                item.containment.phase,
                item.thermal_properties,
            )
        }),
        ammunition_type: item.ammunition_type.clone(),
        ranged_weapon: item.ranged_weapon.clone(),
        component_provenance: None,
        magazine_capacity: item.magazine_capacity,
        integral_magazines: item
            .integral_magazines
            .iter()
            .map(|pocket| IntegralMagazinePocketSnapshotV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                ammunition_type: pocket.ammunition_type.clone(),
                capacity: pocket.capacity,
                rigid: pocket.rigid,
                reloadable: pocket.reloadable,
                unloadable: pocket.unloadable,
                loaded_ammunition: None,
                residual_energy_millijoules: 0,
            })
            .collect(),
        magazine_wells: item
            .magazine_wells
            .iter()
            .map(|well| MagazineWellSnapshotV1 {
                pocket_index: well.pocket_index,
                pocket_id: well.pocket_id.clone(),
                compatible_magazine_type_ids: well.compatible_magazine_type_ids.clone(),
                rigid: well.rigid,
                unloadable: well.unloadable,
                installed_magazine: None,
            })
            .collect(),
        ammunition_containers: item
            .ammunition_containers
            .iter()
            .map(|pocket| AmmunitionContainerPocketSnapshotV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                capacities: pocket.capacities.clone(),
                rigid: pocket.rigid,
                access_moves: pocket.access_moves,
                reloadable: pocket.reloadable,
                unloadable: pocket.unloadable,
                spawn_state: pocket.spawn_rules.clone().map(|rules| SpawnPocketStateV1 {
                    contents_collapsed: rules.contents_collapsed_by_default,
                    rules,
                    sealed: false,
                }),
                contents: Vec::new(),
            })
            .collect(),
        residual_energy_millijoules: item.residual_energy_millijoules,
        powered_tool: item.powered_tool.clone(),
        creature_corpse: None,
        containment: item.containment.clone(),
    };
    valid_item_snapshot(&snapshot)
}

fn valid_worldgen_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_WORLDGEN_ID_BYTES
        && value.chars().all(|character| !character.is_control())
}

/// Pinned `is_ot_match` semantics over a normalized overmap-terrain identity.
#[must_use]
pub fn worldgen_omt_matches(
    target: &str,
    match_type: WorldgenOmtMatchTypeV1,
    identity: &WorldgenOmtIdentityV1,
) -> bool {
    match match_type {
        WorldgenOmtMatchTypeV1::Exact => target == identity.full_id,
        WorldgenOmtMatchTypeV1::Type => target == identity.type_id,
        WorldgenOmtMatchTypeV1::Subtype => target == identity.subtype_id,
        WorldgenOmtMatchTypeV1::Prefix => {
            identity.full_id.starts_with(target)
                && (identity.full_id.len() == target.len()
                    || identity.full_id.as_bytes().get(target.len()) == Some(&b'_'))
        }
        WorldgenOmtMatchTypeV1::Contains => identity.full_id.contains(target),
    }
}

fn valid_worldgen_omt_identity(identity: &WorldgenOmtIdentityV1) -> bool {
    valid_worldgen_id(&identity.full_id)
        && valid_worldgen_id(&identity.type_id)
        && valid_worldgen_id(&identity.subtype_id)
        && valid_worldgen_id(&identity.generator_id)
        && identity.rotation <= 3
}

fn valid_worldgen_start_location(
    start: &WorldgenStartLocationV1,
    used_surface_identities: &BTreeSet<u16>,
    identities: &[WorldgenOmtIdentityV1],
    cities: &[WorldgenCityV1],
) -> bool {
    valid_worldgen_id(&start.start_location_id)
        && !start.targets.is_empty()
        && start.targets.len() <= MAX_WORLDGEN_START_TARGETS
        && start.city_sizes.minimum <= start.city_sizes.maximum
        && start.city_distance.minimum <= start.city_distance.maximum
        && (!start.requires_city()
            || cities
                .iter()
                .any(|city| start.city_sizes.contains(i32::from(city.size))))
        && start
            .targets
            .iter()
            .all(|target| valid_worldgen_id(&target.omt))
        && start.targets.iter().all(|target| {
            used_surface_identities.iter().any(|index| {
                identities.get(usize::from(*index)).is_some_and(|identity| {
                    worldgen_omt_matches(target.omt.as_str(), target.match_type, identity)
                })
            })
        })
}

fn valid_worldgen_cities(layout: &WorldgenOvermapLayoutV1, cities: &[WorldgenCityV1]) -> bool {
    if cities.len() > MAX_WORLDGEN_CITIES {
        return false;
    }
    let Some(maximum_x) = layout
        .origin_x
        .checked_add(i32::from(WORLDGEN_OVERMAP_WIDTH) - 1)
    else {
        return false;
    };
    let Some(maximum_y) = layout
        .origin_y
        .checked_add(i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1)
    else {
        return false;
    };
    let mut centers = BTreeSet::new();
    cities.iter().enumerate().all(|(index, city)| {
        city.city_id.0 == u32::try_from(index + 1).unwrap_or(u32::MAX)
            && (2..=MAX_WORLDGEN_CITY_SIZE).contains(&city.size)
            && city.center.z == 0
            && (layout.origin_x..=maximum_x).contains(&city.center.x)
            && (layout.origin_y..=maximum_y).contains(&city.center.y)
            && centers.insert(city.center)
    })
}

fn valid_worldgen_rivers(layout: &WorldgenOvermapLayoutV1, rivers: &[WorldgenRiverNodeV1]) -> bool {
    if rivers.len() > MAX_WORLDGEN_RIVER_NODES {
        return false;
    }
    let Some(maximum_x) = layout
        .origin_x
        .checked_add(i32::from(WORLDGEN_OVERMAP_WIDTH) - 1)
    else {
        return false;
    };
    let Some(maximum_y) = layout
        .origin_y
        .checked_add(i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1)
    else {
        return false;
    };
    let inside = |point: ChunkCoord| {
        point.z == 0
            && (layout.origin_x..=maximum_x).contains(&point.x)
            && (layout.origin_y..=maximum_y).contains(&point.y)
    };
    let boundary = |point: ChunkCoord| {
        point.x == layout.origin_x
            || point.x == maximum_x
            || point.y == layout.origin_y
            || point.y == maximum_y
    };
    rivers.iter().all(|river| {
        river.size > 0
            && usize::try_from(river.size).is_ok_and(|size| {
                size <= usize::from(WORLDGEN_OVERMAP_WIDTH) * usize::from(WORLDGEN_OVERMAP_HEIGHT)
            })
            && inside(river.start)
            && inside(river.end)
            && inside(river.control_start)
            && inside(river.control_end)
            && (!river.major || boundary(river.start) && boundary(river.end))
    })
}

fn valid_worldgen_specials(
    layout: &WorldgenOvermapLayoutV1,
    specials: &[WorldgenSpecialPlacementV1],
) -> bool {
    if specials.len() > MAX_WORLDGEN_SPECIAL_PLACEMENTS {
        return false;
    }
    let Some(maximum_x) = layout
        .origin_x
        .checked_add(i32::from(WORLDGEN_OVERMAP_WIDTH) - 1)
    else {
        return false;
    };
    let Some(maximum_y) = layout
        .origin_y
        .checked_add(i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1)
    else {
        return false;
    };
    let z_levels = layout
        .layers
        .iter()
        .map(|layer| layer.z)
        .collect::<BTreeSet<_>>();
    let mut owned = BTreeSet::new();
    let mut globally_unique = BTreeSet::new();
    let mut total_omts = 0_usize;
    specials.iter().enumerate().all(|(index, special)| {
        let Some(next_total) = total_omts.checked_add(special.terrain_omts.len()) else {
            return false;
        };
        total_omts = next_total;
        special.placement_id.0 == u32::try_from(index + 1).unwrap_or(u32::MAX)
            && valid_worldgen_id(&special.special_id)
            && special.origin.z == 0
            && (layout.origin_x..=maximum_x).contains(&special.origin.x)
            && (layout.origin_y..=maximum_y).contains(&special.origin.y)
            && special.rotation < 4
            && !special.terrain_omts.is_empty()
            && special.population.as_ref().is_none_or(|population| {
                valid_worldgen_id(&population.group_id)
                    && population.population.minimum <= population.population.maximum
                    && population.population.maximum <= 1_000_000
                    && population.radius.minimum <= population.radius.maximum
                    && population.radius.maximum <= u16::from(WORLDGEN_OVERMAP_WIDTH)
            })
            && total_omts <= MAX_WORLDGEN_SPECIAL_OMTS
            && (special.uniqueness != WorldgenSpecialUniquenessV1::Global
                || globally_unique.insert(special.special_id.as_str()))
            && special.terrain_omts.iter().all(|omt| {
                (layout.origin_x..=maximum_x).contains(&omt.x)
                    && (layout.origin_y..=maximum_y).contains(&omt.y)
                    && z_levels.contains(&omt.z)
                    && owned.insert(*omt)
            })
    })
}

fn valid_worldgen_overmap_layout(layout: &WorldgenOvermapLayoutV1) -> Option<BTreeSet<u16>> {
    if layout.identities.is_empty()
        || layout.identities.len() > MAX_WORLDGEN_OMT_IDENTITIES
        || layout.layers.is_empty()
        || layout.layers.len() > MAX_WORLDGEN_OVERMAP_LAYERS
        || !layout.identities.iter().all(valid_worldgen_omt_identity)
        || !layout
            .identities
            .windows(2)
            .all(|pair| pair[0].full_id < pair[1].full_id)
        || !layout.layers.windows(2).all(|pair| pair[0].z < pair[1].z)
        || layout
            .origin_x
            .checked_add(i32::from(WORLDGEN_OVERMAP_WIDTH) - 1)
            .is_none()
        || layout
            .origin_y
            .checked_add(i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1)
            .is_none()
    {
        return None;
    }
    let expected_cells = u32::from(WORLDGEN_OVERMAP_WIDTH) * u32::from(WORLDGEN_OVERMAP_HEIGHT);
    let mut surface_identities = None;
    let mut all_used_identities = BTreeSet::new();
    for layer in &layout.layers {
        if layer.runs.is_empty() || layer.runs.len() > MAX_WORLDGEN_OVERMAP_RUNS {
            return None;
        }
        let mut used = BTreeSet::new();
        let mut total = 0_u32;
        let mut previous = None;
        for run in &layer.runs {
            if run.length == 0
                || usize::from(run.identity_index) >= layout.identities.len()
                || previous == Some(run.identity_index)
            {
                return None;
            }
            total = total.checked_add(run.length)?;
            used.insert(run.identity_index);
            all_used_identities.insert(run.identity_index);
            previous = Some(run.identity_index);
        }
        if total != expected_cells {
            return None;
        }
        if layer.z == 0 {
            surface_identities = Some(used);
        }
    }
    (all_used_identities.len() == layout.identities.len()).then_some(())?;
    surface_identities
}

fn worldgen_overmap_layer_and_cell(
    catalog: &WorldgenCatalogV1,
    omt: ChunkCoord,
) -> Option<(&WorldgenOvermapLayerV1, u32)> {
    let local_x = omt.x.checked_sub(catalog.overmap.origin_x)?;
    let local_y = omt.y.checked_sub(catalog.overmap.origin_y)?;
    if !(0..i32::from(WORLDGEN_OVERMAP_WIDTH)).contains(&local_x)
        || !(0..i32::from(WORLDGEN_OVERMAP_HEIGHT)).contains(&local_y)
    {
        return None;
    }
    let layer = catalog
        .overmap
        .layers
        .binary_search_by_key(&omt.z, |layer| layer.z)
        .ok()
        .and_then(|index| catalog.overmap.layers.get(index))?;
    let cell = u32::try_from(local_y)
        .ok()?
        .checked_mul(u32::from(WORLDGEN_OVERMAP_WIDTH))?
        .checked_add(u32::try_from(local_x).ok()?)?;
    Some((layer, cell))
}

/// Reports whether one coordinate belongs to a retained overmap layer without
/// scanning its RLE identity runs.
#[must_use]
pub fn worldgen_overmap_contains(catalog: &WorldgenCatalogV1, omt: ChunkCoord) -> bool {
    worldgen_overmap_layer_and_cell(catalog, omt).is_some()
}

/// Returns the immutable OMT identity owned by one canonical coordinate.
#[must_use]
pub fn worldgen_omt_identity_at(
    catalog: &WorldgenCatalogV1,
    omt: ChunkCoord,
) -> Option<&WorldgenOmtIdentityV1> {
    let (layer, cell) = worldgen_overmap_layer_and_cell(catalog, omt)?;
    let mut end = 0_u32;
    for run in &layer.runs {
        end = end.checked_add(run.length)?;
        if cell < end {
            return catalog
                .overmap
                .identities
                .get(usize::from(run.identity_index));
        }
    }
    None
}

fn checked_positive_weight_sum(weights: impl IntoIterator<Item = u32>) -> bool {
    weights
        .into_iter()
        .try_fold(0_u32, |total, weight| {
            (weight > 0).then_some(())?;
            total.checked_add(weight)
        })
        .is_some()
}

fn valid_worldgen_regional_terrain_table(
    table: &WorldgenRegionalTerrainTableV1,
    terrain_prototype_count: usize,
) -> bool {
    valid_worldgen_id(&table.regional_id)
        && !table.choices.is_empty()
        && table.choices.len() <= MAX_WORLDGEN_REGIONAL_CHOICES
        && checked_positive_weight_sum(table.choices.iter().map(|choice| choice.weight))
        && table
            .choices
            .iter()
            .all(|choice| usize::from(choice.prototype_index) < terrain_prototype_count)
}

fn valid_worldgen_regional_furniture_table(
    table: &WorldgenRegionalFurnitureTableV1,
    furniture_prototype_count: usize,
) -> bool {
    valid_worldgen_id(&table.regional_id)
        && !table.choices.is_empty()
        && table.choices.len() <= MAX_WORLDGEN_REGIONAL_CHOICES
        && checked_positive_weight_sum(table.choices.iter().map(|choice| choice.weight))
        && table.choices.iter().all(|choice| match choice.target {
            WorldgenFurniturePrototypeTargetV1::None => true,
            WorldgenFurniturePrototypeTargetV1::Prototype(index) => {
                usize::from(index) < furniture_prototype_count
            }
        })
}

fn valid_worldgen_regional_graph(edges: &[Vec<usize>]) -> bool {
    fn longest_path(
        index: usize,
        edges: &[Vec<usize>],
        visiting: &mut [bool],
        resolved: &mut [Option<usize>],
    ) -> Option<usize> {
        if let Some(depth) = *resolved.get(index)? {
            return Some(depth);
        }
        let active = visiting.get_mut(index)?;
        if *active {
            return None;
        }
        *active = true;
        let mut depth = 1_usize;
        for child in edges.get(index)? {
            depth = depth.max(longest_path(*child, edges, visiting, resolved)?.checked_add(1)?);
            if depth > MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH {
                return None;
            }
        }
        visiting[index] = false;
        resolved[index] = Some(depth);
        Some(depth)
    }

    let mut resolved = vec![None; edges.len()];
    (0..edges.len()).all(|index| {
        longest_path(index, edges, &mut vec![false; edges.len()], &mut resolved).is_some()
    })
}

fn valid_worldgen_regional_terrain_graph(catalog: &WorldgenCatalogV1) -> bool {
    let lookup = catalog
        .regional_terrain
        .iter()
        .enumerate()
        .map(|(index, table)| (table.regional_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let edges = catalog
        .regional_terrain
        .iter()
        .map(|table| {
            table
                .choices
                .iter()
                .filter_map(|choice| {
                    catalog
                        .terrain_prototypes
                        .get(usize::from(choice.prototype_index))
                        .and_then(|prototype| lookup.get(prototype.terrain_id.as_str()))
                        .copied()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    valid_worldgen_regional_graph(&edges)
}

fn valid_worldgen_regional_furniture_graph(catalog: &WorldgenCatalogV1) -> bool {
    let lookup = catalog
        .regional_furniture
        .iter()
        .enumerate()
        .map(|(index, table)| (table.regional_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let edges = catalog
        .regional_furniture
        .iter()
        .map(|table| {
            table
                .choices
                .iter()
                .filter_map(|choice| {
                    let WorldgenFurniturePrototypeTargetV1::Prototype(index) = choice.target else {
                        return None;
                    };
                    catalog
                        .furniture_prototypes
                        .get(usize::from(index))
                        .and_then(|prototype| lookup.get(prototype.furniture_id.as_str()))
                        .copied()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    valid_worldgen_regional_graph(&edges)
}

fn valid_worldgen_item_placement(placement: &WorldgenItemGroupPlacementV1) -> bool {
    valid_worldgen_id(&placement.group_id)
        && (1..=100).contains(&placement.chance)
        && placement.repeat_minimum <= placement.repeat_maximum
        && placement.repeat_maximum <= MAX_WORLDGEN_ITEM_PLACEMENT_REPEAT
}

fn valid_worldgen_coordinate_range(range: WorldgenCoordinateRangeV1) -> bool {
    range.minimum <= range.maximum
        && i16::from(range.minimum).unsigned_abs() < WORLDGEN_OMT_SIZE as u16
        && i16::from(range.maximum).unsigned_abs() < WORLDGEN_OMT_SIZE as u16
}

fn valid_worldgen_deferred_fields(fields: &[String]) -> bool {
    fields.len() <= MAX_WORLDGEN_DEFERRED_FIELDS
        && fields
            .windows(2)
            .all(|pair| pair[0].as_str() < pair[1].as_str())
        && fields.iter().all(|field| {
            matches!(
                field.as_str(),
                "forest_components" | "place_monsters" | "place_vehicles" | "signage_text"
            )
        })
}

fn valid_worldgen_cell_shape(cell: &WorldgenCellV1, catalog: &WorldgenCatalogV1) -> bool {
    cell.terrain.len() <= MAX_WORLDGEN_CELL_LAYERS
        && cell.furniture.len() <= MAX_WORLDGEN_CELL_LAYERS
        && cell.terrain.iter().all(|layer| {
            !layer.is_empty()
                && layer.len() <= MAX_WORLDGEN_CELL_CHOICES
                && checked_positive_weight_sum(layer.iter().map(|choice| choice.weight))
                && layer.iter().all(|choice| match choice.target {
                    WorldgenTerrainTargetV1::Prototype(index) => {
                        usize::from(index) < catalog.terrain_prototypes.len()
                    }
                    WorldgenTerrainTargetV1::Regional(index) => {
                        usize::from(index) < catalog.regional_terrain.len()
                    }
                })
        })
        && cell.furniture.iter().all(|layer| {
            !layer.is_empty()
                && layer.len() <= MAX_WORLDGEN_CELL_CHOICES
                && checked_positive_weight_sum(layer.iter().map(|choice| choice.weight))
                && layer.iter().all(|choice| match choice.target {
                    WorldgenFurnitureTargetV1::None => true,
                    WorldgenFurnitureTargetV1::Prototype(index) => {
                        usize::from(index) < catalog.furniture_prototypes.len()
                    }
                    WorldgenFurnitureTargetV1::Regional(index) => {
                        usize::from(index) < catalog.regional_furniture.len()
                    }
                })
        })
        && cell
            .item_group
            .as_ref()
            .is_none_or(valid_worldgen_item_placement)
}

fn valid_worldgen_builtin_mapgen(
    builtin: WorldgenBuiltinMapgenV1,
    catalog: &WorldgenCatalogV1,
) -> bool {
    let (terrain_ids, regional_id): (&[&str], Option<&str>) = match builtin {
        WorldgenBuiltinMapgenV1::ForestWater => (
            &["t_water_dp", "t_water_murky", "t_water_sh"],
            Some("t_region_groundcover_swamp"),
        ),
        _ => (
            &[
                "t_clay",
                "t_dirt",
                "t_grass",
                "t_sand",
                "t_water_moving_dp",
                "t_water_moving_sh",
            ],
            None,
        ),
    };
    builtin.is_valid()
        && terrain_ids.into_iter().all(|terrain_id| {
            catalog
                .terrain_prototypes
                .binary_search_by(|terrain| terrain.terrain_id.as_str().cmp(terrain_id))
                .is_ok()
        })
        && regional_id.is_none_or(|regional_id| {
            catalog
                .regional_terrain
                .binary_search_by(|table| table.regional_id.as_str().cmp(regional_id))
                .is_ok()
        })
}

fn valid_worldgen_area_item_placement(placement: &WorldgenAreaItemPlacementV1) -> bool {
    valid_worldgen_item_placement(&placement.item_group)
        && valid_worldgen_coordinate_range(placement.x)
        && valid_worldgen_coordinate_range(placement.y)
}

fn valid_worldgen_npc_name_category_id(id: &str) -> bool {
    id.len() >= 3
        && id.len() <= 128
        && id.starts_with('<')
        && id.ends_with('>')
        && id[1..id.len() - 1]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn worldgen_npc_name_references(text: &str) -> Option<Vec<&str>> {
    if text.is_empty()
        || text.len() > MAX_WORLDGEN_NPC_NAME_TEXT_BYTES
        || text.chars().any(char::is_control)
    {
        return None;
    }
    let mut references = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(['<', '>']) {
        if rest.as_bytes()[start] == b'>' {
            return None;
        }
        let suffix = &rest[start..];
        let end = suffix.find('>')?;
        let category = &suffix[..=end];
        if !valid_worldgen_npc_name_category_id(category) {
            return None;
        }
        references.push(category);
        rest = &suffix[end + 1..];
    }
    Some(references)
}

fn valid_worldgen_npc_name_catalog(categories: &[WorldgenNpcNameCategoryV1]) -> bool {
    if categories.is_empty() {
        return true;
    }
    if categories.len() > MAX_WORLDGEN_NPC_NAME_CATEGORIES
        || !categories
            .windows(2)
            .all(|pair| pair[0].category_id < pair[1].category_id)
    {
        return false;
    }
    let mut total_choices = 0_usize;
    let ids = categories
        .iter()
        .map(|category| category.category_id.as_str())
        .collect::<BTreeSet<_>>();
    if !ids.contains("<male_full_name>") || !ids.contains("<female_full_name>") {
        return false;
    }
    for category in categories {
        if !valid_worldgen_npc_name_category_id(&category.category_id)
            || category.choices.is_empty()
        {
            return false;
        }
        let Some(next_total) = total_choices.checked_add(category.choices.len()) else {
            return false;
        };
        total_choices = next_total;
        if total_choices > MAX_WORLDGEN_NPC_NAME_CHOICES
            || category
                .choices
                .iter()
                .try_fold(0_u64, |total, choice| {
                    (choice.weight > 0)
                        .then_some(())
                        .and_then(|()| total.checked_add(choice.weight))
                })
                .is_none()
            || category.choices.iter().any(|choice| {
                worldgen_npc_name_references(&choice.text)
                    .is_none_or(|references| references.iter().any(|id| !ids.contains(id)))
            })
        {
            return false;
        }
    }
    fn visit(
        id: &str,
        categories: &[WorldgenNpcNameCategoryV1],
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        depth: usize,
    ) -> bool {
        if depth > MAX_WORLDGEN_NPC_NAME_EXPANSION_DEPTH {
            return false;
        }
        if visited.contains(id) {
            return true;
        }
        if !visiting.insert(id.to_owned()) {
            return false;
        }
        let Some(category) = categories
            .binary_search_by(|candidate| candidate.category_id.as_str().cmp(id))
            .ok()
            .and_then(|index| categories.get(index))
        else {
            return false;
        };
        for reference in category.choices.iter().flat_map(|choice| {
            worldgen_npc_name_references(&choice.text)
                .unwrap_or_default()
                .into_iter()
        }) {
            if !visit(reference, categories, visiting, visited, depth + 1) {
                return false;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_owned());
        true
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    categories.iter().all(|category| {
        visit(
            &category.category_id,
            categories,
            &mut visiting,
            &mut visited,
            0,
        )
    })
}

fn valid_worldgen_npc_placement(
    placement: &WorldgenNpcPlacementV1,
    name_categories: &BTreeSet<&str>,
) -> bool {
    valid_worldgen_id(&placement.template_id)
        && placement.generated_name_category_ids.len() <= 2
        && placement
            .generated_name_category_ids
            .iter()
            .all(|id| name_categories.contains(id.as_str()))
        && valid_worldgen_u16_range(placement.repeat, 0, MAX_WORLDGEN_MONSTER_REPEAT)
        && valid_worldgen_coordinate_range(placement.x)
        && valid_worldgen_coordinate_range(placement.y)
}

fn valid_worldgen_u16_range(range: WorldgenU16RangeV1, minimum: u16, maximum: u16) -> bool {
    range.minimum >= minimum && range.minimum <= range.maximum && range.maximum <= maximum
}

fn valid_worldgen_monster_placement(
    placement: &WorldgenMonsterPlacementV1,
    group_count: usize,
) -> bool {
    usize::from(placement.group_index) < group_count
        && valid_worldgen_u16_range(placement.chance, 1, u16::MAX)
        && placement.density_millionths <= MAX_WORLDGEN_MONSTER_DENSITY_MILLIONTHS
        && valid_worldgen_u16_range(placement.repeat, 1, MAX_WORLDGEN_MONSTER_REPEAT)
        && valid_worldgen_coordinate_range(placement.x)
        && valid_worldgen_coordinate_range(placement.y)
}

fn valid_worldgen_individual_monster_placement(
    placement: &WorldgenIndividualMonsterPlacementV1,
    prototype_count: usize,
    group_count: usize,
) -> bool {
    let target_is_valid = match placement.target {
        WorldgenIndividualMonsterTargetV1::Monster { prototype_index } => {
            usize::from(prototype_index) < prototype_count
        }
        WorldgenIndividualMonsterTargetV1::Group { group_index } => {
            usize::from(group_index) < group_count
        }
    };
    target_is_valid
        && valid_worldgen_u16_range(placement.chance_percent, 1, 100)
        && valid_worldgen_u16_range(placement.pack_size, 1, MAX_WORLDGEN_MONSTER_PACK_SIZE)
        && valid_worldgen_u16_range(placement.repeat, 1, MAX_WORLDGEN_MONSTER_REPEAT)
        && valid_worldgen_coordinate_range(placement.x)
        && valid_worldgen_coordinate_range(placement.y)
}

fn valid_worldgen_monster_catalog(catalog: &WorldgenCatalogV1) -> bool {
    if catalog.monster_prototypes.len() > MAX_WORLDGEN_MONSTER_PROTOTYPES
        || catalog.monster_groups.len() > MAX_WORLDGEN_MONSTER_GROUPS
        || !catalog.monster_prototypes.iter().all(|prototype| {
            valid_creature_corpse_prototype(&prototype.base)
                && valid_worldgen_id(&prototype.base.monster_type_id)
                && prototype.starting_ammunition.len() <= 256
                && prototype
                    .starting_ammunition
                    .iter()
                    .all(|(item_id, amount)| valid_worldgen_id(item_id) && *amount <= 1_000_000_000)
                && prototype.armor_milli.len() <= 64
                && prototype
                    .armor_milli
                    .iter()
                    .all(|(damage_type, resistance)| {
                        valid_worldgen_id(damage_type) && resistance.unsigned_abs() <= 1_000_000_000
                    })
                && prototype.melee_dice_armor_penetration_milli.unsigned_abs() <= 1_000_000_000
                && valid_worldgen_monster_damage(&prototype.melee_damage)
                && valid_worldgen_monster_effects(&prototype.attack_effects)
                && prototype.special_attacks.len() <= 64
                && prototype
                    .special_attacks
                    .windows(2)
                    .all(|pair| pair[0].attack_id < pair[1].attack_id)
                && prototype.special_attacks.iter().all(|attack| {
                    valid_worldgen_id(&attack.attack_id)
                        && attack.cooldown_turns <= 1_000_000_000
                        && attack.move_cost_moves <= 1_000_000_000
                        && attack
                            .accuracy
                            .is_none_or(|accuracy| (0..=1_000_000).contains(&accuracy))
                        && (((matches!(
                            attack.kind,
                            WorldgenMonsterSpecialAttackKindV1::Polymorph
                        ) || (matches!(
                            attack.kind,
                            WorldgenMonsterSpecialAttackKindV1::Spell
                        ) && attack.spell_target_self))
                            && attack.range == 0)
                            || (1..=1_000_000).contains(&attack.range))
                        && (0..=1_000_000_000)
                            .contains(&attack.minimum_damage_multiplier_millionths)
                        && attack.minimum_damage_multiplier_millionths
                            <= attack.maximum_damage_multiplier_millionths
                        && attack.maximum_damage_multiplier_millionths <= 1_000_000_000
                        && valid_worldgen_monster_damage(&attack.damage)
                        && valid_worldgen_monster_effects(&attack.effects)
                        && attack.attack_amount_minimum > 0
                        && attack.attack_amount_minimum <= attack.attack_amount_maximum
                        && attack.attack_amount_maximum <= 64
                        && (matches!(
                            attack.kind,
                            WorldgenMonsterSpecialAttackKindV1::Melee
                                | WorldgenMonsterSpecialAttackKindV1::Bite
                        ) || (attack.attack_amount_minimum == 1
                            && attack.attack_amount_maximum == 1
                            && !attack.spread_damage))
                        && attack.condition.as_ref().is_none_or(eoc_condition_is_valid)
                        && attack.eoc_ids.len() <= MAX_EOC_REFERENCES
                        && attack
                            .eoc_ids
                            .iter()
                            .all(|eoc_id| valid_worldgen_id(eoc_id))
                        && (if matches!(attack.kind, WorldgenMonsterSpecialAttackKindV1::Polymorph)
                        {
                            valid_worldgen_id(&attack.polymorph_monster_type_id)
                        } else {
                            attack.polymorph_monster_type_id.is_empty()
                                && !attack.polymorph_keep_speed
                                && !attack.polymorph_keep_hp
                                && !attack.polymorph_keep_aggression
                        })
                        && (if matches!(attack.kind, WorldgenMonsterSpecialAttackKindV1::Spell) {
                            let common = attack.spell_aoe <= 32
                                && ((attack.spell_target_self && attack.range == 0)
                                    || (!attack.spell_target_self && attack.range > 0))
                                && (attack.spell_targets_hostile
                                    || attack.spell_targets_ground
                                    || attack.spell_targets_self);
                            let no_field = attack.spell_field_type_id.is_empty()
                                && attack.spell_field_chance == 0
                                && attack.spell_field_intensity == 0
                                && attack.spell_field_intensity_variance_millionths == 0
                                && attack.spell_field_duration_turns == 0;
                            let field = valid_worldgen_id(&attack.spell_field_type_id)
                                && (1..=1_000_000).contains(&attack.spell_field_chance)
                                && attack.spell_field_intensity > 0
                                && attack.spell_field_intensity_variance_millionths <= 1_000_000
                                && attack.spell_field_duration_turns <= 10_000_000;
                            let summon = valid_worldgen_id(&attack.spell_summoned_monster_type_id)
                                && (1..=64).contains(&attack.spell_minimum_summons)
                                && attack.spell_minimum_summons <= attack.spell_maximum_summons
                                && attack.spell_maximum_summons <= 64
                                && !attack.spell_random_summons
                                && attack.spell_minimum_summons == attack.spell_maximum_summons
                                && (1..=32).contains(&attack.spell_aoe)
                                && attack.minimum_damage_multiplier_millionths == 0
                                && attack.maximum_damage_multiplier_millionths == 0
                                && attack.damage.is_empty()
                                && attack.effects.is_empty()
                                && attack.eoc_ids.is_empty()
                                && attack.spell_targets_ground
                                && !attack.spell_targets_hostile
                                && no_field;
                            // Typed spell damage still lacks pinned defense,
                            // body-part selection, and RNG-order semantics.
                            let typed_damage = false;
                            let status_effect = attack.spell_summoned_monster_type_id.is_empty()
                                && !attack.spell_target_self
                                && attack.spell_minimum_summons == 0
                                && attack.spell_maximum_summons == 0
                                && !attack.spell_random_summons
                                && attack.minimum_damage_multiplier_millionths == 0
                                && attack.maximum_damage_multiplier_millionths == 0
                                && attack.damage.is_empty()
                                && attack.effects.len() == 1
                                && attack.effects[0].chance_millionths == 1_000_000
                                && !attack.effects[0].permanent
                                && !attack.effects[0].affect_hit_body_part
                                && !attack.effects[0].requires_cut_or_stab_damage
                                && attack.effects[0].body_part_id.is_none()
                                && attack.effects[0].duration_minimum_turns > 0
                                && attack.effects[0].duration_minimum_turns
                                    == attack.effects[0].duration_maximum_turns
                                && attack.effects[0].intensity_minimum == 1
                                && attack.effects[0].intensity_maximum == 1
                                && attack.eoc_ids.is_empty()
                                && attack.spell_aoe == 0
                                && attack.spell_targets_hostile
                                && !attack.spell_targets_ground
                                && !attack.spell_targets_self
                                && (no_field || field);
                            let eoc = attack.spell_summoned_monster_type_id.is_empty()
                                && !attack.spell_target_self
                                && !attack.spell_targets_ground
                                && attack.spell_minimum_summons == 0
                                && attack.spell_maximum_summons == 0
                                && !attack.spell_random_summons
                                && attack.minimum_damage_multiplier_millionths == 0
                                && attack.maximum_damage_multiplier_millionths == 0
                                && attack.damage.is_empty()
                                && attack.effects.is_empty()
                                && !attack.eoc_ids.is_empty()
                                && attack.spell_aoe == 0
                                && attack.spell_targets_hostile
                                && !attack.spell_targets_self
                                && no_field;
                            common && (summon || typed_damage || status_effect || eoc)
                        } else {
                            attack.spell_summoned_monster_type_id.is_empty()
                                && !attack.spell_target_self
                                && !attack.spell_no_projectile
                                && !attack.spell_ignore_walls
                                && attack.spell_minimum_summons == 0
                                && attack.spell_maximum_summons == 0
                                && !attack.spell_random_summons
                                && attack.spell_aoe == 0
                                && attack.spell_field_type_id.is_empty()
                                && attack.spell_field_chance == 0
                                && attack.spell_field_intensity == 0
                                && attack.spell_field_intensity_variance_millionths == 0
                                && attack.spell_field_duration_turns == 0
                                && !attack.spell_targets_hostile
                                && !attack.spell_targets_ground
                                && !attack.spell_targets_self
                        })
                        && attack.gun_ranges.len() <= 64
                        && attack.infection_chance_millionths <= 1_000_000
                        && (matches!(attack.kind, WorldgenMonsterSpecialAttackKindV1::Bite)
                            || attack.infection_chance_millionths == 0)
                        && match attack.kind {
                            WorldgenMonsterSpecialAttackKindV1::Leap => {
                                attack.gun_type_id.is_empty()
                                    && attack.gun_ammunition_type_id.is_empty()
                                    && attack.gun_ranges.is_empty()
                                    && attack.gun_item_range == 0
                                    && attack.gun_dispersion == 0
                                    && attack.gun_sound_volume == 0
                                    && attack.gun_targeting_cost_moves == 0
                                    && !attack.gun_require_targeting_player
                                    && attack.gun_targeting_timeout_turns == 0
                                    && attack.gun_targeting_timeout_extend_turns == 0
                                    && attack.gun_targeting_sound.is_empty()
                                    && attack.gun_targeting_volume == 0
                                    && !attack.gun_laser_lock
                                    && attack.gun_projectile_effects.is_empty()
                                    && !attack.gun_no_damage_scaling
                                    && !attack.gun_blinds_eyes
                                    && !attack.gun_target_moving_vehicles
                                    && attack.damage.is_empty()
                                    && attack.effects.is_empty()
                                    && attack.attack_amount_minimum == 1
                                    && attack.attack_amount_maximum == 1
                                    && !attack.spread_damage
                                    && attack.condition.is_none()
                                    && attack.eoc_ids.is_empty()
                                    && attack.leap_maximum_range_milli > 0
                                    && attack.leap_maximum_range_milli <= 100_000
                                    && attack.leap_minimum_range_milli
                                        <= attack.leap_maximum_range_milli
                                    && attack.leap_minimum_consider_range_milli
                                        <= attack.leap_maximum_consider_range_milli
                            }
                            WorldgenMonsterSpecialAttackKindV1::Melee
                            | WorldgenMonsterSpecialAttackKindV1::Bite => {
                                attack.gun_type_id.is_empty()
                                    && attack.gun_ammunition_type_id.is_empty()
                                    && attack.gun_ranges.is_empty()
                                    && attack.gun_item_range == 0
                                    && attack.gun_dispersion == 0
                                    && attack.gun_sound_volume == 0
                                    && attack.gun_targeting_cost_moves == 0
                                    && !attack.gun_require_targeting_player
                                    && attack.gun_targeting_timeout_turns == 0
                                    && attack.gun_targeting_timeout_extend_turns == 0
                                    && attack.gun_targeting_sound.is_empty()
                                    && attack.gun_targeting_volume == 0
                                    && !attack.gun_laser_lock
                                    && attack.gun_projectile_effects.is_empty()
                                    && !attack.gun_no_damage_scaling
                                    && !attack.gun_blinds_eyes
                                    && !attack.gun_target_moving_vehicles
                                    && attack.leap_minimum_range_milli == 0
                                    && attack.leap_maximum_range_milli == 0
                                    && attack.leap_minimum_consider_range_milli == 0
                                    && attack.leap_maximum_consider_range_milli == 0
                                    && !attack.leap_allow_no_target
                                    && !attack.leap_prefer
                                    && !attack.leap_random
                                    && !attack.leap_ignore_destination_danger
                                    && attack.kind == WorldgenMonsterSpecialAttackKindV1::Melee
                                    && attack.range == 1
                            }
                            WorldgenMonsterSpecialAttackKindV1::Eoc => {
                                attack.gun_type_id.is_empty()
                                    && attack.gun_ammunition_type_id.is_empty()
                                    && attack.gun_ranges.is_empty()
                                    && attack.gun_item_range == 0
                                    && attack.gun_dispersion == 0
                                    && attack.gun_sound_volume == 0
                                    && attack.gun_targeting_cost_moves == 0
                                    && !attack.gun_require_targeting_player
                                    && attack.gun_targeting_timeout_turns == 0
                                    && attack.gun_targeting_timeout_extend_turns == 0
                                    && attack.gun_targeting_sound.is_empty()
                                    && attack.gun_targeting_volume == 0
                                    && !attack.gun_laser_lock
                                    && attack.gun_projectile_effects.is_empty()
                                    && !attack.gun_no_damage_scaling
                                    && !attack.gun_blinds_eyes
                                    && !attack.gun_target_moving_vehicles
                                    && attack.move_cost_moves == 0
                                    && attack.accuracy.is_none()
                                    && !attack.no_adjacent
                                    && !attack.dodgeable
                                    && attack.minimum_damage_multiplier_millionths == 0
                                    && attack.maximum_damage_multiplier_millionths == 0
                                    && attack.damage.is_empty()
                                    && attack.effects.is_empty()
                                    && attack.attack_amount_minimum == 1
                                    && attack.attack_amount_maximum == 1
                                    && !attack.spread_damage
                                    && attack.infection_chance_millionths == 0
                                    && attack.leap_minimum_range_milli == 0
                                    && attack.leap_maximum_range_milli == 0
                                    && attack.leap_minimum_consider_range_milli == 0
                                    && attack.leap_maximum_consider_range_milli == 0
                                    && !attack.leap_allow_no_target
                                    && !attack.leap_prefer
                                    && !attack.leap_random
                                    && !attack.leap_ignore_destination_danger
                                    && !attack.eoc_ids.is_empty()
                            }
                            // The represented gun profile is retained for a
                            // future pinned ballistic kernel, but is fail-closed.
                            WorldgenMonsterSpecialAttackKindV1::Gun => false,
                            WorldgenMonsterSpecialAttackKindV1::Polymorph => {
                                attack.move_cost_moves == 0
                                    && attack.range == 0
                                    && attack.accuracy.is_none()
                                    && !attack.no_adjacent
                                    && !attack.dodgeable
                                    && attack.minimum_damage_multiplier_millionths == 0
                                    && attack.maximum_damage_multiplier_millionths == 0
                                    && attack.damage.is_empty()
                                    && attack.effects.is_empty()
                                    && !attack.effects_require_damage
                                    && attack.infection_chance_millionths == 0
                                    && attack.leap_minimum_range_milli == 0
                                    && attack.leap_maximum_range_milli == 0
                                    && attack.leap_minimum_consider_range_milli == 0
                                    && attack.leap_maximum_consider_range_milli == 0
                                    && !attack.leap_allow_no_target
                                    && !attack.leap_prefer
                                    && !attack.leap_random
                                    && !attack.leap_ignore_destination_danger
                                    && attack.condition.is_none()
                                    && attack.eoc_ids.is_empty()
                                    && attack.gun_type_id.is_empty()
                                    && attack.gun_ammunition_type_id.is_empty()
                                    && attack.gun_ranges.is_empty()
                                    && attack.gun_item_range == 0
                                    && attack.gun_dispersion == 0
                                    && attack.gun_sound_volume == 0
                                    && attack.gun_targeting_cost_moves == 0
                                    && !attack.gun_require_targeting_player
                                    && attack.gun_targeting_timeout_turns == 0
                                    && attack.gun_targeting_timeout_extend_turns == 0
                                    && attack.gun_targeting_sound.is_empty()
                                    && attack.gun_targeting_volume == 0
                                    && !attack.gun_laser_lock
                                    && attack.gun_projectile_effects.is_empty()
                                    && !attack.gun_no_damage_scaling
                                    && !attack.gun_blinds_eyes
                                    && !attack.gun_target_moving_vehicles
                            }
                            WorldgenMonsterSpecialAttackKindV1::Spell => {
                                attack.accuracy.is_none()
                                    && !attack.no_adjacent
                                    && !attack.dodgeable
                                    && !attack.effects_require_damage
                                    && attack.infection_chance_millionths == 0
                                    && attack.leap_minimum_range_milli == 0
                                    && attack.leap_maximum_range_milli == 0
                                    && attack.leap_minimum_consider_range_milli == 0
                                    && attack.leap_maximum_consider_range_milli == 0
                                    && !attack.leap_allow_no_target
                                    && !attack.leap_prefer
                                    && !attack.leap_random
                                    && !attack.leap_ignore_destination_danger
                                    && attack.polymorph_monster_type_id.is_empty()
                                    && !attack.polymorph_keep_speed
                                    && !attack.polymorph_keep_hp
                                    && !attack.polymorph_keep_aggression
                                    && attack.gun_type_id.is_empty()
                                    && attack.gun_ammunition_type_id.is_empty()
                                    && attack.gun_ranges.is_empty()
                                    && attack.gun_item_range == 0
                                    && attack.gun_dispersion == 0
                                    && attack.gun_sound_volume == 0
                                    && attack.gun_targeting_cost_moves == 0
                                    && !attack.gun_require_targeting_player
                                    && attack.gun_targeting_timeout_turns == 0
                                    && attack.gun_targeting_timeout_extend_turns == 0
                                    && attack.gun_targeting_sound.is_empty()
                                    && attack.gun_targeting_volume == 0
                                    && !attack.gun_laser_lock
                                    && attack.gun_projectile_effects.is_empty()
                                    && !attack.gun_no_damage_scaling
                                    && !attack.gun_blinds_eyes
                                    && !attack.gun_target_moving_vehicles
                            }
                        }
                })
                && prototype.runtime_spawnable == prototype.deferred_behavior_fields.is_empty()
                && prototype.deferred_behavior_fields.len() <= 1_024
                && prototype
                    .deferred_behavior_fields
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
                && prototype
                    .deferred_behavior_fields
                    .iter()
                    .all(|field| valid_worldgen_id(field))
        })
        || !catalog.monster_prototypes.windows(2).all(|pair| {
            pair[0].base.monster_type_id.as_str() < pair[1].base.monster_type_id.as_str()
        })
        || !catalog
            .monster_groups
            .windows(2)
            .all(|pair| pair[0].group_id < pair[1].group_id)
    {
        return false;
    }
    if catalog.specials.iter().any(|special| {
        special.population.as_ref().is_some_and(|population| {
            catalog
                .monster_groups
                .binary_search_by(|group| group.group_id.as_str().cmp(&population.group_id))
                .is_err()
        })
    }) {
        return false;
    }
    if catalog.monster_prototypes.iter().any(|prototype| {
        prototype.special_attacks.iter().any(|attack| {
            let dependency = match attack.kind {
                WorldgenMonsterSpecialAttackKindV1::Polymorph => {
                    Some(&attack.polymorph_monster_type_id)
                }
                WorldgenMonsterSpecialAttackKindV1::Spell => {
                    (!attack.spell_summoned_monster_type_id.is_empty())
                        .then_some(&attack.spell_summoned_monster_type_id)
                }
                WorldgenMonsterSpecialAttackKindV1::Melee
                | WorldgenMonsterSpecialAttackKindV1::Bite
                | WorldgenMonsterSpecialAttackKindV1::Leap
                | WorldgenMonsterSpecialAttackKindV1::Eoc
                | WorldgenMonsterSpecialAttackKindV1::Gun => None,
            };
            dependency.is_some_and(|dependency| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|candidate| {
                        candidate.base.monster_type_id.as_str().cmp(dependency)
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
                    .is_none_or(|dependency| {
                        prototype.runtime_spawnable && !dependency.runtime_spawnable
                    })
            })
        })
    }) {
        return false;
    }
    let mut entry_count = 0_usize;
    let mut edges = Vec::with_capacity(catalog.monster_groups.len());
    for group in &catalog.monster_groups {
        if !valid_worldgen_id(&group.group_id)
            || group.frequency_total == 0
            || group
                .default_prototype_index
                .is_some_and(|index| usize::from(index) >= catalog.monster_prototypes.len())
        {
            return false;
        }
        let Some(total) = entry_count.checked_add(group.entries.len()) else {
            return false;
        };
        entry_count = total;
        if entry_count > MAX_WORLDGEN_MONSTER_GROUP_ENTRIES {
            return false;
        }
        let mut group_edges = Vec::new();
        for entry in &group.entries {
            if entry.weight == 0
                || !valid_worldgen_u16_range(entry.pack_size, 1, MAX_WORLDGEN_MONSTER_PACK_SIZE)
            {
                return false;
            }
            match entry.target {
                WorldgenMonsterGroupTargetV1::Monster { prototype_index } => {
                    if usize::from(prototype_index) >= catalog.monster_prototypes.len() {
                        return false;
                    }
                }
                WorldgenMonsterGroupTargetV1::Group { group_index } => {
                    if usize::from(group_index) >= catalog.monster_groups.len() {
                        return false;
                    }
                    group_edges.push(usize::from(group_index));
                }
            }
        }
        edges.push(group_edges);
    }
    valid_worldgen_bounded_graph(&edges, MAX_WORLDGEN_MONSTER_GROUP_DEPTH)
}

fn valid_worldgen_monster_damage(units: &[WorldgenMonsterMeleeDamageUnitV1]) -> bool {
    units.len() <= 64
        && units
            .iter()
            .map(|unit| unit.damage_type_id.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == units.len()
        && units.iter().all(|unit| {
            valid_worldgen_id(&unit.damage_type_id)
                && unit.amount_milli.unsigned_abs() <= 1_000_000_000
                && unit.armor_penetration_milli.unsigned_abs() <= 1_000_000_000
                && unit.armor_multiplier_millionths.unsigned_abs() <= 1_000_000_000
                && unit.damage_multiplier_millionths > 0
                && unit.damage_multiplier_millionths <= 1_000_000_000
                && unit.constant_armor_multiplier_millionths.unsigned_abs() <= 1_000_000_000
                && unit.constant_damage_multiplier_millionths.unsigned_abs() <= 1_000_000_000
        })
}

fn valid_worldgen_monster_effects(effects: &[WorldgenMonsterAttackEffectV1]) -> bool {
    effects.len() <= 64
        && effects.iter().all(|effect| {
            valid_worldgen_id(&effect.effect_id)
                && effect.chance_millionths <= 1_000_000
                && effect
                    .body_part_id
                    .as_ref()
                    .is_none_or(|body_part_id| valid_worldgen_id(body_part_id))
                && effect.duration_minimum_turns <= effect.duration_maximum_turns
                && effect.duration_maximum_turns <= 1_000_000_000
                && effect.intensity_minimum > 0
                && effect.intensity_minimum <= effect.intensity_maximum
                && effect.intensity_maximum <= 1_000_000
                && effect.maximum_accumulated_duration_turns > 0
                && effect.duration_add_percent <= 1_000
                && effect.blocked_by_effect_ids.len() <= 64
                && effect
                    .blocked_by_effect_ids
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
                && effect
                    .blocked_by_effect_ids
                    .iter()
                    .all(|effect_id| valid_worldgen_id(effect_id))
                && usize::try_from(
                    effect
                        .intensity_maximum
                        .saturating_sub(effect.intensity_minimum)
                        .saturating_add(1),
                )
                .is_ok_and(|count| count <= 64 && effect.intensity_applications.len() == count)
                && effect.intensity_applications.iter().all(|application| {
                    application.intensity > 0
                        && application.intensity <= 1_000_000
                        && actor_effect_modifiers_are_valid(&application.modifiers)
                })
        })
}

fn valid_worldgen_neighbor_condition(
    condition: &WorldgenNeighborConditionV1,
    identity_ids: &BTreeSet<&str>,
) -> bool {
    (-1..=1).contains(&condition.offset_x)
        && (-1..=1).contains(&condition.offset_y)
        && (condition.offset_x != 0 || condition.offset_y != 0)
        && condition.allowed_identity_ids.len() <= MAX_WORLDGEN_OMT_IDENTITIES
        && condition
            .allowed_identity_ids
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && condition
            .allowed_identity_ids
            .iter()
            .all(|id| identity_ids.contains(id.as_str()))
}

fn valid_worldgen_nested_placement(
    placement: &WorldgenNestedPlacementV1,
    identity_ids: &BTreeSet<&str>,
) -> bool {
    let valid_choices = |choices: &[WorldgenNestedChoiceV1]| {
        choices.len() <= MAX_WORLDGEN_CELL_CHOICES
            && (choices.is_empty()
                || checked_positive_weight_sum(choices.iter().map(|choice| choice.weight)))
            && choices
                .iter()
                .all(|choice| valid_worldgen_id(&choice.nested_id))
    };
    (!placement.chunks.is_empty() || !placement.else_chunks.is_empty())
        && valid_choices(&placement.chunks)
        && valid_choices(&placement.else_chunks)
        && valid_worldgen_coordinate_range(placement.x)
        && valid_worldgen_coordinate_range(placement.y)
        && placement
            .conditions
            .all_neighbors
            .iter()
            .all(|condition| valid_worldgen_neighbor_condition(condition, identity_ids))
        && placement
            .conditions
            .any_neighbors
            .iter()
            .all(|condition| valid_worldgen_neighbor_condition(condition, identity_ids))
        && placement
            .conditions
            .predecessor_ids
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && placement
            .conditions
            .predecessor_ids
            .iter()
            .all(|id| valid_worldgen_id(id))
}

fn valid_worldgen_bounded_graph(edges: &[Vec<usize>], maximum_depth: usize) -> bool {
    fn visit(
        index: usize,
        edges: &[Vec<usize>],
        maximum_depth: usize,
        visiting: &mut [bool],
        resolved: &mut [Option<usize>],
    ) -> Option<usize> {
        if let Some(depth) = *resolved.get(index)? {
            return Some(depth);
        }
        let active = visiting.get_mut(index)?;
        if *active {
            return None;
        }
        *active = true;
        let mut depth = 1_usize;
        for child in edges.get(index)? {
            depth =
                depth.max(visit(*child, edges, maximum_depth, visiting, resolved)?.checked_add(1)?);
            if depth > maximum_depth {
                return None;
            }
        }
        visiting[index] = false;
        resolved[index] = Some(depth);
        Some(depth)
    }

    let mut resolved = vec![None; edges.len()];
    (0..edges.len()).all(|index| {
        visit(
            index,
            edges,
            maximum_depth,
            &mut vec![false; edges.len()],
            &mut resolved,
        )
        .is_some()
    })
}

fn valid_worldgen_predecessor_graph(catalog: &WorldgenCatalogV1) -> bool {
    let lookup = catalog
        .omt_generators
        .iter()
        .enumerate()
        .map(|(index, generator)| (generator.omt_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let identity_ids = catalog
        .overmap
        .identities
        .iter()
        .map(|identity| identity.full_id.as_str())
        .collect::<BTreeSet<_>>();
    let Some(edges) = catalog
        .omt_generators
        .iter()
        .map(|generator| {
            generator
                .templates
                .iter()
                .filter_map(|template| template.predecessor_id.as_deref())
                .map(|id| {
                    lookup
                        .get(id)
                        .copied()
                        .filter(|_| identity_ids.contains(id))
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    valid_worldgen_bounded_graph(&edges, MAX_WORLDGEN_NESTED_DEPTH)
}

fn valid_worldgen_nested_graph(generator: &WorldgenOmtGeneratorV1) -> bool {
    let lookup = generator
        .nested_generators
        .iter()
        .enumerate()
        .map(|(index, nested)| (nested.nested_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let Some(edges) = generator
        .nested_generators
        .iter()
        .map(|nested| {
            nested
                .templates
                .iter()
                .flat_map(|template| &template.nested)
                .flat_map(|placement| placement.chunks.iter().chain(&placement.else_chunks))
                .filter(|choice| choice.nested_id != "null")
                .map(|choice| lookup.get(choice.nested_id.as_str()).copied())
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    valid_worldgen_bounded_graph(&edges, MAX_WORLDGEN_NESTED_DEPTH)
}

/// Validates all local bounds and indices without requiring an item-group
/// catalog. This is useful while content is being compiled in dependency order.
/// Runtime admission should call [`worldgen_catalog_is_valid`] instead.
#[must_use]
pub fn worldgen_catalog_shape_is_valid(catalog: &WorldgenCatalogV1) -> bool {
    let Some(used_surface_identities) = valid_worldgen_overmap_layout(&catalog.overmap) else {
        return false;
    };
    if catalog.generator_version != WORLDGEN_GENERATOR_VERSION_V2
        || !valid_worldgen_cities(&catalog.overmap, &catalog.cities)
        || !valid_worldgen_rivers(&catalog.overmap, &catalog.rivers)
        || !valid_worldgen_specials(&catalog.overmap, &catalog.specials)
        || catalog.start_location.as_ref().is_some_and(|start| {
            !valid_worldgen_start_location(
                start,
                &used_surface_identities,
                &catalog.overmap.identities,
                &catalog.cities,
            )
        })
        || catalog.terrain_prototypes.is_empty()
        || catalog.terrain_prototypes.len() > MAX_WORLDGEN_TERRAIN_PROTOTYPES
        || catalog.furniture_prototypes.len() > MAX_WORLDGEN_FURNITURE_PROTOTYPES
        || !valid_worldgen_monster_catalog(catalog)
        || !worldgen_vehicle_catalog_is_valid(
            &catalog.vehicle_part_types,
            &catalog.vehicle_prototypes,
            &catalog.vehicle_groups,
        )
        || !valid_worldgen_npc_name_catalog(&catalog.npc_name_categories)
        || catalog.regional_terrain.len() > MAX_WORLDGEN_REGIONAL_TABLES
        || catalog.regional_furniture.len() > MAX_WORLDGEN_REGIONAL_TABLES
        || catalog.omt_generators.is_empty()
        || catalog.omt_generators.len() > MAX_WORLDGEN_OMT_GENERATORS
    {
        return false;
    }
    if !catalog.terrain_prototypes.iter().all(valid_terrain_tile)
        || !catalog
            .terrain_prototypes
            .windows(2)
            .all(|pair| pair[0].terrain_id < pair[1].terrain_id)
        || !catalog
            .furniture_prototypes
            .iter()
            .all(valid_furniture_tile)
        || !catalog
            .furniture_prototypes
            .windows(2)
            .all(|pair| pair[0].furniture_id < pair[1].furniture_id)
        || !catalog
            .regional_terrain
            .windows(2)
            .all(|pair| pair[0].regional_id < pair[1].regional_id)
        || !catalog
            .regional_furniture
            .windows(2)
            .all(|pair| pair[0].regional_id < pair[1].regional_id)
        || !catalog
            .omt_generators
            .windows(2)
            .all(|pair| pair[0].omt_id < pair[1].omt_id)
    {
        return false;
    }

    let mut regional_choice_count = 0_usize;
    for table in &catalog.regional_terrain {
        if !valid_worldgen_regional_terrain_table(table, catalog.terrain_prototypes.len()) {
            return false;
        }
        let Some(total) = regional_choice_count.checked_add(table.choices.len()) else {
            return false;
        };
        regional_choice_count = total;
    }
    for table in &catalog.regional_furniture {
        if !valid_worldgen_regional_furniture_table(table, catalog.furniture_prototypes.len()) {
            return false;
        }
        let Some(total) = regional_choice_count.checked_add(table.choices.len()) else {
            return false;
        };
        regional_choice_count = total;
    }
    if regional_choice_count > MAX_WORLDGEN_REGIONAL_CHOICES_TOTAL
        || !valid_worldgen_regional_terrain_graph(catalog)
        || !valid_worldgen_regional_furniture_graph(catalog)
    {
        return false;
    }

    let identity_ids = catalog
        .overmap
        .identities
        .iter()
        .map(|identity| identity.full_id.as_str())
        .collect::<BTreeSet<_>>();
    let npc_name_category_ids = catalog
        .npc_name_categories
        .iter()
        .map(|category| category.category_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut template_count = 0_usize;
    let mut nested_template_count = 0_usize;
    let mut nested_placement_count = 0_usize;
    let mut vehicle_placement_count = 0_usize;
    let mut cell_target_count = 0_usize;
    for generator in &catalog.omt_generators {
        if !valid_worldgen_id(&generator.omt_id)
            || generator.templates.is_empty()
            || generator.templates.len() > MAX_WORLDGEN_TEMPLATES_PER_OMT
            || !checked_positive_weight_sum(
                generator.templates.iter().map(|template| template.weight),
            )
        {
            return false;
        }
        let Some(total) = template_count.checked_add(generator.templates.len()) else {
            return false;
        };
        template_count = total;
        if template_count > MAX_WORLDGEN_TEMPLATES {
            return false;
        }
        for template in &generator.templates {
            let valid_body = match template.builtin {
                Some(builtin) => {
                    valid_worldgen_builtin_mapgen(builtin, catalog)
                        && template.cells.is_empty()
                        && template.predecessor_id.is_none()
                        && template.nested.is_empty()
                        && template.area_items.is_empty()
                        && template.npc_placements.is_empty()
                        && template.vehicle_placements.is_empty()
                        && template.monster_placements.is_empty()
                        && template.individual_monster_placements.is_empty()
                        && !template.erase_all_before_placing_terrain
                        && match builtin {
                            WorldgenBuiltinMapgenV1::ForestWater => {
                                template.deferred_fields == ["forest_components"]
                            }
                            _ => template.deferred_fields.is_empty(),
                        }
                }
                None => {
                    template.cells.len() == WORLDGEN_CELLS_PER_OMT
                        && (template.predecessor_id.is_some()
                            || template.cells.iter().all(|cell| !cell.terrain.is_empty()))
                }
            };
            if !valid_body
                || template
                    .predecessor_id
                    .as_ref()
                    .is_some_and(|id| !valid_worldgen_id(id))
                || template.nested.len() > MAX_WORLDGEN_NESTED_PLACEMENTS_PER_TEMPLATE
                || !template
                    .nested
                    .iter()
                    .all(|placement| valid_worldgen_nested_placement(placement, &identity_ids))
                || !template
                    .area_items
                    .iter()
                    .all(valid_worldgen_area_item_placement)
                || !template.npc_placements.iter().all(|placement| {
                    valid_worldgen_npc_placement(placement, &npc_name_category_ids)
                })
                || !template.vehicle_placements.iter().all(|placement| {
                    worldgen_vehicle_placement_is_valid(placement, catalog.vehicle_groups.len())
                })
                || !template.monster_placements.iter().all(|placement| {
                    valid_worldgen_monster_placement(placement, catalog.monster_groups.len())
                })
                || !template
                    .individual_monster_placements
                    .iter()
                    .all(|placement| {
                        valid_worldgen_individual_monster_placement(
                            placement,
                            catalog.monster_prototypes.len(),
                            catalog.monster_groups.len(),
                        )
                    })
                || !valid_worldgen_deferred_fields(&template.deferred_fields)
            {
                return false;
            }
            let Some(total) = nested_placement_count
                .checked_add(template.nested.len())
                .and_then(|total| total.checked_add(template.area_items.len()))
                .and_then(|total| total.checked_add(template.npc_placements.len()))
                .and_then(|total| total.checked_add(template.vehicle_placements.len()))
                .and_then(|total| total.checked_add(template.monster_placements.len()))
                .and_then(|total| total.checked_add(template.individual_monster_placements.len()))
            else {
                return false;
            };
            nested_placement_count = total;
            if nested_placement_count > MAX_WORLDGEN_NESTED_PLACEMENTS {
                return false;
            }
            let Some(total) =
                vehicle_placement_count.checked_add(template.vehicle_placements.len())
            else {
                return false;
            };
            vehicle_placement_count = total;
            if vehicle_placement_count > MAX_WORLDGEN_VEHICLE_PLACEMENTS {
                return false;
            }
            for cell in &template.cells {
                if !valid_worldgen_cell_shape(cell, catalog) {
                    return false;
                }
                let Some(total) = cell_target_count
                    .checked_add(cell.terrain.iter().map(Vec::len).sum::<usize>())
                    .and_then(|total| {
                        total.checked_add(cell.furniture.iter().map(Vec::len).sum::<usize>())
                    })
                else {
                    return false;
                };
                cell_target_count = total;
                if cell_target_count > MAX_WORLDGEN_WEIGHTED_CELL_TARGETS {
                    return false;
                }
            }
        }
        if generator.nested_generators.len() > MAX_WORLDGEN_NESTED_GENERATORS_PER_OMT
            || !generator
                .nested_generators
                .windows(2)
                .all(|pair| pair[0].nested_id < pair[1].nested_id)
            || !valid_worldgen_nested_graph(generator)
        {
            return false;
        }
        for nested in &generator.nested_generators {
            if !valid_worldgen_id(&nested.nested_id)
                || nested.nested_id == "null"
                || nested.templates.is_empty()
                || nested.templates.len() > MAX_WORLDGEN_NESTED_TEMPLATES_PER_GENERATOR
                || !checked_positive_weight_sum(
                    nested.templates.iter().map(|template| template.weight),
                )
            {
                return false;
            }
            let Some(total) = nested_template_count.checked_add(nested.templates.len()) else {
                return false;
            };
            nested_template_count = total;
            if nested_template_count > MAX_WORLDGEN_NESTED_TEMPLATES {
                return false;
            }
            for template in &nested.templates {
                let expected_cells =
                    usize::from(template.width).checked_mul(usize::from(template.height));
                if !(1..=WORLDGEN_OMT_SIZE as u8).contains(&template.width)
                    || !(1..=WORLDGEN_OMT_SIZE as u8).contains(&template.height)
                    || expected_cells != Some(template.cells.len())
                    || template.nested.len() > MAX_WORLDGEN_NESTED_PLACEMENTS_PER_TEMPLATE
                    || !template
                        .nested
                        .iter()
                        .all(|placement| valid_worldgen_nested_placement(placement, &identity_ids))
                    || !template
                        .area_items
                        .iter()
                        .all(valid_worldgen_area_item_placement)
                    || !template.npc_placements.iter().all(|placement| {
                        valid_worldgen_npc_placement(placement, &npc_name_category_ids)
                    })
                    || !template.vehicle_placements.iter().all(|placement| {
                        worldgen_vehicle_placement_is_valid(placement, catalog.vehicle_groups.len())
                    })
                    || !template.monster_placements.iter().all(|placement| {
                        valid_worldgen_monster_placement(placement, catalog.monster_groups.len())
                    })
                    || !template
                        .individual_monster_placements
                        .iter()
                        .all(|placement| {
                            valid_worldgen_individual_monster_placement(
                                placement,
                                catalog.monster_prototypes.len(),
                                catalog.monster_groups.len(),
                            )
                        })
                    || !valid_worldgen_deferred_fields(&template.deferred_fields)
                {
                    return false;
                }
                let Some(total) = nested_placement_count
                    .checked_add(template.nested.len())
                    .and_then(|total| total.checked_add(template.area_items.len()))
                    .and_then(|total| total.checked_add(template.npc_placements.len()))
                    .and_then(|total| total.checked_add(template.vehicle_placements.len()))
                    .and_then(|total| total.checked_add(template.monster_placements.len()))
                    .and_then(|total| {
                        total.checked_add(template.individual_monster_placements.len())
                    })
                else {
                    return false;
                };
                nested_placement_count = total;
                if nested_placement_count > MAX_WORLDGEN_NESTED_PLACEMENTS {
                    return false;
                }
                let Some(total) =
                    vehicle_placement_count.checked_add(template.vehicle_placements.len())
                else {
                    return false;
                };
                vehicle_placement_count = total;
                if vehicle_placement_count > MAX_WORLDGEN_VEHICLE_PLACEMENTS {
                    return false;
                }
                for cell in &template.cells {
                    if !valid_worldgen_cell_shape(cell, catalog) {
                        return false;
                    }
                    let Some(total) = cell_target_count
                        .checked_add(cell.terrain.iter().map(Vec::len).sum::<usize>())
                        .and_then(|total| {
                            total.checked_add(cell.furniture.iter().map(Vec::len).sum::<usize>())
                        })
                    else {
                        return false;
                    };
                    cell_target_count = total;
                    if cell_target_count > MAX_WORLDGEN_WEIGHTED_CELL_TARGETS {
                        return false;
                    }
                }
            }
        }
    }

    valid_worldgen_predecessor_graph(catalog)
        && catalog.overmap.identities.iter().all(|identity| {
            catalog
                .omt_generators
                .binary_search_by(|generator| generator.omt_id.as_str().cmp(&identity.generator_id))
                .is_ok()
        })
}

/// Validates a canonical worldgen catalog and all named item-group placement
/// references. The item-group catalog may contain definitions used elsewhere.
#[must_use]
pub fn worldgen_catalog_is_valid(
    catalog: &WorldgenCatalogV1,
    item_groups: &[ItemGroupDefinitionV1],
) -> bool {
    if !worldgen_catalog_shape_is_valid(catalog) || !item_group_catalog_is_valid(item_groups) {
        return false;
    }
    let group_ids = item_groups
        .iter()
        .map(|definition| definition.group_id.as_str())
        .collect::<BTreeSet<_>>();
    let root_cells_are_valid = catalog
        .omt_generators
        .iter()
        .flat_map(|generator| &generator.templates)
        .flat_map(|template| &template.cells)
        .filter_map(|cell| cell.item_group.as_ref())
        .all(|placement| group_ids.contains(placement.group_id.as_str()));
    let root_areas_are_valid = catalog
        .omt_generators
        .iter()
        .flat_map(|generator| &generator.templates)
        .flat_map(|template| &template.area_items)
        .all(|placement| group_ids.contains(placement.item_group.group_id.as_str()));
    let nested_cells_are_valid = catalog
        .omt_generators
        .iter()
        .flat_map(|generator| &generator.nested_generators)
        .flat_map(|generator| &generator.templates)
        .flat_map(|template| &template.cells)
        .filter_map(|cell| cell.item_group.as_ref())
        .all(|placement| group_ids.contains(placement.group_id.as_str()));
    let nested_areas_are_valid = catalog
        .omt_generators
        .iter()
        .flat_map(|generator| &generator.nested_generators)
        .flat_map(|generator| &generator.templates)
        .flat_map(|template| &template.area_items)
        .all(|placement| group_ids.contains(placement.item_group.group_id.as_str()));
    let vehicle_cargo_is_valid = catalog
        .vehicle_prototypes
        .iter()
        .flat_map(|prototype| &prototype.item_spawns)
        .flat_map(|spawn| &spawn.item_group_ids)
        .all(|group_id| group_ids.contains(group_id.as_str()));
    root_cells_are_valid
        && root_areas_are_valid
        && nested_cells_are_valid
        && nested_areas_are_valid
        && vehicle_cargo_is_valid
}

impl WorldSnapshotV1 {
    /// Validates the complete live vehicle family, passenger positions, owner
    /// closure, and allocator-counter uniqueness across every stable object in
    /// the canonical world. Stable ID wrapper types are distinct in Rust, but
    /// the allocator namespace is intentionally shared across those types.
    #[must_use]
    pub fn vehicles_are_valid(&self) -> bool {
        let Some(catalog) = self.worldgen.as_ref() else {
            return self.vehicles.is_empty();
        };
        let actors = self
            .actors
            .iter()
            .map(|actor| (actor.id, actor.position))
            .collect::<Vec<_>>();
        if !vehicle_snapshots_are_valid(
            self.world_namespace,
            &catalog.vehicle_part_types,
            &catalog.vehicle_prototypes,
            &self.vehicles,
            &actors,
        ) || self.vehicles.iter().any(|vehicle| {
            !vehicle.owner_faction_id.is_empty()
                && !self
                    .factions
                    .iter()
                    .any(|faction| faction.faction_id == vehicle.owner_faction_id)
        }) {
            return false;
        }

        let mut counters = BTreeSet::new();
        let mut item_ids = BTreeSet::new();
        for actor in &self.actors {
            if actor.id.counter() == 0
                || actor.id.world_namespace() != self.world_namespace
                || !counters.insert(actor.id.counter())
                || actor.missions.iter().any(|mission| {
                    mission.mission_id.counter() == 0
                        || mission.mission_id.world_namespace() != self.world_namespace
                        || !counters.insert(mission.mission_id.counter())
                })
                || actor
                    .pending_interaction
                    .as_ref()
                    .is_some_and(|interaction| {
                        interaction.interaction_id.counter() == 0
                            || interaction.interaction_id.world_namespace() != self.world_namespace
                            || !counters.insert(interaction.interaction_id.counter())
                    })
                || !actor
                    .inventory
                    .iter()
                    .all(|item| collect_stable_item_ids(item, self.world_namespace, &mut item_ids))
                || actor.craft_activity.as_ref().is_some_and(|activity| {
                    !activity.consumed_items.iter().all(|consumed| {
                        collect_stable_item_ids(&consumed.item, self.world_namespace, &mut item_ids)
                    }) || activity.reserved_output_items.iter().any(|item_id| {
                        item_id.counter() == 0
                            || item_id.world_namespace() != self.world_namespace
                            || !item_ids.insert(*item_id)
                    })
                })
                || actor.disassembly_activity.as_ref().is_some_and(|activity| {
                    !collect_stable_item_ids(
                        &activity.target_item,
                        self.world_namespace,
                        &mut item_ids,
                    ) || activity.reserved_component_items.iter().any(|item_id| {
                        item_id.counter() == 0
                            || item_id.world_namespace() != self.world_namespace
                            || !item_ids.insert(*item_id)
                    })
                })
                || actor
                    .construction_activity
                    .as_ref()
                    .is_some_and(|activity| {
                        !activity.consumed_items.iter().all(|consumed| {
                            collect_stable_item_ids(
                                &consumed.item,
                                self.world_namespace,
                                &mut item_ids,
                            )
                        })
                    })
            {
                return false;
            }
        }
        if !self.ground_items.iter().all(|ground| {
            collect_stable_item_ids(&ground.item, self.world_namespace, &mut item_ids)
        }) || !self.vehicles.iter().all(|vehicle| {
            vehicle.parts.iter().all(|part| {
                part.cargo
                    .iter()
                    .all(|item| collect_stable_item_ids(item, self.world_namespace, &mut item_ids))
            })
        }) || item_ids
            .iter()
            .any(|item_id| !counters.insert(item_id.counter()))
        {
            return false;
        }
        if self.npcs.iter().any(|npc| {
            npc.id.counter() == 0
                || npc.id.world_namespace() != self.world_namespace
                || !counters.insert(npc.id.counter())
        }) || self.creatures.iter().any(|creature| {
            creature.id.counter() == 0
                || creature.id.world_namespace() != self.world_namespace
                || !counters.insert(creature.id.counter())
        }) {
            return false;
        }
        insert_vehicle_stable_counters(self.world_namespace, &self.vehicles, &mut counters)
    }

    /// Validates the canonical item-group catalog and every terrain/furniture
    /// consumer in one pass. Full world restoration performs its other domain
    /// checks separately.
    #[must_use]
    pub fn item_groups_are_valid(&self) -> bool {
        let mut sources = self
            .terrain_bash_types
            .iter()
            .filter_map(|bash| bash.drop_source.as_ref())
            .chain(
                self.furniture_bash_types
                    .iter()
                    .filter_map(|bash| bash.drop_source.as_ref()),
            )
            .cloned()
            .collect::<Vec<_>>();
        sources.extend(
            self.worldgen
                .iter()
                .flat_map(|catalog| &catalog.omt_generators)
                .flat_map(|generator| &generator.templates)
                .flat_map(|template| &template.cells)
                .filter_map(|cell| cell.item_group.as_ref())
                .map(|placement| ItemGroupSourceV1::Group(placement.group_id.clone())),
        );
        sources.extend(
            self.worldgen
                .iter()
                .flat_map(|catalog| &catalog.omt_generators)
                .flat_map(|generator| &generator.templates)
                .flat_map(|template| &template.area_items)
                .map(|placement| ItemGroupSourceV1::Group(placement.item_group.group_id.clone())),
        );
        sources.extend(
            self.worldgen
                .iter()
                .flat_map(|catalog| &catalog.omt_generators)
                .flat_map(|generator| &generator.nested_generators)
                .flat_map(|generator| &generator.templates)
                .flat_map(|template| &template.cells)
                .filter_map(|cell| cell.item_group.as_ref())
                .map(|placement| ItemGroupSourceV1::Group(placement.group_id.clone())),
        );
        sources.extend(
            self.worldgen
                .iter()
                .flat_map(|catalog| &catalog.omt_generators)
                .flat_map(|generator| &generator.nested_generators)
                .flat_map(|generator| &generator.templates)
                .flat_map(|template| &template.area_items)
                .map(|placement| ItemGroupSourceV1::Group(placement.item_group.group_id.clone())),
        );
        let source_refs = sources.iter().collect::<Vec<_>>();
        item_group_sources_are_valid(&self.item_groups, source_refs.iter().copied())
            && item_group_sources_have_exact_named_closure(&self.item_groups, &source_refs)
    }
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
    valid_item_snapshot_at(item, 0)
}

fn valid_item_rot_metadata(
    variables: &BTreeMap<String, ItemVariableValueV1>,
    comestible_type: &str,
    containment: &ItemContainmentProfileV1,
    has_temperature: bool,
    raw_damage: u16,
) -> bool {
    if !item_rot_variables_are_valid(variables) {
        return false;
    }
    let corpse = containment
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("CORPSE"))
        .is_ok();
    let corpse_source = variables.get(ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE);
    match item_rot_state(variables) {
        Some((shelf_life_turns, _)) if corpse => {
            has_temperature
                && raw_damage == MAX_ITEM_RAW_DAMAGE
                && shelf_life_turns == ITEM_STATIC_CORPSE_SHELF_LIFE_TURNS
                && matches!(
                    corpse_source,
                    Some(ItemVariableValueV1::String(source)) if valid_recipe_id(source)
                )
        }
        Some(_) => has_temperature && !comestible_type.is_empty() && corpse_source.is_none(),
        None => {
            corpse_source.is_none() && !corpse && (!has_temperature || !comestible_type.is_empty())
        }
    }
}

fn valid_item_snapshot_at(item: &ItemSnapshot, depth: usize) -> bool {
    if depth > MAX_ITEM_COMPONENT_DEPTH {
        return false;
    }
    !item.type_id.is_empty()
        && item.type_id.len() <= 512
        && item.damage <= MAX_ITEM_DAMAGE_LEVEL
        && item.raw_damage <= MAX_ITEM_RAW_DAMAGE
        && item.damage == item_damage_level(item.raw_damage)
        && valid_item_fit_state(item.fitted, &item.containment)
        && item.variant.as_ref().is_none_or(item_variant_is_valid)
        && item.snippet.as_ref().is_none_or(item_snippet_is_valid)
        && valid_item_variables(&item.variables)
        && item_degradation_matches_damage(&item.variables, item.raw_damage)
        && item
            .type_id
            .chars()
            .all(|character| !character.is_control())
        && valid_item_containment_profile(&item.containment)
        && (!item.containment.count_by_charges
            || item.charges > 0
            || (item.ammunition_type == "battery"
                && item.charges == 0
                && item.residual_energy_millijoules > 0))
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
        && item
            .temperature
            .as_ref()
            .is_none_or(valid_item_temperature_state)
        && item
            .temperature
            .as_ref()
            .is_none_or(|temperature| temperature.current_phase == item.containment.phase)
        && valid_item_rot_metadata(
            &item.variables,
            &item.comestible_type,
            &item.containment,
            item.temperature.is_some(),
            item.raw_damage,
        )
        && item_pocket_insulation_variables_are_valid(
            &item.variables,
            item.ammunition_containers
                .iter()
                .map(|pocket| pocket.pocket_index),
        )
        && item.ammunition_type.len() <= 64
        && item
            .ammunition_type
            .chars()
            .all(|character| !character.is_control())
        && (item.ammunition_type.is_empty()
            || item.charges > 0
            || (item.magazine_capacity > 0 && item.charges >= 0)
            || (item.ammunition_type == "battery"
                && item.charges >= 0
                && item.residual_energy_millijoules > 0))
        && (item.magazine_capacity == 0
            || (item.charges >= 0
                && u32::try_from(item.charges)
                    .is_ok_and(|charges| charges <= item.magazine_capacity)
                && !item.ammunition_type.is_empty()
                && item.ranged_weapon.is_none()
                && item.magazine_wells.is_empty()
                && item.ammunition_containers.is_empty()))
        && (item.residual_energy_millijoules == 0
            || (item.magazine_capacity > 0
                && u32::try_from(item.charges)
                    .is_ok_and(|charges| charges < item.magazine_capacity)
                && item.residual_energy_millijoules < MILLIJOULES_PER_BATTERY_CHARGE)
            || (item.magazine_capacity == 0
                && item.ammunition_type == "battery"
                && item.charges >= 0
                && item.ranged_weapon.is_none()
                && item.component_provenance.is_none()
                && item.integral_magazines.is_empty()
                && item.magazine_wells.is_empty()
                && item.ammunition_containers.is_empty()
                && item.powered_tool.is_none()
                && item.creature_corpse.is_none()
                && item.residual_energy_millijoules < MILLIJOULES_PER_BATTERY_CHARGE))
        && (item.magazine_wells.is_empty() || {
            item.charges == 0
                && item.magazine_capacity == 0
                && item.ammunition_type.is_empty()
                && item.ranged_weapon.is_none()
        })
        && (item.integral_magazines.is_empty() || {
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
        && valid_integral_magazine_snapshots(&item.integral_magazines, depth)
        && valid_magazine_well_snapshots(&item.magazine_wells, depth)
        && valid_ammunition_container_snapshots(&item.ammunition_containers, depth)
        && item_snapshot_sealing_is_valid(item)
        && item.integral_magazines.iter().all(|magazine| {
            item.magazine_wells
                .iter()
                .all(|well| well.pocket_index != magazine.pocket_index)
                && item
                    .ammunition_containers
                    .iter()
                    .all(|pocket| pocket.pocket_index != magazine.pocket_index)
        })
        && item.magazine_wells.iter().all(|well| {
            item.ammunition_containers
                .iter()
                .all(|pocket| pocket.pocket_index != well.pocket_index)
        })
        && item.powered_tool.as_ref().is_none_or(|powered| {
            item.magazine_wells
                .iter()
                .any(|well| well.pocket_index == powered.power_pocket_index)
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
                && item.temperature.is_none()
                && item.ammunition_type.is_empty()
                && item.ranged_weapon.is_none()
                && item.component_provenance.is_none()
                && item.magazine_capacity == 0
                && item.integral_magazines.is_empty()
                && item.magazine_wells.is_empty()
                && item.ammunition_containers.is_empty()
                && item.residual_energy_millijoules == 0
                && item.powered_tool.is_none()
                && valid_creature_corpse_prototype(&corpse.prototype)
                && (!corpse.revivable || corpse.prototype.revives)
                && (!corpse.revive_special || corpse.prototype.revives)
        })
}

fn valid_item_containment_profile(profile: &ItemContainmentProfileV1) -> bool {
    profile.flags.len() <= 256
        && profile.flags.iter().all(|flag| valid_recipe_id(flag))
        && profile.flags.windows(2).all(|pair| pair[0] < pair[1])
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
    well.pocket_id.len() <= 512
        && !well.pocket_id.chars().any(char::is_control)
        && !well.compatible_magazine_type_ids.is_empty()
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

fn valid_integral_magazine_prototype(pocket: &IntegralMagazinePocketPrototypeV1) -> bool {
    pocket.pocket_id.len() <= 512
        && !pocket.pocket_id.chars().any(char::is_control)
        && valid_recipe_id(&pocket.ammunition_type)
        && pocket.capacity > 0
        && pocket.capacity <= i32::MAX as u32
}

fn valid_integral_magazine_snapshot(
    pocket: &IntegralMagazinePocketSnapshotV1,
    depth: usize,
) -> bool {
    valid_integral_magazine_prototype(&IntegralMagazinePocketPrototypeV1 {
        pocket_index: pocket.pocket_index,
        pocket_id: pocket.pocket_id.clone(),
        ammunition_type: pocket.ammunition_type.clone(),
        capacity: pocket.capacity,
        rigid: pocket.rigid,
        reloadable: pocket.reloadable,
        unloadable: pocket.unloadable,
    }) && (pocket.residual_energy_millijoules == 0
        || (pocket.ammunition_type == "battery"
            && pocket.residual_energy_millijoules < MILLIJOULES_PER_BATTERY_CHARGE
            && pocket.loaded_ammunition.is_some()))
        && pocket.loaded_ammunition.as_ref().is_none_or(|ammunition| {
            ammunition.ammunition_type == pocket.ammunition_type
                && ammunition.charges >= 0
                && (ammunition.charges > 0 || pocket.residual_energy_millijoules > 0)
                && u32::try_from(ammunition.charges).is_ok_and(|charges| {
                    charges <= pocket.capacity
                        && (pocket.residual_energy_millijoules == 0 || charges < pocket.capacity)
                })
                && ammunition.comestible_type.is_empty()
                && ammunition.ranged_weapon.is_none()
                && ammunition.component_provenance.is_none()
                && ammunition.magazine_capacity == 0
                && ammunition.integral_magazines.is_empty()
                && ammunition.magazine_wells.is_empty()
                && ammunition.ammunition_containers.is_empty()
                && ammunition.residual_energy_millijoules == 0
                && ammunition.powered_tool.is_none()
                && ammunition.creature_corpse.is_none()
                && if ammunition.charges > 0 {
                    valid_item_snapshot_at(ammunition, depth + 1)
                } else {
                    let mut materialized = (**ammunition).clone();
                    materialized.charges = 1;
                    valid_item_snapshot_at(&materialized, depth + 1)
                }
        })
}

fn valid_integral_magazine_prototypes(pockets: &[IntegralMagazinePocketPrototypeV1]) -> bool {
    pockets.len() <= MAX_ITEM_INTEGRAL_MAGAZINES
        && pockets.iter().all(valid_integral_magazine_prototype)
        && pockets
            .windows(2)
            .all(|pair| pair[0].pocket_index < pair[1].pocket_index)
}

fn valid_integral_magazine_snapshots(
    pockets: &[IntegralMagazinePocketSnapshotV1],
    depth: usize,
) -> bool {
    pockets.len() <= MAX_ITEM_INTEGRAL_MAGAZINES
        && pockets
            .iter()
            .all(|pocket| valid_integral_magazine_snapshot(pocket, depth))
        && pockets
            .windows(2)
            .all(|pair| pair[0].pocket_index < pair[1].pocket_index)
}

fn valid_ammunition_capacity(capacity: &AmmunitionCapacityV1) -> bool {
    valid_recipe_id(&capacity.ammunition_type)
        && capacity.capacity > 0
        && capacity.capacity <= i32::MAX as u32
}

fn valid_ammunition_container_prototype(pocket: &AmmunitionContainerPocketPrototypeV1) -> bool {
    let base = pocket.pocket_id.len() <= 512
        && !pocket.pocket_id.chars().any(char::is_control)
        && pocket.access_moves > 0;
    if !base {
        return false;
    }
    match &pocket.spawn_rules {
        None => {
            !pocket.capacities.is_empty()
                && pocket.capacities.len() <= MAX_AMMUNITION_CONTAINER_TYPES
                && pocket.capacities.iter().all(valid_ammunition_capacity)
                && pocket
                    .capacities
                    .windows(2)
                    .all(|pair| pair[0].ammunition_type < pair[1].ammunition_type)
        }
        Some(rules) => {
            pocket.capacities.is_empty()
                && pocket.rigid == rules.rigid
                && pocket.access_moves == rules.access_moves
                && valid_spawn_pocket_rules(rules)
        }
    }
}

fn valid_spawn_pocket_rules(rules: &SpawnPocketRulesV1) -> bool {
    rules.access_moves > 0
        && rules.item_restrictions.len() <= 256
        && rules.flag_restrictions.len() <= 256
        && rules.item_restrictions.iter().all(|id| valid_recipe_id(id))
        && rules.flag_restrictions.iter().all(|id| valid_recipe_id(id))
        && !rules
            .flag_restrictions
            .iter()
            .any(|restriction| is_reserved_spawn_pocket_marker(restriction))
        && rules
            .item_restrictions
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && rules
            .flag_restrictions
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        && rules.min_item_volume_milliliters <= rules.max_item_volume_milliliters
        && rules.magazine_well_volume_milliliters < rules.max_contains_volume_milliliters
        && (!rules.rigid || rules.magazine_well_volume_milliliters == 0)
        && match rules.kind {
            SpawnPocketKindV1::Container => {
                rules.max_contains_volume_milliliters > 0
                    && rules.max_contains_weight_milligrams > 0
                    && rules.max_item_volume_milliliters > 0
                    && rules.max_item_length_millimeters > 0
            }
            SpawnPocketKindV1::EFileStorage => {
                rules.rigid
                    && rules.magazine_well_volume_milliliters == 0
                    && !rules.contents_collapsed_by_default
                    && !spawn_pocket_is_single_item(rules)
                    && !spawn_pocket_is_open_container(rules)
            }
        }
}

fn valid_ammunition_container_snapshot(
    pocket: &AmmunitionContainerPocketSnapshotV1,
    depth: usize,
) -> bool {
    let prototype = AmmunitionContainerPocketPrototypeV1 {
        pocket_index: pocket.pocket_index,
        pocket_id: pocket.pocket_id.clone(),
        capacities: pocket.capacities.clone(),
        rigid: pocket.rigid,
        access_moves: pocket.access_moves,
        reloadable: pocket.reloadable,
        unloadable: pocket.unloadable,
        spawn_rules: pocket.spawn_state.as_ref().map(|state| state.rules.clone()),
    };
    valid_ammunition_container_prototype(&prototype)
        && pocket.contents.len() <= MAX_AMMUNITION_CONTAINER_CONTENTS
        && (!pocket
            .spawn_state
            .as_ref()
            .is_some_and(|state| spawn_pocket_is_single_item(&state.rules))
            || pocket.contents.len() <= 1)
        && pocket
            .contents
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
        && pocket.spawn_state.as_ref().is_none_or(|state| {
            (!state.sealed || state.rules.sealable)
                && (!state.rules.contents_collapsed_by_default || state.contents_collapsed)
                && state.rules.rigid == pocket.rigid
                && state.rules.access_moves == pocket.access_moves
        })
        && (pocket.spawn_state.is_some()
            || pocket.contents.first().is_none_or(|first| {
                pocket
                    .contents
                    .iter()
                    .all(|content| content.ammunition_type == first.ammunition_type)
            }))
        && pocket.contents.iter().all(|content| {
            if let Some(state) = &pocket.spawn_state {
                return valid_spawn_pocket_content(&state.rules, content, depth);
            }
            content.charges > 0
                && !content.ammunition_type.is_empty()
                && content.comestible_type.is_empty()
                && content.ranged_weapon.is_none()
                && content.component_provenance.is_none()
                && content.magazine_capacity == 0
                && content.integral_magazines.is_empty()
                && content.magazine_wells.is_empty()
                && content.ammunition_containers.is_empty()
                && content.residual_energy_millijoules == 0
                && content.powered_tool.is_none()
                && content.creature_corpse.is_none()
                && pocket
                    .capacities
                    .binary_search_by(|capacity| {
                        capacity.ammunition_type.cmp(&content.ammunition_type)
                    })
                    .is_ok()
                && valid_item_snapshot_at(content, depth + 1)
        })
        && pocket.spawn_state.as_ref().is_none_or(|state| {
            state.rules.kind == SpawnPocketKindV1::EFileStorage
                || pocket.contents.first().is_none_or(|first| {
                    if first.containment.phase == ItemPhaseV1::Liquid {
                        pocket.contents.iter().all(|content| {
                            content.containment.phase == ItemPhaseV1::Liquid
                                && item_snapshots_can_combine_for_containment(first, content)
                        })
                    } else {
                        pocket
                            .contents
                            .iter()
                            .all(|content| content.containment.phase != ItemPhaseV1::Liquid)
                    }
                })
        })
        && pocket.spawn_state.as_ref().is_none_or(|state| {
            state.rules.kind == SpawnPocketKindV1::EFileStorage
                || pocket
                    .contents
                    .iter()
                    .try_fold((0_u64, 0_u64), |(volume, weight), content| {
                        Some((
                            volume.checked_add(item_snapshot_containment_volume_milliliters(
                                content,
                            )?)?,
                            weight.checked_add(item_snapshot_containment_weight_milligrams(
                                content,
                            )?)?,
                        ))
                    })
                    .is_some_and(|(volume, weight)| {
                        volume <= state.rules.max_contains_volume_milliliters
                            && weight <= state.rules.max_contains_weight_milligrams
                    })
        })
        && (pocket.spawn_state.is_some()
            || pocket.capacities.iter().all(|capacity| {
                pocket
                    .contents
                    .iter()
                    .filter(|content| content.ammunition_type == capacity.ammunition_type)
                    .try_fold(0_u32, |total, content| {
                        u32::try_from(content.charges)
                            .ok()
                            .and_then(|charges| total.checked_add(charges))
                    })
                    .is_some_and(|total| total <= capacity.capacity)
            }))
}

#[must_use]
pub fn item_snapshot_sealing_is_valid(item: &ItemSnapshot) -> bool {
    let sealed_pockets = item.ammunition_containers.iter().filter(|pocket| {
        pocket
            .spawn_state
            .as_ref()
            .is_some_and(|state| state.sealed)
    });
    if sealed_pockets
        .clone()
        .any(|pocket| pocket.contents.is_empty())
    {
        return false;
    }
    if sealed_pockets.count() == 0 {
        return true;
    }
    item_snapshot_is_container_full(item).unwrap_or(false)
}

fn item_snapshot_is_container_full(item: &ItemSnapshot) -> Option<bool> {
    for pocket in &item.ammunition_containers {
        let Some(rules) = pocket
            .spawn_state
            .as_ref()
            .map(|state| &state.rules)
            .filter(|rules| rules.kind == SpawnPocketKindV1::Container)
        else {
            continue;
        };
        let Some(first) = pocket.contents.first() else {
            return Some(false);
        };
        if spawn_pocket_is_single_item(rules) {
            continue;
        }
        let (used_volume, used_weight) =
            pocket
                .contents
                .iter()
                .try_fold((0_u64, 0_u64), |(volume, weight), content| {
                    Some((
                        volume
                            .checked_add(item_snapshot_containment_volume_milliliters(content)?)?,
                        weight
                            .checked_add(item_snapshot_containment_weight_milligrams(content)?)?,
                    ))
                })?;
        if used_volume == rules.max_contains_volume_milliliters {
            continue;
        }
        let same_type = pocket
            .contents
            .iter()
            .all(|content| content.type_id == first.type_id);
        let (one_more_volume, one_more_weight) = if first.containment.count_by_charges {
            (
                item_containment_single_charge_volume_milliliters(&first.containment)?,
                item_containment_weight_milligrams(&first.containment, 1)?,
            )
        } else {
            (
                item_snapshot_containment_volume_milliliters(first)?,
                item_snapshot_containment_weight_milligrams(first)?,
            )
        };
        let can_fit_one_more = used_volume
            .checked_add(one_more_volume)
            .is_some_and(|volume| volume <= rules.max_contains_volume_milliliters)
            && used_weight
                .checked_add(one_more_weight)
                .is_some_and(|weight| weight <= rules.max_contains_weight_milligrams);
        if !same_type || can_fit_one_more {
            return Some(false);
        }
    }
    Some(true)
}

fn valid_spawn_pocket_content(
    rules: &SpawnPocketRulesV1,
    content: &ItemSnapshot,
    depth: usize,
) -> bool {
    if !valid_item_snapshot_at(content, depth + 1) {
        return false;
    }
    item_snapshot_is_compatible_with_spawn_rules(rules, content)
}

#[must_use]
pub fn item_snapshot_containment_weight_milligrams(item: &ItemSnapshot) -> Option<u64> {
    if item
        .containment
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("NO_DROP"))
        .is_ok()
    {
        return Some(0);
    }
    let own = item_containment_weight_milligrams(&item.containment, item.charges)?;
    let integral = item
        .integral_magazines
        .iter()
        .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
        .try_fold(0_u64, |total, content| {
            total.checked_add(item_snapshot_containment_weight_milligrams(content)?)
        })?;
    let wells = item
        .magazine_wells
        .iter()
        .filter_map(|pocket| pocket.installed_magazine.as_deref())
        .try_fold(0_u64, |total, content| {
            total.checked_add(item_snapshot_containment_weight_milligrams(content)?)
        })?;
    let containers = item
        .ammunition_containers
        .iter()
        .try_fold(0_u64, |total, pocket| {
            if pocket
                .spawn_state
                .as_ref()
                .is_some_and(|state| state.rules.kind == SpawnPocketKindV1::EFileStorage)
            {
                return Some(total);
            }
            let multiplier = item_pocket_weight_multiplier(&item.variables, pocket.pocket_index)?;
            pocket.contents.iter().try_fold(total, |total, content| {
                total.checked_add(spawn_pocket_content_weight_with_multiplier_milligrams(
                    item_snapshot_containment_weight_milligrams(content)?,
                    multiplier,
                )?)
            })
        })?;
    own.checked_add(integral)?
        .checked_add(wells)?
        .checked_add(containers)
}

#[must_use]
pub fn item_snapshot_containment_volume_milliliters(item: &ItemSnapshot) -> Option<u64> {
    let own = item_containment_volume_milliliters(&item.containment, item.charges)?;
    let integral = item
        .integral_magazines
        .iter()
        .try_fold(0_u64, |total, pocket| {
            match pocket.loaded_ammunition.as_deref() {
                Some(content) if !pocket.rigid => {
                    total.checked_add(item_snapshot_containment_volume_milliliters(content)?)
                }
                Some(_) | None => Some(total),
            }
        })?;
    let wells = item.magazine_wells.iter().try_fold(0_u64, |total, well| {
        match well.installed_magazine.as_deref() {
            Some(installed) if !well.rigid => {
                total.checked_add(item_snapshot_containment_volume_milliliters(installed)?)
            }
            Some(_) | None => Some(total),
        }
    })?;
    let containers = item
        .ammunition_containers
        .iter()
        .try_fold(0_u64, |total, pocket| {
            if pocket.rigid {
                return Some(total);
            }
            let contents_volume = pocket.contents.iter().try_fold(0_u64, |volume, content| {
                volume.checked_add(item_snapshot_containment_volume_milliliters(content)?)
            })?;
            let external = match pocket.spawn_state.as_ref() {
                None => Some(contents_volume),
                Some(state) => spawn_pocket_external_volume_with_multiplier_milliliters(
                    &state.rules,
                    contents_volume,
                    item_pocket_volume_multiplier(&item.variables, pocket.pocket_index)?,
                ),
            }?;
            total.checked_add(external)
        })?;
    own.checked_add(integral)?
        .checked_add(wells)?
        .checked_add(containers)
}

fn valid_ammunition_container_prototypes(pockets: &[AmmunitionContainerPocketPrototypeV1]) -> bool {
    pockets.len() <= MAX_ITEM_AMMUNITION_CONTAINER_POCKETS
        && pockets.iter().all(valid_ammunition_container_prototype)
        && pockets
            .windows(2)
            .all(|pair| pair[0].pocket_index < pair[1].pocket_index)
}

fn valid_ammunition_container_snapshots(
    pockets: &[AmmunitionContainerPocketSnapshotV1],
    depth: usize,
) -> bool {
    pockets.len() <= MAX_ITEM_AMMUNITION_CONTAINER_POCKETS
        && pockets
            .iter()
            .all(|pocket| valid_ammunition_container_snapshot(pocket, depth))
        && pockets
            .windows(2)
            .all(|pair| pair[0].pocket_index < pair[1].pocket_index)
}

fn valid_magazine_well_snapshot(well: &MagazineWellSnapshotV1, depth: usize) -> bool {
    let prototype = MagazineWellPrototypeV1 {
        pocket_index: well.pocket_index,
        pocket_id: well.pocket_id.clone(),
        compatible_magazine_type_ids: well.compatible_magazine_type_ids.clone(),
        rigid: well.rigid,
        unloadable: well.unloadable,
    };
    valid_magazine_well_prototype(&prototype)
        && well.installed_magazine.as_ref().is_none_or(|installed| {
            well.compatible_magazine_type_ids
                .binary_search(&installed.type_id)
                .is_ok()
                && (installed.magazine_capacity > 0 || !installed.integral_magazines.is_empty())
                && installed.magazine_wells.is_empty()
                && installed.ammunition_containers.is_empty()
                && valid_item_snapshot_at(installed, depth + 1)
        })
}

fn valid_magazine_well_prototypes(wells: &[MagazineWellPrototypeV1]) -> bool {
    wells.len() <= MAX_ITEM_MAGAZINE_WELLS
        && wells.iter().all(valid_magazine_well_prototype)
        && wells
            .windows(2)
            .all(|pair| pair[0].pocket_index < pair[1].pocket_index)
}

fn valid_magazine_well_snapshots(wells: &[MagazineWellSnapshotV1], depth: usize) -> bool {
    wells.len() <= MAX_ITEM_MAGAZINE_WELLS
        && wells
            .iter()
            .all(|well| valid_magazine_well_snapshot(well, depth))
        && wells
            .windows(2)
            .all(|pair| pair[0].pocket_index < pair[1].pocket_index)
}

fn collect_stable_item_ids(
    item: &ItemSnapshot,
    world_namespace: u64,
    ids: &mut BTreeSet<ItemId>,
) -> bool {
    item.id.counter() > 0
        && item.id.world_namespace() == world_namespace
        && ids.insert(item.id)
        && item.integral_magazines.iter().all(|pocket| {
            pocket
                .loaded_ammunition
                .as_deref()
                .is_none_or(|ammunition| collect_stable_item_ids(ammunition, world_namespace, ids))
        })
        && item.magazine_wells.iter().all(|well| {
            well.installed_magazine
                .as_deref()
                .is_none_or(|installed| collect_stable_item_ids(installed, world_namespace, ids))
        })
        && item.ammunition_containers.iter().all(|pocket| {
            pocket
                .contents
                .iter()
                .all(|content| collect_stable_item_ids(content, world_namespace, ids))
        })
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
        && state
            .temperature
            .as_ref()
            .map(|temperature| temperature.current_phase)
            == prototype
                .tracks_temperature
                .then_some(prototype.containment.phase)
        && state
            .temperature
            .as_ref()
            .and_then(|temperature| temperature.thermal_properties.as_ref())
            == prototype.thermal_properties.as_ref()
        && state.ammunition_type == prototype.ammunition_type
        && state.ranged_weapon == prototype.ranged_weapon
        && state.magazine_capacity == prototype.magazine_capacity
        && state.integral_magazines == prototype.integral_magazines
        && state.magazine_wells == prototype.magazine_wells
        && state.ammunition_containers == prototype.ammunition_containers
        && state.residual_energy_millijoules == prototype.residual_energy_millijoules
        && state.powered_tool == prototype.powered_tool
        && state.containment == prototype.containment
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
        && component.raw_damage <= MAX_ITEM_RAW_DAMAGE
        && component.damage == item_damage_level(component.raw_damage)
        && valid_item_fit_state(component.fitted, &component.containment)
        && component.variant.as_ref().is_none_or(item_variant_is_valid)
        && component.snippet.as_ref().is_none_or(item_snippet_is_valid)
        && valid_item_variables(&component.variables)
        && item_degradation_matches_damage(&component.variables, component.raw_damage)
        && valid_item_containment_profile(&component.containment)
        && component.count_by_charges == component.containment.count_by_charges
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
        && component
            .temperature
            .as_ref()
            .is_none_or(valid_item_temperature_state)
        && component
            .temperature
            .as_ref()
            .is_none_or(|temperature| temperature.current_phase == component.containment.phase)
        && valid_item_rot_metadata(
            &component.variables,
            &component.comestible_type,
            &component.containment,
            component.temperature.is_some(),
            component.raw_damage,
        )
        && item_pocket_insulation_variables_are_valid(
            &component.variables,
            component
                .ammunition_containers
                .iter()
                .map(|pocket| pocket.pocket_index),
        )
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
                && component.integral_magazines.is_empty()
                && component.magazine_wells.is_empty()
                && component.ammunition_containers.is_empty()))
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
        && valid_integral_magazine_prototypes(&component.integral_magazines)
        && valid_magazine_well_prototypes(&component.magazine_wells)
        && valid_ammunition_container_prototypes(&component.ammunition_containers)
        && component.integral_magazines.iter().all(|magazine| {
            component
                .magazine_wells
                .iter()
                .all(|well| well.pocket_index != magazine.pocket_index)
                && component
                    .ammunition_containers
                    .iter()
                    .all(|pocket| pocket.pocket_index != magazine.pocket_index)
        })
        && component.magazine_wells.iter().all(|well| {
            component
                .ammunition_containers
                .iter()
                .all(|pocket| pocket.pocket_index != well.pocket_index)
        })
        && component.powered_tool.as_ref().is_none_or(|powered| {
            component
                .magazine_wells
                .iter()
                .any(|well| well.pocket_index == powered.power_pocket_index)
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
            .all(|ground| collect_stable_item_ids(&ground.item, namespace, &mut item_ids))
        && snapshot.vehicles.iter().all(|vehicle| {
            vehicle.tiles.iter().all(|tile| {
                tile.cargo
                    .iter()
                    .all(|item| collect_stable_item_ids(item, namespace, &mut item_ids))
            })
        });
    stable_item_ids_are_valid
        && snapshot.calendar == CalendarSnapshot::at_tick(snapshot.tick)
        && snapshot.natural_light == NaturalLightSnapshot::at_tick(snapshot.tick)
        && actor_eoc_variables_are_valid(&snapshot.controlled_actor.eoc_variables)
        && actor_eoc_schedule_is_valid(
            &snapshot.controlled_actor.scheduled_eocs,
            snapshot.controlled_actor.next_eoc_schedule_sequence,
        )
        && actor_inactive_recurring_eocs_are_valid(
            &snapshot.controlled_actor.inactive_recurring_eocs,
        )
        && snapshot.visible_actors.len() <= 65_536
        && snapshot.npcs.len() <= 65_536
        && snapshot.creatures.len() <= 65_536
        && snapshot.vehicles.len() <= MAX_LIVE_VEHICLES
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
        && {
            let mut actor_ids = BTreeSet::from([snapshot.controlled_actor.id]);
            snapshot.visible_actors.iter().all(|actor| {
                actor.id.counter() > 0
                    && actor.id.world_namespace() == namespace
                    && actor_ids.insert(actor.id)
            }) && visible_vehicle_snapshots_are_valid(namespace, &snapshot.vehicles, &actor_ids)
        }
        && snapshot.npcs.iter().all(|npc| {
            npc.id.counter() > 0
                && npc.id.world_namespace() == namespace
                && !npc.name.is_empty()
                && npc.name.len() <= MAX_NPC_NAME_BYTES
                && !npc.name.chars().any(char::is_control)
                && !npc.template_id.is_empty()
                && npc.template_id.len() <= MAX_DIALOGUE_ID_BYTES
                && !npc.template_id.chars().any(char::is_control)
                && npc
                    .opinion_of_controlled_actor
                    .as_ref()
                    .is_none_or(opinion_is_valid)
        })
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
                output_state: Some(ItemComponentSnapshotV1 {
                    type_id: String::from("scrap"),
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
                    count_by_charges: false,
                    recoverable: true,
                    component_provenance: None,
                    magazine_capacity: 0,
                    integral_magazines: Vec::new(),
                    magazine_wells: Vec::new(),
                    ammunition_containers: Vec::new(),
                    residual_energy_millijoules: 0,
                    powered_tool: None,
                    containment: Default::default(),
                }),
            }],
            tools: Vec::new(),
            qualities: Vec::new(),
        }
    }

    fn item_group_item(type_id: &str) -> ItemGroupTargetV1 {
        let mut prototype = protocol_test_recipe().output;
        prototype.type_id = type_id.to_owned();
        ItemGroupTargetV1::Item(Box::new(ItemGroupItemPrototypeV1 {
            prototype,
            maximum_raw_damage: 0,
            variants: Vec::new(),
            description_expansion: None,
            snippets: Vec::new(),
            initial_variables: BTreeMap::new(),
            default_container: None,
            modifier_side_effects_supported: true,
            charges: None,
            minimum_one_charge: false,
            tool_charge_storage: None,
            charges_supported: true,
            charge_capacity: ItemGroupChargeCapacityV1::ModifierContainer,
            contents_insertion_supported: true,
        }))
    }

    fn item_group_entry(
        probability: u32,
        count_min: u16,
        count_max: u16,
        target: ItemGroupTargetV1,
    ) -> ItemGroupEntryV1 {
        ItemGroupEntryV1 {
            probability,
            count_min,
            count_max,
            raw_damage: None,
            variant_id: None,
            event: None,
            target,
            modifier_charges: None,
            contents: Vec::new(),
            seal_contents: false,
            modifier_default_container_sealed: None,
            direct_wrapper: None,
            modifier_container: None,
        }
    }

    fn item_group_modifier_entry(
        probability: u32,
        count_min: u16,
        count_max: u16,
        target: ItemGroupTargetV1,
    ) -> ItemGroupEntryV1 {
        ItemGroupEntryV1 {
            raw_damage: Some(InclusiveU16RangeV1 {
                minimum: 0,
                maximum: 0,
            }),
            modifier_charges: None,
            contents: Vec::new(),
            seal_contents: false,
            modifier_default_container_sealed: None,
            direct_wrapper: None,
            modifier_container: None,
            ..item_group_entry(probability, count_min, count_max, target)
        }
    }

    fn item_group_wrapper(type_id: &str, overflow: ItemGroupOverflowV1) -> ItemGroupContainerV1 {
        let ItemGroupTargetV1::Item(mut item) = item_group_item(type_id) else {
            unreachable!("fixture is a direct item")
        };
        item.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("CONTENTS"),
            capacities: Vec::new(),
            rigid: true,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind: SpawnPocketKindV1::Container,
                max_contains_volume_milliliters: u64::MAX,
                magazine_well_volume_milliliters: 0,
                contents_collapsed_by_default: false,
                max_contains_weight_milligrams: u64::MAX,
                max_item_volume_milliliters: u64::MAX,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: u64::MAX,
                item_restrictions: Vec::new(),
                flag_restrictions: Vec::new(),
                access_moves: 100,
                rigid: true,
                watertight: false,
                transparent: false,
                forbidden: false,
                sealable: false,
            }),
        }];
        ItemGroupContainerV1 {
            item,
            variant_id: None,
            sealed: false,
            overflow,
        }
    }

    fn worldgen_test_terrain(terrain_id: &str) -> TerrainTileSnapshot {
        TerrainTileSnapshot {
            terrain_id: terrain_id.to_owned(),
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
    }

    fn worldgen_test_item_groups() -> Vec<ItemGroupDefinitionV1> {
        vec![ItemGroupDefinitionV1 {
            group_id: String::from("field_loot"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: Vec::new(),
                }],
                wrapper: None,
            },
        }]
    }

    fn worldgen_test_catalog() -> WorldgenCatalogV1 {
        let cell = WorldgenCellV1 {
            terrain: vec![vec![
                WorldgenWeightedTerrainTargetV1 {
                    target: WorldgenTerrainTargetV1::Prototype(0),
                    weight: 3,
                },
                WorldgenWeightedTerrainTargetV1 {
                    target: WorldgenTerrainTargetV1::Regional(0),
                    weight: 1,
                },
            ]],
            furniture: vec![vec![
                WorldgenWeightedFurnitureTargetV1 {
                    target: WorldgenFurnitureTargetV1::None,
                    weight: 3,
                },
                WorldgenWeightedFurnitureTargetV1 {
                    target: WorldgenFurnitureTargetV1::Regional(0),
                    weight: 1,
                },
            ]],
            item_group: None,
        };
        let mut cells = vec![cell; WORLDGEN_CELLS_PER_OMT];
        cells[0].item_group = Some(WorldgenItemGroupPlacementV1 {
            group_id: String::from("field_loot"),
            chance: 25,
            repeat_minimum: 1,
            repeat_maximum: 1,
        });
        WorldgenCatalogV1 {
            generator_version: WORLDGEN_GENERATOR_VERSION_V2,
            overmap: WorldgenOvermapLayoutV1 {
                origin_x: -90,
                origin_y: -90,
                identities: vec![WorldgenOmtIdentityV1 {
                    full_id: String::from("field"),
                    type_id: String::from("field"),
                    subtype_id: String::from("field"),
                    generator_id: String::from("field"),
                    rotation: 0,
                }],
                layers: vec![WorldgenOvermapLayerV1 {
                    z: 0,
                    runs: vec![WorldgenOvermapRunV1 {
                        identity_index: 0,
                        length: u32::from(WORLDGEN_OVERMAP_WIDTH)
                            * u32::from(WORLDGEN_OVERMAP_HEIGHT),
                    }],
                }],
            },
            cities: Vec::new(),
            rivers: Vec::new(),
            specials: Vec::new(),
            start_location: Some(WorldgenStartLocationV1 {
                start_location_id: String::from("sloc_field"),
                targets: vec![WorldgenStartTargetV1 {
                    omt: String::from("field"),
                    match_type: WorldgenOmtMatchTypeV1::Type,
                }],
                city_sizes: WorldgenI32IntervalV1 {
                    minimum: 0,
                    maximum: i32::MAX,
                },
                city_distance: WorldgenI32IntervalV1 {
                    minimum: 0,
                    maximum: i32::MAX,
                },
            }),
            terrain_prototypes: vec![
                worldgen_test_terrain("t_floor"),
                worldgen_test_terrain("t_grass"),
            ],
            furniture_prototypes: vec![FurnitureTileSnapshot {
                furniture_id: String::from("f_chair"),
                move_cost_mod: 0,
                transparent: true,
                blocks_door: false,
                comfort: 1,
                floor_bedding_warmth: 0,
            }],
            monster_prototypes: Vec::new(),
            monster_groups: Vec::new(),
            regional_terrain: vec![WorldgenRegionalTerrainTableV1 {
                regional_id: String::from("region_groundcover"),
                choices: vec![
                    WorldgenWeightedPrototypeV1 {
                        prototype_index: 0,
                        weight: 3,
                    },
                    WorldgenWeightedPrototypeV1 {
                        prototype_index: 1,
                        weight: 1,
                    },
                ],
            }],
            regional_furniture: vec![WorldgenRegionalFurnitureTableV1 {
                regional_id: String::from("region_furniture"),
                choices: vec![
                    WorldgenWeightedFurniturePrototypeV1 {
                        target: WorldgenFurniturePrototypeTargetV1::None,
                        weight: 3,
                    },
                    WorldgenWeightedFurniturePrototypeV1 {
                        target: WorldgenFurniturePrototypeTargetV1::Prototype(0),
                        weight: 1,
                    },
                ],
            }],
            npc_name_categories: Vec::new(),
            omt_generators: vec![WorldgenOmtGeneratorV1 {
                omt_id: String::from("field"),
                templates: vec![WorldgenTemplateV1 {
                    weight: 1_000,
                    predecessor_id: None,
                    builtin: None,
                    cells,
                    nested: Vec::new(),
                    area_items: Vec::new(),
                    npc_placements: Vec::new(),
                    monster_placements: Vec::new(),
                    individual_monster_placements: Vec::new(),
                    erase_all_before_placing_terrain: false,
                    deferred_fields: Vec::new(),
                }],
                nested_generators: Vec::new(),
            }],
        }
    }

    #[test]
    fn city_identity_constraints_and_exact_start_distances_are_canonical() {
        let city = WorldgenCityV1 {
            city_id: WorldgenCityId(1),
            center: ChunkCoord { x: 0, y: 0, z: 0 },
            size: 8,
        };
        assert_eq!(worldgen_city_start_distance(&city, city.center), -8);
        assert_eq!(
            worldgen_city_start_distance(&city, ChunkCoord { x: 8, y: 0, z: 0 }),
            -8
        );
        assert_eq!(
            worldgen_city_start_distance(&city, ChunkCoord { x: 8, y: 8, z: 0 }),
            -5
        );
        assert_eq!(
            worldgen_city_start_distance(&city, ChunkCoord { x: 16, y: 0, z: 0 }),
            0
        );

        let mut catalog = worldgen_test_catalog();
        catalog.cities.push(city);
        let start = catalog.start_location.as_mut().expect("start");
        start.city_sizes = WorldgenI32IntervalV1 {
            minimum: 8,
            maximum: 8,
        };
        start.city_distance = WorldgenI32IntervalV1 {
            minimum: -8,
            maximum: -5,
        };
        assert!(worldgen_catalog_shape_is_valid(&catalog));

        catalog.cities[0].city_id = WorldgenCityId(2);
        assert!(!worldgen_catalog_shape_is_valid(&catalog));
    }

    #[test]
    fn worldgen_catalog_round_trips_with_independent_regional_targets() {
        let catalog = worldgen_test_catalog();
        let item_groups = worldgen_test_item_groups();
        assert_eq!(WORLDGEN_OMT_SIZE, 24);
        assert_eq!(WORLDGEN_CELLS_PER_OMT, 576);
        assert!(worldgen_catalog_shape_is_valid(&catalog));
        assert!(worldgen_catalog_is_valid(&catalog, &item_groups));

        let encoded = postcard::to_stdvec(&catalog).expect("worldgen catalog should encode");
        let decoded: WorldgenCatalogV1 =
            postcard::from_bytes(&encoded).expect("worldgen catalog should decode");
        assert_eq!(decoded, catalog);
    }

    #[test]
    fn regional_graph_validation_is_bounded_for_shared_dags_and_cycles() {
        let bounded_chain = (0..MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH)
            .map(|index| {
                (index + 1 < MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH)
                    .then_some(index + 1)
                    .into_iter()
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert!(valid_worldgen_regional_graph(&bounded_chain));

        let mut over_depth = bounded_chain.clone();
        over_depth.push(Vec::new());
        over_depth[MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH - 1]
            .push(MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH);
        assert!(!valid_worldgen_regional_graph(&over_depth));

        let layer_width = MAX_WORLDGEN_REGIONAL_TABLES / MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH;
        let shared_dag = (0..MAX_WORLDGEN_REGIONAL_TABLES)
            .map(|index| {
                let next_layer = index / layer_width + 1;
                if next_layer < MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH {
                    let start = next_layer * layer_width;
                    (start..start + layer_width).collect::<Vec<_>>()
                } else {
                    Vec::new()
                }
            })
            .collect::<Vec<_>>();
        assert!(
            valid_worldgen_regional_graph(&shared_dag),
            "the dense layered DAG must validate in linear graph time"
        );

        let mut cycle = vec![vec![1], vec![2], vec![0]];
        assert!(!valid_worldgen_regional_graph(&cycle));
        cycle[2].clear();
        assert!(valid_worldgen_regional_graph(&cycle));
    }

    #[test]
    fn coordinate_owned_overmap_layout_routes_exact_cells_and_fails_closed() {
        let mut catalog = worldgen_test_catalog();
        catalog.overmap.identities = vec![
            WorldgenOmtIdentityV1 {
                full_id: String::from("field_a"),
                type_id: String::from("field"),
                subtype_id: String::from("field"),
                generator_id: String::from("field"),
                rotation: 0,
            },
            WorldgenOmtIdentityV1 {
                full_id: String::from("field_b"),
                type_id: String::from("field"),
                subtype_id: String::from("field"),
                generator_id: String::from("field"),
                rotation: 1,
            },
        ];
        catalog.overmap.layers[0].runs = vec![
            WorldgenOvermapRunV1 {
                identity_index: 0,
                length: 1,
            },
            WorldgenOvermapRunV1 {
                identity_index: 1,
                length: u32::from(WORLDGEN_OVERMAP_WIDTH) * u32::from(WORLDGEN_OVERMAP_HEIGHT) - 1,
            },
        ];
        assert!(worldgen_catalog_shape_is_valid(&catalog));
        assert_eq!(
            worldgen_omt_identity_at(
                &catalog,
                ChunkCoord {
                    x: -90,
                    y: -90,
                    z: 0,
                },
            )
            .map(|identity| identity.full_id.as_str()),
            Some("field_a")
        );
        assert_eq!(
            worldgen_omt_identity_at(
                &catalog,
                ChunkCoord {
                    x: -89,
                    y: -90,
                    z: 0,
                },
            )
            .map(|identity| (identity.full_id.as_str(), identity.rotation)),
            Some(("field_b", 1))
        );
        for outside in [
            ChunkCoord {
                x: -91,
                y: -90,
                z: 0,
            },
            ChunkCoord { x: 90, y: 89, z: 0 },
            ChunkCoord {
                x: -90,
                y: -90,
                z: 1,
            },
        ] {
            assert!(worldgen_omt_identity_at(&catalog, outside).is_none());
        }
    }

    #[test]
    fn worldgen_catalog_rejects_noncanonical_and_out_of_bounds_payloads() {
        let item_groups = worldgen_test_item_groups();

        let mut invalid = worldgen_test_catalog();
        invalid.generator_version += 1;
        assert!(!worldgen_catalog_is_valid(&invalid, &item_groups));

        let mut invalid = worldgen_test_catalog();
        invalid.omt_generators[0].templates[0].cells[0].furniture[0][0].weight = 0;
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.overmap.identities[0].generator_id = String::from("missing");
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        let mut predecessor = invalid.omt_generators[0].clone();
        predecessor.omt_id = String::from("predecessor");
        invalid.omt_generators[0].templates[0].predecessor_id = Some(predecessor.omt_id.clone());
        invalid.omt_generators.push(predecessor);
        assert!(
            !worldgen_catalog_shape_is_valid(&invalid),
            "a predecessor needs a concrete identity carrying its own rotation"
        );

        let mut invalid = worldgen_test_catalog();
        invalid.overmap.layers[0].runs[0].length -= 1;
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.overmap.layers[0].runs[0].length -= 1;
        invalid.overmap.layers[0].runs.push(WorldgenOvermapRunV1 {
            identity_index: 0,
            length: 1,
        });
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.overmap.layers[0].z = 1;
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.overmap.identities.push(WorldgenOmtIdentityV1 {
            full_id: String::from("unused"),
            type_id: String::from("unused"),
            subtype_id: String::from("unused"),
            generator_id: String::from("field"),
            rotation: 0,
        });
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid
            .start_location
            .as_mut()
            .expect("fixture has a start location")
            .targets[0]
            .omt = String::from("forest");
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid
            .start_location
            .as_mut()
            .expect("fixture has a start location")
            .targets
            .push(WorldgenStartTargetV1 {
                omt: String::from("forest"),
                match_type: WorldgenOmtMatchTypeV1::Type,
            });
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.terrain_prototypes.swap(0, 1);
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid
            .regional_terrain
            .push(invalid.regional_terrain[0].clone());
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.omt_generators[0].templates[0].cells.pop();
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.omt_generators[0].templates[0].cells[0].terrain[0][0].target =
            WorldgenTerrainTargetV1::Prototype(u16::MAX);
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut invalid = worldgen_test_catalog();
        invalid.omt_generators[0].templates[0].cells[0]
            .item_group
            .as_mut()
            .expect("fixture has a placement")
            .chance = 0;
        assert!(!worldgen_catalog_shape_is_valid(&invalid));

        let mut missing_group = worldgen_test_catalog();
        missing_group.omt_generators[0].templates[0].cells[0]
            .item_group
            .as_mut()
            .expect("fixture has a placement")
            .group_id = String::from("absent");
        assert!(worldgen_catalog_shape_is_valid(&missing_group));
        assert!(!worldgen_catalog_is_valid(&missing_group, &item_groups));

        let mut too_many_choices = worldgen_test_catalog();
        too_many_choices.regional_terrain[0].choices = vec![
            WorldgenWeightedPrototypeV1 {
                prototype_index: 0,
                weight: 1,
            };
            MAX_WORLDGEN_REGIONAL_CHOICES + 1
        ];
        assert!(!worldgen_catalog_shape_is_valid(&too_many_choices));
    }

    #[test]
    fn worldgen_catalog_checks_every_cumulative_weight_sum() {
        let mut regional_overflow = worldgen_test_catalog();
        regional_overflow.regional_terrain[0].choices = vec![
            WorldgenWeightedPrototypeV1 {
                prototype_index: 0,
                weight: u32::MAX,
            },
            WorldgenWeightedPrototypeV1 {
                prototype_index: 1,
                weight: 1,
            },
        ];
        assert!(!worldgen_catalog_shape_is_valid(&regional_overflow));

        let mut cell_overflow = worldgen_test_catalog();
        cell_overflow.omt_generators[0].templates[0].cells[0].terrain = vec![vec![
            WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(0),
                weight: u32::MAX,
            },
            WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(1),
                weight: 1,
            },
        ]];
        assert!(!worldgen_catalog_shape_is_valid(&cell_overflow));

        let mut template_overflow = worldgen_test_catalog();
        let mut second = template_overflow.omt_generators[0].templates[0].clone();
        template_overflow.omt_generators[0].templates[0].weight = u32::MAX;
        second.weight = 1;
        template_overflow.omt_generators[0].templates.push(second);
        assert!(!worldgen_catalog_shape_is_valid(&template_overflow));
    }

    #[test]
    fn overmap_terrain_matching_preserves_every_pinned_mode() {
        let ordinary = WorldgenOmtIdentityV1 {
            full_id: String::from("forest_thick_north"),
            type_id: String::from("forest_thick"),
            subtype_id: String::from("forest_thick"),
            generator_id: String::from("forest_thick"),
            rotation: 0,
        };
        assert!(worldgen_omt_matches(
            "forest_thick_north",
            WorldgenOmtMatchTypeV1::Exact,
            &ordinary
        ));
        assert!(!worldgen_omt_matches(
            "forest_thick",
            WorldgenOmtMatchTypeV1::Exact,
            &ordinary
        ));
        assert!(worldgen_omt_matches(
            "forest_thick",
            WorldgenOmtMatchTypeV1::Type,
            &ordinary
        ));
        assert!(worldgen_omt_matches(
            "forest",
            WorldgenOmtMatchTypeV1::Prefix,
            &ordinary
        ));
        assert!(!worldgen_omt_matches(
            "fores",
            WorldgenOmtMatchTypeV1::Prefix,
            &ordinary
        ));
        assert!(worldgen_omt_matches(
            "thick_n",
            WorldgenOmtMatchTypeV1::Contains,
            &ordinary
        ));

        let linear = WorldgenOmtIdentityV1 {
            full_id: String::from("road_ne"),
            type_id: String::from("road"),
            subtype_id: String::from("road_curved"),
            generator_id: String::from("road_curved"),
            rotation: 3,
        };
        assert!(worldgen_omt_matches(
            "road",
            WorldgenOmtMatchTypeV1::Type,
            &linear
        ));
        assert!(worldgen_omt_matches(
            "road_curved",
            WorldgenOmtMatchTypeV1::Subtype,
            &linear
        ));
        assert!(!worldgen_omt_matches(
            "road_straight",
            WorldgenOmtMatchTypeV1::Subtype,
            &linear
        ));
    }

    fn item_group_chain(nodes: usize) -> ItemGroupDefinitionV1 {
        let nodes = (0..nodes)
            .map(|index| ItemGroupNodeV1 {
                node_id: u16::try_from(index).expect("test graph is small"),
                kind: ItemGroupKindV1::Collection,
                entries: vec![item_group_entry(
                    100,
                    1,
                    1,
                    if index + 1 == nodes {
                        item_group_item("chain_leaf")
                    } else {
                        ItemGroupTargetV1::Node(
                            u16::try_from(index + 1).expect("test graph is small"),
                        )
                    },
                )],
            })
            .collect();
        ItemGroupDefinitionV1 {
            group_id: String::from("chain"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes,
                wrapper: None,
            },
        }
    }

    #[test]
    fn item_group_graph_round_trips_and_computes_recursive_output_bound() {
        let mut charged_leaf = item_group_item("nail");
        let ItemGroupTargetV1::Item(item) = &mut charged_leaf else {
            unreachable!("fixture is a direct item")
        };
        item.charges = Some(ItemGroupChargeRangeV1 {
            minimum: 0,
            maximum: 12,
        });
        item.minimum_one_charge = true;
        let leaf = ItemGroupDefinitionV1 {
            group_id: String::from("a_leaf"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Distribution,
                    entries: vec![item_group_modifier_entry(250, 1, 3, charged_leaf)],
                }],
                wrapper: None,
            },
        };
        let root = ItemGroupDefinitionV1 {
            group_id: String::from("b_root"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![
                    ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![
                            item_group_entry(
                                100,
                                1,
                                1,
                                ItemGroupTargetV1::Group(String::from("a_leaf")),
                            ),
                            item_group_entry(
                                100,
                                1,
                                1,
                                ItemGroupTargetV1::Group(String::from("a_leaf")),
                            ),
                            item_group_entry(25, 1, 1, ItemGroupTargetV1::Node(1)),
                        ],
                    },
                    ItemGroupNodeV1 {
                        node_id: 1,
                        kind: ItemGroupKindV1::Distribution,
                        entries: vec![item_group_entry(1, 1, 1, item_group_item("splinter"))],
                    },
                ],
                wrapper: None,
            },
        };
        let holidays = [
            ItemGroupEventV1::NewYear,
            ItemGroupEventV1::Easter,
            ItemGroupEventV1::IndependenceDay,
            ItemGroupEventV1::Halloween,
            ItemGroupEventV1::Thanksgiving,
            ItemGroupEventV1::Christmas,
        ];
        let holiday_group = ItemGroupDefinitionV1 {
            group_id: String::from("c_holidays"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Distribution,
                    entries: holidays
                        .into_iter()
                        .map(|event| {
                            let mut entry =
                                item_group_entry(1, 1, 1, item_group_item("holiday_token"));
                            entry.event = Some(event);
                            entry
                        })
                        .collect(),
                }],
                wrapper: None,
            },
        };
        let catalog = vec![leaf, root, holiday_group];
        assert!(item_group_catalog_is_valid(&catalog));
        assert_eq!(
            item_group_source_max_outputs(
                &ItemGroupSourceV1::Group(String::from("b_root")),
                &catalog,
            ),
            Some(7)
        );
        let inline = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 4,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 4,
                kind: ItemGroupKindV1::Distribution,
                entries: vec![item_group_entry(
                    1,
                    1,
                    1,
                    ItemGroupTargetV1::Group(String::from("b_root")),
                )],
            }],
            wrapper: None,
        });
        assert_eq!(item_group_source_max_outputs(&inline, &catalog), Some(7));

        let encoded = postcard::to_stdvec(&(catalog.clone(), inline.clone()))
            .expect("bounded item-group graph should encode");
        let decoded: (Vec<ItemGroupDefinitionV1>, ItemGroupSourceV1) =
            postcard::from_bytes(&encoded).expect("bounded item-group graph should decode");
        assert_eq!(decoded, (catalog, inline));
    }

    #[test]
    fn item_group_variables_cannot_override_unprojected_physical_dimensions() {
        for reserved in ["weight", "integral_weight", "volume"] {
            let mut target = item_group_item("reserved_variable");
            let ItemGroupTargetV1::Item(item) = &mut target else {
                unreachable!("fixture is a direct item")
            };
            item.initial_variables
                .insert(reserved.to_owned(), ItemVariableValueV1::Integer(1));
            let catalog = vec![ItemGroupDefinitionV1 {
                group_id: String::from("reserved_variable_group"),
                graph: ItemGroupGraphV1 {
                    root_node: 0,
                    nodes: vec![ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![item_group_entry(100, 1, 1, target)],
                    }],
                    wrapper: None,
                },
            }];
            assert!(
                !item_group_catalog_is_valid(&catalog),
                "reserved variable {reserved} must fail closed"
            );
        }
    }

    #[test]
    fn item_group_validation_rejects_cycles_missing_refs_and_invalid_ranges() {
        let local_cycle = ItemGroupDefinitionV1 {
            group_id: String::from("local_cycle"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, ItemGroupTargetV1::Node(0))],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(&[local_cycle]));

        let named_cycle = |group_id: &str, target: &str| ItemGroupDefinitionV1 {
            group_id: group_id.to_owned(),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Distribution,
                    entries: vec![item_group_entry(
                        1,
                        1,
                        1,
                        ItemGroupTargetV1::Group(target.to_owned()),
                    )],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(&[
            named_cycle("a", "b"),
            named_cycle("b", "a"),
        ]));
        assert!(!item_group_catalog_is_valid(&[named_cycle(
            "missing", "absent",
        )]));

        let mut invalid_charges = item_group_item("nail");
        let ItemGroupTargetV1::Item(item) = &mut invalid_charges else {
            unreachable!("fixture is a direct item")
        };
        item.charges = Some(ItemGroupChargeRangeV1 {
            minimum: -2,
            maximum: 3,
        });
        let invalid_charges = ItemGroupDefinitionV1 {
            group_id: String::from("invalid_charges"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, invalid_charges)],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(&[invalid_charges]));

        let mut fixed_zero_damage = ItemGroupDefinitionV1 {
            group_id: String::from("fixed_zero_damage"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, item_group_item("rock"))],
                }],
                wrapper: None,
            },
        };
        fixed_zero_damage.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        assert!(item_group_catalog_is_valid(&[fixed_zero_damage.clone()]));
        fixed_zero_damage.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: MAX_ITEM_RAW_DAMAGE + 1,
        });
        assert!(!item_group_catalog_is_valid(&[fixed_zero_damage]));

        let direct_item_definition = |group_id: &str, target| ItemGroupDefinitionV1 {
            group_id: group_id.to_owned(),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, target)],
                }],
                wrapper: None,
            },
        };
        let mut exact_damage_cap = item_group_item("damageable");
        let ItemGroupTargetV1::Item(item) = &mut exact_damage_cap else {
            unreachable!("fixture is a direct item")
        };
        item.maximum_raw_damage = MAX_ITEM_RAW_DAMAGE;
        assert!(item_group_catalog_is_valid(&[direct_item_definition(
            "exact_damage_cap",
            exact_damage_cap.clone(),
        )]));
        let ItemGroupTargetV1::Item(item) = &mut exact_damage_cap else {
            unreachable!("fixture is a direct item")
        };
        item.maximum_raw_damage = 123;
        assert!(
            !item_group_catalog_is_valid(&[direct_item_definition(
                "invented_damage_cap",
                exact_damage_cap,
            )]),
            "upstream item damage caps are exactly zero or the global maximum"
        );

        let variant = |id: &str, weight| ItemGroupVariantOptionV1 {
            variant: ItemVariantV1 {
                id: id.to_owned(),
                name: format!("{id} name"),
                description: String::new(),
                symbol: String::new(),
                color: String::new(),
                ascii_picture: String::new(),
            },
            weight,
            description_expansion: None,
        };
        assert!(
            !item_variant_is_valid(&variant("<any>", 1).variant),
            "the modifier sentinel is not a selectable variant identity"
        );
        let mut weighted_variants = item_group_item("variant_item");
        let ItemGroupTargetV1::Item(item) = &mut weighted_variants else {
            unreachable!("fixture is a direct item")
        };
        item.variants = vec![variant("maximum", i32::MAX as u32), variant("zero", 0)];
        assert!(item_group_catalog_is_valid(&[direct_item_definition(
            "maximum_variant_weight",
            weighted_variants.clone(),
        )]));
        let ItemGroupTargetV1::Item(item) = &mut weighted_variants else {
            unreachable!("fixture is a direct item")
        };
        item.variants[1].weight = 1;
        assert!(
            !item_group_catalog_is_valid(&[direct_item_definition(
                "overflowed_variant_weight",
                weighted_variants,
            )]),
            "the pinned signed-int constructor weight sum must not overflow"
        );

        let mut maximum_variants = item_group_item("maximum_variants");
        let ItemGroupTargetV1::Item(item) = &mut maximum_variants else {
            unreachable!("fixture is a direct item")
        };
        item.variants = (0..MAX_ITEM_VARIANTS)
            .map(|index| variant(&format!("variant_{index}"), 1))
            .collect();
        assert!(item_group_catalog_is_valid(&[direct_item_definition(
            "maximum_unique_variants",
            maximum_variants.clone(),
        )]));
        let ItemGroupTargetV1::Item(item) = &mut maximum_variants else {
            unreachable!("fixture is a direct item")
        };
        item.variants[MAX_ITEM_VARIANTS - 1].variant.id = String::from("variant_0");
        assert!(
            !item_group_catalog_is_valid(&[direct_item_definition(
                "duplicate_maximum_variants",
                maximum_variants,
            )]),
            "duplicate IDs must reject at the maximum bounded shape"
        );

        let mut missing_count_marker = ItemGroupDefinitionV1 {
            group_id: String::from("missing_count_marker"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 2, item_group_item("rock"))],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(
            &[missing_count_marker.clone()]
        ));
        missing_count_marker.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        assert!(item_group_catalog_is_valid(&[missing_count_marker]));

        let mut missing_charge_marker = item_group_item("nail");
        let ItemGroupTargetV1::Item(item) = &mut missing_charge_marker else {
            unreachable!("fixture is a direct item")
        };
        item.charges = Some(ItemGroupChargeRangeV1 {
            minimum: 4,
            maximum: 16,
        });
        let mut missing_charge_marker = ItemGroupDefinitionV1 {
            group_id: String::from("missing_charge_marker"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, missing_charge_marker)],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(&[
            missing_charge_marker.clone()
        ]));
        missing_charge_marker.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        assert!(item_group_catalog_is_valid(&[missing_charge_marker]));

        let null_item = ItemGroupDefinitionV1 {
            group_id: String::from("null_item"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, item_group_item("null"))],
                }],
                wrapper: None,
            },
        };
        assert!(
            !item_group_catalog_is_valid(&[null_item]),
            "upstream discards concrete null leaves rather than materializing an item"
        );

        let mut local_modifier = ItemGroupDefinitionV1 {
            group_id: String::from("local_modifier"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![
                    ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![item_group_entry(100, 1, 1, ItemGroupTargetV1::Node(1))],
                    },
                    ItemGroupNodeV1 {
                        node_id: 1,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![item_group_entry(100, 1, 1, item_group_item("rock"))],
                    },
                ],
                wrapper: None,
            },
        };
        assert!(item_group_catalog_is_valid(&[local_modifier.clone()]));
        local_modifier.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        assert!(
            !item_group_catalog_is_valid(&[local_modifier]),
            "pinned local group objects return before leaf modifiers are parsed"
        );

        let mut named_modifier = named_cycle("named_modifier", "named_target");
        named_modifier.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        });
        let named_target = ItemGroupDefinitionV1 {
            group_id: String::from("named_target"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, item_group_item("rock"))],
                }],
                wrapper: None,
            },
        };
        assert!(
            item_group_catalog_is_valid(&[named_modifier.clone(), named_target.clone()]),
            "named modifiers are applied to each completed child output"
        );
        let mut unsafe_named_target = named_target;
        let ItemGroupTargetV1::Item(leaf) =
            &mut unsafe_named_target.graph.nodes[0].entries[0].target
        else {
            unreachable!("fixture is a direct item")
        };
        leaf.modifier_side_effects_supported = false;
        assert!(
            !item_group_catalog_is_valid(&[named_modifier, unsafe_named_target]),
            "named modifiers must not reach leaves with unrepresented side effects"
        );

        let charged_food_group = |minimum_one_charge| {
            let mut target = item_group_item("food");
            let ItemGroupTargetV1::Item(item) = &mut target else {
                unreachable!("fixture is a direct item")
            };
            item.prototype.comestible_type = String::from("FOOD");
            item.charges = Some(ItemGroupChargeRangeV1 {
                minimum: 0,
                maximum: 0,
            });
            item.minimum_one_charge = minimum_one_charge;
            let mut definition = ItemGroupDefinitionV1 {
                group_id: String::from("charged_food"),
                graph: ItemGroupGraphV1 {
                    root_node: 0,
                    nodes: vec![ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![item_group_entry(100, 1, 1, target)],
                    }],
                    wrapper: None,
                },
            };
            definition.graph.nodes[0].entries[0].raw_damage = Some(InclusiveU16RangeV1 {
                minimum: 0,
                maximum: 0,
            });
            definition
        };
        assert!(item_group_catalog_is_valid(&[charged_food_group(true)]));
        assert!(!item_group_catalog_is_valid(&[charged_food_group(false)]));

        let mut overfilled_magazine = item_group_item("magazine");
        let ItemGroupTargetV1::Item(item) = &mut overfilled_magazine else {
            unreachable!("fixture is a direct item")
        };
        item.prototype.ammunition_type = String::from("9mm");
        item.prototype.magazine_capacity = 10;
        item.charges = Some(ItemGroupChargeRangeV1 {
            minimum: 0,
            maximum: 11,
        });
        let overfilled_magazine = ItemGroupDefinitionV1 {
            group_id: String::from("overfilled_magazine"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![item_group_entry(100, 1, 1, overfilled_magazine)],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(&[overfilled_magazine]));

        let overflowed_weights = ItemGroupDefinitionV1 {
            group_id: String::from("weights"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Distribution,
                    entries: vec![
                        item_group_entry(u32::MAX, 1, 1, item_group_item("rock")),
                        item_group_entry(1, 1, 1, item_group_item("stick")),
                    ],
                }],
                wrapper: None,
            },
        };
        assert!(!item_group_catalog_is_valid(&[overflowed_weights]));
    }

    #[test]
    fn named_item_group_metrics_follow_charge_candidates_and_result_wrappers() {
        let definition = |group_id: &str, entry: ItemGroupEntryV1| ItemGroupDefinitionV1 {
            group_id: group_id.to_owned(),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![entry],
                }],
                wrapper: None,
            },
        };

        let mut charged_tool = item_group_item("charged_tool");
        let ItemGroupTargetV1::Item(tool) = &mut charged_tool else {
            unreachable!("fixture is a direct item")
        };
        tool.prototype.charges = 0;
        tool.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
        tool.prototype.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("BATTERY"),
            ammunition_type: String::from("battery"),
            capacity: 5,
            rigid: true,
            reloadable: false,
            unloadable: false,
        }];
        let ItemGroupTargetV1::Item(ammunition) = item_group_item("battery") else {
            unreachable!("fixture is a direct item")
        };
        let mut ammunition = ammunition.prototype;
        ammunition.ammunition_type = String::from("battery");
        tool.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::Integral { ammunition });
        let charged_child =
            definition("a_charged_child", item_group_entry(100, 1, 1, charged_tool));
        let mut charge_modifier = item_group_modifier_entry(
            100,
            1,
            1,
            ItemGroupTargetV1::Group(String::from("a_charged_child")),
        );
        charge_modifier.modifier_charges = Some(ItemGroupChargeRangeV1 {
            minimum: 2,
            maximum: 2,
        });
        let charge_parent = definition("b_charge_parent", charge_modifier);
        let charge_catalog = vec![charged_child, charge_parent];
        let charge_source = ItemGroupSourceV1::Group(String::from("b_charge_parent"));
        assert_eq!(
            item_groups::item_group_source_metrics_for_test(&charge_source, &charge_catalog),
            Some((2, 1, 1)),
            "a parent charge modifier materializes one nested ammunition object"
        );
        let malformed_charge_catalog = |mutate: fn(&mut CraftItemPrototypeV1)| {
            let mut malformed = charge_catalog[0].clone();
            let ItemGroupTargetV1::Item(tool) = &mut malformed.graph.nodes[0].entries[0].target
            else {
                unreachable!("fixture is a direct item")
            };
            let Some(ItemGroupToolChargeStorageV1::Integral { ammunition }) =
                tool.tool_charge_storage.as_mut()
            else {
                unreachable!("integral charge storage exists")
            };
            mutate(ammunition);
            malformed
        };
        assert!(!item_group_catalog_is_valid(&[malformed_charge_catalog(
            |ammunition| ammunition.ammunition_type = String::from("plutonium"),
        )]));
        assert!(!item_group_catalog_is_valid(&[malformed_charge_catalog(
            |ammunition| ammunition.magazine_capacity = 1,
        )]));

        let mut unsupported_payload = item_group_item("unsupported_payload");
        let ItemGroupTargetV1::Item(payload) = &mut unsupported_payload else {
            unreachable!("fixture is a direct item")
        };
        payload.modifier_side_effects_supported = false;
        payload.charges_supported = false;
        let mut wrapped_entry = item_group_entry(100, 1, 1, unsupported_payload.clone());
        wrapped_entry.direct_wrapper = Some(item_group_wrapper(
            "safe_direct_case",
            ItemGroupOverflowV1::None,
        ));
        let wrapped_child = definition("a_wrapped_child", wrapped_entry.clone());
        let wrapped_parent = definition(
            "b_wrapped_parent",
            item_group_modifier_entry(
                100,
                1,
                1,
                ItemGroupTargetV1::Group(String::from("a_wrapped_child")),
            ),
        );
        assert!(
            item_group_catalog_is_valid(&[wrapped_child.clone(), wrapped_parent.clone()]),
            "a non-spill direct wrapper replaces the modified top-level item"
        );

        wrapped_entry
            .direct_wrapper
            .as_mut()
            .expect("direct wrapper exists")
            .overflow = ItemGroupOverflowV1::Spill;
        let spill_child = definition("a_wrapped_child", wrapped_entry);
        assert!(
            !item_group_catalog_is_valid(&[spill_child, wrapped_parent.clone()]),
            "spill results retain the unsupported payload beside the safe wrapper"
        );

        let mut unsafe_wrapper =
            item_group_wrapper("unsafe_modifier_case", ItemGroupOverflowV1::None);
        unsafe_wrapper.item.modifier_side_effects_supported = false;
        let mut modifier_wrapped_entry =
            item_group_modifier_entry(100, 1, 1, item_group_item("safe_payload"));
        modifier_wrapped_entry.modifier_container = Some(unsafe_wrapper);
        let modifier_wrapped_child = definition("a_wrapped_child", modifier_wrapped_entry);
        assert!(
            !item_group_catalog_is_valid(&[modifier_wrapped_child, wrapped_parent]),
            "a modifier container becomes the completed top-level child"
        );

        let mut charge_unsafe_wrapper =
            item_group_wrapper("charge_unsafe_case", ItemGroupOverflowV1::None);
        charge_unsafe_wrapper.item.charges_supported = false;
        let mut graph_wrapped_child = definition(
            "a_graph_wrapped_child",
            item_group_entry(100, 1, 1, item_group_item("charge_safe_payload")),
        );
        graph_wrapped_child.graph.wrapper = Some(charge_unsafe_wrapper);
        let mut graph_charge_modifier = item_group_modifier_entry(
            100,
            1,
            1,
            ItemGroupTargetV1::Group(String::from("a_graph_wrapped_child")),
        );
        graph_charge_modifier.modifier_charges = Some(ItemGroupChargeRangeV1 {
            minimum: 1,
            maximum: 1,
        });
        let graph_charge_parent = definition("b_graph_charge_parent", graph_charge_modifier);
        assert!(
            !item_group_catalog_is_valid(&[graph_wrapped_child, graph_charge_parent]),
            "a whole-group wrapper supplies the authoritative charge capability"
        );

        let mut ignored_direct_charge =
            item_group_modifier_entry(100, 1, 1, item_group_item("direct_charge_target"));
        ignored_direct_charge.modifier_charges = Some(ItemGroupChargeRangeV1 {
            minimum: 1,
            maximum: 1,
        });
        assert!(
            !item_group_catalog_is_valid(&[definition(
                "ignored_direct_charge",
                ignored_direct_charge,
            )]),
            "direct charges belong on the item prototype and cannot be silently ignored"
        );
    }

    #[test]
    fn item_group_contents_projection_fails_closed_without_rejecting_true_no_pocket_items() {
        let definition = |target, contents: ItemGroupItemPrototypeV1| ItemGroupDefinitionV1 {
            group_id: String::from("contents_projection"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![ItemGroupEntryV1 {
                        contents: vec![ItemGroupContentsSourceV1::Item(Box::new(contents))],
                        ..item_group_modifier_entry(100, 1, 1, target)
                    }],
                }],
                wrapper: None,
            },
        };
        let direct_item = |type_id| match item_group_item(type_id) {
            ItemGroupTargetV1::Item(item) => *item,
            ItemGroupTargetV1::Group(_) | ItemGroupTargetV1::Node(_) => {
                unreachable!("fixture is a direct item")
            }
        };

        let no_pocket = item_group_item("true_no_pocket");
        assert!(item_group_catalog_is_valid(&[definition(
            no_pocket,
            direct_item("payload"),
        )]));

        let mut lost_projection = item_group_item("lost_projection");
        let ItemGroupTargetV1::Item(item) = &mut lost_projection else {
            unreachable!("fixture is a direct item")
        };
        item.contents_insertion_supported = false;
        assert!(
            !item_group_catalog_is_valid(&[definition(lost_projection, direct_item("payload"),)]),
            "an empty strict projection cannot impersonate a true no-pocket item"
        );

        let mut ammunition_container = item_group_item("quiver");
        let ItemGroupTargetV1::Item(quiver) = &mut ammunition_container else {
            unreachable!("fixture is a direct item")
        };
        quiver.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("ARROWS"),
            capacities: vec![AmmunitionCapacityV1 {
                ammunition_type: String::from("arrow"),
                capacity: 20,
            }],
            rigid: true,
            access_moves: 100,
            reloadable: true,
            unloadable: true,
            spawn_rules: None,
        }];
        assert!(
            !item_group_catalog_is_valid(&[
                definition(ammunition_container, direct_item("arrow"),)
            ]),
            "non-estorable contents must not enter an unimplemented ammunition-container path"
        );

        let mut phone = item_group_item("phone");
        let ItemGroupTargetV1::Item(phone_item) = &mut phone else {
            unreachable!("fixture is a direct item")
        };
        phone_item.prototype.ammunition_containers = vec![
            AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("BATTERY"),
                capacities: vec![AmmunitionCapacityV1 {
                    ammunition_type: String::from("battery"),
                    capacity: 1,
                }],
                rigid: true,
                access_moves: 100,
                reloadable: true,
                unloadable: true,
                spawn_rules: None,
            },
            AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 1,
                pocket_id: String::from("EFILES"),
                capacities: Vec::new(),
                rigid: true,
                access_moves: 100,
                reloadable: false,
                unloadable: true,
                spawn_rules: Some(SpawnPocketRulesV1 {
                    kind: SpawnPocketKindV1::EFileStorage,
                    max_contains_volume_milliliters: u64::MAX,
                    magazine_well_volume_milliliters: 0,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: u64::MAX,
                    max_item_volume_milliliters: u64::MAX,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: u64::MAX,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 100,
                    rigid: true,
                    watertight: false,
                    transparent: false,
                    forbidden: false,
                    sealable: false,
                }),
            },
        ];
        let mut efile = direct_item("efile");
        efile.prototype.containment.estorable = true;
        assert!(valid_craft_item_prototype(&phone_item.prototype));
        assert!(valid_craft_item_prototype(&efile.prototype));
        let mut immutable_fit = efile.prototype.clone();
        immutable_fit.containment.flags = vec![String::from("FIT")];
        assert!(
            valid_craft_item_prototype(&immutable_fit),
            "prototype validation must synthesize the immutable FIT state"
        );
        assert!(
            item_group_catalog_is_valid(&[definition(phone.clone(), efile.clone())]),
            "estorable contents choose EFILE before the phone-like reload pocket"
        );
        let ItemGroupTargetV1::Item(non_rigid_phone) = &mut phone else {
            unreachable!("fixture is a direct item")
        };
        let non_rigid_pocket = &mut non_rigid_phone.prototype.ammunition_containers[1];
        non_rigid_pocket.rigid = false;
        non_rigid_pocket
            .spawn_rules
            .as_mut()
            .expect("EFILE spawn rules")
            .rigid = false;
        assert!(
            !item_group_catalog_is_valid(&[definition(phone, efile)]),
            "canonical catalogs must reject unattainable non-rigid EFILE pockets"
        );
    }

    #[test]
    fn item_group_output_depth_and_catalog_limits_are_exact() {
        let output_group = |count_max| ItemGroupDefinitionV1 {
            group_id: String::from("outputs"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![ItemGroupEntryV1 {
                        raw_damage: Some(InclusiveU16RangeV1 {
                            minimum: 0,
                            maximum: 0,
                        }),
                        modifier_charges: None,
                        contents: Vec::new(),
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                        ..item_group_entry(100, 0, count_max, item_group_item("rock"))
                    }],
                }],
                wrapper: None,
            },
        };
        assert!(item_group_catalog_is_valid(&[output_group(
            u16::try_from(MAX_ITEM_GROUP_OUTPUTS).expect("output bound fits u16"),
        )]));
        assert!(!item_group_catalog_is_valid(&[output_group(
            u16::try_from(MAX_ITEM_GROUP_OUTPUTS + 1).expect("test bound fits u16"),
        )]));

        let mut wrapped_outputs = output_group(2);
        let ItemGroupTargetV1::Item(mut wrapper_item) = item_group_item("counted_case") else {
            unreachable!("fixture is a direct item")
        };
        wrapper_item.prototype.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("CONTENTS"),
            capacities: Vec::new(),
            rigid: true,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind: SpawnPocketKindV1::Container,
                max_contains_volume_milliliters: u64::MAX,
                magazine_well_volume_milliliters: 0,
                contents_collapsed_by_default: false,
                max_contains_weight_milligrams: u64::MAX,
                max_item_volume_milliliters: u64::MAX,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: u64::MAX,
                item_restrictions: Vec::new(),
                flag_restrictions: Vec::new(),
                access_moves: 100,
                rigid: true,
                watertight: false,
                transparent: false,
                forbidden: false,
                sealable: false,
            }),
        }];
        wrapped_outputs.graph.nodes[0].entries[0].direct_wrapper = Some(ItemGroupContainerV1 {
            item: wrapper_item,
            variant_id: None,
            sealed: false,
            overflow: ItemGroupOverflowV1::Spill,
        });
        let wrapped_source = ItemGroupSourceV1::Inline(wrapped_outputs.graph);
        assert_eq!(
            item_group_source_max_outputs(&wrapped_source, &[]),
            Some(3),
            "two payloads plus one shared spill wrapper is the exact output bound"
        );

        assert!(item_group_catalog_is_valid(&[item_group_chain(
            MAX_ITEM_GROUP_DEPTH,
        )]));
        assert!(!item_group_catalog_is_valid(&[item_group_chain(
            MAX_ITEM_GROUP_DEPTH + 1,
        )]));

        let containment_catalog = |edges: usize| {
            (0..=edges)
                .map(|index| {
                    let target = item_group_item(&format!("carrier_{index:02}"));
                    let mut entry = item_group_modifier_entry(100, 1, 1, target);
                    if index < edges {
                        let rules = SpawnPocketRulesV1 {
                            kind: SpawnPocketKindV1::Container,
                            max_contains_volume_milliliters: u64::MAX,
                            magazine_well_volume_milliliters: 0,
                            contents_collapsed_by_default: false,
                            max_contains_weight_milligrams: u64::MAX,
                            max_item_volume_milliliters: u64::MAX,
                            min_item_volume_milliliters: 0,
                            max_item_length_millimeters: u64::MAX,
                            item_restrictions: Vec::new(),
                            flag_restrictions: Vec::new(),
                            access_moves: 100,
                            rigid: true,
                            watertight: false,
                            transparent: false,
                            forbidden: false,
                            sealable: false,
                        };
                        let ItemGroupTargetV1::Item(item) = &mut entry.target else {
                            unreachable!("fixture is a direct item")
                        };
                        item.prototype.ammunition_containers =
                            vec![AmmunitionContainerPocketPrototypeV1 {
                                pocket_index: 0,
                                pocket_id: String::from("CONTENTS"),
                                capacities: Vec::new(),
                                rigid: true,
                                access_moves: 100,
                                reloadable: false,
                                unloadable: true,
                                spawn_rules: Some(rules),
                            }];
                        entry.contents = vec![ItemGroupContentsSourceV1::Group(format!(
                            "depth_{:02}",
                            index + 1
                        ))];
                    }
                    ItemGroupDefinitionV1 {
                        group_id: format!("depth_{index:02}"),
                        graph: ItemGroupGraphV1 {
                            root_node: 0,
                            nodes: vec![ItemGroupNodeV1 {
                                node_id: 0,
                                kind: ItemGroupKindV1::Collection,
                                entries: vec![entry],
                            }],
                            wrapper: None,
                        },
                    }
                })
                .collect::<Vec<_>>()
        };
        assert!(item_group_catalog_is_valid(&containment_catalog(
            MAX_ITEM_COMPONENT_DEPTH,
        )));
        assert!(!item_group_catalog_is_valid(&containment_catalog(
            MAX_ITEM_COMPONENT_DEPTH + 1,
        )));

        let empty_group = |index: usize| ItemGroupDefinitionV1 {
            group_id: format!("group_{index:04}"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: Vec::new(),
                }],
                wrapper: None,
            },
        };
        let maximum_catalog = (0..MAX_ITEM_GROUP_DEFINITIONS)
            .map(empty_group)
            .collect::<Vec<_>>();
        assert!(item_group_catalog_is_valid(&maximum_catalog));

        let inline_node_count = MAX_ITEM_GROUP_NODES - maximum_catalog.len();
        let exact_inline = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: std::iter::once(ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: (1..inline_node_count)
                    .map(|index| {
                        item_group_entry(
                            100,
                            1,
                            1,
                            ItemGroupTargetV1::Node(
                                u16::try_from(index).expect("node bound fits u16"),
                            ),
                        )
                    })
                    .collect(),
            })
            .chain((1..inline_node_count).map(|index| ItemGroupNodeV1 {
                node_id: u16::try_from(index).expect("node bound fits u16"),
                kind: ItemGroupKindV1::Collection,
                entries: vec![item_group_entry(100, 1, 1, item_group_item("rock"))],
            }))
            .collect(),
            wrapper: None,
        });
        assert!(item_group_sources_are_valid(
            &maximum_catalog,
            std::iter::once(&exact_inline),
        ));
        let one_more_inline = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: Vec::new(),
            }],
            wrapper: None,
        });
        assert!(!item_group_sources_are_valid(
            &maximum_catalog,
            [&exact_inline, &one_more_inline],
        ));

        let oversized_catalog = (0..=MAX_ITEM_GROUP_DEFINITIONS)
            .map(empty_group)
            .collect::<Vec<_>>();
        assert!(!item_group_catalog_is_valid(&oversized_catalog));
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
                raw_damage: MAX_ITEM_RAW_DAMAGE,
                fitted: false,
                variant: None,
                snippet: None,
                variables: BTreeMap::new(),
                melee_damage_milli: melee_damage_milli.clone(),
                calories: i32::MAX,
                quench: i32::MAX,
                comestible_type: "x".repeat(32),
                temperature: None,
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
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
                creature_corpse: None,
                containment: Default::default(),
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
            magazine_capacity: 10,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: ItemContainmentProfileV1 {
                volume_milliliters: 17,
                ..ItemContainmentProfileV1::default()
            },
        };
        assert!(valid_item_snapshot(&magazine));
        let mut tool = ItemSnapshot {
            id: ItemId::new(1, 1),
            type_id: String::from("flashlight"),
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
            magazine_wells: vec![MagazineWellSnapshotV1 {
                pocket_index: 3,
                pocket_id: String::from("MAGAZINE_WELL"),
                compatible_magazine_type_ids: vec![String::from("medium_battery")],
                rigid: true,
                unloadable: true,
                installed_magazine: Some(Box::new(magazine.clone())),
            }],
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: ItemContainmentProfileV1 {
                volume_milliliters: 10,
                ..ItemContainmentProfileV1::default()
            },
        };
        let mut second_magazine = magazine.clone();
        second_magazine.id = ItemId::new(1, 3);
        second_magazine.type_id = String::from("large_battery");
        tool.magazine_wells.push(MagazineWellSnapshotV1 {
            pocket_index: 7,
            pocket_id: String::from("AUXILIARY"),
            compatible_magazine_type_ids: vec![String::from("large_battery")],
            rigid: true,
            unloadable: true,
            installed_magazine: Some(Box::new(second_magazine.clone())),
        });
        assert!(valid_item_snapshot(&tool));
        let mut fitted_without_capability = tool.clone();
        fitted_without_capability.fitted = true;
        assert!(!valid_item_snapshot(&fitted_without_capability));
        fitted_without_capability.containment.flags = vec![String::from("VARSIZE")];
        assert!(valid_item_snapshot(&fitted_without_capability));
        let mut immutable_fit = tool.clone();
        immutable_fit.containment.flags = vec![String::from("FIT")];
        assert!(!valid_item_snapshot(&immutable_fit));
        immutable_fit.fitted = true;
        assert!(valid_item_snapshot(&immutable_fit));
        assert_eq!(
            item_snapshot_containment_volume_milliliters(&tool),
            Some(10)
        );
        let mut non_rigid_primary = tool.clone();
        non_rigid_primary.magazine_wells[0].rigid = false;
        assert_eq!(
            item_snapshot_containment_volume_milliliters(&non_rigid_primary),
            Some(27)
        );
        non_rigid_primary.magazine_wells[1].rigid = false;
        assert_eq!(
            item_snapshot_containment_volume_milliliters(&non_rigid_primary),
            Some(44)
        );
        let mut ids = BTreeSet::new();
        assert!(collect_stable_item_ids(&tool, 1, &mut ids));
        assert_eq!(
            ids,
            BTreeSet::from([tool.id, magazine.id, second_magazine.id])
        );
        let duplicate_root = ItemSnapshot {
            id: ItemId::new(1, 4),
            containment: Default::default(),
            ..tool.clone()
        };
        assert!(!collect_stable_item_ids(&duplicate_root, 1, &mut ids));

        let mut duplicate_pocket = tool.clone();
        duplicate_pocket.magazine_wells[1].pocket_index = 3;
        assert!(!valid_item_snapshot(&duplicate_pocket));
        let mut oversized = tool.clone();
        while oversized.magazine_wells.len() <= MAX_ITEM_MAGAZINE_WELLS {
            let pocket_index =
                u16::try_from(oversized.magazine_wells.len() + 10).expect("small test index");
            oversized.magazine_wells.push(MagazineWellSnapshotV1 {
                pocket_index,
                pocket_id: String::new(),
                compatible_magazine_type_ids: vec![String::from("large_battery")],
                rigid: true,
                unloadable: true,
                installed_magazine: None,
            });
        }
        assert!(!valid_item_snapshot(&oversized));

        let mut hidden_parent_charges = tool.clone();
        hidden_parent_charges.charges = 1;
        assert!(!valid_item_snapshot(&hidden_parent_charges));
        let mut incompatible = tool.clone();
        incompatible
            .magazine_wells
            .first_mut()
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
            power_pocket_index: 3,
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
        powered.type_id = String::from("flashlight_on");
        powered
            .powered_tool
            .as_mut()
            .expect("powered state exists")
            .power_pocket_index = 6;
        assert!(!valid_item_snapshot(&powered));

        let mut fractional_battery = powered
            .magazine_wells
            .first()
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
    fn integral_magazine_snapshots_bound_item_backed_ammunition_and_stable_ids() {
        let ammunition = ItemSnapshot {
            id: ItemId::new(1, 2),
            type_id: String::from("battery"),
            charges: 6,
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
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: Default::default(),
        };
        let mut loose_fractional_battery = ammunition.clone();
        loose_fractional_battery.charges = 0;
        loose_fractional_battery.residual_energy_millijoules = MILLIJOULES_PER_BATTERY_CHARGE - 1;
        loose_fractional_battery.containment.count_by_charges = true;
        loose_fractional_battery.containment.stack_size = 1;
        assert!(valid_item_snapshot(&loose_fractional_battery));
        loose_fractional_battery.residual_energy_millijoules = 0;
        assert!(!valid_item_snapshot(&loose_fractional_battery));
        loose_fractional_battery.residual_energy_millijoules = MILLIJOULES_PER_BATTERY_CHARGE - 1;
        loose_fractional_battery.ammunition_type = String::from("9mm");
        assert!(!valid_item_snapshot(&loose_fractional_battery));
        loose_fractional_battery.ammunition_type = String::from("battery");
        loose_fractional_battery.residual_energy_millijoules = MILLIJOULES_PER_BATTERY_CHARGE;
        assert!(!valid_item_snapshot(&loose_fractional_battery));
        let mut magazine = ItemSnapshot {
            id: ItemId::new(1, 1),
            type_id: String::from("test_cell"),
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
            integral_magazines: vec![IntegralMagazinePocketSnapshotV1 {
                pocket_index: 3,
                pocket_id: String::from("PRIMARY"),
                ammunition_type: String::from("battery"),
                capacity: 6,
                rigid: true,
                reloadable: true,
                unloadable: true,
                loaded_ammunition: Some(Box::new(ammunition)),
                residual_energy_millijoules: 0,
            }],
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: Default::default(),
        };
        assert!(valid_item_snapshot(&magazine));
        let mut ids = BTreeSet::new();
        assert!(collect_stable_item_ids(&magazine, 1, &mut ids));
        assert_eq!(ids, BTreeSet::from([ItemId::new(1, 1), ItemId::new(1, 2)]));

        magazine.integral_magazines[0]
            .loaded_ammunition
            .as_deref_mut()
            .expect("nested ammunition should exist")
            .charges = 7;
        assert!(!valid_item_snapshot(&magazine));
        magazine.integral_magazines[0]
            .loaded_ammunition
            .as_deref_mut()
            .expect("nested ammunition should exist")
            .charges = 6;
        magazine.integral_magazines[0].capacity = i32::MAX as u32 + 1;
        assert!(!valid_item_snapshot(&magazine));
        magazine.integral_magazines[0].capacity = 6;
        magazine.integral_magazines[0].residual_energy_millijoules = 1;
        assert!(
            !valid_item_snapshot(&magazine),
            "fractional energy must occupy one capacity slot"
        );
        magazine.integral_magazines[0]
            .loaded_ammunition
            .as_deref_mut()
            .expect("nested ammunition should exist")
            .charges = 5;
        assert!(valid_item_snapshot(&magazine));
        magazine.integral_magazines[0].residual_energy_millijoules = 0;
        magazine.integral_magazines[0]
            .loaded_ammunition
            .as_deref_mut()
            .expect("nested ammunition should exist")
            .charges = 6;
        magazine
            .integral_magazines
            .push(IntegralMagazinePocketSnapshotV1 {
                pocket_index: 3,
                pocket_id: String::from("DUPLICATE"),
                ammunition_type: String::from("battery"),
                capacity: 1,
                rigid: true,
                reloadable: true,
                unloadable: true,
                loaded_ammunition: None,
                residual_energy_millijoules: 0,
            });
        assert!(!valid_item_snapshot(&magazine));
    }

    #[test]
    fn ammunition_container_pockets_round_trip_and_enforce_all_vector_bounds() {
        let ammunition = |counter: u64, ammunition_type: &str, charges: i32| ItemSnapshot {
            id: ItemId::new(1, counter),
            type_id: format!("{ammunition_type}_round_{counter}"),
            charges,
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
            ammunition_type: ammunition_type.to_owned(),
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
        let pocket = AmmunitionContainerPocketSnapshotV1 {
            pocket_index: 4,
            pocket_id: String::from("AMMO_POUCH"),
            capacities: vec![AmmunitionCapacityV1 {
                ammunition_type: String::from("9mm"),
                capacity: 30,
            }],
            rigid: false,
            access_moves: 100,
            reloadable: true,
            unloadable: true,
            contents: vec![ammunition(2, "9mm", 10), ammunition(3, "9mm", 20)],
            spawn_state: None,
        };
        let owner = ItemSnapshot {
            id: ItemId::new(1, 1),
            type_id: String::from("ammo_pouch"),
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
            ammunition_containers: vec![pocket.clone()],
            residual_energy_millijoules: 0,
            powered_tool: None,
            creature_corpse: None,
            containment: Default::default(),
        };
        assert!(valid_item_snapshot(&owner));
        let mut ids = BTreeSet::new();
        assert!(collect_stable_item_ids(&owner, 1, &mut ids));
        assert_eq!(
            ids,
            BTreeSet::from([ItemId::new(1, 1), ItemId::new(1, 2), ItemId::new(1, 3)])
        );

        let command = ControlMessage::Command(ClientCommand {
            actor_id: ActorId::new(1, 8),
            sequence: CommandSequence(1),
            client_tick: SimTick(0),
            kind: CommandKind::InsertPocketItem {
                owner_item: owner.id,
                pocket_index: 4,
                source_item: ItemId::new(1, 9),
            },
        });
        let encoded = encode_control(&command).expect("valid insertion command should encode");
        assert_eq!(
            decode_control(&encoded).expect("insertion command should decode"),
            command
        );
        let event = ControlMessage::Events(vec![WorldEvent {
            id: EventId(1),
            tick: SimTick(2),
            kind: WorldEventKind::AmmunitionInsertedIntoContainer {
                actor_id: ActorId::new(1, 8),
                owner_item: owner.id,
                pocket_index: 4,
                source_item: ItemId::new(1, 9),
                contained_item: ItemId::new(1, 10),
                ammunition_type: String::from("9mm"),
                transferred: 5,
                pocket_ammunition: 25,
                source_charges_remaining: 7,
            },
        }]);
        let encoded = encode_control(&event).expect("valid insertion event should encode");
        assert_eq!(
            decode_control(&encoded).expect("insertion event should decode"),
            event
        );

        let mut invalid_command = command.clone();
        let ControlMessage::Command(command) = &mut invalid_command else {
            unreachable!("fixture is a command")
        };
        command.kind = CommandKind::InsertPocketItem {
            owner_item: owner.id,
            pocket_index: 4,
            source_item: owner.id,
        };
        assert_eq!(
            encode_control(&invalid_command),
            Err(FrameError::InvalidBounds)
        );

        let mut over_capacity = owner.clone();
        over_capacity.ammunition_containers[0].contents[1].charges = 21;
        assert!(!valid_item_snapshot(&over_capacity));
        let mut incompatible = owner.clone();
        incompatible.ammunition_containers[0].contents[0].ammunition_type = String::from("45");
        assert!(!valid_item_snapshot(&incompatible));
        let mut mixed_categories = owner.clone();
        mixed_categories.ammunition_containers[0].capacities.insert(
            0,
            AmmunitionCapacityV1 {
                ammunition_type: String::from("45"),
                capacity: 30,
            },
        );
        mixed_categories.ammunition_containers[0].contents[1].ammunition_type = String::from("45");
        assert!(
            mixed_categories.ammunition_containers[0]
                .capacities
                .windows(2)
                .all(|pair| pair[0].ammunition_type < pair[1].ammunition_type)
        );
        assert!(
            mixed_categories.ammunition_containers[0]
                .contents
                .iter()
                .all(|content| mixed_categories.ammunition_containers[0]
                    .capacities
                    .iter()
                    .any(|capacity| capacity.ammunition_type == content.ammunition_type))
        );
        assert!(!valid_item_snapshot(&mixed_categories));
        let mut unsorted_contents = owner.clone();
        unsorted_contents.ammunition_containers[0]
            .contents
            .swap(0, 1);
        assert!(!valid_item_snapshot(&unsorted_contents));
        let mut non_plain_ammunition = owner.clone();
        non_plain_ammunition.ammunition_containers[0].contents[0].component_provenance =
            Some(Vec::new());
        assert!(!valid_item_snapshot(&non_plain_ammunition));

        let mut too_many_types = pocket.clone();
        too_many_types.contents.clear();
        too_many_types.capacities = (0..=MAX_AMMUNITION_CONTAINER_TYPES)
            .map(|index| AmmunitionCapacityV1 {
                ammunition_type: format!("ammo_{index:03}"),
                capacity: 1,
            })
            .collect();
        let prototype = AmmunitionContainerPocketPrototypeV1 {
            pocket_index: too_many_types.pocket_index,
            pocket_id: too_many_types.pocket_id.clone(),
            capacities: too_many_types.capacities.clone(),
            rigid: too_many_types.rigid,
            access_moves: too_many_types.access_moves,
            reloadable: too_many_types.reloadable,
            unloadable: too_many_types.unloadable,
            spawn_rules: None,
        };
        assert!(!valid_ammunition_container_prototype(&prototype));
        let mut zero_access_moves = prototype.clone();
        zero_access_moves.capacities = pocket.capacities.clone();
        zero_access_moves.access_moves = 0;
        assert!(!valid_ammunition_container_prototype(&zero_access_moves));

        let mut too_many_contents = pocket.clone();
        too_many_contents.capacities[0].capacity =
            u32::try_from(MAX_AMMUNITION_CONTAINER_CONTENTS + 1).expect("small bound");
        too_many_contents.contents = (0..=MAX_AMMUNITION_CONTAINER_CONTENTS)
            .map(|index| ammunition(u64::try_from(index + 2).expect("small ID"), "9mm", 1))
            .collect();
        let mut oversized_contents = owner.clone();
        oversized_contents.ammunition_containers = vec![too_many_contents];
        assert!(!valid_item_snapshot(&oversized_contents));

        let mut too_many_pockets = owner.clone();
        too_many_pockets.ammunition_containers = (0..=MAX_ITEM_AMMUNITION_CONTAINER_POCKETS)
            .map(|index| AmmunitionContainerPocketSnapshotV1 {
                pocket_index: u16::try_from(index).expect("small pocket index"),
                contents: Vec::new(),
                spawn_state: None,
                ..pocket.clone()
            })
            .collect();
        assert!(!valid_item_snapshot(&too_many_pockets));

        let mut phone = ammunition(2, "", 1);
        phone.type_id = String::from("smart_phone");
        phone.ammunition_type.clear();
        phone.snippet = Some(ItemSnippetV1 {
            id: String::from("greeting"),
            text: String::from("Hello\nworld"),
        });
        phone.variables.insert(
            String::from("browsed"),
            ItemVariableValueV1::String(String::from("false")),
        );
        phone.containment = ItemContainmentProfileV1 {
            weight_milligrams: 233_000,
            volume_milliliters: 111,
            longest_side_millimeters: 150,
            flags: Vec::new(),
            estorable: false,
            ..ItemContainmentProfileV1::default()
        };
        let rules = SpawnPocketRulesV1 {
            kind: SpawnPocketKindV1::Container,
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
            sealable: true,
        };
        let mut generic_owner = owner.clone();
        generic_owner.ammunition_containers = vec![AmmunitionContainerPocketSnapshotV1 {
            pocket_index: 4,
            pocket_id: String::from("PHONE"),
            capacities: Vec::new(),
            rigid: true,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            contents: vec![phone],
            spawn_state: Some(SpawnPocketStateV1 {
                rules: rules.clone(),
                contents_collapsed: false,
                sealed: true,
            }),
        }];
        assert!(valid_item_snapshot(&generic_owner));
        let mut single_item_owner = generic_owner.clone();
        {
            let single_item_state = single_item_owner.ammunition_containers[0]
                .spawn_state
                .as_mut()
                .expect("spawn state should exist");
            single_item_state.rules.item_restrictions = vec![
                String::from(SPAWN_POCKET_SINGLE_ITEM_MARKER),
                String::from("smart_phone"),
            ];
            single_item_state.sealed = false;
        }
        assert!(valid_item_snapshot(&single_item_owner));
        let single_item_pocket = &mut single_item_owner.ammunition_containers[0];
        let mut second_phone = single_item_pocket.contents[0].clone();
        second_phone.id = ItemId(3);
        single_item_pocket.contents.push(second_phone);
        assert!(
            !valid_item_snapshot(&single_item_owner),
            "holster/ablative recovery must reject two canonical item identities"
        );
        let mut marker_as_flag = rules.clone();
        marker_as_flag.flag_restrictions = vec![String::from(SPAWN_POCKET_SINGLE_ITEM_MARKER)];
        assert!(!valid_spawn_pocket_rules(&marker_as_flag));
        let mut flexible_wrapper = generic_owner.clone();
        flexible_wrapper.type_id = String::from("wrapper");
        flexible_wrapper.containment.weight_milligrams = 3_000;
        flexible_wrapper.containment.volume_milliliters = 50;
        let flexible_pocket = &mut flexible_wrapper.ammunition_containers[0];
        flexible_pocket.rigid = false;
        flexible_pocket.pocket_id.clear();
        flexible_pocket.contents[0].type_id = String::from("chaw");
        flexible_pocket.contents[0].containment.weight_milligrams = 4_000;
        flexible_pocket.contents[0].containment.volume_milliliters = 4;
        flexible_pocket.contents[0]
            .containment
            .longest_side_millimeters = 0;
        let flexible_state = flexible_pocket
            .spawn_state
            .as_mut()
            .expect("spawn state should exist");
        flexible_state.rules.rigid = false;
        flexible_state.rules.max_contains_volume_milliliters = 2_500;
        flexible_state.rules.magazine_well_volume_milliliters = 45;
        flexible_state.rules.max_contains_weight_milligrams = 6_000_000;
        flexible_state.rules.max_item_volume_milliliters = 2_500;
        flexible_state.rules.max_item_length_millimeters = 191;
        flexible_state.rules.item_restrictions.clear();
        flexible_state.contents_collapsed = true;
        flexible_state.sealed = false;
        assert_eq!(
            spawn_pocket_external_volume_milliliters(&flexible_state.rules, 4),
            0
        );
        assert_eq!(
            spawn_pocket_external_volume_milliliters(&flexible_state.rules, 80),
            35
        );
        assert!(valid_item_snapshot(&flexible_wrapper));
        assert_eq!(
            item_snapshot_containment_volume_milliliters(&flexible_wrapper),
            Some(50),
            "contents within the reserved base volume must not expand a flexible wrapper"
        );
        let mut invalid_collapsed_efile = rules.clone();
        invalid_collapsed_efile.kind = SpawnPocketKindV1::EFileStorage;
        invalid_collapsed_efile.rigid = true;
        invalid_collapsed_efile.contents_collapsed_by_default = true;
        assert!(
            !valid_spawn_pocket_rules(&invalid_collapsed_efile),
            "COLLAPSE_CONTENTS applies only to standard physical pockets upstream"
        );
        let mut empty_sealed = generic_owner.clone();
        empty_sealed.ammunition_containers[0].contents.clear();
        assert!(
            !valid_item_snapshot(&empty_sealed),
            "a canonical snapshot cannot inject an impossible sealed-empty pocket"
        );
        let mut partial_sealed = generic_owner.clone();
        let partial_rules = &mut partial_sealed.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules;
        partial_rules.max_contains_volume_milliliters = 222;
        partial_rules.max_contains_weight_milligrams = 466_000;
        partial_rules.max_item_volume_milliliters = 222;
        assert!(
            !valid_item_snapshot(&partial_sealed),
            "a canonical snapshot cannot inject an impossible sealed-partial pocket"
        );
        partial_sealed.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .sealed = false;
        assert!(valid_item_snapshot(&partial_sealed));
        let mut any_restriction = generic_owner.clone();
        let any_rules = &mut any_restriction.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules;
        any_rules.item_restrictions = vec![String::from("different_item")];
        any_rules.flag_restrictions = vec![String::from("FORM_A"), String::from("FORM_B")];
        any_restriction.ammunition_containers[0].contents[0]
            .containment
            .flags = vec![String::from("FORM_B")];
        assert!(valid_item_snapshot(&any_restriction));
        any_restriction.ammunition_containers[0].contents[0]
            .containment
            .flags = vec![String::from("FORM_C")];
        assert!(!valid_item_snapshot(&any_restriction));
        let mut no_unwield = generic_owner.clone();
        no_unwield.ammunition_containers[0].contents[0]
            .containment
            .flags = vec![String::from("NO_UNWIELD")];
        assert!(!valid_item_snapshot(&no_unwield));

        let mut charged_overflow = generic_owner.clone();
        let charged = &mut charged_overflow.ammunition_containers[0].contents[0];
        charged.charges = 2;
        charged.containment.weight_milligrams = 1;
        charged.containment.volume_milliliters = 1_000;
        charged.containment.count_by_charges = true;
        charged.containment.stack_size = 10;
        assert!(
            charged.containment.volume_milliliters
                > charged_overflow.ammunition_containers[0]
                    .spawn_state
                    .as_ref()
                    .expect("spawn state")
                    .rules
                    .max_contains_volume_milliliters
        );
        assert!(!valid_item_snapshot(&charged_overflow));
        charged_overflow.ammunition_containers[0].contents[0].charges = 1;
        assert!(valid_item_snapshot(&charged_overflow));
        charged_overflow.ammunition_containers[0].contents[0].charges = 0;
        assert!(
            !valid_item_snapshot(&charged_overflow),
            "canonical count-by-charges items must retain a positive charge count"
        );

        let mut leaking_liquid = generic_owner.clone();
        let liquid = &mut leaking_liquid.ammunition_containers[0].contents[0];
        liquid.charges = 5;
        liquid.containment.phase = ItemPhaseV1::Liquid;
        liquid.containment.count_by_charges = true;
        liquid.containment.stack_size = 10;
        liquid.containment.weight_milligrams = 1;
        liquid.containment.volume_milliliters = 100;
        leaking_liquid.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules
            .watertight = false;
        leaking_liquid.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .sealed = false;
        assert!(!valid_item_snapshot(&leaking_liquid));
        leaking_liquid.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules
            .watertight = true;
        assert!(valid_item_snapshot(&leaking_liquid));
        let mut matching_liquids = leaking_liquid.clone();
        let mut matching_liquid = matching_liquids.ammunition_containers[0].contents[0].clone();
        matching_liquid.id = ItemId::new(1, 3);
        matching_liquids.ammunition_containers[0]
            .contents
            .push(matching_liquid);
        assert!(valid_item_snapshot(&matching_liquids));
        matching_liquids.ammunition_containers[0].contents[1]
            .variables
            .insert(
                String::from("browsed"),
                ItemVariableValueV1::String(String::from("different")),
            );
        assert!(
            !valid_item_snapshot(&matching_liquids),
            "same-type liquids with different represented stack state cannot combine"
        );
        let encoded = postcard::to_stdvec(&generic_owner)
            .expect("generalized containment snapshot should encode");
        assert_eq!(
            postcard::from_bytes::<ItemSnapshot>(&encoded)
                .expect("generalized containment snapshot should decode"),
            generic_owner.clone()
        );
        let mut too_long = generic_owner.clone();
        too_long.ammunition_containers[0].contents[0]
            .containment
            .longest_side_millimeters = 151;
        assert!(!valid_item_snapshot(&too_long));

        let mut empty_soft = generic_owner.clone();
        empty_soft.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .sealed = false;
        let soft = &mut empty_soft.ammunition_containers[0].contents[0];
        soft.containment.flags = vec![String::from("SOFT")];
        soft.containment.volume_milliliters = 500;
        soft.containment.longest_side_millimeters = 500;
        let soft_rules = &mut empty_soft.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules;
        soft_rules.max_contains_volume_milliliters = 500;
        soft_rules.max_item_volume_milliliters = 100;
        soft_rules.max_item_length_millimeters = 100;
        assert!(
            valid_item_snapshot(&empty_soft),
            "an empty explicit SOFT item bypasses max-item volume and has zero length upstream"
        );
        empty_soft.ammunition_containers[0].contents[0]
            .containment
            .flags
            .clear();
        assert!(
            !valid_item_snapshot(&empty_soft),
            "material-derived softness is fail-closed when the hard interpretation fails"
        );

        let mut nested_length = generic_owner.clone();
        nested_length.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .sealed = false;
        let nested_rules = &mut nested_length.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules;
        nested_rules.max_contains_volume_milliliters = 1_000;
        nested_rules.max_contains_weight_milligrams = 1_000_000;
        nested_rules.max_item_volume_milliliters = 1_000;
        nested_rules.max_item_length_millimeters = 100;
        let inner = &mut nested_length.ammunition_containers[0].contents[0];
        inner.containment.flags = vec![String::from("HARD")];
        inner.containment.longest_side_millimeters = 50;
        let mut long_child = inner.clone();
        long_child.id = ItemId::new(1, 3);
        long_child.type_id = String::from("long_child");
        long_child.snippet = None;
        long_child.variables.clear();
        long_child.containment = ItemContainmentProfileV1 {
            weight_milligrams: 1,
            volume_milliliters: 1,
            longest_side_millimeters: 150,
            flags: vec![String::from("HARD")],
            ..ItemContainmentProfileV1::default()
        };
        long_child.ammunition_containers.clear();
        inner.ammunition_containers = vec![AmmunitionContainerPocketSnapshotV1 {
            pocket_index: 0,
            pocket_id: String::from("INNER"),
            capacities: Vec::new(),
            rigid: true,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            contents: vec![long_child],
            spawn_state: Some(SpawnPocketStateV1 {
                rules: SpawnPocketRulesV1 {
                    kind: SpawnPocketKindV1::Container,
                    max_contains_volume_milliliters: 1_000,
                    magazine_well_volume_milliliters: 0,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: 1_000_000,
                    max_item_volume_milliliters: 1_000,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: 200,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 100,
                    rigid: true,
                    watertight: false,
                    transparent: false,
                    forbidden: false,
                    sealable: false,
                },
                contents_collapsed: false,
                sealed: false,
            }),
        }];
        assert!(
            !valid_item_snapshot(&nested_length),
            "a physical child contributes its recursive length to the wrapper"
        );
        let mut invalid_seal = generic_owner.clone();
        invalid_seal.ammunition_containers[0]
            .spawn_state
            .as_mut()
            .expect("spawn state")
            .rules
            .sealable = false;
        assert!(!valid_item_snapshot(&invalid_seal));
        let mut invalid_snippet = generic_owner.clone();
        invalid_snippet.ammunition_containers[0].contents[0]
            .snippet
            .as_mut()
            .expect("snippet")
            .text = String::from("bad\u{0000}text");
        assert!(!valid_item_snapshot(&invalid_snippet));

        let mut colliding_index = owner;
        colliding_index.magazine_wells = vec![MagazineWellSnapshotV1 {
            pocket_index: 4,
            pocket_id: String::from("COLLISION"),
            compatible_magazine_type_ids: vec![String::from("test_magazine")],
            rigid: true,
            unloadable: true,
            installed_magazine: None,
        }];
        assert!(!valid_item_snapshot(&colliding_index));
    }

    #[test]
    fn containment_weight_honors_no_drop_and_reduced_weight_flags() {
        let ammunition = |counter: u64, ammunition_type: &str, charges: i32| ItemSnapshot {
            id: ItemId::new(1, counter),
            type_id: format!("{ammunition_type}_round_{counter}"),
            charges,
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
            ammunition_type: ammunition_type.to_owned(),
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
        let mut reduced = ammunition(2, "test", 3);
        reduced.containment.weight_milligrams = 5;
        reduced.containment.count_by_charges = true;
        reduced.containment.flags = vec![String::from("REDUCED_WEIGHT")];
        assert_eq!(
            item_snapshot_containment_weight_milligrams(&reduced),
            Some(11),
            "pinned integer mass truncates 5 * 3 * 0.75"
        );

        let mut no_drop = ammunition(2, "", 1);
        no_drop.containment.weight_milligrams = 100;
        no_drop.containment.flags = vec![String::from("NO_DROP")];
        let mut child = ammunition(3, "", 1);
        child.containment.weight_milligrams = 900;
        no_drop.ammunition_containers = vec![AmmunitionContainerPocketSnapshotV1 {
            pocket_index: 0,
            pocket_id: String::from("PHYSICAL"),
            capacities: Vec::new(),
            rigid: true,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            contents: vec![child],
            spawn_state: Some(SpawnPocketStateV1 {
                rules: SpawnPocketRulesV1 {
                    kind: SpawnPocketKindV1::Container,
                    max_contains_volume_milliliters: 1_000,
                    magazine_well_volume_milliliters: 0,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: 1_000,
                    max_item_volume_milliliters: 1_000,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: 1_000,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 100,
                    rigid: true,
                    watertight: false,
                    transparent: false,
                    forbidden: false,
                    sealable: false,
                },
                contents_collapsed: false,
                sealed: false,
            }),
        }];
        assert_eq!(
            item_snapshot_containment_weight_milligrams(&no_drop),
            Some(0),
            "NO_DROP returns before physical child weight upstream"
        );
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
            raw_damage: 1,
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
            creature_corpse: Some(CreatureCorpseSnapshotV1 {
                prototype,
                death_tick: SimTick(20),
                revive_special: false,
                revivable: true,
            }),
            containment: Default::default(),
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
            count_by_charges: false,
            recoverable: true,
            component_provenance: None,
            magazine_capacity: 0,
            integral_magazines: Vec::new(),
            magazine_wells: Vec::new(),
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool: None,
            containment: Default::default(),
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
        let mut mismatched_charge_mode = component();
        mismatched_charge_mode.containment.count_by_charges = true;
        assert!(!valid_item_component_root(&mismatched_charge_mode));
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
