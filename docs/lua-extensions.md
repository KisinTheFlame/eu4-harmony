# Lua extensions plan

Status: decided (2026-11-03), not built. Terms are in [GLOSSARY.md](../GLOSSARY.md). The decisions and their reasons are in [ADR 0024](adr/0024-extensions-are-lua-in-a-sandbox.md) (Lua in a sandbox in place of native DLLs), [ADR 0025](adr/0025-sensitive-capabilities-are-declared.md) (sensitive capabilities are declared and shown), [ADR 0026](adr/0026-one-capability-catalogue.md) (one capability catalogue, a stable abstraction), [ADR 0027](adr/0027-an-error-does-not-stop-an-extension.md) (an error does not stop an extension; the time limit does), [ADR 0028](adr/0028-save-data-is-what-on-save-returns.md) (extension save data is what `on_save` returns) and [ADR 0029](adr/0029-manifest-in-toml-layout-by-convention.md) (the manifest in TOML, the rest of a mod's layout by convention).

## Goal and scope

- **Goal**: mods carry Lua scripts instead of native DLLs, for two reasons. A mod from the Steam Workshop can no longer do anything in the game process it likes: it reaches the game only through capabilities, and beyond the game only through the sensitive capabilities its manifest declares. And writing one takes no Rust toolchain, no Visual Studio build tools and no build: a mod is text files.
- **In scope**: the Lua host in the core, with its sandbox, handlers, errors and time limit; the capability catalogue, shared by extensions and remote control, with remote control moved onto it; the sensitive capabilities `clock`, `files` and `network`; the manifest in TOML and the layout of a mod's files by convention; the mods window; the command line tool for mod authors (`new`, `test`, `link`, `package`, `types`, `wiki`, `skills`, `help`); removing native extensions (`abi/`, `sdk/`, loading DLLs, `build`); the regional combat mod rewritten in Lua, as the acceptance test.
- **Out of scope**: hot reloading (a later version may add it); the player refusing a sensitive capability; extensions calling each other or sharing code; a shared library of Lua code that mods depend on; anything the first version design leaves out and this plan does not take up.

## Behavior

### Extensions

- An extension is the Lua scripts in a mod's `eu4-harmony/lua/`; its entry is `eu4-harmony/lua/main.lua`. The language is Lua 5.4, built into the core.
- Each extension runs in a Lua state of its own. Extensions share nothing: no globals, no tables, no modules.
- `main.lua` is run once, when the extension is loaded at startup, and returns a table of handlers. The handlers are those of the first version's events, one more for saving, and the same order between extensions (the playset's):

  ```lua
  return {
    on_session = function(game, saved) end,        -- a session began; saved is this extension's save data, or nil
    on_session_ended = function(game, reason) end, -- the session is being left; its state is still there to read
    on_frame = function(game) end,                 -- every frame while a session runs, paused or not
    on_day = function(game, date) end,             -- a day finished advancing
    on_button = function(game, name, click) end,   -- a button of this mod was clicked
    on_army_ordered = function(game, army) end,    -- something other than an extension changed an own army's route
    on_army_defeated = function(game, army) end,   -- an own army lost a battle
    on_map_lost = function(game) end,              -- another extension took over the map coloring
    on_save = function(game) return state end,     -- the game is writing a save; see Extension save data
  }
  ```

- A handler left out is an event the extension does not hear.
- `game` is passed to every handler and is valid only until it returns: a capability called through a `game` kept past that raises an error. Game state and commands are reached only through it, by capability group: `game.army.get(id)`, `game.army.move(id, province)`, `game.province.get(id)`, `game.ui.button.visible = true`, `game.map.paint(colors)`, `game.text(key, values)`.
- What has nothing to do with the game's state is global and can be used at any time, at the top of `main.lua` too: `log(...)`, `json.encode` and `json.decode`, `mod_files`, `files`, `http`.
- Game objects are the game's own identifiers, as in the first version: a province's number, a country's tag, an army's full 64-bit ID, which a Lua integer holds exactly.

### Results and errors

- A capability that fails as the game may make it fail (the object is gone, the game refused the command, the state cannot be read yet) returns `nil, err`, where `err` is `{ code = "...", message = "..." }`. The codes are the framework's statuses (`not_ready`, `not_found`, `rejected`, `invalid_argument`); the message is the game's reason where it gave one, in the player language where the framework has the words.
- A capability called wrongly (an argument of the wrong type, an element this mod does not have, a path that leaves its root, a sensitive capability not declared) raises an error.
- An error raised in a handler, by the script or by a capability, does not stop the extension ([ADR 0027](adr/0027-an-error-does-not-stop-an-extension.md)): the handler's call ends there, the error is written to the extension's log with its traceback, and the next event calls the extension as before. What the handler changed before the error stays changed.
- The same error at the same place is written in full the first three times in a session, then only counted; the count is written when the session ends.
- The first error of each extension in a session raises one notice: the mod's name and that its log has the details.

### The time limit

- Each handler call's time is measured, as in the first version: a call over 5 ms is logged, and the startup report sums each extension's time.
- A call that runs for more than 1 s is stopped with an error, as any other error.
- After three stopped calls in a row, the extension is stopped: it is not called again until the game is closed, its interface elements are hidden, and a notice says why.
- The call is watched from another thread, which reaches into the Lua state only once the limit has passed, so a call within the limit pays nothing for it. Measured 2026-11-03: a count hook set for the whole call slows Lua 5.4 by 50 to 80% in tight loops, whatever its interval.

### The sandbox

- A Lua state holds Lua's `string`, `table`, `math`, `utf8` and `coroutine` libraries, and the base library without `dofile`, `loadfile` or loading bytecode: `load` takes text only.
- `require` loads a module from the mod's own `eu4-harmony/lua/` only, by its dotted name (`require("range.bfs")` is `eu4-harmony/lua/range/bfs.lua`).
- There is no `io`, `debug` or `package.loadlib`, and `os` holds `os.clock` only; `os.time` and `os.date` are there when the manifest declares `clock`.
- Nothing reaches the process, the operating system, environment variables, other mods or native code; there is no capability for them, declared or not.

### Sensitive capabilities

A sensitive capability reaches beyond the game. An extension can use one only when its manifest declares it; using one it did not declare raises an error. The player sees what each mod declares in the mods window and cannot refuse it in this version ([ADR 0025](adr/0025-sensitive-capabilities-are-declared.md)).

- **`clock`**: the real date and time, `os.time` and `os.date`.
- **`files`**: the extension's data folder, `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\data\<id>\`, made the first time the extension writes to it, kept across mod updates and never cleaned up by the framework:

  ```lua
  files.write("cache/routes.json", text)   -- written whole or not at all: a temporary file renamed
  local text, err = files.read("cache/routes.json")
  local names = files.list("cache")
  files.remove("cache/routes.json")
  ```

- **`network`**: HTTP and HTTPS requests to the hosts the manifest lists, by name or address and with a port where it is not the default. A request is sent at once and answered at a later safe point, to the function passed with it, which receives a `game` as a handler does and is held to the time limit:

  ```lua
  http.request({ method = "POST", url = "https://api.example.com/score", body = json.encode(s) },
    function(game, response, err)  -- response: { status = 200, headers = {...}, body = "..." }
      if err then log("not sent: " .. err.message) end
    end)
  ```

  A request not answered within 30 s is answered with an error. Answers wait while no session runs and are dropped when the game closes.

Reading the mod's own files needs no declaration: `mod_files.read("data/units.json")` reads `eu4-harmony/data/units.json` from the mod's folder; nothing writes there. Paths for `files` and `mod_files` are relative, separated by `/`, without `..`, a drive or a leading `/`.

### Extension save data

- `on_save(game)` is called whenever the game writes its world, the ironman exit save included, and what it returns is this extension's save data in that save ([ADR 0028](adr/0028-save-data-is-what-on-save-returns.md)). Returning `nil` keeps nothing.
- When a session begins from a save, `on_session(game, saved)` gets back what that save holds for the extension, or `nil`.
- The data is any value JSON can hold: a table whose keys are all strings is an object, a sequence is an array, integers stay 64-bit. A function, a userdata, a table with mixed keys or a cycle fails: that save holds no data for the extension, the error is logged and raises a notice as other errors do. At most 16 MiB of JSON.
- Where and how it is written into the save does not change ([ADR 0020](adr/0020-extension-save-data-in-the-save.md)); a save written by a native extension is read by its Lua rewrite when the data's shape is the same.
- `on_session_ended` keeps its place, for an extension to finish what it was doing; it no longer decides what goes into the ironman exit save, which calls `on_save` like any other.

### The capability catalogue

- Every capability is described once, in one catalogue: its group, name, arguments, result and failures, and which side has it ([ADR 0026](adr/0026-one-capability-catalogue.md)). Extensions and remote control expose it, each with its own form (a Lua call, a JSON request), the same names and the same meanings.
- The catalogue is the framework's stable abstraction. Capabilities are named and shaped after what a player sees and does, not after the game's internals: no internal indices, fixed-point numbers or state bits. When the game's code changes, the capability layer takes it up and the catalogue stays.
- Adding a capability, a handler or a field of a result changes no version; changing what one that exists means or how it is shaped raises `api_version`. An extension tests for a capability added after it was written with `if game.recruit then`.
- Which side has which capability stays as [ADR 0015](adr/0015-remote-control-uses-capabilities.md) has it. Remote control's capabilities are reshaped with the rest; the names `eu4-harmony game` takes change with them, as 0.x allows. Remote clients read `api_version` from `describe`.

### The manifest and a mod's files

```text
<mod folder>\
  descriptor.mod                 name, version: Paradox's, as the launcher reads them
  eu4-harmony\
    manifest.toml
    lua\main.lua                 the extension; other modules beside it
    ui\<host window>\*.gui       fragments spliced into that window
    ui\<host window>\buttons\*.gui   fragments in that window's button area
    localisation\<name>_l_<language>.yml
    data\...                     the mod's own files, read with mod_files
```

```toml
id = "template-mod"
api_version = "0.2"
description = "TEMPLATE_MOD_DESCRIPTION"   # a key in the mod's localisation
capabilities = { clock = true, files = true, network = ["api.example.com", "192.168.1.10:8080"] }
```

- `id` and `api_version` mean what they meant ([ADR 0014](adr/0014-abi-grows-by-appending.md)'s versioning, now of the catalogue): an extension whose `api_version` is not the framework's is not loaded, and the player is told which version it was written for and which the framework provides.
- Fragments are taken in file name order within a mod, and mods in playset order, as before. A file where the layout has no place for it is recorded in the startup report.
- A mod with a manifest is a Harmony mod, with or without `lua/main.lua`.

### The mods window

- A second button in the main menu's corner, next to the settings button, opens the mods window; it is framework UI ([ADR 0013](adr/0013-framework-draws-its-own-ui.md)).
- On the left, the Harmony mods of this start, in playset order. On the right, the one selected: its name and version from `descriptor.mod`, its description in the player language, and the sensitive capabilities it declares, each in a sentence a player understands, with the hosts for `network`.
- It shows; it decides nothing.

### The command line tool

- `eu4-harmony new <id>`: a mod with `main.lua` (a button, as now), a test, localisation in English and Chinese, an `AGENTS.md` telling a coding agent to read `eu4-harmony skills`, and the editor's type definitions with a `.luarc.json`.
- `eu4-harmony test [<mod dir>]`: runs the mod's `eu4-harmony/tests/*.lua` against a fake game, in the same Lua 5.4 and sandbox. The fake game is a data store, as the SDK's was: a test sets out snapshots, picks routes, raises events and reads back what was recorded (elements, coloring, commands, save data). It refuses what the core refuses, in its words, through `checks/`. In a test, `files` is a temporary folder and `mod_files` the mod's real folder.
- `eu4-harmony link`: as now.
- `eu4-harmony package`: a clean folder to upload; refuses when a Lua file does not compile or a localisation file has errors. The type definitions and `.luarc.json` are left out.
- `eu4-harmony types`: writes the editor's type definitions again, for the catalogue of this tool's version. They are LuaLS annotations of every capability, handler and global, so an editor completes them and marks a misspelled capability or a wrong argument.
- `eu4-harmony wiki [<topic>...]`: the reference, made from the framework's own descriptions, never written by hand: the catalogue (each capability as a Lua call and as an `eu4-harmony game` command), handlers, error codes, manifest fields, sensitive capabilities, the sandbox's globals; and for remote clients, how a game goes and what each exit code means. Topics nest: `wiki`, `wiki army`, `wiki army move`. `eu4-harmony game help` goes; `eu4-harmony game` alone prints the command line's form and points to `wiki`.
- `eu4-harmony skills [<topic>...]`: guides for a mod author's coding agent, written by hand in English and embedded in the tool, nesting as `wiki` does: making a mod, writing tests, interface fragments, localisation, trying a mod in the game through remote control. They point into `wiki` instead of repeating it. CI runs the Lua examples in them with `eu4-harmony test`.
- `eu4-harmony help`: lists the commands, as now. `build` goes.

### Open questions

- Plain HTTP lets anyone on the way read and change what a mod sends. It is allowed for now, for authors who run their own servers; a better answer is still to be found.

## How it fits together

The capability layer (`core/src/game/`) is the module everything rests on: it holds every capability once and hides the game behind it ([ADR 0015](adr/0015-remote-control-uses-capabilities.md)). Two adapters sit on its seam today: the C ABI for native extensions (`core/src/ext.rs`, `abi/`, `sdk/`) and remote control (`core/src/remote/`, `remote/`). The Lua host takes the C ABI's place; the catalogue says what both remaining adapters expose.

```text
 extension (Lua)           remote client
       |                         |
   Lua host                remote control engine, named pipe
       \                        /
        capability catalogue (what is exposed, in what shape)
                    |
        capability layer (core/src/game/)
                    |
        the game, through dependency points
```

- **The catalogue** grows out of `remote/`, which already describes every remote capability for the command line tool: one description per capability, from which the Lua host's bindings, the type definitions, `wiki` and `describe` are made, so none can drift from another.
- **The Lua host** (in the core) owns the Lua states: loading `main.lua` with the sandbox, calling handlers at safe points, the `game` passed to them, errors and their log, the time limit's watcher, the globals and the sensitive capabilities.
- **The fake game** (in the command line tool) is the Lua host's other adapter, over a data store instead of the capability layer; `checks/` keeps its refusals the core's.
- Removed: `abi/`, `sdk/`, `core/src/ext.rs`, `core/src/ext/`, `cli/template/extension/`, `build`, loading DLLs.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, and so do the offline checks against `eu4.exe`;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), where it reaches the game;
- README.md and README.zh-CN.md say what changed for players and mod authors.

### L1 Capability catalogue

The catalogue, group by group, reviewed before it is built: each capability's name and shape after what a player sees and does. Remote control moved onto it, with its command line names changed; `describe` and `wiki` made from it; `game help` removed.

### L2 Lua host

Lua 5.4 in the core; a Lua state per extension with the sandbox; `main.lua` and its handlers, including `on_save` and `on_session`'s save data; the `game` adapter over the catalogue; results and errors, the log's deduplication and the notice; the time limit and its watcher; `log`, `json` and `mod_files`; `manifest.toml` and the layout by convention, with the startup report. Native extensions removed: `abi/`, `sdk/`, `ext.rs`, loading DLLs.

### L3 Sensitive capabilities

`clock`, `files` and `network`, declared in the manifest, and the error when one is used undeclared.

### L4 Mods window

The corner button and the two-column window, with descriptions and sensitive capabilities in the player language.

### L5 Tools for mod authors

`new` with its new template, `test` and the fake game, `types`, `package`'s checks, `skills` with its first guides, `help`; `build` removed.

### L6 Acceptance

The regional combat mod rewritten in Lua, in its own repository ([ADR 0004](adr/0004-ports-live-outside-framework-repo.md)):

1. **The same mod**: the button, the radius settings, the range preview, the automatic decisions, and a task turned off by a hand-given order or a defeat, as the native extension does them.
2. **Its saves**: a save written with the native extension, loaded with the Lua one, restores the tasks it held.
3. **Its tests**: the decision core's C++ tests and the extension's Rust tests, as `eu4-harmony test` tests.
4. **Its cost**: in a normal game, no handler call over 5 ms, and less than 0.5 ms per frame on average. Where it is more, the catalogue gains a coarser general capability (a province's range, provinces read in one call), not native code. Measured 2026-11-03 (Lua 5.4.8 through mlua 0.11.6, Ryzen 7 9700X): a handler call costs about 40 ns, a capability call 30 to 60 ns, a snapshot of ten fields 0.5 to 0.9 µs, a fresh Lua state 17 KB; Lua computes 40 to 110 times slower than Rust, a breadth-first search of 2,700 provinces taking 0.2 to 0.65 ms.

Capabilities the rewrite finds missing are added to the catalogue as general capabilities ([ADR 0005](adr/0005-extensions-use-capabilities-only.md)).
