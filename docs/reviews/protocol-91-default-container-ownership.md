# Protocol 91 default-container ownership review

## Frozen implementation

- Parent and merge base:
  `614f725db686b78ea92c8fe6a60a96c9b7250b19`.
- Reviewed implementation:
  `a80f6c2a8c23c29146f67a843f8cbc34d0cbb6ec`, tree
  `22de91ca31f00f87ac269459b8317bcf362655b6`, stable patch ID
  `19d245d3b42bfd55732a596de46061a4029932a1`.
- Scope: the complete 21-file diff, 1,767 insertions and 164 deletions,
  covering content inheritance, protocol representation and bounds, server
  normalization, simulation construction, stable ownership, persistence,
  four-mode conformance, the ordinary client item menu, pinned C++
  characterization and direct Rust comparison, ledgers, and current
  architecture documentation.
- Representation: protocol 91, persistence schema/minimum recoverable schema
  69, CanonicalStateV67, worldgen algorithm 2, replay format 3, and
  CanonicalEventsV18.

## Independent fixed-tree review

An independent reviewer inspected exact commit `a80f6c2` from clean detached
worktree `/tmp/cdda-a80f6c2-review.3DBeJM`. The implementation tree remained
fixed and clean throughout the review, and the reviewer made no edits. The
complete diff produced no confirmed P0, P1, P2, or P3 finding.

The review covered finalized content inheritance; recursive representation;
protocol shape, validation, metrics, recursion, containment, and output bounds;
server raw-wrapper and creator normalization; simulation capacity, sealing,
ordering, and preorder stable IDs; persistence and replay; CanonicalStateV67
evidence; client access; all four conformance modes; the C++ oracle adapter and
exact corpus; the production field boundary; documentation; live status; and
central-module growth.

The reviewer validated and rejected these candidate concerns:

- Explicit `container-item: "null"` is retained semantically after its creator
  reference normalizes to no container. Synthesized fixed-zero damage records
  modifier presence and therefore suppresses item-type fallback.
- Explicit creator bounds count the target plus the creator's complete
  effective subtree. Their containment depth also matches the effective outer
  wrapper.
- Raw wrappers retain nested default descriptors without invoking them. Pinned
  C++ raw `item(...)` construction likewise skips the nested creator phase.
- Front insertion produces the exact explicit `[ibuprofen, aspirin]` order.
- Sealing occurs only for a full effective container and follows pinned pocket
  fullness behavior.
- Dynamic named-group fallback is rejected only when its generated top-level
  closure may require a default container.
- The module-growth comparison deliberately uses the prior reviewed
  implementation `d957903`; the immediate parent `614f725` is that
  implementation's documentation-binding checkpoint.

## Characterization and representation audit

- The item-group oracle has 104 exact assertions. Seven default-container
  traces retain direct water and aspirin, modifier fallback, explicit-null
  suppression, the ordered explicit creator, and production one/twenty-item
  boundaries. Each records its seed, exact ownership/order, charge state,
  sealing state, and downstream C++ RNG draw.
- The reusable Rust comparator executes the production constructor, modifier,
  wrapper, capacity, sealing, and ordering paths. It compares semantic
  ownership and order rather than downstream numeric draws because Rust and
  C++ use different RNG engines; the exact C++ corpus separately pins the
  upstream RNG schedule.
- Direct, per-tick snapshot, SQLite, and portable replay conformance preserve a
  pill bottle owning aspirin with preorder stable IDs. The normal Bevy item
  menu displays the contained item and removes it through authoritative owner,
  pocket, and child identifiers.
- The representative empty-catalog Postcard bytes are unchanged. Under the old
  CanonicalStateV66 domain they reproduce
  `7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`;
  under CanonicalStateV67 they produce
  `b5c12b763060907d68bfbd96b4aea6372c17cb02676b5e499b0bc79f5679899e`.
  Catalogs containing item prototypes have a changed serialized shape, which
  requires Protocol 91/schema 69. The event shape and CanonicalEventsV18 are
  unchanged.

## Module-growth audit

The implementation owns 4,545 lines in `sim/items.rs`, 1,916 in
`protocol/item_groups.rs`, and 1,406 in `server/item_groups.rs`. Relative to
the prior reviewed implementation, those extracted item modules grow by 362,
273, and 91 lines. Central simulation grows by 14 lines solely for exports, the
canonical hash domain, and mechanical fixture fields; central protocol grows
by four version/fixture lines. Persistence and the server library do not grow.
The server executable adds 33 production-normalization/test lines. This is the
complete central-growth justification; future item behavior remains assigned
to extracted item modules.

## Verification

The implementation and review cycle passed:

- `git diff --check` and `cargo fmt --all -- --check`;
- `cargo check --workspace --all-targets --all-features`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-features` -- 388 tests plus doc-tests;
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`;
- dependency-boundary, 30-milestone parity-ledger, runtime-progress,
  astronomy-table, content-validation, content-inventory, JSON, and diff
  gates;
- selected content -- 7,992 files, manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
- content inventory -- 6,571 JSON files, 93,779 top-level objects, and 180
  definition types;
- pinned C++ pocket, item-group, and mapgen oracles -- 8, 104, and 1,179 exact
  assertions, with reusable direct Rust comparisons for item groups and
  mapgen;
- fixed-tree `cargo test -p cdda-content -p cdda-protocol -p cdda-sim
  default_container` -- three passed;
- fixed-tree `cargo test -p cdda-protocol item_groups::tests` -- seven passed;
- fixed-tree `cargo test -p cdda-sim items::tests` -- 27 passed;
- fixed-tree conformance tests for named item-group bash and the ordinary item
  flow -- one passed in each command across direct, per-tick snapshot, SQLite,
  and portable replay execution.

## Residual boundary

Dynamic named-group default fallback, flexible or multiple physical default
pockets, and other unprojected containment remain deliberately fail closed.
This family earns no runtime-progress credit until its admitted definitions are
generated and used in the ordinary field loop. The deterministic complete
field scan now stops first at generalized comestible temperature state for
`chaw`. That state, real-field admission, and ordinary client exploration and
loot remain the next dependency boundary; cities, roads, rivers, specials,
anatomy, and EOCs do not precede it.
