# AI support checks


## The support check

`0x106320`(agent, province, army, strength there, share asked, …), eight arguments, answers yes or no (`al`). An army agent's think (`0xea250`) asks it for its main army where it stands (`0xea430`), when the agent's `+0xb1c` is set, with the strength in that province worked out first (`0xd46800`, into the fourth argument) and a share of 1000; its other units' decisions ask it too (`0xf4490`). A no sets the agent's `+0xb1e` and `+0xb1f` and holds that for 7 thinks (`+0xb20`); a yes counts that down. What the agent does differently while `+0xb1e` is set was not followed; inferred, it keeps away from where it would be outnumbered.

What it works out, in the game's terms: **whether the friendly strength that can reach this province in time outnumbers the enemy strength that can by the share asked.** (Corrected on 2026-11-08: it was first read as the friends against a strength given, which the third loop below shows it is not.)

1. The friends' sum begins at the strength already in the province and a constant (`[0x1fd3b38]` plus the fourth argument's, `0x1063c7`), in `r15`.
2. **The country's own units** (the list at `country+0xd18` of the agent's country, `0x106405`..`0x106422`): for each, a route query (`0x10671e`); one that can get there in time adds its strength (`0xd46800`, or a cached value when it stands there already), scaled down by how long it would take (`add r15, r8`, `0x1068aa`).
3. **Its allies in its wars**: for each country in its `current_war_allies` (`country+0x1bc0`), that country's units, the same way (route query `0x106c3d`, `add r15, r8` at `0x106dca`).
4. **The enemies**: for each country in the list at `country+0x1b88` (_inferred_ its enemies in its wars), that country's units, the same way, but into a second sum, the strength to beat, which begins at 1.0 (`mov r12d, 0x8000`, `0x106e01`; route query `0x107194`, `add r12, r8` at `0x107328`), with their count in `[rbp+0x158]`.
5. After the three loops: the friends' sum over the enemies' (`idiv r12`, `0x107433`), scaled by the seventh argument, against the share asked (the fifth, 1000 from the think), yes when it is larger (`0x107401`..`0x107459`).

In each loop it leaves out the army itself, units without a valid owner, units its method `+0x88` refuses, those retreating (`+0x203`), of three kinds by their method `+0x50` (5, 11 and 12), and those whose map distance to the province, made a time by a square root (`0x88a390`, `0x1065eb`..`0x106634`), is over the define at `[0x233fc7c]`.

Only the answer leaves the function: the routes go into lists of its own, freed after each query ([routes.md](../military/routes.md), "What it writes"), and the cleanup after the loops (`0x10cd80`) gives nothing it uses. Each term added is a strength scaled by a positive factor, and both sums are integers, so neither depends on the order its terms are added in. But the bar the friends must pass is the enemies' sum, which the third loop works out after them: **while the friends are counted, the bar is not yet known**, so the friends' queries cannot be stopped early as the game's code runs. What can, each with the same answer: the enemies' loop stopping once the friends can no longer pass (a no), and, with the enemies counted first, the friends' loops stopping once they have passed (a yes).

## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0xee5a0` | function | Referenced while examining the wait, with s9 and s10 built; complete role not established |
| `0xeea70` | function | Referenced while examining the wait, with s9 and s10 built; complete role not established |


## Limits of the reading

The interpretation of country `+0x1b88` as war enemies and the roles of agent bytes `+0xb1c`/`+0xb1e` remain inferred. `0xd46800` was not followed to prove nonnegative strength. Route side-effect coverage comes from static analysis, not a write watch. The crossing point of the friends’ running sum, unlogged years and games after 1784 remain outside the observed sample.
