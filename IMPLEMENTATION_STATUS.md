# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `d95790301fbe89b7eff291681d0135ab8d853480`,
  tree `7b19b5fecaa9b32879c3b8fa746c653920176866`.
- Active milestone: `regional-terrain-base`.
- Completed family checkpoint: generalized item-group ammunition loading for
  strict magazines and supported tools, with all gun charge modifiers retained
  fail closed.
- Current representation: protocol 90, worldgen algorithm 2, persistence
  schema/minimum recoverable schema 68, replay format 3, CanonicalStateV66,
  and CanonicalEventsV18. Scenario format 7 and observation format 6 are
  unchanged.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only; the server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

Mapgen/overmap completion remains split into bounded milestones:

- `atomic-static-mapgen`: `complete`.
- `omt-identities-routing`: `complete`.
- `start-location-selection`: `complete`.
- `regional-terrain-base`: `in_progress`; `accessory_weaponcarry` and
  `ammo_light_batteries` now admit. The complete field scan next stops at
  default-container ownership for `aspirin` in
  `bottle_otc_painkiller_1_20`.
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
production surface until its complete loot closure and ordinary exploration
loop are exact.

## Completed ammunition-loading family

- The existing owner-independent integral/detachable storage representation
  now normalizes explicit item-group charge ranges for strict magazines and
  supported tools. Zero stays empty, positive values load registry-default
  ammunition, and overflow clamps to exact capacity.
- `ammo_light_batteries` admits both 16-charge light and two-charge ultra-light
  integral magazines. Existing detachable-tool behavior remains exact.
- Every gun charge modifier fails closed before storage selection. Pinned
  integral guns retain owner-local state, while detachable guns use a distinct
  `ammo_set` selection and RNG path. Real `bbgun` with charges `0..150` is the
  production regression for this boundary.
- The 85-assertion pinned C++ oracle retains five direct magazine boundaries
  and four production `ammo_light_batteries` witnesses, including exact item
  and ammunition types, counts, remaining capacity, and downstream RNG. The
  reusable Rust comparator invokes the production constructor and charge
  planner.
- Direct, per-tick snapshot, SQLite, and portable replay conformance preserve
  nested ammunition and preorder stable IDs. The ordinary Bevy item menu
  renders `p0 16/16 battery` from replicated authoritative state.
- No serialized or wire shape changed. Protocol 90/schema 68 remain correct;
  the representative CanonicalStateV66 root remains
  `7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`.

## Measured progress and ownership

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

Current ownership sizes are 29,235 lines in `sim/lib.rs`, 4,183 in
`sim/items.rs`, 9,797 in `protocol/lib.rs`, 1,643 in
`protocol/item_groups.rs`, 13,065 in persistence, 8,797 in the server library,
and 1,315 in `server/item_groups.rs`. Relative to the verified parent, item
behavior grows the extracted item modules. Central simulation grows by three
lines solely for the direct-comparator export; central protocol, persistence,
and server libraries do not grow. The server executable growth is production
normalization/test evidence, and the tools growth is oracle evidence.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Explicit boundaries

- The real field is not runtime-admitted. Its first retained blocker is
  generalized default-container ownership for `aspirin` in
  `bottle_otc_painkiller_1_20`.
- Every gun charge modifier remains unavailable pending a dedicated owner-local
  and `ammo_set` engine with direct pinned C++ traces.
- The complete scan retains later food-temperature, corpse-construction,
  capacity-sentinel, wrapper-shape, dressing, and snippet families. They remain
  fail closed and must be implemented as generalized engines.
- Flexible physical spawn pockets, arbitrary player-driven containment,
  material-derived softness, and other unprojected constructor/pocket semantics
  remain unavailable.
- Cities, roads, rivers, specials, extras, spawn groups/populations, zones,
  vehicles, adjacent overmaps, and additional generated z-levels remain
  unavailable.

## Latest verification and review

The replacement checkpoint passes formatting and diff checks, workspace
all-target/all-feature checking, strict Clippy, 383 workspace tests plus
doc-tests, warning-free rustdoc, dependency boundaries, parity ledger, runtime
progress, the 364-day astronomy table, all 7,992 vendored content files, the
6,571-file schema inventory, and all three pinned C++ oracles: 8 pocket, 85
item-group, and 1,179 mapgen assertions.

No canonical hash changed. The representative fixture retains the
CanonicalStateV66 root above; hashing those Protocol-90 bytes under the old V65
domain still yields
`24e4298046769183c36ee47334b1acc628956a92fa2176dfff4deac32fbef2db`.
The event trace and CanonicalEventsV18 remain unchanged.

The first independent detached review of exact commit `60c048f` found one P1
integral-gun ownership/RNG defect and one P2 detachable-gun overclaim. Both were
validated against pinned upstream behavior and fixed by keeping all guns
closed. The fresh detached review of exact replacement `d957903` confirmed the
repairs and found no remaining P0-P3 issue. The complete audit is recorded in
[docs/reviews/item-group-ammunition-loading.md](docs/reviews/item-group-ammunition-loading.md).

## Next dependency boundary

Implement generalized default-container ownership, rerun the complete field
closure, and continue toward real-field generation plus ordinary client
exploration and loot. Do not start cities, roads, rivers, specials, anatomy, or
EOCs first.
