use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "abstract",
    "copy-from",
    "subtypes",
    "name",
    "description",
    "category",
    "weight",
    "volume",
    "price",
    "price_postapoc",
    "symbol",
    "color",
    "material",
    "flags",
    "qualities",
    "charged_qualities",
    "charges_per_use",
    "power_draw",
    "light",
    "revert_to",
    "sub",
    "melee_damage",
    "to_hit",
    "charges",
    "phase",
    "stackable",
    "stack_size",
    "calories",
    "quench",
    "comestible_type",
    "ammo",
    "ammo_type",
    "tool_ammo",
    "capacity",
    "count",
    "range",
    "dispersion",
    "loudness",
    "damage",
    "ranged_damage",
    "clip_size",
    "required_level",
    "max_level",
    "read_skill",
    "intelligence",
    "time",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemDefinition {
    pub id: String,
    pub subtypes: BTreeSet<String>,
    pub name: String,
    pub description: String,
    pub category: String,
    pub weight_milligrams: i64,
    pub volume_milliliters: i64,
    pub price_cents: i64,
    pub price_postapoc_cents: i64,
    pub symbol: String,
    pub color: String,
    pub materials: BTreeMap<String, i64>,
    pub flags: BTreeSet<String>,
    pub qualities: BTreeMap<String, ItemQualityDefinition>,
    pub charged_qualities: BTreeMap<String, ItemQualityDefinition>,
    pub charges_per_use: i32,
    /// Continuous active-tool draw after inheritance, in integer milliwatts.
    pub power_draw_milliwatts: i64,
    /// Pinned item light emission intensity after inheritance.
    pub light_emission: i32,
    /// Automatic inactive/result type used by active items.
    pub revert_to: String,
    /// Strict transform-action projections. General `use_action` behavior is
    /// still retained as unsupported; this projection admits exact, audited
    /// transform pairs without implying support for other actions.
    pub transform_actions: Vec<ItemTransformActionDefinition>,
    /// Whether the finalized `use_action` value also contains any action that
    /// is not an inline transform. Strict runtime projections use this to
    /// avoid silently discarding link-up, firestarter, or actor behavior.
    pub has_non_transform_use_actions: bool,
    /// Whether an inline transform contains a behavioral field outside the
    /// strict runtime projection. Cosmetic menu/message fields are safe to
    /// omit, but conditions and target mutations must keep a pair fail-closed.
    pub has_unsupported_transform_action_fields: bool,
    pub tool_subtype: String,
    pub melee_damage: BTreeMap<String, f64>,
    /// Finalized pinned melee accuracy. `None` is upstream's default -2.
    pub melee_to_hit: Option<i32>,
    pub charges: i32,
    pub phase: String,
    pub stackable: bool,
    pub stack_size: i32,
    pub calories: i32,
    pub quench: i32,
    pub comestible_type: String,
    pub ammo: BTreeSet<String>,
    pub ammo_types: BTreeSet<String>,
    /// Pinned TOOL fuel/ammunition categories after inheritance and collection
    /// modifiers. Pocket layout remains a separate, currently unsupported
    /// field.
    pub tool_ammunition: BTreeSet<String>,
    /// Legacy/static capacity for MAGAZINE definitions. Modern integral
    /// magazine capacities are also derived from `pocket_data` below.
    pub magazine_capacity: i32,
    /// Strictly parsed MAGAZINE_WELL projections from inherited `pocket_data`.
    /// The full pocket field remains explicitly unsupported until general
    /// containment semantics are implemented.
    pub magazine_wells: Vec<MagazineWellDefinition>,
    /// Strictly parsed integral MAGAZINE ammo restrictions from inherited
    /// `pocket_data`; one entry maps an ammunition category to capacity.
    pub integral_magazines: Vec<BTreeMap<String, i32>>,
    pub count: i32,
    pub range: i32,
    pub dispersion: i32,
    /// Explicit GUN or AMMO loudness. `None` uses the pinned subtype default:
    /// zero for guns and derived range/damage loudness for ammunition.
    pub loudness: Option<i32>,
    pub damage: BTreeMap<String, DamageDefinition>,
    pub ranged_damage: BTreeMap<String, DamageDefinition>,
    pub clip_size: i32,
    /// Pinned BOOK `required_level`: the theoretical skill floor used when a
    /// recipe's own `book_learn` entry does not specify a positive override.
    pub book_required_level: i32,
    /// Pinned BOOK `max_level`: reading cannot raise theory to this level or
    /// beyond.
    pub book_max_level: i32,
    /// Pinned BOOK `read_skill`; empty books are recreational rather than
    /// theoretical-skill training sources.
    pub book_skill: String,
    /// Pinned BOOK intelligence requirement used by reading-time adjustment.
    pub book_intelligence: i32,
    /// Pinned BOOK base reading duration in simulation moves.
    pub book_time_moves: u64,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemQualityDefinition {
    pub level: i32,
    pub speed: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DamageDefinition {
    pub amount: f64,
    pub armor_penetration: f64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MagazineWellDefinition {
    pub default_magazine: String,
    pub item_restrictions: BTreeSet<String>,
    pub flag_restrictions: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ItemTransformActionDefinition {
    pub target: String,
    pub need_charges: i32,
    pub ammo_scale: i32,
    pub moves: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemRegistry {
    items: BTreeMap<String, ItemDefinition>,
    tool_subtype_replacements: BTreeMap<String, Vec<String>>,
    abstract_count: usize,
}

#[derive(Clone)]
struct RawItem {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl ItemRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, ItemRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(ItemRegistryError::Catalog)?;
        let mut pending = read_items(content_root.as_ref(), files)?;
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0_usize;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(ItemRegistryError::InternalQueue)?;
                if load_one(&raw, &mut items, &mut abstracts)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                let unresolved = pending
                    .iter()
                    .take(20)
                    .filter_map(|raw| item_key(&raw.object).ok())
                    .map(|(key, _is_abstract)| key.to_owned())
                    .collect();
                return Err(ItemRegistryError::UnresolvedInheritance(unresolved));
            }
        }
        let tool_subtype_replacements = build_tool_subtype_replacements(&items)?;
        Ok(Self {
            items,
            tool_subtype_replacements,
            abstract_count: abstracts.len(),
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn abstract_count(&self) -> usize {
        self.abstract_count
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ItemDefinition> {
        self.items.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ItemDefinition)> {
        self.items
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    #[must_use]
    pub fn tool_subtype_replacements<'a>(&'a self, base: &'a str) -> Vec<&'a str> {
        std::iter::once(base)
            .chain(
                self.tool_subtype_replacements
                    .get(base)
                    .into_iter()
                    .flatten()
                    .map(String::as_str),
            )
            .collect()
    }

    pub(crate) fn tool_subtype_replacement_map(&self) -> &BTreeMap<String, Vec<String>> {
        &self.tool_subtype_replacements
    }

    /// Returns concrete compatible MAGAZINE item IDs in stable order for a
    /// normalized well. Explicit item restrictions take precedence over flag
    /// restrictions, matching pinned pocket behavior.
    #[must_use]
    pub fn compatible_magazines<'a>(&'a self, well: &'a MagazineWellDefinition) -> Vec<&'a str> {
        self.items
            .iter()
            .filter(|(item_id, item)| {
                item.subtypes.contains("MAGAZINE")
                    && if !well.item_restrictions.is_empty() {
                        well.item_restrictions.contains(item_id.as_str())
                    } else if !well.flag_restrictions.is_empty() {
                        !item.flags.is_disjoint(&well.flag_restrictions)
                    } else {
                        item_id.as_str() == well.default_magazine
                    }
            })
            .map(|(item_id, _)| item_id.as_str())
            .collect()
    }
}

impl ItemDefinition {
    pub fn melee_damage_milli(&self) -> Result<BTreeMap<String, i32>, ItemRegistryError> {
        self.melee_damage
            .iter()
            .map(|(damage_type, value)| {
                let milli = value * 1_000.0;
                if !milli.is_finite() || milli < 0.0 || milli > f64::from(i32::MAX) {
                    return Err(ItemRegistryError::InvalidMeleeDamage(self.id.clone()));
                }
                Ok((damage_type.clone(), milli.round() as i32))
            })
            .collect()
    }

    #[must_use]
    pub fn melee_to_hit(&self) -> i32 {
        self.melee_to_hit.unwrap_or(-2)
    }

    #[must_use]
    pub fn count_by_charges(&self) -> bool {
        self.stackable
            || self.subtypes.contains("AMMO")
            || (self.subtypes.contains("COMESTIBLE")
                && !matches!(self.phase.as_str(), "" | "SOLID" | "solid"))
    }

    /// Exact pinned `item::attack_time` for an ordinary one-instance item.
    /// Count-by-charge items remain outside this projection because their
    /// aggregate weight/volume rounding depends on live stack state.
    #[must_use]
    pub fn ordinary_attack_time_moves(&self) -> Option<u16> {
        if self.count_by_charges() || self.weight_milligrams < 0 || self.volume_milliliters < 0 {
            return None;
        }
        let volume_units = self.volume_milliliters.checked_mul(2)? / 125;
        let weight_units = self.weight_milligrams / 60_000;
        let moves = 65_i64
            .checked_add(volume_units)?
            .checked_add(weight_units)?;
        u16::try_from(moves).ok()
    }

    /// Mirrors pinned `itype::charges_default` precedence for the implemented
    /// ITEM fields. Callers still decide whether a particular operation uses
    /// the default charge count.
    #[must_use]
    pub fn default_charges(&self) -> i32 {
        if (self.subtypes.contains("TOOL") || self.subtypes.contains("COMESTIBLE"))
            && self.charges > 0
        {
            self.charges
        } else if self.subtypes.contains("AMMO") && self.count > 0 {
            self.count
        } else if self.count_by_charges() {
            1
        } else {
            0
        }
    }
}

fn read_items(
    content_root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawItem>, ItemRegistryError> {
    let mut items = VecDeque::new();
    for file in files {
        let bytes = fs::read(content_root.join(&file.destination))
            .map_err(|error| ItemRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| ItemRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_item(&file, value, &mut items)?;
                }
            }
            value => collect_item(&file, value, &mut items)?,
        }
    }
    Ok(items)
}

fn collect_item(
    file: &SelectedContentFile,
    value: Value,
    items: &mut VecDeque<RawItem>,
) -> Result<(), ItemRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("ITEM") {
        return Ok(());
    }
    let object = value
        .as_object()
        .cloned()
        .ok_or_else(|| ItemRegistryError::InvalidDefinition(file.upstream_path.clone()))?;
    items.push_back(RawItem {
        file: file.clone(),
        object,
    });
    Ok(())
}

fn load_one(
    raw: &RawItem,
    items: &mut BTreeMap<String, ItemDefinition>,
    abstracts: &mut BTreeMap<String, ItemDefinition>,
) -> Result<bool, ItemRegistryError> {
    let (key, is_abstract) = item_key(&raw.object)?;
    let parent = optional_string(&raw.object, "copy-from", &raw.file.upstream_path)?;
    let mut definition = if let Some(parent) = parent {
        let Some(base) = items.get(parent).or_else(|| abstracts.get(parent)) else {
            return Ok(false);
        };
        base.clone()
    } else {
        ItemDefinition::default()
    };
    definition.id = key.to_owned();
    definition.source.clone_from(&raw.file.upstream_path);
    let context = format!("{}#{key}", raw.file.upstream_path);
    apply_common_fields(&mut definition, &raw.object, &context)?;
    if !is_abstract && definition.name.is_empty() {
        return Err(ItemRegistryError::MissingName {
            id: key.to_owned(),
            source: raw.file.upstream_path.clone(),
        });
    }
    if is_abstract {
        abstracts.insert(key.to_owned(), definition);
    } else {
        items.insert(key.to_owned(), definition);
    }
    Ok(true)
}

fn item_key(object: &Map<String, Value>) -> Result<(&str, bool), ItemRegistryError> {
    let id = object.get("id");
    let abstract_id = object.get("abstract");
    match (id, abstract_id) {
        (Some(Value::String(id)), None) if !id.is_empty() => Ok((id, false)),
        (None, Some(Value::String(id))) if !id.is_empty() => Ok((id, true)),
        _ => Err(ItemRegistryError::InvalidIdentity),
    }
}

fn apply_common_fields(
    item: &mut ItemDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    apply_string_set(object, "subtypes", &mut item.subtypes, source)?;
    apply_text(object, "name", &mut item.name, source)?;
    apply_text(object, "description", &mut item.description, source)?;
    apply_string(object, "category", &mut item.category, source)?;
    apply_quantity(
        object,
        "weight",
        &mut item.weight_milligrams,
        QuantityKind::Mass,
        source,
    )?;
    apply_quantity(
        object,
        "volume",
        &mut item.volume_milliliters,
        QuantityKind::Volume,
        source,
    )?;
    apply_quantity(
        object,
        "price",
        &mut item.price_cents,
        QuantityKind::Money,
        source,
    )?;
    apply_quantity(
        object,
        "price_postapoc",
        &mut item.price_postapoc_cents,
        QuantityKind::Money,
        source,
    )?;
    apply_string(object, "symbol", &mut item.symbol, source)?;
    apply_string(object, "color", &mut item.color, source)?;
    apply_materials(object, &mut item.materials, source)?;
    apply_string_set(object, "flags", &mut item.flags, source)?;
    apply_qualities(object, "qualities", &mut item.qualities, source)?;
    apply_qualities(
        object,
        "charged_qualities",
        &mut item.charged_qualities,
        source,
    )?;
    apply_integer(object, "charges_per_use", &mut item.charges_per_use, source)?;
    if item.charges_per_use < 0 {
        return Err(invalid_field(source, "charges_per_use"));
    }
    apply_quantity(
        object,
        "power_draw",
        &mut item.power_draw_milliwatts,
        QuantityKind::Power,
        source,
    )?;
    if item.power_draw_milliwatts < 0 {
        return Err(invalid_field(source, "power_draw"));
    }
    apply_integer(object, "light", &mut item.light_emission, source)?;
    if item.light_emission < 0 {
        return Err(invalid_field(source, "light"));
    }
    apply_string(object, "revert_to", &mut item.revert_to, source)?;
    apply_transform_action_projection(item, object, source)?;
    apply_string(object, "sub", &mut item.tool_subtype, source)?;
    apply_number_map(object, "melee_damage", &mut item.melee_damage, source)?;
    apply_melee_to_hit(object, &mut item.melee_to_hit, source)?;
    apply_integer(object, "charges", &mut item.charges, source)?;
    apply_string(object, "phase", &mut item.phase, source)?;
    if let Some(value) = object.get("stackable") {
        item.stackable = value
            .as_bool()
            .ok_or_else(|| invalid_field(source, "stackable"))?;
    }
    apply_integer(object, "stack_size", &mut item.stack_size, source)?;
    apply_integer(object, "calories", &mut item.calories, source)?;
    apply_integer(object, "quench", &mut item.quench, source)?;
    apply_string(object, "comestible_type", &mut item.comestible_type, source)?;
    apply_string_or_set(object, "ammo", &mut item.ammo, source)?;
    apply_string_or_set(object, "ammo_type", &mut item.ammo_types, source)?;
    apply_string_or_set(object, "tool_ammo", &mut item.tool_ammunition, source)?;
    apply_integer(object, "capacity", &mut item.magazine_capacity, source)?;
    if item.magazine_capacity < 0 {
        return Err(invalid_field(source, "capacity"));
    }
    apply_power_pocket_projections(item, object, source)?;
    apply_integer(object, "count", &mut item.count, source)?;
    apply_integer(object, "range", &mut item.range, source)?;
    apply_integer(object, "dispersion", &mut item.dispersion, source)?;
    let loudness_default = if item.subtypes.contains("AMMO") {
        -1
    } else {
        0
    };
    apply_optional_integer(
        object,
        "loudness",
        &mut item.loudness,
        loudness_default,
        source,
    )?;
    if item.subtypes.contains("AMMO") && item.loudness == Some(-1) {
        item.loudness = None;
    } else if item.loudness.is_some_and(|loudness| loudness < 0) {
        return Err(invalid_field(source, "loudness"));
    }
    apply_damage_field(
        object,
        "damage",
        &mut item.damage,
        &mut item.unsupported_fields,
        source,
    )?;
    apply_integer(object, "clip_size", &mut item.clip_size, source)?;
    apply_integer(
        object,
        "required_level",
        &mut item.book_required_level,
        source,
    )?;
    apply_integer(object, "max_level", &mut item.book_max_level, source)?;
    apply_string(object, "read_skill", &mut item.book_skill, source)?;
    apply_integer(object, "intelligence", &mut item.book_intelligence, source)?;
    apply_duration_moves(object, "time", &mut item.book_time_moves, source)?;
    if item.book_required_level < 0 {
        return Err(invalid_field(source, "required_level"));
    }
    if item.book_max_level < 0 {
        return Err(invalid_field(source, "max_level"));
    }
    if item.book_intelligence < 0 {
        return Err(invalid_field(source, "intelligence"));
    }
    apply_damage_field(
        object,
        "ranged_damage",
        &mut item.ranged_damage,
        &mut item.unsupported_fields,
        source,
    )?;

    for field in object.keys() {
        if !field.starts_with("//")
            && !IMPLEMENTED_FIELDS.contains(&field.as_str())
            && !matches!(
                field.as_str(),
                "extend" | "delete" | "relative" | "proportional"
            )
        {
            item.unsupported_fields.insert(field.clone());
        }
    }
    for modifier in ["extend", "delete", "relative", "proportional"] {
        if let Some(Value::Object(fields)) = object.get(modifier) {
            for field in fields.keys() {
                if !IMPLEMENTED_FIELDS.contains(&field.as_str())
                    || (matches!(field.as_str(), "qualities" | "charged_qualities")
                        && modifier == "proportional")
                {
                    item.unsupported_fields.insert(field.clone());
                }
            }
        }
    }
    Ok(())
}

fn apply_power_pocket_projections(
    item: &mut ItemDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    let Some(value) = object.get("pocket_data") else {
        return Ok(());
    };
    let pockets = value
        .as_array()
        .ok_or_else(|| invalid_field(source, "pocket_data"))?;
    let mut magazine_wells = Vec::new();
    let mut integral_magazines = Vec::new();
    for value in pockets {
        let pocket = value
            .as_object()
            .ok_or_else(|| invalid_field(source, "pocket_data"))?;
        match pocket
            .get("pocket_type")
            .and_then(Value::as_str)
            .unwrap_or("CONTAINER")
        {
            "MAGAZINE_WELL" => {
                let item_restriction_value = pocket.get("item_restriction");
                let first_item_restriction = item_restriction_value
                    .and_then(Value::as_array)
                    .and_then(|values| values.first())
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let item_restrictions = item_restriction_value
                    .map(|value| string_set(value, source, "pocket_data"))
                    .transpose()?
                    .unwrap_or_default();
                let flag_restrictions = pocket
                    .get("flag_restriction")
                    .map(|value| string_set(value, source, "pocket_data"))
                    .transpose()?
                    .unwrap_or_default();
                let mut default_magazine = pocket
                    .get("default_magazine")
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                            .ok_or_else(|| invalid_field(source, "pocket_data"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                if let Some(first) = first_item_restriction {
                    default_magazine = first;
                }
                magazine_wells.push(MagazineWellDefinition {
                    default_magazine,
                    item_restrictions,
                    flag_restrictions,
                });
            }
            "MAGAZINE" => {
                let Some(restrictions) = pocket.get("ammo_restriction") else {
                    continue;
                };
                let restrictions = restrictions
                    .as_object()
                    .ok_or_else(|| invalid_field(source, "pocket_data"))?
                    .iter()
                    .map(|(ammunition_type, capacity)| {
                        let capacity = i32::try_from(
                            capacity
                                .as_i64()
                                .filter(|capacity| *capacity > 0)
                                .ok_or_else(|| invalid_field(source, "pocket_data"))?,
                        )
                        .map_err(|_| invalid_field(source, "pocket_data"))?;
                        if ammunition_type.is_empty() {
                            return Err(invalid_field(source, "pocket_data"));
                        }
                        Ok((ammunition_type.clone(), capacity))
                    })
                    .collect::<Result<BTreeMap<_, _>, ItemRegistryError>>()?;
                if restrictions.is_empty() {
                    return Err(invalid_field(source, "pocket_data"));
                }
                integral_magazines.push(restrictions);
            }
            _ => {}
        }
    }
    item.magazine_wells = magazine_wells;
    item.integral_magazines = integral_magazines;
    Ok(())
}

fn apply_transform_action_projection(
    item: &mut ItemDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    let Some(value) = object.get("use_action") else {
        return Ok(());
    };
    let values = match value {
        Value::Array(values) => values.as_slice(),
        value => std::slice::from_ref(value),
    };
    item.has_non_transform_use_actions = values.iter().any(|value| {
        value
            .as_object()
            .and_then(|action| action.get("type"))
            .and_then(Value::as_str)
            != Some("transform")
    });
    const PROJECTED_OR_COSMETIC_TRANSFORM_FIELDS: &[&str] = &[
        "type",
        "target",
        "need_charges",
        "ammo_scale",
        "moves",
        "msg",
        "menu_text",
        "need_charges_msg",
        "damage_failure_msg",
    ];
    item.has_unsupported_transform_action_fields = false;
    let mut actions = Vec::new();
    for value in values {
        let Some(action) = value.as_object() else {
            continue;
        };
        if action.get("type").and_then(Value::as_str) != Some("transform") {
            continue;
        }
        if action
            .keys()
            .any(|field| !PROJECTED_OR_COSMETIC_TRANSFORM_FIELDS.contains(&field.as_str()))
        {
            item.has_unsupported_transform_action_fields = true;
        }
        let target = action
            .get("target")
            .and_then(Value::as_str)
            .filter(|target| !target.is_empty())
            .ok_or_else(|| invalid_field(source, "use_action"))?
            .to_owned();
        let need_charges = action.get("need_charges").map_or(Ok(0), |value| {
            i32::try_from(
                value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| invalid_field(source, "use_action"))?,
            )
            .map_err(|_| invalid_field(source, "use_action"))
        })?;
        let ammo_scale = action.get("ammo_scale").map_or(Ok(1), |value| {
            i32::try_from(
                value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| invalid_field(source, "use_action"))?,
            )
            .map_err(|_| invalid_field(source, "use_action"))
        })?;
        let moves = action.get("moves").map_or(Ok(0), |value| {
            i32::try_from(
                value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .ok_or_else(|| invalid_field(source, "use_action"))?,
            )
            .map_err(|_| invalid_field(source, "use_action"))
        })?;
        actions.push(ItemTransformActionDefinition {
            target,
            need_charges,
            ammo_scale,
            moves,
        });
    }
    item.transform_actions = actions;
    Ok(())
}

fn apply_duration_moves(
    object: &Map<String, Value>,
    field: &str,
    target: &mut u64,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = parse_duration_moves(
            value.as_str().ok_or_else(|| invalid_field(source, field))?,
            source,
            field,
        )?;
    } else if let Some(value) = modifier(object, "proportional", field, source)? {
        let multiplier = value
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| invalid_field(source, field))?;
        let adjusted = (*target as f64) * multiplier;
        if !adjusted.is_finite() || adjusted < 0.0 || adjusted > u64::MAX as f64 {
            return Err(invalid_field(source, field));
        }
        *target = adjusted.round() as u64;
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        let addition = parse_duration_moves(
            value.as_str().ok_or_else(|| invalid_field(source, field))?,
            source,
            field,
        )?;
        *target = target
            .checked_add(addition)
            .ok_or_else(|| invalid_field(source, field))?;
    }
    Ok(())
}

fn parse_duration_moves(value: &str, source: &str, field: &str) -> Result<u64, ItemRegistryError> {
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
            return Err(invalid_field(source, field));
        }
        let number = value[number_start..index]
            .parse::<u64>()
            .map_err(|_| invalid_field(source, field))?;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let unit_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        if unit_start == index {
            return Err(invalid_field(source, field));
        }
        let multiplier = match &value[unit_start..index] {
            "s" | "second" | "seconds" => 1,
            "m" | "minute" | "minutes" => 60,
            "h" | "hour" | "hours" => 60 * 60,
            "d" | "day" | "days" => 24 * 60 * 60,
            _ => return Err(invalid_field(source, field)),
        };
        seconds = seconds
            .checked_add(
                number
                    .checked_mul(multiplier)
                    .ok_or_else(|| invalid_field(source, field))?,
            )
            .ok_or_else(|| invalid_field(source, field))?;
        terms += 1;
    }
    if terms == 0 {
        return Err(invalid_field(source, field));
    }
    seconds
        .checked_mul(100)
        .ok_or_else(|| invalid_field(source, field))
}

fn apply_qualities(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeMap<String, ItemQualityDefinition>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = quality_map(value, source, field, QualityReadMode::Definition)?;
    }
    if let Some(value) = modifier(object, "extend", field, source)? {
        let context = format!("extend.{field}");
        target.extend(quality_map(
            value,
            source,
            &context,
            QualityReadMode::Definition,
        )?);
    }
    if let Some(value) = modifier(object, "delete", field, source)? {
        let context = format!("delete.{field}");
        for quality_id in quality_ids(value, source, &context)? {
            target.remove(&quality_id);
        }
    }
    if let Some(value) = modifier(object, "relative", field, source)? {
        let context = format!("relative.{field}");
        for (quality_id, adjustment) in
            quality_map(value, source, &context, QualityReadMode::Relative)?
        {
            let existing = target
                .get_mut(&quality_id)
                .ok_or_else(|| invalid_field(source, &context))?;
            existing.level = existing
                .level
                .checked_add(adjustment.level)
                .ok_or_else(|| invalid_field(source, &context))?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum QualityReadMode {
    Definition,
    Relative,
}

fn quality_map(
    value: &Value,
    source: &str,
    field: &str,
    mode: QualityReadMode,
) -> Result<BTreeMap<String, ItemQualityDefinition>, ItemRegistryError> {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| quality_entry(value, source, field, mode))
            .collect(),
        Value::Object(values) => values
            .iter()
            .map(|(quality_id, level)| {
                let level =
                    i32::try_from(level.as_i64().ok_or_else(|| invalid_field(source, field))?)
                        .map_err(|_| invalid_field(source, field))?;
                validate_quality_id(quality_id, source, field)?;
                Ok((
                    quality_id.clone(),
                    ItemQualityDefinition { level, speed: 1.0 },
                ))
            })
            .collect(),
        _ => Err(invalid_field(source, field)),
    }
}

fn quality_entry(
    value: &Value,
    source: &str,
    field: &str,
    mode: QualityReadMode,
) -> Result<(String, ItemQualityDefinition), ItemRegistryError> {
    let (quality_id, level, speed) = match value {
        Value::Array(values) if values.len() == 2 => (
            values[0]
                .as_str()
                .ok_or_else(|| invalid_field(source, field))?,
            i32::try_from(
                values[1]
                    .as_i64()
                    .ok_or_else(|| invalid_field(source, field))?,
            )
            .map_err(|_| invalid_field(source, field))?,
            1.0,
        ),
        Value::Object(values) => {
            if values
                .keys()
                .any(|key| !matches!(key.as_str(), "id" | "level" | "speed"))
            {
                return Err(invalid_field(source, field));
            }
            let quality_id = values
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_field(source, field))?;
            let level = i32::try_from(
                values
                    .get("level")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| invalid_field(source, field))?,
            )
            .map_err(|_| invalid_field(source, field))?;
            let speed = values.get("speed").map_or(Ok(1.0), |value| {
                value
                    .as_f64()
                    .filter(|speed| speed.is_finite() && *speed > 0.0)
                    .ok_or_else(|| invalid_field(source, field))
            })?;
            (quality_id, level, speed)
        }
        _ => return Err(invalid_field(source, field)),
    };
    validate_quality_id(quality_id, source, field)?;
    if matches!(mode, QualityReadMode::Relative) && speed != 1.0 {
        return Err(invalid_field(source, field));
    }
    Ok((
        quality_id.to_owned(),
        ItemQualityDefinition { level, speed },
    ))
}

fn quality_ids(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, ItemRegistryError> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid_field(source, field))?;
    values
        .iter()
        .map(|value| {
            let quality_id = match value {
                Value::String(value) => value.as_str(),
                Value::Array(values) if !values.is_empty() => values[0]
                    .as_str()
                    .ok_or_else(|| invalid_field(source, field))?,
                Value::Object(values) => values
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid_field(source, field))?,
                _ => return Err(invalid_field(source, field)),
            };
            validate_quality_id(quality_id, source, field)?;
            Ok(quality_id.to_owned())
        })
        .collect()
}

fn validate_quality_id(
    quality_id: &str,
    source: &str,
    field: &str,
) -> Result<(), ItemRegistryError> {
    if quality_id.is_empty() || quality_id.len() > 64 || quality_id.chars().any(char::is_control) {
        return Err(invalid_field(source, field));
    }
    Ok(())
}

fn apply_integer(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = i32::try_from(value.as_i64().ok_or_else(|| invalid_field(source, field))?)
            .map_err(|_| invalid_field(source, field))?;
    } else if let Some(value) = modifier(object, "proportional", field, source)? {
        let multiplier = value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| invalid_field(source, field))?;
        let adjusted = f64::from(*target) * multiplier;
        if !adjusted.is_finite() || adjusted < f64::from(i32::MIN) || adjusted > f64::from(i32::MAX)
        {
            return Err(invalid_field(source, field));
        }
        *target = adjusted.round() as i32;
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        let addition = i32::try_from(value.as_i64().ok_or_else(|| invalid_field(source, field))?)
            .map_err(|_| invalid_field(source, field))?;
        *target = target
            .checked_add(addition)
            .ok_or_else(|| invalid_field(source, field))?;
    }
    Ok(())
}

fn apply_melee_to_hit(
    object: &Map<String, Value>,
    target: &mut Option<i32>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    for unsupported_modifier in ["extend", "delete", "proportional"] {
        if modifier(object, unsupported_modifier, "to_hit", source)?.is_some() {
            return Err(invalid_field(
                source,
                &format!("{unsupported_modifier}.to_hit"),
            ));
        }
    }
    if let Some(value) = object.get("to_hit") {
        if modifier(object, "relative", "to_hit", source)?.is_some() {
            return Err(invalid_field(source, "relative.to_hit"));
        }
        *target = Some(parse_melee_to_hit(value, source)?);
    } else if let Some(value) = modifier(object, "relative", "to_hit", source)? {
        let addition = i32::try_from(
            value
                .as_i64()
                .ok_or_else(|| invalid_field(source, "relative.to_hit"))?,
        )
        .map_err(|_| invalid_field(source, "relative.to_hit"))?;
        *target = Some(
            target
                .unwrap_or(-2)
                .checked_add(addition)
                .ok_or_else(|| invalid_field(source, "relative.to_hit"))?,
        );
    }
    Ok(())
}

fn parse_melee_to_hit(value: &Value, source: &str) -> Result<i32, ItemRegistryError> {
    if let Some(value) = value.as_i64() {
        return i32::try_from(value).map_err(|_| invalid_field(source, "to_hit"));
    }
    let object = value
        .as_object()
        .ok_or_else(|| invalid_field(source, "to_hit"))?;
    if object.keys().any(|field| {
        !field.starts_with("//")
            && !matches!(field.as_str(), "grip" | "length" | "surface" | "balance")
    }) {
        return Err(invalid_field(source, "to_hit"));
    }
    let component = |field: &str, default: i32, values: &[(&str, i32)]| {
        let Some(value) = object.get(field) else {
            return Ok(default);
        };
        let value = value
            .as_str()
            .ok_or_else(|| invalid_field(source, "to_hit"))?;
        values
            .iter()
            .find_map(|(candidate, resolved)| (*candidate == value).then_some(*resolved))
            .ok_or_else(|| invalid_field(source, "to_hit"))
    };
    let grip = component(
        "grip",
        3,
        &[("bad", 0), ("none", 1), ("solid", 2), ("weapon", 3)],
    )?;
    let length = component("length", 0, &[("hand", 0), ("short", 1), ("long", 2)])?;
    let surface = component(
        "surface",
        2,
        &[("point", 0), ("line", 1), ("any", 2), ("every", 3)],
    )?;
    let balance = component(
        "balance",
        2,
        &[("clumsy", 0), ("uneven", 1), ("neutral", 2), ("good", 3)],
    )?;
    Ok(-7 + grip + length + surface + balance)
}

fn apply_optional_integer(
    object: &Map<String, Value>,
    field: &str,
    target: &mut Option<i32>,
    default: i32,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = Some(
            i32::try_from(value.as_i64().ok_or_else(|| invalid_field(source, field))?)
                .map_err(|_| invalid_field(source, field))?,
        );
        return Ok(());
    }
    if let Some(value) = modifier(object, "proportional", field, source)? {
        let multiplier = value
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| invalid_field(source, field))?;
        let adjusted = f64::from(target.unwrap_or(default)) * multiplier;
        if !adjusted.is_finite() || adjusted < f64::from(i32::MIN) || adjusted > f64::from(i32::MAX)
        {
            return Err(invalid_field(source, field));
        }
        *target = Some(adjusted.round() as i32);
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        let addition = i32::try_from(value.as_i64().ok_or_else(|| invalid_field(source, field))?)
            .map_err(|_| invalid_field(source, field))?;
        *target = Some(
            target
                .unwrap_or(default)
                .checked_add(addition)
                .ok_or_else(|| invalid_field(source, field))?,
        );
    }
    Ok(())
}

fn apply_text(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), ItemRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    *target = match value {
        Value::String(value) => value.clone(),
        Value::Object(value) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(Value::as_str))
            .map(str::to_owned)
            .ok_or_else(|| invalid_field(source, field))?,
        _ => return Err(invalid_field(source, field)),
    };
    Ok(())
}

fn apply_string(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_str()
            .ok_or_else(|| invalid_field(source, field))?
            .to_owned();
    }
    Ok(())
}

fn apply_string_set(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = string_set(value, source, field)?;
    }
    if let Some(value) = modifier(object, "extend", field, source)? {
        target.extend(string_set(value, source, field)?);
    }
    if let Some(value) = modifier(object, "delete", field, source)? {
        for entry in string_set(value, source, field)? {
            target.remove(&entry);
        }
    }
    Ok(())
}

fn apply_string_or_set(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = string_or_set(value, source, field)?;
    }
    if let Some(value) = modifier(object, "extend", field, source)? {
        target.extend(string_or_set(value, source, field)?);
    }
    if let Some(value) = modifier(object, "delete", field, source)? {
        for entry in string_or_set(value, source, field)? {
            target.remove(&entry);
        }
    }
    Ok(())
}

fn string_or_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, ItemRegistryError> {
    if let Some(value) = value.as_str() {
        return if value.is_empty() {
            Err(invalid_field(source, field))
        } else {
            Ok(BTreeSet::from([value.to_owned()]))
        };
    }
    string_set(value, source, field)
}

fn string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, ItemRegistryError> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid_field(source, field))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_field(source, field))
        })
        .collect()
}

#[derive(Clone, Copy)]
enum QuantityKind {
    Mass,
    Volume,
    Money,
    Power,
}

fn apply_quantity(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i64,
    kind: QuantityKind,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = parse_quantity(value, kind, source, field)?;
        return Ok(());
    }
    if let Some(value) = modifier(object, "proportional", field, source)? {
        let multiplier = value
            .as_f64()
            .filter(|value| *value > 0.0 && *value != 1.0)
            .ok_or_else(|| invalid_field(source, field))?;
        *target = ((*target as f64) * multiplier).round() as i64;
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        *target = target
            .checked_add(parse_quantity(value, kind, source, field)?)
            .ok_or_else(|| ItemRegistryError::QuantityOverflow(field.to_owned()))?;
    }
    Ok(())
}

fn parse_quantity(
    value: &Value,
    kind: QuantityKind,
    source: &str,
    field: &str,
) -> Result<i64, ItemRegistryError> {
    let text = value.as_str().ok_or_else(|| invalid_field(source, field))?;
    let mut tokens = Vec::new();
    for token in text.split_whitespace() {
        if let Some(unit_start) = token.find(char::is_alphabetic)
            && unit_start > 0
        {
            tokens.push(&token[..unit_start]);
            tokens.push(&token[unit_start..]);
        } else {
            tokens.push(token);
        }
    }
    if tokens.is_empty() || !tokens.len().is_multiple_of(2) {
        return Err(invalid_field(source, field));
    }
    let mut total = 0_i64;
    for pair in tokens.chunks_exact(2) {
        let amount = pair[0]
            .parse::<i64>()
            .map_err(|_| invalid_field(source, field))?;
        let multiplier = match (kind, pair[1]) {
            (QuantityKind::Mass, "mg") => 1,
            (QuantityKind::Mass, "g") => 1_000,
            (QuantityKind::Mass, "kg") => 1_000_000,
            (QuantityKind::Volume, "ml") => 1,
            (QuantityKind::Volume, "L") => 1_000,
            (QuantityKind::Money, "cent" | "cents") => 1,
            (QuantityKind::Money, "USD" | "dollar" | "dollars") => 100,
            (QuantityKind::Money, "kUSD") => 100_000,
            (QuantityKind::Power, "mW") => 1,
            (QuantityKind::Power, "W") => 1_000,
            (QuantityKind::Power, "kW") => 1_000_000,
            _ => return Err(invalid_field(source, field)),
        };
        total = total
            .checked_add(
                amount
                    .checked_mul(multiplier)
                    .ok_or_else(|| ItemRegistryError::QuantityOverflow(field.to_owned()))?,
            )
            .ok_or_else(|| ItemRegistryError::QuantityOverflow(field.to_owned()))?;
    }
    Ok(total)
}

fn apply_materials(
    object: &Map<String, Value>,
    target: &mut BTreeMap<String, i64>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get("material") {
        *target = materials(value, source)?;
    }
    if let Some(value) = modifier(object, "extend", "material", source)? {
        target.extend(materials(value, source)?);
    }
    if let Some(value) = modifier(object, "delete", "material", source)? {
        for material in materials(value, source)?.into_keys() {
            target.remove(&material);
        }
    }
    Ok(())
}

fn materials(value: &Value, source: &str) -> Result<BTreeMap<String, i64>, ItemRegistryError> {
    if let Value::Object(values) = value {
        return values
            .iter()
            .map(|(id, portion)| {
                portion
                    .as_i64()
                    .map(|portion| (id.clone(), portion))
                    .ok_or_else(|| invalid_field(source, "material"))
            })
            .collect();
    }
    let values = value
        .as_array()
        .ok_or_else(|| invalid_field(source, "material"))?;
    let mut materials = BTreeMap::new();
    for value in values {
        match value {
            Value::String(id) => {
                materials.insert(id.clone(), 1);
            }
            Value::Object(material) => {
                let id = material
                    .get("type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid_field(source, "material"))?;
                let portion = material.get("portion").and_then(Value::as_i64).unwrap_or(1);
                materials.insert(id.to_owned(), portion);
            }
            _ => return Err(invalid_field(source, "material")),
        }
    }
    Ok(materials)
}

fn apply_number_map(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeMap<String, f64>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = number_map(value, source, field)?;
        return Ok(());
    }
    if let Some(value) = modifier(object, "proportional", field, source)? {
        for (key, multiplier) in number_map(value, source, field)? {
            *target.entry(key).or_default() *= multiplier;
        }
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        for (key, addition) in number_map(value, source, field)? {
            *target.entry(key).or_default() += addition;
        }
    }
    Ok(())
}

fn number_map(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeMap<String, f64>, ItemRegistryError> {
    value
        .as_object()
        .ok_or_else(|| invalid_field(source, field))?
        .iter()
        .map(|(key, value)| {
            value
                .as_f64()
                .map(|value| (key.clone(), value))
                .ok_or_else(|| invalid_field(source, field))
        })
        .collect()
}

fn apply_damage_field(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeMap<String, DamageDefinition>,
    unsupported: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        target.clear();
        for unit in damage_units(value, field, unsupported, source)? {
            let entry = target.entry(unit.damage_type).or_default();
            entry.amount += unit.amount.unwrap_or(0.0);
            entry.armor_penetration += unit.armor_penetration.unwrap_or(0.0);
        }
    }
    if let Some(value) = modifier(object, "proportional", field, source)? {
        for unit in damage_units(value, field, unsupported, source)? {
            let entry = target.entry(unit.damage_type).or_default();
            if let Some(multiplier) = unit.amount {
                entry.amount *= multiplier;
            }
            if let Some(multiplier) = unit.armor_penetration {
                entry.armor_penetration *= multiplier;
            }
        }
    }
    if let Some(value) = modifier(object, "relative", field, source)? {
        for unit in damage_units(value, field, unsupported, source)? {
            let entry = target.entry(unit.damage_type).or_default();
            if let Some(addition) = unit.amount {
                entry.amount += addition;
            }
            if let Some(addition) = unit.armor_penetration {
                entry.armor_penetration += addition;
            }
        }
    }
    if target
        .values()
        .any(|damage| !damage.amount.is_finite() || !damage.armor_penetration.is_finite())
    {
        return Err(invalid_field(source, field));
    }
    Ok(())
}

struct RawDamageUnit {
    damage_type: String,
    amount: Option<f64>,
    armor_penetration: Option<f64>,
}

fn damage_units(
    value: &Value,
    field: &str,
    unsupported: &mut BTreeSet<String>,
    source: &str,
) -> Result<Vec<RawDamageUnit>, ItemRegistryError> {
    let values: Vec<&Value> = match value {
        Value::Object(_) => vec![value],
        Value::Array(values) => values.iter().collect(),
        _ => return Err(invalid_field(source, field)),
    };
    values
        .into_iter()
        .map(|value| {
            let object = value
                .as_object()
                .ok_or_else(|| invalid_field(source, field))?;
            let damage_type = object
                .get("damage_type")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| invalid_field(source, field))?
                .to_owned();
            for nested in object.keys() {
                if !matches!(
                    nested.as_str(),
                    "damage_type" | "amount" | "armor_penetration"
                ) {
                    unsupported.insert(format!("{field}.{nested}"));
                }
            }
            let finite_number = |name: &str| -> Result<Option<f64>, ItemRegistryError> {
                object
                    .get(name)
                    .map(|value| {
                        value
                            .as_f64()
                            .filter(|value| value.is_finite())
                            .ok_or_else(|| invalid_field(source, field))
                    })
                    .transpose()
            };
            Ok(RawDamageUnit {
                damage_type,
                amount: finite_number("amount")?,
                armor_penetration: finite_number("armor_penetration")?,
            })
        })
        .collect()
}

fn modifier<'a>(
    object: &'a Map<String, Value>,
    modifier: &str,
    field: &str,
    source: &str,
) -> Result<Option<&'a Value>, ItemRegistryError> {
    match object.get(modifier) {
        None => Ok(None),
        Some(Value::Object(values)) => Ok(values.get(field)),
        Some(_) => Err(invalid_field(source, modifier)),
    }
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<&'a str>, ItemRegistryError> {
    object
        .get(field)
        .map(|value| value.as_str().ok_or_else(|| invalid_field(source, field)))
        .transpose()
}

fn build_tool_subtype_replacements(
    items: &BTreeMap<String, ItemDefinition>,
) -> Result<BTreeMap<String, Vec<String>>, ItemRegistryError> {
    let mut replacements = BTreeMap::<String, Vec<String>>::new();
    for (candidate_id, candidate) in items {
        if !candidate.subtypes.contains("TOOL") {
            continue;
        }
        let mut current = candidate;
        let mut path = Vec::new();
        let mut ancestors = Vec::new();
        while current.subtypes.contains("TOOL") && !current.tool_subtype.is_empty() {
            if path.iter().any(|id| id == &current.id) {
                path.push(current.id.clone());
                return Err(ItemRegistryError::CyclicToolSubtype { chain: path });
            }
            path.push(current.id.clone());
            let next = items.get(&current.tool_subtype).ok_or_else(|| {
                ItemRegistryError::MissingToolSubtype {
                    item: current.id.clone(),
                    subtype: current.tool_subtype.clone(),
                }
            })?;
            ancestors.push(next.id.clone());
            current = next;
        }
        for ancestor in ancestors {
            replacements
                .entry(ancestor)
                .or_default()
                .push(candidate_id.clone());
        }
    }
    Ok(replacements)
}

fn invalid_field(source: &str, field: &str) -> ItemRegistryError {
    ItemRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum ItemRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    InvalidIdentity,
    InvalidMeleeDamage(String),
    MissingToolSubtype { item: String, subtype: String },
    CyclicToolSubtype { chain: Vec<String> },
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    MissingName { id: String, source: String },
    QuantityOverflow(String),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for ItemRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "item mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("internal item load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "ITEM definition is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid ITEM field {field} in {source}")
            }
            Self::InvalidIdentity => {
                formatter.write_str("ITEM must have exactly one non-empty id or abstract")
            }
            Self::InvalidMeleeDamage(id) => {
                write!(formatter, "ITEM {id} has invalid finalized melee damage")
            }
            Self::MissingToolSubtype { item, subtype } => {
                write!(
                    formatter,
                    "ITEM {item} references missing tool subtype {subtype}"
                )
            }
            Self::CyclicToolSubtype { chain } => {
                write!(formatter, "ITEM tool subtype chain is cyclic: {chain:?}")
            }
            Self::Io(path, error) => {
                write!(formatter, "item registry I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(formatter, "item registry JSON failed for {path}: {error}")
            }
            Self::MissingName { id, source } => {
                write!(
                    formatter,
                    "concrete ITEM {id} has no inherited or direct name in {source}"
                )
            }
            Self::QuantityOverflow(field) => write!(formatter, "ITEM {field} quantity overflowed"),
            Self::UnresolvedInheritance(ids) => {
                write!(formatter, "unresolved or cyclic ITEM inheritance: {ids:?}")
            }
        }
    }
}

impl std::error::Error for ItemRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(value: Value) -> RawItem {
        RawItem {
            file: SelectedContentFile {
                owner: String::from("test"),
                upstream_path: String::from("data/json/test.json"),
                destination: String::from("cdda/data/json/test.json"),
            },
            object: value
                .as_object()
                .expect("test item should be object")
                .clone(),
        }
    }

    #[test]
    fn self_override_inherits_previous_item_and_applies_modifiers() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_blade",
            "name": { "str": "test blade" },
            "weight": "2 kg",
            "volume": "1 L",
            "flags": ["SHARP"],
            "qualities": [["CUT", 1], { "id": "BUTCHER", "level": 2, "speed": 0.75 }],
            "charged_qualities": [{ "id": "WELD", "level": 1, "speed": 0.5 }, ["DRILL", 2]],
            "charges_per_use": 5,
            "melee_damage": { "cut": 10 }
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base should load"));
        let override_item = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_blade",
            "copy-from": "test_blade",
            "name": "light test blade",
            "proportional": { "weight": 0.5, "melee_damage": { "cut": 1.2 } },
            "relative": { "volume": "250 ml", "qualities": [["CUT", 2]], "charged_qualities": [["WELD", 1]], "charges_per_use": 2 },
            "extend": { "flags": ["LIGHT"], "qualities": { "HAMMER": 1 }, "charged_qualities": { "FILE": 1 } },
            "delete": { "flags": ["SHARP"], "qualities": ["BUTCHER"], "charged_qualities": ["DRILL"] }
        }));
        assert!(
            load_one(&override_item, &mut items, &mut abstracts).expect("override should load")
        );
        let item = items.get("test_blade").expect("item should remain");
        assert_eq!(item.name, "light test blade");
        assert_eq!(item.weight_milligrams, 1_000_000);
        assert_eq!(item.volume_milliliters, 1_250);
        assert_eq!(item.ordinary_attack_time_moves(), Some(101));
        assert_eq!(item.melee_damage.get("cut"), Some(&12.0));
        assert_eq!(item.flags, BTreeSet::from([String::from("LIGHT")]));
        assert_eq!(item.qualities["CUT"].level, 3);
        assert_eq!(item.qualities["CUT"].speed, 1.0);
        assert_eq!(item.qualities["HAMMER"].level, 1);
        assert!(!item.qualities.contains_key("BUTCHER"));
        assert_eq!(item.charged_qualities["WELD"].level, 2);
        assert_eq!(item.charged_qualities["WELD"].speed, 0.5);
        assert_eq!(item.charged_qualities["FILE"].level, 1);
        assert!(!item.charged_qualities.contains_key("DRILL"));
        assert_eq!(item.charges_per_use, 7);
    }

    #[test]
    fn melee_to_hit_accepts_pinned_objects_legacy_integers_and_relative_inheritance() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let hammer = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_hammer",
            "name": "test hammer",
            "to_hit": {
                "grip": "solid",
                "length": "hand",
                "surface": "any",
                "balance": "neutral"
            }
        }));
        assert!(load_one(&hammer, &mut items, &mut abstracts).expect("object should load"));
        assert_eq!(items["test_hammer"].melee_to_hit(), -1);

        let inherited = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_hammer_precise",
            "copy-from": "test_hammer",
            "name": "precise test hammer",
            "relative": { "to_hit": 2 }
        }));
        assert!(load_one(&inherited, &mut items, &mut abstracts).expect("relative should load"));
        assert_eq!(items["test_hammer_precise"].melee_to_hit(), 1);

        let legacy = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_legacy_weapon",
            "name": "legacy test weapon",
            "to_hit": -3
        }));
        assert!(load_one(&legacy, &mut items, &mut abstracts).expect("legacy integer should load"));
        assert_eq!(items["test_legacy_weapon"].melee_to_hit(), -3);

        let ordinary = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_ordinary_item",
            "name": "ordinary test item"
        }));
        assert!(load_one(&ordinary, &mut items, &mut abstracts).expect("default should load"));
        assert_eq!(items["test_ordinary_item"].melee_to_hit(), -2);

        let invalid = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_invalid_accuracy",
            "copy-from": "test_hammer",
            "name": "invalid test accuracy",
            "proportional": { "to_hit": 2 }
        }));
        assert!(matches!(
            load_one(&invalid, &mut items, &mut abstracts),
            Err(ItemRegistryError::InvalidField { field, .. })
                if field == "proportional.to_hit"
        ));
    }

    #[test]
    fn loudness_inherits_modifies_and_preserves_the_ammunition_derivation_sentinel() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_gun",
            "subtypes": ["GUN"],
            "name": "test gun",
            "loudness": 40
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base gun should load"));
        let proportional = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_quiet_gun",
            "copy-from": "test_gun",
            "name": "test quiet gun",
            "proportional": { "loudness": 0.5 }
        }));
        assert!(
            load_one(&proportional, &mut items, &mut abstracts)
                .expect("proportional loudness should load")
        );
        assert_eq!(items["test_quiet_gun"].loudness, Some(20));
        let relative = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_modified_gun",
            "copy-from": "test_quiet_gun",
            "name": "test modified gun",
            "relative": { "loudness": 5 }
        }));
        assert!(
            load_one(&relative, &mut items, &mut abstracts).expect("relative loudness should load")
        );
        assert_eq!(items["test_modified_gun"].loudness, Some(25));

        let derived_ammunition = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_ammunition",
            "subtypes": ["AMMO"],
            "name": "test ammunition",
            "loudness": -1
        }));
        assert!(
            load_one(&derived_ammunition, &mut items, &mut abstracts)
                .expect("ammunition derivation sentinel should load")
        );
        assert_eq!(items["test_ammunition"].loudness, None);

        let invalid_gun = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_invalid_gun",
            "subtypes": ["GUN"],
            "name": "test invalid gun",
            "loudness": -1
        }));
        assert!(matches!(
            load_one(&invalid_gun, &mut items, &mut abstracts),
            Err(ItemRegistryError::InvalidField { field, .. }) if field == "loudness"
        ));
    }

    #[test]
    fn tool_ammunition_inherits_replaces_extends_and_deletes_as_a_string_set() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_powered_tool",
            "subtypes": ["TOOL"],
            "name": "test powered tool",
            "tool_ammo": "battery"
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base tool should load"));
        let extended = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_extended_tool",
            "copy-from": "test_powered_tool",
            "name": "test extended tool",
            "extend": { "tool_ammo": ["gasoline"] },
            "delete": { "tool_ammo": "battery" }
        }));
        assert!(
            load_one(&extended, &mut items, &mut abstracts).expect("extended tool should load")
        );
        assert_eq!(
            items["test_extended_tool"].tool_ammunition,
            BTreeSet::from([String::from("gasoline")])
        );
        assert!(
            !items["test_extended_tool"]
                .unsupported_fields
                .contains("tool_ammo")
        );

        let replaced = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_replaced_tool",
            "copy-from": "test_powered_tool",
            "name": "test replaced tool",
            "tool_ammo": ["tape", "thread"]
        }));
        assert!(
            load_one(&replaced, &mut items, &mut abstracts).expect("replacement tool should load")
        );
        assert_eq!(
            items["test_replaced_tool"].tool_ammunition,
            BTreeSet::from([String::from("tape"), String::from("thread")])
        );
    }

    #[test]
    fn powered_transform_projection_inherits_and_replaces_exact_integer_units() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let off = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_light",
            "subtypes": ["TOOL"],
            "name": "test light (off)",
            "charges_per_use": 1,
            "use_action": {
                "type": "transform",
                "target": "test_light_on",
                "need_charges": 1
            }
        }));
        assert!(load_one(&off, &mut items, &mut abstracts).expect("off light should load"));
        let on = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_light_on",
            "copy-from": "test_light",
            "name": "test light (on)",
            "power_draw": "1 W 560 mW",
            "light": 300,
            "revert_to": "test_light",
            "use_action": {
                "type": "transform",
                "target": "test_light",
                "ammo_scale": 0,
                "moves": 7
            }
        }));
        assert!(load_one(&on, &mut items, &mut abstracts).expect("on light should load"));
        let off = &items["test_light"];
        assert_eq!(off.power_draw_milliwatts, 0);
        assert_eq!(off.light_emission, 0);
        assert_eq!(
            off.transform_actions,
            [ItemTransformActionDefinition {
                target: String::from("test_light_on"),
                need_charges: 1,
                ammo_scale: 1,
                moves: 0,
            }]
        );
        assert!(!off.has_non_transform_use_actions);
        assert!(!off.has_unsupported_transform_action_fields);
        let on = &items["test_light_on"];
        assert_eq!(on.power_draw_milliwatts, 1_560);
        assert_eq!(on.light_emission, 300);
        assert_eq!(on.revert_to, "test_light");
        assert_eq!(
            on.transform_actions,
            [ItemTransformActionDefinition {
                target: String::from("test_light"),
                need_charges: 0,
                ammo_scale: 0,
                moves: 7,
            }]
        );
        assert!(!on.has_non_transform_use_actions);
        assert!(!on.has_unsupported_transform_action_fields);
        assert!(on.unsupported_fields.contains("use_action"));
        assert!(!on.unsupported_fields.contains("power_draw"));
        assert!(!on.unsupported_fields.contains("light"));
        assert!(!on.unsupported_fields.contains("revert_to"));

        let linked = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_linked_light",
            "copy-from": "test_light",
            "use_action": [
                { "type": "transform", "target": "test_light_on" },
                { "type": "link_up", "charge_rate": "2 W" }
            ]
        }));
        assert!(load_one(&linked, &mut items, &mut abstracts).expect("linked light should load"));
        assert!(items["test_linked_light"].has_non_transform_use_actions);

        let conditional = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_conditional_light",
            "copy-from": "test_light",
            "use_action": {
                "type": "transform",
                "target": "test_light_on",
                "need_fire": 1
            }
        }));
        assert!(
            load_one(&conditional, &mut items, &mut abstracts)
                .expect("conditional light should load")
        );
        assert!(items["test_conditional_light"].has_unsupported_transform_action_fields);
    }

    #[test]
    fn power_pocket_projection_inherits_replaces_and_resolves_magazines() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_portable_tool",
            "subtypes": ["TOOL"],
            "name": "test portable tool",
            "tool_ammo": "battery",
            "pocket_data": [{
                "pocket_type": "MAGAZINE_WELL",
                "flag_restriction": ["BATTERY_MEDIUM"],
                "default_magazine": "test_medium_battery"
            }]
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base tool should load"));
        let inherited = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_inherited_tool",
            "copy-from": "test_portable_tool",
            "name": "test inherited tool"
        }));
        assert!(
            load_one(&inherited, &mut items, &mut abstracts).expect("inherited tool should load")
        );
        let battery = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_medium_battery",
            "subtypes": ["MAGAZINE"],
            "name": "test medium battery",
            "ammo_type": "battery",
            "capacity": 56,
            "flags": ["BATTERY_MEDIUM"],
            "pocket_data": [{
                "pocket_type": "MAGAZINE",
                "ammo_restriction": {"battery": 56}
            }]
        }));
        assert!(load_one(&battery, &mut items, &mut abstracts).expect("battery should load"));
        let replacement = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_integral_tool",
            "copy-from": "test_portable_tool",
            "name": "test integral tool",
            "pocket_data": [{
                "pocket_type": "MAGAZINE",
                "ammo_restriction": {"battery": 20}
            }]
        }));
        assert!(
            load_one(&replacement, &mut items, &mut abstracts)
                .expect("replacement tool should load")
        );

        assert_eq!(items["test_inherited_tool"].magazine_wells.len(), 1);
        assert_eq!(
            items["test_inherited_tool"].magazine_wells[0].default_magazine,
            "test_medium_battery"
        );
        assert!(
            items["test_inherited_tool"]
                .unsupported_fields
                .contains("pocket_data")
        );
        assert_eq!(items["test_medium_battery"].magazine_capacity, 56);
        assert_eq!(
            items["test_medium_battery"].integral_magazines,
            [BTreeMap::from([(String::from("battery"), 56)])]
        );
        assert!(
            !items["test_medium_battery"]
                .unsupported_fields
                .contains("capacity")
        );
        assert!(items["test_integral_tool"].magazine_wells.is_empty());
        assert_eq!(
            items["test_integral_tool"].integral_magazines,
            [BTreeMap::from([(String::from("battery"), 20)])]
        );
        let registry = ItemRegistry {
            items,
            tool_subtype_replacements: BTreeMap::new(),
            abstract_count: 0,
        };
        let inherited_well = &registry
            .get("test_inherited_tool")
            .expect("inherited tool remains")
            .magazine_wells[0];
        assert_eq!(
            registry.compatible_magazines(inherited_well),
            ["test_medium_battery"]
        );
    }

    #[test]
    fn tool_subtypes_replace_every_ancestor_in_stable_item_order() {
        let tool = |id: &str, subtype: &str| ItemDefinition {
            id: id.to_owned(),
            subtypes: BTreeSet::from([String::from("TOOL")]),
            tool_subtype: subtype.to_owned(),
            ..ItemDefinition::default()
        };
        let items = BTreeMap::from([
            (
                String::from("chemistry_set"),
                tool("chemistry_set", "hotplate"),
            ),
            (String::from("hotplate"), tool("hotplate", "")),
            (
                String::from("lab_station"),
                tool("lab_station", "chemistry_set"),
            ),
        ]);
        let replacements = build_tool_subtype_replacements(&items).expect("valid subtype graph");
        assert_eq!(
            replacements["hotplate"],
            vec![String::from("chemistry_set"), String::from("lab_station")]
        );
        assert_eq!(
            replacements["chemistry_set"],
            vec![String::from("lab_station")]
        );
    }

    #[test]
    fn cyclic_tool_subtypes_are_rejected() {
        let tool = |id: &str, subtype: &str| ItemDefinition {
            id: id.to_owned(),
            subtypes: BTreeSet::from([String::from("TOOL")]),
            tool_subtype: subtype.to_owned(),
            ..ItemDefinition::default()
        };
        let items = BTreeMap::from([
            (String::from("a"), tool("a", "b")),
            (String::from("b"), tool("b", "a")),
        ]);
        assert!(matches!(
            build_tool_subtype_replacements(&items),
            Err(ItemRegistryError::CyclicToolSubtype { .. })
        ));
    }

    #[test]
    fn default_charges_follow_tool_comestible_ammunition_precedence() {
        let item = |subtypes: &[&str], charges, count, stackable| ItemDefinition {
            subtypes: subtypes
                .iter()
                .map(|subtype| (*subtype).to_owned())
                .collect(),
            charges,
            count,
            stackable,
            ..ItemDefinition::default()
        };
        assert_eq!(
            item(&["TOOL", "COMESTIBLE", "AMMO"], 3, 40, false).default_charges(),
            3
        );
        assert_eq!(
            item(&["COMESTIBLE", "AMMO"], 2, 40, false).default_charges(),
            2
        );
        assert_eq!(item(&["AMMO"], 0, 40, false).default_charges(), 40);
        assert_eq!(item(&[], 0, 0, true).default_charges(), 1);
        assert_eq!(item(&[], 0, 0, false).default_charges(), 0);
    }
}
