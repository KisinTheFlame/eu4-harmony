# Sixth deepening plan

Status: decided on 2026-11-14 from an architecture review of the codebase after the [fifth deepening](fifth-deepening.md), all five parts authorized together; building. No glossary terms or new ADRs are needed.

## Goal and scope

Gather the courses of the speed-ups that run beside the game into modules whose interfaces carry and test the whole course, as the font page (E2), the native windows (F1) and the drawing coordinator (G7) already do; put the performance collection's clock at a seam; and write the folder listing and the Windows-name rules once:

- the parallel save's provinces and countries written on the pool, the writer-identity check, the join, the fallback and the "the save is wrong, end" rule (`core/src/saving/parallel.rs`, `saving/sum.rs`, `game/commands.rs`, `saves.rs`);
- the army searches worked out ahead: prepare, hand out, the think taking its own, the search taking a matching outcome, the call-off and wait (`core/src/army_search.rs`);
- the local save compressed in the background: the level-0 copy, the moved files, the hand-over and the wait (`core/src/saving/background.rs`, `saves.rs`, `sessions.rs`);
- the performance collection's lifecycle clock (`core/src/performance.rs`);
- the folder listing and the names Windows reads as written, written twice (`core/src/loading/lookups.rs`, `loading/listed.rs`).

It leaves out what the speed-ups compute (ADR [the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md)), the dependency points (declared where they are), the thread division of ADR [simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md), the zip compaction and the account's pure replay (already deep), and the development tools' declarations (set aside by the [third deepening](third-deepening.md)).

## What the game does

- A save's provinces and countries are written by two loops over a memory stream and a writer object; the framework's speed-up runs them on the pool and splices the results into the save the game's own loop writes ([research/saves.md](../research/saves.md)). The save must be byte-for-byte what the game's loops would have written, or the process ends.
- The King's think decides which agents search today; an army's think settles its own search before the search runs, and a search's outcome is taken only while the date, the arguments and the agent/unit state still match ([research/ai.md](../research/ai.md), [research/army-checks-in-searches.md](../research/army-checks-in-searches.md)).
- A local save's entries are copied at level 0 while the main thread saves; the files the save moved are compressed by a worker afterwards, and the next save, a load and a restart wait for it ([research/saves.md](../research/saves.md)).
- The performance collection's events all use one monotonic clock and carry a session identifier; a change of session, speed or pause state divides the window into segments ([research/performance-boundaries.md](../research/performance-boundaries.md), [performance-metrics.md](performance-metrics.md)).
- PhysFS's stat and the folder archive's stat answer from a folder's listing only for ASCII names Windows reads as they are written ([research/loading.md](../research/loading.md), [research/localisation-loading.md](../research/localisation-loading.md)).

## Behavior

Nothing changes for players, extensions or remote clients. A developer build's checked modes keep comparing each speed-up's answers with the game's, and each speed-up can still be left to the game on its own. The fake game is unchanged. What changes is where the knowledge lives: each course lands once, behind an interface a test drives through the same seam the hooks use.

## How it fits together

Each part is one module. The course's state moves out of module statics into the module; the game's functions, memory, pool, worker and clock are reached through an internal seam with two adapters: the real game, and a recording or failure-injecting test adapter (a synchronous worker, a controlled clock, a fake search over a built memory image). The hooks become thin adapters that tell the module their moments. The dependency points stay declared where they are; the memory stream's layout is named once.

## Parts

**H1. The parallel save's course in one module.** Gather the hand to the pool, the writer-identity check, the join where the world has written its game state, the fallback to the game's own loop, and the "the save is wrong, end" rule (five `crate::fatal` sites today) into one module behind `begin` / `countries` / `join`, with the game's stream/writer and the pool as adapters. The memory-stream layout, named in `parallel.rs`, `sum.rs` and `commands.rs` today, is named once. Tests drive whole courses: both loops ahead, a changed writer, a faulted run written again by the main thread, an out-of-order splice, the checked comparison.

**H2. The army searches' course in one module.** Gather prepare (the think's decision tree reproduced), the hand to the pool, the think taking its own search, the search taking a matching outcome (the date, arguments and state hash), the call-off and wait at the armies' day's end, and the gate rules, into one module with the game's search and the agent/unit memory as adapters. Tests drive the take/settle races, a job called off before it begins, a changed state or arguments, a unit handed between agents mid-search, and the end-of-day settle, over a built memory image (`buffers::Image`'s pattern).

**H3. The background compression's course in one module.** Gather the level-0 copy while the main thread saves, the moved files' collection, the hand-over to the worker, and the wait before the next save, load or restart (told from `saves.rs` and `sessions.rs` today) into one module with the worker and the file system as adapters (a real thread and a synchronous test worker). The zip compaction stays the deep module it is.

**H4. The performance collection's clock at a seam.** The lifecycle (begin/end replacement refusal, the deadline, the generation token, the context split) takes the clock at its edge: the real monotonic instant in the game, a controlled clock in the tests. The public recorders stop calling `Instant::now()` themselves; the account stays the pure replay it is. The lifecycle test drives begin, context change, deadline expiry, end and the old-token discard through the public interface without locking `STATE`.

**H5. The folder listing and the Windows-name rules once.** The `Kind` classification, the directory listing and the "names Windows reads as written" rule, written in both `lookups.rs` and `listed.rs` and already drifted (separator, colon, backslash), live in one module both speed-ups read. Each keeps its own seam (PhysFS's stat, the folder archive's stat) and its own switch.

## Every part is done when

Each deepened module replaces the scattered logic in its callers rather than sitting beside it, and is tested through its own interface. Formatting, the locked release build, workspace tests, both clippy configurations, and the offline checks pass. Hidden claimed test games exercise the changed paths (saving, the AI's searches, the collection, startup loading), recorded in `docs/prototype-log/` with unverified limits stated plainly, and all parts ship together in one pull request.
