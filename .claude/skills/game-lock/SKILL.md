---
name: game-lock
description: Claim the game before using it, and release it after; the claim backs up the player's files and the release restores them. Use before installing into the game folder (cargo xtask install), backing up or restoring player files, starting eu4.exe, or sending the running game input, screenshots or remote-control commands.
---

# Claiming the game

Every session on this machine, whatever its checkout, shares one game: its folder, eu4.exe, the player's files and its window. One holder has the game at a time. `game-lock.ps1` in this skill's folder keeps the claim, one record for the whole machine; call it with `&` from PowerShell, your own checkout's copy.

The claim is also the backup AGENTS.md asks for: `-Claim` runs `eu4-harmony backup` and `-Release` runs `eu4-harmony restore`, your checkout's, built by cargo as needed. Running a backup yourself only adds a second backup to restore.

## Steps

1. **Do the work that needs no game first**: build, unit tests, reading code. The claim is held only while the game is in use.
2. **Claim**: `& <this skill's folder>\game-lock.ps1 -Claim -Purpose '<one line: what this run tests>'`.
   - Exit 0: you hold the game, with the player's files backed up. Start `game-lock.ps1 -Watch` as a background command at once: it is what wakes you before the lease runs out (step 3).
   - Exit 2: another holder has it (the output says who). Run the same command with `-Wait` as a background command, carry on with work that needs no game, and end the turn once that is done. The command exits when you hold the game; start `-Watch` then, as for exit 0.
   - "not claimed: the backup failed …" (exit 1): the game is free again, with no claim of yours. Claim once more; failing the same way, show the user the error.
   - "took over from …": the previous holder let its lease run out; the claim restored that holder's backup first. Tell the user who it was. If that restore fails, the claim stops with an error yet holds the game: show the user the error and the backup `game-lock.ps1` (no switch) lists, and go on once they say how.
3. **Use the game**: install, run, test, as AGENTS.md describes. The lease is 30 minutes; it starts again at every `-Claim` and `-Renew`, and keeps starting again while eu4.exe runs, so it counts only the time the game is closed. When `-Watch` exits with 3, the lease is 5 minutes from running out:
   - still using the game: `-Renew`, then start `-Watch` again;
   - done, or waiting on the user: go on to step 4 and release, so a waiting session can have the game.

   `-Watch` exiting with 0 means you no longer hold the game: it was released or taken over.
4. **Keep what the run produced**: release puts back every player file, the framework's logs and the run's crash folders included. Copy the logs you need to your scratchpad, or release with `-KeepCrashes` to look at a dump. Close the game.
5. **Release**: `& <this skill's folder>\game-lock.ps1 -Release`. It restores the backup with `eu4-harmony restore` and frees the game. On a `NOT RESTORED` line it keeps the claim and stops: show the user those lines.

Done when `-Release` reports `released` and `game-lock.ps1` (no switch) lists no backup of yours.

## Reference

- **Owner**: the leaf name of your checkout's folder, unless `-Owner` names another. Sessions sharing one checkout each pass their own `-Owner`.
- **Status**: `game-lock.ps1` with no switch prints the holder, its purpose and lease, whether eu4.exe runs, and backups no claim refers to (left by a takeover whose restore failed).
- **eu4.exe running with no claim** means the player is playing; a claim waits for the game to close.
- **The player** claims with `-Owner user -NoBackup -Lease 600`, so their play is kept, and releases with `-Owner user -Release`.
- The record is `%LOCALAPPDATA%\eu4-harmony-dev\game-lock.json`, the backups are under `%LOCALAPPDATA%\eu4-harmony-dev\backups\`.
