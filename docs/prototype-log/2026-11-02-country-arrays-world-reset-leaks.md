# 2026-11-02 The country arrays a world reset leaks (tested by me)

The framework now frees, as a country's destructor begins, the array the destructor leaves (research/main-menu.md). One process, built with `trace`, `EU4HARMONY_CENSUS` set; backups as AGENTS.md says, the framework files copied aside first.

- **The leak**: France started, then six in-game loads of `英格兰1444_11_11`: each load left about 26,050 blocks, 3.6 to 3.7 MB, that no later reset let go, against 27,450 blocks and 4.3 MB in the run above; `0x283d8d` was in no generation's leftovers. Nothing was logged about an array not freed.
- **Leaving still works**: after the loads, two returns, a new game, a load of `Castilla-5` from the main menu and a return from it, in the same process, no crash.
- **The leaders**: the world's vector at `+0x18` held no leaders in these sessions. Not fixed; where they come from is in the next entry.
- **Cleanup**: `-Restore` listed what the run changed (the framework's logs, `pdx_login_cache.txt`, `settings.txt`, `shortcuts.txt`, `Castilla-5.eu4` and its backup), put each back and found every file as the backup had it. The framework files in the game folder were put back and compared equal.
