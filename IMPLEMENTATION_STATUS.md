# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Live snapshot

- Current checkpoint: this commit; green parent `e75f20b` (`Extract server
  worldgen normalization`).
- Active milestone: `mapgen-overmaps`.
- Runtime versions: protocol 82, persistence schema/minimum recoverable schema
  60, replay format 3, CanonicalStateV58, CanonicalEventsV18.
- Conformance versions: scenario 7, observation 6.
- Supported hosts: macOS, Linux, and Windows. The standalone server has no Bevy
  dependency; Bevy 0.19 is confined to the graphical client. Networking and
  endpoint-owned authentication use iroh 1.0.3.
- This checkpoint contains the audited start-location/overmap-selection
  increment described below.

The detailed Protocol 81-era status and earlier subsystem history are archived
in [docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md](docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md).

## Runnable behavior

The repository builds a persistent server-authoritative multiplayer foundation,
not yet a complete playable CDDA port. A Bevy client can enroll through iroh,
create/select a persistent character, connect to the standalone server, move,
interact with visible terrain/items, fight the current admitted creatures, and
use the implemented crafting, reading, disassembly, construction, containment,
survival, recovery, and replay paths. Characters remain present and vulnerable
while disconnected, and the world clock continues without connected players.

Fresh worlds materialize 36 complete 24x24 OMTs/144 canonical chunks from the
pinned `lmoe` local-map definition. Generation is atomic per four-submap OMT,
coordinate-seeded, traversal-order independent, snapshot/replay stable, and
bounded by one 4,096-ID reservation for admitted loot. The server now loads the
selected start-location family, persists an explicit normalized overmap identity
and start selector, and creates new characters in a deterministically selected
matching OMT. While every coordinate shares the bootstrap identity, the origin
OMT remains first so the character can reach the fixed starter loadout and
encounter; deterministically shuffled matching OMTs are overflow capacity if
that cell fills.

The start-location loader implements strict inheritance and load-order patches;
EXACT, TYPE, SUBTYPE, PREFIX, and CONTAINS terrain matching; source-ordered
targets; city size/distance and z intervals; flags; and retained mapgen
parameters. Runtime admission is deliberately narrower: the current server
uses pinned `sloc_lmoe` only and rejects city-dependent starts, parameterized
targets, preparation/placement flags, or starts excluding z=0. The pinned
`lmoe_north`/`lmoe`/`lmoe` identity is explicit rather than inferred.

Renderer-independent conformance now covers start selection and two-character
placement through direct simulation, per-tick snapshot restoration, SQLite
recovery, and portable replay. Existing character creation exposes this path to
the client without granting the client authority over location choice.

## Explicit boundaries

- Every generated coordinate still shares the explicit `lmoe_north` bootstrap
  identity. There is no persistent coordinate-owned overmap terrain layout yet.
- Cities, roads, rivers, forests, specials, regional overmap settings, overmap
  populations, spawn groups, zones, vehicles, and multiple generated z-levels
  remain unavailable.
- Local start-tile placement currently uses passability and occupancy within
  the selected 24x24 OMT. Upstream inside/outside classification, connected-area
  rating, start-point zones, bash/open reachability, NPC accommodation, and
  `ALLOW_OUTSIDE`/`LONE_START`/`BOARDED` behavior are not claimed; definitions
  requiring their explicit flags fail closed.
- Nested/update mapgen, mapgen parameters, and positive-weight unsupported
  variants remain fail closed. The ordinary `field` mapgen is still unavailable
  because its corpse/container loot cannot yet enter canonical item state.
- Before admitting an item-bearing server default, reservation management must
  cover the full worst-case discovery allocation rather than relying on the
  current 512-ID refill threshold.
- Post-snapshot journal replay of traversal-triggered boundary generation needs
  an explicit conformance case before broader on-demand overmap population.

## Current increment evidence

- Pinned selected content resolves 101 start locations; `sloc_lmoe` is the sole
  current runtime start. `sloc_shelter_safe` parameters, forward-inherited
  `sloc_house_boarded`, and unbounded `sloc_road` distance are retained and
  characterized without being admitted.
- The real C++ mapgen oracle already characterizes all five OMT matching modes,
  rotatable and linear identities, point rotation, and static palette/nested
  phase ordering against the fixed upstream tree.
- Local verification is green: `cargo fmt --all -- --check`, workspace
  all-target/all-feature `cargo check`, strict workspace `clippy`, warning-free
  rustdoc, dependency-boundary and parity-ledger checks, selected-content
  validation, content inventory, astronomy characterization, all three pinned
  C++ differential oracles (8 pocket, 34 item-group, and 17 mapgen assertions),
  and 322 workspace tests. The suite includes all nine shared conformance tests
  and the full-content server normalization case.
- Fresh independent review found that randomized first-OMT selection could
  strand new characters from the fixed starter slice. Origin affinity and
  direct/four-mode conformance assertions now prevent that regression. A
  suggested city-distance rejection was not applied: pinned C++ requires a
  city only for positive minimum city size or maximum distance below 180, and
  the implementation and tests preserve that exact rule. No P0/P1 finding
  remains.
- The final default-parallel gate exposed two real-Iroh fixtures that left the
  simulation's bounded output queue undrained during slow endpoint setup. Both
  now use the existing test acknowledger; the subsequent 22-test server run and
  complete 322-test workspace run are green without serializing network tests.
- The checked item-flow state hash changed from
  `088d6a3945e6f1e59b39021ea1a4986ad22f494e07c18a21c52f2d9c28540f8e` to
  `68cf369b8e35b9b2c7613d273436c0a202c723927113d629e9c1a34a9a56e0a1`.
  The scenario has no worldgen catalog, so encoded state shape is unchanged;
  the change is exactly the intentional CanonicalStateV57-to-V58 domain
  separation. Its event-trace hash remains unchanged.

## Next dependency boundary

Complete a persistent coordinate-owned overmap terrain-selection engine before
admitting another mapgen example: load normalized OMT identities and the
overmap-special relationships needed to place a coherent layout, retain
unsupported constraints explicitly, characterize selection against pinned C++
where practical, persist/replay the layout, and then let the existing generic
start matcher select among genuinely different OMTs. Do not begin spawn groups
or another subsystem until this increment is fully green, reviewed, documented,
and checkpointed.
