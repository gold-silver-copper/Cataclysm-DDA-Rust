use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};
use unicode_width::UnicodeWidthChar;

use crate::{
    ContentManifest, FurnitureRegistry, ItemGroupRegistry, ModCatalog, ModCatalogError,
    OvermapTerrainMatchType, SelectedContentFile, TerrainRegistry,
};

pub const MAPGEN_WIDTH: usize = 24;
pub const MAPGEN_HEIGHT: usize = 24;
pub const DEFAULT_MAPGEN_WEIGHT: u32 = 1_000;
pub const MAX_MAPGEN_WEIGHT: u32 = 1_000_000;
pub const MAX_MAPGEN_ROOTS: usize = 8_192;
pub const MAX_NAMED_PALETTES: usize = 4_096;
pub const MAX_MAPGEN_OM_TERRAINS: usize = 64;
pub const MAX_MAPGEN_VARIANTS: usize = 256;
pub const MAX_MAPGEN_BINDINGS: usize = 1_024;
pub const MAX_MAPGEN_CHOICE_ENTRIES: usize = 256;
pub const MAX_MAPGEN_CHOICE_WEIGHT: u32 = 1_000_000;
pub const MAX_MAPGEN_CHOICE_TOTAL_WEIGHT: u64 = 16_000_000;
pub const MAX_MAPGEN_PALETTE_DEPTH: usize = 32;
pub const MAX_MAPGEN_PALETTE_LAYERS: usize = 4_096;
pub const MAX_MAPGEN_OMT_ASSIGNMENTS: usize = 65_536;
pub const MAX_MAPGEN_REPORT_ASSIGNMENTS: usize = 65_536;
pub const MAX_NESTED_MAPGEN_DEFINITIONS: usize = 8_192;
pub const MAX_NESTED_MAPGEN_PLACEMENTS: usize = 1_024;
pub const MAX_NESTED_MAPGEN_DEPTH: usize = 32;

const MAX_DISCOVERED_OM_TERRAINS: usize = 1_024;
const ROOT_FIELDS: &[&str] = &["type", "om_terrain", "weight", "object"];
const OBJECT_FIELDS: &[&str] = &[
    "fill_ter",
    "rows",
    "terrain",
    "furniture",
    "items",
    "palettes",
    "rotation",
    "fallback_predecessor_mapgen",
    "place_nested",
    "place_items",
    "place_npcs",
    "place_monsters",
    "place_monster",
    "flags",
];
const NESTED_OBJECT_FIELDS: &[&str] = &[
    "mapgensize",
    "fill_ter",
    "rows",
    "terrain",
    "furniture",
    "items",
    "palettes",
    "rotation",
    "place_nested",
    "place_items",
    "place_npcs",
    "place_vehicles",
    "place_monsters",
    "place_monster",
    "flags",
];
const PALETTE_FIELDS: &[&str] = &[
    "type",
    "id",
    "terrain",
    "furniture",
    "items",
    "palettes",
    "signs",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedMapgenId {
    pub id: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenItemPlacement {
    pub item_group: String,
    pub chance: u8,
    pub repeat_minimum: u16,
    pub repeat_maximum: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapgenCoordinateRange {
    pub minimum: i8,
    pub maximum: i8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapgenU16Range {
    pub minimum: u16,
    pub maximum: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenMonsterPlacement {
    pub monster_group: String,
    pub chance: MapgenU16Range,
    /// Pinned spawn-density multiplier in millionths. The upstream default is
    /// the ordinary world spawn-density option, pinned to 1.0 for a world.
    pub density_millionths: u32,
    pub repeat: MapgenU16Range,
    pub x: MapgenCoordinateRange,
    pub y: MapgenCoordinateRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StrictMapgenIndividualMonsterTarget {
    Monster(String),
    Group(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenIndividualMonsterPlacement {
    pub target: StrictMapgenIndividualMonsterTarget,
    pub chance_percent: MapgenU16Range,
    pub pack_size: MapgenU16Range,
    pub repeat: MapgenU16Range,
    pub x: MapgenCoordinateRange,
    pub y: MapgenCoordinateRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenNpcPlacement {
    pub template_id: String,
    pub repeat: MapgenU16Range,
    pub x: MapgenCoordinateRange,
    pub y: MapgenCoordinateRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenAreaItemPlacement {
    pub item_group: String,
    pub chance: u8,
    pub x: MapgenCoordinateRange,
    pub y: MapgenCoordinateRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenChunkChoice {
    pub nested_id: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenOmtMatch {
    pub omt: String,
    pub match_type: OvermapTerrainMatchType,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenNeighborMatch {
    pub direction: String,
    pub alternatives: Vec<StrictMapgenOmtMatch>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenNeighborFlags {
    pub direction: String,
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrictMapgenNestedConditions {
    pub neighbors: Vec<StrictMapgenNeighborMatch>,
    pub flags: Vec<StrictMapgenNeighborFlags>,
    pub flags_any: Vec<StrictMapgenNeighborFlags>,
    pub predecessors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenNestedPlacement {
    pub chunks: Vec<StrictMapgenChunkChoice>,
    pub else_chunks: Vec<StrictMapgenChunkChoice>,
    pub x: MapgenCoordinateRange,
    pub y: MapgenCoordinateRange,
    pub conditions: StrictMapgenNestedConditions,
}

/// A single upstream placement choice. Each value in a binding's `Vec` is a
/// separate placement layer and therefore a separate random roll.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MapgenIdChoice {
    Fixed(String),
    Weighted(Vec<WeightedMapgenId>),
}

impl MapgenIdChoice {
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        let entries: &[WeightedMapgenId] = match self {
            Self::Fixed(id) => {
                return MapgenIdIter::Fixed(Some(id.as_str()));
            }
            Self::Weighted(entries) => entries,
        };
        MapgenIdIter::Weighted(entries.iter())
    }
}

enum MapgenIdIter<'a> {
    Fixed(Option<&'a str>),
    Weighted(std::slice::Iter<'a, WeightedMapgenId>),
}

impl<'a> Iterator for MapgenIdIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Fixed(id) => id.take(),
            Self::Weighted(entries) => entries.next().map(|entry| entry.id.as_str()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenDefinition {
    pub source: String,
    pub om_terrains: Vec<String>,
    pub weight: u32,
    pub cells: Vec<String>,
    pub fill_terrain: Option<MapgenIdChoice>,
    pub terrain: BTreeMap<String, Vec<MapgenIdChoice>>,
    pub furniture: BTreeMap<String, Vec<MapgenIdChoice>>,
    /// Item-phase placements, applied after terrain and furniture. Each entry
    /// is one static named-item-group placement for that glyph.
    pub items: BTreeMap<String, StrictMapgenItemPlacement>,
    /// Named palettes in deterministic reference-expansion order. Repeated
    /// references remain repeated because upstream applies them repeatedly.
    pub palette_closure: Vec<String>,
    /// The pinned fallback mapgen runs before this overlay. It is retained as
    /// an ordinary generator edge rather than being expanded during loading.
    pub fallback_predecessor_mapgen: Option<String>,
    pub nested: Vec<StrictMapgenNestedPlacement>,
    pub area_items: Vec<StrictMapgenAreaItemPlacement>,
    pub npc_placements: Vec<StrictMapgenNpcPlacement>,
    pub monster_placements: Vec<StrictMapgenMonsterPlacement>,
    pub individual_monster_placements: Vec<StrictMapgenIndividualMonsterPlacement>,
    pub erase_all_before_placing_terrain: bool,
    /// Side-effect phases deliberately owned by the later spawning family.
    pub deferred_fields: BTreeSet<String>,
}

impl StrictMapgenDefinition {
    #[must_use]
    pub fn cell(&self, x: usize, y: usize) -> Option<&str> {
        if x >= MAPGEN_WIDTH || y >= MAPGEN_HEIGHT {
            return None;
        }
        self.cells.get(y * MAPGEN_WIDTH + x).map(String::as_str)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictNestedMapgenDefinition {
    pub source: String,
    pub nested_id: String,
    pub weight: u32,
    pub width: u8,
    pub height: u8,
    pub cells: Vec<String>,
    pub fill_terrain: Option<MapgenIdChoice>,
    pub terrain: BTreeMap<String, Vec<MapgenIdChoice>>,
    pub furniture: BTreeMap<String, Vec<MapgenIdChoice>>,
    pub items: BTreeMap<String, StrictMapgenItemPlacement>,
    pub palette_closure: Vec<String>,
    pub nested: Vec<StrictMapgenNestedPlacement>,
    pub area_items: Vec<StrictMapgenAreaItemPlacement>,
    pub npc_placements: Vec<StrictMapgenNpcPlacement>,
    pub monster_placements: Vec<StrictMapgenMonsterPlacement>,
    pub individual_monster_placements: Vec<StrictMapgenIndividualMonsterPlacement>,
    pub erase_all_before_placing_terrain: bool,
    pub deferred_fields: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapgenRootReport {
    pub source: String,
    pub om_terrains: Vec<String>,
    pub rejection_reason: Option<String>,
}

impl MapgenRootReport {
    #[must_use]
    pub const fn is_available(&self) -> bool {
        self.rejection_reason.is_none()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MapgenRegistry {
    definitions: BTreeMap<String, Vec<Arc<StrictMapgenDefinition>>>,
    unavailable: BTreeMap<String, Vec<Arc<MapgenRootReport>>>,
    reports: Vec<Arc<MapgenRootReport>>,
    nested: BTreeMap<String, Vec<Arc<StrictNestedMapgenDefinition>>>,
    unavailable_nested: BTreeMap<String, Vec<Arc<MapgenRootReport>>>,
}

impl MapgenRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
        terrain: &TerrainRegistry,
        furniture: &FurnitureRegistry,
        item_groups: &ItemGroupRegistry,
    ) -> Result<Self, MapgenRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(MapgenRegistryError::Catalog)?;
        let (roots, nested, palettes) = read_mapgen(content_root.as_ref(), files)?;
        compile_registry(
            &roots,
            &nested,
            &palettes,
            &|id| terrain.get(id).is_some(),
            &|id| id == "f_null" || furniture.get(id).is_some(),
            &|id| item_groups.get(id).is_some(),
        )
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    #[must_use]
    pub fn unavailable_len(&self) -> usize {
        self.unavailable.len()
    }

    #[must_use]
    pub fn get(&self, om_terrain: &str) -> Option<&[Arc<StrictMapgenDefinition>]> {
        self.definitions.get(om_terrain).map(Vec::as_slice)
    }

    #[must_use]
    pub fn unavailable_reports(&self, om_terrain: &str) -> Option<&[Arc<MapgenRootReport>]> {
        self.unavailable.get(om_terrain).map(Vec::as_slice)
    }

    pub fn reports(&self) -> impl Iterator<Item = &MapgenRootReport> {
        self.reports.iter().map(Arc::as_ref)
    }

    #[must_use]
    pub fn nested(&self, id: &str) -> Option<&[Arc<StrictNestedMapgenDefinition>]> {
        self.nested.get(id).map(Vec::as_slice)
    }

    #[must_use]
    pub fn unavailable_nested_reports(&self, id: &str) -> Option<&[Arc<MapgenRootReport>]> {
        self.unavailable_nested.get(id).map(Vec::as_slice)
    }

    /// Returns every named nested generator reachable from all variants of an
    /// ordinary root. Missing definitions and cycles fail closed before a
    /// server can persist a partially executable mapgen family.
    pub fn strict_nested_closure(&self, root_id: &str) -> Result<BTreeSet<String>, String> {
        let roots = self
            .get(root_id)
            .ok_or_else(|| format!("ordinary mapgen {root_id:?} is unavailable"))?;
        let mut closure = BTreeSet::new();
        let mut active = Vec::new();
        for root in roots {
            self.visit_nested_placements(&root.nested, &mut closure, &mut active)?;
        }
        Ok(closure)
    }

    fn visit_nested_placements(
        &self,
        placements: &[StrictMapgenNestedPlacement],
        closure: &mut BTreeSet<String>,
        active: &mut Vec<String>,
    ) -> Result<(), String> {
        if active.len() >= MAX_NESTED_MAPGEN_DEPTH {
            return Err(format!(
                "nested mapgen depth exceeds {MAX_NESTED_MAPGEN_DEPTH} at {}",
                active.join(" -> ")
            ));
        }
        for id in placements
            .iter()
            .flat_map(|placement| placement.chunks.iter().chain(&placement.else_chunks))
            .map(|choice| choice.nested_id.as_str())
            .filter(|id| *id != "null")
        {
            if active.iter().any(|ancestor| ancestor == id) {
                let mut cycle = active.clone();
                cycle.push(id.to_owned());
                return Err(format!("nested mapgen cycle: {}", cycle.join(" -> ")));
            }
            if closure.contains(id) {
                continue;
            }
            let variants = self.nested(id).ok_or_else(|| {
                let reason = self
                    .unavailable_nested_reports(id)
                    .and_then(|reports| reports.first())
                    .and_then(|report| report.rejection_reason.as_deref())
                    .unwrap_or("definition is missing");
                format!("nested mapgen {id:?} is unavailable: {reason}")
            })?;
            active.push(id.to_owned());
            for variant in variants {
                self.visit_nested_placements(&variant.nested, closure, active)?;
            }
            active.pop();
            closure.insert(id.to_owned());
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum MapgenRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    LimitExceeded(&'static str, usize),
}

impl fmt::Display for MapgenRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "selected mapgen catalog failed: {error}"),
            Self::Io(path, error) => {
                write!(formatter, "failed to read mapgen file {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "failed to parse mapgen JSON {path}: {error}")
            }
            Self::LimitExceeded(kind, limit) => {
                write!(formatter, "selected mapgen {kind} exceeds limit {limit}")
            }
        }
    }
}

impl std::error::Error for MapgenRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Io(_, error) => Some(error),
            Self::Json(_, error) => Some(error),
            Self::LimitExceeded(_, _) => None,
        }
    }
}

#[derive(Clone)]
struct RawMapgenRoot {
    source: String,
    object: Map<String, Value>,
}

#[derive(Clone)]
struct RawNestedMapgen {
    source: String,
    nested_id: String,
    weight: u32,
    object: Map<String, Value>,
}

#[derive(Clone)]
struct RawPalette {
    source: String,
    object: Map<String, Value>,
}

type PaletteCatalog = BTreeMap<String, Vec<RawPalette>>;

fn read_mapgen(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<(Vec<RawMapgenRoot>, Vec<RawNestedMapgen>, PaletteCatalog), MapgenRegistryError> {
    let mut roots = Vec::new();
    let mut nested = Vec::new();
    let mut palettes: PaletteCatalog = BTreeMap::new();
    let mut palette_count = 0_usize;
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| MapgenRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| MapgenRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for (index, value) in values.into_iter().enumerate() {
                    collect_definition(
                        &file,
                        index,
                        value,
                        &mut roots,
                        &mut nested,
                        &mut palettes,
                        &mut palette_count,
                    )?;
                }
            }
            value => collect_definition(
                &file,
                0,
                value,
                &mut roots,
                &mut nested,
                &mut palettes,
                &mut palette_count,
            )?,
        }
    }
    Ok((roots, nested, palettes))
}

fn collect_definition(
    file: &SelectedContentFile,
    index: usize,
    value: Value,
    roots: &mut Vec<RawMapgenRoot>,
    nested: &mut Vec<RawNestedMapgen>,
    palettes: &mut PaletteCatalog,
    palette_count: &mut usize,
) -> Result<(), MapgenRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    match object.get("type").and_then(Value::as_str) {
        Some("mapgen") if object.contains_key("om_terrain") => {
            if roots.len() >= MAX_MAPGEN_ROOTS {
                return Err(MapgenRegistryError::LimitExceeded(
                    "ordinary roots",
                    MAX_MAPGEN_ROOTS,
                ));
            }
            roots.push(RawMapgenRoot {
                source: format!("{}#{index}", file.upstream_path),
                object: object.clone(),
            });
        }
        Some("mapgen") if object.contains_key("nested_mapgen_id") => {
            if nested.len() >= MAX_NESTED_MAPGEN_DEFINITIONS {
                return Err(MapgenRegistryError::LimitExceeded(
                    "nested mapgen definitions",
                    MAX_NESTED_MAPGEN_DEFINITIONS,
                ));
            }
            let nested_id = object
                .get("nested_mapgen_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    MapgenRegistryError::Json(
                        file.destination.clone(),
                        serde_json::Error::io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "nested_mapgen_id must be a non-empty string",
                        )),
                    )
                })?;
            let weight = parse_nested_weight(object.get("weight")).map_err(|reason| {
                MapgenRegistryError::Json(
                    file.destination.clone(),
                    serde_json::Error::io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        reason,
                    )),
                )
            })?;
            let nested_object = object
                .get("object")
                .and_then(Value::as_object)
                .cloned()
                .ok_or_else(|| {
                    MapgenRegistryError::Json(
                        file.destination.clone(),
                        serde_json::Error::io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "nested mapgen object must be an object",
                        )),
                    )
                })?;
            nested.push(RawNestedMapgen {
                source: format!("{}#{index}", file.upstream_path),
                nested_id: nested_id.to_owned(),
                weight,
                object: nested_object,
            });
        }
        Some("palette") => {
            if *palette_count >= MAX_NAMED_PALETTES {
                return Err(MapgenRegistryError::LimitExceeded(
                    "named palettes",
                    MAX_NAMED_PALETTES,
                ));
            }
            *palette_count += 1;
            if let Some(id) = object
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                palettes.entry(id.to_owned()).or_default().push(RawPalette {
                    source: format!("{}#{index}", file.upstream_path),
                    object: object.clone(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn compile_registry<TerrainExists, FurnitureExists, ItemGroupExists>(
    roots: &[RawMapgenRoot],
    raw_nested: &[RawNestedMapgen],
    palettes: &PaletteCatalog,
    terrain_exists: &TerrainExists,
    furniture_exists: &FurnitureExists,
    item_group_exists: &ItemGroupExists,
) -> Result<MapgenRegistry, MapgenRegistryError>
where
    TerrainExists: Fn(&str) -> bool,
    FurnitureExists: Fn(&str) -> bool,
    ItemGroupExists: Fn(&str) -> bool,
{
    let mut definitions: BTreeMap<String, Vec<Arc<StrictMapgenDefinition>>> = BTreeMap::new();
    let mut unavailable: BTreeMap<String, Vec<Arc<MapgenRootReport>>> = BTreeMap::new();
    let mut reports = Vec::with_capacity(roots.len());
    let mut definition_assignments = 0_usize;
    let mut report_assignments = 0_usize;
    for root in roots {
        let discovered = discover_om_terrains(root.object.get("om_terrain"))?;
        match compile_root(
            root,
            palettes,
            terrain_exists,
            furniture_exists,
            item_group_exists,
        ) {
            Ok(definition) => {
                definition_assignments = definition_assignments
                    .checked_add(definition.om_terrains.len())
                    .ok_or(MapgenRegistryError::LimitExceeded(
                        "OMT assignments",
                        MAX_MAPGEN_OMT_ASSIGNMENTS,
                    ))?;
                if definition_assignments > MAX_MAPGEN_OMT_ASSIGNMENTS {
                    return Err(MapgenRegistryError::LimitExceeded(
                        "OMT assignments",
                        MAX_MAPGEN_OMT_ASSIGNMENTS,
                    ));
                }
                let definition = Arc::new(definition);
                for id in &definition.om_terrains {
                    let variants = definitions.entry(id.clone()).or_default();
                    if variants.len() >= MAX_MAPGEN_VARIANTS {
                        return Err(MapgenRegistryError::LimitExceeded(
                            "variants for one overmap terrain",
                            MAX_MAPGEN_VARIANTS,
                        ));
                    }
                    variants.push(Arc::clone(&definition));
                }
                reports.push(Arc::new(MapgenRootReport {
                    source: root.source.clone(),
                    om_terrains: definition.om_terrains.clone(),
                    rejection_reason: None,
                }));
            }
            Err(reason) => {
                report_assignments = report_assignments.checked_add(discovered.len()).ok_or(
                    MapgenRegistryError::LimitExceeded(
                        "unavailable report assignments",
                        MAX_MAPGEN_REPORT_ASSIGNMENTS,
                    ),
                )?;
                if report_assignments > MAX_MAPGEN_REPORT_ASSIGNMENTS {
                    return Err(MapgenRegistryError::LimitExceeded(
                        "unavailable report assignments",
                        MAX_MAPGEN_REPORT_ASSIGNMENTS,
                    ));
                }
                let report = Arc::new(MapgenRootReport {
                    source: root.source.clone(),
                    om_terrains: discovered.clone(),
                    rejection_reason: Some(reason),
                });
                for id in discovered {
                    unavailable.entry(id).or_default().push(Arc::clone(&report));
                }
                reports.push(report);
            }
        }
    }
    for id in unavailable.keys() {
        definitions.remove(id);
    }
    let mut nested: BTreeMap<String, Vec<Arc<StrictNestedMapgenDefinition>>> = BTreeMap::new();
    let mut unavailable_nested: BTreeMap<String, Vec<Arc<MapgenRootReport>>> = BTreeMap::new();
    for raw in raw_nested {
        if raw.weight == 0 {
            continue;
        }
        match compile_nested_definition(
            raw,
            palettes,
            terrain_exists,
            furniture_exists,
            item_group_exists,
        ) {
            Ok(definition) => {
                let variants = nested.entry(raw.nested_id.clone()).or_default();
                if variants.len() >= MAX_MAPGEN_VARIANTS {
                    return Err(MapgenRegistryError::LimitExceeded(
                        "variants for one nested mapgen",
                        MAX_MAPGEN_VARIANTS,
                    ));
                }
                variants.push(Arc::new(definition));
            }
            Err(reason) => {
                unavailable_nested
                    .entry(raw.nested_id.clone())
                    .or_default()
                    .push(Arc::new(MapgenRootReport {
                        source: raw.source.clone(),
                        om_terrains: vec![raw.nested_id.clone()],
                        rejection_reason: Some(reason),
                    }));
            }
        }
    }
    for id in unavailable_nested.keys() {
        nested.remove(id);
    }
    Ok(MapgenRegistry {
        definitions,
        unavailable,
        reports,
        nested,
        unavailable_nested,
    })
}

fn discover_om_terrains(value: Option<&Value>) -> Result<Vec<String>, MapgenRegistryError> {
    fn visit(value: &Value, ids: &mut BTreeSet<String>) -> Result<(), MapgenRegistryError> {
        match value {
            Value::String(id) if !id.is_empty() => {
                ids.insert(id.clone());
                if ids.len() > MAX_DISCOVERED_OM_TERRAINS {
                    return Err(MapgenRegistryError::LimitExceeded(
                        "identities in one root",
                        MAX_DISCOVERED_OM_TERRAINS,
                    ));
                }
            }
            Value::Array(values) => {
                for value in values {
                    visit(value, ids)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    let mut ids = BTreeSet::new();
    if let Some(value) = value {
        visit(value, &mut ids)?;
    }
    Ok(ids.into_iter().collect())
}

fn compile_root<TerrainExists, FurnitureExists, ItemGroupExists>(
    raw: &RawMapgenRoot,
    palettes: &PaletteCatalog,
    terrain_exists: &TerrainExists,
    furniture_exists: &FurnitureExists,
    item_group_exists: &ItemGroupExists,
) -> Result<StrictMapgenDefinition, String>
where
    TerrainExists: Fn(&str) -> bool,
    FurnitureExists: Fn(&str) -> bool,
    ItemGroupExists: Fn(&str) -> bool,
{
    reject_unknown_fields(&raw.object, ROOT_FIELDS, "mapgen root")?;
    let om_terrains = parse_om_terrains(raw.object.get("om_terrain"))?;
    let weight = parse_root_weight(raw.object.get("weight"))?;
    let object = raw
        .object
        .get("object")
        .and_then(Value::as_object)
        .ok_or_else(|| String::from("object must be an object"))?;
    reject_unknown_fields(object, OBJECT_FIELDS, "mapgen object")?;

    let fill_terrain = object
        .get("fill_ter")
        .map(|value| parse_choice(value, "fill_ter", terrain_exists))
        .transpose()?;
    let cells = parse_rows(object.get("rows"))?;
    let mut state = CompileState::default();
    let palette_ids = parse_palette_ids(object.get("palettes"), "mapgen object")?;
    let mut ancestors = Vec::new();
    for id in palette_ids {
        append_palette(
            id,
            palettes,
            terrain_exists,
            furniture_exists,
            item_group_exists,
            &mut ancestors,
            &mut state,
        )?;
    }
    append_local_bindings(
        object,
        "mapgen object",
        terrain_exists,
        furniture_exists,
        item_group_exists,
        &mut state,
    )?;
    let fallback_predecessor_mapgen = parse_optional_id(
        object.get("fallback_predecessor_mapgen"),
        "fallback_predecessor_mapgen",
    )?;
    let nested = parse_nested_placements(object.get("place_nested"), "mapgen object")?;
    let area_items = parse_area_items(
        object.get("place_items"),
        "mapgen object",
        item_group_exists,
    )?;
    let npc_placements = parse_npc_placements(object.get("place_npcs"), "mapgen object")?;
    let monster_placements =
        parse_monster_placements(object.get("place_monsters"), "mapgen object")?;
    let individual_monster_placements =
        parse_individual_monster_placements(object.get("place_monster"), "mapgen object")?;
    let erase_all_before_placing_terrain = parse_erase_all_flag(object.get("flags"))?;
    if fill_terrain.is_none() {
        if object.get("rows").is_none()
            && fallback_predecessor_mapgen.is_none()
            && nested.is_empty()
        {
            return Err(String::from("mapgen without rows requires fill_ter"));
        }
        for cell in &cells {
            if !state.terrain.contains_key(cell)
                && fallback_predecessor_mapgen.is_none()
                && nested.is_empty()
            {
                return Err(format!(
                    "row cell {cell:?} has no terrain binding and fill_ter is absent"
                ));
            }
        }
    }

    Ok(StrictMapgenDefinition {
        source: raw.source.clone(),
        om_terrains,
        weight,
        cells,
        fill_terrain,
        terrain: state.terrain,
        furniture: state.furniture,
        items: state.items,
        palette_closure: state.palette_closure,
        fallback_predecessor_mapgen,
        nested,
        area_items,
        npc_placements,
        monster_placements,
        individual_monster_placements,
        erase_all_before_placing_terrain,
        deferred_fields: state.deferred_fields,
    })
}

fn compile_nested_definition<TerrainExists, FurnitureExists, ItemGroupExists>(
    raw: &RawNestedMapgen,
    palettes: &PaletteCatalog,
    terrain_exists: &TerrainExists,
    furniture_exists: &FurnitureExists,
    item_group_exists: &ItemGroupExists,
) -> Result<StrictNestedMapgenDefinition, String>
where
    TerrainExists: Fn(&str) -> bool,
    FurnitureExists: Fn(&str) -> bool,
    ItemGroupExists: Fn(&str) -> bool,
{
    reject_unknown_fields(
        &raw.object,
        NESTED_OBJECT_FIELDS,
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    let (width, height) = parse_mapgen_size(
        raw.object.get("mapgensize"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    parse_nested_rotation(
        raw.object.get("rotation"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    let fill_terrain = raw
        .object
        .get("fill_ter")
        .map(|value| parse_choice(value, "nested fill_ter", terrain_exists))
        .transpose()?;
    let cells = parse_rows_sized(
        raw.object.get("rows"),
        usize::from(width),
        usize::from(height),
    )?;
    let mut state = CompileState::default();
    let palette_ids = parse_palette_ids(
        raw.object.get("palettes"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    let mut ancestors = Vec::new();
    for id in palette_ids {
        append_palette(
            id,
            palettes,
            terrain_exists,
            furniture_exists,
            item_group_exists,
            &mut ancestors,
            &mut state,
        )?;
    }
    append_local_bindings(
        &raw.object,
        &format!("nested mapgen {:?}", raw.nested_id),
        terrain_exists,
        furniture_exists,
        item_group_exists,
        &mut state,
    )?;
    let nested = parse_nested_placements(
        raw.object.get("place_nested"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    let area_items = parse_area_items(
        raw.object.get("place_items"),
        &format!("nested mapgen {:?}", raw.nested_id),
        item_group_exists,
    )?;
    let npc_placements = parse_npc_placements(
        raw.object.get("place_npcs"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    let monster_placements = parse_monster_placements(
        raw.object.get("place_monsters"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    let individual_monster_placements = parse_individual_monster_placements(
        raw.object.get("place_monster"),
        &format!("nested mapgen {:?}", raw.nested_id),
    )?;
    for field in ["place_vehicles"] {
        if raw
            .object
            .get(field)
            .is_some_and(|value| !value.as_array().is_some_and(Vec::is_empty) && !value.is_null())
        {
            state.deferred_fields.insert(field.to_owned());
        }
    }
    Ok(StrictNestedMapgenDefinition {
        source: raw.source.clone(),
        nested_id: raw.nested_id.clone(),
        weight: raw.weight,
        width,
        height,
        cells,
        fill_terrain,
        terrain: state.terrain,
        furniture: state.furniture,
        items: state.items,
        palette_closure: state.palette_closure,
        nested,
        area_items,
        npc_placements,
        monster_placements,
        individual_monster_placements,
        erase_all_before_placing_terrain: parse_erase_all_flag(raw.object.get("flags"))?,
        deferred_fields: state.deferred_fields,
    })
}

fn parse_optional_id(value: Option<&Value>, context: &str) -> Result<Option<String>, String> {
    value
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 512)
                .map(str::to_owned)
                .ok_or_else(|| format!("{context} must be a non-empty bounded id"))
        })
        .transpose()
}

fn parse_mapgen_size(value: Option<&Value>, context: &str) -> Result<(u8, u8), String> {
    let values = value
        .and_then(Value::as_array)
        .filter(|values| values.len() == 2)
        .ok_or_else(|| format!("mapgensize in {context} must contain width and height"))?;
    let width = values[0]
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| (1..=MAPGEN_WIDTH as u8).contains(value))
        .ok_or_else(|| format!("mapgensize width in {context} must be 1..={MAPGEN_WIDTH}"))?;
    let height = values[1]
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| (1..=MAPGEN_HEIGHT as u8).contains(value))
        .ok_or_else(|| format!("mapgensize height in {context} must be 1..={MAPGEN_HEIGHT}"))?;
    Ok((width, height))
}

fn parse_nested_rotation(value: Option<&Value>, context: &str) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let values = value
        .as_array()
        .filter(|values| matches!(values.len(), 1 | 2))
        .ok_or_else(|| format!("rotation in {context} must contain one or two quarter turns"))?;
    let rotations = values
        .iter()
        .map(|value| {
            value
                .as_u64()
                .filter(|rotation| *rotation <= 3)
                .ok_or_else(|| format!("rotation in {context} must be between 0 and 3"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if rotations.len() == 2 && rotations[0] > rotations[1] {
        return Err(format!("rotation range in {context} is reversed"));
    }
    Ok(())
}

fn parse_erase_all_flag(value: Option<&Value>) -> Result<bool, String> {
    let Some(value) = value else {
        return Ok(false);
    };
    let flags = value
        .as_array()
        .ok_or_else(|| String::from("mapgen flags must be an array"))?;
    let mut erase_all = false;
    for value in flags {
        match value.as_str() {
            Some("ERASE_ALL_BEFORE_PLACING_TERRAIN") => erase_all = true,
            Some(flag) => return Err(format!("unsupported mapgen flag {flag:?}")),
            None => return Err(String::from("mapgen flags must be strings")),
        }
    }
    Ok(erase_all)
}

fn parse_area_items<Exists>(
    value: Option<&Value>,
    context: &str,
    exists: &Exists,
) -> Result<Vec<StrictMapgenAreaItemPlacement>, String>
where
    Exists: Fn(&str) -> bool,
{
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let placements = value
        .as_array()
        .ok_or_else(|| format!("place_items in {context} must be an array"))?;
    if placements.len() > MAX_NESTED_MAPGEN_PLACEMENTS {
        return Err(format!(
            "place_items in {context} exceeds {MAX_NESTED_MAPGEN_PLACEMENTS} placements"
        ));
    }
    placements
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let object = value
                .as_object()
                .ok_or_else(|| format!("place_items[{index}] in {context} must be an object"))?;
            reject_unknown_fields(
                object,
                &["item", "chance", "x", "y"],
                &format!("place_items[{index}] in {context}"),
            )?;
            let item_group = object
                .get("item")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && exists(id))
                .ok_or_else(|| {
                    format!("place_items[{index}] in {context} references an unknown item group")
                })?;
            let chance = object
                .get("chance")
                .and_then(Value::as_u64)
                .and_then(|value| u8::try_from(value).ok())
                .filter(|chance| (1..=100).contains(chance))
                .ok_or_else(|| {
                    format!("place_items[{index}] chance in {context} must be 1..=100")
                })?;
            Ok(StrictMapgenAreaItemPlacement {
                item_group: item_group.to_owned(),
                chance,
                x: parse_coordinate_range(object.get("x"), "x", context)?,
                y: parse_coordinate_range(object.get("y"), "y", context)?,
            })
        })
        .collect()
}

fn parse_npc_placements(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<StrictMapgenNpcPlacement>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let placements = value
        .as_array()
        .ok_or_else(|| format!("place_npcs in {context} must be an array"))?;
    if placements.len() > MAX_NESTED_MAPGEN_PLACEMENTS {
        return Err(format!(
            "place_npcs in {context} exceeds {MAX_NESTED_MAPGEN_PLACEMENTS} placements"
        ));
    }
    placements
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let placement_context = format!("place_npcs[{index}] in {context}");
            let object = value
                .as_object()
                .ok_or_else(|| format!("{placement_context} must be an object"))?;
            reject_unknown_fields(
                object,
                &[
                    "class",
                    "x",
                    "y",
                    "z",
                    "repeat",
                    "target",
                    "add_trait",
                    "unique_id",
                ],
                &placement_context,
            )?;
            if object
                .get("target")
                .is_some_and(|value| value.as_bool() != Some(false))
            {
                return Err(format!(
                    "mission-target NPC semantics are unsupported in {placement_context}"
                ));
            }
            if object
                .get("add_trait")
                .is_some_and(|value| !value.as_array().is_some_and(|traits| traits.is_empty()))
            {
                return Err(format!(
                    "NPC trait mutation is unsupported in {placement_context}"
                ));
            }
            if object
                .get("unique_id")
                .is_some_and(|value| value.as_str() != Some(""))
            {
                return Err(format!(
                    "unique NPC identity is unsupported in {placement_context}"
                ));
            }
            if let Some(z) = object.get("z") {
                let z = parse_coordinate_range(Some(z), "z", &placement_context)?;
                if z.minimum != 0 || z.maximum != 0 {
                    return Err(format!(
                        "nonzero NPC z placement is unsupported in {placement_context}"
                    ));
                }
            }
            let template_id = object
                .get("class")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 512)
                .ok_or_else(|| {
                    format!("class in {placement_context} must be a fixed bounded NPC template id")
                })?
                .to_owned();
            Ok(StrictMapgenNpcPlacement {
                template_id,
                repeat: parse_u16_range(
                    object.get("repeat"),
                    1,
                    0,
                    MAX_NESTED_MAPGEN_PLACEMENTS as u16,
                    "repeat",
                    &placement_context,
                )?,
                x: parse_coordinate_range(object.get("x"), "x", &placement_context)?,
                y: parse_coordinate_range(object.get("y"), "y", &placement_context)?,
            })
        })
        .collect()
}

fn parse_monster_placements(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<StrictMapgenMonsterPlacement>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let placements = value
        .as_array()
        .ok_or_else(|| format!("place_monsters in {context} must be an array"))?;
    if placements.len() > MAX_NESTED_MAPGEN_PLACEMENTS {
        return Err(format!(
            "place_monsters in {context} exceeds {MAX_NESTED_MAPGEN_PLACEMENTS} placements"
        ));
    }
    placements
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let placement_context = format!("place_monsters[{index}] in {context}");
            let object = value
                .as_object()
                .ok_or_else(|| format!("{placement_context} must be an object"))?;
            reject_unknown_fields(
                object,
                &["monster", "chance", "density", "repeat", "x", "y"],
                &placement_context,
            )?;
            let monster_group = object
                .get("monster")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 512)
                .ok_or_else(|| {
                    format!("monster in {placement_context} must be a bounded group id")
                })?
                .to_owned();
            Ok(StrictMapgenMonsterPlacement {
                monster_group,
                chance: parse_u16_range(
                    object.get("chance"),
                    1,
                    1,
                    u16::MAX,
                    "chance",
                    &placement_context,
                )?,
                density_millionths: parse_density_millionths(
                    object.get("density"),
                    &placement_context,
                )?,
                repeat: parse_u16_range(
                    object.get("repeat"),
                    1,
                    1,
                    MAX_NESTED_MAPGEN_PLACEMENTS as u16,
                    "repeat",
                    &placement_context,
                )?,
                x: parse_coordinate_range(object.get("x"), "x", &placement_context)?,
                y: parse_coordinate_range(object.get("y"), "y", &placement_context)?,
            })
        })
        .collect()
}

fn parse_individual_monster_placements(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<StrictMapgenIndividualMonsterPlacement>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let placements = value
        .as_array()
        .ok_or_else(|| format!("place_monster in {context} must be an array"))?;
    if placements.len() > MAX_NESTED_MAPGEN_PLACEMENTS {
        return Err(format!(
            "place_monster in {context} exceeds {MAX_NESTED_MAPGEN_PLACEMENTS} placements"
        ));
    }
    placements
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let placement_context = format!("place_monster[{index}] in {context}");
            let object = value
                .as_object()
                .ok_or_else(|| format!("{placement_context} must be an object"))?;
            reject_unknown_fields(
                object,
                &[
                    "monster",
                    "group",
                    "chance",
                    "pack_size",
                    "repeat",
                    "x",
                    "y",
                    "one_or_none",
                    "friendly",
                    "target",
                    "use_pack_size",
                    "name",
                    "random_name",
                    "spawn_data",
                ],
                &placement_context,
            )?;
            for field in ["friendly", "target", "use_pack_size"] {
                if object
                    .get(field)
                    .is_some_and(|value| value.as_bool() != Some(false))
                {
                    return Err(format!("unsupported {field} semantics in {placement_context}"));
                }
            }
            if object.get("one_or_none").is_some_and(|value| !value.is_boolean()) {
                return Err(format!("one_or_none in {placement_context} must be boolean"));
            }
            if object.contains_key("name")
                || object
                    .get("random_name")
                    .is_some_and(|value| value.as_str().is_none_or(|name| !name.is_empty()))
                || object.contains_key("spawn_data")
            {
                return Err(format!(
                    "named, mission-targeted, friendly, or spawn-data monster semantics are unsupported in {placement_context}"
                ));
            }
            let target = match (
                object.get("monster").and_then(Value::as_str),
                object.get("group").and_then(Value::as_str),
            ) {
                (Some(id), None) if !id.is_empty() && id.len() <= 512 => {
                    StrictMapgenIndividualMonsterTarget::Monster(id.to_owned())
                }
                (None, Some(id)) if !id.is_empty() && id.len() <= 512 => {
                    StrictMapgenIndividualMonsterTarget::Group(id.to_owned())
                }
                _ => {
                    return Err(format!(
                        "{placement_context} must contain exactly one fixed monster or group id"
                    ));
                }
            };
            Ok(StrictMapgenIndividualMonsterPlacement {
                target,
                chance_percent: parse_u16_range(
                    object.get("chance"),
                    100,
                    1,
                    100,
                    "chance",
                    &placement_context,
                )?,
                pack_size: parse_u16_range(
                    object.get("pack_size"),
                    1,
                    1,
                    1_024,
                    "pack_size",
                    &placement_context,
                )?,
                repeat: parse_u16_range(
                    object.get("repeat"),
                    1,
                    1,
                    MAX_NESTED_MAPGEN_PLACEMENTS as u16,
                    "repeat",
                    &placement_context,
                )?,
                x: parse_coordinate_range(object.get("x"), "x", &placement_context)?,
                y: parse_coordinate_range(object.get("y"), "y", &placement_context)?,
            })
        })
        .collect()
}

fn parse_u16_range(
    value: Option<&Value>,
    default: u16,
    allowed_minimum: u16,
    allowed_maximum: u16,
    field: &str,
    context: &str,
) -> Result<MapgenU16Range, String> {
    let Some(value) = value else {
        return Ok(MapgenU16Range {
            minimum: default,
            maximum: default,
        });
    };
    let (minimum, maximum) = if let Some(value) = value.as_u64() {
        (value, value)
    } else {
        let values = value
            .as_array()
            .filter(|values| values.len() == 2)
            .ok_or_else(|| format!("{field} in {context} must be an integer or interval"))?;
        (
            values[0]
                .as_u64()
                .ok_or_else(|| format!("{field} minimum in {context} must be an integer"))?,
            values[1]
                .as_u64()
                .ok_or_else(|| format!("{field} maximum in {context} must be an integer"))?,
        )
    };
    let minimum = u16::try_from(minimum)
        .ok()
        .filter(|value| (allowed_minimum..=allowed_maximum).contains(value))
        .ok_or_else(|| format!("{field} minimum in {context} is outside its runtime bound"))?;
    let maximum = u16::try_from(maximum)
        .ok()
        .filter(|value| *value >= minimum && *value <= allowed_maximum)
        .ok_or_else(|| format!("{field} maximum in {context} is outside its runtime bound"))?;
    Ok(MapgenU16Range { minimum, maximum })
}

fn parse_density_millionths(value: Option<&Value>, context: &str) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(1_000_000);
    };
    let density = value
        .as_f64()
        .filter(|density| density.is_finite() && (0.0..=81.92).contains(density))
        .ok_or_else(|| {
            format!("density in {context} must be a finite number from 0 through 81.92")
        })?;
    let scaled = (density * 1_000_000.0).round();
    if (scaled / 1_000_000.0 - density).abs() > 0.000_000_5 {
        return Err(format!(
            "density in {context} has more precision than the canonical millionth scale"
        ));
    }
    u32::try_from(scaled as u64)
        .map_err(|_| format!("density in {context} exceeds the canonical runtime bound"))
}

fn parse_nested_placements(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<StrictMapgenNestedPlacement>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let placements = value
        .as_array()
        .ok_or_else(|| format!("place_nested in {context} must be an array"))?;
    if placements.len() > MAX_NESTED_MAPGEN_PLACEMENTS {
        return Err(format!(
            "place_nested in {context} exceeds {MAX_NESTED_MAPGEN_PLACEMENTS} placements"
        ));
    }
    placements
        .iter()
        .enumerate()
        .map(|(index, value)| parse_nested_placement(value, index, context))
        .collect()
}

fn parse_nested_placement(
    value: &Value,
    index: usize,
    context: &str,
) -> Result<StrictMapgenNestedPlacement, String> {
    let placement_context = format!("place_nested[{index}] in {context}");
    let object = value
        .as_object()
        .ok_or_else(|| format!("{placement_context} must be an object"))?;
    reject_unknown_fields(
        object,
        &[
            "chunks",
            "else_chunks",
            "x",
            "y",
            "neighbors",
            "flags",
            "flags_any",
            "predecessors",
        ],
        &placement_context,
    )?;
    let chunks = parse_chunk_choices(object.get("chunks"), &placement_context)?;
    let else_chunks = parse_chunk_choices(object.get("else_chunks"), &placement_context)?;
    if chunks.is_empty() && else_chunks.is_empty() {
        return Err(format!("{placement_context} has no chunks or else_chunks"));
    }
    Ok(StrictMapgenNestedPlacement {
        chunks,
        else_chunks,
        x: parse_coordinate_range(object.get("x"), "x", &placement_context)?,
        y: parse_coordinate_range(object.get("y"), "y", &placement_context)?,
        conditions: StrictMapgenNestedConditions {
            neighbors: parse_neighbor_matches(object.get("neighbors"), &placement_context)?,
            flags: parse_neighbor_flags(object.get("flags"), "flags", &placement_context)?,
            flags_any: parse_neighbor_flags(
                object.get("flags_any"),
                "flags_any",
                &placement_context,
            )?,
            predecessors: parse_string_array(object.get("predecessors"), "predecessors")?,
        },
    })
}

fn parse_chunk_choices(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<StrictMapgenChunkChoice>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|values| !values.is_empty() && values.len() <= MAX_MAPGEN_CHOICE_ENTRIES)
        .ok_or_else(|| {
            format!("chunks in {context} must contain 1..={MAX_MAPGEN_CHOICE_ENTRIES} entries")
        })?;
    let mut total = 0_u64;
    values
        .iter()
        .map(|value| {
            let (id, weight) = if let Some(id) = value.as_str() {
                (id, 1_u32)
            } else {
                let pair = value
                    .as_array()
                    .filter(|pair| pair.len() == 2)
                    .ok_or_else(|| format!("invalid weighted chunk in {context}"))?;
                let id = pair[0]
                    .as_str()
                    .ok_or_else(|| format!("chunk id in {context} must be a string"))?;
                let weight = pair[1]
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|weight| (1..=MAX_MAPGEN_CHOICE_WEIGHT).contains(weight))
                    .ok_or_else(|| format!("chunk weight in {context} is out of bounds"))?;
                (id, weight)
            };
            if id.is_empty() || id.len() > 512 {
                return Err(format!("chunk id in {context} is empty or too long"));
            }
            total = total
                .checked_add(u64::from(weight))
                .ok_or_else(|| format!("chunk weights in {context} overflow"))?;
            if total > MAX_MAPGEN_CHOICE_TOTAL_WEIGHT {
                return Err(format!("chunk weights in {context} exceed the total bound"));
            }
            Ok(StrictMapgenChunkChoice {
                nested_id: id.to_owned(),
                weight,
            })
        })
        .collect()
}

fn parse_coordinate_range(
    value: Option<&Value>,
    field: &str,
    context: &str,
) -> Result<MapgenCoordinateRange, String> {
    let value = value.ok_or_else(|| format!("{field} is required in {context}"))?;
    let (minimum, maximum) = if let Some(value) = value.as_i64() {
        (value, value)
    } else {
        let values = value
            .as_array()
            .filter(|values| values.len() == 2)
            .ok_or_else(|| format!("{field} in {context} must be an integer or interval"))?;
        (
            values[0]
                .as_i64()
                .ok_or_else(|| format!("{field} minimum in {context} must be an integer"))?,
            values[1]
                .as_i64()
                .ok_or_else(|| format!("{field} maximum in {context} must be an integer"))?,
        )
    };
    let minimum = i8::try_from(minimum)
        .ok()
        .filter(|value| i32::from(*value).abs() < MAPGEN_WIDTH as i32)
        .ok_or_else(|| format!("{field} minimum in {context} is outside one OMT offset"))?;
    let maximum = i8::try_from(maximum)
        .ok()
        .filter(|value| i32::from(*value).abs() < MAPGEN_WIDTH as i32 && *value >= minimum)
        .ok_or_else(|| format!("{field} maximum in {context} is invalid"))?;
    Ok(MapgenCoordinateRange { minimum, maximum })
}

fn parse_neighbor_matches(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<StrictMapgenNeighborMatch>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let directions = value
        .as_object()
        .ok_or_else(|| format!("neighbors in {context} must be an object"))?;
    directions
        .iter()
        .map(|(direction, value)| {
            validate_direction(direction, context)?;
            let alternatives = value
                .as_array()
                .filter(|values| !values.is_empty() && values.len() <= MAX_MAPGEN_CHOICE_ENTRIES)
                .ok_or_else(|| format!("neighbor {direction} in {context} must be an array"))?
                .iter()
                .map(|value| {
                    let object = value.as_object().ok_or_else(|| {
                        format!("neighbor alternative {direction} in {context} must be an object")
                    })?;
                    reject_unknown_fields(
                        object,
                        &["om_terrain", "om_terrain_match_type"],
                        &format!("neighbor {direction} in {context}"),
                    )?;
                    let omt = object
                        .get("om_terrain")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty() && id.len() <= 512)
                        .ok_or_else(|| format!("neighbor {direction} has invalid om_terrain"))?;
                    let match_type = match object
                        .get("om_terrain_match_type")
                        .and_then(Value::as_str)
                    {
                        Some("EXACT") => OvermapTerrainMatchType::Exact,
                        Some("TYPE") => OvermapTerrainMatchType::Type,
                        Some("SUBTYPE") => OvermapTerrainMatchType::Subtype,
                        Some("PREFIX") => OvermapTerrainMatchType::Prefix,
                        Some("CONTAINS") => OvermapTerrainMatchType::Contains,
                        _ => return Err(format!("neighbor {direction} has invalid match type")),
                    };
                    Ok(StrictMapgenOmtMatch {
                        omt: omt.to_owned(),
                        match_type,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(StrictMapgenNeighborMatch {
                direction: direction.clone(),
                alternatives,
            })
        })
        .collect()
}

fn parse_neighbor_flags(
    value: Option<&Value>,
    field: &str,
    context: &str,
) -> Result<Vec<StrictMapgenNeighborFlags>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let directions = value
        .as_object()
        .ok_or_else(|| format!("{field} in {context} must be an object"))?;
    directions
        .iter()
        .map(|(direction, value)| {
            validate_direction(direction, context)?;
            let flags = parse_string_array(Some(value), field)?;
            if flags.is_empty() {
                return Err(format!("{field} {direction} in {context} is empty"));
            }
            Ok(StrictMapgenNeighborFlags {
                direction: direction.clone(),
                flags,
            })
        })
        .collect()
}

fn parse_string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|values| values.len() <= MAX_MAPGEN_CHOICE_ENTRIES)
        .ok_or_else(|| format!("{field} must be a bounded string array"))?;
    let mut unique = BTreeSet::new();
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        let entry = value
            .as_str()
            .filter(|entry| !entry.is_empty() && entry.len() <= 512)
            .ok_or_else(|| format!("{field} entries must be non-empty bounded strings"))?;
        if !unique.insert(entry) {
            return Err(format!("{field} contains duplicate {entry:?}"));
        }
        result.push(entry.to_owned());
    }
    Ok(result)
}

fn validate_direction(direction: &str, context: &str) -> Result<(), String> {
    const DIRECTIONS: [&str; 8] = [
        "north",
        "north_east",
        "east",
        "south_east",
        "south",
        "south_west",
        "west",
        "north_west",
    ];
    DIRECTIONS
        .contains(&direction)
        .then_some(())
        .ok_or_else(|| format!("invalid neighbor direction {direction:?} in {context}"))
}

fn parse_om_terrains(value: Option<&Value>) -> Result<Vec<String>, String> {
    let value = value.ok_or_else(|| String::from("om_terrain is required"))?;
    let values = match value {
        Value::String(id) => vec![id.clone()],
        Value::Array(values) => {
            if values.is_empty() {
                return Err(String::from("om_terrain array must not be empty"));
            }
            let mut ids = Vec::with_capacity(values.len());
            for value in values {
                let id = value.as_str().ok_or_else(|| {
                    String::from("om_terrain must be a string or flat string array")
                })?;
                ids.push(id.to_owned());
            }
            ids
        }
        _ => {
            return Err(String::from(
                "om_terrain must be a string or flat string array",
            ));
        }
    };
    if values.len() > MAX_MAPGEN_OM_TERRAINS {
        return Err(format!("om_terrain count exceeds {MAX_MAPGEN_OM_TERRAINS}"));
    }
    let mut unique = BTreeSet::new();
    for id in &values {
        if id.is_empty() {
            return Err(String::from("om_terrain ids must not be empty"));
        }
        if !unique.insert(id) {
            return Err(format!("om_terrain contains duplicate id {id:?}"));
        }
    }
    Ok(values)
}

fn parse_root_weight(value: Option<&Value>) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_MAPGEN_WEIGHT);
    };
    let raw = match value {
        Value::Number(_) => value
            .as_u64()
            .ok_or_else(|| String::from("weight must be a positive integer"))?,
        Value::Object(object)
            if object.len() == 2
                && object
                    .get("global_val")
                    .and_then(Value::as_str)
                    .is_some_and(|id| {
                        !id.is_empty()
                            && id.len() <= 512
                            && id.chars().all(|character| !character.is_control())
                    }) =>
        {
            object
                .get("default")
                .and_then(Value::as_u64)
                .ok_or_else(|| String::from("global weight default must be a positive integer"))?
        }
        Value::Object(_) => {
            return Err(String::from(
                "weight expression is unsupported without one global_val and integer default",
            ));
        }
        _ => return Err(String::from("weight must be a positive integer")),
    };
    let weight = u32::try_from(raw)
        .map_err(|_| format!("weight exceeds strict bound {MAX_MAPGEN_WEIGHT}"))?;
    if weight == 0 || weight > MAX_MAPGEN_WEIGHT {
        return Err(format!("weight must be between 1 and {MAX_MAPGEN_WEIGHT}"));
    }
    Ok(weight)
}

fn parse_nested_weight(value: Option<&Value>) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_MAPGEN_WEIGHT);
    };
    let raw = value
        .as_u64()
        .ok_or_else(|| String::from("nested mapgen weight must be a nonnegative integer"))?;
    u32::try_from(raw)
        .ok()
        .filter(|weight| *weight <= MAX_MAPGEN_WEIGHT)
        .ok_or_else(|| format!("nested mapgen weight exceeds {MAX_MAPGEN_WEIGHT}"))
}

fn parse_rows(value: Option<&Value>) -> Result<Vec<String>, String> {
    parse_rows_sized(value, MAPGEN_WIDTH, MAPGEN_HEIGHT)
}

fn parse_rows_sized(
    value: Option<&Value>,
    width: usize,
    height: usize,
) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(vec![String::from(" "); width * height]);
    };
    let rows = value
        .as_array()
        .ok_or_else(|| String::from("rows must be an array"))?;
    if rows.len() != height {
        return Err(format!("rows must contain exactly {height} rows"));
    }
    let mut cells = Vec::with_capacity(width * height);
    for (y, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .ok_or_else(|| format!("row {y} must be a string"))?;
        let row_cells = split_display_cells(row, &format!("row {y}"))?;
        if row_cells.len() != width {
            return Err(format!(
                "row {y} must contain exactly {width} Unicode display cells, found {}",
                row_cells.len()
            ));
        }
        cells.extend(row_cells);
    }
    Ok(cells)
}

#[derive(Default)]
struct CompileState {
    terrain: BTreeMap<String, Vec<MapgenIdChoice>>,
    furniture: BTreeMap<String, Vec<MapgenIdChoice>>,
    items: BTreeMap<String, StrictMapgenItemPlacement>,
    palette_closure: Vec<String>,
    layer_count: usize,
    deferred_fields: BTreeSet<String>,
}

fn append_palette<TerrainExists, FurnitureExists, ItemGroupExists>(
    id: &str,
    palettes: &PaletteCatalog,
    terrain_exists: &TerrainExists,
    furniture_exists: &FurnitureExists,
    item_group_exists: &ItemGroupExists,
    ancestors: &mut Vec<String>,
    state: &mut CompileState,
) -> Result<(), String>
where
    TerrainExists: Fn(&str) -> bool,
    FurnitureExists: Fn(&str) -> bool,
    ItemGroupExists: Fn(&str) -> bool,
{
    if ancestors.len() >= MAX_MAPGEN_PALETTE_DEPTH {
        return Err(format!(
            "palette reference depth exceeds {MAX_MAPGEN_PALETTE_DEPTH} at {id:?}"
        ));
    }
    if let Some(position) = ancestors.iter().position(|ancestor| ancestor == id) {
        let mut cycle = ancestors[position..].to_vec();
        cycle.push(id.to_owned());
        return Err(format!("palette reference cycle: {}", cycle.join(" -> ")));
    }
    if state.palette_closure.len() >= MAX_MAPGEN_PALETTE_LAYERS {
        return Err(format!(
            "palette expansion exceeds {MAX_MAPGEN_PALETTE_LAYERS} layers"
        ));
    }
    let candidates = palettes
        .get(id)
        .ok_or_else(|| format!("named palette {id:?} does not exist"))?;
    if candidates.len() != 1 {
        return Err(format!(
            "named palette {id:?} has {} selected definitions",
            candidates.len()
        ));
    }
    let palette = &candidates[0];
    reject_unknown_fields(
        &palette.object,
        PALETTE_FIELDS,
        &format!("palette {id:?} from {}", palette.source),
    )?;
    state.palette_closure.push(id.to_owned());
    ancestors.push(id.to_owned());
    let nested = parse_palette_ids(palette.object.get("palettes"), &format!("palette {id:?}"))?;
    for child in nested {
        append_palette(
            child,
            palettes,
            terrain_exists,
            furniture_exists,
            item_group_exists,
            ancestors,
            state,
        )?;
    }
    ancestors.pop();
    append_local_bindings(
        &palette.object,
        &format!("palette {id:?}"),
        terrain_exists,
        furniture_exists,
        item_group_exists,
        state,
    )
}

fn parse_palette_ids<'a>(value: Option<&'a Value>, context: &str) -> Result<Vec<&'a str>, String> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .ok_or_else(|| format!("palettes in {context} must be an array"))?;
    if values.len() > MAX_MAPGEN_PALETTE_LAYERS {
        return Err(format!(
            "palettes in {context} exceeds {MAX_MAPGEN_PALETTE_LAYERS} entries"
        ));
    }
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| format!("palettes in {context} must contain only static ids"))
        })
        .collect()
}

fn append_local_bindings<TerrainExists, FurnitureExists, ItemGroupExists>(
    object: &Map<String, Value>,
    context: &str,
    terrain_exists: &TerrainExists,
    furniture_exists: &FurnitureExists,
    item_group_exists: &ItemGroupExists,
    state: &mut CompileState,
) -> Result<(), String>
where
    TerrainExists: Fn(&str) -> bool,
    FurnitureExists: Fn(&str) -> bool,
    ItemGroupExists: Fn(&str) -> bool,
{
    append_binding_map(
        object.get("terrain"),
        "terrain",
        context,
        terrain_exists,
        &mut state.terrain,
        &mut state.layer_count,
    )?;
    append_binding_map(
        object.get("furniture"),
        "furniture",
        context,
        furniture_exists,
        &mut state.furniture,
        &mut state.layer_count,
    )?;
    append_item_binding_map(
        object.get("items"),
        context,
        item_group_exists,
        &mut state.items,
        &mut state.layer_count,
    )?;
    append_sign_binding_map(
        object.get("signs"),
        context,
        furniture_exists,
        &mut state.furniture,
        &mut state.layer_count,
        &mut state.deferred_fields,
    )
}

fn append_sign_binding_map<Exists>(
    value: Option<&Value>,
    context: &str,
    exists: &Exists,
    furniture: &mut BTreeMap<String, Vec<MapgenIdChoice>>,
    layer_count: &mut usize,
    deferred_fields: &mut BTreeSet<String>,
) -> Result<(), String>
where
    Exists: Fn(&str) -> bool,
{
    let Some(value) = value else {
        return Ok(());
    };
    let bindings = value
        .as_object()
        .ok_or_else(|| format!("signs in {context} must be a glyph object"))?;
    if bindings.len() > MAX_MAPGEN_BINDINGS {
        return Err(format!(
            "signs in {context} exceeds {MAX_MAPGEN_BINDINGS} bindings"
        ));
    }
    for (key, value) in bindings {
        let key = parse_binding_key(key, "signs", context)?;
        let object = value
            .as_object()
            .ok_or_else(|| format!("signs[{key:?}] in {context} must be an object"))?;
        reject_unknown_fields(
            object,
            &["furniture", "signage"],
            &format!("signs[{key:?}] in {context}"),
        )?;
        let furniture_id = object
            .get("furniture")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && exists(id))
            .ok_or_else(|| {
                format!("signs[{key:?}] in {context} has unknown or missing furniture")
            })?;
        *layer_count = layer_count
            .checked_add(1)
            .ok_or_else(|| String::from("mapgen placement layer count overflow"))?;
        if *layer_count > MAX_MAPGEN_PALETTE_LAYERS {
            return Err(format!(
                "mapgen placement layers exceed {MAX_MAPGEN_PALETTE_LAYERS}"
            ));
        }
        furniture
            .entry(key)
            .or_default()
            .push(MapgenIdChoice::Fixed(furniture_id.to_owned()));
        if object.contains_key("signage") {
            deferred_fields.insert(String::from("signage_text"));
        }
    }
    Ok(())
}

fn append_binding_map<Exists>(
    value: Option<&Value>,
    field: &str,
    context: &str,
    exists: &Exists,
    output: &mut BTreeMap<String, Vec<MapgenIdChoice>>,
    layer_count: &mut usize,
) -> Result<(), String>
where
    Exists: Fn(&str) -> bool,
{
    let Some(value) = value else {
        return Ok(());
    };
    let bindings = value
        .as_object()
        .ok_or_else(|| format!("{field} in {context} must be an object"))?;
    if bindings.len() > MAX_MAPGEN_BINDINGS {
        return Err(format!(
            "{field} in {context} exceeds {MAX_MAPGEN_BINDINGS} bindings"
        ));
    }
    for (key, value) in bindings {
        let key = parse_binding_key(key, field, context)?;
        let choice = parse_choice(value, &format!("{field}[{key:?}] in {context}"), exists)?;
        *layer_count = layer_count
            .checked_add(1)
            .ok_or_else(|| String::from("mapgen placement layer count overflow"))?;
        if *layer_count > MAX_MAPGEN_PALETTE_LAYERS {
            return Err(format!(
                "mapgen placement layers exceed {MAX_MAPGEN_PALETTE_LAYERS}"
            ));
        }
        output.entry(key).or_default().push(choice);
    }
    Ok(())
}

fn append_item_binding_map<Exists>(
    value: Option<&Value>,
    context: &str,
    exists: &Exists,
    output: &mut BTreeMap<String, StrictMapgenItemPlacement>,
    layer_count: &mut usize,
) -> Result<(), String>
where
    Exists: Fn(&str) -> bool,
{
    let Some(value) = value else {
        return Ok(());
    };
    let bindings = value
        .as_object()
        .ok_or_else(|| format!("items in {context} must be a glyph object"))?;
    if bindings.len() > MAX_MAPGEN_BINDINGS {
        return Err(format!(
            "items in {context} exceeds {MAX_MAPGEN_BINDINGS} bindings"
        ));
    }
    for (key, value) in bindings {
        let key = parse_binding_key(key, "items", context)?;
        let object = value.as_object().ok_or_else(|| {
            format!("items[{key:?}] in {context} must be one static placement object")
        })?;
        if let Some(field) = object
            .keys()
            .find(|field| !["item", "chance", "repeat"].contains(&field.as_str()))
        {
            return Err(format!(
                "unsupported field {field:?} in items[{key:?}] in {context}"
            ));
        }
        let item_group = object
            .get("item")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                format!("item in items[{key:?}] in {context} must be a named item-group id")
            })?;
        if !exists(item_group) {
            return Err(format!(
                "items[{key:?}] in {context} references unknown or non-strict item group {item_group:?}"
            ));
        }
        let chance = object
            .get("chance")
            .and_then(Value::as_u64)
            .and_then(|chance| u8::try_from(chance).ok())
            .filter(|chance| (1..=100).contains(chance))
            .ok_or_else(|| {
                format!("chance in items[{key:?}] in {context} must be an integer from 1 to 100")
            })?;
        let (repeat_minimum, repeat_maximum) = parse_repeat_range(object.get("repeat"), context)?;
        *layer_count = layer_count
            .checked_add(1)
            .ok_or_else(|| String::from("mapgen placement layer count overflow"))?;
        if *layer_count > MAX_MAPGEN_PALETTE_LAYERS {
            return Err(format!(
                "mapgen placement layers exceed {MAX_MAPGEN_PALETTE_LAYERS}"
            ));
        }
        if output
            .insert(
                key.clone(),
                StrictMapgenItemPlacement {
                    item_group: item_group.to_owned(),
                    chance,
                    repeat_minimum,
                    repeat_maximum,
                },
            )
            .is_some()
        {
            return Err(format!(
                "items[{key:?}] in {context} duplicates an inherited static item placement"
            ));
        }
    }
    Ok(())
}

fn parse_repeat_range(value: Option<&Value>, context: &str) -> Result<(u16, u16), String> {
    let Some(value) = value else {
        return Ok((1, 1));
    };
    let (minimum, maximum) = if let Some(value) = value.as_u64() {
        (value, value)
    } else {
        let values = value
            .as_array()
            .filter(|values| values.len() == 2)
            .ok_or_else(|| format!("repeat in {context} must be an integer or interval"))?;
        (
            values[0]
                .as_u64()
                .ok_or_else(|| format!("repeat minimum in {context} must be an integer"))?,
            values[1]
                .as_u64()
                .ok_or_else(|| format!("repeat maximum in {context} must be an integer"))?,
        )
    };
    let minimum = u16::try_from(minimum)
        .ok()
        .filter(|value| *value <= 256)
        .ok_or_else(|| format!("repeat minimum in {context} exceeds 256"))?;
    let maximum = u16::try_from(maximum)
        .ok()
        .filter(|value| *value <= 256 && *value >= minimum)
        .ok_or_else(|| format!("repeat maximum in {context} is invalid"))?;
    Ok((minimum, maximum))
}

fn parse_binding_key(key: &str, field: &str, context: &str) -> Result<String, String> {
    let cells = split_display_cells(key, &format!("{field} in {context} key"))?;
    if cells.len() != 1 {
        return Err(format!(
            "{field} in {context} key must be one Unicode display cell"
        ));
    }
    cells
        .into_iter()
        .next()
        .ok_or_else(|| format!("{field} in {context} contains an empty display-cell key"))
}

fn split_display_cells(value: &str, context: &str) -> Result<Vec<String>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let mut cells = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        let width = UnicodeWidthChar::width(character)
            .ok_or_else(|| format!("{context} contains a non-displayable character"))?;
        if width > 0 && !current.is_empty() {
            cells.push(std::mem::take(&mut current));
        }
        current.push(character);
    }
    if !current.is_empty() {
        cells.push(current);
    }
    Ok(cells)
}

fn parse_choice<Exists>(
    value: &Value,
    context: &str,
    exists: &Exists,
) -> Result<MapgenIdChoice, String>
where
    Exists: Fn(&str) -> bool,
{
    if let Some(id) = value.as_str() {
        validate_id(id, context, exists)?;
        return Ok(MapgenIdChoice::Fixed(id.to_owned()));
    }
    let values = value
        .as_array()
        .ok_or_else(|| format!("{context} must be an id or weighted id array"))?;
    if values.is_empty() || values.len() > MAX_MAPGEN_CHOICE_ENTRIES {
        return Err(format!(
            "{context} weighted choice must contain 1..={MAX_MAPGEN_CHOICE_ENTRIES} entries"
        ));
    }
    let mut total_weight = 0_u64;
    let mut entries = Vec::with_capacity(values.len());
    for value in values {
        let (id, weight) = if let Some(id) = value.as_str() {
            (id, 1_u32)
        } else {
            let pair = value
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| {
                    format!("{context} entries must be ids or [id, positive integer weight]")
                })?;
            let id = pair[0]
                .as_str()
                .ok_or_else(|| format!("{context} weighted entry id must be a non-empty string"))?;
            let raw_weight = pair[1]
                .as_u64()
                .ok_or_else(|| format!("{context} weights must be positive integers"))?;
            let weight = u32::try_from(raw_weight)
                .map_err(|_| format!("{context} weight exceeds {MAX_MAPGEN_CHOICE_WEIGHT}"))?;
            (id, weight)
        };
        if weight == 0 || weight > MAX_MAPGEN_CHOICE_WEIGHT {
            return Err(format!(
                "{context} weights must be between 1 and {MAX_MAPGEN_CHOICE_WEIGHT}"
            ));
        }
        validate_id(id, context, exists)?;
        total_weight = total_weight
            .checked_add(u64::from(weight))
            .ok_or_else(|| format!("{context} total weight overflow"))?;
        if total_weight > MAX_MAPGEN_CHOICE_TOTAL_WEIGHT {
            return Err(format!(
                "{context} total weight exceeds {MAX_MAPGEN_CHOICE_TOTAL_WEIGHT}"
            ));
        }
        entries.push(WeightedMapgenId {
            id: id.to_owned(),
            weight,
        });
    }
    Ok(MapgenIdChoice::Weighted(entries))
}

fn validate_id<Exists>(id: &str, context: &str, exists: &Exists) -> Result<(), String>
where
    Exists: Fn(&str) -> bool,
{
    if id.is_empty() {
        return Err(format!("{context} contains an empty id"));
    }
    if !exists(id) {
        return Err(format!("{context} references unknown id {id:?}"));
    }
    Ok(())
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), String> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()) && !field.starts_with("//"))
    {
        return Err(format!("unsupported field {field:?} in {context}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_with(symbol: char) -> Value {
        let mut rows = vec!["                        ".to_owned(); MAPGEN_HEIGHT];
        rows[0].replace_range(..1, &symbol.to_string());
        Value::Array(rows.into_iter().map(Value::String).collect())
    }

    #[test]
    fn static_palette_closure_preserves_separate_layers() {
        let base = RawPalette {
            source: String::from("fixture#base"),
            object: serde_json::from_value(serde_json::json!({
                "type": "palette",
                "id": "base",
                "terrain": { "x": "t_wall" },
                "furniture": { "x": [["f_chair", 3], ["f_table", 1]] }
            }))
            .expect("fixture object"),
        };
        let child = RawPalette {
            source: String::from("fixture#child"),
            object: serde_json::from_value(serde_json::json!({
                "type": "palette",
                "id": "child",
                "palettes": ["base"],
                "terrain": { "x": ["t_floor", ["t_dirt", 2]] }
            }))
            .expect("fixture object"),
        };
        let palettes = BTreeMap::from([
            (String::from("base"), vec![base]),
            (String::from("child"), vec![child]),
        ]);
        let raw = RawMapgenRoot {
            source: String::from("fixture#root"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": "fixture",
                "object": {
                    "fill_ter": "t_floor",
                    "rows": rows_with('x'),
                    "palettes": ["child"],
                    "terrain": { "x": "t_local" }
                }
            }))
            .expect("fixture object"),
        };
        let terrain_ids = ["t_wall", "t_floor", "t_dirt", "t_local"];
        let furniture_ids = ["f_chair", "f_table"];
        let compiled = compile_root(
            &raw,
            &palettes,
            &|id| terrain_ids.contains(&id),
            &|id| furniture_ids.contains(&id),
            &|_| true,
        )
        .expect("static palette fixture compiles");

        assert_eq!(compiled.palette_closure, ["child", "base"]);
        let layers = compiled.terrain.get("x").expect("x terrain layers");
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], MapgenIdChoice::Fixed(String::from("t_wall")));
        assert!(matches!(&layers[1], MapgenIdChoice::Weighted(entries) if entries.len() == 2));
        assert_eq!(layers[2], MapgenIdChoice::Fixed(String::from("t_local")));
        assert!(matches!(
            compiled.furniture.get("x").and_then(|layers| layers.first()),
            Some(MapgenIdChoice::Weighted(entries)) if entries[0].weight == 3
        ));
    }

    #[test]
    fn unsupported_item_placement_shape_is_rejected() {
        let raw = RawMapgenRoot {
            source: String::from("fixture#root"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": "fixture",
                "object": {
                    "fill_ter": "t_floor",
                    "items": { "x": { "item": "test", "chance": 1, "unsupported": 2 } }
                }
            }))
            .expect("fixture object"),
        };
        let error = compile_root(&raw, &BTreeMap::new(), &|_| true, &|_| true, &|_| true)
            .expect_err("unknown item-placement fields remain fail-closed");
        assert!(error.contains("unsupported field \"unsupported\""));
    }

    #[test]
    fn an_unsupported_variant_makes_the_overmap_id_unavailable_with_a_report() {
        let supported = RawMapgenRoot {
            source: String::from("fixture#supported"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": "fixture",
                "object": { "fill_ter": "t_floor" }
            }))
            .expect("fixture object"),
        };
        let unsupported = RawMapgenRoot {
            source: String::from("fixture#unsupported"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": "fixture",
                "object": {
                    "fill_ter": "t_floor",
                    "items": { "x": { "item": "test", "chance": 1, "unsupported": 2 } }
                }
            }))
            .expect("fixture object"),
        };
        let registry = compile_registry(
            &[supported, unsupported],
            &[],
            &BTreeMap::new(),
            &|_| true,
            &|_| true,
            &|_| true,
        )
        .expect("unsupported roots are reports, not load errors");

        assert!(registry.get("fixture").is_none());
        let reports = registry
            .unavailable_reports("fixture")
            .expect("explicit unavailability report");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].source, "fixture#unsupported");
        assert!(
            reports[0]
                .rejection_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("items"))
        );
    }

    #[test]
    fn multi_id_definitions_and_reports_share_bounded_registry_storage() {
        let supported = RawMapgenRoot {
            source: String::from("fixture#supported-multi"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": ["supported_a", "supported_b"],
                "object": { "fill_ter": "t_floor" }
            }))
            .expect("fixture object"),
        };
        let unsupported = RawMapgenRoot {
            source: String::from("fixture#unsupported-multi"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": ["unsupported_a", "unsupported_b"],
                "object": { "fill_ter": "t_floor", "unsupported": true }
            }))
            .expect("fixture object"),
        };
        let registry = compile_registry(
            &[supported, unsupported],
            &[],
            &BTreeMap::new(),
            &|_| true,
            &|_| true,
            &|_| true,
        )
        .expect("bounded multi-id fixtures should compile");

        let supported_a = &registry.get("supported_a").expect("first id")[0];
        let supported_b = &registry.get("supported_b").expect("second id")[0];
        assert!(Arc::ptr_eq(supported_a, supported_b));
        let unsupported_a = &registry
            .unavailable_reports("unsupported_a")
            .expect("first unavailable id")[0];
        let unsupported_b = &registry
            .unavailable_reports("unsupported_b")
            .expect("second unavailable id")[0];
        assert!(Arc::ptr_eq(unsupported_a, unsupported_b));
    }

    #[test]
    fn static_named_item_group_placement_preserves_item_phase_data() {
        let raw = RawMapgenRoot {
            source: String::from("fixture#root"),
            object: serde_json::from_value(serde_json::json!({
                "type": "mapgen",
                "om_terrain": "fixture",
                "object": {
                    "fill_ter": "t_floor",
                    "items": { "x": { "item": "field", "chance": 1 } }
                }
            }))
            .expect("fixture object"),
        };
        let compiled = compile_root(&raw, &BTreeMap::new(), &|_| true, &|_| true, &|id| {
            id == "field"
        })
        .expect("strict named item-group placement compiles");

        assert_eq!(
            compiled.items.get("x"),
            Some(&StrictMapgenItemPlacement {
                item_group: String::from("field"),
                chance: 1,
                repeat_minimum: 1,
                repeat_maximum: 1,
            })
        );
    }

    #[test]
    fn rows_count_unicode_cells_instead_of_utf8_bytes() {
        let rows = rows_with('≷');
        let cells = parse_rows(Some(&rows)).expect("one Unicode display cell");
        assert_eq!(cells.len(), MAPGEN_WIDTH * MAPGEN_HEIGHT);
        assert_eq!(cells[0], "≷");
    }

    #[test]
    fn rows_and_bindings_group_combining_marks_like_pinned_display_cells() {
        let combined = "a\u{301}";
        let rows = Value::Array(
            (0..MAPGEN_HEIGHT)
                .map(|_| Value::String(format!("{combined}{}", ".".repeat(MAPGEN_WIDTH - 1))))
                .collect(),
        );
        let cells = parse_rows(Some(&rows)).expect("combining sequence is one display cell");
        assert_eq!(cells.len(), MAPGEN_WIDTH * MAPGEN_HEIGHT);
        assert_eq!(cells[0], combined);
        assert_eq!(
            parse_binding_key(combined, "terrain", "fixture").expect("combined binding"),
            combined
        );
    }

    #[test]
    fn root_weights_admit_static_global_defaults_and_reject_dynamic_math() {
        assert_eq!(
            parse_root_weight(Some(&serde_json::json!({
                "global_val": "vanilla_road_weight",
                "default": 1000
            })))
            .expect("static global default"),
            1000
        );
        assert!(
            parse_root_weight(Some(&serde_json::json!({
                "math": ["time_since('cataclysm')"]
            })))
            .is_err()
        );
    }
}
