# The seams between the world and the interface

Whether the game's world (what the day computes) and its interface and graphics (the windows, the map's layers and icons, the 3D entities, the graphics databases) are held apart by a few fields that could be enumerated, or are linked throughout: W1 of phase 3 in [plans/drawing-thread.md](../plans/drawing-thread.md), which would copy the world page by page into a second process where the interface runs. Baseline 1.37.5 (`9ad3efe1…`); all addresses are RVAs. Measured on 2026-11-10 in one late game with the heap census and a one-off scan built on it ([the run](../prototype-log/2026-11-10-world-interface-seams.md)); nothing in the framework uses it yet.

## How the two were told apart

- **The presentation's blocks**: every live block of the heap census (`dev-heap-census`, [plans/leak-finding.md](../plans/leak-finding.md)) whose allocation site lies in the code of the interface and the graphics, or for a block of the game's generic allocation, either of the two calls above it. The code was taken from the source files the asserts name (`…\source\…cpp`, found by every RIP-relative reference to such a string): the game's `graphics\` and the code after it up to the engine's (`0xde6000`–`0x14a6000`), the engine's graphics and GUI from `0x14d7000` (`clausewitzlib\graphics\`, `pdx_entity.cpp`, `spritetype.cpp`, `windowtype.cpp`; `0x14d7000`–`0x1500000`, `0x1540000`–`0x1640000`), its animation, sound, particles and back end (`0x169e000`–`0x16fb000`), and three pieces of the game's own code outside `graphics\`: the in-game idler (`ingameidler.cpp`, from about `0x804000` to `0x838000`; the map manager is made at `0x80a179`), the messages to the player (`messagehandler.cpp`, `0x8bf000`–`0x8d3000`) and the map's floating texts (`0x6e9860`). Where the idler's code begins was placed between `empire.cpp`'s last function found (`0x801b60`) and its own first (`0x806000`), not function by function.
- **The world's blocks**: what the game state (`[0x233fe78]`, `CGameState`) reaches through pointers, block by block, without passing through a block of the presentation's. A word points into a block when it holds an address inside it (any offset), is 4-aligned and is not below 0x10000.
- **A seam**: a word of a world block that points into a presentation block. Counted by the pair of allocation sites (the owner's and the target's), with the owner's field: for the countries' block (0x34612e0 bytes) its offset less 0x20 modulo the country's 0x9b78, for the provinces' block (0x3793907 bytes) its offset modulo 0x2e10.

## What it found

On the 1780.7.1 save in observe mode, 20 days in, paused:

- **The census**: 8.08 million live blocks, 1,950 MB. **The presentation's**: 605,565 blocks, 187 MB. **The world**, reached from the game state: 7.10 million blocks of 3,875 sites, 1,709 MB, 88% of what the census counts. The world is not a part of the heap set apart: following its pointers reaches nearly everything that is not the presentation's, the game's static databases and some of the engine's own buffers among it (a file system read buffer of 128 KB, `0x19f8450`).
- **World to presentation**: 42,993 words in 2,052 pairs of sites. 30 game days later at speed 5, 45,808 words in 2,076 pairs; 422 of the 2,203 pairs seen in either run had changed their count, by 2,815 words in all. The largest kinds:

  | From the world | Into | Words |
  | --- | --- | --- |
  | Each province, `+0x2f8` (all 4,942), and `+0x1b48` (330 after 30 days) | The in-game idler, the map manager (`0x10da520`, 0x5bc8 bytes) | 4,943, then 5,272 |
  | Each country, `+0x1868` (all 1,380); blocks of `country.cpp`'s (`0x2d2fc0`), `+0x140` | The map manager | 1,380; 723 |
  | Each province, `+0x1258` | Map icons (`0x13320a0`'s, [the map's icons](drawing-beside-the-day.md)) | 1, then 1,548 |
  | Blocks of the game's code by the nearest source file (`0x8ea420` by `modifier.cpp`, `0x7e7cd0` by `history.cpp`, `0x9d8fc0`, `0x71afd0`, …) and generic hash nodes (`0xc9e70`) | 3D entities and meshes (`pdx_entity.cpp`: `0x14f8630`, `0x14f9320`, `0x14fe140`, `0x14ed760`) | about 9,000 |
  | Blocks of the databases' code (`0xbbdf20`, `0x447f60`, …) | Graphics types: holders of sprite types (`0x14d79f0`, `spritetype.cpp`'s `0x156ba70`), their tables | about 5,000 |
  | Blocks of `map.cpp`'s (`0x88bb80`), `+0x30` | Animations (`pdx_anim.cpp`, `0x169e160`) | 580 |
  | Various | The minimap (`0x10b5fb0`, `0x10b59c0`), the graphics back end (`0x16d8990`), the map's layers | about 2,000 |

  The known two are among them: a unit's place (`+0x1a8`) and its model at `+0x70` ([drawing-beside-the-day.md](drawing-beside-the-day.md), "The day's crossings read again"), and a province's floating texts (`+0x2d0`–`+0x2e0`, the texts made by `0x6e9860`).
- **Presentation to world**: 88,962 words in 1,441 pairs of sites (111,122 after 30 days). The largest holders: the graphical unit types (`graphicalunittype.cpp`, `0x114cb30` and `0x114cdb0`: 25,000), the map's layers (`0x12deeb0`: 12,900), the holders of sprite types (`0x14d79f0`: 9,400), the minimap (10,500), the graphical map (`0x10a8930`: 5,200), `eu3graphics.cpp`'s (`0x10b3fe0`: 4,800) and the windows (`windowtype.cpp`, `0x155cb60`: 3,500). Blocks that are neither the world's nor the presentation's held 310,000 words into the world besides.
- **The game's writable data** (`.data`): of its eight-byte words that point into a block the census counts, 143 point into the world, 552 into the presentation's blocks and 712 into the rest. So few globals lead into the world (the game state, the countries' and provinces' arrays and their like); most lead to the engine and the interface.
- **What a paused frame writes into the world's blocks**, two stretches of about 10 and 18 seconds paused, no window open, nothing hovered: 0.2 MB in 70 pages of 6 sites in the first; in the second 16,843 words, of which 16,384 the file system's read buffer (not the world's state, but reached from it), 450 words in 9 of the 10,622 blocks of a site of `province.cpp`'s (`0x42d960`), each rewritten whole, 4 words of a unit (`0xd3acd0`, the unit's constructor) and 4 elsewhere. What wrote them was not found: the scan compares contents, not instructions.

## What it means for a world copied into another process

- The seams are of a dozen kinds, but they are not a dozen fields: some 43,000 to 46,000 pointers from the world into the presentation's objects, over about 2,000 pairs of the types that hold and are held, a fifth of which changed within 30 days; many held in generic containers (hash nodes, vectors) whose type the census does not tell. They include static data the interface loads at startup (the graphics types, the graphical unit types, the sprite types) that the world's own databases point at.
- So copying the world's pages into a second process works only if every presentation object the world points at is at the same address there too, which for the static ones means loading them in the same order through an allocator that gives the same addresses, and for the dynamic ones (the map manager, the entities, the map icons, the animations) the interface process's own, made in step with the world's; or if each handover translated some 45,000 pointers found by type. Neither is a matter of a few fields.
- Against this, the interface reaches the world through few globals (143 words of `.data`), and a paused frame wrote almost nothing of the world.

## Not verified

- **Blocks the census does not count**: those allocated before the framework started, and those of other allocators (Direct3D's and the driver's, the engine's pools that do not go through `malloc`), which the scan neither classifies nor reads.
- **False seams**: an eight-byte value that happens to equal an address inside a presentation block counts as a pointer; 4-alignment and the lower bound cut most numbers out, not all.
- **The classification by site**: a generic container's block is the presentation's when either of the two calls above the allocation is; one allocated deeper is classified by its first two callers alone. The idler's code's start is approximate.
- **One save, observe mode, 50 days**: a player's game, open windows, hovering, map modes other than the default, and the month's turn were not looked at; what the interface writes into the world was sampled paused only.
- **What each seam's field means** was not read in the code but for the map manager's, the map icons' and the units' models.
