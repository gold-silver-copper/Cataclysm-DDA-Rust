use cdda_protocol::{
    ChunkCoord, FurnitureTileSnapshot, ItemGroupDefinitionV1, ItemGroupSourceV1,
    MAX_WORLDGEN_REGIONAL_RESOLUTION_DEPTH, VehiclePartSnapshotV1, VehicleSpawnStatusV1,
    WorldPosition, WorldgenBuiltinMapgenV1, WorldgenCatalogV1, WorldgenFurniturePrototypeTargetV1,
    WorldgenFurnitureTargetV1, WorldgenIndividualMonsterPlacementV1,
    WorldgenIndividualMonsterTargetV1, WorldgenMonsterGroupTargetV1, WorldgenMonsterPlacementV1,
    WorldgenNestedConditionsV1, WorldgenNestedGeneratorV1, WorldgenNestedPlacementV1,
    WorldgenNestedTemplateV1, WorldgenNpcPlacementV1, WorldgenOmtGeneratorV1,
    WorldgenTerrainTargetV1, WorldgenVehiclePlacementV1, WorldgenWeightedFurniturePrototypeV1,
    WorldgenWeightedFurnitureTargetV1, WorldgenWeightedPrototypeV1,
    WorldgenWeightedTerrainTargetV1, item_group_source_max_outputs, worldgen_city_start_distance,
    worldgen_omt_identity_at, worldgen_omt_matches, worldgen_overmap_contains,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

use super::{
    Chunk, CreatureSpawn, ID_RESERVATION_SIZE, PlannedItemSpawn, SUBMAP_SIZE, SimError,
    damage_vehicle_spawn_item, dress_vehicle_spawn_item, inclusive_rng_u64, plan_item_group_source,
    plan_vehicle_direct_item,
};

const OMT_SUBMAP_WIDTH: i32 = 2;
pub(super) const OMT_TILE_WIDTH: usize = (SUBMAP_SIZE as usize) * (OMT_SUBMAP_WIDTH as usize);
const OMT_TILE_COUNT: usize = OMT_TILE_WIDTH * OMT_TILE_WIDTH;
const MAX_PLANNED_CREATURES_PER_OMT: usize = 4_096;
const MAX_PLANNED_NPCS_PER_OMT: usize = 4_096;
const MAX_PLANNED_VEHICLES_PER_OMT: usize = 1_024;

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

pub(super) fn catalog_npc_placements_are_supported(
    catalog: &WorldgenCatalogV1,
    templates: &std::collections::BTreeMap<String, cdda_protocol::NpcTemplateV1>,
) -> bool {
    let placement_is_supported = |placement: &WorldgenNpcPlacementV1| {
        templates
            .get(&placement.template_id)
            .is_some_and(|template| {
                let expected: &[&str] = if template.name_unique.is_some() {
                    &[]
                } else {
                    match template.gender.as_deref() {
                        Some("male") => &["<male_full_name>"],
                        Some("female") => &["<female_full_name>"],
                        None => &["<male_full_name>", "<female_full_name>"],
                        Some(_) => return false,
                    }
                };
                placement.generated_name_category_ids.len() == expected.len()
                    && placement
                        .generated_name_category_ids
                        .iter()
                        .map(String::as_str)
                        .eq(expected.iter().copied())
            })
    };
    catalog.omt_generators.iter().all(|generator| {
        generator
            .templates
            .iter()
            .all(|template| template.npc_placements.iter().all(&placement_is_supported))
            && generator.nested_generators.iter().all(|nested| {
                nested
                    .templates
                    .iter()
                    .all(|template| template.npc_placements.iter().all(&placement_is_supported))
            })
    })
}

pub(super) fn catalog_vehicle_placement_factions_are_known(
    catalog: &WorldgenCatalogV1,
    factions: &std::collections::BTreeMap<String, cdda_protocol::FactionStateV1>,
) -> bool {
    let placement_is_known = |placement: &WorldgenVehiclePlacementV1| {
        placement.faction_id.is_empty() || factions.contains_key(&placement.faction_id)
    };
    catalog.omt_generators.iter().all(|generator| {
        generator
            .templates
            .iter()
            .all(|template| template.vehicle_placements.iter().all(&placement_is_known))
            && generator.nested_generators.iter().all(|nested| {
                nested
                    .templates
                    .iter()
                    .all(|template| template.vehicle_placements.iter().all(&placement_is_known))
            })
    })
}

pub(super) struct PlannedBubble {
    pub chunks: Vec<Chunk>,
    pub items: Vec<(WorldPosition, PlannedItemSpawn)>,
    pub item_object_count: u64,
    pub creatures: Vec<CreatureSpawn>,
    pub npcs: Vec<PlannedNpcSpawn>,
    pub vehicles: Vec<PlannedVehicleSpawn>,
}

pub(super) struct PlannedNpcSpawn {
    pub template_id: String,
    pub generated_name: Option<String>,
    pub generated_gender: Option<String>,
    pub position: WorldPosition,
}

pub(super) struct PlannedVehicleSpawn {
    pub prototype_index: u16,
    pub origin: WorldPosition,
    pub facing_degrees: i16,
    pub owner_faction_id: String,
    pub parts: Vec<VehiclePartSnapshotV1>,
    pub cargo: Vec<Vec<PlannedItemSpawn>>,
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
    occupied: &std::collections::BTreeSet<WorldPosition>,
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
        item_object_count: 0,
        creatures: Vec::new(),
        npcs: Vec::new(),
        vehicles: Vec::new(),
    };
    let mut planned_occupied = occupied.clone();
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
                    let cell = plan_omt_cell(
                        world_seed,
                        catalog,
                        item_groups,
                        omt,
                        &mut planned_occupied,
                    )?;
                    planned.item_object_count = planned
                        .item_object_count
                        .checked_add(cell.item_object_count)
                        .filter(|count| *count <= ID_RESERVATION_SIZE)
                        .ok_or(SimError::InvalidItem)?;
                    planned.chunks.extend(cell.chunks);
                    planned.items.extend(cell.items);
                    planned.creatures.extend(cell.creatures);
                    planned.npcs.extend(cell.npcs);
                    planned.vehicles.extend(cell.vehicles);
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
    occupied: &mut std::collections::BTreeSet<WorldPosition>,
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
    materialize_monster_placements(catalog, &mut rng, &mut plan)?;
    materialize_special_populations(world_seed, catalog, omt, &mut plan)?;

    let terrain = plan
        .terrain
        .into_iter()
        .map(|tile| match tile {
            Some(PlannedTerrainTile::Resolved(tile)) => Ok(tile),
            _ => Err(SimError::InvalidTerrain),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let furniture = plan
        .furniture
        .into_iter()
        .map(|tile| match tile {
            Some(PlannedFurnitureTile::Resolved(tile)) => Ok(tile),
            _ => Err(SimError::InvalidFurniture),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let item_object_count = plan.item_object_count;
    let mut items = Vec::with_capacity(plan.items.len());
    for (index, prototype) in plan.items {
        if terrain[index].move_cost <= 0
            || furniture[index]
                .as_ref()
                .is_some_and(|furniture| furniture.move_cost_mod < 0)
        {
            return Err(SimError::InvalidTerrain);
        }
        items.push((
            omt_tile_position(omt, index % OMT_TILE_WIDTH, index / OMT_TILE_WIDTH)?,
            prototype,
        ));
    }
    let mut vehicles = Vec::with_capacity(plan.vehicles.len());
    for mut vehicle in plan.vehicles {
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(vehicle.prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let origin = omt_tile_position(
            omt,
            vehicle.origin_index % OMT_TILE_WIDTH,
            vehicle.origin_index / OMT_TILE_WIDTH,
        )?;
        let mut parts = Vec::with_capacity(prototype.parts.len());
        let mut cargo = Vec::with_capacity(prototype.parts.len());
        for (index, prototype_part) in prototype.parts.iter().enumerate() {
            let state = vehicle
                .parts
                .get_mut(index)
                .ok_or(SimError::InvalidTerrain)?;
            parts.push(VehiclePartSnapshotV1 {
                prototype_part_index: u16::try_from(index)
                    .map_err(|_| SimError::NumericOverflow)?,
                position: super::vehicles::vehicle_part_position(
                    origin,
                    prototype_part.mount_x,
                    prototype_part.mount_y,
                    vehicle.facing_degrees,
                )?,
                hp: state.hp,
                enabled: state.enabled,
                open: state.open,
                locked: state.locked,
                passenger: None,
                cargo: Vec::new(),
            });
            cargo.push(std::mem::take(&mut state.cargo));
        }
        let mut structure_positions = std::collections::BTreeSet::new();
        for (index, part) in parts.iter().enumerate() {
            let prototype_part = prototype.parts.get(index).ok_or(SimError::InvalidTerrain)?;
            let part_type = catalog
                .vehicle_part_types
                .get(usize::from(prototype_part.part_type_index))
                .ok_or(SimError::InvalidTerrain)?;
            if part.hp == 0 || part_type.location != "structure" {
                continue;
            }
            let local_x = part
                .position
                .x
                .checked_sub(
                    omt.x
                        .checked_mul(OMT_TILE_WIDTH as i32)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .ok_or(SimError::NumericOverflow)?;
            let local_y = part
                .position
                .y
                .checked_sub(
                    omt.y
                        .checked_mul(OMT_TILE_WIDTH as i32)
                        .ok_or(SimError::NumericOverflow)?,
                )
                .ok_or(SimError::NumericOverflow)?;
            let local_index =
                absolute_tile_index(local_x, local_y).ok_or(SimError::InvalidTerrain)?;
            if terrain[local_index].move_cost <= 0
                || furniture[local_index]
                    .as_ref()
                    .is_some_and(|furniture| furniture.move_cost_mod < 0)
                || !structure_positions.insert(part.position)
            {
                return Err(SimError::InvalidTerrain);
            }
        }
        if structure_positions
            .iter()
            .any(|position| !occupied.insert(*position))
        {
            return Err(SimError::InvalidTerrain);
        }
        vehicles.push(PlannedVehicleSpawn {
            prototype_index: vehicle.prototype_index,
            origin,
            facing_degrees: vehicle.facing_degrees,
            owner_faction_id: vehicle.owner_faction_id,
            parts,
            cargo,
        });
    }
    let mut npcs = Vec::with_capacity(plan.npcs.len());
    for (index, template_id, generated_name, generated_gender) in plan.npcs {
        if terrain[index].move_cost <= 0
            || furniture[index]
                .as_ref()
                .is_some_and(|furniture| furniture.move_cost_mod < 0)
        {
            continue;
        }
        let position = omt_tile_position(omt, index % OMT_TILE_WIDTH, index / OMT_TILE_WIDTH)?;
        if !occupied.insert(position) {
            continue;
        }
        npcs.push(PlannedNpcSpawn {
            template_id,
            generated_name,
            generated_gender,
            position,
        });
    }
    let mut creatures = Vec::with_capacity(plan.monsters.len());
    for (index, prototype_index) in plan.monsters {
        if terrain[index].move_cost <= 0
            || furniture[index]
                .as_ref()
                .is_some_and(|furniture| furniture.move_cost_mod < 0)
        {
            continue;
        }
        let position = omt_tile_position(omt, index % OMT_TILE_WIDTH, index / OMT_TILE_WIDTH)?;
        if !occupied.insert(position) {
            continue;
        }
        let prototype = catalog
            .monster_prototypes
            .get(usize::from(prototype_index))
            .ok_or(SimError::InvalidCreature)?;
        if !prototype.runtime_spawnable {
            continue;
        }
        creatures.push(creature_spawn_from_worldgen(prototype, position));
    }
    Ok(PlannedBubble {
        chunks: chunks_from_tiles(omt, terrain, furniture)?.into(),
        items,
        item_object_count,
        creatures,
        npcs,
        vehicles,
    })
}

pub(super) fn creature_spawn_from_worldgen(
    prototype: &cdda_protocol::WorldgenMonsterPrototypeV1,
    position: WorldPosition,
) -> CreatureSpawn {
    let base = &prototype.base;
    CreatureSpawn {
        type_id: base.monster_type_id.clone(),
        faction_id: prototype.default_faction_id.clone(),
        position,
        hp: base.max_hp,
        speed: base.speed,
        attack_cost_moves: base.attack_cost_moves,
        aggression: base.aggression,
        morale: base.morale,
        melee_skill: base.melee_skill,
        dodge: base.dodge,
        size: base.size,
        melee_dice: base.melee_dice,
        melee_dice_sides: base.melee_dice_sides,
        can_see: base.can_see,
        vision_day: base.vision_day,
        vision_night: base.vision_night,
        stumbles: base.stumbles,
        bashes: base.bashes,
        group_bash: base.group_bash,
        hears: base.hears,
        good_hearing: base.good_hearing,
        clumsy_attacks: base.clumsy_attacks,
        immobile: base.immobile,
        pacifist: base.pacifist,
        can_open_doors: base.can_open_doors,
        path_settings: base.path_settings,
        blood_field_type_id: base.blood_field_type_id.clone(),
        corpse: prototype.leaves_corpse.then(|| base.clone()),
    }
}

#[derive(Clone)]
enum PlannedTerrainTile {
    Target(WorldgenTerrainTargetV1),
    Resolved(cdda_protocol::TerrainTileSnapshot),
}

#[derive(Clone)]
enum PlannedFurnitureTile {
    Target(WorldgenFurnitureTargetV1),
    Resolved(Option<FurnitureTileSnapshot>),
}

struct OmtMapgenPlan {
    terrain: Vec<Option<PlannedTerrainTile>>,
    furniture: Vec<Option<PlannedFurnitureTile>>,
    items: Vec<(usize, PlannedItemSpawn)>,
    item_object_count: u64,
    nested_expansions: usize,
    monster_placements: Vec<PlannedMonsterPlacement>,
    individual_monster_placements: Vec<PlannedIndividualMonsterPlacement>,
    npcs: Vec<(usize, String, Option<String>, Option<String>)>,
    monsters: Vec<(usize, u16)>,
    vehicles: Vec<PlannedMapgenVehicle>,
}

struct PlannedVehiclePartState {
    hp: u32,
    enabled: bool,
    open: bool,
    locked: bool,
    cargo: Vec<PlannedItemSpawn>,
}

struct PlannedMapgenVehicle {
    prototype_index: u16,
    origin_index: usize,
    facing_degrees: i16,
    owner_faction_id: String,
    parts: Vec<PlannedVehiclePartState>,
}

struct PlannedMonsterPlacement {
    placement: WorldgenMonsterPlacementV1,
    candidates: Vec<usize>,
}

struct PlannedIndividualMonsterPlacement {
    placement: WorldgenIndividualMonsterPlacementV1,
    candidates: Vec<usize>,
}

impl OmtMapgenPlan {
    fn new() -> Self {
        Self {
            terrain: vec![None; OMT_TILE_COUNT],
            furniture: vec![None; OMT_TILE_COUNT],
            items: Vec::new(),
            item_object_count: 0,
            nested_expansions: 0,
            monster_placements: Vec::new(),
            individual_monster_placements: Vec::new(),
            npcs: Vec::new(),
            monsters: Vec::new(),
            vehicles: Vec::new(),
        }
    }

    fn clear_items_at(&mut self, index: usize) -> Result<(), SimError> {
        let mut removed_object_count = Some(0_u64);
        self.items.retain(|(item_index, item)| {
            if *item_index != index {
                return true;
            }
            removed_object_count =
                removed_object_count.and_then(|count| count.checked_add(item.object_count()?));
            false
        });
        self.item_object_count = self
            .item_object_count
            .checked_sub(removed_object_count.ok_or(SimError::NumericOverflow)?)
            .ok_or(SimError::NumericOverflow)?;
        Ok(())
    }

    fn push_item(&mut self, index: usize, item: PlannedItemSpawn) -> Result<(), SimError> {
        let object_count = item.object_count().ok_or(SimError::NumericOverflow)?;
        self.item_object_count = self
            .item_object_count
            .checked_add(object_count)
            .filter(|count| *count <= ID_RESERVATION_SIZE)
            .ok_or(SimError::InvalidItem)?;
        self.items.push((index, item));
        Ok(())
    }

    fn record_nested_expansion(&mut self) -> Result<(), SimError> {
        self.nested_expansions = self
            .nested_expansions
            .checked_add(1)
            .filter(|count| *count <= cdda_protocol::MAX_WORLDGEN_NESTED_PLACEMENTS)
            .ok_or(SimError::InvalidTerrain)?;
        Ok(())
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
    if let Some(builtin) = template.builtin {
        if depth != 0 {
            return Err(SimError::InvalidTerrain);
        }
        apply_builtin_mapgen(catalog, builtin, rng, plan)?;
        resolve_mapgen_plan(catalog, rng, plan)?;
        return rotate_mapgen_plan(plan, rotation);
    }
    if let Some(predecessor_id) = &template.predecessor_id {
        let predecessor = catalog
            .omt_generators
            .binary_search_by(|candidate| candidate.omt_id.as_str().cmp(predecessor_id.as_str()))
            .ok()
            .and_then(|index| catalog.omt_generators.get(index))
            .ok_or(SimError::InvalidTerrain)?;
        let predecessor_rotation = catalog
            .overmap
            .identities
            .binary_search_by(|identity| identity.full_id.as_str().cmp(predecessor_id))
            .ok()
            .and_then(|index| catalog.overmap.identities.get(index))
            .map(|identity| identity.rotation)
            .ok_or(SimError::InvalidTerrain)?;
        apply_root_generator(
            catalog,
            predecessor,
            item_groups,
            omt,
            predecessor_rotation,
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
        &template.npc_placements,
        &template.vehicle_placements,
        &template.monster_placements,
        &template.individual_monster_placements,
        template.erase_all_before_placing_terrain,
        0,
        0,
        rng,
        plan,
        depth,
    )?;
    resolve_mapgen_plan(catalog, rng, plan)?;
    rotate_mapgen_plan(plan, rotation)
}

fn apply_builtin_mapgen(
    catalog: &WorldgenCatalogV1,
    builtin: WorldgenBuiltinMapgenV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    if plan.terrain.iter().any(Option::is_some)
        || plan.furniture.iter().any(Option::is_some)
        || !plan.items.is_empty()
        || !plan.monster_placements.is_empty()
        || !plan.individual_monster_placements.is_empty()
        || !plan.npcs.is_empty()
        || !plan.monsters.is_empty()
    {
        return Err(SimError::InvalidTerrain);
    }
    if !matches!(builtin, WorldgenBuiltinMapgenV1::ForestWater) {
        let deep = builtin_terrain(catalog, "t_water_moving_dp")?;
        for (terrain, furniture) in plan.terrain.iter_mut().zip(&mut plan.furniture) {
            *terrain = Some(PlannedTerrainTile::Resolved(deep.clone()));
            *furniture = Some(PlannedFurnitureTile::Resolved(None));
        }
    }
    match builtin {
        WorldgenBuiltinMapgenV1::RiverStraight => {
            builtin_river_straight(catalog, rng, plan)?;
        }
        WorldgenBuiltinMapgenV1::RiverCurved { rotation } => {
            builtin_river_curved(catalog, rng, plan)?;
            rotate_mapgen_plan(plan, rotation)?;
        }
        WorldgenBuiltinMapgenV1::RiverCurvedNot { rotation } => {
            builtin_river_curved_not(catalog, rng, plan)?;
            rotate_mapgen_plan(plan, rotation)?;
        }
        WorldgenBuiltinMapgenV1::ForestWater => builtin_forest_water(catalog, rng, plan)?,
    }
    Ok(())
}

fn builtin_forest_water(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let groundcover = catalog
        .regional_terrain
        .binary_search_by(|table| table.regional_id.as_str().cmp("t_region_groundcover_swamp"))
        .ok()
        .and_then(|index| u16::try_from(index).ok())
        .ok_or(SimError::InvalidTerrain)?;
    let terrain_index = |terrain_id: &str| {
        catalog
            .terrain_prototypes
            .binary_search_by(|terrain| terrain.terrain_id.as_str().cmp(terrain_id))
            .ok()
            .and_then(|index| u16::try_from(index).ok())
            .ok_or(SimError::InvalidTerrain)
    };
    let shallow = terrain_index("t_water_sh")?;
    let deep = terrain_index("t_water_dp")?;
    let murky = terrain_index("t_water_murky")?;
    for index in 0..OMT_TILE_COUNT {
        plan.terrain[index] = Some(PlannedTerrainTile::Target(
            WorldgenTerrainTargetV1::Regional(groundcover),
        ));
        plan.furniture[index] = Some(PlannedFurnitureTile::Target(
            WorldgenFurnitureTargetV1::None,
        ));
        if one_in_mapgen(rng, 2) {
            let ticket = inclusive_rng_u64(rng, 1, 19);
            let prototype = if ticket <= 6 {
                shallow
            } else if ticket == 7 {
                deep
            } else {
                murky
            };
            plan.terrain[index] = Some(PlannedTerrainTile::Target(
                WorldgenTerrainTargetV1::Prototype(prototype),
            ));
        }
    }
    Ok(())
}

fn builtin_river_straight(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    for x in 0..OMT_TILE_WIDTH {
        let mut ground_edge =
            usize::try_from(inclusive_rng_u64(rng, 1, 3)).map_err(|_| SimError::NumericOverflow)?;
        let shallow_edge =
            usize::try_from(inclusive_rng_u64(rng, 4, 6)).map_err(|_| SimError::NumericOverflow)?;
        let ground = grass_or_dirt(catalog, rng)?;
        builtin_vertical_line(plan, x, 0, ground_edge, &ground)?;
        if one_in_mapgen(rng, 25) {
            ground_edge = ground_edge
                .checked_add(1)
                .ok_or(SimError::NumericOverflow)?;
            builtin_set_terrain(plan, x, ground_edge, &clay_or_sand(catalog, rng)?)?;
        }
        ground_edge = ground_edge
            .checked_add(1)
            .ok_or(SimError::NumericOverflow)?;
        let shallow = builtin_terrain(catalog, "t_water_moving_sh")?;
        builtin_vertical_line(plan, x, ground_edge, shallow_edge, &shallow)?;
    }
    Ok(())
}

fn builtin_river_curved(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    builtin_river_straight(catalog, rng, plan)?;
    for y in 0..OMT_TILE_WIDTH {
        let mut ground_edge = usize::try_from(inclusive_rng_u64(rng, 19, 21))
            .map_err(|_| SimError::NumericOverflow)?;
        let shallow_edge = usize::try_from(inclusive_rng_u64(rng, 16, 18))
            .map_err(|_| SimError::NumericOverflow)?;
        let ground = grass_or_dirt(catalog, rng)?;
        builtin_horizontal_line(plan, ground_edge, OMT_TILE_WIDTH - 1, y, &ground)?;
        if one_in_mapgen(rng, 25) {
            ground_edge = ground_edge
                .checked_sub(1)
                .ok_or(SimError::NumericOverflow)?;
            builtin_set_terrain(plan, ground_edge, y, &clay_or_sand(catalog, rng)?)?;
        }
        ground_edge = ground_edge
            .checked_sub(1)
            .ok_or(SimError::NumericOverflow)?;
        let shallow = builtin_terrain(catalog, "t_water_moving_sh")?;
        builtin_horizontal_line(plan, shallow_edge, ground_edge, y, &shallow)?;
    }
    Ok(())
}

fn builtin_river_curved_not(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let north_edge =
        usize::try_from(inclusive_rng_u64(rng, 16, 18)).map_err(|_| SimError::NumericOverflow)?;
    let east_edge =
        usize::try_from(inclusive_rng_u64(rng, 4, 8)).map_err(|_| SimError::NumericOverflow)?;
    let shallow = builtin_terrain(catalog, "t_water_moving_sh")?;
    for x in north_edge..OMT_TILE_WIDTH {
        for y in 0..east_edge {
            let dx = OMT_TILE_WIDTH
                .checked_sub(x)
                .ok_or(SimError::NumericOverflow)?;
            let circle_edge = dx
                .checked_mul(dx)
                .and_then(|value| value.checked_add(y.checked_mul(y)?))
                .ok_or(SimError::NumericOverflow)?;
            if circle_edge <= 8 {
                builtin_set_terrain(plan, x, y, &grass_or_dirt(catalog, rng)?)?;
            }
            if circle_edge == 9 && one_in_mapgen(rng, 25) {
                builtin_set_terrain(plan, x, y, &clay_or_sand(catalog, rng)?)?;
            } else if circle_edge <= 36 {
                builtin_set_terrain(plan, x, y, &shallow)?;
            }
        }
    }
    Ok(())
}

fn grass_or_dirt(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
) -> Result<cdda_protocol::TerrainTileSnapshot, SimError> {
    builtin_terrain(
        catalog,
        if one_in_mapgen(rng, 4) {
            "t_grass"
        } else {
            "t_dirt"
        },
    )
}

fn clay_or_sand(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
) -> Result<cdda_protocol::TerrainTileSnapshot, SimError> {
    builtin_terrain(
        catalog,
        if one_in_mapgen(rng, 16) {
            "t_sand"
        } else {
            "t_clay"
        },
    )
}

fn one_in_mapgen(rng: &mut ChaCha8Rng, denominator: u64) -> bool {
    inclusive_rng_u64(rng, 1, denominator) == 1
}

fn builtin_terrain(
    catalog: &WorldgenCatalogV1,
    terrain_id: &str,
) -> Result<cdda_protocol::TerrainTileSnapshot, SimError> {
    catalog
        .terrain_prototypes
        .binary_search_by(|terrain| terrain.terrain_id.as_str().cmp(terrain_id))
        .ok()
        .and_then(|index| catalog.terrain_prototypes.get(index))
        .cloned()
        .ok_or(SimError::InvalidTerrain)
}

fn builtin_set_terrain(
    plan: &mut OmtMapgenPlan,
    x: usize,
    y: usize,
    terrain: &cdda_protocol::TerrainTileSnapshot,
) -> Result<(), SimError> {
    if x >= OMT_TILE_WIDTH || y >= OMT_TILE_WIDTH {
        return Err(SimError::InvalidTerrain);
    }
    let index = y
        .checked_mul(OMT_TILE_WIDTH)
        .and_then(|row| row.checked_add(x))
        .ok_or(SimError::NumericOverflow)?;
    plan.terrain[index] = Some(PlannedTerrainTile::Resolved(terrain.clone()));
    Ok(())
}

fn builtin_vertical_line(
    plan: &mut OmtMapgenPlan,
    x: usize,
    minimum_y: usize,
    maximum_y: usize,
    terrain: &cdda_protocol::TerrainTileSnapshot,
) -> Result<(), SimError> {
    if minimum_y > maximum_y {
        return Err(SimError::InvalidTerrain);
    }
    for y in minimum_y..=maximum_y {
        builtin_set_terrain(plan, x, y, terrain)?;
    }
    Ok(())
}

fn builtin_horizontal_line(
    plan: &mut OmtMapgenPlan,
    minimum_x: usize,
    maximum_x: usize,
    y: usize,
    terrain: &cdda_protocol::TerrainTileSnapshot,
) -> Result<(), SimError> {
    if minimum_x > maximum_x {
        return Err(SimError::InvalidTerrain);
    }
    for x in minimum_x..=maximum_x {
        builtin_set_terrain(plan, x, y, terrain)?;
    }
    Ok(())
}

fn resolve_mapgen_plan(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    for tile in &mut plan.terrain {
        let target = match tile {
            Some(PlannedTerrainTile::Target(target)) => Some(*target),
            _ => None,
        };
        if let Some(target) = target {
            *tile = Some(PlannedTerrainTile::Resolved(resolve_terrain(
                catalog, &target, rng,
            )?));
        }
    }
    for tile in &mut plan.furniture {
        let target = match tile {
            Some(PlannedFurnitureTile::Target(target)) => Some(*target),
            _ => None,
        };
        if let Some(target) = target {
            *tile = Some(PlannedFurnitureTile::Resolved(resolve_furniture(
                catalog, &target, rng,
            )?));
        }
    }
    Ok(())
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
        terrain[target] = plan.terrain[source].clone();
        furniture[target] = plan.furniture[source].clone();
    }
    for (index, _) in &mut plan.items {
        let (target_x, target_y) =
            rotate_tile_xy(*index % OMT_TILE_WIDTH, *index / OMT_TILE_WIDTH, rotation)?;
        *index = target_y * OMT_TILE_WIDTH + target_x;
    }
    for request in &mut plan.monster_placements {
        for index in &mut request.candidates {
            let (target_x, target_y) =
                rotate_tile_xy(*index % OMT_TILE_WIDTH, *index / OMT_TILE_WIDTH, rotation)?;
            *index = target_y * OMT_TILE_WIDTH + target_x;
        }
        request.candidates.sort_unstable();
    }
    for request in &mut plan.individual_monster_placements {
        for index in &mut request.candidates {
            let (target_x, target_y) =
                rotate_tile_xy(*index % OMT_TILE_WIDTH, *index / OMT_TILE_WIDTH, rotation)?;
            *index = target_y * OMT_TILE_WIDTH + target_x;
        }
        request.candidates.sort_unstable();
    }
    for (index, _, _, _) in &mut plan.npcs {
        let (target_x, target_y) =
            rotate_tile_xy(*index % OMT_TILE_WIDTH, *index / OMT_TILE_WIDTH, rotation)?;
        *index = target_y * OMT_TILE_WIDTH + target_x;
    }
    for (index, _) in &mut plan.monsters {
        let (target_x, target_y) =
            rotate_tile_xy(*index % OMT_TILE_WIDTH, *index / OMT_TILE_WIDTH, rotation)?;
        *index = target_y * OMT_TILE_WIDTH + target_x;
    }
    for vehicle in &mut plan.vehicles {
        let (target_x, target_y) = rotate_tile_xy(
            vehicle.origin_index % OMT_TILE_WIDTH,
            vehicle.origin_index / OMT_TILE_WIDTH,
            rotation,
        )?;
        vehicle.origin_index = target_y * OMT_TILE_WIDTH + target_x;
        vehicle.facing_degrees = vehicle
            .facing_degrees
            .checked_add(i16::from(rotation) * 90)
            .ok_or(SimError::NumericOverflow)?
            .rem_euclid(360);
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
    npc_placements: &[WorldgenNpcPlacementV1],
    vehicle_placements: &[WorldgenVehiclePlacementV1],
    monster_placements: &[WorldgenMonsterPlacementV1],
    individual_monster_placements: &[WorldgenIndividualMonsterPlacementV1],
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
                plan.furniture[index] = Some(PlannedFurnitureTile::Target(
                    WorldgenFurnitureTargetV1::None,
                ));
                plan.clear_items_at(index)?;
            }
            plan.terrain[index] = Some(PlannedTerrainTile::Target(choose_terrain_target(
                layer, rng,
            )?));
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
            plan.furniture[index] = Some(PlannedFurnitureTile::Target(choose_furniture_target(
                layer, rng,
            )?));
        }
    }

    // NPC objects are in the pinned default phase before item and monster
    // objects and before the later nested-mapgen phase. Repeat is sampled once;
    // each application then samples x and y.
    for placement in npc_placements {
        plan_npc_placement(catalog, placement, offset_x, offset_y, rng, plan)?;
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
        plan_area_item_placement(placement, offset_x, offset_y, item_groups, rng, plan)?;
    }
    for placement in monster_placements {
        plan_monster_placement_request(placement, offset_x, offset_y, plan)?;
    }
    for placement in individual_monster_placements {
        plan_individual_monster_placement_request(placement, offset_x, offset_y, plan)?;
    }
    for placement in vehicle_placements {
        plan_vehicle_placement(
            catalog,
            placement,
            offset_x,
            offset_y,
            item_groups,
            rng,
            plan,
        )?;
    }

    // Nested mapgen is a later pinned phase than ordinary NPC, item, monster,
    // and vehicle pieces. It may therefore overlay their tiles but must not
    // consume its selector RNG before those default-phase objects.
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
    Ok(())
}

fn plan_npc_placement(
    catalog: &WorldgenCatalogV1,
    placement: &WorldgenNpcPlacementV1,
    offset_x: i32,
    offset_y: i32,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let repeat = if placement.repeat.minimum == placement.repeat.maximum {
        placement.repeat.minimum
    } else {
        choose_worldgen_u16_range(placement.repeat, rng)?
    };
    for _ in 0..repeat {
        let x = offset_x
            .checked_add(i32::from(choose_i8_range(
                placement.x.minimum,
                placement.x.maximum,
                rng,
            )?))
            .ok_or(SimError::NumericOverflow)?;
        let y = offset_y
            .checked_add(i32::from(choose_i8_range(
                placement.y.minimum,
                placement.y.maximum,
                rng,
            )?))
            .ok_or(SimError::NumericOverflow)?;
        let Some(index) = absolute_tile_index(x, y) else {
            continue;
        };
        if plan.npcs.len() >= MAX_PLANNED_NPCS_PER_OMT {
            return Err(SimError::InvalidNpcDialogue);
        }
        let (generated_name, generated_gender) =
            match placement.generated_name_category_ids.as_slice() {
                [] => (None, None),
                [category_id] => (
                    Some(expand_npc_name(catalog, category_id, rng)?),
                    match category_id.as_str() {
                        "<male_full_name>" => Some(String::from("male")),
                        "<female_full_name>" => Some(String::from("female")),
                        _ => return Err(SimError::InvalidNpcDialogue),
                    },
                ),
                [male_category_id, female_category_id] => {
                    let (category_id, gender) = if inclusive_rng_u64(rng, 0, 1) == 0 {
                        (male_category_id, "male")
                    } else {
                        (female_category_id, "female")
                    };
                    (
                        Some(expand_npc_name(catalog, category_id, rng)?),
                        Some(gender.to_owned()),
                    )
                }
                _ => return Err(SimError::InvalidNpcDialogue),
            };
        plan.npcs.push((
            index,
            placement.template_id.clone(),
            generated_name,
            generated_gender,
        ));
    }
    Ok(())
}

fn plan_vehicle_placement(
    catalog: &WorldgenCatalogV1,
    placement: &WorldgenVehiclePlacementV1,
    offset_x: i32,
    offset_y: i32,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let repeat = if placement.repeat.minimum == placement.repeat.maximum {
        placement.repeat.minimum
    } else {
        choose_worldgen_u16_range(placement.repeat, rng)?
    };
    for _ in 0..repeat {
        // Pinned `x_in_y` always evaluates its random distribution, including
        // the 0% and 100% boundaries.
        let chance_roll = inclusive_rng_u64(rng, 0, 99);
        if chance_roll >= u64::from(placement.chance_percent) {
            continue;
        }
        let group = catalog
            .vehicle_groups
            .get(usize::from(placement.group_index))
            .ok_or(SimError::InvalidTerrain)?;
        let total_weight = group.entries.iter().try_fold(0_u64, |total, entry| {
            total.checked_add(u64::from(entry.weight))
        });
        let entry_index = choose_weighted_index(
            group.entries.len(),
            total_weight.ok_or(SimError::NumericOverflow)?,
            rng,
            |index| u64::from(group.entries[index].weight),
            true,
        )?;
        let prototype_index = group
            .entries
            .get(entry_index)
            .ok_or(SimError::InvalidTerrain)?
            .prototype_index;
        let prototype = catalog
            .vehicle_prototypes
            .get(usize::from(prototype_index))
            .ok_or(SimError::InvalidTerrain)?;
        let x = offset_x
            .checked_add(i32::from(choose_i8_range(
                placement.x.minimum,
                placement.x.maximum,
                rng,
            )?))
            .ok_or(SimError::NumericOverflow)?;
        let y = offset_y
            .checked_add(i32::from(choose_i8_range(
                placement.y.minimum,
                placement.y.maximum,
                rng,
            )?))
            .ok_or(SimError::NumericOverflow)?;
        let Some(origin_index) = absolute_tile_index(x, y) else {
            continue;
        };
        let rotation_index = if placement.rotations_degrees.len() == 1 {
            0
        } else {
            usize::try_from(inclusive_rng_u64(
                rng,
                0,
                u64::try_from(placement.rotations_degrees.len() - 1)
                    .map_err(|_| SimError::NumericOverflow)?,
            ))
            .map_err(|_| SimError::NumericOverflow)?
        };
        let facing_degrees = *placement
            .rotations_degrees
            .get(rotation_index)
            .ok_or(SimError::InvalidTerrain)?;
        let mut parts = initial_vehicle_parts(
            catalog,
            prototype,
            placement.status,
            placement.fuel_percent,
            placement.faction_id.is_empty(),
            rng,
        )?;
        plan_vehicle_cargo(catalog, prototype, &mut parts, item_groups, rng)?;
        let cargo_objects = parts
            .iter()
            .flat_map(|part| &part.cargo)
            .try_fold(0_u64, |total, item| total.checked_add(item.object_count()?))
            .ok_or(SimError::NumericOverflow)?;
        plan.item_object_count = plan
            .item_object_count
            .checked_add(cargo_objects)
            .filter(|count| *count <= ID_RESERVATION_SIZE)
            .ok_or(SimError::InvalidItem)?;
        if plan.vehicles.len() >= MAX_PLANNED_VEHICLES_PER_OMT {
            return Err(SimError::InvalidTerrain);
        }
        plan.vehicles.push(PlannedMapgenVehicle {
            prototype_index,
            origin_index,
            facing_degrees,
            owner_faction_id: placement.faction_id.clone(),
            parts,
        });
    }
    Ok(())
}

fn initial_vehicle_parts(
    catalog: &WorldgenCatalogV1,
    prototype: &cdda_protocol::WorldgenVehiclePrototypeV1,
    status: VehicleSpawnStatusV1,
    fuel_percent: i16,
    unowned: bool,
    rng: &mut ChaCha8Rng,
) -> Result<Vec<PlannedVehiclePartState>, SimError> {
    let disabled_failure = if status == VehicleSpawnStatusV1::Disabled {
        u8::try_from(inclusive_rng_u64(rng, 1, 5)).map_err(|_| SimError::NumericOverflow)?
    } else {
        0
    };
    let mut has_no_key = one_in_mapgen(rng, 3);
    let mut destroy_alarm = !one_in_mapgen(rng, 3);
    let mut destroy_engine = disabled_failure == 4 || one_in_mapgen(rng, 3);
    if status == VehicleSpawnStatusV1::Pristine {
        has_no_key = false;
        destroy_alarm = false;
        destroy_engine = false;
    }
    let undamaged = matches!(
        status,
        VehicleSpawnStatusV1::Undamaged | VehicleSpawnStatusV1::Pristine
    );
    let has_engine = prototype.parts.iter().any(|part| {
        catalog
            .vehicle_part_types
            .get(usize::from(part.part_type_index))
            .is_some_and(|part_type| vehicle_part_has_flag(part_type, "ENGINE"))
    });
    if !undamaged {
        if fuel_percent != 0 && has_engine {
            let _ = one_in_mapgen(rng, 4);
        }
        for denominator in [20_u64, 20, 16, 8, 4, 4, 2] {
            let _ = one_in_mapgen(rng, denominator);
        }
    }
    let blood_covered = !undamaged && one_in_mapgen(rng, 10);
    let blood_inside = !undamaged && one_in_mapgen(rng, 8);
    let mut blood_inside_mount = None::<(i16, i16)>;
    let mut output = Vec::with_capacity(prototype.parts.len());
    for part in &prototype.parts {
        let part_type = catalog
            .vehicle_part_types
            .get(usize::from(part.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        let mut open = vehicle_part_has_flag(part_type, "OPENABLE") && one_in_mapgen(rng, 4);
        let mut hp =
            super::vehicles::initial_vehicle_part_hp(part_type.durability, undamaged, rng)?;
        if !undamaged {
            if destroy_engine && vehicle_part_has_flag(part_type, "ENGINE") {
                hp = 0;
            } else if (disabled_failure == 1
                && (vehicle_part_has_flag(part_type, "SEAT")
                    || vehicle_part_has_flag(part_type, "SEATBELT")))
                || (disabled_failure == 2
                    && (vehicle_part_has_flag(part_type, "CONTROLS")
                        || vehicle_part_has_flag(part_type, "SECURITY")))
                || (destroy_alarm && vehicle_part_has_flag(part_type, "SECURITY"))
                || (disabled_failure == 3
                    && (vehicle_part_has_flag(part_type, "FUEL_TANK")
                        || vehicle_part_has_flag(part_type, "FUEL_STORE")))
            {
                hp = 0;
            }
            if vehicle_part_has_flag(part_type, "SOLAR_PANEL") && one_in_mapgen(rng, 4) {
                hp = 0;
            }
            if blood_covered && part.mount_x > 0 {
                if one_in_mapgen(rng, 3) {
                    let _ = inclusive_rng_u64(rng, 200, 600);
                } else {
                    let _ = inclusive_rng_u64(rng, 50, 200);
                }
            }
            if blood_inside {
                if let Some((center_x, center_y)) = blood_inside_mount {
                    let dx = i32::from(center_x) - i32::from(part.mount_x);
                    let dy = i32::from(center_y) - i32::from(part.mount_y);
                    if dx * dx + dy * dy <= 1 {
                        let _ = inclusive_rng_u64(rng, 200, 400);
                    }
                } else if vehicle_part_has_flag(part_type, "SEAT") {
                    blood_inside_mount = Some((part.mount_x, part.mount_y));
                }
            }
        }
        let locked = has_no_key
            && hp > 0
            && (vehicle_part_has_flag(part_type, "LOCKABLE_DOOR")
                || vehicle_part_has_flag(part_type, "LOCKABLE_CARGO"));
        if locked {
            open = false;
            let _ = one_in_mapgen(rng, 2);
        }
        output.push(PlannedVehiclePartState {
            hp,
            enabled: unowned && vehicle_part_has_flag(part_type, "ENGINE"),
            open,
            locked,
            cargo: Vec::new(),
        });
    }
    if status == VehicleSpawnStatusV1::Disabled {
        if disabled_failure == 5 {
            for (index, part) in output.iter_mut().enumerate() {
                let prototype_part = prototype.parts.get(index).ok_or(SimError::InvalidTerrain)?;
                let part_type = catalog
                    .vehicle_part_types
                    .get(usize::from(prototype_part.part_type_index))
                    .ok_or(SimError::InvalidTerrain)?;
                if vehicle_part_has_flag(part_type, "WHEEL") {
                    part.hp = 0;
                }
            }
        }
        if one_in_mapgen(rng, 2) {
            for part in &mut output {
                part.hp /= 2;
            }
        }
    }
    Ok(output)
}

fn plan_vehicle_cargo(
    catalog: &WorldgenCatalogV1,
    prototype: &cdda_protocol::WorldgenVehiclePrototypeV1,
    parts: &mut [PlannedVehiclePartState],
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    for spawn in &prototype.item_spawns {
        let cargo_part = parts
            .get_mut(usize::from(spawn.cargo_prototype_part_index))
            .ok_or(SimError::InvalidTerrain)?;
        let prototype_part = prototype
            .parts
            .get(usize::from(spawn.cargo_prototype_part_index))
            .ok_or(SimError::InvalidTerrain)?;
        let part_type = catalog
            .vehicle_part_types
            .get(usize::from(prototype_part.part_type_index))
            .ok_or(SimError::InvalidTerrain)?;
        let mut stored_volume = cargo_part.cargo.iter().try_fold(0_u64, |total, item| {
            total.checked_add(item.total_volume_milliliters()?)
        });
        let Some(mut stored_volume) = stored_volume.take() else {
            return Err(SimError::InvalidItem);
        };
        let spawn_count = match spawn.chance_percent {
            0 => 0,
            100 => 1,
            chance => usize::from(rng.next_u64() % 100 < u64::from(chance)),
        };
        for _ in 0..spawn_count {
            let broken = cargo_part.hp == 0;
            if broken && one_in_mapgen(rng, 2) {
                continue;
            }
            let mut created = Vec::new();
            for direct in &spawn.direct_items {
                // Pinned `rng_float( 0, 1 ) < ITEM_SPAWNRATE` still consumes
                // its constructor-adjacent draw at the default rate of one.
                let _ = rng.next_u64();
                created.push(plan_vehicle_direct_item(direct, rng)?);
            }
            for group_id in &spawn.item_group_ids {
                created.extend(plan_item_group_source(
                    &ItemGroupSourceV1::Group(group_id.clone()),
                    item_groups,
                    rng,
                )?);
            }
            for mut item in created {
                if broken && !damage_vehicle_spawn_item(&mut item, rng)? {
                    continue;
                }
                dress_vehicle_spawn_item(
                    &mut item,
                    spawn.with_ammo_percent,
                    spawn.with_magazine_percent,
                    rng,
                )?;
                if cargo_part.cargo.len() >= cdda_protocol::MAX_VEHICLE_CARGO_ITEMS_PER_PART {
                    continue;
                }
                let item_volume = item
                    .total_volume_milliliters()
                    .ok_or(SimError::InvalidItem)?;
                let Some(next_volume) = stored_volume.checked_add(item_volume) else {
                    continue;
                };
                if next_volume > part_type.cargo_capacity_milliliters {
                    continue;
                }
                stored_volume = next_volume;
                cargo_part.cargo.push(item);
            }
        }
    }
    Ok(())
}

fn vehicle_part_has_flag(part: &cdda_protocol::WorldgenVehiclePartTypeV1, flag: &str) -> bool {
    part.flags
        .binary_search_by(|candidate| candidate.as_str().cmp(flag))
        .is_ok()
}

fn expand_npc_name(
    catalog: &WorldgenCatalogV1,
    category_id: &str,
    rng: &mut ChaCha8Rng,
) -> Result<String, SimError> {
    fn expand_category(
        catalog: &WorldgenCatalogV1,
        category_id: &str,
        rng: &mut ChaCha8Rng,
        output: &mut String,
        depth: usize,
    ) -> Result<(), SimError> {
        if depth > cdda_protocol::MAX_WORLDGEN_NPC_NAME_EXPANSION_DEPTH {
            return Err(SimError::InvalidNpcDialogue);
        }
        let category = catalog
            .npc_name_categories
            .binary_search_by(|category| category.category_id.as_str().cmp(category_id))
            .ok()
            .and_then(|index| catalog.npc_name_categories.get(index))
            .ok_or(SimError::InvalidNpcDialogue)?;
        let total = category
            .choices
            .iter()
            .try_fold(0_u64, |total, choice| total.checked_add(choice.weight))
            .ok_or(SimError::NumericOverflow)?;
        let index = choose_weighted_index(
            category.choices.len(),
            total,
            rng,
            |index| category.choices[index].weight,
            true,
        )?;
        let text = category
            .choices
            .get(index)
            .ok_or(SimError::InvalidNpcDialogue)?
            .text
            .clone();
        let mut rest = text.as_str();
        while let Some(start) = rest.find(['<', '>']) {
            if rest.as_bytes()[start] == b'>' {
                return Err(SimError::InvalidNpcDialogue);
            }
            output.push_str(&rest[..start]);
            let suffix = &rest[start..];
            let end = suffix.find('>').ok_or(SimError::InvalidNpcDialogue)?;
            expand_category(catalog, &suffix[..=end], rng, output, depth + 1)?;
            if output.len() > cdda_protocol::MAX_NPC_NAME_BYTES {
                return Err(SimError::InvalidNpcDialogue);
            }
            rest = &suffix[end + 1..];
        }
        output.push_str(rest);
        if output.len() > cdda_protocol::MAX_NPC_NAME_BYTES {
            return Err(SimError::InvalidNpcDialogue);
        }
        Ok(())
    }

    let mut output = String::new();
    expand_category(catalog, category_id, rng, &mut output, 0)?;
    if output.is_empty() || output.chars().any(char::is_control) {
        return Err(SimError::InvalidNpcDialogue);
    }
    Ok(output)
}

fn plan_monster_placement_request(
    placement: &WorldgenMonsterPlacementV1,
    offset_x: i32,
    offset_y: i32,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let mut candidates = Vec::new();
    for y in placement.y.minimum..=placement.y.maximum {
        for x in placement.x.minimum..=placement.x.maximum {
            let x = offset_x
                .checked_add(i32::from(x))
                .ok_or(SimError::NumericOverflow)?;
            let y = offset_y
                .checked_add(i32::from(y))
                .ok_or(SimError::NumericOverflow)?;
            if let Some(index) = absolute_tile_index(x, y) {
                candidates.push(index);
            }
        }
    }
    if !candidates.is_empty() {
        plan.monster_placements.push(PlannedMonsterPlacement {
            placement: placement.clone(),
            candidates,
        });
    }
    Ok(())
}

fn plan_individual_monster_placement_request(
    placement: &WorldgenIndividualMonsterPlacementV1,
    offset_x: i32,
    offset_y: i32,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let mut candidates = Vec::new();
    for y in placement.y.minimum..=placement.y.maximum {
        for x in placement.x.minimum..=placement.x.maximum {
            let x = offset_x
                .checked_add(i32::from(x))
                .ok_or(SimError::NumericOverflow)?;
            let y = offset_y
                .checked_add(i32::from(y))
                .ok_or(SimError::NumericOverflow)?;
            if let Some(index) = absolute_tile_index(x, y) {
                candidates.push(index);
            }
        }
    }
    if !candidates.is_empty() {
        plan.individual_monster_placements
            .push(PlannedIndividualMonsterPlacement {
                placement: placement.clone(),
                candidates,
            });
    }
    Ok(())
}

fn materialize_monster_placements(
    catalog: &WorldgenCatalogV1,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let requests = std::mem::take(&mut plan.monster_placements);
    let mut occupied = plan
        .monsters
        .iter()
        .map(|(index, _)| *index)
        .collect::<std::collections::BTreeSet<_>>();
    for request in requests {
        let repeat = choose_worldgen_u16_range(request.placement.repeat, rng)?;
        for _ in 0..repeat {
            let chance = choose_worldgen_u16_range(request.placement.chance, rng)?;
            if !one_in_mapgen(rng, u64::from(chance)) {
                continue;
            }
            let mut quantity = monster_density_count(request.placement.density_millionths, rng)?;
            materialize_monster_group_quantity(
                catalog,
                request.placement.group_index,
                &mut quantity,
                &request.candidates,
                plan,
                &mut occupied,
                rng,
            )?;
        }
    }
    let individual_requests = std::mem::take(&mut plan.individual_monster_placements);
    for request in individual_requests {
        let repeat = choose_worldgen_u16_range(request.placement.repeat, rng)?;
        for _ in 0..repeat {
            let chance = choose_worldgen_u16_range(request.placement.chance_percent, rng)?;
            if inclusive_rng_u64(rng, 1, 100) > u64::from(chance) {
                continue;
            }
            let pack_size = choose_worldgen_u16_range(request.placement.pack_size, rng)?;
            let mut prototypes = Vec::new();
            match request.placement.target {
                WorldgenIndividualMonsterTargetV1::Monster { prototype_index } => {
                    let prototype = catalog
                        .monster_prototypes
                        .get(usize::from(prototype_index))
                        .ok_or(SimError::InvalidCreature)?;
                    if prototype.runtime_spawnable {
                        prototypes.push(prototype_index);
                    }
                }
                WorldgenIndividualMonsterTargetV1::Group { group_index } => {
                    let mut quantity = 1_i64;
                    let mut results = Vec::new();
                    select_monster_group(
                        catalog,
                        group_index,
                        &mut quantity,
                        false,
                        rng,
                        &mut results,
                        0,
                    )?;
                    prototypes.extend(
                        results
                            .into_iter()
                            .map(|(prototype_index, _)| prototype_index),
                    );
                }
            }
            for prototype_index in prototypes {
                for _ in 0..pack_size {
                    let Some(index) = choose_monster_position(
                        &request.candidates,
                        &plan.terrain,
                        &plan.furniture,
                        &occupied,
                        rng,
                    ) else {
                        continue;
                    };
                    if plan.monsters.len() >= MAX_PLANNED_CREATURES_PER_OMT {
                        return Err(SimError::InvalidCreature);
                    }
                    occupied.insert(index);
                    plan.monsters.push((index, prototype_index));
                }
            }
        }
    }
    Ok(())
}

fn materialize_special_populations(
    world_seed: [u8; 32],
    catalog: &WorldgenCatalogV1,
    omt: ChunkCoord,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    if omt.z != 0 {
        return Ok(());
    }
    let candidates = (0..OMT_TILE_COUNT).collect::<Vec<_>>();
    let mut occupied = plan
        .monsters
        .iter()
        .map(|(index, _)| *index)
        .collect::<std::collections::BTreeSet<_>>();
    for special in &catalog.specials {
        let Some(population) = &special.population else {
            continue;
        };
        let mut parameter_rng = coordinate_rng(
            world_seed,
            catalog.generator_version,
            special.origin,
            &format!("special-population:{}", special.placement_id.0),
        );
        let radius = u16::try_from(inclusive_rng_u64(
            &mut parameter_rng,
            u64::from(population.radius.minimum),
            u64::from(population.radius.maximum),
        ))
        .map_err(|_| SimError::NumericOverflow)?;
        let total_population = inclusive_rng_u64(
            &mut parameter_rng,
            u64::from(population.population.minimum),
            u64::from(population.population.maximum),
        );
        let eligible = special_population_omts(catalog, special.origin, radius)?;
        let Some(index) = eligible.iter().position(|candidate| *candidate == omt) else {
            continue;
        };
        let cell_count = u64::try_from(eligible.len()).map_err(|_| SimError::NumericOverflow)?;
        let extra_start = usize::try_from(inclusive_rng_u64(
            &mut parameter_rng,
            0,
            cell_count.checked_sub(1).ok_or(SimError::InvalidCreature)?,
        ))
        .map_err(|_| SimError::NumericOverflow)?;
        let base = total_population / cell_count;
        let remainder = usize::try_from(total_population % cell_count)
            .map_err(|_| SimError::NumericOverflow)?;
        let relative = (index + eligible.len() - extra_start) % eligible.len();
        let mut quantity = i64::try_from(base + u64::from(relative < remainder))
            .map_err(|_| SimError::NumericOverflow)?;
        if quantity == 0 {
            continue;
        }
        let group_index = catalog
            .monster_groups
            .binary_search_by(|group| group.group_id.as_str().cmp(&population.group_id))
            .ok()
            .and_then(|index| u16::try_from(index).ok())
            .ok_or(SimError::InvalidCreature)?;
        let mut spawn_rng = coordinate_rng(
            world_seed,
            catalog.generator_version,
            omt,
            &format!("special-population-spawn:{}", special.placement_id.0),
        );
        materialize_monster_group_quantity(
            catalog,
            group_index,
            &mut quantity,
            &candidates,
            plan,
            &mut occupied,
            &mut spawn_rng,
        )?;
    }
    Ok(())
}

fn special_population_omts(
    catalog: &WorldgenCatalogV1,
    origin: ChunkCoord,
    radius: u16,
) -> Result<Vec<ChunkCoord>, SimError> {
    let maximum_x = catalog
        .overmap
        .origin_x
        .checked_add(i32::from(cdda_protocol::WORLDGEN_OVERMAP_WIDTH) - 1)
        .ok_or(SimError::NumericOverflow)?;
    let maximum_y = catalog
        .overmap
        .origin_y
        .checked_add(i32::from(cdda_protocol::WORLDGEN_OVERMAP_HEIGHT) - 1)
        .ok_or(SimError::NumericOverflow)?;
    let radius = i32::from(radius);
    let radius_squared = i64::from(radius) * i64::from(radius);
    let minimum_x = origin
        .x
        .saturating_sub(radius)
        .max(catalog.overmap.origin_x);
    let maximum_x = origin.x.saturating_add(radius).min(maximum_x);
    let minimum_y = origin
        .y
        .saturating_sub(radius)
        .max(catalog.overmap.origin_y);
    let maximum_y = origin.y.saturating_add(radius).min(maximum_y);
    Ok((minimum_y..=maximum_y)
        .flat_map(|y| (minimum_x..=maximum_x).map(move |x| ChunkCoord { x, y, z: 0 }))
        .filter(|candidate| {
            let dx = i64::from(candidate.x) - i64::from(origin.x);
            let dy = i64::from(candidate.y) - i64::from(origin.y);
            dx * dx + dy * dy <= radius_squared
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn materialize_monster_group_quantity(
    catalog: &WorldgenCatalogV1,
    group_index: u16,
    quantity: &mut i64,
    candidates: &[usize],
    plan: &mut OmtMapgenPlan,
    occupied: &mut std::collections::BTreeSet<usize>,
    rng: &mut ChaCha8Rng,
) -> Result<(), SimError> {
    let mut iterations = 0_usize;
    while *quantity > 0 {
        iterations = iterations
            .checked_add(1)
            .filter(|count| *count <= MAX_PLANNED_CREATURES_PER_OMT)
            .ok_or(SimError::InvalidCreature)?;
        let first_position =
            choose_monster_position(candidates, &plan.terrain, &plan.furniture, occupied, rng);
        let mut results = Vec::new();
        select_monster_group(catalog, group_index, quantity, false, rng, &mut results, 0)?;
        for (prototype_index, pack_size) in results {
            for member in 0..pack_size {
                let index = if member == 0 {
                    first_position.filter(|index| !occupied.contains(index))
                } else {
                    choose_monster_position(
                        candidates,
                        &plan.terrain,
                        &plan.furniture,
                        occupied,
                        rng,
                    )
                };
                let Some(index) = index else {
                    continue;
                };
                if plan.monsters.len() >= MAX_PLANNED_CREATURES_PER_OMT {
                    return Err(SimError::InvalidCreature);
                }
                occupied.insert(index);
                plan.monsters.push((index, prototype_index));
            }
        }
    }
    Ok(())
}

fn choose_worldgen_u16_range(
    range: cdda_protocol::WorldgenU16RangeV1,
    rng: &mut ChaCha8Rng,
) -> Result<u16, SimError> {
    if range.minimum > range.maximum {
        return Err(SimError::InvalidCreature);
    }
    u16::try_from(inclusive_rng_u64(
        rng,
        u64::from(range.minimum),
        u64::from(range.maximum),
    ))
    .map_err(|_| SimError::NumericOverflow)
}

fn monster_density_count(density_millionths: u32, rng: &mut ChaCha8Rng) -> Result<i64, SimError> {
    if density_millionths == 0 {
        return Ok(0);
    }
    let random_millionths = inclusive_rng_u64(rng, 10_000_000, 50_000_000);
    let scaled = u128::from(density_millionths)
        .checked_mul(u128::from(random_millionths))
        .and_then(|value| value.checked_div(1_000_000))
        .ok_or(SimError::NumericOverflow)?;
    let whole = scaled / 1_000_000;
    let remainder = u64::try_from(scaled % 1_000_000).map_err(|_| SimError::NumericOverflow)?;
    let rounded = whole
        .checked_add(u128::from(
            remainder > 0 && inclusive_rng_u64(rng, 1, 1_000_000) <= remainder,
        ))
        .ok_or(SimError::NumericOverflow)?;
    if rounded > MAX_PLANNED_CREATURES_PER_OMT as u128 {
        return Err(SimError::InvalidCreature);
    }
    i64::try_from(rounded).map_err(|_| SimError::NumericOverflow)
}

fn choose_monster_position(
    candidates: &[usize],
    terrain: &[Option<PlannedTerrainTile>],
    furniture: &[Option<PlannedFurnitureTile>],
    occupied: &std::collections::BTreeSet<usize>,
    rng: &mut ChaCha8Rng,
) -> Option<usize> {
    for _ in 0..10 {
        let candidate = candidates.get(
            usize::try_from(inclusive_rng_u64(
                rng,
                0,
                u64::try_from(candidates.len().checked_sub(1)?).ok()?,
            ))
            .ok()?,
        )?;
        let passable_terrain = matches!(
            terrain.get(*candidate),
            Some(Some(PlannedTerrainTile::Resolved(tile))) if tile.move_cost > 0
        );
        let passable_furniture = matches!(
            furniture.get(*candidate),
            Some(Some(PlannedFurnitureTile::Resolved(None)))
                | Some(Some(PlannedFurnitureTile::Resolved(Some(
                    FurnitureTileSnapshot {
                        move_cost_mod: 0..,
                        ..
                    }
                ))))
        );
        if passable_terrain && passable_furniture && !occupied.contains(candidate) {
            return Some(*candidate);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn select_monster_group(
    catalog: &WorldgenCatalogV1,
    group_index: u16,
    quantity: &mut i64,
    recursive: bool,
    rng: &mut ChaCha8Rng,
    output: &mut Vec<(u16, u16)>,
    depth: usize,
) -> Result<bool, SimError> {
    if depth >= cdda_protocol::MAX_WORLDGEN_MONSTER_GROUP_DEPTH {
        return Err(SimError::InvalidCreature);
    }
    let group = catalog
        .monster_groups
        .get(usize::from(group_index))
        .ok_or(SimError::InvalidCreature)?;
    let mut ticket = inclusive_rng_u64(rng, 1, u64::from(group.frequency_total));
    let mut found = false;
    for entry in &group.entries {
        if u64::from(entry.weight) < ticket {
            ticket -= u64::from(entry.weight);
            continue;
        }
        let pack_size = choose_worldgen_u16_range(entry.pack_size, rng)?;
        match entry.target {
            WorldgenMonsterGroupTargetV1::Monster { prototype_index } => {
                let cost = i64::from(entry.cost_multiplier)
                    .saturating_mul(i64::from(pack_size))
                    .max(1);
                *quantity = quantity.saturating_sub(cost);
                let prototype = catalog
                    .monster_prototypes
                    .get(usize::from(prototype_index))
                    .ok_or(SimError::InvalidCreature)?;
                if prototype.runtime_spawnable {
                    output.push((prototype_index, pack_size));
                }
                found = true;
            }
            WorldgenMonsterGroupTargetV1::Group { group_index } => {
                for _ in 0..pack_size {
                    found |= select_monster_group(
                        catalog,
                        group_index,
                        quantity,
                        true,
                        rng,
                        output,
                        depth + 1,
                    )?;
                    if output.len() > MAX_PLANNED_CREATURES_PER_OMT {
                        return Err(SimError::InvalidCreature);
                    }
                }
            }
        }
        break;
    }
    if !recursive && !found {
        if let Some(prototype_index) = group.default_prototype_index {
            let prototype = catalog
                .monster_prototypes
                .get(usize::from(prototype_index))
                .ok_or(SimError::InvalidCreature)?;
            if prototype.runtime_spawnable {
                output.push((prototype_index, 1));
            }
        }
        *quantity = quantity.saturating_sub(1);
    }
    Ok(found)
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
    plan.record_nested_expansion()?;
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
        &template.npc_placements,
        &template.vehicle_placements,
        &template.monster_placements,
        &template.individual_monster_placements,
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
    )?;
    resolve_mapgen_plan(catalog, rng, plan)
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
        plan_item_placement_once(placement, index, item_groups, rng, plan)?;
    }
    Ok(())
}

fn plan_area_item_placement(
    placement: &cdda_protocol::WorldgenAreaItemPlacementV1,
    offset_x: i32,
    offset_y: i32,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    for _ in 0..choose_repeat(&placement.item_group, rng)? {
        if !item_placement_succeeds(placement.item_group.chance, rng) {
            continue;
        }
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
        plan_item_group_at(&placement.item_group, index, item_groups, rng, plan)?;
    }
    Ok(())
}

fn plan_item_placement_once(
    placement: &cdda_protocol::WorldgenItemGroupPlacementV1,
    index: usize,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    if !item_placement_succeeds(placement.chance, rng) {
        return Ok(());
    }
    plan_item_group_at(placement, index, item_groups, rng, plan)
}

fn plan_item_group_at(
    placement: &cdda_protocol::WorldgenItemGroupPlacementV1,
    index: usize,
    item_groups: &std::collections::BTreeMap<String, ItemGroupDefinitionV1>,
    rng: &mut ChaCha8Rng,
    plan: &mut OmtMapgenPlan,
) -> Result<(), SimError> {
    let prototypes = plan_item_group_source(
        &ItemGroupSourceV1::Group(placement.group_id.clone()),
        item_groups,
        rng,
    )?;
    for prototype in prototypes {
        plan.push_item(index, prototype)?;
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

    fn test_terrain(terrain_id: &str) -> cdda_protocol::TerrainTileSnapshot {
        cdda_protocol::TerrainTileSnapshot {
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
        }
    }

    fn test_item(type_id: &str) -> cdda_protocol::ItemGroupItemPrototypeV1 {
        cdda_protocol::ItemGroupItemPrototypeV1 {
            prototype: cdda_protocol::CraftItemPrototypeV1 {
                type_id: type_id.to_owned(),
                charges: 1,
                melee_damage_milli: std::collections::BTreeMap::new(),
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
                containment: Default::default(),
            },
            maximum_raw_damage: cdda_protocol::MAX_ITEM_RAW_DAMAGE,
            variants: Vec::new(),
            description_expansion: None,
            snippets: Vec::new(),
            initial_variables: std::collections::BTreeMap::new(),
            default_container: None,
            modifier_side_effects_supported: true,
            charges: None,
            minimum_one_charge: false,
            tool_charge_storage: None,
            charges_supported: true,
            charge_capacity: cdda_protocol::ItemGroupChargeCapacityV1::ModifierContainer,
            contents_insertion_supported: true,
        }
    }

    fn terrain_cell(target: Option<WorldgenTerrainTargetV1>) -> cdda_protocol::WorldgenCellV1 {
        cdda_protocol::WorldgenCellV1 {
            terrain: target.map_or_else(Vec::new, |target| {
                vec![vec![WorldgenWeightedTerrainTargetV1 { target, weight: 1 }]]
            }),
            furniture: Vec::new(),
            item_group: None,
        }
    }

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

    #[test]
    fn area_item_repeats_draw_chance_before_fresh_coordinates() {
        let group = ItemGroupDefinitionV1 {
            group_id: String::from("empty"),
            graph: cdda_protocol::ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![cdda_protocol::ItemGroupNodeV1 {
                    node_id: 0,
                    kind: cdda_protocol::ItemGroupKindV1::Collection,
                    entries: Vec::new(),
                }],
                wrapper: None,
            },
        };
        let groups = std::collections::BTreeMap::from([(group.group_id.clone(), group)]);
        let placement = cdda_protocol::WorldgenAreaItemPlacementV1 {
            item_group: cdda_protocol::WorldgenItemGroupPlacementV1 {
                group_id: String::from("empty"),
                chance: 100,
                repeat_minimum: 2,
                repeat_maximum: 2,
            },
            x: cdda_protocol::WorldgenCoordinateRangeV1 {
                minimum: 1,
                maximum: 2,
            },
            y: cdda_protocol::WorldgenCoordinateRangeV1 {
                minimum: 3,
                maximum: 4,
            },
        };
        let mut actual = ChaCha8Rng::from_seed([47; 32]);
        let mut exact_trace = actual.clone();
        for _ in 0..2 {
            let _ = choose_i8_range(1, 2, &mut exact_trace).expect("valid x range");
            let _ = choose_i8_range(3, 4, &mut exact_trace).expect("valid y range");
        }
        plan_area_item_placement(
            &placement,
            0,
            0,
            &groups,
            &mut actual,
            &mut OmtMapgenPlan::new(),
        )
        .expect("empty group still characterizes placement RNG");
        assert_eq!(actual.next_u64(), exact_trace.next_u64());

        let mut failed = ChaCha8Rng::from_seed([59; 32]);
        let mut failed_trace = failed.clone();
        let mut failed_placement = placement;
        failed_placement.item_group.chance = 1;
        failed_placement.item_group.repeat_minimum = 1;
        failed_placement.item_group.repeat_maximum = 1;
        assert!(!item_placement_succeeds(1, &mut failed_trace));
        plan_area_item_placement(
            &failed_placement,
            0,
            0,
            &groups,
            &mut failed,
            &mut OmtMapgenPlan::new(),
        )
        .expect("failed chance is a normal no-placement branch");
        assert_eq!(
            failed.next_u64(),
            failed_trace.next_u64(),
            "failed chance must not consume coordinate draws"
        );
    }

    #[test]
    fn predecessor_phases_resolve_before_overlays_and_keep_their_own_rotation() {
        let mut base_cells =
            vec![terrain_cell(Some(WorldgenTerrainTargetV1::Regional(0))); OMT_TILE_COUNT];
        let base_marker_source = 5 * OMT_TILE_WIDTH + 2;
        base_cells[base_marker_source] = terrain_cell(Some(WorldgenTerrainTargetV1::Prototype(1)));
        let overlay_cells = vec![terrain_cell(None); OMT_TILE_COUNT];
        let mut top_cells = overlay_cells.clone();
        top_cells[0].terrain = vec![vec![
            WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(2),
                weight: 1,
            },
            WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(3),
                weight: 1,
            },
            WorldgenWeightedTerrainTargetV1 {
                target: WorldgenTerrainTargetV1::Prototype(4),
                weight: 1,
            },
        ]];
        let template = |predecessor_id: Option<&str>, cells| cdda_protocol::WorldgenTemplateV1 {
            weight: 1,
            predecessor_id: predecessor_id.map(str::to_owned),
            builtin: None,
            cells,
            nested: Vec::new(),
            area_items: Vec::new(),
            npc_placements: Vec::new(),
            omitted_npc_placement_count: 0,
            monster_placements: Vec::new(),
            individual_monster_placements: Vec::new(),
            erase_all_before_placing_terrain: false,
            deferred_fields: Vec::new(),
        };
        let generator = |omt_id: &str, template| WorldgenOmtGeneratorV1 {
            omt_id: omt_id.to_owned(),
            templates: vec![template],
            nested_generators: Vec::new(),
        };
        let identity = |full_id: &str, rotation| cdda_protocol::WorldgenOmtIdentityV1 {
            full_id: full_id.to_owned(),
            type_id: full_id.to_owned(),
            subtype_id: full_id.to_owned(),
            generator_id: full_id.to_owned(),
            rotation,
        };
        let catalog = WorldgenCatalogV1 {
            generator_version: cdda_protocol::WORLDGEN_GENERATOR_VERSION_V2,
            overmap: cdda_protocol::WorldgenOvermapLayoutV1 {
                origin_x: 0,
                origin_y: 0,
                identities: vec![
                    identity("base", 2),
                    identity("middle", 3),
                    identity("top", 1),
                ],
                layers: Vec::new(),
            },
            cities: Vec::new(),
            rivers: Vec::new(),
            specials: Vec::new(),
            start_location: None,
            terrain_prototypes: vec![
                test_terrain("t_base"),
                test_terrain("t_base_marker"),
                test_terrain("t_top_a"),
                test_terrain("t_top_b"),
                test_terrain("t_top_c"),
            ],
            furniture_prototypes: Vec::new(),
            monster_prototypes: Vec::new(),
            monster_groups: Vec::new(),
            regional_terrain: vec![cdda_protocol::WorldgenRegionalTerrainTableV1 {
                regional_id: String::from("region_groundcover"),
                choices: vec![WorldgenWeightedPrototypeV1 {
                    prototype_index: 0,
                    weight: 1,
                }],
            }],
            regional_furniture: Vec::new(),
            npc_name_categories: Vec::new(),
            omt_generators: vec![
                generator("base", template(None, base_cells)),
                generator("middle", template(Some("base"), overlay_cells)),
                generator("top", template(Some("middle"), top_cells)),
            ],
        };
        let omt = ChunkCoord { x: 0, y: 0, z: 0 };
        let seed = [55; 32];
        let mut actual_rng = coordinate_rng(seed, catalog.generator_version, omt, "top");
        let mut plan = OmtMapgenPlan::new();
        apply_root_generator(
            &catalog,
            &catalog.omt_generators[2],
            &std::collections::BTreeMap::new(),
            omt,
            1,
            &mut actual_rng,
            &mut plan,
            0,
        )
        .expect("three-level predecessor chain should plan");

        let (marker_x, marker_y) = rotate_tile_xy(2, 5, 2).expect("base rotation");
        let marker_index = marker_y * OMT_TILE_WIDTH + marker_x;
        let Some(PlannedTerrainTile::Resolved(marker)) = &plan.terrain[marker_index] else {
            panic!("base marker should be resolved");
        };
        assert_eq!(marker.terrain_id, "t_base_marker");

        let mut phased_trace = coordinate_rng(seed, catalog.generator_version, omt, "top");
        for _ in 0..3 {
            let _ = phased_trace.next_u64();
        }
        let mut unphased_trace = phased_trace.clone();
        for _ in 0..(OMT_TILE_COUNT - 1) {
            let _ = phased_trace.next_u64();
        }
        let expected_top = ["t_top_a", "t_top_b", "t_top_c"]
            [usize::try_from(phased_trace.next_u64() % 3).expect("ticket fits")];
        let unphased_top = ["t_top_a", "t_top_b", "t_top_c"]
            [usize::try_from(unphased_trace.next_u64() % 3).expect("ticket fits")];
        assert_ne!(
            expected_top, unphased_top,
            "fixture must distinguish phases"
        );
        let (top_x, top_y) = rotate_tile_xy(0, 0, 1).expect("top rotation");
        let top_index = top_y * OMT_TILE_WIDTH + top_x;
        let Some(PlannedTerrainTile::Resolved(top)) = &plan.terrain[top_index] else {
            panic!("top marker should be resolved");
        };
        assert_eq!(top.terrain_id, expected_top);
        assert_eq!(actual_rng.next_u64(), phased_trace.next_u64());
    }

    #[test]
    fn nested_expansion_budget_rejects_the_first_out_of_bounds_placement() {
        let mut plan = OmtMapgenPlan::new();
        for _ in 0..cdda_protocol::MAX_WORLDGEN_NESTED_PLACEMENTS {
            plan.record_nested_expansion()
                .expect("the exact execution budget remains valid");
        }
        assert!(matches!(
            plan.record_nested_expansion(),
            Err(SimError::InvalidTerrain)
        ));
    }

    #[test]
    fn mapgen_item_budget_counts_recursively_owned_stable_objects() {
        let mut wrapper = test_item("loot_crate");
        wrapper.prototype.ammunition_containers =
            vec![cdda_protocol::AmmunitionContainerPocketPrototypeV1 {
                pocket_index: 0,
                pocket_id: String::from("CONTAINER"),
                capacities: Vec::new(),
                rigid: true,
                access_moves: 100,
                reloadable: false,
                unloadable: true,
                spawn_rules: Some(cdda_protocol::SpawnPocketRulesV1 {
                    kind: cdda_protocol::SpawnPocketKindV1::Container,
                    max_contains_volume_milliliters: 1_000_000,
                    magazine_well_volume_milliliters: 0,
                    contents_collapsed_by_default: false,
                    max_contains_weight_milligrams: 1_000_000,
                    max_item_volume_milliliters: 1_000_000,
                    min_item_volume_milliliters: 0,
                    max_item_length_millimeters: 1_000_000,
                    item_restrictions: Vec::new(),
                    flag_restrictions: Vec::new(),
                    access_moves: 100,
                    rigid: true,
                    watertight: false,
                    transparent: false,
                    forbidden: false,
                    sealable: false,
                }),
            }];
        let definition = ItemGroupDefinitionV1 {
            group_id: String::from("nested_loot"),
            graph: cdda_protocol::ItemGroupGraphV1 {
                root_node: 0,
                nodes: vec![cdda_protocol::ItemGroupNodeV1 {
                    node_id: 0,
                    kind: cdda_protocol::ItemGroupKindV1::Collection,
                    entries: vec![cdda_protocol::ItemGroupEntryV1 {
                        probability: 100,
                        count_min: 255,
                        count_max: 255,
                        raw_damage: Some(cdda_protocol::InclusiveU16RangeV1 {
                            minimum: 0,
                            maximum: 0,
                        }),
                        variant_id: None,
                        event: None,
                        target: cdda_protocol::ItemGroupTargetV1::Item(Box::new(test_item("rock"))),
                        modifier_charges: None,
                        contents: Vec::new(),
                        seal_contents: false,
                        modifier_default_container_sealed: None,
                        direct_wrapper: Some(cdda_protocol::ItemGroupContainerV1 {
                            item: Box::new(wrapper),
                            variant_id: None,
                            sealed: false,
                            overflow: cdda_protocol::ItemGroupOverflowV1::None,
                        }),
                        modifier_container: None,
                    }],
                }],
                wrapper: None,
            },
        };
        let definitions =
            std::collections::BTreeMap::from([(definition.group_id.clone(), definition)]);
        let mut rng = ChaCha8Rng::from_seed([61; 32]);
        let [nested] = plan_item_group_source(
            &ItemGroupSourceV1::Group(String::from("nested_loot")),
            &definitions,
            &mut rng,
        )
        .expect("nested group should plan")
        .try_into()
        .expect("one wrapper should own all 255 children");
        assert_eq!(nested.object_count(), Some(256));

        let mut plan = OmtMapgenPlan::new();
        for _ in 0..128 {
            plan.push_item(0, nested.clone())
                .expect("exactly 32,768 recursive objects should fit");
        }
        assert_eq!(plan.item_object_count, ID_RESERVATION_SIZE);
        let top_level_before = plan.items.len();
        assert!(matches!(
            plan.push_item(0, nested),
            Err(SimError::InvalidItem)
        ));
        assert_eq!(plan.items.len(), top_level_before);
        assert_eq!(plan.item_object_count, ID_RESERVATION_SIZE);
    }
}
