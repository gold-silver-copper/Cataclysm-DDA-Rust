use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ActorId, MissionDefinitionV1, MissionGoalV1, MissionId, MissionSnapshotV1, MissionStatusV1,
    NpcId, PLAYER_FACTION_ID, VehicleSnapshotV1, WorldEvent, WorldEventKind, WorldPosition,
};

use crate::{Actor, SimError, WorldState};

#[derive(Clone, Debug)]
pub(super) enum MissionOperation {
    Assign {
        mission_type_id: String,
        mission_id: Option<MissionId>,
        origin_npc_id: Option<NpcId>,
    },
    Finish {
        mission_type_id: String,
        mission_id: Option<MissionId>,
        success: bool,
    },
}

#[derive(Clone, Debug)]
pub(super) enum MissionLifecycleEvent {
    Assigned {
        mission_id: MissionId,
        mission_type_id: String,
    },
    Finished {
        mission_id: MissionId,
        mission_type_id: String,
        success: bool,
    },
}

impl WorldState {
    pub fn register_mission_catalog(
        &mut self,
        definitions: Vec<MissionDefinitionV1>,
    ) -> Result<(), SimError> {
        let mission_ids = definitions
            .iter()
            .map(|definition| definition.mission_type_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if self.tick.0 != 0
            || !self.actors.is_empty()
            || !self.mission_definitions.is_empty()
            || !cdda_protocol::mission_catalog_is_valid(&definitions)
            || !crate::eocs::mission_phase_effects_are_actor_only(
                definitions.iter(),
                &self.actor_anatomy,
            )
            || !crate::eocs::mission_references_are_valid_for_ids(
                self.eoc_definitions.values(),
                self.dialogue_topics.values(),
                definitions.iter(),
                &mission_ids,
            )
        {
            return Err(SimError::InvalidMission);
        }
        self.mission_definitions = definitions
            .into_iter()
            .map(|definition| (definition.mission_type_id.clone(), definition))
            .collect();
        Ok(())
    }

    pub(super) fn commit_mission_operations(
        &mut self,
        actor_id: ActorId,
        operations: Vec<MissionOperation>,
    ) -> Result<Vec<Option<MissionLifecycleEvent>>, SimError> {
        if operations.is_empty() {
            return Ok(Vec::new());
        }
        let mut actor = self
            .actors
            .get(&actor_id)
            .cloned()
            .ok_or(SimError::UnknownActor)?;
        let mut allocator = self.allocator.clone();
        let mut ground_items = None;
        let mut vehicles = None;
        let mut npc_updates = BTreeMap::new();
        let mut lifecycle = Vec::with_capacity(operations.len());
        for operation in operations {
            match operation {
                MissionOperation::Assign {
                    mission_type_id,
                    mission_id,
                    origin_npc_id,
                } => {
                    let definition = self
                        .mission_definitions
                        .get(&mission_type_id)
                        .ok_or(SimError::InvalidMission)?;
                    let active_count = actor
                        .missions
                        .values()
                        .filter(|mission| mission.status == MissionStatusV1::InProgress)
                        .count();
                    if active_count >= cdda_protocol::MAX_ACTOR_MISSIONS {
                        return Err(SimError::InvalidMission);
                    }
                    prune_finished_mission_history(&mut actor)?;
                    let kill_count_to_reach = match &definition.goal {
                        MissionGoalV1::KillMonsterType { count, .. }
                        | MissionGoalV1::KillMonsterSpecies { count, .. } => Some(
                            current_kill_count(&definition.goal, &actor.creature_kill_counts)?
                                .checked_add(u64::from(*count))
                                .ok_or(SimError::NumericOverflow)?,
                        ),
                        MissionGoalV1::Null | MissionGoalV1::FindItem { .. } => None,
                    };
                    let mission_id = match (mission_id, origin_npc_id) {
                        (None, None) => allocator.allocate_mission()?,
                        (Some(mission_id), Some(npc_id)) => {
                            if actor.missions.contains_key(&mission_id) {
                                return Err(SimError::InvalidMission);
                            }
                            let mut npc = npc_updates
                                .remove(&npc_id)
                                .or_else(|| self.npcs.get(&npc_id).cloned())
                                .ok_or(SimError::InvalidMission)?;
                            if npc.mission_offers.remove(&mission_id).as_deref()
                                != Some(mission_type_id.as_str())
                            {
                                return Err(SimError::InvalidMission);
                            }
                            npc_updates.insert(npc_id, npc);
                            mission_id
                        }
                        _ => return Err(SimError::InvalidMission),
                    };
                    actor.missions.insert(
                        mission_id,
                        MissionSnapshotV1 {
                            mission_id,
                            mission_type_id: mission_type_id.clone(),
                            origin_npc_id,
                            assigned_at_tick: self.tick,
                            finished_at_tick: None,
                            status: MissionStatusV1::InProgress,
                            kill_count_to_reach,
                            kill_count_at_assignment: kill_count_to_reach.and_then(|threshold| {
                                match &definition.goal {
                                    MissionGoalV1::KillMonsterType { count, .. }
                                    | MissionGoalV1::KillMonsterSpecies { count, .. } => {
                                        threshold.checked_sub(u64::from(*count))
                                    }
                                    _ => None,
                                }
                            }),
                        },
                    );
                    lifecycle.push(Some(MissionLifecycleEvent::Assigned {
                        mission_id,
                        mission_type_id,
                    }));
                }
                MissionOperation::Finish {
                    mission_type_id,
                    mission_id,
                    success,
                } => {
                    let definition = self
                        .mission_definitions
                        .get(&mission_type_id)
                        .ok_or(SimError::InvalidMission)?;
                    let mission_id = mission_id.or_else(|| {
                        actor
                            .missions
                            .iter()
                            .find(|(_id, mission)| {
                                mission.mission_type_id == mission_type_id
                                    && mission.status == MissionStatusV1::InProgress
                            })
                            .map(|(id, _mission)| *id)
                    });
                    let Some(mission_id) = mission_id else {
                        // Pinned `finish_mission` silently does nothing when no
                        // active mission of the requested type exists.
                        lifecycle.push(None);
                        continue;
                    };
                    if actor.missions.get(&mission_id).is_none_or(|mission| {
                        mission.mission_type_id != mission_type_id
                            || mission.status != MissionStatusV1::InProgress
                    }) {
                        return Err(SimError::InvalidMission);
                    }
                    if success {
                        self.consume_mission_items(
                            &mut actor,
                            ground_items.get_or_insert_with(|| self.ground_items.clone()),
                            vehicles.get_or_insert_with(|| self.vehicles.clone()),
                            &definition.goal,
                        )?;
                    }
                    let mission = actor
                        .missions
                        .get_mut(&mission_id)
                        .ok_or(SimError::InvalidMission)?;
                    mission.status = if success {
                        MissionStatusV1::Success
                    } else {
                        MissionStatusV1::Failure
                    };
                    mission.finished_at_tick = Some(self.tick);
                    lifecycle.push(Some(MissionLifecycleEvent::Finished {
                        mission_id,
                        mission_type_id,
                        success,
                    }));
                }
            }
        }
        self.allocator = allocator;
        self.actors.insert(actor_id, actor);
        if let Some(ground_items) = ground_items {
            self.ground_items = ground_items;
        }
        if let Some(vehicles) = vehicles {
            self.vehicles = vehicles;
        }
        for (npc_id, npc) in npc_updates {
            self.npcs.insert(npc_id, npc);
        }
        Ok(lifecycle)
    }

    pub(super) fn emit_mission_lifecycle_event(
        &mut self,
        actor_id: ActorId,
        event: MissionLifecycleEvent,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let kind = match event {
            MissionLifecycleEvent::Assigned {
                mission_id,
                mission_type_id,
            } => WorldEventKind::MissionAssigned {
                actor_id,
                mission_id,
                mission_type_id,
            },
            MissionLifecycleEvent::Finished {
                mission_id,
                mission_type_id,
                success,
            } => WorldEventKind::MissionFinished {
                actor_id,
                mission_id,
                mission_type_id,
                success,
            },
        };
        events.push(self.make_event(kind)?);
        Ok(())
    }

    pub(super) fn record_actor_creature_kill(
        &mut self,
        killer: ActorId,
        creature_type_id: &str,
    ) -> Result<(), SimError> {
        if !self.actors.contains_key(&killer) || creature_type_id.is_empty() {
            return Err(SimError::InvalidMission);
        }
        let count = self
            .actors
            .get_mut(&killer)
            .ok_or(SimError::UnknownActor)?
            .creature_kill_counts
            .entry(creature_type_id.to_owned())
            .or_default();
        *count = count.checked_add(1).ok_or(SimError::NumericOverflow)?;
        Ok(())
    }

    pub fn mission_goal_is_complete(
        &self,
        actor_id: ActorId,
        mission_id: cdda_protocol::MissionId,
    ) -> Result<bool, SimError> {
        let actor = self.actors.get(&actor_id).ok_or(SimError::UnknownActor)?;
        let mission = actor
            .missions
            .get(&mission_id)
            .ok_or(SimError::InvalidMission)?;
        if mission.status != MissionStatusV1::InProgress {
            return Ok(mission.status == MissionStatusV1::Success);
        }
        let definition = self
            .mission_definitions
            .get(&mission.mission_type_id)
            .ok_or(SimError::InvalidMission)?;
        Ok(match &definition.goal {
            MissionGoalV1::Null => false,
            MissionGoalV1::FindItem {
                item_type_id,
                count,
                count_by_charges,
            } => {
                if actor.craft_activity.is_some()
                    || actor.read_activity.is_some()
                    || actor.disassembly_activity.is_some()
                    || actor.construction_activity.is_some()
                    || actor.sleeping
                {
                    return Ok(false);
                }
                self.available_mission_item_quantity(
                    actor,
                    item_type_id,
                    *count_by_charges,
                    u64::from(*count),
                )? >= u64::from(*count)
            }
            MissionGoalV1::KillMonsterType { .. } | MissionGoalV1::KillMonsterSpecies { .. } => {
                current_kill_count(&definition.goal, &actor.creature_kill_counts)?
                    >= mission
                        .kill_count_to_reach
                        .ok_or(SimError::InvalidMission)?
            }
        })
    }

    fn available_mission_item_quantity(
        &self,
        actor: &Actor,
        item_type_id: &str,
        count_by_charges: bool,
        limit: u64,
    ) -> Result<u64, SimError> {
        let mut found = 0_u64;
        for item in actor.inventory.values() {
            found = found
                .checked_add(crate::mission_items::item_instance_quantity(
                    item,
                    item_type_id,
                    count_by_charges,
                    limit - found,
                    false,
                )?)
                .ok_or(SimError::NumericOverflow)?;
            if found >= limit {
                return Ok(found);
            }
        }
        let visible = self.visible_mission_source_positions(actor.id, actor.position)?;
        // Terrain/furniture SEALED and LIQUIDCONT flags are not yet canonical.
        // Ground stacks therefore remain fail-closed instead of being treated
        // as accessible. Pinned vehicle cargo bypasses `accessible_items`.
        for position in visible {
            let Some(cargo) =
                crate::mission_items::selected_vehicle_cargo(&self.vehicles, position)
            else {
                continue;
            };
            for item in cargo {
                if !mission_item_is_available_to_player(&item.owner_faction_id) {
                    continue;
                }
                found = found
                    .checked_add(crate::mission_items::item_snapshot_quantity(
                        item,
                        item_type_id,
                        count_by_charges,
                        limit - found,
                        false,
                    )?)
                    .ok_or(SimError::NumericOverflow)?;
                if found >= limit {
                    return Ok(found);
                }
            }
        }
        Ok(found)
    }

    fn visible_mission_source_positions(
        &self,
        actor_id: ActorId,
        origin: WorldPosition,
    ) -> Result<BTreeSet<WorldPosition>, SimError> {
        let mut positions = BTreeSet::new();
        for y in origin.y.saturating_sub(5)..=origin.y.saturating_add(5) {
            for x in origin.x.saturating_sub(5)..=origin.x.saturating_add(5) {
                let position = WorldPosition { x, y, z: origin.z };
                if self.actor_can_see_position(actor_id, position)? {
                    positions.insert(position);
                }
            }
        }
        Ok(positions)
    }

    fn reachable_mission_source_positions(&self, origin: WorldPosition) -> BTreeSet<WorldPosition> {
        let mut reachable = BTreeSet::from([origin]);
        let mut frontier = BTreeSet::from([origin]);
        // Pinned `reachable_flood_steps(PICKUP_RANGE=6, 1, 100)` visits through
        // distance five, then marks each visited tile and its neighbors as a
        // source square. Completion visibility remains the separate radius 5.
        for _ in 0..5 {
            let mut next_frontier = BTreeSet::new();
            for current in frontier {
                for (dx, dy) in [
                    (-1, -1),
                    (0, -1),
                    (1, -1),
                    (-1, 0),
                    (1, 0),
                    (-1, 1),
                    (0, 1),
                    (1, 1),
                ] {
                    let Some(next) = current.checked_offset(dx, dy, 0) else {
                        continue;
                    };
                    if next.x.abs_diff(origin.x) > 6
                        || next.y.abs_diff(origin.y) > 6
                        || reachable.contains(&next)
                        || !matches!(self.tile_movement_cost(next), Some(1..=100))
                    {
                        continue;
                    }
                    reachable.insert(next);
                    next_frontier.insert(next);
                }
            }
            frontier = next_frontier;
        }
        reachable
    }

    pub(super) fn mission_turn_in_source_positions(
        &self,
        origin: WorldPosition,
    ) -> Vec<WorldPosition> {
        let reachable = self.reachable_mission_source_positions(origin);
        let mut source_positions = self
            .vehicles
            .values()
            .flat_map(|vehicle| vehicle.parts.iter().map(|part| part.position))
            .filter(|position| mission_source_is_reachable(*position, &reachable))
            .collect::<Vec<_>>();
        source_positions.sort_by_key(|position| (position.z, position.y, position.x));
        source_positions.dedup();
        source_positions
    }

    fn consume_mission_items(
        &self,
        actor: &mut Actor,
        _ground_items: &mut BTreeMap<cdda_protocol::ItemId, crate::GroundItem>,
        vehicles: &mut BTreeMap<cdda_protocol::VehicleId, VehicleSnapshotV1>,
        goal: &MissionGoalV1,
    ) -> Result<(), SimError> {
        let MissionGoalV1::FindItem {
            item_type_id,
            count,
            count_by_charges,
        } = goal
        else {
            return Ok(());
        };
        let source_positions = self.mission_turn_in_source_positions(actor.position);
        crate::mission_items::consume_mission_items_from_sources(
            &mut actor.inventory,
            &mut actor.worn,
            &mut actor.wielded,
            vehicles,
            &source_positions,
            item_type_id,
            *count_by_charges,
            *count,
        )
    }

    pub(super) fn advance_missions(
        &mut self,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let mut candidates = Vec::new();
        for (actor_id, actor) in &self.actors {
            for mission in actor.missions.values() {
                if mission.status == MissionStatusV1::InProgress && mission.origin_npc_id.is_none()
                {
                    candidates.push((*actor_id, mission.mission_id));
                }
            }
        }
        for (actor_id, mission_id) in candidates {
            if !self.mission_goal_is_complete(actor_id, mission_id)? {
                continue;
            }
            self.apply_mission_finish(
                actor_id,
                mission_id,
                true,
                b"automatic-mission-end",
                self.tick.0,
                events,
            )?;
        }
        Ok(())
    }
}

pub(super) fn actor_missions_are_valid(
    missions: &[MissionSnapshotV1],
    world_namespace: u64,
    definitions: &BTreeMap<String, MissionDefinitionV1>,
) -> bool {
    missions.len() <= cdda_protocol::MAX_ACTOR_MISSIONS
        && missions
            .windows(2)
            .all(|pair| pair[0].mission_id < pair[1].mission_id)
        && missions.iter().all(|mission| {
            definitions
                .get(&mission.mission_type_id)
                .is_some_and(|definition| {
                    cdda_protocol::mission_snapshot_is_valid_for_definition(
                        mission,
                        world_namespace,
                        definition,
                    )
                })
        })
}

fn prune_finished_mission_history(actor: &mut Actor) -> Result<(), SimError> {
    if actor.missions.len() < cdda_protocol::MAX_ACTOR_MISSIONS {
        return Ok(());
    }
    let oldest = actor
        .missions
        .values()
        .filter_map(|mission| {
            mission
                .finished_at_tick
                .map(|finished| (finished, mission.mission_id))
        })
        .min();
    let Some((_finished, mission_id)) = oldest else {
        return Err(SimError::InvalidMission);
    };
    actor.missions.remove(&mission_id);
    Ok(())
}

pub(super) fn active_mission_types(actor: &Actor) -> Vec<String> {
    actor
        .missions
        .values()
        .filter(|mission| mission.status == MissionStatusV1::InProgress)
        .map(|mission| mission.mission_type_id.clone())
        .collect()
}

fn current_kill_count(
    goal: &MissionGoalV1,
    kill_counts: &BTreeMap<String, u64>,
) -> Result<u64, SimError> {
    match goal {
        MissionGoalV1::KillMonsterType {
            monster_type_id, ..
        } => Ok(kill_counts.get(monster_type_id).copied().unwrap_or(0)),
        MissionGoalV1::KillMonsterSpecies {
            monster_type_ids, ..
        } => monster_type_ids.iter().try_fold(0_u64, |total, id| {
            total
                .checked_add(kill_counts.get(id).copied().unwrap_or(0))
                .ok_or(SimError::NumericOverflow)
        }),
        _ => Err(SimError::InvalidMission),
    }
}

pub(super) fn current_kill_count_for_recovery(
    goal: &MissionGoalV1,
    kill_counts: &BTreeMap<String, u64>,
) -> Result<u64, SimError> {
    current_kill_count(goal, kill_counts)
}

fn mission_item_is_available_to_player(owner_faction_id: &str) -> bool {
    owner_faction_id.is_empty() || owner_faction_id == PLAYER_FACTION_ID
}

fn mission_source_is_reachable(
    position: WorldPosition,
    reachable: &BTreeSet<WorldPosition>,
) -> bool {
    reachable.contains(&position)
        || [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ]
        .into_iter()
        .filter_map(|(dx, dy)| position.checked_offset(dx, dy, 0))
        .any(|neighbor| reachable.contains(&neighbor))
}
