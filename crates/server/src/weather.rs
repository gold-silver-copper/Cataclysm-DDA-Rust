use cdda_content::{
    WeatherComparisonDefinition, WeatherConditionDefinition, WeatherMetricDefinition,
    WeatherPrecipitationDefinition, WeatherRegistry,
};
use cdda_protocol::{
    WeatherCatalogV1, WeatherComparisonV1, WeatherConditionV1, WeatherGeneratorV1, WeatherMetricV1,
    WeatherPrecipitationV1, WeatherTypeV1,
};

pub(crate) fn runtime_weather_catalog(
    registry: &WeatherRegistry,
    region_id: &str,
) -> Result<WeatherCatalogV1, Box<dyn std::error::Error>> {
    let generator = registry
        .generator_for_region(region_id)
        .ok_or_else(|| format!("pinned content is missing weather for region {region_id}"))?;
    let mut definitions = registry.weather_types().collect::<Vec<_>>();
    definitions.sort_by_key(|definition| definition.load_order);
    let weather_types = definitions
        .iter()
        .map(|definition| WeatherTypeV1 {
            weather_type_id: definition.id.clone(),
            name: definition.name.clone(),
            symbol: definition.symbol.clone(),
            sun_symbol: definition.sun_symbol.clone(),
            ranged_penalty: definition.ranged_penalty,
            sight_penalty_millionths: definition.sight_penalty_millionths,
            light_modifier: definition.light_modifier,
            temperature_modifier_millikelvin: definition.temperature_modifier_millikelvin,
            light_multiplier_millionths: definition.light_multiplier_millionths,
            sun_multiplier_millionths: definition.sun_multiplier_millionths,
            sound_attenuation: definition.sound_attenuation,
            dangerous: definition.dangerous,
            precipitation: match definition.precipitation {
                WeatherPrecipitationDefinition::None => WeatherPrecipitationV1::None,
                WeatherPrecipitationDefinition::VeryLight => WeatherPrecipitationV1::VeryLight,
                WeatherPrecipitationDefinition::Light => WeatherPrecipitationV1::Light,
                WeatherPrecipitationDefinition::Heavy => WeatherPrecipitationV1::Heavy,
            },
            rains: definition.rains,
            priority: definition.priority,
            required_weathers: definition.required_weathers.clone(),
            duration_min_seconds: definition.duration_min_seconds,
            duration_max_seconds: definition.duration_max_seconds,
            condition: project_condition(&definition.condition),
            has_unsupported_runtime_effects: !definition.unsupported_fields.is_empty(),
        })
        .collect::<Vec<_>>();
    let mut sorted_weather_type_indexes = definitions
        .iter()
        .enumerate()
        .filter(|(_, definition)| {
            if generator.weather_white_list.is_empty() {
                !generator.weather_black_list.contains(&definition.id)
            } else {
                definition.id == "clear" || generator.weather_white_list.contains(&definition.id)
            }
        })
        .map(|(index, _)| u16::try_from(index))
        .collect::<Result<Vec<_>, _>>()?;
    sorted_weather_type_indexes.sort_by_key(|index| weather_types[usize::from(*index)].priority);
    let catalog = WeatherCatalogV1 {
        generator: WeatherGeneratorV1 {
            generator_id: generator.id.clone(),
            base_temperature_millionths_celsius: generator.base_temperature_millionths_celsius,
            base_humidity_millionths: generator.base_humidity_millionths,
            base_pressure_millionths: generator.base_pressure_millionths,
            base_wind_millionths: generator.base_wind_millionths,
            base_wind_distribution_peaks: generator.base_wind_distribution_peaks,
            base_wind_season_variation: generator.base_wind_season_variation,
            seasonal_temperature_modifiers: generator.seasonal_temperature_modifiers,
            seasonal_humidity_modifiers: generator.seasonal_humidity_modifiers,
            sorted_weather_type_indexes,
        },
        weather_types,
    };
    if !cdda_protocol::weather_catalog_is_valid(&catalog) {
        return Err("projected weather catalog is invalid".into());
    }
    Ok(catalog)
}

fn project_condition(condition: &WeatherConditionDefinition) -> WeatherConditionV1 {
    match condition {
        WeatherConditionDefinition::Always => WeatherConditionV1::Always,
        WeatherConditionDefinition::IsDay => WeatherConditionV1::IsDay,
        WeatherConditionDefinition::All(children) => {
            WeatherConditionV1::All(children.iter().map(project_condition).collect())
        }
        WeatherConditionDefinition::Any(children) => {
            WeatherConditionV1::Any(children.iter().map(project_condition).collect())
        }
        WeatherConditionDefinition::Compare {
            metric,
            comparison,
            value,
        } => WeatherConditionV1::Compare {
            metric: match metric {
                WeatherMetricDefinition::TemperatureMillikelvin => {
                    WeatherMetricV1::TemperatureMillikelvin
                }
                WeatherMetricDefinition::HumidityMillionths => WeatherMetricV1::HumidityMillionths,
                WeatherMetricDefinition::PressureMillionths => WeatherMetricV1::PressureMillionths,
                WeatherMetricDefinition::WindpowerMillionths => {
                    WeatherMetricV1::WindpowerMillionths
                }
                WeatherMetricDefinition::DewPointFactorMillionths => {
                    WeatherMetricV1::DewPointFactorMillionths
                }
            },
            comparison: match comparison {
                WeatherComparisonDefinition::Less => WeatherComparisonV1::Less,
                WeatherComparisonDefinition::LessOrEqual => WeatherComparisonV1::LessOrEqual,
                WeatherComparisonDefinition::Greater => WeatherComparisonV1::Greater,
                WeatherComparisonDefinition::GreaterOrEqual => WeatherComparisonV1::GreaterOrEqual,
            },
            value: *value,
        },
        WeatherConditionDefinition::Unsupported(source) => {
            WeatherConditionV1::Unsupported(source.clone())
        }
    }
}
