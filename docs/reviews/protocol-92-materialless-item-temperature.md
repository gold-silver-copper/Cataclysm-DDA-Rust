# Protocol 92 materialless item-temperature review

## Frozen implementation

- Parent and merge base:
  `e4f74aff4a84019c35818aa0a6746ce33bf309e8`.
- Reviewed implementation:
  `84509b8ebb7ceb6b68456e9473ea0816d2b24a80`, tree
  `367af56a854a6a47ccc0ef0eb99d111cf2f4664a`, stable patch ID
  `32ae084473940fef1c715372995d95bc7435433d`.
- Scope: the complete 23-file diff, 1,323 insertions and 245 deletions,
  covering finalized content classification, protocol state, server catalogs,
  simulation ownership and processing, persistence/recovery, four-mode
  conformance, the ordinary client item menu, the pinned C++ oracle/direct Rust
  comparison, ledgers, and live architecture documentation.
- Representation: protocol 92, persistence schema/minimum recoverable schema
  70, CanonicalStateV68, CanonicalEventsV18, replay format 3, worldgen
  algorithm 2, scenario format 7, and observation format 6.

## Independent fixed-tree review

An independent reviewer inspected exact commit `84509b8` from clean detached
worktree `/private/tmp/cdda-84509b8-review.pc04Le`. The tree stayed fixed and
clean, and the reviewer made no edits. No confirmed P0, P1, P2, or P3 issue was
found.

The review covered the complete classifier and admission boundary; crafting
and disassembly reductions; constructor state and validation; recursive item,
component, ammunition, magazine, and physical-content ownership; processing
cadence and future-timestamp rejection; serialization, hashing, recovery, and
replay; client display and stack equality; all four conformance modes; exact
C++ traces and direct Rust comparison; documentation; and central-module
growth.

The reviewer validated these concrete concerns against the committed code:

- The shared finalized-content classifier admits all and only the 36 selected
  materialless/nonperishable definitions. Material-backed thermodynamics, rot,
  custom freezing, and unsupported phases remain fail closed in item groups,
  crafting outputs/byproducts, disassembly targets, and recovered components.
- Constructor state owns the exact birth tick. Processing and recovery recurse
  through every represented canonical ownership location, including temporary
  activity ownership, and restoration rejects nested timestamps from the
  future.
- `None` post-initialization energy deliberately represents the pinned
  indeterminate materialless heat-capacity result without a platform-specific
  NaN payload. Temperature, phase, flags, and cadence remain explicit integers.
- Temperature participates in simulation and client stack identity; the normal
  authoritative Bevy item menu renders pending and initialized state.
- Adding optional temperature fields changes Postcard bytes even when absent.
  The old V67-domain hash of the new bytes and the V68-domain hash are pinned
  separately; the protocol/schema/state-domain bump is therefore required,
  while replay format and CanonicalEventsV18 correctly remain unchanged.
- Runtime progress remains 44 weighted points out of the explicit core-DDA
  denominator. Synthetic temperature characterization earns no production
  credit before the real field is generated, looted, persisted, four-mode
  proven, and client-accessible.

## Module-growth audit

The implementation owns 4,790 lines in `sim/items.rs`, 1,999 in
`protocol/item_groups.rs`, and 1,471 in `server/item_groups.rs`. Relative to
the prior verified implementation `a80f6c2`, those extracted item modules grow
by 245, 83, and 65 lines.

Central `sim/lib.rs` grows by 86 lines for birth-tick call sites,
canonical-owner visitation, recovery validation calls, stack equality, and
mechanical fixtures. Temperature arithmetic and recursive ownership stay in
`sim/items.rs`. Central `protocol/lib.rs` grows by 46 lines for serialized DTO
fields, validation, and fixtures. Persistence and the server library grow by
6 and 7 version/fixture lines. This is the complete central-growth
justification; future containment work remains assigned to the extracted item
modules.

## Verification

The implementation and review cycle passed:

- `git diff --check` and `cargo fmt --all -- --check`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-targets --all-features --no-fail-fast` -- 392
  tests;
- `cargo doc --workspace --all-features --no-deps` without warnings;
- dependency-boundary, parity-ledger, runtime-progress, astronomy-table,
  content-validation, content-inventory, JSON, and diff gates;
- selected content -- 7,992 files; inventory -- 6,571 JSON files, 93,779
  top-level objects, and 180 definition types;
- pinned C++ pocket, item-group, and mapgen oracles -- 8, 119, and 1,179 exact
  assertions, with direct Rust comparisons for item groups and mapgen;
- independent fixed-tree `cargo test --workspace temperature --all-features`;
- independent client item-menu/stack, named item-group four-mode, item-flow/hash
  four-mode, and full selected-content/catalog tests.

## Residual boundary

Material thermodynamics, rot, custom freezing points, weather-driven ambient
temperature, and unsupported phases remain deliberately fail closed. Flexible
physical wrappers are also outside this family. The complete field scan now
stops at `chaw_wrapper_1_20`; generalized flexible containment, real-field
admission, and ordinary client exploration and loot are the next dependency
boundary. Cities, roads, rivers, specials, anatomy, and EOCs do not precede it.
