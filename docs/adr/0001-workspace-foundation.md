# ADR 0001: Workspace foundation

Status: Accepted

The initial implementation uses the crate boundaries fixed in
`ARCHITECTURE_DECISIONS.md`. `cdda-protocol` owns stable wire/domain DTOs;
`cdda-sim` owns canonical plain-Rust state; `cdda-persistence` owns SQLite;
`cdda-server` owns runtime orchestration and iroh; and `cdda-client` alone owns
Bevy presentation.

`cdda-net` is the narrow non-authoritative exception added during the first
network slice. It owns only secure iroh key-file handling and bounded reliable
framing shared by client and server. It cannot authorize, persist, or mutate
gameplay. This avoids either duplicating security-sensitive framing or making
the client depend on server/persistence internals.

The first implementation slice begins with deterministic movement, collision,
combat, stable ID reservation, canonical hashing, and SQLite round trips. These
are complete behaviors needed by the networked slice rather than framework-only
scaffolding. All network and persistence APIs remain project-owned boundaries.
