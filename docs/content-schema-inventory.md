# Content schema inventory

`content-schema-inventory.json` is a deterministic inventory of every top-level
definition type and field observed in the pinned vendored JSON. Regenerate and
verify it with:

```sh
cargo xtask content-inventory
cargo xtask content-inventory-check
```

The current corpus contains 6,571 JSON files, 93,779 top-level objects, no
non-object top-level entries, and 180 definition types. `loader_implemented`
means the pinned fields have a strict Rust loader and validation path; it does
not by itself claim gameplay parity. `unimplemented` is intentionally explicit:
the loader must not silently discard those definitions or fields and report them
as supported.

MOD_INFO and ammunition-type loaders are currently implemented. The mod catalog
loads all 47 pinned records, resolves the upstream recommended default order,
and rejects missing/cyclic/obsolete/conflicting/unsafe selections. The default
server selection exposes 158 ammunition types.

The ITEM definition remains explicitly partial. Its registry finalizes all
10,282 concrete definitions and 179 abstracts in the default selection, supports
self-overrides and deferred `copy-from`, and applies direct/inherited common
identity, presentation, physical, material, flag, melee, ammunition, ranged,
phase, stackable, stack-size, static-quality, power-draw, light, and reversion fields. The inventory marks
those 46 fields `loader_implemented`; the other 239 observed
ITEM fields remain `unimplemented`, are retained per definition as unsupported,
and are not presented as gameplay parity. The inherited tool `sub` chain now
drives stable crafting subtype replacement; other subtype slots, references, and
runtime item behavior are subsequent gates. The added charge, comestible kind, calorie,
and quench fields feed the first authoritative consumption/needs loop; they do
not yet imply complete CDDA digestion, spoilage, health, or pocket semantics.
The 392 observed `tool_ammo` occurrences now resolve as an inherited scalar/list
set, and all 82 observed static `capacity` fields resolve as inherited bounded
integers. The loader now preserves every inherited `pocket_data` object, source
index, optional source ID, type, restrictions, default magazine, and raw field
map. Strict derived projections admit only single-category MAGAZINE and
selector-only MAGAZINE_WELL shapes; any extra behavior remains fail-closed and
the complete field remains explicitly unsupported. Runtime storage is no longer
selected by a battery-only magazine helper, and five additional reversible
detachable-tool targets can atomically detach their modeled magazine before
disassembly. Strict single-category MAGAZINE definitions now project their
canonical pocket index/ID, capacity, ammunition category, and inherited
`NO_RELOAD`/`NO_UNLOAD` access into item-backed runtime storage. Whole and
partial authoritative reloads retain or allocate nested stable IDs explicitly,
and detachable battery cells derive their energy from contained `battery`
items. This does not yet imply general CONTAINER pockets, speedloaders,
casings/ejection behavior, grids, UPS, or arbitrary power parity.
Inherited `power_draw` quantities normalize W/kW/mW to exact integer
milliwatts, while inherited `light` and `revert_to` remain
strict scalar projections. A deliberately narrow transform projection accepts
only `use_action` entries whose type is `transform`, retaining target,
`need_charges`, `ammo_scale`, and nonnegative move cost. Runtime admission currently proves only the
exact off/on flashlight pair (one activation charge, 1,560 mW, light 300); it
does not imply general use-action, transform, or lighting parity.
The ammunition sets, count, range, dispersion, damage, ranged-damage, and clip
size fields feed only the first revolver/ammunition loop. Nested ballistic
curves such as `damage.barrels`, magazines, pockets, recoil/aim, armor, and the
rest of CDDA gun behavior remain explicitly unsupported.
BOOK `required_level` feeds carried-book recipe knowledge. Inherited
`read_skill`, `max_level`, `intelligence`, and duration `time` also feed the
server's 197-entry timed theoretical-study catalog. Recreational books and
recipe-only BOOK items remain outside this initial skill-training path.

The lowercase `recipe` registry is strict and partial. Across the complete
vendored corpus the inventory observes 8,082 definitions and 73 fields: 33
fields are `loader_implemented` and 40 remain `unimplemented`. The default
selection resolves more than 5,000 concrete recipes through `copy-from`,
standalone requirements, replacement semantics, and explicit duration,
component, tool, quality, skill, autolearn, result, multiplier, byproduct, flag,
and unsupported-field state. Exactly 3,049 recipes currently have a complete
runtime path: 1,990 autolearn definitions plus 1,058 additional book-backed
definitions and one disassembly-learned definition, with no flag or the explicitly safe `BLIND_EASY` and
`ALLOW_ROTTEN`, whose latter semantics are exact because canonical items cannot
yet represent rot. Their
skill, ordinary or recursively expanded component-`LIST`, presence/aggregate-charge
tool, and inherent item quality requirements fit the implemented actor/item model.
Component `LIST` references use the pinned first same-kind group, compose integer
multipliers recursively, preserve encounter order, and collapse duplicate item
alternatives to their minimum count. Tool `LIST` references apply the same
first-group recursion and count rules before base-first, stable-ID-sorted ITEM
tool-subtype replacement. A quality's
non-unit speed remains a valid provider for these non-step recipes; pinned CDDA
uses that multiplier only for the unavailable step-recipe model. Charged
qualities and `charges_per_use` also load; each provider must individually hold
that threshold, and qualification does not itself consume charges. Direct and
extended recipe proficiency lists are normalized into mandatory gates or
optional deterministic time/training modifiers.
Root `using` replaces inherited external requirements and `extend.using`
appends to them before the same checked requirement expansion. Pinned
`ch_sheet_metal_small` therefore retains both its inherited blacksmithing
requirements and its carbon alternatives.
Legacy arrays and explicit logistic/linear `batch_time_factors` load, inherit,
and validate. The current one-unit craft command needs no wire field because
both pinned formulas return the ordinary recipe time for batch size one;
multi-item batch commands remain future runtime work.
Legacy pair arrays and explicit maps for `book_learn` load and inherit with
skill thresholds, alternate names, hidden flags, and strict BOOK references.
Inherited BOOK `required_level` supplies the fallback threshold. Identified
carried books now authorize a recipe at craft start when the actor meets its
effective theoretical primary-skill threshold; the server normalizes and
journals these knowledge alternatives. Concrete types are currently treated as
identified because identification state does not yet exist. Timed reading now
trains theoretical skill but does not permanently learn a pinned book recipe.
Scalar primary-skill and explicit skill-list `decomp_learn` metadata also loads,
inherits, and validates. Authoritative completed disassembly can permanently
learn eligible recipes, and the learned IDs are canonical and persistent.
Legacy `byproducts` now normalize into bounded, type-ID-sorted runtime outputs.
Ordinary byproducts retain their declared instance count; count-by-charge
byproducts use one instance with the pinned default stack size multiplied by
that count. Random `byproduct_group` definitions remain unavailable.
Parsing a recipe does not make it craftable.

The lowercase `skill` registry observes 37 corpus definitions and 17 fields.
Eight identity, display, ordering, tag, and focus fields are
`loader_implemented`; nine companion, level-description, attack-time, trait,
and comment fields remain `unimplemented`. The default selection strictly
loads all 28 skills. Stable IDs drive canonical practical/theoretical actor
levels, raw experience, autolearn checks, crafting eligibility, practice caps,
replication, and HUD presentation. Server-normalized physical-book study uses
pinned theoretical bounds, duration, and default-focus/default-intelligence XP
arithmetic. Focus/intelligence variation, recreational reading, helpers,
ebooks, identification discovery, general lightmaps, rust, contextual skills, combat
and other practice sources, and the remaining fields are not implemented.

The lowercase `proficiency` registry observes 320 corpus definitions and 16
fields. Twelve identity, display, category, learnability, default fixed-point
time/skill modifiers, training duration, prerequisite, teaching, focus, and
factory fields are `loader_implemented`; the four comment, bonuses, and
weakpoint fields remain `unimplemented`. The default selection strictly loads
234 definitions. Stable IDs drive mandatory craft gates, optional time maluses,
prerequisite-aware practice, persistence, replay, replication, private
inspection, events, and HUD presentation. Skill penalties are retained for the
future stochastic-failure boundary; combat weakpoints, teaching, focus effects,
and proficiency sources outside crafting are not implemented.

The lowercase `requirement` registry observes 734 corpus definitions and 19
fields. Seven fields are `loader_implemented`, 12 remain `unimplemented`, and
the default selection finalizes 474 IDs with upstream replacement and `extend`
behavior. Component/tool AND/OR groups, recursive `LIST` references, subtype
replacements, and `using` multipliers feed the runtime crafting slice. Presence
and aggregate-charge tool groups plus inherent and positive-per-use charged
quality groups now feed the runtime too. Tool pockets,
batteries, external power and UPS
semantics, explicit nondefault tool charge factors, zero-cost/externally powered
charged qualities, step-recipe quality speed, workspaces,
containers, randomized byproduct groups,
stochastic failure and recipe learning beyond autolearn and disassembly remain
unavailable behavior, with reasons retained per recipe.

The lowercase `construction` and `construction_group` registries separately
retain all 776 selected definitions and 438 groups. Identity/group/category,
duration, activity level, skill/component groups, terrain prerequisites,
special predicates/notes, and result IDs are parsed without treating them as
runtime support. The server catalog admits 55 complete definitions: 18
`LIGHT_EXERCISE` item placements on an empty adjacent tile with `check_empty`
and a furniture result, plus 36 colored-carpet terrain results with one exact
floor prerequisite and no special, plus one brick-oven terrain step with two
non-consuming qualities. Recursive component `LIST` references are expanded
through the same pinned requirement dictionary as recipes. Other predicates,
tools, charged consumption, broader terrain/result chains,
byproducts, deconstruction, and work
site/helper behavior remain fail-closed.

The lowercase `field_type` registry observes 238 corpus definitions across 15
files. It strictly resolves `copy-from` plus intensity-level name/symbol/color,
danger/transparency inheritance, priority, half-life, linear-decay, splatter,
and display fields, while retaining all other top-level keys as unsupported.
The first runtime admission is deliberately narrower: six creature-blood field
types support canonical placement, stable display order, once-per-second
linear or deterministic fixed-point exponential decay, visibility-filtered
replication, and Bevy presentation. Fire/fuel, gas spreading, contact effects,
underwater/outdoor acceleration, mopping, bashing, and the remaining field
processors are not implied by loader support.

The same loader keeps pinned `uncraft` records in their separate inheritance
dictionary: the default selection finalizes 1,428 concrete definitions and one
abstract. Explicit uncrafts override reversible crafts for the same target item;
1,227 currently satisfy the strict runtime disassembly boundary. Ordinary items
use the pinned first component alternative per group; reversible crafted items
override those defaults with their exact bounded retained component state. The
count includes three pinned bare ranged targets whose internal loads can be
unloaded through the strict ammunition registry before reservation, plus 75
powered-tool targets that are admitted only with exactly zero aggregate charges.
The remaining powered target, `flashlight`, instead detaches its exact installed
stable-ID `medium_battery_cell` before reservation.

The MONSTER definition is likewise explicit and partial. Its default-selection
registry finalizes 1,177 concrete definitions and 33 abstracts, including
deferred and self `copy-from`, direct values, collection extension/deletion,
and numeric relative/proportional modifiers. Twenty-five identity,
presentation, volume, health, speed, disposition, attack-cost, melee, dodge,
day/night vision, material, flag, and species fields are marked
`loader_implemented`; the other 87 observed MONSTER fields remain
`unimplemented` and are retained on each definition. The current runtime uses
the documented health, speed, aggression, ordinary-melee attack timing,
`SEES`, visual range, and volume-derived base-size subsets plus exact
material/flag blood-type derivation for admitted creature deaths and revival.

The lowercase `terrain` definition is also explicit and partial. The selected
registry finalizes 1,246 concrete definitions and 23 abstracts, validates all
loaded open/close references, and marks 13 identity, presentation, movement,
flag, and transform fields `loader_implemented`; 36 observed fields remain
`unimplemented` and retained. Runtime chunks currently use terrain identity,
move cost, transparency, finalized `FLAT`, and the simple door transform subset;
open/close targets carry all three behavioral values so construction predicates
remain exact after a transform.

The lowercase `furniture` definition follows the same fail-closed boundary. The
selected registry finalizes 699 concrete definitions and one abstract, validates
open/close references, and marks 18 identity, presentation, movement, comfort,
flag, and transform fields `loader_implemented`; 33 observed fields remain
`unimplemented` and retained. Runtime chunks currently use identity, movement
blocking, transparency, comfort, and floor-bedding warmth. Bash/deconstruction,
storage, examination actions, workbench behavior, fire, and moving furniture
remain unsupported behavior rather than implied parity.
