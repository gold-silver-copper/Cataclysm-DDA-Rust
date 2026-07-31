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
    "name",
    "description",
    "effect",
    "effect_str",
    "shape",
    "valid_targets",
    "flags",
    "min_damage",
    "max_damage",
    "damage_increment",
    "damage_type",
    "max_level",
    "min_range",
    "max_range",
    "range_increment",
    "min_aoe",
    "max_aoe",
    "aoe_increment",
    "base_casting_time",
    "final_casting_time",
    "casting_time_increment",
    "min_duration",
    "max_duration",
    "duration_increment",
    "field_id",
    "field_chance",
    "min_field_intensity",
    "max_field_intensity",
    "field_intensity_increment",
    "field_intensity_variance",
    "message",
    "sound_description",
    "sound_type",
    "sound_ambient",
    "sound_id",
    "sound_variant",
    "skill",
    "teachable",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpellEffectKind {
    Attack,
    EffectOnCondition,
    Summon,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpellDefinition {
    pub id: String,
    pub effect: SpellEffectKind,
    pub effect_str: String,
    pub shape: String,
    pub valid_targets: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub minimum_damage: i32,
    pub maximum_damage: i32,
    pub damage_increment_millionths: i64,
    pub damage_type_id: String,
    pub maximum_level: i32,
    pub minimum_range: i32,
    pub maximum_range: i32,
    pub range_increment_millionths: i64,
    pub minimum_aoe: i32,
    pub maximum_aoe: i32,
    pub aoe_increment_millionths: i64,
    pub base_casting_time_moves: i32,
    pub final_casting_time_moves: i32,
    pub casting_time_increment_millionths: i64,
    pub minimum_duration_moves: i32,
    pub maximum_duration_moves: i32,
    pub duration_increment_millionths: i64,
    pub field_type_id: String,
    pub field_chance: u32,
    pub minimum_field_intensity: i32,
    pub maximum_field_intensity: i32,
    pub field_intensity_increment_millionths: i64,
    pub field_intensity_variance_millionths: i64,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

impl Default for SpellDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            effect: SpellEffectKind::Unsupported,
            effect_str: String::new(),
            shape: String::new(),
            valid_targets: BTreeSet::new(),
            flags: BTreeSet::new(),
            minimum_damage: 0,
            maximum_damage: 0,
            damage_increment_millionths: 0,
            damage_type_id: String::new(),
            maximum_level: 0,
            minimum_range: 0,
            maximum_range: 0,
            range_increment_millionths: 0,
            minimum_aoe: 0,
            maximum_aoe: 0,
            aoe_increment_millionths: 0,
            base_casting_time_moves: 0,
            final_casting_time_moves: 0,
            casting_time_increment_millionths: 0,
            minimum_duration_moves: 0,
            maximum_duration_moves: 0,
            duration_increment_millionths: 0,
            field_type_id: String::new(),
            field_chance: 1,
            minimum_field_intensity: 0,
            maximum_field_intensity: 0,
            field_intensity_increment_millionths: 0,
            field_intensity_variance_millionths: 0,
            unsupported_fields: BTreeSet::new(),
            source: String::new(),
        }
    }
}

impl SpellDefinition {
    fn attack_field_is_supported(&self) -> bool {
        if self.field_type_id.is_empty() {
            return self.field_chance == 1
                && self.minimum_field_intensity == 0
                && self.maximum_field_intensity == 0
                && self.field_intensity_increment_millionths == 0
                && self.field_intensity_variance_millionths == 0;
        }
        (1..=1_000_000).contains(&self.field_chance)
            && self.minimum_field_intensity > 0
            && self.minimum_field_intensity <= self.maximum_field_intensity
            && self.maximum_field_intensity <= 255
            && self.field_intensity_increment_millionths.unsigned_abs() <= 1_000_000_000
            && (0..=1_000_000).contains(&self.field_intensity_variance_millionths)
    }

    #[must_use]
    pub fn supports_hostile_permanent_summoning(&self) -> bool {
        const ALLOWED_FLAGS: &[&str] = &[
            "HOSTILE_SUMMON",
            "PERMANENT",
            "RANDOM_DAMAGE",
            "NO_EXPLOSION_SFX",
            "SILENT",
            "NO_PROJECTILE",
            "IGNORE_WALLS",
        ];
        const ALLOWED_TARGETS: &[&str] = &["ground", "self"];
        self.unsupported_fields.is_empty()
            && self.effect == SpellEffectKind::Summon
            && self.shape == "blast"
            && !self.effect_str.is_empty()
            && self.flags.contains("HOSTILE_SUMMON")
            && self.flags.contains("PERMANENT")
            && self
                .flags
                .iter()
                .all(|flag| ALLOWED_FLAGS.contains(&flag.as_str()))
            && self
                .valid_targets
                .iter()
                .all(|target| ALLOWED_TARGETS.contains(&target.as_str()))
            && self.valid_targets.contains("ground")
            && self.maximum_level >= 0
            && self.damage_type_id.is_empty()
            && self.minimum_damage > 0
            && self.maximum_damage > 0
            && self.minimum_range >= 0
            && self.maximum_range >= 0
            && self.minimum_aoe >= 0
            && self.maximum_aoe >= 0
            && self.base_casting_time_moves >= 0
            && self.final_casting_time_moves >= 0
            && self.minimum_duration_moves == 0
            && self.maximum_duration_moves == 0
            && self.field_type_id.is_empty()
    }

    #[must_use]
    pub fn supports_hostile_typed_damage(&self) -> bool {
        const ALLOWED_FLAGS: &[&str] = &[
            "RANDOM_DAMAGE",
            "NO_PROJECTILE",
            "NO_EXPLOSION_SFX",
            "SILENT",
            "IGNORE_WALLS",
        ];
        self.unsupported_fields.is_empty()
            && self.effect == SpellEffectKind::Attack
            && self.shape == "blast"
            && !self.damage_type_id.is_empty()
            && self.effect_str.is_empty()
            && self
                .flags
                .iter()
                .all(|flag| ALLOWED_FLAGS.contains(&flag.as_str()))
            && self.flags.contains("NO_PROJECTILE")
            && self
                .valid_targets
                .iter()
                .all(|target| matches!(target.as_str(), "ground" | "hostile"))
            && self
                .valid_targets
                .iter()
                .any(|target| matches!(target.as_str(), "ground" | "hostile"))
            && self.maximum_level >= 0
            && self.minimum_damage > 0
            && self.maximum_damage > 0
            && self.minimum_range > 0
            && self.maximum_range > 0
            && self.minimum_aoe >= 0
            && self.maximum_aoe >= 0
            && self.base_casting_time_moves >= 0
            && self.final_casting_time_moves >= 0
            && self.minimum_duration_moves >= 0
            && self.maximum_duration_moves >= 0
            && self.attack_field_is_supported()
    }

    #[must_use]
    pub fn supports_hostile_status_effect(&self) -> bool {
        const ALLOWED_FLAGS: &[&str] = &[
            "RANDOM_DURATION",
            "NO_PROJECTILE",
            "NO_EXPLOSION_SFX",
            "SILENT",
            "IGNORE_WALLS",
        ];
        self.unsupported_fields.is_empty()
            && self.effect == SpellEffectKind::Attack
            && self.shape == "blast"
            && self.damage_type_id.is_empty()
            && !self.effect_str.is_empty()
            && self.minimum_damage == 0
            && self.maximum_damage == 0
            && self
                .flags
                .iter()
                .all(|flag| ALLOWED_FLAGS.contains(&flag.as_str()))
            && self.valid_targets.len() == 1
            && self.valid_targets.contains("hostile")
            && self.maximum_level >= 0
            && self.minimum_range > 0
            && self.maximum_range > 0
            && self.minimum_aoe == 0
            && self.maximum_aoe == 0
            && self.base_casting_time_moves >= 0
            && self.final_casting_time_moves >= 0
            && self.minimum_duration_moves > 0
            && self.maximum_duration_moves > 0
            && self.minimum_duration_moves % 100 == 0
            && self.maximum_duration_moves % 100 == 0
            && (!self.flags.contains("RANDOM_DURATION")
                || self.minimum_duration_moves == self.maximum_duration_moves)
            && self.attack_field_is_supported()
    }

    #[must_use]
    pub fn supports_hostile_effect_on_condition(&self) -> bool {
        const ALLOWED_FLAGS: &[&str] = &[
            "NO_PROJECTILE",
            "IGNORE_WALLS",
            "NO_EXPLOSION_SFX",
            "SILENT",
        ];
        self.unsupported_fields.is_empty()
            && self.effect == SpellEffectKind::EffectOnCondition
            && self.shape == "blast"
            && self.damage_type_id.is_empty()
            && !self.effect_str.is_empty()
            && self.minimum_damage == 0
            && self.maximum_damage == 0
            && self
                .flags
                .iter()
                .all(|flag| ALLOWED_FLAGS.contains(&flag.as_str()))
            && self.valid_targets.len() == 1
            && self.valid_targets.contains("hostile")
            && self.maximum_level >= 0
            && self.minimum_range > 0
            && self.maximum_range > 0
            && self.minimum_aoe == 0
            && self.maximum_aoe == 0
            && self.base_casting_time_moves >= 0
            && self.final_casting_time_moves >= 0
            && self.minimum_duration_moves == 0
            && self.maximum_duration_moves == 0
            && self.field_type_id.is_empty()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpellRegistry {
    spells: BTreeMap<String, SpellDefinition>,
}

#[derive(Clone)]
struct RawSpell {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl SpellRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, SpellRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(SpellRegistryError::Catalog)?;
        let mut pending = read_spells(content_root.as_ref(), files)?;
        let mut spells = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(SpellRegistryError::InternalQueue)?;
                if load_one(&raw, &mut spells)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(SpellRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| raw.object.get("id").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect(),
                ));
            }
        }
        Ok(Self { spells })
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SpellDefinition> {
        self.spells.get(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.spells.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spells.is_empty()
    }
}

fn read_spells(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawSpell>, SpellRegistryError> {
    let mut spells = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| SpellRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| SpellRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_spell(&mut spells, &file, value)?;
                }
            }
            value => collect_spell(&mut spells, &file, value)?,
        }
    }
    Ok(spells)
}

fn collect_spell(
    spells: &mut VecDeque<RawSpell>,
    file: &SelectedContentFile,
    value: Value,
) -> Result<(), SpellRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("SPELL") {
        return Ok(());
    }
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| SpellRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    spells.push_back(RawSpell {
        file: file.clone(),
        object,
    });
    Ok(())
}

fn load_one(
    raw: &RawSpell,
    spells: &mut BTreeMap<String, SpellDefinition>,
) -> Result<bool, SpellRegistryError> {
    let id = parse_id(raw.object.get("id"), &raw.file.upstream_path, "id")?;
    let mut spell = if let Some(parent) = raw.object.get("copy-from").and_then(Value::as_str) {
        let Some(base) = spells.get(parent) else {
            return Ok(false);
        };
        base.clone()
    } else {
        SpellDefinition::default()
    };
    spell.id = id.clone();
    spell.source = format!("{}#{id}", raw.file.upstream_path);
    apply_fields(&mut spell, &raw.object)?;
    spells.insert(id, spell);
    Ok(true)
}

fn apply_fields(
    spell: &mut SpellDefinition,
    object: &Map<String, Value>,
) -> Result<(), SpellRegistryError> {
    let source = spell.source.clone();
    if let Some(value) = object.get("effect") {
        spell.effect = match value.as_str() {
            Some("attack") => SpellEffectKind::Attack,
            Some("effect_on_condition") => SpellEffectKind::EffectOnCondition,
            Some("summon") => SpellEffectKind::Summon,
            Some(_) => SpellEffectKind::Unsupported,
            None => {
                spell.unsupported_fields.insert(String::from("effect"));
                SpellEffectKind::Unsupported
            }
        };
    }
    if let Some(value) = object.get("effect_str") {
        match parse_id(Some(value), &source, "effect_str") {
            Ok(value) => spell.effect_str = value,
            Err(_) => {
                spell.unsupported_fields.insert(String::from("effect_str"));
            }
        }
    }
    if let Some(value) = object.get("damage_type") {
        match parse_id(Some(value), &source, "damage_type") {
            Ok(value) => spell.damage_type_id = value,
            Err(_) => {
                spell.unsupported_fields.insert(String::from("damage_type"));
            }
        }
    }
    if let Some(value) = object.get("field_id") {
        match value.as_str() {
            Some("none") => {}
            Some(id) if !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control) => {
                spell.field_type_id = id.to_owned();
            }
            _ => {
                spell.unsupported_fields.insert(String::from("field_id"));
            }
        }
    }
    if let Some(value) = object.get("shape") {
        match parse_id(Some(value), &source, "shape") {
            Ok(value) => spell.shape = value,
            Err(_) => {
                spell.unsupported_fields.insert(String::from("shape"));
            }
        }
    }
    if let Some(value) = object.get("valid_targets") {
        match parse_string_set(value, &source, "valid_targets") {
            Ok(value) => spell.valid_targets = value,
            Err(_) => {
                spell
                    .unsupported_fields
                    .insert(String::from("valid_targets"));
            }
        }
    }
    if let Some(value) = object.get("flags") {
        match parse_string_set(value, &source, "flags") {
            Ok(value) => spell.flags = value,
            Err(_) => {
                spell.unsupported_fields.insert(String::from("flags"));
            }
        }
    }
    for (field, target) in [
        ("min_damage", &mut spell.minimum_damage),
        ("max_damage", &mut spell.maximum_damage),
        ("max_level", &mut spell.maximum_level),
        ("min_range", &mut spell.minimum_range),
        ("max_range", &mut spell.maximum_range),
        ("min_aoe", &mut spell.minimum_aoe),
        ("max_aoe", &mut spell.maximum_aoe),
        ("base_casting_time", &mut spell.base_casting_time_moves),
        ("final_casting_time", &mut spell.final_casting_time_moves),
        ("min_duration", &mut spell.minimum_duration_moves),
        ("max_duration", &mut spell.maximum_duration_moves),
        ("min_field_intensity", &mut spell.minimum_field_intensity),
        ("max_field_intensity", &mut spell.maximum_field_intensity),
    ] {
        if let Some(value) = object.get(field) {
            match parse_i32(value, &source, field) {
                Ok(value) => *target = value,
                Err(_) => {
                    spell.unsupported_fields.insert(field.to_owned());
                }
            }
        }
    }
    if let Some(value) = object.get("field_chance") {
        match parse_i32(value, &source, "field_chance")
            .and_then(|value| u32::try_from(value).map_err(|_| invalid(&source, "field_chance")))
        {
            Ok(value) => spell.field_chance = value,
            Err(_) => {
                spell
                    .unsupported_fields
                    .insert(String::from("field_chance"));
            }
        }
    }
    for (field, target) in [
        ("damage_increment", &mut spell.damage_increment_millionths),
        ("range_increment", &mut spell.range_increment_millionths),
        ("aoe_increment", &mut spell.aoe_increment_millionths),
        (
            "casting_time_increment",
            &mut spell.casting_time_increment_millionths,
        ),
        (
            "duration_increment",
            &mut spell.duration_increment_millionths,
        ),
        (
            "field_intensity_increment",
            &mut spell.field_intensity_increment_millionths,
        ),
        (
            "field_intensity_variance",
            &mut spell.field_intensity_variance_millionths,
        ),
    ] {
        if let Some(value) = object.get(field) {
            match parse_millionths(value, &source, field) {
                Ok(value) => *target = value,
                Err(_) => {
                    spell.unsupported_fields.insert(field.to_owned());
                }
            }
        }
    }
    if object
        .get("copy-from")
        .is_some_and(|value| !value.is_string())
    {
        spell.unsupported_fields.insert(String::from("copy-from"));
    }
    for field in object.keys() {
        if !field.starts_with("//") && !IMPLEMENTED_FIELDS.contains(&field.as_str()) {
            spell.unsupported_fields.insert(field.clone());
        }
    }
    Ok(())
}

fn parse_string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, SpellRegistryError> {
    let values = value
        .as_array()
        .map_or_else(|| std::slice::from_ref(value), Vec::as_slice);
    let parsed = values
        .iter()
        .map(|value| parse_id(Some(value), source, field))
        .collect::<Result<BTreeSet<_>, _>>()?;
    (!parsed.is_empty())
        .then_some(parsed)
        .ok_or_else(|| invalid(source, field))
}

fn parse_id(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<String, SpellRegistryError> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| invalid(source, field))
}

fn parse_i32(value: &Value, source: &str, field: &str) -> Result<i32, SpellRegistryError> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid(source, field))
}

fn parse_millionths(value: &Value, source: &str, field: &str) -> Result<i64, SpellRegistryError> {
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid(source, field))?;
    let scaled = (value * 1_000_000.0).round();
    (scaled >= i64::MIN as f64 && scaled <= i64::MAX as f64)
        .then_some(scaled as i64)
        .ok_or_else(|| invalid(source, field))
}

fn invalid(source: &str, field: &str) -> SpellRegistryError {
    SpellRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum SpellRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for SpellRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "spell mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("spell load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "spell is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid spell field {field} in {source}")
            }
            Self::Io(path, error) => write!(formatter, "spell I/O failed for {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "spell JSON failed for {path}: {error}"),
            Self::UnresolvedInheritance(ids) => {
                write!(formatter, "unresolved or cyclic spell inheritance: {ids:?}")
            }
        }
    }
}

impl std::error::Error for SpellRegistryError {}
