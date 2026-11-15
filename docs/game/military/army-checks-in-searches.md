# What an army is asked again at every move of a search

## The fort zone's first question

`0xd45250`(army, candidate, current, read the route) (checked: 4 arguments, [routes.md](routes.md), "Route history") answers whether a move from `current` to `candidate` is blocked by a fort's zone of control. Before anything else it answers no for an army of the special country `[0x233d8a4]` (`army+0xd4`), and no when `0xd40ae0`(army, `0xe0`, 0) says the army may pass forts. Only then does it look at the two provinces (land, `+0x228` bit `0x08`), the army's type and exile (`0x99ca0`), the provinces it came from (`+0xf0`, `+0xf8`), its route (`+0x100`; with the fourth argument set, a copy of it made with `0x1beba0` and freed at the end), the forts of the provinces (`0xa273a0`, `0xa275d0`), their owners and controllers, the countries' relations, and the candidate's neighbors.

`0xd40ae0`(unit, modifier, by military tag) (checked: 4 arguments) answers whether a modifier that is on or off is on for the unit. Each test is `0x8e7b60`(set, modifier):

- the leader (`unit+0xd8`, when its method `+0x50` answers yes) and its modifiers at `+0x68`;
- **the army's mercenary company** (`unit+0x48`, null for an army that is none): `0x900070`(company) gives the modifiers of the company's definition (`+0x28`, its set at `+0x88`), or an empty set. That `+0x48` is the company: the check whether two armies may merge (`0xd45de0`) refuses one with it set with `CANT_MERGE_MERCENARY_COMPANY` unless `0x222` `MODIFIER_ALLOW_MERCENARIES_TO_SPLIT` is in the owner's modifiers or in that same set; a split copies it to the new army (`0xd5c920`, `unitcommands.cpp`). A company's definition is read from the game's files at load;
- **the unit's own set**, `0xd4c6e0`(unit, modifier, null), which makes a set on its stack (vtable `0x1c42770`), has `0xd4ca70` fill it, reads the modifier from it, and lets it go;
- its owner's modifiers (`0x93d750(country+0x28c0)+8`, the country by the unit's tag at `+0xd0`, or by its military tag, the method `+0x138`, when the third argument is set);
- for a fleet (the method `+0x88`), each of the units its method `+0x70` lists, through a functor.

The answers are or-ed. Its only other direct caller is the power-spending check `0x9526d0` ([power-spending.md](../countries/power-spending.md)).

**The unit's own set** (`0xd4ca70`(unit, set)): for each regiment (`unit+0x30`..`+0x38`) it adds the regiment's strength (`+0x28`) to a sum. For a regiment that `0xadcb50` lets count, it also adds the strength times the regiment's drill (`+0x30`). `0xadcb50` checks the native virtual eligibility and expansion gate. Non-1 categories then pass directly; category 1 additionally needs the company or country to allow mercenary drill, as established by the [complete drill reading](drill.md). When some regiment counted, `0xd4ca70` copies the static modifier at `0x8ecec0()+0xf30` into the set (`0x8ea2d0`, its name copied with `0x95110` and its pairs) and scales it (`0x8ea920`) by the army's drill over the define at `[0x233e530]`. _Inferred_ from what it reads: the army drill modifier, scaled by the army's average drill. So the unit's own set holds the drill's modifiers, and the question about passing forts is asked of it like any other.

## The movement speed

The army class's method `+0x110` (`0x1a6780` → `0x1a6800`, `army.cpp`, its log line `vSpeed, %d`) works out an army's speed into a province. It starts from the define `[0x233fddc]` and adds:

- `0xd40790`(army, out, `0xfb`, …): the leader (`0xd40710`), its mercenary company's set (`+0x48`), the unit's own set again (`0xd4c390` → `0xd4ca70`), and the owner's modifiers (`0x93d750`);
- a term for the leader's maneuver (`army+0xd8`, its `+0x48`);
- the defines for a retreating (`+0x204`) or forced (`+0x201`) march;
- the province's local friendly or hostile movement speed (`0x8f`, `0x90`, by whether the province is friendly, `0x368560`), read from `province+0x998` and passed through the modifier table's object for it.

Only the province's terms depend on where the army goes; `0xd40790`'s value is the army's.

It is asked through the travel time `0xd4f4a0`(army, out, from, to) (`unit.cpp`, `CalcTargetCost`: `0xd4f570`'s cost over the speed). The threat map's look over an army's neighbors (`0x195750`, [country-daily-update.md](../day/country-daily-update.md), "The threat map") asks it for each neighbor, and `0x195b40`, the King's offensive (`0x146750`, [ai.md](../ai/ai.md)) and `0xd511f0` ask it too. The AI's travel estimates `0xd51320` (from `0xdd8f0`, `0xed7f0`, `0x10d600`, `0x10e140` and others) ask `0x1a6780` the same way.

## What it reads and what changes it

`0xd40ae0` and `0xd40790` read the army's leader and its modifiers, its mercenary company's definition (`+0x48`, read from the game's files at load), the army's regiments (their strength, drill and kind, and their companies' sets), the owner's modifiers, the static drill modifier, the defines, and the modifier table and its objects. They write only their stack and heap blocks made and freed in the call: the sets' arrays and the static modifier's name.

What they read changes when a leader is assigned or dies, when regiments are added, lost or reinforced (their strength, every day for an army that reinforces) and drilled (their drill), when the owner's modifiers are made again (its daily update, [country-daily-update.md](../day/country-daily-update.md)), when an army moves into another fleet or out of one, and when a mercenary company is hired, dismissed or split. Each of these is the game's commands or its daily and monthly updates. None runs inside a search: a search works out a route or a score and writes none of them, and an AI country acts through commands executed later ([ai.md](../ai/ai.md)). The route's copy reads the army's route, which changes when the army is given orders or moves on, neither inside a search.

## Ways to make it faster

1. **The army's question about forts once a search.** At the call in the fort zone check (`0xd45291`), answer from the first answer that search got, for the same army on the same thread. A search is a call of the army search `0x1022b0`, of the route query `0x96c520`, or of the threat map's look `0x195750`, from its start to its return on that thread. Outside a search, at the check's other callers (`0xd4a9e0`, `0xd4b270`) and in the power-spending check, the game's code runs. Nothing is kept past the search, so nothing needs letting go. Searches nest (the support check's route queries run inside an army's think), so a nested search keeps its own answer, which is the same.
2. **The route's copy once a search**, by the same scope: the check reads its copy only. The copy is the army's route, the same at every edge of the search.
3. **The army's speed once a look.** At the call in the speed (`0x1a6988`), answer `0xd40790`'s value from the first one of the same search for the same army, the threat map's look and the AI's estimates included. The province's terms stay the game's at every move.

Each is a difference of the second kind under [ADR the-game-may-be-made-faster-with-reasoned-differences](../../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md), the same answers, as long as nothing the question reads changes during one search. On the main thread and in the AI stage's tasks nothing does (above). A checked mode can work out the game's own answer at every move too and count differences, as S10's does.

**Nothing in the AI stage writes what the questions read**, so their answers are the same from the stage's start to its end. Read on 2026-11-11 from the code, over every function the stage runs: the 567 functions seen under a country's task (`0xb0fa0`) on any thread in the sample of [the run](../../research/mechanics-evidence.md), and the 3,899 their direct calls reach. Among them:

- **No writer of what is read.** The functions that write an army's leader (`0xd405b0`, its 35 callers among the leaders' and the commands' code), its route (the route's install `0xd50340` and stop `0xd4fcb0`, [routes.md](routes.md)), its regiments' strength (`0xaddd70`, `0xadde10`, `0xd76700`, combat's and reinforcement's) and drill (`0xd55c20`, from the unit's day `0xd3efd0`), and the country's modifiers (`0x93d130`, called only from `0x327dc0`, [country-daily-update.md](../day/country-daily-update.md)) are none of them reached. Of the writes to the fields the questions read (an army's `+0x30`..`+0x40`, `+0x48`, `+0xd0`, `+0xd8`, `+0x100`..`+0x110`) within the unit's code (`0xd30000`..`0xd80000`, `0x1a0000`..`0x1b0000`), the reached ones are constructors of new objects: the commands the AI posts (`0xd5ce40`, `0xd5ff20`, `0xd615f0`, which fill a new command's fields), streams (`0x1a0f80`, `0x1a14f0`), and a new regiment (`0xadbf90`, from `0x177830`, one the AI makes to weigh, written before anyone else has it).
- **What the AI writes directly** is its own: units handed between agents (`unit+0x168` and the agents' lists, `0xe4990`), the units' AI fields (`unit+0x24a`..`+0x260`), the agents' targets, fleets lent ([ai.md](../ai/ai.md), "What one agent reads of another the same day"); none of them is read by the three questions. Every change of a leader, a regiment or a company is a command, executed on the main thread after the stage ([ai.md](../ai/ai.md)). The write watch of three army thinks found no army, province or country written ([late-game.md](../ai/work-scheduling.md), "What an army's think writes").
- **Not followed**: calls through a vtable that no sample saw, and calls made only on a path too rare to sample. The agents' days and thinks are virtual calls, and are among what the samples saw.

**With S9.** S9 works out a country's armies' searches on the framework's threads while its King's day goes on, and an army's think takes such a search when its arguments and its army are unchanged ([late-game.md](../ai/work-scheduling.md), "The armies' searches on idle threads"). Beside a search run the country's ministers and army agents on the King's thread, and other countries' tasks on the stage's threads; by the above, none of them writes an army's leader, regiments, company, route or its owner's modifiers. So the questions answer the same in S9's searches as at the army's own think, and once a search the same as at every move: **under S9 the ways are exact too, and add no difference to S9's own**, whose differences come from what the questions do not read (units handed between agents, other armies' targets, the strength planned to arrive). The ways shorten S9's searches as much as the game's: fewer of them would be unbegun when their think comes (11 to 14% were), and the searches the thinks still make themselves are shorter.

**The scope.** A search is a call of `0x1022b0`, `0x96c520` or `0x195750` for one army on one thread, its answers kept for that army alone and let go when the call returns. The threat map's look (`0x195750`) runs inside an army's day (`0xd3fd30`, `0x1a2d30`) in each country's daily update, where the armies' days move, reinforce and drill armies one after another: so nothing is kept from one look to the next, or from one army's day to another's. Within one look nothing is written but the list of moves it fills (its entries' `+0x14`, the array grown by `0x1a33310`), which the questions do not read. Nested searches (a think's support check making route queries) each keep their own answers, for their own army.

**Thread safety.** The answers are kept per thread, for the search under way on it. The searches of one country run on several threads at once under S9, so nothing is shared between threads. What is read is what the game's own code reads at every move, without a lock, so a search reading it once reads less than before.

**A wider way, not proposed:** the unit's own set kept for each army until its regiments, leader or owner's modifiers change. It would take the 537 samples of every caller, but a regiment's strength changes every day an army reinforces, so it would have to be let go from many places of the game's code, and no single point tells of all of them.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x1032f0` | function | army route check |
| `0x34af20` | function | Referenced while examining what it costs; complete role not established |
| `0x34db90` | function | Referenced while examining what it costs; complete role not established |
| `0x3c6ee0` | function | Referenced while examining what it costs; complete role not established |
| `0x90eca0` | function | Referenced while examining not known; complete role not established |
| `0x96cf00` | function | Referenced while examining what it costs; complete role not established |
