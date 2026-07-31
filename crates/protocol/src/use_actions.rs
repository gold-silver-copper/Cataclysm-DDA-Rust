//! Canonical item use-action profiles shared by simulation and persistence.

use serde::{Deserialize, Serialize};

use super::{CraftItemPrototypeV1, valid_craft_item_prototype};

pub const MAX_ITEM_TRANSFORM_TYPES: usize = 65_536;
pub const MAX_ITEM_TRANSFORM_MOVES: u32 = 1_000_000;

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

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}
