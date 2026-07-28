use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{
    BashDefinition, ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile,
    bash::apply_bash_definition,
};

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "name",
    "description",
    "symbol",
    "color",
    "move_cost",
    "flags",
    "open",
    "close",
    "looks_like",
    "bash",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerrainDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub symbol: String,
    pub colors: Vec<String>,
    pub move_cost: i32,
    pub flags: BTreeSet<String>,
    pub open: String,
    pub close: String,
    pub looks_like: String,
    pub bash: Option<BashDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

impl TerrainDefinition {
    #[must_use]
    pub const fn is_passable(&self) -> bool {
        self.move_cost > 0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerrainRegistry {
    terrain: BTreeMap<String, TerrainDefinition>,
    abstract_count: usize,
}

#[derive(Clone)]
struct RawTerrain {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl TerrainRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, TerrainRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(TerrainRegistryError::Catalog)?;
        let mut pending = read_terrain(content_root.as_ref(), files)?;
        let mut terrain = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(TerrainRegistryError::InternalQueue)?;
                if load_one(&raw, &mut terrain, &mut abstracts)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(TerrainRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| definition_key(&raw.object).ok())
                        .map(|(id, _)| id.to_owned())
                        .collect(),
                ));
            }
        }
        validate_transform_references(&terrain)?;
        Ok(Self {
            terrain,
            abstract_count: abstracts.len(),
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.terrain.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty()
    }

    #[must_use]
    pub fn abstract_count(&self) -> usize {
        self.abstract_count
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&TerrainDefinition> {
        self.terrain.get(id)
    }
}

fn read_terrain(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawTerrain>, TerrainRegistryError> {
    let mut terrain = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| TerrainRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| TerrainRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_terrain(&file, value, &mut terrain)?;
                }
            }
            value => collect_terrain(&file, value, &mut terrain)?,
        }
    }
    Ok(terrain)
}

fn collect_terrain(
    file: &SelectedContentFile,
    value: Value,
    terrain: &mut VecDeque<RawTerrain>,
) -> Result<(), TerrainRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("terrain") {
        return Ok(());
    }
    terrain.push_back(RawTerrain {
        file: file.clone(),
        object: value
            .as_object()
            .cloned()
            .ok_or_else(|| TerrainRegistryError::InvalidDefinition(file.upstream_path.clone()))?,
    });
    Ok(())
}

fn load_one(
    raw: &RawTerrain,
    terrain: &mut BTreeMap<String, TerrainDefinition>,
    abstracts: &mut BTreeMap<String, TerrainDefinition>,
) -> Result<bool, TerrainRegistryError> {
    let (id, is_abstract) = definition_key(&raw.object)?;
    let parent = optional_string(&raw.object, "copy-from", &raw.file.upstream_path)?;
    let mut definition = if let Some(parent) = parent {
        let Some(base) = terrain.get(parent).or_else(|| abstracts.get(parent)) else {
            return Ok(false);
        };
        base.clone()
    } else {
        TerrainDefinition::default()
    };
    definition.id = id.to_owned();
    definition.source.clone_from(&raw.file.upstream_path);
    let source = format!("{}#{id}", raw.file.upstream_path);
    apply_fields(&mut definition, &raw.object, &source)?;
    if !is_abstract
        && (definition.name.is_empty() || definition.symbol.is_empty() || definition.move_cost < -1)
    {
        return Err(TerrainRegistryError::InvalidFinalizedTerrain {
            id: id.to_owned(),
            source: raw.file.upstream_path.clone(),
        });
    }
    if is_abstract {
        abstracts.insert(id.to_owned(), definition);
    } else {
        terrain.insert(id.to_owned(), definition);
    }
    Ok(true)
}

fn definition_key(object: &Map<String, Value>) -> Result<(&str, bool), TerrainRegistryError> {
    match (object.get("id"), object.get("abstract")) {
        (Some(Value::String(id)), None) if !id.is_empty() => Ok((id, false)),
        (None, Some(Value::String(id))) if !id.is_empty() => Ok((id, true)),
        _ => Err(TerrainRegistryError::InvalidIdentity),
    }
}

fn apply_fields(
    terrain: &mut TerrainDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), TerrainRegistryError> {
    apply_text(object, "name", &mut terrain.name, source)?;
    apply_text(object, "description", &mut terrain.description, source)?;
    for (field, target) in [
        ("symbol", &mut terrain.symbol),
        ("open", &mut terrain.open),
        ("close", &mut terrain.close),
        ("looks_like", &mut terrain.looks_like),
    ] {
        apply_string(object, field, target, source)?;
    }
    apply_string_choices(object, "color", &mut terrain.colors, source)?;
    apply_integer(object, "move_cost", &mut terrain.move_cost, source)?;
    apply_string_set(object, "flags", &mut terrain.flags, source)?;
    if let Some(value) = object.get("bash") {
        apply_bash_definition(&mut terrain.bash, value, source)
            .map_err(|_| invalid(source, "bash"))?;
    }
    for field in object.keys() {
        if !field.starts_with("//")
            && !IMPLEMENTED_FIELDS.contains(&field.as_str())
            && !matches!(
                field.as_str(),
                "extend" | "delete" | "relative" | "proportional"
            )
        {
            terrain.unsupported_fields.insert(field.clone());
        }
    }
    for modifier_name in ["extend", "delete", "relative", "proportional"] {
        if let Some(Value::Object(fields)) = object.get(modifier_name) {
            for field in fields.keys() {
                if !IMPLEMENTED_FIELDS.contains(&field.as_str()) {
                    terrain.unsupported_fields.insert(field.clone());
                }
            }
        }
    }
    Ok(())
}

fn apply_integer(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    source: &str,
) -> Result<(), TerrainRegistryError> {
    if let Some(value) = object.get(field) {
        *target = i32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))?;
    } else if let Some(value) = modifier(object, "proportional", field, source)? {
        let multiplier = value.as_f64().ok_or_else(|| invalid(source, field))?;
        let adjusted = f64::from(*target) * multiplier;
        if !adjusted.is_finite() || adjusted < f64::from(i32::MIN) || adjusted > f64::from(i32::MAX)
        {
            return Err(invalid(source, field));
        }
        *target = adjusted.round() as i32;
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        let addition = i32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))?;
        *target = target
            .checked_add(addition)
            .ok_or_else(|| invalid(source, field))?;
    }
    if *target < -1 {
        return Err(invalid(source, field));
    }
    Ok(())
}

fn apply_text(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), TerrainRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    *target = match value {
        Value::String(value) => value.clone(),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .map(str::to_owned)
            .ok_or_else(|| invalid(source, field))?,
        _ => return Err(invalid(source, field)),
    };
    Ok(())
}

fn apply_string(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), TerrainRegistryError> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_str()
            .ok_or_else(|| invalid(source, field))?
            .to_owned();
    }
    Ok(())
}

fn apply_string_choices(
    object: &Map<String, Value>,
    field: &str,
    target: &mut Vec<String>,
    source: &str,
) -> Result<(), TerrainRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    *target = match value {
        Value::String(value) => vec![value.clone()],
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(source, field))
            })
            .collect::<Result<_, _>>()?,
        _ => return Err(invalid(source, field)),
    };
    if target.is_empty() {
        return Err(invalid(source, field));
    }
    Ok(())
}

fn apply_string_set(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), TerrainRegistryError> {
    if let Some(value) = object.get(field) {
        *target = string_set(value, source, field)?;
    }
    if let Some(value) = modifier(object, "extend", field, source)? {
        target.extend(string_set(value, source, field)?);
    }
    if let Some(value) = modifier(object, "delete", field, source)? {
        for entry in string_set(value, source, field)? {
            target.remove(&entry);
        }
    }
    Ok(())
}

fn string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, TerrainRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid(source, field))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .collect()
}

fn modifier<'a>(
    object: &'a Map<String, Value>,
    modifier_name: &str,
    field: &str,
    source: &str,
) -> Result<Option<&'a Value>, TerrainRegistryError> {
    match object.get(modifier_name) {
        None => Ok(None),
        Some(Value::Object(values)) => Ok(values.get(field)),
        Some(_) => Err(invalid(source, modifier_name)),
    }
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<&'a str>, TerrainRegistryError> {
    object
        .get(field)
        .map(|value| value.as_str().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn validate_transform_references(
    terrain: &BTreeMap<String, TerrainDefinition>,
) -> Result<(), TerrainRegistryError> {
    for definition in terrain.values() {
        for (field, target) in [("open", &definition.open), ("close", &definition.close)] {
            if !target.is_empty() && !terrain.contains_key(target) {
                return Err(TerrainRegistryError::MissingTransform {
                    id: definition.id.clone(),
                    field,
                    target: target.clone(),
                });
            }
        }
    }
    Ok(())
}

fn invalid(source: &str, field: &str) -> TerrainRegistryError {
    TerrainRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum TerrainRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField {
        source: String,
        field: String,
    },
    InvalidFinalizedTerrain {
        id: String,
        source: String,
    },
    InvalidIdentity,
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    MissingTransform {
        id: String,
        field: &'static str,
        target: String,
    },
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for TerrainRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "terrain mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("internal terrain load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "terrain definition is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid terrain field {field} in {source}")
            }
            Self::InvalidFinalizedTerrain { id, source } => {
                write!(
                    formatter,
                    "terrain {id} is incomplete after inheritance in {source}"
                )
            }
            Self::InvalidIdentity => {
                formatter.write_str("terrain must have exactly one non-empty id or abstract")
            }
            Self::Io(path, error) => {
                write!(formatter, "terrain registry I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "terrain registry JSON failed for {path}: {error}"
                )
            }
            Self::MissingTransform { id, field, target } => {
                write!(
                    formatter,
                    "terrain {id} has missing {field} target {target}"
                )
            }
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved or cyclic terrain inheritance: {ids:?}"
                )
            }
        }
    }
}

impl std::error::Error for TerrainRegistryError {}
