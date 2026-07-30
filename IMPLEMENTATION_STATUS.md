# Implementation Status

Upstream is fixed at commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05`,
tree `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `40037fbb1db9eaac8d4889b811d29f8c00380e6b`,
  tree `a893a64a3928b299489d91ce66285a5534527f50`.
- Audited checkpoint: generalized nonperishable material thermodynamics. The
  fixed-tree review
  and repair cycle is recorded in
  `docs/reviews/protocol-94-nonperishable-material-thermodynamics.md`.
- Checkpoint representation: protocol 94, persistence schema/minimum recoverable
  schema 72, CanonicalStateV70, CanonicalEventsV18, replay format 3, worldgen
  algorithm 2, scenario format 7, observation format 6.
- Candidate representation under verification: protocol 95, persistence
  schema/minimum recoverable schema 73, CanonicalStateV71, with event/replay/
  worldgen/scenario/observation formats unchanged.
- Active milestone: `regional-terrain-base`.
- Hosts remain macOS, Linux, and Windows. Bevy 0.19 is client-only; server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

Mapgen/overmap milestone states:

- `atomic-static-mapgen`: `complete`.
- `omt-identities-routing`: `complete`.
- `start-location-selection`: `complete`.
- `regional-terrain-base`: `in_progress`.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

## Candidate family: item-group charge-capacity sentinels

- Raw lower and upper charge endpoints remain serialized until the concrete
  item and modifier container are known. Explicit capacity ownership selects
  integral/detachable ammunition storage, a physical modifier container, or the
  exact no-capacity no-op; unsupported randomized ordinary-item modifiers still
  fail closed.
- Eleven exact C++ traces cover both ends of integral-tool, detachable-tool,
  magazine, container, lower-sentinel, and unresolved ordinary behavior. The
  oracle derives effective bounds from pinned item APIs and then compares them
  directly with the generalized Rust resolver. Exact e-ink witnesses retain 0
  and 85 loaded battery charges plus downstream RNG state.
- A real `eink_tablet_pc` with its integral battery is identical through direct,
  per-tick snapshot, SQLite, and portable replay modes. The production-content
  audit admits `civilian_eink_tablet_pcs` and verifies raw `[0, -1]` ownership
  instead of accepting a changed aggregate count.

## Runnable behavior and next boundary

The persistent authoritative server authenticates through iroh, creates durable
characters, advances with zero players, keeps disconnected characters present
and vulnerable, traverses generated terrain, fights admitted creatures,
manipulates nested items, and runs implemented crafting, reading, disassembly,
construction, recovery, and replay paths.

The complete `field` scan now passes `civilian_eink_tablet_pcs` and stops exactly
at `costume_accessories`: wrapper `leg_sheath6` has six physical sheath pockets,
while the current generalized insertion engine owns exactly one. The next
playable unlock is multi-pocket wrapper selection, complete real-field
admission, and ordinary client exploration/loot. Do not start cities, roads,
rivers, specials, anatomy, or EOCs first.

## Measured runtime progress

No points are awarded for parser admission or synthetic characterization. The
newly admitted definitions do not earn denominator credit until the real field
is generated, interacted with, persisted, client-accessible, and four-mode
proven as an ordinary gameplay surface.

- Core-DDA ordinary-gameplay denominator: 13,865 definitions and 263,435
  possible weighted points; 44 earned (0.0167%).
- Selectable bundled-mod denominator: 5,967 definitions and 113,373 possible
  weighted points; zero earned.
- Parser inventory remains separate: 7,621 item groups, 9,520 mapgen objects,
  2,712 OMTs, and 150 starts.

## Module-growth budget

Candidate sizes are 29,594 lines in `sim/lib.rs`, 5,216 in `sim/items.rs`,
9,974 in `protocol/lib.rs`, 2,191 in `protocol/item_groups.rs`, 13,077 in
persistence, 8,807 in the server library, 1,535 in `server/item_groups.rs`, and
7,110 in the server executable. Relative to verified implementation `40037fb`,
the central simulation grows two net lines for the resolver export and
canonical-domain bump; charge behavior grows `sim/items.rs` by 169 net lines.
The central protocol grows one net export/version line while validation and
metrics grow `protocol/item_groups.rs` by 90 net lines. Capability projection
grows `server/item_groups.rs` by eight net lines; the server executable grows 40
net test/audit lines and no generalized behavior. Persistence and the server
library do not grow. New behavior therefore stays in the requested ownership
modules, and every central-file increase is mechanical or an exact production
boundary.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Latest verification

Passing checks on the candidate implementation:

- formatting and diff checks;
- raw sentinel validation, capacity ownership, and exact simulation boundaries;
- named item-group direct/snapshot/SQLite/portable-replay conformance;
- representative item-flow four-mode conformance and explicit V70/V71 domain
  audit;
- schema-73 persistence tests;
- deterministic selected-content scan through `civilian_eink_tablet_pcs` to
  the exact six-pocket `leg_sheath6` boundary;
- pinned item-group C++ oracle: 239 assertions plus direct Rust comparison;
- pinned pocket and static-mapgen C++ oracles, including the reusable direct
  mapgen comparator;
- dependency boundaries, parity ledger, runtime denominator, astronomy table,
  content validation, and content inventory gates;
- all 398 workspace target/feature tests, strict workspace Clippy, and rustdoc
  with warnings denied.

The V71 item-flow fixture hash is
`2739a453fb700b8f3118f69631926f5ef0baad7ab5eb7c0434a82b372e8af5ab`.
Hashing the same Postcard bytes under V70 yields
`c073bebfd0e27fddc776df558cdc9fe8a7c11a86f858fe8fa0af0a4f04ee6d08`;
the changed hash is therefore the deliberate domain bump for the newly
serialized item-group representation. CanonicalEventsV18 is unchanged.

The broad workspace gates pass. A fixed committed-tree independent review
remains before this candidate becomes the live verified checkpoint. No failure
is known.
