# Only the claim's commands know the claim

A claim is made, renewed, kept going and released only by the claim's commands of `cargo xtask game` (`claim`, `renew`, `watch`, `release`, `status`), which the holder runs itself ([game-testing.md](../game-testing.md)). Every other command knows nothing of the claim: `install`, `game start`, `game stop`, `scenario` and `profile` neither look whether the game is claimed nor renew the lease nor write the record, and the claim's module gives the rest of xtask no function at all. That the game is claimed while they run is the holder's to see to, as the game-test skill says, not theirs to check.

## Considered Options

- **The other commands refuse without the claim and the runner claims, renews and releases itself, as they did**: a second way into the claim beside the commands, which a session could take without its `watch`, its queue's `--wait` in the background or its own release, and a check by the owner's name only, which any command passes with `--owner`, so it guarded the convention less than it seemed to.
- **The other commands refuse without the claim, and only the claim's commands change it**: one way into the claim, but each command still reads the claim's record and owner, so the claim's rules are spread over every command that touches the game.
- **The runner runs `cargo xtask game claim` and `release` as programs of their own**: the commands alone change the claim, but the runner still decides for the holder when it is made and released.

## Consequences

- A session claims first, runs `watch` in the background, then installs, starts, plays scenarios and profiles, and releases after; nothing stops a command run without the claim, which then may end or change a game the player or another session has.
- `game start` no longer starts the lease again, and does not name the game it started in the record: the holder's `watch` takes the one `eu4.exe` running for the claim's game. A claim's lease is kept going only by its `watch` and `renew`.
- The scenarios' runner does not notice the player taking the game: the player's claim ends the game, which the scenario playing counts as a crash. The holder's `watch` exits with 4, and the holder ends the run.
- `install` and `scenario` take no `--owner`, and `scenario` no `--wait`: the owner and the queue are the claim's commands'.
