//! Bounded authoritative effect-on-condition interpreter.

use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorId, CommandRejection, CommandSequence, EocConditionV1,
    EocDefinitionV1, EocDelayV1, EocEffectV1, EocItemUseTypeV1, EocStringValueV1, ItemId,
    MAX_ACTOR_SCHEDULED_EOCS, MAX_EOC_ACTOR_VARIABLES, ScheduledEocV1, SimTick, WorldEvent,
    WorldEventKind, eoc_catalog_is_valid,
};
use rand_chacha::ChaCha8Rng;
use rand_core::Rng;

use crate::{SimError, WorldState, inclusive_rng_u64};

const MAX_EOC_ACTIVATIONS_PER_COMMAND: usize = 4_096;
const MAX_EOC_OPERATIONS_PER_COMMAND: usize = 16_384;
const MAX_SCHEDULED_EOC_ACTIVATIONS_PER_TICK: usize = 256;
const MAX_RECURRING_EOC_REACTIVATION_CHECKS_PER_TICK: usize = 256;

impl WorldState {
    pub fn register_eoc_catalog(
        &mut self,
        definitions: Vec<EocDefinitionV1>,
        item_use_types: Vec<EocItemUseTypeV1>,
    ) -> Result<(), SimError> {
        if self.tick != SimTick(0)
            || !self.actors.is_empty()
            || !eoc_catalog_is_valid(&definitions, &item_use_types)
            || definitions
                .iter()
                .any(|definition| !eoc_body_parts_are_valid(definition, &self.actor_anatomy))
        {
            return Err(SimError::InvalidItem);
        }
        self.eoc_definitions = definitions
            .into_iter()
            .map(|definition| (definition.eoc_id.clone(), definition))
            .collect();
        self.eoc_item_use_types = item_use_types
            .into_iter()
            .map(|profile| (profile.item_type_id.clone(), profile))
            .collect();
        Ok(())
    }

    pub(super) fn apply_eoc_item_use(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        item_id: ItemId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let Some(item) = actor.inventory.get(&item_id) else {
            return Ok(false);
        };
        let Some(profile) = self.eoc_item_use_types.get(&item.type_id).cloned() else {
            return Ok(false);
        };
        if (profile.need_worn && !actor.worn.contains(&item_id))
            || (profile.need_wielding && actor.wielded != Some(item_id))
            || (profile.consume && item.charges <= 0)
        {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::ItemNotActivatable,
            )?);
            return Ok(true);
        }

        let mut execution = EocExecution {
            effects: actor.effects.clone(),
            variables: actor.eoc_variables.clone(),
            next_schedule_sequence: actor.next_eoc_schedule_sequence,
            scheduled_eocs: actor.scheduled_eocs.clone(),
            inactive_recurring_eocs: actor.inactive_recurring_eocs.clone(),
            messages: Vec::new(),
            activations: 0,
            operations: 0,
            tick: self.tick,
            rng: self.named_rng(
                b"eoc-item-activation",
                &[actor_id.as_u128(), item_id.as_u128()],
                sequence.0,
            ),
        };
        for eoc_id in &profile.eoc_ids {
            if execute_eoc(&self.eoc_definitions, eoc_id, &mut execution, 0).is_err() {
                events.push(self.rejection(
                    actor_id,
                    sequence,
                    CommandRejection::ItemNotActivatable,
                )?);
                return Ok(true);
            }
        }
        if execution.effects.len() > 1_024 {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::ItemNotActivatable,
            )?);
            return Ok(true);
        }
        execution.effects.sort_by(|left, right| {
            (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
        });
        execution
            .scheduled_eocs
            .sort_by_key(|entry| (entry.due_tick, entry.sequence));
        let remaining_charges = {
            let actor = self
                .actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?;
            actor.effects = execution.effects;
            actor.eoc_variables = execution.variables;
            actor.next_eoc_schedule_sequence = execution.next_schedule_sequence;
            actor.scheduled_eocs = execution.scheduled_eocs;
            actor.inactive_recurring_eocs = execution.inactive_recurring_eocs;
            let item = actor
                .inventory
                .get_mut(&item_id)
                .ok_or(SimError::UnknownItem)?;
            if profile.consume {
                item.charges = item.charges.saturating_sub(1);
            }
            let remaining = item.charges;
            if profile.consume && remaining == 0 {
                actor.inventory.remove(&item_id);
                actor.worn.retain(|worn| *worn != item_id);
                if actor.wielded == Some(item_id) {
                    actor.wielded = None;
                }
            }
            remaining
        };
        for text in execution.messages {
            events.push(self.make_event(WorldEventKind::EocMessage { actor_id, text })?);
        }
        events.push(self.make_event(WorldEventKind::EocItemActivated {
            actor_id,
            item_id,
            remaining_charges,
        })?);
        Ok(true)
    }

    pub(super) fn advance_scheduled_eocs(
        &mut self,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        self.reactivate_recurring_eocs()?;
        let mut due = self
            .actors
            .iter()
            .flat_map(|(actor_id, actor)| {
                actor
                    .scheduled_eocs
                    .iter()
                    .filter(|entry| entry.due_tick <= self.tick)
                    .map(|entry| (*actor_id, entry.due_tick, entry.sequence))
            })
            .collect::<Vec<_>>();
        due.sort_by_key(|(actor_id, due_tick, sequence)| (*due_tick, *actor_id, *sequence));
        due.truncate(MAX_SCHEDULED_EOC_ACTIVATIONS_PER_TICK);

        for (actor_id, _due_tick, sequence) in due {
            let rng = self.named_rng(b"scheduled-eoc-activation", &[actor_id.as_u128()], sequence);
            let (entry, mut execution) = {
                let actor = self
                    .actors
                    .get_mut(&actor_id)
                    .ok_or(SimError::UnknownActor)?;
                let index = actor
                    .scheduled_eocs
                    .iter()
                    .position(|entry| entry.sequence == sequence)
                    .ok_or(SimError::InvalidItem)?;
                let entry = actor.scheduled_eocs.remove(index);
                let execution = EocExecution {
                    effects: actor.effects.clone(),
                    variables: actor.eoc_variables.clone(),
                    next_schedule_sequence: actor.next_eoc_schedule_sequence,
                    scheduled_eocs: actor.scheduled_eocs.clone(),
                    inactive_recurring_eocs: actor.inactive_recurring_eocs.clone(),
                    messages: Vec::new(),
                    activations: 0,
                    operations: 0,
                    tick: self.tick,
                    rng,
                };
                (entry, execution)
            };
            let Ok(condition_matches) =
                execute_eoc(&self.eoc_definitions, &entry.eoc_id, &mut execution, 0)
            else {
                continue;
            };
            let Some(definition) = self.eoc_definitions.get(&entry.eoc_id) else {
                continue;
            };
            if let Some(recurrence) = definition.recurrence {
                execution
                    .inactive_recurring_eocs
                    .retain(|eoc_id| eoc_id != &entry.eoc_id);
                let should_deactivate = if !condition_matches && definition.false_effects.is_empty()
                {
                    match definition.deactivate_condition.as_ref() {
                        Some(condition) => match evaluate_condition(
                            condition,
                            &execution.effects,
                            &execution.variables,
                            &mut execution.operations,
                        ) {
                            Ok(matches) => matches,
                            Err(_) => continue,
                        },
                        None => false,
                    }
                } else {
                    false
                };
                if should_deactivate {
                    execution.inactive_recurring_eocs.push(entry.eoc_id.clone());
                    execution.inactive_recurring_eocs.sort();
                    execution.inactive_recurring_eocs.dedup();
                } else if schedule_eoc(&mut execution, &entry.eoc_id, recurrence).is_err() {
                    continue;
                }
            }
            if execution.effects.len() > 1_024 {
                continue;
            }
            execution.effects.sort_by(|left, right| {
                (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
            });
            execution
                .scheduled_eocs
                .sort_by_key(|entry| (entry.due_tick, entry.sequence));
            let actor = self
                .actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?;
            actor.effects = execution.effects;
            actor.eoc_variables = execution.variables;
            actor.next_eoc_schedule_sequence = execution.next_schedule_sequence;
            actor.scheduled_eocs = execution.scheduled_eocs;
            actor.inactive_recurring_eocs = execution.inactive_recurring_eocs;
            for text in execution.messages {
                events.push(self.make_event(WorldEventKind::EocMessage { actor_id, text })?);
            }
        }
        Ok(())
    }

    pub(super) fn initial_recurring_eoc_schedule(
        &self,
        actor_id: ActorId,
    ) -> Result<(u64, Vec<ScheduledEocV1>), SimError> {
        let mut execution = EocExecution {
            effects: Vec::new(),
            variables: BTreeMap::new(),
            next_schedule_sequence: 0,
            scheduled_eocs: Vec::new(),
            inactive_recurring_eocs: Vec::new(),
            messages: Vec::new(),
            activations: 0,
            operations: 0,
            tick: self.tick,
            rng: self.named_rng(b"recurring-eoc-enrollment", &[actor_id.as_u128()], 0),
        };
        for definition in self.eoc_definitions.values() {
            if let Some(recurrence) = definition.recurrence {
                schedule_eoc(&mut execution, &definition.eoc_id, recurrence)?;
            }
        }
        execution
            .scheduled_eocs
            .sort_by_key(|entry| (entry.due_tick, entry.sequence));
        Ok((execution.next_schedule_sequence, execution.scheduled_eocs))
    }

    fn reactivate_recurring_eocs(&mut self) -> Result<(), SimError> {
        let inactive = self
            .actors
            .iter()
            .flat_map(|(actor_id, actor)| {
                actor
                    .inactive_recurring_eocs
                    .iter()
                    .map(|eoc_id| (*actor_id, eoc_id.clone()))
            })
            .collect::<Vec<_>>();
        if inactive.is_empty() {
            return Ok(());
        }
        let start = usize::try_from(self.tick.0 % inactive.len() as u64)
            .map_err(|_| SimError::NumericOverflow)?;
        let candidates = inactive
            .iter()
            .cycle()
            .skip(start)
            .take(
                inactive
                    .len()
                    .min(MAX_RECURRING_EOC_REACTIVATION_CHECKS_PER_TICK),
            )
            .cloned()
            .collect::<Vec<_>>();
        let mut operations = 0;
        for (actor_id, eoc_id) in candidates {
            let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
            let mut execution = EocExecution {
                effects: actor.effects.clone(),
                variables: actor.eoc_variables.clone(),
                next_schedule_sequence: actor.next_eoc_schedule_sequence,
                scheduled_eocs: actor.scheduled_eocs.clone(),
                inactive_recurring_eocs: actor.inactive_recurring_eocs.clone(),
                messages: Vec::new(),
                activations: 0,
                operations,
                tick: self.tick,
                rng: self.named_rng(
                    b"recurring-eoc-reactivation",
                    &[actor_id.as_u128()],
                    actor.next_eoc_schedule_sequence,
                ),
            };
            let definition = self
                .eoc_definitions
                .get(&eoc_id)
                .ok_or(SimError::InvalidItem)?;
            let recurrence = definition.recurrence.ok_or(SimError::InvalidItem)?;
            let deactivate_condition = definition
                .deactivate_condition
                .as_ref()
                .ok_or(SimError::InvalidItem)?;
            let Ok(still_deactivated) = evaluate_condition(
                deactivate_condition,
                &execution.effects,
                &execution.variables,
                &mut execution.operations,
            ) else {
                break;
            };
            operations = execution.operations;
            if still_deactivated {
                continue;
            }
            schedule_eoc(&mut execution, &eoc_id, recurrence)?;
            execution
                .inactive_recurring_eocs
                .retain(|inactive_id| inactive_id != &eoc_id);
            execution
                .scheduled_eocs
                .sort_by_key(|entry| (entry.due_tick, entry.sequence));
            let actor = self
                .actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?;
            actor.next_eoc_schedule_sequence = execution.next_schedule_sequence;
            actor.scheduled_eocs = execution.scheduled_eocs;
            actor.inactive_recurring_eocs = execution.inactive_recurring_eocs;
        }
        Ok(())
    }
}

struct EocExecution {
    effects: Vec<ActorEffectSnapshotV1>,
    variables: BTreeMap<String, String>,
    next_schedule_sequence: u64,
    scheduled_eocs: Vec<ScheduledEocV1>,
    inactive_recurring_eocs: Vec<String>,
    messages: Vec<String>,
    activations: usize,
    operations: usize,
    tick: SimTick,
    rng: ChaCha8Rng,
}

fn execute_eoc(
    catalog: &BTreeMap<String, EocDefinitionV1>,
    eoc_id: &str,
    execution: &mut EocExecution,
    depth: usize,
) -> Result<bool, SimError> {
    if depth >= cdda_protocol::MAX_EOC_TREE_DEPTH
        || execution.activations >= MAX_EOC_ACTIVATIONS_PER_COMMAND
        || execution.operations >= MAX_EOC_OPERATIONS_PER_COMMAND
    {
        return Err(SimError::InvalidItem);
    }
    execution.activations += 1;
    execution.operations += 1;
    let definition = catalog.get(eoc_id).ok_or(SimError::InvalidItem)?;
    let condition_matches = match definition.condition.as_ref() {
        Some(condition) => evaluate_condition(
            condition,
            &execution.effects,
            &execution.variables,
            &mut execution.operations,
        )?,
        None => true,
    };
    let selected = if condition_matches {
        &definition.effects
    } else {
        &definition.false_effects
    };
    execute_effects(catalog, selected, execution, depth + 1)?;
    Ok(condition_matches)
}

fn evaluate_condition(
    condition: &EocConditionV1,
    effects: &[ActorEffectSnapshotV1],
    variables: &BTreeMap<String, String>,
    operations: &mut usize,
) -> Result<bool, SimError> {
    *operations = operations.saturating_add(1);
    if *operations > MAX_EOC_OPERATIONS_PER_COMMAND {
        return Err(SimError::InvalidItem);
    }
    Ok(match condition {
        EocConditionV1::Constant(value) => *value,
        EocConditionV1::HasEffect {
            effect_id,
            body_part_id,
        } => effects.iter().any(|effect| {
            effect.effect_id == *effect_id
                && body_part_id
                    .as_ref()
                    .is_none_or(|body_part_id| effect.body_part_id.as_ref() == Some(body_part_id))
        }),
        EocConditionV1::CompareString(values) => {
            let mut seen = BTreeSet::new();
            let mut matches = false;
            for value in values {
                let value = match value {
                    EocStringValueV1::Literal(value) => value.as_str(),
                    EocStringValueV1::ActorVariable(variable_id) => {
                        variables.get(variable_id).map_or("", String::as_str)
                    }
                };
                if !seen.insert(value) {
                    matches = true;
                    break;
                }
            }
            matches
        }
        EocConditionV1::CompareStringAll(values) => {
            let mut values = values.iter().map(|value| match value {
                EocStringValueV1::Literal(value) => value.as_str(),
                EocStringValueV1::ActorVariable(variable_id) => {
                    variables.get(variable_id).map_or("", String::as_str)
                }
            });
            let first = values.next().ok_or(SimError::InvalidItem)?;
            values.all(|value| value == first)
        }
        EocConditionV1::Not(condition) => {
            !evaluate_condition(condition, effects, variables, operations)?
        }
        EocConditionV1::And(conditions) => {
            let mut matches = true;
            for condition in conditions {
                if !evaluate_condition(condition, effects, variables, operations)? {
                    matches = false;
                    break;
                }
            }
            matches
        }
        EocConditionV1::Or(conditions) => {
            let mut matches = false;
            for condition in conditions {
                if evaluate_condition(condition, effects, variables, operations)? {
                    matches = true;
                    break;
                }
            }
            matches
        }
    })
}

fn execute_effects(
    catalog: &BTreeMap<String, EocDefinitionV1>,
    effects: &[EocEffectV1],
    execution: &mut EocExecution,
    depth: usize,
) -> Result<(), SimError> {
    if depth >= cdda_protocol::MAX_EOC_TREE_DEPTH {
        return Err(SimError::InvalidItem);
    }
    for effect in effects {
        execution.operations = execution.operations.saturating_add(1);
        if execution.operations > MAX_EOC_OPERATIONS_PER_COMMAND {
            return Err(SimError::InvalidItem);
        }
        match effect {
            EocEffectV1::Message { text } => execution.messages.push(text.clone()),
            EocEffectV1::AddEffect {
                effect_id,
                body_part_id,
                duration_turns,
                permanent,
                intensity,
            } => add_effect(
                execution,
                effect_id,
                body_part_id.clone(),
                *duration_turns,
                *permanent,
                *intensity,
            )?,
            EocEffectV1::RemoveEffects {
                effect_ids,
                body_part_id,
            } => execution.effects.retain(|effect| {
                !effect_ids.iter().any(|id| id == &effect.effect_id)
                    || body_part_id
                        .as_ref()
                        .is_some_and(|part| effect.body_part_id.as_ref() != Some(part))
            }),
            EocEffectV1::SetActorVariable {
                variable_id,
                possible_values,
            } => {
                if !execution.variables.contains_key(variable_id)
                    && execution.variables.len() >= MAX_EOC_ACTOR_VARIABLES
                {
                    return Err(SimError::InvalidItem);
                }
                let index = usize::try_from(execution.rng.next_u32())
                    .map_err(|_| SimError::NumericOverflow)?
                    % possible_values.len();
                execution
                    .variables
                    .insert(variable_id.clone(), possible_values[index].clone());
            }
            EocEffectV1::RemoveActorVariable { variable_id } => {
                execution.variables.remove(variable_id);
            }
            EocEffectV1::RunEocs { eoc_ids, delay } => {
                if let Some(delay) = delay {
                    for eoc_id in eoc_ids {
                        schedule_eoc(execution, eoc_id, *delay)?;
                    }
                } else {
                    for eoc_id in eoc_ids {
                        execute_eoc(catalog, eoc_id, execution, depth + 1)?;
                    }
                }
            }
            EocEffectV1::Conditional {
                condition,
                then_effects,
                else_effects,
            } => {
                let selected = if evaluate_condition(
                    condition,
                    &execution.effects,
                    &execution.variables,
                    &mut execution.operations,
                )? {
                    then_effects
                } else {
                    else_effects
                };
                execute_effects(catalog, selected, execution, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn schedule_eoc(
    execution: &mut EocExecution,
    eoc_id: &str,
    delay: EocDelayV1,
) -> Result<(), SimError> {
    if execution.scheduled_eocs.len() >= MAX_ACTOR_SCHEDULED_EOCS {
        return Err(SimError::InvalidItem);
    }
    let delay_turns = inclusive_rng_u64(
        &mut execution.rng,
        u64::from(delay.minimum_turns),
        u64::from(delay.maximum_turns),
    );
    let delay_ticks = delay_turns
        .checked_mul(SimTick::HZ)
        .ok_or(SimError::NumericOverflow)?;
    execution.next_schedule_sequence = execution
        .next_schedule_sequence
        .checked_add(1)
        .ok_or(SimError::NumericOverflow)?;
    execution.scheduled_eocs.push(ScheduledEocV1 {
        sequence: execution.next_schedule_sequence,
        due_tick: SimTick(
            execution
                .tick
                .0
                .checked_add(delay_ticks)
                .ok_or(SimError::NumericOverflow)?,
        ),
        eoc_id: eoc_id.to_owned(),
    });
    Ok(())
}

fn add_effect(
    execution: &mut EocExecution,
    effect_id: &str,
    body_part_id: Option<String>,
    duration_turns: u32,
    permanent: bool,
    intensity: u32,
) -> Result<(), SimError> {
    let duration_ticks = u64::from(duration_turns)
        .checked_mul(SimTick::HZ)
        .ok_or(SimError::NumericOverflow)?;
    if let Some(existing) = execution
        .effects
        .iter_mut()
        .find(|effect| effect.effect_id == effect_id && effect.body_part_id == body_part_id)
    {
        existing.intensity = intensity;
        existing.expires_at_tick = if permanent || existing.expires_at_tick == SimTick(u64::MAX) {
            SimTick(u64::MAX)
        } else {
            SimTick(
                existing
                    .expires_at_tick
                    .0
                    .saturating_add(duration_ticks)
                    .min(u64::MAX - 1),
            )
        };
    } else {
        execution.effects.push(ActorEffectSnapshotV1 {
            effect_id: effect_id.to_owned(),
            body_part_id,
            intensity,
            expires_at_tick: if permanent {
                SimTick(u64::MAX)
            } else {
                SimTick(
                    execution
                        .tick
                        .0
                        .checked_add(duration_ticks)
                        .ok_or(SimError::NumericOverflow)?,
                )
            },
        });
    }
    Ok(())
}

pub(super) fn eoc_body_parts_are_valid(
    definition: &EocDefinitionV1,
    anatomy: &cdda_protocol::AnatomyDefinitionV1,
) -> bool {
    let valid_part = |part: &Option<String>| {
        part.as_ref().is_none_or(|part| {
            anatomy
                .parts
                .iter()
                .any(|prototype| prototype.body_part_id == *part)
        })
    };
    definition
        .condition
        .as_ref()
        .is_none_or(|condition| condition_body_parts_are_valid(condition, &valid_part))
        && definition
            .deactivate_condition
            .as_ref()
            .is_none_or(|condition| condition_body_parts_are_valid(condition, &valid_part))
        && effects_body_parts_are_valid(&definition.effects, &valid_part)
        && effects_body_parts_are_valid(&definition.false_effects, &valid_part)
}

pub(super) fn actor_recurring_eoc_state_is_valid(
    catalog: &BTreeMap<String, EocDefinitionV1>,
    scheduled: &[ScheduledEocV1],
    inactive: &[String],
) -> bool {
    inactive.iter().all(|eoc_id| {
        catalog.get(eoc_id).is_some_and(|definition| {
            definition.recurrence.is_some() && definition.deactivate_condition.is_some()
        })
    }) && catalog
        .values()
        .filter(|definition| definition.recurrence.is_some())
        .all(|definition| {
            scheduled
                .iter()
                .any(|entry| entry.eoc_id == definition.eoc_id)
                || inactive.binary_search(&definition.eoc_id).is_ok()
        })
}

fn condition_body_parts_are_valid(
    condition: &EocConditionV1,
    valid_part: &impl Fn(&Option<String>) -> bool,
) -> bool {
    match condition {
        EocConditionV1::Constant(_)
        | EocConditionV1::CompareString(_)
        | EocConditionV1::CompareStringAll(_) => true,
        EocConditionV1::HasEffect { body_part_id, .. } => valid_part(body_part_id),
        EocConditionV1::Not(condition) => condition_body_parts_are_valid(condition, valid_part),
        EocConditionV1::And(conditions) | EocConditionV1::Or(conditions) => conditions
            .iter()
            .all(|condition| condition_body_parts_are_valid(condition, valid_part)),
    }
}

fn effects_body_parts_are_valid(
    effects: &[EocEffectV1],
    valid_part: &impl Fn(&Option<String>) -> bool,
) -> bool {
    effects.iter().all(|effect| match effect {
        EocEffectV1::AddEffect { body_part_id, .. }
        | EocEffectV1::RemoveEffects { body_part_id, .. } => valid_part(body_part_id),
        EocEffectV1::Conditional {
            condition,
            then_effects,
            else_effects,
        } => {
            condition_body_parts_are_valid(condition, valid_part)
                && effects_body_parts_are_valid(then_effects, valid_part)
                && effects_body_parts_are_valid(else_effects, valid_part)
        }
        EocEffectV1::Message { .. }
        | EocEffectV1::SetActorVariable { .. }
        | EocEffectV1::RemoveActorVariable { .. }
        | EocEffectV1::RunEocs { .. } => true,
    })
}
