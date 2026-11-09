# 2026-11-09 The second deepening, all its parts merged (tested by me)

Every part of [second-deepening.md](../plans/second-deepening.md) together, on `claude/improve-codebase-architecture-4b8160` at `df324843`, with master at `25425252`. One test game of this checkout's, every game in it started hidden, with the player's playset (the Chinese one) and the player's saves copied in.

- **Checks**: `cargo fmt --check`, the release build, `cargo test --release` for the workspace (the core's 633 passed, 5 ignored), both clippy runs and `cargo xtask offline-check` (5 of 5 in both builds) all passed.
- **By hand**, in a test game with the save `Austria-1` (Spain, 1619.8.10) loaded by `menu.load`:
  - `diplomacy.send_gift --country POR --amount 50` answered `done`, and Spain's treasury went from 41,172.763 to 41,122.763 the next day: the amount, filled into the act's details by its name (E1), reached the game's window.
  - `country.get` gave stability 3.0, prestige 100.0 and the treasury in ducats, scaled where the layer reads them (E6); `country.budget` gave each expense in ducats.
  - `army.get` read an army in 拉贾布尔 (4429) with its 32 regiments; `army.move` to 达布尔 (530), Spain's own, set its route by land (`route_by` `player_or_game`); a move into Vijayanagar's 果阿 and 戈尔哈布尔, with no military access, was refused as `no route`, and an unknown province as `no province 99999` (E3, E6).
  - `building.build` of a marketplace in 达布尔 and, ten days later, `building.options` gave it as constructing, progress 0.029, finished 1620.7.11, builder SPI (E7's binding of the buildings group).
  - `console.run` answered at the next safe point with the game's own refusal in ironman.
  - The core's log: at the main menu the pages of `standard_font` (768×768 to 768×1536, for 7 fonts) and of the map's `tahoma_60` (2560×2560 to 2560×5120) grew, then `standard_22` in the game, and the names on the map were built again once, in 47.9 ms, after the save was loaded and not before (E2). No page kept its size or was full.
- **`cargo xtask scenario sweep sessions extensions late perf`**, in the same claim, from 17:20 to 17:54; every scenario passed and every game ended with exit code 1, no crash.

  | Scenario | Outcome | Minutes |
  | --- | --- | --- |
  | sweep | passed: 283 capabilities in 5 places, 3,622 requests, every answer of its shape or a refusal the catalogue has | 5.3 |
  | sessions | passed: 10 rounds, 79 to 93 s, the last three 133 to 148 s with another checkout's test game playing `whole` beside it | 17.3 |
  | extensions | passed: `qa-steady` heard all 271 dates, its count unbroken through the autosave's load and 10 reloads, its save data got back 11 times; `qa-looping` stopped by the time limit 4 times, then for good; `qa-hungry` refused memory 9 times, never stopped | 1.1 |
  | late | passed: five late saves observed, 680 to 2,130 game days a minute, private bytes 4.2 to 4.4 GB | 4.8 |
  | perf | passed: the main menu after 23.2 s, `menu.start` 9.4 s, 511 game days a minute at speed 5 | 3.7 |

Not verified: perf's figure measured nothing, as another test game ran beside it; `long` was not played, nor the leak run. The panel acts with a country among their details (`subsidize_war` with a duration, `sell_province` with a price, `offer_condottieri` with an army) were reached by the sweep only, which checks the answer's shape, not that the details arrive; `send_gift` is the one act whose details were seen in the game. An input's hold on the queue (E4) was exercised by the sweep's and the scenarios' own input only, not looked at on its own. The developer build's `grow-pages` test switch was not tried.
