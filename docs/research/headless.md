# Running the game headless

Whether the game can run with nothing on screen, for remote clients and development runs that need no player watching: what keeps it on screen, what it does when it is not seen, and what each step further would take. Baseline 1.37.5 (`9ad3efe1…`); all addresses are RVAs. Read from `eu4.exe` on 2026-11-04 with `core/examples/disasm.rs` and scripts over the PE file (imports, exports, RIP-relative references, direct calls), then tried in the game the same day ([The game unseen](../prototype-log/2026-11-04-game-unseen.md)). What is marked _inferred_ was concluded from the code's shape or from SDL's and Direct3D's documented behavior, not seen.

"Headless" can mean four things, each harder than the one before:

1. **Unseen**: the window hidden, the game otherwise as it is: drawing, presenting, sound.
2. **Not drawn**: no frame drawn unless a client asks for a screenshot, so the graphics card idles.
3. **No graphics card**: the game on a machine without one, such as a virtual machine.
4. **Several at once**: more than one game on a machine.

## What keeps the window on screen

- **The window is made shown.** `0x14c3dd0` registers a window class, then calls `SDL_CreateWindow` (`0x14c40c1`, through the export `0x17320b0`) at `SDL_WINDOWPOS_CENTERED` (`0x2fff0000`) with:
  - full screen (`settings+0x100`): `SDL_WINDOW_FULLSCREEN | SDL_WINDOW_SHOWN` (5), the settings' size;
  - borderless (`settings+0x101`): `SDL_WINDOW_BORDERLESS | SDL_WINDOW_SHOWN` (0x14), the desktop's size from `SDL_GetDesktopDisplayMode`;
  - otherwise `SDL_WINDOW_SHOWN` (4);
  - `SDL_WINDOW_OPENGL` (2) added for the OpenGL renderer.

  It then draws the splash (`SDL_LoadBMP_RW`, an SDL renderer it destroys after one `SDL_RenderPresent`).
- **The application's Init shows it again**: `0x14c02b0` begins with `ShowWindow(hwnd, SW_SHOWDEFAULT)` and `UpdateWindow` (`0x14c02ca`, `0x14c02d6`). Tried: a window hidden 0.6 s after launch was shown again at 3.1 s, and stayed hidden once hidden after that, through the startup load, the main menu and a game.
- Nothing else in the game calls `ShowWindow`, `SDL_ShowWindow` or `SDL_RaiseWindow`. `SDL_RestoreWindow` is called once, by the event pump on `SDL_WINDOWEVENT_MAXIMIZED` (`0x1569ee3`).

## What the game does when it is not seen

- **The frame is never skipped for being out of sight.** The application frame `0x14c25a0` skips the idler's frame, with `SDL_Delay(50)`, only while `app+0x2f9` is set and `app+0x32b` is not (`0x14c2692`). The pump sets `+0x2f9` on `SDL_APP_WILLENTERBACKGROUND` and clears it on `SDL_APP_WILLENTERFOREGROUND` (`0x1569d75`, `0x1569da0`), events SDL raises on phones and tablets, not on a Windows desktop (_inferred_). Tried: minimized and hidden, days went on.
- **A present that is not shown slows the next frame.** The Present wrapper `0x16d4db0` takes `S_PRESENT_OCCLUDED` (`0x08760878`) as a failure, as it does a negative result. The graphics object's present `0x14ddc40` keeps the answer in `graphics+0x530`, which the device check `0x14df050` sets again while the device is fine. When it is clear:
  - the next application frame waits `SDL_Delay(100)` instead of `SDL_Delay(0)`, unless `app+0x32b` (`0x14c26b3`–`0x14c26e7`);
  - the in-game frame does not draw that frame (`0x8187b6`: it reads the flag before the device check sets it), so the one after draws and presents again.

  So a game whose presents are occluded runs at most about ten frames a second, and draws one in two. Tried: minimized, a remote control request took 86 to 191 ms to be answered instead of about 30, the process used 2% of one core while paused, and 4.1 days passed a second at speed 5 instead of about 16.
- **A hidden window is not occluded.** Tried: hidden but not minimized, requests were answered as fast as with the window shown, the graphics card was as busy, and the speed was the same (below). Direct3D 9Ex reports occlusion for a minimized window (_inferred_ from the timings; the result codes were not read).
- **Windows throttles a process no one sees.** Windows 11 power-throttles (EcoQoS) a process whose windows the player cannot see: its threads are scheduled seldom and on slow cores, and its timer resolution requests are ignored. Measured on 2026-11-10 with two hidden games side by side at speed 5 on the 1780.7.1 save ([A hidden game at full speed](../prototype-log/2026-11-10-hidden-start-at-full-speed.md)): the game as it was used 11 to 14% of a core and passed about 23 days a minute; the one opted out of it (`SetProcessInformation` with `ProcessPowerThrottling`, execution speed and timer resolution controlled and neither throttled) used 168 to 188% and passed about 520. The game that ran as fast hidden as shown on 2026-11-04 (below) was alone; whether Windows throttled it then was not looked at.
- **A lost device** (`0x14df050`: `TestCooperativeLevel`) skips the frame; the first frame lost sleeps 1 s. A device in a window is not lost when it is hidden or minimized (_inferred_); a full screen one is when it loses the foreground. Full screen was not tried.
- **The cursor is read against the active window.** While `[0x1fd10c5]` is set, the game takes the cursor's position as `GetCursorPos` less `GetWindowRect(GetActiveWindow())` ([input.md](input.md)). With the game hidden or behind another program the active window is another program's, so the position the game reads lies wherever the cursor is against that window, often past an edge of the frame, and the map scrolls toward it. Tried: the camera ran to the map's northern edge while the game was hidden and behind; that this was the cause is _inferred_. Remote control's input holds the cursor once it gives one (input.md), after which the camera stayed.
- **Remote control and screenshots** go on unchanged: the framework serves requests from the in-game frame and copies the frame at its present, both of which keep running hidden or minimized. Tried, both.
- **Sound** plays on while hidden (_inferred_: nothing in the frame looks at the window); not listened for.
- **The graphics driver keeps memory while the window is not shown.** With NVIDIA's Direct3D 9 driver (`nvd3dumx.dll`, of 2026-10-21), blocks of 3,072 bytes of the process's default heap, from a pool the driver tags `NVDA` and `NVDX`, are made and never let go at most frames while the window is hidden (`SW_HIDE`, or made with `SDL_WINDOW_HIDDEN` and not shown): 13 to 125 a second at the idle main menu, from run to run, 20 to 22 MB a minute at the most. Shown, or shown and cloaked (`DWMWA_CLOAK`), none are. Tried in one process, hidden, shown and hidden again, and then with the window cloaked ([What grows outside the census, and the hidden window concealed](../prototype-log/2026-11-05-growth-outside-census-window-cloaked.md)). Why the driver keeps them was not looked into, nor other drivers.

## What could reach the player's desktop

What a game that runs unseen could still do to the program the player works in: take the focus, move or confine Windows' cursor, change the display, show a message, or play sound. Read from the code, not tried.

- **The focus.** Only showing the window takes it: `SDL_CreateWindow` with `SDL_WINDOW_SHOWN` and Init's `ShowWindow(SW_SHOWDEFAULT)` (above). The game imports no `SetForegroundWindow`, `SetFocus`, `SetActiveWindow` or `FlashWindow`; SDL's `SDL_RaiseWindow` is never called.
- **Keys.** The player's typing goes to the window that has the focus; the game reads no key state of its own ([input.md](input.md)), so nothing the player types elsewhere reaches it.
- **Windows' cursor.**
  - `SetCursorPos` is called only by SDL (`0x1767970`, `0x17679c0`), for `SDL_WarpMouseInWindow` (`0x1735c90`), which SDL does whether or not the window has the focus (_inferred_ from SDL's source). The game calls it from one place, `0x162e4d0`, slot `+0x40` of the input device's vtable `0x1da3530` (made at `0x15cccf0`): it moves Windows' cursor to a position in the window. What calls that slot, and when, was not followed.
  - `ClipCursor` is called only by SDL (its window procedure `0x173ac20`, `0x1765c66`, `0x1766770`, `0x1766c00`), which confines the cursor only for a window that has the input focus (_inferred_). The game never calls `SDL_SetWindowGrab` or `SDL_CaptureMouse`; it calls `SDL_SetRelativeMouseMode` from slot `+0x30` (`0x162e490`) and `SDL_ShowCursor` from slot `+0x28` (`0x162e480`) and three other places.
- **The display.** `ChangeDisplaySettingsExW` is called only by SDL (`0x1768370`), for a full screen window's display mode. The game calls `SDL_SetWindowFullscreen` once, to leave full screen as it quits (`0x156a21b`), and `SDL_MinimizeWindow` once, as it destroys the window (`0x1654696`).
- **Messages.** The game's own message boxes (an error, the instance check) are left out with `-suppresserrormessages`. The framework's: a failed startup and a fatal error wait on a message box brought to the foreground (`notice::tell_and_wait`, `MB_SETFOREGROUND`), and notices fall back to message boxes when the framework UI is unavailable.
- **The crash reporter.** The game's crash handling (`pdx_crashreport.cpp`) ends in `0x1654600`: with the handler's `+0x2a6` set it writes the crash's files (`0x164f450`); with `+0x2a7` set, and a setting read through `0x23268b0` on, it minimizes and destroys the window (`0x1654696`, `0x165469e`) and calls `0x164f950` (`0x16546ec`). That builds `crash_reporter/binaries/CrashReporter.exe "--crashdir=…" "--uploadurl=…"`, starts it with `CreateProcessW` (`0x165034f`) and waits for it (`WaitForSingleObject`, `0x1650362`); the reporter is a program of its own with a window, which the player sees. Leaving that one call out keeps the crash's files and starts nothing; where `+0x2a7` and the setting come from was not followed.
- **Sound.** The game imports no sound library: it plays through SDL's audio, initialized at `0x16a67f6` (`SDL_InitSubSystem(SDL_INIT_AUDIO)`) and opened at `0x16a692d` (`SDL_OpenAudioDevice`). SDL 2.0.4 picks its driver from the environment variable `SDL_AUDIODRIVER`, which it reads with `GetEnvironmentVariableA` at the time (`SDL_getenv`, `0x1735e10`), and has the drivers XAudio2, DirectSound, winmm, `disk` and `dummy`; the `dummy` driver takes the mixed sound at its pace and plays none (_inferred_ from SDL's source). The volumes are the settings' `master_volume`, `music_volume`, `sound_fx_volume` and `ambient_volume`, which the game writes back to `settings.txt` when it applies its settings.

## The graphics device

- `0x16d8ed0` makes it with `Direct3DCreate9Ex` and `CreateDeviceEx` on `D3DDEVTYPE_HAL` with `D3DCREATE_HARDWARE_VERTEXPROCESSING | D3DCREATE_MULTITHREADED` (0x44), and falls back to `Direct3DCreate9` and `CreateDevice` with the same type and flags. There is no reference or software device in the game, and no option for one.
- `-opengl` switches the back end's function table to OpenGL ([architecture.md](architecture.md), "Presentation"), which goes through `OPENGL32.dll`; the window is then made with `SDL_WINDOW_OPENGL`.

## One game per machine

`main` calls `0x14c4470` first, which looks for a top-level window of the class `Europa Universalis IV` (`FindWindowExA(NULL, NULL, class, NULL)`, `0x14c4496`). When there is one, it logs "An instance of this game is already running on this computer! Exiting.", shows a message box unless `-suppresserrormessages`, and calls `FatalExit`. The game registers that class but makes its window with SDL's, `SDL_app`, so the check finds nothing and a second game starts (tried; [several-games.md](several-games.md)).

**Every game pins its main thread to the same processor.** The application's run `0x14c2d00`, before its first frame, takes the process's affinity mask (`GetProcessAffinityMask(GetCurrentProcess())`, `0x14c2d34`), counts the processors in it (`0x14c2d50`–`0x14c2d61`) and, when there is more than one, walks to the one numbered `count / 2 + 1` among them, counted from the lowest (`0x14c2d69`–`0x14c2d97`), and pins the main thread there: `SetThreadAffinityMask(GetCurrentThread(), 1 << n)` at `0x14c2db4`, whose result nothing reads. On a computer with 16 logical processors that is processor 8 (`0x100`), in every game started there. It is the game's only call of `SetThreadAffinityMask` (the import slot `0x1b66850` has no other reference), and the other threads keep the process's mask. Nothing in the run says why; pinning the thread that reads the clock was once advised where processors' timers differed, which Windows' `QueryPerformanceCounter` has not needed on current hardware (_inferred_). One game loses nothing by it. Several games side by side share the one processor for their main threads, each of which needs a processor of its own: measured on 2026-11-10, four hidden games of `whole` at speed 5 each had their main thread pinned to `0x100` (read from outside with `NtQueryInformationThread`) and running 25 to 30% of the time, while the machine's processors were 62% busy ([Hidden games' main threads unpinned](../prototype-log/2026-11-10-hidden-main-thread-unpinned.md)). The framework leaves the pin out, in every game (below).

## The game's own automation switches

`main` reads these from the command line (`0x876970`–`0x876b6d`); none hides the window or stops drawing:

| Switch | What it does |
| --- | --- |
| `-auto_run=<file>` | At the first in-game frame, runs the console command `run_commands <file>` and logs "Run console commands from startup option auto_run" (`0x81dfb3`–`0x81e0c0`) |
| `-auto` | The same with no file, and the start tag `SWE` |
| `-start_tag=<tag>` | The single player setup selects that country, logging "start_tag set, changing selected country to: %s" (`0x110e844`); whether the setup then starts by itself was not followed |
| `-autotest`, `-autotestsettings=<…>` | Sets a flag in `main`'s frame and reads a value; what reads them was not followed |
| `-seed=<n>` | The random generator's seed |
| `-gamestatetimer`, `-nudge`, `-redqueen` | Flags at `0x233fe9e`, `0x233fe9d`, `0x242bbc8`; not followed |
| `-suppresserrormessages`, `-suppresserrorlog`, `-nodlc`, `-checkdlc`, `-touch_test`, `-mount_tablet` | As named; `-touch_test` sets `[0x233ff4f]`, which the input device's cursor update looks at ([input.md](input.md)) |

## Tried in the game

2026-11-04: `eu4.exe` started directly with the framework (a player build of master at 5d63c0a), the player's playset, a borderless 3840x2160 window, vsync on, at 144 Hz. The window was hidden and shown with `ShowWindow` from outside the game. A remote control request is a whole run of the command line, about 25 ms of it starting the process.

| | A request answered (median) | CPU while paused | Graphics card (`nvidia-smi pmon`, SM %) | Screenshot | Days a second at speed 5 |
| --- | --- | --- | --- | --- | --- |
| Hidden, main menu | 28 ms | 54% of one core | 13 | 103 ms | |
| Hidden, in a game | 30 to 37 ms | 83 to 128% | 11 to 38 | 109 to 158 ms | 16.0, 16.5 |
| Shown, behind another program | 28 ms | 107% | 29 | 104 ms | 17.4, 14.9 |
| Minimized | 134 ms (86 to 191) | 2% | none listed | 581 ms | 4.1 |

- **Startup** with the window hidden from 3.1 s: 36.1 s to the main menu, 34.1 s in a run where it was shown again; the 30.5 to 31.2 s in [performance.md](../performance.md) were counted from the framework loading. Hiding it does not shorten the startup load, which reads files.
- **The speed** was taken from the same game, 180 days a measurement, hidden and shown in turn twice. Two earlier measurements of 90 days gave 6.8 days a second hidden (with a question answered on the way) and 12.7 hidden against 17.5 shown; the alternating ones do not bear out a difference.
- **Input hidden**: `input key space` resumed and paused the game; a click reached the game (`input.get` answered its position).

## What the hidden start relies on

Read from `eu4.exe` on 2026-11-04 with `cargo xtask disasm`, for the hidden start ([hidden-start.md](../plans/hidden-start.md)), and tried in the game the same day ([The hidden start](../prototype-log/2026-11-04-hidden-start.md)).

- **The window's flags.** Both paths of `0x14c3dd0` (full screen, and borderless or windowed) leave the flags at `[rsp+0x28]` and meet at `0x14c40af` (`mov edx, 0x2fff0000`), the last instruction before the arguments of `SDL_CreateWindow` are moved into place. Leaving `SDL_WINDOW_SHOWN` out is not enough: SDL 2 shows every window made without `SDL_WINDOW_HIDDEN` (8) as it finishes making it. Tried: without `SHOWN` alone, the window was shown 0.8 s after the framework started and took the focus; with `HIDDEN`, it stayed hidden.
- **Power throttling** (2026-11-10): `SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, {version 1, control mask execution speed | ignore timer resolution, state mask 0})` as the framework starts; a process may set it on itself, and Windows keeps it for the process's life.
- **The main thread's pin** (2026-11-10): `0x14c2db4` is `call [SetThreadAffinityMask]` (the import slot `0x1b66850`), with `rcx` the current thread and `rdx` the mask; the next instruction (`0x14c2dba`, `mov rax, [rsi]`) overwrites the result. The framework writes a six-byte `nop` there, in every game, hidden or not, so the main thread may run on any of the process's processors ("One game per machine").
- **The window's class** is SDL's, `SDL_app`, and its title `Europa Universalis IV`; besides it, the process has the hidden windows `Default IME`, `MSCTFIME UI` and, early in the start, `PdxDetectWindow`.
- **Init's show**: `0x14c02ca` is `call [ShowWindow]` (the import slot `0x1b66c38`) with `ecx` the window from `[app+0xe8]` and `edx` `SW_SHOWDEFAULT` (10); nothing after it reads its result.
- **`-suppresserrormessages`**: `main` stores whether it is on the command line at `0x876b37` (`mov [0x2357b33], al`), whether or not it is, so a flag set before `main` would be overwritten. The flag is read by the instance check (`0x14c462d`), at `0x12e78a1`, `0x12e7ea0`, `0x16dd0b0` and `0x16dd0c2`.
- **The input device's cursor move** (`0x162e4d0`, slot `+0x40`): its second argument points to the position, `x` and `y` as two ints, which it passes with SDL's window (from `[[0x23495a8]]`) to `SDL_WarpMouseInWindow`. What calls the slot, and when, was not followed.
- **After a crash**, `0x1654600` calls `SDL_MinimizeWindow` (through `0x1735340`) at `0x1654696` and `SDL_DestroyWindow` after it, then `0x164f950` at `0x16546ec`, which starts `CrashReporter.exe` and waits for it; nothing after it reads its result. Minimizing a hidden window would show it, minimized, on the taskbar (_inferred_ from `SW_MINIMIZE`). Tried: with both left out, a crash wrote `exception.txt`, `meta.yml`, `minidump.dmp` and `settings.txt` into the crash folder, showed nothing, and the process ended about a second later.
- **Sound**: `eu4.exe` holds SDL's `dummy` audio driver ("SDL dummy audio driver") beside XAudio2 and DirectSound, and no `disk` driver. Tried: with `SDL_AUDIODRIVER=dummy` set by the framework, the process loaded none of `dsound.dll`, `XAudio2_*.dll`, `AudioSes.dll` or `MMDevAPI.dll`.
- **Full screen in the settings.** Tried with `fullScreen=yes` and `borderless=no` in `settings.txt`: the window was made without `SDL_WINDOW_FULLSCREEN`, the game's log has "Loading settings for adapter -1, fullscreen=1" and then "Creating device with … Fullscreen=no, Resolution=3840x2160", and the frame was 3840 by 2160. Why the device was made windowed was not followed.
## What it would take

1. **Unseen: feasible now, at no cost in speed.** Two dependency points would let the framework do it at startup: the flags passed to `SDL_CreateWindow` (`0x14c40c1`), and Init's `ShowWindow` (`0x14c02ca`); the hidden start has done it since, with the points above. It also has to:
   - run windowed or borderless, whatever `settings.txt` says, since a full screen device would be lost;
   - not minimize, which throttles the game to about ten frames a second;
   - hold the cursor where it moves no camera (remote control's input already answers `0x14e5e50` itself);
   - give a way back to the window, such as a capability that shows it again, since the player has nothing to click.

   [hidden-start.md](../plans/hidden-start.md) has it.
2. **Not drawn: feasible, size of the gain unknown.** Skipping the idler's drawing (`0x8183b0` in a game, the main menu's frame draws the same way) and drawing only the frames a screenshot asks for would idle the graphics card; whether it makes days faster depends on how much of the main thread's frame the drawing takes at speed 5, which was not measured (the day's update runs whole frames inside it, [architecture.md](architecture.md), "Frames inside a day"). What the views update while drawing, and what a skipped frame would leave stale, is not known.
3. **No graphics card: not feasible as the game is.** The device must be a hardware one. Untried routes: Windows' Basic Render Driver, which may or may not take a Direct3D 9 hardware device; `-opengl` with a software OpenGL in the game folder. Steam must also be running and signed in, and the game is licensed per player, so a build machine could not hold it anyway.
4. **Several at once: feasible.** The instance check finds nothing and Steam runs the game twice; it takes each game started from the game folder with a user folder of its own (`-userdir`), a core and a pipe of its own, and a claim per game ([several-games.md](several-games.md)).

## Open questions

- What `-autotest`, `-autotestsettings`, `-gamestatetimer`, `-redqueen` and `app+0x32b` do.
- Whether a full screen game can be hidden at all, and what its device does then.
- How much of a frame at speed 5 is drawing.
