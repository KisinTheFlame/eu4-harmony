# Frames inside a day and scheduled work

## Frames inside a day

`0x769ea0(world, …)` is called between the daily update's phases ([architecture.md](../program/architecture.md), "Frames inside a day"). Read in the code:

- It sets the random-forbidden flag `0x1fd10c4` and `world+0x1e18`, and does nothing more when the world has no map manager (`world+0x1e00`) or the switch `[0x1fc23d8]` is clear (1 by default, and in these runs).
- The interval is the float `[0x245b070]` times the speed's factor at `0x1c33e18 + 4 × speed`: 0.02 s at speeds 1 to 4 and 0.05 s at speed 5. `[0x245b070]` is copied once from the settings object (`[0x242bde8]+0x284`, at `0x76a20a`); it was 1.0 in these runs. Which line of `settings.txt` sets it was not found.
- When more than the interval has passed since `world+0x1e20` (or the clock went back), it runs a whole application frame, `0x14c25a0(app, 0)` (input, the in-game frame, drawing, Present), calls the session's `+0x40` with 1, and sets `world+0x1e20` to the clock.
- Then it puts both flags back as they were.

At speed 5, eligible checkpoints attempt an intervening frame after 50 ms; a long phase or blocking call can still exceed that interval.

Such a frame does not feed the day: the random-forbidden flag is set (a draw gives 0 and is logged), and the session does not execute commands inside it ("Update within execution. Skipping.", [architecture.md](../program/architecture.md)). The player's input it takes is posted as commands, executed at the next frame between days.

## POPS waits on the main thread

`pops_api.dll` (Paradox's account, social and telemetry services; SHA-256 `2f9e2b56…`) is served twice in every application frame: by the frame itself (`0x14ada90`) and by the GUI update (`0x14c5810`). Both reach `pops_api.dll+0x6bd70`, read with Ghidra:

- `+0x6bd70(client)`: when the client is on (`+0x48`), it sends a keep-alive every 45 s (`_time64`, `+0x50`), then calls `+0x1bf9f0(loop, 1)`.
- `+0x1bf9f0(loop, timeout_ms)` is an event loop over sockets. It sends what is queued, then, when the loop has sockets, `select`s on them with the timeout; **when it has none, it calls `Sleep(timeout_ms)`** (`if (param_2 != 0) Sleep(param_2)`).

In these runs the sleep was what the main thread did there: `kernelbase!SleepEx` under `+0x1bf9f0`, through `ZwDelayExecution`, in every sample of `pops_api.dll` but a few. Two sleeps of one millisecond (rounded up by Windows' timer) in every frame came to 5 to 6.5% of the main thread at speed 5 and 32% of a paused frame. What the client is connected to, and whether a signed-out player's game has the client on, was not looked at.

## The autosave

The half-yearly autosave runs in the in-game frame (`0x8137c0` → `0x5cbb00` → `0x5cc160` → `0x5cc6e0`, where the framework's save observer sits, → `0x5c7ac0` → `0x74ba20` → `0x5cde60` → `0x5ce5f0`), and in 1624 mostly inside a frame inside a day. One autosave took about 0.4 to 0.5 s in 1462 and 1.6 to 1.9 s in 1624 (samples per autosave, with the sampler's 7%). Of the 1624 autosaves ([Where the main thread's time goes](../../research/mechanics-evidence.md), the first run at 200 samples a second):

- 55% writing the game state into memory: `0x14e3f00` → the world's writer `0x74ff30` ([saves.md](../program/saves.md)), most of it in `0x2dc6b0` (_inferred_: the countries) and `0x9d6e60` (_inferred_: the provinces);
- 40% in `0x153b9a0` → `0x153c790` → `0x181e890`, which lies among the third-party code (_inferred_: zlib's deflate compressing the zip; that save is an ironman save, compressed although `compress_autosave=no`);
- the rest waiting for a thread it joins (`tbb::internal::tbb_thread_v3::join` from `0x5cde60`).


- **The map draws up to three views a frame.** `0x10a59a0` updates its fades, then `0x10a55c0` draws the views in turn, each into its own target: the first always, the second when `0x8023b0` says so, the third when two flags of its third argument (`+0x778`, `+0x679`) are set; what the second and third are (_inferred_: a reflection or shadow view) was not looked at. Each view goes through `0x1540c70`, then `0x14dcde0`, `0x13f9520` and `0x14dcd30` with the view's index.
- **Most of the map's time is in issuing draws.** Paused, 4.8% of the samples were in `0x16d4f00`, the game's `DrawIndexedPrimitive` (the device's method `+0x290`), nearly all inside `d3d9.dll`: the driver's work on the CPU, per draw call, not the game's. The GPU's work does not show on the main thread except as Present's wait.
- **The GUI already leaves out what is clipped away.** `0x14dd350` draws a list of the GUI's objects; an object clipped (its flag at `+0x130`) whose rectangle (`+0x128`) comes out empty against the screen is skipped, the others are drawn by their method `+0x58`, with a scissor rectangle when clipped. Under it, `0x15f3f20` → `0x15988e0` and `0x15bd540` → `0x156cab0` → `0x156bfb0` were the largest (unnamed).
- **Present paused is the wait for the display**: at 84 frames a second, vsync at 144 Hz, the frame waits there for the GPU (_inferred_).


## Limits of the reading

The setting controlling frame interval `[0x242bde8]+0x284`, all map-view roles and POPS behavior when signed out remain unresolved. Samples included Harmony, so they are not a vanilla-only profile. The top-bar alerts (`0xdf5860`), outliner (`0x128cd30`) and markers under `0x1321880` explain some former unnamed frame work, not its full cost.
