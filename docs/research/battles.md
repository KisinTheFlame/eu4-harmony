# Battles: a battle fought (for `game::events`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-05 with `core/examples/disasm.rs` and capstone scripts over the PE file. A combat's and a side's fields are in [sieges.md](sieges.md#objects); how a battle is decided and settled in [defeat.md](defeat.md).

## Hearing a battle once

- **Naval battles go the same way.** The naval combat's daily update is the land's `0x22c080` (slot `+0x88` of both vtables), so a decided naval battle reaches `0x22bea0` too; the overrun test `0x22acb0` leaves kind 2 alone, but `0x22bea0` may still call `0x22b5a0`.
- **Once per battle.** `0x22b5a0` reached from `0x22bea0` (call at `0x22c002`) is entered before `+0x44` is set, so observing both settlements hears such a battle twice. Two ways to hear it once:
  1. Observe both settlements, and skip `0x22b5a0` when its return address is `0x22c007` (the `0x22bea0` call); or keep the last combat ID (`+0x14`) heard.
  2. Observe the entry of the results `0x239ff0(combat, winner index)`, which both settlements call exactly once (`0x22b917` in the overrun, `0x22c04c` in the regular one, with the settlement's winner index), with `+0x44` still clear; skip the third caller `0x23b2ff` (a unit taken out of the combat; index 0 regardless) and kind 3. At that point the losses are taken (the overrun adds its own before, `0x22b815`/`0x22b82a`) and no unit has been removed yet (the overrun's destroy and the regular retreat come after).
- **What to report, read at either entry** (winner index 0: the attacker `+0x20` won):
  - province: combat `+0x30` → its ID `+0x20`; kind: method `+0x60` (1 land, 2 naval);
  - per side: attacker `+0x20` / defender `+0x28` (side `+0x168` says which), its countries (`+0x100`, tag at entry `+0x10`, entry 0 leading; `0x23a0ab` `4c 8b 92 08 01 00 00 48 8b 92 00 01 00 00`) or each unit's owner (`+0xd0`), its leader (`0x21fe80`), its units' strength now (regiments `+0x30`…`+0x38`, each `+0x28`), its losses (`+0x1c` total, `+0x134` by kind, thousandths) and its composition by kind (`+0x118`…);
  - the combat ID `+0x14` for once-only.
- Not read: when `+0x118`…`+0x130` are filled (whether they are the strength at the start), what losses mean for ships (presumably ships × 1000 or hull lost), and what `0x237d50` (called by the results, "SetupNonSiegeResults") keeps of the battle for the war's history.

## What the framework does, and what the game showed

`game::events` observes the entry of `0x239ff0`, goes on for a combat of kind 1 or 2 whose `+0x44` is clear, unless the call returns to `0x23b304` (a unit taken out of a combat), and reads the province, each side's countries and its losses (`+0x1c`, in thousandths: men, or ships), only while someone listens to `battle`. Tried on 2026-11-05 ([prototype-log.md](../prototype-log.md), "Player actions A4"): two naval battles of England's fleets and one land battle England lost were each heard once; the land battle's loser was heard `army_defeated` too, before it.
