# 2026-11-10 Hidden games' main threads unpinned (tested by me)

Why four hidden games of `whole`, opted out of Windows' power throttling, still ran slowly side by side, and what leaving out the game's pin of its main thread gives ([hidden-start.md](../plans/hidden-start.md); [research/headless.md](../research/headless.md), "One game per machine"). The games were another checkout's `cargo xtask scenario whole`, four at a time, each a hidden test game with a core built from master as it was after the power throttling fix; the machine an AMD Ryzen 7 9700X, 8 cores and 16 logical processors, Windows 11 on AC power.

- **Found first**: each game's power throttling read back off (`GetProcessInformation`, `ProcessPowerThrottling`: control mask 5, state mask 0). Over 20 s, each game used 81 to 132% of a core, but its main thread only 22 to 30%, while the machine's processors were 62% busy. A 30 s sample of one game's main thread (`cargo xtask profile --threads`) found it running game code in most samples and in `Sleep` (the application frame's `SDL_Delay(0)`; the flag that makes it 100 ms, `graphics+0x530`, was read clear in none of 2.1 million reads from outside in each game) in 20%: ready to run, not waiting on anything, and given a processor a quarter of the time.
- **The cause**: every game's main thread had the affinity mask `0x100`, processor 8, and every other thread `0xffff` (`NtQueryInformationThread`, `ThreadBasicInformation`, from outside). The game's run pins its main thread to the processor numbered `count / 2 + 1` of the process's 16, the same in every game (`0x14c2d00`, read for this run).
- **Unpinned from outside** at 02:46:02, the four main threads set to `0xffff` with `SetThreadAffinityMask`, the games running on: over the next 20 s three of them used 89 to 96% of a core and the machine was 85% busy. A game year at speed 5, the scenario's `time.advance` of 365 days:

  | Stretch | Before, the last five years | After |
  | --- | --- | --- |
  | `long-1444` (the 1490s to 1500s) | 61 to 80 s | 26 to 38 s while three others ran beside it, 18 to 27 s once fewer did |
  | `long-1700` (the 1730s) | 71 to 86 s | 22 to 41 s |
  | `long-1780` (the 1780s) | 89 to 94 s | 33 to 61 s |

  The games the run started after that were unpinned from outside within 20 s of their start, by a script of this run's own, until the run ended.
- **The change**: a hidden start makes the run's `SetThreadAffinityMask` call itself only when the game was not started hidden. A test game started hidden from this branch's core, while one other test game ran, logged "hidden start: the main thread is not pinned to processor 8"; all its 102 threads, its above-normal main thread among them, had the mask `0xffff` at its main menu; a new game in observe mode ran a year at speed 5 in 30 s, and the game ended without a crash when released.
- **Not verified**: a player's game, started as always, still pinned (the change keeps the call for it, unread in the game); a run of `whole` from start to end with the change built in; other machines, and processor counts other than 16; whether anything in the game relies on its main thread staying on one processor (nothing in the run says why it pins it; the clock it reads is not looked at here), beyond the hours of play above unpinned without a crash.
- **Cleanup**: the test game released, its logs kept in its claim's `kept\`; the script stops when the run ends, and was in the scratchpad; the core's copies were in the worktree's `target\`, removed with it.
