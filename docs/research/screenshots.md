# Reading the frame back (for screenshots)

Baseline 1.37.5. All addresses are RVAs. What was checked in the running game is marked "tested"; the run is in [prototype-log.md](../prototype-log.md) (2026-10-29, remote control C10). The machine: a 3840x2160 display at 144 Hz, the game in a borderless window of that size with `multi_sampling=8` and `vsync=yes` in its `settings.txt`, an NVIDIA GeForce RTX 4070 Ti SUPER beside AMD integrated graphics (which of the two the game drew with was not checked).

## The frame the player sees

- **Every frame the main thread draws** ends in the Present wrapper `0x16d4db0`, which calls `IDirect3DDevice9::Present(NULL, NULL, NULL, NULL)` (vtable offset `0x88`) on the renderer's device (the dependency points "frame present" and "renderer device", `framework_ui::hook`). So the back buffer 0 of swap chain 0, as it is just before that call, is what the player sees, and the framework UI is drawn into it just before. The screenshot reads it there, after the framework UI. No new place in the game is relied on, so the screenshot adds no dependency point. Tested: a screenshot in the main menu had the framework's corner button pixel for pixel as a capture of the screen had it.
- **The back buffer** is 3840x2160, `D3DFMT_A8R8G8B8` (21), multisampled with `D3DMULTISAMPLE_8_SAMPLES` (8), as `settings.txt` asks. Tested, in the main menu and in a game. A multisampled surface cannot be read back as it is; `StretchRect` into a render target without multisampling resolves it. The alpha byte is not shown, so a screenshot drops it.
- **While a save loads** another thread draws and presents the loading screen ([safe-points.md](safe-points.md)), so the screenshot, which only the main thread's presents take, waits for the first frame after the load. Not tested.

## What reading it back costs the main thread

The device is created multithreaded ([`framework_ui::hook`](../../core/src/framework_ui/hook.rs)); every call below was made on the main thread, in the present hook. Tested, each with several screenshots of the 3840x2160 frame:

- **`GetRenderTargetData` is queued, not done, by this driver**: it returned in 0.00 to 0.02 ms every time, and the wait came where the surface in system memory was next touched.
- **Locking the surface right after the transfer was queued** waited 12 to 68 ms, both at the present after the copy and with `D3DLOCK_DONOTWAIT`, which never answered `D3DERR_WASSTILLDRAWING`: the transfer waited behind the frame the GPU was still drawing.
- **An event query issued after the transfer, and flushed (`D3DGETDATA_FLUSH`) at once**, took 8 to 31 ms to answer "not yet". Looked at without the flag, it answered in about 0.3 ms, and the game's own Present, at the end of that frame, which hands the GPU the work queued, took 9.9 to 11.1 ms, where it mostly took 5 to 6 ms at other frames (it waits for the display), now and then up to 11.6 ms. At the next present the query had passed, and the lock took 0.002 ms. That frame's present came 11.7 to 13.7 ms after the one before, where presents otherwise came 4 to 8 ms apart: about one refresh of the 144 Hz display missed, as far as these numbers tell.
- **The copy on the GPU** (`CreateRenderTarget`, `StretchRect`, an event query) took 0.03 to 1.7 ms, twice 5.1 and 9.8 ms; its query had passed by the first or second present after, every time.
- **A surface in system memory** of that size took 0.03 to 6.7 ms to make, and 2.4 ms to let go, so the framework keeps it for the next screenshot.
- **Copying the pixels out** of the locked surface, 33 MB, took 4.8 to 16.2 ms, on the screenshot's own thread.

So the framework copies at the frame's present, reads back once the copy's query has passed, and locks once the transfer's query has passed, never flushing (`game::screenshot::d3d9`): with the final build its own part on the main thread came to 0.10 to 0.51 ms a screenshot, twice 5.2 and 9.9 ms (the copy step), over 12 screenshots.

## Encoding

Windows' own imaging (WIC) scales and encodes on the screenshot's thread. Tested: a 3840x2160 frame as a PNG of 8 to 12 MB took 360 to 560 ms; at 50%, 1920x1080, 2.9 to 3.1 MB, 107 to 160 ms; at 25%, 960x540, 0.8 MB, 35 to 56 ms. From starting the command line to its printed answer: 0.6 to 0.9 s at 100%, 0.2 to 0.35 s at 50%, 0.1 to 0.2 s at 25%.
