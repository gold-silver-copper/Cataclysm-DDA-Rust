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
Bevy is a client-only dependency: no server, simulation, protocol, persistence,
content, or tooling crate may depend directly or transitively on `bevy` or any
`bevy_*` crate.

### Use a plain-Rust server with no Bevy dependencies

The authoritative server is a conventional Rust application built on Tokio and
project-owned domain crates. It does not use `bevy_app`, `bevy_ecs`,
`bevy_time`, `bevy_tasks`, or any other Bevy crate. Its lifecycle, scheduling,
simulation phases, storage, and networking are explicit Rust services rather
than Bevy plugins, systems, resources, or schedules.

A Tokio multithread runtime owns networking, connection tasks, signals, timers,
and bounded background work. One dedicated simulation thread exclusively owns
canonical `WorldState`, drains bounded command queues at tick boundaries, runs
the ordered 20 Hz phases, and publishes immutable output batches. Simulation
code never awaits while holding world state. The existing single persistence
worker consumes journal and snapshot batches independently.

Thread-boundary queues are fixed: network-to-simulation ingress holds 4,096
envelopes or 16 MiB; simulation-to-persistence holds 64 journal batches or 32
MiB plus two coalescible snapshot jobs; the global simulation-to-network
reliable queue holds 256 batches or 32 MiB; and each client reliable-delivery
queue holds 128 frames or 8 MiB. Unreliable state uses one latest-value slot per
state class rather than a growing queue. A full ingress queue returns
`ServerBusy` for reliable commands and drops unreliable samples. A full client
delivery queue disconnects that slow client, which later reconnects from a
snapshot; it never blocks canonical simulation.

Persistence and global reliable-output capacity are fail-closed. Before each
100-millisecond journal batch, the simulation reserves one slot in both queues
and retains a reversible pre-batch checkpoint until SQLite acknowledges the
journal commit. If capacity is unavailable, a persistence write fails, or the
global network dispatcher cannot accept the committed batch, the server stops
command admission, starts no further ticks, rolls back any uncommitted batch,
publishes no uncommitted outcome, and enters maintenance after the last durable
boundary. Snapshot jobs may coalesce dirty sets because the journal remains the
recovery authority; journal or critical-event batches are never dropped. Fault-
injection tests must cover every saturation and worker-failure transition.

The server must be runnable:

- As a dedicated standalone process on macOS and Linux
- Locally for a hosted or single-player session
- In automated tests without a GPU, window server, or audio device

Client and server may share simulation and protocol crates, but only the client
may adapt domain state into Bevy entities and components. macOS is a mandatory
development platform for both binaries from the first vertical slice; server
compatibility cannot be deferred until after Linux deployment works.

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
is not an initial release requirement. Consequently, the initial Windows client
is remote-client-only; local single-player and locally hosted worlds require a
supported macOS or Linux server host.

Browser, mobile, console, and platform-specific network services are outside
the initial target.

### Use a plain-Rust domain simulation, not ECS

The first complete port will not partially migrate CDDA gameplay into ECS.
Canonical simulation state is represented by typed Rust records, aggregates,
stable-ID registries, containment graphs, queues, and chunk stores. Simulation
phases are ordinary functions and services over that state.

This boundary applies to the entire authoritative implementation in this plan,
not just the first vertical slice. A server-side ECS migration is outside the
pinned port scope and must not be anticipated through dual representations or
ECS-shaped protocol and persistence schemas. The Bevy client may use
presentation entities for rendering, interpolation, animation, and UI, but
those entities are a disposable view of authoritative domain state.

Required representation boundaries:

| Concept | Representation |
| --- | --- |
| Players, NPCs, and monsters | Stable-ID keyed actor records in domain registries |
| Position, health, movement, and faction | Fields and invariant-preserving substructures owned by actor records |
| Status effects | Effect collections owned by affected domain objects |
| Vehicles | Stable-ID keyed vehicle aggregates |
| Vehicle parts | Dense collection owned by the vehicle aggregate |
| Terrain, furniture, and environmental fields | Chunk-local dense or sparse structures |
| Items and nested containers | Stable item IDs with an arena or containment graph |
| Ground item piles | Chunk-local collections of item IDs |
| Recipes and content definitions | Immutable registries/resources |
| Long-running activities | Explicit state machines driven by commands and events |
| Projectiles and active explosions | Stable-ID keyed short-lived records and ordered work queues |
| Sprites, animation, and UI | Client-only Bevy presentation entities |

Indexes derived from canonical records may accelerate spatial and cross-cutting
queries, but they are rebuilt or validated from authoritative state and never
become a second source of truth.

### Use chunked world storage

Terrain and other spatial world data will be divided into addressable chunks.
Chunks are the natural unit for:

- Loading and unloading world regions
- Persistence and incremental saving
- Network interest management
- Dirty tracking and state revisions
- Spatial queries and simulation activation

Tiles are chunk-owned domain data. The atomic map chunk is one CDDA submap: 12
by 12 tiles on one z-level. A connected player receives full 20 Hz
simulation in an 11 by 11 submap square centered on its current submap and on
the current and adjacent z-levels. Network prefetch uses a 13 by 13 square.
Overlapping bubbles are merged rather than simulated twice.

### Use explicit stable IDs

Persistent domain objects will have explicit typed identifiers such as
`ActorId`, `ItemId`, `VehicleId`, and `ChunkId`.

Bevy `Entity` values, memory addresses, and collection indices must not be used
as durable identity in save files or network messages. The client may maintain
a disposable mapping from stable IDs to local Bevy presentation entities; the
server and shared domain crates never receive or store a Bevy `Entity`.

Each ID is a typed 128-bit value composed of a persistent random 64-bit world
namespace and a monotonically allocated 64-bit counter. The world namespace is
generated with the operating system CSPRNG. The server allocator reserves
blocks of 4,096 counters by advancing the persisted high-water mark in a
dedicated SQLite transaction before issuing any ID from a block. Unused or
rolled-back IDs are skipped permanently, and IDs remain valid across saving,
loading, replication, and replay.

`IdBlockReserved` and `IdBlockAbandoned` are authoritative recovery inputs and
replay records. If a crash leaves the SQLite high-water mark ahead of the last
journaled allocator cursor, recovery emits `IdBlockAbandoned` for that entire
unconfirmed remainder before allocating again. Replay consumes these records
instead of reserving fresh database ranges, so deliberately burned IDs and the
allocator high-water mark reproduce exactly.

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

Clients must not directly mutate canonical world state. The wire protocol is a
domain contract and must not mirror arbitrary client components or presentation
state.

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
- Network and save schemas must use domain types; Bevy types never cross the
  client boundary.
- Authoritative simulation uses one plain-Rust representation rather than ECS,
  shadow components, or dual-write migration structures.
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
  server/      connections, networking, interest management, and simulation loop
  client/      Bevy rendering, UI, input, audio, and client prediction
  tools/       validators, importers, benchmarks, and replay inspection
```

Dependency direction points inward toward domain logic. `sim` must not depend
on `client`, and `protocol` must remain usable by client, server, headless tests,
and replay tools without graphical dependencies. Only `client` may depend on
Bevy. `server` and every crate reachable from it must have a Bevy-free Cargo
dependency graph.

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

The pinned Git commit does not contain the non-English catalogs that upstream
release automation fetches from Transifex; its tracked `lang/po` directory has
only a placeholder. The initial parity target therefore ships the pinned
English source strings and English UI only. External non-English catalogs and
runtime language switching are an explicit standing parity exclusion rather
than an implicit missing input. Builds never fetch translations from a mutable
external service.

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
| Server | Plain Rust on Tokio 1.53.1; no Bevy crate is permitted in its dependency graph |
| Networking | `iroh` 1.0.3 authenticated QUIC connections using fixed project ALPNs |
| Network encoding | Explicit versioned Serde domain messages encoded with `postcard` 1.1.3 |
| Persistence | SQLite bundled through `rusqlite` 0.40.1 in WAL mode |
| Persistence blobs | Versioned `postcard` 1.1.3 records compressed with `zstd` 0.13.3 |
| Async runtime | Tokio 1.53.1 for iroh I/O, signals, timers, and bounded background work |
| Authentication | Iroh-authenticated `EndpointId` allowlists mapped to durable accounts and roles; no passwords or bearer tokens |
| Deterministic RNG | `rand_chacha` 0.10.0 `ChaCha8Rng` with BLAKE3 1.8.5-derived named streams |
| Diagnostics | `tracing` 0.1.44, `tracing-subscriber` 0.3.23, and a loopback-only Prometheus-format metrics endpoint |

Direct dependencies use exact `=` version constraints and all dependencies are
pinned in `Cargo.lock`. Networking and persistence remain behind project-owned
interfaces, but iroh and SQLite are the implementation targets. Only
`crates/client/Cargo.toml` may declare Bevy dependencies. A workspace dependency
boundary check must reject any `bevy` or `bevy_*` package reachable from a
non-client crate. Iroh 1.0.3 is the latest stable release verified on the
document's last-updated date and is pinned as `iroh = "=1.0.3"`.

## Network and authentication policy

All gameplay, connection-control, authentication, chat, and remote-administration
traffic uses `iroh` 1.0.3. The server persists one iroh `SecretKey` per world
and advertises its corresponding `EndpointAddr`, which includes the
cryptographic `EndpointId` plus current direct and relay addressing hints. A
client favorite pins that `EndpointId`; an operator shares a changed address
out of band after intentional key rotation. The fixed ALPNs are
`cdda-rust/game/1` for gameplay, `cdda-rust/enroll/1` only to prove possession
of a preauthorized endpoint key, and `cdda-rust/admin/1` for sensitive remote
administration. There is no parallel web login API, connection-token exchange,
or plaintext authentication path.

Each client installation persists one iroh `SecretKey`; its public
`EndpointId` is the only client authentication identity. The client stores the
key using operating-system credential protection when available, otherwise in
an owner-only file or ACL. Secret keys are never logged, synchronized, or sent
to the server. The application awaits the full QUIC handshake and authorizes
the peer returned by `Connection::remote_id()`; it never uses 0-RTT for
enrollment, administration, commands, or gameplay.

An account record contains a durable `AccountId`, display name, role, status,
owned character IDs, and ordered endpoint bindings with active, pending, and
revoked states. An enabled account has at least one active `EndpointId`;
initial-enrollment and recovery-locked accounts have none and cannot play.
Each `EndpointId` is permanently bound to at most one `AccountId` in a world,
including pending and revoked bindings. A database uniqueness constraint and
one transaction for lookup plus mutation reject duplicates and enrollment races;
a revoked ID cannot be rebound to the same or another account.
Display names are labels rather than login identifiers. The server binds a
connection to its authenticated `EndpointId` for its entire lifetime. It rejects
any identity that is neither active nor the exact unexpired pending identity on
the enrollment ALPN, and rejects every disabled, banned, or revoked identity,
immediately after the handshake. There are no passwords, password hashes,
application session or resume tokens, invitation secrets, OAuth identities,
application certificates, or separate login/recovery service.

The audited local administrator CLI creates the initial account and a ten-minute
pending enrollment for an exact `EndpointId`. An authenticated account may add
another exact pending ID or revoke any but its last active ID. A pending client
connects using `cdda-rust/enroll/1`; the server activates it atomically only when
`Connection::remote_id()` exactly matches the unexpired pending ID, then consumes
the pending record. The enrollment stream performs no password, shared-secret,
or bearer-token exchange. If all keys are lost, the local CLI atomically revokes
the old bindings, recovery-locks the account, and creates an exact pending
replacement; successful iroh proof activates the replacement and unlocks it.
Expiry leaves the account recovery-locked until the local CLI creates a new
pending replacement; it never restores an old binding automatically.
Pending enrollment creation, expiration, consumption, recovery, revocation,
role, status, and ownership changes are durable recovery inputs; applicable
changes disconnect affected live connections at the next tick.

Roles are player, moderator, and administrator. An account may own multiple
characters, but exactly one gameplay connection per account and per character
is active. An authorized reconnect may replace a stale connection; only an
administrator may transfer ownership or live control. No application bearer
token or transferable connection credential exists.

Authorization is default-deny and uses this fixed capability matrix:

| Role | Allowed capabilities |
| --- | --- |
| Player | Manage its authorized endpoint set and characters; issue gameplay commands only for its currently controlled character; read state permitted by ownership, perception, and ordinary UI rules; use chat and submit reports |
| Moderator | Every player capability for its own character; view account ID/display name, character name, connection state, chat, reports, and moderation history; mute, kick, and suspend another player-role account for at most 24 hours |
| Administrator | Every moderator capability; create/disable accounts; enroll/revoke endpoint identities; grant/revoke roles; permanently ban; transfer character ownership/control; inspect canonical and private state; issue recorded debug/world mutations; enter maintenance; shut down; configure, migrate, back up, and restore the world |

Moderators may never target themselves or an equal/higher-role account. They may
not view private inventories or unseen world state; transfer/control characters;
change accounts, endpoint sets, or roles;
permanently ban; mutate gameplay; pause or configure the world; or run backup,
restore, migration, or shutdown operations. Administrator maintenance, restore,
role, endpoint-replacement, ownership-transfer, and debug-mutation commands
require a newly established, fully handshaken `cdda-rust/admin/1` connection
whose iroh-authenticated identity has held the administrator role for its entire
lifetime and whose handshake completed within the preceding five minutes. This
is only a connection-freshness check: it uses the same iroh identity, adds no
authentication factor, and does not mitigate compromise of that endpoint key.

Every moderation, enrollment, key, and administration attempt records actor
account, authenticated endpoint, role, target, tick, typed safe metadata, status,
and resulting recovery input in the audit journal. Audit records use per-command
allowlists rather than serializing raw arguments or results and never contain
endpoint secret keys or arbitrary command data. The local privileged CLI is a
separately audited local administrator surface and never grants a network client
privileges. Tests enumerate every command, actor-role, and target-role
combination; cross-account ownership case; duplicate and racing enrollment;
unknown/revoked endpoint; stale admin connection; key rotation/recovery; and
default-deny unknown command.

The application maps explicit project messages onto iroh QUIC primitives:

- One long-lived bidirectional control stream carries negotiation, endpoint
  authorization, character selection, connection lifecycle, chat, semantic
  commands, and typed command results.
- One server-opened long-lived unidirectional event stream carries reliable
  ordered entity lifecycle changes and critical domain events.
- Each content manifest, initial-state bundle, and chunk snapshot uses its own
  short-lived reliable unidirectional stream, avoiding per-stream head-of-line
  blocking while remaining subject to the shared connection budgets below.
- QUIC datagrams carry sequence-numbered held movement, vehicle controls, and
  actor/vehicle state deltas. Datagram payloads are self-contained and bounded;
  receivers discard stale sequences, and state too large for one datagram is
  sent as a reliable snapshot stream rather than application-fragmented.

Every payload uses fixed-width domain numbers, typed stable IDs, an explicit
protocol version, bounded collection lengths, and server-side authorization.
Reliable frames use a 32-bit big-endian length prefix followed by Postcard;
every message kind has a documented maximum decoded size. The server is always
the gameplay authority: iroh's peer-to-peer transport never permits direct
client-to-client gameplay state or trust. Protocol version 1 requires QUIC
datagram support with a maximum size of at least 1,024 bytes; incompatible
connections are rejected during the control handshake. Before every datagram
send, the sender queries the current `max_datagram_size()`, caps its payload to
the smaller of that value and 1,200 bytes, and handles the send result. A full
send buffer drops superseded real-time state; loss of datagram support or a
current maximum below 1,024 bytes closes the connection as transport-
incompatible. Other state too large for the current path moves to a reliable
snapshot stream.

Network resource limits are protocol requirements, not operator tuning. One
server accepts at most 64 established QUIC connections, of which at most 32 may
be pending authorization, 16 authorized gameplay connections, and 16 authorized
non-gameplay control or administration connections. Admission uses a global token
bucket of 60 new connections per minute with burst 20 and a per-`EndpointId`
bucket of six per minute with burst three. Rate-limit key tables expire idle
entries after 30 minutes and hold at most 4,096 entries; overflow is governed by
the global buckets without allocating a new key. A connection must complete
protocol/content negotiation and endpoint authorization within 15 seconds of
QUIC establishment.

A client opens exactly one bidirectional control stream and no unidirectional
streams on each gameplay, enrollment, or administration connection; any
additional client-opened stream closes the connection as a protocol violation.
The framing, timeout, memory, and ingress limits apply to all three ALPNs.
Control frames are limited to 64 KiB encoded and 256 KiB
decoded, event frames to 256 KiB encoded and 1 MiB decoded, and bulk snapshot
frames to 8 MiB encoded and 32 MiB decoded. Length and decompression limits are
checked before allocation. No more than four bulk streams or 16 MiB of encoded
bulk data may be in flight per client. Bulk output uses a per-client 512 KiB/s
token bucket with a 2 MiB burst and a server-wide 1.5 MiB/s bucket with a 3 MiB
burst. On the minimum 20 Mbit/s server link, the global bulk rate leaves more
than 7 Mbit/s for control, events, and real-time deltas. A global output
scheduler drains control and critical events across every connection before
servicing bulk transfers with weighted-fair queuing; within each connection it
also assigns those streams higher QUIC priority. Independent streams and
connections still share host, path, and relay congestion. Each frame must make
progress within five seconds. Authorized clients send a heartbeat every five
seconds and are disconnected after 15 seconds without inbound traffic. Control
ingress is limited to 40 messages per second with burst 80 and datagram ingress
to 60 per second with burst 120; excess traffic is rejected or dropped according
to message reliability without entering the simulation queue.

## Deployment and content handshake

Each persistent world has one authoritative server runtime and one SQLite
database. Worlds are not sharded or federated. A dedicated operator exposes one
iroh endpoint. Accounts and roles are local to that server world.

The initial release constructs both client and server endpoints with iroh's
`endpoint::presets::N0` configuration: connections prefer direct QUIC, use its
default address lookup and hole punching, and fall back to an
end-to-end-encrypted N0 relay path when direct connectivity fails. There is no
project-operated central account service, matchmaking service, or public server
directory, and custom relay operation is outside the pinned completion scope.
Players join with an operator-provided serialized `EndpointAddr`. A favorite
persists the pinned `EndpointId` and treats direct/relay addresses as refreshable
hints; a bare hostname or IP without the expected `EndpointId` is not a valid
remote server identity.

On macOS and Linux, local play launches the same standalone Bevy-free server
binary as a separate child process and connects through iroh over the local
direct path. It never links or embeds server crates into the Bevy client
process. Closing the client only disconnects its character; the local server
keeps running until explicitly stopped. An explicit stop first enters the
administrative maintenance procedure, so its planned downtime freezes world
time. A crash, forced termination, or other unexpected process downtime applies
the persisted-UTC deterministic catch-up policy. The initial Windows client can
connect to remote servers but does not host local worlds.

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
2. Endpoint authorization, ownership, and command validation
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

Every 100 milliseconds, one atomic append-only journal batch commits the
authoritative recovery inputs—accepted commands plus administrative,
connection-control, and wall-clock events that affect simulation—and an audit
copy and BLAKE3 hash of the ordered domain events those inputs produced. Each
batch records its first and last `SimTick`, including frames with no player
commands, so AI and environmental work can be regenerated. Durable outcomes are
acknowledged only after that commit. Dirty entity and submap snapshots are
written every five seconds with the last included journal sequence.

Recovery loads the latest mutually consistent snapshots and replays only later
recovery-input records through the simulation. It regenerates domain events and
requires their ordered hash to equal the stored audit hash. Stored output events
are never applied as a second mutation path; a mismatch aborts startup as a
determinism or corruption failure. WAL checkpoints run every 60 seconds and on
graceful shutdown.

Compressed replay archives roll hourly and are retained for 30 days. After a
verified snapshot and replay-archive write, daily compaction removes
recovery-journal rows older than that snapshot; SQLite reuses the freed pages.
Compaction never removes a snapshot object referenced by a retained replay.

The server creates verified online backups hourly, retaining 24 hourly and 30
daily generations. Each backup includes the database, content-manifest hash,
baseline commit, schema version, protocol version, and BLAKE3 checksum. It also
includes a separate protected identity bundle containing the server-world iroh
`SecretKey`; the bundle uses the same OS credential-store or owner-only file/ACL
rules as the live key and is never logged. The public backup manifest records
the expected server `EndpointId` and identity-bundle checksum. Restore verifies
the bundle, derives the key's `EndpointId`, and refuses replacement or startup
unless it matches the manifest, then replays only authoritative recovery-input
records before opening the world. A backup is incomplete if this identity check
cannot pass.

## Determinism and replay

Canonical simulation uses integer or fixed-point arithmetic. Collections whose
iteration affects outcomes are ordered by stable IDs; hash-map iteration never
determines behavior.

Canonical numeric types use exact integer units wherever CDDA exposes one,
including ticks and domain-specific distance, mass, volume, energy, and
temperature newtypes. Remaining fractional simulation values use signed Q32.32
fixed point stored in `i64`. Multiplication and division use checked `i128`
intermediates and round to nearest with ties to even. Conversion, division by
zero, and overflow are checked; wrapping, saturation, platform defaults, and
floating-point fallbacks are forbidden. A command-caused numeric failure is a
typed rejection before assignment. A background-system overflow is a fatal
deterministic invariant failure: the server commits no part of that tick and
enters maintenance recovery from the preceding journal boundary.

Canonical state hash version 1 is a BLAKE3 Merkle root. Leaves are versioned
Postcard DTOs ordered by `(domain_type_tag, stable_id_or_chunk_coordinate)` and
cover global simulation state, content and schema versions, world seed and
namespace, `SimTick`, command/event sequences, ID allocator high-water state,
all persistent domain objects and chunks, and scheduled work. Derived indexes,
caches, network connections, wall-clock timestamps, diagnostics, and client
presentation are excluded. Interior nodes hash ordered child hashes with an
explicit tree-level domain tag. This schema is shared by replay, persistence
verification, and cross-platform conformance tests and changes only with an
explicit version migration.

Each world stores a 256-bit seed. Random streams are ChaCha8 streams derived
with BLAKE3 from the world seed, a domain tag, relevant stable IDs, tick, and
event sequence. Named streams isolate combat, map generation, weather, loot,
and AI so adding a random call in one domain does not perturb another.

The replay format is a versioned Postcard stream compressed with Zstandard. Its
header contains the baseline commit, content-manifest hash, protocol and schema
versions, world namespace, seed, initial snapshot hash, and start tick. Records
contain every authoritative recovery input in journal order, including accepted
commands, administrative and connection-control events, tick spans without
commands, `IdBlockReserved`, `IdBlockAbandoned`, and
`UnexpectedDowntimeCatchUp { previous_utc_anchor, observed_utc, elapsed_ticks }`.
Replay uses the recorded `elapsed_ticks` and allocator operations rather than
consulting a live clock or database. It also stores version-1 BLAKE3 canonical
state roots every 100 ticks. The same replay, including one continued across a
crash and recovery, must produce identical roots on Windows, macOS, and Linux.

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
