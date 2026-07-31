//! Canonical item use-action profiles shared by simulation and persistence.

use serde::{Deserialize, Serialize};

use super::{CraftItemPrototypeV1, MAX_ITEM_RAW_DAMAGE, valid_craft_item_prototype};

pub const MAX_ITEM_TRANSFORM_TYPES: usize = 65_536;
pub const MAX_ITEM_TRANSFORM_MOVES: u32 = 1_000_000;
pub const MAX_ITEM_PLACE_MONSTER_TYPES: usize = 65_536;
pub const MAX_ITEM_PLACE_MONSTER_SKILLS: usize = 256;
pub const MAX_ITEM_PLACE_MONSTER_MESSAGE_BYTES: usize = 16 * 1_024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemTransformTypeV1 {
    pub source_type_id: String,
    /// Complete target static state. The simulation retains stable identity,
    /// charges, damage, variables, and compatible nested pocket contents.
    pub target: Box<CraftItemPrototypeV1>,
    /// Charges that must be available before the action may begin.
    pub required_charges: u32,
    /// Charges consumed after successful conversion.
    pub consumed_charges: u32,
    pub move_cost_moves: u32,
}

/// Finalized `place_monster` use action for one item type. Pinned C++ stores
/// use functions in a map keyed by action type, so at most one such actor can
/// exist on an item definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ItemPlaceMonsterTypeV1 {
    pub source_type_id: String,
    /// Finalized translated item name used by authoritative prompts and
    /// activation feedback.
    pub source_display_name: String,
    /// `Character::invoke_item` removes successful SINGLE_USE deployables
    /// after the actor's fixed one-charge activation consumption.
    pub single_use: bool,
    /// Static item damage denominator used by pinned `monster::init_from_item`.
    pub maximum_raw_damage: u16,
    pub monster_type_id: String,
    pub friendly_message: String,
    pub hostile_message: String,
    pub difficulty: i32,
    pub move_cost_moves: u32,
    pub place_randomly: bool,
    pub is_pet: bool,
    pub required_charges: u32,
    /// Charges consumed by `activation_consume(1, ...)`: for the pinned
    /// legacy tool path this is the finalized `charges_per_use` value.
    pub activation_charges: u32,
    /// Strictly sorted unique identities, matching pinned C++
    /// `std::set<skill_id>` iteration and floating-point accumulation order.
    pub skills: Vec<String>,
}

#[must_use]
pub fn item_transform_catalog_is_valid(catalog: &[ItemTransformTypeV1]) -> bool {
    catalog.len() <= MAX_ITEM_TRANSFORM_TYPES
        && catalog
            .windows(2)
            .all(|pair| pair[0].source_type_id < pair[1].source_type_id)
        && catalog.iter().all(|profile| {
            valid_id(&profile.source_type_id)
                && profile.source_type_id != profile.target.type_id
                && valid_craft_item_prototype(&profile.target)
                && profile.target.powered_tool.is_none()
                && profile.required_charges <= i32::MAX as u32
                && profile.consumed_charges <= i32::MAX as u32
                && profile.move_cost_moves <= MAX_ITEM_TRANSFORM_MOVES
        })
}

#[must_use]
pub fn item_place_monster_catalog_is_valid(catalog: &[ItemPlaceMonsterTypeV1]) -> bool {
    catalog.len() <= MAX_ITEM_PLACE_MONSTER_TYPES
        && catalog
            .windows(2)
            .all(|pair| pair[0].source_type_id < pair[1].source_type_id)
        && catalog.iter().all(|profile| {
            valid_id(&profile.source_type_id)
                && valid_message(&profile.source_display_name)
                && !profile.source_display_name.is_empty()
                && matches!(profile.maximum_raw_damage, 0 | MAX_ITEM_RAW_DAMAGE)
                && valid_id(&profile.monster_type_id)
                && profile.move_cost_moves <= i32::MAX as u32
                && profile.required_charges <= i32::MAX as u32
                && profile.activation_charges <= i32::MAX as u32
                && valid_message(&profile.friendly_message)
                && valid_message(&profile.hostile_message)
                && profile.skills.len() <= MAX_ITEM_PLACE_MONSTER_SKILLS
                && profile.skills.iter().all(|skill| valid_id(skill))
                && profile.skills.windows(2).all(|pair| pair[0] < pair[1])
        })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

fn valid_message(message: &str) -> bool {
    message.len() <= MAX_ITEM_PLACE_MONSTER_MESSAGE_BYTES
        && !message
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
}
