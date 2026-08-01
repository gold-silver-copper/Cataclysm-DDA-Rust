//! Canonical NPC identity, dialogue programs, and per-player social state.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{
    ActorBodyPartSnapshotV1, ActorEffectSnapshotV1, ActorId, EocConditionV1, EocEffectV1, ItemId,
    ItemSnapshot, MAX_ACTOR_BASE_STAT, MAX_INTERACTION_CHOICE_LABEL_BYTES, MAX_SKILL_LEVEL, NpcId,
    ProficiencyLevelSnapshot, SkillLevelSnapshot, WorldPosition, eoc_condition_is_valid,
    eoc_effects_are_valid, valid_item_snapshot,
};

pub const MAX_NPC_TEMPLATES: usize = 4_096;
pub const MAX_NPCS: usize = 1_048_576;
pub const MAX_NPC_MISSION_OFFERS: usize = 64;
pub const MAX_DIALOGUE_TOPICS: usize = 16_384;
pub const MAX_DIALOGUE_RESPONSES: usize = 64;
pub const MAX_DIALOGUE_TOPIC_STACK: usize = 64;
pub const MAX_DIALOGUE_TEXT_BYTES: usize = MAX_INTERACTION_CHOICE_LABEL_BYTES;
pub const MAX_DIALOGUE_ID_BYTES: usize = 512;
pub const MAX_NPC_NAME_BYTES: usize = 1_024;
pub const MAX_NPC_OPINION_ABS: i32 = 1_000_000_000;
pub const MAX_NPC_CLASSES: usize = 4_096;
pub const MAX_NPC_CLASS_SKILLS: usize = 1_024;
pub const MAX_NPC_DISTRIBUTION_NODES: usize = 1_024;
pub const NPC_PERSONALITY_MIN: i8 = -10;
pub const NPC_PERSONALITY_MAX: i8 = 10;
pub const NPC_BUILTIN_MISSION_TOPICS: [&str; 10] = [
    "TALK_MISSION_LIST",
    "TALK_MISSION_LIST_ASSIGNED",
    "TALK_MISSION_OFFER",
    "TALK_MISSION_ACCEPTED",
    "TALK_MISSION_REJECTED",
    "TALK_MISSION_ADVICE",
    "TALK_MISSION_INQUIRE",
    "TALK_MISSION_SUCCESS",
    "TALK_MISSION_FAILURE",
    "TALK_MISSION_REWARD",
];

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcOpinionV1 {
    pub trust: i32,
    pub fear: i32,
    pub value: i32,
    pub anger: i32,
    pub owed: i32,
}

/// Pinned `distribution` expression. Float literals retain their exact f32
/// representation because upstream captures them into float-returning lambdas
/// before ordered evaluation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum NpcDistributionV1 {
    Constant { value_bits: u32 },
    OneIn { denominator_bits: u32 },
    Range { first: i32, second: i32 },
    Dice { count: i32, sides: i32 },
    Sum(Vec<Self>),
    Multiply(Vec<Self>),
}

impl Default for NpcDistributionV1 {
    fn default() -> Self {
        Self::Constant {
            value_bits: 0_f32.to_bits(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcPersonalityV1 {
    pub aggression: i8,
    pub bravery: i8,
    pub collector: i8,
    pub altruism: i8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcClassSkillV1 {
    pub skill_id: String,
    /// Index in the pinned `Skill::skills` factory vector.  The order is part
    /// of NPC generation because every skill roll consumes RNG in this order.
    pub load_order: u16,
    pub distribution: NpcDistributionV1,
}

/// Final inherited NPC-class kernel admitted for ordinary spawning. Definitions
/// whose traits, equipment, spells, bionics, mutations, or shop behavior are
/// not represented never enter this catalog.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcClassV1 {
    pub class_id: String,
    pub name: String,
    pub job_description: String,
    pub bonus_strength: NpcDistributionV1,
    pub bonus_dexterity: NpcDistributionV1,
    pub bonus_intelligence: NpcDistributionV1,
    pub bonus_perception: NpcDistributionV1,
    pub bonus_aggression: NpcDistributionV1,
    pub bonus_bravery: NpcDistributionV1,
    pub bonus_collector: NpcDistributionV1,
    pub bonus_altruism: NpcDistributionV1,
    /// Pinned global Skill::skills order, after ALL expansion and bonus merging.
    pub skills: Vec<NpcClassSkillV1>,
}

impl NpcOpinionV1 {
    #[must_use]
    pub fn checked_add(&self, delta: &Self) -> Option<Self> {
        Some(Self {
            trust: self.trust.checked_add(delta.trust)?,
            fear: self.fear.checked_add(delta.fear)?,
            value: self.value.checked_add(delta.value)?,
            anger: self.anger.checked_add(delta.anger)?,
            owed: self.owed.checked_add(delta.owed)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DialogueResponseV1 {
    pub response_id: String,
    pub text: String,
    pub next_topic_id: String,
    pub opinion_delta: NpcOpinionV1,
    pub effects: Vec<EocEffectV1>,
    pub condition: Option<EocConditionV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DialogueTopicV1 {
    pub topic_id: String,
    pub dynamic_line: String,
    pub responses: Vec<DialogueResponseV1>,
    /// Matches `json_talk_topic::replace_built_in_responses`.  When false,
    /// the authoritative dialogue kernel appends the pinned built-in family.
    pub replace_built_in_responses: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcTemplateV1 {
    pub template_id: String,
    pub name_unique: Option<String>,
    pub name_suffix: Option<String>,
    pub gender: Option<String>,
    pub faction_id: String,
    pub class_id: String,
    pub attitude: i32,
    pub mission: String,
    pub mission_offered: Vec<String>,
    pub chat_topic_id: String,
    pub base_strength: Option<u16>,
    pub base_dexterity: Option<u16>,
    pub base_intelligence: Option<u16>,
    pub base_perception: Option<u16>,
    pub personality: Option<NpcPersonalityV1>,
    pub age_years: Option<u16>,
    pub height_centimeters: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcSocialStateV1 {
    pub actor_id: ActorId,
    pub opinion: NpcOpinionV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcSnapshotV1 {
    pub id: NpcId,
    pub template_id: String,
    pub name: String,
    pub faction_id: String,
    pub attitude: i32,
    /// Pinned innocent/attacked marker used by later murder consequences.
    pub hit_by_player: bool,
    pub position: WorldPosition,
    pub class_id: String,
    pub profession: String,
    pub gender: String,
    pub hp: i32,
    pub body_parts: Vec<ActorBodyPartSnapshotV1>,
    pub effects: Vec<ActorEffectSnapshotV1>,
    pub eoc_variables: BTreeMap<String, String>,
    pub base_strength: u16,
    pub base_dexterity: u16,
    pub base_intelligence: u16,
    pub base_perception: u16,
    pub personality: NpcPersonalityV1,
    pub age_years: u16,
    pub height_centimeters: u16,
    pub stamina: u32,
    pub maximum_stamina: u32,
    pub dodge_attempts_remaining: u8,
    pub speed: u16,
    pub action_points: i64,
    pub inventory: Vec<ItemSnapshot>,
    pub wielded: Option<ItemId>,
    pub worn: Vec<ItemId>,
    pub skills: Vec<SkillLevelSnapshot>,
    pub proficiencies: Vec<ProficiencyLevelSnapshot>,
    pub social: Vec<NpcSocialStateV1>,
    pub mission_offers: Vec<NpcMissionOfferV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NpcMissionOfferV1 {
    pub mission_id: super::MissionId,
    pub mission_type_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VisibleNpcSnapshotV1 {
    pub id: NpcId,
    pub template_id: String,
    pub name: String,
    pub faction_id: String,
    pub faction_name: String,
    pub hostile_to_controlled_actor: bool,
    pub position: WorldPosition,
    pub hp: i32,
    pub maximum_hp: i32,
    pub profession: String,
    pub opinion_of_controlled_actor: Option<NpcOpinionV1>,
}

#[must_use]
pub fn npc_class_catalog_is_valid(classes: &[NpcClassV1]) -> bool {
    !classes.is_empty()
        && classes.len() <= MAX_NPC_CLASSES
        && classes
            .windows(2)
            .all(|pair| pair[0].class_id < pair[1].class_id)
        && classes.iter().all(|class| {
            valid_id(&class.class_id)
                && valid_text(&class.name, MAX_NPC_NAME_BYTES)
                && valid_text(&class.job_description, MAX_DIALOGUE_TEXT_BYTES)
                && distribution_is_valid(&class.bonus_strength)
                && distribution_is_valid(&class.bonus_dexterity)
                && distribution_is_valid(&class.bonus_intelligence)
                && distribution_is_valid(&class.bonus_perception)
                && distribution_is_valid(&class.bonus_aggression)
                && distribution_is_valid(&class.bonus_bravery)
                && distribution_is_valid(&class.bonus_collector)
                && distribution_is_valid(&class.bonus_altruism)
                && class.skills.len() <= MAX_NPC_CLASS_SKILLS
                && class
                    .skills
                    .windows(2)
                    .all(|pair| pair[0].load_order < pair[1].load_order)
                && {
                    let mut skill_ids = BTreeSet::new();
                    class.skills.iter().all(|skill| {
                        valid_id(&skill.skill_id)
                            && skill_ids.insert(skill.skill_id.as_str())
                            && distribution_is_valid(&skill.distribution)
                            && distribution_i32_bounds(&skill.distribution).is_some_and(
                                |(_minimum, maximum)| maximum.max(0) <= i32::from(MAX_SKILL_LEVEL),
                            )
                    })
                }
        })
}

/// Inclusive bounds after the pinned float expression has been evaluated and
/// truncated to `int`.  `None` rejects expressions whose finite leaves can
/// overflow an intermediate or whose result cannot be represented by C++'s
/// destination type.
#[must_use]
pub fn distribution_i32_bounds(distribution: &NpcDistributionV1) -> Option<(i32, i32)> {
    fn float_bounds(distribution: &NpcDistributionV1) -> Option<(f32, f32)> {
        match distribution {
            NpcDistributionV1::Constant { value_bits } => {
                let value = f32::from_bits(*value_bits);
                value.is_finite().then_some((value, value))
            }
            NpcDistributionV1::OneIn { .. } => Some((0.0, 1.0)),
            NpcDistributionV1::Range { first, second } => {
                Some(((*first).min(*second) as f32, (*first).max(*second) as f32))
            }
            NpcDistributionV1::Dice { count, sides } => {
                let maximum = count.checked_mul(*sides)?;
                Some((*count as f32, maximum as f32))
            }
            NpcDistributionV1::Sum(children) => {
                let mut children = children.iter();
                let mut bounds = float_bounds(children.next()?)?;
                for child in children {
                    let child = float_bounds(child)?;
                    bounds = (bounds.0 + child.0, bounds.1 + child.1);
                    if !bounds.0.is_finite() || !bounds.1.is_finite() {
                        return None;
                    }
                }
                Some(bounds)
            }
            NpcDistributionV1::Multiply(children) => {
                let mut children = children.iter();
                let mut bounds = float_bounds(children.next()?)?;
                for child in children {
                    let child = float_bounds(child)?;
                    let products = [
                        bounds.0 * child.0,
                        bounds.0 * child.1,
                        bounds.1 * child.0,
                        bounds.1 * child.1,
                    ];
                    if products.iter().any(|value| !value.is_finite()) {
                        return None;
                    }
                    bounds = products.into_iter().fold(
                        (f32::INFINITY, f32::NEG_INFINITY),
                        |(minimum, maximum), value| (minimum.min(value), maximum.max(value)),
                    );
                }
                Some(bounds)
            }
        }
    }

    let (minimum, maximum) = float_bounds(distribution)?;
    let exclusive_maximum = 2_147_483_648.0_f32;
    if minimum < i32::MIN as f32 || maximum >= exclusive_maximum {
        return None;
    }
    Some((minimum as i32, maximum as i32))
}

#[must_use]
pub fn npc_template_is_spawn_safe(template: &NpcTemplateV1, class: &NpcClassV1) -> bool {
    fn stat_is_safe(base: Option<u16>, distribution: &NpcDistributionV1) -> bool {
        let (minimum, maximum) = match distribution_i32_bounds(distribution) {
            Some(bounds) => bounds,
            None => return false,
        };
        let (base_minimum, base_maximum) = base.map_or((4_i32, 12_i32), |value| {
            let value = i32::from(value);
            (value, value)
        });
        base_minimum
            .checked_add(minimum)
            .is_some_and(|value| value > 0)
            && base_maximum
                .checked_add(maximum)
                .is_some_and(|value| value <= i32::from(MAX_ACTOR_BASE_STAT))
    }

    template.class_id == class.class_id
        && stat_is_safe(template.base_strength, &class.bonus_strength)
        && stat_is_safe(template.base_dexterity, &class.bonus_dexterity)
        && stat_is_safe(template.base_intelligence, &class.bonus_intelligence)
        && stat_is_safe(template.base_perception, &class.bonus_perception)
        && [
            &class.bonus_aggression,
            &class.bonus_bravery,
            &class.bonus_collector,
            &class.bonus_altruism,
        ]
        .into_iter()
        .all(|distribution| {
            distribution_i32_bounds(distribution).is_some_and(|(minimum, maximum)| {
                i32::from(NPC_PERSONALITY_MIN)
                    .checked_add(minimum)
                    .is_some()
                    && i32::from(NPC_PERSONALITY_MAX)
                        .checked_add(maximum)
                        .is_some()
            })
        })
}

fn generated_stat_is_possible(
    value: u16,
    fixed_base: Option<u16>,
    distribution: &NpcDistributionV1,
) -> bool {
    distribution_i32_bounds(distribution).is_some_and(|(minimum, maximum)| {
        let (base_minimum, base_maximum) = fixed_base.map_or((4_i32, 12_i32), |base| {
            let base = i32::from(base);
            (base, base)
        });
        i32::from(value) >= base_minimum.saturating_add(minimum)
            && i32::from(value) <= base_maximum.saturating_add(maximum)
    })
}

fn generated_personality_is_possible(
    value: i8,
    fixed_base: Option<i8>,
    random_bounds: (i8, i8),
    distribution: &NpcDistributionV1,
) -> bool {
    distribution_i32_bounds(distribution).is_some_and(|(minimum, maximum)| {
        let (base_minimum, base_maximum) = fixed_base.map_or(random_bounds, |base| (base, base));
        let minimum = i32::from(base_minimum).saturating_add(minimum).clamp(
            i32::from(NPC_PERSONALITY_MIN),
            i32::from(NPC_PERSONALITY_MAX),
        );
        let maximum = i32::from(base_maximum).saturating_add(maximum).clamp(
            i32::from(NPC_PERSONALITY_MIN),
            i32::from(NPC_PERSONALITY_MAX),
        );
        (minimum..=maximum).contains(&i32::from(value))
    })
}

fn distribution_is_valid(root: &NpcDistributionV1) -> bool {
    let mut pending = vec![root];
    let mut nodes = 0_usize;
    while let Some(distribution) = pending.pop() {
        nodes += 1;
        if nodes > MAX_NPC_DISTRIBUTION_NODES {
            return false;
        }
        match distribution {
            NpcDistributionV1::Constant { value_bits } => {
                if !f32::from_bits(*value_bits).is_finite() {
                    return false;
                }
            }
            NpcDistributionV1::OneIn { denominator_bits } => {
                let denominator = f32::from_bits(*denominator_bits);
                if !denominator.is_finite() || denominator <= 1.0 {
                    return false;
                }
            }
            NpcDistributionV1::Range { .. } => {}
            NpcDistributionV1::Dice { count, sides } => {
                if *count < 1 || *sides < 1 || *count > 10_000 || *sides > 1_000_000 {
                    return false;
                }
            }
            NpcDistributionV1::Sum(children) | NpcDistributionV1::Multiply(children) => {
                if children.is_empty() || children.len() > MAX_NPC_DISTRIBUTION_NODES {
                    return false;
                }
                pending.extend(children);
            }
        }
    }
    true
}

#[must_use]
pub fn npc_dialogue_catalog_is_valid(
    templates: &[NpcTemplateV1],
    topics: &[DialogueTopicV1],
) -> bool {
    if templates.len() > MAX_NPC_TEMPLATES
        || topics.len() > MAX_DIALOGUE_TOPICS
        || !templates
            .windows(2)
            .all(|pair| pair[0].template_id < pair[1].template_id)
        || !topics
            .windows(2)
            .all(|pair| pair[0].topic_id < pair[1].topic_id)
    {
        return false;
    }
    let topic_ids = topics
        .iter()
        .map(|topic| topic.topic_id.as_str())
        .collect::<BTreeSet<_>>();
    templates.iter().all(|template| {
        valid_id(&template.template_id)
            && template
                .name_unique
                .as_ref()
                .is_none_or(|name| valid_text(name, MAX_NPC_NAME_BYTES))
            && template
                .name_suffix
                .as_ref()
                .is_none_or(|name| valid_text(name, MAX_NPC_NAME_BYTES))
            && template
                .gender
                .as_ref()
                .is_none_or(|gender| matches!(gender.as_str(), "male" | "female"))
            && optional_id_is_valid(&template.faction_id)
            && (0..=18).contains(&template.attitude)
            && optional_id_is_valid(&template.class_id)
            && npc_template_attitude_is_supported(template.attitude)
            && optional_id_is_valid(&template.mission)
            && template.mission_offered.len() <= MAX_NPC_MISSION_OFFERS
            && template
                .mission_offered
                .iter()
                .all(|mission_type_id| valid_id(mission_type_id))
            && (template.chat_topic_id == "TALK_DONE"
                || topic_ids.contains(template.chat_topic_id.as_str()))
            && template
                .base_strength
                .is_none_or(|value| (1..=MAX_ACTOR_BASE_STAT).contains(&value))
            && template
                .base_dexterity
                .is_none_or(|value| (1..=MAX_ACTOR_BASE_STAT).contains(&value))
            && template
                .base_intelligence
                .is_none_or(|value| (1..=MAX_ACTOR_BASE_STAT).contains(&value))
            && template
                .base_perception
                .is_none_or(|value| (1..=MAX_ACTOR_BASE_STAT).contains(&value))
            && template
                .personality
                .as_ref()
                .is_none_or(personality_is_valid)
            && template
                .age_years
                .is_none_or(|value| (1..=200).contains(&value))
            && template
                .height_centimeters
                .is_none_or(|value| (50..=400).contains(&value))
    }) && topics.iter().all(|topic| {
        let builtin = NPC_BUILTIN_MISSION_TOPICS.contains(&topic.topic_id.as_str())
            && !topic.replace_built_in_responses;
        valid_id(&topic.topic_id)
            && topic.topic_id != "TALK_NONE"
            && topic.topic_id != "TALK_DONE"
            && (builtin || valid_text(&topic.dynamic_line, MAX_DIALOGUE_TEXT_BYTES))
            && (builtin || !matches!(topic.dynamic_line.as_str(), "*" | "&"))
            && ((builtin && topic.responses.len() <= MAX_DIALOGUE_RESPONSES)
                || (!builtin && (1..=MAX_DIALOGUE_RESPONSES).contains(&topic.responses.len())))
            && topic.responses.iter().enumerate().all(|(index, response)| {
                response.response_id == index.to_string()
                    && valid_text(&response.text, MAX_DIALOGUE_TEXT_BYTES)
                    && (matches!(response.next_topic_id.as_str(), "TALK_NONE" | "TALK_DONE")
                        || topic_ids.contains(response.next_topic_id.as_str()))
                    && opinion_is_valid(&response.opinion_delta)
                    && opinion_delta_cannot_trigger_hostility(&response.opinion_delta)
                    && eoc_effects_are_valid(&response.effects)
                    && response
                        .condition
                        .as_ref()
                        .is_none_or(eoc_condition_is_valid)
            })
    })
}

#[must_use]
pub fn npc_snapshot_is_valid(
    npc: &NpcSnapshotV1,
    world_namespace: u64,
    templates: &[NpcTemplateV1],
    classes: &[NpcClassV1],
) -> bool {
    templates
        .iter()
        .find(|template| template.template_id == npc.template_id)
        .is_some_and(|template| {
            classes
                .iter()
                .find(|class| class.class_id == template.class_id)
                .is_some_and(|class| {
                    npc_snapshot_is_valid_for_template(npc, world_namespace, template, class)
                })
        })
}

#[must_use]
pub fn npc_snapshot_is_valid_for_template(
    npc: &NpcSnapshotV1,
    world_namespace: u64,
    template: &NpcTemplateV1,
    class: &NpcClassV1,
) -> bool {
    npc.id.counter() > 0
        && npc.id.world_namespace() == world_namespace
        && npc.template_id == template.template_id
        && valid_text(&npc.name, MAX_NPC_NAME_BYTES)
        && template
            .name_unique
            .as_ref()
            .is_none_or(|name| npc.name == *name)
        && optional_id_is_valid(&npc.faction_id)
        && npc.class_id == template.class_id
        && class.class_id == template.class_id
        && npc.profession == *template.name_suffix.as_ref().unwrap_or(&class.name)
        && valid_text(&npc.profession, MAX_NPC_NAME_BYTES)
        && matches!(npc.gender.as_str(), "male" | "female")
        && template
            .gender
            .as_ref()
            .is_none_or(|gender| npc.gender == *gender)
        && !npc.body_parts.is_empty()
        && {
            let mut body_part_ids = BTreeSet::new();
            npc.body_parts.iter().all(|part| {
                valid_id(&part.body_part_id)
                    && body_part_ids.insert(part.body_part_id.as_str())
                    && part.maximum_hp > 0
                    && (0..=part.maximum_hp).contains(&part.current_hp)
            })
        }
        && npc.base_strength > 0
        && npc.base_strength <= MAX_ACTOR_BASE_STAT
        && generated_stat_is_possible(
            npc.base_strength,
            template.base_strength,
            &class.bonus_strength,
        )
        && npc.base_dexterity > 0
        && npc.base_dexterity <= MAX_ACTOR_BASE_STAT
        && generated_stat_is_possible(
            npc.base_dexterity,
            template.base_dexterity,
            &class.bonus_dexterity,
        )
        && npc.base_intelligence > 0
        && npc.base_intelligence <= MAX_ACTOR_BASE_STAT
        && generated_stat_is_possible(
            npc.base_intelligence,
            template.base_intelligence,
            &class.bonus_intelligence,
        )
        && npc.base_perception > 0
        && npc.base_perception <= MAX_ACTOR_BASE_STAT
        && generated_stat_is_possible(
            npc.base_perception,
            template.base_perception,
            &class.bonus_perception,
        )
        && personality_is_valid(&npc.personality)
        && generated_personality_is_possible(
            npc.personality.aggression,
            template.personality.as_ref().map(|value| value.aggression),
            (NPC_PERSONALITY_MIN, NPC_PERSONALITY_MAX),
            &class.bonus_aggression,
        )
        && generated_personality_is_possible(
            npc.personality.bravery,
            template.personality.as_ref().map(|value| value.bravery),
            (-3, NPC_PERSONALITY_MAX),
            &class.bonus_bravery,
        )
        && generated_personality_is_possible(
            npc.personality.collector,
            template.personality.as_ref().map(|value| value.collector),
            (-1, NPC_PERSONALITY_MAX),
            &class.bonus_collector,
        )
        && generated_personality_is_possible(
            npc.personality.altruism,
            template.personality.as_ref().map(|value| value.altruism),
            (NPC_PERSONALITY_MIN, NPC_PERSONALITY_MAX),
            &class.bonus_altruism,
        )
        && (1..=200).contains(&npc.age_years)
        && template.age_years.is_none_or(|age| npc.age_years == age)
        && (50..=400).contains(&npc.height_centimeters)
        && template
            .height_centimeters
            .is_none_or(|height| npc.height_centimeters == height)
        && npc.maximum_stamina > 0
        && npc.stamina <= npc.maximum_stamina
        && npc.dodge_attempts_remaining <= 1
        && npc.speed > 0
        && npc.action_points >= super::MIN_ACTION_POINTS
        && npc.action_points <= i64::from(super::ACTION_POINT_THRESHOLD)
        && super::actor_eoc_variables_are_valid(&npc.eoc_variables)
        && (npc.hp > 0 || (npc.dodge_attempts_remaining == 0 && npc.action_points == 0))
        && (npc.hp > 0
            || (npc.inventory.is_empty() && npc.wielded.is_none() && npc.worn.is_empty()))
        && npc.inventory.len() <= 256
        && npc.inventory.windows(2).all(|pair| pair[0].id < pair[1].id)
        && npc.inventory.iter().all(valid_item_snapshot)
        && npc.wielded.is_none_or(|id| {
            npc.inventory.iter().any(|item| item.id == id) && !npc.worn.contains(&id)
        })
        && npc.worn.len() <= npc.inventory.len()
        && npc.worn.iter().copied().collect::<BTreeSet<_>>().len() == npc.worn.len()
        && npc
            .worn
            .iter()
            .all(|id| npc.inventory.iter().any(|item| item.id == *id))
        && npc.skills.len() <= super::MAX_SKILLS
        && npc
            .skills
            .windows(2)
            .all(|pair| pair[0].skill_id < pair[1].skill_id)
        && npc.skills.iter().all(|skill| {
            valid_id(&skill.skill_id)
                && skill.practical_level <= MAX_SKILL_LEVEL
                && skill.theoretical_level <= MAX_SKILL_LEVEL
        })
        && npc
            .proficiencies
            .windows(2)
            .all(|pair| pair[0].proficiency_id < pair[1].proficiency_id)
        && npc
            .proficiencies
            .iter()
            .all(|proficiency| valid_id(&proficiency.proficiency_id))
        && (npc.attitude == template.attitude
            || (template.attitude == 1 && npc.attitude == 0)
            || (template.attitude == 11 && matches!(npc.attitude, 0 | 17))
            || (npc.hit_by_player && matches!(npc.attitude, 10 | 17)))
        && (!npc.hit_by_player || matches!(npc.attitude, 10 | 17) || npc.hp <= 0)
        && npc
            .social
            .windows(2)
            .all(|pair| pair[0].actor_id < pair[1].actor_id)
        && npc.social.iter().all(|social| {
            social.actor_id.counter() > 0
                && social.actor_id.world_namespace() == world_namespace
                && opinion_is_valid(&social.opinion)
        })
        && npc.mission_offers.len() <= MAX_NPC_MISSION_OFFERS
        && npc
            .mission_offers
            .windows(2)
            .all(|pair| pair[0].mission_id < pair[1].mission_id)
        && npc.mission_offers.iter().all(|offer| {
            offer.mission_id.counter() > 0
                && offer.mission_id.world_namespace() == world_namespace
                && valid_id(&offer.mission_type_id)
                && npc
                    .mission_offers
                    .iter()
                    .filter(|candidate| candidate.mission_type_id == offer.mission_type_id)
                    .count()
                    <= template
                        .mission_offered
                        .iter()
                        .filter(|mission_type_id| {
                            mission_type_id.as_str() == offer.mission_type_id.as_str()
                        })
                        .count()
        })
}

fn personality_is_valid(personality: &NpcPersonalityV1) -> bool {
    [
        personality.aggression,
        personality.bravery,
        personality.collector,
        personality.altruism,
    ]
    .into_iter()
    .all(|value| (NPC_PERSONALITY_MIN..=NPC_PERSONALITY_MAX).contains(&value))
}

/// Values accepted by the pinned `npc_template::load` implementation.
#[must_use]
pub const fn npc_template_attitude_is_supported(attitude: i32) -> bool {
    matches!(attitude, 0 | 1 | 3 | 5 | 6 | 8 | 9 | 10 | 11 | 13)
}

/// Attitudes whose ordinary turn behavior is represented by the authoritative
/// multiplayer NPC scheduler.
#[must_use]
pub const fn npc_template_runtime_ai_is_supported(attitude: i32) -> bool {
    matches!(attitude, 0 | 1 | 3 | 6 | 10 | 11)
}

/// The pinned non-forced dialogue entry rejects hostile and fleeing template attitudes.
#[must_use]
pub const fn npc_template_attitude_will_talk(attitude: i32) -> bool {
    npc_template_attitude_is_supported(attitude) && !matches!(attitude, 10 | 11)
}

/// The current canonical NPC model has no pinned personality values. Admit only deltas that
/// cannot make a previously false `anger >= 20 + fear - aggression` comparison become true.
#[must_use]
pub const fn opinion_delta_cannot_trigger_hostility(delta: &NpcOpinionV1) -> bool {
    delta.anger <= 0 && delta.fear >= 0
}

#[must_use]
pub fn opinion_is_valid(opinion: &NpcOpinionV1) -> bool {
    [
        opinion.trust,
        opinion.fear,
        opinion.value,
        opinion.anger,
        opinion.owed,
    ]
    .into_iter()
    .all(|value| value.unsigned_abs() <= MAX_NPC_OPINION_ABS as u32)
}

fn optional_id_is_valid(id: &str) -> bool {
    id.is_empty() || valid_id(id)
}

fn valid_id(id: &str) -> bool {
    valid_text(id, MAX_DIALOGUE_ID_BYTES)
}

fn valid_text(text: &str, maximum: usize) -> bool {
    !text.is_empty() && text.len() <= maximum && !text.chars().any(char::is_control)
}
