# Protocol 87 generalized item-containment checkpoint review

## Scope

- Reviewed base: `2a3ab9d42b61e0ed90d167650bfb1ee2e2512277`.
- Reviewed implementation commit:
  `0552ce841fd50dd48789b700170ddce9154284fb` (`Implement generalized item
  containment family`).
- Complete implementation tree:
  `372f320a7123ae7c54b926f4bcbbc95aecb669d1`.
- Complete implementation patch ID:
  `fec84981a56457fd6a4ae0bcc4a4540bb36c592c`.
- Files: 24 tracked files, 7,766 insertions and 495 deletions, across the Bevy
  client item view, item and item-group content, protocol item groups and
  canonical state, simulation items, server normalization and commands,
  persistence, conformance, C++ oracle adapters and evidence, ledgers, and
  operational documentation.
- Reviewer: independent fresh-context subagent in detached worktree
  `/tmp/cdda-item-group-final.R8yg77`; no final-tree implementation edits.
- Upstream reference: clean commit
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Findings and resolutions

- P1: an outer named-group `[0, 0]` charge modifier could leave a liquid or
  count-by-charge child at zero. The modifier application now performs the
  upstream minimum-one clamp, and the exact fixture starts from a normalized
  one-charge prototype before proving the outer zero range clamps back to one.
- P1: `TOOL` and `CAN_HAVE_CHARGES` charge behavior used the wrong precedence.
  Charge classification and tests now follow the generalized content semantics
  rather than a narrow type example.
- P1: a broad review fix rejected all zero-charge containment-CBC snapshots and
  therefore broke legitimate loose batteries with fractional residual energy.
  Protocol, simulation, and prototype validation now admit exactly a battery
  ammunition item with zero charges and positive bounded residual energy;
  ordinary CBC items, zero residual, negative charges, non-battery ammunition,
  and oversized residuals remain rejected. A real command test proves
  fractional drain, unload, snapshot recovery, and reload.
- P1: recipe component `count_by_charges` and containment metadata could
  disagree, and craft prototypes did not enforce the snapshot rule. Protocol
  and simulation now require equality, prototype validation mirrors the rule,
  and reconstruction plus reversible nested-provenance tests cover the exact
  boundary.
- P2: initial capacity and containment treatment did not fully account for
  recursive `NO_DROP`/`REDUCED_WEIGHT`, E-file exclusion, maximum length,
  explicit softness, reserved physical variables, and modifier-owned container
  capacity. The generalized engine now handles each represented semantic and
  fails closed on unsupported pocket or material-derived behavior, with direct
  boundary tests.
- P2: the first runtime denominator included obsolete or unselectable mod
  definitions. The ledger now separates 13,865 core DDA target definitions
  (263,435 possible points) from 5,967 definitions in selectable nonobsolete
  bundled mods (113,373 possible points), while ordinary playable loops remain
  separate from parser inventory.
- P3: local `Node` modifiers could pass one validator but fail later in the
  planner. Protocol and simulation validation now reject that unsupported
  shape consistently.
- P3: canonical E-file pocket validation accepted a non-rigid pocket even
  though the runtime representation requires rigidity. Protocol and simulation
  now reject it symmetrically, including catalog and prototype regressions.
- P3: item-group oracle and ownership-size documentation became stale during
  the batch. It now records 65 exact assertions and the independently reproduced
  module counts and net growth.

Every finding was checked against the implementation before it was changed.
The exact final-tree review found no remaining confirmed P0, P1, P2, or P3
issue.

## Rejected or withdrawn concerns

- A concern that insertion could select the wrong same-kind pocket was rejected:
  current normalization fail-closes ambiguous layouts with more than one
  physical or more than one E-file spawn pocket, so the runtime lookup is
  unique.
- A concern that arbitrary E-file fields were silently projected away was
  withdrawn after tracing normalization: represented E-file fields are retained
  and unsupported shapes fail closed.
- No additional protocol or persistence bump was made for the final review
  repairs because they changed validation only. The containment family remains
  one serialized checkpoint: protocol 87, schema/minimum recoverable schema 65,
  CanonicalStateV63, replay format 3, and CanonicalEventsV18.

## Canonical-hash audit

Exactly one checked canonical fixture hash changed, from
`8f8710e06937a50c14bcad35a17dbc41a059128061f4be9316c4c6449358dc66` to
`80e072e755e68be0aad782132f7118f4269b5f664ead99bc50a1b1cd8b27d335`.
The change is the explicit CanonicalStateV62-to-V63 domain plus serialized
containment defaults. Tick, actors, inventory, ground items, commands, events,
and the CanonicalEventsV18 trace/hash are unchanged. Direct, per-tick snapshot,
SQLite, and portable-replay conformance all reproduce the new state.

## Verification

The implementation gate passed on the final source:

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features` — 373 tests plus doc-tests
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`
- dependency-boundary, parity-ledger, runtime-progress, astronomy,
  selected-content, and content-inventory gates
- all nine direct/snapshot/SQLite/replay conformance tests
- pinned C++ pocket/item-group/mapgen oracles — 8/65/17 assertions
- production selected-content test — exactly 524 admitted furniture bashes
- selected-content manifest — 7,992 files, hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`

The independent exact-commit pass reproduced the commit, tree, patch ID, and
24-file scope, inspected the entire diff, and reran formatting/diff checks,
strict targeted Clippy, 35 protocol, 152 simulation, nine conformance, 40
persistence, 76 content, 22 server-library, and 15 server-binary tests. It also
reran every ledger/inventory gate and the production-content normalization
test. Its first fresh-worktree content run lacked the repository's ignored
16-byte `INVALID_RAND.mo` test fixture; copying the identical fixture made the
suite pass, and the fixture was removed afterward.

## Residual risks and next boundary

- Flexible physical pockets, material-derived softness, unprojected constructor
  and pocket semantics, and arbitrary player-driven general containment remain
  unavailable and fail closed.
- The complete `civilian_phones_case` closure is admitted, but the real `field`
  base stops at `accesories_personal_unisex_child` because `wearable_light`
  needs an unrepresented tool ammunition-loading path. Field production and the
  normal client exploration/loot proof remain pending.
- `atomic-static-mapgen`, `omt-identities-routing`, and
  `start-location-selection` remain `oracle_pending`. The next implementation
  boundary is one reusable direct Rust-to-C++ comparator that closes all three
  before ledger expansion or another broad infrastructure initiative.
- Central item/protocol ownership is still oversized. New containment behavior
  has a zero-growth budget in central `lib.rs` files absent a documented
  schema-only integration; the listed mechanical ownership extractions remain
  before anatomy or EOC expansion.
