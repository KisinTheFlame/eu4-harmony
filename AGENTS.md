# Working in this repository

These rules hold for everyone who changes this repository, people and coding agents alike. What the project is and how it is laid out is in [README.md](README.md).

## Language

- Code, comments, documentation, commit messages, logs and the startup report are in English.
- Chinese appears only in [README.zh-CN.md](README.zh-CN.md), in localisation files (`*_l_simp_chinese.yml`), and as data: test inputs and in-game text quoted in notes. When you change README.md, change README.zh-CN.md to match.
- Use the terms in [GLOSSARY.md](GLOSSARY.md), and none of the words it lists under _Avoid_.
- Spelling is American (color, neighbor, initialize), as in the code, except "localisation", which follows the game.

## Where things are written down

- [docs/v1.md](docs/v1.md): the design of the first version. [docs/unicode-module.md](docs/unicode-module.md): the Unicode module's plan, milestones and progress. [docs/remote-control.md](docs/remote-control.md): remote control's plan, its parts and what each delivers.
- [docs/adr/](docs/adr/): decisions that are hard to reverse. A new decision gets the next number; a changed decision gets a new ADR that supersedes the old one, which stays.
- [docs/research/](docs/research/): what is known about the game's code, with addresses as RVAs into the 1.37.5 baseline.
- [docs/prototype-log.md](docs/prototype-log.md): what was tested in the running game and what came out, one dated section per run. Say what was not verified as plainly as what was.

## Building and checking

```powershell
cargo build --release
cargo test --release
```

With the game installed, the offline checks compare every dependency point's bytes with the game's `eu4.exe`, and check that each observer forwards at least the arguments its function reads:

```bash
EU4_GAME_DIR='E:\SteamLibrary\steamapps\common\Europa Universalis IV' cargo test --release -p eu4-harmony-core -- --ignored
```

The Rust version is the one [rust-toolchain.toml](rust-toolchain.toml) names, which rustup picks up. CI (`.github/workflows/ci.yml`) runs the build and the tests on Windows for every pull request and every merge into master; the offline checks need the game and are run by hand.

`tools/dev-install.ps1` installs the built proxy, core and command line tool into the game folder, where a player puts them (`-Uninstall` removes them); like the other tools it finds the game through `EU4_GAME_DIR` or Steam. `tools/release.ps1` packs the same files for players. The other development tools are described in [tools/README.md](tools/README.md).

## Design rules

- Design capabilities for any mod. The regional combat port lives in its own repository and must not shape the framework's interface ([ADR 0004](docs/adr/0004-ports-live-outside-framework-repo.md), [ADR 0005](docs/adr/0005-extensions-use-capabilities-only.md)).
- Every place in the game the framework relies on is a dependency point, with the bytes expected there: a game function, a hook site, a field offset (checked against an instruction that uses it). A different game build then fails at startup instead of misbehaving, and startup succeeds or fails as a whole ([ADR 0012](docs/adr/0012-framework-succeeds-or-fails-as-a-whole.md)).
- Work things out from the game itself. Do not copy code from other EU4 patches or plugins, and do not name them in this repository.

## Testing in the game

- The game rewrites a save it loads and rotates its autosaves, rewrites its settings files when it applies its settings, and rewrites its login cache. Before a run that may load or write a save, copy these from `Documents\Paradox Interactive\Europa Universalis IV\` to somewhere else: `save games\`, `eu4-harmony\saves\`, `settings.txt`, `pdx_settings.txt`, `pdx_login_cache.txt`, `shortcuts.txt`, `continue_game.json` and `eu4-harmony\settings.txt`. Afterwards delete the files the run added, restore the copies and compare them with the backups.
- Leave nothing behind: crash folders the run produced (`crashes\`), scratch files, the backups themselves.
- Record the result in [docs/prototype-log.md](docs/prototype-log.md).

## Commits

One change per commit. The summary line says what the change does, in plain words without a prefix ("Keep the windows mods may splice into in one table"); the body says why, when that is not obvious.

Changes reach master through pull requests; master takes no direct pushes. A pull request is merged with a merge commit, which keeps its commits as they are and takes the pull request's title and description as its summary and body, so the title follows the same rule as a summary line.
