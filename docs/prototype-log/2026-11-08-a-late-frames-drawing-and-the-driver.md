# 2026-11-08 A late frame's drawing, the game's code against the graphics driver (tested by me)

The release build with the test games' pipes, no development tool, on a test game: the 1780.7.1 save copied into its `save games`, `menu.load`, `console run observe`, `session.view TUR`, `time.set_speed 5`, the framework's "Faster time" from the player's settings; then `cargo xtask profile --seconds 180 --threads --waiting-in 0x179e90`, 1780.7.2 to 1788.10.7.

- 2,604 days, 14.5 a second, 69 ms a day; the day 78% of the main thread, between days 22%.
- The drawing 7.9%, 5.5 ms a day: the main thread waiting in the kernel under the driver 2.0 ms, the game's code 1.9, `d3d9.dll` 0.8, the driver 0.5; the frame's end (`0x14ddc40`) was nearly all a wait. One thread of the driver's own ran on 6.6% of the ticks. Weighed in [research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "The Direct3D calls on a thread of their own".
- Between days besides the drawing: the commands 4.9% (3.4 ms a day), the map's markers 1.8%, the GUI's update 1.2%, the top bar's alerts 1.1%, the autosave 1.1%.
- **Not verified**: whether the wait is for the driver's queue or for the graphics card; a game on the desktop, with the player's vertical sync; other saves.
- **Cleanup**: the release removed the test game's folder but `kept\`, and said the game did not crash; the profile's files in the scratchpad.
