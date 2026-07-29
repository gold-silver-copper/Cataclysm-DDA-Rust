# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Live checkpoint

- Verified green commit: `de09724a27064d293041ce1cf4df5e458ac403a7`
  (`Extract protocol item group domain`). The Protocol 86 worktree is not yet
  checkpoint-bound.
- Active milestone: `regional-terrain-base`. Protocol item-group ownership and
  simulation item-instance ownership are extracted; the broader protocol and
  simulation item module milestones remain in progress.
- Worktree representation: protocol 86, worldgen algorithm 2, persistence
  schema/minimum recoverable schema 64, replay format 3, CanonicalStateV62, and
  CanonicalEventsV18.
- Conformance: scenario 7 and observation 6.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only. The server and
  simulation are plain Rust; iroh 1.0.3 owns networking and authentication.

Mapgen/overmap progress is split into durable submilestones:

- `atomic-static-mapgen`: `oracle_pending`; generalized runtime engine and four
  recovery modes exist, but no shared direct Rust/C++ comparator yet.
- `omt-identities-routing`: `oracle_pending` for the same comparator boundary.
- `start-location-selection`: `oracle_pending` for the same comparator boundary.
- `regional-terrain-base`: `in_progress`; exact field closure currently stops
  at general `civilian_phones_case.contents-group` ownership.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

Protocol 81-era and earlier narration is archived in
[docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md](docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md).

## Runnable behavior

This is a persistent server-authoritative multiplayer foundation, not yet a
complete CDDA port. The Bevy client can authenticate with iroh, create/select a
persistent character, traverse durable generated terrain, fight admitted
creatures, manipulate visible items, smash admitted structures, and use the
implemented crafting, reading, disassembly, construction, survival, recovery,
and replay paths. Characters remain present and vulnerable while disconnected,
and world time continues with zero players.

Fresh worlds retain a bounded 180x180 z=0 overmap layout. Production still
repeats the exact admitted `lmoe_north` generator so the bootstrap remains
playable. Coordinate-owned OMT identities, atomic 24x24 generation, shared
terrain/furniture/item rotation, matching start selection, durable chunks, and
ordinary blocked layout edges are implemented. The real default `field` layer
is not admitted until its entire loot/containment closure is exact.

## Active Protocol 86 family

- ITEM content normalization finalizes source-ordered generic variants through
  replacement, inheritance, `extend`, and `delete`. Missing or empty alternate
  text and art use the finalized base ITEM before append, while unsupported
  variant fields and visibility policies remain fail-closed.
- Canonical items and component provenance store exact raw damage, its derived
  display level, and a self-contained selected variant. Snapshot/replay recovery
  never consults live content to reconstruct appearance metadata.
- The generalized item-group planner preserves constructor variant and FIT RNG
  phases, applies direct or named-group raw damage/explicit variants after
  completed child generation, clamps damage per leaf, rolls ranged charges
  before magazine dressing, and implements `<any>` reselection including the
  zero-weight retain-existing boundary.
- Named modifiers are valid only when every possible output leaf has no
  unrepresented modifier side effect. The protocol graph evaluator computes
  this through its memoized closure; simulation checks it again defensively.
  Local-composite modifiers, degradation, gun fouling/faults, unsupported
  constructor state, unsupported variant policy, and general containment fail
  closed.
- The pinned C++ oracle now has 51 assertions. It adds exact constructor-variant
  witnesses for both weighted choices and exact downstream RNG values. Existing
  exact container witnesses cover all six first-observed orders; exact corpse
  witnesses cover fixed seed 1 and the first maximum-damage-content boundary,
  so aggregate minima/maxima cannot satisfy the oracle alone.
- The structural-bash scenario emits two raw-damage-1000/display-level-2
  `weathered` splinters and retains their complete variant metadata identically
  in direct, per-tick snapshot, SQLite, and portable-replay modes. The ordinary
  client item menu displays the authoritative selected variant.
- Ordinary monster corpses now retain the exact pinned float32-derived raw
  overkill damage rather than reconstructing a display-level minimum. A
  non-boundary death is asserted live, after SQLite recovery, and after portable
  replay; the 625-HP/251-overflow raw-1003 rounding witness is explicit.
- Exact production admission rises from 521 to 524 furniture bashes. The three
  audited additions are `f_cardboard_door_o`, `f_cardboard_roof`, and
  `f_pallet_brick_adobe`; all other prior exclusions remain explicit.
- The fixed item-flow scenario remains at tick 80 with identical actors,
  inventory, ground items, and CanonicalEventsV18. Its CanonicalState root
  changes only from
  `2aae0f859788b6e83bd4c03972f32a6a78963a63c1cedf5774b6b1e895e37820`
  to
  `8f8710e06937a50c14bcad35a17dbc41a059128061f4be9316c4c6449358dc66`,
  isolating the new serialized defaults and CanonicalStateV62 domain.

## Measured progress

Runtime evidence remains four definitions and 44 weighted points. Each counted
definition is generated, authoritatively interacted with, persisted, and
client-accessible. No production definition receives four-mode credit from a
normalized semantic substitute. The three newly normalizable furniture bashes
earn no points yet because the current playable LMOE mapgen does not place
them. Parser inventory remains separate: 7,621 item groups, 9,520 mapgen
objects, 2,712 OMTs, and 150 starts earn no runtime credit merely for loading.

Current ownership sizes are 28,696 lines in `sim/lib.rs`, 1,151 in
`sim/items.rs`, 8,180 in `protocol/lib.rs`, 640 in
`protocol/item_groups.rs`, 13,058 in persistence, 8,776 in the server library,
6,377 in the server binary root, and 665 in server item-group normalization.
Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain explicit
mechanical extraction milestones before anatomy or EOC expansion.

## Explicit boundaries

- Production overmap population is still LMOE, not an upstream regional
  forest/city/road/river/special layout.
- General `contents-group` materialization, wrapper stable IDs, recursive item
  ownership, sealing, spill/discard overflow, and complete pocket capacity
  semantics remain unavailable. The regional-field closure rejects this exact
  boundary rather than dropping rare content.
- Adjacent overmaps, additional generated z-levels, cities, forests, roads,
  rivers, specials, extras, spawn groups/populations, zones, vehicles, and
  mapgen monsters remain unavailable.
- Rich start placement/scoring, nested/update mapgen, mapgen parameters,
  multi-layer glyphs, weighted one-time fill, and recursive regional targets
  remain fail-closed.
- Remote start generation remains unavailable until its worldgen mutations are
  journaled atomically with character creation.

## Latest verification

The Protocol 86 implementation candidate passes formatting, all-target
workspace checking, strict Clippy, 346 workspace tests, doc-tests, and
warning-free rustdoc. All six dependency/parity/progress/astronomy/content
gates pass; runtime progress remains four definitions and 44 points. The three
pinned C++ oracles pass 8 pocket, 51 item-group, and 17 mapgen assertions. The
production content test confirms exactly 524 admitted furniture bashes. A
fresh full-diff review against `f882a5d46e8d27163399b97c5ffaf6f0bda67320`
found and resolved charge/dressing order, variant fallback/`<any>`, float32
corpse damage, bounds, duplicate-validation complexity, and stale-documentation
issues; its final pass found no remaining P0/P1. Checkpoint binding is the only
remaining step.

The fixed upstream checkout remains
`4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Next dependency boundary

Finish and checkpoint the Protocol 86 damage/variant family without beginning
another subsystem. Then implement generalized `contents-group` and wrapper
ownership in the extracted item modules, with nested stable IDs and explicit
overflow. Require a pinned exact characterization, generalized engine,
direct-comparison disposition, all four recovery modes, runtime admission, and
ordinary server/client access before replacing the LMOE production fill with
the real field base. Forest/city/road/river/special placement begins only after
that base is exact and green.
