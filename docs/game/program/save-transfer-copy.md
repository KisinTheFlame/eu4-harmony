# A save read for a transfer, compressed for no one in single player

## The read of the chosen save

`0x1132ca0(setup, null)` (its other callers pass a file description in `rdx` instead, not followed) reads the save named at `setup+0x338` with PhysFS into a byte vector (`0x1718300` to open, `0x19f8150` to read, `0x416ef0` to size the vector). It looks at the file's first two bytes (`0x11330a2`): when they are `PK`, it sets `r12b` (the file is a zip already).

Then it asks the main menu for the game's session (`setup+0x30`, method `+0x80`) and branches on the session's bytes `+0x54` and `+0x55`, which its kind sets (kind 1 sets `+0x54`, kind 2 `+0x55`; [recruiting.md](../military/recruiting.md), "The session"):

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

## Not found

- What `+0x54` and `+0x55` mean beyond the session's kind, and which kind a multiplayer host and a client have; the claim that only a peer's session takes the `0x37be` branch rests on the code above and on the single player session's kind, not on a multiplayer game.
- Whether other code reads the manager's descriptions: the global `[0x2340028]` is read by `0x279405` and `0x2794d3` and written by `0xd7a910` and the three that clear it; `cargo xtask disasm refs:` finds only functions in the exception directory, so a leaf function reading it would be missed.
- How often the setup reads a save in a usual session (each click in the save list is one).


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x11337c1` | code | Referenced while examining what a faster way would rest on; complete role not established |
