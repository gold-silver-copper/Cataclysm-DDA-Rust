use std::collections::BTreeMap;

use cdda_protocol::{
    AmmunitionContainerPocketSnapshotV1, CraftItemPrototypeV1, CreatureCorpseSnapshotV1,
    IntegralMagazinePocketSnapshotV1, ItemComponentSnapshotV1, ItemGroupDefinitionV1,
    ItemGroupEntryV1, ItemGroupGraphV1, ItemGroupKindV1, ItemGroupSourceV1, ItemGroupTargetV1,
    ItemGroupVariantOptionV1, ItemId, ItemSnapshot, ItemVariantV1, MAX_ITEM_GROUP_DEPTH,
    MAX_ITEM_GROUP_OUTPUTS, MILLIJOULES_PER_BATTERY_CHARGE, MagazineWellSnapshotV1,
    PoweredToolStateV1, RangedWeaponSnapshot,
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
    pub(super) raw_damage: u16,
    pub(super) variant: Option<ItemVariantV1>,
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
            raw_damage: self.raw_damage,
            variant: self.variant.clone(),
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
            raw_damage: snapshot.raw_damage,
            variant: snapshot.variant.clone(),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PlannedItemSpawn {
    pub(super) prototype: CraftItemPrototypeV1,
    pub(super) raw_damage: u16,
    pub(super) variant: Option<ItemVariantV1>,
    maximum_raw_damage: u16,
    variants: Vec<ItemGroupVariantOptionV1>,
    modifier_side_effects_supported: bool,
}

pub(super) fn plan_item_group_source(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<PlannedItemSpawn>, SimError> {
    let mut output = Vec::new();
    plan_item_group_source_into(source, item_groups, rng, &mut output, 0)?;
    Ok(output)
}

fn plan_item_group_source_into(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<PlannedItemSpawn>,
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
    output: &mut Vec<PlannedItemSpawn>,
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
    output: &mut Vec<PlannedItemSpawn>,
    depth: usize,
) -> Result<(), SimError> {
    let modifier_present = entry.raw_damage.is_some() || entry.variant_id.is_some();
    if modifier_present && matches!(&entry.target, ItemGroupTargetV1::Node(_)) {
        return Err(SimError::InvalidItem);
    }
    let count = if entry.count_min == entry.count_max {
        u64::from(entry.count_min)
    } else {
        inclusive_rng_u64(rng, u64::from(entry.count_min), u64::from(entry.count_max))
    };
    for _ in 0..count {
        let output_start = output.len();
        plan_item_group_target(
            graph,
            &entry.target,
            item_groups,
            rng,
            output,
            depth.checked_add(1).ok_or(SimError::NumericOverflow)?,
            entry,
        )?;
        if modifier_present && matches!(&entry.target, ItemGroupTargetV1::Group(_)) {
            for planned in &mut output[output_start..] {
                apply_item_group_modifier(planned, entry, rng)?;
            }
        }
    }
    Ok(())
}

fn plan_item_group_target(
    graph: &ItemGroupGraphV1,
    target: &ItemGroupTargetV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<PlannedItemSpawn>,
    depth: usize,
    entry: &ItemGroupEntryV1,
) -> Result<(), SimError> {
    if depth > MAX_ITEM_GROUP_DEPTH {
        return Err(SimError::InvalidItem);
    }
    match target {
        ItemGroupTargetV1::Item(item) => {
            let prototype = item.prototype.clone();
            // Every upstream item initializes its per-instance presentation
            // seed before the constructor body. Rust does not expose the
            // presentation feature yet, but the shared generation stream must
            // retain its phase.
            let _ = rng.next_u64();
            // item::select_itype_variant calls weighted_int_list::pick even
            // when the finalized item type has no variants, so construction
            // always consumes one full-width draw before item-group logic.
            let variant_draw = rng.next_u64();
            let variant = select_constructor_variant(&item.variants, variant_draw)?;
            // Single_item_creator always evaluates one_in(3) before testing
            // VARSIZE. Runtime admission excludes VARSIZE until FIT is stored,
            // but the draw is still part of every concrete leaf's RNG phase.
            let _ = rng.next_u64();
            let mut planned = PlannedItemSpawn {
                prototype,
                raw_damage: 0,
                variant,
                maximum_raw_damage: item.maximum_raw_damage,
                variants: item.variants.clone(),
                modifier_side_effects_supported: item.modifier_side_effects_supported,
            };
            let modifier_present = entry.raw_damage.is_some() || entry.variant_id.is_some();
            if modifier_present {
                apply_item_group_modifier_state(&mut planned, entry, rng)?;
            }
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
                planned.prototype.charges = if item.minimum_one_charge {
                    rolled.max(1)
                } else {
                    rolled
                };
            }
            if modifier_present {
                consume_item_group_modifier_dressing(&planned, rng);
            }
            if output.len()
                >= usize::try_from(MAX_ITEM_GROUP_OUTPUTS).map_err(|_| SimError::NumericOverflow)?
            {
                return Err(SimError::InvalidItem);
            }
            if validate_craft_item_prototype(&planned.prototype).is_err() {
                return Err(SimError::InvalidItem);
            }
            output.push(planned);
        }
        ItemGroupTargetV1::Group(group_id) => {
            plan_item_group_source_into(
                &ItemGroupSourceV1::Group(group_id.clone()),
                item_groups,
                rng,
                output,
                depth,
            )?;
        }
        ItemGroupTargetV1::Node(node_id) => {
            plan_item_group_node(graph, *node_id, item_groups, rng, output, depth)?;
        }
    }
    Ok(())
}

fn select_constructor_variant(
    variants: &[ItemGroupVariantOptionV1],
    draw: u64,
) -> Result<Option<ItemVariantV1>, SimError> {
    let total = variants.iter().try_fold(0_u64, |total, option| {
        total.checked_add(u64::from(option.weight))
    });
    let Some(total) = total else {
        return Err(SimError::NumericOverflow);
    };
    if total == 0 {
        return Ok(None);
    }
    let ticket = draw % total;
    let mut accumulated = 0_u64;
    variants
        .iter()
        .find_map(|option| {
            accumulated = accumulated.checked_add(u64::from(option.weight))?;
            (ticket < accumulated).then(|| option.variant.clone())
        })
        .map(Some)
        .ok_or(SimError::InvalidItem)
}

fn apply_item_group_modifier(
    planned: &mut PlannedItemSpawn,
    entry: &ItemGroupEntryV1,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    apply_item_group_modifier_state(planned, entry, rng)?;
    consume_item_group_modifier_dressing(planned, rng);
    Ok(())
}

fn apply_item_group_modifier_state(
    planned: &mut PlannedItemSpawn,
    entry: &ItemGroupEntryV1,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    if !planned.modifier_side_effects_supported {
        return Err(SimError::InvalidItem);
    }
    if let Some(damage) = entry.raw_damage {
        let rolled = if damage.minimum == damage.maximum {
            // Pinned bounded RNG still consumes a draw for a fixed damage
            // range.
            let _ = rng.next_u64();
            damage.minimum
        } else {
            u16::try_from(inclusive_rng_u64(
                rng,
                u64::from(damage.minimum),
                u64::from(damage.maximum),
            ))
            .map_err(|_| SimError::NumericOverflow)?
        };
        planned.raw_damage = rolled.min(planned.maximum_raw_damage);
    }
    if let Some(variant_id) = &entry.variant_id {
        if variant_id == "<any>" {
            // Upstream returns before selection when the type has no options;
            // otherwise `<any>` performs a second weighted selection.
            if !planned.variants.is_empty()
                && let Some(variant) =
                    select_constructor_variant(&planned.variants, rng.next_u64())?
            {
                planned.variant = Some(variant);
            }
        } else if let Some(variant) = planned
            .variants
            .iter()
            .find(|option| option.variant.id == *variant_id)
            .map(|option| option.variant.clone())
        {
            planned.variant = Some(variant);
        }
    }
    Ok(())
}

fn consume_item_group_modifier_dressing(planned: &PlannedItemSpawn, rng: &mut ChaCha8Rng) {
    if prototype_uses_magazine_dressing(&planned.prototype) {
        // Pinned Item_modifier evaluates both zero-chance ammunition and
        // magazine dressing rolls for magazines and wells.
        let _ = rng.next_u64();
        let _ = rng.next_u64();
    }
}

fn prototype_uses_magazine_dressing(prototype: &CraftItemPrototypeV1) -> bool {
    prototype.magazine_capacity > 0
        || !prototype.integral_magazines.is_empty()
        || !prototype.magazine_wells.is_empty()
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
            maximum_raw_damage: 0,
            variants: Vec::new(),
            modifier_side_effects_supported: true,
            charges: None,
            minimum_one_charge: false,
        }))
    }

    fn entry(probability: u32, event: Option<ItemGroupEventV1>, type_id: &str) -> ItemGroupEntryV1 {
        ItemGroupEntryV1 {
            probability,
            count_min: 1,
            count_max: 1,
            raw_damage: None,
            variant_id: None,
            event,
            target: leaf(type_id),
        }
    }

    fn variant(id: &str, weight: u32) -> ItemGroupVariantOptionV1 {
        ItemGroupVariantOptionV1 {
            variant: ItemVariantV1 {
                id: id.to_owned(),
                name: format!("{id} name"),
                description: format!("{id} description"),
                symbol: String::from("*"),
                color: String::from("blue"),
                ascii_picture: String::new(),
            },
            weight,
        }
    }

    #[test]
    fn direct_damage_and_explicit_variant_override_constructor_selection() {
        let mut target = leaf("variant_item");
        let ItemGroupTargetV1::Item(item) = &mut target else {
            unreachable!();
        };
        item.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        item.variants = vec![variant("blue", 1), variant("green", 1)];
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                        minimum: 1_000,
                        maximum: 1_000,
                    }),
                    variant_id: Some(String::from("green")),
                    event: None,
                    target,
                }],
            }],
        });
        let mut rng = ChaCha8Rng::seed_from_u64(41);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut rng)
            .expect("supported modifier should plan");
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].raw_damage, 1_000);
        assert_eq!(cdda_protocol::item_damage_level(planned[0].raw_damage), 2);
        assert_eq!(
            planned[0]
                .variant
                .as_ref()
                .map(|variant| variant.id.as_str()),
            Some("green")
        );
    }

    #[test]
    fn constructor_variant_uses_the_draw_after_the_presentation_seed() {
        let mut target = leaf("variant_item");
        let ItemGroupTargetV1::Item(item) = &mut target else {
            unreachable!();
        };
        item.variants = vec![variant("blue", 1), variant("green", 1)];
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: None,
                    variant_id: None,
                    event: None,
                    target,
                }],
            }],
        });
        let mut expected_rng = ChaCha8Rng::seed_from_u64(2);
        let _collection_roll = expected_rng.next_u64();
        let presentation_draw = expected_rng.next_u64();
        let variant_draw = expected_rng.next_u64();
        assert_ne!(presentation_draw % 2, variant_draw % 2);

        let mut actual_rng = ChaCha8Rng::seed_from_u64(2);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("variant item should plan");
        let expected_id = if variant_draw % 2 == 0 {
            "blue"
        } else {
            "green"
        };
        assert_eq!(
            planned[0]
                .variant
                .as_ref()
                .map(|variant| variant.id.as_str()),
            Some(expected_id)
        );
    }

    #[test]
    fn direct_modifier_rolls_charges_before_magazine_dressing() {
        let mut target = leaf("charged_magazine");
        let ItemGroupTargetV1::Item(item) = &mut target else {
            unreachable!();
        };
        item.prototype.ammunition_type = String::from("9mm");
        item.prototype.magazine_capacity = 10;
        item.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        item.charges = Some(cdda_protocol::InclusiveI32RangeV1 {
            minimum: 1,
            maximum: 4,
        });
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                        minimum: 1_000,
                        maximum: 1_000,
                    }),
                    variant_id: None,
                    event: None,
                    target,
                }],
            }],
        });
        let seed = 2;
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let _collection_roll = expected_rng.next_u64();
        let _presentation_roll = expected_rng.next_u64();
        let _constructor_variant_roll = expected_rng.next_u64();
        let _fit_roll = expected_rng.next_u64();
        let _fixed_damage_roll = expected_rng.next_u64();
        let expected_charges = inclusive_rng_u64(&mut expected_rng, 1, 4);
        let _ammunition_dressing_roll = expected_rng.next_u64();
        let _magazine_dressing_roll = expected_rng.next_u64();

        let mut wrong_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..7 {
            let _ = wrong_rng.next_u64();
        }
        let dressing_first_charges = inclusive_rng_u64(&mut wrong_rng, 1, 4);
        assert_ne!(expected_charges, dressing_first_charges);

        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("supported charged modifier should plan");
        assert_eq!(planned[0].prototype.charges, expected_charges as i32);
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn any_variant_modifier_performs_a_second_weighted_selection() {
        let mut target = leaf("variant_item");
        let ItemGroupTargetV1::Item(item) = &mut target else {
            unreachable!();
        };
        item.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        item.variants = vec![variant("blue", 1), variant("green", 1)];
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                        minimum: 0,
                        maximum: 0,
                    }),
                    variant_id: Some(String::from("<any>")),
                    event: None,
                    target,
                }],
            }],
        });
        let seed = 2;
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let _collection_roll = expected_rng.next_u64();
        let _presentation_roll = expected_rng.next_u64();
        let constructor_draw = expected_rng.next_u64();
        let _fit_roll = expected_rng.next_u64();
        let _fixed_damage_roll = expected_rng.next_u64();
        let modifier_draw = expected_rng.next_u64();
        assert_ne!(constructor_draw % 2, modifier_draw % 2);

        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("the any-variant modifier should plan");
        let expected_id = if modifier_draw % 2 == 0 {
            "blue"
        } else {
            "green"
        };
        assert_eq!(
            planned[0]
                .variant
                .as_ref()
                .map(|variant| variant.id.as_str()),
            Some(expected_id)
        );
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn zero_weight_any_variant_retains_an_explicit_child_selection() {
        let mut target = leaf("variant_item");
        let ItemGroupTargetV1::Item(item) = &mut target else {
            unreachable!();
        };
        item.variants = vec![variant("blue", 0), variant("green", 0)];
        let catalog = BTreeMap::from([(
            String::from("child"),
            ItemGroupDefinitionV1 {
                group_id: String::from("child"),
                graph: ItemGroupGraphV1 {
                    root_node: 0,
                    nodes: vec![ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: Some(String::from("blue")),
                            event: None,
                            target,
                        }],
                    }],
                },
            },
        )]);
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                        minimum: 0,
                        maximum: 0,
                    }),
                    variant_id: Some(String::from("<any>")),
                    event: None,
                    target: ItemGroupTargetV1::Group(String::from("child")),
                }],
            }],
        });
        let seed = 97;
        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&source, &catalog, &mut actual_rng)
            .expect("zero-weight any modifier should remain valid");
        assert_eq!(
            planned[0]
                .variant
                .as_ref()
                .map(|variant| variant.id.as_str()),
            Some("blue")
        );

        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..8 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn named_group_modifier_applies_to_each_completed_child_and_clamps_damage() {
        let mut damageable = leaf("coat");
        let ItemGroupTargetV1::Item(damageable_item) = &mut damageable else {
            unreachable!();
        };
        damageable_item.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        damageable_item.variants = vec![variant("worn", 1)];
        let mut undamageable = leaf("rock_stack");
        let ItemGroupTargetV1::Item(undamageable_item) = &mut undamageable else {
            unreachable!();
        };
        undamageable_item.maximum_raw_damage = 0;
        undamageable_item.variants = vec![variant("worn", 1)];
        let catalog = BTreeMap::from([(
            String::from("child"),
            ItemGroupDefinitionV1 {
                group_id: String::from("child"),
                graph: ItemGroupGraphV1 {
                    root_node: 0,
                    nodes: vec![ItemGroupNodeV1 {
                        node_id: 0,
                        kind: ItemGroupKindV1::Collection,
                        entries: vec![
                            ItemGroupEntryV1 {
                                probability: 100,
                                count_min: 1,
                                count_max: 1,
                                raw_damage: None,
                                variant_id: None,
                                event: None,
                                target: damageable,
                            },
                            ItemGroupEntryV1 {
                                probability: 100,
                                count_min: 1,
                                count_max: 1,
                                raw_damage: None,
                                variant_id: None,
                                event: None,
                                target: undamageable,
                            },
                        ],
                    }],
                },
            },
        )]);
        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    probability: 100,
                    count_min: 1,
                    count_max: 1,
                    raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                        minimum: 4_000,
                        maximum: 4_000,
                    }),
                    variant_id: Some(String::from("worn")),
                    event: None,
                    target: ItemGroupTargetV1::Group(String::from("child")),
                }],
            }],
        });
        let mut rng = ChaCha8Rng::seed_from_u64(73);
        let planned = plan_item_group_source(&source, &catalog, &mut rng)
            .expect("named modifier should plan");
        assert_eq!(planned.len(), 2);
        assert_eq!(planned[0].raw_damage, 4_000);
        assert_eq!(planned[1].raw_damage, 0);
        assert!(planned.iter().all(|planned| {
            planned
                .variant
                .as_ref()
                .is_some_and(|variant| variant.id == "worn")
        }));

        let mut unsafe_catalog = catalog;
        let unsafe_group = unsafe_catalog
            .get_mut("child")
            .expect("child group should exist");
        let ItemGroupTargetV1::Item(unsafe_leaf) =
            &mut unsafe_group.graph.nodes[0].entries[1].target
        else {
            unreachable!("fixture is a direct item")
        };
        unsafe_leaf.modifier_side_effects_supported = false;
        let mut unsafe_rng = ChaCha8Rng::seed_from_u64(73);
        assert!(
            matches!(
                plan_item_group_source(&source, &unsafe_catalog, &mut unsafe_rng),
                Err(SimError::InvalidItem)
            ),
            "simulation must fail closed if an unvalidated graph reaches an unsafe modifier leaf"
        );
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
                .map(|prototype| prototype.prototype.type_id.as_str())
                .collect::<Vec<_>>(),
            ["ordinary"]
        );

        let mut expected_rng = ChaCha8Rng::seed_from_u64(19);
        let _ = expected_rng.next_u64();
        let _ = expected_rng.next_u64();
        let _ordinary_item_seed_roll = expected_rng.next_u64();
        let _ordinary_variant_roll = expected_rng.next_u64();
        let _ordinary_fit_roll = expected_rng.next_u64();
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
                    .map(|prototype| prototype.prototype.type_id.as_str())
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
            if ticket > 3 {
                let _ordinary_item_seed_roll = expected_rng.next_u64();
                let _ordinary_variant_roll = expected_rng.next_u64();
                let _ordinary_fit_roll = expected_rng.next_u64();
            }
            assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
        }
    }
}
