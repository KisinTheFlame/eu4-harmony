# Running the game headless

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
- **A present that is not shown stops drawing and in-game input.** The Present wrapper `0x16d4db0` takes `S_PRESENT_OCCLUDED` (`0x08760878`, tested at `0x16d4ddb`) as a failure, as it does a negative result, writing `0` to `renderer+0x132` (`0x16d4df1`) and returning `0`. The graphics object's present `0x14ddc40` keeps the answer in `graphics+0x530`, which the device check `0x14df050` sets again while the device is fine. When `graphics+0x530` is clear:
  - the next application frame waits `SDL_Delay(100)` instead of `SDL_Delay(0)`, unless `app+0x32b` (`0x14c26b3`–`0x14c26e7`);
  - the in-game drawing `0x8183b0` clears `idler+0x1607` (`0x8186d1`–`0x818720`), reads `graphics+0x530` before `0x14df050` restores it (`0x8187b6`), skips drawing the 3D map and GUI (`0x8187e7`), and still calls `0x14ddc40` (`0x81aa86`), so if every present is occluded (for example, while the display is asleep or the desktop is locked) `graphics+0x530` stays clear and zero frames draw;
  - while `idler+0x1607` is clear, the in-game frame skips all in-game input (`0x81501b` skips calling `0x830fd0`), so keyboard shortcuts and mouse clicks do nothing.

  An unattended run (`-harmony_hidden` or `-harmony_test_game`) therefore treats non-negative `Present` results including `S_PRESENT_OCCLUDED` as presented (`core/src/render/present.rs`), while preserving negative Direct3D errors (`D3DERR_DEVICELOST`, etc.), so unattended runs keep rendering screenshots and handling remote input when the display sleeps.
- **A hidden window is not occluded while the display is active.** Tried: hidden but not minimized on an active desktop, requests were answered as fast as with the window shown, the graphics card was as busy, and the speed was the same (below). Direct3D 9Ex reports `S_PRESENT_OCCLUDED` for a minimized window or when the display is off/locked.
- **Windows throttles a process no one sees.** Windows 11 power-throttles (EcoQoS) a process whose windows the player cannot see: its threads are scheduled seldom and on slow cores, and its timer resolution requests are ignored. Measured on 2026-11-10 with two hidden games side by side at speed 5 on the 1780.7.1 save ([A hidden game at full speed](../../research/performance-evidence.md)): the game as it was used 11 to 14% of a core and passed about 23 days a minute; the one opted out of it (`SetProcessInformation` with `ProcessPowerThrottling`, execution speed and timer resolution controlled and neither throttled) used 168 to 188% and passed about 520. The game that ran as fast hidden as shown on 2026-11-04 (below) was alone; whether Windows throttled it then was not looked at.
- **A lost device** (`0x14df050`: `TestCooperativeLevel`) skips the frame; the first frame lost sleeps 1 s. A device in a window is not lost when it is hidden or minimized (_inferred_); a full screen one is when it loses the foreground. Full screen was not tried.
- **The cursor is read against the active window.** While `[0x1fd10c5]` is set, the game takes the cursor's position as `GetCursorPos` less `GetWindowRect(GetActiveWindow())` ([input.md](../interface/input.md)). With the game hidden or behind another program the active window is another program's, so the position the game reads lies wherever the cursor is against that window, often past an edge of the frame, and the map scrolls toward it. Tried: the camera ran to the map's northern edge while the game was hidden and behind; that this was the cause is _inferred_. Remote control's input holds the cursor once it gives one (input.md), after which the camera stayed.
- **Remote control and screenshots** go on unchanged: the framework serves requests from the in-game frame and copies the frame at its present, both of which keep running hidden or minimized. Tried, both.
- **Sound** plays on while hidden (_inferred_: nothing in the frame looks at the window); not listened for.
- **The graphics driver keeps memory while the window is not shown.** With NVIDIA's Direct3D 9 driver (`nvd3dumx.dll`, of 2026-10-21), blocks of 3,072 bytes of the process's default heap, from a pool the driver tags `NVDA` and `NVDX`, are made and never let go at most frames while the window is hidden (`SW_HIDE`, or made with `SDL_WINDOW_HIDDEN` and not shown): 13 to 125 a second at the idle main menu, from run to run, 20 to 22 MB a minute at the most. Shown, or shown and cloaked (`DWMWA_CLOAK`), none are. Tried in one process, hidden, shown and hidden again, and then with the window cloaked ([What grows outside the census, and the hidden window concealed](../../research/interface-evidence.md)). Why the driver keeps them was not looked into, nor other drivers.

## What could reach the player's desktop

What a game that runs unseen could still do to the program the player works in: take the focus, move or confine Windows' cursor, change the display, show a message, or play sound. Read from the code, not tried.

- **The focus.** Only showing the window takes it: `SDL_CreateWindow` with `SDL_WINDOW_SHOWN` and Init's `ShowWindow(SW_SHOWDEFAULT)` (above). The game imports no `SetForegroundWindow`, `SetFocus`, `SetActiveWindow` or `FlashWindow`; SDL's `SDL_RaiseWindow` is never called.
- **Keys.** The player's typing goes to the window that has the focus; the game reads no key state of its own ([input.md](../interface/input.md)), so nothing the player types elsewhere reaches it.
- **Windows' cursor.**
  - `SetCursorPos` is called only by SDL (`0x1767970`, `0x17679c0`), for `SDL_WarpMouseInWindow` (`0x1735c90`), which SDL does whether or not the window has the focus (_inferred_ from SDL's source). The game calls it from one place, `0x162e4d0`, slot `+0x40` of the input device's vtable `0x1da3530` (made at `0x15cccf0`): it moves Windows' cursor to a position in the window. What calls that slot, and when, was not followed.
  - `ClipCursor` is called only by SDL (its window procedure `0x173ac20`, `0x1765c66`, `0x1766770`, `0x1766c00`), which confines the cursor only for a window that has the input focus (_inferred_). The game never calls `SDL_SetWindowGrab` or `SDL_CaptureMouse`; it calls `SDL_SetRelativeMouseMode` from slot `+0x30` (`0x162e490`) and `SDL_ShowCursor` from slot `+0x28` (`0x162e480`) and three other places.
- **The display.** `ChangeDisplaySettingsExW` is called only by SDL (`0x1768370`), for a full screen window's display mode. The game calls `SDL_SetWindowFullscreen` once, to leave full screen as it quits (`0x156a21b`), and `SDL_MinimizeWindow` once, as it destroys the window (`0x1654696`).
- **Messages.** The game's own message boxes (an error, the instance check) are left out with `-suppresserrormessages`. The framework's: a failed startup and a fatal error wait on a message box brought to the foreground (`notice::tell_and_wait`, `MB_SETFOREGROUND`), and notices fall back to message boxes when the framework UI is unavailable.
- **The crash reporter.** The game's crash handling (`pdx_crashreport.cpp`) ends in `0x1654600`: with the handler's `+0x2a6` set it writes the crash's files (`0x164f450`); with `+0x2a7` set, and a setting read through `0x23268b0` on, it minimizes and destroys the window (`0x1654696`, `0x165469e`) and calls `0x164f950` (`0x16546ec`). That builds `crash_reporter/binaries/CrashReporter.exe "--crashdir=…" "--uploadurl=…"`, starts it with `CreateProcessW` (`0x165034f`) and waits for it (`WaitForSingleObject`, `0x1650362`); the reporter is a program of its own with a window, which the player sees. Leaving that one call out keeps the crash's files and starts nothing; where `+0x2a7` and the setting come from was not followed.
- **Sound.** The game imports no sound library: it plays through SDL's audio, initialized at `0x16a67f6` (`SDL_InitSubSystem(SDL_INIT_AUDIO)`) and opened at `0x16a692d` (`SDL_OpenAudioDevice`). SDL 2.0.4 picks its driver from the environment variable `SDL_AUDIODRIVER`, which it reads with `GetEnvironmentVariableA` at the time (`SDL_getenv`, `0x1735e10`), and has the drivers XAudio2, DirectSound, winmm, `disk` and `dummy`; the `dummy` driver takes the mixed sound at its pace and plays none (_inferred_ from SDL's source). The framework selects `dummy` independently of window visibility when `-harmony_mute` is passed. The volumes are the settings' `master_volume`, `music_volume`, `sound_fx_volume` and `ambient_volume`, which the game writes back to `settings.txt` when it applies its settings.

## The graphics device

- `0x16d8ed0` makes it with `Direct3DCreate9Ex` and `CreateDeviceEx` on `D3DDEVTYPE_HAL` with `D3DCREATE_HARDWARE_VERTEXPROCESSING | D3DCREATE_MULTITHREADED` (0x44), and falls back to `Direct3DCreate9` and `CreateDevice` with the same type and flags. There is no reference or software device in the game, and no option for one.
- `-opengl` switches the back end's function table to OpenGL ([architecture.md](architecture.md), "Presentation"), which goes through `OPENGL32.dll`; the window is then made with `SDL_WINDOW_OPENGL`.

## One game per machine

`main` calls `0x14c4470` first, which looks for a top-level window of the class `Europa Universalis IV` (`FindWindowExA(NULL, NULL, class, NULL)`, `0x14c4496`). When there is one, it logs "An instance of this game is already running on this computer! Exiting.", shows a message box unless `-suppresserrormessages`, and calls `FatalExit`. The game registers that class but makes its window with SDL's, `SDL_app`, so the check finds nothing and a second game starts (tried; [several-games.md](several-games.md)).

**Every game pins its main thread to the same processor.** The application's run `0x14c2d00`, before its first frame, takes the process's affinity mask (`GetProcessAffinityMask(GetCurrentProcess())`, `0x14c2d34`), counts the processors in it (`0x14c2d50`–`0x14c2d61`) and, when there is more than one, walks to the one numbered `count / 2 + 1` among them, counted from the lowest (`0x14c2d69`–`0x14c2d97`), and pins the main thread there: `SetThreadAffinityMask(GetCurrentThread(), 1 << n)` at `0x14c2db4`, whose result nothing reads. On a computer with 16 logical processors that is processor 8 (`0x100`), in every game started there. It is the game's only call of `SetThreadAffinityMask` (the import slot `0x1b66850` has no other reference), and the other threads keep the process's mask. Nothing in the run says why; pinning the thread that reads the clock was once advised where processors' timers differed, which Windows' `QueryPerformanceCounter` has not needed on current hardware (_inferred_). One game loses nothing by it. Several games side by side share the one processor for their main threads, each of which needs a processor of its own: measured on 2026-11-10, four hidden games of `whole` at speed 5 each had their main thread pinned to `0x100` (read from outside with `NtQueryInformationThread`) and running 25 to 30% of the time, while the machine's processors were 62% busy ([Hidden games' main threads unpinned](../../research/mechanics-evidence.md)). The framework leaves the pin out, in every game (below).

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
| `-suppresserrormessages`, `-suppresserrorlog`, `-nodlc`, `-checkdlc`, `-touch_test`, `-mount_tablet` | As named; `-touch_test` sets `[0x233ff4f]`, which the input device's cursor update looks at ([input.md](../interface/input.md)) |


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x876b37` | code | suppress error messages |
| `0x12e78a1` | code | Referenced while examining what the hidden start relies on; complete role not established |
| `0x12e7ea0` | code | Referenced while examining what the hidden start relies on; complete role not established |
| `0x14c2dba` | code | Referenced while examining what the hidden start relies on; complete role not established |
| `0x14c40af` | code | window flags |
| `0x14c462d` | code | Referenced while examining what the hidden start relies on; complete role not established |
| `0x16dd0b0` | code | Referenced while examining what the hidden start relies on; complete role not established |
| `0x16dd0c2` | code | Referenced while examining what the hidden start relies on; complete role not established |
| `0x1735340` | leaf | SDL minimize window |
| `0x1b66c38` | data | Referenced while examining what the hidden start relies on; complete role not established |
| `0x23495a8` | data | Referenced while examining what the hidden start relies on; complete role not established |
| `0x2357b33` | data | Referenced while examining what the hidden start relies on; complete role not established |


## Limits of the reading

Automation switches `-autotest`, `-autotestsettings`, `-gamestatetimer`, `-redqueen` and application byte `+0x32b` were not fully explained. Fullscreen hiding/device behavior and screenshot waiting during loading remain outside the established hidden-window path.
