# 2026-11-08 Every folder answered from its watched listing (tested by me)

[L5](../plans/loading.md) on a test game: a developer build of this branch with the test games' pipes, the test switches and `dev-call-trace`, the player's playset (five workshop mods and the local Regional Combat), every DLC, the music kept by a first start; other test games held the processor at about 45%.

- **Before it, the folders' stats by folder**: two starts with the folder archive's stat (`0x1a07080`) traced, the second reading 8 bytes of each root's path (`fields`), with another test game's write watch and the player's game running beside them (134 and 156 s to the main menu, so the times are long): the user folder 86,923 stats at 247 µs, the game folder 127,961 at 83 µs, the five workshop mods 343,444 at 4 to 83 µs, the local mod 85,483 at 71 µs; called from the path's check (`0x19fb7f8`) and the lookup (`0x19fae80`), and a few from `PHYSFS_enumerate` and `PHYSFS_mkdir`.
- **Checked**: `checked = ["listed-folders"]`, a start, Austria-37_4-1 loaded with `menu.load` and 20 days played: the report counted 702,736 answers to the main menu, none differing; the log named no difference after; every root was watched.
- **Timed**: six starts alternating the part on and `game-paths = ["listed-folders"]`: 19.6, 19.4, 21.1 s against 27.5, 37.0, 32.7 s to the main menu; the stretches are in [research/loading.md](../research/loading.md), "The startup with L5".
- **Lookups after it**: a start with `0x19fad70` and `0x8d80d0` traced: the picture checks' thread 16,030 lookups at 103 µs, the LoadingThread 7,258 at 31 µs, the main thread 3,561 at 23 µs.
- **The same data**: Austria-37_4-1 loaded each way, and the two saves the game wrote compared: `gamestate` (55,051,381 bytes) differed in the 4 bytes at 510; `meta` in its UUID (1029 to 1064) and its closing checksum (2203 to 2234), which sums the meta with its UUID; `ai` not at all.
- **Played**: with the part on, a new game in observe mode run 60 days and returned to the main menu.
- The unit tests make, list and change folders, and one watches a folder for a file made in it.
- **Not verified**: a change made by another program while the game runs, seen by the game; the machine otherwise idle; a user folder in a synced Documents folder.
- **Cleanup**: `dev.toml` removed; the release kept the logs and said no game crashed; the call times and saves deleted from the scratchpad.
