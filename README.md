# Cataclysm: Dark Days Ahead — Rust Multiplayer Port

This repository contains a semantic Rust port of the pinned Cataclysm: Dark
Days Ahead baseline, redesigned as a persistent server-authoritative multiplayer
game. It is under active development and is not yet a playable CDDA release.

The Bevy graphical client is intentionally separate from the plain-Rust server.
Only `crates/client` may depend on Bevy. Canonical simulation, persistence,
network framing, protocol, content loading, tooling, and the server remain
renderer-free.

## Playable foundation slice

The current slice is not CDDA-complete, but it supports passwordless iroh
enrollment, persistent character creation/selection, two graphical clients,
authoritative movement, adjacent combat, and a first deterministic firearm loop,
stable ground/inventory items, pickup/drop/wield/reload/consume controls, a
pinned loaded revolver with compatible ammunition, pinned clean water and food with persistent
calorie/thirst/sleepiness survival state, voluntary and exhaustion sleep that
continues while disconnected, a pinned zombie that persistently hunts connected
or disconnected characters, leaves its pinned blood field and a carryable
corpse when killed, and can later revive from that corpse, a
pinned JSON mapgen that creates persistent 24x24 overmap cells on demand,
server-authoritative new-character placement through the pinned `sloc_lmoe`
start definition,
content-derived terrain and furniture state that participates in sight and
per-character memory, and a first content-derived
crafting loop. B opens the bounded recipe menu or resumes an interrupted
activity; X cancels and restores reserved ingredients. Crafting consumes its
real-time duration while connected or disconnected, remains vulnerable to
interruption, and survives snapshots/replay. The current validated catalog
exposes 3,049 recipes: 1,990 autolearn definitions, 1,058 additional definitions
whose complete runtime semantics can be authorized by an identified carried
book, and one further definition available through permanent disassembly
learning. It includes pinned `rock_sock`, `pointy_stick`,
vegetable juice with recursively expanded tomato/zucchini `LIST` alternatives,
the makeshift deck of cards with eight expanded drawing-tool alternatives, and
`toasterpastry_with_toaster`; carried presence tools, aggregate charged tools,
inherent ITEM qualities, and deterministic legacy `byproducts` are
authoritative. `ALLOW_ROTTEN` is exact over the current item model because no
canonical item can yet carry rot state; all other unsupported flag semantics
remain closed. Both pinned batch-time-factor forms load and validate; because
the client currently requests one craft at a time, each formula exactly reduces
to the ordinary recipe duration. Legacy and explicit `book_learn` metadata plus
inherited BOOK `required_level` normalize into server-owned knowledge
requirements. At craft start, a carried book authorizes its recipe only when
the actor's theoretical primary-skill level meets the pinned effective
threshold; the knowledge check neither reserves nor consumes the book. The
server also normalizes 197 pinned physical skill books into timed study
activities. V opens the carried-book picker or resumes an interrupted study,
and X cancels. Identified books use the actor's canonical Intelligence for the
pinned low-INT duration penalty, comprehension bounds, and stochastic XP
arithmetic under the current default-focus model with a server-owned
deterministic RNG. Study continues while
disconnected, requires daylight or the first supported local light, and interrupts on darkness, damage,
harmful needs, or exhaustion. It raises theoretical skill but does not
permanently learn a book recipe. The loader separately finalizes all 1,428
concrete core `uncraft` definitions, and the server exposes a strict catalog of
1,227 disassembly recipes. Explicit uncrafts override reversible crafts for the
same target, as in pinned CDDA. N opens the eligible carried-item picker
or resumes interrupted disassembly, and X cancels. The authoritative real-time
activity reserves the exact target and possible stable-ID outputs, continues
while disconnected, and interrupts on darkness, damage, harmful needs, or
exhaustion. Completion drops recovered components at the actor's current
position and may permanently learn recipes from scalar or explicit-list
`decomp_learn` metadata using server-owned deterministic recovery and learning
RNG streams. Learned recipe knowledge persists across restart and replay and is
usable from the Bevy craft menu. Damage levels 0 through 4 apply the pinned
100%/80%/64%/51.2%/40.96% recovery multiplier, and cancel restores the exact
condition. Completion also awards the pinned bounded primary-skill practice
under the current default-focus/default-stat character model. The strict initial
boundary uses the pinned first component alternative as the default for an
ordinary item. Reversible crafted outputs instead retain the exact components
actually consumed, including their charges, condition, item properties, and
bounded nested provenance; disassembly recovers those exact component objects
and filters ITEM-level `UNRECOVERABLE` entries. Recipe-local `NO_RECOVER` still
filters ordinary recipe defaults, matching the distinct pinned paths. Before
reserving a supported bare ranged target, the server unloads its exact internal
ammunition count as the pinned ammunition-category default item. The three
currently complete pinned targets are `coilgun`, `compositebow`, and
`compositecrossbow`; cancel restores the now-empty weapon and cannot duplicate
the already-dropped ammunition. The importer also resolves inherited
`tool_ammo` and static magazine `capacity`. Seventy-three powered-tool targets
whose storage is not yet modeled remain eligible only while their aggregate
charge count is exactly zero; the client hides charged instances and the server
rejects them without allocating IDs or consuming the target. The first strict
detachable-storage boundary models pinned `flashlight` plus
`medium_battery_cell`: the cell is an empty-capable 56-charge stable item,
reload atomically installs or swaps it, crafting spends its installed energy,
and disassembly drops that exact partially drained cell before reserving the
empty tool. Protocol 39 admits nine exact reversible battery-light
pairs whose finalized actions contain no unmodeled companion action. They use
the compatible light/ultralight or medium stable battery cells projected by
their pinned wells; `flashlight`, `wearable_light`, and `wearable_big_light`
also detach the exact cell before disassembly. P opens the activation picker.
Turning one on spends one whole cell charge; while active it draws its exact
content-derived millijoules per in-world second into canonical fractional cell
energy, keeps draining while its carrier is disconnected or after it is
dropped, and reverts to its exact inactive type when it cannot fund a full
second. Audited integer-only tables apply pinned open-air falloff: most admitted
lights saturate the 60-tile maximum, while `wizard_cane_cheap` luminance four
reaches three tiles. That cane supplies detail light through CDDA's personal
carried-light bonus but not when dropped; higher external sources use their
separate detail radius. All sources remain LOS-bounded for perception, terrain
memory, shooting, reading, and disassembly. All nine exact pairs carry upstream
`CHARGEDIM` and scale their base output linearly below one fifth of exact cell
energy, including canonical residual millijoules.
Zero energy stops emission immediately even if transform reversion waits for
the next whole-second boundary. Multi-action lights remain
unavailable until their other actions exist. The generalized
integral-charge unload path is snapshot/replay tested, but the pinned corpus has
no default-charged, non-pocket tool that can use it yet. Other detachable
magazines, weapon mods, pockets, recharge, charged/special tool
substitutions, containers, and construction outside the admitted strict subset
remain unavailable.

Protocol 41 provides the first construction loop. The strict loader retains 776
selected construction definitions and 438 groups; the server admits 55 complete
definitions: 18 item-to-empty-adjacent-tile furniture placements, 36
colored-carpet transformations with exact floor-terrain prerequisites, and one
brick-oven terrain step with server-derived AXE 2 plus CHISEL_WOOD 1 provider
requirements. Recursive component `LIST` references are expanded through the
pinned requirement dictionary before commands are journaled. Press M to choose an
available construction and visible target, M again to resume an interrupted
build, or X to cancel. Work is server-authoritative, consumes exact stable
components, continues while disconnected, remains vulnerable, persists through
schema-31 recovery/replay, and changes the chunk only on completion. Targets
must carry the finalized terrain `FLAT` flag required by upstream `check_empty`;
exact-prerequisite targets must retain the required current terrain ID. Terrain
results preserve the independent furniture layer. Far-away requests reject
before any chunk generation. The client
sends only a stable construction ID and target. Construction charge-consuming
tools, other specials, broader terrain chains, deconstruction, and helpers
remain fail-closed.

Protocol 49 adds the first canonical field slice. A strict `field_type` loader
retains pinned intensity visuals, priority, half-life, linear/exponential decay,
splatter, and display data. Chunks persist a sparse, type-ID-sorted field list
per tile with explicit equal-priority display order and age. The default zombie's
`WARM` plus `flesh` content resolves to `fd_blood`, so ordinary death adds one
blood splatter exactly as the pinned upstream death path does. Field aging runs
once per in-world second even with no players; an integer Q0.64 exponential
roll avoids platform `libm`, and schema-31 snapshots, crash recovery, and
portable replay reproduce it. Replication sends dynamic fields only for
currently visible tiles—never stale terrain memory—and Bevy renders the
highest-priority visible field from server-supplied pinned metadata. Fire,
smoke/gas spread, water-accelerated decay, field contact effects, mopping,
vehicles, and terrain/item field processors remain fail-closed.

Protocol 50/schema 32/CanonicalStateV30 add the first ordinary monster-corpse
and revival slice. A fatal non-pulverizing hit replaces the creature with a
stable-ID `corpse` item whose self-contained prototype, death tick, damage, and
revival state persist on the ground or in an actor inventory. The pinned
overkill-to-corpse-damage boundary, damage-slowed effective age, six-hour
minimum check, age-weighted revival roll, one-in-20 special corpse flag, 80%
speed, 70% HP, damage divisor, and
five-second downed state follow the pinned upstream behavior. A carried corpse
revives on the carrier's tile when free or on a deterministic adjacent tile,
removing the exact item and clearing its wielded slot. The multiplayer
adaptation tests special revival against the nearest living actor within three
tiles rather than a process-global avatar. Revival runs once per in-world
second with no connected players and reproduces through snapshots, SQLite
recovery, and portable replay. Death drops, worn-item transfer, butchery,
pulping, burning, rot, and gibs remain fail-closed.

Protocol 51/schema 33/CanonicalStateV31 add the first content-derived monster
vision boundary. The strict MONSTER loader now resolves inherited `vision_day`
and `vision_night`; the fresh-world zombie carries pinned ranges 40 and 3 plus
its `SEES` flag in live state and in every corpse prototype. A hostile acquires
only a living actor it can currently see, choosing distance then stable actor
ID. Opaque terrain or furniture blocks acquisition. Natural vision uses the
existing deterministic two-to-60-tile solar/lunar light abstraction to
interpolate between the type's night and day endpoints; a modeled powered light
on the target enables the type's maximum endpoint but never bypasses LOS or its
maximum range. This is an explicit multiplayer rule for all actors rather than
upstream's singleton-avatar visibility special case. Hearing, scent, target
memory, camouflage, effects, bashing, and pathfinding around obstacles remain
fail-closed.

Protocol 52/schema 34/CanonicalStateV32 make creature movement time depend on
the canonical source and destination tiles. Creatures now carry signed action
debt: an ordinary floor step costs the pinned 100 moves, while furniture adds
its nonnegative movement modifier before the same 20-action-point scaling used
for actors. A ready creature moves immediately and then recovers any extra debt,
which remains stable across snapshots, SQLite recovery, and replay. The current
boundary chooses only an unobstructed cardinal step toward a visible target;
Protocol 53 expands that selection. Bashing, route settings, movement-mode
flags, and fields remain fail-closed.

Protocol 53/schema 35/CanonicalStateV33 add the pinned direct-pursuit candidate
fan and `STUMBLES` behavior. Non-stumbling creatures choose the first passable
positive-progress square from `squares_closer_to`; the default zombie instead
uses a named deterministic stream to apply the pinned Euclidean
progress-weighted replacement rule. Q30 integer square roots replace canonical
floating point. With default circular distance, the same progress scales move
cost: a direct floor diagonal costs 142 moves, while a tested flanking stumble
costs 88. The flag persists in live creatures and corpse prototypes through
SQLite and portable replay. Bashing, path settings, sound, scent, remembered
targets, danger avoidance, and special movement modes remain fail-closed.

Protocol 54/schema 36/CanonicalStateV34 make a creature's exact last-seen
destination canonical. Seeing a living actor refreshes that goal; losing line
of sight continues pursuit toward the remembered tile, and arrival clears it.
The goal survives strict snapshots, SQLite recovery, and portable replay. It is
server-private: client replication now uses a separate visible-creature DTO
containing only stable ID, type, position, and public HP, structurally excluding
AI intent, action debt, combat attributes, blood, and corpse reconstruction
state. Sound and scent goals, route planning, bashing, danger avoidance, and
special movement modes remain fail-closed.

Protocol 55/schema 37/CanonicalStateV35 add the first structural-bashing
boundary. The strict content layer resolves the pinned default and wooden-door
damage profiles plus inherited terrain/furniture bash metadata. The default
zombie retains `BASHES` and `GROUP_BASH` in live state and corpse prototypes.
When pursuit has no better passable step, connected group bashers contribute
through the pinned five-row, three-wide formation; exact fixed-point profile
damage accumulates on the target tile until `t_door_c` becomes `t_door_b` and
then `t_door_frame`. Each attempt spends the pinned 20 moves per point of
creature speed and records deterministic structural sound. Successful stages
atomically preflight stable IDs, transform terrain, create exact direct-item
debris, and apply pinned hit/destroyed fields. Map damage, bash catalogs,
capabilities, drops, and fields survive strict snapshots, SQLite recovery, and
portable replay, while client replication still exposes only the public
creature DTO. Generic furniture bashing, `t_door_frame`'s `t_null` floor
replacement, route planning, sound propagation/hearing, collapse, item groups,
and unsupported bash side effects remain fail-closed.

Protocol 56/schema 38/CanonicalStateV36/CanonicalEventsV9 add the first
authoritative monster-hearing boundary. The strict ITEM importer now resolves
`loudness`; the pinned `sw_619` plus `38_special` starter shot derives volume
70 and emits canonical origin, `bang!`, and volume. Live creatures and corpse
prototypes preserve `HEARS` and `GOODHEARING`. Same-z gunshots and structural
bash attempts are processed in stable event/creature order using Chebyshev
distance, pinned normal versus good-hearing perception, an integer-only named
random interest threshold, imprecision radius, replacement rule, and
action-counted pursuit lifetime. Current sight, persisted visual memory, then
private sound intent form the explicit target priority. Sound goals survive
strict snapshots, SQLite recovery, and portable replay but remain absent from
the public creature DTO. Client audible observations, player hearing, z-level
propagation, weather/obstacle attenuation, sound clustering, and sound markers
remain fail-closed.

Protocol 57/schema 39/CanonicalStateV37/CanonicalEventsV10 add the first
content-derived monster door-opening boundary. `CAN_OPEN_DOORS` persists in
live creatures and corpse/revival prototypes; the default zombie lacks it,
while the pinned feral-human family supplies it. A capable creature may select
an admitted terrain `open` transform before bashing, emits canonical `swish` at
volume 6, pays the pinned zero move cost for opening, and can immediately enter
while readiness remains. The mutation clears incompatible structural damage,
updates terrain memory/replication through chunk revision, drives private
monster hearing after the creature phase, and reproduces through SQLite and
portable replay. Incapable creatures remain blocked. Interior-only transforms
stay fail-closed until indoor/outdoor topology exists, and furniture/vehicle
doors, unlocking, pacification/effect restrictions, route planning, closing,
and client audible observations remain unavailable.

Protocol 58/schema 40/CanonicalStateV38 add the first content-derived monster
obstacle-routing boundary without changing CanonicalEventsV10. The strict
MONSTER loader resolves the complete observed `path_settings` object. A zero
`max_dist` keeps the existing direct greedy movement; a positive value enables
deterministic same-z A* over already loaded canonical terrain, using the pinned
eight-neighbor order, 16-tile padding, five-times-distance cost budget,
terrain/furniture costs, diagonal penalty, door cost, and dangerous-field
penalty. A routed adjacent step may temporarily make no progress toward the
ultimate target, allowing capable creatures to round modeled walls and plan
through admitted terrain doors. Settings persist through corpses, revival,
SQLite recovery, and portable replay but remain private from clients. The
default zombie keeps distance zero; pinned feral humans provide distance 45 and
route-open doors. Route caching/backoff, route-planned structural bashing,
stairs/z-levels, traps, sharp terrain, creature danger avoidance, and spawning
ferals with their unported special attacks remain fail-closed.

Protocol 59/schema 41/CanonicalStateV39/CanonicalEventsV11 add the first
content-derived furniture-destruction boundary and finish the modeled wooden
door chain. Bash selection now follows upstream layer precedence: a registered
bashable furniture tile is damaged and replaced before its underlying terrain.
The then-current pinned cabin dresser used the exact default bash profile, direct drops,
dust/splinter fields, sounds, persistent damage, and `f_null` removal while the
floor remains intact. The existing damaged/closed wooden-door stages now
continue through `t_door_frame`; for the current known z=0 cabin topology, its
dynamic `t_null` result is explicitly resolved to `t_floor`. Canonical bash
events identify whether the target was furniture or terrain and name its exact
content ID. Both layers reproduce after snapshot restoration and replay.
Other furniture definitions, dynamic roof/floor repair outside the admitted
then-current cabin topology, tents, collapse, explosions, item groups, and bash side effects
remain fail-closed.

Protocol 60/schema 42/CanonicalStateV40 broadens the same furniture boundary
without changing CanonicalEventsV11. The server derives and registers all 537
of 699 pinned furniture definitions that satisfy an explicit runtime predicate:
fully represented ordinary/blocked strength bounds, no supported-floor variant,
resolved `t_null` terrain behavior, modeled dust/splinter fields, bounded direct
drops, bounded sound, and no tent, collapse, explosion, item-group, or other
unsupported side effect. Admission is transitively closed over replacement
targets, so a safe first stage cannot lead to an unsupported bash stage. The
set included every furniture tile placed in the former fresh cabin (`f_bed`,
`f_chair`, `f_dresser`, and `f_table`) and supports both `f_null` removal and
explicit furniture-to-furniture replacement without changing underlying
terrain. Pinned tests lock the 537-definition count, validate every canonical
entry, restore the registry snapshot, and cap its encoded size at 128 KiB.

Protocol 61/schema 43/CanonicalStateV41 adds route-planned structural and
furniture bashing without changing CanonicalEventsV11. A positive-distance
basher's A* search now considers registered bash targets after ordinary
movement and door opening, using upstream's base monster bash skill and exact
cost classes: `(20 / rating) + 12` for ratings above one, 500 for a desperate
rating of one, and impassable for rating zero. Like upstream rating, planning
uses ordinary unblocked strength bounds; the actual hit still applies modeled
blocked bounds. Group-bash helpers remain an actual-hit concern and are
deliberately excluded from route estimates, matching the pinned finalized path
settings. Focused tests prove a strong creature takes the shorter destructive
route while a desperate group basher prefers a long walkable detour; a separate
sideways-first route proves identical SQLite recovery and portable replay. The
default zombie still has `max_dist` zero and uses its existing greedy bashing
behavior. Route caching/backoff, vertical, trap/sharp, nearby-creature hazard,
vehicle, and unsupported bash-side-effect paths remain fail-closed.

Protocol 62/schema 44/CanonicalStateV42/CanonicalEventsV12 adds the first
player-controlled structural-smashing boundary. H opens a bounded picker for
all currently visible bashable tiles in the eight horizontal adjacent
directions. Bashability and the selected furniture-or-terrain layer are
server-supplied observation metadata; furniture retains upstream precedence,
and remembered hidden tiles never disclose the live interaction. Canonical
state also records every pinned furniture ID with an upstream bash body: if
that body is outside the admitted strict subset, the opaque furniture layer
blocks the underlying terrain instead of allowing an invented pass-through. The
authoritative `Smash` command revalidates adjacency and the current registered
target. A wielded bash-only item uses its exact pinned bash damage plus the
current default arm Strength 8; the hammer therefore supplies strength 17.
Unarmed, mixed-damage, fractional-bash, and damage-level-above-one tools fail
closed until anatomy and the complete item/profile adjustments are canonical.
Attempts share the existing
standard 100-move actor action boundary, accumulated damage, atomic drops and
fields, sound/hearing, and deterministic RNG with monster bashing. Typed actor
bash events, diagonal furniture-before-terrain destruction, SQLite recovery,
and portable replay are verified. Weapon-specific attack time, wear, stamina,
practice, faction ownership, corpses, vehicles, and field-first smashing remain
separate boundaries.

Protocol 63/schema 45/CanonicalStateV43 retains CanonicalEventsV12 and makes
the first player-smashing subset use canonical character and item timing data.
Actors persist base Strength; the current healthy-limb multiplier is exactly
one, so the default 8 plus the pinned hammer's bash 9 still yields strength 17.
A sorted authoritative item profile admits only ordinary integer-bash-only
types whose instance damage exactly matches pinned content. Guns, charge-bearing
types, and types with ammunition, magazine, or powered state remain fail-closed
because their live weight or melee damage can differ from the base type. Pinned
`item::attack_time` uses finalized weight and volume; the hammer's 566 g and
320 ml produce 79 moves, and the upstream 80% smash multiplier truncates that
to 63 moves instead of the earlier temporary 100. Base Strength, strict item
profiles, signed readiness debt, SQLite recovery, portable replay, private
inspection, replication, and the Bevy HUD are covered. Limb damage,
enchantments, count-by-charge weapon rounding, weapon faults/mods, unarmed
anatomy, wear, stamina, practice, and other damage profiles remain fail-closed.

Protocol 64/schema 46/CanonicalStateV44 retains CanonicalEventsV12 and makes all
four bounded base stats canonical. New survivors currently use pinned defaults
of STR 8, DEX 8, INT 8, and PER 8. The server stores each book's unadjusted pinned time and Intelligence
requirement, then uses the reader's canonical INT for the exact low-INT duration
penalty and pinned comprehension range. Disassembly practice uses canonical INT
and PER for pinned catch-up and knowledge modifiers through checked rational
integer arithmetic, including the minimum-one multiplier and 90% theory cap.
Snapshots, hashes, schema recovery, replay, replication, private inspection,
the operator output, and the Bevy HUD carry all four stats. Protocol 66 supplies
DEX's first gameplay consumer; non-default focus, traits, enchantments, and
non-stat generation choices remain explicit later boundaries.

Protocol 65 retains schema 46/CanonicalStateV44/CanonicalEventsV12 and adds the
pinned baseline's current freeform stat selection to character creation. Each
new survivor independently selects STR, DEX, INT, and PER from 4 through 20;
all four default to 8. This is deliberately not a legacy point-pool system:
the pinned creator uses `FREEFORM`, while its old point-pool paths are inactive.
The bounded values cross the authenticated iroh request, are independently
validated by protocol and simulation before actor-ID allocation, and are stored
unchanged by the existing crash-reconciled character transaction. The Bevy
creator uses Up/Down to select a stat and Left/Right to adjust it; the
`--character` automation shortcut keeps all four defaults. Scenarios,
professions, traits, skills, appearance, and other creation choices remain
future boundaries.

Protocol 66 retains schema 46/CanonicalStateV44/CanonicalEventsV12 and applies
the pinned melee attack-speed formula to the first exact player subset. The
null/unarmed item has attack time 65 moves. Ordinary admitted bash weapons use
their already-canonical pinned item attack time. Simulation halves that time,
adds `floor(base * (15 - practical melee skill) / 15)`, subtracts
`floor(effective DEX / 2)`, with effective base stats capped at the pinned 20,
and clamps the result to 25 moves before converting it to
action points. A default unarmed survivor therefore pays 60 moves, while the
Strength-8/DEX-8 hammer wielder still smashes in 63 moves but now makes an
ordinary melee attack in 74. Connected commands and disconnected defensive
attacks share the same cost, which survives SQLite recovery and portable
replay. Live-weight guns, ammunition/magazines, powered items, mixed-damage
weapons, deeply damaged instances, stamina, limb balance/lift, martial arts,
enchantments, and other attack-speed modifiers retain the temporary 100-move
melee boundary.

Protocol 67/schema 47/CanonicalStateV45 retains CanonicalEventsV12 and imports
the final inherited MONSTER `melee_skill` and `dodge` values into private
authoritative creature state. The pinned classic zombie has melee skill 4 and
dodge 0. Both fields are copied into self-contained corpse prototypes and back
into revived creatures, participate in the canonical hash, and survive SQLite
recovery and portable replay. Public visible-creature snapshots deliberately
omit them. This is the exact prerequisite for player-versus-monster hit/dodge;
Protocol 67 does not yet change whether an attack lands.

Protocol 68 retains schema 47 and CanonicalStateV45 while advancing the event
hash domain to CanonicalEventsV13. It implements the first exact player
hit/dodge subset: an empty-handed actor attacking the pinned medium classic
zombie. Accuracy follows pinned `DEX/4 + practical melee/2 - 2`; monster dodge
is `dodge * 5`; and a hit requires the resulting spread to be nonnegative. The
server adapts the pinned normal-roll shape to the same cross-platform
12-uniform fixed-point sampler already used by deterministic sound perception,
because C++ `std::normal_distribution` has no portable bit-exact output. A
named session stream binds the roll to world seed, source actor, target
creature, and accepted command sequence, so queue delay, restart, and portable
replay cannot reroll it.

A miss emits a typed source-private event, spends the same Protocol 66 attack
time, deals no damage, and allocates no corpse ID. Disconnected trapped defense
uses the same rule and action cost. Armed attacks, other monster types or sizes,
monster attacks, criticals, techniques, and the remaining accuracy modifiers
keep the previous guaranteed-hit behavior until their inputs are canonical;
the implementation does not present that boundary as general melee parity.

Protocol 69 advances to schema 48 and CanonicalStateV46 while retaining
CanonicalEventsV13. The strict ITEM loader now finalizes the pinned legacy
integer and modern grip/length/surface/balance `to_hit` forms, including
inherited relative adjustments. The already-canonical ordinary bash-only
weapon profile stores that value; the pinned hammer resolves to -1.

These strict weapons join the exact classic-zombie hit/dodge path. Their pinned
accuracy is `DEX/4 + dominant bashing/3 + practical melee/2 + item to_hit`.
Bashing contributes only when bash damage exceeds upstream `MELEE_STAT` 5;
weaker objects use the null dominant skill. The named roll, monster dodge,
hit-on-zero rule, miss event, and attack timing are identical for connected
commands and disconnected trapped defense. Profile state and outcomes survive
SQLite recovery and portable replay. Mixed or fractional damage, degraded,
ranged, ammunition/magazine, powered, and otherwise unregistered weapons remain
outside the exact hit path, as do other monster types/sizes and monster attack
rolls.

Protocol 70 advances to schema 49, CanonicalStateV47, and CanonicalEventsV14
for the first exact monster-side hit boundary. A pinned `mon_zombie` attacking
a sleeping actor rolls `normal_roll(melee_skill * 5, 25)` against the exact
zero dodge produced by upstream's sleeping `can_try_dodge` failure. A negative
roll spends the normal 100-move monster attack, deals no damage, and does not
wake or interrupt the actor. Other monster types and awake targets keep the
explicit guaranteed-hit boundary until their canonical defense inputs exist.

The strict MONSTER `CLUMSY_ATTACKS` flag is private canonical creature and
corpse state. On any admitted miss it consumes the next deterministic
one-in-four roll and, on success, applies the pinned two-second down state,
preventing further same-tick actions. `CreatureMissedActor` makes the outcome
replay-verifiable, but the server does not replicate it because pinned CDDA
suppresses miss and fall messages while the target sleeps. Clumsiness, the
event hash, two-second expiry, corpse revival, SQLite recovery, and portable
replay are covered without exposing the capability in public creature DTOs.

Protocol 71 retains schema 49, CanonicalStateV47, and CanonicalEventsV14 while
removing the temporary classic-zombie type restriction. Every canonical
ordinary creature with at least one melee damage die now uses its finalized
MONSTER `melee_skill` against a sleeping actor's zero dodge; the fixed actor
size is medium and therefore contributes no size penalty. Type and attacker
size do not otherwise enter pinned ordinary hit resolution. Zero-dice monsters
retain upstream's early no-hit return, and awake actors remain on the
guaranteed-hit boundary pending canonical defense state. Named rolls, clumsy
falls, non-replication, action cost, damage, wake behavior, recovery, and replay
remain those established by Protocol 70.

Protocol 72 advances to schema 50 and CanonicalStateV48 while retaining
CanonicalEventsV14. The strict MONSTER importer now finalizes inherited
`volume` with the pinned 62,499 ml default, signed-integer ml/L grammar,
relative addition, and truncating proportional multiplication. Fresh runtime
creatures derive the exact tiny/small/medium/large/huge base-size class at the
pinned 7,500/46,250/108,000/483,750 ml thresholds. Private live state,
self-contained corpse prototypes, revival, snapshots, SQLite recovery, and
portable replay retain that immutable class; public creature DTOs do not expose
it. Empty-hand and admitted strict bash-weapon attacks now apply pinned target
size penalties 30/15/0/-10/-20 for every canonical monster type. Tests lock all
thresholds and modifiers, a same-stream medium-hit/tiny-miss transition, an
arbitrary non-zombie target, canonical hashing/restoration, and a huge-target
miss through recovery and replay.

Protocol 73 advances to schema 51 and CanonicalStateV49 while retaining
CanonicalEventsV14. Final inherited MONSTER `IMMOBILE` now becomes a private
canonical live/corpse capability. An immobile creature still performs the
currently modeled perception/goal bookkeeping, then clears all accrued moves
before ordinary adjacent melee, door opening, bashing, or movement, matching
the pinned action order after its still-unimplemented special-attack phase.
Player empty-hand and admitted strict-bash attacks add the pinned 40-point hit
spread bonus against that target after dodge and size. Snapshot restore,
corpse revival, schema-51 SQLite recovery, and portable replay retain the
capability without expanding the public creature DTO. Dynamic `CANNOT_MOVE`
effects and `RIDEABLE_MECH` remain outside the canonical runtime rather than
being conflated with static `IMMOBILE`.

Protocol 74 advances to schema 52 and CanonicalStateV50 while retaining
CanonicalEventsV14. Final inherited MONSTER `PACIFIST` is now private
canonical live/corpse state. It suppresses only the pinned ordinary
`attack_at` melee branch: pacifist monsters still perceive targets, move,
open, and bash, and upstream special attacks remain a separate future slice.
Differential tests prove a pacifist closes distance normally but causes no
adjacent damage while an otherwise identical attacker hits. Hashing, snapshot
restore, corpse revival, schema-52 SQLite recovery, and portable replay retain
the capability without exposing it in visible creature DTOs. Dynamic
`CANNOT_ATTACK` remains part of the future effect model.

Protocol 75 advances to schema 53 and CanonicalStateV51 while retaining
CanonicalEventsV14. The strict MONSTER loader now finalizes inherited
`attack_cost` with the pinned 100-move default, direct-value precedence, integer
relative modifiers, and C++-matching truncating proportional modifiers. Startup validates every selected
definition as a nonzero `u16`; the classic zombie costs 100 moves and the
skeletal slasher costs 70. Private live state and self-contained corpse
prototypes retain the value through hashing, snapshots, revival, SQLite
recovery, and portable replay without expanding visible creature DTOs.
Ordinary adjacent monster melee now spends exactly `attack_cost * 20` action
points on both a hit and a miss. Signed readiness therefore preserves slow
attack debt, while low-cost monsters can spend legitimately banked readiness
on multiple same-tick attacks. Zero is rejected at spawn, snapshot, corpse,
and selected-content boundaries so the authoritative action loop cannot fail
to make progress.

Protocol 76 advances to schema 54, CanonicalStateV52, and CanonicalEventsV15.
Items now own an ordered bounded collection of detachable magazine wells, each
identified by its canonical inherited `pocket_data` index and optional source
ID. Installed magazines keep stable item IDs in every well; validation, state
hashing, snapshots, SQLite recovery, and portable replay traverse them all.
Reload commands select a concrete well index and the authoritative result event
echoes that index; stale or nonexistent indices fail closed. Powered tools name
the exact well supplying energy, tool-charge debit is deterministic across
ordered wells, and disassembly atomically detaches every installed magazine.
The server normalizes all compatible strict wells instead of assuming a single
battery slot, while the Bevy client selects the first compatible canonical well
until the generic server-driven interaction UI lands. Built-in gun ammunition
temporarily retains the explicit `None` reload target pending item-backed
ammunition pockets.
Because this changes Postcard layouts, databases below schema 54 that contain
snapshots or journal batches are rejected before mutation; metadata-only older
databases may still use the existing backed-up SQL migration path.

Protocol 77 advances to schema 55, CanonicalStateV53, and CanonicalEventsV16.
Strict integral `MAGAZINE` pockets now own stable nested ammunition items;
outer magazine charges are zero in newly normalized pinned content. Exact-pocket
reload transfers a whole stack without changing its ID, allocates one new ID
for a partial split into an empty pocket, and merges compatible stacks without
creating a redundant object. Merge compatibility includes every retained
non-identity/non-charge field, so distinct item state cannot disappear into an
existing stack. Fractional battery energy occupies one capacity slot. Capacity,
ammunition category, inherited pocket
index/ID, and `NO_RELOAD`/`NO_UNLOAD` access are canonical. Recursive
validation bounds containment depth, rejects aggregate-plus-item-backed state,
and traverses nested IDs through hashing, snapshots, SQLite recovery, portable
replay, pickup/drop, detachable installation, crafting, and disassembly gates;
loaded integral magazines reject disassembly until explicit unloading exists.
The starter medium cell is generated preloaded with a stable `battery` child,
so it remains usable without violating its pinned `NO_RELOAD` flag. A
renderer-independent conformance scenario proves partial-split identity across
direct execution, per-tick restore, SQLite recovery, and portable replay.
Databases below schema 55 that contain serialized state are rejected before
mutation.

Protocol 78 advances to schema 56, CanonicalStateV54, and CanonicalEventsV17.
The server now accepts an explicit stable-ID `RemovePocketItem` command for
item-backed integral magazines and detachable magazine wells. Successful
removal returns the exact contained object to top-level inventory without
allocating a replacement ID. Pinned `NO_UNLOAD`, stale contained identities,
active power wells, and full inventories reject before mutation. Fractionally
depleted battery ammunition becomes a valid loose item: its sub-charge energy
follows the stable ammunition identity out of an integral pocket and moves back
into that pocket on reload without rounding or loss. Charged fractional items
cannot be silently consumed as crafting/construction components or disassembled.
The Bevy client exposes deterministic removal with `Y`, while conformance
scenario format 4 and observation format 3 prove whole-stack removal through
direct execution, per-tick restore, SQLite recovery, and portable replay.
Databases below schema 56 that contain serialized state are rejected before
mutation.

Protocol 79 advances to schema 57, CanonicalStateV55, and CanonicalEventsV18.
The strict item loader and authoritative runtime now admit ammunition-restricted
`CONTAINER` pockets such as pinned `quiver`, while richer physical containers
remain fail-closed. A pocket retains its inherited index/ID, sorted per-category
capacities, base access moves, rigidity, and `NO_RELOAD`/`NO_UNLOAD` policy.
Only ordinary count-by-charge ammunition may enter; one category is active at a
time, while distinct item variants of that category retain separate stable
objects. Whole insertion preserves the source ID, a partial insertion that
creates a new contained stack allocates exactly one split ID, compatible
merging retains the lowest existing ID, and
exact whole removal returns the same contained object. Capacity, category,
allocator, inventory, and access failures are atomic and cost no action points;
accepted transfers use the pocket's pinned base move cost. The client binds `I`
to deterministic fitting insertion and extends `Y` removal and inventory labels
to these pockets. The starter ground loadout supplies an empty pinned quiver and a
separate wooden-arrow stack. Conformance scenario format 5/observation format 4 proves partial
insertion, multiple same-category variants, exact removal, and category switching
through direct execution, per-tick restore, SQLite recovery, and portable replay.
Databases below schema 57 that contain serialized state are rejected before
mutation.

Protocol 80 advances to schema 58 and CanonicalStateV56 while retaining
CanonicalEventsV18. A strict selected-content item-group registry now models
legacy and modern collection/distribution groups, ordered nested nodes, named
references, count and charge ranges, self-copy/extension load order, item
migrations, cycle/reference checks, and explicit unsupported fields. Canonical
worlds persist only the sorted reachable group closure. Structural bash sources
use named or inline graphs; the authoritative planner consumes one named RNG
stream in pinned source order, preflights at most 4,096 stable output IDs, and
then atomically transforms the tile and materializes drops. The starter-world
`t_wall` now resolves the pinned `wall_bash_results` collection (maximum 82
objects) before becoming `t_floor`; the generalized source path raises strict
furniture-bash admission from 537 to 539 of 699 definitions. Scenario format
6/observation format 5 prove
the same weighted/count/charge result through direct execution, per-tick
snapshot restore, SQLite recovery, and portable replay. Ammo/magazine dressing
and unsupported entry fields remain fail-closed. Databases below schema 58 that
contain serialized state are rejected before mutation.

Protocol 81 advances to schema 59 and CanonicalStateV57 while retaining
CanonicalEventsV18. Fresh worlds no longer use the synthetic grass filler or a
hand-built partial-submap cabin. A strict selected-content loader retains
ordinary string and flat-array OMT roots, exact 24x24 Unicode display-cell rows, variant
weights, fixed and weighted terrain/furniture glyphs, static palette closure,
default-region substitutions, and one named item-group placement per glyph.
The server currently admits the real pinned `lmoe` surface definition and
resolves `t_region_groundcover` through its retained weighted regional table.
Each OMT is generated as an atomic 2x2-submap/24x24 unit from a coordinate-owned
ChaCha stream, so discovery order, restart, and replay cannot change its
contents. The initial active bubble contains 36 complete OMTs/144 chunks;
movement plans every newly intersecting cell before committing any of them.
The worldgen catalog, default OMT identity, regional tables, generated chunks,
and referenced item-group closure are canonical and bounded. The ordinary
`field` mapgen remains fail-closed because its corpse loot requires item damage
and general container nesting that are not yet representable. Databases below
schema 59 that contain serialized state are rejected before mutation.

Protocol 82 advances to schema 60 and CanonicalStateV58 while retaining
CanonicalEventsV18. The selected-content start-location registry finalizes
inheritance, source-ordered terrain targets, city and z constraints, flags, and
retained parameters. Exact, type, subtype, underscore-bounded prefix, and
contains matching use the pinned overmap-terrain identities characterized by
the C++ oracle. The current runtime admits only `sloc_lmoe`: city-independent,
parameter-free, flag-free, and usable at z=0. Fresh worlds persist the explicit
`lmoe_north` full identity plus its `lmoe` type/subtype/generator, and the server
chooses the matching origin OMT for every new character while the identity is
globally repeated, preserving access to the fixed starter loadout and encounter.
It tries deterministically shuffled matching OMTs if that cell is occupied, a multiplayer
adaptation exercised with two actors through direct, per-tick snapshot, SQLite,
and portable-replay conformance (scenario format 7/observation format 6).
Coordinate-owned overmap layouts, cities, specials, inside/outside start-tile
rating, mapgen parameters, preparation flags, and spawn groups remain explicit
later boundaries. Databases below schema 60 that contain serialized state are
rejected before mutation.

Protocol 83 advances to worldgen algorithm 2, schema 61, and
CanonicalStateV59 while retaining CanonicalEventsV18. Worlds now persist a
bounded 180x180 coordinate-owned overmap layout as canonical z-sorted RLE
layers over full/type/subtype/generator/rotation identities. Validation requires
one exact surface layer, sorted and fully used identities, canonical runs,
every referenced generator, and start targets that match a generated surface
identity. Runtime admission additionally requires the complete initial bubble
to fit and every possible start target to occur there. Coordinates outside the
retained region fail closed; traversal into the prefetch boundary is an
ordinary blocked command, not a simulation failure.

The selected-content overmap-terrain registry finalizes inherited and
load-order-overlaid definitions into ordinary four-way, all 16 linear, and
nonrotating peers with the pinned mapgen subtype and clockwise rotation. Local
mapgen dispatches from each coordinate's identity; terrain, furniture, and
item choices retain source-phase RNG order and rotate only after completion.
A heterogeneous fixture proves distinct adjacent generators, shared marker
rotation, matching-only authoritative starts, atomic out-of-layout failure, and
snapshot stability. Direct, per-tick snapshot, SQLite, and portable-replay
conformance agree for a heterogeneous start layout. Character creation selects
only already-generated durable OMTs, and restored snapshots reject complete
cells outside the layout or on absent z-layers.

The production layout deliberately remains filled with `lmoe_north` so the
existing playable bootstrap stays exact to its admitted content. The real
regional z=0 base is `field`, but its reachable loot closure enters
`civilian_phones_case.contents-group`; strict runtime admission records and
rejects that boundary instead of omitting rare results. Exact raw damage, its
derived display level, signed charges, and immutable selected variants are
retained at runtime. Group/entry-wrapper shapes remain explicit, but general
containment is still unavailable.
Forest/city/road/river/special population and adjacent overmaps remain later
work. Databases below schema 61 that contain serialized state are rejected
before mutation.

Protocol 84 advances to schema 62 and CanonicalStateV60 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Item-group
entries now retain all six pinned holiday qualifiers through selected-content
loading, strict server normalization, canonical snapshots, recovery, and
portable replay. The persistent server fixes CDDA's real-world
`EVENT_SPAWNS` option to its pinned default `off`: collection entries still
consume their probability roll before producing nothing, and distribution
entries retain their ticket interval so a selected inactive entry deliberately
yields no item. The simulation never reads host wall time, keeping macOS,
Linux, Windows, recovery, and replay deterministic. The existing authoritative
smash scenario includes an inactive holiday drop and remains identical through
direct execution, per-tick restore, SQLite recovery, and portable replay.
Enabling seasonal content later requires an explicit persisted world policy;
it cannot be inferred from process-local time. Databases below schema 62 that
contain serialized state are rejected before mutation.

Protocol 85 advances to schema 63 and CanonicalStateV61 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Normalized
item-group entries now carry an explicit fixed-zero raw-damage marker whenever
the pinned loader constructed an `Item_modifier`. For every direct leaf, the
simulation preserves the per-item presentation-seed draw, empty-variant
selection, and unconditional fit draw before evaluating the marker's damage
range and charge dressing even for `count: 1`. Variable-size items,
nested-group modifiers, degrading vehicle parts, fouling guns, corpse and
preloaded-magazine construction, temperature-bearing comestibles,
constructor-owned state, and definitions whose nonzero raw damage, variants,
sealing, or containment cannot yet be stored remain fail-closed. Databases
below schema 63 that contain serialized state are rejected before mutation.

Protocol 86 advances to schema 64 and CanonicalStateV62 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Canonical items
and retained components now store exact raw damage and a self-contained
selected variant. The selected-content ITEM importer finalizes ordered generic
variants through replacement, inheritance, extension, and deletion, including
base ITEM fallback for missing or empty alternate text and art. Direct and
named-group modifiers apply damage and explicit variants after constructor/FIT
phases; ranged charges precede magazine dressing and `<any>` performs exact
weighted reselection. Named modifiers are admitted only when every possible
child leaf has no unrepresented modifier side effects. Ordinary corpses retain
the pinned float32-derived raw overkill value through recovery and replay. The
shared structural-bash scenario
retains two raw-damaged, explicitly variant-selected drops through direct,
per-tick snapshot, SQLite, portable replay, and ordinary Bevy item-menu access.
The pinned C++ oracle also records exact constructor-variant choices and
downstream RNG values. General wrapper contents, stable nested ownership, and
overflow remain fail-closed at `civilian_phones_case.contents-group`. Databases
below schema 64 that contain serialized state are rejected before mutation.

Protocol 87 advances to schema 65 and CanonicalStateV63 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Item-group
definitions now preserve whole-group wrappers, entry wrappers, modifier
containers, `contents-item`, `contents-group`, sealing, and explicit
spill/discard overflow. A recursive authoritative planner materializes rigid
physical and E-file pockets in preorder so every wrapper, contained item,
integral battery, and E-file receives a stable identity that survives snapshot,
SQLite recovery, and portable replay. ITEM normalization also retains exact
longest-side fit, inline snippet choices, and typed constructor variables;
selected snippets and per-instance variables are self-contained canonical item
state and appear in the ordinary client item menu. The complete pinned
`civilian_phones_case` closure now normalizes, including case variants, phone
battery charges, locked/unlocked selection, and generated E-files. Flexible
physical wrappers and constructor semantics outside the represented strict
pocket shapes remain fail-closed. The structural-bash conformance scenario now
places its drops inside a sealed rigid wrapper and proves identical nested IDs
and item state through all four execution modes. Databases below schema 65 that
contain serialized state are rejected before mutation.

Protocol 88 advances to schema 66 and CanonicalStateV64 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Tool charge
modifiers now resolve either integral storage or one default detachable
magazine plus its default ammunition before entering simulation. The
generalized planner always installs the detachable magazine for an explicit
zero charge, creates ammunition only for a positive charge, and clamps the
loaded amount to the magazine pocket capacity. Magazine-well rigidity is
canonical, so rigid wells exclude installed-magazine volume while non-rigid
wells include it recursively in wrapper fit checks. Exact C++ traces cover
requests 0, 1, 56, and 100. A production named-group trace also proves that a
later charge modifier reuses the installed magazine, replaces only its
ammunition, and preserves the downstream RNG phase; direct Rust comparison and
direct/snapshot/SQLite/replay conformance preserve the resulting nested stable
IDs. The real field loot closure now passes `wearable_light`. Databases below
schema 66 that contain serialized state are rejected before mutation.

Protocol 89/schema 67/CanonicalStateV65 add self-contained recursive item
description expansion. The selected English snippet registry preserves
identified-before-anonymous weighting, category overrides, and the pinned
`data/names/en.json` gender/usage categories. Server normalization retains only
the exact reachable closure; cycles, oversized output, excessive work, and an
unavailable generated variable slot fail closed. Construction reproduces the
pinned selected-variant expansion before base processing and the later final
variant expansion, while explicit variant modifiers expand once more. The
item-group oracle pins recursive/literal expansion and a real
`saint_necklace`; production gates also retain the complete `dog_tag_id` name
closure. Replicated descriptions render in the ordinary client item menu and
survive direct, snapshot, SQLite, and replay execution. The real field closure
then stops at `leg_sheath6` variable-size `FIT` state; Protocol 89 does not earn
runtime points because the field is not generated or playable.

Protocol 90/schema 68/CanonicalStateV66 add canonical per-instance `FIT` state
without changing replay format 3 or CanonicalEventsV18. Every direct item-group
leaf consumes the pinned one-in-three phase; only `VARSIZE` items gain `FIT`,
and raw wrapper construction does not consume a second FIT phase. Crafting
always fits variable-size outputs and byproducts, while default disassembly
components inherit fit from a fitted target and exact retained components keep
their own state. The pinned oracle's FIT traces record exact direct and
production fitted/unfitted witnesses for `leg_sheath6`, including names and
downstream RNG, and compares the transition directly with Rust. The normal
client item menu renders `(poor fit)` from replicated state. Direct, per-tick
snapshot, SQLite, and portable replay execution preserve FIT. The field closure
now passes `accessory_weaponcarry`.

Without changing Protocol 90 or schema 68, the generalized ammunition-loading
engine reuses the existing integral/detachable canonical descriptor for strict
magazines and supported tools. Every gun remains fail closed because its
owner-local or `ammo_set` state and constructor/RNG semantics differ. The pinned
C++ item-group oracle fixes direct zero/one/capacity/overflow traces plus production
`ammo_light_batteries` empty, partial, full, and ultra-light witnesses, including
downstream RNG; the Rust comparator executes the production charge transition.
Direct, per-tick snapshot, SQLite, and replay modes preserve nested ammunition,
and the Bevy item menu shows its authoritative charge count. The field closure
now passes `ammo_light_batteries` and stops at default-container ownership for
`aspirin` in `bottle_otc_painkiller_1_20`. Runtime points remain unchanged until
the real field is generated, explored, looted, persisted, and client-accessible.

Protocol 91/schema 69/CanonicalStateV67 complete the serialized containment
family by making finalized item-type default containers self-contained in the
authoritative item-group catalog. Direct construction fills liquids to physical
capacity, modifier fallback uses the item type's default, explicit
`container-item: "null"` suppresses that fallback, and an explicit modifier
container runs its own creator before receiving the payload. Raw whole-group
wrappers remain a separate constructor path. Sealing occurs only when the
effective rigid container is full; recursive ownership receives stable IDs in
preorder and remains renderer-independent. Seven exact default-container traces
join the pinned item-group oracle, including production one- and twenty-aspirin
boundaries and an ordered explicit-creator witness; all 104 C++ assertions and
the reusable direct Rust comparison pass. The shared item-group scenario keeps
the bottle/aspirin tree identical through direct, snapshot, SQLite, and replay
execution, and the ordinary Bevy item menu exposes authoritative contained
loot. The representative empty-catalog fixture has unchanged Postcard bytes:
hashing them under V66 reproduces
`7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`,
while the deliberate V67 domain yields
`b5c12b763060907d68bfbd96b4aea6372c17cb02676b5e499b0bc79f5679899e`.
The real field closure now passes `bottle_otc_painkiller_1_20` and next fails
closed on unimplemented temperature state for `chaw`; runtime points remain
unchanged until that field is an ordinary playable surface.

Protocol 92/schema 70/CanonicalStateV68 add canonical per-item temperature
ownership for the complete 36-definition materialless/nonperishable constructor
class. Four exact C++ traces and the reusable Rust comparator pin 0 K,
-10 J/g, birth-tick ownership, active/cadence/phase/flag state, and serialized
`last_temp_check`; the ten-minute Rust processor initializes admitted items to
293.150 K while retaining material thermodynamics, rot, custom freezing, and
weather as fail-closed families. Nested state agrees through direct, per-tick
snapshot, SQLite, and portable replay execution, and the Bevy item menu exposes
pending versus initialized temperature. The full field scan now reaches the
flexible `chaw_wrapper_1_20` container boundary. No runtime progress credit is
claimed before that wrapper family unlocks ordinary field exploration and
loot.

Protocol 93/schema 71/CanonicalStateV69 is the final serialized checkpoint for
the represented containment family: wrapper ownership, item/group contents,
sealing, spill/discard overflow, snippets, variables, default containers,
flexible physical pockets, reserved base volume, constructor collapse, and
automatic homogeneous-content collapse. Flexible external volume is the
contents volume above the pocket's retained `magazine_well` reserve; rigid and
E-file pockets reject that reserve. Constructor-default collapse and actual
runtime collapse are distinct canonical values, because `wrapper` defaults
open but CDDA's automatic whitelist collapses it after a homogeneous fill.
Exact minimum/maximum `chaw_wrapper_1_20`, `chewing_gum_full`, seven
default-container, and overflow traces bring the item-group oracle to 137 C++
assertions, followed by the reusable direct Rust comparison. The shared bash
scenario preserves mixed flexible contents, reserved volume, sealing, nested
automatic collapse, stable IDs, and temperature through direct, per-tick
snapshot, SQLite, and portable replay execution; the Bevy item menu exposes
collapsed authoritative pockets. Selected content now admits the two wrapper
groups and exactly six additional furniture bashes. The complete `field` scan
next fails closed at `chewing_gum_full_caff`, whose `caff_gum` needs material
thermodynamics. The real field is still not generated, so runtime progress
remains 44 of 263,435 core-DDA weighted points (0.0167%). Replay format 3 and
CanonicalEventsV18 are unchanged.

Protocol 94/schema 72/CanonicalStateV70 add the generalized nonperishable
material-thermodynamics family. Selected material inheritance, defaults, and
positive item portions reproduce upstream `float` accumulation before fixed
microjoule quantization; exact `water_clean`, `caff_gum`, and weighted `saline`
traces are compared directly with pinned C++. Snapshots retain the complete
specific-heat/latent-heat profile, the simulation accepts only the constructor
sentinel and exact 20 C initialized state, and materialless indeterminate
energy remains distinct from numeric material energy. Direct, per-tick
snapshot, SQLite, and portable replay preserve a generated material-backed
item, while the normal Bevy menu exposes its authoritative temperature. All
278 selected nonperishable/default-freezing material constructors now admit;
rot, custom freezing, and unsupported phases remain closed. The real `field`
scan advances to `civilian_eink_tablet_pcs`, whose item-group charge capacity
sentinel is the next generalized dependency. No runtime denominator credit is
claimed until the complete field is generated and playable.

Protocol 95/schema 73/CanonicalStateV71 add raw item-group charge endpoints and
explicit capacity ownership. Upper `-1` sentinels resolve only after the
concrete output and modifier container are known; integral `MAGAZINE` pockets,
detachable wells, magazines, and physical containers use their pinned capacity,
while an ordinary item without a capacity retains the exact no-op. Eleven exact
C++ traces include both 0- and 85-charge e-ink tablet boundaries and derive the
effective range from pinned item APIs before direct Rust comparison. The shared
item-group scenario preserves an integral-tool battery child through direct,
per-tick snapshot, SQLite, and portable replay execution. Selected content now
admits `civilian_eink_tablet_pcs`; the complete `field` scan next fails closed
at `costume_accessories`, where `leg_sheath6` requires generalized multi-pocket
wrapper insertion. No runtime denominator credit is claimed before the field is
generated, explored, looted, persisted, and client-accessible.

Inherited
`extend.using` requirements append to root requirements; pinned
`ch_sheet_metal_small` therefore retains blacksmithing plus carbon. Main results
and every byproduct receive stable IDs before work
starts, persist through disconnect/recovery/replay, count toward inventory
capacity, and are all burned if the craft is canceled. Non-unit speed annotations are
valid providers for these legacy recipes because pinned CDDA applies their
multiplier only to the still-unsupported step-recipe model. Charged qualities
require their pinned per-use charge threshold on each provider without spending
that energy merely for qualification. The starter ground loadout supplies a stick, small knife,
hammer, frozen toaster pastry, charged toaster, empty flashlight, a medium
battery cell containing its stable battery-ammunition child, an empty quiver,
wooden arrows, and pistol manual
so the implemented paths are immediately playable.
Pinned skill IDs, practical/theoretical levels, raw practice experience,
skill-gated autolearning, and one practice unit per nominal crafting second are
canonical, persistent, replayable, and visible in the HUD. The selected 234
proficiency definitions and recipe proficiency lists are also strict content:
mandatory proficiencies gate crafting, while missing optional proficiencies
apply deterministic time penalties and train at 5% craft boundaries, including
while disconnected. Learned/progress state is canonical and visible in the HUD.
Charged tool use follows the pinned twenty-bucket schedule, persists spent
charges across cancel and recovery, and interrupts atomically if energy runs
out. The flashlight path draws those charges from its installed stable battery.
Other tool pockets, batteries, power grids/UPS, step-recipe quality speed, randomized
`byproduct_group`, stochastic crafting failure, general construction, and advanced
crafting semantics remain incomplete. The slice also supports visibility-masked authoritative replication,
clean restart, and a world that
advances with no players. Protocol 42 adds a first deterministic
survival-autopilot behavior that lets an idle disconnected
character flee a visible nearby hostile using ordinary movement debt without
attacking, leaving loaded terrain, canceling interrupted work, or generating
chunks. Schema 29 journals ordered connection transitions before tick inputs,
including the forced disconnect after crash recovery, so SQLite recovery and
portable replay reproduce the same flight. Protocol 43 lets a trapped actor
spend one ordinary melee action against a visible adjacent aggressive creature
only when no retreat increases safety; a surviving hostile still acts on its
turn, so disconnected characters remain vulnerable. Protocol 44 lets a safe,
already starving or dangerously dehydrated actor consume one stable-ID-selected
owned unwielded ordinary food or drink, with danger taking priority and exact
recovery/replay. Protocol 45 permits a tired, fed, non-dehydrated actor to sleep
only on current positive-comfort furniture when the same danger radius is
clear, with a typed autopilot reason and exact recovery/replay. The full
hazard/shelter/medicine/sleep-location policy and richer nutrition rules remain
incomplete.
Authenticated character chat is routed by the server,
and moving into unexplored wilderness deterministically generates and persists
every complete 24x24 OMT intersecting the active radius. The initial pinned
`lmoe` surface bootstrap contains 36 atomic OMTs/144 submaps. The HUD displays the pinned default
91-day-season calendar derived from the persistent world tick. Speed-100 actors
and creatures perform ordinary 100-move actions once per second; actor movement
uses the pinned cardinal/diagonal source/destination terrain-plus-furniture cost
as signed readiness debt. Readiness banks while idle and the client may buffer two
semantic actions. Held movement uses authenticated sequence-numbered iroh
datagrams with replayable server state and a loss-safe lease. Per-character
terrain memory persists and is drawn dimly when
a previously seen tile is currently occluded; remembered state never authorizes
an interaction. The pinned Boston sun/moon cycle now changes authoritative
sight and the HUD/terrain palette across day, twilight, and night. Daylight
reaches the pinned 60-tile maximum; independent compressed snapshot
streams keep those 11x11-submap updates off control/chat traffic. From the
repository root, the server also writes verified owner-only hourly replay
archives under `<world>/replays`, publishes their content-addressed initial
snapshots under `<world>/snapshot-objects`, safely compacts old recovery history,
and retains archives for 30 days. After retained replays and their references
fully verify, exact-name unreferenced snapshot objects are collected; any
verification failure preserves every candidate. An initially due and then hourly online backup
under `<world>/backups` retains 24 newest hourly plus 30 older daily generations;
its stepped read-only SQLite copy does not occupy the critical persistence
writer, and each generation binds the verified copy and protected iroh identity.
Archive and backup publication use atomic synced writes. Opening an older
on-disk schema first publishes an integrity-checked owner-only generation under
`<world>/pre-migration-backups`; its bounded manifest binds a sidecar-free
database copy and the exact protected server identity before migration begins:

```sh
cargo build --workspace

# Create a protected client identity and copy the printed endpoint ID.
target/debug/cdda-client --profile player-one --identity-only

# With the server stopped, preauthorize that exact ID for ten minutes.
cargo xtask account-create world/world.db <CLIENT_ENDPOINT_ID> "Player One" player

# Start the standalone server. It prints and writes its shareable EndpointAddr.
target/debug/cdda-server world

# In a second terminal, prove the pending identity through iroh.
target/debug/cdda-client --profile player-one \
  --enroll-address world/endpoint-address.json --enroll-only

# Launch the Bevy client and choose or create a character in its bounded
# keyboard menu. During creation, Up/Down selects STR/DEX/INT/PER and
# Left/Right adjusts the selected stat from 4 through 20. The optional
# `--character "Survivor"` shortcut selects that exact existing name or creates
# it with the pinned default 8 in every stat, which is convenient for automation.
target/debug/cdda-client --profile player-one \
  --play-address world/endpoint-address.json

# Once connected, move cardinally with WASD/arrows or the numpad; numpad
# 1/3/7/9 and Home/PageUp/End/PageDown move diagonally, while . or numpad 5 waits.
# G picks up, Q drops, E/R
# wields/unwields, U reloads a wielded gun from matching carried ammunition, C
# consumes carried food/drink, P activates/deactivates powered items, B crafts or resumes, V reads or resumes, N
# disassembles or resumes, X cancels the current craft, book study, or
# disassembly, O/L
# opens/closes adjacent terrain, H smashes an adjacent visible registered
# structure with a supported wielded bash-only item, Z sleeps
# or wakes, F selects an adjacent melee target, and T selects a visible target in
# the wielded gun's range. Enter opens chat; Enter sends and Escape
# cancels the current message. In chat, `/report-last <details>` durably reports
# the latest other character who spoke; the server returns a typed result.
# When an item, target, terrain, crafting, reading, or disassembly command has multiple valid choices, arrows or
# J/K select the entry, Enter confirms, and Escape cancels; one choice stays immediate.
```

The same client binary exposes passwordless, one-shot account-key operations.
Close that account's gameplay client first because the server permits one live
game connection per account. To add a key, generate a second profile with
`--identity-only`, stage its printed endpoint from an active profile, and prove
the exact new key with the normal enrollment command:

```sh
target/debug/cdda-client --profile player-one \
  --play-address world/endpoint-address.json --account-key list
target/debug/cdda-client --profile player-one \
  --play-address world/endpoint-address.json --account-key add <NEW_ENDPOINT_ID>
target/debug/cdda-client --profile player-one-second-key \
  --enroll-address world/endpoint-address.json --enroll-only
target/debug/cdda-client --profile player-one-second-key \
  --play-address world/endpoint-address.json --account-key revoke <OLD_ENDPOINT_ID>
```

Moderators and administrators use their existing iroh profile through the
dedicated admin ALPN. `--admin` and its arguments must come last. Responses print
copyable stable IDs and pagination cursors; a rejected command exits nonzero.
For example:

```sh
target/debug/cdda-client --profile administrator \
  --admin-address world/endpoint-address.json --admin list-accounts
target/debug/cdda-client --profile moderator \
  --admin-address world/endpoint-address.json --admin list-reports open
target/debug/cdda-client --profile moderator \
  --admin-address world/endpoint-address.json \
  --admin resolve-report 1 actioned
target/debug/cdda-client --profile administrator \
  --admin-address world/endpoint-address.json \
  --admin create-account <ENDPOINT_ID> player "New Player"
target/debug/cdda-client --profile administrator \
  --admin-address world/endpoint-address.json \
  --admin inspect-character <ACTOR_ID>
```

The complete one-shot command grammar is:

```text
list-accounts [AFTER LIMIT]
list-characters ACCOUNT                 # includes public live-session state
inspect-character ACTOR [INVENTORY_AFTER INVENTORY_LIMIT]
list-reports [all|open|actioned|dismissed [AFTER LIMIT]]
history ACCOUNT [AFTER LIMIT]
role ACCOUNT player|moderator|administrator
status ACCOUNT enabled|disabled|banned
suspend ACCOUNT off|SECONDS
mute ACCOUNT off|SECONDS
kick ACCOUNT
transfer ACTOR NEW_ACCOUNT
resolve-report REPORT actioned|dismissed
create-account ENDPOINT ROLE DISPLAY_NAME
list-endpoints ACCOUNT
add-endpoint ACCOUNT ENDPOINT
revoke-endpoint ACCOUNT ENDPOINT
```

`inspect-character` is administrator-only. It returns canonical position,
health, needs, sleep/readiness/input state, equipment, queued-action count,
terrain-memory chunk count, and at most eight inventory items. Pass the printed
`next_inventory_after` as `INVENTORY_AFTER` to page; moderators cannot access
this private response.

With both worlds stopped, restore a verified generation into a directory that
does not already exist. Restore validates installed content, the manifest,
database and key checksums, SQLite integrity, deterministic replay state, and
the key-derived server endpoint before an atomic rename; it never overwrites a
world. First startup re-verifies the untouched restore and records durable
provenance; subsequent starts refuse a different content identity or server
key. An optional content-manifest path may follow the destination:

```sh
target/debug/cdda-server --restore \
  world/backups/backup-<UTC>-<JOURNAL_SEQUENCE> restored-world
target/debug/cdda-server restored-world
```

If every client key for an account is lost, generate a fresh client identity,
stop the server, and use the local recovery surface. It permanently revokes all
old bindings, recovery-locks the account, and stages only the exact replacement
for ten minutes; the normal iroh enrollment proof then unlocks it. No password,
recovery token, or invitation secret exists:

```sh
target/debug/cdda-client --profile recovered-player --identity-only
cargo xtask account-recover \
  world/world.db <ACCOUNT_ID> <NEW_CLIENT_ENDPOINT_ID>
target/debug/cdda-server world
target/debug/cdda-client --profile recovered-player \
  --enroll-address world/endpoint-address.json --enroll-only
```

Repeat the identity/account/enrollment steps with a different profile and
display name for another player. Never copy `client-identity.key` or
`server-identity.key`; the public endpoint IDs and `endpoint-address.json` are
the only identity material intended to be shared.

Both binaries validate `vendor/cdda-content-manifest.json` and all manifested
files before gameplay. A packaged manifest can be selected with the server's
second positional argument or the client's `--content-manifest` option. The
manifest binds peers to identical content but does not imply that every imported
definition has been behaviorally ported; current support is tracked in
`PORTING_MATRIX.md`.

## Developer verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
cargo xtask verify-dependency-boundaries
cargo xtask parity-ledger-check
cargo xtask runtime-progress-check
cargo xtask astronomy-table-check
cargo xtask content-validate
cargo xtask content-inventory-check
```

Run the development-only pinned C++ differential oracle separately. Its first
run exports the exact upstream Git commit into ignored `target/`, compiles the
headless upstream core, and can take several minutes; later runs reuse the
build only after its baseline, tree, adapter, and executable digest validate.
Any cache mismatch discards the export and rebuilds from the pinned archive.
On macOS this command requires Homebrew `ncurses` because the upstream headless
core uses its wide-character API.

```sh
cargo xtask cpp-oracle-check
cargo xtask cpp-oracle-check docs/oracles/item-group-generation-v1.json
cargo xtask cpp-oracle-check docs/oracles/mapgen-static-semantics-v1.json
```

The pocket scenario exercises real upstream `item_pocket::can_contain`
maximum-length behavior at the shorter, equal, and longer boundaries. The
item-group scenario covers ordered collection RNG consumption, distribution
interval boundaries, fixed/ranged count and charges, zero-to-one clamping, and
nested groups sharing the same RNG stream. Its fixed-count representative pins
the exact downstream value after item-seed, empty-variant, fit, and modifier
damage phases. It also characterizes raw versus
display damage, explicit variants, integral and detachable ammunition dressing,
shuffled container insertion with discard/spill overflow, the real
`everyday_corpse` wrapper family, nonholiday collection filtering, and inactive
event entries retaining distribution tickets while producing no item. Exact
seeded container traces retain every first-observed content order and its
top-level spill result; exact corpse traces retain a fixed representative and
the first maximum-damage-content boundary, so aggregates alone cannot satisfy
the scenario. The
mapgen scenario verifies exact, type, subtype, prefix, and contains matching;
rotatable and linear OMT orientation; point rotation; and static palette/nested
phase ordering. The runner rejects unknown JSON
fields, mismatched format or
baseline versions, the wrong upstream Git tree, and any observation drift. It
enforces byte bounds while reading, serializes concurrent invocations, freshly
exports pinned runtime data, removes temporary run data after every result,
never modifies `../Cataclysm-DDA`, and does not link C++ into shipped Rust code.
See `tools/cpp-oracle/README.md` for the bounded bootstrap design and current
limitations.

Export a renderer-independent replay from a stopped or copied world database,
then verify its content identity and deterministic final state without starting
the server or graphical client:

```sh
cargo xtask replay-export world/world.db world/replay.cddar
cargo xtask replay-verify world/replay.cddar
```

Both commands validate the pinned content package. They accept an optional
manifest path as their final argument for packaged installations. Export uses
exclusive creation and will not overwrite an existing replay file.

To reproduce the vendored package from the exact clean upstream checkout:

```sh
cargo xtask content-import ../Cataclysm-DDA
```

See `IMPLEMENTATION_STATUS.md` for current runnable behavior and
`PORTING_MATRIX.md` for behavioral parity status. The fixed upstream reference
is commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05` in
`../Cataclysm-DDA/`.

`docs/parity-ledger.json` is the machine-readable implementation DAG. Its gate
binds the ledger to the current protocol, persistence schema, replay format,
and baseline while rejecting missing Rust paths, duplicate priorities, unknown
dependencies, and cycles. A milestone can be complete only after its pinned
characterization, generalized engine, direct comparison, all four recovery
modes, runtime admission, and authoritative client path (or a recorded
not-applicable rationale) are coherent. `docs/runtime-progress.json` separately
records raw parser inventory and weighted runtime evidence. Only definitions
that are generated, authoritatively interacted with, persisted,
client-accessible, and four-mode verified earn the corresponding points; loaded
JSON alone earns none. The denominator is independently derived from the pinned
manifest and split between core-DDA ordinary gameplay (13,865 definitions,
263,435 possible weighted points; currently 44, or 0.0167%) and selectable
bundled mods (5,967 definitions, 113,373 possible points; currently zero).
The mod target is the union of nonobsolete pinned mods that can participate in
at least one valid new-world selection; mutually exclusive configurations still
contribute their distinct playable definitions. Ordinary playable loops are
tracked separately from both parser coverage and weighted evidence.
The leaf `cdda-conformance` crate runs versioned
item-flow, indexed multi-well, and item-backed split/removal scenarios plus
checked-in semantic/hash expectations through direct headless simulation,
per-tick snapshot restoration, SQLite recovery, and portable replay
verification.
The separate C++ oracle provides checked real-upstream item-pocket and
item-group kernels; additional kernels still require explicit versioned
adapters.

## License

Original Rust source and documentation are licensed under CC BY-SA 3.0. Imported
CDDA content retains its upstream attribution and compatible licenses; every
vendored path is recorded in the content provenance manifest.
