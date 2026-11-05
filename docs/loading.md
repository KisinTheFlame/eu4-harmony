# A shorter startup and load

Status: built (2026-11-05), in one pull request: L1 to L3; the faster inflate set aside by the owner. Where the time went is in [research/loading.md](research/loading.md). The bar is the one the game speed plan meets ([ADR making-the-game-faster-leaves-what-it-computes-as-it-is](adr/making-the-game-faster-leaves-what-it-computes-as-it-is.md)): what the game loads stays what it loads, only sooner.

## Goal and scope

- **Goal**: the game reaches its main menu sooner from its start, and a game sooner from a load, with the same data loaded.
- **In scope**: the startup's music, the lookups of files in the game's folders, a save's lists as they are read.
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

For each music asset the game reads the whole file, opens it with Vorbis and keeps four numbers in its sound: the channels, the rate, the length in samples and the duration in nanoseconds (research/loading.md); the rest it lets go, and when the music plays the game opens the file again as a stream and takes the four numbers again. The DLC's music is 3 GB of zips that every start inflated for those numbers.

- Built (`loading::music`): where the music is added (`0x16a4c62`), the framework fills the four numbers from what it kept, by the file's name and the place PhysFS reads it from (its real directory, a zip or a folder) and that place's size and time, and the game's read whole does not run; for a file it has not kept, or whose place changed, the read runs and the numbers are kept, in `eu4-harmony\music.txt` in the user folder.
- Shown: a developer build's test switch `check-loading` has the game read every file too and compares its numbers with the ones kept: the same for the 270 files of the player's playset.

### L2 Files missing from the game's folders, answered from their listings

PhysFS asks each folder of its search path for a file in turn, and Windows each time for a folder (its platform's stat, `0x1a061d0`): the pictures', textures' and scripts' lookups asked the game folder, each mod's folder and the user folder, mostly for files none of them has.

- Built (`loading::lookups`): inside the game folder and the workshop's folder for the game, each folder is listed once, the first time a path in it is asked for; a path whose folder or file the listings lack is answered as Windows answers a missing file (PhysFS's "not found", no stat), and any other goes to the game's stat. Left to the game: the user folder, the framework's own folder, a name Windows reads otherwise than as written (other than ASCII, `~`, `..`, a trailing dot or space), a folder whose listing holds such a name, links.
- Shown: `check-loading` has the game's stat answer every path the listings answer missing too: it found none of them. Unit tests answer paths of a folder made for them.

### L3 A save's dated nodes added without the walk

A node's add (`0x7e7cd0`) walks the node's whole list for the child before it adds it; reading a save, `0xda66a0` makes each child, reads it and adds it, so the walks grow with the list.

- Built (`loading::children`): where `0xda66a0` calls the add (`0xda69e5`), the framework marks the thread; the add, on that thread, clears the mark and goes past the walk, as it does when the walk does not find the child. The child is one `0xda66a0` made itself, in no list yet. Every other add, and the adds the add makes, walk as before.
- Shown: a save written after a load with the framework's adds and one with the game's (`game-loads`), the same load of the same save: the same entries but for what every save changes (prototype-log.md).

### Set aside

- **A faster inflate**: PhysFS's `tinfl_decompress`, replaced by miniz_oxide's port of the same function in the game's own decompressor memory, about half again as fast. With the music kept (L1) what is left to inflate at startup is the textures and meshes, a second or two and off the main thread, so the owner set it aside (2026-11-05); zlib-rs, faster still, would need its own memory for each open file and the game's frees to let it go.
- **Loading music only when it plays**: not needed: the game takes the four numbers again when it plays, and L1 gives them without the read.

## How it fits together

`loading` is a module of the core: its points, and `set_folders(game_dir, user_dir)`, which the core calls before the game reads its music. A developer build's `game-loads` leaves all three to the game, and `check-loading` has the game's own answer taken as well and compared.
