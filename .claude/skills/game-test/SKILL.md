---
name: game-test
description: Claim a test game of your own before trying anything in the game, start it hidden, ask it on its own pipe, and release it after, which keeps its logs and removes the rest. Use before starting eu4.exe, playing scenarios (cargo xtask scenario), sending the game input, screenshots or remote-control commands, or installing into the game folder (cargo xtask install).
---

# Trying things in the game

Every session on this computer tries things in a test game of its own (GLOSSARY): a game started from the one game folder with a user folder, a core and a remote control pipe of its own. It touches neither the player's files nor the framework installed in the game folder, and any number run at once; a claim holds one until you release it. `cargo xtask game` keeps the claims, one record for the whole computer; run it from your own checkout. The test game starts hidden and silent, so nothing of it reaches the desktop or the speakers of the person at the computer, unless the user asks to see or hear it.

## Steps

1. **Build first, with the test games' pipes**: `cargo build --release --features eu4-harmony-core/dev-test-games`, with any other `dev-*` feature the run needs (`--features eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-memory`). Without it `game start` refuses the core. Plan the run too: what to try, which saves it loads.
2. **Claim**: `cargo xtask game claim --purpose "<one line: what this run tests>"`. It makes the claim's folder, with the test game's user folder copied from the player's settings and playset, and prints it.
   - Exit 0: the test game is yours.
   - Exit 2: a checkout from before test games holds the whole game. Run the same command with `--wait` as a background command, carry on with work that needs no game, and end the turn once that is done: it exits once you hold one.
   - Refused because the game folder's proxy cannot load a test game's core: the installed framework is from before test games. Tell the user; installing is theirs to decide (below).
3. **Start**: `cargo xtask game start --save <name>`, a `--save` for each of the player's saves the run loads, by its name as `menu.saves` gives it. It copies your built core and `dev.toml` into the test game and starts it hidden, returning at its main menu; `cargo xtask game stop` ends it, and a rebuild before the next `start` is taken up then.
4. **Use it**: ask your test game, never the player's: `eu4-harmony game --test-game <owner> …` (`screenshot`, `input`, the capabilities), or set `EU4_HARMONY_TEST_GAME=<owner>` for every call; `claim` prints the line to use. `cargo xtask scenario` claims, starts and releases a test game itself, and within a claim of yours plays in it and leaves it held. `cargo xtask profile --test-game <owner>` samples it. Never start `eu4.exe` any other way.
5. **Release**: `cargo xtask game release`. It ends the test game, keeps its framework logs, game logs and crash folders in the claim's `kept\`, which it prints and keeps for seven days, says how each of its games ended (an exit code that is not 0 or 1, such as `heap corruption`, is a crash, though it leaves no crash folder), and removes the rest. Read the results there, and copy what you need for longer to your scratchpad.

Done when `release` reports `released` and `cargo xtask game status` lists no folder of yours.

## Holding it

Nothing takes your test game from you, and nothing limits how many are held: it is yours until you release it. So release it when you are done; every one held takes memory and the graphics card's from the computer.

## Reference

- **Owner**: the leaf name of your checkout's folder, unless `--owner` names another; it names your test game and its pipe, `\\.\pipe\eu4-harmony-<owner>`. Sessions sharing one checkout each pass their own `--owner`, to every command.
- **Options**: `game start --timeout <seconds>` waits longer than 300 s for the main menu; `game start --visible` shows the window at the main menu and `--sound` keeps the sound, each only when the user asks to watch or hear the game (`eu4-harmony game --test-game <owner> desktop hide` hides it again); `--seed <n>` passes the game its own `-seed`; `--game-dir` names a game folder other than the one found.
- **Status**: `cargo xtask game status` prints each test game's holder, purpose and process, the `eu4.exe` that are no test game's (the player's, or an older checkout's), and folders no claim refers to.
- **The player** plays from Steam as always, beside the test games: their game is no claim's, and no command ends it.
- **Installing into the game folder** (`cargo xtask install`) changes the player's own framework, and is refused while any `eu4.exe` runs, test games included: do it only when the user asks.
- The record is `%LOCALAPPDATA%\eu4-harmony-dev\claims.json`, the claims' folders under `claims\`; `game-lock.json` beside it names `test-games` while any is held, so that checkouts from before test games wait. The design is [docs/plans/test-games.md](../../../docs/plans/test-games.md).
