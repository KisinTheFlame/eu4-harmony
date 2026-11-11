# 2026-11-11 The tooltip at the cursor and the minimap deferred, in a game shown (tested by the owner)

The owner's look at the drawing thread after the tooltip was kept at the cursor and the minimap's drawing deferred ([2026-11-11-tooltip-and-minimap.md](2026-11-11-tooltip-and-minimap.md)). A test game from this branch's core with `dev-test-games`, shown, the 1780.7.1 save in observe mode, the camera on Constantinople, paused when handed over; the owner ran it at speed 5 and dragged the map for about twenty seconds. The player's playset and settings, 3840×2160 borderless, a display of 144 Hz.

- **The owner**: very good.
- **The drawing thread's log** at speed 5, in ten seconds: 126 to 129 frames a second, 38 to 42 late, 2 held back, both by the interface's monthly refresh (`0x81e280`), 8 to 18 waiting for the lock, 22 to 28 neither, the longest gap 159 to 195 ms. Before ([2026-11-11-mesh-objects-shown.md](2026-11-11-mesh-objects-shown.md)): 112 to 126 frames a second, 1 to 12 held back, most by the minimap.
- **Not verified**: a longer run shown; tooltips over windows and buttons; the tooltip at the screen's edges.
- **Cleanup**: the test game released; it quit with exit code 0, no crash; its logs kept in its claim's `kept\`.
