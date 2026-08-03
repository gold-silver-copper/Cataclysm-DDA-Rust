# Handoff Context

Updated 2026-08-02 after a source-verification documentation pass over the
bounded ordinary-vehicle checkpoint. The 2026-07-31 reconciliation is unchanged
except where the source contradicted it; see "Source verification pass" below.

## Repository state

- Repository: `/Users/kisaczka/Desktop/code/cdda/Cataclysm-DDA-Rust`
- Read-only upstream checkout:
  `/Users/kisaczka/Desktop/code/cdda/Cataclysm-DDA`
- Pinned upstream commit:
  `4dfd36038b16650dc1b5cb9d79a3e42363174b05`
- Pinned upstream tree:
  `210f31db2e8b2f0caed1809f1a66781859f9d129`
- Current branch: `acceleration/generalized-subsystems`
- Current implementation commit:
  `a3e6bfcf6a68285ea95205b1794f228985a38a1f`
- At the start of this documentation pass, `origin/main`, local `main`,
  `acceleration/generalized-subsystems`, and `primary/npc-integration-final`
  all pointed to that commit.
- Current contract: `P142/S119/R4/CS117/CE32`; worldgen algorithm 2.

The worktree is intentionally dirty with this uncommitted documentation
reconciliation and two small tools-validator fixes. No commit, push, PR, stash,
reset, force operation, or broad staging was requested or performed.

Intended changed paths are:

- `ARCHITECTURE_DECISIONS.md`
- `GOAL_PROMPT.md`
- `HANDOFF_CONTEXT.md` (currently untracked)
- `IMPLEMENTATION_AGENT_PROMPT.md`
- `IMPLEMENTATION_STATUS.md`
- `PORTING_MATRIX.md`
- `README.md`
- `docs/content-schema-inventory.md`
- `docs/deferred-checks.md`
- `docs/parity-ledger.json`
- `docs/runtime-progress.json`
- `tools/cpp-oracle/README.md`
- `crates/tools/src/main.rs`
- `crates/tools/src/cpp_oracle.rs`

Before any future commit, re-inspect the entire working tree and stage these
paths explicitly. Preserve any new user work and exclude build output,
databases, worlds, logs, credentials, Iroh identity keys, and temporary files.

## Source verification pass

Every hard claim in the live documents was re-checked directly against the
working tree at `a3e6bfc` and confirmed exact:

- `PROTOCOL_VERSION = 142`, `SCHEMA_VERSION = 119`,
  `MIN_RECOVERABLE_SCHEMA_VERSION = 119`, `REPLAY_FORMAT_VERSION = 4`, the
  `CanonicalStateV117` and `CanonicalEventsV32` blake3 derive keys, and
  `WORLDGEN_GENERATOR_VERSION_V2`;
- the four central file sizes (32,362 / 13,416 / 13,196 / 9,436 lines);
- the nine-crate layout and every ownership module named in the ledger; and
- `docs/parity-ledger.json` holding 31 milestones with `vehicles` active.

`cargo check --workspace` now passes in 1 minute 7 seconds with exactly one
warning: never-used `visible_npc_faction` at
`crates/server/src/npc_faction.rs:60`. No deferred gate was run.

Three real documentation gaps were found and fixed:

1. `README.md`'s "Once connected" control list predated the vehicle, pocket,
   construction, and NPC-dialogue keys. It now documents W/D wear and take-off,
   M construction, I/Y pocket insertion and removal, K talk/board/unboard,
   O toggling adjacent vehicle doors, WASD issuing `DriveVehicle` steering and
   propulsion while boarded, and G/Q reaching vehicle cargo from the item menu.
   All were verified against `crates/client/src/main.rs`.
2. `README.md`'s cumulative protocol narrative stops at Protocol 97 while the
   contract is Protocol 142. That truncation is now stated explicitly, with a
   pointer to the live documents for every post-97 family.
3. `PORTING_MATRIX.md` and `IMPLEMENTATION_STATUS.md` listed rot as wholly
   incomplete. Rot is in fact a production family: `crates/sim/src/items.rs`
   implements the pinned ambient and temperature-dependent rot intervals and
   rotten-away threshold, and `crates/sim/src/lib.rs` removes rotten ground and
   vehicle-cargo items each processing pass. The exact remaining boundary is
   that `crates/server/src/main.rs` still rejects rot-bearing results at the
   crafting and construction constructor boundaries, and carried actor/NPC
   inventory receives temperature processing without rot removal, so only
   item-group-sourced perishables and corpses carry rot today.

No other document required a change. `docs/parity-ledger.json`,
`docs/runtime-progress.json`, and `docs/deferred-checks.md` were re-read and are
accurate at the current contract.

## Source-of-truth order

The live documents are now synchronized:

1. `IMPLEMENTATION_STATUS.md` records the exact implementation commit,
   representation, runnable behavior, cheap checks, deferred gates, and module
   growth.
2. `PORTING_MATRIX.md` gives concise human-readable family status.
3. `docs/parity-ledger.json` is the machine-readable dependency and submilestone
   state. It binds to `P142/S119/R4` and names `vehicles` as the paused active
   top-level milestone.
4. `docs/runtime-progress.json` records the verified evidence floor. Its
   `verified_commit` remains null, so it does not convert later implementation
   into four-mode credit.
5. `docs/deferred-checks.md` contains only consolidated subsystem IDs for the
   current contract.

`README.md` and `ARCHITECTURE_DECISIONS.md` now label their protocol-by-protocol
sections as chronological history, not current verification. The long-lived
goal and implementation prompts point agents back to the live status sources;
`IMPLEMENTATION_AGENT_PROMPT.md` remains 299 lines.

Generated inventories, checked oracle observations, accepted ADRs, historical
status, and past review records were not rewritten as current state:

- `docs/content-schema-inventory.json` remains the unchanged generated snapshot
  of the pinned corpus; its Markdown companion now explains that distinction.
- `docs/oracles/*`, `docs/reviews/*`, `docs/history/*`, and
  `docs/adr/0001-workspace-foundation.md` remain immutable evidence records.
- Vendored upstream files were not modified or rescanned.

## Architectural constraints

- Bevy 0.19.0 is client-only. The server, simulation, protocol, persistence,
  content, networking helper, and tools crates must remain Bevy-free.
- The dedicated server and canonical simulation are plain Rust; Tokio owns
  runtime I/O and Iroh 1.0.3 owns transport and endpoint identity.
- Iroh identity is the authentication mechanism. Do not add passwords, bearer
  tokens, invitation secrets, or a second application authentication system.
- The server is authoritative for commands, events, stable identity,
  simulation, persistence, recovery, and replay.
- The real-time persistent world continues with zero players. Disconnected
  characters remain physically present, simulated, vulnerable, and killable.
- Preserve deterministic ordering and RNG consumption, chunked world storage,
  explicit stable IDs, strict hostile-input validation, recovery/replay
  correctness, and the pinned C++ baseline.
- Port generalized semantic families mechanically before redesigning them.
  Keep unsupported behavior explicit and fail closed; do not perform a
  line-by-line C++ rewrite or invent defaults to admit content.
- Batch related wire/canonical/persistence changes into one representation
  version per generalized family.

## Just-completed family

Commit `a3e6bfc` provisionally closes the reviewed static-vehicle, boarding,
cargo, door, and bounded manual-control family. Production behavior includes:

- finalized vehicle content and deterministic mapgen placement;
- stable canonical vehicle/part identity and persistence;
- client-visible vehicles, boarding/unboarding, cargo transfer, doors, and
  WASD-issued authoritative controls;
- fail-closed cargo admission, generation, replication, and recovery through
  broken cargo parts;
- exact pinned zero/one integral cargo-chance behavior;
- fail-closed disabled-vehicle initialization rather than fabricated damage;
  and
- a conservative straight leg-muscle propulsion subset requiring a sole
  boarded driver, live controls, steerable supported wheels, a full-health leg
  engine, two full-health legs, positive represented muscle power, sufficient
  stamina, and no winded effect.

Two independent adversarial vehicle reviews supplied the findings resolved by
the final commit. The following later vehicle semantics remain planned or
fail-closed and must not be described as implemented:

- full steering skill, fumbles, dynamic pivot, turning, and translation;
- explicit persisted controller leases and multiple-passenger control;
- powered propulsion, fuel, batteries, and power networks;
- exact mass, velocity, traction, collision, and damage physics;
- arm-powered grip/lift requirements;
- exact disabled-vehicle fault and randomized damage state; and
- repair plus part installation/removal.

## Other subsystem boundaries

- Containment, item groups, conformance foundations, atomic mapgen, OMT
  identities, start selection, regional terrain, cities, and roads remain
  complete at their demonstrated baselines.
- Rivers/bridges, overmap specials, and content-driven spawning have generalized
  production implementations with the consolidated mapgen gate deferred.
- Anatomy/combat and bounded EOC/use-action families have substantial production
  implementations with comprehensive verification deferred.
- Monsters are at a hard implementation boundary. Ordinary and many
  data-driven special attacks are implemented, but rare unsupported spell
  target/application/immunity branches remain explicit and fail closed. Do not
  resume indefinite rare-variant expansion.
- NPC/social remains `in_progress`. Dialogue, missions, factions, class
  generation, ordinary AI, vulnerability, combat, mission ordering, and atomic
  dialogue transitions have production wiring. Final closure is planned, and
  two final independent NPC reviews were interrupted; do not claim the family
  reviewed or complete.
- Environment remains `in_progress`. Weather, terrain/vehicle shelter, local
  wind, client observation, field contact/effects, gas spread, precipitation,
  persistence, and exact tick-based downtime processing exist. Agriculture and
  broader inactive-region catch-up remain planned, while consolidated review
  and verification are deferred.

See `docs/parity-ledger.json` for exact submilestone and fail-closed state.

## Current work boundary

The last scoped instruction was to finish only the in-progress vehicle family
and not begin another feature. That bounded family is finished at the baseline
above. Do not silently start powered vehicles, resume NPC/social expansion,
expand environment, or run a verification checkpoint. Obtain a new user
direction first.

When implementation resumes:

- keep every player-facing mutation client-reachable and server-authoritative;
- keep long gates off the implementation critical path until the user says
  `run the verification checkpoint`;
- run affected-package `cargo check` with a hard 60-second timeout after a
  coherent family;
- preserve existing tests even when their execution is deferred;
- update parity state and compact deferred IDs once per family; and
- grow focused ownership modules, not the central simulation/protocol files,
  except for unavoidable wiring or canonical representation.

## Verification from this documentation pass

Passed after the documentation and validator fixes:

```text
jq empty docs/parity-ledger.json docs/runtime-progress.json \
  docs/content-schema-inventory.json docs/oracles/*.json
cargo check -p cdda-tools
cargo xtask parity-ledger-check
cargo xtask runtime-progress-check
```

The tools check completed in 1.64 seconds after the cold dependency build. The
parity gate reported 31 milestones with active `vehicles`. Runtime progress
reported 52 generated definitions and 343/263,435 core-DDA weighted points
(0.1302%), with current-contract checkpoint binding still pending.

The first cold `cargo xtask parity-ledger-check` attempt was aborted at the
required 60-second limit while dependencies compiled. A later warm run passed.
During compilation, four pre-existing missing empty vehicle fields were found
in two C++-oracle fixture constructors and fixed mechanically. The validator was
also updated to accept and validate the ledger's existing submilestone records
and `deferred` oracle state.

Before the documentation pass, commit `a3e6bfc` had passed:

```text
cargo fmt --all -- --check
cargo check -p cdda-sim -p cdda-server -p cdda-client
git diff --check
```

The affected implementation check completed in 3.47 seconds with only existing
unused-function warnings in `crates/server/src/npc_faction.rs`.

No C++ oracle scenario, content validation/inventory scan, production-field or
real-Iroh acceptance, four-mode recovery/replay suite, full workspace test,
full Clippy, rustdoc, platform check, fuzzing, benchmark, or soak test was run.
Those consolidated gates remain deferred and must not be claimed as passed.

## Known risks

- The current tree has cheap compilation and document-ledger evidence, not a
  comprehensive gameplay or release checkpoint.
- `CONTENT-TEST-TARGET@P142/S119/R4/CS117/CE32` remains blocked by the recorded
  pre-existing material fixture until the verification checkpoint investigates
  it.
- Central modules remain large relative to the fixed extraction baseline:
  simulation 32,362 lines, protocol 13,416, persistence 13,196, and server
  library 9,436. Use focused modules at the next required feature boundary.
- Weighted runtime evidence is deliberately a verified historical floor; it
  gives no credit to the current deferred mapgen, combat, EOC, monster, NPC,
  vehicle, or environment families.
