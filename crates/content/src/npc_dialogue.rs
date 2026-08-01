use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::eoc::{
    EocConditionDefinition, EocEffectDefinition, parse_inline_condition, parse_inline_effect_list,
};
use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const NPC_FIELDS: &[&str] = &[
    "type",
    "id",
    "name_unique",
    "name_suffix",
    "gender",
    "faction",
    "class",
    "attitude",
    "mission",
    "mission_offered",
    "chat",
    "age",
    "height",
    "str",
    "dex",
    "int",
    "per",
    "personality",
];
const TOPIC_FIELDS: &[&str] = &[
    "type",
    "id",
    "dynamic_line",
    "responses",
    "replace_built_in_responses",
];
const RESPONSE_FIELDS: &[&str] = &["text", "topic", "opinion", "effect", "condition"];
const OPINION_FIELDS: &[&str] = &["trust", "fear", "value", "anger", "owed"];

pub(crate) fn npc_field_is_implemented(field: &str) -> bool {
    NPC_FIELDS.contains(&field)
}

pub(crate) fn topic_field_is_implemented(field: &str) -> bool {
    TOPIC_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DialogueOpinionDefinition {
    pub trust: i32,
    pub fear: i32,
    pub value: i32,
    pub anger: i32,
    pub owed: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueResponseDefinition {
    pub text: String,
    pub next_topic_id: String,
    pub opinion: DialogueOpinionDefinition,
    pub effects: Vec<EocEffectDefinition>,
    pub condition: Option<EocConditionDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueTopicDefinition {
    pub id: String,
    pub dynamic_line: String,
    pub responses: Vec<DialogueResponseDefinition>,
    pub replace_built_in_responses: bool,
    pub unsupported: bool,
    pub sources: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpcTemplateDefinition {
    pub id: String,
    pub name_unique: Option<String>,
    pub name_suffix: Option<String>,
    pub gender: Option<String>,
    pub faction_id: String,
    pub class_id: String,
    pub attitude: i32,
    pub mission: String,
    pub mission_offered: Vec<String>,
    pub chat_topic_id: String,
    pub age_years: Option<u16>,
    pub height_centimeters: Option<u16>,
    pub base_strength: Option<u16>,
    pub base_dexterity: Option<u16>,
    pub base_intelligence: Option<u16>,
    pub base_perception: Option<u16>,
    pub personality: Option<NpcPersonalityDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpcPersonalityDefinition {
    pub aggression: i8,
    pub bravery: i8,
    pub collector: i8,
    pub altruism: i8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DialogueRegistry {
    npcs: BTreeMap<String, NpcTemplateDefinition>,
    topics: BTreeMap<String, DialogueTopicDefinition>,
}

impl DialogueRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, DialogueRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(DialogueRegistryError::Catalog)?;
        let mut registry = Self::default();
        for file in files {
            load_file(content_root.as_ref(), &file, &mut registry)?;
        }
        Ok(registry)
    }

    pub fn npc_iter(&self) -> impl ExactSizeIterator<Item = (&str, &NpcTemplateDefinition)> {
        self.npcs
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    pub fn topic_iter(&self) -> impl ExactSizeIterator<Item = (&str, &DialogueTopicDefinition)> {
        self.topics
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }
}

fn load_file(
    root: &Path,
    file: &SelectedContentFile,
    registry: &mut DialogueRegistry,
) -> Result<(), DialogueRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| DialogueRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| DialogueRegistryError::Json(file.destination.clone(), error))?;
    match value {
        Value::Array(values) => {
            for value in values {
                load_value(file, value, registry)?;
            }
        }
        value => load_value(file, value, registry)?,
    }
    Ok(())
}

fn load_value(
    file: &SelectedContentFile,
    value: Value,
    registry: &mut DialogueRegistry,
) -> Result<(), DialogueRegistryError> {
    match value.get("type").and_then(Value::as_str) {
        Some("npc") => load_npc(file, value, registry),
        Some("talk_topic") => load_topic(file, value, registry),
        _ => Ok(()),
    }
}

fn load_npc(
    file: &SelectedContentFile,
    value: Value,
    registry: &mut DialogueRegistry,
) -> Result<(), DialogueRegistryError> {
    let object = object(&value, file, "npc")?;
    let id = required_string(object.get("id"), file, "id")?;
    if registry.npcs.contains_key(&id) {
        return Ok(());
    }
    let name_unique = object
        .get("name_unique")
        .map(|value| translated_string(value, file, "name_unique"))
        .transpose()?;
    let name_suffix = object
        .get("name_suffix")
        .map(|value| translated_string(value, file, "name_suffix"))
        .transpose()?;
    let definition = NpcTemplateDefinition {
        id: id.clone(),
        name_unique,
        name_suffix,
        gender: optional_string(object.get("gender"), file, "gender")?,
        faction_id: optional_string(object.get("faction"), file, "faction")?.unwrap_or_default(),
        class_id: optional_string(object.get("class"), file, "class")?.unwrap_or_default(),
        attitude: integer(
            object
                .get("attitude")
                .ok_or_else(|| invalid(file, "attitude"))?,
            file,
            "attitude",
        )?,
        mission: optional_string(object.get("mission"), file, "mission")?.unwrap_or_default(),
        mission_offered: object
            .get("mission_offered")
            .map(|value| string_or_array(Some(value), file, "mission_offered"))
            .transpose()?
            .unwrap_or_default(),
        chat_topic_id: required_string(object.get("chat"), file, "chat")?,
        age_years: optional_u16(object.get("age"), file, "age")?,
        height_centimeters: optional_u16(object.get("height"), file, "height")?,
        base_strength: optional_u16(object.get("str"), file, "str")?,
        base_dexterity: optional_u16(object.get("dex"), file, "dex")?,
        base_intelligence: optional_u16(object.get("int"), file, "int")?,
        base_perception: optional_u16(object.get("per"), file, "per")?,
        personality: object
            .get("personality")
            .map(|value| npc_personality(value, file))
            .transpose()?,
        unsupported_fields: unsupported_fields(object, NPC_FIELDS),
        source: file.upstream_path.clone(),
    };
    registry.npcs.entry(id).or_insert(definition);
    Ok(())
}

fn load_topic(
    file: &SelectedContentFile,
    value: Value,
    registry: &mut DialogueRegistry,
) -> Result<(), DialogueRegistryError> {
    let object = object(&value, file, "talk_topic")?;
    let ids = string_or_array(object.get("id"), file, "id")?;
    let mut unsupported = !unsupported_fields(object, TOPIC_FIELDS).is_empty();
    unsupported |= object
        .get("dynamic_line")
        .is_some_and(|value| !dynamic_line_shape_is_supported(value));
    unsupported |= object
        .get("responses")
        .is_some_and(|value| !response_shapes_are_supported(value));
    unsupported |= object
        .get("replace_built_in_responses")
        .is_some_and(|value| !value.is_boolean());
    let line = (!unsupported)
        .then(|| {
            object
                .get("dynamic_line")
                .map(|value| dynamic_line(value, file))
                .transpose()
        })
        .transpose()?
        .flatten();
    let responses = (!unsupported)
        .then(|| {
            object
                .get("responses")
                .map(|value| responses(value, file))
                .transpose()
        })
        .transpose()?
        .flatten();
    for id in ids {
        let topic = registry
            .topics
            .entry(id.clone())
            .or_insert_with(|| DialogueTopicDefinition {
                id,
                dynamic_line: String::new(),
                responses: Vec::new(),
                replace_built_in_responses: false,
                unsupported: false,
                sources: BTreeSet::new(),
            });
        topic.unsupported |= unsupported;
        if let Some(replace) = object.get("replace_built_in_responses") {
            topic.replace_built_in_responses = replace
                .as_bool()
                .ok_or_else(|| invalid(file, "replace_built_in_responses"))?;
        }
        topic.sources.insert(file.upstream_path.clone());
        if let Some(line) = &line {
            topic.dynamic_line.clone_from(line);
        }
        if let Some(responses) = &responses {
            topic.responses.extend(responses.iter().cloned());
        }
    }
    Ok(())
}

fn dynamic_line_shape_is_supported(value: &Value) -> bool {
    match value {
        Value::String(value) => dialogue_line_is_supported(value),
        Value::Object(values) => {
            (translated_string_shape_is_supported(value)
                && translated_string_value(value).is_some_and(dialogue_line_is_supported))
                || (values
                    .keys()
                    .all(|key| key == "gendered_line" || key == "relevant_genders")
                    && values
                        .get("gendered_line")
                        .is_some_and(|line| line.as_str().is_some_and(dialogue_line_is_supported))
                    && values
                        .get("relevant_genders")
                        .and_then(Value::as_array)
                        .is_some_and(|entries| {
                            !entries.is_empty()
                                && entries
                                    .iter()
                                    .all(|entry| matches!(entry.as_str(), Some("npc" | "u")))
                        }))
        }
        _ => false,
    }
}

fn response_shapes_are_supported(value: &Value) -> bool {
    value.as_array().is_some_and(|responses| {
        !responses.is_empty()
            && responses.iter().all(|response| {
                response.as_object().is_some_and(|response| {
                    unsupported_fields(response, RESPONSE_FIELDS).is_empty()
                        && response
                            .get("text")
                            .is_some_and(translated_string_shape_is_supported)
                        && response.get("topic").is_none_or(|topic| {
                            topic.as_str().is_some_and(|topic| !topic.is_empty())
                        })
                        && response.get("opinion").is_none_or(|opinion| {
                            opinion.as_object().is_some_and(|opinion| {
                                unsupported_fields(opinion, OPINION_FIELDS).is_empty()
                                    && opinion.values().all(|value| {
                                        value
                                            .as_i64()
                                            .and_then(|value| i32::try_from(value).ok())
                                            .is_some()
                                    })
                            })
                        })
                        && response.get("effect").is_none_or(|effect| {
                            parse_inline_effect_list(effect, "effect").is_some()
                        })
                        && response.get("condition").is_none_or(|condition| {
                            parse_inline_condition(condition, "condition").is_some()
                        })
                })
            })
    })
}

fn dynamic_line(
    value: &Value,
    file: &SelectedContentFile,
) -> Result<String, DialogueRegistryError> {
    if let Ok(line) = translated_string(value, file, "dynamic_line") {
        return Ok(line);
    }
    let values = value
        .as_object()
        .ok_or_else(|| invalid(file, "dynamic_line"))?;
    if values
        .keys()
        .any(|key| key != "gendered_line" && key != "relevant_genders")
        || !values
            .get("relevant_genders")
            .is_some_and(|value| string_or_array(Some(value), file, "relevant_genders").is_ok())
    {
        return Err(invalid(file, "dynamic_line"));
    }
    required_string(values.get("gendered_line"), file, "gendered_line")
}

fn responses(
    value: &Value,
    file: &SelectedContentFile,
) -> Result<Vec<DialogueResponseDefinition>, DialogueRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid(file, "responses"))?
        .iter()
        .map(|value| {
            let response = value.as_object().ok_or_else(|| invalid(file, "response"))?;
            if !unsupported_fields(response, RESPONSE_FIELDS).is_empty() {
                return Err(invalid(file, "response"));
            }
            Ok(DialogueResponseDefinition {
                text: translated_string(
                    response.get("text").ok_or_else(|| invalid(file, "text"))?,
                    file,
                    "text",
                )?,
                next_topic_id: optional_string(response.get("topic"), file, "topic")?
                    .unwrap_or_else(|| String::from("TALK_NONE")),
                opinion: response
                    .get("opinion")
                    .map(|value| opinion(value, file))
                    .transpose()?
                    .unwrap_or_default(),
                effects: response
                    .get("effect")
                    .map(|effect| {
                        parse_inline_effect_list(effect, "effect")
                            .ok_or_else(|| invalid(file, "effect"))
                    })
                    .transpose()?
                    .unwrap_or_default(),
                condition: response
                    .get("condition")
                    .map(|condition| {
                        parse_inline_condition(condition, "condition")
                            .ok_or_else(|| invalid(file, "condition"))
                    })
                    .transpose()?,
            })
        })
        .collect()
}

fn opinion(
    value: &Value,
    file: &SelectedContentFile,
) -> Result<DialogueOpinionDefinition, DialogueRegistryError> {
    let values = value.as_object().ok_or_else(|| invalid(file, "opinion"))?;
    if !unsupported_fields(values, OPINION_FIELDS).is_empty() {
        return Err(invalid(file, "opinion"));
    }
    let get = |field| {
        values
            .get(field)
            .map(|value| integer(value, file, field))
            .transpose()
            .map(|value| value.unwrap_or_default())
    };
    Ok(DialogueOpinionDefinition {
        trust: get("trust")?,
        fear: get("fear")?,
        value: get("value")?,
        anger: get("anger")?,
        owed: get("owed")?,
    })
}

fn object<'a>(
    value: &'a Value,
    file: &SelectedContentFile,
    field: &str,
) -> Result<&'a Map<String, Value>, DialogueRegistryError> {
    value.as_object().ok_or_else(|| invalid(file, field))
}

fn translated_string(
    value: &Value,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, DialogueRegistryError> {
    if !translated_string_shape_is_supported(value) {
        return Err(invalid(file, field));
    }
    match value {
        Value::String(value) if !value.is_empty() => Ok(value.clone()),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| invalid(file, field)),
        _ => Err(invalid(file, field)),
    }
}

fn translated_string_shape_is_supported(value: &Value) -> bool {
    match value {
        Value::String(value) => !value.is_empty(),
        Value::Object(values) => {
            values.keys().all(|key| {
                key.starts_with("//") || ["str", "str_sp", "str_pl", "ctxt"].contains(&key.as_str())
            }) && values
                .get("ctxt")
                .is_none_or(|context| context.as_str().is_some_and(|context| !context.is_empty()))
                && ["str", "str_sp", "str_pl"].into_iter().any(|key| {
                    values
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|text| !text.is_empty())
                })
        }
        _ => false,
    }
}

fn translated_string_value(value: &Value) -> Option<&str> {
    match value {
        Value::String(value) => Some(value),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str)),
        _ => None,
    }
}

fn dialogue_line_is_supported(line: &str) -> bool {
    !line.is_empty() && !matches!(line, "*" | "&")
}

fn required_string(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, DialogueRegistryError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(file, field))
}

fn optional_string(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<Option<String>, DialogueRegistryError> {
    value
        .map(|value| required_string(Some(value), file, field))
        .transpose()
}

fn string_or_array(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<Vec<String>, DialogueRegistryError> {
    match value {
        Some(Value::String(value)) if !value.is_empty() => Ok(vec![value.clone()]),
        Some(Value::Array(values)) if !values.is_empty() => values
            .iter()
            .map(|value| required_string(Some(value), file, field))
            .collect(),
        _ => Err(invalid(file, field)),
    }
}

fn integer(
    value: &Value,
    file: &SelectedContentFile,
    field: &str,
) -> Result<i32, DialogueRegistryError> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid(file, field))
}

fn optional_u16(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<Option<u16>, DialogueRegistryError> {
    value
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u16::try_from(value).ok())
                .filter(|value| *value > 0)
                .ok_or_else(|| invalid(file, field))
        })
        .transpose()
}

fn npc_personality(
    value: &Value,
    file: &SelectedContentFile,
) -> Result<NpcPersonalityDefinition, DialogueRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(file, "personality"))?;
    if object
        .keys()
        .any(|field| !["aggression", "bravery", "collector", "altruism"].contains(&field.as_str()))
    {
        return Err(invalid(file, "personality"));
    }
    let field = |name| -> Result<i8, DialogueRegistryError> {
        object
            .get(name)
            .and_then(Value::as_i64)
            .and_then(|value| i8::try_from(value).ok())
            .ok_or_else(|| invalid(file, name))
    };
    Ok(NpcPersonalityDefinition {
        aggression: field("aggression")?,
        bravery: field("bravery")?,
        collector: field("collector")?,
        altruism: field("altruism")?,
    })
}

fn unsupported_fields(object: &Map<String, Value>, supported: &[&str]) -> BTreeSet<String> {
    object
        .keys()
        .filter(|field| !field.starts_with("//") && !supported.contains(&field.as_str()))
        .cloned()
        .collect()
}

fn invalid(file: &SelectedContentFile, field: &str) -> DialogueRegistryError {
    DialogueRegistryError::InvalidField {
        source: file.upstream_path.clone(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum DialogueRegistryError {
    Catalog(ModCatalogError),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
}

impl fmt::Display for DialogueRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "NPC/dialogue mod selection failed: {error}"),
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid NPC/dialogue field {field} in {source}")
            }
            Self::Io(path, error) => {
                write!(formatter, "NPC/dialogue I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "NPC/dialogue JSON failed for {path}: {error}")
            }
        }
    }
}

impl std::error::Error for DialogueRegistryError {}
