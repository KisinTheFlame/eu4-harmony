# 2026-11-04 The bar of the units selected (tested by me)

The repair that kills the unit view's selection bar as the view goes ([research/main-menu.md](../research/main-menu.md), "The bar of the units selected"). A player build of this branch with the input branch (`claude/hungry-goldberg-462755`, for `eu4-harmony game input`) merged in for the run only; 769 dependency points matched. The player's playset, 3840x2160, `eu4.exe` started directly; the game claimed with the game-lock skill, which backed up the player's files; `dev-install.ps1` replaced the framework files in the game folder. Armies were selected by clicking their banners, by drawing a box round them, or from the outliner, at positions read off screenshots.

- **Before**: the input run of the same night saw it twice: after `menu.start CAS` from a France session with the Paris army selected, the "1 陆军" bar stayed at the top left of the Castile session's map. Not run again without the repair.
- **With the repair**, each next session and the main menu without any part of the army panel:
  - `menu.start CAS` from France (session 1) with the Paris army selected ("1 陆军");
  - `menu.return` from that Castile session with both its armies selected ("2 陆军");
  - `menu.start FRA` from the main menu, the Paris army selected, `menu.load 英格兰1444_11_11` (session 7);
  - the player's in-game load from the England session (8), through the game's menu, the load window and its question;
  - the player's return from it by the game's question and the end-of-game summary, the Army of Scotland selected under the question ("1 陆军" shown with the summary);
  - France from the main menu, both its armies selected ("2 陆军"), `menu.start CAS` (12); both Castile armies selected, `menu.load 英格兰1444_11_11` (14).
- **What the game does itself**: opening its menu, by Esc or by the menu button by the minimap, deselects: with units selected Esc deselected them without opening the menu, and the menu button opened the menu with nothing selected after. The load window holds the map (clicks on it did nothing); the return's question does not, and units could be selected while it was open.
- **The log**: no line of the repair's failing; the in-process returns let 13 to 14 parts of the session's interface go, as before.
- **Not tried**: several units selected under the return's question (only one of the England armies could be reached from the outliner); a fleet selected; the in-game rebuild with units selected while the game stays in the same session; counting the GUI root's objects after each round.
- **Cleanup**: the game was closed by ending its process. Releasing the game restored the player's files and found every file as the backup had it. The framework files of the test build stay in the game folder.
- **Cleanup**: the game was closed by ending its process. Releasing the game restored the player's files and found every file as the backup had it.
