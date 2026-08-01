//! Persistent field decay and authoritative character contact processing.

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorId, BookStudyInterruptionReason, ConstructionInterruptionReason,
    DisassemblyInterruptionReason, FieldContactDamageV1, FieldContactEffectV1, SimTick, WakeReason,
    WorldEvent, WorldEventKind,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use super::{
    SimError, WorldState, anatomy, apply_actor_effect_applications, tile_index,
    world_position_for_tile_index,
};

impl WorldState {
    pub(super) fn advance_fields(&mut self, events: &mut Vec<WorldEvent>) -> Result<(), SimError> {
        if !self.tick.0.is_multiple_of(SimTick::HZ) {
            return Ok(());
        }
        let mut fields = Vec::new();
        for (coord, chunk) in &self.chunks {
            for (index, tile_fields) in chunk.fields.iter().enumerate() {
                let position = world_position_for_tile_index(*coord, index)?;
                fields.extend(tile_fields.iter().map(|field| {
                    (
                        position,
                        field.field_type_id.clone(),
                        field.intensity,
                        field.display_sequence,
                        field.age_seconds,
                    )
                }));
            }
        }
        for (position, field_type_id, previous_intensity, display_sequence, previous_age) in fields
        {
            let field_type = self
                .field_types
                .get(&field_type_id)
                .cloned()
                .ok_or(SimError::InvalidField)?;
            let actor_id = self.actor_at(position);
            if let Some(actor_id) = actor_id {
                if let Some(contact) = field_type.contact_damage.as_ref() {
                    self.apply_field_contact_damage(
                        actor_id,
                        &field_type_id,
                        previous_intensity,
                        display_sequence,
                        contact,
                        events,
                    )?;
                }
                let level = field_type
                    .intensity_levels
                    .get(usize::from(previous_intensity - 1))
                    .ok_or(SimError::InvalidField)?;
                if !level.contact_effects_supported {
                    return Err(SimError::InvalidField);
                }
                self.apply_field_contact_effects(
                    actor_id,
                    &field_type_id,
                    display_sequence,
                    previous_intensity,
                    &level.contact_effects,
                    events,
                )?;
            }
            let age_seconds = previous_age
                .checked_add(1)
                .ok_or(SimError::NumericOverflow)?;
            let decay_active = age_seconds > 0;
            let decays = if !decay_active || field_type.half_life_seconds == 0 {
                false
            } else if field_type.linear_half_life {
                u64::try_from(age_seconds).is_ok_and(|age| age >= field_type.half_life_seconds)
            } else {
                let threshold = exponential_decay_threshold(field_type.half_life_seconds);
                let mut hasher = blake3::Hasher::new_derive_key("cdda-rust FieldDecayV1");
                hasher.update(&self.world_seed);
                hasher.update(&self.tick.0.to_be_bytes());
                hasher.update(&position.x.to_be_bytes());
                hasher.update(&position.y.to_be_bytes());
                hasher.update(&position.z.to_be_bytes());
                hasher.update(&display_sequence.to_be_bytes());
                hasher.update(&(field_type_id.len() as u64).to_be_bytes());
                hasher.update(field_type_id.as_bytes());
                let mut rng = ChaCha8Rng::from_seed(*hasher.finalize().as_bytes());
                rng.next_u64() < threshold
            };
            let (coord, local) = position.chunk_and_local();
            let chunk = self.chunks.get_mut(&coord).ok_or(SimError::InvalidField)?;
            let index = tile_index(local).ok_or(SimError::InvalidLocalCoordinate)?;
            let tile_fields = chunk
                .fields
                .get_mut(index)
                .ok_or(SimError::InvalidLocalCoordinate)?;
            let field_index = tile_fields
                .binary_search_by(|field| field.field_type_id.cmp(&field_type_id))
                .map_err(|_| SimError::InvalidField)?;
            let decreases_on_contact =
                actor_id.is_some() && field_type.decrease_intensity_on_contact;
            if !decays && !decreases_on_contact {
                tile_fields[field_index].age_seconds = age_seconds;
                continue;
            }
            let reductions = u8::from(decays) + u8::from(decreases_on_contact);
            let intensity = tile_fields[field_index]
                .intensity
                .saturating_sub(reductions);
            if intensity == 0 {
                tile_fields.remove(field_index);
            } else {
                tile_fields[field_index].intensity = intensity;
                tile_fields[field_index].age_seconds = if decays { 0 } else { age_seconds };
            }
            chunk.revision = chunk
                .revision
                .checked_add(1)
                .ok_or(SimError::NumericOverflow)?;
            events.push(self.make_event(WorldEventKind::FieldIntensityChanged {
                position,
                field_type_id,
                intensity,
            })?);
        }
        Ok(())
    }

    fn apply_field_contact_effects(
        &mut self,
        actor_id: ActorId,
        field_type_id: &str,
        display_sequence: u64,
        field_intensity: u8,
        effects: &[FieldContactEffectV1],
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        if effects.is_empty() || self.actors.get(&actor_id).is_none_or(|actor| actor.hp <= 0) {
            return Ok(());
        }
        let mut rng = self.named_rng(
            b"field-contact-effects",
            &[actor_id.as_u128(), u128::from(display_sequence)],
            u64::from(field_intensity),
        );
        let current_tick = self.tick;
        for effect in effects {
            // Player characters cannot occupy vehicles until the vehicle
            // subsystem lands. Preserve every predicate in the contract and
            // execute the exact outside-vehicle branch in the meantime.
            if effect.environmental || effect.immune_outside_vehicle {
                continue;
            }
            if effect.chance_outside_vehicle > 1
                && rng.next_u32() % effect.chance_outside_vehicle != 0
            {
                continue;
            }
            let duration_turns = roll_inclusive_unordered(
                effect.minimum_duration_turns,
                effect.maximum_duration_turns,
                &mut rng,
            )?;
            let blocked = self.actors.get(&actor_id).is_some_and(|actor| {
                actor.effects.iter().any(|active| {
                    active.expires_at_tick > current_tick
                        && effect
                            .blocked_by_effect_ids
                            .binary_search(&active.effect_id)
                            .is_ok()
                })
            });
            let duration_ticks = u64::from(duration_turns)
                .checked_mul(SimTick::HZ)
                .ok_or(SimError::NumericOverflow)?;
            let maximum_duration_ticks = u64::from(effect.maximum_accumulated_duration_turns)
                .checked_mul(SimTick::HZ)
                .ok_or(SimError::NumericOverflow)?;
            if !blocked && duration_turns > 0 {
                let actor = self
                    .actors
                    .get_mut(&actor_id)
                    .ok_or(SimError::UnknownActor)?;
                if let Some(existing) = actor.effects.iter_mut().find(|existing| {
                    existing.effect_id == effect.effect_id
                        && existing.body_part_id == effect.body_part_id
                }) {
                    existing.intensity = effect.intensity;
                    existing.modifiers = effect.modifiers.clone();
                    if existing.expires_at_tick.0 != u64::MAX {
                        let remaining = existing.expires_at_tick.0.saturating_sub(current_tick.0);
                        let added_turns = u128::from(duration_turns)
                            .checked_mul(u128::from(effect.duration_add_percent))
                            .and_then(|value| value.checked_div(100))
                            .and_then(|value| u64::try_from(value).ok())
                            .ok_or(SimError::NumericOverflow)?;
                        let added_ticks = added_turns
                            .checked_mul(SimTick::HZ)
                            .ok_or(SimError::NumericOverflow)?;
                        existing.expires_at_tick = SimTick(
                            current_tick
                                .0
                                .saturating_add(
                                    remaining
                                        .saturating_add(added_ticks)
                                        .min(maximum_duration_ticks),
                                )
                                .min(u64::MAX - 1),
                        );
                    }
                } else {
                    if actor.effects.len() >= 1_024 {
                        return Err(SimError::InvalidField);
                    }
                    actor.effects.push(ActorEffectSnapshotV1 {
                        effect_id: effect.effect_id.clone(),
                        body_part_id: effect.body_part_id.clone(),
                        intensity: effect.intensity,
                        expires_at_tick: SimTick(
                            current_tick
                                .0
                                .checked_add(duration_ticks.min(maximum_duration_ticks))
                                .ok_or(SimError::NumericOverflow)?,
                        ),
                        modifiers: effect.modifiers.clone(),
                    });
                }
            }
            events.push(self.make_event(WorldEventKind::ActorAffectedByField {
                actor_id,
                field_type_id: field_type_id.to_owned(),
                effect_id: effect.effect_id.clone(),
                body_part_id: effect.body_part_id.clone(),
                intensity: effect.intensity,
                duration_turns,
                message: effect.message.clone(),
                message_type: effect.message_type.clone(),
            })?);
        }
        let actor = self
            .actors
            .get_mut(&actor_id)
            .ok_or(SimError::UnknownActor)?;
        actor.effects.sort_by(|left, right| {
            (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
        });
        Ok(())
    }

    fn apply_field_contact_damage(
        &mut self,
        actor_id: ActorId,
        field_type_id: &str,
        field_intensity: u8,
        display_sequence: u64,
        contact: &FieldContactDamageV1,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let body_part_ids = self
            .actor_anatomy
            .parts
            .iter()
            .filter(|part| {
                part.limb_types
                    .binary_search_by(|kind| kind.as_str().cmp(&contact.body_part_type_id))
                    .is_ok()
            })
            .map(|part| part.body_part_id.clone())
            .collect::<Vec<_>>();
        if body_part_ids.is_empty() {
            return Err(SimError::InvalidField);
        }
        let mut rng = self.named_rng(
            b"field-contact-damage",
            &[actor_id.as_u128(), u128::from(display_sequence)],
            u64::from(field_intensity),
        );
        let maximum_damage = u32::from(contact.maximum_damage_base)
            .checked_add(
                u32::from(contact.maximum_damage_per_intensity)
                    .checked_mul(u32::from(field_intensity))
                    .ok_or(SimError::NumericOverflow)?,
            )
            .and_then(|value| value.checked_div(u32::from(contact.maximum_damage_divisor)))
            .ok_or(SimError::NumericOverflow)?;
        let status_intensity = u32::from(contact.status_intensity_base)
            .checked_add(
                u32::from(contact.status_intensity_per_field_intensity)
                    .checked_mul(u32::from(field_intensity))
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)?;
        let maximum_duration = u32::from(contact.status_duration_maximum_base_turns)
            .checked_add(
                u32::from(contact.status_duration_maximum_per_field_intensity)
                    .checked_mul(u32::from(field_intensity))
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)?;
        for body_part_id in body_part_ids {
            if self.actors.get(&actor_id).is_none_or(|actor| actor.hp <= 0) {
                break;
            }
            let damage =
                roll_inclusive(u32::from(contact.minimum_damage), maximum_damage, &mut rng)?;
            let (outcome, was_sleeping) = self.damage_actor_part(
                actor_id,
                &body_part_id,
                &contact.damage_type_id,
                u16::try_from(damage).map_err(|_| SimError::NumericOverflow)?,
                &mut rng,
            )?;
            let duration = roll_inclusive(
                u32::from(contact.status_duration_minimum_turns),
                maximum_duration,
                &mut rng,
            )?;
            let actor = self
                .actors
                .get_mut(&actor_id)
                .ok_or(SimError::UnknownActor)?;
            apply_actor_effect_applications(
                &mut actor.effects,
                vec![anatomy::ActorEffectApplication {
                    effect_id: contact.status_effect_id.clone(),
                    body_part_id: Some(body_part_id),
                    intensity: status_intensity,
                    duration_turns: duration,
                    max_intensity: status_intensity,
                    max_duration_turns: duration,
                }],
                self.tick,
            )?;
            if outcome.amount == 0 {
                continue;
            }
            events.push(self.make_event(WorldEventKind::ActorDamagedByEffect {
                actor_id,
                effect_id: field_type_id.to_owned(),
                body_part_id: outcome.body_part_id,
                amount: outcome.amount,
                remaining_part_hp: outcome.remaining_part_hp,
                remaining_hp: outcome.remaining_hp,
            })?);
            self.interrupt_craft(actor_id, events)?;
            self.interrupt_book_study(actor_id, BookStudyInterruptionReason::Damage, events)?;
            self.interrupt_disassembly(actor_id, DisassemblyInterruptionReason::Damage, events)?;
            self.interrupt_construction(actor_id, ConstructionInterruptionReason::Damage, events)?;
            if was_sleeping && outcome.remaining_hp > 0 {
                self.wake_actor(actor_id, WakeReason::Damage, events)?;
            }
            if outcome.remaining_hp <= 0 {
                events.push(self.make_event(WorldEventKind::ActorDiedFromEffect {
                    actor_id,
                    effect_id: field_type_id.to_owned(),
                })?);
            }
        }
        Ok(())
    }
}

fn roll_inclusive(minimum: u32, maximum: u32, rng: &mut impl Rng) -> Result<u32, SimError> {
    let width = maximum
        .checked_sub(minimum)
        .and_then(|width| width.checked_add(1))
        .ok_or(SimError::InvalidField)?;
    Ok(minimum + rng.next_u32() % width)
}

fn roll_inclusive_unordered(first: u32, second: u32, rng: &mut impl Rng) -> Result<u32, SimError> {
    roll_inclusive(first.min(second), first.max(second), rng)
}

/// Q0.64 probability for `1 - exp(-ln(2) / half_life)`. This keeps the
/// upstream exponential half-life model while avoiding platform libm in the
/// canonical simulation.
pub(super) fn exponential_decay_threshold(half_life_seconds: u64) -> u64 {
    const LN_2_Q64: u64 = 0xb172_17f7_d1cf_79ab;
    let x = LN_2_Q64 / half_life_seconds.max(1);
    let mut term = x;
    let mut result = x;
    for divisor in 2_u64..=32 {
        term = (((u128::from(term) * u128::from(x)) >> 64) as u64) / divisor;
        if term == 0 {
            break;
        }
        if divisor.is_multiple_of(2) {
            result = result.saturating_sub(term);
        } else {
            result = result.saturating_add(term);
        }
    }
    result
}
