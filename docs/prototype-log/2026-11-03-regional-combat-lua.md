# 2026-11-03 Regional combat in Lua (tested by me)

L6 of [lua-extensions.md](../lua-extensions.md): the regional combat mod rewritten in Lua (its own repository, branch `lua`), in the running game. Its descriptor in `mod\` and the playset were the player's own. One run of `eu4.exe`, started directly, driven by `eu4-harmony game`, clicked with `tools/game-input.ps1`: a new game as France, war declared on England on 1444.12.12 (`diplomacy.declare_war`, a core casus belli), time advanced to 1445.4, `menu.load autosave`. The game was claimed with the game-lock skill, which backed up the player's files first.

- **Startup**: "Regional Combat (EU4 Harmony) (id regional-combat, api 0.2): extension lua/main.lua loaded; 2 UI fragment(s)".
- **The button**: the Paris army selected from the outliner showed the "区域" button in the button area with its Chinese tooltip; a left click turned the task on, "center 183 radius 5", and the button's glow showed.
- **The preview**: Paris yellow, the provinces within 5 land steps in blue stripes, England's Normandy among them.
- **Decisions**: waiting at first; on a later day "attacking" and a move to Bordeaux (174) the game accepted; when that target was gone, "returning" and a move back to Paris, accepted. While the army moved, its orders were locked and the task waited and tried again.
- **The radius editor**: a right click opened it ("区域战斗", "半径 5", the arrows, apply and cancel); plus and apply logged "radius 6".
- **A manual order**: `army.move` through remote control, the player's order, turned the task off ("task off: manual order").
- **Save data**: the autosave of 1445.1.1, written while the task was on, held the mod's data; `menu.load autosave` gave "restored 1 task(s)".
- **Cost, not met**: 32,694 stretches, 5,037 ms in all, about 0.15 ms a stretch (the average target is met); but 142 stretches over 5 ms, most 25 to 35 ms and the longest 65 ms, about one a game day: the day's read of every province (`province.list`, about 4,900 provinces made Lua tables) for the snapshot, and the range's `can_step` calls, rebuilt each day. Coarser capabilities are to bring it down (L6's fourth point).
- **Not verified in the game**: a defeat turning the task off, a recovery of an occupied province, a split or merged army; a save written by the native extension (none of the player's saves holds its data: they were written before data went into saves, ADR extension-save-data-in-the-save).
- **Cleanup**: the game was closed by ending its process. Releasing the game restored the player's files, the autosaves and the logs among them, and found every file as the backup had it.
