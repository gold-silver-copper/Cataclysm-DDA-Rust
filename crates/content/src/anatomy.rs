use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_BODY_PART_DEFINITIONS: usize = 4_096;
pub const MAX_ANATOMY_DEFINITIONS: usize = 1_024;
pub const MAX_ANATOMY_PARTS: usize = 256;
pub const ANATOMY_SCALE: i64 = 1_000_000;

const BODY_PART_CORE_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "main_part",
    "connected_to",
    "opposite_part",
    "is_vital",
    "hit_size",
    "hit_difficulty",
    "base_hp",
    "stat_hp_mods",
    "effects_on_hit",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BodyPartStatHpModifiers {
    pub strength_millionths: i64,
    pub dexterity_millionths: i64,
    pub intelligence_millionths: i64,
    pub perception_millionths: i64,
    pub health_millionths: i64,
}

impl Default for BodyPartStatHpModifiers {
    fn default() -> Self {
        Self {
            strength_millionths: 3 * ANATOMY_SCALE,
            dexterity_millionths: 0,
            intelligence_millionths: 0,
            perception_millionths: 0,
            health_millionths: 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyPartDefinition {
    pub id: String,
    pub main_part: String,
    pub connected_to: String,
    pub opposite_part: String,
    pub vital: bool,
    pub hit_size_millionths: u64,
    pub hit_difficulty_millionths: i64,
    pub base_hp: i32,
    pub hp_modifiers: BodyPartStatHpModifiers,
    pub effects_on_hit: Vec<BodyPartOnHitEffectDefinition>,
    /// Fields outside the immutable HP/selection projection remain visible to
    /// later armor, effects, temperature, and mutation admission.
    pub deferred_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyPartOnHitEffectDefinition {
    pub effect_id: String,
    pub global: bool,
    pub damage_type_id: String,
    pub damage_threshold_millionths: i64,
    pub scale_increment_millionths: i64,
    pub chance_percent: i32,
    pub chance_damage_scaling_millionths: i64,
    pub intensity: i32,
    pub intensity_damage_scaling_millionths: i64,
    pub max_intensity: i32,
    pub duration_turns: i32,
    pub duration_damage_scaling_millionths: i64,
    pub max_duration_turns: i32,
    pub deferred_fields: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnatomyDefinition {
    pub id: String,
    pub parts: Vec<String>,
    pub deferred_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnatomyRegistry {
    body_parts: BTreeMap<String, BodyPartDefinition>,
    anatomies: BTreeMap<String, AnatomyDefinition>,
}

impl AnatomyRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, AnatomyRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(AnatomyRegistryError::Catalog)?;
        let mut raw_parts = VecDeque::new();
        let mut raw_anatomies = Vec::new();
        for file in files {
            read_file(
                content_root.as_ref(),
                &file,
                &mut raw_parts,
                &mut raw_anatomies,
            )?;
        }
        if raw_parts.len() > MAX_BODY_PART_DEFINITIONS {
            return Err(AnatomyRegistryError::TooManyBodyParts);
        }
        if raw_anatomies.len() > MAX_ANATOMY_DEFINITIONS {
            return Err(AnatomyRegistryError::TooManyAnatomies);
        }

        let mut body_parts = BTreeMap::new();
        while !raw_parts.is_empty() {
            let pass = raw_parts.len();
            let mut loaded = 0_usize;
            for _ in 0..pass {
                let raw = raw_parts
                    .pop_front()
                    .ok_or(AnatomyRegistryError::InternalQueue)?;
                if let Some(definition) = resolve_body_part(&raw, &body_parts)? {
                    body_parts.insert(definition.id.clone(), definition);
                    loaded += 1;
                } else {
                    raw_parts.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(AnatomyRegistryError::UnresolvedBodyPartInheritance(
                    raw_parts
                        .iter()
                        .take(20)
                        .filter_map(|raw| string_field(&raw.object, "id").ok())
                        .map(str::to_owned)
                        .collect(),
                ));
            }
        }

        let mut anatomies = BTreeMap::new();
        for raw in raw_anatomies {
            let definition = parse_anatomy(&raw, &body_parts)?;
            anatomies.insert(definition.id.clone(), definition);
        }
        Ok(Self {
            body_parts,
            anatomies,
        })
    }

    #[must_use]
    pub fn body_part(&self, id: &str) -> Option<&BodyPartDefinition> {
        self.body_parts.get(id)
    }

    #[must_use]
    pub fn anatomy(&self, id: &str) -> Option<&AnatomyDefinition> {
        self.anatomies.get(id)
    }

    pub fn body_parts(&self) -> impl ExactSizeIterator<Item = (&str, &BodyPartDefinition)> {
        self.body_parts
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    pub fn anatomies(&self) -> impl ExactSizeIterator<Item = (&str, &AnatomyDefinition)> {
        self.anatomies
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

#[derive(Clone)]
struct RawDefinition {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

fn read_file(
    root: &Path,
    file: &SelectedContentFile,
    body_parts: &mut VecDeque<RawDefinition>,
    anatomies: &mut Vec<RawDefinition>,
) -> Result<(), AnatomyRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| AnatomyRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| AnatomyRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                collect_value(file, value, body_parts, anatomies)?;
            }
        }
        value => collect_value(file, value, body_parts, anatomies)?,
    }
    Ok(())
}

fn collect_value(
    file: &SelectedContentFile,
    value: Value,
    body_parts: &mut VecDeque<RawDefinition>,
    anatomies: &mut Vec<RawDefinition>,
) -> Result<(), AnatomyRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let raw = RawDefinition {
        file: file.clone(),
        object: object.clone(),
    };
    match object.get("type").and_then(Value::as_str) {
        Some("body_part") => body_parts.push_back(raw),
        Some("anatomy") => anatomies.push(raw),
        _ => {}
    }
    Ok(())
}

fn resolve_body_part(
    raw: &RawDefinition,
    resolved: &BTreeMap<String, BodyPartDefinition>,
) -> Result<Option<BodyPartDefinition>, AnatomyRegistryError> {
    let id = string_field(&raw.object, "id")?;
    let parent = optional_string_field(&raw.object, "copy-from")?;
    let mut part = match parent {
        Some(parent) => {
            let Some(parent) = resolved.get(parent) else {
                return Ok(None);
            };
            parent.clone()
        }
        None => BodyPartDefinition {
            id: id.to_owned(),
            main_part: String::new(),
            connected_to: String::new(),
            opposite_part: String::new(),
            vital: false,
            hit_size_millionths: 0,
            hit_difficulty_millionths: 0,
            base_hp: 60,
            hp_modifiers: BodyPartStatHpModifiers::default(),
            effects_on_hit: Vec::new(),
            deferred_fields: BTreeSet::new(),
            source: raw.file.upstream_path.clone(),
        },
    };
    part.id = id.to_owned();
    part.source.clone_from(&raw.file.upstream_path);
    apply_id(&raw.object, "main_part", &mut part.main_part)?;
    if part.main_part == part.id {
        apply_id(&raw.object, "connected_to", &mut part.connected_to)?;
    } else {
        part.connected_to.clone_from(&part.main_part);
    }
    if raw.object.contains_key("opposite_part") {
        apply_id(&raw.object, "opposite_part", &mut part.opposite_part)?;
    } else {
        part.opposite_part.clone_from(&part.id);
    }
    if let Some(value) = raw.object.get("is_vital") {
        part.vital = value.as_bool().ok_or_else(|| invalid(raw, "is_vital"))?;
    }
    apply_scaled_u64(&raw.object, "hit_size", &mut part.hit_size_millionths, raw)?;
    apply_scaled_i64(
        &raw.object,
        "hit_difficulty",
        &mut part.hit_difficulty_millionths,
        raw,
    )?;
    apply_i32(&raw.object, "base_hp", &mut part.base_hp, raw)?;
    if let Some(value) = raw.object.get("stat_hp_mods") {
        apply_hp_modifiers(value, &mut part.hp_modifiers, raw)?;
    }
    if let Some(value) = raw.object.get("effects_on_hit") {
        part.effects_on_hit = parse_on_hit_effects(value, raw)?;
    }
    for field in raw.object.keys() {
        if !field.starts_with("//")
            && !BODY_PART_CORE_FIELDS.contains(&field.as_str())
            && !matches!(field.as_str(), "relative" | "proportional")
        {
            part.deferred_fields.insert(field.clone());
        }
    }
    for modifier in ["relative", "proportional"] {
        if let Some(fields) = raw.object.get(modifier).and_then(Value::as_object) {
            for field in fields.keys() {
                if !matches!(field.as_str(), "hit_size" | "hit_difficulty" | "base_hp") {
                    part.deferred_fields.insert(field.clone());
                }
            }
        }
    }
    if part.main_part.is_empty()
        || part.connected_to.is_empty()
        || part.opposite_part.is_empty()
        || part.hit_size_millionths == 0
        || part.base_hp <= 0
        || part.hit_difficulty_millionths < 0
    {
        return Err(invalid(raw, "finalized body part"));
    }
    Ok(Some(part))
}

fn parse_on_hit_effects(
    value: &Value,
    raw: &RawDefinition,
) -> Result<Vec<BodyPartOnHitEffectDefinition>, AnatomyRegistryError> {
    let values = value
        .as_array()
        .filter(|values| values.len() <= 256)
        .ok_or_else(|| invalid(raw, "effects_on_hit"))?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let context = format!("effects_on_hit[{index}]");
            let object = value.as_object().ok_or_else(|| invalid(raw, &context))?;
            let effect_id = string_field(object, "id")?.to_owned();
            let global = object.get("global").map_or(Ok(false), |value| {
                value
                    .as_bool()
                    .ok_or_else(|| invalid(raw, &format!("{context}.global")))
            })?;
            let damage_type_id = optional_string_field(object, "dmg_type")?
                .unwrap_or_default()
                .to_owned();
            let fixed = |field: &str, default: f64| -> Result<i64, AnatomyRegistryError> {
                object.get(field).map_or_else(
                    || Ok((default * ANATOMY_SCALE as f64).round() as i64),
                    |value| scaled_i64(value, raw, &format!("{context}.{field}")),
                )
            };
            let integer = |field: &str, default: i32| -> Result<i32, AnatomyRegistryError> {
                object.get(field).map_or(Ok(default), |value| {
                    i32::try_from(
                        value
                            .as_i64()
                            .ok_or_else(|| invalid(raw, &format!("{context}.{field}")))?,
                    )
                    .map_err(|_| invalid(raw, &format!("{context}.{field}")))
                })
            };
            let deferred_fields = object
                .keys()
                .filter(|field| {
                    !field.starts_with("//")
                        && !matches!(
                            field.as_str(),
                            "id" | "global"
                                | "dmg_type"
                                | "dmg_threshold"
                                | "dmg_scale_increment"
                                | "chance"
                                | "chance_dmg_scaling"
                                | "intensity"
                                | "intensity_dmg_scaling"
                                | "max_intensity"
                                | "duration"
                                | "duration_dmg_scaling"
                                | "max_duration"
                        )
                })
                .cloned()
                .collect();
            let effect = BodyPartOnHitEffectDefinition {
                effect_id,
                global,
                damage_type_id,
                damage_threshold_millionths: fixed("dmg_threshold", 1.0)?,
                scale_increment_millionths: fixed("dmg_scale_increment", 1.0)?,
                chance_percent: integer("chance", 100)?,
                chance_damage_scaling_millionths: fixed("chance_dmg_scaling", 0.0)?,
                intensity: integer("intensity", 1)?,
                intensity_damage_scaling_millionths: fixed("intensity_dmg_scaling", 0.0)?,
                max_intensity: integer("max_intensity", i32::MAX)?,
                duration_turns: integer("duration", 1)?,
                duration_damage_scaling_millionths: fixed("duration_dmg_scaling", 0.0)?,
                max_duration_turns: integer("max_duration", i32::MAX)?,
                deferred_fields,
            };
            if effect.damage_threshold_millionths < 0
                || effect.scale_increment_millionths <= 0
                || effect.chance_percent < 0
                || effect.intensity <= 0
                || effect.max_intensity <= 0
                || effect.duration_turns <= 0
                || effect.max_duration_turns <= 0
            {
                return Err(invalid(raw, &context));
            }
            Ok(effect)
        })
        .collect()
}

fn parse_anatomy(
    raw: &RawDefinition,
    body_parts: &BTreeMap<String, BodyPartDefinition>,
) -> Result<AnatomyDefinition, AnatomyRegistryError> {
    let id = string_field(&raw.object, "id")?.to_owned();
    let values = raw
        .object
        .get("parts")
        .and_then(Value::as_array)
        .filter(|parts| !parts.is_empty() && parts.len() <= MAX_ANATOMY_PARTS)
        .ok_or_else(|| invalid(raw, "parts"))?;
    let mut seen = BTreeSet::new();
    let mut parts = Vec::with_capacity(values.len());
    for value in values {
        let part = value
            .as_str()
            .filter(|part| valid_id(part))
            .ok_or_else(|| invalid(raw, "parts"))?;
        if !body_parts.contains_key(part) || !seen.insert(part.to_owned()) {
            return Err(invalid(raw, "parts"));
        }
        parts.push(part.to_owned());
    }
    let deferred_fields = raw
        .object
        .keys()
        .filter(|field| {
            !field.starts_with("//") && !matches!(field.as_str(), "type" | "id" | "parts")
        })
        .cloned()
        .collect();
    Ok(AnatomyDefinition {
        id,
        parts,
        deferred_fields,
        source: raw.file.upstream_path.clone(),
    })
}

fn apply_hp_modifiers(
    value: &Value,
    modifiers: &mut BodyPartStatHpModifiers,
    raw: &RawDefinition,
) -> Result<(), AnatomyRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(raw, "stat_hp_mods"))?;
    for field in object.keys() {
        if !matches!(
            field.as_str(),
            "str_mod" | "dex_mod" | "int_mod" | "per_mod" | "health_mod"
        ) {
            return Err(invalid(raw, &format!("stat_hp_mods.{field}")));
        }
    }
    for (field, target) in [
        ("str_mod", &mut modifiers.strength_millionths),
        ("dex_mod", &mut modifiers.dexterity_millionths),
        ("int_mod", &mut modifiers.intelligence_millionths),
        ("per_mod", &mut modifiers.perception_millionths),
        ("health_mod", &mut modifiers.health_millionths),
    ] {
        if let Some(value) = object.get(field) {
            *target = scaled_i64(value, raw, &format!("stat_hp_mods.{field}"))?;
        }
    }
    Ok(())
}

fn apply_id(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
) -> Result<(), AnatomyRegistryError> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or_else(|| AnatomyRegistryError::InvalidField(field.to_owned()))?
            .to_owned();
    }
    Ok(())
}

fn apply_scaled_u64(
    object: &Map<String, Value>,
    field: &str,
    target: &mut u64,
    raw: &RawDefinition,
) -> Result<(), AnatomyRegistryError> {
    if let Some(value) = object.get(field) {
        *target = u64::try_from(scaled_i64(value, raw, field)?).map_err(|_| invalid(raw, field))?;
    } else if let Some(value) = modifier(object, "proportional", field) {
        let multiplier = finite_number(value, raw, field)?;
        let adjusted = (*target as f64) * multiplier;
        if !adjusted.is_finite() || adjusted <= 0.0 || adjusted > u64::MAX as f64 {
            return Err(invalid(raw, field));
        }
        *target = adjusted as u64;
    } else if let Some(value) = modifier(object, "relative", field) {
        let addition = scaled_i64(value, raw, field)?;
        *target = if addition >= 0 {
            target.checked_add(addition as u64)
        } else {
            target.checked_sub(addition.unsigned_abs())
        }
        .ok_or_else(|| invalid(raw, field))?;
    }
    Ok(())
}

fn apply_scaled_i64(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i64,
    raw: &RawDefinition,
) -> Result<(), AnatomyRegistryError> {
    if let Some(value) = object.get(field) {
        *target = scaled_i64(value, raw, field)?;
    } else if let Some(value) = modifier(object, "proportional", field) {
        let multiplier = finite_number(value, raw, field)?;
        let adjusted = (*target as f64) * multiplier;
        if !adjusted.is_finite() || adjusted < i64::MIN as f64 || adjusted >= i64::MAX as f64 {
            return Err(invalid(raw, field));
        }
        *target = adjusted as i64;
    } else if let Some(value) = modifier(object, "relative", field) {
        *target = target
            .checked_add(scaled_i64(value, raw, field)?)
            .ok_or_else(|| invalid(raw, field))?;
    }
    Ok(())
}

fn apply_i32(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    raw: &RawDefinition,
) -> Result<(), AnatomyRegistryError> {
    if let Some(value) = object.get(field) {
        *target = i32::try_from(value.as_i64().ok_or_else(|| invalid(raw, field))?)
            .map_err(|_| invalid(raw, field))?;
    } else if let Some(value) = modifier(object, "proportional", field) {
        let adjusted = f64::from(*target) * finite_number(value, raw, field)?;
        if !adjusted.is_finite() || adjusted < f64::from(i32::MIN) || adjusted > f64::from(i32::MAX)
        {
            return Err(invalid(raw, field));
        }
        *target = adjusted as i32;
    } else if let Some(value) = modifier(object, "relative", field) {
        let addition = i32::try_from(value.as_i64().ok_or_else(|| invalid(raw, field))?)
            .map_err(|_| invalid(raw, field))?;
        *target = target
            .checked_add(addition)
            .ok_or_else(|| invalid(raw, field))?;
    }
    Ok(())
}

fn modifier<'a>(object: &'a Map<String, Value>, kind: &str, field: &str) -> Option<&'a Value> {
    object.get(kind)?.as_object()?.get(field)
}

fn scaled_i64(
    value: &Value,
    raw: &RawDefinition,
    field: &str,
) -> Result<i64, AnatomyRegistryError> {
    let scaled = finite_number(value, raw, field)? * ANATOMY_SCALE as f64;
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled >= i64::MAX as f64 {
        return Err(invalid(raw, field));
    }
    Ok(scaled.round() as i64)
}

fn finite_number(
    value: &Value,
    raw: &RawDefinition,
    field: &str,
) -> Result<f64, AnatomyRegistryError> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| invalid(raw, field))
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, AnatomyRegistryError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or_else(|| AnatomyRegistryError::InvalidField(field.to_owned()))
}

fn optional_string_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, AnatomyRegistryError> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .filter(|id| valid_id(id))
                .ok_or_else(|| AnatomyRegistryError::InvalidField(field.to_owned()))
        })
        .transpose()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

fn invalid(raw: &RawDefinition, field: &str) -> AnatomyRegistryError {
    AnatomyRegistryError::InvalidDefinition {
        source: raw.file.upstream_path.clone(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum AnatomyRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    InvalidField(String),
    InvalidDefinition { source: String, field: String },
    TooManyBodyParts,
    TooManyAnatomies,
    UnresolvedBodyPartInheritance(Vec<String>),
    InternalQueue,
}

impl fmt::Display for AnatomyRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "could not resolve anatomy content: {error}"),
            Self::Io(path, error) => write!(formatter, "could not read {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "could not parse {path}: {error}"),
            Self::InvalidField(field) => write!(formatter, "invalid anatomy field {field:?}"),
            Self::InvalidDefinition { source, field } => {
                write!(formatter, "invalid anatomy field {field:?} in {source}")
            }
            Self::TooManyBodyParts => write!(
                formatter,
                "body-part count exceeds {MAX_BODY_PART_DEFINITIONS}"
            ),
            Self::TooManyAnatomies => {
                write!(formatter, "anatomy count exceeds {MAX_ANATOMY_DEFINITIONS}")
            }
            Self::UnresolvedBodyPartInheritance(ids) => {
                write!(formatter, "unresolved body-part inheritance: {ids:?}")
            }
            Self::InternalQueue => write!(formatter, "anatomy resolver queue underflow"),
        }
    }
}

impl std::error::Error for AnatomyRegistryError {}
