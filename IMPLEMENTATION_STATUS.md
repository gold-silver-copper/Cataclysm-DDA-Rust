# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `a80f6c2a8c23c29146f67a843f8cbc34d0cbb6ec`,
  tree `22de91ca31f00f87ac269459b8317bcf362655b6`.
- Completed family checkpoint: generalized item-type default-container
  ownership. The fixed committed tree passed the complete local gates and an
  independent detached-context review with no P0-P3 finding.
- Active milestone: `regional-terrain-base`.
- Current representation: protocol 91, worldgen algorithm 2, persistence
  schema/minimum recoverable schema 69, replay format 3, CanonicalStateV67,
  and CanonicalEventsV18. Scenario format 7 and observation format 6 are
  unchanged.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only; the server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

Mapgen/overmap completion is split into bounded milestones:

- `atomic-static-mapgen`: `complete`.
- `omt-identities-routing`: `complete`.
- `start-location-selection`: `complete`.
- `regional-terrain-base`: `in_progress`; `accessory_weaponcarry`,
  `ammo_light_batteries`, and `bottle_otc_painkiller_1_20` now admit. The
  complete field scan next stops at unimplemented temperature state for
  `chaw`.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

Mapgen completion evidence is in
[docs/reviews/mapgen-direct-comparator-completion.md](docs/reviews/mapgen-direct-comparator-completion.md).
Historical protocol narration remains in `docs/history` and scoped reviews.

## Runnable behavior

The persistent server-authoritative multiplayer foundation authenticates with
iroh, creates and selects durable characters, keeps the world running without
players, traverses generated terrain, fights admitted creatures, manipulates
visible and contained items, smashes admitted structures, and runs implemented
crafting, reading, disassembly, construction, survival, recovery, and replay
paths. Disconnected characters remain physically present and vulnerable.

Worlds retain a bounded 180x180 z=0 overmap currently filled by the admitted
`lmoe_north` generator. Coordinate-owned OMT identities, atomic 24x24
generation, rotation, server-selected starts, durable chunks, and blocked
layout edges are runnable. The real default `field` layer remains outside the
production surface until its complete loot closure and ordinary exploration
loop are exact.

## Completed default-container family

- Finalized ITEM inheritance now retains `container`, `container_variant`, and
  `sealed`. Recursive normalized descriptors are cycle- and depth-bounded.
- Direct construction, modifier fallback, explicit-null suppression, explicit
  creators whose own default wrapper runs first, and raw whole-group wrappers
  are distinct paths. Liquids fill physical capacity, failed default insertion
  remains raw, and only a full sealable container seals.
- Protocol bounds count the complete creator subtree and reject unsupported
  dynamic named-group fallback. The server distinguishes raw wrappers from
  modifier creators; the simulation allocates stable ownership in preorder.
- Seven exact default-container traces cover direct water/aspirin, modifier
  fallback, explicit null, the ordered explicit ibuprofen/aspirin creator, and
  production one/twenty-aspirin boundaries. The item-group oracle passes 104
  assertions and the reusable direct Rust comparison.
- Direct, per-tick snapshot, SQLite, and portable replay conformance preserve a
  pill bottle owning aspirin. The normal Bevy item menu displays its contained
  count and selects removal by authoritative owner, pocket, and child ID.
- The serialized item-group catalog shape changed, so Protocol 91/schema 69 are
  required. Replay and event shapes did not change.

## Measured progress and module ownership

Runtime credit remains four core definitions and 44 weighted points. This
normalization unlock earns no production credit until the real field is
generated, explored, looted, persisted, exercised in all four recovery modes,
and accessible through the client.

- Core DDA ordinary-gameplay target: 13,865 definitions and 263,435 possible
  weighted points; 44 earned (0.0167%).
- Selectable bundled-mod target: 5,967 definitions and 113,373 possible
  weighted points; zero earned.
- Parser-only inventory remains separate: 7,621 item groups, 9,520 mapgen
  objects, 2,712 OMTs, and 150 starts.

Current ownership sizes are 29,249 lines in `sim/lib.rs`, 4,545 in
`sim/items.rs`, 9,801 in `protocol/lib.rs`, 1,916 in
`protocol/item_groups.rs`, 13,065 in persistence, 8,797 in the server library,
and 1,406 in `server/item_groups.rs`. Relative to the exact green parent, item
behavior grows extracted modules by 362 simulation, 273 protocol, and 91 server
lines. Central simulation grows by 14 lines solely for exports, the hash domain,
and mechanical fixture fields; central protocol grows by four version/fixture
lines, while persistence and the server library do not grow. The server
executable adds 33 production-normalization/test lines. This is the documented
module-budget justification; future item work remains in the extracted modules.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Explicit boundaries

- The real field is not runtime-admitted. Its first retained blocker is general
  comestible temperature state for `chaw`.
- Named-group modifier fallback remains fail closed when the generated
  top-level closure may contain item-type defaults.
- Every gun charge modifier remains unavailable pending a dedicated owner-local
  and `ammo_set` engine with direct pinned C++ traces.
- The complete scan retains later corpse-construction, capacity-sentinel,
  wrapper-shape, dressing, and snippet families. They remain fail closed and
  must be implemented as generalized engines.
- Flexible physical spawn pockets, arbitrary player-driven containment,
  material-derived softness, and other unprojected constructor/pocket semantics
  remain unavailable.
- Cities, roads, rivers, specials, extras, spawn groups/populations, zones,
  vehicles, adjacent overmaps, and additional generated z-levels remain
  unavailable.

## Latest verified checkpoint

The exact implementation commit `a80f6c2` passes formatting and diff checks,
workspace all-target/all-feature checking, strict Clippy, 388 workspace tests
plus doc-tests, warning-free rustdoc, dependency boundaries, parity ledger,
runtime progress, the 364-day astronomy table, all 7,992 vendored content
files, the 6,571-file schema inventory, and all three pinned C++ oracles: 8
pocket, 104 item-group, and 1,179 mapgen assertions. The item-group and mapgen
gates also run reusable direct Rust comparisons.

The representative empty-item-group-catalog fixture has unchanged Postcard
bytes. Hashing them under CanonicalStateV66 reproduces
`7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`;
the deliberate CanonicalStateV67 domain produces
`b5c12b763060907d68bfbd96b4aea6372c17cb02676b5e499b0bc79f5679899e`.
Serialized catalogs containing item prototypes do change shape. The event trace
and CanonicalEventsV18 remain unchanged.

An independent reviewer inspected the complete 21-file implementation diff
from a clean detached worktree at exact commit `a80f6c2`, tree `22de91c`. The
review covered content inheritance, protocol bounds, server normalization,
simulation capacity/sealing/order, stable IDs, persistence/replay, client and
four-mode paths, oracle evidence, documentation, and module growth. It found no
confirmed P0, P1, P2, or P3 issue. Concrete scope, rejected concerns, focused
commands, and residual risks are recorded in
[docs/reviews/protocol-91-default-container-ownership.md](docs/reviews/protocol-91-default-container-ownership.md).

## Next dependency boundary

After the audited checkpoint, implement generalized comestible temperature
state, rerun the complete field closure, admit the real field base, and
demonstrate ordinary client exploration and loot. Do not start cities, roads,
rivers, specials, anatomy, or EOCs first.
