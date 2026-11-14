# Window visibility transitions and lifetime

This note investigates the native display and retirement paths needed by the private window-ordering process in [the ordering plan](../plans/window-ordering.md). It distinguishes a show request, a local open flag, reachability through a visible parent, and the lifetime of the native instance. It complements [window-ordering.md](window-ordering.md); it does not establish mouse activation or a safe order-commit/input-refresh point.

Date: 2026-11-14. Baseline: EU4 1.37.5, SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`. RVAs below were looked up with `cargo xtask what` before reading the installed executable with `cargo xtask disasm`. No game was started, attached or modified. The complete bodies of the four display methods, the element visitor, its show/hide callbacks, the enable propagation, Kill, the deleting destructor and the destructor body were read. Generic container destructors and audio routines were not expanded: their internal behavior is not used as a visibility contract here.

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

GUI Kill `0x1547910` is already documented in [notice-window.md](notice-window.md#32-root-window-kill-root-vtbl0x130--0x1547520-verified) as the `root+0x30` adjustment and virtual root Kill dispatch. Existing `core/src/ui.rs` observes root Kill at entry, and has the deleting destructor as a backstop. Those entry points can retire ordering identities while their native memory still exists. Do not wait for a later frame to inspect retired memory.

The ordinary deleting destructor `0x1546510(root, flags)` takes two arguments. It calls `0x1546730(root)` and frees the 0xa80-byte allocation if `flags & 1`. The destructor body first reinstalls the ordinary root/GUI vtables, passes the scene node to `0x14db6a0`, then destroys the typed containers, name storage and internal lists before ending in the base teardown. Destruction is distinct from Kill, and native pointers must be invalidated before it. The earlier lifetime study also found a derived-class destructor which calls the body directly; an observer only on the ordinary deleting destructor is not a proof of coverage of arbitrary derived classes.

A construction address cannot identify a new logical appearance. As recorded in the plan, the framework rebuilds still-wanted windows when the definition generation changes. The replacement native instance must attach to the retained logical identity, while a real close/open sequence remains a new activation. The ordering process should explicitly retire its own destruction paths too: existing `windows::killed` uses a nonblocking lock and may not observe a callback reentering framework-owned reconciliation.

## Candidate dependency bytes

These are verified against the installed binary, not added to the dependency registry by this research. Existing keep points for GUI show/hide and existing observers for Kill/destruction should be reused or changed in place, not overlapped with independent detours.

| RVA | Bytes | Use |
| --- | --- | --- |
| `0x1547d90` | `40 53 48 83 ec 30 80 b9 c5 00 00 00 00 48 8b d9` | GUI show entry, 1 argument |
| `0x1547b00` | `48 89 5c 24 10 57 48 83 ec 30 48 8b f9 48 8b 89 00 0a 00 00` | GUI hide entry, 1 argument |
| `0x1547cc0` | `40 53 48 83 ec 30 80 b9 db 00 00 00 00 48 8b d9` | Root show entry, 1 argument |
| `0x1547a50` | `40 53 48 83 ec 20 48 8b d9 c6 81 f5 00 00 00 00` | Root hide entry, 1 argument |
| `0x1547520` | `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 c6 81 f4 00 00 00 01` | Kill entry and dead byte, 1 argument |
| `0x1546510` | `48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8 0c 02 00 00` | Deleting destructor entry, 2 arguments |
| `0x1547de9` | `48 8b 8b f0 04 00 00 c6 83 c5 00 00 00 01` | Background and local GUI open byte |
| `0x1547b1a` | `c6 87 c5 00 00 00 00` | Local GUI open byte cleared |
| `0x1547d0b` | `48 8b 83 30 0a 00 00 c6 80 78 01 00 00 01` | Root node pointer and child-traversal gate set |
| `0x1547a75` | `48 8b 83 30 0a 00 00 c6 80 78 01 00 00 00` | Root node pointer and child-traversal gate cleared |
| `0x11483a0` | `0f b6 81 78 01 00 00 c3` | Window-container child-traversal gate read |

## Coverage and remaining limits

Statically covered by the shared ordinary-window methods: the army host and its nested picker constructed through the ordinary window prototype, framework extension windows, framework pop-ups, and other registered original windows whose root/GUI/container classes and ancestor traversal contract have been checked. Their placement at top level versus as a nested window does not change these method bodies.

Not proved: that every original panel has these classes, that every possible game path changes visibility only through these four methods, or that arbitrary ancestor node classes use the same child-traversal field. The base scene-node `+0x270` is `0x8f440`, whereas the window-container method is `0x11483a0`; do not read `+0x178` on every ancestor without validating its class. Direct visibility stores and derived-class overrides outside the examined methods have not been exhaustively searched. A broad all-vanilla transition claim would exceed this evidence.

No hook was installed and no transition sequence was executed. Runtime checks still need to cover repeated show, same-frame hide/show, parent root versus GUI hiding, a rejected show request, Kill while open, and instance replacement with retained logical intent. Pointer activation and prepared-input freshness are separate evidence obligations; this note resolves neither.
