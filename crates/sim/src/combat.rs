//! Deterministic character combat-resource arithmetic.

use cdda_protocol::{ActorId, CreatureId, NpcId};
use rand_core::Rng;

use super::{
    SimError, WorldState, actor_effective_dexterity, actor_effective_speed, actor_skill_level,
    anatomy, apply_actor_effect_applications, pinned_bash_weapon_melee_accuracy_twelfths,
    pinned_melee_attack_speed_moves, pinned_melee_hit_roll, pinned_unarmed_melee_accuracy_quarters,
    runtime_armor_is_supported,
};

pub(super) const DEFAULT_MAXIMUM_STAMINA: u32 = 8_500;
pub(super) const NPC_STAMINA: u32 = cdda_protocol::NPC_STAMINA;
pub(super) const BASE_STAMINA_REGEN_PER_SECOND: u32 = 20;
pub(super) const WINDED_STAMINA_REGEN_PER_SECOND: u32 = 2;
pub(super) const ORDINARY_DODGE_ATTEMPTS: u8 = 1;
pub(super) const MAXIMUM_STAMINA_LIMIT: u32 = 1_000_000;

const DAMAGE_MULTIPLIER_SCALE: i128 = 1_000_000;

fn npc_skill_level(npc: &super::npc_dialogue::Npc, skill_id: &str) -> u8 {
    npc.skills
        .get(skill_id)
        .map_or(0, |skill| skill.practical_level)
}

fn npc_effective_dexterity(npc: &super::npc_dialogue::Npc) -> u16 {
    let value = npc
        .effects
        .iter()
        .fold(i64::from(npc.base_dexterity), |value, effect| {
            value.saturating_add(i64::from(effect.modifiers.dexterity))
        });
    u16::try_from(value.clamp(0, i64::from(cdda_protocol::MAX_CHARACTER_CREATION_STAT)))
        .expect("clamped NPC dexterity fits u16")
}

pub(super) fn npc_effective_speed(npc: &super::npc_dialogue::Npc) -> u32 {
    let bonus = npc.effects.iter().fold(0_i64, |value, effect| {
        value.saturating_add(i64::from(effect.modifiers.speed))
    });
    let base = i64::from(npc.speed);
    u32::try_from(base.saturating_add(bonus.max(-(base.saturating_mul(3) / 4)))).unwrap_or(u32::MAX)
}

#[derive(Clone, Debug)]
pub(super) struct ActorDamageUnit {
    pub damage_type_id: String,
    pub amount_milli: i32,
    pub armor_penetration_milli: i32,
    pub armor_multiplier_millionths: i32,
    pub damage_multiplier_millionths: i32,
    pub damage_multiplier_adjustment_millionths: i32,
    pub damage_multiplier_divisor: u32,
    pub constant_armor_multiplier_millionths: i32,
    pub constant_damage_multiplier_millionths: i32,
}

impl ActorDamageUnit {
    pub(super) fn ordinary(damage_type_id: &str, amount: u16) -> Self {
        Self {
            damage_type_id: damage_type_id.to_owned(),
            amount_milli: i32::from(amount) * 1_000,
            armor_penetration_milli: 0,
            armor_multiplier_millionths: 1_000_000,
            damage_multiplier_millionths: 1_000_000,
            damage_multiplier_adjustment_millionths: 1_000_000,
            damage_multiplier_divisor: 1,
            constant_armor_multiplier_millionths: 1_000_000,
            constant_damage_multiplier_millionths: 1_000_000,
        }
    }
}

fn multiply_damage_multiplier(value: i128, factor: i32) -> Result<i128, SimError> {
    value
        .checked_mul(i128::from(factor))
        .and_then(|value| value.checked_div(DAMAGE_MULTIPLIER_SCALE))
        .ok_or(SimError::NumericOverflow)
}

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
    speed: u32,
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
    pub(super) fn npc_melee_action_cost(&self, source: NpcId) -> Result<i64, SimError> {
        let npc = self.npcs.get(&source).ok_or(SimError::InvalidNpcDialogue)?;
        let attack_time_moves = match npc.wielded {
            None => 65,
            Some(item_id) => {
                if self.npc_bash_strength(source)?.is_none() {
                    return Ok(i64::from(super::ACTOR_ACTION_THRESHOLD));
                }
                let weapon = npc.inventory.get(&item_id).ok_or(SimError::UnknownItem)?;
                self.smash_item_types
                    .get(&weapon.type_id)
                    .ok_or(SimError::InvalidItem)?
                    .attack_time_moves
            }
        };
        i64::from(pinned_melee_attack_speed_moves(
            attack_time_moves,
            npc_effective_dexterity(npc),
            npc_skill_level(npc, "melee"),
        )?)
        .checked_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
        .ok_or(SimError::NumericOverflow)
    }

    pub(super) fn npc_melee_damage(&self, source: NpcId) -> Result<u16, SimError> {
        let npc = self.npcs.get(&source).ok_or(SimError::InvalidNpcDialogue)?;
        let Some(item) = npc.wielded.and_then(|item_id| npc.inventory.get(&item_id)) else {
            return Ok(super::UNARMED_DAMAGE);
        };
        let milli = item
            .melee_damage_milli
            .values()
            .try_fold(0_i64, |total, damage| {
                total
                    .checked_add(i64::from(*damage))
                    .ok_or(SimError::NumericOverflow)
            })?;
        let rounded = milli.checked_add(500).ok_or(SimError::NumericOverflow)? / 1_000;
        u16::try_from(rounded.max(1)).map_err(|_| SimError::NumericOverflow)
    }

    fn npc_bash_strength(&self, source: NpcId) -> Result<Option<u64>, SimError> {
        let npc = self.npcs.get(&source).ok_or(SimError::InvalidNpcDialogue)?;
        let Some(weapon) = npc.wielded.and_then(|item_id| npc.inventory.get(&item_id)) else {
            return Ok(None);
        };
        let Some(profile) = self.smash_item_types.get(&weapon.type_id) else {
            return Ok(None);
        };
        if weapon.damage > 1
            || weapon.ranged_weapon.is_some()
            || weapon.magazine_capacity != 0
            || !weapon.integral_magazines.is_empty()
            || !weapon.magazine_wells.is_empty()
            || weapon.residual_energy_millijoules != 0
            || weapon.powered_tool.is_some()
            || !weapon.ammunition_type.is_empty()
            || weapon
                .melee_damage_milli
                .iter()
                .any(|(damage_type, damage)| damage_type != "bash" && *damage > 0)
        {
            return Ok(None);
        }
        let expected_bash_milli = i32::from(profile.bash_damage)
            .checked_mul(1_000)
            .ok_or(SimError::NumericOverflow)?;
        if weapon.melee_damage_milli.get("bash").copied() != Some(expected_bash_milli) {
            return Ok(None);
        }
        Ok(Some(u64::from(profile.bash_damage)))
    }

    fn npc_melee_accuracy(&self, source: NpcId) -> Result<Option<(i64, u8)>, SimError> {
        let npc = self.npcs.get(&source).ok_or(SimError::InvalidNpcDialogue)?;
        let melee_skill = npc_skill_level(npc, "melee");
        match npc.wielded {
            None => Ok(Some((
                pinned_unarmed_melee_accuracy_quarters(npc_effective_dexterity(npc), melee_skill),
                4,
            ))),
            Some(item_id) => {
                if self.npc_bash_strength(source)?.is_none() {
                    return Ok(None);
                }
                let weapon = npc.inventory.get(&item_id).ok_or(SimError::UnknownItem)?;
                let profile = self
                    .smash_item_types
                    .get(&weapon.type_id)
                    .ok_or(SimError::InvalidItem)?;
                Ok(Some((
                    pinned_bash_weapon_melee_accuracy_twelfths(
                        npc_effective_dexterity(npc),
                        npc_skill_level(npc, "bashing"),
                        melee_skill,
                        profile.melee_to_hit,
                        profile.bash_damage,
                    ),
                    12,
                )))
            }
        }
    }

    pub(super) fn charge_npc_melee_stamina(&mut self, source: NpcId) -> Result<(), SimError> {
        self.npcs
            .contains_key(&source)
            .then_some(())
            .ok_or(SimError::InvalidNpcDialogue)
    }

    pub(super) fn npc_actor_hit_spread(
        &self,
        source: NpcId,
        target: ActorId,
        turn_sequence: u64,
    ) -> Result<Option<(i64, bool)>, SimError> {
        let Some((accuracy_numerator, accuracy_denominator)) = self.npc_melee_accuracy(source)?
        else {
            return Ok(None);
        };
        let mut rng = self.named_rng(
            b"npc-melee-hit",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let hit = pinned_melee_hit_roll(accuracy_numerator, accuracy_denominator, &mut rng)?;
        let (dodge, dodge_attempted) = self.actor_dodge_roll(target)?;
        Ok(Some((
            hit.checked_sub(dodge).ok_or(SimError::NumericOverflow)?,
            dodge_attempted,
        )))
    }

    pub(super) fn damage_npc(
        &mut self,
        target: NpcId,
        damage_type: &str,
        damage: u16,
        rng: &mut impl Rng,
    ) -> Result<anatomy::ActorDamageOutcome, SimError> {
        let selected = anatomy::select_body_part_index(&self.actor_anatomy, rng)?;
        self.damage_npc_at(target, selected, damage_type, damage, rng)
    }

    pub(super) fn damage_npc_for_hit(
        &mut self,
        target: NpcId,
        damage_type: &str,
        damage: u16,
        hit_spread: i32,
        rng: &mut impl Rng,
    ) -> Result<anatomy::ActorDamageOutcome, SimError> {
        let selected =
            anatomy::select_body_part_index_for_hit(&self.actor_anatomy, hit_spread, rng)?;
        self.damage_npc_at(target, selected, damage_type, damage, rng)
    }

    fn damage_npc_at(
        &mut self,
        target: NpcId,
        selected: usize,
        damage_type: &str,
        damage: u16,
        rng: &mut impl Rng,
    ) -> Result<anatomy::ActorDamageOutcome, SimError> {
        let body_part_id = self
            .actor_anatomy
            .parts
            .get(selected)
            .ok_or(SimError::InvalidActorAnatomy)?
            .body_part_id
            .as_str();
        let npc = self.npcs.get(&target).ok_or(SimError::InvalidNpcDialogue)?;
        let mut remaining_milli = i128::from(damage) * 1_000;
        let coverage_ticket = rng.next_u32() % 100;
        for item_id in npc.worn.iter().rev() {
            let item = npc.inventory.get(item_id).ok_or(SimError::InvalidArmor)?;
            let armor = self
                .wearable_armor_types
                .get(&item.type_id)
                .filter(|armor| runtime_armor_is_supported(armor))
                .ok_or(SimError::InvalidArmor)?;
            let Some(portion) = armor.portions.iter().find(|portion| {
                portion
                    .covers
                    .binary_search_by(|covered| covered.as_str().cmp(body_part_id))
                    .is_ok()
            }) else {
                continue;
            };
            if coverage_ticket >= u32::from(portion.coverage_percent) {
                continue;
            }
            for material in &portion.materials {
                if rng.next_u32() % 100 >= u32::from(material.covered_by_material_percent) {
                    continue;
                }
                let protection_milli = i128::from(
                    material
                        .protection_milli
                        .get(damage_type)
                        .copied()
                        .unwrap_or_default(),
                );
                remaining_milli = (remaining_milli - protection_milli).max(0);
            }
        }
        let dealt =
            u16::try_from(remaining_milli / 1_000).map_err(|_| SimError::NumericOverflow)?;
        let npc = self
            .npcs
            .get_mut(&target)
            .ok_or(SimError::InvalidNpcDialogue)?;
        let outcome = anatomy::apply_damage_to_part(
            &self.actor_anatomy,
            &mut npc.body_parts,
            selected,
            dealt,
        )?;
        let applications = anatomy::on_hit_effects(
            &self.actor_anatomy,
            &npc.body_parts,
            selected,
            damage_type,
            dealt,
            rng,
        )?;
        apply_actor_effect_applications(&mut npc.effects, applications, self.tick)?;
        npc.hp = outcome.remaining_hp;
        Ok(outcome)
    }

    pub(super) fn actor_npc_hit_spread(
        &self,
        source: ActorId,
        target: NpcId,
        rng_sequence: u64,
    ) -> Result<Option<(i64, bool)>, SimError> {
        let Some((accuracy_numerator, accuracy_denominator)) = self.actor_melee_accuracy(source)?
        else {
            return Ok(None);
        };
        let npc = self.npcs.get(&target).ok_or(SimError::InvalidNpcDialogue)?;
        let disabling = npc.effects.iter().any(|effect| {
            effect.expires_at_tick > self.tick
                && matches!(
                    effect.effect_id.as_str(),
                    "winded" | "downed" | "fearparalyze" | "narcosis"
                )
        });
        let dodge_attempted = npc.hp > 0
            && npc.dodge_attempts_remaining > 0
            && !disabling
            && can_dodge_at_stamina(NPC_STAMINA, NPC_STAMINA);
        let dodge = if dodge_attempted {
            dodge_roll(
                npc_effective_dexterity(npc),
                npc_skill_level(npc, "dodge"),
                npc_effective_speed(npc),
                NPC_STAMINA,
                NPC_STAMINA,
            )
        } else {
            0
        };
        let mut rng = self.named_session_rng(
            b"actor-melee-hit",
            &[source.as_u128(), target.as_u128()],
            rng_sequence,
        );
        let hit = pinned_melee_hit_roll(accuracy_numerator, accuracy_denominator, &mut rng)?;
        Ok(Some((
            hit.checked_sub(dodge).ok_or(SimError::NumericOverflow)?,
            dodge_attempted,
        )))
    }

    pub(super) fn consume_npc_dodge_attempt(&mut self, target: NpcId) -> Result<(), SimError> {
        let npc = self
            .npcs
            .get_mut(&target)
            .ok_or(SimError::InvalidNpcDialogue)?;
        npc.dodge_attempts_remaining = npc.dodge_attempts_remaining.saturating_sub(1);
        Ok(())
    }

    pub(super) fn damage_actor(
        &mut self,
        target: ActorId,
        damage_type: &str,
        damage: u16,
        rng: &mut impl Rng,
    ) -> Result<(anatomy::ActorDamageOutcome, bool), SimError> {
        self.damage_actor_components(
            target,
            &[ActorDamageUnit::ordinary(damage_type, damage)],
            rng,
        )
        .map(|(outcome, was_sleeping, _)| (outcome, was_sleeping))
    }

    pub(super) fn damage_actor_for_hit(
        &mut self,
        target: ActorId,
        damage_type: &str,
        damage: u16,
        hit_spread: i32,
        rng: &mut impl Rng,
    ) -> Result<(anatomy::ActorDamageOutcome, bool), SimError> {
        self.damage_actor_components_for_hit(
            target,
            &[ActorDamageUnit::ordinary(damage_type, damage)],
            hit_spread,
            rng,
        )
        .map(|(outcome, was_sleeping, _)| (outcome, was_sleeping))
    }

    pub(super) fn damage_actor_components(
        &mut self,
        target: ActorId,
        units: &[ActorDamageUnit],
        rng: &mut impl Rng,
    ) -> Result<(anatomy::ActorDamageOutcome, bool, u16), SimError> {
        let selected = anatomy::select_body_part_index(&self.actor_anatomy, rng)?;
        self.damage_actor_components_at(target, selected, units, rng)
    }

    pub(super) fn damage_actor_components_for_hit(
        &mut self,
        target: ActorId,
        units: &[ActorDamageUnit],
        hit_spread: i32,
        rng: &mut impl Rng,
    ) -> Result<(anatomy::ActorDamageOutcome, bool, u16), SimError> {
        let selected =
            anatomy::select_body_part_index_for_hit(&self.actor_anatomy, hit_spread, rng)?;
        self.damage_actor_components_at(target, selected, units, rng)
    }

    pub(super) fn damage_actor_part(
        &mut self,
        target: ActorId,
        body_part_id: &str,
        damage_type: &str,
        damage: u16,
        rng: &mut impl Rng,
    ) -> Result<(anatomy::ActorDamageOutcome, bool), SimError> {
        let selected = self
            .actor_anatomy
            .parts
            .iter()
            .position(|part| part.body_part_id == body_part_id)
            .ok_or(SimError::InvalidActorAnatomy)?;
        self.damage_actor_components_at(
            target,
            selected,
            &[ActorDamageUnit::ordinary(damage_type, damage)],
            rng,
        )
        .map(|(outcome, was_sleeping, _)| (outcome, was_sleeping))
    }

    pub(super) fn damage_actor_components_at(
        &mut self,
        target: ActorId,
        selected: usize,
        units: &[ActorDamageUnit],
        rng: &mut impl Rng,
    ) -> Result<(anatomy::ActorDamageOutcome, bool, u16), SimError> {
        let body_part_id = self
            .actor_anatomy
            .parts
            .get(selected)
            .ok_or(SimError::InvalidActorAnatomy)?
            .body_part_id
            .as_str();
        let actor = self.actors.get(&target).ok_or(SimError::UnknownActor)?;
        let mut dealt_components = Vec::with_capacity(units.len());
        let mut total_damage = 0_u32;
        let mut cut_or_stab_damage = 0_u32;
        for unit in units {
            if unit.amount_milli < 0 {
                // Pinned character absorption clamps negative components and
                // skips armor processing for them entirely.
                dealt_components.push((unit.damage_type_id.as_str(), 0));
                continue;
            }
            let mut remaining_milli = i128::from(unit.amount_milli).max(0);
            let mut penetration_milli = i128::from(unit.armor_penetration_milli);
            let coverage_ticket = rng.next_u32() % 100;
            for item_id in actor.worn.iter().rev() {
                let item = actor.inventory.get(item_id).ok_or(SimError::InvalidArmor)?;
                let armor = self
                    .wearable_armor_types
                    .get(&item.type_id)
                    .filter(|armor| runtime_armor_is_supported(armor))
                    .ok_or(SimError::InvalidArmor)?;
                let Some(portion) = armor.portions.iter().find(|portion| {
                    portion
                        .covers
                        .binary_search_by(|covered| covered.as_str().cmp(body_part_id))
                        .is_ok()
                }) else {
                    continue;
                };
                if coverage_ticket >= u32::from(portion.coverage_percent) {
                    continue;
                }
                for material in &portion.materials {
                    if rng.next_u32() % 100 >= u32::from(material.covered_by_material_percent) {
                        continue;
                    }
                    let protection_milli = i128::from(
                        material
                            .protection_milli
                            .get(&unit.damage_type_id)
                            .copied()
                            .unwrap_or_default(),
                    );
                    let effective = multiply_damage_multiplier(
                        multiply_damage_multiplier(
                            (protection_milli - penetration_milli).max(0),
                            unit.armor_multiplier_millionths,
                        )?,
                        unit.constant_armor_multiplier_millionths,
                    )?;
                    remaining_milli = (remaining_milli - effective).max(0);
                    penetration_milli = (penetration_milli - protection_milli).max(0);
                }
            }
            let multiplier_denominator = DAMAGE_MULTIPLIER_SCALE
                .checked_mul(DAMAGE_MULTIPLIER_SCALE)
                .and_then(|value| value.checked_mul(DAMAGE_MULTIPLIER_SCALE))
                .and_then(|value| value.checked_mul(i128::from(unit.damage_multiplier_divisor)))
                .ok_or(SimError::NumericOverflow)?;
            if unit.damage_multiplier_divisor == 0 {
                return Err(SimError::NumericOverflow);
            }
            let adjusted_milli = remaining_milli
                .checked_mul(i128::from(unit.damage_multiplier_millionths))
                .and_then(|value| {
                    value.checked_mul(i128::from(unit.damage_multiplier_adjustment_millionths))
                })
                .and_then(|value| {
                    value.checked_mul(i128::from(unit.constant_damage_multiplier_millionths))
                })
                .and_then(|value| value.checked_div(multiplier_denominator))
                .ok_or(SimError::NumericOverflow)?;
            // Pinned deal_damage_handle_type truncates each adjusted component
            // to an integer before summing the atomic hit.
            let dealt = u16::try_from((adjusted_milli / 1_000).max(0))
                .map_err(|_| SimError::NumericOverflow)?;
            total_damage = total_damage
                .checked_add(u32::from(dealt))
                .ok_or(SimError::NumericOverflow)?;
            if matches!(unit.damage_type_id.as_str(), "cut" | "stab") {
                cut_or_stab_damage = cut_or_stab_damage
                    .checked_add(u32::from(dealt))
                    .ok_or(SimError::NumericOverflow)?;
            }
            dealt_components.push((unit.damage_type_id.as_str(), dealt));
        }
        let total_damage = u16::try_from(total_damage).map_err(|_| SimError::NumericOverflow)?;
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        let outcome = anatomy::apply_damage_to_part(
            &self.actor_anatomy,
            &mut actor.body_parts,
            selected,
            total_damage,
        )?;
        for (damage_type, dealt) in dealt_components {
            let applications = anatomy::on_hit_effects(
                &self.actor_anatomy,
                &actor.body_parts,
                selected,
                damage_type,
                dealt,
                rng,
            )?;
            apply_actor_effect_applications(&mut actor.effects, applications, self.tick)?;
        }
        actor.hp = outcome.remaining_hp;
        let was_sleeping = actor.sleeping;
        if actor.hp <= 0 {
            actor.sleeping = false;
            actor.sleep_intervals = 0;
            actor.dodge_attempts_remaining = 0;
            actor.held_movement = None;
            actor.queued_actions.clear();
        }
        Ok((
            outcome,
            was_sleeping,
            u16::try_from(cut_or_stab_damage).map_err(|_| SimError::NumericOverflow)?,
        ))
    }

    pub(super) fn actor_melee_accuracy(
        &self,
        source: ActorId,
    ) -> Result<Option<(i64, u8)>, SimError> {
        let actor = self.actors.get(&source).ok_or(SimError::UnknownActor)?;
        let melee_skill = actor_skill_level(actor, "melee", false);
        match actor.wielded {
            None => Ok(Some((
                pinned_unarmed_melee_accuracy_quarters(
                    actor_effective_dexterity(actor),
                    melee_skill,
                ),
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
                        actor_effective_dexterity(actor),
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
                actor_effective_dexterity(actor),
                actor_skill_level(actor, "dodge", false),
                actor_effective_speed(actor),
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

    pub(super) fn creature_creature_attack_roll(
        &self,
        source: CreatureId,
        target: CreatureId,
        turn_sequence: u64,
    ) -> Result<Option<(i64, bool)>, SimError> {
        let attacker = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?;
        let defender = self
            .creatures
            .get(&target)
            .ok_or(SimError::UnknownCreature)?;
        if attacker.melee_dice == 0 {
            return Ok(None);
        }
        let mut rng = self.named_rng(
            b"creature-creature-melee-hit",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let hit = pinned_melee_hit_roll(i64::from(attacker.melee_skill), 1, &mut rng)?;
        let dodge = i64::from(defender.dodge)
            .checked_mul(5)
            .and_then(|value| value.checked_add(super::creature_size_melee_penalty(defender.size)))
            .and_then(|value| {
                value.checked_sub(if defender.immobile {
                    super::IMMOBILE_MELEE_HIT_BONUS
                } else {
                    0
                })
            })
            .ok_or(SimError::NumericOverflow)?;
        let spread = hit.checked_sub(dodge).ok_or(SimError::NumericOverflow)?;
        let stumbled = spread < 0 && attacker.clumsy_attacks && rng.next_u32().is_multiple_of(4);
        Ok(Some((spread, stumbled)))
    }

    pub(super) fn creature_actor_special_attack_roll(
        &self,
        source: CreatureId,
        target: ActorId,
        turn_sequence: u64,
        accuracy: i32,
    ) -> Result<(i64, bool), SimError> {
        let mut rng = self.named_rng(
            b"creature-special-melee-hit",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let hit_roll = pinned_melee_hit_roll(i64::from(accuracy), 1, &mut rng)?;
        let (dodge_roll, dodge_attempted) = self.actor_dodge_roll(target)?;
        Ok((
            hit_roll
                .checked_sub(dodge_roll)
                .ok_or(SimError::NumericOverflow)?,
            dodge_attempted,
        ))
    }

    pub(super) fn creature_creature_special_attack_roll(
        &self,
        source: CreatureId,
        target: CreatureId,
        turn_sequence: u64,
        accuracy: i32,
    ) -> Result<i64, SimError> {
        let defender = self
            .creatures
            .get(&target)
            .ok_or(SimError::UnknownCreature)?;
        let mut rng = self.named_rng(
            b"creature-special-melee-hit",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let hit = pinned_melee_hit_roll(i64::from(accuracy), 1, &mut rng)?;
        let dodge = i64::from(defender.dodge)
            .checked_mul(5)
            .and_then(|value| value.checked_add(super::creature_size_melee_penalty(defender.size)))
            .and_then(|value| {
                value.checked_sub(if defender.immobile {
                    super::IMMOBILE_MELEE_HIT_BONUS
                } else {
                    0
                })
            })
            .ok_or(SimError::NumericOverflow)?;
        hit.checked_sub(dodge).ok_or(SimError::NumericOverflow)
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
