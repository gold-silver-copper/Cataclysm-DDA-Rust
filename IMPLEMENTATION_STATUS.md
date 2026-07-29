# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Live checkpoint

- Verified green commit: `de09724a27064d293041ce1cf4df5e458ac403a7`
  (`Extract protocol item group domain`).
- Checkpoint binding: this docs-only commit binds the reviewed implementation
  above; no later subsystem is in progress.
- Active milestone: `regional-terrain-base`; the
  `protocol-domain-modules` milestone remains in progress with item-group
  ownership complete.
- Runtime: protocol 85, worldgen algorithm 2, persistence schema/minimum
  recoverable schema 63, replay format 3, CanonicalStateV61, and
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
  passing. Its direct fixed-count trace also pins exact downstream state after
  the item presentation seed, empty-variant selection, fit, and modifier damage
  phases. Rust now retains all six holiday qualifiers and applies the pinned
  default disabled policy without host time: inactive collection entries
  consume their roll, while inactive distribution tickets yield no item.
- Selected-content normalization now distinguishes unsigned count/damage ranges
  from signed charge ranges, including upstream's independent `-1` capacity
  sentinels. It retains modifier existence, variants, direct entry wrappers,
  modifier-owned containers, sealing, group wrappers, wrapper variants, and
  overflow policy instead of projecting them away. Strict `field` closure
  advances from `everyday_corpse.damage` and `everyday_gear.charges` to the exact
  next blocker, `civilian_phones_case.contents-group`.
- Protocol 85 carries a fixed-zero raw-damage marker on every admitted entry
  whose upstream loader constructed an `Item_modifier`. The planner consumes
  every direct leaf's presentation-seed draw, empty-variant selection, and
  unconditional fit draw, then evaluates that damage range before charge
  dressing. Explicit default-only modifiers are therefore represented without
  inferring source syntax. Variable-size items, nested-group modifiers,
  degrading vehicle parts, fouling guns, corpse and preloaded-magazine
  construction, temperature-bearing comestibles, constructor-owned state,
  default containers, nonzero damage, variants, sealing, and general
  containment fail closed; zero-chance magazine/ammunition dressing draws are
  retained only when their projected pocket shape is exact.
- The only fixed canonical hash change in this checkpoint is the item-flow state
  root: CanonicalStateV60
  `b5537199f17d36755d7f9dea392646222e55d1671f4107990d7b09b09957326b`
  becomes CanonicalStateV61
  `2aae0f859788b6e83bd4c03972f32a6a78963a63c1cedf5774b6b1e895e37820`.
  That scenario has no item-group catalog and its tick, actors, inventory,
  ground items, and CanonicalEventsV18 trace are unchanged, isolating the
  change to the intentional hash-domain version. The structural-bash fixture's
  fixed nail charge changes from 6 to 4 because the corrected constructor,
  variant, fit, and modifier phases advance the named RNG stream before charge
  dressing. That new output remains identical in direct, snapshot, SQLite, and
  portable-replay modes.
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
- The protocol extraction moves the complete item-group wire DTO, bounds,
  graph evaluator, and validation domain into `protocol/item_groups.rs` while
  retaining every existing crate-root public path through re-exports. The
  424-line evaluator/validation block is textually identical after normalizing
  only the required `pub(super)` closure-check visibility. `protocol/lib.rs`
  falls from 8,532 to 8,008 lines and the focused module is 541 lines. Field,
  variant, and derive order are unchanged, so protocol 85, schema 63, replay 3,
  CanonicalStateV61, and all canonical hash domains remain unchanged.
- Runtime evidence remains four admitted definitions
  through generation, authoritative interaction, persistence, and client
  access: 44 points. Exact production definitions receive
  zero four-mode credit because the current conformance paths use normalized
  semantic substitutes; those fixtures are not mislabeled as production
  evidence. The separate parser inventory counts 7,621 item groups, 9,520
  mapgen objects, 2,712 overmap terrains, and 150 starts but assigns them no
  runtime points merely for loading. Protocol 85 corrects RNG phase for the
  already-counted structural bash definition; it does not claim a new
  runtime-usable definition.

## Explicit boundaries

- The stored layout is genuinely coordinate-owned, but production population
  still repeats `lmoe_north`; it is not an upstream forest/city/road/river/
  special layout.
- The pinned z=0 regional base is `field`, but its mapgen cannot yet be admitted.
  Content normalization reaches `civilian_phones_case.contents-group`; runtime
  storage still lacks nonzero raw damage, item variants, modifier contents,
  general containment, overflow, and ammunition/magazine dressing. Startup and
  its fixed-snapshot test reject that closure instead of dropping rare content.
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

Verified commit `de09724a27064d293041ce1cf4df5e458ac403a7` is green for the complete
30-test protocol suite, formatting, strict all-target/all-feature Clippy,
rustdoc with warnings denied, workspace all-target/all-feature check, the full
338-test workspace suite, and doc-tests. Dependency-boundary, parity-ledger,
weighted-runtime, astronomy, selected-content manifest, generated-inventory,
JSON, and diff gates pass. The pinned default server still loads all 7,621
item-group definitions and retains the audited 521 furniture bashes; runtime
evidence remains four definitions and 44 weighted points.

The 424-line evaluator/validation block, 94-line DTO block, and all five bounds
are textually identical to the verified parent after normalizing only the
required `pub(super)` closure-check visibility. Every formerly public item is
re-exported at the same crate-root path. The pinned C++ pocket, item-group, and
mapgen oracles pass 8, 42, and 17 assertions. The fixed upstream checkout is
clean at `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`. An independent reviewer audited
the exact five-file patch against `e58f03793bda51db257a0374b8905fe694767c01`,
found no P0-P3 issue, and independently reproduced the gates. The durable
scoped record is `docs/reviews/protocol-item-group-domain-extraction.md`.

## Next dependency boundary

The central-file split is now an explicit prerequisite DAG rather than an
informal cleanup task. Current ownership surfaces are about 28.6K lines in
simulation `lib.rs`, 8.0K in protocol, 13K in persistence, 6.3K in the server
binary root, and 8.8K in the server library. Item-group planning and the
canonical item instance live in `sim/items.rs`; server item-group normalization
now lives in `server/item_groups.rs`; protocol item-group DTOs, bounds, and
validation now live in `protocol/item_groups.rs`. Shared item-prototype
conversion remains parent-owned, while simulation validators,
materialization/conversion helpers, ownership transfers, item-bound activities,
and the remaining command/state/event protocol domains remain incremental
extraction boundaries. The ledger separately tracks actor, combat, activity,
monster, canonical-state, protocol-domain, persistence-domain, and
session/replication extractions. Anatomy and EOC expansion depend on the
relevant boundaries.

Finish the coherent item-group modifier/contained-item family needed by the
pinned `field` closure. The next exact content dependency is general
`contents-group` normalization; the next runtime dependency is storing nonzero
raw damage and variants on item state, followed by general wrapper contents
with nested stable identities and explicit overflow. Use the checked oracle and
all four recovery modes before replacing the runnable LMOE population with the
real default field layer.
Forest/city/road/river/special placement begins only after that base layer is
exact and green.
Protocol 85/schema 63 is the fixed-zero modifier-presence batch. Batch the
remaining item-state damage/variant representation with its persistence changes,
and batch general containment separately; do not bump versions for mechanical
module moves or unchanged encodings. The item-group protocol ownership
prerequisite is now green and reviewed; the next representation increment must
remain within the coherent field-closure family.
