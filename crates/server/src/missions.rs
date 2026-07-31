use std::collections::BTreeSet;

use cdda_content::{
    EocEffectDefinition, MissionDefinition, MissionGoalDefinition, MissionRegistry, MonsterRegistry,
};
use cdda_protocol::{EocConditionV1, EocEffectV1, MissionDefinitionV1, MissionGoalV1};

use crate::eocs::runtime_effect;

pub(super) fn runtime_mission_catalog(
    registry: &MissionRegistry,
    monsters: &MonsterRegistry,
    runtime_monster_type_ids: Option<&BTreeSet<String>>,
) -> Result<(Vec<MissionDefinitionV1>, BTreeSet<String>), Box<dyn std::error::Error>> {
    let mut definitions = registry
        .iter()
        .filter_map(|(_id, definition)| {
            runtime_mission_candidate(definition, monsters, runtime_monster_type_ids)
        })
        .collect::<Vec<_>>();
    loop {
        let ids = definitions
            .iter()
            .map(|definition| definition.mission_type_id.clone())
            .collect::<BTreeSet<_>>();
        let before = definitions.len();
        definitions.retain(|definition| {
            phases_reference_only_admitted_missions(
                [
                    &definition.start_effects,
                    &definition.end_effects,
                    &definition.fail_effects,
                ],
                &ids,
            )
        });
        if definitions.len() == before {
            break;
        }
    }
    let ids = definitions
        .iter()
        .map(|definition| definition.mission_type_id.clone())
        .collect::<BTreeSet<_>>();
    if !cdda_protocol::mission_catalog_is_valid(&definitions) {
        return Err("runtime mission catalog is invalid".into());
    }
    Ok((definitions, ids))
}

fn runtime_mission_candidate(
    definition: &MissionDefinition,
    monsters: &MonsterRegistry,
    runtime_monster_type_ids: Option<&BTreeSet<String>>,
) -> Option<MissionDefinitionV1> {
    if !definition.is_fully_supported()
        || !phase_is_supported(&definition.start_effects)
        || !phase_is_supported(&definition.end_effects)
        || !phase_is_supported(&definition.fail_effects)
    {
        return None;
    }
    let goal = match definition.goal {
        MissionGoalDefinition::Null => {
            if !definition.item_type_id.is_empty()
                || !definition.monster_type_id.is_empty()
                || !definition.monster_species_id.is_empty()
                || definition.monster_kill_goal != -1
            {
                return None;
            }
            MissionGoalV1::Null
        }
        MissionGoalDefinition::FindItem => {
            // Pinned completion searches carried items plus owned, visible,
            // reachable ground and vehicle cargo within radius five, then
            // consumes from that crafting-inventory source set and spills
            // containers. The current world kernel cannot represent that
            // whole source-selection contract, so admitting any production
            // find-item mission here would silently change its objective.
            return None;
        }
        MissionGoalDefinition::KillMonsterType => {
            if !definition.item_type_id.is_empty()
                || !definition.monster_species_id.is_empty()
                || definition.item_count != 1
                || monsters.get(&definition.monster_type_id).is_none()
                || runtime_monster_type_ids
                    .is_some_and(|ids| !ids.contains(&definition.monster_type_id))
            {
                return None;
            }
            MissionGoalV1::KillMonsterType {
                monster_type_id: definition.monster_type_id.clone(),
                count: u32::try_from(definition.monster_kill_goal).ok()?,
            }
        }
        MissionGoalDefinition::KillMonsterSpecies => {
            if !definition.item_type_id.is_empty()
                || !definition.monster_type_id.is_empty()
                || definition.item_count != 1
            {
                return None;
            }
            let monster_type_ids = monsters
                .iter()
                .filter(|(id, monster)| {
                    monster.species.contains(&definition.monster_species_id)
                        && runtime_monster_type_ids.is_none_or(|ids| ids.contains(*id))
                })
                .map(|(id, _monster)| id.to_owned())
                .collect::<Vec<_>>();
            if monster_type_ids.is_empty() {
                return None;
            }
            MissionGoalV1::KillMonsterSpecies {
                monster_species_id: definition.monster_species_id.clone(),
                monster_type_ids,
                count: u32::try_from(definition.monster_kill_goal).ok()?,
            }
        }
        _ => return None,
    };
    Some(MissionDefinitionV1 {
        mission_type_id: definition.id.clone(),
        name: definition.name.clone(),
        description: definition.description.clone(),
        difficulty: definition.difficulty,
        value: definition.value,
        goal,
        start_effects: definition
            .start_effects
            .iter()
            .map(runtime_effect)
            .collect(),
        end_effects: definition.end_effects.iter().map(runtime_effect).collect(),
        fail_effects: definition.fail_effects.iter().map(runtime_effect).collect(),
    })
}

fn phase_is_supported(effects: &[EocEffectDefinition]) -> bool {
    // Mission phase callbacks execute during assignment/completion in pinned
    // CDDA. Until that ordered callback kernel is present, only the exact
    // standard no-op phase is admitted.
    effects.is_empty()
}

fn phases_reference_only_admitted_missions<const N: usize>(
    phases: [&[EocEffectV1]; N],
    admitted: &BTreeSet<String>,
) -> bool {
    phases
        .into_iter()
        .all(|effects| effects_reference_only_admitted_missions(effects, admitted))
}

fn effects_reference_only_admitted_missions(
    effects: &[EocEffectV1],
    admitted: &BTreeSet<String>,
) -> bool {
    effects.iter().all(|effect| match effect {
        EocEffectV1::AssignMission { mission_type_id }
        | EocEffectV1::FinishMission {
            mission_type_id, ..
        } => admitted.contains(mission_type_id),
        EocEffectV1::Conditional {
            condition,
            then_effects,
            else_effects,
        } => {
            condition_references_only_admitted_missions(condition, admitted)
                && effects_reference_only_admitted_missions(then_effects, admitted)
                && effects_reference_only_admitted_missions(else_effects, admitted)
        }
        EocEffectV1::Confirmation {
            accept_effects: then_effects,
            decline_effects: else_effects,
            ..
        } => {
            effects_reference_only_admitted_missions(then_effects, admitted)
                && effects_reference_only_admitted_missions(else_effects, admitted)
        }
        _ => true,
    })
}

fn condition_references_only_admitted_missions(
    condition: &EocConditionV1,
    admitted: &BTreeSet<String>,
) -> bool {
    match condition {
        EocConditionV1::HasMission { mission_type_id } => admitted.contains(mission_type_id),
        EocConditionV1::Not(condition) => {
            condition_references_only_admitted_missions(condition, admitted)
        }
        EocConditionV1::And(conditions) | EocConditionV1::Or(conditions) => conditions
            .iter()
            .all(|condition| condition_references_only_admitted_missions(condition, admitted)),
        _ => true,
    }
}
