# Protocol 93 flexible-containment review

## Frozen implementation

- Parent and merge base:
  `4ee1df59495691c256ccaa7dfd1c1dec0e6afded`.
- Reviewed implementation:
  `d863ea545ff5ab0ca18d95a14f06391a2ae3c6d7`, tree
  `6e016ac1f7881a76908ce1147329c0e4262b9c44`, stable patch ID
  `4a93aa3cd4b34960d56061bf0667b43cd645f04f`.
- Scope: the complete 19-file diff, 1,413 insertions and 217 deletions,
  covering strict content projection, protocol state, server normalization,
  simulation ownership and sizing, recovery validation, persistence,
  four-mode conformance, the ordinary Bevy client item path, exact C++ traces,
  direct Rust comparison, ledgers, denominators, and live documentation.
- Representation: protocol 93, persistence schema/minimum recoverable schema
  71, CanonicalStateV69, CanonicalEventsV18, replay format 3, worldgen
  algorithm 2, scenario format 7, and observation format 6.

## Fixed-tree review and repair

The first independent pass inspected commit
`a284fb1d219e6b2f36fa3bd912a9a44f4763f2f1`, tree
`449b7a13ebbd347db5ef68923be1c500bda0c4c4`, from clean detached worktree
`/tmp/cdda-protocol93-review.SYS0uD`. It reviewed the complete 19-file tree
without editing it and found one P2 recovery/data-integrity issue: the
simulation's duplicated pocket validator did not mirror Protocol 93's new
reserve, EFILE, and default-to-actual-collapse invariants. Production
construction was safe, but recovery could accept a canonical item state that
protocol validation rejected.

The confirmed finding was fixed before checkpoint binding. Simulation recovery
now enforces reserve below capacity, zero reserve for rigid and EFILE pockets,
no default-collapsed EFILE state, and actual collapse whenever the constructor
default requires it. Regression coverage checks every invalid rules shape and
mutates an actor inventory snapshot before calling `WorldState::from_snapshot`.

A final independent pass reviewed corrected commit `d863ea5`, tree
`6e016ac`, from clean detached worktree
`/tmp/cdda-protocol93-rereview.MddJZY`. It verified the repair against the
protocol predicate, reran the focused recovery path, reviewed the complete
final diff, and left the worktree clean. It found no remaining P0, P1, P2, or
P3 issue.

The pre-freeze full-diff audit also resolved two concrete issues before the
first commit: a stale simulation test still required flexible wrappers to fail
closed, and server normalization initially applied `COLLAPSE_CONTENTS` to
EFILE pockets. The test now characterizes generalized flexible ownership, and
only physical container pockets receive the collapse default. A candidate
concern about arbitrary dynamic insertion was rejected after tracing the
authoritative command path: it accepts reloadable ammunition-capacity pockets,
while generated physical spawn pockets remain non-reloadable.

## Characterization and representation audit

- The item-group oracle has 137 exact assertions. It retains minimum and
  maximum `chaw_wrapper_1_20`, `chewing_gum_full`, every existing
  default-container trace, exact ownership/order, constructor-default and
  actual collapse, sealing, capacity, volume, weight, seed, and downstream C++
  RNG evidence. The reusable comparator executes production Rust transitions.
- Direct, per-tick snapshot, SQLite, and portable replay modes preserve a
  flexible sealed wrapper with 45 ml reserve, mixed contents, nested collapsed
  ownership, stable IDs, and temperature. The normal Bevy item menu renders
  collapse state and removes contained items through authoritative identifiers.
- Strict admission adds `chaw_wrapper_1_20`, `chewing_gum_full`, and exactly six
  furniture bashes: `f_earthbag_half`, `f_earthbag_wall`,
  `f_exodii_charger`, `f_exodii_pump`, `f_pillow_fort`, and
  `f_string_dimension_pump`. The pinned furniture total is 530; the complete
  field scan stops exactly at `chewing_gum_full_caff` and its unimplemented
  material thermodynamics.
- The representative item-flow V69 hash is
  `5f662ff59bc4c66b4c7e0700fdb0838bf41bac385a513458531d5af255bc5456`.
  Hashing the unchanged Postcard bytes under V68 reproduces
  `ecf2ff2770054b46562dd7cad15c3aa9326586594374b2710af84754beef6a6a`.
  The state-domain bump is deliberate; event and replay representations remain
  unchanged.

## Module-growth audit

The implementation owns 4,997 lines in `sim/items.rs`, 2,013 in
`protocol/item_groups.rs`, and 1,464 in `server/item_groups.rs`. Relative to
parent `4ee1df5`, those modules change by +207, +14, and -7 lines.

Central `sim/lib.rs` grows 58 lines: four mechanical export/fixture lines and
54 lines for the independently requested recovery-invariant mirror plus its
regression. Runtime containment behavior remains in `sim/items.rs`. Central
`protocol/lib.rs` grows 89 serialized-field, validation, and fixture lines.
Persistence remains 13,071 lines; the server library remains 8,804 lines; the
server executable grows 59 normalization and selected-content audit lines.
This is the complete central-growth justification.

## Verification

The implementation and two-pass review cycle passed:

- `git diff --check` and `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-targets --all-features` -- 393 tests;
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`;
- dependency-boundary, 30-milestone parity-ledger, denominator-aware
  runtime-progress, astronomy-table, content-validation, content-inventory,
  JSON, and diff gates;
- selected content -- 7,992 files, manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
- content inventory -- 6,571 JSON files, 93,779 top-level objects, and 180
  definition types;
- pinned C++ pocket, item-group, and mapgen oracles -- 8, 137, and 1,179 exact
  assertions, with direct Rust comparisons for item groups and mapgen;
- independent focused content, protocol, simulation recovery, client menu,
  four-mode conformance, SQLite/replay, canonical-hash, ledger, and runtime
  checks.

## Residual boundary

Automatic collapse deliberately uses exact represented item equality rather
than the broader unported upstream `stacks_with` relation. Arbitrary dynamic
physical-pocket insertion also remains fail closed. The duplicated protocol and
simulation recovery predicates are equivalent now but carry maintenance drift
risk until validation ownership is consolidated.

These limitations earn no new runtime points before the ordinary field loop is
generated, looted, persisted, client-accessible, and four-mode proven. Material
thermodynamics for `caff_gum`, real-field admission, and ordinary client
exploration and loot are the next dependency boundary. Cities, roads, rivers,
specials, anatomy, and EOCs do not precede it.
