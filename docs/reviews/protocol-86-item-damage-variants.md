# Protocol 86 item damage and variants checkpoint review

## Scope

- Reviewed base: `f882a5d46e8d27163399b97c5ffaf6f0bda67320`.
- Reviewed implementation commit:
  `cc34f40d79f9323f395643b7fa4c3d23127eb9e7` (`Represent exact item damage
  and variants`).
- Complete implementation patch ID:
  `8c04d78d645b69b36cddc2d293248e4b7f8f50ec`.
- Files: 23 tracked files across client presentation, ITEM content
  finalization, protocol item groups and canonical state, simulation items and
  corpse creation, server item-group normalization and admission, persistence,
  conformance, the pinned C++ oracle, generated evidence, and operational
  documentation.
- Reviewer: independent fresh-context subagent; no implementation edits.
- Upstream reference: clean commit
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Findings and resolutions

- P1: direct item modifiers rolled charges after magazine dressing. Generation
  now applies damage and variant state, rolls ranged charges, and only then
  performs magazine dressing; an exact seed-2 test pins the phase order.
- P1: missing or empty alternate variant name, description, and art did not
  fall back to finalized base ITEM values. Finalization now matches the pinned
  source behavior, including append composition, and server tests cover it.
- P1: ordinary corpse creation reconstructed the minimum raw damage for a
  display level and therefore lost exact overkill state. It now preserves the
  pinned float32-derived raw value through live state, SQLite recovery, and
  portable replay.
- P1: the reserved `<any>` variant request did not perform upstream's second
  weighted selection. The planner now consumes the exact draw, reselects from
  finalized variants, and retains the existing selection when every weight is
  zero.
- P2: protocol variant-weight totals could exceed the upstream signed integer
  domain. Validation now bounds the total to `i32::MAX`.
- P2: raw-damage cap validation accepted intermediate values. Only the exact
  supported endpoints, 0 and 4000, are admitted.
- P2: duplicate variant validation was quadratic. It now uses a `BTreeSet`, and
  the maximum 256-variant shape has a boundary test.
- P2: an integer-rational approximation of corpse damage could disagree with
  upstream float32 rounding. The implementation now uses pinned float32
  semantics; the 625-HP/251-overflow raw-1003 witness is explicit.
- P2: a literal selectable variant option named `<any>` could collide with the
  reserved request. Protocol/catalog validation rejects it.
- P2: generated inventory and operational documentation retained stale support
  states and counts. They now record the three ITEM fields, 51 oracle
  assertions, exactly 524 admitted furniture bashes, and the audited canonical
  hash transition.

Every finding was validated against the current implementation. No finding was
rejected. A final full-diff pass found no remaining confirmed P0, P1, P2, or P3
issue.

The independent checkpoint-binding pass found three P3 documentation issues:
the simulation-item ownership count was one line stale, `<any>` rejection was
attributed too broadly to content validation, and the residual-risk wording did
not distinguish in-progress item/protocol extractions from planned
persistence/server extractions. All three were corrected and re-reviewed. The
final binding pass found no remaining issue at any severity.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features` — 346 tests
- `cargo test --workspace --doc --all-features`
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --all-features`
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, content-inventory, JSON, and diff checks
- all nine direct/snapshot/SQLite/replay conformance tests
- pinned C++ pocket/item-group/mapgen oracles — 8/51/17 assertions
- production selected-content test — exactly 524 admitted furniture bashes
- selected-content manifest hash:
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`

## Residual risks

- Constructor-variant and modifier/order source evidence is split across exact
  representative witnesses rather than one combined C++ end-to-end trace.
- Custom invalid color or ASCII-art registry references are not exercised. The
  pinned defaults are valid, and the current client path displays the selected
  authoritative name rather than custom art.
- General containment, recursive wrapper stable IDs, pocket capacity,
  sealing, and overflow remain unavailable and fail closed. They are the next
  `regional-terrain-base` dependency boundary.
- The broader item and protocol ownership extractions remain in progress;
  persistence and server ownership extractions remain planned. Anatomy and
  EOCs stay deferred behind them.
