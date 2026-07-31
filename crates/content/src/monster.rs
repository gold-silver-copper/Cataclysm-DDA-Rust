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
