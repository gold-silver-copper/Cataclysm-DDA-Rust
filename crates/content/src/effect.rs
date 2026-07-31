//! Immutable projection of effect-type application and ordinary stat modifiers.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::proficiency::decimal_millionths;
use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const DEFAULT_MAX_DURATION_SECONDS: u64 = 365 * 24 * 60 * 60;
const MODIFIER_SCALE: i64 = 1_000_000;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ScaledModifier {
    base_millionths: i64,
    scaling_millionths: i64,
}

impl ScaledModifier {
    fn resolve(&self, intensity: u32) -> Option<i16> {
        let scaled = i128::from(self.base_millionths).checked_add(
            i128::from(self.scaling_millionths)
                .checked_mul(i128::from(intensity.saturating_sub(1)))?,
        )?;
        i16::try_from(scaled / i128::from(MODIFIER_SCALE)).ok()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LimbScoreModifier {
    score_id: String,
    multiplier_millionths: i64,
    scaling_millionths: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EffectTypeDefinition {
    id: String,
    maximum_duration_seconds: u64,
    maximum_intensity: u32,
    maximum_effective_intensity: u32,
    duration_add_percent: u16,
    strength: ScaledModifier,
    dexterity: ScaledModifier,
    intelligence: ScaledModifier,
    perception: ScaledModifier,
    speed: ScaledModifier,
    limb_scores: Vec<LimbScoreModifier>,
    blocks_effects: BTreeSet<String>,
    application_supported: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EffectTypeRegistry {
    effects: BTreeMap<String, EffectTypeDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEffectApplication {
    pub maximum_duration_seconds: u64,
    pub duration_add_percent: u16,
    pub intensity: u32,
    pub strength_modifier: i16,
    pub dexterity_modifier: i16,
    pub intelligence_modifier: i16,
    pub perception_modifier: i16,
    pub speed_modifier: i16,
    pub limb_score_multipliers: Vec<(String, u32)>,
    pub blocked_by_effect_ids: Vec<String>,
}

impl EffectTypeRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, EffectTypeRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(EffectTypeRegistryError::Catalog)?;
        let mut effects = BTreeMap::new();
        for file in files {
            read_file(content_root.as_ref(), &file, &mut effects)?;
        }
        Ok(Self { effects })
    }

    #[must_use]
    pub fn resolve_application(
        &self,
        effect_id: &str,
        requested_intensity: u32,
    ) -> Option<ResolvedEffectApplication> {
        let definition = self.effects.get(effect_id)?;
        if !definition.application_supported || requested_intensity == 0 {
            return None;
        }
        let intensity = requested_intensity.min(definition.maximum_intensity);
        let modifier_intensity = if definition.maximum_effective_intensity == 0 {
            intensity
        } else {
            intensity.min(definition.maximum_effective_intensity)
        };
        let mut combined = BTreeMap::<String, u128>::new();
        for modifier in &definition.limb_scores {
            let factor = i128::from(modifier.multiplier_millionths).checked_add(
                i128::from(modifier.scaling_millionths)
                    .checked_mul(i128::from(intensity.saturating_sub(1)))?,
            )?;
            let factor = u128::try_from(factor.max(0)).ok()?;
            let entry = combined
                .entry(modifier.score_id.clone())
                .or_insert(MODIFIER_SCALE as u128);
            *entry = entry
                .checked_mul(factor)?
                .checked_div(MODIFIER_SCALE as u128)?;
        }
        let limb_score_multipliers = combined
            .into_iter()
            .map(|(id, multiplier)| u32::try_from(multiplier).ok().map(|value| (id, value)))
            .collect::<Option<Vec<_>>>()?;
        Some(ResolvedEffectApplication {
            maximum_duration_seconds: definition.maximum_duration_seconds,
            duration_add_percent: definition.duration_add_percent,
            intensity,
            strength_modifier: definition.strength.resolve(modifier_intensity)?,
            dexterity_modifier: definition.dexterity.resolve(modifier_intensity)?,
            intelligence_modifier: definition.intelligence.resolve(modifier_intensity)?,
            perception_modifier: definition.perception.resolve(modifier_intensity)?,
            speed_modifier: definition.speed.resolve(modifier_intensity)?,
            limb_score_multipliers,
            blocked_by_effect_ids: self
                .effects
                .values()
                .filter(|candidate| candidate.blocks_effects.contains(effect_id))
                .map(|candidate| candidate.id.clone())
                .collect(),
        })
    }
}

fn read_file(
    root: &Path,
    file: &SelectedContentFile,
    effects: &mut BTreeMap<String, EffectTypeDefinition>,
) -> Result<(), EffectTypeRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| EffectTypeRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| EffectTypeRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                collect_effect(value, &file.upstream_path, effects);
            }
        }
        value => collect_effect(value, &file.upstream_path, effects),
    }
    Ok(())
}

fn collect_effect(
    value: Value,
    source: &str,
    effects: &mut BTreeMap<String, EffectTypeDefinition>,
) {
    let Some(object) = value.as_object() else {
        return;
    };
    if object.get("type").and_then(Value::as_str) != Some("effect_type") {
        return;
    }
    let Some(id) = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    else {
        return;
    };
    effects.insert(id.to_owned(), parse_effect_type(id, object, source));
}

fn parse_effect_type(id: &str, object: &Map<String, Value>, source: &str) -> EffectTypeDefinition {
    let mut supported = !object.contains_key("copy-from");
    let maximum_duration_seconds = object
        .get("max_duration")
        .map_or(Some(DEFAULT_MAX_DURATION_SECONDS), parse_duration_seconds);
    let maximum_intensity = optional_u32(object.get("max_intensity"), 1);
    let maximum_effective_intensity = optional_u32(object.get("max_effective_intensity"), 0);
    let duration_add_percent = optional_u16(object.get("dur_add_perc"), 100);
    let intensity_duration_factor = object
        .get("int_dur_factor")
        .map_or(Some(0), parse_duration_seconds);
    let disallowed_nonempty = [
        "resist_traits",
        "resist_effects",
        "immune_flags",
        "immune_bp_flags",
        "effect_dur_scaling",
        "chance_kill",
        "chance_kill_resist",
        "vitamins",
        "enchantments",
    ]
    .into_iter()
    .any(|field| value_is_nonempty(object.get(field)));
    let main_parts_only = object
        .get("main_parts_only")
        .map_or(Some(false), Value::as_bool);
    let blocks_effects = string_set(object.get("blocks_effects"));
    let removes_effects = string_set(object.get("removes_effects"));
    let flags = string_set(object.get("flags"));
    let modifier_maps_supported = ["base_mods", "scaling_mods"].into_iter().all(|field| {
        object.get(field).is_none_or(|value| {
            value.as_object().is_some_and(|modifiers| {
                modifiers.keys().all(|modifier| {
                    matches!(
                        modifier.as_str(),
                        "str_mod" | "dex_mod" | "int_mod" | "per_mod" | "speed_mod"
                    )
                })
            })
        })
    });
    let intensity_decay_supported = object.get("int_decay_step").map_or(Some(-1), Value::as_i64)
        == Some(-1)
        && object.get("int_decay_tick").map_or(Some(0), |value| {
            value.as_i64().or_else(|| {
                parse_duration_seconds(value).and_then(|value| i64::try_from(value).ok())
            })
        }) == Some(0)
        && object
            .get("int_decay_remove")
            .map_or(Some(false), Value::as_bool)
            == Some(false);
    supported &= maximum_duration_seconds.is_some()
        && maximum_intensity.is_some_and(|value| value > 0)
        && maximum_effective_intensity.is_some_and(|value| {
            value == 0 || maximum_intensity.is_some_and(|maximum| value <= maximum)
        })
        && duration_add_percent.is_some()
        && intensity_duration_factor == Some(0)
        && !disallowed_nonempty
        && main_parts_only == Some(false)
        && blocks_effects.is_some()
        && removes_effects.as_ref().is_some_and(BTreeSet::is_empty)
        && flags.is_some()
        && modifier_maps_supported
        && intensity_decay_supported;
    let (strength, strength_supported) = parse_modifier(object, "str_mod", source);
    let (dexterity, dexterity_supported) = parse_modifier(object, "dex_mod", source);
    let (intelligence, intelligence_supported) = parse_modifier(object, "int_mod", source);
    let (perception, perception_supported) = parse_modifier(object, "per_mod", source);
    let (speed, speed_supported) = parse_modifier(object, "speed_mod", source);
    let (limb_scores, limb_scores_supported) = parse_limb_scores(object, source);
    let flags_supported = flags.as_ref().is_some_and(|flags| {
        flags.iter().all(|flag| flag == "EFFECT_LIMB_SCORE_MOD")
            && (limb_scores.is_empty() || flags.contains("EFFECT_LIMB_SCORE_MOD"))
    });
    supported &= strength_supported
        && dexterity_supported
        && intelligence_supported
        && perception_supported
        && speed_supported
        && limb_scores_supported
        && flags_supported;
    let mut blocking_targets = blocks_effects.unwrap_or_default();
    blocking_targets.extend(removes_effects.unwrap_or_default());
    EffectTypeDefinition {
        id: id.to_owned(),
        maximum_duration_seconds: maximum_duration_seconds.unwrap_or_default(),
        maximum_intensity: maximum_intensity.unwrap_or_default(),
        maximum_effective_intensity: maximum_effective_intensity.unwrap_or_default(),
        duration_add_percent: duration_add_percent.unwrap_or_default(),
        strength,
        dexterity,
        intelligence,
        perception,
        speed,
        limb_scores,
        blocks_effects: blocking_targets,
        application_supported: supported,
    }
}

fn parse_modifier(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> (ScaledModifier, bool) {
    let parse = |container: &str| -> Result<i64, ()> {
        let Some(value) = object.get(container) else {
            return Ok(0);
        };
        let Some(values) = value.as_object() else {
            return Err(());
        };
        let Some(value) = values.get(field) else {
            return Ok(0);
        };
        let number = value
            .as_array()
            .and_then(|values| values.first())
            .and_then(Value::as_number)
            .ok_or(())?;
        decimal_millionths(number, source, field).map_err(|_| ())
    };
    match (parse("base_mods"), parse("scaling_mods")) {
        (Ok(base_millionths), Ok(scaling_millionths)) => (
            ScaledModifier {
                base_millionths,
                scaling_millionths,
            },
            true,
        ),
        _ => (ScaledModifier::default(), false),
    }
}

fn parse_limb_scores(object: &Map<String, Value>, source: &str) -> (Vec<LimbScoreModifier>, bool) {
    let Some(value) = object.get("limb_score_mods") else {
        return (Vec::new(), true);
    };
    let Some(values) = value.as_array() else {
        return (Vec::new(), false);
    };
    let mut result = Vec::new();
    for value in values {
        let Some(value) = value.as_object() else {
            return (Vec::new(), false);
        };
        let Some(score_id) = value
            .get("limb_score")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return (Vec::new(), false);
        };
        let parse = |field: &str, default: i64| -> Option<i64> {
            value.get(field).map_or(Some(default), |value| {
                decimal_millionths(value.as_number()?, source, field).ok()
            })
        };
        let Some(multiplier_millionths) = parse("modifier", MODIFIER_SCALE) else {
            return (Vec::new(), false);
        };
        let Some(scaling_millionths) = parse("scaling", 0) else {
            return (Vec::new(), false);
        };
        result.push(LimbScoreModifier {
            score_id: score_id.to_owned(),
            multiplier_millionths,
            scaling_millionths,
        });
    }
    (result, true)
}

fn parse_duration_seconds(value: &Value) -> Option<u64> {
    if let Some(turns) = value.as_u64() {
        return Some(turns);
    }
    let value = value.as_str()?;
    let bytes = value.as_bytes();
    let mut index = 0_usize;
    let mut seconds = 0_u64;
    let mut terms = 0_u64;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let number = value.get(start..index)?.parse::<u64>().ok()?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let multiplier = match value.get(start..index)? {
            "turn" | "turns" | "s" | "second" | "seconds" => 1,
            "m" | "minute" | "minutes" => 60,
            "h" | "hour" | "hours" => 60 * 60,
            "d" | "day" | "days" => 24 * 60 * 60,
            _ => return None,
        };
        seconds = seconds.checked_add(number.checked_mul(multiplier)?)?;
        terms += 1;
    }
    (terms > 0).then_some(seconds)
}

fn optional_u32(value: Option<&Value>, default: u32) -> Option<u32> {
    value.map_or(Some(default), |value| {
        value.as_u64().and_then(|value| u32::try_from(value).ok())
    })
}

fn optional_u16(value: Option<&Value>, default: u16) -> Option<u16> {
    value.map_or(Some(default), |value| {
        value.as_u64().and_then(|value| u16::try_from(value).ok())
    })
}

fn string_set(value: Option<&Value>) -> Option<BTreeSet<String>> {
    let Some(value) = value else {
        return Some(BTreeSet::new());
    };
    value
        .as_array()?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
        .collect()
}

fn value_is_nonempty(value: Option<&Value>) -> bool {
    value.is_some_and(|value| match value {
        Value::Array(values) => !values.is_empty(),
        Value::Object(values) => !values.is_empty(),
        Value::Null => false,
        _ => true,
    })
}

#[derive(Debug)]
pub enum EffectTypeRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
}

impl fmt::Display for EffectTypeRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "effect-type mod selection failed: {error}"),
            Self::Io(path, error) => {
                write!(formatter, "effect-type I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "effect-type JSON failed for {path}: {error}")
            }
        }
    }
}

impl std::error::Error for EffectTypeRegistryError {}
