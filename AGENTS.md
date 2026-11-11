# Working in this repository

These rules hold for everyone who changes this repository, people and coding agents alike. What the project is and how it is laid out is in [README.md](README.md).

## Language

- Code, comments, documentation, commit messages, logs and the startup report are in English.
- Chinese appears only in [README.zh-CN.md](README.zh-CN.md), in localisation files (`*_l_simp_chinese.yml`), in what the Steam Workshop shows in that language (`workshop/*.schinese.*`; each language's files there are in that language), and as data: test inputs and in-game text quoted in notes. When you change README.md, change README.zh-CN.md to match, and the same for each language's file in `workshop/`.
- Use the terms in [GLOSSARY.md](GLOSSARY.md), and none of the words it lists under _Avoid_.
- Spelling is American (color, neighbor, initialize), as in the code, except "localisation", which follows the game.

## Where things are written down

- [README.md](README.md): the front page, for someone who has not met the project: what it is, what it does, how to install it, a mod's first steps, where to read further. A change that players or mod authors notice gets a line there, or a row of its tables; the details, the full list of capabilities and the reasons go into the documents below and `eu4-harmony wiki`, and the README links to them.
- [docs/README.md](docs/README.md): every document in `docs/`, each with a line on what it holds.
- [docs/plans/](docs/plans/): a plan for each larger piece of work, with a row in docs/README.md. A plan whose parts are all built or set aside is a record, changed afterwards only in its status line; [its README](docs/plans/README.md) says what a plan holds.
- [docs/game/](docs/game/): what is known of the game, one page for each of its systems, in the game's terms; [its README](docs/game/README.md) says how a page is written.
- [docs/research/](docs/research/): the research the game's pages rest on, with addresses as RVAs into the 1.37.5 baseline. A note's first paragraph says what it looks into and what in the framework uses it.
- [docs/adr/](docs/adr/): decisions that are hard to reverse, one file each, named by the day it was written and a short slug of the decision (`2026-11-03-commands-run-at-once.md`), without a number, so that branches written side by side do not take the same one. An ADR is referred to by its slug alone, as `ADR commands-run-at-once`, and keeps its name once merged. A changed decision gets a new ADR that supersedes the old one, which stays.
- [docs/prototype-log/](docs/prototype-log/): what was tested in the running game and what came out, one file per run, named by the run's day and a short slug of its title (`2026-11-04-extension-reload.md`) and without a number, as ADRs are, so that branches written side by side each add a file of their own; its [README](docs/prototype-log/README.md) says what a run's file holds. Say what was not verified as plainly as what was.

## Building and checking

```powershell
cargo fmt --all
cargo build --release
cargo test
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features eu4-harmony-core/dev -- -D warnings
```

The tests are built in the dev profile, which compiles incrementally: after a change to the core they are ready in seconds, where the release profile optimizes the whole core again, its test build alone for more than a minute, and they pass the same in both. CI runs them in the release profile, as a player's build is built, with no debug assertions or overflow checks; the offline checks below do too, since one of them times the decoding of a track.

The second clippy is the developer build's: the development tools, each a feature of the core (`dev` has them all), are on when built in, with their options in `dev.toml`; [docs/dev-tools.md](docs/dev-tools.md) lists them. A player's build has none of them.

Clippy's warnings are fixed, not allowed. Where the code is right as it is, `#[expect(clippy::…, reason = "…")]` at that place says why.

With the game installed, the offline checks compare every dependency point's bytes with the game's `eu4.exe`, and check that each observer forwards at least the arguments its function reads:

```powershell
cargo xtask offline-check
```

It finds the game through `EU4_GAME_DIR` or Steam, as every development command does.

The Rust version is the one [rust-toolchain.toml](rust-toolchain.toml) names, which rustup picks up. CI (`.github/workflows/ci.yml`) checks the formatting on Linux and runs the build, the tests and clippy on Windows for every pull request and every merge into master, and builds the developer build and runs clippy on it; the offline checks need the game and are run by hand. CI passes `--locked`, so a `Cargo.lock` the build would change fails there: commit it with the change that changes it.

`cargo xtask install` installs the built proxy, core and command line tool into the game folder, where a player puts them (`--uninstall` removes them). `cargo xtask release` builds the player build in the `player` profile, the release build in one codegen unit, and packs the same files for players. `cargo xtask` is the repository's own program (`xtask/`), for what needs the repository; `cargo xtask --help` lists its commands, and [docs/dev-tools.md](docs/dev-tools.md) describes them with the other development tools.

## Design rules

- Design capabilities for any mod. The regional combat port lives in its own repository and must not shape the framework's interface ([ADR ports-live-outside-framework-repo](docs/adr/2026-10-27-ports-live-outside-framework-repo.md), [ADR extensions-use-capabilities-only](docs/adr/2026-10-27-extensions-use-capabilities-only.md)).
- Every place in the game the framework relies on is a dependency point, with the bytes expected there: a game function, a hook site, a field offset (checked against an instruction that uses it). A different game build then fails at startup instead of misbehaving, and startup succeeds or fails as a whole ([ADR framework-succeeds-or-fails-as-a-whole](docs/adr/2026-10-27-framework-succeeds-or-fails-as-a-whole.md)).
- Work things out from the game itself. Do not copy code from other EU4 patches or plugins, and do not name them in this repository.
- Understand the game's code before reasoning about changing it: what a function works out in the game's terms and why, what it reads and writes, who calls it and how often; read it to its end and through its callees, and write that into [docs/research/](docs/research/), and what it changes of what is known into the page of [docs/game/](docs/game/) it touches, in the same change. Whether two stretches of the game can run side by side is argued from what each reads, writes and shares, not from runtime tricks. Where the tools at hand cannot answer a question, say so and why.
- Look a game address up before reading it (`cargo xtask what <rva>`): what is known of it is in the notes the index of the game's addresses (`docs/research/addresses.toml`) points to. A name learned for an address goes into the index, with the note that says it; `cargo xtask addresses` adds the addresses a note newly names.
- A setting in the interface says what it does in its label, with no explanation beneath it: explanations go unread and go stale. Where a label cannot say it, change the label. What the player must know of a change (that it takes effect at the next start) shows where and while it applies.

## Testing in the game

- Try things in a test game of your own ([docs/plans/test-games.md](docs/plans/test-games.md)): claim it before you use it, start it only through the claim and hidden unless the user asks to see it, and release it as soon as you are done. [.claude/skills/game-test](.claude/skills/game-test/SKILL.md) says how.
- A test game is started from the game folder with a user folder, a core and a pipe of its own: it never touches the player's files or the framework installed in the game folder, and never ends a game that is not its own. Never install into the game folder while the player or a test game runs, and only when the user asks.
- Leave nothing behind: scratch files, and test games still held.
- Record the result in a file of its own in [docs/prototype-log/](docs/prototype-log/), one per run.

## Commits

One change per commit. The summary line says what the change does, in plain words without a prefix ("Keep the windows mods may splice into in one table"); the body says why, when that is not obvious.

Changes reach master through pull requests; master takes no direct pushes. A pull request is merged with a merge commit, which keeps its commits as they are and takes the pull request's title and description as its summary and body, so the title follows the same rule as a summary line.
