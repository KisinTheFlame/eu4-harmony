# Speed-ups preserve the game's answers

Every speed-up in the framework must preserve what the game computes: the same rules, data, AI decisions and simulation answers as unmodified 1.37.5, including inside the day and inside the AI. Speed-ups are not allowed to introduce behavioral differences by re-ordering dependent work, reading stale state speculatively, or simplifying simulation or AI logic.

This supersedes [ADR the-game-may-be-made-faster-with-reasoned-differences](2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md). Unlike the earlier [ADR making-the-game-faster-leaves-what-it-computes-as-it-is](2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md), speed-ups inside the day and the AI remain allowed — provided they compute the exact same answers as the game:

- **Exact equivalence**: caching invariant queries within a verified read-only scope (such as `army-checks`), exact pruning or branch-and-bound that cannot change the outcome (such as `support-checks`), zero-allocation or indexed replacements (such as `modifier-sums`, `lookups`, `token-lookup`, `localisation`), and parallelization of work whose reads and writes are provably independent (such as `parallel-saves`).
- **Verification**: every speed-up whose computation can be compared against the game's own code provides a `checked` mode in developer builds that compares the two answers call by call. `cargo xtask scenario checked` defaults to zero tolerated differences (`may_differ = ""`). Speed-ups outside simulation outcomes (drawing concurrency, save compression, startup caching) establish equivalence from native read/write analysis and end-to-end checks.
- **Boundary with repairs and player settings**: isolating per-country random streams in the parallel AI stage (`core/src/repairs/ai_draws.rs`, [ADR random-draws-may-change-hands](2026-11-08-random-draws-may-change-hands.md)) is a repair for vanilla's unlocked multi-threaded race on the shared world generator, not a speed-up trading accuracy for speed. Player-chosen presentation settings (such as frame rate limits or drawing-thread pacing) affect presentation only, not simulation state.

## Context

Under `ADR the-game-may-be-made-faster-with-reasoned-differences`, speculative background army searches (`army-searches`) were added as the first speed-up allowed to differ in timing or order. Once exact AI speed-ups (`army-checks` and `support-checks`) landed, re-benchmarking `army-searches` showed only +3.7% mean throughput in 1800, +6.7% in 1780, and no gain in 1444, while remaining the sole source of differing answers in `checked` mode (4–7% of checked searches differed; [performance evidence](../research/performance-evidence.md#parallel-army-searches-removed)). Meanwhile, exact AI speed-ups achieved larger gains (+6.5% in 1800) with tens of millions of checked answers and zero differences.

Keeping behavioral equivalence as a hard invariant gives players and mod authors a clear guarantee that Harmony runs the same game faster, and keeps `checked` mode an unambiguous bug detector rather than a noisy list of tolerated divergences.

## Considered Options

- **Continue allowing "when or in what order" and "simplification" speed-ups** (`ADR the-game-may-be-made-faster-with-reasoned-differences`): permits speculative or approximate speed-ups, but erodes the guarantee that Harmony matches vanilla gameplay and complicates distinguishing real regressions from tolerated differences.
- **Forbid all inside-the-day speed-ups** (`ADR making-the-game-faster-leaves-what-it-computes-as-it-is`): overly restrictive, because exact inside-the-day and AI speed-ups (`army-checks`, `support-checks`, `modifier-sums`) yield substantial gains while matching the game answer-for-answer.
- **Allow inside-the-day speed-ups only when they preserve exact answers** (chosen): captures the practical gains inside the day without introducing vanilla-divergent behavior.

## Consequences

- Future speed-ups must argue and verify exact equivalence from the game's reads, writes and shared state; speculative prefetching over mutable state, order changes among dependent agents, and coarser AI simplifications are out of scope.
- `army-searches` is removed, leaving zero speed-ups that diverge from vanilla simulation answers.
- `cargo xtask scenario checked` enforces zero differences across all checked speed-ups by default.
