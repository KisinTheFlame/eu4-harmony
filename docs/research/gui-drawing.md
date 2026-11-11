# The GUI's drawing

What the GUI's drawing does, from the in-game drawing's two calls (`0x14dd010`, `0x14dd540`, made by `0x8183b0`) down to the graphics back end's function table (`0x1fd10f8`–`0x1fd1378`, [architecture.md](architecture.md), "The graphics back end"): how it walks its nodes, which node classes draw and through what, which back end slots it calls with what data, what it reads and writes as it draws, and how its calls could be recorded at the update and played by the drawing thread. T3b of [drawing-thread.md](../plans/drawing-thread.md) (the windows' drawing recorded) rests on it. Read statically from the 1.37.5 `eu4.exe` on 2026-11-11; nothing here was run. RVAs are into that build; what is inferred is marked so.

## The walk

`0x14dd010(graphics)` and `0x14dd540(graphics)` take the graphics object (`[0x23494b8]`, [architecture.md](architecture.md)) and do the same for two trees of scene nodes embedded in it: the first the one at `graphics+0x110`, its draw list at `+0x298`; the second the one at `+0x2b0`, its list at `+0x438` (inferred: the layer drawn over the first, as it is drawn after it).

1. `0x14dcf90`: the viewport from `graphics+0x325bc` (slot `0x1fd12d0`), then the blend state `graphics+0x90` (`0x1fd1348`) and the rasterizer state `graphics+0xb0` (`0x1fd1350`), through the context slot `0x1fd1200`.
2. The tree is walked depth first (the root's children by `+0xf8`, the next sibling; the children at `+0x100`; `0x14dd7a0` recursing): for each node, its slot `+0x70` (shown) and, with children, `+0x270` (any child shown). A shown node gets its world transform written at `+0xac`–`+0xeb` (its parent's, moved by the node's place at `+0x9c`, `+0xa0`), the rendered objects' counter `[0x23494d0]` counted up, its slot `+0x48` called, and the node appended to the tree's draw list.
3. Unless the other camera is on (`0x233ff4f`), `0x14dda50(graphics, list, graphics+0x48, graphics+0x58)` tests the list against the point at `graphics+0x48` and fills `graphics+0x58` with the nodes under it, and those of them with a `+0x118` go into `graphics+0x70` (inferred: the cursor's point and what it is over, for hovering; [extension-windows.md](extension-windows.md) has `0x14dda50` as the hit test of input).
4. `0x14dd350(graphics, list)` draws the list in order. For each node: when its `+0x130` is set, the rasterizer state with the scissor (`graphics+0xc0`, `0x1fd1350`) and the node's rectangle `+0x128`–`+0x12f` (four i16, cut to the viewport's size) as the scissor rectangle (`0x1fd12a8`); its slot `+0xb8` (a layer, u16) and `+0x98` (its alpha, a float); then its slot `+0x58` called as `draw(node, graphics, context, node+0xac, alpha, layer, 0)`; and the rasterizer state `graphics+0xb0` back after a clipped node.
5. After the first tree only: `0x1587e10`, the particles' statistics text, drawn only while `[0x23495c8]` is set (inferred: a debug switch); then the wireframe switch off (`0x1fd12a0`, the renderer's `+0x131`, while `[0x233ff46]` is set) and the states `+0x98`, `+0xc8` set.

So the walk writes interface state as it draws: each shown node's world transform, both draw lists (`+0x298` is also what input's hit testing walks, [extension-windows.md](extension-windows.md)), the hover lists `+0x58` and `+0x70`, and the counter.

## The node classes

Every scene node derives from the class whose constructor is `0x1561fd0` (vtable `0x1d94728`; the local transform at `+0x6c`, the world transform `+0xac`, the parent `+0xf0`, the sibling `+0xf8`, the children `+0x100`, the hover flag `+0x118`, the rectangle `+0x128`, the clip flag `+0x130`). Sixteen vtables carry its slot `+0x70` (`0x1148230`); the binary has no RTTI, so they are named here by what they do. Every one's `+0x48` is `0x8f480`, which does nothing: the walk does no per-class work but the transform.

| Vtable | Draw (`+0x58`) | What it draws (inferred from its callees) |
| --- | --- | --- |
| `0x1d94728`, `0x1d5ff68`, `0x1d930d8` | `0x1a35124` | nothing: abstract |
| `0x1d60238` | `0x955a0` | nothing: a window's own node (`window+0xa30`, made in `0x1543f80`), a container |
| `0x1d933b0` | `0x1550950` | a text in a texture of its own: buttons and instant text boxes ([font-textures.md](font-textures.md)) |
| `0x1d93c08`, `0x1d9b568`, `0x1d9b848`, `0x1da3630`, `0x1da3960` | `0x1558b90`, `0x15bd540`, `0x15bdce0`, `0x162ed30`, `0x162eee0` | through the element's own slots (`+0x90`, `+0x98`, `+0xb0`, `+0xc0`, `+0x1f8`), not followed further here; `0x162eee0` builds its buffers itself (`0x162f3a0`, below) |
| `0x1d979c0` | `0x158e1d0` | geometry of its own, its vertices and indices written as it draws (`0x1fd1190`, `0x1fd1278`) |
| `0x1d9d3c8` | `0x15d8e50` → `0x15d8780` | a sprite, turned by its angle (`0x1a53df0`, `0x1a564e0`: sine and cosine) |
| `0x1d97630` | `0x158d740` → `0x15d9f80` | a sprite of another kind (inferred) |
| `0x1da1d40` | `0x15f3f20` | its children clipped: its own scissor (`0x1fd12a8`) and rasterizer state |
| `0x1d9a830` | `0x15b1ea0` | an item of the multiplayer server list (`matchmaking_server_item`) |
| `0x1d7e848` | `0x13a9090` | through `0x12339d0`, `0x1233630`, `0x1234090`, `0x1233de0`, which draw with the shared constants at `[0x2340090]`, `[0x2340098]` (inferred: shapes drawn on the GUI) |

The text is drawn by two font classes: `0x1d99108` (slot `+0x58` `0x1596660`, the text into a texture of its own; `+0x60` `0x159c600` and `+0x68` `0x15988e0`, the text drawn as glyph quads at once) and `0x1d57170` (`+0x48` `0x159c600`, `+0x50` `0x15988e0`); `0x14db3c0(graphics, name)` finds a font by its name.

## The back end slots

Every call reaches DX9 through the table: no direct call of any of its DX9 functions was found in the code (`0x16d4e00`, `0x16d4e90`, `0x16d4f00`, `0x16d4fa0`, `0x16d5e00`, `0x16d65d0`, `0x16d6640`, `0x16d6890`, `0x16d71b0`, `0x16d73e0`, `0x16d7770`, `0x16d79c0`, `0x16d80f0`, `0x16d8560` have no direct callers). The context the slots take is the renderer's `+0x20` (`0x1fd1200`, `0xcf460`); the renderer keeps its device at `[[context]+8]` and caches state at `[context]` (the topology `+0x28`, the vertex count `+0x100`, the shader set `+0x108`, the viewport `+0x70`–`+0x87`, the blend values `+0x89`…). The slots the GUI's drawing reaches, with what each reads:

| Slot | DX9 function | What | Reads at the call |
| --- | --- | --- | --- |
| `0x1fd1200` | `0xcf460` | the context | the renderer |
| `0x1fd12d0` | `0x16d4c10` | the viewport (SetViewport) | a viewport struct (24 bytes), cached in the renderer |
| `0x1fd1348`, `0x1fd1350`, `0x1fd11d8` | `0x16d7770`, `0x16d79c0`, `0x16d73e0` | blend, rasterizer, and depth or blend state objects (SetRenderState, 10, 4 and 12 at most) | the state object, against the renderer's cache |
| `0x1fd12a8` | `0x16d8d30` | the scissor (SetScissorRect) | four i16 at the pointer |
| `0x1fd12a0` | `0x16d8ea0` | the wireframe switch | none: writes the renderer's `+0x131` |
| `0x1fd11a0` | `0x16d5e00` | a shader pair and its vertex declaration (SetVertexDeclaration, SetVertexShader, SetPixelShader) | the effect object |
| `0x1fd1198` | `0x16d6890` | vertex buffers (SetStreamSource) | an array of buffers and of strides at pointers; the first's count into the renderer's `+0x100` |
| `0x1fd11e0` | `0x16d70d0` | an index buffer (SetIndices) | the buffer |
| `0x1fd1170` | `0x16d4e70` | the topology | none: writes the renderer's `+0x28` |
| `0x1fd11d0` | `0x16d22e0` | a constant buffer's memory filled | copies from the pointer into the buffer's memory (`[buffer+8]`): no device call |
| `0x1fd11e8` | `0x16d71b0` | constants uploaded (SetPixelShaderConstantF, SetVertexShaderConstantF by the flags) | the buffer's memory, its size `[buffer]`, or the count given |
| `0x1fd11f0` | `0x16d8560` | a texture and its samplers (SetTexture, SetSamplerState) | the texture object |
| `0x1fd1190`, `0x1fd1278` | `0x16d6640`, `0x16d7030` | a vertex or index buffer written (Lock, a copy, Unlock; discard or no-overwrite by the flags) | the source at the pointer, `stride × count` bytes |
| `0x1fd1168`, `0x1fd1208` | `0x16d4f00`, `0x16d4e90` | draw indexed, draw (DrawIndexedPrimitive, DrawPrimitive) | the renderer's topology and count |
| `0x1fd1320`, `0x1fd1370`, `0x1fd1328`, `0x1fd1330`, `0x1fd1188` | `0x16d7de0`, `0x16d82b0`, `0x16d7f30`, `0x16d80f0`, `0x16d65d0` | a texture or a render target made, a texture's description, a texture or a buffer let go | — |
| `0x1fd1360`, `0x1fd1290`, `0x1fd1298`, `0x1fd1368` | `0x16d8ca0`, `0x16d4e00`, `0x16d4e50`, `0x16d4c80` | the render target, BeginScene, EndScene, Clear | — |
| `0x1fd1258`, `0x1fd12c0` | — | taken around a texture's loading (`0x16c3f10`) | — |
| `0x1fd1220`, `0x1fd1248` | `0x8f480`, `0x955a0` | nothing with DX9 | — |

Globals the draws read: the shared constant buffer at `[0x2349688]` (the sprite's draw fills it with `0x1fd11d0` and uploads it with `0x1fd11e8`, so its memory holds the last draw's constants only), `[0x2349690]`, `[0x2349698]` (the text node's), `[0x2340090]`, `[0x2340098]`, the default colour at `0x242b828`, the font table at `graphics+0x580` (`0x38` bytes an entry), the GUI scale at `graphics+0x325b8`.

## What the drawing reads and writes as it draws

Beyond the walk's own writes (above):

- **A text whose texture is due is made as it is drawn.** `0x1550950`: while the node's `+0x2db` is set it is laid out again (its slot `+0x90` with the scale `graphics+0x325b8`) and the flag cleared; while it has no texture (`+0x278` 0) it finds its font by name (`0x14db3c0`) and has its slot `+0x1a8` make it, which is `0x1550e90` → `0x1596660`: the text drawn into a texture of its own, its own BeginScene, render target, Clear and EndScene, the old texture let go (`0x1fd1330`) ([font-textures.md](font-textures.md)). So drawing into a texture happens inside the GUI's drawing whenever a text changed since the last drawing, and at the update for the tooltips ([drawing-beside-the-day.md](drawing-beside-the-day.md), "Frames late at speed 5").
- **A texture not loaded yet is loaded as it is drawn.** The texture manager's `0x16c3f10` loads an entry that has none under the lock at `0x242b818` (`[0x1b66750]`, an import), the slots `0x1fd1258` and `0x1fd12c0` around it; `0x156bfb0` names this lazy loading (`legacy_lazy_load`).
- **Buffers are made and written as they are drawn.** `0x162f3a0` (the node class `0x1da3960`) makes and lets go buffers (`0x1fd11a8`, `0x1fd1188`) and writes them (`0x1fd1190`); `0x158e1d0` writes its vertices and indices every draw; the glyph quads of `0x15988e0` are written into the font's buffer each time a text is drawn so.
- **Read at draw time**: each node's alpha (`+0x98`) and layer (`+0xb8`), its world transform, its rectangle; the text node's flags and texture; the sprite's angle and frame (inferred: its frame from the sprite type at draw time); the cursor's point (`graphics+0x48`) for the hover test; the viewport's size. No read of the world (`[0x233fe78]`) or the localisation was found in the draw methods followed; the element draw methods behind `+0x1f8`, `+0xc0` and the rest were not followed to their ends.

## Recording at the back end

The table is data (`.data`), rewritten only at startup (`0x16c1480`), and every DX9 call of the GUI's goes through it, so the framework can put a recorder in each slot's place without touching code: on the main thread, between the GUI's drawing's entry and its return, while recording, a slot records the call instead of making it; anywhere else it calls the DX9 function as before.

- **Recorded, with copies**: the viewport (24 bytes), the scissor (8 bytes), the constants uploaded (`0x1fd11e8`: the bytes of `[buffer+8]`, `[buffer]` or the count given, as they are at the call: the shared buffer at `[0x2349688]` is refilled for every sprite), the buffer writes (`0x1fd1190`, `0x1fd1278`: `stride × count` bytes from the source, and the offset and flags), the vertex buffers' and strides' arrays (`0x1fd1198`).
- **Recorded, by reference**: the state objects (`0x1fd1348`, `0x1fd1350`, `0x1fd11d8`), the effects (`0x1fd11a0`), the textures (`0x1fd11f0`), the index buffers (`0x1fd11e0`), the vertex buffers themselves, the topology and the draws.
- **Done at once, not recorded**: the context (`0x1fd1200`), a constant buffer's memory filled (`0x1fd11d0`, which writes no device), the making of textures, buffers and state objects, a texture's description, a texture's loading, and everything between a BeginScene and its EndScene that the GUI's drawing begins itself (a text drawn into its texture): that draws into a texture of its own and leaves the frame's state as it found it but for the renderer's cache (inferred from `0x1596660`'s calls; a run must show it).
- **Played** by the drawing thread where its frame now calls `0x14dd010` and `0x14dd540`, each recorded call made with its copies, in order; the renderer's cache is the drawing thread's as it plays (one thread uses the renderer at a time, under the presentation lock).
- **Kept alive**: every texture, buffer, effect and state object a recording refers to, until the next recording replaces it. The game lets textures go through `0x1fd1330` (58 places in 32 functions, `0x1596660`'s among them) and buffers through `0x1fd1188` (58 places in 46 functions): both slots can be deferred as the mesh objects are ([drawing-beside-the-day.md](drawing-beside-the-day.md)), let go when the next recording is made, whoever let them go.
- **Still read live by the drawing thread**: the resources' contents. A texture drawn into again in place, or a buffer written outside the recording, while a recording that uses it plays, changes what is drawn and uses the device beside the drawing thread; that is still a crossing, which takes the lock, but need hold no frame back.

## Risks, and what static reading cannot settle

- **The GUI moved between updates.** The map icons ([drawing-beside-the-day.md](drawing-beside-the-day.md), "Where the map icons are placed") and the tooltip are moved by the drawing thread through their windows' setters, which change nodes' places; a recording holds the world transforms already turned into constants and vertices, so it would not move. Following them needs the recording to know which calls are whose node, and to play them with that node's transform changed: the world transform reaches the device only through the constants uploaded before its draw, which a run must show for each node class.
- **Animations, fades and hover between updates** play frozen at the last recording's: at speed 5 the update comes once a day, paused once a frame.
- **What a recording holds is what the drawing reads at the update**, so a node's draw that reads something the drawing thread changes (the camera, the cursor) is drawn as it was then: nothing of the camera was found in the GUI's draw methods followed, but the element methods behind `+0x1f8` and the rest were not followed to their ends.
- **The renderer's cache**: recording skips the calls, so the cache the main thread's own drawing into textures meets is the drawing thread's last; the cache's filters compare against it (the shader set `+0x108`, the blend values `+0x89`) and could skip a call the device needed. Clearing the cache's compared fields before and after each text drawn into a texture, as BeginScene clears `+0x108`, would be the guard; whether more of it is filtered was not read.
- **Size**: a late game's GUI makes thousands of calls a frame (not counted); the copies are the constants (a few hundred bytes a draw) and the buffer writes (the glyphs of every text drawn as quads).
- Not settled: which element classes reach which node class; whether any draw method followed no further here calls the device outside the table (none found among those followed); the frame index of animated sprites; the shapes behind `0x1d7e848`; what `0x1fd12c0` and `0x1fd1258` do around a load.
