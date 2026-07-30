# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `d115312105b6c340884e73ffdf6b9d12c541a4c4`,
  tree `14405a785f10ecc7ee0ec2bbf9006c68b0de91d6`.
- Active milestone: `regional-terrain-base`.
- Completed family checkpoint: generalized variable-size FIT construction,
  crafting/disassembly propagation, persistence, and client presentation.
- Current representation: protocol 90, worldgen algorithm 2, persistence
  schema/minimum recoverable schema 68, replay format 3, CanonicalStateV66,
  and CanonicalEventsV18. Scenario format 7 and observation format 6 are
  unchanged.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only; the server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

Mapgen/overmap completion is intentionally split:

- `atomic-static-mapgen`: `complete`.
- `omt-identities-routing`: `complete`.
- `start-location-selection`: `complete`.
- `regional-terrain-base`: `in_progress`; `accessory_weaponcarry` and
  `leg_sheath6` now admit, and the exact field closure next stops at
  `ammo_light_batteries` because charge modifiers require generalized
  ammunition loading.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

Mapgen completion evidence is in
[docs/reviews/mapgen-direct-comparator-completion.md](docs/reviews/mapgen-direct-comparator-completion.md).
Historical protocol narration remains in `docs/history` and scoped reviews.

## Runnable behavior

The persistent server-authoritative multiplayer foundation authenticates with
iroh, creates and selects durable characters, keeps the world running without
players, traverses generated terrain, fights admitted creatures, manipulates
visible items, smashes admitted structures, and runs implemented crafting,
reading, disassembly, construction, survival, recovery, and replay paths.
Disconnected characters remain physically present and vulnerable.

Worlds retain a bounded 180x180 z=0 overmap currently filled by the admitted
`lmoe_north` generator. Coordinate-owned OMT identities, atomic 24x24
generation, rotation, server-selected starts, durable chunks, and blocked
layout edges are runnable. The real default `field` layer remains outside the
production surface until its complete loot closure and client exploration loop
are exact.

## Completed variable-size FIT family

- `VARSIZE` remains an immutable finalized item capability; per-instance `FIT`
  is canonical, validated, nested with ownership, and preserved in component
  provenance.
- Every direct item-group leaf consumes the pinned one-in-three FIT phase.
  Only variable-size items change state, already-fitted items remain fitted,
  named groups do not add a second phase, and raw wrappers do not own the leaf
  phase.
- Crafted variable-size primary outputs and byproducts are always fitted.
  Exact disassembly components preserve their state; default variable-size
  components inherit FIT only from a fitted target.
- The 80-assertion pinned C++ oracle retains exact direct witnesses at seeds 1
  and 2, a same-draw non-variable control, and production
  `accessory_weaponcarry` witnesses at seeds 219 and 97. Item type, capability,
  FIT state, rendered name, and downstream draw are fixed. The Rust comparator
  executes the reusable transition directly.
- Direct, per-tick snapshot, SQLite, and portable replay conformance preserve
  the state. The ordinary Bevy item menu renders `(poor fit)` solely from the
  replicated snapshot.
- Production normalization admits `leg_sheath6`; the deterministic field scan
  retains `ammo_light_batteries` ammunition loading as the next boundary.

## Measured progress

Runtime credit remains four core definitions and 44 weighted points. FIT
unlocks more strict normalization but earns no production credit until the real
field is generated, explored, looted, persisted, exercised in all four recovery
modes, and accessible through the client.

- Core DDA ordinary-gameplay target: 13,865 definitions and 263,435 possible
  weighted points; 44 earned (0.0167%).
- Selectable bundled-mod target: 5,967 definitions and 113,373 possible
  weighted points; zero earned.
- Parser-only inventory remains separate: 7,621 item groups, 9,520 mapgen
  objects, 2,712 OMTs, and 150 starts.

Current ownership sizes are 29,232 lines in `sim/lib.rs`, 4,132 in
`sim/items.rs`, 9,797 in `protocol/lib.rs`, 1,641 in
`protocol/item_groups.rs`, 13,065 in persistence, 8,797 in the server library,
and 1,286 in `server/item_groups.rs`. Relative to the verified parent,
`sim/items.rs` owns the generalized transition, item materialization, and its
phase tests; `protocol/item_groups.rs` owns FIT capability validation; and
`server/item_groups.rs` removes the obsolete admission guard. Central-file
changes are restricted to the canonical field and version, mechanical fixture
initializers, crafting/disassembly coordination, and focused integration tests.
Moving both item materializers into `sim/items.rs` makes the central simulation
file smaller than its parent despite the new family.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Explicit boundaries

- The real field is not runtime-admitted. Its next exact retained blocker is
  ammunition loading for `light_battery_cell` in `ammo_light_batteries`.
- The complete scan also retains separate default-container, food-temperature,
  corpse-construction, capacity-sentinel, wrapper-shape, dressing, and snippet
  families. They remain fail closed and must be implemented as generalized
  engines rather than individual definitions.
- Flexible physical spawn pockets, arbitrary player-driven containment,
  material-derived softness, and other unprojected constructor/pocket semantics
  remain unavailable.
- Cities, roads, rivers, specials, extras, spawn groups/populations, zones,
  vehicles, adjacent overmaps, and additional generated z-levels remain
  unavailable.
- Rich start scoring, nested/update mapgen, parameters, multi-layer glyphs,
  weighted one-time fill, and recursive regional targets remain unavailable.

## Latest verification

The implementation checkpoint passes formatting and diff checks, workspace
all-target/all-feature checking, strict Clippy, 382 workspace tests,
warning-free rustdoc, dependency boundaries, parity ledger, runtime progress,
the 364-day astronomy table, all 7,992 vendored content files, the 6,571-file
schema inventory, and all three pinned C++ oracles (8 pocket, 80 item-group,
and 1,179 mapgen assertions). The complete 382-test suite was rerun after the
containment-identity fix. After the final immutable-FIT validation repair, the
affected 38 protocol tests, all 156 simulation tests, the pinned production
content server test, and strict full-workspace Clippy were rerun.

The representative fixture changes from the Protocol-89 V65 root
`0878f47b5e8e159fdee5a57a6c7f90bab5e13e6bb944820a10585b835fb857be`.
Hashing the Protocol-90 bytes under the old V65 domain yields
`24e4298046769183c36ee47334b1acc628956a92fa2176dfff4deac32fbef2db`,
proving the Postcard representation changed. CanonicalStateV66 then yields
`7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`.
The event trace and CanonicalEventsV18 are unchanged.

The first independent detached review of exact commit `8a444f6` found no
P0/P1 and two P2 defects: planned count-by-charge containment merging omitted
FIT identity, and validation admitted `fitted=false` for immutable `FIT`.
Both generic custom/restored-domain defects are fixed and regressed in the
replacement commit above; the pinned recommended registry contains zero
definitions that trigger either shape. The replacement fixed-tree review is
recorded in
[docs/reviews/protocol-90-variable-size-fit.md](docs/reviews/protocol-90-variable-size-fit.md):
the fresh detached review of exact replacement `d115312` confirmed both fixes
and found no remaining P0-P3 issue.

## Next dependency boundary

Implement generalized ammunition loading for battery magazines, then rerun the
complete field closure. Do not start cities, roads, rivers, specials, anatomy,
or EOCs first. The next playable unlock remains the real field plus ordinary
client exploration and loot.
