//! Canonical NPC faction registration and relationship policy.

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorFactionStandingV1, ActorId, FactionStateV1, FactionTemplateV1,
    NpcId, PLAYER_FACTION_ID, SimTick, faction_catalog_is_valid, opinion_is_valid,
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
        let initial_standings = states
            .iter()
            .map(ActorFactionStandingV1::from_faction)
            .map(|standing| (standing.faction_id.clone(), standing))
            .collect::<std::collections::BTreeMap<_, _>>();
        self.faction_templates = templates
            .into_iter()
            .map(|template| (template.faction_id.clone(), template))
            .collect();
        self.factions = states
            .into_iter()
            .map(|state| (state.faction_id.clone(), state))
            .collect();
        for actor in self.actors.values_mut() {
            actor.faction_standings.clone_from(&initial_standings);
        }
        Ok(())
    }

    pub(super) fn initial_actor_faction_standings(
        &self,
    ) -> std::collections::BTreeMap<String, ActorFactionStandingV1> {
        self.factions
            .values()
            .map(ActorFactionStandingV1::from_faction)
            .map(|standing| (standing.faction_id.clone(), standing))
            .collect()
    }

    pub(super) fn npc_is_hostile_to_actor(&self, npc: &Npc, actor_id: ActorId) -> bool {
        npc.attitude == 10
            || self
                .factions
                .get(&npc.faction_id)
                .is_some_and(|faction| faction.relation_to(PLAYER_FACTION_ID).kill_on_sight)
            || self
                .actors
                .get(&actor_id)
                .and_then(|actor| actor.faction_standings.get(&npc.faction_id))
                .is_some_and(|standing| standing.likes_u < -10)
    }

    pub(super) fn npc_is_hostile_to_any_actor(&self, npc: &Npc) -> bool {
        self.actors
            .keys()
            .copied()
            .any(|actor_id| self.npc_is_hostile_to_actor(npc, actor_id))
    }

    pub(super) fn npc_will_talk_to_actor(&self, npc: &Npc, actor_id: ActorId) -> bool {
        !matches!(npc.attitude, 10 | 11 | 17) && !self.npc_is_hostile_to_actor(npc, actor_id)
    }

    /// Pinned `npc::on_attacked`/`make_angry`, adapted so each authoritative
    /// player actor owns the opinion slot that upstream stores as `op_of_u`.
    pub(super) fn npc_on_attacked_by_actor(
        &mut self,
        npc_id: NpcId,
        actor_id: ActorId,
    ) -> Result<(), SimError> {
        const MUTINY_FACTION_ID: &str = "amf";
        const FLEE_ATTITUDE: i32 = 17;
        const KILL_ATTITUDE: i32 = 10;
        const FLEE_EFFECT_ID: &str = "npc_flee_player";
        const FLEE_SECONDS: u64 = 24 * 60 * 60;

        let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
        let faction_hostile = self.npc_is_hostile_to_actor(npc, actor_id);
        if npc.hp <= 0 || matches!(npc.attitude, 10 | 11 | 17) || faction_hostile {
            return Ok(());
        }

        let mut npc = npc.clone();
        let mut actor = self
            .actors
            .get(&actor_id)
            .cloned()
            .ok_or(SimError::UnknownActor)?;
        if npc.faction_id == PLAYER_FACTION_ID {
            if !self.factions.contains_key(MUTINY_FACTION_ID) {
                return Err(SimError::InvalidNpcFaction);
            }
            let followers = actor
                .faction_standings
                .get_mut(PLAYER_FACTION_ID)
                .ok_or(SimError::InvalidNpcFaction)?;
            let mistreated = followers.likes_u < -10;
            let respect_delta = mistreated.then_some(followers.respects_u / 10);
            let anger_delta = mistreated.then_some(followers.likes_u / 10);
            followers.likes_u = (followers.likes_u / 2)
                .checked_add(10)
                .ok_or(SimError::NumericOverflow)?
                .max(0);
            followers.respects_u = followers
                .respects_u
                .checked_sub(5)
                .ok_or(SimError::NumericOverflow)?;
            followers.trusts_u = followers
                .trusts_u
                .checked_sub(5)
                .ok_or(SimError::NumericOverflow)?;
            let opinion = npc.social.entry(actor_id).or_default();
            if let Some(respect_delta) = respect_delta {
                opinion.trust = opinion
                    .trust
                    .checked_add(respect_delta)
                    .ok_or(SimError::NumericOverflow)?;
            }
            if let Some(anger_delta) = anger_delta {
                opinion.anger = opinion
                    .anger
                    .checked_add(anger_delta)
                    .ok_or(SimError::NumericOverflow)?;
            }
            if !opinion_is_valid(opinion) {
                return Err(SimError::NumericOverflow);
            }
            npc.set_faction(MUTINY_FACTION_ID.to_owned());
        }

        let faction_id = npc.faction_id.clone();
        if faction_id != MUTINY_FACTION_ID
            && self
                .faction_templates
                .get(&faction_id)
                .is_some_and(|template| !template.lone_wolf_faction)
        {
            let faction = actor
                .faction_standings
                .get_mut(&faction_id)
                .ok_or(SimError::InvalidNpcFaction)?;
            faction.likes_u = faction
                .likes_u
                .checked_sub(5)
                .ok_or(SimError::NumericOverflow)?
                .min(-15);
            faction.respects_u = faction
                .respects_u
                .checked_sub(5)
                .ok_or(SimError::NumericOverflow)?
                .min(-15);
            faction.trusts_u = faction
                .trusts_u
                .checked_sub(5)
                .ok_or(SimError::NumericOverflow)?
                .min(-15);
        }

        let expires_at_tick = self
            .tick
            .0
            .checked_add(
                FLEE_SECONDS
                    .checked_mul(SimTick::HZ)
                    .ok_or(SimError::NumericOverflow)?,
            )
            .map(SimTick)
            .ok_or(SimError::NumericOverflow)?;
        let opinion = npc.social.entry(actor_id).or_default();
        let flee_threshold = 10_i32
            .checked_add(i32::from(npc.personality.aggression))
            .and_then(|value| value.checked_add(i32::from(npc.personality.bravery)))
            .ok_or(SimError::NumericOverflow)?;
        npc.attitude = if opinion.fear > flee_threshold {
            FLEE_ATTITUDE
        } else {
            KILL_ATTITUDE
        };
        npc.hit_by_player = true;
        if npc.attitude == FLEE_ATTITUDE
            && !npc
                .effects
                .iter()
                .any(|effect| effect.effect_id == FLEE_EFFECT_ID)
        {
            npc.effects.push(ActorEffectSnapshotV1 {
                effect_id: FLEE_EFFECT_ID.to_owned(),
                body_part_id: None,
                intensity: 1,
                expires_at_tick,
                modifiers: Default::default(),
            });
            npc.effects.sort_by(|left, right| {
                (&left.effect_id, &left.body_part_id).cmp(&(&right.effect_id, &right.body_part_id))
            });
        }
        self.actors.insert(actor_id, actor);
        self.npcs.insert(npc_id, npc);
        Ok(())
    }
}
