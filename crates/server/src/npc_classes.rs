//! Strict projection of pinned NPC-class kernels into canonical simulation data.

use cdda_content::{NpcClassDistributionDefinition, NpcClassRegistry, SkillRegistry};
use cdda_protocol::{NpcClassSkillV1, NpcClassV1, NpcDistributionV1, npc_class_catalog_is_valid};

pub(crate) fn runtime_npc_classes(
    registry: &NpcClassRegistry,
    skills: &SkillRegistry,
) -> Result<Vec<NpcClassV1>, Box<dyn std::error::Error>> {
    let mut classes = registry
        .iter()
        .filter(|(_, class)| class.runtime_complete())
        .map(|(class_id, class)| {
            let class_skills = skills
                .skill_list_order_ids()
                .iter()
                .filter_map(|skill_id| {
                    class.skills.get(skill_id).map(|distribution| {
                        Ok(NpcClassSkillV1 {
                            skill_id: skill_id.clone(),
                            distribution: runtime_distribution(distribution)?,
                        })
                    })
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            Ok(NpcClassV1 {
                class_id: class_id.to_owned(),
                name: class.name.clone(),
                job_description: class.job_description.clone(),
                bonus_strength: runtime_distribution(&class.bonus_strength)?,
                bonus_dexterity: runtime_distribution(&class.bonus_dexterity)?,
                bonus_intelligence: runtime_distribution(&class.bonus_intelligence)?,
                bonus_perception: runtime_distribution(&class.bonus_perception)?,
                bonus_aggression: runtime_distribution(&class.bonus_aggression)?,
                bonus_bravery: runtime_distribution(&class.bonus_bravery)?,
                bonus_collector: runtime_distribution(&class.bonus_collector)?,
                bonus_altruism: runtime_distribution(&class.bonus_altruism)?,
                skills: class_skills,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    classes.sort_by(|left, right| left.class_id.cmp(&right.class_id));
    if !npc_class_catalog_is_valid(&classes) {
        return Err("pinned content has no valid supported NPC-class family".into());
    }
    Ok(classes)
}

fn runtime_distribution(
    distribution: &NpcClassDistributionDefinition,
) -> Result<NpcDistributionV1, Box<dyn std::error::Error>> {
    Ok(match distribution {
        NpcClassDistributionDefinition::Constant { value_bits } => NpcDistributionV1::Constant {
            value_bits: *value_bits,
        },
        NpcClassDistributionDefinition::OneIn { denominator_bits } => NpcDistributionV1::OneIn {
            denominator_bits: *denominator_bits,
        },
        NpcClassDistributionDefinition::Rng { from, to } => NpcDistributionV1::Range {
            first: *from,
            second: *to,
        },
        NpcClassDistributionDefinition::Dice { count, sides } => NpcDistributionV1::Dice {
            count: *count,
            sides: *sides,
        },
        NpcClassDistributionDefinition::Sum(children) => NpcDistributionV1::Sum(
            children
                .iter()
                .map(runtime_distribution)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        NpcClassDistributionDefinition::Multiply(children) => NpcDistributionV1::Multiply(
            children
                .iter()
                .map(runtime_distribution)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    })
}
