//! Canonical NPC faction registration and relationship policy.

use cdda_protocol::{
    FactionStateV1, FactionTemplateV1, PLAYER_FACTION_ID, faction_catalog_is_valid,
};

use crate::{SimError, WorldState, npc_dialogue::Npc};

impl WorldState {
    pub fn register_npc_faction_catalog(
        &mut self,
        templates: Vec<FactionTemplateV1>,
        states: Vec<FactionStateV1>,
    ) -> Result<(), SimError> {
        if templates.is_empty()
            || !faction_catalog_is_valid(&templates, &states)
            || !self.faction_templates.is_empty()
            || !self.factions.is_empty()
            || !self.npc_templates.is_empty()
            || !self.npcs.is_empty()
        {
            return Err(SimError::InvalidNpcFaction);
        }
        self.faction_templates = templates
            .into_iter()
            .map(|template| (template.faction_id.clone(), template))
            .collect();
        self.factions = states
            .into_iter()
            .map(|state| (state.faction_id.clone(), state))
            .collect();
        Ok(())
    }

    pub(super) fn npc_is_hostile_to_player_faction(&self, npc: &Npc) -> bool {
        npc.attitude == 10
            || self
                .factions
                .get(&npc.faction_id)
                .is_some_and(|faction| faction.relation_to(PLAYER_FACTION_ID).kill_on_sight)
    }

    pub(super) fn npc_will_talk_to_player_faction(&self, npc: &Npc) -> bool {
        !matches!(npc.attitude, 10 | 11 | 17) && !self.npc_is_hostile_to_player_faction(npc)
    }
}
