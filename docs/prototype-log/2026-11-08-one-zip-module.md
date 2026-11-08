# 2026-11-08 One zip module and one reader of key="value" lines (tested by me)

Whether the game reads what the framework's one zip module writes and the framework reads what the game writes ([deepening.md](../plans/deepening.md), D8 and D16's line reader). The player build with the test games' pipes, of this change on `claude/improve-codebase-architecture-80be67` at `22117b5f`; the player's ironman save `England-1` (a zip, 1448.3.1), one test game of about 6 minutes, started hidden.

- **Framework files**: the archive the new writer packs was mounted (`framework files: mounted 1094 bytes`, the console font's three files), and the game logged no error about them.
- **Music cache**: the start wrote `music.txt` anew in the test game's user folder, 246 of its 270 lines entries of the DLC's zips, their CRCs and sizes read from each zip's directory by the new reader; no directory failed to read; the main theme played.
- **Settings and the game's language**: the framework settings read as before through the one line reader (language, the four fonts, every toggle), and the game's language from `settings.txt` (`english`).
- **Save headers**: `menu.saves` gave the player, the date and no missing mods for the zipped save before the load, and for each save the game wrote after.
- **Background compression**: the ironman saves of a year and a half at speed 5, and the save before an in-game load and before the return to the main menu, each about 37 MB stored, were compressed in the background to about 3.5 MB in 490 to 555 ms, each read back whole before it took the file's place. `menu.load` played the compressed save in game (an in-game load) and from the main menu, whose continue box showed England at 1450.1.1.
- **Measured** (not in the game): the inflate, now `miniz_oxide`'s into a buffer of the entry's size, took 30 ms on 58 MB of game state text against the hand-written inflate's 90, and 2.8 µs against 4.6 on a header of 3.6 KB.
- **Not verified**: a plain (uncompressed) save, and a zipped save with a `meta` in text, in the game (the unit tests cover both); a save made with the save window's "compress" unticked; a developer build.
- **Cleanup**: the release kept the test game's logs in its claim's `kept\` and removed the rest; the benchmark's files deleted from `target\`.
