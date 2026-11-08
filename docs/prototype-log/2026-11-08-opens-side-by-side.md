# 2026-11-08 Files opened and looked up side by side (tested by me)

[L4](../loading.md) on a test game: a developer build of this branch with the test games' pipes, the test switches and `dev-call-trace`, the player's playset, every DLC, the music kept (a first start after the claim to keep it); other sessions' test games held the machine's processor at about 70%. Each start timed the databases (`0x208620`), the history (`0x7ea2c0`), the data checksum (`0x171cdf0`) and the events (`0x6f3250`) with `times`, and alternated the part on with `game-paths = ["opens"]`, which the startup report confirmed as "speed-ups left to the game: opens".

| Start | The main menu | Databases | History | Checksum | Events |
| --- | --- | --- | --- | --- | --- |
| On, 1 | 36 s | 10.16 s | 0.87 s | 4.76 s | 3.31 s |
| Left to the game, 1 | 44 s | 18.00 s | 2.11 s | 16.52 s | 3.48 s |
| On, 2 | 37 s | 12.13 s | 1.02 s | 5.70 s | 3.33 s |
| Left to the game, 2 | 43 s | 17.34 s | 1.96 s | 15.20 s | 3.44 s |
| On, 3 | 29 s | 8.26 s | 0.69 s | 4.13 s | 2.89 s |
| Left to the game, 3 | 34 s | 13.93 s | 1.67 s | 11.66 s | 2.98 s |

- **The same data**: Austria-37_4-1 (an ironman save of 1724) copied into the test game and loaded with `menu.load`, once each way; the game writes an ironman save as it loads it, and the two it wrote were compared entry by entry: `gamestate` (55,051,381 bytes) differed in the 4 bytes at 510 that every save changes, `meta` (2,235 bytes) only in its UUID, `ai` (65,711 bytes) not at all; as the same comparison found for L3 ([The startup and a load sampled](2026-11-05-startup-load-sampled.md)).
- **Played**: with the part on, the save loaded and run at speed 5 (`time.advance --days 60`, which stopped at 1724.5.28), `menu.return`, a new game in observe mode (`menu.start`) and 30 days run, to 1444.12.11.
- The offline checks matched every point of the part against `eu4.exe`.
- Ten starts and no crash; the release said each game ended by `game stop` or the release.
- **Not verified**: the machine otherwise idle; what is left of the LoadingThread's waits; a playset with mods in the user folder, whose opens go through the same code; a start with nothing in the system's file cache.
- **Cleanup**: `dev.toml` removed; the release kept the logs; the call times and saves deleted from the scratchpad.
