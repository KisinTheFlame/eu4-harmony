# Time: speed, pause, the daily tick, game events waiting, restarting (for `game::time`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` with `core/examples/disasm.rs`; what was checked in the running game is marked "tested", from [prototype-log.md](../prototype-log.md) (2026-10-29, remote control C1).

## The speed

- **The world's speed** is an i32 at `world+0x1e08`, 0 to 4, shown to the player as speeds 1 to 5. The speed setter `0x7579f0(world, index)` clamps the index to 0..=4, stores it there and logs `Gamespeed is now <index+1>`.
- **The speed commands** call that setter: the speed-up command's execution `0xb79970` sets `[world+0x1e08] + 1` (its constructor `0xb79920`: vtable `0x1cea1d8`, type `0x165`); the slow-down command's `0xb79ab0` sets `[world+0x1e08] - 1` unless the map manager's method at `+0x3f8` says otherwise at speed 0.
- **How fast days go**: the time step `0x757ad0(world, seconds)`, called by the map manager's frame `0x8137c0` when the game is not paused, adds the frame's seconds up and, once they pass the speed's threshold (the floats at `0x1c33e30`: 2.0, 1.0, 0.5, 0.2 and 0.0 seconds a day for speeds 1 to 5), posts one daily tick command (vtable `0x1cea280`, id `0x28d6`) through the command dispatch `0x14ca2d0` and starts adding up again. So at most one day is posted a frame; at speed 5 one every frame.
- **The daily tick** executes `0xb79520`, which calls the daily update `0x758430(world)` (docs/research/regional-combat/native-scheduler-findings.txt). The date moves on by 24 inside it.
- **The console's `gamespeed <n>`** (`0xe913a0`) calls the setter with `n` as the index (so `gamespeed 3` is speed 4), then toggles the pause: on when the index is 0 and the game runs, off when it is above 0 and the game is paused.

## Pausing

The map manager's pause state is at `map_manager+0x418` (the manager's method at `+0x100`, `0x8024d0`, returns it; its owner at `+0x50`). It holds:

- `+0x00`/`+0x08`: begin and end of a vector of pointers, **the windows holding the game paused**. `0x7e0a20(state, window)` adds one if it is not there yet. An event window adds itself in its constructor when the game pauses on events (the byte at `[0x242bde8]+0x256`, set by the settings' `autopause_on_events_button`), and its destructor `0x10ba030` takes itself out (`0x1aa090`). The other callers of `0x7e0a20` (`0x820990`, `0x8c07b0` in `messagehandler.cpp`, `0x118a8e0`, `0x118b8b0`, `0x11c4390`, the peace offer and demand windows at `0x1307aa0` and `0x1307d90`, `0x130a4a0`, `0x1347a50`) were not all identified.
- `+0x48`: **the player's pause** flag (u8); `+0x28`: the name of who paused.
- **Paused** (`0x7e01b0`, existing dependency point): the flag, or, when the helper `0x6eaa10` says pause requests count (always in single player), a window in the vector.
- **The pause toggle** `0x7e0250(state)`, which the console's `gamespeed` uses: when the flag is set, or windows hold the game paused, it clears the flag and the vector (resuming lets go of every window, which stays open); otherwise it sets the flag and the name (the player's). It plays no sound and writes no log line.
- **The pause command** (execution `0x848970`, MP-safe, what the player's pause goes through): with its "pause only" byte (`+0x71`) it calls the pause `0x7e02d0` ("Game paused by …", a cooldown timer for multiplayer); otherwise it toggles: the unpause `0x7e0770` ("Game unpaused.", the `unpause` sound, the vector cleared) when the flag is set, else the pause. `game::time` uses the toggle, which has the same effect in single player without the sound.

## Stopping on a date

Since one day is posted a frame and the frame checks the pause before posting, pausing as soon as the daily update returns with the date reached means no further day begins. `game::time` pauses there, after `0x758430` (an `after` observer on the existing daily update point), when remote control's advance holds a stop at that date, and also when a game event's window came up during the day while an advance runs.

Pausing at a safe point instead is a day late at speed 5. Tested: an advance that paused the game at the safe point where it found a game event's window answered 1470.9.22, and the game then stood at 1470.9.23: the next day had been posted before the safe point and ran after it. With the pause at the end of the daily update, an advance of 30 days at speed 5 stopped on its date.

## Game events waiting for an answer

- The map manager's method at `+0x108` (`0x828c00`) shows a game event to the player: it makes an event window (`0x10b9a50`, 0x3e8 bytes, vtable `0x1d590f8`, a second one `0x1d590e0` at `+8`; `eventwindow.cpp`, `EventWindow`) and adds it with `0x6ecae0(manager, window)` to the manager's **popups**.
- **The popups** are a list at `map_manager+0x78` (first node), `+0x80` (last) and `+0x88` (count). A node is 0x20 bytes: `+0x00` the window, `+0x08` the previous node, `+0x10` the next, `+0x18` a byte set when the window closed while the list was being walked (the node goes later). Each frame `0x6ea380` asks every window whether it is still open (its method at `+0x18`) and takes the closed ones out.
- So **the game events waiting** are the nodes whose window has the event window's vtable and whose `+0x18` is clear. Tested: see the log.

## Returning to the main menu

- The application's run `0x14c2d00` loops over frames (`0x14c25a0`) while neither `app+0x83` (quit) nor `app+0x2f8` is set, then shuts the game down, and, when `app+0x84` is set, calls **the restart** `0x14c3a10(app)` (`application.cpp`), which starts `eu4.exe` again with `CreateProcessA` (log `Failed to restart process: ` on failure) and returns; the process then ends. Returning to the main menu ends this way (docs/research/safe-points.md).
- `game::time` observes the restart (before it runs) so that remote control answers what waits while this process can still write to the pipe.
