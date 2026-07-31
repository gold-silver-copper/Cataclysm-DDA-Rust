use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    EffectOnConditionDefinition, EffectOnConditionRegistry, EocConditionDefinition,
    EocDelayDefinition, EocEffectDefinition, EocStringValueDefinition, ItemRegistry,
};
use cdda_protocol::{
    AnatomyDefinitionV1, EocConditionV1, EocDefinitionV1, EocDelayV1, EocEffectV1,
    EocItemUseTypeV1, EocStringValueV1, eoc_catalog_is_valid,
};

pub(super) fn runtime_eoc_catalog(
    registry: &EffectOnConditionRegistry,
    items: &ItemRegistry,
    anatomy: &AnatomyDefinitionV1,
) -> Result<(Vec<EocDefinitionV1>, Vec<EocItemUseTypeV1>), Box<dyn std::error::Error>> {
    let mut definitions = registry
        .iter()
        .filter(|(_id, definition)| definition.is_fully_supported())
        .map(|(id, definition)| (id.to_owned(), definition.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut conflicts = BTreeSet::new();
    for (_item_id, item) in items.iter() {
        for inline in item
            .eoc_actions
            .iter()
            .flat_map(|action| &action.inline_eocs)
            .filter(|definition| definition.is_fully_supported())
        {
            match definitions.get(&inline.id) {
                Some(existing) if existing != inline => {
                    conflicts.insert(inline.id.clone());
                }
                Some(_) => {}
                None => {
                    definitions.insert(inline.id.clone(), inline.clone());
                }
            }
        }
    }
    for conflict in conflicts {
        definitions.remove(&conflict);
    }
    definitions.retain(|_id, definition| {
        runtime_eoc_body_parts_are_supported(&runtime_eoc_definition(definition), anatomy)
    });

    loop {
        let unavailable = definitions
            .iter()
            .filter(|(_id, definition)| {
                definition
                    .referenced_eocs()
                    .iter()
                    .any(|reference| !definitions.contains_key(*reference))
            })
            .map(|(id, _definition)| id.clone())
            .collect::<Vec<_>>();
        if unavailable.is_empty() {
            break;
        }
        for id in unavailable {
            definitions.remove(&id);
        }
    }

    let runtime_definitions = definitions
        .values()
        .map(runtime_eoc_definition)
        .collect::<Vec<_>>();
    let item_use_types = items
        .iter()
        .filter_map(|(_id, item)| {
            let [action] = item.eoc_actions.as_slice() else {
                return None;
            };
            if item.has_unsupported_use_actions
                || !item.transform_actions.is_empty()
                || !item.healing_actions.is_empty()
                || !item.comestible_type.is_empty()
                || !action.deferred_fields.is_empty()
                || action.eoc_ids.is_empty()
                || action
                    .eoc_ids
                    .iter()
                    .any(|id| !definitions.contains_key(id))
            {
                return None;
            }
            Some(EocItemUseTypeV1 {
                item_type_id: item.id.clone(),
                eoc_ids: action.eoc_ids.clone(),
                consume: action.consume,
                need_worn: action.need_worn,
                need_wielding: action.need_wielding,
            })
        })
        .collect::<Vec<_>>();
    if !eoc_catalog_is_valid(&runtime_definitions, &item_use_types) {
        return Err("runtime EOC catalog is invalid".into());
    }
    Ok((runtime_definitions, item_use_types))
}

fn runtime_eoc_body_parts_are_supported(
    definition: &EocDefinitionV1,
    anatomy: &AnatomyDefinitionV1,
) -> bool {
    let valid_part = |body_part_id: &Option<String>| {
        body_part_id.as_ref().is_none_or(|body_part_id| {
            anatomy
                .parts
                .iter()
                .any(|part| part.body_part_id == *body_part_id)
        })
    };
    definition
        .condition
        .as_ref()
        .is_none_or(|condition| runtime_condition_body_parts_are_supported(condition, &valid_part))
        && runtime_effect_body_parts_are_supported(&definition.effects, &valid_part)
        && runtime_effect_body_parts_are_supported(&definition.false_effects, &valid_part)
}

fn runtime_condition_body_parts_are_supported(
    condition: &EocConditionV1,
    valid_part: &impl Fn(&Option<String>) -> bool,
) -> bool {
    match condition {
        EocConditionV1::Constant(_)
        | EocConditionV1::CompareString(_)
        | EocConditionV1::CompareStringAll(_) => true,
        EocConditionV1::HasEffect { body_part_id, .. } => valid_part(body_part_id),
        EocConditionV1::Not(condition) => {
            runtime_condition_body_parts_are_supported(condition, valid_part)
        }
        EocConditionV1::And(conditions) | EocConditionV1::Or(conditions) => conditions
            .iter()
            .all(|condition| runtime_condition_body_parts_are_supported(condition, valid_part)),
    }
}

fn runtime_effect_body_parts_are_supported(
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
            runtime_condition_body_parts_are_supported(condition, valid_part)
                && runtime_effect_body_parts_are_supported(then_effects, valid_part)
                && runtime_effect_body_parts_are_supported(else_effects, valid_part)
        }
        EocEffectV1::Message { .. }
        | EocEffectV1::SetActorVariable { .. }
        | EocEffectV1::RemoveActorVariable { .. }
        | EocEffectV1::RunEocs { .. } => true,
    })
}

fn runtime_eoc_definition(definition: &EffectOnConditionDefinition) -> EocDefinitionV1 {
    EocDefinitionV1 {
        eoc_id: definition.id.clone(),
        condition: definition.condition.as_ref().map(runtime_condition),
        effects: definition.effects.iter().map(runtime_effect).collect(),
        false_effects: definition
            .false_effects
            .iter()
            .map(runtime_effect)
            .collect(),
    }
}

fn runtime_condition(condition: &EocConditionDefinition) -> EocConditionV1 {
    match condition {
        EocConditionDefinition::Constant(value) => EocConditionV1::Constant(*value),
        EocConditionDefinition::HasEffect {
            effect_id,
            body_part_id,
        } => EocConditionV1::HasEffect {
            effect_id: effect_id.clone(),
            body_part_id: body_part_id.clone(),
        },
        EocConditionDefinition::CompareString(values) => EocConditionV1::CompareString(
            values
                .iter()
                .map(|value| match value {
                    EocStringValueDefinition::Literal(value) => {
                        EocStringValueV1::Literal(value.clone())
                    }
                    EocStringValueDefinition::ActorVariable(variable_id) => {
                        EocStringValueV1::ActorVariable(variable_id.clone())
                    }
                })
                .collect(),
        ),
        EocConditionDefinition::CompareStringAll(values) => EocConditionV1::CompareStringAll(
            values
                .iter()
                .map(|value| match value {
                    EocStringValueDefinition::Literal(value) => {
                        EocStringValueV1::Literal(value.clone())
                    }
                    EocStringValueDefinition::ActorVariable(variable_id) => {
                        EocStringValueV1::ActorVariable(variable_id.clone())
                    }
                })
                .collect(),
        ),
        EocConditionDefinition::Not(condition) => {
            EocConditionV1::Not(Box::new(runtime_condition(condition)))
        }
        EocConditionDefinition::And(conditions) => {
            EocConditionV1::And(conditions.iter().map(runtime_condition).collect())
        }
        EocConditionDefinition::Or(conditions) => {
            EocConditionV1::Or(conditions.iter().map(runtime_condition).collect())
        }
    }
}

fn runtime_effect(effect: &EocEffectDefinition) -> EocEffectV1 {
    match effect {
        EocEffectDefinition::Message { text } => EocEffectV1::Message { text: text.clone() },
        EocEffectDefinition::AddEffect {
            effect_id,
            body_part_id,
            duration_turns,
            permanent,
            intensity,
        } => EocEffectV1::AddEffect {
            effect_id: effect_id.clone(),
            body_part_id: body_part_id.clone(),
            duration_turns: *duration_turns,
            permanent: *permanent,
            intensity: *intensity,
        },
        EocEffectDefinition::RemoveEffects {
            effect_ids,
            body_part_id,
        } => EocEffectV1::RemoveEffects {
            effect_ids: effect_ids.clone(),
            body_part_id: body_part_id.clone(),
        },
        EocEffectDefinition::SetActorVariable {
            variable_id,
            possible_values,
        } => EocEffectV1::SetActorVariable {
            variable_id: variable_id.clone(),
            possible_values: possible_values.clone(),
        },
        EocEffectDefinition::RemoveActorVariable { variable_id } => {
            EocEffectV1::RemoveActorVariable {
                variable_id: variable_id.clone(),
            }
        }
        EocEffectDefinition::RunEocs { eoc_ids, delay } => EocEffectV1::RunEocs {
            eoc_ids: eoc_ids.clone(),
            delay: delay.map(
                |EocDelayDefinition {
                     minimum_turns,
                     maximum_turns,
                 }| EocDelayV1 {
                    minimum_turns,
                    maximum_turns,
                },
            ),
        },
        EocEffectDefinition::Conditional {
            condition,
            then_effects,
            else_effects,
        } => EocEffectV1::Conditional {
            condition: runtime_condition(condition),
            then_effects: then_effects.iter().map(runtime_effect).collect(),
            else_effects: else_effects.iter().map(runtime_effect).collect(),
        },
    }
}
