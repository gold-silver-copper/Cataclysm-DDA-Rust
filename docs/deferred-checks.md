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
  selection, canonical persistence/replay, production admission, and two-client
  combat scenarios.
- EOC/use-action checkpoint: run pinned condition/effect and `run_eocs`
  differentials, actor-variable comparison/set/remove and deterministic
  possible-value selection traces, strict
  transform/use-action and delayed-activation traces, production catalog
  admission, CanonicalStateV86 queue persistence/replay, hostile recursive,
  variable-bound, queue-bound, overdue catch-up, failed-execution, recurring
  enrollment/range/reschedule/deactivation/reactivation, and disconnected cases,
  actor inventory/count-by-charges/worn/wielded/progression/stat predicates,
  effect-set/intensity predicates,
  two-client item activation, and generic medical-choice request, reconnect,
  expiry, stale-response, protocol 112/schema 90, CanonicalStateV88,
  canonical-event-V19, and
  recovery coverage.
- Environment field-contact checkpoint: run pinned limb-category, acid damage,
  armor absorption, corrosion duration/intensity, stationary/disconnected
  exposure, death/activity-interruption, CanonicalStateV84, SQLite, and portable
  replay coverage for protocol 108/schema 86.
- Content test-target cleanup: `cargo check -p cdda-content --tests` currently
  fails in untouched `material.rs` because one test-only
  `MaterialThermalDefinition` literal lacks `damage_resistance_milli`; the
  ordinary `cdda-content` library check is green.
