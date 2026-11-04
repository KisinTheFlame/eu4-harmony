# Where the main thread's time goes (for making the game faster)

Baseline 1.37.5. All addresses are RVAs, into `eu4.exe` unless another module is named. Measured on 2026-11-04 with [`tools/profile.py`](../../tools/profile.py), which samples the main thread's call stack from outside the process, in the running game with the framework installed (the player build that was in the game folder, built on 2026-11-04 at 01:31); the runs are in [prototype-log.md](../prototype-log.md). What is marked _inferred_ was concluded from the shape of the code or from where it lies ([architecture.md](architecture.md), "Where the code lies"), not read through. The frame, the day and the threads are described in [architecture.md](architecture.md); this note adds what they cost.

## Setup

- The machine and screen of [performance.md](../performance.md): Ryzen 7 9700X, RTX 4070 Ti SUPER, 3840×2160 borderless, 8× multisampling, vsync on at 144 Hz.
- The player's playset (the Chinese language mod and four others), the game in English, system fonts.
- Three games:
  - **1462**: a normal save of 1462.8.30 (Ardabil), run at speed 5 for 60 s;
  - **1624**: an ironman save of 1618.9.2 (Spain), run at speed 5 from 1620.9 to 1624.7, the 120 s measured from 1624.7;
  - **paused**: the 1462 save, paused, on the map as the load leaves it.
- The game ran through remote control: `time.set_speed 5` and `time.resume`, every question answered with its first option and every offer declined as they came. Half-yearly autosaves, the game's setting.
- Samples at 50 a second. Each one suspends the main thread for about 1.4 ms, 7% of the time, which the days per second below include.

## The shares

Shares of the main thread's samples. A sample counts where the world's flags put it: inside a day (`world+0x1e10` set), inside a frame that runs inside a day (`world+0x1e18` set too), or in a frame between days.

| | 1462 | 1624 | paused |
| --- | --- | --- | --- |
| Days a second at speed 5 | 16.8 | 9.8 | |
| **The day** (`0x758430`, outside its frames) | **65.4%** | **38.6%** | |
| … each country's daily update (`0x2ee280`) | 32.8% | 18.9% | |
| … the monthly update (`0x75d690`) | 15.2% | 8.6% | |
| … the AI (`0x75b4b0`) | 7.3% | 4.7% | |
| … the provinces' passes (`0x75b2d0`) | 4.7% | 3.5% | |
| … the rest of the day | 5.4% | 2.9% | |
| **Frames inside a day** (`0x769ea0`) | **6.6%** | **31.5%** | |
| **Frames between days** | **28.0%** | **29.9%** | **100%** |
| In both kinds of frame: drawing (`0x8183b0`), Present included | 8.5% | 16.0% | 37.6% |
| … POPS waiting (`pops_api.dll`, below) | 6.5% | 5.0% | 32.2% |
| … the autosave (`0x5cbb00`) | 4.2% | 9.6% | |
| … the commands executed between days (`0x20f810`) | 5.1% | 2.9% | 0.1% |
| … the GUI update (`0x14c5810`, POPS left out) | 1.7% | 3.5% | 6.8% |
| … input (`0x830fd0`) | 1.0% | 1.5% | 3.5% |
| … the rest of the frames | 7.6% | 22.9% | 19.8% |

- **The main thread is what limits the game.** It ran 0.84 to 0.87 cores at speed 5, suspended by the sampler 7% of the time and asleep or waiting the rest (POPS's sleeps, the driver, the workers). Each other thread ran at most 0.16 cores, all of them together 1.0 to 1.6, on a machine of 16 hardware threads.
- **Later in the game, more than half of the main thread's time is not the day.** In 1624 a day took about 40 ms of simulation and the frames drawn for it about 60 ms; in 1462, 39 ms and 21 ms. The difference is the frames inside days, which the slower days bring (below).
- **Paused**, a frame is drawing, the GUI update and POPS's waits; a third of it is POPS.

## Frames inside a day

`0x769ea0(world, …)` is called between the daily update's phases ([architecture.md](architecture.md), "Frames inside a day"). Read in the code:

- It sets the random-forbidden flag `0x1fd10c4` and `world+0x1e18`, and does nothing more when the world has no map manager (`world+0x1e00`) or the switch `[0x1fc23d8]` is clear (1 by default, and in these runs).
- The interval is the float `[0x245b070]` times the speed's factor at `0x1c33e18 + 4 × speed`: 0.02 s at speeds 1 to 4 and 0.05 s at speed 5. `[0x245b070]` is copied once from the settings object (`[0x242bde8]+0x284`, at `0x76a20a`); it was 1.0 in these runs. Which line of `settings.txt` sets it was not found.
- When more than the interval has passed since `world+0x1e20` (or the clock went back), it runs a whole application frame, `0x14c25a0(app, 0)` (input, the in-game frame, drawing, Present), calls the session's `+0x40` with 1, and sets `world+0x1e20` to the clock.
- Then it puts both flags back as they were.

So at speed 5 a day that takes longer than 50 ms gets a frame drawn every 50 ms while it runs, on top of the frame between days: the screen keeps to at least 20 frames a second however slow the days are.

Such a frame does not feed the day: the random-forbidden flag is set (a draw gives 0 and is logged), and the session does not execute commands inside it ("Update within execution. Skipping.", [architecture.md](architecture.md)). The player's input it takes is posted as commands, executed at the next frame between days.

## POPS waits on the main thread

`pops_api.dll` (Paradox's account, social and telemetry services; SHA-256 `2f9e2b56…`) is served twice in every application frame: by the frame itself (`0x14ada90`) and by the GUI update (`0x14c5810`). Both reach `pops_api.dll+0x6bd70`, read with Ghidra:

- `+0x6bd70(client)`: when the client is on (`+0x48`), it sends a keep-alive every 45 s (`_time64`, `+0x50`), then calls `+0x1bf9f0(loop, 1)`.
- `+0x1bf9f0(loop, timeout_ms)` is an event loop over sockets. It sends what is queued, then, when the loop has sockets, `select`s on them with the timeout; **when it has none, it calls `Sleep(timeout_ms)`** (`if (param_2 != 0) Sleep(param_2)`).

In these runs the sleep was what the main thread did there: `kernelbase!SleepEx` under `+0x1bf9f0`, through `ZwDelayExecution`, in every sample of `pops_api.dll` but a few. Two sleeps of one millisecond (rounded up by Windows' timer) in every frame came to 5 to 6.5% of the main thread at speed 5 and 32% of a paused frame. What the client is connected to, and whether a signed-out player's game has the client on, was not looked at.

## The autosave

The half-yearly autosave runs in the in-game frame (`0x8137c0` → `0x5cbb00` → `0x5cc160` → `0x5cc6e0`, where the framework's save observer sits, → `0x5c7ac0` → `0x74ba20` → `0x5cde60` → `0x5ce5f0`), and in 1624 mostly inside a frame inside a day. One autosave took about 0.4 to 0.5 s in 1462 and 1.6 to 1.9 s in 1624 (samples per autosave, with the sampler's 7%). Of the 1624 autosaves ([prototype-log.md](../prototype-log.md), the first run at 200 samples a second):

- 55% writing the game state into memory: `0x14e3f00` → the world's writer `0x74ff30` ([saves.md](saves.md)), most of it in `0x2dc6b0` (_inferred_: the countries) and `0x9d6e60` (_inferred_: the provinces);
- 40% in `0x153b9a0` → `0x153c790` → `0x181e890`, which lies among the third-party code (_inferred_: zlib's deflate compressing the zip; that save is an ironman save, compressed although `compress_autosave=no`);
- the rest waiting for a thread it joins (`tbb::internal::tbb_thread_v3::join` from `0x5cde60`).

## Drawing

From the profiles of 2026-11-04 (Austria-1, 1618 to 1620, with S1 and S2 built: 3,000 samples at speed 5, 1,000 paused), as shares of all the samples. Drawing (`0x8183b0`) was 21.0% at speed 5 and 37.6% paused.

| | Speed 5 | Paused |
| --- | --- | --- |
| The map, `0x10a59a0` | 10.9% | 16.4% |
| of it, the map's views, `0x10a55c0` | 4.4% | 11.2% |
| of it, the views' second part, `0x14dcde0` | 4.8% | 3.0% |
| The GUI, `0x14dd010` | 7.3% | 10.9% |
| Present, `0x14ddc40` (through the framework's observer) | 1.3% | 7.7% |
| `0x10a51e0` (under it `0x12dab90`, unnamed) | 1.4% | 2.1% |

- **The map draws up to three views a frame.** `0x10a59a0` updates its fades, then `0x10a55c0` draws the views in turn, each into its own target: the first always, the second when `0x8023b0` says so, the third when two flags of its third argument (`+0x778`, `+0x679`) are set; what the second and third are (_inferred_: a reflection or shadow view) was not looked at. Each view goes through `0x1540c70`, then `0x14dcde0`, `0x13f9520` and `0x14dcd30` with the view's index.
- **Most of the map's time is in issuing draws.** Paused, 4.8% of the samples were in `0x16d4f00`, the game's `DrawIndexedPrimitive` (the device's method `+0x290`), nearly all inside `d3d9.dll`: the driver's work on the CPU, per draw call, not the game's. The GPU's work does not show on the main thread except as Present's wait.
- **The GUI already leaves out what is clipped away.** `0x14dd350` draws a list of the GUI's objects; an object clipped (its flag at `+0x130`) whose rectangle (`+0x128`) comes out empty against the screen is skipped, the others are drawn by their method `+0x58`, with a scissor rectangle when clipped. Under it, `0x15f3f20` → `0x15988e0` and `0x15bd540` → `0x156cab0` → `0x156bfb0` were the largest (unnamed).
- **Present paused is the wait for the display**: at 84 frames a second, vsync at 144 Hz, the frame waits there for the GPU (_inferred_).

So the main thread's drawing is the game's draw calls, about as many as the frame has objects: what would draw the same frame for less is fewer draw calls (batching, or caching what did not change), a change to the renderer, not a skip of work nobody sees. At speed 5 with "Faster time", most frames drawn are gone anyway (S4).

## Other costs found

- **The parallel stages' wait.** `0x179e90`, the engine's run-and-wait for a stage's parts ([architecture.md](architecture.md), "Parallel work"), had 3.8 to 5.1% of the samples as their own, nearly all under the AI's stage `0x77d8a0`: the main thread waiting, or spinning, while the workers finish.
- **The heap.** `RtlAllocateHeap` and `RtlFreeHeap` (through the game's `0x1a332d4`, _inferred_: `operator new`) 4 to 6% of the samples together, critical sections 1 to 2%.
- **Files.** PhysFS asks Windows for files' attributes (`GetFileAttributesExW` from `0x1a061d0`) in frames: 0.2 to 0.7%.
- **The framework's own.** ICU's line breaking (`icu.dll`, which only the framework loads) 1.8% in 1462 and 6.0% in 1624, in the text the frames draw. The framework's observer on the in-game frame is on most stacks, but its own samples are few.
- **Present and vsync** were 1% at speed 5: frames were never fast enough for vsync to hold them.

## The game does not repeat itself

Loading one normal save and letting the same days pass, with the same answers, does not give the same game twice (tested with the 1462 save, every province and the 20 largest countries read through remote control):

- after 0 and 1 days, every value the same;
- after 10 days, 13 provinces and the treasury or manpower of at least 6 of the 20 countries different, and 3 provinces and 8 countries with the game started with `-threads=1` (whether that option makes the worker pools smaller was not looked at);
- after 120 days, 24 to 32 provinces and all 20 countries different, and one run's player got one question, another two, the others none.

The generator's state was read at `0x23494b4` (the seed) and `0x23494b0` (the draw count): each load set both to the save's (`0xd7568eb0`, 61,375,213), and paused frames drew nothing. The first day drew 8,664 to 8,763 numbers, varying by run whether the day was run at speed 1 (about 300 frames) or at speed 5 (one or two), so the difference does not come from frames between days; it comes from inside the day. Where was not found: the parallel stages merge their results under a lock ([architecture.md](architecture.md)), and the AI may stop on time (neither looked at). The first 16 bytes at `0x2325220`, where the generator is, differed between two loads with the same seed and count, so `0x2325220` may not be the generator's state, or the load does not set it.

Multiplayer needs every machine to compute the same day from the same commands, so whatever varies here varies only on one machine: inside the AI, which only the host runs and which acts through commands (_inferred_).

## Open questions

- Which line of `settings.txt`, if any, sets the frames' interval (`[0x242bde8]+0x284`).
- What makes a day draw a different number of random numbers from one run to the next.
- What the "rest of the frames" is in 1624 (22.9%): `0xdf5860`, `0x10a6d00`, `0x6ea380`, `0x128cd30` and the framework's frames between them were the largest. By the strings they load, read since: `0xdf5860` updates the top bar's alerts (`ALERT_DISASTER_ENTRY`, `ALERT_REINFORCING_UNITS_DELAYED` and the rest; 9,297 instructions), `0x128cd30` the outliner (`outliner_standard_entry`), and `0x1321880`, under `0x10a6d00`, the map's markers (`capital_indicator`, `fort_15th` to `fort_18th`, `port_blockade`).
- What the map's second and third views are, and whether a player's graphics settings turn them off.
- Whether POPS's client is on for a player who is not signed in, and what it is connected to.
- The profile of a vanilla game: these runs had the framework installed, whose observers and line breaking are part of the samples.
