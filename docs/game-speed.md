# Game speed plan

Status: decided (2026-11-04). S0, the sampler, is built and was used for the measurements in [research/main-thread.md](research/main-thread.md); S1 to S4 are built, and S7 researched, in one pull request; S5 and S6 are set aside. S8, a save's stall made shorter, was asked for by the owner afterwards (2026-11-04): built in one pull request, its first step and the faster half of its second; the rest of the second and the third set aside (2026-11-05). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Dependency point, Frame inside a day, Framework settings, Choice bar, Info mark). The bar a change must meet is [ADR making-the-game-faster-leaves-what-it-computes-as-it-is](adr/making-the-game-faster-leaves-what-it-computes-as-it-is.md). What the framework itself costs the game is in [performance.md](performance.md).

## Goal and scope

- **Goal**: the game runs faster, in days a second at the fastest speed and in frames when paused, while the game played stays the game: the same rules, the same data, the same AI, the same decisions taken by the same code. Nothing is made faster by doing less of the game's work.
- **In scope**: work on the main thread that is not the day's simulation: Paradox's online client (S1), the framework's own line breaking (S2), compressing and writing saves (S3), writing a save's game state and its file (S8), frames inside a day at the fastest speed (S4) and drawing (S7).
- **Out of scope**:
  - anything that changes what the game computes: defines, script, the AI's settings or how often it thinks, fewer monthly or yearly updates, lower precision, features turned off;
  - the day's own code (S6) and the bench it would need (S5), set aside by the owner;
  - cloud saves, which keep the game's own path;
  - multiplayer: every machine must compute the same day, which nothing here is checked for;
  - the player's own graphics settings (shadows, multisampling, vsync), which stay the player's;
  - other mods that make the game faster by changing its data.

## What "the same game" means

[research/main-thread.md](research/main-thread.md) found that the game does not repeat itself: one save, the same days, the same answers, and two runs differ after 10 days. The random draws of a day vary from run to run, and not with the number of frames drawn. So "the same results as before" cannot be shown by running a game twice and comparing. What a change may do is said instead by where it acts ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](adr/making-the-game-faster-leaves-what-it-computes-as-it-is.md)):

| Kind | Where | What may change | How it is shown |
| --- | --- | --- | --- |
| **A. Outside the game state** | Code that writes no game state: the online client, compressing and writing a save, the framework's own work, drawing | Only how long things take | By reading the code: the part's research note names what the changed code writes, and none of it is game state |
| **B. The pace of frames** | Frames inside a day, which execute no commands and draw no random numbers | How often the screen is drawn, and so how soon the player's input is read; only when the player chooses it | By reading the code (the frame's flags), and in the game: the game's own choice gives the game's own pace |
| **C. Inside the day** | The simulation's own code: the heap, the parallel stages, the daily and monthly updates | Only how long things take | Only on a bench that makes runs repeat (S5), which is set aside: no change of kind C is made |

## Where the time goes

From [research/main-thread.md](research/main-thread.md), shares of the main thread at speed 5 (1462: 16.8 days a second; 1624: 9.8), and paused:

| | 1462 | 1624 | Paused | Kind | Part |
| --- | --- | --- | --- | --- | --- |
| The day's simulation | 65% | 39% | | C | S6 |
| Frames inside a day | 7% | 32% | | B | S4 |
| Drawing, in all frames | 9% | 16% | 38% | A | S7 |
| POPS sleeping | 6.5% | 5% | 32% | A | S1 |
| The autosave (half-yearly) | 4% | 10% | | A | S3 |
| The framework's line breaking | 2% | 6% | 3% | A | S2 |
| The heap | 6% | 4% | 1% | C | S6 |
| The parallel stages' wait | 5% | 4% | | C | S6 |

The game is held back by its main thread: the other threads together ran 1.0 to 1.6 cores of 16.

## Behavior

### Always on

S1, S2 and S3 change nothing the player sees but how long things take, so they have no setting: a framework that starts has them.

### POPS without sleeping (S1)

- Paradox's client (`pops_api.dll`, the account, social and telemetry services) is served twice a frame on the main thread with a timeout of 1 ms, and sleeps that long when it has no socket. With the framework it polls: it waits for nothing and sleeps never.
- Paused, frames are a third shorter, so the game draws more of them, up to vsync's limit when vsync is on; nothing holds the rate below that.
- `pops_api.dll` comes with the game, so its bytes are dependency points like those in `eu4.exe`: when they differ, the framework does not start (ADR framework-succeeds-or-fails-as-a-whole).

### Line breaks kept (S2)

- The breaks ICU gives for a text are kept by the text's bytes, all of them compared on a hit, in a table that lets the least recently used texts go once it holds more than a set number of bytes. A text's breaks depend only on its characters and on ICU's rules for the process's locale, which do not change while the game runs, so a kept text answers what ICU would.
- ICU's line break iterator is opened once and given each text in turn.

### Saves written in the background (S3)

- While a local save runs, the game deflates its zip's entries at level 0, which copies them without compressing (a zip any reader reads), and writes, checks and moves its file by its own steps, as before. The file it leaves is a whole save, only larger. (The zip's "stored" method was tried first and wrote a save of zeros: the game's code for it reads past its data; research/saves.md.)
- A thread of the framework's then checks every entry (its CRC, and that a save's own entries begin with `EU4`), deflates the uncompressed ones at level 6, reads them back, and moves the result into the file's place, unless the file changed meanwhile.
- Every local save goes so: autosaves, the player's saves, ironman saves. Cloud saves keep the game's own path, untouched.
- The file holds what the game would have put in it, entry by entry; its compressed bytes, and so its size, differ from what the game would have written. The game, vanilla too, and other tools read it as before; the save's own checksum is of its contents and stays right.
- The next local save, a load and a restart wait until the thread is done. The game's exit does not: a file the thread had not finished stays as the game wrote it, deflated at level 0, and the temporary file the thread was writing beside it, if any, is removed at the next start.
- A file the thread cannot compress (the disk full, the file held by another program) stays as the game wrote it, stored and whole; the log says why. Since no save is lost, there is no notice (changed from the plan's first form, which compressed before anything was written).

### A save's stall shorter (S8)

The game stops while it saves: an ironman save of 1724 stopped it for about 1.2 s (research/main-thread.md, prototype-log.md). Of that, most was writing the game state into memory, the countries and the provinces the larger part, and a quarter the zip.

- Nothing changes but how long the stop lasts. The game's "Autosaving..." line shows as it does now (the owner, 2026-11-04: the save's notice stays as it is).
- The countries and the provinces are written on several threads at once while the main thread waits, and takes its share; the world does not change meanwhile, and what is written is the same, byte for byte, as the game writes on one thread.
- The zip's entries, which a local save copies uncompressed (S3), are copied by the framework into the zip, their CRC taken eight bytes at a step instead of one; the thread of S3 compresses the file afterwards, as before.
- The file is written, checked and moved into place by the game, as before: on the disk once the stop ends.
- Measured: that ironman save stops the game for 0.31 to 0.45 s instead of 1.0 to 1.4 s.

### No frames inside a day (S4)

- **The setting**: the settings window has a last group, "Game speed" (游戏速度), with one setting, "At the fastest speed" (最快速度时), followed by an info mark, set with a choice bar of two options:
  - "Smooth map (as in the game)" (画面流畅（原版）), the default: frames inside a day as the game draws them;
  - "Faster time" (时间更快): no frames inside a day at the fastest speed.
- **The info mark** shows, while the pointer is on it: "Faster time: late in a game, days pass about a tenth faster at the fastest speed, but the map is redrawn less often, so it looks choppier and answers clicks a little later." (时间更快：游戏后期，最快速度下日子过得更快（实测约快一成），但地图刷新变少，会显得一顿一顿，点击的反应也慢一点。)
- **When**: the setting takes effect when the settings window closes, in the game going on too. "Faster time" acts at speed 5 only; at speeds 1 to 4, where time waits between days anyway, frames inside a day are the game's.
- **The widgets**: the choice bar shows its options side by side in one bar, the chosen one under a highlight that slides to the other when it is clicked; the info mark is a small i in a circle, which shows its text beside it while the pointer is on it and does nothing when clicked. Both are widgets of the framework UI any setting may use.

### Cheaper drawing (S7)

- A research note of what costs the most in a frame: which windows, views and layers of the map.
- Changes only where they draw the same frame, pixel for pixel, checked by comparing screenshots. Where none is found, the note is what S7 delivers.

## How it fits together

Each part sits behind an interface no larger than what its callers need; what it knows of the game stays inside it, as dependency points.

- **Dependency points in other modules** (`points`). A place names its module: `eu4.exe` as now, or `pops_api.dll`. Each module has its own load address and its own baseline, recognized at startup as `eu4.exe`'s is; checking, installing all or none and undoing stay one mechanism for every module, and the offline checks map each module's file. Code that declares a point writes `Place::In(Module::Pops, rva)`; nothing else changes for it.
- **`speed`**, a new module of the core, holds what makes the game faster and has no other home:
  - its points: the POPS timeout (`Action::Write`, at startup) and the interval of frames inside a day (`Action::Keep` with its address recorded, and the instruction that reads it checked);
  - `speed::set_pace(Pace)`, called at startup and when the settings window closes, which writes the fastest speed's factor (the game's 0.05, or so large that no frame comes inside a day). The other speeds' factors are never written, so "Faster time" acts at speed 5 alone.
- **Line breaks** (`unicode::breaks`). The interface stays `after` and `last_at_or_before`. Inside: the last text's breaks as now, for the questions a layout loop asks character by character; behind them a table of texts seen before, by their whole bytes, least recently used first out past a byte limit; behind that ICU, through a line breaker of `icu`'s opened once. The table is a type of its own with unit tests; a test over the localisation compares what the module answers with ICU asked anew.
- **Saves in the background** (`saves::background`). Its interface is `wait()`, which the loader's and the restart's observers call, and its points: observers on the two local saves (which wait, then mark a save under way), on the game's zip entry (whose level they make 0 while one is), and on the game's file move (which notes each file the save moved into place). One thread of the framework's compresses each such file with `miniz_oxide`'s deflate (a pure Rust library, MIT, pinned as the workspace pins its dependencies) through a pure function, `compact(zip) -> Option<zip>`, tested by reading its output back with the framework's own reader, `saves::file`. The cloud save's path has no point.
- **A save's countries and provinces side by side** (`saves::parallel`). It has no interface but `start()`, which starts the pool at startup, and `threads()`, which the write watch leaves running; its points are the two loops' starts, the two functions that hold a value in a static object and the five places with that code inline. Inside: the loop's items cut into runs, a pool that takes them in turn, a run's writer and stream made from the game's, and the runs' bytes written into the game's stream; a developer build's `compare-saves` has the game's loop write after the threads and compares the two. `saves::background` stores a local save's entries at level 0 through the framework's own copy (`stored_blocks`, tested by inflating what it writes) and `saves::file::crc32_update`, eight bytes at a step.
- **The settings window** (`framework_ui`, `settings`). Two widgets, `Widget::Choice { id, options, selected }`, which draws the choice bar and slides its highlight, and `Widget::Info(text)`, the info mark, which a row puts after its label. `Settings` gains `fastest_speed: Pace`, written as `fastest_speed="smooth"` or `"faster"`, taken up when saved as the two leaving-session settings are.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and both clippy runs pass, and so do the offline checks against the game's files (`eu4.exe`, and `pops_api.dll` for S1);
- the gain was measured with `cargo xtask profile` and `cargo xtask scenario perf`, before and after, and recorded in [performance.md](performance.md), with no threshold to meet;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), and its research note says what the changed code writes;
- a change players see is in README.md and README.zh-CN.md.

### S0 The sampler

Built: `cargo xtask profile` ([xtask/src/profile.rs](../xtask/src/profile.rs), first written as `tools/profile.py`) samples the main thread's call stack from outside the process, with Windows' own dbghelp, at a steady rate, and writes a report of the shares and flame graph stacks ([dev-tools.md](dev-tools.md)). It needs no developer build and measures vanilla too.

### S1 POPS without sleeping

Dependency points outside `eu4.exe`: a place names its module, checked at startup and in the offline checks as `eu4.exe`'s are. The point at `pops_api.dll+0x6bd70`'s call of `+0x1bf9f0(loop, 1)` makes the timeout 0. Expected: 5 to 6.5% more days a second, a paused frame a third shorter. Tried in the game: signed in and signed out of the Paradox account, its features (the sign-in, the social buttons) work as before, and the client's keep-alive still goes every 45 s.

### S2 The framework's line breaks

The table and the iterator opened once, in `core/src/unicode/breaks.rs` and `core/src/icu.rs`. Unit tests compare the kept breaks with ICU's, asked anew, over the game's localisation and the framework's. Expected: most of the framework's cost at speed 5 ([performance.md](performance.md), "Running the game").

### S3 Saves written in the background

Built: the game's zip entry, `0x153b9a0(zip, name, data, method, level)`, takes the level the framework gives it, 0 in place of the save's 3; the save asks for method 8 at level 3 (zlib 1.2.3's fast setting, so a faster deflate on the main thread would gain little). The file move `0x1715bb0(from, to)` takes C strings in the folder the game writes to ([research/saves.md](research/saves.md)). Expected was 40% of each compressed save's time on the main thread; measured, about a sixth: level 0 still passes the data through zlib, which took 23% of a save where compressing took 39% ([performance.md](performance.md)). Tried in the game ([prototype-log.md](prototype-log.md)): autosaves, a save of the player's and an ironman save, compressed by the thread, which the game loaded with the framework installed; a cloud save, which the game wrote as it always does, reached Steam's cloud and loaded back; a save the thread compressed, which the game loaded and played without the framework; a return to the main menu with a restart, which waited for the thread, and the process ended while the thread compressed, which left the game's file whole at level 0. Not tried: the game's own exit button right after a save, and an end of the process while the thread writes its temporary file, which leaves `<save>.eu4harmony-tmp` beside the whole save until the next start removes it (unit tested).

### S4 No frames inside a day

The choice bar and the info mark in the framework UI; the setting in `core/src/settings.rs` and the settings window; a dependency point on the interval `[0x245b070]`, which the framework sets so large at speed 5 that no frame is drawn inside a day, and back to the game's value otherwise. Looked at with the owner in the game ([prototype-log.md](prototype-log.md)): 1624 to 1627, the game's pace drew 19 frames a second at 13.1 days a second, no frames inside a day 14 at 14.3, 9% faster; from 1730 to 1803, 19.1 days a second against 16.5, 16% faster ([performance.md](performance.md)). Tried in the game: "Smooth map" gives the game's interval; "Faster time" draws none inside a day at speed 5 (the application's frame count against the days) and the game's at speed 4; the pause key and clicks still work; an advance stops on its date; the info mark shows its text.

### S8 A save's stall shorter

Asked for by the owner, 2026-11-04, after S3: "a save's stop is very painful for a player"; the steps to go off the main thread as far as they can, and the game state written by several threads. Planned in three steps; the owner asked on 2026-11-05 for them in one pull request.

1. **The countries and the provinces side by side**, built (`saves::parallel`). What their writing writes was watched (research/saves.md, "What writing the game state writes"): the writer's state is its depth, its binary byte and its stream; besides the output, the only shared memory written is two static objects that hold the value being written (`0x15bf70` with five copies inline, `0x2c59b0`).
   - The two functions are replaced, and the five copies' stores and loads of the object's address jump to stubs, which on a thread of the pool use that thread's copy of the object (its vtable included) and on any other the game's, as before.
   - At the start of the countries' loop (`0x75174c`) and the provinces' (`0x75163a`), the framework writes the list in runs, eight a thread, which a pool of up to seven threads started at startup and the main thread take in turn; each run has a writer of its own, a copy of the game's, into a memory stream of the game's class over a buffer of the framework's, which is made larger and the run written again when it fills. The runs' bytes then go into the game's stream in the list's order, with its own write, and the game goes on after the loop; the save checksum, taken over the stream's bytes afterwards, is the same since the bytes are. When the writer or the stream is not of the class expected, the list is short, or a run fails, the game's loop runs instead.
   - Shown: a developer build's test switch, `compare-saves`, has the game's loop write each save too and compares the bytes: the same on six saves of three ironman games (1724 Austria, 1548 Georgia, 1496 Ottomans) and a plain text autosave of 1445 (England), the countries and the provinces each. The write watch, run again with the pool's threads going on, found them writing only their own copies, the pointer each country's objects keep (`0x29cd80`, the value it read), and the C runtime's own data behind its locks (research/saves.md). Measured: the two loops in 80 to 125 ms on eight threads.
2. **The file off the main thread.** Researched (research/saves.md, "Where a save's zip goes"): after the game state, `0x5ce5f0` takes the save checksum (44 ms in that save), writes the meta and the AI (3 ms), builds the zip in memory (`0x153b9a0` for each entry; the game's level 0 still went through zlib and took the CRC a byte at a time, about 250 ms) and writes it into the `.tmp` file's stream (25 ms); the local save then closes it, checks its size, backs up the old save and moves the new one into place (41 ms).
   - Built: the zip's copy at level 0 by the framework (`saves::background`, the deflate `0x153c790` replaced at that level), stored blocks of 64 KB with the CRC eight bytes at a step: 45 ms.
   - Set aside: moving the rest, the file's 66 ms, to a thread would replace the local saves' own steps after the write (the size check, the backup, the move, the ironman save's) with the framework's, in every local save, for a fifteenth of the stop; and a crash in those seconds would leave the save before it.
3. **Faster parts of the writing**: set aside. What is left of the writing on the main thread is the game state's other sections (66 ms) and the save checksum (44 ms), where faster primitives would gain a few tens of milliseconds.

### S5 A bench that repeats

Set aside by the owner, 2026-11-04, as too hard for now. What it would be:

The bench need not play as vanilla does; it must give the same days twice from the same save, so that a build with a change and one without can be run on it and their days compared. Two routes, the first tried first:

1. **Replaying the commands, as a multiplayer client does.** Multiplayer needs every machine to compute the same day from the same commands, and only the host runs the AI, which acts through commands ([research/architecture.md](research/architecture.md), "Commands and multiplayer", "The AI"). So: a recording run plays a save for some days and records every command executed, with the day it ran in (the command layer hears them all, [commands.md](commands.md)); a replaying run loads the same save with the AI off (`world+0x7484`) and has the game execute the recorded commands on the same days, in the same order. Each day's checksum record (the one multiplayer compares, `0x743aa0`) is compared between two replays. If the commands are what varies, two replays give the same days. What is not known: whether the AI is all that varies (a day drew tens of random numbers more or fewer from run to run, research/main-thread.md); whether commands can be executed at the place in the frame and with the stamps a client gives them; whether the game posts commands of its own that a replay would post twice.
2. **Finding what varies.** A developer build's tool that records the draw count and the checksum record at the boundaries of the day's phases, so that two runs show the first phase in which they part, and that phase is read further. If what varies is not the AI, the replay holds it too, and it is made to repeat on the bench alone (a stage run in a fixed order, for example), both builds compared with it.

Changes to the AI's own code cannot be checked on a replay, which runs no AI.

### S6 Inside the day

Set aside with S5, which it needs. A faster allocator changes where blocks lie, and code that orders by address, hashes addresses or reads memory it did not set would then go another way; the parallel stages are where the runs' differences may come from. The candidates:

- the heap: a faster allocator for the game's `operator new` and `delete`, 4 to 6% of the main thread;
- the parallel stages: the main thread taking parts of a stage instead of waiting (`0x179e90`, 4 to 5%);
- the day's own code, where a profile with names finds one function with a large share of its own: each country's daily update (19 to 33%) and the monthly update (9 to 15%) first.

### S7 Cheaper drawing

Research with the sampler and the call trace: the GUI (`0x14dd010`, `0x14dd540`) and the map (`0x10a59a0`) were each about a third of the drawing, and the rest of the in-game frame (`0xdf5860`, `0x10a6d00`, `0x6ea380`, `0x128cd30`) unnamed. Then changes that draw the same frame for less, such as skipping what is hidden, each checked by screenshots of the same scenes before and after.

Researched, nothing built ([research/main-thread.md](research/main-thread.md), "Drawing"): the main thread's drawing is mostly the game's draw calls, inside `d3d9.dll`, up to three map views a frame and the GUI's objects; the GUI already leaves out what is clipped away. No work was found that draws nothing the player sees, so no change is shown to leave the frame as it is; fewer draw calls would be a change to the renderer, a plan of its own.

## Order

One pull request: S2 and S1 first (small, measured), then S4, S3, and S7 alongside. S8 later, in a pull request of its own.
