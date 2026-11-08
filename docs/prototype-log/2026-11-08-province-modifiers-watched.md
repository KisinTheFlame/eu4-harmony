# 2026-11-08 What a province's modifiers read and write (tested by me)

Whether recounting one province's modifiers (`0x9f9500`) writes anything another province's recount writes or reads, for running the recounts on several threads ([research/monthly-update.md](../research/monthly-update.md)). Master at `0ade2650`, the release build with the test games' pipes and the write watch; `dev.toml` watching `0xa0b500`'s writes with `reads-before = ["0x9f9500"]`, `calls = 2`, `after-days = 3`, `give-up-after = 20`. On a test game: the 1780.7.1 save copied into its `save games`, `menu.load`, `console run observe`, `session.view TUR`, `time advance --days 40 --speed 5`.

- **Two rounds**, each a call of `0x9f9500` read and written (95 other threads held, 2,164 MB of pages inaccessible, set up in 4.4 s), then a call of `0xa0b500` written. Both recounts were made as an army arrived in a province (`0x1a2d43` < `0xd3e657` < `0xd5345b` < `0xd404bf`), not by the month.
- 17,730 and 17,652 reads and writes, in 7,326 and 7,167 groups; 1,069 blocks of 59 and 61 sizes read, and 73 statics. The writes: four fields of the province, one block of 0x2870 or 0x1438 bytes, a few small blocks, blocks made in the call; no static, no country.
- `0xa0b500` wrote 37 times, none to what the recount had read; it was another province's call, not the one after the recount.
- **Not verified**: a recount made by the month; what the recount read beyond the 300 largest groups the report lists.
- **Cleanup**: `dev.toml` removed; the release removed the test game's folder but `kept\` and said the game did not crash.
