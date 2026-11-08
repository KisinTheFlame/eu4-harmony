# 2026-11-03 The capability catalogue through remote control (tested by me)

L1 of [lua-extensions.md](../plans/lua-extensions.md): remote control under the catalogue's names and shapes. Two runs of `eu4.exe`, started directly with the player's playset and framework settings, driven by `eu4-harmony game` from a script: the main menu, `menu.start CAS`, the capabilities, `menu.return`. The game was claimed with the game-lock skill, which backed up the player's files first.

- **Startup**: 707 dependency points match; 120 hooks, 10 writes. No crash.
- **Answered as the catalogue has it**:
  - `describe`: `api_version` 0.2 and every capability, each saying whether extensions have it.
  - `session.get`: `number` 1, `player` CAS, `date` 1444.11.11; `time.get` paused at speed 1; `time.set_speed 3` done.
  - `army.list` and `army.get`: two armies (`kind` army, `blocked` empty) and two navies (`kind` navy, no province). After `army.move` to the neighbor 2754 its `route` was `[2754]` with `route_by` `player_or_game`; `army.stop` done.
  - `army.can_step` 219 to 2754 true; `army.route_to` 2754 `[2754]`.
  - `province.list`: 4027 land, 668 sea, 125 lake, 121 wasteland. `province.get` 219 (托雷多) without recruit options; `recruit.options` 219: infantry and cavalry allowed, the rest not, every kind with its unit type.
  - `camera.look_at` with a province and with x and y, each answered with the camera afterwards; with both, `invalid_argument`.
  - Right after the start, before a day had passed, `army.at` and `army.can_attack` answered `not_ready` (visibility not current yet), as described. After `time.advance 3`: `army.at 219` the player's army there; a Navarrese army found in 210, and `army.can_attack` it false (no war); the army itself false; army 1 `not_found`, "the player sees no army 1".
  - Events: `session` (with `session`, the session's number), `day`, `army_ordered`, and after `menu.return` from the same process, `session_ended` with `reason` `return`.
- **Found and fixed between the runs**: the session event's own field was first named `number`, the name of the event's number beside it, so the JSON showed one `number`. It is `session` again, and a test refuses an event field named `number`.
- **Seen, not caused by this change**: after `menu.return`, `menu: a new game as CAS failed: the game did not start it: the main menu is open` in the framework log, in both runs and with master's core too: the earlier start's attempt is followed again at the returned main menu's frames, which also writes the main menu's continuing flag.
- **Not verified in the game**: `army.can_attack` answering true (no war was declared); how long its search of every province takes; `route_by` `this_mod` and `other_mod`, which need an extension; `recruit.order`, `diplomacy.declare_war` with allies as a list, `question.*` and `offer.*` under the new names; `eu4-harmony wiki`, which needs no game.
- **Cleanup**: each game was closed by ending its process. The three framework files in the game folder were put back and compared equal to the copies taken before each run. Releasing the game restored the player's files from the claim's backup.
