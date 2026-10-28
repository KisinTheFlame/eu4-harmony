# EU4 Harmony

English | [简体中文](README.zh-CN.md)

A framework for writing native Europa Universalis IV mods in Rust. The framework gets into the game process, reads game state, gives orders, adds elements to the vanilla interface and keeps data per save; a mod's extension reaches the game only through the capabilities the framework provides and never touches memory addresses. The design is in [docs/v1.md](docs/v1.md), the terms in [GLOSSARY.md](GLOSSARY.md), and the reasons behind decisions in [docs/adr/](docs/adr/).

Supported game: vanilla 1.37.5, and patched builds of it with the same layout. Single player only.

## Installing the framework (players)

Put `version.dll` and the `eu4-harmony\` folder into the game's root folder. If the root folder already has a `version.dll` (usually the loader of the CJK font plugin), move it into `eu4-harmony\backup\` first. The framework does not load it, and the plugin's files in `plugins\` are not loaded any more either, so they can be deleted. The framework draws Chinese and every other script itself ([ADR 0010](docs/adr/0010-built-in-unicode-module.md)). Then start the game from the Paradox launcher as usual and enable mods built on the framework in your playset.

To uninstall, delete `version.dll` and `eu4-harmony\` from the root folder, and put the backed-up `version.dll` back.

In saves played with the framework, names the player typed or changed are UTF-8. After removing the framework and going back to the CJK font plugin, those names show as garbage.

Logs, the startup report (`report.txt`) and extension save data are in `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`.

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

An extension implements `eu4_harmony_sdk::Extension`, with one method per kind of event; every capability is a method on the `Game` passed to the callbacks. Unit tests use `eu4_harmony_sdk::testing::FakeGame` in place of the game. The smallest example is the mod `eu4-harmony new` creates; a full one is the regional combat mod in the eu4-regional-combat repository.

Texts go in `eu4-harmony/localisation/<name>_l_<language>.yml`, in the format of the game's localisation files but as plain UTF-8, one file per language (`english`, `french`, `german`, `spanish`, `simp_chinese`). Interface fragments use the keys directly, and an extension gets a text with `game.text("KEY")`; the framework shows the player language, see [ADR 0009](docs/adr/0009-localisation-in-the-player-language.md).

## Repository

| Folder | Contents |
| --- | --- |
| `proxy/` | The `version.dll` proxy: forwards version.dll and loads the core, only in `eu4.exe` |
| `core/` | The framework core: dependency points and startup, where the game keeps what the framework reads (`core/src/objects/`), `.gui` splicing, loading extensions, the capabilities, one file per group (`core/src/game/`), interface bindings, extension save data, player language, framework settings, the framework UI (`core/src/framework_ui/`) and the Unicode module (`core/src/unicode/`); `core/localisation/` holds the framework's own texts |
| `loc/` | The localisation file format, the languages and lookup, shared by the core, the SDK and the command line tool |
| `manifest/` | A mod's `eu4-harmony/manifest.json` and the lines of its descriptor, shared by the core and the command line tool |
| `abi/` | The C ABI between the core and extensions |
| `sdk/` | The Rust SDK for extensions |
| `cli/` | `eu4-harmony.exe`; `cli/template/` is the mod `new` writes, its extension built and tested with the rest |
| `tools/` | Development tools: `dev-install.ps1` installs the built proxy and core into the game folder (`-Uninstall` removes them), plus input, screenshot, disassembly, crash dump and call tracing tools, see [tools/README.md](tools/README.md) |
| `docs/` | The design, ADRs, research, in-game test records and [what the framework costs the game](docs/performance.md); `docs/research/regional-combat/` holds the reverse-engineering notes the game capabilities rest on |

Code, comments and documentation are in English. [README.zh-CN.md](README.zh-CN.md) is a Chinese translation of this file; the rest of the documentation is English only. Rules for working in this repository, for people and coding agents alike, are in [AGENTS.md](AGENTS.md).
