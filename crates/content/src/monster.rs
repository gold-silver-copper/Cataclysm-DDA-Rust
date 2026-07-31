use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{
    ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile,
    eoc::{EocConditionDefinition, parse_condition},
};

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
    "special_attacks",
    "starting_ammo",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MonsterSpecialAttackKind {
    Melee,
    Bite,
    Leap,
    Eoc,
    Gun,
    Polymorph,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterGunRangeDefinition {
    pub minimum: u32,
    pub maximum: u32,
    /// Empty and `DEFAULT` both select the ordinary single-shot gun mode.
    pub mode_id: String,
}

/// One finalized generic monster attack actor. Definitions with behavior
/// outside this profile are retained with explicit deferred fields and are not
/// admitted by the authoritative runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonsterSpecialAttackDefinition {
    pub id: String,
    pub kind: MonsterSpecialAttackKind,
    pub cooldown_turns: u32,
    pub move_cost_moves: u32,
    pub accuracy: Option<i32>,
    pub range: u32,
    pub no_adjacent: bool,
    pub dodgeable: bool,
    pub minimum_damage_multiplier_millionths: i32,
    pub maximum_damage_multiplier_millionths: i32,
    pub damage: Vec<MonsterMeleeDamageUnitDefinition>,
    pub effects: Vec<MonsterAttackEffectDefinition>,
    pub effects_require_damage: bool,
    pub infection_chance_millionths: u32,
    pub leap_minimum_range_milli: u32,
    pub leap_maximum_range_milli: u32,
    pub leap_minimum_consider_range_milli: u32,
    pub leap_maximum_consider_range_milli: u32,
    pub leap_allow_no_target: bool,
    pub leap_prefer: bool,
    pub leap_random: bool,
    pub leap_ignore_destination_danger: bool,
    pub condition: Option<EocConditionDefinition>,
    pub eoc_ids: Vec<String>,
    pub polymorph_monster_type_id: String,
    pub polymorph_keep_speed: bool,
    pub polymorph_keep_hp: bool,
    pub polymorph_keep_aggression: bool,
    pub gun_type_id: String,
    pub gun_ammunition_type_id: String,
    pub gun_fake_skills: BTreeMap<String, u16>,
    pub gun_fake_strength: u16,
    pub gun_fake_dexterity: u16,
    pub gun_fake_intelligence: u16,
    pub gun_fake_perception: u16,
    pub gun_ranges: Vec<MonsterGunRangeDefinition>,
    pub gun_max_ammunition: Option<u32>,
    pub gun_targeting_cost_moves: u32,
    pub gun_require_targeting_player: bool,
    pub gun_require_targeting_npc: bool,
    pub gun_require_targeting_monster: bool,
    pub gun_targeting_timeout_turns: u32,
    pub gun_targeting_timeout_extend_turns: i32,
    pub gun_targeting_sound: String,
    pub gun_targeting_volume: u32,
    pub gun_laser_lock: bool,
    pub gun_target_moving_vehicles: bool,
    pub gun_require_sunlight: bool,
    pub unsupported_fields: BTreeSet<String>,
}

impl MonsterSpecialAttackDefinition {
    #[must_use]
    pub fn is_fully_supported(&self) -> bool {
        self.unsupported_fields.is_empty()
            && self.maximum_damage_multiplier_millionths
                >= self.minimum_damage_multiplier_millionths
            && self.minimum_damage_multiplier_millionths >= 0
            && self
                .damage
                .iter()
                .all(|unit| unit.damage_multiplier_millionths > 0)
            && (self.kind != MonsterSpecialAttackKind::Leap
                || (self.leap_maximum_range_milli <= 100_000
                    && self.damage.is_empty()
                    && self.effects.is_empty()
                    && self.infection_chance_millionths == 0
                    && self.leap_minimum_range_milli <= self.leap_maximum_range_milli
                    && self.leap_minimum_consider_range_milli
                        <= self.leap_maximum_consider_range_milli))
            && (self.kind != MonsterSpecialAttackKind::Gun
                || (!self.gun_type_id.is_empty()
                    && !self.gun_ranges.is_empty()
                    && self
                        .gun_ranges
                        .iter()
                        .all(|range| range.minimum <= range.maximum)
                    && self.range
                        == self
                            .gun_ranges
                            .iter()
                            .map(|range| range.maximum)
                            .max()
                            .unwrap_or(0)))
            && (self.kind != MonsterSpecialAttackKind::Polymorph
                || !self.polymorph_monster_type_id.is_empty())
    }
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
    /// ID-sorted finalized generic special attacks. The pinned engine stores
    /// these in a map and attempts them in key order.
    pub special_attacks: BTreeMap<String, MonsterSpecialAttackDefinition>,
    /// Final inherited per-item ammunition pool assigned to each newly
    /// constructed monster. Runtime gun actors index this map by their
    /// concrete `ammo_type` item ID.
    pub starting_ammunition: BTreeMap<String, u32>,
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
            special_attacks: BTreeMap::new(),
            starting_ammunition: BTreeMap::new(),
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
        let (mut pending, raw_attacks) = read_definitions(content_root.as_ref(), files)?;
        let attacks = raw_attacks
            .into_iter()
            .map(|raw| {
                let attack = parse_special_attack(&raw.object, None, &raw.file.upstream_path)?;
                Ok((attack.id.clone(), attack))
            })
            .collect::<Result<BTreeMap<_, _>, MonsterRegistryError>>()?;
        let mut monsters = BTreeMap::new();
        let mut abstracts = BTreeMap::new();
        while !pending.is_empty() {
            let pass_size = pending.len();
            let mut loaded = 0;
            for _ in 0..pass_size {
                let raw = pending
                    .pop_front()
                    .ok_or(MonsterRegistryError::InternalQueue)?;
                if load_one(&raw, &attacks, &mut monsters, &mut abstracts)? {
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

fn read_definitions(
    root: &Path,
    files: Vec<SelectedContentFile>,
) -> Result<(VecDeque<RawMonster>, Vec<RawMonster>), MonsterRegistryError> {
    let mut monsters = VecDeque::new();
    let mut attacks = Vec::new();
    for file in files {
        let bytes = fs::read(root.join(&file.destination))
            .map_err(|error| MonsterRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| MonsterRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    collect_definition(&file, value, &mut monsters, &mut attacks)?;
                }
            }
            value => collect_definition(&file, value, &mut monsters, &mut attacks)?,
        }
    }
    Ok((monsters, attacks))
}

fn collect_definition(
    file: &SelectedContentFile,
    value: Value,
    monsters: &mut VecDeque<RawMonster>,
    attacks: &mut Vec<RawMonster>,
) -> Result<(), MonsterRegistryError> {
    let target = match value.get("type").and_then(Value::as_str) {
        Some("MONSTER") => monsters,
        Some("monster_attack") => {
            attacks.push(RawMonster {
                file: file.clone(),
                object: value.as_object().cloned().ok_or_else(|| {
                    MonsterRegistryError::InvalidDefinition(file.upstream_path.clone())
                })?,
            });
            return Ok(());
        }
        _ => return Ok(()),
    };
    target.push_back(RawMonster {
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
    attacks: &BTreeMap<String, MonsterSpecialAttackDefinition>,
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
    apply_fields(&mut monster, &raw.object, attacks, &context)?;
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
    attacks: &BTreeMap<String, MonsterSpecialAttackDefinition>,
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
    apply_special_attacks(monster, object, attacks, source)?;
    apply_starting_ammunition(monster, object, source)?;
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

fn apply_starting_ammunition(
    monster: &mut MonsterDefinition,
    object: &Map<String, Value>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    if let Some(value) = object.get("starting_ammo") {
        let entries = value
            .as_object()
            .ok_or_else(|| invalid(source, "starting_ammo"))?;
        let mut ammunition = BTreeMap::new();
        for (item_id, amount) in entries {
            if item_id.is_empty() || item_id.len() > 512 || item_id.chars().any(char::is_control) {
                return Err(invalid(source, "starting_ammo"));
            }
            let amount = amount
                .as_u64()
                .and_then(|amount| u32::try_from(amount).ok())
                .ok_or_else(|| invalid(source, "starting_ammo"))?;
            ammunition.insert(item_id.clone(), amount);
        }
        monster.starting_ammunition = ammunition;
    }
    for modifier_name in ["extend", "delete", "relative", "proportional"] {
        if modifier(object, modifier_name, "starting_ammo", source)?.is_some() {
            monster
                .unsupported_fields
                .insert(format!("starting_ammo.{modifier_name}"));
        }
    }
    Ok(())
}

fn apply_special_attacks(
    monster: &mut MonsterDefinition,
    object: &Map<String, Value>,
    attacks: &BTreeMap<String, MonsterSpecialAttackDefinition>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    monster
        .unsupported_fields
        .retain(|field| field != "special_attacks" && !field.starts_with("special_attacks."));
    if let Some(value) = object.get("special_attacks") {
        monster.special_attacks.clear();
        insert_special_attacks(&mut monster.special_attacks, value, attacks, source)?;
    } else {
        if let Some(value) = modifier(object, "extend", "special_attacks", source)? {
            insert_special_attacks(&mut monster.special_attacks, value, attacks, source)?;
        }
        if let Some(value) = modifier(object, "delete", "special_attacks", source)? {
            let values = value
                .as_array()
                .map_or_else(|| vec![value], |values| values.iter().collect());
            for value in values {
                let id = value
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| invalid(source, "delete.special_attacks"))?;
                monster.special_attacks.remove(id);
            }
        }
        for modifier_name in ["relative", "proportional"] {
            if modifier(object, modifier_name, "special_attacks", source)?.is_some() {
                monster
                    .unsupported_fields
                    .insert(format!("special_attacks.{modifier_name}"));
            }
        }
    }
    for attack in monster.special_attacks.values() {
        for field in &attack.unsupported_fields {
            monster
                .unsupported_fields
                .insert(format!("special_attacks.{}.{}", attack.id, field));
        }
    }
    Ok(())
}

fn insert_special_attacks(
    target: &mut BTreeMap<String, MonsterSpecialAttackDefinition>,
    value: &Value,
    attacks: &BTreeMap<String, MonsterSpecialAttackDefinition>,
    source: &str,
) -> Result<(), MonsterRegistryError> {
    let values = value
        .as_array()
        .map_or_else(|| vec![value], |values| values.iter().collect());
    for value in values {
        let attack = if let Some(pair) = value.as_array() {
            if pair.len() != 2 {
                return Err(invalid(source, "special_attacks"));
            }
            let id = pair[0]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid(source, "special_attacks.id"))?;
            let mut attack = attacks
                .get(id)
                .cloned()
                .unwrap_or_else(|| unsupported_special_attack(id, "unresolved_id"));
            attack.cooldown_turns = parse_u32(&pair[1], source, "special_attacks.cooldown")?;
            attack
        } else {
            let fields = value
                .as_object()
                .ok_or_else(|| invalid(source, "special_attacks"))?;
            let actor_type = fields
                .get("attack_type")
                .or_else(|| fields.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("monster_attack");
            let base = (actor_type == "monster_attack")
                .then(|| {
                    fields
                        .get("id")
                        .and_then(Value::as_str)
                        .and_then(|id| attacks.get(id))
                })
                .flatten();
            parse_special_attack(fields, base, source)?
        };
        if target.insert(attack.id.clone(), attack).is_some() {
            return Err(invalid(source, "special_attacks.duplicate_id"));
        }
    }
    Ok(())
}

fn unsupported_special_attack(id: &str, field: &str) -> MonsterSpecialAttackDefinition {
    MonsterSpecialAttackDefinition {
        id: id.to_owned(),
        kind: MonsterSpecialAttackKind::Unsupported,
        cooldown_turns: 0,
        move_cost_moves: 100,
        accuracy: None,
        range: 1,
        no_adjacent: false,
        dodgeable: true,
        minimum_damage_multiplier_millionths: 500_000,
        maximum_damage_multiplier_millionths: 1_000_000,
        damage: vec![MonsterMeleeDamageUnitDefinition {
            damage_type_id: String::from("bash"),
            amount_milli: 9_000,
            armor_penetration_milli: 0,
            armor_multiplier_millionths: 1_000_000,
            damage_multiplier_millionths: 1_000_000,
            constant_armor_multiplier_millionths: 1_000_000,
            constant_damage_multiplier_millionths: 1_000_000,
        }],
        effects: Vec::new(),
        effects_require_damage: true,
        infection_chance_millionths: 0,
        leap_minimum_range_milli: 0,
        leap_maximum_range_milli: 0,
        leap_minimum_consider_range_milli: 0,
        leap_maximum_consider_range_milli: 0,
        leap_allow_no_target: false,
        leap_prefer: false,
        leap_random: false,
        leap_ignore_destination_danger: false,
        condition: None,
        eoc_ids: Vec::new(),
        polymorph_monster_type_id: String::new(),
        polymorph_keep_speed: false,
        polymorph_keep_hp: false,
        polymorph_keep_aggression: false,
        gun_type_id: String::new(),
        gun_ammunition_type_id: String::new(),
        gun_fake_skills: BTreeMap::new(),
        gun_fake_strength: 8,
        gun_fake_dexterity: 8,
        gun_fake_intelligence: 8,
        gun_fake_perception: 8,
        gun_ranges: Vec::new(),
        gun_max_ammunition: None,
        gun_targeting_cost_moves: 100,
        gun_require_targeting_player: true,
        gun_require_targeting_npc: false,
        gun_require_targeting_monster: false,
        gun_targeting_timeout_turns: 8,
        gun_targeting_timeout_extend_turns: 3,
        gun_targeting_sound: String::from("Beep."),
        gun_targeting_volume: 6,
        gun_laser_lock: false,
        gun_target_moving_vehicles: false,
        gun_require_sunlight: false,
        unsupported_fields: BTreeSet::from([field.to_owned()]),
    }
}

fn parse_special_attack(
    fields: &Map<String, Value>,
    base: Option<&MonsterSpecialAttackDefinition>,
    source: &str,
) -> Result<MonsterSpecialAttackDefinition, MonsterRegistryError> {
    let declared_type = fields
        .get("attack_type")
        .or_else(|| fields.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("monster_attack");
    let id = fields
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| (declared_type != "monster_attack").then_some(declared_type))
        .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
        .ok_or_else(|| invalid(source, "special_attacks.id"))?;
    let kind = match declared_type {
        "melee" => MonsterSpecialAttackKind::Melee,
        "bite" => MonsterSpecialAttackKind::Bite,
        "leap" => MonsterSpecialAttackKind::Leap,
        "eoc" => MonsterSpecialAttackKind::Eoc,
        "gun" => MonsterSpecialAttackKind::Gun,
        "polymorph_special" => MonsterSpecialAttackKind::Polymorph,
        "monster_attack" => base
            .map(|base| base.kind)
            .unwrap_or(MonsterSpecialAttackKind::Unsupported),
        _ => MonsterSpecialAttackKind::Unsupported,
    };
    let mut attack = base
        .cloned()
        .unwrap_or_else(|| unsupported_special_attack(id, "missing_actor_type"));
    attack.id = id.to_owned();
    attack.kind = kind;
    if kind != MonsterSpecialAttackKind::Unsupported {
        attack.unsupported_fields.remove("missing_actor_type");
    }
    if declared_type != "monster_attack"
        && !matches!(
            declared_type,
            "melee" | "bite" | "leap" | "eoc" | "gun" | "polymorph_special"
        )
    {
        attack
            .unsupported_fields
            .insert(format!("attack_type.{declared_type}"));
    }

    if let Some(value) = fields.get("cooldown") {
        attack.cooldown_turns = parse_u32(value, source, "special_attacks.cooldown")?;
    } else if base.is_none() {
        return Err(invalid(source, "special_attacks.cooldown"));
    }
    if kind == MonsterSpecialAttackKind::Leap && base.is_none() {
        attack.move_cost_moves = 150;
        attack.damage.clear();
        attack.effects.clear();
        attack.effects_require_damage = false;
        attack.minimum_damage_multiplier_millionths = 0;
        attack.maximum_damage_multiplier_millionths = 0;
        attack.leap_minimum_range_milli = 1_000;
        attack.leap_maximum_consider_range_milli = 200_000;
    }
    if kind == MonsterSpecialAttackKind::Eoc && base.is_none() {
        attack.move_cost_moves = 0;
        attack.accuracy = None;
        attack.no_adjacent = false;
        attack.dodgeable = false;
        attack.minimum_damage_multiplier_millionths = 0;
        attack.maximum_damage_multiplier_millionths = 0;
        attack.damage.clear();
        attack.effects.clear();
    }
    if kind == MonsterSpecialAttackKind::Gun && base.is_none() {
        attack.move_cost_moves = 150;
        attack.accuracy = None;
        attack.range = 0;
        attack.no_adjacent = false;
        attack.dodgeable = false;
        attack.minimum_damage_multiplier_millionths = 0;
        attack.maximum_damage_multiplier_millionths = 0;
        attack.damage.clear();
        attack.effects.clear();
        attack.effects_require_damage = false;
        attack.infection_chance_millionths = 0;
        attack.leap_minimum_range_milli = 0;
        attack.leap_maximum_range_milli = 0;
        attack.leap_minimum_consider_range_milli = 0;
        attack.leap_maximum_consider_range_milli = 0;
        attack.leap_allow_no_target = false;
        attack.leap_prefer = false;
        attack.leap_random = false;
        attack.leap_ignore_destination_danger = false;
        attack.eoc_ids.clear();
        attack.gun_type_id.clear();
        attack.gun_ammunition_type_id.clear();
        attack.gun_fake_skills.clear();
        attack.gun_fake_strength = 8;
        attack.gun_fake_dexterity = 8;
        attack.gun_fake_intelligence = 8;
        attack.gun_fake_perception = 8;
        attack.gun_ranges.clear();
        attack.gun_max_ammunition = None;
        attack.gun_targeting_cost_moves = 100;
        attack.gun_require_targeting_player = true;
        attack.gun_require_targeting_npc = false;
        attack.gun_require_targeting_monster = false;
        attack.gun_targeting_timeout_turns = 8;
        attack.gun_targeting_timeout_extend_turns = 3;
        attack.gun_targeting_sound = String::from("Beep.");
        attack.gun_targeting_volume = 6;
        attack.gun_laser_lock = false;
        attack.gun_target_moving_vehicles = false;
        attack.gun_require_sunlight = false;
    }
    if kind == MonsterSpecialAttackKind::Polymorph
        && base.is_none_or(|base| base.kind != MonsterSpecialAttackKind::Polymorph)
    {
        attack.move_cost_moves = 0;
        attack.accuracy = None;
        attack.range = 0;
        attack.no_adjacent = false;
        attack.dodgeable = false;
        attack.minimum_damage_multiplier_millionths = 0;
        attack.maximum_damage_multiplier_millionths = 0;
        attack.damage.clear();
        attack.effects.clear();
        attack.effects_require_damage = false;
        attack.infection_chance_millionths = 0;
        attack.eoc_ids.clear();
        attack.polymorph_monster_type_id.clear();
        attack.polymorph_keep_speed = true;
        attack.polymorph_keep_hp = true;
        attack.polymorph_keep_aggression = true;
    }
    if let Some(value) = fields.get("move_cost") {
        attack.move_cost_moves = parse_u32(value, source, "special_attacks.move_cost")?;
    }
    if let Some(value) = fields.get("accuracy") {
        let accuracy = parse_i32(value, source, "special_attacks.accuracy")?;
        attack.accuracy = (accuracy >= 0).then_some(accuracy);
    }
    if let Some(value) = fields.get("range") {
        attack.range = parse_u32(value, source, "special_attacks.range")?;
    }
    for (field, target) in [
        ("no_adjacent", &mut attack.no_adjacent),
        ("dodgeable", &mut attack.dodgeable),
        ("effects_require_dmg", &mut attack.effects_require_damage),
    ] {
        if let Some(value) = fields.get(field) {
            *target = value
                .as_bool()
                .ok_or_else(|| invalid(source, &format!("special_attacks.{field}")))?;
        }
    }
    if let Some(value) = fields.get("min_mul") {
        attack.minimum_damage_multiplier_millionths =
            parse_scaled_number(value, 1_000_000, source, "special_attacks.min_mul")?;
    }
    if let Some(value) = fields.get("max_mul") {
        attack.maximum_damage_multiplier_millionths =
            parse_scaled_number(value, 1_000_000, source, "special_attacks.max_mul")?;
    }
    if let Some(value) = fields.get("damage_max_instance") {
        let (damage, deferred) = parse_melee_damage(value, source, "damage_max_instance")?;
        attack.damage = damage;
        attack.unsupported_fields.extend(
            deferred
                .into_iter()
                .map(|field| field.replacen("melee_damage", "damage_max_instance", 1)),
        );
    }
    if let Some(value) = fields.get("effects") {
        let (effects, deferred) = parse_monster_effects(value, source, "special_attacks.effects")?;
        attack.effects = effects;
        attack.unsupported_fields.extend(deferred);
    }
    if let Some(value) = fields.get("condition") {
        attack.unsupported_fields.remove("condition");
        let mut unsupported = BTreeSet::new();
        attack.condition = parse_condition(value, "special_attacks.condition", 0, &mut unsupported);
        attack.unsupported_fields.extend(unsupported);
        if attack.condition.is_none() {
            attack.unsupported_fields.insert(String::from("condition"));
        }
    }
    if let Some(value) = fields.get("eoc") {
        attack.unsupported_fields.remove("eoc");
        let values = value
            .as_array()
            .map_or_else(|| vec![value], |values| values.iter().collect());
        let mut eoc_ids = Vec::with_capacity(values.len());
        for value in values {
            let Some(eoc_id) = value.as_str().filter(|id| {
                !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
            }) else {
                attack.unsupported_fields.insert(String::from("eoc"));
                eoc_ids.clear();
                break;
            };
            eoc_ids.push(eoc_id.to_owned());
        }
        if eoc_ids.is_empty() {
            attack.unsupported_fields.insert(String::from("eoc"));
        } else {
            attack.eoc_ids = eoc_ids;
        }
    }
    if kind == MonsterSpecialAttackKind::Gun {
        if let Some(value) = fields.get("gun_type") {
            attack.gun_type_id = value
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
                .ok_or_else(|| invalid(source, "special_attacks.gun_type"))?
                .to_owned();
        } else if base.is_none() || attack.gun_type_id.is_empty() {
            return Err(invalid(source, "special_attacks.gun_type"));
        }
        if let Some(value) = fields.get("ammo_type") {
            attack.gun_ammunition_type_id = value
                .as_str()
                .filter(|id| id.len() <= 512 && !id.chars().any(char::is_control))
                .ok_or_else(|| invalid(source, "special_attacks.ammo_type"))?
                .to_owned();
        }
        if let Some(value) = fields.get("fake_skills") {
            let values = value
                .as_array()
                .ok_or_else(|| invalid(source, "special_attacks.fake_skills"))?;
            let mut skills = BTreeMap::new();
            for value in values {
                let pair = value
                    .as_array()
                    .filter(|pair| pair.len() == 2)
                    .ok_or_else(|| invalid(source, "special_attacks.fake_skills"))?;
                let skill_id = pair[0]
                    .as_str()
                    .filter(|id| {
                        !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
                    })
                    .ok_or_else(|| invalid(source, "special_attacks.fake_skills"))?;
                let level =
                    u16::try_from(parse_u32(&pair[1], source, "special_attacks.fake_skills")?)
                        .map_err(|_| invalid(source, "special_attacks.fake_skills"))?;
                skills.insert(skill_id.to_owned(), level);
            }
            attack.gun_fake_skills = skills;
        }
        for (field, target) in [
            ("fake_str", &mut attack.gun_fake_strength),
            ("fake_dex", &mut attack.gun_fake_dexterity),
            ("fake_int", &mut attack.gun_fake_intelligence),
            ("fake_per", &mut attack.gun_fake_perception),
        ] {
            if let Some(value) = fields.get(field) {
                *target = u16::try_from(parse_u32(
                    value,
                    source,
                    &format!("special_attacks.{field}"),
                )?)
                .map_err(|_| invalid(source, &format!("special_attacks.{field}")))?;
            }
        }
        if let Some(value) = fields.get("ranges") {
            let values = value
                .as_array()
                .ok_or_else(|| invalid(source, "special_attacks.ranges"))?;
            let mut ranges = BTreeMap::new();
            for value in values {
                let range = value
                    .as_array()
                    .filter(|range| (2..=3).contains(&range.len()))
                    .ok_or_else(|| invalid(source, "special_attacks.ranges"))?;
                let minimum = parse_u32(&range[0], source, "special_attacks.ranges")?;
                let maximum = parse_u32(&range[1], source, "special_attacks.ranges")?;
                if minimum > maximum {
                    return Err(invalid(source, "special_attacks.ranges"));
                }
                let mode_id = range
                    .get(2)
                    .map_or(Ok(""), |value| {
                        value
                            .as_str()
                            .filter(|id| id.len() <= 512 && !id.chars().any(char::is_control))
                            .ok_or_else(|| invalid(source, "special_attacks.ranges"))
                    })?
                    .to_owned();
                ranges.entry((minimum, maximum)).or_insert(mode_id);
            }
            attack.gun_ranges = ranges
                .into_iter()
                .map(|((minimum, maximum), mode_id)| MonsterGunRangeDefinition {
                    minimum,
                    maximum,
                    mode_id,
                })
                .collect();
            attack.range = attack
                .gun_ranges
                .iter()
                .map(|range| range.maximum)
                .max()
                .unwrap_or(0);
        } else if base.is_none() || attack.gun_ranges.is_empty() {
            return Err(invalid(source, "special_attacks.ranges"));
        }
        if let Some(value) = fields.get("max_ammo") {
            attack.gun_max_ammunition = Some(parse_u32(value, source, "special_attacks.max_ammo")?);
        }
        for (field, target) in [
            ("targeting_cost", &mut attack.gun_targeting_cost_moves),
            ("targeting_timeout", &mut attack.gun_targeting_timeout_turns),
            ("targeting_volume", &mut attack.gun_targeting_volume),
        ] {
            if let Some(value) = fields.get(field) {
                *target = parse_u32(value, source, &format!("special_attacks.{field}"))?;
            }
        }
        if let Some(value) = fields.get("targeting_timeout_extend") {
            attack.gun_targeting_timeout_extend_turns =
                parse_i32(value, source, "special_attacks.targeting_timeout_extend")?;
        }
        apply_text(
            fields,
            "targeting_sound",
            &mut attack.gun_targeting_sound,
            source,
        )?;
        for (field, target) in [
            (
                "require_targeting_player",
                &mut attack.gun_require_targeting_player,
            ),
            (
                "require_targeting_npc",
                &mut attack.gun_require_targeting_npc,
            ),
            (
                "require_targeting_monster",
                &mut attack.gun_require_targeting_monster,
            ),
            ("laser_lock", &mut attack.gun_laser_lock),
            (
                "target_moving_vehicles",
                &mut attack.gun_target_moving_vehicles,
            ),
            ("require_sunlight", &mut attack.gun_require_sunlight),
        ] {
            if let Some(value) = fields.get(field) {
                *target = value
                    .as_bool()
                    .ok_or_else(|| invalid(source, &format!("special_attacks.{field}")))?;
            }
        }
    }
    if kind == MonsterSpecialAttackKind::Polymorph {
        if let Some(value) = fields.get("mon_id") {
            attack.polymorph_monster_type_id = value
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
                .ok_or_else(|| invalid(source, "special_attacks.mon_id"))?
                .to_owned();
        } else if base.is_none() || attack.polymorph_monster_type_id.is_empty() {
            return Err(invalid(source, "special_attacks.mon_id"));
        }
        for (field, target) in [
            ("poly_keep_speed", &mut attack.polymorph_keep_speed),
            ("poly_keep_hp", &mut attack.polymorph_keep_hp),
            ("poly_keep_anger", &mut attack.polymorph_keep_aggression),
        ] {
            if let Some(value) = fields.get(field) {
                *target = value
                    .as_bool()
                    .ok_or_else(|| invalid(source, &format!("special_attacks.{field}")))?;
            }
        }
    }
    if kind == MonsterSpecialAttackKind::Bite {
        if let Some(value) = fields.get("infection_chance") {
            let percent = parse_u32(value, source, "special_attacks.infection_chance")?;
            if percent > 100 {
                return Err(invalid(source, "special_attacks.infection_chance"));
            }
            attack.infection_chance_millionths = percent * 10_000;
        } else if base.is_none() || base.is_some_and(|base| base.kind != kind) {
            attack.infection_chance_millionths = 50_000;
        }
    } else {
        attack.infection_chance_millionths = 0;
    }
    if kind == MonsterSpecialAttackKind::Leap {
        if let Some(maximum_range) = fields.get("max_range") {
            attack.leap_maximum_range_milli =
                parse_u32_scaled(maximum_range, 1_000, source, "special_attacks.max_range")?;
        } else if base.is_none() || attack.leap_maximum_range_milli == 0 {
            return Err(invalid(source, "special_attacks.max_range"));
        }
        for (field, target) in [
            ("min_range", &mut attack.leap_minimum_range_milli),
            (
                "min_consider_range",
                &mut attack.leap_minimum_consider_range_milli,
            ),
            (
                "max_consider_range",
                &mut attack.leap_maximum_consider_range_milli,
            ),
        ] {
            if let Some(value) = fields.get(field) {
                *target =
                    parse_u32_scaled(value, 1_000, source, &format!("special_attacks.{field}"))?;
            }
        }
        for (field, target) in [
            ("allow_no_target", &mut attack.leap_allow_no_target),
            ("prefer_leap", &mut attack.leap_prefer),
            ("random_leap", &mut attack.leap_random),
            (
                "ignore_dest_danger",
                &mut attack.leap_ignore_destination_danger,
            ),
        ] {
            if let Some(value) = fields.get(field) {
                *target = value
                    .as_bool()
                    .ok_or_else(|| invalid(source, &format!("special_attacks.{field}")))?;
            }
        }
        if fields
            .get("ignore_dest_terrain")
            .is_some_and(|value| value.as_bool() != Some(false))
        {
            attack
                .unsupported_fields
                .insert(String::from("ignore_dest_terrain"));
        }
    }

    const COSMETIC_FIELDS: &[&str] = &[
        "miss_msg_u",
        "no_dmg_msg_u",
        "hit_dmg_u",
        "miss_msg_npc",
        "no_dmg_msg_npc",
        "hit_dmg_npc",
        "throw_msg_u",
        "throw_msg_npc",
        "description",
        "failure_msg",
        "no_ammo_sound",
        "targeting_sound",
    ];
    const IMPLEMENTED: &[&str] = &[
        "type",
        "attack_type",
        "id",
        "cooldown",
        "damage_max_instance",
        "accuracy",
        "min_mul",
        "max_mul",
        "move_cost",
        "range",
        "no_adjacent",
        "dodgeable",
        "uncanny_dodgeable",
        "blockable",
        "effects_require_dmg",
        "effects_require_organic",
        "attack_amount",
        "spread_damage",
        "throw_strength",
        "effects",
        "condition",
        "eoc",
        "infection_chance",
        "max_range",
        "min_range",
        "allow_no_target",
        "prefer_leap",
        "random_leap",
        "ignore_dest_terrain",
        "ignore_dest_danger",
        "min_consider_range",
        "max_consider_range",
        "message",
        "gun_type",
        "ammo_type",
        "fake_skills",
        "fake_str",
        "fake_dex",
        "fake_int",
        "fake_per",
        "ranges",
        "max_ammo",
        "targeting_cost",
        "require_targeting_player",
        "require_targeting_npc",
        "require_targeting_monster",
        "targeting_timeout",
        "targeting_timeout_extend",
        "targeting_volume",
        "laser_lock",
        "target_moving_vehicles",
        "require_sunlight",
        "mon_id",
        "poly_keep_speed",
        "poly_keep_hp",
        "poly_keep_anger",
    ];
    for field in fields.keys().filter(|field| !field.starts_with("//")) {
        if COSMETIC_FIELDS.contains(&field.as_str()) {
            continue;
        }
        if !IMPLEMENTED.contains(&field.as_str())
            || matches!(
                field.as_str(),
                "grab"
                    | "grab_data"
                    | "body_part_types"
                    | "self_effects_always"
                    | "self_effects_onhit"
                    | "self_effects_ondmg"
            )
        {
            attack.unsupported_fields.insert(field.clone());
        }
    }
    const MELEE_ONLY_FIELDS: &[&str] = &[
        "damage_max_instance",
        "accuracy",
        "min_mul",
        "max_mul",
        "no_adjacent",
        "dodgeable",
        "uncanny_dodgeable",
        "blockable",
        "effects_require_dmg",
        "effects_require_organic",
        "attack_amount",
        "spread_damage",
        "throw_strength",
        "effects",
        "infection_chance",
    ];
    const LEAP_ONLY_FIELDS: &[&str] = &[
        "max_range",
        "min_range",
        "allow_no_target",
        "prefer_leap",
        "random_leap",
        "ignore_dest_terrain",
        "ignore_dest_danger",
        "min_consider_range",
        "max_consider_range",
    ];
    const GUN_ONLY_FIELDS: &[&str] = &[
        "gun_type",
        "ammo_type",
        "fake_skills",
        "fake_str",
        "fake_dex",
        "fake_int",
        "fake_per",
        "ranges",
        "max_ammo",
        "targeting_cost",
        "require_targeting_player",
        "require_targeting_npc",
        "require_targeting_monster",
        "targeting_timeout",
        "targeting_timeout_extend",
        "targeting_volume",
        "laser_lock",
        "target_moving_vehicles",
        "require_sunlight",
    ];
    const POLYMORPH_ONLY_FIELDS: &[&str] = &[
        "mon_id",
        "poly_keep_speed",
        "poly_keep_hp",
        "poly_keep_anger",
    ];
    let inapplicable = match kind {
        MonsterSpecialAttackKind::Leap => MELEE_ONLY_FIELDS,
        MonsterSpecialAttackKind::Melee | MonsterSpecialAttackKind::Bite => LEAP_ONLY_FIELDS,
        MonsterSpecialAttackKind::Eoc | MonsterSpecialAttackKind::Gun => &[],
        MonsterSpecialAttackKind::Polymorph => &[],
        MonsterSpecialAttackKind::Unsupported => &[],
    };
    attack.unsupported_fields.extend(
        inapplicable
            .iter()
            .filter(|field| fields.contains_key(**field))
            .map(|field| (*field).to_owned()),
    );
    if kind != MonsterSpecialAttackKind::Polymorph {
        attack.unsupported_fields.extend(
            POLYMORPH_ONLY_FIELDS
                .iter()
                .filter(|field| fields.contains_key(**field))
                .map(|field| (*field).to_owned()),
        );
    } else {
        attack.unsupported_fields.extend(
            MELEE_ONLY_FIELDS
                .iter()
                .chain(LEAP_ONLY_FIELDS)
                .chain(GUN_ONLY_FIELDS)
                .copied()
                .chain(["move_cost", "range", "eoc"])
                .filter(|field| fields.contains_key(*field))
                .map(str::to_owned),
        );
    }
    if kind == MonsterSpecialAttackKind::Leap && fields.contains_key("range") {
        attack.unsupported_fields.insert(String::from("range"));
    }
    if kind == MonsterSpecialAttackKind::Leap {
        for field in ["condition", "eoc"] {
            if fields.contains_key(field) {
                attack.unsupported_fields.insert(field.to_owned());
            }
        }
    }
    if kind == MonsterSpecialAttackKind::Eoc {
        attack.unsupported_fields.extend(
            MELEE_ONLY_FIELDS
                .iter()
                .chain(LEAP_ONLY_FIELDS)
                .filter(|field| !matches!(**field, "allow_no_target"))
                .filter(|field| fields.contains_key(**field))
                .map(|field| (*field).to_owned()),
        );
        if attack.eoc_ids.is_empty() {
            attack.unsupported_fields.insert(String::from("eoc"));
        }
        if fields
            .get("allow_no_target")
            .is_some_and(|value| value.as_bool() != Some(false))
        {
            attack
                .unsupported_fields
                .insert(String::from("allow_no_target"));
        }
    }
    if kind == MonsterSpecialAttackKind::Gun {
        attack.unsupported_fields.extend(
            MELEE_ONLY_FIELDS
                .iter()
                .chain(LEAP_ONLY_FIELDS)
                .filter(|field| fields.contains_key(**field))
                .map(|field| (*field).to_owned()),
        );
        for field in ["range", "eoc"] {
            if fields.contains_key(field) {
                attack.unsupported_fields.insert(field.to_owned());
            }
        }
    } else {
        attack.unsupported_fields.extend(
            GUN_ONLY_FIELDS
                .iter()
                .filter(|field| fields.contains_key(**field))
                .map(|field| (*field).to_owned()),
        );
    }
    if kind != MonsterSpecialAttackKind::Bite && fields.contains_key("infection_chance") {
        attack
            .unsupported_fields
            .insert(String::from("infection_chance"));
    }
    if fields
        .get("effects_require_organic")
        .is_some_and(|value| value.as_bool() != Some(false))
    {
        attack
            .unsupported_fields
            .insert(String::from("effects_require_organic"));
    }
    if fields.get("attack_amount").is_some_and(|value| {
        value.as_array().is_none_or(|range| {
            range.len() != 2 || range[0].as_i64() != Some(1) || range[1].as_i64() != Some(1)
        })
    }) {
        attack
            .unsupported_fields
            .insert(String::from("attack_amount"));
    }
    for (field, expected) in [("spread_damage", false), ("grab", false)] {
        if fields
            .get(field)
            .is_some_and(|value| value.as_bool() != Some(expected))
        {
            attack.unsupported_fields.insert(field.to_owned());
        }
    }
    if fields
        .get("throw_strength")
        .is_some_and(|value| value.as_i64() != Some(0))
    {
        attack
            .unsupported_fields
            .insert(String::from("throw_strength"));
    }
    if attack.range == 0
        || attack.maximum_damage_multiplier_millionths < attack.minimum_damage_multiplier_millionths
        || attack.minimum_damage_multiplier_millionths < 0
        || (kind == MonsterSpecialAttackKind::Leap
            && (attack.leap_maximum_range_milli == 0
                || attack.leap_minimum_range_milli > attack.leap_maximum_range_milli
                || attack.leap_minimum_consider_range_milli
                    > attack.leap_maximum_consider_range_milli))
    {
        return Err(invalid(source, "special_attacks.bounds"));
    }
    Ok(attack)
}

fn parse_monster_effects(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<(Vec<MonsterAttackEffectDefinition>, BTreeSet<String>), MonsterRegistryError> {
    let values = value.as_array().ok_or_else(|| invalid(source, field))?;
    let mut effects = Vec::with_capacity(values.len());
    let mut deferred = BTreeSet::new();
    for value in values {
        let fields = value.as_object().ok_or_else(|| invalid(source, field))?;
        let effect_id = fields
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
            .ok_or_else(|| invalid(source, &format!("{field}.id")))?
            .to_owned();
        for key in fields.keys().filter(|key| !key.starts_with("//")) {
            if ![
                "id",
                "chance",
                "permanent",
                "affect_hit_bp",
                "bp",
                "duration",
                "intensity",
                "message",
            ]
            .contains(&key.as_str())
            {
                deferred.insert(format!("effects.{key}"));
            }
        }
        let chance = fields.get("chance").map_or(Ok(100.0), |value| {
            value
                .as_f64()
                .filter(|chance| chance.is_finite() && (0.0..=100.0).contains(chance))
                .ok_or_else(|| invalid(source, &format!("{field}.chance")))
        })?;
        effects.push(MonsterAttackEffectDefinition {
            effect_id,
            chance_millionths: (chance * 10_000.0).round() as u32,
            permanent: fields.get("permanent").map_or(Ok(false), |value| {
                value
                    .as_bool()
                    .ok_or_else(|| invalid(source, &format!("{field}.permanent")))
            })?,
            affect_hit_body_part: fields.get("affect_hit_bp").map_or(Ok(false), |value| {
                value
                    .as_bool()
                    .ok_or_else(|| invalid(source, &format!("{field}.affect_hit_bp")))
            })?,
            body_part_id: fields
                .get("bp")
                .map(|value| {
                    value
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| invalid(source, &format!("{field}.bp")))
                })
                .transpose()?,
            duration_turns: parse_attack_effect_range(
                fields.get("duration"),
                1,
                source,
                &format!("{field}.duration"),
            )?,
            intensity: parse_attack_effect_range(
                fields.get("intensity"),
                1,
                source,
                &format!("{field}.intensity"),
            )?,
        });
    }
    Ok((effects, deferred))
}

fn parse_u32(value: &Value, source: &str, field: &str) -> Result<u32, MonsterRegistryError> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| invalid(source, field))
}

fn parse_i32(value: &Value, source: &str, field: &str) -> Result<i32, MonsterRegistryError> {
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid(source, field))
}

fn parse_u32_scaled(
    value: &Value,
    scale: i32,
    source: &str,
    field: &str,
) -> Result<u32, MonsterRegistryError> {
    let scaled = parse_scaled_number(value, scale, source, field)?;
    u32::try_from(scaled).map_err(|_| invalid(source, field))
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
