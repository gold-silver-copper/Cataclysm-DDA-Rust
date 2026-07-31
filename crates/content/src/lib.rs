//! Pinned upstream content identity and provenance primitives.

mod anatomy;
mod bash;
mod city;
mod construction;
mod eoc;
mod eoc_math;
mod field;
mod furniture;
mod item;
mod item_group;
mod mapgen;
mod material;
mod monster;
mod monster_group;
mod overmap_special;
mod overmap_terrain;
mod proficiency;
mod recipe;
mod region;
mod river;
mod skill;
mod snippet;
mod start_location;
mod terrain;

pub use anatomy::{
    ANATOMY_SCALE, AnatomyDefinition, AnatomyRegistry, AnatomyRegistryError, BodyPartDefinition,
    BodyPartStatHpModifiers, MAX_ANATOMY_DEFINITIONS, MAX_ANATOMY_PARTS, MAX_BODY_PART_DEFINITIONS,
};
pub use bash::{
    BASH_MULTIPLIER_SCALE, BashDamageProfileDefinition, BashDamageProfileRegistry,
    BashDamageProfileRegistryError, BashDefinition, BashFieldEffectDefinition, BashItemGroupSource,
};
pub use city::{
    CitySettingsDefinition, CitySettingsRegistry, CitySettingsRegistryError,
    DEFAULT_CITY_SETTINGS_ID, MAX_CITY_SETTINGS, MAX_UPSTREAM_CITY_SIZE, MAX_UPSTREAM_CITY_SPACING,
};
pub use construction::{
    ConstructionDefinition, ConstructionGroupDefinition, ConstructionRegistry,
    ConstructionRegistryError,
};
pub use eoc::{
    EffectOnConditionDefinition, EffectOnConditionRegistry, EffectOnConditionRegistryError,
    EocActorStatDefinition, EocConditionDefinition, EocDelayDefinition, EocEffectDefinition,
    EocEventTriggerDefinition, EocStringValueDefinition, MAX_EOC_TREE_DEPTH,
};
pub use eoc_math::{
    EocMathAssignmentDefinition, EocMathAssignmentOperationDefinition, EocMathExpressionDefinition,
    MAX_EOC_MATH_NODES, MAX_EOC_MATH_SOURCE_BYTES, MAX_EOC_SAFE_INTEGER,
};
pub use field::{
    FieldIntensityDefinition, FieldTypeDefinition, FieldTypeRegistry, FieldTypeRegistryError,
};
pub use furniture::{FurnitureDefinition, FurnitureRegistry, FurnitureRegistryError};
pub use item::{
    ArmorMaterialLayerDefinition, ArmorPortionDefinition, ItemDefinition, ItemEocActionDefinition,
    ItemQualityDefinition, ItemRegistry, ItemRegistryError, ItemSnippetDefinition,
    ItemTemperatureRuntimeClass, ItemVariableValueDefinition, ItemVariantDefinition,
    MagazineWellDefinition, PocketDefinition, PocketTypeDefinition, SpawnPocketKindDefinition,
    StrictMagazineDefinition, StrictSpawnPocketDefinition,
};
pub use item_group::{
    ItemGroupChargesRange, ItemGroupContentsSource, ItemGroupDefinition, ItemGroupEntryWrapper,
    ItemGroupEvent, ItemGroupNode, ItemGroupNodeId, ItemGroupNodeKind, ItemGroupOverflow,
    ItemGroupRange, ItemGroupRegistry, ItemGroupRegistryError, ItemGroupSubtype, ItemGroupWrapper,
    MAX_ITEM_GROUP_LOCAL_DEPTH, MAX_ITEM_GROUP_NODES, MAX_ITEM_GROUP_OUTPUT,
    MAX_ITEM_GROUP_QUANTITY, MAX_ITEM_GROUP_REFERENCE_DEPTH, StrictItemGroupDefinition,
    StrictItemGroupGraph, StrictItemGroupNode, StrictItemGroupNodeKind,
};
pub use mapgen::{
    DEFAULT_MAPGEN_WEIGHT, MAPGEN_HEIGHT, MAPGEN_WIDTH, MAX_MAPGEN_BINDINGS,
    MAX_MAPGEN_CHOICE_ENTRIES, MAX_MAPGEN_CHOICE_TOTAL_WEIGHT, MAX_MAPGEN_CHOICE_WEIGHT,
    MAX_MAPGEN_OM_TERRAINS, MAX_MAPGEN_OMT_ASSIGNMENTS, MAX_MAPGEN_PALETTE_DEPTH,
    MAX_MAPGEN_PALETTE_LAYERS, MAX_MAPGEN_REPORT_ASSIGNMENTS, MAX_MAPGEN_ROOTS,
    MAX_MAPGEN_VARIANTS, MAX_MAPGEN_WEIGHT, MAX_NAMED_PALETTES, MAX_NESTED_MAPGEN_DEFINITIONS,
    MAX_NESTED_MAPGEN_DEPTH, MAX_NESTED_MAPGEN_PLACEMENTS, MapgenCoordinateRange, MapgenIdChoice,
    MapgenRegistry, MapgenRegistryError, MapgenRootReport, MapgenU16Range,
    StrictMapgenAreaItemPlacement, StrictMapgenChunkChoice, StrictMapgenDefinition,
    StrictMapgenIndividualMonsterPlacement, StrictMapgenIndividualMonsterTarget,
    StrictMapgenItemPlacement, StrictMapgenMonsterPlacement, StrictMapgenNeighborFlags,
    StrictMapgenNeighborMatch, StrictMapgenNestedConditions, StrictMapgenNestedPlacement,
    StrictMapgenOmtMatch, StrictNestedMapgenDefinition, WeightedMapgenId,
};
pub use material::{
    ComestibleThermalProperties, MaterialRegistry, MaterialRegistryError, MaterialThermalDefinition,
};
pub use monster::{
    MonsterDefinition, MonsterRegistry, MonsterRegistryError, MonsterSpecialAttackDefinition,
    MonsterSpecialAttackKind,
};
pub use monster_group::{
    MAX_MONSTER_GROUP_ENTRIES, MAX_MONSTER_GROUP_FREQUENCY, MAX_MONSTER_GROUP_PACK_SIZE,
    MAX_MONSTER_GROUPS, MonsterGroupDefinition, MonsterGroupEntry, MonsterGroupRegistry,
    MonsterGroupRegistryError, MonsterGroupTarget,
};
pub use overmap_special::{
    MAX_OVERMAP_SPECIALS, OvermapLocationDefinition, OvermapSpecialConnectionDefinition,
    OvermapSpecialDefinition, OvermapSpecialInterval, OvermapSpecialMonsterSpawnDefinition,
    OvermapSpecialRegistry, OvermapSpecialRegistryError, OvermapSpecialTerrainDefinition,
};
pub use overmap_terrain::{
    MAX_OVERMAP_TERRAIN_ID_BYTES, MAX_OVERMAP_TERRAIN_IDENTITIES,
    MAX_OVERMAP_TERRAIN_IDS_PER_DEFINITION, MAX_OVERMAP_TERRAIN_TYPES, OvermapTerrainIdentity,
    OvermapTerrainRegistry, OvermapTerrainRegistryError, OvermapTerrainShape,
    OvermapTerrainTypeDefinition,
};
pub use proficiency::{
    PROFICIENCY_MULTIPLIER_SCALE, ProficiencyDefinition, ProficiencyRegistry,
    ProficiencyRegistryError,
};
pub use recipe::{
    ComponentRequirement, ExternalRequirement, QualityRequirement, RecipeDefinition,
    RecipeProficiency, RecipeRegistry, RecipeRegistryError, RequirementDefinition, ToolRequirement,
};
pub use region::{
    DEFAULT_REGION_TERRAIN_FURNITURE_ID, DefaultRegionTerrainFurnitureRegistry,
    DefaultRegionTerrainFurnitureRegistryError, MAX_DEFAULT_REGION_TABLES,
    MAX_REGION_SUBSTITUTION_CHOICES, MAX_REGION_SUBSTITUTION_DEFINITIONS,
    MAX_REGION_SUBSTITUTION_DEPTH, MAX_REGION_SUBSTITUTION_TOTAL_WEIGHT,
    MAX_REGION_SUBSTITUTION_WEIGHT, RegionSubstitutionTable, WeightedRegionSubstitution,
};
pub use river::{
    DEFAULT_RIVER_SETTINGS_ID, MAX_RIVER_SETTINGS, RiverSettingsDefinition, RiverSettingsRegistry,
    RiverSettingsRegistryError,
};
pub use skill::{SkillDefinition, SkillRegistry, SkillRegistryError};
pub use snippet::{
    DescriptionSnippetCategory, DescriptionSnippetChoice, DescriptionSnippetRegistry,
    DescriptionSnippetRegistryError,
};
pub use start_location::{
    DEFAULT_START_LOCATION_MAX_Z, DEFAULT_START_LOCATION_MIN_Z, InclusiveI32Interval,
    MAX_START_LOCATION_FLAGS, MAX_START_LOCATION_ID_BYTES, MAX_START_LOCATION_NAME_BYTES,
    MAX_START_LOCATION_PARAMETER_BYTES, MAX_START_LOCATION_PARAMETERS, MAX_START_LOCATION_TARGETS,
    MAX_START_LOCATIONS, OvermapTerrainMatchType, StartLocationDefinition, StartLocationRegistry,
    StartLocationRegistryError, StartLocationTarget,
};
pub use terrain::{TerrainDefinition, TerrainRegistry, TerrainRegistryError};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

pub const PINNED_UPSTREAM_COMMIT: &str = "4dfd36038b16650dc1b5cb9d79a3e42363174b05";
pub const DEFAULT_MANIFEST_PATH: &str = "vendor/cdda-content-manifest.json";
pub const DEFAULT_CORE_MOD_ID: &str = "dda";
const VIRTUAL_AMMUNITION_DEFAULTS: &[(&str, &str)] =
    &[("components", "components"), ("thrown", "thrown")];
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
pub const REQUIRED_CONTENT_ROOTS: &[&str] = &[
    "data/core",
    "data/json",
    "data/names",
    "data/raw",
    "data/mods",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProvenanceEntry {
    pub upstream_path: String,
    pub destination: String,
    pub blake3: [u8; 32],
    pub license: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentManifest {
    pub upstream_commit: String,
    pub entries: Vec<ProvenanceEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SchemaInventory {
    pub upstream_commit: String,
    pub manifest_hash: String,
    pub json_files: u64,
    pub top_level_objects: u64,
    pub top_level_non_objects: u64,
    pub definitions: BTreeMap<String, DefinitionInventory>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DefinitionInventory {
    pub support: SupportStatus,
    pub objects: u64,
    pub source_files: u64,
    pub example_files: Vec<String>,
    pub fields: BTreeMap<String, FieldInventory>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldInventory {
    pub support: SupportStatus,
    pub occurrences: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    LoaderImplemented,
    Unimplemented,
}

#[derive(Default)]
struct DefinitionAccumulator {
    objects: u64,
    source_files: BTreeSet<String>,
    fields: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModCatalog {
    mods: BTreeMap<String, ModInfo>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub authors: BTreeSet<String>,
    pub maintainers: BTreeSet<String>,
    pub category: String,
    pub dependencies: Vec<String>,
    pub conflicts: Vec<String>,
    pub core: bool,
    pub obsolete: bool,
    pub content_directory: String,
    pub loading_images: BTreeSet<String>,
    pub disable_other_loading_screens: bool,
    pub version: String,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedContentFile {
    pub owner: String,
    pub upstream_path: String,
    pub destination: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmmunitionRegistry {
    types: BTreeMap<String, AmmunitionType>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmmunitionType {
    pub id: String,
    pub name: String,
    pub default_item: String,
    pub source: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModInfo {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    authors: BTreeSet<String>,
    #[serde(default)]
    maintainers: BTreeSet<String>,
    #[serde(default)]
    category: String,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    conflicts: Vec<String>,
    #[serde(default)]
    core: bool,
    #[serde(default)]
    obsolete: bool,
    #[serde(default)]
    path: String,
    #[serde(default)]
    loading_images: BTreeSet<String>,
    #[serde(default)]
    disable_other_loading_screens: bool,
    #[serde(default)]
    version: String,
    #[serde(default, rename = "//")]
    _comment: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAmmunitionType {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    name: String,
    default: String,
    #[serde(default, rename = "//")]
    _comment: Option<serde_json::Value>,
}

impl ContentManifest {
    pub fn new(mut entries: Vec<ProvenanceEntry>) -> Result<Self, ManifestError> {
        entries.sort_by(|left, right| left.upstream_path.cmp(&right.upstream_path));
        let manifest = Self {
            upstream_commit: PINNED_UPSTREAM_COMMIT.to_owned(),
            entries,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.upstream_commit != PINNED_UPSTREAM_COMMIT {
            return Err(ManifestError::WrongCommit);
        }
        let mut upstream_paths = BTreeSet::new();
        let mut destinations = BTreeSet::new();
        let mut previous_path: Option<&str> = None;
        for entry in &self.entries {
            if !is_safe_relative_path(&entry.upstream_path) {
                return Err(ManifestError::UnsafePath(entry.upstream_path.clone()));
            }
            if !is_safe_relative_path(&entry.destination) {
                return Err(ManifestError::UnsafeDestination(entry.destination.clone()));
            }
            if entry.license.trim().is_empty() {
                return Err(ManifestError::UnknownLicense(entry.upstream_path.clone()));
            }
            if !REQUIRED_CONTENT_ROOTS.iter().any(|root| {
                entry.upstream_path == *root || entry.upstream_path.starts_with(&format!("{root}/"))
            }) {
                return Err(ManifestError::PathOutsideScope(entry.upstream_path.clone()));
            }
            if previous_path.is_some_and(|path| path > entry.upstream_path.as_str()) {
                return Err(ManifestError::NotCanonicalOrder);
            }
            previous_path = Some(&entry.upstream_path);
            if !upstream_paths.insert(&entry.upstream_path) {
                return Err(ManifestError::DuplicateUpstreamPath(
                    entry.upstream_path.clone(),
                ));
            }
            if !destinations.insert(&entry.destination) {
                return Err(ManifestError::DuplicateDestination(
                    entry.destination.clone(),
                ));
            }
        }
        Ok(())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, ContentLoadError> {
        let path = path.as_ref();
        let metadata = fs::metadata(path).map_err(ContentLoadError::Io)?;
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(ContentLoadError::ManifestTooLarge(metadata.len()));
        }
        let bytes = fs::read(path).map_err(ContentLoadError::Io)?;
        let manifest: Self = serde_json::from_slice(&bytes).map_err(ContentLoadError::Json)?;
        manifest.validate().map_err(ContentLoadError::Manifest)?;
        Ok(manifest)
    }

    /// Verifies every manifested file relative to `content_root`. Symlinks and
    /// other special files are rejected so content cannot escape the package.
    pub fn verify_files(&self, content_root: impl AsRef<Path>) -> Result<(), ContentVerifyError> {
        self.validate().map_err(ContentVerifyError::Manifest)?;
        let content_root = content_root.as_ref();
        for entry in &self.entries {
            let path = content_root.join(&entry.destination);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| ContentVerifyError::Io(entry.destination.clone(), error))?;
            if !metadata.file_type().is_file() {
                return Err(ContentVerifyError::NotRegularFile(
                    entry.destination.clone(),
                ));
            }
            let bytes = fs::read(&path)
                .map_err(|error| ContentVerifyError::Io(entry.destination.clone(), error))?;
            if *blake3::hash(&bytes).as_bytes() != entry.blake3 {
                return Err(ContentVerifyError::HashMismatch(entry.destination.clone()));
            }
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<[u8; 32], ManifestError> {
        self.validate()?;
        let mut hasher = blake3::Hasher::new_derive_key("cdda-rust content manifest v1");
        update_string(&mut hasher, &self.upstream_commit);
        hasher.update(&(self.entries.len() as u64).to_be_bytes());
        for entry in &self.entries {
            update_string(&mut hasher, &entry.upstream_path);
            update_string(&mut hasher, &entry.destination);
            hasher.update(&entry.blake3);
            update_string(&mut hasher, &entry.license);
        }
        Ok(*hasher.finalize().as_bytes())
    }
}

impl SchemaInventory {
    pub fn build(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
    ) -> Result<Self, SchemaInventoryError> {
        manifest
            .verify_files(&content_root)
            .map_err(SchemaInventoryError::Verification)?;
        let content_root = content_root.as_ref();
        let mut json_files = 0_u64;
        let mut top_level_objects = 0_u64;
        let mut top_level_non_objects = 0_u64;
        let mut definitions: BTreeMap<String, DefinitionAccumulator> = BTreeMap::new();
        for entry in manifest
            .entries
            .iter()
            .filter(|entry| entry.destination.ends_with(".json"))
        {
            json_files = json_files
                .checked_add(1)
                .ok_or(SchemaInventoryError::NumericOverflow)?;
            let bytes = fs::read(content_root.join(&entry.destination))
                .map_err(|error| SchemaInventoryError::Io(entry.destination.clone(), error))?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|error| SchemaInventoryError::Json(entry.destination.clone(), error))?;
            match value {
                serde_json::Value::Array(values) => {
                    for value in values {
                        if let serde_json::Value::Object(object) = value {
                            top_level_objects = top_level_objects
                                .checked_add(1)
                                .ok_or(SchemaInventoryError::NumericOverflow)?;
                            record_definition(&mut definitions, &entry.upstream_path, &object)?;
                        } else {
                            top_level_non_objects = top_level_non_objects
                                .checked_add(1)
                                .ok_or(SchemaInventoryError::NumericOverflow)?;
                        }
                    }
                }
                serde_json::Value::Object(object) => {
                    top_level_objects = top_level_objects
                        .checked_add(1)
                        .ok_or(SchemaInventoryError::NumericOverflow)?;
                    record_definition(&mut definitions, &entry.upstream_path, &object)?;
                }
                _ => {
                    top_level_non_objects = top_level_non_objects
                        .checked_add(1)
                        .ok_or(SchemaInventoryError::NumericOverflow)?;
                }
            }
        }
        let definitions = definitions
            .into_iter()
            .map(|(name, accumulator)| {
                let fields = accumulator
                    .fields
                    .into_iter()
                    .map(|(field, occurrences)| {
                        let support = field_support(&name, &field);
                        (
                            field,
                            FieldInventory {
                                support,
                                occurrences,
                            },
                        )
                    })
                    .collect();
                let source_files = u64::try_from(accumulator.source_files.len())
                    .map_err(|_| SchemaInventoryError::NumericOverflow)?;
                let example_files = accumulator.source_files.into_iter().take(5).collect();
                let support = definition_support(&name);
                Ok((
                    name,
                    DefinitionInventory {
                        support,
                        objects: accumulator.objects,
                        source_files,
                        example_files,
                        fields,
                    },
                ))
            })
            .collect::<Result<_, SchemaInventoryError>>()?;
        Ok(Self {
            upstream_commit: manifest.upstream_commit.clone(),
            manifest_hash: blake3::Hash::from_bytes(
                manifest
                    .canonical_hash()
                    .map_err(SchemaInventoryError::Manifest)?,
            )
            .to_string(),
            json_files,
            top_level_objects,
            top_level_non_objects,
            definitions,
        })
    }
}

fn definition_support(kind: &str) -> SupportStatus {
    if matches!(
        kind,
        "MOD_INFO"
            | "ammunition_type"
            | "bash_damage_profile"
            | "construction"
            | "construction_group"
            | "field_type"
            | "item_group"
    ) {
        SupportStatus::LoaderImplemented
    } else {
        SupportStatus::Unimplemented
    }
}

fn field_support(kind: &str, field: &str) -> SupportStatus {
    if (kind == "bash_damage_profile" && bash::field_is_implemented(field))
        || (kind == "construction" && construction::field_is_implemented(field))
        || (kind == "construction_group" && construction::group_field_is_implemented(field))
        || (kind == "ITEM" && item::field_is_implemented(field))
        || (kind == "item_group" && item_group::field_is_implemented(field))
        || (kind == "MONSTER" && monster::field_is_implemented(field))
        || (kind == "field_type" && field::field_is_implemented(field))
        || (kind == "terrain" && terrain::field_is_implemented(field))
        || (kind == "furniture" && furniture::field_is_implemented(field))
        || (kind == "proficiency" && proficiency::field_is_implemented(field))
        || (kind == "recipe" && recipe::recipe_field_is_implemented(field))
        || (kind == "requirement" && recipe::requirement_field_is_implemented(field))
        || (kind == "skill" && skill::field_is_implemented(field))
    {
        SupportStatus::LoaderImplemented
    } else if kind == "item_group" {
        SupportStatus::Unimplemented
    } else {
        definition_support(kind)
    }
}

impl ModCatalog {
    pub fn load(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
    ) -> Result<Self, ModCatalogError> {
        manifest
            .verify_files(&content_root)
            .map_err(ModCatalogError::Verification)?;
        let content_root = content_root.as_ref();
        let mut mods = BTreeMap::new();
        for entry in manifest.entries.iter().filter(|entry| {
            entry.destination.ends_with("/modinfo.json")
                || entry.upstream_path == "data/mods/default.json"
        }) {
            let bytes = fs::read(content_root.join(&entry.destination))
                .map_err(|error| ModCatalogError::Io(entry.destination.clone(), error))?;
            let values: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
                .map_err(|error| ModCatalogError::Json(entry.destination.clone(), error))?;
            for value in values {
                if value.get("type").and_then(serde_json::Value::as_str) != Some("MOD_INFO") {
                    continue;
                }
                let raw: RawModInfo = serde_json::from_value(value)
                    .map_err(|error| ModCatalogError::Json(entry.destination.clone(), error))?;
                if raw.kind != "MOD_INFO" {
                    return Err(ModCatalogError::WrongType(raw.kind));
                }
                validate_mod_id(&raw.id)?;
                if raw
                    .dependencies
                    .iter()
                    .any(|dependency| dependency == &raw.id)
                {
                    return Err(ModCatalogError::SelfDependency(raw.id));
                }
                let content_directory = resolve_mod_path(&entry.destination, &raw.path)?;
                let metadata = fs::symlink_metadata(content_root.join(&content_directory))
                    .map_err(|error| ModCatalogError::Io(content_directory.clone(), error))?;
                if !metadata.file_type().is_dir() {
                    return Err(ModCatalogError::InvalidContentDirectory(content_directory));
                }
                let id = raw.id.clone();
                let info = ModInfo {
                    id: raw.id,
                    name: raw.name,
                    description: raw.description,
                    authors: raw.authors,
                    maintainers: raw.maintainers,
                    category: raw.category,
                    dependencies: raw.dependencies,
                    conflicts: raw.conflicts,
                    core: raw.core,
                    obsolete: raw.obsolete,
                    content_directory,
                    loading_images: raw.loading_images,
                    disable_other_loading_screens: raw.disable_other_loading_screens,
                    version: raw.version,
                    source: entry.upstream_path.clone(),
                };
                if mods.insert(id.clone(), info).is_some() {
                    return Err(ModCatalogError::DuplicateId(id));
                }
            }
        }
        for info in mods.values() {
            for dependency in &info.dependencies {
                if !mods.contains_key(dependency) {
                    return Err(ModCatalogError::MissingDependency {
                        id: info.id.clone(),
                        dependency: dependency.clone(),
                    });
                }
            }
        }
        Ok(Self { mods })
    }

    pub fn len(&self) -> usize {
        self.mods.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&ModInfo> {
        self.mods.get(id)
    }

    /// Iterates every pinned mod definition in stable ID order. Callers that
    /// construct new-world choices must still use `resolve_new_world`, which
    /// rejects obsolete, conflicting, cyclic, and invalid-core selections.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ModInfo)> {
        self.mods
            .iter()
            .map(|(id, information)| (id.as_str(), information))
    }

    pub fn recommended_new_world(&self) -> Result<Vec<String>, ModCatalogError> {
        let defaults = self
            .mods
            .get("dev:default")
            .ok_or(ModCatalogError::MissingDeveloperDefaults)?;
        self.resolve_new_world(&defaults.dependencies)
    }

    pub fn selected_json_files(
        &self,
        manifest: &ContentManifest,
        enabled: &[String],
    ) -> Result<Vec<SelectedContentFile>, ModCatalogError> {
        let canonical = self.resolve_new_world(enabled)?;
        if canonical != enabled {
            return Err(ModCatalogError::NonCanonicalLoadOrder {
                expected: canonical,
                actual: enabled.to_vec(),
            });
        }
        let mut files = Vec::new();
        let mut seen = BTreeSet::new();
        append_selected_files(manifest, "core", "cdda/data/core", &mut seen, &mut files);
        for id in enabled {
            let info = self
                .mods
                .get(id)
                .ok_or_else(|| ModCatalogError::UnknownMod(id.clone()))?;
            append_selected_files(manifest, id, &info.content_directory, &mut seen, &mut files);
        }
        Ok(files)
    }

    /// Resolves dependencies in deterministic load order and rejects obsolete,
    /// cyclic, missing, conflicting, or multi-core selections for a new world.
    pub fn resolve_new_world(&self, requested: &[String]) -> Result<Vec<String>, ModCatalogError> {
        if requested.is_empty() {
            return Err(ModCatalogError::NoModsSelected);
        }
        let mut states = BTreeMap::new();
        let mut resolved = Vec::new();
        for id in requested {
            self.visit(id, &mut states, &mut resolved)?;
        }
        let selected: BTreeSet<_> = resolved.iter().cloned().collect();
        let cores: Vec<_> = resolved
            .iter()
            .filter(|id| self.mods.get(*id).is_some_and(|info| info.core))
            .cloned()
            .collect();
        if cores.len() != 1 {
            return Err(ModCatalogError::CoreSelection(cores));
        }
        for id in &resolved {
            let info = self
                .mods
                .get(id)
                .ok_or_else(|| ModCatalogError::UnknownMod(id.clone()))?;
            if let Some(conflict) = info
                .conflicts
                .iter()
                .find(|conflict| selected.contains(*conflict))
            {
                return Err(ModCatalogError::Conflict {
                    left: id.clone(),
                    right: conflict.clone(),
                });
            }
        }
        Ok(resolved)
    }

    fn visit(
        &self,
        id: &str,
        states: &mut BTreeMap<String, VisitState>,
        resolved: &mut Vec<String>,
    ) -> Result<(), ModCatalogError> {
        match states.get(id) {
            Some(VisitState::Complete) => return Ok(()),
            Some(VisitState::Visiting) => {
                return Err(ModCatalogError::DependencyCycle(id.to_owned()));
            }
            None => {}
        }
        let info = self
            .mods
            .get(id)
            .ok_or_else(|| ModCatalogError::UnknownMod(id.to_owned()))?;
        if info.obsolete {
            return Err(ModCatalogError::ObsoleteMod(id.to_owned()));
        }
        states.insert(id.to_owned(), VisitState::Visiting);
        for dependency in &info.dependencies {
            self.visit(dependency, states, resolved)?;
        }
        states.insert(id.to_owned(), VisitState::Complete);
        resolved.push(id.to_owned());
        Ok(())
    }
}

fn append_selected_files(
    manifest: &ContentManifest,
    owner: &str,
    directory: &str,
    seen: &mut BTreeSet<String>,
    files: &mut Vec<SelectedContentFile>,
) {
    let prefix = format!("{directory}/");
    for entry in &manifest.entries {
        if entry.destination.starts_with(&prefix)
            && entry.destination.ends_with(".json")
            && seen.insert(entry.destination.clone())
        {
            files.push(SelectedContentFile {
                owner: owner.to_owned(),
                upstream_path: entry.upstream_path.clone(),
                destination: entry.destination.clone(),
            });
        }
    }
}

impl AmmunitionRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, AmmunitionRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(AmmunitionRegistryError::Catalog)?;
        let content_root = content_root.as_ref();
        let mut types = BTreeMap::new();
        for file in files {
            let bytes = fs::read(content_root.join(&file.destination))
                .map_err(|error| AmmunitionRegistryError::Io(file.destination.clone(), error))?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|error| AmmunitionRegistryError::Json(file.destination.clone(), error))?;
            match value {
                serde_json::Value::Array(values) => {
                    for value in values {
                        load_ammunition_value(&file, value, &mut types)?;
                    }
                }
                value => load_ammunition_value(&file, value, &mut types)?,
            }
        }
        Ok(Self { types })
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&AmmunitionType> {
        self.types.get(id)
    }

    pub fn validate_item_references(
        &self,
        items: &ItemRegistry,
    ) -> Result<(), AmmunitionRegistryError> {
        for ammunition in self.types.values() {
            if items.get(&ammunition.default_item).is_none()
                && !VIRTUAL_AMMUNITION_DEFAULTS
                    .contains(&(ammunition.id.as_str(), ammunition.default_item.as_str()))
            {
                return Err(AmmunitionRegistryError::MissingDefaultItem {
                    ammunition: ammunition.id.clone(),
                    item: ammunition.default_item.clone(),
                });
            }
        }
        Ok(())
    }
}

fn load_ammunition_value(
    file: &SelectedContentFile,
    value: serde_json::Value,
    types: &mut BTreeMap<String, AmmunitionType>,
) -> Result<(), AmmunitionRegistryError> {
    if value.get("type").and_then(serde_json::Value::as_str) != Some("ammunition_type") {
        return Ok(());
    }
    let raw: RawAmmunitionType = serde_json::from_value(value)
        .map_err(|error| AmmunitionRegistryError::Json(file.destination.clone(), error))?;
    if raw.kind != "ammunition_type" {
        return Err(AmmunitionRegistryError::WrongType(raw.kind));
    }
    if raw.id.is_empty() {
        return Err(AmmunitionRegistryError::EmptyId(file.upstream_path.clone()));
    }
    types.insert(
        raw.id.clone(),
        AmmunitionType {
            id: raw.id,
            name: raw.name,
            default_item: raw.default,
            source: file.upstream_path.clone(),
        },
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Complete,
}

fn validate_mod_id(id: &str) -> Result<(), ModCatalogError> {
    if id.is_empty() || id.contains('#') || id.chars().any(char::is_whitespace) {
        return Err(ModCatalogError::InvalidId(id.to_owned()));
    }
    Ok(())
}

fn resolve_mod_path(source: &str, relative: &str) -> Result<String, ModCatalogError> {
    let mut components: Vec<&str> = source.split('/').collect();
    components.pop();
    for component in relative.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components
                    .pop()
                    .ok_or_else(|| ModCatalogError::UnsafeContentPath(relative.to_owned()))?;
            }
            component if component.contains('\\') => {
                return Err(ModCatalogError::UnsafeContentPath(relative.to_owned()));
            }
            component => components.push(component),
        }
    }
    if components.len() < 2 || components[0] != "cdda" || components[1] != "data" {
        return Err(ModCatalogError::UnsafeContentPath(relative.to_owned()));
    }
    Ok(components.join("/"))
}

fn record_definition(
    definitions: &mut BTreeMap<String, DefinitionAccumulator>,
    source: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), SchemaInventoryError> {
    let kind = match object.get("type") {
        Some(serde_json::Value::String(kind)) => kind.as_str(),
        Some(_) => "<invalid-type-field>",
        None => "<untyped>",
    };
    let definition = definitions.entry(kind.to_owned()).or_default();
    definition.objects = definition
        .objects
        .checked_add(1)
        .ok_or(SchemaInventoryError::NumericOverflow)?;
    definition.source_files.insert(source.to_owned());
    for field in object.keys() {
        let occurrences = definition.fields.entry(field.clone()).or_default();
        *occurrences = occurrences
            .checked_add(1)
            .ok_or(SchemaInventoryError::NumericOverflow)?;
    }
    Ok(())
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn update_string(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    DuplicateDestination(String),
    DuplicateUpstreamPath(String),
    NotCanonicalOrder,
    PathOutsideScope(String),
    UnsafeDestination(String),
    UnsafePath(String),
    UnknownLicense(String),
    WrongCommit,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDestination(path) => write!(formatter, "duplicate destination: {path}"),
            Self::DuplicateUpstreamPath(path) => {
                write!(formatter, "duplicate upstream path: {path}")
            }
            Self::NotCanonicalOrder => {
                formatter.write_str("manifest entries are not canonically ordered")
            }
            Self::PathOutsideScope(path) => {
                write!(formatter, "path is outside pinned content scope: {path}")
            }
            Self::UnsafeDestination(path) => {
                write!(
                    formatter,
                    "content destination is not a safe relative path: {path}"
                )
            }
            Self::UnsafePath(path) => {
                write!(
                    formatter,
                    "upstream path is not a safe relative path: {path}"
                )
            }
            Self::UnknownLicense(path) => write!(formatter, "content license is unknown: {path}"),
            Self::WrongCommit => {
                formatter.write_str("manifest does not use the pinned upstream commit")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

#[derive(Debug)]
pub enum ContentLoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Manifest(ManifestError),
    ManifestTooLarge(u64),
}

impl fmt::Display for ContentLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "content manifest I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "content manifest JSON is invalid: {error}"),
            Self::Manifest(error) => write!(formatter, "content manifest is invalid: {error}"),
            Self::ManifestTooLarge(bytes) => {
                write!(
                    formatter,
                    "content manifest exceeds size limit: {bytes} bytes"
                )
            }
        }
    }
}

impl std::error::Error for ContentLoadError {}

#[derive(Debug)]
pub enum ContentVerifyError {
    HashMismatch(String),
    Io(String, std::io::Error),
    Manifest(ManifestError),
    NotRegularFile(String),
}

impl fmt::Display for ContentVerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HashMismatch(path) => write!(formatter, "content hash mismatch: {path}"),
            Self::Io(path, error) => write!(formatter, "content I/O failed for {path}: {error}"),
            Self::Manifest(error) => write!(formatter, "content manifest is invalid: {error}"),
            Self::NotRegularFile(path) => {
                write!(formatter, "content path is not a regular file: {path}")
            }
        }
    }
}

impl std::error::Error for ContentVerifyError {}

#[derive(Debug)]
pub enum SchemaInventoryError {
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    Manifest(ManifestError),
    NumericOverflow,
    Verification(ContentVerifyError),
}

impl fmt::Display for SchemaInventoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, error) => {
                write!(formatter, "schema inventory I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "schema inventory found invalid JSON in {path}: {error}"
                )
            }
            Self::Manifest(error) => write!(formatter, "schema inventory manifest failed: {error}"),
            Self::NumericOverflow => formatter.write_str("schema inventory counter overflowed"),
            Self::Verification(error) => {
                write!(
                    formatter,
                    "schema inventory content verification failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for SchemaInventoryError {}

#[derive(Debug)]
pub enum ModCatalogError {
    Conflict {
        left: String,
        right: String,
    },
    CoreSelection(Vec<String>),
    DependencyCycle(String),
    DuplicateId(String),
    InvalidContentDirectory(String),
    InvalidId(String),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    MissingDependency {
        id: String,
        dependency: String,
    },
    MissingDeveloperDefaults,
    NoModsSelected,
    NonCanonicalLoadOrder {
        expected: Vec<String>,
        actual: Vec<String>,
    },
    ObsoleteMod(String),
    SelfDependency(String),
    UnknownMod(String),
    UnsafeContentPath(String),
    Verification(ContentVerifyError),
    WrongType(String),
}

impl fmt::Display for ModCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict { left, right } => {
                write!(formatter, "selected mods conflict: {left} and {right}")
            }
            Self::CoreSelection(cores) => {
                write!(
                    formatter,
                    "new world must select exactly one core mod: {cores:?}"
                )
            }
            Self::DependencyCycle(id) => write!(formatter, "mod dependency cycle includes {id}"),
            Self::DuplicateId(id) => write!(formatter, "duplicate mod ID: {id}"),
            Self::InvalidContentDirectory(path) => {
                write!(formatter, "mod content path is not a directory: {path}")
            }
            Self::InvalidId(id) => write!(formatter, "invalid mod ID: {id}"),
            Self::Io(path, error) => {
                write!(formatter, "mod catalog I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "mod catalog JSON failed for {path}: {error}")
            }
            Self::MissingDependency { id, dependency } => {
                write!(formatter, "mod {id} has missing dependency {dependency}")
            }
            Self::MissingDeveloperDefaults => {
                formatter.write_str("pinned content has no developer default mod list")
            }
            Self::NoModsSelected => formatter.write_str("new world has no selected mods"),
            Self::NonCanonicalLoadOrder { expected, actual } => write!(
                formatter,
                "mod load order is not canonical (expected {expected:?}, got {actual:?})"
            ),
            Self::ObsoleteMod(id) => write!(formatter, "obsolete mod cannot be selected: {id}"),
            Self::SelfDependency(id) => write!(formatter, "mod depends on itself: {id}"),
            Self::UnknownMod(id) => write!(formatter, "unknown mod ID: {id}"),
            Self::UnsafeContentPath(path) => write!(formatter, "unsafe mod content path: {path}"),
            Self::Verification(error) => {
                write!(formatter, "mod content verification failed: {error}")
            }
            Self::WrongType(kind) => write!(formatter, "mod metadata has wrong type: {kind}"),
        }
    }
}

impl std::error::Error for ModCatalogError {}

#[derive(Debug)]
pub enum AmmunitionRegistryError {
    Catalog(ModCatalogError),
    EmptyId(String),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    MissingDefaultItem { ammunition: String, item: String },
    WrongType(String),
}

impl fmt::Display for AmmunitionRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "ammunition mod selection failed: {error}"),
            Self::EmptyId(path) => write!(formatter, "ammunition type has an empty ID in {path}"),
            Self::Io(path, error) => {
                write!(
                    formatter,
                    "ammunition registry I/O failed for {path}: {error}"
                )
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "ammunition registry JSON failed for {path}: {error}"
                )
            }
            Self::MissingDefaultItem { ammunition, item } => {
                write!(
                    formatter,
                    "ammunition type {ammunition} references missing default ITEM {item}"
                )
            }
            Self::WrongType(kind) => {
                write!(formatter, "ammunition definition has wrong type: {kind}")
            }
        }
    }
}

impl std::error::Error for AmmunitionRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, byte: u8) -> ProvenanceEntry {
        ProvenanceEntry {
            upstream_path: path.to_owned(),
            destination: format!("vendor/{path}"),
            blake3: [byte; 32],
            license: "CC-BY-SA-3.0".to_owned(),
        }
    }

    #[test]
    fn construction_sorts_entries_for_a_stable_hash() {
        let first = ContentManifest::new(vec![
            entry("data/json/items.json", 1),
            entry("data/core/core.json", 2),
        ])
        .expect("valid manifest");
        let second = ContentManifest::new(vec![
            entry("data/core/core.json", 2),
            entry("data/json/items.json", 1),
        ])
        .expect("valid manifest");
        assert_eq!(
            first.canonical_hash().expect("first hash"),
            second.canonical_hash().expect("second hash")
        );
    }

    #[test]
    fn unknown_provenance_is_rejected() {
        let mut unknown = entry("data/json/items.json", 1);
        unknown.license.clear();
        assert!(matches!(
            ContentManifest::new(vec![unknown]),
            Err(ManifestError::UnknownLicense(_))
        ));
    }

    #[test]
    fn unsafe_manifest_paths_are_rejected() {
        let mut unsafe_entry = entry("data/json/items.json", 1);
        unsafe_entry.destination = String::from("../escape.json");
        assert!(matches!(
            ContentManifest::new(vec![unsafe_entry]),
            Err(ManifestError::UnsafeDestination(_))
        ));
    }

    fn mod_info(id: &str, dependencies: &[&str], core: bool) -> ModInfo {
        ModInfo {
            id: id.to_owned(),
            name: id.to_owned(),
            description: String::new(),
            authors: BTreeSet::new(),
            maintainers: BTreeSet::new(),
            category: String::new(),
            dependencies: dependencies
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            conflicts: Vec::new(),
            core,
            obsolete: false,
            content_directory: String::from("cdda/data/json"),
            loading_images: BTreeSet::new(),
            disable_other_loading_screens: false,
            version: String::new(),
            source: String::from("data/mods/test/modinfo.json"),
        }
    }

    #[test]
    fn mod_dependencies_resolve_before_dependents() {
        let catalog = ModCatalog {
            mods: BTreeMap::from([
                (String::from("dda"), mod_info("dda", &[], true)),
                (
                    String::from("library"),
                    mod_info("library", &["dda"], false),
                ),
                (
                    String::from("feature"),
                    mod_info("feature", &["library"], false),
                ),
            ]),
        };
        assert_eq!(
            catalog
                .resolve_new_world(&[String::from("feature")])
                .expect("selection should resolve"),
            ["dda", "library", "feature"]
        );
    }

    #[test]
    fn mod_conflicts_and_obsolete_mods_fail_closed() {
        let mut first = mod_info("first", &["dda"], false);
        first.conflicts.push(String::from("second"));
        let mut obsolete = mod_info("old", &["dda"], false);
        obsolete.obsolete = true;
        let catalog = ModCatalog {
            mods: BTreeMap::from([
                (String::from("dda"), mod_info("dda", &[], true)),
                (String::from("first"), first),
                (String::from("second"), mod_info("second", &["dda"], false)),
                (String::from("old"), obsolete),
            ]),
        };
        assert!(matches!(
            catalog.resolve_new_world(&[String::from("first"), String::from("second")]),
            Err(ModCatalogError::Conflict { .. })
        ));
        assert!(matches!(
            catalog.resolve_new_world(&[String::from("old")]),
            Err(ModCatalogError::ObsoleteMod(_))
        ));
    }

    #[test]
    fn pinned_package_loads_default_mods_and_ammunition() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest should load");
        let root = manifest_path
            .parent()
            .expect("manifest should have a parent");
        let catalog = ModCatalog::load(&manifest, root).expect("mod catalog should load");
        assert_eq!(catalog.len(), 47);
        let enabled = catalog
            .recommended_new_world()
            .expect("developer defaults should resolve");
        assert_eq!(enabled, ["dda", "no_npc_food", "personal_portal_storms"]);
        let ammunition = AmmunitionRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("ammunition should load");
        assert_eq!(ammunition.len(), 158);
        let rimfire = ammunition.get("22").expect(".22 ammunition should exist");
        assert_eq!(rimfire.name, ".22 LR");
        assert_eq!(rimfire.default_item, "22_lr");
        assert_eq!(rimfire.source, "data/json/items/ammo_types.json");

        let items = ItemRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected ITEM definitions should finalize");
        let item_groups = ItemGroupRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected named item groups should normalize");
        assert!(item_groups.get("field").is_some());
        let overmap_terrain =
            OvermapTerrainRegistry::load_selected(&manifest, root, &catalog, &enabled)
                .expect("selected overmap-terrain identities should finalize");
        assert_eq!(
            overmap_terrain
                .get_identity("field")
                .expect("non-rotating field identity")
                .generator_id,
            "field"
        );
        assert_eq!(
            overmap_terrain
                .get_identity("lmoe_north")
                .expect("rotatable LMOE identity")
                .rotation,
            0
        );
        let road_ew = overmap_terrain
            .get_identity("road_ew")
            .expect("linear east-west road identity");
        assert_eq!(road_ew.subtype_id, "road_straight");
        assert_eq!(road_ew.rotation, 3);
        ammunition
            .validate_item_references(&items)
            .expect("ammunition defaults should resolve to concrete items");
        assert_eq!(items.len(), 10_282);
        assert_eq!(items.abstract_count(), 179);
        let rock = items.get("rock").expect("rock item should exist");
        assert_eq!(rock.name, "rock");
        assert_eq!(rock.weight_milligrams, 657_000);
        assert_eq!(rock.volume_milliliters, 250);
        assert_eq!(rock.melee_damage.get("bash"), Some(&7.0));
        assert!(rock.subtypes.contains("AMMO"));
        assert!(rock.materials.contains_key("stone"));
        let hammer = items.get("hammer").expect("hammer should exist");
        assert_eq!(hammer.qualities["HAMMER"].level, 3);
        assert_eq!(hammer.qualities["HAMMER"].speed, 1.0);
        assert_eq!(hammer.melee_to_hit(), -1);
        let pot = items.get("pot").expect("pot should exist");
        assert_eq!(pot.qualities["COOK"].level, 3);
        assert_eq!(pot.qualities["BOIL"].level, 2);
        let chemistry_set = items
            .get("chemistry_set")
            .expect("chemistry set should exist");
        assert_eq!(chemistry_set.tool_subtype, "hotplate");
        assert_eq!(
            items.tool_subtype_replacements("hotplate"),
            vec![
                "hotplate",
                "chemistry_set",
                "fake_fireplace",
                "fake_stove",
                "fake_woodstove",
            ]
        );
        let water = items.get("water_clean").expect("clean water should exist");
        assert_eq!(water.comestible_type, "DRINK");
        assert_eq!(water.charges, 1);
        assert_eq!(water.default_charges(), 1);
        assert_eq!(water.calories, 0);
        assert_eq!(water.quench, 50);
        let cooked_meat = items.get("meat_cooked").expect("cooked meat should exist");
        assert_eq!(cooked_meat.comestible_type, "FOOD");
        assert_eq!(cooked_meat.calories, 402);
        let glock = items.get("glock_19").expect("starter pistol should exist");
        assert!(glock.subtypes.contains("GUN"));
        assert_eq!(glock.ammo, BTreeSet::from([String::from("9mm")]));
        assert_eq!(glock.dispersion, 480);
        let cartridge = items.get("9mm").expect("starter cartridge should exist");
        assert!(cartridge.subtypes.contains("AMMO"));
        assert_eq!(cartridge.ammo_types, BTreeSet::from([String::from("9mm")]));
        assert_eq!(cartridge.count, 50);
        assert_eq!(cartridge.default_charges(), 50);
        assert_eq!(cartridge.range, 14);
        assert_eq!(cartridge.dispersion, 60);
        assert_eq!(
            cartridge
                .damage
                .get("bullet")
                .expect("9mm should deal bullet damage")
                .amount,
            23.0
        );
        assert!(cartridge.unsupported_fields.contains("damage.barrels"));
        let revolver = items
            .get("model_10_revolver")
            .expect("starter revolver should exist");
        assert_eq!(revolver.ammo, BTreeSet::from([String::from("38")]));
        assert_eq!(revolver.clip_size, 6);
        let revolver_ammo = items
            .get("38_special")
            .expect("starter revolver ammunition should exist");
        assert_eq!(
            revolver_ammo
                .damage
                .get("bullet")
                .expect(".38 should deal bullet damage")
                .amount,
            21.0
        );
        let pistol_manual = items
            .get("manual_pistol")
            .expect("starter pistol manual should exist");
        assert!(pistol_manual.subtypes.contains("BOOK"));
        assert_eq!(pistol_manual.book_skill, "pistol");
        assert_eq!(pistol_manual.book_required_level, 0);
        assert_eq!(pistol_manual.book_max_level, 3);
        assert_eq!(pistol_manual.book_intelligence, 3);
        assert_eq!(pistol_manual.book_time_moves, 15 * 60 * 100);
        let detachable_battery_tools = items
            .iter()
            .filter_map(|(item_id, item)| {
                if !item.subtypes.contains("TOOL")
                    || item.subtypes.contains("GUN")
                    || item.tool_ammunition != BTreeSet::from([String::from("battery")])
                    || item.magazine_wells.len() != 1
                    || !item.integral_magazines.is_empty()
                {
                    return None;
                }
                let compatible = items.compatible_magazines(&item.magazine_wells[0]);
                (!compatible.is_empty()
                    && compatible.iter().all(|magazine_id| {
                        items.get(magazine_id).is_some_and(|magazine| {
                            magazine.magazine_capacity > 0
                                && magazine.ammo_types == BTreeSet::from([String::from("battery")])
                        })
                    }))
                .then_some((
                    item_id,
                    item.magazine_wells[0].default_magazine.as_str(),
                    compatible,
                ))
            })
            .collect::<Vec<_>>();
        assert_eq!(detachable_battery_tools.len(), 160);
        assert!(
            detachable_battery_tools.iter().any(|entry| {
                entry.0 == "flashlight"
                    && entry.1 == "medium_battery_cell"
                    && entry.2 == ["medium_battery_cell"]
            }),
            "the pinned flashlight has one exact medium-battery well"
        );
        let flashlight = items.get("flashlight").expect("pinned flashlight exists");
        assert_eq!(flashlight.charges_per_use, 1);
        assert_eq!(
            flashlight.transform_actions,
            [item::ItemTransformActionDefinition {
                target: String::from("flashlight_on"),
                need_charges: 1,
                ammo_scale: 1,
                moves: 0,
            }]
        );
        let flashlight_on = items
            .get("flashlight_on")
            .expect("pinned active flashlight exists");
        assert_eq!(flashlight_on.power_draw_milliwatts, 1_560);
        assert_eq!(flashlight_on.light_emission, 300);
        assert_eq!(flashlight_on.revert_to, "flashlight");
        assert_eq!(
            flashlight_on.transform_actions,
            [item::ItemTransformActionDefinition {
                target: String::from("flashlight"),
                need_charges: 0,
                ammo_scale: 0,
                moves: 0,
            }]
        );
        let medium_battery = items
            .get("medium_battery_cell")
            .expect("pinned medium battery exists");
        assert_eq!(medium_battery.magazine_capacity, 56);
        assert_eq!(
            medium_battery.integral_magazines,
            [BTreeMap::from([(String::from("battery"), 56)])]
        );

        let monsters = MonsterRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected MONSTER definitions should finalize");
        assert_eq!(monsters.len(), 1_177);
        assert_eq!(monsters.abstract_count(), 33);
        let zombie = monsters
            .get("mon_zombie")
            .expect("classic zombie should exist");
        assert_eq!(zombie.name, "zombie");
        assert_eq!(zombie.hp, 80);
        assert_eq!(zombie.speed, 70);
        assert_eq!(zombie.melee_skill, 4);
        assert_eq!(zombie.dodge, 0);
        assert_eq!(zombie.volume_milliliters, 62_500);
        assert_eq!(zombie.melee_dice, 2);
        assert_eq!(zombie.melee_dice_sides, 3);
        assert!(zombie.flags.contains("BASHES"));
        assert!(zombie.flags.contains("GROUP_BASH"));
        assert!(zombie.flags.contains("HEARS"));
        assert!(zombie.flags.contains("CLUMSY_ATTACKS"));

        let bash_profiles =
            BashDamageProfileRegistry::load_selected(&manifest, root, &catalog, &enabled)
                .expect("selected bash damage profiles should load");
        assert_eq!(bash_profiles.len(), 2);
        assert_eq!(
            bash_profiles
                .get("wooden_door")
                .expect("wooden-door bash profile")
                .multipliers_millionths,
            BTreeMap::from([
                (String::from("bash"), 950_000),
                (String::from("cut"), 1_200_000),
            ])
        );

        let terrain = TerrainRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected terrain definitions should finalize");
        assert_eq!(terrain.len(), 1_246);
        assert_eq!(terrain.abstract_count(), 23);
        let floor = terrain.get("t_floor").expect("floor terrain should exist");
        assert!(floor.is_passable());
        let door = terrain
            .get("t_door_c")
            .expect("closed door terrain should exist");
        assert!(!door.is_passable());
        assert_eq!(door.open, "t_door_o");
        let door_bash = door.bash.as_ref().expect("closed door is bashable");
        assert!(door_bash.is_fully_supported());
        assert_eq!((door_bash.str_min, door_bash.str_max), (8, 80));
        assert_eq!(
            (door_bash.str_min_blocked, door_bash.str_max_blocked),
            (15, 100)
        );
        assert_eq!(door_bash.profile, "wooden_door");
        assert_eq!(door_bash.terrain_result, "t_door_b");
        assert!(matches!(
            &door_bash.item_group,
            Some(BashItemGroupSource::InlineCollection(entries)) if entries.len() == 4
        ));
        assert_eq!(
            door_bash.hit_field,
            Some(BashFieldEffectDefinition {
                field_type_id: String::from("fd_dust"),
                intensity: 2,
            })
        );
        assert_eq!(
            door_bash.destroyed_field,
            Some(BashFieldEffectDefinition {
                field_type_id: String::from("fd_splinters"),
                intensity: 1,
            })
        );
        let damaged_door_bash = terrain
            .get("t_door_b")
            .and_then(|terrain| terrain.bash.as_ref())
            .expect("damaged door is bashable");
        assert!(damaged_door_bash.is_fully_supported());
        assert_eq!(damaged_door_bash.terrain_result, "t_door_frame");
        let frame_bash = terrain
            .get("t_door_frame")
            .and_then(|terrain| terrain.bash.as_ref())
            .expect("door frame is bashable");
        assert!(frame_bash.is_fully_supported());
        assert_eq!(frame_bash.profile, "default");
        assert_eq!(frame_bash.terrain_result, "t_null");

        let furniture = FurnitureRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected furniture definitions should finalize");
        assert_eq!(furniture.len(), 699);
        assert_eq!(furniture.abstract_count(), 1);
        let bed = furniture.get("f_bed").expect("standard bed should exist");
        assert!(bed.is_passable());
        assert!(bed.is_transparent());
        assert_eq!(bed.move_cost_mod, 3);
        assert_eq!(bed.comfort, 5);
        assert_eq!(bed.floor_bedding_warmth, 1_000);
        let dresser = furniture
            .get("f_dresser")
            .expect("standard dresser should exist");
        assert!(!dresser.is_passable());

        let regional = DefaultRegionTerrainFurnitureRegistry::load_selected(
            &manifest, root, &catalog, &enabled, &terrain, &furniture,
        )
        .expect("default regional pseudo substitutions should finalize");
        assert_eq!(regional.terrain_len(), 24);
        assert_eq!(regional.furniture_len(), 10);
        let groundcover = regional
            .terrain_table("t_region_groundcover")
            .expect("default regional groundcover table");
        assert_eq!(groundcover.choices[0].id, "t_grass");
        assert_eq!(groundcover.choices[0].weight, 12_000);
        let shrub = regional
            .terrain_table("t_region_shrub_forest")
            .expect("recursive regional shrub table");
        assert_eq!(shrub.choices[0].id, "t_region_shrub_forest_dense");
        assert!(regional.terrain_table(&shrub.choices[0].id).is_some());

        let mapgen = MapgenRegistry::load_selected(
            &manifest,
            root,
            &catalog,
            &enabled,
            &terrain,
            &furniture,
            &item_groups,
        )
        .expect("selected ordinary mapgen definitions should be inventoried");
        let rock_border = mapgen
            .get("rock_border")
            .and_then(|definitions| definitions.first())
            .expect("strict rock-border mapgen should compile");
        assert_eq!(rock_border.weight, DEFAULT_MAPGEN_WEIGHT);
        assert_eq!(
            rock_border.fill_terrain,
            Some(MapgenIdChoice::Fixed(String::from("t_rock")))
        );
        assert_eq!(rock_border.cells.len(), MAPGEN_WIDTH * MAPGEN_HEIGHT);

        let billboard = mapgen
            .get("billboard_1")
            .and_then(|definitions| definitions.first())
            .expect("strict Unicode billboard mapgen should compile");
        assert_eq!(billboard.weight, DEFAULT_MAPGEN_WEIGHT);
        assert_eq!(billboard.cell(18, 12), Some("."));
        assert_eq!(billboard.cell(19, 12), Some("≷"));
        assert_eq!(
            billboard.terrain.get("≷"),
            Some(&vec![MapgenIdChoice::Fixed(String::from(
                "t_ladder_up_down"
            ))])
        );

        let field = mapgen
            .get("field")
            .and_then(|definitions| definitions.first())
            .expect("pinned field mapgen should now compile strictly");
        assert_eq!(
            field.items.get("3"),
            Some(&StrictMapgenItemPlacement {
                item_group: String::from("field"),
                chance: 1,
                repeat_minimum: 1,
                repeat_maximum: 1,
            })
        );
        assert!(mapgen.unavailable_reports("field").is_none());
        assert!(mapgen.reports().any(|report| {
            report.is_available() && report.om_terrains.iter().any(|id| id == "field")
        }));

        let skills = SkillRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected skill definitions should load");
        assert_eq!(skills.len(), 28);
        let fabrication = skills
            .get("fabrication")
            .expect("fabrication skill should exist");
        assert_eq!(fabrication.name, "fabrication");
        assert_eq!(fabrication.display_category, "display_crafting");
        assert_eq!(fabrication.sort_rank, 17_000);
        assert!(fabrication.consumes_focus);
        assert!(!fabrication.tags.contains("contextual_skill"));
        let proficiencies = ProficiencyRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("selected proficiency definitions should load");
        assert_eq!(proficiencies.len(), 234);
        let metalworking = proficiencies
            .get("prof_metalworking")
            .expect("metalworking proficiency should exist");
        assert_eq!(metalworking.default_time_multiplier_millionths, 1_500_000);
        assert_eq!(metalworking.time_to_learn_moves, 720_000);
        let recipes = RecipeRegistry::load_selected(
            &manifest,
            root,
            &catalog,
            &enabled,
            &items,
            &skills,
            &proficiencies,
        )
        .expect("selected recipe and requirement definitions should finalize");
        assert!(recipes.len() > 5_000, "loaded {} recipes", recipes.len());
        assert_eq!(recipes.uncraft_count(), 1_428);
        assert_eq!(recipes.uncraft_abstract_count(), 1);
        assert_eq!(recipes.requirement_count(), 474);
        assert_eq!(recipes.available().count(), 1_990);
        assert_eq!(recipes.craftable_with_knowledge_source().count(), 3_049);
        let non_second_aligned = recipes
            .available()
            .filter(|recipe| recipe.time_moves % 100 != 0)
            .map(|recipe| (recipe.id.as_str(), recipe.time_moves))
            .collect::<Vec<_>>();
        assert!(
            non_second_aligned.is_empty(),
            "supported recipes must award an integral number of pinned practice ticks: {non_second_aligned:?}"
        );
        let runnable_flags = recipes
            .available()
            .flat_map(|recipe| recipe.flags.iter().cloned())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            runnable_flags,
            BTreeSet::from([String::from("ALLOW_ROTTEN"), String::from("BLIND_EASY"),])
        );
        let pointy_stick = recipes
            .get("pointy_stick")
            .expect("pointy-stick recipe should exist");
        assert_eq!(pointy_stick.time_moves, 4_800);
        assert_eq!(pointy_stick.components[0].len(), 4);
        assert_eq!(pointy_stick.skill_used, "fabrication");
        assert_eq!(pointy_stick.difficulty, 0);
        assert_eq!(pointy_stick.resolved_autolearn_skills()["fabrication"], 0);
        assert!(pointy_stick.unavailable_reasons.is_empty());
        let vegetable_juice = recipes
            .get("V8")
            .expect("vegetable-juice recipe should exist");
        assert_eq!(
            vegetable_juice
                .components
                .iter()
                .flatten()
                .filter(|component| component.requirement_list)
                .count(),
            2
        );
        let resolved_vegetables = recipes
            .resolved_components(vegetable_juice)
            .expect("pinned LIST components should resolve");
        assert_eq!(
            resolved_vegetables[1]
                .iter()
                .map(|component| (component.type_id.as_str(), component.count))
                .collect::<Vec<_>>(),
            vec![("tomato", 1), ("tomato_cut", 1), ("can_tomato", 1)]
        );
        assert_eq!(
            resolved_vegetables[3]
                .iter()
                .map(|component| (component.type_id.as_str(), component.count))
                .collect::<Vec<_>>(),
            vec![("zucchini", 1), ("zucchini_cut", 1)]
        );
        assert!(vegetable_juice.unavailable_reasons.is_empty());
        let makeshift_cards = recipes
            .get("deck_of_cards_deck_of_cards_makeshift")
            .expect("makeshift-card recipe should exist");
        assert!(makeshift_cards.tools[0][0].requirement_list);
        assert_eq!(
            recipes
                .resolved_tools(makeshift_cards)
                .expect("pinned tool LIST should resolve")[0]
                .iter()
                .map(|tool| (tool.type_id.as_str(), tool.count))
                .collect::<Vec<_>>(),
            vec![
                ("pen", 5),
                ("black_pen", 5),
                ("blue_pen", 5),
                ("green_pen", 5),
                ("red_pen", 5),
                ("pencil", 5),
                ("permanent_marker", 5),
                ("survival_marker", 5),
            ]
        );
        assert!(makeshift_cards.unavailable_reasons.is_empty());
        let sawn_lumber = recipes
            .get("2x4_from logs")
            .expect("sawn-lumber recipe should exist");
        assert_eq!(
            sawn_lumber.byproducts,
            BTreeMap::from([(String::from("splinter"), 10)])
        );
        assert!(sawn_lumber.unavailable_reasons.is_empty());
        let designated_oats = recipes
            .get("seed_oats")
            .expect("pinned inherited batch-factor recipe should exist");
        assert_eq!(
            designated_oats.batch_time_factors,
            Some(recipe::BatchTimeFactors::Logistic {
                percent_millionths: 99_000_000,
                target_batch_size: 1,
            })
        );
        assert!(designated_oats.unavailable_reasons.is_empty());
        let acid_chitin_cow = recipes
            .get("acidchitin_armor_cow")
            .expect("pinned inherited autolearn-plus-book recipe should exist");
        assert_eq!(
            acid_chitin_cow
                .book_learn
                .iter()
                .map(|(book_id, metadata)| (book_id.as_str(), metadata.skill_level))
                .collect::<Vec<_>>(),
            vec![("textbook_arthropod", 7)]
        );
        assert!(acid_chitin_cow.autolearn);
        assert!(acid_chitin_cow.unavailable_reasons.is_empty());
        let cottage_cheese = recipes
            .get("cottage_cheese")
            .expect("pinned book-only recipe should exist");
        assert!(!cottage_cheese.autolearn);
        assert!(!cottage_cheese.book_learn.is_empty());
        assert!(
            cottage_cheese
                .unavailable_reasons
                .contains("not_autolearned"),
            "book-only recipes must remain outside the autolearn catalog"
        );
        assert!(
            recipes
                .craftable_with_knowledge_source()
                .any(|recipe| recipe.id == "36navy")
        );
        let arthropod_textbook = items
            .get("textbook_arthropod")
            .expect("pinned arthropod textbook should exist");
        assert_eq!(arthropod_textbook.book_required_level, 3);
        let flashlight = recipes
            .get("flashlight")
            .expect("pinned disassembly-learning recipe should exist");
        assert_eq!(
            flashlight.learn_by_disassembly,
            BTreeMap::from([(String::from("electronics"), 0)])
        );
        assert!(
            flashlight
                .unavailable_reasons
                .iter()
                .all(|reason| !reason.contains("decomp_learn")),
            "retained disassembly metadata must not remain an unsupported field"
        );
        let case_hardened_sheet = recipes
            .get("ch_sheet_metal_small")
            .expect("pinned extend.using recipe should exist");
        assert_eq!(
            case_hardened_sheet
                .using
                .iter()
                .map(|requirement| (requirement.requirement_id.as_str(), requirement.multiplier,))
                .collect::<Vec<_>>(),
            vec![("blacksmithing_standard", 4), ("carbon", 1)]
        );
        let case_hardened_components = recipes
            .resolved_components(case_hardened_sheet)
            .expect("extended carbon requirement should normalize");
        assert!(case_hardened_components.iter().any(|group| {
            group
                .iter()
                .any(|component| component.type_id == "charcoal" && component.count == 25)
        }));
        assert!(case_hardened_sheet.unavailable_reasons.is_empty());
        let crude_lamp_oil = recipes
            .get("crude_lamp_oil")
            .expect("pinned ALLOW_ROTTEN recipe should exist");
        assert!(crude_lamp_oil.flags.contains("ALLOW_ROTTEN"));
        assert!(
            crude_lamp_oil
                .unavailable_reasons
                .iter()
                .all(|reason| reason != "recipe_flag_semantics"),
            "ALLOW_ROTTEN is exact while canonical items cannot carry rot state"
        );
        assert_eq!(
            recipes
                .resolved_qualities(pointy_stick)
                .expect("qualities")
                .len(),
            1
        );
        let rock_sock = recipes
            .get("rock_sock")
            .expect("rock-in-a-sock recipe should exist");
        assert!(rock_sock.unavailable_reasons.is_empty());
    }
}
