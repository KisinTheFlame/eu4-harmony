# Commands plan

Status: decided (2026-11-03); C0 built and tried in the game the same day ([prototype-log.md](prototype-log.md)); C1 to C3 built and tried in the game the same day. Terms are in [GLOSSARY.md](../GLOSSARY.md) (Command, Developer build). The decisions and their reasons are in [ADR one-command-layer-in-written-form](adr/one-command-layer-in-written-form.md) and [ADR commands-run-at-once](adr/commands-run-at-once.md). What the game does with commands, and what was tried in the game before this plan, is in [research/commands.md](research/commands.md) and [prototype-log.md](prototype-log.md).

## Goal and scope

- **Goal**: every capability that changes the game reaches it the way the game's own interface does, as a command, through one module of the core that posts commands from their written form and hears every command the game executes.
- **In scope**: developer builds, one Cargo feature for each development tool, before anything else (C0); the command layer, posting and hearing (C1); the command log rebuilt on it; recruiting and diplomacy moved onto it (C2); the army orders tried in the game and moved onto it, and a list of the capabilities that change the game without a command (C3).
- **Out of scope**: multiplayer; offering commands, their type names or their keys to extensions or remote clients (ADR one-capability-catalogue, ADR one-command-layer-in-written-form); events from what the layer hears, which are added as mods need them (C4).

## Behavior

### Posting a command

- A capability checks its arguments and the player's situation as it does now, and refuses with the game's reasons as the game's interface gives them.
- It then writes its command as text, in the game's own written form, and hands the text to the layer.
- The layer reads the text with the game's reader. It writes the command read out again and refuses the text when any key it was given did not come back with the same value: the reader passes over what it cannot read and says nothing of it. A key left out takes the game's default.
- The layer asks the command's own check. A command the check refuses is destroyed and not posted; the caller is told only that the game refused it, since the check gives no reasons.
- A command the check accepts is executed at once, as the session's update executes one at the next frame, with the stamp the session's post gives it (ADR commands-run-at-once); it does not go through the session's queue, so what the game posted before it, such as the next day, runs at the next frame, after it. When the capability returns, the game has carried the command out.
- The layer executes only on the main thread, at the safe point, in the states in which the session's update executes commands, and never while a command executes; a session that executes none then takes no command.
- The layer posts for whichever country the text names. The capabilities on it act for the player alone (ADR remote-control-uses-capabilities).

### Hearing commands

- Every command the game executes, the player's, the AI's and the game's own, is heard once, before it runs, with its type and its written form.
- A command is written out only while something listens. In a player build nothing listens until C4 adds events, and the layer costs nothing per command; writing one out costs about 10 µs.
- A command's sender does not tell the player's commands from the AI's: what is heard says what happens and to which country, not who asked for it.

### The command log

A tool of developer builds, on the layer: it logs every command heard, or only the types `dev.toml` names, in its written form. The parts the verification needed (reading each command back and comparing, reading and posting lines from a file) go; their results are in research/commands.md.

### Developer builds

- **Features**: one for each development tool, each named with `dev-`: `dev-call-trace`, `dev-heap-census`, `dev-command-log` and `dev-test-switches` (the switches that make startup fail and show a test notice); and `dev`, which turns them all on. `trace` goes; `analysis` stays, for the disassembly example and the offline checks only.
- **On when built in**: a tool built in is on; no environment variable turns it on. The tools' options are in a developer settings file, `eu4-harmony\dev.toml` in the game folder next to the core, one section for each tool. `cargo xtask install` copies it there from the repository's `dev.toml`, which is not checked in; `core/dev.toml.example` shows every section and option. Where the folder is, `eu4-harmony backup` and `restore` leave it alone.
- **Defaults**: a section that is missing takes its defaults: the command log logs every type, the call trace traces nothing, the census counts at every world reset, the test switches are off.
- **What a developer build says**: the first line of the startup report and of the framework log says "developer build" and lists the tools built in; the report lists each tool's options as they took effect.
- **Test switches**: making startup fail and showing a test notice move out of player builds into `dev-test-switches`, turned on in `dev.toml`. The test notice's text leaves the player's localisation files. Player builds then hold no hidden switches. The steps in docs/unicode-module.md and the prototype log's notes that use them are changed to match.
- **Packing**: `cargo xtask release` builds the player build itself before packing it, so what was built last does not matter. `cargo xtask install` installs what was built last, as now.
- **CI**: the player build is built, tested and checked with clippy, as now; the developer build is built and checked with clippy with `dev`, so the tools keep building.

## How it fits together

One module of the core, `commands`, has two entries:

```rust
/// Reads the text, writes it out again and compares, asks the command's check, and has the game execute it at once.
pub fn post(text: &str) -> Result<(), Refused>;
/// Calls `heard` with each command the game executes, before it runs.
pub fn listen(heard: fn(&Heard));
```

`post` runs where capabilities run, at the safe point. Inside, comparing what was given with what came back is a pure function on two texts, with unit tests of its own; the reader, the check and the post are the game's, kept as dependency points. Capabilities write their texts with a small writer of the written form, not by hand.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`, and so do the offline checks against `eu4.exe`;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md);
- docs/research holds what was found in the game's code, and README.md and README.zh-CN.md say what changed for players.

### C0 Developer builds

Its own pull request, before the rest. The features, `dev.toml` and its example, the developer build's first lines and options in the startup report, the test switches moved, `release.ps1` building the player build, CI, and AGENTS.md's and tools/README.md's commands and notes. The call trace, the census and the command log as they were, under their new features and reading their options from `dev.toml`, the command log without reading back and posting; no environment variable is left.

### C1 The command layer

`post` and `listen`, with their dependency points; the command log rebuilt on `listen`. Tried in the game: a command posted from text does what the same command does from the game's interface, its check refusing what the interface would refuse; a key that does not come back is refused; posting at the safe point. Why commands posted from text so far ran with no sender (research/commands.md) is found out, and the layer fills in what the game's interface fills in.

### C2 Recruiting and diplomacy

Recruiting, declaring war, offering peace and proposing an alliance write their commands as text and post them through the layer; the constructors, sizes and fields they made their commands with go, with their dependency points. Their checks and reasons stay as they are. Tried in the game: each gives the same results and the same refusals as before.

### C3 Army orders and the rest

`army.move` and `army.stop` move onto the game's own commands, as the player's right click makes them ([research/army-orders.md](research/army-orders.md)): a move command to another province, with the route replaced and the fleet's own orders cancelled, and, for the province the army is in, the command that cancels its movement. The game finds the route itself as it runs the command; the framework asks the same route query first, so that a goal with no route is refused with its reason, as now, and reads the route the game set afterwards. The route changes the command makes are the framework's own, as its direct calls' were, so route ownership and the orders heard stay as they are. `time.set_speed` posts one of the game's speed commands for each step, as the speed keys do. Tried in the game against what the two did before by calling the game's functions: the route taken, straits, an army already moving, an army stopped; the speed.

How every other capability that changes the game reaches it:

| Capability | How | Why |
| --- | --- | --- |
| `recruit.order`, `diplomacy.*` | the layer (C2) | |
| `army.move`, `army.stop`, `time.set_speed` | the layer (C3) | |
| `time.pause`, `time.resume` | the game's pause toggle, directly | in single player the player's pause key toggles the pause directly too; only in multiplayer does it post a command |
| `question.answer`, `offer.accept`, `offer.decline` | the game's own buttons, pressed | the buttons post the game's commands themselves |
| `console.run` | the game console's run function | console commands change the game directly; they are not commands |
| `camera.*` | the camera, directly | the camera is not game state, and the game posts no command for it |
| `menu.*` | the main menu's own steps | the setup commands are posted by the game's own interface, which the steps drive |

### C4 Events

A placeholder. Events for extensions and remote clients from what the layer hears, named and shaped after what a player sees (ADR one-capability-catalogue), each added when a mod needs it.
