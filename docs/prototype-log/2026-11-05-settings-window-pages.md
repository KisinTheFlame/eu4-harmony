# 2026-11-05 The settings window's pages (tested by me)

The settings window with its settings on four pages chosen in a side list, the player build of this branch installed within a claim and started hidden. The player's playset in Chinese, 3840x2160 at interface scale 2.

- **The pages**: the side list showed 字体, 读档与返回, 成就, 游戏速度, 字体 chosen. 字体 held the four fonts with their weights, 地图文字包边 and the restart line; 读档与返回 its two toggles and the undo line; 成就 its two toggles and their line; 游戏速度 最快速度时 with its info mark and the choice bar. On every page the window kept its size, the fonts page's, and 保存 and 取消 stood in the same place at its bottom.
- **A change on another page**: on 读档与返回, 铁人模式下读档前先保存当前游戏 was turned off; then 字体 was chosen and 保存 pressed. The framework's `settings.txt` got `save_ironman_before_load=no`, and the window opened again showed the toggle off. Escape closed it.
- **Not seen in the running game**: the fifth page, 远程控制, which master gave the window after this run (its toggle and explanation as the run before this showed them under a heading); the other languages' page names; a drop-down list opened on a page and the page changed while it is open (the list closes with the click, as for any click); the mods window, whose side list code changed with this (its unit tests pass).
- **Cleanup**: the release put back the player's files, the framework's `settings.txt` among them, and the game folder's framework files; the screenshots were deleted.
