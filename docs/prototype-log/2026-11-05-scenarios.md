# 2026-11-05 Scenarios (tested by me)

SC1 to SC3 of [scenarios.md](../scenarios.md), with `cargo xtask scenario` from this branch's checkout, each run within a claim of its own that the runner made, waited for in the queue with `--wait`, and released. The game hidden throughout; the player's own install in the game folder before and after.

- **The list**: `cargo xtask scenario` with no name listed `perf`, `growth` and `leak`, each with its build, its minutes and its parameters with their values.
- **Busy**: with another checkout holding the game, `cargo xtask scenario perf` without `--wait` said the game was busy and that `--wait` waits in the queue, exited with 1 and made no report folder.
- **`perf`**: passed, on the player build, in 4.7 min: 47.2 s to the main menu, `menu.start MNG` 9.7 s, 0.81 cores paused and 1.75 at speed 5, 1444.11.11 to 1447.7.14 in 120.1 s, 487 game days a minute. The first try started the game twice, once for the main menu and once for the scenario's restart; the runner since brings an already running game to the main menu and leaves a game that is not running to the scenario's first step. Two later `perf` runs after another scenario passed too, with 305.6 and 453.3 game days a minute: the figure varies from run to run by more than the framework's setups are likely to differ.
- **`leak`**, two rounds with `watch 2`, `watch_after 1` and `quarantine true`: passed, on the heap census build, in 3.9 min. The report had every section `leak-run` wrote: the rounds, where their growth goes and each heap, the leftovers, what holds them, who let them go, what keeps the free pages (184 MB of pages no block is on), and the quarantine's (nothing written after a repair let it go).
- **Two builds in one run**: `growth perf --growth.rounds 2` built and installed the memory account build, played `growth`, then built and installed the player build and played `perf`, and released once.
- **The game ended from outside**: `eu4.exe` ended with `taskkill /F` while `growth` played.
  - During its first round's `menu.return`: `growth` came out failed, "the pipe broke", and `perf` after it passed. A return with "return without restarting" off ends its process on purpose, as the game starts again, so the scenario since takes a broken pipe in a return, with a game still running, for the return.
  - During the second round's first `menu.load`, twice. The first time `growth` came out failed too: the killed game stayed in the process list longer than the 15 s the runner gave it, so the runner did not see it end. Since then any other step whose pipe breaks counts as the game ending. The second time `growth` came out crashed, "the game ended while answering menu.load", with the round it had played. The report's crash section said that no crash folder came of it, as none does of a kill, and the release said the game did not crash.
- **`--from-report`** wrote `leak`'s `report.md` again from its `report.json`, the same byte for byte.
- **Every release** put every player file back ("every file is as the backup has it") and the game folder's framework files ("as they were").
- **Not tried**:
  - a step taking longer than its time, so hung is not verified: steps have 600 s, and no scenario has a parameter to shorten one;
  - the player taking the game while a scenario plays, since a session never claims as the player, so unfinished is not verified;
  - a real crash with a dump, so the crash section's reading of a dump (the exception and the stack) is not verified;
  - a save the scenario names missing;
  - a run within a claim this checkout already held;
  - the lease renewed by the runner after 20 minutes, since no run here lasted that long; a renewal at each start was seen;
  - `leak` and `growth` with their 10 rounds.
- **Cleanup**: the runs' report folders and logs were deleted after the figures above were taken from them; the queue held only other checkouts' claims.
