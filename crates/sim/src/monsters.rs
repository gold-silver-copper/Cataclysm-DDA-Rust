//! Authoritative monster-specific combat behavior.

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorId, BookStudyInterruptionReason, ConstructionInterruptionReason,
    CreatureId, CreatureSnapshot, CreatureSpecialAttackStateV1, DisassemblyInterruptionReason,
    SimTick, WorldEvent, WorldEventKind, WorldPosition, WorldgenCatalogV1,
    WorldgenMonsterAttackEffectV1, WorldgenMonsterExtraSpellEffectV1,
    WorldgenMonsterProjectileFieldEffectV1, WorldgenMonsterPrototypeV1,
    WorldgenMonsterSpecialAttackKindV1, WorldgenMonsterSpecialAttackV1,
    WorldgenMonsterSpellShapeV1,
};
use rand_core::Rng;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::{
    SimError, UNARMED_DAMAGE, WorldState, combat::ActorDamageUnit, horizontal_distance_squared,
    horizontally_adjacent, mapgen::creature_spawn_from_worldgen, ranged_distance,
    ranged_sound_description,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CreatureAttitude {
    Flee,
    Ignore,
    Follow,
    Attack,
}

#[derive(Clone, Copy)]
enum CreatureSpellProfile<'a> {
    Primary(&'a WorldgenMonsterSpecialAttackV1),
    Extra(&'a WorldgenMonsterExtraSpellEffectV1),
}

impl<'a> CreatureSpellProfile<'a> {
    fn shape(self) -> WorldgenMonsterSpellShapeV1 {
        match self {
            Self::Primary(profile) => profile.spell_shape,
            Self::Extra(profile) => profile.shape,
        }
    }

    fn target_self(self) -> bool {
        match self {
            Self::Primary(profile) => profile.spell_target_self,
            Self::Extra(profile) => profile.target_self,
        }
    }

    fn no_projectile(self) -> bool {
        match self {
            Self::Primary(profile) => profile.spell_no_projectile,
            Self::Extra(profile) => profile.no_projectile,
        }
    }

    fn ignore_walls(self) -> bool {
        match self {
            Self::Primary(profile) => profile.spell_ignore_walls,
            Self::Extra(profile) => profile.ignore_walls,
        }
    }

    fn range(self) -> u32 {
        match self {
            Self::Primary(profile) => profile.range,
            Self::Extra(profile) => profile.range,
        }
    }

    fn aoe(self) -> u16 {
        match self {
            Self::Primary(profile) => profile.spell_aoe,
            Self::Extra(profile) => profile.aoe,
        }
    }

    fn damage_bounds(self) -> (i32, i32) {
        match self {
            Self::Primary(profile) => (
                profile.minimum_damage_multiplier_millionths,
                profile.maximum_damage_multiplier_millionths,
            ),
            Self::Extra(profile) => (
                profile.minimum_damage_multiplier_millionths,
                profile.maximum_damage_multiplier_millionths,
            ),
        }
    }

    fn damage(self) -> &'a [cdda_protocol::WorldgenMonsterMeleeDamageUnitV1] {
        match self {
            Self::Primary(profile) => &profile.damage,
            Self::Extra(profile) => &profile.damage,
        }
    }

    fn effects(self) -> &'a [WorldgenMonsterAttackEffectV1] {
        match self {
            Self::Primary(profile) => &profile.effects,
            Self::Extra(profile) => &profile.effects,
        }
    }

    fn eoc_ids(self) -> &'a [String] {
        match self {
            Self::Primary(profile) => &profile.eoc_ids,
            Self::Extra(profile) => &profile.eoc_ids,
        }
    }

    fn summoned_monster_type_id(self) -> &'a str {
        match self {
            Self::Primary(profile) => &profile.spell_summoned_monster_type_id,
            Self::Extra(profile) => &profile.summoned_monster_type_id,
        }
    }

    fn summon_bounds(self) -> (u16, u16, bool) {
        match self {
            Self::Primary(profile) => (
                profile.spell_minimum_summons,
                profile.spell_maximum_summons,
                profile.spell_random_summons,
            ),
            Self::Extra(profile) => (
                profile.minimum_summons,
                profile.maximum_summons,
                profile.random_summons,
            ),
        }
    }

    fn field(self) -> (&'a str, u32, u8, u32, u32) {
        match self {
            Self::Primary(profile) => (
                &profile.spell_field_type_id,
                profile.spell_field_chance,
                profile.spell_field_intensity,
                profile.spell_field_intensity_variance_millionths,
                profile.spell_field_duration_turns,
            ),
            Self::Extra(profile) => (
                &profile.field_type_id,
                profile.field_chance,
                profile.field_intensity,
                profile.field_intensity_variance_millionths,
                profile.field_duration_turns,
            ),
        }
    }

    fn targets(self) -> (bool, bool, bool) {
        match self {
            Self::Primary(profile) => (
                profile.spell_targets_hostile,
                profile.spell_targets_ground,
                profile.spell_targets_self,
            ),
            Self::Extra(profile) => (
                profile.targets_hostile,
                profile.targets_ground,
                profile.targets_self,
            ),
        }
    }
}

pub(super) fn creature_attitude(
    morale: i32,
    aggression: i16,
    hp: i32,
    max_hp: i32,
) -> CreatureAttitude {
    let aggression = i32::from(aggression);
    if morale < 0 {
        if i64::from(morale) + i64::from(aggression) > 0 && hp > max_hp / 3 {
            CreatureAttitude::Follow
        } else {
            CreatureAttitude::Flee
        }
    } else if aggression <= 0 {
        if i64::from(hp) * 5 <= i64::from(max_hp) * 3 {
            CreatureAttitude::Flee
        } else {
            CreatureAttitude::Ignore
        }
    } else if aggression < 10 {
        CreatureAttitude::Follow
    } else {
        CreatureAttitude::Attack
    }
}

fn insert_whole_creature_effect(
    effects: &mut Vec<ActorEffectSnapshotV1>,
    effect_id: &str,
    expires_at_tick: SimTick,
) {
    if let Some(effect) = effects
        .iter_mut()
        .find(|effect| effect.effect_id == effect_id && effect.body_part_id.is_none())
    {
        effect.intensity = effect.intensity.max(1);
        effect.expires_at_tick = effect.expires_at_tick.max(expires_at_tick);
    } else if effects.len() < 1_024 {
        effects.push(ActorEffectSnapshotV1 {
            effect_id: effect_id.to_owned(),
            body_part_id: None,
            intensity: 1,
            expires_at_tick,
            modifiers: Default::default(),
        });
        effects.sort_by(|left, right| {
            (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
        });
    }
}

fn add_or_extend_whole_creature_effect(
    effects: &mut Vec<ActorEffectSnapshotV1>,
    effect_id: &str,
    duration_ticks: i64,
    current_tick: SimTick,
) {
    if let Some(index) = effects
        .iter()
        .position(|effect| effect.effect_id == effect_id && effect.body_part_id.is_none())
    {
        let expiration = i128::from(effects[index].expires_at_tick.0) + i128::from(duration_ticks);
        if expiration <= i128::from(current_tick.0) {
            effects.remove(index);
        } else {
            effects[index].expires_at_tick = SimTick(
                u64::try_from(expiration)
                    .unwrap_or(u64::MAX)
                    .min(u64::MAX - 1),
            );
        }
    } else if duration_ticks > 0 {
        let expiration = current_tick.0.saturating_add(duration_ticks as u64);
        insert_whole_creature_effect(effects, effect_id, SimTick(expiration.min(u64::MAX - 1)));
    }
}

fn projectile_line(origin: WorldPosition, endpoint: WorldPosition) -> Vec<WorldPosition> {
    if origin.z != endpoint.z || origin == endpoint {
        return Vec::new();
    }
    let (mut x, mut y) = (origin.x, origin.y);
    let delta_x = i64::from(endpoint.x) - i64::from(x);
    let delta_y = i64::from(endpoint.y) - i64::from(y);
    let step_x = delta_x.signum() as i32;
    let step_y = delta_y.signum() as i32;
    let doubled_x = delta_x.abs() * 2;
    let doubled_y = delta_y.abs() * 2;
    let mut tie = 0_i64;
    let mut positions = Vec::new();
    if doubled_x == doubled_y {
        while x != endpoint.x {
            y += step_y;
            x += step_x;
            positions.push(WorldPosition { x, y, z: origin.z });
        }
    } else if doubled_x > doubled_y {
        while x != endpoint.x {
            if tie > 0 {
                y += step_y;
                tie -= doubled_x;
            }
            x += step_x;
            tie += doubled_y;
            positions.push(WorldPosition { x, y, z: origin.z });
        }
    } else {
        while y != endpoint.y {
            if tie > 0 {
                x += step_x;
                tie -= doubled_y;
            }
            y += step_y;
            tie += doubled_x;
            positions.push(WorldPosition { x, y, z: origin.z });
        }
    }
    positions
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RelativeSpellPoint {
    x: i32,
    y: i32,
}

impl RelativeSpellPoint {
    const ZERO: Self = Self { x: 0, y: 0 };

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(other.x)?,
            y: self.y.checked_add(other.y)?,
        })
    }

    fn checked_sub(self, other: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_sub(other.x)?,
            y: self.y.checked_sub(other.y)?,
        })
    }
}

fn relative_spell_line(endpoint: RelativeSpellPoint) -> Vec<RelativeSpellPoint> {
    projectile_line(
        WorldPosition { x: 0, y: 0, z: 0 },
        WorldPosition {
            x: endpoint.x,
            y: endpoint.y,
            z: 0,
        },
    )
    .into_iter()
    .map(|position| RelativeSpellPoint {
        x: position.x,
        y: position.y,
    })
    .collect()
}

fn spell_line_side(a: RelativeSpellPoint, b: RelativeSpellPoint, c: RelativeSpellPoint) -> i8 {
    let cross = (i64::from(b.x) - i64::from(a.x)) * (i64::from(c.y) - i64::from(a.y))
        - (i64::from(b.y) - i64::from(a.y)) * (i64::from(c.x) - i64::from(a.x));
    cross.signum() as i8
}

fn spell_line_between_or_on(
    a0: RelativeSpellPoint,
    a1: RelativeSpellPoint,
    direction: RelativeSpellPoint,
    point: RelativeSpellPoint,
) -> bool {
    let Some(a0_end) = a0.checked_add(direction) else {
        return false;
    };
    let Some(a1_end) = a1.checked_add(direction) else {
        return false;
    };
    spell_line_side(a0, a0_end, point) != 1 && spell_line_side(a1, a1_end, point) != -1
}

#[derive(Clone)]
struct SpellLineIterator<'a> {
    delta_line: &'a [RelativeSpellPoint],
    current_origin: RelativeSpellPoint,
    delta: RelativeSpellPoint,
    index: usize,
}

impl SpellLineIterator<'_> {
    fn get(&self) -> Option<RelativeSpellPoint> {
        self.current_origin
            .checked_add(*self.delta_line.get(self.index)?)
    }

    fn next(&mut self) -> Option<()> {
        self.index = (self.index + 1) % self.delta_line.len();
        if self.index == 0 {
            self.current_origin = self.current_origin.checked_add(self.delta)?;
        }
        Some(())
    }

    fn previous(&mut self) -> Option<()> {
        if self.index == 0 {
            self.current_origin = self.current_origin.checked_sub(self.delta)?;
        }
        self.index = (self.index + self.delta_line.len() - 1) % self.delta_line.len();
        Some(())
    }

    fn reset(&mut self, origin: RelativeSpellPoint) {
        self.current_origin = origin;
        self.index = 0;
    }
}

fn move_spell_line_to_boundary(
    line: &mut SpellLineIterator<'_>,
    perpendicular: RelativeSpellPoint,
    while_on_clockwise_side: bool,
    forward: bool,
) -> Result<(), SimError> {
    for _ in 0..=4_096 {
        let current = line.get().ok_or(SimError::NumericOverflow)?;
        let on_clockwise_side =
            spell_line_side(RelativeSpellPoint::ZERO, perpendicular, current) == 1;
        if on_clockwise_side != while_on_clockwise_side {
            return Ok(());
        }
        if forward {
            line.next().ok_or(SimError::NumericOverflow)?;
        } else {
            line.previous().ok_or(SimError::NumericOverflow)?;
        }
    }
    Err(SimError::InvalidCreature)
}

pub(super) fn special_state_matches_catalog(
    catalog: Option<&WorldgenCatalogV1>,
    snapshot: &CreatureSnapshot,
    snapshot_tick: SimTick,
) -> bool {
    let Some(catalog) = catalog else {
        return snapshot.special_attacks.is_empty()
            && snapshot.ammunition.is_empty()
            && snapshot.corpse.is_none();
    };
    let indices = catalog
        .monster_prototypes
        .iter()
        .enumerate()
        .map(|(index, prototype)| (prototype.base.monster_type_id.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let Some(&prototype_index) = indices.get(snapshot.type_id.as_str()) else {
        return false;
    };
    let prototype = &catalog.monster_prototypes[prototype_index];
    if !prototype.runtime_spawnable {
        return false;
    }
    let attacks_match = snapshot.special_attacks.len() == prototype.special_attacks.len()
        && snapshot
            .special_attacks
            .iter()
            .zip(&prototype.special_attacks)
            .all(|(state, attack)| {
                state.attack_id == attack.attack_id
                    && state.enabled
                    && state.cooldown_turns <= attack.cooldown_turns
            });
    let base = &prototype.base;
    let corpse_matches = if prototype.leaves_corpse {
        snapshot.corpse.as_ref() == Some(base)
    } else {
        snapshot.corpse.is_none()
    };
    #[derive(Clone, Copy)]
    struct ReversePolymorph {
        source: usize,
        keep_speed: bool,
        keep_hp: bool,
        keep_aggression: bool,
    }
    let mut reverse = vec![Vec::new(); catalog.monster_prototypes.len()];
    for (source, candidate) in catalog.monster_prototypes.iter().enumerate() {
        if !candidate.runtime_spawnable {
            continue;
        }
        for attack in &candidate.special_attacks {
            if attack.kind != WorldgenMonsterSpecialAttackKindV1::Polymorph {
                continue;
            }
            let Some(&target) = indices.get(attack.polymorph_monster_type_id.as_str()) else {
                continue;
            };
            reverse[target].push(ReversePolymorph {
                source,
                keep_speed: attack.polymorph_keep_speed,
                keep_hp: attack.polymorph_keep_hp,
                keep_aggression: attack.polymorph_keep_aggression,
            });
        }
    }
    let reachable = |keep: fn(ReversePolymorph) -> bool| {
        let mut seen = vec![false; reverse.len()];
        let mut queue = VecDeque::from([prototype_index]);
        seen[prototype_index] = true;
        while let Some(target) = queue.pop_front() {
            for edge in reverse[target].iter().copied().filter(|edge| keep(*edge)) {
                if !seen[edge.source] {
                    seen[edge.source] = true;
                    queue.push_back(edge.source);
                }
            }
        }
        seen
    };
    let ammunition_sources = reachable(|_| true);
    let ammunition_matches =
        catalog
            .monster_prototypes
            .iter()
            .enumerate()
            .any(|(index, candidate)| {
                candidate.runtime_spawnable
                    && ammunition_sources[index]
                    && snapshot.ammunition.len() == candidate.starting_ammunition.len()
                    && snapshot.ammunition.iter().all(|(item_id, amount)| {
                        candidate
                            .starting_ammunition
                            .get(item_id)
                            .is_some_and(|initial| amount <= initial)
                    })
            });
    let speed_sources = reachable(|edge| edge.keep_speed);
    let speed_matches = catalog
        .monster_prototypes
        .iter()
        .enumerate()
        .filter(|(index, candidate)| speed_sources[*index] && candidate.runtime_spawnable)
        .any(|(_index, candidate)| {
            snapshot.speed == candidate.base.speed
                || (candidate.base.revives
                    && (0..=cdda_protocol::MAX_ITEM_DAMAGE_LEVEL).any(|damage| {
                        let speed =
                            u32::from(candidate.base.speed) * 80 / 100 / (u32::from(damage) + 1);
                        speed > 0 && u16::try_from(speed) == Ok(snapshot.speed)
                    }))
        });
    let aggression_sources = reachable(|edge| edge.keep_aggression);
    let aggression_matches =
        catalog
            .monster_prototypes
            .iter()
            .enumerate()
            .any(|(index, candidate)| {
                aggression_sources[index]
                    && candidate.runtime_spawnable
                    && snapshot.aggression == candidate.base.aggression
            });
    let hp_matches = if snapshot.hp == 0 {
        reverse[prototype_index].iter().any(|edge| {
            !edge.keep_hp && catalog.monster_prototypes[edge.source].base.max_hp > base.max_hp
        })
    } else {
        let mut required = vec![None::<i64>; reverse.len()];
        required[prototype_index] = Some(i64::from(snapshot.hp));
        let mut queue = VecDeque::from([prototype_index]);
        let mut matches = false;
        let mut relaxations = 0_usize;
        let relaxation_limit = reverse.len().saturating_mul(4_096);
        while let Some(target) = queue.pop_front() {
            relaxations = relaxations.saturating_add(1);
            if relaxations > relaxation_limit {
                break;
            }
            let needed = required[target].unwrap_or(i64::MAX);
            let candidate = &catalog.monster_prototypes[target];
            if candidate.runtime_spawnable && needed <= i64::from(candidate.base.max_hp) {
                matches = true;
                break;
            }
            for edge in &reverse[target] {
                let source_max = i64::from(catalog.monster_prototypes[edge.source].base.max_hp);
                let target_max = i64::from(catalog.monster_prototypes[target].base.max_hp);
                let source_needed = if edge.keep_hp {
                    needed
                } else {
                    let Some(product) = needed.checked_mul(source_max) else {
                        continue;
                    };
                    product / target_max + i64::from(product % target_max != 0)
                };
                if source_needed <= 0 || source_needed > i64::from(i32::MAX) {
                    continue;
                }
                if required[edge.source].is_none_or(|existing| source_needed < existing) {
                    required[edge.source] = Some(source_needed);
                    queue.push_back(edge.source);
                }
            }
        }
        matches
    };
    let downed_matches = snapshot.downed_until_tick.is_none_or(|until| {
        let remaining = until.0.checked_sub(snapshot_tick.0);
        remaining.is_some_and(|remaining| {
            remaining > 0
                && remaining <= 5 * SimTick::HZ
                && if remaining > 2 * SimTick::HZ {
                    base.revives
                } else {
                    base.revives || snapshot.clumsy_attacks
                }
        })
    });
    attacks_match
        && ammunition_matches
        && hp_matches
        && speed_matches
        && aggression_matches
        && snapshot.morale == base.morale
        && downed_matches
        && snapshot.max_hp == base.max_hp
        && snapshot.attack_cost_moves == base.attack_cost_moves
        && snapshot.melee_skill == base.melee_skill
        && snapshot.dodge == base.dodge
        && snapshot.size == base.size
        && snapshot.melee_dice == base.melee_dice
        && snapshot.melee_dice_sides == base.melee_dice_sides
        && snapshot.can_see == base.can_see
        && snapshot.vision_day == base.vision_day
        && snapshot.vision_night == base.vision_night
        && snapshot.stumbles == base.stumbles
        && snapshot.bashes == base.bashes
        && snapshot.group_bash == base.group_bash
        && snapshot.hears == base.hears
        && snapshot.good_hearing == base.good_hearing
        && snapshot.clumsy_attacks == base.clumsy_attacks
        && snapshot.immobile == base.immobile
        && snapshot.pacifist == base.pacifist
        && snapshot.can_open_doors == base.can_open_doors
        && snapshot.path_settings == base.path_settings
        && snapshot.blood_field_type_id == base.blood_field_type_id
        && corpse_matches
}

impl WorldState {
    pub(super) fn initial_creature_ammunition(
        &self,
        type_id: &str,
    ) -> std::collections::BTreeMap<String, u32> {
        self.worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|prototype| {
                        prototype.base.monster_type_id.as_str().cmp(type_id)
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
            })
            .map(|prototype| prototype.starting_ammunition.clone())
            .unwrap_or_default()
    }

    pub(super) fn initial_creature_special_attacks(
        &self,
        type_id: &str,
        creature_id: CreatureId,
    ) -> Result<Vec<CreatureSpecialAttackStateV1>, SimError> {
        let profiles = self
            .worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|prototype| {
                        prototype.base.monster_type_id.as_str().cmp(type_id)
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
            })
            .map(|prototype| prototype.special_attacks.clone())
            .unwrap_or_default();
        let mut rng = self.named_rng(
            b"creature-special-initial-cooldown",
            &[creature_id.as_u128()],
            0,
        );
        profiles
            .into_iter()
            .map(|attack| {
                let cooldown_turns = if attack.cooldown_turns == 0 {
                    0
                } else {
                    u32::try_from(rng.next_u64() % (u64::from(attack.cooldown_turns) + 1))
                        .map_err(|_| SimError::NumericOverflow)?
                };
                Ok(CreatureSpecialAttackStateV1 {
                    attack_id: attack.attack_id,
                    cooldown_turns,
                    enabled: true,
                })
            })
            .collect()
    }

    pub(super) fn advance_creature_special_cooldowns(&mut self) {
        if !self.tick.0.is_multiple_of(SimTick::HZ) {
            return;
        }
        for creature in self.creatures.values_mut() {
            for attack in &mut creature.special_attacks {
                if attack.enabled {
                    attack.cooldown_turns = attack.cooldown_turns.saturating_sub(1);
                }
            }
        }
    }

    fn creature_prototype(
        &self,
        target: CreatureId,
    ) -> Result<Option<&WorldgenMonsterPrototypeV1>, SimError> {
        let creature = self
            .creatures
            .get(&target)
            .ok_or(SimError::UnknownCreature)?;
        Ok(self.worldgen.as_ref().and_then(|catalog| {
            catalog
                .monster_prototypes
                .binary_search_by(|prototype| {
                    prototype
                        .base
                        .monster_type_id
                        .as_str()
                        .cmp(&creature.type_id)
                })
                .ok()
                .and_then(|index| catalog.monster_prototypes.get(index))
        }))
    }

    fn creature_armor_milli(&self, target: CreatureId, damage_type: &str) -> Result<i32, SimError> {
        Ok(self
            .creature_prototype(target)?
            .and_then(|prototype| prototype.armor_milli.get(damage_type))
            .copied()
            .unwrap_or_default())
    }

    pub(super) fn creature_melee_damage_units(
        &self,
        source: CreatureId,
        rolled_bash_damage: u16,
    ) -> Result<Vec<ActorDamageUnit>, SimError> {
        let Some(prototype) = self.creature_prototype(source)? else {
            return Ok(vec![ActorDamageUnit::ordinary("bash", rolled_bash_damage)]);
        };
        let mut units = prototype
            .melee_damage
            .iter()
            .map(|unit| ActorDamageUnit {
                damage_type_id: unit.damage_type_id.clone(),
                amount_milli: unit.amount_milli,
                armor_penetration_milli: unit.armor_penetration_milli,
                armor_multiplier_millionths: unit.armor_multiplier_millionths,
                damage_multiplier_millionths: unit.damage_multiplier_millionths,
                damage_multiplier_adjustment_millionths: 1_000_000,
                damage_multiplier_divisor: 1,
                constant_armor_multiplier_millionths: unit.constant_armor_multiplier_millionths,
                constant_damage_multiplier_millionths: unit.constant_damage_multiplier_millionths,
            })
            .collect::<Vec<_>>();
        let rolled_milli = i32::from(rolled_bash_damage)
            .checked_mul(1_000)
            .ok_or(SimError::NumericOverflow)?;
        if let Some(bash) = units.iter_mut().find(|unit| unit.damage_type_id == "bash") {
            merge_rolled_bash_damage(
                bash,
                rolled_milli,
                prototype.melee_dice_armor_penetration_milli,
            )?;
        } else {
            units.push(ActorDamageUnit {
                damage_type_id: String::from("bash"),
                amount_milli: rolled_milli,
                armor_penetration_milli: prototype.melee_dice_armor_penetration_milli,
                armor_multiplier_millionths: 1_000_000,
                damage_multiplier_millionths: 1_000_000,
                damage_multiplier_adjustment_millionths: 1_000_000,
                damage_multiplier_divisor: 1,
                constant_armor_multiplier_millionths: 1_000_000,
                constant_damage_multiplier_millionths: 1_000_000,
            });
        }
        Ok(units)
    }

    pub(super) fn apply_creature_attack_effects(
        &mut self,
        source: CreatureId,
        target: ActorId,
        hit_body_part_id: &str,
        dealt_cut_or_stab_damage: bool,
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        let effects = self
            .creature_prototype(source)?
            .map(|prototype| prototype.attack_effects.clone())
            .unwrap_or_default();
        self.apply_monster_attack_effects(
            target,
            hit_body_part_id,
            dealt_cut_or_stab_damage,
            &effects,
            rng,
        )
    }

    fn apply_monster_attack_effects(
        &mut self,
        target: ActorId,
        hit_body_part_id: &str,
        dealt_cut_or_stab_damage: bool,
        effects: &[WorldgenMonsterAttackEffectV1],
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        for effect in effects {
            if effect.requires_cut_or_stab_damage && !dealt_cut_or_stab_damage {
                continue;
            }
            let chance_roll = rng.next_u32() % 1_000_000;
            if chance_roll >= effect.chance_millionths {
                continue;
            }
            self.apply_monster_attack_effect_after_chance(target, hit_body_part_id, effect, rng)?;
        }
        Ok(())
    }

    fn apply_monster_attack_effects_to_body_parts(
        &mut self,
        target: ActorId,
        hit_body_part_ids: &[String],
        dealt_cut_or_stab_damage: bool,
        effects: &[WorldgenMonsterAttackEffectV1],
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        for effect in effects {
            if effect.requires_cut_or_stab_damage && !dealt_cut_or_stab_damage {
                continue;
            }
            let chance_roll = rng.next_u32() % 1_000_000;
            if chance_roll >= effect.chance_millionths {
                continue;
            }
            for hit_body_part_id in hit_body_part_ids {
                self.apply_monster_attack_effect_after_chance(
                    target,
                    hit_body_part_id,
                    effect,
                    rng,
                )?;
            }
        }
        Ok(())
    }

    fn apply_monster_attack_effect_after_chance(
        &mut self,
        target: ActorId,
        hit_body_part_id: &str,
        effect: &WorldgenMonsterAttackEffectV1,
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        let duration_turns = roll_inclusive_u32(
            effect.duration_minimum_turns,
            effect.duration_maximum_turns,
            rng,
        )?;
        let intensity =
            roll_inclusive_u32(effect.intensity_minimum, effect.intensity_maximum, rng)?;
        if (duration_turns == 0 && !effect.permanent) || intensity == 0 {
            return Ok(());
        }
        let application_index = intensity
            .checked_sub(effect.intensity_minimum)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or(SimError::InvalidCreature)?;
        let application = effect
            .intensity_applications
            .get(application_index)
            .ok_or(SimError::InvalidCreature)?;
        let intensity = application.intensity;
        let body_part_id = if effect.affect_hit_body_part {
            Some(hit_body_part_id.to_owned())
        } else {
            effect.body_part_id.clone()
        };
        if body_part_id.as_ref().is_some_and(|body_part_id| {
            !self
                .actor_anatomy
                .parts
                .iter()
                .any(|part| part.body_part_id == *body_part_id)
        }) {
            return Ok(());
        }
        let blocked = self.actors.get(&target).is_some_and(|actor| {
            actor.effects.iter().any(|active| {
                active.expires_at_tick > self.tick
                    && effect
                        .blocked_by_effect_ids
                        .binary_search(&active.effect_id)
                        .is_ok()
            })
        });
        if blocked {
            return Ok(());
        }
        let duration_ticks = u64::from(duration_turns.max(u32::from(effect.permanent)))
            .checked_mul(SimTick::HZ)
            .ok_or(SimError::NumericOverflow)?;
        let maximum_duration_ticks = u64::from(effect.maximum_accumulated_duration_turns)
            .checked_mul(SimTick::HZ)
            .ok_or(SimError::NumericOverflow)?;
        let current_tick = self.tick;
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        if let Some(existing) = actor.effects.iter_mut().find(|existing| {
            existing.effect_id == effect.effect_id && existing.body_part_id == body_part_id
        }) {
            existing.intensity = intensity;
            existing.modifiers = application.modifiers.clone();
            if existing.expires_at_tick != SimTick(u64::MAX) {
                let remaining = existing.expires_at_tick.0.saturating_sub(current_tick.0);
                let added_turns = u128::from(duration_turns)
                    .checked_mul(u128::from(effect.duration_add_percent))
                    .and_then(|value| value.checked_div(100))
                    .and_then(|value| u64::try_from(value).ok())
                    .ok_or(SimError::NumericOverflow)?;
                let added = added_turns
                    .checked_mul(SimTick::HZ)
                    .ok_or(SimError::NumericOverflow)?;
                existing.expires_at_tick = SimTick(
                    current_tick
                        .0
                        .saturating_add(remaining.saturating_add(added).min(maximum_duration_ticks))
                        .min(u64::MAX - 1),
                );
            }
        } else if actor.effects.len() < 1_024 {
            actor.effects.push(ActorEffectSnapshotV1 {
                effect_id: effect.effect_id.clone(),
                body_part_id,
                intensity,
                expires_at_tick: if effect.permanent {
                    SimTick(u64::MAX)
                } else {
                    SimTick(
                        current_tick
                            .0
                            .saturating_add(duration_ticks.min(maximum_duration_ticks))
                            .min(u64::MAX - 1),
                    )
                },
                modifiers: application.modifiers.clone(),
            });
            actor.effects.sort_by(|left, right| {
                (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
            });
        }
        Ok(())
    }

    pub(super) fn try_creature_special_attacks(
        &mut self,
        source: CreatureId,
        visible_target: Option<(ActorId, WorldPosition)>,
        destination: Option<WorldPosition>,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<i64, SimError> {
        if self.creatures.get(&source).is_some_and(|creature| {
            creature.effects.iter().any(|effect| {
                effect.expires_at_tick > self.tick
                    && matches!(
                        effect.effect_id.as_str(),
                        "stunned" | "psi_stunned" | "downed" | "webbed"
                    )
            })
        }) {
            return Ok(0);
        }
        let profiles = self
            .creature_prototype(source)?
            .map(|prototype| prototype.special_attacks.clone())
            .unwrap_or_default();
        let states = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .special_attacks
            .clone();
        let mut total_cost = 0_i64;
        for (index, profile) in profiles.iter().enumerate() {
            let mut move_cost_moves = profile.move_cost_moves;
            let mut starts_cooldown = true;
            let Some(state) = states
                .iter()
                .find(|state| state.attack_id == profile.attack_id)
            else {
                return Err(SimError::InvalidCreature);
            };
            if !state.enabled || state.cooldown_turns > 0 {
                continue;
            }
            if profile.condition.as_ref().is_some_and(|condition| {
                !self
                    .creature_eoc_condition_matches(
                        source,
                        visible_target.map(|(target, _position)| target),
                        condition,
                    )
                    .unwrap_or(false)
            }) {
                continue;
            }
            let sequence = turn_sequence
                .checked_mul(64)
                .and_then(|sequence| sequence.checked_add(index as u64))
                .ok_or(SimError::NumericOverflow)?;
            let used = match profile.kind {
                WorldgenMonsterSpecialAttackKindV1::Melee
                | WorldgenMonsterSpecialAttackKindV1::Bite => {
                    let Some((target, target_position)) = visible_target else {
                        continue;
                    };
                    let source_position = self
                        .creatures
                        .get(&source)
                        .ok_or(SimError::UnknownCreature)?
                        .position;
                    let adjacent = horizontally_adjacent(source_position, target_position);
                    let distance = ranged_distance(source_position, target_position);
                    if (profile.no_adjacent && adjacent)
                        || (profile.range == 1 && !adjacent)
                        || distance > profile.range
                        || (profile.range > 1
                            && !self.has_clear_shot(source_position, target_position))
                        || self.actors.get(&target).is_none_or(|actor| actor.hp <= 0)
                    {
                        continue;
                    }
                    self.execute_creature_special_attack(
                        source, target, profile, sequence, events,
                    )?;
                    if !profile.eoc_ids.is_empty() {
                        let _ =
                            self.apply_creature_eocs(source, target, &profile.eoc_ids, sequence)?;
                    }
                    true
                }
                WorldgenMonsterSpecialAttackKindV1::Leap => {
                    let Some(destination) = destination else {
                        continue;
                    };
                    let has_live_target = visible_target.is_some_and(|(target, _position)| {
                        self.actors.get(&target).is_some_and(|actor| actor.hp > 0)
                    });
                    if !has_live_target && !profile.leap_allow_no_target {
                        continue;
                    }
                    self.execute_creature_leap(source, destination, profile, sequence, events)?
                }
                WorldgenMonsterSpecialAttackKindV1::Eoc => {
                    let Some((target, target_position)) = visible_target else {
                        continue;
                    };
                    let source_position = self
                        .creatures
                        .get(&source)
                        .ok_or(SimError::UnknownCreature)?
                        .position;
                    let distance = ranged_distance(source_position, target_position);
                    if distance == 0
                        || distance > profile.range
                        || (profile.range == 1
                            && !horizontally_adjacent(source_position, target_position))
                        || (profile.range > 1
                            && !self.has_clear_shot(source_position, target_position))
                        || self.actors.get(&target).is_none_or(|actor| actor.hp <= 0)
                    {
                        continue;
                    }
                    let _ = self.apply_creature_eocs(source, target, &profile.eoc_ids, sequence)?;
                    true
                }
                WorldgenMonsterSpecialAttackKindV1::Gun => {
                    let Some((target, target_position)) = visible_target else {
                        continue;
                    };
                    let source_position = self
                        .creatures
                        .get(&source)
                        .ok_or(SimError::UnknownCreature)?
                        .position;
                    let distance = ranged_distance(source_position, target_position);
                    let selected_range = profile
                        .gun_ranges
                        .iter()
                        .find(|range| (range.minimum..=range.maximum).contains(&distance));
                    if distance == 0
                        || distance > profile.gun_item_range
                        || selected_range.is_none()
                        || !self.has_clear_shot(source_position, target_position)
                        || self.actors.get(&target).is_none_or(|actor| actor.hp <= 0)
                    {
                        continue;
                    }
                    if self.creatures.get(&source).is_some_and(|creature| {
                        creature.effects.iter().any(|effect| {
                            effect.expires_at_tick > self.tick
                                && matches!(
                                    effect.effect_id.as_str(),
                                    "stunned" | "psi_stunned" | "sensor_stun"
                                )
                        })
                    }) {
                        continue;
                    }
                    if !self.prepare_creature_gun_targeting(
                        source,
                        target,
                        source_position,
                        profile,
                        events,
                    )? {
                        move_cost_moves = profile.gun_targeting_cost_moves;
                        starts_cooldown = false;
                        true
                    } else {
                        let shot_count = selected_range
                            .map(|range| range.shot_count)
                            .ok_or(SimError::InvalidCreature)?;
                        if !profile.gun_ammunition_type_id.is_empty()
                            && self.creatures.get(&source).is_none_or(|creature| {
                                creature
                                    .ammunition
                                    .get(&profile.gun_ammunition_type_id)
                                    .is_none_or(|amount| *amount == 0)
                            })
                        {
                            continue;
                        }
                        self.execute_creature_gun_attack(
                            source,
                            target,
                            source_position,
                            profile,
                            shot_count,
                            sequence,
                            events,
                        )?;
                        true
                    }
                }
                WorldgenMonsterSpecialAttackKindV1::Polymorph => {
                    self.execute_creature_polymorph(source, profile, events)?;
                    // The replacement owns a different attack catalog and
                    // polymorph clears the current turn's moves.
                    return Ok(0);
                }
                WorldgenMonsterSpecialAttackKindV1::Spell => self.execute_creature_spell_program(
                    source,
                    visible_target,
                    profile,
                    sequence,
                    events,
                )?,
            };
            if !used {
                continue;
            }
            if starts_cooldown {
                self.creatures
                    .get_mut(&source)
                    .and_then(|creature| {
                        creature
                            .special_attacks
                            .iter_mut()
                            .find(|state| state.attack_id == profile.attack_id)
                    })
                    .ok_or(SimError::InvalidCreature)?
                    .cooldown_turns = profile.cooldown_turns;
            }
            total_cost = total_cost
                .checked_add(
                    i64::from(move_cost_moves)
                        .checked_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .ok_or(SimError::NumericOverflow)?;
        }
        Ok(total_cost)
    }

    fn execute_creature_spell_program(
        &mut self,
        source: CreatureId,
        visible_target: Option<(ActorId, WorldPosition)>,
        profile: &WorldgenMonsterSpecialAttackV1,
        sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let origin = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .position;
        let intended_target = if profile.spell_target_self {
            origin
        } else {
            let Some((target, target_position)) = visible_target else {
                return Ok(false);
            };
            if self.actors.get(&target).is_none_or(|actor| actor.hp <= 0)
                || horizontal_euclidean_distance_floor(origin, target_position)? > profile.range
            {
                return Ok(false);
            }
            target_position
        };
        let mut rng = self.named_rng(
            b"creature-special-spell-program",
            &[source.as_u128()],
            sequence,
        );
        if profile.spell_extra_effects_first {
            for (index, extra) in profile.spell_extra_effects.iter().enumerate() {
                self.execute_creature_spell_effect(
                    source,
                    intended_target,
                    CreatureSpellProfile::Extra(extra),
                    sequence.wrapping_add(index as u64 + 1),
                    &mut rng,
                    events,
                )?;
            }
        }
        self.execute_creature_spell_effect(
            source,
            intended_target,
            CreatureSpellProfile::Primary(profile),
            sequence,
            &mut rng,
            events,
        )?;
        if !profile.spell_extra_effects_first {
            for (index, extra) in profile.spell_extra_effects.iter().enumerate() {
                self.execute_creature_spell_effect(
                    source,
                    intended_target,
                    CreatureSpellProfile::Extra(extra),
                    sequence.wrapping_add(index as u64 + 1),
                    &mut rng,
                    events,
                )?;
            }
        }
        Ok(true)
    }

    fn execute_creature_spell_effect(
        &mut self,
        source: CreatureId,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
        sequence: u64,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        if profile.summoned_monster_type_id().is_empty() {
            self.execute_creature_spell_attack(
                source,
                intended_target,
                profile,
                sequence,
                rng,
                events,
            )
        } else {
            self.execute_creature_summon_spell(source, intended_target, profile, rng, events)
        }
    }

    fn execute_creature_spell_attack(
        &mut self,
        source: CreatureId,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
        sequence: u64,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let area = self.creature_spell_area(source, intended_target, profile)?;
        for position in area {
            let (field_type_id, _, _, _, _) = profile.field();
            if !field_type_id.is_empty() {
                self.apply_creature_spell_field(position, profile, rng, events)?;
            }
            let Some(target) = self.actor_at(position) else {
                continue;
            };
            if !profile.eoc_ids().is_empty() {
                let _ =
                    self.apply_creature_spell_eocs(source, target, profile.eoc_ids(), sequence)?;
                continue;
            }
            if profile.damage().is_empty() {
                self.apply_monster_attack_effects(target, "", false, profile.effects(), rng)?;
                continue;
            }
            let (minimum_damage, maximum_damage) = profile.damage_bounds();
            let damage_points =
                roll_inclusive_i32(minimum_damage / 1_000_000, maximum_damage / 1_000_000, rng)?;
            let multiplier = damage_points
                .checked_mul(1_000_000)
                .ok_or(SimError::NumericOverflow)?;
            let damage = profile
                .damage()
                .iter()
                .map(|unit| {
                    Ok(ActorDamageUnit {
                        damage_type_id: unit.damage_type_id.clone(),
                        amount_milli: unit.amount_milli,
                        armor_penetration_milli: unit.armor_penetration_milli,
                        armor_multiplier_millionths: unit.armor_multiplier_millionths,
                        damage_multiplier_millionths: unit.damage_multiplier_millionths,
                        damage_multiplier_adjustment_millionths: multiplier,
                        damage_multiplier_divisor: 1,
                        constant_armor_multiplier_millionths: unit
                            .constant_armor_multiplier_millionths,
                        constant_damage_multiplier_millionths: unit
                            .constant_damage_multiplier_millionths,
                    })
                })
                .collect::<Result<Vec<_>, SimError>>()?;
            let (outcome, was_sleeping, _cut_or_stab_damage) =
                self.damage_actor_components(target, &damage, rng)?;
            if !profile.effects().is_empty() {
                self.apply_monster_attack_effects(
                    target,
                    &outcome.body_part_id,
                    false,
                    profile.effects(),
                    rng,
                )?;
            }
            events.push(self.make_event(WorldEventKind::ActorDamagedByCreature {
                source,
                target,
                body_part_id: outcome.body_part_id,
                amount: u32::from(outcome.amount),
                remaining_part_hp: outcome.remaining_part_hp,
                remaining_hp: outcome.remaining_hp,
            })?);
            if outcome.amount > 0 {
                self.interrupt_craft(target, events)?;
                self.interrupt_book_study(target, BookStudyInterruptionReason::Damage, events)?;
                self.interrupt_disassembly(target, DisassemblyInterruptionReason::Damage, events)?;
                self.interrupt_construction(
                    target,
                    ConstructionInterruptionReason::Damage,
                    events,
                )?;
                if was_sleeping && outcome.remaining_hp > 0 {
                    self.wake_actor(target, cdda_protocol::WakeReason::Damage, events)?;
                }
                if outcome.remaining_hp <= 0 {
                    events.push(self.make_event(WorldEventKind::ActorKilledByCreature {
                        actor_id: target,
                        killer: source,
                    })?);
                }
            }
        }
        Ok(())
    }

    fn creature_spell_area(
        &self,
        source: CreatureId,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
    ) -> Result<Vec<WorldPosition>, SimError> {
        let origin = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .position;
        let intended_target = if profile.target_self() {
            origin
        } else {
            intended_target
        };
        if origin.z != intended_target.z {
            return Ok(Vec::new());
        }
        let raw_area = match profile.shape() {
            WorldgenMonsterSpellShapeV1::Blast => {
                self.creature_blast_spell_area(origin, intended_target, profile)?
            }
            WorldgenMonsterSpellShapeV1::Line => {
                self.creature_line_spell_area(origin, intended_target, profile)?
            }
            WorldgenMonsterSpellShapeV1::Cone => {
                self.creature_cone_spell_area(origin, intended_target, profile)?
            }
        };
        Ok(raw_area
            .into_iter()
            .filter(|position| self.chunks.contains_key(&position.chunk_and_local().0))
            .filter(|position| self.spell_area_position_is_valid(source, *position, profile))
            .collect())
    }

    fn creature_blast_spell_area(
        &self,
        origin: WorldPosition,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
    ) -> Result<BTreeSet<WorldPosition>, SimError> {
        let center = if profile.aoe() == 0 {
            let Some(epicenter) =
                self.spell_blast_epicenter(origin, intended_target, profile.no_projectile())
            else {
                return Ok(BTreeSet::new());
            };
            epicenter
        } else {
            intended_target
        };
        let radius = i32::from(profile.aoe());
        let mut area = BTreeSet::new();
        for offset_x in -radius..=radius {
            for offset_y in -radius..=radius {
                let Some(x) = center.x.checked_add(offset_x) else {
                    continue;
                };
                let Some(y) = center.y.checked_add(offset_y) else {
                    continue;
                };
                let position = WorldPosition { x, y, z: center.z };
                if horizontal_euclidean_distance_floor(center, position)?
                    <= u32::from(profile.aoe())
                    && (profile.ignore_walls()
                        || self.spell_blast_line_is_passable(center, position))
                {
                    area.insert(position);
                }
            }
        }
        Ok(area)
    }

    fn creature_cone_spell_area(
        &self,
        origin: WorldPosition,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
    ) -> Result<BTreeSet<WorldPosition>, SimError> {
        if origin == intended_target {
            return Ok(BTreeSet::new());
        }
        let delta_x = f64::from(intended_target.x) - f64::from(origin.x);
        let delta_y = f64::from(intended_target.y) - f64::from(origin.y);
        let initial_angle = delta_y.atan2(delta_x).to_degrees().rem_euclid(360.0);
        let half_width = f64::from(profile.aoe()) / 2.0;
        let start_angle = initial_angle - half_width;
        let end_angle = initial_angle + half_width;
        let mut targets = BTreeSet::new();
        let mut endpoints = BTreeSet::new();
        let mut angle = start_angle;
        while angle <= end_angle {
            let radians = angle.to_radians();
            for range in 1..=profile.range() {
                let range = f64::from(range);
                let relative_x = (range * radians.cos()) as i32;
                let relative_y = (range * radians.sin()) as i32;
                let Some(x) = origin.x.checked_add(relative_x) else {
                    continue;
                };
                let Some(y) = origin.y.checked_add(relative_y) else {
                    continue;
                };
                let position = WorldPosition { x, y, z: origin.z };
                if profile.ignore_walls() {
                    targets.insert(position);
                } else {
                    endpoints.insert(position);
                }
            }
            angle += 1.0;
        }
        if !profile.ignore_walls() {
            for endpoint in endpoints {
                for position in projectile_line(origin, endpoint) {
                    if !self.is_passable(position) {
                        break;
                    }
                    targets.insert(position);
                }
            }
        }
        targets.remove(&origin);
        Ok(targets)
    }

    fn creature_line_spell_area(
        &self,
        origin: WorldPosition,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
    ) -> Result<BTreeSet<WorldPosition>, SimError> {
        let delta = RelativeSpellPoint {
            x: intended_target
                .x
                .checked_sub(origin.x)
                .ok_or(SimError::NumericOverflow)?,
            y: intended_target
                .y
                .checked_sub(origin.y)
                .ok_or(SimError::NumericOverflow)?,
        };
        let distance = delta.x.unsigned_abs().max(delta.y.unsigned_abs());
        if distance == 0 {
            return Ok(BTreeSet::new());
        }
        let delta_perpendicular = RelativeSpellPoint {
            x: delta.y.checked_neg().ok_or(SimError::NumericOverflow)?,
            y: delta.x,
        };
        let axis_delta = if delta.x.unsigned_abs() > delta.y.unsigned_abs() {
            RelativeSpellPoint { x: delta.x, y: 0 }
        } else {
            RelativeSpellPoint { x: 0, y: delta.y }
        };
        let clockwise_perpendicular_axis = RelativeSpellPoint {
            x: axis_delta
                .y
                .checked_neg()
                .ok_or(SimError::NumericOverflow)?,
            y: axis_delta.x,
        };
        let unit_clockwise_perpendicular_axis = RelativeSpellPoint {
            x: clockwise_perpendicular_axis.x.signum(),
            y: clockwise_perpendicular_axis.y.signum(),
        };
        let counterclockwise_length = i32::from(profile.aoe() / 2);
        let clockwise_length = i32::from(profile.aoe()) - counterclockwise_length;
        let delta_side = spell_line_side(RelativeSpellPoint::ZERO, axis_delta, delta);
        let mut path_to_target = relative_spell_line(delta);
        path_to_target.pop();
        path_to_target.insert(0, RelativeSpellPoint::ZERO);
        let mut base_line = SpellLineIterator {
            delta_line: &path_to_target,
            current_origin: RelativeSpellPoint::ZERO,
            delta,
            index: 0,
        };
        let mut result = BTreeSet::new();
        self.build_creature_spell_line(
            base_line.clone(),
            origin,
            delta,
            delta_perpendicular,
            profile.ignore_walls(),
            &mut result,
        )?;
        let clockwise_leg = relative_spell_line(RelativeSpellPoint {
            x: unit_clockwise_perpendicular_axis.x * clockwise_length,
            y: unit_clockwise_perpendicular_axis.y * clockwise_length,
        });
        let counterclockwise_leg = relative_spell_line(RelativeSpellPoint {
            x: unit_clockwise_perpendicular_axis.x * -counterclockwise_length,
            y: unit_clockwise_perpendicular_axis.y * -counterclockwise_length,
        });
        match delta_side {
            0 => {
                for point in &clockwise_leg {
                    base_line.reset(*point);
                    if !self.creature_spell_line_point_is_passable(
                        origin,
                        *point,
                        profile.ignore_walls(),
                    ) {
                        break;
                    }
                    self.build_creature_spell_line(
                        base_line.clone(),
                        origin,
                        delta,
                        delta_perpendicular,
                        profile.ignore_walls(),
                        &mut result,
                    )?;
                }
                for point in &counterclockwise_leg {
                    base_line.reset(*point);
                    if !self.creature_spell_line_point_is_passable(
                        origin,
                        *point,
                        profile.ignore_walls(),
                    ) {
                        break;
                    }
                    self.build_creature_spell_line(
                        base_line.clone(),
                        origin,
                        delta,
                        delta_perpendicular,
                        profile.ignore_walls(),
                        &mut result,
                    )?;
                }
            }
            1 => {
                for point in &counterclockwise_leg {
                    base_line.reset(*point);
                    move_spell_line_to_boundary(&mut base_line, delta_perpendicular, true, true)?;
                    if !self.creature_spell_line_point_is_passable(
                        origin,
                        *point,
                        profile.ignore_walls(),
                    ) {
                        break;
                    }
                    self.build_creature_spell_line(
                        base_line.clone(),
                        origin,
                        delta,
                        delta_perpendicular,
                        profile.ignore_walls(),
                        &mut result,
                    )?;
                }
                for point in &clockwise_leg {
                    base_line.reset(*point);
                    move_spell_line_to_boundary(&mut base_line, delta_perpendicular, false, false)?;
                    base_line.next().ok_or(SimError::NumericOverflow)?;
                    if !self.creature_spell_line_point_is_passable(
                        origin,
                        *point,
                        profile.ignore_walls(),
                    ) {
                        break;
                    }
                    self.build_creature_spell_line(
                        base_line.clone(),
                        origin,
                        delta,
                        delta_perpendicular,
                        profile.ignore_walls(),
                        &mut result,
                    )?;
                }
            }
            -1 => {
                for point in &counterclockwise_leg {
                    base_line.reset(*point);
                    move_spell_line_to_boundary(&mut base_line, delta_perpendicular, false, false)?;
                    base_line.next().ok_or(SimError::NumericOverflow)?;
                    if !self.creature_spell_line_point_is_passable(
                        origin,
                        *point,
                        profile.ignore_walls(),
                    ) {
                        break;
                    }
                    self.build_creature_spell_line(
                        base_line.clone(),
                        origin,
                        delta,
                        delta_perpendicular,
                        profile.ignore_walls(),
                        &mut result,
                    )?;
                }
                for point in &clockwise_leg {
                    base_line.reset(*point);
                    move_spell_line_to_boundary(&mut base_line, delta_perpendicular, true, true)?;
                    if !self.creature_spell_line_point_is_passable(
                        origin,
                        *point,
                        profile.ignore_walls(),
                    ) {
                        break;
                    }
                    self.build_creature_spell_line(
                        base_line.clone(),
                        origin,
                        delta,
                        delta_perpendicular,
                        profile.ignore_walls(),
                        &mut result,
                    )?;
                }
            }
            _ => return Err(SimError::InvalidCreature),
        }
        result.remove(&origin);
        Ok(result)
    }

    fn build_creature_spell_line(
        &self,
        mut line: SpellLineIterator<'_>,
        source: WorldPosition,
        delta: RelativeSpellPoint,
        delta_perpendicular: RelativeSpellPoint,
        ignore_walls: bool,
        result: &mut BTreeSet<WorldPosition>,
    ) -> Result<(), SimError> {
        for _ in 0..=4_096 {
            let Some(relative) = line.get() else {
                return Err(SimError::NumericOverflow);
            };
            if !spell_line_between_or_on(
                RelativeSpellPoint::ZERO,
                delta,
                delta_perpendicular,
                relative,
            ) {
                return Ok(());
            }
            let Some(position) = self.creature_spell_line_absolute_position(source, relative)
            else {
                return Ok(());
            };
            if !ignore_walls && !self.is_passable(position) {
                return Ok(());
            }
            result.insert(position);
            line.next().ok_or(SimError::NumericOverflow)?;
        }
        Err(SimError::InvalidCreature)
    }

    fn creature_spell_line_point_is_passable(
        &self,
        source: WorldPosition,
        relative: RelativeSpellPoint,
        ignore_walls: bool,
    ) -> bool {
        self.creature_spell_line_absolute_position(source, relative)
            .is_some_and(|position| ignore_walls || self.is_passable(position))
    }

    fn creature_spell_line_absolute_position(
        &self,
        source: WorldPosition,
        relative: RelativeSpellPoint,
    ) -> Option<WorldPosition> {
        Some(WorldPosition {
            x: source.x.checked_add(relative.x)?,
            y: source.y.checked_add(relative.y)?,
            z: source.z,
        })
    }

    fn spell_blast_line_is_passable(&self, origin: WorldPosition, target: WorldPosition) -> bool {
        if origin.z != target.z {
            return false;
        }
        projectile_line(origin, target)
            .into_iter()
            .all(|position| self.is_passable(position))
    }

    fn spell_area_position_is_valid(
        &self,
        source: CreatureId,
        position: WorldPosition,
        profile: CreatureSpellProfile<'_>,
    ) -> bool {
        let (targets_hostile, targets_ground, targets_self) = profile.targets();
        if self
            .creatures
            .get(&source)
            .is_some_and(|creature| creature.position == position)
        {
            return targets_self;
        }
        if self
            .creatures
            .values()
            .any(|creature| creature.position == position)
        {
            return false;
        }
        if let Some(actor) = self
            .actors
            .values()
            .find(|actor| actor.position == position)
        {
            return actor.hp > 0 && targets_hostile;
        }
        if self.npcs.values().any(|npc| npc.position == position) {
            return false;
        }
        targets_ground
    }

    fn summoned_creature_can_enter(
        &self,
        prototype: &WorldgenMonsterPrototypeV1,
        position: WorldPosition,
    ) -> bool {
        self.chunks.contains_key(&position.chunk_and_local().0)
            && self.is_passable(position)
            && !self.actors.values().any(|actor| actor.position == position)
            && !self
                .creatures
                .values()
                .any(|creature| creature.position == position)
            && (!prototype.base.path_settings.avoid_dangerous_fields
                || !self.fields_at(position).is_some_and(|fields| {
                    fields.iter().any(|field| {
                        self.field_types
                            .get(&field.field_type_id)
                            .and_then(|field_type| {
                                field_type
                                    .intensity_levels
                                    .get(usize::from(field.intensity - 1))
                            })
                            .is_some_and(|level| level.dangerous)
                    })
                }))
    }

    fn spell_blast_epicenter(
        &self,
        origin: WorldPosition,
        target: WorldPosition,
        no_projectile: bool,
    ) -> Option<WorldPosition> {
        if origin.z != target.z {
            return None;
        }
        if no_projectile || origin == target {
            return Some(target);
        }
        let mut previous = None;
        for position in projectile_line(origin, target) {
            if !self.is_passable(position) {
                return Some(previous.unwrap_or(position));
            }
            if position == target {
                return Some(position);
            }
            previous = Some(position);
        }
        Some(target)
    }

    fn apply_creature_spell_field(
        &mut self,
        position: WorldPosition,
        profile: CreatureSpellProfile<'_>,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let (field_type_id, field_chance, field_intensity, field_variance, field_duration) =
            profile.field();
        let variance = u32::from(field_intensity)
            .checked_mul(field_variance)
            .and_then(|value| value.checked_div(1_000_000))
            .and_then(|value| i32::try_from(value).ok())
            .ok_or(SimError::NumericOverflow)?;
        let intensity = i32::from(field_intensity)
            .checked_add(roll_inclusive_i32(-variance, variance, rng)?)
            .ok_or(SimError::NumericOverflow)?;
        if intensity <= 0 {
            return Ok(());
        }
        let intensity = u16::try_from(intensity).map_err(|_| SimError::NumericOverflow)?;
        if field_chance > 1 && roll_inclusive_u32(1, field_chance, rng)? != 1 {
            return Ok(());
        }
        let initial_age = -i64::from(field_duration);
        let intensity = self.add_field_with_age(position, field_type_id, intensity, initial_age)?;
        events.push(self.make_event(WorldEventKind::FieldIntensityChanged {
            position,
            field_type_id: field_type_id.to_owned(),
            intensity,
        })?);
        Ok(())
    }

    fn execute_creature_summon_spell(
        &mut self,
        source: CreatureId,
        intended_target: WorldPosition,
        profile: CreatureSpellProfile<'_>,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let prototype = self
            .worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|prototype| {
                        prototype
                            .base
                            .monster_type_id
                            .as_str()
                            .cmp(profile.summoned_monster_type_id())
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
            })
            .cloned()
            .ok_or(SimError::InvalidCreature)?;
        if !prototype.runtime_spawnable {
            return Ok(());
        }
        let mut candidates = self.creature_spell_area(source, intended_target, profile)?;
        let (minimum_summons, maximum_summons, random_summons) = profile.summon_bounds();
        let requested = if random_summons {
            roll_inclusive_u32(u32::from(minimum_summons), u32::from(maximum_summons), rng)?
        } else {
            u32::from(minimum_summons)
        };
        let mut remaining = requested;
        while remaining > 0 && !candidates.is_empty() {
            let index = usize::try_from(rng.next_u64() % candidates.len() as u64)
                .map_err(|_| SimError::NumericOverflow)?;
            let position = candidates.remove(index);
            if !self.summoned_creature_can_enter(&prototype, position) {
                continue;
            }
            let creature_id =
                self.spawn_creature(creature_spawn_from_worldgen(&prototype, position))?;
            remaining -= 1;
            events.push(self.make_event(WorldEventKind::CreatureSummoned {
                caster: source,
                creature_id,
                monster_type_id: prototype.base.monster_type_id.clone(),
                position,
            })?);
        }
        Ok(())
    }

    fn execute_creature_polymorph(
        &mut self,
        source: CreatureId,
        profile: &WorldgenMonsterSpecialAttackV1,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let target = self
            .worldgen
            .as_ref()
            .and_then(|catalog| {
                catalog
                    .monster_prototypes
                    .binary_search_by(|prototype| {
                        prototype
                            .base
                            .monster_type_id
                            .as_str()
                            .cmp(&profile.polymorph_monster_type_id)
                    })
                    .ok()
                    .and_then(|index| catalog.monster_prototypes.get(index))
            })
            .cloned()
            .ok_or(SimError::InvalidCreature)?;
        if !target.runtime_spawnable {
            return Ok(());
        }
        let (from_type_id, old_hp, old_max_hp, old_speed, old_aggression, position) = {
            let creature = self
                .creatures
                .get(&source)
                .ok_or(SimError::UnknownCreature)?;
            (
                creature.type_id.clone(),
                creature.hp,
                creature.max_hp,
                creature.speed,
                creature.aggression,
                creature.position,
            )
        };
        if old_hp <= 0 || old_max_hp <= 0 {
            return Err(SimError::InvalidCreature);
        }
        let proportional_hp = i64::from(old_hp)
            .checked_mul(i64::from(target.base.max_hp))
            .ok_or(SimError::NumericOverflow)?
            / i64::from(old_max_hp);
        let proportional_hp =
            i32::try_from(proportional_hp).map_err(|_| SimError::NumericOverflow)?;
        let creature = self
            .creatures
            .get_mut(&source)
            .ok_or(SimError::UnknownCreature)?;
        creature.type_id = target.base.monster_type_id.clone();
        creature.hp = if profile.polymorph_keep_hp {
            old_hp
        } else {
            proportional_hp
        };
        creature.max_hp = target.base.max_hp;
        creature.speed = if profile.polymorph_keep_speed {
            old_speed
        } else {
            target.base.speed
        };
        creature.attack_cost_moves = target.base.attack_cost_moves;
        creature.aggression = if profile.polymorph_keep_aggression {
            old_aggression
        } else {
            target.base.aggression
        };
        creature.morale = target.base.morale;
        creature.melee_skill = target.base.melee_skill;
        creature.dodge = target.base.dodge;
        creature.size = target.base.size;
        creature.melee_dice = target.base.melee_dice;
        creature.melee_dice_sides = target.base.melee_dice_sides;
        creature.can_see = target.base.can_see;
        creature.vision_day = target.base.vision_day;
        creature.vision_night = target.base.vision_night;
        creature.stumbles = target.base.stumbles;
        creature.bashes = target.base.bashes;
        creature.group_bash = target.base.group_bash;
        creature.hears = target.base.hears;
        creature.good_hearing = target.base.good_hearing;
        creature.clumsy_attacks = target.base.clumsy_attacks;
        creature.immobile = target.base.immobile;
        creature.pacifist = target.base.pacifist;
        creature.can_open_doors = target.base.can_open_doors;
        creature.path_settings = target.base.path_settings;
        creature.action_points = 0;
        creature.special_attacks = target
            .special_attacks
            .iter()
            .map(|attack| CreatureSpecialAttackStateV1 {
                attack_id: attack.attack_id.clone(),
                cooldown_turns: attack.cooldown_turns,
                enabled: true,
            })
            .collect();
        creature.blood_field_type_id = target.base.blood_field_type_id.clone();
        creature.corpse = target.leaves_corpse.then(|| target.base.clone());
        events.push(self.make_event(WorldEventKind::CreaturePolymorphed {
            creature_id: source,
            from_type_id,
            to_type_id: target.base.monster_type_id,
            position,
        })?);
        let remaining_hp = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .hp;
        if remaining_hp <= 0 {
            self.finish_creature_death(source, remaining_hp, events)?;
        }
        Ok(())
    }

    fn prepare_creature_gun_targeting(
        &mut self,
        source: CreatureId,
        target: ActorId,
        origin: WorldPosition,
        profile: &WorldgenMonsterSpecialAttackV1,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let requires_targeting = profile.gun_require_targeting_player;
        let not_targeted = requires_targeting
            && self
                .creatures
                .get(&source)
                .ok_or(SimError::UnknownCreature)?
                .effects
                .iter()
                .all(|effect| effect.effect_id != "targeted");
        let not_laser_locked = requires_targeting
            && profile.gun_laser_lock
            && self
                .actors
                .get(&target)
                .ok_or(SimError::UnknownActor)?
                .effects
                .iter()
                .all(|effect| effect.effect_id != "was_laserlocked");
        if not_targeted || not_laser_locked {
            let duration_ticks = u64::from(profile.gun_targeting_timeout_turns)
                .checked_mul(SimTick::HZ)
                .ok_or(SimError::NumericOverflow)?;
            let expires_at_tick = SimTick(
                self.tick
                    .0
                    .checked_add(duration_ticks)
                    .ok_or(SimError::NumericOverflow)?,
            );
            if expires_at_tick > self.tick {
                if not_targeted {
                    let creature = self
                        .creatures
                        .get_mut(&source)
                        .ok_or(SimError::UnknownCreature)?;
                    insert_whole_creature_effect(
                        &mut creature.effects,
                        "targeted",
                        expires_at_tick,
                    );
                }
                if not_laser_locked {
                    let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
                    insert_whole_creature_effect(
                        &mut actor.effects,
                        "laserlocked",
                        expires_at_tick,
                    );
                    insert_whole_creature_effect(
                        &mut actor.effects,
                        "was_laserlocked",
                        expires_at_tick,
                    );
                }
            }
            let sound_volume = if profile.gun_targeting_sound.is_empty() {
                0
            } else {
                profile.gun_targeting_volume
            };
            if sound_volume > 0 || not_laser_locked {
                events.push(self.make_event(WorldEventKind::CreatureTargetedActor {
                    source,
                    target,
                    origin,
                    sound: profile.gun_targeting_sound.clone(),
                    sound_volume,
                    laser_lock: not_laser_locked,
                })?);
            }
            return Ok(false);
        }
        if requires_targeting {
            let extension_ticks = i64::from(profile.gun_targeting_timeout_extend_turns)
                .checked_mul(i64::try_from(SimTick::HZ).map_err(|_| SimError::NumericOverflow)?)
                .ok_or(SimError::NumericOverflow)?;
            let creature = self
                .creatures
                .get_mut(&source)
                .ok_or(SimError::UnknownCreature)?;
            add_or_extend_whole_creature_effect(
                &mut creature.effects,
                "targeted",
                extension_ticks,
                self.tick,
            );
        }
        if profile.gun_laser_lock {
            let extension_ticks =
                i64::try_from(5 * SimTick::HZ).map_err(|_| SimError::NumericOverflow)?;
            let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
            add_or_extend_whole_creature_effect(
                &mut actor.effects,
                "was_laserlocked",
                extension_ticks,
                self.tick,
            );
        }
        Ok(true)
    }

    fn execute_creature_gun_attack(
        &mut self,
        source: CreatureId,
        target: ActorId,
        origin: WorldPosition,
        profile: &WorldgenMonsterSpecialAttackV1,
        requested_shots: u16,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let target_position = self
            .actors
            .get(&target)
            .ok_or(SimError::UnknownActor)?
            .position;
        let distance = ranged_distance(origin, target_position);
        let mut rng = self.named_rng(
            b"creature-special-gun-hit",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let shot_count = if profile.gun_ammunition_type_id.is_empty() {
            requested_shots
        } else {
            let remaining = self
                .creatures
                .get(&source)
                .and_then(|creature| creature.ammunition.get(&profile.gun_ammunition_type_id))
                .copied()
                .ok_or(SimError::InvalidCreature)?;
            u16::try_from(remaining.min(u32::from(requested_shots)))
                .map_err(|_| SimError::NumericOverflow)?
        };
        for _shot in 0..shot_count {
            let live_target = self.actors.get(&target).is_some_and(|actor| actor.hp > 0);
            if !profile.gun_ammunition_type_id.is_empty() {
                let remaining = self
                    .creatures
                    .get_mut(&source)
                    .and_then(|creature| {
                        creature.ammunition.get_mut(&profile.gun_ammunition_type_id)
                    })
                    .ok_or(SimError::InvalidCreature)?;
                *remaining = remaining.checked_sub(1).ok_or(SimError::InvalidCreature)?;
            }
            let miss_per_thousand = u64::from(profile.gun_dispersion)
                .checked_mul(u64::from(distance))
                .ok_or(SimError::NumericOverflow)?
                .checked_div(u64::from(profile.gun_item_range))
                .ok_or(SimError::NumericOverflow)?
                .min(900);
            let hit = u64::from(rng.next_u32() % 1_000) >= miss_per_thousand;
            events.push(
                self.make_event(WorldEventKind::CreatureRangedAttackResolved {
                    source,
                    target,
                    origin,
                    gun_type_id: profile.gun_type_id.clone(),
                    hit,
                    sound: ranged_sound_description(profile.gun_sound_volume).to_owned(),
                    sound_volume: profile.gun_sound_volume,
                })?,
            );
            self.apply_creature_projectile_trails(
                origin,
                target_position,
                profile,
                &mut rng,
                events,
            )?;
            if !hit || !live_target {
                self.apply_creature_projectile_area(target_position, profile, &mut rng, events)?;
                continue;
            }
            let damage = profile
                .damage
                .iter()
                .map(|unit| ActorDamageUnit {
                    damage_type_id: unit.damage_type_id.clone(),
                    amount_milli: unit.amount_milli,
                    armor_penetration_milli: unit.armor_penetration_milli,
                    armor_multiplier_millionths: unit.armor_multiplier_millionths,
                    damage_multiplier_millionths: unit.damage_multiplier_millionths,
                    damage_multiplier_adjustment_millionths: 1_000_000,
                    damage_multiplier_divisor: 1,
                    constant_armor_multiplier_millionths: unit.constant_armor_multiplier_millionths,
                    constant_damage_multiplier_millionths: unit
                        .constant_damage_multiplier_millionths,
                })
                .collect::<Vec<_>>();
            let (outcome, was_sleeping, _cut_or_stab_damage) =
                self.damage_actor_components(target, &damage, &mut rng)?;
            if profile.gun_blinds_eyes {
                self.apply_creature_projectile_blindness(target, &outcome.body_part_id, &mut rng)?;
            }
            self.apply_creature_projectile_on_hit_effects(target, &outcome.body_part_id, profile)?;
            events.push(self.make_event(WorldEventKind::ActorDamagedByCreature {
                source,
                target,
                body_part_id: outcome.body_part_id,
                amount: u32::from(outcome.amount),
                remaining_part_hp: outcome.remaining_part_hp,
                remaining_hp: outcome.remaining_hp,
            })?);
            if outcome.amount > 0 {
                self.interrupt_craft(target, events)?;
                self.interrupt_book_study(target, BookStudyInterruptionReason::Damage, events)?;
                self.interrupt_disassembly(target, DisassemblyInterruptionReason::Damage, events)?;
                self.interrupt_construction(
                    target,
                    ConstructionInterruptionReason::Damage,
                    events,
                )?;
                if was_sleeping && outcome.remaining_hp > 0 {
                    self.wake_actor(target, cdda_protocol::WakeReason::Damage, events)?;
                }
                if outcome.remaining_hp <= 0 {
                    events.push(self.make_event(WorldEventKind::ActorKilledByCreature {
                        actor_id: target,
                        killer: source,
                    })?);
                }
            }
            self.apply_creature_projectile_area(target_position, profile, &mut rng, events)?;
        }
        Ok(())
    }

    fn apply_creature_projectile_trails(
        &mut self,
        origin: WorldPosition,
        endpoint: WorldPosition,
        profile: &WorldgenMonsterSpecialAttackV1,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        if profile
            .gun_projectile_effects
            .iter()
            .all(|effect| effect.trail_fields.is_empty())
        {
            return Ok(());
        }
        for position in projectile_line(origin, endpoint) {
            for effect in &profile.gun_projectile_effects {
                for field in &effect.trail_fields {
                    self.apply_creature_projectile_field(position, field, rng, events)?;
                }
            }
        }
        Ok(())
    }

    fn apply_creature_projectile_area(
        &mut self,
        endpoint: WorldPosition,
        profile: &WorldgenMonsterSpecialAttackV1,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        for effect in &profile.gun_projectile_effects {
            if rng.next_u32() % 100 >= u32::from(effect.trigger_chance_percent) {
                continue;
            }
            for field in &effect.area_fields {
                let radius = i32::from(field.radius);
                for y in -radius..=radius {
                    for x in -radius..=radius {
                        if x * x + y * y > radius * radius {
                            continue;
                        }
                        let offset_x = i8::try_from(x).map_err(|_| SimError::NumericOverflow)?;
                        let offset_y = i8::try_from(y).map_err(|_| SimError::NumericOverflow)?;
                        let Some(position) = endpoint.checked_offset(offset_x, offset_y, 0) else {
                            continue;
                        };
                        self.apply_creature_projectile_field(position, field, rng, events)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_creature_projectile_field(
        &mut self,
        position: WorldPosition,
        field: &WorldgenMonsterProjectileFieldEffectV1,
        rng: &mut impl Rng,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        if rng.next_u32() % 100 >= u32::from(field.chance_percent) {
            return Ok(());
        }
        if field.check_passable && !self.is_passable(position) {
            return Ok(());
        }
        let (chunk, _local) = position.chunk_and_local();
        if !self.chunks.contains_key(&chunk) {
            return Ok(());
        }
        let intensity = roll_inclusive_u32(
            u32::from(field.intensity_minimum),
            u32::from(field.intensity_maximum),
            rng,
        )?;
        let intensity = u8::try_from(intensity).map_err(|_| SimError::NumericOverflow)?;
        if intensity == 0 {
            return Ok(());
        }
        let intensity = self.add_field(position, &field.field_type_id, intensity)?;
        events.push(self.make_event(WorldEventKind::FieldIntensityChanged {
            position,
            field_type_id: field.field_type_id.clone(),
            intensity,
        })?);
        Ok(())
    }

    fn apply_creature_projectile_on_hit_effects(
        &mut self,
        target: ActorId,
        body_part_id: &str,
        profile: &WorldgenMonsterSpecialAttackV1,
    ) -> Result<(), SimError> {
        let current_tick = self.tick;
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        for projectile_effect in &profile.gun_projectile_effects {
            for on_hit in &projectile_effect.on_hit_effects {
                if actor.effects.iter().any(|active| {
                    active.expires_at_tick > current_tick
                        && on_hit
                            .blocked_by_effect_ids
                            .binary_search(&active.effect_id)
                            .is_ok()
                }) {
                    continue;
                }
                let duration_ticks = on_hit
                    .duration_seconds
                    .checked_mul(SimTick::HZ)
                    .ok_or(SimError::NumericOverflow)?;
                let maximum_duration_ticks = u64::from(on_hit.maximum_accumulated_duration_seconds)
                    .checked_mul(SimTick::HZ)
                    .ok_or(SimError::NumericOverflow)?;
                if let Some(effect) = actor.effects.iter_mut().find(|effect| {
                    effect.effect_id == on_hit.effect_id
                        && effect.body_part_id.as_deref() == Some(body_part_id)
                }) {
                    effect.intensity = on_hit.intensity;
                    effect.modifiers = on_hit.modifiers.clone();
                    if effect.expires_at_tick == SimTick(u64::MAX) {
                        continue;
                    }
                    let remaining = effect.expires_at_tick.0.saturating_sub(current_tick.0);
                    let added_seconds = u128::from(on_hit.duration_seconds)
                        .checked_mul(u128::from(on_hit.duration_add_percent))
                        .and_then(|value| value.checked_div(100))
                        .and_then(|value| u64::try_from(value).ok())
                        .ok_or(SimError::NumericOverflow)?;
                    let added = added_seconds
                        .checked_mul(SimTick::HZ)
                        .ok_or(SimError::NumericOverflow)?;
                    effect.expires_at_tick = SimTick(
                        current_tick
                            .0
                            .saturating_add(
                                remaining.saturating_add(added).min(maximum_duration_ticks),
                            )
                            .min(u64::MAX - 1),
                    );
                } else if actor.effects.len() < 1_024 {
                    actor.effects.push(ActorEffectSnapshotV1 {
                        effect_id: on_hit.effect_id.clone(),
                        body_part_id: Some(body_part_id.to_owned()),
                        intensity: on_hit.intensity,
                        expires_at_tick: SimTick(
                            current_tick
                                .0
                                .saturating_add(duration_ticks.min(maximum_duration_ticks))
                                .min(u64::MAX - 1),
                        ),
                        modifiers: on_hit.modifiers.clone(),
                    });
                }
            }
        }
        actor.effects.sort_by(|left, right| {
            (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
        });
        Ok(())
    }

    fn apply_creature_projectile_blindness(
        &mut self,
        target: ActorId,
        hit_body_part_id: &str,
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        let hit_head = self
            .actor_anatomy
            .parts
            .iter()
            .find(|part| part.body_part_id == hit_body_part_id)
            .is_some_and(|part| part.limb_types.iter().any(|limb_type| limb_type == "head"));
        if !hit_head {
            return Ok(());
        }
        let sensor_parts = self
            .actor_anatomy
            .parts
            .iter()
            .filter(|part| {
                part.limb_types
                    .iter()
                    .any(|limb_type| limb_type == "sensor")
            })
            .map(|part| part.body_part_id.clone())
            .collect::<Vec<_>>();
        if sensor_parts.is_empty() {
            return Ok(());
        }
        let sensor_index = usize::try_from(rng.next_u64() % sensor_parts.len() as u64)
            .map_err(|_| SimError::NumericOverflow)?;
        let body_part_id = sensor_parts[sensor_index].clone();
        let duration_turns = roll_inclusive_u32(3, 10, rng)?;
        let duration_ticks = u64::from(duration_turns)
            .checked_mul(SimTick::HZ)
            .ok_or(SimError::NumericOverflow)?;
        let expires_at_tick = SimTick(self.tick.0.saturating_add(duration_ticks).min(u64::MAX - 1));
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        if let Some(effect) = actor.effects.iter_mut().find(|effect| {
            effect.effect_id == "blind" && effect.body_part_id.as_deref() == Some(&body_part_id)
        }) {
            effect.intensity = effect.intensity.max(5);
            effect.expires_at_tick = effect.expires_at_tick.max(expires_at_tick);
        } else if actor.effects.len() < 1_024 {
            actor.effects.push(ActorEffectSnapshotV1 {
                effect_id: String::from("blind"),
                body_part_id: Some(body_part_id),
                intensity: 5,
                expires_at_tick,
                modifiers: Default::default(),
            });
            actor.effects.sort_by(|left, right| {
                (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
            });
        }
        Ok(())
    }

    fn execute_creature_leap(
        &mut self,
        source: CreatureId,
        destination: WorldPosition,
        profile: &WorldgenMonsterSpecialAttackV1,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<bool, SimError> {
        let origin = self
            .creatures
            .get(&source)
            .ok_or(SimError::UnknownCreature)?
            .position;
        if origin.z != destination.z || origin == destination {
            return Ok(false);
        }
        // The pinned default enables circular distance. `rl_dist` still
        // truncates that Euclidean result to an integer before the actor
        // compares it with the configured floating-point consider bounds.
        let destination_distance_milli = horizontal_euclidean_distance_floor(origin, destination)?
            .checked_mul(1_000)
            .ok_or(SimError::NumericOverflow)?;
        let origin_destination_distance = destination_distance_milli / 1_000;
        if destination_distance_milli < profile.leap_minimum_consider_range_milli
            || destination_distance_milli > profile.leap_maximum_consider_range_milli
        {
            return Ok(false);
        }
        let radius = i32::try_from(profile.leap_maximum_range_milli.div_ceil(1_000))
            .map_err(|_| SimError::NumericOverflow)?;
        let minimum_squared = u128::from(profile.leap_minimum_range_milli).pow(2);
        let maximum_squared = u128::from(profile.leap_maximum_range_milli).pow(2);
        let light_sources = self.active_light_sources();
        let mut candidates = Vec::new();
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let offset_x = i8::try_from(dx).map_err(|_| SimError::NumericOverflow)?;
                let offset_y = i8::try_from(dy).map_err(|_| SimError::NumericOverflow)?;
                let Some(candidate) = origin.checked_offset(offset_x, offset_y, 0) else {
                    continue;
                };
                let dx_milli = i128::from(dx) * 1_000;
                let dy_milli = i128::from(dy) * 1_000;
                let squared = u128::try_from(dx_milli * dx_milli + dy_milli * dy_milli)
                    .map_err(|_| SimError::NumericOverflow)?;
                if squared < minimum_squared || squared > maximum_squared {
                    continue;
                }
                let candidate_distance =
                    horizontal_euclidean_distance_floor(candidate, destination)?;
                if candidate_distance >= origin_destination_distance
                    && !(profile.leap_prefer || profile.leap_random)
                {
                    continue;
                }
                if !self.is_passable(candidate)
                    || self.actor_at(candidate).is_some()
                    || self.creature_at(candidate).is_some()
                    || self.npc_at(candidate).is_some()
                    || !self.has_clear_shot(origin, candidate)
                    || !self.creature_can_see_position(source, candidate, &light_sources)?
                    || (!profile.leap_ignore_destination_danger
                        && self
                            .creatures
                            .get(&source)
                            .ok_or(SimError::UnknownCreature)?
                            .path_settings
                            .avoid_dangerous_fields
                        && self.fields_at(candidate).is_some_and(|fields| {
                            fields.iter().any(|field| {
                                self.field_types
                                    .get(&field.field_type_id)
                                    .and_then(|field_type| {
                                        field_type
                                            .intensity_levels
                                            .get(usize::from(field.intensity - 1))
                                    })
                                    .is_some_and(|level| level.dangerous)
                            })
                        }))
                {
                    continue;
                }
                candidates.push((candidate_distance, candidate));
            }
        }
        if candidates.is_empty() {
            return Ok(false);
        }
        candidates
            .sort_by_key(|(distance, position)| (*distance, position.z, position.y, position.x));
        if !profile.leap_random {
            let best = candidates[0].0;
            candidates.retain(|(distance, _position)| *distance == best);
        }
        let mut rng = self.named_rng(b"creature-special-leap", &[source.as_u128()], turn_sequence);
        let index = usize::try_from(rng.next_u64() % candidates.len() as u64)
            .map_err(|_| SimError::NumericOverflow)?;
        let to = candidates[index].1;
        self.creatures
            .get_mut(&source)
            .ok_or(SimError::UnknownCreature)?
            .position = to;
        events.push(self.make_event(WorldEventKind::CreatureMoved {
            creature_id: source,
            from: origin,
            to,
        })?);
        Ok(true)
    }

    fn execute_creature_special_attack(
        &mut self,
        source: CreatureId,
        target: ActorId,
        profile: &WorldgenMonsterSpecialAttackV1,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let accuracy = profile.accuracy.unwrap_or(i32::from(
            self.creatures
                .get(&source)
                .ok_or(SimError::UnknownCreature)?
                .melee_skill,
        ));
        let (spread, dodge_attempted) =
            self.creature_actor_special_attack_roll(source, target, turn_sequence, accuracy)?;
        if dodge_attempted {
            self.consume_actor_dodge_attempt(target)?;
        }
        if profile.dodgeable && spread < 0 {
            let target_was_sleeping = self
                .actors
                .get(&target)
                .ok_or(SimError::UnknownActor)?
                .sleeping;
            events.push(self.make_event(WorldEventKind::CreatureMissedActor {
                source,
                target,
                stumbled: false,
                target_was_sleeping,
            })?);
            return Ok(());
        }
        let mut rng = self.named_rng(
            b"creature-special-melee-damage",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let attack_amount = roll_inclusive_u32(
            u32::from(profile.attack_amount_minimum),
            u32::from(profile.attack_amount_maximum),
            &mut rng,
        )?;
        let selected_parts = if profile.spread_damage {
            (0..self.actor_anatomy.parts.len()).collect::<Vec<_>>()
        } else {
            let hit_spread = i32::try_from(spread).map_err(|_| SimError::NumericOverflow)?;
            (0..attack_amount)
                .map(|_| {
                    crate::anatomy::select_body_part_index_for_hit(
                        &self.actor_anatomy,
                        hit_spread,
                        &mut rng,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let multiplier = roll_inclusive_i32(
            profile.minimum_damage_multiplier_millionths,
            profile.maximum_damage_multiplier_millionths,
            &mut rng,
        )?;
        let attack_amount_u32 =
            u32::try_from(selected_parts.len()).map_err(|_| SimError::NumericOverflow)?;
        let damage = profile
            .damage
            .iter()
            .map(|unit| {
                Ok(ActorDamageUnit {
                    damage_type_id: unit.damage_type_id.clone(),
                    amount_milli: unit.amount_milli,
                    armor_penetration_milli: unit.armor_penetration_milli,
                    armor_multiplier_millionths: unit.armor_multiplier_millionths,
                    damage_multiplier_millionths: unit.damage_multiplier_millionths,
                    damage_multiplier_adjustment_millionths: multiplier,
                    damage_multiplier_divisor: attack_amount_u32,
                    constant_armor_multiplier_millionths: unit.constant_armor_multiplier_millionths,
                    constant_damage_multiplier_millionths: unit
                        .constant_damage_multiplier_millionths,
                })
            })
            .collect::<Result<Vec<_>, SimError>>()?;
        let mut outcomes = Vec::with_capacity(selected_parts.len());
        let mut was_sleeping = false;
        let mut dealt_cut_or_stab_damage = false;
        for selected in selected_parts {
            let (outcome, hit_was_sleeping, hit_cut_or_stab_damage) =
                self.damage_actor_components_at(target, selected, &damage, &mut rng)?;
            was_sleeping |= hit_was_sleeping;
            dealt_cut_or_stab_damage |= hit_cut_or_stab_damage > 0;
            outcomes.push(outcome);
        }
        let aggregate_amount = outcomes.iter().try_fold(0_u32, |total, hit| {
            total
                .checked_add(u32::from(hit.amount))
                .ok_or(SimError::NumericOverflow)
        })?;
        let selected_body_part_ids = outcomes
            .iter()
            .map(|outcome| outcome.body_part_id.clone())
            .collect::<Vec<_>>();
        let outcome = outcomes.pop().ok_or(SimError::InvalidCreature)?;
        if aggregate_amount > 0 {
            self.apply_monster_attack_effects(
                target,
                &outcome.body_part_id,
                dealt_cut_or_stab_damage,
                &profile.effects,
                &mut rng,
            )?;
        } else if !profile.effects_require_damage {
            self.apply_monster_attack_effects_to_body_parts(
                target,
                &selected_body_part_ids,
                dealt_cut_or_stab_damage,
                &profile.effects,
                &mut rng,
            )?;
        }
        if aggregate_amount > 0 && matches!(profile.kind, WorldgenMonsterSpecialAttackKindV1::Bite)
        {
            self.apply_bite_infection(
                target,
                &outcome.body_part_id,
                profile.infection_chance_millionths,
                &mut rng,
            )?;
        }
        events.push(self.make_event(WorldEventKind::ActorDamagedByCreature {
            source,
            target,
            body_part_id: outcome.body_part_id,
            amount: aggregate_amount,
            remaining_part_hp: outcome.remaining_part_hp,
            remaining_hp: outcome.remaining_hp,
        })?);
        if aggregate_amount > 0 {
            self.interrupt_craft(target, events)?;
            self.interrupt_book_study(target, BookStudyInterruptionReason::Damage, events)?;
            self.interrupt_disassembly(target, DisassemblyInterruptionReason::Damage, events)?;
            self.interrupt_construction(target, ConstructionInterruptionReason::Damage, events)?;
            if was_sleeping && outcome.remaining_hp > 0 {
                self.wake_actor(target, cdda_protocol::WakeReason::Damage, events)?;
            }
            if outcome.remaining_hp <= 0 {
                events.push(self.make_event(WorldEventKind::ActorKilledByCreature {
                    actor_id: target,
                    killer: source,
                })?);
            }
        }
        Ok(())
    }

    fn apply_bite_infection(
        &mut self,
        target: ActorId,
        body_part_id: &str,
        chance_millionths: u32,
        rng: &mut impl Rng,
    ) -> Result<(), SimError> {
        if rng.next_u32() % 1_000_000 >= chance_millionths {
            return Ok(());
        }
        let actor = self.actors.get_mut(&target).ok_or(SimError::UnknownActor)?;
        if let Some(existing) = actor.effects.iter_mut().find(|effect| {
            matches!(effect.effect_id.as_str(), "bite" | "infected")
                && effect.body_part_id.as_deref() == Some(body_part_id)
        }) {
            existing.expires_at_tick = SimTick(u64::MAX);
            return Ok(());
        }
        if actor.effects.len() < 1_024 {
            actor.effects.push(ActorEffectSnapshotV1 {
                effect_id: String::from("bite"),
                body_part_id: Some(body_part_id.to_owned()),
                intensity: 1,
                expires_at_tick: SimTick(u64::MAX),
                modifiers: Default::default(),
            });
            actor.effects.sort_by(|left, right| {
                (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
            });
        }
        Ok(())
    }

    pub(super) fn creature_damage_after_armor(
        &self,
        target: CreatureId,
        damage_type: &str,
        damage_milli: u32,
    ) -> Result<u16, SimError> {
        let armor = i64::from(self.creature_armor_milli(target, damage_type)?);
        let remaining = i64::from(damage_milli).saturating_sub(armor).max(0);
        let rounded = remaining
            .checked_add(500)
            .ok_or(SimError::NumericOverflow)?
            / 1_000;
        u16::try_from(rounded).map_err(|_| SimError::NumericOverflow)
    }

    pub(super) fn actor_melee_damage_against_creature(
        &self,
        actor_id: ActorId,
        target: CreatureId,
    ) -> Result<u16, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let components = actor
            .wielded
            .and_then(|item_id| actor.inventory.get(&item_id))
            .map(|item| item.melee_damage_milli.clone())
            .unwrap_or_else(|| {
                std::collections::BTreeMap::from([(
                    String::from("bash"),
                    i32::from(UNARMED_DAMAGE) * 1_000,
                )])
            });
        let mut total_milli = 0_i64;
        for (damage_type, damage_milli) in components {
            let armor = i64::from(self.creature_armor_milli(target, &damage_type)?);
            total_milli = total_milli
                .checked_add(i64::from(damage_milli).saturating_sub(armor).max(0))
                .ok_or(SimError::NumericOverflow)?;
        }
        let rounded = total_milli
            .checked_add(500)
            .ok_or(SimError::NumericOverflow)?
            / 1_000;
        u16::try_from(rounded).map_err(|_| SimError::NumericOverflow)
    }
}

fn horizontal_euclidean_distance_floor(
    left: WorldPosition,
    right: WorldPosition,
) -> Result<u32, SimError> {
    u32::try_from(horizontal_distance_squared(left, right)?.isqrt())
        .map_err(|_| SimError::NumericOverflow)
}

fn roll_inclusive_u32(minimum: u32, maximum: u32, rng: &mut impl Rng) -> Result<u32, SimError> {
    let span = u64::from(maximum)
        .checked_sub(u64::from(minimum))
        .and_then(|span| span.checked_add(1))
        .ok_or(SimError::NumericOverflow)?;
    let offset = rng.next_u64() % span;
    u32::try_from(u64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
}

fn roll_inclusive_i32(minimum: i32, maximum: i32, rng: &mut impl Rng) -> Result<i32, SimError> {
    let span = i64::from(maximum)
        .checked_sub(i64::from(minimum))
        .and_then(|span| span.checked_add(1))
        .and_then(|span| u64::try_from(span).ok())
        .ok_or(SimError::NumericOverflow)?;
    let offset = i64::try_from(rng.next_u64() % span).map_err(|_| SimError::NumericOverflow)?;
    i32::try_from(i64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
}

fn merge_rolled_bash_damage(
    fixed: &mut ActorDamageUnit,
    rolled_amount_milli: i32,
    rolled_armor_penetration_milli: i32,
) -> Result<(), SimError> {
    let existing_multiplier = i128::from(fixed.damage_multiplier_millionths);
    if existing_multiplier <= 0 {
        return Err(SimError::NumericOverflow);
    }
    // Pinned damage_instance::add normalizes a same-type unit by the ratio of
    // the incoming and existing damage multipliers before adding amount/AP.
    let ratio_millionths = 1_000_000_i128
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(existing_multiplier))
        .ok_or(SimError::NumericOverflow)?;
    let scaled_addition = |value: i32| -> Result<i32, SimError> {
        i128::from(value)
            .checked_mul(ratio_millionths)
            .and_then(|value| value.checked_div(1_000_000))
            .and_then(|value| i32::try_from(value).ok())
            .ok_or(SimError::NumericOverflow)
    };
    fixed.amount_milli = fixed
        .amount_milli
        .checked_add(scaled_addition(rolled_amount_milli)?)
        .ok_or(SimError::NumericOverflow)?;
    fixed.armor_penetration_milli = fixed
        .armor_penetration_milli
        .checked_add(scaled_addition(rolled_armor_penetration_milli)?)
        .ok_or(SimError::NumericOverflow)?;
    // The pinned implementation interpolates toward the incoming damage
    // multiplier (1.0) for the merged armor multiplier.
    let interpolation_millionths = 1_000_000_i128
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(existing_multiplier + 1_000_000))
        .ok_or(SimError::NumericOverflow)?;
    fixed.armor_multiplier_millionths = i128::from(fixed.armor_multiplier_millionths)
        .checked_add(
            (1_000_000_i128 - i128::from(fixed.armor_multiplier_millionths))
                .checked_mul(interpolation_millionths)
                .and_then(|value| value.checked_div(1_000_000))
                .ok_or(SimError::NumericOverflow)?,
        )
        .and_then(|value| i32::try_from(value).ok())
        .ok_or(SimError::NumericOverflow)?;
    Ok(())
}
