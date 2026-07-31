//! Canonical NPC identity, dialogue programs, and per-player social state.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    ActorId, EocConditionV1, EocEffectV1, MAX_INTERACTION_CHOICE_LABEL_BYTES, NpcId, WorldPosition,
    eoc_condition_is_valid, eoc_effects_are_valid,
};

pub const MAX_NPC_TEMPLATES: usize = 4_096;
pub const MAX_DIALOGUE_TOPICS: usize = 16_384;
pub const MAX_DIALOGUE_RESPONSES: usize = 64;
pub const MAX_DIALOGUE_TEXT_BYTES: usize = MAX_INTERACTION_CHOICE_LABEL_BYTES;
pub const MAX_DIALOGUE_ID_BYTES: usize = 512;
pub const MAX_NPC_NAME_BYTES: usize = 1_024;
pub const MAX_NPC_OPINION_ABS: i32 = 1_000_000_000;

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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcTemplateV1 {
    pub template_id: String,
    pub name: String,
    pub gender: Option<String>,
    pub faction_id: String,
    pub class_id: String,
    pub attitude: i32,
    pub mission: String,
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
    pub position: WorldPosition,
    pub social: Vec<NpcSocialStateV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleNpcSnapshotV1 {
    pub id: NpcId,
    pub template_id: String,
    pub name: String,
    pub position: WorldPosition,
    pub opinion_of_controlled_actor: NpcOpinionV1,
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
            && valid_text(&template.name, MAX_NPC_NAME_BYTES)
            && template
                .gender
                .as_ref()
                .is_none_or(|gender| valid_id(gender))
            && optional_id_is_valid(&template.faction_id)
            && optional_id_is_valid(&template.class_id)
            && optional_id_is_valid(&template.mission)
            && topic_ids.contains(template.chat_topic_id.as_str())
    }) && topics.iter().all(|topic| {
        valid_id(&topic.topic_id)
            && valid_text(&topic.dynamic_line, MAX_DIALOGUE_TEXT_BYTES)
            && (1..=MAX_DIALOGUE_RESPONSES).contains(&topic.responses.len())
            && topic.responses.iter().enumerate().all(|(index, response)| {
                response.response_id == index.to_string()
                    && valid_text(&response.text, MAX_DIALOGUE_TEXT_BYTES)
                    && topic_ids.contains(response.next_topic_id.as_str())
                    && opinion_is_valid(&response.opinion_delta)
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
    npc.id.counter() > 0
        && npc.id.world_namespace() == world_namespace
        && templates
            .iter()
            .any(|template| template.template_id == npc.template_id && template.name == npc.name)
        && npc
            .social
            .windows(2)
            .all(|pair| pair[0].actor_id < pair[1].actor_id)
        && npc.social.iter().all(|social| {
            social.actor_id.counter() > 0
                && social.actor_id.world_namespace() == world_namespace
                && opinion_is_valid(&social.opinion)
        })
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
