# EU4 Harmony

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

English | [简体中文](README.zh-CN.md)

**Lua scripting for Europa Universalis IV mods.** EU4 Harmony runs inside the game and lets a mod carry an extension: Lua scripts that read the game, give orders as the player would, add buttons to the vanilla interface and keep data in the save. The extension never touches memory addresses; it calls capabilities such as `game.army.move(army, province)`, and the framework does the rest.

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

> [!NOTE]
> EU4 Harmony is in early development. It supports **EU4 1.37.5 on Windows, with every DLC, single player only**, and has no published release yet: [build it from source](#building-from-source). The extension interface may still change between versions.

## Features

**For mod authors**

- Extensions in Lua 5.4, run in a sandbox of their own: nothing to install, nothing to compile.
- Capabilities for countries, provinces, armies, recruiting, diplomacy, decisions, events, time, the camera and the game console, and buttons and windows in the vanilla interface. Provinces are administered as the province view does: developing, autonomy, states, edicts, cores, culture and buildings.
- Extension save data kept in the save; texts in every game language, shown in the player's.
- A command line tool that creates a mod, checks it, runs its tests against a fake game, reloads it in the running game as you edit, and packs it for upload.
- Completion and type checking in any editor with the Lua language server, and guides written for coding agents.

**For players**

- Chinese and every other script drawn in the game, in system fonts of your choice.
- Back to the main menu in a few seconds instead of a restart of about 45.
- A faster game that computes the same game: 84 frames a second instead of 63 in a paused game of 1618, and days about a tenth faster at the fastest speed ([measurements](docs/performance.md)).
- Optional: achievements outside ironman and with mods that change the checksum.
- On a game build it does not know, the framework stays out of the game as a whole and a message box says why, instead of misbehaving.

**For tools and AIs**

- Remote control: a program outside the game plays it through the same capabilities, from the main menu into a game and back, from the command line or as JSON over a local named pipe.
- The game can be started hidden, with nothing on the desktop, for an AI to play while you go on working.

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

| Setting | Default | What it does |
| --- | --- | --- |
| Fonts | Microsoft YaHei; KaiTi on the map, Consolas in the console | The system fonts for body text, headings, the map and the console. |
| Return to the main menu without restarting the game | On | The main menu is back in seconds. The game keeps about 15 MB of memory each time, until it is closed ([why](docs/sessions.md#known-leaks)). |
| In ironman, save the game before loading another save | On | Ironman games can load another save from the game's menu. Off, loading the same save again undoes what was played since. |
| Achievements outside ironman | Off | Achievements in a game that is not ironman, even with saves loaded again and the console used. |
| Achievements with a changed game checksum | Off | Achievements with mods that change the checksum the main menu shows. |
| Remote control sees everything | Off | Off, programs using the game through remote control read what the player sees: the map under fog of war, and of other countries what their panels show. On, they read everything the game keeps, as mods' extensions always do ([why](docs/adr/remote-control-reads-the-players-view.md)). |
| Game speed: at the fastest speed | Smooth map | "Faster time" draws no frames in the middle of a long day: late in a game, days pass about another tenth faster, but the map moves in jerks and answers clicks later. |

A font change takes effect at the next start, an achievement rule with the next game started or loaded, the rest as soon as it is saved. Every other achievement rule stays the game's own. The always-on speed-ups, and why none of them changes the game, are in [docs/game-speed.md](docs/game-speed.md).

## Writing a mod

The command line tool is `eu4-harmony.exe`, in the game's `eu4-harmony\` folder:

```powershell
eu4-harmony new my-mod --name "My Mod"   # a working mod with a button, English and Chinese texts and editor types
cd my-mod
eu4-harmony check                         # read everything as the framework will
eu4-harmony test                          # run eu4-harmony\tests\*.lua against a fake game
eu4-harmony link                          # register it with the launcher, then enable it in your playset
eu4-harmony reload --watch                # with the game running, reload the extension whenever a file changes
eu4-harmony package                       # a clean folder ready to upload
```

A mod is an ordinary Paradox mod with an `eu4-harmony\` folder, where a file's place says what it is:

```text
my-mod/
├── descriptor.mod
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
- `eu4-harmony skills mod`: step-by-step guides for each kind of work, written for coding agents and readable by anyone.
- [docs/lua-extensions.md](docs/lua-extensions.md): how extensions work, with errors, the time limit and save data.

## Remote control

`eu4-harmony game <capability> [<arguments>]` sends the running game one request and prints the answer as JSON:

```powershell
eu4-harmony start --hidden                 # start the game with nothing on the desktop
eu4-harmony game menu start FRA            # a new game as France
eu4-harmony game time advance 30           # let 30 days pass; stops early for an event to answer
eu4-harmony game question list             # the events waiting for the player
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # back to the main menu
```

Remote control is open only to the Windows user playing, only on this computer, and can do only what the player could. Where no capability reaches, it can use the player's mouse and keyboard. `eu4-harmony wiki remote` explains how a game goes and what each exit code means; [docs/remote-control.md](docs/remote-control.md) and [docs/hidden-start.md](docs/hidden-start.md) have the design.

## Documentation

| | |
| --- | --- |
| [docs/v1.md](docs/v1.md) | The design of the first version |
| [GLOSSARY.md](GLOSSARY.md) | The terms the project uses |
| [docs/adr/](docs/adr/) | Decisions that are hard to reverse, and why they were made |
| [docs/research/](docs/research/) | What is known about the game's code |
| [docs/performance.md](docs/performance.md) | What the framework costs the game, measured |
| [docs/prototype-log.md](docs/prototype-log.md) | What was tested in the running game, and what came out |
| [docs/dev-tools.md](docs/dev-tools.md) | The development tools |

Each larger piece of work has a plan of its own in [docs/](docs/), listed in [AGENTS.md](AGENTS.md).

## Building from source

You need Windows, [rustup](https://rustup.rs/) (it picks up the Rust version [rust-toolchain.toml](rust-toolchain.toml) names) and the Visual Studio C++ build tools. The game is needed only to run it and for the offline checks.

```powershell
cargo build --release
cargo test --release
cargo xtask install        # put the build into the game folder (--uninstall removes it)
cargo xtask release        # pack the player package
```

The game is found through `EU4_GAME_DIR` or Steam. `cargo xtask --help` lists the other development commands.

## Contributing

Read [AGENTS.md](AGENTS.md) first: it has the rules for code, commits, checks and testing in the game, for people and coding agents alike. Changes reach master through pull requests.

| Folder | Contents |
| --- | --- |
| `proxy/` | The `version.dll` that loads the core into `eu4.exe` |
| `core/` | The framework: dependency points and startup, the capabilities, the Lua host on the live game, remote control, the framework UI and the Unicode module |
| `lua/` | The Lua host shared by the game and the fake game: sandbox, handlers, tasks, time limit |
| `remote/` | The capability catalogue: every capability's arguments, results and description |
| `checks/` | The checks a capability's call goes through, and the game's calendar |
| `loc/`, `manifest/` | Localisation files and mod manifests, shared by the core and the tool |
| `cli/` | `eu4-harmony.exe`, and the template mod `new` writes |
| `xtask/` | The repository's development commands (`cargo xtask`) |

## License

[MIT](LICENSE). It covers this repository's own code and documents, not the game: what you may do with a mod is still up to Paradox's terms for mods.
