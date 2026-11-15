# GUI definitions and window factories

## The GUI loading path (verified)

1. **Startup entry**: entry point `0x1a337c8` → `0x1a33654` → `0x1a78630` → `0x8757e0`. After that it splits two ways on a setting (`puVar17[0xb]`):
   - call `0x2003d0` directly on the main thread;
   - or start a **LoadingThread** thread through `0x17320a0(0x874010, "LoadingThread", app)`, which calls `0x200810`.

   Both paths end up in `0x202850` (application initialization in `eu4application.cpp`).
2. **`0x202850` loads the GUI definitions**:
   - It loads the `.gfx` files first, then shows `LOADING_GUI_DEF`.
   - It creates the GUI manager at `app+0x358` (0x550 bytes, EU4 subclass constructor `0x10b3bd0`, base class constructor `0x14e8640`). **The base class constructor writes the singleton to the global `0x23494f0`**.
   - It lists every `.gui` in the interface directory with the VFS enumeration `0x170ffe0(dir, &vec, ".gui", 0)` (the directory string is built at run time; inferred to be `interface`).
   - Each file in turn:
     - `0x170dd70(&lexFile, path)` opens the file;
     - `0x1709570(&lexer, &lexFile, "UNNAMED_SESSION", 0)` builds the lexer;
     - `mgr->vtbl[+0x18](mgr, &lexer)` reads it in (the generic persistence read `0x17068b0`);
     - after reading, it logs `Loaded GUI Definition File: `.
3. **The CLexer file constructor `0x170cd10`** (`clausewitz/pdx_core/lexer.cpp`, 109 direct call sites in all; all of the game's script text is read through it):
   - `0x1718300(path, 0, 0x20000)` opens the file. It is the only direct caller of `PHYSFS_openRead`; after that call it calls `PHYSFS_setBuffer(f, 0x20000)`, and it returns a newly allocated `{PHYSFS_File*, bool}` (0x10 bytes). The open happens inside the global spin lock `*0x242b9c0`.
   - `vec = new std::vector<char>` (0x18 bytes).
   - `len = PHYSFS_fileLength(f)` (call site `0x170d079`), then `0x416ef0(vec, len)` resizes the vector.
   - `PHYSFS_read(f, vec.data, 1, (u32)len)` (call site `0x170d0b3`), **without checking the return value**.
   - `0x171ebc0(stream, 0, 0, vec, 1, 0, 0, 1)` (call site `0x170d0ee`) wraps the vector in a 0x58-byte memory stream (vtable `0x1dc0358`, vector pointer at `+0x38`), and the path string is moved into `stream+0x18`.
   - `PHYSFS_close` (`0x170d15e`).
   - If the open fails, it logs `CLexer failed to load file `.

   **From then on, lexing reads only memory and never touches the file again.**
4. **The guitypes block**:
   - The manager's token handler is `vtbl[+0x20] = 0x14ea9f0`. On token `0x7e` (`guitypes`) it calls `0x14ece90(lexer, mgr+0xC8, mgr+0x10)`.
   - The token numbers come from the static keyword table: `guitypes`=0x7e, `guiButtonType`=0x7f, `windowType`=0x9b, `positionType`=0xb2.
   - For each element, `0x14ece90`:
     - calls the factory `(mgr+0x10)->vtbl[+0x10] = 0x14eac80(factory, lexer, tokenId)` to create a **prototype object**; for example, `guiButtonType` allocates 0x3f0 bytes and constructs with `0x15b17e0`, and `windowType` allocates 0x508 bytes and constructs with `0x155b8e0`;
     - the prototype reads itself in with `vtbl[+0x18]`, and its name comes from `vtbl[+0x40]`;
     - on a duplicate name: if the override flag `mgr+0x228` is 0, it reports `Already Defined. Key: …` and discards the new one, so the one loaded first wins; otherwise it deletes the old prototype.
     - finally, the prototypes are inserted into the type library in a batch, and `+0x20` is called to tidy it.
5. **The type library `mgr+0xC8`** (vtable `0x1d8dc00`): a case-insensitive ternary search tree.
   - `+0x08` looks up by `std::string*`: `0x14ec630`, which passes on to `+0x10`, lookup by `const char*`, `0x6e4880`; it returns the prototype pointer or 0;
   - `+0x30` inserts: `0x14ec950`;
   - `+0x20` tidies: `0x14ec750`.
6. **Instantiation**: the manager's `vtbl[+0x48] = 0x14e9030` is CreateWindow(name) (`clausewitzlib/graphics/gui.cpp`).
   - It first looks the prototype up in the type library; if it is not there, it reports `Failed to create window "%s"` and returns 0.
   - If it is there, it calls `0x155c260(proto, *(mgr+0xC0), mgr, …)`: this allocates 0xa80 bytes, constructs with `0x1543f80`, and clones the child elements one by one from the prototype's child element table.
   - Finally it calls `mgr->vtbl[+0x98]` (`0x14eb360`) to register the window.

   The army panel's root window is created this way: in `0x144fb00`, `panel+0x38 = mgr->vtbl[+0x48](mgr, "single_unitpanel")`, where `mgr = *(*(panel+0x48)+0x58)`.
7. **Hot reload**:
   - The console command `reload <file>`: `0x14e16e0` → `0x14e1140` → `0x14eb7d0(*0x23494f0, path)`, which builds a new CLexer and calls the manager's `vtbl[+0x18]` to read it in.
   - The panel constructor `0x144def0` calls `0x14e1f60(panel+0x10, proto+0x30)` → `0x14e0d90`, which registers the panel in the global table `*0x242bf58`. Inferred: this table is keyed by the prototype's source file and is used to tell panels to rebuild after a reload (matching the known `0x1450580`).


## Mechanism 2: merging definitions after parsing — feasible but fragile

All the structures it needs have been found (verified):

- A `windowType` prototype (0x508 bytes, vtable `0x1d94478`) has its own child element library at `+0xE8`, built the same way as the manager's type library.
- The prototype's token handler `vtbl[+0x20] = 0x155cb60(proto, lexer, tokenId)` creates a child prototype when it meets a child element token, then calls `0x155e3d0(proto, child, lexer, addToOrder)`: the child prototype reads itself in and is inserted into the child element library at `+0xE8`, and its name is added to the ordered name list at `+0x498`. Each type also has a name list of its own; for buttons, for example, it is at `+0x180`.

There are two ways to do it:

- after the guitypes of `unitpanel_main.gui` have been read, wrap our text in a lexer with the game's memory stream `0x171ebc0` and lexer `0x1709570`, and call `proto->vtbl[+0x20]` for each element;
- or move the child prototypes of a "placeholder window" that the mod ships into `single_unitpanel`.

Problems:

- it depends on the object layouts of the lexer and the memory stream (each token buffer is 0x200 bytes, the token number is at `lexer+0x30`, and so on), on the game's allocator, and on several name lists;
- prototypes are deleted and rebuilt on reload (when the override flag is on), so the change has to be applied again after every load or reload;
- it must be done before the panel is instantiated.

Its effect is the same as mechanism 1's, only with a much larger ABI surface. **Not recommended**; consider it only if the file-level hooks cannot be used for some reason.

## Mechanism 3: instantiating at run time — the factories exist, attaching is hard

Factories verified to exist, all on the GUI manager, whose singleton is `*0x23494f0`:

| vtable slot | Function | What it does |
|---|---|---|
| `+0x40` | `0x14e8fb0` | Clones any type by name: calls the prototype's `vtbl[+0x70]`, then registers the result |
| `+0x48` | `0x14e9030` | CreateWindow |
| `+0x58` | `0x14e9160` | Creates a `containerWindowType` (0x264) |
| `+0x60` | `0x14e93b0` | Creates a `gridBoxType` (0x275) |

A mod with an extension only has to ship a new `.gui` file defining a window with a unique name, and these factories can create it by name.

The hard part is "attaching it under the live panel":

- a live window (0xa80 bytes, constructor `0x1543f80`) keeps its child elements in internal vectors split by type (for example, pointers at `+0x738` and names at `+0x750`), which the constructor fills inline;
- a child element also needs its parent set through `child->vtbl[+0xe8]`, and then registering through the manager's `vtbl[+0x98]`;
- no separate AddChild function was found. `0x1546e70`, which the constructor calls after building each child element, is only the out-of-bounds check for `fixedwindow` (`Illegal object boundaries…`).

So attaching means writing the live window's internal vectors directly, and taking on unknown risks in rendering, input traversal and ownership at destruction. **Not recommended for injecting child elements.**

A variant: CreateWindow can create a top-level floating window of the framework's own, positioned every frame to follow the army panel, and destroyed the way the panel's destructor does it (`0x6ecc80(mgr, root)`, then `root->vtbl[+0x130]`; see [interface-bindings.md](interface-bindings.md)). This suits a separate small window "stuck beside the panel"; it is not real injection.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x19f66c0` | function | Referenced while examining hooking option a (recommended): hook the physfs exports; complete role not established |
| `0x19f6e40` | function | Referenced while examining hooking option a (recommended): hook the physfs exports; complete role not established |
| `0x19f7cb0` | function | Referenced while examining hooking option a (recommended): hook the physfs exports; complete role not established |
| `0x19f8160` | code | Referenced while examining hooking option a (recommended): hook the physfs exports; complete role not established |
| `0x19f8310` | function | Referenced while examining hooking option a (recommended): hook the physfs exports; complete role not established |
| `0x19f8c20` | function | Referenced while examining hooking option a (recommended): hook the physfs exports; complete role not established |
