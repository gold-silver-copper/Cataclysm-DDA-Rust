# Protocol item-group domain extraction review

## Scope

- Reviewed base: `e58f03793bda51db257a0374b8905fe694767c01`.
- Reviewed implementation commit:
  `de09724a27064d293041ce1cf4df5e458ac403a7` (`Extract protocol item group
  domain`).
- Complete implementation patch ID:
  `432d57df2464eaefaa91738367ddc13092a332e8`.
- Files: exactly `IMPLEMENTATION_STATUS.md`,
  `crates/protocol/src/item_groups.rs`, `crates/protocol/src/lib.rs`,
  `docs/parity-ledger.json`, and `docs/runtime-progress.json`; 592 insertions
  and 552 deletions.
- Subsystem: mechanical ownership extraction for protocol item-group wire DTOs,
  bounds, graph evaluation, validation, and exact named-closure checking.
- Reviewer: independent fresh-context subagent; no implementation edits.
- Upstream reference: clean commit
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Findings and resolutions

No P0, P1, P2, or P3 implementation finding remained. The reviewer proved the
five constants and 94-line serialized DTO block byte-identical to the parent,
and the 424-line evaluator/validator block byte-identical after normalizing only
the closure helper's required `fn` to `pub(super) fn` visibility change. The
private module re-exports all 19 formerly crate-root-public types, constants,
and functions, so supported import paths and external visibility remain intact.

The review handoff initially named patch ID `5203b31b...`, captured immediately
before two status-only phrases changed from “uncommitted candidate” to
commit-stable “candidate” wording. Normal and binary patch-ID calculation both
confirmed the final frozen ID `432d57df...`; the reviewer restarted provenance
checks against that exact patch. No code or JSON semantics changed in that
correction.

The diagnostic-only defining path observable through `std::any::type_name` was
considered and rejected as a compatibility issue: Rust does not guarantee that
string, this repository does not consume it, and every supported public
crate-root path is unchanged. Parent-private validator imports through `super`
were also checked; Rust item ordering is irrelevant and the extracted bodies
resolve to the same functions. No version bump is warranted because field,
variant, derive, encoding, validation, and runtime behavior are unchanged.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test -p cdda-protocol --all-features` — 30 tests
- focused item-group and worldgen-catalog tests — 3 tests each
- `cargo test --workspace --all-targets --all-features` — 338 tests
- `cargo test --workspace --doc --all-features`
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --all-features`
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, content-inventory, JSON, and diff checks
- pinned C++ pocket/item-group/mapgen oracles — 8/42/17 assertions
- exact normalized source comparisons for constants, DTOs, and evaluator
- selected-content manifest hash:
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`

## Residual risks

- Focused item-group fixtures remain in the protocol root test module; moving
  test ownership is optional mechanical follow-up, not a dependency for the
  next coherent item-state representation increment.
- The broader `protocol-domain-modules` milestone remains in progress: command,
  canonical state, event, and administration domains are still parent-owned.
- Nonzero raw damage, stored variants, general wrapper contents, nested stable
  identities, and overflow remain unavailable and fail closed. The production
  regional field layer therefore remains blocked at
  `civilian_phones_case.contents-group`.
