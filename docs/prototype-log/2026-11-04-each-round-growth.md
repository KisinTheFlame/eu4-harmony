# 2026-11-04 Where each round's growth goes (tested by me)

M1b of [leak-finding.md](../plans/leak-finding.md): the developer build with the memory account alone (`--features eu4-harmony-core/dev-memory`), no census, so that the game ran as a player's build does but for what `memory` reads when asked. One process under the game lock, which backed up and restored the player's files; the framework files in the game folder copied aside first. `tools/leak-run.ps1`, ten rounds, the saves England-1 and 英格兰1444_11_11, new games as France, 30 days.

- **The run**: every answer right, no crash, each round 28 to 35 s (35 to 40 s with the census). Private bytes 3,850 MB at startup, 3,970 MB after the first round, 4,177 MB after the tenth; from round to round between -22 and +56 MB.
- **The heaps**: seven, and only the process's default heap, which the game's C runtime and the framework share, holds more than half a megabyte: 1,855 MB allocated and 2,349 MB committed at the end. The framework's heap stayed at 1.6 to 1.8 MB.
- **Where each round's growth goes**, the least-squares slope over rounds 2 to 10: 17.65 MB a round of private bytes. Of it:
  - 9.21 MB is blocks the game's heap holds for the game, what the census counts;
  - 7.29 MB is free pages that heap commits and does not use, after the framework had the heaps give back what they could at every main menu;
  - 1.15 MB lies outside the heaps;
  - nothing is in the other heaps or the framework's.

  Handles grew 2.6 a round. From the end of round 1 instead of the slope, round 2's jump of the free pages (357 to 434 MB) made them 15.2 MB a round, which is why the report takes the slope.
- **What it says**: leaks of the game's heap are about half of each round and the free pages most of the rest, so finding the leaks (the census) and the free pages (why the default heap keeps them) are both worth the work; Direct3D and the driver are not.
- **Not tried**: the player build itself, which has no `memory`; a run longer than ten rounds.
- **Cleanup**: the game was closed by ending its process. The three framework files in the game folder were put back and compared equal to the copies taken before. Releasing the game restored the player's files (the run's save rewritten, and its backup added, among them) and found every file as the backup had it. The run's report was deleted.
