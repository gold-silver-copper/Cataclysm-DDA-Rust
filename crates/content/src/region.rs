use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{
    ContentManifest, FurnitureRegistry, ModCatalog, ModCatalogError, SelectedContentFile,
    TerrainRegistry,
};

pub const DEFAULT_REGION_TERRAIN_FURNITURE_ID: &str = "default";
pub const MAX_DEFAULT_REGION_TABLES: usize = 256;
pub const MAX_REGION_SUBSTITUTION_DEFINITIONS: usize = 512;
pub const MAX_REGION_SUBSTITUTION_CHOICES: usize = 256;
pub const MAX_REGION_SUBSTITUTION_WEIGHT: u32 = 1_000_000;
pub const MAX_REGION_SUBSTITUTION_TOTAL_WEIGHT: u64 = 16_000_000;
pub const MAX_REGION_SUBSTITUTION_DEPTH: usize = 32;

const SETTINGS_FIELDS: &[&str] = &["type", "id", "ter_furn"];
const TERRAIN_FIELDS: &[&str] = &["type", "id", "ter_id", "replace_with_terrain"];
const FURNITURE_FIELDS: &[&str] = &["type", "id", "furn_id", "replace_with_furniture"];
const REGION_PSEUDO_FLAG: &str = "REGION_PSEUDO";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedRegionSubstitution {
    pub id: String,
    pub weight: u32,
}

/// One direct regional substitution roll. A selected choice may itself be a
/// pseudo ID with another table; consumers must then make a fresh roll from
/// that table, matching the upstream recursive resolution behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionSubstitutionTable {
    pub definition_id: String,
    pub pseudo_id: String,
    pub choices: Vec<WeightedRegionSubstitution>,
    pub source: String,
}

impl RegionSubstitutionTable {
    #[must_use]
    pub fn total_weight(&self) -> u64 {
        self.choices
            .iter()
            .map(|choice| u64::from(choice.weight))
            .sum()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DefaultRegionTerrainFurnitureRegistry {
    terrain: BTreeMap<String, RegionSubstitutionTable>,
    furniture: BTreeMap<String, RegionSubstitutionTable>,
}

impl DefaultRegionTerrainFurnitureRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
        terrain: &TerrainRegistry,
        furniture: &FurnitureRegistry,
    ) -> Result<Self, DefaultRegionTerrainFurnitureRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(DefaultRegionTerrainFurnitureRegistryError::Catalog)?;
        let (settings, definitions) = read_region_substitutions(content_root.as_ref(), files)?;
        compile_default_region(
            &settings,
            &definitions,
            &|id| {
                terrain
                    .get(id)
                    .map(|definition| definition.flags.contains(REGION_PSEUDO_FLAG))
            },
            &|id| {
                furniture
                    .get(id)
                    .map(|definition| definition.flags.contains(REGION_PSEUDO_FLAG))
            },
        )
    }

    #[must_use]
    pub fn terrain_len(&self) -> usize {
        self.terrain.len()
    }

    #[must_use]
    pub fn furniture_len(&self) -> usize {
        self.furniture.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty() && self.furniture.is_empty()
    }

    #[must_use]
    pub fn terrain_table(&self, pseudo_id: &str) -> Option<&RegionSubstitutionTable> {
        self.terrain.get(pseudo_id)
    }

    #[must_use]
    pub fn furniture_table(&self, pseudo_id: &str) -> Option<&RegionSubstitutionTable> {
        self.furniture.get(pseudo_id)
    }

    pub fn terrain_tables(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, &RegionSubstitutionTable)> {
        self.terrain.iter().map(|(id, table)| (id.as_str(), table))
    }

    pub fn furniture_tables(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, &RegionSubstitutionTable)> {
        self.furniture
            .iter()
            .map(|(id, table)| (id.as_str(), table))
    }
}

#[derive(Debug)]
pub enum DefaultRegionTerrainFurnitureRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Invalid(String),
}

impl fmt::Display for DefaultRegionTerrainFurnitureRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "selected regional catalog failed: {error}"),
            Self::Io(path, error) => {
                write!(formatter, "failed to read regional file {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "failed to parse regional JSON {path}: {error}")
            }
            Self::Invalid(reason) => write!(formatter, "invalid default regional tables: {reason}"),
        }
    }
}

impl std::error::Error for DefaultRegionTerrainFurnitureRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Io(_, error) => Some(error),
            Self::Json(_, error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

#[derive(Clone)]
struct RawRegionDefinition {
    source: String,
    object: Map<String, Value>,
}

type RegionDefinitionCatalog = BTreeMap<String, Vec<RawRegionDefinition>>;

fn read_region_substitutions(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<
    (Vec<RawRegionDefinition>, RegionDefinitionCatalog),
    DefaultRegionTerrainFurnitureRegistryError,
> {
    let mut settings = Vec::new();
    let mut definitions: RegionDefinitionCatalog = BTreeMap::new();
    let mut definition_count = 0_usize;
    for file in files {
        let bytes = fs::read(root.join(&file.destination)).map_err(|error| {
            DefaultRegionTerrainFurnitureRegistryError::Io(file.destination.clone(), error)
        })?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            DefaultRegionTerrainFurnitureRegistryError::Json(file.destination.clone(), error)
        })?;
        match value {
            Value::Array(values) => {
                for (index, value) in values.into_iter().enumerate() {
                    collect_region_definition(
                        &file,
                        index,
                        value,
                        &mut settings,
                        &mut definitions,
                        &mut definition_count,
                    )?;
                }
            }
            value => collect_region_definition(
                &file,
                0,
                value,
                &mut settings,
                &mut definitions,
                &mut definition_count,
            )?,
        }
    }
    Ok((settings, definitions))
}

fn collect_region_definition(
    file: &SelectedContentFile,
    index: usize,
    value: Value,
    settings: &mut Vec<RawRegionDefinition>,
    definitions: &mut RegionDefinitionCatalog,
    definition_count: &mut usize,
) -> Result<(), DefaultRegionTerrainFurnitureRegistryError> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let source = format!("{}#{index}", file.upstream_path);
    match object.get("type").and_then(Value::as_str) {
        Some("region_settings_terrain_furniture")
            if object.get("id").and_then(Value::as_str)
                == Some(DEFAULT_REGION_TERRAIN_FURNITURE_ID) =>
        {
            settings.push(RawRegionDefinition {
                source,
                object: object.clone(),
            });
        }
        Some("region_terrain_furniture") => {
            if *definition_count >= MAX_REGION_SUBSTITUTION_DEFINITIONS {
                return Err(DefaultRegionTerrainFurnitureRegistryError::Invalid(
                    format!(
                        "selected region_terrain_furniture count exceeds {MAX_REGION_SUBSTITUTION_DEFINITIONS}"
                    ),
                ));
            }
            *definition_count += 1;
            if let Some(id) = object
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
            {
                definitions
                    .entry(id.to_owned())
                    .or_default()
                    .push(RawRegionDefinition {
                        source,
                        object: object.clone(),
                    });
            }
        }
        _ => {}
    }
    Ok(())
}

fn compile_default_region<TerrainLookup, FurnitureLookup>(
    settings: &[RawRegionDefinition],
    definitions: &RegionDefinitionCatalog,
    terrain_lookup: &TerrainLookup,
    furniture_lookup: &FurnitureLookup,
) -> Result<DefaultRegionTerrainFurnitureRegistry, DefaultRegionTerrainFurnitureRegistryError>
where
    TerrainLookup: Fn(&str) -> Option<bool>,
    FurnitureLookup: Fn(&str) -> Option<bool>,
{
    if settings.len() != 1 {
        return invalid(format!(
            "expected exactly one selected {DEFAULT_REGION_TERRAIN_FURNITURE_ID:?} region_settings_terrain_furniture definition, found {}",
            settings.len()
        ));
    }
    let settings = &settings[0];
    reject_unknown_fields(&settings.object, SETTINGS_FIELDS, &settings.source)?;
    let table_ids = settings
        .object
        .get("ter_furn")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                "ter_furn in {} must be an array",
                settings.source
            ))
        })?;
    if table_ids.is_empty() || table_ids.len() > MAX_DEFAULT_REGION_TABLES {
        return invalid(format!(
            "default ter_furn must contain 1..={MAX_DEFAULT_REGION_TABLES} ids"
        ));
    }
    let mut unique_ids = BTreeSet::new();
    let mut terrain = BTreeMap::new();
    let mut furniture = BTreeMap::new();
    for value in table_ids {
        let definition_id = value.as_str().filter(|id| !id.is_empty()).ok_or_else(|| {
            DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                "ter_furn in {} must contain only non-empty ids",
                settings.source
            ))
        })?;
        if !unique_ids.insert(definition_id) {
            return invalid(format!(
                "default ter_furn contains duplicate id {definition_id:?}"
            ));
        }
        let candidates = definitions.get(definition_id).ok_or_else(|| {
            DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                "default ter_furn references missing definition {definition_id:?}"
            ))
        })?;
        if candidates.len() != 1 {
            return invalid(format!(
                "default ter_furn definition {definition_id:?} has {} selected definitions",
                candidates.len()
            ));
        }
        let raw = &candidates[0];
        match classify_definition(raw)? {
            RegionDefinitionKind::Terrain => {
                let table = parse_table(
                    raw,
                    definition_id,
                    "ter_id",
                    "replace_with_terrain",
                    TERRAIN_FIELDS,
                    terrain_lookup,
                )?;
                if terrain.insert(table.pseudo_id.clone(), table).is_some() {
                    return invalid(format!(
                        "default region has multiple terrain tables for one pseudo id in {definition_id:?}"
                    ));
                }
            }
            RegionDefinitionKind::Furniture => {
                let table = parse_table(
                    raw,
                    definition_id,
                    "furn_id",
                    "replace_with_furniture",
                    FURNITURE_FIELDS,
                    furniture_lookup,
                )?;
                if furniture.insert(table.pseudo_id.clone(), table).is_some() {
                    return invalid(format!(
                        "default region has multiple furniture tables for one pseudo id in {definition_id:?}"
                    ));
                }
            }
        }
    }
    validate_graph("terrain", &terrain, terrain_lookup)?;
    validate_graph("furniture", &furniture, furniture_lookup)?;
    Ok(DefaultRegionTerrainFurnitureRegistry { terrain, furniture })
}

#[derive(Clone, Copy)]
enum RegionDefinitionKind {
    Terrain,
    Furniture,
}

fn classify_definition(
    raw: &RawRegionDefinition,
) -> Result<RegionDefinitionKind, DefaultRegionTerrainFurnitureRegistryError> {
    match (
        raw.object.contains_key("ter_id") || raw.object.contains_key("replace_with_terrain"),
        raw.object.contains_key("furn_id") || raw.object.contains_key("replace_with_furniture"),
    ) {
        (true, false) => Ok(RegionDefinitionKind::Terrain),
        (false, true) => Ok(RegionDefinitionKind::Furniture),
        _ => invalid(format!(
            "{} must define exactly one complete terrain or furniture substitution",
            raw.source
        )),
    }
}

fn parse_table<Lookup>(
    raw: &RawRegionDefinition,
    definition_id: &str,
    pseudo_field: &str,
    choices_field: &str,
    allowed: &[&str],
    lookup: &Lookup,
) -> Result<RegionSubstitutionTable, DefaultRegionTerrainFurnitureRegistryError>
where
    Lookup: Fn(&str) -> Option<bool>,
{
    reject_unknown_fields(&raw.object, allowed, &raw.source)?;
    let pseudo_id = raw
        .object
        .get(pseudo_field)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                "{pseudo_field} in {} must be a non-empty id",
                raw.source
            ))
        })?;
    match lookup(pseudo_id) {
        None => {
            return invalid(format!(
                "{pseudo_field} in {} references missing id {pseudo_id:?}",
                raw.source
            ));
        }
        Some(false) => {
            return invalid(format!(
                "{pseudo_field} in {} references non-REGION_PSEUDO id {pseudo_id:?}",
                raw.source
            ));
        }
        Some(true) => {}
    }
    let choices = parse_weighted_choices(
        raw.object.get(choices_field),
        &format!("{choices_field} in {}", raw.source),
    )?;
    Ok(RegionSubstitutionTable {
        definition_id: definition_id.to_owned(),
        pseudo_id: pseudo_id.to_owned(),
        choices,
        source: raw.source.clone(),
    })
}

fn parse_weighted_choices(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<WeightedRegionSubstitution>, DefaultRegionTerrainFurnitureRegistryError> {
    let values = value.and_then(Value::as_array).ok_or_else(|| {
        DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
            "{context} must be a weighted array"
        ))
    })?;
    if values.is_empty() || values.len() > MAX_REGION_SUBSTITUTION_CHOICES {
        return invalid(format!(
            "{context} must contain 1..={MAX_REGION_SUBSTITUTION_CHOICES} choices"
        ));
    }
    let mut total = 0_u64;
    let mut choices = Vec::with_capacity(values.len());
    for value in values {
        let (id, weight) = if let Some(id) = value.as_str() {
            (id, 1_u32)
        } else {
            let pair = value
                .as_array()
                .filter(|pair| pair.len() == 2)
                .ok_or_else(|| {
                    DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                        "{context} entries must be ids or [id, positive integer weight]"
                    ))
                })?;
            let id = pair[0]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                        "{context} contains an invalid id"
                    ))
                })?;
            let raw_weight = pair[1].as_u64().ok_or_else(|| {
                DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                    "{context} weights must be positive integers"
                ))
            })?;
            let weight = u32::try_from(raw_weight).map_err(|_| {
                DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                    "{context} weight exceeds {MAX_REGION_SUBSTITUTION_WEIGHT}"
                ))
            })?;
            (id, weight)
        };
        if id.is_empty() || weight == 0 || weight > MAX_REGION_SUBSTITUTION_WEIGHT {
            return invalid(format!(
                "{context} contains an empty id or weight outside 1..={MAX_REGION_SUBSTITUTION_WEIGHT}"
            ));
        }
        total = total.checked_add(u64::from(weight)).ok_or_else(|| {
            DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
                "{context} total weight overflow"
            ))
        })?;
        if total > MAX_REGION_SUBSTITUTION_TOTAL_WEIGHT {
            return invalid(format!(
                "{context} total weight exceeds {MAX_REGION_SUBSTITUTION_TOTAL_WEIGHT}"
            ));
        }
        choices.push(WeightedRegionSubstitution {
            id: id.to_owned(),
            weight,
        });
    }
    Ok(choices)
}

fn validate_graph<Lookup>(
    kind: &str,
    tables: &BTreeMap<String, RegionSubstitutionTable>,
    lookup: &Lookup,
) -> Result<(), DefaultRegionTerrainFurnitureRegistryError>
where
    Lookup: Fn(&str) -> Option<bool>,
{
    for table in tables.values() {
        for choice in &table.choices {
            match lookup(&choice.id) {
                None => {
                    return invalid(format!(
                        "{kind} table {:?} references missing target {:?}",
                        table.pseudo_id, choice.id
                    ));
                }
                Some(true) if !tables.contains_key(&choice.id) => {
                    return invalid(format!(
                        "{kind} table {:?} references pseudo target {:?} without a default table",
                        table.pseudo_id, choice.id
                    ));
                }
                Some(_) => {}
            }
        }
    }

    let mut states: BTreeMap<String, u8> = BTreeMap::new();
    let mut path = Vec::new();
    for id in tables.keys() {
        visit_table(kind, id, tables, &mut states, &mut path)?;
    }
    Ok(())
}

fn visit_table(
    kind: &str,
    id: &str,
    tables: &BTreeMap<String, RegionSubstitutionTable>,
    states: &mut BTreeMap<String, u8>,
    path: &mut Vec<String>,
) -> Result<(), DefaultRegionTerrainFurnitureRegistryError> {
    match states.get(id).copied().unwrap_or_default() {
        2 => return Ok(()),
        1 => {
            let start = path.iter().position(|entry| entry == id).unwrap_or(0);
            let mut cycle = path[start..].to_vec();
            cycle.push(id.to_owned());
            return invalid(format!(
                "{kind} regional substitution cycle: {}",
                cycle.join(" -> ")
            ));
        }
        _ => {}
    }
    if path.len() >= MAX_REGION_SUBSTITUTION_DEPTH {
        return invalid(format!(
            "{kind} regional substitution depth exceeds {MAX_REGION_SUBSTITUTION_DEPTH} at {id:?}"
        ));
    }
    states.insert(id.to_owned(), 1);
    path.push(id.to_owned());
    let table = tables.get(id).ok_or_else(|| {
        DefaultRegionTerrainFurnitureRegistryError::Invalid(format!(
            "internal missing {kind} table {id:?}"
        ))
    })?;
    for choice in &table.choices {
        if tables.contains_key(&choice.id) {
            visit_table(kind, &choice.id, tables, states, path)?;
        }
    }
    path.pop();
    states.insert(id.to_owned(), 2);
    Ok(())
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), DefaultRegionTerrainFurnitureRegistryError> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()) && !field.starts_with("//"))
    {
        return invalid(format!("unsupported field {field:?} in {context}"));
    }
    Ok(())
}

fn invalid<T>(reason: String) -> Result<T, DefaultRegionTerrainFurnitureRegistryError> {
    Err(DefaultRegionTerrainFurnitureRegistryError::Invalid(reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(source: &str, value: Value) -> RawRegionDefinition {
        RawRegionDefinition {
            source: source.to_owned(),
            object: serde_json::from_value(value).expect("fixture object"),
        }
    }

    fn settings(ids: &[&str]) -> Vec<RawRegionDefinition> {
        vec![raw(
            "fixture#settings",
            serde_json::json!({
                "type": "region_settings_terrain_furniture",
                "id": "default",
                "ter_furn": ids
            }),
        )]
    }

    #[test]
    fn recursive_tables_preserve_each_direct_weighted_roll() {
        let definitions = BTreeMap::from([
            (
                String::from("a"),
                vec![raw(
                    "fixture#a",
                    serde_json::json!({
                        "type": "region_terrain_furniture",
                        "id": "a",
                        "ter_id": "t_pseudo_a",
                        "replace_with_terrain": [["t_pseudo_b", 2], ["t_final_a", 1]]
                    }),
                )],
            ),
            (
                String::from("b"),
                vec![raw(
                    "fixture#b",
                    serde_json::json!({
                        "type": "region_terrain_furniture",
                        "id": "b",
                        "ter_id": "t_pseudo_b",
                        "replace_with_terrain": ["t_final_b"]
                    }),
                )],
            ),
        ]);
        let lookup = |id: &str| match id {
            "t_pseudo_a" | "t_pseudo_b" => Some(true),
            "t_final_a" | "t_final_b" => Some(false),
            _ => None,
        };
        let registry =
            compile_default_region(&settings(&["a", "b"]), &definitions, &lookup, &|_| None)
                .expect("recursive fixture resolves");

        let first = registry
            .terrain_table("t_pseudo_a")
            .expect("first direct table");
        assert_eq!(first.choices[0].id, "t_pseudo_b");
        assert_eq!(first.choices[0].weight, 2);
        assert_eq!(first.total_weight(), 3);
        assert_eq!(
            registry
                .terrain_table(&first.choices[0].id)
                .expect("second roll table")
                .choices[0]
                .id,
            "t_final_b"
        );
    }

    #[test]
    fn pseudo_cycles_and_missing_pseudo_tables_fail_closed() {
        let cycle = BTreeMap::from([
            (
                String::from("a"),
                vec![raw(
                    "fixture#a",
                    serde_json::json!({
                        "type": "region_terrain_furniture", "id": "a",
                        "ter_id": "t_a", "replace_with_terrain": ["t_b"]
                    }),
                )],
            ),
            (
                String::from("b"),
                vec![raw(
                    "fixture#b",
                    serde_json::json!({
                        "type": "region_terrain_furniture", "id": "b",
                        "ter_id": "t_b", "replace_with_terrain": ["t_a"]
                    }),
                )],
            ),
        ]);
        let pseudo_lookup = |id: &str| ["t_a", "t_b"].contains(&id).then_some(true);
        let error =
            compile_default_region(&settings(&["a", "b"]), &cycle, &pseudo_lookup, &|_| None)
                .expect_err("cycle must fail");
        assert!(error.to_string().contains("cycle"));

        let missing = BTreeMap::from([(
            String::from("a"),
            vec![raw(
                "fixture#a",
                serde_json::json!({
                    "type": "region_terrain_furniture", "id": "a",
                    "ter_id": "t_a", "replace_with_terrain": ["t_b"]
                }),
            )],
        )]);
        let error = compile_default_region(&settings(&["a"]), &missing, &pseudo_lookup, &|_| None)
            .expect_err("missing recursive table must fail");
        assert!(error.to_string().contains("without a default table"));
    }
}
