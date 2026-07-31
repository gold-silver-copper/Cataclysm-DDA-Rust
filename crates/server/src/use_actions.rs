use cdda_content::ItemRegistry;
use cdda_protocol::ItemTransformTypeV1;

use crate::item_groups::{RuntimeItemGroupContent, runtime_item_group_item};

pub(super) fn runtime_item_transform_types(
    items: &ItemRegistry,
    content: RuntimeItemGroupContent<'_>,
) -> Vec<ItemTransformTypeV1> {
    items
        .iter()
        .filter_map(|(_id, source)| {
            let [action] = source.transform_actions.as_slice() else {
                return None;
            };
            if source.has_non_transform_use_actions
                || source.has_unsupported_use_actions
                || source.has_unsupported_transform_action_fields
                || !source.healing_actions.is_empty()
                || !source.eoc_actions.is_empty()
                || source.subtypes.contains("ARMOR")
            {
                return None;
            }
            let target = items.get(&action.target)?;
            if source.subtypes != target.subtypes
                || !source.revert_to.is_empty()
                || !target.revert_to.is_empty()
                || source.power_draw_milliwatts != 0
                || target.power_draw_milliwatts != 0
                || source.light_emission != 0
                || target.light_emission != 0
            {
                return None;
            }
            let source_runtime = runtime_item_group_item(source, None, content).ok()?;
            let mut target_runtime = runtime_item_group_item(target, None, content).ok()?;
            if !transform_layouts_are_compatible(
                &source_runtime.prototype,
                &target_runtime.prototype,
            ) {
                return None;
            }
            let target_prototype = target_runtime.prototype.clone();
            target_runtime.prototype = source_runtime.prototype.clone();
            if source_runtime != target_runtime {
                return None;
            }
            let consumed_charges = if source.subtypes.contains("TOOL") {
                source.charges_per_use.checked_mul(action.ammo_scale)?
            } else {
                0
            };
            Some(ItemTransformTypeV1 {
                source_type_id: source.id.clone(),
                target: Box::new(target_prototype),
                required_charges: u32::try_from(action.need_charges).ok()?,
                consumed_charges: u32::try_from(consumed_charges).ok()?,
                move_cost_moves: u32::try_from(action.moves).ok()?,
            })
        })
        .collect()
}

fn transform_layouts_are_compatible(
    source: &cdda_protocol::CraftItemPrototypeV1,
    target: &cdda_protocol::CraftItemPrototypeV1,
) -> bool {
    let has_fit_capability = |prototype: &cdda_protocol::CraftItemPrototypeV1, flag: &str| {
        prototype
            .containment
            .flags
            .binary_search_by(|candidate| candidate.as_str().cmp(flag))
            .is_ok()
    };
    source.containment.count_by_charges == target.containment.count_by_charges
        && source.containment.phase == target.containment.phase
        && ["FIT", "VARSIZE"]
            .into_iter()
            .all(|flag| has_fit_capability(source, flag) == has_fit_capability(target, flag))
        && source.tracks_temperature == target.tracks_temperature
        && source.thermal_properties == target.thermal_properties
        && source.ammunition_type == target.ammunition_type
        && source.magazine_capacity == target.magazine_capacity
        && source.integral_magazines == target.integral_magazines
        && source.magazine_wells == target.magazine_wells
        && source.ammunition_containers == target.ammunition_containers
        && source.residual_energy_millijoules == target.residual_energy_millijoules
        && source.powered_tool.is_none()
        && target.powered_tool.is_none()
}
