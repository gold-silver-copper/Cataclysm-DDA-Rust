//! Strict pinned mission definitions for the authoritative mission lifecycle.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::eoc::{EocEffectDefinition, parse_inline_effect_list};
use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const MISSION_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "name",
    "description",
    "difficulty",
    "value",
    "origins",
    "dialogue",
    "urgent",
    "item",
    "item_group",
    "count",
    "required_container",
    "remove_container",
    "empty_container",
    "has_generic_rewards",
    "goal",
    "place",
    "start",
    "end",
    "fail",
    "deadline",
    "followup",
    "monster_species",
    "monster_type",
    "monster_kill_goal",
    "destination",
    "goal_condition",
    "invisible_on_complete",
];

const MISSION_DIALOGUE_FIELDS: &[&str] = &[
    "describe",
    "offer",
    "accepted",
    "rejected",
    "advice",
    "inquire",
    "success",
    "success_lie",
    "failure",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    MISSION_FIELDS.contains(&field)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionGoalDefinition {
    Null,
    GoTo,
    GoToType,
    FindItem,
    FindAnyItem,
    FindItemGroup,
    FindMonster,
    FindNpc,
    Assassinate,
    KillMonster,
    KillMonsters,
    KillMonsterType,
    KillNemesis,
    RecruitNpc,
    RecruitNpcClass,
    ComputerToggle,
    KillMonsterSpecies,
    TalkToNpc,
    Condition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub difficulty: i32,
    pub value: i32,
    pub goal: MissionGoalDefinition,
    pub item_type_id: String,
    pub item_count: i32,
    pub monster_type_id: String,
    pub monster_species_id: String,
    pub monster_kill_goal: i32,
    pub origins: Vec<String>,
    pub dialogue: BTreeMap<String, String>,
    pub has_generic_rewards: bool,
    pub start_effects: Vec<EocEffectDefinition>,
    pub end_effects: Vec<EocEffectDefinition>,
    pub fail_effects: Vec<EocEffectDefinition>,
    /// Present only for the legacy NPC offer/dialogue path. Dynamic EOC
    /// assignment does not consult these fields, and the server rejects NPC
    /// templates that would require that unported path.
    pub has_legacy_offer_metadata: bool,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

impl MissionDefinition {
    #[must_use]
    pub fn is_fully_supported(&self) -> bool {
        self.unsupported_fields.is_empty()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MissionRegistry {
    definitions: BTreeMap<String, MissionDefinition>,
}

impl MissionRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, MissionRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(MissionRegistryError::Catalog)?;
        let mut pending = read_missions(content_root.as_ref(), files)?;
        let mut definitions = BTreeMap::new();
        let mut abstract_ids = BTreeSet::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(MissionRegistryError::InternalQueue)?;
                if load_one(&raw, &mut definitions, &mut abstract_ids)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(MissionRegistryError::UnresolvedInheritance(
                    pending.iter().take(20).map(|raw| raw.id.clone()).collect(),
                ));
            }
        }
        definitions.retain(|id, _definition| !abstract_ids.contains(id));
        Ok(Self { definitions })
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &MissionDefinition)> {
        self.definitions
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MissionDefinition> {
        self.definitions.get(id)
    }
}

#[derive(Clone)]
struct RawMission {
    file: SelectedContentFile,
    object: Map<String, Value>,
    id: String,
    is_abstract: bool,
}

fn read_missions(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawMission>, MissionRegistryError> {
    let mut pending = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| MissionRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| MissionRegistryError::Json(file.destination.clone(), error))?;
        let values = value
            .as_array()
            .map_or_else(|| std::slice::from_ref(&value), Vec::as_slice);
        for value in values {
            if value.get("type").and_then(Value::as_str) != Some("mission_definition") {
                continue;
            }
            let object = value
                .as_object()
                .ok_or_else(|| invalid(&file, "mission_definition"))?
                .clone();
            let identifiers = match (object.get("id"), object.get("abstract")) {
                (Some(_), Some(_)) | (None, None) => {
                    return Err(invalid(&file, "id"));
                }
                (Some(value), None) => (definition_ids(value, &file)?, false),
                (None, Some(value)) => {
                    (vec![required_string(Some(value), &file, "abstract")?], true)
                }
            };
            for id in identifiers.0 {
                pending.push_back(RawMission {
                    file: file.clone(),
                    object: object.clone(),
                    id,
                    is_abstract: identifiers.1,
                });
            }
        }
    }
    Ok(pending)
}

fn load_one(
    raw: &RawMission,
    definitions: &mut BTreeMap<String, MissionDefinition>,
    abstract_ids: &mut BTreeSet<String>,
) -> Result<bool, MissionRegistryError> {
    let id = raw.id.clone();
    let parent = raw
        .object
        .get("copy-from")
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(&raw.file, "copy-from"))
        })
        .transpose()?;
    let inherits = parent.is_some();
    if !inherits
        && ["name", "difficulty", "value", "goal"]
            .into_iter()
            .any(|field| !raw.object.contains_key(field))
    {
        return Err(invalid(&raw.file, "mission_definition"));
    }
    let mut definition = if let Some(parent) = parent {
        let Some(parent) = definitions.get(&parent) else {
            return Ok(false);
        };
        let mut inherited = parent.clone();
        inherited.id = id.clone();
        inherited.source = raw.file.upstream_path.clone();
        inherited
    } else {
        MissionDefinition {
            id: id.clone(),
            name: String::new(),
            description: String::new(),
            difficulty: 0,
            value: 0,
            goal: MissionGoalDefinition::Null,
            item_type_id: String::new(),
            item_count: 1,
            monster_type_id: String::new(),
            monster_species_id: String::new(),
            monster_kill_goal: -1,
            origins: Vec::new(),
            dialogue: BTreeMap::new(),
            has_generic_rewards: true,
            start_effects: Vec::new(),
            end_effects: Vec::new(),
            fail_effects: Vec::new(),
            has_legacy_offer_metadata: false,
            unsupported_fields: BTreeSet::new(),
            source: raw.file.upstream_path.clone(),
        }
    };

    for field in raw.object.keys() {
        definition.unsupported_fields.remove(field);
        if !MISSION_FIELDS.contains(&field.as_str()) && !field.starts_with("//") {
            definition.unsupported_fields.insert(field.clone());
        }
    }
    if let Some(value) = raw.object.get("name") {
        definition.name = translated_string(value).ok_or_else(|| invalid(&raw.file, "name"))?;
    }
    if let Some(value) = raw.object.get("description") {
        definition.description =
            translated_string(value).ok_or_else(|| invalid(&raw.file, "description"))?;
    }
    if let Some(value) = raw.object.get("difficulty") {
        definition.difficulty = integer(value).ok_or_else(|| invalid(&raw.file, "difficulty"))?;
    }
    if let Some(value) = raw.object.get("value") {
        definition.value = integer(value).ok_or_else(|| invalid(&raw.file, "value"))?;
    }
    if let Some(value) = raw.object.get("goal") {
        definition.goal = parse_goal(value).ok_or_else(|| invalid(&raw.file, "goal"))?;
    }
    if let Some(value) = raw.object.get("item") {
        definition.item_type_id = string(value).ok_or_else(|| invalid(&raw.file, "item"))?;
    }
    if let Some(value) = raw.object.get("count") {
        definition.item_count = integer(value).ok_or_else(|| invalid(&raw.file, "count"))?;
    }
    if let Some(value) = raw.object.get("monster_type") {
        definition.monster_type_id =
            string(value).ok_or_else(|| invalid(&raw.file, "monster_type"))?;
    }
    if let Some(value) = raw.object.get("monster_species") {
        definition.monster_species_id =
            string(value).ok_or_else(|| invalid(&raw.file, "monster_species"))?;
    }
    if let Some(value) = raw.object.get("monster_kill_goal") {
        definition.monster_kill_goal =
            integer(value).ok_or_else(|| invalid(&raw.file, "monster_kill_goal"))?;
    }
    if let Some(value) = raw.object.get("origins") {
        definition.origins = parse_origins(value).ok_or_else(|| invalid(&raw.file, "origins"))?;
    }
    if let Some(value) = raw.object.get("dialogue") {
        let dialogue = parse_dialogue(value).ok_or_else(|| invalid(&raw.file, "dialogue"))?;
        definition.dialogue.extend(dialogue);
    }
    if let Some(value) = raw.object.get("has_generic_rewards") {
        definition.has_generic_rewards = value
            .as_bool()
            .ok_or_else(|| invalid(&raw.file, "has_generic_rewards"))?;
    }
    definition.start_effects = phase_effects(
        raw.object.get("start"),
        &definition.start_effects,
        "start",
        &mut definition.unsupported_fields,
    );
    definition.end_effects = phase_effects(
        raw.object.get("end"),
        &definition.end_effects,
        "end",
        &mut definition.unsupported_fields,
    );
    definition.fail_effects = phase_effects(
        raw.object.get("fail"),
        &definition.fail_effects,
        "fail",
        &mut definition.unsupported_fields,
    );
    definition.has_legacy_offer_metadata =
        !definition.origins.is_empty() || !definition.dialogue.is_empty();

    for unsupported in [
        "urgent",
        "item_group",
        "required_container",
        "remove_container",
        "empty_container",
        "place",
        "deadline",
        "followup",
        "destination",
        "goal_condition",
        "invisible_on_complete",
    ] {
        if raw.object.contains_key(unsupported) {
            definition.unsupported_fields.insert(unsupported.to_owned());
        }
    }
    let npc_origin = definition.origins.iter().any(|origin| {
        matches!(
            origin.as_str(),
            "ORIGIN_OPENER_NPC" | "ORIGIN_ANY_NPC" | "ORIGIN_SECONDARY"
        )
    });
    if definition.name.is_empty()
        || definition.item_count <= 0
        || (matches!(definition.goal, MissionGoalDefinition::FindItem)
            && definition.item_type_id.is_empty())
        || (matches!(definition.goal, MissionGoalDefinition::KillMonsterType)
            && (definition.monster_type_id.is_empty() || definition.monster_kill_goal <= 0))
        || (matches!(definition.goal, MissionGoalDefinition::KillMonsterSpecies)
            && (definition.monster_species_id.is_empty() || definition.monster_kill_goal <= 0))
        || (npc_origin
            && MISSION_DIALOGUE_FIELDS
                .iter()
                .any(|field| !definition.dialogue.contains_key(*field)))
    {
        return Err(invalid(&raw.file, "mission_definition"));
    }
    definitions.insert(id, definition);
    if raw.is_abstract {
        abstract_ids.insert(raw.id.clone());
    } else {
        abstract_ids.remove(&raw.id);
    }
    Ok(true)
}

fn phase_effects(
    value: Option<&Value>,
    inherited: &[EocEffectDefinition],
    path: &str,
    unsupported: &mut BTreeSet<String>,
) -> Vec<EocEffectDefinition> {
    let Some(value) = value else {
        return inherited.to_vec();
    };
    if value.as_str() == Some("standard") {
        return Vec::new();
    }
    let Some(object) = value.as_object() else {
        unsupported.insert(path.to_owned());
        return Vec::new();
    };
    if object.keys().any(|field| field != "effect") || !object.contains_key("effect") {
        unsupported.insert(path.to_owned());
        return Vec::new();
    }
    let Some(effects) = parse_inline_effect_list(&object["effect"], path) else {
        unsupported.insert(path.to_owned());
        return Vec::new();
    };
    effects
}

fn definition_ids(
    value: &Value,
    file: &SelectedContentFile,
) -> Result<Vec<String>, MissionRegistryError> {
    if let Some(id) = string(value) {
        return Ok(vec![id]);
    }
    let values = value.as_array().ok_or_else(|| invalid(file, "id"))?;
    if values.is_empty() {
        return Err(invalid(file, "id"));
    }
    let ids = values
        .iter()
        .map(|value| string(value).ok_or_else(|| invalid(file, "id")))
        .collect::<Result<Vec<_>, _>>()?;
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(invalid(file, "id"));
    }
    Ok(ids)
}

fn parse_origins(value: &Value) -> Option<Vec<String>> {
    let values = value.as_array()?;
    let origins = values
        .iter()
        .map(|value| {
            let origin = value.as_str()?;
            matches!(
                origin,
                "ORIGIN_NULL"
                    | "ORIGIN_GAME_START"
                    | "ORIGIN_OPENER_NPC"
                    | "ORIGIN_ANY_NPC"
                    | "ORIGIN_SECONDARY"
                    | "ORIGIN_COMPUTER"
            )
            .then(|| origin.to_owned())
        })
        .collect::<Option<Vec<_>>>()?;
    (origins.iter().collect::<BTreeSet<_>>().len() == origins.len()).then_some(origins)
}

fn parse_dialogue(value: &Value) -> Option<BTreeMap<String, String>> {
    let object = value.as_object()?;
    if object
        .keys()
        .any(|field| !MISSION_DIALOGUE_FIELDS.contains(&field.as_str()))
    {
        return None;
    }
    object
        .iter()
        .map(|(field, value)| Some((field.clone(), translated_string(value)?)))
        .collect()
}

fn parse_goal(value: &Value) -> Option<MissionGoalDefinition> {
    Some(match value.as_str()? {
        "MGOAL_NULL" => MissionGoalDefinition::Null,
        "MGOAL_GO_TO" => MissionGoalDefinition::GoTo,
        "MGOAL_GO_TO_TYPE" => MissionGoalDefinition::GoToType,
        "MGOAL_FIND_ITEM" => MissionGoalDefinition::FindItem,
        "MGOAL_FIND_ANY_ITEM" => MissionGoalDefinition::FindAnyItem,
        "MGOAL_FIND_ITEM_GROUP" => MissionGoalDefinition::FindItemGroup,
        "MGOAL_FIND_MONSTER" => MissionGoalDefinition::FindMonster,
        "MGOAL_FIND_NPC" => MissionGoalDefinition::FindNpc,
        "MGOAL_ASSASSINATE" => MissionGoalDefinition::Assassinate,
        "MGOAL_KILL_MONSTER" => MissionGoalDefinition::KillMonster,
        "MGOAL_KILL_MONSTERS" => MissionGoalDefinition::KillMonsters,
        "MGOAL_KILL_MONSTER_TYPE" => MissionGoalDefinition::KillMonsterType,
        "MGOAL_KILL_NEMESIS" => MissionGoalDefinition::KillNemesis,
        "MGOAL_RECRUIT_NPC" => MissionGoalDefinition::RecruitNpc,
        "MGOAL_RECRUIT_NPC_CLASS" => MissionGoalDefinition::RecruitNpcClass,
        "MGOAL_COMPUTER_TOGGLE" => MissionGoalDefinition::ComputerToggle,
        "MGOAL_KILL_MONSTER_SPEC" => MissionGoalDefinition::KillMonsterSpecies,
        "MGOAL_TALK_TO_NPC" => MissionGoalDefinition::TalkToNpc,
        "MGOAL_CONDITION" => MissionGoalDefinition::Condition,
        _ => return None,
    })
}

fn translated_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Object(object) => object
            .get("str")
            .or_else(|| object.get("str_sp"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        _ => None,
    }
}

fn required_string(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, MissionRegistryError> {
    value.and_then(string).ok_or_else(|| invalid(file, field))
}

fn string(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned)
}

fn integer(value: &Value) -> Option<i32> {
    i32::try_from(value.as_i64()?).ok()
}

fn invalid(file: &SelectedContentFile, field: &str) -> MissionRegistryError {
    MissionRegistryError::InvalidDefinition {
        path: file.upstream_path.clone(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum MissionRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    InvalidDefinition { path: String, field: String },
    UnresolvedInheritance(Vec<String>),
    InternalQueue,
}

impl fmt::Display for MissionRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "{error}"),
            Self::Io(path, error) => write!(formatter, "failed reading {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "failed parsing {path}: {error}"),
            Self::InvalidDefinition { path, field } => {
                write!(formatter, "invalid mission definition {field} in {path}")
            }
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved mission inheritance: {}",
                    ids.join(", ")
                )
            }
            Self::InternalQueue => formatter.write_str("mission inheritance queue underflow"),
        }
    }
}

impl std::error::Error for MissionRegistryError {}
