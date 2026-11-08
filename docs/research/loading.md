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

## Not looked into

- Which thread holds the open's lock while the LoadingThread waits on it from 10 to 18 s: the checksum's thread opens through it, and its open waits in turn on PhysFS's lock, which the picture checks' lookups take.
- The databases with the checksum not running, or with the picture checks' lookups not taking PhysFS's lock.
- Vanilla's startup and load, without the framework, on the same machine.
- What `0x1806bf0` keeps of a music file, and whether the game reads every file again when it plays.
- Which of the game's things the dated nodes are, and how long their lists get.
- Who fills the picture checks' queue (above), and what each check costs once polled.
