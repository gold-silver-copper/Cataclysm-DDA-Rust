//! Canonical NPC faction templates, mutable state, and relationships.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const PLAYER_FACTION_ID: &str = "your_followers";
pub const NO_FACTION_ID: &str = "no_faction";
pub const MAX_FACTION_TEMPLATES: usize = 4_096;
pub const MAX_ACTOR_FACTION_STANDINGS: usize = MAX_FACTION_TEMPLATES;
pub const MAX_FACTION_RELATIONS: usize = 4_096;
pub const MAX_FACTION_FOOD_SUPPLY_ENTRIES: usize = 4_096;
pub const MAX_FACTION_ID_BYTES: usize = 512;
pub const MAX_FACTION_NAME_BYTES: usize = 1_024;
pub const MAX_FACTION_DESCRIPTION_BYTES: usize = 16_384;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactionRelationFlagsV1 {
    pub kill_on_sight: bool,
    pub watch_your_back: bool,
    pub share_my_stuff: bool,
    pub share_public_goods: bool,
    pub guard_your_stuff: bool,
    pub lets_you_in: bool,
    pub defends_your_space: bool,
    pub knows_your_voice: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactionRelationshipV1 {
    pub target_faction_id: String,
    pub flags: FactionRelationFlagsV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactionFoodSupplyV1 {
    pub expires_at_turn: i64,
    pub calories: i64,
    pub vitamins: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactionTemplateV1 {
    pub faction_id: String,
    pub name: String,
    pub description: String,
    pub likes_u: i32,
    pub respects_u: i32,
    pub trusts_u: i32,
    pub known_by_u: bool,
    pub size: i32,
    pub power: i32,
    pub wealth: i32,
    pub food_supply: Vec<FactionFoodSupplyV1>,
    pub consumes_food: bool,
    pub lone_wolf_faction: bool,
    pub limited_area_claim: bool,
    pub currency_id: String,
    pub relations: Vec<FactionRelationshipV1>,
    pub monster_faction_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FactionStateV1 {
    pub faction_id: String,
    pub likes_u: i32,
    pub respects_u: i32,
    pub trusts_u: i32,
    pub known_by_u: bool,
    pub size: i32,
    pub power: i32,
    pub wealth: i32,
    pub food_supply: Vec<FactionFoodSupplyV1>,
    /// `None` is upstream's default "ask" state.
    pub steal_persist: Option<bool>,
    pub relations: Vec<FactionRelationshipV1>,
}

/// One player's standing with an NPC faction. Upstream stores these values on
/// the faction because it has one avatar; multiplayer keeps the same values
/// and formulas per authoritative actor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActorFactionStandingV1 {
    pub faction_id: String,
    pub likes_u: i32,
    pub respects_u: i32,
    pub trusts_u: i32,
}

impl ActorFactionStandingV1 {
    #[must_use]
    pub fn from_faction(state: &FactionStateV1) -> Self {
        Self {
            faction_id: state.faction_id.clone(),
            likes_u: state.likes_u,
            respects_u: state.respects_u,
            trusts_u: state.trusts_u,
        }
    }
}

#[must_use]
pub fn actor_faction_standings_are_valid(
    standings: &[ActorFactionStandingV1],
    faction_ids: &BTreeSet<&str>,
) -> bool {
    standings.len() == faction_ids.len()
        && standings.len() <= MAX_ACTOR_FACTION_STANDINGS
        && standings
            .windows(2)
            .all(|pair| pair[0].faction_id < pair[1].faction_id)
        && standings
            .iter()
            .all(|standing| faction_ids.contains(standing.faction_id.as_str()))
}

impl FactionStateV1 {
    #[must_use]
    pub fn from_template(template: &FactionTemplateV1) -> Self {
        Self {
            faction_id: template.faction_id.clone(),
            likes_u: template.likes_u,
            respects_u: template.respects_u,
            trusts_u: template.trusts_u,
            known_by_u: template.known_by_u,
            size: template.size,
            power: template.power,
            wealth: template.wealth,
            food_supply: template.food_supply.clone(),
            steal_persist: None,
            relations: template.relations.clone(),
        }
    }

    #[must_use]
    pub fn relation_to(&self, target_faction_id: &str) -> FactionRelationFlagsV1 {
        self.relations
            .binary_search_by(|relation| relation.target_faction_id.as_str().cmp(target_faction_id))
            .ok()
            .map_or_else(FactionRelationFlagsV1::default, |index| {
                self.relations[index].flags
            })
    }
}

#[must_use]
pub fn faction_catalog_is_valid(
    templates: &[FactionTemplateV1],
    states: &[FactionStateV1],
) -> bool {
    if templates.len() > MAX_FACTION_TEMPLATES
        || templates.len() != states.len()
        || !templates
            .windows(2)
            .all(|pair| pair[0].faction_id < pair[1].faction_id)
        || !states
            .windows(2)
            .all(|pair| pair[0].faction_id < pair[1].faction_id)
    {
        return false;
    }
    let ids = templates
        .iter()
        .map(|template| template.faction_id.as_str())
        .collect::<BTreeSet<_>>();
    templates.iter().all(faction_template_is_valid)
        && states.iter().all(|state| {
            ids.contains(state.faction_id.as_str())
                && food_supply_is_valid(&state.food_supply)
                && relations_are_valid(&state.relations)
        })
}

#[must_use]
pub fn faction_template_is_valid(template: &FactionTemplateV1) -> bool {
    valid_id(&template.faction_id)
        && valid_text(&template.name, MAX_FACTION_NAME_BYTES)
        && valid_text(&template.description, MAX_FACTION_DESCRIPTION_BYTES)
        && optional_id_is_valid(&template.currency_id)
        && valid_id(&template.monster_faction_id)
        && food_supply_is_valid(&template.food_supply)
        && relations_are_valid(&template.relations)
}

fn food_supply_is_valid(food_supply: &[FactionFoodSupplyV1]) -> bool {
    food_supply.len() <= MAX_FACTION_FOOD_SUPPLY_ENTRIES
        && food_supply.iter().all(|entry| {
            entry.expires_at_turn >= 0
                && entry.calories >= 0
                && entry
                    .vitamins
                    .iter()
                    .all(|(id, amount)| valid_id(id) && *amount >= 0)
        })
}

fn relations_are_valid(relations: &[FactionRelationshipV1]) -> bool {
    relations.len() <= MAX_FACTION_RELATIONS
        && relations
            .windows(2)
            .all(|pair| pair[0].target_faction_id < pair[1].target_faction_id)
        && relations
            .iter()
            .all(|relation| valid_id(&relation.target_faction_id))
}

fn optional_id_is_valid(id: &str) -> bool {
    id.is_empty() || valid_id(id)
}

fn valid_id(id: &str) -> bool {
    valid_text(id, MAX_FACTION_ID_BYTES)
}

fn valid_text(text: &str, maximum: usize) -> bool {
    !text.is_empty()
        && text.len() <= maximum
        && !text
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
}
