# Deferred checks

Run only when the user explicitly requests a verification checkpoint.

- Road/mapgen checkpoint: complete the production-field real-Iroh acceptance
  test (interrupted on 2026-07-30), four-mode recovery/replay, mapgen C++
  oracle, content gates, broad Clippy/workspace tests, rustdoc, and platform CI.
- River/bridge checkpoint: run pinned topology and built-in mapgen C++
  differential traces, production-content admission, recovery/replay modes,
  and the real-Iroh two-client exploration path.
- Overmap-special checkpoint: run fixed-special loader/admission coverage,
  exact C++ sector/rotation/city/uniqueness traces, multi-z atomic placement,
  recovery/replay modes, and real-Iroh two-client landmark exploration.
- Monster-spawning checkpoint: run exact C++ group/mapgen density and pack
  traces, finalized armor inheritance/resistance traces, production-content
  admission, canonical hash fixtures, four-mode
  recovery/replay, and real-Iroh two-client population persistence. The broad
  `cdda-server --all-targets` check hit the mandatory 60-second timeout; bounded
  library, binary, and test-target checks are used during implementation.
- Anatomy/combat checkpoint: run exact C++ body-part HP, hit-selection, armor
  layer, on-hit-effect, stamina regeneration/burn, dodge threshold/attempt, and
  actor/monster hit traces, including ordered multi-type monster damage,
  same-type dice merging, penetration, armor/damage multipliers and inheritance
  modifiers, plus attack-effect chance/range/body-part/permanence and repeated
  application traces, sleeping natural-healing remainder rolls and
  medical selection, scaling, wound-treatment, and charge-consumption traces;
  canonical fixture updates; recovery/replay modes; production admission; and
  two-client melee/ranged/monster combat.
- Monster-special checkpoint: run exact C++ initial/reset cooldown, ordered
  scheduling, generic melee multiplier/range/effect, bite-wound, leap candidate
  selection, ammo-free gun range/fake-skill/dispersion/damage/sound traces,
  blindness/environmental-resistance boundaries, canonical persistence/replay,
  production acid-spit admission, and
  ammo-free and ammo-fed single-shot actor traces, authoritative ammunition
  depletion across persistence/replay, production admission, and two-client
  targeting/laser-lock acquisition and signed timeout-extension traces;
  data-driven ammunition-effect inheritance, trail ordering, endpoint field
  bursts on hit and miss, chance/intensity/passability boundaries, and
  body-part on-hit duration/intensity traces; inherited firing-mode selection,
  overlapping range precedence, bounded burst ammunition consumption,
  per-projectile fields/damage/sound, stun blocking, intra-burst recoil and
  cycling boundaries; polymorph prototype dependency closure, proportional HP,
  exact keep-speed/HP/aggression overrides, stable creature identity, target
  special-cooldown reset, authoritative transform events, beta/target effect and
  variable conditions/mutations, no-target root rejection, target-context
  reference closure, permanent hostile summon SPELL inheritance/stat scaling,
  self/hostile targeting, bounded deterministic placement, transitive summoned
  prototype closure, stable IDs and authoritative summon events; hostile typed
  damage scaling, armor/body-part resolution, multi-survivor blast ordering,
  interruption, wake and death handling; deterministic whole-body status spell
  durations, stacking and expiry including production wraith darkness;
  supported creature-alpha/survivor-beta EOC spell closure and multi-target
  activation; and combat scenarios at protocol 126/schema 104,
  CanonicalStateV102,
  canonical-event-V26.
- EOC/use-action checkpoint: run pinned condition/effect and `run_eocs`
  differentials, actor-variable comparison/set/remove and deterministic
  possible-value selection traces, strict
  transform/use-action and delayed-activation traces, production catalog
  admission, CanonicalStateV86 queue persistence/replay, hostile recursive,
  variable-bound, queue-bound, overdue catch-up, failed-execution, recurring
  enrollment/range/reschedule/deactivation/reactivation, and disconnected cases,
  actor inventory/count-by-charges/worn/wielded/progression/stat predicates,
  effect-set/intensity predicates, bounded integer actor-variable math parsing,
  assignments, missing/invalid variable reads, safe-integer overflow, expression
  depth/node limits, comparisons, boolean short-circuiting, and base-stat reads,
  bounded event-EOC dispatch for movement/OMT entry, damage, death, kills, and
  creature damage, including deterministic ordering and activation caps,
  persistent server-driven EOC confirmation, default handling for noninteractive
  actors, tail-continuation rejection, nested confirmation, cancellation,
  authoritative base-stat/stamina/thirst/sleepiness math reads and bounded
  assignments across direct, scheduled, event, and resumed-confirmation paths,
  monster-alpha effect/variable persistence, effect-intensity conditions,
  immediate EOC chains, melee EOC hooks, dedicated EOC special actors,
  creature effect expiry and unsafe target-context/delayed graph rejection,
  two-client item activation, target/beta effect and variable reads and writes,
  target-context reference closure, and generic medical-choice request, reconnect,
  expiry, stale-response, protocol 126/schema 104, CanonicalStateV102,
  canonical-event-V26, and
  recovery coverage.
- Environment field-contact checkpoint: run pinned limb-category, acid damage,
  armor absorption, corrosion duration/intensity, intensity-level inherited
  non-environmental effects, reversed duration bounds, outside-vehicle chance,
  consume-on-contact, stationary/disconnected exposure, unsupported
  environmental/immunity fail-closed admission, death/activity-interruption,
  CanonicalStateV102, canonical-event-V26, SQLite, portable replay, and
  client-visible actor state/event coverage for protocol 126/schema 104.
- Content test-target cleanup: `cargo check -p cdda-content --tests` currently
  fails in untouched `material.rs` because one test-only
  `MaterialThermalDefinition` literal lacks `damage_resistance_milli`; the
  ordinary `cdda-content` library check is green.
