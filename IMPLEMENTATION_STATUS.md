# Implementation Status

Upstream is fixed at commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05`,
tree `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `db09119e73a68d1f67dd2276c6989b37e1ad7bf5`.
- Reviewed tree: `60b1f097a51abedd88735c27bf7b706a64e8c025`.
- `overmap-cities` is complete. The active dependency boundary and next
  playable unlock is the complete `overmap-roads` family.
- Representation is protocol 96, persistence schema/minimum recoverable schema
  74, CanonicalStateV72, CanonicalEventsV18, replay format 3, worldgen
  algorithm 2, scenario format 8, and observation format 6.
- Hosts remain macOS, Linux, and Windows. Bevy 0.19 is client-only; server and
  simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

## Runnable city-backed world

Selected core content admits the placement-affecting `default` and `no_cities`
city settings. A focused deterministic engine generates the pinned default
9-or-10 city centers, bounded 2..55 sizes, two-tile spacing exclusion,
no-city/megacity branches, dense stable city IDs, and immutable center/size
metadata from the world seed. City-aware starts apply the pinned city-size and
double-subtracted edge-distance intervals under server authority.

Production now uses the real regional field with persistent `road_nesw` city
center identities. Their local 24x24 tiles deliberately use the upstream
`field` fallback predecessor until the connected-road family is implemented;
unsupported road nesting, collisions, and loot are not fabricated. The pinned
acceptance seed reaches 208 durable chunks, 59 top-level ground items, one
nested ground owner, and 45 distinct item definitions. Two independently
Iroh-authenticated clients create/select durable characters, explore, pick up
a generated corpse, remove nested loot, disconnect, restart, recover from
SQLite, and verify an encoded portable replay.

- Final state hash:
  `7ba9468989ab159d59f20f065133c08aa796ce3c55a40d611aee28ad01797778`.
- Event trace hash:
  `f30e045311d976b128bb696dd97151ede4c8cc75f3565da9503c3e0dbf5ff70c`.

The state hash changes because V72 retains cities, start intervals, and the
heterogeneous overmap. The event hash changes without a wire change because
heterogeneous start selection intentionally removes the uniform bootstrap's
origin bias, changing the second survivor's deterministic movement position
and expanding the active bubble from 144 to 208 chunks.

## Runtime progress

Parser coverage remains separate from runtime credit. Core ordinary-gameplay
evidence is now 50 generated definitions and 305 of 263,435 weighted points
(0.1158%); selectable bundled mods remain 0 of 113,373. The city milestone
credits one production city-settings definition at all five evidence levels:
generation, authoritative use, persistence, client accessibility, and
four-mode conformance.

## Cumulative module ownership

Growth remains measured from fixed extraction baseline
`40037fbb1db9eaac8d4889b811d29f8c00380e6b`.

- New placement behavior lives in `sim/cities.rs` (486 lines) and selected
  content loading in `content/city.rs` (297 lines).
- Central `sim/lib.rs` is 29,745 lines (+153 cumulative); central
  `protocol/lib.rs` is 10,399 (+426); server executable is 7,176 (+106).
  City-family growth there is limited to exports, serialized representation,
  validation, startup wiring, and focused tests.
- Persistence is 13,103 lines (+26) for schema/literal/recovery coverage;
  `server/worldgen.rs` is 729 (+218) for the production constructor boundary.
- Items, containment, and item-group behavior remain in their extracted
  ownership modules. Roads should grow focused overmap/worldgen modules rather
  than the central simulation or protocol files.

## Exact verification

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Affected-crate all-target Clippy passed with warnings denied.
- 382 affected library tests passed: content 91, protocol 48, simulation 171,
  persistence 40, conformance 10, and server 22.
- The milestone production gate passed in 689.85 seconds through direct,
  declared snapshot, SQLite recovery, portable replay, and real-Iroh
  two-client disconnect/restart execution.
- `cargo xtask cpp-oracle-check docs/oracles/mapgen-static-semantics-v1.json`
  passed 1,179 C++ assertions plus direct Rust comparison.
- Content validation passed all 7,992 vendored files; inventory remained current
  at 6,571 JSON files, 93,779 objects, and 180 definition types.
- Dependency-boundary, 31-milestone parity-ledger, and runtime-progress gates
  passed. The final fixed-tree review is recorded in
  `docs/reviews/overmap-cities.md` and has no unresolved P0/P1 finding.

Tiered verification did not redundantly run full workspace rustdoc, every
oracle, platform CI, fuzzing, or soak tests; those remain release gates. No
test failure is known.

## Next playable unlock

Implement `overmap-roads` as one generalized production family: pinned road
connections, city-to-city topology, boundary continuity, stable persisted
ownership, local road mapgen admission, and the evolving two-client gameplay
scenario. Rivers, specials, spawning, anatomy, and EOCs remain out of scope
until the road family is green.
