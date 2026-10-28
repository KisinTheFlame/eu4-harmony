# Prototype test log

Checked against the 11 items under "Order of work" in [v1.md](v1.md). Baseline: vanilla 1.37.5 (`9ad3efe1…`), with 4 Workshop mods enabled at the same time, the Chinese language mod among them.

## 2026-10-27 First test (running eu4.exe directly)

| Item | Result | Evidence |
| --- | --- | --- |
| 1 When the proxy loads | Pass | The core loads inside `DllMain` on the main thread and installs 9 PhysFS hooks before the game first calls `PHYSFS_init`. Chain-loading not tested: the root folder had no `winmm.dll` to begin with. |
| 2 Which thread loads the GUI | **Not the main thread** | Every `.gui` is read on thread 56780; the main thread is 46896 (56780 is the LoadingThread the research points to). |
| 3 Path format | Pass | `interface/unitpanel_main.gui`: lower case, forward slashes, no leading directory; all 130 `.gui` files go through `PHYSFS_openRead`. |
| 4 Coexisting with mods that override the same file | Partial | `getRealDir` resolves the `.gui` files the Chinese language mod overrides to the Workshop folder. Mount order: mods are mounted later, and `append=0` puts them in front. No mod overrides `unitpanel_main.gui` yet, so whether the splice lands on the winning file is not tested. |
| 5 Whether the spliced file parses | Pass | 22784→22970 bytes; `error.log` has no `Unexpected token` / `Already Defined` / `already exists`, and no line about `eu4harmony` at all. The button shows on the right of the army panel (the user confirmed it by eye). |
| 6 Declarations ready in time for GUI loading | Pass | Deferred initialization finishes inside `PHYSFS_init`, about 17 seconds before the first `.gui` is read. |
| 7 Still works after a reload | Not tested | |
| 8 Coexisting with other plugins that hook PhysFS | Not tested | No conflict seen with the CJK font plugin running at the same time, but it is not clear whether it hooks PhysFS. |
| 9 The framework's own assets can be read | Not tested | |
| 10 Notice window | Not done | Notices are a Windows message box for now. |

Other observations:

- The game calls `PHYSFS_init` twice and remounts every directory in between; deferred initialization is guarded by a `Once` so it runs only once.
- Running `eu4.exe` directly works and uses what `dlc_load.json` holds at the time; later tests go through the Paradox launcher instead.

## 2026-10-27 Second test (Paradox launcher, hello + regional-combat)

The framework, extension loading, click bindings and every game capability run together for the first time. A new game (not a loaded save) as Ethiopia, with an army selected:

- **Loading**: baseline 1.37.5 passes the section layout and dependency point checks; both extensions load; 10 elements and 2 button area slots are spliced in (22784→24562 bytes).
- **Interface**: about 2 seconds after the game starts, 10 elements are bound. Left and right clicks on the hello button both arrive, and it gets the army currently selected. The region button's left and right clicks, the radius box's Chinese titles "区域战斗" (Regional Combat) and "半径 5" (Radius 5), and the apply button all work (the user's screenshot). The framework converts the Chinese and writes it through `0x152ae10`; both strings are within 15 bytes, so writing longer text is still not tested.
- **Combat**: once on, the center is province 1224 with radius 5. It decides daily: it attacks 2771 (correctly holding off during the siege), then 2772 (holding off during the battle and the siege), and finally returns to the center, 1224. The route matches the one recorded in the old regional-combat's tests. The reasons for holding off all come from the framework's army state (movement lock, siege, battle).
- **Save data**: after the autosave finishes, `eu4-harmony/saves/<path digest>-<save digest>.json` is written. Restoring it on load is not tested.
- **Timing**: a button callback takes 9 ms at most; a decision takes about 16–17 ms, and the frame that issues an order about 32 ms (two snapshots); each map redraw while the radius is adjusted takes about 8 ms.
- **Settings box problem**: the background was nearly black and flickered on mouse hover. The cause: the background used the vanilla dark dialog texture, and it was made as a button (which has a hover state). regional-combat now makes the background an icon and keeps the look of the dark texture (a 330×150 image made in advance, because icons do not stretch their texture). The user tested again and confirmed it displays correctly and no longer flickers.
- **The proxy loaded by other processes**: `proxy.log` recorded one `eu4_harmony_bootstrap returned 1`. Other programs in the game folder (such as launcher components) may also load `winmm.dll` indirectly; the core then fails because it finds no PhysFS exports there, and on the way rotates `core.log` to `core.prev.log`. Now the core loads only in `eu4.exe`; other processes only forward calls.

Not tested: the actual colors of the map preview, tooltips, turning off on defeat and on a manual order, restoring on load, the Shift/Ctrl step sizes.

## 2026-10-27 Third test (notice window)

Two test mods whose manifests are wrong on purpose (missing `api_version`) were added to the playset, to trigger two notices:

- `core.log` recorded both notices, and on reaching the main menu (about 32 seconds after start) bound the notice window's 4 elements to the main menu.
- The user confirmed the notice window displays correctly and shows the next notice after one is closed.
- Conclusion: item 9 passes (the texture is read from `eu4-harmony/gfx/`); the main menu part of item 10 passes.
- This run did not go into a game, so the display in the top bar and binding again after the host window is rebuilt are both not tested.
- The user pointed out the texts were English only, so localisation was added (ADR 0009).

## 2026-10-27 Fourth test (localisation)

The same playset as last time (regional-combat plus the two test mods), a new game.

- **Deciding the language**: the result is `simp_chinese`. The decision happens in the deferred initialization inside `PHYSFS_init`; by then the CJK font plugin has loaded, and the Windows display language is Chinese. The game's own language is english.
- **Splicing**: the game reads `localisation/text_l_english.yml` through `PHYSFS_openRead` on the main thread, twice. It reads the copy the Chinese language mod overrides; with regional-combat's 6 keys appended it goes from 716155 to 716886 bytes.
- **Interface**:
  - The notices in the main menu are in Chinese.
  - In the game, the notice window binds to the top bar, and the army panel binds 9 elements.
  - The region button's text (from the spliced keys), the tooltips and the radius box are all in Chinese; the user confirmed they are fine.
- **Errors**: no related errors in `error.log`.
- **Conclusion**: item 11 passes.
- **Cleanup**: after the test the two test mods were deleted (their registration files under `mod\` in the user directory, and `target\notice-test\`).

## 2026-10-27 M0 test (dependency points declared in one place, all-or-nothing startup)

Against the "M0 checklist" in [unicode-module.md](unicode-module.md), tested by Codex with computer use, in two rounds; I checked the results against the raw logs. The playset had the Chinese language mod and regional-combat enabled; a new game as the Ottomans, running `eu4.exe` directly.

- **Startup**: every start reported `startup: baseline 1.37.5: 58 dependency points match; 34 hooks, 0 writes`. Localisation, `core.gfx`, `frontend.gui`, `topbar.gui` and `unitpanel_main.gui` were spliced through PhysFS; the main menu, the top bar and the army panel were all bound.
- **Regional combat**: the button and its Chinese tooltip are fine; a left click turns region mode on, a right click opens the radius box, and after the radius is changed to 6 the map coloring grows to match; the daily decision runs every day. After a declaration of war it issues a move order on its own (`command move to Some(151): accepted`), and the army starts moving.
- **Saves**: saved under the default name (the file name is Chinese, `奥斯曼1444_11_11.eu4`, "Ottomans 1444_11_11"); about 0.86 seconds later the extension save data was written, holding the on state and radius 6. Loading this save logged `loaded … with data for 1 extension(s)`; regional-combat restored 1 task, with the radius still 6.
- **Rebuild**: back to the main menu and into a new game; the button appears again, with no binding errors.
- **Startup failure**: started with `EU4HARMONY_FAIL_STARTUP=1` set; not a single hook was installed; a message box popped up saying EU4 Harmony could not start, and the report says `startup FAILED`; the game runs as usual, with no regional-combat button on the army panel.
- **Four languages**: with each of French, German, Spanish and English set, the game started normally, with the same startup line. The Chinese language mod keeps the menus in Chinese, so the display of these languages themselves was not checked. In the French, German and Spanish runs, the game set the language in `settings.txt` back to `l_english`; the framework did not write that.
- **Not covered**: the stop order, the defeat event.
- **Cleanup**: the test save and its extension data are deleted; the 141 existing saves and data files, `settings.txt` and `dlc_load.json` are all byte-for-byte the same as before the test.

## 2026-10-27/28 Unicode module and framework UI test (run by me)

The same playset as the M0 test (the Chinese language mod, regional-combat and others). The framework enters through `version.dll`; the old CJK font plugin is moved to `eu4-harmony\backup\` and not loaded.

- **Startup**: every dependency point checks out; the last run had 198 dependency points, 83 hooks and 2 writes.
- **Main menu**:
  - Chinese is correct, and a save name in legacy escapes written by the CJK font plugin (Ethiopia) displays too;
  - the notice window (`EU4HARMONY_TEST_NOTICE=1`) and the corner button follow the game's UI scale;
  - the corner button goes away on the country selection screen and comes back on the front page. This is decided by the main menu's page number (`+0x900`); at first I used the visible flag of the `mainmenu` child window, but it does not change when the page changes, so it cannot be used.
- **Two graphics driver failures**:
  - What happened: once a 0x133 blue screen, once a 0x141 GPU timeout followed by a hang. Both happened while the notice window was showing, while the machine was also running a training job that kept the GPU fully loaded.
  - The problem fixed: the loading screen is drawn and presented by another thread, and the overlay drew on that thread, interleaving with the main thread's operations on the same device (the device is created MULTITHREADED, which locks each call but not a sequence of calls). Now it draws only in frames the main thread presents. In many runs since, there have been no more failures.
- **Map names**:
  - What happened: neither country names nor province names showed.
  - Investigation: I removed the framework's groups of hooks by bisection; in the end they did not show even with no framework installed at all, on vanilla with this set of mods. Reading the font state at run time showed that the map font page `zh-hans-map` had a size of 0×0.
  - Cause: texture loading skips files over 16 MiB at 0x16c2cb7, and this page is a 49 MB DXT5. The CJK font plugin relaxed that limit.
  - Fix: the limit is now 256 MiB. Since then country, province and sea names all show as correct Chinese.
- **Crash on loading a save**: loading an August save with `-continuelastsave` crashed the game (eu4+0x88a1fb, an out-of-bounds array read). It crashes the same way with the CJK font plugin installed: the save is incompatible with the current set of mods, which has nothing to do with the framework.
- **A save rewritten**: when `-continuelastsave` loaded `Castilla-5.eu4`, the game saved it again when play began, along with its `_Backup`. The original file could not be recovered; no progress was lost, and the names are now stored as UTF-8. From now on, back up the whole save folder before loading a user's save.

## 2026-10-28 Tests run by Codex, and input tests

Codex tested from the checklist with computer use, and I checked its results against the logs and screenshots. After that I tested input myself with Unicode key events (SendInput).

- **Passed**:
  - startup: 198 dependency points;
  - the two test notices show and close one after the other;
  - the corner button appears only on the main menu's front page;
  - the settings window's Save and Esc to cancel, with changes taking effect after a restart; the framework UI switches to the chosen font;
  - Chinese in the top bar, tooltips, the province window, the army panel and regional-combat's tooltips;
  - curved country names (Ming, Korea, Mongolia, the Timurids and others);
  - save names written by the CJK font plugin;
  - all four game languages start up to the main menu.
- **Input (tested by me)**, in the "Find" box on the country selection screen:
  - typing "朝鲜" (Korea): whole characters are inserted, and the search finds 朝鲜, 朝鲜西部, 朝鲜湾 (Korea, western Korea, Korea Bay) and more;
  - "朝鲜abc", then Left three times and Backspace, gives "朝abc"; Home then Delete gives "abc";
  - Ctrl+V pastes "中文测试" ("Chinese test") correctly;
  - with "abc" selected, copy and cut both work.
- **Could not verify**:
  - selecting Chinese with Shift and the arrow keys, by dragging with the mouse or by double-clicking, then copying: the injected keys and mouse made no selection and the copy came out empty; this needs another try with a real keyboard;
  - where the IME candidate window appears;
  - saving and loading with Chinese save names;
  - accented letters in French, German and Spanish: the Chinese language mod keeps the menus in Chinese, so I did not see them.
- **To fix**:
  - the font drop-down list in the settings window is taller than the screen and cannot scroll;
  - line breaking in main text: paragraphs such as the start-of-game introduction have very short lines, with full stops at the start of a line; pinyin inside Chinese text is cut in the middle (Yu / anzhang). The cause is still being looked into; I checked the logic of the layout loop itself and found nothing wrong.
  - in one game, the game's `error.log` got 7951 `Could not find coloring for character` lines:
    - in 5820 of them the character is the byte 0xB0, which suggests some path takes the UTF-8 continuation byte 0xA7 for a color code;
    - 2131 are `I`, from color codes in the interface definitions themselves, unrelated to the framework.
  - the two truncating functions at vtable `+0x70` and `+0x80` (which measure the width, then add " ...") still measure byte by byte, and may cut a character in half.
- **Observation**: on "Back" from the country selection screen, the game restarts itself as a child process (vanilla behavior, unrelated to the framework).

## 2026-10-28 Line breaks, punctuation and color codes (tested by me)

- **Font drop-down list**: when opened it reaches at most the bottom edge of the screen and scrolls to the item currently selected; the wheel scrolls to either end and stops there; after scrolling, a click picks the row under it; after Cancel the settings file is unchanged.
- **Line breaks**:
  - Cause: in game text, characters 1252 has (`·`, `“”`, `…` and so on) are single bytes. We gave ICU only the valid UTF-8 at the start, so after the first such byte there were no break points, and the game cut the rest of the paragraph as one very long word (see [research/unicode-text.md](research/unicode-text.md)).
  - After the fix: the scenario introduction on the country selection screen (which contains "匈雅提·亚诺什", Hunyadi János) fills all five lines, with no short lines; Ming's start-of-game introduction and the text of the "大明时局" (the state of the Ming) event are fine.
- **Punctuation**: the Chinese language mod's fonts put `“ ” … — •` at their Unicode code points and leave the 1252 byte positions empty, so these marks used to be missing (the "……" at the end of the scenario introduction was gone). Now when the single-byte table has nothing, the lookup uses the code point instead, and "……" shows.
- **Color code errors**:
  - A new game as Ming, run for more than a month, with the province window, events and the start-of-game introduction opened: not a single line of the `°` (0xB0) kind appeared again (Codex's run had 5820 in 7 days). Presumably it had the same source as the line break problem.
  - The remaining `I` lines come from `§V§!§I§!§G商品产出§!`, written in the Chinese language mod's own text (`I` is not a color the game defines), unrelated to the framework.
- **Truncation**: the two functions at vtable `+0x70` and `+0x80` now measure and cut whole characters (`unicode/fitted_text.rs`). Tested in a game as Hungary: the music list, person names and the rebel names in the outliner on the right are all cut between characters and end with " ...", for example "（8k）塞尔维 ...", with no half characters.
- **System fonts**:
  - The "System fonts" look: the text in the main menu and on the country selection screen is all Microsoft YaHei, and map names are KaiTi.
    - The first start crashed while loading: when the truncating functions' glyph lookup stub called Rust, the stack was 8 bytes off alignment; the bitmap look happened not to reach the instruction that faults. The stub now aligns the stack itself.
    - Map names turned white at first: the map uses the page's colors directly. Now added glyphs take the color of `H` in the original page, and where there is no original page, the map font uses black.
  - The "Keep the fonts as they are" look: the Chinese language mod's fonts lack most accented Latin letters (À–Ý, ä ö õ ñ ç and so on), and these letters used to be missing (the diplomat "Lőrinc" showed as "Lrinc"). Now the Chinese language mod's DXT5 pages grow downward too, and the missing letters come from the system font; the baseline and size come from the ink of `H` in the original font (its glyphs are cells as tall as the whole line, so the glyph box itself does not show the baseline).
- **A lesson from diagnostic code**: an observing hook must forward as many arguments as the function really takes (11 for main text, 15 for button text; more than 12 needs a jump stub). When it forwards fewer, the original function reads garbage from the stack, corrupts memory, and the game crashes on another thread. At the time I checked only four by hand; now the offline check (`cargo test -- --ignored`) counts from each observed function's code how many arguments it reads, and fails when an observe point forwards fewer.

## 2026-10-28 Edit box keys and the IME (fixed after the user tested with a real keyboard)

The user tested in the "Find" box on the country selection screen and found three problems, all now fixed. After the fixes I tested again with injected keys and real Microsoft Pinyin composition:

- **Backspace**: after typing "朝鲜", two Backspaces used to leave "œ" (0x9C, the second byte of "朝"); now the box is empty.
- **Home, End**: pressed alone they used to select as well (a vanilla bug, see [research/text-input.md](research/text-input.md)); now they only move.
- **Shift+arrow keys**: the edit box used not to handle them, so the keys went to the map and moved the view (vanilla never had this feature). Now:
  - Shift+Left selects "鲜abc", and copying gives "鲜abc";
  - Home, then Shift+Right twice, selects "朝鲜", and copying gives "朝鲜";
  - the map does not move.
- **IME window**: it used to appear at the top left of the screen (I took the caret's coordinates relative to its parent window for screen coordinates). Now the composition string appears at the start of the edit box's text, with the candidate window right below it.
  - For the retest I switched the game window to Microsoft Pinyin for a while with `WM_INPUTLANGCHANGEREQUEST`, and switched it back to English afterwards.

## 2026-10-28 Saves, search and the history window (tested by me)

Before the test I backed up `save games\`, `eu4-harmony\saves\`, `settings.txt` and `continue_game.json`. A new game as Ming:

- **Province search**: F opens it; typing "北京" (Beijing) lists "北京". (The F key itself also gets typed into the box; vanilla does that.)
- **Saves with Chinese names**:
  - The save dialog's default name is already Chinese ("明1444_11_11"); saved as "测试存档中文.eu4" ("test save Chinese"), it shows correctly in the list.
  - I turned regional-combat on for the army in Beijing and saved as "区域战斗存档.eu4" ("regional combat save"). The framework log says the extension data is bound to `…\save games\区域战斗存档.eu4`.
  - Loading this save: the log has `loaded …区域战斗存档.eu4 with data for 1 extension(s)`, and regional-combat's log `session 2: restored 1 task(s)`.
  - Afterwards I deleted the two test saves and that extension data. The game had rewritten `continue_game.json` to point at a test save; it is restored from the backup. The checksums of the other 140 saves match the backup.
- **Country history window** (the "History" button by the minimap): "「谨慎者」Yu / anzhang" (「谨慎者」 is an epithet, "the Cautious") used to be split. The window breaks its text into lines with the game's multi-line text box, which breaks only at spaces (see [research/text-input.md](research/text-input.md)). After the fix, "Yuanzhang。" and "Yunwen。" move whole to the next line.
- **The "System fonts" look in a game**: Ming's start-of-game introduction (with names highlighted in yellow), the outliner and the top bar are all Microsoft YaHei, and lines break correctly. Afterwards the setting went back to "Keep the fonts as they are".
- **Searching without accents**: the Chinese language mod turns all place names into Chinese, so there was no accented Latin place name to search for, and this could not be verified in the game; unit tests cover it.

## 2026-10-28 Regression after the debt cleanup, and call tracing (tested by me)

Backups as above. A build with the `trace` feature, tracing font vtable `+0x48` (`0x159c600`) and `+0x80` (`0x159f510`); a new game as Poland.

- **Startup**: every dependency point matches (the field check points in `objects.rs` included); the report at the main menu is normal.
- **Call tracing**: the log writes each call's arguments, the text they point to, and the return value.
  - Every call to `+0x80` comes from text box layout at `0x15f440f`: the loading screen's status and tips (an 800×200 box), labels in the main menu and the lobby, the music list and so on, over three thousand times at the start of a game.
  - `+0x48` (floating text) was not called once, from the main menu and the lobby through the start of a game to several months into it.
- **The line breaking branch of `+0x80`**: the description on the diplomacy page of the macro builder (B key) wraps into two lines in a box 290 wide, "…自动开始改善关 / 系的目标类型。" (the break falls between two Chinese characters), and the full stop does not land at the start of a line.
- **The host window table**: regional-combat's "区域" (Region) button appears in the first slot of the third column of the army panel, as before the change.
- **Costs**: by the main menu, 84026 line break queries, 24862 pieces of text whose breaks ICU worked out, 148 ms in all (about 6 µs each); 7 glyphs, 15 ms; 6 font pages, 236 ms.
- **Saves**: the game autosaved during the first game; `autosave.eu4` rotated through three copies and `continue_game.json` was rewritten. Both are restored from the backup, and the checksums match.
- **Tools**: in `game-input.ps1` the constants `$WHEEL` and `$keys` had the same names as the parameters `-Wheel` and `-Keys` (PowerShell variables ignore case), so they overwrote the parameters and the wheel kept turning; they are renamed. Called with `pwsh -File`, a comma list becomes one string; call the script from PowerShell with `&`.

## 2026-10-28 One-character map names and keys during composition (tested by me)

Backups as above. Framework settings as the user had them: system fonts, Microsoft YaHei for text, KaiTi for the map. Everything was checked in the single player lobby, East Asia bookmark (1444), without starting a game.

- **One-character country names** (reported by the user): 明 (Ming) lay upside down, and 藏, 康 and 卫 were turned too, while names of two characters or more read normally. A one-letter name takes the curve's direction at its very end (see [research/unicode-text.md](research/unicode-text.md)).
  - Tried first: counting the game's one-or-two-letter rule in characters. 康, 藏 and 卫 came out upright but 明 lay on its side, because that rule's direction is read from the wrong type of value and is arbitrary. Moving the single glyph to the curve's middle changed nothing visible. Both were dropped.
  - Kept: a one-character name gets a horizontal direction. 明, 康, 藏 and 卫 stand upright in the middle of their territory; 朝鲜 and the other longer names look as before.
- **Enter during composition** (reported by the user): in the lobby's Find box, with Microsoft Pinyin in Chinese mode, `ming` showed its candidates next to the box; Enter put the letters `ming` into the box and the box stayed open. Backspace during the composition `ab` removed only `b` from the composition, leaving the box's `ming` alone; Escape cancelled the composition and the box stayed open. Before the fix, Enter also closed the box.
- **Tools**: `game-input.ps1 -Keys` now presses any letter or digit, so pinyin can be typed through the input method; `-Type` sends Unicode characters past it.

## 2026-10-28 Performance (measured by me)

Framework with bitmap fonts and with system fonts, the CJK font plugin, and neither, each two or three times with `tools/perf-run.ps1`; results and method in [performance.md](performance.md). Backups as above; the runs rotated the three autosaves, which were restored and checked against the backup's hashes. With neither the framework nor the plugin, but the Chinese language mod still in the playset (so not vanilla), the game crashed twice on entering single player, in the Epic Online Services SDK; the two crash folders were removed afterwards, so the dumps are gone.

Then, at the user's request, vanilla in English and the framework in English, three runs each, with the Chinese language mod and the modifier sorting mod (whose text needs the plugin) taken out of `dlc_load.json` for the runs and put back afterwards. Vanilla in English entered single player without crashing every time. Every framework run advanced fewer days at speed 5 than every vanilla run (about 2.5% on average), which the Chinese runs had been too spread out to show. Saves and settings were restored and checked as before.

## 2026-10-28 Where bitmap fonts' memory goes (measured by me)

A build that logs process private bytes and the framework's heap: at startup, around each font page it makes, and at the main menu. Chinese playset, to the main menu only (no save loaded), once with bitmap fonts and once with system fonts, then once more with bitmap fonts after splitting each page into creating it and writing it. Results in [performance.md](performance.md) and [research/font-textures.md](research/font-textures.md): writing the grown dynamic pages a band at a time accounts for about 440 MB (425 MB for the map page), and the glyph records each of the 46 fonts keeps for itself for 63 MB more. The framework settings were put back afterwards.

## How the implementation differs from the final design

- Binding the notice window again after its host window is rebuilt (for example by `reload`) is not tested yet; the top bar binds, but how notices look in the top bar has not been confirmed on its own.
- The button area is a third column to the right of the two vanilla button columns (x=464, on the vanilla 43×44 grid), with no background frame.
- A text box is "hidden" by clearing its text, not by calling the text box's show/hide method (which is not verified on this build).
- Route queries, moves and stops are all refused while the game is paused (`NOT_READY`), as in the old regional-combat.
- Map coloring borrows native map mode 12; provinces that are not colored show that mode's own colors.
- When the user directory cannot be found, logs fall back to `eu4-harmony\logs\` in the game directory.
