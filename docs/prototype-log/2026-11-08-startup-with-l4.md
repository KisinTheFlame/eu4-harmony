# 2026-11-08 The startup's stretches with L4 (tested by me)

Master at `74645e8c`, L4 merged, on a test game: a developer build with the test games' pipes, the test switches and `dev-call-trace` timing the startup's stretches (`0x48c940`, `0x14c02b0`, `0x202850`, `0x14dc5d0`, `0x2043a0`, `0x208620`, `0x87b9e0`, `0x7ea2c0`, `0x6f3250`, `0x10b2b70`, `0x171cdf0`, `0x206260`, `0x10a2e40`), the player's playset with every DLC, the music kept by a first start; other test games held the processor at about 70%. The stretches' times are in [research/loading.md](../research/loading.md), "The startup with L4".

- **Five starts**, alternating L4 on and left to the game (`game-paths = ["opens"]`): on, 29.8, 30.1 and 31.0 s to the main menu (the framework log's "memory at the main menu", from its bootstrap); left to the game, 36.7 and 37.9 s. The third start's `0x206260` was not yet in `call-times.tsv` when it was copied, so that start gave its other stretches only.
- **Six starts before them failed at once**: the trace named the main menu's `0x10d5bc0`, which the framework hooks itself ("main menu takes over at 0x10d5bc0 and traced function at 0x10d5bc0 change the same bytes"), and each ended with exit code 3; the release counted them as crashes. Left out after; no crash folder.
- **Not verified**: the machine otherwise idle; why the gfx and meshes took up to 1.4 s longer with L4 on.
- **Cleanup**: `dev.toml` removed; the release kept the logs; the call times deleted from the scratchpad.
