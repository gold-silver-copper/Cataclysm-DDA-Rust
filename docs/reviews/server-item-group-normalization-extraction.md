# Server item-group normalization extraction review

## Scope

- Reviewed base: `d219c8c69290fc42ee697005ba1cae81ae469001`
- Reviewed implementation commit:
  `d76965c54d5fee3a081b2a7c860b94a750b92cdd`
- Complete implementation patch ID:
  `4eb61e60564bbf7d83b119f9db75eb5e9585185b`
- Files: `crates/server/src/main.rs`,
  `crates/server/src/item_groups.rs`, `IMPLEMENTATION_STATUS.md`,
  `docs/parity-ledger.json`, and `docs/runtime-progress.json`.
- Reviewer: independent fresh-context subagent; no implementation edits.

## Mechanical-equivalence evidence

- The complete 333-line normalization implementation compares byte-for-byte
  after removing only the four required `pub(super)` visibility tokens.
- Branches, error strings, source and output order, `BTreeMap`/`BTreeSet`
  traversal, conversions, bounds, and RNG-admission behavior are unchanged.
- No normalization function was omitted or duplicated. Shared item-prototype
  and default-charge helpers remain parent-owned because unrelated server
  consumers still use them.
- `main.rs` falls from 6,565 to 6,240 lines; `item_groups.rs` is 349 lines.
- Protocol 84, worldgen algorithm 2, schema/minimum schema 62, replay 3,
  CanonicalStateV60, CanonicalEventsV18, and conformance 7/6 are unchanged.

## Findings and resolutions

- P3: `runtime_strict_item_group_catalog` was initially widened to
  `pub(super)` even though its only caller is in the new module. The finding
  was validated and the function restored to module-private visibility.

Every finding was checked against the current sources. The final independent
five-file review found no remaining P0, P1, P2, or P3 issue.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features` — 335 tests plus doc-tests
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, content-inventory, JSON, and diff checks
- pinned C++ pocket/item-group/mapgen oracles — 8/41/17 assertions
- selected-content manifest hash:
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`
- pinned upstream checkout:
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, with relevant tracked sources
  clean

## Residual risks

- The focused module still calls parent-owned item-prototype and default-charge
  helpers. That boundary should move with the next item representation family,
  not as unrelated refactoring.
- Item-group normalization tests remain in `main.rs`, requiring two limited
  test-facing `pub(super)` helpers.
- This extraction does not complete modifier or general containment semantics;
  the `field -> everyday_corpse` closure remains fail-closed.
