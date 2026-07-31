//! Persistent bounded server-owned interaction requests.

use cdda_protocol::{
    ActorId, CommandRejection, CommandSequence, InteractionCancellationReasonV1,
    InteractionChoiceV1, InteractionContextV1, InteractionId, ItemId, PendingInteractionV1,
    SimTick, WorldEvent, WorldEventKind,
};

use crate::{SimError, WorldState};

const MEDICAL_INTERACTION_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;

impl WorldState {
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
        let (outcome, item_id) = match &interaction.context {
            InteractionContextV1::MedicalBodyPart {
                item_id,
                item_type_id,
                activation_sequence,
            } => {
                let Some(healing) = self.healing_item_types.get(item_type_id).cloned() else {
                    return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
                };
                (
                    self.apply_healing_item(
                        actor_id,
                        *item_id,
                        &healing,
                        activation_sequence.0,
                        Some(&choice_id),
                    )?,
                    *item_id,
                )
            }
        };
        let Some(outcome) = outcome else {
            return self.invalidate_interaction(actor_id, sequence, interaction_id, events);
        };
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        events.push(self.make_event(WorldEventKind::MedicalItemApplied {
            actor_id,
            item_id,
            body_part_id: outcome.body_part_id,
            healed_hp: outcome.healed_hp,
            remaining_charges: outcome.remaining_charges,
        })?);
        Ok(())
    }

    pub(super) fn apply_interaction_cancel(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let pending_id = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .as_ref()
            .map(|pending| pending.interaction_id);
        let Some(pending_id) = pending_id else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::NoInteractionPending,
            )?);
            return Ok(());
        };
        if pending_id != interaction_id {
            events.push(self.rejection(actor_id, sequence, CommandRejection::StaleInteraction)?);
            return Ok(());
        }
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
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
                    .map(|interaction| (*actor_id, interaction.interaction_id))
            })
            .collect::<Vec<_>>();
        for (actor_id, interaction_id) in expired {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = None;
            events.push(self.make_event(WorldEventKind::InteractionCanceled {
                actor_id,
                interaction_id,
                reason: InteractionCancellationReasonV1::Expired,
            })?);
        }
        Ok(())
    }

    fn invalidate_interaction(
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
}
