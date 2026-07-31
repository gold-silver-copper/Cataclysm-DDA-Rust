//! Canonical NPC identity, dialogue programs, and per-player social state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    ActorId, EocConditionV1, EocEffectV1, MAX_INTERACTION_CHOICE_LABEL_BYTES, NpcId, WorldPosition,
    eoc_condition_is_valid, eoc_effects_are_valid,
};

pub const MAX_NPC_TEMPLATES: usize = 4_096;
pub const MAX_NPCS: usize = 1_048_576;
pub const MAX_NPC_MISSION_OFFERS: usize = 64;
pub const MAX_DIALOGUE_TOPICS: usize = 16_384;
pub const MAX_DIALOGUE_RESPONSES: usize = 64;
pub const MAX_DIALOGUE_TOPIC_STACK: usize = 64;
pub const MAX_DIALOGUE_TEXT_BYTES: usize = MAX_INTERACTION_CHOICE_LABEL_BYTES;
pub const MAX_DIALOGUE_ID_BYTES: usize = 512;
pub const MAX_NPC_NAME_BYTES: usize = 1_024;
pub const MAX_NPC_OPINION_ABS: i32 = 1_000_000_000;
pub const NPC_BUILTIN_MISSION_TOPICS: [&str; 10] = [
    "TALK_MISSION_LIST",
    "TALK_MISSION_LIST_ASSIGNED",
    "TALK_MISSION_OFFER",
    "TALK_MISSION_ACCEPTED",
    "TALK_MISSION_REJECTED",
    "TALK_MISSION_ADVICE",
    "TALK_MISSION_INQUIRE",
    "TALK_MISSION_SUCCESS",
    "TALK_MISSION_FAILURE",
    "TALK_MISSION_REWARD",
];

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcOpinionV1 {
    pub trust: i32,
    pub fear: i32,
    pub value: i32,
    pub anger: i32,
    pub owed: i32,
}

impl NpcOpinionV1 {
    #[must_use]
    pub fn checked_add(&self, delta: &Self) -> Option<Self> {
        Some(Self {
            trust: self.trust.checked_add(delta.trust)?,
            fear: self.fear.checked_add(delta.fear)?,
            value: self.value.checked_add(delta.value)?,
            anger: self.anger.checked_add(delta.anger)?,
            owed: self.owed.checked_add(delta.owed)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DialogueResponseV1 {
    pub response_id: String,
    pub text: String,
    pub next_topic_id: String,
    pub opinion_delta: NpcOpinionV1,
    pub effects: Vec<EocEffectV1>,
    pub condition: Option<EocConditionV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DialogueTopicV1 {
    pub topic_id: String,
    pub dynamic_line: String,
    pub responses: Vec<DialogueResponseV1>,
    /// Matches `json_talk_topic::replace_built_in_responses`.  When false,
    /// the authoritative dialogue kernel appends the pinned built-in family.
    pub replace_built_in_responses: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcTemplateV1 {
    pub template_id: String,
    pub name_unique: Option<String>,
    pub name_suffix: Option<String>,
    pub gender: Option<String>,
    pub faction_id: String,
    pub class_id: String,
    pub attitude: i32,
    pub mission: String,
    pub mission_offered: Vec<String>,
    pub chat_topic_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcSocialStateV1 {
    pub actor_id: ActorId,
    pub opinion: NpcOpinionV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcSnapshotV1 {
    pub id: NpcId,
    pub template_id: String,
    pub name: String,
    pub faction_id: String,
    pub attitude: i32,
    pub position: WorldPosition,
    pub social: Vec<NpcSocialStateV1>,
    pub mission_offers: Vec<NpcMissionOfferV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcMissionOfferV1 {
    pub mission_id: super::MissionId,
    pub mission_type_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleNpcSnapshotV1 {
    pub id: NpcId,
    pub template_id: String,
    pub name: String,
    pub faction_id: String,
    pub faction_name: String,
    pub hostile_to_controlled_actor: bool,
    pub position: WorldPosition,
    pub opinion_of_controlled_actor: Option<NpcOpinionV1>,
}

#[must_use]
pub fn npc_dialogue_catalog_is_valid(
    templates: &[NpcTemplateV1],
    topics: &[DialogueTopicV1],
) -> bool {
    if templates.len() > MAX_NPC_TEMPLATES
        || topics.len() > MAX_DIALOGUE_TOPICS
        || !templates
            .windows(2)
            .all(|pair| pair[0].template_id < pair[1].template_id)
        || !topics
            .windows(2)
            .all(|pair| pair[0].topic_id < pair[1].topic_id)
    {
        return false;
    }
    let topic_ids = topics
        .iter()
        .map(|topic| topic.topic_id.as_str())
        .collect::<BTreeSet<_>>();
    templates.iter().all(|template| {
        valid_id(&template.template_id)
            && template
                .name_unique
                .as_ref()
                .is_none_or(|name| valid_text(name, MAX_NPC_NAME_BYTES))
            && template
                .name_suffix
                .as_ref()
                .is_none_or(|name| valid_text(name, MAX_NPC_NAME_BYTES))
            && template
                .gender
                .as_ref()
                .is_none_or(|gender| valid_id(gender))
            && optional_id_is_valid(&template.faction_id)
            && (0..=18).contains(&template.attitude)
            && optional_id_is_valid(&template.class_id)
            && npc_template_attitude_is_supported(template.attitude)
            && optional_id_is_valid(&template.mission)
            && template.mission_offered.len() <= MAX_NPC_MISSION_OFFERS
            && template
                .mission_offered
                .iter()
                .all(|mission_type_id| valid_id(mission_type_id))
            && (template.chat_topic_id == "TALK_DONE"
                || topic_ids.contains(template.chat_topic_id.as_str()))
    }) && topics.iter().all(|topic| {
        let builtin = NPC_BUILTIN_MISSION_TOPICS.contains(&topic.topic_id.as_str())
            && !topic.replace_built_in_responses;
        valid_id(&topic.topic_id)
            && topic.topic_id != "TALK_NONE"
            && topic.topic_id != "TALK_DONE"
            && (builtin || valid_text(&topic.dynamic_line, MAX_DIALOGUE_TEXT_BYTES))
            && (builtin || !matches!(topic.dynamic_line.as_str(), "*" | "&"))
            && ((builtin && topic.responses.len() <= MAX_DIALOGUE_RESPONSES)
                || (!builtin && (1..=MAX_DIALOGUE_RESPONSES).contains(&topic.responses.len())))
            && topic.responses.iter().enumerate().all(|(index, response)| {
                response.response_id == index.to_string()
                    && valid_text(&response.text, MAX_DIALOGUE_TEXT_BYTES)
                    && (matches!(response.next_topic_id.as_str(), "TALK_NONE" | "TALK_DONE")
                        || topic_ids.contains(response.next_topic_id.as_str()))
                    && opinion_is_valid(&response.opinion_delta)
                    && opinion_delta_cannot_trigger_hostility(&response.opinion_delta)
                    && eoc_effects_are_valid(&response.effects)
                    && response
                        .condition
                        .as_ref()
                        .is_none_or(eoc_condition_is_valid)
            })
    })
}

#[must_use]
pub fn npc_snapshot_is_valid(
    npc: &NpcSnapshotV1,
    world_namespace: u64,
    templates: &[NpcTemplateV1],
) -> bool {
    templates
        .iter()
        .find(|template| template.template_id == npc.template_id)
        .is_some_and(|template| npc_snapshot_is_valid_for_template(npc, world_namespace, template))
}

#[must_use]
pub fn npc_snapshot_is_valid_for_template(
    npc: &NpcSnapshotV1,
    world_namespace: u64,
    template: &NpcTemplateV1,
) -> bool {
    npc.id.counter() > 0
        && npc.id.world_namespace() == world_namespace
        && npc.template_id == template.template_id
        && valid_text(&npc.name, MAX_NPC_NAME_BYTES)
        && template
            .name_unique
            .as_ref()
            .is_none_or(|name| npc.name == *name)
        && npc.faction_id == template.faction_id
        && (npc.attitude == template.attitude || (template.attitude == 1 && npc.attitude == 0))
        && npc
            .social
            .windows(2)
            .all(|pair| pair[0].actor_id < pair[1].actor_id)
        && npc.social.iter().all(|social| {
            social.actor_id.counter() > 0
                && social.actor_id.world_namespace() == world_namespace
                && opinion_is_valid(&social.opinion)
        })
        && npc.mission_offers.len() <= MAX_NPC_MISSION_OFFERS
        && npc
            .mission_offers
            .windows(2)
            .all(|pair| pair[0].mission_id < pair[1].mission_id)
        && npc.mission_offers.iter().all(|offer| {
            offer.mission_id.counter() > 0
                && offer.mission_id.world_namespace() == world_namespace
                && valid_id(&offer.mission_type_id)
                && npc
                    .mission_offers
                    .iter()
                    .filter(|candidate| candidate.mission_type_id == offer.mission_type_id)
                    .count()
                    <= template
                        .mission_offered
                        .iter()
                        .filter(|mission_type_id| {
                            mission_type_id.as_str() == offer.mission_type_id.as_str()
                        })
                        .count()
        })
}

/// Values accepted by the pinned `npc_template::load` implementation.
#[must_use]
pub const fn npc_template_attitude_is_supported(attitude: i32) -> bool {
    matches!(attitude, 0 | 1 | 3 | 5 | 6 | 8 | 9 | 10 | 11 | 13)
}

/// The pinned non-forced dialogue entry rejects hostile and fleeing template attitudes.
#[must_use]
pub const fn npc_template_attitude_will_talk(attitude: i32) -> bool {
    npc_template_attitude_is_supported(attitude) && !matches!(attitude, 10 | 11)
}

/// The current canonical NPC model has no pinned personality values. Admit only deltas that
/// cannot make a previously false `anger >= 20 + fear - aggression` comparison become true.
#[must_use]
pub const fn opinion_delta_cannot_trigger_hostility(delta: &NpcOpinionV1) -> bool {
    delta.anger <= 0 && delta.fear >= 0
}

#[must_use]
pub fn opinion_is_valid(opinion: &NpcOpinionV1) -> bool {
    [
        opinion.trust,
        opinion.fear,
        opinion.value,
        opinion.anger,
        opinion.owed,
    ]
    .into_iter()
    .all(|value| value.unsigned_abs() <= MAX_NPC_OPINION_ABS as u32)
}

fn optional_id_is_valid(id: &str) -> bool {
    id.is_empty() || valid_id(id)
}

fn valid_id(id: &str) -> bool {
    valid_text(id, MAX_DIALOGUE_ID_BYTES)
}

fn valid_text(text: &str, maximum: usize) -> bool {
    !text.is_empty() && text.len() <= maximum && !text.chars().any(char::is_control)
}
