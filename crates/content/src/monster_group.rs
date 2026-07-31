use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_MONSTER_GROUPS: usize = 16_384;
pub const MAX_MONSTER_GROUP_ENTRIES: usize = 16_384;
pub const MAX_MONSTER_GROUP_PACK_SIZE: u16 = 1_024;
pub const MAX_MONSTER_GROUP_FREQUENCY: u32 = 16_000_000;

const ROOT_FIELDS: &[&str] = &[
    "type",
    "id",
    "name",
    "default",
    "override",
    "is_animal",
    "monsters",
    "replace_monster_group",
    "new_monster_group_id",
    "replacement_time",
    "is_safe",
    "freq_total",
    "auto_total",
];
const ENTRY_FIELDS: &[&str] = &[
    "monster",
    "group",
    "weight",
    "freq",
    "cost_multiplier",
    "pack_size",
    "event",
    "starts",
    "ends",
    "conditions",
    "spawn_data",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MonsterGroupTarget {
    Monster(String),
    Group(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterGroupEntry {
    pub target: MonsterGroupTarget,
    pub weight: u32,
    pub cost_multiplier: i32,
    pub pack_minimum: u16,
    pub pack_maximum: u16,
    /// Empty entries are ordinary timeless selections. Non-empty entries stay
    /// visible to admission code but cannot silently enter the runtime engine.
    pub unsupported_fields: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterGroupDefinition {
    pub id: String,
    pub default_monster: Option<String>,
    pub is_animal: bool,
    pub is_safe: bool,
    pub frequency_total: u32,
    pub entries: Vec<MonsterGroupEntry>,
    /// Root semantics which the static selector cannot reproduce exactly.
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
    explicit_default_null: bool,
}

impl MonsterGroupDefinition {
    #[must_use]
    pub fn is_runtime_static(&self) -> bool {
        self.frequency_total > 0
            && self.unsupported_fields.is_empty()
            && self
                .entries
                .iter()
                .all(|entry| entry.unsupported_fields.is_empty())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MonsterGroupRegistry {
    groups: BTreeMap<String, MonsterGroupDefinition>,
}

impl MonsterGroupRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, MonsterGroupRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(MonsterGroupRegistryError::Catalog)?;
        let mut groups = BTreeMap::new();
        for file in files {
            load_file(content_root.as_ref(), &file, &mut groups)?;
            if groups.len() > MAX_MONSTER_GROUPS {
                return Err(MonsterGroupRegistryError::TooManyGroups);
            }
        }
        Ok(Self { groups })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MonsterGroupDefinition> {
        self.groups.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &MonsterGroupDefinition)> {
        self.groups.iter().map(|(id, group)| (id.as_str(), group))
    }
}

fn load_file(
    root: &Path,
    file: &SelectedContentFile,
    groups: &mut BTreeMap<String, MonsterGroupDefinition>,
) -> Result<(), MonsterGroupRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| MonsterGroupRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| MonsterGroupRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                load_value(&value, file, groups)?;
            }
        }
        value => load_value(&value, file, groups)?,
    }
    Ok(())
}

fn load_value(
    value: &Value,
    file: &SelectedContentFile,
    groups: &mut BTreeMap<String, MonsterGroupDefinition>,
) -> Result<(), MonsterGroupRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    if object.get("type").and_then(Value::as_str) != Some("monstergroup") {
        return Ok(());
    }
    let id = object
        .get("id")
        .or_else(|| object.get("name"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 512)
        .ok_or_else(|| invalid(file, "id"))?
        .to_owned();
    let override_existing = optional_bool(object, "override", false, file)?;
    let extending = groups.contains_key(&id) && !override_existing;
    let mut group = if extending {
        groups.remove(&id).ok_or_else(|| invalid(file, "id"))?
    } else {
        MonsterGroupDefinition {
            id: id.clone(),
            default_monster: None,
            is_animal: false,
            is_safe: false,
            frequency_total: 0,
            entries: Vec::new(),
            unsupported_fields: BTreeSet::new(),
            source: file.upstream_path.clone(),
            explicit_default_null: false,
        }
    };
    group.source = file.upstream_path.clone();
    group.is_animal = optional_bool(object, "is_animal", false, file)?;
    group.is_safe = optional_bool(object, "is_safe", false, file)?;
    group.unsupported_fields.extend(
        object
            .keys()
            .filter(|field| !field.starts_with("//") && !ROOT_FIELDS.contains(&field.as_str()))
            .cloned(),
    );

    if let Some(value) = object.get("default") {
        let default = bounded_id(value, file, "default")?;
        group.explicit_default_null = default == "mon_null";
        group.default_monster = (!group.explicit_default_null).then_some(default);
    } else if !extending {
        group.explicit_default_null = false;
        group.default_monster = None;
    } else if group.default_monster.is_none() {
        // Upstream treats an inherited null default as explicit for an
        // extension, so later entries do not silently become the fallback.
        group.explicit_default_null = true;
    }

    let mut added_frequency = 0_u32;
    let mut best_default: Option<(u32, String)> = None;
    if let Some(value) = object.get("monsters") {
        let entries = value.as_array().ok_or_else(|| invalid(file, "monsters"))?;
        if group.entries.len().saturating_add(entries.len()) > MAX_MONSTER_GROUP_ENTRIES {
            return Err(MonsterGroupRegistryError::TooManyEntries(id));
        }
        for (index, value) in entries.iter().enumerate() {
            let entry = parse_entry(value, file, index)?;
            if entry.unsupported_fields.is_empty() {
                added_frequency = added_frequency
                    .checked_add(entry.weight)
                    .filter(|total| *total <= MAX_MONSTER_GROUP_FREQUENCY)
                    .ok_or_else(|| invalid(file, "monsters.weight"))?;
                if let MonsterGroupTarget::Monster(monster) = &entry.target {
                    if best_default
                        .as_ref()
                        .is_none_or(|(weight, _)| entry.weight > *weight)
                    {
                        best_default = Some((entry.weight, monster.clone()));
                    }
                }
            }
            group.entries.push(entry);
        }
    }
    if group.default_monster.is_none() && !group.explicit_default_null {
        group.default_monster = best_default.map(|(_, monster)| monster);
    }

    if optional_bool(object, "replace_monster_group", false, file)? {
        group
            .unsupported_fields
            .insert(String::from("replace_monster_group"));
    }
    for field in ["new_monster_group_id", "replacement_time"] {
        if object.contains_key(field) {
            group.unsupported_fields.insert(field.to_owned());
        }
    }
    group.frequency_total = if optional_bool(object, "auto_total", false, file)? {
        group.entries.iter().try_fold(0_u32, |total, entry| {
            total
                .checked_add(entry.weight)
                .filter(|total| *total <= MAX_MONSTER_GROUP_FREQUENCY)
                .ok_or_else(|| invalid(file, "auto_total"))
        })?
    } else if let Some(value) = object.get("freq_total") {
        bounded_u32(value, 1, MAX_MONSTER_GROUP_FREQUENCY, file, "freq_total")?
    } else {
        let base = if extending { group.frequency_total } else { 0 };
        base.checked_add(added_frequency)
            .filter(|total| *total <= MAX_MONSTER_GROUP_FREQUENCY)
            .ok_or_else(|| invalid(file, "freq_total"))?
    };
    groups.insert(id, group);
    Ok(())
}

fn parse_entry(
    value: &Value,
    file: &SelectedContentFile,
    index: usize,
) -> Result<MonsterGroupEntry, MonsterGroupRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(file, &format!("monsters[{index}]")))?;
    let target = match (
        object.get("monster").and_then(Value::as_str),
        object.get("group").and_then(Value::as_str),
    ) {
        (Some(monster), None) if valid_id(monster) => {
            MonsterGroupTarget::Monster(monster.to_owned())
        }
        (None, Some(group)) if valid_id(group) => MonsterGroupTarget::Group(group.to_owned()),
        _ => return Err(invalid(file, &format!("monsters[{index}].target"))),
    };
    let weight = object
        .get("freq")
        .or_else(|| object.get("weight"))
        .map_or(Ok(1), |value| {
            bounded_u32(
                value,
                1,
                MAX_MONSTER_GROUP_FREQUENCY,
                file,
                &format!("monsters[{index}].weight"),
            )
        })?;
    let cost_multiplier = object.get("cost_multiplier").map_or(Ok(1), |value| {
        value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| invalid(file, &format!("monsters[{index}].cost_multiplier")))
    })?;
    let (pack_minimum, pack_maximum) = parse_pack_size(object.get("pack_size"), file, index)?;
    let mut unsupported_fields = object
        .keys()
        .filter(|field| !field.starts_with("//") && !ENTRY_FIELDS.contains(&field.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    if object
        .get("event")
        .is_some_and(|value| value.as_str() != Some("none"))
    {
        unsupported_fields.insert(String::from("event"));
    }
    for field in ["starts", "ends"] {
        if object.contains_key(field) {
            unsupported_fields.insert(field.to_owned());
        }
    }
    if object
        .get("conditions")
        .is_some_and(|value| value.as_array().is_none_or(|values| !values.is_empty()))
    {
        unsupported_fields.insert(String::from("conditions"));
    }
    if object
        .get("spawn_data")
        .is_some_and(|value| value.as_object().is_none_or(|data| !data.is_empty()))
    {
        unsupported_fields.insert(String::from("spawn_data"));
    }
    Ok(MonsterGroupEntry {
        target,
        weight,
        cost_multiplier,
        pack_minimum,
        pack_maximum,
        unsupported_fields,
    })
}

fn parse_pack_size(
    value: Option<&Value>,
    file: &SelectedContentFile,
    index: usize,
) -> Result<(u16, u16), MonsterGroupRegistryError> {
    let Some(value) = value else {
        return Ok((1, 1));
    };
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| invalid(file, &format!("monsters[{index}].pack_size")))?;
    let minimum = bounded_u32(
        &values[0],
        1,
        u32::from(MAX_MONSTER_GROUP_PACK_SIZE),
        file,
        &format!("monsters[{index}].pack_size.minimum"),
    )? as u16;
    let maximum = bounded_u32(
        &values[1],
        u32::from(minimum),
        u32::from(MAX_MONSTER_GROUP_PACK_SIZE),
        file,
        &format!("monsters[{index}].pack_size.maximum"),
    )? as u16;
    Ok((minimum, maximum))
}

fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
    default: bool,
    file: &SelectedContentFile,
) -> Result<bool, MonsterGroupRegistryError> {
    object.get(field).map_or(Ok(default), |value| {
        value.as_bool().ok_or_else(|| invalid(file, field))
    })
}

fn bounded_id(
    value: &Value,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, MonsterGroupRegistryError> {
    value
        .as_str()
        .filter(|id| valid_id(id))
        .map(str::to_owned)
        .ok_or_else(|| invalid(file, field))
}

fn bounded_u32(
    value: &Value,
    minimum: u32,
    maximum: u32,
    file: &SelectedContentFile,
    field: &str,
) -> Result<u32, MonsterGroupRegistryError> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| invalid(file, field))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512
}

fn invalid(file: &SelectedContentFile, field: &str) -> MonsterGroupRegistryError {
    MonsterGroupRegistryError::InvalidField {
        source: file.upstream_path.clone(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum MonsterGroupRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    InvalidField { source: String, field: String },
    TooManyGroups,
    TooManyEntries(String),
}

impl fmt::Display for MonsterGroupRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(
                formatter,
                "could not resolve monster-group content: {error}"
            ),
            Self::Io(path, error) => write!(formatter, "could not read {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "could not parse {path}: {error}"),
            Self::InvalidField { source, field } => {
                write!(
                    formatter,
                    "invalid monster-group field {field:?} in {source}"
                )
            }
            Self::TooManyGroups => write!(
                formatter,
                "monster-group count exceeds {MAX_MONSTER_GROUPS}"
            ),
            Self::TooManyEntries(id) => write!(
                formatter,
                "monster group {id:?} exceeds {MAX_MONSTER_GROUP_ENTRIES} entries"
            ),
        }
    }
}

impl std::error::Error for MonsterGroupRegistryError {}
