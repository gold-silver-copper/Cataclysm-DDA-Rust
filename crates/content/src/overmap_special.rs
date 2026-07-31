use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{
    ContentManifest, ModCatalog, ModCatalogError, OvermapTerrainRegistry, SelectedContentFile,
};

pub const MAX_OVERMAP_SPECIALS: usize = 8_192;
const MAX_OVERMAP_LOCATIONS: usize = 1_024;
const MAX_SPECIAL_PARTS: usize = 4_096;
const MAX_SPECIAL_CONNECTIONS: usize = 256;
const MAX_SPECIAL_FLAGS: usize = 256;
const MAX_ID_BYTES: usize = 512;
const MAX_SPECIAL_SPAWN_POPULATION: i32 = 1_000_000;
const MAX_SPECIAL_SPAWN_RADIUS: i32 = 180;

const SPECIAL_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "subtype",
    "locations",
    "overmaps",
    "connections",
    "city_sizes",
    "city_distance",
    "occurrences",
    "priority",
    "spawns",
    "rotate",
    "flags",
    "eoc",
    "extend",
    "delete",
    "relative",
    "proportional",
    "//",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OvermapSpecialInterval {
    pub minimum: i32,
    pub maximum: i32,
}

impl OvermapSpecialInterval {
    #[must_use]
    pub const fn contains(self, value: i32) -> bool {
        value >= self.minimum && value <= self.maximum
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OvermapLocationDefinition {
    pub id: String,
    /// Finalized OMT type IDs after expanding the pinned location flags.
    pub terrain_types: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapSpecialTerrainDefinition {
    pub point: [i32; 3],
    pub overmap: Option<String>,
    pub locations: BTreeSet<String>,
    pub flags: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapSpecialConnectionDefinition {
    pub point: [i32; 3],
    pub from: Option<[i32; 3]>,
    pub terrain: Option<String>,
    pub connection: Option<String>,
    pub existing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapSpecialMonsterSpawnDefinition {
    pub monster_group: String,
    pub population: OvermapSpecialInterval,
    pub radius: OvermapSpecialInterval,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapSpecialDefinition {
    pub id: String,
    pub terrains: Vec<OvermapSpecialTerrainDefinition>,
    pub connections: Vec<OvermapSpecialConnectionDefinition>,
    pub default_locations: BTreeSet<String>,
    pub city_sizes: OvermapSpecialInterval,
    pub city_distance: OvermapSpecialInterval,
    pub occurrences: OvermapSpecialInterval,
    pub priority: i32,
    pub rotate: bool,
    pub flags: BTreeSet<String>,
    pub monster_spawn: Option<OvermapSpecialMonsterSpawnDefinition>,
    /// Every retained reason blocks runtime admission. Unsupported semantics
    /// are never silently discarded while later families are incomplete.
    pub unsupported_reasons: BTreeSet<String>,
    pub source: String,
}

impl Default for OvermapSpecialDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            terrains: Vec::new(),
            connections: Vec::new(),
            default_locations: BTreeSet::new(),
            city_sizes: OvermapSpecialInterval {
                minimum: 0,
                maximum: i32::MAX,
            },
            city_distance: OvermapSpecialInterval {
                minimum: 0,
                maximum: i32::MAX,
            },
            occurrences: OvermapSpecialInterval {
                minimum: 0,
                maximum: 0,
            },
            priority: 0,
            rotate: true,
            flags: BTreeSet::new(),
            monster_spawn: None,
            unsupported_reasons: BTreeSet::new(),
            source: String::new(),
        }
    }
}

impl OvermapSpecialDefinition {
    #[must_use]
    pub fn placement_semantics_are_supported(&self) -> bool {
        self.unsupported_reasons.is_empty()
            && !self.terrains.is_empty()
            && self.occurrences.maximum > 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OvermapSpecialRegistry {
    locations: BTreeMap<String, OvermapLocationDefinition>,
    definitions: BTreeMap<String, OvermapSpecialDefinition>,
}

impl OvermapSpecialRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
        overmap_terrain: &OvermapTerrainRegistry,
    ) -> Result<Self, OvermapSpecialRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(OvermapSpecialRegistryError::Catalog)?;
        compile_registry(content_root.as_ref(), files, overmap_terrain)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&OvermapSpecialDefinition> {
        self.definitions.get(id)
    }

    #[must_use]
    pub fn location(&self, id: &str) -> Option<&OvermapLocationDefinition> {
        self.locations.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &OvermapSpecialDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}

#[derive(Clone)]
struct RawDefinition {
    source: String,
    object: Map<String, Value>,
}

fn compile_registry(
    root: &Path,
    files: Vec<SelectedContentFile>,
    overmap_terrain: &OvermapTerrainRegistry,
) -> Result<OvermapSpecialRegistry, OvermapSpecialRegistryError> {
    let mut raw_locations = Vec::new();
    let mut raw_specials = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| OvermapSpecialRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| OvermapSpecialRegistryError::Json(file.destination.clone(), error))?;
        let values = match value {
            Value::Array(values) => values,
            value => vec![value],
        };
        for (index, value) in values.into_iter().enumerate() {
            let Some(object) = value.as_object().cloned() else {
                continue;
            };
            let raw = RawDefinition {
                source: format!("{}#{index}", file.upstream_path),
                object,
            };
            match raw.object.get("type").and_then(Value::as_str) {
                Some("overmap_location") => raw_locations.push(raw),
                Some("overmap_special") => raw_specials.push_back(raw),
                _ => {}
            }
        }
    }
    if raw_locations.len() > MAX_OVERMAP_LOCATIONS || raw_specials.len() > MAX_OVERMAP_SPECIALS {
        return Err(OvermapSpecialRegistryError::LimitExceeded);
    }

    let mut locations = BTreeMap::new();
    for raw in raw_locations {
        compile_location(&raw, &mut locations, overmap_terrain)?;
    }
    let mut definitions = BTreeMap::new();
    while !raw_specials.is_empty() {
        let pass = raw_specials.len();
        let mut loaded = 0;
        for _ in 0..pass {
            let raw = raw_specials.pop_front().ok_or_else(|| {
                OvermapSpecialRegistryError::Invalid(String::from("special queue disappeared"))
            })?;
            if compile_special(&raw, &locations, &mut definitions)? {
                loaded += 1;
            } else {
                raw_specials.push_back(raw);
            }
        }
        if loaded == 0 {
            return Err(OvermapSpecialRegistryError::Invalid(format!(
                "unresolved or cyclic overmap-special inheritance: {:?}",
                raw_specials
                    .iter()
                    .take(20)
                    .map(|raw| raw.source.as_str())
                    .collect::<Vec<_>>()
            )));
        }
    }
    Ok(OvermapSpecialRegistry {
        locations,
        definitions,
    })
}

fn compile_location(
    raw: &RawDefinition,
    locations: &mut BTreeMap<String, OvermapLocationDefinition>,
    overmap_terrain: &OvermapTerrainRegistry,
) -> Result<(), OvermapSpecialRegistryError> {
    let id = required_id(&raw.object, "id", &raw.source)?;
    let mut definition = raw
        .object
        .get("copy-from")
        .map(|value| required_text_value(value, "copy-from", &raw.source))
        .transpose()?
        .map(|parent| {
            locations.get(parent).cloned().ok_or_else(|| {
                OvermapSpecialRegistryError::Invalid(format!(
                    "{} copies unavailable overmap location {parent:?}",
                    raw.source
                ))
            })
        })
        .transpose()?
        .unwrap_or_default();
    definition.id = id.to_owned();
    definition.source.clone_from(&raw.source);
    if let Some(value) = raw.object.get("terrains") {
        definition.terrain_types = string_set(value, "terrains", &raw.source)?;
    }
    if let Some(value) = raw.object.get("flags") {
        for flag in string_set(value, "flags", &raw.source)? {
            definition
                .terrain_types
                .extend(overmap_terrain.identities().filter_map(|(_, identity)| {
                    overmap_terrain
                        .get_type(&identity.type_id)
                        .is_some_and(|terrain| terrain.flags.contains(&flag))
                        .then(|| identity.type_id.clone())
                }));
        }
    }
    if definition.terrain_types.is_empty() {
        return Err(OvermapSpecialRegistryError::Invalid(format!(
            "{} overmap location {id:?} has no terrain types after flag expansion",
            raw.source
        )));
    }
    locations.insert(id.to_owned(), definition);
    Ok(())
}

fn compile_special(
    raw: &RawDefinition,
    locations: &BTreeMap<String, OvermapLocationDefinition>,
    definitions: &mut BTreeMap<String, OvermapSpecialDefinition>,
) -> Result<bool, OvermapSpecialRegistryError> {
    let id = required_id(&raw.object, "id", &raw.source)?;
    let parent = raw
        .object
        .get("copy-from")
        .map(|value| required_text_value(value, "copy-from", &raw.source))
        .transpose()?;
    let Some(mut definition) = parent
        .map(|parent| definitions.get(parent).cloned())
        .unwrap_or_else(|| Some(OvermapSpecialDefinition::default()))
    else {
        return Ok(false);
    };
    definition.id = id.to_owned();
    definition.source.clone_from(&raw.source);
    if raw.object.get("subtype").and_then(Value::as_str) == Some("mutable") {
        definition
            .unsupported_reasons
            .insert(String::from("mutable overmap-special layout"));
    }
    if let Some(value) = raw.object.get("locations") {
        definition.default_locations = string_set(value, "locations", &raw.source)?;
    }
    if let Some(value) = raw.object.get("overmaps") {
        definition.terrains =
            parse_terrains(value, &raw.source, &mut definition.unsupported_reasons)?;
    } else if parent.is_none() {
        definition
            .unsupported_reasons
            .insert(String::from("missing fixed overmaps array"));
    }
    if let Some(value) = raw.object.get("connections") {
        definition.connections = parse_connections(value, &raw.source)?;
    }
    if let Some(value) = raw.object.get("city_sizes") {
        definition.city_sizes = parse_interval(value, "city_sizes", true, &raw.source)?;
    }
    if let Some(value) = raw.object.get("city_distance") {
        definition.city_distance = parse_interval(value, "city_distance", true, &raw.source)?;
    }
    if let Some(value) = raw.object.get("occurrences") {
        definition.occurrences = parse_interval(value, "occurrences", false, &raw.source)?;
    } else if parent.is_none() {
        definition
            .unsupported_reasons
            .insert(String::from("missing occurrences"));
    }
    if let Some(value) = raw.object.get("priority") {
        definition.priority = integer(value, "priority", &raw.source)?;
    }
    if let Some(value) = raw.object.get("rotate") {
        definition.rotate = value.as_bool().ok_or_else(|| {
            OvermapSpecialRegistryError::Invalid(format!("{} rotate must be boolean", raw.source))
        })?;
    }
    if let Some(value) = raw.object.get("flags") {
        definition.flags = string_set(value, "flags", &raw.source)?;
    }
    if let Some(value) = raw.object.get("spawns") {
        definition.monster_spawn = Some(parse_monster_spawn(value, &raw.source)?);
    }
    for field in raw.object.keys() {
        if !field.starts_with("//") && !SPECIAL_FIELDS.contains(&field.as_str()) {
            definition
                .unsupported_reasons
                .insert(format!("unsupported root field {field}"));
        }
    }
    for field in ["eoc", "extend", "delete", "relative", "proportional"] {
        if raw.object.contains_key(field) {
            definition
                .unsupported_reasons
                .insert(format!("unsupported {field} semantics"));
        }
    }
    if definition.flags.contains("BLOB") {
        definition
            .unsupported_reasons
            .insert(String::from("blob placement"));
    }
    if definition.flags.contains("CITY_UNIQUE") {
        definition
            .unsupported_reasons
            .insert(String::from("city-unique ownership"));
    }
    for terrain in &mut definition.terrains {
        if terrain.locations.is_empty() {
            terrain.locations.clone_from(&definition.default_locations);
        }
        if terrain.locations.is_empty() {
            definition.unsupported_reasons.insert(format!(
                "point {:?} has no location predicate",
                terrain.point
            ));
        }
        for location in &terrain.locations {
            if !locations.contains_key(location) {
                definition
                    .unsupported_reasons
                    .insert(format!("unknown overmap location {location}"));
            }
        }
    }
    let unique_points = definition
        .terrains
        .iter()
        .map(|terrain| terrain.point)
        .collect::<BTreeSet<_>>();
    if unique_points.len() != definition.terrains.len() {
        definition
            .unsupported_reasons
            .insert(String::from("duplicate fixed terrain points"));
    }
    definitions.insert(id.to_owned(), definition);
    Ok(true)
}

fn parse_monster_spawn(
    value: &Value,
    source: &str,
) -> Result<OvermapSpecialMonsterSpawnDefinition, OvermapSpecialRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_field(source, "spawns"))?;
    if object
        .keys()
        .any(|field| !matches!(field.as_str(), "group" | "population" | "radius" | "//"))
    {
        return Err(invalid_field(source, "spawns"));
    }
    let monster_group = required_id(object, "group", source)?.to_owned();
    let population = parse_interval(
        object
            .get("population")
            .ok_or_else(|| invalid_field(source, "spawns.population"))?,
        "spawns.population",
        false,
        source,
    )?;
    let radius = parse_interval(
        object
            .get("radius")
            .ok_or_else(|| invalid_field(source, "spawns.radius"))?,
        "spawns.radius",
        false,
        source,
    )?;
    if population.minimum < 0
        || population.maximum > MAX_SPECIAL_SPAWN_POPULATION
        || radius.minimum < 0
        || radius.maximum > MAX_SPECIAL_SPAWN_RADIUS
    {
        return Err(invalid_field(source, "spawns"));
    }
    Ok(OvermapSpecialMonsterSpawnDefinition {
        monster_group,
        population,
        radius,
    })
}

fn parse_terrains(
    value: &Value,
    source: &str,
    unsupported: &mut BTreeSet<String>,
) -> Result<Vec<OvermapSpecialTerrainDefinition>, OvermapSpecialRegistryError> {
    let Some(values) = value.as_array() else {
        unsupported.insert(String::from("non-fixed overmaps object"));
        return Ok(Vec::new());
    };
    if values.len() > MAX_SPECIAL_PARTS {
        return Err(OvermapSpecialRegistryError::LimitExceeded);
    }
    values
        .iter()
        .map(|value| {
            let object = value.as_object().ok_or_else(|| {
                OvermapSpecialRegistryError::Invalid(format!(
                    "{source} overmaps entries must be objects"
                ))
            })?;
            for field in object.keys() {
                if !matches!(
                    field.as_str(),
                    "point" | "overmap" | "locations" | "flags" | "camp" | "camp_name" | "//"
                ) {
                    unsupported.insert(format!("unsupported overmap entry field {field}"));
                }
            }
            if object.contains_key("camp") || object.contains_key("camp_name") {
                unsupported.insert(String::from("faction-camp placement"));
            }
            Ok(OvermapSpecialTerrainDefinition {
                point: point(
                    object.get("point").ok_or_else(|| {
                        OvermapSpecialRegistryError::Invalid(format!(
                            "{source} overmap entry requires point"
                        ))
                    })?,
                    source,
                )?,
                overmap: object
                    .get("overmap")
                    .map(|value| required_text_value(value, "overmap", source).map(str::to_owned))
                    .transpose()?,
                locations: object
                    .get("locations")
                    .map(|value| string_set(value, "locations", source))
                    .transpose()?
                    .unwrap_or_default(),
                flags: object
                    .get("flags")
                    .map(|value| string_set(value, "flags", source))
                    .transpose()?
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn parse_connections(
    value: &Value,
    source: &str,
) -> Result<Vec<OvermapSpecialConnectionDefinition>, OvermapSpecialRegistryError> {
    let values = value.as_array().ok_or_else(|| {
        OvermapSpecialRegistryError::Invalid(format!("{source} connections must be an array"))
    })?;
    if values.len() > MAX_SPECIAL_CONNECTIONS {
        return Err(OvermapSpecialRegistryError::LimitExceeded);
    }
    values
        .iter()
        .map(|value| {
            let object = value.as_object().ok_or_else(|| {
                OvermapSpecialRegistryError::Invalid(format!(
                    "{source} connection entries must be objects"
                ))
            })?;
            if let Some(field) = object.keys().find(|field| {
                !matches!(
                    field.as_str(),
                    "point" | "from" | "terrain" | "connection" | "existing" | "//"
                )
            }) {
                return Err(OvermapSpecialRegistryError::Invalid(format!(
                    "{source} connection has unsupported field {field}"
                )));
            }
            Ok(OvermapSpecialConnectionDefinition {
                point: point(
                    object.get("point").ok_or_else(|| {
                        OvermapSpecialRegistryError::Invalid(format!(
                            "{source} connection requires point"
                        ))
                    })?,
                    source,
                )?,
                from: object
                    .get("from")
                    .map(|value| point(value, source))
                    .transpose()?,
                terrain: object
                    .get("terrain")
                    .map(|value| required_text_value(value, "terrain", source).map(str::to_owned))
                    .transpose()?,
                connection: object
                    .get("connection")
                    .map(|value| {
                        required_text_value(value, "connection", source).map(str::to_owned)
                    })
                    .transpose()?,
                existing: object
                    .get("existing")
                    .map(|value| {
                        value.as_bool().ok_or_else(|| {
                            OvermapSpecialRegistryError::Invalid(format!(
                                "{source} connection existing must be boolean"
                            ))
                        })
                    })
                    .transpose()?
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn parse_interval(
    value: &Value,
    field: &str,
    negative_one_is_unbounded: bool,
    source: &str,
) -> Result<OvermapSpecialInterval, OvermapSpecialRegistryError> {
    let (minimum, mut maximum) = if let Some(value) = value.as_i64() {
        let value = i32::try_from(value).map_err(|_| invalid_field(source, field))?;
        (value, value)
    } else {
        let values = value
            .as_array()
            .ok_or_else(|| invalid_field(source, field))?;
        if values.len() != 2 {
            return Err(invalid_field(source, field));
        }
        (
            integer(&values[0], field, source)?,
            integer(&values[1], field, source)?,
        )
    };
    if negative_one_is_unbounded && maximum == -1 {
        maximum = i32::MAX;
    }
    if minimum > maximum {
        return Err(invalid_field(source, field));
    }
    Ok(OvermapSpecialInterval { minimum, maximum })
}

fn point(value: &Value, source: &str) -> Result<[i32; 3], OvermapSpecialRegistryError> {
    let values = value.as_array().ok_or_else(|| {
        OvermapSpecialRegistryError::Invalid(format!("{source} point must be an integer triple"))
    })?;
    if values.len() != 3 {
        return Err(OvermapSpecialRegistryError::Invalid(format!(
            "{source} point must be an integer triple"
        )));
    }
    Ok([
        integer(&values[0], "point", source)?,
        integer(&values[1], "point", source)?,
        integer(&values[2], "point", source)?,
    ])
}

fn integer(value: &Value, field: &str, source: &str) -> Result<i32, OvermapSpecialRegistryError> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid_field(source, field))
}

fn string_set(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<BTreeSet<String>, OvermapSpecialRegistryError> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid_field(source, field))?;
    if values.len() > MAX_SPECIAL_FLAGS {
        return Err(OvermapSpecialRegistryError::LimitExceeded);
    }
    values
        .iter()
        .map(|value| required_text_value(value, field, source).map(str::to_owned))
        .collect()
}

fn required_id<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, OvermapSpecialRegistryError> {
    object
        .get(field)
        .ok_or_else(|| invalid_field(source, field))
        .and_then(|value| required_text_value(value, field, source))
}

fn required_text_value<'a>(
    value: &'a Value,
    field: &str,
    source: &str,
) -> Result<&'a str, OvermapSpecialRegistryError> {
    let value = value.as_str().ok_or_else(|| invalid_field(source, field))?;
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control) {
        return Err(invalid_field(source, field));
    }
    Ok(value)
}

fn invalid_field(source: &str, field: &str) -> OvermapSpecialRegistryError {
    OvermapSpecialRegistryError::Invalid(format!("{source} has invalid {field}"))
}

#[derive(Debug)]
pub enum OvermapSpecialRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Invalid(String),
    LimitExceeded,
}

impl fmt::Display for OvermapSpecialRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => {
                write!(formatter, "overmap-special mod selection failed: {error}")
            }
            Self::Io(path, error) => {
                write!(formatter, "overmap-special I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "overmap-special JSON failed for {path}: {error}")
            }
            Self::Invalid(reason) => write!(formatter, "invalid overmap-special content: {reason}"),
            Self::LimitExceeded => {
                write!(formatter, "overmap-special content exceeds a bounded limit")
            }
        }
    }
}

impl std::error::Error for OvermapSpecialRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Io(_, error) => Some(error),
            Self::Json(_, error) => Some(error),
            Self::Invalid(_) | Self::LimitExceeded => None,
        }
    }
}
