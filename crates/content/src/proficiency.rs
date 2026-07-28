use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const PROFICIENCY_MULTIPLIER_SCALE: u32 = 1_000_000;

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "name",
    "description",
    "category",
    "can_learn",
    "default_time_multiplier",
    "default_skill_penalty",
    "time_to_learn",
    "required_proficiencies",
    "ignore_focus",
    "teachable",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProficiencyDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub can_learn: bool,
    pub default_time_multiplier_millionths: u32,
    pub default_skill_penalty_millionths: i32,
    /// Pinned CDDA action moves (100 moves per second).
    pub time_to_learn_moves: u64,
    pub required_proficiencies: BTreeSet<String>,
    pub ignore_focus: bool,
    pub teachable: bool,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProficiencyRegistry {
    proficiencies: BTreeMap<String, ProficiencyDefinition>,
}

impl ProficiencyRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, ProficiencyRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(ProficiencyRegistryError::Catalog)?;
        let mut proficiencies = BTreeMap::new();
        for file in files {
            load_file(content_root.as_ref(), &file, &mut proficiencies)?;
        }
        for definition in proficiencies.values() {
            for required in &definition.required_proficiencies {
                if !proficiencies.contains_key(required) {
                    return Err(ProficiencyRegistryError::MissingRequirement {
                        proficiency: definition.id.clone(),
                        required: required.clone(),
                    });
                }
            }
        }
        Ok(Self { proficiencies })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.proficiencies.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.proficiencies.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ProficiencyDefinition> {
        self.proficiencies.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ProficiencyDefinition)> {
        self.proficiencies
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

fn load_file(
    root: &Path,
    file: &SelectedContentFile,
    proficiencies: &mut BTreeMap<String, ProficiencyDefinition>,
) -> Result<(), ProficiencyRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| ProficiencyRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| ProficiencyRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                load_value(file, value, proficiencies)?;
            }
        }
        value => load_value(file, value, proficiencies)?,
    }
    Ok(())
}

fn load_value(
    file: &SelectedContentFile,
    value: Value,
    proficiencies: &mut BTreeMap<String, ProficiencyDefinition>,
) -> Result<(), ProficiencyRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("proficiency") {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| ProficiencyRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    let source = file.upstream_path.as_str();
    let id = required_string(object, "id", source)?;
    let definition = ProficiencyDefinition {
        id: id.to_owned(),
        name: translated_string(object, "name", source)?,
        description: translated_string(object, "description", source)?,
        category: required_string(object, "category", source)?.to_owned(),
        can_learn: required_bool(object, "can_learn", source)?,
        default_time_multiplier_millionths: optional_unsigned_decimal(
            object,
            "default_time_multiplier",
            source,
        )?
        .unwrap_or(2 * PROFICIENCY_MULTIPLIER_SCALE),
        default_skill_penalty_millionths: optional_signed_decimal(
            object,
            "default_skill_penalty",
            source,
        )?
        .unwrap_or(i32::try_from(PROFICIENCY_MULTIPLIER_SCALE).expect("scale fits i32")),
        time_to_learn_moves: object
            .get("time_to_learn")
            .map(|value| {
                parse_duration_moves(
                    value
                        .as_str()
                        .ok_or_else(|| invalid(source, "time_to_learn"))?,
                    source,
                )
            })
            .transpose()?
            .unwrap_or(9999 * 60 * 60 * 100),
        required_proficiencies: object
            .get("required_proficiencies")
            .map(|value| string_set(value, source, "required_proficiencies"))
            .transpose()?
            .unwrap_or_default(),
        ignore_focus: optional_bool(object, "ignore_focus", source)?.unwrap_or(false),
        teachable: optional_bool(object, "teachable", source)?.unwrap_or(true),
        unsupported_fields: object
            .keys()
            .filter(|field| {
                !field.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&field.as_str())
            })
            .cloned()
            .collect(),
        source: source.to_owned(),
    };
    if definition.default_time_multiplier_millionths == 0 || definition.time_to_learn_moves == 0 {
        return Err(invalid(source, "default_time_multiplier/time_to_learn"));
    }
    // CDDA's generic factory permits a later selected definition to replace an
    // earlier one with the same stable string ID.
    proficiencies.insert(id.to_owned(), definition);
    Ok(())
}

fn translated_string(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<String, ProficiencyRegistryError> {
    match object.get(field) {
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
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, ProficiencyRegistryError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(source, field))
}

fn required_bool(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<bool, ProficiencyRegistryError> {
    object
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid(source, field))
}

fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<bool>, ProficiencyRegistryError> {
    object
        .get(field)
        .map(|value| value.as_bool().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn optional_unsigned_decimal(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<u32>, ProficiencyRegistryError> {
    object
        .get(field)
        .map(|value| {
            let number = value.as_number().ok_or_else(|| invalid(source, field))?;
            let scaled = decimal_millionths(number, source, field)?;
            u32::try_from(scaled).map_err(|_| invalid(source, field))
        })
        .transpose()
}

fn optional_signed_decimal(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<i32>, ProficiencyRegistryError> {
    object
        .get(field)
        .map(|value| {
            let number = value.as_number().ok_or_else(|| invalid(source, field))?;
            i32::try_from(decimal_millionths(number, source, field)?)
                .map_err(|_| invalid(source, field))
        })
        .transpose()
}

pub(crate) fn decimal_millionths(
    number: &Number,
    source: &str,
    field: &str,
) -> Result<i64, ProficiencyRegistryError> {
    let text = number.to_string();
    let (negative, unsigned) = text
        .strip_prefix('-')
        .map_or((false, text.as_str()), |value| (true, value));
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 6
    {
        return Err(invalid(source, field));
    }
    let whole = whole.parse::<i64>().map_err(|_| invalid(source, field))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| invalid(source, field))?
            .checked_mul(10_i64.pow(u32::try_from(6 - fraction.len()).expect("length fits")))
            .ok_or_else(|| invalid(source, field))?
    };
    let scaled = whole
        .checked_mul(i64::from(PROFICIENCY_MULTIPLIER_SCALE))
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| invalid(source, field))?;
    Ok(if negative { -scaled } else { scaled })
}

fn string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, ProficiencyRegistryError> {
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

fn parse_duration_moves(value: &str, source: &str) -> Result<u64, ProficiencyRegistryError> {
    let bytes = value.as_bytes();
    let mut index = 0_usize;
    let mut seconds = 0_u64;
    let mut terms = 0_usize;
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
            return Err(invalid(source, "time_to_learn"));
        }
        let amount = value[number_start..index]
            .parse::<u64>()
            .map_err(|_| invalid(source, "time_to_learn"))?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let unit_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let multiplier = match &value[unit_start..index] {
            "s" | "second" | "seconds" => 1,
            "m" | "minute" | "minutes" => 60,
            "h" | "hour" | "hours" => 60 * 60,
            "d" | "day" | "days" => 24 * 60 * 60,
            _ => return Err(invalid(source, "time_to_learn")),
        };
        seconds = seconds
            .checked_add(
                amount
                    .checked_mul(multiplier)
                    .ok_or_else(|| invalid(source, "time_to_learn"))?,
            )
            .ok_or_else(|| invalid(source, "time_to_learn"))?;
        terms += 1;
    }
    if terms == 0 {
        return Err(invalid(source, "time_to_learn"));
    }
    seconds
        .checked_mul(100)
        .ok_or_else(|| invalid(source, "time_to_learn"))
}

fn invalid(source: &str, field: &str) -> ProficiencyRegistryError {
    ProficiencyRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum ProficiencyRegistryError {
    Catalog(ModCatalogError),
    InvalidDefinition(String),
    InvalidField {
        source: String,
        field: String,
    },
    MissingRequirement {
        proficiency: String,
        required: String,
    },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
}

impl fmt::Display for ProficiencyRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "proficiency mod selection failed: {error}"),
            Self::InvalidDefinition(source) => {
                write!(
                    formatter,
                    "proficiency definition is not an object in {source}"
                )
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid proficiency field {field} in {source}")
            }
            Self::MissingRequirement {
                proficiency,
                required,
            } => write!(
                formatter,
                "proficiency {proficiency} requires missing proficiency {required}"
            ),
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "proficiency registry I/O failed for {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "proficiency registry JSON failed for {path}: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ProficiencyRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_and_duration_parsing_are_exact_and_bounded() {
        assert_eq!(
            decimal_millionths(&Number::from_f64(1.5).expect("number"), "test", "mult")
                .expect("decimal"),
            1_500_000
        );
        assert_eq!(
            parse_duration_moves("2 h 30 m", "test").expect("duration"),
            900_000
        );
        assert!(parse_duration_moves("forever", "test").is_err());
    }
}
