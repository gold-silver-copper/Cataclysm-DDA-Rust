use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_START_LOCATIONS: usize = 4_096;
pub const MAX_START_LOCATION_TARGETS: usize = 256;
pub const MAX_START_LOCATION_FLAGS: usize = 64;
pub const MAX_START_LOCATION_PARAMETERS: usize = 64;
pub const MAX_START_LOCATION_ID_BYTES: usize = 512;
pub const MAX_START_LOCATION_NAME_BYTES: usize = 4_096;
pub const MAX_START_LOCATION_PARAMETER_BYTES: usize = 512;
pub const DEFAULT_START_LOCATION_MIN_Z: i32 = -10;
pub const DEFAULT_START_LOCATION_MAX_Z: i32 = 10;

const ROOT_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "name",
    "terrain",
    "city_sizes",
    "city_distance",
    "allowed_z_levels",
    "flags",
    "extend",
    "delete",
    "//",
];
const TARGET_FIELDS: &[&str] = &["om_terrain", "om_terrain_match_type", "parameters"];
const FLAG_PATCH_FIELDS: &[&str] = &["flags"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OvermapTerrainMatchType {
    Exact,
    Type,
    Subtype,
    Prefix,
    Contains,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartLocationTarget {
    pub overmap_terrain: String,
    pub match_type: OvermapTerrainMatchType,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InclusiveI32Interval {
    pub minimum: i32,
    pub maximum: i32,
}

impl InclusiveI32Interval {
    #[must_use]
    pub const fn contains(self, value: i32) -> bool {
        value >= self.minimum && value <= self.maximum
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartLocationDefinition {
    pub id: String,
    pub name: String,
    /// Source order is observable because upstream selects one target uniformly.
    pub targets: Vec<StartLocationTarget>,
    pub city_sizes: InclusiveI32Interval,
    pub city_distance: InclusiveI32Interval,
    pub allowed_z_levels: InclusiveI32Interval,
    pub flags: BTreeSet<String>,
}

impl Default for StartLocationDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            targets: Vec::new(),
            city_sizes: InclusiveI32Interval {
                minimum: 0,
                maximum: i32::MAX,
            },
            city_distance: InclusiveI32Interval {
                minimum: 0,
                maximum: i32::MAX,
            },
            allowed_z_levels: InclusiveI32Interval {
                minimum: DEFAULT_START_LOCATION_MIN_Z,
                maximum: DEFAULT_START_LOCATION_MAX_Z,
            },
            flags: BTreeSet::new(),
        }
    }
}

impl StartLocationDefinition {
    #[must_use]
    pub fn requires_city(&self) -> bool {
        // Exact pinned split between the city-origin and point-origin search
        // paths. A positive minimum distance alone (as in `sloc_road`) does
        // not require a city and is intentionally ignored by point-origin
        // selection.
        self.city_sizes.minimum > 0 || self.city_distance.maximum < 180
    }

    /// Selection can run before city generation only when city constraints and
    /// mapgen parameters are absent and every placement flag is already
    /// represented. `ALLOW_OUTSIDE` only relaxes the indoor-tile requirement;
    /// the canonical first-available selector already accepts outdoor tiles.
    #[must_use]
    pub fn is_runtime_selectable_without_cities(&self) -> bool {
        !self.requires_city()
            && self.allowed_z_levels.contains(0)
            && self.flags.iter().all(|flag| flag == "ALLOW_OUTSIDE")
            && self
                .targets
                .iter()
                .all(|target| target.parameters.is_empty())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StartLocationRegistry {
    definitions: BTreeMap<String, StartLocationDefinition>,
}

impl StartLocationRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, StartLocationRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(StartLocationRegistryError::Catalog)?;
        compile_registry(content_root.as_ref(), files)
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
    pub fn get(&self, id: &str) -> Option<&StartLocationDefinition> {
        self.definitions.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &StartLocationDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

#[derive(Debug)]
pub enum StartLocationRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Invalid(String),
    LimitExceeded(&'static str, usize),
}

impl fmt::Display for StartLocationRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => {
                write!(formatter, "selected start-location catalog failed: {error}")
            }
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "failed to read start-location file {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "failed to parse start-location JSON {path}: {error}"
                )
            }
            Self::Invalid(reason) => {
                write!(formatter, "invalid start-location definition: {reason}")
            }
            Self::LimitExceeded(kind, limit) => {
                write!(
                    formatter,
                    "selected start-location {kind} exceeds limit {limit}"
                )
            }
        }
    }
}

impl std::error::Error for StartLocationRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Io(_, error) => Some(error),
            Self::Json(_, error) => Some(error),
            Self::Invalid(_) | Self::LimitExceeded(_, _) => None,
        }
    }
}

fn compile_registry(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<StartLocationRegistry, StartLocationRegistryError> {
    let mut pending = Vec::<(Value, String)>::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| StartLocationRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| StartLocationRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for (index, value) in values.into_iter().enumerate() {
                    if is_start_location_value(&value) {
                        pending.push((value, format!("{}#{index}", file.upstream_path)));
                    }
                }
            }
            value if is_start_location_value(&value) => {
                pending.push((value, file.upstream_path.clone()));
            }
            _ => {}
        }
    }
    let mut definitions = BTreeMap::<String, StartLocationDefinition>::new();
    loop {
        let mut deferred = Vec::new();
        let mut compiled = 0_usize;
        for (value, source) in pending {
            match compile_value(&value, &source, &mut definitions)? {
                CompileOutcome::Ignored => {}
                CompileOutcome::Compiled => compiled += 1,
                CompileOutcome::Deferred => deferred.push((value, source)),
            }
        }
        if deferred.is_empty() {
            break;
        }
        if compiled == 0 {
            let sources = deferred
                .iter()
                .take(8)
                .map(|(_value, source)| source.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(StartLocationRegistryError::Invalid(format!(
                "unresolved or cyclic copy-from definitions: {sources}"
            )));
        }
        pending = deferred;
    }
    Ok(StartLocationRegistry { definitions })
}

fn is_start_location_value(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("start_location")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompileOutcome {
    Ignored,
    Compiled,
    Deferred,
}

fn compile_value(
    value: &Value,
    source: &str,
    definitions: &mut BTreeMap<String, StartLocationDefinition>,
) -> Result<CompileOutcome, StartLocationRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(CompileOutcome::Ignored);
    };
    if object.get("type").and_then(Value::as_str) != Some("start_location") {
        return Ok(CompileOutcome::Ignored);
    }
    reject_unknown_fields(object, ROOT_FIELDS, source)?;
    let id = required_bounded_string(object.get("id"), "id", MAX_START_LOCATION_ID_BYTES, source)?;
    let copy_from = object
        .get("copy-from")
        .map(|value| bounded_string(value, "copy-from", MAX_START_LOCATION_ID_BYTES, source))
        .transpose()?;
    let previous = definitions.get(&id).cloned();
    let inherited = match copy_from.as_deref() {
        Some(parent) if parent == id => {
            let Some(previous) = previous.clone() else {
                return Ok(CompileOutcome::Deferred);
            };
            previous
        }
        Some(parent) => {
            let Some(parent) = definitions.get(parent).cloned() else {
                return Ok(CompileOutcome::Deferred);
            };
            parent
        }
        None => previous.unwrap_or_default(),
    };
    let first_load = inherited.id.is_empty();
    let mut definition = inherited;
    definition.id.clone_from(&id);

    if let Some(name) = object.get("name") {
        definition.name = parse_name(name, source)?;
    } else if first_load {
        return Err(StartLocationRegistryError::Invalid(format!(
            "{source} start location {id:?} has no name"
        )));
    }
    if let Some(targets) = object.get("terrain") {
        definition.targets = parse_targets(targets, source)?;
    } else if first_load {
        return Err(StartLocationRegistryError::Invalid(format!(
            "{source} start location {id:?} has no terrain"
        )));
    }
    if let Some(interval) = object.get("city_sizes") {
        definition.city_sizes = parse_interval(interval, "city_sizes", source)?;
    }
    if let Some(interval) = object.get("city_distance") {
        definition.city_distance = parse_interval(interval, "city_distance", source)?;
    }
    if let Some(interval) = object.get("allowed_z_levels") {
        definition.allowed_z_levels = parse_interval(interval, "allowed_z_levels", source)?;
    }
    if let Some(flags) = object.get("flags") {
        definition.flags = parse_flags(flags, "flags", source)?;
    }
    apply_flag_patch(object.get("extend"), true, &mut definition.flags, source)?;
    apply_flag_patch(object.get("delete"), false, &mut definition.flags, source)?;
    if definition.targets.is_empty() {
        return Err(StartLocationRegistryError::Invalid(format!(
            "{source} start location {id:?} has no terrain targets"
        )));
    }
    if !definitions.contains_key(&id) && definitions.len() >= MAX_START_LOCATIONS {
        return Err(StartLocationRegistryError::LimitExceeded(
            "definitions",
            MAX_START_LOCATIONS,
        ));
    }
    definitions.insert(id, definition);
    Ok(CompileOutcome::Compiled)
}

fn parse_name(value: &Value, source: &str) -> Result<String, StartLocationRegistryError> {
    let text = match value {
        Value::String(text) => text,
        Value::Object(object) => object.get("str").and_then(Value::as_str).ok_or_else(|| {
            StartLocationRegistryError::Invalid(format!(
                "{source} translated name must contain a string str member"
            ))
        })?,
        _ => {
            return Err(StartLocationRegistryError::Invalid(format!(
                "{source} name must be a string or translation object"
            )));
        }
    };
    validate_bounded_text(text, "name", MAX_START_LOCATION_NAME_BYTES, source)?;
    Ok(text.to_owned())
}

fn parse_targets(
    value: &Value,
    source: &str,
) -> Result<Vec<StartLocationTarget>, StartLocationRegistryError> {
    let values = value.as_array().ok_or_else(|| {
        StartLocationRegistryError::Invalid(format!("{source} terrain must be an array"))
    })?;
    if values.is_empty() || values.len() > MAX_START_LOCATION_TARGETS {
        return Err(StartLocationRegistryError::LimitExceeded(
            "targets per definition",
            MAX_START_LOCATION_TARGETS,
        ));
    }
    values
        .iter()
        .map(|value| match value {
            Value::String(id) => {
                validate_bounded_text(id, "om_terrain", MAX_START_LOCATION_ID_BYTES, source)?;
                Ok(StartLocationTarget {
                    overmap_terrain: id.clone(),
                    match_type: OvermapTerrainMatchType::Type,
                    parameters: BTreeMap::new(),
                })
            }
            Value::Object(object) => parse_target_object(object, source),
            _ => Err(StartLocationRegistryError::Invalid(format!(
                "{source} terrain entries must be strings or objects"
            ))),
        })
        .collect()
}

fn parse_target_object(
    object: &Map<String, Value>,
    source: &str,
) -> Result<StartLocationTarget, StartLocationRegistryError> {
    reject_unknown_fields(object, TARGET_FIELDS, source)?;
    let overmap_terrain = required_bounded_string(
        object.get("om_terrain"),
        "om_terrain",
        MAX_START_LOCATION_ID_BYTES,
        source,
    )?;
    let match_type = match object
        .get("om_terrain_match_type")
        .map(|value| bounded_string(value, "om_terrain_match_type", 32, source))
        .transpose()?
        .as_deref()
        .unwrap_or("TYPE")
    {
        "EXACT" => OvermapTerrainMatchType::Exact,
        "TYPE" => OvermapTerrainMatchType::Type,
        "SUBTYPE" => OvermapTerrainMatchType::Subtype,
        "PREFIX" => OvermapTerrainMatchType::Prefix,
        "CONTAINS" => OvermapTerrainMatchType::Contains,
        other => {
            return Err(StartLocationRegistryError::Invalid(format!(
                "{source} has unknown om_terrain_match_type {other:?}"
            )));
        }
    };
    let parameters = object
        .get("parameters")
        .map(|value| parse_parameters(value, source))
        .transpose()?
        .unwrap_or_default();
    Ok(StartLocationTarget {
        overmap_terrain,
        match_type,
        parameters,
    })
}

fn parse_parameters(
    value: &Value,
    source: &str,
) -> Result<BTreeMap<String, String>, StartLocationRegistryError> {
    let object = value.as_object().ok_or_else(|| {
        StartLocationRegistryError::Invalid(format!("{source} parameters must be an object"))
    })?;
    if object.len() > MAX_START_LOCATION_PARAMETERS {
        return Err(StartLocationRegistryError::LimitExceeded(
            "parameters per target",
            MAX_START_LOCATION_PARAMETERS,
        ));
    }
    object
        .iter()
        .map(|(key, value)| {
            validate_bounded_text(
                key,
                "parameter key",
                MAX_START_LOCATION_PARAMETER_BYTES,
                source,
            )?;
            let value = bounded_string(
                value,
                "parameter value",
                MAX_START_LOCATION_PARAMETER_BYTES,
                source,
            )?;
            Ok((key.clone(), value))
        })
        .collect()
}

fn parse_interval(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<InclusiveI32Interval, StartLocationRegistryError> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| {
            StartLocationRegistryError::Invalid(format!(
                "{source} {field} must be a two-integer array"
            ))
        })?;
    let minimum = values[0]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| {
            StartLocationRegistryError::Invalid(format!("{source} {field} minimum must be an i32"))
        })?;
    let raw_maximum = values[1]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| {
            StartLocationRegistryError::Invalid(format!("{source} {field} maximum must be an i32"))
        })?;
    let maximum = if raw_maximum < minimum {
        if raw_maximum >= 0 {
            return Err(StartLocationRegistryError::Invalid(format!(
                "{source} {field} maximum is below its minimum"
            )));
        }
        i32::MAX
    } else {
        raw_maximum
    };
    Ok(InclusiveI32Interval { minimum, maximum })
}

fn parse_flags(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<BTreeSet<String>, StartLocationRegistryError> {
    let values = value.as_array().ok_or_else(|| {
        StartLocationRegistryError::Invalid(format!("{source} {field} must be an array"))
    })?;
    if values.len() > MAX_START_LOCATION_FLAGS {
        return Err(StartLocationRegistryError::LimitExceeded(
            "flags per definition",
            MAX_START_LOCATION_FLAGS,
        ));
    }
    values
        .iter()
        .map(|value| bounded_string(value, field, MAX_START_LOCATION_ID_BYTES, source))
        .collect()
}

fn apply_flag_patch(
    value: Option<&Value>,
    extend: bool,
    flags: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), StartLocationRegistryError> {
    let Some(value) = value else {
        return Ok(());
    };
    let object = value.as_object().ok_or_else(|| {
        StartLocationRegistryError::Invalid(format!(
            "{source} {} must be an object",
            if extend { "extend" } else { "delete" }
        ))
    })?;
    reject_unknown_fields(object, FLAG_PATCH_FIELDS, source)?;
    let patch = object
        .get("flags")
        .map(|value| {
            parse_flags(
                value,
                if extend {
                    "extend.flags"
                } else {
                    "delete.flags"
                },
                source,
            )
        })
        .transpose()?
        .unwrap_or_default();
    if extend {
        flags.extend(patch);
    } else {
        flags.retain(|flag| !patch.contains(flag));
    }
    if flags.len() > MAX_START_LOCATION_FLAGS {
        return Err(StartLocationRegistryError::LimitExceeded(
            "flags per definition",
            MAX_START_LOCATION_FLAGS,
        ));
    }
    Ok(())
}

fn required_bounded_string(
    value: Option<&Value>,
    field: &str,
    limit: usize,
    source: &str,
) -> Result<String, StartLocationRegistryError> {
    bounded_string(
        value.ok_or_else(|| {
            StartLocationRegistryError::Invalid(format!("{source} is missing {field}"))
        })?,
        field,
        limit,
        source,
    )
}

fn bounded_string(
    value: &Value,
    field: &str,
    limit: usize,
    source: &str,
) -> Result<String, StartLocationRegistryError> {
    let value = value.as_str().ok_or_else(|| {
        StartLocationRegistryError::Invalid(format!("{source} {field} must be a string"))
    })?;
    validate_bounded_text(value, field, limit, source)?;
    Ok(value.to_owned())
}

fn validate_bounded_text(
    value: &str,
    field: &str,
    limit: usize,
    source: &str,
) -> Result<(), StartLocationRegistryError> {
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err(StartLocationRegistryError::Invalid(format!(
            "{source} {field} must be nonempty, non-control text within {limit} bytes"
        )));
    }
    Ok(())
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    source: &str,
) -> Result<(), StartLocationRegistryError> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(StartLocationRegistryError::Invalid(format!(
            "{source} contains unsupported field {field:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inheritance_intervals_targets_and_runtime_boundary_are_exact() {
        let values: Value = serde_json::from_str(
            r#"[
                {
                    "type": "start_location",
                    "id": "base",
                    "name": "Base",
                    "terrain": [
                        "road",
                        { "om_terrain": "shelter_north", "om_terrain_match_type": "EXACT" },
                        { "om_terrain": "road_straight", "om_terrain_match_type": "SUBTYPE" },
                        { "om_terrain": "forest", "om_terrain_match_type": "PREFIX" },
                        { "om_terrain": "rest_t", "om_terrain_match_type": "CONTAINS" }
                    ],
                    "city_distance": [10, -1],
                    "flags": ["ALLOW_OUTSIDE"]
                },
                {
                    "type": "start_location",
                    "id": "child",
                    "copy-from": "base",
                    "name": "Child",
                    "extend": { "flags": ["BOARDED"] },
                    "delete": { "flags": ["ALLOW_OUTSIDE"] }
                }
            ]"#,
        )
        .expect("fixture should parse");
        let mut definitions = BTreeMap::new();
        for (index, value) in values.as_array().expect("fixture array").iter().enumerate() {
            assert_eq!(
                compile_value(value, &format!("fixture#{index}"), &mut definitions)
                    .expect("fixture should compile"),
                CompileOutcome::Compiled
            );
        }
        let base = &definitions["base"];
        assert_eq!(base.city_distance.maximum, i32::MAX);
        assert_eq!(base.targets[0].match_type, OvermapTerrainMatchType::Type);
        assert_eq!(base.targets[1].match_type, OvermapTerrainMatchType::Exact);
        assert_eq!(base.targets[2].match_type, OvermapTerrainMatchType::Subtype);
        assert_eq!(base.targets[3].match_type, OvermapTerrainMatchType::Prefix);
        assert_eq!(
            base.targets[4].match_type,
            OvermapTerrainMatchType::Contains
        );
        let child = &definitions["child"];
        assert_eq!(child.targets, base.targets);
        assert_eq!(child.flags, BTreeSet::from([String::from("BOARDED")]));
        assert!(!child.is_runtime_selectable_without_cities());
    }

    #[test]
    fn pinned_default_start_locations_finalize_with_retained_parameters() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(crate::DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest should load");
        let root = manifest_path
            .parent()
            .expect("manifest should have a parent");
        let catalog = ModCatalog::load(&manifest, root).expect("mods should load");
        let enabled = catalog
            .recommended_new_world()
            .expect("default mods should resolve");
        let registry = StartLocationRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("start locations should finalize");

        assert_eq!(registry.len(), 101);
        let lmoe = registry.get("sloc_lmoe").expect("lmoe start should exist");
        assert!(lmoe.is_runtime_selectable_without_cities());
        assert_eq!(lmoe.targets.len(), 1);
        assert_eq!(lmoe.targets[0].overmap_terrain, "lmoe");
        assert_eq!(lmoe.targets[0].match_type, OvermapTerrainMatchType::Type);

        let field = registry
            .get("sloc_field")
            .expect("field start should exist");
        assert!(field.is_runtime_selectable_without_cities());
        assert_eq!(field.flags, BTreeSet::from([String::from("ALLOW_OUTSIDE")]));
        assert_eq!(field.targets.len(), 1);
        assert_eq!(field.targets[0].overmap_terrain, "field");

        let shelter = registry
            .get("sloc_shelter_safe")
            .expect("evac shelter start should exist");
        assert_eq!(
            shelter.targets[0].parameters,
            BTreeMap::from([(
                String::from("shelter_palette"),
                String::from("shelter_basic"),
            )])
        );
        assert!(!shelter.is_runtime_selectable_without_cities());

        let boarded = registry
            .get("sloc_house_boarded")
            .expect("boarded house should inherit");
        assert_eq!(boarded.name, "House (boarded up)");
        assert!(boarded.flags.contains("BOARDED"));
        assert_eq!(
            boarded.targets,
            registry
                .get("sloc_house")
                .expect("base house start should exist")
                .targets
        );

        let road = registry.get("sloc_road").expect("road start should exist");
        assert_eq!(road.city_distance.minimum, 10);
        assert_eq!(road.city_distance.maximum, i32::MAX);
        assert!(!road.requires_city());
    }
}
