# 2026-10-27 First test (running eu4.exe directly)

| Item | Result | Evidence |
| --- | --- | --- |
| 1 When the proxy loads | Pass | The core loads inside `DllMain` on the main thread and installs 9 PhysFS hooks before the game first calls `PHYSFS_init`. Chain-loading not tested: the root folder had no `winmm.dll` to begin with. |
| 2 Which thread loads the GUI | **Not the main thread** | Every `.gui` is read on thread 56780; the main thread is 46896 (56780 is the LoadingThread the research points to). |
| 3 Path format | Pass | `interface/unitpanel_main.gui`: lower case, forward slashes, no leading directory; all 130 `.gui` files go through `PHYSFS_openRead`. |
| 4 Coexisting with mods that override the same file | Partial | `getRealDir` resolves the `.gui` files the Chinese language mod overrides to the Workshop folder. Mount order: mods are mounted later, and `append=0` puts them in front. No mod overrides `unitpanel_main.gui` yet, so whether the splice lands on the winning file is not tested. |
| 5 Whether the spliced file parses | Pass | 22784→22970 bytes; `error.log` has no `Unexpected token` / `Already Defined` / `already exists`, and no line about `eu4harmony` at all. The button shows on the right of the army panel (the user confirmed it by eye). |
| 6 Declarations ready in time for GUI loading | Pass | Deferred initialization finishes inside `PHYSFS_init`, about 17 seconds before the first `.gui` is read. |
| 7 Still works after a reload | Not tested | |
| 8 Coexisting with other plugins that hook PhysFS | Not tested | |
| 9 The framework's own assets can be read | Not tested | |
| 10 Notice window | Not done | Notices are a Windows message box for now. |

Other observations:

- The game calls `PHYSFS_init` twice and remounts every directory in between; deferred initialization is guarded by a `Once` so it runs only once.
- Running `eu4.exe` directly works and uses what `dlc_load.json` holds at the time; later tests go through the Paradox launcher instead.
