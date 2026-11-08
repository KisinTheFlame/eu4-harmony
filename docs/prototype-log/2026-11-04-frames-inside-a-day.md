# 2026-11-04 Frames inside a day, looked at by the owner (tested by me)

S4 of [game-speed.md](../game-speed.md), before anything was built: the interval of frames inside a day (`[0x245b070]` times 0.05 s at speed 5) written into the running game by a script of the run's own (`WriteProcessMemory`, not kept), while the owner watched. The player build another session had installed in the game folder (built 2026-11-04 02:30), not replaced; the player's playset, 3840x2160, vsync at 144 Hz; `eu4.exe` started directly; the game claimed with the game-lock skill, which backed up the player's files. Austria-1 (an ironman save of 1618.9.2) at speed 5, questions answered with their first option and offers declined by a script; frames counted at the application's `+0x320`, days at the date, 32 s each, no sampler.

| Interval | Years | Frames a second | Days a second |
| --- | --- | --- | --- |
| 1.0 (the game's, 50 ms) | 1618 to 1620 | 18.2 | 12.5 |
| 2.0 | 1620 to 1621 | 15.0 | 12.6 |
| 4.0 | 1621 to 1623 | 15.3 | 14.6 |
| none inside a day (1000) | 1623 to 1624 | 13.6 | 13.4 |
| 1.0 | 1624 to 1626 | 19.0 | 13.1 |
| none inside a day | 1626 to 1627 | 14.4 | 14.3 |

- Days grow slower as the game goes on, and an autosave falls in some runs, so only the last two, side by side, compare: 9% more days a second, 5 frames a second fewer. Paused, 63 frames a second.
- **The owner's decision**: two choices, the game's pace and no frames inside a day, with a tooltip; recorded in game-speed.md.
- **Not verified**: a game after 1700, where days are slower and the gain larger; input lag measured rather than seen.
- **Cleanup**: the interval was put back to 1.0 and the game closed by ending its process. Releasing the game restored the player's files and found every file as the backup had it.
