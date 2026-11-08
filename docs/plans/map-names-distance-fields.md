# Map names drawn from distance fields

Status: decided with the owner 2026-11-09; D1 to D3 built and tried in a test game the same day, in one pull request ([the run](../prototype-log/2026-11-09-map-names-from-distance-fields.md)). The term it adds to [GLOSSARY.md](../../GLOSSARY.md) is Distance field. The research it rests on is [research/map-name-shader.md](../research/map-name-shader.md); an offline comparison (below, "What the comparison showed") and a measurement of its costs ("What it costs") were made the same day, outside the game. It follows [unicode-module.md](unicode-module.md), whose map font and outline it changes, and rests on [ADR game-text-in-system-fonts](../adr/2026-11-02-game-text-in-system-fonts.md) and [ADR font-pages-grow](../adr/2026-11-08-font-pages-grow.md).

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

So a distance field the size of today's page gives sharper names than a page four times its memory. A field made from the glyph's coverage at the page's own size (below, "How it fits together") looks as the one from a render four times larger, and one thresholded at the page's size has stepped edges. What the comparison could not show: the game's own fog and blending, and the brush faces' sharp corners at larger magnifications (a single distance field rounds corners by about a texel).

## What it costs

Measured 2026-11-09 outside the game, on the owner's machine, with the characters the map shows: the 932 that the Chinese language mod's province and country names use (`prov_names_l_english.yml`, `countries_l_english.yml`), in FZShengShiKaiShuS-EB-GB at 78.2 pixels, the map font and em of the player's log. A timing test beside `outline.rs`, removed afterwards, drew each glyph and made its outline or field; `fxc` compiled the game's cached mapname pixel shader and one changed as D1 and D2 would change it.

| | Now | Distance field |
| --- | --- | --- |
| A glyph on the CPU | 720 to 770 µs in DirectWrite, 170 to 180 µs for its outline | The same DirectWrite, 167 µs for its field from coverage (from a bitmap twice as large, thresholded: 560 + 710 µs; four times: 950 + 2,400 µs) |
| The map's 932 glyphs | about 0.85 s | about the same; 0.16 s more when the player chose no outline, which makes no outline now |
| The pixel shader, Shader Model 3 slots | 40 (3 texture, 37 arithmetic) | 68 (3 texture, 65 arithmetic), on the names' pixels only; the map's terrain shader takes 392 on every pixel of the map |
| Compiling the shader | about 45 ms, once, then cached | about 45 ms, once, then cached |
| The page's memory | 2560 × 5120 by the end of the player's session, 50 MB | a glyph's box 8 texels wider on each side instead of the outline's 6: about 9% more texels with the outline, 45% more without; mip levels a third more |

- **The CPU**: unchanged with the outline, as making the field takes as long as drawing the outline does now. Most of the time is DirectWrite's, as now, and the glyphs are drawn where and when they are now ([research/loading.md](../research/loading.md), "The startup after L1 to L3").
- **The card**: 28 more arithmetic instructions on the pixels the names cover, against the map's own shaders on every pixel; not measurable in a frame, and not measured in the game.
- **Memory**: the page holds the same glyphs at the same size, so about as much as now, a third more with mip levels (D3). A smaller em would save it but costs the details: at 56 pixels the edges waver, at 40 strokes merge (tried in the comparison), so the em stays as now.

## Behavior

### Sharp names (D1)

- The names on the map are drawn from a distance field: their edges stay sharp at every zoom, anti-aliased over one screen pixel, and look as now when the map is at its usual distance.
- Mods that change the map's names (their fonts, their layout) keep working; a mod whose `mapname.shader` does not have the line the framework changes keeps its own shader and today's page, and the framework logs why.

### The outline (D2)

- The outline is drawn by the shader from the distance: the color and the fall-off the map font's own page has, as today, measured from its `H` (`unicode/outline.rs`); or none, as the player chooses (`map_outline`, at the next start as now). It stays as sharp as the ink, and as smooth, at every zoom.

### Steady far away (D3)

- The page has mip levels, as vanilla's has: names far away are drawn from a smaller level and do not shimmer.

## How it fits together

- **The page** (`unicode/fill.rs`, `unicode/map_field.rs`): the page of the map's own font, `tahoma_60`, which the map asks for by name (two dependency points), holds for each glyph the signed distance from the ink's edge in alpha, 0.5 on the edge, `S` texels to each side over 0 to 1: `S` is the outline's reach and 2 more, at least 4 (8 with the Chinese language mod's 6-pixel outline, 4 without one), enough for the outline and two mip levels; red, green and blue are white and unused. Other fonts with lines over 48 pixels keep their colors. A glyph's box grows by `S` on each side instead of the outline's reach; the glyph records are as now. Pages are sampled without sRGB decoding, so the distances reach the shader as stored.
- **Making a field**: from the glyph's coverage at the page's own size, as DirectWrite draws it now. The pixels it covers in part are where the edge runs; one exact distance transform (the one `outline.rs` has, keeping which pixel is nearest) gives each texel its distance to the nearest of them, and that pixel's coverage where in it the edge lies: the signed distance is the distance, negative inside the ink, plus 0.5 less that coverage. Where whole ink meets none, with nothing covered in part between, the edge runs between the two pixels. A field from a bitmap four times larger, thresholded, looks the same and costs fourteen times as much (below, "What it costs").
- **The shader**: `gfx/FX/mapname.shader` gets an `Addition::Edit` (`splice.rs`), applied as PhysFS reads it: the pixel shader's `tex2D( DiffuseTexture, v.vTexCoord )` becomes a function the edit puts before `main`, which decodes the distance, takes a screen pixel's width in texels from its derivatives (`ddx`, `ddy`), and returns the ink over the outline with their colors and the outline's fall-off written in as literals. The rest of the shader, the fog and the opacity, is the game's or the mod's. The cache keys on the whole text, so the changed shader is compiled once and cached beside the game's.
- **One choice, made before the page**: the page holds distances only when the shader will decode them. The map font's page is made at the application's start, before the map's setup reads the shader, so the framework reads `gfx/FX/mapname.shader` itself through PhysFS when it makes the page, and fills it with distances only when the line it changes is there; the edit changes the shader only once a page holds distances, so a game whose fonts are not filled (no Direct3D 9Ex device, `-opengl`) keeps the game's shader. The edit then finds the same text.
- **Mips** (D3): the page is a render target that grows ([ADR font-pages-grow](../adr/2026-11-08-font-pages-grow.md)), made with `D3DUSAGE_AUTOGENMIPMAP` when the card makes mip levels for it (`CheckDeviceFormat`); after glyphs are put in, or the page grows, the card makes its levels again at the next frame the main thread presents (`GenerateMipSubLevels`). A card that makes none leaves the page one level, and the log says so. A glyph's box of `S` texels on each side keeps the glyphs apart down to the levels where `S` is still a texel.

## Parts

| Part | Delivers |
| --- | --- |
| D1 Distance field page and shader | The map font's page holds distance fields, made from the glyphs' coverage; `mapname.shader` is spliced to decode them; a mod's shader without the line keeps today's page. Sharp ink at every zoom, no outline yet; the cost of the glyphs in the framework's log against today's. The frame time was left unmeasured: the owner asked for an estimate instead |
| D2 The outline in the shader | The outline's color and fall-off, as measured from the font's own page, written into the spliced shader; none when the player chooses so |
| D3 Mip levels | The page with mip levels, kept as it grows |

Every part is done when it builds, its tests pass, both clippy runs are clean, and a test game shows it: the names at the closest zoom and from far away, with the outline and without, recorded in [prototype-log/](../prototype-log/) with screenshots.
