use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ItemDefinition, ModCatalog, ModCatalogError, SelectedContentFile};

const DEFAULT_SPECIFIC_HEAT_LIQUID_UJ_PER_G_K: i64 = 4_186_000;
const DEFAULT_SPECIFIC_HEAT_SOLID_UJ_PER_G_K: i64 = 2_108_000;
const DEFAULT_LATENT_HEAT_UJ_PER_G: i64 = 334_000_000;
const THERMAL_SCALE: f64 = 1_000_000.0;
const THERMAL_FIELDS: [&str; 4] = [
    "specific_heat_liquid",
    "specific_heat_solid",
    "latent_heat",
    "freezing_point",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterialThermalDefinition {
    pub id: String,
    /// Finalized damage-type resistance in thousandths of an upstream point.
    pub damage_resistance_milli: BTreeMap<String, i64>,
    pub specific_heat_liquid_microjoules_per_gram_kelvin: i64,
    pub specific_heat_solid_microjoules_per_gram_kelvin: i64,
    pub latent_heat_microjoules_per_gram: i64,
    pub freezing_point_millikelvin: i32,
    pub source: String,
}

impl Default for MaterialThermalDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            damage_resistance_milli: BTreeMap::new(),
            specific_heat_liquid_microjoules_per_gram_kelvin:
                DEFAULT_SPECIFIC_HEAT_LIQUID_UJ_PER_G_K,
            specific_heat_solid_microjoules_per_gram_kelvin: DEFAULT_SPECIFIC_HEAT_SOLID_UJ_PER_G_K,
            latent_heat_microjoules_per_gram: DEFAULT_LATENT_HEAT_UJ_PER_G,
            freezing_point_millikelvin: 273_150,
            source: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComestibleThermalProperties {
    pub specific_heat_liquid_microjoules_per_gram_kelvin: i64,
    pub specific_heat_solid_microjoules_per_gram_kelvin: i64,
    pub latent_heat_microjoules_per_gram: i64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MaterialRegistry {
    materials: BTreeMap<String, MaterialThermalDefinition>,
}

#[derive(Clone)]
struct RawMaterial {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl MaterialRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, MaterialRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(MaterialRegistryError::Catalog)?;
        let mut pending = read_materials(content_root.as_ref(), files)?;
        let mut materials = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(MaterialRegistryError::InternalQueue)?;
                if load_one(&raw, &mut materials, &mut abstracts)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(MaterialRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| {
                            raw.object
                                .get("id")
                                .or_else(|| raw.object.get("abstract"))
                                .and_then(Value::as_str)
                        })
                        .map(str::to_owned)
                        .collect(),
                ));
            }
        }
        Ok(Self { materials })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.materials.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MaterialThermalDefinition> {
        self.materials.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &MaterialThermalDefinition)> {
        self.materials
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    /// Reproduces `Item_factory::finalize_pre`: each material property is
    /// accumulated and divided as an upstream `float`, then quantized once for
    /// deterministic protocol storage. An empty material mix deliberately
    /// remains the separately represented materialless temperature family.
    pub fn comestible_thermal_properties(
        &self,
        item: &ItemDefinition,
    ) -> Result<Option<ComestibleThermalProperties>, MaterialRegistryError> {
        if item.materials.is_empty() {
            return Ok(None);
        }
        let mut total_portions = 0_i64;
        let mut liquid = 0.0_f32;
        let mut solid = 0.0_f32;
        let mut latent = 0.0_f32;
        for (material_id, portion) in &item.materials {
            if *portion <= 0 {
                return Err(MaterialRegistryError::InvalidItemMaterialPortion {
                    item: item.id.clone(),
                    material: material_id.clone(),
                });
            }
            let material = self.materials.get(material_id).ok_or_else(|| {
                MaterialRegistryError::UnknownItemMaterial {
                    item: item.id.clone(),
                    material: material_id.clone(),
                }
            })?;
            total_portions = total_portions
                .checked_add(*portion)
                .ok_or(MaterialRegistryError::ThermalOverflow)?;
            let portion =
                i32::try_from(*portion).map_err(|_| MaterialRegistryError::ThermalOverflow)? as f32;
            liquid += material.specific_heat_liquid_microjoules_per_gram_kelvin as f32
                / THERMAL_SCALE as f32
                * portion;
            solid += material.specific_heat_solid_microjoules_per_gram_kelvin as f32
                / THERMAL_SCALE as f32
                * portion;
            latent +=
                material.latent_heat_microjoules_per_gram as f32 / THERMAL_SCALE as f32 * portion;
        }
        let total_portions = i32::try_from(total_portions)
            .map_err(|_| MaterialRegistryError::ThermalOverflow)?
            as f32;
        Ok(Some(ComestibleThermalProperties {
            specific_heat_liquid_microjoules_per_gram_kelvin: quantize_microjoules(
                liquid / total_portions,
            )?,
            specific_heat_solid_microjoules_per_gram_kelvin: quantize_microjoules(
                solid / total_portions,
            )?,
            latent_heat_microjoules_per_gram: quantize_microjoules(latent / total_portions)?,
        }))
    }
}

fn quantize_microjoules(value: f32) -> Result<i64, MaterialRegistryError> {
    let scaled = f64::from(value) * THERMAL_SCALE;
    if !scaled.is_finite() || scaled <= 0.0 || scaled >= i64::MAX as f64 {
        return Err(MaterialRegistryError::ThermalOverflow);
    }
    Ok(scaled.round() as i64)
}

fn read_materials(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawMaterial>, MaterialRegistryError> {
    let mut materials = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| MaterialRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| MaterialRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_material(&file, value, &mut materials)?;
                }
            }
            value => collect_material(&file, value, &mut materials)?,
        }
    }
    Ok(materials)
}

fn collect_material(
    file: &SelectedContentFile,
    value: Value,
    materials: &mut VecDeque<RawMaterial>,
) -> Result<(), MaterialRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("material") {
        return Ok(());
    }
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| MaterialRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    materials.push_back(RawMaterial {
        file: file.clone(),
        object,
    });
    Ok(())
}

fn load_one(
    raw: &RawMaterial,
    materials: &mut BTreeMap<String, MaterialThermalDefinition>,
    abstracts: &mut BTreeMap<String, MaterialThermalDefinition>,
) -> Result<bool, MaterialRegistryError> {
    let concrete_id = raw
        .object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let abstract_id = raw
        .object
        .get("abstract")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let (id, is_abstract) = match (concrete_id, abstract_id) {
        (Some(id), None) => (id, false),
        (None, Some(id)) => (id, true),
        (None, None) | (Some(_), Some(_)) => {
            return Err(MaterialRegistryError::InvalidIdentity);
        }
    };
    let parent = raw.object.get("copy-from").map(|value| {
        value
            .as_str()
            .filter(|parent| !parent.is_empty())
            .ok_or_else(|| invalid(&raw.file.upstream_path, "copy-from"))
    });
    let mut material = if let Some(parent) = parent.transpose()? {
        let Some(base) = materials.get(parent).or_else(|| abstracts.get(parent)) else {
            return Ok(false);
        };
        base.clone()
    } else {
        MaterialThermalDefinition::default()
    };
    material.id = id.to_owned();
    material.source.clone_from(&raw.file.upstream_path);
    let source = format!("{}#{id}", raw.file.upstream_path);
    reject_unsupported_thermal_modifiers(&raw.object, &source)?;
    apply_scaled_positive(
        &raw.object,
        "specific_heat_liquid",
        &mut material.specific_heat_liquid_microjoules_per_gram_kelvin,
        &source,
    )?;
    if let Some(value) = raw.object.get("resist") {
        let resistances = value
            .as_object()
            .ok_or_else(|| invalid(&source, "resist"))?;
        let mut parsed = BTreeMap::new();
        for (damage_type, value) in resistances {
            if damage_type.is_empty()
                || damage_type.len() > 512
                || damage_type.chars().any(char::is_control)
            {
                return Err(invalid(&source, "resist"));
            }
            let resistance = scaled_i64(value, 1_000.0, &source, "resist")?;
            if resistance < 0 {
                return Err(invalid(&source, "resist"));
            }
            parsed.insert(damage_type.clone(), resistance);
        }
        material.damage_resistance_milli = parsed;
    }
    apply_scaled_positive(
        &raw.object,
        "specific_heat_solid",
        &mut material.specific_heat_solid_microjoules_per_gram_kelvin,
        &source,
    )?;
    apply_scaled_positive(
        &raw.object,
        "latent_heat",
        &mut material.latent_heat_microjoules_per_gram,
        &source,
    )?;
    if let Some(value) = raw.object.get("freezing_point") {
        let celsius = scaled_i32(value, 1_000.0, &source, "freezing_point")?;
        material.freezing_point_millikelvin = 273_150_i32
            .checked_add(celsius)
            .filter(|temperature| *temperature > 0)
            .ok_or_else(|| invalid(&source, "freezing_point"))?;
    }
    if is_abstract {
        abstracts.insert(id.to_owned(), material);
    } else {
        materials.insert(id.to_owned(), material);
    }
    Ok(true)
}

fn reject_unsupported_thermal_modifiers(
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), MaterialRegistryError> {
    for modifier in ["relative", "proportional"] {
        let Some(value) = object.get(modifier) else {
            continue;
        };
        let fields = value.as_object().ok_or_else(|| invalid(source, modifier))?;
        if let Some(field) = THERMAL_FIELDS
            .iter()
            .find(|field| fields.contains_key(**field))
        {
            return Err(invalid(source, &format!("{modifier}.{field}")));
        }
    }
    Ok(())
}

fn apply_scaled_positive(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i64,
    source: &str,
) -> Result<(), MaterialRegistryError> {
    if let Some(value) = object.get(field) {
        let parsed = scaled_i64(value, THERMAL_SCALE, source, field)?;
        if parsed <= 0 {
            return Err(invalid(source, field));
        }
        *target = parsed;
    }
    Ok(())
}

fn scaled_i64(
    value: &Value,
    scale: f64,
    source: &str,
    field: &str,
) -> Result<i64, MaterialRegistryError> {
    let value = value.as_f64().ok_or_else(|| invalid(source, field))?;
    let scaled = value * scale;
    if !scaled.is_finite()
        || scaled < i64::MIN as f64
        || scaled >= i64::MAX as f64
        || (scaled - scaled.round()).abs() > 1.0e-6
    {
        return Err(invalid(source, field));
    }
    Ok(scaled.round() as i64)
}

fn scaled_i32(
    value: &Value,
    scale: f64,
    source: &str,
    field: &str,
) -> Result<i32, MaterialRegistryError> {
    i32::try_from(scaled_i64(value, scale, source, field)?).map_err(|_| invalid(source, field))
}

fn invalid(source: &str, field: &str) -> MaterialRegistryError {
    MaterialRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum MaterialRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    InvalidIdentity,
    InvalidItemMaterialPortion { item: String, material: String },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    ThermalOverflow,
    UnknownItemMaterial { item: String, material: String },
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for MaterialRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "material mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("material inheritance queue underflowed"),
            Self::InvalidDefinition(source) => {
                write!(
                    formatter,
                    "material definition is not an object in {source}"
                )
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid material field {field} in {source}")
            }
            Self::InvalidIdentity => formatter.write_str("material definition has no stable ID"),
            Self::InvalidItemMaterialPortion { item, material } => {
                write!(
                    formatter,
                    "item {item} has an invalid {material} material portion"
                )
            }
            Self::Io(path, error) => write!(
                formatter,
                "material registry I/O failed for {path}: {error}"
            ),
            Self::Json(path, error) => write!(
                formatter,
                "material registry JSON failed for {path}: {error}"
            ),
            Self::ThermalOverflow => formatter.write_str("material thermal arithmetic overflowed"),
            Self::UnknownItemMaterial { item, material } => {
                write!(
                    formatter,
                    "item {item} references unknown material {material}"
                )
            }
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved material inheritance: {}",
                    ids.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for MaterialRegistryError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;

    fn raw(id: &str, parent: Option<&str>, liquid: Option<f64>) -> RawMaterial {
        let mut object = serde_json::json!({"type": "material", "id": id})
            .as_object()
            .expect("fixture is an object")
            .clone();
        if let Some(parent) = parent {
            object.insert(String::from("copy-from"), Value::String(parent.to_owned()));
        }
        if let Some(liquid) = liquid {
            object.insert(String::from("specific_heat_liquid"), Value::from(liquid));
        }
        RawMaterial {
            file: SelectedContentFile {
                owner: String::from("dda"),
                upstream_path: String::from("data/test/materials.json"),
                destination: String::from("data/test/materials.json"),
            },
            object,
        }
    }

    #[test]
    fn thermal_inheritance_resets_without_copy_and_rejects_sub_micro_precision() {
        let mut materials = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        assert!(
            load_one(
                &raw("base", None, Some(1.2)),
                &mut materials,
                &mut abstracts
            )
            .expect("base")
        );
        assert!(
            load_one(
                &raw("child", Some("base"), None),
                &mut materials,
                &mut abstracts
            )
            .expect("child")
        );
        assert_eq!(
            materials["child"].specific_heat_liquid_microjoules_per_gram_kelvin,
            1_200_000
        );
        assert!(
            load_one(&raw("child", None, None), &mut materials, &mut abstracts)
                .expect("replacement")
        );
        assert_eq!(
            materials["child"].specific_heat_liquid_microjoules_per_gram_kelvin,
            DEFAULT_SPECIFIC_HEAT_LIQUID_UJ_PER_G_K
        );
        assert!(
            load_one(
                &raw("too_precise", None, Some(1.000_000_1)),
                &mut materials,
                &mut abstracts
            )
            .is_err()
        );

        let mut template = raw("generic", None, Some(1.7));
        template.object.remove("id");
        template.object.insert(
            String::from("abstract"),
            Value::String(String::from("generic")),
        );
        assert!(load_one(&template, &mut materials, &mut abstracts).expect("abstract template"));
        assert!(!materials.contains_key("generic"));
        assert!(
            load_one(
                &raw("concrete", Some("generic"), None),
                &mut materials,
                &mut abstracts
            )
            .expect("concrete abstract child")
        );
        assert_eq!(
            materials["concrete"].specific_heat_liquid_microjoules_per_gram_kelvin,
            1_700_000
        );

        let mut relative = raw("relative", None, None);
        relative.object.insert(
            String::from("relative"),
            serde_json::json!({"specific_heat_liquid": 1.0}),
        );
        assert!(load_one(&relative, &mut materials, &mut abstracts).is_err());
        assert!(
            scaled_i64(&Value::from(i64::MAX), 1.0, "test", "upper_bound").is_err(),
            "f64's 2^63 alias must not saturate into an accepted i64"
        );
        let mut ambiguous = raw("ambiguous", None, None);
        ambiguous.object.insert(
            String::from("abstract"),
            Value::String(String::from("ambiguous")),
        );
        assert!(load_one(&ambiguous, &mut materials, &mut abstracts).is_err());
    }

    #[test]
    fn pinned_material_profiles_and_weighted_saline_are_exact() {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/cdda-content-manifest.json");
        let manifest = ContentManifest::load(&manifest_path).expect("manifest");
        let root = manifest_path.parent().expect("manifest parent");
        let catalog = ModCatalog::load(&manifest, root).expect("mod catalog");
        let enabled = catalog.recommended_new_world().expect("recommended mods");
        let materials = MaterialRegistry::load_selected(&manifest, root, &catalog, &enabled)
            .expect("materials");
        let items =
            crate::ItemRegistry::load_selected(&manifest, root, &catalog, &enabled).expect("items");

        assert_eq!(materials.len(), 201);
        assert!(materials.get("generic_polymer_resin").is_none());
        assert_eq!(
            materials.get("drug_filler").expect("drug filler"),
            &MaterialThermalDefinition {
                id: String::from("drug_filler"),
                specific_heat_liquid_microjoules_per_gram_kelvin: 1_500_000,
                specific_heat_solid_microjoules_per_gram_kelvin: 1_200_000,
                latent_heat_microjoules_per_gram: 10_000_000,
                freezing_point_millikelvin: 273_150,
                source: String::from("data/json/materials.json"),
            }
        );
        assert_eq!(
            materials
                .comestible_thermal_properties(items.get("caff_gum").expect("caff gum"))
                .expect("caff profile"),
            Some(ComestibleThermalProperties {
                specific_heat_liquid_microjoules_per_gram_kelvin: 1_500_000,
                specific_heat_solid_microjoules_per_gram_kelvin: 1_200_000,
                latent_heat_microjoules_per_gram: 10_000_000,
            })
        );
        assert_eq!(
            materials
                .comestible_thermal_properties(items.get("saline").expect("saline"))
                .expect("saline profile"),
            Some(ComestibleThermalProperties {
                specific_heat_liquid_microjoules_per_gram_kelvin: 4_156_246,
                specific_heat_solid_microjoules_per_gram_kelvin: 2_097_308,
                latent_heat_microjoules_per_gram: 330_092_987,
            })
        );
    }
}
