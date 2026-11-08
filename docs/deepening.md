# Deepening plan

Status: decided (2026-11-08) from an architecture review of the whole codebase, and built the same day, every part; tried together in the game ([prototype-log/2026-11-08-the-deepening-merged.md](prototype-log/2026-11-08-the-deepening-merged.md)). D5's tests through the layer leave out what needs the game's own functions in depth to pass, such as a route found by the game's pathfinding or a treaty object; each part's run file says what it left untried.

## Goal and scope

- **Goal**: knowledge that is written out in several places today lives in one module behind a small interface, so that a change or a fix lands once and is tested at that interface, through the same seam its callers use.
- **In scope**: the parts below, across `core`, `lua`, `remote`, `client`, `cli`, `xtask` and `manifest`.
- **Out of scope**: what the review found deep already and leaves as it is: the dependency point declarations of `points.rs` and `hooks.rs`, the session `Lifecycle`, the `SETTINGS` table, the Unicode module's `text_loops::glyph`, the framework UI's `Measure` and `FontSystem` seams, the remote control engine behind `Catalogue`, the claim's `Machine`, and the Lua host's `Host`. Nothing here changes what players or extensions see, except that the fake game raises every event (D3).

## Parts

Each part is one module, or one module made deeper. The parts run in two waves, so that two never change the same files at once.

### Wave 1

**D1. The observer seam guards, gives memory and reads the arguments.** Today an observer is an `extern "C" fn(*mut HookFrame)`, and each callback dereferences the frame, wraps itself in `guarded` and fetches the game memory on its own. About 35 do not call `guarded`, so a panic in them crosses `extern "C"` and aborts the game with no log and no notice, against ADR framework-succeeds-or-fails-as-a-whole. The seam does these three things itself. A callback is a plain Rust function, given the memory and typed access to the arguments and the result, and it can be called from a test with a test frame. The same holds for every other way the game enters framework code: a replaced function's body is guarded by the seam too. No callback can be written that skips the guard.

**D10. One set of a build's dependency points.** `crate::points()` and `start()` put the points together twice, and the development tools' points are added only in `start()`, so the offline checks and `Checked` have never seen them. One module gives every point of a build: the fixed ones and those `dev.toml` turns on. Startup, the offline checks and `Checked` take them from it. A module's points declaration has no side effects, and a test fails when a module declares points that nothing collects.

**D3. One frame's dispatch to extensions, shared by the core and the fake game.** The order in which a frame calls handlers moves into the `lua` crate: waiting tasks, the events heard, `on_map_lost`, button clicks, `on_frame`, and the session's extension save data for `on_session`. The core feeds it from its state, and the fake game feeds it too. The fake game raises any event the `events!` list declares, with its arguments checked against the event's shape, and a day raises a month and a year when the date crosses one, as in the game.

**D7. One way to read a Harmony mod.** The manifest crate reads a mod's folder: the manifest, its layout, its api_version checked, and the problems, worded once. Startup discovery, extension reload, `eu4-harmony check` and `package` use it, and `check` compiles Lua the way the Lua host loads it.

**D6. The game's process in one module, and the scenario's game through the claim's seam.** One module starts the game, holds its process until its exit code, tells whether a game runs (with one policy when that cannot be told), and ends it. The claim's `Machine` uses it, and `scenario::Game` reaches the process and the pipe only through `Machine`, so how a scenario ended can be tested on the recorded machine. A scenario's report gives the exit code the claim followed.

**D13. A remote client reads answers and waits for the main menu in one place.** The client crate reads the shape of an answer, the counterpart of `remote::response_json`, and waits across a restart until the main menu answers. `start`, `reload`, `eu4-harmony game` and the scenarios use it, with typed requests instead of hand-written JSON.

**D14. The game folder and the user folder in one module.** Finding the game folder in one order, the user folder, the names of what is in it, and which crash folders appeared since a moment, used by the core, the command line tool, `cargo xtask` and the client.

**D15. Smaller parts.** A notice is shown once per session or once per run as the notice module itself is told, and it hears session ends. `eu4-harmony`'s options refuse unknown options as `cargo xtask`'s do. The profiler checks the game's bytes at the places it reads before it samples.

### Wave 2

**D2. Letting go after a return, in one module.** Four repairs each keep a queue of objects to let go once a return has passed, check the class, call the destructor and count. One module does this, and a repair declares only the class and its destructor. The moments of a session a repair acts on (world clearing, the first frame) are part of the `Repair` declaration instead of a dispatch written by hand.

**D4. A capability's arguments declared once.** The layer function takes its request by field name, and the binding comes from the `group!` declaration, so arguments of the same type cannot be swapped and the tests that read the source go.

**D5. The game's containers read and built in one place, and capabilities tested through the layer.** One module walks the game's lists, vectors, maps and countries with one set of safety rules (a limit, cycles, removed nodes). `buffers` builds the same shapes, so that each action capability is tested through its layer function, its refusals included.

**D9. Text in the forms capabilities return.** The game's string module returns a content key, text the player sees or game text, with the conversion inside, instead of raw bytes each caller converts.

**D11. A game function is a typed dependency point.** One handle is both the dependency point and the only way to call the function, with its arguments declared.

**D8. One zip module.** Reading the directory, reading an entry checked against its CRC, writing entries, and CRC, used by the continue box's save header, background compression, framework files and the music cache; the inflate comes from `miniz_oxide`.

**D12. A speed-up declares its own path and its check.** Each speed-up declares its points, the switch that runs the game's own path instead, and how its answer is checked against the game's, in one module that logs a disagreement one way. The save speed-ups leave `saves`, which keeps extension save data. Developer builds' test switches are read through one function that is false in a player's build.

**D16. Smaller parts.** Startup works out what it will set up as a value, applied at its end, so the startup report can be tested. The extensions' desired elements and map colors are kept apart from the hooks that apply them, and tested. The game's `key="value"` line files are read by one reader.

## Done when

Each part has replaced what it gathers, not added beside it; its tests go through its interface; the workspace builds, tests and passes both clippy runs; the offline checks pass; and the game, tried through the scenarios, behaves as before ([prototype-log/](prototype-log/README.md)).
