# Game testing plan

Status: decided, G1 to G5 built and tried (2026-11-05; [prototype-log.md](prototype-log.md), "Game testing"). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Claim, Hidden start). It builds on the hidden start ([hidden-start.md](hidden-start.md)) and on the development commands ([dev-commands.md](dev-commands.md)).

## Goal and scope

- **Goal**: every session on this computer that tries something in the game does it the same way, through one set of commands that hands the game to one holder at a time, fairly, starts it hidden, and puts back everything the run changed: the player's files, the framework files in the game folder and its `dev.toml`. The player, who plays on the same computer, gets the game first when they ask for it.
- **In scope**: the claim, now `game-lock.ps1` in the game-lock skill, as commands of `cargo xtask game`; a queue of those waiting; the player's claim; the claim's own folder, with what a run leaves to look at; starting and stopping the game within a claim, always hidden; `cargo xtask install`, `perf-run` and `leak-run` on top of it; the skill, renamed game-test, rewritten for the commands.
- **Out of scope**: more than one game at once ([research/headless.md](research/headless.md), "What it would take"); a playset of the run's own, since the playset is a player file the release puts back anyway; measuring vanilla, which `perf-run` no longer does: what matters is the framework's own game.

## Behavior

### The commands

| Command | What it does |
| --- | --- |
| `cargo xtask game claim --purpose <text> [--wait]` | Claims the game: backs up the player's files and copies the framework files in the game folder, with `eu4-harmony\dev.toml` if there is one, into the claim's folder. Busy without `--wait`: says who holds it and exits with 2. With `--wait`: joins the queue and exits once it holds the game |
| `cargo xtask game start [--sound]` | Starts the game hidden (`eu4-harmony start --hidden`), and waits for the main menu; a start that has not reached it in time ends the game and fails. Refused without the claim |
| `cargo xtask game stop` | Ends the game |
| `cargo xtask game release` | Ends the game if it runs, copies what the run left into the claim's folder, puts back the player's files and the game folder's framework files and `dev.toml` and compares each, and frees the game; prints a summary |
| `cargo xtask game renew` | Starts the lease again |
| `cargo xtask game watch` | Run in the background by the holder: exits when the lease is near its end, when the player takes the game, or when the claim is gone |
| `cargo xtask game status` | Who holds the game, for what and until when, the queue, whether `eu4.exe` runs, and claim folders no claim refers to |

`cargo xtask install` (and `--uninstall`) needs the claim too. The owner of a claim is the leaf name of the checkout's folder unless `--owner` names another, as now.

### One run

1. `claim`: the player's files backed up into `backup\`, the game folder's `version.dll`, `eu4-harmony\eu4_harmony_core.dll`, `eu4-harmony\eu4-harmony.exe` and `eu4-harmony\dev.toml` (each as there or not there) into `framework\`. The record names the claim's folder and the process making it before the backup begins, and the claim is not one to use until the backup is done: `install` and `start` refuse it, others wait while that process backs up, and once it has ended with the backup not done, a claim of the same owner removes what it made and claims afresh, another claim takes over from it at once, and `release` says that nothing was put back, since nothing ran under it. A backup that fails gives the game back and removes what was made of the folder.
2. `install`, then `start`, as often as the run needs, with `stop` between.
3. `release`: the game ended if it runs; the framework's logs, the game's logs and the run's crash folders copied into `kept\`; the player's files put back by `eu4-harmony restore`, the game folder's files put back from `framework\` (a file that was not there removed), each compared. A file not put back is a `NOT RESTORED` line, and the claim is kept, as for the player's files now. The summary says how long the claim was held, whether the game crashed, and where `kept\` is.

### The claim's folder

Each claim has one, `%LOCALAPPDATA%\eu4-harmony-dev\claims\<owner>-<yyyyMMdd-HHmmss>\`, which `claim` and `release` print and `status` shows: `backup\` and `framework\` while the claim is held, `kept\` after it. A release removes `backup\` and `framework\`; a claim removes the `kept\` of claims released more than 7 days before. What a session wants for longer, it copies elsewhere.

### The queue

- A `claim --wait` joins the queue in the order it asked; when the game is free, only the first in the queue gets it.
- The waiting command says it is still waiting every 15 seconds; one that has not for a minute (its session was closed) leaves the queue, so it holds up no one.
- A `claim` without `--wait` does not join it: a game held, or with anyone in the queue, is busy.

### The player

- `cargo xtask game claim --player` claims the game for the player: no backup and no copy of the game folder, since their play is to be kept, and a lease of ten hours. It goes first in the queue and waits for the holder to release.
- `--now` takes the game from the holder: the holder's `watch` exits at once, saying the player took it, and the holder has two minutes to keep what it needs; then the game is ended, the holder's claim released for it (its files put back as by `release`, `kept\` included), and the game is the player's. The holder's run is unfinished.
- `cargo xtask game release --player` ends the player's claim.
- One process at a time puts a claim back: the holder's `release`, the player's claim taking the game, or a claim taking over from a holder gone marks the record as its own to put back first. A release of a claim the player's claim puts back is refused, saying so; a claim waits while another process puts back the holder's. A put back that fails leaves the record unmarked, still the holder's, to be put back again; one whose process ended is done again by the next release or claim. `release` on a claim being put back, or one whose put back did not end, is what redoes it; `install` and `start` refuse such a claim.
- `eu4.exe` running with no claim is the player playing, as now: a claim waits for the game to close.

### The lease

As now: 30 minutes, starting again at every `claim` and `renew`, and kept going while `eu4.exe` runs, so that it counts only the time the game is closed. A holder whose lease ran out has gone: the first in the queue takes over, its files put back first; its record stays until they are, so a put back that fails keeps the game the gone holder's, to be put back again, rather than handing it on.

### Moving from the skill's script

The record of who holds the game stays where and as it is (`%LOCALAPPDATA%\eu4-harmony-dev\game-lock.json`, the same mutex), so a checkout still on `game-lock.ps1` and one on the commands never hold the game at once. The queue is a file of its own, which the script does not know: such a checkout may take a free game ahead of the queue until it is up to date with master.

## How it fits together

`cargo xtask game` is a module of `xtask/`, beside `install`: the record, the queue and the claim's folder; it backs up, restores and starts the game with the checkout's own `client/`, the library under `eu4-harmony backup`, `restore` and `start`. The scenarios' runner ([scenarios.md](scenarios.md)) claims, starts, renews and releases with the same functions, in its own process; with vanilla gone, the game is reached through remote control only, not by clicking at fixed places of the screen. The skill is instructions only: when to claim, start, renew, keep what a run needs and release, and what each exit means.

Three things the runs showed, which the commands now take care of:

- `eu4-harmony start` makes the game's process with no handle of its own passed on (`CreateProcessW` without inheritance): a game holding a pipe that the command's output went to kept whoever read it, such as a session's shell, waiting until the game ended.
- A hidden start is refused when the framework in the game folder has none, as one from before the hidden start, which passes the switch over and shows the game; the release puts the player's own install back, so a claim installs before it starts.
- `watch` runs for the whole claim, and Windows keeps a running program's file from being replaced, which made every build that relinks xtask fail. It moves its own file aside as it starts and puts a copy where cargo looks.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- [dev-tools.md](dev-tools.md) describes what changed.

### G1 The claim as commands

`claim`, `release`, `renew`, `watch` and `status` in `cargo xtask game`, on the record the script keeps, with the claim's folder: the backup, the game folder's framework files and `dev.toml`, `kept\` and the summary. `install` needs the claim. Tried: a claim and release with the game started and crashed, the game folder and the player's files found as before.

### G2 The queue and the player

The queue with its liveness; `claim --player`, first in the queue, and `--now`, with the holder's two minutes. Tried: two sessions waiting got the game in the order they asked; one closed while waiting left the queue; the player's `--now` took the game from a holder with a running game.

### G3 Starting and stopping

`start` and `stop`, the start always hidden and ended when it hangs. Tried: a start to the main menu, one that hangs.

### G4 The development commands on the claim

`perf-run` without vanilla, through remote control; `perf-run` and `leak-run` started through the claim. Tried: one run of each.

### G5 The skill

The skill renamed game-test, rewritten for the commands, `game-lock.ps1` removed; AGENTS.md's testing section pointing at it.
