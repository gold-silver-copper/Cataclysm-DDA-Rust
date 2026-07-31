use cdda_protocol::{
    CalendarSnapshot, NaturalLightSnapshot, Season, SimTick, SkyPhase, WEATHER_SCALE,
    WeatherCatalogV1, WeatherComparisonV1, WeatherConditionV1, WeatherMetricV1,
    WeatherObservationV1, WeatherStateV1,
};
use rand_core::Rng;

use crate::SimError;

const SECONDS_PER_DAY: f64 = 86_400.0;
const DAYS_PER_YEAR: f64 = 364.0;
const START_DAY_FROM_TURN_ZERO: f64 = 60.0 + 8.0 / 24.0;
const SIMPLEX_NOISE_RANDOM_SEED_LIMIT: u32 = 32_768;

pub(super) fn initial_weather_state(
    catalog: &WeatherCatalogV1,
    world_seed: [u8; 32],
) -> Result<WeatherStateV1, SimError> {
    calculate_weather(catalog, None, world_seed, SimTick(0), 1)
}

pub(super) fn advance_weather_state(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
    world_seed: [u8; 32],
    tick: SimTick,
) -> Result<Option<WeatherStateV1>, SimError> {
    if tick < state.next_update_tick {
        return Ok(None);
    }
    let sequence = state
        .update_sequence
        .checked_add(1)
        .ok_or(SimError::NumericOverflow)?;
    calculate_weather(catalog, Some(state), world_seed, tick, sequence).map(Some)
}

pub(super) fn weather_observation(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
) -> Option<WeatherObservationV1> {
    let weather = catalog
        .weather_types
        .get(usize::from(state.weather_type_index))?;
    Some(WeatherObservationV1 {
        weather_type_id: weather.weather_type_id.clone(),
        name: weather.name.clone(),
        symbol: weather.symbol.clone(),
        dangerous: weather.dangerous,
        precipitation: weather.precipitation,
        rains: weather.rains,
        temperature_millikelvin: state.temperature_millikelvin,
        humidity_millionths: state.humidity_millionths,
        pressure_millionths: state.pressure_millionths,
        windpower_millionths: state.windpower_millionths,
        wind_direction_degrees: state.wind_direction_degrees,
    })
}

fn calculate_weather(
    catalog: &WeatherCatalogV1,
    previous: Option<&WeatherStateV1>,
    world_seed: [u8; 32],
    tick: SimTick,
    sequence: u64,
) -> Result<WeatherStateV1, SimError> {
    let generator = &catalog.generator;
    let mut rng = weather_rng(world_seed, tick, sequence);
    let elapsed_days = tick.0 as f64 / SimTick::HZ as f64 / SECONDS_PER_DAY;
    let absolute_days = START_DAY_FROM_TURN_ZERO + elapsed_days;
    let year_fraction = ((absolute_days + 81.0) % DAYS_PER_YEAR) / DAYS_PER_YEAR;
    let offset = 35.5 / DAYS_PER_YEAR;
    let cyf = libm::cos(core::f64::consts::TAU * (year_fraction - offset));
    let seasonality = -cyf;
    let calendar = CalendarSnapshot::at_tick(tick);
    let season_index = season_index(calendar.season);
    let day_fraction = (f64::from(calendar.hour) * 3_600.0
        + f64::from(calendar.minute) * 60.0
        + f64::from(calendar.second))
        / SECONDS_PER_DAY;
    let day_variation = libm::cos(core::f64::consts::TAU * (day_fraction + 0.5 - 5.0 / 24.0));
    let x = previous.map_or(0.0, |state| f64::from(state.reference_position.x) / 2_000.0);
    let y = previous.map_or(0.0, |state| f64::from(state.reference_position.y) / 2_000.0);
    let z = absolute_days;
    let mod_seed = u32::from_be_bytes(
        world_seed[..4]
            .try_into()
            .map_err(|_| SimError::NumericOverflow)?,
    ) % SIMPLEX_NOISE_RANDOM_SEED_LIMIT;

    let baseline_celsius = generator.base_temperature_millionths_celsius as f64
        / WEATHER_SCALE as f64
        + f64::from(generator.seasonal_temperature_modifiers[season_index])
        + day_variation * (5.0 + 2.0 * (-seasonality + 1.0) / 2.0)
        + seasonality * 12.0;
    let temperature_celsius = baseline_celsius
        + f64::from(raw_noise_4d(x as f32, y as f32, z as f32, mod_seed as f32))
            * (1.0 + (1.0 - seasonality) * 0.5 / 2.0)
            * 6.0;
    let temperature_millikelvin = ((temperature_celsius + 273.15) * 1_000.0) as i32;

    let humidity = (generator.base_humidity_millionths as f64 / WEATHER_SCALE as f64
        + f64::from(generator.seasonal_humidity_modifiers[season_index])
        + 100.0
            * (0.15 * seasonality
                + f64::from(raw_noise_4d(
                    x as f32,
                    y as f32,
                    z as f32,
                    mod_seed.wrapping_add(101) as f32,
                )) * 0.2
                    * (-seasonality + 2.0)))
        .clamp(0.0, 100.0);
    let mut raw_wind_noise = f64::from(raw_noise_4d(
        (x / 2.5) as f32,
        (y / 2.5) as f32,
        (z / 200.0) as f32,
        mod_seed as f32,
    )) * 10.0;
    let pressure = generator.base_pressure_millionths as f64 / WEATHER_SCALE as f64
        + f64::from(raw_noise_4d(
            x as f32,
            y as f32,
            z as f32,
            mod_seed.wrapping_add(211) as f32,
        )) * 15.0
            * (-seasonality + 2.0);
    let variation = if generator.base_wind_season_variation == 0 {
        0.0
    } else {
        cyf * f64::from(generator.base_wind_season_variation) * draw_float_0_2(&mut rng)
    };
    let wind_multiplier = f64::from(draw_inclusive(&mut rng, 1, 2));
    let wind_exponent = f64::from(draw_inclusive(
        &mut rng,
        9,
        u32::try_from(generator.base_wind_distribution_peaks)
            .map_err(|_| SimError::NumericOverflow)?,
    ));
    raw_wind_noise = (generator.base_wind_millionths as f64 / WEATHER_SCALE as f64
        * wind_multiplier
        / libm::pow((pressure + raw_wind_noise) / 1014.78, wind_exponent)
        + variation)
        .max(0.0)
        .trunc();
    let windpower_millionths = (raw_wind_noise * WEATHER_SCALE as f64) as i64;
    let wind_direction_degrees = match previous {
        None => draw_wind_direction(&mut rng, calendar.season),
        Some(state) => {
            let divisor = (raw_wind_noise as u64).saturating_mul(2_160);
            if divisor <= 1 || rng.next_u64() % divisor == 0 {
                draw_wind_direction(&mut rng, calendar.season)
            } else {
                state.wind_direction_degrees
            }
        }
    };

    let precise = PreciseWeather {
        temperature_millikelvin,
        humidity_millionths: (humidity * WEATHER_SCALE as f64) as i64,
        pressure_millionths: (pressure * WEATHER_SCALE as f64) as i64,
        windpower_millionths,
        is_day: NaturalLightSnapshot::at_tick(tick).phase == SkyPhase::Day,
    };
    let weather_type_index = select_weather(catalog, &precise)?;
    let weather = catalog
        .weather_types
        .get(usize::from(weather_type_index))
        .ok_or(SimError::InvalidSnapshot)?;
    let duration_seconds = if weather.duration_min_seconds == weather.duration_max_seconds {
        weather.duration_min_seconds
    } else {
        let span = weather
            .duration_max_seconds
            .checked_sub(weather.duration_min_seconds)
            .and_then(|value| value.checked_add(1))
            .ok_or(SimError::NumericOverflow)?;
        weather.duration_min_seconds + rng.next_u64() % span
    };
    let duration_ticks = duration_seconds
        .checked_mul(SimTick::HZ)
        .ok_or(SimError::NumericOverflow)?
        .max(1);
    Ok(WeatherStateV1 {
        reference_position: previous
            .map_or(cdda_protocol::WorldPosition { x: 0, y: 0, z: 0 }, |state| {
                state.reference_position
            }),
        weather_type_index,
        temperature_millikelvin,
        humidity_millionths: precise.humidity_millionths,
        pressure_millionths: precise.pressure_millionths,
        windpower_millionths,
        wind_direction_degrees,
        next_update_tick: SimTick(
            tick.0
                .checked_add(duration_ticks)
                .ok_or(SimError::NumericOverflow)?,
        ),
        update_sequence: sequence,
    })
}

struct PreciseWeather {
    temperature_millikelvin: i32,
    humidity_millionths: i64,
    pressure_millionths: i64,
    windpower_millionths: i64,
    is_day: bool,
}

fn select_weather(catalog: &WeatherCatalogV1, weather: &PreciseWeather) -> Result<u16, SimError> {
    let mut selected = u16::try_from(
        catalog
            .weather_types
            .iter()
            .position(|definition| definition.weather_type_id == "clear")
            .ok_or(SimError::InvalidSnapshot)?,
    )
    .map_err(|_| SimError::NumericOverflow)?;
    for index in &catalog.generator.sorted_weather_type_indexes {
        let definition = catalog
            .weather_types
            .get(usize::from(*index))
            .ok_or(SimError::InvalidSnapshot)?;
        let required = definition.required_weathers.is_empty()
            || definition.required_weathers.iter().any(|required| {
                catalog.weather_types[usize::from(selected)].weather_type_id == *required
            });
        if required
            && !definition.has_unsupported_runtime_effects
            && evaluate_condition(&definition.condition, weather)
        {
            selected = *index;
        }
    }
    Ok(selected)
}

fn evaluate_condition(condition: &WeatherConditionV1, weather: &PreciseWeather) -> bool {
    match condition {
        WeatherConditionV1::Always => true,
        WeatherConditionV1::IsDay => weather.is_day,
        WeatherConditionV1::All(children) => children
            .iter()
            .all(|child| evaluate_condition(child, weather)),
        WeatherConditionV1::Any(children) => children
            .iter()
            .any(|child| evaluate_condition(child, weather)),
        WeatherConditionV1::Compare {
            metric,
            comparison,
            value,
        } => {
            let actual = match metric {
                WeatherMetricV1::TemperatureMillikelvin => {
                    i64::from(weather.temperature_millikelvin)
                }
                WeatherMetricV1::HumidityMillionths => weather.humidity_millionths,
                WeatherMetricV1::PressureMillionths => weather.pressure_millionths,
                WeatherMetricV1::WindpowerMillionths => weather.windpower_millionths,
                WeatherMetricV1::DewPointFactorMillionths => dew_point_factor(weather),
            };
            match comparison {
                WeatherComparisonV1::Less => actual < *value,
                WeatherComparisonV1::LessOrEqual => actual <= *value,
                WeatherComparisonV1::Greater => actual > *value,
                WeatherComparisonV1::GreaterOrEqual => actual >= *value,
            }
        }
        WeatherConditionV1::Unsupported(_) => false,
    }
}

fn dew_point_factor(weather: &PreciseWeather) -> i64 {
    let celsius = f64::from(weather.temperature_millikelvin) / 1_000.0 - 273.15;
    let humidity = weather.humidity_millionths as f64 / WEATHER_SCALE as f64;
    if humidity <= 0.0 {
        return i64::MAX;
    }
    let gamma = libm::log(humidity / 100.0) + 17.625 * celsius / (243.04 + celsius);
    let dew_point = 243.04 * gamma / (17.625 - gamma);
    ((celsius - dew_point).abs() * WEATHER_SCALE as f64) as i64
}

fn season_index(season: Season) -> usize {
    match season {
        Season::Spring => 0,
        Season::Summer => 1,
        Season::Autumn => 2,
        Season::Winter => 3,
    }
}

fn weather_rng(world_seed: [u8; 32], tick: SimTick, sequence: u64) -> rand_chacha::ChaCha8Rng {
    use rand_core::SeedableRng;
    let mut hasher = blake3::Hasher::new_derive_key("cdda-rust weather-manager RNG v1");
    hasher.update(&world_seed);
    hasher.update(&tick.0.to_be_bytes());
    hasher.update(&sequence.to_be_bytes());
    rand_chacha::ChaCha8Rng::from_seed(*hasher.finalize().as_bytes())
}

fn draw_inclusive(rng: &mut impl Rng, minimum: u32, maximum: u32) -> u32 {
    if minimum == maximum {
        minimum
    } else {
        minimum + rng.next_u32() % (maximum - minimum + 1)
    }
}

fn draw_float_0_2(rng: &mut impl Rng) -> f64 {
    f64::from(rng.next_u32()) / f64::from(u32::MAX) * 2.0
}

fn draw_wind_direction(rng: &mut impl Rng, season: Season) -> i16 {
    let weights: &[u32; 16] = match season {
        Season::Spring => &[3, 3, 5, 8, 11, 10, 5, 2, 5, 6, 6, 5, 8, 10, 8, 6],
        Season::Summer => &[3, 4, 4, 8, 8, 9, 8, 3, 7, 8, 10, 7, 7, 7, 5, 3],
        Season::Autumn => &[4, 6, 6, 7, 6, 5, 4, 3, 5, 6, 8, 8, 10, 10, 8, 5],
        Season::Winter => &[5, 3, 2, 3, 2, 2, 2, 2, 4, 6, 10, 8, 12, 19, 13, 9],
    };
    let mut choice = rng.next_u32() % weights.iter().sum::<u32>();
    let mut index = 0_i16;
    for weight in weights {
        if choice < *weight {
            return index * 45 / 2;
        }
        choice -= *weight;
        index += 1;
    }
    0
}

// Pinned CDDA's Stefan Gustavson-derived 4D simplex kernel. Inputs and all
// intermediate arithmetic intentionally remain f32 to preserve C++ casts.
fn raw_noise_4d(x: f32, y: f32, z: f32, w: f32) -> f32 {
    let f4 = (libm::sqrtf(5.0) - 1.0) / 4.0;
    let g4 = (5.0 - libm::sqrtf(5.0)) / 20.0;
    let s = (x + y + z + w) * f4;
    let i = fastfloor(x + s);
    let j = fastfloor(y + s);
    let k = fastfloor(z + s);
    let l = fastfloor(w + s);
    let t = (i + j + k + l) as f32 * g4;
    let [x0, y0, z0, w0] = [
        x - (i as f32 - t),
        y - (j as f32 - t),
        z - (k as f32 - t),
        w - (l as f32 - t),
    ];
    let c = usize::from(x0 > y0) * 32
        + usize::from(x0 > z0) * 16
        + usize::from(y0 > z0) * 8
        + usize::from(x0 > w0) * 4
        + usize::from(y0 > w0) * 2
        + usize::from(z0 > w0);
    let order = SIMPLEX[c];
    let offsets = |threshold: u8| order.map(|value| i32::from(value >= threshold));
    let o1 = offsets(3);
    let o2 = offsets(2);
    let o3 = offsets(1);
    let corner = |offset: [i32; 4], factor: f32| {
        [
            x0 - offset[0] as f32 + factor * g4,
            y0 - offset[1] as f32 + factor * g4,
            z0 - offset[2] as f32 + factor * g4,
            w0 - offset[3] as f32 + factor * g4,
        ]
    };
    let points = [
        [x0, y0, z0, w0],
        corner(o1, 1.0),
        corner(o2, 2.0),
        corner(o3, 3.0),
        corner([1, 1, 1, 1], 4.0),
    ];
    let bases = [[0, 0, 0, 0], o1, o2, o3, [1, 1, 1, 1]];
    let mut total = 0.0;
    for (point, offset) in points.into_iter().zip(bases) {
        let mut attenuation = 0.6 - point.iter().map(|value| value * value).sum::<f32>();
        if attenuation >= 0.0 {
            let gi = permutation4(i + offset[0], j + offset[1], k + offset[2], l + offset[3]) % 32;
            attenuation *= attenuation;
            total += attenuation * attenuation * dot4(GRAD4[gi], point);
        }
    }
    27.0 * total
}

fn fastfloor(value: f32) -> i32 {
    if value > 0.0 {
        value as i32
    } else {
        value as i32 - 1
    }
}
fn permutation4(i: i32, j: i32, k: i32, l: i32) -> usize {
    let p = |value: i32| usize::from(PERM[(value & 255) as usize]);
    p(i + p(j + p(k + p(l) as i32) as i32) as i32)
}
fn dot4(gradient: [i8; 4], point: [f32; 4]) -> f32 {
    f32::from(gradient[0]) * point[0]
        + f32::from(gradient[1]) * point[1]
        + f32::from(gradient[2]) * point[2]
        + f32::from(gradient[3]) * point[3]
}

const GRAD4: [[i8; 4]; 32] = [
    [0, 1, 1, 1],
    [0, 1, 1, -1],
    [0, 1, -1, 1],
    [0, 1, -1, -1],
    [0, -1, 1, 1],
    [0, -1, 1, -1],
    [0, -1, -1, 1],
    [0, -1, -1, -1],
    [1, 0, 1, 1],
    [1, 0, 1, -1],
    [1, 0, -1, 1],
    [1, 0, -1, -1],
    [-1, 0, 1, 1],
    [-1, 0, 1, -1],
    [-1, 0, -1, 1],
    [-1, 0, -1, -1],
    [1, 1, 0, 1],
    [1, 1, 0, -1],
    [1, -1, 0, 1],
    [1, -1, 0, -1],
    [-1, 1, 0, 1],
    [-1, 1, 0, -1],
    [-1, -1, 0, 1],
    [-1, -1, 0, -1],
    [1, 1, 1, 0],
    [1, 1, -1, 0],
    [1, -1, 1, 0],
    [1, -1, -1, 0],
    [-1, 1, 1, 0],
    [-1, 1, -1, 0],
    [-1, -1, 1, 0],
    [-1, -1, -1, 0],
];
const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30, 69,
    142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219,
    203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, 60, 211, 133, 230,
    220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63, 161, 1, 216, 80, 73, 209, 76,
    132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173,
    186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, 207, 206,
    59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163,
    70, 221, 153, 101, 155, 167, 43, 172, 9, 129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232,
    178, 185, 112, 104, 218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162,
    241, 81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, 184, 84, 204,
    176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141,
    128, 195, 78, 66, 215, 61, 156, 180,
];
const SIMPLEX: [[u8; 4]; 64] = [
    [0, 1, 2, 3],
    [0, 1, 3, 2],
    [0, 0, 0, 0],
    [0, 2, 3, 1],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [1, 2, 3, 0],
    [0, 2, 1, 3],
    [0, 0, 0, 0],
    [0, 3, 1, 2],
    [0, 3, 2, 1],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [1, 3, 2, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [1, 2, 0, 3],
    [0, 0, 0, 0],
    [1, 3, 0, 2],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [2, 3, 0, 1],
    [2, 3, 1, 0],
    [1, 0, 2, 3],
    [1, 0, 3, 2],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [2, 0, 3, 1],
    [0, 0, 0, 0],
    [2, 1, 3, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [2, 0, 1, 3],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [3, 0, 1, 2],
    [3, 0, 2, 1],
    [0, 0, 0, 0],
    [3, 1, 2, 0],
    [2, 1, 0, 3],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [3, 1, 0, 2],
    [0, 0, 0, 0],
    [3, 2, 0, 1],
    [3, 2, 1, 0],
];
