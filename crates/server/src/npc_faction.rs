//! Runtime admission and replication helpers for NPC factions.

use cdda_content::{FactionRegistry, FactionRelationFlagsDefinition};
use cdda_protocol::{
    ActorId, FactionFoodSupplyV1, FactionRelationFlagsV1, FactionRelationshipV1, FactionStateV1,
    FactionTemplateV1, NpcSnapshotV1, PLAYER_FACTION_ID, WorldSnapshotV1, faction_catalog_is_valid,
};

pub(crate) fn runtime_npc_factions(
    registry: &FactionRegistry,
) -> Result<(Vec<FactionTemplateV1>, Vec<FactionStateV1>), Box<dyn std::error::Error>> {
    let templates = registry
        .iter()
        .filter(|(_, definition)| definition.unsupported_fields.is_empty())
        .map(|(faction_id, definition)| FactionTemplateV1 {
            faction_id: faction_id.to_owned(),
            name: definition.name.clone(),
            description: definition.description.clone(),
            likes_u: definition.likes_u,
            respects_u: definition.respects_u,
            trusts_u: definition.trusts_u,
            known_by_u: definition.known_by_u,
            size: definition.size,
            power: definition.power,
            wealth: definition.wealth,
            food_supply: definition
                .food_supply
                .iter()
                .map(|entry| FactionFoodSupplyV1 {
                    expires_at_turn: entry.expires_at_turn,
                    calories: entry.calories,
                    vitamins: entry.vitamins.clone(),
                })
                .collect(),
            consumes_food: definition.consumes_food,
            lone_wolf_faction: definition.lone_wolf_faction,
            limited_area_claim: definition.limited_area_claim,
            currency_id: definition.currency_id.clone(),
            relations: definition
                .relations
                .iter()
                .map(|(target_faction_id, flags)| FactionRelationshipV1 {
                    target_faction_id: target_faction_id.clone(),
                    flags: runtime_relation_flags(*flags),
                })
                .collect(),
            monster_faction_id: definition.monster_faction_id.clone(),
        })
        .collect::<Vec<_>>();
    let states = templates
        .iter()
        .map(FactionStateV1::from_template)
        .collect::<Vec<_>>();
    if templates.is_empty() || !faction_catalog_is_valid(&templates, &states) {
        return Err("pinned content has no valid supported NPC faction family".into());
    }
    Ok((templates, states))
}

pub(crate) fn visible_npc_faction(
    snapshot: &WorldSnapshotV1,
    npc: &NpcSnapshotV1,
    controlled_actor_id: ActorId,
) -> (String, bool) {
    let faction_name = snapshot
        .faction_templates
        .iter()
        .find(|template| template.faction_id == npc.faction_id)
        .map(|template| template.name.clone())
        .unwrap_or_else(|| npc.faction_id.clone());
    let faction_hostile = snapshot
        .factions
        .iter()
        .find(|faction| faction.faction_id == npc.faction_id)
        .is_some_and(|faction| faction.relation_to(PLAYER_FACTION_ID).kill_on_sight);
    let actor_hostile = snapshot
        .actors
        .iter()
        .find(|actor| actor.id == controlled_actor_id)
        .and_then(|actor| {
            actor
                .faction_standings
                .iter()
                .find(|standing| standing.faction_id == npc.faction_id)
        })
        .is_some_and(|standing| standing.likes_u < -10);
    (
        faction_name,
        npc.attitude == 10 || faction_hostile || actor_hostile,
    )
}

const fn runtime_relation_flags(flags: FactionRelationFlagsDefinition) -> FactionRelationFlagsV1 {
    FactionRelationFlagsV1 {
        kill_on_sight: flags.kill_on_sight,
        watch_your_back: flags.watch_your_back,
        share_my_stuff: flags.share_my_stuff,
        share_public_goods: flags.share_public_goods,
        guard_your_stuff: flags.guard_your_stuff,
        lets_you_in: flags.lets_you_in,
        defends_your_space: flags.defends_your_space,
        knows_your_voice: flags.knows_your_voice,
    }
}
