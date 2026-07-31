//! Canonical mission programs and stable per-actor mission state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    EocEffectV1, MAX_INTERACTION_CHOICE_ID_BYTES, MissionId, NpcId, SimTick, eoc_effects_are_valid,
};

pub const MAX_MISSION_DEFINITIONS: usize = 16_384;
pub const MAX_ACTOR_MISSIONS: usize = 4_096;
pub const MAX_MISSION_TEXT_BYTES: usize = 16 * 1_024;
pub const MAX_CREATURE_KILL_COUNT_TYPES: usize = 16_384;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MissionGoalV1 {
    Null,
    FindItem {
        item_type_id: String,
        count: u32,
        count_by_charges: bool,
    },
    KillMonsterType {
        monster_type_id: String,
        count: u32,
    },
    KillMonsterSpecies {
        monster_species_id: String,
        monster_type_ids: Vec<String>,
        count: u32,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MissionDefinitionV1 {
    pub mission_type_id: String,
    pub name: String,
    pub description: String,
    pub difficulty: i32,
    pub value: i32,
    pub goal: MissionGoalV1,
    pub start_effects: Vec<EocEffectV1>,
    pub end_effects: Vec<EocEffectV1>,
    pub fail_effects: Vec<EocEffectV1>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MissionStatusV1 {
    InProgress,
    Success,
    Failure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MissionSnapshotV1 {
    pub mission_id: MissionId,
    pub mission_type_id: String,
    pub origin_npc_id: Option<NpcId>,
    pub assigned_at_tick: SimTick,
    pub finished_at_tick: Option<SimTick>,
    pub status: MissionStatusV1,
    /// Absolute global count captured at assignment plus the required delta.
    pub kill_count_to_reach: Option<u64>,
}

#[must_use]
pub fn mission_catalog_is_valid(definitions: &[MissionDefinitionV1]) -> bool {
    definitions.len() <= MAX_MISSION_DEFINITIONS
        && definitions
            .windows(2)
            .all(|pair| pair[0].mission_type_id < pair[1].mission_type_id)
        && definitions.iter().all(mission_definition_is_valid)
}

#[must_use]
pub fn mission_definition_is_valid(definition: &MissionDefinitionV1) -> bool {
    valid_id(&definition.mission_type_id)
        && valid_text(&definition.name, MAX_MISSION_TEXT_BYTES)
        && (definition.description.is_empty()
            || valid_text(&definition.description, MAX_MISSION_TEXT_BYTES))
        && mission_goal_is_valid(&definition.goal)
        && eoc_effects_are_valid(&definition.start_effects)
        && eoc_effects_are_valid(&definition.end_effects)
        && eoc_effects_are_valid(&definition.fail_effects)
}

#[must_use]
pub fn mission_snapshot_is_valid(
    mission: &MissionSnapshotV1,
    world_namespace: u64,
    definitions: &[MissionDefinitionV1],
) -> bool {
    let Some(definition) = definitions
        .iter()
        .find(|definition| definition.mission_type_id == mission.mission_type_id)
    else {
        return false;
    };
    mission_snapshot_is_valid_for_definition(mission, world_namespace, definition)
}

#[must_use]
pub fn mission_snapshot_is_valid_for_definition(
    mission: &MissionSnapshotV1,
    world_namespace: u64,
    definition: &MissionDefinitionV1,
) -> bool {
    mission.mission_type_id == definition.mission_type_id
        && mission.mission_id.counter() > 0
        && mission.mission_id.world_namespace() == world_namespace
        && mission.origin_npc_id.is_none_or(|npc_id| {
            npc_id.counter() > 0 && npc_id.world_namespace() == world_namespace
        })
        && match mission.status {
            MissionStatusV1::InProgress => mission.finished_at_tick.is_none(),
            MissionStatusV1::Success | MissionStatusV1::Failure => mission
                .finished_at_tick
                .is_some_and(|finished| finished >= mission.assigned_at_tick),
        }
        && match &definition.goal {
            MissionGoalV1::KillMonsterType { count, .. }
            | MissionGoalV1::KillMonsterSpecies { count, .. } => mission
                .kill_count_to_reach
                .is_some_and(|threshold| threshold >= u64::from(*count)),
            MissionGoalV1::Null | MissionGoalV1::FindItem { .. } => {
                mission.kill_count_to_reach.is_none()
            }
        }
}

#[must_use]
pub fn actor_missions_are_valid(
    missions: &[MissionSnapshotV1],
    world_namespace: u64,
    definitions: &[MissionDefinitionV1],
) -> bool {
    missions.len() <= MAX_ACTOR_MISSIONS
        && missions
            .windows(2)
            .all(|pair| pair[0].mission_id < pair[1].mission_id)
        && missions
            .iter()
            .all(|mission| mission_snapshot_is_valid(mission, world_namespace, definitions))
        && missions
            .iter()
            .map(|mission| mission.mission_id)
            .collect::<BTreeSet<_>>()
            .len()
            == missions.len()
}

#[must_use]
pub fn creature_kill_counts_are_valid(counts: &BTreeMap<String, u64>) -> bool {
    counts.len() <= MAX_CREATURE_KILL_COUNT_TYPES
        && counts
            .iter()
            .all(|(monster_type_id, count)| valid_id(monster_type_id) && *count > 0)
}

fn mission_goal_is_valid(goal: &MissionGoalV1) -> bool {
    match goal {
        MissionGoalV1::Null => true,
        MissionGoalV1::FindItem {
            item_type_id,
            count,
            ..
        } => valid_id(item_type_id) && *count > 0,
        MissionGoalV1::KillMonsterType {
            monster_type_id,
            count,
        } => valid_id(monster_type_id) && *count > 0,
        MissionGoalV1::KillMonsterSpecies {
            monster_species_id,
            monster_type_ids,
            count,
        } => {
            valid_id(monster_species_id)
                && *count > 0
                && !monster_type_ids.is_empty()
                && monster_type_ids.windows(2).all(|pair| pair[0] < pair[1])
                && monster_type_ids.iter().all(|id| valid_id(id))
        }
    }
}

fn valid_id(value: &str) -> bool {
    valid_text(value, MAX_INTERACTION_CHOICE_ID_BYTES)
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}
