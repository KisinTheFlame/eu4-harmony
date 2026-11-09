---
status: superseded in part by test-games-have-no-limit-or-lease and checkouts-before-test-games-are-not-held-back
---

# Test games run side by side

The limit of three, the queue and the lease are superseded by ADR test-games-have-no-limit-or-lease: a claim holds its test game until it is released, and any number are held at once. The claim of the whole game a checkout from before test games keeps beside them is superseded by ADR checkouts-before-test-games-are-not-held-back: neither waits for the other.

Every session on the computer that tries something in the game does it in a test game of its own: a game started from the game folder with a user folder (`-userdir`), a core (`-harmony_core`, which the proxy loads) and, in a developer build, a remote control pipe (`-harmony_test_game`) of its own. Up to three run at once, each held by one claim; the others wait in a queue. A claim backs up and puts back nothing, since a test game touches neither the player's files nor the framework installed in the game folder. A player's build serves the one fixed pipe, `\\.\pipe\eu4-harmony`, whatever its command line says.

One claim of the whole game made every session wait while another played a scenario of twenty minutes, though the computer has room for several games (research/several-games.md), and each claim's backup and put back of the player's files was where a run could leave the player's game changed.

## Considered Options

- **One claim of the whole game, as before**: simple, and every run measures alone; but the queue grows with the sessions, and every run changes the player's files and puts them back.
- **A copy of the game folder per test game**: a framework of its own without switches; the game started outside the Steam game folder loses DLCs (research/several-games.md), so it would not be the player's game.
- **The test game's pipe in every build**: a player's build too would serve another pipe when its command line asks; the player's game then has two ways to be reached, and players read about a switch that is for development. In a developer build only, the scenarios' player build is the player's build with that one feature, which changes the pipe's name and the startup report.
- **A claim of the whole computer beside the test games**, for installing into the game folder and measuring alone: left out for now; a measurement says how many other games ran beside it.

## Consequences

- The game folder's proxy must know `-harmony_core`: until the player's installed framework is from this decision on, a claim is refused and says to install it.
- What a run measures, it measures beside up to two other games, which take 4 to 8% of its speed (research/several-games.md); the perf scenario's report says how many ran.
- An idle holder keeps its test game until its lease runs out and a claim waits; nothing wakes it before.
- A checkout from before this decision keeps its claim of the whole game; while it holds the game the test games wait, and while test games are held it waits.
