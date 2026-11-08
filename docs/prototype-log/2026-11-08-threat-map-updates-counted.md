# 2026-11-08 The threat map's updates for an army, counted (tested by me)

Whether each country's daily update updates an army's place on the AI's threat map twice on a day it moves, once when it arrives (`0xd3fd30`) and once after its day (`0x1a2d30`) ([research/country-daily-update.md](../research/country-daily-update.md)). An experiment build of master at `09acf8dc`, not kept: the release build with the test games' pipes and `0x195e10` replaced by a wrapper that timed each call and counted it by army and game day, noting for an army updated again the same day whether its province (`+0xe8`), its route's length (`+0x110`) or its route's first node changed since. On a test game: the 1780.7.1 save copied into its `save games`, `menu.load`, `console run observe`, `session.view TUR`, `time advance --days 2600 --speed 5`, to 1787.8.15.

- 86,928 updates in 2,500 days, 34.8 a day, 100 µs each, 3.49 ms a day, all on the main thread.
- 1,445 were of an army updated already that day (1.7%, 0.05 ms a day); 76 of those had moved since, 8 had another route.
- **Not verified**: the wrapper's own cost in the 100 µs; which of the two callers made each update.
- **Cleanup**: the experiment's code removed from the checkout; the release removed the test game's folder but `kept\` and said the game did not crash.
