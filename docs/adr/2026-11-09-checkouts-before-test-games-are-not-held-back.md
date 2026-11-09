# Checkouts from before test games are not held back

A claim of a test game no longer looks at `game-lock.json`, the record of the claim of the whole game, and no longer writes into it: it waits for no checkout from before test games, and makes none of them wait. `claim --wait`, `scenario --wait` and the exit code 2 go with it, and so do the lease of a hundred years each claim was written with, and the messages for `watch`, `renew`, `--lease`, `--player` and `--now`. This supersedes the part of ADR test-games-run-side-by-side that kept such a checkout's claim of the whole game beside the test games, and the part of ADR test-games-have-no-limit-or-lease that let a claim wait for one and wrote each claim with a lease.

Every checkout on this computer that tries things in the game runs from master, which has had test games since 2026-11-08, so the waiting was never used, and what kept it working was a second record, a holder written for older code and a flag that took a command's exit code, read by nobody.

## Considered Options

- **Keep the marker in `game-lock.json`, drop the waiting**: a checkout from before test games still waits while test games are held; but it is a record of its own kept up for code that is no longer run.

## Consequences

- A checkout from before test games, if one were run, would claim the whole game beside the test games: install into the game folder, back up the player's files and put them back. Such a checkout is updated from master before it tries anything in the game.
- The mutex keeps its name, `eu4-harmony-dev-game-lock`, so that a checkout a little behind master and one on it still guard `claims.json` with the same one.
- A `game-lock.json` left in `%LOCALAPPDATA%\eu4-harmony-dev\` is no longer read; it is removed by hand once no checkout of the older code holds test games.
