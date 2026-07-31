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
    "name",
    "description",
    "default_faction",
    "symbol",
    "color",
    "volume",
    "weight",
    "hp",
    "speed",
    "aggression",
    "morale",
    "attack_cost",
    "melee_skill",
    "melee_dice",
    "melee_dice_sides",
    "melee_dice_ap",
    "melee_damage",
    "attack_effs",
    "dodge",
    "vision_day",
    "vision_night",
    "material",
    "flags",
    "species",
    "path_settings",
    "armor",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MonsterPathSettings {
    pub max_distance: i32,
    pub allow_open_doors: bool,
    pub avoid_traps: bool,
    pub avoid_sharp: bool,
    pub avoid_dangerous_fields: bool,
    pub allow_climb_stairs: bool,
}

/// One finalized component of a monster's ordinary melee damage instance.
/// Amount and penetration use thousandths of one damage point; multipliers use
/// millionths so admitted authoritative combat never depends on host floats.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterMeleeDamageUnitDefinition {
    pub damage_type_id: String,
    pub amount_milli: i32,
    pub armor_penetration_milli: i32,
    pub armor_multiplier_millionths: i32,
    pub damage_multiplier_millionths: i32,
    pub constant_armor_multiplier_millionths: i32,
    pub constant_damage_multiplier_millionths: i32,
}

/// One effect applied after an ordinary monster melee hit deals damage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterAttackEffectDefinition {
    pub effect_id: String,
    pub chance_millionths: u32,
    pub permanent: bool,
    pub affect_hit_body_part: bool,
    pub body_part_id: Option<String>,
    pub duration_turns: (u32, u32),
    pub intensity: (u32, u32),
}

impl Default for MonsterPathSettings {
    fn default() -> Self {
        Self {
            max_distance: 0,
            allow_open_doors: false,
            avoid_traps: false,
            avoid_sharp: false,
            avoid_dangerous_fields: false,
            allow_climb_stairs: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_faction: String,
    pub symbol: String,
    pub color: String,
    /// Final inherited monster volume in the pinned engine's base milliliters.
    pub volume_milliliters: i64,
    /// Final inherited monster mass in integer milligrams. Ordinary corpse
    /// construction uses the monster rather than the item type for mass.
    pub weight_milligrams: i64,
    pub hp: i32,
    pub speed: i32,
    pub aggression: i32,
    pub morale: i32,
    /// Final inherited moves spent by an ordinary melee attack.
    pub attack_cost_moves: i32,
    pub melee_skill: i32,
    pub melee_dice: i32,
    pub melee_dice_sides: i32,
    pub melee_dice_armor_penetration_milli: i32,
    /// Final inherited ordinary melee damage in pinned damage-instance order.
    pub melee_damage: Vec<MonsterMeleeDamageUnitDefinition>,
    /// Final inherited post-damage ordinary melee effects in source order.
    pub attack_effects: Vec<MonsterAttackEffectDefinition>,
    pub dodge: i32,
    pub vision_day: i32,
    pub vision_night: i32,
    pub materials: BTreeSet<String>,
    pub flags: BTreeSet<String>,
    pub species: BTreeSet<String>,
    pub path_settings: MonsterPathSettings,
    /// Final inherited flat monster resistances in thousandths of one damage
    /// point. Arbitrary pinned damage-type IDs are retained; runtime applies
    /// every type it can actually deal and leaves the rest canonical.
    pub armor_milli: BTreeMap<String, i32>,
    pub unsupported_fields: BTreeSet<String>,
    pub source: String,
}

impl Default for MonsterDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            default_faction: String::new(),
            symbol: String::new(),
            color: String::from("white"),
            volume_milliliters: 62_499,
            weight_milligrams: 81_499_000,
            hp: 1,
            speed: 0,
            aggression: 0,
            morale: 0,
            attack_cost_moves: 100,
            melee_skill: 0,
            melee_dice: 0,
            melee_dice_sides: 0,
            melee_dice_armor_penetration_milli: 0,
            melee_damage: Vec::new(),
            attack_effects: Vec::new(),
            dodge: 0,
            vision_day: 40,
            vision_night: 1,
            materials: BTreeSet::new(),
            flags: BTreeSet::new(),
            species: BTreeSet::new(),
            path_settings: MonsterPathSettings::default(),
            armor_milli: BTreeMap::new(),
            unsupported_fields: BTreeSet::new(),
            source: String::new(),
        }
    }
}

impl MonsterDefinition {
    #[must_use]
    pub fn finalized_armor_milli(&self) -> BTreeMap<String, i32> {
        finalized_armor(&self.armor_milli)
    }

    #[must_use]
    pub fn melee_damage_is_fully_supported(&self) -> bool {
        !self
            .unsupported_fields
            .iter()
            .any(|field| field == "melee_damage" || field.starts_with("melee_damage."))
    }

    #[must_use]
    pub fn attack_effects_are_fully_supported(&self) -> bool {
        !self
            .unsupported_fields
            .iter()
            .any(|field| field == "attack_effs" || field.starts_with("attack_effs."))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MonsterRegistry {
    monsters: BTreeMap<String, MonsterDefinition>,
    abstract_count: usize,
}

#[derive(Clone)]
struct RawMonster {
    file: SelectedContentFile,
    object: Map<String, Value>,
}

impl MonsterRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, MonsterRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(MonsterRegistryError::Catalog)?;
        let mut pending = read_monsters(content_root.as_ref(), files)?;
        let mut monsters = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(MonsterRegistryError::InternalQueue)?;
                if load_one(&raw, &mut monsters, &mut abstracts)? {
                    loaded += 1;
                } else {
                    pending.push_back(raw);
                }
            }
            if loaded == 0 {
                return Err(MonsterRegistryError::UnresolvedInheritance(
                    pending
                        .iter()
                        .take(20)
                        .filter_map(|raw| definition_key(&raw.object).ok())
                        .map(|(id, _)| id.to_owned())
                        .collect(),
                ));
            }
        }
        Ok(Self {
            monsters,
            abstract_count: abstracts.len(),
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.monsters.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.monsters.is_empty()
    }

    #[must_use]
    pub fn abstract_count(&self) -> usize {
        self.abstract_count
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MonsterDefinition> {
        self.monsters.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &MonsterDefinition)> {
        self.monsters
            .iter()
            .map(|(id, monster)| (id.as_str(), monster))
    }
}

fn read_monsters(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<VecDeque<RawMonster>, MonsterRegistryError> {
    let mut monsters = VecDeque::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| MonsterRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| MonsterRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_monster(&file, value, &mut monsters)?;
                }
            }
            value => collect_monster(&file, value, &mut monsters)?,
        }
    }
    Ok(monsters)
}

fn collect_monster(
    file: &SelectedContentFile,
    value: Value,
    monsters: &mut VecDeque<RawMonster>,
) -> Result<(), MonsterRegistryError> {
    if value.get("type").and_then(Value::as_str) != Some("MONSTER") {
        return Ok(());
    }
    monsters.push_back(RawMonster {
        file: file.clone(),
        object: value
            .as_object()
            .cloned()
            .ok_or_else(|| MonsterRegistryError::InvalidDefinition(file.upstream_path.clone()))?,
    });
    Ok(())
}

fn load_one(
    raw: &RawMonster,
    monsters: &mut BTreeMap<String, MonsterDefinition>,
    abstracts: &mut BTreeMap<String, MonsterDefinition>,
) -> Result<bool, MonsterRegistryError> {
    let (id, is_abstract) = definition_key(&raw.object)?;
    let parent = optional_string(&raw.object, "copy-from", &raw.file.upstream_path)?;
    let mut monster = if let Some(parent) = parent {
        let Some(base) = monsters.get(parent).or_else(|| abstracts.get(parent)) else {
            return Ok(false);
        };
        base.clone()
    } else {
        MonsterDefinition::default()
    };
    monster.id = id.to_owned();
    monster.source.clone_from(&raw.file.upstream_path);
    let context = format!("{}#{id}", raw.file.upstream_path);
    apply_fields(&mut monster, &raw.object, &context)?;
    if !is_abstract
        && (monster.name.is_empty()
            || monster.symbol.is_empty()
            || monster.hp < 1
            || monster.speed < 0)
    {
        return Err(MonsterRegistryError::InvalidFinalizedMonster {
            id: id.to_owned(),
            source: raw.file.upstream_path.clone(),
        });
    }
    if is_abstract {
        abstracts.insert(id.to_owned(), monster);
    } else {
        monsters.insert(id.to_owned(), monster);
    }
    Ok(true)
}

fn definition_key(object: &Map<String, Value>) -> Result<(&str, bool), MonsterRegistryError> {
    match (object.get("id"), object.get("abstract")) {
        (Some(Value::String(id)), None) if !id.is_empty() => Ok((id, false)),
        (None, Some(Value::String(id))) if !id.is_empty() => Ok((id, true)),
        _ => Err(MonsterRegistryError::InvalidIdentity),
    }
}

fn apply_fields(
    monster: &mut MonsterDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    apply_text(object, "name", &mut monster.name, source)?;
    apply_text(object, "description", &mut monster.description, source)?;
    apply_string(
        object,
        "default_faction",
        &mut monster.default_faction,
        source,
    )?;
    apply_string(object, "symbol", &mut monster.symbol, source)?;
    apply_string(object, "color", &mut monster.color, source)?;
    apply_volume(object, &mut monster.volume_milliliters, source)?;
    apply_mass(object, &mut monster.weight_milligrams, source)?;
    apply_integer(object, "hp", &mut monster.hp, 1, i32::MAX, source)?;
    apply_integer(object, "speed", &mut monster.speed, 0, i32::MAX, source)?;
    apply_integer(
        object,
        "aggression",
        &mut monster.aggression,
        -100,
        100,
        source,
    )?;
    apply_integer(
        object,
        "morale",
        &mut monster.morale,
        i32::MIN,
        i32::MAX,
        source,
    )?;
    apply_integer(
        object,
        "attack_cost",
        &mut monster.attack_cost_moves,
        0,
        i32::MAX,
        source,
    )?;
    for (field, target) in [
        ("melee_skill", &mut monster.melee_skill),
        ("melee_dice", &mut monster.melee_dice),
        ("melee_dice_sides", &mut monster.melee_dice_sides),
        ("dodge", &mut monster.dodge),
        ("vision_day", &mut monster.vision_day),
        ("vision_night", &mut monster.vision_night),
    ] {
        apply_integer(object, field, target, 0, i32::MAX, source)?;
    }
    apply_scaled_number(
        object,
        "melee_dice_ap",
        &mut monster.melee_dice_armor_penetration_milli,
        1_000,
        source,
    )?;
    apply_melee_damage(monster, object, source)?;
    apply_attack_effects(monster, object, source)?;
    apply_string_set(object, "material", &mut monster.materials, source)?;
    apply_string_set(object, "flags", &mut monster.flags, source)?;
    apply_string_set(object, "species", &mut monster.species, source)?;
    apply_path_settings(object, &mut monster.path_settings, source)?;
    apply_armor(object, &mut monster.armor_milli, source)?;
    if modifier(object, "delete", "armor", source)?.is_some() {
        monster
            .unsupported_fields
            .insert(String::from("delete.armor"));
    }
    for field in object.keys() {
        if !field.starts_with("//")
            && !IMPLEMENTED_FIELDS.contains(&field.as_str())
            && !matches!(
                field.as_str(),
                "extend" | "delete" | "relative" | "proportional"
            )
        {
            monster.unsupported_fields.insert(field.clone());
        }
    }
    for modifier_name in ["extend", "delete", "relative", "proportional"] {
        if let Some(Value::Object(fields)) = object.get(modifier_name) {
            for field in fields.keys() {
                if !IMPLEMENTED_FIELDS.contains(&field.as_str()) {
                    monster.unsupported_fields.insert(field.clone());
                }
            }
        }
    }
    Ok(())
}

fn apply_attack_effects(
    monster: &mut MonsterDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get("attack_effs") {
        monster
            .unsupported_fields
            .retain(|field| field != "attack_effs" && !field.starts_with("attack_effs."));
        let values = value
            .as_array()
            .ok_or_else(|| invalid(source, "attack_effs"))?;
        let mut effects = Vec::with_capacity(values.len());
        for value in values {
            let fields = value
                .as_object()
                .ok_or_else(|| invalid(source, "attack_effs"))?;
            const IMPLEMENTED_EFFECT_FIELDS: &[&str] = &[
                "id",
                "chance",
                "permanent",
                "affect_hit_bp",
                "bp",
                "duration",
                "intensity",
            ];
            for field in fields.keys().filter(|field| !field.starts_with("//")) {
                if !IMPLEMENTED_EFFECT_FIELDS.contains(&field.as_str()) {
                    monster
                        .unsupported_fields
                        .insert(format!("attack_effs.{field}"));
                }
            }
            let effect_id = fields
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
                .ok_or_else(|| invalid(source, "attack_effs.id"))?
                .to_owned();
            let chance = fields.get("chance").map_or(Ok(100.0), |value| {
                value
                    .as_f64()
                    .filter(|chance| chance.is_finite() && (0.0..=100.0).contains(chance))
                    .ok_or_else(|| invalid(source, "attack_effs.chance"))
            })?;
            let chance_millionths = (chance * 10_000.0).round() as u32;
            let permanent = fields.get("permanent").map_or(Ok(false), |value| {
                value
                    .as_bool()
                    .ok_or_else(|| invalid(source, "attack_effs.permanent"))
            })?;
            let affect_hit_body_part = fields.get("affect_hit_bp").map_or(Ok(false), |value| {
                value
                    .as_bool()
                    .ok_or_else(|| invalid(source, "attack_effs.affect_hit_bp"))
            })?;
            let body_part_id = fields
                .get("bp")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|id| {
                            !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
                        })
                        .map(str::to_owned)
                        .ok_or_else(|| invalid(source, "attack_effs.bp"))
                })
                .transpose()?;
            effects.push(MonsterAttackEffectDefinition {
                effect_id,
                chance_millionths,
                permanent,
                affect_hit_body_part,
                body_part_id,
                duration_turns: parse_attack_effect_range(
                    fields.get("duration"),
                    1,
                    source,
                    "attack_effs.duration",
                )?,
                intensity: parse_attack_effect_range(
                    fields.get("intensity"),
                    1,
                    source,
                    "attack_effs.intensity",
                )?,
            });
        }
        monster.attack_effects = effects;
    }
    for modifier_name in ["extend", "delete", "relative", "proportional"] {
        if modifier(object, modifier_name, "attack_effs", source)?.is_some() {
            monster
                .unsupported_fields
                .insert(format!("attack_effs.{modifier_name}"));
        }
    }
    Ok(())
}

fn parse_attack_effect_range(
    value: Option<&Value>,
    default: u32,
    source: &str,
    field: &str,
) -> Result<(u32, u32), MonsterRegistryError> {
    let parse = |value: &Value| {
        u32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))
    };
    let range = match value {
        None => (default, default),
        Some(Value::Array(values)) if values.len() == 2 => (parse(&values[0])?, parse(&values[1])?),
        Some(value) => {
            let value = parse(value)?;
            (value, value)
        }
    };
    let maximum = if field.ends_with("intensity") {
        1_000_000
    } else {
        1_000_000_000
    };
    if range.0 > range.1 || range.1 > maximum || (field.ends_with("intensity") && range.0 == 0) {
        return Err(invalid(source, field));
    }
    Ok(range)
}

fn apply_scaled_number(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    scale: i32,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get(field) {
        *target = parse_scaled_number(value, scale, source, field)?;
    } else if let Some(value) = modifier(object, "proportional", field, source)? {
        let factor = parse_proportional_factor(Some(value), source, field)?;
        if factor == 0 {
            return Err(invalid(source, field));
        }
        *target = multiply_scaled(*target, factor, source, field)?;
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        *target = target
            .checked_add(parse_scaled_number(value, scale, source, field)?)
            .ok_or_else(|| invalid(source, field))?;
    }
    for modifier_name in ["extend", "delete"] {
        if modifier(object, modifier_name, field, source)?.is_some() {
            return Err(invalid(source, &format!("{modifier_name}.{field}")));
        }
    }
    Ok(())
}

fn apply_melee_damage(
    monster: &mut MonsterDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get("melee_damage") {
        monster
            .unsupported_fields
            .retain(|field| field != "melee_damage" && !field.starts_with("melee_damage."));
        let (damage, deferred) = parse_melee_damage(value, source, "melee_damage")?;
        monster.melee_damage = damage;
        monster.unsupported_fields.extend(deferred);
    } else if let Some(value) = modifier(object, "proportional", "melee_damage", source)? {
        let (damage_type, factors, deferred) = parse_proportional_melee_damage(value, source)?;
        let unit = monster
            .melee_damage
            .iter_mut()
            .find(|unit| unit.damage_type_id == damage_type)
            .ok_or_else(|| invalid(source, "proportional.melee_damage.damage_type"))?;
        unit.amount_milli = multiply_scaled(
            unit.amount_milli,
            factors.amount_millionths,
            source,
            "proportional.melee_damage.amount",
        )?;
        unit.armor_penetration_milli = multiply_scaled(
            unit.armor_penetration_milli,
            factors.armor_penetration_millionths,
            source,
            "proportional.melee_damage.armor_penetration",
        )?;
        unit.armor_multiplier_millionths = multiply_scaled(
            unit.armor_multiplier_millionths,
            factors.armor_multiplier_millionths,
            source,
            "proportional.melee_damage.armor_multiplier",
        )?;
        unit.damage_multiplier_millionths = multiply_scaled(
            unit.damage_multiplier_millionths,
            factors.damage_multiplier_millionths,
            source,
            "proportional.melee_damage.damage_multiplier",
        )?;
        unit.constant_armor_multiplier_millionths = multiply_scaled(
            unit.constant_armor_multiplier_millionths,
            factors.constant_armor_multiplier_millionths,
            source,
            "proportional.melee_damage.constant_armor_multiplier",
        )?;
        unit.constant_damage_multiplier_millionths = multiply_scaled(
            unit.constant_damage_multiplier_millionths,
            factors.constant_damage_multiplier_millionths,
            source,
            "proportional.melee_damage.constant_damage_multiplier",
        )?;
        monster.unsupported_fields.extend(deferred);
    } else if let Some(value) = modifier(object, "relative", "melee_damage", source)? {
        let (relative, deferred) = parse_melee_damage(value, source, "relative.melee_damage")?;
        for addition in relative {
            let Some(unit) = monster
                .melee_damage
                .iter_mut()
                .find(|unit| unit.damage_type_id == addition.damage_type_id)
            else {
                continue;
            };
            unit.amount_milli = unit
                .amount_milli
                .checked_add(addition.amount_milli)
                .ok_or_else(|| invalid(source, "relative.melee_damage.amount"))?;
            unit.armor_penetration_milli = unit
                .armor_penetration_milli
                .checked_add(addition.armor_penetration_milli)
                .ok_or_else(|| invalid(source, "relative.melee_damage.armor_penetration"))?;
            add_relative_multiplier(
                &mut unit.armor_multiplier_millionths,
                addition.armor_multiplier_millionths,
                source,
                "relative.melee_damage.armor_multiplier",
            )?;
            add_relative_multiplier(
                &mut unit.damage_multiplier_millionths,
                addition.damage_multiplier_millionths,
                source,
                "relative.melee_damage.damage_multiplier",
            )?;
            // Pinned damage_unit::operator+= intentionally does not add the
            // relative unconditional armor multiplier.
            add_relative_multiplier(
                &mut unit.constant_damage_multiplier_millionths,
                addition.constant_damage_multiplier_millionths,
                source,
                "relative.melee_damage.constant_damage_multiplier",
            )?;
        }
        monster.unsupported_fields.extend(deferred);
    }
    for modifier_name in ["extend", "delete"] {
        if modifier(object, modifier_name, "melee_damage", source)?.is_some() {
            monster
                .unsupported_fields
                .insert(format!("melee_damage.{modifier_name}"));
        }
    }
    Ok(())
}

fn add_relative_multiplier(
    target: &mut i32,
    addition: i32,
    source: &str,
    field: &str,
) -> Result<(), MonsterRegistryError> {
    // The pinned relative reader maps its default 1.0 multiplier back to zero
    // before addition; an explicitly supplied 1.0 has the same behavior.
    if addition != 1_000_000 {
        *target = target
            .checked_add(addition)
            .ok_or_else(|| invalid(source, field))?;
    }
    Ok(())
}

fn parse_melee_damage(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<(Vec<MonsterMeleeDamageUnitDefinition>, BTreeSet<String>), MonsterRegistryError> {
    let values = match value {
        Value::Object(_) => vec![value],
        Value::Array(values) => values.iter().collect(),
        _ => return Err(invalid(source, field)),
    };
    let mut units = Vec::with_capacity(values.len());
    let mut deferred = BTreeSet::new();
    for value in values {
        let (unit, unit_deferred) = parse_melee_damage_unit(value, source, field)?;
        if units
            .iter()
            .any(|existing: &MonsterMeleeDamageUnitDefinition| {
                existing.damage_type_id == unit.damage_type_id
            })
        {
            deferred.insert(String::from("melee_damage.duplicate_damage_type"));
        }
        units.push(unit);
        deferred.extend(unit_deferred);
    }
    Ok((units, deferred))
}

fn parse_melee_damage_unit(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<(MonsterMeleeDamageUnitDefinition, BTreeSet<String>), MonsterRegistryError> {
    const FIELDS: &[&str] = &[
        "damage_type",
        "amount",
        "armor_penetration",
        "armor_multiplier",
        "damage_multiplier",
        "constant_armor_multiplier",
        "constant_damage_multiplier",
        "barrels",
    ];
    let object = value.as_object().ok_or_else(|| invalid(source, field))?;
    let damage_type_id = object
        .get("damage_type")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .ok_or_else(|| invalid(source, &format!("{field}.damage_type")))?
        .to_owned();
    let mut deferred = BTreeSet::new();
    for key in object.keys().filter(|key| !key.starts_with("//")) {
        if !FIELDS.contains(&key.as_str()) || key == "barrels" {
            deferred.insert(format!("melee_damage.{key}"));
        }
    }
    Ok((
        MonsterMeleeDamageUnitDefinition {
            damage_type_id,
            amount_milli: parse_optional_scaled_number(
                object.get("amount"),
                0,
                1_000,
                source,
                &format!("{field}.amount"),
            )?,
            armor_penetration_milli: parse_optional_scaled_number(
                object.get("armor_penetration"),
                0,
                1_000,
                source,
                &format!("{field}.armor_penetration"),
            )?,
            armor_multiplier_millionths: parse_optional_scaled_number(
                object.get("armor_multiplier"),
                1_000_000,
                1_000_000,
                source,
                &format!("{field}.armor_multiplier"),
            )?,
            damage_multiplier_millionths: parse_optional_scaled_number(
                object.get("damage_multiplier"),
                1_000_000,
                1_000_000,
                source,
                &format!("{field}.damage_multiplier"),
            )?,
            constant_armor_multiplier_millionths: parse_optional_scaled_number(
                object.get("constant_armor_multiplier"),
                1_000_000,
                1_000_000,
                source,
                &format!("{field}.constant_armor_multiplier"),
            )?,
            constant_damage_multiplier_millionths: parse_optional_scaled_number(
                object.get("constant_damage_multiplier"),
                1_000_000,
                1_000_000,
                source,
                &format!("{field}.constant_damage_multiplier"),
            )?,
        },
        deferred,
    ))
}

struct ProportionalMeleeDamage {
    amount_millionths: i32,
    armor_penetration_millionths: i32,
    armor_multiplier_millionths: i32,
    damage_multiplier_millionths: i32,
    constant_armor_multiplier_millionths: i32,
    constant_damage_multiplier_millionths: i32,
}

fn parse_proportional_melee_damage(
    value: &Value,
    source: &str,
) -> Result<(String, ProportionalMeleeDamage, BTreeSet<String>), MonsterRegistryError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid(source, "proportional.melee_damage"))?;
    let damage_type = object
        .get("damage_type")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .ok_or_else(|| invalid(source, "proportional.melee_damage.damage_type"))?
        .to_owned();
    let known = [
        "damage_type",
        "amount",
        "armor_penetration",
        "armor_multiplier",
        "damage_multiplier",
        "constant_armor_multiplier",
        "constant_damage_multiplier",
        "barrels",
    ];
    let deferred = object
        .keys()
        .filter(|key| {
            !key.starts_with("//") && (!known.contains(&key.as_str()) || key.as_str() == "barrels")
        })
        .map(|key| format!("melee_damage.{key}"))
        .collect();
    Ok((
        damage_type,
        ProportionalMeleeDamage {
            amount_millionths: parse_proportional_factor(
                object.get("amount"),
                source,
                "proportional.melee_damage.amount",
            )?,
            armor_penetration_millionths: parse_proportional_factor(
                object.get("armor_penetration"),
                source,
                "proportional.melee_damage.armor_penetration",
            )?,
            armor_multiplier_millionths: parse_proportional_factor(
                object.get("armor_multiplier"),
                source,
                "proportional.melee_damage.armor_multiplier",
            )?,
            damage_multiplier_millionths: parse_proportional_factor(
                object.get("damage_multiplier"),
                source,
                "proportional.melee_damage.damage_multiplier",
            )?,
            constant_armor_multiplier_millionths: parse_proportional_factor(
                object.get("constant_armor_multiplier"),
                source,
                "proportional.melee_damage.constant_armor_multiplier",
            )?,
            constant_damage_multiplier_millionths: parse_proportional_factor(
                object.get("constant_damage_multiplier"),
                source,
                "proportional.melee_damage.constant_damage_multiplier",
            )?,
        },
        deferred,
    ))
}

fn parse_proportional_factor(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<i32, MonsterRegistryError> {
    let Some(value) = value else {
        return Ok(1_000_000);
    };
    let factor = parse_scaled_number(value, 1_000_000, source, field)?;
    if factor < 0 || factor == 1_000_000 {
        return Err(invalid(source, field));
    }
    Ok(factor)
}

fn parse_optional_scaled_number(
    value: Option<&Value>,
    default: i32,
    scale: i32,
    source: &str,
    field: &str,
) -> Result<i32, MonsterRegistryError> {
    value
        .map(|value| parse_scaled_number(value, scale, source, field))
        .unwrap_or(Ok(default))
}

fn parse_scaled_number(
    value: &Value,
    scale: i32,
    source: &str,
    field: &str,
) -> Result<i32, MonsterRegistryError> {
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid(source, field))?;
    let scaled = value * f64::from(scale);
    if scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err(invalid(source, field));
    }
    Ok(scaled.round() as i32)
}

fn multiply_scaled(
    value: i32,
    factor_millionths: i32,
    source: &str,
    field: &str,
) -> Result<i32, MonsterRegistryError> {
    i64::from(value)
        .checked_mul(i64::from(factor_millionths))
        .and_then(|value| value.checked_div(1_000_000))
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid(source, field))
}

fn apply_armor(
    object: &Map<String, Value>,
    armor: &mut BTreeMap<String, i32>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get("armor") {
        *armor = parse_armor_map(value, source, "armor")?;
    }
    if let Some(value) = modifier(object, "extend", "armor", source)? {
        for (damage_type, addition) in parse_armor_map(value, source, "extend.armor")? {
            let resistance = armor.entry(damage_type).or_default();
            *resistance = resistance
                .checked_add(addition)
                .ok_or_else(|| invalid(source, "extend.armor"))?;
        }
    }
    if let Some(value) = modifier(object, "proportional", "armor", source)?
        && value.is_object()
    {
        let multipliers = parse_armor_map(value, source, "proportional.armor")?;
        let finalized = finalized_armor(armor);
        for (damage_type, multiplier_milli) in multipliers {
            let current = finalized.get(&damage_type).copied().unwrap_or_default();
            let adjusted = i64::from(current)
                .checked_mul(i64::from(multiplier_milli))
                .and_then(|value| value.checked_div(1_000))
                .and_then(|value| i32::try_from(value).ok())
                .ok_or_else(|| invalid(source, "proportional.armor"))?;
            armor.insert(damage_type, adjusted);
        }
    }
    if let Some(value) = modifier(object, "relative", "armor", source)? {
        let additions = parse_armor_map(value, source, "relative.armor")?;
        let finalized = finalized_armor(armor);
        for (damage_type, addition) in additions {
            let current = finalized.get(&damage_type).copied().unwrap_or_default();
            armor.insert(
                damage_type,
                current
                    .checked_add(addition)
                    .ok_or_else(|| invalid(source, "relative.armor"))?,
            );
        }
    }
    Ok(())
}

fn parse_armor_map(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeMap<String, i32>, MonsterRegistryError> {
    value
        .as_object()
        .ok_or_else(|| invalid(source, field))?
        .iter()
        .filter(|(damage_type, _)| !damage_type.starts_with("//"))
        .map(|(damage_type, value)| {
            if damage_type.is_empty()
                || damage_type.len() > 512
                || damage_type.chars().any(char::is_control)
            {
                return Err(invalid(source, field));
            }
            let resistance = value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| invalid(source, field))?;
            let milli = resistance * 1_000.0;
            if milli < f64::from(i32::MIN) || milli > f64::from(i32::MAX) {
                return Err(invalid(source, field));
            }
            Ok((damage_type.clone(), milli.round() as i32))
        })
        .collect()
}

fn finalized_armor(armor: &BTreeMap<String, i32>) -> BTreeMap<String, i32> {
    let all = armor.get("all").copied().unwrap_or_default();
    let physical = armor.get("physical").copied().unwrap_or(all);
    let non_physical = armor.get("non_physical").copied().unwrap_or(all);
    let mut finalized = armor
        .iter()
        .filter(|(damage_type, _)| {
            !matches!(damage_type.as_str(), "all" | "physical" | "non_physical")
        })
        .map(|(damage_type, resistance)| (damage_type.clone(), *resistance))
        .collect::<BTreeMap<_, _>>();
    for damage_type in ["bash", "cut", "bullet"] {
        finalized
            .entry(String::from(damage_type))
            .or_insert(physical);
    }
    for damage_type in ["electric", "heat", "cold", "pure", "biological"] {
        finalized
            .entry(String::from(damage_type))
            .or_insert(non_physical);
    }
    let cut = finalized.get("cut").copied().unwrap_or(physical);
    finalized
        .entry(String::from("stab"))
        .or_insert_with(|| i32::try_from(i64::from(cut) * 8 / 10).unwrap_or(i32::MAX));
    finalized
        .entry(String::from("acid"))
        .or_insert_with(|| i32::try_from(i64::from(cut) / 2).unwrap_or(i32::MAX));
    finalized
}

fn apply_path_settings(
    object: &Map<String, Value>,
    settings: &mut MonsterPathSettings,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    for modifier in ["extend", "delete", "relative", "proportional"] {
        if object
            .get(modifier)
            .and_then(Value::as_object)
            .is_some_and(|fields| fields.contains_key("path_settings"))
        {
            return Err(invalid(source, &format!("{modifier}.path_settings")));
        }
    }
    let Some(value) = object.get("path_settings") else {
        return Ok(());
    };
    let fields = value
        .as_object()
        .ok_or_else(|| invalid(source, "path_settings"))?;
    const OBSERVED_FIELDS: &[&str] = &[
        "max_dist",
        "allow_open_doors",
        "avoid_traps",
        "avoid_sharp",
        "avoid_dangerous_fields",
        "allow_climb_stairs",
    ];
    for field in fields.keys().filter(|field| !field.starts_with("//")) {
        if !OBSERVED_FIELDS.contains(&field.as_str()) {
            return Err(invalid(source, &format!("path_settings.{field}")));
        }
    }
    if let Some(value) = fields.get("max_dist") {
        settings.max_distance = i32::try_from(
            value
                .as_i64()
                .ok_or_else(|| invalid(source, "path_settings.max_dist"))?,
        )
        .map_err(|_| invalid(source, "path_settings.max_dist"))?;
        if !(0..=4_096).contains(&settings.max_distance) {
            return Err(invalid(source, "path_settings.max_dist"));
        }
    }
    for (field, target) in [
        ("allow_open_doors", &mut settings.allow_open_doors),
        ("avoid_traps", &mut settings.avoid_traps),
        ("avoid_sharp", &mut settings.avoid_sharp),
        (
            "avoid_dangerous_fields",
            &mut settings.avoid_dangerous_fields,
        ),
        ("allow_climb_stairs", &mut settings.allow_climb_stairs),
    ] {
        if let Some(value) = fields.get(field) {
            *target = value
                .as_bool()
                .ok_or_else(|| invalid(source, &format!("path_settings.{field}")))?;
        }
    }
    Ok(())
}

fn apply_integer(
    object: &Map<String, Value>,
    field: &str,
    target: &mut i32,
    minimum: i32,
    maximum: i32,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get(field) {
        *target = i32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))?;
    } else if let Some(value) = modifier(object, "proportional", field, source)? {
        let multiplier = value
            .as_f64()
            .filter(|multiplier| *multiplier > 0.0 && *multiplier != 1.0)
            .ok_or_else(|| invalid(source, field))?;
        let adjusted = f64::from(*target) * multiplier;
        if !adjusted.is_finite() || adjusted < f64::from(i32::MIN) || adjusted > f64::from(i32::MAX)
        {
            return Err(invalid(source, field));
        }
        // Pinned primitive `int *= double` converts the product back to `int`,
        // truncating toward zero rather than rounding to the nearest integer.
        *target = adjusted as i32;
    } else if let Some(value) = modifier(object, "relative", field, source)? {
        let addition = i32::try_from(value.as_i64().ok_or_else(|| invalid(source, field))?)
            .map_err(|_| invalid(source, field))?;
        *target = target
            .checked_add(addition)
            .ok_or_else(|| invalid(source, field))?;
    }
    if *target < minimum || *target > maximum {
        return Err(invalid(source, field));
    }
    Ok(())
}

fn apply_volume(
    object: &Map<String, Value>,
    target: &mut i64,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get("volume") {
        *target = parse_volume(value, source)?;
    } else if let Some(value) = modifier(object, "proportional", "volume", source)? {
        let multiplier = value
            .as_f64()
            .filter(|value| *value > 0.0 && *value != 1.0)
            .ok_or_else(|| invalid(source, "volume"))?;
        let adjusted = (*target as f64) * multiplier;
        // `i64::MAX as f64` rounds up to 2^63, so equality is already out of
        // range for the integer base unit.
        if !adjusted.is_finite() || adjusted < 0.0 || adjusted >= i64::MAX as f64 {
            return Err(invalid(source, "volume"));
        }
        // Pinned `units::quantity<int64_t>::operator*=` converts the floating
        // result back to the integer base unit, truncating toward zero.
        *target = adjusted as i64;
    } else if let Some(value) = modifier(object, "relative", "volume", source)? {
        *target = target
            .checked_add(parse_volume(value, source)?)
            .ok_or_else(|| invalid(source, "volume"))?;
    }
    if *target < 0 {
        return Err(invalid(source, "volume"));
    }
    Ok(())
}

fn apply_mass(
    object: &Map<String, Value>,
    target: &mut i64,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    for modifier_name in ["extend", "delete"] {
        if modifier(object, modifier_name, "weight", source)?.is_some() {
            return Err(invalid(source, &format!("{modifier_name}.weight")));
        }
    }
    if let Some(value) = object.get("weight") {
        *target = parse_mass(value, source)?;
    } else if let Some(value) = modifier(object, "proportional", "weight", source)? {
        let multiplier = value
            .as_f64()
            .filter(|value| *value > 0.0 && *value != 1.0)
            .ok_or_else(|| invalid(source, "weight"))?;
        let adjusted = (*target as f64) * multiplier;
        if !adjusted.is_finite() || adjusted < 0.0 || adjusted >= i64::MAX as f64 {
            return Err(invalid(source, "weight"));
        }
        *target = adjusted as i64;
    } else if let Some(value) = modifier(object, "relative", "weight", source)? {
        *target = target
            .checked_add(parse_mass(value, source)?)
            .ok_or_else(|| invalid(source, "weight"))?;
    }
    if *target < 0 {
        return Err(invalid(source, "weight"));
    }
    Ok(())
}

fn parse_mass(value: &Value, source: &str) -> Result<i64, MonsterRegistryError> {
    parse_monster_quantity(
        value,
        source,
        "weight",
        &[("mg", 1), ("g", 1_000), ("kg", 1_000_000)],
    )
}

fn parse_volume(value: &Value, source: &str) -> Result<i64, MonsterRegistryError> {
    parse_monster_quantity(value, source, "volume", &[("ml", 1), ("L", 1_000)])
}

fn parse_monster_quantity(
    value: &Value,
    source: &str,
    field: &str,
    units: &[(&str, i64)],
) -> Result<i64, MonsterRegistryError> {
    let text = value.as_str().ok_or_else(|| invalid(source, field))?;
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
        return Err(invalid(source, field));
    }
    let mut total = 0_i64;
    for pair in tokens.chunks_exact(2) {
        let amount = pair[0].parse::<i64>().map_err(|_| invalid(source, field))?;
        let multiplier = units
            .iter()
            .find_map(|(unit, multiplier)| (*unit == pair[1]).then_some(*multiplier))
            .ok_or_else(|| invalid(source, field))?;
        total = total
            .checked_add(
                amount
                    .checked_mul(multiplier)
                    .ok_or_else(|| invalid(source, field))?,
            )
            .ok_or_else(|| invalid(source, field))?;
    }
    if total < 0 {
        return Err(invalid(source, field));
    }
    Ok(total)
}

fn apply_text(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    *target = match value {
        Value::String(value) => value.clone(),
        Value::Object(values) => ["str", "str_sp", "str_pl"]
            .into_iter()
            .find_map(|key| values.get(key).and_then(Value::as_str))
            .map(str::to_owned)
            .ok_or_else(|| invalid(source, field))?,
        _ => return Err(invalid(source, field)),
    };
    Ok(())
}

fn apply_string(
    object: &Map<String, Value>,
    field: &str,
    target: &mut String,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get(field) {
        *target = value
            .as_str()
            .ok_or_else(|| invalid(source, field))?
            .to_owned();
    }
    Ok(())
}

fn apply_string_set(
    object: &Map<String, Value>,
    field: &str,
    target: &mut BTreeSet<String>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
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

fn string_set(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<BTreeSet<String>, MonsterRegistryError> {
    match value {
        Value::String(value) => Ok(BTreeSet::from([value.clone()])),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(source, field))
            })
            .collect(),
        _ => Err(invalid(source, field)),
    }
}

fn modifier<'a>(
    object: &'a Map<String, Value>,
    modifier_name: &str,
    field: &str,
    source: &str,
) -> Result<Option<&'a Value>, MonsterRegistryError> {
    match object.get(modifier_name) {
        None => Ok(None),
        Some(Value::Object(values)) => Ok(values.get(field)),
        Some(_) => Err(invalid(source, modifier_name)),
    }
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<&'a str>, MonsterRegistryError> {
    object
        .get(field)
        .map(|value| value.as_str().ok_or_else(|| invalid(source, field)))
        .transpose()
}

fn invalid(source: &str, field: &str) -> MonsterRegistryError {
    MonsterRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Debug)]
pub enum MonsterRegistryError {
    Catalog(ModCatalogError),
    InternalQueue,
    InvalidDefinition(String),
    InvalidField { source: String, field: String },
    InvalidFinalizedMonster { id: String, source: String },
    InvalidIdentity,
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    UnresolvedInheritance(Vec<String>),
}

impl fmt::Display for MonsterRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "monster mod selection failed: {error}"),
            Self::InternalQueue => formatter.write_str("internal MONSTER load queue failure"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "MONSTER definition is not an object in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid MONSTER field {field} in {source}")
            }
            Self::InvalidFinalizedMonster { id, source } => {
                write!(
                    formatter,
                    "MONSTER {id} is incomplete after inheritance in {source}"
                )
            }
            Self::InvalidIdentity => {
                formatter.write_str("MONSTER must have exactly one non-empty id or abstract")
            }
            Self::Io(path, error) => {
                write!(formatter, "monster registry I/O failed for {path}: {error}")
            }
            Self::Json(path, error) => {
                write!(
                    formatter,
                    "monster registry JSON failed for {path}: {error}"
                )
            }
            Self::UnresolvedInheritance(ids) => {
                write!(
                    formatter,
                    "unresolved or cyclic MONSTER inheritance: {ids:?}"
                )
            }
        }
    }
}

impl std::error::Error for MonsterRegistryError {}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn volume_uses_pinned_units_defaults_and_inheritance_arithmetic() {
        let mut monster = MonsterDefinition::default();
        assert_eq!(monster.volume_milliliters, 62_499);

        let mut truncated = monster.clone();
        apply_fields(
            &mut truncated,
            json!({ "proportional": { "volume": 2.5 } })
                .as_object()
                .expect("object"),
            "truncating proportional",
        )
        .expect("proportional volume");
        assert_eq!(truncated.volume_milliliters, 156_247);

        apply_fields(
            &mut monster,
            json!({ "volume": "62.5 L" }).as_object().expect("object"),
            "direct",
        )
        .expect_err("fractional quantity text is outside the pinned grammar");
        apply_fields(
            &mut monster,
            json!({ "volume": "62500 ml" }).as_object().expect("object"),
            "direct",
        )
        .expect("direct volume");
        assert_eq!(monster.volume_milliliters, 62_500);

        apply_fields(
            &mut monster,
            json!({ "proportional": { "volume": 0.33 } })
                .as_object()
                .expect("object"),
            "proportional",
        )
        .expect("proportional volume");
        assert_eq!(monster.volume_milliliters, 20_625);

        apply_fields(
            &mut monster,
            json!({ "relative": { "volume": "375 ml" } })
                .as_object()
                .expect("object"),
            "relative",
        )
        .expect("relative volume");
        assert_eq!(monster.volume_milliliters, 21_000);
    }

    #[test]
    fn mass_uses_pinned_units_defaults_and_inheritance_arithmetic() {
        let mut monster = MonsterDefinition::default();
        assert_eq!(monster.weight_milligrams, 81_499_000);
        apply_fields(
            &mut monster,
            json!({ "proportional": { "weight": 0.5 } })
                .as_object()
                .expect("object"),
            "proportional",
        )
        .expect("proportional mass");
        assert_eq!(monster.weight_milligrams, 40_749_500);
        apply_fields(
            &mut monster,
            json!({ "weight": "30 kg" }).as_object().expect("object"),
            "direct",
        )
        .expect("direct mass");
        assert_eq!(monster.weight_milligrams, 30_000_000);
        apply_fields(
            &mut monster,
            json!({ "relative": { "weight": "250 g" } })
                .as_object()
                .expect("object"),
            "relative",
        )
        .expect("relative mass");
        assert_eq!(monster.weight_milligrams, 30_250_000);

        assert!(matches!(
            apply_fields(
                &mut monster,
                json!({ "delete": { "weight": "1 kg" } })
                    .as_object()
                    .expect("object"),
                "delete",
            ),
            Err(MonsterRegistryError::InvalidField { field, .. })
                if field == "delete.weight"
        ));
    }

    #[test]
    fn attack_cost_uses_pinned_default_precedence_and_integer_modifiers() {
        let mut monster = MonsterDefinition::default();
        assert_eq!(monster.attack_cost_moves, 100);
        apply_fields(
            &mut monster,
            json!({ "attack_cost": 70, "relative": { "attack_cost": 50 } })
                .as_object()
                .expect("object"),
            "direct precedence",
        )
        .expect("direct attack cost");
        assert_eq!(monster.attack_cost_moves, 70);
        apply_fields(
            &mut monster,
            json!({ "relative": { "attack_cost": 5 } })
                .as_object()
                .expect("object"),
            "relative",
        )
        .expect("relative attack cost");
        assert_eq!(monster.attack_cost_moves, 75);
        apply_fields(
            &mut monster,
            json!({ "proportional": { "attack_cost": 1.5 } })
                .as_object()
                .expect("object"),
            "proportional",
        )
        .expect("proportional attack cost");
        assert_eq!(monster.attack_cost_moves, 112);
        apply_fields(
            &mut monster,
            json!({ "proportional": { "attack_cost": 0 } })
                .as_object()
                .expect("object"),
            "invalid proportional",
        )
        .expect_err("nonpositive proportional scalar is invalid upstream");
        apply_fields(
            &mut monster,
            json!({ "attack_cost": -1 }).as_object().expect("object"),
            "negative",
        )
        .expect_err("negative attack cost is outside the pinned bound");
    }
}
