# Agent Prompt: Complete the CDDA Rust Multiplayer Port

You are the primary implementation agent responsible for completely
implementing a persistent multiplayer Rust port of Cataclysm: Dark Days Ahead
(CDDA). Work autonomously and persistently until the completion gate in this
prompt is satisfied.

This is an implementation assignment, not a request for an architecture essay
or a one-time prototype. Planning, investigation, documentation, and prototypes
are means to the end. Do not stop after scaffolding, a proof of concept, one
vertical slice, or a list of work that somebody else must finish.

## Mission

Create a production-quality Rust port of the gameplay and content of the pinned
CDDA reference version, redesigned around:

- A Bevy graphical client
- A minimal headless Bevy server
- A persistent server-authoritative multiplayer world
- Real-time simultaneous play for up to 16 connected players initially
- Durable characters that remain physically present and vulnerable after their
  players disconnect
- A world clock that continues while no players are connected
- Native Windows, macOS, and Linux clients
- Native headless macOS and Linux dedicated servers

Preserve all in-scope observable CDDA mechanics and content. Apply only the
locked multiplayer adaptations and the minimum-change adaptation rule in this
prompt. Reimplement behavior cleanly in Rust; do not mechanically transliterate
C++ classes or reproduce accidental legacy architecture.

## Workspace and authoritative inputs

At the beginning of every fresh or resumed work session:

1. Read every applicable `AGENTS.md` completely.
2. Read `ARCHITECTURE_DECISIONS.md` completely.
3. Read the current implementation status, parity matrix, and recent decision
   log described below.
4. Inspect the working tree, including staged, unstaged, and untracked files.
5. Continue existing work rather than recreating or overwriting it.

Use these workspace locations:

- The repository root containing this prompt is the Rust implementation root.
- `../Cataclysm-DDA/` is the expected upstream reference checkout. If the
  workspace is laid out differently, discover and record the equivalent path
  rather than cloning a duplicate blindly.
- `ARCHITECTURE_DECISIONS.md` at the repository root is the living
  architectural authority.

Treat the upstream reference checkout as read-only. Do not mix port changes
into it. It may be built, tested, instrumented, and inspected to establish
behavioral baselines.

The fixed upstream parity target is commit
`4dfd36038b16650dc1b5cb9d79a3e42363174b05`. Verify that the reference
checkout is at that commit and record it in project metadata. Do not move the
baseline until the completion gate is satisfied.

When instructions disagree, use this precedence:

1. System, developer, and applicable `AGENTS.md` instructions
2. Explicit current user instructions
3. `ARCHITECTURE_DECISIONS.md`
4. This implementation prompt
5. Existing project documentation and plans
6. Reasonable implementation judgment

## Locked product and architecture decisions

Implement these decisions as written:

1. This is a derivative multiplayer Rust port of CDDA, not an original game
   merely inspired by it.
2. The port is semantic, not a line-by-line C++ translation.
3. The client uses Bevy.
4. The authoritative server uses minimal/headless `bevy_app` and `bevy_ecs`
   without renderer dependencies.
5. The simulation uses a hybrid ECS design.
6. Dense terrain and spatial data use chunked world storage; tiles are not ECS
   entities.
7. Persistent domain objects use explicit typed stable IDs. Bevy `Entity`
   values never serve as save-file or wire identity.
8. Clients send commands and intentions. Only the server validates and mutates
   canonical gameplay state.
9. The protocol uses explicit commands, events, snapshots, and relevance-
   filtered deltas. It does not mirror arbitrary Bevy component mutations.
10. Tests and replay execution do not require rendering.
11. The persistent world targets 16 simultaneously connected players initially.
12. Disconnected player characters remain present, simulated, and vulnerable.
13. The global world clock advances with zero connected players.
14. Simulation time, network send rate, and client rendering time are separate
    concepts.
15. Windows, macOS, and Linux are the initial client platforms. Headless macOS
    and Linux dedicated servers are required.
16. Simulation runs at 20 Hz, sends network state at 10 Hz, and maps one
    simulation second to one wall-clock second.
17. Players cannot pause or accelerate time. Only the administrative
    maintenance procedure can freeze the world.
18. Disconnected characters use the survival autopilot specified below.
19. Map chunks are 12 by 12 tile CDDA submaps with the fixed Active, Warm, and
    Dormant tier rules below.
20. Networking uses Lightyear 0.28.0 over UDP/Netcode with explicit domain
    messages and bincode encoding.
21. Persistence uses bundled SQLite through `rusqlite` 0.40.1 with WAL,
    versioned Postcard records, and Zstandard compression.
22. Canonical simulation is bit-for-bit deterministic across the supported
    platforms, uses fixed-point/integer arithmetic, and uses named ChaCha8 RNG
    streams derived with BLAKE3.
23. Content scope, authentication, command queues, prediction, reconciliation,
    backup retention, and performance budgets are exactly those specified in
    this prompt and `ARCHITECTURE_DECISIONS.md`.

Do not replace a locked choice with an alternative library or product policy.
Solve implementation problems behind the prescribed interfaces and record
non-product engineering details in ADRs.

## Architecture

Use this Cargo workspace structure:

```text
./
  Cargo.toml
  crates/
    sim/          authoritative mechanics, time, and world state
    content/      CDDA data loading, validation, and migrations
    protocol/     stable IDs, commands, events, snapshots, and wire types
    persistence/  chunk/entity storage, journaling, backups, and migrations
    server/       sessions, auth, networking, interest, and simulation runner
    client/       Bevy rendering, UI, input, audio, and prediction
    tools/        importers, validators, replay tools, benchmarks, and admin CLI
  tests/
  docs/
```

Maintain strict dependency direction:

- `sim` cannot depend on `client`, rendering, windowing, or network transport.
- `protocol` must work in client, server, headless tests, and replay tools.
- `persistence` serializes versioned domain records rather than the raw Bevy
  `World`.
- `server` owns time, identity allocation, randomness, validation, persistence,
  interest management, and canonical outcomes.
- `client` translates local input into commands and authoritative events into
  presentation.
- Lightyear and SQLite sit behind project-owned interfaces so domain logic does
  not depend on transport or database APIs.

The client and headless server must both build and run on the macOS development
host from the first vertical slice. Required standalone server targets are
`aarch64-apple-darwin`, `x86_64-apple-darwin`, and
`x86_64-unknown-linux-gnu`. Windows server support is outside the initial gate.

### Toolchain and foundational crates

Use exact `=` version constraints for these direct dependencies and commit the
resulting `Cargo.lock`:

| Area | Required implementation |
| --- | --- |
| Rust | Rust 1.97.1 in `rust-toolchain.toml`, edition 2024 |
| Client | Bevy 0.19.0 with only the required 2D, UI, audio, asset, input, accessibility, and native-platform features |
| Server | `bevy_app`, `bevy_ecs`, `bevy_time`, and `bevy_tasks` 0.19.0 without rendering, window, or audio features |
| Multiplayer | Lightyear 0.28.0 over native UDP with Netcode |
| Wire encoding | Registered Serde messages, Lightyear bincode serialization, protocol version 1 |
| Database | Bundled SQLite through `rusqlite` 0.40.1 |
| Durable blobs | `postcard` 1.1.3 plus `zstd` 0.13.3 |
| Control API | Tokio 1.53.1, Axum 0.8.9, `axum-server` 0.8.0, and rustls 0.23.42 HTTPS |
| Passwords | RustCrypto `argon2` 0.5.3 using Argon2id |
| Randomness and hashing | `rand_chacha` 0.10.0 `ChaCha8Rng` and BLAKE3 1.8.5 |
| Observability | `tracing` 0.1.44, `tracing-subscriber` 0.3.23, and a Prometheus-format metrics endpoint |

### Hybrid ECS rules

Use ECS for independently acting objects that participate in multiple systems.
Use domain aggregates and dense collections when atomic invariants, spatial
layout, or containment dominate.

Required representations:

| Concept | Representation |
| --- | --- |
| Players, NPCs, monsters | ECS entities with coarse domain components |
| Actor health, position, movement, faction | ECS components |
| Status effects | Domain collection component |
| Vehicles | ECS entity containing a vehicle aggregate |
| Vehicle parts | Dense vehicle-owned collection |
| Terrain, furniture, traps, fields | Chunk-owned dense/sparse data |
| Items and pockets | Stable `ItemId` records and containment graph |
| Ground item piles | Chunk-local stable item references |
| Recipes and type definitions | Immutable validated registries |
| Activities | Explicit interruptible state machines |
| Projectiles and active explosions | Short-lived ECS entities |
| Sprites and UI | Client-only presentation entities |

Do not create an entity for every tile, nested item part, recipe, or static
definition simply to maximize ECS usage.

### Stable identity

Use typed 128-bit IDs including `WorldId`, `ActorId`, `ItemId`, `VehicleId`,
`MissionId`, and `EventId`. The upper 64 bits are the persistent random world
namespace and the lower 64 bits are a monotonically allocated counter. The
world namespace is generated with the operating system CSPRNG. The server
allocator reserves blocks of 4,096 counters by advancing the persisted
high-water mark in a dedicated SQLite transaction before issuing any ID from a
block. Unused or rolled-back IDs are skipped permanently.

Bevy `Entity` values, memory addresses, array positions, and database row IDs
are never durable or wire identities. Maintain runtime maps between stable IDs
and ECS entities. Never reuse an ID after deletion, rollback, or a crash.

### Time and scheduling

Use `SimTick(u64)` and `SimDuration` at 20 Hz; one tick is 50 milliseconds and
one simulation second is one wall-clock second. Send network state at 10 Hz.
Rendering is variable-rate and interpolated. Headless tests and replays drive
the same clock without sleeping.

Every tick runs these ordered phases:

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

Use a stable priority queue keyed by `(due_tick, event_sequence)` for scheduled
work. Order same-tick conflicts by phase, actor readiness, stable `ActorId`, and
command sequence.

Do not depend on incidental Bevy parallel-system ordering. Order conflicting
phases explicitly, and parallelize only computations whose result is invariant
to execution order.

Players cannot pause or accelerate the world. Menus leave the simulation
running; an actor finishes its current action and guards in place. Crafting,
reading, construction, healing, sleep, and travel take their real simulation
durations and continue after disconnect. Threats interrupt them according to
CDDA rules.

Administrative maintenance pause stops command intake, checkpoints SQLite,
disconnects clients, and then freezes `SimTick`. Planned maintenance does not
advance time. Unexpected process downtime does: use the persisted UTC anchor to
perform deterministic catch-up before accepting clients.

### Region activation

Use 12 by 12 tile, single-z-level CDDA submaps as atomic map chunks.

- **Active:** full 20 Hz simulation in an 11 by 11 submap square centered on
  each connected player, on the current and adjacent z-levels, and in chunks
  containing combat, moving vehicles, projectiles, explosions, or immediate
  hazards.
- **Warm:** 1 Hz coarse spatial simulation in the same-size bubble around each
  disconnected character, NPC camp, persistent fire, or live anchor. Hostile
  contact or a complex hazard promotes affected chunks to Active until resolved.
- **Dormant:** unloaded state represented by scheduled events and
  `last_sim_tick`; deterministic catch-up processes needs, rot, plants, weather,
  power, fields, and elapsed-time systems before activation.

Network prefetch uses a 13 by 13 submap square. Merge overlaps and apply the
highest tier. Tier changes occur only at tick boundaries.

### Disconnected-character autopilot

On disconnect, clear held movement and steering input at the next tick. Continue
the current activity; normal danger rules interrupt it only when its activity
definition permits interruption. Enable survival autopilot when the activity
completes or is interrupted. It may defend, flee nearby threats, extinguish
fire, leave dangerous terrain or atmosphere, seek shelter inside the current
reality bubble, use ordinary food and medicine, use wielded equipment, and sleep
when safe.

It does not initiate combat, loot, leave the current bubble, begin projects,
spend unique resources, make dialogue/faction choices, or change equipment
loadouts. Reconnection returns control at the next tick without moving,
healing, canceling, or protecting the character.

### Commands and client prediction

Each actor has one active action and at most two queued semantic commands.
Canceling queued work is free. Active work is cancelable only when its activity
definition is interruptible; elapsed time and already-paid costs remain.
Invalid commands return a typed rejection and are removed. Held movement and
vehicle steering are stateful inputs rather than queued commands.

Predict only the controlled actor's locomotion, vehicle steering, camera, and
cosmetic effects. Never predict inventory, combat, projectiles, RNG outcomes,
item use, crafting, dialogue, or world interaction. Interpolate remote actors
100 milliseconds behind authoritative time. Smooth valid corrections of at
most one tile over 150 milliseconds; snap larger or collision-invalid
divergence and emit a diagnostic event.

### Networking

Use Lightyear 0.28.0 with Bevy 0.19.0, native UDP, the Netcode connection layer,
registered Serde messages, Lightyear's bincode serializer, and protocol version
1. Do not automatically replicate the canonical Bevy world.

Use these channels:

- Redundant unreliable-sequenced input for held movement and vehicle steering
- Reliable-ordered semantic commands, results, chat, and administration
- Reliable-ordered entity lifecycle and critical domain events
- Reliable-unordered fragmented chunk snapshots and content manifests
- Unreliable-sequenced actor and vehicle state deltas

Every payload uses fixed-width domain numbers, typed stable IDs, bounded
collections, explicit versions, and server-side authorization. Implement an
in-process Lightyear transport for integration tests and simulate latency,
jitter, loss, duplication, and reordering.

The rustls HTTPS control API uses Tokio, Axum, and `axum-server` for login,
character selection, and issuance of 30-second Netcode connection tokens.
Production requires a CA-valid certificate; local development generates and
pins a local certificate. Plaintext remote authentication is prohibited.

Account names match `[a-z0-9_]{3,32}`. Passwords are 12 through 256 bytes and
are stored as Argon2id PHC strings using 64 MiB, three iterations, four lanes, a
random 16-byte salt, and a 32-byte output. Public registration is disabled;
administrators create accounts or one-use invites with the CLI. Limit failed
authentication to five attempts per account and source address per 15 minutes.
Generate salts, invitation tokens, session tokens, and Netcode private keys with
the operating system CSPRNG.

Successful login returns an opaque random 256-bit token whose BLAKE3 hash is
stored. It expires after 24 hours and is revoked by password changes or admin
action. One-use invitation tokens use the same size, storage, and 24-hour
expiration policy. Roles are player, moderator, and administrator. Exactly one
gameplay connection per account and per character is active at a time; only the
same reconnect session or an administrator may replace or transfer it.

### Deployment and content handshake

Run one authoritative server runtime and one SQLite database per persistent
world. Do not shard or federate a world. Dedicated operators expose one HTTPS
control endpoint and one UDP gameplay endpoint. Accounts and roles are local to
that server world. Ship native headless server binaries for Apple silicon and
Intel macOS 13 or newer and for x86-64 GNU/Linux with glibc 2.35 or newer.

Clients connect directly by hostname or IP address and can store favorites.
There is no central account service, matchmaking service, public server
directory, relay, or automatic NAT traversal. Local play embeds the same
headless server crates in the client process. Single-player uses the in-process
transport only; locally hosted multiplayer also exposes the HTTPS and UDP
endpoints for remote clients. The host's client-facing command boundary remains
identical and non-authoritative.

The connection handshake compares protocol version, baseline commit,
content-manifest hash, and ordered enabled-mod list before character selection.
Reject a mismatch with all differing values in the diagnostic response. Do not
download or execute content from a server automatically. Third-party mods are
outside the pinned completion scope; an operator using one installs the exact
same files manually on server and clients, and those files participate in the
manifest hash. Multiplayer communication includes server-routed text chat;
voice chat is out of scope.

### Persistence

Use bundled SQLite through `rusqlite` 0.40.1 with `journal_mode=WAL`,
`synchronous=FULL`, foreign keys, and one persistence worker. Use transactional,
numbered, forward-only SQL migrations after an automatic backup; do not support
downgrades.

Commit accepted commands and authoritative events to an append-only journal
every network frame (100 milliseconds) before acknowledging durable outcomes.
Write dirty entity and submap snapshots every five seconds in atomic batches.
Recover by loading the latest snapshots and replaying later journal entries.
Checkpoint WAL every 60 seconds and at graceful shutdown.

Roll compressed replay archives hourly and retain 30 days. After a verified
snapshot and replay-archive write, daily compaction removes recovery-journal
rows older than that snapshot; SQLite reuses the freed pages. This preserves
crash recovery and recent desync diagnosis without unbounded journal growth.
Compaction never removes a snapshot object referenced by a retained replay.

Encode versioned domain blobs with Postcard 1.1.3 and compress them with
`zstd` 0.13.3. Never serialize raw Bevy worlds. Create verified online backups
hourly; retain 24 hourly and 30 daily generations. Include the database,
content-manifest hash, baseline commit, schema/protocol versions, and BLAKE3
checksums. Restore verifies integrity and replays the journal before opening.

### Determinism and replay

Canonical simulation uses integer or fixed-point arithmetic. Floating-point
values are presentation-only. Any collection iteration that affects outcomes is
ordered by stable ID; hash-map iteration order must never affect simulation.

Each world stores a 256-bit seed. Derive named ChaCha8 streams with BLAKE3 from
the world seed, a domain tag, relevant stable IDs, tick, and event sequence.
Use separate domains for combat, map generation, weather, loot, and AI so a new
random draw in one domain cannot perturb another.

The replay format is a versioned Postcard stream compressed with Zstandard.
Its header contains the baseline commit, content-manifest hash, protocol and
schema versions, world namespace, seed, initial snapshot hash, and start tick.
Records contain accepted commands, administrative world events,
connection-control changes that affect actors, and periodic BLAKE3 canonical
state hashes recorded every 100 ticks. The same replay must produce identical
hashes on Windows, macOS, and Linux.

At each hourly replay roll, persist an immutable canonical initial-snapshot
object addressed by its BLAKE3 hash before writing the replay header. Retain the
object for at least as long as every archive that references it and delete it
only when no replay or backup references it. The replay export tool emits a
self-contained bundle containing the replay stream, its snapshot object, and
the matching content manifest; importing verifies all hashes before execution.

### Content compatibility

Vendor the pinned CDDA content with a reproducible importer. Preserve upstream
JSON without semantic rewrites and implement its parser, inheritance,
finalization, dependency, replacement, and effect-on-condition behavior in
Rust. Do not introduce RON or a replacement content language.

Import `data/core`, `data/json`, `data/names`, `data/raw`, and all bundled
`data/mods` from commit `4dfd36038b16650dc1b5cb9d79a3e42363174b05`.
Import `TEST_DATA` and `Standard_Combat_Tests` only as test fixtures. Import
fonts, sound, title, shaders, and tileset assets only with recorded compatible
licenses. Exclude Android, browser, screenshot, packaging, and XDG artifacts.

Generate a manifest for every imported file containing upstream path, commit,
BLAKE3 hash, license, and destination. Fail the import on unknown provenance.

Content work must include:

- Schema validation with useful source locations and diagnostics
- Cross-reference validation
- Deterministic load/finalization order
- Copy-from/inheritance semantics used by the pinned upstream baseline
- Mod dependency, replacement, and load-order behavior
- Stable mapping between string content IDs and runtime definitions
- Effect-on-condition and other data-driven behavior required by baseline
  content
- Provenance and licensing metadata for imported assets and data
- Tests using all core content and every bundled mod in the pinned scope

Test core alone and test every player-selectable bundled mod in a
dependency-resolved load set. Do not force mutually exclusive mods into one
world merely to claim coverage.

Do not silently ignore unsupported fields. Reject them clearly, implement them,
or track them as explicit parity failures.

### Licensing and attribution

License original Rust source and project documentation under CC BY-SA 3.0 and
include the complete license text. Preserve upstream attribution and mark
ported or transformed upstream material with its source path and commit.
Retain separately licensed dependencies, fonts, sounds, tilesets, and other
assets under their own compatible licenses and include their notices. Do not
ship an asset whose license or provenance is unknown or incompatible.

### Supported hardware and performance gates

The minimum x86-64 client target is a four-core CPU, 8 GiB RAM, and a GPU with
2 GiB VRAM supporting Direct3D 12 on Windows 10+, Metal on macOS 13+, or Vulkan
1.2 on GNU/Linux with glibc 2.35 or newer and X11 or Wayland. The minimum Apple
silicon client is a base Apple M1 with 8 GiB unified memory on macOS 13. Every
required client target must sustain 60 frames per second at 1920 by 1080 in the
standard tiles view on its minimum hardware.

The 16-player server target is four dedicated CPU cores on x86-64 or four Apple
silicon performance cores, 8 GiB RAM, NVMe storage, and 20 Mbit/s symmetric
network capacity. Run the standard release workload natively on
`aarch64-apple-darwin`, `x86_64-apple-darwin`, and
`x86_64-unknown-linux-gnu`. It has 16 connected clients plus 64 additional
disconnected characters distributed across representative Active, Warm, and
Dormant regions. All three server targets must meet every gate:

- Simulation tick time below 35 ms at p95 and 50 ms at p99
- Server resident memory below 4 GiB
- Steady-state egress below 256 Kbit/s per connected client on average
- Playable state delivered within five seconds of reconnect on a 20 Mbit/s link
- Cold startup and full content validation completed within 60 seconds
- A 24-hour soak with no crash, desync, database corruption, unbounded growth,
  or missed tick deadline rate above 0.1 percent

## Fixed baseline and parity accounting

Create and continuously maintain `PORTING_MATRIX.md`. Mechanically generate the
source-, data-, and test-derived portions and hand-maintain behavioral evidence
that cannot be generated.

Each subsystem and content category must be classified as one of:

- Not investigated
- Specified
- Implementing
- Implemented but unverified
- Behaviorally verified
- Intentionally adapted for multiplayer
- Out of scope by this prompt

The time model, offline-character policy, simultaneous command resolution, and
other adaptations explicitly prescribed by this prompt are authorized. When a
new pinned behavior cannot coexist with those locked rules, implement the
smallest deterministic behavioral change that preserves CDDA's player-visible
intent. Record that change in an ADR and the parity matrix; do not wait for a
new product decision. Every adaptation must document:

- The pinned CDDA behavior
- Why it fails or produces poor results in persistent real time
- The new behavior
- Multiplayer edge cases
- Tests proving the adaptation

Do not use raw file count, type count, compilation, or content parsing alone as
evidence of parity. A subsystem is verified only when its important observable
behavior is tested.

In-scope parity includes all of these pinned-baseline areas:

- World generation, overmaps, maps, terrain, furniture, traps, fields, weather,
  seasons, map extras, specials, and regional settings
- Player, NPC, monster, faction, and creature behavior
- Movement, pathfinding, line of sight, lighting, sound, scent, and perception
- Anatomy, health, damage, healing, disease, effects, needs, stamina, pain,
  morale, sleep, temperature, mutations, and bionics
- Items, charges, pockets, containment, clothing, armor, weapons, ammunition,
  tools, qualities, ownership, and inventory interaction
- Melee, ranged combat, projectiles, explosions, fire, environmental hazards,
  death, and drops
- Skills, proficiencies, recipes, crafting, disassembly, construction, reading,
  and interruptible long activities
- Vehicles, parts, installation, cargo, fuel, power, controls, movement,
  collision, and damage
- NPC AI, dialogue, missions, trade, camps, companions, and faction interaction
- Character creation, scenarios, professions, traits, achievements, memorials,
  and progression
- Save/load behavior, world options, configuration, keybindings, accessibility,
  localization, sound, and supported tiles/content presentation
- Core CDDA data and every bundled mod in the pinned content scope
- Data-driven scripting and effects required by included content
- Debug and administration capabilities necessary to test and operate the port

Mobile, browser, console, Android, and iOS clients and platform services are out
of scope. `TEST_DATA` and `Standard_Combat_Tests` are test fixtures rather than
player-selectable mods. Assets with unknown or incompatible licenses are not
shipped. Upstream developer and release tools that are unnecessary to build,
validate, import, test, or operate the port are replaced by equivalent Rust
tooling. These are the only standing parity exclusions.

## Persistent multiplayer requirements

The server must behave correctly with 0 through 16 connected players.

At minimum, implement and verify:

- Authentication, authorization, accounts, and durable character ownership
- Character creation, selection, reconnect, and death handling
- Server-authoritative validation of every client command
- No trust in client position, inventory, timing, visibility, RNG, or outcomes
- Chunk-, visibility-, perception-, and ownership-aware interest management
- Relevance-filtered initial snapshots and incremental deltas
- Movement prediction that cannot create canonical state
- Reconciliation that is understandable rather than visibly teleporting under
  ordinary latency
- Concurrent interactions with deterministic conflict resolution
- Chat and essential social/session feedback
- Administrative shutdown, maintenance, backup, restore, inspection, and
  moderation functions
- Graceful and abrupt disconnect handling without removing or protecting the
  character
- Offline character simulation and reconnection
- World-time progress with zero connected clients
- Inactive-region catch-up without full simulation of the entire generated
  world
- Rate limits, size limits, validation, and safe rejection of malformed or
  hostile network messages
- Protocol version negotiation and comprehensible mismatch errors
- Metrics and structured logs sufficient to diagnose desyncs and persistence
  failures

Persistent real-time behavior must be designed explicitly for menus, crafting,
sleep, reading, travel, vehicles, combat, and disconnected actors. Never hide a
single-player pause assumption inside a gameplay system.

## Required project documentation

Create and maintain:

- `README.md`: supported state, setup, running client/server, and license
- `IMPLEMENTATION_STATUS.md`: current phase, working features, blockers, latest
  verification, and immediate next tasks
- `PORTING_MATRIX.md`: pinned-baseline parity accounting
- `docs/architecture.md`: actual crate and runtime architecture
- `docs/protocol.md`: commands, events, deltas, channels, trust boundaries, and
  compatibility policy
- `docs/persistence.md`: schema, transaction boundaries, recovery, migrations,
  backup, and restore
- `docs/time.md`: clocks, simulation phases, activities, offline progress, and
  inactive-region behavior
- `docs/content.md`: upstream baseline, import/loading pipeline, compatibility,
  validation, and provenance
- `docs/operations.md`: dedicated-server deployment, configuration, metrics,
  backup, restore, upgrades, and graceful shutdown
- `docs/testing.md`: commands, test layers, replay fixtures, network simulation,
  and platform verification
- ADRs for consequential decisions and rejected alternatives
- License, attribution, and third-party notices required by upstream and every
  reused dependency or asset

Keep documentation synchronized with behavior. Do not describe aspirational
features as implemented.

## Execution method

### Maintain a playable vertical slice

Build the port through expanding end-to-end slices. Keep the main branch
buildable and the current slice runnable.

The first architecture slice must run on the macOS development host and must:

1. Start a persistent headless server.
2. Connect two Bevy clients, including an in-process test mode.
3. Load and replicate one validated world chunk.
4. Spawn actors with stable IDs.
5. Move both actors simultaneously through server-authoritative commands.
6. Disconnect one client while its character remains physically present.
7. Allow the connected actor or an NPC to affect the disconnected character.
8. Advance world time with both clients disconnected.
9. Stop and restart the server.
10. Restore the same chunk, actors, scheduled state, and world time.
11. Reconnect to the surviving character.
12. Replay the recorded command sequence to the same verified state hash.

Do not confuse completion of this slice with completion of the port. Use it to
validate architecture, then expand the parity matrix subsystem by subsystem.

### Port behavior before breadth

For each subsystem:

1. Study upstream source, data, tests, and observable behavior.
2. Write a concise behavior specification and identify multiplayer conflicts.
3. Add characterization or golden tests for every observable behavior boundary.
4. Implement the smallest complete end-to-end behavior.
5. Integrate persistence, networking, UI, content, and replay handling rather
   than postponing those boundaries indefinitely.
6. Verify edge cases and performance.
7. Update the parity matrix and documentation.
8. Conduct a fresh review before declaring the subsystem verified.

Maintain a working thin path through all layers instead of accumulating a broad
collection of unconnected domain types.

### Track progress durably

Update `IMPLEMENTATION_STATUS.md` before ending any work session. It must state:

- Exact upstream baseline commit
- Current milestone
- What changed
- What is actually runnable
- Verification commands and results
- Known defects and risks
- The next concrete tasks in priority order

Record unfinished work in tracked project documents or issues, not only in chat
context. On resumption, trust the repository and verification results over a
possibly stale conversational summary.

### Work autonomously

- Do not reopen the product, architecture, scope, or release choices fixed in
  this prompt and `ARCHITECTURE_DECISIONS.md`.
- For routine implementation mechanics beneath the locked interfaces, use the
  simplest compliant design, record consequential engineering selections in
  ADRs, and continue without requesting another product decision.
- If one workstream is blocked, continue other valuable in-scope work.
- Use bounded subagents for independent research, implementation, testing, and
  review when the environment permits them. Give each a concrete scope and
  independently validate its output.
- Preserve existing user changes and avoid destructive source-control actions.
- Do not publish, push, open pull requests, deploy public servers, or mutate
  external services unless explicitly authorized.
- Breaking changes to the new port's pre-release APIs and schemas are allowed
  when they improve the architecture. Once persistent worlds are distributed,
  provide explicit migration tooling rather than silently discarding them.

## Verification requirements

Create fast local checks first, then broaden them as functionality grows.

After the workspace exists, the standard local gate includes all four commands:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

Also add and maintain:

- Unit tests for domain invariants and content loaders
- Headless integration tests for complete gameplay interactions
- Golden/characterization tests against pinned CDDA behavior
- Property tests for inventories, containment, coordinates, IDs, time, and
  persistence round trips
- Deterministic replay tests and canonical state hashes
- Network tests with latency, jitter, loss, reordering, duplication,
  disconnects, reconnects, and malformed messages
- Security tests and fuzzing for content, save, and network parsing
- Persistence tests covering transactions, crashes, partial writes, migrations,
  backup, and restore
- World catch-up tests spanning unloaded chunks and zero-player intervals
- Performance benchmarks for active simulation, content loading, pathfinding,
  chunk streaming, serialization, and database commits
- A 16-client soak test with representative active and disconnected characters
- Platform CI for Linux, Windows, and macOS clients and native headless-server
  jobs for `aarch64-apple-darwin`, `x86_64-apple-darwin`, and
  `x86_64-unknown-linux-gnu`
- Clean-machine dedicated-server build, run, and deployment tests on all three
  required server targets
- Dependency license and vulnerability checks

Never weaken, skip, or delete a failing test merely to obtain a green build.
Diagnose it as an implementation defect or a faulty assertion and document the
evidence before changing either side.

## Review discipline

At every milestone:

1. Inspect the full milestone diff and all untracked files.
2. Have a fresh reviewer or independent subagent examine correctness,
   regressions, security, unsafe edge cases, missing tests, architecture drift,
   licensing, and misleading documentation.
3. Validate each finding against current code.
4. Fix every confirmed critical or high-severity issue.
5. Address lower-severity in-scope findings or document a concrete rationale.
6. Rerun all affected checks.
7. Repeat independent review when fixes are substantial.

Do not claim a milestone complete with confirmed critical defects, failing
required checks, corrupted migration paths, known privilege bypasses, or
unverified persistence behavior.

## Completion gate

The assignment is complete only when all of the following are true:

1. The pinned baseline and exact in-scope content set are recorded.
2. Every in-scope row of `PORTING_MATRIX.md` is behaviorally verified or has an
   architecture-authorized, tested multiplayer adaptation. Only the fixed
   exclusions in this prompt may be marked out of scope.
3. Native clients build and pass required checks on Windows, macOS, and Linux.
4. Headless servers for `aarch64-apple-darwin`, `x86_64-apple-darwin`, and
   `x86_64-unknown-linux-gnu` build, deploy, run, shut down, back up, restore,
   recover from tested failures, and pass the performance and 24-hour soak gates
   natively.
5. Sixteen connected clients and 64 additional disconnected characters pass
   the specified 24-hour soak and every stated performance budget.
6. The server remains authoritative under hostile, malformed, late, duplicated,
   reordered, and conflicting client commands.
7. Disconnected characters remain physically present and vulnerable, and can
   be affected, killed, persisted, and reconnected correctly.
8. World time and scheduled consequences progress correctly with zero connected
   clients.
9. Active, inactive, unloaded, and reloaded chunks behave consistently.
10. Save data, migrations, crash recovery, backup, and restore pass the
    persistence conformance suite.
11. Rendering-independent replay reproduces verified outcomes and state hashes.
12. All in-scope CDDA mechanics and included content are usable through the Bevy
    client, not merely parsed or represented internally.
13. Operator, player, contributor, protocol, persistence, content, and testing
    documentation matches the delivered behavior.
14. Required licensing, attribution, provenance, and third-party notices are
    complete and audited.
15. Formatting, linting, tests, documentation, platform CI, fuzz targets,
    security checks, benchmarks, and the specified soak test pass at the fixed
    release gates in this prompt.
16. A final independent full-project review finds no unresolved critical or
    high-severity correctness, security, persistence, or licensing issue.
17. There are no placeholders, stubs, ignored unsupported content fields, or
    undocumented manual steps being counted as completed features.

If the environment, permissions, hardware, or an external service prevents a
required completion check, do not claim the port is complete. Report the exact
blocker, the work already verified, and the remaining command or external action
needed.

## Communication

Lead progress reports with working outcomes. Keep them concise but precise:

- Current milestone and percent of verified parity rows
- User-visible or operator-visible capability added
- Important architectural decision or adaptation
- Verification performed and result
- Confirmed risks or blockers
- Next concrete implementation target

Do not report activity as progress merely because files were created or code
compiled. Report behavior that now works and evidence that it works.

Begin by reading the authoritative inputs, verifying and recording the pinned
upstream baseline, creating the Rust workspace and durable status/parity
documents, and implementing the first architecture slice. Then continue
expanding verified vertical slices until the completion gate is genuinely
satisfied.
