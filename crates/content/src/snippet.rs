use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

/// One entry in the pinned snippet library. Identified and anonymous entries
/// remain distinct because upstream selects all identified weights before all
/// anonymous weights, independently of their interleaved file order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptionSnippetChoice {
    pub id: Option<String>,
    pub text: String,
    pub weight: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DescriptionSnippetCategory {
    pub id: String,
    pub identified: Vec<DescriptionSnippetChoice>,
    pub anonymous: Vec<DescriptionSnippetChoice>,
}

impl DescriptionSnippetCategory {
    pub fn choices(&self) -> impl Iterator<Item = &DescriptionSnippetChoice> {
        self.identified.iter().chain(&self.anonymous)
    }

    #[must_use]
    pub fn total_weight(&self) -> Option<u64> {
        self.choices()
            .try_fold(0_u64, |total, choice| total.checked_add(choice.weight))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DescriptionSnippetRegistry {
    categories: BTreeMap<String, DescriptionSnippetCategory>,
}

impl DescriptionSnippetRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, DescriptionSnippetRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(DescriptionSnippetRegistryError::Catalog)?;
        Self::load_files(manifest, content_root.as_ref(), files)
    }

    fn load_files(
        manifest: &ContentManifest,
        content_root: &Path,
        files: Vec<SelectedContentFile>,
    ) -> Result<Self, DescriptionSnippetRegistryError> {
        let mut categories = BTreeMap::new();
        let mut identified = BTreeSet::new();
        load_english_names(manifest, content_root, &mut categories)?;
        for file in files {
            let bytes = fs::read(content_root.join(&file.destination)).map_err(|error| {
                DescriptionSnippetRegistryError::Io(file.destination.clone(), error)
            })?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
                DescriptionSnippetRegistryError::Json(file.destination.clone(), error)
            })?;
            match value {
                Value::Array(values) => {
                    for value in values {
                        load_snippet_value(
                            value,
                            &file.upstream_path,
                            &mut categories,
                            &mut identified,
                        )?;
                    }
                }
                value => load_snippet_value(
                    value,
                    &file.upstream_path,
                    &mut categories,
                    &mut identified,
                )?,
            }
        }
        Ok(Self { categories })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.categories.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.categories.is_empty()
    }

    #[must_use]
    pub fn get(&self, category: &str) -> Option<&DescriptionSnippetCategory> {
        self.categories.get(category)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &DescriptionSnippetCategory)> {
        self.categories
            .iter()
            .map(|(id, category)| (id.as_str(), category))
    }
}

fn load_english_names(
    manifest: &ContentManifest,
    content_root: &Path,
    categories: &mut BTreeMap<String, DescriptionSnippetCategory>,
) -> Result<(), DescriptionSnippetRegistryError> {
    // Pinned `snippet_library::clear_snippets` loads the active language's
    // name library before ordinary snippet JSON. This port's initial language
    // scope is English, so reproduce that phase from the vendored provenance
    // entry instead of silently treating name tags as unknown literals.
    const NAMES_DESTINATION: &str = "cdda/data/names/en.json";
    let entry = manifest
        .entries
        .iter()
        .find(|entry| entry.destination == NAMES_DESTINATION)
        .ok_or(DescriptionSnippetRegistryError::MissingEnglishNames)?;
    let bytes = fs::read(content_root.join(&entry.destination))
        .map_err(|error| DescriptionSnippetRegistryError::Io(entry.destination.clone(), error))?;
    let values: Vec<Value> = serde_json::from_slice(&bytes)
        .map_err(|error| DescriptionSnippetRegistryError::Json(entry.destination.clone(), error))?;
    for value in values {
        load_name_value(value, &entry.upstream_path, categories)?;
    }
    for category_id in [
        "<male_backer_name>",
        "<female_backer_name>",
        "<male_given_name>",
        "<female_given_name>",
        "<family_name>",
        "<nick_name>",
        "<city_name>",
        "<world_name>",
    ] {
        let category = categories.entry(category_id.to_owned()).or_insert_with(|| {
            DescriptionSnippetCategory {
                id: category_id.to_owned(),
                ..DescriptionSnippetCategory::default()
            }
        });
        if category.anonymous.is_empty() {
            category.anonymous.push(DescriptionSnippetChoice {
                id: None,
                text: String::from("Tom"),
                weight: 1,
            });
        }
    }
    Ok(())
}

fn load_name_value(
    value: Value,
    source: &str,
    categories: &mut BTreeMap<String, DescriptionSnippetCategory>,
) -> Result<(), DescriptionSnippetRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| DescriptionSnippetRegistryError::InvalidNameDefinition(source.to_owned()))?;
    if object.keys().any(|field| {
        !field.starts_with("//") && !matches!(field.as_str(), "usage" | "gender" | "name")
    }) {
        return Err(DescriptionSnippetRegistryError::InvalidNameDefinition(
            source.to_owned(),
        ));
    }
    let usage = object
        .get("usage")
        .and_then(Value::as_str)
        .ok_or_else(|| DescriptionSnippetRegistryError::InvalidNameDefinition(source.to_owned()))?;
    let category_ids: &[&str] = match usage {
        "backer" => match object.get("gender").and_then(Value::as_str) {
            Some("male") => &["<male_backer_name>"],
            Some("female") => &["<female_backer_name>"],
            Some("unisex") => &["<male_backer_name>", "<female_backer_name>"],
            _ => {
                return Err(DescriptionSnippetRegistryError::InvalidNameDefinition(
                    source.to_owned(),
                ));
            }
        },
        "given" => match object.get("gender").and_then(Value::as_str) {
            Some("male") => &["<male_given_name>"],
            Some("female") => &["<female_given_name>"],
            Some("unisex") => &["<male_given_name>", "<female_given_name>"],
            _ => {
                return Err(DescriptionSnippetRegistryError::InvalidNameDefinition(
                    source.to_owned(),
                ));
            }
        },
        "family" => &["<family_name>"],
        "nick" => &["<nick_name>"],
        "city" => &["<city_name>"],
        "world" => &["<world_name>"],
        _ => {
            return Err(DescriptionSnippetRegistryError::InvalidNameDefinition(
                source.to_owned(),
            ));
        }
    };
    let names = match object.get("name") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                DescriptionSnippetRegistryError::InvalidNameDefinition(source.to_owned())
            })?,
        _ => {
            return Err(DescriptionSnippetRegistryError::InvalidNameDefinition(
                source.to_owned(),
            ));
        }
    };
    if names.iter().any(|name| name.is_empty()) {
        return Err(DescriptionSnippetRegistryError::InvalidNameDefinition(
            source.to_owned(),
        ));
    }
    for category_id in category_ids {
        let category = categories
            .entry((*category_id).to_owned())
            .or_insert_with(|| DescriptionSnippetCategory {
                id: (*category_id).to_owned(),
                ..DescriptionSnippetCategory::default()
            });
        category
            .anonymous
            .extend(names.iter().map(|name| DescriptionSnippetChoice {
                id: None,
                text: (*name).to_owned(),
                weight: 1,
            }));
    }
    Ok(())
}

fn load_snippet_value(
    value: Value,
    source: &str,
    categories: &mut BTreeMap<String, DescriptionSnippetCategory>,
    identified: &mut BTreeSet<String>,
) -> Result<(), DescriptionSnippetRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("snippet") {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| DescriptionSnippetRegistryError::InvalidDefinition(source.to_owned()))?;
    if object.keys().any(|field| {
        !field.starts_with("//")
            && !matches!(
                field.as_str(),
                "type"
                    | "category"
                    | "text"
                    | "override"
                    | "id"
                    | "weight"
                    | "effect_on_examine"
                    | "name"
            )
    }) {
        return Err(DescriptionSnippetRegistryError::InvalidDefinition(
            source.to_owned(),
        ));
    }
    let category_id = object
        .get("category")
        .and_then(Value::as_str)
        .filter(|category| !category.is_empty())
        .ok_or_else(|| DescriptionSnippetRegistryError::InvalidCategory(source.to_owned()))?;
    let override_existing = match object.get("override") {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            return Err(DescriptionSnippetRegistryError::InvalidDefinition(
                source.to_owned(),
            ));
        }
    };
    let category =
        categories
            .entry(category_id.to_owned())
            .or_insert_with(|| DescriptionSnippetCategory {
                id: category_id.to_owned(),
                ..DescriptionSnippetCategory::default()
            });
    if override_existing {
        category.identified.clear();
        category.anonymous.clear();
    }
    match object.get("text") {
        Some(Value::Array(values)) => {
            for value in values {
                add_choice(value, source, category, identified)?;
            }
        }
        Some(_) => add_choice(&Value::Object(object.clone()), source, category, identified)?,
        None => {
            return Err(DescriptionSnippetRegistryError::InvalidDefinition(
                source.to_owned(),
            ));
        }
    }
    Ok(())
}

fn add_choice(
    value: &Value,
    source: &str,
    category: &mut DescriptionSnippetCategory,
    identified: &mut BTreeSet<String>,
) -> Result<(), DescriptionSnippetRegistryError> {
    let choice = match value {
        Value::String(text) => DescriptionSnippetChoice {
            id: None,
            text: text.clone(),
            weight: 1,
        },
        Value::Object(object) => parse_object_choice(object, source)?,
        _ => {
            return Err(DescriptionSnippetRegistryError::InvalidDefinition(
                source.to_owned(),
            ));
        }
    };
    if let Some(id) = &choice.id {
        if !identified.insert(id.clone()) {
            return Err(DescriptionSnippetRegistryError::DuplicateId(id.clone()));
        }
        category.identified.push(choice);
    } else {
        category.anonymous.push(choice);
    }
    category
        .total_weight()
        .ok_or_else(|| DescriptionSnippetRegistryError::WeightOverflow(category.id.clone()))?;
    Ok(())
}

fn parse_object_choice(
    object: &Map<String, Value>,
    source: &str,
) -> Result<DescriptionSnippetChoice, DescriptionSnippetRegistryError> {
    if object.keys().any(|field| {
        !field.starts_with("//")
            && !matches!(
                field.as_str(),
                "type"
                    | "category"
                    | "text"
                    | "override"
                    | "id"
                    | "weight"
                    | "effect_on_examine"
                    | "name"
            )
    }) {
        return Err(DescriptionSnippetRegistryError::InvalidDefinition(
            source.to_owned(),
        ));
    }
    let id = object
        .get("id")
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    DescriptionSnippetRegistryError::InvalidDefinition(source.to_owned())
                })
        })
        .transpose()?;
    let text = object
        .get("text")
        .and_then(english_text)
        .ok_or_else(|| DescriptionSnippetRegistryError::InvalidDefinition(source.to_owned()))?;
    let weight = match object.get("weight") {
        None => 1,
        Some(Value::Number(number)) => number
            .as_u64()
            .ok_or_else(|| DescriptionSnippetRegistryError::InvalidDefinition(source.to_owned()))?,
        Some(_) => {
            return Err(DescriptionSnippetRegistryError::InvalidDefinition(
                source.to_owned(),
            ));
        }
    };
    Ok(DescriptionSnippetChoice { id, text, weight })
}

fn english_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|field| object.get(field).and_then(Value::as_str))
            .map(str::to_owned),
        _ => None,
    }
}

#[derive(Debug)]
pub enum DescriptionSnippetRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    InvalidDefinition(String),
    InvalidNameDefinition(String),
    InvalidCategory(String),
    MissingEnglishNames,
    DuplicateId(String),
    WeightOverflow(String),
}

impl fmt::Display for DescriptionSnippetRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "snippet mod selection failed: {error}"),
            Self::Io(path, error) => write!(formatter, "snippet read failed for {path}: {error}"),
            Self::Json(path, error) => write!(formatter, "snippet JSON failed for {path}: {error}"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "invalid snippet definition in {source}")
            }
            Self::InvalidNameDefinition(source) => {
                write!(formatter, "invalid English name definition in {source}")
            }
            Self::InvalidCategory(source) => {
                write!(formatter, "invalid snippet category in {source}")
            }
            Self::MissingEnglishNames => {
                write!(formatter, "English names are absent from the manifest")
            }
            Self::DuplicateId(id) => write!(formatter, "duplicate snippet id {id}"),
            Self::WeightOverflow(category) => {
                write!(formatter, "snippet weight overflow in category {category}")
            }
        }
    }
}

impl std::error::Error for DescriptionSnippetRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_categories_preserve_anonymous_choice_order() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let manifest_path = repository.join(crate::DEFAULT_MANIFEST_PATH);
        let manifest = ContentManifest::load(&manifest_path).expect("manifest should load");
        let root = manifest_path
            .parent()
            .expect("manifest should have a parent");
        let catalog = ModCatalog::load(&manifest, root).expect("mods should load");
        let enabled = catalog
            .recommended_new_world()
            .expect("mods should resolve");
        let snippets =
            DescriptionSnippetRegistry::load_selected(&manifest, root, &catalog, &enabled)
                .expect("selected snippets should load");
        let saints = snippets
            .get("<catholic_saints>")
            .expect("Catholic saints should exist");
        assert!(saints.identified.is_empty());
        assert_eq!(saints.anonymous.len(), 14);
        assert_eq!(saints.total_weight(), Some(14));
        assert_eq!(
            saints.anonymous[0].text,
            "St. Michael the Archangel, patron saint of soldiers, police officers, and doctors, pray for us"
        );
        assert_eq!(
            snippets
                .get("<lt>")
                .expect("literal less-than category")
                .anonymous[0]
                .text,
            "<"
        );
        let family_names = snippets
            .get("<family_name>")
            .expect("the pinned English name library should be loaded");
        assert_eq!(family_names.anonymous.len(), 3_045);
        assert_eq!(family_names.anonymous[0].text, "Aaron");
        assert_eq!(
            snippets
                .get("<female_given_name>")
                .expect("female given names should be loaded")
                .anonymous
                .len(),
            4_275
        );
        assert_eq!(
            snippets
                .get("<male_given_name>")
                .expect("male given names should be loaded")
                .anonymous
                .len(),
            1_219
        );
        assert_eq!(
            snippets
                .get("<world_name>")
                .expect("world names should be loaded")
                .anonymous
                .len(),
            20_900
        );
    }

    #[test]
    fn identified_entries_select_before_anonymous_entries_and_overrides_clear_only_category() {
        let mut categories = BTreeMap::new();
        let mut identified = BTreeSet::new();
        load_snippet_value(
            serde_json::json!({
                "type": "snippet",
                "category": "<test>",
                "text": [
                    "anonymous first",
                    { "id": "identified", "text": "identified second", "weight": 3 }
                ]
            }),
            "test",
            &mut categories,
            &mut identified,
        )
        .expect("mixed category should load");
        let category = &categories["<test>"];
        assert_eq!(
            category
                .choices()
                .next()
                .and_then(|choice| choice.id.as_deref()),
            Some("identified")
        );
        assert_eq!(category.total_weight(), Some(4));
        load_snippet_value(
            serde_json::json!({
                "type": "snippet",
                "category": "<test>",
                "override": true,
                "text": "replacement"
            }),
            "test",
            &mut categories,
            &mut identified,
        )
        .expect("override should load");
        assert_eq!(categories["<test>"].anonymous[0].text, "replacement");
        assert!(categories["<test>"].identified.is_empty());
        assert!(identified.contains("identified"));
    }
}
