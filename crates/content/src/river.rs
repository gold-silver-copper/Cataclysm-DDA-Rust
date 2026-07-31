use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const DEFAULT_RIVER_SETTINGS_ID: &str = "default";
pub const MAX_RIVER_SETTINGS: usize = 256;
const MAX_RIVER_SCALE: u8 = 16;
const MAX_SCALED_SETTING: u32 = 1_000_000;

const ROOT_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "river_scale",
    "river_frequency",
    "river_branch_chance",
    "river_branch_remerge_chance",
    "river_branch_scale_decrease",
    "//",
];

/// Determinism-ready projection of pinned `region_settings_river`. Decimal
/// fields are exact thousandths so generation never depends on host floating
/// point behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RiverSettingsDefinition {
    pub id: String,
    pub river_scale: u8,
    pub river_frequency_millis: u32,
    pub branch_chance_millis: u32,
    pub branch_remerge_chance_millis: u32,
    pub branch_scale_decrease_millis: u32,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RiverSettingsRegistry {
    definitions: BTreeMap<String, RiverSettingsDefinition>,
}

impl RiverSettingsRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, RiverSettingsRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(RiverSettingsRegistryError::Catalog)?;
        compile_registry(content_root.as_ref(), files)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&RiverSettingsDefinition> {
        self.definitions.get(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &RiverSettingsDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

#[derive(Debug)]
pub enum RiverSettingsRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Invalid(String),
}

impl fmt::Display for RiverSettingsRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "selected river settings failed: {error}"),
            Self::Io(path, error) => {
                write!(formatter, "failed to read river settings {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "failed to parse river settings {path}: {error}")
            }
            Self::Invalid(reason) => write!(formatter, "invalid river settings: {reason}"),
        }
    }
}

impl std::error::Error for RiverSettingsRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Io(_, error) => Some(error),
            Self::Json(_, error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

fn compile_registry(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<RiverSettingsRegistry, RiverSettingsRegistryError> {
    let mut definitions = BTreeMap::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| RiverSettingsRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| RiverSettingsRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for (index, value) in values.into_iter().enumerate() {
                    compile_value(
                        &value,
                        &format!("{}#{index}", file.upstream_path),
                        &mut definitions,
                    )?;
                }
            }
            value => compile_value(&value, &file.upstream_path, &mut definitions)?,
        }
    }
    if definitions.len() > MAX_RIVER_SETTINGS {
        return Err(RiverSettingsRegistryError::Invalid(format!(
            "definition count exceeds {MAX_RIVER_SETTINGS}"
        )));
    }
    Ok(RiverSettingsRegistry { definitions })
}

fn compile_value(
    value: &Value,
    source: &str,
    definitions: &mut BTreeMap<String, RiverSettingsDefinition>,
) -> Result<(), RiverSettingsRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    if object.get("type").and_then(Value::as_str) != Some("region_settings_river") {
        return Ok(());
    }
    reject_unknown_fields(object, source)?;
    let id = required_text(object, "id", source)?;
    let mut definition = if let Some(parent_id) = object.get("copy-from").and_then(Value::as_str) {
        definitions.get(parent_id).cloned().ok_or_else(|| {
            RiverSettingsRegistryError::Invalid(format!(
                "{source} copies unavailable river settings {parent_id:?}"
            ))
        })?
    } else {
        RiverSettingsDefinition {
            id: id.to_owned(),
            river_scale: 1,
            river_frequency_millis: 1_500,
            branch_chance_millis: 64_000,
            branch_remerge_chance_millis: 4_000,
            branch_scale_decrease_millis: 1_000,
            source: source.to_owned(),
        }
    };
    definition.id = id.to_owned();
    definition.source = source.to_owned();
    if let Some(value) = object.get("river_scale") {
        let scale = value.as_u64().ok_or_else(|| {
            RiverSettingsRegistryError::Invalid(format!(
                "{source} river_scale must be a nonnegative integer"
            ))
        })?;
        definition.river_scale = u8::try_from(scale)
            .ok()
            .filter(|scale| *scale <= MAX_RIVER_SCALE)
            .ok_or_else(|| {
                RiverSettingsRegistryError::Invalid(format!(
                    "{source} river_scale exceeds {MAX_RIVER_SCALE}"
                ))
            })?;
    }
    for (field, target) in [
        ("river_frequency", &mut definition.river_frequency_millis),
        ("river_branch_chance", &mut definition.branch_chance_millis),
        (
            "river_branch_remerge_chance",
            &mut definition.branch_remerge_chance_millis,
        ),
        (
            "river_branch_scale_decrease",
            &mut definition.branch_scale_decrease_millis,
        ),
    ] {
        if let Some(value) = object.get(field) {
            *target = scaled_thousandths(value, field, source)?;
        }
    }
    definitions.insert(id.to_owned(), definition);
    Ok(())
}

fn scaled_thousandths(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<u32, RiverSettingsRegistryError> {
    let number = value.as_f64().ok_or_else(|| {
        RiverSettingsRegistryError::Invalid(format!("{source} {field} must be numeric"))
    })?;
    let scaled = number * 1_000.0;
    let rounded = scaled.round();
    if !number.is_finite()
        || number <= 0.0
        || (scaled - rounded).abs() > 0.000_001
        || rounded > f64::from(MAX_SCALED_SETTING)
    {
        return Err(RiverSettingsRegistryError::Invalid(format!(
            "{source} {field} must be in 0.001 increments and at most {}",
            MAX_SCALED_SETTING / 1_000
        )));
    }
    Ok(rounded as u32)
}

fn required_text<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, RiverSettingsRegistryError> {
    let value = object.get(field).and_then(Value::as_str).ok_or_else(|| {
        RiverSettingsRegistryError::Invalid(format!("{source} requires string {field}"))
    })?;
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(RiverSettingsRegistryError::Invalid(format!(
            "{source} has invalid {field}"
        )));
    }
    Ok(value)
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), RiverSettingsRegistryError> {
    if let Some(field) = object
        .keys()
        .find(|field| !field.starts_with("//") && !ROOT_FIELDS.contains(&field.as_str()))
    {
        return Err(RiverSettingsRegistryError::Invalid(format!(
            "{source} has unsupported field {field:?}"
        )));
    }
    Ok(())
}
