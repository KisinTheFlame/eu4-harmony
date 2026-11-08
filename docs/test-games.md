# Test games plan

Status: decided 2026-11-08, T1 to T4 built; what was tried in the game is in [prototype-log.md](prototype-log.md), "Test games". The limit of three, the queue and the lease were taken out the same day ([ADR test-games-have-no-limit-or-lease](adr/test-games-have-no-limit-or-lease.md)). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Test game, Claim). It replaces the claim of the whole game of [game-testing.md](game-testing.md), and rests on [research/several-games.md](research/several-games.md), which found that several games run at once. The decision is [ADR test-games-run-side-by-side](adr/test-games-run-side-by-side.md).

## Goal and scope

- **Goal**: several sessions try things in the game at the same time, each in a test game of its own, which touches neither the player's files nor the framework installed in the game folder; so a claim backs up and puts back nothing, and a session never waits for another's test game.
- **In scope**: the proxy loading a test game's core; the framework reading `-userdir`; the test game's own pipe in a developer build; the command line tool asking a test game; the claim as test games; the scenario runner, `cargo xtask profile` and `cargo xtask install` on them; the game-test skill.
- **Out of scope**: a claim of the whole computer, for installing into the game folder or measuring alone (none for now: a measurement says how many other games ran beside it); a way for the player to end the test games, until it is known whether they keep the player from starting the game (open question below); playing several scenarios of one run in several test games at once.

## Behavior

### A test game

A test game is started from the game folder, so that it has every DLC the player's game has (research/several-games.md), with three switches:

| Switch | Read by | What it does |
| --- | --- | --- |
| `-userdir=<folder>` | the game, and the framework in every build | the test game's user folder: its settings, logs, saves and crash folders |
| `-harmony_core=<file>` | the proxy, in every build | the core to load instead of `eu4-harmony\eu4_harmony_core.dll`; its `dev.toml` is the one beside it. A core named that does not load ends the game with 3 |
| `-harmony_test_game=<name>` | the core of a developer build with `dev-test-games` | remote control on `\\.\pipe\eu4-harmony-<name>` instead of `\\.\pipe\eu4-harmony`; a player's build passes it over and says so in `core.log` |

The game restarting itself, as a return to the main menu with the in-process return off does, writes every switch again as `--name="value"` ([research/main-menu.md](research/main-menu.md)); the proxy, the framework and the game read both forms, so the game started in its place is the same test game, hidden.

The test game's name is its owner's (the leaf name of the checkout's folder, or `--owner`): letters, digits, `.`, `_` and `-`. The command line tool asks it with `--test-game <name>` (`game`, `reload`) or with `EU4_HARMONY_TEST_GAME` set; without either it asks the player's pipe, as it always has.

### The commands

| Command | What it does |
| --- | --- |
| `cargo xtask game claim --purpose <text> [--wait]` | Claims a test game: its folder, with a user folder made from the player's. Busy, exit 2, only while a checkout from before these commands holds the whole game; `--wait` waits until it releases it. Refused while the game folder's proxy cannot load a test game's core |
| `cargo xtask game start [--save <name>]... [--visible] [--sound] [--seed <n>] [--timeout <seconds>]` | Copies this checkout's built core and `dev.toml` into the test game's `core\`, and each save named from the player's `save games`, then starts the test game hidden and waits for its main menu. Refused while it runs, or when the built core has no test games' pipes |
| `cargo xtask game stop` | Ends the test game |
| `cargo xtask game release` | Ends the test game if it runs, keeps its logs and crash folders in `kept\`, removes the rest of its folder, and frees it; says how long it was held and how its games ended |
| `cargo xtask game status` | Every test game: its holder, purpose and process; the `eu4.exe` that are no test game's; folders no claim refers to |

A claim holds its test game until it is released. `watch`, `renew`, `--lease`, `--player` and `--now` are gone: nothing waits to be put back, nothing is taken from a holder, and the player's game is not a claim.

### One run

1. Build with the test games' pipes: `cargo build --release --features eu4-harmony-core/dev-test-games` (and any other `dev-*` the run needs).
2. `cargo xtask game claim --purpose "..."`.
3. `cargo xtask game start --save <name>`; then `eu4-harmony game --test-game <owner> ...`, or with `EU4_HARMONY_TEST_GAME` set. A rebuild between `stop` and `start` is taken up at the next start.
4. `cargo xtask game release`, then read `kept\`.

`cargo xtask scenario` claims, starts and releases a test game itself, and plays in the one its owner holds.

### The claim's folder

`%LOCALAPPDATA%\eu4-harmony-dev\claims\<owner>-<yyyyMMdd-HHmmss>\` (`EU4_HARMONY_DEV_DIR` names another folder for the record and the claims' folders, to try the claims apart from the computer's own):

- `user\`: the test game's user folder. The claim copies the player's `settings.txt`, `dlc_load.json` (the playset), `dlc_signature`, `game_data.json`, `gameplaysettings.txt`, `messagetypes_custom.txt`, `pdx_settings.txt`, `shortcuts.txt` and the framework's `eu4-harmony\settings.txt`, those that are there, and makes `mod` a junction to the player's `mod` folder, where the playset's descriptors are. `start` copies the saves it is given into `user\save games\`.
- `core\`: the core and `dev.toml` `start` copied.
- `follow.exe`: a copy of xtask that follows the test game to its end and writes how it ended into `game-ends.tsv`, as the claim's `watch` did; after an end with 0, as a restart's, it follows the game the test game restarts in its place, found by its pipe. One runs a claim: a start ends the one before.
- `kept\`, after the release: `eu4-harmony-logs\`, `game-logs\`, `crashes\` and `game-ends.tsv`. A claim removes the `kept\` of claims released more than 7 days before.

The release removes the junction before the folder, so that the player's `mod` folder is never removed through it.

### How many, and for how long

- Any number of test games at once, and a claim holds its own until its holder releases it: no queue, no lease, nothing taken from a holder ([ADR test-games-have-no-limit-or-lease](adr/test-games-have-no-limit-or-lease.md)). A game in 1820 takes 4.2 GB of memory and 1.3 GB of the graphics card's ([prototype-log.md](prototype-log.md), "Test games"); `status` says how many are held.
- A claim left behind, such as a scenario run's ended by a second Ctrl-C, stays until `cargo xtask game release --owner <name>` releases it.

### Beside other games

- An `eu4.exe` that is no test game's is the player's, or a test game of a checkout from before these commands, and no command ends it.
- A checkout from before these commands knows only `game-lock.json`. While any test game is held, `game-lock.json` names a holder `test-games` with a lease of a year, so that such a checkout waits; while such a checkout's claim is there, a claim of a test game waits for it, looking again every 5 seconds, since that checkout installs into the game folder and backs up the player's files.
- `cargo xtask install` needs no claim, and is refused while any `eu4.exe` runs: Windows keeps a loaded `version.dll` from being replaced.

## How it fits together

- `proxy/` reads `-harmony_core` (its own copy of the folders crate's word splitting, which a test compares, so that the proxy links nothing but the core); `core/src/paths.rs` reads the command line once, `-userdir` through `eu4_harmony_folders::user_dir_of`, and the core's own folder; `core/src/pipe.rs` names the pipe; `eu4_harmony_remote::pipe_of` makes a test game's pipe's name for the core and the client alike.
- `client/src/start.rs` starts a test game, checking the proxy and the core for the switches they need, and waiting for the main menu on the test game's pipe; `client/src/pipe.rs` names the pipe from `--test-game` or the environment.
- `xtask/src/claim/`: `record.rs` the claims and what a claim finds when it looks at them, without files or clock; `course.rs` claiming, starting, stopping, releasing and a run within a claim, over the seam `machine.rs`, which is the computer: the record under its mutex, the processes, the claims' folders and the clock. The course's tests play whole courses over a recorded machine.
- The scenario runner builds each build with `dev-test-games` and copies it into its test game's `core\`; its "player" build is the player's with the test games' pipes only, which the startup report names.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- [dev-tools.md](dev-tools.md) describes what changed.

### T1 A core and a user folder of a test game's own

The proxy's `-harmony_core`, the framework reading `-userdir`, `dev.toml` beside the core.

### T2 A test game's pipe

`dev-test-games`, `eu4_harmony_remote::pipe_of`, the startup report's line, `--test-game` and `EU4_HARMONY_TEST_GAME` in the command line tool, a test game's start in `client/`.

### T3 Claims of test games

`cargo xtask game` on test games: the record, the claim's folder, `follow.exe`, the marker in `game-lock.json`.

### T4 The commands on test games

The scenario runner, `cargo xtask profile --test-game`, `cargo xtask install` without a claim, the game-test skill, AGENTS.md's testing section, [dev-tools.md](dev-tools.md) and [scenarios.md](scenarios.md).

## Open questions

- Whether the player can start the game from Steam or the Paradox launcher while a test game runs.
- Whether a test game writes anything into the player's user folder.
