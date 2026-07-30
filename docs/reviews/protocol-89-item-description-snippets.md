# Protocol 89 item-description snippet review

## Frozen implementation

- Baseline parent: `bcd868170d4fdf50b6e1a2aadff7ebc980dac40d`.
- Reviewed implementation: `032883aed6d3677597248c8e0ec8d0dc7de9324e`
  (`Implement recursive item description snippets`).
- Tree: `6da10d6bcb9fef98f7212a2a536ccc620cb3cae1`.
- Stable patch ID: `f8d8d258e9b4c33029765dc473a12616592ba7c3`.
- Scope: 19 files, 1,778 insertions and 295 deletions.
- Representation: protocol 89, schema/minimum recoverable schema 67,
  CanonicalStateV65, worldgen algorithm 2, replay format 3, and
  CanonicalEventsV18.

The reviewed family covers selected English snippet and name loading,
base/variant description normalization, constructor and explicit-variant RNG
phases, bounded protocol validation, canonical item variables, simulation,
client rendering, four-mode recovery, production content admission, and the
pinned C++ item-group characterization/direct comparator.

## First fixed-tree review and resolutions

The first independent review inspected exact commit
`13bae07487889784e869cf4be7697b113d4a2b8f`, tree
`efadb363f22bf9029b94ebb50bb2eff5ce40f321`, from a detached worktree. It found
four confirmed issues and no other P0-P3 findings across the complete 19-file
diff:

1. **P1: missing initial constructor variant expansion.** Pinned
   `select_itype_variant` calls `set_itype_variant`, expanding an expanding
   variant before the constructor later expands the base and the selected
   variant again. Rust had only the later two phases. The constructor now
   performs the first expansion before inline snippet selection, preserves the
   later base/final-variant order, and keeps explicit modifiers as one further
   expansion. A two-choice phase test pins every draw. The shared conformance
   seed consequently changed its later nail range from six to four; that drift
   was accepted only after tracing the two generated splinters through the
   pinned first, final, and explicit variant calls.
2. **P1: missing name-library categories.** Pinned C++ loads
   `data/names/en.json` separately from ordinary snippet JSON. Rust now loads
   that vendored provenance entry first, applies exact usage/gender/unisex
   routing and fallback behavior, and then processes selected snippet files.
   Production tests retain 3,045 family names, 4,275 female given names, 1,219
   male given names, 20,900 world names, and the complete seven-category
   `dog_tag_id` expansion closure.
3. **P2: exponential valid-DAG validation.** The maximum expanded-length
   traversal now memoizes completed `(category, depth)` results while retaining
   the active visiting set for cycle rejection. A depth-32 DAG with two
   identical downstream choices at every level is a fast positive regression
   case. The self-contained choice bound is 32,768 so the pinned 20,900-choice
   world-name category and dog-tag closure fit while remaining explicit.
4. **P2: generated description could exceed variable capacity.** Protocol
   validation now reserves one of the 64 variable slots whenever a base or any
   selectable variant can generate `description`, unless that key already
   exists. Simulation defensively applies the same rule through one checked
   insertion helper. Exact-full and replacement-capacity cases are tested.

The fixes did not alter a wire or stored shape, so protocol 89/schema 67 remain
the correct representation checkpoint. Replay format 3 and CanonicalEventsV18
also remain unchanged.

## Replacement-tree rereview

The independent reviewer then inspected exact replacement commit `032883a`
from a new detached worktree. The rereview confirmed all four runtime/protocol
fixes and found no new P0-P2 issue. It found one P3 documentation issue:
`IMPLEMENTATION_STATUS.md` still reported the pre-fix module sizes, base
deltas, 378-test count, and incomplete constructor phase prose. This review
checkpoint updates those values to 3,828 lines in `sim/items.rs`, 1,624 in
`protocol/item_groups.rs`, net family growth of 259 and 321 lines respectively,
380 workspace tests, and the exact initial/base/final/explicit expansion order.
It also aligns the architecture, porting matrix, README, and runtime-progress
binding with the reviewed implementation. No other P0-P3 finding was
confirmed.

The replacement review passed formatting/diff checks, strict all-target and
all-feature Clippy, the focused content name, protocol DAG/capacity, simulator
phase/RNG, four-mode conformance, client menu, and production server closure
tests, plus the 76-assertion C++ item-group oracle. The production closure test
completed in 137.32 seconds. The reviewer did not redundantly rerun the full
workspace after the primary implementation pass had already completed all 380
tests.

## Characterization and conformance evidence

- The pinned item-group oracle has 76 exact assertions. It retains the direct
  recursive/literal boundary `Foo <lt>lt<gt> <unknown>` to
  `Foo <lt> <unknown>` and production seed 59 for
  `accessory_necklace` to `holy_symbol/saint_necklace`, including the exact St.
  Mary text and downstream draw 1652.
- The direct comparator runs the bounded expansion through Rust. Because the
  multiplayer simulation deliberately uses its canonical ChaCha stream rather
  than C++'s global engine, constructor phase parity is additionally pinned by
  the exact C++ production downstream trace and the Rust multi-choice
  phase-order test rather than equating unrelated numeric seeds.
- Direct, per-tick snapshot, SQLite recovery, and portable replay preserve the
  same generated description. The ordinary Bevy item menu renders the
  authoritative replicated variable without loading content locally.
- The representative item-flow hash remains
  `0878f47b5e8e159fdee5a57a6c7f90bab5e13e6bb944820a10585b835fb857be`.
  Hashing the same Postcard bytes under CanonicalStateV64 reproduces
  `c476a1ccd153ece571ebf4a98be13242ab3a7163124abff4173d9c9050c1f9b7`,
  isolating the intentional V65 domain.

## Verification

The frozen implementation passed:

- `cargo fmt --all -- --check` and `git diff --check`;
- `cargo check --workspace --all-targets --all-features`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-targets --all-features` -- 380 tests;
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`;
- dependency-boundary, 30-milestone parity-ledger, runtime-progress,
  astronomy-table, content-validation, and content-inventory gates;
- selected-content validation -- 7,992 files, manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
- content inventory -- 6,571 JSON files, 93,779 top-level objects, and 180
  definition types;
- pinned C++ pocket, item-group, and mapgen oracles -- 8, 76, and 1,179
  assertions respectively.

## Residual boundary

The initial platform remains pinned English; non-English catalogs and runtime
language switching are out of scope. This family earns no runtime progress
until the real field can be generated, explored, looted, persisted, and used
through the client. The field closure now fails closed at `leg_sheath6`
variable-size `FIT` state, which is the next generalized dependency boundary.
