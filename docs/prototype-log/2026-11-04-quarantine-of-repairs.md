# 2026-11-04 The quarantine of what the repairs let go (tested by me)

M4 of [leak-finding.md](../leak-finding.md): the developer build with the census (`--features eu4-harmony-core/dev-heap-census`), the quarantine turned on by `memory.quarantine` at the first main menu. One process under the game lock, which backed up and restored the player's files; the framework files in the game folder copied aside first. `tools/leak-run.ps1 -Rounds 10 -Quarantine`, the saves England-1 and 英格兰1444_11_11, new games as France, 30 days.

- **The run**: every answer right, no crash, no crash folder; each round 50 to 61 s, against 48 to 57 s without the quarantine.
- **What it kept**: 1,504,740 blocks, 193.7 MB, about 150,000 blocks and 19 MB a round, from every repair that lets memory go while it runs:

  | Repair | Blocks | MB |
  | --- | --- | --- |
  | rebel faction | 617,343 | 39.3 |
  | transparency mask | 5,995 | 39.2 |
  | agent of a country's AI | 52,963 | 28.5 |
  | set of modifiers | 205,306 | 28.0 |
  | country's array of bytes | 41,400 | 22.0 |
  | text box's animation | 388,445 | 21.2 |
  | mercenary company's leader | 183,210 | 14.5 |
  | session's interface | 6,000 | 0.6 |
  | message window's popup | 2,384 | 0.3 |
  | outliner's entry | 1,694 | 0.2 |

  The text boxes' count holds what their whole destructors free, their strings among them, not only the animations.
- **What was written to**: nothing, in 258 comparisons, the last as it was turned off; and no call went through a block kept, which would have faulted on the pattern. Turning it off freed every block kept in 181 ms, and the game went on.
- **Not verified**: a write found in the running game, which only the unit test makes; reads of a block kept that do not fault, which the pattern does not show; what the GUI's sweep frees of the windows the repairs kill (the map modes' window, the bar of the units selected), which is not kept.
- **Cleanup**: the game was closed by the script. The three framework files in the game folder were put back and compared equal to the copies taken before. Releasing the game restored the player's files (the run's save rewritten, and its backup added, among them) and found every file as the backup had it. The run's report was deleted.
