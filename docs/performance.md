# Performance

What the framework costs the game, measured on 2026-10-28 with the framework at commit 44b89f3. In short:

- **Running the game**: no difference to measure. At speed 5 the game advanced as many days per second with the framework as with the CJK font plugin it replaces, using as much CPU; two runs of the same setup differ more than the setups do.
- **Starting the game**: with bitmap fonts (the default) the main menu came about 2 s sooner than with the plugin or with neither; with system fonts about 1.3 s later than with the plugin, because every character is drawn at startup.
- **Memory**: with system fonts about the same as the plugin (+15 to 35 MB). With bitmap fonts about **420 MB more**, in the menu and in the game: the one cost worth reducing.

## Setups

| Setup | In the game folder |
| --- | --- |
| Framework, bitmap fonts | the framework; framework settings "Keep the fonts as they are" (the default) |
| Framework, system fonts | the framework; "System fonts", Microsoft YaHei for text, KaiTi for the map |
| CJK font plugin | the plugin's `version.dll` loader and `plugins\`, no framework |
| Neither | neither the framework nor the plugin |

The playset was the same throughout: a Chinese language mod, the regional combat mod (inert without the framework) and a few others. The game ran in English with Windows in Chinese, so the player language was Simplified Chinese.

"Neither" is not vanilla EU4: the playset stayed the same, so the Chinese language mod, made for the CJK font plugin, ran without it. It was measured up to the main menu only, because entering single player crashed the game both times it was tried, in the Epic Online Services SDK (`EOSSDK-Win64-Shipping.dll`, `EOS_Presence_RemoveNotifyJoinGameAccepted`), which the game calls as it reports what the player is doing. Possibly the mod's legacy escapes, which nothing turned into text in that setup, reached the SDK; this was not checked, and vanilla without the mod was not tried.

## Machine

AMD Ryzen 7 9700X (8 cores, 16 threads), 64 GB of memory, GeForce RTX 4070 Ti SUPER (16 GB), Windows 11 Pro, 3840×2160. EU4 1.37.5 from Steam, started directly as `eu4.exe`.

A training job that kept the processor about 80% busy and held 14 GB of the graphics card's memory was stopped before the runs below. One run made while it ran is left out: framework with system fonts, 51 s to the main menu and 11 game days per second, against 37 s and 17.4 days idle.

## Method

[`tools/perf-run.ps1`](../tools/perf-run.ps1) runs the game the same way each time:

1. Starts `eu4.exe` and times the launch until the main menu is on screen (a region of the menu's logo, compared every 0.25 s).
2. 20 s later reads the process's working set and private bytes.
3. Opens single player, picks the 1444 East Asia bookmark and Ming, starts in normal mode, and times the load until the start-of-game introduction appears.
4. Closes the introduction, waits 10 s, and measures CPU over 30 s paused, on the map as the game opens it.
5. Sets speed 5, runs the game for 120 s, pauses, and reads the CPU, memory and date.

CPU is given in cores: the process's CPU seconds per second of wall time (the machine has 16 hardware threads). Memory is what Windows reports for the process; graphics memory was not measured. Each setup was measured two or three times, going through the setups and then back again, with menu-only runs for idle CPU at the main menu. Game days are counted in the game's calendar, which has no leap years.

## Results

Means, with the single runs in brackets where there were several.

| | Neither | CJK font plugin | Framework, bitmap fonts | Framework, system fonts |
| --- | --- | --- | --- | --- |
| Launch to main menu (s) | 35.2 (34.9, 35.5) | 35.5 (36.0, 35.3, 35.3) | 33.5 (33.1, 33.1, 34.2) | 36.8 (37.4, 36.3, 36.8) |
| Main menu: working set / private (MB) | 2350 / 3638 | 2440 / 3769 | 2856 / 4194 | 2466 / 3787 |
| Main menu, idle: CPU (cores) | 0.53 (0.57, 0.49) | 0.53 | 0.56 | 0.56 |
| Loading a new game (s) | — | 3.7 (3.7, 3.7) | 3.5 (3.4, 3.5) | 3.7 (3.7, 3.7) |
| Paused in game: CPU (cores) | — | 1.02 (0.99, 1.05) | 0.99 (1.01, 0.97) | 0.93 (0.88, 0.99) |
| Paused in game: working set / private (MB) | — | 2653 / 4052 | 3073 / 4494 | 2672 / 4077 |
| Speed 5: game days per second | — | 17.7 (17.4, 18.0) | 17.3 (16.7, 18.0) | 17.4 (17.5, 17.4) |
| Speed 5: CPU (cores) | — | 2.91 (2.93, 2.90) | 2.95 (2.96, 2.93) | 2.98 (3.07, 2.90) |
| After 2 min at speed 5: working set / private (MB) | — | 2739 / 4169 | 3164 / 4596 | 2774 / 4185 |

### Running the game

Days per second and CPU differ between setups by less than between two runs of the same setup (16.7 and 18.0 days per second with bitmap fonts). Whatever the framework adds to the game's frames and days is below what these runs can show. They kept to the map, though: paused as the game opens and running at speed 5, without opening windows full of text.

### Starting the game

With bitmap fonts the main menu came about 2 s sooner than with the plugin or with neither, in every run. The cause was not looked into; one candidate is that the framework turns the localisation into game text itself (255,864 entries, none left to the game's own conversion).

System fonts take about 3.3 s longer than bitmap fonts. The framework's log accounts for it: every character of the text fonts and the map font is drawn at startup, about 3150 glyphs at 1.1 ms each (3.4 to 3.5 s, on the thread that loads the fonts). Bitmap fonts draw only the few characters the mod's fonts lack (7 glyphs, 15 ms).

### Memory

System fonts cost 15 to 35 MB more than the plugin. Bitmap fonts cost about 420 MB more, at the menu and in the game alike; the difference does not grow while the game runs.

With bitmap fonts, each of the Chinese language mod's six font pages is grown by 1024 rows for the characters the font lacks (log of one run):

| Font | Grown page (DXT5) | Size |
| --- | --- | --- |
| Microsoft YaHei, 12.1 px | 2048×4864 | 10 MB |
| Microsoft YaHei, 10.6 px | 2048×3584 | 7 MB |
| KaiTi, 88 px (map) | 6400×8688 | 56 MB |
| Microsoft YaHei, 13.6 px | 2048×5120 | 10 MB |
| Microsoft YaHei, 18.2 px | 3072×5120 | 16 MB |
| Microsoft YaHei, 9.1 px | 2048×2636 | 5 MB |

That is about 105 MB of pages, a quarter of the difference. The rest is probably other copies of the same pages: the grown pages are dynamic textures (`D3DUSAGE_DYNAMIC`, `unicode/fill.rs`), which Direct3D 9Ex usually keeps in system memory, and the game's own copy of each original page may stay alive. This was not checked. It is the default setting, so it is the first thing to look at.

### The framework's own costs

From the framework's log, at the main menu of each run:

| | Bitmap fonts | System fonts |
| --- | --- | --- |
| Line break questions | 85,000 to 86,000 | 72,700 to 73,100 |
| Texts whose breaks ICU worked out | 24,860, in 98 to 103 ms (4 µs each) | 20,220, in 95 to 100 ms (5 µs each) |
| Glyphs drawn | 7, in 15 ms | 3,140 to 3,160, in 3.4 to 3.5 s |
| Font pages made | 6, in 181 to 186 ms | 6, in 83 to 84 ms |

The regional combat mod's extension was called once by the time of the report, for 1 ms. The framework logs these costs again every 10 minutes when they change; the runs were shorter, so costs during play were not recorded.

## Not covered

- Frame rate and frame times: no tool for them was at hand.
- Graphics memory.
- Interface-heavy play (the ledger, long tooltips, many open windows), late game, and games longer than the six years each run covered.
- Other machines, in particular ones with 8 or 16 GB of memory, where 420 MB matters more; other playsets.
