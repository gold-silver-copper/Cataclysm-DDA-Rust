use std::collections::BTreeMap;

use cdda_protocol::{
    ChunkCoord, MAX_WORLDGEN_CITIES, MAX_WORLDGEN_CITY_SIZE, WORLDGEN_OVERMAP_HEIGHT,
    WORLDGEN_OVERMAP_WIDTH, WorldgenCityId, WorldgenCityV1, WorldgenOmtIdentityV1,
    WorldgenOvermapLayoutV1, WorldgenOvermapRunV1,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use crate::SimError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OvermapCitySettings {
    pub city_size: u8,
    pub city_spacing: u8,
    pub is_megacity: bool,
    /// Pinned biome adjustment for this overmap. The origin overmap is zero.
    pub urbanity: i32,
    /// Pinned biome adjustment truncated exactly where C++ converts it to int.
    pub forestosity: i32,
    pub max_urbanity: i32,
}

impl OvermapCitySettings {
    #[must_use]
    pub const fn core_default(city_size: u8, city_spacing: u8, is_megacity: bool) -> Self {
        Self {
            city_size,
            city_spacing,
            is_megacity,
            urbanity: 0,
            forestosity: 0,
            max_urbanity: 8,
        }
    }
}

/// Places the complete pinned city-center family into one coordinate-owned
/// overmap. Roads and buildings deliberately consume these immutable seeds in
/// the next family; this function owns density, size variation, spacing,
/// megacity centers, stable identity, and center-terrain replacement.
pub fn place_overmap_cities(
    world_seed: [u8; 32],
    generator_version: u16,
    mut layout: WorldgenOvermapLayoutV1,
    settings: OvermapCitySettings,
    default_surface_full_id: &str,
    city_center_identity: WorldgenOmtIdentityV1,
) -> Result<(WorldgenOvermapLayoutV1, Vec<WorldgenCityV1>), SimError> {
    if settings.city_size > 16
        || settings.city_spacing > 8
        || settings.max_urbanity <= 0
        || layout.layers.iter().filter(|layer| layer.z == 0).count() != 1
    {
        return Err(SimError::InvalidTerrain);
    }
    if settings.city_size == 0 {
        return Ok((layout, Vec::new()));
    }

    let base_size = i32::from(settings.city_size);
    // Preserve the pinned comparison and truncation order, including its
    // lower-bound clamp after the urban/forest adjustment.
    let city_size_adjust = (settings.urbanity - settings.forestosity / 2).min(-base_size + 2);
    let mut maximum_size =
        (base_size + city_size_adjust).min(base_size.saturating_mul(settings.max_urbanity));
    if maximum_size < base_size {
        maximum_size = base_size;
    }
    let mut spacing = i32::from(settings.city_spacing);
    if spacing > 0 {
        let spacing_adjust = (settings.urbanity / 2).min(spacing - 2);
        spacing = spacing - spacing_adjust + settings.forestosity;
    }
    spacing = spacing.min(10);
    if spacing < 0 || maximum_size <= 0 || maximum_size >= 90 {
        return Err(SimError::InvalidTerrain);
    }

    let surface = expand_surface(&layout)?;
    let radius = 90_i32
        .checked_sub(maximum_size)
        .ok_or(SimError::NumericOverflow)?;
    let mut candidates = Vec::new();
    for local_y in (90 - radius)..=(90 + radius) {
        for local_x in (90 - radius)..=(90 + radius) {
            let index = surface_index(local_x, local_y)?;
            let identity = layout
                .identities
                .get(usize::from(surface[index]))
                .ok_or(SimError::InvalidTerrain)?;
            if identity.full_id == default_surface_full_id {
                candidates.push((local_x, local_y));
            }
        }
    }

    let mut rng = city_rng(
        world_seed,
        generator_version,
        layout.origin_x,
        layout.origin_y,
    );
    let centers = if settings.is_megacity {
        vec![
            (45, 45, 40),
            (45, 135, 40),
            (90, 90, 40),
            (135, 45, 40),
            (135, 135, 40),
        ]
    } else {
        let count = random_city_count(base_size, maximum_size, spacing, &mut rng)?;
        let mut centers = Vec::with_capacity(count);
        while centers.len() < count && !candidates.is_empty() {
            let mut size = inclusive_i32(&mut rng, base_size - 1, maximum_size)?;
            if one_in(&mut rng, 3) {
                size /= 3;
            } else if one_in(&mut rng, 2) {
                size = size.saturating_mul(2) / 3;
            } else if one_in(&mut rng, 2) {
                size = size.saturating_mul(3) / 2;
            } else {
                size = size.saturating_mul(2);
            }
            size = size.clamp(2, i32::from(MAX_WORLDGEN_CITY_SIZE));
            let selected = inclusive_index(&mut rng, candidates.len())?;
            let (x, y) = candidates.remove(selected);
            candidates.retain(|(candidate_x, candidate_y)| {
                (candidate_x - x).abs() > 2 || (candidate_y - y).abs() > 2
            });
            centers.push((x, y, size));
        }
        centers
    };
    if centers.len() > MAX_WORLDGEN_CITIES {
        return Err(SimError::InvalidTerrain);
    }

    let cities = centers
        .iter()
        .enumerate()
        .map(|(index, (local_x, local_y, size))| {
            Ok(WorldgenCityV1 {
                city_id: WorldgenCityId(
                    u32::try_from(index + 1).map_err(|_| SimError::NumericOverflow)?,
                ),
                center: ChunkCoord {
                    x: layout
                        .origin_x
                        .checked_add(*local_x)
                        .ok_or(SimError::NumericOverflow)?,
                    y: layout
                        .origin_y
                        .checked_add(*local_y)
                        .ok_or(SimError::NumericOverflow)?,
                    z: 0,
                },
                size: u8::try_from(*size).map_err(|_| SimError::NumericOverflow)?,
            })
        })
        .collect::<Result<Vec<_>, SimError>>()?;
    paint_centers(&mut layout, &cities, city_center_identity)?;
    Ok((layout, cities))
}

fn random_city_count(
    base_size: i32,
    maximum_size: i32,
    spacing: i32,
    rng: &mut ChaCha8Rng,
) -> Result<usize, SimError> {
    // Algebraic form of pinned `roll_remainder(32400 / 2^spacing /
    // (((2*a+1)*(2*b+1)*3)/4))`, avoiding cross-platform floating rounding.
    let numerator = 4_u64
        .checked_mul(u64::from(WORLDGEN_OVERMAP_WIDTH))
        .and_then(|value| value.checked_mul(u64::from(WORLDGEN_OVERMAP_HEIGHT)))
        .ok_or(SimError::NumericOverflow)?;
    let coverage = 1_u64
        .checked_shl(u32::try_from(spacing).map_err(|_| SimError::NumericOverflow)?)
        .ok_or(SimError::NumericOverflow)?;
    let width = u64::try_from(
        base_size
            .checked_mul(2)
            .and_then(|v| v.checked_add(1))
            .ok_or(SimError::NumericOverflow)?,
    )
    .map_err(|_| SimError::NumericOverflow)?;
    let maximum_width = u64::try_from(
        maximum_size
            .checked_mul(2)
            .and_then(|v| v.checked_add(1))
            .ok_or(SimError::NumericOverflow)?,
    )
    .map_err(|_| SimError::NumericOverflow)?;
    let denominator = coverage
        .checked_mul(width)
        .and_then(|value| value.checked_mul(maximum_width))
        .and_then(|value| value.checked_mul(3))
        .ok_or(SimError::NumericOverflow)?;
    let whole = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = whole + u64::from(remainder > 0 && rng.next_u64() % denominator < remainder);
    usize::try_from(rounded).map_err(|_| SimError::NumericOverflow)
}

fn expand_surface(layout: &WorldgenOvermapLayoutV1) -> Result<Vec<u16>, SimError> {
    let layer = layout
        .layers
        .iter()
        .find(|layer| layer.z == 0)
        .ok_or(SimError::InvalidTerrain)?;
    let expected = usize::from(WORLDGEN_OVERMAP_WIDTH) * usize::from(WORLDGEN_OVERMAP_HEIGHT);
    let mut cells = Vec::with_capacity(expected);
    for run in &layer.runs {
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
            .is_none_or(|total| total > expected)
        {
            return Err(SimError::InvalidTerrain);
        }
        cells.extend(std::iter::repeat_n(run.identity_index, length));
    }
    (cells.len() == expected)
        .then_some(cells)
        .ok_or(SimError::InvalidTerrain)
}

fn paint_centers(
    layout: &mut WorldgenOvermapLayoutV1,
    cities: &[WorldgenCityV1],
    center_identity: WorldgenOmtIdentityV1,
) -> Result<(), SimError> {
    let mut identities = layout.identities.clone();
    if let Some(existing) = identities
        .iter()
        .find(|identity| identity.full_id == center_identity.full_id)
    {
        if existing != &center_identity {
            return Err(SimError::InvalidTerrain);
        }
    } else {
        identities.push(center_identity.clone());
    }
    identities.sort_by(|left, right| left.full_id.cmp(&right.full_id));
    let indices = identities
        .iter()
        .enumerate()
        .map(|(index, identity)| {
            Ok((
                identity.full_id.as_str(),
                u16::try_from(index).map_err(|_| SimError::NumericOverflow)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, SimError>>()?;
    let old_to_new = layout
        .identities
        .iter()
        .map(|identity| {
            indices
                .get(identity.full_id.as_str())
                .copied()
                .ok_or(SimError::InvalidTerrain)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let center_index = *indices
        .get(center_identity.full_id.as_str())
        .ok_or(SimError::InvalidTerrain)?;
    for layer in &mut layout.layers {
        let mut cells = Vec::new();
        for run in &layer.runs {
            let mapped = *old_to_new
                .get(usize::from(run.identity_index))
                .ok_or(SimError::InvalidTerrain)?;
            cells.extend(std::iter::repeat_n(
                mapped,
                usize::try_from(run.length).map_err(|_| SimError::NumericOverflow)?,
            ));
        }
        if layer.z == 0 {
            for city in cities {
                let local_x = city
                    .center
                    .x
                    .checked_sub(layout.origin_x)
                    .ok_or(SimError::NumericOverflow)?;
                let local_y = city
                    .center
                    .y
                    .checked_sub(layout.origin_y)
                    .ok_or(SimError::NumericOverflow)?;
                let index = surface_index(local_x, local_y)?;
                *cells.get_mut(index).ok_or(SimError::InvalidTerrain)? = center_index;
            }
        }
        layer.runs = encode_runs(&cells)?;
    }
    layout.identities = identities;
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

fn inclusive_i32(rng: &mut ChaCha8Rng, minimum: i32, maximum: i32) -> Result<i32, SimError> {
    if minimum > maximum {
        return Err(SimError::InvalidTerrain);
    }
    let width = i64::from(maximum) - i64::from(minimum) + 1;
    let offset = rng.next_u64() % u64::try_from(width).map_err(|_| SimError::NumericOverflow)?;
    i32::try_from(
        i64::from(minimum) + i64::try_from(offset).map_err(|_| SimError::NumericOverflow)?,
    )
    .map_err(|_| SimError::NumericOverflow)
}

fn inclusive_index(rng: &mut ChaCha8Rng, length: usize) -> Result<usize, SimError> {
    if length == 0 {
        return Err(SimError::InvalidTerrain);
    }
    let length = u64::try_from(length).map_err(|_| SimError::NumericOverflow)?;
    usize::try_from(rng.next_u64() % length).map_err(|_| SimError::NumericOverflow)
}

fn one_in(rng: &mut ChaCha8Rng, chance: u64) -> bool {
    rng.next_u64().is_multiple_of(chance)
}

fn city_rng(
    world_seed: [u8; 32],
    generator_version: u16,
    origin_x: i32,
    origin_y: i32,
) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust overmap city placement v1");
    hasher.update(&world_seed);
    hasher.update(&generator_version.to_be_bytes());
    hasher.update(&origin_x.to_be_bytes());
    hasher.update(&origin_y.to_be_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cdda_protocol::{WorldgenOvermapLayerV1, worldgen_omt_identity_at};

    fn identity(id: &str) -> WorldgenOmtIdentityV1 {
        WorldgenOmtIdentityV1 {
            full_id: id.to_owned(),
            type_id: id.to_owned(),
            subtype_id: id.to_owned(),
            generator_id: id.to_owned(),
            rotation: 0,
        }
    }

    fn field_layout() -> WorldgenOvermapLayoutV1 {
        WorldgenOvermapLayoutV1 {
            origin_x: -90,
            origin_y: -90,
            identities: vec![identity("field")],
            layers: vec![WorldgenOvermapLayerV1 {
                z: 0,
                runs: vec![WorldgenOvermapRunV1 {
                    identity_index: 0,
                    length: u32::from(WORLDGEN_OVERMAP_WIDTH) * u32::from(WORLDGEN_OVERMAP_HEIGHT),
                }],
            }],
        }
    }

    #[test]
    fn default_city_family_is_deterministic_spaced_and_painted() {
        let generate = || {
            place_overmap_cities(
                [37; 32],
                2,
                field_layout(),
                OvermapCitySettings::core_default(8, 4, false),
                "field",
                identity("road_nesw"),
            )
            .expect("cities")
        };
        let (layout, cities) = generate();
        assert_eq!((layout.clone(), cities.clone()), generate());
        assert!((9..=10).contains(&cities.len()));
        for (index, city) in cities.iter().enumerate() {
            assert_eq!(
                city.city_id,
                WorldgenCityId(u32::try_from(index + 1).expect("bounded city index"))
            );
            assert!((2..=55).contains(&city.size));
            for other in cities.iter().skip(index + 1) {
                assert!(
                    (city.center.x - other.center.x).abs() > 2
                        || (city.center.y - other.center.y).abs() > 2
                );
            }
            let catalog = cdda_protocol::WorldgenCatalogV1 {
                generator_version: 2,
                overmap: layout.clone(),
                cities: cities.clone(),
                rivers: Vec::new(),
                specials: Vec::new(),
                start_location: None,
                terrain_prototypes: Vec::new(),
                furniture_prototypes: Vec::new(),
                monster_prototypes: Vec::new(),
                monster_groups: Vec::new(),
                regional_terrain: Vec::new(),
                regional_furniture: Vec::new(),
                npc_name_categories: Vec::new(),
                omt_generators: Vec::new(),
            };
            assert_eq!(
                worldgen_omt_identity_at(&catalog, city.center)
                    .expect("painted center")
                    .full_id,
                "road_nesw"
            );
        }
    }

    #[test]
    fn no_cities_and_megacity_are_complete_branches() {
        let (unchanged, none) = place_overmap_cities(
            [1; 32],
            2,
            field_layout(),
            OvermapCitySettings::core_default(0, 0, false),
            "field",
            identity("road_nesw"),
        )
        .expect("no cities");
        assert!(none.is_empty());
        assert_eq!(unchanged, field_layout());

        let (_layout, cities) = place_overmap_cities(
            [2; 32],
            2,
            field_layout(),
            OvermapCitySettings::core_default(8, 4, true),
            "field",
            identity("road_nesw"),
        )
        .expect("megacity");
        assert_eq!(cities.len(), 5);
        assert!(cities.iter().all(|city| city.size == 40));
        assert_eq!(cities[2].center, ChunkCoord { x: 0, y: 0, z: 0 });
    }
}
