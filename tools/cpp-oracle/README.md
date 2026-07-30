# Pinned C++ differential oracle bootstrap

This development-only adapter executes real CDDA C++ behavior at the Rust
runtime's pinned upstream commit. `cargo xtask cpp-oracle-check` validates the
versioned scenario, verifies the checkout commit and Git tree, exports that
exact commit into ignored `target/cpp-oracle/`, adds the oracle-only Catch test,
builds a minimal `cata_test` from the upstream test main, its fake-message
support, and the adapter, then compares strict JSON output with the checked
observation. For the mapgen family it additionally loads the pinned production
OMT registry, runs the real Rust `WorldState` generator, and compares the two
normalized observations directly through one reusable exact comparator. An
exclusive cross-process file lock spans preparation and
execution, and every execution separately exports the pinned `data/` tree into
its self-cleaning runtime directory.

On macOS the upstream headless build requires Homebrew's wide-character
ncurses package (`brew install ncurses`). The runner puts that package's
`pkg-config` directory ahead of Apple's narrow system ncurses automatically.

The original `../Cataclysm-DDA` checkout is only read. C++ objects and the
exported tree remain under the Rust workspace's ignored `target/`; bounded
observation and test-user data live in one self-cleaning runtime directory. No
C++ code or library is linked into the Rust client, server, simulation, or
protocol crates.

The pocket kernel calls upstream `item_pocket::can_contain` for items shorter
than, equal to, and longer than a fixed container maximum. The item-group kernel
uses the real `Item_group`, `Single_item_creator`, and `Item_modifier` paths to
cover collection order and roll consumption, all distribution interval
boundaries, fixed and ranged count/charges, and nested groups sharing one RNG
stream. A direct fixed-count trace also proves the unconditional item-seed,
empty-variant, fit, and default zero-damage modifier draws against the exact
downstream RNG state. It also covers raw/display damage, variants, detachable
and integral ammunition dressing, randomized general-container insertion with
discard/spill overflow, the selected-content `everyday_corpse` wrappers, and
both collection and distribution event filtering outside their holiday. Exact
modifier-container traces prove ranged charges clamp to physical capacity,
default liquids fill that capacity, and a fixed post-clamp range consumes no
extra charge draw. Twelve exact charge-capacity traces derive their effective
bounds from pinned C++ `is_magazine()`, `uses_magazine()`, ammunition, and
container APIs, then retain minimum/maximum integral-tool, detachable-tool,
magazine, container, lower-sentinel, unresolved ordinary, and explicit
over-capacity roll-then-clamp outcomes with downstream RNG state. The Rust
direct comparator executes the production range resolver against those derived
bounds. Exact
default-container traces distinguish direct construction, modifier fallback,
explicit-null suppression, explicit containers whose own creator applies a
default wrapper, and the production one/twenty `bottle_otc_painkiller_1_20`
boundaries. The Rust half executes the production planner for all seven exact
traces and compares ordered child types directly. Four exact temperature
constructor traces additionally distinguish materialless `chaw`,
material-backed `water_clean`, NO_TEMP `caffeine`, and ordinary `rock`,
including birth tick, active state, cadence, phase, sentinel energy, flags, and
serialized `last_temp_check`; the reusable Rust projection compares the same
trace directly. Exact
phone-case witnesses retain the case variant, locked/unlocked phone, battery
charges, ordered E-files, sealing/capacity state, and downstream RNG draw for
empty and many-E-file boundaries. The static mapgen
kernel covers exact/type/subtype/prefix/contains OMT matching, rotatable and
linear OMT routing using each concrete identity's actual mapgen rotation,
24x24 coordinate rotation, palette piece phases, and successful generation of
a 24x24 admitted terrain/furniture template, including an exact trace of all
576 generated cells. The Rust half independently loads
the production identities and generates that template through the simulation;
the same direct comparison loads the production `sloc_lmoe` definition on both
sides and checks its chosen target, constraints, runtime-admission state, and a
fixed matching candidate set. Multiplayer occupied-tile fallback remains a
Rust adaptation covered by shared conformance. The command fails if either side
differs from the checked corpus or from the other implementation. Run them with:

```sh
cargo xtask cpp-oracle-check docs/oracles/item-group-generation-v1.json
cargo xtask cpp-oracle-check docs/oracles/mapgen-static-semantics-v1.json
```

Seed values used to reach range boundaries are retained in the checked
observation beside normalized semantic results, ordered representative traces,
and downstream RNG state. The adapter finds each boundary within a fixed search
bound, so an implementation cannot satisfy the corpus by matching only
aggregate minima or maxima.
The static mapgen kernel contains no random choices or seed-dependent expected
values. It intentionally does not call full overmap generation, whose standard
library distributions and shuffles are not portable semantic fixtures between
macOS and Linux.

This remains a bounded bootstrap, not a general C++ RPC service: adding a kernel
requires a new versioned scenario/observation shape and explicit C++ adapter
code. The first build compiles upstream's headless core and can take several
minutes; subsequent runs reuse it only when a strict cache record matches the
commit, tree, adapters, and BLAKE3 executable digest. Any mismatch deletes the
export and rebuilds it from the pinned Git archive. Runtime-loaded JSON is never
reused from that build cache.
