# How the AI decides

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Read from the game's code on 2026-11-07, with the timings measured in the running game the same day ([prototype-log.md](../prototype-log.md)). [late-game.md](late-game.md) measures what the AI costs late in a game; this file is what it does. "Inferred" marks a reading from the shape of the code that was not followed to the end; the rest is shown by the instructions named.

In short: **each AI country is played by a small tree of agents, a King at the top with seven ministers and one agent per army or fleet, and they decide by scores, mostly every few days each, and act through commands.** The commands run in a later frame, so within a day's AI stage what one agent does reaches another only through a few objects the agents change directly: units handed from one agent to another, fleets lent as transports, the King's orders to its armies, a table of the strength planned to arrive in each province, the war plan, and the war leader's offensive, which its allies follow from other threads. Those are what make the AI's decisions depend on the order its agents run in: within a country on the order of its lists, and between countries on the threads' timing. With `-seed` the game runs the countries one after another on the main thread instead of on the threads.

## Where the AI lives

- **The AI object** of a country is at `country+0x3d0` (0xa58 bytes, constructor `0x9bc20`, made by `0x75d4c0`; also listed at `world+0xb78`). Its `+0x20` is the country, its `+0x28` the King.
- **The strategy** is not an object of its own: it lies inside the country at `country+0x3d8`, with no vtable. It holds the AI personality (`+0x88`: `ai_diplomat`, `ai_militarist`, `ai_capitalist`, `ai_colonialist`, `ai_balanced`), lists and maps of scores per other country (threat `+0xc8`, antagonize `+0xf0`, befriend `+0x118`, protect `+0x140`, vassal `+0x168`, tributary `+0x190`, access `+0x1b8`, force convert `+0x298`, rival `+0x2c0`), and **the war plan** (`+0x214`..`+0x228`, so `country+0x5ec`..`+0x600`: a target country, flags, a state and a date).
- **The budget**: ten shares at `country+0x430`..`+0x454` (savings, colonists, advisors, navies, subsidies, missionaries, armies, states, forts, corruption: the names at `0x1c40cf8`..`0x1c40d68`, looked up by `0x17a3d0`), and four sums of money set aside at `country+0x404`..`+0x410`.

The source file names in the game's strings (`ai.cpp`, `ai_king.cpp`, `ai_army.cpp`, `ai_admiral.cpp`, `ai_foreignminister.cpp`, `ai_army_database.cpp`, `ai_economy.cpp`, `ai_financial.cpp`, `ai_personality.cpp`, `ai_recruit.cpp`, `ai_religion.cpp`, `ai_minister.cpp`, `ai_strategy.cpp`) and the score terms they log (`0x1c3a8c8`..`0x1c43670`) name most of what follows.

## The agents

Every agent derives from one base (vtable `0x1c3afd8`, constructor `0xe4050(agent, country, label)`, name "Agent" at slot `+0x1d8`): `+0x20` the days to its next think, which each think sets itself; `+0x28` the country; `+0x40`/`+0x48`/`+0x50` its children (first node, last node, count; a node is `{agent, previous, next, removed}`, `0xe47a0` appends); `+0x78` a random generator of its own, used only with `-seed`. Slot `+0x58` is its day (`0xe4890`: think when slot `+0x50` allows and `--[+0x20]` reaches 0, then each child's day), `+0x60` its think. Plans derive from `0x1c40038` (constructor `0x15c050(plan, king, label)`), named by their type: slot `+0x340` indexes the 60 names at `0x1c32ff0`, `_CAI_PLAN_GATHERARMIES_` to `_CAI_PLAN_BIG_ARMY_`, which come from an older game and mostly no longer say what the plan does.

The King's start (`0x1484a0`, "CAIKing::Start()") makes seven children, each kept in a field of the King and appended in this order; army and fleet agents and a few plans are made later. Timed for a year from 1780.7.1 in the AI-only game (115 AI countries, about 375 army and fleet agents; the call trace's `times`, which slows what it times, so the shares count more than the milliseconds):

| Agent | Where | Vtable, think | Thinks | Its thinks' time a day, all countries | What it decides |
| --- | --- | --- | --- | --- | --- |
| The King | `[AI + 0x28]` | `0x1c3e9e0`, `0x14acc0`; its day `0x14b370` | every day | 7.1 ms | its armies' bases, areas, targets and offensives; units to agents; economy and buildings to cut; exploration |
| The Admiral | King `+0xc48` | `0x1c3f0e8`, `0xd16f0` | every 1 to 5 days | 5.9 ms | the fleets: home fleet, blockades, merges, repairs, transports |
| The foreign minister | King `+0xa80` | `0x1c3df20`, `0x13d4c0` | every 10 to 24 days | 5.1 ms | diplomacy (at most one action a think), rivals, wars, peace, the war plan |
| Trade (`ai_financial.cpp`) | King `+0xa88` | `0x1c43690`, `0x1b82b0` | every 10 to 24 days | 2.8 ms | merchants, trade fleets, privateers, pirate hunters |
| Colonization (`ai_personality.cpp`) | King `+0xa90` | `0x1c42dc8`, `0x1b4180` | every 1 to 19 days | 1.1 ms | colonist plans (type 19), colonists |
| Spending (`ai_army_database.cpp`) | King `+0xaa8` | `0x1c3d368`, `0x11dc00` | every 1 to 3 days | 3.5 ms | buildings, great projects, trade companies, advisors, loans |
| Religion plan (type 48) | King `+0xa98` | `0x1c40870`, `0x173180` | every 1 to 3 days | 2.3 ms | missionaries, leagues, conversion, the council |
| Recruitment plan (type 7, `ai_recruit.cpp`) | King `+0xaa0` | `0x1c40438`, `0x168070` | every 1 to 3 days | 7.0 ms | regiments and ships to build |
| Army and fleet agents (type 12) | King's list `+0xc50` | `0x1c3cdb8`, `0xe9f30` | every 2 to 4 days | 27.0 ms | one army's or fleet's moves, merges, sieges, attachments |
| Exploration plans (type 46) | King's children | `0x1c3ed80`, `0x1b39a0` | every 8 days | none in the year | moving explorers |

The King's day as a whole, its children's included, was 65 ms a day of all countries' work; the most for one country was Austria's, 6.5 ms a day (its armies 2.8, its recruitment 2.8), then Great Britain's, 4.9 (its Admiral 1.3). The five most costly countries were a third of it.

## A day of the AI

Before the AI stage, on the main thread, each country's daily update (`0x2ee280`) recalculates its strategy when it needs it (`0x182580`: objectives `0x184360`, attitudes `0x184180`; the first time, then once a month on the country's own day when `country+0x2897` is set, and once a year), its budget once a month (`0x122790`) and the money set aside every day (`0x18e7f0`). The stage (`0x75b4b0`) then rebuilds the threat map (`0x1950e0`, `0x242ba00`) and the table of planned strength (`0x197cd0`, `0x2357bb0`), and runs each AI country's task (`0xb0fa0`):

- **Without `-seed`**, sorted by the AI object's `+0x88` (most likely the last task's time), on the parallel stage `0x77d8a0`: each country on a worker thread, the main thread waiting ([late-game.md](late-game.md), "The AI stage").
- **With `-seed`** (`[0x23581c0]`), sorted by the country's index (`0x75ba10`), one after another on the main thread (`0x75b9f8`..`0x75ba1f`).

A country's task steps its King (`0x14b370`), which, in order:

1. Resets its per-area danger (`+0xd20`, below) and checks that the country is the AI's (`0xe6000`).
2. Twice a month, by (day of the month + the country's index) mod 15, recomputes its armies' bases and their agents (`0x152f20`); one day in 300 at random, or when a province of its own changes hands, its armies' areas (`0x162260`).
3. Chooses whose offensive it follows (`0x14dc80`): its own, or in a war it does not lead its side's leader's. Following its own: starts one, one day in 12 at random (`0x146ee0`), or carries the one it has on (`0x147660`).
4. Hands the units waiting on each army agent's list to it (`0x14b6b0`).
5. Thinks (`0x14acc0`, every day), then has each child do its day (the Admiral first, then the foreign minister, trade, colonization, spending, religion, recruitment), then each army and fleet agent (list `+0xc50`, first to last), then brokers six lists of missions between its agents (`0x150740(king, i)`, i = 0..5; what each list stands for was not found).

Every decision becomes a command posted through `0xbfe50` (move, attach, merge, build, propose, declare war, spend), which the game executes in a later frame on the main thread, as it does a player's; so a command changes nothing another agent reads in the same stage.

## The King

- **Bases** (`0x14fb60` from `0x152f20`): a list of provinces, the capital first and then by score, whose supply limits add up to about 1.5 times the country's regiments (King `+0xd38`). Each army agent has one (`+0xaf0`); an agent whose base left the list is moved to the nearest new one without an agent, or stopped; new agents are made for the rest ("Start new agents"), appended at the end of the list, so the agents run in the order they were made.
- **Areas** (`0x162260`): at peace none; at war each army agent gets an AI area of `areas.cpp` (`+0xaf8`), by an exact match when there are few armies and areas (`MAX_ARMIES_NEW_REGION_ASSIGN_ALGORITHM`, `MAX_TASKS_NEW_REGION_ASSIGN_ALGORITHM`), greedily otherwise.
- **The offensive**, a record at King `+0xcc0`: a target province, where the strongest reachable enemy army stands, and a gathering province next to it, a state (gathering, then attacking) and a deadline. `0x145d50` scores a target from the area's danger, the province, the enemy armies there, a random term, and a route query (`0x96c520`) from every army on the country's side of the war, its allies' too, counting those that arrive in time; that is where the King's route queries come from ([late-game.md](late-game.md)). When the armies have gathered, `0x147af0` posts `toggle_can_attach_to` and `attach_unit` to join them, and the record moves to attacking.
- **Whose offensive a country follows** (`0x14dc80`, King `+0xcf8`): in a war it does not lead, its side's leader's record, when that leader is an AI with an offensive under way (or is its overlord). Its armies then read the leader's record: an army whose area is the target's or next to it goes to the gathering province, then to the target (`0xf6f30`).
- **One province target a day** (the think, `0x14d8a0`, `0x14e3d0`): provinces of its wars and of rebels, by priority; the first that an army agent can take gets it (`0x104720`: the agent's target `+0xb00`, and the army unit's `+0x250` written directly), and a province no agent can take waits `INVASION_ARMY_LOOKUP_INTERVAL_ON_FAILURE` days.
- **Units to agents** (`0x148f10`): every unit without an agent goes to one, fleets to the Admiral, armies to the agent based where they stand, else the first; armies too large are split into a new agent ("Unbalanced splinter", `0x154f00`).
- **Per-area danger** (`+0xd20`): the enemy armies of four regiments or more in an area and 0.75 of its neighbors', reset every day and computed when first asked for, by the King's offensive or an army's search ("Too dangerous").
- The think also cuts what the country cannot pay for (`cancel_unit`, `cancel_building`, `mothball_fort`, `destroybuilding`, `disband_unit`), spends monarch power by the personality (`spend_power_command`), assigns leaders, sends explorers, and clears the war plan when its target is gone.

## The army and fleet agents

An agent's think (`0xe9f30` → `0xea250`) sets its next think 2 to 4 days on (every day in some cases: the country's `+0x289b` set, or `0x1a8b00` of its main army). It merges its units standing together (`merge_units`), marks its main army (`toggle_main_army`), hands lent fleets back, and decides for its main army (`0xeab50`) and its other units (`0xf3f80`):

- Hand its main army to a larger army of its country at a meeting province; detach from an army it follows that is no longer a friend; force-march; drill (`drill_army_mission`); retreat; attach to a friendly army in its province (one think in three); explore.
- **The search** (`0x1022b0`), a best-first search from the army's province over its neighbors, scoring each province with `0xf8ea0`, the lowest best. The score's terms are the ai_army.cpp strings, in three branches: "Supply check branch" (base, focal province, terrain, wrong region, over supply limit, enemy and fort borders, attach to a friend, the distance to its base), "Fight branch" (morale, the focal province, wrong region and too far, the balance of strengths `0x10f2b0` against the balance required, enemies to kill, friends asking for help, the war goal, breaking a siege, "Too dangerous") and "Peaceful/Siege branch" (forts, claims, cores, objectives, the war leader, rebels, "Don't walk back and forth between provinces protected by forts", "No more regiments needed for siege"). With a war plan the enemy border scores −25000 instead of +25000. The search runs only on days when (day % 3) equals (the country's index % 3), so all a country's armies search the same days, except the player's country and countries tied to it (at war with it, allied, its overlord or subjects) and in multiplayer; on three days of four it looks only six steps deep.
- **Split off to besiege** a neighboring fort (`0x10abd0`), unless friendly strength is already planned to arrive there; and **move** (`move_order`), or cancel a move that leads into a battle not worth fighting (`cancel_movement`).
- With a target over the sea (`+0xb00`), ask for transports and sail (`0xf1390`).

## The Admiral

It keeps the country's fleets in a list (`+0xa50`), the first its home fleet, and thinks every 1 to 5 days (30 when the country has no port or no ships). It keeps the heavy ships and galleys in the home fleet and splits the light ships and transports off (`create_unit`, `transfer_subunit`); gives a fleet a blockade target when it is stronger than 1.2 times the enemies' navies (`fleet+0x27c`); merges, repairs and disbands fleets; raids for slaves; moves fleets to the best-scoring sea (`0xd63e0`, the ai_admiral.cpp strings). It reads each army agent's wish to be shipped (`+0xb00`): while any army wants transport, it hunts nothing, and not for six months after the last request. Army agents take fleets from its list during their own think (`0xdc8a0`), and the trade agent takes fleets for trade and privateering (`0xdb950`), both directly, not by command.

## The foreign minister

Every 10 to 24 days it goes through every country it knows (`0x13b810`): it scores a war against each while it has no war plan (war goal, overextension, stability, mercenaries, regiments, loans, manpower, transports, war exhaustion, revolts, coalitions, the Empire, an easy war, antagonism; `0x12e160`, `0x131340`); considers peace and calls to arms; and of about 45 diplomatic actions, each scored by its own AI method and checked (`0x129b60`: its score, its own conditions, the other side's acceptance, a random bonus), sends at most the best one (`diplomaticactioncommand`). It keeps the best war target and, with no plan, writes **the war plan** (`0x18fa00`, which also posts `announced_war`); three months on, once the armies are ready (`0x18e1c0`: their strength and morale at 90%, the force limit), it threatens or declares war; it drops the plan after 12 months, or 30 when a claim must be fabricated first. It also manages rivals, ends relations whose score has fallen, and turns favors into trust.

## The economy

- **Money**: the budget's shares are set once a month and the money set aside every day, both before the stage (above); every purchase is a `spend_money` command, which takes the money only when it runs in a later frame.
- **Recruitment** (the plan of `ai_recruit.cpp`) builds regiments and ships from the money set aside for them, or from the whole treasury while the offensive it follows is under way; it reads that offensive, and the King's `0x146ee0` asks it before starting one. **Spending** (`ai_army_database.cpp`) builds buildings (about once in 28 days), great projects, trade companies, hires advisors, takes loans ("Scripted factor", "Calculated value", "Build cost"). At war both may count the same ducats on the same day (inferred).
- **Trade** sends and steers merchants, sets trade fleets, privateers and pirate hunters. **Colonization** makes colonist plans and sends colonists. **Religion** sends missionaries and handles leagues, conversion and the council. Exploration plans move explorers.
- `ai.cpp`'s country-wide choices: privileges granted and revoked every 64 days (`0xa00e0`), naval doctrine (`0xa3620`), reforms (`0xa44f0`), development (`0xbe370`), each by command.

## What one agent reads of another the same day

Within a country, in the order the King walks them (the King, its children in the order above, its army agents first to last):

| What | Written by, at once | Read by | So |
| --- | --- | --- | --- |
| Units moving between agents (`0xe4990`: the unit's `+0x168` and both agents' lists) | an army handing its main army to a larger one; splits; armies giving units to the King; fleets lent and given back | the agent that holds them, at its next think | an agent that receives a unit later the same day decides with it today; earlier, at its next think |
| Fleets lent as transports (`0xdc8a0`) and fleets taken for trade (`0xdb950`) | army agents, the trade agent, taking from the Admiral's list | the next to ask | the first to ask gets them |
| The armies' targets (`+0xb00`) | the King (`0x104720`), army agents (`0xf1390`) | `0x14f510`, whether a siege already has enough regiments; the King's `0x14e3d0`; the Admiral | an army sees the targets chosen today by the armies before it and yesterday by those after it |
| The war plan (`country+0x5ec`) | the foreign minister; cleared by the King | recruitment, spending, trade; army agents' scores | those after the minister see a new or dropped plan today, those before it tomorrow |
| The units' AI fields (`unit+0x24a`..`+0x260`) | the King, army agents | army agents | written directly, read by whoever thinks next |

Between countries, during the parallel stage, on other threads:

| What | Written by | Read by | So |
| --- | --- | --- | --- |
| **The planned-strength table** `0x2357bb0`: per province, the regiments heading there, its side's and others' (four vectors), rebuilt before the stage | every move and cancel of every army (`0x1980c0`, `lock xadd`) | `0x10a9c0`: a fort with friendly strength planned is not chosen for a siege (`0x197f50`) | the first army to think, of any country, takes the fort; between threads a race |
| **The war leader's offensive** (King `+0xcc0`) | the leader's King (`0x146ee0`, `0x147660`, `0x147af0`), under the lock `[0x2357ba0]` | its allies' armies (`0xf6f30`, through `+0xcf8`) and recruitment, under the lock, one field at a time | an ally sees today's or yesterday's offensive by the threads' timing, and can read a record half old and half new |
| The world's random generator (`world+0xb90`) | every agent's draw, without `-seed` | every agent | which numbers a country gets depends on the threads' timing |
| The ids of new agents, plans and fleets (`[0x2340154]`, spinlock `[0x23587e0]`) | every country's task | the ids in `create_unit` and the agents | the ids depend on the threads' timing (inferred: it may be what still varies with `-seed`, but with `-seed` the stage runs on one thread) |

Not order-dependent within a day: the search itself (it writes only its stack), the threat map (rebuilt before the stage, read-only in it, inferred), the per-area danger (computed from what commands change only after the stage, inferred), the strategy (recalculated before the stage; in it only the war plan is written), and everything done by command.

The one lock `[0x2357ba0]` guards the offensive records, because a leader's King writes its record while its allies read it on other threads; the army search reads the record it follows two to five times for every province it scores ([late-game.md](late-game.md), "One lock for every country").

## What this means for running the AI apart

- The game already runs its countries apart, on the threads, and accepts what that changes: the shared generator's draws, the race for forts in the planned-strength table, a war leader's offensive seen a day early or late. A game loaded twice is not the same game ([late-game.md](late-game.md), "Whether a game repeats its days"), and the developers keep `-seed` for one that is: with it the countries run one after another.
- A country's agents running apart would add the same kind of change within a country, through the same objects: whose target a siege counts, who gets the transports, which army receives a unit. But two things the game's own threads do not do: the units moving between agents' lists (`0xe4990`) and the fleets taken from the Admiral's list are written with no lock, as the code of one thread; two agents of one country on two threads could change the same list at once, which is a crash, not another decision. And each agent was written to see the others' decisions of the day in its list's order; run apart, two armies could each miss the other's siege or transport, which no order of the game's gives.
- So, of the agents of a country, the ones that touch none of those objects could run apart with no more change than the game's threads already make: the foreign minister (it writes only the war plan and its own fields, and reads the army agents' targets as they left them), spending, religion and colonization (they read the war plan, and act only by command), and the search inside an army's think (stack only). Each would then see the war plan, or the armies' targets, of today or of yesterday by the threads' timing. The army agents, the Admiral, trade and recruitment share units, fleets, targets and the offensive. Which of the game's decisions that would change, and whether the AI would play any worse, only a bench that compares runs could say ([game-speed.md](../game-speed.md), S5).

## Not verified

- All of the above is read from the code but the timings; none of the order dependencies was seen in the game, apart from the experiment in [late-game.md](late-game.md) that reversed a country's agents' order. Which of the paths above made that experiment's difference was not followed.
- That the war leader's record and the planned-strength table make the parallel stage depend on the threads' timing is inferred from the code; that the ids from `[0x2340154]` vary was not checked, and what varied with `-seed` (the Royal Navy's admiral, assigned in `0x168db0` or `0x149880`, inferred) was not traced.
- Not followed: what the six mission lists stand for; the four functions that reshape the King's armies (`0x155420`, `0x154620`, `0x154140`, `0x155f10`); the area assignment's scores; the weights of the army and fleet scores term by term; the strategy's score formulas; who reads the fourth sum of money set aside (`country+0x410`); several country fields (`+0x289b`, `+0x2864`, `+0x2883`). Virtual calls were not all followed, so a write behind one may have been missed.
- The timings are one year of one AI-only game from 1780, with the call trace's own cost in them.
