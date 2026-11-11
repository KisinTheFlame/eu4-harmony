# 2026-11-11 Parameterized script given its arguments in one pass (tested by me)

S12 of [game-speed.md](../plans/game-speed.md), the speed-up `script-parameters` (`loading::parameters`): `0xdb1800`, which puts a use's arguments in place of a parameterized scripted trigger's or effect's parameters, replaced by one pass from the left where the parameters are in order and apart and the text has no NUL ([research/parameterized-script.md](../research/parameterized-script.md)). The branch at `7518e9b1` with the part, the release build with the test games' pipes, the test switches and the call trace (`dev-test-games`, `dev-test-switches`, `dev-call-trace`); one test game, hidden, with no other test game running beside it; every DLC, the files in the system's cache.

- **Checked** (`checked = ["script-parameters"]`), the game's function making every use's text as well and the two compared byte for byte:
  - the player's playset: 320,905 uses in a start, none differing; then the 1780.7.1 save loaded (the report written at the session's start counted the same uses, so the load made none), `console run observe` and `time advance --days 30` played, with no difference in the log;
  - the playset with a large mod added (Voltaire's Nightmare, `ugc_2623328037`), a new game in observe mode: 114,886 uses, none differing;
  - the playset with the 2000 AD mod added (`ugc_2935149060`), a new game in observe mode: 1,129,203 uses, none differing.

  The checks' count equals the uses the call trace counted (`0xdb1410`'s calls), so no use was left to the game's function by the pass's conditions.
- **Timed**: the call trace's `times` on `0xdb0860` (each use's text made: the argument check, the copy, the blocks, the parameters found again and their substitution) and `0xdb1410` (the blocks alone), a start to the main menu each way, the part on and left to the game (`game-paths = ["script-parameters"]`), taken in turn:

| Playset | | Uses | `0xdb0860`, all uses | The blocks | First use to last | To the main menu |
| --- | --- | --- | --- | --- | --- | --- |
| The player's | the game's | 320,905 | 1.251, 1.265, 1.234 s | 0.047 s | 6.57, 6.46, 6.47 s | 17, 17, 17 s |
| | the pass | | 0.249, 0.244, 0.244 s | 0.047 s | 5.55, 5.57, 5.47 s | 16, 16, 16 s |
| With Voltaire's Nightmare | the game's | 114,886 | 1.439 s | 0.019 s | 7.92 s | 23 s |
| | the pass | | 0.106 s | 0.019 s | 6.55 s | 21 s |
| With 2000 AD | the game's | 1,129,203 | 4.028 s | 0.183 s | 65.10 s | 90 s |
| | the pass | | 0.979 s | 0.164 s | 55.57 s | 83 s |

  The time to the main menu is `cargo xtask game start`'s, in whole seconds. Each timed call takes the tool's lock, about a microsecond on one thread, the same both ways.
- **What came out**: the uses' making took a fifth of its time with the player's playset, about 1 s less of the LoadingThread, which is on the start's path, and the main menu came about 1 s sooner; with the two large mods 1.3 and 3.0 s less of it, the main menu 2 and 7 s sooner, one start each.
- **Not verified**: the starts with the large mods once each way only, so their main menu's seconds are single runs; a mod whose definitions hold a `$` without its pair or a NUL, which the pass leaves to the game's function (none of the three playsets had one, or the counts would differ); how many uses, if any, the 30 days of play made: the report was not written again after them.
- **Cleanup**: `dev.toml` removed; the release kept the logs in the claim's `kept\` and said the game did not crash; the time files in the scratchpad.
