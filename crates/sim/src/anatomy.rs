use cdda_protocol::{
    ActorBodyPartSnapshotV1, AnatomyDefinitionV1, BodyPartHpModifiersV1, BodyPartPrototypeV1,
    CharacterCreationStatsV1, actor_body_part_summary_hp, actor_body_parts_are_valid,
    anatomy_definition_is_valid,
};
use rand_core::Rng;

use crate::SimError;

pub(super) struct ActorDamageOutcome {
    pub body_part_id: String,
    pub amount: u16,
    pub remaining_part_hp: i32,
    pub remaining_hp: i32,
}

pub(super) struct ActorEffectApplication {
    pub effect_id: String,
    pub body_part_id: Option<String>,
    pub intensity: u32,
    pub duration_turns: u32,
    pub max_intensity: u32,
    pub max_duration_turns: u32,
}

pub(super) fn default_actor_anatomy() -> AnatomyDefinitionV1 {
    AnatomyDefinitionV1 {
        anatomy_id: String::from("legacy_single_part"),
        parts: vec![BodyPartPrototypeV1 {
            body_part_id: String::from("torso"),
            main_part_id: String::from("torso"),
            connected_to_id: String::from("torso"),
            opposite_part_id: String::from("torso"),
            vital: true,
            hit_size_millionths: 1_000_000,
            hit_difficulty_millionths: 1_000_000,
            base_hp: crate::DEFAULT_ACTOR_HP,
            hp_modifiers: BodyPartHpModifiersV1 {
                strength_millionths: 0,
                dexterity_millionths: 0,
                intelligence_millionths: 0,
                perception_millionths: 0,
                health_millionths: 0,
            },
            effects_on_hit: Vec::new(),
            deferred_fields: Vec::new(),
        }],
        deferred_fields: Vec::new(),
    }
}

pub(super) fn initial_body_parts(
    anatomy: &AnatomyDefinitionV1,
    stats: CharacterCreationStatsV1,
) -> Result<Vec<ActorBodyPartSnapshotV1>, SimError> {
    if !anatomy_definition_is_valid(anatomy) {
        return Err(SimError::InvalidActorAnatomy);
    }
    anatomy
        .parts
        .iter()
        .map(|part| {
            let maximum_hp = part.maximum_hp(stats, 0).ok_or(SimError::NumericOverflow)?;
            Ok(ActorBodyPartSnapshotV1 {
                body_part_id: part.body_part_id.clone(),
                current_hp: maximum_hp,
                maximum_hp,
            })
        })
        .collect()
}

pub(super) fn actor_anatomy_state_is_valid(
    anatomy: &AnatomyDefinitionV1,
    parts: &[ActorBodyPartSnapshotV1],
    summary_hp: i32,
) -> bool {
    actor_body_parts_are_valid(anatomy, parts)
        && actor_body_part_summary_hp(anatomy, parts) == Some(summary_hp)
}

pub(super) fn select_body_part_index(
    anatomy: &AnatomyDefinitionV1,
    rng: &mut impl Rng,
) -> Result<usize, SimError> {
    if !anatomy_definition_is_valid(anatomy) {
        return Err(SimError::InvalidActorAnatomy);
    }
    let total = anatomy.parts.iter().try_fold(0_u64, |total, part| {
        total
            .checked_add(part.hit_size_millionths)
            .ok_or(SimError::NumericOverflow)
    })?;
    if total == 0 {
        return Err(SimError::InvalidActorAnatomy);
    }
    let mut ticket = rng.next_u64() % total;
    let mut selected = anatomy.parts.len() - 1;
    for (index, part) in anatomy.parts.iter().enumerate() {
        if ticket < part.hit_size_millionths {
            selected = index;
            break;
        }
        ticket -= part.hit_size_millionths;
    }
    Ok(selected)
}

pub(super) fn apply_damage_to_part(
    anatomy: &AnatomyDefinitionV1,
    parts: &mut [ActorBodyPartSnapshotV1],
    selected: usize,
    amount: u16,
) -> Result<ActorDamageOutcome, SimError> {
    if !actor_body_parts_are_valid(anatomy, parts) || selected >= anatomy.parts.len() {
        return Err(SimError::InvalidActorAnatomy);
    }
    let state = parts
        .get_mut(selected)
        .ok_or(SimError::InvalidActorAnatomy)?;
    state.current_hp = state.current_hp.saturating_sub(i32::from(amount)).max(0);
    let remaining_part_hp = state.current_hp;
    let body_part_id = state.body_part_id.clone();
    let remaining_hp =
        actor_body_part_summary_hp(anatomy, parts).ok_or(SimError::InvalidActorAnatomy)?;
    Ok(ActorDamageOutcome {
        body_part_id,
        amount,
        remaining_part_hp,
        remaining_hp,
    })
}

pub(super) fn on_hit_effects(
    anatomy: &AnatomyDefinitionV1,
    parts: &[ActorBodyPartSnapshotV1],
    selected: usize,
    damage_type: &str,
    amount: u16,
    rng: &mut impl Rng,
) -> Result<Vec<ActorEffectApplication>, SimError> {
    if amount == 0 || !actor_body_parts_are_valid(anatomy, parts) {
        return Ok(Vec::new());
    }
    let prototype = anatomy
        .parts
        .get(selected)
        .ok_or(SimError::InvalidActorAnatomy)?;
    let state = parts.get(selected).ok_or(SimError::InvalidActorAnatomy)?;
    let damage_ratio_millionths = if prototype.main_part_id == prototype.body_part_id {
        i64::from(amount)
            .checked_mul(100)
            .and_then(|value| value.checked_mul(cdda_protocol::ANATOMY_SCALE))
            .and_then(|value| value.checked_div(i64::from(state.maximum_hp)))
            .ok_or(SimError::NumericOverflow)?
    } else {
        i64::from(amount)
            .checked_mul(cdda_protocol::ANATOMY_SCALE)
            .ok_or(SimError::NumericOverflow)?
    };
    let mut applications = Vec::new();
    for effect in &prototype.effects_on_hit {
        if !effect.deferred_fields.is_empty() {
            return Err(SimError::InvalidActorAnatomy);
        }
        if !effect.damage_type_id.is_empty() && effect.damage_type_id != damage_type {
            continue;
        }
        if damage_ratio_millionths < effect.damage_threshold_millionths {
            continue;
        }
        let scaling_millionths = damage_ratio_millionths
            .checked_sub(effect.damage_threshold_millionths)
            .and_then(|value| value.checked_mul(cdda_protocol::ANATOMY_SCALE))
            .and_then(|value| value.checked_div(effect.scale_increment_millionths))
            .ok_or(SimError::NumericOverflow)?;
        let chance = roll_scaled_value(
            effect.chance_percent,
            effect.chance_damage_scaling_millionths,
            scaling_millionths,
            rng,
        )?
        .max(effect.chance_percent);
        if chance < 100 && rng.next_u32() % 100 >= u32::try_from(chance).unwrap_or_default() {
            continue;
        }
        let intensity = roll_scaled_value(
            effect.intensity,
            effect.intensity_damage_scaling_millionths,
            scaling_millionths,
            rng,
        )?
        .max(effect.intensity)
        .min(effect.max_intensity);
        let duration = roll_scaled_value(
            effect.duration_turns,
            effect.duration_damage_scaling_millionths,
            scaling_millionths,
            rng,
        )?
        .max(effect.duration_turns)
        .min(effect.max_duration_turns);
        if intensity <= 0 || duration <= 0 {
            continue;
        }
        applications.push(ActorEffectApplication {
            effect_id: effect.effect_id.clone(),
            body_part_id: (!effect.global).then(|| prototype.body_part_id.clone()),
            intensity: u32::try_from(intensity).map_err(|_| SimError::NumericOverflow)?,
            duration_turns: u32::try_from(duration).map_err(|_| SimError::NumericOverflow)?,
            max_intensity: u32::try_from(effect.max_intensity)
                .map_err(|_| SimError::NumericOverflow)?,
            max_duration_turns: u32::try_from(effect.max_duration_turns)
                .map_err(|_| SimError::NumericOverflow)?,
        });
    }
    Ok(applications)
}

fn roll_scaled_value(
    base: i32,
    per_scale_millionths: i64,
    scaling_millionths: i64,
    rng: &mut impl Rng,
) -> Result<i32, SimError> {
    let value_millionths = i128::from(base)
        .checked_mul(i128::from(cdda_protocol::ANATOMY_SCALE))
        .and_then(|value| {
            i128::from(per_scale_millionths)
                .checked_mul(i128::from(scaling_millionths))
                .and_then(|scaled| scaled.checked_div(i128::from(cdda_protocol::ANATOMY_SCALE)))
                .and_then(|scaled| value.checked_add(scaled))
        })
        .ok_or(SimError::NumericOverflow)?;
    let scale = i128::from(cdda_protocol::ANATOMY_SCALE);
    let whole = value_millionths.div_euclid(scale);
    let remainder = value_millionths.rem_euclid(scale);
    let round_up = remainder > 0
        && rng.next_u64() % (cdda_protocol::ANATOMY_SCALE as u64)
            < u64::try_from(remainder).map_err(|_| SimError::NumericOverflow)?;
    let rounded = whole + if round_up { 1 } else { 0 };
    i32::try_from(rounded).map_err(|_| SimError::NumericOverflow)
}
