# Changelog

What changed in each version of Harmony that players and mod authors notice. Each upload to the Steam Workshop is a version of its own, and its section here is the upload's change note (`cargo xtask workshop-upload`).

## 0.1.1

- A new About page in Harmony's settings shows its version and saves the troubleshooting files to your desktop in one click: Harmony's logs, the game's logs, its last crash folders, your settings and playset, no saves. Attach that file when you report a problem.
- When the game crashes, Harmony's log now says where, so a crash can be looked into from the logs alone.

## 0.1.0

First release.

- Chinese, Japanese and Korean display correctly everywhere, with no double-byte patch or font mod.
- Return to the main menu in seconds, without restarting the game; ironman games can load another save.
- A faster game with the same rules: the map drawn on a thread of its own, shorter save pauses, a quicker start and load.
- Fixes for bugs the unmodified game still has, and for its memory leaks.
- Mods can read the game, act in it as the player does, and add buttons and windows, in Lua.
