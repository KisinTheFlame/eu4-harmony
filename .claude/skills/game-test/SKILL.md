---
name: game-test
description: Claim the game before trying anything in it, start it hidden, and release it after, which puts back the player's files and the game folder's framework files. Use before installing into the game folder (cargo xtask install), starting eu4.exe, playing scenarios (cargo xtask scenario), or sending the running game input, screenshots or remote-control commands.
---

# Trying things in the game

Every session on this computer, whatever its checkout, shares one game: its folder, eu4.exe and the player's files. One holder has it at a time: the claim (GLOSSARY). `cargo xtask game` keeps it, one record for the whole computer; run it from your own checkout. The game is started hidden and silent, so nothing of it reaches the desktop or the speakers of the person at the computer, unless the user asks to see or hear it.

## Steps

1. **Do the work that needs no game first**: build, unit tests, reading code. The claim is held only while the game is in use.
2. **Claim**: `cargo xtask game claim --purpose "<one line: what this run tests>"`. It backs up the player's files and copies the game folder's framework files, and prints the claim's folder.
   - Exit 0: you hold the game. Start `cargo xtask game watch` as a background command at once: it is what wakes you (step 4), and only while it runs does the game running keep your lease going.
   - Exit 2: busy, the output says who holds it. Run the same command with `--wait` as a background command, carry on with work that needs no game, and end the turn once that is done. It joins the queue, first come first served, and exits once you hold the game; start `watch` then, as for exit 0.
   - "took over from …": the previous holder let its lease run out, and the claim put back what its run changed first. Tell the user who it was.
   - "not claimed: …" (exit 1): the game is free again, with no claim of yours. Claim once more; failing the same way, show the user the error.
3. **Use the game**, only while you hold it: none of the commands below checks the claim or keeps it going, which is the claim's commands' alone. `cargo xtask install`, then `cargo xtask game start`, which starts the game hidden and returns at its main menu; `cargo xtask game stop` ends it. `cargo xtask scenario` builds, installs, starts and plays, and ends the game at the end; `cargo xtask profile` samples the game that runs. Drive the game with `eu4-harmony game …` (`screenshot`, `input`, the capabilities); never start `eu4.exe` any other way.
4. **When `watch` exits**:
   - 3: the lease (30 minutes, kept going while the game runs and `watch` does) ends in 5 minutes. Still using the game: `cargo xtask game renew`, then start `watch` again. Done, or waiting on the user: release (step 6), so the next in the queue gets the game.
   - 4: the player takes the game, in two minutes. Stop using it, a scenario run or a profile included (the runner counts the game ended as a crash and would start it again), copy what you need from the user folder's logs if anything, and do not release: the player's claim ends the game and releases yours. Your run is unfinished; say so where you record it.
   - 0: you no longer hold the game.
5. **Keep what the run produced**: release copies the framework's logs, the game's logs and the run's crash folders into the claim's `kept\`, which it prints and keeps for seven days. Copy what you need for longer to your scratchpad. `--keep-crashes` leaves the crash folders in the user folder too.
6. **Release**: `cargo xtask game release`. It ends the game if it runs, puts back the player's files and the game folder's framework files and `dev.toml`, compares each, and frees the game. On a `NOT RESTORED` line it keeps the claim and stops: show the user those lines.

Done when `release` reports `released` and `cargo xtask game status` lists no claim's folder or backup of yours.

## Reference

- **Owner**: the leaf name of your checkout's folder, unless `--owner` names another. Sessions sharing one checkout each pass their own `--owner` to every command of the claim.
- **Options**: `claim --lease <minutes>` sets the lease (30 without it); `game start --timeout <seconds>` waits longer than 300 s for the main menu; `game start --visible` shows the window at the main menu and `--sound` keeps the sound, each only when the user asks to watch or hear the game (`eu4-harmony game desktop hide` hides it again); `watch --warn <minutes>` exits that long before the lease's end (5); `--game-dir` names a game folder other than the one found.
- **Status**: `cargo xtask game status` prints the holder, its purpose and lease, the queue, whether eu4.exe runs, and claims' folders and backups no claim refers to.
- **eu4.exe running with no claim** means the player is playing; a claim waits for the game to close.
- **The player** claims with `cargo xtask game claim --player`, which keeps their files as they play and goes first in the queue, and `--now` to take the game from its holder after two minutes; they release with `cargo xtask game release --player`. A session never claims as the player.
- The record is `%LOCALAPPDATA%\eu4-harmony-dev\game-lock.json`, the queue `game-queue.json` beside it, the claims' folders under `claims\`. The design is [docs/game-testing.md](../../../docs/game-testing.md).
