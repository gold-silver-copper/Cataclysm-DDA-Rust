use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const MAX_EOC_DEFINITIONS: usize = 65_536;
pub const MAX_EOC_ITEM_USE_TYPES: usize = 65_536;
pub const MAX_EOC_EFFECTS: usize = 1_024;
pub const MAX_EOC_REFERENCES: usize = 256;
pub const MAX_EOC_TREE_DEPTH: usize = 64;
pub const MAX_EOC_TREE_NODES: usize = 8_192;
pub const MAX_EOC_MESSAGE_BYTES: usize = 16 * 1_024;
pub const MAX_EOC_ACTOR_VARIABLES: usize = 1_024;
pub const MAX_EOC_VARIABLE_VALUE_BYTES: usize = 16 * 1_024;
pub const MAX_ACTOR_SCHEDULED_EOCS: usize = 4_096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EocDelayV1 {
    pub minimum_turns: u32,
    pub maximum_turns: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScheduledEocV1 {
    pub sequence: u64,
    pub due_tick: crate::SimTick,
    pub eoc_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EocStringValueV1 {
    Literal(String),
    ActorVariable(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EocConditionV1 {
    Constant(bool),
    HasEffect {
        effect_id: String,
        body_part_id: Option<String>,
    },
    /// CDDA `compare_string`: true when any two resolved values match.
    CompareString(Vec<EocStringValueV1>),
    /// CDDA `compare_string_match_all`: true when every value matches.
    CompareStringAll(Vec<EocStringValueV1>),
    Not(Box<Self>),
    And(Vec<Self>),
    Or(Vec<Self>),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EocEffectV1 {
    Message {
        text: String,
    },
    AddEffect {
        effect_id: String,
        body_part_id: Option<String>,
        duration_turns: u32,
        permanent: bool,
        intensity: u32,
    },
    RemoveEffects {
        effect_ids: Vec<String>,
        body_part_id: Option<String>,
    },
    SetActorVariable {
        variable_id: String,
        /// Source-ordered weighted choices. Duplicate values retain weight.
        possible_values: Vec<String>,
    },
    RemoveActorVariable {
        variable_id: String,
    },
    RunEocs {
        eoc_ids: Vec<String>,
        delay: Option<EocDelayV1>,
    },
    Conditional {
        condition: EocConditionV1,
        then_effects: Vec<Self>,
        else_effects: Vec<Self>,
    },
}

impl EocEffectV1 {
    fn collect_references<'a>(&'a self, target: &mut Vec<&'a str>) {
        match self {
            Self::RunEocs { eoc_ids, .. } => target.extend(eoc_ids.iter().map(String::as_str)),
            Self::Conditional {
                then_effects,
                else_effects,
                ..
            } => {
                for effect in then_effects.iter().chain(else_effects) {
                    effect.collect_references(target);
                }
            }
            Self::Message { .. }
            | Self::AddEffect { .. }
            | Self::RemoveEffects { .. }
            | Self::SetActorVariable { .. }
            | Self::RemoveActorVariable { .. } => {}
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EocDefinitionV1 {
    pub eoc_id: String,
    pub condition: Option<EocConditionV1>,
    pub effects: Vec<EocEffectV1>,
    pub false_effects: Vec<EocEffectV1>,
    pub recurrence: Option<EocDelayV1>,
    pub deactivate_condition: Option<EocConditionV1>,
}

impl EocDefinitionV1 {
    #[must_use]
    pub fn referenced_eocs(&self) -> Vec<&str> {
        let mut referenced = Vec::new();
        for effect in self.effects.iter().chain(&self.false_effects) {
            effect.collect_references(&mut referenced);
        }
        referenced
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EocItemUseTypeV1 {
    pub item_type_id: String,
    pub eoc_ids: Vec<String>,
    pub consume: bool,
    pub need_worn: bool,
    pub need_wielding: bool,
}

#[must_use]
pub fn eoc_catalog_is_valid(
    definitions: &[EocDefinitionV1],
    item_use_types: &[EocItemUseTypeV1],
) -> bool {
    if definitions.len() > MAX_EOC_DEFINITIONS
        || item_use_types.len() > MAX_EOC_ITEM_USE_TYPES
        || definitions
            .iter()
            .filter(|definition| definition.recurrence.is_some())
            .count()
            > MAX_ACTOR_SCHEDULED_EOCS
        || !definitions
            .windows(2)
            .all(|pair| pair[0].eoc_id < pair[1].eoc_id)
        || !item_use_types
            .windows(2)
            .all(|pair| pair[0].item_type_id < pair[1].item_type_id)
    {
        return false;
    }
    let ids = definitions
        .iter()
        .map(|definition| definition.eoc_id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != definitions.len()
        || definitions.iter().any(|definition| {
            !valid_id(&definition.eoc_id)
                || definition.effects.len() > MAX_EOC_EFFECTS
                || definition.false_effects.len() > MAX_EOC_EFFECTS
                || definition
                    .referenced_eocs()
                    .iter()
                    .any(|reference| !ids.contains(reference))
                || !valid_eoc_tree(definition)
        })
    {
        return false;
    }
    let activation_ids = definitions
        .iter()
        .filter(|definition| definition.recurrence.is_none())
        .map(|definition| definition.eoc_id.as_str())
        .collect::<BTreeSet<_>>();
    item_use_types.iter().all(|item| {
        valid_id(&item.item_type_id)
            && !item.eoc_ids.is_empty()
            && item.eoc_ids.len() <= MAX_EOC_REFERENCES
            && item
                .eoc_ids
                .iter()
                .all(|id| activation_ids.contains(id.as_str()))
    })
}

fn valid_eoc_tree(definition: &EocDefinitionV1) -> bool {
    let mut nodes = 0;
    definition
        .condition
        .as_ref()
        .is_none_or(|condition| valid_condition(condition, 0, &mut nodes))
        && definition.recurrence.as_ref().is_none_or(|recurrence| {
            recurrence.minimum_turns > 0 && recurrence.maximum_turns >= recurrence.minimum_turns
        })
        && definition
            .deactivate_condition
            .as_ref()
            .is_none_or(|condition| {
                definition.recurrence.is_some() && valid_condition(condition, 0, &mut nodes)
            })
        && valid_effects(&definition.effects, 0, &mut nodes)
        && valid_effects(&definition.false_effects, 0, &mut nodes)
        && nodes <= MAX_EOC_TREE_NODES
}

fn valid_condition(condition: &EocConditionV1, depth: usize, nodes: &mut usize) -> bool {
    if depth >= MAX_EOC_TREE_DEPTH {
        return false;
    }
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_EOC_TREE_NODES {
        return false;
    }
    match condition {
        EocConditionV1::Constant(_) => true,
        EocConditionV1::HasEffect {
            effect_id,
            body_part_id,
        } => valid_id(effect_id) && body_part_id.as_deref().is_none_or(valid_id),
        EocConditionV1::CompareString(values) | EocConditionV1::CompareStringAll(values) => {
            (2..=MAX_EOC_REFERENCES).contains(&values.len())
                && values.iter().all(valid_string_value)
        }
        EocConditionV1::Not(condition) => valid_condition(condition, depth + 1, nodes),
        EocConditionV1::And(conditions) | EocConditionV1::Or(conditions) => {
            !conditions.is_empty()
                && conditions.len() <= MAX_EOC_EFFECTS
                && conditions
                    .iter()
                    .all(|condition| valid_condition(condition, depth + 1, nodes))
        }
    }
}

fn valid_effects(effects: &[EocEffectV1], depth: usize, nodes: &mut usize) -> bool {
    if depth >= MAX_EOC_TREE_DEPTH || effects.len() > MAX_EOC_EFFECTS {
        return false;
    }
    effects.iter().all(|effect| {
        *nodes = nodes.saturating_add(1);
        if *nodes > MAX_EOC_TREE_NODES {
            return false;
        }
        match effect {
            EocEffectV1::Message { text } => {
                !text.is_empty() && text.len() <= MAX_EOC_MESSAGE_BYTES
            }
            EocEffectV1::AddEffect {
                effect_id,
                body_part_id,
                duration_turns,
                permanent,
                intensity,
            } => {
                valid_id(effect_id)
                    && body_part_id.as_deref().is_none_or(valid_id)
                    && (*permanent || *duration_turns > 0)
                    && *intensity > 0
                    && *intensity <= 1_000_000
            }
            EocEffectV1::RemoveEffects {
                effect_ids,
                body_part_id,
            } => {
                !effect_ids.is_empty()
                    && effect_ids.len() <= MAX_EOC_REFERENCES
                    && effect_ids.iter().all(|id| valid_id(id))
                    && body_part_id.as_deref().is_none_or(valid_id)
            }
            EocEffectV1::SetActorVariable {
                variable_id,
                possible_values,
            } => {
                valid_id(variable_id)
                    && (1..=MAX_EOC_REFERENCES).contains(&possible_values.len())
                    && possible_values
                        .iter()
                        .all(|value| valid_variable_value(value))
            }
            EocEffectV1::RemoveActorVariable { variable_id } => valid_id(variable_id),
            EocEffectV1::RunEocs { eoc_ids, delay } => {
                !eoc_ids.is_empty()
                    && eoc_ids.len() <= MAX_EOC_REFERENCES
                    && eoc_ids.iter().all(|id| valid_id(id))
                    && delay.as_ref().is_none_or(|delay| {
                        delay.minimum_turns > 0 && delay.maximum_turns >= delay.minimum_turns
                    })
            }
            EocEffectV1::Conditional {
                condition,
                then_effects,
                else_effects,
            } => {
                valid_condition(condition, depth + 1, nodes)
                    && valid_effects(then_effects, depth + 1, nodes)
                    && valid_effects(else_effects, depth + 1, nodes)
            }
        }
    })
}

fn valid_string_value(value: &EocStringValueV1) -> bool {
    match value {
        EocStringValueV1::Literal(value) => valid_variable_value(value),
        EocStringValueV1::ActorVariable(variable_id) => valid_id(variable_id),
    }
}

#[must_use]
pub fn actor_eoc_variables_are_valid(
    variables: &std::collections::BTreeMap<String, String>,
) -> bool {
    variables.len() <= MAX_EOC_ACTOR_VARIABLES
        && variables
            .iter()
            .all(|(variable_id, value)| valid_id(variable_id) && valid_variable_value(value))
}

#[must_use]
pub fn actor_eoc_schedule_is_valid(schedule: &[ScheduledEocV1], next_sequence: u64) -> bool {
    schedule.len() <= MAX_ACTOR_SCHEDULED_EOCS
        && schedule
            .windows(2)
            .all(|pair| (pair[0].due_tick, pair[0].sequence) < (pair[1].due_tick, pair[1].sequence))
        && schedule
            .iter()
            .map(|entry| entry.sequence)
            .collect::<BTreeSet<_>>()
            .len()
            == schedule.len()
        && schedule.iter().all(|entry| {
            entry.sequence > 0
                && entry.sequence <= next_sequence
                && entry.due_tick > crate::SimTick(0)
                && valid_id(&entry.eoc_id)
        })
}

#[must_use]
pub fn actor_inactive_recurring_eocs_are_valid(inactive: &[String]) -> bool {
    inactive.len() <= MAX_ACTOR_SCHEDULED_EOCS
        && inactive.windows(2).all(|pair| pair[0] < pair[1])
        && inactive.iter().all(|id| valid_id(id))
}

fn valid_variable_value(value: &str) -> bool {
    value.len() <= MAX_EOC_VARIABLE_VALUE_BYTES && !value.chars().any(char::is_control)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}
