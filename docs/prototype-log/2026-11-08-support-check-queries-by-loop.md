# 2026-11-08 The support check's route queries, by loop (tested by me)

Before a prototype of the support check (`0x106320`) cut short: its code counts the friends first and the enemies after, the enemies' sum the bar, so which of its loops its route queries come from says what a cut could save ([research/ai-stage-wait.md](../research/ai-stage-wait.md), "The support check"). Master at `74645e8c`, the release build with the test games' pipes and the call trace; `dev.toml` tracing `0x106320:8` and `0x96c520:8` with `times` and `calls = 60000`. On a test game: the 1780.7.1 save copied into its `save games`, `menu.load`, `console run observe`, `session.view TUR`, `time advance --days 2600 --speed 5`, to 1787.8.15.

- The log's first 60,000 route queries (to 1783.3.16) each with its call site: `0x106723` the country's own units, `0x106c42` its war allies', `0x107199` the enemies'; joined to the time file by thread and order, and to the check they ran inside, whose answer the log held.
- 19,918 checks: 19,487 yes, 431 no. Of their queries' time: in yes checks the own units' 31.9%, the allies' 9.1%, the enemies' 50.1%; in no checks 0.3%, 0.4% and 8.1%.
- **Not verified**: where in a yes check the friends' sum passed the enemies'; the years after 1783.
- **Cleanup**: `dev.toml` removed; the release removed the test game's folder but `kept\` and said the game did not crash; the time file and the log lines in the scratchpad.
