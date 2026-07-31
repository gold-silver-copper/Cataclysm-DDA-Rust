use cdda_protocol::{
    ChunkCoord, FurnitureTileSnapshot, ItemGroupDefinitionV1, ItemGroupSourceV1,
    MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH, WorldPosition, WorldgenCatalogV1,
    WorldgenFurniturePrototypeTargetV1, WorldgenFurnitureTargetV1, WorldgenNestedConditionsV1,
    WorldgenNestedGeneratorV1, WorldgenNestedPlacementV1, WorldgenNestedTemplateV1,
    WorldgenOmtGeneratorV1, WorldgenTerrainTargetV1, WorldgenWeightedFurniturePrototypeV1,
    WorldgenWeightedFurnitureTargetV1, WorldgenWeightedPrototypeV1,
    WorldgenWeightedTerrainTargetV1, item_group_source_max_outputs, worldgen_city_start_distance,
    worldgen_omt_identity_at, worldgen_omt_matches, worldgen_overmap_contains,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use super::{
    Chunk, ID_RESERVATION_SIZE, PlannedItemSpawn, SUBMAP_SIZE, SimError, inclusive_rng_u64,
    plan_item_group_source,
};

const OMT_SUBMAP_WIDTH: i32 = 2;
pub(super) const OMT_TILE_WIDTH: usize = (SUBMAP_SIZE as usize) * (OMT_SUBMAP_WIDTH as usize);
const OMT_TILE_COUNT: usize = OMT_TILE_WIDTH * OMT_TILE_WIDTH;

pub(super) fn catalog_fits_one_id_reservation(
    catalog: &WorldgenCatalogV1,
    item_groups: &[ItemGroupDefinitionV1],
    _radius_submaps: i32,
) -> bool {
    // Every placement must fit atomically. The aggregate worst case is not a
    // useful admission bound for low-chance production groups: planning uses
    // actual deterministic rolls, caps the complete bubble before mutation,
    // and the allocator still rejects any exhausted reservation at commit.
    let placement_fits = |placement: &cdda_protocol::WorldgenItemGroupPlacementV1| {
        item_group_source_max_outputs(
            &ItemGroupSourceV1::Group(placement.group_id.clone()),
            item_groups,
        )
        .and_then(|maximum| maximum.checked_mul(u64::from(placement.repeat_maximum)))
        .is_some_and(|maximum| maximum <= ID_RESERVATION_SIZE)
    };
    catalog.omt_generators.iter().all(|generator| {
        generator.templates.iter().all(|template| {
            template
                .cells
                .iter()
                .filter_map(|cell| cell.item_group.as_ref())
                .all(&placement_fits)
                && template
                    .area_items
                    .iter()
                    .all(|placement| placement_fits(&placement.item_group))
        }) && generator.nested_generators.iter().all(|nested| {
            nested.templates.iter().all(|template| {
                template
                    .cells
                    .iter()
                    .filter_map(|cell| cell.item_group.as_ref())
                    .all(&placement_fits)
                    && template
                        .area_items
                        .iter()
                        .all(|placement| placement_fits(&placement.item_group))
            })
        })
    })
}

pub(super) struct PlannedBubble {
    pub chunks: Vec<Chunk>,
    pub items: Vec<(WorldPosition, PlannedItemSpawn)>,
}

pub(super) fn catalog_initial_bubble_is_admissible(
    catalog: &WorldgenCatalogV1,
    center: ChunkCoord,
    radius_submaps: i32,
) -> bool {
    let Some(minimum_submap_x) = center.x.checked_sub(radius_submaps) else {
        return false;
    };
    let Some(maximum_submap_x) = center.x.checked_add(radius_submaps) else {
        return false;
    };
    let Some(minimum_submap_y) = center.y.checked_sub(radius_submaps) else {
        return false;
    };
    let Some(maximum_submap_y) = center.y.checked_add(radius_submaps) else {
        return false;
    };
    let minimum_omt_x = minimum_submap_x.div_euclid(OMT_SUBMAP_WIDTH);
    let maximum_omt_x = maximum_submap_x.div_euclid(OMT_SUBMAP_WIDTH);
    let minimum_omt_y = minimum_submap_y.div_euclid(OMT_SUBMAP_WIDTH);
    let maximum_omt_y = maximum_submap_y.div_euclid(OMT_SUBMAP_WIDTH);
    let mut candidates = Vec::new();
    for y in minimum_omt_y..=maximum_omt_y {
        for x in minimum_omt_x..=maximum_omt_x {
            let Some(identity) = worldgen_omt_identity_at(catalog, ChunkCoord { x, y, z: 0 })
            else {
                return false;
            };
            candidates.push((ChunkCoord { x, y, z: 0 }, identity));
        }
    }
    let Some(start) = catalog.start_location.as_ref() else {
        return true;
    };
    if start.requires_city() {
        catalog.cities.iter().any(|city| {
            start.city_sizes.contains(i32::from(city.size))
                && candidates.iter().any(|(omt, identity)| {
                    let dx = i64::from(omt.x) - i64::from(city.center.x);
                    let dy = i64::from(omt.y) - i64::from(city.center.y);
                    if dx.abs() > i64::from(city.size) || dy.abs() > i64::from(city.size) {
                        return false;
                    }
                    let edge_distance = worldgen_city_start_distance(city, *omt);
                    start.city_distance.contains(edge_distance)
                        && start.targets.iter().any(|target| {
                            worldgen_omt_matches(&target.omt, target.match_type, identity)
                        })
                })
        })
    } else {
        start.targets.iter().all(|target| {
            candidates.iter().any(|(_omt, identity)| {
                worldgen_omt_matches(&target.omt, target.match_type, identity)
            })
        })
    }
}

pub(super) fn plan_active_bubble(
    world_seed: [u8; 32],
    catalog: &WorldgenCatalogV1,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    existing: &std::collections::BTreeMap<ChunkCoord, Chunk>,
    center: ChunkCoord,
    radius_submaps: i32,
) -> Result<Option<PlannedBubble>, SimError> {
    let minimum_submap_x = center
        .x
        .checked_sub(radius_submaps)
        .ok_or(SimError::NumericOverflow)?;
    let maximum_submap_x = center
        .x
        .checked_add(radius_submaps)
        .ok_or(SimError::NumericOverflow)?;
    let minimum_submap_y = center
        .y
        .checked_sub(radius_submaps)
        .ok_or(SimError::NumericOverflow)?;
    let maximum_submap_y = center
        .y
        .checked_add(radius_submaps)
        .ok_or(SimError::NumericOverflow)?;
    let minimum_omt_x = minimum_submap_x.div_euclid(OMT_SUBMAP_WIDTH);
    let maximum_omt_x = maximum_submap_x.div_euclid(OMT_SUBMAP_WIDTH);
    let minimum_omt_y = minimum_submap_y.div_euclid(OMT_SUBMAP_WIDTH);
    let maximum_omt_y = maximum_submap_y.div_euclid(OMT_SUBMAP_WIDTH);

    // Plan every missing cell before committing any of them. A bad catalog,
    // overflow, or partially materialized 2x2 cell therefore cannot leave a
    // half-generated active bubble behind.
    let mut planned = PlannedBubble {
        chunks: Vec::new(),
        items: Vec::new(),
    };
    for omt_y in minimum_omt_y..=maximum_omt_y {
        for omt_x in minimum_omt_x..=maximum_omt_x {
            let omt = ChunkCoord {
                x: omt_x,
                y: omt_y,
                z: center.z,
            };
            if worldgen_omt_identity_at(catalog, omt).is_none() {
                return Ok(None);
            }
            let chunk_coords = omt_chunk_coords(omt)?;
            let present = chunk_coords
                .iter()
                .filter(|coord| existing.contains_key(coord))
                .count();
            match present {
                0 => {
                    let cell = plan_omt_cell(world_seed, catalog, item_groups, omt)?;
                    planned.chunks.extend(cell.chunks);
                    planned.items.extend(cell.items);
                    if planned.items.len() > ID_RESERVATION_SIZE as usize {
                        return Err(SimError::InvalidItem);
                    }
                }
                4 => {}
                _ => return Err(SimError::InvalidTerrain),
            }
        }
    }
    Ok(Some(planned))
}

pub(super) fn generated_cells_match_layout(
    catalog: &WorldgenCatalogV1,
    chunks: &std::collections::BTreeMap<ChunkCoord, Chunk>,
) -> bool {
    for coord in chunks.keys() {
        let omt = ChunkCoord {
            x: coord.x.div_euclid(OMT_SUBMAP_WIDTH),
            y: coord.y.div_euclid(OMT_SUBMAP_WIDTH),
            z: coord.z,
        };
        if !worldgen_overmap_contains(catalog, omt) {
            return false;
        }
        let Ok(cell) = omt_chunk_coords(omt) else {
            return false;
        };
        if cell.iter().any(|sibling| !chunks.contains_key(sibling)) {
            return false;
        }
    }
    true
}

pub(super) fn generated_omt_coords(
    chunks: &std::collections::BTreeMap<ChunkCoord, Chunk>,
) -> Result<Vec<ChunkCoord>, SimError> {
    let mut cells = std::collections::BTreeSet::new();
    for coord in chunks.keys().filter(|coord| coord.z == 0) {
        let omt = ChunkCoord {
            x: coord.x.div_euclid(OMT_SUBMAP_WIDTH),
            y: coord.y.div_euclid(OMT_SUBMAP_WIDTH),
            z: coord.z,
        };
        if omt_chunk_coords(omt)?
            .iter()
            .any(|sibling| !chunks.contains_key(sibling))
        {
            return Err(SimError::InvalidTerrain);
        }
        cells.insert(omt);
    }
    Ok(cells.into_iter().collect())
}

fn plan_omt_cell(
    world_seed: [u8; 32],
    catalog: &WorldgenCatalogV1,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    omt: ChunkCoord,
) -> Result<PlannedBubble, SimError> {
    let identity = worldgen_omt_identity_at(catalog, omt).ok_or(SimError::InvalidTerrain)?;
    let generator = catalog
        .omt_generators
        .iter()
        .find(|generator| generator.omt_id == identity.generator_id)
        .ok_or(SimError::InvalidTerrain)?;
    let mut rng = coordinate_rng(
        world_seed,
        catalog.generator_version,
        omt,
        &generator.omt_id,
    );
    let mut plan = OmtMapgenPlan::new();
    apply_root_generator(
        catalog,
        generator,
        item_groups,
        omt,
        identity.rotation,
        &mut rng,
        &mut plan,
        0,
    )?;

    // Regional pseudo terrain/furniture resolves only after all mapgen phases,
    // including nested placement and ordinary loot planning.
    let mut terrain = Vec::with_capacity(OMT_TILE_COUNT);
    let mut furniture = Vec::with_capacity(OMT_TILE_COUNT);
    for (terrain_target, furniture_target) in plan.terrain.iter().zip(plan.furniture.iter()) {
        let terrain_target = terrain_target.as_ref().ok_or(SimError::InvalidTerrain)?;
        let furniture_target = furniture_target
            .as_ref()
            .ok_or(SimError::InvalidFurniture)?;
        terrain.push(resolve_terrain(catalog, terrain_target, &mut rng)?);
        furniture.push(resolve_furniture(catalog, furniture_target, &mut rng)?);
    }
    let mut items = Vec::with_capacity(plan.items.len());
    for (index, prototype) in plan.items {
        if terrain[index].move_cost <= 0
            || furniture[index]
                .as_ref()
                .is_some_and(|furniture| furniture.move_cost_mod < 0)
        {
            return Err(SimError::InvalidTerrain);
        }
        let (x, y) = rotate_tile_xy(
            index % OMT_TILE_WIDTH,
            index / OMT_TILE_WIDTH,
            identity.rotation,
        )?;
        items.push((omt_tile_position(omt, x, y)?, prototype));
    }
    let (terrain, furniture) = rotate_tiles(terrain, furniture, identity.rotation)?;
    Ok(PlannedBubble {
        chunks: chunks_from_tiles(omt, terrain, furniture)?.into(),
        items,
    })
}

struct OmtMapgenPlan {
    terrain: Vec<Option<WorldgenTerrainTargetV1>>,
    furniture: Vec<Option<WorldgenFurnitureTargetV1>>,
    items: Vec<(usize, PlannedItemSpawn)>,
}

impl OmtMapgenPlan {
    fn new() -> Self {
        Self {
            terrain: vec![None; OMT_TILE_COUNT],
            furniture: vec![None; OMT_TILE_COUNT],
            items: Vec::new(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_root_generator(
    catalog: &WorldgenCatalogV1,
    generator: &WorldgenOmtGeneratorV1,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    omt: ChunkCoord,
    rotation: u8,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
    depth: usize,
) -> Result<(), SimError> {
    if depth >= cdda_protocol::MAX_WORLDGEN_NESTED_DEPTH {
        return Err(SimError::InvalidTerrain);
    }
    let template = choose_template(&generator.templates, rng)?;
    if let Some(predecessor_id) = &template.predecessor_id {
        let predecessor = catalog
            .omt_generators
            .binary_search_by(|candidate| candidate.omt_id.as_str().cmp(predecessor_id.as_str()))
            .ok()
            .and_then(|index| catalog.omt_generators.get(index))
            .ok_or(SimError::InvalidTerrain)?;
        apply_root_generator(
            catalog,
            predecessor,
            item_groups,
            omt,
            rotation,
            rng,
            plan,
            depth + 1,
        )?;
        rotate_mapgen_plan(plan, (4 - rotation) % 4)?;
    }
    apply_template_body(
        catalog,
        generator,
        item_groups,
        omt,
        rotation,
        template.predecessor_id.as_deref(),
        &template.cells,
        OMT_TILE_WIDTH,
        OMT_TILE_WIDTH,
        &template.nested,
        &template.area_items,
        template.erase_all_before_placing_terrain,
        0,
        0,
        rng,
        plan,
        depth,
    )
}

fn rotate_mapgen_plan(plan: &mut OmtMapgenPlan, rotation: u8) -> Result<(), SimError> {
    if rotation == 0 {
        return Ok(());
    }
    let mut terrain = vec![None; OMT_TILE_COUNT];
    let mut furniture = vec![None; OMT_TILE_COUNT];
    for source in 0..OMT_TILE_COUNT {
        let (target_x, target_y) =
            rotate_tile_xy(source % OMT_TILE_WIDTH, source / OMT_TILE_WIDTH, rotation)?;
        let target = target_y * OMT_TILE_WIDTH + target_x;
        terrain[target] = plan.terrain[source];
        furniture[target] = plan.furniture[source];
    }
    for (index, _) in &mut plan.items {
        let (target_x, target_y) =
            rotate_tile_xy(*index % OMT_TILE_WIDTH, *index / OMT_TILE_WIDTH, rotation)?;
        *index = target_y * OMT_TILE_WIDTH + target_x;
    }
    plan.terrain = terrain;
    plan.furniture = furniture;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_template_body(
    catalog: &WorldgenCatalogV1,
    root_generator: &WorldgenOmtGeneratorV1,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    omt: ChunkCoord,
    rotation: u8,
    predecessor_id: Option<&str>,
    cells: &[cdda_protocol::WorldgenCellV1],
    width: usize,
    height: usize,
    nested: &[WorldgenNestedPlacementV1],
    area_items: &[cdda_protocol::WorldgenAreaItemPlacementV1],
    erase_all_before_placing_terrain: bool,
    offset_x: i32,
    offset_y: i32,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
    depth: usize,
) -> Result<(), SimError> {
    if cells.len() != width.checked_mul(height).ok_or(SimError::NumericOverflow)? {
        return Err(SimError::InvalidTerrain);
    }

    // Terrain and furniture are separate upstream phases. Empty vectors are
    // overlay no-ops, while an explicit furniture `None` clears the tile.
    let terrain_layers = cells
        .iter()
        .map(|cell| cell.terrain.len())
        .max()
        .unwrap_or(0);
    for layer_index in 0..terrain_layers {
        for (source, cell) in cells.iter().enumerate() {
            let Some(layer) = cell.terrain.get(layer_index) else {
                continue;
            };
            let Some(index) = offset_tile_index(source, width, offset_x, offset_y)? else {
                continue;
            };
            if erase_all_before_placing_terrain {
                plan.furniture[index] = Some(WorldgenFurnitureTargetV1::None);
                plan.items.retain(|(item_index, _)| *item_index != index);
            }
            plan.terrain[index] = Some(choose_terrain_target(layer, rng)?);
        }
    }
    let furniture_layers = cells
        .iter()
        .map(|cell| cell.furniture.len())
        .max()
        .unwrap_or(0);
    for layer_index in 0..furniture_layers {
        for (source, cell) in cells.iter().enumerate() {
            let Some(layer) = cell.furniture.get(layer_index) else {
                continue;
            };
            let Some(index) = offset_tile_index(source, width, offset_x, offset_y)? else {
                continue;
            };
            plan.furniture[index] = Some(choose_furniture_target(layer, rng)?);
        }
    }

    for placement in nested {
        apply_nested_placement(
            catalog,
            root_generator,
            item_groups,
            omt,
            rotation,
            predecessor_id,
            placement,
            offset_x,
            offset_y,
            rng,
            plan,
            depth + 1,
        )?;
    }

    for (source, cell) in cells.iter().enumerate() {
        let Some(placement) = &cell.item_group else {
            continue;
        };
        let Some(index) = offset_tile_index(source, width, offset_x, offset_y)? else {
            continue;
        };
        plan_item_placement(placement, index, item_groups, rng, plan)?;
    }
    for placement in area_items {
        let x = choose_i8_range(placement.x.minimum, placement.x.maximum, rng)?;
        let y = choose_i8_range(placement.y.minimum, placement.y.maximum, rng)?;
        let Some(index) = absolute_tile_index(
            offset_x
                .checked_add(i32::from(x))
                .ok_or(SimError::NumericOverflow)?,
            offset_y
                .checked_add(i32::from(y))
                .ok_or(SimError::NumericOverflow)?,
        ) else {
            continue;
        };
        plan_item_placement(&placement.item_group, index, item_groups, rng, plan)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_nested_placement(
    catalog: &WorldgenCatalogV1,
    root_generator: &WorldgenOmtGeneratorV1,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    omt: ChunkCoord,
    rotation: u8,
    predecessor_id: Option<&str>,
    placement: &WorldgenNestedPlacementV1,
    parent_offset_x: i32,
    parent_offset_y: i32,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
    depth: usize,
) -> Result<(), SimError> {
    if depth >= cdda_protocol::MAX_WORLDGEN_NESTED_DEPTH {
        return Err(SimError::InvalidTerrain);
    }
    let choices = if nested_conditions_match(
        catalog,
        omt,
        rotation,
        predecessor_id,
        &placement.conditions,
    )? {
        &placement.chunks
    } else {
        &placement.else_chunks
    };
    if choices.is_empty() {
        return Ok(());
    }
    let choice = choose_nested_choice(choices, rng)?;
    if choice.nested_id == "null" {
        return Ok(());
    }
    let nested_generator = root_generator
        .nested_generators
        .binary_search_by(|candidate| candidate.nested_id.as_str().cmp(choice.nested_id.as_str()))
        .ok()
        .and_then(|index| root_generator.nested_generators.get(index))
        .ok_or(SimError::InvalidTerrain)?;
    let template = choose_nested_template(nested_generator, rng)?;
    let x = choose_i8_range(placement.x.minimum, placement.x.maximum, rng)?;
    let y = choose_i8_range(placement.y.minimum, placement.y.maximum, rng)?;
    apply_template_body(
        catalog,
        root_generator,
        item_groups,
        omt,
        rotation,
        predecessor_id,
        &template.cells,
        usize::from(template.width),
        usize::from(template.height),
        &template.nested,
        &template.area_items,
        template.erase_all_before_placing_terrain,
        parent_offset_x
            .checked_add(i32::from(x))
            .ok_or(SimError::NumericOverflow)?,
        parent_offset_y
            .checked_add(i32::from(y))
            .ok_or(SimError::NumericOverflow)?,
        rng,
        plan,
        depth,
    )
}

fn nested_conditions_match(
    catalog: &WorldgenCatalogV1,
    omt: ChunkCoord,
    rotation: u8,
    predecessor_id: Option<&str>,
    conditions: &WorldgenNestedConditionsV1,
) -> Result<bool, SimError> {
    let matches = |condition: &cdda_protocol::WorldgenNeighborConditionV1| {
        let (offset_x, offset_y) = rotate_neighbor_offset(
            i32::from(condition.offset_x),
            i32::from(condition.offset_y),
            rotation,
        )?;
        let neighbor = ChunkCoord {
            x: omt
                .x
                .checked_add(offset_x)
                .ok_or(SimError::NumericOverflow)?,
            y: omt
                .y
                .checked_add(offset_y)
                .ok_or(SimError::NumericOverflow)?,
            z: omt.z,
        };
        Ok(
            worldgen_omt_identity_at(catalog, neighbor).is_some_and(|identity| {
                condition
                    .allowed_identity_ids
                    .binary_search(&identity.full_id)
                    .is_ok()
            }),
        )
    };
    for condition in &conditions.all_neighbors {
        if !matches(condition)? {
            return Ok(false);
        }
    }
    if !conditions.any_neighbors.is_empty() {
        let mut any = false;
        for condition in &conditions.any_neighbors {
            any |= matches(condition)?;
        }
        if !any {
            return Ok(false);
        }
    }
    Ok(conditions.predecessor_ids.is_empty()
        || predecessor_id.is_some_and(|id| {
            conditions
                .predecessor_ids
                .binary_search_by(|candidate| candidate.as_str().cmp(id))
                .is_ok()
        }))
}

fn rotate_neighbor_offset(x: i32, y: i32, rotation: u8) -> Result<(i32, i32), SimError> {
    match rotation {
        0 => Ok((x, y)),
        1 => Ok((-y, x)),
        2 => Ok((-x, -y)),
        3 => Ok((y, -x)),
        _ => Err(SimError::InvalidTerrain),
    }
}

fn choose_nested_choice<'a>(
    choices: &'a [cdda_protocol::WorldgenNestedChoiceV1],
    rng: &mut ChaCha8Rng,
) -> Result<&'a cdda_protocol::WorldgenNestedChoiceV1, SimError> {
    let total = choices.iter().try_fold(0_u64, |total, choice| {
        total.checked_add(u64::from(choice.weight))
    });
    let index = choose_weighted_index(
        choices.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(choices[index].weight),
        true,
    )?;
    choices.get(index).ok_or(SimError::InvalidTerrain)
}

fn choose_nested_template<'a>(
    generator: &'a WorldgenNestedGeneratorV1,
    rng: &mut ChaCha8Rng,
) -> Result<&'a WorldgenNestedTemplateV1, SimError> {
    let total = generator
        .templates
        .iter()
        .try_fold(0_u64, |total, template| {
            total.checked_add(u64::from(template.weight))
        });
    let index = choose_weighted_index(
        generator.templates.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(generator.templates[index].weight),
        true,
    )?;
    generator
        .templates
        .get(index)
        .ok_or(SimError::InvalidTerrain)
}

fn choose_i8_range(minimum: i8, maximum: i8, rng: &mut ChaCha8Rng) -> Result<i8, SimError> {
    if minimum > maximum {
        return Err(SimError::InvalidTerrain);
    }
    if minimum == maximum {
        return Ok(minimum);
    }
    let width = i64::from(maximum)
        .checked_sub(i64::from(minimum))
        .and_then(|value| value.checked_add(1))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(SimError::NumericOverflow)?;
    let offset = inclusive_rng_u64(rng, 0, width - 1);
    i8::try_from(i64::from(minimum) + i64::try_from(offset).map_err(|_| SimError::NumericOverflow)?)
        .map_err(|_| SimError::NumericOverflow)
}

fn choose_repeat(
    placement: &cdda_protocol::WorldgenItemGroupPlacementV1,
    rng: &mut ChaCha8Rng,
) -> Result<u16, SimError> {
    if placement.repeat_minimum > placement.repeat_maximum {
        return Err(SimError::InvalidItem);
    }
    if placement.repeat_minimum == placement.repeat_maximum {
        return Ok(placement.repeat_minimum);
    }
    let value = inclusive_rng_u64(
        rng,
        u64::from(placement.repeat_minimum),
        u64::from(placement.repeat_maximum),
    );
    u16::try_from(value).map_err(|_| SimError::NumericOverflow)
}

fn plan_item_placement(
    placement: &cdda_protocol::WorldgenItemGroupPlacementV1,
    index: usize,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    for _ in 0..choose_repeat(placement, rng)? {
        if !item_placement_succeeds(placement.chance, rng) {
            continue;
        }
        let prototypes = plan_item_group_source(
            &ItemGroupSourceV1::Group(placement.group_id.clone()),
            item_groups,
            rng,
        )?;
        for prototype in prototypes {
            if plan.items.len() >= ID_RESERVATION_SIZE as usize {
                return Err(SimError::InvalidItem);
            }
            plan.items.push((index, prototype));
        }
    }
    Ok(())
}

fn offset_tile_index(
    source: usize,
    width: usize,
    offset_x: i32,
    offset_y: i32,
) -> Result<Option<usize>, SimError> {
    let x = i32::try_from(source % width)
        .map_err(|_| SimError::NumericOverflow)?
        .checked_add(offset_x)
        .ok_or(SimError::NumericOverflow)?;
    let y = i32::try_from(source / width)
        .map_err(|_| SimError::NumericOverflow)?
        .checked_add(offset_y)
        .ok_or(SimError::NumericOverflow)?;
    Ok(absolute_tile_index(x, y))
}

fn absolute_tile_index(x: i32, y: i32) -> Option<usize> {
    if !(0..OMT_TILE_WIDTH as i32).contains(&x) || !(0..OMT_TILE_WIDTH as i32).contains(&y) {
        return None;
    }
    usize::try_from(y)
        .ok()?
        .checked_mul(OMT_TILE_WIDTH)?
        .checked_add(usize::try_from(x).ok()?)
}

fn rotate_tile_xy(x: usize, y: usize, rotation: u8) -> Result<(usize, usize), SimError> {
    let edge = OMT_TILE_WIDTH - 1;
    match rotation {
        0 => Ok((x, y)),
        1 => Ok((edge.checked_sub(y).ok_or(SimError::InvalidTerrain)?, x)),
        2 => Ok((
            edge.checked_sub(x).ok_or(SimError::InvalidTerrain)?,
            edge.checked_sub(y).ok_or(SimError::InvalidTerrain)?,
        )),
        3 => Ok((y, edge.checked_sub(x).ok_or(SimError::InvalidTerrain)?)),
        _ => Err(SimError::InvalidTerrain),
    }
}

fn rotate_tiles(
    terrain: Vec<cdda_protocol::TerrainTileSnapshot>,
    furniture: Vec<Option<FurnitureTileSnapshot>>,
    rotation: u8,
) -> Result<
    (
        Vec<cdda_protocol::TerrainTileSnapshot>,
        Vec<Option<FurnitureTileSnapshot>>,
    ),
    SimError,
> {
    if terrain.len() != OMT_TILE_COUNT || furniture.len() != OMT_TILE_COUNT {
        return Err(SimError::InvalidTerrain);
    }
    if rotation == 0 {
        return Ok((terrain, furniture));
    }
    let mut rotated_terrain = vec![terrain[0].clone(); OMT_TILE_COUNT];
    let mut rotated_furniture = vec![None; OMT_TILE_COUNT];
    for source_y in 0..OMT_TILE_WIDTH {
        for source_x in 0..OMT_TILE_WIDTH {
            let source = source_y * OMT_TILE_WIDTH + source_x;
            let (target_x, target_y) = rotate_tile_xy(source_x, source_y, rotation)?;
            let target = target_y * OMT_TILE_WIDTH + target_x;
            rotated_terrain[target] = terrain[source].clone();
            rotated_furniture[target] = furniture[source].clone();
        }
    }
    Ok((rotated_terrain, rotated_furniture))
}

fn item_placement_succeeds(chance: u8, rng: &mut ChaCha8Rng) -> bool {
    chance == 100 || rng.next_u64() % 100 < u64::from(chance)
}

fn coordinate_rng(
    world_seed: [u8; 32],
    generator_version: u16,
    omt: ChunkCoord,
    omt_id: &str,
) -> ChaCha8Rng {
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust mapgen coordinate v1");
    hasher.update(&world_seed);
    hasher.update(&generator_version.to_be_bytes());
    hasher.update(&omt.x.to_be_bytes());
    hasher.update(&omt.y.to_be_bytes());
    hasher.update(&omt.z.to_be_bytes());
    hasher.update(&(omt_id.len() as u64).to_be_bytes());
    hasher.update(omt_id.as_bytes());
    ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}

fn choose_template<'a>(
    templates: &'a [cdda_protocol::WorldgenTemplateV1],
    rng: &mut ChaCha8Rng,
) -> Result<&'a cdda_protocol::WorldgenTemplateV1, SimError> {
    let total = templates.iter().try_fold(0_u64, |total, template| {
        total.checked_add(u64::from(template.weight))
    });
    choose_weighted_index(
        templates.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(templates[index].weight),
        true,
    )
    .and_then(|index| templates.get(index).ok_or(SimError::InvalidTerrain))
}

fn choose_terrain_target(
    choices: &[WorldgenWeightedTerrainTargetV1],
    rng: &mut ChaCha8Rng,
) -> Result<WorldgenTerrainTargetV1, SimError> {
    let total = choices.iter().try_fold(0_u64, |total, choice| {
        total.checked_add(u64::from(choice.weight))
    });
    let index = choose_weighted_index(
        choices.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(choices[index].weight),
        false,
    )?;
    choices
        .get(index)
        .map(|choice| choice.target)
        .ok_or(SimError::InvalidTerrain)
}

fn choose_furniture_target(
    choices: &[WorldgenWeightedFurnitureTargetV1],
    rng: &mut ChaCha8Rng,
) -> Result<WorldgenFurnitureTargetV1, SimError> {
    let total = choices.iter().try_fold(0_u64, |total, choice| {
        total.checked_add(u64::from(choice.weight))
    });
    let index = choose_weighted_index(
        choices.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(choices[index].weight),
        false,
    )?;
    choices
        .get(index)
        .map(|choice| choice.target)
        .ok_or(SimError::InvalidFurniture)
}

fn choose_weighted_index(
    len: usize,
    total: u64,
    rng: &mut ChaCha8Rng,
    weight: impl Fn(usize) -> u64,
    consume_singleton: bool,
) -> Result<usize, SimError> {
    if len == 0 || total == 0 {
        return Err(SimError::InvalidTerrain);
    }
    if len == 1 {
        if consume_singleton {
            let _ = inclusive_rng_u64(rng, 1, total);
        }
        return Ok(0);
    }
    let ticket = inclusive_rng_u64(rng, 1, total);
    let mut accumulated = 0_u64;
    for index in 0..len {
        accumulated = accumulated
            .checked_add(weight(index))
            .ok_or(SimError::NumericOverflow)?;
        if ticket <= accumulated {
            return Ok(index);
        }
    }
    Err(SimError::InvalidTerrain)
}

fn resolve_terrain(
    catalog: &WorldgenCatalogV1,
    target: &WorldgenTerrainTargetV1,
    rng: &mut ChaCha8Rng,
) -> Result<cdda_protocol::TerrainTileSnapshot, SimError> {
    let (mut prototype_index, mut resolution_depth) = match target {
        WorldgenTerrainTargetV1::Prototype(index) => (*index, 0_usize),
        WorldgenTerrainTargetV1::Regional(index) => {
            let table = catalog
                .regional_terrain
                .get(usize::from(*index))
                .ok_or(SimError::InvalidTerrain)?;
            (choose_prototype(&table.choices, rng)?, 1)
        }
    };
    loop {
        let prototype = catalog
            .terrain_prototypes
            .get(usize::from(prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let Ok(regional_index) = catalog.regional_terrain.binary_search_by(|table| {
            table
                .regional_id
                .as_str()
                .cmp(prototype.terrain_id.as_str())
        }) else {
            return Ok(prototype.clone());
        };
        if resolution_depth >= MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH {
            return Err(SimError::InvalidTerrain);
        }
        resolution_depth += 1;
        prototype_index = choose_prototype(&catalog.regional_terrain[regional_index].choices, rng)?;
    }
}

fn choose_prototype(
    choices: &[WorldgenWeightedPrototypeV1],
    rng: &mut ChaCha8Rng,
) -> Result<u16, SimError> {
    let total = choices.iter().try_fold(0_u64, |total, choice| {
        total.checked_add(u64::from(choice.weight))
    });
    let index = choose_weighted_index(
        choices.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(choices[index].weight),
        true,
    )?;
    choices
        .get(index)
        .map(|choice| choice.prototype_index)
        .ok_or(SimError::InvalidTerrain)
}

fn resolve_furniture(
    catalog: &WorldgenCatalogV1,
    target: &WorldgenFurnitureTargetV1,
    rng: &mut ChaCha8Rng,
) -> Result<Option<FurnitureTileSnapshot>, SimError> {
    let (mut target, mut resolution_depth) = match target {
        WorldgenFurnitureTargetV1::None => return Ok(None),
        WorldgenFurnitureTargetV1::Prototype(index) => (
            WorldgenFurniturePrototypeTargetV1::Prototype(*index),
            0_usize,
        ),
        WorldgenFurnitureTargetV1::Regional(index) => {
            let table = catalog
                .regional_furniture
                .get(usize::from(*index))
                .ok_or(SimError::InvalidFurniture)?;
            (choose_furniture_prototype(&table.choices, rng)?, 1)
        }
    };
    loop {
        let WorldgenFurniturePrototypeTargetV1::Prototype(index) = target else {
            return Ok(None);
        };
        let prototype = catalog
            .furniture_prototypes
            .get(usize::from(index))
            .ok_or(SimError::InvalidFurniture)?;
        let Ok(regional_index) = catalog.regional_furniture.binary_search_by(|table| {
            table
                .regional_id
                .as_str()
                .cmp(prototype.furniture_id.as_str())
        }) else {
            return Ok(Some(prototype.clone()));
        };
        if resolution_depth >= MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH {
            return Err(SimError::InvalidFurniture);
        }
        resolution_depth += 1;
        target =
            choose_furniture_prototype(&catalog.regional_furniture[regional_index].choices, rng)?;
    }
}

fn choose_furniture_prototype(
    choices: &[WorldgenWeightedFurniturePrototypeV1],
    rng: &mut ChaCha8Rng,
) -> Result<WorldgenFurniturePrototypeTargetV1, SimError> {
    let total = choices.iter().try_fold(0_u64, |total, choice| {
        total.checked_add(u64::from(choice.weight))
    });
    let index = choose_weighted_index(
        choices.len(),
        total.ok_or(SimError::NumericOverflow)?,
        rng,
        |index| u64::from(choices[index].weight),
        true,
    )?;
    choices
        .get(index)
        .map(|choice| choice.target)
        .ok_or(SimError::InvalidFurniture)
}

fn omt_chunk_coords(omt: ChunkCoord) -> Result<[ChunkCoord; 4], SimError> {
    let base_x = omt
        .x
        .checked_mul(OMT_SUBMAP_WIDTH)
        .ok_or(SimError::NumericOverflow)?;
    let base_y = omt
        .y
        .checked_mul(OMT_SUBMAP_WIDTH)
        .ok_or(SimError::NumericOverflow)?;
    let east = base_x.checked_add(1).ok_or(SimError::NumericOverflow)?;
    let south = base_y.checked_add(1).ok_or(SimError::NumericOverflow)?;
    Ok([
        ChunkCoord {
            x: base_x,
            y: base_y,
            z: omt.z,
        },
        ChunkCoord {
            x: east,
            y: base_y,
            z: omt.z,
        },
        ChunkCoord {
            x: base_x,
            y: south,
            z: omt.z,
        },
        ChunkCoord {
            x: east,
            y: south,
            z: omt.z,
        },
    ])
}

pub(super) fn omt_tile_position(
    omt: ChunkCoord,
    x: usize,
    y: usize,
) -> Result<WorldPosition, SimError> {
    let x = i32::try_from(x).map_err(|_| SimError::NumericOverflow)?;
    let y = i32::try_from(y).map_err(|_| SimError::NumericOverflow)?;
    Ok(WorldPosition {
        x: omt
            .x
            .checked_mul(OMT_TILE_WIDTH as i32)
            .and_then(|base| base.checked_add(x))
            .ok_or(SimError::NumericOverflow)?,
        y: omt
            .y
            .checked_mul(OMT_TILE_WIDTH as i32)
            .and_then(|base| base.checked_add(y))
            .ok_or(SimError::NumericOverflow)?,
        z: omt.z,
    })
}

fn chunks_from_tiles(
    omt: ChunkCoord,
    terrain: Vec<cdda_protocol::TerrainTileSnapshot>,
    furniture: Vec<Option<FurnitureTileSnapshot>>,
) -> Result<[Chunk; 4], SimError> {
    if terrain.len() != OMT_TILE_COUNT || furniture.len() != OMT_TILE_COUNT {
        return Err(SimError::InvalidTerrain);
    }
    let coords = omt_chunk_coords(omt)?;
    let mut chunk_terrain: [Vec<cdda_protocol::TerrainTileSnapshot>; 4] =
        std::array::from_fn(|_| Vec::with_capacity((SUBMAP_SIZE * SUBMAP_SIZE) as usize));
    let mut chunk_furniture: [Vec<Option<FurnitureTileSnapshot>>; 4] =
        std::array::from_fn(|_| Vec::with_capacity((SUBMAP_SIZE * SUBMAP_SIZE) as usize));
    for y in 0..OMT_TILE_WIDTH {
        for x in 0..OMT_TILE_WIDTH {
            let global_index = y * OMT_TILE_WIDTH + x;
            let chunk_index =
                usize::from(x >= SUBMAP_SIZE as usize) + 2 * usize::from(y >= SUBMAP_SIZE as usize);
            chunk_terrain[chunk_index].push(terrain[global_index].clone());
            chunk_furniture[chunk_index].push(furniture[global_index].clone());
        }
    }
    Ok(std::array::from_fn(|index| Chunk {
        coord: coords[index],
        revision: 0,
        tiles: std::mem::take(&mut chunk_terrain[index]),
        furniture: std::mem::take(&mut chunk_furniture[index]),
        fields: vec![Vec::new(); (SUBMAP_SIZE * SUBMAP_SIZE) as usize],
        map_damage: vec![0; (SUBMAP_SIZE * SUBMAP_SIZE) as usize],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clockwise_marker_rotation_matches_the_pinned_24_by_24_oracle() {
        assert_eq!(rotate_tile_xy(2, 5, 0).expect("north"), (2, 5));
        assert_eq!(rotate_tile_xy(2, 5, 1).expect("east"), (18, 2));
        assert_eq!(rotate_tile_xy(2, 5, 2).expect("south"), (21, 18));
        assert_eq!(rotate_tile_xy(2, 5, 3).expect("west"), (5, 21));
        assert!(matches!(
            rotate_tile_xy(2, 5, 4),
            Err(SimError::InvalidTerrain)
        ));
    }

    #[test]
    fn weighted_singletons_consume_a_draw_but_fixed_singletons_do_not() {
        let mut weighted = ChaCha8Rng::from_seed([17; 32]);
        let mut after_weighted = weighted.clone();
        let _ = after_weighted.next_u64();
        assert_eq!(
            choose_weighted_index(1, 7, &mut weighted, |_| 7, true)
                .expect("weighted singleton should select its only entry"),
            0
        );
        assert_eq!(weighted.next_u64(), after_weighted.next_u64());

        let mut fixed = ChaCha8Rng::from_seed([23; 32]);
        let mut untouched = fixed.clone();
        assert_eq!(
            choose_weighted_index(1, 7, &mut fixed, |_| 7, false)
                .expect("fixed singleton should select its only entry"),
            0
        );
        assert_eq!(fixed.next_u64(), untouched.next_u64());
    }

    #[test]
    fn guaranteed_item_placement_consumes_no_outer_chance_draw() {
        let mut guaranteed = ChaCha8Rng::from_seed([29; 32]);
        let mut untouched = guaranteed.clone();
        assert!(item_placement_succeeds(100, &mut guaranteed));
        assert_eq!(guaranteed.next_u64(), untouched.next_u64());

        let mut conditional = ChaCha8Rng::from_seed([31; 32]);
        let mut after_conditional = conditional.clone();
        let _ = after_conditional.next_u64();
        let _ = item_placement_succeeds(99, &mut conditional);
        assert_eq!(conditional.next_u64(), after_conditional.next_u64());
    }
}
