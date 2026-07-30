# Implementation Status

Upstream is fixed at commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05`,
tree `210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Verified green commit: `a80f6c2a8c23c29146f67a843f8cbc34d0cbb6ec`.
  Protocol-92 work above documentation parent
  `e4f74aff4a84019c35818aa0a6746ce33bf309e8` is intentionally unbound until
  the full gates and fixed committed-tree review complete.
- Current worktree representation: protocol 92, persistence schema/minimum
  recoverable schema 70, CanonicalStateV68, CanonicalEventsV18, replay format
  3, worldgen algorithm 2, scenario format 7, observation format 6.
- Active milestone: `regional-terrain-base`.
- Hosts remain macOS, Linux, and Windows. Bevy 0.19 is client-only; the server
  and simulation are plain Rust. Iroh 1.0.3 owns networking and authentication.

Mapgen/overmap milestone states:

- `atomic-static-mapgen`: `complete`.
- `omt-identities-routing`: `complete`.
- `start-location-selection`: `complete`.
- `regional-terrain-base`: `in_progress`.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

## Current family: materialless item temperature

- Exact pinned C++ traces characterize `chaw`, `water_clean`, `caffeine`, and
  `rock`: temperature ownership, active state, 600/10,000-turn processing
  cadence, 0 K, -10 J/g sentinel, birth-tick serialization, phase, and flags.
  All 119 item-group assertions and the reusable direct Rust comparison pass.
- Protocol state is integer-only and recursively owned by items, provenance
  components, integral ammunition, installed magazines, and physical contents.
  The first ten-minute check initializes the admitted class to 293.150 K;
  absent energy represents the pinned materialless indeterminate result without
  a platform-dependent NaN. Recovery rejects future last-check timestamps.
- The complete selected-content class is 36 definitions. Material
  thermodynamics, rot, custom freezing, gas phases, and weather-driven ambient
  changes remain fail closed.
- Direct, per-tick snapshot, SQLite, and portable replay modes retain exact
  nested constructor state. The normal Bevy item menu renders pending and
  initialized temperature and keeps different temperature states stack-distinct.
- Shared finalized-content classification keeps client and server disassembly
  eligibility identical. Closing the old unsound boundary removes exactly 420
  runtime craft recipes (208 material, 182 rot, 28 custom-freezing results,
  one rot byproduct, one freezing byproduct) and 66 disassembly recipes whose
  targets or recovered components need those engines.

## Runnable behavior and next blocker

The persistent authoritative server authenticates through iroh, creates durable
characters, advances with zero players, keeps disconnected characters present
and vulnerable, traverses generated terrain, fights admitted creatures,
manipulates nested items, and runs implemented crafting, reading, disassembly,
construction, recovery, and replay paths.

The fixed full `field` scan now passes `chaw` temperature construction and
stops exactly at `chaw_wrapper_1_20`: item-group wrappers currently require one
rigid physical container pocket. The next playable unlock is a generalized
flexible wrapper/containment engine, then admission of the real field base and
an ordinary exploration/pickup/loot demonstration through the client. Do not
start cities, roads, rivers, specials, anatomy, or EOCs first.

## Measured runtime progress

No points are awarded for parser admission or synthetic characterization.
Temperature earns no production credit until real field definitions are
generated, interacted with, persisted, client-accessible, and four-mode proven.

- Core-DDA ordinary-gameplay target: 13,865 definitions, 263,435 possible
  weighted points; 44 earned (0.0167%).
- Selectable bundled-mod target: 5,967 definitions, 113,373 possible weighted
  points; zero earned.
- Parser inventory remains separate: 7,621 item groups, 9,520 mapgen objects,
  2,712 OMTs, and 150 starts.

## Module-growth budget

Current sizes are 29,335 lines in `sim/lib.rs`, 4,790 in `sim/items.rs`, 9,847
in `protocol/lib.rs`, 1,999 in `protocol/item_groups.rs`, 13,071 in persistence,
8,804 in the server library, 1,471 in `server/item_groups.rs`, and 6,904 in the
server executable. Relative to verified `a80f6c2`, primary item behavior grows
the extracted simulation/protocol/server item modules by 245/83/65 lines.
Central simulation growth is 86 lines for birth-tick call sites, canonical-owner
visitation, recovery validation calls, stack equality, and mechanical fixtures;
temperature arithmetic and recursive ownership remain in `sim/items.rs`.
Central protocol growth is 46 lines for the serialized fields, validation, and
mechanical fixtures; persistence/server-library growth of 6/7 lines is version
and fixture plumbing. New behavior should continue in the extracted modules.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain
mechanical extraction milestones before anatomy or EOC expansion.

## Verification state

Latest focused results in the unbound worktree:

- `cargo check --workspace --all-targets --all-features`: pass.
- protocol, simulation, client, content, persistence, and conformance targeted
  suites: pass; conformance is 9/9 across all four execution modes.
- deterministic selected-content field/catalog test: pass in 97.50 seconds.
- item-group C++ oracle/direct comparator: pass, 119 assertions.
- Protocol-92 hash audit: new bytes under old V67 domain are
  `d0b9e7a84fbdb6ef8a751d3536bfb57a8cd092f17d379ea7a960c14ede43f187`;
  current V68 is
  `ecf2ff2770054b46562dd7cad15c3aa9326586594374b2710af84754beef6a6a`.
  CanonicalEventsV18 is unchanged.

The broad formatting, Clippy, workspace-test, documentation, project-gate, all
three C++ oracle, and fixed committed-tree independent-review gates remain to
be run before binding a new verified commit.
