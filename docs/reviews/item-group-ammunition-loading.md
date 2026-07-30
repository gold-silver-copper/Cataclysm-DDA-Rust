# Item-group ammunition-loading review

## Frozen scope

- Parent and merge base:
  `72b27a2b54f27efe37eb0c07b3cb1cefc35ba3a9`.
- First reviewed implementation:
  `60c048fbacf3066476b3057e11a8a9bcd1513b71`, tree
  `3fdc7055396922fc27cee1b5556c553027dec97a`.
- Reviewed replacement implementation:
  `d95790301fbe89b7eff291681d0135ab8d853480`, tree
  `7b19b5fecaa9b32879c3b8fa746c653920176866`, stable patch ID
  `6e08a20c3277514ad61dd2cea6682c7cebf2a2cc`.
- Scope: the complete 15-file replacement diff, 725 insertions and 63
  deletions, covering item-group protocol terminology, server normalization,
  simulation construction, shared conformance, the ordinary client item menu,
  pinned C++ characterization and Rust comparison, ledgers, and current
  architecture documentation.
- Representation: protocol 90, persistence schema/minimum recoverable schema
  68, CanonicalStateV66, worldgen algorithm 2, replay format 3, and
  CanonicalEventsV18 remain unchanged.

## Pre-freeze review

The explicit complete-diff pass before the first commit found one evidence
weakness: a focused next-boundary assertion had replaced the deterministic scan
of every retained `field` group. The full scan was restored. It proves that
`accessory_weaponcarry` and `ammo_light_batteries` no longer fail and that
`bottle_otc_painkiller_1_20` is the first remaining error.

## First independent fixed-tree review

An independent reviewer inspected exact commit `60c048f` from clean detached
worktree `/private/tmp/cdda-60c048f-review.L1wRcm`. The tree remained fixed and
clean, and the reviewer made no edits. The complete diff produced two confirmed
findings:

1. **P1: integral-gun state and RNG divergence.** Pinned `Item_modifier`
   retains charges owner-local on an integral gun in this path and constructs
   no nested ammunition. Rust would have inserted a nested ammunition item,
   changing canonical ownership, persistence/client shape, stable-ID demand,
   and constructor RNG. Real pinned content includes `bbgun` with a `0..150`
   charge range.
2. **P2: detachable-gun overclaim and dead branch.** Current craft projection
   already excludes every gun magazine well, so the claimed one-compatible-
   magazine gun support was unreachable. If later made reachable, the reused
   tool planner would still be wrong: pinned zero charges leave no magazine,
   while positive charges route through distinct `item::ammo_set` magazine
   selection and constructor RNG.

No other P0-P3 correctness, security, recovery, client, comparator, versioning,
or module-budget finding was confirmed.

## Resolutions and final review

The replacement fails closed for every `GUN` charge owner before integral or
detachable storage selection. A subtype-level regression keeps that invariant
independent of future pocket projection. The pinned production test exercises
real `bbgun` and requires the exact fail-closed diagnostic while continuing to
admit detachable `wearable_light` and integral `ammo_light_batteries`. The dead
gun branch and all gun-support claims were removed. Gun parity remains a later
coherent family requiring subtype-specific state, a dedicated planner, and
direct C++ traces.

A fresh independent reviewer inspected replacement `d957903`, tree `7b19b5f`,
against the same parent from clean detached worktree
`/tmp/cdda-d957903-review.SJELIU`. The complete 15-file tree remained fixed and
clean. The reviewer confirmed both resolutions, ran the focused gun, production
content, and simulation tool-charge tests, and found no remaining P0, P1, P2,
or P3 issue.

## Characterization and representation audit

- The pinned item-group oracle has 85 exact assertions. Five direct traces fix
  zero, one, exact capacity, and overflow for the 16-charge light battery plus
  overflow for the two-charge ultra-light battery. Four production witnesses
  fix empty, partial, full, and alternate `ammo_light_batteries` results. Every
  trace includes item/ammunition type, charge count, remaining capacity, and
  downstream RNG.
- The reusable Rust comparator invokes the production constructor and charge
  planner. The shared named-item-group scenario preserves the generated
  magazine, nested battery, clamped 16-charge count, and preorder stable IDs
  through direct, per-tick snapshot, SQLite, and portable replay execution.
  The normal Bevy item menu renders `p0 16/16 battery` from replicated state.
- No field, variant, enum, or ordering changed in Postcard, persistence, or the
  event trace. The descriptor was already owner-independent in representation;
  this increment only admits additional magazine semantics. Therefore the
  Protocol-90 representative V66 root remains
  `7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`,
  its bytes under the old V65 domain remain
  `24e4298046769183c36ee47334b1acc628956a92fa2176dfff4deac32fbef2db`,
  and CanonicalEventsV18 remains unchanged. No version bump is warranted.

## Module-growth audit

The replacement owns 4,183 lines in `sim/items.rs`, 1,643 in
`protocol/item_groups.rs`, and 1,315 in `server/item_groups.rs`. Central
`sim/lib.rs` grows by three lines solely to export the renderer-free comparator;
central protocol, persistence, and server libraries do not grow. The 53-line
server executable increase is production normalization/regression evidence, and
the 257-line tools increase is exact oracle parsing, validation, and direct Rust
comparison. Future item behavior remains assigned to the extracted modules.

## Verification

The implementation/review cycle passed:

- `cargo fmt --all -- --check` and `git diff --check`;
- `cargo check --workspace --all-targets --all-features`;
- strict full-workspace Clippy on the reviewed replacement;
- `cargo test --workspace --all-features` on the replacement -- 383 tests plus
  doc-tests;
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`;
- dependency-boundary, 30-milestone parity-ledger, runtime-progress,
  astronomy-table, content-validation, content-inventory, JSON, and diff gates;
- selected content -- 7,992 files, manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
- content inventory -- 6,571 JSON files, 93,779 top-level objects, and 180
  definition types;
- pinned C++ pocket, item-group, and mapgen oracles -- 8, 85, and 1,179 exact
  assertions respectively.

## Residual boundary

This family earns no runtime-progress credit because the real field is not yet
generated or playable. The complete deterministic field scan now fails first
at `bottle_otc_painkiller_1_20`, where `aspirin` requires generalized default-
container ownership. Gun charge modifiers also remain explicitly unavailable,
but they do not precede the field's next dependency. The next playable unlock
remains real-field generation plus ordinary client exploration and loot.
