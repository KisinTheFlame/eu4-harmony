# 2026-11-04 What still points at the leftovers (tested by me)

M2 of [leak-finding.md](../leak-finding.md): `memory.leftovers` with `holders`, which `tools/leak-run.ps1` asks after its last round. One process under the game lock, which backed up and restored the player's files; the framework files in the game folder copied aside first. The developer build with the heap census, `eu4.exe` started by the script, the saves England-1 and 英格兰1444_11_11, new games as France, 30 days, four rounds.

- **The run**: every answer right, no crash, each round 37 to 39 s; private bytes 4,338 MB at startup, 4,604 MB after four rounds.
- **The scan**, after the fourth round: 1,065,409 leftovers of the 40 sites looked for, 1,671 MB of the game's data and live blocks read, 1,549 ms with the census locked.
- **What it found**:
  - `0x1529f85` (in `0x1529d20`): 32,181 of its 33,790 leftovers are held by nothing, so they are lost: whatever made them never let them go and keeps no pointer to them. Its buffers, `0x15a2658 < 0x152a00a`, are held by those objects alone (33,790 pointers from `0x1529f85`). Together they leave about 600 KB a generation, the most of any site.
  - The other interface sites (`0x155c294`, `0x10b59fe`, `0x10b6098`, `0x15e7654`, `0x15e8c62`, `0x15afb9a`): every leftover held by live blocks, most often of `0x14ec717 < 0x14e8904`. These are kept by a structure that is still alive, not forgotten.
  - The history's sites in `0x2976f0` (`0x299db7`, `0x299e03`, `0x296fba` and others): nearly all held by nothing.
  - No leftover site was held mostly by the game's data. `0x1917cf9` came closest, 20 of 153.
- **Not tried**: a scan in a game, where the threads run; the threads' stacks, which are not read.
- **Cleanup**: the game was closed by the script. The three framework files in the game folder were put back and compared equal to the copies taken before. Releasing the game restored the player's files (the run's save rewritten, and its backup added, among them) and found every file as the backup had it. The run's report was deleted.
