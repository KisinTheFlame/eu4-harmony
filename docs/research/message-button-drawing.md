# Native message buttons: display and drawing

This appendix supports the investigation of missing native action buttons in the game's `DefaultPopup` message window. It follows the button element through show and hide, its cached text texture, its mesh, and the GUI draw lists. The framework uses these paths for native interface elements and shares their drawing state with its drawing thread; this note helps distinguish an element that was hidden from a node that was not drawn. It establishes neither a cause for the reported missing buttons nor a repair.

Read 2026-11-15 against the EU4 1.37.5 baseline. All executable addresses below are RVAs. [V] marks disassembly or installed vanilla resource definitions checked during this investigation. Field and function names in this note describe their observed roles, rather than recovered source symbols. No runtime samples are recorded here. See [message-pop-up-actions.md](message-pop-up-actions.md) for the message factory and action selection, [gui-drawing.md](gui-drawing.md) for the wider drawing path, and [font-textures.md](font-textures.md) for text texture generation.

## The vanilla button resource

In `interface/eu3dialog.gui`, `DefaultPopup` defines `DeclineButton`, `AgreeButton`, and `CenterOK`, all with `quadTextureSprite="largefloaterbutton"` and `buttonFont="vic_18"` [V]. Their positions are respectively `(77, 270)`, `(274, 270)`, and `(175, 270)`. The separate `Settings` element uses `MessageSettingsButton`.

In `interface/eu3dialog.gfx`, `largefloaterbutton` is a `textSpriteType` whose texture file is `gfx//interface//button_type_1.tga` [V]. A text sprite can cache the button background and its label in one texture. The disappearance of both therefore does not, by itself, prove that the element's hide method ran. A mod may replace the definition; the live node's class must be checked before interpreting its fields using the layout below.

## Construction and the first show

The native button class is `0x1d58218`. Its constructor chain is `0x10b53e0` -> `0x15ac670` -> `0x1592840` -> `0x1552750` [V]. The bottom constructor clears the eight bytes at element `+0xc0` at `0x15527eb`; this includes the death byte at `+0xc4` and the visibility byte at `+0xc5`. They initially hold zero. This describes the base construction stage, not the final state of an element returned by the whole window-building process.

The button's main graphics node is at button `+0x290`; an optional additional node is at `+0x188`. For the text texture node class `0x1d933b0`, the constructor chain is `0x15506e0` -> `0x1148180` -> `0x1561fd0` [V]. The bottom constructor clears node `+0x120` at `0x1562094`, so the node starts inactive. The two returning constructors do not overwrite that active byte. The text texture constructor installs its final class at `0x155072c` and initially has no cached texture or populated mesh.

The direct window-building path for a button has been narrowed to [V]:

1. Window constructor `0x1543f80` calls the button factory wrapper `0x155dad0` at `0x1545395`.
2. The wrapper invokes the definition factory's slot `+0x70` at `0x155db1a`. For the native button factory, this is `0x10b59c0`, which allocates the button and calls `0x10b53e0` at `0x10b5bdb`.
3. At `0x155db2d`, the wrapper registers the returned element through the GUI manager's slot `+0x98`, `0x14eb360`. Registration adds it to manager `+0x110` and, if its slot `+0x238` requests updates, to manager `+0x170`. That complete registration function does not call show.
4. Back in the window constructor, `0x15453aa` calls `0x1546e70`, the element bounds check. The button is then stored in the window's `+0x5b8` element array, paired with names at `+0x5d0`.

Neither the complete native button constructor `0x10b53e0` nor its immediate factory `0x10b59c0` calls show after returning from construction [V]. The slot `+0xe8` invoked when attaching a button, `0x15ae9c0`, saves its graphics parent at button `+0x228`, detaches its graphics nodes, and attaches them to that parent. It does not activate them.

**The callback dialog's window show propagates to the buttons.** Base initialization `0x10a06c0` stores the new root and calls root GUI slot `+0x88` at `0x10a0779`, before the message-specific filler. For the window GUI class `0x1d929d0`, this is `0x1547d90`. It sets GUI `+0xc5` (root `+0xf5`) and passes a stack operation with vtable `0x1d92c28` to `0x154e500`. That complete function traverses the window's element groups. Its loop at `0x154e580` reads each pointer from root `+0x5b8` and calls the operation's first virtual method with the element as its second argument. The operation is `0x15f5e60`, whose complete leaf body moves that element to `rcx` and tail-jumps to its slot `+0x88`: the button show method below. The other selected element groups and subwindows are visited by the same traversal [V].

The separate slot `+0x88` call at `0x1544dea` is in the constructor's `root+0xa18` named-window loop and is followed by hide at `0x1544e49`. That call is not needed to explain the action buttons' initial visibility. The demonstrated path is the later dialog base show, which shows the root's button group before the message filler hides the unselected buttons.

## Show, hide, and text are separate operations

For class `0x1d58218`, show is virtual slot `+0x88`, `0x15aeb30`, and hide is slot `+0x90`, `0x15aead0` [V].

- Show returns without changing visibility when button `+0x100` is nonzero. Otherwise it sets element `+0xc5` to one and calls node slot `+0x60` on the main node and the optional additional node. It also handles the optional object at button `+0x198`.
- Hide clears element `+0xc5`, calls node slot `+0x68` on those nodes, and handles the optional object at `+0x198`.
- For a text texture node, slot `+0x70`, `0x1148230`, simply returns the active byte at node `+0x120`.
- Its activation method, slot `+0x60`, `0x1550b30`, records the previous active state, writes active = one, and rebuilds a deferred texture only when the node was inactive and its pending flag at `+0x2d8` is set.
- Its deactivation method, slot `+0x68`, `0x1550c90`, writes active = zero, releases its background resource reference at `+0x208`, and releases and clears a cached texture at `+0x278` if one exists. Releasing that texture sets `+0x2d8` for later rebuilding.

Text setting is a different virtual method: button slot `+0x290`, `0x15adfa0` [V]. It saves the text at button `+0x200` and the font pointer at `+0x220`; unchanged text can return early. For changed text it calls node slots `+0x1a8` and `+0x1c0` on the main and optional additional nodes. For class `0x1d933b0`, these are `0x1550d10` and the empty method `0x955a0`. Text setting does not write the element's visible byte or activate the node.

The message action helpers `0x10a29e0` and `0x10a2a80` find `AgreeButton` and `DeclineButton` respectively and call that text setter; neither helper explicitly shows the button [V]. The single-button branch in the message filler sets `CenterOK`'s text after hiding the side buttons. Setting the chosen label alone is not proof that the selected button becomes visible at that point.

## Deferred texture generation

For the text texture node class, these fields have been checked [V]:

| Offset | Observed role |
| --- | --- |
| `+0x208` | Background resource reference |
| `+0x210` | Background resource dimensions |
| `+0x230` | Text string |
| `+0x250` | Font name string |
| `+0x270` | Graphics context pointer |
| `+0x278` | Cached texture |
| `+0x280`, `+0x284` | Width and height in GUI units |
| `+0x288`, `+0x28c` | Texture width and height |
| `+0x294` | Text format |
| `+0x298` | Mesh |
| `+0x2d8` | Pending texture rebuild flag used on activation |
| `+0x2db` | Layout dirty flag read by drawing |

Node text setter `0x1550d10` forwards text, the font name, retained dimensions and borders, and the requested format to slot `+0x1b0`, `0x1550e90` [V]. The latter first records the parameters and the pending flag. If the active test is false, it retains the parameters and resource information and returns without generating the finished texture. If active, it loads the background resource, asks the font to generate the composed texture through `0x1596660`, saves that texture at `+0x278`, records its dimensions, and builds the mesh through `0x1552040`.

A nonzero texture pointer is evidence that a texture is retained, not that its pixels contain the intended background and label. The normal missing-texture fallback does not inspect the content of an already nonzero texture.

## From a ready node to a mesh submission

The text texture node's draw method is slot `+0x58`, `0x1550950` [V].

1. If node `+0x2db` is set, it updates layout through slot `+0x90` and clears the flag. It also obtains the node's animation time again after that update.
2. If cached texture `+0x278` is zero, it looks up the font and calls node slot `+0x1a8` to attempt rebuilding. At `0x15509ee`, this fallback passes format zero. This is the left-format fallback described in the earlier text texture research; its presence does not establish that it caused this report.
3. It selects drawing resources and calls `0x1551840`, which prepares constants, binds the retained texture and submits the prepared mesh.

If the texture is still zero on entry to `0x1551840`, that function updates the GUI dimensions, rebuilds the mesh with `0x1552040`, and returns without submitting a draw [V]. With a nonzero texture it computes the transform, chooses the texture frame, copies the node's drawing color into the constants, binds the texture, and calls `0x15bd180` with node `+0x298`.

Mesh submission `0x15bd180` returns immediately if the mesh's first `u32` is zero [V]. Otherwise it binds the vertex data and topology and issues either an indexed draw or a draw without indices, according to the mesh's buffers. A retained texture and an active node therefore do not alone prove a mesh submission, and a submission does not alone establish the pixels finally visible on screen.

## Animation time is not alpha

**Node `+0x1b8` is the animation-time value supplied to this drawing path. It must not be interpreted as opacity.** The getter `0x1550af0`, node virtual slot `+0x98`, returns that float [V]. The checked argument flow is:

- `0x14dd350` calls the getter at `0x14dd4c1` and supplies its result as the node draw method's fifth argument at `0x14dd4e2`.
- `0x1550950` keeps that value in `xmm6`, or reads it again after updating dirty layout, and forwards it as the seventh argument to `0x1551840` at `0x1550aaa`.
- `0x1551840` copies it into its constant block at offset `+0x64`, at `0x1551a66` and `0x1551a6e`. The preceding float, offset `+0x60`, receives the graphics context's time value.

The installed vanilla `gfx/FX/buttonstate_rendertarget.shader` declares precisely that layout [V]:

| Constant offset | Shader declaration |
| --- | --- |
| `+0x00` | `float4x4 WorldViewProjectionMatrix` |
| `+0x40` | `float4 Color` |
| `+0x50` | `float2 Offset` |
| `+0x58` | `float2 NextOffset` |
| `+0x60` | `float Time` |
| `+0x64` | `float AnimationTime` |

That shader's pixel code multiplies the sampled texture by `Color` and does not use `AnimationTime`. At `0x15519fe`, the native draw preparation copies the four floats at node `+0x48` into the block's `Color`; its alpha component is therefore node `+0x54`. The selected live drawing resources and any mod replacement must still be checked before attributing an on-screen result to a particular shader. A zero at `+0x1b8` is not evidence of transparency.

## Graphics tree links

The following links belong to the graphics node, not to the button element [V]:

| Offset | Role |
| --- | --- |
| `+0xf0` | Previous sibling |
| `+0xf8` | Next sibling |
| `+0x100` | First child |
| `+0x108` | Last child |
| `+0x110` | Parent |

These meanings are established by the complete list operations, not by pointer resemblance. Node slot `+0xc8`, `0x1562420`, detaches the node and appends it to a parent's children: the old last child becomes node `+0xf0`, the old child's `+0xf8` points to the new node, and the parent is written to node `+0x110`. Slot `+0xd0`, `0x1562480`, prepends instead. Slot `+0xe0`, `0x1562390`, repairs both sibling links, uses `+0x110` to update the parent's first and last child, and clears the departing node's sibling and parent pointers.

In particular, **`+0xf0` is not the parent pointer**. Any earlier interpretation of that field as a parent must be corrected before using a captured tree to reason about visibility or placement.

## Draw-list construction and drawing

The graphics context is the pointer at global `0x23494b8` [V]. Its roots and lists are embedded objects, not pointers stored at those offsets:

| Graphics offset | Role |
| --- | --- |
| `+0x110` | First GUI tree root |
| `+0x298` | First GUI draw-list vector header |
| `+0x2b0` | Second GUI tree root |
| `+0x438` | Second GUI draw-list vector header |

Each list is a vector of node pointers: begin at header `+0`, end at `+8`, capacity at `+0x10`, with count `(end - begin) / 8`. Read the header at `graphics + offset` directly; do not dereference its first entry as if it were another vector header.

First GUI pass `0x14dd010` resets its list's end to begin at `0x14dd073` / `0x14dd076`; the second pass `0x14dd540` does the equivalent at `0x14dd566` / `0x14dd56b` [V]. They call list drawing `0x14dd350` at `0x14dd2b8` and `0x14dd73d`, with returns at `0x14dd2bd` and `0x14dd742`.

Tree traversal `0x14dd7a0` checks node slot `+0x70` to decide whether to append that node [V]. Slot `+0x270` separately decides whether to traverse its children. It combines local position at `+0x9c` / `+0xa0` with the supplied transform, writes the world transform at `+0xac`, appends the eligible node to the vector, recurses into `+0x100`, and follows siblings through `+0xf8`. It also propagates clipping information. Calling this traversal for inspection would mutate the transforms and list; read the prepared state instead.

List drawing `0x14dd350` reads each pointer, then checks the clipping flag at node `+0x130` [V]. If enabled, its rectangle at `+0x128` is four signed 16-bit values. An empty rectangle or an empty intersection with the viewport skips the node. Otherwise it obtains the node's layer through slot `+0xb8`, obtains its animation time through slot `+0x98`, and calls draw slot `+0x58` with graphics, renderer, world transform, time, and layer. This loop does not read the button element's visible byte directly.

A read from another process may overlap list rebuilding. One sample with a node absent from a vector is therefore not enough to establish sustained absence. Element visibility, node activity, membership in a prepared list, clipping, texture retention, mesh data, drawing color and the final picture are separate observations.

## Existing framework boundaries

The current `core/src/drawing/crossings.rs` already declares the native message factory `0x8c1430` and window creation `0x10a03f0` as crossings. It also redirects stateful renderer-table entries. Those existing boundaries must be accounted for before proposing additional protection. This note does not demonstrate an unprotected access or establish a race.

The tooltip filtering in `core/src/tooltips/drawing.rs` removes the exact retained tooltip node pointer from a temporary copy of the native list. It does not remove all instances of class `0x1d933b0`. Sharing the text texture node class with a tooltip is therefore not evidence that message buttons are filtered out.

## Address names

The following descriptive names record the functions and classes used in this appendix. They are suitable for the address index; the instruction sites above identify the individual field and argument checks.

| RVA | Descriptive name |
| --- | --- |
| `0x955a0` | Empty virtual method |
| `0x10a03f0` | Create a native window (existing crossing) |
| `0x10a29e0` | Set the AgreeButton label |
| `0x10a2a80` | Set the DeclineButton label |
| `0x10b53e0` | Construct a native button |
| `0x10b59c0` | Allocate a native button from its definition |
| `0x1148180` | Construct the sprite node base |
| `0x1148230` | Read graphics node active state |
| `0x14dd010` | Prepare and draw the first GUI list |
| `0x14dd350` | Draw a GUI node vector |
| `0x14dd540` | Prepare and draw the second GUI list |
| `0x14dd7a0` | Traverse GUI nodes into a draw list |
| `0x14eb360` | Register a GUI element with the manager |
| `0x1543f80` | Construct a window and its child elements |
| `0x1546e70` | Check a child element's window bounds |
| `0x1547d90` | Show a window through its GUI interface |
| `0x154e500` | Apply a window operation to its element groups |
| `0x15f5e60` | Show the supplied GUI element |
| `0x1d92c28` | Show-element operation virtual table |
| `0x15506e0` | Construct a text texture node |
| `0x1550950` | Draw a text texture node |
| `0x1550af0` | Read text texture node animation time |
| `0x1550b30` | Activate a text texture node |
| `0x1550c90` | Deactivate a text texture node |
| `0x1550d10` | Set text texture node text and font |
| `0x1550e90` | Store text parameters and build an active node's texture |
| `0x1551840` | Prepare constants and draw the cached text mesh |
| `0x1552040` | Build the text texture mesh |
| `0x1552750` | Construct the GUI element base |
| `0x155dad0` | Create and register a button from a definition |
| `0x1561fd0` | Construct the graphics node base |
| `0x1562390` | Detach a graphics node from its parent and siblings |
| `0x1562420` | Append a graphics node to a parent's children |
| `0x1562480` | Prepend a graphics node to a parent's children |
| `0x1592840` | Construct the button base |
| `0x1596660` | Generate a composed text texture |
| `0x15ac670` | Construct a button's GUI state and graphics nodes |
| `0x15adfa0` | Set a button's text |
| `0x15ae880` | Select a button's graphics state |
| `0x15ae9c0` | Attach a button's graphics nodes to their parent |
| `0x15aead0` | Hide a button |
| `0x15aeb30` | Show a button |
| `0x15bd180` | Submit a prepared text mesh |
| `0x1d58218` | Native button virtual table |
| `0x1d933b0` | Text texture node virtual table |
| `0x8c1430` | Create a native message window (existing crossing) |
| `0x23494b8` | Graphics context global pointer |

## Limits

Construction defaults do not establish the later state of a button: the dialog base's show propagates to the button group before message filling. This appendix contains no observation of the reported missing-button state, no attribution of the report to a texture, clipping, color or threading defect, and no claim that the report is repaired. Runtime results belong in the investigation's separate run record.
