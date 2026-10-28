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
  - The problem fixed: the loading screen is drawn and presented by another thread, and the framework UI drew on that thread, interleaving with the main thread's operations on the same device (the device is created MULTITHREADED, which locks each call but not a sequence of calls). Now it draws only in frames the main thread presents. In many runs since, there have been no more failures.
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

## 2026-10-28 Font pages written through system memory (tested by me)

The grown pages are now ordinary video-memory textures, written through a small system-memory texture with `UpdateSurface`. Chinese playset, to the lobby only, once with bitmap fonts and once with system fonts; framework settings put back afterwards.

- **Memory and time**: see [performance.md](performance.md#since-then): 3866 MB at the main menu with bitmap fonts instead of 4353, 3778 with system fonts instead of 3860; system fonts drew their 3,143 glyphs in 28 ms instead of 3.5 s.
- **Display**: the main menu, the lobby's text and every country name on the East Asia map show as before in both looks (bitmap: the Chinese language mod's KaiTi page; system: KaiTi drawn by the framework), so the pages' own glyphs were copied in where they belong.
- **Glyphs drawn while playing**: typing "한글テスト" into the lobby's Find box showed the Korean and the kana in both looks; the Korean is not in the mod's fonts, so it was drawn into the page then, through the new path.

## 2026-10-28 Shared glyph sets (tested by me)

Fonts that bring the same glyphs above 0xFF now share one set. Chinese playset, to the lobby only, twice with each look; framework settings put back afterwards.

- **Memory**: 994,216 glyph records kept as 6 shared sets of 115,053; the framework's heap at the main menu 3.2 MB instead of 66 MB, the process 3761 and 3763 MB instead of 3866 with bitmap fonts, 3767 and 3746 MB with system fonts.
- **Display**: in both looks the lobby's text and the East Asia map's names show as before, and "한글テスト" typed into the Find box shows, so glyphs from the shared sets and glyphs drawn later are both found.
- **Tools**: the first run clicked as soon as the log said the main menu was up, before it took clicks, and stayed on the menu; waiting 8 s first fixed it.

## 2026-10-28 Dates, names and battle names in the player language's order (tested by me)

Backups as above (the game was never unpaused, so nothing was autosaved). Chinese playset, system fonts.

- **Names**: in the lobby, Ming's ruler reads "皇帝 Zhu Qizhen" (the game wrote "Qizhen Zhu") and Korea's "国王 Yi Sejong Do". Started as Korea: the court shows "国王 Yi Sejong Do", "继承人 Yi Hyang", "王后 Sim Soheon", and the ruler's tooltip ends "君主死亡: Yi Hyang继承王位", the line built in place.
- **Dates**: the top bar reads "1444年11月11日"; the admin technology tooltip "我们预计将在1448年10月拥有足够点数来完成它" (the rounded month and year). The ruler's "于1444年11月11日上台" is the long date, which the Chinese language mod already words.
- **Not seen in the running game**: battle names (no battle was fought), plain month and year, year, month and day, the timeline and great projects; each is covered by unit tests only.

## 2026-10-28 After the technical-debt work (tested by me)

Every field, global, class and method the framework relies on is now a dependency point, game strings go through one module, the hook wiring is shared, and extensions report their ABI. The regional-combat extension was rebuilt with the new SDK. Backups as above; only `settings.txt` changed and was put back, no save was written and no crash folder made. Chinese playset, system fonts.

- **Startup**: 365 dependency points match, the vtable methods among them checked against the relocated image; 102 hooks, 4 writes. The extension's exported ABI version was read and it loaded. The device was found multithreaded and the six font pages were filled.
- **Display**: the main menu's continue box reads "1470年3月13日"; the lobby shows Korea's "国王 Yi Sejong Do" and the East Asia map's names, one-character names upright; started as Korea, the top bar reads "1444年11月11日", the start-of-game introduction wraps its Chinese with no punctuation at a line start, and messages read "同意结盟", "接受朝贡提议".
- **Extension**: selecting the army in Hanseong bound 9 elements in the army panel; clicking the extension's button turned its task on ("army 2628519985206 task on: center 735 radius 5") and again off, its tooltip showed in Chinese, and its geography read ran in `on_frame` with no error. Three game days passed unpaused with nothing in the core log.
- **Not seen in the running game**: the map preview (it needs a right click, which the input tool cannot send), an edit box after the change (its offsets are now named constants, which assemble to the same instructions, and the points check them), floating text, a save written with extension data, and a battle.

## 2026-10-29 The capability layer, through regional-combat (remote control F1, tested by me)

Every capability now runs in the capability layer (`core/src/game/`) and the host functions only convert to and from the ABI. The regional-combat extension was not rebuilt: its DLL, built with the SDK before the change, is what an extension sees through the unchanged ABI. Backups as above. Chinese playset, system fonts; a new game as the Ottomans, running `eu4.exe` directly.

- **Startup**: as before: 365 dependency points match; 102 hooks, 4 writes; the extension loaded, and the main menu showed its continue box and the framework button.
- **Reading**: selecting the Balkan army (in province 148) bound 9 elements in the army panel; the button's left click turned its task on ("center 148 radius 5"), and its tooltip showed in Chinese, from the mod's texts. For the 32 days until 1444.12.16 its daily decision read the session, the provinces, their neighbors and the armies in them, with no error.
- **Moving and stopping**: war on Byzantium (without a casus belli, since the game allows none before 1444.12.11). On the next day the extension ordered "move to Some(146): accepted", and the game showed the army marching on Athens. Clicking the button while it marched read the route as its own and ended in "stop to None: accepted". On again, it ordered moves to 151 and then 316, accepted.
- **The player's order**: a right click on Thessaly, with the army marching on the extension's order, turned the task off with "manual order", and the army went to Thessaly as the player asked. The order was heard through the changed frame start.
- **Map coloring**: a right click on the extension's button opened its radius box and colored the map; cancelling restored the map mode.
- **Sessions**: the autosave of 1445.1.1 was written with the extension's data; loading it logged "loaded … with data for 1 extension(s)" and "session 2 started (from a loaded save: true)", and the extension restored its task. After loading, the army's route read as set by someone else, since routes are forgotten at a new session, as before.
- **Costs**: 44,781 callbacks took 2,195 ms in all, the longest 61 ms; the core log has no error, and no extension was stopped.
- **Not seen in the running game**: a remote client's order (`Caller::Remote`), since nothing calls it yet; its bookkeeping (the army taken back from the extension, one player's order raised) is covered by a unit test only. A battle, so the defeat event; a strait; the radius box's apply button.
- **Tools**: `game-input.ps1` now right-clicks (`-RightClick x,y`), which the player's order and the map preview needed.
- **Cleanup**: the run rotated the three autosaves, rewrote `continue_game.json` and added one extension data file; the backups were put back, the new file deleted, and both save folders and the three settings files compared equal to the backup. No crash folder was made. The framework was uninstalled afterwards as AGENTS.md says, which put back the CJK font plugin's `version.dll`; an earlier build of the framework had been installed before the run.

## 2026-10-29 Remote control's engine, through a probe thread (remote control F2, tested by me)

No transport exists yet, so `eu4.exe` was started directly with `EU4HARMONY_TEST_REMOTE=orders`: a thread of the framework's own submits requests every 5 s and logs the answers, as a transport would. Chinese playset with the unchanged regional-combat extension, system fonts; backups as above. Two runs: the second after the fix in the main menu below.

- **Startup**: 367 dependency points match (the two new ones keep the country array); 102 hooks, 4 writes.
- **Before the main menu**: the first request, submitted while the game loaded, had no answer within the probe's 30 s; the engine then dropped it unrun ("given up by its client before it ran").
- **Main menu**: `describe` (18 capabilities), `text.language` (`simp_chinese`) and `events` were answered at the frames the main thread presents; a read waiting 2 s for an event answered after 2.0 s, with none. In the first run `session` was never answered in the main menu or the single player setup: what needs a game was answered "not in game" only at a frame with no world, and the main menu has one. In the second run, told by the main menu's own frame, it answered `not_in_game: the main menu is open`, also in the single player setup with the Ottomans picked.
- **Into a game**: a new game as the Ottomans. A `session` submitted while it loaded was answered at the first game frame: session 1, 1444.11.11, paused, player `TUR`. A read waiting 2 s for an event, submitted at the same time, came back after 8.9 s, at the first frame after the load, with the session's event (number 1).
- **Reading**: `country.hostile` `TUR`/`BYZ` false (no war yet at the start), `TUR`/`QQQ` `not_found: no country QQQ`, 1.6 to 2.4 ms each for two lookups; `province.list` 4941 provinces, 41 of them the Ottomans', 6 to 21 ms (the first builds the map); `army.list` with the army in Edirne (149, 15 regiments, ID 231928234038) and the fleets; `army.in_province` 149 `not_ready` until visibility was current, then that army; `province.neighbors` 149: 4779, 4705, 151, 2750, 1764; `army.can_enter` 4779 true; `army.selected` `not_found: army panel not open`.
- **Orders**: with the game running, `army.find_route` to 4779 gave `[4779]`; `army.move` answered null, the route read `[4779]`, and the army `route_len` 1, `destination` 4779, `route_source` `external`; `army.stop` answered null and the route read empty. The next read of events had `army_ordered` for the army twice, once for the move and once for the stop, then a `day` event for each day.
- **Waiting reads in a game** answered when the next day's event came (after 1.84 s of 2) or when their time ran out.
- **In-game menu and summary**: with the Esc menu open, and on the end-of-game summary, the session was still read at game frames.
- **Returning to the main menu** started `eu4.exe` again: the framework bootstrapped anew (a new main thread, a new log) and request numbers began again at 1 ([research/safe-points.md](research/safe-points.md)).
- **Costs**: 129 requests answered at once in the second run took 0.9 ms on average, the longest 21.5 ms (the first province list). The core log has no error, and no extension was stopped.
- **Not seen in the running game**: a remote order taking an army back from an extension (regional-combat's task was not on), `army.can_attack` and `army.strait_open` (no enemy army in sight, no strait next to the army), a frame cut short by the time budget (the probe asks one request at a time), requests from several threads at once, a defeat event, the session event of a loaded save, an `events` read answered with `missed` (for a `since` beyond the last event, which came in after these runs, or for a reader that fell behind the 10,000 kept), and `not_in_game` while observing. Unit tests cover the bookkeeping of a remote order (F1), the engine's handling of `army.can_attack` and `army.strait_open` with a fake game behind it, the time budget, requests from several threads, and the `missed` read; nothing covers the observing case.
- **Cleanup**: no save was added or changed; `settings.txt`, `continue_game.json` and the framework's settings were put back and, with both save folders, compared equal to the backup. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups and screenshots were deleted.

## 2026-10-29 Remote control's named pipe (remote control F3, tested by me)

The framework now opens `\\.\pipe\eu4-harmony`; the client was `tools/remote-pipe.ps1`, PowerShell's `NamedPipeClientStream`, run in the foreground and as separate background processes for the clients that waited. `eu4.exe` was started directly. Chinese playset with the unchanged regional-combat extension, system fonts; backups as above.

- **Startup**: 367 dependency points match; 102 hooks, 4 writes. The pipe opened 9 ms after startup, "to S-1-5-21-…-1001 only", the user's SID.
- **Before the main menu**: a client that started with the game opened the pipe at once; its `text.language` was answered 40 s later, at the first frame the main thread presented, and its `session` at the main menu with `not_in_game: the main menu is open`.
- **Main menu**: `describe` listed 18 capabilities. A client waiting 15 s for events was answered after 15.0 s with none, while another client was answered in between. A client stopped 3 s into a 60 s wait gave its request up: the transport logged it closing the pipe "before its answer came, which gave the request up", and the engine "given up by its client while it waited", 5 ms apart. Lines that were no request (`not json`, `army.fly`, `army.get` with no army), written together with a good one, were each answered `bad_request` with the reason, in order, and the good one after them; so were the pieces of a request that a command line had split up.
- **Into a game**: the continue box loaded a Castile save of 1470.3.13. Requests written together right after the click were answered in order: `session` with "the main menu is open", 1.0 s after the save had been read (the main menu's frame still ran then; [research/safe-points.md](research/safe-points.md)), then `text.language` and `army.list` at the first frames after a 5 s gap. A second load gave the same: "the main menu is open" 0.5 s after the save was read, then `session` at the game frame.
- **In the game**: `session` read session 2, 1470.3.13, paused, player `CAS`; `country.hostile` `CAS`/`POR` false; `events` the session's event. A client waiting 20 s for events was answered 1.8 s after the game was unpaused, with the day event of 1470.3.14. `province.list` came back as one line of 513,362 bytes, 4941 provinces, 77 ms from the client's start to its end (16 ms in the engine).
- **Returning to the main menu**: a client waiting 60 s for events found its pipe broken 3.9 s after the summary's button was clicked, and said so ("the pipe broke before the answer came: …"), with exit code 2. The new `eu4.exe` started while the old one still had the pipe: its framework logged "cannot open \\.\pipe\eu4-harmony yet … another process has it" and opened the pipe 3 s after its startup. A client waiting for the pipe then got `text.language` answered at the new main menu, request numbers and events starting again from 1.
- **The final build**, after the pipe's code was tidied (no change in what it does, but for waiting a second after an unexpected error while waiting for a client), was run once more, the same way: at the main menu `text.language` and `not_in_game`; written 3 s after the continue box was clicked, `events` with `wait_ms` came back with the session's event once the load was over, then `session` (1470.3.13, `CAS`) and `army.list`; a client waiting for events found its pipe broken on returning to the main menu; the new process logged "cannot open \\.\pipe\eu4-harmony yet (another process has it)", opened the pipe 3 s after its startup, and answered `text.language` at its main menu.
- **After the review**, a third run, with each instance of the pipe made before the one before it is let go, and a client's closing told by the pipe's state. A client that started with the game wrote `text.language`, then `describe` a second later, and closed the pipe 3 s after that without reading: the transport logged it giving its request up 44 ms after the close, with `describe` still unread, and the engine logged `{"capability":"text.language"}: given up by its client before it ran` at the first frame, 40.6 s after startup. At the main menu, `text.language` and `not_in_game` as before. 3 s after the continue box was clicked, during the load, a client wrote `army.move` for army `1` (no army has that ID, so nothing could have moved), then `session`, and closed: the transport logged the give-up 40 ms after the close, and the engine `{"capability":"army.move","army":"1","goal":151}: given up by its client before it ran` at the first frame after the load, 1.1 s later. In the game `session` read session 2, 1470.3.13, paused, `CAS`, and `events` the session's event; a client that wrote `events` with `wait_ms` 60000 and closed 3 s later gave it up, logged by the transport and the engine 18 ms after the close. A client waiting 60 s for events found its pipe broken 2.1 s after the summary's button was clicked; the new process logged "cannot open \\.\pipe\eu4-harmony yet (another process has it)", opened the pipe 3.1 s after its startup, and answered `text.language`, `not_in_game` and `events` (none, `last` 0) at its main menu. The core logs of both processes have no error.
- **Costs**: 29 requests reached the engine in the two processes of the first run; of the 25 answered at once the longest took 16.1 ms (the province list). The core logs have no error, and no extension was stopped.
- **Not seen in the running game**: another Windows user or a remote computer refused by the pipe (only the access list was read back, in a unit test); a line over 1 MiB (unit test); a client whose own requests overtook each other, which the transport cannot let happen now (unit test, with the engine at a frame where a game is being set up); a second game started while the first runs, which would wait for the pipe as the restarted one did; an army order through the pipe (F4's done-when); an unexpected error while the transport waits for a client, after which it waits a second and tries again (no test reaches it either); a client that came and went before the transport waited for it (unit test); another process making a pipe of the same name while the game serves the pipe, which the third run's build no longer lets happen (unit test: no process tried in the game); a request given up before it ran, logged by the engine as JSON, until the third run: in the first two, every request given up was given up while it waited.
- **Cleanup**, after each run: loading the save rewrote `Castilla-5.eu4` and its backup, and `settings.txt` changed (not in the third run); those were put back from the backups, and both save folders and the three settings files compared equal to them. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups and screenshots were deleted.

## 2026-10-29 The command line (remote control F4, tested by me)

Every request went through `eu4-harmony game`, built from this branch (before names in its pipe code were changed, which changes nothing it does); `eu4.exe` was started directly. Chinese playset with the unchanged regional-combat extension, system fonts; backups as above.

- **Startup**: 367 dependency points match; 102 hooks, 4 writes. A `text.language` started 0.5 s before the game waited for the pipe, which opened 0.2 s after startup, checked that `eu4.exe` run by the same user serves it, and printed `"simp_chinese"` 40 s later, at the main menu.
- **Main menu**: `session` printed `{"code": "not_in_game", "message": "the main menu is open"}` with exit code 1; `text language` (the name's dot written as a space) `"simp_chinese"`; `describe` listed 18 capabilities, the same as `eu4-harmony game help`; `events --wait-ms 2000` printed no events after 2.0 s; `army.get 1` exit code 1, `not_in_game`.
- **Into a game**: an `events --wait-ms 60000` started before the continue box was clicked printed the session's event (session 2) 4.2 s after the click, once the Castile save of 1470.3.13 had loaded. `session` then read session 2, 1470.3.13, paused, `CAS`.
- **Armies**: `army list` printed 6: two fleets and four land armies, one of them sieging in 151. `army.move` while paused was answered `not_ready: game is paused`; with the game running, an army with others attached to it `rejected: other armies are attached to this army`. For army 5278514806838 in 4125 (7 regiments): `province neighbors 4125` gave 155, 153, 1953, 162 and 1318; `army can_enter` 155 true; `army find_route` `[155]`; `army move 5278514806838 155` printed `null` with exit code 0, after which its route read `[155]` and the army `destination` 155, `route_len` 1, `route_source` `external`; `army stop` printed `null`, and the route read empty and `route_source` `none`. `events --since 1` had `army_ordered` for that army twice, once for each order, among the day events. Later, in a battle, its move was answered `rejected: army is in battle`; after it had retreated to 151, `army move` to 149 on 1470.10.26 was answered `null`, and `army get` read it in 149 on 1470.11.8.
- **Returning to the main menu**: `events --wait-ms 60000`, waiting with nothing to come, found its pipe broken 4.8 s after the summary's button was clicked and said so (`the pipe broke before the answer came: …`), exit code 3. A `text.language` started at once, with no `--timeout`, found the new process's pipe 0.2 s later (the new process logged "cannot open \\.\pipe\eu4-harmony yet (another process has it)" and opened it 3 s after its startup) and printed `"simp_chinese"` 38 s later, at the new main menu.
- **Giving up**: at the new main menu, `events --wait-ms 60000 --timeout 2` ended after 2.04 s with exit code 3 and "no answer in time; the request was given up"; the transport logged the client closing the pipe before its answer came, and the engine `events: given up by its client while it waited`, 1 ms later. With the game closed, `session --timeout 1` ended after 1 s with exit code 3 and "no game with EU4 Harmony opened \\.\pipe\eu4-harmony in time".
- **Costs**: 56 requests reached the first process's engine and 3 the second's; each took at most 0.1 ms there but the first `province.neighbors`, 18.5 ms (the first read of the map). The core logs of both processes have no error.
- **Not seen in the running game**: a pipe served by another program, or by a game of another Windows user, which the tool refuses (unit tests: a pipe served by the test program, refused as not `eu4.exe`; one closed to the user, refused at once; the check of the user with a process described by hand, since no second user was at hand); a game or a tool run as administrator while the other is not; bytes in an answer and `--out`, since no capability answers bytes yet (unit tests); a boolean argument, since no capability has one (unit test with a capability of the test's own); a wait for the pipe longer than a fraction of a second (unit test: 300 ms); a pipe that breaks in the middle of a long answer; every instance of the pipe busy.
- **Cleanup**: loading the save and leaving the game rewrote `Castilla-5.eu4` and its backup (the continue box then read 1470.11.8); both were put back, and both save folders and the settings files compared equal to the backup. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups and screenshots were deleted.

## 2026-10-29 The camera (remote control C9, tested by me)

Every request went through `eu4-harmony game`, built from this branch; `eu4.exe` was started directly. Chinese playset with the unchanged regional-combat extension, system fonts; backups as above. What the screen showed was seen with `tools/screenshot.ps1` (the whole screen shrunk, and the middle of the screen cropped at full size), since the screenshot capability (C10) is built at the same time.

- **Startup**: 377 dependency points match (the ten new ones keep the camera's class, fields and moves and the map's province grid); 102 hooks, 4 writes.
- **Main menu**: `camera` printed `not_in_game: the main menu is open`, exit code 1.
- **Into a game**: the continue box loaded the Castile save of 1470.3.13. `camera` at the first game frame read 2848, 1415 at height 596, province 199 (Auvergne): the camera of the game's start, before the game moved it. A moment later it read 2745, 1317 at height 310, province 219 (Toledo), with Castile's capital in the middle of the screen; 310 is the height a game starts at.
- **To a province**: `camera look_at_province 183` answered 2841, 1490, province 183, height 310 kept, and the screen showed France with the map name 巴黎 (Paris) in the middle of the screen (`positions.txt` has 2842, 1494 for it). `camera` read the same right after.
- **To a position**: `camera look_at 3254 1343` (Constantinople in `positions.txt`) answered province 151 at 3254, 1343; `camera.zoom --height 1500` then answered height 1500 there, and the screen showed Europe and the Mediterranean with the Bosporus in the middle. At 3000 the known world was small in the middle of the screen; at 50 the screen showed the Sea of Marmara up close, a little south of Constantinople, as the plan says for a camera this low. Where the map wraps: at height 600, `look_at 5600 1000` answered province 1675 and `look_at 20 1000` province 1692 at x 20, the camera having crossed the edge, and the screen showed unexplored ocean; the corners 0, 0 and 5631, 2047 answered provinces 1723 and 1444.
- **Refused**: `zoom 10` and `zoom 3001` `invalid_argument: the height is from 50 to 3000`; `look_at 6000 10` and `look_at 10 2048` `invalid_argument: … is off the map, which is 5632 pixels across and 2048 up`; `look_at_province 0` and `99999` `not_found`. Exit code 1 each.
- **With the game running**: unpaused (1470.3.16 to 3.23), `look_at_province 219` and `zoom 900` answered as when paused, and the screen showed Iberia in the middle.
- **The player's own moves**: three turns of the mouse wheel took the height from 900 to 750, and six presses of the left arrow key x from 2745 to 2630 (province 1292, at sea); `camera` read both.
- **Costs**: the first `look_at_province` took 14.2 ms (it reads the provinces' map first); every other camera request under 0.05 ms. The core log has no error, and no extension was stopped.
- **Not seen in the running game**: a locked camera, which is refused (the game console's `debug_lockcamera` could not be typed: `tools/game-input.ps1` has no key for the game console); over high ground, the camera kept higher than the height asked for (Constantinople is low); an extension calling the camera, since none does (the SDK's tests call it through a capability table built as the core builds its own, and through an older core's table that ends before it); a map that does not wrap; a mod with other camera heights.
- **Cleanup**: loading the save rewrote `Castilla-5.eu4` and its backup, and `settings.txt` changed; those were put back from the backups, and both save folders and the three settings files compared equal to them. The game was ended from outside (its process stopped), and no crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups and screenshots were deleted.

## 2026-10-29 Screenshots (remote control C10, tested by me)

Every screenshot went through `eu4-harmony game screenshot --out`, built from this branch, and was compared with a capture of the screen by `tools/screenshot.ps1 -Scale 1`, the development tools' own way of looking at the screen (GDI's `CopyFromScreen` of the primary screen, DPI aware), with a Python script of the run's own (Pillow and NumPy, not kept) counting how far each pixel differs. `eu4.exe` was started directly; the game ran in a 3840x2160 borderless window filling the screen, with 8x multisampling and vsync at 144 Hz. Chinese playset with the unchanged regional-combat extension, system fonts; backups as above. The game was started six times in one sitting, each time with another build: the first read the frame back at the present after the copy, the second kept the surface in memory and timed each step, the third added a query after the transfer and flushed it, two more, for measuring only and not kept, timed the query and the lock apart, and the game's own Present, with and without the flush, and the last is this branch's.

- **Startup**: the game came up as before; `text.language` waited for the main menu (42 to 46 s) and printed `"simp_chinese"`.
- **Main menu**: `screenshot` printed `{"frame_height": 2160, "frame_width": 3840, "height": 2160, "image": {"bytes": 11878581, "file": ..., "media_type": "image/png"}, "width": 3840}` and wrote the PNG. The main menu's map moves, and fades in, at first: against captures of the screen taken just before and after, the moving map differed as much as the two captures differed from each other, and the framework's corner button ("EU4 Harmony 设置"), the framework UI, was the same pixel for pixel. Once the map had stopped, with the last build: the screenshot differed from the capture before it by 0.115 of 255 on average, 87.7% of the pixels the same and 99.8% within 8, and from the capture after it by 0.138; the two captures differed from each other by 0.185.
- **In a game**: the continue box loaded the Castile save of 1470.3.13, paused. The screenshot differed from the captures before and after it by 1.9 and 2.0 of 255 on average (0.3% of the pixels by more than 32), and the two captures from each other by 3.0 (0.7%): the differences are the animated water, clouds, rivers and units. Against the capture before, the minimap and the buttons below it were 99.6 and 99.9% the same pixel for pixel, the top bar 94.8% (0.3% off by more than 8), the see-through outliner 76% (0.4% off by more than 8); the country's shield, which shimmers, differed most. At 50% (1920x1080), against the capture shrunk to that size, 1.6 of 255.
- **Scale**: `--scale 50` gave 1920x1080 and `--scale 25` 960x540, both with `frame_width` 3840; `--scale 1` 38x22; `--scale 101` was answered `invalid_argument: scale is a percent from 1 to 100; 101 is not one` with exit code 1. Without `--out` the PNG came back as base64.
- **The main thread**: the log has each screenshot's steps ([research/screenshots.md](research/screenshots.md)). The first build spent 16.6 ms reading back, at the present after the copy; the second, which kept the surface, 12 to 68 ms locking it right after the transfer was queued, `GetRenderTargetData` itself taking 0.00 ms; the third 10 to 75 ms, which the measuring builds showed was the flush of the query right after the transfer (8 to 31 ms), the lock at the next present taking 0.002 ms. Without the flush, 12 screenshots with the last build spent 0.10 to 0.51 ms on the main thread, twice 5.2 and 9.9 ms in the copy at the frame's present; in the measuring build without the flush, the game's own Present at the transfer's frame took 9.9 to 11.1 ms instead of mostly 5 to 6, and that frame came 11.7 to 13.7 ms after the one before instead of 4 to 8.
- **Encoding**, on the screenshot's own thread: 3840x2160, 360 to 560 ms for a PNG of 8 to 12 MB; 50%, 107 to 160 ms for about 3 MB; 25%, 35 to 56 ms for 0.8 MB. Copying the pixels out of the locked surface took 4.8 to 16.2 ms there. From starting the command line to its answer: 0.6 to 0.9 s at 100%, 0.2 to 0.35 s at 50%, 0.1 to 0.2 s at 25%.
- **Not seen in the running game**: a screenshot asked for while a save loads, which should wait for the first frame after it; a device reset or a lost device while a frame is read back (the game ran in a borderless window, which keeps its device); a back buffer without multisampling or in another format; the settings window or a notice window in a screenshot, though the corner button showed the framework UI is in it; a screenshot given up by its client (unit test); two screenshots asked for at once, which share one frame (by the code; no test); an extension asking (unit test of the capability layer: refused); the hardware cursor, which neither the screenshot nor the capture of the screen holds.
- **Cleanup**: loading the save rewrote `Castilla-5.eu4` and its backup, and `settings.txt` changed; all three were put back from the backup, and both save folders and the settings files compared equal to it. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups, screenshots and scripts were deleted.

## 2026-10-29 A province's snapshot (remote control C3, tested by me)

`eu4-harmony game province.get`, built from this branch before C9's camera was merged into it, and for the extension side a probe of the run's own: a mod made with `eu4-harmony new` whose `on_session` logs `Game::province` for six numbers, linked and enabled in `dlc_load.json` for the run only. Chinese playset with the unchanged regional-combat extension, system fonts: the Chinese language mod gives the provinces their Chinese names (its localisation in legacy escapes, which the Unicode module decodes as it reads them), and Windows displays Chinese, so the player language is `simp_chinese`; nothing else had to be set up for a Chinese name. Backups as above; `eu4.exe` started directly; the continue box's Castile save of 1470.3.13, never unpaused. Two starts: the second after the fix to the reasons below.

- **Startup**: 384 dependency points match (17 new); 102 hooks, 4 writes; both extensions loaded.
- **Main menu**: `province.get 151` printed `not_in_game: the main menu is open`, exit code 1.
- **Against the game**, its province window, the map's tooltips and the recruit panels:
  - Toledo (219), Castile's capital: 托雷多, owner and controller `CAS`; tax 9, production 7, manpower 5; `grasslands` (the tooltip: 草原); fort level 3, not mothballed (the tooltip: level 3, a free capital fort, an active fort); buildings `fort_15th` (the panel shows the fort alone). Men-at-arms and Latin knights refused, "没有足够的manpower。": the top bar showed manpower 0, and the recruit panel's tooltip that a unit needs 1000 men. Artillery `unit_type` null, and the panel lists none. The four ships refused with no reason: Toledo is inland.
  - Constantinople (151), under Castile's siege: 君士坦丁堡, owner and controller `TUR`; 12, 11, 9; `farmlands` (农田); fort 3; `temple` and `fort_15th` (a mosque and the fort). Every kind refused, "卡斯蒂利亚不是君士坦丁堡的控制者。" first.
  - Seville (224): 塞维利亚, 6, 6, 4; `drylands`; fort 0; no buildings. The four ships allowed: its shipbuilding panel offers the early carrack, barque, galley and cog, the galley's tooltip "此处建造该类舰船。".
  - Also read, not compared on screen: Madrid (217), Stockholm (1), Gibraltar (226, `mountain`, ships allowed), the Øresund (1258, `inland_ocean`, no owner); 0 and 99999 `not_found`, exit code 1.
  - **A mixed name**: 1418 read `"霍特曼·阿布洛霍斯群岛"` in the JSON on standard output, the `·` from game text's 1252 byte and the rest from UTF-8, as the Chinese language mod's localisation decodes.
- **Extension**: the probe's log had the same six answers through `GameApi::province`, countries as `CountryId`s and `NotFound` for 0 and 99999; its `on_session` took 15 ms for them.
- **The reasons, fixed between the starts**: the first start read "没有足够的。" where the game's text is "没有足够的£icon_manpower£。", the icon standing for the word. `game_text::plain` now writes an icon as its name without `icon_`; the second start read "没有足够的manpower。", on the command line and in the probe's log.
- **Costs**: 0.1 ms in the engine for each `province.get`; 33 to 87 ms for each call of the command line, start to end. The core logs have no error.
- **Not seen in the running game**: the mixed name on screen (1418 is a sea zone off Western Australia, which the camera never went to); a mothballed fort; a province with no terrain; a regiment allowed (Castile had no manpower), and a refusal for money, sailors or a siege; a native country's capital, whose tribal development the getters add; the command's own rule for ships (`0x40d2b0`), which the snapshot does not ask. Special regiments and mercenaries are not in the snapshot.
- **Cleanup**: loading the save rewrote `Castilla-5.eu4` and its backup, and the probe changed `dlc_load.json`; the three were put back, the probe's `.mod` file and its logs deleted, and both save folders and the settings files compared equal to the backup. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll` (its hash as before the run); the backups, the probe mod and the screenshots were deleted.
- **Changed after the run, not run in the game**: the SDK now reads a snapshot in one call when it fits in 16 KiB, where the probe's build called twice, so the core built each snapshot twice (the 15 ms above); the SDK reads a kind it does not know as `UnitKind::Unknown`; `game_text::plain` takes `|` in an icon's name, as the game's text methods do. Unit tests cover each.

## 2026-10-29 The game console (remote control C4, tested by me)

Two runs. The first used `eu4-harmony game console run` and the core built from this branch before it was rebased onto C9 and C10, when `console_run` came right after `text` in `GameApi` and startup checked 370 dependency points, and a throwaway extension built with that build's SDK (`Console Check`, made with `eu4-harmony new` in a scratch folder, linked and added to `dlc_load.json` for the run) that ran `help cash` and `cahs 1` at each new session and `cash 7` on its second day, logging what came back. `eu4.exe` was started directly; Chinese playset with the unchanged regional-combat extension, system fonts; backups as above, `dlc_load.json` too.

- **Startup**: 370 dependency points match (the three new ones keep the game console's getter, its run function and its result); 102 hooks, 4 writes; both extensions loaded.
- **Main menu**: `console run "help cash"` answered `not_in_game: the main menu is open`, also once 1 s after the continue box was clicked, as F3 found for every request.
- **An ironman game**: the continue box loaded the Castile save of 1470.3.13, which is an ironman game. `help cash` was refused `rejected: Command not available in multiplayer or ironman mode.`, from the command line and from the extension alike; `print_unit_names` `rejected: Command available only for developers.`; `cahs 100` `rejected: Command not found. Did you mean 'cash'?`; `xyzzy` the same with `'piety'`; `cash 100 | out.txt` and a command of two spaces `invalid_argument`, without reaching the game. `console run -- --help` printed `"--compact" is one value too many` with exit code 2, since `--compact` after it was a value, and `console run --compact -- --help` sent `--help`, answered `Command not found. Did you mean 'help'?`.
- **A normal game**: after starting `eu4.exe` again, a new game as the Ottomans in normal mode. `help cash` printed `"cash [<Amount>] [<Target Country tag> OPTIONAL]\n - Adds gold to the player"`, exit code 0. The top bar showed 63 ducats; `cash 100` printed `"Gold changed by 100"`, and it showed 163. `cash` alone printed `"Now 奥斯曼 have cash!"`: the country's name, game text in UTF-8, came back as Unicode. `help` printed 10,520 characters in 342 lines; `help  cash` (two spaces, so an empty word, as in the window) `rejected: Invalid arguments count.`; `CASH 1` `"Gold changed by 1"`.
- **The extension**: at the ironman game's session its log had `help cash: Err(Rejected, "Command not available in multiplayer or ironman mode.")` and `cahs 1: Err(Rejected, "Command not found. Did you mean 'cash'?")`; at the normal game's, `help cash: Ok("cash [<Amount>] …")` and the same refusal; two days after unpausing, `cash 7: Ok("Gold changed by 7")`. The top bar then showed 5171 ducats: 163 + 5000 + 1 + 7, no month having passed.
- **The console window** was never opened: `console_history.txt` in the game folder was not touched, since the framework does not go through the window.
- **Costs**: each command took at most 0.6 ms in the engine. The core log has no error, and no extension was stopped.
- **Not seen in the running game**: a multiplayer game (its refusal shares the ironman one's flag and words); `enable_all_commands`, and a developer command with the lock off; commands that change the game's state at large, such as `tag` or `observe` (after `observe` the capability, needing a game with a player, would answer `not_in_game` until a game is loaded or started again); a command whose handler faults, which comes back `internal`; the extension's `console_run` with a null callback (only `Game::console`, which always passes one, was used).
- **Cleanup**: loading the ironman save rewrote `Castilla-5.eu4`; the new game was not saved. The game was closed by ending the process both times. Both save folders, the three settings files, `continue_game.json` and `dlc_load.json` were put back and compared equal to the backup; the link file `mod\eu4-harmony-console-check.mod`, the extension's log and the scratch mod were deleted. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups and screenshots were deleted.

The second run used the build of this branch rebased onto C3, the rebases having moved `console_run` behind the camera's fields and the province snapshot's in `GameApi`, and added their dependency points. It was brief, and did not repeat the command line's path: the same throwaway extension, rebuilt with that build's SDK, ran `cash 7` and `cahs 1` at the new session and `cash 7` again at its 60th frame. `eu4.exe` was started directly with `-start_tag=TUR`, which skips the main menu and starts a new game as the Ottomans, in normal mode; Chinese playset with the unchanged regional-combat extension, system fonts; backups as above, `dlc_load.json` too.

- **Startup**: 397 dependency points match; 102 hooks, 4 writes; both extensions loaded.
- **The extension**: its log had `cash 7: Ok("Gold changed by 7")` (0.5 ms), `cahs 1: Err(Rejected, "Command not found. Did you mean 'cash'?")` (0.1 ms) and, at the 60th frame, `cash 7: Ok("Gold changed by 7")` (0.4 ms). The top bar, in an `eu4-harmony game screenshot` taken then, showed 77 ducats: the 63 the Ottomans start with, and 7 twice. The core log has no error.
- **Not seen in the running game with this build**: everything else the first run tried. The game console's own code is the first run's but for the words of one error message; what changed is `console_run`'s place in `GameApi`, which the run above went through, and which the SDK's tests also check against a table built as the core builds its own.
- **Cleanup**: no save was added or changed: both save folders compared equal to the backup. `settings.txt`, `continue_game.json`, `eu4-harmony\settings.txt` and `dlc_load.json` were put back. The game was closed by ending its process; no crash folder was made. The link file and the extension's logs were deleted, among them `console-check.prev.log` from the first run, which its cleanup had missed. The framework was uninstalled, which put back the CJK font plugin's `version.dll` (its hash as before the run); the backups, the scratch mod and the screenshot were deleted.

## 2026-10-29 The country snapshot (remote control C2, tested by me)

Every request went through `eu4-harmony game` built from this branch; `eu4.exe` was started directly, with the playset's unchanged regional-combat extension and system fonts. Backups as AGENTS.md says; `continue_game.json` named a save that no longer exists, so for the run it was pointed at `Castilla-5.eu4` (Castile, 1470.3.13), which the continue box then loaded. What was compared is what the game shows: the top bar and its tooltips, the country view's court and diplomacy pages, the war view, and the Ottomans' diplomacy view, opened from the country list's search.

- **Startup**: 381 dependency points match (the 14 new ones keep the snapshot's fields, the wars and the war); 102 hooks, 4 writes.
- **Main menu**: `country get CAS` answered `not_in_game: the main menu is open`, exit code 1.
- **Castile's numbers**: `treasury` 213.793 against 213 in the top bar; `manpower` 0 against 0; `max_manpower` 30065 against the manpower tooltip's limit of 30,065 men; `stability` 0.0 against +0; `prestige` 80.398 against +80 (and +0.80 legitimacy from prestige in the legitimacy tooltip); `power` 565, 278, 481 against 565, 278 and 481 in the top bar and the court page. After running the game to 1470.5.15: 218.299 against 218, prestige 80.242 against +80, points 581, 288, 491 against the same, `max_manpower` 30068.
- **Castile's relations**: `ally` POL, HAB, PAP against the diplomacy page's alliances "波兰, 奥地利与教宗国"; `subject` BUR, GUY, TOU, ARA, NAP, TLC against its personal unions "勃艮第, 阿拉贡与那不勒斯" and vassals "加斯科涅, 图卢兹与特莱姆森", which the page shows on two rows; `rival` TUR against the rivals row, the Ottomans and two empty slots. The page's enemies row (the Ottomans and France) are countries that rival Castile, which a country's own snapshot does not list; the Ottomans' snapshot has `rival` CAS.
- **The war**: `name` "卡斯蒂利亚对奥斯曼的十字军", the war view's title; `attackers` CAS, BUR, GUY, TOU, ARA, TLC, NAP, the seven the war view lists, Castile first in both (the view orders the rest otherwise); `defenders` TUR, DUL, the view's two with the Ottomans starred as leader, and "交战中：奥斯曼*与杜勒卡迪尔" on the diplomacy page.
- **Another country**: the Ottomans read `stability` 1.0, `prestige` 46.788, `manpower` 3555, the same war, `ally` DUL and `rival` HAB, MAM, CAS; their diplomacy view showed stability +1, prestige 46, a manpower reserve of 3k, the war with Castile, an alliance with Dulkadir, rivals Austria, the Mamluks and Castile, and no subjects. Gascony and Aragon read `overlord` CAS; Poland `ally` BRA and CAS, `subject` MOL and LIT.
- **Errors**: `QQQ` `not_found: no country QQQ`; `C1` exit code 2 (not a tag), nothing sent. `ZZZ` is Zazzau and read as a country; `REB`, the rebels, read with no relations or wars.
- **Costs**: 0.9 to 1.3 ms each in the engine; the core log has no error, and no extension was stopped.
- **Not seen in the running game**: the extension side, `GameApi::country` and the SDK's `Game::country`, since no extension calls it (unit tests: the core's numbers and links, the SDK putting them back together against a table built as the core builds its own, a core whose table ends before it); a relation or a war changing between two reads (no alliance made or broken, no rival chosen, no war declared or ended), which C8 will do; another country's treasury and monarch points, which the game shows only for the player's country; a war name in another language; the observer's `not_in_game`.
- **Cleanup**: loading the save rewrote `Castilla-5.eu4` and its backup, and `settings.txt` changed; those and `continue_game.json` were put back from the backups, and both save folders and the three settings files compared equal to them. No crash folder was made. The framework was uninstalled, which put back the CJK font plugin's `version.dll`; the backups and screenshots were deleted.

## How the implementation differs from the final design

- Binding the notice window again after its host window is rebuilt (for example by `reload`) is not tested yet; the top bar binds, but how notices look in the top bar has not been confirmed on its own.
- The button area is a third column to the right of the two vanilla button columns (x=464, on the vanilla 43×44 grid), with no background frame.
- A text box is "hidden" by clearing its text, not by calling the text box's show/hide method (which is not verified on this build).
- Route queries, moves and stops are all refused while the game is paused (`NOT_READY`), as in the old regional-combat.
- Map coloring borrows native map mode 12; provinces that are not colored show that mode's own colors.
- When the user directory cannot be found, logs fall back to `eu4-harmony\logs\` in the game directory.
