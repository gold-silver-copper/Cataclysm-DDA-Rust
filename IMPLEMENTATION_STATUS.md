# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Last fully green repository commit:
  `bcd868170d4fdf50b6e1a2aadff7ebc980dac40d` (`Record Protocol 88
  tool-charge checkpoint`). Its verified implementation tree is
  `bdeb8570615fae97288b0d5d5d8b1e18e407e04c`.
- Active milestone: `regional-terrain-base`.
- Active candidate: generalized base/variant item-description snippet
  expansion and strict admission of `accessory_necklace`.
- Candidate representation: protocol 89, worldgen algorithm 2, persistence
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

## Active description-expansion family

- `DescriptionSnippetRegistry` loads selected content in source order while
  retaining upstream's identified-before-anonymous weighted choice order,
  overrides, translations, duplicate-ID rejection, and checked weights.
- Server normalization emits only the recursively reachable category closure,
  sorted canonically. Unknown tags remain literal; cycles, excessive depth,
  excessive choices, overflows, and oversized output fail closed.
- The simulation expands base descriptions and then selected variant
  descriptions in constructor order. Variant text overwrites base text exactly
  as upstream does; explicit variant modifiers expand again. Each recognized
  tag consumes one canonical RNG draw, including one-choice and zero-total
  categories.
- The pinned C++ oracle records the exact recursive/literal boundary
  `Foo <lt>lt<gt> <unknown>` -> `Foo <lt> <unknown>`, including downstream RNG,
  plus production seed 59 for
  `accessory_necklace` -> `holy_symbol/saint_necklace`. The production text is
  expanded to St. Mary and leaves downstream draw 1652. The direct comparator
  executes the same bounded expansion through Rust.
- Direct, per-tick snapshot, SQLite, and portable-replay conformance preserve
  an authoritative expanded item variable. The ordinary Bevy item menu renders
  that replicated description without consulting live content.
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

Current ownership sizes are 29,303 lines in `sim/lib.rs`, 3,790 in
`sim/items.rs`, 9,758 in `protocol/lib.rs`, 1,511 in
`protocol/item_groups.rs`, 13,065 in persistence, 8,793 in the server library,
and 1,293 in `server/item_groups.rs`.

Against `bcd8681`, this candidate adds 221 net lines to `sim/items.rs`, 208 to
`protocol/item_groups.rs`, and 95 to `server/item_groups.rs`. Central-file
growth is limited to three net lines in `sim/lib.rs` for an initializer, the
V65 domain, and the reusable comparator export, and five net lines in
`protocol/lib.rs` for exports, initializers, and the Protocol 89 constant. No
new item behavior is implemented in a central `lib.rs`.

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

The active candidate currently passes:

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

The full formatting, all-target check, strict Clippy, 378-test workspace,
warning-free rustdoc, content, inventory, parity, progress, astronomy, and
dependency-boundary gates pass. All three pinned C++ oracles pass; the
item-group oracle includes 76 assertions and the new direct comparison. The
fixed-commit independent review is still pending, so runtime progress remains
deliberately unbound above green parent `bcd8681`.

## Next dependency boundary

Complete and independently review this description family, then implement the
generalized variable-size `FIT` constructor state needed by the field closure.
Do not start cities, roads, rivers, specials, anatomy, or EOCs first. The next
playable unlock remains the real field plus ordinary client exploration and
loot.
