# 2026-11-08 The deepening, all its parts merged (tested by me)

Every part of [deepening.md](../deepening.md) together, on `claude/improve-codebase-architecture-80be67` at `8e244bc3`, with master up to `630fa045` merged in. Test games of this checkout's, every game started hidden; the player's saves `England-1`, `英格兰1444_11_11` and `Austria-1`, copied into the test games.

- **Checks**: `cargo fmt --check`, the release build, `cargo test --release` for the workspace and the core's tests with `--features dev`, both clippy runs, and `cargo xtask offline-check`, which now checks the developer build's points too, all passed. The core's tests passed three times in a row.
- **`cargo xtask scenario perf growth leak --growth.rounds 2 --leak.rounds 2`**: one claim of 12 minutes, each scenario passed, every game ended with exit code 1, no crash folder, and no fatal error or panic in the framework's logs.
  - growth: private bytes +43.0 MB a round, the game's heap +1.03 MB a round.
  - leak: the game's heap +1.45 MB a round; the leftovers were not compared site by site with the earlier runs.
  - perf: 784 game days a minute at speed 5, which measured nothing: the probe's test game below was started beside it during its run.
- **`cargo xtask scenario perf` alone**, no other game running: passed, 42.3 s to the main menu, 1084.4 game days a minute at speed 5 (1082 in the test games' own run, [2026-11-08-test-games.md](2026-11-08-test-games.md)).
- **One frame's dispatch, in the game** (D3): a probe mod in a second test game, added to that test game's copy of the playset beside the player's `regional-combat`; both extensions loaded. `Austria-1` loaded with `menu.load`, `time.advance` for 125 days (it stopped at 1618.11.10), `menu.return`, and `Austria-1` loaded again. The probe's log had, in order: `on_session` with no saved data, the first `on_day`, then `on_frame` twice; a day each day, `on_month` at 1618.10.1 and 1618.11.1; `on_session_ended` with `return`; and `on_session` again with the data its `save_data` had given at the return. The release found no crash and nothing in the core's log.

Not verified: `on_year` in the game (the advance stopped before the year turned; the fake game's tests cover it); a notice shown, since a hidden game sends notices only to the log; a panic of framework code reaching the notice through the guarded seam; each speed-up left to the game on its own; `eu4-harmony reload` with its typed request; the quarantine with the letting go in one module.
