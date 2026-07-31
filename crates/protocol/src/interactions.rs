//! Bounded server-owned interaction requests and stable replies.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    ActorId, CommandSequence, EocEffectV1, InteractionId, ItemId, MAX_DIALOGUE_TOPIC_STACK,
    MissionId, NpcId, SimTick, eoc_confirmation_branches_are_valid,
};

pub const MAX_INTERACTION_CHOICES: usize = 64;
pub const MAX_INTERACTION_PROMPT_BYTES: usize = 4 * 1_024;
pub const MAX_INTERACTION_CHOICE_ID_BYTES: usize = 512;
pub const MAX_INTERACTION_CHOICE_LABEL_BYTES: usize = 1_024;
pub const MAX_INTERACTION_LIFETIME_TICKS: u64 = 24 * 60 * 60 * SimTick::HZ;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InteractionChoiceV1 {
    pub choice_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum InteractionContextV1 {
    MedicalBodyPart {
        item_id: ItemId,
        item_type_id: String,
        activation_sequence: CommandSequence,
    },
    EocConfirmation {
        item_id: ItemId,
        item_type_id: String,
        activation_sequence: CommandSequence,
        default: bool,
        accept_effects: Vec<EocEffectV1>,
        decline_effects: Vec<EocEffectV1>,
    },
    NpcDialogue {
        npc_id: NpcId,
        topic_stack: Vec<String>,
        selected_mission_id: Option<MissionId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PendingInteractionV1 {
    pub interaction_id: InteractionId,
    pub prompt: String,
    pub choices: Vec<InteractionChoiceV1>,
    pub created_at_tick: SimTick,
    pub expires_at_tick: SimTick,
    pub context: InteractionContextV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum InteractionCancellationReasonV1 {
    Replaced,
    Expired,
    ClientCanceled,
    Invalidated,
    Completed,
}

#[must_use]
pub fn pending_interaction_is_valid(interaction: &PendingInteractionV1, actor_id: ActorId) -> bool {
    let mut choice_ids = BTreeSet::new();
    interaction.interaction_id.counter() > 0
        && interaction.interaction_id.world_namespace() == actor_id.world_namespace()
        && !interaction.prompt.is_empty()
        && interaction.prompt.len() <= MAX_INTERACTION_PROMPT_BYTES
        && !interaction.prompt.chars().any(char::is_control)
        && (1..=MAX_INTERACTION_CHOICES).contains(&interaction.choices.len())
        && interaction.choices.iter().all(|choice| {
            valid_id(&choice.choice_id, MAX_INTERACTION_CHOICE_ID_BYTES)
                && !choice.label.is_empty()
                && choice.label.len() <= MAX_INTERACTION_CHOICE_LABEL_BYTES
                && !choice.label.chars().any(char::is_control)
                && choice_ids.insert(choice.choice_id.as_str())
        })
        && interaction.created_at_tick < interaction.expires_at_tick
        && interaction
            .expires_at_tick
            .0
            .checked_sub(interaction.created_at_tick.0)
            .is_some_and(|duration| duration <= MAX_INTERACTION_LIFETIME_TICKS)
        && match &interaction.context {
            InteractionContextV1::MedicalBodyPart {
                item_id,
                item_type_id,
                activation_sequence,
            } => {
                item_id.counter() > 0
                    && item_id.world_namespace() == actor_id.world_namespace()
                    && valid_id(item_type_id, MAX_INTERACTION_CHOICE_ID_BYTES)
                    && activation_sequence.0 > 0
            }
            InteractionContextV1::EocConfirmation {
                item_id,
                item_type_id,
                activation_sequence,
                accept_effects,
                decline_effects,
                ..
            } => {
                item_id.counter() > 0
                    && item_id.world_namespace() == actor_id.world_namespace()
                    && valid_id(item_type_id, MAX_INTERACTION_CHOICE_ID_BYTES)
                    && activation_sequence.0 > 0
                    && interaction.choices.as_slice()
                        == [
                            InteractionChoiceV1 {
                                choice_id: String::from("yes"),
                                label: String::from("Yes"),
                            },
                            InteractionChoiceV1 {
                                choice_id: String::from("no"),
                                label: String::from("No"),
                            },
                        ]
                    && eoc_confirmation_branches_are_valid(accept_effects, decline_effects)
            }
            InteractionContextV1::NpcDialogue {
                npc_id,
                topic_stack,
                selected_mission_id,
            } => {
                npc_id.counter() > 0
                    && npc_id.world_namespace() == actor_id.world_namespace()
                    && (1..=MAX_DIALOGUE_TOPIC_STACK).contains(&topic_stack.len())
                    && topic_stack.iter().all(|topic_id| {
                        !matches!(topic_id.as_str(), "TALK_NONE" | "TALK_DONE")
                            && valid_id(topic_id, MAX_INTERACTION_CHOICE_ID_BYTES)
                    })
                    && selected_mission_id.is_none_or(|mission_id| {
                        mission_id.counter() > 0
                            && mission_id.world_namespace() == actor_id.world_namespace()
                    })
            }
        }
}

fn valid_id(id: &str, maximum_bytes: usize) -> bool {
    !id.is_empty() && id.len() <= maximum_bytes && !id.chars().any(char::is_control)
}
