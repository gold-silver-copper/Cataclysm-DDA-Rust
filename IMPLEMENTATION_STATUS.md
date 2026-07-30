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

## Active family: nonperishable material thermodynamics

- A generalized selected-content material registry resolves inheritance,
  defaults, positive item portions, upstream `float` accumulation, and fixed
  microjoule profiles. The weighted `saline` trace pins the otherwise hidden
  `330092987` microjoule quantization boundary; materialless constructors remain
  distinct, while rot, custom freezing, and unsupported phases fail closed.
- Protocol 94/schema 72/CanonicalStateV70 carry self-contained thermal
  properties and accept only the characterized sentinel-to-20 C lifecycle.
  Material-backed ambient energy is numeric; materialless indeterminate energy
  remains `None` without serializing NaN.
- Six exact constructor traces, including `caff_gum`, `water_clean`, and the
  weighted saline boundary, bring the item-group oracle to 144 C++ assertions
  and pass the reusable direct Rust comparison. A material-backed generated
  item is identical through direct, per-tick snapshot, SQLite, and portable
  replay modes, and the normal Bevy item menu renders its initialized numeric
  energy as 20 C rather than pending.
- Selected content admits all 278 nonperishable/default-freezing
  material-backed constructors, exactly four attributable furniture bashes,
  and 197 attributable recipes. Those aggregate changes are checked against
  the previous materialless boundary and exact owners rather than accepted
  mechanically.

## Runnable behavior and next boundary

The persistent authoritative server authenticates through iroh, creates durable
characters, advances with zero players, keeps disconnected characters present
and vulnerable, traverses generated terrain, fights admitted creatures,
manipulates nested items, and runs implemented crafting, reading, disassembly,
construction, recovery, and replay paths.

The complete `field` scan now passes `chewing_gum_full_caff` and stops exactly
at `civilian_eink_tablet_pcs`: `eink_tablet_pc` uses the upstream item-group
charge capacity sentinel. The next playable unlock remains the generalized
capacity-sentinel family, complete real-field admission, and ordinary client
exploration/loot. Do not start cities, roads, rivers, specials, anatomy, or EOCs
first.

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

Verified sizes are 29,592 lines in `sim/lib.rs`, 5,047 in `sim/items.rs`,
9,973 in `protocol/lib.rs`, 2,101 in `protocol/item_groups.rs`, 13,077 in
persistence, 8,807 in the server library, 1,527 in `server/item_groups.rs`,
7,070 in the server executable, and 596 in the new `content/material.rs`.
Relative to verified implementation `d863ea5`, the central simulation grows
199 net lines to mirror Protocol 94's snapshot, component, prototype,
phase-ownership, and provenance invariants and to retain the independently
requested negative recovery regression, mechanical fixtures, and canonical
domain. Temperature processing behavior grows
`sim/items.rs` by 50 net lines. The central protocol grows 37 net wire,
validation, and fixture lines while `protocol/item_groups.rs` grows 88 net
engine/validation lines. The server executable grows 107 net lines solely for
registry threading, catalog integration, and the fixed production-content
audit; capability logic grows `server/item_groups.rs` by 63 net lines. The
server library grows three fixture lines and no runtime behavior. This larger
central simulation exception is confined to fail-closed recovery validation;
future temperature behavior remains owned by `sim/items.rs`.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Latest verification

Passing checks on the verified implementation:

- formatting and diff checks;
- content material inheritance and exact weighted profiles;
- protocol and simulation constructor/processing boundaries;
- negative snapshot/component/prototype recovery and immutable phase ownership;
- normal Bevy pending/initialized material-energy rendering;
- named item-group direct/snapshot/SQLite/portable-replay conformance;
- representative item-flow four-mode conformance and explicit V69/V70 domain
  audit;
- schema-72 persistence tests;
- deterministic selected-content scan with 278 material constructors, 534
  exact furniture bashes, 2,826 recipes, and the exact next field failure;
- pinned item-group C++ oracle: 144 assertions plus direct Rust comparison;
- pinned static-mapgen C++ oracle plus reusable direct Rust comparison;
- all 395 workspace target/feature tests, strict workspace Clippy, and rustdoc
  with warnings denied.

The V70 item-flow fixture hash is
`c073bebfd0e27fddc776df558cdc9fe8a7c11a86f858fe8fa0af0a4f04ee6d08`.
Hashing the same Postcard bytes under V69 still yields
`5f662ff59bc4c66b4c7e0700fdb0838bf41bac385a513458531d5af255bc5456`;
the changed hash is therefore the deliberate domain bump for the newly
serialized item family. CanonicalEventsV18 is unchanged.

The first fixed committed-tree review found one P1 recovery-integrity issue,
two P2 generalized-validation issues, and two P3 boundary/documentation issues.
The first corrected tree then exposed one P2 immutable-phase relation and stale
growth accounting. All confirmed findings were fixed. A final independent pass
reviewed the complete replacement commit from a clean detached worktree and
found no remaining P0-P3 issue. No known failure remains.
