# Where startup and a save's load spend their time

Read 2026-11-05 for making the game's startup and its loads shorter. Sampled with `cargo xtask profile` (`--wait --slice 5` from the process's start, `--thread` for threads other than the main one), the player build of the framework installed, the player's playset, every DLC; the machine's processor was 88% busy with other work throughout, so the times are long. Addresses are RVAs into the 1.37.5 baseline.

## Startup

From the process's start to the main menu: 46 to 55 s in five starts. In three stretches:

| Stretch | Time | What runs |
| --- | --- | --- |
| The engine's Init on the main thread | 0 to about 17 s | Sound and music loaded (`0x14c02b0` → `0x15c9c10`, "sound", "music"), most of it inflating DLC zips |
| The databases on the LoadingThread | about 17 to 47 s | `0x200810` → `0x2043a0`, "Loading Databases"; the main thread waits |
| The main menu made | about 47 to 53 s | the main menu's constructor, the first frames |

- **Music, read whole.** For each music asset (`0x15c92c0`, `assetfactory_audio.cpp`, "music/065_music.asset"), `0x16a4930` (`pdx_audio.cpp`) calls `0x16a9a40` (`pdx_audiosound_sdl.cpp`), which opens the file (`0x1718300`), takes its length (`0x19f6ec0`), reads all of it into a vector (`0x19f8150`, PhysFS's read) and hands it to `0x1806bf0`. The DLC's music is 238 Ogg files of 3,133 MB in their zips, deflated (method 8) to 3,092 MB: compressed data deflated again, which every read inflates (`0x19f8180` → `0x1a07a10` → `0x1a080d0` → `0x1a08470`, PhysFS's zip archiver). In the stretch from 5 to 15 s, 74 to 81% of the main thread's samples were in `0x1a08470` and 13 to 16% in `ReadFile` under it.
- **The databases** (`0x2043a0`, between "Used RAM Map before Loading Databases" and "after"): `0x208620`, the databases from `common/` (technologies, trading policies and the rest), 44% of the LoadingThread's samples; `0x6f3250`, the events (`eventmanager.cpp`), 18%; `0xb9b4e0`, the triggers' optimization profile (`trigger.cpp`), 9%; `0x10b2b70`, flags and loading screens (`eu3graphics.cpp`), 6%; `0x202850` → `0x14dc5d0`, meshes and particles (`pdxmeshobject.cpp`), 13% at its start. Of the thread's samples, 59% ran, 23% waited (locks, other threads) and 18% were in file calls. The main thread waits for it in a loop (`0x878050`: messages, `0x169a2f0`, the online services' tick, and `Sleep(10)`) while `[0x242bbcb]` is set.
- **File lookups.** Threads of the game's own pool (`0x1712b40` → `0x8d80d0`, pictures, "The image for '…' does not exist as a picture!"; `0x15cce80`, `pdxassetutil.cpp`, a mesh's textures) and the LoadingThread ask PhysFS whether files exist (`0x19f6ea0` → `0x19fad70` → `0x19fb700` → `0x1a07080`), which asks Windows for each directory in the search path (`0x1a061d0`, `GetFileAttributesExW`, and `0x1a05220`, which converts the file's times with `GetTimeZoneInformation`). The search path holds the game folder, the user folder, the playset's mods and the DLC's 123 zips. In the busiest thread of each second from 20 to 30 s, 55 to 69% of the samples were in `GetFileAttributesExW`; from 30 to 40 s, 45 to 51% in a lock (`0x1a137cc`, `0x1a139f0`) under `0x8d80d0`.
- **The framework's own share**: about 4% of the main thread's samples in the first stretch (its startup, `eu4_harmony_core.dll+0x222630`), 1% of the LoadingThread's; on the stack of more, as the observer of PhysFS's read and of the tokenizer.

## A save's load

Austria-37_4-1 (an ironman save of 1724, a game state of 55 MB) loaded from the main menu with `menu.load`: about 10 s to a frame of the game, the ironman save as it is loaded included (prototype-log.md). Of the main thread's samples over those 10 s:

- **Reading the save** (`0xd84b80` → `0x74b8a0` → `0x5cdc60` → `0x5cf740` → `0x14e39d0`, the world's entries): 37%.
- **`0x7e7cd0`**, 24% of its own: a method that many classes share (it is in vtables from `0x1c4fcd0` on, one each 0xd0 bytes, and called directly at `0xa368eb`), which adds an object to a list at `this+0x20`: it walks the whole list first for the object already there (`0x7e7cf0`, by `[node]`, the next at `[node+0x10]`), then walks again from `this+0x28` for the place by `[object+0x10]`, and links a node of 0x20 bytes there. So adding n objects walks the list n times; here from `0xda66a0` (in the vtable at `0x1d21348`) while the save is read. Which class and list was not looked into.
- **The interface made after the load** (`0x10da520` → `0x803270`, `0x81af10`): sprite types' textures (`0x156e0a0` → `0x16c35b0`, 22%) and, under them and elsewhere, PhysFS's lookups again: 12.6% in `GetFileAttributesExW`.
- **The day's setup** (`0x350b80` → `0x2d2520`, `0x7760b0` → `0x2d0b30`): 8 to 14%.

## Not looked into

- Vanilla's startup and load, without the framework, on the same machine.
- What `0x1806bf0` keeps of a music file, and whether the game reads every file again when it plays.
- Which class's list `0x7e7cd0` builds during a load, and how long it gets.
- The lock the picture tasks wait on.
