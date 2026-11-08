# The whole game plan

Status: decided (2026-11-08), at the owner's request for stability before the framework is given to players: "a large scenario that tests the whole game once". W1 to W6 built, and the set played to its end the same night ([The whole game played through](prototype-log/2026-11-08-the-whole-game.md)): it found three defects of the framework, an answer's shape that lost fields, `menu` answering `starting` in a game at speed, and leaving a session while time ran, which crashed the game; all three are fixed. Terms are in [GLOSSARY.md](../GLOSSARY.md) (Scenario, Scenario set, Sweep, Test mod). It builds on the scenarios ([scenarios.md](scenarios.md)) and test games ([test-games.md](test-games.md)), and adds no decision of its own that is hard to reverse.

## Goal and scope

- **Goal**: one command plays the framework through the real game from end to end, unattended, in a test game of its own, and ends in one report that says what held, what failed and where the game crashed or hung: every capability in every place, every way into and out of a session, a game the AI plays from 1444 to its end, the late game, and extensions that go wrong. It is run before a release, and whenever a change may have touched the framework's stability; the first full run is recorded and what it found is fixed or written down.
- **In scope**: a scenario set, `whole`, and the scenarios it names; a check the runner makes after every scenario, that the framework logged no panic and no fatal error; saves from the developers' own saves folder; test mods, a scenario's own Harmony mods put in its test game's playset.
- **Out of scope**: what can only be judged by looking (text drawn right, the framework UI's layout), which the screenshots of a run let a person look at but nothing checks; input that clicks where it lands (remote clients' mouse and keyboard), whose effect depends on what is under the pointer; multiplayer; other baselines than 1.37.5; the player's own computer (other Windows versions, graphics drivers, folders with other names), which a test game on the developers' computer cannot stand for; CI, which has no game; a gate that refuses to pack a release without a report (the release is packed by hand, and its notes link the run).

## Behavior

### The set

| Command | What it plays |
| --- | --- |
| `cargo xtask scenario whole` | Every scenario the set names, in its order, grouped by build: `sweep`, `sessions`, `extensions`, `late`, `long`, `checked`, `growth`, `perf` |
| `cargo xtask scenario whole --long.years 20` | The same, a parameter of one of them changed as for that scenario alone |

A set is a name in `cargo xtask scenario`'s list beside the scenarios, with the scenarios it names; it has no parameters of its own. The runner plays it as if its scenarios were named one by one, so each has its outcome, its sections and its time in the report, and one that fails, crashes or hangs leaves the others to be played. With the parameters as the code has them, `whole` plays for about seven hours; `long` takes most of them.

### What every scenario is checked for

After a scenario that passed, the runner reads the framework's logs of its test game, `core.log` and the one kept from the start before it: a line that tells of a panic or a fatal error, written since the scenario began, fails it, with the line. A scenario that crashed or hung is not passed anyway.

### Saves

A scenario names its saves as before. A save the player's `save games` does not have is looked for in the developers' saves folder, `%LOCALAPPDATA%\eu4-harmony-dev\saves\` (`EU4_HARMONY_DEV_DIR` names another), which keeps saves made for testing out of the player's own: the late game's saves of one game the AI played alone, `observer-1700.1.1` to `observer-1820.7.1`, are there. Either way it is copied into the test game.

### `sweep`: every capability, in every place

On the player build. It asks every capability in the catalogue that remote control has, in each of five places:

| Place | How it gets there |
| --- | --- |
| The main menu | the game started |
| A game, paused | a new game as `country` (France) |
| A game, at speed | the same, its time running at speed 5 while it asks |
| Observing, no country in view | a new game in observe mode |
| Observing, a country in view | `session.view` of `country` |

In each place it first asks the capabilities that read (those whose answer is more than nothing), then those that act. Each is asked once with the arguments a player would give, made from what the reads answered: the player's capital and armies, a neighbor's tag, a country it is at war with where there is one, and for an argument named as a field of an answer (`building`, `idea_group`, `reform`, …) a value that field had, taken first from an entry the player `can` choose. Then once for each argument with a value at the edge of its kind in its place: a province that does not exist, a tag no country has, an army that is not there, a date before the game's, 0 and 2³¹−1, an empty and a long text, empty lists. Optional arguments are left out.

Every answer must be either a result of the capability's own shape, which the catalogue declares, or a refusal with one of the catalogue's codes but `internal`; a request of the right kinds of argument refused with `bad_request` fails too. After each, the game must still be in the place, or the sweep goes back to it (a capability that acts may leave it: a peace that ends the player's country, a view left). It leaves out the capabilities that leave the place by design (`menu.load`, `menu.start`, `menu.return`), those that give the player's input (`input` but `input.get`) or show the game on the desktop (`desktop.show`), and the developer builds' `memory`; `time.advance` is asked for one day, and `console.run` with the console's `help`. What it found does not stop it: its sections list every capability with its answers place by place, and every answer that did not hold, and it fails at its end when there was one.

### `sessions`: every way into and out of a session

On the player build, `rounds` (10) rounds, each from the main menu:

1. load `first_save`, let 10 days run, load `second_save` in the game, let 10 days run;
2. return to the main menu; start a new game as `country`, let 10 days run, write the console's `autosave`, load it in the game, let 10 days run;
3. return; start a game in observe mode, let 10 days run, put `country` in view, take it out of view, return;
4. load `ironman_save`, an ironman game; let 10 days run; load `first_save` in the game (an in-game load in ironman, [ADR in-game-load-in-ironman](adr/2026-11-03-in-game-load-in-ironman.md)); return.

Each step must answer as asked: the session's number grows with every new session, its country and date are the save's or the new game's, a return comes to the main menu in this process, the in-process return. Its sections: each round's seconds, and the steps that took longest.

### `extensions`: extensions that go wrong

On the player build, with four test mods in the playset, each a Harmony mod of the repository's (`xtask/src/scenario/mods/`):

| Mod | Its extension |
| --- | --- |
| `qa-steady` | counts days and months, reads the country and its capital each day, keeps the count as its save data and logs what it gets back in `on_session` |
| `qa-failing` | raises an error in every `on_day`, and calls a capability wrongly in every `on_month` |
| `qa-looping` | loops without end in `on_month`, which the time limit stops; after three such stops the framework stops it |
| `qa-hungry` | grows a table in `on_month` until the memory limit refuses it |

From the main menu: a new game as `country`, `days` (120) days at speed 5; the console's `autosave` and its load in the game; `qa-steady` reloaded `reloads` (10) times while the time runs, a few days between; another `days` days; a return. Then from the extensions' logs: `qa-steady` counted every day the session ran and got its count back after the load and after each reload; `qa-failing` logged its errors, the same error in full three times a session and then counted; `qa-looping` was stopped by the time limit three times and then stopped for good; `qa-hungry` hit the memory limit and was not stopped by it; and the game's date went on through all of it; its sections give how many game days a minute it ran.

### `late`: the late game

On the player build. For each save of `saves` (the AI's own game in 1700, 1760, 1780, 1800 and 1820): load it, observe (the console's `observe`), let `days` (365) days run at speed 5, or up to a month before the game's end on 1821.1.2, write an `autosave` and load it in the game, let 30 days run, return. Checks: every load and run answered, the date went on. Its sections: days a minute and the game's memory for each.

### `long`: a game from 1444 to its end

On the player build. A new game in observe mode, at speed 5 for `years` (376, to 1820.11.11, the last year before the game ends on 1821.1.2), in steps of 365 days; every `save_every` (50) years an `autosave` loaded in the game. After each year: the date, how many days a minute the year ran, the game's working set and private bytes. Checks: each year's step answered and its date is a year on; the private bytes stay under `memory_mb` (8,000). Its sections: the years in tens, with their speed and memory, and the slowest year.

### `checked`: the speed-ups checked against the game

On the developer build with the test switches, its core started with `checked = ["all"]`: every speed-up that can, works out the game's own answer too and keeps that, and counts whether the two agreed. A new game in observe mode, at speed 5 for `years` (20), an `autosave` and its load every 5 years; then the startup report's count of answers checked and differing, speed-up by speed-up. Check: no speed-up whose answers must be the game's differed, in a single answer; those `may_differ` names (`army-searches`, whose differences have their reasons under [ADR the-game-may-be-made-faster-with-reasoned-differences](adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md)) are counted and not failed. It is the long-run form of what each speed-up's own run checked once.

## How it fits together

```text
xtask/src/scenario.rs        the sets beside the scenarios; the log check after each
xtask/src/scenario/
  game.rs                    the game a scenario is handed: also the console, the
                             autosave and its load, the test game's user folder
  sweep.rs                   sweep: the catalogue, the places, the arguments
  sessions.rs                sessions
  extensions.rs              extensions, and the logs it reads
  late.rs                    late, long and checked: a game let run, its memory
  mods/qa-*/                 the test mods
xtask/src/claim/machine.rs   a start's saves from the developers' folder too; its
                             test mods copied into the claim's folder and put in
                             the test game's playset
```

A test mod is copied into the claim's folder, `test-mods\<name>\`, at each start that names it, and the test game's own `dlc_load.json` names its descriptor there by its full path; nothing of it goes into the player's `mod` folder, which the claim reaches through its junction and never writes. A start that names no test mods takes them out of the playset again.

The sweep reads the catalogue from `remote/` as xtask is built, so a capability added later is swept with no change to it; the arguments are made by kind, and by name for the content keys, so a new kind of argument fails the sweep's own test until it has edge values.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`;
- what it delivers was played in a claim's test game, with the run recorded in a file of its own in [prototype-log/](prototype-log/), saying as plainly what was not verified;
- [dev-tools.md](dev-tools.md) and [scenarios.md](scenarios.md) describe what changed.

### W1 The set, the log check and the developers' saves

`whole`, the runner's log check, saves from `%LOCALAPPDATA%\eu4-harmony-dev\saves\`; what the game a scenario is handed gains (the console, the autosave and its load, the user folder).

### W2 sweep

### W3 sessions

### W4 late, long and checked

### W5 Test mods and extensions

The claim's test mods, the four mods and `extensions`.

### W6 The whole run

`whole` played to its end with the parameters as the code has them, recorded; what it found in the framework fixed where the fix is the framework's to make, and the rest written down with what was seen.
