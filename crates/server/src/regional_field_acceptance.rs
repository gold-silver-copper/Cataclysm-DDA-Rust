use cdda_conformance::{
    ScenarioActorSpawnV1, ScenarioActorV1, ScenarioCommandV1, ScenarioExpectationV1,
    ScenarioGeneratedItemSelectorV1, ScenarioGeneratedItemV1, ScenarioMode, ScenarioStepV1,
    ScenarioV1, run_scenario,
};
use cdda_content::{
    DefaultRegionTerrainFurnitureRegistry, FurnitureRegistry, MapgenRegistry,
    OvermapTerrainRegistry, StartLocationRegistry, StrictItemGroupGraph, TerrainRegistry,
};
use cdda_protocol::SimTick;
use cdda_sim::WorldState;

use super::item_groups::{RuntimeItemGroupContent, runtime_named_item_group_catalog};
use super::worldgen::{
    RuntimeMapgenContent, bootstrap_regional_field_overmap, runtime_mapgen_worldgen,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn assert_production_regional_field_gameplay(
    field_graph: &StrictItemGroupGraph,
    item_group_content: RuntimeItemGroupContent<'_>,
    overmap_terrain: &OvermapTerrainRegistry,
    start_locations: &StartLocationRegistry,
    mapgen: &MapgenRegistry,
    regions: &DefaultRegionTerrainFurnitureRegistry,
    terrain: &TerrainRegistry,
    furniture: &FurnitureRegistry,
) {
    let production_field_catalog =
        runtime_named_item_group_catalog(field_graph, item_group_content)
            .expect("production field catalog should normalize");
    let production_field_worldgen = runtime_mapgen_worldgen(
        bootstrap_regional_field_overmap(overmap_terrain)
            .expect("regional field overmap should normalize"),
        start_locations
            .get("sloc_field")
            .expect("field start should exist"),
        RuntimeMapgenContent {
            mapgen,
            regions,
            terrain,
            furniture,
            item_groups: &production_field_catalog,
        },
    )
    .expect("production regional field should normalize");
    assert_eq!(
        production_field_worldgen.overmap.identities[0].full_id,
        "field"
    );
    assert_eq!(
        production_field_worldgen
            .start_location
            .as_ref()
            .expect("field start should remain explicit")
            .start_location_id,
        "sloc_field"
    );

    let mut production_field = WorldState::new(909, [31; 32]);
    production_field
        .install_reserved_block(
            cdda_sim::ReservedIdBlock::new(1, cdda_sim::ID_RESERVATION_SIZE)
                .expect("field ID block should be valid"),
        )
        .expect("field IDs should reserve");
    production_field
        .register_item_group_catalog(production_field_catalog.clone())
        .expect("field item groups should register");
    production_field
        .configure_worldgen(production_field_worldgen.clone())
        .expect("field worldgen should configure");
    production_field
        .generate_initial_bubble(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 })
        .expect("field bubble should generate");
    let first_field_actor = production_field
        .spawn_actor_first_available(true)
        .expect("first field actor should spawn");
    let second_field_actor = production_field
        .spawn_actor_first_available(true)
        .expect("second field actor should spawn");
    let production_field_snapshot = production_field.snapshot();
    fn collect_item_types(
        item: &cdda_protocol::ItemSnapshot,
        item_types: &mut std::collections::BTreeSet<String>,
    ) {
        item_types.insert(item.type_id.clone());
        for ammunition in item
            .integral_magazines
            .iter()
            .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
        {
            collect_item_types(ammunition, item_types);
        }
        for magazine in item
            .magazine_wells
            .iter()
            .filter_map(|pocket| pocket.installed_magazine.as_deref())
        {
            collect_item_types(magazine, item_types);
        }
        for content in item
            .ammunition_containers
            .iter()
            .flat_map(|pocket| &pocket.contents)
        {
            collect_item_types(content, item_types);
        }
    }
    let mut production_field_item_types = std::collections::BTreeSet::new();
    for ground in &production_field_snapshot.ground_items {
        collect_item_types(&ground.item, &mut production_field_item_types);
    }
    assert_eq!(
        production_field_item_types.len(),
        45,
        "production field reached {production_field_item_types:?}"
    );
    let nested_ground_items = production_field_snapshot
        .ground_items
        .iter()
        .filter(|ground| {
            ground
                .item
                .ammunition_containers
                .iter()
                .any(|pocket| !pocket.contents.is_empty())
        })
        .map(|ground| ground.item.type_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(production_field_snapshot.ground_items.len(), 59);
    assert_eq!(nested_ground_items, ["corpse_generic_male"]);
    assert_eq!(
        second_field_actor.counter(),
        first_field_actor.counter() + 1,
        "the two production-field actors should receive consecutive stable IDs"
    );
    assert_eq!(
        second_field_actor.world_namespace(),
        first_field_actor.world_namespace()
    );

    let production_field_scenario = ScenarioV1 {
        format_version: cdda_conformance::SCENARIO_FORMAT_VERSION,
        protocol_version: cdda_protocol::PROTOCOL_VERSION,
        persistence_schema_version: cdda_persistence::SCHEMA_VERSION,
        replay_format_version: cdda_persistence::REPLAY_FORMAT_VERSION,
        baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
        world_namespace: 910,
        world_seed: [31; 32],
        content_manifest_hash: [33; 32],
        enabled_mods: vec![String::from("dda")],
        item_groups: production_field_catalog,
        terrain_bash_types: Vec::new(),
        smash_item_types: Vec::new(),
        worldgen: Some(production_field_worldgen),
        chunks: Vec::new(),
        terrain: Vec::new(),
        actors: vec![
            ScenarioActorV1 {
                alias: String::from("alpha"),
                spawn: ScenarioActorSpawnV1::AtGeneratedGroundItem {
                    item: String::from("field_corpse"),
                },
                connected: true,
                stats: cdda_protocol::CharacterCreationStatsV1::default(),
            },
            ScenarioActorV1 {
                alias: String::from("beta"),
                spawn: ScenarioActorSpawnV1::StartLocation,
                connected: true,
                stats: cdda_protocol::CharacterCreationStatsV1::default(),
            },
        ],
        ground_items: Vec::new(),
        generated_items: vec![
            ScenarioGeneratedItemV1 {
                alias: String::from("field_corpse"),
                selector: ScenarioGeneratedItemSelectorV1::Ground {
                    type_id: String::from("corpse_generic_male"),
                    ordinal: 0,
                },
            },
            ScenarioGeneratedItemV1 {
                alias: String::from("nested_loot"),
                selector: ScenarioGeneratedItemSelectorV1::Pocket {
                    owner: String::from("field_corpse"),
                    pocket_index: 0,
                    ordinal: 0,
                },
            },
        ],
        steps: vec![
            ScenarioStepV1::Command {
                actor: String::from("alpha"),
                command: ScenarioCommandV1::PickUp {
                    item: String::from("field_corpse"),
                },
            },
            ScenarioStepV1::AdvanceBatch { ticks: 25 },
            ScenarioStepV1::Command {
                actor: String::from("alpha"),
                command: ScenarioCommandV1::RemovePocketItem {
                    owner_item: String::from("field_corpse"),
                    pocket_index: 0,
                    contained_item: String::from("nested_loot"),
                },
            },
            ScenarioStepV1::Command {
                actor: String::from("beta"),
                command: ScenarioCommandV1::Move { dx: 0, dy: 1 },
            },
            ScenarioStepV1::AdvanceBatch { ticks: 105 },
            ScenarioStepV1::Connection {
                actor: String::from("alpha"),
                connected: false,
            },
            ScenarioStepV1::Connection {
                actor: String::from("beta"),
                connected: false,
            },
            ScenarioStepV1::Advance { ticks: 3 },
        ],
        expected: ScenarioExpectationV1 {
            final_tick: SimTick(0),
            final_state_hash: [0; 32],
            event_trace_hash: [0; 32],
            actors: Vec::new(),
            ground_items: Vec::new(),
            event_batches: None,
        },
    };
    let direct_field = run_scenario(&production_field_scenario, ScenarioMode::Direct)
        .expect("production field gameplay should run directly");
    assert_eq!(
        (direct_field.final_state_hash, direct_field.event_trace_hash),
        (
            [
                0x1a, 0xc8, 0x03, 0xcf, 0x46, 0x56, 0x90, 0x81, 0x81, 0x76, 0x39, 0xf9, 0x39, 0xac,
                0xaa, 0x18, 0x0e, 0x66, 0x28, 0x48, 0x7f, 0x00, 0xe8, 0x92, 0xa2, 0x18, 0x34, 0x39,
                0xab, 0xa2, 0x1e, 0x97,
            ],
            [
                0x40, 0xb0, 0x5c, 0x27, 0x8a, 0x6a, 0x6a, 0xf9, 0x05, 0x5e, 0x6d, 0xd9, 0xa3, 0xbc,
                0x0a, 0xcf, 0x4e, 0x92, 0x0c, 0x9f, 0x69, 0x0f, 0x7e, 0x0b, 0x1e, 0xea, 0x9a, 0x67,
                0x87, 0x26, 0xed, 0xa7,
            ],
        )
    );
    assert_eq!(direct_field.final_snapshot.chunks.len(), 144);
    assert_eq!(direct_field.final_snapshot.actors.len(), 2);
    assert!(
        direct_field
            .final_snapshot
            .actors
            .iter()
            .all(|actor| !actor.connected)
    );
    let field_event_kinds = direct_field
        .event_batches
        .iter()
        .flat_map(|batch| &batch.events)
        .map(|event| &event.kind)
        .collect::<Vec<_>>();
    assert!(
        field_event_kinds.iter().any(|kind| matches!(
            kind,
            cdda_protocol::WorldEventKind::PocketItemRemoved { .. }
        )),
        "production nested-loot command trace was {field_event_kinds:?}"
    );
    assert!(
        field_event_kinds
            .iter()
            .any(|kind| matches!(kind, cdda_protocol::WorldEventKind::ActorMoved { .. })),
        "production exploration command trace was {field_event_kinds:?}"
    );
    for mode in [
        ScenarioMode::SnapshotEachTick,
        ScenarioMode::SqliteRecovery,
        ScenarioMode::PortableReplay,
    ] {
        assert_eq!(
            run_scenario(&production_field_scenario, mode)
                .expect("production field gameplay should recover"),
            direct_field,
            "{mode} must preserve the complete production field gameplay trace"
        );
    }
}
