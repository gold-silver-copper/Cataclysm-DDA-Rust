# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Live checkpoint

- Current checkpoint: this commit; green parent `98d413d` (`Add authoritative
  start location selection`).
- Active milestone: `mapgen-overmaps`.
- Runtime: protocol 83, worldgen algorithm 2, persistence schema/minimum
  recoverable schema 61, replay format 3, CanonicalStateV59, and
  CanonicalEventsV18.
- Conformance: scenario 7 and observation 6.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only; the server and
  simulation are plain Rust. Networking and endpoint-owned authentication use
  iroh 1.0.3.

Protocol 81-era and earlier narration is archived in
[docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md](docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md).
Protocol 82 is summarized in the repository history and architecture record;
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
- The only checked canonical hash change is the item-flow state root:
  `68cf369b8e35b9b2c7613d273436c0a202c723927113d629e9c1a34a9a56e0a1`
  under CanonicalStateV58 becomes
  `ced77c1dd1cdaab7b30fbf202a15e0aae54548e5a4beb11b9b707417b6e94e11`
  under CanonicalStateV59. That scenario has no worldgen state; its tick,
  actor, inventory, ground item, and CanonicalEventsV18 trace are unchanged,
  isolating the difference to the intentional state-hash domain change.

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

Local implementation gates are green: formatting; workspace all-target,
all-feature check; strict Clippy; warning-free rustdoc; dependency boundaries;
parity ledger; astronomy table; selected-content validation at the unchanged
manifest hash `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
content inventory; and all three pinned C++ differential oracles (17 mapgen, 34
item-group, and 8 pocket assertions); and 331 workspace tests. Independent
review found and the final tree fixes bounded-edge tick failure, unjournaled
remote-start generation, out-of-layout snapshot admission, three OMT loader
parity errors (reset definitions, non-inherited `uniform_terrain`, and
string-only abstracts), resource-bound gaps, and heterogeneous origin bias. A
fresh complete-diff rescan reported no remaining P0/P1 or lower-severity
findings. Future hardening may index highly fragmented RLE identity lookup and
add loader-local JSON byte caps before mutable content packages are admitted.

## Next dependency boundary

Finish this checkpoint first. The next parity increment must inventory and
implement the coherent item-group modifier/contained-item family needed by the
pinned `field` closure, with C++ characterization and all four recovery modes,
before replacing the runnable LMOE population with the real default field
layer. Forest/city/road/river/special placement begins only after that base
layer is exact and green.
