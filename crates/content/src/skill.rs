use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "name",
    "description",
    "display_category",
    "sort_rank",
    "tags",
    "consumes_focus",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub display_category: String,
    pub sort_rank: i32,
    pub tags: BTreeSet<String>,
    pub consumes_focus: bool,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SkillRegistry {
    skills: BTreeMap<String, SkillDefinition>,
}

impl SkillRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, SkillRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(SkillRegistryError::Catalog)?;
        let mut skills = BTreeMap::new();
        for file in files {
            load_file(content_root.as_ref(), &file, &mut skills)?;
        }
        Ok(Self { skills })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SkillDefinition> {
        self.skills.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &SkillDefinition)> {
        self.skills
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

fn load_file(
    root: &Path,
    file: &SelectedContentFile,
    skills: &mut BTreeMap<String, SkillDefinition>,
) -> Result<(), SkillRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| SkillRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| SkillRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                load_value(file, value, skills)?;
            }
        }
        value => load_value(file, value, skills)?,
    }
    Ok(())
}

fn load_value(
    file: &SelectedContentFile,
    value: Value,
    skills: &mut BTreeMap<String, SkillDefinition>,
) -> Result<(), SkillRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("skill") {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| SkillRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    let id = required_string(object.get("id"), &file.upstream_path, "id")?;
    let name = translated_string(object.get("name"), &file.upstream_path, "name")?;
    let description = translated_string(
        object.get("description"),
        &file.upstream_path,
        "description",
    )?;
    let display_category = required_string(
        object.get("display_category"),
        &file.upstream_path,
        "display_category",
    )?;
    let sort_rank = object
        .get("sort_rank")
        .and_then(Value::as_i64)
        .map(i32::try_from)
        .transpose()
        .map_err(|_| invalid(&file.upstream_path, "sort_rank"))?
        .unwrap_or_default();
    let tags = object
        .get("tags")
        .map(|value| string_set(value, &file.upstream_path, "tags"))
        .transpose()?
        .unwrap_or_default();
    let consumes_focus = object
        .get("consumes_focus")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid(&file.upstream_path, "consumes_focus"))
        })
        .transpose()?
        .unwrap_or(true);
    let unsupported_fields = object
        .keys()
        .filter(|field| !field.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&field.as_str()))
        .cloned()
        .collect();
    let definition = SkillDefinition {
        id: id.to_owned(),
        name,
        description,
        display_category: display_category.to_owned(),
        sort_rank,
        tags,
        consumes_focus,
        unsupported_fields,
        source: file.upstream_path.clone(),
    };
    // CDDA's generic factory permits a later selected definition to replace an
    // earlier one with the same stable string ID.
    skills.insert(id.to_owned(), definition);
    Ok(())
}

fn translated_string(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<String, SkillRegistryError> {
    match value {
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(Value::Object(values)) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| invalid(source, field)),
        _ => Err(invalid(source, field)),
    }
}

fn required_string<'a>(
    value: Option<&'a Value>,
    source: &str,
    field: &str,
) -> Result<&'a str, SkillRegistryError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(source, field))
}

fn string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, SkillRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid(source, field))?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .collect()
}

fn invalid(source: &str, field: &str) -> SkillRegistryError {
    SkillRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum SkillRegistryError {
    Catalog(ModCatalogError),
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
}

impl fmt::Display for SkillRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "skill mod selection failed: {error}"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "skill definition is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid skill field {field} in {source}")
            }
            Self::Io(path, error) => {
                write!(formatter, "skill registry I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "skill registry JSON failed for {path}: {error}")
            }
        }
    }
}

impl std::error::Error for SkillRegistryError {}
