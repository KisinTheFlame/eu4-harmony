# Safe points (for `safe_point` and remote control)

Baseline 1.37.5. All addresses are RVAs. What was checked in the running game is marked "tested"; the runs are in [prototype-log.md](../prototype-log.md) (2026-10-29, remote control F2 and F3).

## Where the main thread comes by

- **In a game**: the map manager's frame `0x8137c0` runs once a frame, also while paused (docs/research/regional-combat/native-scheduler-findings.txt); extensions and remote control run after it (`game::events`).
- **Every frame the main thread draws** ends in the Present wrapper `0x16d4db0` (`framework_ui::hook`), in the main menu and in a game. While a save loads, another thread draws and presents the loading screen, and the main thread presents nothing until the load is over. Tested: a request waiting 2 s for an event, submitted as a new game began to load, was answered after 8.9 s, at the first frame after the load.
- **The main menu's frame** `0x10d2e80` (rcx the main menu object, vtable `0x1d5a8c8`; notice-window.md) runs on every page of the main menu, the single player setup with its map included. Tested: with a country picked in the single player setup, remote control answered "the main menu is open".
- **Loading a save from the main menu**: the save is read first (the save observer, `saves`), then the main menu's frame still runs at least once, then the main thread presents nothing for about 5 s, until the first game frame. Tested twice, with the continue box: a `session` submitted as the box was clicked was answered "the main menu is open" 0.5 s and 1.0 s after the save was read, and the next request at the first frame after the gap. "The main menu is open" right after a load began so does not say the load failed; loading a save from remote control (C5) has to tell a load under way from the main menu.
- **During an in-game menu** (Esc) and the end-of-game summary, the map manager's frame keeps running. Tested: the session was read with the in-game menu open.

## What is no sign of a game

- **The world** (`[0x233fe78]`) is not null in the main menu, which shows the map behind its buttons. Tested: when requests that need a game were answered "not in game" only at a frame with no world (or at a game frame failing for a reason other than "not ready", which also reads the world first), they were never answered in the main menu or the single player setup, while requests that need no game were.

## Returning to the main menu

- **"Return to main menu" starts `eu4.exe` again.** Tested: after confirming it and leaving the end-of-game summary, the framework bootstrapped anew (a new main thread, a new framework log, remote control's request numbers from 1) and the main menu came up after the usual loading screen. What a process keeps, such as remote control's events, starts again empty.
- **The new process starts before the old one has ended.** Tested: the new framework started while the old process still had remote control's named pipe open; the new one opened the pipe 3 s after its startup, at the fourth try. A client waiting on the old process's pipe found it broken about 4 s after the summary's button to leave was clicked.
