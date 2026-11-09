# 2026-11-09 Scenarios played several at a time (tested by me)

`cargo xtask scenario` with `--jobs`, from this branch, while another checkout's `whole` ran on the same computer in a test game of its own. Each run: `extensions sessions growth perf --jobs 3 --sessions.rounds 1 --growth.rounds 1 --extensions.reloads 2`, two builds (player, memory account) made first and set aside in the report's `builds\`.

- **First run**: all four passed, but one after another: a worker held the queue's lock while its scenario played (the lock taken in a `while let`, whose guard lives through the loop's body), so the other workers waited. 1.7, 3.0, 1.3 and 3.9 minutes.
- **Second run**, the scenario taken off the queue before it plays: `sessions`, `extensions` and `growth` played at once, each in its own test game (`parallel-scenarios-<scenario>`), and `perf` alone after them; the four in about nine minutes. `extensions` failed: qa-hungry, which is to be refused memory before its handlers run past the time limit of a second, ran past it first, three times, and was stopped. The time limit is wall time, and with three games and the other checkout's on 16 logical processors its handlers took longer. So `extensions` plays alone too, after the rest, as `perf` does.
- **Not verified**: `whole` itself at four or more at a time; `long` and `late` beside other games, whose game days a minute their reports give and which other games slow; a run with `extensions` alone.
- **Cleanup**: each scenario's claim released by the runner, with its logs kept; nothing else left.
