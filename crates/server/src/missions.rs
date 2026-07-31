use std::collections::BTreeSet;

use cdda_content::{
    EocEffectDefinition, ItemDefinition, ItemRegistry, MissionDefinition, MissionGoalDefinition,
    MissionRegistry, MonsterRegistry, ProficiencyRegistry, RecipeRegistry,
};
use cdda_protocol::{
    AnatomyDefinitionV1, CraftItemPrototypeV1, EocConditionV1, EocEffectV1, ItemGroupContainerV1,
    ItemGroupContentsSourceV1, ItemGroupDefinitionV1, ItemGroupItemPrototypeV1, ItemGroupTargetV1,
    ItemGroupToolChargeStorageV1, ItemPhaseV1, MissionDefinitionV1, MissionGoalV1,
};

use crate::eocs::{
    effects_references_are_supported, runtime_dialogue_effects_are_supported, runtime_effect,
};
use crate::item_groups::{RuntimeItemGroupContent, runtime_item_group_item};

pub(super) fn runtime_mission_catalog(
    registry: &MissionRegistry,
    items: &ItemRegistry,
    monsters: &MonsterRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
    runtime_item_type_ids: Option<&BTreeSet<String>>,
    anatomy: &AnatomyDefinitionV1,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    runtime_monster_type_ids: Option<&BTreeSet<String>>,
    runtime_actor_only_eoc_ids: Option<&BTreeSet<String>>,
) -> Result<(Vec<MissionDefinitionV1>, BTreeSet<String>), Box<dyn std::error::Error>> {
    let candidate_mission_ids = registry
        .iter()
        .filter(|(_id, definition)| definition.is_fully_supported())
        .map(|(id, _definition)| id.to_owned())
        .collect::<BTreeSet<_>>();
    let mut definitions = registry
        .iter()
        .filter_map(|(_id, definition)| {
            runtime_mission_candidate(
                definition,
                items,
                monsters,
                item_group_content,
                runtime_item_type_ids,
                anatomy,
                proficiencies,
                recipes,
                &candidate_mission_ids,
                runtime_monster_type_ids,
                runtime_actor_only_eoc_ids,
            )
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
    items: &ItemRegistry,
    monsters: &MonsterRegistry,
    item_group_content: RuntimeItemGroupContent<'_>,
    runtime_item_type_ids: Option<&BTreeSet<String>>,
    anatomy: &AnatomyDefinitionV1,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    candidate_mission_ids: &BTreeSet<String>,
    runtime_monster_type_ids: Option<&BTreeSet<String>>,
    runtime_actor_only_eoc_ids: Option<&BTreeSet<String>>,
) -> Option<MissionDefinitionV1> {
    if !definition.is_fully_supported()
        || !phase_is_supported(
            &definition.start_effects,
            items,
            anatomy,
            proficiencies,
            recipes,
            candidate_mission_ids,
            runtime_actor_only_eoc_ids,
        )
        || !phase_is_supported(
            &definition.end_effects,
            items,
            anatomy,
            proficiencies,
            recipes,
            candidate_mission_ids,
            runtime_actor_only_eoc_ids,
        )
        || !phase_is_supported(
            &definition.fail_effects,
            items,
            anatomy,
            proficiencies,
            recipes,
            candidate_mission_ids,
            runtime_actor_only_eoc_ids,
        )
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
            if !definition.monster_type_id.is_empty()
                || !definition.monster_species_id.is_empty()
                || definition.monster_kill_goal != -1
            {
                return None;
            }
            let item = items.get(&definition.item_type_id)?;
            if !runtime_find_item_target_is_supported(
                item,
                item_group_content,
                runtime_item_type_ids,
            ) {
                return None;
            }
            MissionGoalV1::FindItem {
                item_type_id: definition.item_type_id.clone(),
                count: u32::try_from(definition.item_count).ok()?,
                count_by_charges: item.count_by_charges(),
            }
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
        dialogue: definition.dialogue.clone(),
        has_generic_rewards: definition.has_generic_rewards,
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

fn phase_is_supported(
    effects: &[EocEffectDefinition],
    items: &ItemRegistry,
    anatomy: &AnatomyDefinitionV1,
    proficiencies: &ProficiencyRegistry,
    recipes: &RecipeRegistry,
    mission_ids: &BTreeSet<String>,
    runtime_actor_only_eoc_ids: Option<&BTreeSet<String>>,
) -> bool {
    let runtime = effects.iter().map(runtime_effect).collect::<Vec<_>>();
    runtime_dialogue_effects_are_supported(effects, anatomy, mission_ids)
        && effects_references_are_supported(effects, items, proficiencies, recipes, mission_ids)
        && !cdda_protocol::eoc_effects_require_target_context(&runtime)
        && !cdda_protocol::eoc_effects_contain_confirmation(&runtime)
        && runtime_actor_only_eoc_ids.is_none_or(|admitted| {
            cdda_protocol::eoc_effect_referenced_ids(&runtime)
                .into_iter()
                .all(|reference| admitted.contains(reference))
        })
}

fn runtime_find_item_target_is_supported(
    item: &ItemDefinition,
    content: RuntimeItemGroupContent<'_>,
    runtime_item_type_ids: Option<&BTreeSet<String>>,
) -> bool {
    if item.category == "software"
        || runtime_item_type_ids.is_some_and(|ids| !ids.contains(&item.id))
        || !matches!(item.phase.as_str(), "" | "SOLID" | "solid")
        || !item.pockets.is_empty()
        || item.flags.contains("HIDDEN_POISON")
        || item.flags.contains("HIDDEN_HALLU")
        || item.subtypes.iter().any(|kind| {
            matches!(
                kind.as_str(),
                "GUN" | "TOOL" | "MAGAZINE" | "GUNMOD" | "TOOLMOD"
            )
        })
    {
        return false;
    }
    let Ok(runtime) = runtime_item_group_item(item, None, content) else {
        return false;
    };
    runtime.prototype.containment.phase == ItemPhaseV1::Solid
        && runtime.prototype.containment.count_by_charges == item.count_by_charges()
        && runtime.prototype.integral_magazines.is_empty()
        && runtime.prototype.magazine_wells.is_empty()
        && runtime.prototype.ammunition_containers.is_empty()
        && runtime.prototype.ranged_weapon.is_none()
        && runtime.prototype.powered_tool.is_none()
}

pub(super) fn runtime_item_group_item_type_ids(
    catalog: &[ItemGroupDefinitionV1],
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for definition in catalog {
        if let Some(wrapper) = &definition.graph.wrapper {
            collect_container_item_type_ids(wrapper, &mut ids);
        }
        for node in &definition.graph.nodes {
            for entry in &node.entries {
                if let ItemGroupTargetV1::Item(item) = &entry.target {
                    collect_group_item_type_ids(item, &mut ids);
                }
                for content in &entry.contents {
                    if let ItemGroupContentsSourceV1::Item(item) = content {
                        collect_group_item_type_ids(item, &mut ids);
                    }
                }
                if let Some(wrapper) = &entry.direct_wrapper {
                    collect_container_item_type_ids(wrapper, &mut ids);
                }
                if let Some(wrapper) = &entry.modifier_container {
                    collect_container_item_type_ids(wrapper, &mut ids);
                }
            }
        }
    }
    ids
}

fn collect_container_item_type_ids(container: &ItemGroupContainerV1, ids: &mut BTreeSet<String>) {
    collect_group_item_type_ids(&container.item, ids);
}

fn collect_group_item_type_ids(item: &ItemGroupItemPrototypeV1, ids: &mut BTreeSet<String>) {
    collect_craft_item_type_id(&item.prototype, ids);
    if let Some(container) = &item.default_container {
        collect_container_item_type_ids(container, ids);
    }
    if let Some(storage) = &item.tool_charge_storage {
        match storage {
            ItemGroupToolChargeStorageV1::Integral { ammunition } => {
                collect_craft_item_type_id(ammunition, ids);
            }
            ItemGroupToolChargeStorageV1::Detachable {
                magazine,
                ammunition,
                ..
            } => {
                collect_craft_item_type_id(magazine, ids);
                collect_craft_item_type_id(ammunition, ids);
            }
            ItemGroupToolChargeStorageV1::MultiDetachable { wells } => {
                for well in wells {
                    collect_craft_item_type_id(&well.magazine, ids);
                    collect_craft_item_type_id(&well.ammunition, ids);
                }
            }
        }
    }
}

fn collect_craft_item_type_id(item: &CraftItemPrototypeV1, ids: &mut BTreeSet<String>) {
    ids.insert(item.type_id.clone());
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
