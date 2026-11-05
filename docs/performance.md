# Performance

What the framework costs the game, measured on 2026-10-28 with the framework at commit 44b89f3. In short:

- **Running the game**: at speed 5 the game advanced about 2.5% fewer days per second with the framework, on as much CPU: with the English playset, each of three runs with the framework was behind each of three runs without it (17.8 against 18.2 days per second). Where the time goes was not looked into.
- **Starting the game**: about 1 s longer with the English playset. With the Chinese playset, system fonts took about 3.3 s longer than bitmap fonts (the default), because every character is drawn at startup.
- **Memory**: 30 to 65 MB more with the English playset. With the Chinese playset, bitmap fonts took about **410 MB more** than system fonts, in the menu and in the game. That is the largest cost, and the first to reduce: most of it is what Direct3D keeps after the framework writes its grown font pages, the rest the glyph records each font keeps a copy of.

## Since then

Two changes the same day, both found by the measurements under [Memory](#memory):

1. The grown font pages are written through a small system-memory texture instead of being locked in place.
2. Fonts that bring the same glyphs from their `.fnt` files share one set of them, stored inline, instead of each keeping its own copy of every record.

Measured from the framework's log, Chinese playset, to the main menu:

| | Bitmap fonts: before | after 1 | after 2 | System fonts: before | after 1 | after 2 |
| --- | --- | --- | --- | --- | --- | --- |
| Process private bytes at the main menu | 4353 MB | 3866 MB | 3761 MB, 3763 MB | 3860 MB | 3778 MB | 3767 MB, 3746 MB |
| The framework's heap at the main menu | 66 MB | 66 MB | 3.2 MB | 2.8 MB | 2.8 MB | 1.3 MB |
| Writing the six pages | 560 MB | 40 MB | 38 MB | not measured | 9 MB | 9 MB |
| Drawing a glyph into a page | 2.1 ms | 91 µs | 58 µs | 1.1 ms | 9 µs | 9 µs |
| Making the six pages | 181 ms | 27 ms | 27 ms | 84 ms | 23 ms | 22 ms |
| From the framework loading to the main menu | 30.8 s | 31.5 s | 31.2 s | 34.9 s | 30.8 s | 30.6 s, 30.5 s |

- **Memory**: bitmap fonts now hold about 590 MB less than before, and as much as system fonts. The 994,216 glyph records the 46 fonts brought are kept as 6 shared sets of 115,053.
- **Starting the game**: system fonts no longer take longer than bitmap fonts. Their 3,143 glyphs were drawn in 28 ms instead of 3.5 s; the time had been the locks.
- **System fonts at an interface scale above 1** (2026-10-30): glyphs are drawn that many times larger, on pages that many times wider and taller, so a page holds as many. At 1.5 the five pages of Microsoft YaHei are 1536 pixels square instead of 1024, and creating each added 9 MB of private bytes instead of 4, about 25 MB in all, as the framework's log counts it; the process at the main menu was not measured again.
- **Bitmap fonts are gone** (2026-11-02, [ADR game-text-in-system-fonts](adr/game-text-in-system-fonts.md)): every font is drawn from system fonts, which the measurements above put level with bitmap fonts in memory and in the time to start.
- The tables below are from before the changes. Game speed was not measured again.

## The game speed plan (2026-11-04)

[game-speed.md](game-speed.md)'s S1 to S4, built on the branch of that date: Austria-1 (an ironman save of 1618.9.2, the player's playset), 3840x2160, vsync at 144 Hz; frames counted at the application's frame count and days at the date, 30 to 32 s each, no sampler. Before is the same save the same morning with the framework of that time ([prototype-log.md](prototype-log.md)).

| | Before | After |
| --- | --- | --- |
| Paused: frames a second | 63 | 83.8 |
| Speed 5, the game's pace, 1618.9 to 1620.4: days a second (frames a second) | 12.5 (18.2) | 13.8 (19.3) |
| Speed 5, 1620 to 1622: "Faster time" against the game's pace | | 15.3 (15.4) against 13.9 (19.2) |
| ICU's line breaking at speed 5, share of the main thread | 6% (1624) | 0% |
| POPS on the main thread at speed 5 | 5% (1624) | 0% |
| An ironman autosave: compressing, share of the save's samples | 39% | 23% (copying at level 0) |

- Two more pairs of the paces, later the same run, came out at 11.0 and 6.4 days a second, both paces alike; the machine was busy with other work then (a sample took 2.2 ms against 1.4), and they are left out.
- The autosave's main thread time, from the shares: about a sixth less. The thread then compressed 61 MB in 1.6 to 2.0 s, to 7.1 to 7.2 MB, smaller than the game's 8.8 MB.

Later in a game (the same day, the build of master at `c6bda5f`): Austria-37_4-1, an ironman save of 1724, the same machine and playset, the game resumed within 0.3 s whenever a message paused it ([prototype-log.md](prototype-log.md)):

| | The game's pace | "Faster time" |
| --- | --- | --- |
| Paused, 1724: frames a second | 44.6, 49.7 | |
| Speed 5, 1730 to 1803, eight blocks of 60 s each way: days a second (frames a second) | 16.5 (20.0 to 24.2) | 19.1 (16.9 to 21.6) |

- "Faster time" was 16% faster there, more than the 10% of 1620. Earlier runs of the same save, resumed only every 2 s, came out the other way round, which was not explained; they are left out.
- The thread compressed each ironman save of 55 MB to 6.1 MB in 1.5 s; the game without the framework wrote the same game at 7.75 MB.

## A save's stop (2026-11-05)

[game-speed.md](game-speed.md)'s S8 on its branch: Austria-37_4-1 (an ironman save of 1724, the player's playset), the same machine, loaded three times with `menu.load`, which saves it as it is loaded and as it is left; the stop is the framework's log line from the start of the local save to its end, five saves each way, in one developer build with the test switches, `game-writes-saves` on for "before" ([prototype-log.md](prototype-log.md)).

| | The game's code | The framework's |
| --- | --- | --- |
| A local ironman save of 55 MB stops the game, ms | 986, 1116, 1233, 1256, 1362 | 308, 431, 437, 438, 447 |
| Of that, the provinces and the countries (eight threads), ms | | 80 to 125 |
| Of that, the zip's three entries, ms | about 250 | 45 |

- What is left of the 310 ms save: the game state's other sections 66 ms, its save checksum 44, the zip 45, its bytes into the file 25, the file's checks and move 41 ([research/saves.md](research/saves.md), "Where a save's zip goes").
- The pool of seven threads waits from startup and costs nothing between saves; while a save is written, the runs' buffers hold the two loops' bytes once more, about 60 MB in that save.

## Setups

Two playsets:

- **Chinese**, the one the project's author plays: the Chinese language mod, a mod that sorts and colors modifiers, Graphical Map Improvements, Emoji Universalis, and the regional combat mod (inert without the framework).
- **English**: the same without the Chinese language mod and the modifier mod.

| Setup | Playset | In the game folder |
| --- | --- | --- |
| Framework, bitmap fonts | Chinese | the framework; framework settings "Keep the fonts as they are" (the default) |
| Framework, system fonts | Chinese | the framework; "System fonts", Microsoft YaHei for text, KaiTi for the map |
| Vanilla | English | no framework |
| Framework | English | the framework, bitmap fonts |

The game ran in English with Windows in Chinese, so the framework's player language was Simplified Chinese throughout.

## Machine

AMD Ryzen 7 9700X (8 cores, 16 threads), 64 GB of memory, GeForce RTX 4070 Ti SUPER (16 GB), Windows 11 Pro, 3840×2160. EU4 1.37.5 from Steam, started directly as `eu4.exe`.

A training job that kept the processor about 80% busy and held 14 GB of the graphics card's memory was stopped before the runs below. One run made while it ran is left out: framework with system fonts, 51 s to the main menu and 11 game days per second, against 37 s and 17.4 days idle.

## Method

The runs below were made with `tools/perf-run.ps1`, which measured vanilla too and so drove the game by the screen. It ran the game the same way each time:

1. Starts `eu4.exe` and times the launch until the main menu is on screen (a region of the menu's logo, compared every 0.25 s).
2. 20 s later reads the process's working set and private bytes.
3. Opens single player, picks the 1444 East Asia bookmark and Ming, starts in normal mode, and times the load until the start-of-game introduction appears.
4. Closes the introduction, waits 10 s, and measures CPU over 30 s paused, on the map as the game opens it.
5. Sets speed 5, runs the game for 120 s, pauses, and reads the CPU, memory and date.

CPU is given in cores: the process's CPU seconds per second of wall time (the machine has 16 hardware threads). Memory is what Windows reports for the process; graphics memory was not measured. Each setup was measured two or three times, going through the setups and then back again, with menu-only runs for idle CPU at the main menu (Chinese playset). Game days are counted in the game's calendar, which has no leap years.

The scenario `perf` ([xtask/src/scenario/perf.rs](../xtask/src/scenario/perf.rs), `cargo xtask scenario perf`, [scenarios.md](scenarios.md)) measures the framework's game only, on the player build: within the claim it starts the game hidden and times the start to the main menu, reads memory 20 s later, starts a new game as Ming with `menu.start` and times it, measures CPU over 30 s paused, then 120 s at speed 5 through `time.set_speed`, `time.resume` and `time.pause`, and reads the dates with `time.get`, from which come the game days a minute. Its start is timed from the start command, not from a menu logo on screen, so its menu seconds are not those of the runs below.

The figures on this page come from `tools/perf-run.ps1` (since removed), from the framework's log and from `cargo xtask profile`; none yet from the scenario. Its first runs, on 2026-11-05 ([prototype-log.md](prototype-log.md), "Scenarios"), checked that it plays to its end: 47.2 s to the main menu, 9.7 s for `menu.start MNG`, 0.81 cores paused and 1.75 at speed 5, and 487 game days a minute, with 305.6 and 453.3 in two later runs, a spread larger than the framework's setups are likely to differ by. The next measurement of what the framework costs is taken with `cargo xtask scenario perf` and recorded here.

## Results

Means, with the single runs in brackets where there were several.

### Chinese playset

| | Framework, bitmap fonts | Framework, system fonts |
| --- | --- | --- |
| Launch to main menu (s) | 33.5 (33.1, 33.1, 34.2) | 36.8 (37.4, 36.3, 36.8) |
| Main menu: working set / private (MB) | 2856 / 4194 | 2466 / 3787 |
| Main menu, idle: CPU (cores) | 0.56 | 0.56 |
| Loading a new game (s) | 3.5 (3.4, 3.5) | 3.7 (3.7, 3.7) |
| Paused in game: CPU (cores) | 0.99 (1.01, 0.97) | 0.93 (0.88, 0.99) |
| Paused in game: working set / private (MB) | 3073 / 4494 | 2672 / 4077 |
| Speed 5: game days per second | 17.3 (16.7, 18.0) | 17.4 (17.5, 17.4) |
| Speed 5: CPU (cores) | 2.95 (2.96, 2.93) | 2.98 (3.07, 2.90) |
| After 2 min at speed 5: working set / private (MB) | 3164 / 4596 | 2774 / 4185 |

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

With the Chinese playset, where the runs of one setup spread from 16.7 to 18.0 days per second, the two font looks ran alike.

Paused, the framework used as much CPU as vanilla, within the spread between runs. All runs kept to the map: paused as the game opens and running at speed 5, without opening windows full of text.

### Starting the game

With the English playset the framework added about 0.9 s; every framework run was slower than every vanilla run.

System fonts take about 3.3 s longer than bitmap fonts. The framework's log accounts for it: every character of the text fonts and the map font is drawn at startup, about 3150 glyphs at 1.1 ms each (3.4 to 3.5 s, on the thread that loads the fonts). Bitmap fonts draw only the few characters the mod's fonts lack (7 glyphs, 15 ms).

### Memory

With the English playset the framework took 27 to 37 MB more working set and 52 to 65 MB more private memory, at the menu and in the game. Its bitmap fonts grow the 18 vanilla font pages too, but those are small (256×1280 for most).

With the Chinese playset, bitmap fonts cost about 410 MB more than system fonts, at the menu and in the game alike; the difference does not grow while the game runs.

With bitmap fonts, each of the Chinese language mod's six font pages is grown by 1024 rows for the characters the font lacks (log of one run):

| Font | Grown page (DXT5) | Size |
| --- | --- | --- |
| Microsoft YaHei, 12.1 px | 2048×4864 | 10 MB |
| Microsoft YaHei, 10.6 px | 2048×3584 | 7 MB |
| KaiTi, 88 px (map) | 6400×8688 | 56 MB |
| Microsoft YaHei, 13.6 px | 2048×5120 | 10 MB |
| Microsoft YaHei, 18.2 px | 3072×5120 | 16 MB |
| Microsoft YaHei, 9.1 px | 2048×2636 | 5 MB |

That is about 105 MB of pages, and the 1024 rows reserved below the mod's glyphs are only 18 MB of it. Measuring the process's private bytes around each step of loading the fonts (logged since, see `core/src/heap.rs`) found where the difference goes, against a run with system fonts to the same main menu (4353 against 3860 MB):

| What | Bitmap fonts | System fonts | Difference |
| --- | --- | --- | --- |
| Loading the map font, page included | +539 MB | +253 MB | +286 MB |
| Loading the five body font pages | +213 MB | +58 MB | +155 MB |
| The framework's heap at the menu | 66 MB | 3 MB | +63 MB |

- **The pages**: creating each grown page as a dynamic texture commits about its own size, but writing it, 256 rows at a time with a lock each, commits several times its size and keeps it: 425 MB for the 56 MB map page alone (details in [research/font-textures.md](research/font-textures.md)). Writing a page with fewer locks, or keeping it in video memory and copying glyphs in from a small system-memory texture, should remove most of it.
- **The heap**: 46 font objects load, most of them from the same few `.fnt` files, and each keeps its own copy of the 21,443 glyphs above 0xFF: 994,216 records, each allocated on its own. System fonts drop them. Sharing one table per file, with the records stored inline, would leave a few MB.

It is the default setting, so it is the first thing to reduce.

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

- Where the 2.5% at speed 5 goes. Since measured in part: the framework's line breaking was 1.8% of the main thread at speed 5 early in a game and 6% late ([research/main-thread.md](research/main-thread.md)), what the game itself spends its time on is there too, and making the game faster is planned in [game-speed.md](game-speed.md).
- Frame rate and frame times: no tool for them was at hand.
- Graphics memory.
- Interface-heavy play (the ledger, long tooltips, many open windows), late game, and games longer than the six years each run covered.
- Other machines, in particular ones with 8 or 16 GB of memory, where 410 MB matters more; other playsets.
