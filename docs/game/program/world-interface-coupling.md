# The seams between the world and the interface

The following allocation and pointer observations describe a particular paused save. They show couplings to account for when reasoning about ownership; they do not establish a complete world/interface separation or a safe world-copy design.

## What it found

On the 1780.7.1 save in observe mode, 20 days in, paused:

- **The census**: 8.08 million live blocks, 1,950 MB. **The presentation's**: 605,565 blocks, 187 MB. **The world**, reached from the game state: 7.10 million blocks of 3,875 sites, 1,709 MB, 88% of what the census counts. The world is not a part of the heap set apart: following its pointers reaches nearly everything that is not the presentation's, the game's static databases and some of the engine's own buffers among it (a file system read buffer of 128 KB, `0x19f8450`).
- **World to presentation**: 42,993 words in 2,052 pairs of sites. 30 game days later at speed 5, 45,808 words in 2,076 pairs; 422 of the 2,203 pairs seen in either run had changed their count, by 2,815 words in all. The largest kinds:

  | From the world | Into | Words |
  | --- | --- | --- |
  | Each province, `+0x2f8` (all 4,942), and `+0x1b48` (330 after 30 days) | The in-game idler, the map manager (`0x10da520`, 0x5bc8 bytes) | 4,943, then 5,272 |
  | Each country, `+0x1868` (all 1,380); blocks of `country.cpp`'s (`0x2d2fc0`), `+0x140` | The map manager | 1,380; 723 |
  | Each province, `+0x1258` | Map icons (`0x13320a0`'s, [the map's icons](../interface/frame-lifecycle.md)) | 1, then 1,548 |
  | Blocks of the game's code by the nearest source file (`0x8ea420` by `modifier.cpp`, `0x7e7cd0` by `history.cpp`, `0x9d8fc0`, `0x71afd0`, …) and generic hash nodes (`0xc9e70`) | 3D entities and meshes (`pdx_entity.cpp`: `0x14f8630`, `0x14f9320`, `0x14fe140`, `0x14ed760`) | about 9,000 |
  | Blocks of the databases' code (`0xbbdf20`, `0x447f60`, …) | Graphics types: holders of sprite types (`0x14d79f0`, `spritetype.cpp`'s `0x156ba70`), their tables | about 5,000 |
  | Blocks of `map.cpp`'s (`0x88bb80`), `+0x30` | Animations (`pdx_anim.cpp`, `0x169e160`) | 580 |
  | Various | The minimap (`0x10b5fb0`, `0x10b59c0`), the graphics back end (`0x16d8990`), the map's layers | about 2,000 |

  The known two are among them: a unit's place (`+0x1a8`) and its model at `+0x70` ([drawing-beside-the-day.md](../interface/frame-lifecycle.md), "The day's crossings read again"), and a province's floating texts (`+0x2d0`–`+0x2e0`, the texts made by `0x6e9860`).
- **Presentation to world**: 88,962 words in 1,441 pairs of sites (111,122 after 30 days). The largest holders: the graphical unit types (`graphicalunittype.cpp`, `0x114cb30` and `0x114cdb0`: 25,000), the map's layers (`0x12deeb0`: 12,900), the holders of sprite types (`0x14d79f0`: 9,400), the minimap (10,500), the graphical map (`0x10a8930`: 5,200), `eu3graphics.cpp`'s (`0x10b3fe0`: 4,800) and the windows (`windowtype.cpp`, `0x155cb60`: 3,500). Blocks that are neither the world's nor the presentation's held 310,000 words into the world besides.
- **The game's writable data** (`.data`): of its eight-byte words that point into a block the census counts, 143 point into the world, 552 into the presentation's blocks and 712 into the rest. So few globals lead into the world (the game state, the countries' and provinces' arrays and their like); most lead to the engine and the interface.
- **What a paused frame writes into the world's blocks**, two stretches of about 10 and 18 seconds paused, no window open, nothing hovered: 0.2 MB in 70 pages of 6 sites in the first; in the second 16,843 words, of which 16,384 the file system's read buffer (not the world's state, but reached from it), 450 words in 9 of the 10,622 blocks of a site of `province.cpp`'s (`0x42d960`), each rewritten whole, 4 words of a unit (`0xd3acd0`, the unit's constructor) and 4 elsewhere. What wrote them was not found: the scan compares contents, not instructions.

## Not verified

- **Blocks the census does not count**: those allocated before the framework started, and those of other allocators (Direct3D's and the driver's, the engine's pools that do not go through `malloc`), which the scan neither classifies nor reads.
- **False seams**: an eight-byte value that happens to equal an address inside a presentation block counts as a pointer; 4-alignment and the lower bound cut most numbers out, not all.
- **The classification by site**: a generic container's block is the presentation's when either of the two calls above the allocation is; one allocated deeper is classified by its first two callers alone. The idler's code's start is approximate.
- **One save, observe mode, 50 days**: a player's game, open windows, hovering, map modes other than the default, and the month's turn were not looked at; what the interface writes into the world was sampled paused only.
- **What each seam's field means** was not read in the code but for the map manager's, the map icons' and the units' models.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x801b60` | function | Referenced while examining how the two were told apart; complete role not established |
| `0x804000` | code | Referenced while examining how the two were told apart; complete role not established |
| `0x806000` | code | Referenced while examining how the two were told apart; complete role not established |
| `0x838000` | code | Referenced while examining how the two were told apart; complete role not established |
| `0x8bf000` | code | Referenced while examining how the two were told apart; complete role not established |
| `0x8d3000` | code | Referenced while examining how the two were told apart; complete role not established |
| `0x14d7000` | code | Referenced while examining how the two were told apart; complete role not established |


Presentation blocks were classified by allocation-caller ranges. World blocks were traced from `CGameState` without traversing presentation blocks; aligned words pointing inside counted blocks were treated as candidate pointers. The false-positive and allocator limitations above apply to that method.
