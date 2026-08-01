use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{
    EffectOnConditionDefinition, EffectOnConditionRegistry, EocActorStatDefinition,
    EocActorValueDefinition, EocConditionDefinition, EocDelayDefinition, EocEffectDefinition,
    EocEventTriggerDefinition, EocMathAssignmentOperationDefinition,
    EocMathAssignmentTargetDefinition, EocMathExpressionDefinition, EocStringValueDefinition,
    ItemRegistry, ProficiencyRegistry, RecipeRegistry,
};
use cdda_protocol::{
    AnatomyDefinitionV1, EocActorStatV1, EocActorValueV1, EocConditionV1, EocDefinitionV1,
    EocDelayV1, EocEffectV1, EocEventTriggerV1, EocItemUseTypeV1, EocMathAssignmentOperationV1,
    EocMathAssignmentTargetV1, EocMathExpressionV1, EocStringValueV1, eoc_catalog_is_valid,
};

pub(super) fn runtime_eoc_catalog(
    registry: &EffectOnConditionRegistry,
    items: &ItemRegistry,
    anatomy: &AnatomyDefinitionV1,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    mission_ids: &BTreeSet<String>,
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
            && eoc_references_are_supported(definition, items, proficiencies, recipes, mission_ids)
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

    let mut target_context_ids = definitions
        .iter()
        .filter(|(_id, definition)| {
            cdda_protocol::eoc_definition_requires_target_context(&runtime_eoc_definition(
                definition,
            ))
        })
        .map(|(id, _definition)| id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        let inherited = definitions
            .iter()
            .filter(|(id, definition)| {
                !target_context_ids.contains(*id)
                    && definition
                        .referenced_eocs()
                        .iter()
                        .any(|reference| target_context_ids.contains(*reference))
            })
            .map(|(id, _definition)| id.clone())
            .collect::<Vec<_>>();
        if inherited.is_empty() {
            break;
        }
        target_context_ids.extend(inherited);
    }
    definitions.retain(|id, definition| {
        !target_context_ids.contains(id)
            || (definition.recurrence.is_none() && definition.event_trigger.is_none())
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
            target_context_ids.remove(&id);
        }
    }
    target_context_ids.retain(|id| definitions.contains_key(id));

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
                    .any(|id| target_context_ids.contains(id))
                || action.eoc_ids.iter().any(|id| {
                    definitions.get(id).is_none_or(|definition| {
                        definition.recurrence.is_some() || definition.event_trigger.is_some()
                    })
                })
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

fn eoc_references_are_supported(
    definition: &EffectOnConditionDefinition,
    items: &ItemRegistry,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    mission_ids: &BTreeSet<String>,
) -> bool {
    definition.condition.as_ref().is_none_or(|condition| {
        condition_references_are_supported(condition, items, proficiencies, recipes, mission_ids)
    }) && definition
        .deactivate_condition
        .as_ref()
        .is_none_or(|condition| {
            condition_references_are_supported(
                condition,
                items,
                proficiencies,
                recipes,
                mission_ids,
            )
        })
        && effects_references_are_supported(
            &definition.effects,
            items,
            proficiencies,
            recipes,
            mission_ids,
        )
        && effects_references_are_supported(
            &definition.false_effects,
            items,
            proficiencies,
            recipes,
            mission_ids,
        )
}

fn condition_references_are_supported(
    condition: &EocConditionDefinition,
    items: &ItemRegistry,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    mission_ids: &BTreeSet<String>,
) -> bool {
    match condition {
        EocConditionDefinition::HasItem { item_type_id, .. }
        | EocConditionDefinition::IsWearing { item_type_id } => items.get(item_type_id).is_some(),
        EocConditionDefinition::HasProficiency { proficiency_id } => {
            proficiencies.get(proficiency_id).is_some()
        }
        EocConditionDefinition::KnowsRecipe { recipe_id } => recipes.get(recipe_id).is_some(),
        EocConditionDefinition::HasMission { mission_type_id } => {
            mission_ids.contains(mission_type_id)
        }
        EocConditionDefinition::Not(condition) => condition_references_are_supported(
            condition,
            items,
            proficiencies,
            recipes,
            mission_ids,
        ),
        EocConditionDefinition::And(conditions) | EocConditionDefinition::Or(conditions) => {
            conditions.iter().all(|condition| {
                condition_references_are_supported(
                    condition,
                    items,
                    proficiencies,
                    recipes,
                    mission_ids,
                )
            })
        }
        EocConditionDefinition::Constant(_)
        | EocConditionDefinition::HasEffect { .. }
        | EocConditionDefinition::HasAnyEffect { .. }
        | EocConditionDefinition::TargetHasEffect { .. }
        | EocConditionDefinition::TargetHasAnyEffect { .. }
        | EocConditionDefinition::CompareString(_)
        | EocConditionDefinition::CompareStringAll(_)
        | EocConditionDefinition::HasWeapon
        | EocConditionDefinition::StatAtLeast { .. }
        | EocConditionDefinition::Math(_) => true,
    }
}

pub(super) fn effects_references_are_supported(
    effects: &[EocEffectDefinition],
    items: &ItemRegistry,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    mission_ids: &BTreeSet<String>,
) -> bool {
    effects.iter().all(|effect| match effect {
        EocEffectDefinition::Conditional {
            condition,
            then_effects,
            else_effects,
        } => {
            condition_references_are_supported(
                condition,
                items,
                proficiencies,
                recipes,
                mission_ids,
            ) && effects_references_are_supported(
                then_effects,
                items,
                proficiencies,
                recipes,
                mission_ids,
            ) && effects_references_are_supported(
                else_effects,
                items,
                proficiencies,
                recipes,
                mission_ids,
            )
        }
        EocEffectDefinition::Confirmation {
            accept_effects,
            decline_effects,
            ..
        } => {
            effects_references_are_supported(
                accept_effects,
                items,
                proficiencies,
                recipes,
                mission_ids,
            ) && effects_references_are_supported(
                decline_effects,
                items,
                proficiencies,
                recipes,
                mission_ids,
            )
        }
        EocEffectDefinition::AssignMission { mission_type_id }
        | EocEffectDefinition::FinishMission {
            mission_type_id, ..
        } => mission_ids.contains(mission_type_id),
        // The current EOC wire form does not carry resolved effect-type
        // duration caps, blockers, intensity mapping, or stat modifiers.
        // Admitting either raw application would fabricate semantics.
        EocEffectDefinition::AddEffect { .. } | EocEffectDefinition::AddTargetEffect { .. } => {
            false
        }
        EocEffectDefinition::Message { .. }
        | EocEffectDefinition::RemoveEffects { .. }
        | EocEffectDefinition::SetActorVariable { .. }
        | EocEffectDefinition::RemoveActorVariable { .. }
        | EocEffectDefinition::RemoveTargetEffects { .. }
        | EocEffectDefinition::SetTargetVariable { .. }
        | EocEffectDefinition::RemoveTargetVariable { .. }
        | EocEffectDefinition::ChangeTargetFaction { .. }
        | EocEffectDefinition::AddTargetFactionReputation { .. }
        | EocEffectDefinition::AddTargetFactionTrust { .. }
        | EocEffectDefinition::MathAssignment(_)
        | EocEffectDefinition::RunEocs { .. } => true,
    })
}

pub(super) fn runtime_actor_only_eoc_ids(definitions: &[EocDefinitionV1]) -> BTreeSet<String> {
    let mut available = definitions
        .iter()
        .filter(|definition| {
            !cdda_protocol::eoc_definition_requires_target_context(definition)
                && !cdda_protocol::eoc_effects_contain_confirmation(&definition.effects)
                && !cdda_protocol::eoc_effects_contain_confirmation(&definition.false_effects)
        })
        .map(|definition| (definition.eoc_id.as_str(), definition))
        .collect::<BTreeMap<_, _>>();
    loop {
        let unavailable = available
            .iter()
            .filter(|(_id, definition)| {
                definition
                    .referenced_eocs()
                    .iter()
                    .any(|reference| !available.contains_key(*reference))
            })
            .map(|(id, _definition)| *id)
            .collect::<Vec<_>>();
        if unavailable.is_empty() {
            break;
        }
        for id in unavailable {
            available.remove(id);
        }
    }
    available.keys().map(|id| (*id).to_owned()).collect()
}

pub(super) fn runtime_dialogue_eoc_ids(
    definitions: &[EocDefinitionV1],
    actor_only_ids: &BTreeSet<String>,
    faction_ids: &BTreeSet<&str>,
) -> BTreeSet<String> {
    let mut available = definitions
        .iter()
        .filter(|definition| {
            definition.recurrence.is_none()
                && definition.event_trigger.is_none()
                && !cdda_protocol::eoc_effects_contain_confirmation(&definition.effects)
                && !cdda_protocol::eoc_effects_contain_confirmation(&definition.false_effects)
                && dialogue_effect_applications_are_supported(&definition.effects)
                && dialogue_effect_applications_are_supported(&definition.false_effects)
                && dialogue_faction_references_are_supported(&definition.effects, faction_ids)
                && dialogue_faction_references_are_supported(&definition.false_effects, faction_ids)
        })
        .map(|definition| (definition.eoc_id.as_str(), definition))
        .collect::<BTreeMap<_, _>>();
    loop {
        let immediate_ids = available.keys().copied().collect::<BTreeSet<_>>();
        let unavailable = available
            .iter()
            .filter(|(_id, definition)| {
                !dialogue_effect_references_are_available(
                    &definition.effects,
                    &immediate_ids,
                    actor_only_ids,
                ) || !dialogue_effect_references_are_available(
                    &definition.false_effects,
                    &immediate_ids,
                    actor_only_ids,
                )
            })
            .map(|(id, _definition)| *id)
            .collect::<Vec<_>>();
        if unavailable.is_empty() {
            break;
        }
        for id in unavailable {
            available.remove(id);
        }
    }
    available.keys().map(|id| (*id).to_owned()).collect()
}

fn dialogue_effect_applications_are_supported(effects: &[EocEffectV1]) -> bool {
    effects.iter().all(|effect| match effect {
        // The current EOC wire form does not carry the resolved effect-type
        // duration caps, blockers, or stat modifiers. Admitting a raw
        // application would fabricate semantics, so keep it fail-closed until
        // the shared application profile is represented.
        EocEffectV1::AddEffect { .. } | EocEffectV1::AddTargetEffect { .. } => false,
        EocEffectV1::Conditional {
            then_effects,
            else_effects,
            ..
        }
        | EocEffectV1::Confirmation {
            accept_effects: then_effects,
            decline_effects: else_effects,
            ..
        } => {
            dialogue_effect_applications_are_supported(then_effects)
                && dialogue_effect_applications_are_supported(else_effects)
        }
        _ => true,
    })
}

pub(super) fn dialogue_faction_references_are_supported(
    effects: &[EocEffectV1],
    faction_ids: &BTreeSet<&str>,
) -> bool {
    effects.iter().all(|effect| match effect {
        EocEffectV1::ChangeTargetFaction { faction_id } => {
            faction_ids.contains(faction_id.as_str())
        }
        EocEffectV1::Conditional {
            then_effects,
            else_effects,
            ..
        }
        | EocEffectV1::Confirmation {
            accept_effects: then_effects,
            decline_effects: else_effects,
            ..
        } => {
            dialogue_faction_references_are_supported(then_effects, faction_ids)
                && dialogue_faction_references_are_supported(else_effects, faction_ids)
        }
        EocEffectV1::Message { .. }
        | EocEffectV1::AddEffect { .. }
        | EocEffectV1::RemoveEffects { .. }
        | EocEffectV1::SetActorVariable { .. }
        | EocEffectV1::RemoveActorVariable { .. }
        | EocEffectV1::AddTargetEffect { .. }
        | EocEffectV1::RemoveTargetEffects { .. }
        | EocEffectV1::SetTargetVariable { .. }
        | EocEffectV1::RemoveTargetVariable { .. }
        | EocEffectV1::AddTargetFactionReputation { .. }
        | EocEffectV1::AddTargetFactionTrust { .. }
        | EocEffectV1::MathAssignment { .. }
        | EocEffectV1::RunEocs { .. }
        | EocEffectV1::AssignMission { .. }
        | EocEffectV1::FinishMission { .. } => true,
    })
}

pub(super) fn dialogue_effect_references_are_available(
    effects: &[EocEffectV1],
    immediate_ids: &BTreeSet<impl AsRef<str>>,
    delayed_ids: &BTreeSet<String>,
) -> bool {
    effects.iter().all(|effect| match effect {
        EocEffectV1::RunEocs { eoc_ids, delay } => eoc_ids.iter().all(|id| {
            if delay.is_some() {
                delayed_ids.contains(id)
            } else {
                immediate_ids
                    .iter()
                    .any(|available| available.as_ref() == id)
            }
        }),
        EocEffectV1::Conditional {
            then_effects,
            else_effects,
            ..
        } => {
            dialogue_effect_references_are_available(then_effects, immediate_ids, delayed_ids)
                && dialogue_effect_references_are_available(
                    else_effects,
                    immediate_ids,
                    delayed_ids,
                )
        }
        EocEffectV1::Confirmation { .. } => false,
        EocEffectV1::Message { .. }
        | EocEffectV1::AddEffect { .. }
        | EocEffectV1::RemoveEffects { .. }
        | EocEffectV1::SetActorVariable { .. }
        | EocEffectV1::RemoveActorVariable { .. }
        | EocEffectV1::AddTargetEffect { .. }
        | EocEffectV1::RemoveTargetEffects { .. }
        | EocEffectV1::SetTargetVariable { .. }
        | EocEffectV1::RemoveTargetVariable { .. }
        | EocEffectV1::ChangeTargetFaction { .. }
        | EocEffectV1::AddTargetFactionReputation { .. }
        | EocEffectV1::AddTargetFactionTrust { .. }
        | EocEffectV1::MathAssignment { .. }
        | EocEffectV1::AssignMission { .. }
        | EocEffectV1::FinishMission { .. } => true,
    })
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
        && definition
            .deactivate_condition
            .as_ref()
            .is_none_or(|condition| {
                runtime_condition_body_parts_are_supported(condition, &valid_part)
            })
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
        | EocConditionV1::CompareStringAll(_)
        | EocConditionV1::HasItem { .. }
        | EocConditionV1::HasWeapon
        | EocConditionV1::IsWearing { .. }
        | EocConditionV1::HasProficiency { .. }
        | EocConditionV1::KnowsRecipe { .. }
        | EocConditionV1::HasMission { .. }
        | EocConditionV1::StatAtLeast { .. }
        | EocConditionV1::Math(_) => true,
        EocConditionV1::HasEffect { body_part_id, .. }
        | EocConditionV1::HasAnyEffect { body_part_id, .. }
        | EocConditionV1::TargetHasEffect { body_part_id, .. }
        | EocConditionV1::TargetHasAnyEffect { body_part_id, .. } => valid_part(body_part_id),
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
        | EocEffectV1::RemoveEffects { body_part_id, .. }
        | EocEffectV1::AddTargetEffect { body_part_id, .. }
        | EocEffectV1::RemoveTargetEffects { body_part_id, .. } => valid_part(body_part_id),
        EocEffectV1::Conditional {
            condition,
            then_effects,
            else_effects,
        } => {
            runtime_condition_body_parts_are_supported(condition, valid_part)
                && runtime_effect_body_parts_are_supported(then_effects, valid_part)
                && runtime_effect_body_parts_are_supported(else_effects, valid_part)
        }
        EocEffectV1::Confirmation {
            accept_effects,
            decline_effects,
            ..
        } => {
            runtime_effect_body_parts_are_supported(accept_effects, valid_part)
                && runtime_effect_body_parts_are_supported(decline_effects, valid_part)
        }
        EocEffectV1::Message { .. }
        | EocEffectV1::SetActorVariable { .. }
        | EocEffectV1::RemoveActorVariable { .. }
        | EocEffectV1::SetTargetVariable { .. }
        | EocEffectV1::RemoveTargetVariable { .. }
        | EocEffectV1::ChangeTargetFaction { .. }
        | EocEffectV1::AddTargetFactionReputation { .. }
        | EocEffectV1::AddTargetFactionTrust { .. }
        | EocEffectV1::MathAssignment { .. }
        | EocEffectV1::RunEocs { .. }
        | EocEffectV1::AssignMission { .. }
        | EocEffectV1::FinishMission { .. } => true,
    })
}

pub(super) fn runtime_dialogue_effects_are_supported(
    effects: &[EocEffectDefinition],
    anatomy: &AnatomyDefinitionV1,
    mission_ids: &BTreeSet<String>,
) -> bool {
    let runtime = effects.iter().map(runtime_effect).collect::<Vec<_>>();
    let valid_part = |body_part_id: &Option<String>| {
        body_part_id.as_ref().is_none_or(|body_part_id| {
            anatomy
                .parts
                .iter()
                .any(|part| part.body_part_id == *body_part_id)
        })
    };
    cdda_protocol::eoc_effects_are_valid(&runtime)
        && runtime_effect_body_parts_are_supported(&runtime, &valid_part)
        && dialogue_effect_applications_are_supported(&runtime)
        && dialogue_mission_references_are_supported(effects, mission_ids)
}

fn dialogue_mission_references_are_supported(
    effects: &[EocEffectDefinition],
    mission_ids: &BTreeSet<String>,
) -> bool {
    effects.iter().all(|effect| match effect {
        EocEffectDefinition::AssignMission { mission_type_id }
        | EocEffectDefinition::FinishMission {
            mission_type_id, ..
        } => mission_ids.contains(mission_type_id),
        EocEffectDefinition::Conditional {
            condition,
            then_effects,
            else_effects,
        } => {
            dialogue_condition_mission_references_are_supported(condition, mission_ids)
                && dialogue_mission_references_are_supported(then_effects, mission_ids)
                && dialogue_mission_references_are_supported(else_effects, mission_ids)
        }
        EocEffectDefinition::Confirmation {
            accept_effects: then_effects,
            decline_effects: else_effects,
            ..
        } => {
            dialogue_mission_references_are_supported(then_effects, mission_ids)
                && dialogue_mission_references_are_supported(else_effects, mission_ids)
        }
        _ => true,
    })
}

fn dialogue_condition_mission_references_are_supported(
    condition: &EocConditionDefinition,
    mission_ids: &BTreeSet<String>,
) -> bool {
    match condition {
        EocConditionDefinition::HasMission { mission_type_id } => {
            mission_ids.contains(mission_type_id)
        }
        EocConditionDefinition::Not(condition) => {
            dialogue_condition_mission_references_are_supported(condition, mission_ids)
        }
        EocConditionDefinition::And(conditions) | EocConditionDefinition::Or(conditions) => {
            conditions.iter().all(|condition| {
                dialogue_condition_mission_references_are_supported(condition, mission_ids)
            })
        }
        _ => true,
    }
}

pub(super) fn runtime_dialogue_condition_is_supported(
    condition: &EocConditionDefinition,
    items: &ItemRegistry,
    anatomy: &AnatomyDefinitionV1,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    mission_ids: &BTreeSet<String>,
) -> bool {
    let runtime = runtime_condition(condition);
    let valid_part = |body_part_id: &Option<String>| {
        body_part_id.as_ref().is_none_or(|body_part_id| {
            anatomy
                .parts
                .iter()
                .any(|part| part.body_part_id == *body_part_id)
        })
    };
    cdda_protocol::eoc_condition_is_valid(&runtime)
        && runtime_condition_body_parts_are_supported(&runtime, &valid_part)
        && condition_references_are_supported(condition, items, proficiencies, recipes, mission_ids)
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
        recurrence: definition.recurrence.map(
            |EocDelayDefinition {
                 minimum_turns,
                 maximum_turns,
             }| EocDelayV1 {
                minimum_turns,
                maximum_turns,
            },
        ),
        deactivate_condition: definition
            .deactivate_condition
            .as_ref()
            .map(runtime_condition),
        event_trigger: definition.event_trigger.map(|trigger| match trigger {
            EocEventTriggerDefinition::ActorMoved => EocEventTriggerV1::ActorMoved,
            EocEventTriggerDefinition::ActorEnteredOvermapTile => {
                EocEventTriggerV1::ActorEnteredOvermapTile
            }
            EocEventTriggerDefinition::ActorTookDamage => EocEventTriggerV1::ActorTookDamage,
            EocEventTriggerDefinition::ActorDied => EocEventTriggerV1::ActorDied,
            EocEventTriggerDefinition::ActorKilledCreature => {
                EocEventTriggerV1::ActorKilledCreature
            }
            EocEventTriggerDefinition::CreatureTookDamage => EocEventTriggerV1::CreatureTookDamage,
        }),
    }
}

pub(super) fn runtime_condition(condition: &EocConditionDefinition) -> EocConditionV1 {
    match condition {
        EocConditionDefinition::Constant(value) => EocConditionV1::Constant(*value),
        EocConditionDefinition::HasEffect {
            effect_id,
            body_part_id,
            minimum_intensity,
        } => EocConditionV1::HasEffect {
            effect_id: effect_id.clone(),
            body_part_id: body_part_id.clone(),
            minimum_intensity: *minimum_intensity,
        },
        EocConditionDefinition::HasAnyEffect {
            effect_ids,
            body_part_id,
            minimum_intensity,
        } => EocConditionV1::HasAnyEffect {
            effect_ids: effect_ids.clone(),
            body_part_id: body_part_id.clone(),
            minimum_intensity: *minimum_intensity,
        },
        EocConditionDefinition::TargetHasEffect {
            effect_id,
            body_part_id,
            minimum_intensity,
        } => EocConditionV1::TargetHasEffect {
            effect_id: effect_id.clone(),
            body_part_id: body_part_id.clone(),
            minimum_intensity: *minimum_intensity,
        },
        EocConditionDefinition::TargetHasAnyEffect {
            effect_ids,
            body_part_id,
            minimum_intensity,
        } => EocConditionV1::TargetHasAnyEffect {
            effect_ids: effect_ids.clone(),
            body_part_id: body_part_id.clone(),
            minimum_intensity: *minimum_intensity,
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
                    EocStringValueDefinition::TargetVariable(variable_id) => {
                        EocStringValueV1::TargetVariable(variable_id.clone())
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
                    EocStringValueDefinition::TargetVariable(variable_id) => {
                        EocStringValueV1::TargetVariable(variable_id.clone())
                    }
                })
                .collect(),
        ),
        EocConditionDefinition::HasItem {
            item_type_id,
            minimum_count,
            minimum_charges,
        } => EocConditionV1::HasItem {
            item_type_id: item_type_id.clone(),
            minimum_count: *minimum_count,
            minimum_charges: *minimum_charges,
        },
        EocConditionDefinition::HasWeapon => EocConditionV1::HasWeapon,
        EocConditionDefinition::IsWearing { item_type_id } => EocConditionV1::IsWearing {
            item_type_id: item_type_id.clone(),
        },
        EocConditionDefinition::HasProficiency { proficiency_id } => {
            EocConditionV1::HasProficiency {
                proficiency_id: proficiency_id.clone(),
            }
        }
        EocConditionDefinition::KnowsRecipe { recipe_id } => EocConditionV1::KnowsRecipe {
            recipe_id: recipe_id.clone(),
        },
        EocConditionDefinition::HasMission { mission_type_id } => EocConditionV1::HasMission {
            mission_type_id: mission_type_id.clone(),
        },
        EocConditionDefinition::StatAtLeast { stat, minimum } => EocConditionV1::StatAtLeast {
            stat: match stat {
                EocActorStatDefinition::Strength => EocActorStatV1::Strength,
                EocActorStatDefinition::Dexterity => EocActorStatV1::Dexterity,
                EocActorStatDefinition::Intelligence => EocActorStatV1::Intelligence,
                EocActorStatDefinition::Perception => EocActorStatV1::Perception,
            },
            minimum: *minimum,
        },
        EocConditionDefinition::Math(expression) => {
            EocConditionV1::Math(runtime_math_expression(expression))
        }
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

pub(super) fn runtime_effect(effect: &EocEffectDefinition) -> EocEffectV1 {
    match effect {
        EocEffectDefinition::Message { text } => EocEffectV1::Message { text: text.clone() },
        EocEffectDefinition::AddEffect {
            effect_id,
            body_part_id,
            duration_turns,
            permanent,
            intensity,
            intensity_is_explicit,
        } => EocEffectV1::AddEffect {
            effect_id: effect_id.clone(),
            body_part_id: body_part_id.clone(),
            duration_turns: *duration_turns,
            permanent: *permanent,
            intensity: *intensity,
            intensity_is_explicit: *intensity_is_explicit,
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
        EocEffectDefinition::AddTargetEffect {
            effect_id,
            body_part_id,
            duration_turns,
            permanent,
            intensity,
            intensity_is_explicit,
        } => EocEffectV1::AddTargetEffect {
            effect_id: effect_id.clone(),
            body_part_id: body_part_id.clone(),
            duration_turns: *duration_turns,
            permanent: *permanent,
            intensity: *intensity,
            intensity_is_explicit: *intensity_is_explicit,
        },
        EocEffectDefinition::RemoveTargetEffects {
            effect_ids,
            body_part_id,
        } => EocEffectV1::RemoveTargetEffects {
            effect_ids: effect_ids.clone(),
            body_part_id: body_part_id.clone(),
        },
        EocEffectDefinition::SetTargetVariable {
            variable_id,
            possible_values,
        } => EocEffectV1::SetTargetVariable {
            variable_id: variable_id.clone(),
            possible_values: possible_values.clone(),
        },
        EocEffectDefinition::RemoveTargetVariable { variable_id } => {
            EocEffectV1::RemoveTargetVariable {
                variable_id: variable_id.clone(),
            }
        }
        EocEffectDefinition::ChangeTargetFaction { faction_id } => {
            EocEffectV1::ChangeTargetFaction {
                faction_id: faction_id.clone(),
            }
        }
        EocEffectDefinition::AddTargetFactionReputation { delta } => {
            EocEffectV1::AddTargetFactionReputation { delta: *delta }
        }
        EocEffectDefinition::AddTargetFactionTrust { delta } => {
            EocEffectV1::AddTargetFactionTrust { delta: *delta }
        }
        EocEffectDefinition::MathAssignment(assignment) => EocEffectV1::MathAssignment {
            target: match &assignment.target {
                EocMathAssignmentTargetDefinition::ActorVariable(variable_id) => {
                    EocMathAssignmentTargetV1::ActorVariable(variable_id.clone())
                }
                EocMathAssignmentTargetDefinition::ActorStat(stat) => {
                    EocMathAssignmentTargetV1::ActorStat(runtime_actor_stat(*stat))
                }
                EocMathAssignmentTargetDefinition::ActorValue(value) => {
                    EocMathAssignmentTargetV1::ActorValue(runtime_actor_value(*value))
                }
            },
            operation: match assignment.operation {
                EocMathAssignmentOperationDefinition::Set => EocMathAssignmentOperationV1::Set,
                EocMathAssignmentOperationDefinition::Add => EocMathAssignmentOperationV1::Add,
                EocMathAssignmentOperationDefinition::Subtract => {
                    EocMathAssignmentOperationV1::Subtract
                }
                EocMathAssignmentOperationDefinition::Multiply => {
                    EocMathAssignmentOperationV1::Multiply
                }
            },
            value: runtime_math_expression(&assignment.value),
        },
        EocEffectDefinition::Confirmation {
            prompt,
            default,
            accept_effects,
            decline_effects,
        } => EocEffectV1::Confirmation {
            prompt: prompt.clone(),
            default: *default,
            accept_effects: accept_effects.iter().map(runtime_effect).collect(),
            decline_effects: decline_effects.iter().map(runtime_effect).collect(),
        },
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
        EocEffectDefinition::AssignMission { mission_type_id } => EocEffectV1::AssignMission {
            mission_type_id: mission_type_id.clone(),
        },
        EocEffectDefinition::FinishMission {
            mission_type_id,
            success,
        } => EocEffectV1::FinishMission {
            mission_type_id: mission_type_id.clone(),
            success: *success,
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

fn runtime_math_expression(expression: &EocMathExpressionDefinition) -> EocMathExpressionV1 {
    let binary = |left: &EocMathExpressionDefinition, right: &EocMathExpressionDefinition| {
        (
            Box::new(runtime_math_expression(left)),
            Box::new(runtime_math_expression(right)),
        )
    };
    match expression {
        EocMathExpressionDefinition::Constant(value) => EocMathExpressionV1::Constant(*value),
        EocMathExpressionDefinition::ActorVariable(variable_id) => {
            EocMathExpressionV1::ActorVariable(variable_id.clone())
        }
        EocMathExpressionDefinition::HasActorVariable(variable_id) => {
            EocMathExpressionV1::HasActorVariable(variable_id.clone())
        }
        EocMathExpressionDefinition::EffectIntensity(effect_id) => {
            EocMathExpressionV1::EffectIntensity(effect_id.clone())
        }
        EocMathExpressionDefinition::ActorStat(stat) => {
            EocMathExpressionV1::ActorStat(runtime_actor_stat(*stat))
        }
        EocMathExpressionDefinition::ActorValue(value) => {
            EocMathExpressionV1::ActorValue(runtime_actor_value(*value))
        }
        EocMathExpressionDefinition::Negate(value) => {
            EocMathExpressionV1::Negate(Box::new(runtime_math_expression(value)))
        }
        EocMathExpressionDefinition::Not(value) => {
            EocMathExpressionV1::Not(Box::new(runtime_math_expression(value)))
        }
        EocMathExpressionDefinition::Add(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Add(left, right)
        }
        EocMathExpressionDefinition::Subtract(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Subtract(left, right)
        }
        EocMathExpressionDefinition::Multiply(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Multiply(left, right)
        }
        EocMathExpressionDefinition::Equal(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Equal(left, right)
        }
        EocMathExpressionDefinition::NotEqual(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::NotEqual(left, right)
        }
        EocMathExpressionDefinition::Less(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Less(left, right)
        }
        EocMathExpressionDefinition::LessOrEqual(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::LessOrEqual(left, right)
        }
        EocMathExpressionDefinition::Greater(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Greater(left, right)
        }
        EocMathExpressionDefinition::GreaterOrEqual(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::GreaterOrEqual(left, right)
        }
        EocMathExpressionDefinition::And(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::And(left, right)
        }
        EocMathExpressionDefinition::Or(left, right) => {
            let (left, right) = binary(left, right);
            EocMathExpressionV1::Or(left, right)
        }
    }
}

fn runtime_actor_stat(stat: EocActorStatDefinition) -> EocActorStatV1 {
    match stat {
        EocActorStatDefinition::Strength => EocActorStatV1::Strength,
        EocActorStatDefinition::Dexterity => EocActorStatV1::Dexterity,
        EocActorStatDefinition::Intelligence => EocActorStatV1::Intelligence,
        EocActorStatDefinition::Perception => EocActorStatV1::Perception,
    }
}

fn runtime_actor_value(value: EocActorValueDefinition) -> EocActorValueV1 {
    match value {
        EocActorValueDefinition::Stamina => EocActorValueV1::Stamina,
        EocActorValueDefinition::MaximumStamina => EocActorValueV1::MaximumStamina,
        EocActorValueDefinition::Thirst => EocActorValueV1::Thirst,
        EocActorValueDefinition::Sleepiness => EocActorValueV1::Sleepiness,
    }
}
