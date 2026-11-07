# 2026-11-08 What the main thread waits on, sampled (tested by me)

`cargo xtask profile` sharing out the main thread's waits among the threads that ran ([dev-tools.md](../dev-tools.md)), with the player build of master at `c201eac0` installed; one claim of 4 minutes, the game started hidden.

- **Run**: the 1780.7.1 save loaded, observe mode, speed 5, `--threads --waiting-in 0x179e90 --slice 60` for 180 s: 2,935 days, 16.3 a second; 8,503 samples of the main thread (0.094 ms each) and 443,042 of the other threads (0.023 ms each). Half of what the main thread waited on in the AI stage was the armies' searches ([research/late-game.md](../research/late-game.md), "What the main thread waits on in the AI stage").
- **Not verified**: how near the shared-out wait comes to the stage's longest task; other years and saves; a vanilla game.
- **Cleanup**: the release put back the player's files and found the game folder's framework files as they were; the profile's files deleted from the scratchpad.
