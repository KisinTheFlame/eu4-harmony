# Working in this repository

These rules hold for everyone who changes this repository, people and coding agents alike. What the project is and how it is laid out is in [README.md](README.md).

## Language

- Code, comments, documentation, commit messages, logs and the startup report are in English.
- Chinese appears only in [README.zh-CN.md](README.zh-CN.md), in localisation files (`*_l_simp_chinese.yml`), and as data: test inputs and in-game text quoted in notes. When you change README.md, change README.zh-CN.md to match.
- Use the terms in [GLOSSARY.md](GLOSSARY.md), and none of the words it lists under _Avoid_.
- Spelling is American (color, neighbor, initialize), as in the code, except "localisation", which follows the game.

## Where things are written down

- [README.md](README.md): the front page, for someone who has not met the project: what it is, what it does, how to install it, a mod's first steps, where to read further. A change that players or mod authors notice gets a line there, or a row of its tables; the details, the full list of capabilities and the reasons go into the documents below and `eu4-harmony wiki`, and the README links to them.
- [docs/v1.md](docs/v1.md): the design of the first version. [docs/unicode-module.md](docs/unicode-module.md): the Unicode module's plan, milestones and progress. [docs/remote-control.md](docs/remote-control.md): remote control's plan, its parts and what each delivers. [docs/sessions.md](docs/sessions.md): leaving a session without a restart, its plan and parts. [docs/lua-extensions.md](docs/lua-extensions.md): extensions in Lua instead of native DLLs, its plan and parts. [docs/leak-finding.md](docs/leak-finding.md): finding what the game's memory keeps of its sessions, its plan and parts. [docs/extension-reload.md](docs/extension-reload.md): reloading an extension in the running game, its plan and parts. [docs/commands.md](docs/commands.md): capabilities that change the game reaching it as commands, and developer builds, its plan and parts. [docs/input.md](docs/input.md): remote clients giving the game mouse and keyboard input, its plan and parts. [docs/game-speed.md](docs/game-speed.md): making the game faster without changing the game it plays, its plan and parts. [docs/dev-commands.md](docs/dev-commands.md): the development tools as commands of `cargo xtask` and the command line tool instead of scripts, its plan and parts. [docs/hidden-start.md](docs/hidden-start.md): starting the game with nothing of it on the player's desktop, its plan and parts. [docs/game-testing.md](docs/game-testing.md): one way for every session to try things in the game, the claim as commands with a fair queue and the player first, its plan and parts. [docs/player-actions.md](docs/player-actions.md): extensions and remote clients doing in the game's panels what the player does, its plan and parts; [docs/adding-a-capability.md](docs/adding-a-capability.md): how a capability is added, layer by layer.
- [docs/adr/](docs/adr/): decisions that are hard to reverse, one file each, named by a short slug of the decision (`commands-run-at-once.md`) and without a number, so that branches written side by side do not take the same one. An ADR is referred to by its slug, as `ADR commands-run-at-once`, and keeps it once merged. A changed decision gets a new ADR that supersedes the old one, which stays.
- [docs/research/](docs/research/): what is known about the game's code, with addresses as RVAs into the 1.37.5 baseline.
- [docs/prototype-log.md](docs/prototype-log.md): what was tested in the running game and what came out, one dated section per run. Say what was not verified as plainly as what was.
- [docs/performance.md](docs/performance.md): what the framework costs the game, as measured with `cargo xtask perf-run`.
- [docs/dev-tools.md](docs/dev-tools.md): the development tools, what each is for and how to use it.

## Building and checking

```powershell
cargo fmt --all
cargo build --release
cargo test --release
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features eu4-harmony-core/dev -- -D warnings
```

The second clippy is the developer build's: the development tools, each a feature of the core (`dev-call-trace`, `dev-heap-census`, `dev-command-log`, `dev-test-switches`; `dev` has them all), are on when built in, with their options in `dev.toml` ([docs/dev-tools.md](docs/dev-tools.md)). A player's build has none of them.

Clippy's warnings are fixed, not allowed. Where the code is right as it is, `#[expect(clippy::…, reason = "…")]` at that place says why.

With the game installed, the offline checks compare every dependency point's bytes with the game's `eu4.exe`, and check that each observer forwards at least the arguments its function reads:

```powershell
cargo xtask offline-check
```

It finds the game through `EU4_GAME_DIR` or Steam, as every development command does.

The Rust version is the one [rust-toolchain.toml](rust-toolchain.toml) names, which rustup picks up. CI (`.github/workflows/ci.yml`) checks the formatting and runs the build, the tests and clippy on Windows for every pull request and every merge into master, and builds the developer build and runs clippy on it; the offline checks need the game and are run by hand.

`cargo xtask install` installs the built proxy, core and command line tool into the game folder, where a player puts them (`--uninstall` removes them). `cargo xtask release` builds the player build and packs the same files for players. `cargo xtask` is the repository's own program (`xtask/`), for what needs the repository; `cargo xtask --help` lists its commands, and [docs/dev-tools.md](docs/dev-tools.md) describes them with the other development tools.

## Design rules

- Design capabilities for any mod. The regional combat port lives in its own repository and must not shape the framework's interface ([ADR ports-live-outside-framework-repo](docs/adr/ports-live-outside-framework-repo.md), [ADR extensions-use-capabilities-only](docs/adr/extensions-use-capabilities-only.md)).
- Every place in the game the framework relies on is a dependency point, with the bytes expected there: a game function, a hook site, a field offset (checked against an instruction that uses it). A different game build then fails at startup instead of misbehaving, and startup succeeds or fails as a whole ([ADR framework-succeeds-or-fails-as-a-whole](docs/adr/framework-succeeds-or-fails-as-a-whole.md)).
- Work things out from the game itself. Do not copy code from other EU4 patches or plugins, and do not name them in this repository.

## Testing in the game

- One game serves every checkout on the machine. Claim it before you use it, and release it after, with `cargo xtask game` as [.claude/skills/game-test](.claude/skills/game-test/SKILL.md) says, and start it only through the claim, hidden ([docs/game-testing.md](docs/game-testing.md)).
- The game rewrites a save it loads and rotates its autosaves, rewrites its settings files when it applies its settings, and rewrites its login cache; a run installs into the game folder. The claim backs up the player's files (`eu4-harmony backup`) and the game folder's framework files, and the release puts every file back and compares each (`eu4-harmony restore`), removing the run's crash folders.
- Leave nothing behind: scratch files, and what `restore` reports it could not put back.
- Record the result in [docs/prototype-log.md](docs/prototype-log.md).

## Commits

One change per commit. The summary line says what the change does, in plain words without a prefix ("Keep the windows mods may splice into in one table"); the body says why, when that is not obvious.

Changes reach master through pull requests; master takes no direct pushes. A pull request is merged with a merge commit, which keeps its commits as they are and takes the pull request's title and description as its summary and body, so the title follows the same rule as a summary line.
