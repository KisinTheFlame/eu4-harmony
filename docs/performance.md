# Performance

What the framework costs the game, measured on 2026-10-28 with the framework at commit 44b89f3. In short:

- **Running the game**: at speed 5 the game advanced about 2 to 3% fewer days per second with the framework, on as much CPU. With the English playset, each of three runs with the framework was behind each of three runs without it (17.8 against 18.2 days per second). With the Chinese playset the framework was behind the CJK font plugin it replaces by about as much, though there the runs spread too widely to show it on their own. Where the time goes was not looked into.
- **Starting the game**: about 1 s longer with the English playset. With the Chinese playset and bitmap fonts (the default) the main menu came about 2 s sooner than with the plugin; with system fonts about 1.3 s later, because every character is drawn at startup.
- **Memory**: 30 to 65 MB more with the English playset. With the Chinese playset and system fonts, about the same as the plugin (+15 to 35 MB); with bitmap fonts about **420 MB more**, in the menu and in the game. That is the largest cost, and the first to reduce.

## Setups

Two playsets:

- **Chinese**, the one the project's author plays: the Chinese language mod (made for the CJK font plugin), a mod that sorts and colors modifiers (whose text also needs the plugin), Graphical Map Improvements, Emoji Universalis, and the regional combat mod (inert without the framework).
- **English**: the same without the two mods whose text needs the plugin.

| Setup | Playset | In the game folder |
| --- | --- | --- |
| Framework, bitmap fonts | Chinese | the framework; framework settings "Keep the fonts as they are" (the default) |
| Framework, system fonts | Chinese | the framework; "System fonts", Microsoft YaHei for text, KaiTi for the map |
| CJK font plugin | Chinese | the plugin's `version.dll` loader and `plugins\`, no framework: how the game ran in Chinese before the framework |
| Neither | Chinese | neither the framework nor the plugin |
| Vanilla | English | neither the framework nor the plugin |
| Framework | English | the framework, bitmap fonts |

The game ran in English with Windows in Chinese, so the framework's player language was Simplified Chinese throughout.

"Neither" is not a setup anyone plays: mods made for the CJK font plugin ran without it. Entering single player crashed the game both times it was tried, in the Epic Online Services SDK (`EOSSDK-Win64-Shipping.dll`, `EOS_Presence_RemoveNotifyJoinGameAccepted`), which the game calls as it reports what the player is doing. Vanilla with the English playset never crashed there, so the crash comes from running those mods without the plugin, possibly from their legacy escapes reaching the SDK (not checked). "Neither" was measured up to the main menu only.

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

CPU is given in cores: the process's CPU seconds per second of wall time (the machine has 16 hardware threads). Memory is what Windows reports for the process; graphics memory was not measured. Each setup was measured two or three times, going through the setups and then back again, with menu-only runs for idle CPU at the main menu (Chinese playset). Game days are counted in the game's calendar, which has no leap years.

## Results

Means, with the single runs in brackets where there were several.

### Chinese playset

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

### English playset

| | Vanilla | Framework, bitmap fonts |
| --- | --- | --- |
| Launch to main menu (s) | 30.3 (30.2, 30.5, 30.2) | 31.2 (31.3, 31.2, 31.2) |
| Main menu: working set / private (MB) | 2365 / 3622 | 2401 / 3674 |
| Loading a new game (s) | 3.3 (3.4, 3.2, 3.2) | 3.3 (3.2, 3.2, 3.4) |
| Paused in game: CPU (cores) | 0.93 (0.95, 0.84, 1.00) | 0.98 (1.03, 1.06, 0.86) |
| Paused in game: working set / private (MB) | 2582 / 3907 | 2619 / 3971 |
| Speed 5: game days per second | 18.2 (18.24, 18.22, 18.23) | 17.8 (17.73, 17.96, 17.59) |
| Speed 5: CPU (cores) | 3.02 (2.99, 3.01, 3.05) | 2.97 (2.95, 2.93, 3.03) |
| After 2 min at speed 5: working set / private (MB) | 2672 / 4017 | 2699 / 4082 |

### Running the game

With the English playset, vanilla advanced 2210 to 2212 days in 121.3 s and the framework 2134 to 2179: every framework run was behind every vanilla run, by 2.5% on average. CPU was the same, so with the framework each game day took slightly more of it. Candidates, none measured: the stubs every drawn glyph goes through, the daily events the framework sends the regional combat extension, and the framework UI's work each frame. A profile of a run at speed 5 would tell.

With the Chinese playset the framework, with either font look, advanced 17.3 to 17.4 days per second against the plugin's 17.7: about 2% fewer, the same direction. There the runs of one setup spread from 16.7 to 18.0 days per second, so these runs alone could not have shown it.

Paused, the framework used as much CPU as the setups without it, within the spread between runs. All runs kept to the map: paused as the game opens and running at speed 5, without opening windows full of text.

For scale: the Chinese playset with the plugin starts 5 s slower than vanilla in English and runs about 3% slower (17.7 against 18.2 days per second), about what the framework costs at speed 5.

### Starting the game

With the English playset the framework added about 0.9 s; every framework run was slower than every vanilla run. With the Chinese playset and bitmap fonts the main menu came about 2 s sooner than with the plugin or with neither, in every run. The cause was not looked into; one candidate is that the framework turns the localisation into game text itself (255,864 entries, none left to the game's own conversion).

System fonts take about 3.3 s longer than bitmap fonts. The framework's log accounts for it: every character of the text fonts and the map font is drawn at startup, about 3150 glyphs at 1.1 ms each (3.4 to 3.5 s, on the thread that loads the fonts). Bitmap fonts draw only the few characters the mod's fonts lack (7 glyphs, 15 ms).

### Memory

With the English playset the framework took 27 to 37 MB more working set and 52 to 65 MB more private memory, at the menu and in the game. Its bitmap fonts grow the 18 vanilla font pages too, but those are small (256×1280 for most).

With the Chinese playset, system fonts cost 15 to 35 MB more than the plugin. Bitmap fonts cost about 420 MB more, at the menu and in the game alike; the difference does not grow while the game runs.

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

| | Chinese, bitmap fonts | Chinese, system fonts | English, bitmap fonts |
| --- | --- | --- | --- |
| Line break questions | 85,000 to 86,000 | 72,700 to 73,100 | 412 |
| Texts whose breaks ICU worked out | 24,860, in 98 to 103 ms (4 µs each) | 20,220, in 95 to 100 ms (5 µs each) | 378, in 7 ms |
| Glyphs drawn | 7, in 15 ms | 3,140 to 3,160, in 3.4 to 3.5 s | 1, in 0.6 ms |
| Font pages made | 6, in 181 to 186 ms | 6, in 83 to 84 ms | 18, in 27 ms |

The regional combat mod's extension was called once by the time of the report, for 1 ms. The framework logs these costs again every 10 minutes when they change; the runs were shorter, so costs during play were not recorded.

## Not covered

- Where the 2 to 3% at speed 5 goes.
- Frame rate and frame times: no tool for them was at hand.
- Graphics memory.
- Interface-heavy play (the ledger, long tooltips, many open windows), late game, and games longer than the six years each run covered.
- Other machines, in particular ones with 8 or 16 GB of memory, where 420 MB matters more; other playsets.
