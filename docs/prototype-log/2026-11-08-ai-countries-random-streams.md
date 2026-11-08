# 2026-11-08 The AI countries' random streams (tested by me)

The repair that gives each AI country's task a stream of random numbers of its own instead of the world's one generator ([research/random.md](../research/random.md), `core/src/repairs/ai_draws.rs`). The developer build with the test switches and the test games' pipes of `claude/ai-random-streams`; `dev.toml` with `game-shares-ai-draws` off (the repair on) or on (the game's draws). On a test game: the 1780.7.1 save copied into its `save games`, `menu.load`, `console run observe`, `session.view TUR`, speed 5. One claim of 40 minutes, the game hidden.

- **First start**: startup refused the build, the game ended at its first AI phase with "the framework used 0x23581c0, which no dependency point checks": the `-seed` byte was read with no point checking it. The draw site at `0xe9fd3` now says it uses `0x23581c0`, the world's pointer and `0xb90`, and the task's read of its tag at `0xb1028` the offset `0x20`.
- **Second start**: the game crashed on the first day, an integer divide by zero in `0x1349f2` (an AI agent's day, under `0xb0fa0` → `0x14b640` → `0xe48bc` → `0x13e078` → `0x13c941`), its divisor `x % 2 + 1` of a drawn `x`: the streams gave all 32 bits, and the game's twister gives numbers below 2^31, which its callers take as non-negative. The streams now give their top 31 bits.
- **Third start**, the repair on: 120 days, 1780.7.1 to 1780.10.29, no crash. The log at 100 stages: 11,500 countries' tasks, 307,678 numbers drawn from their streams (3,077 a stage), none outside a task.
- **Measured**, four starts in turn, `cargo xtask profile --seconds 180 --threads --waiting-in 0x179e90`, the main thread's wait in the AI stage in ms a game day:

  | Run | Days a second | A day | The AI stage | Its wait |
  | --- | --- | --- | --- | --- |
  | Streams, first | 5.79 | 172.7 ms | 23.0 ms | 14.1 ms |
  | The game's draws, first | 4.73 | 211.4 ms | 20.5 ms | 11.1 ms |
  | Streams, second | 5.20 | 192.3 ms | 19.6 ms | 11.2 ms |
  | The game's draws, second | 5.90 | 169.5 ms | 20.3 ms | 12.0 ms |

  No difference beyond the runs' spread. Another session's test game ran its scenarios all through the runs, so that a day took 169 to 211 ms against 67 alone, and the sampler held the main thread 0.8 ms a sample against 0.14: the runs measure under that load, not the game alone.
- **Not verified**: the stage's time on a computer running nothing else; a game started with `-seed` (read from the code to draw nothing from the world's generator); whether any agent draws by an inlined copy of the twister rather than by `0x14c71b0`.
- **Cleanup**: `dev.toml` removed; the release removed the test game's folder but `kept\`, with the second start's crash folder in it.
