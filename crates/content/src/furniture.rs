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
    "bgcolor",
    "move_cost_mod",
    "required_str",
    "coverage",
    "comfort",
    "floor_bedding_warmth",
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
pub struct FurnitureDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub symbol: String,
    pub colors: Vec<String>,
    pub move_cost_mod: i32,
    pub required_str: i32,
    pub coverage: i32,
    pub comfort: i32,
    pub floor_bedding_warmth: i32,
    pub flags: BTreeSet<String>,
    pub open: String,
    pub close: String,
    pub looks_like: String,
    pub bash: Option<BashDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
    has_move_cost_mod: bool,
    has_required_str: bool,
}

impl FurnitureDefinition {
    #[must_use]
    pub const fn is_passable(&self) -> bool {
        self.move_cost_mod >= 0
    }

    #[must_use]
    pub fn is_transparent(&self) -> bool {
        self.flags.contains("TRANSPARENT") && !self.flags.contains("TRANSLUCENT")
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FurnitureRegistry {
    furniture: BTreeMap<String, FurnitureDefinition>,
    abstract_count: usize,
}

#[derive(Clone)]
struct RawFurniture {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl FurnitureRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, FurnitureRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(FurnitureRegistryError::Catalog)?;
        let mut pending = read_furniture(content_root.as_ref(), files)?;
        let mut furniture = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(FurnitureRegistryError::InternalQueue)?;
                if load_one(&raw, &mut furniture, &mut abstracts)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(FurnitureRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| definition_key(&raw.object).ok())
                        .map(|(id, _)| id.to_owned())
                        .collect(),
                ));
            }
        }
        validate_transform_references(&furniture)?;
        Ok(Self {
            furniture,
            abstract_count: abstracts.len(),
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.furniture.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.furniture.is_empty()
    }

    #[must_use]
    pub fn abstract_count(&self) -> usize {
        self.abstract_count
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&FurnitureDefinition> {
        self.furniture.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &FurnitureDefinition> {
        self.furniture.values()
    }
}

fn read_furniture(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawFurniture>, FurnitureRegistryError> {
    let mut furniture = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| FurnitureRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| FurnitureRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_furniture(&file, value, &mut furniture)?;
                }
            }
            value => collect_furniture(&file, value, &mut furniture)?,
        }
    }
    Ok(furniture)
}

fn collect_furniture(
    file: &SelectedContentFile,
    value: Value,
    furniture: &mut VecDeque<RawFurniture>,
) -> Result<(), FurnitureRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("furniture") {
        return Ok(());
    }
    furniture.push_back(RawFurniture {
        file: file.clone(),
        object: value
            .as_object()
            .cloned()
            .ok_or_else(|| FurnitureRegistryError::InvalidDefinition(file.upstream_path.clone()))?,
    });
    Ok(())
}

fn load_one(
    raw: &RawFurniture,
    furniture: &mut BTreeMap<String, FurnitureDefinition>,
    abstracts: &mut BTreeMap<String, FurnitureDefinition>,
) -> Result<bool, FurnitureRegistryError> {
    let (id, is_abstract) = definition_key(&raw.object)?;
    let parent = optional_string(&raw.object, "copy-from", &raw.file.upstream_path)?;
    let mut definition = if let Some(parent) = parent {
        let Some(base) = furniture.get(parent).or_else(|| abstracts.get(parent)) else {
            return Ok(false);
        };
        base.clone()
    } else {
        FurnitureDefinition::default()
    };
    definition.id = id.to_owned();
    definition.source.clone_from(&raw.file.upstream_path);
    let source = format!("{}#{id}", raw.file.upstream_path);
    apply_fields(&mut definition, &raw.object, &source)?;
    if !is_abstract
        && (definition.name.is_empty()
            || definition.symbol.is_empty()
            || !definition.has_move_cost_mod
            || !definition.has_required_str)
    {
        return Err(FurnitureRegistryError::InvalidFinalizedFurniture {
            id: id.to_owned(),
            source: raw.file.upstream_path.clone(),
        });
    }
    if is_abstract {
        abstracts.insert(id.to_owned(), definition);
    } else {
        furniture.insert(id.to_owned(), definition);
    }
    Ok(true)
}

fn definition_key(object: &Map<String, Value>) -> Result<(&str, bool), FurnitureRegistryError> {
    match (object.get("id"), object.get("abstract")) {
        (Some(Value::String(id)), None) if !id.is_empty() => Ok((id, false)),
        (None, Some(Value::String(id))) if !id.is_empty() => Ok((id, true)),
        _ => Err(FurnitureRegistryError::InvalidIdentity),
    }
}

fn apply_fields(
    furniture: &mut FurnitureDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), FurnitureRegistryError> {
    apply_text(object, "name", &mut furniture.name, source)?;
    apply_text(object, "description", &mut furniture.description, source)?;
    for (field, target) in [
        ("symbol", &mut furniture.symbol),
        ("open", &mut furniture.open),
        ("close", &mut furniture.close),
        ("looks_like", &mut furniture.looks_like),
    ] {
        apply_string(object, field, target, source)?;
    }
    if object.contains_key("color") {
        apply_string_choices(object, "color", &mut furniture.colors, source)?;
    } else {
        apply_string_choices(object, "bgcolor", &mut furniture.colors, source)?;
    }
    if has_numeric_update(object, "move_cost_mod") {
        furniture.has_move_cost_mod = true;
    }
    if has_numeric_update(object, "required_str") {
        furniture.has_required_str = true;
    }
    for (field, target) in [
        ("move_cost_mod", &mut furniture.move_cost_mod),
        ("required_str", &mut furniture.required_str),
        ("coverage", &mut furniture.coverage),
        ("comfort", &mut furniture.comfort),
        ("floor_bedding_warmth", &mut furniture.floor_bedding_warmth),
    ] {
        apply_integer(object, field, target, source)?;
    }
    apply_string_set(object, "flags", &mut furniture.flags, source)?;
    if let Some(value) = object.get("bash") {
        apply_bash_definition(&mut furniture.bash, value, source)
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
            furniture.unsupported_fields.insert(field.clone());
        }
    }
    for modifier_name in ["extend", "delete", "relative", "proportional"] {
        if let Some(Value::Object(fields)) = object.get(modifier_name) {
            for field in fields.keys() {
                if !IMPLEMENTED_FIELDS.contains(&field.as_str()) {
                    furniture.unsupported_fields.insert(field.clone());
                }
            }
        }
    }
    Ok(())
}

fn has_numeric_update(object: &Map<String, Value>, field: &str) -> bool {
    object.contains_key(field)
        || ["relative", "proportional"].into_iter().any(|modifier| {
            object
                .get(modifier)
                .and_then(Value::as_object)
                .is_some_and(|fields| fields.contains_key(field))
        })
}

fn apply_integer(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    source: &str,
) -> Result<(), FurnitureRegistryError> {
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
    Ok(())
}

fn apply_text(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), FurnitureRegistryError> {
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
) -> Result<(), FurnitureRegistryError> {
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
) -> Result<(), FurnitureRegistryError> {
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
) -> Result<(), FurnitureRegistryError> {
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
) -> Result<BTreeSet<String>, FurnitureRegistryError> {
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
) -> Result<Option<&'a Value>, FurnitureRegistryError> {
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
) -> Result<Option<&'a str>, FurnitureRegistryError> {
    object
        .get(field)
        .map(|value| value.as_str().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn validate_transform_references(
    furniture: &BTreeMap<String, FurnitureDefinition>,
) -> Result<(), FurnitureRegistryError> {
    for definition in furniture.values() {
        for (field, target) in [
            ("open", definition.open.as_str()),
            ("close", definition.close.as_str()),
        ] {
            if !target.is_empty() && !furniture.contains_key(target) {
                return Err(FurnitureRegistryError::MissingTransform {
                    id: definition.id.clone(),
                    field,
                    target: target.to_owned(),
                });
            }
        }
    }
    Ok(())
}

fn invalid(source: &str, field: &str) -> FurnitureRegistryError {
    FurnitureRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum FurnitureRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField {
        source: String,
        field: String,
    },
    InvalidFinalizedFurniture {
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

impl fmt::Display for FurnitureRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "furniture mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("internal furniture load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(
                    formatter,
                    "furniture definition is not an object in {source}"
                )
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid furniture field {field} in {source}")
            }
            Self::InvalidFinalizedFurniture { id, source } => {
                write!(
                    formatter,
                    "furniture {id} is incomplete after inheritance in {source}"
                )
            }
            Self::InvalidIdentity => {
                formatter.write_str("furniture must have exactly one non-empty id or abstract")
            }
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "furniture registry I/O failed for {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "furniture registry JSON failed for {path}: {error}"
                )
            }
            Self::MissingTransform { id, field, target } => {
                write!(
                    formatter,
                    "furniture {id} has missing {field} target {target}"
                )
            }
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved or cyclic furniture inheritance: {ids:?}"
                )
            }
        }
    }
}

impl std::error::Error for FurnitureRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translucent_furniture_passes_light_but_blocks_binary_actor_sight() {
        let mut furniture = FurnitureDefinition::default();
        furniture.flags.insert(String::from("TRANSPARENT"));
        assert!(furniture.is_transparent());
        furniture.flags.insert(String::from("TRANSLUCENT"));
        assert!(!furniture.is_transparent());
    }
}
