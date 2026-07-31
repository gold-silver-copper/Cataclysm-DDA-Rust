use std::collections::BTreeMap;

use cdda_protocol::{
    ActorId, ItemSnapshot, MissionDefinitionV1, MissionGoalV1, MissionSnapshotV1, MissionStatusV1,
    NpcId,
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
        success: bool,
    },
}

impl WorldState {
    pub fn register_mission_catalog(
        &mut self,
        definitions: Vec<MissionDefinitionV1>,
    ) -> Result<(), SimError> {
        if self.tick.0 != 0
            || !self.actors.is_empty()
            || !self.mission_definitions.is_empty()
            || !cdda_protocol::mission_catalog_is_valid(&definitions)
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
    ) -> Result<(), SimError> {
        if operations.is_empty() {
            return Ok(());
        }
        let mut actor = self
            .actors
            .get(&actor_id)
            .cloned()
            .ok_or(SimError::UnknownActor)?;
        let mut allocator = self.allocator.clone();
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
                    if actor.missions.len() >= cdda_protocol::MAX_ACTOR_MISSIONS
                        || origin_npc_id.is_some_and(|npc_id| !self.npcs.contains_key(&npc_id))
                    {
                        return Err(SimError::InvalidMission);
                    }
                    let kill_count_to_reach = match &definition.goal {
                        MissionGoalV1::KillMonsterType { count, .. }
                        | MissionGoalV1::KillMonsterSpecies { count, .. } => Some(
                            current_kill_count(&definition.goal, &self.monster_kill_counts)?
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
                            mission_type_id,
                            origin_npc_id,
                            assigned_at_tick: self.tick,
                            finished_at_tick: None,
                            status: MissionStatusV1::InProgress,
                            kill_count_to_reach,
                        },
                    );
                }
                MissionOperation::Finish {
                    mission_type_id,
                    success,
                } => {
                    let definition = self
                        .mission_definitions
                        .get(&mission_type_id)
                        .ok_or(SimError::InvalidMission)?;
                    let Some(mission_id) = actor
                        .missions
                        .iter()
                        .find(|(_id, mission)| {
                            mission.mission_type_id == mission_type_id
                                && mission.status == MissionStatusV1::InProgress
                        })
                        .map(|(id, _mission)| *id)
                    else {
                        // Pinned `finish_mission` silently does nothing when no
                        // active mission of the requested type exists.
                        continue;
                    };
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
                }
            }
        }
        self.allocator = allocator;
        self.actors.insert(actor_id, actor);
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
            .monster_kill_counts
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
                current_kill_count(&definition.goal, &self.monster_kill_counts)?
                    >= mission
                        .kill_count_to_reach
                        .ok_or(SimError::InvalidMission)?
            }
        })
    }
}

pub(super) fn actor_missions_are_valid(
    missions: &[MissionSnapshotV1],
    world_namespace: u64,
    definitions: &BTreeMap<String, MissionDefinitionV1>,
) -> bool {
    let catalog = definitions.values().cloned().collect::<Vec<_>>();
    cdda_protocol::actor_missions_are_valid(missions, world_namespace, &catalog)
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

fn consume_mission_items(actor: &mut Actor, goal: &MissionGoalV1) -> Result<(), SimError> {
    let MissionGoalV1::FindItem {
        item_type_id,
        count,
        count_by_charges,
    } = goal
    else {
        return Ok(());
    };
    let mut remaining = i64::from(*count);
    let ids = actor.inventory.keys().copied().collect::<Vec<_>>();
    for id in ids {
        if remaining == 0 {
            break;
        }
        let remove = consume_item_instance(
            actor.inventory.get_mut(&id).ok_or(SimError::InvalidItem)?,
            item_type_id,
            *count_by_charges,
            &mut remaining,
        )?;
        if remove {
            actor.inventory.remove(&id);
            actor.worn.retain(|worn| *worn != id);
            if actor.wielded == Some(id) {
                actor.wielded = None;
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
