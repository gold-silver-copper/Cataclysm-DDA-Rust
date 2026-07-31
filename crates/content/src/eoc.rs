use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_EOC_TREE_DEPTH: usize = 64;
const MAX_EOC_STRING_VALUES: usize = 256;
const MAX_EOC_VARIABLE_VALUE_BYTES: usize = 16 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EocStringValueDefinition {
    Literal(String),
    ActorVariable(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EocDelayDefinition {
    pub minimum_turns: u32,
    pub maximum_turns: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EocConditionDefinition {
    Constant(bool),
    HasEffect {
        effect_id: String,
        body_part_id: Option<String>,
        minimum_intensity: u32,
    },
    HasAnyEffect {
        effect_ids: Vec<String>,
        body_part_id: Option<String>,
        minimum_intensity: u32,
    },
    CompareString(Vec<EocStringValueDefinition>),
    CompareStringAll(Vec<EocStringValueDefinition>),
    HasItem {
        item_type_id: String,
        minimum_count: u32,
        minimum_charges: u32,
    },
    HasWeapon,
    IsWearing {
        item_type_id: String,
    },
    HasProficiency {
        proficiency_id: String,
    },
    KnowsRecipe {
        recipe_id: String,
    },
    StatAtLeast {
        stat: EocActorStatDefinition,
        minimum: i32,
    },
    Not(Box<Self>),
    And(Vec<Self>),
    Or(Vec<Self>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EocActorStatDefinition {
    Strength,
    Dexterity,
    Intelligence,
    Perception,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EocEffectDefinition {
    Message {
        text: String,
    },
    AddEffect {
        effect_id: String,
        body_part_id: Option<String>,
        duration_turns: u32,
        permanent: bool,
        intensity: u32,
    },
    RemoveEffects {
        effect_ids: Vec<String>,
        body_part_id: Option<String>,
    },
    SetActorVariable {
        variable_id: String,
        possible_values: Vec<String>,
    },
    RemoveActorVariable {
        variable_id: String,
    },
    RunEocs {
        eoc_ids: Vec<String>,
        delay: Option<EocDelayDefinition>,
    },
    Conditional {
        condition: EocConditionDefinition,
        then_effects: Vec<Self>,
        else_effects: Vec<Self>,
    },
}

impl EocEffectDefinition {
    pub fn collect_referenced_eocs<'a>(&'a self, target: &mut Vec<&'a str>) {
        match self {
            Self::RunEocs { eoc_ids, .. } => target.extend(eoc_ids.iter().map(String::as_str)),
            Self::Conditional {
                then_effects,
                else_effects,
                ..
            } => {
                for effect in then_effects.iter().chain(else_effects) {
                    effect.collect_referenced_eocs(target);
                }
            }
            Self::Message { .. }
            | Self::AddEffect { .. }
            | Self::RemoveEffects { .. }
            | Self::SetActorVariable { .. }
            | Self::RemoveActorVariable { .. } => {}
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectOnConditionDefinition {
    pub id: String,
    pub condition: Option<EocConditionDefinition>,
    pub effects: Vec<EocEffectDefinition>,
    pub false_effects: Vec<EocEffectDefinition>,
    pub recurrence: Option<EocDelayDefinition>,
    pub deactivate_condition: Option<EocConditionDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

impl EffectOnConditionDefinition {
    #[must_use]
    pub fn is_fully_supported(&self) -> bool {
        self.unsupported_fields.is_empty()
    }

    #[must_use]
    pub fn referenced_eocs(&self) -> Vec<&str> {
        let mut referenced = Vec::new();
        for effect in self.effects.iter().chain(&self.false_effects) {
            effect.collect_referenced_eocs(&mut referenced);
        }
        referenced
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EffectOnConditionRegistry {
    definitions: BTreeMap<String, EffectOnConditionDefinition>,
}

#[derive(Clone)]
struct RawEoc {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl EffectOnConditionRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, EffectOnConditionRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(EffectOnConditionRegistryError::Catalog)?;
        let mut pending = read_eocs(content_root.as_ref(), files)?;
        let mut definitions = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(EffectOnConditionRegistryError::InternalQueue)?;
                if load_one(&raw, &mut definitions)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(EffectOnConditionRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| raw.object.get("id").and_then(Value::as_str))
                        .map(str::to_owned)
                        .collect(),
                ));
            }
        }
        Ok(Self { definitions })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&EffectOnConditionDefinition> {
        self.definitions.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &EffectOnConditionDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

pub(crate) fn parse_inline_eoc(
    object: &Map<String, Value>,
    source: &str,
) -> Result<EffectOnConditionDefinition, EffectOnConditionRegistryError> {
    parse_definition(object, None, source)
}

fn read_eocs(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawEoc>, EffectOnConditionRegistryError> {
    let mut pending = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| EffectOnConditionRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            EffectOnConditionRegistryError::Json(file.destination.clone(), error)
        })?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_eoc(&file, value, &mut pending)?;
                }
            }
            value => collect_eoc(&file, value, &mut pending)?,
        }
    }
    Ok(pending)
}

fn collect_eoc(
    file: &SelectedContentFile,
    value: Value,
    pending: &mut VecDeque<RawEoc>,
) -> Result<(), EffectOnConditionRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("effect_on_condition") {
        return Ok(());
    }
    pending.push_back(RawEoc {
        file: file.clone(),
        object: value
            .as_object()
            .cloned()
            .ok_or_else(|| invalid(&file.upstream_path, "effect_on_condition"))?,
    });
    Ok(())
}

fn load_one(
    raw: &RawEoc,
    definitions: &mut BTreeMap<String, EffectOnConditionDefinition>,
) -> Result<bool, EffectOnConditionRegistryError> {
    let parent = raw.object.get("copy-from").and_then(Value::as_str);
    let base = match parent {
        Some(parent) => {
            let Some(base) = definitions.get(parent) else {
                return Ok(false);
            };
            Some(base)
        }
        None => None,
    };
    let id = raw
        .object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or_else(|| invalid(&raw.file.upstream_path, "id"))?;
    let source = format!("{}#{id}", raw.file.upstream_path);
    let definition = parse_definition(&raw.object, base, &source)?;
    definitions.insert(id.to_owned(), definition);
    Ok(true)
}

fn parse_definition(
    object: &Map<String, Value>,
    base: Option<&EffectOnConditionDefinition>,
    source: &str,
) -> Result<EffectOnConditionDefinition, EffectOnConditionRegistryError> {
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or_else(|| invalid(source, "id"))?;
    let mut definition = base.cloned().unwrap_or(EffectOnConditionDefinition {
        id: id.to_owned(),
        condition: None,
        effects: Vec::new(),
        false_effects: Vec::new(),
        recurrence: None,
        deactivate_condition: None,
        unsupported_fields: BTreeSet::new(),
        source: source.to_owned(),
    });
    definition.id = id.to_owned();
    definition.source = source.to_owned();

    if let Some(value) = object.get("condition") {
        definition
            .unsupported_fields
            .retain(|field| !field.starts_with("condition"));
        definition.condition =
            parse_condition(value, "condition", 0, &mut definition.unsupported_fields);
    }
    if let Some(value) = object.get("effect") {
        definition
            .unsupported_fields
            .retain(|field| !field.starts_with("effect"));
        definition.effects = parse_effects(value, "effect", 0, &mut definition.unsupported_fields);
    }
    if let Some(value) = object.get("false_effect") {
        definition
            .unsupported_fields
            .retain(|field| !field.starts_with("false_effect"));
        definition.false_effects =
            parse_effects(value, "false_effect", 0, &mut definition.unsupported_fields);
    }
    if let Some(value) = object.get("recurrence") {
        definition.unsupported_fields.remove("recurrence");
        definition.recurrence = parse_delay(value);
        if definition.recurrence.is_none() {
            definition
                .unsupported_fields
                .insert(String::from("recurrence"));
        }
    }
    if let Some(value) = object.get("deactivate_condition") {
        definition
            .unsupported_fields
            .retain(|field| !field.starts_with("deactivate_condition"));
        definition.deactivate_condition = parse_condition(
            value,
            "deactivate_condition",
            0,
            &mut definition.unsupported_fields,
        );
    }

    const TOP_LEVEL_FIELDS: &[&str] = &[
        "type",
        "id",
        "copy-from",
        "condition",
        "effect",
        "false_effect",
        "eoc_type",
        "global",
        "run_for_npcs",
        "recurrence",
        "deactivate_condition",
        "required_event",
    ];
    for field in object.keys().filter(|field| !field.starts_with("//")) {
        if !TOP_LEVEL_FIELDS.contains(&field.as_str()) {
            definition.unsupported_fields.insert(field.clone());
        }
    }
    if let Some(eoc_type) = object.get("eoc_type") {
        definition.unsupported_fields.remove("eoc_type");
        match eoc_type.as_str() {
            Some("ACTIVATION") if !object.contains_key("recurrence") => {
                definition.recurrence = None;
                definition.deactivate_condition = None;
            }
            Some("RECURRING") if definition.recurrence.is_some() => {}
            _ => {
                definition
                    .unsupported_fields
                    .insert(String::from("eoc_type"));
            }
        }
    }
    if object.contains_key("recurrence")
        && object
            .get("eoc_type")
            .is_some_and(|value| value.as_str() != Some("RECURRING"))
    {
        definition
            .unsupported_fields
            .insert(String::from("eoc_type"));
    }
    if definition.deactivate_condition.is_some() && definition.recurrence.is_none() {
        definition
            .unsupported_fields
            .insert(String::from("deactivate_condition"));
    }
    if object.contains_key("required_event") {
        definition
            .unsupported_fields
            .insert(String::from("required_event"));
    }
    for field in ["global", "run_for_npcs"] {
        if object
            .get(field)
            .is_some_and(|value| value.as_bool() != Some(false))
        {
            definition.unsupported_fields.insert(field.to_owned());
        }
    }
    Ok(definition)
}

fn parse_condition(
    value: &Value,
    path: &str,
    depth: usize,
    unsupported: &mut BTreeSet<String>,
) -> Option<EocConditionDefinition> {
    if depth >= MAX_EOC_TREE_DEPTH {
        unsupported.insert(path.to_owned());
        return None;
    }
    if let Some(value) = value.as_bool() {
        return Some(EocConditionDefinition::Constant(value));
    }
    if value.as_str() == Some("u_has_weapon") {
        return Some(EocConditionDefinition::HasWeapon);
    }
    let Some(object) = value.as_object() else {
        unsupported.insert(path.to_owned());
        return None;
    };
    if let Some(value) = object.get("not") {
        if object.len() != 1 {
            unsupported.insert(path.to_owned());
            return None;
        }
        return parse_condition(value, &format!("{path}.not"), depth + 1, unsupported)
            .map(|condition| EocConditionDefinition::Not(Box::new(condition)));
    }
    for (field, is_and) in [("and", true), ("or", false)] {
        if let Some(values) = object.get(field) {
            if object.len() != 1 {
                unsupported.insert(path.to_owned());
                return None;
            }
            let Some(values) = values.as_array().filter(|values| !values.is_empty()) else {
                unsupported.insert(format!("{path}.{field}"));
                return None;
            };
            let mut conditions = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let Some(condition) = parse_condition(
                    value,
                    &format!("{path}.{field}[{index}]"),
                    depth + 1,
                    unsupported,
                ) else {
                    return None;
                };
                conditions.push(condition);
            }
            return Some(if is_and {
                EocConditionDefinition::And(conditions)
            } else {
                EocConditionDefinition::Or(conditions)
            });
        }
    }
    for (field, match_all) in [
        ("compare_string", false),
        ("compare_string_match_all", true),
    ] {
        if let Some(values) = object.get(field) {
            if object.len() != 1 {
                unsupported.insert(path.to_owned());
                return None;
            }
            let Some(values) = values
                .as_array()
                .filter(|values| (2..=MAX_EOC_STRING_VALUES).contains(&values.len()))
            else {
                unsupported.insert(format!("{path}.{field}"));
                return None;
            };
            let mut resolved = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let Some(value) = parse_string_value(value) else {
                    unsupported.insert(format!("{path}.{field}[{index}]"));
                    return None;
                };
                resolved.push(value);
            }
            return Some(if match_all {
                EocConditionDefinition::CompareStringAll(resolved)
            } else {
                EocConditionDefinition::CompareString(resolved)
            });
        }
    }
    if let Some(item) = object.get("u_has_item") {
        if object.len() != 1 {
            unsupported.insert(path.to_owned());
            return None;
        }
        let Some(item_type_id) = item.as_str().filter(|id| valid_id(id)) else {
            unsupported.insert(format!("{path}.u_has_item"));
            return None;
        };
        return Some(EocConditionDefinition::HasItem {
            item_type_id: item_type_id.to_owned(),
            minimum_count: 1,
            minimum_charges: 0,
        });
    }
    if let Some(requirement) = object.get("u_has_items") {
        if object.len() != 1 {
            unsupported.insert(path.to_owned());
            return None;
        }
        let Some(requirement) = requirement.as_object() else {
            unsupported.insert(format!("{path}.u_has_items"));
            return None;
        };
        if requirement
            .keys()
            .any(|field| !matches!(field.as_str(), "item" | "count" | "charges"))
        {
            unsupported.insert(format!("{path}.u_has_items"));
            return None;
        }
        let Some(item_type_id) = requirement
            .get("item")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
        else {
            unsupported.insert(format!("{path}.u_has_items.item"));
            return None;
        };
        let minimum_count = requirement.get("count").map_or(Some(0), parse_u32_literal);
        let minimum_charges = requirement
            .get("charges")
            .map_or(Some(0), parse_u32_literal);
        let (Some(minimum_count), Some(minimum_charges)) = (minimum_count, minimum_charges) else {
            unsupported.insert(format!("{path}.u_has_items"));
            return None;
        };
        if (minimum_count == 0 && minimum_charges == 0)
            || (!requirement.contains_key("count") && !requirement.contains_key("charges"))
        {
            unsupported.insert(format!("{path}.u_has_items"));
            return None;
        }
        return Some(EocConditionDefinition::HasItem {
            item_type_id: item_type_id.to_owned(),
            minimum_count,
            minimum_charges,
        });
    }
    for field in ["u_is_wearing", "u_has_proficiency", "u_know_recipe"] {
        if let Some(value) = object.get(field) {
            if object.len() != 1 {
                unsupported.insert(path.to_owned());
                return None;
            }
            let Some(id) = value.as_str().filter(|id| valid_id(id)) else {
                unsupported.insert(format!("{path}.{field}"));
                return None;
            };
            return Some(match field {
                "u_is_wearing" => EocConditionDefinition::IsWearing {
                    item_type_id: id.to_owned(),
                },
                "u_has_proficiency" => EocConditionDefinition::HasProficiency {
                    proficiency_id: id.to_owned(),
                },
                "u_know_recipe" => EocConditionDefinition::KnowsRecipe {
                    recipe_id: id.to_owned(),
                },
                _ => unreachable!(),
            });
        }
    }
    for (field, stat) in [
        ("u_has_strength", EocActorStatDefinition::Strength),
        ("u_has_dexterity", EocActorStatDefinition::Dexterity),
        ("u_has_intelligence", EocActorStatDefinition::Intelligence),
        ("u_has_perception", EocActorStatDefinition::Perception),
    ] {
        if let Some(value) = object.get(field) {
            if object.len() != 1 {
                unsupported.insert(path.to_owned());
                return None;
            }
            let Some(minimum) = value.as_i64().and_then(|value| i32::try_from(value).ok()) else {
                unsupported.insert(format!("{path}.{field}"));
                return None;
            };
            return Some(EocConditionDefinition::StatAtLeast { stat, minimum });
        }
    }
    if let Some(effect) = object.get("u_has_effect") {
        let Some(effect_id) = effect.as_str().filter(|id| valid_id(id)) else {
            unsupported.insert(format!("{path}.u_has_effect"));
            return None;
        };
        if object.keys().any(|field| {
            !matches!(
                field.as_str(),
                "u_has_effect" | "bodypart" | "target_part" | "intensity"
            )
        }) {
            unsupported.insert(path.to_owned());
            return None;
        }
        let body_part_id = object
            .get("bodypart")
            .or_else(|| object.get("target_part"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if body_part_id.as_deref().is_some_and(|id| !valid_id(id)) {
            unsupported.insert(path.to_owned());
            return None;
        }
        let Some(minimum_intensity) = object.get("intensity").map_or(Some(0), parse_u32_literal)
        else {
            unsupported.insert(format!("{path}.intensity"));
            return None;
        };
        return Some(EocConditionDefinition::HasEffect {
            effect_id: effect_id.to_owned(),
            body_part_id,
            minimum_intensity,
        });
    }
    if let Some(effects) = object.get("u_has_any_effect") {
        if object.keys().any(|field| {
            !matches!(
                field.as_str(),
                "u_has_any_effect" | "bodypart" | "target_part" | "intensity"
            )
        }) {
            unsupported.insert(path.to_owned());
            return None;
        }
        let Some(effect_ids) = effects
            .as_array()
            .filter(|effects| (1..=MAX_EOC_STRING_VALUES).contains(&effects.len()))
            .and_then(|effects| {
                effects
                    .iter()
                    .map(|effect| effect.as_str().filter(|id| valid_id(id)).map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
            })
        else {
            unsupported.insert(format!("{path}.u_has_any_effect"));
            return None;
        };
        let body_part_id = object
            .get("bodypart")
            .or_else(|| object.get("target_part"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if body_part_id.as_deref().is_some_and(|id| !valid_id(id)) {
            unsupported.insert(path.to_owned());
            return None;
        }
        let Some(minimum_intensity) = object.get("intensity").map_or(Some(0), parse_u32_literal)
        else {
            unsupported.insert(format!("{path}.intensity"));
            return None;
        };
        return Some(EocConditionDefinition::HasAnyEffect {
            effect_ids,
            body_part_id,
            minimum_intensity,
        });
    }
    unsupported.insert(path.to_owned());
    None
}

fn parse_effects(
    value: &Value,
    path: &str,
    depth: usize,
    unsupported: &mut BTreeSet<String>,
) -> Vec<EocEffectDefinition> {
    if depth >= MAX_EOC_TREE_DEPTH {
        unsupported.insert(path.to_owned());
        return Vec::new();
    }
    let values = value
        .as_array()
        .map_or_else(|| std::slice::from_ref(value), Vec::as_slice);
    let mut effects = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let Some(object) = value.as_object() else {
            unsupported.insert(item_path);
            continue;
        };
        if let Some(condition) = object.get("if") {
            if object
                .keys()
                .any(|field| !matches!(field.as_str(), "if" | "then" | "else"))
                || !object.contains_key("then")
            {
                unsupported.insert(item_path);
                continue;
            }
            let Some(condition) = parse_condition(
                condition,
                &format!("{item_path}.if"),
                depth + 1,
                unsupported,
            ) else {
                continue;
            };
            effects.push(EocEffectDefinition::Conditional {
                condition,
                then_effects: parse_effects(
                    &object["then"],
                    &format!("{item_path}.then"),
                    depth + 1,
                    unsupported,
                ),
                else_effects: object.get("else").map_or_else(Vec::new, |value| {
                    parse_effects(value, &format!("{item_path}.else"), depth + 1, unsupported)
                }),
            });
            continue;
        }
        if let Some(message) = object.get("u_message") {
            let Some(text) = translated_text(message) else {
                unsupported.insert(format!("{item_path}.u_message"));
                continue;
            };
            if object.keys().any(|field| {
                !matches!(field.as_str(), "u_message" | "type")
                    || matches!(field.as_str(), "popup" | "snippet" | "sound")
            }) {
                unsupported.insert(item_path);
                continue;
            }
            effects.push(EocEffectDefinition::Message { text });
            continue;
        }
        if let Some(effect) = object.get("u_add_effect") {
            let Some(effect_id) = effect.as_str().filter(|id| valid_id(id)) else {
                unsupported.insert(format!("{item_path}.u_add_effect"));
                continue;
            };
            if object.keys().any(|field| {
                !matches!(
                    field.as_str(),
                    "u_add_effect" | "duration" | "target_part" | "bodypart" | "intensity"
                )
            }) {
                unsupported.insert(item_path);
                continue;
            }
            let Some((duration_turns, permanent)) =
                object.get("duration").and_then(parse_duration_turns)
            else {
                unsupported.insert(format!("{item_path}.duration"));
                continue;
            };
            let Some(intensity) = object
                .get("intensity")
                .map_or(Some(1), Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|value| *value > 0)
            else {
                unsupported.insert(format!("{item_path}.intensity"));
                continue;
            };
            let body_part_id = object
                .get("target_part")
                .or_else(|| object.get("bodypart"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if body_part_id
                .as_deref()
                .is_some_and(|id| !valid_id(id) || id == "RANDOM")
            {
                unsupported.insert(item_path);
                continue;
            }
            effects.push(EocEffectDefinition::AddEffect {
                effect_id: effect_id.to_owned(),
                body_part_id,
                duration_turns,
                permanent,
                intensity,
            });
            continue;
        }
        if let Some(effect) = object.get("u_lose_effect") {
            if object.keys().any(|field| {
                !matches!(field.as_str(), "u_lose_effect" | "target_part" | "bodypart")
            }) {
                unsupported.insert(item_path);
                continue;
            }
            let Some(effect_ids) = string_or_string_array(effect) else {
                unsupported.insert(format!("{item_path}.u_lose_effect"));
                continue;
            };
            let body_part_id = object
                .get("target_part")
                .or_else(|| object.get("bodypart"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if body_part_id.as_deref().is_some_and(|id| !valid_id(id)) {
                unsupported.insert(item_path);
                continue;
            }
            effects.push(EocEffectDefinition::RemoveEffects {
                effect_ids,
                body_part_id,
            });
            continue;
        }
        if let Some(variable) = object.get("u_add_var") {
            let has_value = object.contains_key("value");
            let has_possible_values = object.contains_key("possible_values");
            if has_value == has_possible_values
                || object.keys().any(|field| {
                    !matches!(field.as_str(), "u_add_var" | "value" | "possible_values")
                })
            {
                unsupported.insert(item_path);
                continue;
            }
            let Some(variable_id) = variable.as_str().filter(|id| valid_id(id)) else {
                unsupported.insert(format!("{item_path}.u_add_var"));
                continue;
            };
            let Some(possible_values) = object.get("value").map_or_else(
                || {
                    object.get("possible_values").and_then(|values| {
                        values
                            .as_array()
                            .filter(|values| (1..=MAX_EOC_STRING_VALUES).contains(&values.len()))?
                            .iter()
                            .map(|value| {
                                value
                                    .as_str()
                                    .filter(|value| valid_variable_value(value))
                                    .map(str::to_owned)
                            })
                            .collect::<Option<Vec<_>>>()
                    })
                },
                |value| {
                    value
                        .as_str()
                        .filter(|value| valid_variable_value(value))
                        .map(|value| vec![value.to_owned()])
                },
            ) else {
                unsupported.insert(format!("{item_path}.value"));
                continue;
            };
            effects.push(EocEffectDefinition::SetActorVariable {
                variable_id: variable_id.to_owned(),
                possible_values,
            });
            continue;
        }
        if let Some(variable) = object.get("u_lose_var") {
            if object.len() != 1 {
                unsupported.insert(item_path);
                continue;
            }
            let Some(variable_id) = variable.as_str().filter(|id| valid_id(id)) else {
                unsupported.insert(format!("{item_path}.u_lose_var"));
                continue;
            };
            effects.push(EocEffectDefinition::RemoveActorVariable {
                variable_id: variable_id.to_owned(),
            });
            continue;
        }
        if let Some(run) = object.get("run_eocs") {
            if object
                .keys()
                .any(|field| !matches!(field.as_str(), "run_eocs" | "time_in_future"))
            {
                unsupported.insert(item_path);
                continue;
            }
            let Some(eoc_ids) = string_or_string_array(run) else {
                unsupported.insert(format!("{item_path}.run_eocs"));
                continue;
            };
            let delay = match object.get("time_in_future") {
                Some(value) => match parse_delay(value) {
                    Some(delay) => Some(delay),
                    None => {
                        unsupported.insert(format!("{item_path}.time_in_future"));
                        continue;
                    }
                },
                None => None,
            };
            effects.push(EocEffectDefinition::RunEocs { eoc_ids, delay });
            continue;
        }
        unsupported.insert(item_path);
    }
    effects
}

fn translated_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn parse_string_value(value: &Value) -> Option<EocStringValueDefinition> {
    match value {
        Value::String(value) if valid_variable_value(value) => {
            Some(EocStringValueDefinition::Literal(value.clone()))
        }
        Value::Object(object) if object.len() == 1 => object
            .get("u_val")
            .and_then(Value::as_str)
            .filter(|id| valid_id(id))
            .map(|id| EocStringValueDefinition::ActorVariable(id.to_owned())),
        _ => None,
    }
}

fn string_or_string_array(value: &Value) -> Option<Vec<String>> {
    let values = value
        .as_array()
        .map_or_else(|| std::slice::from_ref(value), Vec::as_slice);
    let parsed = values
        .iter()
        .map(|value| value.as_str().filter(|id| valid_id(id)).map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    (!parsed.is_empty()).then_some(parsed)
}

fn parse_duration_turns(value: &Value) -> Option<(u32, bool)> {
    if let Some(turns) = value.as_u64() {
        return u32::try_from(turns)
            .ok()
            .filter(|turns| *turns > 0)
            .map(|turns| (turns, false));
    }
    let text = value.as_str()?.trim();
    if text.eq_ignore_ascii_case("PERMANENT") {
        return Some((0, true));
    }
    let split = text
        .char_indices()
        .find(|(_index, character)| {
            !character.is_ascii_digit() && *character != '.' && !character.is_whitespace()
        })
        .map_or(text.len(), |(index, _character)| index);
    let (number, unit) = text.split_at(split);
    let number = number.trim().parse::<f64>().ok()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
        "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
        "h" | "hour" | "hours" => 3_600.0,
        "d" | "day" | "days" => 86_400.0,
        _ => return None,
    };
    let turns = (number * multiplier).round();
    (turns > 0.0 && turns <= f64::from(u32::MAX)).then(|| (turns as u32, false))
}

fn parse_u32_literal(value: &Value) -> Option<u32> {
    value.as_u64().and_then(|value| u32::try_from(value).ok())
}

fn parse_delay(value: &Value) -> Option<EocDelayDefinition> {
    let values = match value {
        Value::Array(values) if values.len() == 2 => values.as_slice(),
        Value::Array(_) => return None,
        value => std::slice::from_ref(value),
    };
    let minimum_turns = parse_duration_turns(values.first()?)
        .filter(|(_turns, permanent)| !permanent)?
        .0;
    let maximum_turns = parse_duration_turns(values.last()?)
        .filter(|(_turns, permanent)| !permanent)?
        .0;
    (maximum_turns >= minimum_turns).then_some(EocDelayDefinition {
        minimum_turns,
        maximum_turns,
    })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}

fn valid_variable_value(value: &str) -> bool {
    value.len() <= MAX_EOC_VARIABLE_VALUE_BYTES && !value.chars().any(char::is_control)
}

fn invalid(source: &str, field: &str) -> EffectOnConditionRegistryError {
    EffectOnConditionRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum EffectOnConditionRegistryError {
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Catalog(ModCatalogError),
    InvalidField { source: String, field: String },
    UnresolvedInheritance(Vec<String>),
    InternalQueue,
}

impl fmt::Display for EffectOnConditionRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, error) => {
                write!(formatter, "failed to read EOC content {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "failed to parse EOC content {path}: {error}")
            }
            Self::Catalog(error) => write!(formatter, "failed to select EOC content: {error}"),
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid EOC field {field} in {source}")
            }
            Self::UnresolvedInheritance(ids) => {
                write!(formatter, "unresolved EOC inheritance: {}", ids.join(", "))
            }
            Self::InternalQueue => formatter.write_str("internal EOC loader queue failure"),
        }
    }
}

impl std::error::Error for EffectOnConditionRegistryError {}
