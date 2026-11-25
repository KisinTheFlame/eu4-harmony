# Working in this repository

These rules hold for everyone who changes this repository, people and coding agents alike. What the project is and how it is laid out is in [README.md](README.md).

## Language

- Code, comments, documentation, commit messages, logs and the startup report are in English.
- Chinese appears only in [README.zh-CN.md](README.zh-CN.md), in localisation files (`*_l_simp_chinese.yml`), in what the Steam Workshop shows in that language (`workshop/*.schinese.*`; each language's files there are in that language), and as data: test inputs and in-game text quoted in notes. When you change README.md, change README.zh-CN.md to match, and the same for each language's file in `workshop/`.
- In Chinese localisation files, use corner brackets (`「」`, `『』`) for quotation marks, never single or double quotes (`“”`, `‘’`, `"`, `'`).
- Use the terms in [GLOSSARY.md](GLOSSARY.md), and none of the words it lists under _Avoid_.
- Spelling is American (color, neighbor, initialize), as in the code, except "localisation", which follows the game.

## Documentation

- [README.md](README.md) and its Chinese counterpart introduce the project, installation and first steps. Update them when that guidance changes, not for every visible change. Capability reference comes from `eu4-harmony wiki`; procedures live in the shipped [skills](cli/skills/); implementation belongs in code comments.
- [docs/README.md](docs/README.md) links useful entry pages, not every document. [docs/game/](docs/game/) holds the game's known logic, organized by system and topic, with RVAs, callers, state, ownership and explicit unknowns. System entries navigate rather than repeat their topics.
- [docs/research/](docs/research/) keeps independently useful investigation and measurement evidence. Extract established game logic into its topic; do not leave it buried in an incident narrative or duplicate it in both places.
- [docs/plans/](docs/plans/) keeps open work and constraints. A larger change may need a short plan; completed plans can be consolidated after useful conclusions, decisions, evidence and remaining work have been retained and references repaired.
- [docs/adr/](docs/adr/) records costly, non-obvious decisions with real alternatives. Use `YYYY-MM-DD-<decision>.md`, without numbering; cite `ADR <decision>`. Preserve merged ADR names; a changed decision gets a superseding ADR.
- Ordinary builds, unit tests and routine regressions are reported in the PR description. Do not add permanent run transcripts or screenshots for them. Keep only durable findings the code/tests do not already preserve, summarized by topic with setup, results, significant failures and limits. Keep an image only if it supplies irreplaceable evidence. No per-run document or central run list is required.

## Building and checking

```powershell
cargo fmt --all
cargo build --release
cargo test
cargo test --features eu4-harmony-core/dev
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features eu4-harmony-core/dev-test-games -- -D warnings
cargo clippy --workspace --all-targets --features eu4-harmony-core/dev -- -D warnings
```

The tests are built in the dev profile, which compiles incrementally: after a change to the core they are ready in seconds, where the release profile optimizes the whole core again, its test build alone for more than a minute, and they pass the same in both. CI runs them in the release profile, as a player's build is built, with no debug assertions or overflow checks; the offline checks below do too, since one of them times the decoding of a track.

The additional clippy checks cover the two development features: `dev-test-games` (test-game pipes only) and `dev` (which includes `dev-test-games` and all development tools, configured in `dev.toml`; [docs/dev-tools.md](docs/dev-tools.md) lists them). A player's build has neither of them.

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
- Understand the game's code before reasoning about changing it: what it works out, why, what it reads/writes, who calls it and how often. Read it to its end and through relevant callees. Update the corresponding [game topic](docs/game/) with established logic and explicit limits. Whether stretches can run side by side is argued from their reads, writes and shared state, not runtime tricks. Retain a separate research note only when its evidence has independent value.
- Look a game address up before reading it (`cargo xtask what <rva | terms>`), then read its code with `cargo xtask disasm` or `cargo xtask decompile` ([.claude/skills/game-analysis](.claude/skills/game-analysis/SKILL.md)). [docs/game/addresses.toml](docs/game/addresses.toml) links only game topics, including nested ones. Record learned names and update the index with `cargo xtask addresses`; consolidation must preserve existing addresses and names, including partial leads.
- A setting in the interface says what it does in its label, with no explanation beneath it: explanations go unread and go stale. Where a label cannot say it, change the label. What the player must know of a change (that it takes effect at the next start) shows where and while it applies.

## Testing in the game

- Try things in a test game of your own ([docs/dev-tools.md#test-games](docs/dev-tools.md#test-games)): claim it before you use it, start it only through the claim and hidden unless the user asks to see it, and release it as soon as you are done. [.claude/skills/game-test](.claude/skills/game-test/SKILL.md) says how.
- A test game is started from the game folder with a user folder, a core and a pipe of its own: it never touches the player's files or the framework installed in the game folder, and never ends a game that is not its own. Never install into the game folder while the player or a test game runs, and only when the user asks.
- Leave nothing behind: scratch files, and test games still held.
- Report routine validation in the PR; retain durable evidence only under the documentation rules above.

## Commits

One change per commit. The summary line says what the change does, in plain words without a prefix ("Keep the windows mods may splice into in one table"); the body says why, when that is not obvious.

Changes reach master through pull requests; master takes no direct pushes. A pull request is merged with a merge commit, which keeps its commits as they are and takes the pull request's title and description as its summary and body, so the title follows the same rule as a summary line.
