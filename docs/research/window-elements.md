# Window elements beyond text and buttons: frames, sprites, progress bars, flags, lists and tooltips (for extension windows)

Date 2026-11-09. Subject: vanilla 1.37.5 `eu4.exe`, image base `0x140000000`. All addresses are **RVAs**.

Method: static analysis only. The game was not started and no process was attached. Ghidra 12.1.4 headless, opened with `-readOnly -noanalysis`, decompiled the selected functions. Vtables, RIP-relative references, `E8` calls, the interface keyword table and `.pdata` function bounds were read with scripts written for this study. The vanilla `interface/*.gui` and `*.gfx` files were read as text. What is marked **verified** was seen in the decompilation or the bytes. What is marked **inferred** is a judgement drawn from structure. Prototypes are tentative. Nothing here has been tried in the running game.

This continues [interface-bindings.md](interface-bindings.md), which covers finding elements by name, button clicks, text, show and hide, enabling and the army panel's tooltip. The windows themselves are covered in [notice-window.md](notice-window.md) (CreateWindow, Kill, the sweep), and how definitions get in is in [ui-injection.md](ui-injection.md). The plan this serves is [extension-windows.md](../plans/extension-windows.md), W3.

## Conclusion

- **Frames: verified.** One method sets a frame on both buttons and icons: element `vtbl+0xa8(element, i32 frame)`. The panel button strip (`0x830700`) and the siege view (`0x13afda0`) both use it. Frames count from 1 (inferred).
- **Progress bars: verified.** A progress bar is an `iconType` whose sprite is a `progressbartype` from a `.gfx`. The same `vtbl+0xa8` gives it its value, an integer from 0 to 100 that the sprite clamps at 100. The game passes `(int)(fraction * 100.0)`.
- **Sprites by name: verified for buttons, inferred for icons.** `vtbl+0x2b0(element, std::string* name)` changes an element's sprite by name. It does nothing when the name is already the element's sprite, or when no sprite has that name.
- **Flags: verified.** A flag is a `guiButtonType` with a `maskedShieldType` sprite such as `GFX_shield_thin`. It is given its country with `vtbl+0x218(button, &{Tag, u8 1, u8 0})` and then `vtbl+0x1b0(button, index)`. Here `Tag` is the game's 8-byte country tag, the core's `game::Tag`. The tag's country index is used as an index into the countries array without any check, so it must name a country that exists.
- **Scrolling lists: verified.** Use `listboxType` with `scrollbartype`. Each row is a listbox entry of the game's own base class (0x50 bytes), which builds its row window from a `windowType` by name. The list takes rows with `vtbl+0x20`, and clears them with `vtbl+0xb8`, which deletes every row. The list owns its rows, and a row's destruction kills its window. `gridboxtype` works the same way with an entry of 0x18 bytes, but it has no scrollbar of its own.
- **Tooltips**:
  - **Fixed text: inferred.** The `tooltipText` and `delayedTooltipText` keywords take a localisation key in a button's or icon's definition. Vanilla does this, for example with `"GOTO"`.
  - **Text that changes: partly verified.** An element's tooltip provider is at `element+0x58` and is set through `vtbl+0x220`. The HRE's elector rows give their elements a provider of their own, whose first method fills a `std::string`. The code that calls a provider was not found.

What would change the plan: nothing in its shape. Three things need care.

- **The flag game widget** must refuse a tag whose country does not exist.
- **A text box's tooltip**: no vanilla text box has one, and none was found.
- **Tooltips that change while shown** need a framework provider, which is not verified. A fixed tooltip key per game widget is safe. A tooltip that a mod sets at run time needs the provider tried in the game first.

## The element classes

| `.gui` keyword | Token | Live class (vtable) | Size | Where the constructor sets the class | Found in a window by |
| --- | --- | --- | --- | --- | --- |
| `guiButtonType` | `0x7f` | `0x1d58218` (the game's button; [interface-bindings.md](interface-bindings.md)) | | `0x10b55fc` | `vtbl+0x68`, a C string |
| `iconType` | `0xb6` | Game class `0x1d585c8`; base class `0x1d96d00` | Game `0x1f0`; base `0x1e8` | Game constructor `0x10b5780` replaces the base vtable at `0x10b5823`; base constructor `0x15857d0` sets it at `0x15857fc` (`48 8d 05 fd 14 81 00`) | `vtbl+0xc8` (`0x15492e0`), a C string |
| `listBoxType` | `0xa9` | `0x1d90ea0`, its element interface at `+8` (`0x1d90fd8`) | `0x2c0` | `0x152e2bc` (`48 8d 05 dd 2b 86 00`), constructor `0x152e220`, made by `0x15ebbf0` | `vtbl+0x1a8` (`0x1548420`), a `std::string*` |
| `gridboxtype` | `0x275` | `0x1d97ef0` | `0x1a8` | `0x15907ef` (`48 8d 05 fa 76 80 00`), made by `0x1590770` | `vtbl+0x1b8` (`0x1548480`, `48 81 c1 98 08 00 00 48 8b 01 48 ff 60 08`), a `std::string*` |
| `smoothListboxType` | `0x1fb` | `0x1d967d0` | `0x298` | constructor `0x1583000`, made by `0x15ebd10` | `vtbl+0x1a0` (`0x1548490`, `48 81 c1 d8 08 00 00 48 8b 01 48 ff 60 08`) |
| `progressbartype` (`.gfx`) | `0x112` | a sprite, `0x1d9d3c8` | `0x218` | `0x15d8b1b` (`48 8d 05 a6 48 7c 00`), made by its type's `vtbl+0xa8` (`0x15d8ab0`) | it is an icon's sprite |
| `maskedShieldType` (`.gfx`) | `0x10f` | a sprite, told apart by its type's id at type `+8` | | | it is a button's sprite |

Correction, 2026-11-14: the original icon row identified the base constructor's class as the final live class. The game's definition factory replaces the definition's factory method, and its live constructor replaces that base vtable. The complete path and its consequence for attached button areas are in [window-placement.md](window-placement.md#game-icons-have-a-derived-class).

The token numbers come from the keyword table (`{i32 token, char name[]}`, for example `guiButtonType` at `0x1fe67dc` with `0x7f` before it). The window's constructor `0x1543f80` builds a window's children from linked lists of their definitions. It reads them at `this+0x9d0`, `+0x9e8` and `+0xa00`, whose origin was not traced:

- **list boxes**: from `+0x9d0`, through `0x15ebbf0`, entered in the window's map at `window+0x858`;
- **grid boxes**: from `+0x9e8`, through `0x1590770`, map `+0x898`;
- **smooth list boxes**: from `+0xa00`, through `0x15ebd10`, map `+0x8d8`.

Each finder above looks up its map, and `0x1548420` returns 0 when nothing has that name. Verified. Unlike the button and text box finders, these were not seen returning a stand-in. `0x1548420` returns whatever its map's lookup (`0x1548450`) gives, which is 0 on a miss.

Every allocation the game frees must come from the game's own allocator: `operator new` at `0x1a332d4` (`40 53 48 83 ec 20 48 8b d9 eb`), with the size as its only argument. The game frees with `0x17394f0(pointer, size)`.

## 1. Frames and sprites

- **Setting a frame (verified).** Element `vtbl+0xa8(element, i32 frame)`.
  - **Buttons** (`0x15aeda0`: `89 91 70 02 00 00 48 8b 89 90 02 00 00 0f bf d2 48 8b 01 48 ff a0 a8 00`): writes the frame to `button+0x270`, then calls its sprite (`+0x290`) `vtbl+0xa8` with the frame as an i16.
  - **Icons** (`0x1585ed0`: `48 8b 89 68 01 00 00 48 8b 01 48 ff`): forward to their sprite at `+0x168`.
  - **Callers**: the panel button strip's `0x830700` uses it on its buttons, and the siege view's `0x13afda0` on the icons `morale`, `defender_morale` and others, with the frames 1, 2 and 3.
- **Frames count from 1 (inferred).** In the vanilla `.gui` files, `frame = 1` is the most common value (74 times) and `frame = 0` never occurs. `GFX_menubar_floating_strip` has `noOfFrames = 10`, and the strip uses frames 1 to 10.
- **Reading the frame**: `vtbl+0xb0` (`0x15aedc0` for buttons, `0x15722d0` for icons) asks the sprite. Not needed.
- **Changing the sprite by name.**
  - **Buttons (verified)**: `vtbl+0x2b0`, `0x15ad950(button, std::string* name)` (`48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec`).
    - It returns at once when the button's sprite type already has that name.
    - Otherwise it looks the sprite type up with `0x14df580(button+0x168's manager, name)` and returns, changing nothing, when there is none.
    - Otherwise it puts the old sprite back (`0x14db6a0`), makes the new one (`0x14db940` or `0x14dbdc0`), and keeps the old sprite's position and scale.
    - The HRE's reform rows call it with `"GFX_" + the reform's name` (`0x1164790`, after looking up `reform_button`).
  - **Icons (inferred)**: `vtbl+0x2b0`, `0x15864b0` (`48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 57 41 54 41 55 41 56 41 57`), does the same lookup through `0x14df580`. It keeps the frame the old sprite had (old sprite `vtbl+0xb8`). That its second argument is a `std::string*` is inferred from the lookup it shares with the button's method, because the decompiler lost that argument.
- **Showing and hiding icons (verified).** Icons have the same show and hide as buttons.
  - `vtbl+0x88`, `0x1585e80` (`80 b9 e1 01 00 00 00 75 1f c6 81 c5 00 00 00 01`): unless `+0x1e1` is set, it sets `+0xc5` and shows the sprite.
  - `vtbl+0x90`, `0x1585eb0` (`c6 81 c5 00 00 00 00 c7 81 94 01 00 00 00 00 00`): clears it.

## 2. Progress bars

- **Definition (verified).** No vanilla `.gui` has a progress bar element. A progress bar is an `iconType` whose `spriteType` names a `progressbartype` from a `.gfx` file. Vanilla has 70 or so, such as `GFX_default_progressbar`, `GFX_combat_morale` and `GFX_technologyView_progressbar`, in `general_stuff.gfx` and others. Each has the keys `textureFile1` and `textureFile2` (or two colors), `size`, and `effectFile = "gfx/FX/progress.lua"`. An extension window that wants a bar of another size needs a `progressbartype` of that size: from the mod's own `.gfx`, or written by the framework among its framework files.
- **Setting the value (verified).** Icon `vtbl+0xa8(icon, value)`, as for a frame. The live progress sprite's `vtbl+0xa8` (`0x15d90d0`: `48 8b 01 48 ff a0 a0 00`) jumps to its `vtbl+0xa0`. That is `0x15d90a0` (`0f b7 c2 ba 64 00 00 00 3b c2 0f 47 c2 89 81 a8`), which takes the value as a u16, clamps it at 100 and stores it at `+0x1a8`.
- **What the game passes (verified).** The siege view computes `(int)(fraction * 100.0)`, with the float 100.0 at `0x1dc18c8`, and passes that, for example for `defender_morale`. So a bar is filled by an integer from 0 to 100. A negative value becomes a large u16 and is clamped to 100, so the framework clamps to 0 first.

## 3. Country flags

- **Definition (verified).** The HRE's `emperor_shield` and `elector_shield` are `guiButtonType`s with `quadTextureSprite = "GFX_shield_thin"`, whose type in the `.gfx` is a `maskedShieldType` (token `0x10f`). The vanilla `shieldtype` element (`eu3dialog.gui`'s `LeftShield`) is another way, and was not read.
- **How the game sets one (verified).** In the elector row's constructor, `0x1168600(row, panel, Tag)`:
  1. The row keeps the `Tag` at `row+0x38`.
  2. It finds `elector_shield` with the row window's `vtbl+0x68`.
  3. It calls the button's `vtbl+0x218` with a pointer to 16 bytes on its stack: the `Tag` (u64), then a u16 1, so byte 8 is 1 and byte 9 is 0.
  4. It calls `vtbl+0x1b0(button, (i32)(i16)(Tag >> 32))`, the country's index.
- **`vtbl+0x218` on a button.** It is `0x10b56b0` (`48 85 d2 74 5f 48 89 74 24 18 41 56 48 83 ec 30 48 8b 01 48 8b f2 48 89`). Given a null pointer it does nothing. Otherwise it reads bytes 8 and 9, gets the button's sprite (`vtbl+0x2c0`), and calls `0x10b4f50(button+0x298, tag, button, sprite, byte 8, byte 9)` (`48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 41 56 48 83 ec 40 66 0f 6f`). `0x10b4f50` then:
  - stores the tag at `button+0x298`;
  - calls the button's `vtbl+0x130` (`0x15aed40`, which tells its sprite and writes `+0x27c`) with the index;
  - reads the country as `countries[index]`, with `countries = *([0x233d8d0] + 0x118)`, and **without a bound check**;
  - when the sprite's type is a masked shield (`+8` is `0x10f`), sets the shield's colors from the country, its overlord or its colonial region. The two bytes choose which of these the shield shows. Their meaning is inferred, and the framework passes 1 and 0 as the game does.
- **`vtbl+0x1b0`** (`0xfaab90`: `89 91 38 01 00 00 c3`, shared by icons and buttons) only writes `+0x138`. What reads it was not looked for; the framework calls it as the game does.
- **The tag** is the game's 8 bytes, as `objects::campaign::tag` describes: the three letters, the index (i16) at bit 32, the kind at bit 48 and a valid byte at bit 56. The framework takes it from the country it names, for example through `game::countries`, and never builds one from the letters alone. The index has to be valid.

## 4. Lists

### `listBoxType`, which scrolls (recommended)

- **Definition**: `listboxType = { name, position, size, scrollbartype = "standardlistbox_slider", … }`. For example, the HRE's `previous_incidents_container` scrolls its rows with its own scrollbar, which the definition brings.
- **Rows (verified)** are listbox entries of 0x50 bytes, built by the base constructor `0x152df80(entry, gui manager, std::string* row name)` (`48 89 5c 24 10 48 89 74 24 18 48 89 4c 24 08 57 48 83 ec 20 49 8b f0 48`):
  - It sets the classes `0x1d65360` at `+0` and `0x1d4a140` at `+0x38` (`0x152dfbd`, `0x152dfc7`).
  - It looks the name up in the type library (`mgr+0xc8`). For a `containerWindowType` (`0x264`) it uses the manager's `vtbl+0x58`; otherwise it calls CreateWindow (`vtbl+0x48`).
  - It keeps the window at `entry+0x40`.
  - The gui manager is the one the incidents pass, `*(map manager + 0x58)`, the same as `*0x23494f0` (inferred).
  - The incidents' row (`0x116b1b0`) is a subclass that builds this base with `"hre_previous_incident"`, a top-level `windowType`. The base class itself is concrete: its methods are real functions or `ret 0` (`0x955a0`).
  - So the framework can allocate 0x50 bytes with `0x1a332d4`, build this base with the name of a row `windowType` it wrote, and hand it to the list.
- **Adding (verified).** List `vtbl+0x20`, `0x152e910(list, entry, u8 lay out)` (`48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 40 45 0f b6 e0`):
  1. It takes the row's window in: the window's `vtbl+0x138`, then `0x15621f0`.
  2. It appends a node of 0x20 bytes `{entry+0x38, previous, next, u8}` to the list's rows, which are at `list+0xe0` (first), `+0xe8` (last) and `+0xf0` (count, i32).
  3. It tells its inner list (`+0xf8`) of the row.
  4. When the third argument is set, it lays the rows out (`vtbl+0xd8`).

  The incidents pass 1 for every row (the loop after `0x115c383`).
- **Clearing (verified).** List `vtbl+0xb8`, `0x15320f0(list)` (`48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 48 8b f1 33 ed 89 a9 88 02`):
  1. It empties the inner list.
  2. Through `0x152df00` it deletes every row through its node: the interface's first method with 1, a thunk (`0xfd2f98`, `48 83 e9 38 e9 …`) to the deleting destructor `0xfc6d40` (`48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 dc 72 56 00`).
  3. It frees the nodes and lays the list out.

  The row's destructor `0x152e030` kills the row's window (`vtbl+0x130`, Kill `0x1547520`). So **the list owns its rows**: once given to it, a row is freed by the next clear, or by the list's own destruction (inferred), never by the framework.
- **Counting**: `vtbl+0x60` gives an object whose i32 at `+0x10` is the number of rows, which the incidents compare with their own count. `objects::gui::list_box::ENTRIES` (`vtbl+0x58`) is the first node.
- **Scrolling (verified from the incidents).** The list's scrollbar is at `list+0x1e8` (null without one), and its interface is at `scrollbar+0xc8`:
  - `vtbl+0x20` gives the position as a float;
  - `vtbl+0x38(interface, f32 position, 0, 1)` sets it.

  The incidents keep the position before clearing and put it back after refilling, when it was above the float at `0x1dc1a10`.
- **A row's elements**: looked up in the row's window (`entry+0x40`), with that window's `vtbl+0x68`, `+0x98` and `+0xc8`, as in any window. That window is built by CreateWindow from its own definition, so it has the window class `0x1d926b0`.
- **Clicks in rows**: every row's buttons are objects of their own. The framework subscribes one of its callbacks to each, as `ui.rs` does, and keeps which row and game widget it belongs to. The game has nothing that says which row a button is in.
- **Lifetime**: a clear kills each row's window through Kill, so `ui.rs`'s observer on `0x1547520` sees every row go. A binding to a row's button is retired there, before the sweep frees the buttons.
- **Refilling** is a clear followed by adding every row again, as the incidents and the HRE do on each update.

### `gridboxtype`

- **Definition**: `gridBoxType = { name, position, size, slotsize, max_slots_horizontal, max_slots_vertical, format }`. It has no scrollbar keyword. The HRE scrolls one by putting it in a `smoothListboxType`: `hre_members` holds one row, `hre_members_grid`, whose `gridbox` is filled.
- **Rows (verified)** are entries of 0x18 bytes, built by `0x158ef10(entry, gui manager, std::string* row name)` (`48 89 5c 24 10 48 89 74 24 18 57 48 83 ec 20 48 8d 05 02 37 7d 00 48 8b`):
  - It sets the class `0x1d62628` (`0x158ef1f`).
  - It builds the row window as above and keeps it at `entry+0x10`; `+8` is the window's position (from the window interface's `vtbl+0x98`).
  - The deleting destructor `0x116a4b0` (`48 89 5c 24 08 57 48 83 ec 20 8b fa 48 8b d9 48 8d 05 62 81 bf 00 48 89`) kills the window and frees 0x18 bytes.
  - The HRE's electors (`0x1168600`, `hre_elector`) and members (`0x1167550`) are subclasses of 0x40 bytes.
- **Adding**: `0xf2bed0(grid, entry, u8 lay out)` (`48 83 ec 38 33 c0 45 0f b6 d0 4c 8b da 48 89 44 24 40 38 81 71 01 00 00`):
  - It returns 0, without taking the row, when the grid is full (`max_slots_*` at `+0x160`/`+0x164`) or when the row is already in it.
  - Otherwise `0xf2c130` appends the row to `+0x190`..`+0x198` and its slot to `+0x178`..`+0x180`, shows the row when the grid shows, and lays out when asked.
- **Clearing**: `0xf4cfb0(grid)` (`48 89 5c 24 08 57 48 83 ec 20 48 8b d9 33 ff 48 8b 89 90 01 00 00 48 8b`) deletes every row (its method `+0` with 1) and empties both vectors. The grid owns its rows, as the list does.

### `smoothListboxType`

Found with `vtbl+0x1a0`. Its adding (`vtbl+0x20`, `0x1583c30`) and clearing (`vtbl+0xb8`, `0x1584340`) are its own and were not read. The HRE uses it only to scroll a grid.

## 5. Tooltips

- **What `ui.rs` has**: an observer on the army panel's own tooltip function `0x1453b70(panel, out, element)`. That function is reached only from the in-game tooltip dispatcher `0x1471ee0(this, out, element, …)`, which `0xb7caf0` calls. `0xb7caf0` is the map manager's tooltip function: it walks the element's name through a long chain of comparisons. Its `out` is 0x4c bytes: two game strings (the text at `+0`, the delayed text at `+0x20`) and `0x1000001` at `+0x40`. None of this reaches a window the framework builds, so extension windows need one of the two ways below.
- **Fixed text (inferred)**: the keywords `tooltipText` (token `0x93`) and `delayedTooltipText` (`0x94`) in a `guiButtonType`'s or an `iconType`'s definition take a localisation key, as in vanilla's `tooltipText = "GOTO"`. The framework writes them into the definitions it writes. No vanilla text box has them.
- **A provider (partly verified)**:
  - **Where it lives**: every element keeps a tooltip provider at `element+0x58`. It is set with `vtbl+0x220` (`0xfbf490`: `48 89 51 58 c3`) and read with `vtbl+0x228` (`0x8650f0`: `48 8b 41 58 c3`).
  - **Inheritance**: a window that takes a child gives it its own provider (`0x154b2f0`, `0x154ac90`: `child vtbl+0x220(window interface vtbl+0x228())`).
  - **The elector row's own**: it hands its buttons and text box its own interface at `row+0x18`, whose class `0x1d62618` has one method, `0x1168da0(provider, element, std::string* out) → u8`. That method compares the element with the row's own elements (`row+0x20`, `+0x28`, `+0x30`), fills `out` for the one it knows, and returns 1.
  - **The caller** of a provider's method was not found, so this signature is the elector row's alone. That the game calls it when the pointer rests on such an element is inferred from those shields showing tooltips in the game.
  - **What it would mean for the framework**: a provider of its own, a vtable of one method in its own memory, set on each element whose tooltip a mod sets at run time. This has to be tried in the game before the plan relies on it.

## Not found or not verified

- None of this was tried in the running game.
- Where a provider's method is called from, and with what. Whether it fills the tooltip's heading or only its text.
- What reads `button+0x138` (`vtbl+0x1b0`), and what bytes 8 and 9 of the flag's argument choose.
- That the icon's `vtbl+0x2b0` takes a `std::string*`; that frames count from 1; that a list's destruction deletes its rows.
- The smooth list box's own add and clear, and the vanilla `shieldtype` element.
- Text boxes' show and hide (`vtbl+0x88` `0x152ac90`, `vtbl+0x90` `0x152ad10`), which `ui.rs` still does not use.
