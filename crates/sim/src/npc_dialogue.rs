//! Authoritative deterministic NPC spawning and dialogue execution.

use std::collections::BTreeMap;

use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, DialogueTopicV1, InteractionCancellationReasonV1,
    InteractionChoiceV1, InteractionContextV1, InteractionId, NpcId, NpcOpinionV1, NpcSnapshotV1,
    NpcSocialStateV1, NpcTemplateV1, PendingInteractionV1, SimTick, WorldEvent, WorldEventKind,
    WorldPosition, npc_dialogue_catalog_is_valid,
};
use serde::{Deserialize, Serialize};

use crate::{SimError, WorldState};

const DIALOGUE_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;

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

    pub(super) fn spawn_initial_npc_near(&mut self, origin: WorldPosition) -> Result<(), SimError> {
        if !self.npcs.is_empty() || self.npc_templates.is_empty() {
            return Ok(());
        }
        let template = self
            .npc_templates
            .get("apis")
            .cloned()
            .ok_or(SimError::InvalidNpcDialogue)?;
        let mut position = None;
        let maximum_radius =
            i8::try_from(cdda_protocol::SUBMAP_SIZE).map_err(|_| SimError::NumericOverflow)?;
        'radius: for radius in 1_i8..=maximum_radius {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let Some(candidate) = origin.checked_offset(dx, dy, 0) else {
                        continue;
                    };
                    if self.is_passable(candidate)
                        && self.actor_at(candidate).is_none()
                        && self.creature_at(candidate).is_none()
                        && self.npc_at(candidate).is_none()
                    {
                        position = Some(candidate);
                        break 'radius;
                    }
                }
            }
        }
        let position = position.ok_or(SimError::SpawnBlocked)?;
        let id = self.allocator.allocate_npc()?;
        self.npcs.insert(
            id,
            Npc {
                id,
                template_id: template.template_id,
                name: template.name,
                position,
                social: BTreeMap::new(),
            },
        );
        Ok(())
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
        let Some(npc) = self.npcs.get(&npc_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::TargetMissing)?);
            return Ok(());
        };
        if !adjacent(actor_position, npc.position) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::TargetOutOfRange)?);
            return Ok(());
        }
        let topic_id = self
            .npc_templates
            .get(&npc.template_id)
            .ok_or(SimError::InvalidNpcDialogue)?
            .chat_topic_id
            .clone();
        self.request_npc_topic(actor_id, npc_id, topic_id, events)
    }

    pub(super) fn npc_dialogue_action_cost(&self, actor_id: ActorId, npc_id: NpcId) -> i64 {
        self.actors
            .get(&actor_id)
            .zip(self.npcs.get(&npc_id))
            .filter(|(actor, npc)| adjacent(actor.position, npc.position))
            .map_or(0, |_| i64::from(crate::ACTOR_ACTION_THRESHOLD))
    }

    pub(super) fn apply_npc_dialogue_response(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        npc_id: NpcId,
        topic_id: &str,
        choice_id: &str,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let Some(npc) = self.npcs.get(&npc_id) else {
            return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
        };
        if !adjacent(actor_position, npc.position) {
            return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
        }
        let Some(response) = self
            .dialogue_topics
            .get(topic_id)
            .and_then(|topic| {
                topic
                    .responses
                    .iter()
                    .find(|response| response.response_id == choice_id)
            })
            .cloned()
        else {
            return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
        };
        let Some(opinion) = self
            .npcs
            .get(&npc_id)
            .and_then(|npc| npc.social.get(&actor_id))
            .cloned()
            .unwrap_or_default()
            .checked_add(&response.opinion_delta)
            .filter(cdda_protocol::opinion_is_valid)
        else {
            return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
        };
        if !self.apply_dialogue_response_effects(
            actor_id,
            npc_id,
            interaction_id,
            sequence,
            &response.effects,
            events,
        )? {
            return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
        }
        let npc = self.npcs.get_mut(&npc_id).ok_or(SimError::UnknownNpc)?;
        npc.social.insert(actor_id, opinion);
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        if response.next_topic_id == "TALK_DONE" {
            events.push(self.make_event(WorldEventKind::InteractionCanceled {
                actor_id,
                interaction_id,
                reason: InteractionCancellationReasonV1::Completed,
            })?);
            return Ok(());
        }
        self.request_npc_topic(actor_id, npc_id, response.next_topic_id, events)
    }

    fn request_npc_topic(
        &mut self,
        actor_id: ActorId,
        npc_id: NpcId,
        topic_id: String,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let topic = self
            .dialogue_topics
            .get(&topic_id)
            .cloned()
            .ok_or(SimError::InvalidNpcDialogue)?;
        let npc_name = self
            .npcs
            .get(&npc_id)
            .ok_or(SimError::UnknownNpc)?
            .name
            .clone();
        if let Some(previous) = self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .take()
        {
            events.push(self.make_event(WorldEventKind::InteractionCanceled {
                actor_id,
                interaction_id: previous.interaction_id,
                reason: InteractionCancellationReasonV1::Replaced,
            })?);
        }
        let choices = topic
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
            .collect();
        let interaction = PendingInteractionV1 {
            interaction_id: InteractionId::new(self.world_namespace, self.next_event_counter),
            prompt: format!("{npc_name}: {}", topic.dynamic_line),
            choices,
            created_at_tick: self.tick,
            expires_at_tick: SimTick(
                self.tick
                    .0
                    .checked_add(DIALOGUE_LIFETIME_TICKS)
                    .ok_or(SimError::NumericOverflow)?,
            ),
            context: InteractionContextV1::NpcDialogue { npc_id, topic_id },
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
}

fn adjacent(left: WorldPosition, right: WorldPosition) -> bool {
    left.z == right.z
        && left.x.abs_diff(right.x) <= 1
        && left.y.abs_diff(right.y) <= 1
        && left != right
}
