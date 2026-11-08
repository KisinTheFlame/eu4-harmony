# EU4 Harmony

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

English | [简体中文](README.zh-CN.md)

**Europa Universalis IV, made better from within.** Chinese everywhere, no restarts, a faster game, fewer bugs, and mods that can do far more.

> [!NOTE]
> Early development: EU4 1.37.5 on Windows, with every DLC, single player only. No release yet; [build it from source](#building-from-source).

## Why EU4 Harmony

- **Every language, everywhere.** Every script shows in the whole game, in fonts made for your language or ones you choose. No font mod needed.
- **No more restarts.** Back to the main menu in seconds instead of 45. Ironman games can load another save too.
- **A faster game, the same game.** Smoother, about a tenth faster at top speed, a save stops it for a third of the time, and it starts and loads sooner; late in a game the AI's armies plan on idle cores. The game's rules and data stay as they are.
- **Bugs fixed that vanilla still has.** Heirs no longer get the same name half the time, coalitions and an overlord's fall no longer risk a crash, and loading a save no longer costs you achievements.
- **Achievements your way.** Optionally earn them outside ironman, or with mods.
- **Mods that do more.** Mods can read the game, add buttons and do what you do in its panels (build, develop, research, take ideas and reforms, trade, wage war and make peace, every diplomatic action), each with the game's own reasons when it is not allowed, with a few lines of Lua.
- **Let an AI play.** A program can play the whole game for you, as you would, even hidden in the background.

## Installing (players)

1. Get the player package: a folder holding `version.dll` and an `eu4-harmony\` folder. Until there is a release, build it with `cargo xtask release` ([Building from source](#building-from-source)).
2. If the game's root folder already has a `version.dll`, move it into `eu4-harmony\backup\`; the framework does not load it.
3. Copy `version.dll` and `eu4-harmony\` into the game's root folder, next to `eu4.exe`.
4. Start the game from the Paradox launcher as usual, and enable EU4 Harmony mods in your playset.

The main menu now has two buttons in its corner: **EU4 Harmony settings** and **EU4 Harmony mods**, where you see each mod's name, version and what it may do beyond the game (its permissions).

**Uninstalling:** delete `version.dll` and `eu4-harmony\` from the root folder, and move back the `version.dll` you backed up, if any.

Good to know:

- Names you type in a game played with the framework are saved as UTF-8, and show as garbage once the framework is removed.
- An extension that keeps data writes it into the save; such a save is not promised to load without the framework.
- Logs and the startup report (`report.txt`) are in `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`. Look there first when something goes wrong.

### Settings

**EU4 Harmony settings** has the player language and its fonts, returning to the main menu without restarting the game, achievement rules, game speed, what remote control may see and how even the music is; each setting says there what it does. The details are in [docs/player-language.md](docs/player-language.md), [docs/sessions.md](docs/sessions.md), [docs/game-speed.md](docs/game-speed.md), [docs/music-volume.md](docs/music-volume.md) and [ADR remote-control-reads-the-players-view](docs/adr/remote-control-reads-the-players-view.md).

With the framework, the game's own volume sliders go as the square of their position, so each step sounds about as large, where vanilla's last tenth fell off steeply: volumes saved below 100 sound quieter than before, and are set again once.

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

The command line tool is `eu4-harmony.exe`, in the game's `eu4-harmony\` folder:

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

`main.lua` returns handlers for events such as `on_session`, `on_day`, `on_frame` and `on_button`. Each gets `game`, through which every capability is called; a refusal returns `nil, err` with the game's reason, and a handler can wait for an answer, such as another country's to a proposal. An extension that needs the real time, files or the network declares it under `permissions` in its manifest.

Where to learn more:

- `eu4-harmony wiki`: every capability with its arguments and results, the handlers and globals (`wiki lua`), the manifest (`wiki manifest`). It needs no game.
- `eu4-harmony skills`: step-by-step guides for each kind of work (`skills mod` for making a mod, `skills ui`, `skills testing`, …), written for coding agents and readable by anyone.
- [docs/lua-extensions.md](docs/lua-extensions.md): how extensions work, with errors, the time and memory limits and save data.

## Remote control

`eu4-harmony game <capability> [<arguments>]` sends the running game one request and prints the answer as JSON:

```powershell
eu4-harmony start --hidden                 # start the game with nothing on the desktop; Steam must be running
eu4-harmony game menu start FRA            # a new game as France
eu4-harmony game time advance 30           # let 30 days pass; stops early for an event to answer
eu4-harmony game question list             # the events waiting for the player
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # back to the main menu
```

In observe mode, the AI playing every country, `menu start` with no country starts such a game, `session view FRA` puts France in view and `session leave_view` takes it out; what acts as a country then acts as the one in view while the AI goes on playing it, and with none in view fails with `no_country` ([docs/observe-mode.md](docs/observe-mode.md)).

Remote control is open only to the Windows user playing, only on this computer, and can do only what the player could. Where no capability reaches, it can use the player's mouse and keyboard. `eu4-harmony wiki remote` explains how a game goes and what each exit code means; [docs/remote-control.md](docs/remote-control.md) and [docs/hidden-start.md](docs/hidden-start.md) have the design.

## Documentation

| | |
| --- | --- |
| [docs/v1.md](docs/v1.md) | The design of the first version |
| [GLOSSARY.md](GLOSSARY.md) | The terms the project uses |
| [docs/adr/](docs/adr/) | Decisions that are hard to reverse, and why they were made |
| [docs/research/](docs/research/) | What is known about the game's code |
| [docs/performance.md](docs/performance.md) | What the framework costs the game, measured |
| [docs/prototype-log/](docs/prototype-log/) | What was tested in the running game, and what came out, one file per run |
| [docs/dev-tools.md](docs/dev-tools.md) | The development tools |

Each larger piece of work has a plan of its own in [docs/](docs/), listed in [docs/README.md](docs/README.md) with every other document.

## Building from source

You need Windows, [rustup](https://rustup.rs/) (it picks up the Rust version [rust-toolchain.toml](rust-toolchain.toml) names) and the Visual Studio C++ build tools. The game is needed only to run it and for the offline checks.

```powershell
cargo build --release
cargo test --release
cargo xtask release        # pack the player package
```

The game is found through `EU4_GAME_DIR` or Steam. To try the build in the game, claim a test game: a game started from the game folder with a user folder, a core and a remote control pipe of its own, so that it touches neither the player's files nor the framework installed in the game folder, and several run side by side ([docs/test-games.md](docs/test-games.md)). The game folder needs a framework from test games on, installed once with no game running (`cargo xtask install`).

```powershell
cargo build --release --features eu4-harmony-core/dev-test-games
cargo xtask game claim --purpose "try my change"
cargo xtask game start     # start the test game, hidden, with this build
eu4-harmony game --test-game <this checkout's folder name> session get
cargo xtask game release   # keep its logs, remove the rest
```

The framework takes the game's own `-userdir` for the user folder, as the game does.

`cargo xtask --help` lists the other development commands.

## Contributing

Read [AGENTS.md](AGENTS.md) first: it has the rules for code, commits, checks and testing in the game, for people and coding agents alike. Changes reach master through pull requests.

| Folder | Contents |
| --- | --- |
| `proxy/` | The `version.dll` that loads the core into `eu4.exe` |
| `core/` | The framework: dependency points and startup, the capabilities, the Lua host on the live game, remote control, the framework UI and the Unicode module |
| `lua/` | The Lua host shared by the game and the fake game: sandbox, handlers, tasks, time limit, and what extensions hear in which order |
| `remote/` | The capability catalogue: every capability's arguments, results and description |
| `checks/` | The checks a capability's call goes through, and the game's calendar |
| `client/` | Reaching the running game from outside: remote control's pipe, the game's process, started, followed to its exit code and ended, and backing up and restoring the player's files, shared by the tool and `cargo xtask` |
| `folders/` | The game folder and the user folder, as every program finds them, and the names of what the user folder holds |
| `loc/`, `manifest/` | Localisation files and mod manifests, shared by the core and the tool |
| `cli/` | `eu4-harmony.exe`, and the template mod `new` writes |
| `xtask/` | The repository's development commands (`cargo xtask`) |

## License

[MIT](LICENSE). It covers this repository's own code and documents, not the game: what you may do with a mod is still up to Paradox's terms for mods.
