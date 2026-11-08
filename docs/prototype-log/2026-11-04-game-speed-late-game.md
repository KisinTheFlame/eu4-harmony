# 2026-11-04 Game speed late in a game, the exit after a save, and a compressed save in vanilla (tested by me)

The build of master at `c6bda5f` (the game speed plan's S1 to S4), installed with `tools/dev-install.ps1` over the framework files another session had there, which were copied aside first. Austria-37_4-1, an ironman save of the Holy Roman Empire, 1724.5.13 when loaded; the player's playset; 3840x2160. Frames counted at the application's frame count and days at the date; speed 5's factor set with `WriteProcessMemory` (the game's 0.05 against the 1e6 "Faster time" writes), while a script kept the game at speed 5, resumed it and answered its questions.

**Paused**, in 1724: 44.6 and 49.7 frames a second over 20 s each.

**Speed 5, "Faster time" against the game's pace.** Counted are the runs in which the script resumed the game every 0.3 s, so a message that paused the game cost little, in the order smooth, faster, faster, smooth, 60 s each, four times, 1730 to 1803:

| | Days a second, each | Mean | Frames a second |
| --- | --- | --- | --- |
| The game's pace | 17.7, 19.5, 18.1, 15.1, 16.9, 15.0, 16.4, 13.1 | 16.5 | 20.0 to 24.2 |
| "Faster time" | 20.7, 21.4, 19.4, 18.8, 18.9, 20.0, 16.7, 17.0 | 19.1 | 16.9 to 21.6 |

- In each group of four, "Faster time" was ahead of both its neighbors: 16% faster in all.
- Before those, with the game resumed only every 2 s, three pairs each way round (30 s each, 1724 to 1730) came out the other way: the game's pace 19.3 to 24.2 days a second, "Faster time" 15.8 to 17.8. Why was not found. One pair under the sampler (45 s each, 50 a second) gave 15.4 for the game's pace and 19.7 for "Faster time".
- The main thread's samples in that pair: the ironman autosave 13 to 20% (the game saved every few seconds of play), frames inside a day 16% of the game's pace and none with "Faster time".
- The game piled up its messages: by 1803 more than a hundred waited, each a window of its own.

**The exit right after a save.**

- **Returning to the main menu with a restart** (the framework setting `return_without_restart=no`, `menu.return`): the game wrote the ironman save at 18:01:48.3; the restart's observer waited 1.5 s for the thread ("waited 1.5s for the background compression"), and the process ended at 18:01:50.2. The file: 6,076,089 bytes, every entry deflated, Python's `zipfile` testing it whole.
- **The process ended 0.5 s after an ironman autosave**, while the thread compressed it: the save stayed as the game wrote it, 55,280,102 bytes, deflated at level 0, whole; no `.eu4harmony-tmp` beside it.
- **Not tried**: the game's own exit (its menu's exit button): the input capability's clicks on the in-game menu did not reach its buttons in this run, the game's hover tip staying on one province, which was not looked into. Nor an end of the process in the moment the thread writes its temporary file, which would leave `<save>.eu4harmony-tmp` beside the whole save.

**A compressed save in vanilla.**

- The framework removed with `tools/dev-install.ps1 -Uninstall` (no `version.dll`, no `eu4-harmony` folder in the game folder). A copy of the 1803 save the thread had compressed (6,077,219 bytes, its entries deflated at level 6) put in `save games` as `eu4harmony_vanilla_test.eu4`.
- `eu4.exe -continue eu4harmony_vanilla_test.eu4` (research/main-menu.md, "Startup"): the continue box showed the save, a window with two buttons opened (its text unreadable, the Chinese language mod's text needing the framework), Enter posted to the game's window answered it, and the game loaded 1803.3.1 as the Holy Roman Empire, its treasury, manpower and the map as in the framework's game. Unpaused, 1803.3.4 to 1804.2.3 in 15 s. The game's ironman save then wrote it again, 7,752,332 bytes.
- `-continuelastsave` with `continue_game.json` naming the save, and `-continue=<file>`, stayed at the main menu; `-continue` alone crashed at startup ("Could not open file: save games/"), the crash being the argument's.

**Cleanup**: the game closed by ending its process. The framework files put back and compared equal to the copies (SHA-256). Releasing the game restored the player's files, removed the test saves, the save the vanilla game added and the crash folder, and found every file as the backup had it.
