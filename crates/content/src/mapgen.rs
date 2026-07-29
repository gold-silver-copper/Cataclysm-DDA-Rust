use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};
use unicode_width::UnicodeWidthChar;

use crate::{
    ContentManifest, FurnitureRegistry, ItemGroupRegistry, ModCatalog, ModCatalogError,
    SelectedContentFile, TerrainRegistry,
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

const MAX_DISCOVERED_OM_TERRAINS: usize = 1_024;
const ROOT_FIELDS: &[&str] = &["type", "om_terrain", "weight", "object"];
const OBJECT_FIELDS: &[&str] = &[
    "fill_ter",
    "rows",
    "terrain",
    "furniture",
    "items",
    "palettes",
];
const PALETTE_FIELDS: &[&str] = &["type", "id", "terrain", "furniture", "items", "palettes"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedMapgenId {
    pub id: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMapgenItemPlacement {
    pub item_group: String,
    pub chance: u8,
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
        let (roots, palettes) = read_mapgen(content_root.as_ref(), files)?;
        compile_registry(
            &roots,
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
struct RawPalette {
    source: String,
    object: Map<String, Value>,
}

type PaletteCatalog = BTreeMap<String, Vec<RawPalette>>;

fn read_mapgen(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<(Vec<RawMapgenRoot>, PaletteCatalog), MapgenRegistryError> {
    let mut roots = Vec::new();
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
                &mut palettes,
                &mut palette_count,
            )?,
        }
    }
    Ok((roots, palettes))
}

fn collect_definition(
    file: &SelectedContentFile,
    index: usize,
    value: Value,
    roots: &mut Vec<RawMapgenRoot>,
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
    Ok(MapgenRegistry {
        definitions,
        unavailable,
        reports,
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
    if fill_terrain.is_none() {
        if object.get("rows").is_none() {
            return Err(String::from("mapgen without rows requires fill_ter"));
        }
        for cell in &cells {
            if !state.terrain.contains_key(cell) {
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
    })
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
    let raw = value
        .as_u64()
        .ok_or_else(|| String::from("weight must be a positive integer"))?;
    let weight = u32::try_from(raw)
        .map_err(|_| format!("weight exceeds strict bound {MAX_MAPGEN_WEIGHT}"))?;
    if weight == 0 || weight > MAX_MAPGEN_WEIGHT {
        return Err(format!("weight must be between 1 and {MAX_MAPGEN_WEIGHT}"));
    }
    Ok(weight)
}

fn parse_rows(value: Option<&Value>) -> Result<Vec<String>, String> {
    let Some(value) = value else {
        return Ok(vec![String::from(" "); MAPGEN_WIDTH * MAPGEN_HEIGHT]);
    };
    let rows = value
        .as_array()
        .ok_or_else(|| String::from("rows must be an array"))?;
    if rows.len() != MAPGEN_HEIGHT {
        return Err(format!("rows must contain exactly {MAPGEN_HEIGHT} rows"));
    }
    let mut cells = Vec::with_capacity(MAPGEN_WIDTH * MAPGEN_HEIGHT);
    for (y, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .ok_or_else(|| format!("row {y} must be a string"))?;
        let row_cells = split_display_cells(row, &format!("row {y}"))?;
        if row_cells.len() != MAPGEN_WIDTH {
            return Err(format!(
                "row {y} must contain exactly {MAPGEN_WIDTH} Unicode display cells, found {}",
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
    )
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
            .find(|field| !["item", "chance"].contains(&field.as_str()))
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
                    "items": { "x": { "item": "test", "chance": 1, "repeat": 2 } }
                }
            }))
            .expect("fixture object"),
        };
        let error = compile_root(&raw, &BTreeMap::new(), &|_| true, &|_| true, &|_| true)
            .expect_err("repeat remains outside this slice");
        assert!(error.contains("unsupported field \"repeat\""));
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
                    "items": { "x": { "item": "test", "chance": 1, "repeat": 2 } }
                }
            }))
            .expect("fixture object"),
        };
        let registry = compile_registry(
            &[supported, unsupported],
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
}
