# Protocol 88 detachable tool-charge checkpoint review

## Scope

- Reviewed base and merge base:
  `57d542235f7d1fa0049591d234f3ae25907072cb`.
- Reviewed implementation commit:
  `bdeb8570615fae97288b0d5d5d8b1e18e407e04c` (`Implement detachable
  item-group tool charging`).
- Implementation tree:
  `36edcec2a01c5a747e943867324b1479a84843ce`.
- Stable patch ID: `5ca50adee4f93d81d4cfdd46a4a8d7d3e7330ea1`.
- Scope: 20 files, 1,657 insertions and 326 deletions. The reviewed family
  covers canonical magazine-well rigidity, content normalization, generalized
  integral/detachable tool-charge planning, recursive stable IDs, server
  admission, direct/snapshot/SQLite/replay conformance, client visibility,
  C++ characterization, ledgers, and operational documentation.
- Final reviewer: independent fresh-context subagent in clean detached
  worktree `/tmp/cdda-bdeb-review.J4hsfF`; no implementation edits or commits.
- Upstream reference: clean commit
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.

The review was fixed to the exact committed family diff. It did not claim a
review of unrelated repository history or a moving working tree.

## Finding and resolution

- P1: the pre-freeze review found that applying a second named-group charge
  modifier reconstructed an already installed detachable magazine. That
  consumed the magazine constructor's two RNG draws again and diverged from
  C++, which retains the magazine and replaces only its ammunition. The fixed
  planner removes and reuses the matching planned magazine, clears its existing
  ammunition, constructs only replacement ammunition, and reinserts the same
  magazine. A nested Rust regression pins the final three-object plan and exact
  17-draw boundary.

The final exact-commit reviewer validated the repair against the full diff and
found no remaining confirmed P0, P1, P2, or P3 issue.

## Exact oracle and representation audit

- The production C++ witness uses
  `accesories_personal_unisex_child`, seed 235, a leaf charge range of 0 through
  100, and an outer replacement of one. It retains `wearable_light` ->
  `medium_battery_cell` -> `battery` with one charge and pins downstream draw
  2632. The oracle passes 70 assertions before the runner directly compares the
  resulting Rust projection.
- Direct requests 0, 1, 56, and 100 retain their exact traces. Zero installs the
  default detachable magazine without ammunition; positive charges create
  ammunition and clamp it to magazine capacity.
- Canonical magazine-well rigidity and the generalized tool-charge storage plan
  change serialized representation. Protocol 88, schema/minimum recoverable
  schema 66, and CanonicalStateV64 are therefore required. Replay format 3 and
  CanonicalEventsV18 remain unchanged. The later RNG repair changes behavior,
  not representation, so it does not receive another version bump.
- Exactly one checked canonical fixture hash changes, from
  `80e072e755e68be0aad782132f7118f4269b5f664ead99bc50a1b1cd8b27d335` to
  `c476a1ccd153ece571ebf4a98be13242ab3a7163124abff4173d9c9050c1f9b7`.
  The audit hashes the same Postcard bytes under CanonicalStateV63 and
  reproduces the old root, isolating the change to the intentional V64 domain.
  CanonicalEventsV18 and the represented tick, actors, inventory, ground items,
  and commands remain unchanged.
- Direct, per-tick snapshot, SQLite, and portable-replay conformance preserve
  the charged headlamp, its installed magazine and ammunition, recursive stable
  IDs, and sealed wrapper ownership. The ordinary client item view exposes the
  nested state through the server-authoritative path.

## Module-growth audit

Against the preceding bound documentation tree, the increment adds 304 lines to
`sim/items.rs`, 111 to `protocol/item_groups.rs`, and 87 to
`server/item_groups.rs`, while removing 39 lines from `sim/lib.rs`. The 31-line
`protocol/lib.rs` increase is limited to the canonical magazine-well field,
recursive-volume behavior, and focused tests; the server library adds two
fixture lines. No item behavior was added to a central `lib.rs`.

The mechanical move of item-spawn construction into `sim/items.rs` makes that
module the future ownership boundary. Further item-group behavior has a
zero-growth budget in central libraries absent a recorded schema-only
integration justification.

## Rejected concerns

- Structural validation requires only a nonzero production seed, but strict
  fixture equality separately pins seed 235 and downstream draw 2632; a changed
  production trace cannot pass by satisfying the loose guard alone.
- The direct Rust projection intentionally excludes C++ RNG numbers because the
  engines use different generators. C++ output is pinned numerically, while the
  separate Rust test pins its exact 17-draw phase boundary.
- Planner output metrics conservatively overestimate repeated replacements,
  but authoritative planned-object counting, capacity checks, recursive ID
  assignment, and persistence operate on the actual final tree. The reviewer
  found no correctness or stable-ID defect.

## Verification

The implementation and independent exact-commit review passed:

- `cargo fmt --all -- --check` and `git diff --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-targets --all-features` — 374 tests plus
  workspace doc-tests
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`
- dependency-boundary, 30-milestone parity-ledger, runtime-progress,
  astronomy-table, content-validation, and content-inventory gates
- pinned C++ item-group oracle — 70 assertions and direct Rust comparison
- production-content server admission test — exactly 524 admitted furniture
  bashes
- selected-content manifest — 7,992 files, hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`
- content inventory — 6,571 JSON files, 93,779 objects, and 180 types

The primary implementation gate also reran the unchanged pocket and mapgen C++
oracles at 8 and 1,179 assertions. The final detached reviewer did not rerun
those two unrelated oracle executables.

## Residual risks and next boundary

- There is no separate four-mode scenario specifically for a repeated
  replacement to zero. Zero-charge magazine construction, positive repeated
  replacement, and the resulting nested persistence/stable-ID shape are covered
  independently; this is a focused residual test gap, not a production failure.
- Flexible physical pockets, material-derived softness, arbitrary
  player-driven general containment, and unprojected constructor/pocket
  semantics remain explicit and fail closed.
- The real field base still stops at `saint_necklace` variant-description
  snippet expansion. Field admission requires the complete closure plus normal
  server/client exploration and loot proof before cities, roads, rivers,
  specials, spawning, anatomy, or EOCs expand.
