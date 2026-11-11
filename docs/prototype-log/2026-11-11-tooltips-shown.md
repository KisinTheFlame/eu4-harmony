# 2026-11-11 The tooltips updated before the frame, in a game shown (tested by the owner)

The owner's look at the drawing thread after the tooltips were updated before the frame is prepared ([2026-11-10-tooltips-before-the-frame.md](2026-11-10-tooltips-before-the-frame.md)), as [2026-11-10-speed-5-shown.md](2026-11-10-speed-5-shown.md) looked before it. A test game from this branch's core with `dev-test-games`, shown, the 1780.7.1 save in observe mode, the camera on Constantinople; the owner ran it at speed 5 and dragged the map for about a minute. The player's playset and settings, 3840×2160 borderless, a display of 144 Hz.

- **The owner**: it feels very good.
- **The drawing thread's log** at speed 5, in ten seconds: 124 to 131 frames a second, 25 to 36 late, 12 to 21 held back, the longest gap 142 to 168 ms; before: 80 to 114 frames a second, 38 to 96 late, the longest gap 190 to 419 ms. No frame held back by a scene; those held back were by mesh objects let go (`0x153e560`, 2 to 16), the minimap (`0x10b6c50`, 2 to 10), a port's model (`0x13238b0`) and the monthly refresh (`0x81e280`); waiting for the lock 3 to 6; neither 7 to 16.
- **Paused**, before speed 5: 141 to 144 frames a second, up to 8 late, the longest gap 44 ms.
- **Not verified**: tooltips over windows and buttons, which the owner did not say of.
- **Cleanup**: the test game released; it quit with exit code 0, no crash; its logs kept in its claim's `kept\`.
