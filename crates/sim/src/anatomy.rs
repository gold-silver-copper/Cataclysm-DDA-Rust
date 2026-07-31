use cdda_protocol::{
    ActorBodyPartSnapshotV1, ActorEffectSnapshotV1, ActorId, AnatomyDefinitionV1,
    BodyPartHpModifiersV1, BodyPartPrototypeV1, CharacterCreationStatsV1, HealingItemTypeV1,
    ItemId, SimTick, actor_body_part_summary_hp, actor_body_parts_are_valid,
    anatomy_definition_is_valid,
};
use rand_core::Rng;

use crate::{NEEDS_INTERVAL_TICKS, SimError, WorldState, actor_skill_level};

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

pub(super) struct HealingItemOutcome {
    pub body_part_id: String,
    pub healed_hp: i32,
    pub remaining_charges: i32,
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
            limb_types: vec![String::from("torso")],
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

impl WorldState {
    pub(super) fn healing_item_body_part_choices(
        &self,
        actor_id: ActorId,
        item_id: ItemId,
        healing: &HealingItemTypeV1,
    ) -> Result<Vec<String>, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let required_charges = i32::from(healing.charges_per_use.max(1));
        if actor.worn.contains(&item_id)
            || actor.inventory.get(&item_id).is_none_or(|item| {
                item.type_id != healing.item_type_id || item.charges < required_charges
            })
        {
            return Ok(Vec::new());
        }
        Ok(actor
            .body_parts
            .iter()
            .filter(|part| healing_part_score(actor, part, healing).is_some())
            .map(|part| part.body_part_id.clone())
            .collect())
    }

    pub(super) fn apply_healing_item(
        &mut self,
        actor_id: ActorId,
        item_id: ItemId,
        healing: &HealingItemTypeV1,
        sequence: u64,
        selected_body_part: Option<&str>,
    ) -> Result<Option<HealingItemOutcome>, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let required_charges = i32::from(healing.charges_per_use.max(1));
        if actor.worn.contains(&item_id)
            || actor.inventory.get(&item_id).is_none_or(|item| {
                item.type_id != healing.item_type_id || item.charges < required_charges
            })
        {
            return Ok(None);
        }
        let first_aid = actor_skill_level(actor, "firstaid", false);
        let selected = if let Some(selected_body_part) = selected_body_part {
            actor.body_parts.iter().position(|part| {
                part.body_part_id == selected_body_part
                    && healing_part_score(actor, part, healing).is_some()
            })
        } else {
            actor
                .body_parts
                .iter()
                .enumerate()
                .fold(None, |selected: Option<(usize, u64)>, (index, part)| {
                    let Some(score) = healing_part_score(actor, part, healing) else {
                        return selected;
                    };
                    match selected {
                        Some((_selected_index, selected_score)) if selected_score >= score => {
                            selected
                        }
                        _ => Some((index, score)),
                    }
                })
                .map(|(index, _score)| index)
        };
        let Some(selected) = selected else {
            return Ok(None);
        };
        let part_id = actor.body_parts[selected].body_part_id.clone();
        let (base, scaling) = match part_id.as_str() {
            "head" => (healing.head_power_milli, healing.head_scaling_milli),
            "torso" => (healing.torso_power_milli, healing.torso_scaling_milli),
            _ => (healing.limb_power_milli, healing.limb_scaling_milli),
        };
        let scaled = |base: i32, per_skill: i32| -> Result<u32, SimError> {
            let milli = i64::from(base)
                .checked_add(i64::from(per_skill) * i64::from(first_aid))
                .ok_or(SimError::NumericOverflow)?;
            u32::try_from((milli + 500) / 1_000).map_err(|_| SimError::NumericOverflow)
        };
        let heal_amount = scaled(base, scaling)?;
        let bandage_intensity =
            scaled(healing.bandages_power_milli, healing.bandages_scaling_milli)?;
        let disinfectant_intensity = scaled(
            healing.disinfectant_power_milli,
            healing.disinfectant_scaling_milli,
        )?;
        let mut rng = self.named_session_rng(
            b"healing-item",
            &[actor_id.as_u128(), item_id.as_u128()],
            sequence,
        );
        let clean_bite = rng.next_u32() % 1_000_000 < healing.bite_chance_millionths;
        let clean_infection = rng.next_u32() % 1_000_000 < healing.infect_chance_millionths;
        let actor = self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?;
        let part = actor
            .body_parts
            .get_mut(selected)
            .ok_or(SimError::InvalidActorAnatomy)?;
        let previous_hp = part.current_hp;
        part.current_hp = part
            .current_hp
            .saturating_add(i32::try_from(heal_amount).map_err(|_| SimError::NumericOverflow)?)
            .min(part.maximum_hp);
        let healed_hp = part.current_hp.saturating_sub(previous_hp);
        if healing.bleed > 0 {
            let stop_level = u32::from(healing.bleed) * u32::from(first_aid) / 2;
            actor.effects.retain(|effect| {
                !(effect.effect_id == "bleed"
                    && effect.body_part_id.as_deref() == Some(&part_id)
                    && stop_level.saturating_mul(3) > effect.intensity)
            });
        }
        if clean_bite {
            actor.effects.retain(|effect| {
                !(effect.effect_id == "bite" && effect.body_part_id.as_deref() == Some(&part_id))
            });
        }
        if clean_infection {
            let recovered_until = actor
                .effects
                .iter()
                .find(|effect| {
                    effect.effect_id == "infected"
                        && effect.body_part_id.as_deref() == Some(&part_id)
                })
                .map(|effect| effect.expires_at_tick);
            actor.effects.retain(|effect| {
                !(effect.effect_id == "infected"
                    && effect.body_part_id.as_deref() == Some(&part_id))
            });
            if let Some(expires_at_tick) = recovered_until {
                let expires_at_tick = actor
                    .effects
                    .iter()
                    .find(|effect| effect.effect_id == "recover" && effect.body_part_id.is_none())
                    .map_or(expires_at_tick, |effect| {
                        effect.expires_at_tick.max(expires_at_tick)
                    });
                actor.effects.retain(|effect| {
                    !(effect.effect_id == "recover" && effect.body_part_id.is_none())
                });
                actor.effects.push(ActorEffectSnapshotV1 {
                    effect_id: String::from("recover"),
                    body_part_id: None,
                    intensity: 1,
                    expires_at_tick,
                });
            }
        }
        for (effect_id, intensity) in [
            ("bandaged", bandage_intensity),
            ("disinfected", disinfectant_intensity),
        ] {
            if intensity == 0 {
                continue;
            }
            let intensity = intensity.clamp(1, 16);
            let duration = u64::from(intensity)
                .checked_mul(6 * 60 * 60 * SimTick::HZ)
                .ok_or(SimError::NumericOverflow)?;
            actor.effects.retain(|effect| {
                !(effect.effect_id == effect_id && effect.body_part_id.as_deref() == Some(&part_id))
            });
            actor.effects.push(ActorEffectSnapshotV1 {
                effect_id: String::from(effect_id),
                body_part_id: Some(part_id.clone()),
                intensity,
                expires_at_tick: SimTick(
                    self.tick
                        .0
                        .checked_add(duration)
                        .ok_or(SimError::NumericOverflow)?,
                ),
            });
        }
        actor.effects.sort_by(|left, right| {
            (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
        });
        actor.hp = actor_body_part_summary_hp(&self.actor_anatomy, &actor.body_parts)
            .ok_or(SimError::InvalidActorAnatomy)?;
        let previous_charges = actor
            .inventory
            .get(&item_id)
            .ok_or(SimError::UnknownItem)?
            .charges;
        let remove = previous_charges <= required_charges;
        let remaining_charges = if remove {
            0
        } else {
            previous_charges - required_charges
        };
        if remove {
            actor.inventory.remove(&item_id);
            if actor.wielded == Some(item_id) {
                actor.wielded = None;
            }
            actor.worn.retain(|worn| *worn != item_id);
        } else {
            actor
                .inventory
                .get_mut(&item_id)
                .ok_or(SimError::UnknownItem)?
                .charges -= required_charges;
        }
        Ok(Some(HealingItemOutcome {
            body_part_id: part_id,
            healed_hp,
            remaining_charges,
        }))
    }

    /// Pinned default avatar healing while fully at rest: 0.0001 HP per
    /// upstream turn becomes a 0.03-HP deterministic remainder roll at the
    /// existing five-minute needs boundary. Awake healing is zero without a
    /// mutation or effect, and zero-HP limbs remain broken.
    pub(super) fn advance_natural_healing(&mut self) -> Result<(), SimError> {
        if !self.tick.0.is_multiple_of(NEEDS_INTERVAL_TICKS) {
            return Ok(());
        }
        let interval = self.tick.0 / NEEDS_INTERVAL_TICKS;
        let actor_ids = self.actors.keys().copied().collect::<Vec<_>>();
        for actor_id in actor_ids {
            let candidates = {
                let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
                if actor.hp <= 0 || !actor.sleeping {
                    continue;
                }
                actor
                    .body_parts
                    .iter()
                    .enumerate()
                    .filter(|(_, part)| part.current_hp > 0 && part.current_hp < part.maximum_hp)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>()
            };
            let healed = candidates
                .into_iter()
                .filter(|index| {
                    let mut rng = self.named_rng(
                        b"natural-healing",
                        &[actor_id.as_u128(), *index as u128],
                        interval,
                    );
                    rng.next_u32() % 100 < 3
                })
                .collect::<Vec<_>>();
            if healed.is_empty() {
                continue;
            }
            let actor = self
                .actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?;
            for index in healed {
                let part = actor
                    .body_parts
                    .get_mut(index)
                    .ok_or(SimError::InvalidActorAnatomy)?;
                part.current_hp = part.current_hp.saturating_add(1).min(part.maximum_hp);
            }
            actor.hp = actor_body_part_summary_hp(&self.actor_anatomy, &actor.body_parts)
                .ok_or(SimError::InvalidActorAnatomy)?;
        }
        Ok(())
    }
}

fn healing_part_score(
    actor: &super::Actor,
    part: &ActorBodyPartSnapshotV1,
    healing: &HealingItemTypeV1,
) -> Option<u64> {
    if part.current_hp <= 0 {
        return None;
    }
    let missing = part.maximum_hp.saturating_sub(part.current_hp) as u64;
    let effect_score = actor
        .effects
        .iter()
        .filter(|effect| effect.body_part_id.as_deref() == Some(&part.body_part_id))
        .fold(0_u64, |score, effect| {
            score
                + match effect.effect_id.as_str() {
                    "bleed" if healing.bleed > 0 => u64::from(effect.intensity) * 100,
                    "bite" if healing.bite_chance_millionths > 0 => 1_000,
                    "infected" if healing.infect_chance_millionths > 0 => 2_000,
                    _ => 0,
                }
        });
    let dressing =
        missing > 0 && (healing.bandages_power_milli > 0 || healing.disinfectant_power_milli > 0);
    let score = missing.saturating_add(effect_score);
    (score > 0 || dressing).then_some(score)
}
