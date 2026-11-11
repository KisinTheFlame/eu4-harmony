# 2026-11-11: The crash record and the troubleshooting files

Date: 2026-11-11. Build: this checkout's release build with `dev-test-games` and `dev-test-switches`, on master at `11b7f09f` with the crash record and the About page added. What the crash record joins in the game is in [research/crash-handling.md](../research/crash-handling.md).

## What was tried

A test game of its own (`crash-record`), hidden, with the player's playset (six mods, Regional Combat among them) and `dev.toml`'s `[test-switches] crash = true`, which reads address 8 in the framework's frame at the first frame of a game. Requests sent through `eu4-harmony game`.

- **The About page.** At the main menu, a click on the corner button opened the settings window, which listed 关于 (About) last in its side list and no longer had the version line at its foot. The About page showed "Harmony 0.1.0", the line to report a problem with the issues' address, and the button 把排查问题用的文件存到桌面 (save the troubleshooting files to the desktop).
- **The troubleshooting files.** A click on the button: within four seconds the line under it said 已存到桌面：eu4-harmony-troubleshooting-20261111-210104.zip, and the file was on the desktop, 7.5 MB. Windows' `Expand-Archive` unpacked it: `about.txt`, the framework's logs, the game's logs, the last crash folder, the three settings files.
- **A crash.** `menu.start --country FRA`; five seconds later the game crashed, and its `core.log` ended:

  ```text
  21:01:50.335 tid=46384 test switch crash: crashing the game
  21:01:50.335 tid=46384 crash: Unhandled exception: 0xc0000005, an access violation reading 0x8 at eu4_harmony_core.dll+0x3a2994, on the main thread (46384), the game at 1444.11.11
  21:01:50.335 tid=46384 crash: in the calls eu4_harmony_core.dll+0x3a2994 < eu4_harmony_core.dll+0x1edfa1 < eu4_harmony_core.dll+0x1ee263 < eu4_harmony_core.dll+0x131ee6 < eu4_harmony_core.dll+0x8703bb < eu4_harmony_core.dll+0x870631 < 0x14c2818 < eu4_harmony_core.dll+0x8703a3 < eu4_harmony_core.dll+0x870631 < 0x14c2e18 < 0x8781b9 < 0x1a7889a < 0x1a3375a < KERNEL32.DLL+0x2ccb7 < ntdll.dll+0xaad6c
  ```

  The game's own crash folder (`eu4_20261111_210150`) was written as before: its `exception.txt` gives the same address (`…72994`) and, frame for frame, the same modules in the same order.
- **The troubleshooting files after the crash**, in the game started again: the zip held that crash folder, and the framework's earlier log (`core.prev.log`) with the two `crash:` lines.

## What it showed

- **The first packing read the wrong user folder.** `about.txt` named `Documents\Paradox Interactive\Europa Universalis IV`, the player's folder, while the test game runs with `-userdir` set to its claim's: the packing took the user folder as `userdir.txt` names it and left out the command line. Fixed before the second packing, which named the claim's folder; that first zip was moved off the desktop and deleted.
- The record costs nothing while nothing crashes: one observed function, which runs only in the game's crash handling.

## What was not verified

- A stack overflow, a pure virtual function's call and `abort`: none was brought about. That the record joins them too is read from the code alone.
- A crash on a thread other than the main thread, and one inside the game's code rather than the framework's: the test switch crashes in the framework, on the main thread.
- The troubleshooting files with a large `error.log` (the player's of this morning was 60 MB): how long the packing takes then was not timed.
- Any language but Chinese on the About page.
