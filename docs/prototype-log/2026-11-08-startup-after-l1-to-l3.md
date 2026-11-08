# 2026-11-08 Where the startup goes after L1 to L3 (tested by me)

One claim of 26 minutes, the test game started hidden, five starts of master at `05b3181f`: the player build with the test games' pipes, then a developer build with `dev-call-trace` timing the startup's stretches ([research/loading.md](../research/loading.md), "The startup after L1 to L3"); other sessions' test games ran beside it, the processor 40 to 96% busy.

- **The player build**: the first start, the claim's user folder without the music kept, reached the main menu in 61 s; the second, sampled with `cargo xtask profile --wait --threads`, in 51 s with the processor 96% busy, and the sampler lost whole stretches of 5 s, so that run gave only the threads' stacks.
- **The developer build**: two starts in 42 and 38 s; the stretches' times are in research/loading.md. A first start that traced the triggers' optimization profile `0xb9b4e0` too crashed in it, the observer forwarding fewer arguments than it reads; left out after.
- **Glyphs** outside the game, timed in a test that was not kept: DirectWrite and the outline for the map font and the body font (research/loading.md).
- **Not verified**: what the LoadingThread waits on in the kernel; the startup with the machine otherwise idle; vanilla.
- **Cleanup**: `dev.toml` removed; the release kept the logs and the crash folder of the traced start; the profiles deleted from the scratchpad.
