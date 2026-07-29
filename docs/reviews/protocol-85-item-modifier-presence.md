# Protocol 85 item-modifier presence checkpoint review

## Scope

- Reviewed base: `40f24489282c1f9c3aeeaf66b71142eab78be98e`
- Reviewed implementation commit:
  `761962583d359c0992af3d6a4d1ec10c41a30905`
- Complete implementation patch ID:
  `802d60c042b801b200febd4f168a8b2bb995863e`
- Files: 20 tracked files across item-group content loading, protocol,
  simulation, server normalization/admission, persistence, conformance, the
  pinned C++ oracle, generated evidence, and operational documentation:
  `ARCHITECTURE_DECISIONS.md`, `IMPLEMENTATION_STATUS.md`, `PORTING_MATRIX.md`,
  `README.md`, `crates/conformance/src/lib.rs`,
  `crates/content/src/item_group.rs`, `crates/content/src/lib.rs`,
  `crates/persistence/src/lib.rs`, `crates/protocol/src/lib.rs`,
  `crates/server/src/item_groups.rs`, `crates/server/src/main.rs`,
  `crates/sim/src/items.rs`, `crates/sim/src/lib.rs`,
  `crates/tools/src/cpp_oracle.rs`, `docs/content-schema-inventory.json`,
  `docs/oracles/item-group-generation-v1.json`, `docs/parity-ledger.json`,
  `docs/runtime-progress.json`, `tools/cpp-oracle/README.md`, and
  `tools/cpp-oracle/item_group_oracle_test.cpp`.
- Reviewer: independent fresh-context subagent; no implementation edits.
- Upstream reference: clean commit
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Findings and resolutions

- P1: initial modifier admission did not require the fixed-zero marker for all
  nondefault count and charge shapes. The protocol now requires the marker and
  tests cover missing count/charge markers.
- P1: nested and named targets could accept a modifier marker even though the
  current representation cannot prove or store recursive modifier side
  effects. Content, server, protocol, and simulation now reject those paths.
- P1: direct leaf construction omitted upstream RNG phases. The planner now
  consumes presentation-seed, empty-variant, unconditional fit, and modifier
  damage phases in order before charge dressing. The C++ oracle pins four
  semantic operations and exact downstream value 6558.
- P1: item construction could project away hidden modifier or constructor
  behavior. Admission now rejects degrading vehicle parts, fouling guns,
  default containers, corpses, preloaded magazines, temperature-bearing
  comestibles, constructor state/RNG fields and flags, and VARSIZE state unless
  the represented behavior is exact.
- P2: wrapper overlays initially lost inherited sealing, overflow, or variant
  policy, and output bounds undercounted wrapper/container objects. Loader
  tests now pin self-copy and string/object overlay behavior, and maximum output
  includes each materialized wrapper/container object.
- P2: signed charge sentinels and the `"null"` leaf/container distinction were
  not preserved exactly. Raw charge endpoints now remain signed and independent;
  one-sided sentinels and materialized null leaves fail closed.
- P2: the initial aggregate-only modifier/container observations could be
  matched by an incorrect implementation. The checked corpus retains exact
  representative container orders, spill results, corpse traces, boundary
  traces, and the fixed modifier downstream trace.
- P2: strict furniture-bash admission was initially carried forward as 539
  without accounting for newly explicit constructor semantics. The complete
  selected-content test now pins 521 and names every one of the 18 newly
  rejected paths: six degrading vehicle-part paths, one default-container path,
  three constructor-RNG paths, three constructor-state paths, and five
  temperature-state paths.
- P2: the C++ documentation retained the previous 41-assertion count after the
  exact modifier trace was added. It now records the verified 42 assertions.
- P2: status initially described the item-flow hash-domain update as the only
  fixture change, omitting the structural-bash nail expectation changing from
  6 to 4. The final status distinguishes the sole canonical hash change from
  that intentional four-mode output change.

A concern that integral-magazine dressing might diverge was rejected after
checking the actual admission path: projected integral magazines originate
from the strict `MAGAZINE` subtype path, so the two retained dressing draws are
not spuriously applied to unrelated items. Every finding and rejection was
validated against the current code rather than accepted mechanically. The
final review found no remaining P0, P1, P2, or P3 issue.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features` — 338 tests
- `cargo test --workspace --doc --all-features`
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --all-features`
- focused content, protocol, simulation, server, persistence, and all-nine
  conformance tests, including the post-fix named four-mode bash scenario
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, content-inventory, JSON, and diff checks
- pinned C++ pocket/item-group/mapgen oracles — 8/42/17 assertions
- selected-content manifest hash:
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`

## Residual risks

- Nonzero raw damage, stored item variants, wrappers/contents, overflow,
  presentation-seed state, and general modifier state remain unavailable and
  fail closed.
- The default regional field closure remains blocked exactly at
  `civilian_phones_case.contents-group`; production therefore retains the LMOE
  bootstrap surface.
- The fixed-zero C++ trace records the source-level semantic operation count and
  exact downstream state. A fixed `rng(0, 0)` need not advance every standard
  library engine internally, so the Rust stream deliberately represents the
  pinned semantic phase rather than relying on an implementation accident.
- The next representation change must follow the prerequisite protocol-domain
  extraction and must not expand anatomy, EOCs, or another subsystem while
  these documented boundaries remain.
