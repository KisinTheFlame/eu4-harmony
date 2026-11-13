# Changelog

What changed in each version of Harmony that players and mod authors notice. Merging a version PR automatically publishes its section as the GitHub Release notes; a local Steam Workshop upload uses the same section as its change note (`cargo xtask workshop-upload`).

## 0.2.0

- Version tags now publish GitHub Releases with separate Windows x64 framework and CLI packages. Framework downloads are installed and updated manually; Steam Workshop uploads remain a local operation.
- Faster startup with large mods: localisation is loaded once, replacement lookups are indexed, and trigger profile matching reuses equivalent trigger structures. The drawing thread also sleeps while it has no prepared frame during startup.
- Improved startup folder lookups when a mod includes non-ASCII filenames.
- More reliable native window and tooltip lifetimes: partial window creation is cleaned up, replaced controls retain their current bindings, and delayed tooltip drawing follows explicit ownership.
- Extension save data is retained through ironman setup saves that run before the session starts.

The large-mod startup changes were checked against the game's own results with zero differences in the tested runs. Guixu 1.254 reached the main menu in about 20 seconds instead of 31–32 seconds on the test computer; results depend on the mod and computer.

## 0.1.5

- Fixed: hovering a country's core icon could lose the map outlines of its core provinces when the drawing thread was enabled. Prepared frames now retain the temporary highlights they need.
- Fixed: with achievements outside ironman enabled, new non-ironman games now build and retain their tracked achievement list, and daily completion uses the same eligibility setting.
- Mods can use one localized text description for labels and tooltips. Managed windows, panel entries and framework controls share tooltip updates, visibility and cleanup, with the game's native presentation.
- Panel entries can use a mod's own image file, show an optional formatted number, and be hidden or disabled. The icon and the number follow the game's separate interaction rules.
- Mods can read an owned area's members and governing costs, predict the capacity added by making it a state without completing cores, and separately read live national governing capacity.
- Framework logs now rotate and clean up their own old files, summarize repeated errors, and retain a small recent-event snapshot. Records identify the run, process, thread and source; critical writes have a separate fallback.
- Troubleshooting files collect evidence within a size limit, prefer current logs and recent crashes, retain the tail of large game logs, and redact authentication fields.

To apply the proxy's new log rotation, copy the updated `version.dll` from Harmony's Workshop folder into the game folder while the game is closed. The core updates through Steam as usual.

## 0.1.4

- Fixed: an event's option text could be drawn before its initialization finished, leaving the text misaligned. The drawing thread now waits for the whole event window and its options to be initialized.

## 0.1.3

- Fixed: since 0.1.2, the picture could stop while the sound and the game's time went on, after switching country, declaring war or continuing a saved game.

## 0.1.2

- Fixed: an event's first option could show part of another window (the top bar's numbers, a shrunken panel, an empty band) and the screen could flash as the event appeared. Everything the game draws in the middle of a day now waits for the map's own drawing to finish.

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
