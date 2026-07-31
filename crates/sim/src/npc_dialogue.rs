//! Authoritative deterministic NPC spawning and dialogue execution.

use std::collections::BTreeMap;

use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, DialogueTopicV1, InteractionCancellationReasonV1,
    InteractionChoiceV1, InteractionContextV1, InteractionId, MAX_DIALOGUE_TOPIC_STACK,
    MAX_NPC_NAME_BYTES, NpcId, NpcOpinionV1, NpcSnapshotV1, NpcSocialStateV1, NpcTemplateV1,
    PendingInteractionV1, SimTick, WorldEvent, WorldEventKind, WorldPosition,
    npc_dialogue_catalog_is_valid, npc_template_attitude_will_talk,
};
use serde::{Deserialize, Serialize};

use crate::{SimError, WorldState};

const DIALOGUE_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;
pub(super) const DIALOGUE_FALLBACK_CHOICE_ID: &str = "__fallback_done";
pub(super) const DIALOGUE_FALLBACK_CHOICE_LABEL: &str = "Bye.";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Npc {
    pub(super) id: NpcId,
    pub(super) template_id: String,
    pub(super) name: String,
    pub(super) position: WorldPosition,
    pub(super) social: BTreeMap<ActorId, NpcOpinionV1>,
}

impl Npc {
    pub(super) fn snapshot(&self) -> NpcSnapshotV1 {
        NpcSnapshotV1 {
            id: self.id,
            template_id: self.template_id.clone(),
            name: self.name.clone(),
            position: self.position,
            social: self
                .social
                .iter()
                .map(|(actor_id, opinion)| NpcSocialStateV1 {
                    actor_id: *actor_id,
                    opinion: opinion.clone(),
                })
                .collect(),
        }
    }
}

impl WorldState {
    pub fn register_npc_dialogue_catalog(
        &mut self,
        templates: Vec<NpcTemplateV1>,
        topics: Vec<DialogueTopicV1>,
    ) -> Result<(), SimError> {
        if !npc_dialogue_catalog_is_valid(&templates, &topics)
            || !self.npc_templates.is_empty()
            || !self.dialogue_topics.is_empty()
            || !self.npcs.is_empty()
        {
            return Err(SimError::InvalidNpcDialogue);
        }
        self.npc_templates = templates
            .into_iter()
            .map(|template| (template.template_id.clone(), template))
            .collect();
        self.dialogue_topics = topics
            .into_iter()
            .map(|topic| (topic.topic_id.clone(), topic))
            .collect();
        Ok(())
    }

    pub fn spawn_npc(
        &mut self,
        template_id: &str,
        generated_name: Option<&str>,
        position: WorldPosition,
    ) -> Result<NpcId, SimError> {
        let template = self
            .npc_templates
            .get(template_id)
            .cloned()
            .ok_or(SimError::InvalidNpcDialogue)?;
        let name = template
            .name_unique
            .clone()
            .or_else(|| generated_name.map(str::to_owned))
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= MAX_NPC_NAME_BYTES
                    && !name.chars().any(char::is_control)
            })
            .ok_or(SimError::InvalidNpcDialogue)?;
        if !self.is_passable(position)
            || self.actor_at(position).is_some()
            || self.creature_at(position).is_some()
            || self.npc_at(position).is_some()
        {
            return Err(SimError::SpawnBlocked);
        }
        let id = self.allocator.allocate_npc()?;
        self.npcs.insert(
            id,
            Npc {
                id,
                template_id: template.template_id,
                name,
                position,
                social: BTreeMap::new(),
            },
        );
        Ok(id)
    }

    pub(super) fn start_npc_dialogue(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        npc_id: NpcId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let target = self.npcs.get(&npc_id).and_then(|npc| {
            adjacent(actor_position, npc.position)
                .then(|| self.npc_templates.get(&npc.template_id))
                .flatten()
                .filter(|template| npc_template_attitude_will_talk(template.attitude))
                .map(|template| template.chat_topic_id.clone())
        });
        let Some(topic_id) = target else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::TargetMissing)?);
            return Ok(());
        };
        if !self.resolve_pending_before_npc_dialogue(actor_id, sequence, events)? {
            return Ok(());
        }
        if topic_id == "TALK_DONE" {
            return Ok(());
        }
        self.request_npc_topic(actor_id, npc_id, vec![topic_id], events)
    }

    pub(super) fn apply_npc_dialogue_response(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        npc_id: NpcId,
        topic_stack: &[String],
        choice_id: &str,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let Some(npc) = self.npcs.get(&npc_id) else {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::TargetMissing,
                events,
            );
        };
        if !adjacent(actor_position, npc.position) {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::TargetMissing,
                events,
            );
        }
        let Some(topic_id) = topic_stack.last() else {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        };
        let response = (choice_id != DIALOGUE_FALLBACK_CHOICE_ID)
            .then(|| {
                self.dialogue_topics.get(topic_id).and_then(|topic| {
                    topic
                        .responses
                        .iter()
                        .find(|response| response.response_id == choice_id)
                })
            })
            .flatten()
            .cloned();
        if choice_id != DIALOGUE_FALLBACK_CHOICE_ID && response.is_none() {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        if let Some(condition) = response
            .as_ref()
            .and_then(|response| response.condition.as_ref())
            && !self.dialogue_condition_matches(actor_id, condition)?
        {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        let next_topic_id = response
            .as_ref()
            .map_or("TALK_NONE", |response| response.next_topic_id.as_str());
        let Some(next_topic_stack) = advance_topic_stack(topic_stack, next_topic_id) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::InvalidInteractionChoice,
            )?);
            return Ok(());
        };
        let opinion_delta = response
            .as_ref()
            .map_or_else(NpcOpinionV1::default, |response| {
                response.opinion_delta.clone()
            });
        let Some(opinion) = self
            .npcs
            .get(&npc_id)
            .and_then(|npc| npc.social.get(&actor_id))
            .cloned()
            .unwrap_or_default()
            .checked_add(&opinion_delta)
            .filter(cdda_protocol::opinion_is_valid)
        else {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        };
        if let Some(response) = &response
            && !self.apply_dialogue_response_effects(
                actor_id,
                npc_id,
                interaction_id,
                sequence,
                &response.effects,
                events,
            )?
        {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        let npc = self.npcs.get_mut(&npc_id).ok_or(SimError::UnknownNpc)?;
        npc.social.insert(actor_id, opinion);
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        match next_topic_stack {
            Some(topic_stack) => self.request_npc_topic(actor_id, npc_id, topic_stack, events),
            None => {
                events.push(self.make_event(WorldEventKind::InteractionCanceled {
                    actor_id,
                    interaction_id,
                    reason: InteractionCancellationReasonV1::Completed,
                })?);
                Ok(())
            }
        }
    }

    fn request_npc_topic(
        &mut self,
        actor_id: ActorId,
        npc_id: NpcId,
        topic_stack: Vec<String>,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let topic_id = topic_stack.last().ok_or(SimError::InvalidNpcDialogue)?;
        let topic = self
            .dialogue_topics
            .get(topic_id)
            .cloned()
            .ok_or(SimError::InvalidNpcDialogue)?;
        let npc_name = self
            .npcs
            .get(&npc_id)
            .ok_or(SimError::UnknownNpc)?
            .name
            .clone();
        if self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .is_some()
        {
            return Err(SimError::InvalidNpcDialogue);
        }
        let mut choices = topic
            .responses
            .iter()
            .map(|response| {
                let matches = response.condition.as_ref().map_or(Ok(true), |condition| {
                    self.dialogue_condition_matches(actor_id, condition)
                })?;
                Ok(matches.then(|| InteractionChoiceV1 {
                    choice_id: response.response_id.clone(),
                    label: response.text.clone(),
                }))
            })
            .collect::<Result<Vec<Option<_>>, SimError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if choices.is_empty() {
            choices.push(InteractionChoiceV1 {
                choice_id: String::from(DIALOGUE_FALLBACK_CHOICE_ID),
                label: String::from(DIALOGUE_FALLBACK_CHOICE_LABEL),
            });
        }
        let interaction = PendingInteractionV1 {
            interaction_id: InteractionId::new(self.world_namespace, self.next_event_counter),
            prompt: render_dialogue_prompt(&npc_name, &topic.dynamic_line),
            choices,
            created_at_tick: self.tick,
            expires_at_tick: SimTick(
                self.tick
                    .0
                    .checked_add(DIALOGUE_LIFETIME_TICKS)
                    .ok_or(SimError::NumericOverflow)?,
            ),
            context: InteractionContextV1::NpcDialogue {
                npc_id,
                topic_stack,
            },
        };
        if !cdda_protocol::pending_interaction_is_valid(&interaction, actor_id) {
            return Err(SimError::InvalidNpcDialogue);
        }
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = Some(interaction.clone());
        events.push(self.make_event(WorldEventKind::InteractionRequested {
            actor_id,
            interaction,
        })?);
        Ok(())
    }

    pub(super) fn npc_dialogue_quit_choice(
        &self,
        interaction: &PendingInteractionV1,
    ) -> Option<String> {
        let InteractionContextV1::NpcDialogue { topic_stack, .. } = &interaction.context else {
            return None;
        };
        if interaction.choices.len() == 1 {
            return Some(interaction.choices[0].choice_id.clone());
        }
        let topic = self.dialogue_topics.get(topic_stack.last()?)?;
        interaction.choices.iter().find_map(|choice| {
            topic
                .responses
                .iter()
                .find(|response| response.response_id == choice.choice_id)
                .filter(|response| {
                    response.effects.is_empty() && response.next_topic_id == "TALK_DONE"
                })
                .map(|_| choice.choice_id.clone())
        })
    }

    fn invalidate_npc_dialogue(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        rejection: CommandRejection,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        events.push(self.make_event(WorldEventKind::InteractionCanceled {
            actor_id,
            interaction_id,
            reason: InteractionCancellationReasonV1::Invalidated,
        })?);
        events.push(self.rejection(actor_id, sequence, rejection)?);
        Ok(())
    }
}

pub(super) fn render_dialogue_prompt(npc_name: &str, dynamic_line: &str) -> String {
    if let Some(line) = dynamic_line.strip_prefix('&') {
        line.to_owned()
    } else if let Some(line) = dynamic_line.strip_prefix('*') {
        format!("{npc_name} {line}")
    } else {
        format!("{npc_name}: \"{dynamic_line}\"")
    }
}

fn advance_topic_stack(topic_stack: &[String], next_topic_id: &str) -> Option<Option<Vec<String>>> {
    if next_topic_id == "TALK_DONE" {
        return Some(None);
    }
    let mut next = topic_stack.to_vec();
    if next_topic_id == "TALK_NONE" {
        let category = next.last().map_or(-1, |topic| topic_category(topic));
        next.pop();
        while category != -1
            && next
                .last()
                .is_some_and(|topic| topic_category(topic) == category)
        {
            next.pop();
        }
        return Some((!next.is_empty()).then_some(next));
    }
    if next.len() >= MAX_DIALOGUE_TOPIC_STACK {
        return None;
    }
    next.push(next_topic_id.to_owned());
    Some(Some(next))
}

fn topic_category(topic: &str) -> i32 {
    match topic {
        "TALK_MISSION_START"
        | "TALK_MISSION_DESCRIBE"
        | "TALK_MISSION_OFFER"
        | "TALK_MISSION_ACCEPTED"
        | "TALK_MISSION_REJECTED"
        | "TALK_MISSION_ADVICE"
        | "TALK_MISSION_INQUIRE"
        | "TALK_MISSION_SUCCESS"
        | "TALK_MISSION_SUCCESS_LIE"
        | "TALK_MISSION_FAILURE"
        | "TALK_MISSION_REWARD"
        | "TALK_MISSION_END"
        | "TALK_MISSION_DESCRIBE_URGENT" => 1,
        "TALK_SHARE_EQUIPMENT" | "TALK_GIVE_EQUIPMENT" | "TALK_DENY_EQUIPMENT" => 2,
        "TALK_SUGGEST_FOLLOW" | "TALK_AGREE_FOLLOW" | "TALK_DENY_FOLLOW" => 3,
        "TALK_COMBAT_ENGAGEMENT" => 4,
        "TALK_COMBAT_COMMANDS" => 5,
        "TALK_TRAIN"
        | "TALK_TRAIN_START"
        | "TALK_TRAIN_FORCE"
        | "TALK_TRAIN_NPC_START"
        | "TALK_TRAIN_NPC_FORCE" => 6,
        "TALK_MISC_RULES" => 7,
        "TALK_AIM_RULES" => 8,
        "TALK_FRIEND" | "TALK_GIVE_ITEM" | "TALK_USE_ITEM" => 9,
        "TALK_SIZE_UP" | "TALK_ASSESS_PERSON" | "TALK_LOOK_AT" | "TALK_OPINION" | "TALK_SHOUT" => {
            99
        }
        _ => -1,
    }
}

fn adjacent(left: WorldPosition, right: WorldPosition) -> bool {
    left.z == right.z
        && left.x.abs_diff(right.x) <= 1
        && left.y.abs_diff(right.y) <= 1
        && left != right
}
