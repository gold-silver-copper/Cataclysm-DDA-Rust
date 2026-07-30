# Protocol 94 nonperishable material-thermodynamics review

## Frozen implementation

- Parent and merge base:
  `3ed60345061ae31ceeae46fafbe6f1959b6a9039`.
- Final reviewed code commit:
  `40037fbb1db9eaac8d4889b811d29f8c00380e6b`, tree
  `a893a64a3928b299489d91ce66285a5534527f50`, stable patch ID
  `850f7eef4cc87fe0d4b3d119ba0ebf0ba98b984e`.
- Scope: the complete 19-file diff, 1,715 insertions and 203 deletions,
  covering the selected-content material engine, protocol state, simulation
  processing and recovery, server normalization, persistence, four-mode
  conformance, the ordinary Bevy client item path, exact C++ traces, direct
  Rust comparison, ledgers, denominators, and live documentation.
- Representation: protocol 94, persistence schema/minimum recoverable schema
  72, CanonicalStateV70, CanonicalEventsV18, replay format 3, worldgen
  algorithm 2, scenario format 7, and observation format 6.

## Fixed-tree review and repair

The first independent pass inspected implementation commit
`ef3480980cc45747fa647e86887d254702af6600`, tree
`b378e638aebd95dc645a704a5b546f228d36fc24`, from clean detached worktree
`/tmp/cdda-protocol94-review-ef34809`. It reviewed the complete 19-file tree
without editing it and reported these confirmed findings:

- P1: simulation and persistence recovery checked temperature timestamps but
  not the complete canonical temperature state, allowing malformed energy,
  flags, phases, properties, or temperature on a non-comestible.
- P2: simulation craft/provenance validation did not mirror Protocol 94's
  thermal property and tracking invariants.
- P2: the material registry exposed abstract templates as concrete materials,
  accepted ambiguous identities, and silently ignored thermal modifiers.
- P3: the `f64` upper guard could accept the `2^63` alias before a saturating
  integer cast.
- P3: the parity milestone omitted the upstream material and generic-factory
  sources that define the implemented inheritance/default behavior.

All five findings were validated against the pinned C++ and current recovery
call chains before repair. Protocol now exports one shared bounded thermal-state
predicate; simulation snapshot, component, prototype, and provenance recovery
enforce it; and a live malformed `WorldState::from_snapshot` regression covers
the persistence path. Material abstracts and concrete definitions use separate
maps, both `id` and `abstract` is rejected, thermal modifiers fail closed, the
concrete registry count is 201, and both integer upper guards reject the alias.
The ledger names `generic_factory.h`, `material.h`, and `material.cpp`.

The first corrected-tree review inspected commit `7b57530`, tree `a1b3cbd`,
and found one further P2 relation: individually valid temperature state could
claim a phase different from its immutable containment phase. It also identified
stale module-growth figures. The shared predicate now requires
`current_phase == containment.phase`; protocol and simulation snapshot,
component, prototype, provenance, direct-validator, and live-recovery tests all
cover the mismatch. Growth accounting was updated to the exact final tree.

The final independent pass reviewed replacement commit `40037fb`, tree
`a893a64`, from clean detached worktree
`/tmp/cdda-protocol94-rereview-40037fb`. It rechecked every prior finding and
the complete final diff, ran focused negative recovery, protocol lifecycle, and
material tests, parsed the JSON documents, left the worktree clean, and found
no remaining P0, P1, P2, or P3 issue.

## Characterization and representation audit

- Six exact constructor traces distinguish materialless, ordinary-material,
  field-blocker, weighted-mixture, `NO_TEMP`, and ordinary-control cases. The
  item-group oracle now passes 144 C++ assertions plus reusable direct Rust
  comparison. The weighted saline trace pins the hidden `330092987`
  microjoule-per-gram `float` quantization boundary and exact ambient energy.
- Direct, per-tick snapshot, SQLite, and portable replay modes preserve a
  generated material-backed item and its complete thermal profile. The normal
  Bevy menu distinguishes the unprocessed sentinel from initialized numeric
  material energy and renders the latter as 20 C.
- Strict admission adds 278 nonperishable/default-freezing material-backed
  constructors, exactly four attributable furniture bashes, and 197
  attributable recipes. Exact owner/difference checks explain the changed
  aggregates. The production totals are 534 furniture bashes and 2,826
  recipes. The complete field scan stops exactly at
  `civilian_eink_tablet_pcs` and its unsupported charge-capacity sentinel.
- The representative item-flow V70 hash is
  `c073bebfd0e27fddc776df558cdc9fe8a7c11a86f858fe8fa0af0a4f04ee6d08`.
  Hashing the same Postcard bytes under V69 reproduces
  `5f662ff59bc4c66b4c7e0700fdb0838bf41bac385a513458531d5af255bc5456`.
  The changed hash is the deliberate canonical domain bump; event and replay
  representations are unchanged.

## Module-growth audit

The final tree owns 5,047 lines in `sim/items.rs`, 2,101 in
`protocol/item_groups.rs`, 1,527 in `server/item_groups.rs`, and 596 in the new
`content/material.rs`. Relative to verified implementation `d863ea5`, these
modules change by +50, +88, +63, and +596 net lines.

Central `sim/lib.rs` is 29,592 lines and grows 199 net lines. That exception is
limited to mirroring protocol snapshot/component/prototype/provenance recovery
invariants and the independently required direct and live negative recovery
regressions; temperature processing behavior remains in `sim/items.rs`.
Central `protocol/lib.rs` is 9,973 lines and grows 37 net wire, validation, and
fixture lines. Persistence is 13,077 lines; the server library is 8,807 lines;
and the server executable is 7,070 lines, growing 107 net registry-threading,
catalog-integration, and fixed production-audit lines. No temperature runtime
behavior was added to the central server library.

## Verification

The implementation and review/fix cycle passed:

- `git diff --check` and `cargo fmt --all -- --check`;
- `cargo test --workspace --all-targets --all-features` -- 395 tests on the
  repaired tree, followed by all 210 affected protocol, simulation, and
  conformance tests after the final phase-ownership repair;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` on the
  final replacement;
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps` on
  the final replacement;
- the 121-second selected production-content scan with exact 278/534/2,826
  counts and unchanged next blocker;
- dependency-boundary, 31-milestone parity-ledger, denominator-aware
  runtime-progress, astronomy-table, content-validation, content-inventory,
  JSON, and diff gates;
- selected content -- 7,992 files, manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
- content inventory -- 6,571 JSON files, 93,779 top-level objects, and 180
  definition types;
- pinned C++ pocket, item-group, and mapgen oracles, with direct Rust
  comparisons for item groups and mapgen;
- final independent focused material, protocol, negative simulation recovery,
  canonical-hash, ledger, and JSON review checks.

## Residual boundary

Rot, custom item freezing points, and thermal modifiers remain explicit
fail-closed families. Material definitions outside the selected production scan
remain a content-expansion risk, not admitted behavior. The protocol and
simulation recovery checks now share the state/phase predicate, but their
relational callers still require maintenance until validation ownership is
further consolidated.

No runtime points are awarded before the ordinary field loop is generated,
looted, persisted, client-accessible, and four-mode proven. The next dependency
is the generalized item-group charge-capacity sentinel family, followed by real
field admission and ordinary client exploration/loot. Cities, roads, rivers,
specials, anatomy, and EOCs do not precede it.
