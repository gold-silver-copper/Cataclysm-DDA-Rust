//! Strict pinned NPC faction definitions.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const FACTION_FIELDS: &[&str] = &[
    "type",
    "id",
    "name",
    "description",
    "likes_u",
    "respects_u",
    "trusts_u",
    "known_by_u",
    "size",
    "power",
    "wealth",
    "fac_food_supply",
    "consumes_food",
    "lone_wolf_faction",
    "limited_area_claim",
    "currency",
    "relations",
    "mon_faction",
];

const RELATION_FIELDS: &[&str] = &[
    "kill on sight",
    "watch your back",
    "share my stuff",
    "share public goods",
    "guard your stuff",
    "lets you in",
    "defends your space",
    "knows your voice",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    FACTION_FIELDS.contains(&field)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FactionRelationFlagsDefinition {
    pub kill_on_sight: bool,
    pub watch_your_back: bool,
    pub share_my_stuff: bool,
    pub share_public_goods: bool,
    pub guard_your_stuff: bool,
    pub lets_you_in: bool,
    pub defends_your_space: bool,
    pub knows_your_voice: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactionFoodSupplyDefinition {
    pub expires_at_turn: i64,
    pub calories: i64,
    pub vitamins: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactionDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub likes_u: i32,
    pub respects_u: i32,
    pub trusts_u: i32,
    pub known_by_u: bool,
    pub size: i32,
    pub power: i32,
    pub wealth: i32,
    pub food_supply: Vec<FactionFoodSupplyDefinition>,
    pub consumes_food: bool,
    pub lone_wolf_faction: bool,
    pub limited_area_claim: bool,
    pub currency_id: String,
    pub relations: BTreeMap<String, FactionRelationFlagsDefinition>,
    pub monster_faction_id: String,
    /// Exact unimplemented top-level and relation members. Runtime admission
    /// is fail-closed when this map is nonempty.
    pub unsupported_fields: BTreeMap<String, Value>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FactionRegistry {
    definitions: BTreeMap<String, FactionDefinition>,
}

impl FactionRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, FactionRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(FactionRegistryError::Catalog)?;
        let mut registry = Self::default();
        for file in files {
            load_file(content_root.as_ref(), &file, &mut registry)?;
        }
        Ok(registry)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &FactionDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

fn load_file(
    root: &Path,
    file: &SelectedContentFile,
    registry: &mut FactionRegistry,
) -> Result<(), FactionRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| FactionRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| FactionRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                load_value(file, value, registry)?;
            }
        }
        value => load_value(file, value, registry)?,
    }
    Ok(())
}

fn load_value(
    file: &SelectedContentFile,
    value: Value,
    registry: &mut FactionRegistry,
) -> Result<(), FactionRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("faction") {
        return Ok(());
    }
    let object = value.as_object().ok_or_else(|| invalid(file, "faction"))?;
    let id = required_string(object.get("id"), file, "id")?;
    let mut unsupported_fields = object
        .iter()
        .filter(|(field, _)| !field.starts_with("//") && !FACTION_FIELDS.contains(&field.as_str()))
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let relations = relations(object.get("relations"), file, &mut unsupported_fields)?;
    let definition = FactionDefinition {
        id: id.clone(),
        name: required_string(object.get("name"), file, "name")?,
        description: translated_string(
            object
                .get("description")
                .ok_or_else(|| invalid(file, "description"))?,
            file,
            "description",
        )?,
        likes_u: required_i32(object.get("likes_u"), file, "likes_u")?,
        respects_u: required_i32(object.get("respects_u"), file, "respects_u")?,
        trusts_u: optional_i32(object.get("trusts_u"), file, "trusts_u")?.unwrap_or(0),
        known_by_u: required_bool(object.get("known_by_u"), file, "known_by_u")?,
        size: required_i32(object.get("size"), file, "size")?,
        power: required_i32(object.get("power"), file, "power")?,
        wealth: required_i32(object.get("wealth"), file, "wealth")?,
        food_supply: food_supply(object.get("fac_food_supply"), file)?,
        consumes_food: optional_bool(object.get("consumes_food"), file, "consumes_food")?
            .unwrap_or(false),
        lone_wolf_faction: optional_bool(
            object.get("lone_wolf_faction"),
            file,
            "lone_wolf_faction",
        )?
        .unwrap_or(false),
        limited_area_claim: optional_bool(
            object.get("limited_area_claim"),
            file,
            "limited_area_claim",
        )?
        .unwrap_or(false),
        currency_id: optional_string(object.get("currency"), file, "currency")?.unwrap_or_default(),
        relations,
        monster_faction_id: optional_string(object.get("mon_faction"), file, "mon_faction")?
            .unwrap_or_else(|| String::from("human")),
        unsupported_fields,
        source: file.upstream_path.clone(),
    };
    registry.definitions.insert(id, definition);
    Ok(())
}

fn relations(
    value: Option<&Value>,
    file: &SelectedContentFile,
    unsupported: &mut BTreeMap<String, Value>,
) -> Result<BTreeMap<String, FactionRelationFlagsDefinition>, FactionRegistryError> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| invalid(file, "relations"))?;
    let mut result = BTreeMap::new();
    for (target, value) in object {
        if target.is_empty() {
            return Err(invalid(file, "relations"));
        }
        let flags = value
            .as_object()
            .ok_or_else(|| invalid(file, &format!("relations.{target}")))?;
        for (field, value) in flags {
            if !field.starts_with("//") && !RELATION_FIELDS.contains(&field.as_str()) {
                unsupported.insert(format!("relations.{target}.{field}"), value.clone());
            }
        }
        result.insert(
            target.clone(),
            FactionRelationFlagsDefinition {
                kill_on_sight: relation_bool(flags, target, "kill on sight", file)?,
                watch_your_back: relation_bool(flags, target, "watch your back", file)?,
                share_my_stuff: relation_bool(flags, target, "share my stuff", file)?,
                share_public_goods: relation_bool(flags, target, "share public goods", file)?,
                guard_your_stuff: relation_bool(flags, target, "guard your stuff", file)?,
                lets_you_in: relation_bool(flags, target, "lets you in", file)?,
                defends_your_space: relation_bool(flags, target, "defends your space", file)?,
                knows_your_voice: relation_bool(flags, target, "knows your voice", file)?,
            },
        );
    }
    Ok(result)
}

fn relation_bool(
    object: &Map<String, Value>,
    target: &str,
    field: &str,
    file: &SelectedContentFile,
) -> Result<bool, FactionRegistryError> {
    optional_bool(
        object.get(field),
        file,
        &format!("relations.{target}.{field}"),
    )
    .map(|value| value.unwrap_or(false))
}

fn food_supply(
    value: Option<&Value>,
    file: &SelectedContentFile,
) -> Result<Vec<FactionFoodSupplyDefinition>, FactionRegistryError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| invalid(file, "fac_food_supply"))?;
    entries
        .iter()
        .map(|entry| {
            let pair = entry
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| invalid(file, "fac_food_supply"))?;
            let expires_at_turn = pair[0]
                .as_i64()
                .ok_or_else(|| invalid(file, "fac_food_supply.expires"))?;
            let nutrients = pair[1]
                .as_object()
                .ok_or_else(|| invalid(file, "fac_food_supply.nutrients"))?;
            if nutrients.keys().any(|field| {
                !field.starts_with("//") && !["calories", "vitamins"].contains(&field.as_str())
            }) {
                return Err(invalid(file, "fac_food_supply.nutrients"));
            }
            let calories = nutrients
                .get("calories")
                .and_then(Value::as_i64)
                .ok_or_else(|| invalid(file, "fac_food_supply.calories"))?;
            let vitamins = nutrients
                .get("vitamins")
                .and_then(Value::as_object)
                .ok_or_else(|| invalid(file, "fac_food_supply.vitamins"))?
                .iter()
                .map(|(id, value)| {
                    value
                        .as_i64()
                        .map(|amount| (id.clone(), amount))
                        .ok_or_else(|| invalid(file, "fac_food_supply.vitamins"))
                })
                .collect::<Result<_, _>>()?;
            Ok(FactionFoodSupplyDefinition {
                expires_at_turn,
                calories,
                vitamins,
            })
        })
        .collect()
}

fn translated_string(
    value: &Value,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, FactionRegistryError> {
    match value {
        Value::String(value) if !value.is_empty() => Ok(value.clone()),
        Value::Object(values)
            if values.keys().all(|key| {
                key.starts_with("//") || ["str", "str_sp", "str_pl", "ctxt"].contains(&key.as_str())
            }) && values.get("ctxt").is_none_or(|context| {
                context.as_str().is_some_and(|context| !context.is_empty())
            }) =>
        {
            ["str", "str_sp", "str_pl"]
                .into_iter()
                .find_map(|key| values.get(key).and_then(Value::as_str))
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(file, field))
        }
        _ => Err(invalid(file, field)),
    }
}

fn required_string(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, FactionRegistryError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(file, field))
}

fn optional_string(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<Option<String>, FactionRegistryError> {
    value
        .map(|value| required_string(Some(value), file, field))
        .transpose()
}

fn required_i32(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<i32, FactionRegistryError> {
    value
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid(file, field))
}

fn optional_i32(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<Option<i32>, FactionRegistryError> {
    value
        .map(|value| required_i32(Some(value), file, field))
        .transpose()
}

fn required_bool(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<bool, FactionRegistryError> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid(file, field))
}

fn optional_bool(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<Option<bool>, FactionRegistryError> {
    value
        .map(|value| required_bool(Some(value), file, field))
        .transpose()
}

fn invalid(file: &SelectedContentFile, field: &str) -> FactionRegistryError {
    FactionRegistryError::InvalidField {
        source: file.upstream_path.clone(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum FactionRegistryError {
    Catalog(ModCatalogError),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
}

impl fmt::Display for FactionRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "faction mod selection failed: {error}"),
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid faction field {field} in {source}")
            }
            Self::Io(path, error) => write!(formatter, "faction I/O failed for {path}: {error}"),
            Self::Json(path, error) => {
                write!(formatter, "faction JSON failed for {path}: {error}")
            }
        }
    }
}

impl std::error::Error for FactionRegistryError {}
