use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_VEHICLE_PART_DEFINITIONS: usize = 16_384;
pub const MAX_VEHICLE_PROTOTYPES: usize = 8_192;
pub const MAX_VEHICLE_GROUPS: usize = 8_192;
pub const MAX_VEHICLE_PARTS_PER_PROTOTYPE: usize = 4_096;
pub const MAX_VEHICLE_GROUP_ENTRIES: usize = 4_096;

const PART_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "name",
    "item",
    "location",
    "durability",
    "flags",
    "variants",
    "variants_bases",
    "extend",
    "delete",
    "relative",
    "proportional",
];
const PROTOTYPE_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "name",
    "blueprint",
    "parts",
    "items",
    "zones",
    "extend",
    "delete",
    "relative",
    "proportional",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehiclePartVariantDefinition {
    pub variant_id: String,
    /// Pinned `symbols` string. Its directional indexing is preserved for the
    /// runtime renderer rather than collapsed to one invented glyph.
    pub symbols: String,
    pub broken_symbols: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehiclePartDefinition {
    pub id: String,
    pub name: String,
    pub item_id: String,
    pub location: String,
    pub durability: u32,
    pub flags: BTreeSet<String>,
    pub variants: Vec<VehiclePartVariantDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
    pub abstract_definition: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehiclePrototypePartDefinition {
    pub mount_x: i16,
    pub mount_y: i16,
    pub part_id: String,
    pub variant_id: String,
    pub fuel_item_id: String,
    pub with_ammo_percent: u8,
    pub ammo_type_ids: Vec<String>,
    pub ammo_quantity_minimum: i32,
    pub ammo_quantity_maximum: i32,
    pub tool_item_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehiclePrototypeDefinition {
    pub id: String,
    pub name: String,
    /// Exact upstream order. `vehicle::init_state` consumes randomness while
    /// iterating this sequence, so sorting parts would drift future replays.
    pub parts: Vec<VehiclePrototypePartDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
    pub abstract_definition: bool,
}

impl VehiclePrototypeDefinition {
    #[must_use]
    pub fn has_runtime_static_lifecycle(&self) -> bool {
        !self.abstract_definition && self.unsupported_fields.is_empty() && !self.parts.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehicleGroupEntryDefinition {
    pub prototype_id: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VehicleGroupDefinition {
    pub id: String,
    /// Exact append order from the pinned loader. Vehicle prototypes first add
    /// themselves to their same-ID group and explicit group definitions append.
    pub entries: Vec<VehicleGroupEntryDefinition>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VehicleRegistry {
    parts: BTreeMap<String, VehiclePartDefinition>,
    prototypes: BTreeMap<String, VehiclePrototypeDefinition>,
    groups: BTreeMap<String, VehicleGroupDefinition>,
}

impl VehicleRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, VehicleRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(VehicleRegistryError::Catalog)?;
        let mut raw_parts = RawDefinitionLayers::new();
        let mut raw_prototypes = RawDefinitionLayers::new();
        let mut group_entries = BTreeMap::<String, Vec<VehicleGroupEntryDefinition>>::new();
        for file in files {
            load_file(
                content_root.as_ref(),
                &file,
                &mut raw_parts,
                &mut raw_prototypes,
                &mut group_entries,
            )?;
            if raw_parts.len() > MAX_VEHICLE_PART_DEFINITIONS {
                return Err(VehicleRegistryError::TooManyParts);
            }
            if raw_prototypes.len() > MAX_VEHICLE_PROTOTYPES {
                return Err(VehicleRegistryError::TooManyPrototypes);
            }
            if group_entries.len() > MAX_VEHICLE_GROUPS {
                return Err(VehicleRegistryError::TooManyGroups);
            }
        }

        let resolved_parts = resolve_definitions(&raw_parts)?;
        let resolved_prototypes = resolve_definitions(&raw_prototypes)?;
        let parts = resolved_parts
            .iter()
            .map(|(id, (object, source))| {
                parse_part(id, object, source).map(|part| (id.clone(), part))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let prototypes = resolved_prototypes
            .iter()
            .map(|(id, (object, source))| {
                parse_prototype(id, object, source, &parts).map(|prototype| (id.clone(), prototype))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        for (id, prototype) in &prototypes {
            if !prototype.abstract_definition {
                group_entries.entry(id.clone()).or_default().insert(
                    0,
                    VehicleGroupEntryDefinition {
                        prototype_id: id.clone(),
                        weight: 100,
                    },
                );
            }
        }
        for (group_id, entries) in &group_entries {
            if entries.is_empty() || entries.len() > MAX_VEHICLE_GROUP_ENTRIES {
                return Err(VehicleRegistryError::InvalidDefinition {
                    source: String::from("selected vehicle groups"),
                    field: format!("{group_id}.vehicles"),
                });
            }
            for entry in entries {
                if !prototypes.contains_key(&entry.prototype_id) {
                    return Err(VehicleRegistryError::MissingPrototype {
                        group_id: group_id.clone(),
                        prototype_id: entry.prototype_id.clone(),
                    });
                }
            }
        }
        let groups = group_entries
            .into_iter()
            .map(|(id, entries)| (id.clone(), VehicleGroupDefinition { id, entries }))
            .collect();
        Ok(Self {
            parts,
            prototypes,
            groups,
        })
    }

    #[must_use]
    pub fn part(&self, id: &str) -> Option<&VehiclePartDefinition> {
        self.parts.get(id)
    }

    #[must_use]
    pub fn prototype(&self, id: &str) -> Option<&VehiclePrototypeDefinition> {
        self.prototypes.get(id)
    }

    #[must_use]
    pub fn group(&self, id: &str) -> Option<&VehicleGroupDefinition> {
        self.groups.get(id)
    }

    pub fn parts(&self) -> impl ExactSizeIterator<Item = (&str, &VehiclePartDefinition)> {
        self.parts.iter().map(|(id, part)| (id.as_str(), part))
    }

    pub fn prototypes(&self) -> impl ExactSizeIterator<Item = (&str, &VehiclePrototypeDefinition)> {
        self.prototypes
            .iter()
            .map(|(id, prototype)| (id.as_str(), prototype))
    }

    pub fn groups(&self) -> impl ExactSizeIterator<Item = (&str, &VehicleGroupDefinition)> {
        self.groups.iter().map(|(id, group)| (id.as_str(), group))
    }
}

#[derive(Debug)]
pub enum VehicleRegistryError {
    Catalog(ModCatalogError),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    InvalidDefinition {
        source: String,
        field: String,
    },
    MissingBase {
        source: String,
        base_id: String,
    },
    MissingPart {
        prototype_id: String,
        part_id: String,
    },
    MissingPrototype {
        group_id: String,
        prototype_id: String,
    },
    TooManyParts,
    TooManyPrototypes,
    TooManyGroups,
}

impl fmt::Display for VehicleRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "vehicle mod selection failed: {error}"),
            Self::Io(path, error) => {
                write!(formatter, "vehicle registry I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => write!(
                formatter,
                "vehicle registry JSON failed for {path}: {error}"
            ),
            Self::InvalidDefinition { source, field } => {
                write!(
                    formatter,
                    "invalid vehicle definition field {field} in {source}"
                )
            }
            Self::MissingBase { source, base_id } => {
                write!(
                    formatter,
                    "vehicle definition in {source} copies missing {base_id}"
                )
            }
            Self::MissingPart {
                prototype_id,
                part_id,
            } => {
                write!(
                    formatter,
                    "vehicle {prototype_id} references missing part {part_id}"
                )
            }
            Self::MissingPrototype {
                group_id,
                prototype_id,
            } => {
                write!(
                    formatter,
                    "vehicle group {group_id} references missing prototype {prototype_id}"
                )
            }
            Self::TooManyParts => formatter.write_str("vehicle-part definition bound exceeded"),
            Self::TooManyPrototypes => formatter.write_str("vehicle prototype bound exceeded"),
            Self::TooManyGroups => formatter.write_str("vehicle-group definition bound exceeded"),
        }
    }
}

impl std::error::Error for VehicleRegistryError {}

fn load_file(
    root: &Path,
    file: &SelectedContentFile,
    parts: &mut RawDefinitionLayers,
    prototypes: &mut RawDefinitionLayers,
    groups: &mut BTreeMap<String, Vec<VehicleGroupEntryDefinition>>,
) -> Result<(), VehicleRegistryError> {
    let bytes = fs::read(root.join(&file.destination))
        .map_err(|error| VehicleRegistryError::Io(file.destination.clone(), error))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| VehicleRegistryError::Json(file.destination.clone(), error))?;
    let values = value
        .as_array()
        .map_or_else(|| vec![&value], |values| values.iter().collect());
    for value in values {
        let Some(object) = value.as_object() else {
            continue;
        };
        match object.get("type").and_then(Value::as_str) {
            Some("vehicle_part") => merge_definition(object, file, parts)?,
            Some("vehicle") => merge_definition(object, file, prototypes)?,
            Some("vehicle_group") => load_group(object, file, groups)?,
            _ => {}
        }
    }
    Ok(())
}

type RawDefinitionLayers = BTreeMap<String, Vec<(Map<String, Value>, String, bool)>>;

fn merge_definition(
    object: &Map<String, Value>,
    file: &SelectedContentFile,
    definitions: &mut RawDefinitionLayers,
) -> Result<(), VehicleRegistryError> {
    let (id, abstract_definition) = definition_id(object, file)?;
    definitions.entry(id).or_default().push((
        object.clone(),
        file.upstream_path.clone(),
        abstract_definition,
    ));
    Ok(())
}

fn resolve_definitions(
    definitions: &RawDefinitionLayers,
) -> Result<BTreeMap<String, (Map<String, Value>, String)>, VehicleRegistryError> {
    fn resolve_one(
        id: &str,
        definitions: &RawDefinitionLayers,
        resolved: &mut BTreeMap<String, (Map<String, Value>, String)>,
        active: &mut BTreeSet<String>,
    ) -> Result<(Map<String, Value>, String), VehicleRegistryError> {
        if let Some(result) = resolved.get(id) {
            return Ok(result.clone());
        }
        if !active.insert(id.to_owned()) {
            return Err(VehicleRegistryError::InvalidDefinition {
                source: String::from("selected vehicle definitions"),
                field: format!("copy-from cycle at {id}"),
            });
        }
        let layers = definitions
            .get(id)
            .ok_or_else(|| VehicleRegistryError::MissingBase {
                source: String::from("selected vehicle definitions"),
                base_id: id.to_owned(),
            })?;
        let mut merged = Map::new();
        let mut source = String::new();
        let mut abstract_definition = false;
        for (object, layer_source, layer_is_abstract) in layers {
            if let Some(base_id) = object.get("copy-from").and_then(Value::as_str) {
                let (base, _) = resolve_one(base_id, definitions, resolved, active).map_err(
                    |error| match error {
                        VehicleRegistryError::MissingBase { .. } => {
                            VehicleRegistryError::MissingBase {
                                source: layer_source.clone(),
                                base_id: base_id.to_owned(),
                            }
                        }
                        error => error,
                    },
                )?;
                merged = base;
            }
            for (field, value) in object {
                if !matches!(
                    field.as_str(),
                    "extend" | "delete" | "relative" | "proportional"
                ) {
                    merged.insert(field.clone(), value.clone());
                }
            }
            apply_array_patch_raw(&mut merged, object.get("extend"), false, layer_source)?;
            apply_array_patch_raw(&mut merged, object.get("delete"), true, layer_source)?;
            apply_numeric_patch(&mut merged, object.get("relative"), false, layer_source)?;
            apply_numeric_patch(&mut merged, object.get("proportional"), true, layer_source)?;
            source.clone_from(layer_source);
            abstract_definition = *layer_is_abstract;
        }
        merged.remove("id");
        merged.remove("abstract");
        merged.insert(
            if abstract_definition {
                "abstract"
            } else {
                "id"
            }
            .to_owned(),
            Value::String(id.to_owned()),
        );
        active.remove(id);
        let result = (merged, source);
        resolved.insert(id.to_owned(), result.clone());
        Ok(result)
    }

    let mut resolved = BTreeMap::new();
    for id in definitions.keys() {
        resolve_one(id, definitions, &mut resolved, &mut BTreeSet::new())?;
    }
    Ok(resolved)
}

fn apply_array_patch_raw(
    merged: &mut Map<String, Value>,
    patch: Option<&Value>,
    delete: bool,
    source: &str,
) -> Result<(), VehicleRegistryError> {
    let Some(patch) = patch else {
        return Ok(());
    };
    let patch = patch
        .as_object()
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: if delete { "delete" } else { "extend" }.to_owned(),
        })?;
    for (field, values) in patch {
        let values = values
            .as_array()
            .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                source: source.to_owned(),
                field: field.clone(),
            })?;
        let target = merged
            .entry(field.clone())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                source: source.to_owned(),
                field: field.clone(),
            })?;
        if delete {
            target.retain(|value| !values.contains(value));
        } else {
            target.extend(values.iter().cloned());
        }
    }
    Ok(())
}

fn apply_numeric_patch(
    merged: &mut Map<String, Value>,
    patch: Option<&Value>,
    proportional: bool,
    source: &str,
) -> Result<(), VehicleRegistryError> {
    let Some(patch) = patch else {
        return Ok(());
    };
    let patch = patch
        .as_object()
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: if proportional {
                "proportional"
            } else {
                "relative"
            }
            .to_owned(),
        })?;
    for (field, operand) in patch {
        let current = merged
            .get(field)
            .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                source: source.to_owned(),
                field: field.clone(),
            })?;
        let result = if !proportional {
            if let (Some(current), Some(operand)) = (current.as_i64(), operand.as_i64()) {
                Value::from(current.checked_add(operand).ok_or_else(|| {
                    VehicleRegistryError::InvalidDefinition {
                        source: source.to_owned(),
                        field: field.clone(),
                    }
                })?)
            } else {
                let result = current
                    .as_f64()
                    .zip(operand.as_f64())
                    .map(|(left, right)| left + right);
                Value::from(result.filter(|result| result.is_finite()).ok_or_else(|| {
                    VehicleRegistryError::InvalidDefinition {
                        source: source.to_owned(),
                        field: field.clone(),
                    }
                })?)
            }
        } else {
            let result = current
                .as_f64()
                .zip(operand.as_f64())
                .map(|(left, right)| left * right);
            Value::from(result.filter(|result| result.is_finite()).ok_or_else(|| {
                VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: field.clone(),
                }
            })?)
        };
        merged.insert(field.clone(), result);
    }
    Ok(())
}

fn load_group(
    object: &Map<String, Value>,
    file: &SelectedContentFile,
    groups: &mut BTreeMap<String, Vec<VehicleGroupEntryDefinition>>,
) -> Result<(), VehicleRegistryError> {
    if object.keys().any(|field| {
        !field.starts_with("//") && !["type", "id", "vehicles"].contains(&field.as_str())
    }) {
        return Err(invalid(file, "vehicle_group"));
    }
    let id = bounded_id(object.get("id"), file, "id")?;
    let entries = object
        .get("vehicles")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(file, "vehicles"))?;
    let target = groups.entry(id).or_default();
    if target.len().saturating_add(entries.len()) > MAX_VEHICLE_GROUP_ENTRIES {
        return Err(VehicleRegistryError::TooManyGroups);
    }
    for entry in entries {
        let pair = entry
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| invalid(file, "vehicles"))?;
        let prototype_id = bounded_id(pair.first(), file, "vehicles.prototype")?;
        let weight = pair
            .get(1)
            .and_then(Value::as_u64)
            .and_then(|weight| u32::try_from(weight).ok())
            .filter(|weight| *weight > 0)
            .ok_or_else(|| invalid(file, "vehicles.weight"))?;
        target.push(VehicleGroupEntryDefinition {
            prototype_id,
            weight,
        });
    }
    Ok(())
}

fn parse_part(
    id: &str,
    object: &Map<String, Value>,
    source: &str,
) -> Result<VehiclePartDefinition, VehicleRegistryError> {
    let abstract_definition = object.contains_key("abstract");
    let name = translated_string(object.get("name")).unwrap_or_else(|| id.to_owned());
    let item_id = object
        .get("item")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let location = object
        .get("location")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let durability = object
        .get("durability")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0);
    let flags = string_set(object.get("flags"), source, "flags")?;
    let variants = parse_variants(object.get("variants"), source)?;
    let unsupported_fields = object
        .keys()
        .filter(|field| !field.starts_with("//") && !PART_FIELDS.contains(&field.as_str()))
        .cloned()
        .collect();
    if !abstract_definition
        && (item_id.is_empty() || location.is_empty() || durability == 0 || variants.is_empty())
    {
        return Err(VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: format!("vehicle_part {id}"),
        });
    }
    Ok(VehiclePartDefinition {
        id: id.to_owned(),
        name,
        item_id,
        location,
        durability,
        flags,
        variants,
        unsupported_fields,
        source: source.to_owned(),
        abstract_definition,
    })
}

fn parse_variants(
    value: Option<&Value>,
    source: &str,
) -> Result<Vec<VehiclePartVariantDefinition>, VehicleRegistryError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: String::from("variants"),
        })?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let object =
                value
                    .as_object()
                    .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                        source: source.to_owned(),
                        field: String::from("variants"),
                    })?;
            let variant_id = object
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let symbols = object
                .get("symbols")
                .and_then(Value::as_str)
                .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: format!("variants[{index}].symbols"),
                })?
                .to_owned();
            let broken_symbols = object
                .get("symbols_broken")
                .and_then(Value::as_str)
                .unwrap_or(&symbols)
                .to_owned();
            if symbols.is_empty()
                || symbols.len() > 32
                || broken_symbols.is_empty()
                || broken_symbols.len() > 32
            {
                return Err(VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: format!("variants[{index}]"),
                });
            }
            Ok(VehiclePartVariantDefinition {
                variant_id,
                symbols,
                broken_symbols,
            })
        })
        .collect()
}

fn parse_prototype(
    id: &str,
    object: &Map<String, Value>,
    source: &str,
    parts: &BTreeMap<String, VehiclePartDefinition>,
) -> Result<VehiclePrototypeDefinition, VehicleRegistryError> {
    let abstract_definition = object.contains_key("abstract");
    let name = translated_string(object.get("name")).unwrap_or_else(|| id.to_owned());
    let mut result_parts = Vec::new();
    if let Some(groups) = object.get("parts") {
        for group in groups
            .as_array()
            .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                source: source.to_owned(),
                field: String::from("parts"),
            })?
        {
            let group =
                group
                    .as_object()
                    .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                        source: source.to_owned(),
                        field: String::from("parts"),
                    })?;
            let mount_x = bounded_i16(group.get("x"), source, "parts.x")?;
            let mount_y = bounded_i16(group.get("y"), source, "parts.y")?;
            for part in group
                .get("parts")
                .and_then(Value::as_array)
                .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: String::from("parts.parts"),
                })?
            {
                result_parts.push(parse_prototype_part(part, mount_x, mount_y, source)?);
                if result_parts.len() > MAX_VEHICLE_PARTS_PER_PROTOTYPE {
                    return Err(VehicleRegistryError::InvalidDefinition {
                        source: source.to_owned(),
                        field: String::from("parts"),
                    });
                }
            }
        }
    }
    for part in &result_parts {
        if !parts.contains_key(&part.part_id) {
            return Err(VehicleRegistryError::MissingPart {
                prototype_id: id.to_owned(),
                part_id: part.part_id.clone(),
            });
        }
    }
    let mut unsupported_fields = object
        .keys()
        .filter(|field| !field.starts_with("//") && !PROTOTYPE_FIELDS.contains(&field.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    for field in ["items", "zones"] {
        if object
            .get(field)
            .is_some_and(|value| !value.is_null() && !value.as_array().is_some_and(Vec::is_empty))
        {
            unsupported_fields.insert(field.to_owned());
        }
    }
    Ok(VehiclePrototypeDefinition {
        id: id.to_owned(),
        name,
        parts: result_parts,
        unsupported_fields,
        source: source.to_owned(),
        abstract_definition,
    })
}

fn parse_prototype_part(
    value: &Value,
    mount_x: i16,
    mount_y: i16,
    source: &str,
) -> Result<VehiclePrototypePartDefinition, VehicleRegistryError> {
    let (part, object) = if let Some(part) = value.as_str() {
        (part, None)
    } else {
        let object = value
            .as_object()
            .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                source: source.to_owned(),
                field: String::from("parts.parts"),
            })?;
        (
            object.get("part").and_then(Value::as_str).ok_or_else(|| {
                VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: String::from("parts.parts.part"),
                }
            })?,
            Some(object),
        )
    };
    let (part_id, variant_id) = part
        .split_once('#')
        .map_or((part, ""), |(part, variant)| (part, variant));
    let object = object.cloned().unwrap_or_default();
    let with_ammo_percent =
        optional_u8(object.get("ammo"), source, "parts.parts.ammo")?.unwrap_or(0);
    if with_ammo_percent > 100 {
        return Err(VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: String::from("parts.parts.ammo"),
        });
    }
    let ammo_type_ids = string_vec(object.get("ammo_types"), source, "parts.parts.ammo_types")?;
    let (ammo_quantity_minimum, ammo_quantity_maximum) =
        optional_i32_range(object.get("ammo_qty"), source, "parts.parts.ammo_qty")?
            .unwrap_or((-1, -1));
    Ok(VehiclePrototypePartDefinition {
        mount_x,
        mount_y,
        part_id: part_id.to_owned(),
        variant_id: variant_id.to_owned(),
        fuel_item_id: object
            .get("fuel")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        with_ammo_percent,
        ammo_type_ids,
        ammo_quantity_minimum,
        ammo_quantity_maximum,
        tool_item_ids: string_vec(object.get("tools"), source, "parts.parts.tools")?,
    })
}

fn definition_id(
    object: &Map<String, Value>,
    file: &SelectedContentFile,
) -> Result<(String, bool), VehicleRegistryError> {
    let abstract_definition = object.contains_key("abstract");
    let id = bounded_id(
        object.get(if abstract_definition {
            "abstract"
        } else {
            "id"
        }),
        file,
        "id",
    )?;
    Ok((id, abstract_definition))
}

fn bounded_id(
    value: Option<&Value>,
    file: &SelectedContentFile,
    field: &str,
) -> Result<String, VehicleRegistryError> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(|| invalid(file, field))
}

fn bounded_i16(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<i16, VehicleRegistryError> {
    value
        .and_then(Value::as_i64)
        .and_then(|value| i16::try_from(value).ok())
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: field.to_owned(),
        })
}

fn optional_u8(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<Option<u8>, VehicleRegistryError> {
    value
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u8::try_from(value).ok())
                .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: field.to_owned(),
                })
        })
        .transpose()
}

fn optional_i32_range(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<Option<(i32, i32)>, VehicleRegistryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Some(single) = value.as_i64().and_then(|value| i32::try_from(value).ok()) {
        return Ok(Some((single, single)));
    }
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: field.to_owned(),
        })?;
    let minimum = values[0]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: field.to_owned(),
        })?;
    let maximum = values[1]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: field.to_owned(),
        })?;
    if minimum > maximum {
        return Err(VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: field.to_owned(),
        });
    }
    Ok(Some((minimum, maximum)))
}

fn string_vec(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<Vec<String>, VehicleRegistryError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
            source: source.to_owned(),
            field: field.to_owned(),
        })?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| VehicleRegistryError::InvalidDefinition {
                    source: source.to_owned(),
                    field: field.to_owned(),
                })
        })
        .collect()
}

fn string_set(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, VehicleRegistryError> {
    string_vec(value, source, field).map(|values| values.into_iter().collect())
}

fn translated_string(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_object()?.get("str")?.as_str().map(str::to_owned))
}

fn invalid(file: &SelectedContentFile, field: &str) -> VehicleRegistryError {
    VehicleRegistryError::InvalidDefinition {
        source: file.upstream_path.clone(),
        field: field.to_owned(),
    }
}
