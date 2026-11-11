# 2026-11-11 The settings window without explanations, and the name Harmony (tested by me)

Parts W4 and W5 of [workshop-release.md](../plans/workshop-release.md), and the core's side of W1. A hidden test game from this branch's core with `dev-test-games`, at the main menu, no save loaded; the player's playset and settings, Simplified Chinese, 3840×2160. The game folder's `version.dll` was the one installed before this branch.

- **The main menu**: its corner shows "Harmony Mod" and "Harmony 设置".
- **The settings window**: titled "Harmony 设置". The pages hold only their settings, with no explanation under any: "语言与字体" its language, fonts, weights and "地图文字包边"; "声音" its two toggles, "所有曲目一样响" and "缩小音乐的轻响差距". The foot reads "Harmony 0.1.0 · 反馈问题：https://github.com/KisinTheFlame/eu4-harmony/issues", wrapped onto two lines at the window's width.
- **The restart line**: absent at first, its room kept above the save button; turning "地图文字包边" off showed "语言、字体和地图文字的修改在下次启动游戏后生效。" there, and it stayed on the sound page while the change was pending. The change was cancelled.
- **The startup report** opens "Harmony 0.1.0 (api 0.3), developer build with the test games' pipes"; `core.log` has `proxy interface: 0`, the installed proxy being from before the interface, which the core accepts as its lowest.
- **Not verified**: the new proxy itself, which was not installed into the game folder: its log in the user folder, its interface 1, and loading the core from Harmony's Workshop item, which waits for the item's id (part W8). The other four languages' labels, and the restart line after changing the language or a font.
- **Cleanup**: the test game released; it quit with exit code 1 at the release, no crash; its logs kept in its claim's `kept\`.
