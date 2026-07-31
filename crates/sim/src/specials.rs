use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ChunkCoord, MAX_WORLDGEN_SPECIAL_PLACEMENTS, WORLDGEN_OVERMAP_HEIGHT, WORLDGEN_OVERMAP_WIDTH,
    WorldgenCityV1, WorldgenOmtIdentityV1, WorldgenOvermapLayerV1, WorldgenOvermapLayoutV1,
    WorldgenOvermapRunV1, WorldgenSpecialId, WorldgenSpecialPlacementV1,
    WorldgenSpecialPopulationV1, WorldgenSpecialUniquenessV1,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use crate::{OvermapRoadBoundary, SimError};

const SPECIAL_SECTOR_WIDTH: i32 = 15;
const ATTEMPTS_PER_SECTOR: usize = 10;
const MAX_SPECIAL_PARTS: usize = 4_096;
const MAX_SPECIAL_CONNECTIONS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OvermapSpecialInterval {
    pub minimum: i32,
    pub maximum: i32,
}

impl OvermapSpecialInterval {
    const fn contains(self, value: i32) -> bool {
        value >= self.minimum && value <= self.maximum
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapFixedSpecialTerrain {
    pub offset: ChunkCoord,
    /// One concrete peer for nonrotating terrain, or four clockwise peers for
    /// terrain that rotates with the special.
    pub rotated_identities: Vec<WorldgenOmtIdentityV1>,
    pub allowed_location_types: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapFixedSpecialConnection {
    pub offset: ChunkCoord,
    pub from: Option<ChunkCoord>,
    pub terrain_type: String,
    pub connection_id: String,
    pub existing: bool,
    /// Finalized union of every location accepted by this connection subtype.
    pub allowed_location_types: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapFixedSpecial {
    pub special_id: String,
    pub terrains: Vec<OvermapFixedSpecialTerrain>,
    pub connections: Vec<OvermapFixedSpecialConnection>,
    pub city_sizes: OvermapSpecialInterval,
    pub city_distance: OvermapSpecialInterval,
    pub occurrences: OvermapSpecialInterval,
    pub priority: i32,
    pub rotate: bool,
    pub uniqueness: WorldgenSpecialUniquenessV1,
    pub population: Option<WorldgenSpecialPopulationV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OvermapSpecialRoadAnchor {
    pub endpoint: ChunkCoord,
    pub target: ChunkCoord,
    pub initial_direction: Option<OvermapRoadBoundary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapSpecialPlacementResult {
    pub layout: WorldgenOvermapLayoutV1,
    pub placements: Vec<WorldgenSpecialPlacementV1>,
    pub road_anchors: Vec<OvermapSpecialRoadAnchor>,
}

struct PlacementState {
    placed: usize,
    minimum: usize,
    maximum: usize,
}

/// Places the pinned fixed-special family into one coordinate-owned overmap.
/// All required predicates are tested before any cell is changed. Mutable
/// specials, EOCs, camps, and non-admitted connection engines
/// never reach this boundary.
pub fn place_overmap_specials(
    world_seed: [u8; 32],
    generator_version: u16,
    layout: WorldgenOvermapLayoutV1,
    cities: &[WorldgenCityV1],
    definitions: &[OvermapFixedSpecial],
    default_below: WorldgenOmtIdentityV1,
    default_above: WorldgenOmtIdentityV1,
    already_global_unique: &BTreeSet<String>,
) -> Result<OvermapSpecialPlacementResult, SimError> {
    validate_definitions(definitions)?;
    let mut known_identities = layout
        .identities
        .iter()
        .cloned()
        .map(|identity| (identity.full_id.clone(), identity))
        .collect::<BTreeMap<_, _>>();
    install_known_identity(&mut known_identities, default_below.clone())?;
    install_known_identity(&mut known_identities, default_above.clone())?;
    for definition in definitions {
        for terrain in &definition.terrains {
            for identity in &terrain.rotated_identities {
                install_known_identity(&mut known_identities, identity.clone())?;
            }
        }
    }
    let mut layers = expand_layers(&layout)?;
    let mut rng = special_rng(
        world_seed,
        generator_version,
        layout.origin_x,
        layout.origin_y,
    );
    let mut states = definitions
        .iter()
        .map(|definition| placement_state(definition, already_global_unique, &mut rng))
        .collect::<Result<Vec<_>, _>>()?;
    let mut sectors = (0..i32::from(WORLDGEN_OVERMAP_WIDTH))
        .step_by(usize::try_from(SPECIAL_SECTOR_WIDTH).map_err(|_| SimError::NumericOverflow)?)
        .flat_map(|x| {
            (0..i32::from(WORLDGEN_OVERMAP_HEIGHT))
                .step_by(usize::try_from(SPECIAL_SECTOR_WIDTH).unwrap_or(1))
                .map(move |y| (x, y))
        })
        .collect::<Vec<_>>();
    let mut placements = Vec::new();
    let mut road_anchors = Vec::new();
    let mut placed_unique = BTreeSet::new();

    for mandatory in [true, false] {
        shuffle(&mut sectors, &mut rng)?;
        let mut remaining = Vec::new();
        for sector in sectors {
            let mut placed_in_sector = false;
            for _ in 0..ATTEMPTS_PER_SECTOR {
                let local_x = inclusive_i32(
                    &mut rng,
                    sector.0,
                    (sector.0 + SPECIAL_SECTOR_WIDTH - 1)
                        .min(i32::from(WORLDGEN_OVERMAP_WIDTH) - 1),
                )?;
                let local_y = inclusive_i32(
                    &mut rng,
                    sector.1,
                    (sector.1 + SPECIAL_SECTOR_WIDTH - 1)
                        .min(i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1),
                )?;
                let origin = ChunkCoord {
                    x: layout
                        .origin_x
                        .checked_add(local_x)
                        .ok_or(SimError::NumericOverflow)?,
                    y: layout
                        .origin_y
                        .checked_add(local_y)
                        .ok_or(SimError::NumericOverflow)?,
                    z: 0,
                };
                let mut candidate_order = (0..definitions.len()).collect::<Vec<_>>();
                shuffle(&mut candidate_order, &mut rng)?;
                candidate_order
                    .sort_by_key(|index| std::cmp::Reverse(definitions[*index].priority));
                for index in candidate_order {
                    let state = states.get(index).ok_or(SimError::InvalidTerrain)?;
                    if state.placed >= state.maximum || mandatory && state.placed >= state.minimum {
                        continue;
                    }
                    let definition = definitions.get(index).ok_or(SimError::InvalidTerrain)?;
                    if definition.uniqueness != WorldgenSpecialUniquenessV1::None
                        && placed_unique.contains(&definition.special_id)
                    {
                        continue;
                    }
                    let Some(city) = nearest_eligible_city(definition, cities, origin) else {
                        if special_requires_city(definition) {
                            continue;
                        }
                        let Some(rotation) = choose_rotation(
                            definition,
                            &layout,
                            &layers,
                            &known_identities,
                            &default_below,
                            &default_above,
                            origin,
                            &mut rng,
                        )?
                        else {
                            continue;
                        };
                        apply_placement(
                            definition,
                            &layout,
                            &mut layers,
                            &default_below,
                            &default_above,
                            origin,
                            rotation,
                            None,
                            &mut placements,
                            &mut road_anchors,
                        )?;
                        states[index].placed += 1;
                        if definition.uniqueness != WorldgenSpecialUniquenessV1::None {
                            placed_unique.insert(definition.special_id.clone());
                        }
                        placed_in_sector = true;
                        break;
                    };
                    let Some(rotation) = choose_rotation(
                        definition,
                        &layout,
                        &layers,
                        &known_identities,
                        &default_below,
                        &default_above,
                        origin,
                        &mut rng,
                    )?
                    else {
                        continue;
                    };
                    apply_placement(
                        definition,
                        &layout,
                        &mut layers,
                        &default_below,
                        &default_above,
                        origin,
                        rotation,
                        Some(city),
                        &mut placements,
                        &mut road_anchors,
                    )?;
                    states[index].placed += 1;
                    if definition.uniqueness != WorldgenSpecialUniquenessV1::None {
                        placed_unique.insert(definition.special_id.clone());
                    }
                    placed_in_sector = true;
                    break;
                }
                if placed_in_sector {
                    break;
                }
            }
            if !placed_in_sector {
                remaining.push(sector);
            }
        }
        sectors = remaining;
    }

    let layout = encode_layout(layout, layers, known_identities)?;
    if placements.len() > MAX_WORLDGEN_SPECIAL_PLACEMENTS {
        return Err(SimError::InvalidTerrain);
    }
    Ok(OvermapSpecialPlacementResult {
        layout,
        placements,
        road_anchors,
    })
}

fn placement_state(
    definition: &OvermapFixedSpecial,
    already_global_unique: &BTreeSet<String>,
    rng: &mut ChaCha8Rng,
) -> Result<PlacementState, SimError> {
    let sector_count = usize::from(WORLDGEN_OVERMAP_WIDTH / SPECIAL_SECTOR_WIDTH as u16)
        * usize::from(WORLDGEN_OVERMAP_HEIGHT / SPECIAL_SECTOR_WIDTH as u16);
    let minimum = usize::try_from(definition.occurrences.minimum.max(0))
        .map_err(|_| SimError::NumericOverflow)?
        .min(sector_count);
    let maximum = usize::try_from(definition.occurrences.maximum.max(0))
        .map_err(|_| SimError::NumericOverflow)?
        .min(sector_count);
    if definition.uniqueness == WorldgenSpecialUniquenessV1::Global
        && already_global_unique.contains(&definition.special_id)
    {
        return Ok(PlacementState {
            placed: 0,
            minimum: 0,
            maximum: 0,
        });
    }
    if definition.uniqueness != WorldgenSpecialUniquenessV1::None {
        if minimum == 0 || maximum == 0 {
            return Ok(PlacementState {
                placed: 0,
                minimum: 0,
                maximum: 0,
            });
        }
        let selected = rng.next_u64()
            % u64::try_from(maximum).map_err(|_| SimError::NumericOverflow)?
            < u64::try_from(minimum).map_err(|_| SimError::NumericOverflow)?;
        return Ok(PlacementState {
            placed: 0,
            minimum: usize::from(selected),
            maximum: usize::from(selected),
        });
    }
    Ok(PlacementState {
        placed: 0,
        minimum: minimum.min(maximum),
        maximum,
    })
}

fn choose_rotation(
    definition: &OvermapFixedSpecial,
    layout: &WorldgenOvermapLayoutV1,
    layers: &BTreeMap<i32, Vec<String>>,
    identities: &BTreeMap<String, WorldgenOmtIdentityV1>,
    default_below: &WorldgenOmtIdentityV1,
    default_above: &WorldgenOmtIdentityV1,
    origin: ChunkCoord,
    rng: &mut ChaCha8Rng,
) -> Result<Option<u8>, SimError> {
    let rotations = if definition.rotate { 4 } else { 1 };
    let mut scored = Vec::new();
    let mut best = -1;
    for rotation in 0..rotations {
        let score = connection_score(
            definition,
            layout,
            layers,
            identities,
            default_below,
            default_above,
            origin,
            rotation,
        )?;
        if score > best {
            best = score;
            scored.clear();
        }
        if score == best && score >= 0 {
            scored.push(rotation);
        }
    }
    shuffle(&mut scored, rng)?;
    for rotation in scored {
        if can_place(
            definition,
            layout,
            layers,
            identities,
            default_below,
            default_above,
            origin,
            rotation,
        )? {
            return Ok(Some(rotation));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn connection_score(
    definition: &OvermapFixedSpecial,
    layout: &WorldgenOvermapLayoutV1,
    layers: &BTreeMap<i32, Vec<String>>,
    identities: &BTreeMap<String, WorldgenOmtIdentityV1>,
    default_below: &WorldgenOmtIdentityV1,
    default_above: &WorldgenOmtIdentityV1,
    origin: ChunkCoord,
    rotation: u8,
) -> Result<i32, SimError> {
    let mut score = 0;
    for connection in &definition.connections {
        let position = absolute_rotated(origin, connection.offset, rotation)?;
        if !inbounds(layout, position, 0) {
            return Ok(-1);
        }
        let identity = identity_at(
            layout,
            layers,
            identities,
            default_below,
            default_above,
            position,
        )?;
        if identity.type_id == connection.terrain_type {
            score += 1;
        } else if connection.existing
            || !connection
                .allowed_location_types
                .contains(&identity.type_id)
        {
            return Ok(-1);
        }
    }
    Ok(score)
}

#[allow(clippy::too_many_arguments)]
fn can_place(
    definition: &OvermapFixedSpecial,
    layout: &WorldgenOvermapLayoutV1,
    layers: &BTreeMap<i32, Vec<String>>,
    identities: &BTreeMap<String, WorldgenOmtIdentityV1>,
    default_below: &WorldgenOmtIdentityV1,
    default_above: &WorldgenOmtIdentityV1,
    origin: ChunkCoord,
    rotation: u8,
) -> Result<bool, SimError> {
    for terrain in &definition.terrains {
        let position = absolute_rotated(origin, terrain.offset, rotation)?;
        if !inbounds(layout, position, 1) {
            return Ok(false);
        }
        let identity = identity_at(
            layout,
            layers,
            identities,
            default_below,
            default_above,
            position,
        )?;
        let default_non_surface = position.z != 0
            && (position.z < 0 && identity.full_id == default_below.full_id
                || position.z > 0 && identity.full_id == default_above.full_id);
        if !default_non_surface && !terrain.allowed_location_types.contains(&identity.type_id) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn apply_placement(
    definition: &OvermapFixedSpecial,
    layout: &WorldgenOvermapLayoutV1,
    layers: &mut BTreeMap<i32, Vec<String>>,
    default_below: &WorldgenOmtIdentityV1,
    default_above: &WorldgenOmtIdentityV1,
    origin: ChunkCoord,
    rotation: u8,
    city: Option<&WorldgenCityV1>,
    placements: &mut Vec<WorldgenSpecialPlacementV1>,
    road_anchors: &mut Vec<OvermapSpecialRoadAnchor>,
) -> Result<(), SimError> {
    let mut terrain_omts = Vec::new();
    for terrain in &definition.terrains {
        let Some(identity) = rotated_identity(terrain, rotation) else {
            continue;
        };
        let position = absolute_rotated(origin, terrain.offset, rotation)?;
        ensure_layer(layers, position.z, default_below, default_above)?;
        let index = absolute_index(layout, position)?;
        *layers
            .get_mut(&position.z)
            .and_then(|layer| layer.get_mut(index))
            .ok_or(SimError::InvalidTerrain)? = identity.full_id.clone();
        terrain_omts.push(position);
    }
    if terrain_omts.is_empty() {
        return Err(SimError::InvalidTerrain);
    }
    for connection in &definition.connections {
        if connection.connection_id != "local_road" || connection.terrain_type != "road" {
            return Err(SimError::InvalidTerrain);
        }
        let endpoint = absolute_rotated(origin, connection.offset, rotation)?;
        if endpoint.z != 0 {
            return Err(SimError::InvalidTerrain);
        }
        let target = city.map_or_else(
            || nearest_existing_road(layout, layers, endpoint),
            |city| Ok(city.center),
        )?;
        let initial_direction = connection
            .from
            .map(|from| direction_from(from, connection.offset))
            .transpose()?
            .map(|direction| rotate_direction(direction, rotation));
        road_anchors.push(OvermapSpecialRoadAnchor {
            endpoint,
            target,
            initial_direction,
        });
    }
    let placement_id = WorldgenSpecialId(
        u32::try_from(placements.len() + 1).map_err(|_| SimError::NumericOverflow)?,
    );
    placements.push(WorldgenSpecialPlacementV1 {
        placement_id,
        special_id: definition.special_id.clone(),
        origin,
        rotation,
        uniqueness: definition.uniqueness,
        terrain_omts,
        population: definition.population.clone(),
    });
    Ok(())
}

fn nearest_eligible_city<'a>(
    definition: &OvermapFixedSpecial,
    cities: &'a [WorldgenCityV1],
    origin: ChunkCoord,
) -> Option<&'a WorldgenCityV1> {
    let nearest = cities
        .iter()
        .min_by_key(|city| (city_distance(city, origin), city.city_id));
    if !special_requires_city(definition) {
        return nearest;
    }
    let city = nearest?;
    if !definition.city_sizes.contains(i32::from(city.size)) {
        return None;
    }
    let distance = city_distance(city, origin);
    if definition.city_distance.maximum > i32::from(WORLDGEN_OVERMAP_WIDTH) {
        (cities
            .iter()
            .all(|candidate| city_distance(candidate, origin) > definition.city_distance.minimum))
        .then_some(city)
    } else {
        (distance <= definition.city_distance.maximum
            && definition.city_distance.minimum < distance)
            .then_some(city)
    }
}

fn special_requires_city(definition: &OvermapFixedSpecial) -> bool {
    definition.city_sizes.minimum > 0
        || definition.city_distance.maximum < i32::from(WORLDGEN_OVERMAP_WIDTH)
}

fn city_distance(city: &WorldgenCityV1, origin: ChunkCoord) -> i32 {
    let dx = i64::from(origin.x) - i64::from(city.center.x);
    let dy = i64::from(origin.y) - i64::from(city.center.y);
    let squared = u64::try_from(dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy)))
        .unwrap_or(u64::MAX);
    i32::try_from(integer_sqrt(squared))
        .unwrap_or(i32::MAX)
        .saturating_sub(i32::from(city.size))
        .max(0)
}

fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut lower = 1;
    let mut upper = value.min(u64::from(u32::MAX)).saturating_add(1);
    while lower + 1 < upper {
        let middle = lower + (upper - lower) / 2;
        if middle <= value / middle {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    lower
}

fn rotated_identity(
    terrain: &OvermapFixedSpecialTerrain,
    rotation: u8,
) -> Option<&WorldgenOmtIdentityV1> {
    match terrain.rotated_identities.len() {
        0 => None,
        1 => terrain.rotated_identities.first(),
        4 => terrain.rotated_identities.get(usize::from(rotation)),
        _ => None,
    }
}

fn nearest_existing_road(
    layout: &WorldgenOvermapLayoutV1,
    layers: &BTreeMap<i32, Vec<String>>,
    endpoint: ChunkCoord,
) -> Result<ChunkCoord, SimError> {
    let surface = layers.get(&0).ok_or(SimError::InvalidTerrain)?;
    surface
        .iter()
        .enumerate()
        .filter(|(_, id)| id.starts_with("road_") || id.starts_with("bridge_"))
        .map(|(index, _)| {
            let x = i32::try_from(index % usize::from(WORLDGEN_OVERMAP_WIDTH))
                .map_err(|_| SimError::NumericOverflow)?;
            let y = i32::try_from(index / usize::from(WORLDGEN_OVERMAP_WIDTH))
                .map_err(|_| SimError::NumericOverflow)?;
            let position = ChunkCoord {
                x: layout.origin_x + x,
                y: layout.origin_y + y,
                z: 0,
            };
            let distance =
                i64::from(position.x - endpoint.x).abs() + i64::from(position.y - endpoint.y).abs();
            Ok((distance, position))
        })
        .collect::<Result<Vec<_>, SimError>>()?
        .into_iter()
        .min_by_key(|(distance, position)| (*distance, position.x, position.y))
        .map(|(_, position)| position)
        .ok_or(SimError::InvalidTerrain)
}

fn identity_at<'a>(
    layout: &WorldgenOvermapLayoutV1,
    layers: &'a BTreeMap<i32, Vec<String>>,
    identities: &'a BTreeMap<String, WorldgenOmtIdentityV1>,
    default_below: &'a WorldgenOmtIdentityV1,
    default_above: &'a WorldgenOmtIdentityV1,
    position: ChunkCoord,
) -> Result<&'a WorldgenOmtIdentityV1, SimError> {
    let full_id = if let Some(layer) = layers.get(&position.z) {
        layer
            .get(absolute_index(layout, position)?)
            .ok_or(SimError::InvalidTerrain)?
            .as_str()
    } else if position.z < 0 {
        default_below.full_id.as_str()
    } else if position.z > 0 {
        default_above.full_id.as_str()
    } else {
        return Err(SimError::InvalidTerrain);
    };
    identities.get(full_id).ok_or(SimError::InvalidTerrain)
}

fn expand_layers(layout: &WorldgenOvermapLayoutV1) -> Result<BTreeMap<i32, Vec<String>>, SimError> {
    let expected = usize::from(WORLDGEN_OVERMAP_WIDTH) * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    let mut layers = BTreeMap::new();
    for layer in &layout.layers {
        let mut cells = Vec::with_capacity(expected);
        for run in &layer.runs {
            let identity = layout
                .identities
                .get(usize::from(run.identity_index))
                .ok_or(SimError::InvalidTerrain)?;
            let length = usize::try_from(run.length).map_err(|_| SimError::NumericOverflow)?;
            if cells
                .len()
                .checked_add(length)
                .is_none_or(|sum| sum > expected)
            {
                return Err(SimError::InvalidTerrain);
            }
            cells.extend(std::iter::repeat_n(identity.full_id.clone(), length));
        }
        if cells.len() != expected || layers.insert(layer.z, cells).is_some() {
            return Err(SimError::InvalidTerrain);
        }
    }
    layers
        .contains_key(&0)
        .then_some(layers)
        .ok_or(SimError::InvalidTerrain)
}

fn ensure_layer(
    layers: &mut BTreeMap<i32, Vec<String>>,
    z: i32,
    default_below: &WorldgenOmtIdentityV1,
    default_above: &WorldgenOmtIdentityV1,
) -> Result<(), SimError> {
    if layers.contains_key(&z) {
        return Ok(());
    }
    if z == 0 {
        return Err(SimError::InvalidTerrain);
    }
    let identity = if z < 0 { default_below } else { default_above };
    let count = usize::from(WORLDGEN_OVERMAP_WIDTH) * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    layers.insert(z, vec![identity.full_id.clone(); count]);
    Ok(())
}

fn encode_layout(
    mut layout: WorldgenOvermapLayoutV1,
    layers: BTreeMap<i32, Vec<String>>,
    known: BTreeMap<String, WorldgenOmtIdentityV1>,
) -> Result<WorldgenOvermapLayoutV1, SimError> {
    let used = layers
        .values()
        .flatten()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let identities = used
        .iter()
        .map(|id| known.get(*id).cloned().ok_or(SimError::InvalidTerrain))
        .collect::<Result<Vec<_>, _>>()?;
    let indices = identities
        .iter()
        .enumerate()
        .map(|(index, identity)| {
            Ok((
                identity.full_id.clone(),
                u16::try_from(index).map_err(|_| SimError::NumericOverflow)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, SimError>>()?;
    layout.identities = identities;
    layout.layers = layers
        .into_iter()
        .map(|(z, cells)| {
            let cells = cells
                .iter()
                .map(|id| indices.get(id).copied().ok_or(SimError::InvalidTerrain))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(WorldgenOvermapLayerV1 {
                z,
                runs: encode_runs(&cells)?,
            })
        })
        .collect::<Result<Vec<_>, SimError>>()?;
    Ok(layout)
}

fn encode_runs(cells: &[u16]) -> Result<Vec<WorldgenOvermapRunV1>, SimError> {
    let mut runs: Vec<WorldgenOvermapRunV1> = Vec::new();
    for identity_index in cells {
        if let Some(run) = runs
            .last_mut()
            .filter(|run| run.identity_index == *identity_index)
        {
            run.length = run.length.checked_add(1).ok_or(SimError::NumericOverflow)?;
        } else {
            runs.push(WorldgenOvermapRunV1 {
                identity_index: *identity_index,
                length: 1,
            });
        }
    }
    Ok(runs)
}

fn install_known_identity(
    known: &mut BTreeMap<String, WorldgenOmtIdentityV1>,
    identity: WorldgenOmtIdentityV1,
) -> Result<(), SimError> {
    if known
        .get(&identity.full_id)
        .is_some_and(|existing| existing != &identity)
    {
        return Err(SimError::InvalidTerrain);
    }
    known.insert(identity.full_id.clone(), identity);
    Ok(())
}

fn validate_definitions(definitions: &[OvermapFixedSpecial]) -> Result<(), SimError> {
    let mut ids = BTreeSet::new();
    for definition in definitions {
        if definition.special_id.is_empty()
            || !ids.insert(definition.special_id.as_str())
            || definition.terrains.is_empty()
            || definition.terrains.len() > MAX_SPECIAL_PARTS
            || definition.connections.len() > MAX_SPECIAL_CONNECTIONS
            || definition.city_sizes.minimum > definition.city_sizes.maximum
            || definition.city_distance.minimum > definition.city_distance.maximum
            || definition.occurrences.minimum > definition.occurrences.maximum
            || definition.occurrences.maximum <= 0
            || definition.population.as_ref().is_some_and(|population| {
                population.group_id.is_empty()
                    || population.population.minimum > population.population.maximum
                    || population.radius.minimum > population.radius.maximum
            })
        {
            return Err(SimError::InvalidTerrain);
        }
        let mut points = BTreeSet::new();
        for terrain in &definition.terrains {
            if !points.insert(terrain.offset)
                || !matches!(terrain.rotated_identities.len(), 0 | 1 | 4)
                || terrain.allowed_location_types.is_empty()
                || terrain.offset.z < -10
                || terrain.offset.z > 10
            {
                return Err(SimError::InvalidTerrain);
            }
        }
        for connection in &definition.connections {
            if connection.connection_id != "local_road"
                || connection.terrain_type != "road"
                || connection.offset.z != 0
                || connection.allowed_location_types.is_empty()
            {
                return Err(SimError::InvalidTerrain);
            }
            if let Some(from) = connection.from {
                direction_from(from, connection.offset)?;
            }
        }
    }
    Ok(())
}

fn absolute_rotated(
    origin: ChunkCoord,
    offset: ChunkCoord,
    rotation: u8,
) -> Result<ChunkCoord, SimError> {
    let (x, y) = rotate_xy(offset.x, offset.y, rotation)?;
    Ok(ChunkCoord {
        x: origin.x.checked_add(x).ok_or(SimError::NumericOverflow)?,
        y: origin.y.checked_add(y).ok_or(SimError::NumericOverflow)?,
        z: origin
            .z
            .checked_add(offset.z)
            .ok_or(SimError::NumericOverflow)?,
    })
}

fn rotate_xy(x: i32, y: i32, rotation: u8) -> Result<(i32, i32), SimError> {
    match rotation {
        0 => Ok((x, y)),
        1 => Ok((y.checked_neg().ok_or(SimError::NumericOverflow)?, x)),
        2 => Ok((
            x.checked_neg().ok_or(SimError::NumericOverflow)?,
            y.checked_neg().ok_or(SimError::NumericOverflow)?,
        )),
        3 => Ok((y, x.checked_neg().ok_or(SimError::NumericOverflow)?)),
        _ => Err(SimError::InvalidTerrain),
    }
}

fn direction_from(from: ChunkCoord, to: ChunkCoord) -> Result<OvermapRoadBoundary, SimError> {
    match (to.x - from.x, to.y - from.y, to.z - from.z) {
        (0, dy, 0) if dy < 0 => Ok(OvermapRoadBoundary::North),
        (dx, 0, 0) if dx > 0 => Ok(OvermapRoadBoundary::East),
        (0, dy, 0) if dy > 0 => Ok(OvermapRoadBoundary::South),
        (dx, 0, 0) if dx < 0 => Ok(OvermapRoadBoundary::West),
        _ => Err(SimError::InvalidTerrain),
    }
}

fn rotate_direction(direction: OvermapRoadBoundary, rotation: u8) -> OvermapRoadBoundary {
    let index = match direction {
        OvermapRoadBoundary::North => 0,
        OvermapRoadBoundary::East => 1,
        OvermapRoadBoundary::South => 2,
        OvermapRoadBoundary::West => 3,
    };
    match (index + usize::from(rotation)) % 4 {
        0 => OvermapRoadBoundary::North,
        1 => OvermapRoadBoundary::East,
        2 => OvermapRoadBoundary::South,
        _ => OvermapRoadBoundary::West,
    }
}

fn inbounds(layout: &WorldgenOvermapLayoutV1, position: ChunkCoord, margin: i32) -> bool {
    let local_x = position.x - layout.origin_x;
    let local_y = position.y - layout.origin_y;
    (margin..i32::from(WORLDGEN_OVERMAP_WIDTH) - margin).contains(&local_x)
        && (margin..i32::from(WORLDGEN_OVERMAP_HEIGHT) - margin).contains(&local_y)
        && (-10..=10).contains(&position.z)
}

fn absolute_index(
    layout: &WorldgenOvermapLayoutV1,
    position: ChunkCoord,
) -> Result<usize, SimError> {
    let x = position
        .x
        .checked_sub(layout.origin_x)
        .ok_or(SimError::NumericOverflow)?;
    let y = position
        .y
        .checked_sub(layout.origin_y)
        .ok_or(SimError::NumericOverflow)?;
    if !(0..i32::from(WORLDGEN_OVERMAP_WIDTH)).contains(&x)
        || !(0..i32::from(WORLDGEN_OVERMAP_HEIGHT)).contains(&y)
    {
        return Err(SimError::InvalidTerrain);
    }
    usize::try_from(y)
        .ok()
        .and_then(|row| row.checked_mul(usize::from(WORLDGEN_OVERMAP_WIDTH)))
        .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
        .ok_or(SimError::NumericOverflow)
}

fn inclusive_i32(rng: &mut ChaCha8Rng, minimum: i32, maximum: i32) -> Result<i32, SimError> {
    if minimum > maximum {
        return Err(SimError::InvalidTerrain);
    }
    let width = u64::try_from(i64::from(maximum) - i64::from(minimum) + 1)
        .map_err(|_| SimError::NumericOverflow)?;
    let offset = i64::try_from(rng.next_u64() % width).map_err(|_| SimError::NumericOverflow)?;
    i32::try_from(i64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
}

fn shuffle<T>(values: &mut [T], rng: &mut ChaCha8Rng) -> Result<(), SimError> {
    for upper in (1..values.len()).rev() {
        let selected = usize::try_from(
            rng.next_u64() % u64::try_from(upper + 1).map_err(|_| SimError::NumericOverflow)?,
        )
        .map_err(|_| SimError::NumericOverflow)?;
        values.swap(upper, selected);
    }
    Ok(())
}

fn special_rng(
    world_seed: [u8; 32],
    generator_version: u16,
    origin_x: i32,
    origin_y: i32,
) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust overmap special placement v1");
    hasher.update(&world_seed);
    hasher.update(&generator_version.to_be_bytes());
    hasher.update(&origin_x.to_be_bytes());
    hasher.update(&origin_y.to_be_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}
