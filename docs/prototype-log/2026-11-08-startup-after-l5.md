# 2026-11-08 The startup's stretches after L5, timed and sampled (tested by me)

Master at `7a712563`, L5 merged, on a test game: a developer build with the test games' pipes and `dev-call-trace` timing the stretches (`0x48c940`, `0x14c02b0`, `0x202850`, `0x14dc5d0`, `0x2043a0`, `0x208620`, `0x6f3250`, `0x10b2b70`, `0x44f820`, `0xb77f50`, `0xb9b4e0`, `0x171cdf0`, `0x206260`, `0x10a2e40`), the player's playset with every DLC. Two other sessions' test games ran beside it throughout; the processor's load was not measured. What the run found is in [research/loading.md](../research/loading.md), "The startup after L5, broken down".

- **Five starts.** The first, with `0x76d260` and `0xb67830` traced as well, ended at `0xb67830` (its call logged, no return, a crash folder); those two were left out after. The second read the music and kept it, and reached the main menu in 29 s. The next three reached it in 26, 26 and 27 s as `game start` counts from the game's start; the third was sampled as well (`cargo xtask profile --wait --threads --hz 100 --slice 1`, 85 samples a second).
- **The trace's own cost**: `0xb9b4e0` was called 2,345,254 times in each start, and its 1.1 s includes what timing each call costs.
- **The framework's glyphs**: 4,239, 4,234 and 4,255 drawn in 2.1, 2.0 and 2.6 s (the core log's "unicode costs"), against 0.87 to 0.98 s in [the startup with L4](2026-11-08-startup-with-l4.md).
- **Not verified**: the machine otherwise idle; why the glyphs took twice as long as in the run with L4.
- **Cleanup**: `dev.toml` removed; the release kept the logs and the crash folder; the call times deleted from the scratchpad.
