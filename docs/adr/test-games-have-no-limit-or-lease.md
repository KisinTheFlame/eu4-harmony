# Test games have no limit and no lease

A claim holds its test game until its holder releases it, and any number of test games are held at once: no queue, no lease, nothing taken from a holder. A claim waits only while a checkout from before test games holds the whole game. This supersedes the limit of three, the queue and the lease of ADR test-games-run-side-by-side.

The limit kept a session waiting while the computer had room, and the lease let a claim end another session's game while that session waited on its user, so that it came back to a test game gone. The sessions on the computer are few, and each releases what it holds.

## Considered Options

- **The limit and the lease, as before**: no more games than the computer holds in the late game; but a session waits on a number, and an idle holder loses its game to whoever comes.
- **A higher limit, without the lease**: still a number to wait on, and a holder that never releases keeps one of them for good.

## Consequences

- Nothing bounds what the test games take of the memory and the graphics card: a game in 1820 took 4.2 GB and 1.3 GB ([Test games](../prototype-log/2026-11-08-test-games.md)). Whoever starts one sees in `cargo xtask game status` how many are held.
- A claim not released stays until its holder, or whoever cleans up, runs `cargo xtask game release --owner <name>`; `status` lists it with its holder and purpose.
- `claim --lease` and the queue are gone; `claims.json` keeps no queue, and older records read as they are. Each claim is still written with a lease, of a hundred years from 1970, since a checkout from before this decision drops a claim with none from the record and takes one whose lease ran out.
