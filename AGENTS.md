# Working in this repository

These rules hold for everyone who changes this repository, people and coding agents alike. What the project is and how it is laid out is in [README.md](README.md).

## Language

- Code, comments, documentation, commit messages, logs and the startup report are in English.
- Chinese appears only in [README.zh-CN.md](README.zh-CN.md), in localisation files (`*_l_simp_chinese.yml`), and as data: test inputs and in-game text quoted in notes. When you change README.md, change README.zh-CN.md to match.
- Use the terms in [GLOSSARY.md](GLOSSARY.md), and none of the words it lists under _Avoid_.
- Spelling is American (color, neighbor, initialize), as in the code, except "localisation", which follows the game.

## Where things are written down

- [README.md](README.md): the front page, for someone who has not met the project: what it is, what it does, how to install it, a mod's first steps, where to read further. A change that players or mod authors notice gets a line there, or a row of its tables; the details, the full list of capabilities and the reasons go into the documents below and `eu4-harmony wiki`, and the README links to them.
- [docs/v1.md](docs/v1.md): the design of the first version. [docs/unicode-module.md](docs/unicode-module.md): the Unicode module's plan, milestones and progress. [docs/remote-control.md](docs/remote-control.md): remote control's plan, its parts and what each delivers. [docs/sessions.md](docs/sessions.md): leaving a session without a restart, its plan and parts. [docs/lua-extensions.md](docs/lua-extensions.md): extensions in Lua instead of native DLLs, its plan and parts. [docs/leak-finding.md](docs/leak-finding.md): finding what the game's memory keeps of its sessions, its plan and parts. [docs/extension-reload.md](docs/extension-reload.md): reloading an extension in the running game, its plan and parts. [docs/commands.md](docs/commands.md): capabilities that change the game reaching it as commands, and developer builds, its plan and parts. [docs/input.md](docs/input.md): remote clients giving the game mouse and keyboard input, its plan and parts. [docs/game-speed.md](docs/game-speed.md): making the game faster without changing the game it plays, its plan and parts. [docs/loading.md](docs/loading.md): a shorter startup and load, the data loaded the same, its plan and parts. [docs/dev-commands.md](docs/dev-commands.md): the development tools as commands of `cargo xtask` and the command line tool instead of scripts, its plan and parts. [docs/hidden-start.md](docs/hidden-start.md): starting the game with nothing of it on the player's desktop, its plan and parts. [docs/game-testing.md](docs/game-testing.md): the claim of the whole game as commands, now replaced by test games, its plan and parts as they were. [docs/player-actions.md](docs/player-actions.md): extensions and remote clients doing in the game's panels what the player does, its plan and parts; [docs/adding-a-capability.md](docs/adding-a-capability.md): how a capability is added, layer by layer. [docs/scenarios.md](docs/scenarios.md): scenarios, the runs of the real game that one runner plays and reports on, its plan and parts. [docs/player-language.md](docs/player-language.md): the player language chosen out of twelve, and fonts that follow it, its plan and parts. [docs/music-volume.md](docs/music-volume.md): the volume sliders in decibels and the music mixed by the framework, its plan and parts. [docs/observe-mode.md](docs/observe-mode.md): remote control and extensions in observe mode, a country in view and acting as it, its plan and parts. [docs/test-games.md](docs/test-games.md): test games, several at once, each with a user folder, a core and a pipe of its own, which replace the claim of the whole game, its plan and parts. [docs/deepening.md](docs/deepening.md): knowledge written out in several places gathered into one module each, its plan and parts.
- [docs/adr/](docs/adr/): decisions that are hard to reverse, one file each, named by a short slug of the decision (`commands-run-at-once.md`) and without a number, so that branches written side by side do not take the same one. An ADR is referred to by its slug, as `ADR commands-run-at-once`, and keeps it once merged. A changed decision gets a new ADR that supersedes the old one, which stays.
- [docs/research/](docs/research/): what is known about the game's code, with addresses as RVAs into the 1.37.5 baseline.
- [docs/prototype-log/](docs/prototype-log/): what was tested in the running game and what came out, one file per run, named by the run's day and a short slug of its title (`2026-11-04-extension-reload.md`) and without a number, as ADRs are, so that branches written side by side each add a file of their own; its [README](docs/prototype-log/README.md) says what a run's file holds. Say what was not verified as plainly as what was.
- [docs/performance.md](docs/performance.md): what the framework costs the game, as measured in the running game; `cargo xtask scenario perf` and `cargo xtask profile` take new measurements.
- [docs/dev-tools.md](docs/dev-tools.md): the development tools, what each is for and how to use it.

## Building and checking

```powershell
cargo fmt --all
cargo build --release
cargo test --release
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features eu4-harmony-core/dev -- -D warnings
```

The second clippy is the developer build's: the development tools, each a feature of the core (`dev-call-trace`, `dev-memory`, `dev-heap-census`, `dev-command-log`, `dev-test-switches`, `dev-write-watch`; `dev` has them all), are on when built in, with their options in `dev.toml` ([docs/dev-tools.md](docs/dev-tools.md)). A player's build has none of them.

Clippy's warnings are fixed, not allowed. Where the code is right as it is, `#[expect(clippy::…, reason = "…")]` at that place says why.

With the game installed, the offline checks compare every dependency point's bytes with the game's `eu4.exe`, and check that each observer forwards at least the arguments its function reads:

```powershell
cargo xtask offline-check
```

It finds the game through `EU4_GAME_DIR` or Steam, as every development command does.

The Rust version is the one [rust-toolchain.toml](rust-toolchain.toml) names, which rustup picks up. CI (`.github/workflows/ci.yml`) checks the formatting on Linux and runs the build, the tests and clippy on Windows for every pull request and every merge into master, and builds the developer build and runs clippy on it; the offline checks need the game and are run by hand. CI passes `--locked`, so a `Cargo.lock` the build would change fails there: commit it with the change that changes it.

`cargo xtask install` installs the built proxy, core and command line tool into the game folder, where a player puts them (`--uninstall` removes them). `cargo xtask release` builds the player build and packs the same files for players. `cargo xtask` is the repository's own program (`xtask/`), for what needs the repository; `cargo xtask --help` lists its commands, and [docs/dev-tools.md](docs/dev-tools.md) describes them with the other development tools.

## Design rules

- Design capabilities for any mod. The regional combat port lives in its own repository and must not shape the framework's interface ([ADR ports-live-outside-framework-repo](docs/adr/ports-live-outside-framework-repo.md), [ADR extensions-use-capabilities-only](docs/adr/extensions-use-capabilities-only.md)).
- Every place in the game the framework relies on is a dependency point, with the bytes expected there: a game function, a hook site, a field offset (checked against an instruction that uses it). A different game build then fails at startup instead of misbehaving, and startup succeeds or fails as a whole ([ADR framework-succeeds-or-fails-as-a-whole](docs/adr/framework-succeeds-or-fails-as-a-whole.md)).
- Work things out from the game itself. Do not copy code from other EU4 patches or plugins, and do not name them in this repository.

## Testing in the game

- Try things in a test game of your own: claim one before you use it and release it after, with `cargo xtask game` as [.claude/skills/game-test](.claude/skills/game-test/SKILL.md) says, and start it only through the claim, hidden unless the user asks to see it ([docs/test-games.md](docs/test-games.md)). A few run at once and the rest wait, so release one you are done with.
- A test game is started from the game folder with a user folder, a core and a pipe of its own: it never touches the player's files or the framework installed in the game folder, and never ends a game that is not its own. Never install into the game folder while the player or a test game runs, and only when the user asks.
- Leave nothing behind: scratch files, and what `restore` reports it could not put back.
- Record the result in a file of its own in [docs/prototype-log/](docs/prototype-log/), one per run.

## Commits

One change per commit. The summary line says what the change does, in plain words without a prefix ("Keep the windows mods may splice into in one table"); the body says why, when that is not obvious.

Changes reach master through pull requests; master takes no direct pushes. A pull request is merged with a merge commit, which keeps its commits as they are and takes the pull request's title and description as its summary and body, so the title follows the same rule as a summary line.
