# Placing native windows among siblings

This note establishes the scene-placement contract for the private [window-ordering process](../plans/window-ordering.md), including the distinction between the scene's sibling order and a window's internal child lists. It refines the interpretation of `0x1547f10` in [window-ordering.md](window-ordering.md): that method does **not** synchronize a complete mirror of scene order. The implementation must not use it indiscriminately for ordinary child windows.

Date: 2026-11-14. Baseline: EU4 1.37.5, SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`. Addresses are RVAs. Existing research and the framework's `windows/lifecycle.rs`, `windows/live.rs`, `windows/mod.rs` and `ui.rs` were examined; newly investigated functions were looked up with `cargo xtask what` before disassembling the installed executable with the existing analysis executable. No game was started or modified. This note establishes static contracts, not a runtime placement result.

## A native window already has same-parent front and back methods

The ordinary root vtable `0x1d926b0` contains these methods, both taking only the root:

| Root slot | RVA | Complete behavior |
| --- | --- | --- |
| `+0x268` | `0x15465a0` | Read `root+0xa30`, save that node's parent at `+0x110`, detach the node, append it to that same parent |
| `+0x270` | `0x15465f0` | Read the same node and parent, detach the node, prepend it to that same parent |

Both complete bodies were read. Neither updates a GUI-manager vector, an idler list, nor either of the containing window's internal lists. Neither changes visibility, position, focus or modal ownership. Neither checks for a null parent: a caller must establish the live attached instance first. Both preserve the subtree rooted at the window's container.

These functions are not specific to a top-level parent: they use the actual current parent. Thus there is direct native evidence for same-parent window placement by scene links alone, including an ordinary nested window. This does not justify moving arbitrary control internals independently, nor migrating a window to another parent.

The method table search found references in the ordinary root table and the derived table `0x1d57768`, with no direct call sites. Original indirect callers have not been resolved. The complete method bodies establish the mutation contract independently of that unresolved caller inventory.

## Which other collections must remain unchanged

The complete `CreateWindow`, `0x14e9030(manager, name)`, builds a window with no parent through `0x155c260`, then registers its GUI subobject, `root+0x30`, through manager slot `+0x98`. The complete registration method `0x14eb360(manager, gui)` appends the GUI pointer to `manager+0x110`; if GUI slot `+0x238` requests updates, it also adds the pointer to `manager+0x170` unless already there.

These are lifetime/update registration vectors. Their consumers and the separate idler update list are documented in [notice-window.md](notice-window.md#3-destruction-paths-and-where-to-hook). The actual mouse and shortcut lists are prepared from the scene traversal, as traced in [window-activation.md](window-activation.md) and [window-input-refresh.md](window-input-refresh.md). The native same-parent front/back methods leave the registration vectors untouched. The placement adapter should do the same; sorting manager registration order would add an unrelated change to update and teardown order.

The same applies to the original subwindow name table at `root+0x958`: it remains the owner's lookup and lifetime inventory. Placement is not a removal from that table, a new registration, or a reconstruction.

## Why named-child promotion is not the general placement primitive

`0x1547f10(root, name)` does exactly the writes already reported in the earlier note: resolve a GUI object using `0x1548040(root+0x30, name)`, invoke its parent setter to append its scene nodes, remove its old entry **from `root+0x158` if present**, then append a new entry to `root+0x158`. The correction is what that list means relative to the rest of the window.

The constructor `0x1543f80` has two distinct child lists, not one scene-order mirror:

- At `0x154406a`, and again at `0x15460b3` immediately before the final order-restoration walk, `r15` is `root+0x140`.
- The common constructor path at `0x1546454` appends the found GUI object to the list at `r15`, through `0xdf120`. This includes ordinary nested windows: the subwindow lookup at `0x15463f1` obtains `child_root+0x30` and joins that common path.
- A separate element-kind branch at `0x15462d0` finds an object in the pointer table at `root+0x768`, attaches its scene nodes, and at `0x1546310` appends it to **`root+0x158`** instead.
- The special background at `root+0x520` is attached before the ordered-name walk, at `0x1546106`. That attachment does not append it to either of these lists.

`0xdf120` was read completely: it allocates one 0x20-byte native list node and appends `{item, previous, next, removed}` to the supplied header. It performs no uniqueness check.

Consequently, calling `0x1547f10` on a normal nested window can leave the existing GUI entry in `root+0x140` **and add the same GUI pointer to `root+0x158`**. It does not remove or reorder that original `+0x140` entry. If it is called repeatedly, it removes/replaces only the `+0x158` entry. During iteration (`root+0x16c != 0`) it marks that entry removed instead of freeing it immediately.

These lists have observable consumers beyond drawing. GUI slot `+0x150`, `0x1546660(gui)`, walks `gui+0x110` and then `gui+0x128` (root `+0x140` and `+0x158`) and invokes every item's slot `+0x150`. An item present in both receives two calls on that path. GUI slot `+0x148`, `0x154bd50`, likewise walks both lists and calls each item's `+0x138` with zero. GUI slot `+0x140`, `0x154bcb0(gui, name)`, searches only the second list and may call `+0x138` with one. All three complete bodies were read. The product-level names of this propagation/selection state have not been established; the duplicate-membership distinction does not depend on assigning them a name.

The named lookup is also narrower than the constructor's full lookup walk. `0x1548040` tries two parallel-array groups, two named maps, the subwindow table, and one final array group; the constructor tries additional element groups. An absent result does not prove that the host has no child of that name. A successful lookup must be compared with the already-bound expected GUI pointer before relying on it.

Therefore do not replay a desired full host order by calling `0x1547f10` for every named child. Do not describe `root+0x158` as an order that must always mirror the complete scene list. Preserve both internal lists when doing same-parent scene placement, just as the window's own front/back methods do. A future use of the named-child method needs its own reason for changing second-list membership.

## Button-area objects can own more than one direct scene node

The complete parent-setter bodies establish the following for the bound classes used by the framework:

| GUI class / setter | Scene nodes moved, in order | Additional parent bookkeeping |
| --- | --- | --- |
| Ordinary window, `0x154a440` | Pointer at GUI `+0xa00` = root `+0xa30` | None outside the scene links |
| Button `0x1d58218`, `0x15ae9c0` | Required pointer at `+0x290`, then optional pointer at `+0x188` | Writes GUI `+0x228` |
| Special background `0x1d954b0`, `0x15714f0` | Pointer at `+0x168` | Writes GUI `+0x1a8` |
| Game icon `0x1d585c8` or base icon `0x1d96d00`, `0x1586190` | Pointer at `+0x168` | Writes GUI `+0x170` |
| Text box `0x1d8fd10`, `0x152b0a0` | Renderer pointer at `+0x118`, then the embedded container at `scrollbar+0x528` when the scrollbar pointer at `+0x1a0` is nonzero | Writes GUI `+0xd8` |

The button setter's two nodes are siblings under the supplied parent. Moving only the node whose owner equals the button pointer can split its visuals if the other node has no owner or a different one. Names and node-owner pointers alone are insufficient to discover a complete arbitrary control group.

### Game icons have a derived class

Follow-up, 2026-11-14: investigation of the failed [actual-playset run](../prototype-log/2026-11-14-window-ordering-user-game.md) exposed a class restriction missing from the adapter contract. The following complete function bodies and selected method tables were read from the same installed baseline; no additional game was started.

- The game's definition factory `0x10b3fe0` handles token `0xb6` (subtract `0x7f`, then `0x37`). It calls the base icon-definition constructor `0x1592000`, then installs definition vtable `0x1d58938` at `0x10b40a1`. That table's slot `+0x70` is `0x10b5fb0`.
- Both branches of `0x10b5fb0` allocate `0x1f0` bytes and call `0x10b5780`. That constructor calls base icon constructor `0x15857d0`, then installs live vtable `0x1d585c8` at `0x10b5823`, with associated interface tables at `+0xc8` and `+0xf8`. Its extra storage begins at `+0x1e8`; it constructs no additional scene node. This differs from base definition factory `0x1592580`, which allocates `0x1e8` bytes and returns the base constructor's class `0x1d96d00` unchanged.
- Game icon vtable `0x1d585c8` retains show/hide `0x1585e80` / `0x1585eb0`, placement `0x1585c80`, and parent setter `0x1586190`. The complete parent setter detaches and appends only `*(icon+0x168)`, then stores the parent at `+0x170`. Its scene-node group is therefore the same single node as the base icon's. Earlier [panel-strip research](extension-windows.md#moving-an-element-there-every-frame) also recorded this derived vtable on a spliced icon in the running game.

The actual fragment declares a button `toggle` and an `iconType` `glow`. Its run logged two mod elements and one button slot, then `ui: unrecognized attached visual node group` at 20:35:21.818. `ui::bind_one` already checks the button's exact class; the remaining icon reaches `window_ordering::attach`, which accepts only base icon class `0x1d96d00`. It rejects the game's derived class before `Registry::attached` is called. `ui::bind` then removes the whole binding and unlinks its button subscriptions. The native spliced elements remain present, but receive no attached ordering registration. This explains the unchanged overlap and also means the failure affects the mod's binding, beyond drawing order alone.

The prior pointer fixture declares only one button, so it never exercised this icon path. A correction must recognize the checked game icon class and its inherited parent setter, register the constructor/field dependencies, and retain complete sibling validation. It must not accept arbitrary unknown classes as one-node controls. Regression coverage needs a button and icon together, followed by acceptance with the actual playset. No code correction or successful actual-playset acceptance is claimed here. The failed run did not record the glow object's address or vtable directly; its type is established by the declaration and static game factory path, rather than by a new runtime object read.

### A text box with a scrollbar contributes two sibling subtrees

The existing [text study](panel-entry-text.md#positioning) identifies the renderer pointer at text box `+0x118` and the optional scrollbar pointer at `+0x1a0`. The complete text-box constructor `0x1529d20` calls scrollbar factory `0x15e8c30` at `0x152a26a` and stores its result at `+0x1a0` at `0x152a292`. The complete factory allocates 0x6d0 bytes and calls constructor `0x1552b80`; it does not replace the constructed class with a derived class before returning.

The scrollbar constructor, read through its return at `0x1553f0f`, installs GUI class `0x1d93748` at `0x1552bfc`. It initializes an **embedded** container at `scrollbar+0x528`, selected at `0x15531f1`, and installs node class `0x1d60238` there at `0x155321f`. This is an address within the scrollbar allocation, not a pointer loaded from that offset. The initial scene attachment appends this container to `scene+0x110`. The constructor supplies that container as parent when constructing the visual objects stored at scrollbar `+0x3e0`, `+0x2f8`, `+0x158`, `+0x2f0`, and optional `+0x4c8` and `+0x4d0`. Those objects belong within its subtree, not in the text box's host-level node group. Their individual visual roles and complete factory internals were not investigated here.

Scrollbar GUI slot `+0xe8` resolves to `0x1554660(scrollbar, parent)`. Its complete body computes `scrollbar+0x528`, calls that node's `+0xe0` detach method, and ends with a tail call to its `+0xc8` append method at `0x1554693`. For node class `0x1d60238`, these resolve to `0x1562390` and `0x1562420`, whose complete bodies were already read. The setter moves only this one container subtree; it neither enumerates additional siblings nor updates another parent field.

The complete text-box setter `0x152b0a0` first attaches the renderer to the requested parent. If the scrollbar exists, it detaches the embedded container and then invokes scrollbar slot `+0xe8` at `0x152b0ef`, which detaches that same container again before appending it to the same parent. Thus the complete native attachment order is `[*(text_box+0x118), scrollbar+0x528]` when the scrollbar exists, and just the renderer otherwise. The scrollbar's internal controls move with its container; they must not be detached and inserted as extra siblings of the renderer.

For checked placement, validate the bound text-box class `0x1d8fd10` and its renderer layout. For a nonzero scrollbar pointer, additionally validate scrollbar class `0x1d93748`, its `+0xe8` method `0x1554660`, and the embedded node class `0x1d60238` at `scrollbar+0x528`. Validate the moved nodes' native methods and require both nodes to occur exactly once under the expected host parent in the complete sibling snapshot. Preserve their existing relative order when moving the group. These checks support the nonzero-scrollbar case directly; its presence alone is not a reason to reject the text box. Same-parent ordering uses node placement, without replaying the GUI parent setters or changing positions.

The framework's button-area frame is already a nested ordinary window (`ui::bind_frame`), so its full subtree is one movable node. Its background must remain inside that subtree. The host's own special background is a different object and must remain behind the host's contents.

## Concrete adapter contract

A scene-placement adapter can use the native detach, prepend and insert-after methods already read in [window-ordering.md](window-ordering.md), without reconstructing nodes or editing internal membership lists:

1. Resolve each live generation to its complete movable node group. Ordinary extension windows, pop-ups, and ordinary original child windows each contribute their container node. A bound button contributes both of the setter's nodes, when present, preserving their existing relative order. Use explicit checked class/layout knowledge for other bound controls; an unknown node group is not a one-node fallback.
2. Snapshot the **complete** sibling chain, including hidden nodes and nodes outside ordering registration, using a bounded loop check. Verify every expected node is present exactly once, has the same nonzero parent, and has consistent neighboring/parent links. Validate native method slots on moved nodes. Do not derive a full placement from only the visible draw list.
3. Form a desired sibling sequence which changes only the allowed relationships. Keep the relative order of original windows; keep unregistered nodes in their original relative order; keep every control's complete node group together. The host's special background remains before all its contents. The attached button area is not a separate activation target.
4. Realize the desired sequence by moving only framework-owned windows or the checked attached node groups around unchanged anchors. To insert after a sibling, call detach first and then `0x15624e0(node, predecessor)`. For an actual head insertion, use `0x1562480(node, parent)`, which already detaches. Never insert a node after itself. Never prepend an attached button area blindly ahead of the host's background.
5. Validate the resulting complete chain against the desired sequence and generation tokens. A stale generation or changed parent invalidates the plan. If a native call faults, do not report the plan committed: resnapshot live instances at the next permitted seam and recompute from reality.
6. Rebuild the prepared input lists and derived hit state before the next event, under the established presentation ownership and outside active candidate/tree traversal. Scene mutation itself does not make the old input vectors current.

For the army picker relationship, the fixed host background and anonymous original nodes are anchors, not objects to look up by synthetic names. Moving the complete attached group immediately behind the relevant original child windows can establish picker coverage without moving that background or rewriting pure-original window order. The exact full sibling inventory and grouping must be checked in the isolated test; the static explanation does not prove that this particular host contains no additional nodes.

Hidden instances still participate in the native-chain validation. Showing or hiding is not performed by placement. A technical rebuild supplies a new node group to the retained logical identity; the old node addresses are retired before Kill/destruction.

## Candidate dependency bytes

These were read from the installed executable. The implementation's `window_ordering::points` registers the subset used by its adapter, with the existing object points supplying shared dependencies. The complete table also includes investigated operations that the implementation deliberately does not call, such as named-child promotion. Field-use points cover the offsets actually read by the adapter.

| RVA | Expected bytes | Meaning |
| --- | --- | --- |
| `0x15465a0` | `48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8b 89 30 0a 00 00 48 8b 01 48 8b 99 10 01 00 00` | Root same-parent append; root `+0xa30`, node parent `+0x110` |
| `0x15465f0` | `48 89 5c 24 08 57 48 83 ec 20 48 8b f9 48 8b 89 30 0a 00 00 48 8b 01 48 8b 99 10 01 00 00` | Root same-parent prepend |
| `0x1562390` | `48 8b 91 f8 00 00 00 48 85 d2 74 0e 48 8b 81 f0 00 00 00` | Node detach; next and previous siblings |
| `0x1562420` | `48 89 5c 24 08 57 48 83 ec 20 48 8b 01 48 8b fa 48 8b d9 ff 90 e0 00 00 00` | Node append, including virtual detach |
| `0x1562480` | `48 89 5c 24 08 57 48 83 ec 20 48 8b 01 48 8b fa 48 8b d9 ff 90 e0 00 00 00` | Node prepend, including virtual detach |
| `0x15624e0` | `48 8b 82 10 01 00 00 48 89 81 10 01 00 00 48 8b 82 f8 00 00 00` | Insert already-detached node after a sibling |
| `0x15464b0` | `49 8b 86 30 0a 00 00 49 8d 4e 30 48 89 88 18 01 00 00` | Window container and its GUI owner |
| `0x15ae9d4` | `48 8b 89 90 02 00 00` | Button's first scene node |
| `0x15ae9fa` | `48 8b 8b 88 01 00 00` | Button's optional second scene node |
| `0x1571500` | `48 8b 89 68 01 00 00` | Special background's scene node |
| `0x15861a0` | `48 8b 89 68 01 00 00` | Icon's scene node |
| `0x152b0b0` | `48 8b 89 18 01 00 00` | Text box's first scene node |
| `0x152b0d3` | `48 8b 8b a0 01 00 00 48 85 c9 74 23 48 81 c1 28 05 00 00` | Optional scrollbar and its embedded scene node |
| `0x152b0ef` | `48 8b 8b a0 01 00 00 48 8b d7 48 8b 01 ff 90 e8 00 00 00` | Text box forwards its requested parent to scrollbar slot `+0xe8` |
| `0x1552bfc` | `48 8d 05 45 0b 84 00 48 89 01` | Scrollbar constructor installs GUI class `0x1d93748` |
| `0x15531f1` | `49 8d b6 28 05 00 00` | Scrollbar's embedded container address, not a loaded pointer |
| `0x155321f` | `48 8d 05 12 d0 80 00 48 89 06` | Embedded container uses node class `0x1d60238` |
| `0x1554660` | `48 89 5c 24 08 57 48 83 ec 20 48 8d 99 28 05 00 00` | Scrollbar parent setter selects its embedded container at `+0x528` |
| `0x1554674` | `48 8b 03 48 8b cb ff 90 e0 00 00 00` | Scrollbar parent setter detaches that container through node slot `+0xe0` |
| `0x1547f10` | `48 89 5c 24 10 57 48 83 ec 20 48 8b 41 30 48 8b d9 48 83 c1 30 ff 90 e8 01 00 00` | Named-child operation, not the recommended placement primitive |
| `0x1547f53` | `48 8b 8b 58 01 00 00` | Named-child operation searches only the second internal list |
| `0x15460b3` | `4d 8d be 40 01 00 00` | Constructor's common child-list header |
| `0x1546310` | `49 8d 8e 58 01 00 00` | Constructor's separate child-kind list header |

The node class `0x1d60238` was checked to contain append/prepend/insert-after/detach at `+0xc8/+0xd0/+0xd8/+0xe0`. The root class was checked for the two same-parent methods and named-child slot `+0x2b0`; GUI class `0x1d929d0` for parent setter `+0xe8` and child lookup `+0x1e8`.

## Limits

The static investigation itself executed no native placement. Later [keyboard testing](../prototype-log/2026-11-14-window-ordering-keyboard.md) exercised nontrivial placement and verified the resulting complete sibling chains; [native-drawing testing](../prototype-log/2026-11-14-window-ordering-startup-screen.md) identified the original startup screen's first-key handling and passed the prepared three-key course. Remaining runtime obligations include the army host's complete node-group inventory, paired button visuals, background placement, clipping/transform equality after same-parent movement, teardown after movement, and pointer input freshness. The text-box scrollbar's complete parent setter and container layout are established statically; moving a live text box with a nonzero scrollbar has not been tested in this investigation. The specific semantic name of the second child list and the original indirect callers of named promotion remain unknown. None of those unknowns warrants treating `root+0x158` as a complete scene-order mirror.
