use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use cdda_protocol::{
    ChunkCoord, WORLDGEN_OVERMAP_HEIGHT, WORLDGEN_OVERMAP_WIDTH, WorldgenCityV1,
    WorldgenOmtIdentityV1, WorldgenOvermapLayoutV1, WorldgenOvermapRunV1,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use crate::SimError;

pub const OVERMAP_ROAD_MASK_IDS: [&str; 16] = [
    "road_isolated",
    "road_end_south",
    "road_end_west",
    "road_ne",
    "road_end_north",
    "road_ns",
    "road_es",
    "road_nes",
    "road_end_east",
    "road_wn",
    "road_ew",
    "road_new",
    "road_sw",
    "road_nsw",
    "road_esw",
    "road_nesw",
];

const SOUTH: u8 = 1;
const WEST: u8 = 2;
const NORTH: u8 = 4;
const EAST: u8 = 8;
const MINIMUM_CORE_EXITS: usize = 3;
const MAXIMUM_INHERITED_EXITS: usize = 16;
const MAXIMUM_ROAD_POINTS: usize = 2_048;
const BORDER_CORNER_MARGIN: i32 = 10;
const FIELD_STEP_COST: u64 = 6;
const EXISTING_ROAD_STEP_COST: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OvermapRoadBoundary {
    North,
    East,
    South,
    West,
}

impl OvermapRoadBoundary {
    const ALL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    const fn outward_mask(self) -> u8 {
        match self {
            Self::North => NORTH,
            Self::East => EAST,
            Self::South => SOUTH,
            Self::West => WEST,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OvermapRoadExit {
    pub position: ChunkCoord,
    pub boundary: OvermapRoadBoundary,
}

/// Places the core inter-city road topology into one retained overmap.
///
/// The caller supplies the finalized 16-peer `road` identity family and any
/// exits inherited from already-owned neighboring overmaps. The returned exits
/// are encoded into the endpoint masks as outward segments, so the durable
/// layout itself remains sufficient to recover the topology. Local 24x24 road
/// mapgen is a separate consumer of these OMT identities.
pub fn place_overmap_roads(
    world_seed: [u8; 32],
    generator_version: u16,
    mut layout: WorldgenOvermapLayoutV1,
    cities: &[WorldgenCityV1],
    inherited_exits: &[OvermapRoadExit],
    road_identities: &[WorldgenOmtIdentityV1],
) -> Result<(WorldgenOvermapLayoutV1, Vec<OvermapRoadExit>), SimError> {
    let identities = road_identity_family(road_identities)?;
    let mut surface = expand_surface(&layout)?;
    let mut surface_ids = surface
        .iter()
        .map(|index| {
            layout
                .identities
                .get(usize::from(*index))
                .map(|identity| identity.full_id.clone())
                .ok_or(SimError::InvalidTerrain)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut road_masks = surface
        .iter()
        .map(|index| {
            layout
                .identities
                .get(usize::from(*index))
                .and_then(|identity| road_mask(&identity.full_id))
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let mut road_cells = road_masks.iter().map(|mask| *mask != 0).collect::<Vec<_>>();

    let mut rng = road_rng(
        world_seed,
        generator_version,
        layout.origin_x,
        layout.origin_y,
    );
    let mut exits = validate_inherited_exits(&layout, inherited_exits)?;
    add_core_exits(&layout, &mut exits, &mut rng)?;

    let mut points = exits
        .iter()
        .map(|exit| absolute_to_index(&layout, exit.position))
        .collect::<Result<Vec<_>, _>>()?;
    if cities.is_empty() {
        let local_x = inclusive_i32(&mut rng, 45, 135)?;
        let local_y = inclusive_i32(&mut rng, 45, 135)?;
        points.push(surface_index(local_x, local_y)?);
    } else {
        for city in cities {
            if city.center.z != 0 {
                return Err(SimError::InvalidTerrain);
            }
            let index = absolute_to_index(&layout, city.center)?;
            if road_masks.get(index).copied() != Some(15) {
                return Err(SimError::InvalidTerrain);
            }
            points.push(index);
        }
    }
    if !(2..=MAXIMUM_ROAD_POINTS).contains(&points.len())
        || points.iter().copied().collect::<BTreeSet<_>>().len() != points.len()
    {
        return Err(SimError::InvalidTerrain);
    }

    connect_closest_points(&points, &mut road_masks, &mut road_cells, &mut rng)?;
    for exit in &exits {
        let index = absolute_to_index(&layout, exit.position)?;
        let mask = road_masks.get_mut(index).ok_or(SimError::InvalidTerrain)?;
        *mask |= exit.boundary.outward_mask();
        road_cells[index] = true;
    }
    for (index, mask) in road_masks.iter().copied().enumerate() {
        if mask == 0 {
            continue;
        }
        let identity = identities
            .get(usize::from(mask))
            .ok_or(SimError::InvalidTerrain)?;
        install_identity(&mut layout.identities, identity)?;
        surface_ids[index] = identity.full_id.clone();
    }
    layout
        .identities
        .sort_by(|left, right| left.full_id.cmp(&right.full_id));
    let remap = layout
        .identities
        .iter()
        .enumerate()
        .map(|(index, identity)| {
            Ok((
                identity.full_id.as_str(),
                u16::try_from(index).map_err(|_| SimError::NumericOverflow)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, SimError>>()?;
    for (index, full_id) in surface_ids.iter().enumerate() {
        surface[index] = *remap
            .get(full_id.as_str())
            .ok_or(SimError::InvalidTerrain)?;
    }
    let surface_layer = layout
        .layers
        .iter_mut()
        .find(|layer| layer.z == 0)
        .ok_or(SimError::InvalidTerrain)?;
    surface_layer.runs = encode_runs(&surface)?;
    if layout.layers.iter().any(|layer| layer.z != 0) {
        return Err(SimError::InvalidTerrain);
    }
    Ok((layout, exits))
}

fn road_identity_family(
    identities: &[WorldgenOmtIdentityV1],
) -> Result<[WorldgenOmtIdentityV1; 16], SimError> {
    if identities.len() != OVERMAP_ROAD_MASK_IDS.len() {
        return Err(SimError::InvalidTerrain);
    }
    let by_id = identities
        .iter()
        .map(|identity| (identity.full_id.as_str(), identity))
        .collect::<BTreeMap<_, _>>();
    OVERMAP_ROAD_MASK_IDS
        .map(|id| {
            let identity = by_id.get(id).copied().ok_or(SimError::InvalidTerrain)?;
            (identity.type_id == "road")
                .then(|| identity.clone())
                .ok_or(SimError::InvalidTerrain)
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| SimError::InvalidTerrain)
}

fn validate_inherited_exits(
    layout: &WorldgenOvermapLayoutV1,
    inherited: &[OvermapRoadExit],
) -> Result<Vec<OvermapRoadExit>, SimError> {
    if inherited.len() > MAXIMUM_INHERITED_EXITS {
        return Err(SimError::InvalidTerrain);
    }
    let mut unique = BTreeSet::new();
    let mut exits = Vec::with_capacity(inherited.len().max(MINIMUM_CORE_EXITS));
    for exit in inherited {
        validate_exit(layout, *exit)?;
        if !unique.insert(*exit) {
            return Err(SimError::InvalidTerrain);
        }
        exits.push(*exit);
    }
    Ok(exits)
}

fn validate_exit(layout: &WorldgenOvermapLayoutV1, exit: OvermapRoadExit) -> Result<(), SimError> {
    if exit.position.z != 0 {
        return Err(SimError::InvalidTerrain);
    }
    let local_x = exit
        .position
        .x
        .checked_sub(layout.origin_x)
        .ok_or(SimError::NumericOverflow)?;
    let local_y = exit
        .position
        .y
        .checked_sub(layout.origin_y)
        .ok_or(SimError::NumericOverflow)?;
    let maximum_x = i32::from(WORLDGEN_OVERMAP_WIDTH) - 1;
    let maximum_y = i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1;
    let valid = match exit.boundary {
        OvermapRoadBoundary::North => local_y == 0,
        OvermapRoadBoundary::East => local_x == maximum_x,
        OvermapRoadBoundary::South => local_y == maximum_y,
        OvermapRoadBoundary::West => local_x == 0,
    };
    if !valid
        || !(BORDER_CORNER_MARGIN..=maximum_x - BORDER_CORNER_MARGIN).contains(&local_x)
            && matches!(
                exit.boundary,
                OvermapRoadBoundary::North | OvermapRoadBoundary::South
            )
        || !(BORDER_CORNER_MARGIN..=maximum_y - BORDER_CORNER_MARGIN).contains(&local_y)
            && matches!(
                exit.boundary,
                OvermapRoadBoundary::East | OvermapRoadBoundary::West
            )
    {
        return Err(SimError::InvalidTerrain);
    }
    Ok(())
}

fn add_core_exits(
    layout: &WorldgenOvermapLayoutV1,
    exits: &mut Vec<OvermapRoadExit>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    if exits.len() >= MINIMUM_CORE_EXITS {
        return Ok(());
    }
    let mut boundaries = OvermapRoadBoundary::ALL;
    for upper in (1..boundaries.len()).rev() {
        let selected = usize::try_from(
            rng.next_u64() % u64::try_from(upper + 1).map_err(|_| SimError::NumericOverflow)?,
        )
        .map_err(|_| SimError::NumericOverflow)?;
        boundaries.swap(upper, selected);
    }
    for boundary in boundaries {
        if exits.len() >= MINIMUM_CORE_EXITS {
            break;
        }
        let variable = inclusive_i32(
            rng,
            BORDER_CORNER_MARGIN,
            i32::from(WORLDGEN_OVERMAP_WIDTH) - 1 - BORDER_CORNER_MARGIN,
        )?;
        let (local_x, local_y) = match boundary {
            OvermapRoadBoundary::North => (variable, 0),
            OvermapRoadBoundary::East => (i32::from(WORLDGEN_OVERMAP_WIDTH) - 1, variable),
            OvermapRoadBoundary::South => (variable, i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1),
            OvermapRoadBoundary::West => (0, variable),
        };
        let exit = OvermapRoadExit {
            position: ChunkCoord {
                x: layout
                    .origin_x
                    .checked_add(local_x)
                    .ok_or(SimError::NumericOverflow)?,
                y: layout
                    .origin_y
                    .checked_add(local_y)
                    .ok_or(SimError::NumericOverflow)?,
                z: 0,
            },
            boundary,
        };
        if !exits.contains(&exit) {
            exits.push(exit);
        }
    }
    (exits.len() >= MINIMUM_CORE_EXITS)
        .then_some(())
        .ok_or(SimError::InvalidTerrain)
}

fn connect_closest_points(
    points: &[usize],
    road_masks: &mut [u8],
    road_cells: &mut [bool],
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    if !(2..=MAXIMUM_ROAD_POINTS).contains(&points.len()) {
        return Err(SimError::InvalidTerrain);
    }
    let edges = sorted_edges(points)?;
    let mut components = (0..points.len()).collect::<Vec<_>>();
    for (_distance, left, right) in edges {
        let connect =
            join_components(&mut components, left, right) || rng.next_u64().is_multiple_of(10);
        if connect {
            let path = road_path(points[left], points[right], road_cells)?;
            paint_path(&path, road_masks, road_cells)?;
        }
    }
    Ok(())
}

/// Returns the exact sorted-edge minimum spanning tree used by production
/// road placement for local coordinates in one retained 180x180 overmap.
/// This side-effect-free projection is shared with the pinned C++ oracle.
pub fn overmap_road_mst_edges(local_points: &[ChunkCoord]) -> Result<Vec<(u16, u16)>, SimError> {
    if !(2..=MAXIMUM_ROAD_POINTS).contains(&local_points.len()) {
        return Err(SimError::InvalidTerrain);
    }
    let points = local_points
        .iter()
        .map(|point| {
            if point.z != 0 {
                return Err(SimError::InvalidTerrain);
            }
            surface_index(point.x, point.y)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if points.iter().copied().collect::<BTreeSet<_>>().len() != points.len() {
        return Err(SimError::InvalidTerrain);
    }
    let mut components = (0..points.len()).collect::<Vec<_>>();
    let mut result = Vec::with_capacity(points.len() - 1);
    for (_distance, left, right) in sorted_edges(&points)? {
        if join_components(&mut components, left, right) {
            result.push((
                u16::try_from(left).map_err(|_| SimError::NumericOverflow)?,
                u16::try_from(right).map_err(|_| SimError::NumericOverflow)?,
            ));
        }
    }
    Ok(result)
}

fn sorted_edges(points: &[usize]) -> Result<Vec<(u64, usize, usize)>, SimError> {
    let capacity = points
        .len()
        .checked_mul(points.len().saturating_sub(1))
        .and_then(|twice| twice.checked_div(2))
        .ok_or(SimError::NumericOverflow)?;
    let mut edges = Vec::with_capacity(capacity);
    for left in 0..points.len() - 1 {
        for right in left + 1..points.len() {
            let (left_x, left_y) = index_xy(points[left])?;
            let (right_x, right_y) = index_xy(points[right])?;
            let dx = i64::from(left_x) - i64::from(right_x);
            let dy = i64::from(left_y) - i64::from(right_y);
            let distance =
                u64::try_from(dx * dx + dy * dy).map_err(|_| SimError::NumericOverflow)?;
            edges.push((distance, left, right));
        }
    }
    edges.sort_unstable();
    Ok(edges)
}

fn join_components(components: &mut [usize], left: usize, right: usize) -> bool {
    if components[left] == components[right] {
        return false;
    }
    let replaced = components[right];
    let replacement = components[left];
    for component in components {
        if *component == replaced {
            *component = replacement;
        }
    }
    true
}

fn road_path(source: usize, destination: usize, roads: &[bool]) -> Result<Vec<usize>, SimError> {
    let cell_count = usize::from(WORLDGEN_OVERMAP_WIDTH) * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    if source >= cell_count || destination >= cell_count || roads.len() != cell_count {
        return Err(SimError::InvalidTerrain);
    }
    let (destination_x, destination_y) = index_xy(destination)?;
    let mut costs = vec![u64::MAX; cell_count];
    let mut previous = vec![None; cell_count];
    let mut queue = BinaryHeap::new();
    costs[source] = 0;
    queue.push(Reverse((manhattan(source, destination)?, 0_u64, source)));
    while let Some(Reverse((_estimate, cost, current))) = queue.pop() {
        if current == destination {
            break;
        }
        if cost != costs[current] {
            continue;
        }
        for neighbor in neighbors(current)? {
            let step = if roads[neighbor] {
                EXISTING_ROAD_STEP_COST
            } else {
                FIELD_STEP_COST
            };
            let next = cost.checked_add(step).ok_or(SimError::NumericOverflow)?;
            if next < costs[neighbor] {
                costs[neighbor] = next;
                previous[neighbor] = Some(current);
                let (x, y) = index_xy(neighbor)?;
                let heuristic =
                    u64::try_from((x - destination_x).abs() + (y - destination_y).abs())
                        .map_err(|_| SimError::NumericOverflow)?;
                queue.push(Reverse((
                    next.checked_add(heuristic)
                        .ok_or(SimError::NumericOverflow)?,
                    next,
                    neighbor,
                )));
            }
        }
    }
    if costs[destination] == u64::MAX {
        return Err(SimError::InvalidTerrain);
    }
    let mut path = vec![destination];
    let mut current = destination;
    while current != source {
        current = previous[current].ok_or(SimError::InvalidTerrain)?;
        path.push(current);
    }
    path.reverse();
    Ok(path)
}

fn paint_path(path: &[usize], masks: &mut [u8], roads: &mut [bool]) -> Result<(), SimError> {
    if path.len() < 2 {
        return Err(SimError::InvalidTerrain);
    }
    for pair in path.windows(2) {
        let (from_mask, to_mask) = segment_masks(pair[0], pair[1])?;
        *masks.get_mut(pair[0]).ok_or(SimError::InvalidTerrain)? |= from_mask;
        *masks.get_mut(pair[1]).ok_or(SimError::InvalidTerrain)? |= to_mask;
        roads[pair[0]] = true;
        roads[pair[1]] = true;
    }
    Ok(())
}

fn segment_masks(from: usize, to: usize) -> Result<(u8, u8), SimError> {
    let (from_x, from_y) = index_xy(from)?;
    let (to_x, to_y) = index_xy(to)?;
    match (to_x - from_x, to_y - from_y) {
        (0, -1) => Ok((NORTH, SOUTH)),
        (1, 0) => Ok((EAST, WEST)),
        (0, 1) => Ok((SOUTH, NORTH)),
        (-1, 0) => Ok((WEST, EAST)),
        _ => Err(SimError::InvalidTerrain),
    }
}

fn neighbors(index: usize) -> Result<Vec<usize>, SimError> {
    let (x, y) = index_xy(index)?;
    let width = i32::from(WORLDGEN_OVERMAP_WIDTH);
    let height = i32::from(WORLDGEN_OVERMAP_HEIGHT);
    [(0, -1), (1, 0), (0, 1), (-1, 0)]
        .into_iter()
        .filter_map(|(dx, dy)| {
            let next_x = x + dx;
            let next_y = y + dy;
            (next_x >= 0 && next_x < width && next_y >= 0 && next_y < height)
                .then(|| surface_index(next_x, next_y))
        })
        .collect()
}

fn manhattan(left: usize, right: usize) -> Result<u64, SimError> {
    let (left_x, left_y) = index_xy(left)?;
    let (right_x, right_y) = index_xy(right)?;
    u64::try_from((left_x - right_x).abs() + (left_y - right_y).abs())
        .map_err(|_| SimError::NumericOverflow)
}

fn index_xy(index: usize) -> Result<(i32, i32), SimError> {
    let width = usize::from(WORLDGEN_OVERMAP_WIDTH);
    let cell_count = width * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    if index >= cell_count {
        return Err(SimError::InvalidTerrain);
    }
    Ok((
        i32::try_from(index % width).map_err(|_| SimError::NumericOverflow)?,
        i32::try_from(index / width).map_err(|_| SimError::NumericOverflow)?,
    ))
}

fn absolute_to_index(
    layout: &WorldgenOvermapLayoutV1,
    position: ChunkCoord,
) -> Result<usize, SimError> {
    if position.z != 0 {
        return Err(SimError::InvalidTerrain);
    }
    surface_index(
        position
            .x
            .checked_sub(layout.origin_x)
            .ok_or(SimError::NumericOverflow)?,
        position
            .y
            .checked_sub(layout.origin_y)
            .ok_or(SimError::NumericOverflow)?,
    )
}

fn surface_index(x: i32, y: i32) -> Result<usize, SimError> {
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

fn expand_surface(layout: &WorldgenOvermapLayoutV1) -> Result<Vec<u16>, SimError> {
    if layout.layers.len() != 1 || layout.layers[0].z != 0 {
        return Err(SimError::InvalidTerrain);
    }
    let expected = usize::from(WORLDGEN_OVERMAP_WIDTH) * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    let mut cells = Vec::with_capacity(expected);
    for run in &layout.layers[0].runs {
        if layout
            .identities
            .get(usize::from(run.identity_index))
            .is_none()
        {
            return Err(SimError::InvalidTerrain);
        }
        let length = usize::try_from(run.length).map_err(|_| SimError::NumericOverflow)?;
        if cells
            .len()
            .checked_add(length)
            .is_none_or(|sum| sum > expected)
        {
            return Err(SimError::InvalidTerrain);
        }
        cells.extend(std::iter::repeat_n(run.identity_index, length));
    }
    (cells.len() == expected)
        .then_some(cells)
        .ok_or(SimError::InvalidTerrain)
}

fn road_mask(full_id: &str) -> Option<u8> {
    OVERMAP_ROAD_MASK_IDS
        .iter()
        .position(|candidate| *candidate == full_id)
        .and_then(|index| u8::try_from(index).ok())
}

fn install_identity(
    identities: &mut Vec<WorldgenOmtIdentityV1>,
    identity: &WorldgenOmtIdentityV1,
) -> Result<(), SimError> {
    if let Some((index, existing)) = identities
        .iter()
        .enumerate()
        .find(|(_index, existing)| existing.full_id == identity.full_id)
    {
        if existing != identity {
            return Err(SimError::InvalidTerrain);
        }
        u16::try_from(index).map_err(|_| SimError::NumericOverflow)?;
        return Ok(());
    }
    identities.push(identity.clone());
    u16::try_from(identities.len() - 1).map_err(|_| SimError::NumericOverflow)?;
    Ok(())
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

fn inclusive_i32(rng: &mut ChaCha8Rng, minimum: i32, maximum: i32) -> Result<i32, SimError> {
    if minimum > maximum {
        return Err(SimError::InvalidTerrain);
    }
    let width = u64::try_from(i64::from(maximum) - i64::from(minimum) + 1)
        .map_err(|_| SimError::NumericOverflow)?;
    let offset = i64::try_from(rng.next_u64() % width).map_err(|_| SimError::NumericOverflow)?;
    i32::try_from(i64::from(minimum) + offset).map_err(|_| SimError::NumericOverflow)
}

fn road_rng(
    world_seed: [u8; 32],
    generator_version: u16,
    origin_x: i32,
    origin_y: i32,
) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust overmap road placement v1");
    hasher.update(&world_seed);
    hasher.update(&generator_version.to_be_bytes());
    hasher.update(&origin_x.to_be_bytes());
    hasher.update(&origin_y.to_be_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_protocol::{WorldgenCityId, WorldgenOvermapLayerV1, worldgen_omt_identity_at};

    fn identity(id: &str, type_id: &str) -> WorldgenOmtIdentityV1 {
        WorldgenOmtIdentityV1 {
            full_id: id.to_owned(),
            type_id: type_id.to_owned(),
            subtype_id: id.to_owned(),
            generator_id: String::from("field"),
            rotation: 0,
        }
    }

    fn road_identities() -> Vec<WorldgenOmtIdentityV1> {
        OVERMAP_ROAD_MASK_IDS
            .iter()
            .map(|id| identity(id, "road"))
            .collect()
    }

    fn city_layout() -> (WorldgenOvermapLayoutV1, Vec<WorldgenCityV1>) {
        let field = identity("field", "field");
        let center = identity("road_nesw", "road");
        let mut cells = vec![0_u16; 180 * 180];
        let city_points = [(70, 70), (110, 75), (90, 115)];
        for (x, y) in city_points {
            cells[surface_index(x, y).expect("index")] = 1;
        }
        let layout = WorldgenOvermapLayoutV1 {
            origin_x: -90,
            origin_y: -90,
            identities: vec![field, center],
            layers: vec![WorldgenOvermapLayerV1 {
                z: 0,
                runs: encode_runs(&cells).expect("runs"),
            }],
        };
        let cities = city_points
            .into_iter()
            .enumerate()
            .map(|(index, (x, y))| WorldgenCityV1 {
                city_id: WorldgenCityId(u32::try_from(index + 1).expect("city ID")),
                center: ChunkCoord {
                    x: x - 90,
                    y: y - 90,
                    z: 0,
                },
                size: 8,
            })
            .collect();
        (layout, cities)
    }

    fn road_cells(layout: &WorldgenOvermapLayoutV1) -> BTreeSet<ChunkCoord> {
        let catalog = cdda_protocol::WorldgenCatalogV1 {
            generator_version: 2,
            overmap: layout.clone(),
            cities: Vec::new(),
            start_location: None,
            terrain_prototypes: Vec::new(),
            furniture_prototypes: Vec::new(),
            regional_terrain: Vec::new(),
            regional_furniture: Vec::new(),
            omt_generators: Vec::new(),
        };
        (0..180)
            .flat_map(|y| (0..180).map(move |x| (x, y)))
            .filter_map(|(x, y)| {
                let coord = ChunkCoord {
                    x: x - 90,
                    y: y - 90,
                    z: 0,
                };
                worldgen_omt_identity_at(&catalog, coord)
                    .is_some_and(|identity| identity.type_id == "road")
                    .then_some(coord)
            })
            .collect()
    }

    fn layout_trace_hash(layout: &WorldgenOvermapLayoutV1) -> String {
        let mut hasher = blake3::Hasher::new_derive_key("cdda-rust road layout test trace v1");
        hasher.update(&layout.origin_x.to_be_bytes());
        hasher.update(&layout.origin_y.to_be_bytes());
        for identity in &layout.identities {
            for value in [
                identity.full_id.as_str(),
                identity.type_id.as_str(),
                identity.subtype_id.as_str(),
                identity.generator_id.as_str(),
            ] {
                hasher.update(
                    &u64::try_from(value.len())
                        .expect("test identity length should fit")
                        .to_be_bytes(),
                );
                hasher.update(value.as_bytes());
            }
            hasher.update(&identity.rotation.to_be_bytes());
        }
        for layer in &layout.layers {
            hasher.update(&layer.z.to_be_bytes());
            for run in &layer.runs {
                hasher.update(&run.identity_index.to_be_bytes());
                hasher.update(&run.length.to_be_bytes());
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    #[test]
    fn city_network_is_deterministic_connected_and_reaches_three_boundaries() {
        let (layout, cities) = city_layout();
        let generate = || {
            place_overmap_roads(
                [41; 32],
                2,
                layout.clone(),
                &cities,
                &[],
                &road_identities(),
            )
            .expect("roads")
        };
        let (roads, exits) = generate();
        assert_eq!((roads.clone(), exits.clone()), generate());
        assert_eq!(exits.len(), 3);
        let cells = road_cells(&roads);
        assert!(cities.iter().all(|city| cells.contains(&city.center)));
        assert!(exits.iter().all(|exit| cells.contains(&exit.position)));

        let start = cities[0].center;
        let mut reached = BTreeSet::from([start]);
        let mut pending = vec![start];
        while let Some(current) = pending.pop() {
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let neighbor = ChunkCoord {
                    x: current.x + dx,
                    y: current.y + dy,
                    z: 0,
                };
                if cells.contains(&neighbor) && reached.insert(neighbor) {
                    pending.push(neighbor);
                }
            }
        }
        assert!(cities.iter().all(|city| reached.contains(&city.center)));
        assert!(exits.iter().all(|exit| reached.contains(&exit.position)));
        assert_eq!(
            exits,
            [
                OvermapRoadExit {
                    position: ChunkCoord {
                        x: -16,
                        y: -90,
                        z: 0,
                    },
                    boundary: OvermapRoadBoundary::North,
                },
                OvermapRoadExit {
                    position: ChunkCoord {
                        x: 89,
                        y: -78,
                        z: 0,
                    },
                    boundary: OvermapRoadBoundary::East,
                },
                OvermapRoadExit {
                    position: ChunkCoord {
                        x: -80,
                        y: 89,
                        z: 0,
                    },
                    boundary: OvermapRoadBoundary::South,
                },
            ]
        );
        assert_eq!(cells.len(), 410);
        assert_eq!(
            layout_trace_hash(&roads),
            "fb3b5bcb1aa34732eac67c3b3b342a1b0b2ab3087486dda67103908c8b0279dc"
        );
    }

    #[test]
    fn representative_mst_trace_is_exact() {
        let points = [
            ChunkCoord { x: 10, y: 0, z: 0 },
            ChunkCoord {
                x: 179,
                y: 40,
                z: 0,
            },
            ChunkCoord {
                x: 100,
                y: 179,
                z: 0,
            },
            ChunkCoord { x: 70, y: 70, z: 0 },
            ChunkCoord {
                x: 110,
                y: 75,
                z: 0,
            },
            ChunkCoord {
                x: 90,
                y: 115,
                z: 0,
            },
        ];
        assert_eq!(
            overmap_road_mst_edges(&points).expect("MST"),
            [(3, 4), (4, 5), (2, 5), (1, 4), (0, 3)]
        );
    }

    #[test]
    fn malformed_topology_inputs_fail_closed() {
        let (layout, cities) = city_layout();
        let valid_identities = road_identities();
        let mut incomplete_identities = valid_identities.clone();
        incomplete_identities.pop();
        assert!(
            place_overmap_roads(
                [47; 32],
                2,
                layout.clone(),
                &cities,
                &[],
                &incomplete_identities,
            )
            .is_err()
        );

        let too_many_exits = (0..=MAXIMUM_INHERITED_EXITS)
            .map(|offset| OvermapRoadExit {
                position: ChunkCoord {
                    x: -80 + i32::try_from(offset).expect("small offset"),
                    y: -90,
                    z: 0,
                },
                boundary: OvermapRoadBoundary::North,
            })
            .collect::<Vec<_>>();
        assert!(
            place_overmap_roads(
                [47; 32],
                2,
                layout,
                &cities,
                &too_many_exits,
                &valid_identities,
            )
            .is_err()
        );
        assert!(
            overmap_road_mst_edges(&[
                ChunkCoord { x: 1, y: 1, z: 0 },
                ChunkCoord { x: 1, y: 1, z: 0 },
            ])
            .is_err()
        );
    }

    #[test]
    fn inherited_boundary_exits_are_preserved_without_generating_replacements() {
        let (layout, cities) = city_layout();
        let inherited = [
            OvermapRoadExit {
                position: ChunkCoord {
                    x: -50,
                    y: -90,
                    z: 0,
                },
                boundary: OvermapRoadBoundary::North,
            },
            OvermapRoadExit {
                position: ChunkCoord { x: 89, y: 10, z: 0 },
                boundary: OvermapRoadBoundary::East,
            },
            OvermapRoadExit {
                position: ChunkCoord { x: 20, y: 89, z: 0 },
                boundary: OvermapRoadBoundary::South,
            },
        ];
        let (roads, exits) =
            place_overmap_roads([43; 32], 2, layout, &cities, &inherited, &road_identities())
                .expect("inherited roads");
        assert_eq!(exits, inherited);
        let cells = road_cells(&roads);
        assert!(inherited.iter().all(|exit| cells.contains(&exit.position)));
    }
}
