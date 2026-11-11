# 2026-11-11 A save chosen in single player without the transfer's copy (tested by me)

S11 of [game-speed.md](../plans/game-speed.md): the save chosen in the single player setup added to the file transfer without the level-9 deflate nothing reads in single player ([research/save-transfer-copy.md](../research/save-transfer-copy.md)). The branch at `6b3a4f89` with the part (`loading::transfer`, the speed-up `transfer-copy`); the release build with the test games' pipes, the call trace and the test switches; one test game, hidden; the 1780.7.1 save of the developers' saves copied into its `save games`. `dev.toml` traced `0x1132ca0` (the setup's read of the chosen save), `0x153cfc0` (the transfer's deflate) and `0x110d760` (the load) with `times`; the transfer's add `0xd7aad0` itself is the part's and cannot be traced.

- **With the part**: `menu.load observer-1780.7.1` from the main menu; `console run observe`, `time advance 20` to 1780.7.21; `session view TUR`, `console run autosave` (69,095,978 bytes, plain text); `menu return`; `menu load autosave`, the setup's way again; `time advance 10` to 1780.7.31. Both loads played on.
- **Left to the game**: the same build, `[test-switches] game-paths = ["transfer-copy"]` (the startup report says so); `menu.load observer-1780.7.1`, `menu return`, `menu load autosave`.

| | The setup's read, the part | The setup's read, the game's | Of it the deflate, the game's | The load after it, the part | The load, the game's |
| --- | --- | --- | --- | --- | --- |
| `observer-1780.7.1` (69,073,157 bytes) | 0.047 s | 2.852 s | 2.799 s | 3.243 s | 5.048 s |
| the autosave of 1780.7.21 (69,095,978 bytes) | 0.033 s | 2.542 s | 2.510 s | 3.759 s | 3.151 s |

So each save chosen was 2.5 to 2.8 s sooner on the main thread; the deflate was not called with the part. The load itself is the game's either way, and its times vary between runs by as much as their difference here (the first load of a start reads the save's textures and the like into the cache).

- **Not verified**: a hosted multiplayer game and a client, which the part leaves to the game by the world's multiplayer kind and which could not be tried here (the code path is in the research note: the host adds its file without a copy itself, `0xd84815`); the continue box's own button, which calls the same read (`0x10ef410` → `0x1130f40` → `0x1132ca0`) as `menu.load` does; a zipped save, which the add never deflated; a game a player plays on an idle machine.
- **Cleanup**: `dev.toml` removed; the release kept the logs in the claim's `kept\` and said the game did not crash; the two time files in the scratchpad.
