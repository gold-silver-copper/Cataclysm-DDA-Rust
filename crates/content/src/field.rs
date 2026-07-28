use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "intensity_levels",
    "priority",
    "half_life",
    "linear_half_life",
    "is_splattering",
    "display_field",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldIntensityDefinition {
    pub name: String,
    pub symbol: String,
    pub color: String,
    pub dangerous: bool,
    pub transparent: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FieldTypeDefinition {
    pub id: String,
    pub intensity_levels: Vec<FieldIntensityDefinition>,
    pub priority: i32,
    pub half_life_seconds: u64,
    pub linear_half_life: bool,
    pub is_splattering: bool,
    pub display_field: bool,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FieldTypeRegistry {
    fields: BTreeMap<String, FieldTypeDefinition>,
}

#[derive(Clone)]
struct RawFieldType {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl FieldTypeRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, FieldTypeRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(FieldTypeRegistryError::Catalog)?;
        let mut pending = read_field_types(content_root.as_ref(), files)?;
        let mut fields = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(FieldTypeRegistryError::InternalQueue)?;
                if load_one(&raw, &mut fields)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(FieldTypeRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| raw.object.get("id").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect(),
                ));
            }
        }
        Ok(Self { fields })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&FieldTypeDefinition> {
        self.fields.get(id)
    }
}

fn read_field_types(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawFieldType>, FieldTypeRegistryError> {
    let mut fields = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| FieldTypeRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| FieldTypeRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_field_type(&file, value, &mut fields)?;
                }
            }
            value => collect_field_type(&file, value, &mut fields)?,
        }
    }
    Ok(fields)
}

fn collect_field_type(
    file: &SelectedContentFile,
    value: Value,
    fields: &mut VecDeque<RawFieldType>,
) -> Result<(), FieldTypeRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("field_type") {
        return Ok(());
    }
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| FieldTypeRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    // The zone UI deliberately reuses this discriminator. Its definitions do
    // not participate in the map field factory.
    if !object.contains_key("intensity_levels") && !object.contains_key("copy-from") {
        return Ok(());
    }
    fields.push_back(RawFieldType {
        file: file.clone(),
        object,
    });
    Ok(())
}

fn load_one(
    raw: &RawFieldType,
    fields: &mut BTreeMap<String, FieldTypeDefinition>,
) -> Result<bool, FieldTypeRegistryError> {
    let id = raw
        .object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or(FieldTypeRegistryError::InvalidIdentity)?;
    let parent = optional_string(&raw.object, "copy-from", &raw.file.upstream_path)?;
    let mut field = if let Some(parent) = parent {
        let Some(base) = fields.get(parent) else {
            return Ok(false);
        };
        base.clone()
    } else {
        FieldTypeDefinition::default()
    };
    field.id = id.to_owned();
    field.source.clone_from(&raw.file.upstream_path);
    let source = format!("{}#{id}", raw.file.upstream_path);
    apply_fields(&mut field, &raw.object, &source)?;
    if field.intensity_levels.is_empty() {
        return Err(FieldTypeRegistryError::InvalidFinalizedField {
            id: id.to_owned(),
            source: raw.file.upstream_path.clone(),
        });
    }
    fields.insert(id.to_owned(), field);
    Ok(true)
}

fn apply_fields(
    field: &mut FieldTypeDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), FieldTypeRegistryError> {
    if let Some(levels) = object.get("intensity_levels") {
        for level in levels
            .as_array()
            .ok_or_else(|| invalid(source, "intensity_levels"))?
        {
            let object = level
                .as_object()
                .ok_or_else(|| invalid(source, "intensity_levels"))?;
            let fallback =
                field
                    .intensity_levels
                    .last()
                    .cloned()
                    .unwrap_or(FieldIntensityDefinition {
                        name: String::new(),
                        symbol: String::from("%"),
                        color: String::from("white"),
                        dangerous: false,
                        transparent: true,
                    });
            field.intensity_levels.push(FieldIntensityDefinition {
                name: object
                    .get("name")
                    .map(|value| parse_text(value, source, "name"))
                    .transpose()?
                    .unwrap_or(fallback.name),
                symbol: optional_string_owned(object, "sym", source)?.unwrap_or(fallback.symbol),
                color: optional_string_owned(object, "color", source)?.unwrap_or(fallback.color),
                dangerous: optional_bool(object, "dangerous", source)?
                    .unwrap_or(fallback.dangerous),
                transparent: optional_bool(object, "transparent", source)?
                    .unwrap_or(fallback.transparent),
            });
        }
    }
    if let Some(value) = object.get("priority") {
        field.priority = i32::try_from(value.as_i64().ok_or_else(|| invalid(source, "priority"))?)
            .map_err(|_| invalid(source, "priority"))?;
    }
    if let Some(value) = object.get("half_life") {
        field.half_life_seconds = parse_duration_seconds(
            value.as_str().ok_or_else(|| invalid(source, "half_life"))?,
            source,
        )?;
    }
    if let Some(value) = optional_bool(object, "linear_half_life", source)? {
        field.linear_half_life = value;
    }
    if let Some(value) = optional_bool(object, "is_splattering", source)? {
        field.is_splattering = value;
    }
    if let Some(value) = optional_bool(object, "display_field", source)? {
        field.display_field = value;
    }
    for key in object.keys() {
        if !key.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&key.as_str()) {
            field.unsupported_fields.insert(key.clone());
        }
    }
    Ok(())
}

fn parse_duration_seconds(value: &str, source: &str) -> Result<u64, FieldTypeRegistryError> {
    let bytes = value.as_bytes();
    let mut index = 0_usize;
    let mut seconds = 0_u64;
    let mut terms = 0_u64;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let number_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if number_start == index {
            return Err(invalid(source, "half_life"));
        }
        let number = value[number_start..index]
            .parse::<u64>()
            .map_err(|_| invalid(source, "half_life"))?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let unit_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let multiplier = match &value[unit_start..index] {
            "turn" | "turns" | "s" | "second" | "seconds" => 1,
            "m" | "minute" | "minutes" => 60,
            "h" | "hour" | "hours" => 60 * 60,
            "d" | "day" | "days" => 24 * 60 * 60,
            _ => return Err(invalid(source, "half_life")),
        };
        seconds = seconds
            .checked_add(
                number
                    .checked_mul(multiplier)
                    .ok_or_else(|| invalid(source, "half_life"))?,
            )
            .ok_or_else(|| invalid(source, "half_life"))?;
        terms += 1;
    }
    if terms == 0 {
        return Err(invalid(source, "half_life"));
    }
    Ok(seconds)
}

fn parse_text(value: &Value, source: &str, field: &str) -> Result<String, FieldTypeRegistryError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .map(str::to_owned)
            .ok_or_else(|| invalid(source, field)),
        _ => Err(invalid(source, field)),
    }
}

fn optional_string_owned(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<String>, FieldTypeRegistryError> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .transpose()
}

fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<bool>, FieldTypeRegistryError> {
    object
        .get(field)
        .map(|value| value.as_bool().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<&'a str>, FieldTypeRegistryError> {
    object
        .get(field)
        .map(|value| value.as_str().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn invalid(source: &str, field: &str) -> FieldTypeRegistryError {
    FieldTypeRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum FieldTypeRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    InvalidFinalizedField { id: String, source: String },
    InvalidIdentity,
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for FieldTypeRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "field-type mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("field-type load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "field type is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid field-type field {field} in {source}")
            }
            Self::InvalidFinalizedField { id, source } => {
                write!(
                    formatter,
                    "field type {id} is incomplete after inheritance in {source}"
                )
            }
            Self::InvalidIdentity => formatter.write_str("field type must have a non-empty id"),
            Self::Io(path, error) => write!(formatter, "field-type I/O failed for {path}: {error}"),
            Self::Json(path, error) => {
                write!(formatter, "field-type JSON failed for {path}: {error}")
            }
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved or cyclic field-type inheritance: {ids:?}"
                )
            }
        }
    }
}

impl std::error::Error for FieldTypeRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_blood_inherits_intensity_visuals_and_duration() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(crate::DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest should load");
        let root = manifest_path
            .parent()
            .expect("manifest should have a parent");
        let catalog = ModCatalog::load(&manifest, root).expect("mods should load");
        let enabled = catalog
            .recommended_new_world()
            .expect("mods should resolve");
        let fields = FieldTypeRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("field types should load");
        let blood = fields.get("fd_blood").expect("blood should exist");
        assert_eq!(blood.half_life_seconds, 2 * 24 * 60 * 60);
        assert_eq!(blood.intensity_levels.len(), 3);
        assert_eq!(blood.intensity_levels[0].name, "blood splatter");
        assert_eq!(blood.intensity_levels[1].color, "red");
        assert_eq!(blood.intensity_levels[2].symbol, "%");
        assert!(blood.is_splattering);
        assert!(blood.display_field);
    }

    #[test]
    fn duration_parser_accepts_cdda_turns_and_compound_units() {
        assert_eq!(
            parse_duration_seconds("2 turns", "test").expect("turn duration should parse"),
            2
        );
        assert_eq!(
            parse_duration_seconds("1 h 20m", "test").expect("mixed duration should parse"),
            4_800
        );
        assert!(parse_duration_seconds("forever", "test").is_err());
    }
}
