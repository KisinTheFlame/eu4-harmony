# EU4 Harmony

English | [简体中文](README.zh-CN.md)

A framework for Europa Universalis IV mods that carry Lua scripts. The framework gets into the game process, reads game state, gives orders, adds elements to the vanilla interface and keeps data per save; a mod's extension reaches the game only through the capabilities the framework provides and never touches memory addresses. The design is in [docs/v1.md](docs/v1.md), the terms in [GLOSSARY.md](GLOSSARY.md), and the reasons behind decisions in [docs/adr/](docs/adr/).

Supported game: vanilla 1.37.5, and patched builds of it with the same layout. Single player only.

## Installing the framework (players)

The framework comes as a folder, or a zip of it, holding `version.dll` and an `eu4-harmony\` folder with the core (`eu4_harmony_core.dll`) and the command line tool (`eu4-harmony.exe`); `tools\release.ps1` makes both from a release build. Put `version.dll` and the `eu4-harmony\` folder into the game's root folder. If the root folder already has a `version.dll`, move it into `eu4-harmony\backup\` first; the framework does not load it. The framework draws Chinese and every other script itself ([ADR 0010](docs/adr/0010-built-in-unicode-module.md)). Then start the game from the Paradox launcher as usual and enable mods built on the framework in your playset. Mods whose extension is a DLL, made for earlier versions of the framework, are no longer loaded: their extensions have to be rewritten in Lua.

To uninstall, delete `version.dll` from the root folder, move the backed-up `version.dll`, if there is one, from `eu4-harmony\backup\` back into the root folder, and then delete `eu4-harmony\`.

In saves played with the framework, names the player typed or changed are UTF-8, and show as garbage once the framework is removed.

Logs and the startup report (`report.txt`) are in `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`. Extension save data is written into the save itself; a save played with an extension that keeps data is not promised to load without the framework.

## Leaving a game

Returning to the main menu does not start the game again: the main menu is back in a few seconds instead of about 45 ([ADR 0022](docs/adr/0022-return-to-the-main-menu-in-process.md)). An ironman game can be left by loading another save from the game's menu too; the current game is saved first ([ADR 0023](docs/adr/0023-in-game-load-in-ironman.md)). Both are framework settings, in the settings window the "EU4 Harmony settings" button opens in the main menu's corner: with "Return to the main menu without restarting the game" off, the game starts again as in vanilla; with "In ironman, save the game before loading another save" off, loading the same save again undoes what was played since it was written. The game keeps some memory of every game left so, about 15 MB a return, until it is closed ([known leaks](docs/sessions.md#known-leaks)); after many returns, starting the game again gives it back.

## Remote control (players and AIs)

Remote control lets a program outside the game use it through the framework's capabilities, as the player would: from the main menu into a game and back, reading the game, letting time pass, answering the game's events and other countries' offers, moving armies, recruiting, diplomacy, the game console, the camera and screenshots. It is how an AI plays a whole game, and how developers try the framework from the command line. It is on whenever the framework runs, open only to the Windows user playing and only on this computer (a named pipe), and it acts as the player: what the player could not do, it cannot either ([ADR 0015](docs/adr/0015-remote-control-uses-capabilities.md), [ADR 0017](docs/adr/0017-remote-control-is-always-on.md)).

The client is `eu4-harmony.exe`, in the game's `eu4-harmony\` folder. `eu4-harmony game <capability> [<arguments>]` sends the running game one request and prints the answer as JSON; the exit code is 0 when the game answered, 1 when it answered with an error (`{"code": ..., "message": ...}`, the game's own reasons for a refusal), 2 when the command line is wrong and nothing was sent, and 3 when no answer came. Arguments may be named (`--province 167`) or given in their order, and a capability's dot may be a space:

```powershell
cd "<game folder>\eu4-harmony"
.\eu4-harmony wiki remote                 # how a game goes, and what each exit code means
.\eu4-harmony game menu                    # where the game is: main_menu, starting or game
.\eu4-harmony game menu start FRA          # a new game as France, answered once it is ready
.\eu4-harmony game question list           # the game's events waiting for the player's answer
.\eu4-harmony game question answer 2 0     # answer one with one of its options
.\eu4-harmony game time advance 30         # let 30 days pass, then pause; stops early for a question
.\eu4-harmony game army list               # the player's armies
.\eu4-harmony game army move 545460846646 185
.\eu4-harmony game diplomacy war_goals ENG
.\eu4-harmony game diplomacy declare_war ENG cb_core --province 167
.\eu4-harmony game screenshot --scale 50 --out shot.png
.\eu4-harmony game menu return             # back to the main menu, answered once it is up
```

The capabilities, by group:

| Group | Capabilities |
| --- | --- |
| Main menu | `menu` (where the game is), `menu.saves`, `menu.load`, `menu.start`, `menu.return` |
| Session and time | `session.get`, `time.get`, `time.pause`, `time.resume`, `time.set_speed`, `time.advance` (let days pass and wait, with the events on the way) |
| Countries and provinces | `country.get` (treasury, manpower, stability, prestige, monarch points, wars, allies, subjects and overlord with the kind of subject, rivals), `country.hostile`, `province.list`, `province.get` (name, owner, development, terrain, fort, buildings), `province.neighbors` |
| Armies | `army.list`, `army.get` (with the route still to go and what stops it taking orders), `army.at`, `army.route_to`, `army.can_step`, `army.can_attack`, `army.move`, `army.stop`, `army.selected` |
| Recruiting | `recruit.options` (what can be recruited in a province, and why not), `recruit.order` (a regiment or ship of a kind) |
| Questions and offers | `question.list`, `question.answer` (the game's events waiting for the player's answer); `offer.list`, `offer.accept`, `offer.decline` (other countries' proposals) |
| Diplomacy | `diplomacy.war_goals`, `diplomacy.declare_war` (calling allies to arms), `diplomacy.offer_peace` (a white peace), `diplomacy.propose_alliance` |
| Game console | `console.run` (a command, as the player types it, and what it printed) |
| Camera and screenshots | `camera.get`, `camera.look_at` (a province or a map position), `camera.zoom`; `screenshot` (the frame as the player sees it, as a PNG) |
| Events and descriptions | `events` (numbered: a session begun or ended, a new day, an army ordered or defeated), `describe` (the catalogue's version and every capability, from the running game), `text.language` |

The full list, with every argument and result, is `eu4-harmony wiki`, which works without the game: `eu4-harmony wiki army` for a group, `eu4-harmony wiki army move` for one capability, `eu4-harmony wiki errors` for what each error means; `eu4-harmony game describe` asks the running game. How each capability works in the game is in [docs/remote-control.md](docs/remote-control.md).

## Writing a mod (developers)

A mod's extension is Lua 5.4 scripts, run by the framework in a sandbox of their own ([ADR 0024](docs/adr/0024-extensions-are-lua-in-a-sandbox.md)): nothing to install and nothing to build. The command line tool is `eu4-harmony.exe`, in the game's `eu4-harmony\` folder (or `target\x86_64-pc-windows-msvc\release\` in a build of this repository):

```powershell
eu4-harmony new my-mod --name "My Mod"   # create a mod: descriptor.mod, manifest.toml, lua\main.lua, a button, English and Chinese texts, the editor's type definitions, an AGENTS.md
cd my-mod
eu4-harmony test                          # run its eu4-harmony\tests\*.lua against a fake game
eu4-harmony link                          # register it under mod\ in the user folder, then tick it in the launcher
eu4-harmony package                       # once every Lua and localisation file reads, make a clean folder to upload, ..\my-mod-package
```

A mod's own files are in its `eu4-harmony\` folder, where they are says what they are ([ADR 0029](docs/adr/0029-manifest-in-toml-layout-by-convention.md)): `manifest.toml` (`id`, `api_version`, `description`, `permissions`), the extension in `lua\` with `main.lua` first, interface fragments in `ui\<window>\` (and `ui\<window>\buttons\` for the button area), texts in `localisation\`, and files the extension reads in `data\`.

`permissions` declares what the extension may do beyond the game: the real time (`clock = true`), files in a data folder of its own (`files = true`) and HTTP requests to the hosts it lists (`network = ["api.example.com"]`); using one it did not declare raises an error ([ADR 0025](docs/adr/0025-sensitive-capabilities-are-declared.md)). Players see what each mod declares, with its name, version and description, in the mods window the "EU4 Harmony mods" button opens in the main menu's corner.

`main.lua` returns a table of handlers, one per event (`on_session`, `on_day`, `on_button`, `on_frame`, `on_save` and the rest). Each gets `game`, through which every capability is called by its group, such as `game.army.move(army, province)`; a refusal returns `nil, err`, and a handler may wait for a capability's answer, such as another country's to a proposal, as with `await`. `eu4-harmony wiki` lists every capability with its Lua form, `eu4-harmony wiki lua` the handlers and globals, `eu4-harmony wiki manifest` the manifest's fields and permissions; `eu4-harmony game` asks the running game the same through remote control (above). The smallest example is the mod `eu4-harmony new` creates; the plan, with handlers, errors, the time limit and extension save data, is [docs/lua-extensions.md](docs/lua-extensions.md).

`eu4-harmony skills` holds guides for making a mod, written for a coding agent and readable by anyone: the steps of each kind of work and how to tell each is done (`eu4-harmony skills mod` first, then `tasks`, `state`, `ui`, `localisation`, `testing`, `game`). The `AGENTS.md` that `eu4-harmony new` writes points an agent at them.

An editor with the Lua language server (LuaLS) completes `game`, the capabilities and their results' fields, the handlers and the globals, and marks a misspelled capability or a wrong argument: `eu4-harmony new` writes the type definitions into the mod's `.eu4-harmony\types\` with a `.luarc.json`, and `eu4-harmony types` writes them again for a newer tool. `eu4-harmony package` leaves both out.

Texts go in `eu4-harmony/localisation/<name>_l_<language>.yml`, in the format of the game's localisation files but as plain UTF-8, one file per language (`english`, `french`, `german`, `spanish`, `simp_chinese`). Interface fragments use the keys directly, and an extension gets a text with `text.get("KEY")`; the framework shows the player language, see [ADR 0009](docs/adr/0009-localisation-in-the-player-language.md).

## Repository

| Folder | Contents |
| --- | --- |
| `proxy/` | The `version.dll` proxy: forwards version.dll and loads the core, only in `eu4.exe` |
| `core/` | The framework core: dependency points and startup, where the game keeps what the framework reads (`core/src/objects/`), `.gui` splicing, loading extensions and calling their handlers on the live host (`core/src/lua/`), the capabilities, one file per group (`core/src/game/`), remote control's engine (`core/src/remote/`), interface bindings, extension save data, player language, framework settings, the framework UI (`core/src/framework_ui/`) and the Unicode module (`core/src/unicode/`); `core/localisation/` holds the framework's own texts |
| `lua/` | The Lua host: an extension's sandbox, handlers and tasks, errors and time limit, over a host that is the running game in the core and a fake game in the command line tool |
| `loc/` | The localisation file format, the languages and lookup, shared by the core and the command line tool |
| `manifest/` | A mod's `eu4-harmony/manifest.toml`, where the layout puts its other files, and the lines of its descriptor, shared by the core and the command line tool |
| `remote/` | The capability catalogue: remote control's requests and responses, their JSON form and a description of every capability, saying which extensions have too, shared by the core and the command line tool |
| `checks/` | The checks the framework makes of a capability's call itself, with the status and words of each refusal, shared by the core and `remote/`; and the game's calendar |
| `cli/` | `eu4-harmony.exe`; `cli/template/` is the mod `new` writes |
| `tools/` | Development tools: `dev-install.ps1` installs the built proxy, core and command line tool into the game folder (`-Uninstall` removes them), `release.ps1` packs them for players, plus tools for backing up the player's files around a test run, input, screenshots, disassembly, crash dumps and call tracing, see [tools/README.md](tools/README.md) |
| `docs/` | The design, ADRs, research, in-game test records and [what the framework costs the game](docs/performance.md) |

Code, comments and documentation are in English. [README.zh-CN.md](README.zh-CN.md) is a Chinese translation of this file; the rest of the documentation is English only. Rules for working in this repository, for people and coding agents alike, are in [AGENTS.md](AGENTS.md).
