//! Deterministic character combat-resource arithmetic.

use cdda_protocol::{ActorId, CreatureId};
use rand_core::Rng;

use super::{
    SimError, WorldState, actor_skill_level, effective_base_stat,
    pinned_bash_weapon_melee_accuracy_twelfths, pinned_melee_hit_roll,
    pinned_unarmed_melee_accuracy_quarters,
};

pub(super) const DEFAULT_MAXIMUM_STAMINA: u32 = 8_500;
pub(super) const BASE_STAMINA_REGEN_PER_SECOND: u32 = 20;
pub(super) const WINDED_STAMINA_REGEN_PER_SECOND: u32 = 2;
pub(super) const ORDINARY_DODGE_ATTEMPTS: u8 = 1;
pub(super) const MAXIMUM_STAMINA_LIMIT: u32 = 1_000_000;

// Pinned samples of CDDA's `1 - logarithmic_range(10%, 90%, stamina)`.
// Runtime interpolation is integer-only so authoritative rolls and replay do
// not depend on the target platform's floating-point implementation.
const STAMINA_DODGE_MODIFIER_MILLIONTHS: [u32; 101] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1_923, 4_040, 6_370, 8_932, 11_749, 14_843, 18_241, 21_970,
    26_058, 30_538, 35_442, 40_806, 46_666, 53_061, 60_031, 67_619, 75_866, 84_815, 94_510,
    104_994, 116_306, 128_487, 141_572, 155_592, 170_575, 186_540, 203_499, 221_455, 240_402,
    260_320, 281_179, 302_937, 325_536, 348_909, 372_971, 397_630, 422_780, 448_306, 474_089,
    500_000, 525_911, 551_694, 577_220, 602_370, 627_029, 651_091, 674_464, 697_063, 718_821,
    739_680, 759_598, 778_545, 796_501, 813_460, 829_425, 844_408, 858_428, 871_513, 883_694,
    895_006, 905_490, 915_185, 924_134, 932_381, 939_969, 946_939, 953_334, 959_194, 964_558,
    969_462, 973_942, 978_030, 981_759, 985_157, 988_251, 991_068, 993_630, 995_960, 998_077,
    1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000,
    1_000_000, 1_000_000, 1_000_000,
];

pub(super) fn stamina_dodge_modifier_millionths(stamina: u32, maximum: u32) -> u32 {
    if maximum == 0 || stamina == 0 {
        return 0;
    }
    if stamina >= maximum {
        return 1_000_000;
    }
    let scaled = u64::from(stamina) * 100;
    let index = usize::try_from(scaled / u64::from(maximum))
        .unwrap_or(100)
        .min(100);
    let remainder = scaled % u64::from(maximum);
    let low = u64::from(STAMINA_DODGE_MODIFIER_MILLIONTHS[index]);
    let high = u64::from(STAMINA_DODGE_MODIFIER_MILLIONTHS[(index + 1).min(100)]);
    u32::try_from(low + (high - low) * remainder / u64::from(maximum))
        .expect("interpolated stamina modifier is bounded")
}

pub(super) fn can_dodge_at_stamina(stamina: u32, maximum: u32) -> bool {
    stamina_dodge_modifier_millionths(stamina, maximum) > 110_000
}

pub(super) fn dodge_roll(
    dexterity: u16,
    dodge_skill: u8,
    speed: u16,
    stamina: u32,
    maximum_stamina: u32,
) -> i64 {
    let half_points = u128::from(dexterity) + u128::from(dodge_skill) * 2;
    let modifier = u128::from(stamina_dodge_modifier_millionths(stamina, maximum_stamina));
    let speed = u128::from(speed.min(100));
    // `get_dodge` is DEX / 2 + dodge skill; `dodge_roll` multiplies it by 5.
    let scaled = half_points * 5 * modifier * speed / (2 * 1_000_000 * 100);
    i64::try_from(scaled).expect("validated actor stats bound dodge roll")
}

pub(super) fn dodge_stamina_cost(dodge_skill: u8) -> u32 {
    // floor(base burn 15 * 6 * (20 - dodge skill) / 20)
    90 * (20 - u32::from(dodge_skill.min(20))) / 20
}

pub(super) fn melee_stamina_cost(weight_milligrams: u64, melee_skill: u8) -> u32 {
    let standard = weight_milligrams / 16_000 + 50;
    let adjusted = standard.saturating_sub(u64::from(melee_skill));
    u32::try_from(adjusted.max(50).min(u64::from(u32::MAX)))
        .expect("melee stamina cost was clamped")
}

impl WorldState {
    pub(super) fn actor_melee_accuracy(
        &self,
        source: ActorId,
    ) -> Result<Option<(i64, u8)>, SimError> {
        let actor = self.actors.get(&source).ok_or(SimError::UnknownActor)?;
        let melee_skill = actor_skill_level(actor, "melee", false);
        match actor.wielded {
            None => Ok(Some((
                pinned_unarmed_melee_accuracy_quarters(actor.base_dexterity, melee_skill),
                4,
            ))),
            Some(item_id) => {
                if self.actor_bash_strength(source)?.is_none() {
                    return Ok(None);
                }
                let weapon = actor.inventory.get(&item_id).ok_or(SimError::UnknownItem)?;
                let Some(profile) = self.smash_item_types.get(&weapon.type_id) else {
                    return Ok(None);
                };
                Ok(Some((
                    pinned_bash_weapon_melee_accuracy_twelfths(
                        actor.base_dexterity,
                        actor_skill_level(actor, "bashing", false),
                        melee_skill,
                        profile.melee_to_hit,
                        profile.bash_damage,
                    ),
                    12,
                )))
            }
        }
    }

    pub(super) fn charge_actor_melee_stamina(&mut self, source: ActorId) -> Result<(), SimError> {
        let cost = {
            let actor = self.actors.get(&source).ok_or(SimError::UnknownActor)?;
            let weight = actor
                .wielded
                .map(|item_id| {
                    let item = actor
                        .inventory
                        .get(&item_id)
                        .ok_or(SimError::UnknownItem)?
                        .snapshot();
                    cdda_protocol::item_snapshot_containment_weight_milligrams(&item)
                        .ok_or(SimError::NumericOverflow)
                })
                .transpose()?
                .unwrap_or(0);
            melee_stamina_cost(weight, actor_skill_level(actor, "melee", false))
        };
        let actor = self.actors.get_mut(&source).ok_or(SimError::UnknownActor)?;
        actor.stamina = actor.stamina.saturating_sub(cost);
        Ok(())
    }

    pub(super) fn actor_dodge_roll(&self, target: ActorId) -> Result<(i64, bool), SimError> {
        let actor = self.actors.get(&target).ok_or(SimError::UnknownActor)?;
        let has_disabling_effect = actor.effects.iter().any(|effect| {
            effect.expires_at_tick > self.tick
                && matches!(
                    effect.effect_id.as_str(),
                    "winded" | "downed" | "fearparalyze" | "narcosis"
                )
        });
        let busy = actor
            .craft_activity
            .as_ref()
            .is_some_and(|activity| !activity.interrupted)
            || actor
                .read_activity
                .as_ref()
                .is_some_and(|activity| !activity.interrupted)
            || actor
                .disassembly_activity
                .as_ref()
                .is_some_and(|activity| !activity.interrupted)
            || actor
                .construction_activity
                .as_ref()
                .is_some_and(|activity| !activity.interrupted);
        if actor.hp <= 0
            || actor.sleeping
            || actor.dodge_attempts_remaining == 0
            || has_disabling_effect
            || busy
            || !can_dodge_at_stamina(actor.stamina, actor.maximum_stamina)
        {
            return Ok((0, false));
        }
        Ok((
            dodge_roll(
                effective_base_stat(actor.base_dexterity),
                actor_skill_level(actor, "dodge", false),
                actor.speed,
                actor.stamina,
                actor.maximum_stamina,
            ),
            true,
        ))
    }

    pub(super) fn consume_actor_dodge_attempt(&mut self, target: ActorId) -> Result<(), SimError> {
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        actor.dodge_attempts_remaining = actor.dodge_attempts_remaining.saturating_sub(1);
        actor.stamina = actor
            .stamina
            .saturating_sub(dodge_stamina_cost(actor_skill_level(actor, "dodge", false)));
        Ok(())
    }

    pub(super) fn actor_actor_hit_spread(
        &self,
        source: ActorId,
        target: ActorId,
        rng_sequence: u64,
    ) -> Result<Option<(i64, bool)>, SimError> {
        let Some((accuracy_numerator, accuracy_denominator)) = self.actor_melee_accuracy(source)?
        else {
            return Ok(None);
        };
        let mut rng = self.named_session_rng(
            b"actor-melee-hit",
            &[source.as_u128(), target.as_u128()],
            rng_sequence,
        );
        let hit_roll = pinned_melee_hit_roll(accuracy_numerator, accuracy_denominator, &mut rng)?;
        let (dodge_roll, dodge_attempted) = self.actor_dodge_roll(target)?;
        Ok(Some((
            hit_roll
                .checked_sub(dodge_roll)
                .ok_or(SimError::NumericOverflow)?,
            dodge_attempted,
        )))
    }

    /// Exact currently admitted player-hit subset. Weapons outside the strict
    /// ordinary bash catalog return `None` and are rejected fail-closed.
    pub(super) fn actor_creature_hit_spread(
        &self,
        source: ActorId,
        target: CreatureId,
        rng_sequence: u64,
    ) -> Result<Option<i64>, SimError> {
        let creature = self
            .creatures
            .get(&target)
            .ok_or(SimError::UnknownCreature)?;
        let Some((accuracy_numerator, accuracy_denominator)) = self.actor_melee_accuracy(source)?
        else {
            return Ok(None);
        };
        let mut rng = self.named_session_rng(
            b"actor-melee-hit",
            &[source.as_u128(), target.as_u128()],
            rng_sequence,
        );
        let hit_roll = pinned_melee_hit_roll(accuracy_numerator, accuracy_denominator, &mut rng)?;
        let dodge_roll = i64::from(creature.dodge)
            .checked_mul(5)
            .ok_or(SimError::NumericOverflow)?;
        let spread = hit_roll
            .checked_sub(dodge_roll)
            .and_then(|spread| {
                spread.checked_sub(super::creature_size_melee_penalty(creature.size))
            })
            .ok_or(SimError::NumericOverflow)?;
        spread
            .checked_add(if creature.immobile {
                super::IMMOBILE_MELEE_HIT_BONUS
            } else {
                0
            })
            .map(Some)
            .ok_or(SimError::NumericOverflow)
    }

    pub(super) fn creature_actor_attack_roll(
        &self,
        source: CreatureId,
        target: ActorId,
        turn_sequence: u64,
    ) -> Result<Option<(i64, bool, bool)>, SimError> {
        let creature = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?;
        if creature.melee_dice == 0 {
            return Ok(None);
        }
        let mut rng = self.named_rng(
            b"creature-melee-hit",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let hit_roll = pinned_melee_hit_roll(i64::from(creature.melee_skill), 1, &mut rng)?;
        let (dodge_roll, dodge_attempted) = self.actor_dodge_roll(target)?;
        let spread = hit_roll
            .checked_sub(dodge_roll)
            .ok_or(SimError::NumericOverflow)?;
        let stumbled = spread < 0 && creature.clumsy_attacks && rng.next_u32().is_multiple_of(4);
        Ok(Some((spread, stumbled, dodge_attempted)))
    }

    /// Retained as a direct characterization boundary for the already pinned
    /// sleeping-target subset. Runtime attacks use the generalized method.
    #[cfg(test)]
    pub(super) fn sleeping_target_creature_attack_roll(
        &self,
        source: CreatureId,
        target: ActorId,
        turn_sequence: u64,
    ) -> Result<Option<(i64, bool)>, SimError> {
        if !self
            .actors
            .get(&target)
            .ok_or(SimError::UnknownActor)?
            .sleeping
        {
            return Ok(None);
        }
        self.creature_actor_attack_roll(source, target, turn_sequence)
            .map(|result| result.map(|(spread, stumbled, _)| (spread, stumbled)))
    }
}
