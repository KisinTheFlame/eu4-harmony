# 2026-11-04 What keeps the heap's free pages committed (tested by me)

M5 of [leak-finding.md](../leak-finding.md): the developer build with the census (`--features eu4-harmony-core/dev-heap-census`), and `memory.pages` asked once, at the main menu after the last round. One process under the game lock, which backed up and restored the player's files; the framework files in the game folder copied aside first. `tools/leak-run.ps1 -Rounds 7`, the saves England-1 and 英格兰1444_11_11, new games as France, 30 days.

- **The run**: every answer right, no crash. The walk of the game's heap took 1.9 s, every entry read.
- **The heap's pages**: 2,950 MB committed, 2,565 MB of them used by busy blocks with the heap's overhead; 258 MB of pages no block is on; 127 MB free on the pages blocks are on, 107 MB of it kept by the census's leftovers. Of the 689,000 pages blocks are on, 643,000 are more than seven eighths full and 16,600 at most an eighth.
- **The pages no block is on** were walked 10 s after the framework had the heaps give back their free pages (121 MB then): most of the free bytes the heap keeps are in whole free pages, not on pages leftovers keep.
- **What keeps the most** of the free bytes on pages in use:
  - `0xcc2a3` (in `0xcc220`): 541,285 blocks, 14.5 MB, keeping 38.9 MB, nearly all of them leftovers;
  - `0x5e4e4d` (in `0x5dec50`): 89,701 blocks, 12.6 MB, keeping 11.5 MB;
  - not counted (the framework's, the census's own among them, and other modules'): 126,644 blocks, 824 MB, keeping 8.2 MB;
  - `0xbb8dcc` 5.6 MB, `0x8f163` 3.4 MB, `0x7e7db7` 2.4 MB, `0x1cecf8` 2.1 MB; the rest under 2 MB each.
- **Not verified**: the walk counts 2,950 MB committed where the heap's summary (`memory`'s figures, allocated and free) gives about 2,806 MB, so up to 144 MB of what it takes for pages no block is on may be the heap's own: its segments' headers, or the low-fragmentation heap's subsegments, whose free blocks the walk does not list. Why the heap keeps the pages no block is on after giving back its free pages, and whether they are the low-fragmentation heap's, was not looked into.
- **Cleanup**: the game was closed by the script. The three framework files in the game folder were put back and compared equal to the copies taken before. Releasing the game restored the player's files (the run's save rewritten, and its backup added, among them) and found every file as the backup had it. The run's report was deleted.
