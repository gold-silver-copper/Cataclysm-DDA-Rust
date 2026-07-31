use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use cdda_persistence::{
    JournalBatchV1, JournalTickV1, REPLAY_FORMAT_VERSION, ReplayBundleV1, SCHEMA_VERSION,
    StoreError, WorldStore,
};
use cdda_protocol::{
    ActorConnectionUpdateV1, ActorId, AmmunitionContainerPocketPrototypeV1,
    CharacterCreationStatsV1, ChunkCoord, ClientCommand, CommandKind, CommandSequence,
    ContentIdentity, IntegralMagazinePocketPrototypeV1, ItemGroupDefinitionV1, ItemId,
    ItemSnapshot, MagazineWellPrototypeV1, PoweredToolStateV1, RangedWeaponSnapshot, SimTick,
    SmashItemTypeV1, TerrainBashTypeV1, TerrainTileSnapshot, WorldEvent, WorldPosition,
    WorldSnapshotV1, WorldgenCatalogV1, worldgen_catalog_is_valid,
};
use cdda_sim::{
    Chunk, ID_RESERVATION_SIZE, ItemSpawn, ReservedIdBlock, SimError, WorldState,
    canonical_events_hash,
};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use cdda_protocol::WorldEventKind;

pub const SCENARIO_FORMAT_VERSION: u16 = 8;
pub const OBSERVATION_FORMAT_VERSION: u16 = 6;
const MAX_ALIASES: usize = 512;
const MAX_ALIAS_BYTES: usize = 64;
const MAX_CHUNKS: usize = 121;
const MAX_ACTORS: usize = 16;
const MAX_ITEMS: usize = 256;
const MAX_TERRAIN_FIXTURES: usize = 512;
const MAX_STEPS: usize = 4_096;
const MAX_TICKS: u64 = 100_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioV1 {
    pub format_version: u16,
    pub protocol_version: u16,
    pub persistence_schema_version: i64,
    pub replay_format_version: u16,
    pub baseline_commit: String,
    pub world_namespace: u64,
    pub world_seed: [u8; 32],
    pub content_manifest_hash: [u8; 32],
    pub enabled_mods: Vec<String>,
    pub item_groups: Vec<ItemGroupDefinitionV1>,
    pub terrain_bash_types: Vec<TerrainBashTypeV1>,
    pub smash_item_types: Vec<SmashItemTypeV1>,
    pub worldgen: Option<WorldgenCatalogV1>,
    pub chunks: Vec<ChunkCoord>,
    pub terrain: Vec<ScenarioTerrainV1>,
    pub actors: Vec<ScenarioActorV1>,
    pub ground_items: Vec<ScenarioItemV1>,
    pub generated_items: Vec<ScenarioGeneratedItemV1>,
    pub steps: Vec<ScenarioStepV1>,
    pub expected: ScenarioExpectationV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioTerrainV1 {
    pub position: WorldPosition,
    pub terrain: TerrainTileSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioActorV1 {
    pub alias: String,
    pub spawn: ScenarioActorSpawnV1,
    pub connected: bool,
    pub stats: CharacterCreationStatsV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ScenarioActorSpawnV1 {
    At(WorldPosition),
    StartLocation,
    AtGeneratedGroundItem { item: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioItemV1 {
    pub alias: String,
    pub position: WorldPosition,
    pub type_id: String,
    pub charges: i32,
    pub melee_damage_milli: BTreeMap<String, i32>,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    pub ammunition_type: String,
    pub ranged_weapon: Option<RangedWeaponSnapshot>,
    pub magazine_capacity: u32,
    pub integral_magazines: Vec<IntegralMagazinePocketPrototypeV1>,
    pub magazine_wells: Vec<MagazineWellPrototypeV1>,
    pub ammunition_containers: Vec<AmmunitionContainerPocketPrototypeV1>,
    pub residual_energy_millijoules: u32,
    pub powered_tool: Option<PoweredToolStateV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGeneratedItemV1 {
    pub alias: String,
    pub selector: ScenarioGeneratedItemSelectorV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ScenarioGeneratedItemSelectorV1 {
    Ground {
        type_id: String,
        ordinal: u16,
    },
    Pocket {
        owner: String,
        pocket_index: u16,
        ordinal: u16,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ScenarioStepV1 {
    Command {
        actor: String,
        command: ScenarioCommandV1,
    },
    Connection {
        actor: String,
        connected: bool,
    },
    Advance {
        ticks: u16,
    },
    /// Advance every authoritative tick and retain every journal input, but
    /// perform snapshot-mode recovery once at the declared boundary. This is
    /// useful for long action-point debts in production-sized worlds.
    AdvanceBatch {
        ticks: u16,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum ScenarioCommandV1 {
    Move {
        dx: i8,
        dy: i8,
    },
    PickUp {
        item: String,
    },
    Wield {
        item: String,
    },
    Unwield,
    Drop {
        item: String,
    },
    Reload {
        ammunition: String,
        target_pocket_index: Option<u16>,
    },
    RemovePocketItem {
        owner_item: String,
        pocket_index: u16,
        contained_item: String,
    },
    InsertPocketItem {
        owner_item: String,
        pocket_index: u16,
        source_item: String,
    },
    Smash {
        dx: i8,
        dy: i8,
    },
    Wait,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioExpectationV1 {
    pub final_tick: SimTick,
    pub final_state_hash: [u8; 32],
    pub event_trace_hash: [u8; 32],
    pub actors: Vec<ScenarioActorExpectationV1>,
    pub ground_items: Vec<ScenarioGroundItemExpectationV1>,
    pub event_batches: Option<Vec<ScenarioEventBatchV1>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioActorExpectationV1 {
    pub actor: String,
    pub position: WorldPosition,
    pub connected: bool,
    pub inventory: Vec<String>,
    pub wielded: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroundItemExpectationV1 {
    pub item: String,
    pub position: WorldPosition,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEventBatchV1 {
    pub tick: SimTick,
    pub events: Vec<WorldEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioObservationV1 {
    pub format_version: u16,
    pub final_tick: SimTick,
    pub final_state_hash: [u8; 32],
    pub final_snapshot: WorldSnapshotV1,
    pub event_trace_hash: [u8; 32],
    pub event_batches: Vec<ScenarioEventBatchV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScenarioMode {
    Direct,
    SnapshotEachTick,
    SqliteRecovery,
    PortableReplay,
}

impl fmt::Display for ScenarioMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Direct => "direct simulation",
            Self::SnapshotEachTick => "per-tick snapshot round-trip",
            Self::SqliteRecovery => "SQLite recovery",
            Self::PortableReplay => "portable replay",
        })
    }
}

#[derive(Debug)]
pub enum ConformanceError {
    InvalidScenario(&'static str),
    UnknownActor(String),
    UnknownItem(String),
    NumericOverflow,
    Simulation(SimError),
    Persistence(StoreError),
    Codec(postcard::Error),
    Diverged(ScenarioMode),
    ExpectationFailed(String),
}

impl fmt::Display for ConformanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScenario(detail) => write!(formatter, "invalid scenario: {detail}"),
            Self::UnknownActor(alias) => write!(formatter, "unknown actor alias: {alias}"),
            Self::UnknownItem(alias) => write!(formatter, "unknown item alias: {alias}"),
            Self::NumericOverflow => formatter.write_str("scenario numeric overflow"),
            Self::Simulation(error) => write!(formatter, "scenario simulation failed: {error}"),
            Self::Persistence(error) => write!(formatter, "scenario persistence failed: {error}"),
            Self::Codec(error) => write!(formatter, "scenario codec failed: {error}"),
            Self::Diverged(mode) => write!(formatter, "scenario diverged in {mode}"),
            Self::ExpectationFailed(detail) => {
                write!(formatter, "scenario expectation failed: {detail}")
            }
        }
    }
}

impl Error for ConformanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Simulation(error) => Some(error),
            Self::Persistence(error) => Some(error),
            Self::Codec(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SimError> for ConformanceError {
    fn from(error: SimError) -> Self {
        Self::Simulation(error)
    }
}

impl From<StoreError> for ConformanceError {
    fn from(error: StoreError) -> Self {
        Self::Persistence(error)
    }
}

impl From<postcard::Error> for ConformanceError {
    fn from(error: postcard::Error) -> Self {
        Self::Codec(error)
    }
}

struct ScenarioHandles {
    actors: BTreeMap<String, ActorId>,
    items: BTreeMap<String, ItemId>,
}

struct ScenarioExecution {
    initial_world: WorldState,
    final_world: WorldState,
    journal_ticks: Vec<JournalTickV1>,
    event_batches: Vec<ScenarioEventBatchV1>,
}

pub fn verify_scenario(scenario: &ScenarioV1) -> Result<ScenarioObservationV1, ConformanceError> {
    let expected = run_scenario(scenario, ScenarioMode::Direct)?;
    for mode in [
        ScenarioMode::SnapshotEachTick,
        ScenarioMode::SqliteRecovery,
        ScenarioMode::PortableReplay,
    ] {
        if run_scenario(scenario, mode)? != expected {
            return Err(ConformanceError::Diverged(mode));
        }
    }
    verify_expectation(scenario, &expected)?;
    Ok(expected)
}

pub fn run_scenario(
    scenario: &ScenarioV1,
    mode: ScenarioMode,
) -> Result<ScenarioObservationV1, ConformanceError> {
    validate_scenario(scenario)?;
    let snapshot_each_tick = mode == ScenarioMode::SnapshotEachTick;
    let execution = execute_scenario(scenario, snapshot_each_tick)?;
    let final_world = match mode {
        ScenarioMode::Direct | ScenarioMode::SnapshotEachTick => execution.final_world.clone(),
        ScenarioMode::SqliteRecovery => {
            let store = persist_execution(scenario, &execution)?;
            store
                .recover_latest(WorldState::new(
                    scenario.world_namespace,
                    scenario.world_seed,
                ))?
                .1
        }
        ScenarioMode::PortableReplay => {
            let store = persist_execution(scenario, &execution)?;
            let content = scenario_content(scenario);
            let encoded = postcard::to_stdvec(&store.export_replay(content.clone())?)?;
            postcard::from_bytes::<ReplayBundleV1>(&encoded)?.verify(&content)?
        }
    };
    observation(final_world, execution.event_batches)
}

fn validate_scenario(scenario: &ScenarioV1) -> Result<(), ConformanceError> {
    if scenario.format_version != SCENARIO_FORMAT_VERSION {
        return Err(ConformanceError::InvalidScenario(
            "unsupported format version",
        ));
    }
    if scenario.protocol_version != cdda_protocol::PROTOCOL_VERSION
        || scenario.persistence_schema_version != SCHEMA_VERSION
        || scenario.replay_format_version != REPLAY_FORMAT_VERSION
        || scenario.baseline_commit != cdda_protocol::BASELINE_COMMIT
    {
        return Err(ConformanceError::InvalidScenario(
            "runtime version gates differ",
        ));
    }
    if scenario.world_namespace == 0 {
        return Err(ConformanceError::InvalidScenario(
            "world namespace must be nonzero",
        ));
    }
    if scenario.enabled_mods.is_empty()
        || scenario.enabled_mods.len() > 64
        || scenario
            .enabled_mods
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || scenario.enabled_mods.iter().any(|mod_id| {
            mod_id.is_empty() || mod_id.len() > 128 || mod_id.chars().any(char::is_control)
        })
    {
        return Err(ConformanceError::InvalidScenario(
            "enabled mods must be bounded, unique, and sorted",
        ));
    }
    if scenario.chunks.len() > MAX_CHUNKS
        || (scenario.worldgen.is_none() && scenario.chunks.is_empty())
        || scenario.worldgen.as_ref().is_some_and(|worldgen| {
            !scenario.chunks.is_empty()
                || !scenario.terrain.is_empty()
                || !scenario.ground_items.is_empty()
                || !worldgen_catalog_is_valid(worldgen, &scenario.item_groups)
        })
        || (scenario.worldgen.is_none() && !scenario.generated_items.is_empty())
    {
        return Err(ConformanceError::InvalidScenario("invalid chunk count"));
    }
    if scenario.actors.is_empty() || scenario.actors.len() > MAX_ACTORS {
        return Err(ConformanceError::InvalidScenario("invalid actor count"));
    }
    if scenario
        .actors
        .iter()
        .any(|actor| matches!(&actor.spawn, ScenarioActorSpawnV1::StartLocation))
        && scenario
            .worldgen
            .as_ref()
            .and_then(|catalog| catalog.start_location.as_ref())
            .is_none()
    {
        return Err(ConformanceError::InvalidScenario(
            "start-location actor requires a worldgen start selector",
        ));
    }
    if scenario.ground_items.len() > MAX_ITEMS
        || scenario.generated_items.len() > MAX_ITEMS
        || scenario.terrain.len() > MAX_TERRAIN_FIXTURES
        || scenario.steps.len() > MAX_STEPS
    {
        return Err(ConformanceError::InvalidScenario(
            "item or step count exceeds bounds",
        ));
    }
    let ticks = scenario.steps.iter().try_fold(0_u64, |total, step| {
        total.checked_add(match step {
            ScenarioStepV1::Advance { ticks } | ScenarioStepV1::AdvanceBatch { ticks } => {
                u64::from(*ticks)
            }
            ScenarioStepV1::Command { .. } | ScenarioStepV1::Connection { .. } => 1,
        })
    });
    if ticks.is_none_or(|ticks| ticks == 0 || ticks > MAX_TICKS) {
        return Err(ConformanceError::InvalidScenario(
            "invalid total tick count",
        ));
    }
    let mut aliases = BTreeSet::new();
    for alias in scenario
        .actors
        .iter()
        .map(|actor| actor.alias.as_str())
        .chain(scenario.ground_items.iter().map(|item| item.alias.as_str()))
        .chain(
            scenario
                .generated_items
                .iter()
                .map(|item| item.alias.as_str()),
        )
    {
        if !valid_alias(alias) || !aliases.insert(alias) || aliases.len() > MAX_ALIASES {
            return Err(ConformanceError::InvalidScenario(
                "aliases must be unique and bounded",
            ));
        }
    }
    let mut generated_aliases = BTreeSet::new();
    for generated in &scenario.generated_items {
        match &generated.selector {
            ScenarioGeneratedItemSelectorV1::Ground { type_id, .. } => {
                if type_id.is_empty()
                    || type_id.len() > 512
                    || type_id.chars().any(char::is_control)
                {
                    return Err(ConformanceError::InvalidScenario(
                        "generated ground selector type must be bounded",
                    ));
                }
            }
            ScenarioGeneratedItemSelectorV1::Pocket { owner, .. } => {
                if !generated_aliases.contains(owner.as_str()) {
                    return Err(ConformanceError::InvalidScenario(
                        "generated pocket selector owner must precede the child alias",
                    ));
                }
            }
        }
        generated_aliases.insert(generated.alias.as_str());
    }
    if scenario.actors.iter().any(|actor| {
        matches!(
            &actor.spawn,
            ScenarioActorSpawnV1::AtGeneratedGroundItem { item }
                if !generated_aliases.contains(item.as_str())
        )
    }) {
        return Err(ConformanceError::InvalidScenario(
            "generated-item actor spawn references an unknown alias",
        ));
    }
    if scenario.steps.iter().any(|step| {
        matches!(
            step,
            ScenarioStepV1::Advance { ticks: 0 } | ScenarioStepV1::AdvanceBatch { ticks: 0 }
        )
    }) {
        return Err(ConformanceError::InvalidScenario(
            "advance step must contain at least one tick",
        ));
    }
    for actor in &scenario.expected.actors {
        if !aliases.contains(actor.actor.as_str())
            || actor
                .inventory
                .iter()
                .chain(actor.wielded.iter())
                .any(|item| !aliases.contains(item.as_str()))
        {
            return Err(ConformanceError::InvalidScenario(
                "expectation references an unknown alias",
            ));
        }
    }
    if scenario
        .expected
        .ground_items
        .iter()
        .any(|item| !aliases.contains(item.item.as_str()))
    {
        return Err(ConformanceError::InvalidScenario(
            "expectation references an unknown alias",
        ));
    }
    let expected_actors = scenario
        .expected
        .actors
        .iter()
        .map(|actor| actor.actor.as_str())
        .collect::<BTreeSet<_>>();
    let expected_ground_items = scenario
        .expected
        .ground_items
        .iter()
        .map(|item| item.item.as_str())
        .collect::<BTreeSet<_>>();
    if expected_actors.len() != scenario.expected.actors.len()
        || expected_ground_items.len() != scenario.expected.ground_items.len()
        || scenario.expected.actors.iter().any(|actor| {
            actor.inventory.iter().collect::<BTreeSet<_>>().len() != actor.inventory.len()
        })
    {
        return Err(ConformanceError::InvalidScenario(
            "expectations must not contain duplicate aliases",
        ));
    }
    Ok(())
}

fn valid_alias(alias: &str) -> bool {
    !alias.is_empty() && alias.len() <= MAX_ALIAS_BYTES && !alias.chars().any(char::is_control)
}

fn execute_scenario(
    scenario: &ScenarioV1,
    snapshot_each_tick: bool,
) -> Result<ScenarioExecution, ConformanceError> {
    let (mut world, handles) = build_world(scenario)?;
    let initial_world = world.clone();
    let mut sequences = BTreeMap::<ActorId, u64>::new();
    let mut journal_ticks = Vec::new();
    let mut event_batches = Vec::new();
    for step in &scenario.steps {
        match step {
            ScenarioStepV1::Command { actor, command } => {
                let actor_id = handles
                    .actors
                    .get(actor)
                    .copied()
                    .ok_or_else(|| ConformanceError::UnknownActor(actor.clone()))?;
                let sequence = sequences.entry(actor_id).or_default();
                *sequence = sequence
                    .checked_add(1)
                    .ok_or(ConformanceError::NumericOverflow)?;
                let command = ClientCommand {
                    actor_id,
                    sequence: CommandSequence(*sequence),
                    client_tick: world.tick(),
                    kind: resolve_command(command, &handles)?,
                };
                advance_and_record(
                    &mut world,
                    vec![command],
                    Vec::new(),
                    snapshot_each_tick,
                    &mut journal_ticks,
                    &mut event_batches,
                )?;
            }
            ScenarioStepV1::Connection { actor, connected } => {
                let actor_id = handles
                    .actors
                    .get(actor)
                    .copied()
                    .ok_or_else(|| ConformanceError::UnknownActor(actor.clone()))?;
                advance_and_record(
                    &mut world,
                    Vec::new(),
                    vec![ActorConnectionUpdateV1 {
                        actor_id,
                        connected: *connected,
                    }],
                    snapshot_each_tick,
                    &mut journal_ticks,
                    &mut event_batches,
                )?;
            }
            ScenarioStepV1::Advance { ticks } => {
                for _ in 0..*ticks {
                    advance_and_record(
                        &mut world,
                        Vec::new(),
                        Vec::new(),
                        snapshot_each_tick,
                        &mut journal_ticks,
                        &mut event_batches,
                    )?;
                }
            }
            ScenarioStepV1::AdvanceBatch { ticks } => {
                for _ in 0..*ticks {
                    advance_and_record(
                        &mut world,
                        Vec::new(),
                        Vec::new(),
                        false,
                        &mut journal_ticks,
                        &mut event_batches,
                    )?;
                }
                if snapshot_each_tick {
                    round_trip_world_snapshot(&mut world)?;
                }
            }
        }
    }
    Ok(ScenarioExecution {
        initial_world,
        final_world: world,
        journal_ticks,
        event_batches,
    })
}

fn build_world(scenario: &ScenarioV1) -> Result<(WorldState, ScenarioHandles), ConformanceError> {
    let mut world = WorldState::new(scenario.world_namespace, scenario.world_seed);
    world.install_reserved_block(ReservedIdBlock::new(1, ID_RESERVATION_SIZE)?)?;
    world.register_item_group_catalog(scenario.item_groups.clone())?;
    for profile in &scenario.smash_item_types {
        world.register_smash_item_type(profile.clone())?;
    }
    for bash in &scenario.terrain_bash_types {
        world.register_terrain_bash_type(bash.clone())?;
    }
    if let Some(worldgen) = &scenario.worldgen {
        world.configure_worldgen(worldgen.clone())?;
        world.generate_initial_bubble(WorldPosition { x: 0, y: 0, z: 0 })?;
    } else {
        let mut chunks = BTreeMap::new();
        for coord in &scenario.chunks {
            if chunks.insert(*coord, Chunk::floor(*coord)).is_some() {
                return Err(ConformanceError::InvalidScenario("duplicate chunk"));
            }
        }
        let mut terrain_positions = BTreeSet::new();
        for fixture in &scenario.terrain {
            let (coord, local) = fixture.position.chunk_and_local();
            if !terrain_positions.insert(fixture.position) {
                return Err(ConformanceError::InvalidScenario(
                    "duplicate terrain fixture",
                ));
            }
            chunks
                .get_mut(&coord)
                .ok_or(ConformanceError::InvalidScenario(
                    "terrain fixture is outside declared chunks",
                ))?
                .set_terrain(local, fixture.terrain.clone())?;
        }
        for (_, chunk) in chunks {
            world.insert_chunk(chunk);
        }
    }
    let mut items = resolve_generated_item_aliases(&world, &scenario.generated_items)?;
    let mut actors = BTreeMap::new();
    let mut actor_specs = scenario.actors.iter().collect::<Vec<_>>();
    actor_specs.sort_by(|left, right| left.alias.cmp(&right.alias));
    for actor in actor_specs {
        let id = match &actor.spawn {
            ScenarioActorSpawnV1::At(position) => {
                world.spawn_actor_with_base_stats(*position, actor.connected, actor.stats)?
            }
            ScenarioActorSpawnV1::StartLocation => {
                if scenario
                    .worldgen
                    .as_ref()
                    .and_then(|catalog| catalog.start_location.as_ref())
                    .is_none()
                {
                    return Err(ConformanceError::InvalidScenario(
                        "start-location actor requires a worldgen start selector",
                    ));
                }
                world.spawn_actor_first_available_with_stats(actor.connected, actor.stats)?
            }
            ScenarioActorSpawnV1::AtGeneratedGroundItem { item } => {
                let item_id = items
                    .get(item)
                    .copied()
                    .ok_or_else(|| ConformanceError::UnknownItem(item.clone()))?;
                let position = world
                    .ground_item_snapshot(item_id)
                    .ok_or(ConformanceError::InvalidScenario(
                        "generated-item actor spawn requires a top-level ground item",
                    ))?
                    .position;
                world.spawn_actor_with_base_stats(position, actor.connected, actor.stats)?
            }
        };
        actors.insert(actor.alias.clone(), id);
    }
    let mut item_specs = scenario.ground_items.iter().collect::<Vec<_>>();
    item_specs.sort_by(|left, right| left.alias.cmp(&right.alias));
    for item in item_specs {
        let spawn = ItemSpawn {
            position: item.position,
            type_id: item.type_id.clone(),
            charges: item.charges,
            melee_damage_milli: item.melee_damage_milli.clone(),
            calories: item.calories,
            quench: item.quench,
            comestible_type: item.comestible_type.clone(),
            ammunition_type: item.ammunition_type.clone(),
            ranged_weapon: item.ranged_weapon.clone(),
        };
        let id = if !item.ammunition_containers.is_empty() {
            if item.magazine_capacity != 0
                || !item.integral_magazines.is_empty()
                || !item.magazine_wells.is_empty()
                || item.residual_energy_millijoules != 0
                || item.powered_tool.is_some()
            {
                return Err(ConformanceError::InvalidScenario(
                    "ammunition-container fixtures cannot mix storage families",
                ));
            }
            world.spawn_ground_item_with_ammunition_containers(
                spawn,
                item.ammunition_containers.clone(),
            )?
        } else if item.integral_magazines.is_empty() {
            world.spawn_ground_item_with_powered_magazine_wells(
                spawn,
                item.magazine_capacity,
                item.magazine_wells.clone(),
                item.residual_energy_millijoules,
                item.powered_tool.clone(),
            )?
        } else {
            if item.magazine_capacity != 0 || item.residual_energy_millijoules != 0 {
                return Err(ConformanceError::InvalidScenario(
                    "item-backed magazines cannot carry aggregate storage",
                ));
            }
            world.spawn_ground_item_with_item_backed_magazines(
                spawn,
                item.integral_magazines.clone(),
                item.magazine_wells.clone(),
                item.powered_tool.clone(),
            )?
        };
        items.insert(item.alias.clone(), id);
    }
    Ok((world, ScenarioHandles { actors, items }))
}

fn resolve_generated_item_aliases(
    world: &WorldState,
    generated_items: &[ScenarioGeneratedItemV1],
) -> Result<BTreeMap<String, ItemId>, ConformanceError> {
    let snapshot = world.snapshot();
    let mut aliases = BTreeMap::new();
    let mut selected_ids = BTreeSet::new();
    for generated in generated_items {
        let item_id = match &generated.selector {
            ScenarioGeneratedItemSelectorV1::Ground { type_id, ordinal } => snapshot
                .ground_items
                .iter()
                .filter(|ground| ground.item.type_id == *type_id)
                .nth(usize::from(*ordinal))
                .map(|ground| ground.item.id),
            ScenarioGeneratedItemSelectorV1::Pocket {
                owner,
                pocket_index,
                ordinal,
            } => {
                let owner_id = aliases
                    .get(owner)
                    .copied()
                    .ok_or_else(|| ConformanceError::UnknownItem(owner.clone()))?;
                find_item_snapshot(&snapshot, owner_id)
                    .and_then(|owner| {
                        owner
                            .ammunition_containers
                            .iter()
                            .find(|pocket| pocket.pocket_index == *pocket_index)
                    })
                    .and_then(|pocket| pocket.contents.get(usize::from(*ordinal)))
                    .map(|item| item.id)
            }
        }
        .ok_or(ConformanceError::InvalidScenario(
            "generated item selector did not resolve exactly",
        ))?;
        if !selected_ids.insert(item_id) {
            return Err(ConformanceError::InvalidScenario(
                "generated item selectors must identify distinct items",
            ));
        }
        aliases.insert(generated.alias.clone(), item_id);
    }
    Ok(aliases)
}

fn find_item_snapshot(snapshot: &WorldSnapshotV1, item_id: ItemId) -> Option<&ItemSnapshot> {
    snapshot
        .ground_items
        .iter()
        .find_map(|ground| find_nested_item_snapshot(&ground.item, item_id))
        .or_else(|| {
            snapshot.actors.iter().find_map(|actor| {
                actor
                    .inventory
                    .iter()
                    .find_map(|item| find_nested_item_snapshot(item, item_id))
            })
        })
}

fn find_nested_item_snapshot(item: &ItemSnapshot, item_id: ItemId) -> Option<&ItemSnapshot> {
    if item.id == item_id {
        return Some(item);
    }
    item.integral_magazines
        .iter()
        .filter_map(|pocket| pocket.loaded_ammunition.as_deref())
        .chain(
            item.magazine_wells
                .iter()
                .filter_map(|well| well.installed_magazine.as_deref()),
        )
        .chain(
            item.ammunition_containers
                .iter()
                .flat_map(|pocket| &pocket.contents),
        )
        .find_map(|child| find_nested_item_snapshot(child, item_id))
}

fn resolve_command(
    command: &ScenarioCommandV1,
    handles: &ScenarioHandles,
) -> Result<CommandKind, ConformanceError> {
    let item_id = |alias: &str| {
        handles
            .items
            .get(alias)
            .copied()
            .ok_or_else(|| ConformanceError::UnknownItem(alias.to_owned()))
    };
    Ok(match command {
        ScenarioCommandV1::Move { dx, dy } => CommandKind::Move {
            dx: *dx,
            dy: *dy,
            dz: 0,
        },
        ScenarioCommandV1::PickUp { item } => CommandKind::PickUp {
            item_id: item_id(item)?,
        },
        ScenarioCommandV1::Wield { item } => CommandKind::Wield {
            item_id: item_id(item)?,
        },
        ScenarioCommandV1::Unwield => CommandKind::Unwield,
        ScenarioCommandV1::Drop { item } => CommandKind::Drop {
            item_id: item_id(item)?,
        },
        ScenarioCommandV1::Reload {
            ammunition,
            target_pocket_index,
        } => CommandKind::Reload {
            ammunition_item: item_id(ammunition)?,
            target_pocket_index: *target_pocket_index,
        },
        ScenarioCommandV1::RemovePocketItem {
            owner_item,
            pocket_index,
            contained_item,
        } => CommandKind::RemovePocketItem {
            owner_item: item_id(owner_item)?,
            pocket_index: *pocket_index,
            contained_item: item_id(contained_item)?,
        },
        ScenarioCommandV1::InsertPocketItem {
            owner_item,
            pocket_index,
            source_item,
        } => CommandKind::InsertPocketItem {
            owner_item: item_id(owner_item)?,
            pocket_index: *pocket_index,
            source_item: item_id(source_item)?,
        },
        ScenarioCommandV1::Smash { dx, dy } => CommandKind::Smash { dx: *dx, dy: *dy },
        ScenarioCommandV1::Wait => CommandKind::Wait,
    })
}

fn advance_and_record(
    world: &mut WorldState,
    commands: Vec<ClientCommand>,
    connection_updates: Vec<ActorConnectionUpdateV1>,
    snapshot_each_tick: bool,
    journal_ticks: &mut Vec<JournalTickV1>,
    event_batches: &mut Vec<ScenarioEventBatchV1>,
) -> Result<(), ConformanceError> {
    let outcome = world.advance_tick_with_recovery_inputs(
        commands.clone(),
        Vec::new(),
        connection_updates.clone(),
    )?;
    let events_hash = canonical_events_hash(&outcome.events)?;
    journal_ticks.push(JournalTickV1 {
        tick: outcome.tick,
        commands,
        held_movement: Vec::new(),
        connection_updates,
        events_hash,
        state_hash: outcome.canonical_hash,
    });
    event_batches.push(ScenarioEventBatchV1 {
        tick: outcome.tick,
        events: outcome.events,
    });
    if snapshot_each_tick {
        round_trip_world_snapshot(world)?;
    }
    Ok(())
}

fn round_trip_world_snapshot(world: &mut WorldState) -> Result<(), ConformanceError> {
    let encoded = postcard::to_stdvec(&world.snapshot())?;
    let snapshot = postcard::from_bytes(&encoded)?;
    *world = WorldState::from_snapshot(&snapshot)?;
    Ok(())
}

fn persist_execution(
    scenario: &ScenarioV1,
    execution: &ScenarioExecution,
) -> Result<WorldStore, ConformanceError> {
    let mut store = WorldStore::open_in_memory()?;
    store.initialize_world(scenario.world_namespace, scenario.world_seed)?;
    store.write_snapshot(0, &execution.initial_world)?;
    store.append_journal_batch(&JournalBatchV1 {
        ticks: execution.journal_ticks.clone(),
        allocator_inputs: Vec::new(),
    })?;
    Ok(store)
}

fn scenario_content(scenario: &ScenarioV1) -> ContentIdentity {
    ContentIdentity {
        baseline_commit: cdda_protocol::BASELINE_COMMIT.to_owned(),
        manifest_hash: scenario.content_manifest_hash,
        enabled_mods: scenario.enabled_mods.clone(),
    }
}

fn observation(
    world: WorldState,
    event_batches: Vec<ScenarioEventBatchV1>,
) -> Result<ScenarioObservationV1, ConformanceError> {
    let event_trace_hash = scenario_event_trace_hash(&event_batches)?;
    Ok(ScenarioObservationV1 {
        format_version: OBSERVATION_FORMAT_VERSION,
        final_tick: world.tick(),
        final_state_hash: world.canonical_hash()?,
        final_snapshot: world.snapshot(),
        event_trace_hash,
        event_batches,
    })
}

fn verify_expectation(
    scenario: &ScenarioV1,
    observation: &ScenarioObservationV1,
) -> Result<(), ConformanceError> {
    if observation.final_tick != scenario.expected.final_tick {
        return Err(ConformanceError::ExpectationFailed(String::from(
            "final tick differs",
        )));
    }
    if observation.final_state_hash != scenario.expected.final_state_hash {
        return Err(ConformanceError::ExpectationFailed(format!(
            "final state hash differs: expected {:02x?}, observed {:02x?}",
            scenario.expected.final_state_hash, observation.final_state_hash
        )));
    }
    if observation.event_trace_hash != scenario.expected.event_trace_hash {
        return Err(ConformanceError::ExpectationFailed(format!(
            "event trace hash differs: expected {:02x?}, observed {:02x?}",
            scenario.expected.event_trace_hash, observation.event_trace_hash
        )));
    }
    if scenario
        .expected
        .event_batches
        .as_ref()
        .is_some_and(|events| events != &observation.event_batches)
    {
        return Err(ConformanceError::ExpectationFailed(String::from(
            "event trace differs",
        )));
    }
    let (_, handles) = build_world(scenario)?;
    if observation.final_snapshot.actors.len() != scenario.expected.actors.len() {
        return Err(ConformanceError::ExpectationFailed(String::from(
            "actor count differs",
        )));
    }
    for expected in &scenario.expected.actors {
        let id = handles
            .actors
            .get(&expected.actor)
            .ok_or_else(|| ConformanceError::UnknownActor(expected.actor.clone()))?;
        let actor = observation
            .final_snapshot
            .actors
            .iter()
            .find(|actor| actor.id == *id)
            .ok_or_else(|| {
                ConformanceError::ExpectationFailed(format!("actor {} is absent", expected.actor))
            })?;
        let expected_inventory = expected
            .inventory
            .iter()
            .map(|alias| {
                handles
                    .items
                    .get(alias)
                    .copied()
                    .ok_or_else(|| ConformanceError::UnknownItem(alias.clone()))
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let actual_inventory = actor
            .inventory
            .iter()
            .map(|item| item.id)
            .collect::<BTreeSet<_>>();
        let expected_wielded = expected
            .wielded
            .as_ref()
            .map(|alias| {
                handles
                    .items
                    .get(alias)
                    .copied()
                    .ok_or_else(|| ConformanceError::UnknownItem(alias.clone()))
            })
            .transpose()?;
        if actor.position != expected.position
            || actor.connected != expected.connected
            || actual_inventory != expected_inventory
            || actor.wielded != expected_wielded
        {
            return Err(ConformanceError::ExpectationFailed(format!(
                "actor {} state differs",
                expected.actor
            )));
        }
    }
    if observation.final_snapshot.ground_items.len() != scenario.expected.ground_items.len() {
        return Err(ConformanceError::ExpectationFailed(String::from(
            "ground item count differs",
        )));
    }
    for expected in &scenario.expected.ground_items {
        let id = handles
            .items
            .get(&expected.item)
            .ok_or_else(|| ConformanceError::UnknownItem(expected.item.clone()))?;
        if !observation
            .final_snapshot
            .ground_items
            .iter()
            .any(|item| item.item.id == *id && item.position == expected.position)
        {
            return Err(ConformanceError::ExpectationFailed(format!(
                "ground item {} state differs",
                expected.item
            )));
        }
    }
    Ok(())
}

fn scenario_event_trace_hash(
    event_batches: &[ScenarioEventBatchV1],
) -> Result<[u8; 32], ConformanceError> {
    let encoded = postcard::to_stdvec(event_batches)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"CddaScenarioEventsV5\0");
    hasher.update(&encoded);
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_flow_scenario() -> ScenarioV1 {
        ScenarioV1 {
            format_version: SCENARIO_FORMAT_VERSION,
            protocol_version: cdda_protocol::PROTOCOL_VERSION,
            persistence_schema_version: SCHEMA_VERSION,
            replay_format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
            world_namespace: 900,
            world_seed: [9; 32],
            content_manifest_hash: [7; 32],
            enabled_mods: vec![String::from("dda")],
            item_groups: Vec::new(),
            terrain_bash_types: Vec::new(),
            smash_item_types: Vec::new(),
            worldgen: None,
            chunks: vec![ChunkCoord { x: 0, y: 0, z: 0 }],
            terrain: Vec::new(),
            actors: vec![ScenarioActorV1 {
                alias: String::from("survivor"),
                spawn: ScenarioActorSpawnV1::At(WorldPosition { x: 1, y: 1, z: 0 }),
                connected: true,
                stats: CharacterCreationStatsV1::default(),
            }],
            ground_items: vec![ScenarioItemV1 {
                alias: String::from("rock"),
                position: WorldPosition { x: 1, y: 1, z: 0 },
                type_id: String::from("rock"),
                charges: 1,
                melee_damage_milli: BTreeMap::from([(String::from("bash"), 7_000)]),
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
            }],
            generated_items: Vec::new(),
            steps: vec![
                ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command: ScenarioCommandV1::PickUp {
                        item: String::from("rock"),
                    },
                },
                ScenarioStepV1::Advance { ticks: 25 },
                ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command: ScenarioCommandV1::Wield {
                        item: String::from("rock"),
                    },
                },
                ScenarioStepV1::Advance { ticks: 25 },
                ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command: ScenarioCommandV1::Drop {
                        item: String::from("rock"),
                    },
                },
                ScenarioStepV1::Advance { ticks: 25 },
                ScenarioStepV1::Connection {
                    actor: String::from("survivor"),
                    connected: false,
                },
                ScenarioStepV1::Advance { ticks: 1 },
            ],
            expected: ScenarioExpectationV1 {
                final_tick: SimTick(80),
                final_state_hash: [
                    0x96, 0x40, 0x3a, 0xc5, 0x36, 0x1f, 0xb1, 0x3c, 0xee, 0x8b, 0x45, 0x4b, 0x8c,
                    0x6c, 0xcd, 0x44, 0x6f, 0xe8, 0xf3, 0xa2, 0x50, 0x38, 0x31, 0x22, 0x71, 0x2e,
                    0x7b, 0x18, 0x15, 0x59, 0xa5, 0xec,
                ],
                event_trace_hash: [
                    0x44, 0x45, 0x7b, 0xe9, 0xc8, 0xc2, 0xfe, 0x22, 0xa1, 0x86, 0x4f, 0x43, 0x0f,
                    0x07, 0x4a, 0x20, 0xcc, 0xee, 0x48, 0xcd, 0xa0, 0x5d, 0xba, 0xcf, 0x69, 0x3d,
                    0x95, 0xd3, 0x93, 0x3d, 0x31, 0x28,
                ],
                actors: vec![ScenarioActorExpectationV1 {
                    actor: String::from("survivor"),
                    position: WorldPosition { x: 1, y: 1, z: 0 },
                    connected: false,
                    inventory: Vec::new(),
                    wielded: None,
                }],
                ground_items: vec![ScenarioGroundItemExpectationV1 {
                    item: String::from("rock"),
                    position: WorldPosition { x: 1, y: 1, z: 0 },
                }],
                event_batches: None,
            },
        }
    }

    #[test]
    fn item_flow_is_identical_across_all_conformance_modes() {
        let observation = verify_scenario(&item_flow_scenario())
            .expect("item scenario should conform across every mode");
        assert_eq!(observation.format_version, OBSERVATION_FORMAT_VERSION);
        let actor = observation
            .final_snapshot
            .actors
            .first()
            .expect("scenario actor should remain");
        assert!(!actor.connected);
        assert!(actor.inventory.is_empty());
        assert_eq!(actor.wielded, None);
        assert_eq!(observation.final_snapshot.ground_items.len(), 1);
        let encoded = postcard::to_stdvec(&observation.final_snapshot)
            .expect("the audited snapshot should encode");
        let mut legacy_hasher = blake3::Hasher::new_derive_key("cdda-rust CanonicalStateV72");
        legacy_hasher.update(&encoded);
        assert_eq!(
            legacy_hasher.finalize().as_bytes(),
            &[
                0x91, 0x6f, 0x4a, 0x8d, 0x8d, 0x41, 0x95, 0xec, 0xbd, 0x51, 0x02, 0x44, 0xd3, 0x22,
                0x3a, 0xfb, 0x8e, 0xc1, 0x8a, 0xa0, 0x9c, 0x69, 0x79, 0x46, 0x1f, 0x68, 0x2d, 0x4d,
                0x93, 0x15, 0x72, 0xec,
            ],
            "the old V72 domain must pin the Protocol 97 item-flow Postcard bytes independently of the V73 domain bump"
        );
        assert_eq!(
            observation.final_snapshot.ground_items[0].item.type_id,
            "rock"
        );
    }

    #[test]
    fn start_location_spawns_are_identical_across_all_conformance_modes() {
        let terrain = |terrain_id: &str| TerrainTileSnapshot {
            terrain_id: terrain_id.to_owned(),
            move_cost: 2,
            transparent: true,
            flat: true,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        let cell = |prototype_index| cdda_protocol::WorldgenCellV1 {
            terrain: vec![vec![cdda_protocol::WorldgenWeightedTerrainTargetV1 {
                target: cdda_protocol::WorldgenTerrainTargetV1::Prototype(prototype_index),
                weight: 1,
            }]],
            furniture: vec![vec![cdda_protocol::WorldgenWeightedFurnitureTargetV1 {
                target: cdda_protocol::WorldgenFurnitureTargetV1::None,
                weight: 1,
            }]],
            item_group: None,
        };
        let worldgen = WorldgenCatalogV1 {
            generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
            overmap: cdda_protocol::WorldgenOvermapLayoutV1 {
                origin_x: -90,
                origin_y: -90,
                identities: vec![
                    cdda_protocol::WorldgenOmtIdentityV1 {
                        full_id: String::from("field"),
                        type_id: String::from("field"),
                        subtype_id: String::from("field"),
                        generator_id: String::from("field"),
                        rotation: 0,
                    },
                    cdda_protocol::WorldgenOmtIdentityV1 {
                        full_id: String::from("lmoe_north"),
                        type_id: String::from("lmoe"),
                        subtype_id: String::from("lmoe"),
                        generator_id: String::from("lmoe"),
                        rotation: 0,
                    },
                ],
                layers: vec![cdda_protocol::WorldgenOvermapLayerV1 {
                    z: 0,
                    runs: vec![
                        cdda_protocol::WorldgenOvermapRunV1 {
                            identity_index: 0,
                            length: 90 * u32::from(cdda_protocol::WORLDGEN_OVERMAP_WIDTH) + 90,
                        },
                        cdda_protocol::WorldgenOvermapRunV1 {
                            identity_index: 1,
                            length: 1,
                        },
                        cdda_protocol::WorldgenOvermapRunV1 {
                            identity_index: 0,
                            length: u32::from(cdda_protocol::WORLDGEN_OVERMAP_WIDTH)
                                * u32::from(cdda_protocol::WORLDGEN_OVERMAP_HEIGHT)
                                - (90 * u32::from(cdda_protocol::WORLDGEN_OVERMAP_WIDTH) + 90)
                                - 1,
                        },
                    ],
                }],
            },
            cities: vec![cdda_protocol::WorldgenCityV1 {
                city_id: cdda_protocol::WorldgenCityId(1),
                center: cdda_protocol::ChunkCoord { x: 0, y: 0, z: 0 },
                size: 8,
            }],
            rivers: Vec::new(),
            specials: Vec::new(),
            start_location: Some(cdda_protocol::WorldgenStartLocationV1 {
                start_location_id: String::from("sloc_lmoe"),
                targets: vec![cdda_protocol::WorldgenStartTargetV1 {
                    omt: String::from("lmoe"),
                    match_type: cdda_protocol::WorldgenOmtMatchTypeV1::Type,
                }],
                city_sizes: cdda_protocol::WorldgenI32IntervalV1 {
                    minimum: 0,
                    maximum: i32::MAX,
                },
                city_distance: cdda_protocol::WorldgenI32IntervalV1 {
                    minimum: 0,
                    maximum: i32::MAX,
                },
            }),
            terrain_prototypes: vec![terrain("t_field"), terrain("t_lmoe_floor")],
            furniture_prototypes: Vec::new(),
            regional_terrain: Vec::new(),
            regional_furniture: Vec::new(),
            omt_generators: vec![
                cdda_protocol::WorldgenOmtGeneratorV1 {
                    omt_id: String::from("field"),
                    templates: vec![cdda_protocol::WorldgenTemplateV1 {
                        weight: 1,
                        predecessor_id: None,
                        builtin: None,
                        cells: vec![cell(0); cdda_protocol::WORLDGEN_CELLS_PER_OMT],
                        nested: Vec::new(),
                        area_items: Vec::new(),
                        erase_all_before_placing_terrain: false,
                        deferred_fields: Vec::new(),
                    }],
                    nested_generators: Vec::new(),
                },
                cdda_protocol::WorldgenOmtGeneratorV1 {
                    omt_id: String::from("lmoe"),
                    templates: vec![cdda_protocol::WorldgenTemplateV1 {
                        weight: 1,
                        predecessor_id: None,
                        builtin: None,
                        cells: vec![cell(1); cdda_protocol::WORLDGEN_CELLS_PER_OMT],
                        nested: Vec::new(),
                        area_items: Vec::new(),
                        erase_all_before_placing_terrain: false,
                        deferred_fields: Vec::new(),
                    }],
                    nested_generators: Vec::new(),
                },
            ],
        };
        let actor = |alias: &str| ScenarioActorV1 {
            alias: alias.to_owned(),
            spawn: ScenarioActorSpawnV1::StartLocation,
            connected: true,
            stats: CharacterCreationStatsV1::default(),
        };
        let scenario = ScenarioV1 {
            format_version: SCENARIO_FORMAT_VERSION,
            protocol_version: cdda_protocol::PROTOCOL_VERSION,
            persistence_schema_version: SCHEMA_VERSION,
            replay_format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
            world_namespace: 906,
            world_seed: [31; 32],
            content_manifest_hash: [32; 32],
            enabled_mods: vec![String::from("dda")],
            item_groups: Vec::new(),
            terrain_bash_types: Vec::new(),
            smash_item_types: Vec::new(),
            worldgen: Some(worldgen),
            chunks: Vec::new(),
            terrain: Vec::new(),
            actors: vec![actor("alpha"), actor("beta")],
            ground_items: Vec::new(),
            generated_items: Vec::new(),
            steps: vec![ScenarioStepV1::Advance { ticks: 1 }],
            expected: ScenarioExpectationV1 {
                final_tick: SimTick(0),
                final_state_hash: [0; 32],
                event_trace_hash: [0; 32],
                actors: Vec::new(),
                ground_items: Vec::new(),
                event_batches: None,
            },
        };
        let direct = run_scenario(&scenario, ScenarioMode::Direct)
            .expect("start-location scenario should run directly");
        for mode in [
            ScenarioMode::SnapshotEachTick,
            ScenarioMode::SqliteRecovery,
            ScenarioMode::PortableReplay,
        ] {
            assert_eq!(
                run_scenario(&scenario, mode).expect("start-location scenario should recover"),
                direct,
                "{mode} must preserve start selection"
            );
        }
        assert_eq!(direct.final_snapshot.chunks.len(), 144);
        assert_eq!(direct.final_snapshot.actors.len(), 2);
        assert!(direct.final_snapshot.actors.iter().all(|actor| {
            (0..cdda_protocol::WORLDGEN_OMT_SIZE as i32).contains(&actor.position.x)
                && (0..cdda_protocol::WORLDGEN_OMT_SIZE as i32).contains(&actor.position.y)
                && actor.position.z == 0
        }));
        assert_ne!(
            direct.final_snapshot.actors[0].position,
            direct.final_snapshot.actors[1].position
        );
        assert_eq!(
            direct
                .final_snapshot
                .worldgen
                .as_ref()
                .and_then(|catalog| catalog.start_location.as_ref())
                .map(|start| start.start_location_id.as_str()),
            Some("sloc_lmoe")
        );
    }

    #[test]
    fn named_item_group_bash_is_identical_across_snapshot_sqlite_and_replay() {
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let wall_position = WorldPosition { x: 2, y: 1, z: 0 };
        let item_leaf = |type_id: &str, charges: Option<cdda_protocol::ItemGroupChargeRangeV1>| {
            cdda_protocol::ItemGroupTargetV1::Item(Box::new(
                cdda_protocol::ItemGroupItemPrototypeV1 {
                    prototype: cdda_protocol::CraftItemPrototypeV1 {
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
                        containment: cdda_protocol::ItemContainmentProfileV1 {
                            volume_milliliters: match type_id {
                                "splinter" => 250,
                                "nail" => 500,
                                _ => 0,
                            },
                            ..Default::default()
                        },
                    },
                    maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
                    variants: Vec::new(),
                    description_expansion: None,
                    snippets: Vec::new(),
                    initial_variables: BTreeMap::new(),
                    default_container: None,
                    modifier_side_effects_supported: true,
                    minimum_one_charge: charges.is_some(),
                    charges,
                    tool_charge_storage: None,
                    charges_supported: true,
                    charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
                    contents_insertion_supported: true,
                },
            ))
        };
        let cdda_protocol::ItemGroupTargetV1::Item(mut wrapper_item) =
            item_leaf("flexible_salvage_wrapper", None)
        else {
            unreachable!("wrapper fixture is a direct item")
        };
        wrapper_item.prototype.ammunition_containers =
            vec![cdda_protocol::AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("SALVAGE"),
                capacities: Vec::new(),
                rigid: false,
                access_moves: 100,
                reloadable: false,
                unloadable: true,
                spawn_rules: Some(cdda_protocol::SpawnPocketRulesV1 {
                    kind: cdda_protocol::SpawnPocketKindV1::Container,
                    // Two 250 ml splinters, one 500 ml nail stack, the 251 ml
                    // painkiller bottle, and the 137 ml tablet exactly fill
                    // the wrapper, preserving pinned seal-on-full behavior.
                    max_contains_volume_milliliters: 1_388,
                    magazine_well_volume_milliliters: 45,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: 10_000,
                    max_item_volume_milliliters: 1_000,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: 1_000,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 100,
                    rigid: false,
                    watertight: false,
                    transparent: false,
                    forbidden: false,
                    sealable: true,
                }),
            }];
        let mut wearable_light = item_leaf(
            "wearable_light",
            Some(cdda_protocol::ItemGroupChargeRangeV1 {
                minimum: 100,
                maximum: 100,
            }),
        );
        let cdda_protocol::ItemGroupTargetV1::Item(wearable_light_item) = &mut wearable_light
        else {
            unreachable!("headlamp fixture is a direct item")
        };
        wearable_light_item.minimum_one_charge = false;
        wearable_light_item.charge_capacity =
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage;
        wearable_light_item.prototype.charges = 0;
        wearable_light_item.prototype.magazine_wells =
            vec![cdda_protocol::MagazineWellPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("MAGAZINE_WELL"),
                compatible_magazine_type_ids: vec![String::from("medium_battery_cell")],
                rigid: true,
                unloadable: true,
            }];
        let mut magazine = item_leaf("medium_battery_cell", None);
        let cdda_protocol::ItemGroupTargetV1::Item(magazine) = &mut magazine else {
            unreachable!("magazine fixture is a direct item")
        };
        magazine.prototype.charges = 0;
        magazine.prototype.integral_magazines =
            vec![cdda_protocol::IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("MAGAZINE"),
                ammunition_type: String::from("battery"),
                capacity: 56,
                rigid: true,
                reloadable: false,
                unloadable: false,
            }];
        let mut ammunition = item_leaf("battery", None);
        let cdda_protocol::ItemGroupTargetV1::Item(ammunition) = &mut ammunition else {
            unreachable!("ammunition fixture is a direct item")
        };
        ammunition.prototype.charges = 1;
        ammunition.prototype.ammunition_type = String::from("battery");
        ammunition.prototype.containment.count_by_charges = true;
        ammunition.prototype.containment.stack_size = 100;
        wearable_light_item.tool_charge_storage =
            Some(cdda_protocol::ItemGroupToolChargeStorageV1::Detachable {
                well_pocket_index: 0,
                magazine: magazine.prototype.clone(),
                ammunition: Box::new(ammunition.prototype.clone()),
            });
        let mut light_battery = item_leaf(
            "light_battery_cell",
            Some(cdda_protocol::ItemGroupChargeRangeV1 {
                minimum: 100,
                maximum: 100,
            }),
        );
        let cdda_protocol::ItemGroupTargetV1::Item(light_battery_item) = &mut light_battery else {
            unreachable!("light-battery fixture is a direct item")
        };
        light_battery_item.minimum_one_charge = false;
        light_battery_item.charge_capacity =
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage;
        light_battery_item.prototype.charges = 0;
        light_battery_item.prototype.integral_magazines =
            vec![cdda_protocol::IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("MAGAZINE"),
                ammunition_type: String::from("battery"),
                capacity: 16,
                rigid: true,
                reloadable: false,
                unloadable: false,
            }];
        light_battery_item.tool_charge_storage =
            Some(cdda_protocol::ItemGroupToolChargeStorageV1::Integral {
                ammunition: ammunition.prototype.clone(),
            });
        let mut eink_tablet = item_leaf(
            "eink_tablet_pc",
            Some(cdda_protocol::ItemGroupChargeRangeV1 {
                minimum: 0,
                maximum: -1,
            }),
        );
        let cdda_protocol::ItemGroupTargetV1::Item(eink_tablet_item) = &mut eink_tablet else {
            unreachable!("e-ink tablet fixture is a direct item")
        };
        eink_tablet_item.minimum_one_charge = false;
        eink_tablet_item.charge_capacity =
            cdda_protocol::ItemGroupChargeCapacityV1::AmmunitionStorage;
        eink_tablet_item.prototype.charges = 0;
        eink_tablet_item.prototype.containment.volume_milliliters = 137;
        eink_tablet_item.prototype.integral_magazines =
            vec![cdda_protocol::IntegralMagazinePocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("MAGAZINE"),
                ammunition_type: String::from("battery"),
                capacity: 85,
                rigid: true,
                reloadable: true,
                unloadable: true,
            }];
        eink_tablet_item.tool_charge_storage =
            Some(cdda_protocol::ItemGroupToolChargeStorageV1::Integral {
                ammunition: ammunition.prototype.clone(),
            });
        let mut aspirin = item_leaf("aspirin", None);
        let cdda_protocol::ItemGroupTargetV1::Item(aspirin_item) = &mut aspirin else {
            unreachable!("aspirin fixture is a direct item")
        };
        aspirin_item.prototype.containment.volume_milliliters = 1;
        aspirin_item.prototype.containment.weight_milligrams = 1_000;
        aspirin_item.prototype.comestible_type = String::from("MED");
        aspirin_item.prototype.tracks_temperature = true;
        let mut painkiller_bottle = item_leaf("bottle_plastic_pill_painkiller", None);
        let cdda_protocol::ItemGroupTargetV1::Item(painkiller_bottle) = &mut painkiller_bottle
        else {
            unreachable!("painkiller bottle fixture is a direct item")
        };
        painkiller_bottle.prototype.containment.volume_milliliters = 251;
        painkiller_bottle.prototype.containment.weight_milligrams = 7_000;
        painkiller_bottle.prototype.ammunition_containers =
            vec![cdda_protocol::AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("CONTAINER"),
                capacities: Vec::new(),
                rigid: true,
                access_moves: 400,
                reloadable: false,
                unloadable: true,
                spawn_rules: Some(cdda_protocol::SpawnPocketRulesV1 {
                    kind: cdda_protocol::SpawnPocketKindV1::Container,
                    max_contains_volume_milliliters: 250,
                    magazine_well_volume_milliliters: 0,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: 1_000_000,
                    max_item_volume_milliliters: 17,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: 170,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 400,
                    rigid: true,
                    watertight: true,
                    transparent: true,
                    forbidden: false,
                    sealable: true,
                }),
            }];
        aspirin_item.default_container = Some(cdda_protocol::ItemGroupContainerV1 {
            item: painkiller_bottle.clone(),
            variant_id: None,
            sealed: true,
            overflow: cdda_protocol::ItemGroupOverflowV1::None,
        });
        let mut caff_gum = item_leaf("caff_gum", None);
        let cdda_protocol::ItemGroupTargetV1::Item(caff_gum_item) = &mut caff_gum else {
            unreachable!("caffeine-gum fixture is a direct item")
        };
        caff_gum_item.prototype.comestible_type = String::from("MED");
        caff_gum_item.prototype.tracks_temperature = true;
        caff_gum_item.prototype.thermal_properties = Some(cdda_protocol::ItemThermalPropertiesV1 {
            specific_heat_liquid_microjoules_per_gram_kelvin: 1_500_000,
            specific_heat_solid_microjoules_per_gram_kelvin: 1_200_000,
            latent_heat_microjoules_per_gram: 10_000_000,
            freezing_point_millikelvin: 273_150,
        });
        let mut item_groups = vec![ItemGroupDefinitionV1 {
            group_id: String::from("wall_bash_results"),
            graph: cdda_protocol::ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![cdda_protocol::ItemGroupNodeV1 {
                    node_id: 0,
                    kind: cdda_protocol::ItemGroupKindV1::Collection,
                    entries: vec![
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: None,
                            variant_id: None,
                            event: Some(cdda_protocol::ItemGroupEventV1::Christmas),
                            target: item_leaf("holiday_token", None),
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 2,
                            count_max: 2,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: None,
                            event: None,
                            target: item_leaf("splinter", None),
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: None,
                            event: None,
                            target: item_leaf(
                                "nail",
                                Some(cdda_protocol::ItemGroupChargeRangeV1 {
                                    minimum: 4,
                                    maximum: 6,
                                }),
                            ),
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: None,
                            event: None,
                            target: wearable_light,
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: None,
                            event: None,
                            target: light_battery,
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                                minimum: 0,
                                maximum: 0,
                            }),
                            variant_id: None,
                            event: None,
                            target: eink_tablet,
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: None,
                            variant_id: None,
                            event: None,
                            target: aspirin,
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                        cdda_protocol::ItemGroupEntryV1 {
                            probability: 100,
                            count_min: 1,
                            count_max: 1,
                            raw_damage: None,
                            variant_id: None,
                            event: None,
                            target: caff_gum,
                            modifier_charges: None,
                            contents: Vec::new(),
                            seal_contents: false,
                            modifier_default_container_sealed: None,
                            direct_wrapper: None,
                            modifier_container: None,
                        },
                    ],
                }],
                wrapper: Some(cdda_protocol::ItemGroupContainerV1 {
                    item: wrapper_item,
                    variant_id: None,
                    sealed: true,
                    overflow: cdda_protocol::ItemGroupOverflowV1::Spill,
                }),
            },
        }];
        let splinter = &mut item_groups[0].graph.nodes[0].entries[1];
        splinter.raw_damage = Some(cdda_protocol::InclusiveU16RangeV1 {
            minimum: 1_000,
            maximum: 1_000,
        });
        splinter.variant_id = Some(String::from("weathered"));
        let cdda_protocol::ItemGroupTargetV1::Item(splinter_item) = &mut splinter.target else {
            unreachable!("splinter fixture is a direct item")
        };
        splinter_item.maximum_raw_damage = cdda_protocol::MAX_ITEM_RAW_DAMAGE;
        splinter_item.prototype.containment.flags = vec![String::from("VARSIZE")];
        splinter_item.variants = vec![cdda_protocol::ItemGroupVariantOptionV1 {
            variant: cdda_protocol::ItemVariantV1 {
                id: String::from("weathered"),
                name: String::from("weathered splinter"),
                description: String::from("A weathered splinter."),
                symbol: String::from("/"),
                color: String::from("brown"),
                ascii_picture: String::new(),
            },
            weight: 1,
            description_expansion: Some(cdda_protocol::ItemDescriptionExpansionV1 {
                template: String::from("A <condition> splinter."),
                categories: vec![cdda_protocol::ItemDescriptionSnippetCategoryV1 {
                    category: String::from("<condition>"),
                    choices: vec![cdda_protocol::ItemDescriptionSnippetChoiceV1 {
                        text: String::from("weathered"),
                        weight: 1,
                    }],
                }],
            }),
        }];
        splinter_item.snippets = vec![cdda_protocol::ItemSnippetV1 {
            id: String::from("salvage_note"),
            text: String::from("Recovered from the wall"),
        }];
        splinter_item.initial_variables.insert(
            String::from("browsed"),
            cdda_protocol::ItemVariableValueV1::String(String::from("false")),
        );
        let floor = TerrainTileSnapshot {
            terrain_id: String::from("t_floor"),
            move_cost: 2,
            transparent: true,
            flat: true,
            open: String::new(),
            open_move_cost: None,
            open_transparent: None,
            open_flat: None,
            close: String::new(),
            close_move_cost: None,
            close_transparent: None,
            close_flat: None,
        };
        let mut wall = floor.clone();
        wall.terrain_id = String::from("t_wall");
        wall.move_cost = 0;
        wall.transparent = false;
        wall.flat = false;
        let scenario = ScenarioV1 {
            format_version: SCENARIO_FORMAT_VERSION,
            protocol_version: cdda_protocol::PROTOCOL_VERSION,
            persistence_schema_version: SCHEMA_VERSION,
            replay_format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
            world_namespace: 905,
            world_seed: [29; 32],
            content_manifest_hash: [30; 32],
            enabled_mods: vec![String::from("dda")],
            item_groups,
            terrain_bash_types: vec![TerrainBashTypeV1 {
                terrain_id: String::from("t_wall"),
                str_min: 1,
                str_max: 5,
                str_min_blocked: -1,
                str_max_blocked: -1,
                str_min_supported: -1,
                str_max_supported: -1,
                bash_multiplier_millionths: 1_000_000,
                result: floor,
                drop_source: Some(cdda_protocol::ItemGroupSourceV1::Group(String::from(
                    "wall_bash_results",
                ))),
                hit_field: None,
                destroyed_field: None,
                sound: String::from("crash!"),
                failure_sound: String::from("whump!"),
                sound_volume: 12,
                failure_sound_volume: 8,
            }],
            smash_item_types: vec![SmashItemTypeV1 {
                item_type_id: String::from("hammer"),
                bash_damage: 12,
                attack_time_moves: 100,
                melee_to_hit: 0,
            }],
            worldgen: None,
            chunks: vec![ChunkCoord { x: 0, y: 0, z: 0 }],
            terrain: vec![ScenarioTerrainV1 {
                position: wall_position,
                terrain: wall,
            }],
            actors: vec![ScenarioActorV1 {
                alias: String::from("survivor"),
                spawn: ScenarioActorSpawnV1::At(position),
                connected: true,
                stats: CharacterCreationStatsV1::default(),
            }],
            ground_items: vec![ScenarioItemV1 {
                alias: String::from("hammer"),
                position,
                type_id: String::from("hammer"),
                charges: 1,
                melee_damage_milli: BTreeMap::from([(String::from("bash"), 12_000)]),
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
            }],
            generated_items: Vec::new(),
            steps: vec![
                ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command: ScenarioCommandV1::PickUp {
                        item: String::from("hammer"),
                    },
                },
                ScenarioStepV1::Advance { ticks: 25 },
                ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command: ScenarioCommandV1::Wield {
                        item: String::from("hammer"),
                    },
                },
                ScenarioStepV1::Advance { ticks: 25 },
                ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command: ScenarioCommandV1::Smash { dx: 1, dy: 0 },
                },
                ScenarioStepV1::Advance { ticks: 25 },
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
        assert!(
            cdda_protocol::item_group_catalog_is_valid(&scenario.item_groups),
            "detachable-tool conformance catalog should validate"
        );
        let direct = run_scenario(&scenario, ScenarioMode::Direct)
            .expect("named item-group bash should run directly");
        for mode in [
            ScenarioMode::SnapshotEachTick,
            ScenarioMode::SqliteRecovery,
            ScenarioMode::PortableReplay,
        ] {
            assert_eq!(
                run_scenario(&scenario, mode).expect("named bash scenario should replay"),
                direct,
                "{mode} must preserve item-group generation"
            );
        }
        assert_eq!(direct.final_snapshot.item_groups, scenario.item_groups);
        let [salvage_box] = direct.final_snapshot.ground_items.as_slice() else {
            panic!("the group wrapper should leave one top-level salvage box");
        };
        assert_eq!(salvage_box.item.type_id, "flexible_salvage_wrapper");
        let [pocket] = salvage_box.item.ammunition_containers.as_slice() else {
            panic!("the salvage box should retain one physical pocket");
        };
        let wrapper_state = pocket
            .spawn_state
            .as_ref()
            .expect("the flexible wrapper should retain spawn state");
        assert!(!pocket.rigid);
        assert_eq!(wrapper_state.rules.magazine_well_volume_milliliters, 45);
        assert!(!wrapper_state.contents_collapsed);
        assert!(
            wrapper_state.sealed,
            "the group-level seal must survive every recovery mode"
        );
        assert_eq!(pocket.contents.len(), 8);
        assert!(
            pocket
                .contents
                .iter()
                .all(|item| item.id > salvage_box.item.id),
            "preorder materialization must allocate the wrapper before every nested item"
        );
        let mut contained = pocket
            .contents
            .iter()
            .map(|item| (item.type_id.as_str(), item.charges))
            .collect::<Vec<_>>();
        contained.sort_unstable();
        assert_eq!(
            contained,
            [
                ("bottle_plastic_pill_painkiller", 1),
                ("caff_gum", 1),
                ("eink_tablet_pc", 0),
                ("light_battery_cell", 0),
                ("nail", 4),
                ("splinter", 1),
                ("splinter", 1),
                ("wearable_light", 0),
            ],
            "the fixed seed must include default containment, both constructor variant expansions, the explicit variant, fit, and wrapper phases"
        );
        let headlamp = pocket
            .contents
            .iter()
            .find(|item| item.type_id == "wearable_light")
            .expect("the generalized tool-charge path should generate a headlamp");
        let installed = headlamp.magazine_wells[0]
            .installed_magazine
            .as_deref()
            .expect("the headlamp should retain its default magazine");
        let battery = installed.integral_magazines[0]
            .loaded_ammunition
            .as_deref()
            .expect("the default magazine should retain clamped battery charge");
        assert_eq!(installed.type_id, "medium_battery_cell");
        assert_eq!(battery.type_id, "battery");
        assert_eq!(battery.charges, 56);
        assert!(headlamp.id < installed.id && installed.id < battery.id);
        let light_battery = pocket
            .contents
            .iter()
            .find(|item| item.type_id == "light_battery_cell")
            .expect("the generalized integral-ammunition path should generate a light battery");
        let light_ammunition = light_battery.integral_magazines[0]
            .loaded_ammunition
            .as_deref()
            .expect("the light battery should retain clamped ammunition");
        assert_eq!(light_ammunition.type_id, "battery");
        assert_eq!(light_ammunition.charges, 16);
        assert!(light_battery.id < light_ammunition.id);
        let eink_tablet = pocket
            .contents
            .iter()
            .find(|item| item.type_id == "eink_tablet_pc")
            .expect("the integral-tool sentinel path should generate an e-ink tablet");
        let tablet_ammunition = eink_tablet.integral_magazines[0]
            .loaded_ammunition
            .as_deref()
            .expect("the capacity-resolved sentinel should construct integral ammunition");
        assert_eq!(tablet_ammunition.type_id, "battery");
        assert!((1..=85).contains(&tablet_ammunition.charges));
        assert!(eink_tablet.id < tablet_ammunition.id);
        let painkiller_bottle = pocket
            .contents
            .iter()
            .find(|item| item.type_id == "bottle_plastic_pill_painkiller")
            .expect("the generalized default-container path should generate a bottle");
        let [painkiller_pocket] = painkiller_bottle.ammunition_containers.as_slice() else {
            panic!("the painkiller bottle should retain one physical pocket");
        };
        let [aspirin] = painkiller_pocket.contents.as_slice() else {
            panic!("the painkiller bottle should own one aspirin");
        };
        assert!(
            painkiller_pocket
                .spawn_state
                .as_ref()
                .is_some_and(|state| state.contents_collapsed),
            "homogeneous auto-wrapped contents must remain collapsed in every recovery mode"
        );
        assert_eq!(aspirin.type_id, "aspirin");
        assert_eq!(
            aspirin.temperature,
            Some(cdda_protocol::initial_item_temperature_state(
                SimTick(53),
                cdda_protocol::ItemPhaseV1::Solid,
                None,
            )),
            "all four modes should retain the exact nested constructor state and birth tick"
        );
        assert!(painkiller_bottle.id < aspirin.id);
        let caff_gum = pocket
            .contents
            .iter()
            .find(|item| item.type_id == "caff_gum")
            .expect("the material-backed constructor should generate caffeine gum");
        let caff_state = caff_gum
            .temperature
            .expect("caffeine gum should retain temperature state");
        assert_eq!(
            caff_state,
            cdda_protocol::initial_item_temperature_state(
                SimTick(53),
                cdda_protocol::ItemPhaseV1::Solid,
                Some(cdda_protocol::ItemThermalPropertiesV1 {
                    specific_heat_liquid_microjoules_per_gram_kelvin: 1_500_000,
                    specific_heat_solid_microjoules_per_gram_kelvin: 1_200_000,
                    latent_heat_microjoules_per_gram: 10_000_000,
                    freezing_point_millikelvin: 273_150,
                }),
            ),
            "direct, snapshot, SQLite, and portable replay must retain the exact material profile"
        );
        let splinters = pocket
            .contents
            .iter()
            .filter(|item| item.type_id == "splinter")
            .collect::<Vec<_>>();
        assert_eq!(splinters.len(), 2);
        assert!(splinters.iter().all(|item| {
            item.raw_damage == 1_000
                && item.damage == 2
                && item.containment.flags == ["VARSIZE"]
                && item
                    .variant
                    .as_ref()
                    .is_some_and(|variant| variant.id == "weathered")
                && item
                    .snippet
                    .as_ref()
                    .is_some_and(|snippet| snippet.id == "salvage_note")
                && item.variables.get("browsed")
                    == Some(&cdda_protocol::ItemVariableValueV1::String(String::from(
                        "false",
                    )))
                && item.variables.get("description")
                    == Some(&cdda_protocol::ItemVariableValueV1::String(String::from(
                        "A weathered splinter.",
                    )))
        }));
        let (coord, local) = wall_position.chunk_and_local();
        let chunk = direct
            .final_snapshot
            .chunks
            .iter()
            .find(|chunk| chunk.coord == coord)
            .expect("target chunk should remain");
        let index = usize::from(local.y)
            * usize::try_from(cdda_protocol::SUBMAP_SIZE).expect("submap size fits")
            + usize::from(local.x);
        assert_eq!(chunk.tiles[index].terrain_id, "t_floor");
        assert!(direct.event_batches.iter().any(|batch| {
            batch.events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::ActorBashed { success: true, .. }
                )
            })
        }));
    }

    #[test]
    fn indexed_multi_well_reload_is_identical_across_sqlite_and_portable_replay() {
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let item = |alias: &str,
                    type_id: &str,
                    charges: i32,
                    ammunition_type: &str,
                    magazine_capacity: u32,
                    magazine_wells: Vec<MagazineWellPrototypeV1>,
                    powered_tool: Option<PoweredToolStateV1>| ScenarioItemV1 {
            alias: alias.to_owned(),
            position,
            type_id: type_id.to_owned(),
            charges,
            melee_damage_milli: BTreeMap::new(),
            calories: 0,
            quench: 0,
            comestible_type: String::new(),
            ammunition_type: ammunition_type.to_owned(),
            ranged_weapon: None,
            magazine_capacity,
            integral_magazines: Vec::new(),
            magazine_wells,
            ammunition_containers: Vec::new(),
            residual_energy_millijoules: 0,
            powered_tool,
        };
        let commands = [
            ScenarioCommandV1::PickUp {
                item: String::from("tool"),
            },
            ScenarioCommandV1::PickUp {
                item: String::from("primary"),
            },
            ScenarioCommandV1::PickUp {
                item: String::from("auxiliary"),
            },
            ScenarioCommandV1::Wield {
                item: String::from("tool"),
            },
            ScenarioCommandV1::Reload {
                ammunition: String::from("auxiliary"),
                target_pocket_index: Some(4),
            },
            ScenarioCommandV1::Reload {
                ammunition: String::from("primary"),
                target_pocket_index: Some(1),
            },
        ];
        let mut steps = Vec::new();
        for command in commands {
            steps.push(ScenarioStepV1::Command {
                actor: String::from("survivor"),
                command,
            });
            steps.push(ScenarioStepV1::Advance { ticks: 25 });
        }
        let scenario = ScenarioV1 {
            format_version: SCENARIO_FORMAT_VERSION,
            protocol_version: cdda_protocol::PROTOCOL_VERSION,
            persistence_schema_version: SCHEMA_VERSION,
            replay_format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
            world_namespace: 901,
            world_seed: [10; 32],
            content_manifest_hash: [8; 32],
            enabled_mods: vec![String::from("dda")],
            item_groups: Vec::new(),
            terrain_bash_types: Vec::new(),
            smash_item_types: Vec::new(),
            worldgen: None,
            chunks: vec![ChunkCoord { x: 0, y: 0, z: 0 }],
            terrain: Vec::new(),
            actors: vec![ScenarioActorV1 {
                alias: String::from("survivor"),
                spawn: ScenarioActorSpawnV1::At(position),
                connected: true,
                stats: CharacterCreationStatsV1::default(),
            }],
            ground_items: vec![
                item(
                    "tool",
                    "dual_tool",
                    0,
                    "",
                    0,
                    vec![
                        MagazineWellPrototypeV1 {
                            pocket_index: 1,
                            pocket_id: String::from("PRIMARY"),
                            compatible_magazine_type_ids: vec![String::from("light_battery")],
                            rigid: true,
                            unloadable: true,
                        },
                        MagazineWellPrototypeV1 {
                            pocket_index: 4,
                            pocket_id: String::from("AUXILIARY"),
                            compatible_magazine_type_ids: vec![String::from("heavy_battery")],
                            rigid: true,
                            unloadable: true,
                        },
                    ],
                    Some(PoweredToolStateV1 {
                        inactive_type_id: String::from("dual_tool"),
                        active_type_id: String::from("dual_tool_on"),
                        activation_charges: 1,
                        power_draw_milliwatts: 1_000,
                        light_emission: 4,
                        dims_with_charge: false,
                        power_pocket_index: 1,
                        active: false,
                    }),
                ),
                item(
                    "primary",
                    "light_battery",
                    3,
                    "battery",
                    10,
                    Vec::new(),
                    None,
                ),
                item(
                    "auxiliary",
                    "heavy_battery",
                    4,
                    "battery",
                    10,
                    Vec::new(),
                    None,
                ),
            ],
            generated_items: Vec::new(),
            steps,
            expected: ScenarioExpectationV1 {
                final_tick: SimTick(0),
                final_state_hash: [0; 32],
                event_trace_hash: [0; 32],
                actors: vec![ScenarioActorExpectationV1 {
                    actor: String::from("survivor"),
                    position,
                    connected: true,
                    inventory: vec![String::from("tool")],
                    wielded: Some(String::from("tool")),
                }],
                ground_items: Vec::new(),
                event_batches: None,
            },
        };
        let direct = run_scenario(&scenario, ScenarioMode::Direct)
            .expect("multi-well scenario should run directly");
        for mode in [
            ScenarioMode::SnapshotEachTick,
            ScenarioMode::SqliteRecovery,
            ScenarioMode::PortableReplay,
        ] {
            assert_eq!(
                run_scenario(&scenario, mode).expect("multi-well scenario should replay"),
                direct,
                "{mode} must preserve indexed nested items"
            );
        }
        let tool = direct
            .final_snapshot
            .actors
            .first()
            .and_then(|actor| actor.inventory.first())
            .expect("tool should remain wielded");
        assert_eq!(
            tool.magazine_wells
                .iter()
                .map(|well| {
                    (
                        well.pocket_index,
                        well.pocket_id.as_str(),
                        well.installed_magazine
                            .as_deref()
                            .map(|magazine| (magazine.type_id.as_str(), magazine.charges)),
                    )
                })
                .collect::<Vec<_>>(),
            [
                (1, "PRIMARY", Some(("light_battery", 3))),
                (4, "AUXILIARY", Some(("heavy_battery", 4))),
            ]
        );
    }

    #[test]
    fn item_backed_ammunition_split_is_identical_across_recovery_and_replay() {
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let commands = [
            ScenarioCommandV1::PickUp {
                item: String::from("cell"),
            },
            ScenarioCommandV1::PickUp {
                item: String::from("ammunition"),
            },
            ScenarioCommandV1::Wield {
                item: String::from("cell"),
            },
            ScenarioCommandV1::Reload {
                ammunition: String::from("ammunition"),
                target_pocket_index: Some(3),
            },
        ];
        let mut steps = Vec::new();
        for command in commands {
            steps.push(ScenarioStepV1::Command {
                actor: String::from("survivor"),
                command,
            });
            steps.push(ScenarioStepV1::Advance { ticks: 25 });
        }
        let scenario = ScenarioV1 {
            format_version: SCENARIO_FORMAT_VERSION,
            protocol_version: cdda_protocol::PROTOCOL_VERSION,
            persistence_schema_version: SCHEMA_VERSION,
            replay_format_version: REPLAY_FORMAT_VERSION,
            baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
            world_namespace: 902,
            world_seed: [11; 32],
            content_manifest_hash: [9; 32],
            enabled_mods: vec![String::from("dda")],
            item_groups: Vec::new(),
            terrain_bash_types: Vec::new(),
            smash_item_types: Vec::new(),
            worldgen: None,
            chunks: vec![ChunkCoord { x: 0, y: 0, z: 0 }],
            terrain: Vec::new(),
            actors: vec![ScenarioActorV1 {
                alias: String::from("survivor"),
                spawn: ScenarioActorSpawnV1::At(position),
                connected: true,
                stats: CharacterCreationStatsV1::default(),
            }],
            ground_items: vec![
                ScenarioItemV1 {
                    alias: String::from("cell"),
                    position,
                    type_id: String::from("test_cell"),
                    charges: 0,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::new(),
                    ranged_weapon: None,
                    magazine_capacity: 0,
                    integral_magazines: vec![IntegralMagazinePocketPrototypeV1 {
                        pocket_index: 3,
                        pocket_id: String::from("PRIMARY"),
                        ammunition_type: String::from("battery"),
                        capacity: 6,
                        rigid: true,
                        reloadable: true,
                        unloadable: true,
                    }],
                    magazine_wells: Vec::new(),
                    ammunition_containers: Vec::new(),
                    residual_energy_millijoules: 0,
                    powered_tool: None,
                },
                ScenarioItemV1 {
                    alias: String::from("ammunition"),
                    position,
                    type_id: String::from("battery"),
                    charges: 10,
                    melee_damage_milli: BTreeMap::new(),
                    calories: 0,
                    quench: 0,
                    comestible_type: String::new(),
                    ammunition_type: String::from("battery"),
                    ranged_weapon: None,
                    magazine_capacity: 0,
                    integral_magazines: Vec::new(),
                    magazine_wells: Vec::new(),
                    ammunition_containers: Vec::new(),
                    residual_energy_millijoules: 0,
                    powered_tool: None,
                },
            ],
            generated_items: Vec::new(),
            steps,
            expected: ScenarioExpectationV1 {
                final_tick: SimTick(0),
                final_state_hash: [0; 32],
                event_trace_hash: [0; 32],
                actors: Vec::new(),
                ground_items: Vec::new(),
                event_batches: None,
            },
        };
        let direct = run_scenario(&scenario, ScenarioMode::Direct)
            .expect("item-backed scenario should run directly");
        for mode in [
            ScenarioMode::SnapshotEachTick,
            ScenarioMode::SqliteRecovery,
            ScenarioMode::PortableReplay,
        ] {
            assert_eq!(
                run_scenario(&scenario, mode).expect("item-backed scenario should replay"),
                direct,
                "{mode} must preserve split ammunition identity"
            );
        }
        let actor = direct
            .final_snapshot
            .actors
            .first()
            .expect("actor should remain");
        let cell = actor
            .inventory
            .iter()
            .find(|item| item.type_id == "test_cell")
            .expect("cell should remain");
        let source = actor
            .inventory
            .iter()
            .find(|item| item.type_id == "battery")
            .expect("partial source should remain");
        let nested = cell.integral_magazines[0]
            .loaded_ammunition
            .as_deref()
            .expect("split stack should be nested");
        assert_eq!(nested.charges, 6);
        assert_eq!(source.charges, 4);
        assert_ne!(nested.id, source.id);
        assert!(direct.event_batches.iter().any(|batch| {
            batch.events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::AmmunitionLoadedIntoPocket {
                        loaded: 6,
                        pocket_ammunition: 6,
                        source_charges_remaining: 4,
                        ..
                    }
                )
            })
        }));

        let mut round_trip = scenario;
        round_trip
            .ground_items
            .iter_mut()
            .find(|item| item.alias == "ammunition")
            .expect("ammunition fixture should exist")
            .charges = 6;
        round_trip.steps.push(ScenarioStepV1::Command {
            actor: String::from("survivor"),
            command: ScenarioCommandV1::RemovePocketItem {
                owner_item: String::from("cell"),
                pocket_index: 3,
                contained_item: String::from("ammunition"),
            },
        });
        round_trip.steps.push(ScenarioStepV1::Advance { ticks: 25 });
        let direct_round_trip = run_scenario(&round_trip, ScenarioMode::Direct)
            .expect("whole-stack pocket round trip should run directly");
        for mode in [
            ScenarioMode::SnapshotEachTick,
            ScenarioMode::SqliteRecovery,
            ScenarioMode::PortableReplay,
        ] {
            assert_eq!(
                run_scenario(&round_trip, mode).expect("pocket removal should replay"),
                direct_round_trip,
                "{mode} must preserve whole-stack removal identity"
            );
        }
        let actor = direct_round_trip
            .final_snapshot
            .actors
            .first()
            .expect("round-trip actor should remain");
        assert!(
            actor
                .inventory
                .iter()
                .find(|item| item.type_id == "test_cell")
                .expect("round-trip cell should remain")
                .integral_magazines[0]
                .loaded_ammunition
                .is_none()
        );
        let ammunition = actor
            .inventory
            .iter()
            .find(|item| item.type_id == "battery")
            .expect("whole stack should return to inventory");
        assert_eq!(ammunition.charges, 6);
        assert!(direct_round_trip.event_batches.iter().any(|batch| {
            batch.events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::PocketItemRemoved {
                        contained_item,
                        charges: 6,
                        ..
                    } if contained_item == ammunition.id
                )
            })
        }));
    }

    #[test]
    fn ammunition_containers_are_identical_across_recovery_and_replay() {
        let position = WorldPosition { x: 1, y: 1, z: 0 };
        let ordinary =
            |alias: &str, type_id: &str, charges: i32, ammunition_type: &str| ScenarioItemV1 {
                alias: alias.to_owned(),
                position,
                type_id: type_id.to_owned(),
                charges,
                melee_damage_milli: BTreeMap::new(),
                calories: 0,
                quench: 0,
                comestible_type: String::new(),
                ammunition_type: ammunition_type.to_owned(),
                ranged_weapon: None,
                magazine_capacity: 0,
                integral_magazines: Vec::new(),
                magazine_wells: Vec::new(),
                ammunition_containers: Vec::new(),
                residual_energy_millijoules: 0,
                powered_tool: None,
            };
        let quiver = || {
            let mut item = ordinary("quiver", "quiver", 1, "");
            item.ammunition_containers = vec![AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 2,
                pocket_id: String::from("QUIVER"),
                capacities: vec![
                    cdda_protocol::AmmunitionCapacityV1 {
                        ammunition_type: String::from("arrow"),
                        capacity: 6,
                    },
                    cdda_protocol::AmmunitionCapacityV1 {
                        ammunition_type: String::from("bolt"),
                        capacity: 4,
                    },
                ],
                rigid: false,
                access_moves: 20,
                reloadable: true,
                unloadable: true,
                spawn_rules: None,
            }];
            item
        };
        let scenario = |world_namespace: u64,
                        ground_items: Vec<ScenarioItemV1>,
                        commands: Vec<ScenarioCommandV1>| {
            let mut steps = Vec::new();
            for command in commands {
                steps.push(ScenarioStepV1::Command {
                    actor: String::from("survivor"),
                    command,
                });
                steps.push(ScenarioStepV1::Advance { ticks: 25 });
            }
            ScenarioV1 {
                format_version: SCENARIO_FORMAT_VERSION,
                protocol_version: cdda_protocol::PROTOCOL_VERSION,
                persistence_schema_version: SCHEMA_VERSION,
                replay_format_version: REPLAY_FORMAT_VERSION,
                baseline_commit: String::from(cdda_protocol::BASELINE_COMMIT),
                world_namespace,
                world_seed: [21; 32],
                content_manifest_hash: [22; 32],
                enabled_mods: vec![String::from("dda")],
                item_groups: Vec::new(),
                terrain_bash_types: Vec::new(),
                smash_item_types: Vec::new(),
                worldgen: None,
                chunks: vec![ChunkCoord { x: 0, y: 0, z: 0 }],
                terrain: Vec::new(),
                actors: vec![ScenarioActorV1 {
                    alias: String::from("survivor"),
                    spawn: ScenarioActorSpawnV1::At(position),
                    connected: true,
                    stats: CharacterCreationStatsV1::default(),
                }],
                ground_items,
                generated_items: Vec::new(),
                steps,
                expected: ScenarioExpectationV1 {
                    final_tick: SimTick(0),
                    final_state_hash: [0; 32],
                    event_trace_hash: [0; 32],
                    actors: Vec::new(),
                    ground_items: Vec::new(),
                    event_batches: None,
                },
            }
        };
        let assert_all_modes = |scenario: &ScenarioV1| {
            let direct = run_scenario(scenario, ScenarioMode::Direct)
                .expect("container scenario should run directly");
            for mode in [
                ScenarioMode::SnapshotEachTick,
                ScenarioMode::SqliteRecovery,
                ScenarioMode::PortableReplay,
            ] {
                assert_eq!(
                    run_scenario(scenario, mode).expect("container scenario should replay"),
                    direct,
                    "{mode} must preserve container state and events"
                );
            }
            direct
        };

        let partial = scenario(
            903,
            vec![quiver(), ordinary("arrows", "arrow_wood", 10, "arrow")],
            vec![
                ScenarioCommandV1::PickUp {
                    item: String::from("quiver"),
                },
                ScenarioCommandV1::PickUp {
                    item: String::from("arrows"),
                },
                ScenarioCommandV1::InsertPocketItem {
                    owner_item: String::from("quiver"),
                    pocket_index: 2,
                    source_item: String::from("arrows"),
                },
            ],
        );
        let direct = assert_all_modes(&partial);
        let actor = direct.final_snapshot.actors.first().expect("actor remains");
        let quiver_item = actor
            .inventory
            .iter()
            .find(|item| item.type_id == "quiver")
            .expect("quiver remains");
        let source = actor
            .inventory
            .iter()
            .find(|item| item.type_id == "arrow_wood")
            .expect("partial source remains");
        let nested = &quiver_item.ammunition_containers[0].contents[0];
        assert_eq!((nested.charges, source.charges), (6, 4));
        assert_ne!(nested.id, source.id);
        assert!(direct.event_batches.iter().any(|batch| {
            batch.events.iter().any(|event| {
                matches!(
                    event.kind,
                    WorldEventKind::AmmunitionInsertedIntoContainer {
                        transferred: 6,
                        pocket_ammunition: 6,
                        source_charges_remaining: 4,
                        ..
                    }
                )
            })
        }));

        let category_switch = scenario(
            904,
            vec![
                quiver(),
                ordinary("wood", "arrow_wood", 2, "arrow"),
                ordinary("metal", "arrow_metal", 4, "arrow"),
                ordinary("bolts", "bolt_wood", 3, "bolt"),
            ],
            vec![
                ScenarioCommandV1::PickUp {
                    item: String::from("quiver"),
                },
                ScenarioCommandV1::PickUp {
                    item: String::from("wood"),
                },
                ScenarioCommandV1::PickUp {
                    item: String::from("metal"),
                },
                ScenarioCommandV1::PickUp {
                    item: String::from("bolts"),
                },
                ScenarioCommandV1::InsertPocketItem {
                    owner_item: String::from("quiver"),
                    pocket_index: 2,
                    source_item: String::from("wood"),
                },
                ScenarioCommandV1::InsertPocketItem {
                    owner_item: String::from("quiver"),
                    pocket_index: 2,
                    source_item: String::from("metal"),
                },
                ScenarioCommandV1::RemovePocketItem {
                    owner_item: String::from("quiver"),
                    pocket_index: 2,
                    contained_item: String::from("wood"),
                },
                ScenarioCommandV1::RemovePocketItem {
                    owner_item: String::from("quiver"),
                    pocket_index: 2,
                    contained_item: String::from("metal"),
                },
                ScenarioCommandV1::InsertPocketItem {
                    owner_item: String::from("quiver"),
                    pocket_index: 2,
                    source_item: String::from("bolts"),
                },
            ],
        );
        let direct = assert_all_modes(&category_switch);
        let actor = direct.final_snapshot.actors.first().expect("actor remains");
        let pocket = &actor
            .inventory
            .iter()
            .find(|item| item.type_id == "quiver")
            .expect("quiver remains")
            .ammunition_containers[0];
        assert_eq!(pocket.contents.len(), 1);
        assert_eq!(pocket.contents[0].type_id, "bolt_wood");
        assert_eq!(pocket.contents[0].charges, 3);
        assert!(
            actor
                .inventory
                .iter()
                .any(|item| item.type_id == "arrow_wood" && item.charges == 2)
        );
        assert!(
            actor
                .inventory
                .iter()
                .any(|item| item.type_id == "arrow_metal" && item.charges == 4)
        );
    }

    #[test]
    fn scenario_json_is_versioned_and_rejects_unknown_aliases() {
        let scenario = item_flow_scenario();
        let encoded = serde_json::to_vec(&scenario).expect("scenario should encode");
        assert_eq!(
            serde_json::from_slice::<ScenarioV1>(&encoded).expect("scenario should decode"),
            scenario
        );
        let mut unknown_field = serde_json::to_value(&scenario).expect("scenario should encode");
        unknown_field
            .as_object_mut()
            .expect("scenario is an object")
            .insert(String::from("protcol_version"), serde_json::json!(75));
        assert!(serde_json::from_value::<ScenarioV1>(unknown_field).is_err());
        let mut invalid = scenario;
        invalid.steps[0] = ScenarioStepV1::Command {
            actor: String::from("survivor"),
            command: ScenarioCommandV1::PickUp {
                item: String::from("missing"),
            },
        };
        assert!(matches!(
            run_scenario(&invalid, ScenarioMode::Direct),
            Err(ConformanceError::UnknownItem(alias)) if alias == "missing"
        ));
    }

    #[test]
    fn scenario_bounds_reject_zero_tick_and_duplicate_aliases() {
        let mut invalid = item_flow_scenario();
        invalid.steps = vec![ScenarioStepV1::Advance { ticks: 0 }];
        assert!(matches!(
            run_scenario(&invalid, ScenarioMode::Direct),
            Err(ConformanceError::InvalidScenario(_))
        ));
        let mut invalid = item_flow_scenario();
        invalid.steps = vec![ScenarioStepV1::AdvanceBatch { ticks: 0 }];
        assert!(matches!(
            run_scenario(&invalid, ScenarioMode::Direct),
            Err(ConformanceError::InvalidScenario(_))
        ));
        let mut invalid = item_flow_scenario();
        invalid.ground_items[0].alias = String::from("survivor");
        assert!(matches!(
            run_scenario(&invalid, ScenarioMode::Direct),
            Err(ConformanceError::InvalidScenario(_))
        ));
        let mut invalid = item_flow_scenario();
        invalid.actors[0].spawn = ScenarioActorSpawnV1::StartLocation;
        assert!(matches!(
            run_scenario(&invalid, ScenarioMode::Direct),
            Err(ConformanceError::InvalidScenario(_))
        ));
    }

    #[test]
    fn declared_snapshot_boundaries_preserve_the_complete_journal_trace() {
        let mut scenario = item_flow_scenario();
        for step in &mut scenario.steps {
            if let ScenarioStepV1::Advance { ticks } = step {
                *step = ScenarioStepV1::AdvanceBatch { ticks: *ticks };
            }
        }
        assert_eq!(
            run_scenario(&scenario, ScenarioMode::SnapshotEachTick)
                .expect("declared snapshot boundaries should recover"),
            run_scenario(&scenario, ScenarioMode::Direct)
                .expect("batched journal trace should run directly")
        );
    }

    #[test]
    fn action_point_scale_is_the_pinned_twenty_per_move() {
        assert_eq!(cdda_protocol::ACTION_POINTS_PER_UPSTREAM_MOVE, 20);
    }
}
