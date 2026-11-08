# 2026-11-08 The databases one by one, and the meshes, with L4 (tested by me)

Master at `bc2731e1`, L4 merged, on a test game: a developer build with the test games' pipes and `dev-call-trace`, the player's playset (five workshop mods and the local Regional Combat), every DLC, the music kept by a first start; other test games held the processor at 50 to 70%. What came out is in [research/loading.md](../research/loading.md), "The databases with L4" and "The meshes".

- **The databases by their brackets**: `0x14e44d0`, `0x14e4500` and 12 loaders without a bracket traced, in two starts (34 and 35 s to the main menu) and a third sampled at the same time with `cargo xtask profile --wait --threads --hz 100` (30 s): the databases 9.5, 9.8 and 8.5 s. The brackets came in the code's order; the history's opening one was missing from each start's `call-times.tsv`.
- **The picture checks**: a start with `0x8f0350`, `0x17139c0`, `0x19fad70`, `0x8effc0` and `0x1718300` traced (39 s): the databases' end waited 4.75 s in `0x8f0350`; the checks' thread made 16,031 lookups, 11.3 s in all.
- **A lookup's parts**: a start with `0x8d80d0`, `0x19fad70`, `0x19fb700`, `0x1a07950`, `0x1a07080` and `0x19f9280` traced (38 s), 6 million calls and a file of 1 GB: on the checks' thread 139 entries checked a lookup, 20 folder stats at 29 µs and 232 zip stats at 0.5 µs.
- Three starts were refused by `game start`: a `cargo xtask disasm` run between had built the core again without the test games' pipes; built again before each run after.
- **Not verified**: which folders the stats that reach Windows are for; what a mesh's load writes; the machine otherwise idle.
- **Cleanup**: `dev.toml` removed; the release kept the logs and said no game crashed; the call times and profiles deleted from the scratchpad.
