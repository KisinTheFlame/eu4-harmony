# Font page textures (for the Unicode module)

Baseline 1.37.5 (`9ad3efe1…`). All addresses are RVAs. What is marked [V] comes from eu4.exe's disassembly and has been checked; [I] is inferred; what is marked "tested" has been verified in the game.

## The texture manager

- **The texture manager** = `[[font+0x48]+0x480]` (`font+0x48` is the graphics context) [V]. Its table is guarded by the `CRITICAL_SECTION*` at `[0x242b818]`.
  - `+0x08`: an array of 0x48-byte entries; a font's `+0x970` holds an index into this array.
  - `+0x38`: a map from name to index.
- **An entry (0x48 bytes)**[V]:
  - `+0x00` pointer to the texture wrapper;
  - `+0x08` name (std::string);
  - `+0x28` / `+0x2c` width, height;
  - `+0x34` reference count;
  - `+0x38` the "loaded" flag.
- **The texture wrapper (0x28 bytes, allocated with the game's allocator)**[V]: `+0x00` is the `IDirect3DTexture9*`, `+0x08` the sRGB sampling flag, `+0x0c` the usage (0 or 0x200 = DYNAMIC).
- **The renderer's function table**: 0x1fd11a8–0x1fd1378, 81 slots in all, filled in at run time by 0x16c1480 with the same values as the static ones in `.data` [V].

## How a font page loads

- `0x15953c0` loads the page texture first, then calls the `.fnt` parser 0x1595570 [V].
- **Page file name**: the path at `font+0xe0` plus `.tga`; when the interface scale is ≥ 1.6 and `font+0x110` is not empty, `font+0x100` is used instead [V]. The resolving function 0x16c24d0 looks for `.dds` first and uses `.tga` if there is none.
- **`+0x970`**: the texture manager index, -1 at construction; returned by `0x16c35b0` (Load) and written at 0x15954b0 [V].
- **`+0x978` / `+0x97c`**: the page's width and height, taken from the values that the texture's `GetLevelDesc(0)` put in the entry at `+0x28` [V].
- **Creation chain**[V]:
  1. 0x16c2980 reads the file and hands it to 0x16cc6e0 to decode (DDS through 0x16dd990, TGA through 0x16dd5f0);
  2. through the renderer slot `[0x1fd1320]` = 0x16d7de0 it reaches 0x16d7aa0;
  3. 0x16d7aa0 calls `D3DXCreateTexture`, then `D3DXLoadSurfaceFromMemory` for each mip level, with a NULL destination rectangle, that is, the whole surface.
- **Format**[V]: a 32-bit TGA becomes `A8R8G8B8` with 1 mip level; a `.dds` keeps the format and mips it has in the file. DDS decoding already rounds a DXT width and height up to multiples of 4 (0x16ddb01).
- **Memory pool**: on a 9Ex device it is `D3DPOOL_DEFAULT`, which cannot be locked [V].

## The 16 MiB limit (tested)

- After 0x16c2980 reads the file, it runs `cmp r14d, 0x1000000` at 0x16c2cb7: a file of 16 MiB or more is not decoded, no error is reported, the texture stays empty, and the font's `+0x978` / `+0x97c` are both 0.
- The Chinese language mod's map font page `zh-hans-map.dds` is 49 MB (DXT5, 6400×7663), so the country, province and sea names on the map are all invisible. Vanilla without the framework behaves the same; the CJK font plugin relaxed this limit.
- The framework raises the limit to 256 MiB (`font_load`). Tested after the change: map names all show as correct Chinese.

## How drawing gets the texture

- **Fetched again at every bind**[V]: `0x16c3f10`(texture manager, index) returns the wrapper and has about 80 callers. The bind sites for text:
  - main text: 0x159a15d, 0x159a3a0, 0x159a4e4, 0x159a62d, 0x159a741, 0x159b322;
  - buttons: 0x1597c0c, 0x1597e77;
  - floating text on the map: 0x159cc9c;
  - map names: 0xfd7d86 (binds 3 textures, the first being the font page).
- They then call slot `[0x1fd11f0]` = 0x16d8560, which reads `wrapper+0` every time and then calls `SetTexture`. So **replacing the texture at +0 in the wrapper makes every text path use the new texture**, without hooking any drawing code.

## UV

- The four text paths use the same formula [V]: `u = x / W`, `v = y / H`, where `W = font+0x978` and `H = font+0x97c`, with no half-pixel offset.
- A glyph's x, y, width and height are read as signed 16-bit values, so coordinates on the page must be below 32768.
- Where `+0x978` / `+0x97c` are read: main text 0x159a957 / 0x159a983, buttons 0x159852c / 0x1598557, floating text 0x159cfd8 / 0x159d007, map names 0x159e09b / 0x159e0d0.
- When a page grows taller but keeps its width, only `font+0x97c` and the entry's `+0x2c` need changing; the existing glyphs' pixel coordinates stay the same.

## Lifetime

- **A device reset leaves font textures alone** [V]. It calls 0x16d4bb0 / 0x16d4b20 before and after, then tells each font to release and rebuild its text vertex buffers (0x15949a0 / 0x15948e0). On 9Ex, textures in the DEFAULT pool stay valid after Reset [I].
- **Font destruction** [V]: the destructor 0x15947e0 releases the font's entry in the texture manager through 0x16c3ca0 (once the reference count reaches zero, the entry goes into a ring of entries waiting to be reclaimed rather than being destroyed at once), and frees the glyph table (0x1594360).
- **Actually destroying a texture** always goes through slot `[0x1fd1330]` = 0x16d80f0 [V].
- **Reloading in place**: the console command `reloadtexture` goes through 0x16c4220, which rebuilds the texture in the same wrapper but does not update the entry's width and height [V].
- Fonts are created only while `.gfx` files are parsed (0x17068b0 calls Load at 0x170741a); `reload` / `reloadinterface` do not rebuild fonts [I].

## Threads

- Fonts are created by the loading thread while `.gfx` files are parsed [V]. Text in the main menu and in the game is drawn by the main thread [I]; the loading screen is drawn and presented by another thread (tested, see `framework_ui/hook.rs`).
- The device is created with `D3DCREATE_MULTITHREADED` (0x44) (tested; set at 0x16d911f).

## Constraints from batches and caches

- **One texture per batch**: all of a font's glyphs must be on the same page [V].
- **UVs are cached** [V]:
  - each font has 40 cached vertex buffers at `font+0x3c08`;
  - buttons draw their text into cached render targets;
  - map name meshes are cached too.
- So a new glyph must be put in the page before its first use; existing glyphs cannot move; and once the page has been used for the first time, its width and height cannot change either. The page's size has to be settled when the font loads.
- Main text submits its vertices every 0x48c of them (0x159afa0); the shared vertex buffer holds 1200 vertices.

## Adding glyphs to a page (`unicode/fill.rs`)

- **Format numbers**[V]: 0x16f9850 maps the engine's format numbers to Direct3D's: 1 and 2 are DXT1, 3 and 4 DXT3, 5 and 6 DXT5, 0xb and 0xc A8R8G8B8; for the rest, see the function's jump table.
- **Compressed pages** (tested): the Chinese language mod's six pages are all single-level DXT5. A new `D3DUSAGE_DYNAMIC` texture is created, also DXT5 and 1024 rows taller, and the original blocks are copied into it unchanged; new glyphs are encoded as 4×4 blocks, and a glyph with its one-pixel gap fills whole blocks, so every position in the atlas also falls on whole blocks.
  - All four edges of a rectangle locked in a compressed texture must lie on whole blocks, otherwise `LockRect` returns `D3DERR_INVALIDCALL`.
  - Video memory grows only by "width × 1024" bytes per page; the map page grows from 6400×7664 to 6400×8688.
- **What a dynamic page costs in memory** (measured 2026-10-28, process private bytes around each step, Chinese language mod, bitmap fonts):
  - creating a `D3DUSAGE_DYNAMIC` page commits about its own size in system memory (2048×4864 DXT5: +10 MB; 6400×8688: +53 MB);
  - writing it in bands of 256 rows, each locked on its own, commits far more and keeps it: +77 MB for the first 10 MB page, +57 MB for the next 7 MB one, +425 MB for the 56 MB map page, then under 1 MB for each of the three pages after it, as if a pool had grown to the largest need;
  - the framework's own heap had no part in it. System fonts write their dynamic pages the same way (A8R8G8B8, the map's 4096×4096, 64 MB) and their font loads cost much less (the map font's +253 MB against +539 MB); why was not looked into.
- **The glyph records the fonts bring** (measured the same way): 46 font objects load, and most of them parse the same `.fnt` files, so the 21,443 glyphs above 0xFF of the body font are stored once per font: 994,216 records, each allocated on its own, 66 MB of the framework's heap. System fonts drop them.
- **Ink color** (tested): the pages of body text fonts are white and are tinted when drawn; map names use the page's own color (the Chinese language mod's map page is near black, `0x100c10`). Added glyphs take the color of the most opaque pixel of the letter `H` in the original page.
