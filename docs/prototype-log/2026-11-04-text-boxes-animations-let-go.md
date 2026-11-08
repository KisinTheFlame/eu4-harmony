# 2026-11-04 The text boxes' animations let go (tested by me)

The repair of [research/interface-bindings.md](../research/interface-bindings.md#text): `0x152aab0` is a text box's deleting destructor, and `0x1529f85` its animation, which only an element's list of animations ever deletes. Two runs in one claim of the game, which backed up and restored the player's files; the framework files in the game folder copied aside first. `tools/leak-run.ps1`, the saves England-1 and 英格兰1444_11_11, new games as France, 30 days.

- **With the census** (`dev-heap-census`), seven rounds, against the seven of "Who let the leftovers' owners go" above:
  - every world reset handed 1,668 to 3,725 text boxes' animations to their elements; the animations' deleting destructor (`0x15a3746` in `0x15a3720`) let 62,925 of them go;
  - `0x1529f85` left 20 KB a generation, 2,532 blocks, against 488 KB and 61,890; the 2,532 are held by text boxes still alive, of `0x15e7654`, which are leftovers of their own (held by live blocks, "What still points at the leftovers");
  - the animations' buffers (`0x15a2658 < 0x152a00a`, 112 KB a generation) were among no site's leftovers;
  - the leftovers at the end: 5.71 million blocks, 1,222.6 MB, against 5.89 million and 1,233.7 MB.
- **Without the census** (`dev-memory`), twenty rounds: every answer right, no crash, no crash folder, each round 31 to 45 s. Over rounds 2 to 10, as M1b's run measured before the repair ("Where each round's growth goes"): the game's heap held 7.45 MB a round more, against 9.21; the private bytes grew 15.54 MB a round, against 17.65; the free pages in the heap 12.0 MB, against 7.3, and outside the heaps -3.9 MB, against 1.2, which go up and down from run to run. Over rounds 2 to 20: 14.09 MB of private bytes a round, 7.33 MB of it the game's heap and 6.88 MB free pages; handles 0.2 a round.
- **Not verified**: the player build itself, which has no `memory`; a text box let go while its animation plays, which the element deleted before the repair too, is not told apart in the counts.
- **Cleanup**: the game was closed by the script. The three framework files in the game folder were put back and compared equal to the copies taken before. Releasing the game restored the player's files (the run's save rewritten, and its backup added, among them) and found every file as the backup had it. The runs' reports were deleted.
