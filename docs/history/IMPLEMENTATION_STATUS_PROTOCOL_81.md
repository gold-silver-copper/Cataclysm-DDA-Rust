# Implementation Status

Upstream baseline: `4dfd36038b16650dc1b5cb9d79a3e42363174b05`

## Current milestone

Playable network foundation plus pinned-content crafting, book-study,
disassembly, strict furniture/terrain construction activities, canonical
creature blood, ordinary corpse/revival, and the first content-derived monster
vision, terrain-costed movement, stumbling, last-seen-goal, structural-bashing,
sound-driven monster-pursuit, terrain-door-opening, deterministic
obstacle-routing, route-planned bashing, broad safe furniture-bashing, and final
wooden-door-frame slices plus initial player-controlled smashing: close
persistent iroh identity, recovery, and audit
guarantees while accelerating toward generalized subsystem parity. The active
machine-readable milestone is mapgen and overmaps. Protocol 81 now drives fresh
and traversed terrain through strict pinned 24x24 JSON mapgen, atomic 2x2-submap
cells, coordinate-owned RNG, retained regional substitutions, and canonical
bounded worldgen catalogs. The server repeats the pinned `lmoe` surface until
real overmap layout and start-location selection exist. Unsupported mapgen
phases and the ordinary `field` corpse-loot closure remain fail-closed; the next
boundary is persistent overmap terrain selection, starts, and spawn rules.

## Runnable behavior

- A limited multiplayer foundation slice is runnable; it is not a CDDA-complete
  gameplay release.
- The versioned `cdda-conformance` scenario DSL has checked-in final-state and
  semantic expectations and proves both basic item flow and explicit indexed
  multi-well reloads through direct simulation, per-tick snapshot restore,
  SQLite recovery, and portable replay. `docs/parity-ledger.json` supplies an executable,
  version-bound, acyclic dependency DAG for generalized subsystem work.
- A development-only C++ differential-oracle command verifies the exact pinned
  upstream commit and Git tree, exports it into ignored `target/`, and invokes
  real upstream behavior through two minimal oracle-only `cata_test` adapters.
  Strict version-1 JSON scenarios cover `item_pocket::can_contain` shorter,
  equal, and longer maximum-length boundaries plus item-group collection order
  and RNG consumption, distribution interval boundaries, fixed/ranged count
  and charges (including zero-to-one clamping), and nested shared-RNG behavior.
  Unknown fields or any exact observation drift reject. Reads enforce their
  byte cap, each runtime directory is
  self-cleaning, and reusable binaries require exact cache identity plus a
  matching BLAKE3 digest or the export is rebuilt. One cross-process exclusive
  lock covers build and execution, while runtime `data/` is freshly exported
  from the pinned commit on every run. The initial upstream core build is
  intentionally not part of the fast workspace test gate; every additional
  kernel still requires an explicit versioned adapter.
- The ITEM loader preserves every inherited `pocket_data` object and source
  index without claiming unsupported runtime behavior. Strict MAGAZINE and
  MAGAZINE_WELL projections retain explicit pocket indices/IDs, reject extra
  behavior, and generalize the server's storage admission beyond a
  battery-only helper. Six reversible detachable-tool definitions now use
  modeled unload-before-disassembly instead of the prior empty-charge gate.
- Protocol 78/schema 56/CanonicalStateV54/CanonicalEventsV17 add explicit,
  server-authoritative stable-ID removal from integral magazines and detachable
  wells. Whole contained objects return to top-level inventory without ID
  allocation; pinned `NO_UNLOAD`, stale IDs, active power wells, and inventory
  capacity reject atomically. Fractional battery energy follows loose battery
  ammunition through unload/reload without rounding or identity loss, and such
  state is excluded from destructive component and disassembly paths. The
  client selects the first removable canonical pocket with `Y`; conformance
  scenario format 4/observation format 3 cover whole-stack removal through all
  four execution/recovery modes. Pre-56 serialized state rejects before
  mutation.
- Protocol 79/schema 57/CanonicalStateV55/CanonicalEventsV18 add strict
  ammunition-restricted `CONTAINER` pockets without claiming general physical
  containment. Pinned quivers retain sorted category capacities, inherited
  index/ID, base access moves, rigidity, and access flags. Authoritative `I`
  insertion and `Y` removal preserve stable IDs across whole, partial, merge,
  multi-variant, and category-switch paths; invalid capacity/category/access
  requests are atomic. The starter ground loadout supplies a quiver and wooden arrows,
  while scenario format 5/observation format 4 proves direct/snapshot/SQLite/portable-replay
  equivalence. Pre-57 serialized state rejects before mutation.
- Protocol 80/schema 58/CanonicalStateV56 retain CanonicalEventsV18 while
  adding bounded canonical item-group graphs and a strict selected-content
  registry. Modern and legacy collection/distribution forms retain source
  order, nested nodes, named references, count/charge ranges, load-order reset
  and self-extension, migration resolution, and explicit unsupported fields.
  Cycles, missing references/items, excessive depth/output, unsupported
  ammo/magazine dressing, and unsupported entry semantics fail closed. Bash
  consumers persist only their reachable sorted closure and plan every drop on
  one named RNG stream before checking placement and stable-ID capacity. Pinned
  `t_wall` uses the exact `wall_bash_results` source with maximum output 82 and
  resolves to `t_floor`. Scenario format 6/observation format 5 prove exact
  direct/snapshot/SQLite/portable-replay equivalence. Pre-58 serialized state
  rejects before mutation.
- Protocol 81/schema 59/CanonicalStateV57 retain CanonicalEventsV18 and replace
  synthetic flat grass plus the hand-built cabin with strict pinned JSON
  mapgen. Ordinary roots, exact 24x24 Unicode display-cell rows, fixed/weighted glyphs,
  static palette closure, regional terrain/furniture substitutions, and one
  named item-group placement per glyph normalize into a bounded canonical
  catalog. Fresh worlds repeat pinned `lmoe`, materializing 36 atomic OMTs/144
  chunks from coordinate-owned RNG; traversal-order, snapshot, SQLite snapshot
  recovery, portable replay, aggregate stable-ID preflight, and partial-cell
  rejection tests cover the runtime. The C++ oracle
  verifies mapgen matching, orientation, rotation, and static phase ordering.
  Pre-59 serialized state rejects before mutation.
- Protocol 77/schema 55/CanonicalStateV53/CanonicalEventsV16 add strict
  item-backed integral MAGAZINE pockets to Protocol 76's ordered detachable
  wells. Whole-stack reload preserves the source ID; partial transfer into an
  empty pocket allocates one nested ID; compatible merges retain the existing
  nested ID, while differing retained stack state rejects without data loss.
  Fractional energy occupies one capacity slot. Capacity, category, source
  pocket index/ID, and pinned
  `NO_RELOAD`/`NO_UNLOAD` access are canonical. Newly normalized magazines have
  zero outer charges, batteries derive power from nested `battery` items, and
  fractional energy remains with that pocket. Validation bounds recursion,
  rejects aggregate-plus-contained ammunition, traverses all nested IDs, and
  prevents loaded integral magazines from entering disassembly.
  Direct execution, snapshot restore, SQLite recovery, and portable replay
  agree on partial-split state and events. The starter medium cell is preloaded
  with a stable battery-ammunition child. Pre-55 serialized state rejects before
  mutation.
- Protocol 76/schema 54/CanonicalStateV52/CanonicalEventsV15 replace the
  single optional runtime well with up to 16 ordered wells carrying canonical
  pocket indices and source IDs. Reload requests and events identify the exact
  well; powered tools identify their energy well; all installed stable IDs are
  validated, hashed, restored, recovered, and replayed; and disassembly
  detaches every installed magazine. A two-well simulation proves independent
  reloads, missing-pocket rejection, nested identity, and snapshot restoration.
  Pre-54 databases with serialized world state reject before mutation because
  backwards Postcard migration is deliberately unsupported.
- The Rust 1.97.1 workspace builds on macOS. The native Bevy 0.19 client and
  standalone Bevy-free server use iroh 1.0.3.
- Protocol 75 advances to schema 53/CanonicalStateV51 while retaining
  CanonicalEventsV14. Strict inherited MONSTER `attack_cost` uses the pinned
  100-move default and exact integer direct/relative plus truncating
  proportional modifier precedence. Startup proves every selected cost is nonzero and fits `u16`;
  fresh live/corpse state, hashing, snapshot restore, corpse revival, SQLite
  recovery, and portable replay retain it privately. Ordinary adjacent monster
  melee now charges exactly `attack_cost * 20` action points on hits and misses,
  preserving signed slow-attack debt and multiple attacks from legitimately
  banked low-cost readiness. Zero-cost live and corpse state is rejected, and
  visible creature DTOs and CanonicalEventsV14 remain unchanged.
- Protocol 74 advances to schema 52/CanonicalStateV50 while retaining
  CanonicalEventsV14. Static inherited MONSTER `PACIFIST` is private canonical
  live/corpse state and suppresses only ordinary adjacent monster melee.
  Pacifists still perceive and move; opening, bashing, and upstream special
  attacks are not globally disabled. Differential movement/attack tests,
  hashing, snapshot restore, corpse revival, SQLite recovery, and portable
  replay retain the flag without changing visible creature DTOs.
- Protocol 73 advances to schema 51/CanonicalStateV49 while retaining
  CanonicalEventsV14. Static inherited MONSTER `IMMOBILE` is private canonical
  live/corpse state. It clears the creature's full accrued move budget after
  perception/goal bookkeeping and before ordinary melee/open/bash/movement,
  and adds the pinned 40-point player-hit-spread bonus after dodge and size.
  Snapshot restore, corpse revival, SQLite recovery, and portable replay retain
  it without changing visible creature DTOs. Dynamic `CANNOT_MOVE`,
  `RIDEABLE_MECH`, and special attacks remain explicit later boundaries.
- Protocol 72 advances to schema 50/CanonicalStateV48 while retaining
  CanonicalEventsV14. Strict inherited MONSTER `volume` uses the pinned 62,499
  ml default, integer ml/L grammar, relative addition, and truncating
  proportional multiplication. Fresh creatures derive exact base size at the
  four pinned volume thresholds; private live/corpse state, revival, snapshots,
  SQLite, and replay retain it without changing public creature DTOs. Every
  canonical monster type and base size now joins the exact empty-hand and
  admitted strict-bash-weapon player hit path with penalties
  30/15/0/-10/-20. Tests cover every threshold/modifier, an arbitrary
  non-zombie same-stream medium-hit/tiny-miss transition, state hashing and
  restore, large corpse revival, and huge-target recovery/replay.
- Protocol 71 retains schema 49/CanonicalStateV47/CanonicalEventsV14 and
  broadens exact sleeping-target hit resolution from the classic zombie to
  every canonical ordinary creature with nonzero melee dice. Finalized MONSTER
  `melee_skill` is the complete base accuracy in this effect-free path, the
  modeled actor is medium, and sleeping makes dodge exactly zero. Tests prove
  an arbitrary non-zombie type crosses from miss to hit solely with its skill,
  while zero-dice monsters preserve upstream's early no-hit return. Protocol
  70's deterministic stream, no-damage/no-wake miss, private clumsy fall,
  action cost, recovery, replay, and non-disclosure remain unchanged. Awake
  defense is still closed on its missing canonical inputs.
- Protocol 70 advances to schema 49/CanonicalStateV47/CanonicalEventsV14 for
  the first exact monster-side hit boundary. A pinned classic zombie attacking
  a sleeping actor rolls finalized melee skill through the cross-platform
  fixed-point `normal_roll(melee_skill * 5, 25)` adaptation against exact zero
  dodge. Negative spread spends the standard 100-move attack without damage,
  interruption, or waking. The finalized private `CLUMSY_ATTACKS` capability
  then uses pinned one-in-four and applies an exact two-second down state,
  including stopping extra same-tick actions. Creature/corpse state, revival,
  event and state hashes, snapshots, SQLite recovery, and portable replay
  retain the result. The miss event is not replicated because pinned CDDA
  suppresses both miss and stumble messages to a sleeping target. Awake actors,
  other monster types, and their unmodeled defensive inputs retain the explicit
  guaranteed-hit boundary.
- Protocol 69 advances to schema 48/CanonicalStateV46 while retaining
  CanonicalEventsV13. The strict ITEM importer now finalizes both legacy integer
  and current grip/length/surface/balance `to_hit` forms, including inherited
  relative adjustments. Every admitted ordinary bash-only smash weapon stores
  its exact bounded to-hit in the canonical profile; the pinned hammer resolves
  to -1. Those weapons now join the exact player-versus-medium-`mon_zombie`
  hit/dodge subset with pinned accuracy
  `DEX/4 + dominant_bashing/3 + practical_melee/2 + item_to_hit`. Bashing is
  dominant only above upstream `MELEE_STAT` 5. Connected commands and
  disconnected trapped defense use the same named session RNG, miss event, and
  exact Protocol 66 timing. Profile hashing, strict restoration, SQLite
  recovery, and portable replay retain the result. Mixed, fractional,
  degraded, ranged, magazine/ammunition, powered, and otherwise unregistered
  weapons, other monster types/sizes, monster attacks outside Protocol 70's
  sleeping-target classic-zombie subset, and remaining melee modifiers keep
  their explicit guaranteed-hit boundary.
- Protocol 68 retains schema 47/CanonicalStateV45 and advances to
  CanonicalEventsV13 for the first deterministic player hit/dodge boundary.
  An empty-handed actor attacking the pinned medium `mon_zombie` uses exact
  pinned accuracy `DEX/4 + practical_melee/2 - 2`, monster `dodge * 5`, and
  the pinned `normal_roll(accuracy * 5, 25)` shape. Because the C++ standard
  normal distribution is implementation-defined, the server uses the existing
  documented cross-platform 12-uniform fixed-point normal adaptation and a
  named session RNG keyed by world seed, actor, target, and accepted command
  sequence. Negative spread emits `ActorMissedCreature`, spends the same exact
  DEX-adjusted attack time, and causes no damage or corpse-ID allocation.
  Connected commands, disconnected trapped defense, snapshots, SQLite
  recovery, portable replay, and event hashing reproduce misses exactly. The
  public creature DTO remains unchanged and miss delivery is source-private.
  Armed attacks, other monster types/sizes, monster attacks, criticals, and
  broader melee accuracy inputs retain the documented guaranteed-hit boundary.
- Protocol 67/schema 47/CanonicalStateV45 retain CanonicalEventsV12 and make
  finalized inherited monster `melee_skill` and `dodge` private canonical
  state. The pinned classic zombie supplies melee skill 4 and dodge 0. Fresh
  world creation copies both values from the strict MONSTER registry; live
  creatures, self-contained corpse prototypes, revival, snapshots, SQLite
  recovery, portable replay, and the state hash retain them exactly. The
  public visible-creature DTO still omits both values. Combat outcomes remain
  unchanged in this prerequisite slice; deterministic hit/dodge resolution is
  the next boundary.
- Protocol 66 retains schema 46/CanonicalStateV44/CanonicalEventsV12 and gives
  canonical DEX its first gameplay consumer through pinned player melee attack
  speed. Unarmed attacks use the null item's exact 65-move attack time; strict
  ordinary bash-only weapons reuse their canonical attack-time profile. The
  server computes `base=attack_time/2`, adds the pinned practical-melee skill
  cost, subtracts effective `DEX/2` after the pinned effective-stat cap of 20,
  clamps to 25 moves, and converts once to action
  points. Default unarmed attacks cost 60 moves and the default hammer costs 74
  for melee versus its distinct 63-move smash. Player commands and disconnected
  defense share the same scheduler path, and the resulting signed readiness
  recovers and replays exactly. Complex live-weight, mixed-damage, ranged,
  magazine/ammunition, powered, or deeply damaged weapons retain the temporary
  100-move melee cost; stamina, limbs, martial arts, enchantments, weapon wear,
  hit/dodge, criticals, and practice remain unavailable.
- Protocol 65 retains schema 46/CanonicalStateV44/CanonicalEventsV12 and carries
  the pinned freeform character creator's STR/DEX/INT/PER values end to end.
  Each stat is independently bounded from 4 through 20 and defaults to 8; the
  pinned baseline's current `FREEFORM` pool means there is intentionally no
  point budget. Protocol decoding and simulation both reject invalid values,
  and simulation validates before allocating an actor ID. The authenticated
  iroh creation request, crash-reconciled character transaction, canonical
  actor, initial replication, SQLite state, private inspection, and replay all
  retain the exact selections. The Bevy creator uses Up/Down to choose a stat
  and Left/Right to adjust it; automatic named creation uses defaults.
  Scenarios, professions, traits, skills, appearance, and other creation
  choices remain unavailable.
- Protocol 64/schema 46/CanonicalStateV44 retain CanonicalEventsV12 and persist
  bounded STR/DEX/INT/PER for every actor; new survivors currently start at the
  pinned default 8 in each stat unless Protocol 65's freeform creator supplies
  another bounded value. Book definitions retain unadjusted pinned time
  plus their Intelligence threshold, and the authoritative reader's INT drives
  the checked low-INT duration penalty and pinned comprehension range.
  Disassembly practice uses canonical INT/PER for its pinned catch-up and
  knowledge multipliers, including the minimum-one branch and 90% theory cap.
  Snapshots, hashing, SQLite recovery, replay, replication, private inspection,
  operator output, and the Bevy HUD carry all four values. Protocol 66 consumes
  DEX for its strict melee-speed subset; non-stat generation, non-default focus, traits, and
  enchantments remain unavailable.
- Protocol 63/schema 45/CanonicalStateV43 retain CanonicalEventsV12 and replace
  the temporary player-smash stat/timing constants with canonical inputs.
  Actors persist base Strength, currently projected through an exact healthy
  arm multiplier of one. A sorted strict smash-item catalog binds ordinary
  integer-bash-only types to exact pinned bash damage and `item::attack_time`;
  restored instance damage and static-weight shape must match. Guns,
  charge-bearing types, and types with ammunition, magazine, or powered state
  are excluded. The default Strength 8 plus hammer bash
  9 remains strength 17, while its 566 g/320 ml attack time is 79 moves and the
  pinned 80% smash cost truncates to 63 moves. The HUD and administrator-private
  view expose base Strength. Snapshot validation, signed readiness debt,
  SQLite recovery, portable replay, and the complete pinned catalog are tested.
  Limb damage, enchantments, count-by-charge rounding, faults/mods, unarmed
  anatomy, wear, stamina, practice, and non-bash profiles remain closed.
- Protocol 62/schema 44/CanonicalStateV42/CanonicalEventsV12 add the first
  player structural-smashing path. H offers every currently visible registered
  furniture/terrain target in all eight horizontal adjacent directions using
  server-supplied interaction metadata; hidden memory exposes none. The server
  revalidates adjacency, registry membership, and furniture precedence. Every
  pinned furniture ID with an upstream bash body is canonical even when its
  behavior is excluded from the strict runtime subset; excluded layers block
  the terrain beneath them rather than permitting a pass-through. A
  wielded bash-only item contributes exact pinned bash damage plus default arm
  Strength 8, so the clean pinned hammer supplies 17. Unarmed, mixed-damage,
  fractional-bash, and damage-level-above-one tools reject instead of
  approximating missing anatomy or item/profile rules.
  Attempts use the existing standard 100-move actor action, persistent damage,
  atomic transforms/drops/fields, named RNG, and monster-hearing sound path.
  Typed actor events, diagonal layered destruction, snapshot restoration,
  SQLite recovery, and portable replay are covered. Weapon-specific attack
  time, wear, stamina/practice, full actor stats, corpses, fields, vehicles, and
  ownership warnings remain unavailable.
- Protocol 61/schema 43/CanonicalStateV41 adds route-planned bashing without a
  new event shape, so CanonicalEventsV11 remains current. Positive-distance
  A* routes consider registered terrain and furniture bash targets after
  ordinary movement and door opening. Rating above one costs
  `(20 / rating) + 12`, rating one costs 500, and zero remains impassable.
  Estimates use ordinary unblocked bounds and base content-derived bash skill;
  actual hits still use contextual blocked bounds. The direct candidate
  estimate may still double `GROUP_BASH`, and actual hits still use connected
  helpers. A strong basher chooses a destructive shortcut while a desperate
  group basher chooses a walkable detour. A sideways-first destructive path
  recovers and verifies through SQLite and portable replay. The fresh zombie's
  zero path distance deliberately retains greedy behavior. Route caching and
  backoff, vertical/trap/sharp/nearby-creature hazards, vehicles, and unsupported
  bash effects remain unavailable.
- Protocol 60/schema 42/CanonicalStateV40 broadens furniture bashing without a
  new event shape, so CanonicalEventsV11 remains current. An explicit strict
  predicate admits 537 of 699 pinned furniture definitions with ordinary or
  blocked strength bounds, modeled fields, bounded direct drops/sounds, and no
  supported-floor, tent, collapse, explosion, item-group, or other retained
  side effect. Replacement closure prevents an admitted first stage from
  producing a later unsupported bash stage. All four furniture IDs placed in
  the then-current fresh cabin were included.
  Runtime supports both removal and furniture replacement while preserving the
  terrain layer. The pinned count, every canonical definition, registry
  restoration, and a 128-KiB encoded-size ceiling are tested. Unsupported
  furniture definitions remain loaded as content but absent from simulation.
- Protocol 59/schema 41/CanonicalStateV39/CanonicalEventsV11 add the first
  strict furniture-destruction path and finish the modeled wooden-door chain.
  Furniture takes upstream precedence over terrain at the same tile and shares
  the canonical persistent damage accumulator. The pinned cabin dresser uses
  its exact default-profile strength, direct drops, fields, sounds, and
  `f_null` removal without changing the underlying floor. The damaged/closed
  door stages continue through `t_door_frame`, whose dynamic result is
  explicitly `t_floor` for the known z=0 cabin topology. Bash events now carry
  a typed furniture/terrain layer plus the exact target content ID. Live and
  restored simulations produce identical events and CanonicalStateV39 hashes.
  Other furniture types and dynamic roof/floor repair outside this admitted
  topology remain unavailable.
- Protocol 58/schema 40/CanonicalStateV38 add the first strict monster
  obstacle-routing path without changing CanonicalEventsV10. All observed
  MONSTER `path_settings` fields load with inheritance. Positive `max_dist`
  enables deterministic same-z A* over already loaded canonical tiles, using
  upstream-shaped neighbor order, bounds, budget, ordinary costs, door cost,
  and dangerous-field penalty; zero preserves direct greedy behavior. The
  routed adjacent step may temporarily move sideways or away from the ultimate
  target, so a capable creature can round a wall or plan through a modeled
  terrain door. Settings persist through corpses/revival, recovery, and replay
  but are absent from the public creature DTO. The default zombie remains a
  distance-zero greedy pursuer; pinned feral humans resolve distance 45 and
  route-open doors. Route caching/backoff, pathing through structural bashes,
  vertical routes, traps/sharp terrain, creature danger avoidance, and fresh
  feral spawning remain unavailable.
- Protocol 57/schema 39/CanonicalStateV37/CanonicalEventsV10 add the first
  strict monster terrain-door-opening path. `CAN_OPEN_DOORS` is content-derived
  and persists through live creatures, corpses, revival, recovery, and replay.
  A capable creature selects an admitted terrain `open` transform before
  bashing, emits canonical `swish` at volume 6, pays the pinned zero move cost,
  and may immediately enter while it remains ready; opening also clears stale
  structural damage. The default zombie remains incapable, while the pinned
  feral-human family supplies the flag. Door sound enters private same-z
  monster hearing after the creature phase. Interior-only terrain transforms
  remain fail-closed until canonical indoor/outdoor topology exists. Furniture
  and vehicle doors, unlocking, pacification/effect restrictions, broader routing,
  closing, and client audible observations remain unavailable.
- Protocol 56/schema 38/CanonicalStateV36/CanonicalEventsV9 add the first
  authoritative monster-hearing path. The strict ITEM importer resolves
  `loudness`; the pinned starter revolver/ammunition pair derives CDDA volume
  70 and emits `bang!` with its canonical origin. `HEARS` and `GOODHEARING`
  persist through live creatures, corpses, revival, recovery, and replay.
  Same-z gunfire and structural-bash sounds create deterministic private
  imprecise goals using pinned distance, perception, interest, replacement,
  and action-counted lifetime rules. Current sight and last-seen visual intent
  retain priority over sound intent. Deaf creatures ignore sound and good
  hearing doubles perceived volume and multiplies pursuit lifetime by six.
  The public creature DTO excludes hearing flags and sound goals. Player/client
  audible observations, sound markers, z propagation, weather/obstacle
  attenuation, clustering, and other sound producers remain unavailable.
- Protocol 55/schema 37/CanonicalStateV35 add the first strict structural-bash
  path. The content importer resolves the default and wooden-door bash damage
  profiles plus inherited terrain/furniture bash metadata. The default zombie
  persists `BASHES` and `GROUP_BASH`; connected helpers contribute through the
  pinned five-row, three-wide formation. Exact fixed-point damage accumulates
  per tile, survives recovery/replay, and transforms `t_door_c` to `t_door_b`
  and then `t_door_frame`. Successful stages atomically reserve stable debris
  IDs before changing terrain, place exact direct-item drops, apply hit and
  destroyed fields, emit canonical attempt sound, and charge the pinned action
  cost. Client creature replication still excludes capabilities and all other
  private simulation state. Generic furniture bashing, the frame's `t_null`
  floor replacement, item groups, collapse, and routing remain unavailable;
  Protocol 56 consumes its canonical attempt sounds for private monster hearing.
- Protocol 54/schema 36/CanonicalStateV34 persist each creature's exact
  last-seen destination. Current sight refreshes the goal, loss of sight follows
  it with the existing deterministic movement rules, and reaching it clears the
  memory. Snapshot restoration, SQLite recovery, and portable replay reproduce
  the behavior. Client replication projects canonical creatures into a
  dedicated public DTO that structurally omits goal, signed readiness, combat
  attributes, blood, and corpse reconstruction data. Sound/scent goals, route
  planning, danger avoidance, and special movement remain unavailable;
  Protocol 55 adds the strict structural-bash boundary described above.
- Protocol 53/schema 35/CanonicalStateV33 preserve content-derived `STUMBLES`
  in live creatures and corpse prototypes. Direct pursuit now uses the pinned
  ordered `squares_closer_to` fan. A non-stumbler takes the first valid
  positive-progress square; a stumbler uses named ChaCha8 randomness and the
  pinned Euclidean progress-weighted replacement rule. Q30 integer square roots
  keep selection and default-circular-distance stagger cost cross-platform and
  replayable: tests lock 142-move direct diagonals and an 88-move flanking
  stumble. SQLite recovery and portable replay retain both the flag and exact
  signed debt. Bashing, path settings, sound, scent, richer target memory,
  danger avoidance, and movement-mode flags remain unavailable; Protocol 54
  adds the exact last-seen destination described above.
- Protocol 52/schema 34/CanonicalStateV32 replace the creature cadence-only
  counter with signed readiness debt. A cardinal move uses the same pinned
  source/destination terrain plus nonnegative furniture cost as upstream's
  ordinary monster movement, scaled at 20 local action points per upstream
  move. Floor costs 100 moves; entering or leaving the modeled bed costs 175.
  Expensive movement executes at readiness and leaves debt that survives strict
  snapshots, canonical hashing, SQLite recovery, and portable replay. Tests
  prove the exact move and recovery boundaries. Stumble weighting, bashing,
  path settings, fields, movement flags, and diagonal stagger adjustment remain
  unavailable.
- Protocol 51/schema 33/CanonicalStateV31 load inherited MONSTER `vision_day`
  and `vision_night` and preserve `SEES` plus both ranges in live creatures and
  corpse revival prototypes. The pinned default zombie uses day 40/night 3.
  Hostile target acquisition now requires current terrain/furniture LOS and a
  range derived from the existing deterministic solar/lunar and powered-light
  model, then selects distance and stable actor ID. External light can expose a
  target up to the monster type's maximum visual range but cannot bypass an
  occluder. Every actor uses the same multiplayer rule instead of upstream's
  singleton-avatar special case. Tests cover darkness, target illumination,
  opaque terrain, restored pursuit, strict snapshot bounds, canonical hashes,
  SQLite recovery, and portable replay. Hearing, scent, richer target memory,
  camouflage/effects, obstacle bashing, and pathfinding remain unavailable;
  Protocol 54 adds the exact last-seen destination described above.
- Protocol 50/schema 32/CanonicalStateV30 replace a dead ordinary creature with
  a stable-ID `corpse` item carrying its strict self-contained revival
  prototype, death tick, damage, and deterministic special-revival flag. The
  pinned overkill curve can pulverize a body; other corpses remain ordinary
  ground or inventory items and may revive after the upstream damage-slowed
  effective-age/probability checks at 80% speed and 70% HP divided by corpse damage, initially downed for
  five seconds. Carried revival removes the exact item, clears wielding, and
  chooses the carrier tile or a deterministic adjacent tile. Special revival
  uses the nearest living actor within three tiles as the explicit multiplayer
  replacement for upstream's singleton avatar. The once-per-second processor
  continues with zero connected players and is covered by strict protocol,
  snapshot, SQLite-recovery, portable-replay, atomic stable-ID-exhaustion, and
  boundary tests. Death drops, worn items, butchery, pulping, burning, rot,
  gibs, and nonordinary death functions remain unavailable.
- Protocol 49/schema 31/CanonicalStateV29 add the first canonical pinned field
  slice. The strict `field_type` registry resolves inherited intensity names,
  symbols, colors, danger/transparency, priority, half-life, linear-decay,
  splatter, and display state while retaining unsupported fields. Canonical
  chunks hold sparse type-ID-sorted multi-field tiles with stable equal-priority
  display sequence and per-field age. The MONSTER loader now includes inherited
  material sets; the default `WARM` flesh zombie resolves to `fd_blood`, and its
  ordinary death adds one upstream-shaped splatter. Once-per-second decay uses
  an integer Q0.64 exponential probability or exact linear boundary, advances
  with zero players and downtime catch-up, and is included in snapshots,
  canonical hashes, SQLite recovery, event hashes, and portable replay. Visible
  field metadata replicates only on currently perceived tiles, never through
  terrain memory, and the Bevy client colors the highest-priority displayed
  field. Fire/fuel, gas spread, contact effects, underwater acceleration,
  mopping, and other processors remain unavailable.
- Protocol 41/schema 28/CanonicalStateV27 add the first strict construction
  slice. The selected loader retains all 776 construction definitions and 438
  groups; the server exposes exactly 17 definitions whose complete current
  semantics are item-to-empty-adjacent-tile furniture placement, including
  pinned `constr_place_table` (`w_table`, one minute, `f_table`). M opens the
  Bevy construction picker and adjacent visible-target picker, resumes an
  interrupted build, and X cancels it. Clients send only construction ID and
  target; the Bevy-free server replaces any supplied body before simulation or
  journaling. The authoritative activity checks practical skills and detail
  light, reserves exact stable component items, progresses while disconnected,
  and rechecks the target. Damage, harmful needs, exhaustion, darkness, or a
  changed target interrupt it. Cancel restores exact items and split charges;
  completion persistently mutates furniture. Protocol bounds, private and
  replication DTOs, canonical hashing, schema recovery, and portable replay
  cover in-progress construction. Protocol 41 also persists the finalized
  terrain `FLAT` flag (including open/close transforms) and requires it for the
  pinned `check_empty` predicate, so a forged request cannot place furniture on
  a wall or force far-away chunk generation. Tools, qualities, reusable requirements,
  specials beyond `check_empty`, terrain/result chains, deconstruction, work
  sites/helpers, and the remaining construction corpus remain fail-closed.
- Protocol 46 widens that same server-owned construction boundary to exact
  terrain prerequisites and terrain results without changing schema or
  CanonicalStateV27. Exactly 20 pinned colored-carpet definitions now join the
  original 17 furniture placements, for 37 authoritative constructions. Each
  admitted carpet recipe has one modeled floor prerequisite, ordinary item
  components, `LIGHT_EXERCISE`, and no tools, qualities, flags, requirement
  lists, or special predicates. The Bevy picker filters currently visible
  adjacent tiles by the recipe's current terrain ID; the server independently
  rechecks the exact ID before start, resume, and completion. Terrain mutation
  preserves the independent furniture layer like upstream `ter_set`.
  Snapshot restoration plus SQLite recovery and portable replay reproduce the
  completed terrain exactly. Other prerequisites/results remain fail-closed
  unless every definition field is modeled.
- Protocol 47/schema 30/CanonicalStateV28 add non-consuming construction
  qualities to the immutable server-normalized activity. The pinned strict
  catalog gains exactly `constr_brick_oven_finisher`, which requires AXE 2 and
  CHISEL_WOOD 1 while turning `t_brick_oven_struct` into `t_brick_oven`; the
  catalog now contains 38 definitions. Provider type IDs and any per-provider
  charge threshold come from the pinned item registry. A provider is protected
  from component reservation, must remain carried while work advances, and
  produces a typed interruption/rejection if missing at start or resume. The
  Bevy picker applies the same presentation filter. Snapshot validation,
  canonical hashing, SQLite recovery, and portable replay retain the quality
  requirements exactly. Sixteen additional carpet definitions still remain
  unavailable because their components use independently unsupported reusable
  `LIST` requirements.
- Protocol 48 reuses the pinned recipe requirement dictionary to recursively
  inline construction component `LIST` references before catalog admission.
  It adds the 16 previously blocked HAMMER carpet variants plus `constr_hay`,
  bringing the strict server-owned catalog to 55 without changing schema 30 or
  CanonicalStateV28. Multipliers, nested references, upstream first-group
  selection, alternative order, duplicate-count minimization, cycle/missing
  reference rejection, count-by-charge normalization, and recoverability flags
  use the same tested resolver as crafting. The client expands the same pinned
  dictionary only to present affordable choices; commands still carry just ID
  and target, while the server journals the fully expanded immutable component
  alternatives. Pinned `nails` therefore becomes five `nail` or five
  `bronze_nail`, and no `LIST` marker crosses the protocol boundary.
- Protocol 42/schema 29 add the first disconnected survival-autopilot boundary
  and its exact recovery input, which
  runs after activities and needs but before hostile creature turns. A living,
  awake, disconnected
  actor with a ready movement slot and no queued command or uninterrupted
  activity deterministically flees visible aggressive creatures within eight
  Manhattan tiles. Candidate order and stable actor/creature IDs are fixed; a
  move must strictly increase minimum threat distance, uses ordinary terrain
  and furniture movement debt, and may enter only an already loaded passable
  tile. It never attacks while such a retreat exists, generates chunks,
  collects items, changes equipment, or cancels an interrupted activity. Tests
  prove deterministic flight at a
  chunk edge, preserved interrupted construction, trapped-character
  vulnerability, snapshot restoration, SQLite recovery, and portable replay.
  Ordered actor connection transitions are journaled with the next tick before
  held movement and commands. Crash-start disconnects and stale offline
  movement-lease clears are carried into the first downtime-catch-up tick or
  seeded into the first live tick, so recovery from an online snapshot cannot
  suppress or invent an autopilot move.
  Protocol 43 adds the first defensive fallback: only when no valid retreat
  increases threat distance, an adjacent visible aggressive creature may
  receive one ordinary unarmed or currently wielded melee hit, selected by
  stable creature ID and paid for with the normal one-second action cost. The
  actor never chases, shoots, attacks neutral creatures, or changes equipment,
  and the hostile still takes its same-tick turn if alive. SQLite recovery and
  portable replay reproduce the hit. Protocol 44 adds a safe emergency
  nutrition fallback. With no visible aggressive threat inside the eight-tile
  autopilot radius, an already
  needs-damaged actor spends one ordinary action on an owned unwielded
  `FOOD`/`DRINK` that improves dehydration or starvation. Dehydration has
  fixed priority, stable item ID breaks ties, and the ordinary consume event,
  exact charges, connection boundary, SQLite recovery, and portable replay all
  agree. It does not consume medicine, wielded items, ground items, or anything
  before the existing harmful threshold; danger always wins. Fire/hazard
  escape and shelter seeking remain unavailable. Protocol 45 adds a
  conservative sleep fallback: a tired, fed, non-dehydrated disconnected actor
  with no visible aggressive creature inside eight tiles may sleep only on its
  current positive-comfort furniture. The typed `Autopilot` sleep reason,
  danger priority, floor and harmful-needs refusal, snapshot state, SQLite
  recovery, and portable replay are verified. Medicine use, sleep-location
  search, alarms, multiple-z pathing, and richer threat memory remain
  unavailable.
- Protocol 39/schema 26/CanonicalStateV25 extend the powered-item and
  local-light slice to nine strict pinned detachable-battery transform pairs
  (18 off/on ITEM definitions), including `flashlight`, both
  simple diving flashlights, wearable/mounted lights, `mipim`, and
  `wizard_cane`.
  P sends only a stable item ID; the server owns the zero-move transform,
  one-charge activation debit, content-derived exact draw, fractional cell energy, automatic
  reversion, and typed events/rejections. Active carried and dropped lights
  continue draining with zero players and supply LOS-bounded local detail light
  for terrain memory, replication, ranged targeting, reading, and disassembly.
  State, residual energy, and nested cell identity survive recovery and replay.
  Audited integer-only open-air tables map luminance to ordinary and external
  detail radii without canonical floating point. The luminance-four
  `wizard_cane_cheap` reaches three tiles, supplies the pinned personal-light
  detail bonus while carried, and does not supply external detail light after
  dropping. All nine pinned pairs carry `CHARGEDIM` and now scale exact base
  emission below one fifth of their installed cell's exact
  charge-plus-residual energy. Multi-action sources remain
  fail-closed. Directional/full-spectrum lightmaps, weather and
  interior ambient light, recharge, and other powered transforms remain unavailable.
- Protocol 36/schema 24/CanonicalStateV23 implement the first strict
  detachable-battery slice. Pinned `flashlight` owns an optional stable-ID
  `medium_battery_cell`; authoritative reload swaps cells intact, crafting
  spends installed energy, disassembly detaches the exact remaining cell, and
  snapshot/replication/private-inspection/recovery/replay validation enforce
  nested ID uniqueness, namespace, compatibility, and capacity. The cabin
  starts with an empty flashlight and full 56-charge cell. Full `pocket_data`
  and every other pocket or detachable-magazine runtime remain unsupported.
- The renderer-independent simulation advances at 20 Hz with zero players,
  deterministically resolves movement/combat and hostile creature turns, and
  leaves disconnected actors present, targetable, and vulnerable.
- Protocol 36 extends the authoritative content-derived crafting loop. The
  strict selected registry resolves more than 5,000 recipes and 474 reusable
  requirements while preserving unsupported reasons; the server offers exactly
  3,049 recipes with complete current runtime semantics: 1,990 independently
  autolearned definitions, 1,058 additional book-backed definitions, and one
  further definition available through permanent disassembly learning, all
  with no flag or the explicitly safe `BLIND_EASY` and `ALLOW_ROTTEN`,
  including pinned `rock_sock` with its `socks`/`socks_wool` alternative and
  `pointy_stick` with a carried `CUT` provider, vegetable juice with recursively
  expanded tomato/zucchini `LIST` alternatives, a makeshift deck of cards with
  eight concrete drawing-tool alternatives, charged
  `toasterpastry_with_toaster`, and the charged-DRILL suppressor/vehicle-light
  recipes. ITEM qualities and `charges_per_use` honor direct, inherited,
  extend, delete, and relative definitions. Ordered tool/quality OR groups are
  server-normalized and protected from also serving as ingredients. Presence
  requirements aggregate distinct stable carried instances; charged tools use
  aggregate carried energy and stable-ID-ordered depletion. The starter loadout
  adds a stick, small knife, hammer, frozen toaster pastry, charged toaster,
  empty flashlight, full medium battery, and pistol manual so implemented
  crafting-support, power, and study paths are playable. Clients send only a
  recipe ID and the server replaces every supplied body before simulation and
  journaling. Ingredients reserve atomically in stable-ID order, partial charge
  stacks split without replacing the carried parent, and every output ID is
  preallocated. Legacy-array and explicit logistic/linear
  `batch_time_factors` load, inherit, and validate; current commands craft one
  unit, for which both pinned formulas equal the ordinary recipe time. Both
  pinned `book_learn` shapes load, inherit, validate BOOK references, and retain
  skill level, alternate name, and hidden metadata. Inherited BOOK
  `required_level` supplies the fallback threshold. A carried concrete BOOK
  type is treated as identified and authorizes its recipe at craft start only
  when the actor meets the effective theoretical primary-skill threshold. The
  server journals sorted book requirements in the normalized recipe and checks
  inventory atomically; the knowledge check does not reserve or consume the
  book. `never_learn` applies only to explicit permanent learning, not autolearn
  or live book use, matching pinned CDDA. The server separately normalizes 197
  pinned physical skill books. V starts or resumes a real-time study activity
  and X cancels it; active study continues while disconnected and remains
  vulnerable. Under the current self-reading model, concrete books are
  identified, canonical actor Intelligence applies, focus is 100, no
  trait/enchantment/helper modifier applies, and natural daylight or an active modeled personal/external light
  supplies fine-detail light. Pinned duration, comprehension bounds, double level scaling,
  theory/practice gap reduction, threshold reset, and last-practiced behavior
  drive server-owned deterministic named-RNG XP. Damage, harmful needs,
  exhaustion, and darkness interrupt; resume rechecks the exact carried book,
  light, and skill bounds. Study persists through canonical snapshots, SQLite
  recovery, replay, replication, private inspection, and the Bevy HUD. Reading
  does not permanently learn a book recipe. Scalar and explicit-list
  `decomp_learn` metadata loads, inherits, and validates skill references.
  The selected loader separately finalizes all 1,428 concrete pinned core
  `uncraft` definitions plus one abstract. Explicit uncrafts use their own
  inheritance dictionary and override reversible craft recipes by target item
  ID, matching upstream precedence. After the current fail-closed runtime
  boundary, the server publishes 1,227 authoritative disassembly recipes.
  N starts or resumes disassembly of an eligible carried item and X cancels it.
  The server replaces client recipe bodies, requires a canonical damage level
  0 through 4, authoritative detail light, and ordered carried tool/quality support, then reserves
  the target and all possible output IDs.
  Non-guns remain the general boundary. Three pinned bare ranged targets
  (`coilgun`, `compositebow`, and `compositecrossbow`) are also admitted: their
  exact internal load is dropped first as the ammunition registry's pinned
  default item, and the activity reserves the emptied weapon. The ammunition ID
  is preallocated with component IDs, survives snapshot/recovery/replay, and
  remains on the ground if the player cancels, so cancel cannot duplicate it.
  Inherited scalar/list `tool_ammo` is canonical content. The 73 still-unmodeled
  zero-default-charge powered-tool targets carry a server-owned empty-storage
  requirement: an empty instance remains disassemblable, while a charged one is
  hidden by the client and rejected atomically by simulation. A generalized
  unload-before-reserve path handles integral tool charges and is covered by
  hostile-snapshot, cancel, SQLite-recovery, and portable-replay tests; no
  default-charged non-pocket tool in the pinned corpus currently qualifies for
  that path. The three modeled powered-light disassembly targets are
  `flashlight`, `wearable_light`, and `wearable_big_light`; their installed cell
  is detached intact before target reservation.
  For an ordinary item, the server uses CDDA's first component alternative per
  group as the deterministic default. Reversible non-charge crafts retain the
  exact stable-ID-ordered component objects actually consumed, including
  alternative type, charges, condition, item properties, recoverability, and
  bounded recursive component history. Disassembly replaces catalog defaults
  with that retained state and recreates exact recovered components. Pinned
  `NO_RECOVER` filters ordinary recipe defaults, while exact stored objects use
  the distinct ITEM-level `UNRECOVERABLE` filter. Work advances
  in real time while disconnected and interrupts on damage, harmful needs,
  exhaustion, or darkness. Cancel restores the exact target and wield state;
  completion drops stable-ID-ordered recovered components at the actor's current
  position and deterministically accounts for destroyed components. Recovery
  combines the pinned skill contest with the exact integer
  100%/80%/64%/51.2%/40.96% condition multiplier; cancel restores the original
  condition. A separate
  server-owned named RNG applies pinned one-in-four permanent learning when the
  actor meets `decomp_learn`; sorted learned recipe IDs persist through
  snapshots, SQLite recovery, replay, replication, private inspection, and the
  Bevy craft menu. Completion applies pinned `difficulty * 2` primary-skill
  practice with the current focus-100 and canonical actor INT/PER, including
  catch-up, cap, level/theory, last-practiced, recovery, and replay semantics.
  Component trees are bounded to 256 entries and depth eight in hostile wire,
  canonical snapshot, persistence, and replay paths. General detachable
  magazines, weapon mods, other batteries and pocket contents, default-charged
  targets, and charged/special tool substitutions remain fail-closed or
  explicitly deferred. Root
  `using` replaces inherited external requirements, while
  `extend.using` appends and flows through the same recursive normalization;
  pinned case-hardened sheet metal retains both blacksmithing and carbon. Legacy
  deterministic `byproducts` materialize in sorted type-ID
  order after the main result, using pinned count-by-charge defaults or exact
  instance counts. They participate in capacity checks, protocol validation,
  cancellation ID burning, snapshots, SQLite recovery, and portable replay;
  randomized `byproduct_group` remains unavailable. Exact real-time progress
  continues while disconnected; damage,
  harmful needs, exhaustion, and missing tool energy interrupt. Charged tools
  debit atomically on the pinned twenty 5% buckets with the remainder
  front-loaded; a shortfall commits no progress or practice, ordinary actions
  can resupply an interrupted character, and resume can reselect an ordered
  alternative. B resumes and X cancels with exact ingredient/wield restoration,
  while spent energy remains spent. Canonical snapshots, SQLite recovery,
  portable replay, private/interest replication, the Bevy menu/HUD, and genuine
  iroh forged-payload coverage are tested. The strict registry loads all 28 default
  pinned skills. Sorted practical/theoretical levels, raw experience, and
  last-practiced ticks are canonical; theoretical levels gate autolearn and
  carried-book comprehension, while a complete practical or theoretical
  requirement set authorizes crafting.
  Default-focus crafting awards 100 raw experience per nominal second up to the
  pinned integer difficulty cap, including while disconnected, and earned
  practice survives interruption/cancel/recovery/replay. The HUD shows skill
  progress and the craft menu shows primary requirements. The selected
  proficiency registry strictly loads 234 definitions. Mandatory recipe
  proficiencies gate the menu and authoritative start; missing optional
  proficiencies deterministically slow progress, train at 5% craft boundaries
  subject to prerequisites and caps, and emit learned events. Sparse sorted
  practice/remainder/learned state persists while disconnected and across
  cancellation, recovery, replay, replication, private inspection, and the HUD.
  Skill penalties remain retained-only until stochastic failure exists. Tool pockets,
  batteries, external power/UPS, nondefault charge factors, zero-cost/external
  charged qualities, step-recipe quality speed, other recipe flags, non-default focus and remaining stat consumers,
  rust, helpers/ebooks/recreational reading/identification/full lightmaps,
  workspaces, multi-item batch commands, containers, randomized byproduct
  groups, expanded disassembly semantics, stochastic failure, construction,
  and broader learning remain.
- Protocol 35 derives the pinned generic-scenario 91-day calendar from canonical
  ticks, beginning on Spring day 61 at 08:00. The server replicates and validates
  that clock and the Bevy HUD renders year, season, day, and time. Weather and
  scenario-specific starts remain.
- Protocol 35 carries the pinned default Boston civil-dawn/sunrise/sunset/dusk model
  through an audited 364-day whole-second table and deterministic integer moon
  phases. Day, twilight, and moonlit-night sight radii consistently gate terrain
  memory, replication, entities, and targeted ranged attacks; the HUD reports
  sky/moon/sight state and shades current terrain by phase. Runtime canonical
  code uses no astronomical floating point. Daylight now uses the pinned
  60-tile `MAX_VIEW_DISTANCE`; civil twilight remains eight tiles and moonlit
  night uses radii 2, 2, 3, 11, and 12. The exact powered flashlight adds the
  first LOS-bounded local source. Full CDDA brightness/lightmaps, attenuation,
  weather, interiors, and vision modifiers remain.
- Protocol 35 retains canonical per-character terrain/furniture memory in sparse 12x12
  submap chunks. Deterministic LOS refreshes the last perceived tile for living
  awake connected and disconnected actors; occluded or sleep-time changes remain
  stale until seen while awake. Exact transient chunk-revision comparison avoids
  recomputing unchanged LOS every tick without changing canonical outcomes.
  Memory participates in CanonicalStateV22,
  snapshots, crash recovery, and replay.
  Interest replication strips the full private memory collection, labels every
  delivered tile as current or remembered, omits unseen chunk revisions, and
  never remembers dynamic entities. The Bevy client renders memory with a dim
  palette and refuses door interaction through remembered state.
- Actors now use integer speed plus signed readiness/debt: speed 100 performs an
  ordinary 100-move action per real-time second, idle readiness banks up to one
  action, and at most two semantic commands buffer in canonical state. Protocol
  14 charges horizontal movement from the pinned source/destination tile costs,
  with each tile combining terrain and nonnegative furniture cost and using the
  upstream 50-cardinal/71-diagonal axis multipliers. A normal cardinal floor
  step costs 100 moves and floor-to-bed costs 175; movement happens
  immediately when ready and its signed debt delays the next action. Queue
  admission/execution, full-queue rejection, snapshots, recovery, replay, and
  client debt presentation are deterministic. Numpad and Home/PageUp/End/
  PageDown expose diagonals, and diagonal neighbors participate in player and
  creature melee. Held key state travels in strictly bounded, sequence-numbered
  iroh datagrams refreshed every 100 ms; it is canonical, journaled, replayable,
  subordinate to semantic commands, and cleared by client release, disconnect,
  or a 250 ms server lease. Vertical movement, stance, stamina, encumbrance, traits,
  fields, and vehicles remain.
- Protocol 35 carries persistent sleepiness at the pinned 191/383/575/1000
  thresholds. Awake actors gain one point every five in-world minutes;
  voluntary sleep begins at `TIRED`, exhaustion forces sleep at 1,000, and
  replay-safe recovery accelerates over the pinned 24 sleep intensities until
  natural waking at -20. Sleep halves food/water progression, banks no action
  readiness, rejects ordinary commands, and reveals only remembered terrain and
  no dynamic entities. Z issues sleep/wake from the Bevy client, sleepers render
  distinctly, wake commands bypass readiness, and damage wakes a surviving
  sleeper. The state and reasoned events persist across disconnect, snapshots,
  crash recovery, portable replay, private/public replication, and real iroh
  validation. Comfort, temperature, traits, sleep deprivation, alarms,
  microsleeps below forced exhaustion, and sleep healing remain.
- SQLite WAL persistence reserves stable-ID blocks transactionally and writes
  state-hashed, bounded compressed snapshots. It journals every committed tick's
  exact command set, event hash, and state hash in 100 ms batches; recovery
  replays inputs after the latest snapshot and rejects tick/hash divergence.
  Gameplay replies wait for their command batch's successful SQLite commit.
  Character creation pauses tick advancement across its provisional spawn,
  flushes every preceding tick, durably records the initial actor, creation
  tick, and exact latest journal sequence, then commits or rolls back before
  resuming. Recovery reinserts a missing committed actor on the correct side of
  same-tick allocator boundaries. Startup also snapshots each newly
  reserved ID block before gameplay can begin. Periodic snapshots now use an
  exact paused tick barrier, flush only through that boundary, and atomically
  roll to a fresh transactionally reserved object-ID block before resuming;
  strict zero-tick `IdBlockAbandoned`/`IdBlockReserved` boundary batches now
  reproduce startup burns, crash gaps, and refill state without consulting the
  live allocator. Replay-derived event IDs use a separate canonical sequence
  and cannot exhaust object reservations. The CLI exports a self-contained,
  versioned Postcard/
  Zstandard replay bundle containing its initial snapshot, character spawn
  inputs, journal batches, pinned content identity, and final canonical hash;
  renderer-free verification rejects header, content, tick, event, or state
  divergence.
  A schema-v16 runtime marker distinguishes clean maintenance stops from process
  crashes. Each journal commit atomically advances its UTC anchor; unclean
  restart converts the exact elapsed whole seconds into commandless 20 Hz ticks,
  journals every regenerated event/state hash before opening networking, and
  clean shutdown disables catch-up. Long-outage analytical/coarse catch-up is
  still required to replace this exact but potentially expensive first path.
  After startup, one bounded dedicated thread exclusively owns the live SQLite
  writer connection and serializes journal, snapshot, checkpoint, stable-ID,
  account, authorization, and character operations. Network tasks bridge replies
  through Tokio's blocking pool. Its 64-request queue and snapshot handoff share a
  32 MiB serialized-payload budget. Snapshot work is limited to one in flight
  and one pending latest-value job; replacement is explicit, periodic capture
  resumes immediately after an exact tick barrier, and clean shutdown waits for
  the newest write. Queue/byte saturation, worker loss, and reply timeout fail
  closed instead of opening ad-hoc connections.
  A durable sequence/UTC cursor rolls exact replay ranges hourly on one bounded
  non-SQLite worker. Before extraction, schema 23 persists exact pending world-
  journal and security-audit end sequences plus UTC, so a restart retries that
  range even after later journals, security inputs, and snapshots exist. The
  worker renderer-independently verifies CanonicalStateV22,
  compresses and atomically fsyncs an owner-only archive, advances and clears
  the pending cursor only after success, accepts only byte-identical crash
  retries, and prunes only recognized archives older than 30 days. Each format-3
  replay binds a separately published, bounded, re-read, content-addressed
  `SnapshotObjectV1`. After pruning, the worker fully verifies every retained
  replay and referenced object before deleting exact-name unreferenced objects;
  malformed or missing inputs fail closed and unrelated files remain untouched.
  Current backup formats have no external object references. Daily compaction
  runs only without a pending archive and removes journals through the committed
  cursor plus snapshots before its preserved anchor while keeping recovery
  deterministic.
  The server creates an initially due and then hourly SQLite online backup,
  retaining 24 newest hourly and 30 older daily generations. Before atomic
  publication, its existing backup thread uses a separate read-only SQLite
  source connection and 256-page/two-millisecond online-copy steps, so it never
  occupies the critical persistence queue or writer connection. The copy is
  converted out of WAL mode and verified to be sidecar-free. The worker runs
  independent integrity and foreign-key checks and headlessly replays the copy,
  then binds its canonical root, content/schema/protocol, database checksum, and
  separately protected iroh key checksum/derived endpoint in a manifest. Restart
  scheduling trusts only fully verified generations.
  `cdda-server --restore` repeats verification, refuses overwrite, and atomically
  installs a private world whose restored identity retains the endpoint. First
  startup fully re-verifies and consumes the untouched backup manifest into
  durable provenance; later startups reject a different content identity or
  server key.
- Schema 15 records initial account creation, iroh enrollment proof, authenticated
  endpoint addition/revocation, local lost-key recovery, and expected rejections
  as typed, BLAKE3-verified security records. Records contain only safe actor
  account/endpoint/role, target, persisted tick, UTC, action, and outcome; no
  secret key or arbitrary argument/result is serializable. Endpoint IDs remain
  permanently single-account-bound. An enabled account may stage an exact
  ten-minute replacement, prove it through the enrollment ALPN, and revoke any
  but its last active ID. `cargo xtask account-recover` atomically revokes old
  IDs, recovery-locks the account, and stages an exact replacement without a
  password or separate credential. Remote non-curve key bytes are rejected and
  audited before consuming a binding. Local account creation/recovery refuses a
  live runtime marker. On-disk older schemas receive a private atomic generation
  before migration. Its manifest binds an integrity/foreign-key-checked,
  sidecar-free database copy and the exact protected sibling server identity
  when present; the exact file set and checksums are re-verified before
  publication.
- Protocol 35 implements a dedicated `cdda-rust/admin/1` surface without a
  password or second credential. It reauthorizes the iroh-authenticated operator
  in every SQLite transaction and returns its current moderator or administrator
  role at connection open. Both roles can page at most 128 account summaries,
  list a target's characters plus public live gameplay-session/controlled-actor
  metadata, and apply role-matrix-checked kick, mute, and
  suspension actions to another account for at most 24 hours. Mute is enforced
  on every chat attempt; kick and active suspension invalidate matching live
  connections. Administrators additionally change roles and explicit enabled/
  disabled/terminal-banned statuses and atomically transfer character ownership
  while rejecting destination-name conflicts and a 65th destination character.
  They also allocate accounts from an independent durable stable-ID sequence,
  list any account's at-most-256 endpoint records, stage a permanently unique
  exact endpoint for ten minutes, and revoke pending or non-last-active
  endpoints. Creation and staging never activate a key: that exact iroh identity
  must still complete the enrollment ALPN proof. The offline account CLI uses
  the same account allocator and cannot collide with live-created accounts.
  Self-targeting, invalid targets,
  enrollment/recovery bypass, and removal of the last available administrator
  are transactionally forbidden. Admin connections expire after five minutes,
  fail closed on audit/broadcast lag, use the 40/s burst-80 ingress limit, and
  default-deny unexpected messages. Every open, list, moderation, transfer,
  mutation, malformed-message, and rate-limit attempt appends a typed security
  record. Invalidation is published after durable commit and before a fallible
  response write. Players can submit a durable report against another account;
  details are bounded to 1,024 UTF-8 bytes and 512 characters, and each account
  may submit at most five reports in a rolling hour. Reports preserve both
  character names, but their free-form details are deliberately excluded from
  the typed security audit. Moderators and administrators can page at most 32
  reports, optionally filtered as open, actioned, or dismissed, and 128
  successful moderation-history entries per request. Report resolution is a
  one-way open-to-actioned/dismissed transition carrying the resolution UTC,
  operator account, and exact audit sequence; racing or repeated resolution
  fails closed. History is likewise linked to the exact audit sequence;
  rejected attempts remain visible in the security audit rather than in
  successful-action history. A real iroh test proves report submission,
  filtered lookup and resolution, history lookup, moderator denial of account
  creation, administrator account/endpoint management, exact-key enrollment,
  live connection lookup without private-state disclosure, muted-chat rejection,
  prompt suspension disconnect, and audit survival after reopen.
  Protocol 35 also joins durable character names to the locked live-session
  registry for moderator-visible account/controlled-actor presence. A distinct
  administrator-only inspection returns one canonical character's position,
  health, needs, sleep, readiness/input, equipment, and queued state; terrain
  memory is count-only and inventory is stable-ID-paged at eight items so the
  response remains inside the 64 KiB control bound. Authorization and requested
  actor/inventory cursor are typed audited, while the private result is not
  copied into recovery audit. Real iroh rejects a moderator and returns the
  disconnected character state to an administrator.
- The local CLI creates an exact ten-minute pending account. The client persists
  an owner-only iroh identity, pins the server identity, and enrolls over the
  dedicated ALPN without passwords, tokens, or application credentials. The
  macOS-compatible client binary also exposes one-shot authenticated key list,
  stage, and revoke commands over the gameplay ALPN. Moderator/administrator
  one-shot commands cover every implemented admin request: paged account,
  character/public-connection, private-character/inventory, report, history,
  and endpoint inspection; report resolution;
  kick/mute/suspension; account creation; endpoint staging/revocation;
  role/status changes; and character transfer. They reuse the profile's iroh
  key and pinned server identity, print copyable IDs/cursors, and turn typed
  rejections into nonzero exits. Each operation has a 30-second total deadline;
  no parallel credential or web surface exists.
- Authorized connections create or select a durable character, receive
  authoritative state, move, pick up/drop/wield/reload items, attack adjacent
  actors or pinned creatures, and fire a wielded revolver at an explicitly
  selected visible creature or actor. Ranged validation, terrain line of sight, seeded hit rolls,
  damage/death, ammunition expenditure, compatible-stack reloading, persistence,
  replay, replication, and a real iroh command path are wired end to end. The
  Bevy client renders the 12x12 chunk, local
  player, peers, disconnected actors, ground items, and hostile creatures while
  naming inventory and monsters from pinned content. Pinned clean water and
  cooked-meat stats drive a consume command; stored calories, thirst, and
  sleepiness advance every five in-world minutes for connected and disconnected actors, with
  starvation/dehydration damage and death persisted and replayed. A
  shared registry enforces at most 16 live gameplay sessions and one live
  session per account and character; 64 bounded connection tasks cap handshakes.
  Omitting `--character` now opens a bounded Bevy character list/create menu;
  the shortcut remains for automation. Authenticated pre-selection heartbeats
  keep the connection alive without disclosing snapshots, and all gameplay/chat
  input remains blocked until the authoritative character-ready response. Hello,
  list, and selection-response reads use the five-second frame-progress deadline.
  After character selection, the server pushes authoritative interest-filtered
  snapshots at 10 Hz without waiting for commands or heartbeats. Initial and
  later snapshots use independent server-opened, Zstandard-compressed bulk
  streams with 8 MiB encoded/32 MiB decoded bounds, five-second progress
  deadlines, lower QUIC priority, and codecs on Tokio's bounded blocking pool.
  One stream transmits while a one-slot watch queue retains only the newest
  replacement, preventing snapshot backlog.
  Control traffic can no longer represent a snapshot. The current
  bounded interest window is the specified 11x11-submap neighborhood. A dedicated
  client DTO excludes the world seed, namespace, allocator/event counters, and
  other actors' private inventory, equipment, needs, and command state. Terrain/furniture,
  actors, creatures, and items are masked by the phase-dependent authoritative line of
  sight using content-derived `TRANSPARENT` flags; opaque destination
  tiles remain visible while state behind them does not. Committed actor-relevant
  outcomes travel on a bounded, server-opened ordered iroh event stream only
  after SQLite commit; a lagging client fails closed rather than losing critical
  events. Authenticated, bounded character chat is server-routed on control
  traffic and displayed by the Bevy client. `/report-last <details>` submits a
  typed report against the latest other character seen in chat and presents a
  typed acceptance or rejection without exposing the administration surface.
- The repository vendors all 7,992 tracked files from the five required content
  roots at the pinned commit. The manifest records each source, destination,
  BLAKE3 hash, and upstream license; the importer, validator, server, and client
  fail closed on provenance, path, file-set, or hash divergence.
- A reproducible schema inventory covers 6,571 JSON files, 93,779 top-level
  objects, 180 definition types, and every observed top-level field. MOD_INFO is
  strictly loaded for all 47 pinned records; dependency/conflict/obsolete/core
  rules resolve the upstream recommended default mod order. The server also
  strictly loads the 158 ammunition types visible to that enabled mod set. The
  first ITEM registry finalizes 10,282 concrete definitions and 179 abstracts,
  including deferred/self `copy-from` and modifiers for 29 inventoried common
  fields; every definition retains the names of its unsupported fields.
- The first MONSTER registry finalizes 1,177 selected concrete definitions and
  33 abstracts through strict inheritance and modifiers. Canonical creature
  instances use content-derived zombie health, speed, aggression, and melee
  dice; stable-ID AI deterministically chases and attacks the nearest living
  actor even while that actor is disconnected. Player attacks, creature death,
  persistence, replay, networking, and Bevy presentation are wired end to end.
- The first terrain registry finalizes 1,246 selected concrete definitions and
  23 abstracts, retains explicit unsupported fields, and validates open/close
  transform references. Canonical 12x12 chunks persist the pinned terrain ID,
  move cost, and door transform for every tile. Protocol 81 renders strict
  pinned 24x24 JSON mapgen in Bevy with chunk-relative rebasing; the initial
  36-OMT/144-chunk bubble and newly intersected atomic OMTs persist canonically.
  A content-derived loaded revolver and compatible ammunition join the existing
  food, water, and rock starter items. Authoritative O/L commands open and close
  any admitted door, revise the chunk, journal the event, and immediately change
  collision. Persistent overmap layout and start-location selection remain.
- The first furniture registry finalizes 699 selected concrete definitions and
  one abstract through strict inheritance and modifiers. Eighteen inventoried
  identity, presentation, movement, comfort, flag, and transform fields load;
  all 33 unsupported fields remain explicit. Protocol 35 stores optional
  furniture beside terrain in every canonical tile, CanonicalStateV22, snapshots,
  replay, per-character memory, and current/remembered replication. Negative
  movement modifiers block actors and creatures, opaque furniture blocks sight,
  and the Bevy client colors and names furniture from pinned definitions. The
  current `lmoe` bootstrap has no furniture glyphs; definitions placed by later
  admitted OMTs retain the same canonical behavior. Furniture move
  cost now contributes to signed action debt; sleep-deprivation/comfort effects,
  storage, examination, moving, bashing/deconstruction, construction, and fire
  behavior remain unsupported.
- Clean server restart preserves its endpoint ID, world tick, accounts, and
  characters. Client restart selects the existing named character.

## Latest verification

Run on Apple-silicon macOS on 2026-07-28 against Protocol 81, persistence
schema/minimum recoverable schema 59, CanonicalStateV57, and
CanonicalEventsV18:

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --all-features` — passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps` —
  passed.
- `cargo xtask verify-dependency-boundaries` — passed; Bevy remains
  client-only.
- `cargo xtask parity-ledger-check` — passed for 13 version-bound milestones;
  active milestone `mapgen-overmaps`, all Rust paths and DAG edges valid.
- `cargo xtask content-validate` — passed for all 7,992 tracked files and
  manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`.
- `cargo xtask content-inventory-check` — passed for 6,571 JSON files, 93,779
  top-level objects, and 180 definition types after regenerating the checked
  MONSTER field-support classifications; all 1,374 `volume` and 284
  `attack_cost` occurrences are now marked loader-implemented.
- `cargo xtask astronomy-table-check` — passed for all 364 pinned Boston days.
- `cargo xtask cpp-oracle-check` plus the explicit item-group and mapgen-static
  scenario paths — passed against the exact pinned C++ commit and tree: the
  pocket kernel's eight Catch assertions, the item-group kernel's 34
  assertions, and the mapgen kernel's 17 assertions over OMT matching,
  orientation, rotation, and static phase ordering. All require exact
  version-1 observation equality and digest-checked adapter/runtime identity.
- `cargo test --workspace --all-targets --all-features` — passed, 317 tests,
  including atomic worldgen, SQLite snapshot/portable-replay recovery, all four
  real loopback iroh session tests in parallel, authoritative crafting,
  charged-tool bucket debit, resupply timing/capacity, offline
  interruption/resume/cancel, mandatory/optional/zero-learning proficiency
  behavior, recursive component/tool-`LIST` and tool-subtype expansion,
  canonical STR/DEX/INT/PER validation/hash/replication, pinned freeform
  4-through-20 creation bounds/defaults, rejection without actor-ID burn,
  exact custom-stat creation over real iroh, custom-stat SQLite recovery,
  actor-INT reading time and comprehension, and actor-INT/PER disassembly
  practice arithmetic. Protocol 66 additionally proves exact pinned
  DEX/practical-melee attack timing for unarmed and ordinary bash weapons,
  identical command/disconnected-defense costs, the upstream effective-stat
  cap of 20 despite the defensive serialized ceiling, and exact
  snapshot/SQLite/portable-replay recovery,
  Protocol 67's exact monster melee-skill/dodge loading, corpse/revival,
  privacy, hashing, and migration invariants, plus Protocol 68's deterministic
  connected and disconnected empty-hand/classic-zombie hits and misses, exact
  action cost, typed event, and SQLite/portable-replay equivalence,
  and Protocol 69's pinned ITEM accuracy-object/legacy/relative loading,
  fail-closed modifier rejection, hammer to-hit and dominant-bashing formula,
  connected/disconnected armed misses, CanonicalStateV46 profile hashing, and
  schema-48 SQLite/portable-replay equivalence,
  plus Protocol 70's sleeping-target classic-zombie hit/miss threshold,
  deterministic clumsy one-in-four fall, exact two-second down duration,
  no-damage/no-wake miss semantics, awake-target fallback, private replication
  boundary, corpse/revival retention, CanonicalStateV47/CanonicalEventsV14,
  and schema-49 SQLite/portable-replay equivalence,
  plus Protocol 71's arbitrary ordinary-monster sleeping-target skill
  transition and exact zero-dice early-return boundary,
  plus Protocol 72's strict inherited MONSTER volume grammar/default/modifiers,
  all five size thresholds and melee penalties, arbitrary-type
  medium-hit/tiny-miss transition, private hash/snapshot/corpse/revival
  retention, and schema-50 huge-target SQLite/portable-replay equivalence,
  plus Protocol 73's pinned static-`IMMOBILE` action ordering/full readiness
  clearing, exact 40-point player-hit modifier, content projection, private
  hash/snapshot/corpse/revival retention, and schema-51 tiny-target
  SQLite/portable-replay equivalence,
  plus Protocol 74's static-`PACIFIST` content projection, ordinary-melee-only
  suppression, preserved target pursuit, private hash/snapshot/corpse/revival
  retention, and schema-52 SQLite/portable-replay equivalence,
  plus Protocol 75's strict inherited MONSTER attack-cost default, modifiers,
  selected-content range validation, exact hit/miss charging, signed debt and
  banked multi-attack behavior, private hash/snapshot/corpse/revival retention,
  and schema-53 SQLite/portable-replay equivalence,
  plus Protocol 76's bounded ordered magazine wells, canonical pocket
  identity, explicit reload selection/result events, exact powered-tool pocket,
  all-well disassembly detach, multi-well nested stable-ID restoration,
  active-tool auxiliary reload with power-pocket exclusion,
  CanonicalStateV52/CanonicalEventsV15, and schema-54 SQLite/portable-replay
  equivalence,
  plus Protocol 77's ordered strict integral MAGAZINE pockets, item-backed
  whole/split/merge reload identities, pinned reload/unload access, nested
  battery debit/residual energy, recursive snapshot/ID validation,
  CanonicalStateV53/CanonicalEventsV16, and schema-55 direct/snapshot/SQLite/
  portable-replay equivalence,
  plus Protocol 78's explicit stable-ID integral/well removal, fractional loose
  battery unload/reload, atomic access/identity/capacity rejections,
  CanonicalStateV54/CanonicalEventsV17, and schema-56 direct/snapshot/SQLite/
  portable-replay equivalence,
  deterministic legacy byproduct capacity/cancel/order validation, and
  mid-craft recovery/replay, plus forged-book-payload normalization and
  interrupted/offline book-study recovery/replay. Exact-prerequisite terrain
  construction additionally proves furniture-layer preservation, snapshot
  restoration, SQLite recovery, and portable replay. New detachable-battery
  coverage proves bounded pocket projections, empty-capable magazine
  validation, compatible reload and exact swap identity, installed-energy tool
  debit, incompatible atomic rejection, unload-before-disassembly, cancel
  anti-duplication, hostile loaded-activity and duplicate-ID rejection, client
  exact-pocket battery selection and power display, and exact SQLite
  recovery/portable replay. New coverage proves forged
  disassembly bodies are replaced, out-of-range item damage rejects, exact
  target/wield/condition state survives cancel, damage interruption and resume
  are deterministic,
  recovered/destroyed components use reserved stable IDs, permanent learning
  authorizes crafting, the complete 1,227-entry client/server catalog agrees and
  fits the codec, active disassembly recovers through SQLite/replay, and a
  verified older-schema backup migrates through schema 49. Field coverage adds
  strict pinned loading, fixed-point decay bounds, death splatter, snapshot
  restoration, visibility non-disclosure, SQLite recovery, and portable replay.
  Corpse coverage additionally proves exact pulverization boundaries, strict
  self-contained snapshots, atomic stable-ID exhaustion, ground-to-inventory
  transfer, carried revival placement, damage-scaled stats, initial downed
  state, blood plus corpse recovery, and portable replay.
  Monster-vision coverage additionally proves pinned inherited 40/3 ranges,
  natural darkness, dynamic target illumination, opaque LOS, deterministic
  pursuit resumption, and vision retained through corpse reconstruction.
  Structural-bash coverage proves exact profile parsing, inherited two-stage
  wooden-door definitions, connected group strength, blocked thresholds,
  semi-persistent damage, atomic stable-ID exhaustion, deterministic direct
  debris, hit/destroyed fields, attempt sound/action cost, strict restoration,
  SQLite recovery, and portable replay.
  Monster-hearing coverage proves content-derived starter volume 70, canonical
  `bang!` origin and volume, deaf/normal/`GOODHEARING` behavior, private
  same-z sound goals, visual-memory priority, per-action expiry, structural
  sound ingestion, strict restoration, replication non-disclosure, SQLite
  recovery, and portable replay.
  Monster door-opening coverage proves incapable versus capable behavior,
  opening-before-bashing selection, the pinned zero-cost open-plus-enter turn,
  canonical `swish` volume 6, structural-damage clearing, sound-goal pursuit,
  content-derived feral capability, interior-only fail-closed transforms,
  corpse/revival preservation, strict restoration, SQLite recovery, and
  portable replay.
  Monster-routing coverage proves a pinned positive-distance creature takes a
  deterministic nonprogress step around a wall while a distance-zero creature
  remains on greedy fallback, route-open door choice recovers/replays, settings
  survive corpse revival, distance 401 is rejected, and the public projection
  excludes the private routing policy. Route-bash coverage additionally proves
  the pinned three cost classes, ordinary-bound/base-strength planning versus
  blocked-bound/group-strength execution, destructive-shortcut versus walkable-
  detour selection, typed bash
  events, snapshot identity, SQLite recovery, and portable replay of a
  sideways-first destructive route.
  Player-smash coverage proves authoritative visible-only menu metadata,
  hidden-memory non-disclosure, eight-direction targeting, invalid-tool
  rejection, exact Strength-8-plus-hammer damage, mixed-profile fail-closed
  behavior, furniture precedence, persistent terrain damage, typed sound
  events, snapshot identity, SQLite recovery, and portable replay.
  Creature-movement coverage additionally proves a 100-move floor baseline,
  exact 175-move floor/bed transitions, signed debt recovery boundaries, and
  snapshot/hash restoration. Stumble coverage proves the pinned candidate fan,
  named-RNG reproducibility, weighted flanking selection, Q30 Euclidean cost,
  direct-diagonal cost, corpse preservation, and SQLite/portable-replay state.
  Last-seen-goal coverage proves acquisition, occluded pursuit, arrival
  clearing, snapshot identity, SQLite/portable-replay retention, and the
  server's restricted visible-creature projection.
  The construction
  coverage proves strict 776-definition/438-group loading, the exact 17-entry
  server catalog, authoritative payload replacement, adjacent empty-target and
  detail-light validation, disconnected completion, exact split-component and
  wield restoration on cancel, interruption after target mutation, exclusion
  of competing activities while interrupted, far-target rejection before any
  remote chunk generation, exact `FLAT` target enforcement, and identical
  snapshot, SQLite, and portable-replay furniture outcomes. The powered-light
  coverage additionally proves strict inherited W/kW/mW parsing, complete
  transform-only classification, the exact 18-definition powered-light family,
  its 18 battery-shaped storage definitions and compatible cell alternatives,
  fail-closed behavioral-field/multi-action exclusion, integer low-light
  falloff, carried-versus-external detail behavior, exact residual-energy
  `CHARGEDIM` scaling, zero-energy darkness before delayed transform reversion,
  a powered craft output, three
  reversible cell-detach targets, stable-ID activation validation,
  one-charge startup, 1,560 mJ/s fractional drain while disconnected or on the
  ground, automatic reversion, local detail light/LOS and privacy-masked
  replication, client activation filtering, and identical snapshot, SQLite,
  and portable-replay outcomes. Provenance coverage additionally proves an
  actually consumed alternative retains its
  condition and nested history, overrides a deliberately wrong recipe default,
  survives cancellation, and recovers exactly. A separate review found and
  fixed completion-time provenance overflow: the server now previews exact
  split IDs and component trees at admission, rejecting an over-depth result
  before consuming items, spending tools, or allocating stable IDs.
  Ranged-unload coverage additionally proves the three-entry pinned catalog,
  client visibility only with canonical ranged state, exact start-time ground
  ammunition, stable-ID exhaustion and ammunition-type mismatch atomicity,
  the empty-weapon no-allocation path, hostile still-loaded activity rejection,
  cancel anti-duplication, completion, snapshot restoration, SQLite recovery,
  authoritative payload replacement, and portable replay.
  Tool-charge coverage additionally proves inherited scalar/list replacement,
  extension, and deletion; the exact 76-entry empty-only corpus boundary;
  client hiding and allocation-free authoritative rejection for charged
  instances; integral-charge unload/cancel anti-duplication; hostile activity
  rejection; and exact SQLite recovery and portable replay.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --all-features` —
  passed.
- `cargo xtask verify-dependency-boundaries` — passed; no non-client workspace
  member reaches `bevy` or `bevy_*` transitively.
- Live macOS client-operator test — an enrolled administrator authenticated on
  the admin ALPN, listed accounts, and created an exact pending player account;
  that key enrolled, listed its binding, staged and proved a second profile key,
  revoked the original, and listed the resulting durable revoked/active states.
- `cargo xtask astronomy-table-check` — passed for all 364 pinned Boston days.
- `cargo xtask content-inventory-check` — passed for 6,571 JSON files,
  93,779 top-level objects, and 180 definition types after regenerating the
  ITEM, skill, proficiency, recipe, and requirement field-support
  classifications.
- Manual macOS process test — server ran through multiple five-second
  snapshots, clean shutdown, and restart with the same endpoint ID; tick
  advanced from 269 to 672 after restart.
- Manual two-client macOS test — two independent owner-only client keys enrolled
  as separate accounts, two Metal/Bevy windows connected simultaneously, and
  SQLite retained one distinct character for each account.
- Manual SIGKILL recovery test — SQLite integrity remained `ok` with the latest
  snapshot at tick 400 and journal at tick 436; restart hash-verified the 36-tick
  gap, retained the server endpoint ID, advanced to tick 806, and checkpointed
  at journal sequence 403.
- `cargo xtask content-validate` — passed for 7,992 vendored files with manifest
  hash `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`.
- Separate full crafting-diff review — validated the loader against pinned
  `requirement_data` replacement/extension semantics and the normalized journal
  path. It found one high-risk recovery gap: a forged snapshot could describe
  reservations that were individually valid but did not reconstruct the exact
  deterministic recipe plan, including an overflowing split parent. Recovery,
  cancellation, and completion now preflight exact reservation reconstruction;
  the follow-up pass found no remaining critical/high crafting defect.
- Separate tool/quality crafting review — checked every published recipe through
  the bounded control codec and independently traced provider selection,
  component overlap, interrupted support loss, snapshots, replay, and client/
  server agreement. It found a solid comestible result whose content `charges`
  value is zero even though CDDA creates one item instance; output normalization
  now clamps non-count-by-charge results to one. Published recipes are also
  required to consume at most half the private-inspection frame limit, leaving
  room for the surrounding canonical character state. The follow-up pass found
  no remaining critical/high defect in this slice.
- Separate skills/practice crafting review — traced autolearn, practical-versus-
  theoretical eligibility, caps, nominal-time practice, interruption,
  cancellation, snapshots, replay, private inspection, and client/server
  agreement against the pinned C++ source. It found a high-risk recovery gap:
  an otherwise valid snapshot could understate awarded practice and grant the
  deferred experience after recovery. Skill-bearing craft snapshots now require
  the exact practice boundary implied by canonical progress; skill-less crafts
  require zero. It also found and fixed skill-less live progress that recovery
  rejected, and private inventory pagination that rejected valid caller-selected
  page sizes below eight. Every exposed recipe now proves second-aligned nominal
  time, and private responses are encoded and deterministically shortened before
  transmission when necessary. The follow-up pass found no remaining critical/
  high defect in this slice.
- Separate charged-tool crafting review — traced the pinned start/resume formulas
  and twenty progress buckets through ordered alternatives, aggregate stable-ID
  depletion, cancellation, offline progress, protocol bounds, private event
  routing, snapshots, and replay. It found high-risk interrupted-resupply gaps:
  ordinary actions bypassed readiness, could mutate a reserved partial-stack
  parent, and could fill capacity required by cancel or completion. Those actions
  now retain normal costs, protected parent mutations reject cleanly, and the
  larger of restoration/output capacity remains reserved. The added recovery
  assertion found that exact depleted-tool snapshots were wrongly subjected to
  start-time support checks; immutable ingredient reconstruction is now separate
  from support checks performed on resume. Client prediction also now aggregates
  repeated same-type tool groups like the server, and invalid zero-count or
  nondefault-charge-factor content fails closed. The post-fix pass found no
  remaining critical/high defect in this slice.
- Separate inherent-quality-speed review — traced ITEM quality parsing and
  `best_quality_speed_modifier`/`compute_tool_speeds` in the pinned source. CDDA
  consults quality speed only for `steps` recipes, all of which remain
  unavailable, so non-unit annotations are now valid inherent providers for the
  legacy recipe model without introducing floating point into canonical state.
  Server and client tests prove `circsaw_on` (CUT 1 at speed 0.5) satisfies
  `pointy_stick`; that boundary's 316 recipes pass codec/frame bounds. Charged
  qualities and per-step speed were still fail-closed. The follow-up pass found no
  critical/high defect in this boundary.
- Separate charged-quality review — traced the pinned crafter-aware
  `get_quality_for_crafter`, `ammo_sufficient`, and `shots_remaining` path.
  Positive bounded `charges_per_use` is now carried in each normalized provider
  descriptor: every selected instance must independently meet its threshold,
  qualification spends no energy, and zero-cost/external-only providers remain
  unavailable. This exposed exactly three additional DRILL recipes at that
  boundary (501 total before component `LIST` expansion).
  Server catalog/frame tests pin `cordless_drill` at five charges; simulation
  proves two providers cannot pool energy and retain their charges; client
  prediction proves the same four/five-charge boundary. Protocol validation,
  CanonicalStateV22, snapshot recovery, and replay retain the self-contained
  descriptor. The current aggregate item charge models loaded/linked energy only
  until pockets, batteries, grids, and UPS support arrive. The post-fix review
  found no remaining critical/high defect in this slice.
- Separate proficiency-crafting review — traced selected-content factory and
  recipe inheritance, required gates, the upstream cosine time-malus curve,
  5% whole-second practice quantization, prerequisites, learning multipliers,
  experience caps, offline progress, client/server agreement, protocol bounds,
  canonical state, recovery, and replay against the pinned C++ source. It found
  and fixed three high-risk state-shape errors: required entries retained a
  default skill penalty instead of canonical zero; zero-learning recipes could
  create an invalid empty practice record; and proficiency-free crafts advanced
  a new boundary counter that recovery correctly rejected. Protocol 35 now
  rejects malformed required entries, zero-learning entries create no state,
  and proficiency-free activities retain zero proficiency counters. The
  post-fix full review found no remaining critical/high defect in this slice.
- Separate component-`LIST` review — traced pinned
  `requirements.cpp::inline_requirements` and implemented recursive expansion
  of the first same-kind requirement group, checked multiplier composition,
  encounter-order preservation, and minimum-count duplicate collapse. Unknown,
  cyclic, empty, oversized, unsupported, or missing-item expansions fail closed.
  Pinned vegetable juice proves raw `LIST` data becomes concrete tomato and
  zucchini alternatives through content loading, the then-1,005-entry authoritative
  server catalog, bounded protocol encoding, and Bevy menu eligibility. Tool
  `LIST` and subtype-replacement semantics remained unavailable at that boundary. The post-fix
  source review found no remaining critical/high defect in this boundary.
- Separate tool-`LIST`/subtype review — traced pinned requirement and item
  finalization together. Recursive tool references now use the first tool group,
  checked positive multipliers, presence clamping, encounter-order/minimum-count
  deduplication, and then base-first stable-ID subtype replacement through the
  inherited ITEM `sub` chain. Missing or cyclic subtype/reference graphs fail
  closed. Pinned makeshift cards prove five-charge drawing-tool expansion through
  content, the 1,990-entry authoritative catalog, protocol bounds, and the Bevy
  menu. The post-fix source review found no remaining critical/high defect in
  this boundary.
- Separate legacy-byproduct review — traced pinned recipe loading,
  `create_byproducts_legacy`, item default-charge precedence, activity
  completion, cancellation, snapshots, recovery, and replay. The review fixed
  hybrid COMESTIBLE/AMMO defaults (for example charged buttermilk) and retained
  schema migration marker 15 beneath schema 23. Main results plus sorted
  byproducts now reserve one bounded stable-ID sequence; all 1,990 catalog
  recipes pass codec validation. The post-fix review found no remaining
  critical/high defect in this boundary.
- Separate batch-factor review — traced both pinned deserializers and
  `batch_savings::apply`, including inheritance and linear setup bounds. The
  legacy array and explicit logistic/linear shapes now load exactly; at the only
  currently admitted batch size of one, both formulas return the ordinary
  recipe time. Pinned inherited `seed_oats` and the complete 1,990-recipe server
  codec path pass. Multi-item batches remain explicit future work. The post-fix
  review found no remaining critical/high defect in this single-unit boundary.
- Separate book-learning-metadata review — traced the pinned legacy-array and
  explicit-map deserializers, `book_recipe_data`, inheritance, first-duplicate
  insertion, and BOOK consistency checks. Both shapes now retain the exact
  skill-level, alternate-name, and hidden metadata. Pinned inherited
  `acidchitin_armor_cow` passes through the complete authoritative catalog,
  while pinned `cottage_cheese` proves that book metadata alone does not bypass
  the existing autolearn authorization gate. At that metadata-only boundary,
  reading and canonical learned-recipe state were unavailable; timed study was
  implemented in a later slice, followed by permanent disassembly learning.
  All 1,990 catalog recipes passed the bounded server codec path; the independent
  source/diff pass found no remaining critical/high defect in that boundary.
- Separate disassembly-learning-metadata review — traced the pinned recipe
  loader, `Character::can_decomp_learn`, and the one-in-four learning branch.
  Scalar values bind to the effective primary skill, explicit lists replace the
  inherited map and use pinned last-entry-wins assignment, empty lists clear it,
  and selected skill references validate. Pinned `flashlight` retains its
  zero-level electronics threshold. At that metadata-only boundary the
  autolearn gate remained the sole knowledge authority: neither metadata nor a
  client claim could learn a recipe. The subsequent authoritative disassembly
  slice now owns the canonical learned-recipe state. The metadata boundary
  exposed 47 independently autolearnable recipes for
  a 1,969-entry bounded server catalog; the final source/diff pass found no
  remaining critical/high defect.
- Separate `extend.using` review — traced generic pinned JSON inheritance and
  recipe external-requirement finalization. A root `using` value still replaces
  the inherited vector, while `extend.using` appends in encounter order and
  uses the same positive/truncating multiplier parser, overflow checks,
  recursive `LIST` expansion, and server normalization. Pinned
  `ch_sheet_metal_small` now resolves inherited blacksmithing requirements plus
  25-charcoal/coal carbon alternatives and becomes the 1,970th bounded catalog
  entry. The complete post-change source/diff pass found no remaining critical/
  high defect in this sole corpus boundary.
- Separate `ALLOW_ROTTEN` review — traced pinned component filtering and audited
  every canonical item/prototype/snapshot/replay field for rot or spoilage state.
  No representable component can be rotten, so flagged and ordinary non-rotten
  filters accept the same current domain; the flag needs no wire field and does
  not weaken any component, support, skill, capacity, or authorization check.
  Exactly 20 otherwise complete recipes join the 1,990-entry bounded catalog,
  including pinned `crude_lamp_oil`. `BLIND_HARD`, `FULL_MAGAZINE`, `NO_RESIZE`,
  and all other flags remain closed. The final full-diff review found no
  remaining critical/high defect in this invariant-bound adaptation.
- Separate carried-book recipe review — traced pinned recipe-dictionary
  finalization, BOOK `required_level`, `Character::get_recipes_from_books`,
  identification, and theoretical primary-skill checks. Protocol 35 stores a
  sorted bounded set of book type IDs and effective thresholds in every
  normalized recipe. The server admitted 3,048 complete recipes at that
  boundary, rejected missing
  books and insufficient theory, preserves the carried book, and keeps
  autolearn distinct from live book knowledge. All current concrete type IDs
  are explicitly treated as identified because identification is not yet
  canonical state. Content, simulation, client prediction, protocol bounds,
  CanonicalStateV22, schema-23 recovery, and the complete pinned catalog are
  covered. The source/diff review found no critical/high defect in this slice;
  identification discovery, helper inventories, and ebooks remained separate
  boundaries at that review; the subsequent timed-study slice is documented
  above.
- Separate timed-book-study review — traced pinned `time_to_read`,
  `read_activity_actor::read_book`, `SkillLevel::readBook`, and
  `knowledge_train` behavior through strict ITEM inheritance, server catalog
  normalization, admission, interruption, canonical state, SQLite recovery,
  portable replay, real iroh, and Bevy presentation. The review found and fixed
  one high-risk determinism flaw: completion tick had leaked into the XP seed,
  so delaying resume could change an already-started study's roll. Study XP is
  now fixed by world seed, actor/book stable IDs, domain, and accepted start
  sequence; a delayed-resume clone proves the same gain. The post-fix full pass
  found no remaining critical/high defect in this slice. Custom generation,
  non-default focus, identification, helpers, morale, ebooks, recreational reading,
  and general lightmaps remain explicitly outside it.
- Separate disassembly/permanent-learning review — traced pinned
  `can_disassemble`, `complete_disassemble`, component recovery dice,
  `can_decomp_learn`, and one-in-four learning through strict content selection,
  authoritative normalization, support selection, admission, interruption,
  cancellation, stable-ID reservation, named RNG, canonical state, schema-23
  SQLite recovery, portable replay, replication, private inspection, and Bevy
  presentation. All 91 published definitions are exactly the same set shown by
  the client helper and fit the bounded control codec. The review fixed a client
  regression test that had used pinned `36navy`, which has no `decomp_learn`;
  pinned `flashlight` now proves learned-recipe prediction. It also found and
  fixed a high-risk migration-ledger gap during the schema-19 slice: database
  creation skipped the immutable schema-18 marker, breaking verified
  immediately-prior backup reconstruction. Schema 22 now records every prior
  marker. Finally, strict client/server selection rejects targets
  with default charges until canonical power unloading exists, and
  the complete catalog asserts that invariant. The post-fix pass found no
  remaining critical/high defect in that strict provenance-free boundary.
  A follow-up Protocol 34 pass added canonical damage levels 0 through 4 and the
  exact pinned condition multiplier, with mixed recovery, cancel restoration,
  protocol bounds, recovery, and replay coverage. The next pass loaded all
  1,428 concrete core `uncraft` definitions in a separate inherited dictionary,
  applied upstream explicit-over-reversible target precedence, and expanded the
  codec-checked client/server catalog to 1,099 definitions. The subsequent
  Protocol 34/schema-22/CanonicalStateV21 provenance pass retained exact
  consumed component objects recursively, implemented pinned first-alternative
  defaults for ordinary items, and expanded the codec-checked client/server
  catalog to 1,224 definitions. The subsequent ranged-unload pass added the
  three pinned bare weapons, making 1,227 definitions, and proved
  unload-before-reserve, cancel anti-duplication, canonical recovery, and
  portable replay. The Protocol 35/schema-23/CanonicalStateV22 tool-charge pass
  then implemented inherited `tool_ammo`, preserved 76 powered-tool recipes
  behind an exact empty-storage guard, and generalized exact integral-charge
  unloading without claiming a pinned candidate. The Protocol
  36/schema-24/CanonicalStateV23 pass then implemented inherited `capacity`,
  strict pocket-derived projections, and the exact flashlight/medium-battery
  pair, leaving 75 recipes behind the empty-only guard. The Protocol
  37/schema-25/CanonicalStateV24 pass then implemented the pinned off/on
  transform, exact residual energy, persistent draw, and the first local light
  source without widening any other powered-item boundary. The Protocol 38
  pass then admitted the exact nine-pair transform-only
  detachable-battery family, retained 18 exact battery-shaped storage
  projections, added integer open-air attenuation plus personal/external detail
  rules, and reduced the still-empty-only disassembly set from 75 to 73
  without changing schema or canonical DTO shape. Protocol 39/schema-26/
  CanonicalStateV25 then persisted the finalized `CHARGEDIM` bit and scales all
  nine pinned dimming pairs from exact fractional installed energy. Construction, general
  detachable magazines/mods, other pockets and power systems, and
  charged/special tool substitutions remain explicit later boundaries.
- Real content-bound server smoke test — validated the complete package,
  finalized 10,282 concrete items and 179 abstracts, resolved ammunition default
  item references (with two explicit upstream virtual categories), allocated
  initial ground loot, preserved its iroh endpoint across restart, reached tick
  681, and retained SQLite integrity `ok`.
- Fresh creature-enabled macOS server smoke test — finalized 1,177 concrete
  monsters and 33 abstracts, persisted the initial zombie through the canonical
  snapshot, restarted with the same iroh endpoint, advanced commandlessly from
  tick 280 to 486, and retained SQLite integrity `ok`.
- Fresh terrain/needs macOS smoke test — finalized 1,246 concrete terrain
  definitions and 23 abstracts, generated and snapshotted the cabin plus three
  pinned starter items, restarted with the same endpoint, reached tick 667,
  and retained SQLite integrity `ok`. A native Bevy 0.19 client window also
  initialized successfully on Metal with the expanded disjoint tile/entity
  queries.
- Full nine-submap macOS play smoke test — enrollment and durable character
  creation succeeded through a native Metal client, the server advanced to tick
  991 with 500 committed journal batches, the final snapshot compressed to 326
  bytes, and SQLite integrity was `ok`.
- `cargo xtask replay-export` plus `cargo xtask replay-verify` against that real
  smoke-test database — passed through tick 991 across five journal batches;
  the 830-byte archive reproduced canonical state hash
  `8be1090d26dd6e184661607a9f2f59bdf3181636153f05608bfa5e279162c81f`.
- Fresh protocol-5 macOS smoke test — a native Bevy 0.19 Metal client enrolled,
  created `Protocol Five Survivor`, and consumed 10 Hz visibility-masked state
  without a rendering or query failure. The Bevy-free server retained its iroh
  identity, committed 427 journal batches through tick 848, wrote a 336-byte
  final snapshot, and SQLite integrity was `ok`. A 2,826-byte replay archive
  reproduced all 31 batches after its initial snapshot and final state hash
  `b10db434ecd331cf5668ad0bebc3b3e30fcc934630b9a103dedf719742a45f99`.
- Fresh protocol-6 macOS smoke test — a native Bevy 0.19 client enrolled over
  iroh, created `Protocol Six Survivor`, opened the server-owned event stream,
  and rendered successfully on Apple Metal. The server retained endpoint
  `6bee9d3eb3db130df44c3de9d34e15aecb9082f60502e07dcfea8b8847c647cd`.
  A deliberate SIGKILL left SQLite integrity `ok`; restart detected 11 seconds
  of unclean downtime and journaled 220 catch-up ticks before accepting peers.
  Final clean shutdown reset the runtime marker, checkpointed at tick 1,913,
  and retained the character. Replay export produced a 2,448-byte archive whose
  26 batches verified through tick 1,913 with final state hash
  `74797d1f3364805bdd66e248f0a6ab98fb1cfc64aed669c2b382bed550b64af7`.
  This remains historical protocol-6 evidence; the current wire protocol is 7
  after adding the tick-derived calendar.
- Fresh protocol-7 macOS process smoke test — newly built standalone binaries
  enrolled a fresh owner-only iroh client identity, created `Protocol Seven
  Character`, and exchanged calendar-bearing state while a native Bevy 0.19
  window rendered through Apple M2 Max Metal. Clean shutdown left the runtime
  marker inactive, SQLite integrity `ok`, 319 journal batches, and a tick-994
  snapshot. A 3,919-byte replay archive verified 45 post-snapshot batches
  through tick 994 with final state hash
  `edf06b75c6be0681d697ab39a618ce5102cb67ef4ce6e1461b20f8a32a3866c2`.
  This also exercised the dedicated persistence worker in a real process. The
  generated world was moved recoverably to
  `~/.Trash/cdda-rust-v7-smoke.ersNr2`.
  This remains historical protocol-7 evidence; current protocol 8 is verified
  separately below.
- Fresh protocol-8 macOS process smoke test — a newly built Bevy-free server
  enrolled a fresh iroh identity, created `Memory Keeper`, and delivered the
  filtered current/remembered terrain DTO to a native Bevy 0.19 client window
  rendered through Apple M2 Max Metal. Clean shutdown left the runtime marker
  inactive, SQLite integrity `ok`, 401 journal batches, and a 519-byte tick-796
  snapshot containing canonical map memory. A 1,712-byte replay archive verified
  15 later batches through tick 796 with final CanonicalStateV4 hash
  `4a1bbb9be5aa69997466056f5d7d560e48595839e25917fd4c212fc70818f8fe`.
  The generated world was moved recoverably to
  `~/.Trash/cdda-rust-v8-smoke.qYEv8F`.
  This remains historical protocol-8 evidence; current protocol 9 is verified
  separately below.
- Fresh protocol-9 macOS process smoke test — a newly built standalone server
  enrolled a fresh iroh client identity, created `Sky Watcher`, and delivered
  validated calendar/solar/lunar/sight metadata to a native Bevy 0.19 window on
  Apple M2 Max Metal. Clean shutdown left the runtime marker inactive, SQLite
  integrity `ok`, 446 journal batches, and a 522-byte tick-886 snapshot. A
  1,928-byte replay archive verified 17 later batches through tick 886 with final
  CanonicalStateV4 hash
  `addf3248f313b861cb6e82059cebaf40c87bdaadd2f804fe3afb4c1a71c7a5db`.
  The generated world was moved recoverably to
  `~/.Trash/cdda-rust-v9-smoke.vCZ5RK`.
- Fresh protocol-10 macOS process smoke test — a fresh owner-only iroh client
  identity enrolled against a newly built standalone server, created `Sleep
  Walker`, and received the sleep-bearing authoritative DTO while a native Bevy
  0.19 window rendered through Apple M2 Max Metal. Clean shutdown left the
  runtime marker inactive, SQLite integrity `ok`, 490 journal batches, and a
  519-byte tick-971 snapshot. An 865-byte replay archive verified four later
  batches through tick 971 with final CanonicalStateV5 hash
  `7fe1502142654b7c9690f33e3e63af8d87c237d1c2b8de7e7e36df440812942f`.
  The generated world was moved recoverably to
  `~/.Trash/cdda-rust-v10-smoke.3Dt7bd`.
- Fresh protocol-11 macOS process smoke test — the standalone Bevy-free server
  finalized 699 concrete furniture definitions and one abstract, retained its
  iroh endpoint across initialization/restart, enrolled a fresh owner-only
  identity, and durably created `Furnished Survivor`. A native Bevy 0.19 window
  initialized through Apple M2 Max Metal and consumed the furniture-bearing
  authoritative DTO. Clean shutdown left the runtime marker inactive, SQLite
  integrity `ok`, 596 journal batches, and a 624-byte tick-1,181 snapshot. A
  1,043-byte replay archive verified five later batches through tick 1,181 with
  final CanonicalStateV6 hash
  `3389f0fbbc31f0d8e37a32205fcc2719867b86164b982ddc80ac72a127b9aaf9`.
  The disposable world was moved recoverably to
  `~/.Trash/cdda-rust-v11-smoke.dtbu46`.
- Fresh protocol-12 macOS process smoke test — a fresh iroh endpoint enrolled,
  created `Debt Runner`, and consumed the signed-readiness authoritative DTO in
  a native Bevy 0.19 window through Apple M2 Max Metal. The Bevy-free server
  finalized the furniture/terrain registries and cleanly retained its endpoint;
  shutdown left the runtime marker inactive, SQLite integrity `ok`, 519 journal
  batches, and a 616-byte tick-1,029 snapshot. A 3,639-byte replay archive
  verified 39 later batches through tick 1,029 with final CanonicalStateV7 hash
  `d5ddc1aca8365d65c18c85cace12bab3cbd1f33947665830553143a1604255d3`.
  The disposable world was moved recoverably to
  `~/.Trash/cdda-rust-v12-smoke.cf9bQ3`.
- Fresh protocol-13 macOS process smoke test — newly built standalone binaries
  retained the server's iroh endpoint across initialization and restart,
  enrolled a fresh owner-only identity, and durably created `Bulk Runner`. A
  native Bevy 0.19 window initialized on macOS 26.5.2 through Apple M2 Max Metal
  and consumed the initial and recurring compressed snapshot streams. Clean
  shutdown left the runtime marker inactive, SQLite integrity `ok`, schema 6,
  1,120 journal batches, 24 snapshots, and a 623-byte tick-2,218 snapshot. A
  3,654-byte replay archive verified 39 later batches through tick 2,218 with
  final CanonicalStateV11 hash
  `eaaa2cb90168adcf8152ab07d7528b7f209a8b0c189fc6595379aa2c9cacebd8`.
  The disposable world was moved recoverably to
  `~/.Trash/cdda-rust-v13-smoke.ClEsCP`.
- Fresh schema-7 macOS archive smoke test — the standalone Bevy-free server
  retained endpoint
  `c87b6945341cac716de6b7ef982d045eafc92b23f79eb53ce6140153f77bca58`
  across restart, replayed a startup allocator boundary, and produced the
  forced-due hourly range from journal sequence 1 through 732. The 53,047-byte
  owner-only archive independently verified 731 batches through tick 1,448 with
  CanonicalStateV11 hash
  `da2d071a786a7bc279ab18489be1592ca98b516ab750fcabf55417802602733d`.
  Its cursor advanced only after the archive log reported BLAKE3
  `82c9f22cafc877d02b103c478442d78e91f8316faff0ce78a1a8870f2dddf64c`;
  clean shutdown left schema 7 inactive, SQLite integrity `ok`, 1,377 journal
  batches, and 31 snapshots. The disposable world was moved recoverably to
  `~/.Trash/cdda-rust-v13-archive.rOq4oM`.
- Fresh schema-10 macOS backup/restore smoke test — the rebuilt standalone
  server retained endpoint
  `bace1e0fe651d6d02b3bcb6057ba7527c2fc31ff9e8b8351fa4a99732921ee00`
  and automatically published a private verified backup at journal sequence
  193/tick 380 with database BLAKE3
  `a8a966d5fe8e4f8186d4775ea69dad917bf26ec109573026db0749c94acf8fbc`.
  The real `cdda-server --restore` command installed it into a new directory;
  that server derived the same endpoint, deterministically journaled 33 seconds/
  660 ticks of restored downtime through tick 1,040, and published another
  verified backup at journal sequence 195. Clean shutdown left schema 10
  inactive with 428 journal batches, 12 snapshots, matching 0600 identity files,
  0700 generation directories, and SQLite integrity `ok`. The disposable source
  and restored worlds were moved recoverably to
  `~/.Trash/cdda-rust-v13-backup.dew1k4`.

## Known defects and risks

- Tick/command replay, portable replay export/verification, exact crash-downtime
  replay, and commit-gated
  gameplay acknowledgements are implemented,
  and actor creation has a crash-reconciled durable spawn record ordered by its
  exact journal boundary. Ephemeral
  socket presence is excluded from canonical hashes and resets offline on
  recovery without removing the actor. Initial account, enrollment, endpoint
  rotation, revocation, rejected attempts, and local lost-key recovery are
  exact typed security recovery inputs; remote account/endpoint management,
  role/status attempts and mutations, ownership transfers, moderation actions,
  player-report attempts, filtered report queries/resolution, and moderation-
  history queries are now exact typed security inputs; every applicable action
  revokes affected sessions. Other administrative inputs remain. Account-ID
  allocation and simulation allocator reservation/
  abandonment are exact. Exact downtime
  currently expands to 20 journal records per
  elapsed second, so analytical warm/dormant catch-up remains required for long
  outages. Content-addressed snapshot objects, daily recovery compaction, and
  online backup/restore and automatic verified pre-migration database backups
  are implemented. Snapshot-object garbage collection is fail-closed over the
  fully verified retained replay set. Its current hourly proof replays the full
  retained set, so a verified reference catalog/cache remains necessary at
  large-world scale. Full persistence/process-crash failure-injection coverage
  remains.
- Gameplay pushes a bounded 11x11 interest snapshot at 10 Hz and on durable
  commands/heartbeats; pre-character heartbeats cannot disclose world state.
  Canonical persistence fields and private peer state are no longer representable
  in replication, and current terrain/entity occlusion is server-derived.
  The 60-tile daylight 121-chunk DTO intentionally exceeds the control-frame
  budget and uses bounded compressed bulk streams; critical actor outcomes use
  a separate ordered event stream. Held movement uses datagram ingress at 60/s
  burst 120 with stale-sequence rejection and a loss-safe lease. Server-to-client
  actor/vehicle datagram deltas, manifest streams, global weighted-fair output
  scheduling, and per-client/server bulk byte buckets remain. Sixteen-player
  soak coverage also remains. Session
  uniqueness and hard gameplay/handshake task ceilings are enforced, including
  a real iroh duplicate account rejection test.
- Graphical client key-management and administration screens, maintenance/world
  mutation, shutdown/configuration, and
  remote backup/restore remain. Every currently implemented key, report,
  moderation, connection/private-character, account, endpoint, role/status, and
  ownership request is now
  available from the macOS-compatible client as an iroh-authenticated one-shot
  command. Mute and suspension remain verified over real iroh.
- Failed character enrollment rolls back its provisional actor while burning
  the stable ID. Tick pausing plus the versioned, journal-ordered spawn record closes the known
  process-crash split between simulation and character ownership; a process-level
  SIGKILL test specifically inside each creation phase still remains.
- The Bevy client exposes a named-character convenience path, tile/actor/item
  and creature view, locomotion, inventory text, wielding/reloading, adjacent
  melee, explicit visible-target shooting, and bounded text-chat controls. Pickup,
  drop, wield, reload, and consume now derive valid choices from the latest
  authoritative snapshot, sort them by stable item ID, act immediately for one
  choice, or open a bounded nine-row keyboard picker for multiple choices. The
  same bounded interaction selects melee and ranged targets by visible stable ID,
  distance, and target kind. Open/close similarly offers every currently visible
  cardinal terrain interaction instead of silently choosing one. Pickers release
  held movement and never authorize an action client-side. Period and numpad 5
  submit the existing authoritative one-second wait action.
  Rich combat/needs UI, sleep confirmation/alarms, pockets, and
  nearly every ordinary CDDA gameplay interface remain.
- Pinned upstream content is vendored and inventoried, and MOD_INFO plus
  ammunition-type loading is implemented. ITEM common-field loading is partial:
  47 fields are implemented while 238 observed fields, subtype slots beyond the
  crafting-tool `sub` chain, reference validation, and runtime behavior remain.
  MONSTER loading is also partial: 25 fields are implemented while 87 observed
  fields and most behavior remain.
  Terrain loading is partial: 13 fields are implemented while 36 remain.
  Furniture loading is partial: 18 fields are implemented while 33 remain.
  Skill loading is partial: 8 fields are implemented while 9 remain.
  Recipe loading is partial: 33 fields are implemented while 40 remain;
  proficiency loading implements 12 fields while 4 remain and strictly selects
  234 definitions;
  requirement loading implements 7 while 12 remain. The other 171 definition types,
  replacements/EOCs, and almost all content gameplay behaviors remain
  unimplemented. Import or parsing success is not parity, and no bundled
  supplemental mod is presented as playable yet.
- Protocol 55 initially admitted only terrain bashing for two structural
  wooden-door stages. Protocol 59 admits the pinned cabin dresser and the final
  `t_door_frame` stage, resolving that frame's `t_null` result to the known
  z=0 cabin `t_floor`. Other furniture bash definitions and dynamic
  roof/new-floor resolution remain closed. Protocol 56 consumes structural sound for private
  same-z monster hearing, but it is not forwarded as a client-visible event
  until spatial hearing and disclosure rules exist. Item-group drops, collapse, explosive/tent behavior, support
  from below remain explicit follow-up boundaries. Protocol 61 now prices the
  admitted terrain and furniture targets in positive-distance routes.
- Protocol 57 admits only terrain transforms whose current source definition
  does not require `OPENCLOSE_INSIDE`. It models the content-derived
  `CAN_OPEN_DOORS` capability and upstream zero-cost open action, but not
  indoor/outdoor side checks, locks, furniture or vehicle doors, pacification
  and effect restrictions, door closing, or route-planning policy. Its
  canonical sound remains private until a spatial client-audibility DTO exists.
- Protocol 58 routes only on one z-level through already loaded passable tiles
  and admitted terrain doors. The observed trap/sharp/stair policy is canonical,
  but no trap/sharp or vertical runtime layer exists yet. Protocol 61 adds
  route-planned bashing for every admitted terrain/furniture target, while paths
  are still recomputed rather than cached with upstream backoff and nearby-
  creature danger flags do not yet affect routes. The default fresh zombie has
  `max_dist` zero; content-derived feral routing is verified without spawning a
  feral whose special attacks are still unmodeled.
- Protocol 59 established furniture-before-terrain selection with the pinned
  dresser; Protocol 60 expands that path to 537 strictly compatible pinned
  definitions. The complete upstream bash-presence set is retained separately,
  so unsupported furniture blocks rather than exposing underlying terrain. It
  still does not model item damage/on-drop behavior, signage,
  plants, fungus, tents, collapse, explosions, item damage/on-drop group
  behavior, supported-strength variants, support-from-below, or arbitrary `t_null`
  roof/floor repair. The cabin frame repair is valid only because the current
  generated structure has an explicit z=0 `t_floor` result.
- Protocol 62 exposes those registered structures to an eight-direction player
  picker and server-authoritative `Smash` command, but deliberately supports
  only clean or superficially damaged integer-bash-only wielded items.
- Protocol 63 makes base Strength, the strict pinned item profile, and exact
  ordinary weapon attack time canonical. Full generated stats and limb
  modifiers, complete damage profiles, count-by-charge weapons, unarmed anatomy,
  stamina, wear, practice, field/corpse/vehicle precedence, and faction
  ownership warnings remain explicit follow-up work.
- Protocol 64 makes all four base stats canonical. Reading consumes INT and
  disassembly practice consumes INT/PER with pinned deterministic arithmetic.
- Protocol 65 implements the pinned freeform 4-through-20 STR/DEX/INT/PER
  creator across Bevy, protocol, iroh, server validation, and the durable actor
  transaction. Point pools are intentionally absent because the pinned current
  creator uses `FREEFORM`; the remaining scenario, profession, trait, skill,
  appearance, and other generation choices are follow-up work.
- The follow-up creation audit deliberately rejected an ID-only
  scenario/profession slice. Pinned `unemployed` includes gendered worn clothing
  plus randomized nested smartphone and wallet groups, while pinned `evacuee`
  requires an evac-shelter start location. Strict implementation therefore
  depends on sex selection, worn/pocket semantics, complete item-group
  ammo/magazine/container dressing, and mapgen/start-location work; those
  dependencies must land before the choices are exposed.
- Protocol 66 applies pinned DEX/practical-melee attack speed to unarmed and
  already-admitted ordinary bash weapons, including the disconnected defensive
  path, without changing the canonical snapshot/event shapes.
- Protocol 67 promotes finalized inherited MONSTER melee skill and dodge to
  private canonical creature/corpse state with fresh-world loading, revival,
  schema-47 recovery, portable replay, and CanonicalStateV45 coverage. Public
  replication remains unchanged; Protocol 68 consumes the private dodge value
  in its first exact hit/dodge subset.
- Protocol 68 implements that first hit/dodge slice for empty-hand attacks on
  the pinned classic zombie. It adds a source-private typed miss event, exact
  action-cost behavior, named deterministic rolls for commands and disconnected
  defense, SQLite/portable-replay coverage, and CanonicalEventsV13 while leaving
  schema 47 and CanonicalStateV45 unchanged.
- Protocol 69 extends the exact hit/dodge slice to every already-admitted
  ordinary bash-only weapon. Finalized ITEM to-hit and its modern accuracy
  object become strict imported data and canonical profile state; the
  `MELEE_STAT` threshold, practical bashing/general-melee formula, connected and
  disconnected misses, schema-48 recovery, and CanonicalStateV46 hashing are
  covered without changing CanonicalEventsV13.
- Protocol 70 implements the first exact monster attack roll for the pinned
  classic zombie against a sleeping actor, whose upstream dodge is exactly
  zero. It adds private canonical `CLUMSY_ATTACKS`, a non-replicated typed miss
  outcome, the pinned one-in-four two-second self-down consequence, same-tick
  action suppression, corpse/revival retention, schema-49 recovery,
  CanonicalStateV47, CanonicalEventsV14, and portable-replay coverage. Awake
  defense remains closed until its stamina, dodge-attempt, encumbrance, effect,
  and limb inputs are canonical.
- Protocol 71 removes the temporary monster type-ID restriction: every
  canonical ordinary nonzero-melee-dice creature now rolls finalized melee
  skill against a sleeping medium actor's exact zero dodge. An arbitrary
  non-zombie low/high-skill transition and upstream's zero-dice early return
  are covered without changing schema, canonical state, or event shapes.
- Protocol 72 finalizes inherited MONSTER volume and derives all five pinned
  base-size classes. Size is private canonical live/corpse state and now feeds
  exact empty-hand/strict-bash player hit resolution for every monster type;
  all cutoffs/modifiers, CanonicalStateV48, schema-50 recovery, revival, and
  portable replay are covered without changing CanonicalEventsV14.
- Protocol 73 retains inherited static MONSTER `IMMOBILE` as private canonical
  live/corpse state, clears all available moves before ordinary monster
  interactions, and adds the exact 40-point player melee hit-spread modifier.
  Content projection, action order, CanonicalStateV49, schema-51 recovery,
  corpse revival, and portable replay are covered without changing
  CanonicalEventsV14 or public creature DTOs.
- Protocol 74 retains inherited static MONSTER `PACIFIST` as private canonical
  live/corpse state and suppresses ordinary adjacent melee without suppressing
  perception or movement. Content projection, differential AI behavior,
  CanonicalStateV50, schema-52 recovery, corpse revival, and portable replay
  are covered without changing CanonicalEventsV14 or public creature DTOs.
- Protocol 75 finalizes inherited MONSTER `attack_cost` and makes it private
  canonical live/corpse state. Ordinary adjacent monster melee spends the exact
  content-derived cost on hits and misses, preserving signed debt and banked
  low-cost attacks. Loader precedence, selected-content bounds,
  CanonicalStateV51, schema-53 recovery, corpse revival, and portable replay
  are covered without changing CanonicalEventsV14 or public creature DTOs.
- Protocol 76 replaces the runtime's single optional magazine well with a
  bounded ordered collection keyed by canonical pocket index and optional
  source ID. Exact-pocket reload, powered draw, stable nested IDs, multi-well
  disassembly, CanonicalStateV52/CanonicalEventsV15, schema-54 recovery, and
  portable replay are covered.
- Protocol 77 adds strict integral magazine pockets with stable contained ammo,
  exact whole/split/merge ID semantics, pinned reload/unload access, nested
  battery power and residual energy, recursive validation, a preloaded pinned
  starter cell, CanonicalStateV53/CanonicalEventsV16, schema-55 recovery, and
  direct/snapshot/SQLite/portable-replay conformance. General container
  contents remain the next containment boundary.
- Protocol 78 adds explicit server-authoritative removal from integral and
  detachable pockets, preserving contained IDs and fractional battery energy
  across lossless unload/reload. Access, stale-ID, active-power, and capacity
  failures are atomic; CanonicalStateV54/CanonicalEventsV17, schema-56 recovery,
  and direct/snapshot/SQLite/portable-replay conformance cover the boundary.
  Ammunition-restricted containers remain the next containment boundary.
- Protocol 79 adds that ammunition-restricted container boundary with strict
  pinned quiver projection, stable-ID partial/whole insertion, same-category
  multi-variant contents, exact removal, category switching, access-move cost,
  CanonicalStateV55/CanonicalEventsV18, schema-57 recovery, and four-mode
  conformance. General physical containers remain fail-closed.
- Protocol 80 adds strict bounded item-group content finalization and canonical
  generation graphs, named/inline structural bash sources, stable ordered RNG
  consumption, atomic output-ID preflight, the pinned wall-drop closure, and
  four-mode scenario conformance under CanonicalStateV56/schema 58. Item-group
  ammo/magazine dressing and non-bash consumers remain fail-closed.
- Protocol 81 adds strict ordinary 24x24 mapgen and default-region loading,
  bounded canonical worldgen DTOs, atomic coordinate-owned cell generation,
  generated loot planning with stable IDs, and C++ static-semantics evidence
  under CanonicalStateV57/schema 59. The current server default has no item
  placements. Before admitting an item-bearing default, server reservation
  management must guarantee the full worst-case discovery allocation instead
  of relying on its 512-ID refill threshold. SQLite/portable replay currently
  verifies already-generated snapshot chunks; journal-driven post-snapshot
  boundary discovery remains a coverage gap. Overmap layout, starts,
  nested/update mapgen, spawn groups, specials, populations, and multiple
  z-levels remain.

## Next tasks

1. Extend melee hit/dodge beyond the exact empty-hand/strict-bash-weapon versus
   all canonical monster base sizes/static immobility/pacifism and
   all-ordinary-monster sleeping-target subset
   to other weapon profiles, runtime size effects, awake monster attacks with
   canonical defense state, criticals,
   techniques, practice, stamina, wear, and limb/encumbrance effects. Complete
   player smashing with limb modifiers, full
   structural damage profiles, count-by-charge weapon timing, unarmed anatomy,
   stamina/wear/practice, and field/corpse/vehicle precedence while retaining
   strict server authority.
2. Complete the next monster/world-interaction boundary: route caching,
   pathfinding cooldown/backoff, unsupported furniture/terrain side effects,
   and hazard/vertical routing. Add spatial client audible observations
   only through a public DTO that cannot disclose hidden canonical target data.
3. Extend crafting/disassembly through charged and special tool substitutions,
   default-charged target power unloading, detachable magazines/mods, tool
   pockets, batteries, power grids/UPS,
   zero-cost and externally powered charged qualities, step-recipe speed,
   workspaces/light, non-default focus and remaining stat consumers, helpers/ebooks/recreational
   reading/book identification/full lightmaps/rust, non-crafting proficiency
   sources, multi-item batches, containers, randomized byproduct groups,
   stochastic failure, broader recipe learning, and construction while keeping
   every unsupported recipe unavailable.
4. Add graphical key-management, report-queue, connection, private-character,
   and administration screens over the implemented one-shot protocol.
5. Cache the verified retained-replay snapshot reference catalog.
5. Add server-to-client actor/vehicle datagram deltas, manifest streams, the
   global weighted-fair output scheduler, and per-client/server bulk byte buckets.
6. Add creation-phase SIGKILL fault injection and the remaining maintenance,
   world-mutation, shutdown, configuration, backup, and restore administration
   operations.
7. Extend the partial ITEM and MONSTER registries through magazines/pockets,
   complete aiming/recoil/armor/projectile semantics, comestible
   digestion/spoilage/health, remaining subtype slots, references, attacks, defenses,
   senses, effects, and death drops, then
   dependencies/replacements/EOCs without silently accepting unsupported data.
8. Add persistent overmap terrain selection and start-location boundaries on
   top of the atomic OMT planner, then add nested/update mapgen, creature/item
   spawn rules, specials, populations, and multiple z-levels without treating
   parsed unsupported fields as behavior.
9. Continue the porting matrix subsystem by subsystem to full parity.
