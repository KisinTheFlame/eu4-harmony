# The AI

Every country the player does not play is played by the AI: it moves the armies and fleets, chooses wars and alliances, spends the money and the monarch power, sends merchants, colonists and missionaries. Each AI country is a small tree of agents, a King at the top with seven ministers under it and an agent for each army or fleet, which decide by scores, most of them every few days, and act through the same commands the player's interface posts. The AI runs in one stage of the day, every AI country at once on the worker threads, and late in a game it is what makes the days slow ([research/ai.md](../research/ai.md)).

## Where it runs

- **Only on the host**, or in single player, and only while the world lets the AI run; it acts only by posting ordinary commands, so only the commands, not the AI, have to come out the same on every machine of a multiplayer game ([research/architecture.md](../research/architecture.md), "The AI").
- **Before the AI stage**, in each country's daily update on the main thread ([day.md](day.md), "A day, in order"): the country's strategy recalculated when it needs it (its objectives and attitudes, the first time, then once a month on the country's own day, and once a year), its budget's shares once a month, and the money set aside for the day ([research/ai.md](../research/ai.md), "A day of the AI").
- **The stage** (`0x75b4b0`) rebuilds the threat map and the table of strength planned to arrive in each province, then runs one task for each AI country on the worker threads, the countries that took longest first (sorted, inferred, by their last task's time); the main thread takes tasks too, then waits until the last one is done ([research/ai.md](../research/ai.md), "A day of the AI"; [research/late-game.md](../research/late-game.md), "The AI stage").
- **With `-seed`** the stage runs the countries one after another on the main thread, by their index, instead ([research/ai.md](../research/ai.md), "A day of the AI").
- **The player's own country has no AI**; in [observe mode](../plans/observe-mode.md) the AI plays every country, the country in view too ([research/late-game.md](../research/late-game.md), "What this means").

## A country's AI

- **The strategy** lies inside the country: its AI personality (diplomat, militarist, capitalist, colonialist, balanced), its scores for every other country (threat, antagonize, befriend, protect, vassal, tributary, access, force convert, rival) and **the war plan**: a target country, a state and a date ([research/ai.md](../research/ai.md), "Where the AI lives").
- **The budget**: ten shares (savings, colonists, advisors, navies, subsidies, missionaries, armies, states, forts, corruption) and four sums of money set aside ([research/ai.md](../research/ai.md), "Where the AI lives").
- The game's source file names and the score terms it logs name most of what the AI weighs ([research/ai.md](../research/ai.md), "Where the AI lives").

## The agents

An agent counts the days to its next think, which each think sets itself; on the day it reaches 0 the agent thinks, then has each of its own agents do their day ([research/ai.md](../research/ai.md), "The agents"). So on one day few of a country's agents think and on another many, which is why a country's AI costs so much more some days than others ([research/late-game.md](../research/late-game.md), "An AI country's agents").

| Agent | Thinks | What it decides |
| --- | --- | --- |
| The King | every day | its armies' bases, areas, targets and offensives; which agent leads which unit; what the country cannot pay for; exploration |
| The Admiral | every 1 to 5 days | the fleets: the home fleet, blockades, merges, repairs, transports |
| The foreign minister | every 10 to 24 days | diplomacy, at most one action a think; rivals, wars, peace, the war plan |
| Trade | every 10 to 24 days | merchants, trade fleets, privateers, pirate hunters |
| Colonization | every 1 to 19 days | colonists |
| Spending | every 1 to 3 days | buildings, great projects, trade companies, advisors, loans |
| Religion | every 1 to 3 days | missionaries, leagues, conversion, the council |
| Recruitment | every 1 to 3 days | regiments and ships to build |
| An army's or a fleet's agent | every 2 to 4 days | one army's or fleet's moves, merges, sieges and attachments |

From [research/ai.md](../research/ai.md), "The agents"; the plans' names the game keeps come from an older game and mostly no longer say what a plan does.

## A King's day

A country's task steps its King, which in order ([research/ai.md](../research/ai.md), "A day of the AI"):

1. Resets its danger of each area, and checks that the country is still the AI's.
2. Twice a month recomputes its armies' bases and their agents; one day in 300, or when a province of its own changes hands, its armies' areas.
3. Chooses whose offensive it follows: its own, or in a war it does not lead, its side's leader's. Following its own, it starts one, one day in 12 at random, or carries on the one under way.
4. Hands the units waiting for each army's agent to it.
5. Thinks, then has each minister do its day (the Admiral first, then the foreign minister, trade, colonization, spending, religion, recruitment), then each army's and fleet's agent in the order they were made, then brokers missions between its agents.

Every decision becomes a command (move, attach, merge, build, propose, declare war, spend), which the game executes in a later frame on the main thread, as it does a player's ([research/ai.md](../research/ai.md), "A day of the AI").

**What the King decides** ([research/ai.md](../research/ai.md), "The King"):

- **Bases**: provinces, the capital first and then by score, whose supply limits add up to about one and a half times the country's regiments; each army's agent has one.
- **Areas**: at war each army's agent is given an area to fight in; at peace none.
- **The offensive**: a target province, where the strongest enemy army it can reach stands, a gathering province next to it, and a deadline. Choosing a target asks a route from every army of its side of the war, its allies' too, to count those that arrive in time. Once the armies have gathered, it joins them and attacks.
- **Following the war leader**: in a war it does not lead, its armies near the leader's target go to the leader's gathering province, then to the target.
- **One province target a day**, from its wars and rebels by priority, to the first army that can take it.
- **Units to agents**: every unit without an agent gets one, fleets to the Admiral; armies too large are split.
- It also cuts what the country cannot pay for (units, buildings, forts), spends monarch power by the personality, assigns leaders and sends explorers.

## Armies and fleets

- **An army's agent** merges its units standing together, marks its main army, hands lent fleets back, and decides for its main army and its other units: hand the main army to a larger one, follow or leave a friendly army, force-march, drill, retreat, attach, explore, split off to besiege a neighboring fort, move, or call off a move into a battle not worth fighting; with a target over the sea it asks for transports and sails ([research/ai.md](../research/ai.md), "The army and fleet agents"). Armies are in [military.md](military.md).
- **Where to go** is a search out from the army's province over its neighbors, best first, each province scored in one of three ways: for supply (its base, terrain, the supply limit, borders), for a fight (morale, the balance of strengths against the balance required, enemies to kill, friends asking for help, the war goal, "Too dangerous"), or for a siege at peace (forts, claims, cores, objectives, rebels). A country's armies search only one day in three, all on the same days, except the player's country and those tied to it, and on three days of four only six steps deep ([research/ai.md](../research/ai.md), "The army and fleet agents"). Routes are in [military.md](military.md).
- **Whether it would be outnumbered**: an army near enemies asks whether the friendly strength that can reach its province in time outnumbers the enemy strength that can, with a route asked for every unit of either side close enough; a no is held for seven thinks, and what it makes the agent do was not followed ([research/ai-stage-wait.md](../research/ai-stage-wait.md), "The support check").
- **The Admiral** keeps the heavy ships and galleys in the home fleet and splits light ships and transports off, sends a fleet to blockade when it is stronger than the enemies' navies, merges, repairs and disbands fleets, and moves them to the best seas; while any army wants transport it hunts nothing ([research/ai.md](../research/ai.md), "The Admiral").

## Diplomacy and war

The foreign minister goes through every country it knows every 10 to 24 days. While it has no war plan it scores a war against each (the war goal, overextension, stability, regiments, loans, manpower, war exhaustion, revolts, coalitions, the Empire, antagonism); it considers peace and calls to arms; of about 45 diplomatic actions, each scored and checked against its conditions and the other side's acceptance, it sends at most the best one. It writes the war plan for the best target and, three months on, once its armies are ready, threatens or declares war; it drops the plan after 12 months, or 30 when a claim must be fabricated first ([research/ai.md](../research/ai.md), "The foreign minister"). Diplomacy is in [diplomacy.md](diplomacy.md).

## The economy

- **Money** is spent by command, so it is taken only when the command runs in a later frame; recruitment and spending may count the same ducats on the same day at war (inferred) ([research/ai.md](../research/ai.md), "The economy").
- **Recruitment** builds from the money set aside for it, or from the whole treasury while its offensive is under way; **spending** builds buildings, great projects and trade companies, hires advisors and takes loans; **trade**, **colonization** and **religion** send merchants, colonists and missionaries ([research/ai.md](../research/ai.md), "The economy"). The religion plan asks the country's tolerance of the religion in each of its provinces at every think, every 1 to 3 days, and again for the provinces it weighs for missionaries ([research/tolerance-in-a-province.md](../research/tolerance-in-a-province.md#who-asks); [countries.md](countries.md)).
- **Country-wide choices**, by command: privileges granted and revoked every 64 days, the naval doctrine, reforms, development ([research/ai.md](../research/ai.md), "The economy"). Countries are in [countries.md](countries.md), provinces and trade in [provinces.md](provinces.md).

## What one agent sees of another

Since everything is done by command and the commands run later, what one agent does reaches another the same day only through the few objects agents change directly ([research/ai.md](../research/ai.md), "What one agent reads of another the same day"):

- **Within a country**, in the order the King walks them: units handed from one agent to another, fleets lent as transports or taken for trade, the armies' targets that a siege counts, the war plan, the units' AI fields, and the country's cache of where its units may go.
- **Between countries**, on other threads: the table of planned strength, where the first army of any country to think takes a fort; a war leader's offensive, which its allies read under the one lock the AI shares; the world's random generator; the ids of new agents and fleets.

So **the AI depends on the order its agents run in**: walked in reverse, a country's agents took other decisions within a month, about 7,500 lines of a save changed against 2, in armies, trade, provinces and relations ([research/late-game.md](../research/late-game.md), "Whether an AI country's agents depend on each other"). The game already accepts what its threads change between countries: the generator's draws, the race for forts, an offensive seen a day early or late ([research/ai.md](../research/ai.md), "What this means for running the AI apart"). Within a country, units and fleets move between agents' lists with no lock, written as the code of one thread ([research/ai.md](../research/ai.md), "What this means for running the AI apart").

## What it costs

- **It grows with the armies.** In a game the AI played alone, the AI stage went from about 5 ms of the main thread a game day to 30, as the world's regiments tripled after 1600 and gathered in a few great powers, the Ottomans' from 249 to 1,575 ([research/late-game.md](../research/late-game.md), "The days, twenty-five years at a time", "What the world gathers").
- **A stage lasts as long as its longest task**, a great power's (Austria's, Great Britain's, the Ottomans', the Commonwealth's), which starts among the first and still ends last, while the other workers sleep; the main thread spins meanwhile ([research/late-game.md](../research/late-game.md), "Each AI country's task, timed"; [research/ai-stage-wait.md](../research/ai-stage-wait.md), "Whose task the stage waits for"). More cores would not help.
- **Half of that wait is the armies' searches**, and on the slowest days a great power's King planning its offensive, asking route after route ([research/late-game.md](../research/late-game.md), "What the main thread waits on in the AI stage", "Where an agent's time goes"). Routes asked again the same day are few ([research/late-game.md](../research/late-game.md), "Where an agent's time goes").
- **The stages have a long tail**: most take about 10 ms, one in a hundred over 75, the longest seen nearly half a second, one country's task alone; such a day is a stutter at speed 5 ([research/late-game.md](../research/late-game.md), "Each AI country's task, timed").
- **Little of it can be made faster with the same decisions**: the order of the tasks gives nothing, and running a country's agents apart would change what they decide ([research/late-game.md](../research/late-game.md), "Whether it can be made faster"; [research/ai-stage-wait.md](../research/ai-stage-wait.md), "What this means for making the game faster").

## What the framework relies on

- **The armies' searches on idle threads**: once a King has thought, the framework works out the searches of the armies that think that day on threads of its own, and a think takes the one made for it when nothing it depends on changed (S9, [research/late-game.md](../research/late-game.md), "The armies' searches on idle threads"; [plans/game-speed.md](../plans/game-speed.md); [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md)).
- **The AI countries' random draws as a repair**: each country's task draws from a stream of its own instead of racing on the world's generator ([research/random.md](../research/random.md), "A stream for each task"; [ADR random-draws-may-change-hands](../adr/2026-11-08-random-draws-may-change-hands.md)).
- **Observe mode**: what acts as a country acts as the country in view while the AI goes on playing it ([ADR observe-mode-acts-as-the-country-in-view](../adr/2026-11-07-observe-mode-acts-as-the-country-in-view.md)).
- The developer build's test switch that walks a country's agents in reverse, and the call trace that times each country's task ([research/late-game.md](../research/late-game.md), "Whether an AI country's agents depend on each other"; [dev-tools.md](../dev-tools.md)).

## What is not known yet

- Which of the paths by which an agent reads what the agents before it did that day made the reversed order's difference; what the King's offensive planning decides, and whether its routes depend on each other ([research/late-game.md](../research/late-game.md), "Not verified").
- What the six lists of missions the King brokers stand for; the four ways it reshapes its armies; the areas' scores; the army and fleet scores term by term; the strategy's formulas ([research/ai.md](../research/ai.md), "Not verified").
- What the threat map is to the AI, and why an army's day updates it ([research/country-daily-update.md](../research/country-daily-update.md), "Not verified").
- What an army's agent does differently after the outnumbering check says no, and that the third list the outnumbering check counts is the country's enemies ([research/ai-stage-wait.md](../research/ai-stage-wait.md), "Not verified").
- What the transports' routes ask ([research/ai-stage-wait.md](../research/ai-stage-wait.md), "What this means for making the game faster").
- What picks the admiral of Great Britain's navy that varies under `-seed` ([research/late-game.md](../research/late-game.md), "Whether a game repeats its days").
- None of the order dependencies within a day was seen in the game but the reversed walk; virtual calls were not all followed, and the timings are of one AI-only game in observe mode ([research/ai.md](../research/ai.md), "Not verified").
