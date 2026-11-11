# 2026-11-11 The startup's stretches in the framework log (tested by me)

The player build's own timing of the game's startup and of a save's load (`core/src/loading_times.rs`), on a test game: a developer build of this branch (master at `10b16708`) with the test games' pipes and `dev-call-trace` timing functions inside the stretches, to check the new lines against it; the player's playset, every DLC; another session's test game ran beside it throughout.

- **The lines**: every start logged one `loading:` line as each stretch's function returned, on its own thread, and the summary at the main menu, as [research/loading.md](../research/loading.md) ("The startup's stretches in the framework log") shows. Three starts, seconds from the bootstrap:

| Stretch | Start 1 (music not kept yet) | Start 2 | Start 3 |
| --- | --- | --- | --- |
| `main` up to the engine's Init | 0 to 2.79 | 0 to 2.47 | 0 to 2.18 |
| The engine's Init `0x14c02b0` | 2.79 to 13.51 | 2.47 to 2.68 | 2.18 to 2.40 |
| The gfx stage `0x202850` | 13.63 to 17.78 | 2.80 to 6.30 | 2.51 to 6.03 |
| of that, meshes and particles `0x14dc5d0` | 15.19 to 17.71 | 4.43 to 6.23 | 4.21 to 5.96 |
| The databases `0x208620` | 17.89 to 22.33 | 6.41 to 10.57 | 6.13 to 11.45 |
| The events `0x6f3250` | 22.35 to 24.65 | 10.60 to 12.71 | 11.47 to 14.25 |
| Flags and loading screens `0x10b2b70` | 24.67 to 25.75 | 12.75 to 13.19 | 14.28 to 14.81 |
| The rest of InitGame `0x2043a0` | 25.75 to 27.88 | 13.19 to 16.19 | 14.81 to 18.13 |
| The main menu's part `0x206260` | 27.89 to 30.94 | 16.21 to 18.27 | 18.13 to 20.02 |
| The main menu | 31.40 | 18.69 | 20.41 |

- **Against the call trace**: the trace's times count from its own start, 0.08 to 0.09 s after the bootstrap in these starts. Its functions lay inside the stretches the new lines gave, within that: the mesh types `0x14dc390` at 4.65 to 6.23 s and 4.42 to 5.96 s in the meshes' 4.43 to 6.23 and 4.21 to 5.96; the map logic `0x87b9e0` at 7.30 to 7.84 and 7.20 to 7.92 in the databases; the map's graphics `0x10a2e40` at 16.20 to 17.37 and 18.13 to 19.14 in the main menu's part, each ending where its stretch ends when it is the stretch's last part. In the first start `0xb9b4e0` was traced too, which the triggers' optimization profile calls 2.3 million times: its timed calls slowed the rest of InitGame and wrote a file of more than 64 MB, so it was left out after.
- **The first start**: the test game's user folder had no music kept yet, so the engine's Init took 10.7 s reading the music, against 0.2 s in the two starts after it; a player's first start after installing does the same.
- **A save's load**: observer-1700.1.1 (66 MB, `player="TUR"`) loaded with `menu.load` from the main menu in the first start: "the save read and the world set up (0x5cf740) from 62.90 to 66.82 s, 3.92 s" and "a save's load (0xd84b80) from 62.89 to 67.94 s, 5.05 s".
- Tests, both clippies and the offline checks pass; the offline checks matched the ten points' bytes and found that three read more arguments than first given (`0x202850` and `0x10b2b70` two, `0xd84b80` five), which the points now forward.
- Three starts, no crash; the release said each game ended by `game stop` or the release.
- **Not verified**: the player build itself (the lines are the same code in every build, no feature gates them); a load from inside a game, and the other ways in to `0xd84b80` (the queued load, a multiplayer client's received save); a device reset after the main menu, which may call the main menu's part again (only its first call is logged); a playset other than the player's; the machine otherwise idle.
- **Cleanup**: `dev.toml` removed; the release kept the logs; the copies of the logs and call times deleted from the scratchpad.
