use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_OVERMAP_TERRAIN_TYPES: usize = 16_384;
pub const MAX_OVERMAP_TERRAIN_IDENTITIES: usize = 65_536;
pub const MAX_OVERMAP_TERRAIN_IDS_PER_DEFINITION: usize = 1_024;
pub const MAX_OVERMAP_TERRAIN_ID_BYTES: usize = 512;

const IDENTITY_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "flags",
    "uniform_terrain",
];
const LINEAR_PEERS: &[(&str, &str, u8)] = &[
    ("_isolated", "_four_way", 0),
    ("_end_south", "_end", 2),
    ("_end_west", "_end", 3),
    ("_ne", "_curved", 3),
    ("_end_north", "_end", 0),
    ("_ns", "_straight", 0),
    ("_es", "_curved", 0),
    ("_nes", "_tee", 3),
    ("_end_east", "_end", 1),
    ("_wn", "_curved", 2),
    ("_ew", "_straight", 3),
    ("_new", "_tee", 2),
    ("_sw", "_curved", 1),
    ("_nsw", "_tee", 1),
    ("_esw", "_tee", 0),
    ("_nesw", "_four_way", 0),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OvermapTerrainShape {
    Rotatable,
    Linear,
    NonRotating,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OvermapTerrainTypeDefinition {
    pub id: String,
    pub flags: BTreeSet<String>,
    pub uniform_terrain: Option<String>,
    /// Fields outside the identity family are retained by name so later
    /// runtime admission cannot mistake this projection for full OMT support.
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

impl OvermapTerrainTypeDefinition {
    #[must_use]
    pub fn shape(&self) -> OvermapTerrainShape {
        if self.flags.contains("LINEAR") {
            OvermapTerrainShape::Linear
        } else if self.flags.contains("NO_ROTATE") {
            OvermapTerrainShape::NonRotating
        } else {
            OvermapTerrainShape::Rotatable
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OvermapTerrainIdentity {
    pub full_id: String,
    pub type_id: String,
    pub subtype_id: String,
    pub generator_id: String,
    /// Clockwise quarter turns applied by pinned local mapgen.
    pub rotation: u8,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OvermapTerrainRegistry {
    types: BTreeMap<String, OvermapTerrainTypeDefinition>,
    identities: BTreeMap<String, OvermapTerrainIdentity>,
    abstract_count: usize,
}

impl OvermapTerrainRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, OvermapTerrainRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(OvermapTerrainRegistryError::Catalog)?;
        let pending = read_definitions(content_root.as_ref(), files)?;
        compile_registry(pending)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.types.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    #[must_use]
    pub fn identity_len(&self) -> usize {
        self.identities.len()
    }

    #[must_use]
    pub const fn abstract_count(&self) -> usize {
        self.abstract_count
    }

    #[must_use]
    pub fn get_type(&self, id: &str) -> Option<&OvermapTerrainTypeDefinition> {
        self.types.get(id)
    }

    #[must_use]
    pub fn get_identity(&self, full_id: &str) -> Option<&OvermapTerrainIdentity> {
        self.identities.get(full_id)
    }

    pub fn identities(&self) -> impl ExactSizeIterator<Item = (&str, &OvermapTerrainIdentity)> {
        self.identities
            .iter()
            .map(|(id, identity)| (id.as_str(), identity))
    }
}

#[derive(Clone)]
struct RawDefinition {
    source: String,
    object: Map<String, Value>,
}

fn read_definitions(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawDefinition>, OvermapTerrainRegistryError> {
    let mut definitions = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| OvermapTerrainRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| OvermapTerrainRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for (index, value) in values.into_iter().enumerate() {
                    collect_definition(
                        value,
                        format!("{}#{index}", file.upstream_path),
                        &mut definitions,
                    )?;
                }
            }
            value => collect_definition(value, file.upstream_path, &mut definitions)?,
        }
    }
    if definitions.len() > MAX_OVERMAP_TERRAIN_TYPES {
        return Err(OvermapTerrainRegistryError::LimitExceeded(
            "source definitions",
            MAX_OVERMAP_TERRAIN_TYPES,
        ));
    }
    Ok(definitions)
}

fn collect_definition(
    value: Value,
    source: String,
    definitions: &mut VecDeque<RawDefinition>,
) -> Result<(), OvermapTerrainRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("overmap_terrain") {
        return Ok(());
    }
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| OvermapTerrainRegistryError::Invalid(source.clone()))?;
    definitions.push_back(RawDefinition { source, object });
    Ok(())
}

fn compile_registry(
    mut pending: VecDeque<RawDefinition>,
) -> Result<OvermapTerrainRegistry, OvermapTerrainRegistryError> {
    let mut types = BTreeMap::<String, OvermapTerrainTypeDefinition>::new();
    let mut abstracts = BTreeMap::<String, OvermapTerrainTypeDefinition>::new();
    while !pending.is_empty() {
        let pass_size = pending.len();
        let mut loaded = 0_usize;
        for _ in 0..pass_size {
            let raw = pending
                .pop_front()
                .ok_or_else(|| OvermapTerrainRegistryError::Invalid(String::from("queue")))?;
            if compile_one(&raw, &mut types, &mut abstracts)? {
                loaded += 1;
            } else {
                pending.push_back(raw);
            }
        }
        if loaded == 0 {
            return Err(OvermapTerrainRegistryError::UnresolvedInheritance(
                pending
                    .iter()
                    .take(20)
                    .map(|raw| raw.source.clone())
                    .collect(),
            ));
        }
    }
    if types.len() > MAX_OVERMAP_TERRAIN_TYPES {
        return Err(OvermapTerrainRegistryError::LimitExceeded(
            "finalized types",
            MAX_OVERMAP_TERRAIN_TYPES,
        ));
    }
    let mut identities = BTreeMap::new();
    for definition in types.values() {
        for identity in identities_for_type(definition)? {
            let full_id = identity.full_id.clone();
            if identities.insert(full_id.clone(), identity).is_some() {
                return Err(OvermapTerrainRegistryError::DuplicateIdentity(full_id));
            }
            if identities.len() > MAX_OVERMAP_TERRAIN_IDENTITIES {
                return Err(OvermapTerrainRegistryError::LimitExceeded(
                    "finalized identities",
                    MAX_OVERMAP_TERRAIN_IDENTITIES,
                ));
            }
        }
    }
    Ok(OvermapTerrainRegistry {
        types,
        identities,
        abstract_count: abstracts.len(),
    })
}

fn compile_one(
    raw: &RawDefinition,
    types: &mut BTreeMap<String, OvermapTerrainTypeDefinition>,
    abstracts: &mut BTreeMap<String, OvermapTerrainTypeDefinition>,
) -> Result<bool, OvermapTerrainRegistryError> {
    let (ids, is_abstract) = definition_ids(&raw.object, &raw.source)?;
    let parent = optional_bounded_string(raw.object.get("copy-from"), "copy-from", &raw.source)?;
    let inherited = if let Some(parent) = parent.as_deref() {
        types.get(parent).or_else(|| abstracts.get(parent)).cloned()
    } else {
        None
    };
    if parent.is_some() && inherited.is_none() {
        return Ok(false);
    }
    for id in ids {
        // Pinned generic_factory starts every no-copy definition from a fresh
        // value, even when a prior definition has the same ID. Only an
        // explicit copy-from inherits prior concrete or abstract state.
        let mut definition = inherited.clone().unwrap_or_default();
        // Pinned oter loading deliberately makes uniform_terrain
        // non-inheritable even when other generic-factory fields copy.
        definition.uniform_terrain = None;
        definition.id.clone_from(&id);
        definition.source.clone_from(&raw.source);
        apply_fields(&mut definition, &raw.object, &raw.source)?;
        if definition.flags.contains("LINEAR") && definition.flags.contains("NO_ROTATE") {
            return Err(OvermapTerrainRegistryError::Invalid(format!(
                "{} has mutually exclusive LINEAR and NO_ROTATE flags in {}",
                definition.id, raw.source
            )));
        }
        if is_abstract {
            abstracts.insert(id, definition);
        } else {
            types.insert(id, definition);
        }
        if types
            .len()
            .checked_add(abstracts.len())
            .is_none_or(|count| count > MAX_OVERMAP_TERRAIN_TYPES)
        {
            return Err(OvermapTerrainRegistryError::LimitExceeded(
                "compiled concrete and abstract types",
                MAX_OVERMAP_TERRAIN_TYPES,
            ));
        }
    }
    Ok(true)
}

fn definition_ids(
    object: &Map<String, Value>,
    source: &str,
) -> Result<(Vec<String>, bool), OvermapTerrainRegistryError> {
    match (object.get("id"), object.get("abstract")) {
        (Some(value), None) => Ok((bounded_ids(value, "id", source)?, false)),
        (None, Some(value)) => Ok((vec![bounded_string(value, "abstract", source)?], true)),
        _ => Err(OvermapTerrainRegistryError::Invalid(format!(
            "overmap terrain must have exactly one id or abstract in {source}"
        ))),
    }
}

fn bounded_ids(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<Vec<String>, OvermapTerrainRegistryError> {
    let values = match value {
        Value::String(id) => vec![id.clone()],
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    OvermapTerrainRegistryError::Invalid(format!(
                        "{field} must contain strings in {source}"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(OvermapTerrainRegistryError::Invalid(format!(
                "{field} must be a string or string array in {source}"
            )));
        }
    };
    if values.is_empty() || values.len() > MAX_OVERMAP_TERRAIN_IDS_PER_DEFINITION {
        return Err(OvermapTerrainRegistryError::LimitExceeded(
            "ids per definition",
            MAX_OVERMAP_TERRAIN_IDS_PER_DEFINITION,
        ));
    }
    let mut unique = BTreeSet::new();
    for id in &values {
        // Pinned core defines the generic-factory null OMT with an empty ID.
        // It remains loadable for exact registry parity but protocol admission
        // rejects it through the ordinary non-empty worldgen ID bound.
        let valid_null = field == "id" && values.len() == 1 && id.is_empty();
        if (!valid_null && !valid_id(id)) || !unique.insert(id) {
            return Err(OvermapTerrainRegistryError::Invalid(format!(
                "invalid or duplicate {field} in {source}"
            )));
        }
    }
    Ok(values)
}

fn apply_fields(
    definition: &mut OvermapTerrainTypeDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), OvermapTerrainRegistryError> {
    if let Some(value) = object.get("flags") {
        definition.flags = string_set(value, "flags", source)?;
    }
    if let Some(value) = modifier(object, "extend", "flags", source)? {
        definition.flags.extend(string_set(value, "flags", source)?);
    }
    if let Some(value) = modifier(object, "delete", "flags", source)? {
        for flag in string_set(value, "flags", source)? {
            definition.flags.remove(&flag);
        }
    }
    if let Some(value) = object.get("uniform_terrain") {
        definition.uniform_terrain = match value {
            Value::Null => None,
            _ => Some(bounded_string(value, "uniform_terrain", source)?),
        };
    }
    for field in object.keys() {
        if !field.starts_with("//")
            && !IDENTITY_FIELDS.contains(&field.as_str())
            && !matches!(
                field.as_str(),
                "extend" | "delete" | "relative" | "proportional"
            )
        {
            definition.unsupported_fields.insert(field.clone());
        }
    }
    for modifier_name in ["extend", "delete", "relative", "proportional"] {
        if let Some(value) = object.get(modifier_name) {
            let fields = value.as_object().ok_or_else(|| {
                OvermapTerrainRegistryError::Invalid(format!(
                    "{modifier_name} must be an object in {source}"
                ))
            })?;
            for field in fields.keys() {
                if field != "flags" {
                    definition.unsupported_fields.insert(field.clone());
                }
            }
        }
    }
    Ok(())
}

fn identities_for_type(
    definition: &OvermapTerrainTypeDefinition,
) -> Result<Vec<OvermapTerrainIdentity>, OvermapTerrainRegistryError> {
    let id = definition.id.as_str();
    let identities = match definition.shape() {
        OvermapTerrainShape::NonRotating => vec![OvermapTerrainIdentity {
            full_id: id.to_owned(),
            type_id: id.to_owned(),
            subtype_id: id.to_owned(),
            generator_id: id.to_owned(),
            rotation: 0,
        }],
        OvermapTerrainShape::Rotatable => ["north", "east", "south", "west"]
            .into_iter()
            .enumerate()
            .map(|(rotation, direction)| OvermapTerrainIdentity {
                full_id: format!("{id}_{direction}"),
                type_id: id.to_owned(),
                subtype_id: id.to_owned(),
                generator_id: id.to_owned(),
                rotation: u8::try_from(rotation).expect("four rotations fit u8"),
            })
            .collect(),
        OvermapTerrainShape::Linear => LINEAR_PEERS
            .iter()
            .map(
                |(full_suffix, mapgen_suffix, rotation)| OvermapTerrainIdentity {
                    full_id: format!("{id}{full_suffix}"),
                    type_id: id.to_owned(),
                    subtype_id: format!("{id}{mapgen_suffix}"),
                    generator_id: format!("{id}{mapgen_suffix}"),
                    rotation: *rotation,
                },
            )
            .collect(),
    };
    Ok(identities)
}

fn modifier<'a>(
    object: &'a Map<String, Value>,
    modifier_name: &str,
    field: &str,
    source: &str,
) -> Result<Option<&'a Value>, OvermapTerrainRegistryError> {
    match object.get(modifier_name) {
        None => Ok(None),
        Some(Value::Object(fields)) => Ok(fields.get(field)),
        Some(_) => Err(OvermapTerrainRegistryError::Invalid(format!(
            "{modifier_name} must be an object in {source}"
        ))),
    }
}

fn string_set(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<BTreeSet<String>, OvermapTerrainRegistryError> {
    value
        .as_array()
        .ok_or_else(|| {
            OvermapTerrainRegistryError::Invalid(format!("{field} must be an array in {source}"))
        })?
        .iter()
        .map(|value| bounded_string(value, field, source))
        .collect()
}

fn optional_bounded_string(
    value: Option<&Value>,
    field: &str,
    source: &str,
) -> Result<Option<String>, OvermapTerrainRegistryError> {
    value
        .map(|value| bounded_string(value, field, source))
        .transpose()
}

fn bounded_string(
    value: &Value,
    field: &str,
    source: &str,
) -> Result<String, OvermapTerrainRegistryError> {
    let value = value.as_str().ok_or_else(|| {
        OvermapTerrainRegistryError::Invalid(format!("{field} must be a string in {source}"))
    })?;
    if !valid_id(value) {
        return Err(OvermapTerrainRegistryError::Invalid(format!(
            "invalid {field} in {source}"
        )));
    }
    Ok(value.to_owned())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OVERMAP_TERRAIN_ID_BYTES
        && value.chars().all(|character| !character.is_control())
}

#[derive(Debug)]
pub enum OvermapTerrainRegistryError {
    Catalog(ModCatalogError),
    DuplicateIdentity(String),
    Invalid(String),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    LimitExceeded(&'static str, usize),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for OvermapTerrainRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => {
                write!(formatter, "overmap-terrain mod selection failed: {error}")
            }
            Self::DuplicateIdentity(id) => {
                write!(formatter, "duplicate finalized OMT identity {id}")
            }
            Self::Invalid(reason) => {
                write!(formatter, "invalid overmap-terrain definition: {reason}")
            }
            Self::Io(path, error) => {
                write!(formatter, "overmap-terrain I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "overmap-terrain JSON failed for {path}: {error}")
            }
            Self::LimitExceeded(kind, limit) => {
                write!(formatter, "overmap-terrain {kind} exceeds limit {limit}")
            }
            Self::UnresolvedInheritance(sources) => {
                write!(
                    formatter,
                    "unresolved or cyclic overmap-terrain inheritance: {sources:?}"
                )
            }
        }
    }
}

impl std::error::Error for OvermapTerrainRegistryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Io(_, error) => Some(error),
            Self::Json(_, error) => Some(error),
            Self::DuplicateIdentity(_)
            | Self::Invalid(_)
            | Self::LimitExceeded(_, _)
            | Self::UnresolvedInheritance(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(value: Value, source: &str) -> RawDefinition {
        RawDefinition {
            source: source.to_owned(),
            object: value.as_object().expect("fixture object").clone(),
        }
    }

    #[test]
    fn inheritance_and_overlays_retain_unsupported_semantics_fail_closed() {
        let definitions = VecDeque::from([
            raw(
                serde_json::json!({
                    "type": "overmap_terrain",
                    "abstract": "open_land",
                    "flags": ["NO_ROTATE"],
                    "uniform_terrain": "t_grass",
                    "name": "open land"
                }),
                "base",
            ),
            raw(
                serde_json::json!({
                    "type": "overmap_terrain",
                    "id": "field",
                    "copy-from": "open_land",
                    "extend": { "flags": ["TEST_FLAG"] }
                }),
                "field",
            ),
            raw(
                serde_json::json!({
                    "type": "overmap_terrain",
                    "id": "field",
                    "copy-from": "field",
                    "delete": { "flags": ["TEST_FLAG"] },
                    "spawns": { "group": "GROUP_TEST" }
                }),
                "overlay",
            ),
        ]);
        let registry = compile_registry(definitions).expect("fixture should compile");
        let field = registry.get_type("field").expect("field should finalize");
        assert_eq!(field.flags, BTreeSet::from([String::from("NO_ROTATE")]));
        assert_eq!(field.uniform_terrain, None);
        assert_eq!(
            field.unsupported_fields,
            BTreeSet::from([String::from("name"), String::from("spawns")])
        );
        assert_eq!(registry.abstract_count(), 1);
        assert_eq!(registry.identity_len(), 1);
        assert_eq!(
            registry
                .get_identity("field")
                .expect("nonrotating identity")
                .generator_id,
            "field"
        );
    }

    #[test]
    fn no_copy_redefinition_resets_prior_identity_state() {
        let definitions = VecDeque::from([
            raw(
                serde_json::json!({
                    "type": "overmap_terrain",
                    "id": "field",
                    "flags": ["LINEAR"],
                    "uniform_terrain": "t_grass",
                    "name": "old field"
                }),
                "old",
            ),
            raw(
                serde_json::json!({
                    "type": "overmap_terrain",
                    "id": "field",
                    "flags": ["NO_ROTATE"]
                }),
                "replacement",
            ),
        ]);
        let registry = compile_registry(definitions).expect("replacement should compile");
        let field = registry.get_type("field").expect("field should finalize");
        assert_eq!(field.flags, BTreeSet::from([String::from("NO_ROTATE")]));
        assert_eq!(field.uniform_terrain, None);
        assert!(field.unsupported_fields.is_empty());
        assert_eq!(registry.identity_len(), 1);
        assert!(registry.get_identity("field").is_some());
        assert!(registry.get_identity("field_ew").is_none());
    }

    #[test]
    fn invalid_shapes_and_unresolved_inheritance_are_rejected() {
        let incompatible = VecDeque::from([raw(
            serde_json::json!({
                "type": "overmap_terrain",
                "id": "bad",
                "flags": ["LINEAR", "NO_ROTATE"]
            }),
            "bad",
        )]);
        assert!(matches!(
            compile_registry(incompatible),
            Err(OvermapTerrainRegistryError::Invalid(_))
        ));

        let missing_parent = VecDeque::from([raw(
            serde_json::json!({
                "type": "overmap_terrain",
                "id": "child",
                "copy-from": "missing"
            }),
            "child",
        )]);
        assert!(matches!(
            compile_registry(missing_parent),
            Err(OvermapTerrainRegistryError::UnresolvedInheritance(_))
        ));

        let abstract_array = VecDeque::from([raw(
            serde_json::json!({
                "type": "overmap_terrain",
                "abstract": ["first", "second"]
            }),
            "abstract-array",
        )]);
        assert!(matches!(
            compile_registry(abstract_array),
            Err(OvermapTerrainRegistryError::Invalid(_))
        ));
    }

    #[test]
    fn peer_identity_shapes_match_pinned_rotation_semantics() {
        let rotatable = OvermapTerrainTypeDefinition {
            id: String::from("shelter"),
            ..OvermapTerrainTypeDefinition::default()
        };
        let peers = identities_for_type(&rotatable).expect("rotatable peers");
        assert_eq!(
            peers
                .iter()
                .map(|peer| (
                    peer.full_id.as_str(),
                    peer.generator_id.as_str(),
                    peer.rotation
                ))
                .collect::<Vec<_>>(),
            [
                ("shelter_north", "shelter", 0),
                ("shelter_east", "shelter", 1),
                ("shelter_south", "shelter", 2),
                ("shelter_west", "shelter", 3),
            ]
        );

        let linear = OvermapTerrainTypeDefinition {
            id: String::from("road"),
            flags: BTreeSet::from([String::from("LINEAR")]),
            ..OvermapTerrainTypeDefinition::default()
        };
        let peers = identities_for_type(&linear).expect("linear peers");
        let east_west = peers
            .iter()
            .find(|peer| peer.full_id == "road_ew")
            .expect("road east-west peer");
        assert_eq!(east_west.subtype_id, "road_straight");
        assert_eq!(east_west.generator_id, "road_straight");
        assert_eq!(east_west.rotation, 3);
    }
}
