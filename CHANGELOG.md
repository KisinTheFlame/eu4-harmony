# Changelog

What changed in each version of Harmony that players and mod authors notice. Merging a version PR automatically publishes its section as the GitHub Release notes; a local Steam Workshop upload uses the same section as its change note (`cargo xtask workshop-upload`).

## 0.3.2

- Fixed: a safe point could run during a frame inside a day while the daily update was still in progress, allowing an extension or remote request to load a save or leave the session mid-day and crash the game after the world reset. Safe points now run only on normal frames outside a daily update, in-game loads are refused while a daily update is in progress, and leaving a session right after a day ends waits for the day's queued commands to finish first.

The core updates through Steam Workshop with no proxy replacement required; restart the game after the update. Manual framework downloads include the updated core and proxy in the same layout as 0.3.1.

## 0.3.1

- Fixed: hovering interface elements while the drawing thread was held back during a day could crash when freed embedded controls such as scrollbars remained in stale scene lists. Scene lists are now refreshed before tooltip updates.
- Fixed: crashes occurring inside Harmony's native hooks now unwind cleanly back to the game's crash handling, so Harmony writes its crash record and minidump and the game creates its crash folder instead of exiting silently.
- Fixed: troubleshooting files now include Windows Application Error events for `eu4.exe` on Windows 10 as well as Windows 11.
- The settings window's **Game speed** page is now **Display**, replacing the unlimited-frame-rate toggle with a **Frame rate limit** choice bar (`30`–`240`, your monitor's refresh rate, or `∞`). By default it follows the refresh rate of the monitor showing the game window; previous `drawing_unlimited` entries are ignored until you choose a limit again. Choosing `∞` also avoids busy-waiting while a crossing holds the drawing thread back.
- For mod authors: `eu4-harmony check` now enforces the same manifest identifier and interface fragment rules as the core at startup, `eu4-harmony test` shares the core's Lua `require` resolution and extension reload handoff, and `eu4-harmony start` checks the proxy, core and user folder consistently.

The core updates through Steam Workshop with no proxy replacement required; restart the game after the update. Manual framework downloads include the updated core and proxy in the same layout as 0.3.0.

## 0.3.0

- Harmony now saves minidumps automatically when the game crashes: before the game writes its own crash files, preserving the original exception registers, and through Windows Error Reporting when a fatal exception bypasses the game's crash handling.
- The **About** page in Harmony's settings window adds **Enable Windows crash dumps for EU4 (admin)** to set a Windows `LocalDumps` folder for `eu4.exe` after a one-time administrator prompt, without running the game as administrator.
- Troubleshooting files now include completed Harmony minidumps, recent Windows Application Error events for `eu4.exe`, and Windows minidumps matched to a logged run by process ID and creation time. Minidumps can contain process memory; nothing is uploaded automatically.
- Faster save loading: in-game loads of local plain-text saves read from memory while keeping the file open, country-distance rebuilds reuse scratch space and skip countries without source provinces, and cache completion wakes the session update wait immediately.

The core and crash-dump components update together through Steam Workshop with no proxy replacement required; restart the game after the update. Manual framework downloads now include `eu4_harmony_crash_dumps.dll` and `eu4-harmony-dump-writer.exe` in `eu4-harmony\` alongside `eu4_harmony_core.dll`; remove all three files there to return from a manual install to Workshop updates.

## 0.2.3

- Fixed: opening an army panel with attached mod elements and advancing time could end the game when a derived window used a shared native hide method. Window ordering now classifies the receiver before applying ordinary-window checks, while preserving native picker coverage and input priorities.

The core updates through Steam Workshop as usual. No Lua interface changes or proxy replacement are required for this fix; restart the game after the update.

## 0.2.2

- Fixed: creating a reformation center in a province being converted by another center now clears the old conversion references and progress, preventing stale references when the old center is removed.
- Fixed: removing color codes preserves complete UTF-8 characters, including Chinese mercenary company names when hiring armies. Names already damaged in existing saves are not restored automatically.
- Native extension windows, pop-ups and attached button areas now follow their existing window scopes and input priorities. The army attach picker can cover added button areas without covered controls intercepting its input; closing it restores access to those areas. No Lua interface changes are required.
- Fixed extension windows and button areas keep their blue backgrounds and gold borders on hover and press.
- Shorter drawing-lock holds during daily AI work reduce long gaps between submitted frames in the tested early-game view. Results vary with the game and computer; simulation throughput and interface freshness did not improve consistently.
- Troubleshooting files now include complete logs and crash dumps from all existing crash folders, with no file-size, total-size or file-count limits. Text credential redaction remains in place; collecting large archives can take more time and disk space.
- Development performance reports distinguish completed days, successful frame submissions and interface updates. Player builds contain no performance collection code.

## 0.2.1

- Merging a version PR now automatically checks the source, builds the framework and CLI packages, creates the version tag and publishes the GitHub Release. A separate manual tag push is no longer needed.
- Ordinary code merges and optional manual preparation do not publish. Existing version tags and public release attachments are never moved or overwritten.

The game runtime and the separate framework and CLI package contents are unchanged from 0.2.0. Steam Workshop upload remains a local operation.

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
