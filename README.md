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
| Fewer long gaps between frames during daily AI work ([measurements](docs/performance.md#why-submission-interval-p99-is-long)) | ✗ | ✓ |
| Saving pauses the game a third as long | ✗ | ✓ |
| The game starts and loads saves sooner | ✗ | ✓ |
| A faster late game | ✗ | ✓ |
| Heirs no longer get the same name half the time | ✗ | ✓ |
| Fewer crashes, including [stale reformation-center references](docs/game/provinces/reformation-centers.md) | ✗ | ✓ |
| Loading a save no longer costs you achievements | ✗ | ✓ |
| A long game no longer eats ever more memory | ✗ | ✓ |
| Optionally, achievements outside ironman or with mods | ✗ | ✓ |
| Mods that do far more: buttons, windows, acting for you | ✗ | ✓ |
| **The game's rules and data unchanged** | ✓ | ✓ |

Your translation mod is still what translates the game; translation mods made for the old double-byte patch, and the saves played with them, work as they are. Harmony runs on EU4 1.37.5 for Windows, in single player.

## Installing

1. Subscribe to **Harmony** on the Steam Workshop.
2. In Steam, right-click Europa Universalis IV and choose **Manage → Browse local files**: this opens the game's folder (keep it open).
3. Open the Paradox launcher, go to **Mod library**, find **Harmony**, and click **… → Show in folder**: this opens Harmony's folder. (If Harmony does not appear there yet, restart Steam; you can also open `steamapps\workshop\content\236850\<id>\` in the same Steam library as the game, where `<id>` is the number at the end of Harmony's Workshop page address.)
4. Copy `version.dll` from Harmony's folder into the game's folder, next to `eu4.exe`. (Windows may hide file extensions by default, showing them as `version` and `eu4`. If the game's folder already has a `version.dll` — for example from an older CJK patch — choose **Replace**; if you want to keep the old file to switch back later, rename or move it aside first.)

Start the game from the Paradox launcher as usual. Harmony needs no place in your playset; mods built on it are enabled there as any mod. The main menu and the in-game menu now have two buttons in the corner, **Harmony settings** and **Harmony mods**; the second lists each mod built on Harmony, with what it may do beyond the game.

Steam keeps Harmony up to date. When an update includes a new `version.dll`, Harmony updates the copy in the game's folder on startup and restarts the game automatically (only asking you to copy it if the game folder cannot be written).

**Uninstalling:** unsubscribe from Harmony, and the game runs as it always did. To leave nothing behind, delete `version.dll` from the game's folder too, and put back any backup of an earlier `version.dll` you kept.

For a manual install, download `eu4-harmony-<version>-windows-x64.zip` from [GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases), close the game and extract it into the game folder beside `eu4.exe` (replace any existing `version.dll` from another patch, or move it aside first if you want a backup). The package includes the proxy, core, and components that save minidumps when the game crashes. Download and extract each update yourself: the local core takes precedence over the Workshop's, so Steam does not update this copy. To return to Workshop updates, remove the local `eu4_harmony_core.dll`, `eu4_harmony_crash_dumps.dll`, and `eu4-harmony-dump-writer.exe` from the game folder's `eu4-harmony\` directory. To uninstall the manual build, also remove the proxy `version.dll`. The CLI is a separate `eu4-harmony-cli-<version>-windows-x64.zip` download and can run outside the game folder; players do not need it.

Good to know:

- Names you type in a game played with Harmony are saved as UTF-8, and show as garbage without it.
- A mod that keeps data of its own writes it into the save; such a save is not promised to load without Harmony.
- Achievements outside ironman must be enabled before starting a new game. Saves made without a tracked achievement list, including those affected by the earlier non-ironman tracking bug, need a new game; enabling the setting cannot reconstruct their starting conditions. [Achievement tracking](docs/game/countries/achievements.md#each-achievements-script-conditions) explains why.
- With Harmony, the game's volume sliders go as the square of their position, so each step sounds about as large. Volumes saved below 100 sound quieter than before; set them once again.

## When something goes wrong

In the main menu or the in-game menu, open Harmony's settings, go to **About** and press **Save troubleshooting files to the desktop**: it packs Harmony's logs, the game's logs, its existing crash folders and your settings and playset (no saves) into one zip file on your desktop. Report a problem on [GitHub Issues](https://github.com/KisinTheFlame/eu4-harmony/issues), with that file attached.

Logs are in `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\`: `report.txt` is the startup report, `core.log` records framework work, and `proxy.log` explains loading failures. [Logs](docs/logs.md) describe retention and diagnostic coverage.

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

Download the command line tool, `eu4-harmony.exe`, separately from [GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases).

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

A mod is an ordinary Paradox mod with an `eu4-harmony/` folder. Handlers use capabilities through `game`; sensitive permissions are declared in the manifest and shown to players in Harmony mods. The guides below cover layouts, tasks and save data.

Where to learn more:

- `eu4-harmony wiki`: every capability with its arguments and results, the handlers and globals (`wiki lua`), the manifest (`wiki manifest`). It needs no game.
- `eu4-harmony skills`: step-by-step guides for each kind of work (`skills mod` for making a mod, `skills ui`, `skills testing`, …), written for coding agents and readable by anyone.
- [cli/skills/mod.md](cli/skills/mod.md): how extensions work, with errors, the time and memory limits and save data.

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

In observe mode, the AI playing every country, `menu start` with no country starts such a game, `session view FRA` puts France in view and `session leave_view` takes it out; what acts as a country then acts as the one in view while the AI goes on playing it, and with none in view fails with `no_country` ([docs/game/countries/countries.md](docs/game/countries/countries.md)).

Remote control is open only to the Windows user playing, only on this computer. A request acts as the player: it reads what the player sees and does only what the player could. With `--as-game` it acts as the game instead: it reads everything the game keeps, and may also change the game as the mods' scripts do, which no player could ([ADR requests-act-as-the-player-or-as-the-game](docs/adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md)). Where no capability reaches, it can use the player's mouse and keyboard. `eu4-harmony wiki remote` explains how a game goes and what each exit code means.

## Documentation

The [documentation entry](docs/README.md) leads to capability reference, procedures, game logic, development tools and durable evidence. The [glossary](GLOSSARY.md) defines project language; [open work](docs/plans/README.md) lists only what remains unfinished.

## Building from source

You need Windows 10 (1903+) x64, [rustup](https://rustup.rs/) (it picks up the Rust version [rust-toolchain.toml](rust-toolchain.toml) names) and the Visual Studio C++ build tools. The game is needed only to run it, for offline dependency checks, and for test games.

```powershell
cargo build --release
cargo test
cargo xtask offline-check          # requires EU4 1.37.5 (Steam or EU4_GAME_DIR)
cargo xtask release --no-workshop  # pack GitHub release ZIPs without Edge or SteamCMD
```

To run isolated test games and scenarios locally, Steam must be running with EU4 1.37.5 (standard scenarios expect full DLCs):

```powershell
cargo xtask install                # once, with the game closed, so version.dll loads test cores
cargo xtask saves fetch            # once, to download and verify shared test saves
cargo xtask scenario sweep --owner <name>
```

Build, release, maintainer-only Workshop publication and isolated test-game procedures are in [development tools](docs/dev-tools.md). A test game has a pure synthetic user folder, core and pipe of its own; start it through a claim without changing the player's files.

## Contributing

Read [AGENTS.md](AGENTS.md) first: it has the rules for code, commits, checks and testing in the game, for people and coding agents alike. Changes reach master through pull requests.

## License

[MIT](LICENSE). It covers this repository's own code and documents, not the game: what you may do with a mod is still up to Paradox's terms for mods.
