# Windows of the framework's own, the game's big panels and the panel button strip: a static study

Date 2026-11-09. Subject: vanilla 1.37.5 `eu4.exe` (SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`, image base `0x140000000`). All addresses below are **RVAs**.

Method: static analysis only; the game was not started and no process was attached. Ghidra 12.1.4 headless, opened with `-readOnly -noanalysis`, decompiled only selected functions; disassembly with capstone, `.pdata` function bounds, RIP-relative references and `E8` calls scanned with scripts written for this study; the vanilla `interface/menubar.gui` read as text. What is marked **verified** was seen in the decompilation or the bytes; what is marked **inferred** is a judgement drawn from structure. All prototypes are tentative.

It builds on [ui-injection.md](ui-injection.md) (how `.gui` files are read), [notice-window.md](notice-window.md) (CreateWindow, the idler, Kill and the sweep, the window holder) and [interface-bindings.md](interface-bindings.md) (elements, clicks, text, tooltips), and does not repeat them. It is the research for [docs/extension-windows.md](../extension-windows.md), W1, W2 and W4.

## Conclusion

- **Window definitions after startup** (verified): `0x14eb7d0(mgr, const std::string* path)` reads one `.gui` file through the same CLexer and PhysFS path as startup, so a framework file mounted in the game's file system is found, and with the override flag set a definition of the same name replaces the old one. It does not tell the game's reload listeners, so no vanilla window is rebuilt.
- **A window of the framework's own** (verified): CreateWindow builds it hidden, top level, oriented to the screen; GUI `+0x88` shows it. It draws once built; its per-frame update (animations, `show_position`) runs only when it is put in the idler's window list (`0x6ecbd0`). Destroying it is the holder's order: off the idler's list, hide, Kill; the manager's sweep frees it in the same frame.
- **Place on screen** (verified): `root+0x598` holds the window's position (x, y, i32, GUI units from the screen's top left), which dragging changes. Root `+0x30` (`0x15472e0`) moves it to an absolute position; root `+0x38` (`0x1547370`) places it by its orientation, and also shifts its show and hide positions each time it is called, so it is not for moving a window more than once.
- **Dragging** (verified): every window the constructor builds is wired to the drag function `0x1547170`, which moves it by the mouse's delta, clamped to the screen, only while `root+0x5b0` (`moveable`) is set. The framework reads the dragged place back from `root+0x598`.
- **Above the game's panels** (inferred): hit testing walks the scene's nodes from the last to the first, so the window made last takes the click; no function was found that brings a window to the front.
- **The game's big panels** (verified): `0x818080(idler)` closes the views that cover the map and `0x80c6e0(idler)` closes every big panel; every opener calls both first. An observer on `0x80c6e0` hears every such closing, the framework's own calls included.
- **The panel button strip** (verified): after `0x80c9f0(idler)` the used slots are `f_button_1..n`, the first `n` shown ones; the next entry button stands at `x = -360 - 45 * n`, `y = -58`, `LOWER_RIGHT`. A button's `+0x288` (`0x15ae930`) places it by its orientation every frame; an icon's `+0x288` (`0x1571cf0`) takes a position already resolved.
- **Text and tooltips**: `TEXT_SET` fits the text into the box (wraps to its width, cuts at its lines, or grows a scrolling box's lines) (verified); the formatting codes are read when the text is drawn (inferred). Every in-game tooltip goes through the idler's `0xb81870(idler, out, element)` (verified), where an observer can fill the tooltip of any of the framework's elements as `ui` does for the army panel's (inferred).

## 1. Reading an interface file after startup

`0x14eb7d0(GuiManager* mgr, const std::string* path)`, `void` (verified):

1. Sets `mgr+0x228 = 1`, the override flag (ui-injection.md §4: with it set, a definition whose name is already in the type library replaces the old prototype, which is deleted, instead of "Already Defined").
2. Copies the path (`path->size` and the heap pointer when `capacity > 15`, MSVC `std::string`: 16 bytes of buffer or pointer, size at `+0x10`, capacity at `+0x18`) into a string of its own with `0x95110`; the caller's string is only read and stays the caller's to free.
3. `0x170dd70(&lexFile, &string)`, which calls the CLexer file constructor `0x170cd10` (verified), the one that opens every script file through `0x1718300` → `PHYSFS_openRead` (ui-injection.md §3). So the file is looked for along PhysFS's search path, where the framework files are mounted (ADR framework-files-in-the-game-file-system).
4. `mgr->vtbl[+0x18](mgr, &lexer)`: the generic persistence read, which hands each top-level token to the manager's handler `0x14ea9f0`; only `guitypes` blocks make window definitions. Sprites (`.gfx`) are not read this way.
5. `0x14eaaa0(mgr)`: tidies the type library (walks its tree with the callback `0x14ee2b8`), looks up again the four names it keeps at `mgr+0x20`, `+0x40`, `+0x60`, `+0x80` in the registry at `mgr+0x220` (the sound registry, which the windows' show and hide also use) into `mgr+0xa0..+0xb8`, and calls `mgr->vtbl[+0xc0]` (`0x10b4f40`).
6. Clears `mgr+0x228`.

Its only caller is the console's `reload` (`0x14e1140` at `0x14e13b8`), which then finds the file's reload listener and calls it; `0x14eb7d0` itself calls no listener (verified).

Thread (inferred): the console runs it on the main thread; it changes the type library and opens a file inside the global spin lock `*0x242b9c0`, so the framework calls it on the main thread at a safe point, not from inside a GUI callback.

Replacing a definition a live window was built from (not verified): whether an instance keeps a pointer to its prototype was not traced. The safe course is a fresh name for every definition the framework writes again (an extension reload's), or no instance of it alive when it is read.

## 2. A window of the framework's own

### Building it (verified)

- `CreateWindow` `0x14e9030(mgr, const std::string* name)` (notice-window.md §1) calls `0x155c260(proto, scene, mgr, *(scene+0x490), 0)` with `scene = *(mgr+0xc0)`, so the window has no parent and its orientation is reckoned against `scene+0x490`, the screen's size (i32 width and height in GUI units, inferred from this use and from the drag's clamp below). It returns the window (`root`), registered with the manager as `root+0x30`.
- The constructor `0x1543f80` clears the visible byte `root+0xf5` and places the window once with `0x1547370` from the definition's `position` and `Orientation`; nothing in it shows the window. The game's owners show it themselves: the top bar with GUI `+0x88` right after CreateWindow.
- Show and hide: GUI `+0x88` `0x1547d90` and `+0x90` `0x1547b00` (notice-window.md §4). They set `root+0xf5`, show or hide the background, notify the window's listeners (`0x154e500`) and play its sounds (`0x16a7d00` on `mgr+0x220`). The root's own `+0x40` `0x1547cc0` and `+0x48` `0x1547a50` set the byte and the background without the rest.
- Drawing needs no per-frame call (inferred): the constructor makes the window's scene node (`root+0xa30`), which the scene draws while it is shown.
- **Per-frame update**: the manager updates only what is on `mgr+0x170` (in its update `0x14e9fa0`, each with `vtbl[+0x240](dt)`), and `0x14eb360` puts a registered object there only when its `+0x238` says so, which a window's GUI subobject does not (`0x8f450`). A window's own update, root `+0x238` `0x1546ac0` (the slide between `show_position` and `hide_position`), runs when its owner puts it on the idler's list, `0x6ecbd0(idler, root)`, and stops with `0x6ecc80(idler, root)` (notice-window.md §3.1).

### Destroying it (verified)

The window holder's order (`0x149aca0`, notice-window.md §3.4): `0x6ecc80(idler, root)` if it was put there, GUI `+0x90`, root `+0x130` Kill (`0x1547520`). The manager's update `0x14e9fa0`, at the end of `0x14c5810` in the same frame, deletes every registered object marked dead (`+0xc4`) and drops it from `mgr+0x110`; the root's destructor `0x1546730` takes its node out of the scene (`0x14db6a0`). The framework's existing Kill observer in `ui` sees it.

The game's own window holder (`{idler, root, std::string name}`: `0x149ad00` builds the root from the name, `0x149ade0(holder, idler)` puts it on the idler's list, `0x149aca0` destroys it) does all of this for a window of one name; the framework may keep its windows in holders of its own and call these.

### Its place on screen (verified)

| | What | |
|---|---|---|
| `root+0x598` | position, x and y (i32) | GUI units from the screen's top left; GUI `+0x98` (`0x154a7b0`) returns it (`gui+0x568`) |
| `root+0x52c`, `+0x534` | `show_position`, `hide_position` | absolute, made so by `0x1547370` |
| `root+0x53c`, `+0x544` | where a slide starts and ends | written by root `+0x240` `0x1546d50` |
| `root+0xa38` | orientation, the enum below | |
| `root+0xa78`, `+0xa7c` | the size it is oriented against (i16) | |
| `root+0x5b0` | moveable (u8) | root `+0x58` `0x1546a30` sets it |
| `root+0x564` | slide state: 0 still, 1 showing, 2 hiding | |

- Root `+0x30` `0x15472e0(root, i64 pos)`, `pos = x | y << 32`: writes `root+0x598` and moves the scene node to `pos + screen/2` (the scene's coordinates are centered). Absolute: what the framework uses to put a window back where it was dragged, or to cascade.
- Root `+0x38` `0x1547370(root, i64 pos)`: adds the orientation's anchor (below) to `pos`, adds the same anchor to `root+0x52c` and `+0x534`, makes `+0x53c` and `+0x544` the result and calls `+0x30`. The constructor calls it once. Called again it shifts the show and hide positions a second time.
- The orientations (`0x152dce0`, the same for elements, verified from its jump table): 0 upper left `(x, y)`; 1 upper right `(w + x, y)`; 2 lower left `(x, h + y)`; 3 lower right `(w + x, h + y)`; 4 left `(x, h/2 + y)`; 5 right `(w + x, h/2 + y)`; 6 center `(w/2 + x, h/2 + y)`; 7 center up `(w/2 + x, y)`; 8 center down `(w/2 + x, h + y)`. Which name in `.gui` is which number was not traced; `LOWER_RIGHT` is 3 (inferred: the strip's buttons, at negative x and y against a full-screen window, show on screen only if both the width and the height are added, section 5).

### Dragging (verified)

The constructor builds a callback (class `0x1cb8150`, the window as its context, the function `0x1547170`; `lea` at `0x1544149`) for the window's drag. `0x1547170(root, _, int* now, int* before)` returns at once unless `root+0x5b0` is set; otherwise it adds the mouse's delta to `root+0x598` and `+0x59c`, keeps the window on the screen (against `scene+0x490` and the window's size from root `+0x158`, with the margin `root+0x5b4`), and calls `+0x30`. So a `moveable = yes` window built by CreateWindow drags as the game's do, and where it was dragged is `root+0x598`. That vanilla's moveable panels are dragged through it is inferred.

The mouse handler of the window's GUI subobject (`+0x48` `0x1546a50`) only notifies the window's listeners on a release (`0x15a4610`).

### Screen size

- `*(*(mgr+0xc0) + 0x490)`: width and height in GUI units (inferred, section above).
- `*(0x23494b8) + 0x488`: width and height in pixels, which the idler's tooltip update compares the mouse with (`0x81f0a9`; inferred). `objects::gui::globals::SCALE` is the same global.

## 3. What draws above what

- Hit testing (`0x14ea630`, `0x14de440`) asks `0x14dda50(scene, scene+0x298, point, out)` for the nodes under the pointer in the order of the scene's list `scene+0x298`, then walks them from the last; the first shown and live element takes the input (verified). Keys go the same way through `scene+0x70`, last first, to the elements with a shortcut (`+0x8d`), until one takes it (`0x14ea150`, verified): so the window made last also takes Esc first.
- A window's node is added to the scene when it is built (inferred), so a window made later is on top of those made before, and a pop-up made after another cascades over it.
- No function was found that moves a node to the end of `scene+0x298`. To bring an older window to the front, it can be built again.
- Modal input: `mgr+0x140`, when set and shown, gets the keyboard first (`0x14ea150`) and is cleared by the manager's update when its `+0xc8` says it is done (verified); what sets it was not traced. Nothing was found that keeps the map from a window's clicks other than the window's own elements taking them.

## 4. The game's big panels

- `0x818080(idler)`, `void` (verified): closes the views that cover the map: `idler+0x1b80`'s (`0x116cbe0`), the one at `+0x1228` when shown, the one at `+0x1100` (with a map mode switch through `0x821ff0` when that view's mode is `0x30`), and the one at `+0x11b8`. 56 calls in 41 functions.
- `0x80c6e0(idler)`, `void` (verified): closes the big panels by their slots in the idler (`+0x10b8`, `+0x1280` through `0x115ef60`, `+0x1288`, `+0x1290`, `+0x1270`, `+0x1278`, the papacy `+0x1298` through `0x80d670`, `+0x12a0`, `+0x12c0`, the Celestial Empire `+0x12b0`, `+0x12c8`, `+0x10c0`, `+0x10c8`, `+0x12a8`), and the first dialog on the idler's list `+0x78` whose `vtbl[+0x30]` says so (`vtbl[+0x28]`). 32 calls in 25 functions: every opener, the idler's frame (`0x815bce`, under a condition not traced), the return to the main menu (`0x81e8f0`).
- An opener (the papacy's, `0x80c670`, verified): if its panel is open, take it off the list the idler's `vtbl[+0x100]` returns and hide it; otherwise `0x818080`, `0x80c6e0`, build its window when missing, show it, fill it.
- The idler: `*(*0x233fe78 + 0x1e00)`, vtable `0x1cb8460`, or the first argument of the idler's frame `0x8137c0`, which notice-window.md §2.1 advises for the frame where idlers change.
- For the framework (inferred): open an extension window that is a big panel by calling `0x818080` and `0x80c6e0` with the idler, on the main thread at a safe point (the game calls them from its click handlers on that thread), and close the extension windows that are big panels in an observer before `0x80c6e0`, ignoring the calls the framework makes itself while it opens one.

## 5. The panel button strip

### The layout (verified)

- `floating_buttons` is a `windowType` nested in `menubar` (`interface/menubar.gui`, line 1779): `fullscreen = yes`, `show_position {0, 0}`, `hide_position {0, 100}`, so it slides down with the menubar. The in-game RestoreDeviceObjects fetches it from the `menubar` root by the subwindow lookup with the `std::string` `"floating_buttons"` (`0x81c19b`) into `idler+0x1198`; the GUI reload `0x82b470` fetches it again (`0x82b9c3`).
- The idler's frame calls `0x80c9f0(idler)` (at `0x815d72`, under a condition). Each kind of panel that applies is given the next slot, starting at 1, through `0x830700(idler, slot, kind, frame, number, visible, show_number, window)`; the slot counter only goes up when the kind is visible (`movzx r13d, r14b; inc r13d` after the first). Then `0x80d522..0x80d576` hides every slot from the first unused to 8. So after `0x80c9f0` the shown buttons are `f_button_1..n` and `n` is how many are shown; it is kept in no field.
- `0x830700` finds `f_button_<slot>` (window `+0x68`), `menubar_floating_bg_<slot>` (`+0xc8`) and `floating_number_<slot>` (`+0x98`); for a visible one it shows the button (`+0x88`), sets its frame (`+0xa8`, `0x15aeda0`: `button+0x270`, then the sprite's `+0xa8`), writes the kind to `button+0x10`, shows the background and the number (`TEXT_SET` with the number made by `0x17039a0`); for a hidden one it hides all three.
- `0x81add0` subscribes the strip's one callback to `f_button_1..8` and `hre_strip_glow` by name (verified), so an element spliced into the window is not wired to it.

### The places (from `menubar.gui`, all `LOWER_RIGHT`)

| Slot `k` | Button `f_button_k` | Background `menubar_floating_bg_k` | Number `floating_number_k` |
|---|---|---|---|
| 1..5 | `(-360 - 45(k-1), -58)` | `(-362 - 45(k-1), -36)` | about `(-364 - 45(k-1), -18)` |
| 6..8 | the same | 55 apart from slot 5 (vanilla's own mismatch) | 48 apart |

An entry button after `n` used slots: the button at `(-360 - 45n, -58)`, its background 2 left and 22 down of it, its number 4 left and 40 down.

### Moving an element there every frame

- A button's `+0x288`, `0x15ae930(button, i64 pos)` (verified): resolves `pos` with the button's orientation (`button+0x230`) against its parent's size (`button+0x234`) through `0x152dce0`, keeps the result at `button+0x178`, and moves its sprite (`+0x290`) and text (`+0x188`). `+0x98`, `0x1571dc0(button, i64* out)`, gives `+0x178` back.
- An icon's `+0x288`, `0x1571cf0(icon, i64 pos)` (verified; icon class `0x1d954b0`, inferred from its methods), writes `pos` to `+0x178` as it is, already resolved; the strip moves `hre_strip_glow` onto a button with `icon->+0x288(button->+0x98())` (`0x80cdcb..0x80cdf2`).
- So, in an observer after `0x80c9f0` (inferred): take `idler+0x1198`, count the shown `f_button_k`, find the spliced elements by name (they must be spliced into the `floating_buttons` block itself, since a window's lookups see only its own elements), place each entry button with its `+0x288` at its slot and its background with the icon's `+0x288` at the button's resolved place plus the offset above.

## 6. Text and tooltips

- `TEXT_SET` `0x152ae10(text_box, std::string* text, bool fit)` (verified): moves the string into `text_box+0x170` (the caller's is left empty, still the caller's to destroy), then `0x15f42c0(text_box+0x118 renderer, &text, fit)` copies it and, for a box with a width (`renderer+0x1c0` and `+0x254`) or with `fit`, asks the font (`renderer+0x258`, `vtbl[+0x80]`) to break it to the box's width (`+0x238`) within its lines (`+0x23c`; with `fit`, as many as it takes, which it then keeps). A box with a scroll bar (`text_box+0x1a0`) shows or hides the bar by the lines. So a multi-line text box wraps what it is given.
- The formatting codes (`§`, `£`, `$`) stay in the text and are read as it is drawn (unicode-text.md, the main text's eleventh parameter) (inferred: the game's own text boxes show colored text this way).
- Tooltips (verified as calls; what is filled is inferred): the idler's tooltip update `0x81f020` calls `0xb81870(idler, out, element)` (idler vtable `+0x408`, call at `0x81f0ea`), which goes to `0xb7caf0`, the game's tooltips by the element's name and window; the army panel's `0x1453b70`, which `ui` observes, is reached from there (`0xb7caf0` → `0x1471ee0` → `0x1453b70`). An observer after `0xb81870` can fill `out` (the two game strings `ui` fills, interface-bindings.md) for the framework's own elements in any window of the game. How a `.gui` element's own `tooltip` and `tooltipText` reach the tooltip was not traced.

## Key RVAs and entry bytes

| RVA | What | First bytes |
|---|---|---|
| `0x14eb7d0` | read an interface file after startup | `48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55` |
| `0x14eaaa0` | tidy the type library after a read | `48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 30 48` |
| `0x15472e0` | root `+0x30`: absolute position | `48 83 ec 28 4c 8b 99 30 0a 00 00 4c 8b c2 48 89 91 98 05 00 00` |
| `0x1547370` | root `+0x38`: position by orientation | `48 89 54 24 10 48 83 ec 28 44 8b 54 24 38 4c 8b` |
| `0x1546a30` | root `+0x58`: moveable | `88 91 b0 05 00 00 c3` |
| `0x1547170` | the drag | `40 57 48 83 ec 20 80 b9 b0 05 00 00 00 48 8b f9` |
| `0x154a7b0` | GUI `+0x98`: the window's position | `48 8b 81 68 05 00 00 48 89 02 48 8b c2 c3` |
| `0x1546d50` | root `+0x240`: slide to a position | `48 89 5c 24 08 48 89 6c 24 18 48 89 74 24 20 48` |
| `0x1547cc0` | root `+0x40`: show | `40 53 48 83 ec 30 80 b9 db 00 00 00 00 48 8b d9` |
| `0x1547a50` | root `+0x48`: hide | `40 53 48 83 ec 20 48 8b d9 c6 81 f5 00 00 00 00` |
| `0x152dce0` | an orientation's anchor | `33 c0 48 89 01 83 fa 08 0f 87 0d 01 00 00 48 63` |
| `0x818080` | close the views over the map | `48 89 5c 24 10 57 48 83 ec 20 48 8b 99 80 1b 00 00 48 8b f9` |
| `0x80c6e0` | close the big panels | `48 89 5c 24 10 57 48 83 ec 20 48 8b 99 b8 10 00 00 48 8b f9` |
| `0x80c670` | the papacy's opener | `40 53 48 83 ec 20 48 8b 81 98 12 00 00 48 8b d9 80 78 08 00 75 45` |
| `0x80c9f0` | lay out the panel button strip | `40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c` |
| `0x830700` | one slot of the strip | `48 89 5c 24 08 44 89 4c 24 20 44 89 44 24 18 55` |
| `0x81add0` | wire the strip's buttons | `48 85 c9 0f 84 36 01 00 00 48 89 5c 24 08 57 48` |
| `0x80da30` | the strip's click | `48 89 5c 24 10 57 48 83 ec 50 48 8b d9 48 8b 81` |
| `0x15ae930` | button `+0x288`: position by orientation | `48 89 54 24 10 53 48 83 ec 20 8b 91 30 02 00 00 4c 8d 81 34 02 00 00` |
| `0x1571dc0` | button and icon `+0x98`: resolved position | `48 8b 81 78 01 00 00 48 89 02 48 8b c2 c3` |
| `0x15aeda0` | button `+0xa8`: frame | `89 91 70 02 00 00 48 8b 89 90 02 00 00 0f bf d2 48 8b 01 48 ff a0` |
| `0x1571cf0` | icon `+0x288`: resolved position | `48 83 ec 28 48 89 91 78 01 00 00 48 8b 89 68 01 00 00` |
| `0x15f42c0` | fit a text box's text | `48 89 5c 24 18 48 89 6c 24 20 56 57 41 56 48 81` |
| `0xb81870` | the idler's tooltip (vtable `+0x408`) | `48 89 5c 24 08 48 89 54 24 10 55 56 57 41 54 41` |
| `0x81f020` | the idler's tooltip update | `48 89 5c 24 08 48 89 74 24 10 48 89 7c 24 18 55` |

`0x154a7b0`, `0x1571dc0` and `0x1546a30` are leaves with no entry of their own in the exception directory; check them by their bytes and the vtable slots that hold them. `0x80c6e0` and `0x818080` share their first ten bytes; the field each reads first tells them apart.

## A window's background (found in the running game)

A `windowType` takes no `background = { … }` block: that is a `containerWindowType`'s, and in a `windowType` the game's reading stops at it ("Unexpected token", in the game's `error.log`), leaving the definitions after it unread. A `windowType` names its background with `backGround = "<element>"`, an element of its own, as the game's dialogs do: a `guiButtonType` of the corner-tiled `SandboxFloaterBackground`, which the window stretches to its size ([prototype log 2026-11-09](../prototype-log/2026-11-09-extension-windows.md)).

## What needs verifying at run time

1. That a definition read after startup through `0x14eb7d0` builds with CreateWindow at once, and what happens to a live window when its definition is replaced.
2. That a window built by CreateWindow shows above the game's panels and takes clicks before the map, and that the one made last is on top, cascades included.
3. That `moveable = yes` drags it and `root+0x598` follows; that root `+0x30` puts it back after it is built again.
4. That the observer on `0x80c6e0` closes extension windows when the player opens the papacy, the Holy Roman Empire and the country view, and what the call in the idler's frame (`0x815bce`) answers to.
5. That entry buttons placed after `0x80c9f0` stand beside the game's own at every resolution, slide with the strip, and that the strip's own click ignores them.
6. That the observer after `0xb81870` shows an extension window's tooltips.
