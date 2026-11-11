# A save read for a transfer, compressed for no one in single player

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-11 for a candidate speed-up, built the same day as S11 of [game-speed.md](../plans/game-speed.md) (`loading::transfer`): when the single player setup reads the save the player chose, it also deflates the whole file at level 9 for a multiplayer file transfer, and in single player nothing ever reads that copy. Read from the code with `cargo xtask disasm` and a script that maps direct calls to their functions through `.pdata`; timed in the game the same day ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)).

In short: **`0x1132ca0`**, the single player setup's read of the chosen save ([main-menu.md](main-menu.md), "The single player setup"), hands the file to the game's file transfer manager (`filetransfer.cpp`) whenever the game's session has kind 1 or 2, which a single player game's has. **The transfer's add `0xd7aad0` deflates a file that does not start with `PK` at level 9** into a copy of its own, on the main thread, before the setup posts the command that loads the save. The load uses the file as it was read; the deflated copy is read only by the transfer's send, which a peer's request starts, and a single player game has no peer. **Timed in the game, the 69 MB plain text save of 1780.7.1 chosen took 2.49 s on the main thread, 2.47 s of it the transfer's add and 2.46 s its deflate**, against 3.10 s for the load that follows ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)). Plain saves are common: autosaves are plain unless `compress_autosave` is set (the player's settings have it off, and the continue box most often continues an autosave), and every one of the late game test saves is plain text.

## The read of the chosen save

`0x1132ca0(setup, null)` (its other callers pass a file description in `rdx` instead, not followed) reads the save named at `setup+0x338` with PhysFS into a byte vector (`0x1718300` to open, `0x19f8150` to read, `0x416ef0` to size the vector). It looks at the file's first two bytes (`0x11330a2`): when they are `PK`, it sets `r12b` (the file is a zip already).

Then it asks the main menu for the game's session (`setup+0x30`, method `+0x80`) and branches on the session's bytes `+0x54` and `+0x55`, which its kind sets (kind 1 sets `+0x54`, kind 2 `+0x55`; [recruiting.md](recruiting.md), "The session"):

- **Both clear** (`0x113371f`): it loads the save at once, `0x110d760(setup+0x58, path, bytes, 0)`; no transfer.
- **Either set** (`0x1133744`): it makes the file transfer manager if the setup has none (`setup+0x2d48`; `0xd7a910`, below), adds the file to it, `0xd7aad0(manager, path, &bytes, compress)` with `compress` = not `r12b` (`0x11337af`), and posts a command (`0x14ca2d0`; vtable `0x1c4eb88`, its type `0x37bd` at `+0x44`, the transfer's index at `+0x50`) to the session.

A single player game's session is of kind 2 (`+0x55` set, read in the game; recruiting.md), and the main menu's research saw a save chosen in the setup load 1.3 to 1.8 s after `0x1132ca0` returned, by that command (main-menu.md). So in single player the second branch is taken, and a plain save is deflated.

Who calls `0x1132ca0` with no description: `0x1130f40` (choosing a save: from the continue box `0x10ef410` at `0x10ef7ee`, and from `0x1130bb0` at `0x1130f0e`) and `0x1132310` (from `0x11316a0`, which posts the same command `0x280e` as `0x1130f40` does). `0x1130bb0` and `0x11316a0` are _inferred_ to be the setup's save list, not read. So every save chosen in the setup's list, and every continue, goes through it; a save clicked in the list is read, and deflated, before the player starts it.

**Not through it**: the in-game load (`0x5d0580`), which reads a local save through a file's stream ([saves.md](saves.md)), and the multiplayer host's start (`0xd84490`, from `0x11376b0`), which adds its file with `compress` 0 (`0xd84815`, `xor r9d, r9d`).

## The transfer's add

`0xd7aad0(manager, name, data, compress)`, four arguments (`args:`): `data` points to a shared pointer to the byte vector, which the add takes over.

1. It copies the name, and takes the save checksum of the bytes with a memory stream's sum (`0x171f3f0`, as [saves.md](saves.md) describes; 44 ms for 55 MB there), kept for the send.
2. **With `compress`** (`0xd7ac6c`): it logs "Compressing save for transfer" (`0x17122a0`, category `0x2000`; not in the player's `game.log`, so that category is off by default), makes a zip writer (`0x153c9b0`) over the application's 50 MB buffer (`[0x242bf50]+0x330`, or one of its own of the same size when the application has none), and writes the bytes into it as one entry named `data` (`0x153cfc0`). `0x153cfc0` sets method 8 and level 9 and runs two tasks of the engine's task scheduler and waits for both: one writes the entry with the zip entry writer `0x153b9a0` (the same one a compressed save is written with; its deflate `0x153c790` and its CRC a byte at a time are in [saves.md](saves.md)); the other, until the first is done, calls a method (`+0x40`) of an object it got from the setup's load interface (`setup+0x58`, slot `+8`, `0x114133c`) and sleeps, _inferred_ to keep a network connection served meanwhile. So the main thread waits for the whole deflate. It then logs "Compression Done, Original: %u, %u" with the two sizes.
3. It appends a file description of 0x48 bytes to the manager's vector (`manager+0x28`..`+0x30`): `+0` the name (`std::string`), `+0x20` the bytes as read (a shared pointer), `+0x30` the deflated copy (a shared pointer; empty without `compress`), `+0x40` the checksum; and returns its index.

The manager (`0xd7a910`, 0x58 bytes, vtable `0x1d1e9b0`) keeps its load interface at `+0x20` (`setup+0x58`, vtable `0x1d5e300`) and registers itself as the one manager, `[0x2340028]`, which the setter of `setup+0x2d48` (`0xad6ee0`) and the manager's destructors (`0xad6f50`, `0xad6fb0`) clear. The descriptions, the deflated copy among them, are kept until the manager goes.

## Who reads the deflated copy

- **The load**: the command `0x37bd`'s execute (`0x279310`, its vtable's `+0x48`) asks the session again. With `+0x54` or `+0x55` set, it takes description `[command+0x50]` from `[0x2340028]` and calls the load interface's slot `+0` with the name and **the bytes as read** (`+0x20`) and 0 (`0x279435`..`0x279459`): that slot is `0x110d760`, the same load as the first branch. With both clear, it posts a command `0x37be` (vtable `0x1c4ec30`) with the same index instead.
- **The send**: `0x37be`'s execute (`0x2794d0`) calls `0xd7b2a0(manager, index, [command+0x38])`, the command's sender, which makes a network message (`0x1528950`, type `0x1a`) and writes into it the deflated copy when there is one (`+0x30`) and the bytes as read otherwise (`0xd7b36f`), with the checksum (`+0x40`). "File Transfer Error: %s" is its failure.

So the deflated copy is read only when some session with neither byte set executes the load command and asks for the file, which is a peer's in a multiplayer game. In single player the one session has `+0x55` set, the load command loads the bytes as read, and nothing posts `0x37be`.

## What a faster way would rest on

- **Leaving the deflate out in single player**: at `0x11337c1`, call the add with `compress` 0 when the game is single player (`world+0x747c` 0, [main-menu.md](main-menu.md)); the description then has no deflated copy, and the load is the same call with the same bytes. A hosted game keeps the game's way. The checksum stays (it is taken before the branch). Under [ADR making-the-game-faster-leaves-what-it-computes-as-it-is](../adr/2026-11-04-making-the-game-faster-leaves-what-it-computes-as-it-is.md) it is of the kind outside the game state: the copy is no game state, and nothing reads it.
- **What it gives**: the deflate of every plain save chosen or continued in single player, on the main thread before the load command can run: 2.46 s for the 1780.7.1 save in the game ("Timed in the game"). Estimated beforehand from the late game test saves: `observer-1780.7.1.eu4`, 69 MB of plain text, deflated raw at level 9 with memory level 9 by zlib 1.3.1 in 2.45 s (7.8 MB out), and at level 3 in 0.37 s, on this machine with nothing else measured beside it. The game's zlib 1.2.3 and its byte-at-a-time CRC are not faster. A zipped save (`PK`) gives nothing, nor does the in-game load. It also frees the deflated copy's memory (about 8 MB for that save) for as long as the manager lives.
- **Risks**: a single player game made a hosted one from the same setup without choosing the save again would have no deflated copy to send; the send then sends the bytes as read (`0xd7b36f`), which is the path a description without `compress` takes in the game already (`0xd84490`'s). Whether the game can do that at all was not found.

## Not found

- What `+0x54` and `+0x55` mean beyond the session's kind, and which kind a multiplayer host and a client have; the claim that only a peer's session takes the `0x37be` branch rests on the code above and on the single player session's kind, not on a multiplayer game.
- Whether other code reads the manager's descriptions: the global `[0x2340028]` is read by `0x279405` and `0x2794d3` and written by `0xd7a910` and the three that clear it; `cargo xtask disasm refs:` finds only functions in the exception directory, so a leaf function reading it would be missed.
- How often the setup reads a save in a usual session (each click in the save list is one).

## Timed in the game

With `dev-call-trace`'s `times` on, the 1780.7.1 save chosen with `menu.load`, which reads it as the continue box does ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)), each once on the main thread:

| Function | What | Time |
| --- | --- | --- |
| `0x1132ca0` (2 arguments) | the setup's whole read of the chosen save | 2.490 s |
| `0xd7aad0` (4) | the transfer's add | 2.468 s |
| `0x153cfc0` (4) | its deflate, waited for | 2.455 s |
| `0x110d760` (4) | the load itself, at a later frame | 3.101 s |

So the deflate is 99% of the read and 44% of the chosen save's read and load together; the add's own sum and copies are 13 ms. Leaving it out in single player takes about 2.5 s off every late plain save chosen or continued. Not timed: an autosave from the continue box, a zipped save (which the add does not deflate), a machine with nothing else running (another test game ran beside this one).
