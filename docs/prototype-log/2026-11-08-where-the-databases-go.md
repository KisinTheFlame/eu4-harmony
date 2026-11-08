# 2026-11-08 Where the databases go (tested by me)

One claim of 30 minutes, the test game started hidden, eight starts of master at `05b3181f` that reached the main menu and two that failed at once, in a developer build with the test games' pipes and `dev-call-trace`, the first to keep the music; other sessions' test games held the processor at about 50% ([research/loading.md](../research/loading.md), "The databases").

- **The loaders by name**: 128 traced at once were cut to 16, the observer slots the framework leaves to the trace; so the 67 databases were timed by their memory brackets (`0x14e44d0`, `0x14e4500`) and 13 loaders without one, in two starts that reached the main menu in 36 s, the databases 13.7 and 13.8 s.
- **Running**: a start sampled with `cargo xtask profile --wait --threads --hz 100` beside the same trace, in 34 s.
- **Opening files**: a start with the game's open (`0x1718300`) and its spin lock traced, 36 s, the databases 14.2 s; one with PhysFS's lock (`0x1a05e60`) and the path's check (`0x19fb700`) traced, 40 s, the databases 15.8 s, slowed by 5.5 million traced takes. Two starts failed at once, since the framework hooks the functions they traced too: PhysFS's open (`0x19f7f60`) and its stat (`0x1a061d0`).
- **Not verified**: which thread holds the open's lock while the LoadingThread waits on it; the databases with the checksum or the picture checks' lookups out of the way; the machine otherwise idle.
- **Cleanup**: `dev.toml` removed; the release kept the logs; the call times and profiles deleted from the scratchpad.
