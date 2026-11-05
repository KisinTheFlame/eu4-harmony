# Scenarios plan

Status: SC1 to SC3 built and tried in the game (2026-11-05); what was not tried is in [prototype-log.md](prototype-log.md). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Scenario, Leak run, Claim, Hidden start). It builds on the claim ([game-testing.md](game-testing.md)) and on remote control ([remote-control.md](remote-control.md)); the decision behind it is [ADR scenarios-drive-the-game-through-remote-control-only](adr/scenarios-drive-the-game-through-remote-control-only.md).

## Goal and scope

- **Goal**: the runs the framework's developers make in the real game to measure it, the leak run and the perf run first, become scenarios that one runner plays. A new kind of run is then a short scenario, not another program that claims, installs, starts, drives and reports on its own, and a set of them plays unattended within one claim and ends in one report.
- **In scope**: the runner, `cargo xtask scenario`; a library for reaching the game that the command line tool and xtask share (remote control's pipe, the hidden start, the backup of the player's files); the leak run and the perf run as scenarios, in place of `cargo xtask leak-run` and `perf-run`.
- **Out of scope**: mod authors, whose tests run against the fake game (`eu4-harmony test`); vanilla, which has no remote control; comparing figures with those of earlier runs by themselves (reference figures), a later part; what can only be judged by looking, such as text drawn on screen; CI, which has no game.

## Behavior

### The command

| Command | What it does |
| --- | --- |
| `cargo xtask scenario` | Lists the scenarios: each one's build, about how long it plays, and its parameters with their values |
| `cargo xtask scenario <name>… [--out <dir>] [--wait] [--owner <name>] [--game-dir <dir>] [--<name>.<parameter> <value>]…` | Plays the scenarios named, in that order but grouped by build, and writes the report into `--out` |
| `cargo xtask scenario --from-report <dir>` | Writes `report.md` again from the `report.json` in the folder, with no game |

`--wait` waits in the claim's queue when the game is held. `--owner` and `--game-dir` are the claim's, as in `cargo xtask game`. `--leak.rounds 1` and the like change one of a scenario's parameters for this run only.

### A scenario

A scenario is Rust code in xtask. It says:

- its name and what it plays;
- the build it needs: the player build, a developer build with the memory account, or one with the heap census;
- its parameters, each with its value in the code: the saves it loads by name, rounds, days, a country.

It is handed the game at its main menu and plays it through remote control, asking the capabilities with their own request and answer types. It makes checks, which must hold for it to pass, records figures, and writes its sections of the report. A bound on a figure, such as the growth of private bytes per round, is a check written in the scenario. A save the scenario names that this computer does not have fails it at once, naming the save.

### One run

1. **Steam**: the runner finds Steam running with a user logged in, or stops before anything else, saying to start it. It never starts Steam itself.
2. **The claim**: it claims the game, waiting in the queue with `--wait`; if this checkout holds the game already, it plays within that claim and leaves it held at the end.
3. **Each build** the scenarios named need, in turn: it builds it, installs it, starts the game hidden, and plays that build's scenarios one after another, each from the main menu.
4. **The release**, of a claim it made itself; then it writes the report.

While it plays, it keeps the lease going and notices the player taking the game. Of the processes it runs, only cargo, for the builds, and the game are programs of their own; everything else it does in its own process.

### How a scenario ends

| Outcome | When | Then |
| --- | --- | --- |
| Passed | Every check held | The next scenario |
| Failed | A check did not hold | The scenario ends there, with the figures it recorded until then; the game returns to the main menu, and the next one plays |
| Crashed | The game ended | The report has what the crash dump says; the game is started again for the next one |
| Hung | A step took longer than its time: an answer, the main menu, the days to run | The report says which step; the game is ended and started again for the next one |
| Unfinished | The player took the game | The scenarios left are not played; the runner does not release, since the player's claim does, and writes the report as far as it got |
| Not played | Its build could not be built or installed, or the player had taken the game before it began | The report says why; the scenarios of the next build are played |

### The report

One folder a run, with `report.json`, every answer and figure, and `report.md`: first a table of the scenarios with their build, outcome, the check or step they ended at and how long they played; then each scenario's own sections. The run is recorded in [prototype-log.md](prototype-log.md) by whoever ran it, from the report, saying what was not verified.

### The scenarios

- **`leak`**: the leak run on the build with the heap census: rounds of a load, an in-game load, a return, a new game and a return; after each, the memory account and the leftovers; after the last, what holds them and what keeps the free pages; with its parameters, the watch and the quarantine. The sections are those `leak-run` wrote.
- **`growth`**: the same rounds on the build with the memory account only, which slows nothing, for where each round's growth goes, part by part and heap by heap, with a bound on it per round once the recorded runs settle one.
- **`perf`**: what `perf-run` measured, on the player build: the seconds to the main menu, memory at it, a new game as Ming and the seconds it takes, CPU and memory paused for 30 s and at speed 5 for 120 s; and game days a minute at speed 5, from the dates. Its check is that it played to its end; its figures go into [performance.md](performance.md) when they measure what the framework costs, as `perf-run`'s did.

## How it fits together

```text
client/                  remote control's pipe, asked with remote/'s request and answer types;
                         the hidden start; the backup and restore of the player's files
cli/                     eu4-harmony game, start, reload, backup, restore: over client/
xtask/src/
  claim.rs               the claim, cargo xtask game; the runner calls the same functions
  scenario.rs            the runner: Steam, the claim, the builds, install, start,
                         the outcomes, the lease, the report
  scenario/game.rs       what a scenario is handed: ask, the main menu, days, events,
                         checks, figures, sections
  scenario/leak.rs       leak and growth
  scenario/perf.rs       perf
```

The claim backs up and starts the game with `client/`, the library under the command line tool, not through the installed tool. A scenario asks its capabilities over one connection, typed, so a capability whose fields change fails to build the scenario instead of failing its run. xtask builds `remote/` with the memory group, which only developer builds answer.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`;
- what it delivers was tried in the game, within a claim, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- [dev-tools.md](dev-tools.md), and the game-test skill where it names a command, describe what changed.

### SC1 The client library

`client/`, with the pipe, the hidden start, the backup and restore; the command line tool and the claim over it, with no change in what they do. Tried: `eu4-harmony game`, `start`, `backup` and `restore`, and a claim and its release.

### SC2 The runner, with perf

`cargo xtask scenario` with Steam's check, the claim, the builds, the outcomes and the report; `perf` as its first scenario, `perf-run` removed. Tried: `perf` played; the game ended from outside while a scenario played. Not tried: a step taking longer than its time, so the hung outcome is not verified.

### SC3 leak and growth

`leak` and `growth`, `leak-run` removed, `--from-report`. Tried: `leak` with two rounds, the watch and the quarantine; `growth` and `perf` in one run, on their two builds. Not tried: the player taking the game while it plays, so the unfinished outcome is not verified.

### Later

Each its own part when it is wanted: the other runs [prototype-log.md](prototype-log.md) records again and again as scenarios (a session's round trip, an extension reload, extension save data through a save, ironman, input); reference figures, to compare a run's figures with earlier ones.
