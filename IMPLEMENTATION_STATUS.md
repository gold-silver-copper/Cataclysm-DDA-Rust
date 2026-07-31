# Implementation Status

Upstream is fixed at commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05`,
tree `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green parent: `40037fbb1db9eaac8d4889b811d29f8c00380e6b`.
- Candidate base: `0d883b58d60c12540e3bcf05315606a2e4abe1a3`.
- Candidate state: the complete regional-field milestone is green in the
  working tree; its fixed committed-tree review and audited checkpoint are the
  only remaining gates.
- Representation remains frozen at protocol 95, persistence schema/minimum
  recoverable schema 73, CanonicalStateV71, CanonicalEventsV18, replay format
  3, worldgen algorithm 2, and observation format 6. Scenario format 8 is a
  test-harness-only change for generated-item selectors and batched advances.
- Active next milestone: `overmap-cities`. No city implementation is included
  in this checkpoint.
- Hosts remain macOS, Linux, and Windows. Bevy 0.19 is client-only; server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

## Completed regional-field unlock

The server now replaces the repeated LMOE surface with the production `field`
OMT and `sloc_field` start. Recursive regional terrain and furniture tables,
multi-pocket selection, wrapper ownership, contents groups, sealing, overflow,
snippets, variables, perishable food, static corpses, and insulated containers
are generalized and strict. Unsupported semantics remain retained and fail
closed.

The pinned production seed generates 144 chunks, 59 top-level ground items,
one nested ground owner, and 45 distinct item definitions. Two consecutive
stable actors explore the field; one picks up a generated corpse and removes
nested loot while the other moves; both disconnect and the world advances.
Direct, per-tick snapshot, SQLite recovery, and portable replay observations
are identical.

- Final state hash:
  `1ac803cf46569081817639f939acaa180e6628487f00e892a2183439aba21e97`.
- Event trace hash:
  `40b05c278a6a6af9055e6dd9a3bc0acf4e920c9f690f7e0b1eea9a678726eda7`.

The state hash changed from the prior candidate only because `thermos` moved
from a fail-closed projection to an exact physical-pocket projection carrying
its upstream 10.0 `float` insulation in the existing typed-variable envelope.
Temporarily restoring the old missing pocket, insertion capability, and
variable reproduced the old hash exactly; the event hash did not change.

Conformance, the pinned C++ oracle, general containment, item groups, and the
regional terrain base are complete at this demonstrated baseline. Future
semantic additions extend those foundations rather than reopening them.

## Runtime progress

Parser coverage remains separate from runtime credit. The production field
raises core ordinary-gameplay evidence to 49 generated definitions and 286 of
263,435 weighted points (0.1086%). Selectable bundled mods remain 0 of 113,373.
The new field category credits 45 generated and persisted item definitions but
only the two definitions exercised through the authoritative nested-loot path
receive interaction, client-accessibility, and four-mode points.

## Cumulative module ownership

Growth is measured from fixed extraction baseline
`40037fbb1db9eaac8d4889b811d29f8c00380e6b`, not reset per commit.

- `sim/items.rs`: 6,657 lines, +1,610 net; item construction, containment,
  temperature, rot, and item-group behavior live here.
- `protocol/item_groups.rs`: 2,705 lines, +604 net; item-group representation,
  validation, graph metrics, rot, and pocket-insulation metadata live here.
- `server/item_groups.rs`: 2,868 lines, +1,341 net; production content
  normalization and strict closure admission live here.
- `server/regional_field_acceptance.rs`: 290 new lines; the broad gameplay
  acceptance no longer expands the server executable.
- Central `sim/lib.rs`: 29,668 lines, +76 net; central `protocol/lib.rs`: 10,239
  lines, +266 net; server executable: 7,151 lines, +81 net. Their growth is
  wiring, canonical validation/representation, or production constructor
  boundaries rather than new item engines.
- Persistence remains 13,077 lines with zero net growth; the server library
  remains 8,807 lines with zero growth.

Actors, combat, activities, monsters, remaining protocol domains, persistence
responsibilities, and sessions/replication still require focused mechanical
extraction before anatomy or EOC expansion.

## Exact verification

Passing on the complete candidate:

- `cargo fmt --all -- --check` and `git diff --check`;
- `cargo check --workspace --all-targets --all-features`;
- strict workspace Clippy with all targets/features and warnings denied;
- 421 workspace target/feature tests, including the 324.14-second production
  content/field test;
- workspace doc tests and rustdoc build;
- dependency-boundary, parity-ledger, runtime-progress, astronomy-table,
  content-validation, and content-inventory gates;
- live pinned C++ pocket oracle: 8 assertions;
- live pinned C++ item-group oracle: 272 assertions plus direct Rust comparison;
- live pinned C++ static-mapgen oracle: 1,179 assertions plus direct Rust
  comparison.

No test failure is known. The next and final operation for this milestone is a
fixed-tree review of the committed candidate, followed by one consolidated
review record and audited checkpoint. The next playable unlock after that is
server-owned city placement feeding ordinary urban mapgen and later road
topology; roads, rivers, specials, anatomy, and EOCs remain out of this
checkpoint.
