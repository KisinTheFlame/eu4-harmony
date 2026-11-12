![Harmony for Europa Universalis IV](docs/images/banner.jpg)

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

English | [简体中文](README.zh-CN.md)

**Europa Universalis IV, made better from within.** Chinese, Japanese and Korean everywhere, no restarts, a faster game, fewer bugs, and mods that can do far more, with the game's rules left as they are.

| | Unmodified game | Harmony |
| --- | :-: | :-: |
| Chinese, Japanese and Korean display correctly, with no double-byte patch or font mod | ✗ | ✓ |
| Back to the main menu in seconds, without restarting the game | ✗ | ✓ |
| Load another save from an ironman game | ✗ | ✓ |
| The map scrolls and zooms smoothly, even at top speed, with core hover highlights preserved | ✗ | ✓ |
| Saving pauses the game a third as long | ✗ | ✓ |
| The game starts and loads saves sooner | ✗ | ✓ |
| A faster late game | ✗ | ✓ |
| Heirs no longer get the same name half the time | ✗ | ✓ |
| Fewer crashes | ✗ | ✓ |
| Loading a save no longer costs you achievements | ✗ | ✓ |
| A long game no longer eats ever more memory | ✗ | ✓ |
| Optionally, achievements outside ironman or with mods | ✗ | ✓ |
| Mods that do far more: buttons, windows, acting for you | ✗ | ✓ |
| **The game's rules and data unchanged** | ✓ | ✓ |

Your translation mod is still what translates the game; translation mods made for the old double-byte patch, and the saves played with them, work as they are. Harmony runs on EU4 1.37.5 for Windows, in single player.

## Installing

1. If you use a patch that makes the game display Chinese, Japanese or Korean, uninstall it first, as that patch says.
2. Subscribe to **Harmony** on the Steam Workshop.
3. In Steam, right-click Europa Universalis IV and choose **Manage → Browse local files**: this opens the game's folder.
4. Open Harmony's Workshop folder: `steamapps\workshop\content\236850\<id>\` in the same Steam library, two folders up from the game's, where `<id>` is the number at the end of Harmony's Workshop page address. Copy `version.dll` from there into the game's folder, next to `eu4.exe`. If the game's folder already has a `version.dll`, move that one somewhere else first; Harmony does not load it.

Start the game from the Paradox launcher as usual. Harmony needs no place in your playset; mods built on it are enabled there as any mod. The main menu now has two buttons in its corner, **Harmony settings** and **Harmony mods**; the second lists each mod built on Harmony, with what it may do beyond the game.

Steam keeps Harmony up to date. Once in a while an update needs a new `version.dll`; Harmony then tells you at startup to copy it again.

**Uninstalling:** unsubscribe from Harmony, and the game runs as it always did. To leave nothing behind, delete `version.dll` from the game's folder too, and put back the one you moved, if any.

Good to know:

- Names you type in a game played with Harmony are saved as UTF-8, and show as garbage without it.
- A mod that keeps data of its own writes it into the save; such a save is not promised to load without Harmony.
- With Harmony, the game's volume sliders go as the square of their position, so each step sounds about as large. Volumes saved below 100 sound quieter than before; set them once again.

## When something goes wrong

In the main menu, open Harmony's settings, go to **About** and press **Save troubleshooting files to the desktop**: it packs Harmony's logs, the game's logs, its last crash folders and your settings and playset (no saves) into one zip file on your desktop. Report a problem on [GitHub Issues](https://github.com/KisinTheFlame/eu4-harmony/issues), with that file attached.

The logs are in `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\`: `report.txt` says what Harmony found and did as the game started, `core.log` records its work and crashes that reach the game's crash handling, and `proxy.log`, there only when something went wrong, says why Harmony did not start at all. Framework logs rotate in bounded segments, count repeated errors and keep a small recent-event snapshot; inactive owned logs expire automatically. Game logs, saves, crash dumps and your saved troubleshooting files are left alone. Large text logs contribute a marked tail to troubleshooting files. [Log limits and diagnostic coverage](docs/logs.md) describe what is kept and what an unexplained exit may still leave unknown.

## Writing a mod

A mod's extension reads the game and acts in it through capabilities, never through memory addresses:

```lua
-- eu4-harmony/lua/main.lua: a button, declared in eu4-harmony/ui/, that shows in the
-- army panel while one of your armies is selected
return {
  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

  on_button = function(game)
    log("army " .. tostring(game.army.selected()) .. " clicked")
  end,
}
```

The command line tool, `eu4-harmony.exe`, is built from source for now ([Building from source](#building-from-source)):

```powershell
eu4-harmony new my-mod --name "My Mod"   # a working mod with a button, texts in English and your language, and editor types
cd my-mod
eu4-harmony check                         # read everything as the framework will
eu4-harmony test                          # run eu4-harmony\tests\*.lua against a fake game
eu4-harmony link                          # register it with the launcher, then enable it in your playset
eu4-harmony reload --watch                # with the game running, reload the extension whenever a file changes
eu4-harmony types                         # write the editor types again, after the tool is updated
eu4-harmony package                       # a clean folder ready to upload
```

A mod is an ordinary Paradox mod with an `eu4-harmony\` folder, where a file's place says what it is:

```text
my-mod/
├── descriptor.mod
├── .eu4-harmony/types/        # editor types, written by eu4-harmony new and eu4-harmony types
├── .luarc.json                # points the editor at them
└── eu4-harmony/
    ├── manifest.toml          # id, api_version, description, permissions
    ├── lua/main.lua           # the extension: a table of handlers
    ├── ui/<window>/buttons/   # interface fragments spliced into vanilla windows
    ├── localisation/          # texts, one UTF-8 file per language
    ├── data/                  # files the extension reads
    └── tests/                 # tests run by eu4-harmony test
```

`main.lua` returns handlers, such as `on_day` for a day passing and `on_button` for a click on its button. Each gets `game`, through which every capability is called; a refusal returns `nil, err` with the game's reason. A handler can wait for an answer, such as another country's to a proposal, or for days to pass, and do several things at once as tasks it awaits. An extension that needs the real time, files or the network declares it under `permissions` in its manifest, and players see that in the Harmony mods window.

`province.area` reads an owned area's members and governing costs, including the capacity added by making it a state without completing cores; [the province page](docs/game/provinces.md) explains the distinction.

An extension window's text can show a tooltip, set with `game.window.<window>.<text>.tooltip`; the [window guide](cli/skills/ui/windows.md) covers layouts and properties.

Where to learn more:

- `eu4-harmony wiki`: every capability with its arguments and results, the handlers and globals (`wiki lua`), the manifest (`wiki manifest`). It needs no game.
- `eu4-harmony skills`: step-by-step guides for each kind of work (`skills mod` for making a mod, `skills ui`, `skills testing`, …), written for coding agents and readable by anyone.
- [docs/plans/mod-capabilities.md](docs/plans/mod-capabilities.md): what the framework opens to mods, area by area.
- [docs/plans/lua-extensions.md](docs/plans/lua-extensions.md): how extensions work, with errors, the time and memory limits and save data.

## Remote control: let a program play

A program, an AI among them, can play the whole game as a player would, even hidden in the background. `eu4-harmony game <capability> [<arguments>]` sends the running game one request and prints the answer as JSON:

```powershell
eu4-harmony start --hidden                 # start the game with nothing on the desktop; Steam must be running
eu4-harmony game menu start FRA            # a new game as France
eu4-harmony game time advance 30           # let 30 days pass; stops early for an event to answer
eu4-harmony game question list             # the events waiting for the player
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # back to the main menu
```

In observe mode, the AI playing every country, `menu start` with no country starts such a game, `session view FRA` puts France in view and `session leave_view` takes it out; what acts as a country then acts as the one in view while the AI goes on playing it, and with none in view fails with `no_country` ([docs/plans/observe-mode.md](docs/plans/observe-mode.md)).

Remote control is open only to the Windows user playing, only on this computer. A request acts as the player: it reads what the player sees and does only what the player could. With `--as-game` it acts as the game instead: it reads everything the game keeps, and may also change the game as the mods' scripts do, which no player could ([ADR requests-act-as-the-player-or-as-the-game](docs/adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md)). Where no capability reaches, it can use the player's mouse and keyboard. `eu4-harmony wiki remote` explains how a game goes and what each exit code means; [docs/plans/remote-control.md](docs/plans/remote-control.md) and [docs/plans/hidden-start.md](docs/plans/hidden-start.md) have the design.

## Documentation

| | |
| --- | --- |
| [docs/plans/v1.md](docs/plans/v1.md) | The design of the first version |
| [GLOSSARY.md](GLOSSARY.md) | The terms the project uses |
| [docs/adr/](docs/adr/) | Decisions that are hard to reverse, and why they were made |
| [docs/game/](docs/game/) | What is known of the game, one page for each of its systems |
| [docs/research/](docs/research/) | The research into the game's code those pages rest on |
| [docs/performance.md](docs/performance.md) | What the framework costs the game, measured |
| [docs/prototype-log/](docs/prototype-log/) | What was tested in the running game, and what came out, one file per run |
| [docs/dev-tools.md](docs/dev-tools.md) | The development tools |

Each larger piece of work has a plan of its own in [docs/plans/](docs/plans/), listed in [docs/README.md](docs/README.md) with every other document.

## Building from source

You need Windows, [rustup](https://rustup.rs/) (it picks up the Rust version [rust-toolchain.toml](rust-toolchain.toml) names) and the Visual Studio C++ build tools. The game is needed only to run it and for the offline checks.

```powershell
cargo build --release
cargo test
cargo xtask release        # pack the player package and the Workshop item's folder
```

`cargo xtask release` packs `target\eu4-harmony\`, a folder holding `version.dll` and an `eu4-harmony\` folder with the core and the command line tool, and a zip of it. To install a build by hand, copy both into the game's folder, next to `eu4.exe`; `cargo xtask install` does that, and `--uninstall` removes them. A core in the game's folder is loaded before the one from the Workshop, so a build installed this way is the one that runs.

The game is found through `EU4_GAME_DIR` or Steam. To try the build in the game, claim a test game: a game started from the game folder with a user folder, a core and a remote control pipe of its own, so that it touches neither the player's files nor the framework installed in the game folder, and several run side by side ([docs/plans/test-games.md](docs/plans/test-games.md)). The game folder needs a framework from test games on, installed once with the player's game closed (`cargo xtask install`).

```powershell
cargo build --release --features eu4-harmony-core/dev-test-games
cargo xtask game claim --owner mine --purpose "try my change"
cargo xtask game start --owner mine     # start the test game, hidden, with this build
eu4-harmony game --test-game mine session get
cargo xtask game release --owner mine   # keep its logs, remove the rest
```

The framework takes the game's own `-userdir` for the user folder, as the game does.

`cargo xtask --help` lists the other development commands.

## Contributing

Read [AGENTS.md](AGENTS.md) first: it has the rules for code, commits, checks and testing in the game, for people and coding agents alike. Changes reach master through pull requests.

| Folder | Contents |
| --- | --- |
| `proxy/` | The `version.dll` that loads the core into `eu4.exe` |
| `core/` | The framework: dependency points and startup, the capabilities, the Lua host on the live game, remote control, the framework UI and the Unicode module |
| `lua/` | The Lua host shared by the game and the fake game: sandbox, handlers, tasks, time limit, what extensions hear in which order, and what they want of their windows and pop-ups |
| `remote/` | The capability catalogue: every capability's arguments, results and description |
| `checks/` | The checks a capability's call goes through, and the game's calendar |
| `client/` | Reaching the running game from outside: remote control's pipe, the game's process, started, followed to its exit code and ended, and backing up and restoring the player's files, shared by the tool and `cargo xtask` |
| `folders/` | The game folder and the user folder, as every program finds them, and the names of what the user folder holds |
| `logging/` | Bounded framework log files, repeated-error summaries, event snapshots and credential redaction |
| `loc/`, `manifest/` | Localisation files and mod manifests, shared by the core and the tool |
| `cli/` | `eu4-harmony.exe`, and the template mod `new` writes |
| `xtask/` | The repository's development commands (`cargo xtask`) |

## License

[MIT](LICENSE). It covers this repository's own code and documents, not the game: what you may do with a mod is still up to Paradox's terms for mods.
