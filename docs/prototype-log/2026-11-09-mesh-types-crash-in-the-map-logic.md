# 2026-11-09 The mesh types beside the databases crash the map logic (tested by me)

Starts of a test game while trying the map names drawn from distance fields, on a branch of their own: a developer build with the test games' pipes, of that branch with master at `967dd5bd` merged, so with B1 of [plans/startup-side-by-side.md](../plans/startup-side-by-side.md) on; the player's playset, every DLC; the player's own game running beside it and the machine otherwise busy (line breaks took 2 to 20 µs each in the framework's costs, against 1.6 µs in the player's game earlier that night).

- **Two crashes in about twelve starts**, both access violations on the LoadingThread in the map logic (`0x87b9e0`, which the databases call at `0x20c3ab`), seven seconds into the start:
  - `eu4_20261109_040115`: at `0xa4197d` (in `0xa41570`, from `0x87ce85`), `test byte ptr [rcx+0x228], 8` with `rcx` an entry `0x230` bytes apart from `[rdi+0x6c0]` indexed by a 16-bit value of the grid at `[rdi+0x10]` (width `[rdi+0x28]`, height `[rdi+0x2c]`) that read `0x9200`, negative, so far outside the table.
  - `eu4_20261109_041542`: at `0x888fe3` (from `0x87c3a3`), `mov [rax+0x154], edx`.
- **The meshes' thread** was in the first dump inside a file read: `0x158944c` (a mesh type's load) → `0x1727adb` (the game's read of a whole file, `0x17279f0`) → `PHYSFS_read` → the framework's `read_bytes_detour` → `PHYSFS_readBytes`. The second dump was not looked at for it.
- **Not shown**: what the two threads share that the map logic read wrong; whether the crashes need the busy machine; whether master alone crashes (its build started twice that night, with no crash). The map names' code of the branch runs when the fonts load and when the map's shader is read, neither in those stretches.
- So B1 was taken out of master the same day, until it is known.
- **Kept**: the dumps and logs in the claim's `kept\` folder, for seven days.
