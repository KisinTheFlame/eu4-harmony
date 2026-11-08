# A shorter startup and load

Status: built (2026-11-05), in one pull request: L1 to L3; the faster inflate set aside by the owner. L4, files opened and looked up side by side, and L5, every folder of the search path answered from its listing, asked for and built on 2026-11-08. L6, the mesh types loaded beside the databases, asked for on 2026-11-09. Where the time went is in [research/loading.md](research/loading.md). The bar is the one the game speed plan meets ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](adr/making-the-game-faster-leaves-what-it-computes-as-it-is.md)): what the game loads stays what it loads, only sooner.

## Goal and scope

- **Goal**: the game reaches its main menu sooner from its start, and a game sooner from a load, with the same data loaded.
- **In scope**: the startup's music, the lookups of files in the game's folders, a save's lists as they are read, the threads that open and look up files waiting for each other, the startup's stretches that can run beside each other.
- **Out of scope**:
  - the databases' own parsing on the LoadingThread, the largest part of the startup, which is the game's script reader on one thread; reading them on several would need each database's order of loading shown not to matter;
  - the pictures' lock;
  - the user folder's lookups: the game writes saves, logs and settings there, and the player may edit a mod in it while the game runs;
  - a start with nothing in the system's file cache, which may wait on the disk: not measured.

## Behavior

- Nothing the player sees changes but how long the startup and a load take.
- The first start after the framework is installed, and the first after a DLC or a mod changes its music, reads that music as the game always did, and keeps its numbers for the next.

## Parts

### L1 Music kept from one start to the next

For each music asset the game reads the whole file, opens it with Vorbis and keeps four numbers in its sound: the channels, the rate, the length in samples and the duration in nanoseconds; the rest it lets go. Of the four, only the duration is read before the song plays: the songs' list, made with the databases, keeps each song's length in minutes and seconds for the music player; a song that plays is opened again as a stream, which takes the four numbers anew (research/loading.md, watched in the game). The DLC's music is 3 GB of zips that every start inflated for those numbers.

- Built (`loading::music`): where the music is added (`0x16a4c62`), the framework fills the four numbers from what it kept, by the place PhysFS reads the file from (its real directory, a zip or a folder) and the file's name, and the game's read whole does not run. What tells the file as it was is, for an entry of a zip, its CRC-32 and its sizes from the zip's directory (the zip's end, read once a start for each zip with music), and for a file in a folder its size and time. For a file it has not kept, or one that changed, the read runs and the numbers are kept, in `eu4-harmony\music.txt` in the user folder.
- Shown: a developer build's test switch `check-loading` has the game read every file too and compares its numbers with the ones kept: the same for the 270 files of the player's playset.

### L2 Files missing from the game's folders, answered from their listings

PhysFS asks each folder of its search path for a file in turn, and Windows each time for a folder (its platform's stat, `0x1a061d0`): the pictures', textures' and scripts' lookups asked the game folder, each mod's folder and the user folder, mostly for files none of them has.

- Built (`loading::lookups`): inside the game folder and the workshop's folder for the game, each folder is listed once, the first time a path in it is asked for; a path whose folder or file the listings lack is answered as Windows answers a missing file (PhysFS's "not found", no stat), and any other goes to the game's stat. Left to the game: the user folder, the framework's own folder, a name Windows reads otherwise than as written (other than ASCII, `~`, `..`, a trailing dot or space), a folder whose listing holds such a name, links.
- Shown: `check-loading` has the game's stat answer every path the listings answer missing too: it found none of them. Unit tests answer paths of a folder made for them.

### L3 A save's dated nodes added without the walk

A node's add (`0x7e7cd0`) walks the node's whole list for the child before it adds it; reading a save, `0xda66a0` makes each child, reads it and adds it, so the walks grow with the list.

- Built (`loading::children`): where `0xda66a0` calls the add (`0xda69e5`), the framework marks the thread; the add, on that thread, clears the mark and goes past the walk, as it does when the walk does not find the child. The child is one `0xda66a0` made itself, in no list yet. Every other add, and the adds the add makes, walk as before.
- Shown: a save written after a load with the framework's adds and one with the game's (`game-loads`), the same load of the same save: the same entries but for what every save changes ([The startup and a load sampled](prototype-log/2026-11-05-startup-load-sampled.md)).

### L4 Files opened and looked up side by side

Asked for by the owner on 2026-11-08, after [research/loading.md](research/loading.md) ("The databases") found the LoadingThread spending about 6.5 s of the databases' 14 s opening files, most of it waiting for the threads that open and look up files beside it: the data checksum's, the picture checks', the pool threads that read the history. Two global locks let one of them in at a time:

- the game's own, a byte spin lock (the global at `0x242b9c0`, taken by `0x1581820`) around every PhysFS open in `0x1718300` and every close;
- PhysFS's (`stateLock`, `[0x2339238]`, a critical section taken by `0x1a05e60` and given back by `0x1a06150`), held through each open's and each lookup's walk of the search path, which asks every folder and zip in turn and may wait on Windows for each folder.

What the walks read nothing changes while they run but a mount, an unmount or the write folder's change, which are rare and stay alone. What they write:

- an open links its new file into the list of open files (`[0x23391f8]`), and a close takes it out;
- a zip's lookup of a name (`0x19f9280`, PhysFS's tree) moves the entry it finds to the front of its bucket, so that the next lookup of it is quicker;
- the first open or stat of an entry of a zip reads the entry's local header through the zip's own file (`0x1a0b420`), which every open of that zip shares, and keeps where its data starts.

So the part makes PhysFS's lock one that many may hold to read, and one alone to change, and makes each of the three writes safe among readers:

- The game's lock is not taken for an open to read (`0x1718300` with its mode 0); an open to write or append, and every close, take it as before.
- An open to read (`PHYSFS_openRead`), a lookup (`0x19fad70`, under `PHYSFS_exists` and `PHYSFS_getRealDir`) and a close (`PHYSFS_close`) hold PhysFS's lock shared; every other PhysFS call that takes it holds it alone, as before. A thread that holds it alone and opens, looks up or closes holds it alone still.
- The list of open files is changed under a small lock of its own: an open's link and a close's walk of the list.
- A lookup in a zip leaves the entries where they are: it finds the same entry, without the move.
- An entry of a zip is read under a lock of its own the first time; once read (or known as a folder) it is answered as the game answers it, without the lock.

The files the game opens, what it reads from them and what each lookup answers stay what they were; only threads that waited for each other's opens and lookups no longer do. Under the ADR the part is of the kind that leaves what the game computes as it is.

- Built (`loading::opens`): where the game's open takes and gives back its lock (`0x1718346`, `0x17183a8`), for a mode other than reading only; PhysFS's grab and release of a mutex (`0x1a05e60`, `0x1a06150`) replaced, its lock held alone through an SRW lock of the framework's as well; where the open, the lookup and the close grab and release it, held shared instead; the open's link and the close's walk under the list's lock; the tree's move to the front left out (`0x19f93a4`, `je` made `jmp`); the entry's read (`0x1a0b420`) under a lock that the thread holding it may take again.
- Shown: three starts each way, the part on and left to the game (`game-paths`), with the machine's processor about 70% busy with other test games: the databases (`0x208620`) took 10.2, 12.1 and 8.3 s against 18.0, 17.3 and 13.9 s, the history 0.7 to 1.0 s against 1.7 to 2.1 s, the data checksum 4.1 to 5.7 s against 11.7 to 16.5 s, and the main menu came 5 to 8 s sooner. An ironman save loaded each way and written by the game after its load: `gamestate` the same but for the 4 bytes at 510 that every save changes, `meta` but for its UUID, `ai` the same ([the run](prototype-log/2026-11-08-opens-side-by-side.md)).

### L5 Every folder of the search path answered from its listing, kept up to date

Asked for by the owner on 2026-11-08, after [research/loading.md](research/loading.md) ("The databases with L4") found a lookup taking 0.7 ms: the 139 entries of the search path checked in turn, and 77% of the time in about 20 stats of folders, each asking Windows. The picture checks make 16,000 such lookups one after another, and the databases wait about 3 s for them at their end; the LoadingThread and the main thread make 11,000 more. A trace of the folders' stats found every folder of the search path among them: the user folder, the game folder, the workshop's mods and a local mod. L2 answers only a path missing from the game folder or the workshop's; a folder a path goes through that is there (`gfx`, `gfx/interface`) still asks Windows, and the user folder and the local mods are left to the game.

- PhysFS asks a folder about a path in two places that read no more of the answer than whether it is there and whether it is a file, a folder or a link: the path's check, which asks for each of its folders in turn and for the path (`0x19fb700`, refusing a link), and the lookup of the entry that has a file (`0x19fad70`, under `PHYSFS_exists` and `PHYSFS_getRealDir`). There, the framework answers a folder entry of the search path from its listings: there, a file or a folder; missing, PhysFS's "not found". Every other stat, `PHYSFS_stat`'s with its sizes and times among them, stays the game's.
- Each folder is listed once, the first time a path in it is asked for, as L2 lists them; a name Windows would read otherwise than as written, a folder whose listing holds such a name, and a link are left to the game.
- A listing is kept only while it holds: the framework watches each folder of the search path for names made, removed or renamed below it (`ReadDirectoryChangesW`, from before its first listing), and lets go of the listings of the folders that changed; a change too many to tell lets go of all the folder's. A file the game makes, removes or a folder it makes through PhysFS (`PHYSFS_openWrite`, `PHYSFS_openAppend`, `PHYSFS_mkdir`, `PHYSFS_delete`) lets go of its folder's listing at once, before the call returns. What the game writes another way, and what another program changes, is let go of when Windows tells the framework, a moment after.

- Built (`loading::listed`): where the path's check and the lookup ask an entry (`0x19fb7e5`, `0x19fae6e`), a folder entry's question is answered from the listings when they can; the folder archive's stat answers the rest. The watch is opened and its first `ReadDirectoryChangesW` made when a folder is first asked about, before its first listing; a listing made across a change Windows told of is not kept. The game's writes are observed at the open to write or append (`0x19fa240`), `PHYSFS_mkdir` and `PHYSFS_delete`.
- Shown: checked (`checked = ["listed-folders"]`), through a start, a save's load and 20 days played, 702,736 answers to the main menu and none differing from the game's stat. Three starts each way, the part on and left to the game, the processor about 45% busy with other test games: the main menu at 19.6, 19.4 and 21.1 s against 27.5, 37.0 and 32.7 s; the databases 4.6 to 5.1 s against 7.4 to 12.4 s, of which the wait for the picture checks 0 against 1.9 to 4.7 s; the meshes 1.7 to 1.9 s against 4.0 to 6.2 s. A lookup took 103 µs on the picture checks' thread (760 µs before), 31 µs on the LoadingThread (480 µs) and 23 µs on the main thread (375 µs). An ironman save loaded each way and written by the game after its load: `gamestate` the same but for the 4 bytes at 510 that every save changes, `meta` but for its UUID and the checksum that ends it, which sums the UUID with the rest, `ai` the same ([the run](prototype-log/2026-11-08-listed-folders.md)).

### L6 The mesh types loaded beside the databases

Asked for by the owner on 2026-11-09, after [research/loading.md](research/loading.md) ("Which stretches can run side by side") found that the meshes can load beside the databases: at the end of the gfx stage the LoadingThread spends about 2.2 s in the meshes (`0x14dc5d0`), nearly all of it in `0x14dc390`, which loads every mesh type (its `.mesh` file, its vertex and index buffers, its textures, its skeleton) and then makes the particles' shaders. Nothing the databases load reads what it makes; what the two use alike (Direct3D, made multithreaded; PhysFS; the log; the heap; the texture lookups) is safe from two threads.

- At the startup's first call, `0x14dc390` runs on a thread of the framework's, and the LoadingThread goes on with the rest of the meshes (the mesh objects before it, the meshes' shaders and the sprites' vertex declaration after it, which neither need it nor are read by it), the rest of the gfx stage and the databases.
- The LoadingThread waits for that thread as the databases end, at the DLC's metadata (`0x2060f0`), which InitGame calls next: the events, the flags and what follows them, the main menu and every drawing of a mesh or a particle come after it, as they do now. A check of the device (`0x16d8e20`) that finds it lost waits for it too, before the game lets every device object go and makes it again.
- A later call, such as after the device's reset, loads the mesh types on its caller's thread, as the game does.
- The mesh types loaded, in the same order, and every other step stay what they were; only the time changes: `setup.log`'s "Loading Animated Map-Sprites" line, which times the meshes from the LoadingThread, counts only what is left on it.

### Set aside

- **The zips' index**, a lookup going past the zips that do not have its path: with L5 the lookups left on the LoadingThread took 0.23 s of a start, and the picture checks end before the databases do, so what it could take off the startup's path is a few tenths of a second.
- **A faster inflate**: PhysFS's `tinfl_decompress`, replaced by miniz_oxide's port of the same function in the game's own decompressor memory, about half again as fast. With the music kept (L1) what is left to inflate at startup is the textures and meshes, a second or two and off the main thread, so the owner set it aside (2026-11-05); zlib-rs, faster still, would need its own memory for each open file and the game's frees to let it go.
- **Loading music only when it plays**: the songs' list needs every song's duration as the databases load, before any plays; L1 gives the same durations without the read, and the play takes the four numbers again as it always did.

## How it fits together

`loading` is a module of the core: `set_folders(game_dir, user_dir)`, which the core calls before the game reads its music, and its six parts, each a speed-up of `crate::speed` (`music`, `lookups`, `children`, `opens`, `listed-folders` and `meshes`, the group `loading`). A developer build's `game-paths` leaves any of them to the game, and `checked` has the game's own answer taken as well and compared, for `music`, `lookups` and `listed-folders` (the switches `game-loads` and `check-loading` before them did both for all three at once).
