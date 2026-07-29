# Mapgen direct-comparator completion review

## Scope

- Family base: `68a032ab220ca51c16b0698b966692c8bc56ebba`.
- Initial implementation: `6da2b7a21fa5f595c596fefa7535cf2a1f5a116e`
  (`Complete direct mapgen oracle cycle`).
- Reviewed implementation after fixes:
  `37bb6f473153d3dc320f055c5ae35330b48b38a1` (`Address mapgen
  checkpoint review findings`).
- Final implementation tree:
  `4551187dfc6992ee63ff11d5d6dbde8bef0dc17a`.
- Cumulative stable patch ID:
  `7a7d1fdfac24417e73215cda94f07b245b07e2a8`; review/fix patch ID:
  `406ced6df0b58daf20cd4f7e67f2c1ae369a9625`.
- Cumulative scope: 12 files, 952 insertions and 36 deletions. The fix delta is
  9 files, 253 insertions and 23 deletions.
- First independent worktree: `/tmp/cdda-mapgen-comparator-review.QHd5o8`.
  Final independent worktree: `/tmp/cdda-mapgen-final-review.9F0RwO`.
- Upstream reference: commit
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.

The review covered only the complete family diff and its review/fix delta:
tools dependencies and comparator code, the C++ adapter, strict oracle JSON,
the missing manifested fixture, parity/runtime ledgers, and operational and
architecture documentation. It did not claim a full-tree code review.

## Findings and resolutions

- **P1 — false start-location completion.** The first implementation compared
  OMT match predicates but never loaded or invoked a start-location definition;
  its synthetic Rust oracle catalog explicitly used `start_location: None`.
  The fix loads the production `sloc_lmoe` definition through upstream
  `start_location_id::obj()` and `random_target()` and through Rust's production
  `StartLocationRegistry`. Both sides now compare the target, match type,
  parameter count, city and Z constraints, flags, runtime-admission boundary,
  fixed candidate identities, matching subset, and selected candidate. Strict
  validation and negative tests reject a changed start observation.
- **P2 — inflated runtime evidence.** The first ledger awarded production
  four-mode credit to a hand-built conformance catalog. Its synthetic LMOE used
  `t_lmoe_floor`, whereas admitted production LMOE uses regional groundcover.
  The fix restores every production four-mode count to zero and the score to 44
  of 263,435 core points (0.0167%). The heterogeneous scenario remains valid
  engine/recovery evidence but no longer masquerades as production-definition
  evidence.
- **P2 — pristine-checkout failure.** The manifest included upstream's 16-byte
  `INVALID_RAND.mo`, but a nested upstream `.gitignore` excluded it from this
  repository. The populated implementation worktree hid the defect. The exact
  upstream bytes are now force-tracked: Git blob
  `0e1e1005dd86a3c666fec1f082629076c17aa2b7`, SHA-256
  `6c85a17c8347233d7ca84e34f2b23ba4a22491d667b67a9345f11f61b76499c2`.
  Content validation and the complete direct oracle pass in a new pristine
  detached worktree.

Every finding was validated against the fixed commit before resolution. The
final independent pass found no remaining confirmed P0, P1, P2, or P3 issue.

## Semantic audit

- The C++ side generates a real 24x24 nested terrain/furniture template. The
  observation scans all 576 cells, retains an exact row trace, and rejects any
  unexpected tile. Rust runs the actual `WorldState` mapgen engine and compares
  the normalized trace exactly.
- Production shelter and road identities are loaded on both sides. The direct
  cycle corrected an earlier oracle assumption: `road_ns` concretely rotates by
  0 and `road_ew` by 3; the north/south/east/west request label is not itself
  the mapgen rotation.
- The production `sloc_lmoe` definition has one source target and the fixed
  candidate set has one match, `lmoe_north`. This closes the upstream-equivalent
  target/candidate boundary. Deterministic occupied-tile fallback is an explicit
  multiplayer adaptation and remains covered by Rust direct, per-tick snapshot,
  SQLite, portable-replay, server creation, and client paths.
- No protocol, persistence, replay, canonical-state, server, simulation, or
  client representation changed. Protocol 87, schema 65, replay format 3,
  CanonicalStateV63, and CanonicalEventsV18 therefore remain correct.
- `sim/lib.rs` and `protocol/lib.rs` remain 29,339 and 9,722 lines. New behavior
  is confined to the tools/oracle boundary; no central module-growth exception
  was used.

## Rejected concerns and residual limitations

- The synthetic one-marker template is a characterization case for the generic
  atomic engine, not a claim that it is production LMOE. Production admission,
  conformance, and client evidence remain separate completion links.
- Hard-coded Rust phase labels summarize represented terrain/furniture fields;
  actual output is still generated and compared for every cell. This is a
  limited trace of phase metadata, not silent runtime projection.
- The C++ child process has bounded output, direct arguments, a pinned checkout,
  hashed cache invalidation, an exclusive runtime lock, and rooted temporary
  cleanup, but no internal wall-clock timeout. CI cancellation remains the
  outer operational bound. The reviewer classified this as a residual
  operational limitation, not a release finding.
- Multi-target start definitions, city-constrained selection, remote start
  generation, and rich placement/scoring remain explicitly unavailable. The
  completed milestone is bounded to the admitted ordinary start family.

## Verification

The implementation and review/fix tree passed:

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features` — 373 tests plus doc-tests
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --all-features`
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, and content-inventory gates
- pinned C++ pocket/item-group/mapgen oracles — 8/65/1,179 assertions
- direct mapgen oracle in the pristine final review worktree
- 10 tools, 9 conformance, and 152 simulation tests independently
- selected-content manifest — 7,992 files, hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`

No push or pull request was requested, so remote CI and GitHub review state are
not part of this local checkpoint.
