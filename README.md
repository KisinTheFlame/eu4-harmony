# EU4 Harmony

English | [简体中文](README.zh-CN.md)

A framework for writing native Europa Universalis IV mods in Rust. The framework gets into the game process, reads game state, gives orders, adds elements to the vanilla interface and keeps data per save; a mod's extension reaches the game only through the capabilities the framework provides and never touches memory addresses. The design is in [docs/v1.md](docs/v1.md), the terms in [GLOSSARY.md](GLOSSARY.md), and the reasons behind decisions in [docs/adr/](docs/adr/).

Supported game: vanilla 1.37.5, and patched builds of it with the same layout. Single player only.

## Installing the framework (players)

The framework comes as a folder, or a zip of it, holding `version.dll` and an `eu4-harmony\` folder with the core (`eu4_harmony_core.dll`) and the command line tool (`eu4-harmony.exe`); `tools\release.ps1` makes both from a release build. Put `version.dll` and the `eu4-harmony\` folder into the game's root folder. If the root folder already has a `version.dll` (usually the loader of the CJK font plugin), move it into `eu4-harmony\backup\` first. The framework does not load it, and the plugin's files in `plugins\` are not loaded any more either, so they can be deleted. The framework draws Chinese and every other script itself ([ADR 0010](docs/adr/0010-built-in-unicode-module.md)). Then start the game from the Paradox launcher as usual and enable mods built on the framework in your playset.

To uninstall, delete `version.dll` from the root folder, move the backed-up `version.dll`, if there is one, from `eu4-harmony\backup\` back into the root folder, and then delete `eu4-harmony\`.

In saves played with the framework, names the player typed or changed are UTF-8. After removing the framework and going back to the CJK font plugin, those names show as garbage.

Logs, the startup report (`report.txt`) and extension save data are in `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`.

## Remote control (players and AIs)

Remote control lets a program outside the game use it through the framework's capabilities, as the player would: from the main menu into a game and back, reading the game, letting time pass, answering the game's events and other countries' offers, moving armies, recruiting, diplomacy, the game console, the camera and screenshots. It is how an AI plays a whole game, and how developers try the framework from the command line. It is on whenever the framework runs, open only to the Windows user playing and only on this computer (a named pipe), and it acts as the player: what the player could not do, it cannot either ([ADR 0015](docs/adr/0015-remote-control-uses-capabilities.md), [ADR 0017](docs/adr/0017-remote-control-is-always-on.md)).

The client is `eu4-harmony.exe`, in the game's `eu4-harmony\` folder. `eu4-harmony game <capability> [<arguments>]` sends the running game one request and prints the answer as JSON; the exit code is 0 when the game answered, 1 when it answered with an error (`{"code": ..., "message": ...}`, the game's own reasons for a refusal), 2 when the command line is wrong and nothing was sent, and 3 when no answer came. Arguments may be named (`--province 167`) or given in their order, and a capability's dot may be a space:

```powershell
cd "<game folder>\eu4-harmony"
.\eu4-harmony game help                    # how a game goes, what each error means, every capability
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
.\eu4-harmony game menu return             # back to the main menu: the game starts again
```

The capabilities, by group:

| Group | Capabilities |
| --- | --- |
| Main menu | `menu` (where the game is), `menu.saves`, `menu.load`, `menu.start`, `menu.return` |
| Session and time | `session`, `time`, `time.pause`, `time.resume`, `time.speed`, `time.advance` (let days pass and wait, with the events on the way) |
| Countries and provinces | `country.get` (treasury, manpower, stability, prestige, monarch points, wars, allies, subjects and overlord with the kind of subject, rivals), `country.hostile`, `province.list`, `province.get` (name, owner, development, terrain, fort, buildings, what can be recruited there), `province.neighbors` |
| Armies | `army.list`, `army.get`, `army.in_province`, `army.route`, `army.find_route`, `army.can_enter`, `army.strait_open`, `army.can_attack`, `army.move`, `army.stop`, `army.selected` |
| Recruiting | `recruit` (a regiment or ship of a kind in a province) |
| Questions and offers | `question.list`, `question.answer` (the game's events waiting for the player's answer); `offer.list`, `offer.accept`, `offer.decline` (other countries' proposals) |
| Diplomacy | `diplomacy.war_goals`, `diplomacy.declare_war` (calling allies to arms), `diplomacy.offer_peace` (a white peace), `diplomacy.propose_alliance` |
| Game console | `console.run` (a command, as the player types it, and what it printed) |
| Camera and screenshots | `camera`, `camera.look_at_province`, `camera.look_at`, `camera.zoom`; `screenshot` (the frame as the player sees it, as a PNG) |
| Events and descriptions | `events` (numbered: a new session, a new day, an army ordered or defeated), `describe` (every capability, from the running game), `text.language` |

The full list, with every argument and result, is `eu4-harmony game help`, which works without the game, and `eu4-harmony game help <capability>` for one; `eu4-harmony game describe` asks the running game. How each capability works in the game is in [docs/remote-control.md](docs/remote-control.md).

## Writing a mod (developers)

You need Windows x64, Rust (the MSVC toolchain) and the Visual Studio C++ build tools. First build this repository:

```powershell
cargo build --release
```

The command line tool is `target\x86_64-pc-windows-msvc\release\eu4-harmony.exe`:

```powershell
eu4-harmony new my-mod --name "My Mod"   # create a mod: descriptor.mod, manifest, a button, English and Chinese texts, a Rust extension project
cd my-mod
eu4-harmony build                         # build the extension and put the DLL where the manifest says
eu4-harmony link                          # register it under mod\ in the user folder, then tick it in the launcher
eu4-harmony package                       # make a clean folder to upload, ..\my-mod-package
```

With the game running, `eu4-harmony game` asks it things through remote control (above): `eu4-harmony game session` reads the date and the player's country, and `eu4-harmony game help` lists every capability.

An extension implements `eu4_harmony_sdk::Extension`, with one method per kind of event; every capability is a method on the `Game` passed to the callbacks. Unit tests use `eu4_harmony_sdk::testing::FakeGame` in place of the game. The smallest example is the mod `eu4-harmony new` creates; a full one is the regional combat mod in the eu4-regional-combat repository.

Texts go in `eu4-harmony/localisation/<name>_l_<language>.yml`, in the format of the game's localisation files but as plain UTF-8, one file per language (`english`, `french`, `german`, `spanish`, `simp_chinese`). Interface fragments use the keys directly, and an extension gets a text with `game.text("KEY")`; the framework shows the player language, see [ADR 0009](docs/adr/0009-localisation-in-the-player-language.md).

## Repository

| Folder | Contents |
| --- | --- |
| `proxy/` | The `version.dll` proxy: forwards version.dll and loads the core, only in `eu4.exe` |
| `core/` | The framework core: dependency points and startup, where the game keeps what the framework reads (`core/src/objects/`), `.gui` splicing, loading extensions, the capabilities, one file per group (`core/src/game/`), remote control's engine (`core/src/remote/`), interface bindings, extension save data, player language, framework settings, the framework UI (`core/src/framework_ui/`) and the Unicode module (`core/src/unicode/`); `core/localisation/` holds the framework's own texts |
| `loc/` | The localisation file format, the languages and lookup, shared by the core, the SDK and the command line tool |
| `manifest/` | A mod's `eu4-harmony/manifest.json` and the lines of its descriptor, shared by the core and the command line tool |
| `remote/` | Remote control's requests and responses, their JSON form and a description of every capability, shared by the core and the command line tool |
| `abi/` | The C ABI between the core and extensions |
| `sdk/` | The Rust SDK for extensions |
| `cli/` | `eu4-harmony.exe`; `cli/template/` is the mod `new` writes, its extension built and tested with the rest |
| `tools/` | Development tools: `dev-install.ps1` installs the built proxy, core and command line tool into the game folder (`-Uninstall` removes them), `release.ps1` packs them for players, plus input, screenshot, disassembly, crash dump and call tracing tools, see [tools/README.md](tools/README.md) |
| `docs/` | The design, ADRs, research, in-game test records and [what the framework costs the game](docs/performance.md); `docs/research/regional-combat/` holds the reverse-engineering notes the game capabilities rest on |

Code, comments and documentation are in English. [README.zh-CN.md](README.zh-CN.md) is a Chinese translation of this file; the rest of the documentation is English only. Rules for working in this repository, for people and coding agents alike, are in [AGENTS.md](AGENTS.md).
