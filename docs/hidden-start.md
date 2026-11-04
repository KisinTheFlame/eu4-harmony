# Hidden start plan

Status: H1 to H4 done (2026-11-04). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Hidden start). What the game does when it is not seen, what could still reach the player's desktop and what the hidden start relies on is in [research/headless.md](research/headless.md); the runs are in [prototype-log.md](prototype-log.md), "The game unseen" and "The hidden start".

## Goal and scope

- **Goal**: the game runs for remote clients with nothing of it on the player's desktop, while the player goes on working in other programs: no window, no sound, the focus, Windows' cursor and the display left alone. The game itself runs as when it is played: it draws every frame at the size the player plays at, and `screenshot` and `input` work as they do with the window shown.
- **In scope**: starting the game hidden, by a switch on its command line and by a command of the command line tool; sound off in a hidden start unless asked for; showing and hiding the game again through remote control; what the framework does instead of a message box while hidden.
- **Out of scope**: not drawing frames; a machine without a graphics card; more than one game at once ([research/headless.md](research/headless.md), "What it would take"); turning sound on or off while the game runs.

## Behavior

### Starting hidden

- `eu4-harmony start --hidden` starts `eu4.exe` from the game folder with the switch `-harmony_hidden`, and returns once remote control answers that the main menu is up (300 seconds at most, `--timeout` for another), printing the game's process id; when the game ends first, it says so with the reason from the framework's log of this start and fails. Steam must be running, as for any start; a game already running is refused before anything starts, and so is a hidden start into a game folder without the framework, or with one from before the hidden start, which would pass the switch over and show the game. Without `--hidden` it starts the game as the player would see it.
- The switches are the framework's, read from the game's command line, so `-harmony_hidden` in Steam's launch options does the same for a player who starts the game themselves; the start command only saves typing the path and waits for the main menu. Nothing else needs to be passed: what the game's own switches would do for a hidden start, the framework does itself.
- The start command finds the game folder as every command of the tool does: `--game-dir`, else `EU4_GAME_DIR`, else the folder the tool is installed in, else the Steam library that holds the game.
- A return to the main menu stays hidden: the in-process return keeps the process and its window.
- The player build has the hidden start; it is not a development tool.

### What a hidden game does

- **No window on the desktop.** The game's window is made hidden and is never shown, not even for a moment: no splash, nothing in the taskbar or Alt+Tab, and the focus stays with the program the player is in.
- **The size the player plays at.** The frame is the size the game's settings give it. With full screen in the settings the game made its window and its device windowed, at the settings' resolution, and the display was left alone (tried, below); should the window ever be made full screen, which would take the display, the hidden start ends the game instead.
- **Drawing goes on.** Every frame is drawn and presented as with the window shown; the window is never minimized, which would slow the game to about ten frames a second.
- **Windows' cursor and keys are the player's.** Remote control holds the game's cursor from the start, in the middle of the window, where it does not scroll the map, until the first input moves it; where the game would move Windows' cursor, it moves remote control's instead. What the player types and clicks in other programs never reaches the game.
- **No messages on the desktop.** What the framework would show in a message box (a failed startup, a fatal error, notices without the framework UI) goes to the framework's log only. A framework that fails to start ends the game, since without the framework a hidden game could neither be used nor kept hidden. The game's own error messages are left out as its `-suppresserrormessages` leaves them out, without the switch.
- **No crash reporter.** After a crash the game writes its crash files as usual, is not minimized (which would put it on the taskbar), and does not start its crash reporter, a program of its own with a window; the start command, or the next request, finds the game gone.

### Sound

- A hidden start has no sound: SDL's `dummy` audio driver takes the game's mixed sound and plays none, so nothing reaches the player's speakers and the game opens no audio device.
- `eu4-harmony start --hidden --sound` (the switch `-harmony_sound`) keeps the game's sound, at the volumes of its settings, for a client that wants it heard.
- Sound is chosen at the start and stays so until the game ends, also when the game is shown: turning it on later would take a dependency point in the game's mixing for a need not yet seen. A remote client that wants to know which sounds play is better served by an event of its own.

### Showing and hiding

| Capability | Arguments | What it does |
| --- | --- | --- |
| `desktop.get` | | `hidden` (the window is hidden now), `hidden_start` (it started hidden), `sound` (it plays sound) |
| `desktop.show` | | Shows the window of a game started hidden, as the player would have it, without taking the focus; from then on the player's mouse and keyboard reach it as usual |
| `desktop.hide` | | Hides it again, remote control holding the cursor as at a hidden start |

Both `desktop.show` and `desktop.hide` are refused (`invalid_argument`) for a game not started hidden: such a game was started for the player to see, plays its sound, and may be full screen, which a hidden window cannot keep. They are remote control's only; extensions do not have them.

## How it fits together

One module of the core, `hidden`, reads the switches once as the framework starts (before the game's `main`), and answers whether the game was started hidden and whether it is hidden now. Its dependency points pass through unchanged in a start that is not hidden:

- where the window is made (`0x14c40af`), its flags take `SDL_WINDOW_HIDDEN` in place of `SDL_WINDOW_SHOWN`, since SDL shows every window made without it;
- Init's `ShowWindow` (`0x14c02ca`) is left out while hidden;
- `main`'s store of `-suppresserrormessages` (`0x876b37`) stores it on for a hidden start;
- the input device's move of Windows' cursor (`0x162e4d0`) moves remote control's held cursor instead while hidden;
- after a crash, its minimizing of the window (`0x1654696`) is left out while hidden, and the start of the crash reporter (`0x164f950`) for a hidden start.

Sound needs none: before SDL's audio starts, the framework sets `SDL_AUDIODRIVER` to `dummy` in its own process, which SDL reads then. The cursor held from the start is `game::input`'s own. The framework UI finds the game's window, which it needs for input, by its class (SDL's, `SDL_app`) when it is hidden. Notices and message boxes ask whether the game is hidden now. The start command is the command line tool's, as `link` is.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, for the player build and with `dev`, and so do the offline checks against `eu4.exe`;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- docs/research holds what was found in the game's code, every new place in it is a dependency point, and README.md and README.zh-CN.md say what changed for players.

### H1 What a hidden game needs (done)

Found in the game's code, in [research/headless.md](research/headless.md), "What the hidden start relies on": where the window's flags are set and why SDL shows a window made without `SDL_WINDOW_HIDDEN`; Init's `ShowWindow`; the flag `-suppresserrormessages` sets and what reads it; the input device's cursor move; the crash handling's minimizing and its start of the crash reporter; that SDL's audio starts after the framework and has the `dummy` driver. Not followed: everything that reads the full screen setting, and why a hidden start with full screen in the settings made its device windowed.

### H2 The hidden start (done)

The `hidden` module and its dependency points; the cursor held from the start; sound off unless `-harmony_sound`; message boxes to the log while hidden and a failed startup ending the game; the game's error messages and its crash reporter left out. A developer build's test switch `crash` crashes the game at the first frame of a game, to try what follows a crash.

### H3 Showing and hiding (done)

`desktop.get`, `desktop.show` and `desktop.hide`, in remote control and the command line tool's catalogue.

### H4 The start command (done)

`eu4-harmony start [--hidden] [--sound] [--timeout <seconds>]` and its usage text. The development tools that start the game themselves (`cargo xtask perf-run`, `leak-run`) still start it shown; moving them onto the start command belongs to the test management that comes next.
