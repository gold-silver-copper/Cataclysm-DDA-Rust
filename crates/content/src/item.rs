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
    "expand_snippets",
    "category",
    "weight",
    "volume",
    "longest_side",
    "price",
    "price_postapoc",
    "symbol",
    "color",
    "ascii_picture",
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
    "container",
    "container_variant",
    "sealed",
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
    "variant_type",
    "variants",
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
    /// Whether the finalized base description is recursively expanded through
    /// the selected snippet library when an instance is constructed.
    pub expand_description_snippets: bool,
    pub category: String,
    pub weight_milligrams: i64,
    pub volume_milliliters: i64,
    /// Explicit or inherited longest item dimension in integer millimeters.
    /// When the source never sets this field, `finalized_longest_side_millimeters`
    /// derives the pinned volume-based default.
    pub longest_side_millimeters: i64,
    pub longest_side_is_explicit: bool,
    pub price_cents: i64,
    pub price_postapoc_cents: i64,
    pub symbol: String,
    pub color: String,
    pub ascii_picture: String,
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
    /// Finalized item-type default container. The literal `null` sentinel is
    /// retained because it explicitly disables inherited containment.
    pub default_container: String,
    pub default_container_variant: String,
    /// An absent value is upstream's default `true`; `Some` preserves an
    /// explicit or inherited item-type sealing policy.
    pub default_container_sealed: Option<bool>,
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
    /// Every inherited `pocket_data` entry in source order. The raw field map
    /// is retained losslessly so admission code can distinguish a strict
    /// supported pocket from one containing behavior that Rust does not yet
    /// interpret.
    pub pockets: Vec<PocketDefinition>,
    /// Strictly parsed MAGAZINE_WELL projections from inherited `pocket_data`.
    /// The full pocket field remains explicitly unsupported until general
    /// containment semantics are implemented.
    pub magazine_wells: Vec<MagazineWellDefinition>,
    /// Strictly parsed integral MAGAZINE ammo restrictions from inherited
    /// `pocket_data`; one entry maps an ammunition category to capacity.
    pub integral_magazines: Vec<BTreeMap<String, i32>>,
    /// Strict ammo-restricted CONTAINER pockets from inherited `pocket_data`.
    /// Mixed layouts remain closed so projecting these pockets cannot silently
    /// discard unsupported sibling pocket behavior.
    pub ammunition_containers: Vec<StrictAmmunitionContainerDefinition>,
    /// Strict general spawn-time pockets used by item-group containment. This
    /// is separate from ammunition pockets so existing reload semantics stay
    /// fail-closed while wrapper/contents insertion gains a reusable engine.
    pub spawn_pockets: Vec<StrictSpawnPocketDefinition>,
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
    /// Finalized source-ordered immutable appearance variants. Unsupported
    /// per-variant behavior is retained explicitly for strict admission.
    pub variants: Vec<ItemVariantDefinition>,
    pub variant_type: String,
    /// Strict inline snippet choices retained in source order. Named snippet
    /// categories and expansion remain explicitly unsupported.
    pub snippets: Vec<ItemSnippetDefinition>,
    /// Typed default item variables copied into every new instance. The raw
    /// field remains unsupported outside constructors that explicitly opt in.
    pub variables: BTreeMap<String, ItemVariableValueDefinition>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemVariantDefinition {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub symbol: Option<String>,
    pub color: Option<String>,
    pub ascii_picture: Option<String>,
    pub weight: u32,
    pub append: bool,
    pub expand_description_snippets: bool,
    pub unsupported_fields: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemTemperatureRuntimeClass {
    NotTracked,
    MateriallessNonperishable,
    RequiresRot,
    RequiresCustomFreezing,
    RequiresMaterialThermodynamics,
    UnsupportedPhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemSnippetDefinition {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemVariableValueDefinition {
    Integer(i64),
    String(String),
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
    pub pocket_index: u16,
    pub pocket_id: String,
    pub default_magazine: String,
    pub item_restrictions: BTreeSet<String>,
    pub flag_restrictions: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PocketTypeDefinition {
    Container,
    Magazine,
    MagazineWell,
    Mod,
    Corpse,
    Software,
    EFileStorage,
    Cable,
    Migration,
    Ebook,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PocketDefinition {
    pub pocket_index: u16,
    pub pocket_type: PocketTypeDefinition,
    pub pocket_id: String,
    pub ammo_restrictions: BTreeMap<String, i32>,
    pub item_restrictions: BTreeSet<String>,
    pub flag_restrictions: BTreeSet<String>,
    pub default_magazine: String,
    pub raw_fields: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictMagazineDefinition {
    pub pocket_index: u16,
    pub pocket_id: String,
    pub ammunition_type: String,
    pub capacity: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictAmmunitionContainerDefinition {
    pub pocket_index: u16,
    pub pocket_id: String,
    pub capacities: BTreeMap<String, u32>,
    pub access_moves: u16,
    pub rigid: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnPocketKindDefinition {
    Container,
    EFileStorage,
}

const DEFAULT_SPAWN_POCKET_VOLUME_MILLILITERS: u64 = 200_000_000;
const DEFAULT_SPAWN_POCKET_WEIGHT_MILLIGRAMS: u64 = 2_000_000_000_000;

fn default_spawn_pocket_max_item_length_millimeters(volume_milliliters: u64) -> Option<u64> {
    // Pinned `pocket_data::load`: round the cubic side to centimeters, convert
    // to millimeters, multiply by sqrt(2), then truncate into integer length.
    let length = (volume_milliliters as f64).cbrt().round() * 10.0 * std::f64::consts::SQRT_2;
    if !length.is_finite() || length < 0.0 || length > u64::MAX as f64 {
        return None;
    }
    Some(length as u64)
}

/// A pocket shape whose spawn-time compatibility, capacity, sealing, and
/// access state can be represented without consulting live JSON.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictSpawnPocketDefinition {
    pub pocket_index: u16,
    pub pocket_id: String,
    pub kind: SpawnPocketKindDefinition,
    pub max_contains_volume_milliliters: u64,
    /// Volume already included by the owning item before a flexible pocket
    /// expands externally. Pinned JSON calls this `magazine_well` even for
    /// ordinary general-container pockets.
    pub magazine_well_volume_milliliters: u64,
    pub max_contains_weight_milligrams: u64,
    pub max_item_volume_milliliters: u64,
    pub min_item_volume_milliliters: u64,
    pub max_item_length_millimeters: u64,
    pub item_restrictions: BTreeSet<String>,
    pub flag_restrictions: BTreeSet<String>,
    pub access_moves: u16,
    pub rigid: bool,
    pub watertight: bool,
    pub transparent: bool,
    pub forbidden: bool,
    pub sealable: bool,
}

impl PocketDefinition {
    /// A strict integral magazine has no behavior beyond ammunition category
    /// and capacity. Other preserved fields keep the pocket fail-closed.
    #[must_use]
    pub fn strict_integral_magazine(&self) -> Option<&BTreeMap<String, i32>> {
        const FIELDS: &[&str] = &["ammo_restriction", "id", "pocket_type", "rigid"];
        (self.pocket_type == PocketTypeDefinition::Magazine
            && !self.ammo_restrictions.is_empty()
            && self
                .raw_fields
                .keys()
                .all(|field| FIELDS.contains(&field.as_str()))
            && self.raw_fields.get("rigid").is_none_or(Value::is_boolean))
        .then_some(&self.ammo_restrictions)
    }

    /// A strict detachable well only selects compatible magazine definitions.
    /// Capacity, sealing, holster, overflow, and other pocket behavior remain
    /// unavailable until their interpreters exist.
    #[must_use]
    pub fn strict_magazine_well(&self) -> bool {
        const FIELDS: &[&str] = &[
            "default_magazine",
            "flag_restriction",
            "id",
            "item_restriction",
            "pocket_type",
            "rigid",
        ];
        self.pocket_type == PocketTypeDefinition::MagazineWell
            && (!self.item_restrictions.is_empty()
                || !self.flag_restrictions.is_empty()
                || !self.default_magazine.is_empty())
            && self
                .raw_fields
                .keys()
                .all(|field| FIELDS.contains(&field.as_str()))
            && self.raw_fields.get("rigid").is_none_or(Value::is_boolean)
    }

    /// A strict ammunition container uses only category capacities and the
    /// base access cost. General volume, length, sealing, holster, and
    /// encumbrance behavior remains unavailable rather than being discarded.
    #[must_use]
    pub fn strict_ammunition_container(&self) -> Option<StrictAmmunitionContainerDefinition> {
        const FIELDS: &[&str] = &["ammo_restriction", "id", "moves", "pocket_type", "rigid"];
        if self.pocket_type != PocketTypeDefinition::Container
            || self.ammo_restrictions.is_empty()
            || !self.raw_fields.contains_key("ammo_restriction")
            || !self
                .raw_fields
                .keys()
                .all(|field| FIELDS.contains(&field.as_str()))
        {
            return None;
        }
        let access_moves = match self.raw_fields.get("moves") {
            Some(value) => u16::try_from(value.as_i64()?)
                .ok()
                .filter(|moves| *moves > 0)?,
            None => 100,
        };
        let rigid = match self.raw_fields.get("rigid") {
            Some(value) => value.as_bool()?,
            None => false,
        };
        let capacities = self
            .ammo_restrictions
            .iter()
            .map(|(ammunition_type, capacity)| {
                Some((ammunition_type.clone(), u32::try_from(*capacity).ok()?))
            })
            .collect::<Option<BTreeMap<_, _>>>()?;
        Some(StrictAmmunitionContainerDefinition {
            pocket_index: self.pocket_index,
            pocket_id: self.pocket_id.clone(),
            capacities,
            access_moves,
            rigid,
        })
    }

    /// Strict spawn-time projection for ordinary physical containers and
    /// electronic-file storage. Reload-only ammunition containers keep using
    /// their dedicated interpreter.
    #[must_use]
    pub fn strict_spawn_pocket(&self) -> Option<StrictSpawnPocketDefinition> {
        const FIELDS: &[&str] = &[
            "//",
            "ememory_max",
            "flag_restriction",
            "forbidden",
            "id",
            "item_restriction",
            "magazine_well",
            "max_contains_volume",
            "max_contains_weight",
            "max_item_length",
            "max_item_volume",
            "min_item_volume",
            "moves",
            "pocket_type",
            "rigid",
            "sealed_data",
            "transparent",
            "watertight",
            "weight_multiplier",
        ];
        let kind = match self.pocket_type {
            PocketTypeDefinition::Container if self.ammo_restrictions.is_empty() => {
                SpawnPocketKindDefinition::Container
            }
            PocketTypeDefinition::EFileStorage => SpawnPocketKindDefinition::EFileStorage,
            _ => return None,
        };
        if !self
            .raw_fields
            .keys()
            .all(|field| FIELDS.contains(&field.as_str()) || field.starts_with("//"))
        {
            return None;
        }
        let weight_multiplier = match self.raw_fields.get("weight_multiplier") {
            Some(value) => Some(value.as_f64()?),
            None => None,
        };
        if (kind == SpawnPocketKindDefinition::EFileStorage
            && (weight_multiplier != Some(0.0)
                || self.raw_fields.get("rigid").and_then(Value::as_bool) != Some(true)))
            || (kind == SpawnPocketKindDefinition::Container
                && weight_multiplier.is_some_and(|multiplier| multiplier != 1.0))
        {
            return None;
        }
        if kind == SpawnPocketKindDefinition::EFileStorage {
            self.raw_fields
                .get("ememory_max")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())?;
        }
        let quantity = |field, quantity_kind, default| {
            self.raw_fields.get(field).map_or(Some(default), |value| {
                u64::try_from(parse_quantity(value, quantity_kind, "pocket_data", field).ok()?).ok()
            })
        };
        let boolean = |field, default| {
            self.raw_fields
                .get(field)
                .map_or(Some(default), Value::as_bool)
        };
        let access_moves = match self.raw_fields.get("moves") {
            Some(value) => u16::try_from(value.as_i64()?)
                .ok()
                .filter(|moves| *moves > 0)?,
            None => 100,
        };
        let sealable = match self.raw_fields.get("sealed_data") {
            None => false,
            Some(Value::Object(data))
                if data.keys().all(|field| field == "spoil_multiplier")
                    && data
                        .get("spoil_multiplier")
                        .and_then(Value::as_f64)
                        .is_some_and(|value| value.is_finite() && value >= 0.0) =>
            {
                true
            }
            Some(_) => return None,
        };
        let max_contains_volume_milliliters = quantity(
            "max_contains_volume",
            QuantityKind::Volume,
            DEFAULT_SPAWN_POCKET_VOLUME_MILLILITERS,
        )?;
        let max_contains_weight_milligrams = quantity(
            "max_contains_weight",
            QuantityKind::Mass,
            DEFAULT_SPAWN_POCKET_WEIGHT_MILLIGRAMS,
        )?;
        let magazine_well_volume_milliliters = quantity("magazine_well", QuantityKind::Volume, 0)?;
        if magazine_well_volume_milliliters >= max_contains_volume_milliliters
            || (boolean("rigid", false)? && magazine_well_volume_milliliters > 0)
            || (kind == SpawnPocketKindDefinition::EFileStorage
                && magazine_well_volume_milliliters > 0)
        {
            return None;
        }
        let max_item_length_millimeters = match self.raw_fields.get("max_item_length") {
            Some(value) => u64::try_from(
                parse_quantity(
                    value,
                    QuantityKind::Length,
                    "pocket_data",
                    "max_item_length",
                )
                .ok()?,
            )
            .ok()?,
            None => {
                default_spawn_pocket_max_item_length_millimeters(max_contains_volume_milliliters)?
            }
        };
        Some(StrictSpawnPocketDefinition {
            pocket_index: self.pocket_index,
            pocket_id: self.pocket_id.clone(),
            kind,
            max_contains_volume_milliliters,
            magazine_well_volume_milliliters,
            max_contains_weight_milligrams,
            max_item_volume_milliliters: quantity(
                "max_item_volume",
                QuantityKind::Volume,
                u64::MAX,
            )?,
            min_item_volume_milliliters: quantity("min_item_volume", QuantityKind::Volume, 0)?,
            max_item_length_millimeters,
            item_restrictions: self.item_restrictions.clone(),
            flag_restrictions: self.flag_restrictions.clone(),
            access_moves,
            rigid: boolean("rigid", false)?,
            watertight: boolean("watertight", false)?,
            transparent: boolean("transparent", false)?,
            forbidden: boolean("forbidden", false)?,
            sealable,
        })
    }
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
    /// Classifies the finalized pinned constructor without implying support
    /// for material thermodynamics, rot, or weather.
    #[must_use]
    pub fn temperature_runtime_class(&self) -> ItemTemperatureRuntimeClass {
        if !self.subtypes.contains("COMESTIBLE") || self.flags.contains("NO_TEMP") {
            return ItemTemperatureRuntimeClass::NotTracked;
        }
        if self.unsupported_fields.contains("spoils_in") {
            return ItemTemperatureRuntimeClass::RequiresRot;
        }
        if self.unsupported_fields.contains("freezing_point") {
            return ItemTemperatureRuntimeClass::RequiresCustomFreezing;
        }
        if !self.materials.is_empty() {
            return ItemTemperatureRuntimeClass::RequiresMaterialThermodynamics;
        }
        if !matches!(
            self.phase.to_ascii_lowercase().as_str(),
            "" | "solid" | "liquid"
        ) {
            return ItemTemperatureRuntimeClass::UnsupportedPhase;
        }
        ItemTemperatureRuntimeClass::MateriallessNonperishable
    }

    /// Pinned `Item_factory::finalize_pre` default: round the cube root of the
    /// effective one-charge volume to whole centimeters.
    #[must_use]
    pub fn finalized_longest_side_millimeters(&self) -> Option<u64> {
        if self.longest_side_is_explicit {
            return u64::try_from(self.longest_side_millimeters).ok();
        }
        let mut effective_volume = u64::try_from(self.volume_milliliters).ok()?;
        if self.count_by_charges() && self.stack_size > 0 {
            effective_volume /= u64::try_from(self.stack_size).ok()?;
        }
        let centimeters = (effective_volume as f64).cbrt().round();
        if !centimeters.is_finite() || centimeters < 0.0 {
            return None;
        }
        (centimeters as u64).checked_mul(10)
    }

    /// Returns the generalized runtime shape for a concrete, single-pocket
    /// magazine. Multiple ammunition categories and pockets with extra
    /// behavior remain unavailable rather than being projected lossily.
    #[must_use]
    pub fn strict_magazine(&self) -> Option<StrictMagazineDefinition> {
        let [pocket] = self.pockets.as_slice() else {
            return None;
        };
        let restrictions = pocket.strict_integral_magazine()?;
        let mut restrictions = restrictions.iter();
        let (ammunition_type, capacity) = restrictions.next()?;
        if restrictions.next().is_some() {
            return None;
        }
        let capacity = u32::try_from(*capacity).ok()?;
        (self.subtypes.contains("MAGAZINE")
            && self.ammo_types.len() == 1
            && self.ammo_types.contains(ammunition_type.as_str())
            && self.magazine_capacity == i32::try_from(capacity).ok()?
            && capacity > 0)
            .then(|| StrictMagazineDefinition {
                pocket_index: pocket.pocket_index,
                pocket_id: pocket.pocket_id.clone(),
                ammunition_type: ammunition_type.clone(),
                capacity,
            })
    }

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
    apply_boolean(
        object,
        "expand_snippets",
        &mut item.expand_description_snippets,
        source,
    )?;
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
    let longest_side_is_modified = object.contains_key("longest_side")
        || ["relative", "proportional"].into_iter().any(|modifier| {
            object
                .get(modifier)
                .and_then(Value::as_object)
                .is_some_and(|fields| fields.contains_key("longest_side"))
        });
    apply_quantity(
        object,
        "longest_side",
        &mut item.longest_side_millimeters,
        QuantityKind::Length,
        source,
    )?;
    item.longest_side_is_explicit |= longest_side_is_modified;
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
    apply_string(object, "ascii_picture", &mut item.ascii_picture, source)?;
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
    apply_string(object, "container", &mut item.default_container, source)?;
    apply_string(
        object,
        "container_variant",
        &mut item.default_container_variant,
        source,
    )?;
    if let Some(value) = object.get("sealed") {
        item.default_container_sealed = Some(
            value
                .as_bool()
                .ok_or_else(|| invalid_field(source, "sealed"))?,
        );
    }
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
    apply_string(object, "variant_type", &mut item.variant_type, source)?;
    apply_item_variants(object, item, source)?;
    apply_inline_snippets(object, item, source)?;
    apply_item_variables(object, item, source)?;
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

fn apply_inline_snippets(
    object: &Map<String, Value>,
    item: &mut ItemDefinition,
    source: &str,
) -> Result<(), ItemRegistryError> {
    let Some(Value::Array(values)) = object.get("snippet_category") else {
        if object.contains_key("snippet_category") {
            item.snippets.clear();
        }
        return Ok(());
    };
    let mut ids = BTreeSet::new();
    item.snippets = values
        .iter()
        .map(|value| {
            let snippet = value
                .as_object()
                .ok_or_else(|| invalid_field(source, "snippet_category"))?;
            if snippet
                .keys()
                .any(|field| !matches!(field.as_str(), "//" | "id" | "text"))
            {
                return Err(invalid_field(source, "snippet_category"));
            }
            let id = snippet
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid_field(source, "snippet_category"))?
                .to_owned();
            if !ids.insert(id.clone()) {
                return Err(invalid_field(source, "snippet_category"));
            }
            let text = snippet
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .ok_or_else(|| invalid_field(source, "snippet_category"))?
                .to_owned();
            Ok(ItemSnippetDefinition { id, text })
        })
        .collect::<Result<Vec<_>, ItemRegistryError>>()?;
    if item.snippets.is_empty() {
        return Err(invalid_field(source, "snippet_category"));
    }
    Ok(())
}

fn apply_item_variables(
    object: &Map<String, Value>,
    item: &mut ItemDefinition,
    source: &str,
) -> Result<(), ItemRegistryError> {
    let Some(value) = object.get("variables") else {
        return Ok(());
    };
    let variables = value
        .as_object()
        .ok_or_else(|| invalid_field(source, "variables"))?;
    item.variables = variables
        .iter()
        .map(|(key, value)| {
            if key.is_empty() {
                return Err(invalid_field(source, "variables"));
            }
            let value = match value {
                Value::String(value) => ItemVariableValueDefinition::String(value.clone()),
                Value::Number(value) => ItemVariableValueDefinition::Integer(
                    value
                        .as_i64()
                        .ok_or_else(|| invalid_field(source, "variables"))?,
                ),
                _ => return Err(invalid_field(source, "variables")),
            };
            Ok((key.clone(), value))
        })
        .collect::<Result<BTreeMap<_, _>, ItemRegistryError>>()?;
    Ok(())
}

fn apply_item_variants(
    object: &Map<String, Value>,
    item: &mut ItemDefinition,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get("variants") {
        item.variants = parse_item_variants(value, source)?;
    }
    if let Some(value) = modifier(object, "extend", "variants", source)? {
        item.variants.extend(parse_item_variants(value, source)?);
    }
    if let Some(value) = modifier(object, "delete", "variants", source)? {
        let deleted = parse_deleted_variant_ids(value, source)?;
        item.variants
            .retain(|variant| !deleted.contains(&variant.id));
    }
    if !item.variants.is_empty() && item.variant_type.is_empty() {
        item.variant_type = String::from("generic");
    }
    Ok(())
}

fn parse_item_variants(
    value: &Value,
    source: &str,
) -> Result<Vec<ItemVariantDefinition>, ItemRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid_field(source, "variants"))?
        .iter()
        .map(|value| parse_item_variant(value, source))
        .collect()
}

fn parse_item_variant(
    value: &Value,
    source: &str,
) -> Result<ItemVariantDefinition, ItemRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_field(source, "variants"))?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid_field(source, "variants"))?
        .to_owned();
    let weight = object.get("weight").map_or(Ok(1), |weight| {
        u32::try_from(
            weight
                .as_i64()
                .filter(|weight| *weight >= 0)
                .ok_or_else(|| invalid_field(source, "variants"))?,
        )
        .map_err(|_| invalid_field(source, "variants"))
    })?;
    let unsupported_fields = object
        .keys()
        .filter(|field| {
            !field.starts_with("//")
                && !matches!(
                    field.as_str(),
                    "id" | "name"
                        | "description"
                        | "symbol"
                        | "color"
                        | "ascii_picture"
                        | "weight"
                        | "append"
                        | "expand_snippets"
                )
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    let expand_description_snippets = match object.get("expand_snippets") {
        None => false,
        Some(Value::Bool(expand)) => *expand,
        Some(_) => return Err(invalid_field(source, "variants")),
    };
    Ok(ItemVariantDefinition {
        id,
        name: optional_text(object, "name", source)?,
        description: optional_text(object, "description", source)?,
        symbol: optional_owned_string(object, "symbol", source)?,
        color: optional_owned_string(object, "color", source)?,
        ascii_picture: optional_owned_string(object, "ascii_picture", source)?,
        weight,
        append: match object.get("append") {
            None => false,
            Some(Value::Bool(append)) => *append,
            Some(_) => return Err(invalid_field(source, "variants")),
        },
        expand_description_snippets,
        unsupported_fields,
    })
}

fn apply_boolean(
    object: &Map<String, Value>,
    field: &str,
    target: &mut bool,
    source: &str,
) -> Result<(), ItemRegistryError> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_bool()
            .ok_or_else(|| invalid_field(source, field))?;
    }
    Ok(())
}

fn parse_deleted_variant_ids(
    value: &Value,
    source: &str,
) -> Result<BTreeSet<String>, ItemRegistryError> {
    value
        .as_array()
        .ok_or_else(|| invalid_field(source, "variants"))?
        .iter()
        .map(|value| match value {
            Value::String(id) if !id.is_empty() => Ok(id.clone()),
            Value::Object(object) => object
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid_field(source, "variants")),
            _ => Err(invalid_field(source, "variants")),
        })
        .collect()
}

fn optional_text(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<String>, ItemRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    match value {
        Value::String(value) => Ok(Some(value.clone())),
        Value::Object(value) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| value.get(key).and_then(Value::as_str))
            .map(str::to_owned)
            .map(Some)
            .ok_or_else(|| invalid_field(source, "variants")),
        _ => Err(invalid_field(source, "variants")),
    }
}

fn optional_owned_string(
    object: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<String>, ItemRegistryError> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_field(source, "variants"))
        })
        .transpose()
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
    let mut normalized_pockets = Vec::with_capacity(pockets.len());
    for (index, value) in pockets.iter().enumerate() {
        let pocket = value
            .as_object()
            .ok_or_else(|| invalid_field(source, "pocket_data"))?;
        let pocket_index =
            u16::try_from(index).map_err(|_| invalid_field(source, "pocket_data"))?;
        normalized_pockets.push(parse_pocket_definition(pocket, pocket_index, source)?);
    }
    let mut magazine_wells = Vec::new();
    let mut integral_magazines = Vec::new();
    for pocket in &normalized_pockets {
        match pocket.pocket_type {
            PocketTypeDefinition::MagazineWell => {
                magazine_wells.push(MagazineWellDefinition {
                    pocket_index: pocket.pocket_index,
                    pocket_id: pocket.pocket_id.clone(),
                    default_magazine: pocket.default_magazine.clone(),
                    item_restrictions: pocket.item_restrictions.clone(),
                    flag_restrictions: pocket.flag_restrictions.clone(),
                });
            }
            PocketTypeDefinition::Magazine if !pocket.ammo_restrictions.is_empty() => {
                integral_magazines.push(pocket.ammo_restrictions.clone());
            }
            _ => (),
        }
    }
    let all_pockets_have_supported_shapes = normalized_pockets.iter().all(|pocket| {
        pocket.strict_integral_magazine().is_some()
            || pocket.strict_magazine_well()
            || pocket.strict_ammunition_container().is_some()
            || pocket.strict_spawn_pocket().is_some()
    });
    let ammunition_containers = if all_pockets_have_supported_shapes {
        normalized_pockets
            .iter()
            .filter_map(PocketDefinition::strict_ammunition_container)
            .collect()
    } else {
        Vec::new()
    };
    let spawn_pockets = if all_pockets_have_supported_shapes {
        normalized_pockets
            .iter()
            .filter_map(PocketDefinition::strict_spawn_pocket)
            .collect()
    } else {
        Vec::new()
    };
    item.pockets = normalized_pockets;
    item.magazine_wells = magazine_wells;
    item.integral_magazines = integral_magazines;
    item.ammunition_containers = ammunition_containers;
    item.spawn_pockets = spawn_pockets;
    Ok(())
}

fn parse_pocket_definition(
    pocket: &Map<String, Value>,
    pocket_index: u16,
    source: &str,
) -> Result<PocketDefinition, ItemRegistryError> {
    let pocket_type = match pocket
        .get("pocket_type")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| invalid_field(source, "pocket_data"))
        })
        .transpose()?
        .unwrap_or("CONTAINER")
    {
        "CONTAINER" => PocketTypeDefinition::Container,
        "MAGAZINE" => PocketTypeDefinition::Magazine,
        "MAGAZINE_WELL" => PocketTypeDefinition::MagazineWell,
        "MOD" => PocketTypeDefinition::Mod,
        "CORPSE" => PocketTypeDefinition::Corpse,
        "SOFTWARE" => PocketTypeDefinition::Software,
        "E_FILE_STORAGE" => PocketTypeDefinition::EFileStorage,
        "CABLE" => PocketTypeDefinition::Cable,
        "MIGRATION" => PocketTypeDefinition::Migration,
        "EBOOK" => PocketTypeDefinition::Ebook,
        _ => return Err(invalid_field(source, "pocket_data")),
    };
    let ammo_restrictions = pocket
        .get("ammo_restriction")
        .map(|value| parse_ammo_restrictions(value, source))
        .transpose()?
        .unwrap_or_default();
    let item_restriction_value = pocket.get("item_restriction");
    let item_restrictions = item_restriction_value
        .map(|value| string_set(value, source, "pocket_data"))
        .transpose()?
        .unwrap_or_default();
    if item_restriction_value.is_some() && item_restrictions.is_empty() {
        return Err(invalid_field(source, "pocket_data"));
    }
    let flag_restriction_value = pocket.get("flag_restriction");
    let flag_restrictions = flag_restriction_value
        .map(|value| string_set(value, source, "pocket_data"))
        .transpose()?
        .unwrap_or_default();
    if flag_restriction_value.is_some() && flag_restrictions.is_empty() {
        return Err(invalid_field(source, "pocket_data"));
    }
    let pocket_id = optional_nonempty_pocket_string(pocket, "id", source)?;
    let mut default_magazine = optional_nonempty_pocket_string(pocket, "default_magazine", source)?;
    if let Some(first) = item_restriction_value
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_str)
    {
        default_magazine = first.to_owned();
    }
    Ok(PocketDefinition {
        pocket_index,
        pocket_type,
        pocket_id,
        ammo_restrictions,
        item_restrictions,
        flag_restrictions,
        default_magazine,
        raw_fields: pocket
            .iter()
            .map(|(field, value)| (field.clone(), value.clone()))
            .collect(),
    })
}

fn parse_ammo_restrictions(
    value: &Value,
    source: &str,
) -> Result<BTreeMap<String, i32>, ItemRegistryError> {
    let restrictions = value
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
    Ok(restrictions)
}

fn optional_nonempty_pocket_string(
    pocket: &Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<String, ItemRegistryError> {
    pocket
        .get(field)
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| invalid_field(source, "pocket_data"))
        })
        .transpose()
        .map(Option::unwrap_or_default)
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
    Length,
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
            (QuantityKind::Length, "mm") => 1,
            (QuantityKind::Length, "cm") => 10,
            (QuantityKind::Length, "m" | "meter" | "meters") => 1_000,
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
    fn missing_longest_side_uses_pinned_finalized_volume_derivation() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        for definition in [
            serde_json::json!({
                "type": "ITEM",
                "id": "derived_cube",
                "name": "derived cube",
                "volume": "1 L"
            }),
            serde_json::json!({
                "type": "ITEM",
                "id": "derived_charge",
                "subtypes": ["AMMO"],
                "name": "derived charge",
                "volume": "1 L",
                "stack_size": 8
            }),
            serde_json::json!({
                "type": "ITEM",
                "id": "explicit_zero",
                "name": "explicit zero",
                "volume": "1 L",
                "longest_side": "0 mm"
            }),
        ] {
            assert!(
                load_one(&raw(definition), &mut items, &mut abstracts)
                    .expect("length fixture should load")
            );
        }
        assert_eq!(
            items["derived_cube"].finalized_longest_side_millimeters(),
            Some(100)
        );
        assert_eq!(
            items["derived_charge"].finalized_longest_side_millimeters(),
            Some(50)
        );
        assert_eq!(
            items["explicit_zero"].finalized_longest_side_millimeters(),
            Some(0)
        );
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
    fn default_container_identity_variant_and_sealing_inherit_exactly() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_contained",
            "name": "test contained item",
            "container": "test_bottle",
            "container_variant": "blue",
            "sealed": false
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base item should load"));
        let inherited = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_inherited_container",
            "copy-from": "test_contained",
            "name": "inherited contained item"
        }));
        assert!(
            load_one(&inherited, &mut items, &mut abstracts)
                .expect("inherited default container should load")
        );
        let inherited = &items["test_inherited_container"];
        assert_eq!(inherited.default_container, "test_bottle");
        assert_eq!(inherited.default_container_variant, "blue");
        assert_eq!(inherited.default_container_sealed, Some(false));
        assert!(!inherited.unsupported_fields.contains("container"));
        assert!(!inherited.unsupported_fields.contains("container_variant"));
        assert!(!inherited.unsupported_fields.contains("sealed"));

        let disabled = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_disabled_container",
            "copy-from": "test_contained",
            "name": "uncontained item",
            "container": "null",
            "sealed": true
        }));
        assert!(
            load_one(&disabled, &mut items, &mut abstracts)
                .expect("null container sentinel should load")
        );
        let disabled = &items["test_disabled_container"];
        assert_eq!(disabled.default_container, "null");
        assert_eq!(disabled.default_container_sealed, Some(true));
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
        assert_eq!(items["test_inherited_tool"].pockets.len(), 1);
        assert_eq!(
            items["test_inherited_tool"].pockets[0].pocket_type,
            PocketTypeDefinition::MagazineWell
        );
        assert!(items["test_inherited_tool"].pockets[0].strict_magazine_well());
        assert_eq!(
            items["test_inherited_tool"].pockets[0]
                .raw_fields
                .get("flag_restriction"),
            Some(&serde_json::json!(["BATTERY_MEDIUM"]))
        );
        assert_eq!(
            items["test_inherited_tool"].magazine_wells[0].pocket_index,
            0
        );
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
        assert_eq!(
            items["test_medium_battery"].pockets[0].strict_integral_magazine(),
            Some(&BTreeMap::from([(String::from("battery"), 56)]))
        );
        assert_eq!(
            items["test_medium_battery"].strict_magazine(),
            Some(StrictMagazineDefinition {
                pocket_index: 0,
                pocket_id: String::new(),
                ammunition_type: String::from("battery"),
                capacity: 56,
            })
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
    fn pocket_shape_is_preserved_while_unsupported_runtime_behavior_stays_closed() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let magazine = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_sealed_magazine",
            "subtypes": ["MAGAZINE"],
            "name": "sealed magazine",
            "ammo_type": "test_ammo",
            "capacity": 10,
            "pocket_data": [{
                "id": "main",
                "pocket_type": "MAGAZINE",
                "ammo_restriction": {"test_ammo": 10},
                "watertight": true,
                "moves": 75
            }]
        }));
        assert!(load_one(&magazine, &mut items, &mut abstracts).expect("shape should load"));
        let pocket = &items["test_sealed_magazine"].pockets[0];
        assert_eq!(pocket.pocket_index, 0);
        assert_eq!(pocket.pocket_id, "main");
        assert_eq!(
            pocket.raw_fields.get("watertight"),
            Some(&Value::Bool(true))
        );
        assert_eq!(pocket.raw_fields.get("moves"), Some(&serde_json::json!(75)));
        assert_eq!(pocket.strict_integral_magazine(), None);
        assert!(
            items["test_sealed_magazine"]
                .unsupported_fields
                .contains("pocket_data")
        );
    }

    #[test]
    fn strict_ammunition_container_projection_applies_defaults_and_inherits_replacements() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_quiver",
            "name": "test quiver",
            "pocket_data": [{
                "ammo_restriction": {"arrow": 20, "bolt": 20}
            }]
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base quiver should load"));
        let inherited = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_inherited_quiver",
            "copy-from": "test_quiver",
            "name": "inherited quiver"
        }));
        assert!(
            load_one(&inherited, &mut items, &mut abstracts).expect("inherited quiver should load")
        );
        let replacement = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_replacement_quiver",
            "copy-from": "test_quiver",
            "name": "replacement quiver",
            "pocket_data": [
                {
                    "id": "short",
                    "pocket_type": "CONTAINER",
                    "ammo_restriction": {"bolt": 12},
                    "moves": 20,
                    "rigid": true
                },
                {
                    "id": "long",
                    "ammo_restriction": {"arrow": 30, "atlatl": 5},
                    "moves": 30
                }
            ]
        }));
        assert!(
            load_one(&replacement, &mut items, &mut abstracts)
                .expect("replacement quiver should load")
        );

        let default_projection = StrictAmmunitionContainerDefinition {
            pocket_index: 0,
            pocket_id: String::new(),
            capacities: BTreeMap::from([(String::from("arrow"), 20), (String::from("bolt"), 20)]),
            access_moves: 100,
            rigid: false,
        };
        assert_eq!(
            items["test_quiver"].ammunition_containers,
            std::slice::from_ref(&default_projection)
        );
        assert_eq!(
            items["test_inherited_quiver"].ammunition_containers,
            [default_projection]
        );
        assert_eq!(
            items["test_replacement_quiver"].ammunition_containers,
            [
                StrictAmmunitionContainerDefinition {
                    pocket_index: 0,
                    pocket_id: String::from("short"),
                    capacities: BTreeMap::from([(String::from("bolt"), 12)]),
                    access_moves: 20,
                    rigid: true,
                },
                StrictAmmunitionContainerDefinition {
                    pocket_index: 1,
                    pocket_id: String::from("long"),
                    capacities: BTreeMap::from([
                        (String::from("arrow"), 30),
                        (String::from("atlatl"), 5),
                    ]),
                    access_moves: 30,
                    rigid: false,
                },
            ]
        );
    }

    #[test]
    fn strict_ammunition_container_projection_rejects_invalid_or_extra_behavior() {
        for restriction in [
            serde_json::json!({}),
            serde_json::json!({"": 1}),
            serde_json::json!({"arrow": 0}),
            serde_json::json!({"arrow": -1}),
            serde_json::json!({"arrow": i64::from(i32::MAX) + 1}),
            serde_json::json!({"arrow": 1.5}),
            serde_json::json!({"arrow": "20"}),
        ] {
            let value = serde_json::json!({"ammo_restriction": restriction});
            assert!(
                parse_pocket_definition(
                    value.as_object().expect("pocket should be an object"),
                    0,
                    "test"
                )
                .is_err()
            );
        }

        for moves in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(u64::from(u16::MAX) + 1),
            serde_json::json!(1.5),
            serde_json::json!("20"),
        ] {
            let value = serde_json::json!({
                "ammo_restriction": {"arrow": 20},
                "moves": moves
            });
            let pocket = parse_pocket_definition(
                value.as_object().expect("pocket should be an object"),
                0,
                "test",
            )
            .expect("raw pocket shape should remain preserved");
            assert_eq!(pocket.strict_ammunition_container(), None);
        }

        let invalid_rigid = serde_json::json!({
            "ammo_restriction": {"arrow": 20},
            "rigid": "true"
        });
        let pocket = parse_pocket_definition(
            invalid_rigid
                .as_object()
                .expect("pocket should be an object"),
            0,
            "test",
        )
        .expect("raw pocket shape should remain preserved");
        assert_eq!(pocket.strict_ammunition_container(), None);

        for (field, field_value) in [
            ("volume_encumber_modifier", serde_json::json!(0.3)),
            ("holster", serde_json::json!(true)),
            ("watertight", serde_json::json!(true)),
            ("max_contains_volume", serde_json::json!("1 L")),
            ("item_restriction", serde_json::json!(["arrow_wood"])),
            ("weight_multiplier", serde_json::json!(0.5)),
            ("description", serde_json::json!("unsupported behavior")),
            ("//", serde_json::json!("comments are not canonical fields")),
        ] {
            let mut pocket_value = serde_json::json!({
                "ammo_restriction": {"arrow": 20}
            });
            pocket_value
                .as_object_mut()
                .expect("pocket should be an object")
                .insert(field.to_owned(), field_value);
            let pocket = parse_pocket_definition(
                pocket_value
                    .as_object()
                    .expect("pocket should be an object"),
                0,
                "test",
            )
            .expect("raw pocket shape should remain preserved");
            assert_eq!(pocket.strict_ammunition_container(), None, "field {field}");
        }

        for invalid_type in [serde_json::json!("MAGIC"), serde_json::json!(1)] {
            let value = serde_json::json!({
                "pocket_type": invalid_type,
                "ammo_restriction": {"arrow": 20}
            });
            assert!(
                parse_pocket_definition(
                    value.as_object().expect("pocket should be an object"),
                    0,
                    "test"
                )
                .is_err()
            );
        }
    }

    #[test]
    fn supported_ammunition_and_spawn_pockets_coexist_by_index() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let mixed = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_mixed_quiver",
            "name": "mixed quiver",
            "pocket_data": [
                {"ammo_restriction": {"arrow": 20}, "moves": 20},
                {
                    "pocket_type": "CONTAINER",
                    "max_contains_volume": "1 L",
                    "max_contains_weight": "1 kg"
                }
            ]
        }));
        assert!(load_one(&mixed, &mut items, &mut abstracts).expect("mixed host should load"));
        assert_eq!(items["test_mixed_quiver"].pockets.len(), 2);
        assert!(
            items["test_mixed_quiver"].pockets[0]
                .strict_ammunition_container()
                .is_some()
        );
        assert_eq!(items["test_mixed_quiver"].ammunition_containers.len(), 1);
        assert_eq!(
            items["test_mixed_quiver"].ammunition_containers[0].pocket_index,
            0
        );
        assert_eq!(items["test_mixed_quiver"].spawn_pockets.len(), 1);
        assert_eq!(items["test_mixed_quiver"].spawn_pockets[0].pocket_index, 1);
        assert_eq!(
            items["test_mixed_quiver"].spawn_pockets[0].kind,
            SpawnPocketKindDefinition::Container
        );
    }

    #[test]
    fn strict_spawn_pockets_preserve_physical_and_efile_boundaries() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let host = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_phone_case",
            "name": "test phone case",
            "pocket_data": [
                {
                    "id": "physical",
                    "pocket_type": "CONTAINER",
                    "max_contains_volume": "2 L",
                    "max_contains_weight": "3 kg",
                    "max_item_volume": "1500 ml",
                    "min_item_volume": "5 ml",
                    "max_item_length": "30 cm",
                    "item_restriction": ["test_phone"],
                    "flag_restriction": ["ELECTRONIC"],
                    "moves": 80,
                    "rigid": true,
                    "watertight": true,
                    "transparent": true,
                    "sealed_data": {"spoil_multiplier": 0.0}
                },
                {
                    "id": "efiles",
                    "pocket_type": "E_FILE_STORAGE",
                    "ememory_max": "1 GB",
                    "weight_multiplier": 0,
                    "rigid": true
                }
            ]
        }));
        assert!(load_one(&host, &mut items, &mut abstracts).expect("host should load"));

        let pockets = &items["test_phone_case"].spawn_pockets;
        assert_eq!(pockets.len(), 2);
        assert_eq!(
            pockets[0],
            StrictSpawnPocketDefinition {
                pocket_index: 0,
                pocket_id: String::from("physical"),
                kind: SpawnPocketKindDefinition::Container,
                max_contains_volume_milliliters: 2_000,
                magazine_well_volume_milliliters: 0,
                max_contains_weight_milligrams: 3_000_000,
                max_item_volume_milliliters: 1_500,
                min_item_volume_milliliters: 5,
                max_item_length_millimeters: 300,
                item_restrictions: BTreeSet::from([String::from("test_phone")]),
                flag_restrictions: BTreeSet::from([String::from("ELECTRONIC")]),
                access_moves: 80,
                rigid: true,
                watertight: true,
                transparent: true,
                forbidden: false,
                sealable: true,
            }
        );
        assert_eq!(pockets[1].kind, SpawnPocketKindDefinition::EFileStorage);
        assert_eq!(
            pockets[1].max_contains_volume_milliliters,
            DEFAULT_SPAWN_POCKET_VOLUME_MILLILITERS
        );
        assert_eq!(
            pockets[1].max_contains_weight_milligrams,
            DEFAULT_SPAWN_POCKET_WEIGHT_MILLIGRAMS
        );
        assert_eq!(pockets[1].max_item_length_millimeters, 8_273);
        assert_eq!(
            default_spawn_pocket_max_item_length_millimeters(250),
            Some(84)
        );

        let unsupported = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_bad_efile",
            "name": "test bad efile",
            "pocket_data": [{
                "pocket_type": "E_FILE_STORAGE",
                "ememory_max": "1 GB",
                "weight_multiplier": 0.5
            }]
        }));
        assert!(
            load_one(&unsupported, &mut items, &mut abstracts)
                .expect("unsupported pocket should remain inventoried")
        );
        assert!(items["test_bad_efile"].spawn_pockets.is_empty());
        assert!(
            items["test_bad_efile"]
                .unsupported_fields
                .contains("pocket_data")
        );

        for (id, pocket) in [
            (
                "test_missing_efile_multiplier",
                serde_json::json!({
                    "pocket_type": "E_FILE_STORAGE",
                    "ememory_max": "1 GB"
                }),
            ),
            (
                "test_bad_container_multiplier",
                serde_json::json!({
                    "pocket_type": "CONTAINER",
                    "max_contains_volume": "1 L",
                    "max_contains_weight": "1 kg",
                    "weight_multiplier": 0.5
                }),
            ),
            (
                "test_nonrigid_efile",
                serde_json::json!({
                    "pocket_type": "E_FILE_STORAGE",
                    "ememory_max": "1 GB",
                    "weight_multiplier": 0,
                    "rigid": false
                }),
            ),
        ] {
            let unsupported = raw(serde_json::json!({
                "type": "ITEM",
                "id": id,
                "name": id,
                "pocket_data": [pocket]
            }));
            assert!(
                load_one(&unsupported, &mut items, &mut abstracts)
                    .expect("unsupported pocket should remain inventoried")
            );
            assert!(items[id].spawn_pockets.is_empty());
            assert!(items[id].unsupported_fields.contains("pocket_data"));
        }
    }

    #[test]
    fn flexible_spawn_pockets_preserve_reserved_base_volume_fail_closed() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let wrapper = raw(serde_json::json!({
            "type": "ITEM",
            "id": "test_flexible_wrapper",
            "name": "test flexible wrapper",
            "pocket_data": [{
                "pocket_type": "CONTAINER",
                "magazine_well": "45 ml",
                "max_contains_volume": "2500 ml",
                "max_contains_weight": "6 kg"
            }]
        }));
        assert!(load_one(&wrapper, &mut items, &mut abstracts).expect("wrapper should load"));
        let [pocket] = items["test_flexible_wrapper"].spawn_pockets.as_slice() else {
            panic!("flexible wrapper should retain one strict spawn pocket")
        };
        assert!(!pocket.rigid);
        assert_eq!(pocket.magazine_well_volume_milliliters, 45);
        assert_eq!(pocket.max_contains_volume_milliliters, 2_500);

        for (id, magazine_well, rigid) in [
            ("test_reserved_equals_capacity", "2500 ml", false),
            ("test_reserved_rigid", "45 ml", true),
        ] {
            let unsupported = raw(serde_json::json!({
                "type": "ITEM",
                "id": id,
                "name": id,
                "pocket_data": [{
                    "pocket_type": "CONTAINER",
                    "magazine_well": magazine_well,
                    "max_contains_volume": "2500 ml",
                    "max_contains_weight": "6 kg",
                    "rigid": rigid
                }]
            }));
            assert!(
                load_one(&unsupported, &mut items, &mut abstracts)
                    .expect("unsupported reserved-volume shape should remain inventoried")
            );
            assert!(items[id].spawn_pockets.is_empty());
            assert!(items[id].unsupported_fields.contains("pocket_data"));
        }
    }

    #[test]
    fn pinned_ammunition_container_fixtures_admit_and_exclude_exact_shapes() {
        fn pinned_item(relative_path: &str, item_id: &str) -> Value {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(relative_path);
            let bytes = fs::read(&path).expect("pinned item file should be readable");
            serde_json::from_slice::<Value>(&bytes)
                .expect("pinned item file should be valid JSON")
                .as_array()
                .expect("pinned item file should contain an array")
                .iter()
                .find(|value| value.get("id").and_then(Value::as_str) == Some(item_id))
                .unwrap_or_else(|| panic!("pinned item {item_id} should exist in {path:?}"))
                .clone()
        }

        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        for item_id in ["quiver", "nylon_quiver", "quiver_simple_cloth"] {
            let item = raw(pinned_item(
                "vendor/cdda/data/json/items/armor/ammo_pouch.json",
                item_id,
            ));
            assert!(
                load_one(&item, &mut items, &mut abstracts)
                    .unwrap_or_else(|error| panic!("pinned item {item_id} should load: {error}"))
            );
        }
        {
            let item_id = "quiver_takedown_bow";
            let item = raw(pinned_item(
                "vendor/cdda/data/json/items/armor/ammo_pouch.json",
                item_id,
            ));
            assert!(
                load_one(&item, &mut items, &mut abstracts)
                    .unwrap_or_else(|error| panic!("pinned item {item_id} should load: {error}"))
            );
        }
        let stone_pouch = raw(pinned_item(
            "vendor/cdda/data/json/items/armor/bandolier.json",
            "stone_pouch",
        ));
        assert!(
            load_one(&stone_pouch, &mut items, &mut abstracts)
                .expect("pinned stone pouch should load")
        );

        assert_eq!(
            items["quiver"].ammunition_containers,
            [StrictAmmunitionContainerDefinition {
                pocket_index: 0,
                pocket_id: String::new(),
                capacities: BTreeMap::from([
                    (String::from("arrow"), 20),
                    (String::from("bolt"), 20),
                ]),
                access_moves: 20,
                rigid: false,
            }]
        );
        assert_eq!(
            items["nylon_quiver"].ammunition_containers,
            items["quiver"].ammunition_containers
        );
        assert_eq!(
            items["quiver_simple_cloth"].ammunition_containers,
            [StrictAmmunitionContainerDefinition {
                pocket_index: 0,
                pocket_id: String::new(),
                capacities: BTreeMap::from([
                    (String::from("arrow"), 30),
                    (String::from("atlatl"), 5),
                    (String::from("bolt"), 30),
                    (String::from("fishspear"), 5),
                ]),
                access_moves: 30,
                rigid: false,
            }]
        );
        assert!(items["stone_pouch"].ammunition_containers.is_empty());
        assert!(
            items["quiver_takedown_bow"]
                .ammunition_containers
                .is_empty()
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
    fn item_variants_preserve_order_inheritance_modifiers_and_unsupported_semantics() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "abstract": "variant_base",
            "name": "variant base",
            "description": "base description",
            "expand_snippets": true,
            "ascii_picture": "base_art",
            "variant_type": "generic",
            "variants": [
                { "id": "blue", "name": { "str": "blue base" }, "weight": 2 },
                { "id": "deleted", "weight": 0 }
            ]
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base should load"));
        let concrete = raw(serde_json::json!({
            "type": "ITEM",
            "id": "variant_item",
            "copy-from": "variant_base",
            "name": "variant item",
            "extend": { "variants": [
                { "id": "green", "description": "green description", "append": true },
                { "id": "snippet", "expand_snippets": true }
            ] },
            "delete": { "variants": [ { "id": "deleted" } ] }
        }));
        assert!(load_one(&concrete, &mut items, &mut abstracts).expect("derived item should load"));
        let variants = &items["variant_item"].variants;
        assert_eq!(
            variants
                .iter()
                .map(|variant| variant.id.as_str())
                .collect::<Vec<_>>(),
            ["blue", "green", "snippet"]
        );
        assert_eq!(variants[0].name.as_deref(), Some("blue base"));
        assert!(items["variant_item"].expand_description_snippets);
        assert_eq!(items["variant_item"].ascii_picture, "base_art");
        assert_eq!(variants[0].weight, 2);
        assert_eq!(
            variants[1].description.as_deref(),
            Some("green description")
        );
        assert!(variants[1].append);
        assert!(variants[2].unsupported_fields.is_empty());
        assert!(variants[2].expand_description_snippets);
        assert!(
            !items["variant_item"]
                .unsupported_fields
                .contains("variants")
        );
    }

    #[test]
    fn inline_snippets_and_typed_variables_finalize_without_hiding_raw_markers() {
        let mut items = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        let base = raw(serde_json::json!({
            "type": "ITEM",
            "abstract": "constructor_state_base",
            "name": "constructor state base",
            "snippet_category": [
                { "id": "hello", "text": "Hello" },
                { "id": "map_note", "text": "North\nthen east", "//": "ordered" }
            ],
            "variables": { "browsed": "false", "attempts": 2 }
        }));
        assert!(load_one(&base, &mut items, &mut abstracts).expect("base should load"));
        let inherited = raw(serde_json::json!({
            "type": "ITEM",
            "id": "constructor_state_item",
            "copy-from": "constructor_state_base",
            "name": "constructor state item"
        }));
        assert!(
            load_one(&inherited, &mut items, &mut abstracts).expect("derived item should load")
        );
        let item = &items["constructor_state_item"];
        assert_eq!(
            item.snippets
                .iter()
                .map(|snippet| (snippet.id.as_str(), snippet.text.as_str()))
                .collect::<Vec<_>>(),
            [("hello", "Hello"), ("map_note", "North\nthen east")]
        );
        assert_eq!(
            item.variables,
            BTreeMap::from([
                (
                    String::from("attempts"),
                    ItemVariableValueDefinition::Integer(2)
                ),
                (
                    String::from("browsed"),
                    ItemVariableValueDefinition::String(String::from("false"))
                ),
            ])
        );
        assert!(item.unsupported_fields.contains("snippet_category"));
        assert!(item.unsupported_fields.contains("variables"));

        let named = raw(serde_json::json!({
            "type": "ITEM",
            "id": "named_snippet_item",
            "copy-from": "constructor_state_base",
            "name": "named snippet item",
            "snippet_category": "external_category",
            "variables": { "browsed": "true" }
        }));
        assert!(load_one(&named, &mut items, &mut abstracts).expect("named marker should load"));
        assert!(items["named_snippet_item"].snippets.is_empty());
        assert_eq!(
            items["named_snippet_item"].variables,
            BTreeMap::from([(
                String::from("browsed"),
                ItemVariableValueDefinition::String(String::from("true"))
            )])
        );

        let duplicate = raw(serde_json::json!({
            "type": "ITEM",
            "id": "duplicate_snippets",
            "name": "duplicate snippets",
            "snippet_category": [
                { "id": "same", "text": "first" },
                { "id": "same", "text": "second" }
            ]
        }));
        assert!(matches!(
            load_one(&duplicate, &mut items, &mut abstracts),
            Err(ItemRegistryError::InvalidField { field, .. }) if field == "snippet_category"
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
