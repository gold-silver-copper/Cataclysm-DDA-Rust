# Pinned C++ differential oracle bootstrap

This development-only adapter executes real CDDA C++ behavior at the Rust
runtime's pinned upstream commit. `cargo xtask cpp-oracle-check` validates the
versioned scenario, verifies the checkout commit and Git tree, exports that
exact commit into ignored `target/cpp-oracle/`, adds the oracle-only Catch test,
builds a minimal `cata_test` from the upstream test main, its fake-message
support, and the adapter, then compares strict JSON output with the checked
observation. An exclusive cross-process file lock spans preparation and
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
stream. The static mapgen kernel covers exact/type/subtype/prefix/contains OMT
matching, rotatable and linear OMT routing, 24x24 coordinate rotation, palette
piece phases, and successful setup of a one-cell nested JSON mapgen. Run them
with:

```sh
cargo xtask cpp-oracle-check docs/oracles/item-group-generation-v1.json
cargo xtask cpp-oracle-check docs/oracles/mapgen-static-semantics-v1.json
```

Seed values used to reach range boundaries are deliberately omitted from the
observation. The adapter finds them within a fixed search bound, then emits only
normalized semantic results, ordered traces, and downstream RNG-state equality.
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
