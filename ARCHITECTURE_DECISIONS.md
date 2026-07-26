# CDDA Rust Multiplayer Port Architecture Decisions

Status: Living design record
Last updated: 2026-07-26

## Purpose

This document records the architectural direction for a multiplayer Rust port
of Cataclysm: Dark Days Ahead (CDDA). The project is a derivative port rather
than an independently authored game merely inspired by CDDA. It aims to
preserve and adapt CDDA behavior and content while replacing the implementation
architecture and adding persistent multiplayer.

This is a semantic port, not a commitment to translate the C++ implementation
line by line or preserve its internal class structure and control flow.

The primary architectural goal is to make multiplayer, headless simulation,
testing, and long-term maintainability foundational properties rather than
features added after the single-player game is complete.

## Confirmed decisions

### Build a derivative Rust multiplayer port of CDDA

CDDA is the authoritative behavioral and content reference. Existing mechanics,
data, text, and assets may be reused or converted where their licenses permit.
Behavior may be deliberately adapted where real-time persistent multiplayer
requires different rules.

The project must comply with CDDA's share-alike license, preserve required
attribution, and track separately licensed bundled material. License provenance
must be retained during automated imports and manual ports.

### Use Bevy for the client

The graphical client will use Bevy for rendering, windowing, input, audio,
assets, and presentation-layer ECS.

Bevy's renderer is built on wgpu, so specialized rendering can still use
lower-level rendering APIs if required. Using Bevy avoids building unrelated
engine infrastructure before working on the simulation and multiplayer.

The client is a view and input device. It is not authoritative over gameplay.

### Use a minimal, headless Bevy server

The authoritative server will use `bevy_app` and `bevy_ecs` without client
rendering, windowing, or audio features.

The server must be runnable:

- As a dedicated standalone process
- Locally for a hosted or single-player session
- In automated tests without a GPU, window server, or audio device

Client and server may share simulation and protocol crates, but the server must
not depend on the graphical client.

### Run a persistent shared-world server

The primary multiplayer target is a persistent server rather than a temporary
session hosted only while a group is playing.

- Authoritative world state outlives individual connections and server process
  restarts.
- Players may connect to and leave the same continuing world independently.
- Character identity and ownership must be authenticated and durable.
- No individual client owns the world lifecycle or may reset canonical state.
- Persistence, recovery, backups, and migrations are production runtime
  concerns rather than optional tooling.

The world clock continues to advance when zero players are connected. The
server must therefore process scheduled global and character effects without
requiring an online player. Empty and inactive regions may use coarse or
analytical catch-up rather than full-detail simulation.

Disconnected characters remain physically present in the canonical world and
receive no special protection. They remain subject to ordinary simulation,
including needs, environmental hazards, attacks, injury, theft, and death. A
reconnecting player resumes control of the same actor if it is still available.
Disconnecting must never become an escape, teleport, invulnerability, or
simulation-freeze mechanism.

What an uncontrolled character attempts to do after disconnection remains an
open decision. Possibilities include continuing its current activity, waiting,
sleeping, following a previously selected standing order, or using limited
defensive AI.

### Target 16 concurrent players initially

The initial supported concurrency target is 16 simultaneously connected
players in one persistent world. This is not a limit on registered accounts,
stored characters, NPCs, or world size.

The architecture should avoid assumptions that make later scaling impossible,
but the first implementation will optimize for correctness and a good
16-player experience rather than speculative MMO-scale distribution.

### Target native desktop clients and a dedicated server

The initial client platforms are Windows, macOS, and Linux. A headless Linux
dedicated server is required. Supporting the dedicated server on Windows and
macOS is desirable when it follows naturally from portable Rust code, but is
not an initial deployment requirement.

Browser, mobile, console, and platform-specific network services are outside
the initial target.

### Use a hybrid ECS simulation

ECS will model independently acting objects that participate in multiple game
systems. Purpose-built Rust data structures will model data with strong
internal invariants, dense spatial layout, or deep containment.

The rule of thumb is:

> Use ECS for independently acting things; use domain-specific structures for
> aggregates and dense world data.

Initial representation guidelines:

| Concept | Likely representation |
| --- | --- |
| Players, NPCs, and monsters | ECS entities with coarse-grained components |
| Position, health, movement, and faction | ECS components |
| Status effects | An effect collection component rather than one component type per effect |
| Vehicles | ECS entity containing a vehicle aggregate |
| Vehicle parts | Dense collection owned by the vehicle aggregate |
| Terrain, furniture, and environmental fields | Chunk-local dense or sparse structures |
| Items and nested containers | Stable item IDs with an arena or containment graph |
| Ground item piles | Chunk-local collections of item IDs |
| Recipes and content definitions | Immutable registries/resources |
| Long-running activities | Explicit state machines driven by commands and events |
| Sprites, animation, and UI | Client-only presentation entities |

This avoids forcing every tile, item part, or nested object into a standalone
ECS entity while retaining ECS benefits for actors and cross-cutting systems.

### Use chunked world storage

Terrain and other spatial world data will be divided into addressable chunks.
Chunks are the natural unit for:

- Loading and unloading world regions
- Persistence and incremental saving
- Network interest management
- Dirty tracking and state revisions
- Spatial queries and simulation activation

Tiles will not be individual ECS entities. Chunk dimensions and the exact data
layout remain to be selected through representative benchmarks.

### Use explicit stable IDs

Persistent domain objects will have explicit typed identifiers such as
`ActorId`, `ItemId`, `VehicleId`, and `ChunkId`.

Bevy `Entity` values, memory addresses, and collection indices must not be used
as durable identity in save files or network messages. Runtime mappings may
associate stable IDs with local ECS entities.

ID generation must be controlled by the authoritative server and must remain
valid across saving, loading, replication, and replay.

### Use server-authoritative commands and events

Clients send intentions. The server validates them, advances the simulation,
and emits resulting events and state deltas.

The conceptual flow is:

```text
client input
    -> typed command
    -> server validation
    -> authoritative simulation step
    -> domain events
    -> relevance-filtered state deltas
    -> client presentation
```

Examples of commands include moving, attacking, beginning an activity, using
an item, or selecting dialogue. Events describe accepted outcomes such as an
actor moving, damage being applied, an item changing ownership, or a sound
being produced.

Clients must not directly mutate canonical world state, and the wire protocol
must not simply mirror arbitrary Bevy component changes.

### Keep tests and replays independent of rendering

Simulation tests must run without initializing the renderer. Important
scenarios should be expressible as initial state plus a sequence of timestamped
or ordered commands.

The simulation should produce a recordable event stream and deterministic
state fingerprints where practical. This enables:

- Fast headless integration tests
- Regression fixtures
- Desync diagnosis
- Server debugging
- Replay inspection
- Performance benchmarking

Bit-for-bit determinism across every supported platform is not yet a confirmed
requirement. Stable command ordering, explicit simulation phases, controlled
randomness, and reproducibility on the authoritative server are requirements.

### Do not perform a line-by-line C++ rewrite

CDDA is a behavioral and design reference, not an architecture template.

Features will be implemented as vertical slices against the new multiplayer
architecture. We may write compatibility or behavior tests for mechanics worth
preserving, but we will not preserve legacy class boundaries, global state, or
control flow merely because they exist upstream.

An illustrative early vertical slice is:

1. Load one world chunk.
2. Connect two players.
3. Move both players through the same authoritative simulation.
4. Add one creature with perception and pathfinding.
5. Resolve one combat interaction.
6. Save, reload, and replay the interaction.

This proves the core architecture before large amounts of content are ported or
created.

## Derived architectural constraints

The confirmed decisions imply the following constraints:

- Simulation code must not depend on rendering or client input types.
- Network and save schemas must use domain types rather than Bevy internals.
- The server owns identity allocation, random outcomes, validation, and time.
- Presentation entities may be freely created or destroyed without affecting
  canonical simulation identity.
- Simulation work must be arranged into explicit phases where outcome ordering
  matters. Parallel execution is permitted only where its result is invariant
  to execution order.
- Randomness should come from named or otherwise controlled streams so failures
  can be reproduced.
- Large state replication must be relevance-filtered, most likely by chunks,
  visibility, ownership, and perception.
- Persistence should operate incrementally at world scale rather than requiring
  a monolithic snapshot for every save.
- The initial architecture must support the same simulation in dedicated,
  locally hosted, test, and replay modes.
- The persistent server must recover to a consistent state after an ordinary
  shutdown or crash without trusting clients to reconstruct canonical state.
- Authentication and authorization must be enforced at the command boundary,
  including character ownership and administrative operations.
- Unobserved regions need an explicit inactive or coarse-simulation policy so a
  persistent world does not require full-detail simulation everywhere.

## Tentative Rust workspace boundaries

These crate boundaries are a starting hypothesis rather than a frozen public
API:

```text
crates/
  sim/         authoritative mechanics and world state
  content/     definitions, loading, validation, and migrations
  protocol/    commands, events, snapshots, and wire types
  persistence/ chunk and entity storage
  server/      sessions, networking, interest management, and simulation loop
  client/      Bevy rendering, UI, input, audio, and client prediction
  tools/       validators, importers, benchmarks, and replay inspection
```

Dependency direction should point inward toward domain logic. In particular,
`sim` must not depend on `client`, and `protocol` should remain usable by both
client and server without graphical dependencies.

## Compatibility and licensing boundary

This is a derivative CDDA port. The upstream repository states that its code
and content are licensed under CC BY-SA 3.0, with separately licensed bundled
material. The port must retain attribution and share-alike obligations and must
not assume that every vendored file has the same license as core CDDA.

Import tooling should retain source paths and provenance where practical. A
license and attribution audit is required before redistribution. Specific legal
conclusions should be confirmed separately rather than inferred solely from
this architecture document.

## Technology candidates to validate in the first prototype

The following choices are recommended starting points, not yet confirmed
long-term dependencies:

| Area | Candidate |
| --- | --- |
| Rust toolchain | Stable Rust, edition 2024 |
| Game framework | Bevy 0.19 with minimal feature sets per binary |
| Networking | Lightyear 0.28 behind a project-owned networking adapter |
| Networking fallback | `bevy_renet` 5.0 if Lightyear is too opinionated |
| Serialization | Serde with explicit domain protocol and persistence records |
| Persistence | SQLite through `rusqlite`, using transactions and WAL |
| Diagnostics | `tracing` and `tracing-subscriber` |

The networking prototype must use stable domain IDs and explicit commands,
events, and chunk deltas. Automatic replication of the canonical Bevy world is
not part of the architecture even if the selected networking library supports
it.

## Working decision: real-time global action time

The current direction is one global simulation timeline paced in real time.
This is a working decision that may be revisited after prototyping.

- The authoritative server owns a single integer simulation clock.
- Simulation time normally advances in proportion to wall-clock time.
- Simulation time continues when no clients are connected.
- Actions have durations; actors remain busy until their action completes or is
  interrupted.
- Players may act simultaneously and do not wait for every other player to
  submit a command.
- Players in distant regions do not receive independent timelines or time
  bubbles.
- Network update frequency, rendering frame rate, and simulation-time
  resolution are separate configuration concerns.
- Headless tests and replays advance simulation time without waiting for wall
  time.
- Server-approved pause and acceleration may change pacing without changing the
  underlying simulation semantics.

The implementation may use fixed simulation steps, an event-driven scheduler,
or a combination of the two. That mechanism remains open until representative
movement, combat, vehicle, field, and long-activity workloads are prototyped.

## Open decisions

The following have intentionally not been selected yet:

1. Autonomous behavior and standing orders for disconnected characters
2. Gameplay pause authority, time acceleration, and long-activity behavior
3. Full-detail versus coarse simulation for inactive regions
4. Simulation-time resolution and fixed-step versus event-driven scheduling
5. Command buffering, cancellation, and interruption rules
6. Client prediction and reconciliation policy
7. Final network transport and serialization format
8. Persistence format, recovery, backup, and migration strategy
9. Authentication, accounts, and character ownership model
10. CDDA content import, validation, and mod compatibility strategy
11. Chunk dimensions and active simulation radius
12. Exact ECS component granularity
13. Random number stream and replay format
14. Minimum supported hardware and performance budgets

## Next time-model decisions

The real-time direction still requires explicit policies for:

- What happens to an actor while its player is using a menu
- Whether any gameplay pause exists beyond administrative maintenance
- When sleeping, crafting, reading, travel, and other long activities may
  accelerate global time, if acceleration is allowed at all
- How threats and interruptions stop acceleration
- What an uncontrolled character does after its client disconnects
- How many commands a client may queue and when queued actions may be canceled
- Whether the server uses fixed steps, scheduled completion events, or both

These policies should be decided before implementing the authoritative
simulation loop because they shape command scheduling, latency handling,
activities, combat, vehicles, NPC processing, and replay timestamps.
