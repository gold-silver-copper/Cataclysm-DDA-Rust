use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "copy-from",
    "trigger_chance",
    "aoe",
    "trail",
    "on_hit_effects",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmmunitionFieldEffectDefinition {
    pub field_type_id: String,
    pub intensity_minimum: u8,
    pub intensity_maximum: u8,
    pub chance_percent: u8,
    pub radius: u8,
    pub radius_z: u8,
    pub size: u8,
    pub check_passable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmmunitionOnHitEffectDefinition {
    pub effect_id: String,
    pub duration_seconds: u64,
    pub intensity: u32,
    pub need_touch_skin: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AmmunitionEffectDefinition {
    pub id: String,
    pub trigger_chance_percent: u8,
    pub area_fields: Vec<AmmunitionFieldEffectDefinition>,
    pub trail_fields: Vec<AmmunitionFieldEffectDefinition>,
    pub on_hit_effects: Vec<AmmunitionOnHitEffectDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AmmunitionEffectRegistry {
    effects: BTreeMap<String, AmmunitionEffectDefinition>,
}

#[derive(Clone)]
struct RawEffect {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl AmmunitionEffectRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, AmmunitionEffectRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(AmmunitionEffectRegistryError::Catalog)?;
        let mut pending = read_effects(content_root.as_ref(), files)?;
        let mut effects = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(AmmunitionEffectRegistryError::InternalQueue)?;
                if load_one(&raw, &mut effects)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(AmmunitionEffectRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| raw.object.get("id").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect(),
                ));
            }
        }
        Ok(Self { effects })
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AmmunitionEffectDefinition> {
        self.effects.get(id)
    }
}

fn read_effects(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawEffect>, AmmunitionEffectRegistryError> {
    let mut effects = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| AmmunitionEffectRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            AmmunitionEffectRegistryError::Json(file.destination.clone(), error)
        })?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_effect(&file, value, &mut effects)?;
                }
            }
            value => collect_effect(&file, value, &mut effects)?,
        }
    }
    Ok(effects)
}

fn collect_effect(
    file: &SelectedContentFile,
    value: Value,
    effects: &mut VecDeque<RawEffect>,
) -> Result<(), AmmunitionEffectRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("ammo_effect") {
        return Ok(());
    }
    let object = value.as_object().cloned().ok_or_else(|| {
        AmmunitionEffectRegistryError::InvalidDefinition(file.upstream_path.clone())
    })?;
    effects.push_back(RawEffect {
        file: file.clone(),
        object,
    });
    Ok(())
}

fn load_one(
    raw: &RawEffect,
    effects: &mut BTreeMap<String, AmmunitionEffectDefinition>,
) -> Result<bool, AmmunitionEffectRegistryError> {
    let id = raw
        .object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .ok_or(AmmunitionEffectRegistryError::InvalidIdentity)?;
    let mut effect = if let Some(parent) = raw.object.get("copy-from").and_then(Value::as_str) {
        let Some(base) = effects.get(parent) else {
            return Ok(false);
        };
        base.clone()
    } else {
        AmmunitionEffectDefinition {
            trigger_chance_percent: 100,
            ..AmmunitionEffectDefinition::default()
        }
    };
    effect.id = id.to_owned();
    effect.source.clone_from(&raw.file.upstream_path);
    let source = format!("{}#{id}", raw.file.upstream_path);
    apply_fields(&mut effect, &raw.object, &source)?;
    effects.insert(id.to_owned(), effect);
    Ok(true)
}

fn apply_fields(
    effect: &mut AmmunitionEffectDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), AmmunitionEffectRegistryError> {
    if let Some(value) = object.get("trigger_chance") {
        effect.trigger_chance_percent = parse_u8(value, 0, 100, source, "trigger_chance")?;
    }
    if let Some(value) = object.get("aoe") {
        effect.area_fields =
            parse_field_effects(value, true, source, &mut effect.unsupported_fields)?;
    }
    if let Some(value) = object.get("trail") {
        effect.trail_fields =
            parse_field_effects(value, false, source, &mut effect.unsupported_fields)?;
    }
    if let Some(value) = object.get("on_hit_effects") {
        effect.on_hit_effects =
            parse_on_hit_effects(value, source, &mut effect.unsupported_fields)?;
    }
    for field in object.keys() {
        if !field.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&field.as_str()) {
            effect.unsupported_fields.insert(field.clone());
        }
    }
    Ok(())
}

fn parse_field_effects(
    value: &Value,
    area: bool,
    source: &str,
    unsupported: &mut BTreeSet<String>,
) -> Result<Vec<AmmunitionFieldEffectDefinition>, AmmunitionEffectRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid(source, if area { "aoe" } else { "trail" }))?
        .iter()
        .map(|value| {
            let object = value
                .as_object()
                .ok_or_else(|| invalid(source, if area { "aoe" } else { "trail" }))?;
            let prefix = if area { "aoe" } else { "trail" };
            let field_type_id = object
                .get("field_type")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
                .ok_or_else(|| invalid(source, prefix))?
                .to_owned();
            let intensity_minimum = object
                .get("intensity_min")
                .map(|value| parse_u8(value, 0, u8::MAX, source, prefix))
                .transpose()?
                .unwrap_or(0);
            let intensity_maximum = object
                .get("intensity_max")
                .map(|value| parse_u8(value, intensity_minimum, u8::MAX, source, prefix))
                .transpose()?
                .unwrap_or(intensity_minimum);
            let allowed = if area {
                [
                    "field_type",
                    "intensity_min",
                    "intensity_max",
                    "radius",
                    "radius_z",
                    "chance",
                    "size",
                    "check_passable",
                ]
                .as_slice()
            } else {
                ["field_type", "intensity_min", "intensity_max", "chance"].as_slice()
            };
            for field in object.keys() {
                if !field.starts_with("//") && !allowed.contains(&field.as_str()) {
                    unsupported.insert(format!("{prefix}.{field}"));
                }
            }
            Ok(AmmunitionFieldEffectDefinition {
                field_type_id,
                intensity_minimum,
                intensity_maximum,
                chance_percent: object
                    .get("chance")
                    .map(|value| parse_u8(value, 0, 100, source, prefix))
                    .transpose()?
                    .unwrap_or(100),
                radius: if area {
                    object
                        .get("radius")
                        .map(|value| parse_u8(value, 0, 64, source, prefix))
                        .transpose()?
                        .unwrap_or(1)
                } else {
                    0
                },
                radius_z: if area {
                    object
                        .get("radius_z")
                        .map(|value| parse_u8(value, 0, 16, source, prefix))
                        .transpose()?
                        .unwrap_or(0)
                } else {
                    0
                },
                size: if area {
                    object
                        .get("size")
                        .map(|value| parse_u8(value, 0, 64, source, prefix))
                        .transpose()?
                        .unwrap_or(0)
                } else {
                    0
                },
                check_passable: if area {
                    object
                        .get("check_passable")
                        .map(|value| value.as_bool().ok_or_else(|| invalid(source, prefix)))
                        .transpose()?
                        .unwrap_or(false)
                } else {
                    false
                },
            })
        })
        .collect()
}

fn parse_on_hit_effects(
    value: &Value,
    source: &str,
    unsupported: &mut BTreeSet<String>,
) -> Result<Vec<AmmunitionOnHitEffectDefinition>, AmmunitionEffectRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid(source, "on_hit_effects"))?
        .iter()
        .map(|value| {
            let object = value
                .as_object()
                .ok_or_else(|| invalid(source, "on_hit_effects"))?;
            for field in object.keys() {
                if !field.starts_with("//")
                    && !["effect", "duration", "intensity", "need_touch_skin"]
                        .contains(&field.as_str())
                {
                    unsupported.insert(format!("on_hit_effects.{field}"));
                }
            }
            Ok(AmmunitionOnHitEffectDefinition {
                effect_id: object
                    .get("effect")
                    .and_then(Value::as_str)
                    .filter(|id| {
                        !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
                    })
                    .ok_or_else(|| invalid(source, "on_hit_effects"))?
                    .to_owned(),
                duration_seconds: parse_duration_seconds(
                    object
                        .get("duration")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid(source, "on_hit_effects"))?,
                    source,
                )?,
                intensity: object
                    .get("intensity")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value > 0)
                    .ok_or_else(|| invalid(source, "on_hit_effects"))?,
                need_touch_skin: object
                    .get("need_touch_skin")
                    .map(|value| {
                        value
                            .as_bool()
                            .ok_or_else(|| invalid(source, "on_hit_effects"))
                    })
                    .transpose()?
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn parse_u8(
    value: &Value,
    minimum: u8,
    maximum: u8,
    source: &str,
    field: &str,
) -> Result<u8, AmmunitionEffectRegistryError> {
    value
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| invalid(source, field))
}

fn parse_duration_seconds(value: &str, source: &str) -> Result<u64, AmmunitionEffectRegistryError> {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut total = 0_u64;
    let mut terms = 0;
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
        let amount = value[start..index]
            .parse::<u64>()
            .map_err(|_| invalid(source, "duration"))?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let multiplier = match &value[start..index] {
            "turn" | "turns" | "s" | "second" | "seconds" => 1,
            "m" | "minute" | "minutes" => 60,
            "h" | "hour" | "hours" => 3_600,
            "d" | "day" | "days" => 86_400,
            _ => return Err(invalid(source, "duration")),
        };
        total = total
            .checked_add(
                amount
                    .checked_mul(multiplier)
                    .ok_or_else(|| invalid(source, "duration"))?,
            )
            .ok_or_else(|| invalid(source, "duration"))?;
        terms += 1;
    }
    (terms > 0)
        .then_some(total)
        .ok_or_else(|| invalid(source, "duration"))
}

fn invalid(source: &str, field: &str) -> AmmunitionEffectRegistryError {
    AmmunitionEffectRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum AmmunitionEffectRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    InvalidIdentity,
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for AmmunitionEffectRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => {
                write!(formatter, "ammunition-effect mod selection failed: {error}")
            }
            Self::InternalQueue => formatter.write_str("ammunition-effect load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "ammunition effect is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(
                    formatter,
                    "invalid ammunition-effect field {field} in {source}"
                )
            }
            Self::InvalidIdentity => {
                formatter.write_str("ammunition effect must have a bounded non-empty id")
            }
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "ammunition-effect I/O failed for {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "ammunition-effect JSON failed for {path}: {error}"
                )
            }
            Self::UnresolvedInheritance(ids) => write!(
                formatter,
                "unresolved or cyclic ammunition-effect inheritance: {ids:?}"
            ),
        }
    }
}

impl std::error::Error for AmmunitionEffectRegistryError {}
