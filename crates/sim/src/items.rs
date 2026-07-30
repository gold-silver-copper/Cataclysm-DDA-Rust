use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    AmmunitionContainerPocketSnapshotV1, CraftItemPrototypeV1, CreatureCorpseSnapshotV1,
    ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN, ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS,
    IntegralMagazinePocketSnapshotV1, ItemComponentSnapshotV1, ItemDescriptionExpansionV1,
    ItemDescriptionSnippetCategoryV1, ItemGroupContainerV1, ItemGroupContentsSourceV1,
    ItemGroupDefinitionV1, ItemGroupEntryV1, ItemGroupGraphV1, ItemGroupItemPrototypeV1,
    ItemGroupKindV1, ItemGroupNodeV1, ItemGroupOverflowV1, ItemGroupSourceV1, ItemGroupTargetV1,
    ItemGroupToolChargeStorageV1, ItemGroupVariantOptionV1, ItemId, ItemSnapshot, ItemSnippetV1,
    ItemTemperatureStateV1, ItemVariableValueV1, ItemVariantV1, MAX_EXPANDED_DESCRIPTION_BYTES,
    MAX_ITEM_COMPONENT_DEPTH, MAX_ITEM_GROUP_DEPTH, MAX_ITEM_GROUP_OUTPUTS, MAX_ITEM_VARIABLES,
    MILLIJOULES_PER_BATTERY_CHARGE, MagazineWellSnapshotV1, PoweredToolStateV1,
    RangedWeaponSnapshot, SimTick, SpawnPocketKindV1, initial_item_temperature_state,
    item_containment_single_charge_volume_milliliters, item_containment_volume_milliliters,
    item_containment_weight_milligrams,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use super::{
    IdAllocator, SimError, debit_integral_magazine_charges, debit_snapshot_ammunition_charges,
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
    pub(super) fitted: bool,
    pub(super) variant: Option<ItemVariantV1>,
    pub(super) snippet: Option<ItemSnippetV1>,
    pub(super) variables: BTreeMap<String, ItemVariableValueV1>,
    pub(super) melee_damage_milli: BTreeMap<String, i32>,
    pub(super) calories: i32,
    pub(super) quench: i32,
    pub(super) comestible_type: String,
    pub(super) temperature: Option<ItemTemperatureStateV1>,
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
    pub(super) containment: cdda_protocol::ItemContainmentProfileV1,
}

impl ItemInstance {
    pub(super) fn process_temperature(&mut self, current_tick: SimTick) -> Result<(), SimError> {
        process_temperature_state(&mut self.temperature, current_tick)?;
        if let Some(components) = &mut self.component_provenance {
            for component in components {
                process_component_temperature(component, current_tick)?;
            }
        }
        for pocket in &mut self.integral_magazines {
            if let Some(ammunition) = pocket.loaded_ammunition.as_deref_mut() {
                process_item_snapshot_temperature(ammunition, current_tick)?;
            }
        }
        for well in &mut self.magazine_wells {
            if let Some(magazine) = well.installed_magazine.as_deref_mut() {
                process_item_snapshot_temperature(magazine, current_tick)?;
            }
        }
        for pocket in &mut self.ammunition_containers {
            for content in &mut pocket.contents {
                process_item_snapshot_temperature(content, current_tick)?;
            }
        }
        Ok(())
    }

    pub(super) fn force_fit_if_variable_size(&mut self) {
        if item_profile_has_flag(&self.containment, "VARSIZE") {
            self.fitted = true;
        }
    }

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
            fitted: self.fitted,
            variant: self.variant.clone(),
            snippet: self.snippet.clone(),
            variables: self.variables.clone(),
            melee_damage_milli: self.melee_damage_milli.clone(),
            calories: self.calories,
            quench: self.quench,
            comestible_type: self.comestible_type.clone(),
            temperature: self.temperature,
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
            containment: self.containment.clone(),
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
            fitted: snapshot.fitted,
            variant: snapshot.variant.clone(),
            snippet: snapshot.snippet.clone(),
            variables: snapshot.variables.clone(),
            melee_damage_milli: snapshot.melee_damage_milli.clone(),
            calories: snapshot.calories,
            quench: snapshot.quench,
            comestible_type: snapshot.comestible_type.clone(),
            temperature: snapshot.temperature,
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
            containment: snapshot.containment.clone(),
        })
    }
}

fn process_temperature_state(
    state: &mut Option<ItemTemperatureStateV1>,
    current_tick: SimTick,
) -> Result<(), SimError> {
    let Some(state) = state else {
        return Ok(());
    };
    let elapsed = current_tick
        .0
        .checked_sub(state.last_check_tick.0)
        .ok_or(SimError::InvalidItem)?;
    if elapsed < ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS {
        return Ok(());
    }
    if state.specific_energy_millijoules_per_gram.is_some() {
        state.temperature_millikelvin = ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN;
        state.specific_energy_millijoules_per_gram = None;
    } else if state.temperature_millikelvin != ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN {
        return Err(SimError::InvalidItem);
    }
    state.last_check_tick = current_tick;
    Ok(())
}

pub(super) fn process_item_snapshot_temperature(
    item: &mut ItemSnapshot,
    current_tick: SimTick,
) -> Result<(), SimError> {
    process_temperature_state(&mut item.temperature, current_tick)?;
    if let Some(components) = &mut item.component_provenance {
        for component in components {
            process_component_temperature(component, current_tick)?;
        }
    }
    for pocket in &mut item.integral_magazines {
        if let Some(ammunition) = pocket.loaded_ammunition.as_deref_mut() {
            process_item_snapshot_temperature(ammunition, current_tick)?;
        }
    }
    for well in &mut item.magazine_wells {
        if let Some(magazine) = well.installed_magazine.as_deref_mut() {
            process_item_snapshot_temperature(magazine, current_tick)?;
        }
    }
    for pocket in &mut item.ammunition_containers {
        for content in &mut pocket.contents {
            process_item_snapshot_temperature(content, current_tick)?;
        }
    }
    Ok(())
}

pub(super) fn item_temperature_timestamps_are_valid(
    item: &ItemSnapshot,
    current_tick: SimTick,
) -> bool {
    temperature_timestamp_is_valid(item.temperature.as_ref(), current_tick)
        && item.component_provenance.as_ref().is_none_or(|components| {
            components.iter().all(|component| {
                component_temperature_timestamps_are_valid(component, current_tick)
            })
        })
        && item.integral_magazines.iter().all(|pocket| {
            pocket
                .loaded_ammunition
                .as_deref()
                .is_none_or(|ammunition| {
                    item_temperature_timestamps_are_valid(ammunition, current_tick)
                })
        })
        && item.magazine_wells.iter().all(|well| {
            well.installed_magazine.as_deref().is_none_or(|magazine| {
                item_temperature_timestamps_are_valid(magazine, current_tick)
            })
        })
        && item.ammunition_containers.iter().all(|pocket| {
            pocket
                .contents
                .iter()
                .all(|content| item_temperature_timestamps_are_valid(content, current_tick))
        })
}

fn temperature_timestamp_is_valid(
    state: Option<&ItemTemperatureStateV1>,
    current_tick: SimTick,
) -> bool {
    state.is_none_or(|state| state.last_check_tick <= current_tick)
}

fn component_temperature_timestamps_are_valid(
    component: &ItemComponentSnapshotV1,
    current_tick: SimTick,
) -> bool {
    temperature_timestamp_is_valid(component.temperature.as_ref(), current_tick)
        && component
            .component_provenance
            .as_ref()
            .is_none_or(|children| {
                children
                    .iter()
                    .all(|child| component_temperature_timestamps_are_valid(child, current_tick))
            })
}

fn process_component_temperature(
    component: &mut ItemComponentSnapshotV1,
    current_tick: SimTick,
) -> Result<(), SimError> {
    process_temperature_state(&mut component.temperature, current_tick)?;
    if let Some(children) = &mut component.component_provenance {
        for child in children {
            process_component_temperature(child, current_tick)?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PlannedItemSpawn {
    pub(super) prototype: CraftItemPrototypeV1,
    pub(super) raw_damage: u16,
    pub(super) fitted: bool,
    pub(super) variant: Option<ItemVariantV1>,
    maximum_raw_damage: u16,
    variants: Vec<ItemGroupVariantOptionV1>,
    pub(super) snippet: Option<ItemSnippetV1>,
    pub(super) initial_variables: BTreeMap<String, ItemVariableValueV1>,
    default_container: Option<ItemGroupContainerV1>,
    modifier_side_effects_supported: bool,
    charges_supported: bool,
    modifier_container_capacity_applies: bool,
    tool_charge_storage: Option<ItemGroupToolChargeStorageV1>,
    minimum_one_charge: bool,
    default_charge_range: Option<cdda_protocol::InclusiveI32RangeV1>,
    pub(super) pocket_contents: BTreeMap<u16, Vec<PlannedItemSpawn>>,
    pub(super) sealed_pockets: BTreeSet<u16>,
    pub(super) integral_ammunition: BTreeMap<u16, Box<PlannedItemSpawn>>,
    pub(super) detachable_magazines: BTreeMap<u16, Box<PlannedItemSpawn>>,
}

pub(super) fn item_from_craft_prototype(
    id: ItemId,
    prototype: &CraftItemPrototypeV1,
    birth_tick: SimTick,
) -> ItemInstance {
    ItemInstance {
        id,
        type_id: prototype.type_id.clone(),
        charges: prototype.charges,
        damage: 0,
        raw_damage: 0,
        fitted: item_profile_has_flag(&prototype.containment, "FIT"),
        variant: None,
        snippet: None,
        variables: BTreeMap::new(),
        melee_damage_milli: prototype.melee_damage_milli.clone(),
        calories: prototype.calories,
        quench: prototype.quench,
        comestible_type: prototype.comestible_type.clone(),
        temperature: prototype
            .tracks_temperature
            .then(|| initial_item_temperature_state(birth_tick, prototype.containment.phase)),
        ammunition_type: prototype.ammunition_type.clone(),
        ranged_weapon: prototype.ranged_weapon.clone(),
        component_provenance: None,
        magazine_capacity: prototype.magazine_capacity,
        integral_magazines: prototype
            .integral_magazines
            .iter()
            .map(|pocket| IntegralMagazinePocketSnapshotV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                ammunition_type: pocket.ammunition_type.clone(),
                capacity: pocket.capacity,
                rigid: pocket.rigid,
                reloadable: pocket.reloadable,
                unloadable: pocket.unloadable,
                loaded_ammunition: None,
                residual_energy_millijoules: 0,
            })
            .collect(),
        magazine_wells: prototype
            .magazine_wells
            .iter()
            .map(|well| MagazineWellSnapshotV1 {
                pocket_index: well.pocket_index,
                pocket_id: well.pocket_id.clone(),
                compatible_magazine_type_ids: well.compatible_magazine_type_ids.clone(),
                rigid: well.rigid,
                unloadable: well.unloadable,
                installed_magazine: None,
            })
            .collect(),
        ammunition_containers: prototype
            .ammunition_containers
            .iter()
            .map(|pocket| AmmunitionContainerPocketSnapshotV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                capacities: pocket.capacities.clone(),
                access_moves: pocket.access_moves,
                rigid: pocket.rigid,
                reloadable: pocket.reloadable,
                unloadable: pocket.unloadable,
                spawn_state: pocket.spawn_rules.clone().map(|rules| {
                    cdda_protocol::SpawnPocketStateV1 {
                        rules,
                        sealed: false,
                    }
                }),
                contents: Vec::new(),
            })
            .collect(),
        residual_energy_millijoules: prototype.residual_energy_millijoules,
        powered_tool: prototype.powered_tool.clone(),
        creature_corpse: None,
        containment: prototype.containment.clone(),
    }
}

pub(super) fn item_from_component(id: ItemId, component: &ItemComponentSnapshotV1) -> ItemInstance {
    ItemInstance {
        id,
        type_id: component.type_id.clone(),
        charges: component.charges,
        damage: component.damage,
        raw_damage: component.raw_damage,
        fitted: component.fitted,
        variant: component.variant.clone(),
        snippet: component.snippet.clone(),
        variables: component.variables.clone(),
        melee_damage_milli: component.melee_damage_milli.clone(),
        calories: component.calories,
        quench: component.quench,
        comestible_type: component.comestible_type.clone(),
        temperature: component.temperature,
        ammunition_type: component.ammunition_type.clone(),
        ranged_weapon: component.ranged_weapon.clone(),
        component_provenance: component.component_provenance.clone(),
        magazine_capacity: component.magazine_capacity,
        integral_magazines: component
            .integral_magazines
            .iter()
            .map(|pocket| IntegralMagazinePocketSnapshotV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                ammunition_type: pocket.ammunition_type.clone(),
                capacity: pocket.capacity,
                rigid: pocket.rigid,
                reloadable: pocket.reloadable,
                unloadable: pocket.unloadable,
                loaded_ammunition: None,
                residual_energy_millijoules: 0,
            })
            .collect(),
        magazine_wells: component
            .magazine_wells
            .iter()
            .map(|well| MagazineWellSnapshotV1 {
                pocket_index: well.pocket_index,
                pocket_id: well.pocket_id.clone(),
                compatible_magazine_type_ids: well.compatible_magazine_type_ids.clone(),
                rigid: well.rigid,
                unloadable: well.unloadable,
                installed_magazine: None,
            })
            .collect(),
        ammunition_containers: component
            .ammunition_containers
            .iter()
            .map(|pocket| AmmunitionContainerPocketSnapshotV1 {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                capacities: pocket.capacities.clone(),
                access_moves: pocket.access_moves,
                rigid: pocket.rigid,
                reloadable: pocket.reloadable,
                unloadable: pocket.unloadable,
                spawn_state: pocket.spawn_rules.clone().map(|rules| {
                    cdda_protocol::SpawnPocketStateV1 {
                        rules,
                        sealed: false,
                    }
                }),
                contents: Vec::new(),
            })
            .collect(),
        residual_energy_millijoules: component.residual_energy_millijoules,
        powered_tool: component.powered_tool.clone(),
        creature_corpse: None,
        containment: component.containment.clone(),
    }
}

pub(super) fn item_from_planned_spawn(
    id: ItemId,
    planned: &PlannedItemSpawn,
    allocator: &mut IdAllocator,
    birth_tick: SimTick,
) -> Result<ItemInstance, SimError> {
    let mut item = item_from_craft_prototype(id, &planned.prototype, birth_tick);
    item.raw_damage = planned.raw_damage;
    item.damage = cdda_protocol::item_damage_level(planned.raw_damage);
    item.fitted = planned.fitted;
    item.variant.clone_from(&planned.variant);
    item.snippet.clone_from(&planned.snippet);
    item.variables.clone_from(&planned.initial_variables);
    for (pocket_index, ammunition) in &planned.integral_ammunition {
        let pocket = item
            .integral_magazines
            .iter_mut()
            .find(|pocket| pocket.pocket_index == *pocket_index)
            .ok_or(SimError::InvalidItem)?;
        let ammunition_id = allocator.allocate_item()?;
        pocket.loaded_ammunition = Some(Box::new(
            item_from_planned_spawn(ammunition_id, ammunition, allocator, birth_tick)?.snapshot(),
        ));
    }
    for (pocket_index, magazine) in &planned.detachable_magazines {
        let pocket = item
            .magazine_wells
            .iter_mut()
            .find(|pocket| pocket.pocket_index == *pocket_index)
            .ok_or(SimError::InvalidItem)?;
        let magazine_id = allocator.allocate_item()?;
        pocket.installed_magazine = Some(Box::new(
            item_from_planned_spawn(magazine_id, magazine, allocator, birth_tick)?.snapshot(),
        ));
    }
    for (pocket_index, contents) in &planned.pocket_contents {
        let pocket = item
            .ammunition_containers
            .iter_mut()
            .find(|pocket| pocket.pocket_index == *pocket_index)
            .ok_or(SimError::InvalidItem)?;
        for content in contents {
            let content_id = allocator.allocate_item()?;
            pocket.contents.push(
                item_from_planned_spawn(content_id, content, allocator, birth_tick)?.snapshot(),
            );
        }
    }
    for pocket_index in &planned.sealed_pockets {
        let state = item
            .ammunition_containers
            .iter_mut()
            .find(|pocket| pocket.pocket_index == *pocket_index)
            .and_then(|pocket| pocket.spawn_state.as_mut())
            .ok_or(SimError::InvalidItem)?;
        if !state.rules.sealable {
            return Err(SimError::InvalidItem);
        }
        state.sealed = true;
    }
    validate_item_snapshot(&item.snapshot())?;
    Ok(item)
}

pub(super) fn plan_item_group_source(
    source: &ItemGroupSourceV1,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<PlannedItemSpawn>, SimError> {
    let mut output = Vec::new();
    plan_item_group_source_into(source, item_groups, rng, &mut output, 0)?;
    if output.iter().any(|item| {
        item.containment_depth()
            .is_none_or(|depth| depth > MAX_ITEM_COMPONENT_DEPTH)
    }) {
        return Err(SimError::InvalidItem);
    }
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
    let output_start = output.len();
    plan_item_group_node(graph, graph.root_node, item_groups, rng, output, depth)?;
    if let Some(wrapper) = &graph.wrapper {
        wrap_item_group_output(output, output_start, wrapper, rng)?;
    }
    validate_planned_output_bound(output)
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
    let modifier_present = entry.raw_damage.is_some()
        || entry.variant_id.is_some()
        || entry.modifier_charges.is_some()
        || !entry.contents.is_empty()
        || entry.seal_contents
        || entry.modifier_default_container_sealed.is_some()
        || entry.modifier_container.is_some();
    if modifier_present && matches!(&entry.target, ItemGroupTargetV1::Node(_)) {
        return Err(SimError::InvalidItem);
    }
    let count = if entry.count_min == entry.count_max {
        u64::from(entry.count_min)
    } else {
        inclusive_rng_u64(rng, u64::from(entry.count_min), u64::from(entry.count_max))
    };
    let wrapped_output_start = output.len();
    for _ in 0..count {
        let iteration_output_start = output.len();
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
            for planned in &mut output[iteration_output_start..] {
                apply_item_group_modifier(
                    planned,
                    entry,
                    entry.modifier_charges,
                    item_groups,
                    rng,
                )?;
            }
        }
    }
    if let Some(wrapper) = &entry.direct_wrapper {
        // Pinned `Single_item_creator::create` wraps the complete count result
        // once, including the zero-count case where it emits one empty wrapper.
        wrap_item_group_output(output, wrapped_output_start, wrapper, rng)?;
    }
    validate_planned_output_bound(output)
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
            let mut planned = construct_item_group_item(item, rng)?;
            let modifier_present = entry.raw_damage.is_some()
                || entry.variant_id.is_some()
                || item.charges.is_some()
                || !entry.contents.is_empty()
                || entry.seal_contents
                || entry.modifier_default_container_sealed.is_some()
                || entry.modifier_container.is_some();
            if modifier_present {
                apply_item_group_modifier(&mut planned, entry, item.charges, item_groups, rng)?;
            } else {
                apply_unmodified_default_container(&mut planned, rng)?;
            }
            output.push(planned);
            validate_planned_output_bound(output)?;
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
) -> Result<Option<ItemGroupVariantOptionV1>, SimError> {
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
            (ticket < accumulated).then(|| option.clone())
        })
        .map(Some)
        .ok_or(SimError::InvalidItem)
}

fn construct_item_group_item(
    item: &ItemGroupItemPrototypeV1,
    rng: &mut ChaCha8Rng,
) -> Result<PlannedItemSpawn, SimError> {
    construct_item_group_item_with_fit_phase(item, rng, true)
}

fn construct_item_group_item_with_fit_phase(
    item: &ItemGroupItemPrototypeV1,
    rng: &mut ChaCha8Rng,
    consumes_fit_phase: bool,
) -> Result<PlannedItemSpawn, SimError> {
    if validate_craft_item_prototype(&item.prototype).is_err() {
        return Err(SimError::InvalidItem);
    }
    let generates_description = item.description_expansion.is_some()
        || item
            .variants
            .iter()
            .any(|variant| variant.description_expansion.is_some());
    if item.initial_variables.len() > MAX_ITEM_VARIABLES
        || (generates_description
            && !item.initial_variables.contains_key("description")
            && item.initial_variables.len() == MAX_ITEM_VARIABLES)
    {
        return Err(SimError::InvalidItem);
    }
    // Every item constructor retains presentation and finalized-variant RNG.
    // Only the item-group creator layer performs the later variable-size FIT
    // phase; raw wrapper construction does not.
    let _ = rng.next_u64();
    let selected_variant = select_constructor_variant(&item.variants, rng.next_u64())?;
    let variant = selected_variant
        .as_ref()
        .map(|option| option.variant.clone());
    let mut initial_variables = item.initial_variables.clone();
    if let Some(expansion) = selected_variant
        .as_ref()
        .and_then(|option| option.description_expansion.as_ref())
    {
        set_description_variable(
            &mut initial_variables,
            expand_item_description(expansion, rng)?,
        )?;
    }
    let snippet = if item.snippets.is_empty() {
        None
    } else {
        let index = usize::try_from(rng.next_u64() % item.snippets.len() as u64)
            .map_err(|_| SimError::NumericOverflow)?;
        Some(
            item.snippets
                .get(index)
                .ok_or(SimError::InvalidItem)?
                .clone(),
        )
    };
    if let Some(expansion) = &item.description_expansion {
        set_description_variable(
            &mut initial_variables,
            expand_item_description(expansion, rng)?,
        )?;
    }
    if let Some(expansion) = selected_variant
        .as_ref()
        .and_then(|option| option.description_expansion.as_ref())
    {
        set_description_variable(
            &mut initial_variables,
            expand_item_description(expansion, rng)?,
        )?;
    }
    let initially_fitted = item_profile_has_flag(&item.prototype.containment, "FIT");
    let fitted = if consumes_fit_phase {
        let roll_succeeded = rng.next_u64().is_multiple_of(3);
        item_group_fitted_after_phase(
            item_profile_has_flag(&item.prototype.containment, "VARSIZE"),
            initially_fitted,
            roll_succeeded,
        )
    } else {
        initially_fitted
    };
    Ok(PlannedItemSpawn {
        prototype: item.prototype.clone(),
        raw_damage: 0,
        fitted,
        variant,
        maximum_raw_damage: item.maximum_raw_damage,
        variants: item.variants.clone(),
        snippet,
        initial_variables,
        default_container: item.default_container.clone(),
        modifier_side_effects_supported: item.modifier_side_effects_supported,
        charges_supported: item.charges_supported,
        modifier_container_capacity_applies: item.modifier_container_capacity_applies,
        tool_charge_storage: item.tool_charge_storage.clone(),
        minimum_one_charge: item.minimum_one_charge,
        default_charge_range: item.charges,
        pocket_contents: BTreeMap::new(),
        sealed_pockets: BTreeSet::new(),
        integral_ammunition: BTreeMap::new(),
        detachable_magazines: BTreeMap::new(),
    })
}

/// Pure transition used by the production item-group constructor and the
/// direct C++ differential projection. The phase always consumes its roll;
/// only variable-size items gain `FIT`, and prior fitted state is idempotent.
#[must_use]
pub fn item_group_fitted_after_phase(
    variable_size: bool,
    already_fitted: bool,
    one_in_three_succeeded: bool,
) -> bool {
    already_fitted || (variable_size && one_in_three_succeeded)
}

/// Direct, renderer-free projection of the generalized integral-ammunition
/// transition used by the pinned C++ differential comparator. This executes
/// the production constructor and charge planner; it does not duplicate their
/// clamp or empty-ammunition rules in tooling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemGroupIntegralChargeProjection {
    pub item_type: String,
    pub ammunition_type: Option<String>,
    pub ammunition_remaining: i32,
    pub remaining_capacity: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemGroupDefaultContainerMode {
    Unmodified,
    ModifierFallback {
        sealed: bool,
    },
    ModifierSuppressed,
    ModifierExplicit {
        container: ItemGroupContainerV1,
    },
    GroupWrapperExplicitNull {
        container: ItemGroupContainerV1,
        count: u16,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemGroupDefaultContainerProjection {
    pub outer_type: String,
    pub content_types: Vec<String>,
    pub payload_charges: Option<i32>,
    pub sealed: bool,
}

/// Renderer-free direct projection used by the pinned C++ differential
/// comparator. It executes the production constructor, modifier fallback, and
/// default-container insertion paths rather than duplicating their semantics
/// in tooling.
pub fn item_group_default_container_projection(
    item: &ItemGroupItemPrototypeV1,
    mode: ItemGroupDefaultContainerMode,
) -> Result<ItemGroupDefaultContainerProjection, SimError> {
    let mut rng = ChaCha8Rng::from_seed([0; 32]);
    let planned = match mode {
        ItemGroupDefaultContainerMode::Unmodified => {
            let mut planned = construct_item_group_item(item, &mut rng)?;
            apply_unmodified_default_container(&mut planned, &mut rng)?;
            planned
        }
        ItemGroupDefaultContainerMode::ModifierFallback { sealed } => {
            let mut planned = construct_item_group_item(item, &mut rng)?;
            let entry = direct_default_container_projection_entry(Some(sealed));
            apply_item_group_modifier(&mut planned, &entry, None, &BTreeMap::new(), &mut rng)?;
            planned
        }
        ItemGroupDefaultContainerMode::ModifierSuppressed => {
            let mut planned = construct_item_group_item(item, &mut rng)?;
            let entry = direct_default_container_projection_entry(None);
            apply_item_group_modifier(&mut planned, &entry, None, &BTreeMap::new(), &mut rng)?;
            planned
        }
        ItemGroupDefaultContainerMode::ModifierExplicit { container } => {
            let mut planned = construct_item_group_item(item, &mut rng)?;
            let mut entry = direct_default_container_projection_entry(None);
            entry.modifier_container = Some(container);
            apply_item_group_modifier(&mut planned, &entry, None, &BTreeMap::new(), &mut rng)?;
            planned
        }
        ItemGroupDefaultContainerMode::GroupWrapperExplicitNull { container, count } => {
            let mut entry = direct_default_container_projection_entry(None);
            entry.count_min = count;
            entry.count_max = count;
            entry.target = ItemGroupTargetV1::Item(Box::new(item.clone()));
            let graph = ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![entry],
                }],
                wrapper: Some(container),
            };
            let mut output = Vec::new();
            plan_item_group_source_into(
                &ItemGroupSourceV1::Inline(graph),
                &BTreeMap::new(),
                &mut rng,
                &mut output,
                0,
            )?;
            let [planned] = output.try_into().map_err(|_| SimError::InvalidItem)?;
            planned
        }
    };
    let payloads = planned
        .pocket_contents
        .values()
        .flatten()
        .collect::<Vec<_>>();
    let payload_charges = payloads.first().map(|payload| payload.prototype.charges);
    if let Some(payload_charges) = payload_charges
        && payloads
            .iter()
            .any(|payload| payload.prototype.charges != payload_charges)
    {
        return Err(SimError::InvalidItem);
    }
    Ok(ItemGroupDefaultContainerProjection {
        outer_type: planned.prototype.type_id,
        content_types: payloads
            .iter()
            .map(|payload| payload.prototype.type_id.clone())
            .collect(),
        payload_charges,
        sealed: !planned.sealed_pockets.is_empty(),
    })
}

fn direct_default_container_projection_entry(sealed: Option<bool>) -> ItemGroupEntryV1 {
    ItemGroupEntryV1 {
        probability: 100,
        count_min: 1,
        count_max: 1,
        raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
            minimum: 0,
            maximum: 0,
        }),
        variant_id: None,
        event: None,
        modifier_charges: None,
        contents: Vec::new(),
        seal_contents: false,
        modifier_default_container_sealed: sealed,
        direct_wrapper: None,
        modifier_container: None,
        target: ItemGroupTargetV1::Node(0),
    }
}

pub fn item_group_integral_charge_projection(
    item: &ItemGroupItemPrototypeV1,
    requested_charges: i32,
) -> Result<ItemGroupIntegralChargeProjection, SimError> {
    if requested_charges < 0
        || !matches!(
            item.tool_charge_storage,
            Some(ItemGroupToolChargeStorageV1::Integral { .. })
        )
    {
        return Err(SimError::InvalidItem);
    }
    let mut rng = ChaCha8Rng::from_seed([0; 32]);
    let mut planned = construct_item_group_item(item, &mut rng)?;
    apply_item_group_charges(
        &mut planned,
        Some(cdda_protocol::InclusiveI32RangeV1 {
            minimum: requested_charges,
            maximum: requested_charges,
        }),
        None,
        &mut rng,
    )?;
    let [pocket] = planned.prototype.integral_magazines.as_slice() else {
        return Err(SimError::InvalidItem);
    };
    let loaded = planned.integral_ammunition.get(&pocket.pocket_index);
    let ammunition_remaining = loaded.map_or(0, |ammunition| ammunition.prototype.charges);
    let ammunition_type = loaded.map(|ammunition| ammunition.prototype.ammunition_type.clone());
    let loaded = u32::try_from(ammunition_remaining).map_err(|_| SimError::InvalidItem)?;
    Ok(ItemGroupIntegralChargeProjection {
        item_type: planned.prototype.type_id,
        ammunition_type,
        ammunition_remaining,
        remaining_capacity: pocket.capacity.saturating_sub(loaded),
    })
}

pub(super) fn item_profile_has_flag(
    profile: &cdda_protocol::ItemContainmentProfileV1,
    expected: &str,
) -> bool {
    profile
        .flags
        .binary_search_by(|flag| flag.as_str().cmp(expected))
        .is_ok()
}

pub(super) fn item_fit_state_is_valid(
    fitted: bool,
    profile: &cdda_protocol::ItemContainmentProfileV1,
) -> bool {
    let immutable_fit = item_profile_has_flag(profile, "FIT");
    let variable_size = item_profile_has_flag(profile, "VARSIZE");
    (!immutable_fit || fitted) && (!fitted || immutable_fit || variable_size)
}

fn construct_charge_ammunition(
    prototype: &CraftItemPrototypeV1,
    charges: i32,
    rng: &mut ChaCha8Rng,
) -> Result<PlannedItemSpawn, SimError> {
    let _ = rng.next_u64();
    let _ = rng.next_u64();
    let mut prototype = prototype.clone();
    prototype.charges = charges;
    if validate_craft_item_prototype(&prototype).is_err() {
        return Err(SimError::InvalidItem);
    }
    let fitted = item_profile_has_flag(&prototype.containment, "FIT");
    Ok(PlannedItemSpawn {
        prototype,
        raw_damage: 0,
        fitted,
        variant: None,
        maximum_raw_damage: 0,
        variants: Vec::new(),
        snippet: None,
        initial_variables: BTreeMap::new(),
        default_container: None,
        modifier_side_effects_supported: true,
        charges_supported: true,
        modifier_container_capacity_applies: false,
        tool_charge_storage: None,
        minimum_one_charge: true,
        default_charge_range: None,
        pocket_contents: BTreeMap::new(),
        sealed_pockets: BTreeSet::new(),
        integral_ammunition: BTreeMap::new(),
        detachable_magazines: BTreeMap::new(),
    })
}

pub fn expand_item_description<R: Rng + ?Sized>(
    expansion: &ItemDescriptionExpansionV1,
    rng: &mut R,
) -> Result<String, SimError> {
    let expanded = expand_description_text(&expansion.template, &expansion.categories, rng, 0)?;
    if expanded.len() > MAX_EXPANDED_DESCRIPTION_BYTES {
        return Err(SimError::InvalidItem);
    }
    Ok(expanded)
}

fn set_description_variable(
    variables: &mut BTreeMap<String, ItemVariableValueV1>,
    description: String,
) -> Result<(), SimError> {
    if !variables.contains_key("description") && variables.len() >= MAX_ITEM_VARIABLES {
        return Err(SimError::InvalidItem);
    }
    variables.insert(
        String::from("description"),
        ItemVariableValueV1::String(description),
    );
    Ok(())
}

fn expand_description_text<R: Rng + ?Sized>(
    text: &str,
    categories: &[ItemDescriptionSnippetCategoryV1],
    rng: &mut R,
    depth: usize,
) -> Result<String, SimError> {
    if depth > cdda_protocol::MAX_DESCRIPTION_SNIPPET_DEPTH {
        return Err(SimError::InvalidItem);
    }
    let mut output = String::new();
    let mut remaining = text;
    loop {
        let Some(begin) = remaining.find('<') else {
            output.push_str(remaining);
            break;
        };
        let Some(relative_end) = remaining[begin + 1..].find('>') else {
            output.push_str(remaining);
            break;
        };
        let end = begin
            .checked_add(relative_end)
            .and_then(|end| end.checked_add(2))
            .ok_or(SimError::NumericOverflow)?;
        let tag = &remaining[begin..end];
        output.push_str(&remaining[..begin]);
        let Some(category) = categories.iter().find(|category| category.category == tag) else {
            output.push_str(tag);
            remaining = &remaining[end..];
            continue;
        };
        // Pinned `random_from_category` obtains a fresh seed even for a
        // one-choice or zero-total category. Rust intentionally uses its
        // canonical RNG while retaining that one-draw phase boundary.
        let draw = rng.next_u64();
        let total = category
            .choices
            .iter()
            .try_fold(0_u64, |total, choice| total.checked_add(choice.weight))
            .ok_or(SimError::NumericOverflow)?;
        let replacement = if total == 0 {
            None
        } else {
            let ticket = draw % total;
            let mut accumulated = 0_u64;
            category.choices.iter().find_map(|choice| {
                accumulated = accumulated.checked_add(choice.weight)?;
                (ticket < accumulated).then_some(choice.text.as_str())
            })
        };
        if let Some(replacement) = replacement {
            output.push_str(&expand_description_text(
                replacement,
                categories,
                rng,
                depth.checked_add(1).ok_or(SimError::NumericOverflow)?,
            )?);
        } else {
            output.push_str(tag);
        }
        if output.len() > MAX_EXPANDED_DESCRIPTION_BYTES {
            return Err(SimError::InvalidItem);
        }
        remaining = &remaining[end..];
    }
    if output.len() > MAX_EXPANDED_DESCRIPTION_BYTES {
        return Err(SimError::InvalidItem);
    }
    Ok(output)
}

fn apply_item_group_modifier(
    planned: &mut PlannedItemSpawn,
    entry: &ItemGroupEntryV1,
    charges: Option<cdda_protocol::InclusiveI32RangeV1>,
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    // Pinned `Item_modifier::modify` applies item state first, constructs the
    // modifier container before charge/dressing RNG, and inserts the payload
    // only after those phases have completed.
    apply_item_group_modifier_state(planned, entry, rng)?;
    let active_wrapper = if let Some(wrapper) = &entry.modifier_container {
        Some((wrapper.clone(), true))
    } else if let (Some(sealed), Some(wrapper)) = (
        entry.modifier_default_container_sealed,
        planned.default_container.clone(),
    ) {
        Some((ItemGroupContainerV1 { sealed, ..wrapper }, false))
    } else {
        None
    };
    let modifier_container = active_wrapper
        .as_ref()
        .map(|(wrapper, consumes_fit_phase)| {
            construct_item_group_container(wrapper, rng, *consumes_fit_phase)
        })
        .transpose()?;
    let modifier_container_capacity = modifier_container
        .as_ref()
        .map(|container| modifier_container_charge_capacity(planned, container))
        .transpose()?
        .flatten();
    apply_item_group_charges(planned, charges, modifier_container_capacity, rng)?;
    consume_item_group_modifier_dressing(planned, rng);
    if let (Some(container), Some((wrapper, _))) = (modifier_container, active_wrapper.as_ref()) {
        wrap_single_item(planned, container, wrapper)?;
    }
    insert_item_group_contents(planned, &entry.contents, item_groups, rng)?;
    if entry.seal_contents && !planned.prototype.comestible_type.is_empty() {
        seal_planned_item(planned)?;
    }
    Ok(())
}

fn apply_item_group_charges(
    planned: &mut PlannedItemSpawn,
    charges: Option<cdda_protocol::InclusiveI32RangeV1>,
    modifier_container_capacity: Option<i32>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let Some(mut charges) = charges else {
        if planned.prototype.containment.phase == cdda_protocol::ItemPhaseV1::Liquid
            && let Some(capacity) = modifier_container_capacity
        {
            planned.prototype.charges = capacity.max(1);
        }
        return Ok(());
    };
    if !planned.charges_supported || charges.minimum < 0 || charges.maximum < charges.minimum {
        return Err(SimError::InvalidItem);
    }
    if let Some(capacity) = modifier_container_capacity {
        charges.maximum = charges.maximum.min(capacity);
        charges.minimum = charges.minimum.min(charges.maximum);
    }
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
    // Pinned Item_modifier clamps every count-by-charges item and every
    // liquid to one even when the charge range belongs to an outer named
    // group entry. `minimum_one_charge` only records the equivalent rule for
    // a leaf-local default range; it cannot describe later modifiers.
    let rolled = if planned.minimum_one_charge
        || planned.prototype.containment.count_by_charges
        || planned.prototype.containment.phase == cdda_protocol::ItemPhaseV1::Liquid
    {
        rolled.max(1)
    } else {
        rolled
    };
    match planned.tool_charge_storage.clone() {
        Some(ItemGroupToolChargeStorageV1::Integral { ammunition }) => {
            let [pocket] = planned.prototype.integral_magazines.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            planned.integral_ammunition.remove(&pocket.pocket_index);
            if rolled > 0 {
                let capacity =
                    i32::try_from(pocket.capacity).map_err(|_| SimError::NumericOverflow)?;
                let loaded = construct_charge_ammunition(&ammunition, rolled.min(capacity), rng)?;
                planned
                    .integral_ammunition
                    .insert(pocket.pocket_index, Box::new(loaded));
            }
        }
        Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index,
            magazine,
            ammunition,
            ..
        }) => {
            let [well] = planned.prototype.magazine_wells.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            if well.pocket_index != well_pocket_index
                || well
                    .compatible_magazine_type_ids
                    .binary_search(&magazine.type_id)
                    .is_err()
            {
                return Err(SimError::InvalidItem);
            }
            let [magazine_pocket] = magazine.integral_magazines.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            let capacity =
                i32::try_from(magazine_pocket.capacity).map_err(|_| SimError::NumericOverflow)?;
            let mut loaded_magazine = match planned.detachable_magazines.remove(&well_pocket_index)
            {
                Some(installed) if installed.prototype == magazine => *installed,
                Some(_) => return Err(SimError::InvalidItem),
                None => construct_charge_ammunition(&magazine, 0, rng)?,
            };
            loaded_magazine
                .integral_ammunition
                .remove(&magazine_pocket.pocket_index);
            if rolled > 0 {
                let loaded = construct_charge_ammunition(&ammunition, rolled.min(capacity), rng)?;
                loaded_magazine
                    .integral_ammunition
                    .insert(magazine_pocket.pocket_index, Box::new(loaded));
            }
            planned
                .detachable_magazines
                .insert(well_pocket_index, Box::new(loaded_magazine));
        }
        None => planned.prototype.charges = rolled,
    }
    Ok(())
}

fn modifier_container_charge_capacity(
    planned: &PlannedItemSpawn,
    container: &PlannedItemSpawn,
) -> Result<Option<i32>, SimError> {
    if !planned.modifier_container_capacity_applies || planned.tool_charge_storage.is_some() {
        // Ammunition capacity is owned by the integral pocket or detachable
        // magazine rather than by the modifier container in pinned
        // Item_modifier.
        return Ok(None);
    }
    let mut physical = container
        .prototype
        .ammunition_containers
        .iter()
        .filter_map(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .filter(|rules| rules.kind == SpawnPocketKindV1::Container)
        });
    let rules = physical.next().ok_or(SimError::InvalidItem)?;
    if physical.next().is_some() {
        return Err(SimError::InvalidItem);
    }
    let profile = &planned.prototype.containment;
    let volume_capacity = if profile.count_by_charges {
        match profile.volume_milliliters {
            0 => u64::try_from(i32::MAX).expect("i32::MAX fits u64"),
            divisor => {
                rules
                    .max_contains_volume_milliliters
                    .checked_mul(u64::from(profile.stack_size))
                    .ok_or(SimError::NumericOverflow)?
                    / divisor
            }
        }
    } else {
        let volume = planned
            .total_volume_milliliters()
            .ok_or(SimError::NumericOverflow)?;
        rules
            .max_contains_volume_milliliters
            .checked_div(volume)
            .unwrap_or_else(|| u64::try_from(i32::MAX).expect("i32::MAX fits u64"))
    };
    let weight_capacity = if profile.weight_milligrams == 0 {
        u64::try_from(i32::MAX).expect("i32::MAX fits u64")
    } else {
        let weight = if profile.count_by_charges {
            profile.weight_milligrams
        } else {
            planned
                .total_weight_milligrams()
                .ok_or(SimError::NumericOverflow)?
        };
        rules
            .max_contains_weight_milligrams
            .checked_div(weight)
            .unwrap_or_else(|| u64::try_from(i32::MAX).expect("i32::MAX fits u64"))
    };
    let capacity = volume_capacity
        .min(weight_capacity)
        .min(u64::try_from(i32::MAX).expect("i32::MAX fits u64"));
    Ok(Some(
        i32::try_from(capacity).map_err(|_| SimError::NumericOverflow)?,
    ))
}

fn apply_unmodified_default_container(
    planned: &mut PlannedItemSpawn,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let Some(wrapper) = planned.default_container.clone() else {
        return Ok(());
    };
    let mut container = construct_item_group_container(&wrapper, rng, false)?;
    let mut payload = planned.clone();
    if payload.prototype.containment.count_by_charges
        || payload.prototype.containment.phase == cdda_protocol::ItemPhaseV1::Liquid
    {
        let Some(capacity) = physical_container_charge_capacity(&payload, &container)? else {
            return Err(SimError::InvalidItem);
        };
        if capacity <= 0 {
            return Ok(());
        }
        payload.prototype.charges =
            if payload.prototype.containment.phase == cdda_protocol::ItemPhaseV1::Liquid {
                capacity.max(1)
            } else {
                payload.prototype.charges.min(capacity)
            };
        if payload.prototype.charges <= 0 {
            return Ok(());
        }
    }
    if insert_planned_item(&mut container, payload)?.is_err() {
        return Ok(());
    }
    if wrapper.sealed {
        seal_planned_item(&mut container)?;
    }
    *planned = container;
    Ok(())
}

fn physical_container_charge_capacity(
    planned: &PlannedItemSpawn,
    container: &PlannedItemSpawn,
) -> Result<Option<i32>, SimError> {
    let mut unrestricted = planned.clone();
    unrestricted.modifier_container_capacity_applies = true;
    unrestricted.tool_charge_storage = None;
    modifier_container_charge_capacity(&unrestricted, container)
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
                set_planned_variant(planned, &variant, rng)?;
            }
        } else if let Some(variant) = planned
            .variants
            .iter()
            .find(|option| option.variant.id == *variant_id)
            .cloned()
        {
            set_planned_variant(planned, &variant, rng)?;
        }
    }
    Ok(())
}

fn set_planned_variant(
    planned: &mut PlannedItemSpawn,
    option: &ItemGroupVariantOptionV1,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    planned.variant = Some(option.variant.clone());
    if let Some(expansion) = &option.description_expansion {
        let description = expand_item_description(expansion, rng)?;
        set_description_variable(&mut planned.initial_variables, description)?;
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

fn construct_item_group_container(
    wrapper: &ItemGroupContainerV1,
    rng: &mut ChaCha8Rng,
    consumes_fit_phase: bool,
) -> Result<PlannedItemSpawn, SimError> {
    if wrapper.item.charges.is_some() || wrapper.item.tool_charge_storage.is_some() {
        return Err(SimError::InvalidItem);
    }
    let mut container =
        construct_item_group_item_with_fit_phase(&wrapper.item, rng, consumes_fit_phase)?;
    if let Some(variant_id) = &wrapper.variant_id {
        let variant = container
            .variants
            .iter()
            .find(|option| option.variant.id == *variant_id)
            .cloned()
            .ok_or(SimError::InvalidItem)?;
        set_planned_variant(&mut container, &variant, rng)?;
    }
    if consumes_fit_phase {
        // An explicit Item_modifier container is itself a
        // Single_item_creator. With no nested modifier, pinned C++ applies
        // that container type's ordinary default-container constructor before
        // returning it. Raw group wrappers and type-default fallbacks instead
        // construct the named container directly and skip this phase.
        apply_unmodified_default_container(&mut container, rng)?;
    }
    Ok(container)
}

fn wrap_single_item(
    planned: &mut PlannedItemSpawn,
    container: PlannedItemSpawn,
    wrapper: &ItemGroupContainerV1,
) -> Result<(), SimError> {
    let payload = std::mem::replace(planned, container);
    let _ = insert_planned_item(planned, payload)?;
    if wrapper.sealed {
        seal_planned_item(planned)?;
    }
    Ok(())
}

fn wrap_item_group_output(
    output: &mut Vec<PlannedItemSpawn>,
    output_start: usize,
    wrapper: &ItemGroupContainerV1,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    if output_start > output.len() {
        return Err(SimError::InvalidItem);
    }
    let mut payloads = output.split_off(output_start);
    for index in (1..payloads.len()).rev() {
        let selected = usize::try_from(inclusive_rng_u64(rng, 0, index as u64))
            .map_err(|_| SimError::NumericOverflow)?;
        payloads.swap(index, selected);
    }
    // Pinned `put_into_container` uses the direct item constructor. Modifier
    // containers use `create_single` above and retain the additional FIT draw.
    let mut container = construct_item_group_container(wrapper, rng, false)?;
    let mut excess = Vec::new();
    for payload in payloads {
        if let Err(payload) = insert_planned_item(&mut container, payload)? {
            match wrapper.overflow {
                ItemGroupOverflowV1::Spill => excess.push(payload),
                ItemGroupOverflowV1::None | ItemGroupOverflowV1::Discard => {}
            }
        }
    }
    if wrapper.sealed {
        seal_planned_item(&mut container)?;
    }
    output.extend(excess);
    output.push(container);
    validate_planned_output_bound(output)
}

fn insert_item_group_contents(
    target: &mut PlannedItemSpawn,
    sources: &[ItemGroupContentsSourceV1],
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    for source in sources {
        if sources.len() > 1 {
            // Pinned `load_sub_ref` wraps multiple contents sources in a
            // probability-100 collection, whose range-100 check still
            // consumes one RNG draw per source. A single source remains a
            // direct creator and has no outer collection draw.
            let _ = rng.next_u64();
        }
        let mut contents = match source {
            ItemGroupContentsSourceV1::Item(item) => {
                let mut item = construct_item_group_item(item, rng)?;
                if let Some(charges) = item.default_charge_range {
                    apply_item_group_charges(&mut item, Some(charges), None, rng)?;
                }
                apply_unmodified_default_container(&mut item, rng)?;
                vec![item]
            }
            ItemGroupContentsSourceV1::Group(group_id) => {
                let mut contents = Vec::new();
                plan_item_group_source_into(
                    &ItemGroupSourceV1::Group(group_id.clone()),
                    item_groups,
                    rng,
                    &mut contents,
                    0,
                )?;
                contents
            }
        };
        for content in contents.drain(..) {
            let _ = insert_planned_item(target, content)?;
        }
    }
    Ok(())
}

impl PlannedItemSpawn {
    pub(super) fn object_count(&self) -> Option<u64> {
        self.pocket_contents
            .values()
            .flatten()
            .chain(self.integral_ammunition.values().map(Box::as_ref))
            .chain(self.detachable_magazines.values().map(Box::as_ref))
            .try_fold(1_u64, |total, child| {
                total.checked_add(child.object_count()?)
            })
    }

    fn containment_depth(&self) -> Option<usize> {
        self.pocket_contents
            .values()
            .flatten()
            .chain(self.integral_ammunition.values().map(Box::as_ref))
            .chain(self.detachable_magazines.values().map(Box::as_ref))
            .try_fold(0_usize, |depth, child| {
                child
                    .containment_depth()?
                    .checked_add(1)
                    .map(|child_depth| depth.max(child_depth))
            })
    }

    fn total_weight_milligrams(&self) -> Option<u64> {
        if self
            .prototype
            .containment
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("NO_DROP"))
            .is_ok()
        {
            return Some(0);
        }
        let own = item_containment_weight_milligrams(
            &self.prototype.containment,
            self.prototype.charges,
        )?;
        let integral = self
            .integral_ammunition
            .values()
            .try_fold(0_u64, |total, child| {
                total.checked_add(child.total_weight_milligrams()?)
            })?;
        let detachable = self
            .detachable_magazines
            .values()
            .try_fold(0_u64, |total, child| {
                total.checked_add(child.total_weight_milligrams()?)
            })?;
        let pocketed =
            self.pocket_contents
                .iter()
                .try_fold(0_u64, |total, (pocket_index, contents)| {
                    let pocket = self
                        .prototype
                        .ammunition_containers
                        .iter()
                        .find(|pocket| pocket.pocket_index == *pocket_index)?;
                    if pocket
                        .spawn_rules
                        .as_ref()
                        .is_some_and(|rules| rules.kind == SpawnPocketKindV1::EFileStorage)
                    {
                        return Some(total);
                    }
                    contents.iter().try_fold(total, |total, child| {
                        total.checked_add(child.total_weight_milligrams()?)
                    })
                })?;
        own.checked_add(integral)?
            .checked_add(detachable)?
            .checked_add(pocketed)
    }

    fn total_volume_milliliters(&self) -> Option<u64> {
        let own = item_containment_volume_milliliters(
            &self.prototype.containment,
            self.prototype.charges,
        )?;
        let integral =
            self.integral_ammunition
                .iter()
                .try_fold(0_u64, |total, (pocket_index, child)| {
                    let pocket = self
                        .prototype
                        .integral_magazines
                        .iter()
                        .find(|pocket| pocket.pocket_index == *pocket_index)?;
                    if pocket.rigid {
                        Some(total)
                    } else {
                        total.checked_add(child.total_volume_milliliters()?)
                    }
                })?;
        let detachable =
            self.detachable_magazines
                .iter()
                .try_fold(0_u64, |total, (pocket_index, child)| {
                    if self.detachable_well_is_rigid(*pocket_index)? {
                        Some(total)
                    } else {
                        total.checked_add(child.total_volume_milliliters()?)
                    }
                })?;
        let pocketed =
            self.pocket_contents
                .iter()
                .try_fold(0_u64, |total, (pocket_index, contents)| {
                    let pocket = self
                        .prototype
                        .ammunition_containers
                        .iter()
                        .find(|pocket| pocket.pocket_index == *pocket_index)?;
                    if pocket.rigid {
                        return Some(total);
                    }
                    contents.iter().try_fold(total, |total, child| {
                        total.checked_add(child.total_volume_milliliters()?)
                    })
                })?;
        own.checked_add(integral)?
            .checked_add(detachable)?
            .checked_add(pocketed)
    }

    fn detachable_well_is_rigid(&self, pocket_index: u16) -> Option<bool> {
        self.prototype
            .magazine_wells
            .iter()
            .find(|well| well.pocket_index == pocket_index)
            .map(|well| well.rigid)
    }

    fn has_no_contained_items(&self) -> bool {
        self.integral_ammunition.is_empty()
            && self.detachable_magazines.is_empty()
            && self.pocket_contents.values().all(std::vec::Vec::is_empty)
    }

    fn standard_contents(&self) -> Option<Vec<&PlannedItemSpawn>> {
        let mut contents = self
            .integral_ammunition
            .values()
            .map(Box::as_ref)
            .chain(self.detachable_magazines.values().map(Box::as_ref))
            .collect::<Vec<_>>();
        for (pocket_index, pocket_contents) in &self.pocket_contents {
            let pocket = self
                .prototype
                .ammunition_containers
                .iter()
                .find(|pocket| pocket.pocket_index == *pocket_index)?;
            if pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.kind == SpawnPocketKindV1::EFileStorage)
            {
                continue;
            }
            contents.extend(pocket_contents);
        }
        Some(contents)
    }

    fn containment_length_millimeters(&self) -> Option<u64> {
        let profile = &self.prototype.containment;
        let soft = profile
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("SOFT"))
            .is_ok();
        if profile.phase == cdda_protocol::ItemPhaseV1::Liquid
            || (soft && self.has_no_contained_items())
        {
            return Some(0);
        }
        self.standard_contents()?.into_iter().try_fold(
            if soft {
                0
            } else {
                profile.longest_side_millimeters
            },
            |longest, child| Some(longest.max(child.containment_length_millimeters()?)),
        )
    }

    fn soft_volume_fits(&self, maximum: u64) -> Option<bool> {
        self.standard_contents()?
            .into_iter()
            .try_fold(true, |fits, child| {
                Some(fits && child.max_item_volume_fits(maximum)?)
            })
    }

    fn max_item_volume_fits(&self, maximum: u64) -> Option<bool> {
        let profile = &self.prototype.containment;
        if matches!(
            profile.phase,
            cdda_protocol::ItemPhaseV1::Liquid | cdda_protocol::ItemPhaseV1::Gas
        ) {
            return Some(true);
        }
        let hard_fits = if profile.count_by_charges {
            item_containment_single_charge_volume_milliliters(profile)
        } else {
            self.total_volume_milliliters()
        }? <= maximum;
        let soft_fits = self.soft_volume_fits(maximum)?;
        let soft = profile
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("SOFT"))
            .is_ok();
        let hard = profile
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("HARD"))
            .is_ok();
        Some(if soft {
            soft_fits
        } else if hard {
            hard_fits
        } else {
            // Material-derived softness is not projected yet. Requiring both
            // interpretations prevents ambiguous definitions from being
            // admitted with behavior that differs from pinned C++.
            hard_fits && soft_fits
        })
    }
}

fn insert_planned_item(
    target: &mut PlannedItemSpawn,
    payload: PlannedItemSpawn,
) -> Result<Result<(), PlannedItemSpawn>, SimError> {
    let preferred_kind = if payload.prototype.containment.estorable
        && target.prototype.ammunition_containers.iter().any(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.kind == SpawnPocketKindV1::EFileStorage)
        }) {
        SpawnPocketKindV1::EFileStorage
    } else {
        SpawnPocketKindV1::Container
    };
    let pocket = target
        .prototype
        .ammunition_containers
        .iter()
        .find(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.kind == preferred_kind)
        });
    let Some(pocket) = pocket else {
        return Ok(Err(payload));
    };
    let rules = pocket.spawn_rules.as_ref().ok_or(SimError::InvalidItem)?;
    if rules.kind == SpawnPocketKindV1::Container && !rules.rigid {
        return Err(SimError::InvalidItem);
    }
    if !spawn_pocket_accepts(target, pocket.pocket_index, rules, &payload)? {
        return Ok(Err(payload));
    }
    if payload
        .containment_depth()
        .and_then(|depth| depth.checked_add(1))
        .is_none_or(|depth| depth > MAX_ITEM_COMPONENT_DEPTH)
    {
        return Err(SimError::InvalidItem);
    }
    let contents = target
        .pocket_contents
        .entry(pocket.pocket_index)
        .or_default();
    let mut payload = payload;
    if payload.prototype.containment.count_by_charges {
        let combined_charges = contents
            .iter()
            .filter(|existing| planned_items_can_combine_for_containment(existing, &payload))
            .try_fold(payload.prototype.charges, |charges, existing| {
                charges.checked_add(existing.prototype.charges)
            })
            .ok_or(SimError::NumericOverflow)?;
        contents.retain(|existing| !planned_items_can_combine_for_containment(existing, &payload));
        payload.prototype.charges = combined_charges;
    }
    contents.insert(0, payload);
    Ok(Ok(()))
}

fn spawn_pocket_accepts(
    target: &PlannedItemSpawn,
    pocket_index: u16,
    rules: &cdda_protocol::SpawnPocketRulesV1,
    payload: &PlannedItemSpawn,
) -> Result<bool, SimError> {
    let profile = &payload.prototype.containment;
    if rules.kind == SpawnPocketKindV1::EFileStorage {
        return Ok(profile.estorable);
    }
    if profile.count_by_charges
        && (!payload.pocket_contents.is_empty()
            || !payload.integral_ammunition.is_empty()
            || !payload.detachable_magazines.is_empty())
    {
        // Pinned max-item-volume recursively inspects soft count-by-charge
        // contents. Rigidity/charge apportionment for that shape is not in the
        // canonical profile yet, so retain it explicitly but fail closed.
        return Ok(false);
    }
    let restricted = !rules.item_restrictions.is_empty() || !rules.flag_restrictions.is_empty();
    let accepted_restriction = rules
        .item_restrictions
        .binary_search(&payload.prototype.type_id)
        .is_ok()
        || rules
            .flag_restrictions
            .iter()
            .any(|flag| profile.flags.binary_search(flag).is_ok());
    let compatibility_volume = if profile.count_by_charges {
        item_containment_single_charge_volume_milliliters(profile)
    } else {
        payload.total_volume_milliliters()
    }
    .ok_or(SimError::NumericOverflow)?;
    let compatibility_length = payload
        .containment_length_millimeters()
        .ok_or(SimError::NumericOverflow)?;
    if profile.phase == cdda_protocol::ItemPhaseV1::Gas
        || profile
            .flags
            .binary_search_by(|flag| flag.as_str().cmp("NO_UNWIELD"))
            .is_ok()
        || (profile.phase == cdda_protocol::ItemPhaseV1::Liquid && !rules.watertight)
        || (restricted && !accepted_restriction)
        || compatibility_volume < rules.min_item_volume_milliliters
        || payload.max_item_volume_fits(rules.max_item_volume_milliliters) != Some(true)
        || compatibility_length > rules.max_item_length_millimeters
    {
        return Ok(false);
    }
    let existing = target
        .pocket_contents
        .get(&pocket_index)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if profile.phase == cdda_protocol::ItemPhaseV1::Liquid
        && existing.iter().any(|item| {
            item.prototype.containment.phase != cdda_protocol::ItemPhaseV1::Liquid
                || !planned_items_can_combine_for_containment(item, payload)
        })
        || profile.phase != cdda_protocol::ItemPhaseV1::Liquid
            && existing
                .iter()
                .any(|item| item.prototype.containment.phase == cdda_protocol::ItemPhaseV1::Liquid)
    {
        return Ok(false);
    }
    let (volume, weight) = target
        .pocket_contents
        .get(&pocket_index)
        .into_iter()
        .flatten()
        .try_fold((0_u64, 0_u64), |(volume, weight), item| {
            Some((
                volume.checked_add(item.total_volume_milliliters()?)?,
                weight.checked_add(item.total_weight_milligrams()?)?,
            ))
        })
        .ok_or(SimError::NumericOverflow)?;
    let payload_volume = payload
        .total_volume_milliliters()
        .ok_or(SimError::NumericOverflow)?;
    let payload_weight = payload
        .total_weight_milligrams()
        .ok_or(SimError::NumericOverflow)?;
    Ok(volume
        .checked_add(payload_volume)
        .is_some_and(|volume| volume <= rules.max_contains_volume_milliliters)
        && weight
            .checked_add(payload_weight)
            .is_some_and(|weight| weight <= rules.max_contains_weight_milligrams))
}

fn planned_items_can_combine_for_containment(
    left: &PlannedItemSpawn,
    right: &PlannedItemSpawn,
) -> bool {
    if !left.prototype.containment.count_by_charges
        || !right.prototype.containment.count_by_charges
        || !left.pocket_contents.is_empty()
        || !right.pocket_contents.is_empty()
        || !left.integral_ammunition.is_empty()
        || !right.integral_ammunition.is_empty()
        || !left.detachable_magazines.is_empty()
        || !right.detachable_magazines.is_empty()
    {
        return false;
    }
    let mut left_prototype = left.prototype.clone();
    let mut right_prototype = right.prototype.clone();
    left_prototype.charges = 0;
    right_prototype.charges = 0;
    left_prototype == right_prototype
        && left.raw_damage == right.raw_damage
        && left.fitted == right.fitted
        && left.variant == right.variant
        && left.snippet == right.snippet
        && left.initial_variables == right.initial_variables
        && left.sealed_pockets == right.sealed_pockets
}

fn seal_planned_item(item: &mut PlannedItemSpawn) -> Result<(), SimError> {
    if !planned_item_is_container_full(item)? {
        return Ok(());
    }
    for pocket in &item.prototype.ammunition_containers {
        if pocket
            .spawn_rules
            .as_ref()
            .is_some_and(|rules| rules.sealable)
            && item
                .pocket_contents
                .get(&pocket.pocket_index)
                .is_some_and(|contents| !contents.is_empty())
        {
            item.sealed_pockets.insert(pocket.pocket_index);
        }
    }
    Ok(())
}

fn planned_item_is_container_full(item: &PlannedItemSpawn) -> Result<bool, SimError> {
    for pocket in &item.prototype.ammunition_containers {
        let Some(rules) = pocket
            .spawn_rules
            .as_ref()
            .filter(|rules| rules.kind == SpawnPocketKindV1::Container)
        else {
            continue;
        };
        let Some(contents) = item
            .pocket_contents
            .get(&pocket.pocket_index)
            .filter(|contents| !contents.is_empty())
        else {
            return Ok(false);
        };
        let used_volume = contents.iter().try_fold(0_u64, |total, content| {
            total.checked_add(content.total_volume_milliliters()?)
        });
        if used_volume.is_some_and(|volume| volume == rules.max_contains_volume_milliliters) {
            continue;
        }
        let first = contents.first().ok_or(SimError::InvalidItem)?;
        let same_type = contents
            .iter()
            .all(|content| content.prototype.type_id == first.prototype.type_id);
        let mut one_more = first.clone();
        if one_more.prototype.containment.count_by_charges {
            one_more.prototype.charges = 1;
        }
        if !same_type || spawn_pocket_accepts(item, pocket.pocket_index, rules, &one_more)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_planned_output_bound(output: &[PlannedItemSpawn]) -> Result<(), SimError> {
    let objects = output
        .iter()
        .try_fold(0_u64, |total, item| total.checked_add(item.object_count()?));
    if objects.is_none_or(|objects| objects > MAX_ITEM_GROUP_OUTPUTS) {
        return Err(SimError::InvalidItem);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_protocol::{
        AmmunitionContainerPocketPrototypeV1, IntegralMagazinePocketPrototypeV1,
        ItemContainmentProfileV1, ItemGroupEventV1, ItemGroupItemPrototypeV1, ItemGroupNodeV1,
        ItemPhaseV1, MagazineWellPrototypeV1, SpawnPocketRulesV1,
    };
    use rand_core::SeedableRng;

    fn leaf_item(type_id: &str) -> ItemGroupItemPrototypeV1 {
        ItemGroupItemPrototypeV1 {
            prototype: CraftItemPrototypeV1 {
                type_id: type_id.to_owned(),
                charges: 1,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                tracks_temperature: false,
                ammunition_type: String::new(),
                ranged_weapon: None,
                magazine_capacity: 0,
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
                containment: Default::default(),
            },
            maximum_raw_damage: 0,
            variants: Vec::new(),
            description_expansion: None,
            snippets: Vec::new(),
            initial_variables: BTreeMap::new(),
            default_container: None,
            modifier_side_effects_supported: true,
            charges: None,
            minimum_one_charge: false,
            tool_charge_storage: None,
            charges_supported: true,
            modifier_container_capacity_applies: true,
            contents_insertion_supported: true,
        }
    }

    fn leaf(type_id: &str) -> ItemGroupTargetV1 {
        ItemGroupTargetV1::Item(Box::new(leaf_item(type_id)))
    }

    fn spawn_pocket(
        kind: SpawnPocketKindV1,
        rigid: bool,
        maximum_volume: u64,
        maximum_length: u64,
        restrictions: Vec<String>,
        sealable: bool,
    ) -> AmmunitionContainerPocketPrototypeV1 {
        AmmunitionContainerPocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("SPAWN"),
            capacities: Vec::new(),
            rigid,
            access_moves: 100,
            reloadable: false,
            unloadable: true,
            spawn_rules: Some(SpawnPocketRulesV1 {
                kind,
                max_contains_volume_milliliters: maximum_volume,
                max_contains_weight_milligrams: u64::MAX,
                max_item_volume_milliliters: maximum_volume,
                min_item_volume_milliliters: 0,
                max_item_length_millimeters: maximum_length,
                item_restrictions: restrictions,
                flag_restrictions: Vec::new(),
                access_moves: 100,
                rigid,
                watertight: false,
                transparent: false,
                forbidden: false,
                sealable,
            }),
        }
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
            modifier_charges: None,
            contents: Vec::new(),
            seal_contents: false,
            modifier_default_container_sealed: None,
            direct_wrapper: None,
            modifier_container: None,
        }
    }

    fn temperature_prototype() -> CraftItemPrototypeV1 {
        let mut prototype = leaf_item("chaw").prototype;
        prototype.comestible_type = String::from("MED");
        prototype.tracks_temperature = true;
        prototype.containment.phase = ItemPhaseV1::Solid;
        prototype
    }

    #[test]
    fn temperature_constructor_and_ten_minute_processing_are_exact() {
        let birth_tick = SimTick(123);
        let mut item =
            item_from_craft_prototype(ItemId::new(1, 1), &temperature_prototype(), birth_tick);
        assert_eq!(
            item.temperature,
            Some(initial_item_temperature_state(
                birth_tick,
                ItemPhaseV1::Solid
            ))
        );

        item.process_temperature(SimTick(
            birth_tick.0 + ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS - 1,
        ))
        .expect("temperature should remain pending before the boundary");
        assert_eq!(
            item.temperature
                .expect("temperature state should exist")
                .specific_energy_millijoules_per_gram,
            Some(cdda_protocol::ITEM_TEMPERATURE_UNPROCESSED_ENERGY_MJ_PER_G)
        );

        let processing_tick = SimTick(birth_tick.0 + ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS);
        item.process_temperature(processing_tick)
            .expect("the exact ten-minute boundary should process");
        let initialized = item.temperature.expect("temperature state should exist");
        assert_eq!(
            initialized.temperature_millikelvin,
            ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN
        );
        assert_eq!(initialized.specific_energy_millijoules_per_gram, None);
        assert_eq!(initialized.last_check_tick, processing_tick);
        assert_eq!(
            ItemInstance::from_snapshot(&item.snapshot())
                .expect("processed temperature state should restore")
                .snapshot(),
            item.snapshot()
        );
    }

    #[test]
    fn temperature_processing_walks_physical_container_contents() {
        let birth_tick = SimTick(77);
        let child =
            item_from_craft_prototype(ItemId::new(1, 2), &temperature_prototype(), birth_tick)
                .snapshot();
        let mut owner_prototype = leaf_item("wrapper").prototype;
        owner_prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            1_000,
            1_000,
            Vec::new(),
            false,
        )];
        let mut owner = item_from_craft_prototype(ItemId::new(1, 1), &owner_prototype, birth_tick);
        owner.ammunition_containers[0].contents.push(child);
        assert!(item_temperature_timestamps_are_valid(
            &owner.snapshot(),
            birth_tick
        ));
        assert!(
            !item_temperature_timestamps_are_valid(&owner.snapshot(), SimTick(birth_tick.0 - 1)),
            "recovery must reject nested temperature checks from the future"
        );

        let processing_tick = SimTick(birth_tick.0 + ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS);
        owner
            .process_temperature(processing_tick)
            .expect("nested contents should process through their physical owner");
        let child_state = owner.ammunition_containers[0].contents[0]
            .temperature
            .expect("nested temperature state should survive ownership");
        assert_eq!(
            child_state.temperature_millikelvin,
            ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN
        );
        assert_eq!(child_state.last_check_tick, processing_tick);
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
            description_expansion: None,
        }
    }

    fn description_expansion(
        template: &str,
        categories: &[(&str, &[(&str, u64)])],
    ) -> ItemDescriptionExpansionV1 {
        ItemDescriptionExpansionV1 {
            template: template.to_owned(),
            categories: categories
                .iter()
                .map(|(category, choices)| ItemDescriptionSnippetCategoryV1 {
                    category: (*category).to_owned(),
                    choices: choices
                        .iter()
                        .map(
                            |(text, weight)| cdda_protocol::ItemDescriptionSnippetChoiceV1 {
                                text: (*text).to_owned(),
                                weight: *weight,
                            },
                        )
                        .collect(),
                })
                .collect(),
        }
    }

    fn default_container_item(
        payload_type: &str,
        count_by_charges: bool,
        phase: ItemPhaseV1,
        payload_volume: u64,
        container_volume: u64,
        sealed: bool,
    ) -> ItemGroupItemPrototypeV1 {
        let mut payload = leaf_item(payload_type);
        payload.prototype.containment.count_by_charges = count_by_charges;
        payload.prototype.charges = i32::from(count_by_charges);
        payload.prototype.containment.stack_size = 1;
        payload.prototype.containment.phase = phase;
        payload.prototype.containment.volume_milliliters = payload_volume;
        payload.prototype.containment.weight_milligrams = payload_volume;
        let mut container = leaf_item("default_bottle");
        let mut pocket = spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            container_volume,
            container_volume,
            Vec::new(),
            true,
        );
        if phase == ItemPhaseV1::Liquid {
            pocket
                .spawn_rules
                .as_mut()
                .expect("fixture pocket has spawn rules")
                .watertight = true;
        }
        container.prototype.ammunition_containers = vec![pocket];
        payload.default_container = Some(ItemGroupContainerV1 {
            item: Box::new(container),
            variant_id: None,
            sealed,
            overflow: ItemGroupOverflowV1::None,
        });
        payload
    }

    #[test]
    fn default_container_direct_modifier_and_explicit_null_paths_are_distinct() {
        let water =
            default_container_item("water_clean", true, ItemPhaseV1::Liquid, 250, 500, true);
        assert_eq!(
            item_group_default_container_projection(
                &water,
                ItemGroupDefaultContainerMode::Unmodified,
            )
            .expect("direct default containment should project"),
            ItemGroupDefaultContainerProjection {
                outer_type: String::from("default_bottle"),
                content_types: vec![String::from("water_clean")],
                payload_charges: Some(2),
                sealed: true,
            }
        );

        let aspirin = default_container_item("aspirin", false, ItemPhaseV1::Solid, 1, 250, true);
        assert_eq!(
            item_group_default_container_projection(
                &aspirin,
                ItemGroupDefaultContainerMode::ModifierFallback { sealed: true },
            )
            .expect("modifier default containment should project"),
            ItemGroupDefaultContainerProjection {
                outer_type: String::from("default_bottle"),
                content_types: vec![String::from("aspirin")],
                payload_charges: Some(0),
                sealed: false,
            },
            "a partially filled default bottle cannot seal upstream"
        );
        assert_eq!(
            item_group_default_container_projection(
                &aspirin,
                ItemGroupDefaultContainerMode::ModifierSuppressed,
            )
            .expect("an explicit null modifier container should suppress fallback"),
            ItemGroupDefaultContainerProjection {
                outer_type: String::from("aspirin"),
                content_types: Vec::new(),
                payload_charges: None,
                sealed: false,
            }
        );

        let mut ibuprofen = leaf_item("ibuprofen");
        ibuprofen.prototype.charges = 0;
        ibuprofen.prototype.containment.volume_milliliters = 1;
        ibuprofen.prototype.containment.weight_milligrams = 1_000;
        assert_eq!(
            item_group_default_container_projection(
                &ibuprofen,
                ItemGroupDefaultContainerMode::ModifierExplicit {
                    container: ItemGroupContainerV1 {
                        item: Box::new(aspirin.clone()),
                        variant_id: None,
                        sealed: true,
                        overflow: ItemGroupOverflowV1::None,
                    },
                },
            )
            .expect("an explicit container creator should apply its own default wrapper"),
            ItemGroupDefaultContainerProjection {
                outer_type: String::from("default_bottle"),
                content_types: vec![String::from("ibuprofen"), String::from("aspirin")],
                payload_charges: Some(0),
                sealed: false,
            }
        );

        assert_eq!(
            item_group_default_container_projection(
                &aspirin,
                ItemGroupDefaultContainerMode::GroupWrapperExplicitNull {
                    container: aspirin
                        .default_container
                        .clone()
                        .expect("fixture should define a default bottle"),
                    count: 2,
                },
            )
            .expect("an entry-level null should keep payloads raw inside a group wrapper"),
            ItemGroupDefaultContainerProjection {
                outer_type: String::from("default_bottle"),
                content_types: vec![String::from("aspirin"), String::from("aspirin")],
                payload_charges: Some(0),
                sealed: false,
            }
        );
    }

    #[test]
    fn variable_size_fit_phase_is_generalized_and_preserves_the_rng_schedule() {
        assert!(!item_group_fitted_after_phase(false, false, true));
        assert!(!item_group_fitted_after_phase(true, false, false));
        assert!(item_group_fitted_after_phase(true, false, true));
        assert!(item_group_fitted_after_phase(true, true, false));

        let mut variable = leaf_item("variable_item");
        variable.prototype.containment.flags = vec![String::from("VARSIZE")];
        for expected_fitted in [false, true] {
            let seed = (0..10_000)
                .find(|seed| {
                    let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                    let _presentation = rng.next_u64();
                    let _variant = rng.next_u64();
                    rng.next_u64().is_multiple_of(3) == expected_fitted
                })
                .expect("both FIT outcomes should have a bounded witness");
            let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
            let _presentation = expected_rng.next_u64();
            let _variant = expected_rng.next_u64();
            let _fit = expected_rng.next_u64();
            let expected_downstream = expected_rng.next_u64();
            let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
            let planned = construct_item_group_item(&variable, &mut actual_rng)
                .expect("variable-size item should construct");
            assert_eq!(planned.fitted, expected_fitted);
            assert_eq!(actual_rng.next_u64(), expected_downstream);
        }

        let mut control_rng = ChaCha8Rng::seed_from_u64(2);
        let control = construct_item_group_item(&leaf_item("ordinary"), &mut control_rng)
            .expect("ordinary control should construct");
        assert!(!control.fitted);
        let mut expected_rng = ChaCha8Rng::seed_from_u64(2);
        let _presentation = expected_rng.next_u64();
        let _variant = expected_rng.next_u64();
        let _fit = expected_rng.next_u64();
        assert_eq!(control_rng.next_u64(), expected_rng.next_u64());

        let mut immutable_fit = leaf_item("immutable_fit");
        immutable_fit.prototype.containment.flags = vec![String::from("FIT")];
        let planned = construct_item_group_item(&immutable_fit, &mut control_rng)
            .expect("immutable FIT item should construct in the fitted state");
        assert!(planned.fitted);
        assert!(item_fit_state_is_valid(
            planned.fitted,
            &planned.prototype.containment
        ));
    }

    #[test]
    fn base_and_variant_description_snippets_expand_in_constructor_phase_order() {
        let mut item = leaf_item("described_item");
        item.description_expansion = Some(description_expansion(
            "Base <outer> <unknown>",
            &[
                ("<inner>", &[("done", 1)]),
                ("<outer>", &[("nested <inner>", 1)]),
            ],
        ));
        let mut selected = variant("described", 1);
        selected.description_expansion = Some(description_expansion(
            "Variant <saint> <unknown>",
            &[("<saint>", &[("first", 1), ("second", 1)])],
        ));
        item.variants = vec![selected.clone()];

        let seed = 1_337;
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let _ = expected_rng.next_u64(); // presentation
        let _ = expected_rng.next_u64(); // constructor variant
        let _initial_variant_choice = if expected_rng.next_u64() % 2 == 0 {
            "first"
        } else {
            "second"
        }; // set_itype_variant expansion
        let _ = expected_rng.next_u64(); // base <outer>
        let _ = expected_rng.next_u64(); // nested base <inner>
        let expected_variant_choice = if expected_rng.next_u64() % 2 == 0 {
            "first"
        } else {
            "second"
        }; // constructor's final variant expansion
        let _ = expected_rng.next_u64(); // item-group FIT phase
        let expected_next = expected_rng.next_u64();
        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let mut planned = construct_item_group_item(&item, &mut actual_rng)
            .expect("bounded description closure should construct");
        assert_eq!(
            planned.initial_variables.get("description"),
            Some(&ItemVariableValueV1::String(format!(
                "Variant {expected_variant_choice} <unknown>"
            )))
        );
        assert_eq!(
            planned.variant.as_ref().map(|variant| variant.id.as_str()),
            Some("described")
        );
        assert_eq!(actual_rng.next_u64(), expected_next);

        let explicit_expected = if actual_rng.clone().next_u64() % 2 == 0 {
            "first"
        } else {
            "second"
        };
        set_planned_variant(&mut planned, &selected, &mut actual_rng)
            .expect("an explicit variant modifier should expand again");
        assert_eq!(
            planned.initial_variables.get("description"),
            Some(&ItemVariableValueV1::String(format!(
                "Variant {explicit_expected} <unknown>"
            )))
        );

        let literal_tags = "<>".repeat(MAX_EXPANDED_DESCRIPTION_BYTES / 2);
        let literal_expansion = ItemDescriptionExpansionV1 {
            template: literal_tags.clone(),
            categories: Vec::new(),
        };
        let mut literal_rng = ChaCha8Rng::seed_from_u64(2_041);
        let expected_next = literal_rng.clone().next_u64();
        assert_eq!(
            expand_item_description(&literal_expansion, &mut literal_rng)
                .expect("sequential unknown tags should remain bounded and literal"),
            literal_tags
        );
        assert_eq!(
            literal_rng.next_u64(),
            expected_next,
            "unknown tags must not consume the canonical stream"
        );
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
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
    fn integral_tool_charge_ammunition_uses_the_two_draw_direct_constructor() {
        let mut target = leaf_item("charged_tool");
        target.prototype.charges = 0;
        target.prototype.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("BATTERY"),
            ammunition_type: String::from("battery"),
            capacity: 5,
            rigid: true,
            reloadable: false,
            unloadable: false,
        }];
        target.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        target.charges = Some(cdda_protocol::InclusiveI32RangeV1 {
            minimum: 1,
            maximum: 4,
        });
        let mut ammunition = leaf_item("battery").prototype;
        ammunition.ammunition_type = String::from("battery");
        ammunition.containment = ItemContainmentProfileV1 {
            count_by_charges: true,
            stack_size: 1,
            ..ItemContainmentProfileV1::default()
        };
        target.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::Integral { ammunition });
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
                    variant_id: None,
                    event: None,
                    target: ItemGroupTargetV1::Item(Box::new(target)),
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
        });

        let seed = 37;
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let _collection = expected_rng.next_u64();
        for _ in 0..3 {
            let _tool_constructor = expected_rng.next_u64();
        }
        let _fixed_damage = expected_rng.next_u64();
        let expected_charges = 1 + expected_rng.next_u64() % 4;
        let _ammunition_presentation = expected_rng.next_u64();
        let _ammunition_variant = expected_rng.next_u64();
        let _ammunition_dressing = expected_rng.next_u64();
        let _magazine_dressing = expected_rng.next_u64();

        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("integral-tool charge modifier should plan");
        assert_eq!(
            planned[0].integral_ammunition[&0].prototype.charges,
            expected_charges as i32
        );
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn detachable_tool_charges_install_the_default_magazine_and_clamp_ammunition() {
        let source = |charges: i32| {
            let mut target = leaf_item("wearable_light");
            target.prototype.charges = 0;
            target.prototype.magazine_wells = vec![MagazineWellPrototypeV1 {
                pocket_index: 4,
                pocket_id: String::from("BATTERY_WELL"),
                compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
                rigid: true,
                unloadable: true,
            }];
            target.charges = Some(cdda_protocol::InclusiveI32RangeV1 {
                minimum: charges,
                maximum: charges,
            });
            let mut magazine = leaf_item("medium_battery_cell").prototype;
            magazine.charges = 0;
            magazine.ammunition_type.clear();
            magazine.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("MAGAZINE"),
                ammunition_type: String::from("battery"),
                capacity: 56,
                rigid: true,
                reloadable: false,
                unloadable: false,
            }];
            let mut ammunition = leaf_item("battery").prototype;
            ammunition.ammunition_type = String::from("battery");
            ammunition.containment = ItemContainmentProfileV1 {
                count_by_charges: true,
                stack_size: 100,
                ..ItemContainmentProfileV1::default()
            };
            target.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::Detachable {
                well_pocket_index: 4,
                magazine,
                ammunition: Box::new(ammunition),
            });
            ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
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
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Item(Box::new(target)),
                        modifier_charges: None,
                        contents: Vec::new(),
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            })
        };

        for (requested, expected, expected_objects, expected_draws) in [
            (0, 0, 2, 9),
            (1, 1, 3, 11),
            (56, 56, 3, 11),
            (100, 56, 3, 11),
        ] {
            let seed = 97;
            let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
            let planned =
                plan_item_group_source(&source(requested), &BTreeMap::new(), &mut actual_rng)
                    .expect("detachable tool charge modifier should plan");
            let magazine = &planned[0].detachable_magazines[&4];
            assert_eq!(magazine.prototype.type_id, "medium_battery_cell");
            assert_eq!(
                magazine
                    .integral_ammunition
                    .get(&0)
                    .map(|ammunition| ammunition.prototype.charges)
                    .unwrap_or(0),
                expected
            );
            assert_eq!(planned[0].object_count(), Some(expected_objects));
            let actual_next = actual_rng.next_u64();
            let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
            let observed_draws = (0..20)
                .find(|_| expected_rng.next_u64() == actual_next)
                .expect("downstream draw should remain on the deterministic stream");
            assert_eq!(
                observed_draws, expected_draws,
                "requested charges {requested}"
            );
        }

        let ItemGroupSourceV1::Inline(inner_graph) = source(56) else {
            unreachable!("the fixture uses an inline inner group")
        };
        let groups = BTreeMap::from([(
            String::from("inner_tool_charge"),
            ItemGroupDefinitionV1 {
                group_id: String::from("inner_tool_charge"),
                graph: inner_graph,
            },
        )]);
        let repeated = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
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
                    variant_id: None,
                    event: None,
                    target: ItemGroupTargetV1::Group(String::from("inner_tool_charge")),
                    modifier_charges: Some(cdda_protocol::InclusiveI32RangeV1 {
                        minimum: 1,
                        maximum: 1,
                    }),
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
        });
        let seed = 97;
        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&repeated, &groups, &mut actual_rng)
            .expect("an outer modifier should reuse the installed default magazine");
        let magazine = &planned[0].detachable_magazines[&4];
        assert_eq!(planned[0].object_count(), Some(3));
        assert_eq!(
            magazine.integral_ammunition[&0].prototype.charges, 1,
            "the outer modifier should replace only the installed ammunition"
        );
        let actual_next = actual_rng.next_u64();
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let observed_draws = (0..30)
            .find(|_| expected_rng.next_u64() == actual_next)
            .expect("the downstream draw should stay on the deterministic stream");
        assert_eq!(
            observed_draws, 17,
            "the second modifier constructs ammunition but not another magazine"
        );
    }

    #[test]
    fn modifier_state_precedes_container_construction_and_charge_dressing() {
        let seed = (1_u64..100)
            .find(|seed| {
                let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                let draws = (0..12).map(|_| rng.next_u64()).collect::<Vec<_>>();
                draws[6] % 2 != draws[5] % 2
            })
            .expect("a distinguishing deterministic seed should exist");
        let mut target = leaf_item("charged_magazine");
        target.prototype.ammunition_type = String::from("9mm");
        target.prototype.magazine_capacity = 10;
        target.charges = Some(cdda_protocol::InclusiveI32RangeV1 {
            minimum: 1,
            maximum: 4,
        });
        let mut container = leaf_item("magazine_case");
        container.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            100,
            100,
            Vec::new(),
            false,
        )];
        container.variants = vec![variant("red_case", 1), variant("blue_case", 1)];
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
                    variant_id: None,
                    event: None,
                    target: ItemGroupTargetV1::Item(Box::new(target)),
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: Some(ItemGroupContainerV1 {
                        item: Box::new(container),
                        variant_id: None,
                        sealed: false,
                        overflow: ItemGroupOverflowV1::None,
                    }),
                }],
            }],
            wrapper: None,
        });

        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let _collection = expected_rng.next_u64();
        for _ in 0..3 {
            let _target_constructor = expected_rng.next_u64();
        }
        let _fixed_damage = expected_rng.next_u64();
        let _container_presentation = expected_rng.next_u64();
        let container_variant = expected_rng.next_u64();
        let _container_fit = expected_rng.next_u64();
        let expected_charges = 1 + expected_rng.next_u64() % 4;
        let _ammunition_dressing = expected_rng.next_u64();
        let _magazine_dressing = expected_rng.next_u64();

        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("modifier container should plan");
        let [case] = planned.as_slice() else {
            panic!("one modifier container should remain")
        };
        assert_eq!(case.prototype.type_id, "magazine_case");
        assert_eq!(
            case.variant.as_ref().map(|variant| variant.id.as_str()),
            Some(if container_variant % 2 == 0 {
                "red_case"
            } else {
                "blue_case"
            })
        );
        assert_eq!(
            case.pocket_contents[&0][0].prototype.charges,
            expected_charges as i32
        );
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn modifier_container_capacity_clamps_ranges_and_fills_liquids() {
        let source = |charges| {
            let mut liquid = leaf_item("liquid_payload");
            liquid.prototype.charges = 1;
            liquid.prototype.containment = ItemContainmentProfileV1 {
                weight_milligrams: 100,
                volume_milliliters: 1_000,
                longest_side_millimeters: 1,
                flags: Vec::new(),
                phase: ItemPhaseV1::Liquid,
                count_by_charges: true,
                stack_size: 10,
                ..ItemContainmentProfileV1::default()
            };
            liquid.charges = charges;
            liquid.minimum_one_charge = charges.is_some();
            liquid.charges_supported = true;

            let mut container = leaf_item("bottle");
            container.prototype.ammunition_containers = vec![spawn_pocket(
                SpawnPocketKindV1::Container,
                true,
                500,
                100,
                Vec::new(),
                false,
            )];
            let rules = container.prototype.ammunition_containers[0]
                .spawn_rules
                .as_mut()
                .expect("spawn rules");
            rules.max_contains_weight_milligrams = 1_000;
            rules.watertight = true;

            ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
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
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Item(Box::new(liquid)),
                        modifier_charges: None,
                        contents: Vec::new(),
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: Some(ItemGroupContainerV1 {
                            item: Box::new(container),
                            variant_id: None,
                            sealed: false,
                            overflow: ItemGroupOverflowV1::None,
                        }),
                    }],
                }],
                wrapper: None,
            })
        };

        for charges in [
            Some(cdda_protocol::InclusiveI32RangeV1 {
                minimum: 50,
                maximum: 80,
            }),
            None,
        ] {
            let mut rng = ChaCha8Rng::seed_from_u64(73);
            let planned = plan_item_group_source(&source(charges), &BTreeMap::new(), &mut rng)
                .expect("capacity-coupled liquid should plan");
            assert_eq!(planned.len(), 1);
            assert_eq!(planned[0].prototype.type_id, "bottle");
            assert_eq!(planned[0].pocket_contents[&0][0].prototype.charges, 5);
        }

        let mut actual_rng = ChaCha8Rng::seed_from_u64(73);
        let _ = plan_item_group_source(
            &source(Some(cdda_protocol::InclusiveI32RangeV1 {
                minimum: 50,
                maximum: 80,
            })),
            &BTreeMap::new(),
            &mut actual_rng,
        )
        .expect("clamped range should plan");
        let mut expected_rng = ChaCha8Rng::seed_from_u64(73);
        for _ in 0..8 {
            let _fixed_phase = expected_rng.next_u64();
        }
        assert_eq!(
            actual_rng.next_u64(),
            expected_rng.next_u64(),
            "clamping the explicit range to one value consumes no charge RNG draw"
        );
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
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
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        }],
                    }],
                    wrapper: None,
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
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
                                modifier_charges: None,
                                contents: Vec::new(),
                                seal_contents: false,
                                modifier_default_container_sealed: None,
                                direct_wrapper: None,
                                modifier_container: None,
                            },
                            ItemGroupEntryV1 {
                                probability: 100,
                                count_min: 1,
                                count_max: 1,
                                raw_damage: None,
                                variant_id: None,
                                event: None,
                                target: undamageable,
                                modifier_charges: None,
                                contents: Vec::new(),
                                seal_contents: false,
                                modifier_default_container_sealed: None,
                                direct_wrapper: None,
                                modifier_container: None,
                            },
                        ],
                    }],
                    wrapper: None,
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
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
    fn named_group_zero_charge_modifier_clamps_count_by_charges_leaf_to_one() {
        let mut count_by_charges = leaf_item("nail");
        count_by_charges.prototype.charges = 1;
        count_by_charges.prototype.containment.count_by_charges = true;
        count_by_charges.prototype.containment.stack_size = 1;
        count_by_charges.charges = None;
        count_by_charges.minimum_one_charge = false;
        let catalog = BTreeMap::from([(
            String::from("counted_child"),
            ItemGroupDefinitionV1 {
                group_id: String::from("counted_child"),
                graph: ItemGroupGraphV1 {
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
                            target: ItemGroupTargetV1::Item(Box::new(count_by_charges)),
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        }],
                    }],
                    wrapper: None,
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
                    raw_damage: None,
                    variant_id: None,
                    event: None,
                    target: ItemGroupTargetV1::Group(String::from("counted_child")),
                    modifier_charges: Some(cdda_protocol::InclusiveI32RangeV1 {
                        minimum: 0,
                        maximum: 0,
                    }),
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
        });

        let mut rng = ChaCha8Rng::seed_from_u64(113);
        let planned = plan_item_group_source(&source, &catalog, &mut rng)
            .expect("outer charge modifier should plan");
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].prototype.charges, 1);
    }

    #[test]
    fn nested_phone_family_retains_wrapper_contents_snippet_and_variables() {
        let mut phone = leaf_item("smart_phone");
        phone.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 233_000,
            volume_milliliters: 111,
            longest_side_millimeters: 150,
            flags: Vec::new(),
            estorable: false,
            ..ItemContainmentProfileV1::default()
        };
        phone.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::EFileStorage,
            true,
            u64::MAX,
            u64::MAX,
            Vec::new(),
            false,
        )];
        phone.snippets = vec![
            ItemSnippetV1 {
                id: String::from("greeting_a"),
                text: String::from("Hello"),
            },
            ItemSnippetV1 {
                id: String::from("greeting_b"),
                text: String::from("Hi"),
            },
        ];
        phone.initial_variables.insert(
            String::from("browsed"),
            ItemVariableValueV1::String(String::from("false")),
        );

        let phone_group = ItemGroupDefinitionV1 {
            group_id: String::from("phone_choice"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Distribution,
                    entries: vec![ItemGroupEntryV1 {
                        probability: 1,
                        count_min: 1,
                        count_max: 1,
                        raw_damage: None,
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Item(Box::new(phone)),
                        modifier_charges: None,
                        contents: Vec::new(),
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            },
        };
        let efiles = ["efile_map", "efile_lore", "efile_recipes"]
            .into_iter()
            .map(|type_id| {
                let mut efile = leaf_item(type_id);
                efile.prototype.containment.estorable = true;
                ItemGroupContentsSourceV1::Item(Box::new(efile))
            })
            .collect();
        let mut case = leaf_item("waterproof_smart_phone_case");
        case.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            111,
            150,
            vec![String::from("smart_phone")],
            false,
        )];
        case.variants = vec![variant("black_smart_phone_case", 1)];
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
                    target: ItemGroupTargetV1::Group(String::from("phone_choice")),
                    modifier_charges: None,
                    contents: efiles,
                    seal_contents: true,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: Some(ItemGroupContainerV1 {
                item: Box::new(case),
                variant_id: Some(String::from("black_smart_phone_case")),
                sealed: false,
                overflow: ItemGroupOverflowV1::None,
            }),
        });
        let catalog = BTreeMap::from([(String::from("phone_choice"), phone_group)]);
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let planned = plan_item_group_source(&source, &catalog, &mut rng)
            .expect("the generalized nested phone family should plan");
        let [case] = planned.as_slice() else {
            panic!("the family should emit exactly one case");
        };
        assert_eq!(case.prototype.type_id, "waterproof_smart_phone_case");
        assert_eq!(
            case.variant.as_ref().map(|variant| variant.id.as_str()),
            Some("black_smart_phone_case")
        );
        let [phone] = case
            .pocket_contents
            .get(&0)
            .expect("the case should contain its phone")
            .as_slice()
        else {
            panic!("the case should contain exactly one phone");
        };
        assert_eq!(phone.prototype.type_id, "smart_phone");
        assert!(matches!(
            phone.snippet.as_ref().map(|snippet| snippet.id.as_str()),
            Some("greeting_a" | "greeting_b")
        ));
        assert_eq!(
            phone.initial_variables.get("browsed"),
            Some(&ItemVariableValueV1::String(String::from("false")))
        );
        assert_eq!(
            phone
                .pocket_contents
                .get(&0)
                .expect("the phone should retain generated E-files")
                .iter()
                .map(|item| item.prototype.type_id.as_str())
                .collect::<Vec<_>>(),
            ["efile_recipes", "efile_lore", "efile_map"]
        );
        assert!(
            phone.sealed_pockets.is_empty(),
            "upstream seals modifier contents only when the modified item is comestible"
        );
    }

    #[test]
    fn physical_pockets_use_or_restrictions_and_recursive_dynamic_capacity() {
        let mut target_definition = leaf_item("rigid_case");
        target_definition.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            100,
            100,
            vec![String::from("different_item")],
            false,
        )];
        let rules = target_definition.prototype.ammunition_containers[0]
            .spawn_rules
            .as_mut()
            .expect("spawn rules exist");
        rules.flag_restrictions = vec![String::from("FORM_A"), String::from("FORM_B")];
        rules.max_contains_weight_milligrams = 70;

        let mut payload_definition = leaf_item("smart_phone");
        payload_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 20,
            volume_milliliters: 10,
            longest_side_millimeters: 10,
            flags: vec![String::from("FORM_B")],
            ..ItemContainmentProfileV1::default()
        };
        payload_definition.prototype.charges = 0;
        payload_definition.prototype.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("BATTERY"),
            ammunition_type: String::from("battery"),
            capacity: 5,
            rigid: true,
            reloadable: false,
            unloadable: false,
        }];
        let mut battery_definition = leaf_item("battery");
        battery_definition.prototype.charges = 5;
        battery_definition.prototype.ammunition_type = String::from("battery");
        battery_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 10,
            volume_milliliters: 100,
            longest_side_millimeters: 10,
            phase: ItemPhaseV1::Solid,
            count_by_charges: true,
            stack_size: 10,
            ..ItemContainmentProfileV1::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(17);
        let mut target = construct_item_group_item(&target_definition, &mut rng)
            .expect("target should construct");
        let mut payload = construct_item_group_item(&payload_definition, &mut rng)
            .expect("payload should construct");
        let battery = construct_item_group_item(&battery_definition, &mut rng)
            .expect("battery should construct");
        payload.integral_ammunition.insert(0, Box::new(battery));
        assert!(matches!(
            insert_planned_item(&mut target, payload.clone()),
            Ok(Ok(()))
        ));
        assert_eq!(
            target.pocket_contents[&0][0].total_weight_milligrams(),
            Some(70),
            "the integral battery must count toward the phone's carried weight"
        );

        let mut no_unwield_payload = payload.clone();
        no_unwield_payload
            .prototype
            .containment
            .flags
            .push(String::from("NO_UNWIELD"));
        let mut no_unwield_target = construct_item_group_item(&target_definition, &mut rng)
            .expect("NO_UNWIELD target should construct");
        assert!(matches!(
            insert_planned_item(&mut no_unwield_target, no_unwield_payload),
            Ok(Err(_))
        ));

        let mut too_small = construct_item_group_item(&target_definition, &mut rng)
            .expect("second target should construct");
        too_small.prototype.ammunition_containers[0]
            .spawn_rules
            .as_mut()
            .expect("spawn rules exist")
            .max_contains_weight_milligrams = 69;
        assert!(matches!(
            insert_planned_item(&mut too_small, payload),
            Ok(Err(_))
        ));
    }

    #[test]
    fn liquid_requires_watertight_capacity_and_uses_charge_scaled_volume() {
        let mut target_definition = leaf_item("bottle");
        target_definition.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            100,
            100,
            Vec::new(),
            false,
        )];
        let mut liquid_definition = leaf_item("water");
        liquid_definition.prototype.charges = 5;
        liquid_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 10,
            volume_milliliters: 100,
            longest_side_millimeters: 50,
            phase: ItemPhaseV1::Liquid,
            count_by_charges: true,
            stack_size: 10,
            ..ItemContainmentProfileV1::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(23);
        let liquid = construct_item_group_item(&liquid_definition, &mut rng)
            .expect("liquid should construct");
        assert_eq!(liquid.total_volume_milliliters(), Some(50));

        let mut leaking = construct_item_group_item(&target_definition, &mut rng)
            .expect("leaking target should construct");
        assert!(matches!(
            insert_planned_item(&mut leaking, liquid.clone()),
            Ok(Err(_))
        ));

        target_definition.prototype.ammunition_containers[0]
            .spawn_rules
            .as_mut()
            .expect("spawn rules exist")
            .watertight = true;
        let mut watertight = construct_item_group_item(&target_definition, &mut rng)
            .expect("watertight target should construct");
        assert!(matches!(
            insert_planned_item(&mut watertight, liquid.clone()),
            Ok(Ok(()))
        ));
        assert!(matches!(
            insert_planned_item(&mut watertight, liquid.clone()),
            Ok(Ok(()))
        ));
        assert_eq!(watertight.pocket_contents[&0].len(), 1);
        assert_eq!(watertight.pocket_contents[&0][0].prototype.charges, 10);
        let mut mismatched_state = liquid.clone();
        mismatched_state.initial_variables.insert(
            String::from("source"),
            ItemVariableValueV1::String(String::from("different")),
        );
        let mut mixed = construct_item_group_item(&target_definition, &mut rng)
            .expect("mixed-state target should construct");
        assert!(matches!(
            insert_planned_item(&mut mixed, liquid.clone()),
            Ok(Ok(()))
        ));
        assert!(matches!(
            insert_planned_item(&mut mixed, mismatched_state),
            Ok(Err(_))
        ));
        let mut too_small = construct_item_group_item(&target_definition, &mut rng)
            .expect("small target should construct");
        too_small.prototype.ammunition_containers[0]
            .spawn_rules
            .as_mut()
            .expect("spawn rules exist")
            .max_contains_volume_milliliters = 49;
        assert!(matches!(
            insert_planned_item(&mut too_small, liquid),
            Ok(Err(_))
        ));
    }

    #[test]
    fn count_by_charge_containment_keeps_fitted_states_distinct() {
        let mut target_definition = leaf_item("rigid_case");
        target_definition.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            100,
            100,
            Vec::new(),
            false,
        )];
        let mut payload_definition = leaf_item("variable_powder");
        payload_definition.prototype.charges = 2;
        payload_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 1,
            volume_milliliters: 1,
            longest_side_millimeters: 1,
            flags: vec![String::from("VARSIZE")],
            count_by_charges: true,
            stack_size: 1,
            ..ItemContainmentProfileV1::default()
        };

        let mut rng = ChaCha8Rng::seed_from_u64(29);
        let mut target = construct_item_group_item(&target_definition, &mut rng)
            .expect("target should construct");
        let mut unfitted = construct_item_group_item(&payload_definition, &mut rng)
            .expect("payload should construct");
        unfitted.fitted = false;
        let mut fitted = unfitted.clone();
        fitted.fitted = true;

        assert!(!planned_items_can_combine_for_containment(
            &unfitted, &fitted
        ));
        assert!(matches!(
            insert_planned_item(&mut target, unfitted),
            Ok(Ok(()))
        ));
        assert!(matches!(
            insert_planned_item(&mut target, fitted),
            Ok(Ok(()))
        ));
        let contents = &target.pocket_contents[&0];
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0].prototype.charges, 2);
        assert!(contents[0].fitted);
        assert_eq!(contents[1].prototype.charges, 2);
        assert!(!contents[1].fitted);
    }

    #[test]
    fn planned_containment_depth_matches_the_canonical_snapshot_boundary() {
        let mut container_definition = leaf_item("nested_case");
        container_definition.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            u64::MAX,
            u64::MAX,
            Vec::new(),
            false,
        )];
        let mut rng = ChaCha8Rng::seed_from_u64(31);
        let mut nested = construct_item_group_item(&leaf_item("payload"), &mut rng)
            .expect("payload should construct");
        for _ in 0..MAX_ITEM_COMPONENT_DEPTH {
            let mut container = construct_item_group_item(&container_definition, &mut rng)
                .expect("container should construct");
            assert!(matches!(
                insert_planned_item(&mut container, nested),
                Ok(Ok(()))
            ));
            nested = container;
        }
        assert_eq!(nested.containment_depth(), Some(MAX_ITEM_COMPONENT_DEPTH));

        let mut one_too_deep = construct_item_group_item(&container_definition, &mut rng)
            .expect("outer container should construct");
        assert!(matches!(
            insert_planned_item(&mut one_too_deep, nested),
            Err(SimError::InvalidItem)
        ));
        assert!(one_too_deep.pocket_contents.is_empty());
    }

    #[test]
    fn multiple_modifier_contents_sources_consume_implicit_collection_rolls() {
        let seed = (1_u64..100)
            .find(|seed| {
                let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                let draws = (0..13).map(|_| rng.next_u64()).collect::<Vec<_>>();
                draws[10] % 2 != draws[8] % 2
            })
            .expect("a distinguishing deterministic seed should exist");
        let mut target = leaf_item("case");
        target.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            100,
            100,
            Vec::new(),
            false,
        )];
        let first = leaf_item("first_payload");
        let mut second = leaf_item("second_payload");
        second.variants = vec![variant("red", 1), variant("blue", 1)];
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
                    target: ItemGroupTargetV1::Item(Box::new(target)),
                    modifier_charges: None,
                    contents: vec![
                        ItemGroupContentsSourceV1::Item(Box::new(first)),
                        ItemGroupContentsSourceV1::Item(Box::new(second)),
                    ],
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
        });

        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        let draws = (0..12).map(|_| expected_rng.next_u64()).collect::<Vec<_>>();
        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut actual_rng)
            .expect("multiple direct contents should plan");
        let second = planned[0]
            .pocket_contents
            .get(&0)
            .and_then(|contents| contents.first())
            .expect("second source is inserted at the front");
        assert_eq!(second.prototype.type_id, "second_payload");
        assert_eq!(
            second.variant.as_ref().map(|variant| variant.id.as_str()),
            Some(if draws[10] % 2 == 0 { "red" } else { "blue" })
        );
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());
    }

    #[test]
    fn named_contents_group_starts_an_independent_recursion_budget() {
        let nodes = (0..MAX_ITEM_GROUP_DEPTH)
            .map(|index| ItemGroupNodeV1 {
                node_id: u16::try_from(index).expect("bounded depth fits u16"),
                kind: ItemGroupKindV1::Collection,
                entries: vec![entry(
                    100,
                    None,
                    if index + 1 == MAX_ITEM_GROUP_DEPTH {
                        "chain_leaf"
                    } else {
                        "unused"
                    },
                )],
            })
            .collect::<Vec<_>>();
        let mut definition = ItemGroupDefinitionV1 {
            group_id: String::from("depth_chain"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes,
                wrapper: None,
            },
        };
        for index in 0..MAX_ITEM_GROUP_DEPTH - 1 {
            definition.graph.nodes[index].entries[0].target =
                ItemGroupTargetV1::Node(u16::try_from(index + 1).expect("bounded depth fits u16"));
        }
        let catalog = BTreeMap::from([(definition.group_id.clone(), definition)]);
        let mut case = leaf_item("depth_case");
        case.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            u64::MAX,
            u64::MAX,
            Vec::new(),
            false,
        )];
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
                    variant_id: None,
                    event: None,
                    target: ItemGroupTargetV1::Item(Box::new(case)),
                    modifier_charges: None,
                    contents: vec![ItemGroupContentsSourceV1::Group(String::from(
                        "depth_chain",
                    ))],
                    seal_contents: false,
                    modifier_default_container_sealed: None,
                    direct_wrapper: None,
                    modifier_container: None,
                }],
            }],
            wrapper: None,
        });

        let planned = plan_item_group_source(&source, &catalog, &mut ChaCha8Rng::seed_from_u64(73))
            .expect("contents group should receive the same root recursion budget as any source");
        assert_eq!(
            planned[0].pocket_contents[&0][0].prototype.type_id,
            "chain_leaf"
        );
    }

    #[test]
    fn modifier_contents_seal_only_comestible_spawn_pockets() {
        let mut food = leaf_item("jarred_food");
        food.prototype.comestible_type = String::from("FOOD");
        food.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            10,
            10,
            Vec::new(),
            true,
        )];
        let mut contents = leaf_item("seasoning");
        contents.prototype.containment.volume_milliliters = 10;
        let source = |seal_contents| {
            ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
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
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Item(Box::new(food.clone())),
                        modifier_charges: None,
                        contents: vec![ItemGroupContentsSourceV1::Item(Box::new(contents.clone()))],
                        seal_contents,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            })
        };

        let mut sealed_rng = ChaCha8Rng::seed_from_u64(27);
        let sealed = plan_item_group_source(&source(true), &BTreeMap::new(), &mut sealed_rng)
            .expect("default-sealed comestible contents should plan");
        assert_eq!(sealed[0].sealed_pockets, BTreeSet::from([0]));
        assert_eq!(sealed[0].pocket_contents[&0].len(), 1);
        let mut expected_single_source_rng = ChaCha8Rng::seed_from_u64(27);
        for _ in 0..8 {
            let _ = expected_single_source_rng.next_u64();
        }
        assert_eq!(
            sealed_rng.next_u64(),
            expected_single_source_rng.next_u64(),
            "one contents source stays a direct creator without a collection roll"
        );

        let mut unsealed_rng = ChaCha8Rng::seed_from_u64(27);
        let unsealed = plan_item_group_source(&source(false), &BTreeMap::new(), &mut unsealed_rng)
            .expect("explicitly unsealed comestible contents should plan");
        assert!(unsealed[0].sealed_pockets.is_empty());
    }

    #[test]
    fn direct_entry_wrapper_contains_the_complete_count_and_exists_when_empty() {
        let source = |count| {
            let mut wrapper = leaf_item("counted_case");
            wrapper.prototype.ammunition_containers = vec![spawn_pocket(
                SpawnPocketKindV1::Container,
                true,
                10,
                100,
                Vec::new(),
                false,
            )];
            wrapper.variants = vec![variant("red_case", 1), variant("blue_case", 1)];
            ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![ItemGroupEntryV1 {
                        probability: 100,
                        count_min: count,
                        count_max: count,
                        raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                            minimum: 0,
                            maximum: 0,
                        }),
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Item(Box::new(leaf_item("payload"))),
                        modifier_charges: None,
                        contents: Vec::new(),
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: Some(ItemGroupContainerV1 {
                            item: Box::new(wrapper),
                            variant_id: None,
                            sealed: true,
                            overflow: ItemGroupOverflowV1::None,
                        }),
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            })
        };

        let mut counted_rng = ChaCha8Rng::seed_from_u64(41);
        let counted = plan_item_group_source(&source(2), &BTreeMap::new(), &mut counted_rng)
            .expect("two payloads should share one direct wrapper");
        assert_eq!(counted.len(), 1);
        assert_eq!(counted[0].prototype.type_id, "counted_case");
        assert_eq!(counted[0].pocket_contents[&0].len(), 2);
        assert!(
            counted[0].sealed_pockets.is_empty(),
            "a partially filled wrapper remains unsealed"
        );
        let mut expected_counted_rng = ChaCha8Rng::seed_from_u64(41);
        let _collection = expected_counted_rng.next_u64();
        for _ in 0..2 {
            for _ in 0..3 {
                let _payload_constructor = expected_counted_rng.next_u64();
            }
            let _fixed_damage = expected_counted_rng.next_u64();
        }
        let _payload_shuffle = expected_counted_rng.next_u64();
        let _wrapper_presentation = expected_counted_rng.next_u64();
        let counted_wrapper_variant = expected_counted_rng.next_u64();
        assert_eq!(
            counted[0]
                .variant
                .as_ref()
                .map(|variant| variant.id.as_str()),
            Some(if counted_wrapper_variant % 2 == 0 {
                "red_case"
            } else {
                "blue_case"
            })
        );
        assert_eq!(counted_rng.next_u64(), expected_counted_rng.next_u64());

        let mut empty_rng = ChaCha8Rng::seed_from_u64(41);
        let empty = plan_item_group_source(&source(0), &BTreeMap::new(), &mut empty_rng)
            .expect("zero count should still construct its direct wrapper");
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0].prototype.type_id, "counted_case");
        assert!(empty[0].pocket_contents.is_empty());
        assert!(
            empty[0].sealed_pockets.is_empty(),
            "an empty wrapper remains unsealed"
        );
        let mut expected_empty_rng = ChaCha8Rng::seed_from_u64(41);
        let _collection = expected_empty_rng.next_u64();
        let _wrapper_presentation = expected_empty_rng.next_u64();
        let empty_wrapper_variant = expected_empty_rng.next_u64();
        assert_eq!(
            empty[0].variant.as_ref().map(|variant| variant.id.as_str()),
            Some(if empty_wrapper_variant % 2 == 0 {
                "red_case"
            } else {
                "blue_case"
            })
        );
        assert_eq!(empty_rng.next_u64(), expected_empty_rng.next_u64());
    }

    #[test]
    fn rigid_wrapper_boundaries_spill_or_discard_without_losing_the_container() {
        let payload = |type_id: &str, length: u64| {
            let mut item = leaf_item(type_id);
            item.prototype.containment = ItemContainmentProfileV1 {
                weight_milligrams: 1,
                volume_milliliters: 1,
                longest_side_millimeters: length,
                flags: Vec::new(),
                estorable: false,
                ..ItemContainmentProfileV1::default()
            };
            item
        };
        let source = |overflow| {
            let mut wrapper = leaf_item("rigid_case");
            wrapper.prototype.ammunition_containers = vec![spawn_pocket(
                SpawnPocketKindV1::Container,
                true,
                2,
                10,
                Vec::new(),
                true,
            )];
            ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![
                        ItemGroupEntryV1 {
                            target: ItemGroupTargetV1::Item(Box::new(payload("short_a", 10))),
                            ..entry(100, None, "unused")
                        },
                        ItemGroupEntryV1 {
                            target: ItemGroupTargetV1::Item(Box::new(payload("short_b", 9))),
                            ..entry(100, None, "unused")
                        },
                        ItemGroupEntryV1 {
                            target: ItemGroupTargetV1::Item(Box::new(payload("too_long", 11))),
                            ..entry(100, None, "unused")
                        },
                    ],
                }],
                wrapper: Some(ItemGroupContainerV1 {
                    item: Box::new(wrapper),
                    variant_id: None,
                    sealed: true,
                    overflow,
                }),
            })
        };

        let mut spill_rng = ChaCha8Rng::seed_from_u64(9);
        let spilled = plan_item_group_source(
            &source(ItemGroupOverflowV1::Spill),
            &BTreeMap::new(),
            &mut spill_rng,
        )
        .expect("spill overflow should plan");
        assert_eq!(spilled.len(), 2);
        assert_eq!(spilled[0].prototype.type_id, "too_long");
        assert_eq!(spilled[1].prototype.type_id, "rigid_case");
        assert_eq!(
            spilled[1]
                .pocket_contents
                .get(&0)
                .expect("the rigid case should contain both fitting items")
                .len(),
            2
        );
        assert_eq!(spilled[1].sealed_pockets, BTreeSet::from([0]));

        let mut discard_rng = ChaCha8Rng::seed_from_u64(9);
        let discarded = plan_item_group_source(
            &source(ItemGroupOverflowV1::Discard),
            &BTreeMap::new(),
            &mut discard_rng,
        )
        .expect("discard overflow should plan");
        assert_eq!(discarded.len(), 1);
        assert_eq!(discarded[0].prototype.type_id, "rigid_case");

        let mut non_rigid = leaf_item("bag");
        non_rigid.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            false,
            2,
            10,
            Vec::new(),
            false,
        )];
        let invalid = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![entry(100, None, "payload")],
            }],
            wrapper: Some(ItemGroupContainerV1 {
                item: Box::new(non_rigid),
                variant_id: None,
                sealed: false,
                overflow: ItemGroupOverflowV1::None,
            }),
        });
        let mut invalid_rng = ChaCha8Rng::seed_from_u64(1);
        assert!(
            matches!(
                plan_item_group_source(&invalid, &BTreeMap::new(), &mut invalid_rng),
                Err(SimError::InvalidItem)
            ),
            "unsupported flexible-container semantics must fail closed"
        );
    }

    #[test]
    fn spawn_pockets_use_soft_and_recursive_length_compatibility() {
        let planned = |definition: &ItemGroupItemPrototypeV1| {
            let mut rng = ChaCha8Rng::seed_from_u64(1);
            construct_item_group_item(definition, &mut rng).expect("fixture should construct")
        };
        let mut outer_definition = leaf_item("outer");
        outer_definition.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            1_000,
            100,
            Vec::new(),
            false,
        )];
        outer_definition.prototype.ammunition_containers[0]
            .spawn_rules
            .as_mut()
            .expect("spawn rules")
            .max_item_volume_milliliters = 100;

        let mut soft_definition = leaf_item("soft_bundle");
        soft_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 1,
            volume_milliliters: 500,
            longest_side_millimeters: 500,
            flags: vec![String::from("SOFT")],
            ..ItemContainmentProfileV1::default()
        };
        let mut soft_outer = planned(&outer_definition);
        assert!(
            insert_planned_item(&mut soft_outer, planned(&soft_definition))
                .expect("compatibility should evaluate")
                .is_ok(),
            "an empty explicit SOFT item bypasses max-item volume and has zero length upstream"
        );

        let mut ambiguous_definition = soft_definition.clone();
        ambiguous_definition.prototype.type_id = String::from("material_softness_unknown");
        ambiguous_definition.prototype.containment.flags.clear();
        let mut ambiguous_outer = planned(&outer_definition);
        assert!(
            insert_planned_item(&mut ambiguous_outer, planned(&ambiguous_definition))
                .expect("compatibility should evaluate")
                .is_err(),
            "material-derived softness remains fail-closed when the hard interpretation fails"
        );

        let mut inner_definition = leaf_item("inner");
        inner_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 1,
            volume_milliliters: 1,
            longest_side_millimeters: 50,
            flags: vec![String::from("HARD")],
            ..ItemContainmentProfileV1::default()
        };
        inner_definition.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            1_000,
            200,
            Vec::new(),
            false,
        )];
        let mut child_definition = leaf_item("long_child");
        child_definition.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 1,
            volume_milliliters: 1,
            longest_side_millimeters: 150,
            flags: vec![String::from("HARD")],
            ..ItemContainmentProfileV1::default()
        };
        let mut inner = planned(&inner_definition);
        assert!(
            insert_planned_item(&mut inner, planned(&child_definition))
                .expect("inner compatibility should evaluate")
                .is_ok()
        );
        let mut nested_outer = planned(&outer_definition);
        assert!(
            insert_planned_item(&mut nested_outer, inner)
                .expect("outer compatibility should evaluate")
                .is_err(),
            "the physical child makes its wrapper longer than the outer pocket limit"
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
            wrapper: None,
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
            wrapper: None,
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
