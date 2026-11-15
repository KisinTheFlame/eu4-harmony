# Window visibility transitions and lifetime

## Four display methods, not two

For ordinary `windowType` instances, the root vtable is `0x1d926b0`; the GUI interface at `root+0x30` has vtable `0x1d929d0`. All four methods below take one argument, the indicated receiver. `cargo xtask disasm args:...` also reports one argument each.

| Method | RVA | Receiver | Relevant effect |
| --- | --- | --- | --- |
| GUI `+0x88` | `0x1547d90` | GUI | Show own node when configured, set the local open byte, show background, visit children with show, permit child traversal |
| GUI `+0x90` | `0x1547b00` | GUI | Hide own node, clear local open byte, notify listeners, hide background, visit children with hide, suppress child traversal |
| Root `+0x40` | `0x1547cc0` | Root | Set local open byte, show background, permit child traversal; no child show visitor |
| Root `+0x48` | `0x1547a50` | Root | Clear local open byte, hide background, suppress child traversal; no child hide visitor |

`root+0xf5` and `gui+0xc5` are the same local open byte. Both show methods return without showing when `root+0xdb` (`gui+0xab`) is nonzero. The meaning/name of that prohibiting flag is not established here, but its branch is explicit. Show can additionally enable the GUI subtree through GUI slot `+0x210`, `0x154dd30(gui, enabled)`, if `gui+0x8d` is false. That setter writes `+0x8d`, forwards to the background and visits the children with the same boolean. This is separate from visibility.

GUI show checks the old local open byte only to decide whether to notify the manager through its `+0xa8` slot. An already-open window still reaches the visibility writes and child visitor. Root show does not have an old-open check. Thus an observer that treats every call as an appearance is wrong: repeated show can repeat propagation without a local closed-to-open transition. Both hide methods likewise execute even when the local byte is already zero.

The root methods also contain audio handling. The older description that they set the byte and background "without the rest" should be read narrowly as the absence of the child visitor, not as an absence of sound side effects.

## Parent propagation and effective visibility

`0x154e500(root, visitor)` is a general element visitor, not merely a listener notification. It iterates the window's typed element tables and its subwindow entries at `root+0x958..+0x960`; each subwindow contributes its GUI pointer, `child_root+0x30`.

GUI show supplies the one-method visitor vtable `0x1d92c28`, whose callback `0x15f5e60(visitor, gui)` tail-calls `gui->slot(+0x88)`. GUI hide supplies `0x1d92c30`, whose callback `0x15f5e50(visitor, gui)` tail-calls `gui->slot(+0x90)`. Therefore ordinary nested windows really enter the same show/hide functions recursively; the visitor is not just a notification that leaves children's open bytes alone. A parent's GUI show can make a previously hidden ordinary child locally open, unless that child's prohibiting flag rejects show. Native behavior, rather than a newly invented ownership rule, must remain authoritative.

The scene node at `root+0xa30` has two distinct flags:

- `node+0x120`: whether the node itself is appended for drawing. Slot `+0x70`, `0x1148230`, returns this byte. `0x15625a0(node)` sets it and `0x1558b80(node)` clears it.
- `node+0x178`: whether its children may be traversed. The window-container slot `+0x270`, `0x11483a0`, returns this byte. All four window display methods set or clear it; it is not an aggregate computed from the children's visible flags.

The complete traversal `0x14dd7a0` distinguishes these flags. It updates a node's transform if the node itself is shown **or** there are children and `+0x270` permits walking them. It appends the node only if its own `+0x70` succeeds; it recurses only if the separate child condition succeeds. A parent with an invisible own node can still have drawn children. Conversely a child with a true local open byte is not effectively visible beneath a parent whose child traversal is disabled. Testing every ancestor's own `+0x120` is therefore not a correct visibility test.

For the examined ordinary window-container ancestors, effective window display needs the window's local open state and an unbroken chain of ancestors that permit traversal into its subtree. The root methods make ancestor-aware observation essential: hiding the parent through root `+0x48` leaves children's local open bytes unchanged while preventing their drawing. Restoring it through root `+0x40` permits those children again. A parent's GUI method instead also changes descendant local flags recursively.

A child show observer returns before the outer parent show finishes: GUI show sets its own `node+0x178=1` only after the child visitor. Any registration that calculates effective visibility at each child return must also reevaluate affected registered descendants when the parent operation returns. It must not require the child to emit another show call once the parent gate has become true.

These facts support observing entry and successful return of all four methods, comparing actual state rather than requested state, and reevaluating the registered windows affected by a changed ancestor. Separate hide and show calls in one frame remain distinct observations; one end-of-frame sample cannot recover their ordering. This is an integration contract derived from static control flow, not a runtime-verified hook implementation.

## Retirement and reconstruction

`0x1547520(root)` (root slot `+0x130`) first writes `root+0xf4=1`, then marks the typed children dead and recursively invokes Kill on the ordinary subwindows. It finally marks the background dead. The complete body frees no memory and does not clear `root+0xf5`. A locally open dead window must be excluded from ordering; visibility alone is not a lifetime check.

GUI Kill `0x1547910` is already documented in [notice-window.md](window-construction.md#32-root-window-kill-root-vtbl0x130--0x1547520-verified) as the `root+0x30` adjustment and virtual root Kill dispatch. Existing `core/src/ui.rs` observes root Kill at entry, and has the deleting destructor as a backstop. Those entry points can retire ordering identities while their native memory still exists. Do not wait for a later frame to inspect retired memory.

The ordinary deleting destructor `0x1546510(root, flags)` takes two arguments. It calls `0x1546730(root)` and frees the 0xa80-byte allocation if `flags & 1`. The destructor body first reinstalls the ordinary root/GUI vtables, passes the scene node to `0x14db6a0`, then destroys the typed containers, name storage and internal lists before ending in the base teardown. Destruction is distinct from Kill, and native pointers must be invalidated before it. The earlier lifetime study also found a derived-class destructor which calls the body directly; an observer only on the ordinary deleting destructor is not a proof of coverage of arbitrary derived classes.

A construction address cannot identify a new logical appearance. As recorded in the plan, the framework rebuilds still-wanted windows when the definition generation changes. The replacement native instance must attach to the retained logical identity, while a real close/open sequence remains a new activation. The ordering process should explicitly retire its own destruction paths too: existing `windows::killed` uses a nonblocking lock and may not observe a callback reentering framework-owned reconciliation.


## Shared display methods also receive derived windows

Follow-up, 2026-11-15: the reported army-panel crash reached root hide with root class `0x1d57768` and GUI class `0x1d57510`. A temporary diagnostic in the failing ordinary-window check captured both vtables in an isolated test game using the player's unchanged playset. Selecting an army while paused survived; advancing without selecting an army also survived. Selecting an army and advancing failed after the first day, when the panel's attached elements had registered an ordering scope. The earlier [actual-playset acceptance run](../../research/interface-evidence.md) had kept the game paused and did not cover this course.

The complete derived constructor `0x10a14d0` calls the ordinary constructor `0x1543f80`, then installs root vtable `0x1d57768` at `0x10a186f` and GUI vtable `0x1d57510` at `0x10a1879`. Its extra members start at `+0xa80`. The complete deleting destructor `0x10a1cb0` reinstalls those tables, destroys its extra members, calls the ordinary destructor body `0x1546730`, and conditionally frees a `0xbd0`-byte allocation. The derived root table retains ordinary root show `0x1547cc0` at `+0x40` and root hide `0x1547a50` at `+0x48`; its Kill slot `+0x130` instead holds `0x10a1dd0`. Generic allocation/container helpers were not expanded; none of their internal semantics is needed to establish this inheritance and dispatch contract.

The captured stack also contains return address `0x123fedc` in the message-list item constructor `0x123f800`. Its complete body stores its incoming popup pointer at item `+0x130`, configures the row and its controls, and calls that incoming object's virtual slot `+0x70` at `0x123fed9`. This places the observed hide within message-list construction; the intervening virtual method was not resolved, and the derived window's exact in-game name is not established here. The receiver's two vtables match the constructor's assignments, rather than an invalid root/GUI adjustment or a random stale pointer.

An observer on a shared implementation must therefore classify the receiver before requiring the ordinary GUI interface or reading its ordinary container. Nonordinary receivers do not become ordinary ordering entries merely because they enter the same function. The successful-return observer must still reevaluate registered windows: an unregistered derived ancestor can change their traversal reachability. Explicit ordinary-window registrations and recognized ordinary receivers retain the strict container-owner check. No new derived-class dependency is required to skip an unsupported class; the classifier uses the already-checked ordinary GUI vtable.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x1547a75` | code | Referenced while examining candidate dependency bytes; complete role not established |
| `0x1547b1a` | code | Referenced while examining candidate dependency bytes; complete role not established |
| `0x1547d0b` | code | Referenced while examining candidate dependency bytes; complete role not established |
| `0x1547de9` | code | Referenced while examining candidate dependency bytes; complete role not established |
