# Accepted input and window activation

This study establishes where native mouse input is accepted, rather than merely hit-tested, and how native keyboard dispatch selects recipients, for the ordering process in [native interface ordering](../plans/window-ordering.md). It follows the complete manager update, mouse and keyboard dispatch, common element dispatch, button base input, background input, list input, text-box input, edit-box key handling, and hit-state update in the installed EU4 1.37.5 executable on 2026-11-14. The investigation is static; the separately reported keyboard test outcome is identified below and does not supply a runtime field trace. Addresses are RVAs in the baseline identified by [window-ordering.md](window-ordering.md).

## Event sources and the single-event seam

The manager update `0x14e9a70(manager, input_source, context, events)` iterates a linked list of events. At `0x14e9c72`, `event+0x50` selects the dispatcher:

| Value | Dispatcher | Evidence |
| --- | --- | --- |
| 0 | `0x14ea3d0(manager, event, input_source)` | Ordinary mouse; call at `0x14e9c94` |
| 1 | `0x14ea150(manager, event)` | Keyboard; call at `0x14e9c80` |
| 2 | Focus listener at `manager+0x210`, slot `+8` | Character input, passing `event+0x10` |
| 3 | `0x14ea630(manager, event)` | Another event source; call at `0x14e9ce5`; its exact product name is not established |

In particular, **`0x14ea630` is not the ordinary mouse dispatcher**. Its sampled-point and kind-2 branches cannot establish the behavior of mouse clicks. Earlier notes that use that function as generic mouse evidence must be read with this correction.

The manager calls its registered pre-listeners (`manager+0x188` vector, listener slot `+8`) before each dispatch, and post-listeners (slot `+0x10`) after it. It may then unlink and free that event, and proceeds to the next event without a drawing pass between them. After the first pass it sends remaining events through `0x14eb4b0`, then updates all registered GUI elements. An observer after the entire manager update is therefore too late to affect another event in the same batch.

An after-observer of `0x14ea3d0` runs after the current mouse candidate loop, focus changes and release cleanup, but before manager post-listeners, event deletion, and the next event. This establishes a useful single-event seam, not by itself a drawing-lock or reentrancy guarantee. A before-observer can establish per-event context; both observers forward three arguments. A callback may enter other input or mutate windows, so implementation must track nesting and native lifetimes rather than assuming every call is outermost.

## The ordinary mouse walk

`0x14ea3d0` reads `event+0x14` (3 press, 4 release, 5 double press), `+0x18` (1 left, 2 right, 4 wheel), and the position at `+0x20`.

For a press or release other than wheel, it compares the event position with the remembered scene position, scaled through `scene+0x4e0/+0x4e4`. Only when either difference exceeds two does it rebuild the hit list: clear `scene+0x58`, call `0x14dda50(scene, scene+0x298, scene+0x48, scene+0x58)`, then call `0x14defd0(scene)`. The position used in this hit test is the scene's current cursor point. A stationary click after reordering cannot be assumed to trigger this refresh.

It then clears the candidate array at `scene+0x30` using `0x14dff50`, and fills it from `scene+0x58` through `0x14deaf0(hit_list, 1000, candidate_array)`. That function walks the hits in reverse order, admits nodes with a nonnull GUI owner at `node+0x118`, and excludes owners that are hidden (`+0xc5 == 0`) or dead (`+0xc4 != 0`). Each output entry occupies 16 bytes: an auxiliary object's vtable and the GUI pointer. The function stops after 1000 candidates.

The mouse loop starts at `0x14ea538`. It stops as soon as the **whole 16-bit** handled mask at `event+0x54` is nonzero. Before dispatch it checks the GUI's shown byte, `manager+0x138`, and, when `manager+0x1a` is set, GUI slot `+0x18`. At `0x14ea57d` it calls GUI slot `+0x10` with `(gui, event, input_source)`. This actual call target, followed by the mask change, is stronger evidence of acceptance than a guessed rectangle or the first hit.

The function remembers the prior object at `manager+0x140`. On a press, if that object did not receive a dispatch and remains the same, it calls its slot `+0xc0` and clears the field. Releases also call `0x14eb740` and may finish a retained object at global `0x23496b0`. These calls are before the function's return. The detailed work of arbitrary virtual handlers and the drag-related tail target `0x15e68d0` is not claimed here.

## Observe acceptance through the common element dispatcher

The complete `0x1552800(gui, event, input_source)` is the common GUI slot `+0x10`. For ordinary mouse input it calls GUI slot `+0x48` and returns; only source 3 first updates animations. A table-reference search found 52 tables carrying this function. The following concrete supported classes were checked directly:

| Class | Vtable | Slot `+0x10` | Slot `+0x48` |
| --- | --- | --- | --- |
| Game button | `0x1d58218` | `0x1552800` | `0x15aefc0` |
| Window background | `0x1d954b0` | `0x1552800` | `0x1571570` |
| Window GUI subobject | `0x1d929d0` | `0x1552800` | `0x1546a50` |
| List GUI subobject | `0x1d90fd8` | `0x1552800` | `0x1531e00` |
| Text box | `0x1d8fd10` | `0x1552800` | `0x152ab70` |
| Edit box | `0x1d91358` | `0x1552800` | Not needed to establish the common dispatcher |

A static before/after observation at this common dispatcher can record:

1. Before the native handler, confirm it belongs to the current ordinary-mouse dispatch and has a zero handled mask. Capture source, kind, button, the input pointer, and the target's already-resolved logical window identity and native generation.
2. Run the original handler unchanged, exactly once.
3. After it returns, read only the still-live event's handled mask. A transition from zero to nonzero establishes that this dispatch took the event. Do not dereference the GUI pointer at this point merely because it was valid before the action.
4. Record activation for the innermost participating window captured before the call, then commit only after the mouse dispatcher finishes and the generation is still current.

Keep a stack keyed by the originating event for nested dispatcher calls. If an inner handler already recorded acceptance, the outer after-observer may see the same zero-to-nonzero mask transition; it must not record a second activation or promote an ancestor. Commit at the outermost completed mouse dispatch, never inside a parent's still-active candidate walk. A reentrant event that requires fresh order before that outer walk ends is a separate condition to detect and verify, not silently treat as safe.

Filter activation to mouse presses/double presses on the supported click buttons. Wheel, hover, source 1 shortcuts and source 3 animation input are not click activation. A button can modify the event kind (the framework's existing right-double-click correction does so), so capture the originating kind before calling it. A release can execute a click action or destroy the window; it must still complete its native handling once, and any earlier pending activation must be invalidated if the logical window closed. No additional call of a handler or signal is required.

This proposal uses actual native acceptance, not click-signal emission: a background can take a click without emitting a subscribed signal. Native acceptance is also not a claim that a game action succeeded. A disabled button or an input-catching background may consume input to prevent it from reaching the map. Attached button-area content maps to its existing window; it must not become an independently activated participant.

The table checks establish coverage for these classes, not every unrelated native class or an overridden slot in a future game build. Any registry that accepts additional classes must verify their dispatch method or observe the single mouse call site instead. Candidate-to-window resolution and lifecycle invalidation are requirements on the ordering registry; this study does not establish that the current framework already provides them.

## What the concrete handlers do

- `0x15aefc0` checks an attached object's slot `+0x270` and the button's `+0x289`, then delegates to `0x1592bd0` when allowed. It does not always reach the base handler.
- `0x1592bd0` checks source, hit state at `gui+0x8f`, supported buttons, state, and optional listeners. A press changes button state; a matching release can call the click signal. It adds `gui+0xa8` to the 16-bit handled mask at `0x1592fb1`, with another acceptance path at the end. A mask delta around the whole dispatcher therefore includes early exits as well as real consumption.
- `0x1571570`, the background handler, has its own left, right, double, wheel, drag and hover branches. It also reads `gui+0x8f` and adds its mask at `0x15717be` or the release branch. Observing only `0x15aefc0` misses it.
- `0x1531e00` accepts wheel input for a usable list scrollbar; it sets the mask to `0xffff` after changing the scrollbar. It is not a generic row-click action. Row buttons and row windows use their own GUI dispatches, which the common observer also sees.
- `0x152ab70` accepts wheel input for an attached, usable scrollbar, and otherwise falls through. A text box in front of a button must not be treated as click acceptance merely because it was a geometric hit.
- `0x1546a50` tests mouse source and **button field `+0x18 == 4`**, then calls `0x15a4610` with the wheel amount when enabled. This corrects an earlier description of that branch as a release. Separately, a shown window whose slot `+0x18` succeeds can add its mask. Whether a particular native window is eligible remains its own method's decision.

The handlers were read to their returns and their acceptance branches were identified. Arbitrary button actions, scrollbar implementations, sounds and drag callbacks were not recursively reconstructed. Their work is retained by observing the existing dispatcher rather than substituting its behavior.

## Keyboard dispatch, focus and shortcut acceptance

The complete keyboard dispatcher `0x14ea150(manager, event)` was read through its return and allocation-error tail. Unlike ordinary mouse dispatch, **both its priority-object call and its normal candidate calls go directly to GUI slot `+0x48`**. Neither passes through GUI slot `+0x10` or the shared dispatcher `0x1552800`. That shared function can forward a keyboard event if explicitly called with one, but an observer there does not cover the native manager's shortcut path. A return seam at `0x14ea150` is after its temporary candidate walk and before the manager's post-listeners and next event. It forwards two arguments; the dispatcher obtains the third argument for GUI handlers from `[[0x23494e8]+8]`.

### Earlier opportunities and gates

The order inside the dispatcher is:

1. Call `0x1560b60(event)` and return immediately if it succeeds. Its complete body requires a nonzero key at `event+0` and modifiers at `+8` other than -1, then accepts either modifiers 3 with the key matching the conversion of `0x63`, or `event+4 == 0x35`. It does not write the handled mask. The conversion helper `0x1a4ae3c`, also read completely, returns its input with ASCII uppercase converted to lowercase in the default branch, or uses the locale table through `0x1a4abc4`; a -1 result falls back to `0x63`. The locale helper's complete body was read, but its runtime-library dependencies were not reconstructed. This early exclusion must not be confused with an accepted GUI shortcut.
2. For Escape (`event+0 == 0x1b`) with modifiers zero, if `manager+0x230 != 0`, `manager+0x248 != 0`, and `manager+0x231 == 0`, clear the two bytes at `+0x230/+0x231`, set the whole handled word to `0xffff`, and return. Otherwise, if global byte `0x233ff4e` is nonzero, obtain the singleton through `0x14ef4a0` and set its `+0xb9` byte. This latter branch continues dispatch and does not itself mark the event handled. The singleton getter and constructor `0x14ee2d0` were read completely; the constructor initializes state and resolves a resource through virtual calls. The product identity of this state and the capture fields is not established here.
3. If `manager+0x230` and `manager+0x248` are nonzero, the source is 1 and the key is nonzero, walk the sentinel list at `manager+0x240` forward, calling each entry's listener slot `+0x18` with the event. This loop does not stop on a listener's handled result. It then sets `event+0x54 = 0xffff` and returns even when the list was empty. These capture conditions have priority over ordinary GUI shortcuts; reordering scene nodes does not change them.
4. If `manager+0x140` is nonnull and that object's shown byte `+0xc5` is nonzero, give it an opportunity through slot `+0x48`, provided the whole handled word is still zero. Afterward, any nonzero handled word returns immediately. This priority call does **not** use the normal candidate's `+0x8d`, dead-byte, manager `+0x138`, or conditional slot `+0x18` gates. Calling it a retained priority object describes the observed behavior; the pointer alone does not identify every product-level modal or focus owner.
5. Only then collect and dispatch the ordinary shortcut candidates. An already-handled event never reaches this walk.

The manager's registered pre-listeners run before this function, as described above. Their arbitrary effects and the handlers of unknown retained/capture objects are not reconstructed here. The dispatcher itself does not read the character-focus listener at `manager+0x210`: that listener receives source-2 character input through the separate manager branch. Therefore a focus pointer is not evidence that a source-1 key bypasses the keyboard list. Focused controls can still consume keys through their own slot `+0x48` or an earlier retained/listener opportunity.

### Exact candidate order and eligibility

The dispatcher gets `scene = *(manager+0xc0)`, then reads the node-pointer vector `[scene+0x70, scene+0x78)`. This is the entire prepared GUI-owner list, **not** the cursor-hit list. Its construction in drawing order is established in [window-input-refresh.md](window-input-refresh.md#the-prepared-lists).

It walks that vector from its last element to its first. For each node it reads `node+0x118` and appends the owner to a local GUI-pointer vector only when the owner is nonnull, shown (`+0xc5 != 0`), keyboard-eligible (`+0x8d != 0`), and not dead (`+0xc4 == 0`). The walk performs no geometric hit test, no key-binding match, no owner deduplication, and no 1000-candidate cutoff. Growth uses `0xcc220`, whose complete pointer-vector behavior is established in the input-refresh study.

It then walks the local vector from first to last: the effective order is reverse prepared drawing order. Before each call it rechecks shown state, skips every normal candidate when `manager+0x138 != 0`, and, when `manager+0x1a != 0`, requires GUI slot `+0x18` to return true. For the checked game-button and ordinary-window GUI classes, that slot is `0x15529a0`, whose complete body returns byte `gui+0x8c`. The collection-time dead and `+0x8d` checks are not repeated at the call site. The priority object is not removed from this ordinary list, so it may receive another opportunity if its first call left the event unhandled.

Every actual normal dispatch at `0x14ea366` calls `(gui, event, input_source)` through slot `+0x48`. After each candidate, the loop stops on **any nonzero value of the whole 16-bit mask at `event+0x54`**; the handler's return register is not the acceptance test. The candidate storage is freed before return. Mutating the scene does not retroactively change this local snapshot, and a keyboard callback must not cause an ordering refresh inside an outer active candidate walk.

### What makes a button accept Escape or Return

The complete game-button override `0x15aefc0` first asks its parent node at `button+0x228` for slot `+0x270`, when a parent exists, and refuses input if it returns false or `button+0x289` is nonzero. For ordinary window-container class `0x1d60238`, the parent method is `0x11483a0`, a read of `node+0x178`. Thus button input still depends on the parent's child-traversal gate after candidate collection.

The complete base handler `0x1592bd0` has additional independent gates and acceptance paths:

- A shown button with `event+0x56 != 0` first requires its own slot `+0x18`. An optional listener at `button+0x140` then receives slot `+8`; a nonzero handled word after that listener returns ends button handling.
- Slot `+0x258` supplies state; for game buttons it is `0x1148250`, a read of `button+0x124`. State 3 takes a separate path: a shown button with bit 0 of `button+0x12c` set consumes source-1 input by OR-ing its mask, without matching a shortcut or emitting the ordinary click signal. Keep this numeric state distinct from any inferred product label.
- In the ordinary source-1 branch, global GUI manager `[[0x23494f0]+0x18]` must be nonzero. The event key must be nonzero, its modifiers must not be -1, and at least one of button bytes `+0x160/+0x161` must be nonzero.
- A shortcut matches either `(button+0x104 key, button+0x10c modifiers)` or `(button+0x114 key, button+0x11c modifiers)`. A binding with key zero or modifiers -1 is rejected. Both key and modifiers must equal the event; this path does not implement -1 as a wildcard. Escape and Return are handled by the same comparison, not special close-window logic in the dispatcher.
- A match builds a click record containing the button, an all-ones 64-bit position, and `taken = 1` at record `+0x10`, then calls `0x1593260(button+0xc8, record)`. If `taken` remains nonzero after the listeners, it ORs `button+0xa8` into `event+0x54` at `0x1592fb1`. A zero element mask would not stop manager dispatch even after a click signal.
- The final catch path can also consume an unmatched event: a shown button whose slot `+0x18` succeeds and whose event `+0x56` is zero ORs its mask at `0x1592fdf`. Consequently, a handled shortcut event does not prove a matching button action ran.

The click dispatcher `0x1593260` was read completely. With a direct listener it calls slots `+0x18`, `+0x10` with the button, and `+8` with the click record, subject to its intervening null checks. Otherwise it makes those three passes over the listener collection through `0x15940e0`. That helper snapshots listener pointers and checks their continued membership before calling; the three wrappers `0x1594290`, `0x1594240`, and `0x15941f0` forward those respective slots and return false to continue iteration. The click record's final `taken` byte, not a guessed close-button role, controls the ordinary shortcut mask write. Arbitrary listener action bodies remain native behavior.

An ordinary window can consume keyboard input without executing a shortcut: the complete `0x1546a50` has a source-independent final branch which ORs its element mask whenever it is shown and slot `+0x18` succeeds. Its wheel branch is separate. This is another reason to preserve earlier native acceptance rather than forcing a chosen extension button to run.

### Focused text editing

The complete edit-box handler `0x1534190` refuses all input when `edit_box+0x261` is nonzero. For source 1 it requires shown state and focus byte `+0x260`, then calls the inner editor's slot `+0xa8` at `edit_box+0xc8`, implemented by `0x15366c0`. A true result causes the outer handler to OR its mask into the event. It then refreshes displayed text through `0x1534bb0`; that complete body was also read and has no direct event-mask write.

The complete inner key handler recognizes navigation/editing keys and copy/paste/cut combinations; unmatched keys are offered to listeners through slot `+0x48`, and their boolean results are accumulated. Plain Return is a recognized branch which can insert a newline and/or notify listeners and returns handled; Escape has no built-in branch here and can be accepted by those fallback listeners. Editing-operation internals remain covered by [text-input.md](text-input.md), not reimplemented by ordering. The focus setter `0x1535250`, read completely, sets `+0x260` and stores `edit_box+0x1d0` in `manager+0x210`; it does not itself assign `manager+0x140`. Its listener effects do not establish that all focused editors have priority independently of scene order.

### Naming an accepted ordinary window for diagnosis

Ordinary window GUI class `0x1d929d0` uses `0x154ad60` at its name slot `+0xa0`, unlike the common button/text getter. The complete body first selects the native string at GUI `+0xa28`. If its first character is nonzero, it returns that string; otherwise it tail-calls slot `+0x40` on the definition pointer at GUI `+0xb0`. Its initial bytes are `48 8d 81 28 0a 00 00 48`. The definition's actual virtual getter must be resolved separately; the window getter alone does not establish every definition layout. The temporary recipient probe reads the instance string and only recognizes a definition getter whose complete bytes are a single `lea rax, [rcx+displacement]` followed by `ret`; it does not execute an arbitrary definition getter. This diagnostic is not a new ordering dependency or a public inspection capability.

### Consequence for ordering and test interpretation

Refreshing the GUI-owner list after a scene reorder changes ordinary shortcut precedence to reverse the new drawing order, while leaving earlier capture, retained-object, focus and handler decisions intact. It does not promise that the foremost registered extension always receives Escape or Return. Keyboard dispatch is not mouse-click activation, and its direct `+0x48` calls need their own event-depth seam if keyboard callbacks can request ordering changes.

The isolated test coordinator reported that a France `flavor_fra.206` opening event was present during the first unsuccessful Escape attempt. After answering that event through the question capability, two Escape presses and one Return passed the `qa-window-ordering` assertions and produced the final `Ordering keyboard passed` pop-up. That before/after observation shows the original event affected the test conditions; it does not identify which retained pointer, capture byte, listener, or candidate consumed the first input. No runtime field trace or game run was performed by this static investigation.

## Hit-state freshness is more than list order

`0x14defd0(scene)` clears and rebuilds `scene+0x30` from `scene+0x58` via the same candidate builder. It then iterates candidates from the last to the first and calls each GUI's slot `+8`, passing that candidate's index.

The common slot `+8`, `0x15528d0(gui, rank)`, first asks GUI slot 0. If allowed, it compares the rank with the signed setting at `gui+0x60`: -1 always sets `gui+0x8f`; otherwise it sets the byte according to `rank <= setting`. For settings greater than -2 it also writes the GUI pointer to its manager's `+0x130`. The exact product meaning of all negative settings is not named here.

Buttons, backgrounds and lists read `+0x8f`. Consequently, after tree reordering, refreshing only the draw list and shortcut-owner list is insufficient: the cursor hit list and its derived hit-state updates also need to reflect the new order before another mouse event. `0x14defd0` performs no drawing in its own body, but reaches each candidate's virtual slot `+8`; known common-method coverage and synchronization still require checked dependencies. It must not run while the mouse dispatcher is traversing the same `scene+0x30` candidate array.

All 52 tables found holding the common input dispatcher were also checked for slots 0 and `+8`. Fifty-one use `0x15528d0`; `0x1d9f0b8` uses `0x15f19e0`, whose complete body performs the same rank update but first sets `gui+0x1901` to one. No call of a click or hover signal occurs in either method. The latter flag's product meaning is not established. Eligibility slot zero has only five implementations in this inventory: `0x8f440` returns true; `0x10b3b40`, `0x1570e60`, `0xfaabd0`, and `0x15f2850` return whether bytes `+0x289`, `+0x1e8`, `+0x1e2`, and `+0x200`, respectively, are zero. All five were read to their returns and have no other effects. This closes the virtual-call effects for this concrete inventory; it is not a blanket promise for an unrecognized GUI class.

Candidate clearing and growth do not call the GUI target. `0x14dff50` calls the wrappers' destructors with deletion disabled; the actual wrapper destructor `0x14e0d10` only restores its own vtable in this case. The temporary 24-byte wrapper's `0x15e4c20` has the same behavior. `0x14e06e0` grows storage by about one half using `0x950b0`, constructs 16-byte wrappers by copying the GUI pointer and using `0x1d8cff8`, then calls `0x14e0900` to retire the old wrappers and free their backing storage. The latter follows the large-allocation alignment convention before operator delete. Both functions were read through return and terminal allocation-error branches. The candidate array remains native-owned.

### Cursor point and derived state

The correct point for a faithful refresh is `scene+0x48`, not the just-handled event's `+0x20`. At the start of `0x14e9a70`, input-source slot `+0xb8` supplies the current pointer position; the manager scales it through `scene+0x4d8/+0x4dc`, truncates to integers, converts back to floats, and stores that point at `scene+0x48`. The mouse dispatcher compares the event position against the remembered hit position to decide whether to refresh, but even that refresh passes `scene+0x48` to the hit collector and copies it to `scene+0x50`. A queued event may carry a different position; replacing the native point with it would change existing semantics.

`manager+0x130` is the last qualifying GUI written during the reverse candidate iteration, normally the foremost qualifying hit. This field is not the accepted-click record. The manager clears it at `0x14e9a9d` before its initial hit-state update; `0x14defd0` itself does not clear it. The in-game input function tests it for null at `0x8310ba` and passes that condition to map input `0x10a87c0`. Release cleanup `0x14eb740` passes it to a retained object's slot `+0x158`, and drawing/tooltip paths also read it. It is observable native state, not disposable scratch.

An extra mid-batch refresh must therefore specify what happens when no hit remains after a hide or destruction; merely calling `0x14defd0` can retain the previous `manager+0x130`. Similarly, the ordinary manager's per-element update calls slot `+0x40`, copies the current `+0x8f` to `+0xaa`, and clears `+0x8f` at `0x14e9e87`. Rebuilding candidates does not clear old hit-state bytes on objects that cease to be candidates. The refresh itself emits no hover signal for the inventoried classes, but changing or retaining these fields affects the later ordinary hover update. Clear only known-live state under an explicit lifecycle contract; do not revisit retired GUI pointers from an old array. These invalidation cases still require runtime verification alongside [window-input-refresh.md](window-input-refresh.md).

### Application frame and synchronization

The direct-caller search found one caller of the ordinary mouse dispatcher (`0x14e9c94`) and one of the manager update (`0x10b4f09`). The latter is in `0x10b4f00`, a small wrapper that calls the update and then clears `manager+0x518`; it is GUI manager vtable `0x1d58108` slot `+0xd0`. The complete in-game input function `0x830fd0` calls this slot at `0x8310a2` through `idler+0x58`, after earlier modal input opportunities and before map and camera input. Its one direct caller is `0x815028` in map-manager frame `0x8137c0`. The application frame `0x14c25a0` calls the current idler's frame slot `+0x20` at `0x14c2815`. This matches [architecture.md](architecture.md)'s application and idler frame structure.

The source's `drawing.rs` observes the application frame before and after it. `MainSide::frame_begins` acquires the existing presentation lock and retains it through a nested frame that did not acquire it separately. Daily update releases that lock, and returning from the day reacquires it; a nested application frame in a day also takes it. No second ordering-specific lock is justified by this chain. Native event callbacks are indirect and may enter other work, so the caller scan does not prove absence of reentrant input; local candidate-walk depth must still be tracked, and actual lock ownership checked at a mutation point.

There is a material mode distinction: `speed::installed` omits all `drawing::points` when the drawing-thread speed-up is left to the game, including the application-frame observer. In that mode `drawing::main_holds()` can correctly be false while the main thread alone owns all GUI work. A universal `main_holds()` requirement would incorrectly disable ordering in the native drawing mode. Conversely, main-thread identity alone is insufficient while the drawing thread runs. The drawing module's existing state must express whether there is no concurrent drawing thread or the current main-thread call holds its presentation lock; this is a condition to verify, not a reason to introduce another mutex.

The native mouse loop's return provides the local traversal exclusion. The framework must additionally exclude the drawing thread, guard reentrant input, preserve the existing modal/focus decisions, and establish the prepared-list rebuilding and invalidation contract. A process mutex or an after-observer alone does not prove those conditions.

## Candidate dependency points

Bytes below are exact baseline prefixes, not assertions that hooks have been installed. The hook implementation must retain complete instructions and the repository's ordinary point validation. `argc` is checked by the existing argument-reader tool.

| RVA | Purpose / arguments | Bytes |
| --- | --- | --- |
| `0x14ea3d0` | Single mouse event; 3 arguments | `48 89 5c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 20` |
| `0x1552800` | Common element dispatch; 3 arguments | `48 89 74 24 18 48 89 7c 24 20 41 56 48 83 ec 20 83 7a 50 03` |
| `0x14defd0` | Rebuild and update hit candidates; 1 argument | `48 89 5c 24 10 56 48 83 ec 20 48 8d 71 30 48 8b d9` |
| `0x14deaf0` | Build reversed candidate array; 3 arguments | `48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 56 41 57` |
| `0x14ea56d` | Actual mouse call site forwards GUI, event and input source | `4a 8b 4c 3b 08 48 8b 01 4c 8b 44 24 70 49 8b d6 ff 50 10` |
| `0x1592fb1` | Element mask and event handled field | `0f b7 87 a8 00 00 00 66 09 46 54` |
| `0x14ea150` | Single keyboard event; 2 arguments | `48 89 5c 24 08 48 89 6c 24 10 56 57 41 54 41 56 41 57 48 83 ec 40` |
| `0x14ea198` | Escape capture checks, state clear and handled write | `80 be 30 02 00 00 00 74 2a 48 83 be 48 02 00 00 00 74 20 80 be 31 02 00 00 00 75 17 66 c7 86 30 02 00 00 00 00 b8 ff ff 00 00 66 89 47 54` |
| `0x14ea1e0` | Capture mode and listener-list selection | `80 be 30 02 00 00 00 74 4d 48 83 be 48 02 00 00 00 74 43 83 7f 50 01 75 3d 83 3f 00 74 38 48 8b b6 40 02 00 00` |
| `0x14ea236` | Retained shown object at manager `+0x140`, direct `+0x48` call and handled check | `48 8b 8e 40 01 00 00 48 85 c9 74 20 80 b9 c5 00 00 00 00 74 17 66 83 7f 54 00 0f 85 56 01 00 00 48 8b 01 4d 8b c4 48 8b d7 ff 50 48 66 83 7f 54 00` |
| `0x14ea284` | GUI-owner vector end/start and initial reverse index | `49 8b 46 78 49 2b 46 70 48 c1 f8 03 83 e8 01 48 63 d8` |
| `0x14ea2a0` | Node owner and shown/keyboard/dead candidate gates | `49 8b 46 70 48 8b 0c d8 48 8b 81 18 01 00 00 48 89 84 24 80 00 00 00 48 85 c0 74 4a 80 b8 c5 00 00 00 00 74 41 80 b8 8d 00 00 00 00 74 38 80 b8 c4 00 00 00 00 75 2f` |
| `0x14ea335` | Dispatch-time gates, direct `+0x48` call, and whole-word stop | `48 8b 0b 80 b9 c5 00 00 00 00 74 28 80 be 38 01 00 00 00 75 1f 80 7e 1a 00 74 0a 48 8b 01 ff 50 18 84 c0 74 0f 48 8b 0b 48 8b 01 4d 8b c4 48 8b d7 ff 50 48 48 83 c3 08 66 83 7f 54 00 74 bc` |
| `0x15529a0` | Checked button/window GUI eligibility slot `+0x18` reads `+0x8c` | `0f b6 81 8c 00 00 00 c3` |
| `0x1592f02` | Button shortcut global manager gate `+0x18` | `48 8b 05 e7 65 db 00 80 78 18 00` |
| `0x1592f3f` | Exact comparison of both button key/modifier pairs | `8b 97 04 01 00 00 85 d2 74 16 44 8b 87 0c 01 00 00 41 83 f8 ff 74 09 3b d1 75 05 44 3b c0 74 20 44 8b 87 14 01 00 00 45 85 c0 74 51 8b 97 1c 01 00 00 83 fa ff 74 46 44 3b c1 75 41 3b d0 75 3d` |
| `0x1592fa3` | Click record's taken byte gates the shortcut mask write | `80 7c 24 30 00 74 12 eb 05 83 f9 03 75 0b 0f b7 87 a8 00 00 00 66 09 46 54` |

## Subsequent recipient verification

The [native-drawing recipient run](../prototype-log/2026-11-14-window-ordering-startup-screen.md) identified `startup_screen` / `close_button` as the initial Escape consumer. Answering the country event left this independent original window open. After closing it and reloading the fixture, exactly two Escape requests and one Return passed; copied identities tied them to `a_last`, reopened `z_first`, and the fixture popup respectively. This resolves the earlier recipient uncertainty without suppressing original input or changing ordering to force the fixture ahead of it. It does not establish pointer behavior or same-batch readiness.

## Remaining verification obligations

Static reading now distinguishes the real mouse path, actual accepted dispatches, the separate direct keyboard path, per-event return seams, and the extra cursor-hit state that must be refreshed. It does not prove a running hook's target-to-window mapping, lifetime retirement, drawing lock ownership, nested-dispatch handling, or complete prepared-list refresh. Isolated game checks must include a stationary click followed by another input in one batch; a click action that closes its own window; a background that consumes without a signal; a text box that falls through; nested rows; and both drawing modes. The successful Escape/Return course and the original startup-screen recipient are reported above. That recipient trace does not establish the product meanings or arbitrary virtual implementations of every capture/priority object.
