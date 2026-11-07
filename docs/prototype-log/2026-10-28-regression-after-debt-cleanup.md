# 2026-10-28 Regression after the debt cleanup, and call tracing (tested by me)

Backups as above. A build with the `trace` feature, tracing font vtable `+0x48` (`0x159c600`) and `+0x80` (`0x159f510`); a new game as Poland.

- **Startup**: every dependency point matches (the field check points in `objects.rs` included); the report at the main menu is normal.
- **Call tracing**: the log writes each call's arguments, the text they point to, and the return value.
  - Every call to `+0x80` comes from text box layout at `0x15f440f`: the loading screen's status and tips (an 800×200 box), labels in the main menu and the lobby, the music list and so on, over three thousand times at the start of a game.
  - `+0x48` (floating text) was not called once, from the main menu and the lobby through the start of a game to several months into it.
- **The line breaking branch of `+0x80`**: the description on the diplomacy page of the macro builder (B key) wraps into two lines in a box 290 wide, "…自动开始改善关 / 系的目标类型。" (the break falls between two Chinese characters), and the full stop does not land at the start of a line.
- **The host window table**: regional-combat's "区域" (Region) button appears in the first slot of the third column of the army panel, as before the change.
- **Costs**: by the main menu, 84026 line break queries, 24862 pieces of text whose breaks ICU worked out, 148 ms in all (about 6 µs each); 7 glyphs, 15 ms; 6 font pages, 236 ms.
- **Saves**: the game autosaved during the first game; `autosave.eu4` rotated through three copies and `continue_game.json` was rewritten. Both are restored from the backup, and the checksums match.
- **Tools**: in `game-input.ps1` the constants `$WHEEL` and `$keys` had the same names as the parameters `-Wheel` and `-Keys` (PowerShell variables ignore case), so they overwrote the parameters and the wheel kept turning; they are renamed. Called with `pwsh -File`, a comma list becomes one string; call the script from PowerShell with `&`.
