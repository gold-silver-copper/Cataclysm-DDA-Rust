//! Pinned `npc_class` generic-factory definitions.
//!
//! This module keeps the mechanically observable class-loading kernel separate
//! from multiplayer NPC adaptation.  Unsupported class branches are retained
//! verbatim so runtime projection can reject them rather than fabricate NPC
//! state.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile, SkillRegistry};

const CLASS_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "name",
    "job_description",
    "common",
    "common_spawn_weight",
    "bonus_str",
    "bonus_dex",
    "bonus_int",
    "bonus_per",
    "bonus_aggression",
    "bonus_bravery",
    "bonus_collector",
    "bonus_altruism",
    "skills",
];
const DISTRIBUTION_FIELDS: &[&str] = &["constant", "one_in", "dice", "rng", "sum", "mul"];
pub const MAX_NPC_CLASS_DISTRIBUTION_DEPTH: usize = 64;
pub const MAX_NPC_CLASS_DISTRIBUTION_NODES: usize = 4_096;

pub(crate) fn field_is_implemented(field: &str) -> bool {
    CLASS_FIELDS.contains(&field)
}

/// Exact syntax tree loaded by pinned `load_distribution`.
///
/// Floating point leaves retain their IEEE-754 single-precision bits.  This
/// avoids host formatting or equality changing the values that C++ stores as
/// `float`, while leaving RNG evaluation to the simulation kernel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NpcClassDistributionDefinition {
    Constant { value_bits: u32 },
    OneIn { denominator_bits: u32 },
    Rng { from: i32, to: i32 },
    Dice { count: i32, sides: i32 },
    Sum(Vec<NpcClassDistributionDefinition>),
    Multiply(Vec<NpcClassDistributionDefinition>),
}

impl Default for NpcClassDistributionDefinition {
    fn default() -> Self {
        Self::Constant {
            value_bits: 0.0_f32.to_bits(),
        }
    }
}

impl NpcClassDistributionDefinition {
    #[must_use]
    pub fn constant(value: f32) -> Self {
        Self::Constant {
            value_bits: value.to_bits(),
        }
    }

    #[must_use]
    pub fn constant_value(&self) -> Option<f32> {
        match self {
            Self::Constant { value_bits } => Some(f32::from_bits(*value_bits)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpcClassDefinition {
    pub id: String,
    pub name: String,
    pub job_description: String,
    pub common: bool,
    /// Exact IEEE-754 bits of pinned `double common_spawn_weight`.
    pub common_spawn_weight_bits: u64,
    pub bonus_strength: NpcClassDistributionDefinition,
    pub bonus_dexterity: NpcClassDistributionDefinition,
    pub bonus_intelligence: NpcClassDistributionDefinition,
    pub bonus_perception: NpcClassDistributionDefinition,
    pub bonus_aggression: NpcClassDistributionDefinition,
    pub bonus_bravery: NpcClassDistributionDefinition,
    pub bonus_collector: NpcClassDistributionDefinition,
    pub bonus_altruism: NpcClassDistributionDefinition,
    /// Finalized, ID-sorted skill distributions.  Each bonus is represented as
    /// an ordered `Sum([level, bonus])`, matching pinned `operator+` order.
    pub skills: BTreeMap<String, NpcClassDistributionDefinition>,
    /// Unknown top-level class behavior inherited from the selected base and
    /// overwritten by later derived definitions.  Nonempty means fail closed.
    pub unsupported_fields: BTreeMap<String, Value>,
    /// Explicit skill IDs absent from the selected pinned skill factory.
    pub unresolved_skill_ids: BTreeSet<String>,
    pub source: String,
    bonus_skills: BTreeMap<String, NpcClassDistributionDefinition>,
}

impl Default for NpcClassDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            job_description: String::new(),
            common: true,
            common_spawn_weight_bits: 1.0_f64.to_bits(),
            bonus_strength: NpcClassDistributionDefinition::default(),
            bonus_dexterity: NpcClassDistributionDefinition::default(),
            bonus_intelligence: NpcClassDistributionDefinition::default(),
            bonus_perception: NpcClassDistributionDefinition::default(),
            bonus_aggression: NpcClassDistributionDefinition::default(),
            bonus_bravery: NpcClassDistributionDefinition::default(),
            bonus_collector: NpcClassDistributionDefinition::default(),
            bonus_altruism: NpcClassDistributionDefinition::default(),
            skills: BTreeMap::new(),
            unsupported_fields: BTreeMap::new(),
            unresolved_skill_ids: BTreeSet::new(),
            source: String::new(),
            bonus_skills: BTreeMap::new(),
        }
    }
}

impl NpcClassDefinition {
    #[must_use]
    pub fn common_spawn_weight(&self) -> f64 {
        f64::from_bits(self.common_spawn_weight_bits)
    }

    /// Whether this definition can be projected without inventing behavior or
    /// silently dropping a selected-content dependency.
    #[must_use]
    pub fn runtime_complete(&self) -> bool {
        self.unsupported_fields.is_empty() && self.unresolved_skill_ids.is_empty()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NpcClassRegistry {
    definitions: BTreeMap<String, NpcClassDefinition>,
    /// Pinned generic-factory vector order.  Replacements retain their slot.
    load_order: Vec<String>,
}

#[derive(Clone)]
struct RawNpcClass {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl NpcClassRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
        skills: &SkillRegistry,
    ) -> Result<Self, NpcClassRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(NpcClassRegistryError::Catalog)?;
        let mut pending = read_classes(content_root.as_ref(), files)?;
        let mut definitions = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let mut load_order = Vec::new();

        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(NpcClassRegistryError::InternalQueue)?;
                if load_one(&raw, &mut definitions, &mut abstracts, &mut load_order)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(NpcClassRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .map(raw_identity)
                        .collect::<Vec<_>>(),
                ));
            }
        }

        for definition in definitions.values_mut() {
            finalize_skills(definition, skills);
        }

        Ok(Self {
            definitions,
            load_order,
        })
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&NpcClassDefinition> {
        self.definitions.get(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Real definitions in pinned generic-factory vector order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &NpcClassDefinition)> {
        self.load_order.iter().map(|id| {
            let definition = &self.definitions[id];
            (id.as_str(), definition)
        })
    }

    #[must_use]
    pub fn load_order_ids(&self) -> &[String] {
        &self.load_order
    }
}

fn read_classes(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawNpcClass>, NpcClassRegistryError> {
    let mut classes = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| NpcClassRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| NpcClassRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_class(&mut classes, &file, value)?;
                }
            }
            value => collect_class(&mut classes, &file, value)?,
        }
    }
    Ok(classes)
}

fn collect_class(
    classes: &mut VecDeque<RawNpcClass>,
    file: &SelectedContentFile,
    value: Value,
) -> Result<(), NpcClassRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("npc_class") {
        return Ok(());
    }
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| NpcClassRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    definition_identity(&object, &file.upstream_path)?;
    classes.push_back(RawNpcClass {
        file: file.clone(),
        object,
    });
    Ok(())
}

fn load_one(
    raw: &RawNpcClass,
    definitions: &mut BTreeMap<String, NpcClassDefinition>,
    abstracts: &mut BTreeMap<String, NpcClassDefinition>,
    load_order: &mut Vec<String>,
) -> Result<bool, NpcClassRegistryError> {
    let source = &raw.file.upstream_path;
    let inherited = raw.object.contains_key("copy-from");
    let base = match raw.object.get("copy-from") {
        Some(value) => {
            let parent = required_id(value, source, "copy-from")?;
            let Some(base) = definitions.get(&parent).or_else(|| abstracts.get(&parent)) else {
                return Ok(false);
            };
            base.clone()
        }
        None => NpcClassDefinition::default(),
    };

    match definition_identity(&raw.object, source)? {
        DefinitionIdentity::Abstract(id) => {
            let mut definition = base;
            definition.id = id.clone();
            definition.source = format!("{source}#{id}");
            apply_fields(&mut definition, &raw.object, inherited)?;
            abstracts.insert(id, definition);
        }
        DefinitionIdentity::Real(ids) => {
            for id in ids {
                let mut definition = base.clone();
                definition.id = id.clone();
                definition.source = format!("{source}#{id}");
                apply_fields(&mut definition, &raw.object, inherited)?;
                if !definitions.contains_key(&id) {
                    load_order.push(id.clone());
                }
                definitions.insert(id, definition);
            }
        }
    }
    Ok(true)
}

enum DefinitionIdentity {
    Abstract(String),
    Real(Vec<String>),
}

fn definition_identity(
    object: &Map<String, Value>,
    source: &str,
) -> Result<DefinitionIdentity, NpcClassRegistryError> {
    match (object.get("id"), object.get("abstract")) {
        (Some(_), Some(_)) => Err(invalid(source, "id/abstract")),
        (None, None) => Err(invalid(source, "id/abstract")),
        (None, Some(value)) => Ok(DefinitionIdentity::Abstract(required_id(
            value, source, "abstract",
        )?)),
        (Some(value), None) => Ok(DefinitionIdentity::Real(ids(value, source, "id")?)),
    }
}

fn raw_identity(raw: &RawNpcClass) -> String {
    raw.object
        .get("id")
        .or_else(|| raw.object.get("abstract"))
        .map(Value::to_string)
        .unwrap_or_else(|| raw.file.upstream_path.clone())
}

fn apply_fields(
    definition: &mut NpcClassDefinition,
    object: &Map<String, Value>,
    inherited: bool,
) -> Result<(), NpcClassRegistryError> {
    let source = definition.source.clone();
    if let Some(value) = object.get("name") {
        definition.name = translated_string(value, &source, "name")?;
    } else if !inherited {
        return Err(invalid(&source, "name"));
    }
    if let Some(value) = object.get("job_description") {
        definition.job_description = translated_string(value, &source, "job_description")?;
    } else if !inherited {
        return Err(invalid(&source, "job_description"));
    }
    if let Some(value) = object.get("common") {
        definition.common = value.as_bool().ok_or_else(|| invalid(&source, "common"))?;
    }
    if let Some(value) = object.get("common_spawn_weight") {
        let weight = number(value, &source, "common_spawn_weight")?;
        if !definition.common {
            return Err(NpcClassRegistryError::SpawnWeightOnUncommon(source));
        }
        definition.common_spawn_weight_bits = weight.to_bits();
    }

    // Pinned npc_class::load assigns every distribution on every load.  An
    // omitted member therefore resets an inherited distribution to zero.
    definition.bonus_strength = optional_distribution(object, "bonus_str", &source)?;
    definition.bonus_dexterity = optional_distribution(object, "bonus_dex", &source)?;
    definition.bonus_intelligence = optional_distribution(object, "bonus_int", &source)?;
    definition.bonus_perception = optional_distribution(object, "bonus_per", &source)?;
    definition.bonus_aggression = optional_distribution(object, "bonus_aggression", &source)?;
    definition.bonus_bravery = optional_distribution(object, "bonus_bravery", &source)?;
    definition.bonus_collector = optional_distribution(object, "bonus_collector", &source)?;
    definition.bonus_altruism = optional_distribution(object, "bonus_altruism", &source)?;

    if let Some(value) = object.get("skills") {
        apply_skills(definition, value, &source)?;
    }

    for (field, value) in object {
        if !field.starts_with("//") && !CLASS_FIELDS.contains(&field.as_str()) {
            definition
                .unsupported_fields
                .insert(field.clone(), value.clone());
        }
    }
    Ok(())
}

fn apply_skills(
    definition: &mut NpcClassDefinition,
    value: &Value,
    source: &str,
) -> Result<(), NpcClassRegistryError> {
    let entries = value.as_array().ok_or_else(|| invalid(source, "skills"))?;
    for (index, value) in entries.iter().enumerate() {
        let field = format!("skills[{index}]");
        let object = value.as_object().ok_or_else(|| invalid(source, &field))?;
        let skill_ids = tags(object.get("skill"), source, &format!("{field}.skill"))?;

        for (member, value) in object {
            if !member.starts_with("//") && !matches!(member.as_str(), "skill" | "level" | "bonus")
            {
                definition
                    .unsupported_fields
                    .insert(format!("{field}.{member}"), value.clone());
            }
        }

        // The pinned branch accepts `level` only as an object.  Any other
        // shape, or specifying both branches, is rejected by its member/type
        // validation rather than becoming a zero-valued fallback.
        if let Some(level) = object.get("level") {
            if !level.is_object() || object.contains_key("bonus") {
                return Err(invalid(source, &field));
            }
            let distribution =
                parse_distribution_member(object.get("level"), source, &format!("{field}.level"))?;
            for skill_id in skill_ids {
                definition.skills.insert(skill_id, distribution.clone());
            }
        } else {
            let distribution = match object.get("bonus") {
                Some(value) => {
                    parse_distribution_member(Some(value), source, &format!("{field}.bonus"))?
                }
                None => NpcClassDistributionDefinition::default(),
            };
            for skill_id in skill_ids {
                definition
                    .bonus_skills
                    .insert(skill_id, distribution.clone());
            }
        }
    }
    Ok(())
}

fn finalize_skills(definition: &mut NpcClassDefinition, registry: &SkillRegistry) {
    apply_all_to_unassigned(&mut definition.skills, registry.skill_list_order_ids());
    apply_all_to_unassigned(
        &mut definition.bonus_skills,
        registry.skill_list_order_ids(),
    );

    let bonuses = std::mem::take(&mut definition.bonus_skills);
    for (skill_id, bonus) in bonuses {
        match definition.skills.remove(&skill_id) {
            Some(level) => {
                definition.skills.insert(
                    skill_id,
                    NpcClassDistributionDefinition::Sum(vec![level, bonus]),
                );
            }
            None => {
                definition.skills.insert(skill_id, bonus);
            }
        }
    }

    definition.unresolved_skill_ids = definition
        .skills
        .keys()
        .filter(|id| registry.get(id).is_none())
        .cloned()
        .collect();
}

fn apply_all_to_unassigned(
    distributions: &mut BTreeMap<String, NpcClassDistributionDefinition>,
    skill_load_order: &[String],
) {
    let Some(all) = distributions.remove("ALL") else {
        return;
    };
    for skill_id in skill_load_order {
        distributions
            .entry(skill_id.clone())
            .or_insert_with(|| all.clone());
    }
}

fn optional_distribution(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<NpcClassDistributionDefinition, NpcClassRegistryError> {
    match object.get(field) {
        Some(value) => parse_distribution_member(Some(value), source, field),
        None => Ok(NpcClassDistributionDefinition::default()),
    }
}

fn parse_distribution_member(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<NpcClassDistributionDefinition, NpcClassRegistryError> {
    let value = value.ok_or_else(|| invalid(source, field))?;
    if value.is_number() {
        return Ok(NpcClassDistributionDefinition::constant(number_f32(
            value, source, field,
        )?));
    }
    let object = value.as_object().ok_or_else(|| invalid(source, field))?;
    let mut nodes = 0;
    parse_distribution(object, source, field, 0, &mut nodes)
}

fn parse_distribution(
    object: &Map<String, Value>,
    source: &str,
    field: &str,
    depth: usize,
    nodes: &mut usize,
) -> Result<NpcClassDistributionDefinition, NpcClassRegistryError> {
    if depth > MAX_NPC_CLASS_DISTRIBUTION_DEPTH {
        return Err(NpcClassRegistryError::DistributionLimit {
            source: source.to_owned(),
            field: field.to_owned(),
        });
    }
    *nodes += 1;
    if *nodes > MAX_NPC_CLASS_DISTRIBUTION_NODES {
        return Err(NpcClassRegistryError::DistributionLimit {
            source: source.to_owned(),
            field: field.to_owned(),
        });
    }

    let members = object
        .iter()
        .filter(|(member, _)| !member.starts_with("//"))
        .collect::<Vec<_>>();
    if members.len() != 1 || !DISTRIBUTION_FIELDS.contains(&members[0].0.as_str()) {
        return Err(invalid(source, field));
    }
    let (kind, value) = members[0];
    match kind.as_str() {
        "constant" => Ok(NpcClassDistributionDefinition::constant(number_f32(
            value, source, field,
        )?)),
        "one_in" => Ok(NpcClassDistributionDefinition::OneIn {
            denominator_bits: number_f32(value, source, field)?.to_bits(),
        }),
        "rng" => {
            let [from, to] = pair(value, source, field)?;
            Ok(NpcClassDistributionDefinition::Rng { from, to })
        }
        "dice" => {
            let [count, sides] = pair(value, source, field)?;
            Ok(NpcClassDistributionDefinition::Dice { count, sides })
        }
        "sum" | "mul" => {
            let values = value
                .as_array()
                .filter(|values| !values.is_empty())
                .ok_or_else(|| invalid(source, field))?;
            let mut parsed = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let child = value
                    .as_object()
                    .ok_or_else(|| invalid(source, &format!("{field}[{index}]")))?;
                parsed.push(parse_distribution(
                    child,
                    source,
                    &format!("{field}[{index}]"),
                    depth + 1,
                    nodes,
                )?);
            }
            if kind == "sum" {
                Ok(NpcClassDistributionDefinition::Sum(parsed))
            } else {
                Ok(NpcClassDistributionDefinition::Multiply(parsed))
            }
        }
        _ => Err(invalid(source, field)),
    }
}

fn pair(value: &Value, source: &str, field: &str) -> Result<[i32; 2], NpcClassRegistryError> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| invalid(source, field))?;
    Ok([
        integer(&values[0], source, &format!("{field}[0]"))?,
        integer(&values[1], source, &format!("{field}[1]"))?,
    ])
}

fn tags(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, NpcClassRegistryError> {
    let Some(value) = value else {
        return Ok(BTreeSet::new());
    };
    let values = value
        .as_array()
        .map_or_else(|| std::slice::from_ref(value), Vec::as_slice);
    let mut result = BTreeSet::new();
    for value in values {
        let id = required_id(value, source, field)?;
        if !result.insert(id) {
            return Err(invalid(source, field));
        }
    }
    Ok(result)
}

fn ids(value: &Value, source: &str, field: &str) -> Result<Vec<String>, NpcClassRegistryError> {
    match value {
        Value::String(_) => Ok(vec![required_id(value, source, field)?]),
        Value::Array(values) if !values.is_empty() => values
            .iter()
            .map(|value| required_id(value, source, field))
            .collect(),
        _ => Err(invalid(source, field)),
    }
}

fn required_id(value: &Value, source: &str, field: &str) -> Result<String, NpcClassRegistryError> {
    value
        .as_str()
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| invalid(source, field))
}

fn translated_string(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<String, NpcClassRegistryError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .map(str::to_owned)
            .ok_or_else(|| invalid(source, field)),
        _ => Err(invalid(source, field)),
    }
}

fn integer(value: &Value, source: &str, field: &str) -> Result<i32, NpcClassRegistryError> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid(source, field))
}

fn number_f32(value: &Value, source: &str, field: &str) -> Result<f32, NpcClassRegistryError> {
    number(value, source, field).map(|value| value as f32)
}

fn number(value: &Value, source: &str, field: &str) -> Result<f64, NpcClassRegistryError> {
    value.as_f64().ok_or_else(|| invalid(source, field))
}

fn invalid(source: &str, field: &str) -> NpcClassRegistryError {
    NpcClassRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum NpcClassRegistryError {
    Catalog(ModCatalogError),
    DistributionLimit { source: String, field: String },
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    SpawnWeightOnUncommon(String),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for NpcClassRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "NPC class mod selection failed: {error}"),
            Self::DistributionLimit { source, field } => write!(
                formatter,
                "NPC class distribution limit exceeded for {field} in {source}"
            ),
            Self::InternalQueue => write!(formatter, "NPC class inheritance queue underflow"),
            Self::InvalidDefinition(source) => {
                write!(
                    formatter,
                    "NPC class definition is not an object in {source}"
                )
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid NPC class field {field} in {source}")
            }
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "NPC class registry I/O failed for {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "NPC class registry JSON failed for {path}: {error}"
                )
            }
            Self::SpawnWeightOnUncommon(source) => write!(
                formatter,
                "uncommon NPC class defines common_spawn_weight in {source}"
            ),
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved NPC class inheritance: {}",
                    ids.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for NpcClassRegistryError {}
