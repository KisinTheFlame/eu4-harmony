# 2026-11-04 Who let the leftovers' owners go (tested by me)

M3 of [leak-finding.md](../plans/leak-finding.md): the developer build with the census (`--features eu4-harmony-core/dev-heap-census`). One process under the game lock, which backed up and restored the player's files; the framework files in the game folder copied aside first. `tools/leak-run.ps1 -Rounds 7 -Watch 4 -WatchAfter 3`, the saves England-1 and 英格兰1444_11_11, new games as France, 30 days.

- **The run**: every answer right, no crash. After round 2 the run picked `0x1529f85`, `0x15a2658 < 0x152a00a`, `0x152a122` and `0xda635b` and watched them from round 3, 62,575 live blocks of them. Rounds took 49 to 54 s before watching and 49 to 58 s while watching; the framework's heap, the census's own memory, grew from 536 to 553 MB.
- **`0x1529f85`** (in `0x1529d20`, about 488 KB a generation, never let go): 58,842 of its 61,890 leftovers held by nothing at the end. Of those, the last owner let go was:
  - for 29,864, a block of `0x15e7654` (in `0x15e7600`), let go at `0x152aad6` in `0x152aab0`, the code that lets go every block of that site (62,920 in the run);
  - for 2,971, 2,946, 1,679 and 465, blocks of `0x7d34e6`, `0x8ea526`, `0x114cb73` and `0xc9eba`, let go in `0x7d33b0`, `0x94370`, `0x114c730` and `0x114c950`;
  - for 17,010, none: lost in rounds 1 and 2, before watching began.

  So about half of the site's blocks are lost when a `0x15e7654` object goes: `0x152aab0` lets the object go and not the `0x1529f85` block it points at. The rest were lost through owners whose part is smaller, as some other pointers to these blocks are.
- **`0xda635b`** (in `0xda5820`): 4,994 of 5,022 held by nothing; for 2,311 the last owner was a block of `0xda6537`, from the same function, let go at `0x4e2046` in `0x4e2020`.
- **`0x15a2658 < 0x152a00a` and `0x152a122`**: none held by nothing, so no owners to give; the first are held by `0x1529f85`'s blocks only, and go with them.
- **Beside it**: the free pages grew 13.1 MB a round with the census against 7.3 MB without (M1b's run); the census changes how the heap is used, which is why M1b measures without it.
- **Not verified**: that `0x152aab0` is the object's destructor and that it should let the `0x1529f85` block go, which is for reading the code; the owners lost before watching began.
- **Cleanup**: the game was closed by the script. The three framework files in the game folder were put back and compared equal to the copies taken before. Releasing the game restored the player's files (the run's save rewritten, and its backup added, among them) and found every file as the backup had it. The run's report was deleted.
