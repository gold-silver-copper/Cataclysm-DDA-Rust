use std::collections::BTreeMap;

use cdda_protocol::{
    AmmunitionContainerPocketSnapshotV1, CraftItemPrototypeV1, CreatureCorpseSnapshotV1,
    IntegralMagazinePocketSnapshotV1, ItemComponentSnapshotV1, ItemGroupDefinitionV1,
    ItemGroupEntryV1, ItemGroupGraphV1, ItemGroupKindV1, ItemGroupSourceV1, ItemGroupTargetV1,
    ItemId, ItemSnapshot, MAX_ITEM_GROUP_DEPTH, MAX_ITEM_GROUP_OUTPUTS,
    MILLIJOULES_PER_BATTERY_CHARGE, MagazineWellSnapshotV1, PoweredToolStateV1,
    RangedWeaponSnapshot,
};
use rand_chacha::ChaCha8Rng;
use rand_core::Rng;
use serde::{Deserialize, Serialize};

use super::{
    SimError, debit_integral_magazine_charges, debit_snapshot_ammunition_charges,
    inclusive_rng_u64, powered_light_effective_emission, snapshot_ammunition_capacity,
    snapshot_stored_ammunition_charges, validate_craft_item_prototype, validate_item_snapshot,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct ItemInstance {
    pub(super) id: ItemId,
    pub(super) type_id: String,
    pub(super) charges: i32,
    pub(super) damage: u16,
    pub(super) melee_damage_milli: BTreeMap<String, i32>,
    pub(super) calories: i32,
    pub(super) quench: i32,
    pub(super) comestible_type: String,
    pub(super) ammunition_type: String,
    pub(super) ranged_weapon: Option<RangedWeaponSnapshot>,
    pub(super) component_provenance: Option<Vec<ItemComponentSnapshotV1>>,
    pub(super) magazine_capacity: u32,
    pub(super) integral_magazines: Vec<IntegralMagazinePocketSnapshotV1>,
    pub(super) magazine_wells: Vec<MagazineWellSnapshotV1>,
    pub(super) ammunition_containers: Vec<AmmunitionContainerPocketSnapshotV1>,
    pub(super) residual_energy_millijoules: u32,
    pub(super) powered_tool: Option<PoweredToolStateV1>,
    pub(super) creature_corpse: Option<CreatureCorpseSnapshotV1>,
}

impl ItemInstance {
    pub(super) fn integral_ammunition_charges(&self) -> i32 {
        self.integral_magazines
            .iter()
            .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
            .fold(0_i32, |total, ammunition| {
                total.saturating_add(ammunition.charges)
            })
    }

    pub(super) fn stored_ammunition_charges(&self) -> i32 {
        if self.integral_magazines.is_empty() {
            self.charges
        } else {
            self.integral_ammunition_charges()
        }
    }

    pub(super) fn available_tool_charges(&self) -> i32 {
        if self.magazine_wells.is_empty() {
            return self.stored_ammunition_charges();
        }
        self.magazine_wells
            .iter()
            .filter_map(|well| well.installed_magazine.as_deref())
            .fold(0, |total, magazine| {
                total.saturating_add(snapshot_stored_ammunition_charges(magazine))
            })
    }

    pub(super) fn debit_tool_charges(&mut self, charges: i32) -> Result<i32, SimError> {
        if charges < 0 {
            return Err(SimError::InvalidItem);
        }
        if self.magazine_wells.is_empty() {
            if self.integral_magazines.is_empty() {
                self.charges = self
                    .charges
                    .checked_sub(charges)
                    .filter(|remaining| *remaining >= 0)
                    .ok_or(SimError::InvalidItem)?;
            } else {
                debit_integral_magazine_charges(&mut self.integral_magazines, charges)?;
            }
            return Ok(self.stored_ammunition_charges());
        }
        if self.available_tool_charges() < charges {
            return Err(SimError::InvalidItem);
        }
        let mut required = charges;
        for magazine in self
            .magazine_wells
            .iter_mut()
            .filter_map(|well| well.installed_magazine.as_deref_mut())
        {
            let debit = required.min(snapshot_stored_ammunition_charges(magazine));
            debit_snapshot_ammunition_charges(magazine, debit)?;
            required -= debit;
            if required == 0 {
                break;
            }
        }
        Ok(self.available_tool_charges())
    }

    fn power_magazine(&self) -> Option<&ItemSnapshot> {
        let pocket_index = self.powered_tool.as_ref()?.power_pocket_index;
        self.magazine_wells
            .iter()
            .find(|well| well.pocket_index == pocket_index)?
            .installed_magazine
            .as_deref()
    }

    fn power_magazine_mut(&mut self) -> Option<&mut ItemSnapshot> {
        let pocket_index = self.powered_tool.as_ref()?.power_pocket_index;
        self.magazine_wells
            .iter_mut()
            .find(|well| well.pocket_index == pocket_index)?
            .installed_magazine
            .as_deref_mut()
    }

    pub(super) fn available_power_energy_millijoules(&self) -> Result<u64, SimError> {
        let Some(magazine) = self.power_magazine() else {
            return Ok(0);
        };
        let charges = u64::try_from(snapshot_stored_ammunition_charges(magazine))
            .map_err(|_| SimError::InvalidItem)?;
        let residual = if magazine.integral_magazines.is_empty() {
            magazine.residual_energy_millijoules
        } else {
            magazine
                .integral_magazines
                .iter()
                .try_fold(0_u32, |total, pocket| {
                    total.checked_add(pocket.residual_energy_millijoules)
                })
                .ok_or(SimError::NumericOverflow)?
        };
        charges
            .checked_mul(u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
            .and_then(|energy| energy.checked_add(u64::from(residual)))
            .ok_or(SimError::NumericOverflow)
    }

    pub(super) fn effective_powered_light_emission(&self) -> Result<u16, SimError> {
        let Some(powered) = self.powered_tool.as_ref().filter(|powered| powered.active) else {
            return Ok(0);
        };
        let Some(magazine) = self.power_magazine() else {
            return Ok(0);
        };
        Ok(powered_light_effective_emission(
            powered.light_emission,
            powered.dims_with_charge,
            self.available_power_energy_millijoules()?,
            snapshot_ammunition_capacity(magazine),
        ))
    }

    pub(super) fn consume_activation_power(&mut self, charges: u16) -> Result<bool, SimError> {
        let Some(magazine) = self.power_magazine_mut() else {
            return Ok(false);
        };
        let charges = i32::from(charges);
        if snapshot_stored_ammunition_charges(magazine) < charges {
            return Ok(false);
        }
        debit_snapshot_ammunition_charges(magazine, charges)?;
        Ok(true)
    }

    pub(super) fn consume_continuous_power(&mut self, millijoules: u32) -> Result<bool, SimError> {
        let Some(magazine) = self.power_magazine_mut() else {
            return Ok(false);
        };
        if !magazine.integral_magazines.is_empty() {
            let available =
                magazine
                    .integral_magazines
                    .iter()
                    .try_fold(0_u64, |total, pocket| {
                        let charges = pocket
                            .loaded_ammunition
                            .as_deref()
                            .map(|ammunition| ammunition.charges)
                            .unwrap_or(0);
                        u64::try_from(charges)
                            .map_err(|_| SimError::InvalidItem)?
                            .checked_mul(u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
                            .and_then(|energy| {
                                energy.checked_add(u64::from(pocket.residual_energy_millijoules))
                            })
                            .and_then(|energy| total.checked_add(energy))
                            .ok_or(SimError::NumericOverflow)
                    })?;
            let mut remaining = u64::from(millijoules);
            if available < remaining {
                return Ok(false);
            }
            for pocket in &mut magazine.integral_magazines {
                let Some(ammunition) = pocket.loaded_ammunition.as_deref_mut() else {
                    continue;
                };
                let pocket_energy = u64::try_from(ammunition.charges)
                    .map_err(|_| SimError::InvalidItem)?
                    .checked_mul(u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
                    .and_then(|energy| {
                        energy.checked_add(u64::from(pocket.residual_energy_millijoules))
                    })
                    .ok_or(SimError::NumericOverflow)?;
                let consumed = pocket_energy.min(remaining);
                let retained = pocket_energy
                    .checked_sub(consumed)
                    .ok_or(SimError::NumericOverflow)?;
                ammunition.charges =
                    i32::try_from(retained / u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
                        .map_err(|_| SimError::NumericOverflow)?;
                pocket.residual_energy_millijoules =
                    u32::try_from(retained % u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
                        .map_err(|_| SimError::NumericOverflow)?;
                remaining = remaining
                    .checked_sub(consumed)
                    .ok_or(SimError::NumericOverflow)?;
                if retained == 0 {
                    pocket.loaded_ammunition = None;
                }
                if remaining == 0 {
                    break;
                }
            }
            return Ok(true);
        }
        let available = u64::try_from(magazine.charges)
            .map_err(|_| SimError::InvalidItem)?
            .checked_mul(u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
            .and_then(|energy| energy.checked_add(u64::from(magazine.residual_energy_millijoules)))
            .ok_or(SimError::NumericOverflow)?;
        if available < u64::from(millijoules) {
            return Ok(false);
        }
        let mut residual = u64::from(magazine.residual_energy_millijoules);
        while residual < u64::from(millijoules) {
            magazine.charges = magazine
                .charges
                .checked_sub(1)
                .ok_or(SimError::NumericOverflow)?;
            residual = residual
                .checked_add(u64::from(MILLIJOULES_PER_BATTERY_CHARGE))
                .ok_or(SimError::NumericOverflow)?;
        }
        residual = residual
            .checked_sub(u64::from(millijoules))
            .ok_or(SimError::NumericOverflow)?;
        magazine.residual_energy_millijoules =
            u32::try_from(residual).map_err(|_| SimError::NumericOverflow)?;
        Ok(true)
    }

    pub(super) fn set_powered_active(&mut self, active: bool) -> Result<(), SimError> {
        let powered = self.powered_tool.as_mut().ok_or(SimError::InvalidItem)?;
        powered.active = active;
        self.type_id.clone_from(if active {
            &powered.active_type_id
        } else {
            &powered.inactive_type_id
        });
        Ok(())
    }

    pub(super) fn snapshot(&self) -> ItemSnapshot {
        ItemSnapshot {
            id: self.id,
            type_id: self.type_id.clone(),
            charges: self.charges,
            damage: self.damage,
            melee_damage_milli: self.melee_damage_milli.clone(),
            calories: self.calories,
            quench: self.quench,
            comestible_type: self.comestible_type.clone(),
            ammunition_type: self.ammunition_type.clone(),
            ranged_weapon: self.ranged_weapon.clone(),
            component_provenance: self.component_provenance.clone(),
            magazine_capacity: self.magazine_capacity,
            integral_magazines: self.integral_magazines.clone(),
            magazine_wells: self.magazine_wells.clone(),
            ammunition_containers: self.ammunition_containers.clone(),
            residual_energy_millijoules: self.residual_energy_millijoules,
            powered_tool: self.powered_tool.clone(),
            creature_corpse: self.creature_corpse.clone(),
        }
    }

    pub(super) fn from_snapshot(snapshot: &ItemSnapshot) -> Result<Self, SimError> {
        validate_item_snapshot(snapshot)?;
        Ok(Self {
            id: snapshot.id,
            type_id: snapshot.type_id.clone(),
            charges: snapshot.charges,
            damage: snapshot.damage,
            melee_damage_milli: snapshot.melee_damage_milli.clone(),
            calories: snapshot.calories,
            quench: snapshot.quench,
            comestible_type: snapshot.comestible_type.clone(),
            ammunition_type: snapshot.ammunition_type.clone(),
            ranged_weapon: snapshot.ranged_weapon.clone(),
            component_provenance: snapshot.component_provenance.clone(),
            magazine_capacity: snapshot.magazine_capacity,
            integral_magazines: snapshot.integral_magazines.clone(),
            magazine_wells: snapshot.magazine_wells.clone(),
            ammunition_containers: snapshot.ammunition_containers.clone(),
            residual_energy_millijoules: snapshot.residual_energy_millijoules,
            powered_tool: snapshot.powered_tool.clone(),
            creature_corpse: snapshot.creature_corpse.clone(),
        })
    }
}

pub(super) fn plan_item_group_source(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<CraftItemPrototypeV1>, SimError> {
    let mut output = Vec::new();
    plan_item_group_source_into(source, item_groups, rng, &mut output, 0)?;
    Ok(output)
}

fn plan_item_group_source_into(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    let graph = match source {
        ItemGroupSourceV1::Group(group_id) => {
            &item_groups
                .get(group_id)
                .ok_or(SimError::InvalidItem)?
                .graph
        }
        ItemGroupSourceV1::Inline(graph) => graph,
    };
    plan_item_group_node(graph, graph.root_node, item_groups, rng, output, depth)
}

fn plan_item_group_node(
    graph: &ItemGroupGraphV1,
    node_id: u16,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    if depth > MAX_ITEM_GROUP_DEPTH {
        return Err(SimError::InvalidItem);
    }
    let node = graph
        .nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .ok_or(SimError::InvalidItem)?;
    match node.kind {
        ItemGroupKindV1::Collection => {
            for entry in &node.entries {
                // Pinned collection generation rolls even for guaranteed and
                // inactive event entries. The server fixes EVENT_SPAWNS to its
                // upstream default `off`, so a qualified entry never spawns.
                let roll = rng.next_u64() % 100;
                if entry.event.is_none() && roll < u64::from(entry.probability) {
                    plan_item_group_entry(graph, entry, item_groups, rng, output, depth)?;
                }
            }
        }
        ItemGroupKindV1::Distribution => {
            let total = node.entries.iter().try_fold(0_u64, |total, entry| {
                total.checked_add(u64::from(entry.probability))
            });
            let Some(total) = total.filter(|total| *total > 0) else {
                return Ok(());
            };
            let ticket = inclusive_rng_u64(rng, 1, total);
            let mut accumulated = 0_u64;
            let entry = node
                .entries
                .iter()
                .find(|entry| {
                    accumulated = accumulated.saturating_add(u64::from(entry.probability));
                    ticket <= accumulated
                })
                .ok_or(SimError::InvalidItem)?;
            // Distribution tickets retain the original event entry weight.
            // Under the deterministic disabled policy, landing on one yields
            // no item instead of selecting another entry.
            if entry.event.is_some() {
                return Ok(());
            }
            plan_item_group_entry(graph, entry, item_groups, rng, output, depth)?;
        }
    }
    Ok(())
}

fn plan_item_group_entry(
    graph: &ItemGroupGraphV1,
    entry: &ItemGroupEntryV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    let count = if entry.count_min == entry.count_max {
        u64::from(entry.count_min)
    } else {
        inclusive_rng_u64(rng, u64::from(entry.count_min), u64::from(entry.count_max))
    };
    for _ in 0..count {
        plan_item_group_target(
            graph,
            &entry.target,
            item_groups,
            rng,
            output,
            depth.checked_add(1).ok_or(SimError::NumericOverflow)?,
        )?;
    }
    Ok(())
}

fn plan_item_group_target(
    graph: &ItemGroupGraphV1,
    target: &ItemGroupTargetV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<CraftItemPrototypeV1>,
    depth: usize,
) -> Result<(), SimError> {
    if depth > MAX_ITEM_GROUP_DEPTH {
        return Err(SimError::InvalidItem);
    }
    match target {
        ItemGroupTargetV1::Item(item) => {
            let mut prototype = item.prototype.clone();
            if let Some(charges) = item.charges {
                let rolled = if charges.minimum == charges.maximum {
                    charges.minimum
                } else {
                    i32::try_from(inclusive_rng_u64(
                        rng,
                        u64::try_from(charges.minimum).map_err(|_| SimError::InvalidItem)?,
                        u64::try_from(charges.maximum).map_err(|_| SimError::InvalidItem)?,
                    ))
                    .map_err(|_| SimError::NumericOverflow)?
                };
                prototype.charges = if item.minimum_one_charge {
                    rolled.max(1)
                } else {
                    rolled
                };
            }
            if output.len()
                >= usize::try_from(MAX_ITEM_GROUP_OUTPUTS).map_err(|_| SimError::NumericOverflow)?
            {
                return Err(SimError::InvalidItem);
            }
            if validate_craft_item_prototype(&prototype).is_err() {
                return Err(SimError::InvalidItem);
            }
            output.push(prototype);
        }
        ItemGroupTargetV1::Group(group_id) => plan_item_group_source_into(
            &ItemGroupSourceV1::Group(group_id.clone()),
            item_groups,
            rng,
            output,
            depth,
        )?,
        ItemGroupTargetV1::Node(node_id) => {
            plan_item_group_node(graph, *node_id, item_groups, rng, output, depth)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_protocol::{ItemGroupEventV1, ItemGroupItemPrototypeV1, ItemGroupNodeV1};
    use rand_core::SeedableRng;

    fn leaf(type_id: &str) -> ItemGroupTargetV1 {
        ItemGroupTargetV1::Item(Box::new(ItemGroupItemPrototypeV1 {
            prototype: CraftItemPrototypeV1 {
                type_id: type_id.to_owned(),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: String::new(),
                ranged_weapon: None,
                magazine_capacity: 0,
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
            },
            charges: None,
            minimum_one_charge: false,
        }))
    }

    fn entry(probability: u32, event: Option<ItemGroupEventV1>, type_id: &str) -> ItemGroupEntryV1 {
        ItemGroupEntryV1 {
            probability,
            count_min: 1,
            count_max: 1,
            event,
            target: leaf(type_id),
        }
    }

    #[test]
    fn disabled_event_collection_still_consumes_its_probability_roll() {
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![
                    entry(100, Some(ItemGroupEventV1::Christmas), "holiday_token"),
                    entry(100, None, "ordinary"),
                ],
            }],
        });
        let mut actual_rng = ChaCha8Rng::seed_from_u64(19);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("valid event collection should plan");
        assert_eq!(
            planned
                .iter()
                .map(|prototype| prototype.type_id.as_str())
                .collect::<Vec<_>>(),
            ["ordinary"]
        );

        let mut expected_rng = ChaCha8Rng::seed_from_u64(19);
        let _ = expected_rng.next_u64();
        let _ = expected_rng.next_u64();
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn disabled_event_distribution_retains_empty_ticket_intervals() {
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Distribution,
                entries: vec![
                    entry(3, Some(ItemGroupEventV1::Halloween), "holiday_token"),
                    entry(2, None, "ordinary"),
                ],
            }],
        });
        for ticket in 1..=5 {
            let seed = (0..100_000)
                .find(|seed| {
                    let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                    inclusive_rng_u64(&mut rng, 1, 5) == ticket
                })
                .expect("every bounded ticket should have a witness seed");
            let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
            let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
                .expect("valid event distribution should plan");
            assert_eq!(
                planned
                    .iter()
                    .map(|prototype| prototype.type_id.as_str())
                    .collect::<Vec<_>>(),
                if ticket <= 3 {
                    Vec::<&str>::new()
                } else {
                    vec!["ordinary"]
                },
                "ticket {ticket} must retain its pinned interval"
            );

            let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
            assert_eq!(inclusive_rng_u64(&mut expected_rng, 1, 5), ticket);
            assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
        }
    }
}
