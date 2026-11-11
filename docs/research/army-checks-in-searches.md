# What an army is asked again at every move of a search (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-11 as the third step of [game-speed.md](../plans/game-speed.md), "How a part is chosen", for a candidate of that plan, not built: two things a route search asks of the army at every move it weighs, each time with the same answer, the fort zone's first question and the army's movement speed. It bears on S9, the armies' searches on idle threads, which runs the same searches on the framework's threads. From the game's code (`cargo xtask disasm`, and a script over the image for the direct calls), the modifiers' names from the place where the game makes its modifier table (`0x8d84a0`, [tolerance-in-a-province.md](tolerance-in-a-province.md)), and a sample of every thread taken on 2026-11-11 with `cargo xtask profile --threads` on the 1780.7.1 save in observe mode at speed 5: 180 s, 1,529 days, 8.5 days a second, 117.7 ms of the main thread a day. The run's conditions (the build, another test game running beside it) are in [the run](../prototype-log/2026-11-11-five-candidates-timed.md). That pace is slower than the 14.5 days a second of [game-speed.md](../plans/game-speed.md) ("Which part comes next"), so the shares below matter more than the milliseconds.

In short: the route edge check (`0x96f860`, [routes.md](routes.md)) asks the fort zone check `0xd45250` at every edge. That check first asks **whether the army may pass forts** (`0xd40ae0`(army, `0xe0` `MODIFIER_CAN_BYPASS_FORTS`, 0)). To answer it, the game looks at the army's leader, its owner's modifiers and a set the army points at, and **builds the army's own set of modifiers afresh**: it walks every regiment for the army's drill, makes a set of its own for each regiment that may count toward drill, and scales the drill modifier into a new set. The answer is the army's alone, the same at every edge of one search, and it is **44% of the edge check's time**: 369 of 835 samples of every thread, 2.0% of the whole game's CPU. It is also 1.0 ms a day of what the main thread waited on in the AI stage, and 0.76 ms of the main thread's own. Worked out once for each search, it gives the same answers. So does the copy of the army's route that the same check makes at every edge when told to read the route (60 samples more). In the same way, the travel time a threat map look or an AI estimate works out for each neighbor asks the army's movement speed (`0xd40790`(army, `0xfb` `MODIFIER_MOVEMENT_SPEED`)), which builds the same drill set again: 58 samples of the main thread, 0.9 ms a day.

## The fort zone's first question

`0xd45250`(army, candidate, current, read the route) (checked: 4 arguments, [routes.md](routes.md), "Route history") answers whether a move from `current` to `candidate` is blocked by a fort's zone of control. Before anything else it answers no for an army of the special country `[0x233d8a4]` (`army+0xd4`), and no when `0xd40ae0`(army, `0xe0`, 0) says the army may pass forts. Only then does it look at the two provinces (land, `+0x228` bit `0x08`), the army's type and exile (`0x99ca0`), the provinces it came from (`+0xf0`, `+0xf8`), its route (`+0x100`; with the fourth argument set, a copy of it made with `0x1beba0` and freed at the end), the forts of the provinces (`0xa273a0`, `0xa275d0`), their owners and controllers, the countries' relations, and the candidate's neighbors.

`0xd40ae0`(unit, modifier, by military tag) (checked: 4 arguments) answers whether a modifier that is on or off is on for the unit. Each test is `0x8e7b60`(set, modifier):

- the leader (`unit+0xd8`, when its method `+0x50` answers yes) and its modifiers at `+0x68`;
- the set that `0x900070` gives for the unit's `+0x48`;
- **the unit's own set**, `0xd4c6e0`(unit, modifier, null), which makes a set on its stack (vtable `0x1c42770`), has `0xd4ca70` fill it, reads the modifier from it, and lets it go;
- its owner's modifiers (`0x93d750(country+0x28c0)+8`, the country by the unit's tag at `+0xd0`, or by its military tag, the method `+0x138`, when the third argument is set);
- for a fleet (the method `+0x88`), each of the units its method `+0x70` lists, through a functor.

The answers are or-ed. Its only other direct caller is the power-spending check `0x9526d0` ([power-spending.md](power-spending.md)).

**The unit's own set** (`0xd4ca70`(unit, set)): for each regiment (`unit+0x30`..`+0x38`) it adds the regiment's strength (`+0x28`) to a sum. For a regiment that `0xadcb50` lets count, it also adds the strength times the regiment's drill (`+0x30`). `0xadcb50`(regiment) answers yes when the regiment's method `+0x60` answers yes, the expansion check of `[0x233ff24]` passes, and the regiment is of the kind at `+0x60` = 1. For that it makes another set on its stack, adds to it the set of the regiment's `+0x40` → `+0x48` (`0x8ea420`), and tests `0x21f` `MODIFIER_ALLOW_MERCENARY_DRILL` in that set and in the owner's modifiers. _Inferred_: a mercenary regiment counts toward drill only when its company or its country allows mercenary drill. When some regiment counted, `0xd4ca70` copies the static modifier at `0x8ecec0()+0xf30` into the set (`0x8ea2d0`, its name copied with `0x95110` and its pairs) and scales it (`0x8ea920`) by the army's drill over the define at `[0x233e530]`. _Inferred_ from what it reads: the army drill modifier, scaled by the army's average drill. So the unit's own set holds the drill's modifiers, and the question about passing forts is asked of it like any other.

## The movement speed

The army class's method `+0x110` (`0x1a6780` → `0x1a6800`, `army.cpp`, its log line `vSpeed, %d`) works out an army's speed into a province. It starts from the define `[0x233fddc]` and adds:

- `0xd40790`(army, out, `0xfb`, …): the leader (`0xd40710`), the set of `+0x48`, the unit's own set again (`0xd4c390` → `0xd4ca70`), and the owner's modifiers (`0x93d750`);
- a term for the leader's maneuver (`army+0xd8`, its `+0x48`);
- the defines for a retreating (`+0x204`) or forced (`+0x201`) march;
- the province's local friendly or hostile movement speed (`0x8f`, `0x90`, by whether the province is friendly, `0x368560`), read from `province+0x998` and passed through the modifier table's object for it.

Only the province's terms depend on where the army goes; `0xd40790`'s value is the army's.

It is asked through the travel time `0xd4f4a0`(army, out, from, to) (`unit.cpp`, `CalcTargetCost`: `0xd4f570`'s cost over the speed). The threat map's look over an army's neighbors (`0x195750`, [country-daily-update.md](country-daily-update.md), "The threat map") asks it for each neighbor, and `0x195b40`, the King's offensive (`0x146750`, [ai.md](ai.md)) and `0xd511f0` ask it too. The AI's travel estimates `0xd51320` (from `0xdd8f0`, `0xed7f0`, `0x10d600`, `0x10e140` and others) ask `0x1a6780` the same way.

## What it costs

In the sample above:

| | Every thread (samples) | The main thread's own | The main thread's wait in the AI stage |
| --- | --- | --- | --- |
| The edge check `0x96f860` | 835 | 167 | 2.36 ms a day |
| the fort zone check `0xd45250` | 578 | 97 | 1.64 ms a day |
| whether the army may pass forts `0xd40ae0` | 369 | 50 (0.76 ms a day) | 1.02 ms a day |
| of it, the unit's own set (`0xd4c6e0` → `0xd4ca70`) | 293 | | |
| the route's copy (`0x1beba0`) | 60 | 17 | |
| the speed `0x1a6800` | 142 | 82 | 0.19 ms a day |
| of it, `0xd40790` | 106 | 58 (0.88 ms a day) | |

- **Where the fort zone check is asked**, of its 578 samples: the army agent's search (`0xeab50` → `0x1022b0`) 193, and 14 more in S9's searches on the framework's threads. Route queries about 245: the support check `0x106320` (from the think and from `0xf4490`), `0x1032f0` and the rebel hunt's check `0x970f60` among them. The threat map's look (`0x195750`, from the army's day `0xd3fd30` and `0x1a2d30`) 91, and `0x96cf00` 26. Of the 1.02 ms a day of the main thread's wait on `0xd40ae0`, the army search's was 0.51, the support check's route queries 0.28, and the rest (`0x1032f0`, the rebel hunt's check, `0x96cf00`, S9's threads) 0.23.
- **The unit's own set** is built for any modifier asked of an army: `0xd4ca70` has 537 samples in all, 3.0% of the game's CPU, from the fort question (293), the speed (86), the army strength `0xd46800` (23), and through `0xae7230` from the daily updates of provinces and units (`0x34af20`, `0x34db90`, `0x3c6ee0`, 100 or so). Inside it: its own loop 120, the regiments' mercenary test 120 (43 in that test's `0x8ea420`), the static modifier's copy 138 (117 of it the name's copy, `0x95110`).
- **What the ways could give**, at most: the fort question's 0.76 ms a day of the main thread's own and 1.02 ms of its wait in the AI stage, the speed's 0.88 ms and 0.19 ms of the wait, and the route's copy's 17 samples (about 0.26 ms a day); about 3 ms of a 117.7 ms day at that run's pace, 2.6%. The main thread's own share is the threat map's looks and the route queries made between days; the wait's is what S9 leaves on the day's path.
- How many edge checks one search makes was not counted. The edge check was called 86 to 138 million times in a few minutes of a traced run ([Inside an AI country's task](../prototype-log/2026-11-07-inside-ai-country-task.md)), many per search, so a search's first question is nearly all of it.

## What it reads and what changes it

`0xd40ae0` and `0xd40790` read the army's leader and its modifiers, the set of `+0x48`, the army's regiments (their strength, drill and kind, and their companies' sets), the owner's modifiers, the static drill modifier, the defines, and the modifier table and its objects. They write only their stack and heap blocks made and freed in the call: the sets' arrays and the static modifier's name.

What they read changes when a leader is assigned or dies, when regiments are added, lost or reinforced (their strength, every day for an army that reinforces) and drilled (their drill), when the owner's modifiers are made again (its daily update, [country-daily-update.md](country-daily-update.md)), and when an army moves into another fleet or out of one. Each of these is the game's commands or its daily and monthly updates. None runs inside a search: a search works out a route or a score and writes none of them, and an AI country acts through commands executed later ([ai.md](ai.md)). The route's copy reads the army's route, which changes when the army is given orders or moves on, neither inside a search.

## Ways to make it faster

1. **The army's question about forts once a search.** At the call in the fort zone check (`0xd45291`), answer from the first answer that search got, for the same army on the same thread. A search is a call of the army search `0x1022b0`, of the route query `0x96c520`, or of the threat map's look `0x195750`, from its start to its return on that thread. Outside a search, at the check's other callers (`0xd4a9e0`, `0xd4b270`) and in the power-spending check, the game's code runs. Nothing is kept past the search, so nothing needs letting go. Searches nest (the support check's route queries run inside an army's think), so a nested search keeps its own answer, which is the same.
2. **The route's copy once a search**, by the same scope: the check reads its copy only. The copy is the army's route, the same at every edge of the search.
3. **The army's speed once a look.** At the call in the speed (`0x1a6988`), answer `0xd40790`'s value from the first one of the same search for the same army, the threat map's look and the AI's estimates included. The province's terms stay the game's at every move.

Each is a difference of the second kind under [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md), the same answers, as long as nothing the question reads changes during one search. On the main thread and in the AI stage's tasks nothing does (above). A checked mode can work out the game's own answer at every move too and count differences, as S10's does.

**With S9.** S9 works out a country's armies' searches on the framework's threads while its King's day goes on, and an army's think takes such a search when its arguments and its army are unchanged ([late-game.md](late-game.md), "The armies' searches on idle threads"). There the King's day runs beside the search, so what the question reads could change during it only if the King's day changes the army's leader, regiments or the owner's modifiers. It posts commands for those; whether it writes any of them itself was not read. A search's first answer would then differ from a later move's, a difference of the first kind that S9 already allows and counts by the whole search's result. The ways shorten S9's own searches as much as the game's: fewer of them would be unbegun when their think comes (11 to 14% were), and the searches the thinks still make themselves are shorter.

**Thread safety.** The answers are kept per thread, for the search under way on it. The searches of one country run on several threads at once under S9, so nothing is shared between threads. What is read is what the game's own code reads at every move, without a lock, so a search reading it once reads less than before.

**A wider way, not proposed:** the unit's own set kept for each army until its regiments, leader or owner's modifiers change. It would take the 537 samples of every caller, but a regiment's strength changes every day an army reinforces, so it would have to be let go from many places of the game's code, and no single point tells of all of them.

## Calls to time, and what to sample instead

The functions here are too hot for the call trace's `times` ([dev-tools.md](../dev-tools.md), "Call tracing"). The edge check `0x96f860` ran 138 million times in about four minutes of a traced run, the fort zone check, `0xd40ae0`, the speed `0x1a6800` and `0xd40790` as often or more, and tracing the army search `0x1022b0` crashed the game. Sampled instead, with `cargo xtask profile --threads --waiting-in 0x179e90 --slice 60` on a late save, the same table as above with the parts left to the game and with a prototype on:

- the inclusive samples of `0xd40ae0`, `0xd4c6e0`, `0x1beba0` under `0xd45250`, and `0xd40790` under `0x1a6800`, which the ways take away;
- the main thread's wait in the AI stage on `0x1022b0`, `0x96c520` and `0xd45250`, which is what S9 and the ways shorten on the day's path.

One of the searches is called seldom enough to time, which counts them: the route query, `"0x96c520:8"`, as before ([the AI stage's wait, measured](../prototype-log/2026-11-08-ai-stage-wait-measured.md), 36,049 to 411,719 calls over its runs). The threat map's look `0x195750` (6 arguments) is called for each army every day, too often to time over a long run. A prototype can count its own answers (moves asked against first answers) instead.

## Not known

- How many edge checks one search makes, by kind of search.
- What the set at `unit+0x48` is (`0x900070`).
- Whether the King's day writes any of what the question reads while S9's searches run beside it.
- `0xd45250`'s other callers, `0xd4a9e0` and `0xd4b270`, and what they are for.
