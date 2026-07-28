# Pinned C++ differential oracle bootstrap

This development-only adapter executes real CDDA C++ behavior at the Rust
runtime's pinned upstream commit. `cargo xtask cpp-oracle-check` validates the
versioned scenario, verifies the checkout commit and Git tree, exports that
exact commit into ignored `target/cpp-oracle/`, adds the oracle-only Catch test,
builds a minimal `cata_test` from the upstream test main, its fake-message
support, and the adapter, then compares strict JSON output with the checked
observation.

On macOS the upstream headless build requires Homebrew's wide-character
ncurses package (`brew install ncurses`). The runner puts that package's
`pkg-config` directory ahead of Apple's narrow system ncurses automatically.

The original `../Cataclysm-DDA` checkout is only read. C++ objects, the exported
tree, test user data, and observations remain under the Rust workspace's
ignored `target/`. No C++ code or library is linked into the Rust client,
server, simulation, or protocol crates.

The initial kernel calls upstream `item_pocket::can_contain` for items shorter
than, equal to, and longer than a fixed container maximum. This is a bounded
bootstrap, not a general C++ RPC service: adding a kernel requires a new
versioned scenario/observation shape and explicit C++ adapter code. The first
build compiles upstream's headless core and can take several minutes; subsequent
runs reuse the commit-and-adapter-keyed binary.
