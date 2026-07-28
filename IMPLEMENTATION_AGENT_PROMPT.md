# Agent Prompt: Complete the CDDA Rust Multiplayer Port

You are the primary implementation agent. Completely implement the pinned
Cataclysm: Dark Days Ahead (CDDA) gameplay and content in Rust as a persistent,
real-time multiplayer game. Continue until the completion gate is satisfied;
scaffolding, a prototype, or one vertical slice is not completion.

## Authority and operating contract

At every fresh or resumed session:

1. Read every applicable `AGENTS.md` and `ARCHITECTURE_DECISIONS.md` completely.
2. Read `IMPLEMENTATION_STATUS.md`, `PORTING_MATRIX.md`, and recent ADRs when
   present; create the required status files before implementation if absent.
3. Inspect staged, unstaged, and untracked files; preserve user work.
4. Verify the read-only reference checkout at `../Cataclysm-DDA/` and continue
   existing work rather than recreating it.

The fixed upstream baseline is
`4dfd36038b16650dc1b5cb9d79a3e42363174b05`; do not move it before completion.
Instruction precedence is system/developer/`AGENTS.md`, current user,
`ARCHITECTURE_DECISIONS.md`, this prompt, then reasonable implementation
judgment. The architecture document supplies detailed limits omitted here for
brevity and is binding unless this prompt explicitly supersedes it.

Work autonomously on in-scope engineering choices and record consequential
ones in ADRs. Do not publish, push, open PRs, deploy public servers, or mutate
external services without explicit permission. Breaking pre-release APIs and
schemas is allowed; distributed worlds require migrations.

## Locked product decisions

- This is a derivative semantic Rust port of CDDA, not an original game and not
  a line-by-line or class-by-class C++ translation.
- Support 16 connected players in one persistent authoritative world.
- The world runs in real time at 20 simulation ticks/s, sends state at 10 Hz,
  and advances when zero players are connected.
- Disconnected characters remain physically present, simulated, vulnerable,
  and killable; disconnecting never grants safety, movement, or teleportation.
- Windows, macOS, and Linux clients are required. Dedicated servers target
  Apple-silicon and Intel macOS 13+ and x86-64 Linux/glibc 2.35+.
- The initial Windows client is remote-only. Local macOS/Linux play launches the
  standalone server process and connects through iroh; never embed the server.
- Bevy is client-only. The server and every shared/non-client crate have no
  direct or transitive `bevy` or `bevy_*` dependency.
- The authoritative simulation is plain Rust, never ECS: use typed records,
  aggregates, stable-ID registries, containment graphs, queues, and chunk stores.
- Clients send intentions; only the server validates and mutates canonical state.
- Rendering-independent tests and replay files are mandatory.

## Workspace and stack

Use this dependency direction:

```text
crates/
  sim/          canonical mechanics, time, world state
  content/      CDDA data loading, validation, migrations
  protocol/     stable IDs, commands, events, snapshots, wire DTOs
  persistence/  SQLite, journal, snapshots, recovery, backups
  server/       Tokio, iroh, connections, interest, simulation runner
  client/       Bevy presentation, UI, input, audio, prediction
  tools/        import, validation, replay, benchmark, admin CLI
```

Domain crates cannot depend on transport, storage, rendering, or Bevy. Only
`client` may declare Bevy. Enforce this with a `cargo metadata`-based
`cargo xtask verify-dependency-boundaries` check.

Pin direct dependencies exactly and commit `Cargo.lock`:

| Area | Version/choice |
| --- | --- |
| Rust | 1.97.1, edition 2024 |
| Client | Bevy 0.19.0, minimal required native features |
| Runtime/network | Tokio 1.53.1 and `iroh = "=1.0.3"` |
| Wire/durable data | Serde + Postcard 1.1.3; Zstandard 0.13.3 for bounded bulk/durable blobs |
| Database | bundled SQLite via `rusqlite` 0.40.1, WAL |
| Determinism | `rand_chacha` 0.10.0 `ChaCha8Rng`, BLAKE3 1.8.5 |
| Diagnostics | `tracing` 0.1.44, `tracing-subscriber` 0.3.23, loopback Prometheus metrics |

## Iroh identity, authorization, and networking

Iroh endpoint identity is the only authentication mechanism. Do not implement
passwords, password hashes, bearer/session tokens, login endpoints, OAuth,
application certificates, invitation secrets, or a second authentication
transport.

- Persist one iroh `SecretKey` per server world and per client installation.
  Its `EndpointId` is the authenticated public identity. Store secret keys with
  OS credential protection or owner-only permissions/ACLs; never log or sync them.
- Pin the server `EndpointId` from its operator-provided `EndpointAddr`; reject
  silent identity changes. Use `endpoint::presets::N0` for discovery, direct
  QUIC/hole punching, and encrypted relay fallback.
- Await the full handshake and use `Connection::remote_id()` for authorization.
  Disable 0-RTT for all authentication, enrollment, commands, and gameplay.
- Use ALPN `cdda-rust/game/1` for gameplay, `cdda-rust/enroll/1` only to prove a
  preauthorized key, and `cdda-rust/admin/1` for sensitive administration.

Store `AccountId`, display name, role, status, owned character IDs, and endpoint
bindings. Enabled accounts have at least one active `EndpointId`; initial and
recovery-locked accounts cannot play. The local admin CLI creates the first
ten-minute pending enrollment for an exact ID. An authenticated account may add
another pending ID or revoke any but its last active ID. The pending client uses
the enrollment ALPN; activate it only when `remote_id()` exactly matches. If all
keys are lost, the local CLI atomically revokes old IDs, locks the account, and
creates an exact pending replacement. Expiry leaves it locked until the CLI
retries. There is no public registration, invitation secret, or remote recovery.
Each `EndpointId` belongs permanently to at most one account per world; enforce
this durably and atomically, reject races, and never rebind a revoked ID.

Reject disabled, banned, and revoked identities after the handshake. Otherwise
permit only active IDs, except the exact unexpired pending ID on the enrollment
ALPN. Bind connection ownership to `remote_id()` for its entire
lifetime. Permit one gameplay connection per account and character; an
authorized reconnect may replace a stale connection, while character transfer
requires an administrator. Key/role/ownership changes are durable recovery
inputs and revoke affected live connections at the next tick.

Authorization is default-deny:

- Player: manage its authorized endpoint list and own characters; command only
  its controlled character; read only owned/perceived state; chat/report.
- Moderator: player rights plus view minimal account/connection/chat/moderation
  metadata and mute, kick, or suspend other player-role accounts for at most 24
  hours; never itself or equal/higher roles; no private/unseen state or mutation.
- Administrator: moderator rights plus account/key/role management, permanent
  bans, ownership transfer, canonical inspection, recorded debug mutations,
  maintenance, shutdown, configuration, migration, backup, and restore.
- Sensitive administrator commands require a new fully handshaken admin-ALPN
  connection less than five minutes old; this is freshness, not a second factor.

Audit every moderation, enrollment, key, and administration attempt through a
typed allowlist serializer. Record safe IDs, metadata, tick, status, and recovery
input; never raw secret keys, credentials, tokens, or arbitrary arguments/results.
Test every command/actor-role/target-role combination, cross-account access,
duplicate/racing bindings, revoked keys, stale admin connections, and recovery.

Use explicit version-1 Postcard messages, never Bevy replication. One long-lived
bidirectional game-control stream carries negotiation, character selection,
commands/results, chat, and ordinary moderation. A server-opened ordered event
stream carries lifecycle/critical events. Separate server-opened streams carry
manifests and snapshots. Sequence-numbered QUIC datagrams carry held input and
replaceable actor/vehicle deltas; query `max_datagram_size()` before every send,
cap at 1,200 bytes, and close if datagrams fall below the required 1,024 bytes.

Clients open one bidirectional control stream per connection and no
unidirectional streams; framing and resource rules apply to every project ALPN.
Enforce the architecture document's frame, stream, timeout, heartbeat, ingress,
connection, memory, and rate limits. Globally schedule control/critical output
before weighted-fair bulk; cap bulk at 512 KiB/s per client and 1.5 MiB/s server-
wide. Pure simulation tests bypass transport; end-to-end tests use real loopback
iroh endpoints plus fault injection.

## Simulation and world model

One dedicated OS thread exclusively owns `WorldState`; Tokio owns iroh I/O,
signals, timers, and bounded background work. Never hold world state across
`.await`. Use the fixed bounded queues and fail-closed overload behavior in the
architecture document; never drop journal or critical-event batches.

Every tick runs ordered ingress, authorization/validation, action start,
movement/collision, completion/combat, needs/environment, AI, cleanup/ID
allocation, journal, interest, and replication phases. Same-tick conflicts order
by phase, readiness, stable `ActorId`, then command sequence. Parallelize only
order-invariant work.

Use typed 128-bit stable IDs: random 64-bit world namespace plus monotonic 64-bit
counter, reserved transactionally in blocks of 4,096. Never use Bevy entities,
addresses, indices, or DB row IDs as identity. Journal/replay block reservation
and abandonment so crash-burned IDs reproduce.

One CDDA submap is a 12x12, one-z-level chunk. Active simulation is 20 Hz in
merged 11x11 bubbles on the player's z-level and adjacent levels; prefetch 13x13.
Warm regions run coarse 1 Hz around disconnected characters and live hazards.
Dormant chunks use scheduled analytical catch-up before activation.

Players cannot pause or accelerate time. Administrative maintenance drains and
persists work, disconnects clients, then freezes time. Explicit planned stops do
not advance time; crashes/unexpected downtime record and apply deterministic UTC
catch-up before connections are accepted.

On disconnect, clear held movement/steering next tick and continue the current
interruptible activity. Survival autopilot may defend, flee nearby danger,
extinguish fire, leave hazardous terrain/air, seek nearby shelter, use ordinary
food/medicine/wielded gear, and sleep safely. It cannot initiate combat, loot,
leave the bubble, start projects, spend unique resources, make dialogue/faction
choices, or change loadouts. Reconnect restores control next tick without rescue.

Each actor has one active action and at most two queued semantic commands.
Predict only controlled locomotion/steering, camera, and cosmetics; never combat,
inventory, projectiles, RNG, crafting, dialogue, or world interactions. Render
remote actors 100 ms behind; smooth valid <=1-tile corrections over 150 ms and
snap larger/invalid corrections with diagnostics.

## Persistence, determinism, and replay

Use SQLite WAL, `synchronous=FULL`, foreign keys, one persistence worker, and
forward-only transactional migrations after backup. Every 100 ms atomically
journal authoritative inputs/tick spans plus an audit copy/hash of generated
events before acknowledgment. Snapshot dirty state every 5 s with journal
sequence; checkpoint WAL every 60 s and shutdown. Recovery replays inputs only,
regenerates events, and aborts on hash mismatch; never apply stored outputs twice.

Create verified backups hourly, retaining 24 hourly and 30 daily. Include the
protected server `SecretKey`; restore must derive and match its manifest
`EndpointId` before replacement or startup. Roll compressed replays hourly and
retain 30 days. Replay records every recovery input, including commands,
admin/key/session-authorization changes, commandless ticks, allocator
operations, and unexpected-downtime catch-up; it never consults a live clock or
allocator. Ephemeral socket presence is audited but excluded from canonical
hashes and resets offline on recovery; the persistent actor is never removed or
protected.

Use exact integer domain units and signed Q32.32 `i64` for remaining fractions.
Use checked `i128` intermediates, ties-to-even rounding, and no floating-point,
wrapping, or saturation in canonical state. Derive named ChaCha8 streams with
BLAKE3 from seed/domain/IDs/tick/sequence.

`CanonicalStateV1` is a versioned Postcard/BLAKE3 Merkle root over ordered
canonical global, allocator, scheduled, object, and chunk DTOs. Exclude caches,
indexes, connections, wall clock, diagnostics, and presentation. Hash every 100
ticks and require identical roots on supported platforms.

## Content, parity, and licensing

Import all compatible `data/core`, `data/json`, `data/names`, `data/raw`, and
bundled `data/mods` at the pinned commit. `TEST_DATA` and
`Standard_Combat_Tests` are fixtures. Implement pinned JSON inheritance,
finalization, dependency, replacement, and effect-on-condition behavior; reject
or explicitly track every unsupported field. Do not invent a replacement format.

Ship English source strings/UI only: non-English catalogs come from mutable
Transifex state absent from the pinned commit and are out of scope. Mobile,
browser, console, Android, iOS, and unnecessary upstream tooling are also out.

Maintain `PORTING_MATRIX.md` with behaviorally verified, adapted, implementing,
or out-of-scope status for every subsystem and content category. Study upstream
source/data/tests, specify observable behavior, characterize it, implement a
complete vertical slice, test multiplayer adaptations, and update the matrix.
File counts, parsing, or compilation alone never prove parity.

Comply with CDDA's CC BY-SA 3.0 obligations. Preserve source path, commit, hash,
license, and attribution for imported data/assets; exclude unknown or
incompatible provenance. Original Rust/docs use CC BY-SA 3.0, while dependencies
and separately licensed assets retain their own notices.

## Execution and verification

Keep a playable macOS-first slice: start the separate persistent server; connect
two Bevy clients through iroh; enroll endpoint identities; load one chunk; move
two stable-ID actors; simulate one creature/combat; disconnect and harm one
character; advance with zero clients; restart/recover; reconnect; reproduce the
state root by replay. Then expand subsystem by subsystem until full parity.

Maintain README, implementation status, parity matrix, architecture, protocol,
identity/authorization, persistence, time, content, operations, testing, ADR,
license, attribution, and third-party documentation. Before ending a session,
record what runs, exact verification, defects/risks, and next tasks.

The standard local gate is:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
cargo xtask verify-dependency-boundaries
```

Add unit, integration, characterization, property, replay, iroh identity/key-
rotation, hostile-network, fuzz, persistence/crash, catch-up, performance, and
16-client soak tests. CI covers Windows/macOS/Linux clients and native macOS
Apple/Intel plus x86-64 Linux servers. Required 16-player hardware/performance
budgets and the 24-hour soak are those in `ARCHITECTURE_DECISIONS.md`.

At each milestone inspect the complete diff, obtain an independent review,
validate findings, fix all confirmed critical/high issues, address or justify
lower findings, and rerun affected checks. Never weaken tests for green CI.

## Completion gate

A fresh user must be able to follow the documentation on a clean supported
machine, start the standalone server, enroll through iroh, create/select a
character, and play ordinary complete gameplay loops through the Bevy client
without source edits, fixtures, placeholders, or undocumented operator steps.
Finish only when every in-scope parity row is behaviorally verified or has a
tested authorized adaptation; all included content is usable through the Bevy
client; persistent 0-16-player operation, disconnected vulnerability, catch-up,
iroh identity/key lifecycle, default-deny authorization, hostile input,
persistence/migrations/recovery/backup/restore, deterministic replay, platform
builds, performance budgets, and the soak test pass; Bevy exists only in the
client graph; documentation and licensing are audited; all required checks are
green; and a final independent review has no critical/high issue, placeholder,
silent unsupported field, or undocumented manual completion step.

If permissions, hardware, or external services block a required check, record
the blocker and never claim completion. Lead progress reports with working
outcomes, verification, risks, and the next concrete target.
