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

- As a dedicated standalone process on macOS and Linux
- Locally for a hosted or single-player session
- In automated tests without a GPU, window server, or audio device

Client and server may share simulation and protocol crates, but the server must
not depend on the graphical client. macOS is a mandatory development platform
for both binaries from the first vertical slice; server compatibility cannot be
deferred until after Linux deployment works.

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

On disconnection, held movement and steering input clears at the next tick. The
character continues its current activity; normal danger rules interrupt it only
when its definition permits interruption. Server-controlled survival autopilot
starts when that activity completes or is interrupted. The autopilot may
defend, flee nearby threats, extinguish fire, leave dangerous terrain or
atmosphere, seek shelter within the current reality bubble, use ordinary food,
medicine, and wielded equipment, and sleep when safe. It does not initiate
combat, loot, travel beyond the current bubble, begin projects, spend unique
resources, make dialogue or faction choices, or change equipment loadouts.
Reconnection returns control at the next simulation boundary without moving or
otherwise protecting the actor.

### Target 16 concurrent players initially

The initial supported concurrency target is 16 simultaneously connected
players in one persistent world. This is not a limit on registered accounts,
stored characters, NPCs, or world size.

The architecture must not impose an artificial 16-player data-model limit, but
the first implementation optimizes for correctness and a good
16-player experience rather than speculative MMO-scale distribution.

### Target native desktop clients and a dedicated server

The initial client platforms are Windows, macOS, and Linux. Required headless
dedicated-server platforms are macOS 13 or newer on Apple silicon and Intel,
plus x86-64 GNU/Linux with glibc 2.35 or newer. A Windows dedicated-server build
is not an initial release requirement.

Browser, mobile, console, and platform-specific network services are outside
the initial target.

### Use a hybrid ECS simulation

ECS will model independently acting objects that participate in multiple game
systems. Purpose-built Rust data structures will model data with strong
internal invariants, dense spatial layout, or deep containment.

The rule of thumb is:

> Use ECS for independently acting things; use domain-specific structures for
> aggregates and dense world data.

Required representation boundaries:

| Concept | Representation |
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
| Projectiles and active explosions | Short-lived ECS entities |
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

Tiles will not be individual ECS entities. The atomic map chunk is one CDDA
submap: 12 by 12 tiles on one z-level. A connected player receives full 20 Hz
simulation in an 11 by 11 submap square centered on its current submap and on
the current and adjacent z-levels. Network prefetch uses a 13 by 13 square.
Overlapping bubbles are merged rather than simulated twice.

### Use explicit stable IDs

Persistent domain objects will have explicit typed identifiers such as
`ActorId`, `ItemId`, `VehicleId`, and `ChunkId`.

Bevy `Entity` values, memory addresses, and collection indices must not be used
as durable identity in save files or network messages. Runtime mappings may
associate stable IDs with local ECS entities.

Each ID is a typed 128-bit value composed of a persistent random 64-bit world
namespace and a monotonically allocated 64-bit counter. The world namespace is
generated with the operating system CSPRNG. The server allocator reserves
blocks of 4,096 counters by advancing the persisted high-water mark in a
dedicated SQLite transaction before issuing any ID from a block. Unused or
rolled-back IDs are skipped permanently, and IDs remain valid across saving,
loading, replication, and replay.

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
scenarios must be expressible as initial state plus a sequence of timestamped or
ordered commands.

The simulation produces a recordable event stream and deterministic state
fingerprints. This enables:

- Fast headless integration tests
- Regression fixtures
- Desync diagnosis
- Server debugging
- Replay inspection
- Performance benchmarking

Canonical simulation and replay are bit-for-bit deterministic across supported
platforms. Stable command ordering, fixed-point canonical arithmetic, explicit
simulation phases, controlled randomness, and canonical serialization are
requirements. Floating-point values are presentation-only.

### Do not perform a line-by-line C++ rewrite

CDDA is a behavioral and design reference, not an architecture template.

Features are implemented as vertical slices against the new multiplayer
architecture. Compatibility and characterization tests preserve observable
mechanics; legacy class boundaries, global state, and control flow are not
preserved merely because they exist upstream.

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
- Randomness comes from named ChaCha8 streams so failures can be reproduced.
- Large state replication is filtered by chunks, visibility, ownership, and
  perception.
- Persistence operates incrementally at world scale instead of requiring a
  monolithic snapshot for every save.
- The initial architecture must support the same simulation in dedicated,
  locally hosted, test, and replay modes.
- The persistent server must recover to a consistent state after an ordinary
  shutdown or crash without trusting clients to reconstruct canonical state.
- Authentication and authorization must be enforced at the command boundary,
  including character ownership and administrative operations.
- Unobserved regions use the fixed Active, Warm, and Dormant policies below.

## Rust workspace boundaries

The workspace uses these crate boundaries:

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

Dependency direction points inward toward domain logic. `sim` must not depend
on `client`, and `protocol` must remain usable by client, server, headless tests,
and replay tools without graphical dependencies.

## Compatibility and licensing boundary

This is a derivative CDDA port. Original Rust source and project documentation
are released under CC BY-SA 3.0. The port retains upstream attribution and
share-alike obligations. Separately licensed dependencies and bundled material
retain their compatible licenses and notices rather than being relicensed.

Import tooling retains source paths, hashes, upstream commit, license, and
provenance. The build excludes any asset with unknown or incompatible
provenance. A license and attribution audit is a mandatory release gate.

## Pinned upstream baseline and content scope

The initial parity baseline is CDDA commit
`4dfd36038b16650dc1b5cb9d79a3e42363174b05` from the upstream `master`
branch. This baseline does not move until the completion gate is satisfied.

The port ships all compatible gameplay data in `data/core`, `data/json`,
`data/names`, and `data/raw`, plus every bundled mod under `data/mods` at the
pinned commit. `TEST_DATA` and `Standard_Combat_Tests` are imported as test
fixtures rather than player-selectable mods. Applicable fonts, sound, title,
shader, and tileset assets are included only after their individual licenses
and provenance are recorded. Android, browser, screenshot, packaging, and XDG
artifacts are outside the initial platform scope.

The import tool vendors the selected upstream files into the Rust project
without semantic rewrites, emits a manifest containing the upstream path,
commit, BLAKE3 hash, license, and destination for every file, and fails on
unknown provenance. The runtime loader implements the pinned CDDA JSON,
inheritance, finalization, dependency, replacement, and effect-on-condition
semantics. RON and a newly invented content format are not used.

## Locked technology stack

| Area | Decision |
| --- | --- |
| Rust toolchain | Rust 1.97.1, pinned by `rust-toolchain.toml`, edition 2024 |
| Client | Bevy 0.19.0 with explicit 2D, UI, audio, asset, input, accessibility, and platform features |
| Server | `bevy_app`, `bevy_ecs`, `bevy_time`, and `bevy_tasks` 0.19.0 without render/window/audio features |
| Networking | Lightyear 0.28.0 using native UDP and its Netcode connection layer |
| Network encoding | Lightyear registered Serde messages with its bincode serializer and protocol version 1 |
| Persistence | SQLite bundled through `rusqlite` 0.40.1 in WAL mode |
| Persistence blobs | Versioned `postcard` 1.1.3 records compressed with `zstd` 0.13.3 |
| Async control plane | Tokio 1.53.1, Axum 0.8.9, `axum-server` 0.8.0, and rustls 0.23.42 HTTPS |
| Authentication | RustCrypto `argon2` 0.5.3 using Argon2id plus opaque random session tokens |
| Deterministic RNG | `rand_chacha` 0.10.0 `ChaCha8Rng` with BLAKE3 1.8.5-derived named streams |
| Diagnostics | `tracing` 0.1.44, `tracing-subscriber` 0.3.23, and a Prometheus-format metrics endpoint |

Direct dependencies use exact `=` version constraints and all dependencies are
pinned in `Cargo.lock`. Networking and persistence remain
behind project-owned interfaces, but Lightyear and SQLite are the implementation
targets. Automatic replication of the canonical Bevy world is prohibited.

## Network and authentication policy

The server exposes a rustls HTTPS control API for login, character selection,
and issuance of a 30-second Lightyear Netcode connection token. Production
servers require a configured CA-valid certificate. Local development uses a
generated certificate explicitly pinned by the local client. Plaintext remote
authentication is prohibited.

Accounts use canonical ASCII names matching `[a-z0-9_]{3,32}` and passwords of
12 through 256 bytes. Passwords are stored as Argon2id PHC strings using 64 MiB
memory, three iterations, four lanes, a random 16-byte salt, and a 32-byte
output. Public self-registration is disabled; administrators create accounts or
one-use invitations through the server CLI. Authentication is limited to five
failed attempts per account and source address per 15 minutes. Salts,
invitation tokens, session tokens, and Netcode private keys come from the
operating system CSPRNG.

Successful login returns an opaque random 256-bit session token; only its
BLAKE3 hash is stored. Sessions expire after 24 hours and are revoked on password
change or administrative action. One-use invitation tokens use the same size,
storage, and 24-hour expiration policy. Roles are player, moderator, and
administrator. An account owns multiple characters, but exactly one gameplay
connection per account and per character is active at a time. A second
connection is rejected unless it presents the same reconnect session or an
administrator explicitly replaces or transfers control.

Lightyear carries explicit project messages over these channels:

- Redundantly transmitted unreliable-sequenced input for held movement and
  vehicle controls
- Reliable-ordered semantic commands, command results, chat, and administration
- Reliable-ordered entity lifecycle and critical domain events
- Reliable-unordered fragmented chunk snapshots and content manifests
- Unreliable-sequenced actor and vehicle state deltas

Every payload uses fixed-width domain numbers, typed stable IDs, an explicit
protocol version, bounded collection lengths, and server-side authorization.

## Deployment and content handshake

Each persistent world has one authoritative server runtime and one SQLite
database. Worlds are not sharded or federated. A dedicated operator exposes one
HTTPS control endpoint and one UDP gameplay endpoint. Accounts and roles are
local to that server world.

Clients connect directly by hostname or IP address and may save favorites.
There is no central account service, matchmaking service, public server
directory, relay, or automatic NAT traversal. Local play embeds the same
headless server crates in the client process. Single-player uses the in-process
transport only; locally hosted multiplayer also exposes the HTTPS and UDP
endpoints for remote clients. The host's client-facing command boundary remains
identical and non-authoritative.

The connection handshake compares protocol version, baseline commit,
content-manifest hash, and ordered enabled-mod list before character selection.
A mismatch is rejected with all differing values in the diagnostic response.
The client never downloads or executes server-provided content automatically.
Third-party mods are outside the pinned completion scope; operators using them
install identical files manually on the server and clients, and those files
participate in the manifest hash. Server-routed text chat is included. Voice
chat is outside the initial product scope.

## Real-time simulation and activities

The authoritative clock uses `SimTick(u64)` at 20 Hz: one tick is 50
milliseconds and one simulation second equals one wall-clock second. Network
state is sent at 10 Hz. Rendering is variable-rate and interpolated.

Each tick runs these ordered phases:

1. Network ingress and input collection
2. Authentication/ownership and command validation
3. Action start and interruption
4. Movement, vehicle motion, projectiles, and collision
5. Action completion and combat resolution
6. Effects, needs, fields, weather, and environmental processing
7. AI decisions
8. Cleanup, stable-ID allocation, and dirty marking
9. Persistence journal batching
10. Relevance calculation and replication

A stable priority queue keyed by `(due_tick, event_sequence)` handles scheduled
work. Deterministic conflicts at the same tick are ordered by phase, actor
readiness, stable `ActorId`, and command sequence.

Players cannot pause or accelerate the world. Menus do not pause; the actor
finishes its current action and then guards in place until another command is
received. Crafting, reading, construction, sleep, healing, and travel consume
their real simulation durations and continue while disconnected. Threats and
hazards interrupt them using CDDA interruption rules, after which survival
autopilot acts.

Only an administrator may enter maintenance pause. Maintenance pause first
stops command intake, checkpoints the database, disconnects clients, and then
freezes the simulation clock. Planned maintenance downtime does not advance
world time. Unexpected process downtime does advance world time: on restart the
server uses the persisted UTC anchor to perform deterministic analytical
catch-up before accepting connections.

## Region activation and catch-up

Region processing has three fixed tiers:

- **Active:** 20 Hz full simulation for the 11 by 11 submap bubble around each
  connected player and for any chunk involved in combat, moving vehicles,
  projectiles, explosions, or immediate hazards.
- **Warm:** 1 Hz coarse spatial simulation for the same-size bubble around each
  disconnected character, NPC camp, persistent fire, or other live anchor.
  Hostile contact or a complex hazard promotes affected chunks to Active until
  the interaction resolves.
- **Dormant:** unloaded state represented by persisted scheduled events and a
  `last_sim_tick`. Loading applies deterministic analytical catch-up for needs,
  rot, plants, weather exposure, power, fields, and other elapsed-time systems
  before the chunk can become Warm or Active.

Tier transitions occur only at tick boundaries. Overlapping bubbles take the
highest tier. Offline characters therefore remain vulnerable without forcing
the entire generated world to run at 20 Hz.

## Command, prediction, and reconciliation policy

An actor has one active action and at most two queued semantic commands.
Canceling a queued command is free. An active action may be canceled only when
its activity definition marks it interruptible; elapsed time and already-paid
costs are retained. Invalidated queued commands are rejected with a typed reason
and removed. Held movement and vehicle steering are stateful inputs rather than
queued semantic commands.

The client predicts only its controlled actor's locomotion, vehicle steering,
camera, and cosmetic effects. Inventory, combat, projectiles, RNG outcomes,
item use, crafting, dialogue, and world interactions are never predicted.
Remote actors render with a 100-millisecond interpolation delay. Reconciliation
uses authoritative tick-tagged snapshots: corrections within one tile are
smoothed over 150 milliseconds, while larger or collision-invalid divergence
snaps immediately and records a diagnostic event.

## Persistence, recovery, and backups

Each world uses one SQLite database with `journal_mode=WAL`,
`synchronous=FULL`, foreign keys enabled, and a single persistence worker.
Numbered forward-only SQL migrations run transactionally after an automatic
backup. Downgrades are unsupported.

Commands and authoritative domain events are committed to an append-only
journal every network frame (100 milliseconds) before their durable results are
acknowledged. Dirty entity and submap snapshots are written every five seconds
in atomic batches. Recovery loads the latest snapshots and replays later journal
entries. WAL checkpoints run every 60 seconds and on graceful shutdown.

Compressed replay archives roll hourly and are retained for 30 days. After a
verified snapshot and replay-archive write, daily compaction removes
recovery-journal rows older than that snapshot; SQLite reuses the freed pages.
Compaction never removes a snapshot object referenced by a retained replay.

The server creates verified online backups hourly, retaining 24 hourly and 30
daily generations. Each backup includes the database, content-manifest hash,
baseline commit, schema version, protocol version, and BLAKE3 checksum. Restore
always verifies integrity and replays the journal before opening the world.

## Determinism and replay

Canonical simulation uses integer or fixed-point arithmetic. Collections whose
iteration affects outcomes are ordered by stable IDs; hash-map iteration never
determines behavior.

Each world stores a 256-bit seed. Random streams are ChaCha8 streams derived
with BLAKE3 from the world seed, a domain tag, relevant stable IDs, tick, and
event sequence. Named streams isolate combat, map generation, weather, loot,
and AI so adding a random call in one domain does not perturb another.

The replay format is a versioned Postcard stream compressed with Zstandard. Its
header contains the baseline commit, content-manifest hash, protocol and schema
versions, world namespace, seed, initial snapshot hash, and start tick. Records
contain accepted commands, administrative world events, connection-control
changes that affect actors, and BLAKE3 canonical state hashes every 100 ticks.
The same replay must produce identical hashes on Windows, macOS, and Linux.

At each hourly replay roll, the server persists an immutable canonical
initial-snapshot object addressed by its BLAKE3 hash before writing the replay
header. The object is retained at least as long as every archive that references
it and is deleted only when no replay or backup references it. Replay export
produces a self-contained bundle containing the replay stream, its snapshot
object, and the matching content manifest; replay import verifies every hash
before execution.

## Performance and supported hardware

The minimum x86-64 client target is a four-core CPU, 8 GiB RAM, and a GPU with
2 GiB VRAM supporting Direct3D 12 on Windows 10+, Metal on macOS 13+, or Vulkan
1.2 on GNU/Linux with glibc 2.35 or newer and X11 or Wayland. The minimum Apple
silicon client is a base Apple M1 with 8 GiB unified memory on macOS 13. Every
required client target must sustain 60 frames per second at 1920 by 1080 in the
standard tiles view on its minimum hardware.

The 16-player dedicated-server target is four dedicated CPU cores on x86-64 or
four Apple silicon performance cores, 8 GiB RAM, NVMe storage, and 20 Mbit/s
symmetric network capacity. Run the standard 16-client workload natively on
`aarch64-apple-darwin`, `x86_64-apple-darwin`, and
`x86_64-unknown-linux-gnu`:

- Simulation tick time is below 35 ms at p95 and 50 ms at p99.
- Server resident memory remains below 4 GiB.
- Steady-state egress averages below 256 Kbit/s per client.
- A reconnecting client receives playable state within five seconds on a 20
  Mbit/s connection.
- Cold server startup and content validation complete within 60 seconds.
- A 24-hour soak with 16 connected clients and 64 additional disconnected
  characters has no crash, desync, database corruption, unbounded growth, or
  missed tick deadline above 0.1 percent.

These are release gates, not optional optimization goals.
