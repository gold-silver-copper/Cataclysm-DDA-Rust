use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const DEFAULT_CITY_SETTINGS_ID: &str = "default";
pub const MAX_CITY_SETTINGS: usize = 256;
pub const MAX_UPSTREAM_CITY_SIZE: u8 = 16;
pub const MAX_UPSTREAM_CITY_SPACING: u8 = 8;

const ROOT_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "is_megacity",
    "city_size",
    "city_spacing",
    "shop_radius",
    "shop_sigma",
    "park_radius",
    "park_sigma",
    "name_snippet",
    "houses",
    "shops",
    "parks",
    "extend",
    "delete",
    "//",
];

/// The placement-affecting subset of pinned `region_settings_city`.
/// Building bins and presentation names remain in pinned source for their
/// later road/city construction family; they do not affect
/// `overmap::place_cities` centers and are not projected here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CitySettingsDefinition {
    pub id: String,
    pub is_megacity: bool,
    pub city_size: u8,
    pub city_spacing: u8,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CitySettingsRegistry {
    definitions: BTreeMap<String, CitySettingsDefinition>,
}

impl CitySettingsRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, CitySettingsRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(CitySettingsRegistryError::Catalog)?;
        compile_registry(content_root.as_ref(), files)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CitySettingsDefinition> {
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

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &CitySettingsDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

#[derive(Debug)]
pub enum CitySettingsRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Invalid(String),
}

impl fmt::Display for CitySettingsRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "selected city settings failed: {error}"),
            Self::Io(path, error) => {
                write!(formatter, "failed to read city settings {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "failed to parse city settings {path}: {error}")
            }
            Self::Invalid(reason) => write!(formatter, "invalid city settings: {reason}"),
        }
    }
}

impl std::error::Error for CitySettingsRegistryError {
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
) -> Result<CitySettingsRegistry, CitySettingsRegistryError> {
    let mut definitions = BTreeMap::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| CitySettingsRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| CitySettingsRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    compile_value(
                        value,
                        &format!("{}#{index}", file.upstream_path),
                        &mut definitions,
                    )?;
                }
            }
            value => compile_value(&value, &file.upstream_path, &mut definitions)?,
        }
    }
    if definitions.len() > MAX_CITY_SETTINGS {
        return Err(CitySettingsRegistryError::Invalid(format!(
            "definition count exceeds {MAX_CITY_SETTINGS}"
        )));
    }
    Ok(CitySettingsRegistry { definitions })
}

fn compile_value(
    value: &Value,
    source: &str,
    definitions: &mut BTreeMap<String, CitySettingsDefinition>,
) -> Result<(), CitySettingsRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    if object.get("type").and_then(Value::as_str) != Some("region_settings_city") {
        return Ok(());
    }
    reject_unknown_fields(object, source)?;
    let id = required_text(object, "id", source)?;
    let mut definition = if let Some(parent_id) = object.get("copy-from").and_then(Value::as_str) {
        definitions.get(parent_id).cloned().ok_or_else(|| {
            CitySettingsRegistryError::Invalid(format!(
                "{source} copies unavailable city settings {parent_id:?}"
            ))
        })?
    } else {
        CitySettingsDefinition {
            id: id.to_owned(),
            is_megacity: false,
            city_size: 8,
            city_spacing: 4,
            source: source.to_owned(),
        }
    };
    definition.id = id.to_owned();
    definition.source = source.to_owned();
    if let Some(value) = object.get("is_megacity") {
        definition.is_megacity = value.as_bool().ok_or_else(|| {
            CitySettingsRegistryError::Invalid(format!("{source} is_megacity must be boolean"))
        })?;
    }
    if let Some(value) = object.get("city_size") {
        definition.city_size = bounded_u8(value, "city_size", MAX_UPSTREAM_CITY_SIZE, source)?;
    } else if !object.contains_key("copy-from") {
        return Err(CitySettingsRegistryError::Invalid(format!(
            "{source} initial city settings require city_size"
        )));
    }
    if let Some(value) = object.get("city_spacing") {
        definition.city_spacing =
            bounded_u8(value, "city_spacing", MAX_UPSTREAM_CITY_SPACING, source)?;
    }
    definitions.insert(id.to_owned(), definition);
    Ok(())
}

fn required_text<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, CitySettingsRegistryError> {
    let value = object.get(field).and_then(Value::as_str).ok_or_else(|| {
        CitySettingsRegistryError::Invalid(format!("{source} requires string {field}"))
    })?;
    if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(CitySettingsRegistryError::Invalid(format!(
            "{source} has invalid {field}"
        )));
    }
    Ok(value)
}

fn bounded_u8(
    value: &Value,
    field: &str,
    maximum: u8,
    source: &str,
) -> Result<u8, CitySettingsRegistryError> {
    let value = value.as_u64().ok_or_else(|| {
        CitySettingsRegistryError::Invalid(format!("{source} {field} must be an integer"))
    })?;
    u8::try_from(value)
        .ok()
        .filter(|value| *value <= maximum)
        .ok_or_else(|| {
            CitySettingsRegistryError::Invalid(format!(
                "{source} {field} must be within 0..={maximum}"
            ))
        })
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), CitySettingsRegistryError> {
    if let Some(field) = object
        .keys()
        .find(|field| !ROOT_FIELDS.contains(&field.as_str()))
    {
        return Err(CitySettingsRegistryError::Invalid(format!(
            "{source} contains unsupported field {field:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_core_city_placement_settings_are_fully_admitted() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(crate::DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest");
        let root = manifest_path.parent().expect("manifest parent");
        let mods = ModCatalog::load(&manifest, root).expect("mods");
        let enabled = mods.recommended_new_world().expect("recommended mods");
        let registry = CitySettingsRegistry::load_selected(&manifest, root, &mods, &enabled)
            .expect("city settings");
        assert_eq!(registry.len(), 2);
        let default = registry.get(DEFAULT_CITY_SETTINGS_ID).expect("default");
        assert_eq!((default.city_size, default.city_spacing), (8, 4));
        assert!(!default.is_megacity);
        assert_eq!(registry.get("no_cities").expect("no cities").city_size, 0);
    }

    #[test]
    fn same_id_overlay_inherits_previous_definition() {
        let mut definitions = BTreeMap::from([(
            String::from("default"),
            CitySettingsDefinition {
                id: String::from("default"),
                is_megacity: false,
                city_size: 8,
                city_spacing: 4,
                source: String::from("base"),
            },
        )]);
        let value = serde_json::json!({
            "type": "region_settings_city",
            "id": "default",
            "copy-from": "default",
            "is_megacity": true
        });
        compile_value(&value, "overlay", &mut definitions).expect("overlay");
        let definition = &definitions["default"];
        assert!(definition.is_megacity);
        assert_eq!((definition.city_size, definition.city_spacing), (8, 4));
    }
}
