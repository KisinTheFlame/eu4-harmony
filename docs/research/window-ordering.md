# Native window order and the army attach picker

This note investigates how native windows and their elements are ordered, and why the army panel's attach picker can be covered by Harmony's button area. It informs interface-fragment placement and extension-window design; it introduces no framework ordering capability or repair.

Date: 2026-11-14. Baseline: EU4 1.37.5 `eu4.exe`, SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`. Addresses are RVAs. Evidence is static disassembly of the installed binary through `cargo xtask disasm`, the vanilla `interface/unitpanel_main.gui`, and the framework source. REA identified the binary, but Ghidra startup timed out and returned no pseudocode; no claim below rests on a successful decompilation. No game was started or attached, and no native operation was tried.

## What establishes the order

The examined native path has an ordered scene tree, rather than a sort of all windows by a numeric z-index. The drawing walks each subtree completely before its next sibling. Showing an existing window does not by itself append it again. Native methods can change sibling order without rebuilding the window.

This corrects the earlier inference in [extension-windows.md](extension-windows.md), section 3. Creation order explains newly appended top-level windows, but not all nested elements: construction explicitly restores the order recorded in the definition. It also corrects two field interpretations in [gui-drawing.md](gui-drawing.md): node `+0xf0` is the previous sibling, not the parent; the u16 returned by draw-node slot `+0xb8` selects visual state/frame behavior, not the order of the draw list.

## Definition order becomes sibling order

The prototype reader `0x155e3d0(proto, child, lexer, add_to_order)` parses a child and registers it in the type table. When `add_to_order` is true it appends the child's name to `proto+0x498`, through `0x98090`. That append writes the old tail's `+0x28` and replaces the tail, preserving the order of these names. The window parser `0x155cb60` uses this route for ordinary elements and nested windows; the nested-window branch at `0x155ce74` also records the name in `proto+0x3e8`. Some special definitions do not join the ordered list, so this is not a claim about every parser token.

The factory `0x155c260` passes `proto+0x498` to the window constructor `0x1543f80`. The constructor was read through its return:

1. It makes the window's container node and keeps it at `root+0xa30`. Initially it appends that node to `scene+0x110`.
2. It builds the element classes separately, using a temporary parent node. It builds nested windows from the separate subwindow-name list, stores them in `root+0x958..+0x960` (`{name, window}` entries of 0x28 bytes), and gives them the temporary parent too.
3. If a parent was supplied, at `0x15460c9` it detaches the window's container and appends it to that parent.
4. It attaches the special background first, when present.
5. At `0x1546122` it starts the ordered-name walk. It looks each name up in the already-built element and subwindow tables, then calls the found GUI object's slot `+0xe8` with `root+0xa30`. It advances through the name node's `+0x28` until the list ends.

Thus the type-by-type allocation order is not the final sibling order. For the ordinary entries on this path, the definition's recorded order is restored across element kinds, including nested windows. A name lookup and a show call later do not repeat this construction step.

## Scene links and native reorder methods

The node constructor is `0x1561fd0`. The window container's vtable is `0x1d60238`; the base node's is `0x1d94728`. Direct link writes in the methods below establish these fields:

| Node field | Meaning |
| --- | --- |
| `+0xf0` | Previous sibling |
| `+0xf8` | Next sibling |
| `+0x100` | First child |
| `+0x108` | Last child |
| `+0x110` | Parent |
| `+0x118` | GUI interface pointer used by input selection |
| `+0x120` | Shown flag used by the node's visibility methods |
| `+0x128..+0x12f` | Clip rectangle, four i16 values |
| `+0x130` | Clip enabled |
| `+0x134` | Clip inheritance mode examined by the traversal |

| Node slot | RVA | Operation established by its complete body |
| --- | --- | --- |
| `+0xe0` | `0x1562390` | Detach: repair neighboring links and the parent's first/last links, then clear parent/previous/next |
| `+0xc8` | `0x1562420` | Detach, then append to a supplied parent's children |
| `+0xd0` | `0x1562480` | Detach, then prepend to a supplied parent's children |
| `+0xd8` | `0x15624e0` | Insert after a supplied sibling; unlike the two methods above, it does not first detach itself |

Appending to the same parent moves a node and its attached subtree after its siblings. These methods change the tree, not the already-prepared flat draw list. Their existence does not establish when a framework call may safely mutate that tree beside the drawing thread.

A window's GUI interface is `root+0x30` (vtable `0x1d929d0`). Its parent setter, slot `+0xe8`, is `0x154a440`: it takes the container at `gui+0xa00`, detaches it and appends it to the supplied node. Root method `0x154a480` returns the container's parent.

There is also a window-level method, root slot `+0x2b0`, `0x1547f10(root, name)`. It calls GUI slot `+0x1e8`, `0x1548040`, to look up an element by the supplied name. If found, it calls that element's parent setter with the window's container, then removes its old entry from the window's list at `root+0x158` (or marks the entry removed if that list is being iterated) and appends a new entry at the tail. The [placement study](window-placement.md) corrects the interpretation of that list: ordinary subwindows retain an entry in the separate list at `root+0x140`, so this method can add duplicate membership across the two lists. It is not a general scene-order synchronization operation. Root methods `0x15465a0` and `0x15465f0` instead move the window container within its existing parent while leaving both internal lists and manager registration unchanged. The lookup includes subwindows, returning their GUI subobject. Its array lookup `0x154f650` returns the matching existing element; its subwindow-table access `0x154ed50` is preceded by a membership check on this path. This method was not called in a running game, and its original indirect callers were not resolved.

GUI slot `+0x110`, `0x154e4a0`, does something different: it detaches the whole window container and appends it to `scene+0x2b0`, the second scene tree. Direct-caller searches found vtable references but no direct calls. Its native callers and intended window kinds remain unknown. It must not be treated as a proven general-purpose interactive-window front operation.

## Drawing and input

`0x14dd010` builds the first tree's list at `scene+0x298`; `0x14dd540` builds the second at `scene+0x438`. Both use the recursive traversal `0x14dd7a0`: test visibility, update the world transform, append the shown node, descend through `+0x100`, then follow `+0xf8`. A subtree is contiguous in this traversal. `0x14dd350` draws the resulting list from beginning to end without sorting it by a node field. The second pass follows the first in the in-game draw path ([gui-drawing.md](gui-drawing.md)).

`0x14dda50` scans a supplied draw list forward. It requires a GUI owner at node `+0x118`, checks the owner's acceptance method, applies the clip rectangle when enabled, and calls the node's geometric hit test. It appends passing nodes in that order.

The input paths add conditions to this order:

- Ordinary mouse input uses `0x14ea3d0`, which consumes the cached cursor hits, calls GUI slot `+0x10`, and stops on a nonzero 16-bit handled mask. The common dispatch `0x1552800` provides an actual accepted-target observation seam. The follow-up [activation study](window-activation.md) corrects the earlier event-source classification and records the per-event return seam and derived hit-state requirements.
- `0x14ea630` handles event source 3, whose product name remains unknown. For input kind 2 at `input+0x38`, builds candidates from the hit list in reverse, excluding hidden or dead GUI objects (`+0xc5`, `+0xc4`). It dispatches through GUI slot `+0x10` until bit 1 of `input+0x54` says the event was handled.
- Its other branch calls `0x14ddec0`. That function asks `0x14de440` for candidate lists at the pointer and, for a positive radius, at nearby points. Each individual list comes from the first draw list in reverse order, with a limit of 1,000 candidates. `0x14ddec0` merges candidates by their rank, includes a pass over the values 0 through 2 of the GUI object's `+8` field, and uses `0x14dde00` to avoid duplicates. The meaning of that classification and the radius setting were not established. This source-3 path does not establish ordinary mouse selection behavior.
- `0x14ea150` gives a shown object at `manager+0x140` an earlier opportunity to handle input. If still unhandled, it walks `scene+0x70` backward, filters visibility, liveness and `+0x8d`, and calls GUI slot `+0x48`. The shortcut/modal behavior described in [extension-windows.md](extension-windows.md) depends on these additional rules.

In `0x14dd010`, `scene+0x58` contains the cursor hit list, but `scene+0x70` is built from the entire first draw list's nodes with a GUI owner, not just the cursor hits. The second pass does not build either input list. Moving an ordinary interactive window into the second tree therefore changes more than its relative draw order.

Drawing, clipping, event handling and modal priority are related but distinct. Reordering the tree changes future draw-list and hit-candidate order; it does not by itself prove a new modal or keyboard policy.

### The u16 draw argument is not a z-index

`0x14dd350` obtains a u16 from node slot `+0xb8` and passes it to the node's draw method. Window containers return zero. Text and sprite nodes return `node+0x1bc` through `0x1550b20`.

In the text-node draw `0x1550950`, that value selects among text style/color entries for values 2, 3 and 4. The downstream drawing function `0x1551840` uses `(value - 1) % frame_count` to select a texture frame, unless `node+0x64` supplies an explicit frame override. Sprite drawing `0x1558b90` forwards both this u16 and the explicit frame override. The exact names of the state values were not recovered. These reads establish visual-state/frame selection; they provide no evidence for numeric depth sorting.

## The army attach picker

The army-panel constructor `0x144fb00` builds `single_unitpanel` and subscribes its attach and detach buttons to the panel's action. `attach_unit_window` is already a nested window in that definition; the action does not create a new top-level picker.

The action `0x1487490` has three relevant routes:

- An already attached army posts `detach_unit`.
- With the action's Shift flag at `+0x10` set, it obtains candidates through `0xd56750`, posts `attach_unit` for the first candidate if there is one, and closes the picker.
- Otherwise it finds the existing subwindow, checks `root+0xf5`, and closes it if shown or calls `0x1474f80` to open it. There is no single-candidate shortcut in this ordinary-click branch.

`0x1474f80` looks up `attach_unit_window`, calls its GUI show slot `+0x88` at `0x1475021`, clears its `list`, obtains candidates, and adds one `attachable_army_entry` per candidate. The row constructor is `0x1470d60`; the list insertion and layout are `0x152e910` and `0x1531850`. Layout attaches each row's window to the list's node through `0x152e090` and the window parent setter. New rows remain inside the picker subtree: their later allocation cannot put them beyond a later sibling of the picker.

Subwindow lookup `0x1548190` forwards to `0x15481a0`, which returns an existing entry from the subwindow table. Show `0x1547d90` sets visibility, shows the background and propagates show to children through `0x154e500`; node show `0x15625a0` sets `+0x120`. The examined picker path does not call the node reorder methods or the window's named-child reorder method. Close `0x1475280` hides the same subwindow. Ordinary row selection `0x14710c0` posts the attach command and closes it; a separate confirmation branch was not expanded here.

### Why the button area overlaps it

In vanilla `unitpanel_main.gui`, `attach_unit_window` is at `(490, 50)` inside `single_unitpanel`, with size `(300, 300)`. Its list begins at `(15, 50)` within it, hence `(505, 100)` relative to the army panel.

In the framework source inspected:

- `core/src/hosts.rs` places the first button at `(515, 80)`. The one-column background begins at `(504, 69)` and has size `(70, 392)`, even with only one button.
- `core/src/pdx.rs::splice_window` inserts fragments just before the host definition's closing brace.
- `core/src/splice.rs` orders the added background before added buttons, but both come after the original definition's contents, including `attach_unit_window`.
- `core/src/ui.rs` shows the background whenever a declared button-area button or icon is visible. It does not consult picker visibility or change sibling order. The disabled background still consumes clicks, as the earlier game runs in [extension-windows.md](extension-windows.md) established.

The resulting relevant order is: original picker subtree, added background subtree, added controls. The rectangles overlap, construction restores that order, and opening the picker only shows it. This provides a static causal explanation for the reported visual obstruction. It does not constitute a runtime capture of this player's exact scene or a measurement of which pixels and clicks are blocked.

## Design constraints and remaining verification

The native mechanisms support an ordering policy expressed relative to a parent and its children. A global numeric z-index is not required to explain or address this particular conflict. Whether the framework should expose ordering publicly, or keep it internal to host bindings and extension windows, remains a design decision.

Any implementation must preserve the window's own lists as well as scene links where native methods maintain both, keep all parts of a button area in their intended relative order, and reapply the policy when a host is rebuilt. It must also coordinate tree mutation with preparation and drawing; this study does not establish a safe scheduling point for a new native call. Moving a window between parents can change transforms and clipping, so sibling reordering and reparenting must be treated separately.

Limits of this study:

- Native reorder methods were read, not executed. Pointer validity, refresh timing, reload behavior and the effect on this exact overlap still need a controlled game run before a repair can be called verified.
- No exhaustive search of indirect callers was performed. In particular, the callers of the named-child reorder method and second-tree migration method remain unresolved.
- The pointer-input classification and radius merge, modal ownership setters, and exact visual-state enum names remain unresolved. No claim of a universal input-priority rule or of the absence of every numeric ordering field is made.
- Complete bodies of the central constructor, traversal, list drawing, link operations, candidate selection and picker action/opener were read. Their relevant GUI callees were followed; generic string/allocation helpers and unrelated sound, command, confirmation and first-show initialization paths were not exhaustively reconstructed.
- This is research only: no source behavior, Lua interface, installation or player files changed.
