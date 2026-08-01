//! Authoritative deterministic NPC spawning and dialogue execution.

use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ActorBodyPartSnapshotV1, ActorEffectSnapshotV1, ActorId, CharacterCreationStatsV1,
    CommandRejection, CommandSequence, DialogueTopicV1, InteractionCancellationReasonV1,
    InteractionChoiceV1, InteractionContextV1, InteractionId, MAX_ACTOR_BASE_STAT,
    MAX_DIALOGUE_TOPIC_STACK, MAX_NPC_NAME_BYTES, NO_FACTION_ID, NpcClassV1, NpcDistributionV1,
    NpcId, NpcMissionOfferV1, NpcOpinionV1, NpcPersonalityV1, NpcSnapshotV1, NpcSocialStateV1,
    NpcTemplateV1, PendingInteractionV1, ProficiencyLevelSnapshot, SimTick, SkillLevelSnapshot,
    WorldEvent, WorldEventKind, WorldPosition, npc_class_catalog_is_valid,
    npc_dialogue_catalog_is_valid,
};
use rand_core::Rng;
use serde::{Deserialize, Serialize};

use crate::{ItemInstance, SimError, WorldState};

const DIALOGUE_LIFETIME_TICKS: u64 = 5 * 60 * SimTick::HZ;
pub(super) const DIALOGUE_FALLBACK_CHOICE_ID: &str = "__fallback_done";
pub(super) const DIALOGUE_FALLBACK_CHOICE_LABEL: &str = "Bye.";

const BUILTIN_MISSION_ACCEPT: &str = "__builtin_mission_accept";
const BUILTIN_MISSION_REJECT: &str = "__builtin_mission_reject";
const BUILTIN_MISSION_ADVICE: &str = "__builtin_mission_advice";
const BUILTIN_MISSION_NOT_YET: &str = "__builtin_mission_not_yet";
const BUILTIN_MISSION_REPORT: &str = "__builtin_mission_report";
const BUILTIN_MISSION_NO_REWARD: &str = "__builtin_mission_no_reward";
const BUILTIN_MISSION_REWARD: &str = "__builtin_mission_reward";
const BUILTIN_MISSION_CLEAR: &str = "__builtin_mission_clear";
const BUILTIN_MISSION_DONE: &str = "__builtin_mission_done";
const BUILTIN_MISSION_OFFER_PREFIX: &str = "__builtin_mission_offer/";
const BUILTIN_MISSION_ASSIGNED_PREFIX: &str = "__builtin_mission_assigned/";
const NPC_RANDOM_AGE_MIN: i32 = 18;
const NPC_RANDOM_AGE_MAX: i32 = 55;
const NPC_MIN_HEIGHT_CENTIMETERS: i32 = 145;
const NPC_MAX_HEIGHT_CENTIMETERS: i32 = 200;

struct NpcTopicPresentation {
    prompt: String,
    choices: Vec<InteractionChoiceV1>,
}

struct BuiltinMissionTransition {
    next_topic_id: Option<String>,
    selected_mission_id: Option<cdda_protocol::MissionId>,
}

fn npc_roll_inclusive(rng: &mut impl Rng, first: i32, second: i32) -> Result<i32, SimError> {
    let minimum = i64::from(first.min(second));
    let maximum = i64::from(first.max(second));
    let width = u64::try_from(maximum - minimum)
        .map_err(|_| SimError::NumericOverflow)?
        .checked_add(1)
        .ok_or(SimError::NumericOverflow)?;
    let offset = i64::try_from(rng.next_u64() % width).map_err(|_| SimError::NumericOverflow)?;
    i32::try_from(minimum + offset).map_err(|_| SimError::NumericOverflow)
}

fn evaluate_npc_distribution(
    distribution: &NpcDistributionV1,
    rng: &mut impl Rng,
) -> Result<f32, SimError> {
    match distribution {
        NpcDistributionV1::Constant { value_bits } => Ok(f32::from_bits(*value_bits)),
        NpcDistributionV1::OneIn { denominator_bits } => {
            let denominator = f64::from(f32::from_bits(*denominator_bits));
            let sample = f64::from(rng.next_u32()) / f64::from(u32::MAX);
            Ok(if sample <= 1.0 / denominator {
                1.0
            } else {
                0.0
            })
        }
        NpcDistributionV1::Range { first, second } => {
            Ok(npc_roll_inclusive(rng, *first, *second)? as f32)
        }
        NpcDistributionV1::Dice { count, sides } => {
            let mut total = 0_i32;
            for _ in 0..*count {
                total = total
                    .checked_add(npc_roll_inclusive(rng, 1, *sides)?)
                    .ok_or(SimError::NumericOverflow)?;
            }
            Ok(total as f32)
        }
        NpcDistributionV1::Sum(children) => {
            let mut values = children.iter();
            let mut total =
                evaluate_npc_distribution(values.next().ok_or(SimError::InvalidNpcDialogue)?, rng)?;
            for child in values {
                total += evaluate_npc_distribution(child, rng)?;
            }
            Ok(total)
        }
        NpcDistributionV1::Multiply(children) => {
            let mut values = children.iter();
            let mut total =
                evaluate_npc_distribution(values.next().ok_or(SimError::InvalidNpcDialogue)?, rng)?;
            for child in values {
                total *= evaluate_npc_distribution(child, rng)?;
            }
            Ok(total)
        }
    }
}

fn roll_npc_distribution_i32(
    distribution: &NpcDistributionV1,
    rng: &mut impl Rng,
) -> Result<i32, SimError> {
    let value = evaluate_npc_distribution(distribution, rng)?;
    if !value.is_finite() || value < i32::MIN as f32 || value > i32::MAX as f32 {
        return Err(SimError::NumericOverflow);
    }
    Ok(value as i32)
}

fn npc_standard_normal_q32(rng: &mut impl Rng) -> i128 {
    let sum = (0..12).fold(0_i128, |total, _| total + i128::from(rng.next_u32()));
    sum - 6 * i128::from(u32::MAX)
}

fn round_q32(value: i128) -> Result<i32, SimError> {
    const SCALE: i128 = 1_i128 << 32;
    let rounded = if value >= 0 {
        value.checked_add(SCALE / 2)
    } else {
        value.checked_sub(SCALE / 2)
    }
    .and_then(|value| value.checked_div(SCALE))
    .ok_or(SimError::NumericOverflow)?;
    i32::try_from(rounded).map_err(|_| SimError::NumericOverflow)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Npc {
    pub(super) id: NpcId,
    pub(super) template_id: String,
    pub(super) name: String,
    pub(super) faction_id: String,
    pub(super) attitude: i32,
    pub(super) hit_by_player: bool,
    pub(super) position: WorldPosition,
    pub(super) class_id: String,
    pub(super) profession: String,
    pub(super) gender: String,
    pub(super) hp: i32,
    pub(super) body_parts: Vec<ActorBodyPartSnapshotV1>,
    pub(super) effects: Vec<ActorEffectSnapshotV1>,
    pub(super) eoc_variables: BTreeMap<String, String>,
    pub(super) base_strength: u16,
    pub(super) base_dexterity: u16,
    pub(super) base_intelligence: u16,
    pub(super) base_perception: u16,
    pub(super) personality: NpcPersonalityV1,
    pub(super) age_years: u16,
    pub(super) height_centimeters: u16,
    pub(super) stamina: u32,
    pub(super) maximum_stamina: u32,
    pub(super) dodge_attempts_remaining: u8,
    pub(super) speed: u16,
    pub(super) action_points: i64,
    pub(super) inventory: BTreeMap<cdda_protocol::ItemId, ItemInstance>,
    pub(super) wielded: Option<cdda_protocol::ItemId>,
    pub(super) worn: Vec<cdda_protocol::ItemId>,
    pub(super) skills: BTreeMap<String, SkillLevelSnapshot>,
    pub(super) proficiencies: BTreeMap<String, ProficiencyLevelSnapshot>,
    pub(super) social: BTreeMap<ActorId, NpcOpinionV1>,
    pub(super) mission_offers: BTreeMap<cdda_protocol::MissionId, String>,
}

impl Npc {
    pub(super) fn snapshot(&self) -> NpcSnapshotV1 {
        NpcSnapshotV1 {
            id: self.id,
            template_id: self.template_id.clone(),
            name: self.name.clone(),
            faction_id: self.faction_id.clone(),
            attitude: self.attitude,
            hit_by_player: self.hit_by_player,
            position: self.position,
            class_id: self.class_id.clone(),
            profession: self.profession.clone(),
            gender: self.gender.clone(),
            hp: self.hp,
            body_parts: self.body_parts.clone(),
            effects: self.effects.clone(),
            eoc_variables: self.eoc_variables.clone(),
            base_strength: self.base_strength,
            base_dexterity: self.base_dexterity,
            base_intelligence: self.base_intelligence,
            base_perception: self.base_perception,
            personality: self.personality.clone(),
            age_years: self.age_years,
            height_centimeters: self.height_centimeters,
            stamina: self.stamina,
            maximum_stamina: self.maximum_stamina,
            dodge_attempts_remaining: self.dodge_attempts_remaining,
            speed: self.speed,
            action_points: self.action_points,
            inventory: self
                .inventory
                .values()
                .map(ItemInstance::snapshot)
                .collect(),
            wielded: self.wielded,
            worn: self.worn.clone(),
            skills: self.skills.values().cloned().collect(),
            proficiencies: self.proficiencies.values().cloned().collect(),
            social: self
                .social
                .iter()
                .map(|(actor_id, opinion)| NpcSocialStateV1 {
                    actor_id: *actor_id,
                    opinion: opinion.clone(),
                })
                .collect(),
            mission_offers: self
                .mission_offers
                .iter()
                .map(|(mission_id, mission_type_id)| NpcMissionOfferV1 {
                    mission_id: *mission_id,
                    mission_type_id: mission_type_id.clone(),
                })
                .collect(),
        }
    }
}

impl WorldState {
    pub(super) fn invalidate_moved_npc_dialogues(
        &mut self,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let mut last_movement = BTreeMap::<ActorId, usize>::new();
        let mut last_dialogue_request = BTreeMap::<ActorId, usize>::new();
        for (index, event) in events.iter().enumerate() {
            match &event.kind {
                WorldEventKind::ActorMoved { actor_id, .. } => {
                    last_movement.insert(*actor_id, index);
                }
                WorldEventKind::InteractionRequested {
                    actor_id,
                    interaction,
                } if matches!(
                    interaction.context,
                    InteractionContextV1::NpcDialogue { .. }
                ) =>
                {
                    last_dialogue_request.insert(*actor_id, index);
                }
                _ => {}
            }
        }
        let invalidated = last_movement
            .into_iter()
            .filter_map(|(actor_id, moved_at)| {
                let actor = self.actors.get(&actor_id)?;
                let interaction = actor.pending_interaction.as_ref()?;
                if !matches!(
                    interaction.context,
                    InteractionContextV1::NpcDialogue { .. }
                ) || last_dialogue_request
                    .get(&actor_id)
                    .is_some_and(|requested_at| *requested_at > moved_at)
                {
                    return None;
                }
                Some((actor_id, interaction.interaction_id))
            })
            .collect::<Vec<_>>();
        for (actor_id, interaction_id) in invalidated {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = None;
            events.push(self.make_event(WorldEventKind::InteractionCanceled {
                actor_id,
                interaction_id,
                reason: InteractionCancellationReasonV1::Invalidated,
            })?);
        }
        Ok(())
    }

    pub(super) fn cancel_dialogues_with_npc(
        &mut self,
        npc_id: NpcId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let pending = self
            .actors
            .iter()
            .filter_map(|(actor_id, actor)| {
                actor.pending_interaction.as_ref().and_then(|interaction| {
                    matches!(
                        interaction.context,
                        InteractionContextV1::NpcDialogue { npc_id: target, .. } if target == npc_id
                    )
                    .then_some((*actor_id, interaction.interaction_id))
                })
            })
            .collect::<Vec<_>>();
        for (actor_id, interaction_id) in pending {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = None;
            events.push(self.make_event(WorldEventKind::InteractionCanceled {
                actor_id,
                interaction_id,
                reason: InteractionCancellationReasonV1::Invalidated,
            })?);
        }
        Ok(())
    }

    /// Finalizes the durable state shared by direct and effect-driven NPC
    /// deaths.  The canonical NPC remains as a dead identity so assigned
    /// missions retain a stable origin; carried roots become ordinary ground
    /// items on the death tile in their existing stable-ID order.
    pub(super) fn finish_npc_death(
        &mut self,
        npc_id: NpcId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let position = self
            .npcs
            .get(&npc_id)
            .filter(|npc| npc.hp <= 0)
            .map(|npc| npc.position)
            .ok_or(SimError::UnknownNpc)?;
        let failed_missions = self
            .actors
            .iter()
            .flat_map(|(actor_id, actor)| {
                actor.missions.values().filter_map(|mission| {
                    (mission.origin_npc_id == Some(npc_id)
                        && mission.status == cdda_protocol::MissionStatusV1::InProgress)
                        .then_some((*actor_id, mission.mission_id))
                })
            })
            .collect::<Vec<_>>();
        for (actor_id, mission_id) in failed_missions {
            self.apply_mission_finish(
                actor_id,
                mission_id,
                false,
                b"npc-death-mission-fail",
                self.tick.0,
                events,
            )?;
        }
        if self
            .npcs
            .get(&npc_id)
            .ok_or(SimError::UnknownNpc)?
            .inventory
            .keys()
            .any(|item_id| self.ground_items.contains_key(item_id))
        {
            return Err(SimError::InvalidItem);
        }
        let inventory = {
            let npc = self.npcs.get_mut(&npc_id).ok_or(SimError::UnknownNpc)?;
            npc.dodge_attempts_remaining = 0;
            npc.action_points = 0;
            npc.wielded = None;
            npc.worn.clear();
            std::mem::take(&mut npc.inventory)
        };
        for (item_id, item) in inventory {
            self.ground_items
                .insert(item_id, super::GroundItem { item, position });
        }
        self.cancel_dialogues_with_npc(npc_id, events)
    }

    pub fn register_npc_dialogue_catalog(
        &mut self,
        classes: Vec<NpcClassV1>,
        templates: Vec<NpcTemplateV1>,
        topics: Vec<DialogueTopicV1>,
    ) -> Result<(), SimError> {
        let mission_ids = self
            .mission_definitions
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let class_ids = classes
            .iter()
            .map(|class| class.class_id.as_str())
            .collect::<BTreeSet<_>>();
        if !npc_class_catalog_is_valid(&classes)
            || !npc_dialogue_catalog_is_valid(&templates, &topics)
            || templates.iter().any(|template| {
                !cdda_protocol::npc_template_runtime_ai_is_supported(template.attitude)
            })
            || templates
                .iter()
                .any(|template| !class_ids.contains(template.class_id.as_str()))
            || templates.iter().any(|template| {
                classes
                    .iter()
                    .find(|class| class.class_id == template.class_id)
                    .is_none_or(|class| !cdda_protocol::npc_template_is_spawn_safe(template, class))
            })
            || templates.iter().any(|template| {
                !template.faction_id.is_empty()
                    && template.faction_id != NO_FACTION_ID
                    && !self.factions.contains_key(&template.faction_id)
            })
            || !self.npc_templates.is_empty()
            || !self.dialogue_topics.is_empty()
            || !self.npc_classes.is_empty()
            || !self.npcs.is_empty()
            || !crate::eocs::mission_references_are_valid_for_ids(
                self.eoc_definitions.values(),
                topics.iter(),
                self.mission_definitions.values(),
                &mission_ids,
            )
            || templates.iter().any(|template| {
                template
                    .mission_offered
                    .iter()
                    .any(|mission_type_id| !mission_ids.contains(mission_type_id.as_str()))
            })
        {
            return Err(SimError::InvalidNpcDialogue);
        }
        self.npc_classes = classes
            .into_iter()
            .map(|class| (class.class_id.clone(), class))
            .collect();
        self.npc_templates = templates
            .into_iter()
            .map(|template| (template.template_id.clone(), template))
            .collect();
        self.dialogue_topics = topics
            .into_iter()
            .map(|topic| (topic.topic_id.clone(), topic))
            .collect();
        Ok(())
    }

    pub fn spawn_npc(
        &mut self,
        template_id: &str,
        generated_name: Option<&str>,
        position: WorldPosition,
    ) -> Result<NpcId, SimError> {
        self.spawn_npc_with_gender(template_id, generated_name, None, position)
    }

    pub(super) fn spawn_npc_with_gender(
        &mut self,
        template_id: &str,
        generated_name: Option<&str>,
        generated_gender: Option<&str>,
        position: WorldPosition,
    ) -> Result<NpcId, SimError> {
        let template = self
            .npc_templates
            .get(template_id)
            .cloned()
            .ok_or(SimError::InvalidNpcDialogue)?;
        let name = template
            .name_unique
            .clone()
            .or_else(|| generated_name.map(str::to_owned))
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= MAX_NPC_NAME_BYTES
                    && !name.chars().any(char::is_control)
            })
            .ok_or(SimError::InvalidNpcDialogue)?;
        if !self.is_passable(position)
            || self.actor_at(position).is_some()
            || self.creature_at(position).is_some()
            || self.npc_at(position).is_some()
        {
            return Err(SimError::SpawnBlocked);
        }
        let required = 1_u64
            .checked_add(
                u64::try_from(template.mission_offered.len())
                    .map_err(|_| SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)?;
        if self.allocator.remaining() < required {
            return Err(SimError::IdReservationExhausted);
        }
        let id = self.allocator.allocate_npc()?;
        let class = self
            .npc_classes
            .get(&template.class_id)
            .cloned()
            .ok_or(SimError::InvalidNpcDialogue)?;
        let mut rng = self.named_rng(b"npc-randomize", &[id.as_u128()], 0);
        let mut personality = NpcPersonalityV1 {
            aggression: i8::try_from(npc_roll_inclusive(&mut rng, -10, 10)?)
                .map_err(|_| SimError::NumericOverflow)?,
            bravery: i8::try_from(npc_roll_inclusive(&mut rng, -3, 10)?)
                .map_err(|_| SimError::NumericOverflow)?,
            collector: i8::try_from(npc_roll_inclusive(&mut rng, -1, 10)?)
                .map_err(|_| SimError::NumericOverflow)?,
            altruism: 0,
        };
        let altruism = round_q32(
            npc_standard_normal_q32(&mut rng)
                .checked_mul(3)
                .ok_or(SimError::NumericOverflow)?,
        )?
        .clamp(-10, 10);
        personality.altruism = i8::try_from(altruism).map_err(|_| SimError::NumericOverflow)?;
        // Pinned randomize always consumes the base gender draw before a
        // template override is applied by load_npc_template.
        let random_gender = if npc_roll_inclusive(&mut rng, 0, 1)? == 0 {
            "male"
        } else {
            "female"
        };
        const Q32: i128 = 1_i128 << 32;
        let height_q32 = (16_835_i128 * Q32 / 100)
            .checked_add(
                npc_standard_normal_q32(&mut rng)
                    .checked_mul(1_550)
                    .and_then(|value| value.checked_div(100))
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)?;
        let random_height =
            round_q32(height_q32)?.clamp(NPC_MIN_HEIGHT_CENTIMETERS, NPC_MAX_HEIGHT_CENTIMETERS);
        let random_age = npc_roll_inclusive(&mut rng, NPC_RANDOM_AGE_MIN, NPC_RANDOM_AGE_MAX)?;
        let mut roll_base_stat = || -> Result<i32, SimError> {
            let mut value = 0_i32;
            for _ in 0..4 {
                value = value
                    .checked_add(npc_roll_inclusive(&mut rng, 1, 3)?)
                    .ok_or(SimError::NumericOverflow)?;
            }
            Ok(value)
        };
        let mut strength =
            i32::from(template.base_strength.unwrap_or(
                u16::try_from(roll_base_stat()?).map_err(|_| SimError::NumericOverflow)?,
            ));
        let mut dexterity =
            i32::from(template.base_dexterity.unwrap_or(
                u16::try_from(roll_base_stat()?).map_err(|_| SimError::NumericOverflow)?,
            ));
        let mut intelligence =
            i32::from(template.base_intelligence.unwrap_or(
                u16::try_from(roll_base_stat()?).map_err(|_| SimError::NumericOverflow)?,
            ));
        let mut perception =
            i32::from(template.base_perception.unwrap_or(
                u16::try_from(roll_base_stat()?).map_err(|_| SimError::NumericOverflow)?,
            ));
        if let Some(fixed) = template.personality.clone() {
            personality = fixed;
        }
        strength = strength
            .checked_add(roll_npc_distribution_i32(&class.bonus_strength, &mut rng)?)
            .ok_or(SimError::NumericOverflow)?;
        dexterity = dexterity
            .checked_add(roll_npc_distribution_i32(&class.bonus_dexterity, &mut rng)?)
            .ok_or(SimError::NumericOverflow)?;
        intelligence = intelligence
            .checked_add(roll_npc_distribution_i32(
                &class.bonus_intelligence,
                &mut rng,
            )?)
            .ok_or(SimError::NumericOverflow)?;
        perception = perception
            .checked_add(roll_npc_distribution_i32(
                &class.bonus_perception,
                &mut rng,
            )?)
            .ok_or(SimError::NumericOverflow)?;
        let mut add_personality =
            |current: i8, distribution: &NpcDistributionV1| -> Result<i8, SimError> {
                let value = i32::from(current)
                    .checked_add(roll_npc_distribution_i32(distribution, &mut rng)?)
                    .ok_or(SimError::NumericOverflow)?
                    .clamp(-10, 10);
                i8::try_from(value).map_err(|_| SimError::NumericOverflow)
            };
        personality.aggression = add_personality(personality.aggression, &class.bonus_aggression)?;
        personality.bravery = add_personality(personality.bravery, &class.bonus_bravery)?;
        personality.collector = add_personality(personality.collector, &class.bonus_collector)?;
        personality.altruism = add_personality(personality.altruism, &class.bonus_altruism)?;
        let stats = CharacterCreationStatsV1 {
            strength: u16::try_from(strength).map_err(|_| SimError::InvalidNpcDialogue)?,
            dexterity: u16::try_from(dexterity).map_err(|_| SimError::InvalidNpcDialogue)?,
            intelligence: u16::try_from(intelligence).map_err(|_| SimError::InvalidNpcDialogue)?,
            perception: u16::try_from(perception).map_err(|_| SimError::InvalidNpcDialogue)?,
        };
        if [
            stats.strength,
            stats.dexterity,
            stats.intelligence,
            stats.perception,
        ]
        .into_iter()
        .any(|value| value == 0 || value > MAX_ACTOR_BASE_STAT)
        {
            return Err(SimError::InvalidNpcDialogue);
        }
        let body_parts = crate::anatomy::initial_body_parts(&self.actor_anatomy, stats)?;
        let hp = cdda_protocol::actor_body_part_summary_hp(&self.actor_anatomy, &body_parts)
            .ok_or(SimError::InvalidActorAnatomy)?;
        let mut skills = BTreeMap::new();
        for skill in &class.skills {
            let level = roll_npc_distribution_i32(&skill.distribution, &mut rng)?.max(0);
            let level = u8::try_from(level).map_err(|_| SimError::InvalidNpcDialogue)?;
            if level > cdda_protocol::MAX_SKILL_LEVEL {
                return Err(SimError::InvalidNpcDialogue);
            }
            if level > 0 {
                skills.insert(
                    skill.skill_id.clone(),
                    SkillLevelSnapshot {
                        skill_id: skill.skill_id.clone(),
                        practical_level: level,
                        practical_experience: 0,
                        theoretical_level: level,
                        theoretical_experience: 0,
                        last_practiced: self.tick,
                    },
                );
            }
        }
        let mut mission_offers = BTreeMap::new();
        for mission_type_id in &template.mission_offered {
            if !self.mission_definitions.contains_key(mission_type_id) {
                return Err(SimError::InvalidMission);
            }
            let mission_id = self.allocator.allocate_mission()?;
            mission_offers.insert(mission_id, mission_type_id.clone());
        }
        self.npcs.insert(
            id,
            Npc {
                id,
                template_id: template.template_id,
                name,
                faction_id: template.faction_id,
                attitude: template.attitude,
                hit_by_player: false,
                position,
                class_id: class.class_id,
                profession: template.name_suffix.unwrap_or(class.name),
                gender: template
                    .gender
                    .or_else(|| generated_gender.map(str::to_owned))
                    .unwrap_or_else(|| random_gender.to_owned()),
                hp,
                body_parts,
                effects: Vec::new(),
                eoc_variables: BTreeMap::new(),
                base_strength: stats.strength,
                base_dexterity: stats.dexterity,
                base_intelligence: stats.intelligence,
                base_perception: stats.perception,
                personality,
                age_years: template
                    .age_years
                    .unwrap_or(u16::try_from(random_age).map_err(|_| SimError::NumericOverflow)?),
                height_centimeters: template.height_centimeters.unwrap_or(
                    u16::try_from(random_height).map_err(|_| SimError::NumericOverflow)?,
                ),
                stamina: crate::DEFAULT_ACTOR_MAXIMUM_STAMINA,
                maximum_stamina: crate::DEFAULT_ACTOR_MAXIMUM_STAMINA,
                dodge_attempts_remaining: crate::combat::ORDINARY_DODGE_ATTEMPTS,
                speed: crate::DEFAULT_ACTOR_SPEED,
                action_points: i64::from(crate::ACTOR_ACTION_THRESHOLD),
                inventory: BTreeMap::new(),
                wielded: None,
                worn: Vec::new(),
                skills,
                proficiencies: BTreeMap::new(),
                social: BTreeMap::new(),
                mission_offers,
            },
        );
        Ok(id)
    }

    pub(super) fn start_npc_dialogue(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        npc_id: NpcId,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let Some(npc) = self.npcs.get(&npc_id) else {
            events.push(self.rejection(actor_id, sequence, CommandRejection::TargetMissing)?);
            return Ok(());
        };
        if npc.hp <= 0 || !adjacent(actor_position, npc.position) {
            events.push(self.rejection(actor_id, sequence, CommandRejection::TargetMissing)?);
            return Ok(());
        }
        let template_id = npc.template_id.clone();
        let reset_talk_attitude = npc.attitude == 1;
        let will_talk = self.npc_will_talk_to_player_faction(npc);
        let topic_id = self
            .npc_templates
            .get(&template_id)
            .ok_or(SimError::InvalidNpcDialogue)?
            .chat_topic_id
            .clone();
        if !self.resolve_pending_before_npc_dialogue(actor_id, sequence, events)? {
            return Ok(());
        }
        if !will_talk {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::NpcRefusedDialogue,
            )?);
            return Ok(());
        }
        if reset_talk_attitude {
            self.npcs
                .get_mut(&npc_id)
                .ok_or(SimError::UnknownNpc)?
                .attitude = 0;
        }
        // Multiplayer adaptation: faction identities are public world data.
        // Do not mutate the legacy single-avatar `known_by_u` bit when one
        // particular actor starts a conversation.
        if topic_id == "TALK_DONE" {
            return Ok(());
        }
        self.request_npc_topic(actor_id, npc_id, vec![topic_id], None, events)
    }

    pub(super) fn apply_npc_dialogue_response(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        npc_id: NpcId,
        topic_stack: &[String],
        selected_mission_id: Option<cdda_protocol::MissionId>,
        choice_id: &str,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let actor_position = self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .position;
        let Some((npc_position, will_talk)) = self
            .npcs
            .get(&npc_id)
            .map(|npc| (npc.position, self.npc_will_talk_to_player_faction(npc)))
        else {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::TargetMissing,
                events,
            );
        };
        if !adjacent(actor_position, npc_position) {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::TargetMissing,
                events,
            );
        }
        if !will_talk {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::NpcRefusedDialogue,
                events,
            );
        }
        let Some(topic_id) = topic_stack.last() else {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        };
        let expected =
            self.npc_topic_presentation(actor_id, npc_id, topic_id, selected_mission_id)?;
        if !expected
            .choices
            .iter()
            .any(|choice| choice.choice_id == choice_id)
        {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        if let Some(transition) = self.apply_builtin_mission_choice(
            actor_id,
            npc_id,
            interaction_id,
            sequence,
            selected_mission_id,
            choice_id,
            events,
        )? {
            self.actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?
                .pending_interaction = None;
            return match transition.next_topic_id {
                Some(next_topic_id) => {
                    let Some(next_topic_stack) = advance_topic_stack(topic_stack, &next_topic_id)
                    else {
                        return Err(SimError::InvalidNpcDialogue);
                    };
                    match next_topic_stack {
                        Some(stack) => self.request_npc_topic(
                            actor_id,
                            npc_id,
                            stack,
                            transition.selected_mission_id,
                            events,
                        ),
                        None => {
                            events.push(self.make_event(WorldEventKind::InteractionCanceled {
                                actor_id,
                                interaction_id,
                                reason: InteractionCancellationReasonV1::Completed,
                            })?);
                            Ok(())
                        }
                    }
                }
                None => {
                    events.push(self.make_event(WorldEventKind::InteractionCanceled {
                        actor_id,
                        interaction_id,
                        reason: InteractionCancellationReasonV1::Completed,
                    })?);
                    Ok(())
                }
            };
        }
        let response = (choice_id != DIALOGUE_FALLBACK_CHOICE_ID)
            .then(|| {
                self.dialogue_topics.get(topic_id).and_then(|topic| {
                    topic
                        .responses
                        .iter()
                        .find(|response| response.response_id == choice_id)
                })
            })
            .flatten()
            .cloned();
        if choice_id != DIALOGUE_FALLBACK_CHOICE_ID && response.is_none() {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        if let Some(condition) = response
            .as_ref()
            .and_then(|response| response.condition.as_ref())
            && !self.dialogue_condition_matches(actor_id, npc_id, condition)?
        {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        let next_topic_id = response
            .as_ref()
            .map_or("TALK_DONE", |response| response.next_topic_id.as_str());
        let Some(next_topic_stack) = advance_topic_stack(topic_stack, next_topic_id) else {
            events.push(self.rejection(
                actor_id,
                sequence,
                CommandRejection::InvalidInteractionChoice,
            )?);
            return Ok(());
        };
        let opinion_delta = response
            .as_ref()
            .map_or_else(NpcOpinionV1::default, |response| {
                response.opinion_delta.clone()
            });
        let Some(opinion) = self
            .npcs
            .get(&npc_id)
            .and_then(|npc| npc.social.get(&actor_id))
            .cloned()
            .unwrap_or_default()
            .checked_add(&opinion_delta)
            .filter(cdda_protocol::opinion_is_valid)
        else {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        };
        if let Some(response) = &response
            && !self.apply_dialogue_response_effects(
                actor_id,
                npc_id,
                interaction_id,
                sequence,
                &response.effects,
                events,
            )?
        {
            return self.invalidate_npc_dialogue(
                actor_id,
                sequence,
                interaction_id,
                CommandRejection::InvalidInteractionChoice,
                events,
            );
        }
        let npc = self.npcs.get_mut(&npc_id).ok_or(SimError::UnknownNpc)?;
        npc.social.insert(actor_id, opinion);
        self.actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction = None;
        match next_topic_stack {
            Some(topic_stack) => {
                self.request_npc_topic(actor_id, npc_id, topic_stack, selected_mission_id, events)
            }
            None => {
                events.push(self.make_event(WorldEventKind::InteractionCanceled {
                    actor_id,
                    interaction_id,
                    reason: InteractionCancellationReasonV1::Completed,
                })?);
                Ok(())
            }
        }
    }

    fn request_npc_topic(
        &mut self,
        actor_id: ActorId,
        npc_id: NpcId,
        topic_stack: Vec<String>,
        selected_mission_id: Option<cdda_protocol::MissionId>,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let topic_id = topic_stack.last().ok_or(SimError::InvalidNpcDialogue)?;
        let presentation =
            self.npc_topic_presentation(actor_id, npc_id, topic_id, selected_mission_id)?;
        if self
            .actors
            .get(&actor_id)
            .ok_or(SimError::UnknownActor)?
            .pending_interaction
            .is_some()
        {
            return Err(SimError::InvalidNpcDialogue);
        }
        let mut choices = presentation.choices;
        if choices.is_empty() {
            choices.push(InteractionChoiceV1 {
                choice_id: String::from(DIALOGUE_FALLBACK_CHOICE_ID),
                label: String::from(DIALOGUE_FALLBACK_CHOICE_LABEL),
            });
        }
        let interaction = PendingInteractionV1 {
            interaction_id: InteractionId::new(self.world_namespace, self.next_event_counter),
            prompt: presentation.prompt,
            choices,
            created_at_tick: self.tick,
            expires_at_tick: SimTick(
                self.tick
                    .0
                    .checked_add(DIALOGUE_LIFETIME_TICKS)
                    .ok_or(SimError::NumericOverflow)?,
            ),
            context: InteractionContextV1::NpcDialogue {
                npc_id,
                topic_stack,
                selected_mission_id,
            },
        };
        if !cdda_protocol::pending_interaction_is_valid(&interaction, actor_id) {
            return Err(SimError::InvalidNpcDialogue);
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

    fn npc_topic_presentation(
        &self,
        actor_id: ActorId,
        npc_id: NpcId,
        topic_id: &str,
        selected_mission_id: Option<cdda_protocol::MissionId>,
    ) -> Result<NpcTopicPresentation, SimError> {
        let topic = self
            .dialogue_topics
            .get(topic_id)
            .ok_or(SimError::InvalidNpcDialogue)?;
        let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let mut choices = topic
            .responses
            .iter()
            .map(|response| {
                let matches = response.condition.as_ref().map_or(Ok(true), |condition| {
                    self.dialogue_condition_matches(actor_id, npc_id, condition)
                })?;
                Ok(matches.then(|| InteractionChoiceV1 {
                    choice_id: response.response_id.clone(),
                    label: response.text.clone(),
                }))
            })
            .collect::<Result<Vec<Option<_>>, SimError>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let mut dynamic_line = topic.dynamic_line.clone();
        if !topic.replace_built_in_responses {
            match topic_id {
                "TALK_MISSION_LIST" => {
                    dynamic_line = match npc.mission_offers.len() {
                        0 => String::from("I don't have any jobs for you."),
                        1 => String::from("I have a job for you.  Want to hear about it?"),
                        _ => String::from("I have other jobs for you.  Want to hear about them?"),
                    };
                    choices.push(builtin_choice(
                        BUILTIN_MISSION_DONE,
                        if npc.mission_offers.is_empty() {
                            "Oh, okay."
                        } else {
                            "Never mind, I'm not interested."
                        },
                    ));
                    for (mission_id, mission_type_id) in &npc.mission_offers {
                        let definition = self
                            .mission_definitions
                            .get(mission_type_id)
                            .ok_or(SimError::InvalidMission)?;
                        choices.push(InteractionChoiceV1 {
                            choice_id: format!(
                                "{BUILTIN_MISSION_OFFER_PREFIX}{}",
                                mission_id.counter()
                            ),
                            label: if npc.mission_offers.len() == 1 {
                                String::from("Tell me about it.")
                            } else {
                                definition.name.clone()
                            },
                        });
                    }
                }
                "TALK_MISSION_LIST_ASSIGNED" => {
                    let assigned = actor
                        .missions
                        .values()
                        .filter(|mission| {
                            mission.origin_npc_id == Some(npc_id)
                                && mission.status == cdda_protocol::MissionStatusV1::InProgress
                        })
                        .collect::<Vec<_>>();
                    dynamic_line = match assigned.len() {
                        0 => String::from("You're not working on anything for me now."),
                        1 => String::from("What about it?"),
                        _ => String::from("Which job?"),
                    };
                    choices.push(builtin_choice(BUILTIN_MISSION_DONE, "Never mind."));
                    for mission in &assigned {
                        let definition = self
                            .mission_definitions
                            .get(&mission.mission_type_id)
                            .ok_or(SimError::InvalidMission)?;
                        choices.push(InteractionChoiceV1 {
                            choice_id: format!(
                                "{BUILTIN_MISSION_ASSIGNED_PREFIX}{}",
                                mission.mission_id.counter()
                            ),
                            label: if assigned.len() == 1 {
                                String::from("I have news.")
                            } else {
                                definition.name.clone()
                            },
                        });
                    }
                }
                "TALK_MISSION_OFFER" => {
                    let definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = mission_dialogue_line(definition, "offer")?;
                    choices.extend([
                        builtin_choice(BUILTIN_MISSION_ACCEPT, "I'll do it!"),
                        builtin_choice(BUILTIN_MISSION_REJECT, "Not interested."),
                    ]);
                }
                "TALK_MISSION_ACCEPTED" => {
                    let definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = mission_dialogue_line(definition, "accepted")?;
                    choices.extend([
                        builtin_choice(BUILTIN_MISSION_DONE, "Not a problem."),
                        builtin_choice(BUILTIN_MISSION_ADVICE, "Got any advice?"),
                    ]);
                }
                "TALK_MISSION_REJECTED" => {
                    let definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = mission_dialogue_line(definition, "rejected")?;
                    choices.push(builtin_choice(BUILTIN_MISSION_DONE, "I'm sorry."));
                }
                "TALK_MISSION_ADVICE" => {
                    let definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = mission_dialogue_line(definition, "advice")?;
                    choices.push(builtin_choice(BUILTIN_MISSION_DONE, "Sounds good, thanks."));
                }
                "TALK_MISSION_INQUIRE" => {
                    let mission_id = selected_mission_id.ok_or(SimError::InvalidMission)?;
                    let mission = actor
                        .missions
                        .get(&mission_id)
                        .filter(|mission| mission.origin_npc_id == Some(npc_id))
                        .ok_or(SimError::InvalidMission)?;
                    let definition = self
                        .mission_definitions
                        .get(&mission.mission_type_id)
                        .ok_or(SimError::InvalidMission)?;
                    dynamic_line = mission_dialogue_line(definition, "inquire")?;
                    if mission.status == cdda_protocol::MissionStatusV1::InProgress
                        && self.mission_goal_is_complete(actor_id, mission_id)?
                    {
                        choices.push(builtin_choice(BUILTIN_MISSION_REPORT, "Mission success!"));
                    } else {
                        choices.push(builtin_choice(BUILTIN_MISSION_NOT_YET, "Not yet."));
                    }
                }
                "TALK_MISSION_SUCCESS" => {
                    let definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = mission_dialogue_line(definition, "success")?;
                    choices.push(builtin_choice(
                        BUILTIN_MISSION_NO_REWARD,
                        "Glad to help.  I need no payment.",
                    ));
                    if definition.has_generic_rewards {
                        choices.push(builtin_choice(
                            BUILTIN_MISSION_REWARD,
                            "How about some items as payment?",
                        ));
                    }
                }
                "TALK_MISSION_FAILURE" => {
                    let definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = mission_dialogue_line(definition, "failure")?;
                    choices.push(builtin_choice(
                        BUILTIN_MISSION_CLEAR,
                        "I'm sorry.  I did what I could.",
                    ));
                }
                "TALK_MISSION_REWARD" => {
                    let _definition = self.selected_npc_mission_definition(
                        actor_id,
                        npc_id,
                        selected_mission_id,
                    )?;
                    dynamic_line = String::from("Sure, here you go!");
                    choices.push(builtin_choice(BUILTIN_MISSION_CLEAR, "Thank you."));
                }
                _ => {}
            }
        }
        if choices.len() > cdda_protocol::MAX_INTERACTION_CHOICES {
            return Err(SimError::InvalidNpcDialogue);
        }
        if choices.is_empty() {
            choices.push(InteractionChoiceV1 {
                choice_id: String::from(DIALOGUE_FALLBACK_CHOICE_ID),
                label: String::from(DIALOGUE_FALLBACK_CHOICE_LABEL),
            });
        }
        Ok(NpcTopicPresentation {
            prompt: render_dialogue_prompt(&npc.name, &dynamic_line),
            choices,
        })
    }

    fn selected_npc_mission_definition(
        &self,
        actor_id: ActorId,
        npc_id: NpcId,
        selected_mission_id: Option<cdda_protocol::MissionId>,
    ) -> Result<&cdda_protocol::MissionDefinitionV1, SimError> {
        let mission_id = selected_mission_id.ok_or(SimError::InvalidMission)?;
        let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
        let mission_type_id = npc.mission_offers.get(&mission_id).or_else(|| {
            self.actors.get(&actor_id).and_then(|actor| {
                actor
                    .missions
                    .get(&mission_id)
                    .filter(|mission| mission.origin_npc_id == Some(npc_id))
                    .map(|mission| &mission.mission_type_id)
            })
        });
        mission_type_id
            .and_then(|mission_type_id| self.mission_definitions.get(mission_type_id))
            .ok_or(SimError::InvalidMission)
    }

    fn apply_builtin_mission_choice(
        &mut self,
        actor_id: ActorId,
        npc_id: NpcId,
        interaction_id: InteractionId,
        sequence: CommandSequence,
        selected_mission_id: Option<cdda_protocol::MissionId>,
        choice_id: &str,
        events: &mut Vec<WorldEvent>,
    ) -> Result<Option<BuiltinMissionTransition>, SimError> {
        let transition = if let Some(counter) = choice_id.strip_prefix(BUILTIN_MISSION_OFFER_PREFIX)
        {
            let mission_id = self
                .npcs
                .get(&npc_id)
                .and_then(|npc| {
                    npc.mission_offers
                        .keys()
                        .find(|mission_id| mission_id.counter().to_string() == counter)
                })
                .copied()
                .ok_or(SimError::InvalidMission)?;
            BuiltinMissionTransition {
                next_topic_id: Some(String::from("TALK_MISSION_OFFER")),
                selected_mission_id: Some(mission_id),
            }
        } else if let Some(counter) = choice_id.strip_prefix(BUILTIN_MISSION_ASSIGNED_PREFIX) {
            let mission_id = self
                .actors
                .get(&actor_id)
                .and_then(|actor| {
                    actor.missions.keys().find(|mission_id| {
                        mission_id.counter().to_string() == counter
                            && actor.missions.get(mission_id).is_some_and(|mission| {
                                mission.origin_npc_id == Some(npc_id)
                                    && mission.status == cdda_protocol::MissionStatusV1::InProgress
                            })
                    })
                })
                .copied()
                .ok_or(SimError::InvalidMission)?;
            BuiltinMissionTransition {
                next_topic_id: Some(String::from("TALK_MISSION_INQUIRE")),
                selected_mission_id: Some(mission_id),
            }
        } else {
            let mission_id = selected_mission_id;
            match choice_id {
                BUILTIN_MISSION_ACCEPT => {
                    let mission_id = mission_id.ok_or(SimError::InvalidMission)?;
                    self.apply_npc_mission_accept(
                        actor_id,
                        npc_id,
                        mission_id,
                        interaction_id,
                        sequence,
                        events,
                    )?;
                    BuiltinMissionTransition {
                        next_topic_id: Some(String::from("TALK_MISSION_ACCEPTED")),
                        selected_mission_id: Some(mission_id),
                    }
                }
                BUILTIN_MISSION_REJECT => BuiltinMissionTransition {
                    next_topic_id: Some(String::from("TALK_MISSION_REJECTED")),
                    selected_mission_id: mission_id,
                },
                BUILTIN_MISSION_ADVICE => BuiltinMissionTransition {
                    next_topic_id: Some(String::from("TALK_MISSION_ADVICE")),
                    selected_mission_id: mission_id,
                },
                BUILTIN_MISSION_NOT_YET | BUILTIN_MISSION_DONE => BuiltinMissionTransition {
                    next_topic_id: Some(String::from("TALK_NONE")),
                    selected_mission_id: mission_id,
                },
                BUILTIN_MISSION_REPORT => {
                    let mission_id = mission_id.ok_or(SimError::InvalidMission)?;
                    if !self.mission_goal_is_complete(actor_id, mission_id)? {
                        return Err(SimError::InvalidMission);
                    }
                    self.actors
                        .get(&actor_id)
                        .and_then(|actor| actor.missions.get(&mission_id))
                        .filter(|mission| mission.origin_npc_id == Some(npc_id))
                        .ok_or(SimError::InvalidMission)?;
                    self.apply_mission_finish(
                        actor_id,
                        mission_id,
                        true,
                        b"npc-mission-end",
                        sequence.0,
                        events,
                    )?;
                    BuiltinMissionTransition {
                        next_topic_id: Some(String::from("TALK_MISSION_SUCCESS")),
                        selected_mission_id: Some(mission_id),
                    }
                }
                BUILTIN_MISSION_NO_REWARD | BUILTIN_MISSION_CLEAR => BuiltinMissionTransition {
                    next_topic_id: Some(String::from("TALK_NONE")),
                    selected_mission_id: mission_id,
                },
                BUILTIN_MISSION_REWARD => {
                    let definition =
                        self.selected_npc_mission_definition(actor_id, npc_id, mission_id)?;
                    if !definition.has_generic_rewards {
                        return Err(SimError::InvalidMission);
                    }
                    let value = definition.value;
                    let opinion = self
                        .npcs
                        .get(&npc_id)
                        .and_then(|npc| npc.social.get(&actor_id))
                        .cloned()
                        .unwrap_or_default();
                    let next = opinion
                        .checked_add(&NpcOpinionV1 {
                            owed: value,
                            ..NpcOpinionV1::default()
                        })
                        .filter(cdda_protocol::opinion_is_valid)
                        .ok_or(SimError::NumericOverflow)?;
                    self.npcs
                        .get_mut(&npc_id)
                        .ok_or(SimError::UnknownNpc)?
                        .social
                        .insert(actor_id, next);
                    BuiltinMissionTransition {
                        next_topic_id: Some(String::from("TALK_MISSION_REWARD")),
                        selected_mission_id: mission_id,
                    }
                }
                _ => return Ok(None),
            }
        };
        Ok(Some(transition))
    }

    pub(super) fn recovered_npc_interactions_are_exact(&self) -> Result<bool, SimError> {
        for (actor_id, actor) in &self.actors {
            let Some(interaction) = &actor.pending_interaction else {
                continue;
            };
            let InteractionContextV1::NpcDialogue {
                npc_id,
                topic_stack,
                selected_mission_id,
            } = &interaction.context
            else {
                continue;
            };
            let Some(npc) = self.npcs.get(npc_id) else {
                return Ok(false);
            };
            if !adjacent(actor.position, npc.position)
                || !self.npc_will_talk_to_player_faction(npc)
                || !topic_stack
                    .iter()
                    .all(|topic_id| self.dialogue_topics.contains_key(topic_id))
            {
                return Ok(false);
            }
            if selected_mission_id.is_some_and(|mission_id| {
                !npc.mission_offers.contains_key(&mission_id)
                    && actor
                        .missions
                        .get(&mission_id)
                        .is_none_or(|mission| mission.origin_npc_id != Some(*npc_id))
            }) {
                return Ok(false);
            }
            let Some(topic_id) = topic_stack.last() else {
                return Ok(false);
            };
            let Ok(expected) =
                self.npc_topic_presentation(*actor_id, *npc_id, topic_id, *selected_mission_id)
            else {
                return Ok(false);
            };
            if interaction.prompt != expected.prompt || interaction.choices != expected.choices {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn npc_dialogue_quit_choice(
        &self,
        interaction: &PendingInteractionV1,
    ) -> Option<String> {
        let InteractionContextV1::NpcDialogue { topic_stack, .. } = &interaction.context else {
            return None;
        };
        if interaction.choices.as_slice()
            == [InteractionChoiceV1 {
                choice_id: String::from(DIALOGUE_FALLBACK_CHOICE_ID),
                label: String::from(DIALOGUE_FALLBACK_CHOICE_LABEL),
            }]
        {
            return Some(String::from(DIALOGUE_FALLBACK_CHOICE_ID));
        }
        let topic = self.dialogue_topics.get(topic_stack.last()?)?;
        if interaction
            .choices
            .iter()
            .any(|choice| choice.choice_id == BUILTIN_MISSION_DONE)
        {
            return Some(String::from(BUILTIN_MISSION_DONE));
        }
        interaction.choices.iter().find_map(|choice| {
            topic
                .responses
                .iter()
                .find(|response| response.response_id == choice.choice_id)
                .filter(|response| {
                    response.effects.is_empty() && response.next_topic_id == "TALK_DONE"
                })
                .map(|_| choice.choice_id.clone())
        })
    }

    fn invalidate_npc_dialogue(
        &mut self,
        actor_id: ActorId,
        sequence: CommandSequence,
        interaction_id: InteractionId,
        rejection: CommandRejection,
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
        events.push(self.rejection(actor_id, sequence, rejection)?);
        Ok(())
    }
}

fn builtin_choice(choice_id: &str, label: &str) -> InteractionChoiceV1 {
    InteractionChoiceV1 {
        choice_id: choice_id.to_owned(),
        label: label.to_owned(),
    }
}

fn mission_dialogue_line(
    definition: &cdda_protocol::MissionDefinitionV1,
    key: &str,
) -> Result<String, SimError> {
    definition
        .dialogue
        .get(key)
        .cloned()
        .ok_or(SimError::InvalidMission)
}

pub(super) fn render_dialogue_prompt(npc_name: &str, dynamic_line: &str) -> String {
    if let Some(line) = dynamic_line.strip_prefix('&') {
        line.to_owned()
    } else if let Some(line) = dynamic_line.strip_prefix('*') {
        format!("{npc_name} {line}")
    } else {
        format!("{npc_name}: \"{dynamic_line}\"")
    }
}

fn advance_topic_stack(topic_stack: &[String], next_topic_id: &str) -> Option<Option<Vec<String>>> {
    if next_topic_id == "TALK_DONE" {
        return Some(None);
    }
    let mut next = topic_stack.to_vec();
    if next_topic_id == "TALK_NONE" {
        let category = next.last().map_or(-1, |topic| topic_category(topic));
        next.pop();
        while category != -1
            && next
                .last()
                .is_some_and(|topic| topic_category(topic) == category)
        {
            next.pop();
        }
        return Some((!next.is_empty()).then_some(next));
    }
    if next.len() >= MAX_DIALOGUE_TOPIC_STACK {
        return None;
    }
    next.push(next_topic_id.to_owned());
    Some(Some(next))
}

fn topic_category(topic: &str) -> i32 {
    match topic {
        "TALK_MISSION_START"
        | "TALK_MISSION_DESCRIBE"
        | "TALK_MISSION_OFFER"
        | "TALK_MISSION_ACCEPTED"
        | "TALK_MISSION_REJECTED"
        | "TALK_MISSION_ADVICE"
        | "TALK_MISSION_INQUIRE"
        | "TALK_MISSION_SUCCESS"
        | "TALK_MISSION_SUCCESS_LIE"
        | "TALK_MISSION_FAILURE"
        | "TALK_MISSION_REWARD"
        | "TALK_MISSION_END"
        | "TALK_MISSION_DESCRIBE_URGENT" => 1,
        "TALK_SHARE_EQUIPMENT" | "TALK_GIVE_EQUIPMENT" | "TALK_DENY_EQUIPMENT" => 2,
        "TALK_SUGGEST_FOLLOW" | "TALK_AGREE_FOLLOW" | "TALK_DENY_FOLLOW" => 3,
        "TALK_COMBAT_ENGAGEMENT" => 4,
        "TALK_COMBAT_COMMANDS" => 5,
        "TALK_TRAIN"
        | "TALK_TRAIN_START"
        | "TALK_TRAIN_FORCE"
        | "TALK_TRAIN_NPC_START"
        | "TALK_TRAIN_NPC_FORCE" => 6,
        "TALK_MISC_RULES" => 7,
        "TALK_AIM_RULES" => 8,
        "TALK_FRIEND" | "TALK_GIVE_ITEM" | "TALK_USE_ITEM" => 9,
        "TALK_SIZE_UP" | "TALK_ASSESS_PERSON" | "TALK_LOOK_AT" | "TALK_OPINION" | "TALK_SHOUT" => {
            99
        }
        _ => -1,
    }
}

fn adjacent(left: WorldPosition, right: WorldPosition) -> bool {
    left.z == right.z
        && left.x.abs_diff(right.x) <= 1
        && left.y.abs_diff(right.y) <= 1
        && left != right
}
