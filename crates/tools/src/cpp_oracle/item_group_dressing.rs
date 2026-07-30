use super::*;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemGroupDressingObservationV1 {
    direct: Vec<ItemGroupDressingTraceV1>,
    production: Vec<ItemGroupDressingTraceV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemGroupDressingTraceV1 {
    case_id: String,
    source_group: String,
    seed: u32,
    ammunition_chance: u8,
    magazine_chance: u8,
    charges_minimum: i32,
    charges_maximum: i32,
    item_type: String,
    magazine_present: bool,
    magazine_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
    remaining_capacity: i32,
    downstream_draw: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(super) struct ItemGroupDressingDirectV1 {
    case_id: String,
    ammunition_chance: u8,
    magazine_chance: u8,
    charges_minimum: i32,
    charges_maximum: i32,
    item_type: String,
    magazine_present: bool,
    magazine_type: String,
    ammunition_type: String,
    ammunition_remaining: i32,
    remaining_capacity: i32,
}

impl ItemGroupDressingObservationV1 {
    pub(super) fn direct_projection(&self) -> Vec<ItemGroupDressingDirectV1> {
        self.direct
            .iter()
            .map(|trace| ItemGroupDressingDirectV1 {
                case_id: trace.case_id.clone(),
                ammunition_chance: trace.ammunition_chance,
                magazine_chance: trace.magazine_chance,
                charges_minimum: trace.charges_minimum,
                charges_maximum: trace.charges_maximum,
                item_type: trace.item_type.clone(),
                magazine_present: trace.magazine_present,
                magazine_type: trace.magazine_type.clone(),
                ammunition_type: trace.ammunition_type.clone(),
                ammunition_remaining: trace.ammunition_remaining,
                remaining_capacity: trace.remaining_capacity,
            })
            .collect()
    }
}

pub(super) fn validate(
    observation: &ItemGroupDressingObservationV1,
) -> Result<(), Box<dyn std::error::Error>> {
    let expected_direct = [
        (
            "integral_full",
            100,
            100,
            -1,
            -1,
            "matches",
            false,
            "",
            "match",
            20,
            0,
        ),
        (
            "integral_chance_failure",
            50,
            100,
            -1,
            -1,
            "matches",
            false,
            "",
            "null",
            0,
            20,
        ),
        (
            "integral_chance_success",
            50,
            100,
            -1,
            -1,
            "matches",
            false,
            "",
            "match",
            20,
            0,
        ),
        (
            "detachable_ammunition",
            100,
            0,
            -1,
            -1,
            "wearable_light",
            false,
            "",
            "null",
            0,
            0,
        ),
        (
            "detachable_magazine",
            0,
            100,
            -1,
            -1,
            "wearable_light",
            true,
            "medium_battery_cell",
            "null",
            0,
            0,
        ),
        (
            "explicit_charge_suppression",
            100,
            100,
            0,
            0,
            "permanent_marker",
            false,
            "",
            "null",
            0,
            500,
        ),
    ];
    if observation.direct.len() != expected_direct.len()
        || observation
            .direct
            .iter()
            .zip(expected_direct)
            .any(|(trace, expected)| {
                !trace.source_group.is_empty()
                    || trace.seed == 0
                    || !(0..=9_999).contains(&trace.downstream_draw)
                    || trace.case_id != expected.0
                    || trace.ammunition_chance != expected.1
                    || trace.magazine_chance != expected.2
                    || trace.charges_minimum != expected.3
                    || trace.charges_maximum != expected.4
                    || trace.item_type != expected.5
                    || trace.magazine_present != expected.6
                    || trace.magazine_type != expected.7
                    || trace.ammunition_type != expected.8
                    || trace.ammunition_remaining != expected.9
                    || trace.remaining_capacity != expected.10
            })
    {
        return Err("item-group direct dressing traces are incomplete".into());
    }
    let expected_production = [
        (
            "lighter_matches_empty",
            "everyday_lighter",
            "matches",
            0,
            20,
            0,
        ),
        (
            "lighter_matches_full",
            "everyday_lighter",
            "matches",
            0,
            20,
            20,
        ),
        (
            "lighter_ref_matches_empty",
            "everyday_lighter",
            "ref_matches",
            0,
            32,
            0,
        ),
        (
            "lighter_ref_matches_full",
            "everyday_lighter",
            "ref_matches",
            0,
            32,
            32,
        ),
        (
            "gear_marker_empty",
            "everyday_gear",
            "permanent_marker",
            0,
            -1,
            0,
        ),
        (
            "gear_marker_full",
            "everyday_gear",
            "permanent_marker",
            0,
            -1,
            500,
        ),
        (
            "gear_inhaler_minimum",
            "everyday_gear",
            "inhaler",
            10,
            100,
            10,
        ),
        (
            "gear_inhaler_maximum",
            "everyday_gear",
            "inhaler",
            10,
            100,
            100,
        ),
        (
            "gear_flashlight_empty",
            "everyday_gear",
            "flashlight",
            0,
            300,
            0,
        ),
        (
            "gear_flashlight_full",
            "everyday_gear",
            "flashlight",
            0,
            300,
            56,
        ),
    ];
    if observation.production.len() != expected_production.len()
        || observation
            .production
            .iter()
            .zip(expected_production)
            .any(|(trace, expected)| {
                trace.seed == 0
                    || !(0..=9_999).contains(&trace.downstream_draw)
                    || trace.case_id != expected.0
                    || trace.source_group != expected.1
                    || trace.item_type != expected.2
                    || trace.charges_minimum != expected.3
                    || trace.charges_maximum != expected.4
                    || trace.ammunition_remaining != expected.5
                    || (trace.source_group == "everyday_gear"
                        && (trace.ammunition_chance != 75 || trace.magazine_chance != 100))
                    || (trace.source_group == "everyday_lighter"
                        && (trace.ammunition_chance != 0 || trace.magazine_chance != 0))
            })
    {
        return Err("item-group production dressing traces are incomplete".into());
    }
    Ok(())
}

pub(super) fn rust_observation()
-> Result<Vec<ItemGroupDressingDirectV1>, Box<dyn std::error::Error>> {
    let plain = |type_id: &str| CraftItemPrototypeV1 {
        type_id: type_id.to_owned(),
        charges: 0,
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
        containment: ItemContainmentProfileV1::default(),
    };
    let ammunition = |type_id: &str, ammunition_type: &str| {
        let mut item = plain(type_id);
        item.charges = 1;
        item.ammunition_type = ammunition_type.to_owned();
        item.containment = ItemContainmentProfileV1 {
            count_by_charges: true,
            stack_size: 100,
            ..ItemContainmentProfileV1::default()
        };
        item
    };
    let integral = |type_id: &str, ammunition_type: &str, capacity: u32, ammo_id: &str| {
        let mut owner = plain(type_id);
        owner.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
            pocket_index: 0,
            pocket_id: String::from("MAGAZINE"),
            ammunition_type: ammunition_type.to_owned(),
            capacity,
            rigid: true,
            reloadable: false,
            unloadable: false,
        }];
        ItemGroupItemPrototypeV1 {
            prototype: owner,
            maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
            variants: Vec::new(),
            description_expansion: None,
            snippets: Vec::new(),
            initial_variables: BTreeMap::new(),
            default_container: None,
            modifier_side_effects_supported: true,
            charges: None,
            minimum_one_charge: false,
            tool_charge_storage: Some(ItemGroupToolChargeStorageV1::Integral {
                ammunition: ammunition(ammo_id, ammunition_type),
            }),
            charges_supported: true,
            charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
            contents_insertion_supported: true,
        }
    };
    let matches = integral("matches", "match", 20, "match");
    let permanent_marker = integral("permanent_marker", "permanent_ink", 500, "permanent_ink");

    let mut magazine = plain("medium_battery_cell");
    magazine.integral_magazines = vec![IntegralMagazinePocketPrototypeV1 {
        pocket_index: 0,
        pocket_id: String::from("MAGAZINE"),
        ammunition_type: String::from("battery"),
        capacity: 56,
        rigid: true,
        reloadable: false,
        unloadable: false,
    }];
    let mut tool = plain("wearable_light");
    tool.magazine_wells = vec![MagazineWellPrototypeV1 {
        pocket_index: 0,
        pocket_id: String::from("MAGAZINE_WELL"),
        compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
        rigid: true,
        unloadable: true,
    }];
    let wearable_light = ItemGroupItemPrototypeV1 {
        prototype: tool,
        maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
        variants: Vec::new(),
        description_expansion: None,
        snippets: Vec::new(),
        initial_variables: BTreeMap::new(),
        default_container: None,
        modifier_side_effects_supported: true,
        charges: None,
        minimum_one_charge: false,
        tool_charge_storage: Some(ItemGroupToolChargeStorageV1::Detachable {
            well_pocket_index: 0,
            magazine,
            ammunition: Box::new(ammunition("battery", "battery")),
        }),
        charges_supported: true,
        charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage,
        contents_insertion_supported: true,
    };

    [
        ("integral_full", &matches, 100, 100, None, 20),
        ("integral_chance_failure", &matches, 50, 100, None, 0),
        ("integral_chance_success", &matches, 50, 100, None, 20),
        ("detachable_ammunition", &wearable_light, 100, 0, None, 0),
        ("detachable_magazine", &wearable_light, 0, 100, None, 0),
        (
            "explicit_charge_suppression",
            &permanent_marker,
            100,
            100,
            Some(cdda_protocol::ItemGroupChargeRangeV1 {
                minimum: 0,
                maximum: 0,
            }),
            0,
        ),
    ]
    .into_iter()
    .map(
        |(case_id, item, ammunition_chance, magazine_chance, charges, expected_remaining)| {
            let mut last_error = None;
            let projection = (1_u64..=100)
                .find_map(|seed| {
                    match cdda_sim::item_group_dressing_projection(
                        item,
                        ammunition_chance,
                        magazine_chance,
                        charges,
                        seed,
                    ) {
                        Ok(projection)
                            if projection.ammunition_remaining == expected_remaining =>
                        {
                            Some(projection)
                        }
                        Ok(_) => None,
                        Err(error) => {
                            last_error = Some(error);
                            None
                        }
                    }
                })
                .ok_or_else(|| {
                    format!(
                        "Rust dressing case {case_id} found no bounded outcome; last error: {last_error:?}"
                    )
                })?;
            let (charges_minimum, charges_maximum) =
                charges.map_or((-1, -1), |charges| (charges.minimum, charges.maximum));
            Ok(ItemGroupDressingDirectV1 {
                case_id: case_id.to_owned(),
                ammunition_chance,
                magazine_chance,
                charges_minimum,
                charges_maximum,
                item_type: projection.item_type,
                magazine_present: projection.magazine_present,
                magazine_type: projection.magazine_type.unwrap_or_default(),
                ammunition_type: projection
                    .ammunition_type
                    .unwrap_or_else(|| String::from("null")),
                ammunition_remaining: projection.ammunition_remaining,
                remaining_capacity: i32::try_from(projection.remaining_capacity)?,
            })
        },
    )
    .collect()
}
