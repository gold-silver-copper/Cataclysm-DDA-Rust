# Overmap-city placement review

## Fixed scope

- Pinned upstream: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`,
  tree `210f31db2e8b2f0caed1809f1a66781859f9d129`.
- Family base: `ec96e78357ece04fa493604305042d2d9a3f5775`.
- Reviewed implementation: `db09119e73a68d1f67dd2276c6989b37e1ad7bf5`,
  tree `60b1f097a51abedd88735c27bf7b706a64e8c025`.
- Review worktree: `/tmp/cdda-overmap-cities-review.YgyBbE`, detached and
  clean at the reviewed commit for the final pass.
- Representation: protocol 96, persistence schema/minimum recoverable schema
  74, CanonicalStateV72, CanonicalEventsV18, replay format 3, worldgen
  algorithm 2, scenario format 8, and observation format 6.

The review covered the complete 20-file family diff: selected city settings,
static global mapgen weights, deterministic city placement and keyed RNG,
stable IDs and center identities, city-constrained starts, canonical protocol
validation, SQLite/replay retention, production admission, real-Iroh gameplay,
the C++ oracle adapter and exact traces, and live milestone evidence.

## Findings and resolutions

1. **P2 — the parity ledger omitted the actual upstream placement kernel.**
   `overmap_city.cpp` now appears in the fixed source inventory alongside the
   city, regional-settings, start-location, and overmap sources.
2. **P3 — the minimum-schema comment described only the Protocol 95 item-group
   boundary.** It now records the Protocol 96 city/start worldgen fields that
   make schema 74 snapshots intentionally incompatible.
3. **P3 — city content documentation overstated the projection.** The focused
   registry correctly consumes only placement-affecting fields, but its comment
   said building bins were retained and its test said the whole settings object
   was admitted. Both now state that houses, shops, parks, and presentation
   data remain in pinned source for the later road/building family.

No confirmed P0 or P1 issue was found. The final pass rechecked checked
coordinate arithmetic, city-count bounds, candidate and spacing behavior,
dense stable IDs, canonical identity sorting/RLE, hostile shape bounds,
city-start eligibility, server-only world creation, persistence versioning,
replay equivalence, and the absence of host-time or unordered iteration in the
new deterministic path.

## Canonical hash audit

The production state hash changed to
`7ba9468989ab159d59f20f065133c08aa796ce3c55a40d611aee28ad01797778`
because CanonicalStateV72 retains city records, city-aware start intervals,
and the heterogeneous field/road-center overmap layout. The event trace changed
to `f30e045311d976b128bb696dd97151ede4c8cc75f3565da9503c3e0dbf5ff70c`
without an event representation change: the existing heterogeneous-layout
rule removes the uniform bootstrap's origin bias, so the second survivor gets
a different deterministic start and movement position. That also expands the
durable active bubble from 144 to 208 chunks. Focused simulation coverage pins
the no-origin-bias behavior.

## Verification

- formatting and diff checks passed;
- affected-crate all-target Clippy passed with warnings denied;
- 382 affected library tests passed across content, protocol, simulation,
  persistence, conformance, and server, including existing real-Iroh tests;
- the production city-backed field gate passed in 689.85 seconds through
  direct, declared snapshot, SQLite recovery, portable replay, and real-Iroh
  two-client disconnect/restart execution;
- the pinned static-mapgen C++ oracle passed 1,179 assertions plus direct Rust
  comparison, including default city settings and exact city-distance traces;
- all 7,992 vendored content files validated and the 6,571-file,
  93,779-object inventory remained current;
- dependency boundaries, parity ledger, and denominator-aware runtime progress
  gates passed. The final focused comment/test-name fix also passed its two city
  tests and strict content-crate Clippy.

Tiered verification deliberately did not repeat full workspace rustdoc, every
oracle, fuzzing, platform CI, or soak tests; those remain consolidated release
gates. No test regression is known.

## Residual boundary

The keyed ChaCha placement stream is a deliberate deterministic multiplayer
adaptation rather than CDDA's process-global RNG stream; pinned density,
size/spacing branches, exact distance traces, and fixed Rust representative
output are covered. City centers carry the upstream `road_nesw` identity but
generate its documented `field` predecessor until the next generalized road
family owns connected road topology and nested local road mapgen. Bundled-mod
predefined city databases, building bins, roads, rivers, specials, and spawning
remain explicit later families and receive no runtime-progress credit here.
