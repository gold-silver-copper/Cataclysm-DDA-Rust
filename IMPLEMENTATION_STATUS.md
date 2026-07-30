# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `032883aed6d3677597248c8e0ec8d0dc7de9324e`
  (`Implement recursive item description snippets`), tree
  `6da10d6bcb9fef98f7212a2a536ccc620cb3cae1`.
- Active milestone: `regional-terrain-base`.
- Completed family checkpoint: generalized base/variant item-description
  snippet expansion, selected English name categories, and strict admission of
  `accessory_necklace` plus `dog_tag_id`.
- Current representation: protocol 89, worldgen algorithm 2, persistence
  schema/minimum recoverable schema 67, replay format 3, CanonicalStateV65,
  and CanonicalEventsV18. Scenario format 7 and observation format 6 do not
  change.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only; the server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

Mapgen/overmap completion is intentionally split:

- `atomic-static-mapgen`: `complete`.
- `omt-identities-routing`: `complete`.
- `start-location-selection`: `complete`.
- `regional-terrain-base`: `in_progress`; description expansion is admitted,
  and exact field closure now stops at `accessory_weaponcarry` because
  `leg_sheath6` requires unimplemented variable-size `FIT` state.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

The mapgen completion evidence is in
[docs/reviews/mapgen-direct-comparator-completion.md](docs/reviews/mapgen-direct-comparator-completion.md).
Earlier protocol narration is retained in `docs/history` and scoped review
records instead of this live status.

## Runnable behavior

The persistent server-authoritative multiplayer foundation can authenticate
through iroh, create/select durable characters, keep the world running with no
players, traverse generated terrain, fight admitted creatures, manipulate
visible items, smash admitted structures, and use the implemented crafting,
reading, disassembly, construction, survival, recovery, and replay paths.
Disconnected characters remain physically present and vulnerable.

Worlds currently retain a bounded 180x180 z=0 overmap filled by the admitted
`lmoe_north` generator. Coordinate-owned OMT identities, atomic 24x24
generation, rotation, server-selected starts, durable chunks, and blocked
layout edges are runnable. The real default `field` layer remains outside the
production surface until its entire loot closure is exact.

## Completed description-expansion family

- `DescriptionSnippetRegistry` loads the pinned English name library before
  selected snippet content, maps gendered/unisex name usages into the eight
  upstream categories, and retains identified-before-anonymous weighting,
  overrides, translations, duplicate-ID rejection, and checked weights.
- Server normalization emits only the recursively reachable category closure,
  sorted canonically. Unknown tags remain literal; cycles, excessive depth,
  excessive choices, overflows, oversized output, exponential repeated-DAG
  work, and unavailable variable capacity fail closed.
- The simulation expands a selected variant immediately, then expands the base
  and selected variant again in the later constructor phase. The overwritten
  first result still advances RNG exactly as upstream; explicit variant
  modifiers expand once more. Each recognized tag consumes one canonical RNG
  draw, including one-choice and zero-total categories.
- The pinned C++ oracle records the exact recursive/literal boundary
  `Foo <lt>lt<gt> <unknown>` -> `Foo <lt> <unknown>`, including downstream RNG,
  plus production seed 59 for
  `accessory_necklace` -> `holy_symbol/saint_necklace`. The production text is
  expanded to St. Mary and leaves downstream draw 1652. The direct comparator
  executes the same bounded expansion through Rust.
- Direct, per-tick snapshot, SQLite, and portable-replay conformance preserve
  an authoritative expanded item variable. The ordinary Bevy item menu renders
  that replicated description without consulting live content.
- Production normalization retains all seven reachable `dog_tag_id`
  categories, including 3,045 family names, 4,275 female given names, and 1,219
  male given names. The real field boundary remains `leg_sheath6`/`VARSIZE`.
- Protocol 89 batches the description template and its reachable weighted
  categories into this containment-family checkpoint. Schema 67 and
  CanonicalStateV65 follow because Postcard state changed; replay format and
  CanonicalEvents remain unchanged.

## Measured progress

Runtime credit remains four core definitions and 44 weighted points. This
family enables strict normalization but does not earn production runtime credit
until the real field is generated, explored, looted, persisted, and exercised
through all four recovery modes.

- Core DDA ordinary-gameplay target: 13,865 definitions and 263,435 possible
  weighted points; 44 earned (0.0167%).
- Selectable bundled-mod target: 5,967 definitions and 113,373 possible
  weighted points; zero earned.
- Parser-only inventory remains separate: 7,621 item groups, 9,520 mapgen
  objects, 2,712 OMTs, and 150 starts.

Current ownership sizes are 29,303 lines in `sim/lib.rs`, 3,828 in
`sim/items.rs`, 9,758 in `protocol/lib.rs`, 1,624 in
`protocol/item_groups.rs`, 13,065 in persistence, 8,793 in the server library,
and 1,293 in `server/item_groups.rs`.

Against `bcd8681`, this family adds 259 net lines to `sim/items.rs`, 321 to
`protocol/item_groups.rs`, and 95 to `server/item_groups.rs`. Central-file
growth is limited to three net lines in `sim/lib.rs` for an initializer, the
V65 domain, and the reusable comparator export, and five net lines in
`protocol/lib.rs` for exports, initializers, and the Protocol 89 constant. The
server executable's growth is registry wiring and production admission tests;
the server implementation remains in `server/item_groups.rs`. No new item
behavior is implemented in a central `lib.rs`.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Explicit boundaries

- The real field is not yet runtime-admitted. Its next exact retained blocker
  is variable-size `FIT` state on `leg_sheath6`.
- Flexible physical spawn pockets, arbitrary player-driven containment,
  material-derived softness, and other unprojected constructor/pocket semantics
  remain explicit and fail closed.
- Cities, roads, rivers, specials, extras, spawn groups/populations, zones,
  vehicles, adjacent overmaps, and additional generated z-levels remain
  unavailable.
- Rich start scoring, nested/update mapgen, parameters, multi-layer glyphs,
  weighted one-time fill, and recursive regional targets remain unavailable.
- Remote start generation remains unavailable until its worldgen mutations can
  journal atomically with character creation.

## Latest verification

The fixed implementation passes:

- targeted content, protocol, simulation, server production-closure, client,
  and four-mode conformance tests;
- the pinned item-group C++ oracle with 76 exact assertions and the direct Rust
  comparison;
- production normalization of `accessory_necklace`, including all 14
  `<catholic_saints>` choices, followed by the exact `leg_sheath6` fail-closed
  boundary.

The representative canonical fixture changes from
`c476a1ccd153ece571ebf4a98be13242ab3a7163124abff4173d9c9050c1f9b7` to
`0878f47b5e8e159fdee5a57a6c7f90bab5e13e6bb944820a10585b835fb857be`.
Hashing the same Postcard bytes under CanonicalStateV64 reproduces the old
root, so this fixture change is solely the intentional V65 domain. The named
item-group scenario separately proves the new description representation
through all four recovery modes. CanonicalEventsV18 is unchanged.

The full formatting, all-target check, strict Clippy, 380-test workspace,
warning-free rustdoc, content, inventory, parity, progress, astronomy, and
dependency-boundary gates pass. All three pinned C++ oracles pass; the
item-group oracle includes 76 assertions and the direct comparison. The initial
`13bae07` review found two P1 and two P2 defects; all four are fixed in
`032883a`. Its replacement-tree review confirmed those runtime fixes and found
one P3: this live status still described the pre-fix line counts, test count,
and constructor order. The review checkpoint corrects that documentation; no
runtime/protocol P0-P2 remains, and no other P0-P3 was confirmed. Runtime
progress is bound to exact implementation commit `032883a` above green parent
`bcd8681`.

## Next dependency boundary

Implement the generalized variable-size `FIT` constructor state needed by the
field closure.
Do not start cities, roads, rivers, specials, anatomy, or EOCs first. The next
playable unlock remains the real field plus ordinary client exploration and
loot.
