# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Live checkpoint

- Verified green commit: `d76965c54d5fee3a081b2a7c860b94a750b92cdd`
  (`Extract server item group normalization`).
- Checkpoint binding: this documentation-only follow-up records the exact green
  implementation commit and scoped independent review; runtime sources are
  unchanged.
- Active milestone: `regional-terrain-base`.
- Runtime: protocol 84, worldgen algorithm 2, persistence schema/minimum
  recoverable schema 62, replay format 3, CanonicalStateV60, and
  CanonicalEventsV18.
- Conformance: scenario 7 and observation 6.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only; the server and
  simulation are plain Rust. Networking and endpoint-owned authentication use
  iroh 1.0.3.
- Mapgen submilestones: `atomic-static-mapgen`, `omt-identities-routing`, and
  `start-location-selection` have runnable engines but remain `oracle_pending`
  until one normalized Rust/C++ comparator covers them; `regional-terrain-base`
  is in progress; `overmap-cities`, `overmap-roads`, `overmap-rivers`,
  `overmap-specials`, and `mapgen-spawning` are planned.

Protocol 81-era and earlier narration is archived in
[docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md](docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md).
Protocols 82 and 83 are summarized in repository history and the architecture record;
this file describes only the current runnable state and next boundary.

## Runnable behavior

The repository is a persistent server-authoritative multiplayer foundation,
not yet a complete CDDA port. The Bevy client can enroll through iroh,
create/select a persistent character, move, fight the admitted creatures,
interact with visible terrain and items, and use the implemented crafting,
reading, disassembly, construction, containment, survival, recovery, and replay
paths. Characters remain present and vulnerable while disconnected, and the
world continues without players.

Fresh worlds persist a bounded 180x180, z=0 overmap layout. The current server
fills that coordinate-owned layout with the pinned `lmoe_north` identity to
preserve the runnable bootstrap; it then materializes the 36 OMTs intersecting
the initial active bubble as 144 canonical submaps. Each OMT is generated from
its coordinate-selected generator, source-phase RNG is resolved before the
completed terrain/furniture/item result rotates, and discovery outside the
fixed layout fails atomically. Ordinary movement that would extend the active
bubble past that boundary is rejected as blocked without failing the tick or
stopping the server.

The selected-content overmap-terrain registry strictly finalizes inheritance,
ordered overlays, flag replacement/extension/deletion, ordinary four-way
peers, all 16 linear peers, nonrotating identities, mapgen subtype routing, and
clockwise local rotations. Unsupported OMT fields remain named in the content
definition and do not imply runtime admission.

Start selection matches the identity owned by each generated coordinate rather
than a global default. Exact, type, subtype, prefix, and contains matching are
supported. Runtime admission requires every possible target to have a candidate
in the durable initial bubble; character creation never generates unjournaled
terrain. Only a uniform one-identity bootstrap receives origin affinity, while
heterogeneous layouts retain their seeded shuffle. City constraints, mapgen
parameters, every placement/preparation flag, and starts excluding z=0 fail
closed. The production server still uses `sloc_lmoe`; character placement is
authoritative and retains deterministic multiplayer fallback.

## Measured evidence

- A heterogeneous two-identity fixture dispatches separate generators by
  coordinate, rotates a source marker at `(2,5)` to `(18,2)` together with its
  furniture and item, restricts start selection to the matching OMT, exposes
  that path through authoritative character creation, round-trips the snapshot,
  and rejects out-of-layout discovery without mutation.
- The shared heterogeneous start scenario produces identical state in direct,
  per-tick snapshot, SQLite recovery, and portable-replay modes.
- The persisted SQLite/replay worldgen fixture retains the complete bounded
  layout and its generated chunks. Snapshot admission rejects complete 2x2 OMT
  groups outside the owned coordinates and chunks on absent z-layers.
- Edge traversal produces an ordinary authoritative `Blocked` rejection while
  advancing the tick and leaving chunks unchanged. Remote-only start catalogs
  fail runtime admission before world mutation.
- The pinned C++ mapgen oracle characterizes all five OMT match modes, ordinary
  and linear mapgen routing/rotation, marker rotation, and static
  palette/nested phase ordering.
- The pinned C++ item-group oracle now characterizes collection/distribution
  RNG, count/charge ranges, raw/display damage, explicit variants, ammunition
  dressing, bounded semantic-witness searches for randomized discard/spill
  containment and the real `everyday_corpse` family, and inactive event
  tickets. The checked scenario pins ordered seed traces for all six first
  container orders and exact corpse traces for seed 1 and the first raw-damage
  4000 content boundary, preventing an aggregate-only implementation from
  passing. Rust now retains all six holiday qualifiers and applies the pinned
  default disabled policy without host time: inactive collection entries
  consume their roll, while inactive distribution tickets yield no item.
- The only checked canonical hash change is the item-flow state root:
  `ced77c1dd1cdaab7b30fbf202a15e0aae54548e5a4beb11b9b707417b6e94e11`
  under CanonicalStateV59 becomes
  `b5537199f17d36755d7f9dea392646222e55d1671f4107990d7b09b09957326b`
  under CanonicalStateV60. That scenario has no item groups; its tick, actor,
  inventory, ground item, and CanonicalEventsV18 trace are unchanged, isolating
  the difference to the intentional state-hash domain change.
- The extraction moves the complete 300-line canonical `ItemInstance`
  implementation without changing derives, field order, branches, mutations,
  errors, output order, or RNG. After normalizing only the newly required
  `pub(super)` visibility, the reviewer found the old and new definitions
  textually identical. `sim/lib.rs` is now 28,558 lines and `sim/items.rs` is
  599 lines; no wire, persistence, replay, event, or canonical-hash version
  changes.
- The server extraction moves the complete 333-line item-group normalization
  implementation from `main.rs` into `item_groups.rs`. After normalizing only
  the required `pub(super)` visibility, the old and new function bodies are
  textually identical. The binary root falls from 6,565 to 6,240 lines; the
  focused module is 349 lines. No runtime representation or behavior changes.
- Runtime evidence at verified commit
  `d76965c54d5fee3a081b2a7c860b94a750b92cdd` counts four admitted definitions
  through generation, authoritative interaction, persistence, and client
  access: 44 points. Exact production definitions receive
  zero four-mode credit because the current conformance paths use normalized
  semantic substitutes; those fixtures are not mislabeled as production
  evidence. The separate parser inventory counts 7,621 item groups, 9,520
  mapgen objects, 2,712 overmap terrains, and 150 starts but assigns them no
  runtime points merely for loading. The checked artifact, runtime versions,
  and every named source of evidence are bound byte-for-byte to that commit.

## Explicit boundaries

- The stored layout is genuinely coordinate-owned, but production population
  still repeats `lmoe_north`; it is not an upstream forest/city/road/river/
  special layout.
- The pinned z=0 regional base is `field`, but its mapgen cannot yet be admitted:
  the reachable `field -> everyday_corpse` item-group closure uses the general
  `damage` modifier, and later container semantics are also not canonical.
  Startup and its fixed-snapshot test reject that closure instead of dropping
  rare content.
- Adjacent overmaps, multiple generated z-levels, cities, forests, roads,
  rivers, specials, map extras, spawn groups, populations, zones, vehicles,
  and monsters from mapgen remain unavailable.
- Inside/outside connected-area scoring, start zones, opening/bashing
  reachability, NPC accommodation, `ALLOW_OUTSIDE`, `LONE_START`, and `BOARDED`
  remain unavailable and fail closed.
- Nested/update mapgen, mapgen parameters, multi-layer glyphs, weighted
  one-time fill, recursive regional targets, and unsupported positive-weight
  variants remain fail closed.
- Generating a remote start OMT inside the character-creation transaction is
  unavailable until that worldgen mutation is journaled and rolled back with
  the character transaction.

## Latest verification

Verified commit `d76965c54d5fee3a081b2a7c860b94a750b92cdd` is green for
formatting; workspace all-target/all-feature check; strict Clippy; warning-free
rustdoc; dependency boundaries; the parity ledger and runtime-progress gates;
astronomy; selected-content validation at unchanged manifest hash
`45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
content inventory; all three pinned C++ differential oracles (8/41/17
assertions); and 335 workspace tests plus doc-tests. An independent reviewer
audited the complete five-file server extraction against base
`d219c8c69290fc42ee697005ba1cae81ae469001`; one P3 visibility widening was
validated and fixed, and the final rescan found no P0-P3 issue. The durable
record is `docs/reviews/server-item-group-normalization-extraction.md`. Existing
nonblocking hardening opportunities remain indexed RLE identity lookup,
loader-local JSON byte caps for future mutable content packages, and the
explicit normalized Rust/C++ mapgen comparator named by the `oracle_pending`
state.

## Next dependency boundary

The central-file split is now an explicit prerequisite DAG rather than an
informal cleanup task. Current ownership surfaces are about 28.6K lines in
simulation `lib.rs`, 8.3K in protocol, 13K in persistence, 6.2K in the server
binary root, and 8.8K in the server library. Item-group planning and the
canonical item instance live in `sim/items.rs`; server item-group normalization
now lives in `server/item_groups.rs`. Shared item-prototype conversion remains
parent-owned, while simulation validators, materialization/conversion helpers,
ownership transfers, and item-bound activities remain incremental extraction
boundaries. The ledger separately tracks actor, combat, activity, monster,
canonical-state, protocol-domain, persistence-domain, and session/replication
extractions. Anatomy and EOC expansion depend on the relevant boundaries.

Implement the coherent item-group modifier/contained-item family needed by the
pinned `field` closure, using the checked oracle and all four recovery modes
before replacing the runnable LMOE population with the real default field
layer. Damage must preserve upstream raw scaling and RNG order; general wrappers
must retain nested stable identities and explicit overflow rather than flattening
contents.
Forest/city/road/river/special placement begins only after that base layer is
exact and green.
Batch the related modifier/containment wire and persistence representation into
one reviewed version increment where practical; do not bump either version for
mechanical module moves or unchanged encodings.
