# CDDA Porting Matrix

Updated 2026-08-02 at implementation commit `a3e6bfc` and contract
`P142/S119/R4/CS117/CE32`.

Status values are `not investigated`, `specified`, `implementing`, `implemented
but unverified`, `behaviorally verified`, `multiplayer adaptation`, and `out of
scope`. “Implemented but unverified” means production code exists but one or
more required long gates are explicitly deferred; it is not a parity claim.

| Subsystem or content category | Status | Evidence / next boundary |
| --- | --- | --- |
| Architecture and authority | Multiplayer adaptation | Bevy 0.19.0 is client-only. Server/simulation are plain Rust; Tokio owns runtime work and Iroh 1.0.3 owns networking and endpoint identity. Clients submit intentions and never mutate canonical state. |
| Iroh identity and authorization | Implementing | Passwordless endpoint enrollment, persistent bindings, recovery locking, role checks, and authenticated gameplay/admin paths exist. Release-grade hostile-input, platform, and recovery gates remain deferred. |
| Stable identity, persistence, and replay | Implementing | Typed 128-bit stable IDs, SQLite WAL/journaling, snapshots, recovery, backups, and replay format 4 exist. Current `P142/S119/R4/CS117/CE32` comprehensive recovery/replay verification is deferred. |
| Simulation clock and disconnected actors | Implementing | The authoritative 20 Hz world advances with zero clients. Disconnected characters remain present, vulnerable, and governed by bounded survival behavior. Broader analytical inactive-region catch-up remains planned. |
| Item containment and item groups | Behaviorally verified | General container ownership, nested stable IDs, pockets, sealing/overflow, snippets/variables, charge storage, default containers, flexible collapse, temperature foundations, and deterministic item-group generation are complete at their demonstrated baseline. New semantics remain extensions and unsupported shapes fail closed. |
| Inventory, equipment, and ordinary item use | Implementing | Client-accessible pickup/drop, nested removal, wielding, reload, consumption, powered-item, crafting, study, and disassembly paths exist. Canonical item temperature is processed for actor, NPC, ground, and vehicle-cargo items, and shelf-life rot advances at the pinned temperature-dependent rate; ground and vehicle-cargo items are removed when they rot away. Rot-bearing results remain fail-closed at the crafting and construction constructor boundaries, so only item-group-sourced perishables and corpses carry rot today. Full CDDA item/use-action coverage, rot for carried inventory removal, custom freezing, and power networks remain incomplete. |
| World chunks and overmap foundations | Behaviorally verified | Atomic 24x24 mapgen, explicit OMT identities, start selection, regional field, deterministic cities, and connected roads have recorded completion evidence. Chunk storage remains 12x12 submaps. |
| Rivers, specials, and spawning | Implemented but unverified | Persistent rivers/bridges, overmap specials, and content-driven monster populations have generalized production wiring. Their consolidated mapgen/content/oracle/recovery gates are deferred. |
| Anatomy, armor, effects, and ordinary combat | Implemented but unverified | Canonical body parts, armor absorption, typed damage, effects, dodge, stamina, natural healing, medical healing, melee, death, corpses, and revival are wired through authoritative state. Comprehensive family verification is deferred. |
| EOCs and use actions | Implemented but unverified | Bounded conditions/effects, transitive EOCs, actor variables and math, delayed/recurring scheduling, world-event dispatch, confirmations, item activation, transforms, and several use actions are wired. Dynamic values and broader generic interaction consumers remain. |
| Generic server-driven interactions | Implementing | Stable server-owned prompt IDs, bounded choices, cancellation, timeout/stale response rejection, and medical body-part selection exist. Inventory/dialogue/target/activity consolidation is still incomplete. |
| Monsters | Implemented but unverified | Ordinary perception/movement/combat/death plus natural armor, attack effects, melee/bite/leap, monster-alpha, gun/burst/projectile, polymorph, target-context EOC, multi-strike, and shaped spell programs are implemented at a hard boundary. Rare unsupported target/application/immunity branches remain explicit and fail closed; expensive family checks are deferred. |
| NPCs, dialogue, missions, and factions | Implementing | Stable NPC/faction state, leased dialogue, conditional responses/effects, mission lifecycle/order/callbacks, owned-world item turn-ins, generated NPC actors, vulnerability, ordinary attitudes/pathing/combat, and client mission display exist. Final effect/faction closure and independent reviews remain incomplete. |
| Vehicles | Implementing | Static vehicles, stable parts, boarding/unboarding, cargo, doors, client controls, and conservative straight leg-muscle motion are implemented. Powered propulsion, full steering/pivot, controller leases, fuel/battery, mass/traction/collision/damage, repair, and part mutation remain planned; the ordinary-vehicle gate is deferred. |
| Weather, fields, and environment | Implementing | Deterministic weather, local wind, terrain/vehicle shelter, dangerous-weather interruption, client observations, field contact/effects, gas spread/dissipation, precipitation, and tick-based downtime processing exist. Agriculture, broad inactive-region processing, review, and consolidated gates remain. |
| Client presentation and ordinary access | Implementing | The macOS-compatible Bevy client reaches movement, combat, inventory, activities, dialogue/missions, weather, vehicle interactions, and bounded controls through server-authoritative commands. Complete UI/accessibility and Windows/Linux runtime verification remain. |
| Full CDDA gameplay and release | Implementing | The repository is not yet a fully playable CDDA release. NPC/social closure, powered vehicles, agriculture/inactive catch-up, broader content families, release packaging, cross-platform CI, security/fuzzing, performance, and the 16-player soak remain. |

Definition-level state and exact fail-closed branches are maintained in
`docs/parity-ledger.json`. `docs/runtime-progress.json` counts only definitions
with recorded runtime evidence and intentionally gives no new credit to the
current deferred families.
