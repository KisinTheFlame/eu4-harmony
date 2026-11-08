# 2026-11-08 Letting go after a return in one module (tested by me)

What waits to be let go until a return or a world reset has passed, gathered from four repairs into `core/src/repairs/waiting.rs`, and the moments of the sessions each repair acts on declared in its `Repair` ([deepening.md](../plans/deepening.md), D2), on the deepening branch with master's one-file-per-run log merged in. `cargo xtask scenario growth leak --growth.rounds 3 --leak.rounds 3` in a test game of my own, started hidden, 9 minutes in all.

- **growth** (memory account build), three rounds of a load, an in-game load, a return, a new game as France and a return: passed. The game's heap held 1,753.9, 1,754.4 and 1,754.5 MB after each round, +0.19 MB a round over rounds 2 to 3.
- **leak** (heap census build), three rounds: passed. The game's heap held 1,752.6, 1,756.1 and 1,756.2 MB.
- **What the repairs let go**, from the leak run's `core.log` at each world reset, the same each round and as in [The estates' modifiers and the message icon holders](2026-11-08-estates-modifiers-icon-holders.md) and [The relations and the trade nodes let go](2026-11-07-relations-trade-nodes-let-go.md):
  - the load's reset: 2,838 persons and 419 battles of the histories;
  - the in-game load's world clear: 911 relations, 160 trade nodes (two databases), 496 to 502 estates' modifiers and 144 entries of the lists;
  - the return's: 917 relations, 80 trade nodes and 60 entries; the new game's first frame, 2,280 to 2,291 persons and 347 battles.
  No "was not one; kept" and no "was not deleted" line, and no fatal error.
- **Leftovers** at the last generation: the trade nodes' sites (`0xb6654f`, `0xb6abfa`, 6 KB a generation) only "sometimes", one generation of them waiting for its return as before; the estates' modifiers' and the lists' entries' sites were not among the leftovers listed.
- **The release**: both games ended by the run (exit code 1), the game did not crash, and the test game's logs were kept in the claim's `kept\`.
- **Offline**: `cargo xtask offline-check` passed, and `cargo test -p eu4-harmony-core --features dev -- --ignored` with the developer build's points.
- **Not verified**: the quarantine with the new module (a leak run with `--leak.quarantine true`); a game deleting a waiting object before its repair does, which only the tests of withdrawing cover; more than three rounds.
