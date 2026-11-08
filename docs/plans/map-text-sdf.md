# Map names drawn from distance fields

Status: proposed 2026-11-09, not yet decided with the owner; nothing built. The research it rests on is [research/map-name-shader.md](../research/map-name-shader.md); an offline comparison (below, "What the comparison showed") was made the same day, outside the game. It follows [unicode-module.md](unicode-module.md), whose map font and outline it changes, and rests on [ADR game-text-in-system-fonts](../adr/2026-11-02-game-text-in-system-fonts.md) and [ADR font-pages-grow](../adr/2026-11-08-font-pages-grow.md).

## Goal and scope

- **Goal**: the names on the map stay sharp however close the camera comes, with the outline the player chose, and stay steady when it is far.
- **In scope**: the map font's page, which holds distances instead of colors; `gfx/FX/mapname.shader`, changed as the game reads it so that it draws the names from those distances; the outline, drawn by the shader; mip levels for the page.
- **Out of scope**: interface text, which reaches the window a texel to a pixel already ([research/font-textures.md](../research/font-textures.md), "The glyph scale"); the names' meshes, layout and curves; the OpenGL renderer (`-opengl`), where the page and the shader stay as they are now.

## Why names blur now

From [research/map-name-shader.md](../research/map-name-shader.md), "Why names blur when the map is close":

- The names are meshes on the map, drawn by the map's own shader, which samples the font page with a linear filter and returns its color and alpha as they are.
- The framework draws a map font's glyphs at an em of about 75 pixels for a Chinese face (lines of 100). At the closest zoom a character is three to four times that on screen, so each texel is spread over three or four pixels: soft edges, and stepped ones along slanting strokes. Anti-aliasing the glyph in the page cannot help: it makes the page right at its own size, and the blur comes after.
- The page has one level, where vanilla's `Mapfont.dds` has 11: farther out the names are minified from the first level alone.

## What the comparison showed

A script drew 贝凯什 in KaiTi as the framework fills the page now (75-pixel em, the Chinese language mod's outline: a `0x838283` band at 190, 141, 94, 50, 21 and 7 one to six pixels out, `0x100c10` ink), and as a signed distance field of the same em (made from a render four times larger, eight texels of distance on each side in 8 bits), and magnified each four times as the map does at its closest:

- **The page as now, magnified bilinearly**: soft edges and halo, as the player's screenshot shows.
- **The page drawn twice as large** (150-pixel em, magnified twice): better, still soft.
- **The distance field, decoded by a shader**: edges sharp at the screen's pixels, the outline's fall-off the same curve and smooth; without the outline, sharp black ink.
- At the page's own size and at half of it the distance field looks as the page does now.

So a distance field the size of today's page gives sharper names than a page four times its memory. What the comparison could not show: the game's own fog and blending, the brush faces' sharp corners at larger magnifications (a single distance field rounds corners by about a texel), and the cost of making the fields.

## Behavior

### Sharp names (D1)

- The names on the map are drawn from a distance field: their edges stay sharp at every zoom, anti-aliased over one screen pixel, and look as now when the map is at its usual distance.
- Mods that change the map's names (their fonts, their layout) keep working; a mod whose `mapname.shader` does not have the line the framework changes keeps its own shader and today's page, and the framework logs why.

### The outline (D2)

- The outline is drawn by the shader from the distance: the color and the fall-off the map font's own page has, as today, measured from its `H` (`unicode/outline.rs`); or none, as the player chooses (`map_outline`, at the next start as now). It stays as sharp as the ink, and as smooth, at every zoom.

### Steady far away (D3)

- The page has mip levels, as vanilla's has: names far away are drawn from a smaller level and do not shimmer.

## How it fits together

- **The page** (`unicode/fill.rs`): a map font's page holds, for each glyph, the signed distance from the ink's edge in alpha, 0.5 on the edge, `S` texels to each side over 0 to 1 (`S` 8 in the comparison; enough for the outline's reach and the mip levels kept); red, green and blue are white and unused. A glyph's box grows by `S` on each side instead of the outline's reach; the glyph records are as now. Pages are sampled without sRGB decoding, so the distances reach the shader as stored.
- **Making a field**: from a coverage bitmap drawn larger than the page (four times in the comparison) by the exact distance transform `outline.rs` already has, then reduced to the page's texels. A map glyph takes 100 to 600 µs in DirectWrite now and 160 to 170 µs for its outline ([research/loading.md](../research/loading.md), "The startup after L1 to L3"); a field four times larger costs more, and the names on the map ask for thousands of glyphs when they are built. D1 measures it and chooses between that, a field from a coverage bitmap of the page's size (an anti-aliased distance transform), and a field from the glyph's outline, which DirectWrite gives as curves.
- **The shader**: `gfx/FX/mapname.shader` gets an `Addition::Edit` (`splice.rs`), applied as PhysFS reads it: the pixel shader's `tex2D( DiffuseTexture, v.vTexCoord )` becomes a function the edit puts before `main`, which decodes the distance, takes its width on screen from `fwidth`, and returns the ink over the outline with their colors and the outline's fall-off written in as literals. The rest of the shader, the fog and the opacity, is the game's or the mod's. The cache keys on the whole text, so the changed shader is compiled once and cached beside the game's.
- **One choice, made before the page**: the page holds distances only when the shader will decode them. The map font's page is made at the application's start, before the map's setup reads the shader, so the framework reads `gfx/FX/mapname.shader` itself through PhysFS when it makes the page, and fills it with distances only when the line it changes is there (and the renderer is Direct3D 9). The edit then finds the same text.
- **Mips** (D3): the page is a render target that grows ([ADR font-pages-grow](../adr/2026-11-08-font-pages-grow.md)); its levels are made by the card (`D3DUSAGE_AUTOGENMIPMAP`, if the device can for the format) after glyphs are put in and after it grows, or written with each glyph from the field reduced on the CPU. A glyph's box of `S` texels on each side keeps the glyphs apart down to the levels where `S` is still a texel.

## Parts

| Part | Delivers |
| --- | --- |
| D1 Distance field page and shader | The map font's page holds distance fields, made at a measured cost; `mapname.shader` is spliced to decode them; a mod's shader without the line keeps today's page. Sharp ink at every zoom, no outline yet |
| D2 The outline in the shader | The outline's color and fall-off, as measured from the font's own page, written into the spliced shader; none when the player chooses so |
| D3 Mip levels | The page with mip levels, kept as it grows |

Every part is done when it builds, its tests pass, both clippy runs are clean, and a test game shows it: the names at the closest zoom and from far away, with the outline and without, recorded in [prototype-log/](../prototype-log/) with screenshots.
