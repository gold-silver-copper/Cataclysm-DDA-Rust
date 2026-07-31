# Regional-field integration review

## Fixed scope

- Pinned upstream: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`, tree
  `210f31db2e8b2f0caed1809f1a66781859f9d129`.
- Family base: `0d883b58d60c12540e3bcf05315606a2e4abe1a3`.
- Main implementation: `fbf6313b2cf6032bb376e64bc33c0cdfd310afc9`.
- Reviewed implementation and focused fixes:
  `38ff729d97e073473bb26b4a9b8b28ac573ed29a`, tree
  `896dc917e41b015e20c95a687c2b78863fadac19`.
- Review worktree: `/tmp/cdda-regional-field-final-review.fjYG5y`, detached at
  the reviewed commit and clean throughout review.
- Representation stayed at protocol 95, persistence schema/minimum schema 73,
  replay format 3, CanonicalStateV71, CanonicalEventsV18, worldgen algorithm 2,
  scenario format 8, and observation format 6.

The fixed review covered the complete 24-file family diff: production field
worldgen, recursive regional terrain/furniture, item-group closure, general
containment and multi-pocket selection, rot/corpses/insulation, stable-ID
preflight, conformance selectors, real-Iroh sessions, SQLite recovery, portable
replay, generated oracle evidence, and live ledgers.

## Findings and resolutions

1. **P1 — the acceptance claim described two clients but only drove two
   renderer-free actors.** The field-owned acceptance now creates two distinct
   Iroh identities and accounts, selects two persisted characters through the
   production session handler, consumes event/snapshot streams, and submits
   server-authoritative pickup, nested removal, and movement commands. Both
   control streams finish cleanly and both actors become durably disconnected.
2. **P1 — production characters could not be persisted.** A newly spawned
   field character carries legitimate 60-tile-radius terrain memory, exceeding
   the obsolete 4 KiB spawn-record cap. The cap now matches the existing 32
   MiB canonical snapshot bound. This remains fail-closed and does not alter
   the stored record shape.
3. **P1 — a production-field snapshot could time out during a valid commit.**
   Snapshot compression and SQLite commit can exceed the generic five-second
   request deadline in debug builds. Snapshot writes and receipts now use a
   distinct bounded 30-second deadline; ordinary persistence/auth calls retain
   five seconds.
4. **P3 — live status called batched snapshot recovery “per-tick.”** The status
   now says declared-boundary snapshot recovery. Every authoritative tick and
   journal input remains retained; only the expensive reconstruction boundary
   is batched.

The hash audit found no new canonical drift: the fixed conformance scenario
retains state hash
`1ac803cf46569081817639f939acaa180e6628487f00e892a2183439aba21e97`
and event hash
`40b05c278a6a6af9055e6dd9a3bc0acf4e920c9f690f7e0b1eea9a678726eda7`.
The only earlier state-hash change was traced to exact thermos pocket and 10.0
insulation admission; event representation stayed unchanged. No wire or stored
shape changed, so a protocol/schema bump would have been incorrect.

The final detached pass rechecked stable-ID atomicity, content graph bounds,
spawn-pocket order against pinned `find_pocket_for`, account/actor ownership,
shared session registry use, persistence sequencing at the checkpoint barrier,
hostile record bounds, and clean disconnect handling. It found no remaining
confirmed P0, P1, or P2 defect.

## Verification

The main implementation passed the consolidated workspace gate recorded in
`IMPLEMENTATION_STATUS.md`: 421 tests, strict all-target/all-feature Clippy,
docs/rustdoc, content gates, all four conformance modes, and the 8/272/1,179
pocket/item-group/mapgen C++ oracle assertions with direct Rust comparison.

The focused `38ff729` fixes then passed:

- formatting and diff checks;
- affected server/persistence all-target checks and strict Clippy;
- all 40 persistence-library tests;
- all 22 server-library tests, including four existing real-Iroh cases;
- the isolated two-client production-field path in 414.60 seconds, including
  persistent accounts/characters, nested loot interaction, disconnect, large
  snapshot commit, restart, SQLite recovery, and encoded portable replay.

The temporary development short-circuit used to isolate that expensive path
was removed before commit. Unchanged four-mode hashes remained pinned by the
preceding consolidated implementation run rather than being redundantly rerun
during each networking fix.

## Residual boundary

The production gate is intentionally expensive and belongs at milestone or
release checkpoints, not ordinary city-development iterations. The Iroh path
selects pre-provisioned persistent characters; production character creation
uses the same now-admitted spawn snapshot and remains covered by the existing
real-Iroh creation test. City placement is the next runnable family. Roads,
rivers, specials, and spawning must not be folded into city placement until
the city family itself is generalized, admitted, and green.
