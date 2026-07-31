use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{CharacterCreationStatsV1, MAX_ACTOR_BASE_STAT};

pub const ANATOMY_SCALE: i64 = 1_000_000;
pub const MAX_ANATOMY_PARTS: usize = 256;
pub const MAX_BODY_PART_ID_BYTES: usize = 512;
pub const MAX_BODY_PART_DEFERRED_FIELDS: usize = 128;
pub const MAX_BODY_PART_LIMB_TYPES: usize = 32;
pub const MAX_WEARABLE_ARMOR_TYPES: usize = 16_384;
pub const MAX_ARMOR_PORTIONS: usize = 256;
pub const MAX_ARMOR_DAMAGE_TYPES: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BodyPartHpModifiersV1 {
    pub strength_millionths: i64,
    pub dexterity_millionths: i64,
    pub intelligence_millionths: i64,
    pub perception_millionths: i64,
    pub health_millionths: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BodyPartPrototypeV1 {
    pub body_part_id: String,
    pub main_part_id: String,
    pub connected_to_id: String,
    pub opposite_part_id: String,
    pub vital: bool,
    pub hit_size_millionths: u64,
    pub hit_difficulty_millionths: i64,
    /// Sorted semantic categories used by generalized anatomy consumers.
    pub limb_types: Vec<String>,
    pub base_hp: i32,
    pub hp_modifiers: BodyPartHpModifiersV1,
    pub effects_on_hit: Vec<BodyPartOnHitEffectV1>,
    /// Sorted retained upstream fields owned by later anatomy extensions.
    pub deferred_fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BodyPartOnHitEffectV1 {
    pub effect_id: String,
    pub global: bool,
    /// Empty means every damage type.
    pub damage_type_id: String,
    pub damage_threshold_millionths: i64,
    pub scale_increment_millionths: i64,
    pub chance_percent: i32,
    pub chance_damage_scaling_millionths: i64,
    pub intensity: i32,
    pub intensity_damage_scaling_millionths: i64,
    pub max_intensity: i32,
    pub duration_turns: i32,
    pub duration_damage_scaling_millionths: i64,
    pub max_duration_turns: i32,
    pub deferred_fields: Vec<String>,
}

impl BodyPartPrototypeV1 {
    pub fn maximum_hp(&self, stats: CharacterCreationStatsV1, health: i32) -> Option<i32> {
        if !stats.is_valid() || !body_part_prototype_is_valid(self) {
            return None;
        }
        let modifiers = self.hp_modifiers;
        let scaled = i128::from(stats.strength)
            .checked_mul(i128::from(modifiers.strength_millionths))?
            .checked_add(
                i128::from(stats.dexterity)
                    .checked_mul(i128::from(modifiers.dexterity_millionths))?,
            )?
            .checked_add(
                i128::from(stats.intelligence)
                    .checked_mul(i128::from(modifiers.intelligence_millionths))?,
            )?
            .checked_add(
                i128::from(stats.perception)
                    .checked_mul(i128::from(modifiers.perception_millionths))?,
            )?
            .checked_add(
                i128::from(health).checked_mul(i128::from(modifiers.health_millionths))?,
            )?;
        let stat_hp = scaled.checked_div(i128::from(ANATOMY_SCALE))?;
        let maximum = i128::from(self.base_hp).checked_add(stat_hp)?.max(1);
        i32::try_from(maximum).ok()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AnatomyDefinitionV1 {
    pub anatomy_id: String,
    /// Source-ordered body parts. Hit selection observes this order at exact
    /// cumulative-weight boundaries.
    pub parts: Vec<BodyPartPrototypeV1>,
    pub deferred_fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorBodyPartSnapshotV1 {
    pub body_part_id: String,
    pub current_hp: i32,
    pub maximum_hp: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
pub struct ActorEffectSnapshotV1 {
    pub effect_id: String,
    /// `None` is a global effect; otherwise this is a human anatomy part ID.
    pub body_part_id: Option<String>,
    pub intensity: u32,
    pub expires_at_tick: crate::SimTick,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArmorMaterialProtectionV1 {
    /// Independent material-hit chance. A portion may be hit while one of its
    /// partial layers is missed.
    pub covered_by_material_percent: u8,
    /// Damage-type-keyed protection in thousandths of a damage point.
    pub protection_milli: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WearableArmorPortionV1 {
    pub covers: Vec<String>,
    pub coverage_percent: u8,
    pub encumbrance_minimum: u16,
    pub encumbrance_maximum: u16,
    /// Source-ordered materials; each layer makes its own coverage roll after
    /// the containing armor portion is hit.
    pub materials: Vec<ArmorMaterialProtectionV1>,
    pub deferred_fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WearableArmorTypeV1 {
    pub item_type_id: String,
    pub portions: Vec<WearableArmorPortionV1>,
    pub deferred_fields: Vec<String>,
}

#[must_use]
pub fn anatomy_definition_is_valid(anatomy: &AnatomyDefinitionV1) -> bool {
    valid_id(&anatomy.anatomy_id)
        && !anatomy.parts.is_empty()
        && anatomy.parts.len() <= MAX_ANATOMY_PARTS
        && sorted_unique_ids(&anatomy.deferred_fields, MAX_BODY_PART_DEFERRED_FIELDS)
        && anatomy.parts.iter().all(body_part_prototype_is_valid)
        && {
            let ids = anatomy
                .parts
                .iter()
                .map(|part| part.body_part_id.as_str())
                .collect::<BTreeSet<_>>();
            ids.len() == anatomy.parts.len()
                && anatomy.parts.iter().all(|part| {
                    ids.contains(part.main_part_id.as_str())
                        && ids.contains(part.connected_to_id.as_str())
                        && ids.contains(part.opposite_part_id.as_str())
                })
                && anatomy.parts.iter().any(|part| part.vital)
        }
}

#[must_use]
pub fn body_part_prototype_is_valid(part: &BodyPartPrototypeV1) -> bool {
    valid_id(&part.body_part_id)
        && valid_id(&part.main_part_id)
        && valid_id(&part.connected_to_id)
        && valid_id(&part.opposite_part_id)
        && part.hit_size_millionths > 0
        && part.hit_difficulty_millionths >= 0
        && !part.limb_types.is_empty()
        && sorted_unique_ids(&part.limb_types, MAX_BODY_PART_LIMB_TYPES)
        && part.base_hp > 0
        && part.effects_on_hit.len() <= 256
        && part.effects_on_hit.iter().all(|effect| {
            valid_id(&effect.effect_id)
                && (effect.damage_type_id.is_empty() || valid_id(&effect.damage_type_id))
                && effect.damage_threshold_millionths >= 0
                && effect.scale_increment_millionths > 0
                && effect.chance_percent >= 0
                && effect.intensity > 0
                && effect.max_intensity > 0
                && effect.duration_turns > 0
                && effect.max_duration_turns > 0
                && sorted_unique_ids(&effect.deferred_fields, MAX_BODY_PART_DEFERRED_FIELDS)
        })
        && sorted_unique_ids(&part.deferred_fields, MAX_BODY_PART_DEFERRED_FIELDS)
        && [
            part.hp_modifiers.strength_millionths,
            part.hp_modifiers.dexterity_millionths,
            part.hp_modifiers.intelligence_millionths,
            part.hp_modifiers.perception_millionths,
            part.hp_modifiers.health_millionths,
        ]
        .into_iter()
        .all(|modifier| modifier.unsigned_abs() <= 1_000 * ANATOMY_SCALE as u64)
}

#[must_use]
pub fn actor_effects_are_valid(
    anatomy: &AnatomyDefinitionV1,
    effects: &[ActorEffectSnapshotV1],
    current_tick: crate::SimTick,
) -> bool {
    effects.len() <= 1_024
        && effects.windows(2).all(|pair| {
            (&pair[0].effect_id, &pair[0].body_part_id)
                < (&pair[1].effect_id, &pair[1].body_part_id)
        })
        && effects.iter().all(|effect| {
            valid_id(&effect.effect_id)
                && effect.intensity > 0
                && effect.expires_at_tick > current_tick
                && effect.body_part_id.as_ref().is_none_or(|part_id| {
                    anatomy
                        .parts
                        .iter()
                        .any(|part| part.body_part_id == *part_id)
                })
        })
}

#[must_use]
pub fn actor_body_parts_are_valid(
    anatomy: &AnatomyDefinitionV1,
    parts: &[ActorBodyPartSnapshotV1],
) -> bool {
    anatomy_definition_is_valid(anatomy)
        && parts.len() == anatomy.parts.len()
        && parts.iter().zip(&anatomy.parts).all(|(state, prototype)| {
            state.body_part_id == prototype.body_part_id
                && state.maximum_hp > 0
                && state.current_hp <= state.maximum_hp
                && state.current_hp >= 0
        })
}

#[must_use]
pub fn actor_body_part_summary_hp(
    anatomy: &AnatomyDefinitionV1,
    parts: &[ActorBodyPartSnapshotV1],
) -> Option<i32> {
    actor_body_parts_are_valid(anatomy, parts).then(|| {
        anatomy
            .parts
            .iter()
            .zip(parts)
            .filter(|(prototype, _)| prototype.vital)
            .map(|(_, state)| state.current_hp)
            .min()
            .unwrap_or(0)
    })
}

#[must_use]
pub fn wearable_armor_type_is_valid(armor: &WearableArmorTypeV1) -> bool {
    valid_id(&armor.item_type_id)
        && !armor.portions.is_empty()
        && armor.portions.len() <= MAX_ARMOR_PORTIONS
        && sorted_unique_ids(&armor.deferred_fields, MAX_BODY_PART_DEFERRED_FIELDS)
        && armor.portions.iter().all(|portion| {
            !portion.covers.is_empty()
                && portion.covers.len() <= MAX_ANATOMY_PARTS
                && portion.covers.windows(2).all(|pair| pair[0] < pair[1])
                && portion.covers.iter().all(|id| valid_id(id))
                && portion.coverage_percent <= 100
                && portion.encumbrance_minimum <= portion.encumbrance_maximum
                && !portion.materials.is_empty()
                && portion.materials.len() <= 64
                && portion.materials.iter().all(|material| {
                    (1..=100).contains(&material.covered_by_material_percent)
                        && material.protection_milli.len() <= MAX_ARMOR_DAMAGE_TYPES
                        && material
                            .protection_milli
                            .keys()
                            .all(|damage_type| valid_id(damage_type))
                })
                && sorted_unique_ids(&portion.deferred_fields, MAX_BODY_PART_DEFERRED_FIELDS)
        })
}

#[must_use]
pub fn wearable_armor_catalog_is_valid(catalog: &[WearableArmorTypeV1]) -> bool {
    catalog.len() <= MAX_WEARABLE_ARMOR_TYPES
        && catalog
            .windows(2)
            .all(|pair| pair[0].item_type_id < pair[1].item_type_id)
        && catalog.iter().all(wearable_armor_type_is_valid)
}

fn sorted_unique_ids(values: &[String], maximum: usize) -> bool {
    values.len() <= maximum
        && values.windows(2).all(|pair| pair[0] < pair[1])
        && values.iter().all(|value| valid_id(value))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_BODY_PART_ID_BYTES && !id.chars().any(char::is_control)
}

const _: () = assert!(MAX_ACTOR_BASE_STAT <= u16::MAX);
