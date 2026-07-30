# Implementation Status

Upstream is fixed at commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05`,
tree `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Last audited green implementation: `84509b8ebb7ceb6b68456e9473ea0816d2b24a80`,
  tree `367af56a854a6a47ccc0ef0eb99d111cf2f4664a`.
- Active candidate: generalized flexible containment, not yet a green checkpoint
  until broad local gates and a fixed committed-tree review finish.
- Candidate representation: protocol 93, persistence schema/minimum recoverable
  schema 71, CanonicalStateV69, CanonicalEventsV18, replay format 3, worldgen
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

## Active family: flexible containment

- One generalized engine now owns flexible physical pockets, checked reserved
  base volume, insertion/fit accounting, constructor collapse, and automatic
  collapse after a homogeneous fill. Unsupported shapes and broader stack
  equivalence remain fail closed.
- Exact C++ observations distinguish constructor-default from actual collapse.
  Minimum/maximum `chaw_wrapper_1_20`, `chewing_gum_full`, all seven
  default-container traces, and existing overflow witnesses pass 137 assertions
  plus the reusable direct production Rust comparison.
- Direct, per-tick snapshot, SQLite, and portable replay modes preserve a mixed
  flexible sealed wrapper, 45 ml reserve, nested auto-collapsed bottle, stable
  IDs, and temperature. The normal Bevy menu renders collapsed pockets and
  retains authoritative contained-item removal.
- Selected content admits `chaw_wrapper_1_20`, `chewing_gum_full`, and exactly
  six new furniture bashes: `f_earthbag_half`, `f_earthbag_wall`,
  `f_exodii_charger`, `f_exodii_pump`, `f_pillow_fort`, and
  `f_string_dimension_pump`. The furniture total is 530.

## Runnable behavior and next boundary

The persistent authoritative server authenticates through iroh, creates durable
characters, advances with zero players, keeps disconnected characters present
and vulnerable, traverses generated terrain, fights admitted creatures,
manipulates nested items, and runs implemented crafting, reading, disassembly,
construction, recovery, and replay paths.

The complete `field` scan now stops exactly at `chewing_gum_full_caff` because
`caff_gum` requires material thermodynamics. The next playable unlock remains
admitting the real field base and demonstrating ordinary exploration and loot
through the client. Do not start cities, roads, rivers, specials, anatomy, or
EOCs first.

## Measured runtime progress

No points are awarded for parser admission or synthetic characterization. The
new definitions do not earn credit until the real field is generated,
interacted with, persisted, client-accessible, and four-mode proven.

- Core-DDA ordinary-gameplay denominator: 13,865 definitions and 263,435
  possible weighted points; 44 earned (0.0167%).
- Selectable bundled-mod denominator: 5,967 definitions and 113,373 possible
  weighted points; zero earned.
- Parser inventory remains separate: 7,621 item groups, 9,520 mapgen objects,
  2,712 OMTs, and 150 starts.

## Module-growth budget

Candidate sizes are 29,393 lines in `sim/lib.rs`, 4,997 in `sim/items.rs`,
9,936 in `protocol/lib.rs`, 2,013 in `protocol/item_groups.rs`, 13,071 in
persistence, 8,804 in the server library, 1,464 in `server/item_groups.rs`, and
6,963 in the server executable. Relative to parent `4ee1df5`, central
`sim/lib.rs` grows 58 lines: four mechanical export/fixture lines plus the
Protocol 93 recovery-invariant mirror and its fixed-snapshot regression added
after independent review found the prior validator incomplete. Item behavior
remains in `sim/items.rs`, which grows 207 lines. Central `protocol/lib.rs`
grows 89 serialized-field,
validation, and fixture lines; `protocol/item_groups.rs` grows 14 lines for the
shared volume engine. `server/item_groups.rs` shrinks seven lines; the server
executable grows 59 mapping and selected-content audit lines. No containment
runtime behavior was added to a central simulation or server library monolith;
the central simulation delta is limited to recovery validation and its test.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Latest candidate verification

Passing checks on the current candidate:

- formatting and diff checks;
- content flexible-pocket validation;
- protocol pocket round trip and bounds;
- direct default-container simulation projection;
- normal Bevy contained-item menu/action path;
- named item-group direct/snapshot/SQLite/portable-replay conformance;
- representative item-flow four-mode conformance and explicit V68/V69 hash
  audit;
- deterministic selected-content catalog test with 530 exact furniture bashes
  and `chewing_gum_full_caff` as the next failure;
- pinned item-group C++ oracle: 137 assertions plus direct Rust comparison;
- all 393 workspace target/feature tests and strict workspace Clippy.

The V69 fixture hash is
`5f662ff59bc4c66b4c7e0700fdb0838bf41bac385a513458531d5af255bc5456`.
Hashing the unchanged Postcard bytes under V68 still yields
`ecf2ff2770054b46562dd7cad15c3aa9326586594374b2710af84754beef6a6a`;
this proves the changed canonical hash is the deliberate domain bump rather
than an uninvestigated byte change. CanonicalEventsV18 is unchanged.

The broad workspace gates pass. The first fixed committed-tree review found one
P2 recovery-validation gap; its confirmed fix now passes the focused recovery
regression and the broad suite. A final independent review of the corrected
fixed commit is the remaining checkpoint gate. No known failure remains.
