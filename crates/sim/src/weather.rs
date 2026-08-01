// SPDX-License-Identifier: GPL-3.0-or-later
// Weather formulas and the 4D simplex kernel are mechanically adapted from
// pinned Cataclysm-DDA. Simplex kernel copyright (c) 2007-2012 Eliot Eshelman;
// Cataclysm-DDA and this adaptation are distributed under GPL-3.0-or-later.

use cdda_protocol::{
    BookStudyInterruptionReason, CalendarSnapshot, ConstructionInterruptionReason,
    DisassemblyInterruptionReason, NaturalLightSnapshot, PoweredToolTransitionReason, Season,
    SimTick, SkyPhase, WEATHER_SCALE, WeatherCatalogV1, WeatherComparisonV1, WeatherConditionV1,
    WeatherMetricV1, WeatherObservationV1, WeatherPrecipitationV1, WeatherStateV1,
    WeatherTemperatureBandV1, WeatherTypeV1, WeatherWindBandV1, WorldEvent, WorldEventKind,
    WorldPosition,
};
use rand_core::Rng;

use crate::{ItemInstance, SimError, WorldState, powered_light_sight_radius};

const SECONDS_PER_DAY: f64 = 86_400.0;
const DAYS_PER_YEAR: f64 = 364.0;
const START_DAY_FROM_TURN_ZERO: f64 = 60.0 + 8.0 / 24.0;
const SIMPLEX_NOISE_RANDOM_SEED_LIMIT: u32 = 32_768;
const MAX_WEATHER_TRANSITIONS_PER_ADVANCE: usize = 4_096;

impl WorldState {
    pub(super) fn terrain_or_furniture_has_flag(
        &self,
        position: WorldPosition,
        flag: &str,
    ) -> Option<bool> {
        let (coord, local) = position.chunk_and_local();
        let chunk = self.chunks.get(&coord)?;
        let terrain = chunk.tile(local)?;
        Some(
            terrain
                .flags
                .binary_search_by(|candidate| candidate.as_str().cmp(flag))
                .is_ok()
                || chunk.furniture(local).is_some_and(|furniture| {
                    furniture
                        .flags
                        .binary_search_by(|candidate| candidate.as_str().cmp(flag))
                        .is_ok()
                }),
        )
    }

    /// Pinned outside-cache topology: underground positions are sheltered;
    /// otherwise an `INDOORS` terrain or furniture tile shelters itself and
    /// all eight horizontal neighbors.
    pub(super) fn position_is_outside(&self, position: WorldPosition) -> bool {
        if position.z < 0 {
            return false;
        }
        for dy in -1_i8..=1 {
            for dx in -1_i8..=1 {
                let Some(neighbor) = position.checked_offset(dx, dy, 0) else {
                    return false;
                };
                let Some(indoors) = self.terrain_or_furniture_has_flag(neighbor, "INDOORS") else {
                    // Unmaterialized topology is not proof of exposure.
                    return false;
                };
                if indoors {
                    return false;
                }
            }
        }
        true
    }

    /// Pinned `get_local_windpower`: shelter removes wind, forest overmap
    /// terrain halves it, elevation increases it, and the first upwind
    /// `BLOCK_WIND` tile reduces it to one tenth.
    pub(super) fn local_windpower(&self, position: WorldPosition) -> i64 {
        if !self.position_is_outside(position) {
            return 0;
        }
        let Some(state) = self.weather_state.as_ref() else {
            return 0;
        };
        let Some(catalog) = self.worldgen.as_ref() else {
            return 0;
        };
        adjusted_local_windpower(catalog, state, position, |blocker| {
            self.terrain_or_furniture_has_flag(blocker, "BLOCK_WIND")
                .is_none_or(|blocked| blocked)
        })
    }

    pub(super) fn snapshot_local_windpower(
        snapshot: &cdda_protocol::WorldSnapshotV1,
        position: WorldPosition,
    ) -> i64 {
        if !snapshot_position_is_outside(snapshot, position) {
            return 0;
        }
        let (Some(state), Some(catalog)) =
            (snapshot.weather_state.as_ref(), snapshot.worldgen.as_ref())
        else {
            return 0;
        };
        adjusted_local_windpower(catalog, state, position, |blocker| {
            snapshot_terrain_or_furniture_has_flag(snapshot, blocker, "BLOCK_WIND")
                .is_none_or(|blocked| blocked)
        })
    }

    pub(super) fn advance_weather_environment(
        &mut self,
        events: &mut Vec<WorldEvent>,
    ) -> Result<(), SimError> {
        let reference_position = self
            .actors
            .values()
            .find(|actor| actor.hp > 0)
            .map(|actor| actor.position)
            .or_else(|| {
                self.weather_state
                    .as_ref()
                    .map(|state| state.reference_position)
            })
            .unwrap_or(WorldPosition { x: 0, y: 0, z: 0 });
        let transition =
            if let (Some(catalog), Some(state)) = (&self.weather_catalog, &self.weather_state) {
                advance_weather_state(
                    catalog,
                    state,
                    self.world_seed,
                    self.tick,
                    reference_position,
                )?
            } else {
                None
            };
        if let Some(next) = transition {
            let became_dangerous = self
                .weather_catalog
                .as_ref()
                .zip(self.weather_state.as_ref())
                .and_then(|(catalog, state)| is_dangerous(catalog, state))
                == Some(false)
                && self
                    .weather_catalog
                    .as_ref()
                    .and_then(|catalog| is_dangerous(catalog, &next))
                    == Some(true);
            self.weather_state = Some(next);
            if became_dangerous {
                let actor_ids = self
                    .actors
                    .iter()
                    .filter_map(|(actor_id, actor)| {
                        let inside_vehicle = self.actor_vehicle_context(*actor_id).1;
                        (!inside_vehicle && self.position_is_outside(actor.position))
                            .then_some(*actor_id)
                    })
                    .collect::<Vec<_>>();
                for actor_id in actor_ids {
                    self.interrupt_craft(actor_id, events)?;
                    self.interrupt_book_study(
                        actor_id,
                        BookStudyInterruptionReason::DangerousWeather,
                        events,
                    )?;
                    self.interrupt_disassembly(
                        actor_id,
                        DisassemblyInterruptionReason::DangerousWeather,
                        events,
                    )?;
                    self.interrupt_construction(
                        actor_id,
                        ConstructionInterruptionReason::DangerousWeather,
                        events,
                    )?;
                }
            }
        }
        self.advance_precipitation(events)
    }

    pub(super) fn effective_natural_sight_radius(&self) -> u16 {
        let natural = NaturalLightSnapshot::at_tick(self.tick).sight_radius;
        let (Some(catalog), Some(state)) = (&self.weather_catalog, &self.weather_state) else {
            return natural;
        };
        effective_natural_sight_radius(catalog, state, natural).unwrap_or(0)
    }

    pub(super) fn effective_weather_temperature_millikelvin(&self) -> i32 {
        self.weather_catalog
            .as_ref()
            .zip(self.weather_state.as_ref())
            .and_then(|(catalog, state)| effective_temperature_millikelvin(catalog, state))
            .unwrap_or(cdda_protocol::ITEM_TEMPERATURE_NORMAL_AMBIENT_MILLIKELVIN)
    }

    fn advance_precipitation(&mut self, events: &mut Vec<WorldEvent>) -> Result<(), SimError> {
        if !self.tick.0.is_multiple_of(SimTick::HZ) {
            return Ok(());
        }
        let Some(one_in) = self
            .weather_catalog
            .as_ref()
            .zip(self.weather_state.as_ref())
            .and_then(|(catalog, state)| precipitation_extinguish_one_in(catalog, state))
        else {
            return Ok(());
        };
        let mut candidates = Vec::new();
        for (actor_id, actor) in &self.actors {
            let inside_vehicle = self.actor_vehicle_context(*actor_id).1;
            if inside_vehicle || !self.position_is_outside(actor.position) {
                continue;
            }
            if let Some(item_id) = actor.wielded
                && actor
                    .inventory
                    .get(&item_id)
                    .is_some_and(ItemInstance::is_active_and_water_extinguishable)
            {
                candidates.push((Some(*actor_id), item_id));
            }
        }
        candidates.extend(self.ground_items.values().filter_map(|ground| {
            (self.position_is_outside(ground.position)
                && ground.item.is_active_and_water_extinguishable())
            .then_some((None, ground.item.id))
        }));
        for (actor_id, item_id) in candidates {
            let mut rng = self.named_rng(
                b"weather-precipitation-extinguish",
                &[item_id.as_u128()],
                self.tick.0,
            );
            if !rng.next_u32().is_multiple_of(one_in) {
                continue;
            }
            let item = actor_id.map_or_else(
                || {
                    self.ground_items
                        .get_mut(&item_id)
                        .map(|ground| &mut ground.item)
                        .ok_or(SimError::UnknownItem)
                },
                |actor_id| {
                    self.actors
                        .get_mut(&actor_id)
                        .and_then(|actor| actor.inventory.get_mut(&item_id))
                        .ok_or(SimError::UnknownItem)
                },
            )?;
            item.set_powered_active(false)?;
            let available_energy_millijoules = item.available_power_energy_millijoules()?;
            events.push(self.make_event(WorldEventKind::PoweredToolChanged {
                actor_id,
                item_id,
                active: false,
                reason: PoweredToolTransitionReason::Precipitation,
                available_energy_millijoules,
            })?);
        }
        Ok(())
    }
}

fn adjusted_local_windpower(
    catalog: &cdda_protocol::WorldgenCatalogV1,
    state: &WeatherStateV1,
    position: WorldPosition,
    is_blocked: impl FnOnce(WorldPosition) -> bool,
) -> i64 {
    let mut windpower = (state.windpower_millionths / WEATHER_SCALE).max(0);
    let omt_size = cdda_protocol::WORLDGEN_OMT_SIZE as i32;
    let omt = cdda_protocol::ChunkCoord {
        x: position.x.div_euclid(omt_size),
        y: position.y.div_euclid(omt_size),
        z: position.z,
    };
    let Some(identity) = cdda_protocol::worldgen_omt_identity_at(catalog, omt) else {
        return 0;
    };
    if matches!(identity.type_id.as_str(), "forest" | "forest_water") {
        windpower /= 2;
    }
    if position.z > 0 {
        windpower =
            windpower.saturating_add(i64::from(position.z).saturating_mul(windpower.min(5)));
    }
    let (dx, dy) = wind_blocker_offset(state.wind_direction_degrees);
    let Some(blocker) = position.checked_offset(dx, dy, 0) else {
        return 0;
    };
    if is_blocked(blocker) {
        windpower /= 10;
    }
    windpower
}

fn snapshot_terrain_or_furniture_has_flag(
    snapshot: &cdda_protocol::WorldSnapshotV1,
    position: WorldPosition,
    flag: &str,
) -> Option<bool> {
    let (coord, local) = position.chunk_and_local();
    let chunk = snapshot.chunks.iter().find(|chunk| chunk.coord == coord)?;
    let index = usize::from(local.y)
        .checked_mul(cdda_protocol::SUBMAP_SIZE as usize)?
        .checked_add(usize::from(local.x))?;
    let terrain = chunk.tiles.get(index)?;
    Some(
        terrain
            .flags
            .binary_search_by(|candidate| candidate.as_str().cmp(flag))
            .is_ok()
            || chunk
                .furniture
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|furniture| {
                    furniture
                        .flags
                        .binary_search_by(|candidate| candidate.as_str().cmp(flag))
                        .is_ok()
                }),
    )
}

fn snapshot_position_is_outside(
    snapshot: &cdda_protocol::WorldSnapshotV1,
    position: WorldPosition,
) -> bool {
    if position.z < 0 {
        return false;
    }
    (-1_i8..=1).all(|dy| {
        (-1_i8..=1).all(|dx| {
            position.checked_offset(dx, dy, 0).is_some_and(|neighbor| {
                snapshot_terrain_or_furniture_has_flag(snapshot, neighbor, "INDOORS") == Some(false)
            })
        })
    })
}

fn wind_blocker_offset(direction_degrees: i16) -> (i8, i8) {
    match direction_degrees.rem_euclid(360) {
        330..=359 => (0, -1),
        301..=329 => (-1, -1),
        240..=300 => (-1, 0),
        211..=239 => (-1, 1),
        150..=210 => (0, 1),
        121..=149 => (1, 1),
        60..=120 => (1, 0),
        31..=59 => (1, -1),
        _ => (0, -1),
    }
}

pub(super) fn initial_weather_state(
    catalog: &WeatherCatalogV1,
    world_seed: [u8; 32],
) -> Result<WeatherStateV1, SimError> {
    calculate_weather(
        catalog,
        None,
        world_seed,
        SimTick(0),
        1,
        WorldPosition { x: 0, y: 0, z: 0 },
    )
}

pub(super) fn advance_weather_state(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
    world_seed: [u8; 32],
    tick: SimTick,
    reference_position: WorldPosition,
) -> Result<Option<WeatherStateV1>, SimError> {
    if tick < state.next_update_tick {
        return Ok(None);
    }
    let mut next = state.clone();
    let mut transitions = 0_usize;
    while tick >= next.next_update_tick {
        transitions = transitions
            .checked_add(1)
            .ok_or(SimError::NumericOverflow)?;
        if transitions > MAX_WEATHER_TRANSITIONS_PER_ADVANCE {
            return Err(SimError::NumericOverflow);
        }
        let transition_tick = next.next_update_tick;
        let sequence = next
            .update_sequence
            .checked_add(1)
            .ok_or(SimError::NumericOverflow)?;
        next = calculate_weather(
            catalog,
            Some(&next),
            world_seed,
            transition_tick,
            sequence,
            reference_position,
        )?;
    }
    Ok(Some(next))
}

pub(super) fn weather_observation(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
    tick: SimTick,
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
        temperature_band: temperature_band(effective_temperature_millikelvin(catalog, state)?),
        wind_band: wind_band(state.windpower_millionths),
        effective_sight_radius: effective_natural_sight_radius(
            catalog,
            state,
            NaturalLightSnapshot::at_tick(tick).sight_radius,
        )?,
    })
}

pub(super) fn current_weather_type<'a>(
    catalog: &'a WeatherCatalogV1,
    state: &WeatherStateV1,
) -> Option<&'a WeatherTypeV1> {
    catalog
        .weather_types
        .get(usize::from(state.weather_type_index))
}

pub(super) fn effective_temperature_millikelvin(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
) -> Option<i32> {
    state
        .temperature_millikelvin
        .checked_add(current_weather_type(catalog, state)?.temperature_modifier_millikelvin)
}

pub(super) fn effective_natural_sight_radius(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
    natural_sight_radius: u16,
) -> Option<u16> {
    let weather = current_weather_type(catalog, state)?;
    let natural_light = (0_u16..=35)
        .find(|light| powered_light_sight_radius(*light) >= u32::from(natural_sight_radius))?;
    let adjusted_light = i128::from(natural_light)
        .checked_mul(i128::from(weather.light_multiplier_millionths))?
        .checked_div(i128::from(WEATHER_SCALE))?
        .checked_add(i128::from(weather.light_modifier))?
        .max(0);
    let light_radius = powered_light_sight_radius(u16::try_from(adjusted_light).ok()?);
    let attenuated = i128::from(light_radius)
        .checked_mul(i128::from(WEATHER_SCALE))?
        .checked_div(i128::from(
            weather.sight_penalty_millionths.max(WEATHER_SCALE),
        ))?;
    u16::try_from(attenuated.min(60)).ok()
}

pub(super) fn sound_attenuation(catalog: &WeatherCatalogV1, state: &WeatherStateV1) -> Option<i32> {
    Some(current_weather_type(catalog, state)?.sound_attenuation)
}

pub(super) fn is_dangerous(catalog: &WeatherCatalogV1, state: &WeatherStateV1) -> Option<bool> {
    Some(current_weather_type(catalog, state)?.dangerous)
}

pub(super) fn precipitation_rate_micrometers_per_hour(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
) -> Option<u32> {
    let weather = current_weather_type(catalog, state)?;
    if !weather.rains {
        return Some(0);
    }
    Some(match weather.precipitation {
        WeatherPrecipitationV1::None => 0,
        WeatherPrecipitationV1::VeryLight => 500,
        WeatherPrecipitationV1::Light => 1_500,
        WeatherPrecipitationV1::Heavy => 3_000,
    })
}

pub(super) fn precipitation_extinguish_one_in(
    catalog: &WeatherCatalogV1,
    state: &WeatherStateV1,
) -> Option<u32> {
    Some(
        match precipitation_rate_micrometers_per_hour(catalog, state)? {
            0 => return None,
            ..=500 => 100,
            501..=1_500 => 50,
            _ => 10,
        },
    )
}

fn temperature_band(temperature_millikelvin: i32) -> WeatherTemperatureBandV1 {
    match temperature_millikelvin {
        ..=263_149 => WeatherTemperatureBandV1::Frigid,
        263_150..=278_149 => WeatherTemperatureBandV1::Cold,
        278_150..=288_149 => WeatherTemperatureBandV1::Cool,
        288_150..=298_149 => WeatherTemperatureBandV1::Mild,
        298_150..=308_149 => WeatherTemperatureBandV1::Warm,
        _ => WeatherTemperatureBandV1::Hot,
    }
}

pub(super) fn wind_band(windpower_millionths: i64) -> WeatherWindBandV1 {
    match windpower_millionths / WEATHER_SCALE {
        ..=1 => WeatherWindBandV1::Calm,
        2..=7 => WeatherWindBandV1::Light,
        8..=18 => WeatherWindBandV1::Moderate,
        19..=31 => WeatherWindBandV1::Strong,
        _ => WeatherWindBandV1::Gale,
    }
}

fn calculate_weather(
    catalog: &WeatherCatalogV1,
    previous: Option<&WeatherStateV1>,
    world_seed: [u8; 32],
    tick: SimTick,
    sequence: u64,
    reference_position: WorldPosition,
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
    let x = f64::from(reference_position.x) / 2_000.0;
    let y = f64::from(reference_position.y) / 2_000.0;
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
        is_day: NaturalLightSnapshot::at_tick(tick).phase != SkyPhase::Night,
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
        .ok_or(SimError::NumericOverflow)?;
    if duration_ticks == 0 {
        return Err(SimError::InvalidSnapshot);
    }
    Ok(WeatherStateV1 {
        reference_position,
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
    // Multiplayer adaptation: weather owns a deterministic named stream so
    // unrelated authoritative actions cannot perturb its draw order. This is
    // intentionally not claimed to reproduce C++'s process-global minstd RNG.
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
        let mut attenuation = 0.6;
        attenuation -= point[0] * point[0];
        attenuation -= point[1] * point[1];
        attenuation -= point[2] * point[2];
        attenuation -= point[3] * point[3];
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
