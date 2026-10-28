# Persistent notice window: a static study of the host window and its lifecycle

Date 2026-10-27. Subject: vanilla 1.37.5 `eu4.exe` (SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`, image base `0x140000000`). All addresses below are **RVAs**.

The method is the same as in `ui-injection.md`: static analysis only, without starting the game or attaching to the process:

- Ghidra 12.1.4 headless, opened with `-readOnly -noanalysis`, decompiling or linearly disassembling only selected functions;
- strings, RIP-relative references, `E8` direct calls, function pointers in vtables and `.pdata` function bounds scanned with self-written scripts;
- no REA session this time; the same Ghidra that REA is configured with was used directly.

Items marked **verified** were seen directly in decompiled code or in the bytes; items marked **inferred** are judgements from structure. All function prototypes in the pseudocode are tentative.

## Conclusion

**Feasible. The recommended host window is the top bar window `topbar` (`interface/topbar.gui`).** Confidence is high for the host and the lifecycle; the display position and the slide-out animation need confirming at run time.

- **CreateWindow prototype** (verified): `Window* CreateWindow(GuiManager* mgr, const std::string* name)`, with only two parameters; the name is an MSVC `std::string`. Only the methods that find elements by name, `+0x68/+0x98/+0xc8`, take a `const char*`.
- **Where the root pointer lives** (verified): the `topbar` root window is created by the in-game idler. This idler is `MAP_MANAGER` in the framework, that is `*(world+0x1e00)`, vtable `0x1cb8460`, source file `ingameidler.cpp`. In RestoreDeviceObjects it builds a top bar object of 0x6e0 bytes, and the root pointer is at `*(*(idler+0x12d8)+0x658)`.
- **Lifecycle**: built once per game, that is once per in-game idler (inferred). When `reload` is run on any `.gui` file this idler watches, the root window is destroyed and rebuilt, and the pointer changes (verified).
- **One class for all** (verified): every root CreateWindow builds is of the same class (0xa80 bytes, constructor `0x1543f80`, vtable `0x1d926b0`, `0x1d929d0` at `+0x30`). The lookup methods, show/hide methods and visible byte that the army panel uses work the same on the `topbar` root.
- **Where to hook destruction** (verified): every path ends in the root window's Kill, that is `0x1547520` (root vtable `+0x130`).
  - Kill only sets the flag `root+0xf4=1` and marks the child elements in cascade. The memory is freed, through `0x1546510`, only when the GUI manager's per-frame sweep `0x14e9fa0` runs.
  - `0x6ecc80` does not destroy anything. It only takes the root off the idler's per-frame update list.
- **Child elements outside the window**: statically, no code was found that clips child elements to the parent window's `size`. Mouse hits are tested against each element's own rectangle (partly verified). This matches what the army panel shows at run time.

## 1. The calling convention of CreateWindow

The GUI manager singleton is `*0x23494f0`. The vtable of the EU4 subclass is `0x1d58108`, that of the base class `0x1d8dc40`. The `+0x48` slot is `0x14e9030`.

**Disassembly (verified)**:

1. `rcx` (mgr) is saved in `rdi`, `rdx` (name) in `rbx`.
2. It calls `(mgr+0xc8)->vtbl[+0x08](mgr+0xc8, name)` to look up the type library; at this point `rdx` is still the same `std::string*`.
3. When nothing is found:
   - it reads `name+0x18` (capacity): if that is 0x10 or more, it takes `*name` as the heap pointer, otherwise it uses the inline buffer;
   - it logs `Failed to create window "%s"` (`gui.cpp` line 0x16f);
   - it returns 0.
4. When the type is found:
   - it calls `0x155c260(proto, *(mgr+0xc0), mgr, *(*(mgr+0xc0)+0x490), 0)`. That always allocates 0xa80 bytes and calls the constructor `0x1543f80`;
   - it then calls `mgr->vtbl[+0x98](mgr, win ? win+0x30 : 0)` (`0x14eb360`) to register it;
   - it returns `win`.

**Parameters and return value**:

- `r8` and `r9` are both overwritten after entry before any use (`0x14e9078`, `0x14e90a0`, `0x14e90b2`), so there is **no third or fourth parameter**.
- The return value is a pointer to the window's main object (offset 0). What is registered with the manager is `win+0x30`, the GUI subobject.
- CreateWindow only reads the name string; it neither changes nor frees it. The caller frees the temporary string afterwards (verified).
- Inferred: the framework can build a 32-byte `std::string` of its own and pass it in. A name of at most 15 bytes uses the inline buffer and needs no game allocator.

**Call sites** (all verified):

| Call site | Window name | mgr taken from | Name string |
|---|---|---|---|
| `0x144fb00` army panel | `single_unitpanel` | `*(*(panel+0x48)+0x58)` | temporary `std::string` on the stack; 16 bytes, so on the heap (size 0x10, cap 0x1f) |
| `0x1431020` top bar constructor | `topbar` | global `*0x23494f0` | top bar object member `+0x660` |
| `0x149ad00` window holder rebuild | the holder's own name | global `*0x23494f0` | holder `+0x10` |
| `0x1431a80` top bar wiring | `speed_controls` | `*(*(topbar+0x648)+0x58)` | temporary |
| `0x81af10` in-game RestoreDeviceObjects | `status_message_window`, `message_icons`, `countryview`, `alerts_parent`, `hotkey_assign_parent`, `macro_build_interface`, `menubar` | `*(idler+0x58)` | temporary, assigned through `0x95110(str, literal, len)` |
| `0x82b470` in-game GUI reload | `menubar` and others | `*(idler+0x58)` | temporary |
| `0x10d5bc0` main menu RestoreDeviceObjects | `country_selection_panel` | `*(frontend+0x58)` | temporary |

`*(idler+0x58)` and the global `*0x23494f0` are the same manager (inferred, because only one GUI manager is ever built).

**How this fits the old notes**: the regional-combat notes say "Root creation uses a DIFFERENT slot and an engine string". The slot meant is the **manager's** `+0x48`, not a slot of the root window; the "engine string" is the MSVC `std::string`. The two do not contradict each other.

Note also that the root window has slots of its own that take a `std::string*`. They find **subwindows** (not elements); do not confuse them with the above:

- `+0x180` (`0x1548190`) only tail-jumps to `+0x1c0`;
- `+0x1c0` (`0x15481a0`) has the prototype `(root, const std::string* name, bool warn)`. It searches the subwindow table at `root+0x958..+0x960`, whose entries are 0x28 bytes, `{std::string name, Window*}`. When nothing is found it returns 0; when `warn` is true it also logs `Attempt to access nonexistent subwindow`;
- the army panel uses `+0x180` to get `mission_settings_win`; the top bar uses `+0x1c0`.

## 2. The persistent host window: `topbar`

### 2.1 Pointer chain and checks (verified)

```
world  = *(0x233fe78)
idler  = *(world + 0x1e00)        vtable 0x1cb8460; this is also the rcx of MANAGER_FRAME 0x8137c0
topbar = *(idler + 0x12d8)        vtable 0x1d84250, 0x6e0 bytes
root   = *(topbar + 0x658)        vtable 0x1d926b0; vtable 0x1d929d0 at root+0x30
```

**Fields of the top bar object** (its vtable has only one entry, the deleting destructor `0x14317c0`):

| Offset | Contents |
|---|---|
| `+0x648` | the idler passed to the constructor |
| `+0x650` | start of the window holder: the idler the root is currently registered with (owner). 0 after construction; filled in by `0x1431a80` |
| `+0x658` | holder `+0x08`: the `topbar` root window |
| `+0x660` | holder `+0x10`: `std::string "topbar"` |
| `+0x680` | `speed_controls` root window, created by `0x1431a80` |
| `+0x688` | `timeline_controls` helper object: 0x290 bytes, vtable `0x1d84238`, with its own root at `+0x30` |
| `+0x698`, `+0x6a0` | subwindows fetched through `root->vtbl[+0x1c0]` |

**Relevant fields of the in-game idler**:

| Offset | Contents |
|---|---|
| `+0x58` | GUI manager |
| `+0x90` / `+0x98` / `+0xa0` / `+0xa4` | per-frame window list: head, tail, count, "iterating" flag |
| `+0x3d0` | GUI reload listener, vtable `0x1cb88b8`; its `+0x10` slot is `0x82b470` |
| `+0x460` | "reloading a save in game" flag |
| `+0x12d8` | top bar object (unique_ptr) |
| `+0x1178` | `menubar` root |
| `+0x10d0` / `+0x10d8` | `alerts_parent` / `hotkey_assign_parent` roots |
| `+0x1148` | `status_message_window` root |
| `+0x16f0` | `countryview` root |

**When `world+0x1e00` changes**: it is written by `0x756a30(world, idler)` (verified). The callers are:

- the idler constructor `0x803270`;
- the in-game RestoreDeviceObjects `0x81af10`;
- the main menu RestoreDeviceObjects `0x10d5bc0`, which passes `r15`, the register it uses for zeroing in a loop; inferred to be 0.

Inferred: in the main menu `world+0x1e00 == 0`, so the pointer chain breaks at the first step. It is safer to take the idler directly from `rcx` in the `0x8137c0` hook.

**Suggested checks every frame**:

- `*(idler)==0x1cb8460`, `*(topbar)==0x1d84250`, `*(topbar+0x648)==idler`;
- `root!=0`, `*(root)==0x1d926b0`, `*(root+0x30)==0x1d929d0`;
- `root+0xf4==0` (not killed);
- optional: the string at `topbar+0x660` equals `topbar`.

### 2.2 Creation (verified)

1. **Entry**: the in-game RestoreDeviceObjects is `0x81af10`, at `+0x30` in the idler vtable; it logs `Start RestoreDeviceObjects`.
2. **Reset the window list**: the function starts by calling `0x1400d0020(idler+0x90)`.
3. **Build the top bar object**: in the `idler+0x460 == 0` branch:
   - it first builds `alerts_parent` and `hotkey_assign_parent`;
   - then it does `new(0x6e0)` and constructs the object with `0x1431020(topbar, idler)`;
   - the result goes into `idler+0x12d8`. The new object is swapped in first, then the old one is deleted.
4. **Top bar constructor `0x1431020`**:
   - assigns `"topbar"` to `+0x660`;
   - calls `root = (*0x23494f0)->vtbl[+0x48](mgr, topbar+0x660)` and stores the result in `+0x658`;
   - if the root is not null: calls `(root+0x30)->vtbl[+0x88]()` to show it, then calls `root->vtbl[+0x50](root, 1)`;
   - finally calls `0x1431a80(topbar)`.
5. **Wiring, `0x1431a80`**:
   - registers the root with the idler: calls `0x6ecc80` for the old owner and `0x6ecbd0` for the new idler, and records the owner at `+0x650`;
   - finds buttons with methods such as `+0x68` and `+0xc8` and subscribes to them;
   - creates a new `speed_controls` root (`+0x680`) and a new `timeline_controls` object (`+0x688`).
6. **Register again**: whichever branch was taken, `0x81af10` then calls `0x149ade0(topbar+0x650, idler)`, which registers the root again in the window list that was just cleared.

**What `topbar.gui` defines**:

- `fullScreen = yes`, `size = 2048×100`;
- two animation positions, `show_position {0,-1}` and `hide_position {0,-165}`.

The vanilla `pause_window` nested in it says `Orientation = "CENTER_UP"`, `x=-203` (half its width of 406), and it shows centered. Inferred: the orientation anchor for child elements inside `topbar` is the full-screen top bar, that is, the screen.

### 2.3 When it is rebuilt and when destroyed

**Starting or loading a game (from the main menu)**:

1. The main menu `0x10da520` (`frontend.cpp`) does `new(0x5bc8)` and constructs an idler (`0x803270`), then calls `0x14c2530(app, &idler, 0)` to queue the switch. This step is verified.
2. The application frame `0x14c25a0` performs the switch when `app+0x48` is set (verified):
   - the old idler has `+0x50` and then `+0x38` (InvalidateDeviceObjects) called on it;
   - unless `app+0x80` is set, the old idler is deleted;
   - the new idler goes into `app+0x40` and has `+0x48`, `+0x28` and `+0x30` (RestoreDeviceObjects, where the top bar is created) called on it in that order.

Conclusion: **it is built once per game, that is once per in-game idler, not once per run of the program.**

**Returning to the main menu**:

1. On the switch to the main menu idler, the old idler's `+0x38`, that is `0x81e8f0`, is called. This step is inferred: the switch mechanism is verified, but that returning to the main menu goes through this switch is inferred.
2. `0x81e8f0` first sets `idler+0x12d8` to 0, then deletes the top bar object (verified).
3. The deletion: `0x14317c0` → `0x14318c0` (Kills `speed_controls` and others) → `0x149aca0(topbar+0x650)` (verified).

**Reloading a save in game**:

- The function `0x5d0580` carries cloud save strings; its callers are a command object and the debug command `0xe95610`. Inferred: it is the path that loads a save in game without going through the main menu.
- It first sets `idler+0x460=1`, calls `0x81af10` directly after loading, then clears `+0x460` (verified).
- On this path **neither the top bar object nor the root is rebuilt**. The root is only registered again, and the pointer stays the same.

**GUI reload** (verified):

1. **Registering the listener**: the idler constructor uses `0x14e0d90` to register `idler+0x3d0` as the reload listener for these files: `interface/main.gui`, `topbar.gui`, `menubar.gui`, `macrobuildinterface.gui`, `macrobuildinterface_override.gui`, `papacy.gui`, `great_powers.gui`, `parliament.gui`, `mapmode_more.gui`, `more_interface.gui`, `unitpanel.gui`, `outliner.gui`, `core.gui`.
   - The `topbar.gui` entry is seen directly in the disassembly at `0x80a4dd` (`lea rbx,[rsi+0x3d0]`);
   - the others are judged from their strings lying next to it.
2. **Console `reload <file>`**: `0x14e1140` first rereads the file, then finds the listener by path in the global table `*0x242bf58` and calls `listener->vtbl[+0x10]`. For the idler that is `0x82b470`.
3. **What `0x82b470` does with the top bar**:
   - `0x14318c0(topbar)`;
   - `0x149ad60(topbar+0x650)`: notes the old root's `+0xf5`, then calls `0x149aca0` to destroy the old root and `0x149ad00` to rebuild the `topbar` root, and finally registers it again;
   - `0x1431a80(topbar)`.

   **The root pointer changes.** Whichever of the files above is reloaded, this code runs.

**Console `reloadinterface` (no argument)**: `0xe590d0` only calls `0x81ecf0(idler)`, which rebuilds the 14 view objects under `idler+0x1110` and does not touch the top bar (verified).

**Graphics device reset** (switching out of full screen, changing resolution): no path was found that shows whether it also goes through Invalidate/Restore; not verified. The Kill hook covers this case.

**Sliding out and visibility** (verified):

- Root vtable `+0x298` (`0x1547c60`) moves the window's animation to `hide_position`; `+0x288` (`0x1547ec0`) moves it back to `show_position`.
- These slide `topbar` (and `menubar` and others) out:
  - switching to map mode 0x25 (`0x821ff0`). `timeline_controls` shows only in 0x25 (`0x1430510`), so 0x25 is inferred to be the timeline mode;
  - the two callers of `0x81fdc0`: `0x145b160` (the army reorganization window) and `0x1170ba0` (not identified).
- `0x81ffb0` slides it back.
- When the top bar slides out, its child elements leave the screen with it. Whether `+0xf5` changes as a result is not confirmed.

### 2.4 Other candidates

| Candidate | Notes |
|---|---|
| `speed_controls` | Owned by the top bar (`+0x680`), rebuilt by every `0x1431a80`, and destroyed first by `0x14318c0` on a reload. Its lifecycle is shorter than the `topbar` root's; no advantage |
| `menubar` | `idler+0x1178`, created by the same RestoreDeviceObjects, rebuilt by `0x82b470`, destroyed in `0x81e8f0` through GUI `+0x78`. Usable, but has no holder object like the top bar's |
| `outliner` | Created only when the global flag `*0x233ff4f == 0` (`0x81af10`, `0x82b470`); not guaranteed to exist |
| `minimap_panel` | Referenced at `0x1247ed0`, `0x1248850`; not looked into further |

## 3. Destruction paths and where to hook

### 3.1 `0x6ecc80(idler, root)` (verified)

- The first parameter is the **in-game idler**, not the GUI manager. The army panel destructor passes `*(world+0x1e00)`; the top bar passes `*(topbar+0x648)`.
- It finds `root`'s node in the list at `idler+0x90` by **comparing pointers**:
  - a node is 0x20 bytes: `{root, prev, next, removed byte}`;
  - when `idler+0xa4` is non-zero (iterating), it only sets the node's `+0x18` to 1;
  - otherwise it unlinks the node, frees it and decrements the count.
- It **does not read the root's contents**, so calling it on a root that has already been killed is safe.
- The matching register function is `0x6ecbd0`, with 71 direct call sites; this list is the idler's "per-frame window update table". The base class's frame function `0x6ea380` walks it:
  - for each root not killed, it calls `root->vtbl[+0x238](root, dt)`, that is `0x1546ac0`;
  - roots already killed (`+0xf4` non-zero) it removes with `0x6ecc80`.
- It is **not** the general teardown path for windows from CreateWindow. CreateWindow only registers a window with the GUI manager; whether the window goes into an idler's list is up to its owner.

### 3.2 Root window Kill: `root->vtbl[+0x130]` = `0x1547520` (verified)

- It first sets `root+0xf4 = 1`. This byte is the GUI subobject's `+0xc4`, the "dead" flag; the byte right after it, `+0xf5`, is the visible flag (GUI `+0xc5`).
- It calls `child->vtbl[+0x78]` on every child element in each kind of element table. The element tables include `+0x5b8`, `+0x5e8`, `+0x618`, `+0x648`, `+0x678`, `+0x708`, `+0x738`, `+0x768` and several hash tables. A button's `+0x78` is `0x15aea30`, which sets `button+0xc4 = 1`.
- It calls `+0x130` recursively on every subwindow in the subwindow table `+0x958`.
- It calls `+0x78` on the background element `+0x520`.
- **It frees no memory.**
- The GUI subobject's `+0x78` is `0x1547910`, which does `rcx -= 0x30` and then jumps to `root->vtbl[+0x130]`. So a Kill through the GUI subobject (as `0x81e8f0` does for `alerts_parent`) also ends up in `0x1547520`.

### 3.3 The actual freeing (verified)

- The GUI manager's update `0x14e9fa0(mgr, dt)` walks `mgr+0x110` (everything registered through `0x14eb360`) and calls `obj->vtbl[+0x38](obj, 1)` on every object whose `+0xc4` is non-zero.
- For a root this entry is `0x15503cc`: `rcx -= 0x30`, then a jump to `0x1546510`. `0x1546510` calls the destructor body `0x1546730`, then `free(root, 0xa80)`.
- The window constructor `0x1543f80` registers each child element and subwindow with `mgr->vtbl[+0x98]`, so each of them is freed on its own in the same sweep. The framework's existing `BUTTON_DESTROY 0x10b5670` is the button class's `+0x38`.
- `0x14e9fa0` is called unconditionally at the end of `0x14c5810`. `0x14c5810` in turn is called by the in-game idler frame `0x8137c0` (at `0x815914`), by the main menu frame `0x10d2e80` and by `0x1253d80`.
- Conclusion: **between Kill and the freeing there is at least the gap until the sweep later in the same frame.** And the sweep happens **before** `0x8137c0` returns.

### 3.4 Every teardown path goes through Kill (verified)

| Path | Order |
|---|---|
| Top bar holder `0x149aca0` (top bar destructor, GUI reload) | `0x6ecc80` → GUI hide `+0x90` → `+0x130` → `holder.root = 0` |
| Army panel destructor `0x144f560` | `0x6ecc80(*(world+0x1e00), root)` → `+0x130` |
| `0x82b470` rebuilding `menubar` | hide → `+0x130` → `0x6ecc80` |
| `0x81e8f0` (return to the main menu) | deletes the top bar object (through the holder); other roots through GUI `+0x78` |
| Main menu `0x10d8330` / `0x10dcb00` | hide → `+0x130` |

**Derived window classes**: `0x10a1dd0` (the `+0x130` of another window class) also jumps to `0x1547520`, and `0x10a1cb0` calls `0x1546730`. That is a derived class built by another factory; it does not affect roots from CreateWindow.

### 3.5 Recommended hooks

1. **Main hook**: an observing hook at the entry of `0x1547520`; `rcx` is the window pointer.
   - If it equals the bound root, unbind at once: unsubscribe from the button signals and clear the object pointers.
   - At this point the memory of the root and its child elements is still valid, so unbinding is safe.
   - The hook also fires when subwindows are killed recursively; filtering by pointer is enough.
2. **Fallback**: another hook at the entry of `0x1546510`, where `rcx` is the window pointer, also when it comes from the thunk `0x15503cc`; or keep using element destructor hooks such as `BUTTON_DESTROY`.
3. **Do not** use the after hook of `0x8137c0` to detect destruction: by then the sweep is done and the elements have been freed.

## 4. Lookups on other roots (verified)

**`0x155c260` does not pick a class by prototype**: it always allocates 0xa80 bytes and calls `0x1543f80`. The constructor unconditionally writes `*root = 0x1d926b0` and `*(root+0x30) = 0x1d929d0` right at its start. Nested `windowType`s are also built through `0x155c260`, are of the same class, and are kept in the subwindow table at `root+0x958`.

**`0x1d926b0` is the vtable of the general window class**, so the `ROOT_VT` and `ROOT_GUI_VT` checks in `ui.rs` hold for the `topbar` root as well. The relevant slots:

| Slot | Function | Purpose |
|---|---|---|
| Root `+0x00` | `0x1546510` | Deleting destructor (0xa80) |
| Root `+0x68` | `0x1548700` | Finds a button by `const char*`. Searches `+0x5b8/+0x5d0`, `+0x618/+0x630`, `+0x5e8/+0x600` in that order |
| Root `+0x98` | `0x1548d30` | Finds an instant text box by `const char*` (`+0x708/+0x720`) |
| Root `+0xc8` | `0x15492e0` | Finds an icon by `const char*` (`+0x648/+0x660`) |
| Root `+0x130` | `0x1547520` | Kill |
| Root `+0x1c0` | `0x15481a0` | Finds a subwindow by `std::string*` |
| Root `+0x238` | `0x1546ac0` | Per-frame update (called from the idler's list) |
| GUI `+0x38` | `0x15503cc` | Thunk for the deleting destructor |
| GUI `+0x78` | `0x1547910` | Thunk for Kill |
| GUI `+0x88` | `0x1547d90` | Show: sets `+0xc5=1`, that is `root+0xf5` |
| GUI `+0x90` | `0x1547b00` | Hide: sets `+0xc5=0` |

Notes:

- The three lookup methods **only search the root's own direct child elements**; they do not descend into subwindows. When splicing, put the elements in the `topbar` `windowType` block, not in a nested window. If they go in a nested window, fetch the subwindow with `+0x1c0` first.
- When nothing is found, all three go on to `0x14154e930` with the type name `Button`, `InstantTextBox` or `Icon`, and may return a stand-in object. So the "check the element's name" check in `ui.rs` must stay.
- Mouse input (`0x14ea150`, `0x14ea630`) skips objects whose `+0xc4` is non-zero or whose `+0xc5` is 0. After Kill, elements receive no more clicks.

## 5. Child elements outside the window's bounds

- **Hit testing** (verified): `0x14ea630` walks a flat table of scene nodes (`scene+0x298`) through `0x14dda50`. Each node is tested against **its own** rectangle, the shorts at `node+0x128..+0x12e`; the parent window's bounds are not checked along the way.
- **The bounds check only logs** (verified): when a child element goes past the size of the background element (`root+0x520`), `0x1546e70` logs `Illegal object boundaries`. It neither clips nor moves the element. `topbar` has `backGround=""`, so it is inferred to have no background element and not to trigger this.
- **Rendering** (not fully verified): no vanilla `.gui` has a `clipping` field anywhere. The D3D9 renderer has a clipping implementation (`0x173f830`), but which GUI elements turn it on was not traced.
- **Conclusion**: no code was found that clips to the parent window's `size`. This matches what is seen at run time, where child elements of the army panel that go past its bounds still show. Clipping remains inferred.

## 6. A persistent window in the main menu (optional)

The vtable of the **main menu idler** (`frontend.cpp`) is `0x1d5a8c8` (verified):

| Slot | Function | Purpose |
|---|---|---|
| `+0x20` | `0x10d2e80` | Frame function; `rcx` is the main menu object |
| `+0x30` | `0x10d5bc0` | RestoreDeviceObjects |
| `+0x38` | `0x10d8330` | InvalidateDeviceObjects |

**Root window `country_selection_panel`** (`interface/frontend.gui` line 113):

- It is created in `0x10d5bc0` through `*(frontend+0x58)->vtbl[+0x48]`, with a temporary `std::string` as the name, and the root is stored in `frontend+0x790` (verified).
- `0x10d8330` first hides it, then Kills it, then clears `+0x790` (verified).
- The reload listener is at `frontend+0x3c8`, vtable `0x1d5a880`; its `+0x10` slot is `0x10dcb00`, which Kills and rebuilds this root (verified).

The `.gui` definition: `fullscreen=yes`, with a background button `Background`; `mainmenu`, `multiplayer_menu`, `lobby`, `settings` and others are all its subwindows.

Inferred:

- it is the container window shown for as long as the main menu is up;
- its lifecycle is that of this main menu idler.

Use it the same way as in game: in the `0x10d2e80` hook check `*(frontend)==0x1d5a8c8`, read `+0x790`, then check the root's vtable. It has a background element, so child elements that go past its bounds will trigger the log line above, which is harmless.

## Key RVAs and entry bytes

All first 16 bytes were read from the image. For the few where the common prologue does not set them apart well enough, a longer signature is given.

| RVA | Purpose | First 16 bytes |
|---|---|---|
| `0x14e9030` | GUI manager `+0x48` CreateWindow | `48 89 5c 24 10 57 48 83 ec 40 48 8b f9 48 8b da` |
| `0x14eb360` | GUI manager `+0x98` register | `48 85 d2 0f 84 bf 00 00 00 48 89 5c 24 08 48 89` |
| `0x6ecbd0` | idler: add to the per-frame window list | `48 89 5c 24 10 57 48 83 ec 20 48 8b 81 90 00 00` |
| `0x6ecc80` | idler: remove from the per-frame window list | `40 53 48 83 ec 20 48 8b d9 48 8b 89 90 00 00 00` |
| `0x1547520` | Root `+0x130` Kill | `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57`, followed by `48 83 ec 20 c6 81 f4 00 00 00 01` |
| `0x1546510` | Root deleting destructor | `48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8`, followed by `0c 02 00 00 f6 c3 01 74 0d ba 80 0a 00 00` |
| `0x14e9fa0` | GUI manager update and sweep | `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57`, followed by `48 83 ec 30 0f 29 74 24 20` |
| `0x8137c0` | In-game idler frame (the existing `MANAGER_FRAME`) | `48 8b c4 88 50 10 55 53 56 57 41 54 41 55 41 56` |
| `0x81af10` | In-game RestoreDeviceObjects | `48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57` |
| `0x81e8f0` | In-game InvalidateDeviceObjects | `48 89 5c 24 10 48 89 6c 24 18 56 57 41 56 48 83` |
| `0x82b470` | In-game GUI reload callback | `48 89 5c 24 18 55 56 57 41 54 41 55 41 56 41 57` |
| `0x1431020` | Top bar constructor | `4c 8b dc 49 89 5b 18 49 89 73 20 49 89 4b 08 57` |
| `0x14317c0` | Top bar deleting destructor | `48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 8b` |
| `0x1431a80` | Top bar wiring (called at the end of the constructor and after a reload) | `48 89 5c 24 10 48 89 74 24 18 55 57 41 56 48 8b` |
| `0x149aca0` | Window holder: destroy the root | `40 53 48 83 ec 20 48 8b d9 48 8b 49 08 48 85 c9` |
| `0x149ad00` | Window holder: create the root | `48 89 5c 24 08 57 48 83 ec 20 48 8b d9 0f b6 fa` |
| `0x149ad60` | Window holder: rebuild the root | `48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 48` |
| `0x1548700` | Root `+0x68` find button | `49 c7 c0 ff ff ff ff 66 0f 1f 84 00 00 00 00 00` |
| `0x1548d30` | Root `+0x98` find text box | `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48` |
| `0x15492e0` | Root `+0xc8` find icon | `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48` |
| `0x15481a0` | Root `+0x1c0` find subwindow | `48 89 5c 24 10 48 89 74 24 18 55 57 41 54 41 56` |
| `0x1547d90` | GUI `+0x88` show | `40 53 48 83 ec 30 80 b9 c5 00 00 00 00 48 8b d9` |
| `0x1547b00` | GUI `+0x90` hide | `48 89 5c 24 10 57 48 83 ec 30 48 8b f9 48 8b 89` |
| `0x10d2e80` | Main menu frame | `48 8b c4 48 89 58 10 55 56 57 41 54 41 55 41 56` |
| `0x10d5bc0` | Main menu RestoreDeviceObjects | `40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d ac` |
| `0x10d8330` | Main menu InvalidateDeviceObjects | `40 53 48 83 ec 20 48 89 6c 24 30 48 8b d9 48 89` |
| `0x10dcb00` | Main menu GUI reload callback | `48 89 5c 24 20 55 56 57 41 54 41 55 41 56 41 57` |

The first 32 bytes of `0x1548d30` and `0x15492e0` are identical. When checking them, match them by vtable slot; the bytes alone are not enough.

**Data**:

| Address | Contents |
|---|---|
| `0x233fe78` | world pointer |
| `0x23494f0` | GUI manager pointer |
| `0x1cb8460` | in-game idler vtable |
| `0x1cb88b8` | idler reload listener vtable |
| `0x1d84250` | top bar object vtable |
| `0x1d926b0` | root window vtable |
| `0x1d929d0` | vtable at root `+0x30` |
| `0x1d5a8c8` | main menu idler vtable |

## Suggested implementation

1. **Splicing**: splice the notice window's elements (background icon, text box, close button) straight into the `windowType "topbar"` block of `interface/topbar.gui`, with names prefixed `eu4harmony_`. They can be placed relative to the screen with `Orientation`, the way `pause_window` does it.
   - They can also be spliced in as a nested `windowType`, shown and hidden as a whole with GUI `+0x88/+0x90`. The cost is that lookups must first fetch the subwindow with `+0x1c0`, and that step needs a `std::string` built.
2. **Discovery and binding**: in the after hook of the existing `MANAGER_FRAME` (`0x8137c0`):
   - take `rcx` as the idler, then read `root` and check it as in 2.1;
   - if it differs from the bound root, unbind first and then bind again, the same way as for the army panel.
3. **Unbinding**: an observing hook at the entry of `0x1547520`; when `rcx` is the bound root, unbind at once. The entry of `0x1546510` is the fallback. Keep `BUTTON_DESTROY`.
4. **Main menu (optional)**: bind `*(frontend+0x790)` in `0x10d2e80` the same way.

## Risks and what needs verifying at run time

1. **Sliding out**: map mode 0x25 (inferred to be the timeline) and the army reorganization window slide `topbar` to `y=-165`. The notice window goes off screen with it. Confirm whether that is acceptable, and what `+0xf5` is while the top bar is slid out.
2. **Position**: that child elements in a full-screen window are oriented relative to the screen is inferred. Confirm it at different resolutions, especially on screens narrower than 2048.
3. **Layering**: the notice window renders on the same layer as the top bar. Other windows may cover it, or it may cover them.
4. **Confirming the paths**: whether returning to the main menu really goes through `0x81e8f0`; in what order the old top bar is deleted during the switch when a new game starts; whether a graphics device reset rebuilds the root. Confirm with logging at `0x1547520`, `0x1431020` and `0x149ad00`.
5. **Reload**: after `reload interface/topbar.gui`, or a reload of another watched file such as `menubar.gui` or `unitpanel.gui`, the root pointer changes. Confirm that the Kill hook fires first, that binding happens again on the next frame, and that unsubscribing from signals does not happen in the middle of dispatching a click.
6. **Threads**: all the functions above should run on the main thread (the application frame `0x14c25a0`, inferred), but the Kill hook fires on whatever thread calls Kill, so the hook must check which thread it is on.
7. **The world pointer in the main menu**: `world+0x1e00` is inferred to be 0 in the main menu. In the frame where the idler is switched, it briefly points at the deleted idler. So read the pointer chain only inside the `0x8137c0` hook, starting from `rcx`.
8. **Other mods that change `topbar.gui`**: if one renames or removes the `topbar` window, CreateWindow returns 0, and vanilla `0x1431a80` does not check for a null root and crashes outright; this has nothing to do with the framework. When the splicer cannot find the target block, it should skip it and report it.
9. **Baseline**: this document covers only vanilla `9ad3efe1…`.
