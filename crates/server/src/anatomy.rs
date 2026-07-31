use std::collections::{BTreeMap, BTreeSet};

use cdda_content::{AnatomyRegistry, BodyPartDefinition, ItemRegistry, MaterialRegistry};
use cdda_protocol::{
    AnatomyDefinitionV1, ArmorMaterialProtectionV1, BodyPartHpModifiersV1, BodyPartOnHitEffectV1,
    BodyPartPrototypeV1, WearableArmorPortionV1, WearableArmorTypeV1, anatomy_definition_is_valid,
    wearable_armor_catalog_is_valid,
};

pub(super) fn runtime_actor_anatomy(
    registry: &AnatomyRegistry,
) -> Result<AnatomyDefinitionV1, Box<dyn std::error::Error>> {
    let anatomy = registry
        .anatomy("human_anatomy")
        .ok_or("pinned content is missing human_anatomy")?;
    if !anatomy.deferred_fields.is_empty() {
        return Err(format!(
            "human_anatomy has unsupported structural fields: {:?}",
            anatomy.deferred_fields
        )
        .into());
    }
    let runtime = AnatomyDefinitionV1 {
        anatomy_id: anatomy.id.clone(),
        parts: anatomy
            .parts
            .iter()
            .map(|id| {
                registry
                    .body_part(id)
                    .ok_or_else(|| format!("human_anatomy references missing body part {id}"))
                    .and_then(runtime_body_part)
            })
            .collect::<Result<Vec<_>, _>>()?,
        deferred_fields: anatomy.deferred_fields.iter().cloned().collect(),
    };
    if !anatomy_definition_is_valid(&runtime) {
        return Err("normalized human anatomy is invalid".into());
    }
    Ok(runtime)
}

pub(super) fn runtime_wearable_armor_types(
    items: &ItemRegistry,
    materials: &MaterialRegistry,
) -> Result<Vec<WearableArmorTypeV1>, Box<dyn std::error::Error>> {
    let catalog = items
        .iter()
        .map(|(_, definition)| definition)
        .filter(|definition| definition.subtypes.contains("ARMOR") && !definition.armor.is_empty())
        .map(|definition| {
            let mut deferred_fields = BTreeSet::new();
            if definition.unsupported_fields.contains("armor") {
                deferred_fields.insert(String::from("armor"));
            }
            Ok(WearableArmorTypeV1 {
                item_type_id: definition.id.clone(),
                portions: definition
                    .armor
                    .iter()
                    .map(|portion| {
                        let mut portion_deferred = portion.deferred_fields.clone();
                        let mut layers = portion
                            .materials
                            .iter()
                            .map(|layer| {
                                (
                                    layer.material_id.as_str(),
                                    layer.covered_by_material_percent,
                                    layer.thickness_micrometers,
                                )
                            })
                            .collect::<Vec<_>>();
                        if layers.is_empty() {
                            let total_portions = definition
                                .materials
                                .values()
                                .filter(|portion| **portion > 0)
                                .try_fold(0_u64, |total, portion| {
                                    total.checked_add(u64::try_from(*portion).ok()?)
                                });
                            let average_thickness = if portion.material_thickness_micrometers > 0 {
                                portion.material_thickness_micrometers
                            } else {
                                u32::try_from(definition.material_thickness_micrometers)
                                    .unwrap_or_default()
                            };
                            if let Some(total_portions) = total_portions.filter(|total| *total > 0)
                            {
                                layers = definition
                                    .materials
                                    .iter()
                                    .filter(|(_, material_portion)| **material_portion > 0)
                                    .map(|(material_id, material_portion)| {
                                        let thickness = u64::from(average_thickness)
                                            .saturating_mul(
                                                u64::try_from(*material_portion)
                                                    .unwrap_or_default(),
                                            )
                                            .checked_div(total_portions)
                                            .and_then(|value| u32::try_from(value).ok())
                                            .unwrap_or_default();
                                        (material_id.as_str(), 100, thickness)
                                    })
                                    .collect();
                            } else {
                                portion_deferred.insert(String::from("material"));
                            }
                        }
                        let mut runtime_materials = Vec::with_capacity(layers.len().max(1));
                        for (material_id, material_coverage, thickness_micrometers) in layers {
                            let material = materials.get(material_id).ok_or_else(|| {
                                format!(
                                    "armor {} references unknown material {}",
                                    definition.id, material_id
                                )
                            })?;
                            let mut protection = BTreeMap::<String, u32>::new();
                            for (damage_type, resistance_milli) in &material.damage_resistance_milli
                            {
                                let contribution = u128::try_from(*resistance_milli)
                                    .ok()
                                    .and_then(|resistance| {
                                        resistance.checked_mul(u128::from(thickness_micrometers))
                                    })
                                    .and_then(|value| value.checked_div(1_000))
                                    .and_then(|value| u32::try_from(value).ok())
                                    .ok_or("armor protection overflow")?;
                                protection.insert(damage_type.clone(), contribution);
                            }
                            runtime_materials.push(ArmorMaterialProtectionV1 {
                                covered_by_material_percent: material_coverage,
                                protection_milli: protection,
                            });
                        }
                        if runtime_materials.is_empty() {
                            runtime_materials.push(ArmorMaterialProtectionV1 {
                                covered_by_material_percent: 100,
                                protection_milli: BTreeMap::new(),
                            });
                        }
                        Ok(WearableArmorPortionV1 {
                            covers: portion.covers.iter().cloned().collect(),
                            coverage_percent: portion.coverage_percent,
                            encumbrance_minimum: portion.encumbrance_minimum,
                            encumbrance_maximum: portion.encumbrance_maximum,
                            materials: runtime_materials,
                            deferred_fields: portion_deferred.into_iter().collect(),
                        })
                    })
                    .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?,
                deferred_fields: deferred_fields.into_iter().collect(),
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    if !wearable_armor_catalog_is_valid(&catalog) {
        return Err("normalized wearable armor catalog is invalid".into());
    }
    Ok(catalog)
}

fn runtime_body_part(definition: &BodyPartDefinition) -> Result<BodyPartPrototypeV1, String> {
    Ok(BodyPartPrototypeV1 {
        body_part_id: definition.id.clone(),
        main_part_id: definition.main_part.clone(),
        connected_to_id: definition.connected_to.clone(),
        opposite_part_id: definition.opposite_part.clone(),
        vital: definition.vital,
        hit_size_millionths: definition.hit_size_millionths,
        hit_difficulty_millionths: definition.hit_difficulty_millionths,
        base_hp: definition.base_hp,
        hp_modifiers: BodyPartHpModifiersV1 {
            strength_millionths: definition.hp_modifiers.strength_millionths,
            dexterity_millionths: definition.hp_modifiers.dexterity_millionths,
            intelligence_millionths: definition.hp_modifiers.intelligence_millionths,
            perception_millionths: definition.hp_modifiers.perception_millionths,
            health_millionths: definition.hp_modifiers.health_millionths,
        },
        effects_on_hit: definition
            .effects_on_hit
            .iter()
            .map(|effect| BodyPartOnHitEffectV1 {
                effect_id: effect.effect_id.clone(),
                global: effect.global,
                damage_type_id: effect.damage_type_id.clone(),
                damage_threshold_millionths: effect.damage_threshold_millionths,
                scale_increment_millionths: effect.scale_increment_millionths,
                chance_percent: effect.chance_percent,
                chance_damage_scaling_millionths: effect.chance_damage_scaling_millionths,
                intensity: effect.intensity,
                intensity_damage_scaling_millionths: effect.intensity_damage_scaling_millionths,
                max_intensity: effect.max_intensity,
                duration_turns: effect.duration_turns,
                duration_damage_scaling_millionths: effect.duration_damage_scaling_millionths,
                max_duration_turns: effect.max_duration_turns,
                deferred_fields: effect.deferred_fields.iter().cloned().collect(),
            })
            .collect(),
        deferred_fields: definition.deferred_fields.iter().cloned().collect(),
    })
}
