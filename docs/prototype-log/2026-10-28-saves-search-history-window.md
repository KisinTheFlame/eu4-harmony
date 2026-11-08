# 2026-10-28 Saves, search and the history window (tested by me)

Before the test I backed up `save games\`, `eu4-harmony\saves\`, `settings.txt` and `continue_game.json`. A new game as Ming:

- **Province search**: F opens it; typing "北京" (Beijing) lists "北京". (The F key itself also gets typed into the box; vanilla does that.)
- **Saves with Chinese names**:
  - The save dialog's default name is already Chinese ("明1444_11_11"); saved as "测试存档中文.eu4" ("test save Chinese"), it shows correctly in the list.
  - I turned regional-combat on for the army in Beijing and saved as "区域战斗存档.eu4" ("regional combat save"). The framework log says the extension data is bound to `…\save games\区域战斗存档.eu4`.
  - Loading this save: the log has `loaded …区域战斗存档.eu4 with data for 1 extension(s)`, and regional-combat's log `session 2: restored 1 task(s)`.
  - Afterwards I deleted the two test saves and that extension data. The game had rewritten `continue_game.json` to point at a test save; it is restored from the backup. The checksums of the other 140 saves match the backup.
- **Country history window** (the "History" button by the minimap): "「谨慎者」Yu / anzhang" (「谨慎者」 is an epithet, "the Cautious") used to be split. The window breaks its text into lines with the game's multi-line text box, which breaks only at spaces (see [research/text-input.md](../research/text-input.md)). After the fix, "Yuanzhang。" and "Yunwen。" move whole to the next line.
- **The "System fonts" look in a game**: Ming's start-of-game introduction (with names highlighted in yellow), the outliner and the top bar are all Microsoft YaHei, and lines break correctly. Afterwards the setting went back to "Keep the fonts as they are".
- **Searching without accents**: the Chinese language mod turns all place names into Chinese, so there was no accented Latin place name to search for, and this could not be verified in the game; unit tests cover it.
