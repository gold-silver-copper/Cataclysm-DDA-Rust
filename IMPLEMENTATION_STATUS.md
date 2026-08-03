# Implementation Status

Updated 2026-08-02 for commit
`a3e6bfcf6a68285ea95205b1794f228985a38a1f`.

The upstream baseline is fixed at
`4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Live checkpoint

- Implemented tree: `a3e6bfcf6a68285ea95205b1794f228985a38a1f`.
- Cheap compilation checkpoint: the same commit passed the affected-client,
  server, and simulation checks listed below, and a later full
  `cargo check --workspace` with one pre-existing dead-code warning.
- Current representation: protocol 142, persistence schema and minimum
  recoverable schema 119, replay format 4, CanonicalStateV117,
  CanonicalEventsV32, and worldgen algorithm 2.
- `origin/main`, local `main`, and `acceleration/generalized-subsystems` pointed
  to the implemented tree when this status was written.
- Current work boundary: the reviewed bounded ordinary-vehicle family is
  provisionally complete. No next family is authorized by the last scoped
  instruction; obtain a new user direction before expanding powered vehicles,
  NPC/social, environment, or another subsystem.

This is not a comprehensively verified green release. Expensive acceptance,
oracle, content, recovery/replay, workspace, and platform gates remain deferred.
Implementation state and verification state are intentionally reported
separately.

## Runnable implementation

The project remains an incomplete multiplayer CDDA port, but the cumulative
implementation includes:

- a standalone persistent authoritative server using Tokio and Iroh, with a
  Bevy-only graphical client and no Bevy dependency in server/shared crates;
- passwordless Iroh endpoint enrollment and authorization, persistent
  characters, server-owned commands/events, stable IDs, SQLite recovery, and
  portable replay infrastructure;
- chunked production regional world generation, deterministic cities and
  connected roads, plus provisionally implemented rivers, bridges, overmap
  specials, and content-driven monster spawning;
- the mature containment/item-group baseline and ordinary client inventory,
  loot, crafting, study, disassembly, consumption, equipment, and interaction
  paths described in the parity matrix;
- canonical anatomy, armor, effects, stamina, dodge, healing, ordinary combat,
  and a generalized monster boundary covering ordinary and several data-driven
  special-attack families;
- substantial authoritative NPC dialogue, mission, faction, generation,
  vulnerability, AI, and client presentation paths, with final family closure
  and independent review still pending;
- static vehicles, boarding, cargo transfer, doors, client-visible controls,
  and a conservative straight leg-muscle movement subset; and
- deterministic weather, local wind and shelter, field contact effects, gas
  spread, precipitation effects, and exact tick-based downtime processing; and
- canonical per-item temperature for actor, NPC, ground, and vehicle-cargo
  items, with pinned temperature-dependent shelf-life rot that removes rotten
  ground and vehicle-cargo items. Rot-bearing crafting and construction results
  still fail closed, so only item-group-sourced perishables and corpses carry
  rot state today.

Disconnected characters remain physically present and vulnerable, and the
world continues advancing with zero connected players. Unsupported content
branches remain explicit and fail closed.

## Family state

| Family | Implemented state | Verification state / boundary |
| --- | --- | --- |
| Containment, item groups, mapgen foundations, regional field, cities, roads | Complete at their recorded baselines | Mature completion evidence is retained in `docs/parity-ledger.json`; later extensions do not reopen the foundation. |
| Rivers, bridges, overmap specials, spawning | Generalized production paths implemented | Consolidated regional/mapgen verification deferred. |
| Anatomy and ordinary combat | Generalized production paths implemented | Comprehensive family checks deferred. |
| EOCs and use actions | Bounded conditions, effects, scheduling, variables, actor math, event dispatch, confirmations, transforms, and item activation implemented | Dynamic-value and broader interaction closure remains; verification deferred. |
| Monsters | Hard implementation boundary established | Rare/unsupported spell-target and field-immunity branches stay fail closed; family verification deferred. Do not expand rare variants indefinitely. |
| NPCs, dialogue, missions, factions | Substantial production wiring implemented | Final dialogue/effect closure and two interrupted independent reviews remain; do not call the family complete. |
| Vehicles | Static/boarding/cargo/doors and bounded manual controls implemented | Ordinary-vehicle gate deferred. Powered propulsion, general steering, collision/damage, fuel/power, repair, and part mutation remain planned. |
| Environment | Weather, shelter/wind, field effects, gas spread, precipitation, and downtime tick processing implemented | Gas/environment review and consolidated verification remain; agriculture and broader inactive-region catch-up are planned. |

The definition-level states, explicit fail-closed boundaries, and dependency
graph live in `docs/parity-ledger.json`. Weighted progress in
`docs/runtime-progress.json` is a verified historical evidence floor, not a
claim that later implemented families passed four-mode or production gates.

## Latest exact verification

The following passed at `a3e6bfcf6a68285ea95205b1794f228985a38a1f`
before it was pushed:

```text
cargo fmt --all -- --check
cargo check -p cdda-sim -p cdda-server -p cdda-client
git diff --check
```

The affected Cargo check completed in 3.47 seconds and emitted only existing
unused-function warnings in `crates/server/src/npc_faction.rs`.

A later documentation pass additionally ran a full workspace check against the
same implementation tree:

```text
cargo check --workspace
```

It finished in 1 minute 7 seconds with exactly one warning: the never-used
`visible_npc_faction` function at `crates/server/src/npc_faction.rs:60`. This
confirms that every crate, including the Bevy client, compiles at the current
contract. It is still only a compilation checkpoint. No gameplay,
C++ differential, real-Iroh, content-inventory, recovery/replay, platform, fuzz,
benchmark, or soak result is implied by this compilation checkpoint.

## Deferred verification

Run the compact subsystem gates in `docs/deferred-checks.md` only when the user
explicitly requests a verification checkpoint. Do not claim a deferred gate
passed. The last fully documented long production checkpoint predates the
current representation and therefore cannot verify the current tree.

## Module ownership

The fixed extraction baseline is
`40037fbb1db9eaac8d4889b811d29f8c00380e6b`. Current central file sizes are:

- `crates/sim/src/lib.rs`: 32,362 lines (+2,770 from baseline);
- `crates/protocol/src/lib.rs`: 13,416 lines (+3,443);
- `crates/persistence/src/lib.rs`: 13,196 lines (+119); and
- `crates/server/src/lib.rs`: 9,436 lines (+629).

The workspace currently holds 182,434 lines of Rust across nine crates
(`sim`, `server`, `protocol`, `persistence`, `content`, `client`, `net`,
`tools`, `conformance`) and 435 `#[test]` functions.

The largest specialized ownership modules outside the four central files are:

- simulation: `items.rs` (7,582 lines), `monsters.rs` (3,776), `mapgen.rs`
  (3,335), `eocs.rs` (2,881), `npc_dialogue.rs` (1,660), `vehicles.rs` (1,449),
  `roads.rs` (1,333), `weather.rs` (1,003), `combat.rs` (967), `specials.rs`
  (922), `rivers.rs` (912), `missions.rs` (727), `fields.rs` (709), `npcs.rs`
  (709), plus `anatomy.rs`, `cities.rs`, `interactions.rs`, `mission_items.rs`,
  `npc_faction.rs`, `overmap.rs`, and `use_actions.rs`;
- server: `main.rs` (7,994), `worldgen.rs` (4,336), `item_groups.rs` (3,024),
  `eocs.rs` (1,106), `regional_field_acceptance.rs` (1,073), plus `anatomy.rs`,
  `missions.rs`, `npc_classes.rs`, `npc_faction.rs`, `use_actions.rs`,
  `vehicles.rs`, and `weather.rs`;
- protocol: `item_groups.rs` (3,158), `eocs.rs` (1,077), plus `anatomy.rs`,
  `astronomy_table.rs`, `interactions.rs`, `missions.rs`, `npc_dialogue.rs`,
  `npc_faction.rs`, `use_actions.rs`, `vehicles.rs`, and `weather.rs`; and
- content: 34 definition modules led by `item.rs` (5,226), `monster.rs`
  (3,061), `recipe.rs` (2,853), `mapgen.rs` (2,834), `item_group.rs` (2,330),
  `vehicle.rs` (1,564), and `eoc.rs` (1,483).

`crates/persistence` remains a single 13,196-line `lib.rs` with no extracted
modules and is the least-decomposed crate. Future work should grow the focused
modules above and keep central-file changes to unavoidable wiring or canonical
representation.
