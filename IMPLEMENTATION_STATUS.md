# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Live checkpoint

- Verified green commit: `37bb6f473153d3dc320f055c5ae35330b48b38a1`
  (`Address mapgen checkpoint review findings`).
- Active milestone: `regional-terrain-base`. The three runnable mapgen/overmap
  families now have their reusable direct Rust-to-C++ comparator and complete
  six-part evidence; field admission resumes at the retained tool-charge
  boundary.
- Verified representation: protocol 87, worldgen algorithm 2, persistence
  schema/minimum recoverable schema 65, replay format 3, CanonicalStateV63, and
  CanonicalEventsV18.
- Conformance: scenario 7 and observation 6.
- Hosts: macOS, Linux, and Windows. Bevy 0.19 is client-only. The server and
  simulation are plain Rust; iroh 1.0.3 owns networking and authentication.

Mapgen/overmap progress is split into durable submilestones:

- `atomic-static-mapgen`: `complete`; pinned static-template characterization,
  the generalized 24x24 engine, direct Rust/C++ execution, four recovery modes,
  admitted LMOE runtime content, and the authoritative client view are linked
  in the checked completion evidence.
- `omt-identities-routing`: `complete`; the same direct run covers finalized
  rotatable and linear identities while conformance covers durable coordinate
  routing and bounded recovery.
- `start-location-selection`: `complete`; the pinned production `sloc_lmoe`
  definition, chosen target, constraints, and matching candidate set compare
  directly. Normal server-authoritative character creation and the explicitly
  Rust-specific multiplayer occupied-tile fallback agree through all four
  recovery modes.
- `regional-terrain-base`: `in_progress`; the containment engine removes the
  `civilian_phones_case` blocker. The exact field closure now stops at
  `accesories_personal_unisex_child` because `wearable_light` tool charge
  modifiers still require an unrepresented ammunition-loading path.
- `overmap-cities`, `overmap-roads`, `overmap-rivers`, `overmap-specials`, and
  `mapgen-spawning`: `planned`.

Protocol 81-era and earlier narration is archived in
[docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md](docs/history/IMPLEMENTATION_STATUS_PROTOCOL_81.md).

## Runnable behavior

This is a persistent server-authoritative multiplayer foundation, not yet a
complete CDDA port. The Bevy client can authenticate with iroh, create/select a
persistent character, traverse durable generated terrain, fight admitted
creatures, manipulate visible items, smash admitted structures, and use the
implemented crafting, reading, disassembly, construction, survival, recovery,
and replay paths. Characters remain present and vulnerable while disconnected,
and world time continues with zero players.

Fresh worlds retain a bounded 180x180 z=0 overmap layout. Production still
repeats the exact admitted `lmoe_north` generator so the bootstrap remains
playable. Coordinate-owned OMT identities, atomic 24x24 generation, shared
terrain/furniture/item rotation, matching start selection, durable chunks, and
ordinary blocked layout edges are implemented. The real default `field` layer
is not admitted until its entire loot/containment closure is exact.

## Verified Protocol 87 containment family

- One generalized planner covers whole-group, direct-entry, and modifier-owned
  wrappers; `contents-item` and `contents-group`; sealing; spill/discard;
  snippets; typed variables; and recursive preorder stable-ID allocation.
- Strict ITEM projections retain physical/E-file spawn pockets, phase,
  count-by-charge mass and volume, longest side, restrictions, watertight and
  sealable state, wrapper capacity, and represented item flags. Unsupported
  pocket shapes and ambiguous material-derived softness remain explicit and
  fail closed.
- Capacity checks use recursive weight, volume, and length. `NO_DROP`,
  `REDUCED_WEIGHT`, explicit `SOFT`/`HARD`, liquid stacking, E-file exclusion,
  item-or-flag restrictions, `NO_UNWIELD`, and exact full-container sealing are
  characterized. Reserved physical variables cannot overwrite canonical
  weight or volume.
- Charge modifiers preserve constructor/dressing RNG order, clamp liquids and
  count-by-charge items to at least one even through outer named groups, and
  apply modifier-container capacity/default liquid fill before insertion.
- The pinned item-group oracle has 65 exact assertions, including representative
  traces and boundary/downstream-RNG witnesses rather than aggregate ranges
  alone. The server normalization admits the complete
  `civilian_phones_case` closure and 524 furniture-bash definitions.
- The structural-bash conformance path now generates a sealed rigid wrapper
  with contained drops and proves its nested ownership through direct,
  per-tick snapshot, SQLite, portable replay, and the ordinary Bevy item view.

## Measured progress

Runtime evidence remains four core definitions and 44 weighted points. Each
counted definition is generated, authoritatively interacted with, persisted, and
client-accessible. No production definition receives four-mode credit from a
normalized semantic substitute. The three newly normalizable furniture bashes
earn no points yet because the current playable LMOE mapgen does not place
them. The synthetic heterogeneous conformance catalog proves all four engine
recovery modes but does not award production-definition credit to the admitted
mapgen generator, OMT identity, or start location. The structural-bash item
group also lacks production four-mode credit.
Parser inventory remains separate: 7,621 item groups, 9,520 mapgen
objects, 2,712 OMTs, and 150 starts earn no runtime credit merely for loading.

The independently checked ordinary-gameplay denominator is split by source:
core DDA has 13,865 target definitions and 263,435 possible weighted points,
with 44 earned (0.0167%); selectable bundled mods have 5,967 target definitions
and 113,373 possible points, with zero earned. The bundled universe is the
union of nonobsolete pinned mods that participate in at least one valid
new-world selection; mutually exclusive configurations still contribute their
distinct playable definitions. Ordinary playable loops remain listed separately
from parser and weighted coverage.

Current ownership sizes are 29,339 lines in `sim/lib.rs`, 3,265 in
`sim/items.rs`, 9,722 in `protocol/lib.rs`, 1,192 in
`protocol/item_groups.rs`, 13,064 in persistence, 8,791 in the server library,
and 1,111 in server item-group normalization. Against the green implementation,
the containment family primarily grows the three bounded owners:
`sim/items.rs` +2,115 net lines, `protocol/item_groups.rs` +552, and
`server/item_groups.rs` +446. Central growth is limited to canonical/wire
integration: `sim/lib.rs` +643, `protocol/lib.rs` +1,542, server `lib.rs` +15,
and persistence +6. The protocol exception is large because item snapshots and
their validators have not yet been mechanically extracted; after this
checkpoint, further item-group behavior has a zero-growth budget in central
`lib.rs` files unless a review record identifies an unavoidable schema-only
integration. New behavior belongs in the three bounded owners.

Actors, combat, activities, monsters, canonical state, remaining protocol
domains, persistence responsibilities, and sessions/replication remain explicit
mechanical extraction milestones before anatomy or EOC expansion.

## Explicit boundaries

- Production overmap population is still LMOE, not an upstream regional
  forest/city/road/river/special layout.
- Flexible physical spawn pockets, arbitrary player-driven general containment,
  material-derived softness, and unprojected constructor/pocket semantics remain
  unavailable. They are retained explicitly and rejected rather than guessed.
- The real field base is not normalization-ready yet: its next exact retained
  blocker is the `wearable_light` tool charge path. It remains outside the
  production surface until the full closure, exploration/loot client proof,
  and fixed-tree review are green.
- Adjacent overmaps, additional generated z-levels, cities, forests, roads,
  rivers, specials, extras, spawn groups/populations, zones, vehicles, and
  mapgen monsters remain unavailable.
- Rich start placement/scoring, nested/update mapgen, mapgen parameters,
  multi-layer glyphs, weighted one-time fill, and recursive regional targets
  remain fail-closed.
- Remote start generation remains unavailable until its worldgen mutations are
  journaled atomically with character creation.

## Latest verification

The exact verified implementation commit
`37bb6f473153d3dc320f055c5ae35330b48b38a1`, parent
`6da2b7a21fa5f595c596fefa7535cf2a1f5a116e`, passes formatting, all-target
workspace checking, strict Clippy, 373 workspace tests plus doc-tests, and
warning-free rustdoc. All six dependency/parity/progress/astronomy/content
gates pass; runtime progress remains four definitions and 44 points. The three
pinned C++ oracles pass 8 pocket, 65 item-group, and 1,179 mapgen assertions. The
production content test confirms exactly 524 admitted furniture bashes and the
7,992-file manifest hash
`45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`.

The comparator loads the pinned production OMT and start-location registries,
runs the Rust 24x24 `WorldState` generator, and compares eight match witnesses,
eight concrete rotatable/linear identities, every cell in an exact 24-row
terrain/furniture trace, and the production `sloc_lmoe` target, constraints,
fixed candidates, matching subset, and selected candidate with C++. It exposed
and corrected the stale assumption that a linear peer rotates by the requested
compass label: concrete `road_ns` uses rotation 0, and `road_ew` uses rotation
3. Multiplayer occupied-tile fallback remains a Rust-specific adaptation and
is proven through direct, per-tick snapshot, SQLite, and portable replay.

The first exact-commit review of `6da2b7a21fa5f595c596fefa7535cf2a1f5a116e`
confirmed one P1 and two P2 findings: the start milestone lacked a real start
observation, synthetic conformance definitions had been awarded production
four-mode credit, and a manifested 16-byte translation fixture was ignored by
the repository. The review/fix commit adds the direct production start
observation, restores the honest 44-point score, and force-tracks the exact
upstream `INVALID_RAND.mo` fixture.

The final independent review used clean detached worktree
`/tmp/cdda-mapgen-final-review.9F0RwO`, fixed tree
`4551187dfc6992ee63ff11d5d6dbde8bef0dc17a`, cumulative patch ID
`7a7d1fdfac24417e73215cda94f07b245b07e2a8`, and review/fix patch ID
`406ced6df0b58daf20cd4f7e67f2c1ae369a9625`. It reviewed the complete 12-file
family diff and 9-file fix delta, reproduced the pristine content and direct
oracle gates, and found no remaining P0-P3 issue. Scope, findings, resolutions,
verification, rejected concerns, and residual limitations are recorded in
[docs/reviews/mapgen-direct-comparator-completion.md](docs/reviews/mapgen-direct-comparator-completion.md).

The only checked canonical fixture hash changed from
`8f8710e06937a50c14bcad35a17dbc41a059128061f4be9316c4c6449358dc66` to
`80e072e755e68be0aad782132f7118f4269b5f664ead99bc50a1b1cd8b27d335`.
This is the expected CanonicalStateV62-to-V63 domain change plus serialized
containment defaults; the tick, actors, inventory, ground items, commands, and
CanonicalEventsV18 trace/hash remain unchanged. No other checked canonical hash
was edited. The direct, per-tick snapshot, SQLite, and portable-replay modes all
reproduce this representation.

The fixed upstream checkout remains
`4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
`210f31db2e8b2f0caed1809f1a66781859f9d129`.

## Next dependency boundary

Admit the real field base and demonstrate ordinary client
exploration and loot. Forest/city/road/river/special placement starts only after
that playable base is exact and green; anatomy and EOCs remain behind the
listed modularization milestones.
