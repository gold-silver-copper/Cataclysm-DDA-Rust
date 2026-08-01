//! Authoritative ordinary NPC scheduling, pathing, and survivor combat.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use cdda_protocol::{
    ActorEffectSnapshotV1, ActorId, InteractionContextV1, NpcId, SimTick, WorldEvent,
    WorldEventKind, WorldPosition,
};
use rand_core::Rng;

use super::{
    ACTOR_ACTION_THRESHOLD, BookStudyInterruptionReason, ConstructionInterruptionReason,
    DisassemblyInterruptionReason, SimError, WakeReason, WorldState, horizontally_adjacent,
    ranged_distance,
};

const NPC_ATTITUDE_NULL: i32 = 0;
const NPC_ATTITUDE_TALK: i32 = 1;
const NPC_ATTITUDE_FOLLOW: i32 = 3;
const NPC_ATTITUDE_WAIT: i32 = 6;
const NPC_ATTITUDE_KILL: i32 = 10;
const NPC_ATTITUDE_FLEE: i32 = 11;
const NPC_ATTITUDE_FLEE_TEMPORARY: i32 = 17;
const NPC_FLEE_EFFECT_ID: &str = "npc_flee_player";
const NPC_FLEE_DURATION_SECONDS: u64 = 24 * 60 * 60;
const MAX_NPC_ROUTE_DISTANCE: u32 = 60;
const NPC_FOLLOW_DISTANCE: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NpcTurnBehavior {
    Pause,
    Talk,
    Follow,
    Attack,
    Flee,
}

impl WorldState {
    pub(super) fn advance_npcs(&mut self, events: &mut Vec<WorldEvent>) -> Result<(), SimError> {
        let npc_ids = self.npcs.keys().copied().collect::<Vec<_>>();
        for npc_id in npc_ids {
            self.advance_npc_flee_state(npc_id)?;
            let Some(npc) = self.npcs.get(&npc_id) else {
                continue;
            };
            if npc.hp <= 0 {
                continue;
            }
            let gained_action_points = i64::from(super::combat::npc_effective_speed(npc));
            let npc = self.npcs.get_mut(&npc_id).ok_or(SimError::UnknownNpc)?;
            npc.action_points = npc
                .action_points
                .checked_add(gained_action_points)
                .ok_or(SimError::NumericOverflow)?;
            let mut turn_sequence = 0_u64;
            while self.npcs.get(&npc_id).is_some_and(|npc| {
                npc.hp > 0 && npc.action_points >= i64::from(ACTOR_ACTION_THRESHOLD)
            }) {
                let cost = self.take_npc_turn(npc_id, turn_sequence, events)?;
                turn_sequence = turn_sequence
                    .checked_add(1)
                    .ok_or(SimError::NumericOverflow)?;
                let Some(npc) = self.npcs.get_mut(&npc_id) else {
                    break;
                };
                npc.action_points = npc
                    .action_points
                    .checked_sub(cost.max(1))
                    .ok_or(SimError::NumericOverflow)?;
            }
        }
        Ok(())
    }

    fn advance_npc_flee_state(&mut self, npc_id: NpcId) -> Result<(), SimError> {
        let attitude = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?.attitude;
        if attitude == NPC_ATTITUDE_FLEE {
            let expires_at_tick = self
                .tick
                .0
                .checked_add(
                    NPC_FLEE_DURATION_SECONDS
                        .checked_mul(SimTick::HZ)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .map(SimTick)
                .ok_or(SimError::NumericOverflow)?;
            let npc = self.npcs.get_mut(&npc_id).ok_or(SimError::UnknownNpc)?;
            npc.attitude = NPC_ATTITUDE_FLEE_TEMPORARY;
            if !npc
                .effects
                .iter()
                .any(|effect| effect.effect_id == NPC_FLEE_EFFECT_ID)
            {
                npc.effects.push(ActorEffectSnapshotV1 {
                    effect_id: NPC_FLEE_EFFECT_ID.to_owned(),
                    body_part_id: None,
                    intensity: 1,
                    expires_at_tick,
                    modifiers: Default::default(),
                });
                npc.effects.sort_by(|left, right| {
                    (&left.effect_id, &left.body_part_id)
                        .cmp(&(&right.effect_id, &right.body_part_id))
                });
            }
        } else if attitude == NPC_ATTITUDE_FLEE_TEMPORARY
            && self.npcs.get(&npc_id).is_some_and(|npc| {
                !npc.effects
                    .iter()
                    .any(|effect| effect.effect_id == NPC_FLEE_EFFECT_ID)
            })
        {
            self.npcs
                .get_mut(&npc_id)
                .ok_or(SimError::UnknownNpc)?
                .attitude = NPC_ATTITUDE_NULL;
        }
        Ok(())
    }

    fn take_npc_turn(
        &mut self,
        npc_id: NpcId,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<i64, SimError> {
        if self.npc_turn_is_disabled(npc_id)? {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        }
        if self.npc_has_active_dialogue(npc_id) {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        }
        let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
        let attitude = npc.attitude;
        let mut behavior = match attitude {
            NPC_ATTITUDE_NULL | NPC_ATTITUDE_WAIT => NpcTurnBehavior::Pause,
            NPC_ATTITUDE_TALK => NpcTurnBehavior::Talk,
            NPC_ATTITUDE_FOLLOW => NpcTurnBehavior::Follow,
            NPC_ATTITUDE_KILL => NpcTurnBehavior::Attack,
            NPC_ATTITUDE_FLEE | NPC_ATTITUDE_FLEE_TEMPORARY => NpcTurnBehavior::Flee,
            _ => return Err(SimError::InvalidNpcDialogue),
        };
        let forms_hostile_attitude = !matches!(
            attitude,
            NPC_ATTITUDE_KILL | NPC_ATTITUDE_FLEE | NPC_ATTITUDE_FLEE_TEMPORARY
        ) && self.npc_is_hostile_to_any_actor(npc);
        let mut target = if forms_hostile_attitude {
            self.npc_actor_target(npc_id, NpcTurnBehavior::Attack)?
        } else if behavior == NpcTurnBehavior::Pause {
            None
        } else {
            self.npc_actor_target(npc_id, behavior)?
        };
        if forms_hostile_attitude {
            let Some((target_id, _)) = target else {
                return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
            };
            let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
            let fear = npc.social.get(&target_id).map_or(0, |opinion| opinion.fear);
            let flee_threshold = 10_i32
                .checked_add(i32::from(npc.personality.aggression))
                .and_then(|value| value.checked_add(i32::from(npc.personality.bravery)))
                .ok_or(SimError::NumericOverflow)?;
            behavior = if fear > flee_threshold {
                NpcTurnBehavior::Flee
            } else {
                NpcTurnBehavior::Attack
            };
        }
        if behavior == NpcTurnBehavior::Pause {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        }
        if target.is_none() {
            target = self.npc_actor_target(npc_id, behavior)?;
        }
        let Some((target_id, target_position)) = target else {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        };
        let npc_position = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?.position;
        let distance = ranged_distance(npc_position, target_position);
        match behavior {
            NpcTurnBehavior::Attack => {
                if horizontally_adjacent(npc_position, target_position) {
                    let action_cost = self.npc_melee_action_cost(npc_id)?;
                    self.npc_attack_actor(npc_id, target_id, turn_sequence, events)?;
                    return Ok(action_cost);
                }
                self.npc_approach(npc_id, npc_position, target_position, events)
            }
            NpcTurnBehavior::Talk => {
                // Multiplayer dialogue remains player-accepted: the NPC closes
                // to the command's adjacent interaction boundary, then waits.
                if distance <= 1 {
                    Ok(i64::from(ACTOR_ACTION_THRESHOLD))
                } else {
                    self.npc_approach(npc_id, npc_position, target_position, events)
                }
            }
            NpcTurnBehavior::Follow => {
                if distance <= NPC_FOLLOW_DISTANCE {
                    Ok(i64::from(ACTOR_ACTION_THRESHOLD))
                } else {
                    self.npc_approach(npc_id, npc_position, target_position, events)
                }
            }
            NpcTurnBehavior::Flee => {
                if distance <= 1 && self.npc_hp_percentage(npc_id)? > 30 {
                    let action_cost = self.npc_melee_action_cost(npc_id)?;
                    self.npc_attack_actor(npc_id, target_id, turn_sequence, events)?;
                    Ok(action_cost)
                } else {
                    self.npc_flee_step(
                        npc_id,
                        target_id,
                        turn_sequence,
                        npc_position,
                        target_position,
                        events,
                    )
                }
            }
            NpcTurnBehavior::Pause => Ok(i64::from(ACTOR_ACTION_THRESHOLD)),
        }
    }

    fn npc_turn_is_disabled(&self, npc_id: NpcId) -> Result<bool, SimError> {
        let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
        Ok(npc.effects.iter().any(|effect| {
            effect.expires_at_tick > self.tick
                && matches!(
                    effect.effect_id.as_str(),
                    "sleep" | "downed" | "fearparalyze" | "narcosis"
                )
        }))
    }

    fn npc_has_active_dialogue(&self, npc_id: NpcId) -> bool {
        let Some(npc) = self.npcs.get(&npc_id) else {
            return false;
        };
        self.actors.values().any(|actor| {
            actor.connected
                && actor.position.z == npc.position.z
                && ranged_distance(actor.position, npc.position) <= 1
                && actor.position != npc.position
                && actor
                    .pending_interaction
                    .as_ref()
                    .is_some_and(|interaction| {
                        matches!(
                            interaction.context,
                            InteractionContextV1::NpcDialogue { npc_id: target, .. } if target == npc_id
                        )
                    })
        })
    }

    fn npc_actor_target(
        &self,
        npc_id: NpcId,
        behavior: NpcTurnBehavior,
    ) -> Result<Option<(ActorId, WorldPosition)>, SimError> {
        let position = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?.position;
        let requires_connection =
            matches!(behavior, NpcTurnBehavior::Talk | NpcTurnBehavior::Follow);
        let requires_sight = matches!(
            behavior,
            NpcTurnBehavior::Talk | NpcTurnBehavior::Attack | NpcTurnBehavior::Flee
        );
        let mut selected = None;
        for actor in self.actors.values() {
            if actor.hp <= 0
                || actor.position.z != position.z
                || (requires_connection && !actor.connected)
                || ranged_distance(position, actor.position) > MAX_NPC_ROUTE_DISTANCE
                || (requires_sight && !self.npc_can_see_position(npc_id, actor.position)?)
                || (behavior == NpcTurnBehavior::Attack
                    && !self.npc_is_hostile_to_actor(
                        self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?,
                        actor.id,
                    ))
            {
                continue;
            }
            let key = (ranged_distance(position, actor.position), actor.id);
            if selected
                .as_ref()
                .is_none_or(|(best_key, _)| key < *best_key)
            {
                selected = Some((key, actor.position));
            }
        }
        Ok(selected.map(|((_, actor_id), position)| (actor_id, position)))
    }

    fn npc_can_see_position(&self, npc_id: NpcId, target: WorldPosition) -> Result<bool, SimError> {
        let npc = self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?;
        let origin = npc.position;
        let distance = ranged_distance(origin, target);
        if npc.effects.iter().any(|effect| {
            effect.expires_at_tick > self.tick
                && matches!(effect.effect_id.as_str(), "blind" | "no_sight")
        }) {
            return Ok(distance == 0);
        }
        if origin.z != target.z
            || distance > super::TERRAIN_MEMORY_RADIUS_TILES
            || !self.has_clear_shot(origin, target)
        {
            return Ok(false);
        }
        let natural_radius =
            u32::from(super::NaturalLightSnapshot::at_tick(self.tick).sight_radius);
        if distance <= natural_radius {
            return Ok(true);
        }
        Ok(self.active_light_sources().into_iter().any(|source| {
            ranged_distance(source.position, target) <= source.sight_radius
                && self.has_clear_shot(source.position, target)
        }))
    }

    fn npc_approach(
        &mut self,
        npc_id: NpcId,
        from: WorldPosition,
        target: WorldPosition,
        events: &mut Vec<WorldEvent>,
    ) -> Result<i64, SimError> {
        let Some(step) = self.npc_route_step(from, target)? else {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        };
        self.move_npc_step(npc_id, from, step, events)
    }

    fn npc_flee_step(
        &mut self,
        npc_id: NpcId,
        target_id: ActorId,
        turn_sequence: u64,
        from: WorldPosition,
        threat: WorldPosition,
        events: &mut Vec<WorldEvent>,
    ) -> Result<i64, SimError> {
        const NEIGHBORS: [(i8, i8); 8] = [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
            (1, 1),
        ];
        let mut selected = None;
        let mut tie_chance = 2_u32;
        let mut rng = self.named_rng(
            b"npc-flee-step",
            &[npc_id.as_u128(), target_id.as_u128()],
            turn_sequence,
        );
        for (dx, dy) in NEIGHBORS {
            let Some(next) = from.checked_offset(dx, dy, 0) else {
                continue;
            };
            if self.npc_route_tile_cost(next, next)?.is_none()
                || self.actor_at(next).is_some()
                || self.creature_at(next).is_some()
                || self.npc_at(next).is_some()
            {
                continue;
            }
            let Some(cost) = self.npc_step_action_cost(npc_id, from, next, dx, dy)? else {
                continue;
            };
            let distance = u64::from(next.x.abs_diff(threat.x))
                .checked_add(u64::from(next.y.abs_diff(threat.y)))
                .ok_or(SimError::NumericOverflow)?;
            let rating = i128::from(distance)
                .checked_mul(1_000)
                .and_then(|value| value.checked_div(i128::from(cost)))
                .ok_or(SimError::NumericOverflow)?;
            if selected.is_none_or(|(best, _)| rating > best) {
                selected = Some((rating, next));
                tie_chance = 2;
            } else if selected.is_some_and(|(best, _)| rating == best)
                && rng.next_u32() % tie_chance == 0
            {
                selected = Some((rating, next));
                tie_chance = tie_chance.checked_add(1).ok_or(SimError::NumericOverflow)?;
            }
        }
        let Some((_distance, next)) = selected else {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        };
        self.move_npc_step(npc_id, from, next, events)
    }

    fn move_npc_step(
        &mut self,
        npc_id: NpcId,
        from: WorldPosition,
        to: WorldPosition,
        events: &mut Vec<WorldEvent>,
    ) -> Result<i64, SimError> {
        if self.actor_at(to).is_some()
            || self.creature_at(to).is_some()
            || self.npc_at(to).is_some()
            || self.vehicle_blocks_actor_at(to)
        {
            return Ok(i64::from(ACTOR_ACTION_THRESHOLD));
        }
        let dx = i8::try_from(i64::from(to.x) - i64::from(from.x))
            .map_err(|_| SimError::NumericOverflow)?;
        let dy = i8::try_from(i64::from(to.y) - i64::from(from.y))
            .map_err(|_| SimError::NumericOverflow)?;
        let cost = match self.loaded_movement_action_cost(from, to, dx, dy)? {
            Some(cost) => cost,
            None if self.projected_open_terrain(to).is_some() => {
                self.perform_npc_open_terrain(npc_id, to, events)?;
                return self.npc_door_action_cost(npc_id);
            }
            None => return Ok(i64::from(ACTOR_ACTION_THRESHOLD)),
        };
        self.npcs
            .get_mut(&npc_id)
            .ok_or(SimError::UnknownNpc)?
            .position = to;
        events.push(self.make_event(WorldEventKind::NpcMoved { npc_id, from, to })?);
        Ok(cost)
    }

    fn npc_step_action_cost(
        &self,
        npc_id: NpcId,
        from: WorldPosition,
        to: WorldPosition,
        dx: i8,
        dy: i8,
    ) -> Result<Option<i64>, SimError> {
        if let Some(cost) = self.loaded_movement_action_cost(from, to, dx, dy)? {
            return Ok(Some(cost));
        }
        self.projected_open_terrain(to)
            .map(|_| self.npc_door_action_cost(npc_id))
            .transpose()
    }

    fn npc_door_action_cost(&self, npc_id: NpcId) -> Result<i64, SimError> {
        i64::from(super::combat::npc_effective_speed(
            self.npcs.get(&npc_id).ok_or(SimError::UnknownNpc)?,
        ))
        .checked_mul(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE)
        .ok_or(SimError::NumericOverflow)
    }

    fn perform_npc_open_terrain(
        &mut self,
        npc_id: NpcId,
        position: WorldPosition,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let (from, to, transformed) = self
            .projected_open_terrain(position)
            .ok_or(SimError::InvalidTerrain)?;
        let (coord, local) = position.chunk_and_local();
        let chunk = self
            .chunks
            .get_mut(&coord)
            .ok_or(SimError::InvalidTerrain)?;
        chunk.set_terrain(local, transformed)?;
        chunk.set_map_damage(local, 0)?;
        events.push(self.make_event(WorldEventKind::NpcOpenedTerrain {
            npc_id,
            position,
            from,
            to,
            sound: String::from("swish"),
            volume: 6,
        })?);
        Ok(())
    }

    fn npc_route_step(
        &self,
        from: WorldPosition,
        target: WorldPosition,
    ) -> Result<Option<WorldPosition>, SimError> {
        if from.z != target.z || ranged_distance(from, target) > MAX_NPC_ROUTE_DISTANCE {
            return Ok(None);
        }
        let maximum_cost = MAX_NPC_ROUTE_DISTANCE
            .checked_mul(20)
            .ok_or(SimError::NumericOverflow)?;
        let minimum_x = i64::from(from.x.min(target.x)) - 16;
        let maximum_x = i64::from(from.x.max(target.x)) + 16;
        let minimum_y = i64::from(from.y.min(target.y)) - 16;
        let maximum_y = i64::from(from.y.max(target.y)) + 16;
        let mut open = BinaryHeap::from([Reverse((0_u32, from))]);
        let mut costs = BTreeMap::from([(from, 0_u32)]);
        let mut parents = BTreeMap::new();
        let mut closed = BTreeSet::new();
        const NEIGHBORS: [(i8, i8); 8] = [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
            (1, 1),
        ];
        while let Some(Reverse((_score, current))) = open.pop() {
            if !closed.insert(current) {
                continue;
            }
            let current_cost = *costs.get(&current).ok_or(SimError::InvalidNpcDialogue)?;
            if current == target {
                let mut step = target;
                while let Some(parent) = parents.get(&step).copied() {
                    if parent == from {
                        return Ok(Some(step));
                    }
                    step = parent;
                }
                return Ok(None);
            }
            for (dx, dy) in NEIGHBORS {
                let Some(next) = current.checked_offset(dx, dy, 0) else {
                    continue;
                };
                if i64::from(next.x) < minimum_x
                    || i64::from(next.x) >= maximum_x
                    || i64::from(next.y) < minimum_y
                    || i64::from(next.y) >= maximum_y
                {
                    continue;
                }
                let Some(tile_cost) = self.npc_route_tile_cost(next, target)? else {
                    continue;
                };
                let next_cost = current_cost
                    .checked_add(tile_cost)
                    .and_then(|cost| cost.checked_add(u32::from(dx != 0 && dy != 0)))
                    .ok_or(SimError::NumericOverflow)?;
                if next_cost > maximum_cost
                    || costs
                        .get(&next)
                        .is_some_and(|existing| *existing <= next_cost)
                {
                    continue;
                }
                costs.insert(next, next_cost);
                parents.insert(next, current);
                let estimate = ranged_distance(next, target)
                    .checked_mul(2)
                    .and_then(|distance| distance.checked_add(next_cost))
                    .ok_or(SimError::NumericOverflow)?;
                open.push(Reverse((estimate, next)));
            }
        }
        Ok(None)
    }

    fn npc_route_tile_cost(
        &self,
        position: WorldPosition,
        target: WorldPosition,
    ) -> Result<Option<u32>, SimError> {
        if position != target
            && (self.actor_at(position).is_some()
                || self.creature_at(position).is_some()
                || self.npc_at(position).is_some())
        {
            return Ok(None);
        }
        if self.vehicle_blocks_actor_at(position)
            || self.fields_at(position).is_some_and(|fields| {
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
            })
        {
            return Ok(None);
        }
        if let Some(cost) = self.tile_movement_cost(position) {
            return u32::try_from(cost)
                .map(Some)
                .map_err(|_| SimError::NumericOverflow);
        }
        if self
            .projected_open_terrain(position)
            .is_some_and(|(_, _, opened)| {
                opened.move_cost > 0
                    && self
                        .chunks
                        .get(&position.chunk_and_local().0)
                        .and_then(|chunk| chunk.furniture(position.chunk_and_local().1))
                        .is_none_or(|furniture| furniture.move_cost_mod >= 0)
            })
        {
            return Ok(Some(4));
        }
        Ok(None)
    }

    fn npc_hp_percentage(&self, npc_id: NpcId) -> Result<i32, SimError> {
        let parts = &self
            .npcs
            .get(&npc_id)
            .ok_or(SimError::UnknownNpc)?
            .body_parts;
        let mut current = 0_i64;
        let mut maximum = 0_i64;
        for part in parts {
            let weight = match part.body_part_id.as_str() {
                "head" => 3_i64,
                "torso" => 2_i64,
                _ => 1_i64,
            };
            current = current
                .checked_add(
                    i64::from(part.current_hp)
                        .checked_mul(weight)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .ok_or(SimError::NumericOverflow)?;
            maximum = maximum
                .checked_add(
                    i64::from(part.maximum_hp)
                        .checked_mul(weight)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .ok_or(SimError::NumericOverflow)?;
        }
        if maximum <= 0 {
            return Err(SimError::InvalidActorAnatomy);
        }
        current
            .checked_mul(100)
            .and_then(|value| value.checked_div(maximum))
            .and_then(|value| i32::try_from(value).ok())
            .ok_or(SimError::NumericOverflow)
    }

    fn npc_attack_actor(
        &mut self,
        source: NpcId,
        target: ActorId,
        turn_sequence: u64,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let Some((spread, dodge_attempted)) =
            self.npc_actor_hit_spread(source, target, turn_sequence)?
        else {
            return Ok(());
        };
        self.charge_npc_melee_stamina(source)?;
        if dodge_attempted {
            self.consume_actor_dodge_attempt(target)?;
        }
        if spread < 0 {
            events.push(self.make_event(WorldEventKind::NpcMissedActor { source, target })?);
            return Ok(());
        }
        let damage = self.npc_melee_damage(source)?;
        let mut rng = self.named_rng(
            b"npc-melee",
            &[source.as_u128(), target.as_u128()],
            turn_sequence,
        );
        let (outcome, was_sleeping) = self.damage_actor(target, "bash", damage, &mut rng)?;
        events.push(self.make_event(WorldEventKind::ActorDamagedByNpc {
            source,
            target,
            body_part_id: outcome.body_part_id,
            amount: outcome.amount,
            remaining_part_hp: outcome.remaining_part_hp,
            remaining_hp: outcome.remaining_hp,
        })?);
        if outcome.amount > 0 {
            self.interrupt_craft(target, events)?;
            self.interrupt_book_study(target, BookStudyInterruptionReason::Damage, events)?;
            self.interrupt_disassembly(target, DisassemblyInterruptionReason::Damage, events)?;
            self.interrupt_construction(target, ConstructionInterruptionReason::Damage, events)?;
            if was_sleeping && outcome.remaining_hp > 0 {
                self.wake_actor(target, WakeReason::Damage, events)?;
            }
            if outcome.remaining_hp <= 0 {
                events.push(self.make_event(WorldEventKind::ActorKilledByNpc {
                    actor_id: target,
                    killer: source,
                })?);
            }
        }
        Ok(())
    }
}
