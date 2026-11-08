# Where startup and a save's load spend their time

Read 2026-11-05 for making the game's startup and its loads shorter. Sampled with `cargo xtask profile` (`--wait --slice 5` from the process's start, `--thread` for threads other than the main one), the player build of the framework installed, the player's playset, every DLC; the machine's processor was 88% busy with other work throughout, so the times are long. Addresses are RVAs into the 1.37.5 baseline.

## Startup

From the process's start to the main menu: 46 to 55 s in five starts. In three stretches:

| Stretch | Time | What runs |
| --- | --- | --- |
| The engine's Init on the main thread | 0 to about 17 s | Sound and music loaded (`0x14c02b0` → `0x15c9c10`, "sound", "music"), most of it inflating DLC zips |
| The databases on the LoadingThread | about 17 to 47 s | `0x200810` → `0x2043a0`, "Loading Databases"; the main thread waits |
| The main menu made | about 47 to 53 s | the main menu's constructor, the first frames |

- **Music, read whole.** For each music asset (`0x15c92c0`, `assetfactory_audio.cpp`, "music/065_music.asset"), `0x16a4930` (`pdx_audio.cpp`) calls `0x16a9a40` (`pdx_audiosound_sdl.cpp`), which opens the file (`0x1718300`), takes its length (`0x19f6ec0`), reads all of it into a vector (`0x19f8150`, PhysFS's read) and hands it to `0x1806bf0`, Vorbis's open from memory (`ov_open_callbacks`). It then keeps in the sound the channels (`+0xd8`, 1 for mono and 2 otherwise, from `ov_info`), the rate (`+0xdc`), the length in samples (`+0xc8`, `ov_pcm_total`) and the duration in nanoseconds (`+0xd0`, `ov_time_total` times 1e9, the constant at `0x1dc1850`), and returns the decoder, which the caller (`0x16a4c78`) closes and lets go with the file's bytes. With its second argument 0, `0x16a9a40` opens the file as a stream instead (`0x1710220`) and keeps the same four numbers.
- **Who reads the four numbers** (watched 2026-11-05 with the processor's debug registers on the sound of `music/maintheme.ogg`, set for reads and writes on every thread by a developer build made for it, the first 45 s from the startup):
  - the songs' list: `0x1fdf32`, under the LoadingThread's databases (`0x200abc` → `0x202696` → `0x20491d` → `0x20f092`), finds each song's music by its name (`0x16a4cd0`), reads its duration (`[record+0xe0]`, the sound's `+0xd0`), and keeps it as minutes and seconds (`+0x58` of the song's entry): the length the music player shows for each song. Once, for this song;
  - its play: when the song starts, `0x16a8522` opens it again as a stream (`0x16a9a40` with 0) and writes the four numbers anew; after that the audio thread reads the channels and the rate (`0x16aa3b9`, `0x16aa3e3`, under `0x16a6b39`) and the playing sound its length in samples (`0x16a74be`, against where it is, for its end), about once a second each;
  - nothing else read them. So of what the startup reads every file whole for, only the duration is used before a song plays: for the songs' list. The DLC's music is 238 Ogg files of 3,133 MB in their zips, deflated (method 8) to 3,092 MB: compressed data deflated again, which every read inflates (`0x19f8180` → `0x1a07a10` → `0x1a080d0` → `0x1a08470`, PhysFS's zip archiver). In the stretch from 5 to 15 s, 74 to 81% of the main thread's samples were in `0x1a08470` and 13 to 16% in `ReadFile` under it.
- **The databases** (`0x2043a0`, between "Used RAM Map before Loading Databases" and "after"): `0x208620`, the databases from `common/` (technologies, trading policies and the rest), 44% of the LoadingThread's samples; `0x6f3250`, the events (`eventmanager.cpp`), 18%; `0xb9b4e0`, the triggers' optimization profile (`trigger.cpp`), 9%; `0x10b2b70`, flags and loading screens (`eu3graphics.cpp`), 6%; `0x202850` → `0x14dc5d0`, meshes and particles (`pdxmeshobject.cpp`), 13% at its start. Of the thread's samples, 59% ran, 23% waited (locks, other threads) and 18% were in file calls. The main thread waits for it in a loop (`0x878050`: messages, `0x169a2f0`, the online services' tick, and `Sleep(10)`) while `[0x242bbcb]` is set.
- **File lookups.** Threads of the game's own pool (`0x1712b40` → `0x8d80d0`, pictures, "The image for '…' does not exist as a picture!"; `0x15cce80`, `pdxassetutil.cpp`, a mesh's textures) and the LoadingThread ask PhysFS whether files exist (`0x19f6ea0` → `0x19fad70` → `0x19fb700` → `0x1a07080`), which asks Windows for each directory in the search path (`0x1a061d0`, `GetFileAttributesExW`, and `0x1a05220`, which converts the file's times with `GetTimeZoneInformation`). The search path holds the game folder, the user folder, the playset's mods and the DLC's 123 zips. In the busiest thread of each second from 20 to 30 s, 55 to 69% of the samples were in `GetFileAttributesExW`; from 30 to 40 s, 45 to 51% in a lock (`0x1a137cc`, `0x1a139f0`) under `0x8d80d0`.
- **The framework's own share**: about 4% of the main thread's samples in the first stretch (its startup, `eu4_harmony_core.dll+0x222630`), 1% of the LoadingThread's; on the stack of more, as the observer of PhysFS's read and of the tokenizer.

## A save's load

Austria-37_4-1 (an ironman save of 1724, a game state of 55 MB) loaded from the main menu with `menu.load`: about 10 s to a frame of the game, the ironman save as it is loaded included ([The startup and a load sampled](../prototype-log/2026-11-05-startup-load-sampled.md)). Of the main thread's samples over those 10 s:

- **Reading the save** (`0xd84b80` → `0x74b8a0` → `0x5cdc60` → `0x5cf740` → `0x14e39d0`, the world's entries): 37%.
- **`0x7e7cd0`**, 24% of its own: a method that many classes share (it is in vtables from `0x1c4fcd0` on, one each 0xd0 bytes, and called directly at `0xa368eb`), which adds an object to a list at `this+0x20`: it walks the whole list first for the object already there (`0x7e7cf0`, by `[node]`, the next at `[node+0x10]`), then walks again from `this+0x28` for the place by `[object+0x10]`, and links a node of 0x20 bytes there. So adding n objects walks the list n times; here from `0xda66a0` (in the vtable at `0x1d21348`) while the save is read. The child is one `0xda66a0` made itself (`new(0x40)` at `0xda6905`, the vtable `0x1d214c8`), read (`0x17068b0`) and then added to its parent with the parent's method `+0x98` (`0xda69e5`); the node's date (`+0x10`) is the least of its children's (`0x7e7f80`, from `0x29c77f8`), and the add's second walk, from the list's end (`this+0x28`) back by date, stops at once when the children come in order of date. The class's name is not in the binary (no RTTI).
- **The interface made after the load** (`0x10da520` → `0x803270`, `0x81af10`): sprite types' textures (`0x156e0a0` → `0x16c35b0`, 22%) and, under them and elsewhere, PhysFS's lookups again: 12.6% in `GetFileAttributesExW`.
- **The day's setup** (`0x350b80` → `0x2d2520`, `0x7760b0` → `0x2d0b30`): 8 to 14%.

## The startup after L1 to L3

Timed 2026-11-08 with `dev-call-trace`'s `times` on the stretches' functions, in a test game of the developer build with the test games' pipes, the player's playset, every DLC, the music kept (the second start after the claim) and the files in the system's cache; other test games held the machine's processor at 40 to 65% ([the run](../prototype-log/2026-11-08-startup-after-l1-to-l3.md)). Seconds from the framework's bootstrap, in two starts that reached the main menu at 41.6 and 37.7 s:

| Stretch | Function | Thread | s, s |
| --- | --- | --- | --- |
| `main` up to the defines (DLC, mods, the search path, the listings) | | main | 2.7, 2.3 |
| The engine's Init, music now kept | `0x14c02b0` | main | 1.0, 0.9 |
| `.gfx`, the map, `shortcuts.txt`, `.gui` | `0x202850` | LoadingThread | 7.1, 7.4 |
| of that, meshes and particles | `0x14dc5d0` | LoadingThread | 4.7, 4.8 |
| The databases | `0x208620` | LoadingThread | 16.7, 15.5 |
| of that, the map logic | `0x87b9e0` | LoadingThread | 0.7, 0.7 |
| of that, the history | `0x7ea2c0` | LoadingThread | 1.9, 1.8 |
| The events, on_actions, disasters | `0x6f3250` | LoadingThread | 3.5, 3.4 |
| Flags and loading screens | `0x10b2b70` | LoadingThread | 1.6, 1.5 |
| The rest of InitGame (the triggers' optimization profile `0xb9b4e0` among it) | `0x2043a0` | LoadingThread | 3.6, 3.4 |
| The data checksum, beside the databases | `0x171cdf0` | a pool thread | 14.5, 13.3 |
| The main menu's RestoreDeviceObjects' part `0x206260` | `0x206260` | main | 3.9, 2.3 |
| of that, the map's graphics | `0x10a2e40` | main | 1.8, 1.3 |
| Then to the main menu's first frame | | main | 1.1, 0.6 |

- **The data checksum** (`0x171cdf0`) starts on a thread of the pool as the databases start and ends before them, so it is not what the startup waits for; it reads and sums every file `checksum_manifest.txt` names, opening each through `0x1718300`.
- **The LoadingThread waits.** Sampled the same day (`cargo xtask profile --wait --threads --slice 5`, 47 a second), the LoadingThread was found running at 25 to 75% of the ticks of each stretch while it loaded the databases, the rest waiting in the kernel. Where it ran in a wait: in `0x1718300`, the game's only place that opens a file, which takes a byte spin lock (`0x1581820` on the pointer at `0x242b9c0`: `lock cmpxchg`, `pause` with backoff, then `SwitchToThread` from the 16th try) around PhysFS's open (`0x19f7f60`); the checksum's thread opens through the same lock. What it waited on in the kernel the sampler cannot see.
- **The picture checks poll.** `0x208620` starts them first (`0x208699` → `0x8effc0`, a task with vtable `0x1cc8e98` whose run is `0x8f5830` → `0x8d80d0`). `0x8d80d0` takes a `std::mutex` (`0x1a139cc`/`0x1a137cc`, `_Mtx_lock`), pops an entry from its queue (count at `+0x1c`, entries at `+0x10`), and, with none and `+0xa8` not set, lets the mutex go and takes it again at once (`0x8d8155` back to `0x8d8100`), with no wait between. One pool thread was found running at 94% of the ticks from 20 to 25 s, nearly all in that lock's acquire and release; who fills the queue and sets `+0xa8` was not followed.
- **Glyphs.** The framework drew about 4,250 glyphs before the main menu, 1.4 to 2.5 s in all as its log counts them (330 to 590 µs each), against 28 ms for 3,143 on 2026-10-28; most on the main thread in the main menu's RestoreDeviceObjects, a few on the LoadingThread as the fonts load. Timed outside the game on the same machine, a glyph of the map font (FZShengShiKaiShuS-EB-GB, 78 px) took 100 to 600 µs in DirectWrite and 160 to 170 µs more for its outline (`outline::around`); one of the body font at 21 px 2 to 260 µs, the spread with what DirectWrite had already read of the font.

## The databases

Read 2026-11-08 from `0x208620`'s code, and timed in a test game as above, the machine's processor about 50% busy with other test games ([the run](../prototype-log/2026-11-08-where-the-databases-go.md)).

- **What it does.** `0x208620` loads about 110 databases one after another on the LoadingThread, in an order fixed in its code. For most it takes the database's object from its getter, makes the folder's path (`"/trading_policies"`), and calls the loader with both, which lists the folder's `.txt` files under `common\` and reads each through the lexer (`0x1709570` makes one over a file, as for `technology.txt`). 67 of them it brackets with the process's memory counters (`0x14e44d0` before, `0x14e4500` after) for setup.log's "<Name> Initialized [n KB]"; the rest, among them `estates_preload`, `subject_types`, `great_projects`, `institutions`, `ages`, the personalities, `missions` (`0x8d5bd0`), `decisions` (`0x4882c0`) and `mercenary_companies`, have no line. Nothing of it runs beside it but the picture checks' task it starts first (`0x8effc0`), the data checksum on a pool thread, and the history's files, which pool threads open (below).
- **Where the time goes.** Timed with `dev-call-trace`'s `times` on the brackets and 13 loaders without them, in two starts of 13.7 and 13.8 s of databases; seconds from the second:

| Databases | Functions | s |
| --- | --- | --- |
| Technologies, the second phase | `0x9b240` → `0xb74120` | 1.93 |
| Estates: crown land, agendas, privileges, estates | `0x728570` to `0x72e4a0` | 1.55 |
| History | `0x7ea2c0` | 1.50 |
| The countries' files ("CountryDatabaseFiles Read") | `0x283d20` to `0x2855b0` | 1.48 |
| Decisions | `0x4882c0` | 1.29 |
| `government_mechanics`, the second read | `0x7b8ed0` | 0.81 |
| `0x46fab0`, after the federation advancements | `0x46fab0` | 0.76 |
| Map logic | `0x87b9e0` | 0.68 |
| `custom_ideas` | `0x46cf00` | 0.59 |
| Missions | `0x8d5bd0` | 0.45 |
| `scripted_triggers`, `government_reforms`, `scripted_effects` | `0xace660`, `0x7a7520`, `0xacb950` | 0.36, 0.27, 0.25 |
| The other 90 or so | | 1.9 |

- **Running and waiting.** Sampled at 94 a second with `--threads` in a third start, the LoadingThread was running at 455 of the about 1,280 ticks of the databases (36%), 17% of those in the heap. Decisions, missions and the map logic ran at nearly all their ticks; the technologies, the estates, the countries and the history at a fifth or fewer.
- **Opening files.** Timed in another start, with `0x1718300` and its spin lock (`0x1581820`) traced: in 14.2 s of databases the LoadingThread opened 3,908 files, 6.4 s in all (1.6 ms an open), and waited 6.7 s on the open's lock (the global at `0x242b9c0`, 7,824 takes); the 431,112 takes of another lock through the same function waited 0.07 s. Beside it the checksum's thread opened 8,478 files, and the pool's threads 3,900 from 22 to 24 s, under the history, waiting 15 s on the lock among them. From 10 to 18 s only the LoadingThread and the checksum's thread opened files: the LoadingThread 100 to 300 a second, waiting 0.4 to 0.9 s of each second on the lock, the checksum's thread about 200 a second.
- **Inside PhysFS's open.** `PHYSFS_openRead` (`0x19f7f60`) takes PhysFS's own lock (`[0x2339238]`, taken by `0x1a05e60`, given back by `0x1a06150`) and walks the search path: in each entry it checks the path (`0x19fb700`) and asks the archive to open it (`[archiver+0x40]`), until one does. Timed: 61 checks for each of the LoadingThread's opens (238,188), and about 100 for each operation of the other threads; the picture checks' lookups (`0x19f6ea0`) took 16,044 calls and 9.9 s on the other threads in the same stretch, 0.6 ms each, under the same lock. With PhysFS's lock traced as well, 5.5 million takes, the databases took 15.8 s, and the LoadingThread waited 2.6 s in its take.
- So of the databases' 14 s, about 6.5 s on the LoadingThread is opening files, most of it waiting for the threads that open and look up files beside it, each operation of theirs walking the search path under two global locks; the scripts' parsing itself is about 4.5 s of running.
- What the walks write, and so what reading side by side must make safe (read 2026-11-08 for [L4](../plans/loading.md)): the open links its file into the list of open files (`[0x23391f8]`, the next at `+0x38`) and the close walks the list and takes it out (`0x19f9980`); the lookup of a name in a zip's tree (`0x19f9280`, the hash `h * 33 ^ c` from 5381) moves the entry it finds to the front of its bucket (`0x19f93a1`); a zip's entry read (`0x1a0b420`, from the zip's open `0x1a07580` and stat `0x1a07950`) reads the entry's local header through the zip's own file the first time, its state at `+0x30` (0 a file and 1 a link not read yet, 2 being read, 3 read, 4 a folder, 5 and 6 broken). Nothing else the open, the lookup or the close reaches writes what another thread reads: the path's check (`0x19fb700`) and the archives' opens and stats take no PhysFS lock but the error's (`[0x2339230]`), which keeps each thread's error apart; a memory file's copy counts its users with `lock inc` (`0x19f637b`). The archives' tables: the zip's at `0x1c1d890`, the folder's at `0x1c1d7a0` (open at `+0x40`, stat at `+0x68`).

## The startup with L4

Timed 2026-11-08 as "The startup after L1 to L3" was, after [L4](../plans/loading.md) was built: the same functions with `dev-call-trace`'s `times`, in a test game of the developer build with the test switches, the music kept, five starts alternating L4 on and left to the game (`game-paths = ["opens"]`); other test games held the processor at about 70% ([the run](../prototype-log/2026-11-08-startup-with-l4.md)). Seconds from the framework's bootstrap, two starts each way (the third with L4 on reached the main menu at 31.0 s):

| Stretch | Function | Thread | L4 on | Left to the game |
| --- | --- | --- | --- | --- |
| `main` up to the engine's Init | | main | 2.3, 2.5 | 2.4, 2.3 |
| The engine's Init | `0x14c02b0` | main | 1.1, 1.0 | 0.9, 1.0 |
| `.gfx`, the map, `shortcuts.txt`, `.gui` | `0x202850` | LoadingThread | 7.6, 6.7 | 6.2, 6.6 |
| of that, meshes and particles | `0x14dc5d0` | LoadingThread | 5.0, 4.6 | 4.2, 4.5 |
| The databases | `0x208620` | LoadingThread | 8.3, 8.7 | 15.9, 15.7 |
| of that, the map logic | `0x87b9e0` | LoadingThread | 0.7, 0.6 | 0.7, 0.6 |
| of that, the history | `0x7ea2c0` | LoadingThread | 0.7, 0.7 | 2.0, 1.9 |
| The events, on_actions, disasters | `0x6f3250` | LoadingThread | 2.9, 3.0 | 3.2, 3.1 |
| Flags and loading screens | `0x10b2b70` | LoadingThread | 1.4, 1.5 | 1.6, 1.4 |
| The rest of InitGame | `0x2043a0` | LoadingThread | 3.1, 3.3 | 3.2, 4.4 |
| The data checksum, beside the databases | `0x171cdf0` | a pool thread | 4.0, 4.0 | 13.1, 13.0 |
| The main menu's RestoreDeviceObjects' part `0x206260` | `0x206260` | main | 2.1, 2.3 | 2.4, 2.4 |
| of that, the map's graphics | `0x10a2e40` | main | 1.2, 1.3 | 1.4, 1.4 |
| Then to the main menu's first frame | | main | 0.6, 0.8 | 0.6, 0.6 |
| The main menu | | | 29.8, 30.1 | 36.7, 37.9 |

- The databases are 7.3 s shorter, nearly all of the 7 s the main menu comes sooner; the history about 1.2 s of it, and the data checksum, on its own thread, a third of what it was.
- The LoadingThread's path is now 3.6 s to the gfx, 7 s of gfx and meshes, 8.5 s of databases and 7.5 s of events, flags and the rest of InitGame; the main thread waits for it 23.8 s of the 29.8.
- The glyphs the framework draws before the main menu: about 4,250 in 0.87 to 0.98 s, either way.
- The gfx and meshes took 0.4 to 1.4 s longer with L4 on in these starts; not looked into, and within what other test games' load moved the stretches before.

## The databases with L4

Timed 2026-11-08 in a test game with L4 on, as "The databases" was: the 67 memory brackets and 12 loaders without one, three starts, the databases 9.5, 9.8 and 8.5 s ([the run](../prototype-log/2026-11-08-databases-and-meshes.md)). Each start's brackets ran in the order of `0x208620`'s code but for the history's opening one, which was not in `call-times.tsv`, so the history's time is counted with the dynasty colors before it. Seconds, three starts:

| Databases | Functions | s |
| --- | --- | --- |
| Waiting for the picture checks, after the history | `0x8f0350` | 3.26, 3.31, 2.75 |
| Decisions | `0x4882c0` | 1.44, 1.47, 1.28 |
| `customizable_localization` (the folder's name set at `0x7e510`) | `0x46fab0` | 0.82, 0.83, 0.74 |
| Dynasty colors and the history | `0x5b91f0`, `0x7ea2c0` | 0.78, 0.77, 0.65 |
| Map logic | `0x87b9e0` | 0.67, 0.69, 0.64 |
| Missions | `0x8d5bd0` | 0.51, 0.53, 0.47 |
| The countries' files, before `country_colors` | `0x283d20` and the rest | 0.38, 0.43, 0.34 |
| Estates | `0x728570` to `0x72e4a0` | 0.38, 0.40, 0.35 |
| Technologies, the second phase | `0xb74120` | 0.21, 0.23, 0.19 |
| The other 90 or so | | about 0.7 |

- **The picture checks' wait.** The task the databases start first (`0x8effc0`) checks, on a pool thread, that each picture the modifiers name exists: `0x8d80d0` takes a name from its queue, makes the picture's path (`0x8ef620`) and asks the texture manager (`[0x23494a0]`, its method `+0x30`), which asks PhysFS; a picture that is not there adds "The image for '…' does not exist as a picture!" to a list (`[0x2358128]`). The answers go nowhere else. At the databases' end `0x8f0350` (`modifier.cpp`) sets that no more will come (`[0x2358148]`, under the mutex `[0x23580a0]`), waits for the task (`0x17139c0` on `[0x2358098]`) and writes the list to the log. Timed in another start: the checks' thread made 16,031 lookups (`0x19fad70`) one after another from the databases' start, 11.3 s in all, 0.7 ms each, and finished 4.75 s after the history; the LoadingThread waited all of that in `0x17139c0`, and sampled, ran at none of its ticks.
- **What a lookup costs.** Traced on the checks' thread in a third start (slowed by 6 million traced calls): each lookup checked 139 of the search path's entries (`0x19fb700`, 2,228,309 calls); the zips' stats (`0x1a07950`) were 232 a lookup at 0.5 µs, the tree's lookups (`0x19f9280`) among them; the folders' stats (`0x1a07080`, the folder archive's) 20 a lookup at 29 µs, 77% of the lookup's time. L2 answered about half of them from the listings (131,072 by the end of the startup); the others asked Windows: the user folder and the local mod's folder, which L2 leaves to the game, and the paths the listings have.
- **Running.** Sampled at 100 a second in the same start as the brackets, the LoadingThread ran at 400 ticks of the databases' about 850 (47%): decisions 113, `customizable_localization` 68, the map logic 47, missions 43, the history 17.
- **The other threads' lookups**: in the start that timed them, the LoadingThread made 7,258 lookups, 3.5 s in all, and the main thread 3,601, 1.35 s.

## The meshes

Read 2026-11-08 for whether the meshes could load beside the databases.

- `0x14dc5d0` (`pdxmeshobject.cpp`), at the end of the LoadingThread's first step, walks the graphics object's table of mesh objects (`[graphics+0xf0]`, `[graphics+0xec]` buckets of linked entries) and loads each not loaded yet (`[object+0x44]` and `[object+0x40]` clear) through its method `+0x40`, setting `+0x44`; then it makes the meshes' shaders and vertex declarations through the device and keeps them in globals from `0x2349538` to `0x23496a0`.
- Timed with L4 on: 4.6 and 5.5 s. Sampled, the LoadingThread ran at 139 of about 460 ticks (30%): 39% of those inflating (the zips), 28% opening and looking up files, 9% in the heap, 7% in Direct3D. The meshes' textures are looked up on the pool's threads (`0x15cce80`), whose 97 samples were all in lookups.
- Of the databases, only the graphical unit types (`0x114a7a0`) is about graphics: it reads `gfx/sprite_packs` and `gfx/sprite_packs_order` (`0x114aa10`, `0x114b160`), 0.03 s; nothing found reads the mesh objects. What a mesh's load writes and whether the databases read it: "Which stretches can run side by side".

## The startup with L5

Timed 2026-11-08 with [L5](../plans/loading.md) on, three starts each way, as "The startup with L4" was ([the run](../prototype-log/2026-11-08-listed-folders.md)); seconds, L5 on against left to the game:

| Stretch | Function | L5 on | Left to the game |
| --- | --- | --- | --- |
| `.gfx`, the map, `shortcuts.txt`, `.gui` | `0x202850` | 3.5, 3.4, 3.8 | 6.1, 8.6, 7.6 |
| of that, meshes and particles | `0x14dc5d0` | 1.7, 1.7, 1.9 | 4.0, 6.2, 5.1 |
| InitGame | `0x2043a0` | 11.3, 11.2, 12.5 | 15.4, 21.6, 18.6 |
| of that, the databases | `0x208620` | 4.6, 4.7, 5.1 | 7.4, 12.4, 9.5 |
| of those, the wait for the picture checks | `0x8f0350` | 0.00, 0.00, 0.00 | 1.85, 4.67, 3.03 |
| of that, the events | `0x6f3250` | 3.0, 2.9, 3.3 | 3.0, 3.7, 3.6 |
| The main menu's part `0x206260` | `0x206260` | 2.1, 2.1, 1.9 | 2.2, 2.7, 2.1 |
| The main menu | | 19.6, 19.4, 21.1 | 27.5, 37.0, 32.7 |

- The LoadingThread's path is now about 3.6 s to the gfx, 3.5 s of gfx and meshes, 4.8 s of databases and 6.5 s of events, flags and the rest of InitGame: the events and what follows them are now its longest part.

## The startup after L5, broken down

Timed 2026-11-08 on master with L5 merged, as "The startup with L5" was but with the stretches after the events timed too, and one of the starts sampled on every thread (`cargo xtask profile --threads`, 85 samples a second); two other test games ran beside it ([the run](../prototype-log/2026-11-08-startup-after-l5.md)). Seconds from the framework's bootstrap, the mean of three starts, which reached the main menu at 26 to 27 s from the game's start:

| Stretch | Function | Thread | From, to | s |
| --- | --- | --- | --- | --- |
| `main` up to the engine's Init | | main | 0, 2.6 | 2.6 |
| The engine's Init | `0x14c02b0` | main | 2.6, 2.9 | 0.3 |
| `.gfx`, the map, `shortcuts.txt`, `.gui`, the meshes' before | `0x202850` | LoadingThread | 3.0, 5.3 | 2.2 |
| Meshes and particles | `0x14dc5d0` | LoadingThread | 5.3, 7.5 | 2.2 |
| The databases | `0x208620` | LoadingThread | 7.7, 13.7 | 6.0 |
| The data checksum, beside them | `0x171cdf0` | a pool thread | 7.7, 12.7 | 3.5 to 7.0 |
| The events | `0x6f3250` | LoadingThread | 13.7, 17.6 | 3.9 |
| Flags and loading screens | `0x10b2b70` | LoadingThread | 17.6, 18.2 | 0.6 |
| The trade nodes, the checksum's start, the bookmarks | `0x2043a0` | LoadingThread | 18.2, 18.7 | 0.5 |
| The triggers' optimization profile | `0xb9b4e0` | LoadingThread | 18.7, 19.8 | 1.1 |
| The rest of InitGame (the effects' validation) | `0x2043a0` | LoadingThread | 19.8, 20.6 | 0.8 |
| The main menu's part `0x206260` | `0x206260` | main | 20.6, 25.1 | 4.5 |
| of that, the map's graphics | `0x10a2e40` | main | 20.6, 22.7 | 2.1 |

- **The LoadingThread hardly waits now.** In the sampled start it was found running at 66 to 94 ticks of each second from 7 to 17 s, of the 85 to 100 the sampler took a second, against 25 to 75% after L1 to L3; it waited mostly in the gfx (13 samples from 4 to 5 s). The main thread waits for it from 2.9 to 20.6 s, 17.7 s of the 26.
- **The databases' 6 s**, by the loader the samples were under: the decisions (`0x4882c0`) 30%, `customizable_localization` (`0x46fab0`) 19%, the map logic (`0x87b9e0`) 11%, the missions (`0x8d5bd0`) 11%, the rest below 4% each; 74% of the samples in the game's own code, 17% in `ntdll.dll` (the heap among it).
- **The events' 3.9 s**: 96% of the samples in the script reader (`0x17068b0`) under `0x6f3250`, the events' files read on the one thread.
- **What follows the flags**, 2.4 s: InitGame's own code 30%, `0xb89690` 17% and `0xb89560` 11% (which `0x2043a0` calls before and after the triggers' optimization profile), `0xdd4330` 10%, `0xb980a0` 7%, `0x7e8470` 6%. `0xb9b4e0` itself is called 2.3 million times, recursively.
- **The main menu's part**: of the main thread's samples under `0x206260`, 44% under `0x10af270` → `0xfd58b0` → `0xfd5b70` → `0x159e510`, nearly all of them in the framework's drawing of glyphs with DirectWrite; 39% in the map's graphics (`0x10a2e40`, mostly `0x12db850`). The framework's log counted 4,234 to 4,255 glyphs drawn in 2.0 to 2.6 s, twice what it took in "The startup with L4". From 20 s on, 35% of the main thread's samples waited in the kernel under Direct3D 9 and the graphics driver (`NtWaitForSingleObject`, from `0x16d4db0` and the window's shared surface).
- So the startup's path is now the LoadingThread's 17.6 s from 3.0 to 20.6 s, of which the script reader's databases and events are 9.9 s, then the main thread's 4.5 s, of which the glyphs are about 2 s.

## The startup with B1

Timed 2026-11-09 with [B1](../plans/startup-side-by-side.md) on, the mesh types loaded on a thread of the framework's beside the databases, three starts each way, as "The startup after L5, broken down" was ([the run](../prototype-log/2026-11-09-mesh-types-beside-the-databases.md)); no other test game ran. Seconds, B1 on against left to the game:

| Stretch | Function | B1 on | Left to the game |
| --- | --- | --- | --- |
| The gfx stage | `0x202850` | 2.11, 2.02, 1.64 | 3.69, 3.03, 3.07 |
| of that, the meshes on the LoadingThread | `0x14dc5d0` | 0.20, 0.21, 0.17 | 1.74, 1.53, 1.55 |
| The mesh types | `0x14dc390` | 1.72, 1.92, 1.66, beside | 1.53, 1.35, 1.37 |
| From the gfx stage's start to InitGame's | | 2.21, 2.13, 1.73 | 3.83, 3.12, 3.19 |
| The databases | `0x208620` | 4.95, 4.91, 4.56 | 4.93, 4.20, 4.62 |
| The events | `0x6f3250` | 3.22, 3.05, 2.41 | 3.12, 2.48, 2.52 |
| The main menu's part ends | `0x206260` | 18.88, 19.64, 16.05 | 21.46, 17.91, 18.28 |

- InitGame begins 1.0 to 1.6 s sooner: the meshes' time left on the LoadingThread is their mesh objects and shaders, about 0.2 s. The mesh types take 0.2 to 0.5 s longer beside the databases than alone; the databases' times overlap either way.
- The databases ended after the mesh types in every start, so the LoadingThread never waited for them.
- The stretches after InitGame's start moved by 1 to 2 s from start to start either way, more than B1's gain, so the main menu's own time does not show it in three starts.

## Which stretches can run side by side

Read 2026-11-09 from the game's code, for which of the stretches of "The startup after L5, broken down" could run beside each other instead of one after another: what each reads, writes and shares with the others. Nothing here was tried in the game. The direct calls of every function and the globals each reads and writes were indexed from the exception directory and the code (capstone, over the whole of `.text`); a function reached only through a vtable was followed by hand.

### What every stretch shares

| What | Safe from several threads | How it is known |
| --- | --- | --- |
| Direct3D | yes, each call apart | the device is made by `CreateDeviceEx` with the flags `0x44` (`0x16d911f`), `D3DCREATE_MULTITHREADED` among them; the game reaches it through a table of its own (`0x1fd1150` to `0x1fd1368`, set to `0x16d4a40` to `0x16d8ea0`) |
| PhysFS's opens, lookups and reads | yes | L4 and L5 |
| The log | yes | its mutex (`[0x242b8c8]`) |
| The C runtime's heap | yes | the process heap |
| A token's lookup or making (`0x170d250`, 1,420 call sites) | yes | it holds the spin lock `[0x242b8b8]` (`0x1581820`) |
| A dynamic token's making (`0x170d410`, "Creation of dynamic token", 48 call sites, most in the databases' loaders) | **no** | it takes no lock, looks the name up (`0x6e4880`), pushes into the list at `[0x242b8a0]` and may grow the tokens' table (`0x170da50`), which `0x170d250` grows under the lock: a lookup on another thread can meet the table being remade |
| The timers (`[0x23494a0]`'s method `+8`, `0x15ccc90` → `0x15e4c80`) | **no** | each pushes itself into the list at `[0x23682a0]` and takes itself out on its end, with no lock; made by the map logic (`0x87baa2`), the countries' files (`0x286267`) and the interface, never by the meshes or the map graphics |
| The loading screen (`0x11bb4f0` → `0x10b3600`) | **no** | each call draws a whole frame, BeginScene to Present, so two threads drawing it would interleave their scenes |
| The device's reset (`0x14df050`) | **no** | when TestCooperativeLevel gives `D3DERR_DEVICENOTRESET`, every device object is let go (`0x14ddc70`) and made again (`0x14ddd50`) |
| Globals made on first use (`[0x2456bc0]`, `0x1763260`'s, `0x91670`'s, `0x923f0`'s and the like) | yes, once made | each is made when first null, long before the gfx stage, and only read after |

### The meshes beside the databases: can, on two conditions

`0x14dc5d0` loads the mesh objects of `[graphics+0xf0]` not loaded yet (their method `+0x40`), then the mesh types of `[graphics+0x558]` in `0x14dc390`, each through `0x1589060` (`pdxmeshtype.cpp`: the `.mesh` file's existence, its read by `0x17279f0`, its vertices, normals, tangents and UVs, its vertex and index buffers, its textures through `0x15cf0d0`, its skeleton through `0x15d21c0`), then makes the meshes' and particles' shaders and vertex declarations into the globals from `0x2349538` to `0x23496a0`, and the sprites' vertex declaration `[0x23495b0]` through an object it registers as a device resource (`[0x242bf88]`, vtable `0x1d952c0`, in `[[0x23494b8]+0x518]`). Nearly all its time is `0x14dc390`.

- **What the meshes read**: the mesh types the gfx stage registered, their files, the defines; nothing a database writes. The skeleton's builder does not use the context it is given (`[[0x242bf50]+0x348]+8`); the texture loader's and the mesh types' globals (`[0x242b830]`, `[0x2358a00]`, `[0x2358a30]`, the shaders' cache `[0x242b820]`) are used by graphics code only; `[0x2358a18]` is read. The meshes make no token, dynamic or not, and no timer.
- **What they make, and who reads it**: their objects, buffers, shaders and globals are read by drawing code only. The databases reach the graphics object twice, both unrelated: the loading screen's text (`0x20878b`, `0x11bb4f0`) and the map's size after the map logic (`0x20c3c7`, `0x14dfb30`, which writes `+0x534` to `+0x540`, which the meshes do not read). The one lookup of a mesh type by name the databases can reach (`0x14db870`) is the map's message text (`0x8c07b0` → `0x6e9860`), which returns at once without a player country (`[[0x233fe78]+0x1e00]`), as at startup.
- **The loading screen meanwhile**: its window is made before the gfx stage (`0x2024d2` → `0x11bb850`) and drawn before the meshes too (`0x203821`), so it needs nothing of theirs; the meshes' own functions only make resources (vertex and index buffers), and the loading screen's drawing changes the device's state, which a multithreaded device keeps apart.
- **In the game it crashed** (2026-11-09, [the crashes](../prototype-log/2026-11-09-mesh-types-crash-in-the-map-logic.md)): two of about twelve starts with B1 on, on a busy machine, met an access violation on the LoadingThread in the map logic (`0x87b9e0`), one reading a 16-bit province grid's entry as `0x9200`, while the mesh types' thread read a file through `0x17279f0` and `PHYSFS_read`. So something the two share is missing from what is said here, or a read is not as safe from two threads as the table above says; which was not found.
- **So** (as read before the crashes): the mesh types' and mesh objects' loads can run on a thread of their own beside the databases, if (1) the meshes' end (their shaders, the sprites' declaration, their device resource's registration) stays on the LoadingThread after it has waited for that thread, before anything draws a mesh or a particle (the flags do, through `0x10c7280` → `0xde7480` → `0x14dcd30`), and (2) no device reset runs while that thread runs.

### The events beside the databases: cannot

The events (`0x6f3250`) read each file of `events` with the script reader and hand it to each event's read, which reads its triggers and effects; a trigger resolves its names when it is read. The idea group trigger (`0xcb64f0`) takes the idea groups' database from its getter (`0x850750`, the singleton `[0x242bce0]` that `0x208620` loads), looks the name up (`0x8e9f0`) and logs "Invalid idea group for idea_group trigger" when it is not there. So the events need the databases they name loaded first: they can begin only after the databases.

### The map graphics in the main thread's wait: can after the map logic, on four conditions

`0x10a2e40`, on the main thread in the main menu's part `0x206260`, makes the map's graphics object (`0x12d9b70`) and loads it in `0x12db850`, one part at a time with a loading screen text each (`LOAD_GFX_PDXMAP_BORDERS`, `RIVERS`, `TERRAIN`, `STRAITS`, `LAKES`, `TEXTURES`, `TREES`), then the map's trees and terrain (`0x885d90`, `map.cpp`), `0xa42f00`, `0x12daa60`, its shaders (`bloom`, `downsample`, `restorescene`, `mapname`) and its fonts (`tahoma_60`, through the framework's glyphs).

- **What it reads**: the map object `[0x233fed8]`, which the map logic (`0x87b9e0`, called from `0x20c3ab` within the databases) fills; the map's files (`heightmap.bmp`, the terrain's `.dds`); a define (`[0x233ea04]`); and `[[0x233fe78]+0x2260]+0xb0`, a country's index (the trade nodes' setup indexes the countries with it), which the other code found reading it reads as a game starts (`0x210000`, `0x749500`); who sets it was not found, so that it is the same at startup either way is assumed, not shown.
- **What it makes, and who reads it**: its object `[0x2340078]` is read by code that uses it without a check for null (`0x1448880`, `0x10b6c50`), which so does not run before the main menu in the game as it is; nothing of the startup after it reads what it makes.
- **What it shares**: the particles' globals (`0x2349600` to `0x2349628`), which the meshes' particles make; the loading screen, which it draws itself at each part; the tokens, which it looks up once, at its end (`0x10ae980` → `0x170d250`); the device's reset.
- **So**: it can begin on the main thread once the map logic has run, beside the rest of the databases, the events, the flags and the rest of InitGame, if (1) the meshes have ended first, (2) only one thread draws the loading screen at a time (its texts while the LoadingThread runs not drawn, or drawn by one of the two), (3) no dynamic token is made while it looks up its tokens (its last step after the LoadingThread is done, or the dynamic tokens made under the tokens' lock), and (4) no device reset runs meanwhile. It is 2.1 s of the main menu's part; how much of the main thread's wait it could fill depends on where the map logic falls in the databases.

### The flags beside the events: only with the dynamic tokens locked

The flags (`0x10b2b70`, 0.6 s) read the countries' tags and their flags' files and make textures (D3DX), and draw through the meshes' and particles' drawing (`0x10c7280` → `0xde7480` → `0x14dcd30`); they look tokens up (`0x10c7280` → `0x170ded0` → `0x170d250`). The events look tokens up too (`0x170cd10`), and their effects, read through their vtables, were not followed for dynamic tokens. Nothing the events write was found among what the flags read. So the flags could load beside the events only if the dynamic tokens' making takes the tokens' lock as their lookup does; 0.6 s at most.

### The rest of InitGame after the flags: after the events

The triggers' optimization profile (`0xb9b4e0`, about 1.1 s) and the effects' validation work on every trigger and effect the databases and the events read, and the trade nodes' setup (`0xb67830`) on the countries and provinces: they follow the events.

### What the startup's path could become

Of the LoadingThread's 17.6 s (3.0 to 20.6 s) and the main thread's 4.5 s after it: the meshes beside the databases take about 2.2 s off the LoadingThread; the map graphics in the main thread's wait take up to 2.1 s off the main thread's part, the glyphs drawn earlier (the framework's own, "The startup after L5, broken down") up to 2 s more; the flags beside the events at most 0.6 s. The databases and the events stay one after the other: the databases' loaders in their fixed order, the events after them.

## Not looked into

- Which thread held the open's lock while the LoadingThread waited on it from 10 to 18 s; with L4 the opens to read no longer take it.
- Why the framework's glyphs took twice as long after L5 as with L4.
- What the gfx stage's part before the meshes (`.gfx`, the map, `.gui`) shares with the databases, and whether the databases could begin before it.
- Whether the events' effects make dynamic tokens.
- Vanilla's startup and load, without the framework, on the same machine.
- What `0x1806bf0` keeps of a music file, and whether the game reads every file again when it plays.
- Which of the game's things the dated nodes are, and how long their lists get.
- Where in the modifiers' loading each picture is put in the checks' queue.
