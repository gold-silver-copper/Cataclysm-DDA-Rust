use std::collections::BTreeMap;

use cdda_protocol::{
    ActorId, ItemSnapshot, MissionDefinitionV1, MissionGoalV1, MissionId, MissionSnapshotV1,
    MissionStatusV1, NpcId, WorldEvent, WorldEventKind,
};

use crate::{Actor, SimError, WorldState};

#[derive(Clone, Debug)]
pub(super) enum MissionOperation {
    Assign {
        mission_type_id: String,
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
            || definitions.iter().any(|definition| {
                !definition.start_effects.is_empty()
                    || !definition.end_effects.is_empty()
                    || !definition.fail_effects.is_empty()
            })
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
    ) -> Result<Vec<MissionLifecycleEvent>, SimError> {
        if operations.is_empty() {
            return Ok(Vec::new());
        }
        let mut actor = self
            .actors
            .get(&actor_id)
            .cloned()
            .ok_or(SimError::UnknownActor)?;
        let mut allocator = self.allocator.clone();
        let mut lifecycle = Vec::with_capacity(operations.len());
        for operation in operations {
            match operation {
                MissionOperation::Assign {
                    mission_type_id,
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
                    if active_count >= cdda_protocol::MAX_ACTOR_MISSIONS
                        || origin_npc_id.is_some_and(|npc_id| !self.npcs.contains_key(&npc_id))
                    {
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
                    let mission_id = allocator.allocate_mission()?;
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
                    lifecycle.push(MissionLifecycleEvent::Assigned {
                        mission_id,
                        mission_type_id,
                    });
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
                        continue;
                    };
                    if actor.missions.get(&mission_id).is_none_or(|mission| {
                        mission.mission_type_id != mission_type_id
                            || mission.status != MissionStatusV1::InProgress
                    }) {
                        return Err(SimError::InvalidMission);
                    }
                    if success {
                        consume_mission_items(&mut actor, &definition.goal)?;
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
                    lifecycle.push(MissionLifecycleEvent::Finished {
                        mission_id,
                        mission_type_id,
                        success,
                    });
                }
            }
        }
        self.allocator = allocator;
        self.actors.insert(actor_id, actor);
        Ok(lifecycle)
    }

    pub(super) fn emit_mission_lifecycle_events(
        &mut self,
        actor_id: ActorId,
        lifecycle: Vec<MissionLifecycleEvent>,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        for event in lifecycle {
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
        }
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
                {
                    return Ok(false);
                }
                let summary = crate::items::summarize_inventory_by_type(actor.inventory.values());
                summary.get(item_type_id).is_some_and(|entry| {
                    if *count_by_charges {
                        entry.charges >= u64::from(*count)
                    } else {
                        entry.amount >= u64::from(*count)
                    }
                })
            }
            MissionGoalV1::KillMonsterType { .. } | MissionGoalV1::KillMonsterSpecies { .. } => {
                current_kill_count(&definition.goal, &actor.creature_kill_counts)?
                    >= mission
                        .kill_count_to_reach
                        .ok_or(SimError::InvalidMission)?
            }
        })
    }

    pub(super) fn accept_npc_mission(
        &mut self,
        actor_id: ActorId,
        npc_id: NpcId,
        mission_id: MissionId,
    ) -> Result<MissionLifecycleEvent, SimError> {
        let mut actor = self
            .actors
            .get(&actor_id)
            .cloned()
            .ok_or(SimError::UnknownActor)?;
        let mut npc = self
            .npcs
            .get(&npc_id)
            .cloned()
            .ok_or(SimError::UnknownNpc)?;
        let mission_type_id = npc
            .mission_offers
            .remove(&mission_id)
            .ok_or(SimError::InvalidMission)?;
        let definition = self
            .mission_definitions
            .get(&mission_type_id)
            .ok_or(SimError::InvalidMission)?;
        if actor
            .missions
            .values()
            .filter(|mission| mission.status == MissionStatusV1::InProgress)
            .count()
            >= cdda_protocol::MAX_ACTOR_MISSIONS
            || actor.missions.contains_key(&mission_id)
        {
            return Err(SimError::InvalidMission);
        }
        prune_finished_mission_history(&mut actor)?;
        let kill_count_at_assignment = match &definition.goal {
            MissionGoalV1::KillMonsterType { .. } | MissionGoalV1::KillMonsterSpecies { .. } => {
                Some(current_kill_count(
                    &definition.goal,
                    &actor.creature_kill_counts,
                )?)
            }
            MissionGoalV1::Null | MissionGoalV1::FindItem { .. } => None,
        };
        let kill_count_to_reach = match (&definition.goal, kill_count_at_assignment) {
            (MissionGoalV1::KillMonsterType { count, .. }, Some(baseline))
            | (MissionGoalV1::KillMonsterSpecies { count, .. }, Some(baseline)) => Some(
                baseline
                    .checked_add(u64::from(*count))
                    .ok_or(SimError::NumericOverflow)?,
            ),
            (MissionGoalV1::Null | MissionGoalV1::FindItem { .. }, None) => None,
            _ => return Err(SimError::InvalidMission),
        };
        actor.missions.insert(
            mission_id,
            MissionSnapshotV1 {
                mission_id,
                mission_type_id: mission_type_id.clone(),
                origin_npc_id: Some(npc_id),
                assigned_at_tick: self.tick,
                finished_at_tick: None,
                status: MissionStatusV1::InProgress,
                kill_count_to_reach,
                kill_count_at_assignment,
            },
        );
        self.actors.insert(actor_id, actor);
        self.npcs.insert(npc_id, npc);
        Ok(MissionLifecycleEvent::Assigned {
            mission_id,
            mission_type_id,
        })
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
                    candidates.push((
                        *actor_id,
                        mission.mission_id,
                        mission.mission_type_id.clone(),
                    ));
                }
            }
        }
        for (actor_id, mission_id, mission_type_id) in candidates {
            if !self.mission_goal_is_complete(actor_id, mission_id)? {
                continue;
            }
            let lifecycle = self.commit_mission_operations(
                actor_id,
                vec![MissionOperation::Finish {
                    mission_type_id,
                    mission_id: Some(mission_id),
                    success: true,
                }],
            )?;
            self.emit_mission_lifecycle_events(actor_id, lifecycle, events)?;
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

fn consume_mission_items(actor: &mut Actor, goal: &MissionGoalV1) -> Result<(), SimError> {
    consume_mission_items_from_inventory(
        &mut actor.inventory,
        &mut actor.worn,
        &mut actor.wielded,
        goal,
    )
}

pub(super) fn consume_mission_items_from_inventory(
    inventory: &mut BTreeMap<cdda_protocol::ItemId, crate::items::ItemInstance>,
    worn: &mut Vec<cdda_protocol::ItemId>,
    wielded: &mut Option<cdda_protocol::ItemId>,
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
    let mut remaining = i64::from(*count);
    let ids = inventory.keys().copied().collect::<Vec<_>>();
    for id in ids {
        if remaining == 0 {
            break;
        }
        let remove = consume_item_instance(
            inventory.get_mut(&id).ok_or(SimError::InvalidItem)?,
            item_type_id,
            *count_by_charges,
            &mut remaining,
        )?;
        if remove {
            inventory.remove(&id);
            worn.retain(|worn| *worn != id);
            if *wielded == Some(id) {
                *wielded = None;
            }
        }
    }
    Ok(())
}

fn consume_item_instance(
    item: &mut crate::items::ItemInstance,
    item_type_id: &str,
    count_by_charges: bool,
    remaining: &mut i64,
) -> Result<bool, SimError> {
    if consume_matching_item(
        &item.type_id,
        &mut item.charges,
        item_type_id,
        count_by_charges,
        remaining,
    )? {
        return Ok(true);
    }
    consume_nested_items(
        &mut item.integral_magazines,
        &mut item.magazine_wells,
        &mut item.ammunition_containers,
        item_type_id,
        count_by_charges,
        remaining,
    )?;
    Ok(false)
}

fn consume_item_snapshot(
    item: &mut ItemSnapshot,
    item_type_id: &str,
    count_by_charges: bool,
    remaining: &mut i64,
) -> Result<bool, SimError> {
    if consume_matching_item(
        &item.type_id,
        &mut item.charges,
        item_type_id,
        count_by_charges,
        remaining,
    )? {
        return Ok(true);
    }
    consume_nested_items(
        &mut item.integral_magazines,
        &mut item.magazine_wells,
        &mut item.ammunition_containers,
        item_type_id,
        count_by_charges,
        remaining,
    )?;
    Ok(false)
}

fn consume_matching_item(
    actual_type_id: &str,
    charges: &mut i32,
    sought_type_id: &str,
    count_by_charges: bool,
    remaining: &mut i64,
) -> Result<bool, SimError> {
    if *remaining == 0 || actual_type_id != sought_type_id {
        return Ok(false);
    }
    if !count_by_charges {
        *remaining -= 1;
        return Ok(true);
    }
    let available = i64::from((*charges).max(0));
    let consumed = (*remaining).min(available);
    *charges = i32::try_from(available - consumed).map_err(|_| SimError::NumericOverflow)?;
    *remaining -= consumed;
    Ok(*charges == 0)
}

fn consume_nested_items(
    integral_magazines: &mut [cdda_protocol::IntegralMagazinePocketSnapshotV1],
    magazine_wells: &mut [cdda_protocol::MagazineWellSnapshotV1],
    ammunition_containers: &mut [cdda_protocol::AmmunitionContainerPocketSnapshotV1],
    item_type_id: &str,
    count_by_charges: bool,
    remaining: &mut i64,
) -> Result<(), SimError> {
    for pocket in integral_magazines {
        if *remaining == 0 {
            return Ok(());
        }
        let remove = if let Some(item) = pocket.loaded_ammunition.as_deref_mut() {
            consume_item_snapshot(item, item_type_id, count_by_charges, remaining)?
        } else {
            false
        };
        if remove {
            pocket.loaded_ammunition = None;
        }
    }
    for well in magazine_wells {
        if *remaining == 0 {
            return Ok(());
        }
        let remove = if let Some(item) = well.installed_magazine.as_deref_mut() {
            consume_item_snapshot(item, item_type_id, count_by_charges, remaining)?
        } else {
            false
        };
        if remove {
            well.installed_magazine = None;
        }
    }
    for pocket in ammunition_containers {
        let mut index = 0;
        while index < pocket.contents.len() && *remaining > 0 {
            if consume_item_snapshot(
                &mut pocket.contents[index],
                item_type_id,
                count_by_charges,
                remaining,
            )? {
                pocket.contents.remove(index);
            } else {
                index += 1;
            }
        }
    }
    Ok(())
}
