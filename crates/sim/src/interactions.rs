//! Persistent bounded server-owned interaction requests.

use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, EocEffectV1, InteractionCancellationReasonV1,
    InteractionChoiceV1, InteractionContextV1, InteractionId, ItemId, PendingInteractionV1,
    SimTick, WorldEvent, WorldEventKind,
};

use crate::{SimError, WorldState};

const MEDICAL_INTERACTION_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;
const EOC_CONFIRMATION_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;
const PLACE_MONSTER_INTERACTION_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;

impl WorldState {
    pub(super) fn resolve_disconnected_eoc_confirmations(
        &mut self,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let pending = self
            .actors
            .iter()
            .filter(|(_actor_id, actor)| !actor.connected)
            .filter_map(|(actor_id, actor)| {
                let interaction = actor.pending_interaction.as_ref()?;
                let InteractionContextV1::EocConfirmation {
                    item_id,
                    item_type_id,
                    activation_sequence,
                    default,
                    accept_effects,
                    decline_effects,
                } = &interaction.context
                else {
                    return None;
                };
                Some((
                    *actor_id,
                    interaction.interaction_id,
                    *activation_sequence,
                    *item_id,
                    item_type_id.clone(),
                    if *default {
                        accept_effects.clone()
                    } else {
                        decline_effects.clone()
                    },
                ))
            })
            .collect::<Vec<_>>();
        for (actor_id, interaction_id, activation_sequence, item_id, item_type_id, effects) in
            pending
        {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = None;
            if !self.apply_eoc_confirmation_response(
                actor_id,
                activation_sequence,
                activation_sequence,
                item_id,
                &item_type_id,
                &effects,
                false,
                events,
            )? {
                events.push(self.make_event(WorldEventKind::InteractionCanceled {
                    actor_id,
                    interaction_id,
                    reason: InteractionCancellationReasonV1::Invalidated,
                })?);
            }
        }
        Ok(())
    }

    pub(super) fn request_medical_body_part(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        item_id: ItemId,
        item_type_id: String,
        body_part_ids: Vec<String>,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        if body_part_ids.len() < 2 {
            return Err(SimError::InvalidItem);
        }
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
        let interaction_id = InteractionId::new(self.world_namespace, self.next_event_counter);
        let interaction = PendingInteractionV1 {
            interaction_id,
            prompt: String::from("Choose the body part to treat."),
            choices: body_part_ids
                .into_iter()
                .map(|body_part_id| InteractionChoiceV1 {
                    label: body_part_id.replace('_', " "),
                    choice_id: body_part_id,
                })
                .collect(),
            created_at_tick: self.tick,
            expires_at_tick: SimTick(
                self.tick
                    .0
                    .checked_add(MEDICAL_INTERACTION_LIFETIME_TICKS)
                    .ok_or(SimError::NumericOverflow)?,
            ),
            context: InteractionContextV1::MedicalBodyPart {
                item_id,
                item_type_id,
                activation_sequence: sequence,
            },
        };
        if !cdda_protocol::pending_interaction_is_valid(&interaction, actor_id) {
            return Err(SimError::InvalidItem);
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

    pub(super) fn request_eoc_confirmation(
        &mut self,
        actor_id: ActorId,
        activation_sequence: CommandSequence,
        item_id: ItemId,
        item_type_id: String,
        prompt: String,
        default: bool,
        accept_effects: Vec<EocEffectV1>,
        decline_effects: Vec<EocEffectV1>,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
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
        let interaction_id = InteractionId::new(self.world_namespace, self.next_event_counter);
        let interaction = PendingInteractionV1 {
            interaction_id,
            prompt,
            choices: vec![
                InteractionChoiceV1 {
                    choice_id: String::from("yes"),
                    label: String::from("Yes"),
                },
                InteractionChoiceV1 {
                    choice_id: String::from("no"),
                    label: String::from("No"),
                },
            ],
            created_at_tick: self.tick,
            expires_at_tick: SimTick(
                self.tick
                    .0
                    .checked_add(EOC_CONFIRMATION_LIFETIME_TICKS)
                    .ok_or(SimError::NumericOverflow)?,
            ),
            context: InteractionContextV1::EocConfirmation {
                item_id,
                item_type_id,
                activation_sequence,
                default,
                accept_effects,
                decline_effects,
            },
        };
        if !cdda_protocol::pending_interaction_is_valid(&interaction, actor_id) {
            return Err(SimError::InvalidItem);
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

    pub(super) fn request_place_monster_position(
        &mut self,
        actor_id: ActorId,
        activation_sequence: CommandSequence,
        item_id: ItemId,
        item_type_id: String,
        monster_type_id: &str,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
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
        let interaction_id = InteractionId::new(self.world_namespace, self.next_event_counter);
        let interaction = PendingInteractionV1 {
            interaction_id,
            prompt: format!("Place the {monster_type_id} where?"),
            choices: cdda_protocol::place_monster_interaction_choices(),
            created_at_tick: self.tick,
            expires_at_tick: SimTick(
                self.tick
                    .0
                    .checked_add(PLACE_MONSTER_INTERACTION_LIFETIME_TICKS)
                    .ok_or(SimError::NumericOverflow)?,
            ),
            context: InteractionContextV1::PlaceMonster {
                item_id,
                item_type_id,
                activation_sequence,
            },
        };
        if !cdda_protocol::pending_interaction_is_valid(&interaction, actor_id) {
            return Err(SimError::InvalidItem);
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

    pub(super) fn interaction_action_cost(
        &self,
        actor_id: ActorId,
        interaction_id: InteractionId,
        choice_id: &str,
    ) -> Result<i64, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(interaction) = actor.pending_interaction.as_ref().filter(|interaction| {
            interaction.interaction_id == interaction_id
                && interaction.expires_at_tick > self.tick
                && interaction
                    .choices
                    .iter()
                    .any(|choice| choice.choice_id == choice_id)
        }) else {
            return Ok(0);
        };
        match &interaction.context {
            InteractionContextV1::MedicalBodyPart { item_type_id, .. } => self
                .healing_item_types
                .get(item_type_id)
                .map_or(Ok(0), |healing| {
                    i64::from(healing.move_cost_moves)
                        .checked_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
                        .ok_or(SimError::NumericOverflow)
                }),
            InteractionContextV1::EocConfirmation { .. } => Ok(0),
            InteractionContextV1::PlaceMonster { item_id, .. } => self
                .place_monster_action_cost(actor_id, *item_id, Some(choice_id))
                .map(|cost| cost.unwrap_or(0)),
            InteractionContextV1::NpcDialogue { .. } => Ok(0),
        }
    }

    pub(super) fn apply_interaction_response(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        choice_id: String,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let Some(interaction) = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .clone()
        else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::NoInteractionPending,
            )?);
            return Ok(());
        };
        if interaction.interaction_id != interaction_id {
            events.push(self.rejection(actor_id, sequence, CommandRejection::StaleInteraction)?);
            return Ok(());
        }
        if !interaction
            .choices
            .iter()
            .any(|choice| choice.choice_id == choice_id)
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::InvalidInteractionChoice,
            )?);
            return Ok(());
        }
        match &interaction.context {
            InteractionContextV1::MedicalBodyPart {
                item_id,
                item_type_id,
                activation_sequence,
            } => {
                let Some(healing) = self.healing_item_types.get(item_type_id).cloned() else {
                    return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
                };
                let outcome = self.apply_healing_item(
                    actor_id,
                    *item_id,
                    &healing,
                    activation_sequence.0,
                    Some(&choice_id),
                )?;
                let Some(outcome) = outcome else {
                    return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
                };
                self.actors
                    .get_mut(&actor_id)
                    .ok_or(SimError::UnknownActor)?
                    .pending_interaction = None;
                events.push(self.make_event(WorldEventKind::MedicalItemApplied {
                    actor_id,
                    item_id: *item_id,
                    body_part_id: outcome.body_part_id,
                    healed_hp: outcome.healed_hp,
                    remaining_charges: outcome.remaining_charges,
                })?);
            }
            InteractionContextV1::EocConfirmation {
                item_id,
                item_type_id,
                activation_sequence,
                accept_effects,
                decline_effects,
                ..
            } => {
                let selected = if choice_id == "yes" {
                    accept_effects
                } else {
                    decline_effects
                };
                self.actors
                    .get_mut(&actor_id)
                    .ok_or(SimError::UnknownActor)?
                    .pending_interaction = None;
                if !self.apply_eoc_confirmation_response(
                    actor_id,
                    *activation_sequence,
                    sequence,
                    *item_id,
                    item_type_id,
                    selected,
                    true,
                    events,
                )? {
                    return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
                }
            }
            InteractionContextV1::PlaceMonster {
                item_id,
                item_type_id,
                activation_sequence,
            } => {
                if !self.apply_place_monster_item(
                    actor_id,
                    *activation_sequence,
                    *item_id,
                    item_type_id,
                    Some(&choice_id),
                    events,
                )? {
                    return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
                }
                self.actors
                    .get_mut(&actor_id)
                    .ok_or(SimError::UnknownActor)?
                    .pending_interaction = None;
            }
            InteractionContextV1::NpcDialogue {
                npc_id,
                topic_stack,
                selected_mission_id,
            } => self.apply_npc_dialogue_response(
                actor_id,
                sequence,
                interaction_id,
                *npc_id,
                topic_stack,
                *selected_mission_id,
                &choice_id,
                events,
            )?,
        }
        Ok(())
    }

    pub(super) fn apply_interaction_cancel(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let pending = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .as_ref()
            .cloned();
        let Some(pending) = pending else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::NoInteractionPending,
            )?);
            return Ok(());
        };
        if pending.interaction_id != interaction_id {
            events.push(self.rejection(actor_id, sequence, CommandRejection::StaleInteraction)?);
            return Ok(());
        }
        if let InteractionContextV1::NpcDialogue {
            npc_id,
            topic_stack,
            selected_mission_id,
        } = &pending.context
        {
            let Some(choice_id) = self.npc_dialogue_quit_choice(&pending) else {
                events.push(self.rejection(
                    actor_id,
                    sequence,
                    CommandRejection::InvalidInteractionChoice,
                )?);
                return Ok(());
            };
            return self.apply_npc_dialogue_response(
                actor_id,
                sequence,
                interaction_id,
                *npc_id,
                topic_stack,
                *selected_mission_id,
                &choice_id,
                events,
            );
        }
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        if let InteractionContextV1::EocConfirmation {
            item_id,
            item_type_id,
            activation_sequence,
            default,
            accept_effects,
            decline_effects,
        } = pending.context
        {
            let effects = if default {
                accept_effects
            } else {
                decline_effects
            };
            if self.apply_eoc_confirmation_response(
                actor_id,
                activation_sequence,
                sequence,
                item_id,
                &item_type_id,
                &effects,
                false,
                events,
            )? {
                return Ok(());
            }
        }
        events.push(self.make_event(WorldEventKind::InteractionCanceled {
            actor_id,
            interaction_id,
            reason: InteractionCancellationReasonV1::ClientCanceled,
        })?);
        Ok(())
    }

    pub(super) fn expire_interactions(
        &mut self,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let expired = self
            .actors
            .iter()
            .filter_map(|(actor_id, actor)| {
                actor
                    .pending_interaction
                    .as_ref()
                    .filter(|interaction| interaction.expires_at_tick <= self.tick)
                    .map(|interaction| (*actor_id, interaction.clone()))
            })
            .collect::<Vec<_>>();
        for (actor_id, interaction) in expired {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = None;
            let resolved = match interaction.context {
                InteractionContextV1::EocConfirmation {
                    item_id,
                    item_type_id,
                    activation_sequence,
                    default,
                    accept_effects,
                    decline_effects,
                } => {
                    let effects = if default {
                        accept_effects
                    } else {
                        decline_effects
                    };
                    self.apply_eoc_confirmation_response(
                        actor_id,
                        activation_sequence,
                        activation_sequence,
                        item_id,
                        &item_type_id,
                        &effects,
                        false,
                        events,
                    )?
                }
                InteractionContextV1::MedicalBodyPart { .. } => false,
                InteractionContextV1::PlaceMonster { .. } => false,
                InteractionContextV1::NpcDialogue { .. } => false,
            };
            if !resolved {
                events.push(self.make_event(WorldEventKind::InteractionCanceled {
                    actor_id,
                    interaction_id: interaction.interaction_id,
                    reason: InteractionCancellationReasonV1::Expired,
                })?);
            }
        }
        Ok(())
    }

    pub(super) fn invalidate_interaction(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
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
        events.push(self.rejection(actor_id, sequence, CommandRejection::ItemNotActivatable)?);
        Ok(())
    }

    pub(super) fn resolve_pending_before_npc_dialogue(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let pending = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .clone();
        let Some(pending) = pending else {
            return Ok(true);
        };
        let InteractionContextV1::EocConfirmation {
            item_id,
            item_type_id,
            activation_sequence,
            default,
            accept_effects,
            decline_effects,
        } = &pending.context
        else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::ActorBusy)?);
            return Ok(false);
        };
        let effects = if *default {
            accept_effects
        } else {
            decline_effects
        };
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        if !self.apply_eoc_confirmation_response(
            actor_id,
            *activation_sequence,
            sequence,
            *item_id,
            item_type_id,
            effects,
            false,
            events,
        )? {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = Some(pending);
            events.push(self.rejection(actor_id, sequence, CommandRejection::ActorBusy)?);
            return Ok(false);
        }
        Ok(self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .is_none())
    }
}
