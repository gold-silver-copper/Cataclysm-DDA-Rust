# Protocol 84 item-group event checkpoint review

## Scope

- Reviewed base: `73a64d0d3822cc38623bd30a3585968f4dfe4961`
- Reviewed implementation commit:
  `f4591fb9049228f3677777b8357cfed9ae325ea9`
- Complete staged patch ID: `ce0fa4e15530f756a101fd6816937b21e596c949`
- Files: 18 files across item-group content/protocol/simulation/server/
  conformance, schema and canonical hashing, C++ characterization, milestone
  and runtime-progress gates, and current operational documentation.
- Reviewer: independent fresh-context subagent; no implementation edits.

## Findings and resolutions

- P1: runtime progress called the Protocol 83 parent the verified source for
  Protocol 84/schema 62 worktree data. Resolved with separate green-parent and
  optional verified bindings. Final binding requires an ancestor commit, exact
  runtime versions, an unbound artifact at that commit, and unchanged evidence.
- P2: synthetic LMOE and wall-bash conformance fixtures were counted as exact
  production-definition four-mode evidence. Resolved by assigning zero
  definition-level four-mode credit and reducing the score from 76 to 44.
- P2: the six-step milestone completion gate was only a literal list. Resolved
  by requiring one six-part local evidence record for every milestone marked
  complete, with an enforcement test.
- P2: three mapgen families were marked complete without a normalized
  Rust-to-C++ comparator. Resolved by marking each `oracle_pending` and naming
  the exact missing boundary.
- P1: final runtime binding originally did not compare the metric artifact
  itself. Resolved by normalizing only `verified_commit` and comparing every
  other artifact field to the version at the implementation commit.
- P2: an untracked evidence path could satisfy the worktree existence check.
  Resolved by requiring every evidence path to exist at the implementation
  commit before also requiring byte identity through the binding commit.

Every finding was validated against the current code. No finding was accepted
mechanically. The final review found no remaining P0, P1, P2, or P3 issue.

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

- Mapgen completion is blocked on the explicit normalized Rust/C++ comparator;
  the ledger does not claim otherwise.
- Evidence-path semantic adequacy remains a human review responsibility. The
  gate proves record completeness, provenance, and byte identity, not that an
  arbitrary source file semantically proves a claim.
- Indexed RLE OMT lookup and loader-local JSON byte caps remain nonblocking
  hardening opportunities outside this checkpoint.
