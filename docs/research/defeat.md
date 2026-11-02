# Defeat: an army losing a land battle (for `game::events`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` while regional-combat was developed (2026-10-26), and checked again with `core/examples/disasm.rs` on 2026-11-03: every address below holds the instructions described. What was seen in the running game on 2026-10-26, before the framework existed, is marked "seen 2026-10-26"; it is not in [prototype-log.md](../prototype-log.md). What was checked in the running game through the framework is marked "tested", from that log. Everything else was read from the code only. The logged texts quoted come from the game's own log calls.

## A battle

- A battle has its two sides at `+0x20` and `+0x28` (`objects::campaign::battle::SIDES`), its ID at `+0x14`, and at `+0x44` a u8 set once its result is in (`battle::SETTLED`). Its method `+0x60` says its kind.
- A side has its units as a list at `+0xe8` (head; tail at `+0xf0`) and their count, an i32, at `+0xf8` (`side::UNITS`, `side::COUNT`); a node holds the unit at `+0`, the previous node at `+8` and the next at `+0x10`, null at the end. A u8 at `+0x169` is set when the side is overrun.
- A unit has its ID, a u64, at `+0x20`, the battle it is in at `+0xc0` and the siege at `+0xc8`; the land army's class is `0x1c42540`.
- Dependency points hold these offsets: `0x23a066` (the result's guard and the sides, in `0x239ff0`), `0x23a0e3` (the loser at `+0x28`), `0x21eb42` (`mov rdi, [rcx+0xe8]`) and `0x21ec87` (`dec dword ptr [r15+0xf8]`), in `objects::campaign`.

## Deciding a battle

The battle's daily update `0x22c080` (it logs `DailyUpdate`) decides at `0x22ce87`:

- if `0x21d470`(side 0) or `0x21d540`(side 1) says yes, side 1 wins: winner side 1, loser side 0, index 1;
- else if `0x21d470`(side 1) or `0x21d540`(side 0) says yes, side 0 wins, index 0;
- else, for a battle of kind 1 (its method `+0x60`) whose method `+0x70` says no, the overrun test `0x22acb0` (at `0x22ceef`); otherwise the fighting goes on.

A decided battle goes to the regular settlement `0x22bea0`(battle, winner side, loser side, winner index) at `0x22cf5d`. The winner index picks the winner among the sides: index 0 makes `+0x20` the winner and `+0x28` the loser, index 1 the other way round.

`0x21d470` says yes for a side overrun (`+0x169`), with no units, whose regiments' strength (each regiment's `+0x28`) adds up to less than 10, in a battle of kind 1 where every unit has `+0x210` set, or when the side's method `+0xa0` says so: it is no single morale field. `0x21d540` first tests the world's byte `+0x23a2`, then compares the current player's country with the side's (a vector at `+0x100` to `+0x108`); what it stands for in play is not known.

## Settling it

- **The regular settlement** `0x22bea0` adds up the regiments' strength on both sides and, by their ratio, either overruns the loser, calling `0x22b5a0` with the same four arguments (at `0x22c002`), or logs `Combat ended (regular)`, makes the results (`0x239ff0`(battle, winner index) at `0x22c04c`) and sends the loser off (`0x21eb30`(loser side) at `0x22c054`).
- **The overrun** `0x22b5a0`, same arguments: sets the loser's `+0x169`, takes the rest of its losses, logs `Combat ended (overrun)`, makes the results (`0x239ff0`) and destroys the losing side's units (`0x21ef40`(loser side, …), which copies its list and removes each unit through `0x768e20`).
- **The overrun test** `0x22acb0` calls `0x22b5a0` itself, without `0x22bea0`: side 0 winning at `0x22b0af`, side 1 at `0x22b127`. It leaves a battle of kind 2 alone. The battle's daily update calls it before the fighting (`0x22c9ef`) and in the later test (`0x22ceef`). So `0x22bea0` alone misses an army overrun at once.
- **The retreat** `0x21eb30` finds each losing unit's retreat province (`0xd4a9e0`, `0xd4b270`), stops it (`0xd4fcb0`) and orders it there through the army's action, `0xd429e0`(army, province, 0, 0) ([army-orders.md](army-orders.md)). A unit with nowhere to retreat to is removed (`0x768e20`), as are some others in special states (`+0x210`); such an army is gone by the next day.
- **The results** `0x239ff0`(battle, winner index) set `+0x44` once and make the results.

The loser is the third argument of both settlements, and their entry is the last moment its list is whole: the retreat and the overrun remove units, and a removed unit is deleted.

## What is not a defeat

- `0x21eb30` is also the siege's way out (`0x23fce0`, called at `0x24036e`), with an empty side.
- `0x239ff0` is also called by `0x23b1d0`, which takes one unit out of a battle and, if it was its side's last, logs `Combat ended (Unit removed)` and makes the results.
- The retreat flags (`+0x203`, `+0x204`) are set by the retreat command `0xd6b910` (the player retreating, [army-orders.md](army-orders.md)) and by `0xa9e160`, which takes a rebel unit out of sieges and combats, as well as after a lost battle.
- A battle whose side is already empty returns early from its daily update without either settlement.

So none of the flags, `0x21eb30` or `0x239ff0` tells a lost battle from a retreat, a siege ending or a unit taken out by script; and an army that is simply gone has not, by that alone, been defeated.

## What the framework hears

`game::events` observes the entry of both settlements, `0x22bea0` and `0x22b5a0` (`DEFEAT_A`, `DEFEAT_B`), except while a save loads. It goes on only when the index is 0 or 1, the battle's `+0x44` is still clear and the sides at `+0x20` and `+0x28` are the winner and loser the arguments name. It walks the loser's list, at most its count and never a node twice; if any unit is not a land army (a naval battle) it hears nothing, otherwise it hears `Defeated` for each unit whose owner (`+0xd0`) is the player.

- An overrun reached through `0x22bea0` enters `0x22b5a0` with the same arguments before `+0x44` is set, and the framework does not tell the two apart: from the code, such an army is heard defeated twice.
- An army that loses and retreats is heard just as one destroyed; its retreat also changes its route, which [army-orders.md](army-orders.md) hears as an order.

## In the running game

- Seen 2026-10-26: the regular settlement's retreat called from `0x22c054` three times during the daily update, inside the map manager's frame, each with one losing land army whose retreat flags were still clear. Whether those armies were the player's was not known. The other calls of `0x21eb30` seen then came from the siege (`0x24036e`) with empty sides.
- Tested (2026-10-30, regional combat's defeat): own armies destroyed in a battle were heard as `army_defeated`. Not tested: an army that survives a defeat, an overrun, a retreat with nowhere to go, a naval battle, and whether the overrun through `0x22bea0` is in fact heard twice.
