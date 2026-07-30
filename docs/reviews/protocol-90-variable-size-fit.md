# Protocol 90 variable-size FIT review

## Frozen replacement implementation

- Baseline parent and merge base:
  `e851628726521b9aa1a7ae90d7a4ca67e4cb4473`.
- Reviewed replacement implementation:
  `d115312105b6c340884e73ffdf6b9d12c541a4c4` (`Implement canonical
  variable-size item fit`).
- Tree: `14405a785f10ecc7ee0ec2bbf9006c68b0de91d6`.
- Stable patch ID: `c33f61d0cec199151d1bcbe1a9d316a76b4d0cc8`.
- Scope: the complete 18-file diff, 877 insertions and 207 deletions.
- Representation: protocol 90, schema/minimum recoverable schema 68,
  CanonicalStateV66, worldgen algorithm 2, replay format 3, and
  CanonicalEventsV18.

The reviewed family covers immutable `VARSIZE` capability, canonical
per-instance `FIT`, direct item-group construction and RNG order, crafting and
disassembly propagation, component provenance, validation, stack identity,
persistence and four-mode conformance, client presentation, exact C++ traces,
production field-closure admission, ledgers, and the item materializer
extraction.

## Pre-freeze review

An explicit full-diff pass before the first implementation freeze found one P1:
simulation and client stack identity, plus simulation craft split-parent
identity, did not yet compare the new `fitted` bit. That could merge or select
state-distinct instances. The implementation added FIT to all three predicates
and added simulation/client regression assertions before the first commit.

## First independent fixed-tree review

An independent reviewer inspected exact commit
`8a444f64913f62df411b23276d1caf40b675d24f`, tree
`c770a4bae224c93005bac5f0a7e87f8ae7ff3052`, stable patch ID
`25696456f67f4466218a9b34aea6e2bdda5b1533`, from clean detached worktree
`/tmp/cdda-8a444f-review.OPx0R9`. The reviewer made no retained edits and found
no P0/P1 issue, exactly two P2 issues, and no other confirmed P0-P3 finding:

1. **P2: planned charge-stack containment omitted FIT identity.**
   `insert_planned_item` combines matching count-by-charge contents, removes
   the old stacks, and retains the incoming instance. Its private planned-state
   predicate omitted `fitted`, so a fitted and unfitted variable-size pair
   could collapse and lose one canonical state. The pinned finalized
   recommended registry has zero `count_by_charges && VARSIZE` definitions,
   but the admitted custom/restored domain permits the shape.
2. **P2: immutable FIT could validate as false.** Snapshot and component
   validation enforced only `fitted => FIT || VARSIZE`; it accepted
   `fitted=false` when the immutable flags already contained `FIT`. That state
   cannot exist under upstream effective flag semantics and `[FIT,VARSIZE]`
   would render it as poor fit. The pinned finalized recommended registry has
   zero static-FIT definitions, but valid custom/restored data could reach the
   inconsistency.

## Resolutions in the replacement

- Planned count-by-charge containment comparison now includes `fitted`. An
  end-to-end regression inserts fitted and unfitted variable-size charge
  payloads into one rigid pocket and proves they remain two exact stacks.
- Protocol and simulation snapshot/component validation now require immutable
  `FIT` items to be fitted while continuing to permit both states for
  `VARSIZE`. Prototype validation synthesizes the immutable fitted state, and
  protocol, simulation, component, and constructor regressions cover both the
  rejected and admitted boundaries.
- These repairs change accepted state and identity behavior, not the serialized
  or wire shape. They therefore remain within protocol 90/schema 68 rather than
  creating another representation checkpoint.

## Replacement-tree review

A fresh independent reviewer inspected exact replacement `d115312`, tree
`14405a7`, and stable patch ID `c33f61d0` against parent `e851628` from clean
detached worktree `/private/tmp/codex-d115312-review.F8Z8tw`. The complete
18-file diff was fixed throughout the review, and the reviewer made no edits.

The reviewer confirmed both P2 repairs through their production predicates and
regressions, then found no remaining P0, P1, P2, or P3 issue. Focused
verification passed the two workspace FIT tests, all 38 protocol tests, planned
charge-stack separation, craft/byproduct FIT persistence, disassembly FIT
propagation, simulation snapshot/component validation, the client item menu,
and four-mode named-item-group conformance. The reviewer did not redundantly
complete the entire workspace, strict Clippy, rustdoc, repository xtasks, or
external C++ run; the aggregate implementation verification below supplies
those broader gates, including the replacement commit's 80-assertion C++
item-group oracle and direct Rust comparison.

## Characterization and representation audit

- The pinned item-group oracle has 80 exact assertions. It retains a
  same-draw non-variable control, direct fitted and unfitted `leg_sheath6`
  witnesses, production fitted and unfitted `accessory_weaponcarry` witnesses,
  their rendered names, and every downstream RNG draw. The runner invokes the
  reusable Rust FIT transition directly.
- Every direct leaf consumes one FIT draw. Only `VARSIZE` items gain FIT from
  that draw, immutable FIT is idempotent, named-group indirection adds no phase,
  and raw wrappers do not double-construct their leaf.
- Normal primary craft outputs and byproducts force-fit `VARSIZE`. Default
  disassembly components inherit FIT from a fitted target only when variable
  size; retained exact component state keeps its own bit.
- Direct, per-tick snapshot, SQLite, and portable-replay conformance preserve
  the state. The ordinary Bevy item menu renders `(poor fit)` from the
  replicated authoritative snapshot.
- The Protocol-89 V65 representative root was
  `0878f47b5e8e159fdee5a57a6c7f90bab5e13e6bb944820a10585b835fb857be`.
  Protocol-90 Postcard bytes hashed under the old V65 domain yield
  `24e4298046769183c36ee47334b1acc628956a92fa2176dfff4deac32fbef2db`,
  proving a representation change. CanonicalStateV66 yields
  `7fffb3bccad59a52e64540aeb421cde5f1fd8912e3a11946368170b2eeec91cb`.
  CanonicalEventsV18 and its event trace are unchanged.

## Module-growth audit

The replacement owns 4,132 lines in `sim/items.rs`, 1,641 in
`protocol/item_groups.rs`, and 1,286 in `server/item_groups.rs`. Central
`sim/lib.rs` is 29,232 lines, 71 fewer than the parent, because both item
materializers moved completely into the item module. The 39-line net growth in
`protocol/lib.rs` is limited to the canonical field/version, prototype
integration, and focused representation/validation tests. Server central
growth is four mechanical fitted-field initializers; its item-group module
removes the obsolete VARSIZE guard. Future item behavior remains assigned to
the extracted item modules.

## Verification

The implementation and review cycle passed:

- `cargo fmt --all -- --check` and `git diff --check`;
- `cargo check --workspace --all-targets --all-features`;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo test --workspace --all-targets --all-features` -- 382 tests after the
  first review fix; after the final immutable-FIT repair, all 38 protocol tests,
  all 156 simulation tests, and the pinned production-content server test were
  rerun;
- `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`;
- dependency-boundary, 30-milestone parity-ledger, runtime-progress,
  astronomy-table, content-validation, and content-inventory gates;
- selected-content validation -- 7,992 files, manifest hash
  `45d913ee0d0dbd3ef353668e9fb7c4839033227ea3de1ed6650333ffd560ca82`;
- content inventory -- 6,571 JSON files, 93,779 top-level objects, and 180
  definition types;
- pinned C++ pocket, item-group, and mapgen oracles -- 8, 80, and 1,179 exact
  assertions respectively, including the direct Rust comparisons.

## Residual boundary

The repairs close the admitted generic FIT-state invariants without adding
runtime-progress credit. The complete deterministic field scan now fails
closed first at `ammo_light_batteries`, where `light_battery_cell` needs the
generalized ammunition-loading constructor family. Default containers,
temperature, corpse construction, capacity sentinels, wrapper shapes,
dressing, and remaining snippet shapes are later retained boundaries. The next
playable unlock remains complete real-field generation plus ordinary client
exploration and loot; cities, roads, rivers, specials, anatomy, and EOCs do not
precede it.
