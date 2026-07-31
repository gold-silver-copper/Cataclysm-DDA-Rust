use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    AmmunitionContainerPocketSnapshotV1, CraftItemPrototypeV1, CreatureCorpseSnapshotV1,
    ITEM_DEGRADATION_INCREMENTS_VARIABLE, ITEM_DEGRADATION_VARIABLE,
    ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE, ITEM_GROUP_GUN_FOULING_VARIABLE,
    ITEM_GUN_DIRT_FAULT_VARIABLE, ITEM_GUN_UNLUBRICATED_FAULT_VARIABLE, ITEM_ROT_TURNS_VARIABLE,
    ITEM_STATIC_CORPSE_SHELF_LIFE_TURNS, ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN,
    ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS, IntegralMagazinePocketSnapshotV1,
    ItemComponentSnapshotV1, ItemContainmentProfileV1, ItemDescriptionExpansionV1,
    ItemDescriptionSnippetCategoryV1, ItemGroupChargeCapacityV1, ItemGroupChargeRangeV1,
    ItemGroupContainerV1, ItemGroupContentsSourceV1, ItemGroupDefinitionV1, ItemGroupEntryV1,
    ItemGroupGraphV1, ItemGroupItemPrototypeV1, ItemGroupKindV1, ItemGroupNodeV1,
    ItemGroupOverflowV1, ItemGroupSourceV1, ItemGroupTargetV1, ItemGroupToolChargeStorageV1,
    ItemGroupVariantOptionV1, ItemId, ItemSnapshot, ItemSnippetV1, ItemTemperatureStateV1,
    ItemVariableValueV1, ItemVariantV1, MAX_CRAFT_RECIPE_ID_BYTES, MAX_EXPANDED_DESCRIPTION_BYTES,
    MAX_ITEM_COMPONENT_DEPTH, MAX_ITEM_GROUP_DEPTH, MAX_ITEM_GROUP_OUTPUTS, MAX_ITEM_RAW_DAMAGE,
    MAX_ITEM_VARIABLES, MILLIJOULES_PER_BATTERY_CHARGE, MagazineWellSnapshotV1, PoweredToolStateV1,
    RangedWeaponSnapshot, SimTick, SpawnPocketKindV1, WorldgenVehicleDirectItemSpawnV1,
    decode_item_group_custom_flag_marker, decode_item_group_dressing_marker,
    encode_item_group_dressing_marker, initial_item_temperature_state,
    is_reserved_item_group_custom_flag_marker, is_reserved_item_group_dressing_marker,
    is_reserved_item_group_internal_marker, item_containment_single_charge_volume_milliliters,
    item_containment_volume_milliliters, item_containment_weight_milligrams,
    item_degradation_state, item_pocket_volume_multiplier, item_pocket_weight_multiplier,
    item_rot_state, item_rot_variables_are_valid,
    spawn_pocket_content_weight_with_multiplier_milligrams,
    spawn_pocket_external_volume_with_multiplier_milliliters,
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

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct InventoryTypeSummary {
    pub(super) amount: u64,
    pub(super) charges: u64,
    pub(super) count_by_charges: bool,
}

pub(super) fn summarize_inventory_by_type<'a>(
    items: impl Iterator<Item = &'a ItemInstance>,
) -> BTreeMap<String, InventoryTypeSummary> {
    let mut inventory = BTreeMap::new();
    for item in items {
        summarize_item_instance(item, &mut inventory);
    }
    inventory
}

fn summarize_item_instance(
    item: &ItemInstance,
    inventory: &mut BTreeMap<String, InventoryTypeSummary>,
) {
    summarize_item(
        &item.type_id,
        item.charges,
        item.containment.count_by_charges,
        inventory,
    );
    for ammunition in item
        .integral_magazines
        .iter()
        .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
    {
        summarize_item_snapshot(ammunition, inventory);
    }
    for magazine in item
        .magazine_wells
        .iter()
        .filter_map(|well| well.installed_magazine.as_deref())
    {
        summarize_item_snapshot(magazine, inventory);
    }
    for content in item
        .ammunition_containers
        .iter()
        .flat_map(|pocket| &pocket.contents)
    {
        summarize_item_snapshot(content, inventory);
    }
}

fn summarize_item_snapshot(
    item: &ItemSnapshot,
    inventory: &mut BTreeMap<String, InventoryTypeSummary>,
) {
    summarize_item(
        &item.type_id,
        item.charges,
        item.containment.count_by_charges,
        inventory,
    );
    for ammunition in item
        .integral_magazines
        .iter()
        .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
    {
        summarize_item_snapshot(ammunition, inventory);
    }
    for magazine in item
        .magazine_wells
        .iter()
        .filter_map(|well| well.installed_magazine.as_deref())
    {
        summarize_item_snapshot(magazine, inventory);
    }
    for content in item
        .ammunition_containers
        .iter()
        .flat_map(|pocket| &pocket.contents)
    {
        summarize_item_snapshot(content, inventory);
    }
}

fn summarize_item(
    type_id: &str,
    charges: i32,
    count_by_charges: bool,
    inventory: &mut BTreeMap<String, InventoryTypeSummary>,
) {
    let entry = inventory.entry(type_id.to_owned()).or_default();
    let charges = u64::try_from(charges.max(0)).unwrap_or(0);
    entry.amount = entry
        .amount
        .saturating_add(if count_by_charges { charges } else { 1 });
    entry.charges = entry.charges.saturating_add(charges);
    entry.count_by_charges |= count_by_charges;
}

impl ItemInstance {
    pub(super) fn process_temperature(&mut self, current_tick: SimTick) -> Result<(), SimError> {
        self.process_temperature_and_rot(current_tick, false)
            .map(|_| ())
    }

    pub(super) fn process_temperature_and_rot(
        &mut self,
        current_tick: SimTick,
        removable: bool,
    ) -> Result<bool, SimError> {
        self.process_temperature_and_rot_with_insulation(current_tick, removable, 1.0)
    }

    fn process_temperature_and_rot_with_insulation(
        &mut self,
        current_tick: SimTick,
        removable: bool,
        parent_insulation: f32,
    ) -> Result<bool, SimError> {
        let rotten_away = process_item_temperature_and_rot_state(
            &mut self.temperature,
            &mut self.variables,
            current_tick,
            parent_insulation,
        )?;
        if let Some(components) = &mut self.component_provenance {
            for component in components {
                process_component_temperature(component, current_tick)?;
            }
        }
        for pocket in &mut self.integral_magazines {
            if let Some(ammunition) = pocket.loaded_ammunition.as_deref_mut() {
                let remove = process_item_snapshot_temperature_and_rot(
                    ammunition,
                    current_tick,
                    removable,
                    parent_insulation,
                )?;
                if remove {
                    pocket.loaded_ammunition = None;
                }
            }
        }
        for well in &mut self.magazine_wells {
            if let Some(magazine) = well.installed_magazine.as_deref_mut() {
                let remove = process_item_snapshot_temperature_and_rot(
                    magazine,
                    current_tick,
                    removable,
                    parent_insulation,
                )?;
                if remove {
                    well.installed_magazine = None;
                }
            }
        }
        let pocket_insulations = self
            .ammunition_containers
            .iter()
            .map(|pocket| {
                (
                    pocket.pocket_index,
                    cdda_protocol::item_pocket_insulation(&self.variables, pocket.pocket_index)
                        .unwrap_or(1.0)
                        .max(parent_insulation),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for pocket in &mut self.ammunition_containers {
            let insulation = *pocket_insulations
                .get(&pocket.pocket_index)
                .ok_or(SimError::InvalidItem)?;
            let mut index = 0;
            while index < pocket.contents.len() {
                if process_item_snapshot_temperature_and_rot(
                    pocket
                        .contents
                        .get_mut(index)
                        .ok_or(SimError::InvalidItem)?,
                    current_tick,
                    removable,
                    insulation,
                )? {
                    pocket.contents.remove(index);
                } else {
                    index += 1;
                }
            }
        }
        Ok(removable && rotten_away)
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
) -> Result<Option<u64>, SimError> {
    let Some(state) = state else {
        return Ok(None);
    };
    let elapsed = current_tick
        .0
        .checked_sub(state.last_check_tick.0)
        .ok_or(SimError::InvalidItem)?;
    if elapsed < ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS {
        return Ok(None);
    }
    match (
        state.temperature_millikelvin,
        state.specific_energy_millijoules_per_gram,
    ) {
        (
            cdda_protocol::ITEM_TEMPERATURE_UNPROCESSED_MILLIKELVIN,
            Some(cdda_protocol::ITEM_TEMPERATURE_UNPROCESSED_ENERGY_MJ_PER_G),
        ) => {
            state.temperature_millikelvin = ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN;
            state.specific_energy_millijoules_per_gram =
                state.thermal_properties.and_then(|properties| {
                    properties.normal_ambient_specific_energy_millijoules_per_gram()
                });
        }
        (ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN, energy)
            if energy
                == state.thermal_properties.and_then(|properties| {
                    properties.normal_ambient_specific_energy_millijoules_per_gram()
                }) => {}
        _ => return Err(SimError::InvalidItem),
    }
    state.last_check_tick = current_tick;
    Ok(Some(elapsed / SimTick::HZ))
}

const NORMAL_AMBIENT_HOURLY_ROT_TURNS: u64 = 4_099;
const SECONDS_PER_HOUR: u64 = 60 * 60;
const STATIC_CORPSE_REMOVAL_TURNS: u64 = 10 * 24 * 60 * 60;

/// Exact integer projection of the pinned 20 C rot curve. Upstream produces
/// 683 turns for ten minutes and 4,099 for one hour; normal runtime cadence is
/// ten minutes, so repeated checks retain the same per-call truncation.
#[must_use]
pub fn normal_ambient_rot_increment_turns(elapsed_seconds: u64) -> Option<u64> {
    elapsed_seconds
        .checked_mul(NORMAL_AMBIENT_HOURLY_ROT_TURNS)?
        .checked_div(SECONDS_PER_HOUR)
}

#[must_use]
pub fn rot_has_rotten_away(
    shelf_life_turns: u64,
    rot_turns: u64,
    static_corpse: bool,
) -> Option<bool> {
    let threshold = if static_corpse {
        STATIC_CORPSE_REMOVAL_TURNS
    } else {
        shelf_life_turns.checked_mul(2)?
    };
    Some(rot_turns > threshold)
}

pub(super) fn item_rot_metadata_is_valid(
    variables: &BTreeMap<String, ItemVariableValueV1>,
    comestible_type: &str,
    containment: &ItemContainmentProfileV1,
    has_temperature: bool,
    raw_damage: u16,
) -> bool {
    if !item_rot_variables_are_valid(variables) {
        return false;
    }
    let corpse = containment
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("CORPSE"))
        .is_ok();
    let corpse_source = variables.get(ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE);
    match item_rot_state(variables) {
        Some((shelf_life_turns, _)) if corpse => {
            has_temperature
                && raw_damage == MAX_ITEM_RAW_DAMAGE
                && shelf_life_turns == ITEM_STATIC_CORPSE_SHELF_LIFE_TURNS
                && matches!(
                    corpse_source,
                    Some(ItemVariableValueV1::String(source))
                        if !source.is_empty()
                            && source.len() <= MAX_CRAFT_RECIPE_ID_BYTES
                            && source.chars().all(|character| !character.is_control())
                )
        }
        Some(_) => has_temperature && !comestible_type.is_empty() && corpse_source.is_none(),
        None => {
            corpse_source.is_none() && !corpse && (!has_temperature || !comestible_type.is_empty())
        }
    }
}

fn process_item_temperature_and_rot_state(
    temperature: &mut Option<ItemTemperatureStateV1>,
    variables: &mut BTreeMap<String, ItemVariableValueV1>,
    current_tick: SimTick,
    insulation: f32,
) -> Result<bool, SimError> {
    if !insulation.is_finite() || insulation <= 0.0 {
        return Err(SimError::InvalidItem);
    }
    let elapsed_seconds = process_temperature_state(temperature, current_tick)?;
    if !item_rot_variables_are_valid(variables) {
        return Err(SimError::InvalidItem);
    }
    let Some((shelf_life_turns, previous_rot_turns)) = item_rot_state(variables) else {
        return Ok(false);
    };
    let Some(elapsed_seconds) = elapsed_seconds else {
        return Ok(false);
    };
    let static_corpse = variables.contains_key(ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE);
    if !static_corpse
        && rot_has_rotten_away(shelf_life_turns, previous_rot_turns, false)
            .ok_or(SimError::NumericOverflow)?
    {
        return Ok(true);
    }
    let increment =
        normal_ambient_rot_increment_turns(elapsed_seconds).ok_or(SimError::NumericOverflow)?;
    let rot_turns = previous_rot_turns
        .checked_add(increment)
        .ok_or(SimError::NumericOverflow)?;
    let rot_turns_i64 = i64::try_from(rot_turns).map_err(|_| SimError::NumericOverflow)?;
    let Some(ItemVariableValueV1::Integer(stored_rot)) = variables.get_mut(ITEM_ROT_TURNS_VARIABLE)
    else {
        return Err(SimError::InvalidItem);
    };
    *stored_rot = rot_turns_i64;
    rot_has_rotten_away(shelf_life_turns, rot_turns, static_corpse).ok_or(SimError::NumericOverflow)
}

pub(super) fn process_item_snapshot_temperature(
    item: &mut ItemSnapshot,
    current_tick: SimTick,
) -> Result<(), SimError> {
    process_item_snapshot_temperature_and_rot(item, current_tick, false, 1.0).map(|_| ())
}

fn process_item_snapshot_temperature_and_rot(
    item: &mut ItemSnapshot,
    current_tick: SimTick,
    removable: bool,
    parent_insulation: f32,
) -> Result<bool, SimError> {
    let rotten_away = process_item_temperature_and_rot_state(
        &mut item.temperature,
        &mut item.variables,
        current_tick,
        parent_insulation,
    )?;
    if let Some(components) = &mut item.component_provenance {
        for component in components {
            process_component_temperature(component, current_tick)?;
        }
    }
    for pocket in &mut item.integral_magazines {
        if let Some(ammunition) = pocket.loaded_ammunition.as_deref_mut()
            && process_item_snapshot_temperature_and_rot(
                ammunition,
                current_tick,
                removable,
                parent_insulation,
            )?
        {
            pocket.loaded_ammunition = None;
        }
    }
    for well in &mut item.magazine_wells {
        if let Some(magazine) = well.installed_magazine.as_deref_mut()
            && process_item_snapshot_temperature_and_rot(
                magazine,
                current_tick,
                removable,
                parent_insulation,
            )?
        {
            well.installed_magazine = None;
        }
    }
    let pocket_insulations = item
        .ammunition_containers
        .iter()
        .map(|pocket| {
            (
                pocket.pocket_index,
                cdda_protocol::item_pocket_insulation(&item.variables, pocket.pocket_index)
                    .unwrap_or(1.0)
                    .max(parent_insulation),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for pocket in &mut item.ammunition_containers {
        let insulation = *pocket_insulations
            .get(&pocket.pocket_index)
            .ok_or(SimError::InvalidItem)?;
        let mut index = 0;
        while index < pocket.contents.len() {
            if process_item_snapshot_temperature_and_rot(
                pocket
                    .contents
                    .get_mut(index)
                    .ok_or(SimError::InvalidItem)?,
                current_tick,
                removable,
                insulation,
            )? {
                pocket.contents.remove(index);
            } else {
                index += 1;
            }
        }
    }
    Ok(removable && rotten_away)
}

pub(super) fn process_vehicle_cargo_temperature_and_rot(
    item: &mut ItemSnapshot,
    current_tick: SimTick,
) -> Result<bool, SimError> {
    process_item_snapshot_temperature_and_rot(item, current_tick, true, 1.0)
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
    process_item_temperature_and_rot_state(
        &mut component.temperature,
        &mut component.variables,
        current_tick,
        1.0,
    )?;
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
    charge_capacity: ItemGroupChargeCapacityV1,
    tool_charge_storage: Option<ItemGroupToolChargeStorageV1>,
    minimum_one_charge: bool,
    default_charge_range: Option<ItemGroupChargeRangeV1>,
    pub(super) pocket_contents: BTreeMap<u16, Vec<PlannedItemSpawn>>,
    pub(super) collapsed_pockets: BTreeSet<u16>,
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
        temperature: prototype.tracks_temperature.then(|| {
            initial_item_temperature_state(
                birth_tick,
                prototype.containment.phase,
                prototype.thermal_properties,
            )
        }),
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
                    let contents_collapsed = rules.contents_collapsed_by_default;
                    cdda_protocol::SpawnPocketStateV1 {
                        rules,
                        contents_collapsed,
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
                    let contents_collapsed = rules.contents_collapsed_by_default;
                    cdda_protocol::SpawnPocketStateV1 {
                        rules,
                        contents_collapsed,
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
    for pocket_index in &planned.collapsed_pockets {
        let state = item
            .ammunition_containers
            .iter_mut()
            .find(|pocket| pocket.pocket_index == *pocket_index)
            .and_then(|pocket| pocket.spawn_state.as_mut())
            .ok_or(SimError::InvalidItem)?;
        state.contents_collapsed = true;
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
    }) || output
        .iter()
        .any(|item| !planned_static_corpses_are_nonreviving(item))
    {
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

fn select_item_snippet(
    snippets: &[ItemSnippetV1],
    ticket: u64,
) -> Result<Option<ItemSnippetV1>, SimError> {
    if snippets.is_empty() {
        return Ok(None);
    }
    let index =
        usize::try_from(ticket % snippets.len() as u64).map_err(|_| SimError::NumericOverflow)?;
    snippets
        .get(index)
        .cloned()
        .map(Some)
        .ok_or(SimError::InvalidItem)
}

fn construct_item_group_item(
    item: &ItemGroupItemPrototypeV1,
    rng: &mut ChaCha8Rng,
) -> Result<PlannedItemSpawn, SimError> {
    construct_item_group_item_with_fit_phase(item, rng, true)
}

pub(super) fn plan_vehicle_direct_item(
    direct: &WorldgenVehicleDirectItemSpawnV1,
    rng: &mut ChaCha8Rng,
) -> Result<PlannedItemSpawn, SimError> {
    let mut planned = construct_item_group_item_with_fit_phase(&direct.item, rng, false)?;
    if !direct.variant_id.is_empty() {
        let variant = planned
            .variants
            .iter()
            .find(|variant| variant.variant.id == direct.variant_id)
            .cloned()
            .ok_or(SimError::InvalidItem)?;
        set_planned_variant(&mut planned, &variant, rng)?;
    }
    apply_unmodified_default_container(&mut planned, rng)?;
    Ok(planned)
}

pub(super) fn dress_vehicle_spawn_item(
    planned: &mut PlannedItemSpawn,
    with_ammo_percent: u8,
    with_magazine_percent: u8,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let markers = encode_item_group_dressing_marker(with_ammo_percent, with_magazine_percent)
        .into_iter()
        .map(ItemGroupContentsSourceV1::Group)
        .collect::<Vec<_>>();
    apply_item_group_modifier_dressing(planned, None, &markers, rng)
}

pub(super) fn damage_vehicle_spawn_item(
    planned: &mut PlannedItemSpawn,
    rng: &mut ChaCha8Rng,
) -> Result<bool, SimError> {
    if planned.maximum_raw_damage == 0 {
        return Err(SimError::InvalidItem);
    }
    let damage = u16::try_from(inclusive_rng_u64(
        rng,
        1,
        u64::from(planned.maximum_raw_damage),
    ))
    .map_err(|_| SimError::NumericOverflow)?;
    if damage >= planned.maximum_raw_damage {
        return Ok(false);
    }
    planned.raw_damage = damage;
    Ok(true)
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
        select_item_snippet(&item.snippets, rng.next_u64())?
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
        charge_capacity: item.charge_capacity,
        tool_charge_storage: item.tool_charge_storage.clone(),
        minimum_one_charge: item.minimum_one_charge,
        default_charge_range: item.charges,
        pocket_contents: BTreeMap::new(),
        collapsed_pockets: item
            .prototype
            .ammunition_containers
            .iter()
            .filter(|pocket| {
                pocket
                    .spawn_rules
                    .as_ref()
                    .is_some_and(|rules| rules.contents_collapsed_by_default)
            })
            .map(|pocket| pocket.pocket_index)
            .collect(),
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
pub struct ItemGroupDressingProjection {
    pub item_type: String,
    pub magazine_present: bool,
    pub magazine_type: Option<String>,
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
    pub pocket_collapsed: bool,
}

/// Exact physical projection of a generalized whole-group wrapper. Tooling
/// uses this production transition for direct C++ comparison of flexible
/// volume, capacity, constructor presentation defaults, and nested ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemGroupFlexibleWrapperProjection {
    pub outer_type: String,
    pub outer_variant: String,
    pub pocket_rigid: bool,
    pub pocket_collapsed_by_default: bool,
    pub pocket_collapsed: bool,
    pub content_types: Vec<String>,
    pub content_variants: Vec<String>,
    pub content_charges: Vec<i32>,
    pub outer_volume_milliliters: u64,
    pub outer_weight_grams: u64,
    pub pocket_capacity_volume_milliliters: u64,
    pub pocket_remaining_volume_milliliters: u64,
    pub pocket_remaining_weight_grams: u64,
    pub sealed: bool,
}

/// Ordered pocket ownership produced by the generalized whole-group wrapper
/// insertion engine. Empty pockets remain present so differential tooling can
/// prove declared-order first-compatible selection rather than aggregate
/// containment alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemGroupMultiPocketProjection {
    pub outer_type: String,
    pub pocket_contents: Vec<(u16, Vec<String>)>,
}

/// Exact post-modifier ownership trace for the static, maximum-damaged corpse
/// constructor supported by the frozen item-group model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemGroupStaticCorpseProjection {
    pub wrapper_type: String,
    pub wrapper_raw_damage: u16,
    pub wrapper_damage_level: u16,
    pub wrapper_pocket_forbidden: bool,
    pub wrapper_pocket_unloadable: bool,
    pub unloadable_content_count: usize,
    pub content_types: Vec<String>,
    pub content_raw_damage: Vec<u16>,
    pub content_damage_levels: Vec<u16>,
}

/// Exact uniform snippet selection used by production item construction and
/// the direct C++ differential comparator. The caller owns the RNG draw so an
/// empty category consumes no entropy, matching the production constructor.
pub fn item_group_snippet_projection(
    snippets: &[ItemSnippetV1],
    ticket: u64,
) -> Result<Option<ItemSnippetV1>, SimError> {
    select_item_snippet(snippets, ticket)
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
            plan_group_wrapper_explicit_null(item, container, count, None, &mut rng)?
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
        pocket_collapsed: !planned.collapsed_pockets.is_empty(),
    })
}

/// Renderer-free direct projection for the flexible whole-group wrapper
/// family. The optional content variant models an explicit item-group entry
/// modifier, not a synthetic post-construction mutation.
pub fn item_group_flexible_wrapper_projection(
    item: &ItemGroupItemPrototypeV1,
    container: ItemGroupContainerV1,
    count: u16,
    content_variant_id: Option<&str>,
) -> Result<ItemGroupFlexibleWrapperProjection, SimError> {
    let mut rng = ChaCha8Rng::from_seed([0; 32]);
    let planned =
        plan_group_wrapper_explicit_null(item, container, count, content_variant_id, &mut rng)?;
    let [pocket] = planned.prototype.ammunition_containers.as_slice() else {
        return Err(SimError::InvalidItem);
    };
    let rules = pocket.spawn_rules.as_ref().ok_or(SimError::InvalidItem)?;
    if rules.kind != SpawnPocketKindV1::Container {
        return Err(SimError::InvalidItem);
    }
    let contents = planned
        .pocket_contents
        .get(&pocket.pocket_index)
        .ok_or(SimError::InvalidItem)?;
    let contents_volume = contents.iter().try_fold(0_u64, |total, content| {
        total
            .checked_add(
                content
                    .total_volume_milliliters()
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)
    })?;
    let contents_weight = contents.iter().try_fold(0_u64, |total, content| {
        total
            .checked_add(
                content
                    .total_weight_milligrams()
                    .ok_or(SimError::NumericOverflow)?,
            )
            .ok_or(SimError::NumericOverflow)
    })?;
    Ok(ItemGroupFlexibleWrapperProjection {
        outer_type: planned.prototype.type_id.clone(),
        outer_variant: planned
            .variant
            .as_ref()
            .map_or_else(String::new, |variant| variant.id.clone()),
        pocket_rigid: pocket.rigid,
        pocket_collapsed_by_default: rules.contents_collapsed_by_default,
        pocket_collapsed: planned.collapsed_pockets.contains(&pocket.pocket_index),
        content_types: contents
            .iter()
            .map(|content| content.prototype.type_id.clone())
            .collect(),
        content_variants: contents
            .iter()
            .map(|content| {
                content
                    .variant
                    .as_ref()
                    .map_or_else(String::new, |variant| variant.id.clone())
            })
            .collect(),
        content_charges: contents
            .iter()
            .map(|content| content.prototype.charges)
            .collect(),
        outer_volume_milliliters: planned
            .total_volume_milliliters()
            .ok_or(SimError::NumericOverflow)?,
        outer_weight_grams: planned
            .total_weight_milligrams()
            .ok_or(SimError::NumericOverflow)?
            / 1_000,
        pocket_capacity_volume_milliliters: rules.max_contains_volume_milliliters,
        pocket_remaining_volume_milliliters: rules
            .max_contains_volume_milliliters
            .checked_sub(contents_volume)
            .ok_or(SimError::InvalidItem)?,
        pocket_remaining_weight_grams: rules
            .max_contains_weight_milligrams
            .checked_sub(contents_weight)
            .ok_or(SimError::InvalidItem)?
            / 1_000,
        sealed: planned.sealed_pockets.contains(&pocket.pocket_index),
    })
}

pub fn item_group_multi_pocket_projection(
    item: &ItemGroupItemPrototypeV1,
    container: ItemGroupContainerV1,
    count: u16,
) -> Result<ItemGroupMultiPocketProjection, SimError> {
    let mut rng = ChaCha8Rng::from_seed([0; 32]);
    let planned = plan_group_wrapper_explicit_null(item, container, count, None, &mut rng)?;
    let pocket_contents = planned
        .prototype
        .ammunition_containers
        .iter()
        .filter_map(|pocket| {
            pocket.spawn_rules.as_ref().map(|_| {
                (
                    pocket.pocket_index,
                    planned
                        .pocket_contents
                        .get(&pocket.pocket_index)
                        .into_iter()
                        .flatten()
                        .map(|content| content.prototype.type_id.clone())
                        .collect(),
                )
            })
        })
        .collect();
    Ok(ItemGroupMultiPocketProjection {
        outer_type: planned.prototype.type_id,
        pocket_contents,
    })
}

/// Renderer-free direct projection for already selected corpse-wrapper
/// content. The input content order is constructor traversal order; the
/// returned order is canonical pocket ownership order.
pub fn item_group_static_corpse_projection(
    wrapper: &ItemGroupItemPrototypeV1,
    wrapper_raw_damage: u16,
    contents: &[(ItemGroupItemPrototypeV1, u16)],
) -> Result<ItemGroupStaticCorpseProjection, SimError> {
    let mut rng = ChaCha8Rng::from_seed([0; 32]);
    let mut planned = construct_item_group_item_with_fit_phase(wrapper, &mut rng, false)?;
    let fixed_damage_entry = |raw_damage| ItemGroupEntryV1 {
        probability: 100,
        count_min: 1,
        count_max: 1,
        raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
            minimum: raw_damage,
            maximum: raw_damage,
        }),
        variant_id: None,
        event: None,
        target: ItemGroupTargetV1::Item(Box::new(wrapper.clone())),
        modifier_charges: None,
        contents: Vec::new(),
        seal_contents: false,
        modifier_default_container_sealed: None,
        direct_wrapper: None,
        modifier_container: None,
    };
    apply_item_group_modifier_state(
        &mut planned,
        &fixed_damage_entry(wrapper_raw_damage),
        &mut rng,
    )?;
    for (content, raw_damage) in contents {
        let mut content = construct_item_group_item(content, &mut rng)?;
        apply_item_group_modifier_state(&mut content, &fixed_damage_entry(*raw_damage), &mut rng)?;
        if let Err(content) = insert_planned_item(&mut planned, content)? {
            if !planned_item_is_static_corpse(&planned) {
                return Err(SimError::InvalidItem);
            }
            force_insert_planned_corpse_content(&mut planned, content)?;
        }
    }
    if !planned_static_corpses_are_nonreviving(&planned) {
        return Err(SimError::InvalidItem);
    }
    let pocket_index = planned
        .prototype
        .ammunition_containers
        .iter()
        .find_map(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.kind == SpawnPocketKindV1::Container)
                .then_some(pocket.pocket_index)
        })
        .ok_or(SimError::InvalidItem)?;
    let contents = planned
        .pocket_contents
        .get(&pocket_index)
        .ok_or(SimError::InvalidItem)?;
    let pocket = planned
        .prototype
        .ammunition_containers
        .iter()
        .find(|pocket| pocket.pocket_index == pocket_index)
        .ok_or(SimError::InvalidItem)?;
    Ok(ItemGroupStaticCorpseProjection {
        wrapper_type: planned.prototype.type_id,
        wrapper_raw_damage: planned.raw_damage,
        wrapper_damage_level: cdda_protocol::item_damage_level(planned.raw_damage),
        wrapper_pocket_forbidden: pocket
            .spawn_rules
            .as_ref()
            .is_some_and(|rules| rules.forbidden),
        wrapper_pocket_unloadable: pocket.unloadable,
        unloadable_content_count: if pocket.unloadable { contents.len() } else { 0 },
        content_types: contents
            .iter()
            .map(|content| content.prototype.type_id.clone())
            .collect(),
        content_raw_damage: contents.iter().map(|content| content.raw_damage).collect(),
        content_damage_levels: contents
            .iter()
            .map(|content| cdda_protocol::item_damage_level(content.raw_damage))
            .collect(),
    })
}

fn plan_group_wrapper_explicit_null(
    item: &ItemGroupItemPrototypeV1,
    container: ItemGroupContainerV1,
    count: u16,
    content_variant_id: Option<&str>,
    rng: &mut ChaCha8Rng,
) -> Result<PlannedItemSpawn, SimError> {
    let mut entry = direct_default_container_projection_entry(None);
    entry.count_min = count;
    entry.count_max = count;
    entry.variant_id = content_variant_id.map(str::to_owned);
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
        rng,
        &mut output,
        0,
    )?;
    let [planned] = output.try_into().map_err(|_| SimError::InvalidItem)?;
    Ok(planned)
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
        Some(ItemGroupChargeRangeV1 {
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

/// Renderer-free direct projection for inherited group ammunition/magazine
/// dressing. This executes the same constructor, modifier, and storage planner
/// used by production item groups; the seed is explicit so differential cases
/// can retain successful and failed chance boundaries independently.
pub fn item_group_dressing_projection(
    item: &ItemGroupItemPrototypeV1,
    ammunition_chance: u8,
    magazine_chance: u8,
    charges: Option<ItemGroupChargeRangeV1>,
    seed: u64,
) -> Result<ItemGroupDressingProjection, SimError> {
    let marker =
        cdda_protocol::encode_item_group_dressing_marker(ammunition_chance, magazine_chance);
    if marker.is_none() && (ammunition_chance != 0 || magazine_chance != 0) {
        return Err(SimError::InvalidItem);
    }
    let mut item = item.clone();
    item.charges = charges;
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut output = Vec::new();
    let graph = ItemGroupGraphV1 {
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
                modifier_charges: None,
                contents: marker
                    .map(ItemGroupContentsSourceV1::Group)
                    .into_iter()
                    .collect(),
                seal_contents: false,
                modifier_default_container_sealed: Some(true),
                direct_wrapper: None,
                modifier_container: None,
                target: ItemGroupTargetV1::Item(Box::new(item)),
            }],
        }],
        wrapper: None,
    };
    plan_item_group_source_into(
        &ItemGroupSourceV1::Inline(graph),
        &BTreeMap::new(),
        &mut rng,
        &mut output,
        0,
    )?;
    let [planned] = output.try_into().map_err(|_| SimError::InvalidItem)?;
    match &planned.tool_charge_storage {
        Some(ItemGroupToolChargeStorageV1::Integral { .. }) => {
            let [pocket] = planned.prototype.integral_magazines.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            let ammunition = planned.integral_ammunition.get(&pocket.pocket_index);
            let ammunition_remaining =
                ammunition.map_or(0, |ammunition| ammunition.prototype.charges);
            Ok(ItemGroupDressingProjection {
                item_type: planned.prototype.type_id,
                magazine_present: false,
                magazine_type: None,
                ammunition_type: ammunition
                    .map(|ammunition| ammunition.prototype.ammunition_type.clone()),
                ammunition_remaining,
                remaining_capacity: pocket.capacity.saturating_sub(
                    u32::try_from(ammunition_remaining).map_err(|_| SimError::InvalidItem)?,
                ),
            })
        }
        Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index, ..
        }) => {
            let magazine = planned.detachable_magazines.get(well_pocket_index);
            let ammunition = magazine.and_then(|magazine| {
                let [pocket] = magazine.prototype.integral_magazines.as_slice() else {
                    return None;
                };
                magazine.integral_ammunition.get(&pocket.pocket_index)
            });
            Ok(ItemGroupDressingProjection {
                item_type: planned.prototype.type_id,
                magazine_present: magazine.is_some(),
                magazine_type: magazine.map(|magazine| magazine.prototype.type_id.clone()),
                ammunition_type: ammunition
                    .map(|ammunition| ammunition.prototype.ammunition_type.clone()),
                ammunition_remaining: ammunition
                    .map_or(0, |ammunition| ammunition.prototype.charges),
                // Pinned `item::remaining_ammo_capacity()` reports zero on a
                // detachable owner; the installed magazine owns capacity.
                remaining_capacity: 0,
            })
        }
        Some(ItemGroupToolChargeStorageV1::MultiDetachable { .. }) => Err(SimError::InvalidItem),
        None => Err(SimError::InvalidItem),
    }
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
    let collapsed_pockets = prototype
        .ammunition_containers
        .iter()
        .filter(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.contents_collapsed_by_default)
        })
        .map(|pocket| pocket.pocket_index)
        .collect();
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
        charge_capacity: ItemGroupChargeCapacityV1::None,
        tool_charge_storage: None,
        minimum_one_charge: true,
        default_charge_range: None,
        pocket_contents: BTreeMap::new(),
        collapsed_pockets,
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
    charges: Option<ItemGroupChargeRangeV1>,
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
    apply_item_group_modifier_dressing(planned, charges, &entry.contents, rng)?;
    if let (Some(container), Some((wrapper, _))) = (modifier_container, active_wrapper.as_ref()) {
        wrap_single_item(planned, container, wrapper)?;
    }
    insert_item_group_contents(planned, &entry.contents, item_groups, rng)?;
    if entry.seal_contents && !planned.prototype.comestible_type.is_empty() {
        seal_planned_item(planned)?;
    }
    apply_item_group_custom_flags(planned, &entry.contents)?;
    Ok(())
}

fn apply_item_group_custom_flags(
    planned: &mut PlannedItemSpawn,
    sources: &[ItemGroupContentsSourceV1],
) -> Result<(), SimError> {
    let mut seen = BTreeSet::new();
    for source in sources {
        let ItemGroupContentsSourceV1::Group(group_id) = source else {
            continue;
        };
        if !is_reserved_item_group_custom_flag_marker(group_id) {
            continue;
        }
        let flag = decode_item_group_custom_flag_marker(group_id).ok_or(SimError::InvalidItem)?;
        if !seen.insert(flag) {
            return Err(SimError::InvalidItem);
        }
        match planned
            .prototype
            .containment
            .flags
            .binary_search_by(|existing| existing.as_str().cmp(flag))
        {
            Ok(_) => {}
            Err(index) => {
                if planned.prototype.containment.flags.len() >= 256 {
                    return Err(SimError::InvalidItem);
                }
                planned
                    .prototype
                    .containment
                    .flags
                    .insert(index, flag.to_owned());
            }
        }
        if flag == "FIT" {
            planned.fitted = true;
        }
    }
    Ok(())
}

fn apply_item_group_charges(
    planned: &mut PlannedItemSpawn,
    charges: Option<ItemGroupChargeRangeV1>,
    modifier_container_capacity: Option<i32>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let Some(charges) = charges else {
        if planned.prototype.containment.phase == cdda_protocol::ItemPhaseV1::Liquid
            && let Some(capacity) = modifier_container_capacity
        {
            planned.prototype.charges = capacity.max(1);
        }
        return Ok(());
    };
    let maximum_capacity = match planned.charge_capacity {
        ItemGroupChargeCapacityV1::None => None,
        ItemGroupChargeCapacityV1::AmmunitionStorage => item_group_ammunition_capacity(planned)?,
        ItemGroupChargeCapacityV1::ModifierContainer => modifier_container_capacity,
    };
    let Some(charges) =
        resolve_item_group_charge_range(charges, planned.charge_capacity, maximum_capacity)?
    else {
        return Ok(());
    };
    if !planned.charges_supported {
        return Err(SimError::InvalidItem);
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
        Some(ItemGroupToolChargeStorageV1::MultiDetachable { .. }) => {
            // Pinned Item_modifier emits an ambiguity diagnostic and ignores
            // explicit charges on a multi-well owner. Such entries are
            // rejected by protocol validation before simulation.
            return Err(SimError::InvalidItem);
        }
        None => planned.prototype.charges = rolled,
    }
    Ok(())
}

/// Resolve pinned charge endpoints after the concrete output type and modifier
/// container are known. Ammunition capacity supplies only an upper `-1`
/// sentinel; explicit ammunition ranges roll first and clamp while loading.
/// Modifier-container capacity clamps before the roll. `None` is an exact
/// no-op, distinct from a resolved zero which can empty ammunition storage.
pub fn resolve_item_group_charge_range(
    charges: ItemGroupChargeRangeV1,
    capacity_owner: ItemGroupChargeCapacityV1,
    maximum_capacity: Option<i32>,
) -> Result<Option<cdda_protocol::InclusiveI32RangeV1>, SimError> {
    if charges.minimum < -1 || charges.maximum < -1 {
        return Err(SimError::InvalidItem);
    }
    if maximum_capacity.is_some_and(|capacity| capacity < 0)
        || (capacity_owner == ItemGroupChargeCapacityV1::None && maximum_capacity.is_some())
    {
        return Err(SimError::InvalidItem);
    }
    if charges.minimum == -1 && charges.maximum == -1 {
        return Ok(None);
    }
    let applicable_capacity = match capacity_owner {
        ItemGroupChargeCapacityV1::None => None,
        ItemGroupChargeCapacityV1::AmmunitionStorage
            if charges.minimum != -1 && charges.maximum == -1 =>
        {
            maximum_capacity
        }
        ItemGroupChargeCapacityV1::AmmunitionStorage => None,
        ItemGroupChargeCapacityV1::ModifierContainer => maximum_capacity,
    };
    let mut minimum = if charges.minimum == -1 {
        0
    } else {
        charges.minimum
    };
    let mut maximum = if charges.maximum == -1 {
        applicable_capacity.unwrap_or(-1)
    } else {
        charges.maximum
    };
    if let Some(capacity) = applicable_capacity
        && (maximum > capacity || (minimum != 1 && maximum == -1))
    {
        maximum = capacity;
    }
    if minimum > maximum {
        minimum = maximum;
    }
    if minimum == -1 && maximum == -1 {
        return Ok(None);
    }
    if minimum < 0 || maximum < minimum {
        return Err(SimError::InvalidItem);
    }
    Ok(Some(cdda_protocol::InclusiveI32RangeV1 {
        minimum,
        maximum,
    }))
}

fn item_group_ammunition_capacity(planned: &PlannedItemSpawn) -> Result<Option<i32>, SimError> {
    match &planned.tool_charge_storage {
        Some(ItemGroupToolChargeStorageV1::Integral { .. }) => {
            let [pocket] = planned.prototype.integral_magazines.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            Ok(Some(
                i32::try_from(pocket.capacity).map_err(|_| SimError::NumericOverflow)?,
            ))
        }
        Some(ItemGroupToolChargeStorageV1::Detachable { magazine, .. }) => {
            let [pocket] = magazine.integral_magazines.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            Ok(Some(
                i32::try_from(pocket.capacity).map_err(|_| SimError::NumericOverflow)?,
            ))
        }
        Some(ItemGroupToolChargeStorageV1::MultiDetachable { .. }) => Ok(None),
        None => Ok(None),
    }
}

fn modifier_container_charge_capacity(
    planned: &PlannedItemSpawn,
    container: &PlannedItemSpawn,
) -> Result<Option<i32>, SimError> {
    if planned.charge_capacity != ItemGroupChargeCapacityV1::ModifierContainer {
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
    apply_automatic_pocket_collapse(&mut container);
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
    unrestricted.charge_capacity = ItemGroupChargeCapacityV1::ModifierContainer;
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
    if let Some((increments, _)) = item_degradation_state(&planned.initial_variables) {
        let degradation_roll = inclusive_rng_u64(rng, 0, u64::from(planned.raw_damage));
        let degradation = ((degradation_roll as f32 * 50.0_f32) / f32::from(increments)) as u16;
        let degradation = degradation.min(MAX_ITEM_RAW_DAMAGE);
        planned.raw_damage = planned.raw_damage.max(degradation);
        set_planned_integer_variable(
            planned,
            ITEM_DEGRADATION_INCREMENTS_VARIABLE,
            i64::from(increments),
        )?;
        set_planned_integer_variable(planned, ITEM_DEGRADATION_VARIABLE, i64::from(degradation))?;
    }
    if matches!(
        planned
            .initial_variables
            .get(ITEM_GROUP_GUN_FOULING_VARIABLE),
        Some(ItemVariableValueV1::Integer(1))
    ) {
        let dirt =
            i64::try_from(inclusive_rng_u64(rng, 0, 500)).map_err(|_| SimError::NumericOverflow)?;
        if dirt > 0 {
            set_planned_integer_variable(planned, "dirt", dirt)?;
            set_planned_integer_variable(planned, ITEM_GUN_DIRT_FAULT_VARIABLE, 1)?;
        } else {
            let unlubricated = rng.next_u64().is_multiple_of(10)
                && !item_profile_has_flag(&planned.prototype.containment, "NEEDS_NO_LUBE");
            if unlubricated {
                set_planned_integer_variable(planned, ITEM_GUN_UNLUBRICATED_FAULT_VARIABLE, 1)?;
            }
        }
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

fn set_planned_integer_variable(
    planned: &mut PlannedItemSpawn,
    key: &str,
    value: i64,
) -> Result<(), SimError> {
    if !planned.initial_variables.contains_key(key)
        && planned.initial_variables.len() >= MAX_ITEM_VARIABLES
    {
        return Err(SimError::InvalidItem);
    }
    planned
        .initial_variables
        .insert(key.to_owned(), ItemVariableValueV1::Integer(value));
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

fn item_group_dressing_policy(sources: &[ItemGroupContentsSourceV1]) -> Result<(u8, u8), SimError> {
    let mut policy = None;
    for source in sources {
        let ItemGroupContentsSourceV1::Group(group_id) = source else {
            continue;
        };
        if !is_reserved_item_group_dressing_marker(group_id) {
            continue;
        }
        let decoded = decode_item_group_dressing_marker(group_id).ok_or(SimError::InvalidItem)?;
        if policy.replace(decoded).is_some() {
            return Err(SimError::InvalidItem);
        }
    }
    Ok(policy.unwrap_or((0, 0)))
}

fn apply_item_group_modifier_dressing(
    planned: &mut PlannedItemSpawn,
    charges: Option<ItemGroupChargeRangeV1>,
    sources: &[ItemGroupContentsSourceV1],
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let (ammunition_chance, magazine_chance) = item_group_dressing_policy(sources)?;
    if planned.charge_capacity != ItemGroupChargeCapacityV1::AmmunitionStorage {
        return Ok(());
    }

    // Pinned Item_modifier evaluates both rolls for every integral magazine
    // and magazine well, including zero-chance policies. Explicit charges
    // suppress only ammunition dressing and do not remove either draw.
    let ammunition_roll = rng.next_u64() % 100;
    let magazine_roll = rng.next_u64() % 100;
    let charges_not_set =
        charges.is_none_or(|charges| charges.minimum == -1 && charges.maximum == -1);
    let spawn_ammunition = ammunition_roll < u64::from(ammunition_chance) && charges_not_set;

    match planned.tool_charge_storage.clone() {
        Some(ItemGroupToolChargeStorageV1::Integral { ammunition }) => {
            let [pocket] = planned.prototype.integral_magazines.as_slice() else {
                return Err(SimError::InvalidItem);
            };
            let has_ammunition = planned
                .integral_ammunition
                .get(&pocket.pocket_index)
                .is_some_and(|ammunition| ammunition.prototype.charges > 0);
            if spawn_ammunition && !has_ammunition {
                let charges =
                    i32::try_from(pocket.capacity).map_err(|_| SimError::NumericOverflow)?;
                let loaded = construct_charge_ammunition(&ammunition, charges, rng)?;
                planned
                    .integral_ammunition
                    .insert(pocket.pocket_index, Box::new(loaded));
            }
        }
        Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index,
            magazine,
            ammunition,
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
            let current_magazine = planned.detachable_magazines.get(&well_pocket_index);
            if current_magazine.is_some_and(|installed| installed.prototype != magazine) {
                return Err(SimError::InvalidItem);
            }
            let has_ammunition = current_magazine
                .and_then(|installed| {
                    installed
                        .integral_ammunition
                        .get(&magazine_pocket.pocket_index)
                })
                .is_some_and(|ammunition| ammunition.prototype.charges > 0);
            let has_current_magazine = current_magazine.is_some();
            let spawn_ammunition = spawn_ammunition && !has_ammunition;
            let spawn_magazine =
                magazine_roll < u64::from(magazine_chance) && !has_current_magazine;
            if spawn_magazine {
                let loaded_magazine = construct_charge_ammunition(&magazine, 0, rng)?;
                planned
                    .detachable_magazines
                    .insert(well_pocket_index, Box::new(loaded_magazine));
            }
            if spawn_ammunition && (spawn_magazine || has_current_magazine) {
                let installed = planned
                    .detachable_magazines
                    .get_mut(&well_pocket_index)
                    .ok_or(SimError::InvalidItem)?;
                let charges = i32::try_from(magazine_pocket.capacity)
                    .map_err(|_| SimError::NumericOverflow)?;
                let loaded = construct_charge_ammunition(&ammunition, charges, rng)?;
                installed
                    .integral_ammunition
                    .insert(magazine_pocket.pocket_index, Box::new(loaded));
            }
        }
        Some(ItemGroupToolChargeStorageV1::MultiDetachable { wells }) => {
            let spawn_magazine = magazine_roll < u64::from(magazine_chance);
            for storage in wells {
                let well = planned
                    .prototype
                    .magazine_wells
                    .iter()
                    .find(|well| well.pocket_index == storage.well_pocket_index)
                    .ok_or(SimError::InvalidItem)?;
                if well
                    .compatible_magazine_type_ids
                    .binary_search(&storage.magazine.type_id)
                    .is_err()
                {
                    return Err(SimError::InvalidItem);
                }
                let [magazine_pocket] = storage.magazine.integral_magazines.as_slice() else {
                    return Err(SimError::InvalidItem);
                };
                let current = planned.detachable_magazines.get(&storage.well_pocket_index);
                if current.is_some_and(|installed| installed.prototype != storage.magazine) {
                    return Err(SimError::InvalidItem);
                }
                if current.is_none() && spawn_magazine {
                    let magazine = construct_charge_ammunition(&storage.magazine, 0, rng)?;
                    planned
                        .detachable_magazines
                        .insert(storage.well_pocket_index, Box::new(magazine));
                }
                if spawn_ammunition {
                    let Some(installed) = planned
                        .detachable_magazines
                        .get_mut(&storage.well_pocket_index)
                    else {
                        continue;
                    };
                    let has_ammunition = installed
                        .integral_ammunition
                        .get(&magazine_pocket.pocket_index)
                        .is_some_and(|ammunition| ammunition.prototype.charges > 0);
                    if !has_ammunition {
                        let charges = i32::try_from(magazine_pocket.capacity)
                            .map_err(|_| SimError::NumericOverflow)?;
                        let ammunition =
                            construct_charge_ammunition(&storage.ammunition, charges, rng)?;
                        installed
                            .integral_ammunition
                            .insert(magazine_pocket.pocket_index, Box::new(ammunition));
                    }
                }
            }
        }
        None if ammunition_chance == 0 && magazine_chance == 0 => {}
        None => return Err(SimError::InvalidItem),
    }
    Ok(())
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
    apply_automatic_pocket_collapse(planned);
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
            if planned_item_is_static_corpse(&container) {
                force_insert_planned_corpse_content(&mut container, payload)?;
                continue;
            }
            match wrapper.overflow {
                ItemGroupOverflowV1::Spill => excess.push(payload),
                ItemGroupOverflowV1::None | ItemGroupOverflowV1::Discard => {}
            }
        }
    }
    apply_automatic_pocket_collapse(&mut container);
    if wrapper.sealed {
        seal_planned_item(&mut container)?;
    }
    output.extend(excess);
    output.push(container);
    validate_planned_output_bound(output)
}

fn planned_item_is_static_corpse(item: &PlannedItemSpawn) -> bool {
    item.prototype
        .containment
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("CORPSE"))
        .is_ok()
        && matches!(
            item.initial_variables
                .get(ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE),
            Some(ItemVariableValueV1::String(source)) if !source.is_empty()
        )
}

fn planned_static_corpses_are_nonreviving(item: &PlannedItemSpawn) -> bool {
    let corpse_flag = item
        .prototype
        .containment
        .flags
        .binary_search_by(|flag| flag.as_str().cmp("CORPSE"))
        .is_ok();
    let corpse_source = item
        .initial_variables
        .get(ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE);
    let valid_self = match (corpse_flag, corpse_source) {
        (true, Some(ItemVariableValueV1::String(source))) => {
            !source.is_empty() && item.raw_damage == MAX_ITEM_RAW_DAMAGE
        }
        (false, None) => true,
        (true, Some(ItemVariableValueV1::Integer(_))) | (true, None) | (false, Some(_)) => false,
    };
    valid_self
        && item
            .pocket_contents
            .values()
            .flatten()
            .all(planned_static_corpses_are_nonreviving)
        && item
            .integral_ammunition
            .values()
            .all(|ammunition| planned_static_corpses_are_nonreviving(ammunition))
        && item
            .detachable_magazines
            .values()
            .all(|magazine| planned_static_corpses_are_nonreviving(magazine))
}

fn force_insert_planned_corpse_content(
    target: &mut PlannedItemSpawn,
    payload: PlannedItemSpawn,
) -> Result<(), SimError> {
    let pocket_index = target
        .prototype
        .ammunition_containers
        .iter()
        .find_map(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.kind == SpawnPocketKindV1::Container)
                .then_some(pocket.pocket_index)
        })
        .ok_or(SimError::InvalidItem)?;
    if payload
        .containment_depth()
        .and_then(|depth| depth.checked_add(1))
        .is_none_or(|depth| depth > MAX_ITEM_COMPONENT_DEPTH)
    {
        return Err(SimError::InvalidItem);
    }
    target
        .pocket_contents
        .entry(pocket_index)
        .or_default()
        .insert(0, payload);
    Ok(())
}

fn insert_item_group_contents(
    target: &mut PlannedItemSpawn,
    sources: &[ItemGroupContentsSourceV1],
    item_groups: &BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let _ = item_group_dressing_policy(sources)?;
    let actual_source_count = sources
        .iter()
        .filter(|source| {
            !matches!(
                source,
                ItemGroupContentsSourceV1::Group(group_id)
                    if is_reserved_item_group_internal_marker(group_id)
            )
        })
        .count();
    for source in sources {
        if matches!(
            source,
            ItemGroupContentsSourceV1::Group(group_id)
                if is_reserved_item_group_internal_marker(group_id)
        ) {
            continue;
        }
        if actual_source_count > 1 {
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
                    let multiplier =
                        item_pocket_weight_multiplier(&self.initial_variables, *pocket_index)?;
                    contents.iter().try_fold(total, |total, child| {
                        total.checked_add(spawn_pocket_content_weight_with_multiplier_milligrams(
                            child.total_weight_milligrams()?,
                            multiplier,
                        )?)
                    })
                })?;
        own.checked_add(integral)?
            .checked_add(detachable)?
            .checked_add(pocketed)
    }

    pub(super) fn total_volume_milliliters(&self) -> Option<u64> {
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
                    let contents_volume = contents.iter().try_fold(0_u64, |volume, child| {
                        volume.checked_add(child.total_volume_milliliters()?)
                    })?;
                    let external = match pocket.spawn_rules.as_ref() {
                        None => Some(if pocket.rigid { 0 } else { contents_volume }),
                        Some(rules) => spawn_pocket_external_volume_with_multiplier_milliliters(
                            rules,
                            contents_volume,
                            item_pocket_volume_multiplier(&self.initial_variables, *pocket_index)?,
                        ),
                    }?;
                    total.checked_add(external)
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
    let mut selected_pocket_index = None;
    for pocket in &target.prototype.ammunition_containers {
        let Some(rules) = pocket
            .spawn_rules
            .as_ref()
            .filter(|rules| rules.kind == preferred_kind)
        else {
            continue;
        };
        if spawn_pocket_accepts(target, pocket.pocket_index, rules, &payload)? {
            selected_pocket_index = Some(pocket.pocket_index);
            break;
        }
    }
    let Some(pocket_index) = selected_pocket_index else {
        return Ok(Err(payload));
    };
    if payload
        .containment_depth()
        .and_then(|depth| depth.checked_add(1))
        .is_none_or(|depth| depth > MAX_ITEM_COMPONENT_DEPTH)
    {
        return Err(SimError::InvalidItem);
    }
    let contents = target.pocket_contents.entry(pocket_index).or_default();
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
    let restricted = cdda_protocol::spawn_pocket_has_item_restrictions(rules)
        || !rules.flag_restrictions.is_empty();
    let accepted_restriction = rules.item_restrictions.iter().any(|restriction| {
        !cdda_protocol::is_reserved_spawn_pocket_marker(restriction)
            && restriction == &payload.prototype.type_id
    }) || rules
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
    if cdda_protocol::spawn_pocket_is_single_item(rules)
        && !existing.is_empty()
        && !(existing.len() == 1 && planned_items_can_combine_for_containment(existing[0], payload))
    {
        return Ok(false);
    }
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
        && left.collapsed_pockets == right.collapsed_pockets
        && left.sealed_pockets == right.sealed_pockets
}

fn apply_automatic_pocket_collapse(item: &mut PlannedItemSpawn) {
    let physical_pockets = item
        .prototype
        .ammunition_containers
        .iter()
        .filter(|pocket| {
            pocket
                .spawn_rules
                .as_ref()
                .is_some_and(|rules| rules.kind == SpawnPocketKindV1::Container)
        })
        .map(|pocket| pocket.pocket_index)
        .collect::<Vec<_>>();
    for pocket_index in physical_pockets {
        let Some(contents) = item.pocket_contents.get(&pocket_index) else {
            continue;
        };
        if !contents.is_empty() && contents.windows(2).all(|pair| pair[0] == pair[1]) {
            item.collapsed_pockets.insert(pocket_index);
        }
    }
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
        if cdda_protocol::spawn_pocket_is_single_item(rules) {
            continue;
        }
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
        encode_item_group_dressing_marker,
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
                thermal_properties: None,
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
            charge_capacity: ItemGroupChargeCapacityV1::ModifierContainer,
            contents_insertion_supported: true,
        }
    }

    fn leaf(type_id: &str) -> ItemGroupTargetV1 {
        ItemGroupTargetV1::Item(Box::new(leaf_item(type_id)))
    }

    #[test]
    fn uniform_named_snippet_projection_selects_exact_boundaries() {
        let snippets = [
            ItemSnippetV1 {
                id: String::from("first"),
                text: String::from("first text"),
            },
            ItemSnippetV1 {
                id: String::from("middle"),
                text: String::from("middle text"),
            },
            ItemSnippetV1 {
                id: String::from("last"),
                text: String::from("last text"),
            },
        ];
        assert_eq!(
            item_group_snippet_projection(&snippets, 0)
                .expect("first ticket should select")
                .expect("nonempty snippets should select")
                .id,
            "first"
        );
        assert_eq!(
            item_group_snippet_projection(&snippets, 2)
                .expect("last ticket should select")
                .expect("nonempty snippets should select")
                .id,
            "last"
        );
        assert_eq!(
            item_group_snippet_projection(&snippets, 3)
                .expect("wrapped ticket should select")
                .expect("nonempty snippets should select")
                .id,
            "first"
        );
        assert_eq!(
            item_group_snippet_projection(&[], 0).expect("empty snippets are valid"),
            None
        );
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
                magazine_well_volume_milliliters: 0,
                contents_collapsed_by_default: false,
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
                ItemPhaseV1::Solid,
                None,
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

        let mut material_prototype = temperature_prototype();
        material_prototype.type_id = String::from("water_clean");
        material_prototype.containment.phase = ItemPhaseV1::Liquid;
        material_prototype.thermal_properties = Some(cdda_protocol::ItemThermalPropertiesV1 {
            specific_heat_liquid_microjoules_per_gram_kelvin: 4_186_000,
            specific_heat_solid_microjoules_per_gram_kelvin: 2_108_000,
            latent_heat_microjoules_per_gram: 333_000_000,
            freezing_point_millikelvin: 273_150,
        });
        let mut material =
            item_from_craft_prototype(ItemId::new(1, 2), &material_prototype, birth_tick);
        material
            .process_temperature(processing_tick)
            .expect("material-backed temperature should initialize at the boundary");
        let material_state = material
            .temperature
            .expect("temperature state should exist");
        assert_eq!(material_state.current_phase, ItemPhaseV1::Liquid);
        assert_eq!(
            material_state.specific_energy_millijoules_per_gram,
            Some(992_520)
        );
        assert_eq!(
            ItemInstance::from_snapshot(&material.snapshot())
                .expect("material-backed temperature should restore")
                .snapshot(),
            material.snapshot()
        );

        let mut whiskey_prototype = material_prototype;
        whiskey_prototype.type_id = String::from("whiskey");
        whiskey_prototype.thermal_properties = Some(cdda_protocol::ItemThermalPropertiesV1 {
            specific_heat_liquid_microjoules_per_gram_kelvin: 4_000_000,
            specific_heat_solid_microjoules_per_gram_kelvin: 2_000_000,
            latent_heat_microjoules_per_gram: 310_000_000,
            freezing_point_millikelvin: 243_150,
        });
        let mut whiskey =
            item_from_craft_prototype(ItemId::new(1, 3), &whiskey_prototype, birth_tick);
        whiskey
            .process_temperature(processing_tick)
            .expect("custom freezing should use the generalized thermal curve");
        let whiskey_state = whiskey
            .temperature
            .expect("custom temperature state should exist");
        assert_eq!(whiskey_state.current_phase, ItemPhaseV1::Liquid);
        assert_eq!(
            whiskey_state.specific_energy_millijoules_per_gram,
            Some(996_300)
        );
        assert_eq!(
            ItemInstance::from_snapshot(&whiskey.snapshot())
                .expect("custom freezing state should restore")
                .snapshot(),
            whiskey.snapshot()
        );
    }

    fn perishable_item(shelf_life_turns: i64) -> ItemInstance {
        let mut item =
            item_from_craft_prototype(ItemId::new(1, 90), &temperature_prototype(), SimTick(0));
        item.variables.insert(
            cdda_protocol::ITEM_ROT_SHELF_LIFE_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(shelf_life_turns),
        );
        item.variables.insert(
            ITEM_ROT_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(0),
        );
        item
    }

    #[test]
    fn normal_ambient_rot_matches_exact_upstream_intervals_and_boundaries() {
        assert_eq!(normal_ambient_rot_increment_turns(10 * 60), Some(683));
        assert_eq!(normal_ambient_rot_increment_turns(60 * 60), Some(4_099));
        assert_eq!(rot_has_rotten_away(86_400, 172_800, false), Some(false));
        assert_eq!(rot_has_rotten_away(86_400, 172_801, false), Some(true));
        assert_eq!(rot_has_rotten_away(86_400, 864_000, true), Some(false));
        assert_eq!(rot_has_rotten_away(86_400, 864_001, true), Some(true));

        let mut item = perishable_item(86_400);
        assert!(!item
            .process_temperature_and_rot(
                SimTick(ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS),
                true,
            )
            .expect("ten-minute rot should process"));
        assert_eq!(item_rot_state(&item.variables), Some((86_400, 683)));

        let mut carried = perishable_item(86_400);
        carried.variables.insert(
            ITEM_ROT_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(172_800),
        );
        assert!(
            !carried
                .process_temperature_and_rot(
                    SimTick(ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS),
                    false,
                )
                .expect("carried rotten food should remain physical")
        );
        assert_eq!(item_rot_state(&carried.variables), Some((86_400, 173_483)));

        let mut ground = perishable_item(86_400);
        ground.variables.insert(
            ITEM_ROT_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(172_800),
        );
        assert!(ground
            .process_temperature_and_rot(
                SimTick(ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS),
                true,
            )
            .expect("ground rotten food should be removable"));
    }

    #[test]
    fn nested_ground_rot_removes_only_the_rotten_owned_content() {
        let mut outer = item_from_craft_prototype(
            ItemId::new(1, 91),
            &leaf_item("lunchbox").prototype,
            SimTick(0),
        );
        let mut content = perishable_item(86_400);
        content.variables.insert(
            ITEM_ROT_TURNS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(172_800),
        );
        outer
            .ammunition_containers
            .push(AmmunitionContainerPocketSnapshotV1 {
                pocket_index: 0,
                pocket_id: String::from("CONTAINER"),
                capacities: Vec::new(),
                rigid: true,
                access_moves: 100,
                reloadable: false,
                unloadable: true,
                spawn_state: None,
                contents: vec![content.snapshot()],
            });
        assert!(!outer
            .process_temperature_and_rot(
                SimTick(ITEM_TEMPERATURE_PROCESS_INTERVAL_TICKS),
                true,
            )
            .expect("outer container should process nested rot"));
        assert!(outer.ammunition_containers[0].contents.is_empty());
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
        let insulation_key = cdda_protocol::item_pocket_insulation_variable_key(0);
        owner.variables.insert(
            insulation_key.clone(),
            ItemVariableValueV1::Integer(i64::from(10.0_f32.to_bits())),
        );
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
        assert_eq!(
            cdda_protocol::item_pocket_insulation(&owner.variables, 0),
            Some(10.0),
        );
        assert!(owner.variables.contains_key(&insulation_key));
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
                pocket_collapsed: true,
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
                pocket_collapsed: true,
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
                pocket_collapsed: false,
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
                pocket_collapsed: true,
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
                pocket_collapsed: true,
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
    fn degradation_fouling_and_custom_flags_follow_modifier_rng_order() {
        let mut vehicle_part = leaf_item("steel_frame");
        vehicle_part.maximum_raw_damage = MAX_ITEM_RAW_DAMAGE;
        vehicle_part.initial_variables.insert(
            ITEM_DEGRADATION_INCREMENTS_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(50),
        );
        vehicle_part.initial_variables.insert(
            ITEM_DEGRADATION_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(0),
        );
        let custom_flag = cdda_protocol::encode_item_group_custom_flag_marker("WET")
            .expect("canonical custom flag");
        let source = |item: ItemGroupItemPrototypeV1, contents| {
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
                            minimum: 1_000,
                            maximum: 1_000,
                        }),
                        variant_id: None,
                        event: None,
                        target: ItemGroupTargetV1::Item(Box::new(item)),
                        modifier_charges: None,
                        contents,
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            })
        };

        let seed = 51;
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..5 {
            let _ = expected_rng.next_u64();
        }
        let expected_degradation = expected_rng.next_u64() % 1_001;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let [planned] = plan_item_group_source(
            &source(
                vehicle_part,
                vec![ItemGroupContentsSourceV1::Group(custom_flag.clone())],
            ),
            &BTreeMap::new(),
            &mut rng,
        )
        .expect("vehicle-part modifier should plan")
        .try_into()
        .expect("one vehicle part");
        assert_eq!(planned.raw_damage, 1_000);
        assert_eq!(
            planned.initial_variables.get(ITEM_DEGRADATION_VARIABLE),
            Some(&ItemVariableValueV1::Integer(
                i64::try_from(expected_degradation).expect("bounded degradation"),
            ))
        );
        assert!(item_profile_has_flag(&planned.prototype.containment, "WET"));
        assert_eq!(rng.next_u64(), expected_rng.next_u64());

        let mut gun = leaf_item("service_rifle");
        gun.maximum_raw_damage = MAX_ITEM_RAW_DAMAGE;
        gun.initial_variables.insert(
            ITEM_GROUP_GUN_FOULING_VARIABLE.to_owned(),
            ItemVariableValueV1::Integer(1),
        );
        let gun_seed = (1_u64..100)
            .find(|seed| {
                let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                for _ in 0..5 {
                    let _ = rng.next_u64();
                }
                rng.next_u64() % 501 > 0
            })
            .expect("a deterministic nonzero dirt witness");
        let mut expected_rng = ChaCha8Rng::seed_from_u64(gun_seed);
        for _ in 0..5 {
            let _ = expected_rng.next_u64();
        }
        let expected_dirt = expected_rng.next_u64() % 501;
        let mut rng = ChaCha8Rng::seed_from_u64(gun_seed);
        let [planned] =
            plan_item_group_source(&source(gun, Vec::new()), &BTreeMap::new(), &mut rng)
                .expect("ordinary gun modifier should plan")
                .try_into()
                .expect("one gun");
        assert_eq!(
            planned.initial_variables.get("dirt"),
            Some(&ItemVariableValueV1::Integer(
                i64::try_from(expected_dirt).expect("bounded dirt"),
            ))
        );
        assert_eq!(
            planned.initial_variables.get(ITEM_GUN_DIRT_FAULT_VARIABLE),
            Some(&ItemVariableValueV1::Integer(1))
        );
        assert_eq!(rng.next_u64(), expected_rng.next_u64());

        let duplicated_flags = vec![
            ItemGroupContentsSourceV1::Group(custom_flag.clone()),
            ItemGroupContentsSourceV1::Group(custom_flag),
        ];
        assert!(matches!(
            plan_item_group_source(
                &source(leaf_item("hostile_flags"), duplicated_flags),
                &BTreeMap::new(),
                &mut ChaCha8Rng::seed_from_u64(1),
            ),
            Err(SimError::InvalidItem)
        ));
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
        item.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
        item.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        item.charges = Some(ItemGroupChargeRangeV1 {
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
        target.charges = Some(ItemGroupChargeRangeV1 {
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
        target.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
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
            target.charges = Some(ItemGroupChargeRangeV1 {
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
            target.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
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

        let mut ranged = source(0);
        let ItemGroupSourceV1::Inline(graph) = &mut ranged else {
            unreachable!("the fixture uses an inline group")
        };
        let ItemGroupTargetV1::Item(target) = &mut graph.nodes[0].entries[0].target else {
            unreachable!("the fixture uses a direct item")
        };
        target.charges = Some(ItemGroupChargeRangeV1 {
            minimum: 0,
            maximum: 100,
        });
        let (seed, expected_loaded, preclamped_loaded) = (1_u64..1_000)
            .find_map(|seed| {
                let roll = |maximum| {
                    let mut rng = ChaCha8Rng::seed_from_u64(seed);
                    for _ in 0..5 {
                        let _constructor_phase = rng.next_u64();
                    }
                    inclusive_rng_u64(&mut rng, 0, maximum)
                };
                let expected = roll(100).min(56);
                let preclamped = roll(56);
                (expected > 0 && expected != preclamped).then_some((
                    seed,
                    expected as i32,
                    preclamped as i32,
                ))
            })
            .expect("a deterministic seed must distinguish roll-then-clamp ordering");
        let mut actual_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(&ranged, &BTreeMap::new(), &mut actual_rng)
            .expect("an explicit detachable range should plan");
        assert_eq!(
            planned[0].detachable_magazines[&4].integral_ammunition[&0]
                .prototype
                .charges,
            expected_loaded
        );
        assert_ne!(expected_loaded, preclamped_loaded);
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..12 {
            let _expected_phase = expected_rng.next_u64();
        }
        assert_eq!(actual_rng.next_u64(), expected_rng.next_u64());

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
                    modifier_charges: Some(ItemGroupChargeRangeV1 {
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
    fn group_dressing_fills_integral_and_detachable_defaults_with_exact_phases() {
        let marker = |ammunition, magazine| {
            ItemGroupContentsSourceV1::Group(
                encode_item_group_dressing_marker(ammunition, magazine)
                    .expect("bounded nonzero dressing should encode"),
            )
        };
        let source = |target: ItemGroupItemPrototypeV1, contents| {
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
                        contents,
                        seal_contents: false,
                        modifier_default_container_sealed: Some(true),
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            })
        };
        let ammunition = || {
            let mut ammunition = leaf_item("battery").prototype;
            ammunition.ammunition_type = String::from("battery");
            ammunition.containment = ItemContainmentProfileV1 {
                count_by_charges: true,
                stack_size: 100,
                ..ItemContainmentProfileV1::default()
            };
            ammunition
        };

        let mut integral = leaf_item("integral_light");
        integral.prototype.charges = 0;
        integral.prototype.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
            pocket_index: 2,
            pocket_id: String::from("BATTERY"),
            ammunition_type: String::from("battery"),
            capacity: 20,
            rigid: true,
            reloadable: false,
            unloadable: false,
        }];
        integral.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::Integral {
            ammunition: ammunition(),
        });
        integral.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
        let seed = 31;
        let mut integral_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(
            &source(integral.clone(), vec![marker(100, 100)]),
            &BTreeMap::new(),
            &mut integral_rng,
        )
        .expect("integral dressing should plan");
        assert_eq!(planned[0].integral_ammunition[&2].prototype.charges, 20);
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..9 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(integral_rng.next_u64(), expected_rng.next_u64());

        let failure_seed = (1_u64..100)
            .find(|seed| {
                let mut rng = ChaCha8Rng::seed_from_u64(*seed);
                for _ in 0..5 {
                    let _ = rng.next_u64();
                }
                rng.next_u64() % 100 >= 50
            })
            .expect("a deterministic failing ammunition ticket should exist");
        let mut failure_rng = ChaCha8Rng::seed_from_u64(failure_seed);
        let failed = plan_item_group_source(
            &source(integral.clone(), vec![marker(50, 100)]),
            &BTreeMap::new(),
            &mut failure_rng,
        )
        .expect("a failed dressing chance should still plan");
        assert!(failed[0].integral_ammunition.is_empty());
        let mut expected_rng = ChaCha8Rng::seed_from_u64(failure_seed);
        for _ in 0..7 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(failure_rng.next_u64(), expected_rng.next_u64());

        integral.charges = Some(ItemGroupChargeRangeV1 {
            minimum: 0,
            maximum: 0,
        });
        let mut explicit_rng = ChaCha8Rng::seed_from_u64(seed);
        let explicit = plan_item_group_source(
            &source(integral, vec![marker(100, 100)]),
            &BTreeMap::new(),
            &mut explicit_rng,
        )
        .expect("explicit zero charges should suppress ammunition dressing");
        assert!(explicit[0].integral_ammunition.is_empty());
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..7 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(explicit_rng.next_u64(), expected_rng.next_u64());

        let mut detachable = leaf_item("wearable_light");
        detachable.prototype.charges = 0;
        detachable.prototype.magazine_wells = vec![MagazineWellPrototypeV1 {
            pocket_index: 4,
            pocket_id: String::from("BATTERY_WELL"),
            compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
            rigid: true,
            unloadable: true,
        }];
        let mut magazine = leaf_item("medium_battery_cell").prototype;
        magazine.charges = 0;
        magazine.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("MAGAZINE"),
            ammunition_type: String::from("battery"),
            capacity: 56,
            rigid: true,
            reloadable: false,
            unloadable: false,
        }];
        detachable.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index: 4,
            magazine,
            ammunition: Box::new(ammunition()),
        });
        detachable.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
        let mut ammunition_only_rng = ChaCha8Rng::seed_from_u64(seed);
        let ammunition_only = plan_item_group_source(
            &source(detachable.clone(), vec![marker(100, 0)]),
            &BTreeMap::new(),
            &mut ammunition_only_rng,
        )
        .expect("ammunition-only detachable dressing should plan");
        assert!(
            ammunition_only[0].detachable_magazines.is_empty(),
            "without an installed magazine, upstream ammunition chance alone has no target"
        );
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..7 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(ammunition_only_rng.next_u64(), expected_rng.next_u64());

        let mut detachable_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(
            &source(detachable.clone(), vec![marker(100, 100)]),
            &BTreeMap::new(),
            &mut detachable_rng,
        )
        .expect("detachable dressing should plan");
        let magazine = &planned[0].detachable_magazines[&4];
        assert_eq!(magazine.prototype.type_id, "medium_battery_cell");
        assert_eq!(magazine.integral_ammunition[&0].prototype.charges, 56);
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..11 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(detachable_rng.next_u64(), expected_rng.next_u64());

        let Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index,
            magazine,
            ammunition,
        }) = detachable.tool_charge_storage.clone()
        else {
            panic!("fixture should retain detachable storage")
        };
        let mut multi = detachable;
        multi
            .prototype
            .magazine_wells
            .push(MagazineWellPrototypeV1 {
                pocket_index: 5,
                pocket_id: String::from("SECOND_BATTERY_WELL"),
                compatible_magazine_type_ids: vec![magazine.type_id.clone()],
                rigid: true,
                unloadable: true,
            });
        multi.charges_supported = false;
        multi.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::MultiDetachable {
            wells: vec![
                cdda_protocol::ItemGroupDetachableStorageV1 {
                    well_pocket_index,
                    magazine: magazine.clone(),
                    ammunition: ammunition.clone(),
                },
                cdda_protocol::ItemGroupDetachableStorageV1 {
                    well_pocket_index: 5,
                    magazine,
                    ammunition,
                },
            ],
        });
        let mut multi_rng = ChaCha8Rng::seed_from_u64(seed);
        let planned = plan_item_group_source(
            &source(multi, vec![marker(100, 100)]),
            &BTreeMap::new(),
            &mut multi_rng,
        )
        .expect("multi-well dressing should plan");
        assert_eq!(planned[0].object_count(), Some(5));
        for pocket_index in [4, 5] {
            let installed = &planned[0].detachable_magazines[&pocket_index];
            assert_eq!(installed.prototype.type_id, "medium_battery_cell");
            assert_eq!(installed.integral_ammunition[&0].prototype.charges, 56);
        }
        let mut expected_rng = ChaCha8Rng::seed_from_u64(seed);
        for _ in 0..15 {
            let _ = expected_rng.next_u64();
        }
        assert_eq!(
            multi_rng.next_u64(),
            expected_rng.next_u64(),
            "one ammunition chance and one magazine chance must be shared across every well"
        );
    }

    #[test]
    fn group_dressing_rejects_malformed_and_duplicate_reserved_policies() {
        let source = |contents| {
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
                        target: leaf("hostile_dressing"),
                        modifier_charges: None,
                        contents,
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: None,
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            })
        };
        let marker = encode_item_group_dressing_marker(1, 1).expect("policy should encode");
        for contents in [
            vec![ItemGroupContentsSourceV1::Group(String::from(
                "__CDDA_ITEM_GROUP_DRESSING_V1:1:101",
            ))],
            vec![
                ItemGroupContentsSourceV1::Group(marker.clone()),
                ItemGroupContentsSourceV1::Group(marker.clone()),
            ],
        ] {
            assert!(
                plan_item_group_source(
                    &source(contents),
                    &BTreeMap::new(),
                    &mut ChaCha8Rng::seed_from_u64(1),
                )
                .is_err()
            );
        }
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
        target.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
        target.charges = Some(ItemGroupChargeRangeV1 {
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
            Some(ItemGroupChargeRangeV1 {
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
            &source(Some(ItemGroupChargeRangeV1 {
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
    fn charge_capacity_sentinels_resolve_all_pinned_endpoint_families() {
        let resolved = |minimum, maximum, owner, capacity| {
            resolve_item_group_charge_range(
                ItemGroupChargeRangeV1 { minimum, maximum },
                owner,
                capacity,
            )
            .expect("characterized charge range should resolve")
            .map(|range| (range.minimum, range.maximum))
        };
        assert_eq!(
            resolved(-1, -1, ItemGroupChargeCapacityV1::None, None),
            None
        );
        assert_eq!(resolved(0, -1, ItemGroupChargeCapacityV1::None, None), None);
        assert_eq!(resolved(4, -1, ItemGroupChargeCapacityV1::None, None), None);
        assert_eq!(
            resolved(
                0,
                -1,
                ItemGroupChargeCapacityV1::AmmunitionStorage,
                Some(85)
            ),
            Some((0, 85))
        );
        assert_eq!(
            resolved(
                0,
                -1,
                ItemGroupChargeCapacityV1::AmmunitionStorage,
                Some(56)
            ),
            Some((0, 56))
        );
        assert_eq!(
            resolved(1, -1, ItemGroupChargeCapacityV1::ModifierContainer, Some(2)),
            Some((1, 2))
        );
        assert_eq!(
            resolved(
                0,
                100,
                ItemGroupChargeCapacityV1::AmmunitionStorage,
                Some(56)
            ),
            Some((0, 100)),
            "explicit ammunition ranges roll before the loaded result is clamped"
        );
        assert_eq!(
            resolved(
                50,
                80,
                ItemGroupChargeCapacityV1::ModifierContainer,
                Some(2)
            ),
            Some((2, 2)),
            "physical modifier containers clamp before the roll"
        );
        assert_eq!(
            resolved(-1, 4, ItemGroupChargeCapacityV1::None, None),
            Some((0, 4))
        );
        assert_eq!(
            resolved(7, 2, ItemGroupChargeCapacityV1::None, None),
            Some((2, 2))
        );
        assert!(
            resolve_item_group_charge_range(
                ItemGroupChargeRangeV1 {
                    minimum: -2,
                    maximum: 4,
                },
                ItemGroupChargeCapacityV1::None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn integral_tool_sentinel_resolves_against_its_magazine_capacity() {
        let mut tablet = leaf_item("eink_tablet_pc");
        tablet.prototype.charges = 0;
        tablet.prototype.integral_magazines =
            vec![cdda_protocol::IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("MAGAZINE"),
                ammunition_type: String::from("battery"),
                capacity: 85,
                rigid: true,
                reloadable: true,
                unloadable: true,
            }];
        let mut battery = tablet.prototype.clone();
        battery.type_id = String::from("battery");
        battery.charges = 1;
        battery.integral_magazines.clear();
        battery.containment.count_by_charges = true;
        battery.containment.stack_size = 1;
        battery.ammunition_type = String::from("battery");
        tablet.tool_charge_storage = Some(ItemGroupToolChargeStorageV1::Integral {
            ammunition: battery,
        });
        tablet.charge_capacity = ItemGroupChargeCapacityV1::AmmunitionStorage;
        tablet.charges = Some(ItemGroupChargeRangeV1 {
            minimum: 0,
            maximum: -1,
        });
        tablet.minimum_one_charge = false;

        let source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Distribution,
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
                    modifier_charges: None,
                    contents: Vec::new(),
                    seal_contents: false,
                    modifier_default_container_sealed: Some(true),
                    direct_wrapper: None,
                    modifier_container: None,
                    target: ItemGroupTargetV1::Item(Box::new(tablet)),
                }],
            }],
            wrapper: None,
        });
        let mut rng = ChaCha8Rng::seed_from_u64(31_415);
        let planned = plan_item_group_source(&source, &BTreeMap::new(), &mut rng)
            .expect("integral tool sentinel should plan");
        let [tablet] = planned.as_slice() else {
            panic!("one tablet should be generated")
        };
        assert_eq!(tablet.prototype.charges, 0);
        let charges = tablet
            .integral_ammunition
            .get(&0)
            .map_or(0, |ammunition| ammunition.prototype.charges);
        assert!((0..=85).contains(&charges));
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

        let mut wrapped_source = source.clone();
        let ItemGroupSourceV1::Inline(graph) = &mut wrapped_source else {
            unreachable!("fixture is inline")
        };
        let mut small_can = leaf_item("small_can");
        small_can.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            true,
            100,
            100,
            Vec::new(),
            false,
        )];
        graph.nodes[0].entries[0].modifier_container = Some(ItemGroupContainerV1 {
            item: Box::new(small_can),
            variant_id: None,
            sealed: false,
            overflow: ItemGroupOverflowV1::None,
        });
        let wrapped = plan_item_group_source(
            &wrapped_source,
            &catalog,
            &mut ChaCha8Rng::seed_from_u64(73),
        )
        .expect("one named-group modifier container should wrap every completed child");
        assert_eq!(wrapped.len(), 2);
        assert!(wrapped.iter().all(|container| {
            container.prototype.type_id == "small_can"
                && container
                    .pocket_contents
                    .get(&0)
                    .is_some_and(|contents| contents.len() == 1)
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
                    modifier_charges: Some(ItemGroupChargeRangeV1 {
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
    fn multi_pocket_wrappers_select_the_first_compatible_unoccupied_slot() {
        let single_item_pocket = |index: u16, accepted_flag: &str| {
            let mut pocket = spawn_pocket(
                SpawnPocketKindV1::Container,
                false,
                100,
                350,
                vec![String::from(cdda_protocol::SPAWN_POCKET_SINGLE_ITEM_MARKER)],
                false,
            );
            pocket.pocket_index = index;
            pocket.pocket_id = format!("SLOT_{index}");
            pocket
                .spawn_rules
                .as_mut()
                .expect("spawn rules exist")
                .flag_restrictions = vec![accepted_flag.to_owned()];
            pocket
        };
        let wrapper = |type_id: &str, pockets| {
            let mut item = leaf_item(type_id);
            item.prototype.ammunition_containers = pockets;
            ItemGroupContainerV1 {
                item: Box::new(item),
                variant_id: None,
                sealed: false,
                overflow: ItemGroupOverflowV1::None,
            }
        };

        let mut knife = leaf_item("throwing_knife");
        knife.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 200_000,
            volume_milliliters: 56,
            longest_side_millimeters: 350,
            flags: vec![String::from("SHEATH_KNIFE")],
            ..ItemContainmentProfileV1::default()
        };
        let sheath = wrapper(
            "leg_sheath",
            (0..3)
                .map(|index| single_item_pocket(index, "SHEATH_KNIFE"))
                .collect(),
        );
        assert_eq!(
            item_group_multi_pocket_projection(&knife, sheath, 3)
                .expect("three knives should spread across declared holster pockets")
                .pocket_contents,
            [
                (0, vec![String::from("throwing_knife")]),
                (1, vec![String::from("throwing_knife")]),
                (2, vec![String::from("throwing_knife")]),
            ]
        );

        let mut guard = leaf_item("plastic_mandible_guard");
        guard.prototype.containment = ItemContainmentProfileV1 {
            weight_milligrams: 250_000,
            volume_milliliters: 200,
            longest_side_millimeters: 100,
            flags: vec![String::from("HELMET_MANDIBLE_GUARD_STRAPPED")],
            ..ItemContainmentProfileV1::default()
        };
        let hard_hat = wrapper(
            "hat_hard",
            [
                "HELMET_FACE_SHIELD",
                "HELMET_EAR_ATTACHMENT",
                "HELMET_NAPE_PROTECTOR",
                "HELMET_MANDIBLE_GUARD_STRAPPED",
            ]
            .into_iter()
            .enumerate()
            .map(|(index, flag)| {
                let mut pocket = single_item_pocket(index as u16, flag);
                let rules = pocket.spawn_rules.as_mut().expect("spawn rules exist");
                rules.max_contains_volume_milliliters = 500;
                rules.max_item_volume_milliliters = 500;
                pocket
            })
            .collect(),
        );
        assert_eq!(
            item_group_multi_pocket_projection(&guard, hard_hat, 1)
                .expect("the guard should skip three incompatible hard-hat pockets")
                .pocket_contents,
            [
                (0, Vec::new()),
                (1, Vec::new()),
                (2, Vec::new()),
                (3, vec![String::from("plastic_mandible_guard")]),
            ]
        );
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
                        contents: vec![
                            ItemGroupContentsSourceV1::Item(Box::new(contents.clone())),
                            ItemGroupContentsSourceV1::Group(
                                encode_item_group_dressing_marker(100, 100)
                                    .expect("policy should encode"),
                            ),
                        ],
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
            "a reserved dressing marker must not turn one content source into a collection"
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
    fn wrapper_boundaries_preserve_overflow_and_flexible_containment() {
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
        let flexible_source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    target: ItemGroupTargetV1::Item(Box::new(payload("payload", 10))),
                    ..entry(100, None, "unused")
                }],
            }],
            wrapper: Some(ItemGroupContainerV1 {
                item: Box::new(non_rigid),
                variant_id: None,
                sealed: false,
                overflow: ItemGroupOverflowV1::None,
            }),
        });
        let mut flexible_rng = ChaCha8Rng::seed_from_u64(1);
        let [flexible] =
            plan_item_group_source(&flexible_source, &BTreeMap::new(), &mut flexible_rng)
                .expect("the generalized flexible wrapper should plan")
                .try_into()
                .expect("one wrapper should remain top-level");
        assert_eq!(flexible.prototype.type_id, "bag");
        assert_eq!(flexible.pocket_contents[&0][0].prototype.type_id, "payload");
        assert_eq!(flexible.total_volume_milliliters(), Some(1));
        assert_eq!(flexible.collapsed_pockets, BTreeSet::from([0]));

        let mut compressed = leaf_item("compressed_bag");
        compressed.prototype.containment.weight_milligrams = 10;
        compressed.prototype.containment.volume_milliliters = 100;
        compressed.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            false,
            100,
            100,
            Vec::new(),
            false,
        )];
        compressed.initial_variables.insert(
            cdda_protocol::item_pocket_weight_multiplier_variable_key(0),
            ItemVariableValueV1::Integer(i64::from(0.5_f32.to_bits())),
        );
        compressed.initial_variables.insert(
            cdda_protocol::item_pocket_volume_multiplier_variable_key(0),
            ItemVariableValueV1::Integer(i64::from(0.25_f32.to_bits())),
        );
        let mut compressed_payload = payload("compressed_payload", 10);
        compressed_payload.prototype.containment.weight_milligrams = 20;
        compressed_payload.prototype.containment.volume_milliliters = 40;
        let compressed_source = ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
            root_node: 0,
            nodes: vec![ItemGroupNodeV1 {
                node_id: 0,
                kind: ItemGroupKindV1::Collection,
                entries: vec![ItemGroupEntryV1 {
                    target: ItemGroupTargetV1::Item(Box::new(compressed_payload)),
                    ..entry(100, None, "unused")
                }],
            }],
            wrapper: Some(ItemGroupContainerV1 {
                item: Box::new(compressed),
                variant_id: None,
                sealed: false,
                overflow: ItemGroupOverflowV1::None,
            }),
        });
        let [compressed] = plan_item_group_source(
            &compressed_source,
            &BTreeMap::new(),
            &mut ChaCha8Rng::seed_from_u64(1),
        )
        .expect("multiplier-backed flexible wrapper should plan")
        .try_into()
        .expect("one compressed wrapper");
        assert_eq!(compressed.total_weight_milligrams(), Some(20));
        assert_eq!(compressed.total_volume_milliliters(), Some(110));
    }

    #[test]
    fn static_corpse_wrappers_force_contents_but_require_final_maximum_damage() {
        let mut corpse = leaf_item("corpse_child_calm");
        corpse.maximum_raw_damage = MAX_ITEM_RAW_DAMAGE;
        corpse
            .prototype
            .containment
            .flags
            .push(String::from("CORPSE"));
        corpse.initial_variables.insert(
            ITEM_GROUP_CORPSE_SOURCE_MONSTER_VARIABLE.to_owned(),
            ItemVariableValueV1::String(String::from("mon_child")),
        );
        corpse.prototype.ammunition_containers = vec![spawn_pocket(
            SpawnPocketKindV1::Container,
            false,
            1,
            1,
            Vec::new(),
            false,
        )];
        let mut payload = leaf_item("oversized_loot");
        payload.prototype.containment.volume_milliliters = 2;
        payload.prototype.containment.longest_side_millimeters = 1;
        let inner = ItemGroupDefinitionV1 {
            group_id: String::from("static_corpse_inner"),
            graph: ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![ItemGroupEntryV1 {
                        target: ItemGroupTargetV1::Item(Box::new(payload)),
                        ..entry(100, None, "unused")
                    }],
                }],
                wrapper: Some(ItemGroupContainerV1 {
                    item: Box::new(corpse),
                    variant_id: None,
                    sealed: false,
                    overflow: ItemGroupOverflowV1::None,
                }),
            },
        };
        let catalog = BTreeMap::from([(inner.group_id.clone(), inner.clone())]);
        let source = |damage| {
            ItemGroupSourceV1::Inline(ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![ItemGroupNodeV1 {
                    node_id: 0,
                    kind: ItemGroupKindV1::Collection,
                    entries: vec![ItemGroupEntryV1 {
                        raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                            minimum: damage,
                            maximum: damage,
                        }),
                        target: ItemGroupTargetV1::Group(inner.group_id.clone()),
                        ..entry(100, None, "unused")
                    }],
                }],
                wrapper: None,
            })
        };

        let planned = plan_item_group_source(
            &source(MAX_ITEM_RAW_DAMAGE),
            &catalog,
            &mut ChaCha8Rng::seed_from_u64(11),
        )
        .expect("maximum-damaged static corpse should be non-reviving and retain forced contents");
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].raw_damage, MAX_ITEM_RAW_DAMAGE);
        assert_eq!(
            planned[0].pocket_contents[&0][0].prototype.type_id,
            "oversized_loot"
        );
        assert!(matches!(
            plan_item_group_source(
                &source(MAX_ITEM_RAW_DAMAGE - 1),
                &catalog,
                &mut ChaCha8Rng::seed_from_u64(11),
            ),
            Err(SimError::InvalidItem)
        ));
        assert!(matches!(
            plan_item_group_source(
                &ItemGroupSourceV1::Group(inner.group_id),
                &catalog,
                &mut ChaCha8Rng::seed_from_u64(11),
            ),
            Err(SimError::InvalidItem)
        ));
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
