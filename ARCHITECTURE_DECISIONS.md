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
code never awaits while holding world state. One dedicated, Bevy-free
persistence thread exclusively owns the live `WorldStore` after startup and
serializes bounded journal, snapshot, ID-allocation, account, authorization,
and character requests. Tokio connection tasks bridge blocking replies through
the blocking pool, so they never execute SQLite work on runtime workers.

Thread-boundary queues are fixed: network-to-simulation ingress holds 4,096
envelopes or 16 MiB; the persistence command queue holds 64 requests and shares
a 32 MiB payload budget with snapshot work; the global simulation-to-network
reliable queue holds 256 batches or 32 MiB; and each client reliable-delivery
queue holds 128 frames or 8 MiB. Unreliable state uses one latest-value slot per
state class rather than a growing queue. A full ingress queue returns
`ServerBusy` for reliable commands and drops unreliable samples. A full client
delivery queue disconnects that slow client, which later reconnects from a
snapshot; it never blocks canonical simulation.

The implemented persistence handoff accounts serialized queued and in-flight
payload bytes before admission. It permits exactly one snapshot write in flight
and one pending immutable snapshot; a newer snapshot atomically replaces the
pending one and completes its receipt as `Superseded`. Journal and critical
requests never enter that replaceable slot. Count or byte saturation fails new
calls with `PersistenceBusy`, every accepted synchronous request has a
five-second reply deadline, and clean shutdown waits for the newest snapshot to
be written before marking the world inactive. Periodic snapshot capture uses an
exact paused tick/journal boundary but resumes simulation immediately after the
bounded handoff.

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

Each canonical tile carries its pinned terrain definition ID and the strict
runtime subset needed to simulate it without a content-library dependency:
integer movement cost, line-of-sight transparency, plus bounded open/close
transform IDs and their movement/transparency results. Transform commands mutate
the chunk only in the authoritative
simulation, increment its revision, and emit a journaled domain event. Additional
terrain behavior joins this canonical tile DTO only when its content field and
simulation semantics are both implemented.

Protocol 11 adds a parallel optional furniture tile to every canonical submap
cell. Its runtime DTO contains the pinned furniture ID, movement-cost modifier,
transparency, comfort, and floor-bedding warmth. A negative furniture movement
modifier blocks the tile; terrain and furniture must both be transparent for
line of sight. Furniture mutations increment the same chunk revision and are
part of snapshots, CanonicalStateV22 hashes, replay, and per-character memory.
The strict selected-content registry finalizes 699 concrete definitions and one
abstract through inheritance/modifiers while retaining every unsupported field.
Protocol 35 applies positive movement modifiers through signed action debt. The
current slice deliberately does not interpret comfort as sleep-quality parity or
implement furniture interaction, storage, destruction, fire, or construction.

Network replication never serializes `WorldSnapshotV1`. A dedicated
`ReplicationSnapshotV1` structurally omits the world namespace/seed, allocator,
event sequence, and peer-private inventory, equipment, needs, and command state.
The first implemented perception policy masks tiles and spatial entities behind
opaque pinned terrain or furniture within a phase-dependent same-z radius: 60
tiles in daylight, eight in civil twilight, and 2/2/3/11/12 across folded moon
phases. It still exposes the opaque destination tile itself. Per-character map knowledge retains
the last perceived terrain and furniture but never dynamic entities. Full
illumination, character senses, vertical vision, and richer concealment remain
explicit follow-on semantics; clients must not infer masked values in their
absence.

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

SQLite schema 11 implements that rule with strict zero-tick boundary batches.
Each batch contains exactly an `IdBlockAbandoned` record followed by its
contiguous `IdBlockReserved` record at the same canonical tick. Startup burns
the prior process's unused reservation before installing a fresh block; paused
checkpoint refills use the same path. These inputs are part of journal hashes,
portable replay, hourly archive ranges, and crash recovery. Durable character
spawn records also store the exact journal sequence after which creation
occurred, so recovery and replay order a spawn on the correct side of a
same-tick allocator boundary.

Domain events are replay-derived records rather than persistent world objects.
Their typed `EventId` uses the same world namespace plus a separate canonical
monotonic event sequence stored in every snapshot and reproduced by journal
replay. Event emission therefore cannot consume or exhaust transactionally
reserved persistent-object counters; no event is acknowledged before the tick
that advances its sequence is durable.

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
pending enrollment for an exact `EndpointId`; local account creation and
lost-key recovery refuse an active runtime marker and require the standalone
server to be stopped. An authenticated account may add
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

The implemented Protocol 35/schema-23 authorization slice keeps Bevy and
transport types out of persistence while using iroh's fully handshaken `remote_id()` as the
actor identity. Initial account creation, endpoint proof, authenticated key
addition/revocation, expected rejection, and local lost-key recovery append a
typed BLAKE3-verified security record. Its allowlisted payload contains only
actor account/endpoint/role when applicable, target public ID, persisted tick,
UTC, action, and result. Security records have an independent monotonic cursor;
hourly replay preparation atomically pins both its journal and security ends,
so a crash retry cannot absorb a later key mutation. Non-curve public-key bytes
are rejected and audited before insertion. The local `account-recover` command
permanently revokes old bindings and recovery-locks the account until the exact
replacement proves its iroh key.

The dedicated admin ALPN authorizes an enabled moderator or administrator and
reauthorizes that same endpoint inside every list or mutation transaction. Both
roles receive bounded 128-record account pages and bounded character lists. A
character-list response joins the durable ownership/name list to the
concurrently locked gameplay-session registry and reveals only whether that
account has a live session and its currently controlled actor. It remains valid
while a session is authenticating but has not selected an actor. It exposes no
endpoint, inventory, need, position, or unseen-world state. Both roles may
kick, mute, or suspend another permitted target for at most 24 hours. Moderators
may target only player-role accounts; administrators may target any other
enabled account. Mute is checked transactionally for every chat submission,
while kick and a newly active suspension publish account-wide invalidation.
Schema 15 persists exact suspension and mute deadlines; reaching the deadline
restores authorization without a cleanup race. Administrators additionally
perform player/moderator/administrator role changes, enabled/disabled/terminal-
banned status transitions, and atomic ownership transfers that reject a same-
name character collision or a 65th character at the destination. The server publishes ownership
invalidation for both old and new accounts after commit.

The admin surface forbids self-targeting and remote changes to initial-enrollment
or recovery-locked states, requires an active endpoint before re-enabling, and
transactionally protects the last available administrator. Connections expire
after five minutes, apply the shared 40/s burst-80 control limit, default-deny
unexpected frames, and subscribe to authorization changes before initial
authorization so an identity cannot retain a stale privileged connection after
demotion and re-promotion. Successful role/status changes publish account-wide
invalidation, while key revocation publishes endpoint-scoped invalidation;
matching gameplay and admin sessions disconnect fail-closed. Invalidation is
published immediately after durable commit and before fallible response I/O.
Open, list, moderation, ownership, mutation, malformed-message, and rate-limit
attempts use typed security-audit records. A player may durably report another
account with a typed reason and at most 1,024 UTF-8 bytes/512 characters of
details, limited transactionally to five reports per rolling hour. Stored
reports preserve reporter and target character names; arbitrary details are not
copied into the typed security audit. Moderators and administrators may page at
most 32 reports, optionally filtered by state, and 128 successful moderation-
history records at a time. An open report may transition exactly once to
actioned or dismissed. That transaction stores its UTC, operator account, and
exact security-audit sequence; repeated or racing resolution is rejected. Each
history record likewise links to the exact successful audit sequence; rejected
attempts remain in that audit instead. Administrators allocate accounts through
a dedicated durable account-ID sequence separate from simulation object blocks,
list any target's bounded endpoint set, stage exact permanently unique pending
identities, and revoke pending or non-last-active bindings. Neither account
creation nor staging bypasses iroh: only a connection whose `remote_id()` equals
the pending identity can activate it on the enrollment ALPN. The offline account
CLI uses the same allocator. The client binary exposes every implemented key and
admin request as a one-shot command authenticated only by its profile's iroh
identity; it shares server pinning with gameplay and prints stable IDs and
pagination cursors suitable for subsequent commands. Graphical operator screens
and world operations remain to be implemented. Protocol 35 provides a distinct
administrator-only private-character request. Persistence transactionally
reauthorizes it and audits only the actor ID plus inventory cursor/limit. The
server then joins that durable identity to an immutable canonical simulation
snapshot. It exposes position, health, needs, sleep, readiness/input, wielded
item, queued actions, and a stable-ID-sorted inventory page of at most eight
items; map memory is represented only by its chunk count. This bound keeps even
the largest valid item page below the 64 KiB control-frame limit. Moderators are
typed-rejected before simulation state is read.

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

The implemented Protocol 35 foundation now opens the ordered event stream after
character selection and publishes actor-relevant domain events only after their
journal batch commits. A bounded broadcast lag disconnects the affected client
instead of dropping a critical event. Authenticated single-line character chat
uses the control stream and a separate bounded server broadcast. Initial and
10 Hz interest snapshots use independent server-opened, Zstandard-compressed
bulk streams; 8 MiB encoded and 32 MiB decoded limits are checked before
allocation/decompression, progress is limited to five seconds, and bulk streams
run below control/event priority. Postcard and Zstandard bulk work runs on the
bounded blocking pool rather than an async network worker. One stream transmits
while a one-slot watch queue retains only the newest replacement. The 60-tile
daylight 11x11-submap DTO intentionally cannot be represented by the 64 KiB
control protocol.

Protocol 35 also implements authenticated held-movement datagrams. The Bevy
client samples key state, sends changes and 100 ms refreshes, queries the current
iroh maximum before every send, and caps at 1,200 bytes. The server requires at
least 1,024-byte support, applies a 60/s burst-120 token bucket, discards stale
sequences, records accepted state in the canonical tick journal, prioritizes
queued semantic commands, and clears abandoned input after a 250 ms lease or
disconnect. Server-to-client actor/vehicle datagram deltas, manifest streams,
global weighted-fair output scheduling/byte buckets and chat audit
remain; their absence is not parity.

Protocol 35 also carries per-tile observations as an explicit current/remembered
sum type. Canonical per-character map memory is sparse and chunked in 12x12
submaps, stores the last perceived terrain and furniture tile, persists in
snapshots, and is regenerated during replay. It never remembers actors,
creatures, or items.
Current LOS refreshes memory for living awake connected and disconnected
characters; an occluded tile retains its old value and sleep creates no new
perception. Replication sends only memory within the
controlled character's current 11x11 interest window, strips the full private
memory collection from the actor DTO, and omits whole-chunk revisions that could
signal unseen terrain changes. Clients may render remembered terrain and
furniture but may interact only with tiles explicitly marked currently visible.
Furniture interaction, vehicles, traps, symbols, rotations, and overmap
knowledge remain later parity.

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

Protocol 10 deterministically derives the visible calendar from `SimTick` using
the pinned default 91-day seasons and generic-scenario start of Spring day 61
at 08:00. Only whole simulation seconds advance the displayed calendar. The
calendar is replicated and validated against its accompanying tick, so clients
cannot receive internally inconsistent time.

The pinned default Boston solar model is precomputed into an audited 364-day
integer table of civil dawn, sunrise, sunset, and civil dusk boundaries. An
independent `cargo xtask astronomy-table-check` regenerates all rows from the
pinned formula within a one-second cross-platform tolerance. Runtime simulation
uses only integer table lookup and integer lunar-phase arithmetic; no floating
point enters canonical outcomes. Protocol 35 replicates phase, eight-step moon
phase, and authoritative sight radius. Day uses pinned `MAX_VIEW_DISTANCE` 60
through the bulk-snapshot path, civil twilight uses eight tiles, and night maps
the pinned moonlight
quarters to radii 2, 2, 3, 11, and 12. The same radius gates dynamic entities,
terrain-memory refresh, and stable-ID targeted shooting. Protocol 37 adds the
exact flashlight as the first local source. Full CDDA perception still requires
general source attenuation, indoor lightmaps, weather multipliers,
transparency attenuation, eye traits/equipment,
scenario-specific starts, and exact continuous brightness remain parity work.

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

Protocol 35 establishes the first crafting activity boundary. Clients request
only a recipe ID. Before simulation admission or journal capture, the server
replaces any client-supplied recipe body with its immutable definition from the
validated pinned-content catalog; an unknown or currently unsupported recipe is
normalized to no definition and rejected. This makes authorization independent
of client data while keeping every accepted journal command and portable replay
self-contained. Replays never consult a mutable or newly installed recipe
registry.

Recipe component groups are conjunctive and their entries are ordered
alternatives. The simulation chooses the first satisfiable alternative, then
the lowest stable item IDs, matching the pinned content order while giving
multiplayer replay an explicit tie-break. Starting a craft atomically removes
whole ingredients, splits partial charge stacks into a new reserved ID while
preserving the carried parent ID, and preallocates every eventual output ID.
Failure to satisfy components, inventory capacity, validation, or ID capacity
changes no item state. Cancellation merges split charges into their exact
parent, restores whole inputs and the previously wielded input, and burns all
temporary/output IDs. Completion consumes reservations and materializes only
the preallocated outputs.

Recipe-level external `using` requirements follow pinned inheritance semantics:
a root field replaces the inherited vector, while `extend.using` appends in
encounter order. Both paths use the same strictly bounded multiplier parser and
recursive component/tool/quality expansion. The pinned case-hardened sheet-
metal recipe consequently requires its inherited blacksmithing support and flux
plus the appended carbon alternatives; the server-normalized recipe, not a
client-supplied expansion, remains authoritative.

Pinned legacy-array and explicit logistic/linear `batch_time_factors` are
strictly parsed, inherited, and retained in content definitions. The current
command surface deliberately crafts one recipe unit. At batch size one, both
pinned formulas return the unmodified single-recipe time, so the factors do not
enter the protocol or canonical activity yet. This is exact single-unit
behavior, not permission to ignore the factors once multi-item batch commands
are added; that future command must normalize its batch count and derived time,
components, tools, byproducts, practice, capacity, and output IDs together.

Pinned legacy-array and explicit-map `book_learn` metadata is strictly parsed,
inherited, and validated against selected BOOK items. BOOK `required_level`
also loads through inheritance. Recipe-dictionary finalization uses a positive
recipe-local threshold directly; otherwise it takes the maximum of the BOOK
required level and recipe difficulty. Protocol 35 stores the resulting bounded,
type-ID-sorted alternatives inside the server-normalized recipe so journals and
portable replays remain self-contained.

At craft start, autolearn knowledge succeeds from theoretical skill, or a
carried identified BOOK succeeds when the recipe's theoretical primary skill
meets that book threshold. The server checks both paths; the client only
predicts them for presentation. The knowledge check does not make the book a
tool or ingredient and does not reserve or consume it. `never_learn` blocks
explicit permanent learning but does not disable autolearn or live book use. The current
item model exposes concrete type IDs without separate identification state, so
all carried types are explicitly treated as identified. Pinned CDDA does not
permanently learn a book recipe merely by reading it.

Protocol 35 adds a distinct server-normalized physical-book study catalog. The
server exposes 197 selected BOOK definitions with a nonempty non-contextual
`read_skill`, positive whole-minute `time`, and `required_level < max_level <=
10`. Clients submit only the stable item and type IDs; before admission and
journaling the server replaces any supplied study body with the pinned catalog
entry. The simulation verifies the exact carried item/type and theoretical
level bounds. Protocol 35 initially adjusted study time under an explicit
self-reader model: intelligence 8, focus 100, no traits,
enchantments, helpers, or rereading/skimming multiplier, and all concrete books
identified. Protocol 37 permits natural daylight or LOS-bounded detail light
from the modeled powered flashlight; general fine-detail lightmaps and
interior/source lighting do not yet exist.

Study is a mutually exclusive canonical activity alongside crafting. It stores
the normalized book definition, stable item ID, remaining action points,
interruption state, and accepted start-command sequence. That sequence plus the
actor/book stable IDs names the server-owned ChaCha8 XP stream. Completion uses
the pinned self-reading minimum/maximum comprehension arithmetic, both skill-
level scaling stages, theory/practice-gap reduction, single-level threshold
reset, and last-practiced update. Reading continues while disconnected and is
interrupted by damage, harmful needs, exhaustion, or loss of detail light. Resume
rechecks the exact book, light, and skill bounds; an interrupted actor may drop
the book but cannot resume without reacquiring it. Snapshots, CanonicalStateV22,
SQLite recovery, portable replay, interest/private replication, events, and the
Bevy menu/HUD all carry the activity. At that milestone, identification
discovery, variable intelligence/focus, morale/fun, helpers, ebooks, recreational books, complete
lightmaps, traits, and enchantments remain explicit future boundaries.

Pinned scalar and explicit skill-list `decomp_learn` metadata is strictly
parsed, inherited, and replaced when explicitly present. The scalar form binds
to the recipe's effective primary skill; explicit lists use last-entry-wins map
assignment as upstream does. Protocol 35 implements a strict disassembly
boundary. The selected loader maintains the separate pinned
`uncraft` inheritance dictionary, finalizes all 1,428 concrete core definitions
plus one abstract, and gives an explicit definition precedence over every
reversible craft with the same target item ID. The server publishes the 1,227
definitions representable by the current runtime. It admits exactly one
non-count-by-charge result with no byproducts,
canonical target damage levels 0 through 4, no charged disassembly tool, and
none of the upstream welder/forge/fire or SEW/GLARE/KNIT substitution rules.
Most admitted targets are non-guns. A gun is admitted only when it has exactly
one pinned ammunition category whose registry default is a concrete,
count-by-charge ammunition item. This currently adds exactly `coilgun`,
`compositebow`, and `compositecrossbow`.
Pinned scalar/list `tool_ammo` is inherited with ITEM collection replacement,
extension, and deletion. A non-gun tool with a charge category and zero default
charges may enter the catalog, but its normalized recipe requires aggregate
charges to be exactly zero. The Bevy client applies the same predicate and the
server replaces the recipe body and rechecks it before any stable-ID allocation.
There are exactly 76 such targets. A charged instance is therefore never
silently destroyed even though pocket, detachable-battery, grid, and UPS state
is not represented yet. The protocol also supports unload-before-reserve for a
non-pocket integral-charge tool: it materializes the registry-default carrier
with the exact charge count and zeros the reserved tool. Synthetic simulation,
recovery, and replay fixtures prove that path, while the pinned corpus currently
contains no qualifying default-charged tool.
For an ordinary item, pinned `get_uncraft_components` supplies the first
component alternative from each recipe group. Default-charged targets,
detachable magazines, weapon mods, batteries and other pocket contents,
containers, liquids, and special substitutions remain unavailable rather than
being approximated.

Reversible non-count-by-charge crafts retain the exact component objects they
actually consume. Nested objects do not retain world IDs, but do retain type,
charges, damage level, melee/comestible/ammunition/ranged properties,
count-by-charge mode, recoverability, and optional child provenance. Protocol,
simulation validation, canonical snapshots, and persistence bound each tree to
256 entries and depth eight. Multi-output recipes use pinned component splitting:
individual objects distribute deterministically and charge stacks divide evenly;
an indivisible set stores no provenance. At disassembly admission, `Some`
provenance replaces catalog defaults, including deliberate `Some([])`, while
`None` preserves the ordinary first-alternative path. Pinned `NO_RECOVER` and
ITEM `UNRECOVERABLE` filter the ordinary recipe-default path. Exact stored
objects follow pinned behavior separately: only the component ITEM's
`UNRECOVERABLE` property prevents materialization; recipe-local `NO_RECOVER`
does not alter an already stored object.

Clients send only the carried stable item and concrete type IDs. Before
simulation admission or journaling, the Bevy-free server replaces every recipe
body from its immutable result-type catalog; an unsupported type becomes no
definition and rejects. Start rechecks authoritative detail light, exact ownership/type, target
condition, and ordered tools/qualities, atomically removes the target, remembers
wield state, and preallocates every possible component ID. For a supported bare
ranged or integral-charge target, it also preallocates one charge-carrier ID,
materializes the pinned registry-default item with the exact internal count,
drops it at the actor's position, zeros the reserved target, and only then announces the
activity start. The target and IDs live in a mutually exclusive canonical
activity; pickup capacity always reserves one slot so cancel can restore the
exact target and wield state. Because the reserved target is already empty,
cancel leaves the single unloaded stack on the ground and cannot duplicate it.
An empty-only tool allocates no carrier ID; a nonzero count rejects before the
target is removed or the allocator advances.
Damage, harmful needs, exhaustion, and darkness interrupt. Resume reselects
current support and rechecks authoritative detail light; disconnect never pauses or protects the
actor.

Completion drops recovered components at the actor's current position. Each
instance is either the catalog default or an exact retained component object;
it applies the pinned `2 + 4 * practical skill`
dice against recipe-difficulty dice with 24 sides under a server-owned ChaCha8
session stream fixed by world seed, actor/target IDs, domain, and accepted start
sequence. A second roll applies the exact pinned `0.8 ^ damage_level` recovery
multiplier through the integer 100%/80%/64%/51.2%/40.96% table; difficulty zero
bypasses only the skill contest, not item damage. A distinct fixed session
stream performs the pinned one-in-four permanent-learning roll only when
the actor meets `decomp_learn`, does not already hold explicit knowledge, and
does not already satisfy autolearn. Learned recipe IDs are bounded, sorted,
canonical character state and authorize the matching normalized craft without a
book. Protocol 35, CanonicalStateV22, schema-23 snapshots/journals, recovery,
portable replay, interest/private replication, events, the operator view, and
the Bevy `N` menu/HUD carry this entire boundary. Clients cannot claim recovery
or learning outcomes. Completion also applies pinned
`practice(primary_skill, difficulty * 2, difficulty)` under Protocol 35's
explicit focus-100, INT-8, PER-8, training-enabled, trait-free character model. Integer
catch-up, cap, one-level reset, theory synchronization, last-practiced state,
events, recovery, and replay are canonical. Variable focus/stats, traits,
enchantments, and training toggles remain later character-model boundaries.

Protocol 35 includes legacy deterministic recipe `byproducts`. Content
normalization sorts these by type ID. A non-count-by-charge byproduct reserves
and materializes its declared number of separate instances; a count-by-charge
byproduct reserves one instance whose initial charges are its pinned item
default multiplied by the declared count. The main result precedes byproducts
in the preallocated stable-ID sequence. The combined result count is bounded to
256 and is used consistently for inventory admission, allocator preflight,
activity validation, snapshots, recovery, completion events, cancellation ID
burning, and portable replay. Random `byproduct_group` evaluation remains
unavailable rather than being collapsed into a deterministic approximation.

Component entries marked `LIST` are references to reusable requirement IDs,
not item groups. Content finalization recursively replaces each reference with
the first component group of that requirement, composing checked integer
multipliers at every level. Expansion preserves encounter order; duplicate
concrete item IDs in one alternative group retain the first entry's metadata
and use the minimum count, matching pinned `inline_requirements`. Unknown,
cyclic, empty, oversized, unsupported, or missing-item expansions make the
recipe unavailable rather than approximating it. The server catalog and replay
payload contain only the resulting concrete alternatives, so this content-only
change requires no new wire or canonical-state schema. Tool `LIST` references
use the same recursion, first-tool-group, scaling, order, and deduplication rules.
After inlining, each concrete tool expands base-first through every transitive
ITEM `sub` descendant in stable type-ID order, exactly where pinned item and
requirement finalization apply subtype replacement. Missing or cyclic subtype
graphs fail closed.

Protocol 35 extends the support-item boundary. Pinned ITEM `qualities`,
`charged_qualities`, and `charges_per_use` load through direct definitions,
inheritance, `extend`, `delete`, and `relative` where the source permits those
forms. All inherent qualities enter the legacy-recipe runtime catalog. A
charged quality enters only when its item has a positive `charges_per_use` that
fits the bounded protocol representation. Each carried provider instance must
individually meet that threshold; charges on multiple instances are never
pooled to qualify one provider, and satisfying a quality does not itself spend
energy. The current item charge field represents immediately available
loaded/linked energy until pockets, batteries, grids, and UPS sources exist.
Pinned CDDA consults a quality's speed only in `steps` recipes, so a non-unit
annotation remains a valid provider here and no floating-point speed enters
canonical outcomes. Step recipes remain unavailable until their per-step
requirements and bounded speed accumulation are implemented together.
Presence tools and positive-count tools backed by aggregate carried charges are
supported; tool definitions with count-by-charge stacking or an explicit
nondefault `charge_factor` remain unavailable. Each normalized quality
alternative carries bounded, sorted provider descriptors containing a type ID
and its minimum charge threshold so journals and portable replays remain
self-contained. Tool and quality groups use pinned
ordered OR selection. Repeated requirements for the same tool type are
aggregated, presence counts choose distinct lowest stable item IDs, and charge
depletion walks matching carried items in stable-ID order. Chosen support items
remain in inventory and are excluded from component selection. Officially
exposed recipes are conservatively rejected when any component type overlaps
any possible support type, avoiding an incomplete approximation of upstream
requirement deduplication.

The server records one selected alternative index per tool group in the craft
activity. It debits the pinned start amount `count / 20 + count % 20`, then
reaches the remaining cumulative total over twenty 5% progress buckets. Every
boundary preflights all required presence and aggregate charges before changing
inventory, progress, or practice. A shortfall therefore freezes the exact prior
state, emits an interruption, and lets the character use ordinary actions at
their normal readiness cost to acquire energy or another tool. Inventory slots
needed by restoration or output stay reserved, and commands cannot mutate a
partial-stack parent needed for exact cancellation. Resume uses the pinned `max(count / 20, 1)`
availability check and may deterministically reselect an ordered alternative.
Spent charges are never refunded by cancel and cannot be charged twice by
snapshot recovery or replay. Zero-cost or externally powered charged qualities,
tool pockets, batteries, power grids, and UPS sources are deliberately outside
this aggregate first slice.

Craft progress is canonical action points: one upstream move is 20 action
points, so a speed-100 actor advances 100 points per 50 ms tick. Disconnect does
not pause or protect the actor. Damage and harmful needs/exhaustion transitions
interrupt without releasing ingredients; progress then remains frozen until an
explicit resume or cancel. The activity, exact recipe, remaining work,
ingredients, split-parent links, output IDs, wield restoration, interruption
state, selected tool alternatives, and exact charge boundary are in snapshots,
CanonicalStateV22, SQLite recovery, replication, private inspection, and replay.

Protocol 35 also establishes canonical skills for crafting. The selected
registry strictly loads all 28 default pinned skill IDs and retains unsupported
fields. Each actor stores a stable-ID-sorted sparse list of practical level/raw
exercise, theoretical level/raw knowledge experience, and last-practiced tick;
an absent skill is exactly level zero. Autolearn requirements use theoretical
levels. Craft eligibility requires the complete primary/secondary set in either
practical or theoretical levels, matching the pinned `has_recipe_requirements`
boundary. Clients predict this only for presentation; the server-normalized
recipe and simulation enforce it again.

Craft progress crosses one practice boundary per nominal 100 recipe moves.
Under the currently fixed default-focus/no-trait model, each boundary grants
100 raw practical experience and keeps theory caught up when both levels are
equal. Training continues while disconnected and already-earned practice is
not rolled back by interruption or cancellation. The pinned ordinary-recipe
cap uses integer-truncated `difficulty * 1.25`; crafting at the cap may raise a
skill once beyond it, after which that recipe stops training. Practice counters
and skill state are validated in snapshots, recovery, replication, private
inspection, and replay. The runtime still excludes tool pockets,
batteries, power grids/UPS, explicit nondefault tool charge factors, zero-cost or
externally powered charged qualities and step-recipe quality speed.

Protocol 35 establishes the first canonical proficiency boundary. The selected
registry loads stable IDs, fixed-point default time and skill modifiers,
learnability, training time, and prerequisite lists. Recipe-local entries may
override those modifiers or require a proficiency. Required entries are hard
craft gates and never train during that craft. Every unknown optional entry
contributes its pinned time penalty; products and per-tick progress use checked
integer millionths with a canonical fractional remainder. The upstream
cosine-based reduction as practice approaches completion is represented by a
committed 101-sample integer lookup table with deterministic linear
interpolation, keeping floating point out of runtime state and replay.

Optional proficiency practice is awarded only when craft progress crosses one
of twenty 5% boundaries. It mirrors the pinned whole-second per-boundary
quantization, divides time across eligible unknown proficiencies, applies the
recipe learning multiplier and raw time multiplier, honors direct prerequisites
and per-recipe experience caps, and emits a learned event at the exact threshold.
No progress or practice commits when charged tools fail their boundary debit.
Practice continues while disconnected and is not rolled back by interruption or
cancel. Sorted sparse practice action points, fractional remainders, learned
flags, recipe definitions, and awarded-boundary counters are validated in
Protocol 35, CanonicalStateV22, snapshots, recovery, replication, private
inspection, and replay. Skill penalties are retained in normalized recipes but
deliberately do not affect outcomes before stochastic crafting failure exists.

`ALLOW_ROTTEN` is an admitted recipe flag under an explicit domain invariant:
canonical item instances currently have no rot/spoilage state, so every
representable component passes both the flagged filter and the ordinary
non-rotten filter. No wire or activity flag is needed while that invariant
holds. Adding rot must first introduce canonical item rot state, catch-up,
component filtering, persistence, replication, and replay, then preserve this
flag as the server-authoritative exception. `BLIND_HARD`, `FULL_MAGAZINE`,
`NO_RESIZE`, and other flags remain unavailable because their differences are
representable only after missing light, magazine, or per-instance fit behavior
exists.

The runtime still excludes tool pockets, batteries, power grids/UPS,
explicit nondefault tool charge factors, zero-cost or externally powered charged
qualities, step-recipe quality speed, workspaces/local light, recipe flags other
than `BLIND_EASY` and `ALLOW_ROTTEN`, non-default focus, reading helpers,
ebooks, recreational books and book identification, skill rust, multi-item batch commands, containers, randomized
byproduct groups, stochastic failure and permanent recipe learning beyond
autolearn/live books, disassembly, and construction. Their parsed definitions remain
unavailable rather than silently approximated.

Protocol 10 adds the first canonical sleep slice. Sleepiness is a signed integer
with pinned thresholds `TIRED=191`, `DEAD_TIRED=383`, `EXHAUSTED=575`, and
`MASSIVE_SLEEPINESS=1000`; an awake actor gains one point at every five-minute
needs boundary. A voluntary sleep command requires the tired threshold. At
1,000 the actor loses ten points and falls asleep from exhaustion. Sleeping
actors execute no queued actions, gain no action readiness, expose only
remembered terrain, perceive no dynamic entities, and remain physically
targetable. A wake command is processed without waiting for readiness; damage
also wakes a surviving sleeper. Natural rest ends at sleepiness -20.

Sleep recovery uses pinned default rate one plus a deterministic replay-safe
accelerated-recovery roll that ramps through the upstream effect's 24 intensity
steps. Sleeping halves food and water rates with integer alternating intervals.
Sleepiness, sleeping state, recovery intensity, commands, reasons, needs events,
snapshots, CanonicalStateV22 hashes, SQLite journals, recovery, and portable
replays are authoritative. This first slice intentionally omits comfort,
temperature, traits, stimulants, sleep deprivation, alarms, microsleeps below
the forced threshold, healing bonuses, and the full interruption matrix.

Only an administrator may enter maintenance pause. Maintenance pause first
stops command intake, checkpoints the database, disconnects clients, and then
freezes the simulation clock. Planned maintenance downtime does not advance
world time. Unexpected process downtime does advance world time: on restart the
server uses the persisted UTC anchor to perform deterministic analytical
catch-up before accepting connections.

The first implemented crash-time mechanism stores an active/clean runtime marker
and UTC anchor in SQLite schema 5. Each 100 ms journal commit advances the anchor
in the same transaction. An unclean restart deterministically advances and
journals 20 commandless ticks per elapsed whole second before opening iroh;
clean shutdown marks the runtime inactive after its final checkpoint. This is
exact for the current simulation but intentionally temporary for long outages:
warm 1 Hz and dormant analytical spans must replace per-tick expansion as their
subsystems land.

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

The first implemented firearm slice deliberately stays inside the canonical
command/event model. A wielded gun snapshot carries a strict ammunition type,
capacity, remaining rounds, range, scalar damage, and dispersion derived from
the pinned gun and ammunition definitions. The server validates target life,
range, terrain line of sight, ammunition, and compatible carried reload stacks;
it alone advances the named deterministic RNG stream, spends rounds, applies
damage/death, and emits typed shoot/reload outcomes. This is a vertical proof,
not gun parity: magazines and pockets, aiming/recoil, skills, armor/anatomy,
projectile travel, intervening creatures, sound, attachments, and nonlinear
barrel damage remain explicit gates.

An actor has one active action and at most two queued semantic commands.
Canceling a queued command is free. An active action may be canceled only when
its activity definition marks it interruptible; elapsed time and already-paid
costs are retained. Invalidated queued commands are rejected with a typed reason
and removed. Held movement and vehicle steering are stateful inputs rather than
queued semantic commands.

The implemented discrete-action foundation stores integer speed, signed action
readiness/debt, and two buffered semantic commands in canonical actor state.
Each 20 Hz tick adds speed up to a 2,000-point one-action cap; speed 100 therefore
executes an ordinary 100-move action once per second, matching hostile creature
scheduling. Protocol 35 charges horizontal movement by the pinned
`(source tile cost + destination tile cost) * axis multiplier / 2` rule, using
50 for cardinal and 71 for diagonal steps, then scaling by 20 for the 20 Hz
accumulator. Tile cost is terrain movement plus a nonnegative furniture
modifier. Movement happens at readiness, then any cost above the bank becomes
signed canonical debt; this preserves an immediately responsive banked first
step while delaying the next action. Admission and execution are separate
replayable phases, full queues reject with a typed result, and actor IDs order
same-tick completions. Held movement is a separately sequenced canonical state,
not a queued semantic command; authenticated datagram changes/refreshes are
journaled, semantic commands win a ready slot, and release, disconnect, or a
250 ms lease clears it deterministically. Vertical steps, character move-mode/
stamina/encumbrance/trait modifiers, fields, vehicles, long-running active
actions, and cancelation remain to be layered onto this foundation.

The client predicts only its controlled actor's locomotion, vehicle steering,
camera, and cosmetic effects. Inventory, combat, projectiles, RNG outcomes,
item use, crafting, dialogue, and world interactions are never predicted.
Remote actors render with a 100-millisecond interpolation delay. Reconciliation
uses authoritative tick-tagged snapshots: corrections within one tile are
smoothed over 150 milliseconds, while larger or collision-invalid divergence
snaps immediately and records a diagnostic event.

## Persistence, recovery, and backups

Each world uses one SQLite database with `journal_mode=WAL`,
`synchronous=FULL`, foreign keys enabled, and a single persistence writer.
Online backup is the sole additional live-database connection: the dedicated
backup thread opens it read-only and uses SQLite's stepped online-backup API, so
journal commits and durable acknowledgements continue between copy steps.
Numbered forward-only SQL migrations run transactionally after an automatic
backup. Downgrades are unsupported.

Every 100 milliseconds, one atomic append-only journal batch commits the
authoritative recovery inputs—accepted commands plus administrative,
authorization/session-control, and wall-clock events that affect simulation—and an audit
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

Socket presence is ephemeral session metadata, not canonical actor state. It is
excluded from canonical hashes, may be audited for operations, and is reset to
offline during recovery. Disconnecting never removes, pauses, protects, or
otherwise changes the persistent actor; all world-affecting commands remain
canonical recovery inputs.

Compressed replay archives roll hourly and are retained for 30 days. After a
verified snapshot and replay-archive write, daily compaction removes
recovery-journal rows older than that snapshot; SQLite reuses the freed pages.
Compaction never removes a snapshot object referenced by a retained replay.

The implemented schema-23 archive scheduler stores an exact durable
sequence/UTC cursor, captures a matching end snapshot, and persists that end as
the sole pending range before extracting the closed journal range on the SQLite
owner. One dedicated bounded worker verifies the renderer-free replay and final
CanonicalStateV22 root, encodes Postcard, compresses with Zstandard, and
atomically publishes an owner-only file after file and directory sync. Only
then may a compare-and-swap advance the database cursor and clear the pending
range. A crash or failure after preparation therefore retries the identical
endpoint even if the live world has advanced. Deterministic retries accept only
byte-identical existing archives, one job may exist at a time, and
filename-scoped retention deletes archives older than 30 days without touching
unrelated files. Before publishing each archive, the worker publishes and
re-reads a bounded owner-only `SnapshotObjectV1` whose Postcard bytes determine
its BLAKE3 filename and whose hash is bound into replay format 3. After archive
retention runs, reference collection fully decodes and renderer-independently
verifies every retained recognized replay before it verifies every referenced
object. Only then does it remove exact-name unreferenced snapshot objects and
sync their directory. A malformed retained replay, missing or corrupt referenced
object, unsafe file type, oversized file, content mismatch, or unsafe Unix mode
fails closed before deletion; unrelated filenames remain untouched. The current
online and pre-migration backup formats embed their database state and contain
no external snapshot-object references, so their reference set is empty. A
future backup format must add its references to this proof before publication.
Once per day, and only with no pending archive, SQLite atomically deletes journal
batches at or before the committed archive cursor and snapshots before its
retained anchor; tests re-run canonical recovery after that compaction.

The server creates a verified online backup at first startup opportunity and
hourly thereafter, retaining 24 newest hourly and 30 older daily generations.
The dedicated backup worker opens a read-only source connection and copies 256
pages per step with a two-millisecond yield; it never occupies the persistence
request queue or writer connection. The resulting point-in-time copy is switched
out of WAL mode and must be self-contained without sidecars.
Each backup includes the database, content-manifest hash,
baseline commit, schema version, protocol version, and BLAKE3 checksum. It also
includes a separate protected identity bundle containing the server-world iroh
`SecretKey`; the bundle uses the same OS credential-store or owner-only file/ACL
rules as the live key and is never logged. The public backup manifest records
the expected server `EndpointId` and identity-bundle checksum. Restore verifies
the bundle, derives the key's `EndpointId`, and refuses replacement or startup
unless it matches the manifest, then replays only authoritative recovery-input
records before opening the world. The implemented SQLite online copy is reopened
read-only, integrity/foreign-key checked, replayed headlessly, and compared with
its manifest state root before its private generation directory is atomically
published. The protected key is a distinct owner-only member; its checksum is
manifested but never logged. Startup recognizes only generations whose name,
content, endpoint, checksums, schema, and canonical database root all verify.
`cdda-server --restore` repeats those checks, copies into a private temporary
directory, refuses an existing destination, and renames it atomically. First
startup fully re-verifies the untouched copy, then atomically converts its
manifest to durable restore provenance; every later startup rejects content or
key material whose derived endpoint/checksum disagrees with that provenance. A
backup is incomplete if this identity check cannot pass. Before any migration,
the SQLite owner now publishes a separate owner-only atomic generation. It
integrity/foreign-key checks the source-schema database copy, converts the copy
out of WAL mode so no sidecar is required, includes the exact sibling protected
server identity when present, and binds the exact file set, lengths, and BLAKE3
checksums in a bounded Postcard manifest. Migration aborts if generation
publication or re-verification fails. Injected process-crash coverage remains
required.

## Determinism and replay

Hostile creature scheduling maps CDDA speed 100 to one action per real-time
second: each 20 Hz simulation tick adds the creature's integer speed to an
accumulator, and an action costs 2,000 points. Stable creature ID orders
simultaneous turns. The current deterministic pursuit chooses the nearest
living same-z actor by Manhattan distance and then stable actor ID; connection
presence is deliberately ignored, so disconnected characters remain valid
targets. Full CDDA senses, pathfinding, special attacks, and disposition rules
will replace the corresponding partial policies without changing authoritative
server ownership or the accumulator's integer/replay guarantees.

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
it and is deleted only after the complete retained replay reference set verifies
and contains no reference to it. Current backup formats carry no external object
references. Replay export produces a self-contained bundle containing the replay
stream, its snapshot object, and the matching content manifest; replay import
verifies every hash before execution.

The implemented replay bundle is `ReplayBundleV1` at format version 3: a bounded Postcard
value compressed with Zstandard. It embeds the exact initial canonical snapshot,
character-spawn recovery inputs, subsequent ordered journal batches, an exact
typed security-audit range, baseline/protocol/content identity, and expected
final state root. The local
`replay-export` and `replay-verify` tools validate the installed pinned content
before deterministic headless execution. Hourly rolling/30-day retention and
allocator boundary inputs are implemented. Spawn records carry their journal
boundary and hourly ranges durably pin exact pending journal and security ends
across process restart. Each range binds and publishes a verified content-addressed immutable
snapshot object before its archive, and daily recovery-history compaction keeps
the cursor anchor and all newer state. Older on-disk schemas receive a private,
integrity/foreign-key-verified, exclusively published SQLite backup before the
first migration transaction. Retained replay archives now form a fully verified,
fail-closed snapshot-object reference set used for safe collection. Remaining
administration inputs and broader fault injection remain required before this
mechanism is release-complete.

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

## First canonical detachable-battery boundary

Protocol 36, schema 24, and CanonicalStateV23 establish the first deliberately
narrow pocket-derived runtime model. The content loader implements inherited
static `capacity` and derives strict MAGAZINE/MAGAZINE_WELL projections from
inherited `pocket_data`; the complete pocket field remains unsupported. The
pinned audit identifies 160 non-gun tools with one battery well, but runtime
admission is initially limited to the exact `flashlight` and
`medium_battery_cell` pair.

A compatible magazine is a stable item even at zero charges. A tool with a
modeled well has zero parent charges and owns at most one installed magazine;
that nested item keeps its original ID, type, capacity, and remaining charges.
Reload is an authoritative atomic swap: the selected carried cell moves into
the wielded tool, and any prior cell returns intact to inventory. Incompatible
cells reject without mutation. Charged crafting and charged-quality checks read
and debit installed energy. A loaded tool cannot be consumed as a crafting
component while general nested component containment is unavailable.

Disassembly detaches an installed cell unchanged at the actor's position before
reserving the empty tool. Cancel therefore restores an empty tool and cannot
duplicate the already-dropped cell. Snapshot, replication, private inspection,
SQLite recovery, and replay validation include nested stable IDs in the global
uniqueness and namespace checks. Empty magazines remain valid; over-capacity,
nested-well, incompatible, duplicate-ID, and hidden-parent-charge shapes fail
closed. The fresh cabin provides an empty flashlight and a full 56-charge
medium battery. Seventy-five other powered disassembly targets retain the exact
empty-only fallback until their storage is modeled, and all other magazines,
pockets, batteries, mods, UPS/grid behavior, and recharge semantics remain
unsupported.

## First powered transformation and local-light boundary

Protocol 37, schema 25, and CanonicalStateV24 extend the exact detachable
flashlight pair without generalizing unsupported item actions. The content
loader strictly projects inherited integer `power_draw` quantities into
milliwatts, plus inherited `light`, `revert_to`, and only `use_action` entries
whose type is `transform`. Server startup pins the off action to
`flashlight_on` with one required charge and the on action back to `flashlight`
with zero charge scale; it also pins the active definition to 1,560 mW and
light 300, and both actions to their upstream zero move cost. Any drift fails
catalog construction.

The client sends `Activate { item_id }`; ownership, stable-ID namespace,
modeled configuration, and available whole activation charges are authoritative
server checks. Activation spends one whole nested-cell charge and changes the
canonical item type/state. Once per whole simulation second, active carried and
ground tools consume power in exact millijoules. A charge represents 1,000,000
mJ; sub-charge residual energy lives on the stable magazine item, never on its
parent. Drain continues for disconnected carriers and with zero connected
players. If less than a full second's draw remains, the tool atomically reverts
off and emits a typed depletion event without destroying the residual. Reload
of an active tool rejects. Protocol validation, global nested-ID rules,
snapshots, SQLite recovery, private/interest replication, and portable replay
all cover the powered configuration and residual energy.

An active flashlight is the first dynamic light source. A final exactly funded
second may leave zero stored energy while the transform remains active until
the next whole-second processing boundary, but pinned `getlight_emit` returns
zero immediately and the item emits no free light during that interval.
Its carrier position, or its ground position after dropping, illuminates tiles
within the current 60-tile maximum only when source-to-tile LOS is clear. An
observer must still have observer-to-target LOS and be within that same maximum;
natural sight continues to cover its phase-specific radius. The server derives
sources from canonical private inventories without replicating another
character's inventory. This light refreshes terrain memory, permits ranged
targets and fine-detail reading/disassembly, and supplies a single replicated
`detail_vision_available` decision for client menus.

This is intentionally not a complete CDDA lightmap. Light value 300 maps to the
current constant 60-tile LOS boundary; `CHARGEDIM` attenuation, directional and
full-spectrum sources, overlapping intensity, transparency attenuation,
weather, interiors, eye/gear modifiers, other transforms and use actions,
recharge, UPS, grids, and other powered items remain unavailable and must not be
reported as supported.

## Strict attenuation-aware detachable-battery light family

Protocol 38 widens the first light only where the already canonical mechanics
are complete. Server startup derives a pair from finalized content only when
both definitions are non-gun TOOLs with identical single detachable battery
wells; exactly one zero-move transform in each direction; no companion
non-transform use action; one positive activation charge at scale one; a
zero-scale deactivation; an exact inactive `revert_to`/draw/light zero state;
and a positive active draw, `revert_to`, and light. Every compatible MAGAZINE
must have one exact positive integral `battery` restriction whose capacity
matches its finalized capacity. The default magazine must be in that compatible
set. Anything else remains unmodeled rather than losing an action or pocket
behavior silently.

The current pinned corpus yields exactly nine pairs (18 definitions):
`flashlight`, `diving_flashlight_small`,
`diving_flashlight_small_hipower`, `mipim`, `mounted_flashlight`,
`wearable_big_light`, `wearable_light`, `wizard_cane`, and
`wizard_cane_cheap`, each with its exact `_on` target. The compatible runtime cells are the selected ultralight, light,
rechargeable-light, or medium cells named by their wells. Eighteen exact
battery-shaped MAGAZINE definitions may retain bounded integral energy storage,
but a tool accepts only the stable type IDs resolved from its own well. Crafted
`wearable_light` retains this runtime pair and well, and the reversible
`flashlight`, `wearable_light`, and `wearable_big_light` disassembly paths first
detach their exact installed cell. This lowers the explicit empty-only guard
from 75 to 73 targets without weakening it for any other tool.

Runtime attenuation never evaluates the pinned floating-point logarithm.
Audited integer tables cover ordinary open-air visibility through luminance 35
and external fine-detail range through luminance 70; brighter sources saturate
the 60-tile maximum. Luminance-four `wizard_cane_cheap` therefore reaches
exactly three open-air tiles. CDDA's personal-light bonus makes it sufficient
for its carrier's detail work, while the same dropped or peer-carried source is
below the external detail threshold. Source-to-target and observer-to-target
LOS still apply, and same-position sources merge only by maximum derived
radius. At the Protocol 38 boundary, `CHARGEDIM`, smoke/graded transparency,
directional/colored light,
linked power, recharge, additive overlapping intensity, and all other
action/pocket semantics remain explicit gaps.

## Exact charge-dependent light dimming

Protocol 39, schema 26, and CanonicalStateV25 persist a content-derived
`dims_with_charge` bit on powered state. Startup pins the finalized active flags:
all nine admitted pairs have `CHARGEDIM`: `flashlight`, both simple diving
flashlight pairs, `mipim`, `mounted_flashlight`, `wearable_light`,
`wearable_big_light`, and both wizard canes. Drift in that exact nine-pair set
fails the content-bound catalog test.

For a dimming source at or above one fifth of installed capacity, effective
emission equals the finalized base. Below that threshold it is
`floor(base * stored_energy * 5 / capacity_energy)` using checked `u128`
intermediates. Stored energy includes whole charges and canonical residual
millijoules, a deterministic precision-preserving adaptation of upstream's
integer remaining-charge query. A non-dimming source keeps full output while
any energy remains. Zero energy always emits zero, even during the interval in
which the active type has not yet reverted. The effective value, rather than
the base, feeds ordinary radius, personal-detail eligibility, and external
detail radius in both simulation memory/activities and privacy-filtered server
replication. Recharge and non-battery power remain unavailable.

## First strict furniture-construction boundary

Protocol 40, schema 27, and CanonicalStateV26 made construction a persistent
server-owned activity without introducing ECS into the server. The selected
content loader retains all 776 construction definitions and 438 construction
groups. A definition enters the initial authoritative catalog only when its
complete behavior is an item-component placement of furniture: category
`FURN`, activity `LIGHT_EXERCISE`, an empty `pre_terrain`, exactly
`check_empty`, no unsupported or reusable-requirement component semantics, and
a finalized furniture result. This produces an exact pinned catalog of 17
definitions, including `constr_place_table`.

The Bevy client uses local pinned content only to present eligible choices and
visible empty adjacent targets. Its command contains a construction ID and
absolute target but no trusted definition. The Bevy-free server replaces any
supplied definition from its catalog before the command crosses the simulation
and journal boundary; unknown IDs normalize to no definition and reject.

Start requires horizontal adjacency, practical skill levels, detail light, an
unchanged empty target, and an atomically satisfiable component plan. Stable-ID
ordered exact components move into the canonical activity; partial charge
stacks receive a new reserved stable ID while their parent remains carried.
The real-time action continues for disconnected actors, who remain physically
present and vulnerable. Damage, harmful needs, exhaustion, darkness, or target
mutation interrupt work. Resume rechecks light and target. Cancel restores the
exact reserved items, charges, and eligible prior wield state; completion
consumes them and changes the chunk furniture. In-progress state participates
in global stable-ID uniqueness, replication, private inspection, snapshot
validation, SQLite recovery, canonical hashes, and portable replay.

Tools, qualities, reusable requirements, other pre/post specials, terrain and
multi-stage result chains, byproducts, deconstruction, work sites/helpers, and
all remaining definitions stay unavailable rather than being approximated.

Protocol 41, schema 28, and CanonicalStateV27 close the pinned `check_empty`
terrain predicate exactly for the currently modeled world. Canonical terrain
tiles now preserve the finalized `FLAT` flag for their current, open, and close
forms. Construction requires that flag in addition to the already modeled lack
of furniture, creatures, actors, and ground items; traps and vehicles are also
absent because neither system exists canonically yet. Adjacency is checked
before any target-bubble generation, preventing a valid construction ID plus a
forged remote coordinate from allocating world chunks.

## Exact-prerequisite terrain-construction boundary

Protocol 46 widens construction without changing schema 29 or
CanonicalStateV27. A definition may now enter the authoritative catalog when
it has either the already modeled empty-tile `check_empty` predicate or a
nonempty list of resolved exact terrain/furniture prerequisite IDs with no
special predicate. Result IDs may resolve to either modeled terrain or modeled
furniture. Every other strict condition remains: explicit
`LIGHT_EXERCISE`, nonzero duration, ordinary non-reusable item components,
bounded modeled skills, a supported group, and no unsupported fields.

This admits exactly 20 pinned colored-carpet transformations in addition to
the original 17 furniture placements. These definitions are a deliberately
closed slice: one exact floor-terrain prerequisite, one terrain result, and no
tools, qualities, flags, requirement lists, byproducts, or special completion
behavior. Prerequisite IDs are sorted and deduplicated in the normalized
server-owned recipe before journaling. Client commands still contain only the
stable construction ID and target.

The Bevy client mirrors the strict catalog for presentation and filters visible
adjacent targets against either `check_empty` or the current exact terrain or
furniture identity. The Bevy-free server independently evaluates the same
predicate before start, resume, and completion. Terrain and furniture remain
independent chunk layers: a terrain result performs the equivalent of upstream
`ter_set` and preserves furniture, while a furniture result preserves terrain.
Simulation, snapshot, SQLite recovery, and portable replay tests lock the exact
terrain outcome. Broader chains remain unavailable until their additional
tools, qualities, flags, specials, item displacement, and completion effects
are represented rather than approximated.

## Non-consuming construction-quality boundary

Protocol 47, schema 30, and CanonicalStateV28 make item qualities part of the
immutable construction recipe and activity. The content loader now types the
same quality groups already understood by crafting. The server resolves each
quality to a stable type-ID-sorted list of pinned inherent or charged providers,
including the minimum per-provider charge threshold, and rejects definitions
whose complete provider set cannot be represented within protocol bounds.
Clients never supply trusted provider semantics.

This admits exactly one additional definition:
`constr_brick_oven_finisher`, requiring AXE 2 and CHISEL_WOOD 1 while consuming
one log and transforming `t_brick_oven_struct` into `t_brick_oven`. Sixteen
otherwise nearby carpet definitions remain excluded because their nail input
uses reusable `LIST` requirement semantics, which this boundary intentionally
does not approximate. The authoritative catalog therefore grows from 37 to 38.

Quality selection uses stable inventory and provider order. Selected provider
items are protected before component planning, remain carried rather than
reserved or consumed, and are rechecked on every active tick, resume, and
completion. Missing support rejects start/resume with `MissingQualities` or
interrupts active work with
`ConstructionInterruptionReason::MissingQualities`. Reservation reconstruction
protects every possible quality-provider type so a forged component/provider
overlap fails closed while a legitimately missing provider can remain a valid
interrupted activity. Protocol bounds, snapshot validation, SQLite recovery,
and portable replay cover the normalized provider requirements.

## Construction component-requirement expansion

Protocol 48 widens catalog normalization without changing schema 30 or
CanonicalStateV28. Construction component entries marked `LIST` are references
into the already pinned recipe requirement dictionary, not literal item IDs.
The shared resolver recursively inlines the referenced requirement's first
component group, multiplies counts at each edge, preserves upstream alternative
order and recoverability, minimizes duplicate alternative counts, and rejects
missing references, cycles, empty groups, overflow, or unresolved nested
markers. Only the resulting ordinary item alternatives enter
`ConstructionRecipeV1`; no reference name or client expansion crosses the
authoritative command/journal boundary.

This admits 17 more complete definitions: 16 HAMMER carpet transformations and
`constr_hay`. For example, pinned `["nails", 5, "LIST"]` becomes the exact
alternatives five `nail` or five `bronze_nail`. Combined with the earlier
slices, the catalog now contains 55 definitions: 18 empty-tile furniture
placements, 36 exact-floor carpet transformations, and one quality-gated
brick-oven terrain step. The Bevy client uses the same pinned resolver only for
affordability presentation; server normalization remains final authority.

## First disconnected survival-autopilot boundary

Protocol 42 makes the first implemented autopilot action deterministic flight,
not a general AI controller. After actor activities and needs but before hostile creature
turns, a living awake disconnected actor may spend a fully ready ordinary action
to flee visible aggressive creatures within eight Manhattan tiles. It starts
only when there is no queued semantic command and every retained activity is
absent or interrupted. Reconnection therefore wins at the next simulation
boundary, while an interrupted project's reservations remain intact.

Threats order by stable creature ID and actors act by stable actor ID. Eight
candidate steps have a fixed order; the selected loaded, passable, unoccupied
tile must strictly maximize the minimum distance from all perceived nearby
threats. Movement uses the same canonical terrain/furniture debt as a player
step and emits the ordinary replay-derived movement event. The autopilot never
generates a destination chunk, attacks, loots, changes equipment, or leaves the
currently loaded map while a safer retreat exists.

Protocol 43 adds the first narrow defense fallback. If and only if no loaded,
passable, unoccupied candidate step strictly increases the minimum threat
distance, the actor may spend the ordinary one-second melee cost to hit one
adjacent visible aggressive creature. Stable creature ID breaks ties. Damage
uses the actor's already wielded item or the same unarmed path as a player
command. It does not chase, attack a neutral creature, shoot, reload, select or
change equipment, or suppress the surviving creature's turn later in the same
tick. A blocked actor with no adjacent threat stays in place and remains
attackable. The resulting ordinary damage/death events and state reproduce
through SQLite recovery and portable replay.

Protocol 44 adds one conservative nutrition fallback after the threat path
finds no visible aggressive creature inside its eight-tile radius. The actor
acts only at the already
harmful canonical threshold—thirst at least 1,200 or stored calories at most
zero—and selects an owned, positive-charge, unwielded ordinary `FOOD` or
`DRINK` that strictly improves the active condition. Dehydration has fixed
priority over starvation; the actor's stable item map breaks ties. Consumption
spends the ordinary one-second action cost and emits the existing
`ItemConsumed` event. It never consumes from the ground, changes the wielded
slot, uses medicine, acts early for comfort, or consumes while a visible threat
is in range. Exact item charges and need state reproduce through the same
connection journal, SQLite recovery, and portable replay.

Protocol 45 adds a conservative safe-sleep fallback. After threat and emergency
nutrition handling decline to act, a tired actor may sleep only when stored
calories are positive, thirst is below the harmful threshold, and the actor's
current loaded tile contains furniture with positive modeled comfort. The
ordinary sleep transition uses a typed `SleepReason::Autopilot`, resets the
same action/sleep counters as voluntary sleep, and reproduces through snapshot,
SQLite recovery, and portable replay. It never sleeps on bare terrain, searches
for a bed, moves toward shelter, or sleeps while a visible aggressive creature
is inside the eight-tile radius.

Connection presence remains noncanonical metadata, but it now has canonical
consequences. Schema 29 therefore records each ordered
`ActorConnectionUpdateV1` beside the next journal tick and reapplies those
updates before held movement and semantic commands. Normal session transitions
apply immediately and are repeated idempotently at that boundary. On process
recovery, every formerly connected actor—and any offline actor retaining a
raced held-movement lease—receives a recorded disconnect in the first
unexpected-downtime tick; when there is no catch-up interval, those updates
seed the simulation thread and appear in its first live tick. This keeps
recovery from an online snapshot and portable replay behaviorally identical
without placing sockets or endpoint state in the canonical world hash.

This is intentionally narrower than the locked final policy. Fire and
dangerous-terrain escape, shelter search, medicine, wielded-equipment use,
sleep-location search and alarms, threat memory, and richer pathfinding remain
explicit future boundaries.

## First canonical field and creature-blood boundary

Protocol 49, schema 31, and CanonicalStateV29 establish fields as canonical map
state without pretending that fire is a small isolated feature. The strict
selected-content `field_type` registry resolves `copy-from`, inherited
intensity-level names, symbols, colors, danger/transparency, priority,
half-life, linear decay, splatter classification, and display state. Unknown
top-level keys remain attached to the definition as unsupported. The server
admits six creature-blood types for this slice; parsing the other definitions
does not activate their processors.

Every 12x12 chunk owns a parallel vector of sparse field collections. A tile's
entries are sorted by stable field type ID and contain intensity, age in whole
upstream seconds, and a globally monotonic display sequence. The sequence makes
upstream's “later field wins equal priority” rule explicit and replayable;
adding intensity to an existing type preserves its age and display order.
`next_field_sequence` and the admitted type catalog live in the world snapshot,
so no process-local content pointer or map iteration order affects canonical
state.

Field aging executes exactly on 20-tick whole-second boundaries and therefore
continues during empty-server operation and journaled downtime catch-up. Linear
half-lives decay at their exact age boundary. Exponential half-lives use the
upstream memoryless model, but their per-second probability is computed as
`1 - exp(-ln(2) / half_life)` in Q0.64 with a fixed integer series. A named
ChaCha8 stream includes world seed, tick, position, type, and display sequence.
This deliberately removes standard-library exponential-distribution and
`libm` variation from cross-platform replay. Intensity decrements reset age;
zero removes the entry and emits a replay-derived `FieldIntensityChanged`
event.

The MONSTER registry now resolves inherited material sets. The first runtime
blood derivation follows pinned ordering for `ACID_BLOOD`, `BILE_BLOOD`,
`ARTHROPOD_BLOOD`, vegetable/plant, insect flesh, then warm flesh. The only
fresh-world creature is the pinned `WARM` plus `flesh` zombie, so its ordinary
death adds exactly one `fd_blood` splatter at its death tile, matching upstream
`mdeath::normal` through `Creature::bleed`. Species fallback is not claimed
until a strict species registry exists.

Replication expands a visible field entry to its current pinned intensity
metadata only after authoritative LOS succeeds. Hidden tiles may use stale
terrain/furniture memory, but their field vector is always empty. The Bevy
client renders the display-enabled entry with maximum `(priority,
display_sequence)` and never reconstructs hidden field state. Fire fuel and
spread, smoke/gas movement, contact effects, water/outdoor acceleration,
mopping, bashing, item/terrain transforms, vertical falling, field-generated
light/transparency, and vehicle interaction remain separate fail-closed slices.

## First ordinary corpse and revival boundary

Protocol 50, schema 32, and CanonicalStateV30 make ordinary monster corpses
canonical items rather than retaining a dead creature beside presentation-only
debris. Fatal damage first emits blood and death, then applies the pinned
`mdeath::normal` overkill curve. A body beyond the strict pulverization boundary
leaves no ordinary corpse; gib production remains unavailable until its own
processor exists. Otherwise a newly allocated stable item stores type `corpse`,
damage zero through four, death tick, deterministic one-in-20 special flag, and
a strict self-contained creature prototype. Lack of an item-ID allocation is an
atomic typed rejection before the fatal command spends ammunition, action
readiness, or HP.

Corpses participate in the ordinary ground/inventory/drop/pickup/wield model,
but crafting, construction, and disassembly explicitly exclude them. Once per
in-world second, revivable corpses use whole age hours divided by damage plus
one, then require an effective age over six hours and the pinned age-weighted
one-in roll, becoming automatic at 48 effective hours. A special corpse also
requires a proximity roll. Upstream consults a singleton avatar here; the
multiplayer adaptation instead selects the nearest living actor within three
tiles with stable actor ID as the tie-break. This rule is server-owned and does
not privilege a connection or local client.

Revival removes the exact item from the ground or carrier, clears a matching
wielded slot, and allocates a new stable creature ID. An unoccupied passable
corpse tile wins; otherwise a named deterministic RNG chooses among valid
adjacent tiles. Speed starts at 80% and HP at 70% of the prototype, both divided
by corpse damage plus one, and the creature remains downed for five seconds.
The prototype travels in the corpse so revival never depends on mutable
process-local content after snapshot creation. Death drops, clothing transfer,
butchery, pulping, burning, rot, gibs, and nonordinary death functions remain
separate fail-closed boundaries.

## First content-derived monster-vision boundary

Protocol 51, schema 33, and CanonicalStateV31 make visual acquisition explicit
canonical creature state. The strict MONSTER loader now applies direct,
inherited, relative, and proportional `vision_day`/`vision_night` values, using
the pinned defaults 40 and 1. Fresh `mon_zombie` therefore carries `SEES`, day
40, and its explicit night override 3. Those values also travel in the corpse
prototype, so revival cannot silently consult changed process-local content.

An aggressive creature considers only living same-z actors that are inside its
current visual range and connected by transparent terrain/furniture LOS. It
chooses minimum tile distance and then stable actor ID. Modeled natural light
already exposes a deterministic two-to-60-tile solar/lunar sight abstraction;
monster range linearly interpolates from the type's night endpoint at two to its
day endpoint at 60. A modeled powered source that illuminates the target permits
the maximum of the type's endpoints, but the creature-to-target LOS and maximum
range still apply. This reuses canonical integer light state rather than adding
process-local floating ambient calculations.

Pinned `Creature::sees` has a singleton-avatar symmetry special case. On a
multiplayer server no actor is the process-global avatar, so all player
characters use the same monster-owned visual test. Hearing, scent, remembered
destinations, visibility/camouflage modifiers, blindness and other effects,
three-dimensional vision, bashing, and obstacle-routing remain separate
fail-closed boundaries.

## First terrain-costed creature-movement boundary

Protocol 52, schema 34, and CanonicalStateV32 replace the creature's unsigned
fixed-cadence counter with signed action readiness. Speed still accrues on each
20 Hz simulation tick. Once readiness reaches 2,000, the server resolves one
action and subtracts its actual cost; an expensive move therefore occurs at the
readiness boundary and leaves debt rather than waiting to bank a potentially
unbounded positive balance.

The current cardinal pursuit step uses the canonical source and destination
terrain movement costs plus nonnegative furniture modifiers. Their sum is
multiplied by the pinned cardinal factor 25 and by 20 local action points per
upstream move. Ordinary floor-to-floor movement is therefore 100 upstream moves
or 2,000 action points, and a floor-to-bed transition is 175 moves or 3,500
action points. Melee and a turn with no executable modeled action retain the
ordinary 100-move charge. Strict snapshots bound signed debt with the same
worst-case canonical tile-cost limit as actors, and persistence/replay retain it
exactly.

This boundary deliberately does not claim the rest of `monster::move`.
`STUMBLES` candidate weighting, bashing, `path_settings`, terrain/field danger,
special movement modes, diagonal stagger adjustment, scent, sound, and target
memory require explicit content and simulation slices before they may affect an
authoritative turn.

## First direct-pursuit stumble boundary

Protocol 53, schema 35, and CanonicalStateV33 preserve whether a creature has
the already strictly loaded `STUMBLES` flag. The value is copied into live
creatures and self-contained corpse prototypes, so revival and replay cannot
silently reconsult a changed content registry. The pinned default zombie has
the flag; synthetic tests opt in explicitly.

Direct pursuit now builds the pinned `squares_closer_to` fan. For an x-dominant
vector it considers the direct cardinal, both forward diagonals, and then the
other target-facing cardinal when present; y-dominant and pure-diagonal vectors
use the corresponding pinned order. Impassable, occupied, or non-progressing
squares are removed. A creature without `STUMBLES` chooses the first remaining
square. A stumbler retains the direct option initially and applies the pinned
progress-over-twice-cumulative-progress replacement probability to each later
option. A named ChaCha8 stream keyed by world seed, tick, creature ID, and its
same-tick turn sequence makes that selection replayable and independent of map
iteration order.

Upstream computes both selection progress and default-`CIRCLEDIST` stagger cost
with floating Euclidean distance. Canonical simulation instead takes an integer
square root of squared distance shifted to Q30. Selection uses those fixed-point
progress weights. Movement takes the ceiling of base upstream terrain/furniture
cost multiplied by the same progress, then scales by 20 local action points per
move. A direct floor diagonal therefore costs 142 upstream moves; a pinned test
flanking step that advances approximately 0.876 tiles costs 88. This keeps the
behavioral boundary deterministic across supported targets while retaining the
upstream geometry and ceiling. Exact squared-distance comparison preserves a
positive candidate even below one Q30 unit, while movement retains upstream's
minimum 0.01 stagger multiplier.

Bashing, door opening, pushing, creature-vs-creature attitudes, path settings,
sound, scent, target memory, danger fields, z movement, and special locomotion
remain separate fail-closed boundaries.

## First canonical last-seen-goal boundary

Protocol 54, schema 36, and CanonicalStateV34 add one optional same-z world
position to canonical creature state. Whenever an aggressive creature selects
a currently visible living actor, that actor's exact position replaces the
goal. If no actor is visible, the creature continues using the stored goal with
the existing deterministic candidate fan, stumble selection, collision rules,
and terrain/furniture cost. Reaching the exact tile clears the goal. No sound,
scent, inferred motion, search radius, or path cache is invented by this slice.

The field is strict persisted simulation state: a goal on another z-level or on
the creature's current tile is rejected, canonical hashing includes it, and
snapshots, SQLite recovery, and portable replay reproduce occluded pursuit.
Creature turns still attack only a currently visible adjacent actor; remembered
intent never authorizes damage against hidden occupancy.

Canonical creature state is no longer a network DTO. Replication projects each
currently visible creature into `VisibleCreatureSnapshot`, containing only its
stable ID, type ID, position, HP, and maximum HP. Last-seen goals, signed action
readiness, vision/aggression/melee values, downed timing, blood type, and corpse
prototype are absent by construction. This preserves server authority and
prevents AI intent or reconstruction data from becoming a client-visible side
channel.

Sound and scent goals, route planning, obstacle bashing/opening, danger fields,
z movement, and special locomotion remain separate fail-closed boundaries.

## First structural-bashing boundary

Protocol 55, schema 37, and CanonicalStateV35 introduce immutable world-level
terrain and furniture bash catalogs plus a parallel per-tile `u16` damage
array. Catalogs are registered only while creating tick-zero state and then
travel in every strict snapshot. This avoids recursive content definitions on
tiles, makes hash ordering explicit, and keeps recovery independent of mutable
process-local registries. A tile with positive damage must still reference an
admitted bash type and remain below that type's maximum hit points.

The strict content importer resolves inherited common bash metadata and the
pinned default and wooden-door damage profiles. Profile multipliers are parsed
as exact millionths, so canonical damage never depends on floating point. The
runtime initially admits `t_door_c` and `t_door_b`; their direct-item drops,
hit/destroyed fields, result terrain, and sound are normalized into the world
catalog. `t_door_frame` is retained by content but rejected at runtime because
its `t_null` result requires exact roof/new-floor semantics that do not yet
exist. Item groups, collapse, explosive/tent behavior, bash-below, and other
unsupported effects also remain fail-closed.

`BASHES` and `GROUP_BASH` are canonical creature capabilities copied through
ordinary corpses and revival. When direct pursuit has no preferred passable
step, a bashable impassable candidate may be selected. A `GROUP_BASH` creature
estimates a target using twice its base bash strength and avoids estimates below
the pinned bad-choice rating while alternatives remain. Actual group strength
walks five rows behind the target in a three-wide swath and accepts only
connected `GROUP_BASH` creature paths, with pinned Chebyshev falloff. Base bash
strength is exact melee dice times sides. Cardinal adjacent `BLOCKSDOOR`
furniture selects the blocked strength threshold.

Each attempt spends creature speed times 20 action points. Fixed-point profile
damage minus minimum strength accumulates semi-persistently. Success first
precomputes all direct-item outputs, placement, and stable IDs. If any resource
is unavailable, damage stops at one below destruction and no terrain, item, or
field mutation occurs; retry after capacity returns can complete the same
stage. A committed success changes terrain, clears structural damage, creates
stable debris in deterministic order, applies hit/destroyed fields, and emits a
canonical `CreatureBashed` record containing result, exact damage, accumulated
damage, and structural sound.

Canonical events and public replication are separate trust boundaries. The
visible creature DTO still contains only stable ID, type, position, and public
HP; bash capability, group membership, action debt, and catalogs are private.
The new event is retained for deterministic replay and server-side processors,
but it is not yet generalized into spatially filtered client hearing. Exposing
sound later must derive a public audible observation rather than forwarding the
canonical event (whose target and terrain fields may be hidden). Furniture
bashing, the final door-frame floor replacement, route planning, public sound
observations, and broader terrain effects remain separate slices.

## First authoritative monster-hearing boundary

Protocol 56, schema 38, CanonicalStateV36, and CanonicalEventsV9 make sound
stimulus and monster response explicit replay state. The strict ITEM importer
now resolves direct, inherited, relative, and proportional `loudness`. An
absent gun value means zero. An absent ammunition value uses the pinned
ballistic fallback: twice range plus twice every damage-unit amount and armor
penetration, truncated to an integer before adding gun loudness. The fresh-world
`sw_619` and `38_special` therefore resolve to volume 70. A successful admitted
ranged attack records its source position, bounded volume, and the pinned
description (`bang!` at 70) in the canonical event; no client supplies these
values.

`HEARS` and `GOODHEARING` are canonical creature capabilities copied into live
creatures and self-contained corpse prototypes. `GOODHEARING` without `HEARS`
is invalid. A normal listener perceives `volume - Chebyshev distance`; good
hearing perceives `2 * volume - distance`. This first boundary is same-z only.
Nonpositive perception is ignored. Nonprovocative gun and structural-bash
stimuli use the pinned mean-30, standard-deviation-5 interest shape; canonical
code approximates that normal variate as a 12-uniform Irwin-Hall sum in Q32 so
replay does not depend on platform floating-point distribution code. All draws
come from a named ChaCha8 stream keyed by world seed, tick, creature ID, and
canonical event ID.

An interested sound produces an imprecise same-z goal. Perceived volume below
2, 5, 10, or 20 uses maximum coordinate error 10, 5, 3, or 1 respectively;
louder sound is exact. A creature with an existing sound goal has the pinned
one-in-two chance to retain it and never replaces a longer-lived goal with a
weaker one. Normal pursuit lasts perceived-volume creature actions; good
hearing multiplies that lifetime by six. Lifetime decrements exactly once per
creature action, not per server tick, and reaching the inferred tile clears it.
Strict restoration caps lifetime at the reachable `2 * u16::MAX * 6` actions.
Imprecise coordinate offsets saturate at world-coordinate bounds so hearing
cannot abort an already-mutated tick through integer overflow.

Phase order is explicit. Actor gunfire is processed after actor/autopilot work
and before creature turns, allowing a ready listener to respond in the same
tick. Creature bash events are processed after the creature phase in stable
event and stable creature-ID order, making them available on later actions
without recursively changing the current creature iteration. Current visible
actor position has highest destination priority, persisted visual memory is
next, and private sound intent is last.

Sound goals and hearing flags survive strict snapshots, canonical hashing,
SQLite journal recovery, and portable replay. They are deliberately absent
from `VisibleCreatureSnapshot`; forwarding the canonical ranged or bash event
would disclose hidden origin or terrain state. A later client-audibility slice
must derive a bounded spatial observation DTO. Player hearing, UI sound
markers, z-level propagation, obstacle/weather attenuation, sound clustering,
provocative sounds, footsteps, speech, alarms, vehicles, and the wider sound
producer catalog remain fail-closed.

## First content-derived monster door-opening boundary

Protocol 57, schema 39, CanonicalStateV37, and CanonicalEventsV10 add terrain
door opening without inventing a general pathfinder. The strict MONSTER flag
projection copies `CAN_OPEN_DOORS` into live creatures and self-contained corpse
prototypes, so revival, strict snapshots, SQLite recovery, and portable replay
retain the exact capability. The pinned default zombie does not have this flag;
the pinned feral-human family does. Fresh-world population remains the default
zombie because the feral special-attack surface is not yet modeled.

Candidate selection follows the pinned direct-movement order. An already
passable square is an ordinary move. Otherwise, a capable creature remembers a
terrain `open` projection before considering structural bashing, without adding
that door to stumble weight; a later passable alternative can replace the door
under the same pinned selection rule. Incapable creatures cannot use that path.
Furniture and vehicle doors, locks/unlocking, and route-planner
`allow_open_doors` policy are separate boundaries.

Opening atomically applies the canonical transform, clears incompatible partial
structural damage, increments the chunk revision through normal terrain
mutation, and emits `CreatureOpenedTerrain` with source/result terrain IDs plus
the pinned movement sound `swish` at volume 6. Upstream spends zero monster
moves on the open operation. The local readiness loop therefore charges zero,
runs the creature again, and can move it through the now-passable doorway in
the same tick. This also decrements action-counted sound intent once for opening
and again for entering, matching upstream's repeated `monster::move` calls.

Canonical door sound is processed after the creature phase as a same-z
nonprovocative stimulus, alongside structural bash sound, and may create a
private later-turn sound goal. Neither the capability, goal, nor canonical open
event crosses the public creature/event DTO boundary. Client audibility must
later be derived from spatial perception rather than forwarding hidden terrain
state.

`OPENCLOSE_INSIDE` requires authoritative indoor/outdoor side topology. The
content definition remains loaded, but runtime terrain construction withholds
that source transform until the topology exists. Pacification, hallucination
and other effect restrictions, door closing by creatures, furniture/vehicle
doors, unlocking, routing, and client audible observations remain fail-closed.

## First content-derived monster obstacle-routing boundary

Protocol 58, schema 40, and CanonicalStateV38 add routing policy without a new
canonical event shape, so CanonicalEventsV10 remains current. The strict
MONSTER registry resolves all fields observed inside inherited
`path_settings`: `max_dist`, `allow_open_doors`, `avoid_traps`, `avoid_sharp`,
`avoid_dangerous_fields`, and `allow_climb_stairs`. The upstream default is
distance zero, doors and hazard avoidance false, and stair climbing true. An
absent explicit `max_length` in the pinned selected corpus finalizes to
`max_dist * 5`, matching upstream. Protocol restoration caps the currently
selected maximum at 400.

A zero route distance leaves the existing direct `squares_closer_to` pursuit
unchanged. For a positive distance and same-z goal inside that range, canonical
simulation searches only already loaded tiles. It uses upstream's cardinal-
then-diagonal neighbor order, 16-tile endpoint padding, ordinary
terrain-plus-furniture cost, one extra point for diagonals, cost 4 for a
route-admitted terrain door, and cost 500 for a known dangerous field when the
setting requests avoidance. The first reconstructed adjacent step becomes the
movement destination for candidate selection, progress weighting, and stagger
cost. It may therefore move sideways or temporarily away from the ultimate
goal to round an obstacle. Failure or an out-of-range goal falls back to direct
greedy movement and never generates a chunk.

Upstream's equal-score priority-queue order is not a portable contract. The
multiplayer adaptation orders equal scores by absolute `WorldPosition`, while
retaining the pinned neighbor order and cost model. Search is additionally
bounded by the 400-tile protocol cap, endpoint padding, loaded-world boundary,
and five-times-distance cost. The route itself is derived every creature action
rather than persisted; deterministic settings, world state, and tie-breaking
reproduce the same first step after snapshot recovery and portable replay.

The settings are canonical private creature state and are copied into ordinary
corpse prototypes and revival. They remain structurally absent from
`VisibleCreatureSnapshot`. Pinned content proves the default zombie keeps
distance zero while `mon_feral_human_pipe` supplies distance 45, route-open
doors, and trap/sharp avoidance. Fresh worlds still spawn the zombie because
the feral special-attack set is outside the implemented boundary.

This first search admits modeled passable terrain and terrain doors. Structural
bashing remains direct/greedy and is not priced into routes. Trap and sharp
policies persist but no canonical trap/sharp tile layer can currently
instantiate them; vertical paths likewise wait for z-level and stair semantics.
Route caching and pathfinding cooldown/backoff, furniture/vehicle doors,
creature-danger avoidance, climbing, swimming, ramps, locked doors, and other
path costs remain fail-closed.

## First content-derived furniture-bashing and final door-frame boundary

Protocol 59, schema 41, CanonicalStateV39, and CanonicalEventsV11 extend the
existing structural-bash model across the furniture layer. Upstream
`map::bash_ter_furn` always chooses bashable furniture before terrain at the
same position. Canonical simulation now performs that same selection, uses the
shared per-tile damage accumulator, and leaves the underlying terrain unchanged
when furniture is removed or replaced. Furniture and terrain retain separate
strict definition registries, but use the same fixed-point profile damage,
blocked support check, direct-drop preflight, hit/destroyed fields, sound, group
bash strength, action cost, recovery, and deterministic named randomness.

The first admitted furniture definition is the pinned cabin `f_dresser`. Its
default bash profile has a 1,000,000-millionths bash multiplier, removes to
`f_null`, produces only already modeled direct item/charge drops, and uses the
pinned dust and splinter fields. The cabin already places this furniture in a
fresh world, so monster bash attempts can target it without a fixture; the
single starter zombie is not strong enough to destroy it alone. Definitions with
tent centers, non-direct item groups, explosions, collapse, or other retained
unsupported semantics remain excluded from runtime registration.

The wooden terrain chain now registers `t_door_frame` after `t_door_c` and
`t_door_b`. Upstream treats its `t_null` result as a request to derive a new
floor from z-level support and roof metadata. The current generated cabin has a
known z=0 floor topology but does not yet carry the general roof/support graph,
so the server explicitly normalizes this one frame result to `t_floor`.
Supplying a dynamic result for a terrain that does not request it is rejected,
and every other unresolved `t_null` terrain remains fail-closed. This prevents
an impassable sentinel tile from entering canonical state while avoiding a
false claim of general roof repair.

`CreatureBashed` now carries `BashTargetKindV1` plus the exact target content ID
instead of a terrain-only field. This is a canonical event-shape change and is
why CanonicalEventsV11 is required. It lets replay, hearing, audit, and client
presentation distinguish furniture from terrain without revealing any private
creature AI state. Focused restoration tests advance live and restored worlds
through dresser removal and frame destruction and require identical events and
CanonicalStateV39 hashes.

Item damage and `on_drop`, signage, plants, fungus, alarms, tents, collapse,
explosions, bash item groups, supported-strength variants, arbitrary dynamic
roof/floor repair, vehicle layers, and broader furniture admission remain
separate fail-closed boundaries.

## Broad strict furniture-bash admission

Protocol 60, schema 42, and CanonicalStateV40 broaden the Protocol 59 runtime
registry without changing the typed CanonicalEventsV11 shape. `FurnitureRegistry`
exposes its stable content-ID order to the server, which derives runtime bash
definitions only when every currently needed semantic is representable. The
predicate requires nonnegative ordinary bounds inside protocol limits; either
an absent or valid blocked variant; no supported-floor variant; a fully
supported bash body; `t_null` terrain behavior; only registered dust/splinter
fields; at most 128 direct drop types and one reservation block of worst-case
outputs; and bounded sound volumes. Normalization still resolves every profile,
item prototype, field intensity, and furniture replacement and fails startup if
an admitted definition becomes invalid.

Admission is a fixed point over furniture replacement edges. A definition that
replaces itself with another bashable furniture type remains admitted only when
that target is also in the admitted set; pruning repeats until stable. A result
with no bash behavior is terminal and valid. This prevents a fully modeled
first hit from creating a later stage whose unsupported semantics would be
silently lost.

The pinned baseline admits 537 of 699 concrete furniture definitions. This is
a locked content boundary, not a claim that the other 162 definitions are
equivalent: many have no bash behavior, while others require tents, collapse,
explosions, item groups, supported-strength handling, or another excluded side
effect. Every furniture ID placed by the fresh cabin is in the admitted set.
The canonical registry snapshot encodes below a 128-KiB guard (67,928 bytes in
the characterization run), validates every entry, and restores exactly.

Simulation already distinguishes removal from replacement in the canonical
definition. Protocol 60 adds a focused replacement test proving that an
admitted furniture result changes only the furniture layer, clears shared map
damage, emits a typed successful bash event, retains the underlying terrain,
and restores from a snapshot. Protocol 59's dedicated SQLite recovery and
portable replay test continues to cover removal and the same event path.

The admission predicate is deliberately server-owned rather than a content
loader success flag. Parsing a furniture definition still does not imply its
runtime side effects are safe. Expanding fields, supported strength, item
damage/`on_drop`, plants/fungus/signage, tents, collapse, explosions, groups,
and dynamic floors requires separate canonical models and tests before those
definitions can cross this boundary.

## Route-planned bashing with base-strength estimates

Protocol 61, schema 43, and CanonicalStateV41 extend the positive-distance
same-z A* search across the already admitted terrain and furniture bash
registries without changing CanonicalEventsV11. The ordering matches pinned
`map::cost_to_avoid`: ordinary passable movement wins first, then an admissible
door-open transform, then bashing. That rating uses the ordinary unblocked
strength bounds, as upstream `bash_rating_internal` does; contextual blocked
bounds are selected only when the actual bash executes. An obstacle whose base
bash rating exceeds one costs `(20 / rating) + 12`; rating one is a desperate
cost of 500; rating zero or an unregistered target remains impassable. Existing
dangerous-field penalties then compose with that base tile cost.

The route estimate intentionally uses `mtype::path_settings.bash_strength`,
which pinned finalization derives from the monster type's base bash skill when
no explicit override exists. The selected corpus has no explicit override.
This differs from immediate greedy candidate evaluation, where `GROUP_BASH`
doubles the estimate, and from the actual hit, where connected helpers in the
five-row swath contribute. Planning therefore does not depend on mutable helper
positions and matches the upstream separation between type path settings,
`bash_estimate`, and `group_bash_skill`.

The focused simulation boundary locks all three rating classes and the
ordinary-estimate/blocked-execution split, and proves that
a strong basher chooses a short destructive route while a base-rating-one group
basher rejects cost 500 in favor of a longer walkable route. Live and restored
worlds produce the same typed bash event and CanonicalStateV41 hash. A separate
SQLite/portable-replay fixture places the only bashable opening away from the
direct candidate fan; the creature must first move sideways, then destroy the
opening, so replay cannot accidentally pass via the old greedy fallback.

The fresh default zombie retains upstream's zero `max_dist`, so this does not
silently make it an A* user. Route caching, pathfinding cooldown/backoff,
vertical movement, traps/sharp terrain, nearby-creature danger avoidance,
vehicles, and targets with unsupported bash side effects remain separate
fail-closed boundaries.

## First player-controlled structural-smashing boundary

Protocol 62, schema 44, CanonicalStateV42, and CanonicalEventsV12 add a typed
`Smash { dx, dy }` command for all eight horizontal adjacent directions. The
Bevy client binds H to the existing bounded terrain picker. Each currently
visible observation carries only the authoritative bash target layer, with
registered furniture before registered terrain; remembered hidden tiles carry
no live bash metadata. This is discovery and presentation data, not authority:
simulation revalidates the direction, current tile layers, and registry at
execution time.

Canonical state separately stores a sorted set of every pinned furniture ID
whose resolved content definition has an upstream bash body. Runtime-admitted
furniture bash definitions must be a subset of this set. A present furniture
layer in the complete set but absent from the admitted definitions is an opaque,
unsupported structural interaction: neither simulation nor replication falls
through to registered terrain below it. Furniture with no upstream bash body
continues to expose an otherwise valid terrain target, matching the upstream
layer order without approximating excluded behavior.

Pinned `Character::smash_ability` adds arm Strength to each wielded damage unit
using that damage type's bash-conversion factor. The first exact subset admits
only wielded items whose positive modeled melee damage is integer bash-only,
for which the factor is exactly one, and whose damage level is at most one so
upstream applies no degradation multiplier. With the explicit current default
arm Strength 8, the clean pinned hammer's bash 9 becomes structural strength
17. No weapon, sub-one/fractional bash damage, damage level above one, or any
positive mixed cut/stab profile rejects with
`InvalidBashTool`; this avoids inventing anatomy or uncanonical profile
multipliers. The action currently pays the same standard 100-upstream-move
budget as existing actor melee, an explicit multiplayer-port adaptation until
weight/volume/skill-dependent weapon attack time is canonical.

Actor and creature attempts share furniture precedence, contextual blocked
bounds, fixed-point profile scaling, semi-persistent tile damage, atomic stable
drop IDs, replacement, fields, and sound. They retain separate named RNG
domains and emit separate typed actor/creature events. Actor sound enters the
same private monster-hearing phase before creature actions. Focused tests lock
diagonal layer order, strength and damage, fail-closed tools, restored event and
state identity, visible-only replication, SQLite recovery, and portable replay.
Unarmed body-part selection, complete damage profiles, attack time, weapon wear,
stamina/exertion, skill practice, faction warnings, and field/corpse/vehicle
precedence remain closed.

## Canonical base Strength and strict smash-item timing

Protocol 63, schema 45, and CanonicalStateV43 retain CanonicalEventsV12 while
removing the first smash boundary's temporary stat and action-duration
adaptations. Every actor now persists a bounded base Strength. Until body-part
HP and limb scores exist, the only admitted projection is a healthy
`character_modifier_limb_str_mod` of exactly one; no hidden injury or mutation
modifier is invented. New survivors retain pinned default Strength 8, and the
controlled-character HUD plus administrator-private inspection expose that
canonical value.

Canonical world state also stores an item-type-ID-sorted strict smash-item
catalog. An entry contains exact integral bash damage and pinned
`item::attack_time`; a live/restored item must match the catalog damage exactly,
be no worse than superficial damage level one, and contain no positive
non-bash damage. The server derives entries only for ordinary non-charge pinned
items without `REDUCED_BASHING`. Guns and charge-bearing item types or types
with ammunition, magazine, or powered state are excluded because
`item::attack_time` reads live weight and volume. Simulation independently
rejects live ranged, ammunition, magazine, or powered shapes even if a corrupt
catalog names their type. Count-by-charge aggregate rounding, faults,
gunmods, bayonets, enchantments, and other damage conversions remain absent
rather than approximated.

For an ordinary item, pinned attack time is `65 + floor(volume / 62.5 ml) +
floor(weight / 60 g)`. The hammer's 320 ml and 566 g therefore produce 79
moves. Player smashing pays the pinned truncating 80% multiplier, or 63 moves,
through the existing signed readiness-debt scheduler. The strict catalog,
base Strength, action debt, state hash, SQLite recovery, and portable replay
round-trip together; the actor bash event shape does not change.

## Canonical base stats and actor-dependent learning

Protocol 64, schema 46, and CanonicalStateV44 retain CanonicalEventsV12 and make
base Strength, Dexterity, Intelligence, and Perception bounded canonical actor
state. New survivors use the pinned default 8 for each stat unless the freeform
creator supplies another bounded value. All four values participate in
snapshot validation, persistence, replay, replication, private inspection, the
operator view, the Bevy HUD, and the state hash. DEX is deliberately stored
before its first gameplay consumer so later combat work does not require another
character-identity retrofit.

The reading catalog now stores the pinned BOOK Intelligence requirement and
unadjusted base duration. At activity admission, the simulation applies pinned
`base_time + floor(base_time * max(requirement - INT, 0) / 60)` with checked
integer arithmetic. Completion uses the actor's INT in the pinned minimum and
maximum comprehension formulas; the accepted start sequence continues to name
the deterministic XP stream, so disconnect, interruption, and delayed resume do
not reroll it.

Disassembly practice uses canonical INT/PER in the pinned catch-up modifier
`1 + (2*INT + PER)/24` and knowledge modifier `1 + INT/40`. The port evaluates
these as checked rational integers and truncates gains when committing them,
preserving deterministic replay while covering the upstream minimum-one branch
and 90% knowledge cap. Focus remains fixed at 100 and training remains enabled;
traits, enchantments, non-stat generation choices, other stat consumers, and non-default
focus remain future boundaries.

## Pinned freeform character creation stats

Protocol 65 retains schema 46, CanonicalStateV44, and CanonicalEventsV12. The
pinned baseline defines `CHARACTER_STAT_MIN=4`, `CHARACTER_STAT_MAX=20`, starts
all four stats at 8, and runs the current creator with pool type `FREEFORM`.
Accordingly, the multiplayer port independently bounds STR, DEX, INT, and PER
from 4 through 20 and imposes no shared point budget. This is a source-backed
current-baseline decision, not an approximation of the inactive legacy
multi-pool or single-pool creators.

`CharacterRequest::Create` carries a versioned value object containing all four
stats over the authenticated iroh control stream. Wire validation rejects any
out-of-range value. The Bevy-free simulation repeats the invariant check before
position search or stable actor-ID allocation, then writes the exact values to
the canonical actor. The existing crash-reconciled creation transaction binds
that actor snapshot to the character record, so a commit cannot silently
substitute defaults. The interactive Bevy client selects a stat with Up/Down
and changes it with Left/Right; its automation shortcut deliberately supplies
the four defaults.

Creation stats change no canonical snapshot or event shape because Protocol 64
already persisted all four fields. Scenario, profession, trait, skill,
appearance, gender, name randomization, and other creation choices require
their own strict content and gameplay slices.

The first follow-up audit fixes the dependency order for those remaining
choices. The pinned generic `evacuee` scenario names `sloc_shelter_safe`, which
the current generated cabin cannot impersonate. The pinned generic
`unemployed` profession grants gender-specific worn clothing plus
`charged_smart_phone` and `starter_wallet_full` item groups, including nested
contents. Exposing either choice as a label while omitting these consequences
would create false parity. Scenario/profession selection therefore waits for
start-location mapgen, sex selection, worn/pocket state, and deterministic
item-group generation; unsupported choices remain absent from the protocol.

## DEX-adjusted melee attack speed

Protocol 66 retains schema 46, CanonicalStateV44, and CanonicalEventsV12. It
implements pinned `Character::attack_speed` for the subset whose inputs are
already exact: unarmed actors and clean or superficially damaged ordinary
bash-only wielded items admitted by the player-smash catalog. The null item has
`item::attack_time=65`; admitted weapons carry their pinned static weight/volume
attack time. The current boundary has full stamina, healthy lift/balance limbs,
no posture penalty, no martial-arts modifier, and no enchantment modifier.

For those actors, simulation evaluates `base=floor(item_attack_time/2)`,
`skill_cost=floor(base*(15-practical_melee_skill)/15)`, and
`max(25, base+skill_cost-floor(effective_DEX/2))`. Effective base stats use the
pinned default maximum of 20 even though the serialized fields retain a larger
defensive corruption ceiling. It then converts the final upstream
move count to action points exactly once. Thus default DEX 8, melee 0 yields 60
moves unarmed and 74 with the pinned 79-move hammer. The separate pinned smash
multiplier still makes that hammer's structural smash cost 63 moves.

The same cost function governs player-vs-player, player-vs-creature, and
disconnected trapped-defense attacks. Signed remaining readiness is already
canonical, so recovery and replay preserve the faster cadence without a state
or event format change. A wielded instance outside the strict catalog retains
the existing 100-move melee cost: exact live-weight timing for guns,
ammunition/magazines, powered tools, count-by-charge stacks, mixed-damage and
degraded weapons requires instance weight, condition, faults/mods, and broader
melee semantics first. Hit/dodge rolls, criticals, stamina burn, limb effects,
martial arts, wear, and practice remain separate boundaries.

## Canonical monster accuracy and dodge prerequisite

Protocol 67, schema 47, and CanonicalStateV45 retain CanonicalEventsV12. The
strict MONSTER loader already finalized inherited `melee_skill` and `dodge`;
the simulation now stores both as private authoritative `u16` creature state.
Fresh-world spawning converts the finalized values exactly, so the pinned
classic zombie carries melee skill 4 and dodge 0 rather than temporary combat
constants.

Both values are included in snapshots and the canonical hash. Ordinary corpse
prototypes copy them alongside the other immutable monster-type inputs, and
revival reconstructs them without consulting process-local content. Spawn and
snapshot validation reject a live creature whose immutable corpse accuracy,
dodge, or damage dice disagree. Live speed is intentionally exempt from this
equality because pinned corpse damage reduces a revived monster's speed.
SQLite recovery and portable replay preserve nonzero values exactly.

The public visible-creature DTO continues to expose only stable ID, type,
position, HP, and maximum HP; accuracy and dodge remain server-private. This
protocol is deliberately a state prerequisite and does not change hit outcomes
or event shapes. The following combat slice can therefore implement pinned
player hit versus monster dodge without another creature-identity migration.

## First deterministic player hit/dodge boundary

Protocol 68 retains schema 47 and CanonicalStateV45 and advances the canonical
event hash domain to CanonicalEventsV13. Its exact admitted subset is an
empty-handed actor attacking the pinned medium `mon_zombie`. Pinned
`Character::get_hit_base`, `Character::get_hit_weapon`, and the null item's
default to-hit combine as `DEX/4 + practical_melee/2 - 2`; the null item does
not contribute an unarmed-skill term. The target contributes `dodge * 5`, and
the medium-size penalty is zero. A spread of zero is a hit, matching the pinned
melee boundary.

The source uses `normal_roll(accuracy * 5, 25)`. Since C++
`std::normal_distribution` is implementation-defined, exact cross-platform
multiplayer replay takes precedence over matching a particular standard-library
sample sequence. Simulation reuses the documented integer Q32 sum-of-twelve-
uniforms normal adaptation already used for monster hearing. A named session
ChaCha8 stream is keyed by world seed, domain, stable actor and creature IDs,
and accepted command sequence. It intentionally excludes execution tick so the
admitted command cannot change its roll merely by waiting in the deterministic
queue. Disconnected defense uses current canonical tick as its one-action-per-
actor sequence input.

Negative spread produces `ActorMissedCreature`, spends the same exact
DEX/practical-melee attack cost as a hit, and changes neither creature HP nor
corpse-ID state. Only the source actor receives the public event. Recovery and
portable replay lock the same miss and CanonicalEventsV13 hash. Armed attacks,
other monster types and sizes, monster attacks, criticals, techniques, and
remaining accuracy/dodge modifiers retain the explicit guaranteed-hit boundary
until their canonical inputs are implemented.

## Strict ordinary bash-weapon accuracy

Protocol 69 advances to schema 48 and CanonicalStateV46 while retaining
CanonicalEventsV13. ITEM `to_hit` is now a strict finalized input. The importer
accepts pinned legacy integers and the current object form, maps
grip/length/surface/balance with the exact upstream enum offsets, and applies
inherited integer `relative.to_hit`; absent values retain upstream default -2.
Unsupported shapes fail closed. The existing immutable ordinary bash-only
profile stores the bounded result, so it is hashed and restored independently
of process-local content. The pinned hammer's object resolves to -1.

An already-admitted weapon attacking the pinned medium `mon_zombie` uses
`DEX/4 + dominant_skill/3 + practical_melee/2 + item_to_hit`. Because the
strict profile contains only integer bash damage, the dominant skill is
bashing exactly when damage exceeds pinned `MELEE_STAT` 5; at or below five,
`item::is_melee` is false and the dominant skill is null. Twelfths retain both
thirds and quarters exactly before the Protocol 68 fixed-point normal
adaptation. The same named session stream, target dodge, nonnegative-hit rule,
source-private miss event, and exact attack cost apply online and during
disconnected trapped defense.

Mixed/fractional damage, deeply damaged instances, guns, ammunition or
magazine state, powered items, and types outside the strict profile do not use
this path. Other monster types and sizes, monster attack rolls, martial arts,
enchantments, effects, techniques, criticals, wear, stamina, and practice also
remain explicit later boundaries.

## Sleeping-target monster accuracy and clumsy misses

Protocol 70 advances to schema 49, CanonicalStateV47, and CanonicalEventsV14.
The first exact monster-side subset is a pinned `mon_zombie` attacking a
sleeping actor. Pinned `monster::get_hit_base` is the finalized MONSTER
`melee_skill`, and `melee_hit_range` is `normal_roll(accuracy * 5, 25)`.
Sleeping makes `Character::can_try_dodge` fail, so the target's dodge roll is
exactly zero without approximating unimplemented dodge-attempt, stamina,
encumbrance, effect, or limb state. The simulation uses the established Q32
twelve-uniform normal adaptation in a tick- and turn-sequence-keyed
`creature-melee-hit` stream. Nonnegative spread hits; negative spread spends
the normal 100-move attack but causes no damage, activity interruption, or
wake-up. Awake actors and other monster types retain guaranteed monster hits
until their missing defense inputs become canonical.

Finalized `CLUMSY_ATTACKS` is copied into private live-creature and immutable
corpse-prototype state and restored on revival. After the twelve hit samples,
an admitted miss by a clumsy creature consumes the next stream value for
pinned `one_in(4)`. Success sets the existing private downed deadline to two
seconds after the attack tick and immediately stops additional same-tick
actions. A typed `CreatureMissedActor` with the stable source, target, and
stumble result participates in CanonicalEventsV14. It is deliberately not
replicated: pinned CDDA suppresses both ordinary miss and stumble/fall messages
while the target is asleep. Public creature DTOs continue to omit accuracy,
clumsiness, action debt, and downed state. Snapshot restoration, corpse
revival, canonical hashes, SQLite recovery, and portable replay retain the
exact private consequence.

## All ordinary monsters against sleeping actors

Protocol 71 retains schema 49, CanonicalStateV47, and CanonicalEventsV14. The
Protocol 70 type-ID check was only a rollout boundary, not an upstream combat
input. For an ordinary monster attack, finalized MONSTER `melee_skill` is the
base accuracy regardless of monster type or attacker size. Every modeled actor
is currently medium, so `size_melee_penalty` is zero; sleeping still forces
the exact zero dodge. The same deterministic roll therefore applies to every
canonical creature with nonzero `melee_dice`.

Pinned `monster::melee_attack` returns before damage and miss consequences
when `melee_dice` is zero, so zero-dice creatures remain outside hit and
clumsy-miss handling. The existing named stream, nonnegative hit boundary,
100-move action cost, no-damage/no-wake miss, private `CLUMSY_ATTACKS`
one-in-four fall, two-second down deadline, non-replicated canonical event,
SQLite recovery, and portable replay are unchanged. Awake defense still
requires canonical dodge attempts, stamina, encumbrance, effects, and limb
state and is not approximated.

## Canonical monster base size and player melee modifiers

Protocol 72 advances to schema 50 and CanonicalStateV48 while retaining
CanonicalEventsV14. Pinned MONSTER `volume` defaults to 62,499 ml, accepts the
shared signed-integer ml/L quantity grammar, and participates in ordinary
`copy-from`, direct, proportional, and relative loading. Proportional loading
rejects nonpositive or unit scalars and truncates the floating product toward
zero when converting back to the integer base unit, matching
`units::quantity<int64_t>::operator*=`. The strict loader retains final volume
and marks the inventoried field implemented rather than inferring size from an
ID or display name.

Pinned finalization maps volume at inclusive upper thresholds: at most 7,500 ml
is tiny, 46,250 small, 108,000 medium, 483,750 large, and anything greater is
huge. The server performs that mapping when projecting content into a fresh
creature. The resulting closed `CreatureSizeV1` is private immutable live
state and is copied into a self-contained corpse prototype. Spawn/snapshot
validation rejects a live creature whose corpse size differs; snapshot restore,
revival, SQLite recovery, and portable replay never consult process-local
content. Runtime size-changing effects are not admitted yet, so base size is
the complete current `monster::get_size()` input. Public visible-creature DTOs
remain unchanged.

`Creature::deal_melee_attack` subtracts the target's `size_melee_penalty` after
its dodge roll. Protocol 72 therefore removes the final target-type and
medium-size rollout restrictions from the exact player hit path: every
canonical creature targeted empty-handed or with an admitted strict bash
weapon applies tiny/small/medium/large/huge penalties 30/15/0/-10/-20. The
same named session RNG and nonnegative hit boundary remain unchanged, so size
does not reroll an attack. Tests use one stream to prove a medium hit becomes a
tiny miss, cover every modifier and threshold, retain a nondefault size through
corpse revival and snapshot restore, prove CanonicalStateV48 participation,
and recover/replay a huge non-zombie miss. Other weapon profiles, runtime size
effects, criticals, techniques, and the larger awake-actor defense boundary
remain unavailable rather than approximated.

## Static monster immobility and ordinary action order

Protocol 73 advances to schema 51 and CanonicalStateV49 while retaining
CanonicalEventsV14. The pinned MONSTER `flags` inheritance already finalizes
`IMMOBILE`; fresh runtime projection now copies that capability into private
live-creature and self-contained corpse-prototype state. Spawn and snapshot
validation require the corpse value to match the live creature. Snapshot
restore, corpse revival, SQLite recovery, and portable replay therefore never
need a process-local content lookup, and the public visible-creature DTO remains
unchanged.

Pinned `monster::move` performs planning and special attacks before checking
`IMMOBILE`, then sets all remaining moves to zero before ordinary adjacent
melee, door opening, bashing, or movement. The current runtime has no admitted
monster special attacks, so an immobile creature performs its modeled
perception/goal and sound-lifetime bookkeeping once, spends its complete
accrued action-point balance, and produces no ordinary interaction. Clearing
the complete balance matters for high-speed or carried-over readiness: merely
subtracting the normal 100-move action cost would allow an impossible second
action in the same tick.

Pinned `Creature::deal_melee_attack` computes hit roll minus dodge roll minus
the target size penalty and then adds 40 when the target has `IMMOBILE` or the
dynamic `CANNOT_MOVE` flag. Protocol 73 applies that exact 40-point addition to
the already-admitted empty-hand and strict ordinary-bash player attack paths;
the named RNG stream is unchanged. Tests prove the same roll differs by exactly
40, a mobile miss becomes an immobile hit, excess readiness clears to zero
without an ordinary adjacent attack, and the private capability participates
in CanonicalStateV49, snapshot restore, corpse revival, schema-51 recovery,
and portable replay.

No selected pinned MONSTER definition statically carries `CANNOT_MOVE`; it is
a dynamic effect flag in the upstream engine. `RIDEABLE_MECH` also stops
ordinary movement but does not receive the 40-point target bonus. Dynamic
effects, rideable mechs, and monster special attacks remain separate later
boundaries instead of being folded into the static `IMMOBILE` boolean.

## Static monster pacifism and ordinary melee suppression

Protocol 74 advances to schema 52 and CanonicalStateV50 while retaining
CanonicalEventsV14. The strict MONSTER flag set already applies ordinary
inheritance, extension, and deletion to `PACIFIST`; fresh runtime projection
now copies that final capability into private live-creature and self-contained
corpse-prototype state. Spawn/snapshot validation requires both copies to
match, and snapshot restore, corpse revival, SQLite recovery, and portable
replay retain it without consulting process-local content. Public
visible-creature DTOs remain unchanged.

Pinned `monster::attack_at` returns false immediately for `PACIFIST` or dynamic
`CANNOT_ATTACK`. `PACIFIST` does not stop perception, pursuit, ordinary
movement, opening, bashing, or the earlier special-attack phase. Protocol 74
therefore gates only the existing adjacent ordinary `creature_attack` call.
When a pacifist reaches an occupied target tile, normal candidate selection
cannot move into that tile and the turn ends without damage; an otherwise
identical non-pacifist uses the existing exact ordinary attack path. A
differential test proves a pacifist first advances toward the actor, then causes
no adjacent damage while its cloned non-pacifist attacker hits.

Static pacifism participates in CanonicalStateV50, snapshot restore,
self-contained corpse revival, schema-52 SQLite recovery, and portable replay.
The selected pinned corpus contains inherited pacifists, including
`mon_grocerybot`; the classic zombie remains non-pacifist. Dynamic
`CANNOT_ATTACK`, special attacks, monster-versus-monster disposition, pushing,
and pacification effects remain separate future boundaries rather than being
approximated as static pacifism.

## Content-derived ordinary monster attack timing

Protocol 75 advances to schema 53 and CanonicalStateV51 while retaining
CanonicalEventsV14. Pinned MONSTER `attack_cost` defaults to 100 moves and is
an inherited bounded integer. The strict loader now applies the same direct,
relative, and C++-truncating proportional numeric-modifier precedence used by
the other finalized monster integers. The server validates every selected final value as
a nonzero `u16`; this admits the selected corpus exactly while rejecting a
zero-cost action that could prevent the real-time authoritative loop from
making progress. The classic zombie resolves to 100 moves and
`mon_skeleton_slasher` to its direct 70 moves.

The finalized value is private canonical live-creature state and is duplicated
in each self-contained corpse prototype. Spawn and snapshot validation require
the two copies to match and reject zero in either copy. Snapshot restore,
corpse revival, schema-53 SQLite recovery, and portable replay therefore do not
depend on process-local content, and public creature DTOs remain unchanged.

Pinned ordinary `monster::melee_attack` subtracts the type's `attack_cost`
after attempting the attack, including when the hit roll misses. Protocol 75
replaces the temporary fixed 100-move ordinary-melee charge with exactly
`attack_cost * 20` canonical action points. The existing signed-readiness loop
is deliberately retained: a 150-move attack from 100-move readiness produces
1,000 points of debt, while a 40-move attack can consume banked readiness on
multiple same-tick attacks. Tests lock both outcomes, exact miss charging,
hashing and snapshot rejection, corpse revival, selected-content projection,
SQLite recovery, and portable replay. Special-attack timing remains outside
this ordinary-melee slice.

## Canonical item-group graphs and authoritative generation

Protocol 80 advances to schema 58 and CanonicalStateV56 while retaining
CanonicalEventsV18. Item groups are content programs, not pre-expanded drop
lists. The selected-content loader therefore finalizes legacy and modern
collection/distribution syntax into ordered local graphs, applies reset and
self-copy extension in selected load order, resolves item migrations, retains
unsupported fields, and rejects missing references and cycles. A strict graph
contains one root plus the complete reachable named closure. Canonical worlds
persist only the sorted closure referenced by admitted consumers, never all
7,621 pinned definitions.

Protocol graphs use bounded local node IDs and explicit item, named-group, and
local-node targets. Validation checks sorted uniqueness, reachability, positive
normalized probabilities, count and charge ranges, direct item prototypes,
global/local cycles, depth 32, 512 definitions, 2,048 nodes, 8,192 entries, and
at most one 4,096-object stable-ID reservation per invocation. Collection
entries retain source order and consume a percentage roll even at probability
100. Distribution nodes consume one inclusive weighted ticket. Fixed counts
and charges consume no roll; ranges consume one inclusive roll at the pinned
point, and group counts repeat the complete child evaluation. Every nested
evaluation shares the caller-owned named RNG stream.

Structural bash is the first authoritative consumer. Terrain and furniture
definitions reference a named group or an inline implicit collection. On a
successful damage threshold, the server evaluates the complete output plan
without mutating world state, verifies a materialization position and enough
reserved stable IDs, then transforms the structure and creates objects in plan
order. A failed preflight leaves damage capped below destruction and burns no
ID. The pinned `t_wall` source uses `wall_bash_results`, whose strict maximum is
82, and resolves to the known starter-world `t_floor`. Two additional furniture
definitions become strictly representable, raising current admission to 539 of
699 while preserving replacement-closure checks. Direct, per-tick
snapshot, SQLite, and portable-replay scenario modes must produce identical
state and semantic events.

Ammo and magazine dressing, damage/on-drop modifiers, container nesting, and
charges applied to nested group targets are not represented by Protocol 80 and
must reject admission. This boundary prevents the generic interpreter from
claiming behavior that currently requires deeper item-content semantics.

## Canonical atomic mapgen discovery

Protocol 81 advances to schema 59 and CanonicalStateV57 while retaining
CanonicalEventsV18. The canonical world stores a bounded normalized worldgen
catalog rather than an opaque process-local loader or a single flat-terrain
prototype. It contains concrete terrain/furniture prototypes, regional
substitution tables, weighted OMT templates, exactly 576 cells per template,
and optional named item-group placements. IDs and tables are sorted and indexed
explicitly; validation bounds every aggregate and requires the exact reachable
item-group closure. A persisted world is therefore self-contained and does not
silently change when runtime content loading changes.

One CDDA overmap terrain cell owns exactly four canonical 12x12 submaps. The
server plans and commits the complete 2x2-submap/24x24 cell as one unit and
rejects any snapshot or live discovery that contains only some siblings.
Generation uses a named ChaCha8 stream derived from world seed, generator
version, OMT coordinates, z-level, and OMT identity—not simulation tick or
traversal order. Template selection, terrain glyph choices, furniture glyph
choices, item chance/group evaluation, and then tile-ordered regional terrain
and furniture resolution consume that stream in explicit phases. Template and
regional-table selection remain weighted calls and consume a draw even with
one candidate; fixed cell targets do not. A guaranteed 100-percent item
placement consumes no outer chance draw before its item group. Loot is
planned before mutation, checked for passable placement, bounded to one stable
ID reservation, and preflighted against the allocator before chunks or objects
are committed. Catalog admission also proves the worst-case output of every
selectable template across the maximum 36-OMT active-bubble discovery fits that
reservation, so an accepted world cannot become permanently ungeneratable at a
discovery boundary.

The strict content layer retains ordinary string/flat-array OMT roots, exact
24x24 Unicode display-cell rows (including base-plus-combining sequences),
positive variant weights, fixed/weighted terrain and
furniture glyphs, repeated static-palette expansion, default regional tables,
and one named item-group placement per glyph. Unsupported positive-weight
variants fail closed instead of being omitted. Runtime worldgen v1 additionally
rejects multi-layer glyphs, weighted one-time fill, recursive regional targets,
and one-entry weighted choices whose RNG phase cannot be retained. These
definitions remain available in the content report for the next semantic
version. Successful definitions and unavailable reports are shared across OMT
indices, and aggregate assignment limits bound expansion independently of the
raw-root limit.

Fresh servers currently repeat the real pinned `lmoe` surface mapgen and
resolve its `t_region_groundcover` pseudo terrain through the default region,
creating 36 complete OMTs/144 chunks in the initial active bubble. This is a
deliberate bootstrap, not an approximation of the upstream overmap: terrain
layout, start locations, nested/update mapgen, parameters, zones, specials,
populations, vehicles, monsters, and multiple z-levels remain unavailable. The
ordinary `field` definition is also unavailable to the server until its
`everyday_corpse` closure can preserve item damage and general container
nesting. A development-only pinned C++ oracle locks OMT matching and rotation,
point rotation, and static palette/nested phase order without entering the
shipped Rust runtime.

## Canonical start-location selection over explicit OMT identities

Protocol 82 advances to schema 60 and CanonicalStateV58 while retaining
CanonicalEventsV18. A canonical worldgen catalog no longer stores only a local
mapgen ID: it stores the overmap terrain's full ID, base type ID, linear/mapgen
subtype ID, and normalized local generator ID. This is the minimum immutable
identity needed to implement pinned `is_ot_match` behavior without consulting
process-local content after world creation. Exact compares the full ID; type and
subtype compare their explicit identities; prefix requires either a full match
or an underscore boundary; contains uses the full ID substring. The pinned C++
mapgen oracle characterizes all five modes, including rotated and linear cases.

The selected-content start-location registry is a strict semantic loader rather
than a list of hand-picked starts. It finalizes forward and self inheritance in
load order, retains source-ordered targets and string parameters, normalizes
negative interval maxima to the pinned unbounded value, applies flag
extension/deletion, and rejects unknown or excessive data. Definitions remain
available even when the runtime cannot yet execute their constraints. Runtime
normalization rejects any start requiring a city, excluding z=0, carrying
mapgen parameters, or carrying placement/preparation flags. The current server
therefore admits pinned `sloc_lmoe` and explicitly rejects shelter parameters,
boarded/allow-outside behavior, and city-constrained starts.

Target selection and actor placement are server-authoritative. A ChaCha8 stream
is derived from world seed, generator version, the next stable actor counter,
and start-location ID. It chooses one target in source order, then deterministically
shuffles matching generated OMT coordinates. While the bootstrap repeats one
identity everywhere, the origin OMT is moved to the front so new characters
remain beside the fixed playable starter loadout and encounter. The first free
passable tile in that order receives the character; subsequent matching OMTs
are tried if a cell is full. Trying more than the first matching OMT is the
multiplayer adaptation that keeps later joins possible without letting a client
choose a location. The stable actor counter makes fallback order repeat after
snapshot restore while keeping separate characters' choices independent.
Two-character conformance requires identical direct, per-tick snapshot,
SQLite, and portable-replay state and origin affinity.

Every current coordinate still uses the explicit pinned `lmoe_north` bootstrap
identity. This is not a generated overmap layout and must be replaced by
coordinate-owned identities before admitting heterogeneous terrain starts.
Within the selected OMT, current placement models passability and occupancy
only. Upstream inside/outside caches, reachable-area rating, start-point zones,
bashing/opening reachability, and NPC accommodation are not claimed; starts
requesting their explicit flags remain fail closed. These boundaries avoid
smuggling a hash-grid approximation into the later city/special/road/forest
overmap population engine.

## Bounded coordinate-owned overmap layout and identity routing

Protocol 83 advances to worldgen algorithm 2, schema 61, and
CanonicalStateV59 while retaining CanonicalEventsV18. The single repeated
`default_omt` field is replaced by a canonical 180x180 layout with explicit
origin, strictly z-sorted layers, full-ID-sorted identities, and canonical
row-major RLE runs. Every layer expands to exactly 32,400 cells, z=0 is
mandatory, every retained identity must be used, every generator must exist,
and coordinates outside the retained region fail closed. The fixed size follows
the pinned upstream overmap dimension; adjacent-overmap ownership is deferred
rather than inferred from a hash grid.

The selected-content OMT registry finalizes inheritance and load-order overlays
while retaining unsupported field names. It derives ordinary north/east/south/
west peers, the complete pinned 16-entry linear table, and nonrotating peers.
Each identity carries full, type, subtype, generator, and clockwise quarter-turn
rotation explicitly. Linear mapgen routing and inverse rotation follow the
pinned `om_lines` table and are checked by the real C++ oracle.

Local generation first resolves all source-phase template, terrain, furniture,
item-group, and regional choices from the coordinate-owned generator stream.
Only the completed terrain, furniture, and item placements rotate. This keeps
RNG consumption independent of orientation and applies one coordinate transform
to every generated layer. Start targets now filter the identities actually
owned by generated coordinates. Every start flag, city constraint, parameter,
and z=0 exclusion remains closed until its placement semantics exist.

Runtime admission requires the entire initial active bubble to fit inside the
layout and every possible start target to have a candidate in that durable
bubble. Character creation does not generate terrain because its persistence
transaction currently commits only the character spawn; remote-only starts
therefore fail closed until worldgen mutations join that transaction. Uniform
single-identity layouts keep origin affinity for the playable bootstrap, while
heterogeneous layouts retain the seeded shuffle. Movement that would prefetch
past the fixed boundary is rejected as blocked without aborting the tick.
Snapshot restore cross-validates every complete 2x2 OMT against an owned layout
coordinate and z-layer.

The production layout intentionally repeats `lmoe_north` inside the new
representation. Pinned regional settings identify `field` as the real z=0 base,
but its named loot closure reaches `everyday_corpse` entries with the general
`damage` modifier and later container behavior. Canonical items cannot yet
represent that family, so strict startup tests preserve the rejection rather
than deleting the rare group edge or its output. A heterogeneous synthetic
layout proves coordinate dispatch, shared terrain/furniture/item rotation,
matching-only authoritative character placement, snapshot stability, and
atomic out-of-layout failure; the shared scenario proves direct, per-tick
snapshot, SQLite, and portable-replay equivalence. Regional population replaces
the LMOE fill only after the complete field dependency closure is supported.

## Deterministic holiday-qualified item groups

Protocol 84 advances to schema 62 and CanonicalStateV60 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. The canonical
item-group graph stores every upstream holiday qualifier: New Year, Easter,
Independence Day, Halloween, Thanksgiving, and Christmas. Unknown values and
the internal `num_holiday` sentinel fail closed during selected-content load.

The persistent multiplayer server fixes CDDA's `EVENT_SPAWNS` setting to its
pinned default `off`. This is an explicit authoritative world rule, not a
locale-sensitive wall-clock query. A collection still consumes one probability
roll for each event-qualified entry and then emits nothing. A distribution
still includes inactive entries in its cumulative weight; selecting one emits
nothing instead of falling through. Those details match the pinned C++ oracle
and preserve all downstream RNG ordering.

Reading host time would make identical worlds diverge by timezone, date,
restart, and replay host, so it is forbidden in simulation. Future seasonal
content can be enabled only by adding an explicit persisted world policy and a
versioned deterministic active-holiday value. It must not silently inherit the
server process's current date.

## Milestone completion, module ownership, and version batching

The former `mapgen-overmaps` milestone is not a permanent umbrella. Atomic
static mapgen, OMT identities/routing, start selection, regional terrain,
cities, roads, rivers, specials, and general spawning each carry an independent
state in the checked parity ledger. “Complete” means the whole semantic family
has a pinned characterization (including exact traces at randomized
boundaries), a generalized Rust engine, direct Rust/C++ comparison,
direct/snapshot/SQLite/portable-replay conformance, runtime content admission,
and a normal authoritative client path when player interaction applies.
The first three runnable families use one reusable exact comparator rather than
separate hand-maintained expectations. The tool loads the pinned production OMT
registry, generates a real 24x24 Rust `WorldState` template, and compares its
matching, concrete identity routing, terrain/furniture placement, and rotation
directly with the C++ observation. The C++ side also generates the admitted
static template instead of reporting setup alone, and both sides compare an
exact 24-row trace so an incorrect engine cannot match only sampled tiles or
aggregate observations. Both sides also load the production `sloc_lmoe`
definition and compare its sole chosen target, constraints, runtime-admission
boundary, fixed candidate identities, matching subset, and selected candidate.
Only target/candidate semantics are upstream-equivalent; deterministic
occupied-tile fallback remains an explicit multiplayer adaptation. The shared heterogeneous
scenario supplies direct, per-tick snapshot, SQLite, and portable-replay
evidence; existing server character creation and Bevy exploration supply the
normal authoritative path. `atomic-static-mapgen`, `omt-identities-routing`,
and `start-location-selection` are therefore complete while the broader C++
oracle and regional-terrain milestones remain in progress.

Linear OMT witnesses use the concrete identity's mapgen rotation, not the
compass direction used to request that peer. At the pinned baseline both north
and south resolve to `road_ns` rotation 0, while east and west resolve to
`road_ew` rotation 3. The direct comparator caught the earlier direction-based
oracle expectation before milestone completion.

Large behavioral systems must not keep accumulating in the central crate files.
Items, actors, combat, activities, monsters, canonical state, protocol domains,
persistence responsibilities, and server sessions/replication have separate
mechanical extraction milestones. Each extraction preserves behavior and is
verified independently; anatomy and EOC expansion wait on their relevant
ownership boundaries. The extraction itself never changes protocol, schema,
replay, or canonical hash versions.

Serialized or wire changes are grouped into coherent semantic families where
practical. A version changes only when its representation changes, never as a
progress counter. Closely related item modifier and containment fields should be
batched in one increment after their full representation is designed, rather
than repeating migration, replay, fixture, and documentation work for isolated
fields.

## Explicit item-modifier presence and RNG phase

Protocol 85 advances to schema 63 and CanonicalStateV61 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Selected-content
normalization keeps the distinction between an absent item modifier and one
whose damage range is explicitly zero. It also retains signed charge ranges,
variants, direct and modifier-owned container IDs, group wrappers, sealing, and
overflow policy. Independent `-1` charge endpoints and the string `"null"`
container sentinel are preserved instead of coerced into ordinary unsigned
ranges or resolvable item IDs.

Only a fixed-zero raw-damage marker on a direct item is admitted to the
canonical graph in this increment. Every concrete leaf first consumes the
pinned per-item presentation-seed draw and empty-variant selection, followed by
the unconditional `one_in(3)` fit draw; variable-size items remain closed until
FIT state is canonical. The marker then evaluates the pinned modifier damage
range before charge dressing. Local collection/distribution nodes reject
the marker because the pinned loader never applies leaf modifiers to those
composites. Named-group markers also reject because the wire prototypes cannot
prove every returned item lacks degradation, gun-fault, or other unrepresented
modifier side effects. Modifier-bearing degrading vehicle parts and fouling
guns fail closed; exactly projected magazines and wells retain their two
zero-chance dressing draws. Corpse construction, preloaded magazines,
temperature-bearing comestibles, constructor-owned state, default containers,
nonzero damage, variants, explicit sealing, nested modifier groups, and all
wrapper/container materialization remain fail closed. This corrects the RNG
phase of an already admitted structural-bash definition without claiming new
runtime content.

The Postcard representation therefore changes even when the marker is absent,
so schema 63 is the minimum recoverable schema and the canonical state domain
advances. The checked item-flow fixture contains no item-group catalog and keeps
the same tick, actors, inventory, ground items, and CanonicalEventsV18 trace;
only its intentional state-hash domain changes. The item-state damage and
variant representation is the next version batch; general containment remains
a separate batch after the protocol item-group domain extraction.

## Exact item damage and immutable selected variants

Protocol 86 advances to schema 64 and CanonicalStateV62 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. It is one
coherent serialized batch: every canonical item and retained component now
stores exact raw damage plus its derived display level, and may store a
self-contained immutable selected variant. A recovered world therefore never
consults the live content tree to reconstruct a name, description, glyph,
color, or ASCII picture.

The selected-content ITEM importer finalizes source-ordered generic variants
through replacement, inheritance, `extend`, and `delete`. Missing or empty
alternate names, descriptions, and ASCII art fall back to the finalized base
ITEM before description append. Construction always consumes the pinned
variant-selection and FIT phases. Direct and named-group modifiers then apply
raw damage and an explicit variant after completed child generation, in child
output order; ranged charge selection precedes magazine dressing, and `<any>`
performs a second weighted selection without clearing a prior selection when
all weights are zero. Raw damage clamps to the exact zero-or-4000 leaf boundary.
A named modifier is admitted only when every reachable
leaf declares that its other modifier side effects are represented. The
protocol graph evaluator computes that property once through its existing
memoized closure, and simulation rechecks it defensively. Degrading vehicle
parts, fouling guns, unsupported variant fields or visibility policies, and
unrepresented constructor state remain fail-closed.

The C++ oracle retains exact constructor-variant witnesses and downstream RNG
values in addition to the existing representative corpse/container traces and
damage boundaries. Rust unit tests cover the generalized direct and named
modifier engines, and the structural-bash scenario retains exact raw damage and
variant metadata through direct execution, per-tick restore, SQLite recovery,
portable replay, and the normal Bevy item-menu path. Three additional
production furniture bashes (`f_cardboard_door_o`, `f_cardboard_roof`, and
`f_pallet_brick_adobe`) now normalize exactly. They do not yet earn weighted
runtime points because the current playable LMOE generator does not place
them.

Ordinary monster death also retains the exact raw overkill value produced by
the pinned float32 ratio, multiply, floor, and item clamp. The raw-1003
625-HP/251-overflow witness prevents an exact-rational substitute, and a
non-boundary ordinary death preserves raw/display condition through live state,
SQLite recovery, and portable replay.

The fixed item-flow scenario has no item-group catalog and keeps tick 80, the
same actors, inventory, ground items, and CanonicalEventsV18 trace. Its state
root changes from
`2aae0f859788b6e83bd4c03972f32a6a78963a63c1cedf5774b6b1e895e37820`
to
`8f8710e06937a50c14bcad35a17dbc41a059128061f4be9316c4c6449358dc66`,
isolating the new serialized defaults and CanonicalStateV62 domain. General
`contents-group` ownership, wrapper stable IDs, and overflow remain the next
regional-field dependency and require their own representation batch.

## Canonical detachable tool-charge storage

Protocol 88 advances to schema 66 and CanonicalStateV64 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Normalization
resolves every admitted tool-charge modifier into one explicit storage plan:
either the existing single integral magazine or one single detachable well,
its pinned default magazine prototype, and that magazine's pinned default
ammunition prototype. Simulation never guesses content defaults.

The generalized planner follows the characterized `Item_modifier::charges`
boundaries. A detachable tool receives its default magazine even for an exact
zero request; positive requests create nested ammunition and clamp to the
magazine pocket capacity. The tool, magazine, and ammunition receive recursive
preorder stable IDs and use the same direct, snapshot, SQLite, and portable
replay path as other item-group containment. Requests 0, 1, 56, and 100 are
retained as exact C++ traces and directly compared with the Rust engine. If an
outer named-group modifier applies charges again, both engines retain the
already-installed magazine and replace only its ammunition. The production
`accesories_personal_unisex_child` seed-235 trace pins the final one-charge
battery and downstream draw, while the Rust phase test proves no second
magazine-constructor draws occur.

Magazine-well rigidity belongs to the canonical well prototype and snapshot,
not to an item-group-only descriptor. Rigid wells exclude installed-magazine
volume from recursive wrapper fit; non-rigid wells include the complete nested
magazine volume. The production content gate also proves that modeling this
field moves exactly 13 pinned disassembly targets from an empty-charge guard to
general detachable storage while keeping the total admitted surface fixed.

The serialized well and item-group descriptor shapes require the protocol and
schema change. The representative fixed snapshot bytes still reproduce the
old CanonicalStateV63 root under the V63 domain, proving its only fixture change
is the intentional V64 domain. The next retained regional-field edge is
`saint_necklace` variant description snippet expansion; real field generation
and runtime progress credit remain closed until that full closure and a normal
client exploration/loot path are green.

## Recursive item-description expansion

Protocol 89 advances to schema 67 and CanonicalStateV65 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Base ITEM and
variant `expand_snippets` definitions normalize into a self-contained template
plus the exact reachable weighted category closure. This representation is
stored with the authoritative item-group catalog rather than consulting live
content during simulation or recovery.

The selected English registry loads `data/names/en.json` in the pinned
pre-snippet phase, maps gendered and unisex entries into the eight upstream
name categories, and then loads ordinary snippet JSON in selected source
order. Identified choices precede anonymous choices, overrides clear only their
category, zero-weight choices remain represented, and duplicate global IDs,
weight overflow, cycles, excessive depth, excessive closure size, oversized
output, and unavailable item-variable capacity fail closed. Maximum-length
validation memoizes category/depth results so a bounded repeated DAG cannot
create exponential validation work. Non-English catalogs and runtime language
switching remain out of scope.

Construction preserves the pinned phase order even when intermediate text is
overwritten. Selecting an expanding variant immediately expands it; the later
constructor phase expands the base and then the selected variant again.
Explicit item-group variant modifiers expand once more. Recognized categories
consume one canonical draw even when they have one choice or zero total
weight; genuinely unknown tags stay literal without consuming the stream. The
generated `description` variable replaces an existing value or consumes one
of the bounded variable slots before materialization.

The pinned item-group oracle records the exact recursive/literal boundary and
the production seed-59 `accessory_necklace`/`saint_necklace` result plus its
downstream draw. Complementary Rust phase tests use a multi-choice variant so
omitting the initial overwritten expansion changes both the final choice and
the shared stream. Production normalization also proves the seven-category
`dog_tag_id` name closure. Direct, per-tick snapshot, SQLite, and portable
replay execution preserve the resulting canonical description, and the
ordinary Bevy item menu renders only the replicated value.

This closes the description-expansion edge without earning runtime progress:
the real field is still neither generated nor traversable as ordinary
gameplay. Its next exact fail-closed boundary is variable-size `FIT` state on
`leg_sheath6`; that generalized constructor family must complete before the
field/client exploration-and-loot unlock.

## Variable-size FIT is canonical item-instance state

Protocol 90 advances to schema 68 and CanonicalStateV66 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. `VARSIZE` is an
immutable finalized type capability; `FIT` is a per-instance boolean owned by
the authoritative item. A fitted snapshot without `VARSIZE` or an explicit
`FIT` capability is invalid. The state remains self-contained across nested
ownership, component provenance, SQLite recovery, and portable replay.

The generalized item-group constructor preserves upstream phase ownership.
Every direct item leaf consumes one one-in-three FIT draw, including
non-variable controls. The result changes state only for `VARSIZE` items, and
an already-fitted item remains fitted. A named group delegates to its selected
leaf and does not add another phase. Raw direct wrappers likewise do not add a
phase, while a modifier-created container follows the ordinary item constructor
path. These rules live in `sim/items.rs`; the central simulation file only
coordinates crafting and disassembly ownership boundaries.

Crafted variable-size primary outputs and byproducts are always fitted.
Disassembly preserves exact retained component state; when it reconstructs a
default component, a fitted target transfers FIT only to a variable-size
output. The client derives the upstream `(poor fit)` suffix solely from the
replicated immutable capability and canonical boolean. It never rerolls or
consults live item content for this state.

The pinned oracle retains representative direct and production traces instead
of aggregate outcomes: seeds 1 and 2 cover unfitted/fitted `leg_sheath6`, seed
2 supplies the same successful draw to a non-variable control, and production
seeds 219 and 97 cover both states through `accessory_weaponcarry`. Each trace
also fixes the rendered name and downstream draw. A direct Rust projection
executes the reusable transition, and shared direct/snapshot/SQLite/replay
conformance plus the normal client menu cover the multiplayer path.

This removes `accessory_weaponcarry` from the real `field` closure. The field
is still fail-closed: its next deterministic boundary is ammunition loading for
`ammo_light_batteries`, followed by separate retained families such as default
containers, food temperature, corpse construction, and generalized wrapper
shapes. Those must be completed as coherent engines; FIT does not earn runtime
progress until the real field is generated, explored, looted, persisted, and
client-accessible.

## Generalized item-group ammunition loading without representation churn

The ammunition-loading increment retains Protocol 90, schema 68,
CanonicalStateV66, worldgen algorithm 2, replay format 3, and CanonicalEventsV18.
The existing integral/detachable `ItemGroupToolChargeStorageV1` Postcard shape
already describes storage independently of the owner subtype; its historical
name is source terminology, not a tool-only invariant. Reusing it changes
neither wire nor persisted representation, so a version bump would be ceremony
rather than a schema change.

Server normalization now resolves that descriptor for strict magazines and
tools. A strict single integral magazine loads
its registry-default ammunition, preserves an exact zero request as empty, and
clamps a positive request to pocket capacity. Integral tools use the same
engine. A detachable tool retains the earlier exact default-magazine path.
Every gun remains fail closed: pinned integral guns retain owner-local charges,
while detachable guns route through `item::ammo_set`; both have state ownership
and constructor RNG semantics distinct from the magazine/tool planner.
Multi-well, missing-default, incompatible-ammunition, constructor-state, and
signed capacity-sentinel cases also remain fail closed.

The pinned C++ oracle retains five direct boundary traces: zero, one, exact
capacity, overflow for `light_battery_cell`, and overflow for the two-charge
`light_minus_battery_cell`. It also retains production
`ammo_light_batteries` witnesses at seeds 378, 19, 1, and 4 for empty, partial,
clamped-full, and alternate-magazine results. Every trace includes item type,
ammunition type/count, remaining capacity, and the exact downstream RNG draw.
The reusable Rust projection calls the production constructor and charge
planner rather than reimplementing the clamp. The shared named-item-group
scenario preserves the generated magazine and nested ammunition through direct,
per-tick snapshot, SQLite, and portable replay execution; the ordinary Bevy
item menu displays the replicated integral charge count.

Production normalization now admits `ammo_light_batteries`. This earns no
weighted runtime points because the real `field` is not yet generated or
playable. Its next audited fail-closed boundary is default-container ownership:
`bottle_otc_painkiller_1_20` reaches `aspirin`, whose finalized default
containment is retained but not yet materialized. That containment family must
be completed before food temperature, corpse construction, or later field
dependencies are expanded.

## Default-container ownership completes one serialized containment family

Protocol 91 advances to schema 69 and CanonicalStateV67 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. This is the one
final representation checkpoint for wrapper ownership, contents groups,
sealing, overflow, snippets, variables, and item-type default containers. Each
normalized item prototype now carries an optional self-contained default
container descriptor, and each modifier records whether it owns the fallback
phase. The literal upstream `null` sentinel remains meaningful during content
normalization but becomes an explicit modifier-without-fallback shape; it is
not confused with an omitted modifier.

Construction follows the pinned ownership split. A direct item invokes its
type default through the raw item constructor, fills liquid or count-by-charge
payloads to physical capacity, leaves an uninsertable payload raw, and seals
only a full container. A modifier first applies item state, then either creates
its explicit container (including that creator's own default-container phase),
uses the target type's fallback, or suppresses fallback through explicit
`null`. Whole-group and entry wrappers remain raw constructors and never borrow
the modifier creator phase. Physical contents insert at the front, preserving
the exact ordered ownership observed upstream. Recursive default descriptors
are depth- and cycle-bounded. Named-group modifiers whose possible generated
types have different default-container behavior remain represented but fail
closed until their aggregate wrapper closure is explicit.

The pinned C++ oracle adds seven exact traces: direct water and aspirin,
modifier fallback, explicit-null suppression, an explicit ibuprofen creator
whose aspirin container first becomes a pill bottle, and production
`bottle_otc_painkiller_1_20` boundaries of one and twenty aspirin. The explicit
creator fixes ordered children `[ibuprofen, aspirin]` and downstream draw 8323;
the production boundary seeds and all other downstream draws are retained in
the checked corpus. The complete item-group kernel passes 104 assertions, then
the reusable comparator executes the production Rust planner for the same
traces. Protocol bounds count the creator's complete subtree, and raw wrapper
validation remains distinct from creator validation.

The shared authoritative scenario generates a pill bottle owning aspirin with
preorder stable IDs and preserves it through direct, per-tick snapshot, SQLite,
and portable replay execution. The normal Bevy item menu displays the nested
count and issues removal by authoritative owner, pocket, and child ID. The
representative empty-catalog fixture retains identical Postcard bytes: the V66
domain reproduces
`7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`,
while V67 deliberately produces
`b5c12b763060907d68bfbd96b4aea6372c17cb02676b5e499b0bc79f5679899e`.
Serialized catalogs containing item prototypes do change shape, so Protocol 91
and schema 69 are required; replay format and event representation do not
change.

This admits `bottle_otc_painkiller_1_20` but earns no weighted progress before
the real field is generated and playable. The complete field scan now fails
closed at `chaw` because general comestible temperature state is not yet
represented. That semantic family, followed by the remaining field closure and
ordinary client exploration/loot demonstration, is the next dependency
boundary.

## Materialless item temperature is the Protocol 92 boundary

Protocol 92 advances to schema 70 and CanonicalStateV68 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. Temperature is
owned by each item instance, including provenance components, integral
ammunition, installed magazines, and physical container contents. The state is
integer-only: millikelvin, optional millijoules per gram, last-check tick,
phase, and HOT/COLD/FROZEN flags. A future last-check tick is invalid during
world recovery.

The pinned constructor creates every temperature-tracked comestible active at
its birth tick with 0 K, -10 J/g sentinel energy, its finalized phase, and no
temperature flags. A four-case exact C++ trace fixes materialless `chaw`,
material-backed `water_clean`, NO_TEMP `caffeine`, and ordinary `rock`,
including processing cadence and serialized `last_temp_check`. The reusable
Rust comparator executes the same constructor projection directly. At the
first ten-minute boundary, the currently admitted materialless/nonperishable
class initializes to deterministic normal ambient 293.150 K. Upstream's
materialless coefficient calculation is indeterminate after initialization;
Rust represents that result as absent specific energy instead of persisting a
platform-dependent NaN payload.

Exactly 36 selected core definitions satisfy this bounded constructor class.
Material-backed thermodynamics, `spoils_in` rot, custom freezing points,
weather-driven ambient changes, and gas phases remain explicit fail-closed
classes. The shared finalized-content classifier is used by server catalog
normalization and client disassembly eligibility. This closes a pre-existing
unsound admission: 420 crafting recipes and 66 disassembly recipes that need
those later engines are now retained in content but excluded from runtime.
The crafting audit is 208 material, 182 rot, and 28 custom-freezing results,
plus one rot and one custom-freezing byproduct.

The authoritative item-group scenario retains a temperature-tracked item
inside its default container through direct, per-tick snapshot, SQLite, and
portable replay execution. The Bevy item menu distinguishes constructor-pending
state from initialized 20.0 °C state and will not merge stacks whose temperature
state differs. Because the real `field` group is still blocked, no runtime
progress points are awarded. Its exact next boundary is now the flexible
`chaw_wrapper_1_20` wrapper, which requires a general non-rigid physical
container engine before the ordinary field exploration/loot loop can ship.

Protocol 92 changes representative Postcard bytes even when temperature is
absent. Hashing those new bytes under the old V67 domain yields
`d0b9e7a84fbdb6ef8a751d3536bfb57a8cd092f17d379ea7a960c14ede43f187`;
the V68 domain yields
`ecf2ff2770054b46562dd7cad15c3aa9326586594374b2710af84754beef6a6a`.
The prior V67 fixture remains documented as historical evidence rather than
being mechanically overwritten.

## Flexible containment closes at Protocol 93

Protocol 93 advances to schema 71 and CanonicalStateV69 while retaining
worldgen algorithm 2, replay format 3, and CanonicalEventsV18. This is the
single final representation checkpoint for the currently supported containment
family: wrapper ownership, contents-item and contents-group sources, sealing,
spill/discard overflow, snippets, variables, default containers, flexible
physical pockets, reserved base volume, and collapsed presentation state.

A physical pocket now retains the volume already included in its owner as a
`magazine_well` reserve. Rigid and E-file pockets require a zero reserve;
flexible external volume is `max(contents - reserve, 0)`. Insertion and wrapper
planning share that one checked helper. Runtime state stores both the
constructor default and the actual collapsed setting. Construction applies the
default first, then reproduces pinned `add_automatic_whitelist`: an item with
exactly one physical pocket becomes collapsed after a nonempty homogeneous
fill. Rust deliberately accepts only exact equivalent planned stacks for that
automatic transition, keeping broader unrepresented `stacks_with` cases fail
closed.

The item-group oracle records exact one- and twenty-item paper-wrapper traces,
the production chewing-gum blister pack, and actual collapse on all seven
default-container traces. The paper wrapper proves the important split between
an open constructor default and collapsed post-fill state; chewing gum proves
the explicit `COLLAPSE_CONTENTS` default. Together with the existing overflow
and ownership witnesses, 137 C++ assertions and the direct production Rust
projection cover the generalized engine rather than aggregate counts alone.

Direct, per-tick snapshot, SQLite, and portable replay conformance retain a
mixed flexible sealed wrapper, 45 ml reserve, nested auto-collapsed painkiller
bottle, stable preorder IDs, and temperature state. The normal Bevy item menu
renders collapsed pockets and retains server-authoritative removal. Selected
runtime admission gains `chaw_wrapper_1_20`, `chewing_gum_full`, and exactly six
furniture bashes: `f_earthbag_half`, `f_earthbag_wall`, `f_exodii_charger`,
`f_exodii_pump`, `f_pillow_fort`, and `f_string_dimension_pump`. The complete
field scan remains fail closed at `chewing_gum_full_caff` because material
thermodynamics for `caff_gum` is not represented. No weighted runtime credit is
awarded until that real field is generated, explored, looted, persisted, and
client-accessible.

The Postcard bytes for the representative item-flow fixture are unchanged:
the old V68 domain still yields
`ecf2ff2770054b46562dd7cad15c3aa9326586594374b2710af84754beef6a6a`.
Only the deliberate V69 domain changes that fixture to
`5f662ff59bc4c66b4c7e0700fdb0838bf41bac385a513458531d5af255bc5456`.
