use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const WEATHER_SCALE: i64 = 1_000_000;
pub const MAX_WEATHER_TYPES: usize = 256;
pub const MAX_WEATHER_GENERATORS: usize = 64;
pub const MAX_WEATHER_CONDITION_DEPTH: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherPrecipitationDefinition {
    None,
    VeryLight,
    Light,
    Heavy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherComparisonDefinition {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeatherMetricDefinition {
    TemperatureMillikelvin,
    HumidityMillionths,
    PressureMillionths,
    WindpowerMillionths,
    DewPointFactorMillionths,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WeatherConditionDefinition {
    Always,
    IsDay,
    All(Vec<Self>),
    Any(Vec<Self>),
    Compare {
        metric: WeatherMetricDefinition,
        comparison: WeatherComparisonDefinition,
        value: i64,
    },
    Unsupported(String),
}

impl WeatherConditionDefinition {
    #[must_use]
    pub fn is_supported(&self) -> bool {
        match self {
            Self::Always | Self::IsDay | Self::Compare { .. } => true,
            Self::All(children) | Self::Any(children) => {
                !children.is_empty() && children.iter().all(Self::is_supported)
            }
            Self::Unsupported(_) => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeatherTypeDefinition {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub sun_symbol: String,
    pub color: String,
    pub map_color: String,
    pub ranged_penalty: i32,
    pub sight_penalty_millionths: i64,
    pub light_modifier: i32,
    pub temperature_modifier_millikelvin: i32,
    pub light_multiplier_millionths: i64,
    pub sun_multiplier_millionths: i64,
    pub sound_attenuation: i32,
    pub dangerous: bool,
    pub precipitation: WeatherPrecipitationDefinition,
    pub rains: bool,
    pub priority: i32,
    pub required_weathers: Vec<String>,
    pub duration_min_seconds: u64,
    pub duration_max_seconds: u64,
    pub condition: WeatherConditionDefinition,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
    pub load_order: u32,
}

impl Default for WeatherTypeDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            symbol: "%".to_owned(),
            sun_symbol: "☼".to_owned(),
            color: "white".to_owned(),
            map_color: "white".to_owned(),
            ranged_penalty: 0,
            sight_penalty_millionths: 0,
            light_modifier: 0,
            temperature_modifier_millikelvin: 0,
            light_multiplier_millionths: WEATHER_SCALE,
            sun_multiplier_millionths: WEATHER_SCALE,
            sound_attenuation: 0,
            dangerous: false,
            precipitation: WeatherPrecipitationDefinition::None,
            rains: false,
            priority: 0,
            required_weathers: Vec::new(),
            duration_min_seconds: 5 * 60,
            duration_max_seconds: 5 * 60,
            condition: WeatherConditionDefinition::Always,
            unsupported_fields: BTreeSet::new(),
            source: String::new(),
            load_order: u32::MAX,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WeatherGeneratorDefinition {
    pub id: String,
    pub base_temperature_millionths_celsius: i64,
    pub base_humidity_millionths: i64,
    pub base_pressure_millionths: i64,
    pub base_wind_millionths: i64,
    pub base_wind_distribution_peaks: i32,
    pub base_wind_season_variation: i32,
    pub seasonal_temperature_modifiers: [i32; 4],
    pub seasonal_humidity_modifiers: [i32; 4],
    pub weather_black_list: Vec<String>,
    pub weather_white_list: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WeatherRegistry {
    weather_types: BTreeMap<String, WeatherTypeDefinition>,
    generators: BTreeMap<String, WeatherGeneratorDefinition>,
    region_generators: BTreeMap<String, String>,
}

#[derive(Clone)]
struct RawWeather {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl WeatherRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, WeatherRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(WeatherRegistryError::Catalog)?;
        let (mut weather, mut generators, region_generators) =
            read_weather(content_root.as_ref(), files)?;
        let mut weather_types = BTreeMap::new();
        resolve_weather_types(&mut weather, &mut weather_types)?;
        let mut resolved_generators = BTreeMap::new();
        resolve_generators(&mut generators, &mut resolved_generators)?;
        if weather_types.len() > MAX_WEATHER_TYPES
            || resolved_generators.len() > MAX_WEATHER_GENERATORS
            || !weather_types.contains_key("clear")
            || !weather_types.contains_key("null")
        {
            return Err(WeatherRegistryError::InvalidRegistry);
        }
        for definition in weather_types.values() {
            if definition
                .required_weathers
                .iter()
                .any(|id| !weather_types.contains_key(id))
            {
                return Err(WeatherRegistryError::UnknownWeatherReference(
                    definition.id.clone(),
                ));
            }
        }
        if region_generators
            .values()
            .any(|generator_id| !resolved_generators.contains_key(generator_id))
        {
            return Err(WeatherRegistryError::InvalidRegistry);
        }
        Ok(Self {
            weather_types,
            generators: resolved_generators,
            region_generators,
        })
    }

    pub fn weather_types(&self) -> impl Iterator<Item = &WeatherTypeDefinition> {
        self.weather_types.values()
    }

    #[must_use]
    pub fn generator(&self, id: &str) -> Option<&WeatherGeneratorDefinition> {
        self.generators.get(id)
    }

    #[must_use]
    pub fn generator_for_region(&self, region_id: &str) -> Option<&WeatherGeneratorDefinition> {
        self.region_generators
            .get(region_id)
            .and_then(|generator_id| self.generator(generator_id))
    }
}

fn read_weather(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<
    (
        VecDeque<RawWeather>,
        VecDeque<RawWeather>,
        BTreeMap<String, String>,
    ),
    WeatherRegistryError,
> {
    let mut weather = VecDeque::new();
    let mut generators = VecDeque::new();
    let mut region_generators = BTreeMap::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| WeatherRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| WeatherRegistryError::Json(file.destination.clone(), error))?;
        let values = match value {
            Value::Array(values) => values,
            value => vec![value],
        };
        for value in values {
            let Some(object) = value.as_object() else {
                continue;
            };
            let raw = RawWeather {
                file: file.clone(),
                object: object.clone(),
            };
            match object.get("type").and_then(Value::as_str) {
                Some("weather_type") => weather.push_back(raw),
                Some("weather_generator") => generators.push_back(raw),
                Some("region_settings") => {
                    if let (Some(region_id), Some(generator_id)) = (
                        object.get("id").and_then(Value::as_str),
                        object.get("weather").and_then(Value::as_str),
                    ) {
                        region_generators.insert(region_id.to_owned(), generator_id.to_owned());
                    }
                }
                _ => {}
            }
        }
    }
    Ok((weather, generators, region_generators))
}

fn resolve_weather_types(
    pending: &mut VecDeque<RawWeather>,
    output: &mut BTreeMap<String, WeatherTypeDefinition>,
) -> Result<(), WeatherRegistryError> {
    while !pending.is_empty() {
        let pass = pending.len();
        let mut loaded = 0;
        for _ in 0..pass {
            let raw = pending
                .pop_front()
                .ok_or(WeatherRegistryError::InvalidRegistry)?;
            let inherited = raw.object.get("copy-from").and_then(Value::as_str);
            if inherited.is_some_and(|id| !output.contains_key(id)) {
                pending.push_back(raw);
                continue;
            }
            let raw_id = raw.object.get("id").and_then(Value::as_str);
            let was_loaded = raw_id.is_some_and(|id| output.contains_key(id))
                || inherited.is_some_and(|id| output.contains_key(id));
            let mut definition = raw_id
                .and_then(|id| output.get(id).cloned())
                .or_else(|| inherited.and_then(|id| output.get(id).cloned()))
                .unwrap_or_default();
            patch_weather_type(&raw, &mut definition, was_loaded)?;
            definition.load_order = output.get(&definition.id).map_or(
                u32::try_from(output.len()).map_err(|_| WeatherRegistryError::InvalidRegistry)?,
                |existing| existing.load_order,
            );
            output.insert(definition.id.clone(), definition);
            loaded += 1;
        }
        if loaded == 0 {
            return Err(WeatherRegistryError::UnresolvedInheritance);
        }
    }
    Ok(())
}

fn resolve_generators(
    pending: &mut VecDeque<RawWeather>,
    output: &mut BTreeMap<String, WeatherGeneratorDefinition>,
) -> Result<(), WeatherRegistryError> {
    while !pending.is_empty() {
        let pass = pending.len();
        let mut loaded = 0;
        for _ in 0..pass {
            let raw = pending
                .pop_front()
                .ok_or(WeatherRegistryError::InvalidRegistry)?;
            let inherited = raw.object.get("copy-from").and_then(Value::as_str);
            if inherited.is_some_and(|id| !output.contains_key(id)) {
                pending.push_back(raw);
                continue;
            }
            let raw_id = raw.object.get("id").and_then(Value::as_str);
            let was_loaded = raw_id.is_some_and(|id| output.contains_key(id))
                || inherited.is_some_and(|id| output.contains_key(id));
            let mut definition = raw_id
                .and_then(|id| output.get(id).cloned())
                .or_else(|| inherited.and_then(|id| output.get(id).cloned()))
                .unwrap_or_default();
            patch_generator(&raw, &mut definition, was_loaded)?;
            output.insert(definition.id.clone(), definition);
            loaded += 1;
        }
        if loaded == 0 {
            return Err(WeatherRegistryError::UnresolvedInheritance);
        }
    }
    Ok(())
}

fn patch_weather_type(
    raw: &RawWeather,
    definition: &mut WeatherTypeDefinition,
    was_loaded: bool,
) -> Result<(), WeatherRegistryError> {
    let source = raw.file.upstream_path.as_str();
    definition.id = text(&raw.object, "id", source)?.to_owned();
    require_fields(
        &raw.object,
        was_loaded,
        &[
            "name",
            "sym",
            "ranged_penalty",
            "sight_penalty",
            "light_modifier",
            "priority",
            "sound_attn",
            "dangerous",
            "precip",
            "rains",
        ],
        source,
    )?;
    if let Some(value) = raw.object.get("name") {
        definition.name = translation(value, source)?;
    }
    patch_text(&raw.object, "sym", &mut definition.symbol, source)?;
    patch_text(&raw.object, "sun_sym", &mut definition.sun_symbol, source)?;
    patch_text(&raw.object, "color", &mut definition.color, source)?;
    patch_text(&raw.object, "map_color", &mut definition.map_color, source)?;
    patch_i32(
        &raw.object,
        "ranged_penalty",
        &mut definition.ranged_penalty,
        source,
    )?;
    patch_fixed(
        &raw.object,
        "sight_penalty",
        &mut definition.sight_penalty_millionths,
        source,
    )?;
    patch_i32(
        &raw.object,
        "light_modifier",
        &mut definition.light_modifier,
        source,
    )?;
    if let Some(value) = raw.object.get("temperature_modifier") {
        definition.temperature_modifier_millikelvin =
            temperature_delta_millikelvin(value, source, "temperature_modifier")?;
    }
    patch_fixed(
        &raw.object,
        "light_multiplier",
        &mut definition.light_multiplier_millionths,
        source,
    )?;
    patch_fixed(
        &raw.object,
        "sun_multiplier",
        &mut definition.sun_multiplier_millionths,
        source,
    )?;
    patch_i32(
        &raw.object,
        "sound_attn",
        &mut definition.sound_attenuation,
        source,
    )?;
    patch_bool(&raw.object, "dangerous", &mut definition.dangerous, source)?;
    patch_bool(&raw.object, "rains", &mut definition.rains, source)?;
    patch_i32(&raw.object, "priority", &mut definition.priority, source)?;
    if let Some(value) = raw.object.get("precip") {
        definition.precipitation = match value.as_str() {
            Some("none") => WeatherPrecipitationDefinition::None,
            Some("very_light") => WeatherPrecipitationDefinition::VeryLight,
            Some("light") => WeatherPrecipitationDefinition::Light,
            Some("heavy") => WeatherPrecipitationDefinition::Heavy,
            _ => return Err(invalid(source, "precip")),
        };
    }
    if let Some(value) = raw.object.get("required_weathers") {
        definition.required_weathers = string_array(value, source, "required_weathers")?;
    }
    if let Some(value) = raw.object.get("duration_min") {
        definition.duration_min_seconds = duration_seconds(value, source, "duration_min")?;
    }
    if let Some(value) = raw.object.get("duration_max") {
        definition.duration_max_seconds = duration_seconds(value, source, "duration_max")?;
    }
    if let Some(value) = raw.object.get("condition") {
        definition.condition = parse_condition(value, source, 0)?;
    } else {
        // `read_condition(..., true)` resets an absent inherited condition to
        // the pinned default predicate instead of retaining the source one.
        definition.condition = WeatherConditionDefinition::Always;
    }
    for field in ["passive_effects", "debug_cause_eoc", "debug_leave_eoc"] {
        if raw.object.contains_key(field) {
            definition.unsupported_fields.insert(field.to_owned());
        }
    }
    if !definition.condition.is_supported() {
        definition.unsupported_fields.insert("condition".to_owned());
    } else {
        definition.unsupported_fields.remove("condition");
    }
    if definition.id.is_empty()
        || definition.name.is_empty()
        || definition.symbol.is_empty()
        || definition.duration_min_seconds == 0
        || definition.duration_min_seconds > definition.duration_max_seconds
    {
        return Err(invalid(source, "weather identity"));
    }
    definition.source = source.to_owned();
    Ok(())
}

fn patch_generator(
    raw: &RawWeather,
    definition: &mut WeatherGeneratorDefinition,
    was_loaded: bool,
) -> Result<(), WeatherRegistryError> {
    let source = raw.file.upstream_path.as_str();
    definition.id = text(&raw.object, "id", source)?.to_owned();
    require_fields(
        &raw.object,
        was_loaded,
        &[
            "base_temperature",
            "base_humidity",
            "base_pressure",
            "base_wind",
        ],
        source,
    )?;
    patch_fixed(
        &raw.object,
        "base_temperature",
        &mut definition.base_temperature_millionths_celsius,
        source,
    )?;
    patch_fixed(
        &raw.object,
        "base_humidity",
        &mut definition.base_humidity_millionths,
        source,
    )?;
    patch_fixed(
        &raw.object,
        "base_pressure",
        &mut definition.base_pressure_millionths,
        source,
    )?;
    patch_fixed(
        &raw.object,
        "base_wind",
        &mut definition.base_wind_millionths,
        source,
    )?;
    patch_i32(
        &raw.object,
        "base_wind_distrib_peaks",
        &mut definition.base_wind_distribution_peaks,
        source,
    )?;
    patch_i32(
        &raw.object,
        "base_wind_season_variation",
        &mut definition.base_wind_season_variation,
        source,
    )?;
    for (field, index) in [
        ("spring_temp_manual_mod", 0),
        ("summer_temp_manual_mod", 1),
        ("autumn_temp_manual_mod", 2),
        ("winter_temp_manual_mod", 3),
    ] {
        patch_i32(
            &raw.object,
            field,
            &mut definition.seasonal_temperature_modifiers[index],
            source,
        )?;
    }
    for (field, index) in [
        ("spring_humidity_manual_mod", 0),
        ("summer_humidity_manual_mod", 1),
        ("autumn_humidity_manual_mod", 2),
        ("winter_humidity_manual_mod", 3),
    ] {
        patch_i32(
            &raw.object,
            field,
            &mut definition.seasonal_humidity_modifiers[index],
            source,
        )?;
    }
    if let Some(value) = raw.object.get("weather_black_list") {
        definition.weather_black_list = string_array(value, source, "weather_black_list")?;
    }
    if let Some(value) = raw.object.get("weather_white_list") {
        definition.weather_white_list = string_array(value, source, "weather_white_list")?;
    }
    if !definition.weather_black_list.is_empty() && !definition.weather_white_list.is_empty() {
        return Err(invalid(source, "weather lists"));
    }
    if definition.id.is_empty() || definition.base_wind_millionths < 0 {
        return Err(invalid(source, "weather generator"));
    }
    definition.source = source.to_owned();
    Ok(())
}

fn parse_condition(
    value: &Value,
    source: &str,
    depth: usize,
) -> Result<WeatherConditionDefinition, WeatherRegistryError> {
    if depth >= MAX_WEATHER_CONDITION_DEPTH {
        return Err(invalid(source, "condition depth"));
    }
    if value.as_str() == Some("is_day") {
        return Ok(WeatherConditionDefinition::IsDay);
    }
    let Some(object) = value.as_object() else {
        return Ok(WeatherConditionDefinition::Unsupported(value.to_string()));
    };
    if let Some(children) = object.get("and").and_then(Value::as_array) {
        return children
            .iter()
            .map(|child| parse_condition(child, source, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(WeatherConditionDefinition::All);
    }
    if let Some(children) = object.get("or").and_then(Value::as_array) {
        return children
            .iter()
            .map(|child| parse_condition(child, source, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(WeatherConditionDefinition::Any);
    }
    if let Some(expression) = object
        .get("math")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
    {
        return Ok(parse_weather_math(expression)
            .unwrap_or_else(|| WeatherConditionDefinition::Unsupported(expression.to_owned())));
    }
    Ok(WeatherConditionDefinition::Unsupported(value.to_string()))
}

fn parse_weather_math(expression: &str) -> Option<WeatherConditionDefinition> {
    let compact = expression
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    for (operator, comparison) in [
        (">=", WeatherComparisonDefinition::GreaterOrEqual),
        ("<=", WeatherComparisonDefinition::LessOrEqual),
        (">", WeatherComparisonDefinition::Greater),
        ("<", WeatherComparisonDefinition::Less),
    ] {
        let Some((left, right)) = compact.split_once(operator) else {
            continue;
        };
        let (metric, value) = match left {
            "weather('temperature')" => (
                WeatherMetricDefinition::TemperatureMillikelvin,
                parse_fahrenheit(right)?,
            ),
            "weather('humidity')" => (
                WeatherMetricDefinition::HumidityMillionths,
                parse_decimal_fixed(right)?,
            ),
            "weather('pressure')" => (
                WeatherMetricDefinition::PressureMillionths,
                parse_decimal_fixed(right)?,
            ),
            "weather('windpower')" => (
                WeatherMetricDefinition::WindpowerMillionths,
                parse_decimal_fixed(right)?,
            ),
            "dew_point_factor(celsius(weather('temperature')))" => (
                WeatherMetricDefinition::DewPointFactorMillionths,
                parse_decimal_fixed(right)?,
            ),
            _ => continue,
        };
        return Some(WeatherConditionDefinition::Compare {
            metric,
            comparison,
            value,
        });
    }
    None
}

fn parse_fahrenheit(value: &str) -> Option<i64> {
    let inner = value.strip_prefix("from_fahrenheit(")?.strip_suffix(')')?;
    let fahrenheit = inner.trim().parse::<f64>().ok()?;
    Some((((fahrenheit - 32.0) * 5.0 / 9.0 + 273.15) * 1_000.0) as i64)
}

fn parse_decimal_fixed(value: &str) -> Option<i64> {
    let number = value.parse::<f64>().ok()?;
    if !number.is_finite() {
        return None;
    }
    Some((number * WEATHER_SCALE as f64) as i64)
}

fn text<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, WeatherRegistryError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(source, field))
}

fn require_fields(
    object: &Map<String, Value>,
    was_loaded: bool,
    fields: &[&str],
    source: &str,
) -> Result<(), WeatherRegistryError> {
    if was_loaded {
        return Ok(());
    }
    for field in fields {
        if !object.contains_key(*field) {
            return Err(invalid(source, field));
        }
    }
    Ok(())
}

fn temperature_delta_millikelvin(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<i32, WeatherRegistryError> {
    let delta_kelvin = if let Some(number) = value.as_f64() {
        number
    } else {
        let text = value.as_str().ok_or_else(|| invalid(source, field))?;
        let (number, unit) = text
            .trim()
            .split_once(' ')
            .ok_or_else(|| invalid(source, field))?;
        let number = number.parse::<f64>().map_err(|_| invalid(source, field))?;
        match unit {
            "C" | "K" => number,
            "F" => number * 5.0 / 9.0,
            _ => return Err(invalid(source, field)),
        }
    };
    if !delta_kelvin.is_finite() {
        return Err(invalid(source, field));
    }
    i32::try_from((delta_kelvin * 1_000.0) as i64).map_err(|_| invalid(source, field))
}

fn translation(value: &Value, source: &str) -> Result<String, WeatherRegistryError> {
    value
        .as_str()
        .or_else(|| value.get("str").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| invalid(source, "name"))
}

fn patch_text(
    object: &Map<String, Value>,
    field: &str,
    output: &mut String,
    source: &str,
) -> Result<(), WeatherRegistryError> {
    if let Some(value) = object.get(field) {
        *output = value
            .as_str()
            .ok_or_else(|| invalid(source, field))?
            .to_owned();
    }
    Ok(())
}

fn patch_i32(
    object: &Map<String, Value>,
    field: &str,
    output: &mut i32,
    source: &str,
) -> Result<(), WeatherRegistryError> {
    if let Some(value) = object.get(field) {
        *output = i32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))?;
    }
    Ok(())
}

fn patch_fixed(
    object: &Map<String, Value>,
    field: &str,
    output: &mut i64,
    source: &str,
) -> Result<(), WeatherRegistryError> {
    if let Some(value) = object.get(field) {
        let number = value.as_f64().ok_or_else(|| invalid(source, field))?;
        if !number.is_finite() {
            return Err(invalid(source, field));
        }
        *output = (number * WEATHER_SCALE as f64) as i64;
    }
    Ok(())
}

fn patch_bool(
    object: &Map<String, Value>,
    field: &str,
    output: &mut bool,
    source: &str,
) -> Result<(), WeatherRegistryError> {
    if let Some(value) = object.get(field) {
        *output = value.as_bool().ok_or_else(|| invalid(source, field))?;
    }
    Ok(())
}

fn string_array(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Vec<String>, WeatherRegistryError> {
    let values = value.as_array().ok_or_else(|| invalid(source, field))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .collect()
}

fn duration_seconds(value: &Value, source: &str, field: &str) -> Result<u64, WeatherRegistryError> {
    if let Some(turns) = value.as_u64() {
        return Ok(turns);
    }
    let text = value.as_str().ok_or_else(|| invalid(source, field))?;
    let (number, unit) = text
        .trim()
        .split_once(' ')
        .ok_or_else(|| invalid(source, field))?;
    let amount = number.parse::<u64>().map_err(|_| invalid(source, field))?;
    let multiplier = match unit.trim_end_matches('s') {
        "turn" | "second" => 1,
        "minute" => 60,
        "hour" => 60 * 60,
        "day" => 24 * 60 * 60,
        _ => return Err(invalid(source, field)),
    };
    amount
        .checked_mul(multiplier)
        .ok_or_else(|| invalid(source, field))
}

fn invalid(source: &str, field: &str) -> WeatherRegistryError {
    WeatherRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum WeatherRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    InvalidField { source: String, field: String },
    UnresolvedInheritance,
    UnknownWeatherReference(String),
    InvalidRegistry,
}

impl fmt::Display for WeatherRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "weather catalog selection failed: {error}"),
            Self::Io(path, error) => write!(formatter, "weather read failed for {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "weather JSON failed for {path}: {error}"),
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid weather field {field} in {source}")
            }
            Self::UnresolvedInheritance => {
                formatter.write_str("weather inheritance could not be resolved")
            }
            Self::UnknownWeatherReference(id) => {
                write!(formatter, "weather {id} references an unknown weather type")
            }
            Self::InvalidRegistry => {
                formatter.write_str("weather registry is invalid or exceeds bounds")
            }
        }
    }
}

impl std::error::Error for WeatherRegistryError {}
