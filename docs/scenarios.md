# Scenarios plan

Status: SC1 to SC3 built and tried in the game (2026-11-05); what was not tried is in [Scenarios](prototype-log/2026-11-05-scenarios.md). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Scenario, Leak run, Claim, Hidden start). It builds on the claims of test games ([test-games.md](test-games.md)) and on remote control ([remote-control.md](remote-control.md)); the decision behind it is [ADR scenarios-drive-the-game-through-remote-control-only](adr/scenarios-drive-the-game-through-remote-control-only.md).

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

`--wait` waits while a checkout from before test games holds the whole game. `--owner` and `--game-dir` are the claim's, as in `cargo xtask game`. `--leak.rounds 1` and the like change one of a scenario's parameters for this run only.

### A scenario

A scenario is Rust code in xtask. It says:

- its name and what it plays;
- the build it needs: the player build, a developer build with the memory account, or one with the heap census;
- its parameters, each with its value in the code: the saves it loads by name, rounds, days, a country.

It is handed the game at its main menu and plays it through remote control, asking the capabilities with their own request and answer types. It makes checks, which must hold for it to pass, records figures, and writes its sections of the report. A bound on a figure, such as the growth of private bytes per round, is a check written in the scenario. A save the scenario names that this computer does not have fails it at once, naming the save.

### One run

1. **Steam**: the runner finds Steam running with a user logged in, or stops before anything else, saying to start it. It never starts Steam itself.
2. **The claim**: it claims a test game, waiting with `--wait` while it is busy; if this checkout holds one already, it plays within that claim and leaves it held at the end.
3. **Each build** the scenarios named need, in turn, each with the test games' pipes (`dev-test-games`; the player build has that feature alone): it builds it, starts the test game hidden with it, the saves its scenarios load copied in from the player's, and plays that build's scenarios one after another, each from the main menu. Each scenario's report says how many other games ran beside it (`games_beside`).
4. **The report**, then **the release**, of a claim it made itself.

Whatever ends the run, an error such as a report it cannot write, a scenario's panic, or Ctrl-C, the runner ends the game and releases a claim it made, the panic going on after. Ctrl-C ends the game, so that the step waiting on it ends, leaves the scenarios after it unplayed, and writes the report as far as it got; a second Ctrl-C ends the runner at once, leaving the claim, which stays until `cargo xtask game release` releases it ([test-games.md](test-games.md), "How many, and for how long").

Of the processes it runs, only cargo, for the builds, and the game are programs of their own; everything else it does in its own process.

A scenario reaches its test game's process only through the claim, whose `follow.exe` follows each game to its exit code, holding it from the start on, and remote control through the test game's pipe by its name. So how a scenario ends, crashed, hung, failed, and the wait for the main menu across a restart, is tested without the game, on the claim's recorded computer with the client's own test server for the pipe (`xtask/src/scenario/game.rs`).

### How a scenario ends

| Outcome | When | Then |
| --- | --- | --- |
| Passed | Every check held | The next scenario |
| Failed | A check did not hold | The scenario ends there, with the figures it recorded until then; the game returns to the main menu, and the next one plays |
| Crashed | The game ended | The report has how the game ended, its exit code as the claim followed it, and what its crash folder and dump say; the game is started again for the next one |
| Hung | A step took longer than its time: an answer, the main menu, the days to run | The report says which step; the game is ended and started again for the next one |
| Not played | Its build could not be built, or the run was interrupted before it began | The report says why; the scenarios of the next build are played |
| Unfinished | Ctrl-C | The game is ended and the scenarios left are not played; the runner writes the report as far as it got and releases a claim it made |

### The report

One folder a run, with `report.json`, every answer and figure, and `report.md`: first a table of the scenarios with their build, outcome, the check or step they ended at and how long they played; then each scenario's own sections. The run is recorded in a file of its own in [prototype-log/](prototype-log/) by whoever ran it, from the report, saying what was not verified.

### The scenarios

- **`leak`**: the leak run on the build with the heap census: rounds of a load, an in-game load, a return, a new game and a return; after each, the memory account and the leftovers; after the last, what holds them and what keeps the free pages; with its parameters, the watch and the quarantine. The sections are those `leak-run` wrote.
- **`growth`**: the same rounds on the build with the memory account only, which slows nothing, for where each round's growth goes, part by part and heap by heap, with a bound on it per round once the recorded runs settle one.
- **`perf`**: what `perf-run` measured, on the player build: the seconds to the main menu, memory at it, a new game as Ming and the seconds it takes, CPU and memory paused for 30 s and at speed 5 for 120 s; and game days a minute at speed 5, from the dates. Its check is that it played to its end; its figures go into [performance.md](performance.md) when they measure what the framework costs, as `perf-run`'s did. With `--perf.game_paths`, the speed-ups it names (by name, group or `all`, by commas) are left to the game: it plays a developer build with the test switches, its core started with those in its own `dev.toml`, so that the same run measures the game's own code ([game-speed.md](game-speed.md), "How it fits together").

## How it fits together

```text
client/                  remote control's pipe, asked with remote/'s request and answer types;
                         the hidden start; the backup and restore of the player's files
cli/                     eu4-harmony game, start, reload, backup, restore: over client/
xtask/src/
  claim.rs               the claims of test games, cargo xtask game; a run within a
                         claim, from holding it to giving it back, which the runner
                         plays in
  scenario.rs            the runner: Steam, the builds, start, the outcomes,
                         the report
  scenario/game.rs       what a scenario is handed: ask, the main menu, days, events,
                         checks, figures, sections
  scenario/leak.rs       leak and growth
  scenario/perf.rs       perf
```

The claim starts its test game with `client/`, the library under the command line tool, not through the installed tool. A scenario asks its capabilities over one connection, typed, so a capability whose fields change fails to build the scenario instead of failing its run. xtask builds `remote/` with the memory group, which only developer builds answer.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`;
- what it delivers was tried in the game, in a claim's test game, with the run recorded in a file of its own in [prototype-log/](prototype-log/), saying as plainly what was not verified;
- [dev-tools.md](dev-tools.md), and the game-test skill where it names a command, describe what changed.

### SC1 The client library

`client/`, with the pipe, the hidden start, the backup and restore; the command line tool and the claim over it, with no change in what they do. Tried: `eu4-harmony game`, `start`, `backup` and `restore`, and a claim and its release.

### SC2 The runner, with perf

`cargo xtask scenario` with Steam's check, the claim, the builds, the outcomes and the report; `perf` as its first scenario, `perf-run` removed. Tried: `perf` played; the game ended from outside while a scenario played. Not tried: a step taking longer than its time, so the hung outcome is not verified.

### SC3 leak and growth

`leak` and `growth`, `leak-run` removed, `--from-report`. Tried: `leak` with two rounds, the watch and the quarantine; `growth` and `perf` in one run, on their two builds. Not tried: the player taking the game while it plays, so the unfinished outcome is not verified.

### Later

Each its own part when it is wanted: the other runs [prototype-log/](prototype-log/) records again and again as scenarios (input); reference figures, to compare a run's figures with earlier ones. A session's round trip, an extension reload, extension save data through a save and ironman became scenarios of the set `whole`, with the sweep of every capability and the long games ([whole-game.md](whole-game.md)), which also added sets, the check of the framework's log after each scenario, the developers' saves folder and test mods.
