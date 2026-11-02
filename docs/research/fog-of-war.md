# Fog of war: what the player sees (for `game::events` and `game::world`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` while regional-combat was developed (2026-10-26), and checked again with `core/examples/disasm.rs` on 2026-11-03: every address below holds the instructions described. What was seen in the running game on 2026-10-26, before the framework existed, is marked "seen 2026-10-26"; it is not in [prototype-log.md](../prototype-log.md). Everything else was read from the code only.

## The two bits

A province (`0x2e10` bytes, by ID from `world+0x1ca8`, as `objects::campaign` names it) is hidden from the player by fog of war when both of these are set:

- **bit `0x20` of `province+0x959`**: fog of war is on for it. The game console's `fow` sets it on every province at once;
- **bit `0x02` of `province+0x958`**: the latest fog refresh hid it from the player.

Neither is enough alone: `+0x959` bit `0x20` is set on every province whenever fog of war is on. The game's own readers test the two together, and so does the framework (`game::world::hidden`). This is the current player's view only; there is no such answer for another country. It is not the camera's view, and not which provinces the player has discovered.

## The switch

- `world+0x23a6`, u8: fog of war is on.
- `0x76ed60(world, on)` goes through the world's vector of province pointers (`+0x1c90` to `+0x1c98`), sets or clears bit `0x20` of each one's `+0x959`, and stores `on` at `world+0x23a6`.
- The game console's `fow` is `0xe7a990` ("Fog of War is now ON." at `0x1d2ef60`, "…OFF." at `0x1d2ef78`). Without an argument it calls `0x76ed60(world, !world+0x23a6)`. With a province ID it tests the two bits: when they hide the province it calls `0xa0b150(province, 0)` ("Fog of War turned off in …"), otherwise `0xa0b150(province, 1)` ("…turned on in …").

## The setter

`0xa0b150(province, hidden)` writes the result of the fog for one province:

- bit `0x04` of the map data's `+0x228` (`[province+0x28]`, the byte whose bit 8 is land) to `hidden`;
- bit `0x02` of `province+0x958` to `hidden`;
- when that bit changed and the province has a render object (`province+0x2d8`), it adds the map data's `+0x154` to that object's vector at `+0x340` and sets its `+0x338`, so the map is drawn again.

It writes state, so it is never called to ask what the player sees. The framework observes it (`VISIBILITY` in `game::events`): a call outside a fog refresh, or on another thread than the refresh, means the cache changed without one.

## The refresh

`0x8275e0()` works out again what the player sees, and is the only place that sets the cache for the whole map (`FOW_REFRESH` in `game::events`):

- It takes the player's country as `0x811b20` does: `world+0x1e68` when `world+0x1e66` is 7 and `world+0x1e6f` is set, otherwise `world+0x1e60`.
- When fog of war is off (`world+0x23a6` clear) or the tag is not valid (its top byte clear), it calls `0xa0b150(province, 0)` on every province: nothing is hidden. So an observer without a country sees the whole map.
- Otherwise it fills an array of one byte a province, all hidden, and clears the provinces the player's country and the countries sharing its vision (decided with `0x3bb250`) own or control, their neighbors, and the provinces where those countries' armies are (each country's units at `+0xd18`, each army's province at `army+0xe8`) and their neighbors. Other branches, for subjects, special armies and other relations, were not followed.
- Last it goes through the provinces (`world+0x1ca8`, stride `0x2e10`) and calls `0xa0b150` (at `0x828083`) on each whose hidden state differs from the array. A province with bit `0x10` of `+0x959` set whose bit set at `+0x5e0` (indexed by the player's country index) does not hold the player is hidden whatever the array says; what that bit and that set stand for (inferred: a province the player has not discovered yet) was not looked at.
- It allocates its array on the heap and writes the cache, so it is never called by the framework.

When this refresh runs, and how often, was not read from the code. Seen 2026-10-26: one refresh came while a save was loaded (4942 provinces, 4557 hidden), and others as days passed in the game (4561 hidden by the eighth); the provinces sampled, 149, 151 and 4703, were visible; the setter's call from `0x828083` turned bit `0x02` of `+0x958` on (`0x10` to `0x12`); the player's tag and the world were the same before and after each refresh; every refresh, the daily update and the map manager's frame ran on one thread.

## Readers in the game

- The province tooltip `0xa075a0` returns `CANT_SEE_UNDER_FOW` (`0x1cd9270`) at once when the two bits hide the province.
- The army tooltip `0xd47780` (the army class's method `+0x18`, vtable `0x1c42540`) leaves out an army whose province (`army+0xe8`) the two bits hide, unless the army's owner (`army+0xd0`) is the player. So the player's own armies show under fog; an army anywhere else does not.

## What the framework does

Visibility counts as current only after a complete refresh (`game::events`): `0x8275e0` returned without unwinding, on the thread it started on, with the same world, date, player and fog switch as when it started, and with no daily update, no setter call outside it, no order or defeat the framework heard of and no load since. Capabilities that read what the player sees (`game::armies`, `game::provinces`, `game::world`) answer only then, and read a province's two bits only while it holds. A load starts a new session and drops the proof (`load_begin`).
