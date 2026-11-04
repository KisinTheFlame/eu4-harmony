# EU4 Harmony

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

English | [简体中文](README.zh-CN.md)

**A framework that runs inside Europa Universalis IV and makes the game better from within.** EU4 Harmony draws Chinese and every other script, returns to the main menu without restarting the game, makes the game faster without changing what it computes, repairs bugs of the game's own, lets programs and AIs play the game, and lets mods carry Lua scripts.

> [!NOTE]
> EU4 Harmony is in early development. It supports **EU4 1.37.5 on Windows, single player only**, and has no published release yet: [build it from source](#building-from-source). The extension interface may still change between versions.

## What it does

### Text in every script

- All of the game's text is drawn in system fonts, so Chinese, Japanese, Korean and every other script show without a font mod.
- Body text, headings, names on the map and the game console each take a font and weight of their own, chosen in the settings.
- Names you type can be in any script.

[docs/unicode-module.md](docs/unicode-module.md)

### Leaving a game without a restart

- Returning to the main menu takes a few seconds instead of a restart of about 45.
- An ironman game can load another save from the game's menu; the current game is saved first, or, with that setting off, loading the same save again undoes what was played since.

[docs/sessions.md](docs/sessions.md)

### A faster game, the same game

The same rules, data and AI, the same days, only computed faster:

- Paradox's online client no longer sleeps on the main thread while it waits for its connections.
- The line breaks of the text the framework draws are kept instead of worked out again every frame.
- Local saves are written uncompressed and compressed in the background, so a save holds up the game for less time. The game without the framework still loads them.
- Optionally, the fastest speed stops drawing frames in the middle of a long day, for days that pass faster still.

In a game of 1618: 84 frames a second instead of 63 while paused, and days about a tenth faster at the fastest speed ([docs/performance.md](docs/performance.md), [docs/game-speed.md](docs/game-speed.md)).

### Repairs to the game's own bugs

Bugs that are in vanilla too, put right where the game goes wrong:

- **Heir names:** a new heir took the first name of the list about half the time (Zhanji, for Ming), and that name's sex with it. Names are now drawn by their weights.
- **Coalitions:** the AI of several countries left the same coalition at once on the game's worker threads, with no lock, which could corrupt the game's memory. Leaving and joining now take turns.
- **Achievements after a load:** a save loaded from the game's menu never matched its checksum, and turned achievements off for the rest of the game. It now matches as it should.
- **Memory:** what the game never gives back at a world reset (countries' data, mercenary leaders, rebel factions, AI agents), as modifiers change, as the interface is rebuilt (the map modes' window, the bar of the units selected, the outliner's entries), with text boxes' animations and textures' transparency masks, and the heap's free pages the leftovers keep.

[docs/sessions.md](docs/sessions.md#known-leaks) says what the game still keeps.

### Achievements on your terms

Two settings, both off by default, let the game give achievements outside ironman and with mods that change the game checksum. Every other rule stays the game's own.

### Mods in Lua

- Extensions in Lua 5.4, run in a sandbox of their own: nothing to install, nothing to compile.
- Capabilities for countries, provinces, armies, recruiting, diplomacy, events, time, the camera and the game console, and buttons in the vanilla interface.
- Extension save data kept in the save; texts in every game language, shown in the player's.
- Permissions: an extension may reach the real time, files or the network only when its manifest declares it, and players see what each mod declares.
- A command line tool that creates a mod, checks it, runs its tests against a fake game, reloads it in the running game as you edit, and packs it for upload.
- Completion and type checking in any editor with the Lua language server, and guides written for coding agents.

[Writing a mod](#writing-a-mod) below.

### Remote control and AI play

- A program outside the game plays it through the same capabilities, as the player would: from the main menu into a game and back, time, events, armies, recruiting, diplomacy, the console, the camera and screenshots.
- Where no capability reaches, it can use the player's mouse and keyboard, with the game in front or behind other windows.
- The game can start hidden, with nothing on the desktop, for an AI to play while you go on working.
- Open only to the Windows user playing, only on this computer, and never beyond what the player could do.

[Remote control](#remote-control) below.

### Safe by design

- Every place in the game the framework relies on is checked against the bytes it expects at startup. On a game build it does not know, the framework stays out of the game as a whole and a message box says why, instead of misbehaving.
- An error in a mod's script ends that one call, not the mod or the game; a script stuck in a loop is stopped after a time limit.

### Research and tooling

- [docs/research/](docs/research/): what is known about the game's code (armies, diplomacy, saves, the main thread, the interface, text and more), worked out from the game itself.
- A heap census and leak runs that find what the game's memory keeps of each game left ([docs/leak-finding.md](docs/leak-finding.md)).
- Measuring what the framework costs the game against vanilla ([docs/performance.md](docs/performance.md)).
- Offline checks of every dependency point against `eu4.exe`, and a claim that lets several checkouts take turns at the one game, with the player's files backed up and put back ([docs/dev-tools.md](docs/dev-tools.md), [docs/game-testing.md](docs/game-testing.md)).

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
| Game speed: at the fastest speed | Smooth map | "Faster time" draws no frames in the middle of a long day: late in a game, days pass about another tenth faster, but the map moves in jerks and answers clicks later. |

A font change takes effect at the next start, an achievement rule with the next game started or loaded, the rest as soon as it is saved. Every other achievement rule stays the game's own. The always-on speed-ups, and why none of them changes the game, are in [docs/game-speed.md](docs/game-speed.md).

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
