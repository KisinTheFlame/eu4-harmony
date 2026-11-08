# 2026-10-27/28 Unicode module and framework UI test (run by me)

The same playset as the M0 test (the Chinese language mod, regional-combat and others). The framework enters through `version.dll`; the `version.dll` that was there before is moved to `eu4-harmony\backup\` and not loaded.

- **Startup**: every dependency point checks out; the last run had 198 dependency points, 83 hooks and 2 writes.
- **Main menu**:
  - Chinese is correct, and a save name in legacy escapes (Ethiopia) displays too;
  - the notice window (`EU4HARMONY_TEST_NOTICE=1`) and the corner button follow the game's UI scale;
  - the corner button goes away on the country selection screen and comes back on the front page. This is decided by the main menu's page number (`+0x900`); at first I used the visible flag of the `mainmenu` child window, but it does not change when the page changes, so it cannot be used.
- **Two graphics driver failures**:
  - What happened: once a 0x133 blue screen, once a 0x141 GPU timeout followed by a hang. Both happened while the notice window was showing, while the machine was also running a training job that kept the GPU fully loaded.
  - The problem fixed: the loading screen is drawn and presented by another thread, and the framework UI drew on that thread, interleaving with the main thread's operations on the same device (the device is created MULTITHREADED, which locks each call but not a sequence of calls). Now it draws only in frames the main thread presents. In many runs since, there have been no more failures.
- **Map names**:
  - What happened: neither country names nor province names showed.
  - Investigation: I removed the framework's groups of hooks by bisection; in the end they did not show even with no framework installed at all, on vanilla with this set of mods. Reading the font state at run time showed that the map font page `zh-hans-map` had a size of 0×0.
  - Cause: texture loading skips files over 16 MiB at 0x16c2cb7, and this page is a 49 MB DXT5.
  - Fix: the limit is now 256 MiB. Since then country, province and sea names all show as correct Chinese.
- **Crash on loading a save**: loading an August save with `-continuelastsave` crashed the game (eu4+0x88a1fb, an out-of-bounds array read). It crashes the same way without the framework: the save is incompatible with the current set of mods, which has nothing to do with the framework.
- **A save rewritten**: when `-continuelastsave` loaded `Castilla-5.eu4`, the game saved it again when play began, along with its `_Backup`. The original file could not be recovered; no progress was lost, and the names are now stored as UTF-8. From now on, back up the whole save folder before loading a user's save.
