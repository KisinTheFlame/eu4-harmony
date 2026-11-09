# At most four test games run at once

At most four test games run at once on this computer, counting those being started. A start while four others run waits, saying which, until one of them ends, and then starts; nothing is taken from a test game that runs. Any number of claims are still held, and a claim is still held until it is released: the limit is on games that run, not on claims. This supersedes the part of ADR test-games-have-no-limit-or-lease that let any number of test games run at once.

Sessions playing scenarios several at a time, each in a test game of its own, together start more games than the computer holds: a game in 1820 takes 4.2 GB of memory and 1.3 GB of the graphics card's ([Test games](../prototype-log/2026-11-08-test-games.md)). Each scenario played at once is a test game of its own, so each takes one of the four.

## Considered Options

- **A limit on claims, as the limit of three was**: a claim held with no game running, between two runs or while its holder waits on its user, would keep a place no game uses, and a holder that never releases would keep it for good.
- **Counting every `eu4.exe`, the player's too**: closer to what the computer holds, but a session would wait on the player's game and on an older checkout's, which no command ends; the player's game is no claim's.
- **Refusing a start instead of waiting**: a run of four scenarios beside another session's game would fail its fourth instead of playing it a little later.

## Consequences

- A start takes its place in the record under the claims' mutex, as the process starting it, before it starts the game; the place is given up when the start ends, and a place whose process has ended counts no more. A checkout from before this decision writes the record without the places being taken.
- `game start` waits as long as it takes; a scenario run interrupted while one of its starts waits ends that scenario as interrupted. `--jobs` above four plays the rest as places come free.
- The player's game, and an `eu4.exe` of no claim, are not counted.
- A test game that restarts itself, as a return to the main menu does, holds no place between its old process ending and the new one serving its pipe, so another start can take it then and five run for a while.
