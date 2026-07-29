use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::{ContentManifest, ModCatalog, ModCatalogError, SelectedContentFile};

pub const MAX_ITEM_GROUP_LOCAL_DEPTH: usize = 32;
pub const MAX_ITEM_GROUP_REFERENCE_DEPTH: usize = 64;
pub const MAX_ITEM_GROUP_NODES: usize = 65_536;
pub const MAX_ITEM_GROUP_OUTPUT: u64 = 1_000_000;
pub const MAX_ITEM_GROUP_QUANTITY: u32 = 1_000_000;

const IMPLEMENTED_FIELDS: &[&str] = &[
    "type",
    "id",
    "subtype",
    "entries",
    "items",
    "groups",
    "ammo",
    "magazine",
    "copy-from",
    "extend",
];
const IMPLEMENTED_ENTRY_FIELDS: &[&str] = &[
    "item",
    "group",
    "collection",
    "distribution",
    "prob",
    "count",
    "charges",
];

pub(crate) fn field_is_implemented(field: &str) -> bool {
    IMPLEMENTED_FIELDS.contains(&field)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemGroupSubtype {
    Collection,
    Distribution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemGroupRange {
    pub minimum: u32,
    pub maximum: u32,
}

impl ItemGroupRange {
    pub const ONE: Self = Self {
        minimum: 1,
        maximum: 1,
    };
}

pub type ItemGroupNodeId = u32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemGroupNodeKind {
    Item(String),
    Group(String),
    Collection(Vec<ItemGroupNodeId>),
    Distribution(Vec<ItemGroupNodeId>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemGroupNode {
    pub kind: ItemGroupNodeKind,
    pub probability: u32,
    pub count: ItemGroupRange,
    pub charges: Option<ItemGroupRange>,
    pub unsupported_fields: BTreeMap<String, Value>,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemGroupDefinition {
    pub id: String,
    pub subtype: ItemGroupSubtype,
    pub ammo_chance: u8,
    pub magazine_chance: u8,
    pub roots: Vec<ItemGroupNodeId>,
    pub nodes: Vec<ItemGroupNode>,
    pub unsupported_fields: BTreeMap<String, Value>,
    pub source: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StrictItemGroupNodeKind {
    Item(String),
    Group(String),
    Collection(Vec<ItemGroupNodeId>),
    Distribution(Vec<ItemGroupNodeId>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictItemGroupNode {
    pub kind: StrictItemGroupNodeKind,
    pub probability: u32,
    pub count: ItemGroupRange,
    pub charges: Option<ItemGroupRange>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictItemGroupDefinition {
    pub id: String,
    pub subtype: ItemGroupSubtype,
    pub ammo_chance: u8,
    pub magazine_chance: u8,
    pub roots: Vec<ItemGroupNodeId>,
    pub nodes: Vec<StrictItemGroupNode>,
}

/// A strict root and all named groups reachable from it. Named roots are also
/// present in `groups`; inline roots are not.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrictItemGroupGraph {
    pub root: StrictItemGroupDefinition,
    pub groups: BTreeMap<String, StrictItemGroupDefinition>,
    pub maximum_output: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemGroupRegistry {
    groups: BTreeMap<String, ItemGroupDefinition>,
    concrete_items: BTreeSet<String>,
    migrations: BTreeMap<String, ItemMigration>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ItemMigration {
    replacement: String,
    variant: Option<String>,
}

impl ItemGroupRegistry {
    pub fn load_selected(
        manifest: &ContentManifest,
        content_root: impl AsRef<Path>,
        catalog: &ModCatalog,
        enabled: &[String],
    ) -> Result<Self, ItemGroupRegistryError> {
        let files = catalog
            .selected_json_files(manifest, enabled)
            .map_err(ItemGroupRegistryError::Catalog)?;
        Self::load_files(content_root.as_ref(), files)
    }

    fn load_files(
        content_root: &Path,
        files: Vec<SelectedContentFile>,
    ) -> Result<Self, ItemGroupRegistryError> {
        let mut registry = Self::with_builtins();
        for file in files {
            registry.load_file(content_root, &file)?;
        }
        registry.validate_group_graph()?;
        Ok(registry)
    }

    fn with_builtins() -> Self {
        let mut registry = Self::default();
        registry.groups.insert(
            String::from("EMPTY_GROUP"),
            ItemGroupDefinition {
                id: String::from("EMPTY_GROUP"),
                subtype: ItemGroupSubtype::Collection,
                ammo_chance: 0,
                magazine_chance: 0,
                roots: Vec::new(),
                nodes: Vec::new(),
                unsupported_fields: BTreeMap::new(),
                source: String::from("<builtin>"),
            },
        );
        registry
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&ItemGroupDefinition> {
        self.groups.get(id)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ItemGroupDefinition)> {
        self.groups
            .iter()
            .map(|(id, definition)| (id.as_str(), definition))
    }

    pub fn strict_graph(&self, id: &str) -> Result<StrictItemGroupGraph, ItemGroupRegistryError> {
        let root = self
            .groups
            .get(id)
            .ok_or_else(|| ItemGroupRegistryError::UnknownGroup(id.to_owned()))?;
        self.build_strict_graph(root, true)
    }

    /// Strictly normalizes an inline collection entry array with the same
    /// parser, reference checks, migration handling, and output bound as named
    /// groups. This is the integration point for inline bash/deconstruction
    /// drops.
    pub fn strict_inline_collection(
        &self,
        entries: &[Value],
        source: &str,
    ) -> Result<StrictItemGroupGraph, ItemGroupRegistryError> {
        let mut definition = ItemGroupDefinition {
            id: format!("<inline:{source}>"),
            subtype: ItemGroupSubtype::Collection,
            ammo_chance: 0,
            magazine_chance: 0,
            roots: Vec::new(),
            nodes: Vec::new(),
            unsupported_fields: BTreeMap::new(),
            source: source.to_owned(),
        };
        for (index, entry) in entries.iter().enumerate() {
            let location = format!("{source}#inline[{index}]");
            if let Some(node) = parse_object_entry(entry, &location, 0, &mut definition.nodes)? {
                definition.roots.push(node);
            }
        }
        self.build_strict_graph(&definition, false)
    }

    fn load_file(
        &mut self,
        content_root: &Path,
        file: &SelectedContentFile,
    ) -> Result<(), ItemGroupRegistryError> {
        let bytes = fs::read(content_root.join(&file.destination))
            .map_err(|error| ItemGroupRegistryError::Io(file.destination.clone(), error))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| ItemGroupRegistryError::Json(file.destination.clone(), error))?;
        match value {
            Value::Array(values) => {
                for value in values {
                    self.load_value(file, value)?;
                }
            }
            value => self.load_value(file, value)?,
        }
        Ok(())
    }

    fn load_value(
        &mut self,
        file: &SelectedContentFile,
        value: Value,
    ) -> Result<(), ItemGroupRegistryError> {
        let Some(kind) = value.get("type").and_then(Value::as_str) else {
            return Ok(());
        };
        match kind {
            "ITEM" => {
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    self.concrete_items.insert(id.to_owned());
                }
            }
            "MIGRATION" => {
                let object = value.as_object().ok_or_else(|| {
                    ItemGroupRegistryError::InvalidDefinition(file.upstream_path.clone())
                })?;
                let replacement = required_string(object, "replace", &file.upstream_path)?;
                let variant = optional_string(object, "variant", &file.upstream_path)?;
                let ids = string_or_strings(object.get("id"), &file.upstream_path, "id")?;
                for id in ids {
                    self.migrations.insert(
                        id,
                        ItemMigration {
                            replacement: replacement.to_owned(),
                            variant: variant.map(str::to_owned),
                        },
                    );
                }
            }
            "item_group" => {
                let object = value.as_object().ok_or_else(|| {
                    ItemGroupRegistryError::InvalidDefinition(file.upstream_path.clone())
                })?;
                self.load_group(object, &file.upstream_path)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn load_group(
        &mut self,
        object: &Map<String, Value>,
        source: &str,
    ) -> Result<(), ItemGroupRegistryError> {
        let id = required_string(object, "id", source)?;
        let copied_from = optional_string(object, "copy-from", source)?;
        let subtype_value = object
            .get("subtype")
            .cloned()
            .unwrap_or_else(|| Value::String(String::from("old")));
        let requested_subtype = parse_subtype(&subtype_value, source)?;
        let legacy_data = subtype_value.as_str() == Some("old");
        let requested_ammo = object
            .get("ammo")
            .map_or(Ok(0), |value| percentage(value, source, "ammo"))?;
        let requested_magazine = object
            .get("magazine")
            .map_or(Ok(0), |value| percentage(value, source, "magazine"))?;
        let mut definition = if let Some(parent) = copied_from {
            // The pinned item-group loader uses copy-from only to extend an
            // already loaded definition of the same ID (typically from an
            // earlier selected mod). It is not generic prototype inheritance.
            if parent != id {
                return Err(invalid(source, "copy-from"));
            }
            let definition = self.groups.get(parent).cloned().ok_or_else(|| {
                ItemGroupRegistryError::MissingCopyFrom {
                    id: id.to_owned(),
                    parent: parent.to_owned(),
                    source: source.to_owned(),
                }
            })?;
            if definition.subtype != requested_subtype {
                return Err(invalid(source, "subtype"));
            }
            definition
        } else {
            ItemGroupDefinition {
                id: id.to_owned(),
                subtype: requested_subtype,
                ammo_chance: requested_ammo,
                magazine_chance: requested_magazine,
                roots: Vec::new(),
                nodes: Vec::new(),
                unsupported_fields: BTreeMap::new(),
                source: source.to_owned(),
            }
        };
        definition.id = id.to_owned();
        definition.source = source.to_owned();
        retain_unsupported(
            object,
            IMPLEMENTED_FIELDS,
            &mut definition.unsupported_fields,
        );
        if let Some(value) = object.get("extend") {
            let extension = value.as_object().ok_or_else(|| invalid(source, "extend"))?;
            for (field, value) in extension {
                if !matches!(field.as_str(), "entries" | "items" | "groups")
                    && !field.starts_with("//")
                {
                    definition
                        .unsupported_fields
                        .insert(format!("extend.{field}"), value.clone());
                }
            }
            append_group_entries(extension, source, legacy_data, &mut definition)?;
        } else if copied_from.is_none() {
            // Pinned CDDA reads exactly one source: `extend` when present,
            // otherwise the top-level object only for a fresh definition.
            // A bare self copy is an identity operation.
            append_group_entries(object, source, legacy_data, &mut definition)?;
        }
        self.groups.insert(id.to_owned(), definition);
        Ok(())
    }

    fn validate_group_graph(&self) -> Result<(), ItemGroupRegistryError> {
        for definition in self.groups.values() {
            for target in definition.group_references() {
                if !self.groups.contains_key(target) {
                    return Err(ItemGroupRegistryError::MissingGroup {
                        group: definition.id.clone(),
                        target: target.to_owned(),
                    });
                }
            }
        }
        let mut states = BTreeMap::new();
        let mut path = Vec::new();
        for id in self.groups.keys() {
            self.visit_group(id, &mut states, &mut path, 0)?;
        }
        Ok(())
    }

    fn visit_group(
        &self,
        id: &str,
        states: &mut BTreeMap<String, VisitState>,
        path: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), ItemGroupRegistryError> {
        if depth > MAX_ITEM_GROUP_REFERENCE_DEPTH {
            return Err(ItemGroupRegistryError::ReferenceDepthExceeded(
                id.to_owned(),
            ));
        }
        match states.get(id) {
            Some(VisitState::Complete) => return Ok(()),
            Some(VisitState::Visiting) => {
                let start = path.iter().position(|entry| entry == id).unwrap_or(0);
                let mut cycle = path[start..].to_vec();
                cycle.push(id.to_owned());
                return Err(ItemGroupRegistryError::ReferenceCycle(cycle));
            }
            None => {}
        }
        states.insert(id.to_owned(), VisitState::Visiting);
        path.push(id.to_owned());
        let definition = self
            .groups
            .get(id)
            .ok_or_else(|| ItemGroupRegistryError::UnknownGroup(id.to_owned()))?;
        for target in definition.group_references() {
            self.visit_group(target, states, path, depth + 1)?;
        }
        path.pop();
        states.insert(id.to_owned(), VisitState::Complete);
        Ok(())
    }

    fn build_strict_graph(
        &self,
        root: &ItemGroupDefinition,
        named_root: bool,
    ) -> Result<StrictItemGroupGraph, ItemGroupRegistryError> {
        let mut reachable = BTreeSet::new();
        self.collect_strict_closure(root, &mut reachable)?;
        let mut strict_groups = BTreeMap::new();
        for id in &reachable {
            let definition = self
                .groups
                .get(id)
                .ok_or_else(|| ItemGroupRegistryError::UnknownGroup(id.clone()))?;
            strict_groups.insert(id.clone(), self.strict_definition(definition)?);
        }
        let strict_root = if named_root {
            strict_groups
                .get(&root.id)
                .cloned()
                .ok_or_else(|| ItemGroupRegistryError::UnknownGroup(root.id.clone()))?
        } else {
            self.strict_definition(root)?
        };
        let mut output_memo = BTreeMap::new();
        let maximum_output = self.maximum_definition_output(root, &mut output_memo)?;
        Ok(StrictItemGroupGraph {
            root: strict_root,
            groups: strict_groups,
            maximum_output,
        })
    }

    fn collect_strict_closure(
        &self,
        definition: &ItemGroupDefinition,
        reachable: &mut BTreeSet<String>,
    ) -> Result<(), ItemGroupRegistryError> {
        check_supported(definition)?;
        for target in definition.group_references() {
            if reachable.insert(target.to_owned()) {
                let child = self.groups.get(target).ok_or_else(|| {
                    ItemGroupRegistryError::MissingGroup {
                        group: definition.id.clone(),
                        target: target.to_owned(),
                    }
                })?;
                self.collect_strict_closure(child, reachable)?;
            }
        }
        if self.groups.contains_key(&definition.id) {
            reachable.insert(definition.id.clone());
        }
        Ok(())
    }

    fn strict_definition(
        &self,
        definition: &ItemGroupDefinition,
    ) -> Result<StrictItemGroupDefinition, ItemGroupRegistryError> {
        check_supported(definition)?;
        let mut nodes: Vec<StrictItemGroupNode> = definition
            .nodes
            .iter()
            .map(|node| {
                let kind = match &node.kind {
                    ItemGroupNodeKind::Item(item) => {
                        StrictItemGroupNodeKind::Item(self.resolve_item(item, &definition.id)?)
                    }
                    ItemGroupNodeKind::Group(group) => {
                        StrictItemGroupNodeKind::Group(group.clone())
                    }
                    ItemGroupNodeKind::Collection(children) => {
                        StrictItemGroupNodeKind::Collection(children.clone())
                    }
                    ItemGroupNodeKind::Distribution(children) => {
                        StrictItemGroupNodeKind::Distribution(children.clone())
                    }
                };
                Ok(StrictItemGroupNode {
                    kind,
                    probability: node.probability,
                    count: node.count,
                    charges: node.charges,
                })
            })
            .collect::<Result<_, ItemGroupRegistryError>>()?;
        normalize_probabilities(definition.subtype, &definition.roots, &mut nodes)?;
        Ok(StrictItemGroupDefinition {
            id: definition.id.clone(),
            subtype: definition.subtype,
            ammo_chance: definition.ammo_chance,
            magazine_chance: definition.magazine_chance,
            roots: definition.roots.clone(),
            nodes,
        })
    }

    fn resolve_item(&self, item: &str, group: &str) -> Result<String, ItemGroupRegistryError> {
        if self.concrete_items.contains(item) {
            return Ok(item.to_owned());
        }
        let mut current = item;
        let mut visited = BTreeSet::new();
        while let Some(migration) = self.migrations.get(current) {
            if !visited.insert(current.to_owned()) {
                return Err(ItemGroupRegistryError::MigrationCycle(item.to_owned()));
            }
            if migration.variant.is_some() {
                return Err(ItemGroupRegistryError::UnsupportedMigrationVariant {
                    group: group.to_owned(),
                    item: item.to_owned(),
                });
            }
            current = &migration.replacement;
            if self.concrete_items.contains(current) {
                return Ok(current.to_owned());
            }
        }
        Err(ItemGroupRegistryError::MissingItem {
            group: group.to_owned(),
            item: item.to_owned(),
        })
    }

    fn maximum_definition_output(
        &self,
        definition: &ItemGroupDefinition,
        memo: &mut BTreeMap<String, u64>,
    ) -> Result<u64, ItemGroupRegistryError> {
        if let Some(output) = memo.get(&definition.id) {
            return Ok(*output);
        }
        let values = definition
            .roots
            .iter()
            .map(|root| self.maximum_node_output(definition, *root, memo))
            .collect::<Result<Vec<_>, _>>()?;
        let output = combine_outputs(definition.subtype, &values, &definition.id)?;
        memo.insert(definition.id.clone(), output);
        Ok(output)
    }

    fn maximum_node_output(
        &self,
        definition: &ItemGroupDefinition,
        node_id: ItemGroupNodeId,
        memo: &mut BTreeMap<String, u64>,
    ) -> Result<u64, ItemGroupRegistryError> {
        let node = definition
            .nodes
            .get(usize::try_from(node_id).map_err(|_| ItemGroupRegistryError::NumericOverflow)?)
            .ok_or(ItemGroupRegistryError::InvalidNodeId(node_id))?;
        let one = match &node.kind {
            ItemGroupNodeKind::Item(_) => 1,
            ItemGroupNodeKind::Group(group) => {
                let child =
                    self.groups
                        .get(group)
                        .ok_or_else(|| ItemGroupRegistryError::MissingGroup {
                            group: definition.id.clone(),
                            target: group.clone(),
                        })?;
                self.maximum_definition_output(child, memo)?
            }
            ItemGroupNodeKind::Collection(children) => {
                let values = children
                    .iter()
                    .map(|child| self.maximum_node_output(definition, *child, memo))
                    .collect::<Result<Vec<_>, _>>()?;
                combine_outputs(ItemGroupSubtype::Collection, &values, &definition.id)?
            }
            ItemGroupNodeKind::Distribution(children) => {
                let values = children
                    .iter()
                    .map(|child| self.maximum_node_output(definition, *child, memo))
                    .collect::<Result<Vec<_>, _>>()?;
                combine_outputs(ItemGroupSubtype::Distribution, &values, &definition.id)?
            }
        };
        bounded_output(
            one.checked_mul(u64::from(node.count.maximum))
                .ok_or(ItemGroupRegistryError::NumericOverflow)?,
            &definition.id,
        )
    }
}

impl ItemGroupDefinition {
    fn group_references(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().filter_map(|node| match &node.kind {
            ItemGroupNodeKind::Group(group) => Some(group.as_str()),
            _ => None,
        })
    }
}

fn append_group_entries(
    object: &Map<String, Value>,
    source: &str,
    legacy: bool,
    definition: &mut ItemGroupDefinition,
) -> Result<(), ItemGroupRegistryError> {
    if legacy {
        if let Some(items) = object.get("items") {
            append_legacy_items(items, source, definition)?;
        }
        return Ok(());
    }
    if let Some(entries) = object.get("entries") {
        append_object_array(entries, source, "entries", definition)?;
    }
    if let Some(items) = object.get("items") {
        append_shortcut_array(items, source, "items", false, definition)?;
    }
    if let Some(groups) = object.get("groups") {
        append_shortcut_array(groups, source, "groups", true, definition)?;
    }
    Ok(())
}

fn append_legacy_items(
    value: &Value,
    source: &str,
    definition: &mut ItemGroupDefinition,
) -> Result<(), ItemGroupRegistryError> {
    let entries = value.as_array().ok_or_else(|| invalid(source, "items"))?;
    for (index, entry) in entries.iter().enumerate() {
        let location = format!("{source}#items[{index}]");
        let node = match entry {
            Value::Object(_) => parse_object_entry(entry, &location, 0, &mut definition.nodes)?,
            Value::Array(tuple) if tuple.len() == 2 => {
                let id = tuple[0]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| invalid(&location, "items"))?;
                match admissible_probability(&tuple[1], &location, "prob")? {
                    Some(probability) => Some(push_node(
                        &mut definition.nodes,
                        ItemGroupNode {
                            kind: ItemGroupNodeKind::Item(id.to_owned()),
                            probability,
                            count: ItemGroupRange::ONE,
                            charges: None,
                            unsupported_fields: BTreeMap::new(),
                            source: location,
                        },
                    )?),
                    None => None,
                }
            }
            _ => return Err(invalid(&location, "items")),
        };
        if let Some(node) = node {
            definition.roots.push(node);
        }
    }
    Ok(())
}

fn append_object_array(
    value: &Value,
    source: &str,
    field: &str,
    definition: &mut ItemGroupDefinition,
) -> Result<(), ItemGroupRegistryError> {
    let entries = value.as_array().ok_or_else(|| invalid(source, field))?;
    for (index, entry) in entries.iter().enumerate() {
        let location = format!("{source}#{field}[{index}]");
        if let Some(node) = parse_object_entry(entry, &location, 0, &mut definition.nodes)? {
            definition.roots.push(node);
        }
    }
    Ok(())
}

fn append_shortcut_array(
    value: &Value,
    source: &str,
    field: &str,
    group_shortcut: bool,
    definition: &mut ItemGroupDefinition,
) -> Result<(), ItemGroupRegistryError> {
    let entries = value.as_array().ok_or_else(|| invalid(source, field))?;
    for (index, entry) in entries.iter().enumerate() {
        let location = format!("{source}#{field}[{index}]");
        let node = match entry {
            Value::Object(_) => parse_object_entry(entry, &location, 0, &mut definition.nodes)?,
            Value::String(id) if !id.is_empty() => Some(push_node(
                &mut definition.nodes,
                ItemGroupNode {
                    kind: shortcut_kind(group_shortcut, id.clone()),
                    probability: 100,
                    count: ItemGroupRange::ONE,
                    charges: None,
                    unsupported_fields: BTreeMap::new(),
                    source: location,
                },
            )?),
            Value::Array(tuple) if tuple.len() == 2 => {
                let id = tuple[0]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| invalid(&location, field))?;
                match admissible_probability(&tuple[1], &location, "prob")? {
                    Some(probability) => Some(push_node(
                        &mut definition.nodes,
                        ItemGroupNode {
                            kind: shortcut_kind(group_shortcut, id.to_owned()),
                            probability,
                            count: ItemGroupRange::ONE,
                            charges: None,
                            unsupported_fields: BTreeMap::new(),
                            source: location,
                        },
                    )?),
                    None => None,
                }
            }
            _ => return Err(invalid(&location, field)),
        };
        if let Some(node) = node {
            definition.roots.push(node);
        }
    }
    Ok(())
}

fn shortcut_kind(group: bool, id: String) -> ItemGroupNodeKind {
    if group {
        ItemGroupNodeKind::Group(id)
    } else {
        ItemGroupNodeKind::Item(id)
    }
}

fn parse_object_entry(
    value: &Value,
    source: &str,
    depth: usize,
    nodes: &mut Vec<ItemGroupNode>,
) -> Result<Option<ItemGroupNodeId>, ItemGroupRegistryError> {
    if depth > MAX_ITEM_GROUP_LOCAL_DEPTH {
        return Err(ItemGroupRegistryError::LocalDepthExceeded(
            source.to_owned(),
        ));
    }
    let object = value
        .as_object()
        .ok_or_else(|| ItemGroupRegistryError::InvalidDefinition(source.to_owned()))?;
    let discriminators: Vec<_> = ["item", "group", "collection", "distribution"]
        .into_iter()
        .filter(|field| object.contains_key(*field))
        .collect();
    if discriminators.is_empty() && object.keys().all(|field| field.starts_with("//")) {
        return Ok(None);
    }
    if discriminators.len() != 1 {
        return Err(invalid(source, "item/group/collection/distribution"));
    }
    let probability = match object.get("prob") {
        None => 100,
        Some(value) => match admissible_probability(value, source, "prob")? {
            Some(probability) => probability,
            None => return Ok(None),
        },
    };
    let discriminator = discriminators[0];
    let kind = match discriminator {
        "item" => ItemGroupNodeKind::Item(required_string(object, "item", source)?.to_owned()),
        "group" => ItemGroupNodeKind::Group(required_string(object, "group", source)?.to_owned()),
        "collection" | "distribution" => {
            let entries = object[discriminator]
                .as_array()
                .ok_or_else(|| invalid(source, discriminator))?;
            let mut children = Vec::new();
            for (index, entry) in entries.iter().enumerate() {
                let location = format!("{source}.{discriminator}[{index}]");
                if let Some(child) = parse_object_entry(entry, &location, depth + 1, nodes)? {
                    children.push(child);
                }
            }
            if discriminator == "collection" {
                ItemGroupNodeKind::Collection(children)
            } else {
                ItemGroupNodeKind::Distribution(children)
            }
        }
        _ => return Err(invalid(source, discriminator)),
    };
    let nested_group = matches!(
        &kind,
        ItemGroupNodeKind::Collection(_) | ItemGroupNodeKind::Distribution(_)
    );
    let mut unsupported_fields = BTreeMap::new();
    retain_unsupported(object, IMPLEMENTED_ENTRY_FIELDS, &mut unsupported_fields);
    let (count, charges) = if nested_group {
        // Pinned add_entry returns immediately after building a local group;
        // leaf modifiers on that object are not evaluated. Strict admission
        // rejects their presence instead of executing divergent behavior.
        for field in ["count", "charges"] {
            if let Some(value) = object.get(field) {
                unsupported_fields.insert(field.to_owned(), value.clone());
            }
        }
        (ItemGroupRange::ONE, None)
    } else {
        (
            admissible_range(
                object.get("count"),
                source,
                "count",
                &mut unsupported_fields,
            )?
            .unwrap_or(ItemGroupRange::ONE),
            admissible_range(
                object.get("charges"),
                source,
                "charges",
                &mut unsupported_fields,
            )?,
        )
    };
    Ok(Some(push_node(
        nodes,
        ItemGroupNode {
            kind,
            probability,
            count,
            charges,
            unsupported_fields,
            source: source.to_owned(),
        },
    )?))
}

fn push_node(
    nodes: &mut Vec<ItemGroupNode>,
    node: ItemGroupNode,
) -> Result<ItemGroupNodeId, ItemGroupRegistryError> {
    if nodes.len() >= MAX_ITEM_GROUP_NODES {
        return Err(ItemGroupRegistryError::TooManyNodes);
    }
    let id = u32::try_from(nodes.len()).map_err(|_| ItemGroupRegistryError::NumericOverflow)?;
    nodes.push(node);
    Ok(id)
}

fn admissible_range(
    value: Option<&Value>,
    source: &str,
    field: &str,
    unsupported: &mut BTreeMap<String, Value>,
) -> Result<Option<ItemGroupRange>, ItemGroupRegistryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let pair = match value {
        Value::Number(number) => number.as_u64().map(|number| (number, number)),
        Value::Array(values) if values.len() == 2 => values[0].as_u64().zip(values[1].as_u64()),
        Value::Array(_) => None,
        _ => return Err(invalid(source, field)),
    };
    let Some((minimum, maximum)) = pair else {
        unsupported.insert(field.to_owned(), value.clone());
        return Ok(None);
    };
    let range = u32::try_from(minimum)
        .ok()
        .zip(u32::try_from(maximum).ok())
        .filter(|(minimum, maximum)| minimum <= maximum && *maximum <= MAX_ITEM_GROUP_QUANTITY);
    if let Some((minimum, maximum)) = range {
        Ok(Some(ItemGroupRange { minimum, maximum }))
    } else {
        unsupported.insert(field.to_owned(), value.clone());
        Ok(None)
    }
}

fn retain_unsupported(
    object: &Map<String, Value>,
    implemented: &[&str],
    target: &mut BTreeMap<String, Value>,
) {
    for (field, value) in object {
        if !field.starts_with("//") && !implemented.contains(&field.as_str()) {
            target.insert(field.clone(), value.clone());
        }
    }
}

fn check_supported(definition: &ItemGroupDefinition) -> Result<(), ItemGroupRegistryError> {
    if !definition.unsupported_fields.is_empty() {
        return Err(ItemGroupRegistryError::UnsupportedFields {
            group: definition.id.clone(),
            source: definition.source.clone(),
            fields: definition.unsupported_fields.keys().cloned().collect(),
        });
    }
    if let Some(node) = definition
        .nodes
        .iter()
        .find(|node| !node.unsupported_fields.is_empty())
    {
        return Err(ItemGroupRegistryError::UnsupportedFields {
            group: definition.id.clone(),
            source: node.source.clone(),
            fields: node.unsupported_fields.keys().cloned().collect(),
        });
    }
    Ok(())
}

fn parse_subtype(value: &Value, source: &str) -> Result<ItemGroupSubtype, ItemGroupRegistryError> {
    match value.as_str() {
        Some("collection") => Ok(ItemGroupSubtype::Collection),
        Some("distribution" | "old") => Ok(ItemGroupSubtype::Distribution),
        _ => Err(invalid(source, "subtype")),
    }
}

fn percentage(value: &Value, source: &str, field: &str) -> Result<u8, ItemGroupRegistryError> {
    let value = value
        .as_u64()
        .filter(|value| *value <= 100)
        .ok_or_else(|| invalid(source, field))?;
    u8::try_from(value).map_err(|_| invalid(source, field))
}

fn admissible_probability(
    value: &Value,
    source: &str,
    field: &str,
) -> Result<Option<u32>, ItemGroupRegistryError> {
    if value.as_i64().is_some_and(|value| value <= 0) {
        return Ok(None);
    }
    let value = value.as_u64().ok_or_else(|| invalid(source, field))?;
    u32::try_from(value)
        .map(Some)
        .map_err(|_| invalid(source, field))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<&'a str, ItemGroupRegistryError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(source, field))
}

fn optional_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    source: &str,
) -> Result<Option<&'a str>, ItemGroupRegistryError> {
    object
        .get(field)
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| invalid(source, field))
        })
        .transpose()
}

fn string_or_strings(
    value: Option<&Value>,
    source: &str,
    field: &str,
) -> Result<Vec<String>, ItemGroupRegistryError> {
    match value {
        Some(Value::String(value)) if !value.is_empty() => Ok(vec![value.clone()]),
        Some(Value::Array(values)) if !values.is_empty() => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(source, field))
            })
            .collect(),
        _ => Err(invalid(source, field)),
    }
}

fn combine_outputs(
    subtype: ItemGroupSubtype,
    outputs: &[u64],
    group: &str,
) -> Result<u64, ItemGroupRegistryError> {
    match subtype {
        ItemGroupSubtype::Collection => outputs.iter().try_fold(0_u64, |total, output| {
            bounded_output(
                total
                    .checked_add(*output)
                    .ok_or(ItemGroupRegistryError::NumericOverflow)?,
                group,
            )
        }),
        ItemGroupSubtype::Distribution => Ok(outputs.iter().copied().max().unwrap_or(0)),
    }
}

fn normalize_probabilities(
    subtype: ItemGroupSubtype,
    children: &[ItemGroupNodeId],
    nodes: &mut [StrictItemGroupNode],
) -> Result<(), ItemGroupRegistryError> {
    let mut distribution_total = 0_u32;
    for child in children {
        let index = usize::try_from(*child).map_err(|_| ItemGroupRegistryError::NumericOverflow)?;
        let node = nodes
            .get_mut(index)
            .ok_or(ItemGroupRegistryError::InvalidNodeId(*child))?;
        if subtype == ItemGroupSubtype::Collection {
            node.probability = node.probability.min(100);
        } else {
            distribution_total = distribution_total
                .checked_add(node.probability)
                .ok_or(ItemGroupRegistryError::NumericOverflow)?;
        }
        let nested = match &node.kind {
            StrictItemGroupNodeKind::Collection(children) => {
                Some((ItemGroupSubtype::Collection, children.clone()))
            }
            StrictItemGroupNodeKind::Distribution(children) => {
                Some((ItemGroupSubtype::Distribution, children.clone()))
            }
            StrictItemGroupNodeKind::Item(_) | StrictItemGroupNodeKind::Group(_) => None,
        };
        if let Some((nested_subtype, nested_children)) = nested {
            normalize_probabilities(nested_subtype, &nested_children, nodes)?;
        }
    }
    Ok(())
}

fn bounded_output(value: u64, group: &str) -> Result<u64, ItemGroupRegistryError> {
    if value > MAX_ITEM_GROUP_OUTPUT {
        Err(ItemGroupRegistryError::OutputBoundExceeded {
            group: group.to_owned(),
            output: value,
        })
    } else {
        Ok(value)
    }
}

fn invalid(source: &str, field: &str) -> ItemGroupRegistryError {
    ItemGroupRegistryError::InvalidField {
        source: source.to_owned(),
        field: field.to_owned(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Complete,
}

#[derive(Debug)]
pub enum ItemGroupRegistryError {
    Catalog(ModCatalogError),
    InvalidDefinition(String),
    InvalidField {
        source: String,
        field: String,
    },
    InvalidNodeId(ItemGroupNodeId),
    Io(String, std::io::Error),
    Json(String, serde_json::Error),
    LocalDepthExceeded(String),
    MigrationCycle(String),
    MissingCopyFrom {
        id: String,
        parent: String,
        source: String,
    },
    MissingGroup {
        group: String,
        target: String,
    },
    MissingItem {
        group: String,
        item: String,
    },
    NumericOverflow,
    OutputBoundExceeded {
        group: String,
        output: u64,
    },
    ReferenceCycle(Vec<String>),
    ReferenceDepthExceeded(String),
    TooManyNodes,
    UnknownGroup(String),
    UnsupportedFields {
        group: String,
        source: String,
        fields: Vec<String>,
    },
    UnsupportedMigrationVariant {
        group: String,
        item: String,
    },
}

impl fmt::Display for ItemGroupRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "item-group mod selection failed: {error}"),
            Self::InvalidDefinition(source) => {
                write!(formatter, "invalid item-group definition in {source}")
            }
            Self::InvalidField { source, field } => {
                write!(formatter, "invalid item-group field {field} in {source}")
            }
            Self::InvalidNodeId(id) => write!(formatter, "invalid item-group node ID {id}"),
            Self::Io(path, error) => write!(formatter, "item-group I/O failed for {path}: {error}"),
            Self::Json(path, error) => {
                write!(formatter, "item-group JSON failed for {path}: {error}")
            }
            Self::LocalDepthExceeded(source) => {
                write!(formatter, "item-group inline depth exceeded in {source}")
            }
            Self::MigrationCycle(item) => write!(formatter, "item migration cycle includes {item}"),
            Self::MissingCopyFrom { id, parent, source } => {
                write!(
                    formatter,
                    "item group {id} copies missing {parent} in {source}"
                )
            }
            Self::MissingGroup { group, target } => {
                write!(
                    formatter,
                    "item group {group} references missing group {target}"
                )
            }
            Self::MissingItem { group, item } => {
                write!(
                    formatter,
                    "item group {group} references missing ITEM {item}"
                )
            }
            Self::NumericOverflow => formatter.write_str("item-group numeric overflow"),
            Self::OutputBoundExceeded { group, output } => write!(
                formatter,
                "item group {group} can produce {output} objects, above the strict bound"
            ),
            Self::ReferenceCycle(cycle) => {
                write!(formatter, "item-group reference cycle: {cycle:?}")
            }
            Self::ReferenceDepthExceeded(group) => {
                write!(formatter, "item-group reference depth exceeded at {group}")
            }
            Self::TooManyNodes => formatter.write_str("item group has too many local nodes"),
            Self::UnknownGroup(group) => write!(formatter, "unknown item group {group}"),
            Self::UnsupportedFields {
                group,
                source,
                fields,
            } => write!(
                formatter,
                "item group {group} has unsupported fields {fields:?} in {source}"
            ),
            Self::UnsupportedMigrationVariant { group, item } => write!(
                formatter,
                "item group {group} uses migration variant for {item}"
            ),
        }
    }
}

impl std::error::Error for ItemGroupRegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn selected(path: &str) -> SelectedContentFile {
        SelectedContentFile {
            owner: "dda".to_owned(),
            upstream_path: path.to_owned(),
            destination: path.to_owned(),
        }
    }

    fn load_values(values: Value) -> Result<ItemGroupRegistry, ItemGroupRegistryError> {
        let mut registry = ItemGroupRegistry::with_builtins();
        let file = selected("synthetic.json");
        let values = values.as_array().expect("array fixture");
        for value in values {
            registry.load_value(&file, value.clone())?;
        }
        registry.validate_group_graph()?;
        Ok(registry)
    }

    #[test]
    fn normalizes_source_order_inline_nodes_ranges_and_old_distribution() {
        let registry = load_values(serde_json::json!([
            { "type": "ITEM", "id": "a" },
            { "type": "ITEM", "id": "b" },
            {
                "type": "item_group",
                "id": "mixed",
                "subtype": "distribution",
                "entries": [
                    { "collection": [ { "item": "a", "count": [0, 2] } ], "prob": 25 }
                ],
                "items": [ ["b", 7] ],
                "groups": []
            }
        ]))
        .expect("valid registry");
        let group = registry.get("mixed").expect("mixed group");
        assert_eq!(group.subtype, ItemGroupSubtype::Distribution);
        assert_eq!(group.roots, vec![1, 2]);
        assert!(matches!(
            group.nodes[1].kind,
            ItemGroupNodeKind::Collection(ref children) if children == &[0]
        ));
        assert_eq!(group.nodes[0].count.maximum, 2);
        assert_eq!(group.nodes[2].probability, 7);
        assert_eq!(
            registry
                .strict_graph("mixed")
                .expect("strict graph")
                .maximum_output,
            2
        );
    }

    #[test]
    fn legacy_old_groups_read_only_items_and_require_pair_shortcuts() {
        let registry = load_values(serde_json::json!([
            { "type": "ITEM", "id": "a" },
            { "type": "ITEM", "id": "b" },
            {
                "type": "item_group",
                "id": "legacy",
                "items": [["a", 7]],
                "entries": [{ "item": "b" }],
                "groups": ["missing_group"]
            }
        ]))
        .expect("legacy group should ignore modern top-level fields");
        let legacy = registry.get("legacy").expect("legacy group");
        assert_eq!(legacy.subtype, ItemGroupSubtype::Distribution);
        assert_eq!(legacy.roots.len(), 1);
        assert!(matches!(legacy.nodes[0].kind, ItemGroupNodeKind::Item(ref id) if id == "a"));
        assert_eq!(legacy.nodes[0].probability, 7);

        assert!(
            load_values(serde_json::json!([
                { "type": "ITEM", "id": "a" },
                { "type": "item_group", "id": "legacy", "items": ["a"] }
            ]))
            .is_err()
        );
    }

    #[test]
    fn reset_and_self_copy_extend_follow_selected_order() {
        let registry = load_values(serde_json::json!([
            { "type": "ITEM", "id": "a" },
            { "type": "ITEM", "id": "b" },
            { "type": "ITEM", "id": "c" },
            { "type": "item_group", "id": "g", "subtype": "collection", "items": ["a"] },
            { "type": "item_group", "id": "g", "subtype": "collection", "ammo": 7, "items": ["b"] },
            {
                "type": "item_group",
                "id": "g",
                "subtype": "collection",
                "copy-from": "g",
                "ammo": 99,
                "magazine": 99,
                "items": ["a"],
                "extend": { "items": [ ["c", 20] ] }
            },
            { "type": "item_group", "id": "g", "subtype": "collection", "copy-from": "g", "items": ["a"] },
            {
                "type": "item_group",
                "id": "h",
                "subtype": "collection",
                "items": ["a"],
                "extend": { "items": ["c"] }
            }
        ]))
        .expect("valid registry");
        let group = registry.get("g").expect("group");
        assert_eq!(group.nodes.len(), 2);
        assert_eq!(group.ammo_chance, 7);
        assert_eq!(group.magazine_chance, 0);
        assert!(matches!(group.nodes[0].kind, ItemGroupNodeKind::Item(ref id) if id == "b"));
        assert!(matches!(group.nodes[1].kind, ItemGroupNodeKind::Item(ref id) if id == "c"));
        let fresh_extend = registry.get("h").expect("fresh extended group");
        assert_eq!(fresh_extend.nodes.len(), 1);
        assert!(matches!(fresh_extend.nodes[0].kind, ItemGroupNodeKind::Item(ref id) if id == "c"));
    }

    #[test]
    fn copy_from_cannot_inherit_a_different_item_group_id() {
        let error = load_values(serde_json::json!([
            { "type": "ITEM", "id": "a" },
            { "type": "item_group", "id": "base", "subtype": "collection", "items": ["a"] },
            { "type": "item_group", "id": "derived", "copy-from": "base" }
        ]))
        .expect_err("cross-ID item-group copy-from is not pinned behavior");
        assert!(matches!(
            error,
            ItemGroupRegistryError::InvalidField { ref field, .. } if field == "copy-from"
        ));

        let mismatch = load_values(serde_json::json!([
            { "type": "ITEM", "id": "a" },
            { "type": "item_group", "id": "g", "subtype": "collection", "items": ["a"] },
            { "type": "item_group", "id": "g", "copy-from": "g", "extend": { "items": [["a", 1]] } }
        ]));
        assert!(matches!(
            mismatch,
            Err(ItemGroupRegistryError::InvalidField { ref field, .. }) if field == "subtype"
        ));
    }

    #[test]
    fn unsupported_ranges_and_fields_are_retained_and_fail_closed() {
        let registry = load_values(serde_json::json!([
            { "type": "ITEM", "id": "a" },
            {
                "type": "item_group",
                "id": "g",
                "subtype": "collection",
                "entries": [ { "item": "a", "count": [0, -1], "damage": [0, 2] } ]
            },
            {
                "type": "item_group",
                "id": "nested",
                "subtype": "collection",
                "entries": [ { "collection": [ { "item": "a" } ], "count": 2 } ]
            }
        ]))
        .expect("finalized registry");
        let node = &registry.get("g").expect("group").nodes[0];
        assert!(node.unsupported_fields.contains_key("count"));
        assert!(node.unsupported_fields.contains_key("damage"));
        assert!(matches!(
            registry.strict_graph("g"),
            Err(ItemGroupRegistryError::UnsupportedFields { .. })
        ));
        assert!(
            registry.get("nested").expect("nested group").nodes[1]
                .unsupported_fields
                .contains_key("count")
        );
        assert!(matches!(
            registry.strict_graph("nested"),
            Err(ItemGroupRegistryError::UnsupportedFields { .. })
        ));
    }

    #[test]
    fn nonpositive_probabilities_are_discarded_before_nested_nodes_are_built() {
        let registry = load_values(serde_json::json!([
            { "type": "ITEM", "id": "kept" },
            { "type": "ITEM", "id": "discarded" },
            {
                "type": "item_group",
                "id": "g",
                "subtype": "collection",
                "items": [
                    ["discarded", 0],
                    { "collection": [ { "item": "discarded" } ], "prob": -1 },
                    { "item": "kept", "prob": 101 }
                ]
            }
        ]))
        .expect("nonpositive entries are ignored by the pinned loader");
        let strict = registry
            .strict_graph("g")
            .expect("remaining group is strict");
        assert_eq!(strict.root.roots.len(), 1);
        assert_eq!(strict.root.nodes.len(), 1);
        assert_eq!(strict.root.nodes[0].probability, 100);
        assert!(matches!(
            strict.root.nodes[0].kind,
            StrictItemGroupNodeKind::Item(ref id) if id == "kept"
        ));
    }

    #[test]
    fn detects_missing_groups_and_reference_cycles() {
        let missing = load_values(serde_json::json!([
            { "type": "item_group", "id": "a", "subtype": "distribution", "entries": [ { "group": "b" } ] }
        ]));
        assert!(matches!(
            missing,
            Err(ItemGroupRegistryError::MissingGroup { .. })
        ));
        let cycle = load_values(serde_json::json!([
            { "type": "item_group", "id": "a", "subtype": "distribution", "entries": [ { "group": "b" } ] },
            { "type": "item_group", "id": "b", "subtype": "distribution", "entries": [ { "group": "a" } ] }
        ]));
        assert!(matches!(
            cycle,
            Err(ItemGroupRegistryError::ReferenceCycle(_))
        ));
    }

    #[test]
    fn inline_collection_uses_the_named_strict_closure_and_migrations() {
        let registry = load_values(serde_json::json!([
            { "type": "ITEM", "id": "new_item" },
            { "type": "MIGRATION", "id": "old_item", "replace": "new_item" },
            { "type": "item_group", "id": "named", "subtype": "collection", "items": ["old_item"] }
        ]))
        .expect("registry");
        let graph = registry
            .strict_inline_collection(
                &[serde_json::json!({ "group": "named", "count": [1, 3] })],
                "bash#drops",
            )
            .expect("strict inline group");
        assert_eq!(graph.maximum_output, 3);
        assert!(matches!(
            graph.groups["named"].nodes[0].kind,
            StrictItemGroupNodeKind::Item(ref id) if id == "new_item"
        ));
    }

    #[test]
    fn pinned_wall_bash_results_has_eight_strict_local_entries() {
        let mut registry = ItemGroupRegistry::default();
        registry
            .load_file(
                &workspace_root(),
                &selected("vendor/cdda/data/json/furniture_and_terrain/terrain-walls.json"),
            )
            .expect("pinned terrain walls");
        registry.validate_group_graph().expect("valid group graph");
        let group = registry.get("wall_bash_results").expect("wall drops");
        assert_eq!(group.subtype, ItemGroupSubtype::Collection);
        assert_eq!(group.roots.len(), 8);
        assert_eq!(group.nodes.len(), 8);
        assert_eq!(
            group.nodes[0].count,
            ItemGroupRange {
                minimum: 0,
                maximum: 2
            }
        );
        assert_eq!(
            group.nodes[2].charges,
            Some(ItemGroupRange {
                minimum: 4,
                maximum: 16
            })
        );
        assert_eq!(group.nodes[4].probability, 25);
        assert!(
            group
                .nodes
                .iter()
                .all(|node| node.unsupported_fields.is_empty())
        );
    }

    #[test]
    fn pinned_default_core_finalizes_wall_bash_results_strictly() {
        let workspace = workspace_root();
        let manifest = ContentManifest::load(workspace.join("vendor/cdda-content-manifest.json"))
            .expect("pinned content manifest");
        let files = manifest
            .entries
            .iter()
            .filter(|entry| {
                entry.destination.starts_with("cdda/data/json/")
                    && entry.destination.ends_with(".json")
            })
            .map(|entry| SelectedContentFile {
                owner: String::from("dda"),
                upstream_path: entry.upstream_path.clone(),
                destination: entry.destination.clone(),
            })
            .collect();
        let registry = ItemGroupRegistry::load_files(&workspace.join("vendor"), files)
            .expect("default-core item groups");
        let graph = registry
            .strict_graph("wall_bash_results")
            .expect("strict wall bash results");
        assert_eq!(graph.maximum_output, 82);
        assert_eq!(graph.root.roots.len(), 8);
    }
}
