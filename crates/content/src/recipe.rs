use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::{
    AmmunitionRegistry, ContentManifest, ItemRegistry, ItemTemperatureRuntimeClass, ModCatalog,
    ModCatalogError, PROFICIENCY_MULTIPLIER_SCALE, ProficiencyRegistry, SelectedContentFile,
    SkillRegistry,
};

const RUNTIME_MAX_SUPPORT_GROUPS: usize = 128;
const RUNTIME_MAX_SUPPORT_ALTERNATIVES: usize = 128;
const RUNTIME_MAX_COMPONENT_GROUPS: usize = 128;
const RUNTIME_MAX_COMPONENT_ALTERNATIVES: usize = 128;
const RUNTIME_MAX_QUALITY_PROVIDERS: usize = 512;

const RECIPE_IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "result",
    "id_suffix",
    "variant",
    "name",
    "description",
    "category",
    "subcategory",
    "time",
    "activity_level",
    "batch_time_factors",
    "skill_used",
    "difficulty",
    "skills_required",
    "autolearn",
    "decomp_learn",
    "book_learn",
    "never_learn",
    "obsolete",
    "components",
    "tools",
    "qualities",
    "proficiencies",
    "using",
    "charges",
    "result_mult",
    "byproducts",
    "reversible",
    "flags",
    "extend",
];

const REQUIREMENT_IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "name",
    "components",
    "tools",
    "qualities",
    "extend",
];

pub(crate) fn recipe_field_is_implemented(field: &str) -> bool {
    RECIPE_IMPLEMENTED_FIELDS.contains(&field)
}

pub(crate) fn requirement_field_is_implemented(field: &str) -> bool {
    REQUIREMENT_IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentRequirement {
    pub type_id: String,
    pub count: u32,
    pub requirement_list: bool,
    pub recoverable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolRequirement {
    pub type_id: String,
    pub count: i32,
    pub requirement_list: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityRequirement {
    pub quality_id: String,
    pub level: i32,
    pub amount: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalRequirement {
    pub requirement_id: String,
    pub multiplier: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BatchTimeFactors {
    Logistic {
        percent_millionths: u32,
        target_batch_size: u32,
    },
    Linear {
        setup_moves: u64,
        maximum_batch_size: Option<u32>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookLearnDefinition {
    pub skill_level: i32,
    pub recipe_name: Option<String>,
    pub hidden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipeProficiency {
    pub proficiency_id: String,
    pub required: bool,
    /// `None` uses the referenced proficiency's default time multiplier.
    pub time_multiplier_millionths: Option<u32>,
    /// `None` uses the referenced proficiency's default skill penalty.
    pub skill_penalty_millionths: Option<i32>,
    pub learning_time_multiplier_millionths: u32,
    pub max_experience_moves: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequirementDefinition {
    pub id: String,
    pub name: String,
    pub components: Vec<Vec<ComponentRequirement>>,
    pub tools: Vec<Vec<ToolRequirement>>,
    pub qualities: Vec<Vec<QualityRequirement>>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecipeDefinition {
    pub id: String,
    pub result: String,
    pub variant: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub subcategory: String,
    /// Pinned CDDA action moves (100 moves per second).
    pub time_moves: u64,
    /// Retained exactly for future batch commands. For the currently supported
    /// batch size of one, both pinned formulas equal `time_moves`.
    pub batch_time_factors: Option<BatchTimeFactors>,
    pub activity_level: String,
    pub skill_used: String,
    pub difficulty: u8,
    pub skills_required: BTreeMap<String, u8>,
    pub autolearn: bool,
    pub autolearn_skills: BTreeMap<String, u8>,
    /// Skill levels required to learn this recipe from disassembly. Retained
    /// for authoritative one-in-four learning after successful disassembly.
    pub learn_by_disassembly: BTreeMap<String, i32>,
    pub book_learn: BTreeMap<String, BookLearnDefinition>,
    pub never_learn: bool,
    pub obsolete: bool,
    pub charges: Option<u32>,
    pub result_mult: u32,
    pub components: Vec<Vec<ComponentRequirement>>,
    pub tools: Vec<Vec<ToolRequirement>>,
    pub qualities: Vec<Vec<QualityRequirement>>,
    pub proficiencies: Vec<RecipeProficiency>,
    pub using: Vec<ExternalRequirement>,
    pub byproducts: BTreeMap<String, u32>,
    pub reversible: bool,
    /// Pinned object-form `reversible.time`; zero reuses ordinary craft time.
    pub uncraft_time_moves: u64,
    pub flags: BTreeSet<String>,
    pub unsupported_fields: BTreeSet<String>,
    pub unavailable_reasons: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecipeRegistry {
    recipes: BTreeMap<String, RecipeDefinition>,
    /// Explicit pinned `uncraft` definitions, keyed by target item type. These
    /// live in a separate upstream dictionary and override reversible crafts.
    uncrafts: BTreeMap<String, RecipeDefinition>,
    requirements: BTreeMap<String, RequirementDefinition>,
    tool_subtype_replacements: BTreeMap<String, Vec<String>>,
    abstract_count: usize,
    uncraft_abstract_count: usize,
}

#[derive(Clone)]
struct RawRecipe {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

struct RawDefinitions {
    requirements: Vec<RawRecipe>,
    recipes: VecDeque<RawRecipe>,
    uncrafts: VecDeque<RawRecipe>,
}

impl RecipeRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
        items: &ItemRegistry,
        skills: &SkillRegistry,
        proficiencies: &ProficiencyRegistry,
    ) -> Result<Self, RecipeRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(RecipeRegistryError::Catalog)?;
        let RawDefinitions {
            requirements: raw_requirements,
            recipes: mut pending,
            uncrafts: mut pending_uncrafts,
        } = read_definitions(content_root.as_ref(), files)?;
        let requirements = load_requirements(raw_requirements)?;
        let tool_subtype_replacements = items.tool_subtype_replacement_map().clone();
        let mut recipes = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0_usize;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(RecipeRegistryError::InternalQueue)?;
                if load_one_recipe(&raw, &mut recipes, &mut abstracts)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(RecipeRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .map(|raw| raw_recipe_label(&raw.object))
                        .collect(),
                ));
            }
        }
        let mut uncrafts = BTreeMap::new();
        let mut uncraft_abstracts = BTreeMap::new();
        while !pending_uncrafts.is_empty() {
            let pass_size = pending_uncrafts.len();
            let mut loaded = 0_usize;
            for _ in 0..pass_size {
                let raw = pending_uncrafts
                    .pop_front()
                    .ok_or(RecipeRegistryError::InternalQueue)?;
                if load_one_recipe(&raw, &mut uncrafts, &mut uncraft_abstracts)? {
                    loaded += 1;
                } else {
                    pending_uncrafts.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(RecipeRegistryError::UnresolvedInheritance(
                    pending_uncrafts
                        .iter()
                        .take(20)
                        .map(|raw| raw_recipe_label(&raw.object))
                        .collect(),
                ));
            }
        }
        let quality_providers = supported_quality_providers(items);
        for recipe in recipes.values_mut() {
            finalize_availability(
                recipe,
                &requirements,
                items,
                skills,
                proficiencies,
                &quality_providers,
                &tool_subtype_replacements,
            )?;
        }
        for recipe in uncrafts.values_mut() {
            finalize_availability(
                recipe,
                &requirements,
                items,
                skills,
                proficiencies,
                &quality_providers,
                &tool_subtype_replacements,
            )?;
            // Explicit uncraft definitions are actions, not player-known craft
            // recipes, so they do not require a crafting knowledge source.
            recipe.unavailable_reasons.remove("not_autolearned");
        }
        Ok(Self {
            recipes,
            uncrafts,
            requirements,
            tool_subtype_replacements,
            abstract_count: abstracts.len(),
            uncraft_abstract_count: uncraft_abstracts.len(),
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    #[must_use]
    pub const fn abstract_count(&self) -> usize {
        self.abstract_count
    }

    #[must_use]
    pub const fn uncraft_abstract_count(&self) -> usize {
        self.uncraft_abstract_count
    }

    #[must_use]
    pub fn uncraft_count(&self) -> usize {
        self.uncrafts.len()
    }

    #[must_use]
    pub fn requirement_count(&self) -> usize {
        self.requirements.len()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&RecipeDefinition> {
        self.recipes.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &RecipeDefinition)> {
        self.recipes
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    pub fn available(&self) -> impl Iterator<Item = &RecipeDefinition> {
        self.recipes
            .values()
            .filter(|recipe| recipe.unavailable_reasons.is_empty())
    }

    /// Recipes whose complete implemented runtime semantics have at least one
    /// supported knowledge source. The sole `not_autolearned` reason is
    /// removable by an identified carried BOOK or authoritative permanent
    /// learning through disassembly; all other reasons remain fail-closed.
    pub fn craftable_with_knowledge_source(&self) -> impl Iterator<Item = &RecipeDefinition> {
        self.recipes.values().filter(|recipe| {
            recipe.unavailable_reasons.is_empty()
                || ((!recipe.book_learn.is_empty()
                    || (!recipe.never_learn && !recipe.learn_by_disassembly.is_empty()))
                    && recipe.unavailable_reasons.len() == 1
                    && recipe.unavailable_reasons.contains("not_autolearned"))
        })
    }

    /// Returns the first recipe-ID-ordered reversible definition that the
    /// strict disassembly slice can represent exactly. An ordinary item uses
    /// the first component alternative from each group, matching pinned
    /// `get_uncraft_components`; a crafted item's retained provenance replaces
    /// those defaults at authoritative admission.
    #[must_use]
    pub fn strict_disassembly_recipe_for_result<'a>(
        &'a self,
        result_type_id: &str,
        items: &ItemRegistry,
        ammunition: &AmmunitionRegistry,
    ) -> Option<&'a RecipeDefinition> {
        const SPECIAL_TOOL_SUBSTITUTIONS: [&str; 8] = [
            "welder",
            "welder_crude",
            "oxy_torch",
            "forge",
            "char_forge",
            "crucible",
            "press",
            "fire",
        ];
        const SPECIAL_QUALITY_SUBSTITUTIONS: [&str; 3] = ["SEW", "GLARE", "KNIT"];
        const MAX_RECOVERED_INSTANCES: u32 = 256;

        let supported = |recipe: &RecipeDefinition| {
            let Some(result) = items.get(&recipe.result) else {
                return false;
            };
            if !matches!(
                result.temperature_runtime_class(),
                ItemTemperatureRuntimeClass::NotTracked
                    | ItemTemperatureRuntimeClass::MateriallessNonperishable
            ) {
                return false;
            }
            let charge_carrier_is_supported = |ammunition_type: &str| {
                ammunition
                    .get(ammunition_type)
                    .and_then(|ammunition| items.get(&ammunition.default_item))
                    .is_some_and(|default_item| {
                        matches!(
                            default_item.temperature_runtime_class(),
                            ItemTemperatureRuntimeClass::NotTracked
                                | ItemTemperatureRuntimeClass::MateriallessNonperishable
                        ) && default_item.count_by_charges()
                            && default_item.default_charges() > 0
                            && default_item.ammo_types.len() == 1
                            && default_item.ammo_types.contains(ammunition_type)
                    })
            };
            let gun_ammunition_is_supported = if result.subtypes.contains("GUN") {
                if result.flags.contains("NO_UNLOAD")
                    || result.ammo.len() != 1
                    || result.ammo_types.len() > 1
                {
                    false
                } else {
                    let ammunition_type = result
                        .ammo
                        .first()
                        .expect("a singleton ammunition set has a first entry");
                    charge_carrier_is_supported(ammunition_type)
                }
            } else {
                true
            };
            let powered_tool = !result.subtypes.contains("GUN")
                && result.subtypes.contains("TOOL")
                && !result.tool_ammunition.is_empty();
            let integral_tool_charges_are_supported = if powered_tool
                && result.default_charges() > 0
            {
                if result.flags.contains("NO_UNLOAD")
                    || result.tool_ammunition.len() != 1
                    || result.unsupported_fields.contains("pocket_data")
                {
                    false
                } else {
                    let ammunition_type = result
                        .tool_ammunition
                        .first()
                        .expect("a singleton tool-ammunition set has a first entry");
                    ammunition_type != "plutonium" && charge_carrier_is_supported(ammunition_type)
                }
            } else {
                true
            };
            let mut unsupported = recipe.unavailable_reasons.clone();
            if (!recipe.book_learn.is_empty()
                || (!recipe.never_learn && !recipe.learn_by_disassembly.is_empty()))
                && unsupported.contains("not_autolearned")
            {
                unsupported.remove("not_autolearned");
            }
            if result.subtypes.contains("GUN") && gun_ammunition_is_supported {
                unsupported.remove("unsupported_output_runtime");
            }
            if recipe.result != result_type_id
                || !recipe.reversible
                || recipe.obsolete
                || !unsupported.is_empty()
                || !recipe.byproducts.is_empty()
                || recipe.charges.unwrap_or(1).checked_mul(recipe.result_mult) != Some(1)
            {
                return false;
            }
            if result.count_by_charges()
                || !gun_ammunition_is_supported
                || !integral_tool_charges_are_supported
                || (result.default_charges() > 0 && !powered_tool)
            {
                return false;
            }
            let Ok(components) = self.resolved_components(recipe) else {
                return false;
            };
            if components.is_empty() || components.iter().any(Vec::is_empty) {
                return false;
            }
            let mut recovered = 0_u32;
            for component in components
                .into_iter()
                .filter_map(|group| group.into_iter().next())
            {
                let Some(item) = items.get(&component.type_id) else {
                    return false;
                };
                if !component.recoverable || item.flags.contains("UNRECOVERABLE") {
                    continue;
                }
                if !matches!(
                    item.temperature_runtime_class(),
                    ItemTemperatureRuntimeClass::NotTracked
                        | ItemTemperatureRuntimeClass::MateriallessNonperishable
                ) {
                    return false;
                }
                let instances = if item.count_by_charges() {
                    1
                } else {
                    component.count
                };
                let Some(next) = recovered.checked_add(instances) else {
                    return false;
                };
                if next > MAX_RECOVERED_INSTANCES {
                    return false;
                }
                recovered = next;
            }
            let Ok(tools) = self.resolved_tools(recipe) else {
                return false;
            };
            if tools.iter().any(|group| {
                group.is_empty()
                    || group.iter().any(|tool| {
                        tool.requirement_list
                            || tool.count >= 0
                            || tool.count.unsigned_abs() > MAX_RECOVERED_INSTANCES
                            || SPECIAL_TOOL_SUBSTITUTIONS.contains(&tool.type_id.as_str())
                    })
            }) {
                return false;
            }
            let Ok(qualities) = self.resolved_qualities(recipe) else {
                return false;
            };
            !qualities.iter().any(|group| {
                group.is_empty()
                    || group.iter().any(|quality| {
                        quality.amount == 0
                            || quality.amount > MAX_RECOVERED_INSTANCES
                            || SPECIAL_QUALITY_SUBSTITUTIONS.contains(&quality.quality_id.as_str())
                    })
            })
        };

        // This is upstream's recipe_dictionary precedence: a valid explicit
        // uncraft shadows every reversible craft for the same target item.
        if let Some(explicit) = self.uncrafts.get(result_type_id) {
            return supported(explicit).then_some(explicit);
        }
        self.craftable_with_knowledge_source()
            .find(|recipe| supported(recipe))
    }

    pub fn resolved_components(
        &self,
        recipe: &RecipeDefinition,
    ) -> Result<Vec<Vec<ComponentRequirement>>, RecipeRegistryError> {
        resolve_components(recipe, &self.requirements)
    }

    /// Expands recursive `LIST` component indirection for another pinned
    /// activity that shares CDDA's requirement dictionary.
    pub fn resolved_component_groups(
        &self,
        owner_id: &str,
        components: &[Vec<ComponentRequirement>],
    ) -> Result<Vec<Vec<ComponentRequirement>>, RecipeRegistryError> {
        resolve_component_groups_with_references(owner_id, components.to_vec(), &self.requirements)
            .map(|(components, _references)| components)
    }

    pub fn resolved_tools(
        &self,
        recipe: &RecipeDefinition,
    ) -> Result<Vec<Vec<ToolRequirement>>, RecipeRegistryError> {
        resolve_tools(recipe, &self.requirements, &self.tool_subtype_replacements)
    }

    pub fn resolved_qualities(
        &self,
        recipe: &RecipeDefinition,
    ) -> Result<Vec<Vec<QualityRequirement>>, RecipeRegistryError> {
        resolve_qualities(recipe, &self.requirements)
    }
}

impl RecipeDefinition {
    /// CDDA expands boolean `autolearn: true` during recipe finalization to the
    /// secondary skill requirements plus the primary skill at difficulty.
    #[must_use]
    pub fn resolved_autolearn_skills(&self) -> BTreeMap<String, u8> {
        if !self.autolearn_skills.is_empty() {
            return self.autolearn_skills.clone();
        }
        let mut requirements = self.skills_required.clone();
        if !self.skill_used.is_empty() {
            requirements.insert(self.skill_used.clone(), self.difficulty);
        }
        requirements
    }
}

fn supported_quality_providers(items: &ItemRegistry) -> BTreeMap<String, Vec<(String, i32, u16)>> {
    let mut providers = BTreeMap::<String, Vec<(String, i32, u16)>>::new();
    for (type_id, item) in items.iter() {
        if !item.unsupported_fields.contains("qualities") {
            for (quality_id, quality) in &item.qualities {
                providers.entry(quality_id.clone()).or_default().push((
                    type_id.to_owned(),
                    quality.level,
                    0,
                ));
            }
        }
        if !item.unsupported_fields.contains("charged_qualities")
            && u16::try_from(item.charges_per_use).is_ok_and(|charges| charges > 0)
        {
            let minimum_charges = u16::try_from(item.charges_per_use)
                .expect("positive bounded charge count was checked");
            for (quality_id, quality) in &item.charged_qualities {
                providers.entry(quality_id.clone()).or_default().push((
                    type_id.to_owned(),
                    quality.level,
                    minimum_charges,
                ));
            }
        }
    }
    providers
}

fn read_definitions(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<RawDefinitions, RecipeRegistryError> {
    let mut requirements = Vec::new();
    let mut recipes = VecDeque::new();
    let mut uncrafts = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| RecipeRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| RecipeRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_definition(
                        &file,
                        value,
                        &mut requirements,
                        &mut recipes,
                        &mut uncrafts,
                    )?;
                }
            }
            value => {
                collect_definition(&file, value, &mut requirements, &mut recipes, &mut uncrafts)?
            }
        }
    }
    Ok(RawDefinitions {
        requirements,
        recipes,
        uncrafts,
    })
}

fn collect_definition(
    file: &SelectedContentFile,
    value: Value,
    requirements: &mut Vec<RawRecipe>,
    recipes: &mut VecDeque<RawRecipe>,
    uncrafts: &mut VecDeque<RawRecipe>,
) -> Result<(), RecipeRegistryError> {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return Ok(());
    };
    if !matches!(kind, "recipe" | "uncraft" | "requirement") {
        return Ok(());
    }
    let raw = RawRecipe {
        file: file.clone(),
        object: value
            .as_object()
            .cloned()
            .ok_or_else(|| RecipeRegistryError::InvalidDefinition(file.upstream_path.clone()))?,
    };
    match kind {
        "requirement" => requirements.push(raw),
        "uncraft" => uncrafts.push_back(raw),
        "recipe" => recipes.push_back(raw),
        _ => unreachable!("definition kind was matched above"),
    }
    Ok(())
}

fn load_requirements(
    raw_requirements: Vec<RawRecipe>,
) -> Result<BTreeMap<String, RequirementDefinition>, RecipeRegistryError> {
    let mut definitions = BTreeMap::<String, RequirementDefinition>::new();
    for raw in raw_requirements {
        let source = raw.file.upstream_path.as_str();
        let id = required_string(&raw.object, "id", source)?;
        let context = format!("{source}#{id}");
        let mut direct = RequirementDefinition {
            id: id.to_owned(),
            source: source.to_owned(),
            ..RequirementDefinition::default()
        };
        if let Some(value) = raw.object.get("name") {
            direct.name = translated_string(value, &context, "name")?;
        }
        if let Some(value) = raw.object.get("components") {
            direct.components = parse_component_groups(value, &context, "components")?;
        }
        if let Some(value) = raw.object.get("tools") {
            direct.tools = parse_tool_groups(value, &context, "tools")?;
        }
        if let Some(value) = raw.object.get("qualities") {
            direct.qualities = parse_quality_groups(value, &context, "qualities")?;
        }
        for field in raw.object.keys() {
            if !field.starts_with("//") && !REQUIREMENT_IMPLEMENTED_FIELDS.contains(&field.as_str())
            {
                direct.unsupported_fields.insert(field.clone());
            }
        }
        let existing = definitions
            .entry(id.to_owned())
            .or_insert_with(|| RequirementDefinition {
                id: id.to_owned(),
                ..RequirementDefinition::default()
            });
        existing.source = source.to_owned();
        if !direct.name.is_empty() {
            existing.name = direct.name;
        }
        if !direct.components.is_empty() {
            existing.components = direct.components;
        }
        if !direct.tools.is_empty() {
            existing.tools = direct.tools;
        }
        if !direct.qualities.is_empty() {
            existing.qualities = direct.qualities;
        }
        existing
            .unsupported_fields
            .extend(direct.unsupported_fields);
        if let Some(extend) = raw.object.get("extend") {
            apply_requirement_extension(existing, extend, &context)?;
        }
    }
    Ok(definitions)
}

fn apply_requirement_extension(
    definition: &mut RequirementDefinition,
    value: &Value,
    source: &str,
) -> Result<(), RecipeRegistryError> {
    let object = value.as_object().ok_or_else(|| invalid(source, "extend"))?;
    for field in object.keys() {
        if !matches!(field.as_str(), "components" | "tools" | "qualities") {
            definition.unsupported_fields.insert(field.clone());
        }
    }
    if let Some(value) = object.get("components") {
        extend_groups(
            &mut definition.components,
            parse_component_groups(value, source, "extend.components")?,
        );
    }
    if let Some(value) = object.get("tools") {
        extend_groups(
            &mut definition.tools,
            parse_tool_groups(value, source, "extend.tools")?,
        );
    }
    if let Some(value) = object.get("qualities") {
        extend_groups(
            &mut definition.qualities,
            parse_quality_groups(value, source, "extend.qualities")?,
        );
    }
    Ok(())
}

fn extend_groups<T>(target: &mut [Vec<T>], extension: Vec<Vec<T>>) {
    for (group, additions) in target.iter_mut().zip(extension) {
        group.extend(additions);
    }
}

fn load_one_recipe(
    raw: &RawRecipe,
    recipes: &mut BTreeMap<String, RecipeDefinition>,
    abstracts: &mut BTreeMap<String, RecipeDefinition>,
) -> Result<bool, RecipeRegistryError> {
    let source = raw.file.upstream_path.as_str();
    let parent_id = optional_string(&raw.object, "copy-from", source)?;
    let mut definition = if let Some(parent_id) = parent_id {
        let Some(parent) = recipes.get(parent_id).or_else(|| abstracts.get(parent_id)) else {
            return Ok(false);
        };
        parent.clone()
    } else {
        RecipeDefinition {
            result_mult: 1,
            ..RecipeDefinition::default()
        }
    };
    let abstract_id = optional_string(&raw.object, "abstract", source)?;
    let provisional = abstract_id
        .or_else(|| raw.object.get("result").and_then(Value::as_str))
        .or(parent_id)
        .unwrap_or("<recipe>");
    let context = format!("{source}#{provisional}");
    apply_recipe_fields(&mut definition, &raw.object, &context)?;
    definition.source = source.to_owned();
    let is_abstract = abstract_id.is_some();
    let id = if let Some(abstract_id) = abstract_id {
        abstract_id.to_owned()
    } else {
        let mut id = if definition.result.is_empty() {
            required_string(&raw.object, "id", source)?.to_owned()
        } else {
            definition.result.clone()
        };
        if !definition.variant.is_empty() {
            id.push('_');
            id.push_str(&definition.variant);
        }
        if let Some(suffix) = optional_string(&raw.object, "id_suffix", source)? {
            if suffix.is_empty() {
                return Err(invalid(&context, "id_suffix"));
            }
            id.push('_');
            id.push_str(suffix);
        }
        id
    };
    definition.id.clone_from(&id);
    if is_abstract {
        abstracts.insert(id, definition);
    } else {
        recipes.insert(id, definition);
    }
    Ok(true)
}

fn apply_recipe_fields(
    recipe: &mut RecipeDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), RecipeRegistryError> {
    if object.get("type").and_then(Value::as_str) == Some("uncraft") {
        recipe.reversible = true;
    }
    if let Some(value) = object.get("obsolete") {
        recipe.obsolete = value.as_bool().ok_or_else(|| invalid(source, "obsolete"))?;
        if recipe.obsolete {
            return Ok(());
        }
    }
    apply_string(object, "result", &mut recipe.result, source)?;
    apply_string(object, "variant", &mut recipe.variant, source)?;
    if let Some(value) = object.get("name") {
        recipe.name = translated_string(value, source, "name")?;
    }
    if let Some(value) = object.get("description") {
        recipe.description = translated_string(value, source, "description")?;
    }
    for (field, target) in [
        ("category", &mut recipe.category),
        ("subcategory", &mut recipe.subcategory),
        ("activity_level", &mut recipe.activity_level),
        ("skill_used", &mut recipe.skill_used),
    ] {
        apply_string(object, field, target, source)?;
    }
    if let Some(value) = object.get("time") {
        recipe.time_moves = parse_duration_moves(
            value.as_str().ok_or_else(|| invalid(source, "time"))?,
            source,
        )?;
    }
    if let Some(value) = object.get("batch_time_factors") {
        recipe.batch_time_factors = Some(parse_batch_time_factors(value, source)?);
    }
    if recipe.batch_time_factors.as_ref().is_some_and(|factors| {
        matches!(
            factors,
            BatchTimeFactors::Linear { setup_moves, .. } if *setup_moves > recipe.time_moves
        )
    }) {
        return Err(invalid(source, "batch_time_factors"));
    }
    if let Some(value) = object.get("difficulty") {
        recipe.difficulty = u8::try_from(
            value
                .as_u64()
                .ok_or_else(|| invalid(source, "difficulty"))?,
        )
        .map_err(|_| invalid(source, "difficulty"))?;
    }
    if let Some(value) = object.get("skills_required") {
        recipe.skills_required = parse_skill_map(value, source, "skills_required")?;
    }
    if let Some(value) = object.get("autolearn") {
        match value {
            Value::Bool(value) => {
                recipe.autolearn = *value;
                recipe.autolearn_skills.clear();
            }
            Value::Array(_) => {
                recipe.autolearn = true;
                recipe.autolearn_skills = parse_skill_map(value, source, "autolearn")?;
            }
            _ => return Err(invalid(source, "autolearn")),
        }
    }
    if let Some(value) = object.get("decomp_learn") {
        recipe.learn_by_disassembly =
            parse_decomp_learn(value, recipe.skill_used.as_str(), source)?;
    }
    if let Some(value) = object.get("book_learn") {
        recipe.book_learn = parse_book_learn(value, source)?;
    }
    if let Some(value) = object.get("never_learn") {
        recipe.never_learn = value
            .as_bool()
            .ok_or_else(|| invalid(source, "never_learn"))?;
    }
    if let Some(value) = object.get("reversible") {
        if let Some(reversible) = value.as_bool() {
            recipe.reversible = reversible;
        } else {
            let reversible = value
                .as_object()
                .ok_or_else(|| invalid(source, "reversible"))?;
            let time = reversible
                .get("time")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(source, "reversible.time"))?;
            recipe.reversible = true;
            recipe.uncraft_time_moves = parse_duration_moves(time, source)?;
        }
    }
    if let Some(value) = object.get("charges") {
        recipe.charges = Some(
            u32::try_from(value.as_u64().ok_or_else(|| invalid(source, "charges"))?)
                .map_err(|_| invalid(source, "charges"))?,
        );
    }
    if let Some(value) = object.get("result_mult") {
        recipe.result_mult = u32::try_from(
            value
                .as_u64()
                .ok_or_else(|| invalid(source, "result_mult"))?,
        )
        .map_err(|_| invalid(source, "result_mult"))?;
        if recipe.result_mult == 0 {
            return Err(invalid(source, "result_mult"));
        }
    }
    // Upstream intentionally replaces inline requirements on every recipe
    // definition instead of inheriting them.
    recipe.components = object
        .get("components")
        .map(|value| parse_component_groups(value, source, "components"))
        .transpose()?
        .unwrap_or_default();
    recipe.tools = object
        .get("tools")
        .map(|value| parse_tool_groups(value, source, "tools"))
        .transpose()?
        .unwrap_or_default();
    recipe.qualities = object
        .get("qualities")
        .map(|value| parse_quality_groups(value, source, "qualities"))
        .transpose()?
        .unwrap_or_default();
    if let Some(value) = object.get("proficiencies") {
        recipe.proficiencies = parse_recipe_proficiencies(value, source, "proficiencies")?;
    }
    if let Some(value) = object
        .get("extend")
        .and_then(Value::as_object)
        .and_then(|extend| extend.get("proficiencies"))
    {
        recipe.proficiencies.extend(parse_recipe_proficiencies(
            value,
            source,
            "extend.proficiencies",
        )?);
    }
    if let Some(value) = object.get("using") {
        recipe.using = parse_using(value, source)?;
    }
    if let Some(value) = object
        .get("extend")
        .and_then(Value::as_object)
        .and_then(|extend| extend.get("using"))
    {
        recipe.using.extend(parse_using(value, source)?);
    }
    if let Some(value) = object.get("byproducts") {
        recipe.byproducts = parse_byproducts(value, source)?;
    }
    if let Some(value) = object.get("flags") {
        recipe.flags = parse_string_set(value, source, "flags")?;
    }
    if let Some(value) = object.get("delete_flags") {
        for flag in parse_string_set(value, source, "delete_flags")? {
            recipe.flags.remove(&flag);
        }
    }
    for field in object.keys() {
        if !field.starts_with("//")
            && !RECIPE_IMPLEMENTED_FIELDS.contains(&field.as_str())
            && field != "delete_flags"
        {
            recipe.unsupported_fields.insert(field.clone());
        }
    }
    for modifier in ["extend", "proportional"] {
        if let Some(value) = object.get(modifier) {
            let fields = value.as_object().ok_or_else(|| invalid(source, modifier))?;
            recipe.unsupported_fields.extend(
                fields
                    .keys()
                    .filter(|field| {
                        !(modifier == "extend"
                            && matches!(field.as_str(), "proficiencies" | "using"))
                    })
                    .map(|field| format!("{modifier}.{field}")),
            );
        }
    }
    Ok(())
}

fn parse_recipe_proficiencies(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Vec<RecipeProficiency>, RecipeRegistryError> {
    let entries = value.as_array().ok_or_else(|| invalid(source, field))?;
    let mut seen = BTreeSet::new();
    entries
        .iter()
        .map(|entry| {
            let object = entry.as_object().ok_or_else(|| invalid(source, field))?;
            if object.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "proficiency"
                        | "required"
                        | "time_multiplier"
                        | "skill_penalty"
                        | "learning_time_multiplier"
                        | "max_experience"
                )
            }) {
                return Err(invalid(source, field));
            }
            let proficiency_id = object
                .get("proficiency")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid(source, field))?
                .to_owned();
            if !seen.insert(proficiency_id.clone()) {
                return Err(invalid(source, field));
            }
            let required = object
                .get("required")
                .map(|value| value.as_bool().ok_or_else(|| invalid(source, field)))
                .transpose()?
                .unwrap_or(false);
            let time_multiplier_millionths = object
                .get("time_multiplier")
                .map(|value| {
                    let scaled = decimal_millionths(value, source, field)?;
                    u32::try_from(scaled).map_err(|_| invalid(source, field))
                })
                .transpose()?;
            if required && time_multiplier_millionths.is_some_and(|value| value != 0) {
                return Err(invalid(source, field));
            }
            let skill_penalty_millionths = object
                .get("skill_penalty")
                .map(|value| {
                    i32::try_from(decimal_millionths(value, source, field)?)
                        .map_err(|_| invalid(source, field))
                })
                .transpose()?;
            let learning_time_multiplier_millionths = object
                .get("learning_time_multiplier")
                .map(|value| {
                    let scaled = decimal_millionths(value, source, field)?;
                    u32::try_from(scaled).map_err(|_| invalid(source, field))
                })
                .transpose()?
                .unwrap_or(PROFICIENCY_MULTIPLIER_SCALE);
            let max_experience_moves = object
                .get("max_experience")
                .map(|value| {
                    parse_duration_moves(
                        value.as_str().ok_or_else(|| invalid(source, field))?,
                        source,
                    )
                })
                .transpose()?;
            Ok(RecipeProficiency {
                proficiency_id,
                required,
                time_multiplier_millionths,
                skill_penalty_millionths,
                learning_time_multiplier_millionths,
                max_experience_moves,
            })
        })
        .collect()
}

fn decimal_millionths(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<i64, RecipeRegistryError> {
    let number = value.as_number().ok_or_else(|| invalid(source, field))?;
    decimal_number_millionths(number, source, field)
}

fn parse_batch_time_factors(
    value: &Value,
    source: &str,
) -> Result<BatchTimeFactors, RecipeRegistryError> {
    if let Some(values) = value.as_array() {
        if values.len() != 2 {
            return Err(invalid(source, "batch_time_factors"));
        }
        let percent_millionths = u32::try_from(decimal_millionths(
            &values[0],
            source,
            "batch_time_factors",
        )?)
        .map_err(|_| invalid(source, "batch_time_factors"))?;
        let target_batch_size = u32::try_from(
            values[1]
                .as_u64()
                .ok_or_else(|| invalid(source, "batch_time_factors"))?,
        )
        .map_err(|_| invalid(source, "batch_time_factors"))?;
        if percent_millionths == 0
            || percent_millionths > 100 * PROFICIENCY_MULTIPLIER_SCALE
            || target_batch_size == 0
        {
            return Err(invalid(source, "batch_time_factors"));
        }
        return Ok(BatchTimeFactors::Logistic {
            percent_millionths,
            target_batch_size,
        });
    }

    let object = value
        .as_object()
        .ok_or_else(|| invalid(source, "batch_time_factors"))?;
    let mode = object
        .get("mode")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(source, "batch_time_factors"))?;
    match mode {
        "logistic" => {
            if object
                .keys()
                .any(|key| !matches!(key.as_str(), "mode" | "percent" | "at"))
            {
                return Err(invalid(source, "batch_time_factors"));
            }
            let percent_millionths = u32::try_from(decimal_millionths(
                object
                    .get("percent")
                    .ok_or_else(|| invalid(source, "batch_time_factors"))?,
                source,
                "batch_time_factors",
            )?)
            .map_err(|_| invalid(source, "batch_time_factors"))?;
            let target_batch_size = u32::try_from(
                object
                    .get("at")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| invalid(source, "batch_time_factors"))?,
            )
            .map_err(|_| invalid(source, "batch_time_factors"))?;
            if percent_millionths > 100 * PROFICIENCY_MULTIPLIER_SCALE || target_batch_size == 0 {
                return Err(invalid(source, "batch_time_factors"));
            }
            Ok(BatchTimeFactors::Logistic {
                percent_millionths,
                target_batch_size,
            })
        }
        "linear" => {
            if object
                .keys()
                .any(|key| !matches!(key.as_str(), "mode" | "setup" | "max"))
            {
                return Err(invalid(source, "batch_time_factors"));
            }
            let setup_moves = parse_duration_moves(
                object
                    .get("setup")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid(source, "batch_time_factors"))?,
                source,
            )?;
            let maximum_batch_size = object
                .get("max")
                .map(|value| {
                    u32::try_from(
                        value
                            .as_u64()
                            .ok_or_else(|| invalid(source, "batch_time_factors"))?,
                    )
                    .map_err(|_| invalid(source, "batch_time_factors"))
                })
                .transpose()?;
            if maximum_batch_size == Some(0) {
                return Err(invalid(source, "batch_time_factors"));
            }
            Ok(BatchTimeFactors::Linear {
                setup_moves,
                maximum_batch_size,
            })
        }
        _ => Err(invalid(source, "batch_time_factors")),
    }
}

fn parse_book_learn(
    value: &Value,
    source: &str,
) -> Result<BTreeMap<String, BookLearnDefinition>, RecipeRegistryError> {
    let mut books = BTreeMap::new();
    if let Some(entries) = value.as_array() {
        for entry in entries {
            let values = entry
                .as_array()
                .filter(|values| (1..=2).contains(&values.len()))
                .ok_or_else(|| invalid(source, "book_learn"))?;
            let book_id = values[0]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid(source, "book_learn"))?;
            let skill_level = values
                .get(1)
                .map(|value| {
                    i32::try_from(
                        value
                            .as_i64()
                            .ok_or_else(|| invalid(source, "book_learn"))?,
                    )
                    .map_err(|_| invalid(source, "book_learn"))
                })
                .transpose()?
                .unwrap_or(-1);
            // Pinned legacy loading uses `emplace`, so the first duplicate wins.
            books
                .entry(book_id.to_owned())
                .or_insert(BookLearnDefinition {
                    skill_level,
                    recipe_name: None,
                    hidden: false,
                });
        }
        return Ok(books);
    }

    let entries = value
        .as_object()
        .ok_or_else(|| invalid(source, "book_learn"))?;
    for (book_id, value) in entries {
        if book_id.is_empty() {
            return Err(invalid(source, "book_learn"));
        }
        let object = value
            .as_object()
            .ok_or_else(|| invalid(source, "book_learn"))?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "skill_level" | "recipe_name" | "hidden"))
        {
            return Err(invalid(source, "book_learn"));
        }
        let skill_level = object
            .get("skill_level")
            .map(|value| {
                i32::try_from(
                    value
                        .as_i64()
                        .ok_or_else(|| invalid(source, "book_learn"))?,
                )
                .map_err(|_| invalid(source, "book_learn"))
            })
            .transpose()?
            .unwrap_or(-1);
        let recipe_name = object
            .get("recipe_name")
            .map(|value| translated_string(value, source, "book_learn"))
            .transpose()?;
        let hidden = object
            .get("hidden")
            .map(|value| value.as_bool().ok_or_else(|| invalid(source, "book_learn")))
            .transpose()?
            .unwrap_or(false);
        books.insert(
            book_id.clone(),
            BookLearnDefinition {
                skill_level,
                recipe_name,
                hidden,
            },
        );
    }
    Ok(books)
}

fn parse_decomp_learn(
    value: &Value,
    skill_used: &str,
    source: &str,
) -> Result<BTreeMap<String, i32>, RecipeRegistryError> {
    if let Some(level) = value.as_i64() {
        if skill_used.is_empty() {
            return Err(invalid(source, "decomp_learn"));
        }
        return Ok(BTreeMap::from([(
            skill_used.to_owned(),
            i32::try_from(level).map_err(|_| invalid(source, "decomp_learn"))?,
        )]));
    }

    let entries = value
        .as_array()
        .ok_or_else(|| invalid(source, "decomp_learn"))?;
    let mut skills = BTreeMap::new();
    for entry in entries {
        let values = entry
            .as_array()
            .filter(|values| values.len() == 2)
            .ok_or_else(|| invalid(source, "decomp_learn"))?;
        let skill_id = values[0]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid(source, "decomp_learn"))?;
        let level = i32::try_from(
            values[1]
                .as_i64()
                .ok_or_else(|| invalid(source, "decomp_learn"))?,
        )
        .map_err(|_| invalid(source, "decomp_learn"))?;
        // Pinned array loading assigns through `operator[]`, so the last
        // duplicate skill replaces earlier entries.
        skills.insert(skill_id.to_owned(), level);
    }
    Ok(skills)
}

fn decimal_number_millionths(
    number: &Number,
    source: &str,
    field: &str,
) -> Result<i64, RecipeRegistryError> {
    let text = number.to_string();
    let (negative, unsigned) = text
        .strip_prefix('-')
        .map_or((false, text.as_str()), |value| (true, value));
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 6
    {
        return Err(invalid(source, field));
    }
    let whole = whole.parse::<i64>().map_err(|_| invalid(source, field))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| invalid(source, field))?
            .checked_mul(10_i64.pow(u32::try_from(6 - fraction.len()).expect("length fits")))
            .ok_or_else(|| invalid(source, field))?
    };
    let scaled = whole
        .checked_mul(i64::from(PROFICIENCY_MULTIPLIER_SCALE))
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| invalid(source, field))?;
    Ok(if negative { -scaled } else { scaled })
}

pub(crate) fn parse_component_groups(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Vec<Vec<ComponentRequirement>>, RecipeRegistryError> {
    parse_groups(value, source, field, |entry, context| {
        let values = entry.as_array().ok_or_else(|| invalid(context, field))?;
        if !(2..=4).contains(&values.len()) {
            return Err(invalid(context, field));
        }
        let type_id = values[0]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid(context, field))?
            .to_owned();
        let count = u32::try_from(values[1].as_u64().ok_or_else(|| invalid(context, field))?)
            .map_err(|_| invalid(context, field))?;
        if count == 0 {
            return Err(invalid(context, field));
        }
        let mut requirement_list = false;
        let mut recoverable = true;
        for flag in &values[2..] {
            match flag.as_str() {
                Some("LIST") => requirement_list = true,
                Some("NO_RECOVER") => recoverable = false,
                _ => return Err(invalid(context, field)),
            }
        }
        Ok(ComponentRequirement {
            type_id,
            count,
            requirement_list,
            recoverable,
        })
    })
}

fn parse_tool_groups(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Vec<Vec<ToolRequirement>>, RecipeRegistryError> {
    parse_groups(value, source, field, |entry, context| {
        if let Some(type_id) = entry.as_str() {
            if type_id.is_empty() {
                return Err(invalid(context, field));
            }
            return Ok(ToolRequirement {
                type_id: type_id.to_owned(),
                count: -1,
                requirement_list: false,
            });
        }
        let values = entry.as_array().ok_or_else(|| invalid(context, field))?;
        if !(2..=3).contains(&values.len()) {
            return Err(invalid(context, field));
        }
        let type_id = values[0]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid(context, field))?
            .to_owned();
        let count = i32::try_from(values[1].as_i64().ok_or_else(|| invalid(context, field))?)
            .map_err(|_| invalid(context, field))?;
        if count == 0 {
            return Err(invalid(context, field));
        }
        let requirement_list = match values.get(2).and_then(Value::as_str) {
            None => false,
            Some("LIST") => true,
            Some(_) => return Err(invalid(context, field)),
        };
        Ok(ToolRequirement {
            type_id,
            count,
            requirement_list,
        })
    })
}

pub(crate) fn parse_quality_groups(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Vec<Vec<QualityRequirement>>, RecipeRegistryError> {
    let groups = value.as_array().ok_or_else(|| invalid(source, field))?;
    let mut result = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        let context = format!("{source}.{field}[{index}]");
        if let Some(alternatives) = group.as_array() {
            let parsed = alternatives
                .iter()
                .map(|entry| parse_quality(entry, &context, field))
                .collect::<Result<Vec<_>, _>>()?;
            if !parsed.is_empty() {
                result.push(parsed);
            }
        } else {
            result.push(vec![parse_quality(group, &context, field)?]);
        }
    }
    Ok(result)
}

fn parse_quality(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<QualityRequirement, RecipeRegistryError> {
    let object = value.as_object().ok_or_else(|| invalid(source, field))?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "id" | "level" | "amount"))
    {
        return Err(invalid(source, field));
    }
    let quality_id = required_string(object, "id", source)?.to_owned();
    let level = object
        .get("level")
        .map(|value| {
            i32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
                .map_err(|_| invalid(source, field))
        })
        .transpose()?
        .unwrap_or(1);
    let amount = object
        .get("amount")
        .map(|value| {
            u32::try_from(value.as_u64().ok_or_else(|| invalid(source, field))?)
                .map_err(|_| invalid(source, field))
        })
        .transpose()?
        .unwrap_or(1);
    if amount == 0 {
        return Err(invalid(source, field));
    }
    Ok(QualityRequirement {
        quality_id,
        level,
        amount,
    })
}

fn parse_groups<T>(
    value: &Value,
    source: &str,
    field: &str,
    parse: impl Fn(&Value, &str) -> Result<T, RecipeRegistryError>,
) -> Result<Vec<Vec<T>>, RecipeRegistryError> {
    let groups = value.as_array().ok_or_else(|| invalid(source, field))?;
    let mut result = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        let context = format!("{source}.{field}[{index}]");
        let alternatives = group.as_array().ok_or_else(|| invalid(&context, field))?;
        let parsed = alternatives
            .iter()
            .map(|entry| parse(entry, &context))
            .collect::<Result<Vec<_>, _>>()?;
        if !parsed.is_empty() {
            result.push(parsed);
        }
    }
    Ok(result)
}

fn parse_using(
    value: &Value,
    source: &str,
) -> Result<Vec<ExternalRequirement>, RecipeRegistryError> {
    let entries = value.as_array().ok_or_else(|| invalid(source, "using"))?;
    entries
        .iter()
        .map(|entry| {
            let values = entry.as_array().ok_or_else(|| invalid(source, "using"))?;
            if values.len() != 2 {
                return Err(invalid(source, "using"));
            }
            let requirement_id = values[0]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid(source, "using"))?
                .to_owned();
            // Upstream's weighted_string_id_reader<int> accepts JSON floats
            // and uses C++'s truncating numeric cast.
            let raw_multiplier = values[1]
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0 && *value <= f64::from(u32::MAX))
                .ok_or_else(|| invalid(source, "using"))?;
            let multiplier = raw_multiplier.trunc() as u32;
            if multiplier == 0 {
                return Err(invalid(source, "using"));
            }
            Ok(ExternalRequirement {
                requirement_id,
                multiplier,
            })
        })
        .collect()
}

fn parse_byproducts(
    value: &Value,
    source: &str,
) -> Result<BTreeMap<String, u32>, RecipeRegistryError> {
    let entries = value
        .as_array()
        .ok_or_else(|| invalid(source, "byproducts"))?;
    let mut byproducts = BTreeMap::<String, u32>::new();
    for entry in entries {
        let values = entry
            .as_array()
            .ok_or_else(|| invalid(source, "byproducts"))?;
        if !(1..=2).contains(&values.len()) {
            return Err(invalid(source, "byproducts"));
        }
        let id = values[0]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid(source, "byproducts"))?;
        let count = values
            .get(1)
            .map(|value| {
                u32::try_from(
                    value
                        .as_u64()
                        .ok_or_else(|| invalid(source, "byproducts"))?,
                )
                .map_err(|_| invalid(source, "byproducts"))
            })
            .transpose()?
            .unwrap_or(1);
        if count == 0 {
            return Err(invalid(source, "byproducts"));
        }
        let total = byproducts.entry(id.to_owned()).or_default();
        *total = total
            .checked_add(count)
            .ok_or_else(|| invalid(source, "byproducts"))?;
    }
    Ok(byproducts)
}

pub(crate) fn parse_skill_map(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeMap<String, u8>, RecipeRegistryError> {
    let values = value.as_array().ok_or_else(|| invalid(source, field))?;
    let entries: Vec<&Value> = if values.first().is_some_and(Value::is_array) {
        values.iter().collect()
    } else if values.is_empty() {
        Vec::new()
    } else {
        vec![value]
    };
    let mut skills = BTreeMap::new();
    for entry in entries {
        let pair = entry.as_array().ok_or_else(|| invalid(source, field))?;
        if pair.len() != 2 {
            return Err(invalid(source, field));
        }
        let id = pair[0]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid(source, field))?;
        let level = u8::try_from(pair[1].as_u64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))?;
        skills.insert(id.to_owned(), level);
    }
    Ok(skills)
}

fn parse_string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, RecipeRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid(source, field))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|entry| !entry.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid(source, field))
        })
        .collect()
}

pub(crate) fn parse_duration_moves(value: &str, source: &str) -> Result<u64, RecipeRegistryError> {
    let bytes = value.as_bytes();
    let mut index = 0_usize;
    let mut seconds = 0_u64;
    let mut terms = 0_usize;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index == bytes.len() {
            break;
        }
        let number_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if number_start == index {
            return Err(invalid(source, "time"));
        }
        let number = value[number_start..index]
            .parse::<u64>()
            .map_err(|_| invalid(source, "time"))?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let unit_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        if unit_start == index {
            return Err(invalid(source, "time"));
        }
        let multiplier = match &value[unit_start..index] {
            "s" | "second" | "seconds" => 1,
            "m" | "minute" | "minutes" => 60,
            "h" | "hour" | "hours" => 60 * 60,
            "d" | "day" | "days" => 24 * 60 * 60,
            _ => return Err(invalid(source, "time")),
        };
        seconds = seconds
            .checked_add(
                number
                    .checked_mul(multiplier)
                    .ok_or_else(|| invalid(source, "time"))?,
            )
            .ok_or_else(|| invalid(source, "time"))?;
        terms += 1;
    }
    if terms == 0 {
        return Err(invalid(source, "time"));
    }
    seconds
        .checked_mul(100)
        .ok_or_else(|| invalid(source, "time"))
}

fn resolve_components(
    recipe: &RecipeDefinition,
    requirements: &BTreeMap<String, RequirementDefinition>,
) -> Result<Vec<Vec<ComponentRequirement>>, RecipeRegistryError> {
    resolve_components_with_references(recipe, requirements).map(|(components, _)| components)
}

fn resolve_components_with_references(
    recipe: &RecipeDefinition,
    requirements: &BTreeMap<String, RequirementDefinition>,
) -> Result<(Vec<Vec<ComponentRequirement>>, BTreeSet<String>), RecipeRegistryError> {
    let mut components = recipe.components.clone();
    for external in &recipe.using {
        let requirement = requirements.get(&external.requirement_id).ok_or_else(|| {
            RecipeRegistryError::MissingRequirement {
                recipe: recipe.id.clone(),
                requirement: external.requirement_id.clone(),
            }
        })?;
        for group in &requirement.components {
            let mut scaled = group.clone();
            for component in &mut scaled {
                component.count = component
                    .count
                    .checked_mul(external.multiplier)
                    .ok_or_else(|| invalid(&recipe.source, "using"))?;
            }
            components.push(scaled);
        }
    }
    resolve_component_groups_with_references(&recipe.id, components, requirements)
}

fn resolve_component_groups_with_references(
    owner_id: &str,
    components: Vec<Vec<ComponentRequirement>>,
    requirements: &BTreeMap<String, RequirementDefinition>,
) -> Result<(Vec<Vec<ComponentRequirement>>, BTreeSet<String>), RecipeRegistryError> {
    let mut references = BTreeSet::new();
    let mut expanded = Vec::with_capacity(components.len());
    for group in components {
        let mut alternatives = Vec::new();
        let mut path = Vec::new();
        for component in group {
            inline_component_requirement(
                &component,
                1,
                requirements,
                owner_id,
                &mut path,
                &mut references,
                &mut alternatives,
            )?;
        }
        expanded.push(alternatives);
    }
    Ok((expanded, references))
}

#[allow(clippy::too_many_arguments)]
fn inline_component_requirement(
    component: &ComponentRequirement,
    multiplier: u32,
    requirements: &BTreeMap<String, RequirementDefinition>,
    recipe_id: &str,
    path: &mut Vec<String>,
    references: &mut BTreeSet<String>,
    alternatives: &mut Vec<ComponentRequirement>,
) -> Result<(), RecipeRegistryError> {
    let count = component
        .count
        .checked_mul(multiplier)
        .ok_or_else(|| invalid(recipe_id, "components.LIST"))?;
    if !component.requirement_list {
        if let Some(existing) = alternatives
            .iter_mut()
            .find(|existing| existing.type_id == component.type_id)
        {
            // Pinned inline_requirements keeps the first alternative's other
            // fields and only selects the smaller duplicate count.
            existing.count = existing.count.min(count);
        } else {
            let mut expanded = component.clone();
            expanded.count = count;
            alternatives.push(expanded);
        }
        return Ok(());
    }

    if path
        .iter()
        .any(|requirement| requirement == &component.type_id)
    {
        let mut chain = path.clone();
        chain.push(component.type_id.clone());
        return Err(RecipeRegistryError::CyclicRequirement {
            recipe: recipe_id.to_owned(),
            chain,
        });
    }
    let requirement = requirements.get(&component.type_id).ok_or_else(|| {
        RecipeRegistryError::MissingRequirement {
            recipe: recipe_id.to_owned(),
            requirement: component.type_id.clone(),
        }
    })?;
    references.insert(component.type_id.clone());
    path.push(component.type_id.clone());
    if let Some(group) = requirement.components.first() {
        for nested in group {
            inline_component_requirement(
                nested,
                count,
                requirements,
                recipe_id,
                path,
                references,
                alternatives,
            )?;
        }
    }
    path.pop();
    Ok(())
}

fn resolve_tools(
    recipe: &RecipeDefinition,
    requirements: &BTreeMap<String, RequirementDefinition>,
    tool_subtype_replacements: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<Vec<ToolRequirement>>, RecipeRegistryError> {
    resolve_tools_with_references(recipe, requirements, tool_subtype_replacements)
        .map(|(tools, _)| tools)
}

fn resolve_tools_with_references(
    recipe: &RecipeDefinition,
    requirements: &BTreeMap<String, RequirementDefinition>,
    tool_subtype_replacements: &BTreeMap<String, Vec<String>>,
) -> Result<(Vec<Vec<ToolRequirement>>, BTreeSet<String>), RecipeRegistryError> {
    let mut tools = recipe.tools.clone();
    for external in &recipe.using {
        let requirement = requirements.get(&external.requirement_id).ok_or_else(|| {
            RecipeRegistryError::MissingRequirement {
                recipe: recipe.id.clone(),
                requirement: external.requirement_id.clone(),
            }
        })?;
        for group in &requirement.tools {
            let mut scaled = group.clone();
            for tool in &mut scaled {
                if tool.count > 0 {
                    tool.count = i32::try_from(
                        u32::try_from(tool.count)
                            .map_err(|_| invalid(&recipe.source, "using"))?
                            .checked_mul(external.multiplier)
                            .ok_or_else(|| invalid(&recipe.source, "using"))?,
                    )
                    .map_err(|_| invalid(&recipe.source, "using"))?;
                } else {
                    // Pinned requirement_data::operator* clamps every
                    // non-charge tool requirement back to one instance.
                    tool.count = -1;
                }
            }
            tools.push(scaled);
        }
    }
    let mut references = BTreeSet::new();
    let mut expanded = Vec::with_capacity(tools.len());
    for group in tools {
        let mut alternatives = Vec::new();
        let mut path = Vec::new();
        for tool in group {
            inline_tool_requirement(
                &tool,
                1,
                requirements,
                &recipe.id,
                &mut path,
                &mut references,
                &mut alternatives,
            )?;
        }
        let mut replacements = Vec::new();
        for tool in alternatives {
            replacements.push(tool.clone());
            for replacement in tool_subtype_replacements
                .get(&tool.type_id)
                .into_iter()
                .flatten()
            {
                let mut replacement_tool = tool.clone();
                replacement_tool.type_id.clone_from(replacement);
                replacements.push(replacement_tool);
            }
        }
        expanded.push(replacements);
    }
    Ok((expanded, references))
}

#[allow(clippy::too_many_arguments)]
fn inline_tool_requirement(
    tool: &ToolRequirement,
    multiplier: u32,
    requirements: &BTreeMap<String, RequirementDefinition>,
    recipe_id: &str,
    path: &mut Vec<String>,
    references: &mut BTreeSet<String>,
    alternatives: &mut Vec<ToolRequirement>,
) -> Result<(), RecipeRegistryError> {
    let count = if tool.count < 0 {
        -1
    } else {
        i32::try_from(
            u32::try_from(tool.count)
                .map_err(|_| invalid(recipe_id, "tools.LIST"))?
                .checked_mul(multiplier)
                .ok_or_else(|| invalid(recipe_id, "tools.LIST"))?,
        )
        .map_err(|_| invalid(recipe_id, "tools.LIST"))?
    };
    if !tool.requirement_list {
        if let Some(existing) = alternatives
            .iter_mut()
            .find(|existing| existing.type_id == tool.type_id)
        {
            existing.count = existing.count.min(count);
        } else {
            let mut expanded = tool.clone();
            expanded.count = count;
            alternatives.push(expanded);
        }
        return Ok(());
    }

    let requirement_multiplier =
        u32::try_from(count).map_err(|_| invalid(recipe_id, "tools.LIST"))?;
    if path.iter().any(|requirement| requirement == &tool.type_id) {
        let mut chain = path.clone();
        chain.push(tool.type_id.clone());
        return Err(RecipeRegistryError::CyclicRequirement {
            recipe: recipe_id.to_owned(),
            chain,
        });
    }
    let requirement =
        requirements
            .get(&tool.type_id)
            .ok_or_else(|| RecipeRegistryError::MissingRequirement {
                recipe: recipe_id.to_owned(),
                requirement: tool.type_id.clone(),
            })?;
    references.insert(tool.type_id.clone());
    path.push(tool.type_id.clone());
    if let Some(group) = requirement.tools.first() {
        for nested in group {
            inline_tool_requirement(
                nested,
                requirement_multiplier,
                requirements,
                recipe_id,
                path,
                references,
                alternatives,
            )?;
        }
    }
    path.pop();
    Ok(())
}

fn resolve_qualities(
    recipe: &RecipeDefinition,
    requirements: &BTreeMap<String, RequirementDefinition>,
) -> Result<Vec<Vec<QualityRequirement>>, RecipeRegistryError> {
    let mut qualities = recipe.qualities.clone();
    for external in &recipe.using {
        let requirement = requirements.get(&external.requirement_id).ok_or_else(|| {
            RecipeRegistryError::MissingRequirement {
                recipe: recipe.id.clone(),
                requirement: external.requirement_id.clone(),
            }
        })?;
        // Pinned requirement multiplication does not scale quality counts.
        qualities.extend(requirement.qualities.clone());
    }
    Ok(qualities)
}

fn finalize_availability(
    recipe: &mut RecipeDefinition,
    requirements: &BTreeMap<String, RequirementDefinition>,
    items: &ItemRegistry,
    skills: &SkillRegistry,
    proficiencies: &ProficiencyRegistry,
    quality_providers: &BTreeMap<String, Vec<(String, i32, u16)>>,
    tool_subtype_replacements: &BTreeMap<String, Vec<String>>,
) -> Result<(), RecipeRegistryError> {
    let mut reasons = BTreeSet::new();
    if recipe.obsolete {
        reasons.insert(String::from("obsolete"));
    }
    if !recipe.autolearn {
        reasons.insert(String::from("not_autolearned"));
    }
    if recipe.time_moves == 0 {
        reasons.insert(String::from("no_supported_time"));
    }
    let all_skill_ids = recipe
        .skills_required
        .keys()
        .chain(recipe.autolearn_skills.keys())
        .chain(recipe.learn_by_disassembly.keys())
        .chain((!recipe.skill_used.is_empty()).then_some(&recipe.skill_used));
    for skill_id in all_skill_ids {
        match skills.get(skill_id) {
            Some(skill) if !skill.tags.contains("contextual_skill") => {}
            Some(_) => {
                reasons.insert(format!("contextual_skill:{skill_id}"));
            }
            None => {
                reasons.insert(format!("missing_skill:{skill_id}"));
            }
        }
    }
    for book_id in recipe.book_learn.keys() {
        match items.get(book_id) {
            Some(book) if book.subtypes.contains("BOOK") => {}
            Some(_) => {
                reasons.insert(format!("invalid_recipe_book:{book_id}"));
            }
            None => {
                reasons.insert(format!("missing_recipe_book:{book_id}"));
            }
        }
    }
    let mut proficiency_ids = BTreeSet::new();
    for proficiency in &recipe.proficiencies {
        if !proficiency_ids.insert(proficiency.proficiency_id.as_str()) {
            reasons.insert(String::from("duplicate_recipe_proficiency"));
        }
        if proficiencies.get(&proficiency.proficiency_id).is_none() {
            reasons.insert(format!(
                "missing_proficiency:{}",
                proficiency.proficiency_id
            ));
        }
    }
    let mut total_output_instances = 0_u32;
    if recipe.result.is_empty() {
        reasons.insert(String::from("missing_result_item"));
    } else if let Some(result) = items.get(&recipe.result) {
        let amount = recipe.charges.unwrap_or(1).checked_mul(recipe.result_mult);
        if amount.is_none_or(|amount| amount == 0 || amount > 256) {
            reasons.insert(String::from("unsupported_output_count"));
        } else if result.count_by_charges() {
            total_output_instances = 1;
        } else if let Some(amount) = amount {
            total_output_instances = amount;
        }
        if result.subtypes.contains("GUN") || result.ammo_types.len() > 1 {
            reasons.insert(String::from("unsupported_output_runtime"));
        }
    } else {
        reasons.insert(String::from("missing_result_item"));
    }
    if recipe.byproducts.len() > 64 {
        reasons.insert(String::from("unsupported_byproduct_count"));
    }
    for (byproduct_id, count) in &recipe.byproducts {
        let Some(byproduct) = items.get(byproduct_id) else {
            reasons.insert(format!("missing_byproduct_item:{byproduct_id}"));
            continue;
        };
        if byproduct.subtypes.contains("GUN") || byproduct.ammo_types.len() > 1 {
            reasons.insert(String::from("unsupported_byproduct_runtime"));
        }
        if byproduct.count_by_charges() {
            let default_charges = byproduct.default_charges();
            if default_charges <= 0
                || i32::try_from(*count)
                    .ok()
                    .and_then(|count| default_charges.checked_mul(count))
                    .is_none()
            {
                reasons.insert(String::from("unsupported_byproduct_count"));
            }
            total_output_instances = total_output_instances.saturating_add(1);
        } else {
            total_output_instances = total_output_instances.saturating_add(*count);
        }
    }
    if total_output_instances == 0 || total_output_instances > 256 {
        reasons.insert(String::from("unsupported_output_count"));
    }
    if recipe
        .flags
        .iter()
        .any(|flag| !matches!(flag.as_str(), "BLIND_EASY" | "ALLOW_ROTTEN"))
    {
        reasons.insert(String::from("recipe_flag_semantics"));
    }
    if !recipe.unsupported_fields.is_empty() {
        reasons.insert(String::from("unsupported_recipe_fields"));
    }
    let (components, component_requirement_references) =
        match resolve_components_with_references(recipe, requirements) {
            Ok(resolved) => resolved,
            Err(RecipeRegistryError::MissingRequirement { .. }) => {
                reasons.insert(String::from("missing_requirement_reference"));
                (Vec::new(), BTreeSet::new())
            }
            Err(RecipeRegistryError::CyclicRequirement { .. }) => {
                reasons.insert(String::from("cyclic_requirement_reference"));
                (Vec::new(), BTreeSet::new())
            }
            Err(error) => return Err(error),
        };
    for external in &recipe.using {
        if requirements
            .get(&external.requirement_id)
            .is_some_and(|requirement| !requirement.unsupported_fields.is_empty())
        {
            reasons.insert(String::from("unsupported_requirement_fields"));
        }
    }
    if component_requirement_references
        .iter()
        .any(|requirement_id| {
            requirements
                .get(requirement_id)
                .is_some_and(|requirement| !requirement.unsupported_fields.is_empty())
        })
    {
        reasons.insert(String::from("unsupported_requirement_fields"));
    }
    let (tools, tool_requirement_references) =
        match resolve_tools_with_references(recipe, requirements, tool_subtype_replacements) {
            Ok(resolved) => resolved,
            Err(RecipeRegistryError::MissingRequirement { .. }) => {
                reasons.insert(String::from("missing_requirement_reference"));
                (Vec::new(), BTreeSet::new())
            }
            Err(RecipeRegistryError::CyclicRequirement { .. }) => {
                reasons.insert(String::from("cyclic_requirement_reference"));
                (Vec::new(), BTreeSet::new())
            }
            Err(error) => return Err(error),
        };
    if tool_requirement_references.iter().any(|requirement_id| {
        requirements
            .get(requirement_id)
            .is_some_and(|requirement| !requirement.unsupported_fields.is_empty())
    }) {
        reasons.insert(String::from("unsupported_requirement_fields"));
    }
    let qualities = match resolve_qualities(recipe, requirements) {
        Ok(qualities) => qualities,
        Err(RecipeRegistryError::MissingRequirement { .. }) => Vec::new(),
        Err(error) => return Err(error),
    };
    let mut support_type_ids = BTreeSet::new();
    if tools.len() > RUNTIME_MAX_SUPPORT_GROUPS
        || qualities.len() > RUNTIME_MAX_SUPPORT_GROUPS
        || tools
            .iter()
            .any(|group| group.is_empty() || group.len() > RUNTIME_MAX_SUPPORT_ALTERNATIVES)
        || qualities
            .iter()
            .any(|group| group.is_empty() || group.len() > RUNTIME_MAX_SUPPORT_ALTERNATIVES)
    {
        reasons.insert(String::from("unsupported_support_shape"));
    }
    for tool in tools.iter().flatten() {
        if tool.count == 0
            || tool.count == i32::MIN
            || (tool.count < 0 && tool.count.unsigned_abs() > 256)
            || (tool.count > 0 && tool.count.unsigned_abs() > u32::from(u16::MAX))
        {
            reasons.insert(String::from("unsupported_tool_count"));
        } else if let Some(item) = items.get(&tool.type_id) {
            if item.count_by_charges() {
                reasons.insert(String::from("stacked_tool_semantics"));
            }
            if tool.count > 0 && item.unsupported_fields.contains("charge_factor") {
                reasons.insert(String::from("unsupported_tool_charge_factor"));
            }
            support_type_ids.insert(tool.type_id.clone());
        } else {
            reasons.insert(format!("missing_tool_item:{}", tool.type_id));
        }
    }
    for quality in qualities.iter().flatten() {
        if quality.amount == 0 || quality.amount > 256 {
            reasons.insert(String::from("unsupported_quality_count"));
            continue;
        }
        let providers = quality_providers
            .get(&quality.quality_id)
            .into_iter()
            .flatten()
            .filter(|(_, level, _)| *level >= quality.level)
            .map(|(type_id, _, _)| type_id.clone())
            .collect::<BTreeSet<_>>();
        if providers.is_empty() {
            reasons.insert(format!(
                "missing_supported_quality_provider:{}",
                quality.quality_id
            ));
        }
        if providers.len() > RUNTIME_MAX_QUALITY_PROVIDERS {
            reasons.insert(String::from("unsupported_quality_provider_count"));
        }
        support_type_ids.extend(providers);
    }
    if components.is_empty() {
        reasons.insert(String::from("no_components"));
    }
    if components.len() > RUNTIME_MAX_COMPONENT_GROUPS
        || components.iter().any(Vec::is_empty)
        || components
            .iter()
            .any(|group| group.len() > RUNTIME_MAX_COMPONENT_ALTERNATIVES)
    {
        reasons.insert(String::from("unsupported_component_shape"));
    }
    for component in components.iter().flatten() {
        if items.get(&component.type_id).is_none() {
            reasons.insert(format!("missing_component_item:{}", component.type_id));
        } else if support_type_ids.contains(&component.type_id) {
            // This first support slice deliberately avoids the upstream
            // component/tool deduplication search. It therefore never consumes
            // an item that could also be satisfying a tool or quality gate.
            reasons.insert(String::from("support_component_overlap"));
        }
    }
    recipe.unavailable_reasons = reasons;
    Ok(())
}

fn translated_string(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<String, RecipeRegistryError> {
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

fn apply_string(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), RecipeRegistryError> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid(source, field))?
            .to_owned();
    }
    Ok(())
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, RecipeRegistryError> {
    optional_string(object, field, source)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(source, field))
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<&'a str>, RecipeRegistryError> {
    object
        .get(field)
        .map(|value| value.as_str().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn raw_recipe_label(object: &Map<String, Value>) -> String {
    object
        .get("abstract")
        .or_else(|| object.get("result"))
        .or_else(|| object.get("copy-from"))
        .and_then(Value::as_str)
        .unwrap_or("<recipe>")
        .to_owned()
}

fn invalid(source: &str, field: &str) -> RecipeRegistryError {
    RecipeRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum RecipeRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    MissingRequirement { recipe: String, requirement: String },
    CyclicRequirement { recipe: String, chain: Vec<String> },
    MissingResult { source: String },
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for RecipeRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "recipe mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("internal recipe load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "recipe definition is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid recipe field {field} in {source}")
            }
            Self::Io(path, error) => {
                write!(formatter, "recipe registry I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "recipe registry JSON failed for {path}: {error}")
            }
            Self::MissingRequirement {
                recipe,
                requirement,
            } => {
                write!(
                    formatter,
                    "recipe {recipe} references missing requirement {requirement}"
                )
            }
            Self::CyclicRequirement { recipe, chain } => {
                write!(
                    formatter,
                    "recipe {recipe} references a cyclic requirement chain: {chain:?}"
                )
            }
            Self::MissingResult { source } => write!(formatter, "recipe has no result in {source}"),
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved or cyclic recipe inheritance: {ids:?}"
                )
            }
        }
    }
}

impl std::error::Error for RecipeRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_parser_uses_pinned_moves() {
        assert_eq!(
            parse_duration_moves("48 s", "test").expect("duration"),
            4_800
        );
        assert_eq!(
            parse_duration_moves("1 h 20m", "test").expect("duration"),
            480_000
        );
        assert!(parse_duration_moves("one minute", "test").is_err());
    }

    #[test]
    fn object_reversible_time_is_inherited_and_boolean_false_disables_it() {
        let parent = serde_json::json!({
            "type": "recipe",
            "abstract": "timed_uncraft_parent",
            "activity_level": "LIGHT_EXERCISE",
            "time": "10 m",
            "reversible": { "time": "30 s" },
            "components": [ [ [ "scrap", 1 ] ] ]
        });
        let child = serde_json::json!({
            "type": "recipe",
            "result": "test_result",
            "copy-from": "timed_uncraft_parent"
        });
        let disabled = serde_json::json!({
            "type": "recipe",
            "result": "disabled_result",
            "copy-from": "timed_uncraft_parent",
            "reversible": false
        });
        let mut recipes = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let raw = |object: Value| RawRecipe {
            file: SelectedContentFile {
                owner: String::from("dda"),
                upstream_path: String::from("test.json"),
                destination: String::from("test.json"),
            },
            object: object.as_object().expect("object").clone(),
        };
        assert!(load_one_recipe(&raw(parent), &mut recipes, &mut abstracts).expect("parent"));
        assert!(load_one_recipe(&raw(child), &mut recipes, &mut abstracts).expect("child"));
        assert!(load_one_recipe(&raw(disabled), &mut recipes, &mut abstracts).expect("disabled"));
        let inherited = recipes.get("test_result").expect("inherited recipe");
        assert!(inherited.reversible);
        assert_eq!(inherited.uncraft_time_moves, 30 * 100);
        let disabled = recipes.get("disabled_result").expect("disabled recipe");
        assert!(!disabled.reversible);
        assert_eq!(disabled.uncraft_time_moves, 30 * 100);
    }

    #[test]
    fn batch_factors_accept_pinned_shapes_and_leave_single_recipe_time_unchanged() {
        assert_eq!(
            parse_batch_time_factors(&serde_json::json!([67, 5]), "test")
                .expect("legacy logistic factors"),
            BatchTimeFactors::Logistic {
                percent_millionths: 67_000_000,
                target_batch_size: 5,
            }
        );
        let mut recipe = RecipeDefinition::default();
        apply_recipe_fields(
            &mut recipe,
            serde_json::json!({
                "time": "10 m",
                "batch_time_factors": { "mode": "linear", "setup": "5 m", "max": 10 }
            })
            .as_object()
            .expect("object"),
            "test",
        )
        .expect("linear factors");
        assert_eq!(recipe.time_moves, 60_000);
        assert_eq!(
            recipe.batch_time_factors,
            Some(BatchTimeFactors::Linear {
                setup_moves: 30_000,
                maximum_batch_size: Some(10),
            })
        );
        assert!(
            apply_recipe_fields(
                &mut recipe,
                serde_json::json!({ "time": "1 m" })
                    .as_object()
                    .expect("object"),
                "test",
            )
            .is_err(),
            "an inherited linear setup cannot exceed a replacement recipe time"
        );
        assert!(parse_batch_time_factors(&serde_json::json!([0, 1]), "test").is_err());
        assert!(
            parse_batch_time_factors(
                &serde_json::json!({ "mode": "logistic", "percent": 101, "at": 1 }),
                "test"
            )
            .is_err()
        );
    }

    #[test]
    fn book_learning_metadata_accepts_legacy_and_explicit_pinned_shapes() {
        assert_eq!(
            parse_book_learn(
                &serde_json::json!([["cookbook", 2], ["survival_book"], ["cookbook", 9]]),
                "test"
            )
            .expect("legacy book list"),
            BTreeMap::from([
                (
                    String::from("cookbook"),
                    BookLearnDefinition {
                        skill_level: 2,
                        recipe_name: None,
                        hidden: false,
                    }
                ),
                (
                    String::from("survival_book"),
                    BookLearnDefinition {
                        skill_level: -1,
                        recipe_name: None,
                        hidden: false,
                    }
                ),
            ])
        );
        assert_eq!(
            parse_book_learn(
                &serde_json::json!({
                    "textbook_anarch": {
                        "skill_level": 7,
                        "recipe_name": "Forbidden chemistry",
                        "hidden": true
                    }
                }),
                "test"
            )
            .expect("explicit book map")["textbook_anarch"],
            BookLearnDefinition {
                skill_level: 7,
                recipe_name: Some(String::from("Forbidden chemistry")),
                hidden: true,
            }
        );
        assert!(parse_book_learn(&serde_json::json!([["book", 1, 2]]), "test").is_err());
    }

    #[test]
    fn disassembly_learning_metadata_accepts_pinned_shapes_and_replacement() {
        assert_eq!(
            parse_decomp_learn(&serde_json::json!(3), "electronics", "test")
                .expect("primary-skill shorthand"),
            BTreeMap::from([(String::from("electronics"), 3)])
        );
        assert_eq!(
            parse_decomp_learn(
                &serde_json::json!([["electronics", 3], ["fabrication", 1], ["electronics", 5]]),
                "ignored_for_explicit_list",
                "test"
            )
            .expect("explicit skill list"),
            BTreeMap::from([
                (String::from("electronics"), 5),
                (String::from("fabrication"), 1),
            ])
        );
        assert!(parse_decomp_learn(&serde_json::json!(2), "", "test").is_err());
        assert!(parse_decomp_learn(&serde_json::json!([["electronics"]]), "", "test").is_err());

        let mut inherited = RecipeDefinition {
            skill_used: String::from("electronics"),
            learn_by_disassembly: BTreeMap::from([(String::from("electronics"), 2)]),
            ..RecipeDefinition::default()
        };
        apply_recipe_fields(
            &mut inherited,
            serde_json::json!({ "description": "inherits metadata" })
                .as_object()
                .expect("object"),
            "test",
        )
        .expect("inherited metadata");
        assert_eq!(inherited.learn_by_disassembly["electronics"], 2);
        apply_recipe_fields(
            &mut inherited,
            serde_json::json!({ "decomp_learn": [] })
                .as_object()
                .expect("object"),
            "test",
        )
        .expect("explicit empty replacement");
        assert!(inherited.learn_by_disassembly.is_empty());
    }

    #[test]
    fn extend_using_appends_to_inherited_external_requirements() {
        let mut inherited = RecipeDefinition {
            using: vec![ExternalRequirement {
                requirement_id: String::from("blacksmithing_standard"),
                multiplier: 4,
            }],
            ..RecipeDefinition::default()
        };
        apply_recipe_fields(
            &mut inherited,
            serde_json::json!({ "extend": { "using": [["carbon", 1]] } })
                .as_object()
                .expect("object"),
            "test",
        )
        .expect("extended requirements");
        assert_eq!(
            inherited.using,
            vec![
                ExternalRequirement {
                    requirement_id: String::from("blacksmithing_standard"),
                    multiplier: 4,
                },
                ExternalRequirement {
                    requirement_id: String::from("carbon"),
                    multiplier: 1,
                },
            ]
        );
        assert!(inherited.unsupported_fields.is_empty());

        apply_recipe_fields(
            &mut inherited,
            serde_json::json!({ "using": [["forging_standard", 2]] })
                .as_object()
                .expect("object"),
            "test",
        )
        .expect("root replacement");
        assert_eq!(
            inherited.using,
            vec![ExternalRequirement {
                requirement_id: String::from("forging_standard"),
                multiplier: 2,
            }]
        );
    }

    #[test]
    fn component_groups_preserve_and_or_structure_and_flags() {
        let value = serde_json::json!([
            [["stick", 1], ["2x4", 1]],
            [["cordage_short", 2, "LIST", "NO_RECOVER"]]
        ]);
        let groups = parse_component_groups(&value, "test", "components").expect("components");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
        assert!(groups[1][0].requirement_list);
        assert!(!groups[1][0].recoverable);
    }

    #[test]
    fn component_lists_inline_recursively_scale_and_deduplicate_like_pinned_cdda() {
        let component = |type_id: &str, count: u32, requirement_list: bool| ComponentRequirement {
            type_id: type_id.to_owned(),
            count,
            requirement_list,
            recoverable: true,
        };
        let requirements = BTreeMap::from([
            (
                String::from("cordage_short"),
                RequirementDefinition {
                    id: String::from("cordage_short"),
                    components: vec![vec![
                        component("string_6", 1, false),
                        component("cordage_6_leather", 2, false),
                    ]],
                    ..RequirementDefinition::default()
                },
            ),
            (
                String::from("cordage"),
                RequirementDefinition {
                    id: String::from("cordage"),
                    components: vec![vec![
                        component("string_36", 1, false),
                        component("cordage_short", 6, true),
                    ]],
                    ..RequirementDefinition::default()
                },
            ),
        ]);
        let recipe = RecipeDefinition {
            id: String::from("test_recipe"),
            components: vec![vec![
                component("string_6", 20, false),
                component("cordage", 2, true),
            ]],
            ..RecipeDefinition::default()
        };
        let (resolved, references) =
            resolve_components_with_references(&recipe, &requirements).expect("LIST expansion");
        assert_eq!(
            resolved,
            vec![vec![
                component("string_6", 12, false),
                component("string_36", 2, false),
                component("cordage_6_leather", 24, false),
            ]]
        );
        assert_eq!(
            references,
            BTreeSet::from([String::from("cordage"), String::from("cordage_short")])
        );
    }

    #[test]
    fn tool_lists_inline_scale_deduplicate_and_expand_subtypes_like_pinned_cdda() {
        let tool = |type_id: &str, count: i32, requirement_list: bool| ToolRequirement {
            type_id: type_id.to_owned(),
            count,
            requirement_list,
        };
        let requirements = BTreeMap::from([(
            String::from("surface_heat"),
            RequirementDefinition {
                id: String::from("surface_heat"),
                tools: vec![vec![tool("hotplate", 3, false), tool("fire", -1, false)]],
                ..RequirementDefinition::default()
            },
        )]);
        let subtype_replacements = BTreeMap::from([(
            String::from("hotplate"),
            vec![String::from("chemistry_set"), String::from("fake_stove")],
        )]);
        let recipe = RecipeDefinition {
            id: String::from("test_recipe"),
            tools: vec![vec![
                tool("hotplate", 5, false),
                tool("surface_heat", 2, true),
            ]],
            ..RecipeDefinition::default()
        };
        let (resolved, references) =
            resolve_tools_with_references(&recipe, &requirements, &subtype_replacements)
                .expect("tool LIST expansion");
        assert_eq!(
            resolved,
            vec![vec![
                tool("hotplate", 5, false),
                tool("chemistry_set", 5, false),
                tool("fake_stove", 5, false),
                tool("fire", -1, false),
            ]]
        );
        assert_eq!(references, BTreeSet::from([String::from("surface_heat")]));
    }

    #[test]
    fn cyclic_component_lists_fail_closed() {
        let listed = |type_id: &str| ComponentRequirement {
            type_id: type_id.to_owned(),
            count: 1,
            requirement_list: true,
            recoverable: true,
        };
        let requirements = BTreeMap::from([
            (
                String::from("a"),
                RequirementDefinition {
                    id: String::from("a"),
                    components: vec![vec![listed("b")]],
                    ..RequirementDefinition::default()
                },
            ),
            (
                String::from("b"),
                RequirementDefinition {
                    id: String::from("b"),
                    components: vec![vec![listed("a")]],
                    ..RequirementDefinition::default()
                },
            ),
        ]);
        let recipe = RecipeDefinition {
            id: String::from("cycle"),
            components: vec![vec![listed("a")]],
            ..RecipeDefinition::default()
        };
        assert!(matches!(
            resolve_components(&recipe, &requirements),
            Err(RecipeRegistryError::CyclicRequirement { .. })
        ));
    }

    #[test]
    fn skills_accept_single_and_multiple_upstream_shapes() {
        assert_eq!(
            parse_skill_map(&serde_json::json!(["survival", 3]), "test", "skills").expect("single")
                ["survival"],
            3
        );
        let multiple = parse_skill_map(
            &serde_json::json!([["survival", 2], ["fabrication", 1]]),
            "test",
            "skills",
        )
        .expect("multiple");
        assert_eq!(multiple.len(), 2);
    }
}
