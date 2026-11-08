# 2026-11-08 Sets of modifiers added through an index (tested by me)

The developer build with the test switches and the test games' pipes of `claude/modifier-sums`, in a test game (`cargo xtask game claim`, the 1780.7.1 save put in its own user folder), observe mode at speed 5, 120 s a run; four claims of 4 to 15 minutes, the game hidden. What came of it is in [research/late-game.md](../research/late-game.md), "Adding sets of modifiers".

- **First start**: the test game ended before its main menu with `0xc0000005`, and no crash folder: the stubs handed the framework the stub's own frame for the set (`mov rcx, rbp` after `call_rust!` has set `rbp`), not the game's `rbp`, which it saves at `[rbp]`. The release then said the game did not crash, though the start had reported the exit code.
- **Check**: with `check-modifier-sums`, 63 million searches through the index, none differing from the game's.
- **Measured**: `0x8ea420`'s own code 101 and 107 ms of the main thread a game month with the game's code (`game-sums-modifiers` leaving the framework's points out), 16 and 20 with the index, and 22 and 24 in the framework's search. A first pair of runs had `game-sums-modifiers` search pair by pair in the framework's code, behind the stub, where the sampler cannot follow; the comparison was made again with the game's own code.
- **Observe mode**: `console run --command observe` begins a new session, and a `time.advance` sent before it began was refused (`session_changed`), so that three runs of four did not advance; the runs wait for the session to be observing.
- **Not verified**: other saves and years; the build without the test switches in the game (built and checked offline).
- **Cleanup**: `dev.toml` removed; the releases removed the test games' folders, their logs kept; the profiles deleted from the scratchpad.
