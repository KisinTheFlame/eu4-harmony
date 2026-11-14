# Refreshing native window input order

This study follows the native scene preparation used by the window-ordering implementation plan: what can refresh the next input's node order without drawing a frame. Read statically from the installed EU4 1.37.5 executable on 2026-11-14, using the repository's disassembler after consulting the address index. The runtime results below cover independent traversal and repeated live-list refresh; event-level activation and ordering require their own checks. The baseline and the original scene-order study are in [window-ordering.md](window-ordering.md).

## The prepared lists

The complete first scene draw function, `0x14dd010`, clears three vector ends without freeing their storage: cursor hits at scene `+0x58`, GUI-owner nodes at `+0x70`, and drawing nodes at `+0x298`. Each vector is three pointers: begin, end, capacity end. It walks the first scene root at `+0x110`, then builds the two input lists. The GUI-owner vector contains scene-node pointers, not GUI pointers: the input reader dereferences each node's `+0x118`.

At `0x14dd23f`, unless the other camera flag at `0x233ff4f` is set, it copies the current cursor point from scene `+0x48` to `+0x50`, calls `0x14dda50(scene, draw_vector, cursor_point, hits_vector)`, and appends every draw-list node with a nonzero `+0x118` to the GUI-owner vector. The full function subsequently draws through `0x14dd350`, touches graphics state, and draws statistics; it is not an appropriate input-refresh entry.

`0x14dda50` is the hit collector. It reads each owner's slot zero to test eligibility, then clipping and the node's geometric hit method at slot `+0x88`. It appends matching node pointers in drawing order. Its output is not the keyboard list; keyboard dispatch reads the entire GUI-owner list.

## A traversal separate from drawing

`0x14dd7a0(scene, parent_transform, first_node, inherited_clip, output_vector)` visits that node and its subsequent siblings, recursively visiting children. Its complete body ends at `0x14dda40`. It does not call the draw method, allocate a texture, set graphics state or submit graphics commands. It does:

- Query node slot `+0x70` for the node's shown state and slot `+0x270` when children exist.
- Copy the parent transform, add the node's `+0x9c` and `+0xa0` translation, set z to zero and w to one, and write the resulting world transform at `+0xac`.
- Increment the rendered-node counter at `0x23494d0` for every shown node, invoke node slot `+0x48`, and append the node to the supplied vector.
- Apply inherited clipping to shown nodes and carry clipping into children according to `+0x134` (1 supplies a rectangle, 2 cancels inheritance).
- Visit the child at `+0x100`, then the sibling at `+0xf8`.

The root loop in `0x14dd010` is an inlined instance of the same traversal, supplied with the identity matrix. The four rows loaded at `0x1dc1b40`, `0x1dc1be0`, `0x1dc1fd0` and `0x1dc23f0` were read as bytes and are exactly the four rows of a 4-by-4 identity matrix. Starting the recursive traversal at scene `+0x110` with that matrix and no inherited clip therefore includes the root and applies the same root behavior. This static equivalence was subsequently checked against native drawing by the isolated traversal runs below.

All sixteen node vtables referring to visibility method `0x1148230` were enumerated from the executable. Their slot `+0x48` is `0x8f480` (returns zero without side effects). Their slot `+0x270` is `0x11483a0` (reads byte `node+0x178`), except the abstract base `0x1d94728`, whose slot is `0x8f440` (returns true). `0x1148230` reads byte `node+0x120`. The tables are `0x1d5ff68`, `0x1d60238`, `0x1d7e848`, `0x1d930d8`, `0x1d933b0`, `0x1d93c08`, `0x1d94728`, `0x1d97630`, `0x1d979c0`, `0x1d9a830`, `0x1d9b568`, `0x1d9b848`, `0x1d9d3c8`, `0x1da1d40`, `0x1da3630`, and `0x1da3960`. This enumeration covers the known node classes, not arbitrary objects a different binary or another injector could provide.

Repeated preparation must not inflate the rendered-node statistic. A refresh adapter must preserve and restore that counter while holding the presentation lock, including its failure exit. Updating only the old vectors' order is insufficient: newly shown nodes may be absent from them, and hiding a parent may retire descendants from the traversal.

## Native vector ownership

The traversal and hit collector append directly when capacity remains. On growth they call `0xcc220(vector, insertion_position, pointer_to_value)`, whose entire body was read. This grows capacity by roughly one half, allocates through `0x950b0`, copies the pointer value and existing pointer ranges, then calls `0xc9d60` to release old storage and set the new begin, end and capacity end. `0x950b0` uses operator new, with 32-byte alignment and an original-allocation pointer before buffers of at least 4096 bytes. `0xc9d60` applies the matching deallocation convention through operator delete.

An adapter must let the native traversal grow its native vector, and use the matching native insertion operation when rebuilding the GUI-owner vector. Putting Rust-owned storage in one of these vectors would violate native allocation ownership. Clearing the end pointer retains native capacity, as the ordinary draw function does.

## Mutation and refresh contract to verify

The proposed sequence is to commit registered scene order, clear the three ends, traverse the first root, rebuild cursor hits and GUI-owner nodes under the same camera condition, and restore the node counter. Run it only when a relevant change requires a refresh. It is a full first-scene walk on a change, not an every-frame scan for vanilla activation.

The existing presentation lock excludes the drawing thread while the main application frame runs. It does not alone exclude an active traversal on the same thread. The event integration must place this sequence after the current event's native candidate iteration has finished and before the next event starts; the outer input batch's return is too late. A reentrant event must not trigger mutation inside its parent's candidate iteration. A frame hook must likewise run outside node traversal.

The [first isolated traversal run](../prototype-log/2026-11-14-window-ordering-traversal.md) compared 53 samples of node sequences, transforms and clipping exactly, all equal, through loading, menu and an in-game window course with the drawing thread enabled. It used a separate native vector and did not replace the live lists.

The [activation study](window-activation.md#hit-state-freshness-is-more-than-list-order) adds two obligations beyond those three lists: rebuild the candidate array through `0x14defd0`, and invalidate the old current hit and hit-eligibility state without touching retired GUI pointers. The cursor point must remain `scene+0x48`, as in native dispatch.

The [native-drawing traversal run](../prototype-log/2026-11-14-window-ordering-native-traversal.md) added 44 matching samples with the drawing thread disabled. The [live refresh run](../prototype-log/2026-11-14-window-ordering-input-refresh.md) compared 39 pairs of consecutive preparations, including drawing nodes, transforms, clipping, hits, owners, candidates, eligibility and current hit; every pair was equal and the node counter was preserved. The [first integration run](../prototype-log/2026-11-14-window-ordering-integration.md) added 54 stable refresh comparisons and 42 matching traversals while exercising the seven-step window course. These periodic samples do not establish every intermediate show/hide or row-replacement state.

Still to verify in the running game: event-level recipients after show/hide and row replacement; presentation ownership at every chosen event seam with native drawing; pointer activation followed immediately by a shared shortcut in the same input batch; and the army picker's complete attached groups. These remain prerequisites to claiming O1 complete.
