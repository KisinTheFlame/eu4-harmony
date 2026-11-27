# AI support checks


## The support check

`0x106320`(agent, province, army, strength there, share asked, …), eight arguments, answers yes or no (`al`). An army agent's think (`0xea250`) asks it for its main army where it stands (`0xea430`), when the agent's `+0xb1c` is set, with the strength in that province worked out first (`0xd46800`, into the fourth argument) and a share of 1000; its other units' decisions ask it too (`0xf4490`). A no sets the agent's `+0xb1e` and `+0xb1f` and holds that for 7 thinks (`+0xb20`); a yes counts that down. What the agent does differently while `+0xb1e` is set was not followed; inferred, it keeps away from where it would be outnumbered.

What it works out, in the game's terms: **whether the friendly strength that can reach this province in time outnumbers the enemy strength that can by the share asked.** (Corrected on 2026-11-08: it was first read as the friends against a strength given, which the third loop below shows it is not.)

1. The friends' sum begins at the strength already in the province and a constant (`[0x1fd3b38]` plus the fourth argument's, `0x1063c7`), in `r15`.
2. **The country's own units** (the list at `country+0xd18` of the agent's country, `0x106405`..`0x106422`): for each, a route query (`0x10671e`); one that can get there in time adds its strength (`0xd46800`, or a cached value when it stands there already), scaled down by how long it would take (`add r15, r8`, `0x1068aa`).
3. **Its allies in its wars**: for each country in its `current_war_allies` (`country+0x1bc0`), that country's units, the same way (route query `0x106c3d`, `add r15, r8` at `0x106dca`).
4. **The enemies**: for each country in the list at `country+0x1b88` (_inferred_ its enemies in its wars), that country's units, the same way, but into a second sum, the strength to beat, which begins at 1.0 (`mov r12d, 0x8000`, `0x106e01`; route query `0x107194`, `add r12, r8` at `0x107328`), with their count in `[rbp+0x158]`.
5. After the three loops: if the seventh argument (`only_enemies`, `param_7`) is non-zero, the function skips the two friendly loops (keeping only the initial strength and `param_3`'s leader pips) and immediately returns yes (`al = 1`) when the reachable enemy regiment count `[rbp+0x158]` does not exceed the combat-width threshold `((([country+0x2440]+0x8c) + 1000) * 20) / 1000` (`0x10739a`..`0x1073d3`). Otherwise it calls `0x10cd80` with `(max_friendly_leader_pips - max_enemy_leader_pips)` and `-terrain_penalty` (`[province+0x28]+0x30` → `+0xd4`) to produce a fixed-point roll factor into `[rbp+0x170]`, scales the friends-over-enemies ratio (`idiv r12`, `0x107433`) by that factor, and compares against the share asked (`0x107401`..`0x107459`).

In each loop it leaves out the army itself, units without a valid owner (unless standing in the target province), units its method `+0x88` refuses, those retreating (`+0x203`), of three kinds by their task method `+0x50` (5, 11 and 12), and—in the two friendly loops only—units in a siege (`+0xc8`) without `+0x209` set; then those whose map distance to the province, made a time by a square root (`0x88a390`, `0x1065eb`..`0x106634`), is over the define at `[0x233fc7c]`. Units standing in the target province's region (`+0x340`) or an adjacent region (`region+0x20`) bypass the route query `0x96c520`.

Only the answer leaves the function: the routes go into lists of its own, freed after each query ([routes.md](../military/routes.md), "What it writes"), and `0x10cd80` writes only its local output slot. Because `0xd46800` records the maximum leader fire+shock pips (`[leader+0x4c] + [leader+0x50]`) into its sixth argument and `0x10cd80` is monotonically non-decreasing in the leader difference, both friendly/enemy strength sums and leader maxima are monotonic and order-independent. Evaluating free (same/adjacent-region) units first, bounding pending units with their scaled strengths and leader pips, and resolving pending `0x96c520` route queries lazily in descending strength order (`core/src/speed/support_checks.rs`) stops as soon as the `[lower, upper]` interval guarantees either yes or no, while preserving exact integer equality.

## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0xee5a0` | function | Referenced while examining the wait, with s9 and s10 built; complete role not established |
| `0xeea70` | function | Referenced while examining the wait, with s9 and s10 built; complete role not established |


## Limits of the reading

The interpretation of country `+0x1b88` as war enemies and the roles of agent bytes `+0xb1c`/`+0xb1e` remain inferred. `0xd46800` was not followed to prove nonnegative strength. Route side-effect coverage comes from static analysis, not a write watch. The crossing point of the friends’ running sum, unlogged years and games after 1784 remain outside the observed sample.
