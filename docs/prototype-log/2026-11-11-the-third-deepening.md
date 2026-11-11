# 2026-11-11 The third deepening, all its parts merged (tested by me)

Every part of [third-deepening.md](../plans/third-deepening.md) together, on `claude/improve-codebase-architecture-eb5578` at `564de0d8`, with master at `ce49e723`, and again with master at `b9a5e757` merged (below). One test game of this checkout's, every game in it started hidden, with the player's playset (the Chinese one, which holds regional-combat from `D:\Workspace\eu4-regional-combat\mod`) and the player's save `Austria-1` (Spain, 1619.8.10, ironman) copied in.

- **Checks**: `cargo fmt --check`, the release build, `cargo test` for the workspace (the core's 660 passed, 5 ignored), both clippy runs and `cargo xtask offline-check` (5 of 5) all passed. regional-combat's own tests, `eu4-harmony test` in its folder with this build's command line tool, passed, 50 of 50, against the fake game that now keeps windows and pop-ups by the core's rules (F2).
- **By hand**, with `Austria-1` loaded by `menu.load`:
  - `leader.list` read Spain's leaders, each general with the army it leads (F5's read of the list through `containers`). `leader.remove` took Filips Willem Brouwer from his army and `leader.assign` gave him back, each seen in `leader.list` a day later: both commands, written by the command layer's writer with the unit's `{ id= type= }` (F4), were taken by the game.
  - The army selected with `camera.look_at` and an `input.click` on it (the owner allowed clicking by screen position for this), regional-combat's toggle showed in the button area beside the single army panel with its text, 区域, and its tooltip, filled where the game's was empty (F3's bound element in a vanilla window). A left click turned the army's task on (the mod's log: `task on: center 1215 radius 7`).
  - A right click on the toggle opened the mod's settings window, 区域战斗, with 半径 5 and the map preview painted (F2's state, F3's bound elements in an extension window); its right arrow raised the radius to 6, then 7, the text changing at once; 关闭 closed it.
  - An in-game load with the settings window open left no window open in the session it began (F2: a session's end closes every window).
- **`cargo xtask scenario sweep sessions extensions late perf`**, in the same claim, from 13:56 to 14:23; every scenario passed, and every game of the claim ended with exit code 1, no crash.

  | Scenario | Outcome | Minutes |
  | --- | --- | --- |
  | sweep | passed: 285 capabilities in 5 places, 3,656 requests, every answer of its shape or a refusal the catalogue has | 5.4 |
  | sessions | passed: 10 rounds, 74 to 77 s | 13.1 |
  | extensions | passed: `qa-steady` heard all 271 dates, its count unbroken through the autosave's load and 10 reloads, its save data got back 11 times; `qa-looping` stopped by the time limit 4 times, then for good; `qa-hungry` refused memory 9 times, never stopped | 1.0 |
  | late | passed: five late saves observed, 895 to 2,161 game days a minute, private bytes 4.2 to 4.4 GB | 3.4 |
  | perf | passed: the main menu after 16.7 s, `menu.start` 4.0 s, 1,043 game days a minute at speed 5 | 3.4 |

- **The drawing thread** (F1): the core's log gave 153.5 frames a second while the settings window was used, none drawn by the main thread.
- **Again, with master merged** (`b9a5e757`, #164, whose new commands and list walks were moved onto the writer and `containers` in the merge), at `e07fff16`, in a second claim from 14:47: `sweep sessions extensions late perf` all passed, no crash. The sweep asked 391 capabilities in 5 places, 5,005 requests, every answer of its shape or a refusal the catalogue has; sessions' rounds took 74 to 87 s; `qa-steady` heard all 271 dates, `qa-looping` was stopped for good. perf gave 810 game days a minute, and 841 played again alone, with the main menu after 26 to 28 s; master's own build, played straight after on the same computer, gave 692 and 40 s. The computer was busier than in the first claim (1,043 and 16.7 s), so these figures say only that this work is not slower than master.

Seen, and not of this work:

- **A click's modifiers from remote control are lost.** A shift click given by `input.click --modifiers shift` on the settings window's arrow raised the radius by 1, not 5. The framework reads a click's modifiers with `GetKeyState` as the click comes (`ui::modifiers`), which reflects the keys the window's message queue has handed out, while remote control's input reaches the window procedure directly (`game::input::window`); the game itself reads no key state ([research/input.md](../research/input.md)). It was the same before this work: the reading of the modifiers moved into `bound_element` unchanged.
- **The in-game menu stays open after `menu.load` in a game**: after each in-game load of the ironman save, the game's Esc menu was open over the map. The same build of master (`ce49e723`), started in the same test game, did the same.

Not verified: a pop-up of an extension's in the game (regional-combat shows none; the sweep asked `popup.list` and `popup.answer` with none open); the 32 pop-ups' limit and the refusals outside a session in the game rather than the fake game; an icon spliced into a vanilla window, whose show and hide methods are now checked as an extension window's are (F3): no mod in the playset splices one; the armies aboard a fleet and a defeated side's units read through `containers` (F5), reached only by the sweep's and the scenarios' own play; the write watch's hold through Present (F1), a developer build's tool, not built in; `long`, `checked`, `growth` and the leak run were not played.
