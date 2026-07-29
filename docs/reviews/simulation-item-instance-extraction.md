# Simulation item-instance extraction review

## Scope

- Reviewed base: `dd3c70cc4f9a2e2d3914c0a6c84f47b808e5ea06`
- Reviewed implementation commit:
  `58140edef392d98f8ad87a2224396b84c599382b`
- Complete implementation patch ID:
  `cc63516e9a47bebb676e969ccefcef6e93d4e131`
- Files: `crates/sim/src/items.rs`, `crates/sim/src/lib.rs`,
  `IMPLEMENTATION_STATUS.md`, and `docs/runtime-progress.json`.
- Reviewer: independent fresh-context subagent; no implementation edits.

## Mechanical-equivalence evidence

- After removing only the required `pub(super)` visibility tokens, the old and
  new `ItemInstance` struct and implementation have no textual difference.
- Derives, field names, field types, field order, method order, branches,
  mutations, validation calls, errors, and output ordering are unchanged.
- The moved code performs no RNG operation; the item-group planner is unchanged.
- Private power-pocket helpers remain private. The type, fields, and methods
  needed by the parent module are only `pub(super)`; no public crate API exists.
- Serde derives and field order are unchanged. Module location is not encoded.
- Protocol, schema, replay, event-hash, canonical-state, persistence, server,
  mapgen, and overmap sources are unchanged.

## Findings and resolutions

- P3: status initially described a 301-line implementation. The exact struct
  plus implementation is 300 lines; Git's 301-line deletion count includes its
  trailing separator. The wording was corrected and the final rescan was clean.

Every finding was checked against the current sources. The final independent
review found no remaining P0, P1, P2, or P3 issue.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features` — 335 tests plus doc-tests
- `RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --all-features --no-deps`
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, content-inventory, JSON, and diff checks
- pinned C++ pocket/item-group/mapgen oracles — 8/41/17 assertions
- selected-content manifest hash:
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`

## Residual risks

- Item validators, materialization/conversion helpers, ownership transfers, and
  item-bound activities still live in `sim/lib.rs` and retain direct access to
  `ItemInstance` fields. Later mechanical extractions should reduce that
  encapsulation surface before large item systems are added.
- Mapgen completion remains blocked on the explicit normalized Rust/C++
  comparator; this extraction does not change its `oracle_pending` state.
