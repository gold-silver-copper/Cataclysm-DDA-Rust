use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const IMPLEMENTED_FIELDS: &[&str] = &["type", "id", "profile"];
pub const BASH_MULTIPLIER_SCALE: u32 = 1_000_000;
const BASH_FIELDS: &[&str] = &[
    "str_min",
    "str_max",
    "str_min_blocked",
    "str_max_blocked",
    "str_min_supported",
    "str_max_supported",
    "profile",
    "explosive",
    "sound_vol",
    "sound_fail_vol",
    "collapse_radius",
    "destroy_only",
    "bash_below",
    "sound",
    "sound_fail",
    "hit_field",
    "destroyed_field",
    "items",
    "tent_centers",
    "ter_set",
    "ter_set_bashed_from_above",
    "furn_set",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BashDamageProfileDefinition {
    pub id: String,
    /// Damage-type-keyed susceptibility in millionths.
    pub multipliers_millionths: BTreeMap<String, u32>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BashFieldEffectDefinition {
    pub field_type_id: String,
    pub intensity: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BashDropDefinition {
    pub item_id: String,
    pub probability_percent: u8,
    pub count_min: u16,
    pub count_max: u16,
    pub charges_min: Option<i32>,
    pub charges_max: Option<i32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BashDefinition {
    pub str_min: i32,
    pub str_max: i32,
    pub str_min_blocked: i32,
    pub str_max_blocked: i32,
    pub str_min_supported: i32,
    pub str_max_supported: i32,
    pub profile: String,
    pub explosive: i32,
    pub sound_volume: i32,
    pub failure_sound_volume: i32,
    pub collapse_radius: i32,
    pub destroy_only: bool,
    pub bash_below: bool,
    pub sound: String,
    pub failure_sound: String,
    pub hit_field: Option<BashFieldEffectDefinition>,
    pub destroyed_field: Option<BashFieldEffectDefinition>,
    pub drops: Vec<BashDropDefinition>,
    pub tent_centers: Vec<String>,
    pub terrain_result: String,
    pub terrain_result_bashed_from_above: String,
    pub furniture_result: String,
    pub unsupported_fields: BTreeMap<String, String>,
}

impl Default for BashDefinition {
    fn default() -> Self {
        Self {
            str_min: -1,
            str_max: -1,
            str_min_blocked: -1,
            str_max_blocked: -1,
            str_min_supported: -1,
            str_max_supported: -1,
            profile: String::from("default"),
            explosive: -1,
            sound_volume: -1,
            failure_sound_volume: -1,
            collapse_radius: 1,
            destroy_only: false,
            bash_below: false,
            sound: String::from("smash!"),
            failure_sound: String::from("thump!"),
            hit_field: None,
            destroyed_field: None,
            drops: Vec::new(),
            tent_centers: Vec::new(),
            terrain_result: String::from("t_null"),
            terrain_result_bashed_from_above: String::new(),
            furniture_result: String::from("f_null"),
            unsupported_fields: BTreeMap::new(),
        }
    }
}

impl BashDefinition {
    #[must_use]
    pub fn is_fully_supported(&self) -> bool {
        self.unsupported_fields.is_empty()
            && self.explosive == -1
            && self.collapse_radius == 1
            && !self.destroy_only
            && !self.bash_below
            && self.tent_centers.is_empty()
            && self.terrain_result_bashed_from_above.is_empty()
    }
}

pub(crate) fn apply_bash_definition(
    current: &mut Option<BashDefinition>,
    value: &Value,
    source: &str,
) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("bash must be an object in {source}"))?;
    let was_loaded = current.is_some();
    let mut bash = current.clone().unwrap_or_default();
    for (field, target) in [
        ("str_min", &mut bash.str_min),
        ("str_max", &mut bash.str_max),
        ("str_min_blocked", &mut bash.str_min_blocked),
        ("str_max_blocked", &mut bash.str_max_blocked),
        ("str_min_supported", &mut bash.str_min_supported),
        ("str_max_supported", &mut bash.str_max_supported),
        ("explosive", &mut bash.explosive),
        ("sound_vol", &mut bash.sound_volume),
        ("sound_fail_vol", &mut bash.failure_sound_volume),
        ("collapse_radius", &mut bash.collapse_radius),
    ] {
        apply_i32(object, field, target, source)?;
    }
    for (field, target) in [
        ("destroy_only", &mut bash.destroy_only),
        ("bash_below", &mut bash.bash_below),
    ] {
        apply_bool(object, field, target, source)?;
    }
    for (field, target) in [
        ("profile", &mut bash.profile),
        ("sound", &mut bash.sound),
        ("sound_fail", &mut bash.failure_sound),
        ("ter_set", &mut bash.terrain_result),
        (
            "ter_set_bashed_from_above",
            &mut bash.terrain_result_bashed_from_above,
        ),
        ("furn_set", &mut bash.furniture_result),
    ] {
        apply_string(object, field, target, source)?;
    }
    if let Some(value) = object.get("hit_field") {
        bash.hit_field = parse_field_effect(value, source, "hit_field")?;
    }
    if let Some(value) = object.get("destroyed_field") {
        bash.destroyed_field = parse_field_effect(value, source, "destroyed_field")?;
    }
    if let Some(value) = object.get("tent_centers") {
        bash.tent_centers = string_array(value, source, "tent_centers")?;
    }
    if let Some(value) = object.get("items") {
        bash.unsupported_fields
            .retain(|field, _reason| field != "items" && !field.starts_with("items["));
        bash.drops = parse_drops(value, source, &mut bash.unsupported_fields)?;
    } else if !was_loaded {
        bash.drops.clear();
    }
    for field in object.keys() {
        if !field.starts_with("//") && !BASH_FIELDS.contains(&field.as_str()) {
            bash.unsupported_fields
                .insert(field.clone(), String::from("unsupported bash field"));
        }
    }
    if bash.str_min < -1
        || bash.str_max < -1
        || (bash.str_min >= 0 && bash.str_max < bash.str_min)
        || bash.sound_volume < -1
        || bash.failure_sound_volume < -1
    {
        return Err(format!("invalid bash bounds in {source}"));
    }
    *current = Some(bash);
    Ok(())
}

fn apply_i32(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    source: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        *target = i32::try_from(
            value
                .as_i64()
                .ok_or_else(|| format!("{field} must be an integer in {source}"))?,
        )
        .map_err(|_| format!("{field} is out of range in {source}"))?;
    }
    Ok(())
}

fn apply_bool(
    object: &Map<String, Value>,
    field: &str,
    target: &mut bool,
    source: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_bool()
            .ok_or_else(|| format!("{field} must be a boolean in {source}"))?;
    }
    Ok(())
}

fn apply_string(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), String> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{field} must be a non-empty string in {source}"))?
            .to_owned();
    }
    Ok(())
}

fn parse_field_effect(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Option<BashFieldEffectDefinition>, String> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| format!("{field} must be [field_id, intensity] in {source}"))?;
    let field_type_id = values[0]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} field id is invalid in {source}"))?;
    let intensity = u8::try_from(
        values[1]
            .as_u64()
            .filter(|value| *value > 0)
            .ok_or_else(|| format!("{field} intensity is invalid in {source}"))?,
    )
    .map_err(|_| format!("{field} intensity is out of range in {source}"))?;
    Ok(Some(BashFieldEffectDefinition {
        field_type_id: field_type_id.to_owned(),
        intensity,
    }))
}

fn string_array(value: &Value, source: &str, field: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{field} must be an array in {source}"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("{field} contains an invalid id in {source}"))
        })
        .collect()
}

fn parse_drops(
    value: &Value,
    source: &str,
    unsupported: &mut BTreeMap<String, String>,
) -> Result<Vec<BashDropDefinition>, String> {
    let Some(entries) = value.as_array() else {
        unsupported.insert(
            String::from("items"),
            String::from("bash item groups are not yet represented"),
        );
        return Ok(Vec::new());
    };
    let mut drops = Vec::new();
    for (index, value) in entries.iter().enumerate() {
        let entry = value
            .as_object()
            .ok_or_else(|| format!("items[{index}] must be an object in {source}"))?;
        for field in entry.keys() {
            if !matches!(field.as_str(), "item" | "prob" | "count" | "charges") {
                unsupported.insert(
                    format!("items[{index}].{field}"),
                    String::from("unsupported bash drop field"),
                );
            }
        }
        let Some(item_id) = entry.get("item").and_then(Value::as_str) else {
            unsupported.insert(
                format!("items[{index}]"),
                String::from("bash drop is not a direct item"),
            );
            continue;
        };
        let probability_percent = match entry.get("prob") {
            None => 100,
            Some(value) => u8::try_from(
                value
                    .as_u64()
                    .filter(|value| *value <= 100)
                    .ok_or_else(|| format!("items[{index}].prob is invalid in {source}"))?,
            )
            .map_err(|_| format!("items[{index}].prob is out of range in {source}"))?,
        };
        if entry.contains_key("count") && entry.contains_key("charges") {
            unsupported.insert(
                format!("items[{index}]"),
                String::from("count and charges cannot both be represented"),
            );
            continue;
        }
        let count = match entry.get("count") {
            None => (1, 1),
            Some(value) => match parse_u16_range(value, source, &format!("items[{index}].count")) {
                Ok(range) => range,
                Err(error) => {
                    unsupported.insert(format!("items[{index}].count"), error);
                    continue;
                }
            },
        };
        let charges = match entry.get("charges") {
            None => None,
            Some(value) => {
                match parse_i32_range(value, source, &format!("items[{index}].charges")) {
                    Ok(range) => Some(range),
                    Err(error) => {
                        unsupported.insert(format!("items[{index}].charges"), error);
                        continue;
                    }
                }
            }
        };
        drops.push(BashDropDefinition {
            item_id: item_id.to_owned(),
            probability_percent,
            count_min: count.0,
            count_max: count.1,
            charges_min: charges.map(|range| range.0),
            charges_max: charges.map(|range| range.1),
        });
    }
    Ok(drops)
}

fn parse_u16_range(value: &Value, source: &str, field: &str) -> Result<(u16, u16), String> {
    let (min, max) = parse_u64_range(value, source, field)?;
    let min = u16::try_from(min).map_err(|_| format!("{field} is out of range in {source}"))?;
    let max = u16::try_from(max).map_err(|_| format!("{field} is out of range in {source}"))?;
    Ok((min, max))
}

fn parse_i32_range(value: &Value, source: &str, field: &str) -> Result<(i32, i32), String> {
    let parse = |value: &Value| {
        i32::try_from(
            value
                .as_i64()
                .filter(|value| *value >= 0)
                .ok_or_else(|| format!("{field} is invalid in {source}"))?,
        )
        .map_err(|_| format!("{field} is out of range in {source}"))
    };
    let range = match value {
        Value::Array(values) if values.len() == 2 => (parse(&values[0])?, parse(&values[1])?),
        value => {
            let value = parse(value)?;
            (value, value)
        }
    };
    if range.0 > range.1 {
        return Err(format!("{field} has reversed bounds in {source}"));
    }
    Ok(range)
}

fn parse_u64_range(value: &Value, source: &str, field: &str) -> Result<(u64, u64), String> {
    let parse = |value: &Value| {
        value
            .as_u64()
            .ok_or_else(|| format!("{field} is invalid in {source}"))
    };
    let range = match value {
        Value::Array(values) if values.len() == 2 => (parse(&values[0])?, parse(&values[1])?),
        value => {
            let value = parse(value)?;
            (value, value)
        }
    };
    if range.0 > range.1 {
        return Err(format!("{field} has reversed bounds in {source}"));
    }
    Ok(range)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BashDamageProfileRegistry {
    profiles: BTreeMap<String, BashDamageProfileDefinition>,
}

impl BashDamageProfileRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, BashDamageProfileRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(BashDamageProfileRegistryError::Catalog)?;
        let mut profiles = BTreeMap::new();
        for file in files {
            read_file(content_root.as_ref(), &file, &mut profiles)?;
        }
        Ok(Self { profiles })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&BashDamageProfileDefinition> {
        self.profiles.get(id)
    }
}

fn read_file(
    root: &Path,
    file: &SelectedContentFile,
    profiles: &mut BTreeMap<String, BashDamageProfileDefinition>,
) -> Result<(), BashDamageProfileRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| BashDamageProfileRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| BashDamageProfileRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                collect_profile(file, value, profiles)?;
            }
        }
        value => collect_profile(file, value, profiles)?,
    }
    Ok(())
}

fn collect_profile(
    file: &SelectedContentFile,
    value: Value,
    profiles: &mut BTreeMap<String, BashDamageProfileDefinition>,
) -> Result<(), BashDamageProfileRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("bash_damage_profile") {
        return Ok(());
    }
    let object = value.as_object().ok_or_else(|| invalid(file, "object"))?;
    if object
        .keys()
        .any(|field| !field.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&field.as_str()))
    {
        return Err(invalid(file, "unsupported field"));
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid(file, "id"))?;
    let raw_profile = object
        .get("profile")
        .and_then(Value::as_object)
        .filter(|profile| !profile.is_empty())
        .ok_or_else(|| invalid(file, "profile"))?;
    let mut multipliers_millionths = BTreeMap::new();
    for (damage_type, multiplier) in raw_profile {
        if damage_type.is_empty()
            || damage_type.len() > 512
            || damage_type.chars().any(char::is_control)
        {
            return Err(invalid(file, "profile damage type"));
        }
        multipliers_millionths.insert(
            damage_type.clone(),
            decimal_millionths(multiplier).ok_or_else(|| invalid(file, "profile multiplier"))?,
        );
    }
    let definition = BashDamageProfileDefinition {
        id: id.to_owned(),
        multipliers_millionths,
        source: file.upstream_path.clone(),
    };
    if profiles.insert(id.to_owned(), definition).is_some() {
        return Err(BashDamageProfileRegistryError::DuplicateId(id.to_owned()));
    }
    Ok(())
}

fn decimal_millionths(value: &Value) -> Option<u32> {
    let text = value.as_number()?.to_string();
    if text.starts_with('-') || text.contains('e') || text.contains('E') {
        return None;
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    if fraction.len() > 6 || !whole.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    let whole = whole.parse::<u64>().ok()?;
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().ok()?
            * 10_u64.checked_pow(u32::try_from(6_usize.checked_sub(fraction.len())?).ok()?)?
    };
    let scaled = whole
        .checked_mul(u64::from(BASH_MULTIPLIER_SCALE))?
        .checked_add(fraction_value)?;
    u32::try_from(scaled).ok()
}

fn invalid(file: &SelectedContentFile, field: &str) -> BashDamageProfileRegistryError {
    BashDamageProfileRegistryError::InvalidDefinition {
        source: file.upstream_path.clone(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum BashDamageProfileRegistryError {
    Catalog(ModCatalogError),
    DuplicateId(String),
    InvalidDefinition { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
}

impl fmt::Display for BashDamageProfileRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "bash profile mod selection failed: {error}"),
            Self::DuplicateId(id) => write!(formatter, "duplicate bash damage profile `{id}`"),
            Self::InvalidDefinition { source, field } => {
                write!(
                    formatter,
                    "invalid bash damage profile field `{field}` in {source}"
                )
            }
            Self::Io(path, error) => write!(formatter, "failed to read {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "failed to parse {path}: {error}"),
        }
    }
}

impl std::error::Error for BashDamageProfileRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_multiplier_conversion_is_exact_and_bounded() {
        assert_eq!(decimal_millionths(&Value::from(1)), Some(1_000_000));
        assert_eq!(decimal_millionths(&Value::from(0.95)), Some(950_000));
        assert_eq!(decimal_millionths(&Value::from(1.2)), Some(1_200_000));
        assert_eq!(decimal_millionths(&Value::from(-0.1)), None);
    }
}
