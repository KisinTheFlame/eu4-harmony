# 2026-11-10 Speed 5 in a game shown (tested by the owner)

The owner's look at the drawing thread after the map icons followed the camera ([2026-11-10-map-icons-follow-the-camera.md](2026-11-10-map-icons-follow-the-camera.md)) and the frames late at speed 5 were cut ([2026-11-10-frames-late-at-speed-5.md](2026-11-10-frames-late-at-speed-5.md)). A test game from this branch's core with `dev-test-games`, shown, the 1780.7.1 save in observe mode, the camera on Constantinople; the owner ran it at speed 5 and dragged the map for about two minutes. The player's playset and settings, 3840×2160 borderless, a display of 144 Hz.

- **The owner**: the map still stutters at speed 5, but much less than before; the units' flags and numbers follow the map as it is dragged.
- **The drawing thread's log** at speed 5, in ten seconds: 80 to 114 frames a second, 38 to 96 late, 19 to 54 held back, the longest gap 190 to 419 ms. Of the held back, the scene begun (`0x16d4e00`) was the most (18 to 39), then mesh objects let go (`0x153e560`, 0 to 17); waiting for the lock 0 to 10; neither 6 to 59. The hidden runs had the scene begun at 10 to 16 and neither at 4 to 25: the map dragged in a game shown has more of both.
- **Paused**, before the owner set speed 5: 141 to 144 frames a second, up to 8 late, the longest gap 45 ms.
- **Not verified**: which of the scenes begun are the map's own and which the interface's; what "neither" is.
- **Cleanup**: the test game released; it quit with exit code 0, no crash; its logs kept in its claim's `kept\`.
