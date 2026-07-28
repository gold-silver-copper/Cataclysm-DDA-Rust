use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::recipe::{
    parse_component_groups, parse_duration_moves, parse_quality_groups, parse_skill_map,
};
use crate::{
    ComponentRequirement, ContentManifest, ModCatalog, ModCatalogError, QualityRequirement,
    SelectedContentFile,
};

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "group",
    "category",
    "required_skills",
    "time",
    "components",
    "qualities",
    "pre_terrain",
    "pre_special",
    "pre_note",
    "post_terrain",
    "activity_level",
];

const GROUP_IMPLEMENTED_FIELDS: &[&str] = &["type", "id", "name"];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

pub(crate) fn group_field_is_implemented(field: &str) -> bool {
    GROUP_IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConstructionDefinition {
    pub id: String,
    pub group: String,
    pub category: String,
    /// Pinned CDDA action moves (100 moves per second).
    pub time_moves: u64,
    pub activity_level: String,
    pub required_skills: BTreeMap<String, u8>,
    pub components: Vec<Vec<ComponentRequirement>>,
    /// Non-consuming item qualities required while work proceeds.
    pub qualities: Vec<Vec<QualityRequirement>>,
    /// Empty means no exact terrain/furniture prerequisite. Multiple entries
    /// retain the pinned OR-list form.
    pub pre_terrain: Vec<String>,
    /// Ordered pinned construction predicate names.
    pub pre_special: Vec<String>,
    pub pre_note: String,
    /// Despite the upstream field name, this may name terrain or furniture.
    pub post_terrain: String,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConstructionGroupDefinition {
    pub id: String,
    pub name: String,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConstructionRegistry {
    constructions: BTreeMap<String, ConstructionDefinition>,
    groups: BTreeMap<String, ConstructionGroupDefinition>,
}

impl ConstructionRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, ConstructionRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(ConstructionRegistryError::Catalog)?;
        let mut registry = Self::default();
        for file in files {
            let bytes = fs::read(content_root.as_ref().join(&file.destination))
                .map_err(|error| ConstructionRegistryError::Io(file.destination.clone(), error))?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
                ConstructionRegistryError::Json(file.destination.clone(), error)
            })?;
            match value {
                Value::Array(values) => {
                    for value in values {
                        registry.collect(&file, value)?;
                    }
                }
                value => registry.collect(&file, value)?,
            }
        }
        for construction in registry.constructions.values() {
            if !registry.groups.contains_key(&construction.group) {
                return Err(ConstructionRegistryError::MissingGroup {
                    construction: construction.id.clone(),
                    group: construction.group.clone(),
                });
            }
        }
        Ok(registry)
    }

    fn collect(
        &mut self,
        file: &SelectedContentFile,
        value: Value,
    ) -> Result<(), ConstructionRegistryError> {
        match value.get("type").and_then(Value::as_str) {
            Some("construction") => {
                let definition = parse_construction(&file.upstream_path, value)?;
                let id = definition.id.clone();
                if self.constructions.insert(id.clone(), definition).is_some() {
                    return Err(ConstructionRegistryError::DuplicateId(id));
                }
            }
            Some("construction_group") => {
                let definition = parse_group(&file.upstream_path, value)?;
                let id = definition.id.clone();
                if self.groups.insert(id.clone(), definition).is_some() {
                    return Err(ConstructionRegistryError::DuplicateGroup(id));
                }
            }
            _ => {}
        }
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.constructions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.constructions.is_empty()
    }

    #[must_use]
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ConstructionDefinition> {
        self.constructions.get(id)
    }

    #[must_use]
    pub fn group(&self, id: &str) -> Option<&ConstructionGroupDefinition> {
        self.groups.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ConstructionDefinition)> {
        self.constructions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

fn parse_construction(
    path: &str,
    value: Value,
) -> Result<ConstructionDefinition, ConstructionRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| ConstructionRegistryError::InvalidDefinition(path.to_owned()))?;
    let id = required_string(object, "id", path)?;
    let source = format!("{path}#{id}");
    let group = required_string(object, "group", &source)?;
    let category = required_string(object, "category", &source)?;
    let time = required_string(object, "time", &source)?;
    let time_moves = parse_duration_moves(&time, &source).map_err(|_| invalid(&source, "time"))?;
    let activity_level = optional_string(object, "activity_level", &source)?;
    let required_skills = object
        .get("required_skills")
        .map(|value| parse_skill_map(value, &source, "required_skills"))
        .transpose()
        .map_err(|_| invalid(&source, "required_skills"))?
        .unwrap_or_default();
    let components = object
        .get("components")
        .map(|value| parse_component_groups(value, &source, "components"))
        .transpose()
        .map_err(|_| invalid(&source, "components"))?
        .unwrap_or_default();
    let qualities = object
        .get("qualities")
        .map(|value| parse_quality_groups(value, &source, "qualities"))
        .transpose()
        .map_err(|_| invalid(&source, "qualities"))?
        .unwrap_or_default();
    let pre_terrain = optional_string_list(object, "pre_terrain", &source)?;
    let pre_special = optional_string_list(object, "pre_special", &source)?;
    let pre_note = optional_text(object, "pre_note", &source)?;
    let post_terrain = optional_string(object, "post_terrain", &source)?;
    let unsupported_fields = object
        .keys()
        .filter(|field| !field.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&field.as_str()))
        .cloned()
        .collect();
    Ok(ConstructionDefinition {
        id,
        group,
        category,
        time_moves,
        activity_level,
        required_skills,
        components,
        qualities,
        pre_terrain,
        pre_special,
        pre_note,
        post_terrain,
        unsupported_fields,
        source,
    })
}

fn parse_group(
    path: &str,
    value: Value,
) -> Result<ConstructionGroupDefinition, ConstructionRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| ConstructionRegistryError::InvalidDefinition(path.to_owned()))?;
    let id = required_string(object, "id", path)?;
    let source = format!("{path}#{id}");
    let name = required_text(object, "name", &source)?;
    let unsupported_fields = object
        .keys()
        .filter(|field| {
            !field.starts_with("//") && !GROUP_IMPLEMENTED_FIELDS.contains(&field.as_str())
        })
        .cloned()
        .collect();
    Ok(ConstructionGroupDefinition {
        id,
        name,
        unsupported_fields,
        source,
    })
}

fn required_string(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<String, ConstructionRegistryError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(source, field))
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<String, ConstructionRegistryError> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn optional_string_list(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Vec<String>, ConstructionRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(Vec::new());
    };
    if let Some(value) = value.as_str() {
        if value.is_empty() {
            return Err(invalid(source, field));
        }
        return Ok(vec![value.to_owned()]);
    }
    value
        .as_array()
        .ok_or_else(|| invalid(source, field))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .collect()
}

fn required_text(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<String, ConstructionRegistryError> {
    let text = optional_text(object, field, source)?;
    if text.is_empty() {
        Err(invalid(source, field))
    } else {
        Ok(text)
    }
}

fn optional_text(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<String, ConstructionRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(String::new());
    };
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Object(values) => values
            .get("str")
            .or_else(|| values.get("str_sp"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| invalid(source, field)),
        _ => Err(invalid(source, field)),
    }
}

fn invalid(source: &str, field: &str) -> ConstructionRegistryError {
    ConstructionRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum ConstructionRegistryError {
    Catalog(ModCatalogError),
    DuplicateGroup(String),
    DuplicateId(String),
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    MissingGroup { construction: String, group: String },
}

impl fmt::Display for ConstructionRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "construction mod selection failed: {error}"),
            Self::DuplicateGroup(id) => write!(formatter, "duplicate construction group {id}"),
            Self::DuplicateId(id) => write!(formatter, "duplicate construction {id}"),
            Self::InvalidDefinition(source) => {
                write!(
                    formatter,
                    "construction definition is not an object in {source}"
                )
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid construction field {field} in {source}")
            }
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "construction registry I/O failed for {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "construction registry JSON failed for {path}: {error}"
                )
            }
            Self::MissingGroup {
                construction,
                group,
            } => write!(
                formatter,
                "construction {construction} references missing group {group}"
            ),
        }
    }
}

impl std::error::Error for ConstructionRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_item_placement_shape_retains_time_requirements_and_predicates() {
        let definition = parse_construction(
            "data/json/construction/furniture_surfaces.json",
            serde_json::json!({
                "type": "construction",
                "id": "constr_place_table",
                "group": "place_table",
                "category": "FURN",
                "required_skills": [["fabrication", 0]],
                "time": "1 m",
                "components": [[ ["w_table", 1] ]],
                "pre_special": "check_empty",
                "pre_note": "Can be deconstructed without tools.",
                "post_terrain": "f_table",
                "activity_level": "LIGHT_EXERCISE"
            }),
        )
        .expect("pinned placement shape should parse");
        assert_eq!(definition.time_moves, 6_000);
        assert_eq!(definition.required_skills["fabrication"], 0);
        assert_eq!(definition.components.len(), 1);
        assert_eq!(definition.components[0][0].type_id, "w_table");
        assert_eq!(definition.pre_special, ["check_empty"]);
        assert_eq!(definition.post_terrain, "f_table");
        assert!(definition.unsupported_fields.is_empty());
    }

    #[test]
    fn unsupported_behavior_is_retained_fail_closed() {
        let definition = parse_construction(
            "test.json",
            serde_json::json!({
                "type": "construction",
                "id": "constr_special",
                "group": "special",
                "category": "OTHER",
                "time": "5 s",
                "post_special": "done_mine_downstair"
            }),
        )
        .expect("unsupported behavior should remain inspectable");
        assert_eq!(
            definition.unsupported_fields,
            BTreeSet::from([String::from("post_special")])
        );
    }

    #[test]
    fn exact_quality_groups_are_typed_for_authoritative_construction() {
        let definition = parse_construction(
            "data/json/construction/floors_indoors.json",
            serde_json::json!({
                "type": "construction",
                "id": "constr_carpet_green",
                "group": "carpet_floor_green",
                "category": "DECORATE",
                "time": "15 m",
                "qualities": [[{ "id": "HAMMER", "level": 2 }]],
                "components": [[ ["nails", 5] ], [ ["g_carpet", 1] ]],
                "pre_terrain": "t_floor",
                "post_terrain": "t_carpet_green",
                "activity_level": "LIGHT_EXERCISE"
            }),
        )
        .expect("pinned quality construction should parse");
        assert_eq!(definition.qualities.len(), 1);
        assert_eq!(definition.qualities[0][0].quality_id, "HAMMER");
        assert_eq!(definition.qualities[0][0].level, 2);
        assert_eq!(definition.qualities[0][0].amount, 1);
        assert!(definition.unsupported_fields.is_empty());
    }
}
