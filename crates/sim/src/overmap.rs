use std::collections::BTreeMap;

use cdda_protocol::{
    ChunkCoord, WorldgenCatalogV1, worldgen_omt_identity_at, worldgen_omt_matches,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use super::{Chunk, SimError, inclusive_rng_u64, mapgen};

/// Selects one source-ordered start target, then returns a deterministic random
/// ordering of every generated OMT matching it. Trying later matching OMTs is a
/// multiplayer extension: a full first cell must not prevent another survivor
/// from joining the same persistent world.
pub(super) fn start_location_omt_order(
    world_seed: [u8; 32],
    next_actor_counter: u64,
    catalog: Option<&WorldgenCatalogV1>,
    chunks: &BTreeMap<ChunkCoord, Chunk>,
) -> Result<Option<Vec<ChunkCoord>>, SimError> {
    let Some(catalog) = catalog else {
        return Ok(None);
    };
    let Some(start) = catalog.start_location.as_ref() else {
        return Ok(None);
    };
    let target_count = u64::try_from(start.targets.len()).map_err(|_| SimError::NumericOverflow)?;
    if target_count == 0 {
        return Err(SimError::InvalidTerrain);
    }
    let mut rng = start_location_rng(
        world_seed,
        catalog.generator_version,
        next_actor_counter,
        &start.start_location_id,
    );
    let target_index = usize::try_from(inclusive_rng_u64(&mut rng, 0, target_count - 1))
        .map_err(|_| SimError::NumericOverflow)?;
    let target = start
        .targets
        .get(target_index)
        .ok_or(SimError::InvalidTerrain)?;
    let mut cells = mapgen::generated_omt_coords(chunks)?;
    cells.retain(|omt| {
        worldgen_omt_identity_at(catalog, *omt)
            .is_some_and(|identity| worldgen_omt_matches(&target.omt, target.match_type, identity))
    });
    for upper in (1..cells.len()).rev() {
        let upper = u64::try_from(upper).map_err(|_| SimError::NumericOverflow)?;
        let chosen = usize::try_from(inclusive_rng_u64(&mut rng, 0, upper))
            .map_err(|_| SimError::NumericOverflow)?;
        cells.swap(
            usize::try_from(upper).map_err(|_| SimError::NumericOverflow)?,
            chosen,
        );
    }
    // Preserve the playable bootstrap only for a uniform single-identity
    // layout. Heterogeneous layouts retain the seeded upstream-style order.
    if catalog.overmap.identities.len() == 1 {
        let origin = ChunkCoord { x: 0, y: 0, z: 0 };
        if let Some(origin_index) = cells.iter().position(|cell| *cell == origin) {
            cells.swap(0, origin_index);
        }
    }
    Ok(Some(cells))
}

fn start_location_rng(
    world_seed: [u8; 32],
    generator_version: u16,
    next_actor_counter: u64,
    start_location_id: &str,
) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust start location selection v1");
    hasher.update(&world_seed);
    hasher.update(&generator_version.to_be_bytes());
    hasher.update(&next_actor_counter.to_be_bytes());
    hasher.update(&(start_location_id.len() as u64).to_be_bytes());
    hasher.update(start_location_id.as_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}
