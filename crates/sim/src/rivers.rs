use std::collections::{BTreeMap, BTreeSet};

use cdda_protocol::{
    ChunkCoord, WORLDGEN_OVERMAP_HEIGHT, WORLDGEN_OVERMAP_WIDTH, WorldgenOmtIdentityV1,
    WorldgenOvermapLayoutV1, WorldgenOvermapRunV1, WorldgenRiverNodeV1,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use crate::SimError;

pub const OVERMAP_RIVER_IDS: [&str; 14] = [
    "forest_water",
    "river_center",
    "river_north",
    "river_east",
    "river_south",
    "river_west",
    "river_ne",
    "river_nw",
    "river_se",
    "river_sw",
    "river_c_not_ne",
    "river_c_not_nw",
    "river_c_not_se",
    "river_c_not_sw",
];

const RIVER_BORDER: i32 = 10;
const MAX_RIVER_CONTINUATIONS: usize = 4;
const MAX_RIVER_NODES: usize = 64;
const MAX_CURVE_POINTS: usize = 1_024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum OvermapRiverBoundary {
    North,
    East,
    South,
    West,
}

impl OvermapRiverBoundary {
    const fn is_start(self) -> bool {
        matches!(self, Self::North | Self::West)
    }

    const fn order(self) -> u8 {
        match self {
            Self::North => 0,
            Self::East => 1,
            Self::South => 2,
            Self::West => 3,
        }
    }
}

/// A major-river endpoint inherited from an already-owned adjacent overmap.
/// `inward_control` carries the neighboring curve tangent across the boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OvermapRiverContinuation {
    pub position: ChunkCoord,
    pub boundary: OvermapRiverBoundary,
    pub inward_control: Option<ChunkCoord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OvermapRiverSettings {
    pub river_scale: u8,
    /// Fixed-point thousandths of the pinned major-river frequency.
    pub river_frequency_millis: u32,
    /// Fixed-point thousandths of pinned `one_in` denominators.
    pub branch_chance_millis: u32,
    pub branch_remerge_chance_millis: u32,
    /// Fixed-point thousandths subtracted before C++'s integer argument cast.
    pub branch_scale_decrease_millis: u32,
}

impl OvermapRiverSettings {
    #[must_use]
    pub const fn core_default() -> Self {
        Self {
            river_scale: 1,
            river_frequency_millis: 1_500,
            branch_chance_millis: 64_000,
            branch_remerge_chance_millis: 2_000,
            branch_scale_decrease_millis: 1_000,
        }
    }
}

pub type OvermapRiverNode = WorldgenRiverNodeV1;

/// Places major rivers, bounded local branches, and the complete pinned shore
/// identity family into one coordinate-owned surface overmap.
pub fn place_overmap_rivers(
    world_seed: [u8; 32],
    generator_version: u16,
    mut layout: WorldgenOvermapLayoutV1,
    settings: OvermapRiverSettings,
    inherited: &[OvermapRiverContinuation],
    existing_major_rivers: u32,
    river_identities: &[WorldgenOmtIdentityV1],
) -> Result<(WorldgenOvermapLayoutV1, Vec<OvermapRiverNode>), SimError> {
    validate_settings(settings)?;
    let identities = river_identity_family(river_identities)?;
    let mut surface = expand_surface(&layout)?;
    if settings.river_scale == 0 {
        return Ok((layout, Vec::new()));
    }
    let continuations = validate_continuations(&layout, inherited)?;
    let mut rng = river_rng(
        world_seed,
        generator_version,
        layout.origin_x,
        layout.origin_y,
    );
    if continuations.is_empty()
        && !major_river_frequency_roll(
            &mut rng,
            settings.river_frequency_millis,
            existing_major_rivers,
        )?
    {
        return Ok((layout, Vec::new()));
    }

    let mut starts = continuations
        .iter()
        .filter(|continuation| continuation.boundary.is_start())
        .copied()
        .collect::<Vec<_>>();
    let mut ends = continuations
        .iter()
        .filter(|continuation| !continuation.boundary.is_start())
        .copied()
        .collect::<Vec<_>>();
    starts.sort_by_key(|continuation| continuation.boundary.order());
    ends.sort_by_key(|continuation| continuation.boundary.order());
    if starts.len() > 2 || ends.len() > 2 {
        return Err(SimError::InvalidTerrain);
    }
    let lock_rivers = starts.len() == 2 || ends.len() == 2;
    let river_count = if lock_rivers { 2 } else { 1 };
    if lock_rivers {
        ensure_boundary(&layout, &mut starts, OvermapRiverBoundary::North, &mut rng)?;
        ensure_boundary(&layout, &mut starts, OvermapRiverBoundary::West, &mut rng)?;
        ensure_boundary(&layout, &mut ends, OvermapRiverBoundary::East, &mut rng)?;
        ensure_boundary(&layout, &mut ends, OvermapRiverBoundary::South, &mut rng)?;
        starts.sort_by_key(|continuation| continuation.boundary.order());
        ends.sort_by_key(|continuation| continuation.boundary.order());
    } else {
        if starts.is_empty() {
            let boundary = if rng.next_u64().is_multiple_of(2) {
                OvermapRiverBoundary::North
            } else {
                OvermapRiverBoundary::West
            };
            starts.push(random_continuation(&layout, boundary, &mut rng)?);
        }
        if ends.is_empty() {
            let boundary = if rng.next_u64().is_multiple_of(2) {
                OvermapRiverBoundary::South
            } else {
                OvermapRiverBoundary::East
            };
            ends.push(random_continuation(&layout, boundary, &mut rng)?);
        }
    }

    let mut water = vec![false; surface.len()];
    let mut nodes = Vec::new();
    let scale = 1_u8
        .checked_add(settings.river_scale.max(1))
        .ok_or(SimError::NumericOverflow)?;
    for index in 0..river_count {
        let start = *starts.get(index).ok_or(SimError::InvalidTerrain)?;
        let end = *ends.get(index).ok_or(SimError::InvalidTerrain)?;
        draw_river(
            &layout,
            start.position,
            end.position,
            start.inward_control,
            end.inward_control,
            scale,
            true,
            settings,
            &mut rng,
            &mut water,
            &mut nodes,
        )?;
    }
    if nodes.is_empty() {
        return Err(SimError::InvalidTerrain);
    }

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
    for (index, is_water) in water.iter().copied().enumerate() {
        if !is_water {
            continue;
        }
        let identity_id = polished_river_identity(index, &water)?;
        let identity = identities
            .get(identity_id)
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
    layout.layers[0].runs = encode_runs(&surface)?;
    Ok((layout, nodes))
}

fn validate_settings(settings: OvermapRiverSettings) -> Result<(), SimError> {
    if settings.river_scale > 16
        || settings.river_frequency_millis == 0
        || settings.branch_chance_millis == 0
        || settings.branch_remerge_chance_millis == 0
        || settings.branch_scale_decrease_millis == 0
    {
        return Err(SimError::InvalidTerrain);
    }
    Ok(())
}

fn river_identity_family(
    identities: &[WorldgenOmtIdentityV1],
) -> Result<BTreeMap<&str, WorldgenOmtIdentityV1>, SimError> {
    if identities.len() != OVERMAP_RIVER_IDS.len() {
        return Err(SimError::InvalidTerrain);
    }
    let mut by_id = BTreeMap::new();
    for identity in identities {
        if !OVERMAP_RIVER_IDS.contains(&identity.full_id.as_str())
            || by_id
                .insert(identity.full_id.as_str(), identity.clone())
                .is_some()
        {
            return Err(SimError::InvalidTerrain);
        }
    }
    let valid = OVERMAP_RIVER_IDS.iter().all(|id| {
        by_id.get(id).is_some_and(|identity| match *id {
            "river_north" | "river_east" | "river_south" | "river_west" => {
                identity.type_id == "river" && identity.generator_id == "river"
            }
            _ => identity.type_id == *id,
        })
    });
    valid.then_some(by_id).ok_or(SimError::InvalidTerrain)
}

fn validate_continuations(
    layout: &WorldgenOvermapLayoutV1,
    inherited: &[OvermapRiverContinuation],
) -> Result<Vec<OvermapRiverContinuation>, SimError> {
    if inherited.len() > MAX_RIVER_CONTINUATIONS {
        return Err(SimError::InvalidTerrain);
    }
    let mut unique_boundaries = BTreeSet::new();
    let mut continuations = Vec::with_capacity(inherited.len());
    for continuation in inherited {
        validate_boundary_position(layout, continuation.position, continuation.boundary)?;
        if !unique_boundaries.insert(continuation.boundary) {
            return Err(SimError::InvalidTerrain);
        }
        if let Some(control) = continuation.inward_control {
            if control.z != 0 || absolute_to_index(layout, control).is_err() {
                return Err(SimError::InvalidTerrain);
            }
        }
        continuations.push(*continuation);
    }
    Ok(continuations)
}

fn ensure_boundary(
    layout: &WorldgenOvermapLayoutV1,
    continuations: &mut Vec<OvermapRiverContinuation>,
    boundary: OvermapRiverBoundary,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    if !continuations
        .iter()
        .any(|continuation| continuation.boundary == boundary)
    {
        continuations.push(random_continuation(layout, boundary, rng)?);
    }
    Ok(())
}

fn random_continuation(
    layout: &WorldgenOvermapLayoutV1,
    boundary: OvermapRiverBoundary,
    rng: &mut ChaCha8Rng,
) -> Result<OvermapRiverContinuation, SimError> {
    let maximum_x = i32::from(WORLDGEN_OVERMAP_WIDTH) - 1;
    let maximum_y = i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1;
    let variable = inclusive_i32(rng, RIVER_BORDER, maximum_x - RIVER_BORDER)?;
    let (x, y) = match boundary {
        OvermapRiverBoundary::North => (variable, 0),
        OvermapRiverBoundary::East => (maximum_x, variable),
        OvermapRiverBoundary::South => (variable, maximum_y),
        OvermapRiverBoundary::West => (0, variable),
    };
    Ok(OvermapRiverContinuation {
        position: absolute_position(layout, x, y)?,
        boundary,
        inward_control: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn draw_river(
    layout: &WorldgenOvermapLayoutV1,
    start: ChunkCoord,
    end: ChunkCoord,
    forced_control_start: Option<ChunkCoord>,
    forced_control_end: Option<ChunkCoord>,
    scale: u8,
    major: bool,
    settings: OvermapRiverSettings,
    rng: &mut ChaCha8Rng,
    water: &mut [bool],
    nodes: &mut Vec<OvermapRiverNode>,
) -> Result<(), SimError> {
    if scale == 0 || nodes.len() >= MAX_RIVER_NODES {
        return Ok(());
    }
    let (start_x, start_y) = local_position(layout, start)?;
    let (end_x, end_y) = local_position(layout, end)?;
    let distance = (end_x - start_x).abs().max((end_y - start_y).abs());
    let segments = usize::try_from(distance / 2).map_err(|_| SimError::NumericOverflow)?;
    if segments < 4 || segments > MAX_CURVE_POINTS {
        return Ok(());
    }
    let amplitude = distance / 2;
    let default_control_start = (
        (start_x + (start_x - end_x).abs() / 3 + inclusive_i32(rng, 0, amplitude)?)
            .clamp(0, i32::from(WORLDGEN_OVERMAP_WIDTH) - 1),
        (start_y + (start_y - end_y).abs() / 3 + inclusive_i32(rng, 0, amplitude)?)
            .clamp(0, i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1),
    );
    let default_control_end = (
        (start_x + (start_x - end_x).abs() * 2 / 3 + inclusive_i32(rng, -amplitude, 0)?)
            .clamp(0, i32::from(WORLDGEN_OVERMAP_WIDTH) - 1),
        (start_y + (start_y - end_y).abs() * 2 / 3 + inclusive_i32(rng, -amplitude, 0)?)
            .clamp(0, i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1),
    );
    let control_start = forced_control_start
        .map(|point| local_position(layout, point))
        .transpose()?
        .unwrap_or(default_control_start);
    let control_end = forced_control_end
        .map(|point| local_position(layout, point))
        .transpose()?
        .unwrap_or(default_control_end);
    let curve = cubic_bezier(
        (start_x, start_y),
        control_start,
        control_end,
        (end_x, end_y),
        segments,
    )?;
    let mut size = 0_u32;
    for (segment_index, pair) in curve.windows(2).enumerate() {
        for (mut x, mut y) in line_to(pair[0], pair[1])? {
            if segment_index != 0 && segment_index + 1 != curve.len() - 1 {
                meander(end_x, end_y, scale, rng, &mut x, &mut y)?;
            }
            paint_circle(x, y, scale, water, &mut size)?;
        }
    }
    if size == 0 {
        return Ok(());
    }
    nodes.push(OvermapRiverNode {
        start,
        end,
        control_start: absolute_position(layout, control_start.0, control_start.1)?,
        control_end: absolute_position(layout, control_end.0, control_end.1)?,
        size,
        major,
    });

    let branch_scale = u8::try_from(
        u32::from(scale)
            .saturating_mul(1_000)
            .saturating_sub(settings.branch_scale_decrease_millis)
            / 1_000,
    )
    .map_err(|_| SimError::NumericOverflow)?;
    if branch_scale == 0 || curve.len() < 10 || nodes.len() >= MAX_RIVER_NODES {
        return Ok(());
    }
    let ahead = 2_usize.max(curve.len() / 5);
    let mut branch_last_end = 0;
    for (index, point) in curve.iter().copied().take(curve.len() - 1).enumerate() {
        if nodes.len() >= MAX_RIVER_NODES
            || !inbounds_with_margin(point.0, point.1, i32::from(scale) + 1)
            || !one_in_millis(rng, settings.branch_chance_millis)
            || index <= branch_last_end
        {
            continue;
        }
        let endpoint = if one_in_millis(rng, settings.branch_remerge_chance_millis) {
            let minimum = index.saturating_add(ahead);
            let maximum = index.saturating_add(ahead.saturating_mul(2));
            if minimum >= curve.len() {
                continue;
            }
            let selected = inclusive_usize(rng, minimum, maximum.min(curve.len() - 1))?;
            branch_last_end = selected;
            curve[selected]
        } else {
            (
                point.0 + inclusive_i32(rng, 32, 64)?,
                point.1 + inclusive_i32(rng, 32, 64)?,
            )
        };
        if !inbounds(endpoint.0, endpoint.1) {
            continue;
        }
        draw_river(
            layout,
            absolute_position(layout, point.0, point.1)?,
            absolute_position(layout, endpoint.0, endpoint.1)?,
            None,
            None,
            branch_scale,
            false,
            settings,
            rng,
            water,
            nodes,
        )?;
    }
    Ok(())
}

fn cubic_bezier(
    start: (i32, i32),
    control_start: (i32, i32),
    control_end: (i32, i32),
    end: (i32, i32),
    segments: usize,
) -> Result<Vec<(i32, i32)>, SimError> {
    let n = i128::try_from(segments).map_err(|_| SimError::NumericOverflow)?;
    let denominator = n.checked_pow(3).ok_or(SimError::NumericOverflow)?;
    let mut points = Vec::with_capacity(segments + 1);
    for index in 0..=segments {
        let t = i128::try_from(index).map_err(|_| SimError::NumericOverflow)?;
        let inverse = n.checked_sub(t).ok_or(SimError::NumericOverflow)?;
        let coordinate = |p0: i32, p1: i32, p2: i32, p3: i32| {
            inverse
                .checked_pow(3)?
                .checked_mul(i128::from(p0))?
                .checked_add(
                    3_i128
                        .checked_mul(inverse.checked_pow(2)?)?
                        .checked_mul(t)?
                        .checked_mul(i128::from(p1))?,
                )?
                .checked_add(
                    3_i128
                        .checked_mul(inverse)?
                        .checked_mul(t.checked_pow(2)?)?
                        .checked_mul(i128::from(p2))?,
                )?
                .checked_add(t.checked_pow(3)?.checked_mul(i128::from(p3))?)?
                .checked_add(denominator / 2)?
                .checked_div(denominator)
        };
        let point = (
            i32::try_from(
                coordinate(start.0, control_start.0, control_end.0, end.0)
                    .ok_or(SimError::NumericOverflow)?,
            )
            .map_err(|_| SimError::NumericOverflow)?,
            i32::try_from(
                coordinate(start.1, control_start.1, control_end.1, end.1)
                    .ok_or(SimError::NumericOverflow)?,
            )
            .map_err(|_| SimError::NumericOverflow)?,
        );
        if points.last().copied() != Some(point) {
            points.push(point);
        }
    }
    (points.len() >= 2)
        .then_some(points)
        .ok_or(SimError::InvalidTerrain)
}

fn line_to(start: (i32, i32), end: (i32, i32)) -> Result<Vec<(i32, i32)>, SimError> {
    let mut x = start.0;
    let mut y = start.1;
    let dx = (end.0 - start.0).abs();
    let sx = if start.0 < end.0 { 1 } else { -1 };
    let dy = -(end.1 - start.1).abs();
    let sy = if start.1 < end.1 { 1 } else { -1 };
    let mut error = dx.checked_add(dy).ok_or(SimError::NumericOverflow)?;
    let mut points = Vec::new();
    loop {
        points.push((x, y));
        if x == end.0 && y == end.1 {
            break;
        }
        let twice = error.checked_mul(2).ok_or(SimError::NumericOverflow)?;
        if twice >= dy {
            error = error.checked_add(dy).ok_or(SimError::NumericOverflow)?;
            x = x.checked_add(sx).ok_or(SimError::NumericOverflow)?;
        }
        if twice <= dx {
            error = error.checked_add(dx).ok_or(SimError::NumericOverflow)?;
            y = y.checked_add(sy).ok_or(SimError::NumericOverflow)?;
        }
        if points.len() > MAX_CURVE_POINTS {
            return Err(SimError::InvalidTerrain);
        }
    }
    if !points.is_empty() {
        points.remove(0);
    }
    Ok(points)
}

fn meander(
    end_x: i32,
    end_y: i32,
    scale: u8,
    rng: &mut ChaCha8Rng,
    x: &mut i32,
    y: &mut i32,
) -> Result<(), SimError> {
    let distance_x = (end_x - *x).abs();
    let distance_y = (end_y - *y).abs();
    if *x != end_x
        && (inclusive_i32(rng, 0, 215)? < distance_x
            || (inclusive_i32(rng, 0, 35)? > distance_x && inclusive_i32(rng, 0, 35)? > distance_y))
    {
        *x += if end_x > *x { 1 } else { -1 };
    }
    if *y != end_y
        && (inclusive_i32(rng, 0, 215)? < distance_y
            || (inclusive_i32(rng, 0, 35)? > distance_y && inclusive_i32(rng, 0, 35)? > distance_x))
    {
        *y += if end_y > *y { 1 } else { -1 };
    }
    if scale > 1 {
        *x = x
            .checked_add(inclusive_i32(rng, -1, 1)?)
            .ok_or(SimError::NumericOverflow)?;
        *y = y
            .checked_add(inclusive_i32(rng, -1, 1)?)
            .ok_or(SimError::NumericOverflow)?;
    }
    Ok(())
}

fn paint_circle(
    center_x: i32,
    center_y: i32,
    scale: u8,
    water: &mut [bool],
    size: &mut u32,
) -> Result<(), SimError> {
    let radius = i32::from(scale);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let x = center_x.checked_add(dx).ok_or(SimError::NumericOverflow)?;
            let y = center_y.checked_add(dy).ok_or(SimError::NumericOverflow)?;
            if !inbounds(x, y) {
                continue;
            }
            let index = surface_index(x, y)?;
            if !water[index] {
                water[index] = true;
                *size = size.checked_add(1).ok_or(SimError::NumericOverflow)?;
            }
        }
    }
    Ok(())
}

fn polished_river_identity(index: usize, water: &[bool]) -> Result<&'static str, SimError> {
    let (x, y) = index_xy(index)?;
    let adjacent = [
        water_at_or_outside(x, y - 1, water)?,
        water_at_or_outside(x + 1, y, water)?,
        water_at_or_outside(x, y + 1, water)?,
        water_at_or_outside(x - 1, y, water)?,
    ];
    let mask = adjacent
        .iter()
        .enumerate()
        .fold(0_u8, |mask, (bit, present)| {
            mask | (u8::from(*present) << bit)
        });
    if mask == 15 {
        for (dx, dy, identity) in [
            (1, -1, "river_c_not_ne"),
            (1, 1, "river_c_not_se"),
            (-1, 1, "river_c_not_sw"),
            (-1, -1, "river_c_not_nw"),
        ] {
            if inbounds(x + dx, y + dy) && !water[surface_index(x + dx, y + dy)?] {
                return Ok(identity);
            }
        }
    }
    Ok([
        "forest_water",
        "river_south",
        "river_west",
        "river_sw",
        "river_north",
        "forest_water",
        "river_nw",
        "river_west",
        "river_east",
        "river_se",
        "forest_water",
        "river_south",
        "river_ne",
        "river_east",
        "river_north",
        "river_center",
    ][usize::from(mask)])
}

fn water_at_or_outside(x: i32, y: i32, water: &[bool]) -> Result<bool, SimError> {
    if !inbounds(x, y) {
        return Ok(true);
    }
    water
        .get(surface_index(x, y)?)
        .copied()
        .ok_or(SimError::InvalidTerrain)
}

fn major_river_frequency_roll(
    rng: &mut ChaCha8Rng,
    frequency_millis: u32,
    existing_major_rivers: u32,
) -> Result<bool, SimError> {
    let mut numerator = 1_u128;
    let mut denominator = 1_u128;
    for _ in 0..existing_major_rivers.min(32) {
        numerator = numerator
            .checked_mul(1_000)
            .ok_or(SimError::NumericOverflow)?;
        denominator = denominator
            .checked_mul(u128::from(frequency_millis))
            .ok_or(SimError::NumericOverflow)?;
    }
    if numerator >= denominator {
        return Ok(true);
    }
    let ticket = (u128::from(rng.next_u64()) << 64 | u128::from(rng.next_u64())) % denominator;
    Ok(ticket < numerator)
}

fn validate_boundary_position(
    layout: &WorldgenOvermapLayoutV1,
    position: ChunkCoord,
    boundary: OvermapRiverBoundary,
) -> Result<(), SimError> {
    let (x, y) = local_position(layout, position)?;
    let maximum_x = i32::from(WORLDGEN_OVERMAP_WIDTH) - 1;
    let maximum_y = i32::from(WORLDGEN_OVERMAP_HEIGHT) - 1;
    let valid = match boundary {
        OvermapRiverBoundary::North => {
            y == 0 && (RIVER_BORDER..=maximum_x - RIVER_BORDER).contains(&x)
        }
        OvermapRiverBoundary::East => {
            x == maximum_x && (RIVER_BORDER..=maximum_y - RIVER_BORDER).contains(&y)
        }
        OvermapRiverBoundary::South => {
            y == maximum_y && (RIVER_BORDER..=maximum_x - RIVER_BORDER).contains(&x)
        }
        OvermapRiverBoundary::West => {
            x == 0 && (RIVER_BORDER..=maximum_y - RIVER_BORDER).contains(&y)
        }
    };
    valid.then_some(()).ok_or(SimError::InvalidTerrain)
}

fn absolute_position(
    layout: &WorldgenOvermapLayoutV1,
    x: i32,
    y: i32,
) -> Result<ChunkCoord, SimError> {
    if !inbounds(x, y) {
        return Err(SimError::InvalidTerrain);
    }
    Ok(ChunkCoord {
        x: layout
            .origin_x
            .checked_add(x)
            .ok_or(SimError::NumericOverflow)?,
        y: layout
            .origin_y
            .checked_add(y)
            .ok_or(SimError::NumericOverflow)?,
        z: 0,
    })
}

fn local_position(
    layout: &WorldgenOvermapLayoutV1,
    position: ChunkCoord,
) -> Result<(i32, i32), SimError> {
    if position.z != 0 {
        return Err(SimError::InvalidTerrain);
    }
    let x = position
        .x
        .checked_sub(layout.origin_x)
        .ok_or(SimError::NumericOverflow)?;
    let y = position
        .y
        .checked_sub(layout.origin_y)
        .ok_or(SimError::NumericOverflow)?;
    inbounds(x, y)
        .then_some((x, y))
        .ok_or(SimError::InvalidTerrain)
}

fn absolute_to_index(
    layout: &WorldgenOvermapLayoutV1,
    position: ChunkCoord,
) -> Result<usize, SimError> {
    let (x, y) = local_position(layout, position)?;
    surface_index(x, y)
}

fn surface_index(x: i32, y: i32) -> Result<usize, SimError> {
    if !inbounds(x, y) {
        return Err(SimError::InvalidTerrain);
    }
    usize::try_from(y)
        .ok()
        .and_then(|row| row.checked_mul(usize::from(WORLDGEN_OVERMAP_WIDTH)))
        .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
        .ok_or(SimError::NumericOverflow)
}

fn index_xy(index: usize) -> Result<(i32, i32), SimError> {
    let width = usize::from(WORLDGEN_OVERMAP_WIDTH);
    let count = width * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    if index >= count {
        return Err(SimError::InvalidTerrain);
    }
    Ok((
        i32::try_from(index % width).map_err(|_| SimError::NumericOverflow)?,
        i32::try_from(index / width).map_err(|_| SimError::NumericOverflow)?,
    ))
}

fn inbounds(x: i32, y: i32) -> bool {
    (0..i32::from(WORLDGEN_OVERMAP_WIDTH)).contains(&x)
        && (0..i32::from(WORLDGEN_OVERMAP_HEIGHT)).contains(&y)
}

fn inbounds_with_margin(x: i32, y: i32, margin: i32) -> bool {
    (margin..i32::from(WORLDGEN_OVERMAP_WIDTH) - margin).contains(&x)
        && (margin..i32::from(WORLDGEN_OVERMAP_HEIGHT) - margin).contains(&y)
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

fn install_identity(
    identities: &mut Vec<WorldgenOmtIdentityV1>,
    identity: &WorldgenOmtIdentityV1,
) -> Result<(), SimError> {
    if let Some((index, existing)) = identities
        .iter()
        .enumerate()
        .find(|(_, existing)| existing.full_id == identity.full_id)
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

fn inclusive_usize(
    rng: &mut ChaCha8Rng,
    minimum: usize,
    maximum: usize,
) -> Result<usize, SimError> {
    if minimum > maximum {
        return Err(SimError::InvalidTerrain);
    }
    let width = maximum
        .checked_sub(minimum)
        .and_then(|width| width.checked_add(1))
        .ok_or(SimError::NumericOverflow)?;
    let offset = usize::try_from(
        rng.next_u64() % u64::try_from(width).map_err(|_| SimError::NumericOverflow)?,
    )
    .map_err(|_| SimError::NumericOverflow)?;
    minimum.checked_add(offset).ok_or(SimError::NumericOverflow)
}

fn one_in_millis(rng: &mut ChaCha8Rng, denominator_millis: u32) -> bool {
    rng.next_u64() % u64::from(denominator_millis) < 1_000
}

fn river_rng(
    world_seed: [u8; 32],
    generator_version: u16,
    origin_x: i32,
    origin_y: i32,
) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust overmap river placement v1");
    hasher.update(&world_seed);
    hasher.update(&generator_version.to_be_bytes());
    hasher.update(&origin_x.to_be_bytes());
    hasher.update(&origin_y.to_be_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}
