use serde::{Deserialize, Serialize};

use crate::{SimTick, WorldPosition};

pub const MAX_WEATHER_TYPES: usize = 256;
pub const MAX_WEATHER_CONDITION_NODES: usize = 256;
pub const MAX_WEATHER_ID_BYTES: usize = 256;
pub const MAX_WEATHER_TEXT_BYTES: usize = 512;
pub const MAX_WEATHER_DURATION_SECONDS: u64 = 31 * 24 * 60 * 60;
pub const WEATHER_SCALE: i64 = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeatherPrecipitationV1 {
    None,
    VeryLight,
    Light,
    Heavy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeatherComparisonV1 {
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeatherMetricV1 {
    TemperatureMillikelvin,
    HumidityMillionths,
    PressureMillionths,
    WindpowerMillionths,
    DewPointFactorMillionths,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeatherConditionV1 {
    Always,
    IsDay,
    All(Vec<Self>),
    Any(Vec<Self>),
    Compare {
        metric: WeatherMetricV1,
        comparison: WeatherComparisonV1,
        value: i64,
    },
    /// Retained source expression. It deliberately evaluates false until a
    /// matching authoritative interpreter exists.
    Unsupported(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherTypeV1 {
    pub weather_type_id: String,
    pub name: String,
    pub symbol: String,
    pub sun_symbol: String,
    pub ranged_penalty: i32,
    pub sight_penalty_millionths: i64,
    pub light_modifier: i32,
    pub temperature_modifier_millikelvin: i32,
    pub light_multiplier_millionths: i64,
    pub sun_multiplier_millionths: i64,
    pub sound_attenuation: i32,
    pub dangerous: bool,
    pub precipitation: WeatherPrecipitationV1,
    pub rains: bool,
    pub priority: i32,
    pub required_weathers: Vec<String>,
    pub duration_min_seconds: u64,
    pub duration_max_seconds: u64,
    pub condition: WeatherConditionV1,
    pub has_unsupported_runtime_effects: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherGeneratorV1 {
    pub generator_id: String,
    pub base_temperature_millionths_celsius: i64,
    pub base_humidity_millionths: i64,
    pub base_pressure_millionths: i64,
    pub base_wind_millionths: i64,
    pub base_wind_distribution_peaks: i32,
    pub base_wind_season_variation: i32,
    /// Spring, summer, autumn, winter, matching pinned enum order.
    pub seasonal_temperature_modifiers: [i32; 4],
    pub seasonal_humidity_modifiers: [i32; 4],
    /// Weather-type indexes sorted by increasing priority. Equal priorities
    /// retain source factory order as projected by the server.
    pub sorted_weather_type_indexes: Vec<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherCatalogV1 {
    pub generator: WeatherGeneratorV1,
    pub weather_types: Vec<WeatherTypeV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherStateV1 {
    /// The position of the lowest stable-ID living player actor at the last
    /// transition. With no living player, the previous position is retained.
    /// This canonical multiplayer policy replaces upstream's process-global
    /// avatar reference without accepting a client-supplied observation point.
    pub reference_position: WorldPosition,
    pub weather_type_index: u16,
    pub temperature_millikelvin: i32,
    pub humidity_millionths: i64,
    pub pressure_millionths: i64,
    pub windpower_millionths: i64,
    pub wind_direction_degrees: i16,
    pub next_update_tick: SimTick,
    pub update_sequence: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeatherTemperatureBandV1 {
    Frigid,
    Cold,
    Cool,
    Mild,
    Warm,
    Hot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum WeatherWindBandV1 {
    Calm,
    Light,
    Moderate,
    Strong,
    Gale,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WeatherObservationV1 {
    pub weather_type_id: String,
    pub name: String,
    pub symbol: String,
    pub dangerous: bool,
    pub precipitation: WeatherPrecipitationV1,
    pub rains: bool,
    /// Human-sensible bands are public. Instrument-only precise atmospheric
    /// values remain canonical server state and never enter replication.
    pub temperature_band: WeatherTemperatureBandV1,
    pub wind_band: WeatherWindBandV1,
    pub effective_sight_radius: u16,
}

#[must_use]
pub fn weather_catalog_is_valid(catalog: &WeatherCatalogV1) -> bool {
    valid_id(&catalog.generator.generator_id)
        && catalog.generator.base_humidity_millionths >= 0
        && catalog.generator.base_humidity_millionths <= 100 * WEATHER_SCALE
        && catalog.generator.base_pressure_millionths > 0
        && catalog.generator.base_wind_millionths >= 0
        && catalog.generator.base_wind_distribution_peaks >= 9
        && !catalog.weather_types.is_empty()
        && catalog.weather_types.len() <= MAX_WEATHER_TYPES
        && catalog
            .weather_types
            .iter()
            .enumerate()
            .all(|(index, weather)| {
                weather_type_is_valid(weather)
                    && !catalog.weather_types[..index]
                        .iter()
                        .any(|other| other.weather_type_id == weather.weather_type_id)
                    && weather.required_weathers.iter().all(|required| {
                        catalog
                            .weather_types
                            .iter()
                            .any(|candidate| candidate.weather_type_id == *required)
                    })
            })
        && catalog
            .weather_types
            .iter()
            .any(|weather| weather.weather_type_id == "clear")
        && catalog
            .weather_types
            .iter()
            .any(|weather| weather.weather_type_id == "null")
        && !catalog.generator.sorted_weather_type_indexes.is_empty()
        && catalog.generator.sorted_weather_type_indexes.len() <= catalog.weather_types.len()
        && catalog
            .generator
            .sorted_weather_type_indexes
            .iter()
            .enumerate()
            .all(|(index, weather_index)| {
                usize::from(*weather_index) < catalog.weather_types.len()
                    && !catalog.generator.sorted_weather_type_indexes[..index]
                        .contains(weather_index)
            })
}

#[must_use]
pub fn weather_state_is_valid(state: &WeatherStateV1, catalog: &WeatherCatalogV1) -> bool {
    usize::from(state.weather_type_index) < catalog.weather_types.len()
        && state.humidity_millionths >= 0
        && state.humidity_millionths <= 100 * WEATHER_SCALE
        && state.pressure_millionths > 0
        && state.windpower_millionths >= 0
        && (0..360).contains(&state.wind_direction_degrees)
        && state.next_update_tick.0 > 0
        && state.update_sequence > 0
}

#[must_use]
pub fn weather_observation_is_valid(observation: &WeatherObservationV1) -> bool {
    valid_id(&observation.weather_type_id)
        && valid_text(&observation.name)
        && valid_text(&observation.symbol)
        && observation.effective_sight_radius <= 60
}

fn weather_type_is_valid(weather: &WeatherTypeV1) -> bool {
    valid_id(&weather.weather_type_id)
        && valid_text(&weather.name)
        && valid_text(&weather.symbol)
        && valid_text(&weather.sun_symbol)
        && weather.sight_penalty_millionths > 0
        && weather.light_multiplier_millionths >= 0
        && weather.sun_multiplier_millionths >= 0
        && weather.duration_min_seconds > 0
        && weather.duration_min_seconds <= weather.duration_max_seconds
        && weather.duration_max_seconds <= MAX_WEATHER_DURATION_SECONDS
        && weather.required_weathers.len() <= MAX_WEATHER_TYPES
        && weather.required_weathers.iter().all(|id| valid_id(id))
        && weather_condition_is_valid(&weather.condition, 0, &mut 0)
}

fn weather_condition_is_valid(
    condition: &WeatherConditionV1,
    depth: usize,
    nodes: &mut usize,
) -> bool {
    if depth > 16 || *nodes >= MAX_WEATHER_CONDITION_NODES {
        return false;
    }
    *nodes += 1;
    match condition {
        WeatherConditionV1::Always | WeatherConditionV1::IsDay => true,
        WeatherConditionV1::All(children) | WeatherConditionV1::Any(children) => {
            !children.is_empty()
                && children
                    .iter()
                    .all(|child| weather_condition_is_valid(child, depth + 1, nodes))
        }
        WeatherConditionV1::Compare { .. } => true,
        WeatherConditionV1::Unsupported(source) => valid_text(source),
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_WEATHER_ID_BYTES && !value.chars().any(char::is_control)
}

fn valid_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_WEATHER_TEXT_BYTES
        && !value.chars().any(char::is_control)
}
