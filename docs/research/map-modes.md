# Map modes: switching them and coloring provinces (for `map`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` with `core/examples/disasm.rs`, and the shaders from the game's `gfx/FX`. What was seen in the running game on 2026-10-26, before this framework existed, is marked "seen (2026-10-26)"; it is not in [prototype-log/](../prototype-log/). The framework's own coloring was tested there (2026-10-29, remote control F1: the coloring showed, and cancelling restored the map mode).

## The map mode

The map manager (`[0x233fe78]+0x1e00`, vtable `0x1cb8460`) holds:

- `+0x2c`: **the current map mode**, i32 (`map_manager::MODE`; the dependency point `0x82205c` reads it in the refresh below).
- `+0x30`: the mode before it.
- `+0x40`: **the secondary option**, i32, which picks what the second layer of province colors shows (below).

The labels' switch `0x822580` names each mode by its localisation key and covers 0 to 0x54: 0 is `MAPMODE_TERRAIN`, 1 `MAPMODE_POLITICAL`, 12 `MAPMODE_UNREST`; 0x25 is "Timeline (WIP)", and 0x49 has no label of its own (it shares `MAPMODE_NOTSELECTED` with the modes past 0x54). The table that picks the secondary option covers only 0 to 0x4c.

## Switching: the refresh

**The refresh** `0x821ff0(manager, mode, reset, force)` is the map manager's method at `+0xc8` (`map_manager::REFRESH`). After some work of its own (a choice by mode passed to `0x10a9be0`, and extra steps for mode 0x25) it calls `0x6ec450(manager, mode, reset, force)`, which:

- **Returns without drawing** when `force` is clear and the mode is already current, unless `reset` is set and the option is not 0, in which case the option is cleared and the map drawn again.
- Otherwise stores the old mode at `+0x30` (unless the two are the same or either is 0x49), stores the new one at `+0x2c`, and sets the option: 0 when `reset` is set or the mode is above 0x4c, else by mode through the byte table at `0x6eca44` and the jump table at `0x6ec9c4`.
- **Mode 12 (unrest) gets option 0x1a** when `0xcfc70(0x233ff98)` answers yes, and option 1 otherwise. `0xcfc70` answers whether one of the game's switches is on (see [provinces.md](provinces.md)); which switch `0x233ff98` is was not worked out (inferred: a DLC's).
- Then calls the renderer `0x10a8c30(renderer, mode, option)`, where the renderer is `[[manager+0x330]+0x360]`, when it is not null.

The suppress-rebels window's constructor `0x1462aa0` switches to unrest through the same method, as `(12, 0, 0)`. It is no way to show the unrest map on its own: before that it calls `0x1c7a60`, which gives the selected army a new mission, whenever the army's mission (`army+0x1f8`, its method `+0x50`) is not of type 0xd (suppressing rebels).

## The color texture

**The renderer** `0x10a8c30(renderer, mode, option)` (`pdxmaptexturegeneration.cpp`) takes the map renderer `[renderer+0x58]`, stores the mode at its `+0x16c8` and the option at `+0x16cc`, and has the color buffer at its `+0x16d8` filled by `0x12fd9b0(buffer, mode, option)`. It then makes the province color texture at `+0x1698` from the buffer (once, in the engine's format 0xb, its height twice a layer's) or updates it, and passes the mode to each of the 0x1400-byte records at `[renderer+0x70]` (count `+0x7c`, through `0x132c050`; not looked at).

**The buffer** is `+0x00` a pointer to 4-byte pixels, `+0x08` and `+0x0c` two i32 dimensions. It holds two layers one after the other, each of `dim0 * dim1` pixels; the second starts at `data + dim0 * dim1 * 4`. Use the product, not either dimension, to index it. Seen (2026-10-26): dimensions 20 and 256, so 5,120 entries a layer, for 4,942 provinces.

**The fill** `0x12fd9b0(buffer, mode, option)`:

1. writes the first layer with `0x12f0b40(data, kind)`, where the kind is 3, or 4 in mode 1 when the settings' byte `[0x242bde8]+0x14a` is set (`0x12f0b40` was not looked at);
2. zeroes the first layer past the count of records in the map's vector at `[0x233fed8]+0x6c0` (0x230 bytes each);
3. writes the second layer with **the secondary colors** `0x12fdcb0(layer, option)`;
4. zeroes the second layer past the same count.

The shaders confirm the two layers: `GetProvinceColor` in `gfx/FX/standardfuncsgfx.fxh` picks the layer by adding `TextureIndex * 0.5` to V, and `calculate_secondary` in `gfx/FX/pdxmap.shader` samples layer 1 and blends its RGB over the map by its alpha times the `OccupationMask` texture's alpha. So the second layer shows as the stripes of that mask over the whole province, not as an outline. The map shader's color branch (`COLOR_SHADER`, where it draws the first layer's colors) uses `calculate_secondary`; its terrain branch uses `calculate_secondary_compressed` instead, which reduces each channel to six steps. Which branch the unrest mode takes was not looked at. The fog of war is drawn from a texture of its own, so these colors say nothing about what the player can see.

## The secondary colors

`0x12fdcb0(layer, option)` walks the map's provinces in their order (`[0x233fed8]+0x6f0` to `+0x6f8`, 0x2e10 bytes each, `objects::campaign::map::PROVINCES`) and writes one 4-byte entry for each, so entry `i` belongs to the `i`-th province of that vector, whose ID is its `+0x20`. Seen (2026-10-26): the IDs equal the indexes (149, 151 and 4703 sat at those indexes); the framework checks it for every province it colors rather than assuming it.

The bytes are in memory order B, G, R, A: the game's gradients write them in that order, and seen (2026-10-26), `FF 4B 14 FF` showed blue and `00 C8 FF FF` gold.

Options 1, 2 and 0x1a share the code; **only 0x1a builds the suppression areas**:

- It takes the selected army from the manager's selection at `+0x1b80` (count `+0x38`, the first item through `+0x28`, an army when its method `+8` answers 0), and walks the player's armies in the country's list at `+0xd18` (node: the army at `+0x00`, the next node at `+0x10`).
- For each army whose method `+0x80` says yes it calls `0x1a9890(army, set, unrest out, count out)`, the selected army's provinces going into one hash set and the others' into a second.
- `0x1a9890` goes on only when the army's mission (`+0x1f8`) is of type 0xd. It lists the countries the mission may act in (`0x1c7cf0`, from the army's country), then `0x1c7b40(mission, countries, set, …)` walks the mission's areas (array `+0x30`, count `+0x3c`), and in each area the province IDs from its `+0x88` to `+0x90`. It finds each province at `[0x233fe78]+0x1ca8 + id * 0x2e10`, keeps those whose owner tag (`+0x468`) is one of the countries, adds them to the set with `0x1aa6a0`, and adds up their unrest for the out arguments.
- Each province then gets `FF 4B 14 FF` when it is in the selected army's set, `C8 32 00 FF` when it is in another army's. Otherwise its entry is cleared, and in the unrest mode it gets a gradient from `0xa091b0(province)` (inferred: its unrest) when that is above 0.

## What the framework does

`map.rs` colors provinces without touching any army or mission: it switches to the unrest mode and overwrites the second layer's entries for its provinces after the game has written them.

- **Points**: it observes the fill `0x12fd9b0` (before, to note the buffer, its size, mode and option, and after), the secondary colors `0x12fdcb0` (after) and the renderer `0x10a8c30` (after), and calls the refresh `0x821ff0`, checked to be the map manager's method `+0xc8`.
- **Coloring**: it saves the current mode (only 0 to 0x4c), then calls the refresh as `(manager, 12, 0, 1)`. After the secondary colors, when the fill around them has mode 12 and option 26 (0x1a), the layer passed is the fill's data plus one layer, and the province vector fits in a layer, it writes each province's 4 bytes at `layer + id * 4`. It writes them again on every redraw of that mode while its colors are set.
- **Forcing**: `force` is set because a refresh to the mode already current returns without drawing.
- **Confirming**: the refresh succeeds only when the mode reads back, the fill was seen with that mode, the renderer returned for it, the colors were written (when coloring) and nothing unwound.
- **Restoring**: clearing calls the refresh with the saved mode, `(manager, mode, 0, 1)`, whatever mode the player chose in between. At a session change it restores the saved mode when the same map manager is still in mode 12, and otherwise redraws the current mode.
- **Thread**: it calls the refresh at a safe point, on the main thread, and the fill, the secondary colors and the renderer run inside that call on the same thread. Seen (2026-10-26): the mode request, the renderer, the fill and the secondary colors all ran on one thread, the one the game's interface runs on.
- The colors a refresh reads are borrowed from the game: the buffer and the province vector are read anew at every pass and never kept.

## Seen in the running game (2026-10-26)

- A switch to the unrest mode as `(12, 0, 0)` reached the renderer with option 26, so the switch at `0x233ff98` was on in that game.
- Writing `FF 4B 14 FF` into the entries of provinces 149 and 151 after the secondary colors, in mode 12 with option 26, showed blue stripes in exactly those provinces. A refresh to the saved mode 1 with `force` set then brought back the political map unchanged.
- Sets of 28, 66 and 107 provinces around province 149 drew the same way, the center in `00 C8 FF FF`.

## Not known

- When the switch at `0x233ff98` is off, the unrest mode gets option 1, and the framework, which writes only under option 26, reports the refresh as not confirmed. This is read from the code and was never seen.
- The provinces the framework does not color keep the unrest mode's own colors (a gradient of their unrest), and the first layer is the unrest mode's.
- The colors go when the player chooses another map mode while they are set; the framework does not hear it, and restoring returns to the mode saved when coloring began.
