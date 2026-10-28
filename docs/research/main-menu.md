# The main menu (for `game::menu`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-10-29 (disassembly, and Ghidra's decompiler on single functions without analysis); what was checked in the running game is marked "tested", and the runs are in [prototype-log.md](../prototype-log.md) (2026-10-29, remote control C5). See also [safe-points.md](safe-points.md) and [notice-window.md](notice-window.md) (the main menu object and its frame).

## The main menu object

- **Class and frame**: vtable `0x1d5a8c8`; its frame `0x10d2e80` (`rcx` the object) runs on every page. It is also the current idler while it shows: the start command reaches it through `world+0x1e00` (below).
- **Pages**: the page shown is the u32 at `+0x900`; the pages are an array of pointers at `[+0x8e8]`, by page number, each pointing to the page's interface. `0x10daf60(main menu, page, animate)` shows a page: the old page's interface method `+0x18` (leave), then the new one's `+8` (enter), then, when `animate` is set, its `+0x10`. Page 0 is the first page; 3 is the single player setup; 4 the multiplayer lobby.
- **The application** is at `+0x330`. Its idler switch: `0x14c2530(application, &idler, keep)` sets `application+0x48` (pending) and keeps the new idler at `+0x70`; the application's frame `0x14c25a0` switches when `+0x48` is set (notice-window.md).
- **Starting a game**: `+0x714` (u8) is set by the main menu's method `+0x320` (`0x10dc290`), which the single player setup's start command calls when it runs (`0x738f10`, the command's execute, through `world+0x1e00`). The single player setup's frame (`0x1117670`, its interface's `+0x20`) then calls `0x1120dd0`, which sets `+0x715` (u8). The main menu's frame calls `0x10da520` every time (`0x10d3c55`); with `+0x715` set it logs "Launching SINGLEPLAYER-game" and the start date, makes the game idler (`new(0x5bc8)`, `0x803270`) and queues the switch through `+0x330`. So "a game is starting from the main menu" is any of `+0x714`, `+0x715`, `application+0x48`, or a save being read.

## The single player setup (page 3)

- **Class**: `gamesetup.cpp`; the page object's vtable `0x1d5e278`, its interface (what the pages array holds) `0x28` bytes into it with vtable `0x1d5e320`, its main menu at `+0x30` (constructor `0x1108700`, `0x11087b0`).
- **Entering it** (`0x110dda0`, the interface's `+8`): in single player (`world+0x747c` 0) it sets up a session and the setup's windows; unless `main menu+0x910` is set (continuing, below), when the text at `0x1fd39a8` (a `std::string`) is not empty, it finds that tag (`0x286120`, the country manager's lookup by letters), selects that country (`0x111cb80`), empties the text and starts the game (`0x1120280`). That text is the `-start_tag` launch option's (read in `0x8757e0`; the main menu's constructor `0x10d23c0` logs "start_tag is set, skipping main menu" and sets `+0x730`, and the main menu's frame then shows page 3, `0x10d3a68`).
- **Selecting a country**: `0x111cb80(setup, &tag, 0)` posts a command (`0x277310`) to the session (`main menu` method `+0x80`, its queue at `+0x58`); `0x763840(world, tag)` makes the country the world's player at once.
- **Starting**: `0x1120280(setup)` sets `+0x2cb0` (u8, set nowhere else but by the start paths and cleared only by the constructor) and posts the start commands (one with vtable `0x1caef90`, whose execute is `0x738f10`). `0x1120740(setup, continuing)` is what the continue box calls: it stores `continuing` at `world+0x7482`; in single player, not ironman, with a save chosen (`0x111bfa0`: the path at `+0x338`), it calls `0x1120280`; without a save it opens a dialog first, whose answer calls `0x1120280` (`0x13b7190`); for ironman it calls `0x111f930`.
- **The save chosen**: `+0x338`, a `std::string` with the save's PhysFS path (`save games/<file>.eu4`, UTF-8); `+0x358`, u8, the save is in the cloud; `+0x35c`, i32, the save list's selection (-1 for none). `0x1132ca0(setup, null)` reads that file (PhysFS; a zip when it starts with `PK`) and, in single player, hands it to `0x110d760(setup+0x58, path, bytes, 0)`, which loads it into the world (`0xd84b80` → `0x74b8a0` → the save loader `0x5cdc60` that `saves` observes), shows "SAVE_LOADED" or "LOAD_GAME_FAILED" in the setup, and on success shows the political map mode. After the load the world's player tag (`world+0x1e60`, or `+0x1e68` while switched) is the save's player.

## The continue box

`0x10ef410` (inferred: the box's click, on an object of the first page whose `+8` is the main menu): reads the continue information (`0x210a80(application)`, a copy of what the application keeps at `+0x5d8`, read once at startup from `continue_game.json` by `0x5d1670` and rewritten after each save by `0x5d1050`), checks it can be continued (`0x10da420`), then:

1. sets `main menu+0x910` to 1 (continuing), `world+0x747c` to 0 (single player) and `world+0x23b4` to 1;
2. shows page 3 without animation (`0x10daf60(main menu, 3, 0)`), then finds the setup in the pages array;
3. writes `-1` to `setup+0x35c`, the save's path to `setup+0x338`, the cloud flag to `setup+0x358`, and, with a path, the player's tag to `setup+0xc48`, and calls `0x1130f40(setup)`: it loads the chosen save (`0x1132ca0`) and selects `setup+0xc48`'s country (an ironman save: the world's player, through `0x111cb80`);
4. makes that country the world's player (`0x763840`) and starts (`0x1120740(setup, 1)`).

`game::menu::load` does the same with the save asked for, except that it selects the world's player after the load, since it has no continue information to take the tag from.

## Returning to the main menu

- **The flag**: the game idler's (`campaign::classes::MAP_MANAGER`, `world+0x1e00` in a game) `+0x12e9`, u8. The return question's answer (`0x11bddf0`, a dialog method, vtable slot at `0x1d65a08`) sets it, and so does the end-of-game summary when it is left (`0x8156ba`, from `0x140cdc0`).
- **What the idler's frame does with it** (`0x8137c0`, at `0x815c9f`; only when the frame's second argument is set, as in a normal frame): `0x80fee0(idler)`, `0x5cc160()` (it reads the world's ironman flag `+0x24d0`; inferred: it saves an ironman game), then `0x14c2500(application, "-skiplauncher")`, which sets `application+0x83` (quit) and `+0x84` (start again) and keeps the extra argument at `+0x88`; the rest of the frame is skipped (`0x815d5c`).
- **The application's run** (`0x14c2d00`, from `0x8757e0`) loops over its frame while `+0x83` and `+0x2f8` are clear; after the loop it shuts down (vtable `+0x50`) and, with `+0x84` set, calls `0x14c3a10`: the command line (`GetCommandLineA`, rebuilt by `0x14c3100`, which handles `continue` and the login arguments) is started with `CreateProcessA` on the module's own path ("Failed to restart process: " when that fails). Then `0x14c2d00` returns and the process ends.
- Tested: see prototype-log.md.

## The save games folder

`save games` in the user directory (`paths::user_dir`), where the game writes saves; continue information names a save as `save games/<file>.eu4`. Saves are zip files (`gamestate`, `meta`, `ai`, deflated) or plain text; the framework lists the files without reading them.
