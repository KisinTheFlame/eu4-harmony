# The day and its time

The game's time moves a day at a time. At each of its five speeds the game waits a while between days, then works the whole day out: every country, province, army and war a day further, the AI's decisions, a month's and a year's business when they turn. All of it runs on one thread, the main thread, which also takes the player's input and draws the screen, so the day and the frames take turns, and the frames that come inside a day are drawn between its phases. How fast the game plays late in a game is how long that one thread takes for a day.

## Speed and the days

- **The speed** is one of five, kept as 0 to 4 in the world and shown as speeds 1 to 5; the game's speed-up and slow-down commands move it one step at a time ([research/time.md](../research/time.md), "The speed").
- **A day is a command.** Each in-game frame first executes the commands queued since the last frame, then adds the frame's seconds up; once they pass the speed's interval (2, 1, 0.5 and 0.2 seconds a day for speeds 1 to 4, none at speed 5) it posts the day's command, `DailyTick`, which the next frame executes like any other ([research/time.md](../research/time.md), "The speed"; [research/architecture.md](../research/architecture.md), "Time"). So at most one day begins a frame, and at speed 5 one every frame.
- **In multiplayer** only the host posts days, and it waits for the slowest player to report the day before; every machine then executes the same commands in the same order, and a checksum of the day's values, the random generator's included, catches a machine that went another way ([research/architecture.md](../research/architecture.md), "Commands and multiplayer").

## Pausing

- **The game is paused** when the player paused it, or when a window holds it paused: an event window adds itself when the settings pause on events, and takes itself out as it closes ([research/time.md](../research/time.md), "Pausing"). Which other windows hold the game this way was not all found.
- **In single player the pause key toggles the pause directly**, not by command: pressed while only windows hold the game, it lets every window go (they stay open) and resumes; pressed again it pauses. In multiplayer the key posts the pause command ([research/time.md](../research/time.md), "Pausing").
- **The end date** (1821.1.2 unmodded): past it the end-of-game summary comes up, the game stays paused, and the player can resume only once that summary is closed to play on. A game whose player has no country and that does not let an observer run time cannot be resumed either; the console's `observe` does let time run ([research/time.md](../research/time.md), "The player's resuming, and the end date"). Observing is [observe mode](../plans/observe-mode.md).

## Dates

- **A date is a count of hours** since 1 January 5000 BC, in a calendar of 365-day years with no leap day and months of fixed lengths, January 31 days to December 31 ([research/dates.md](../research/dates.md), "How a date is kept").
- **The date moves on first.** The day's update adds a day to the date before anything of the new day runs, and keeps the year, the month and the day of the month beside it ([research/time.md](../research/time.md), "The daily update").
- **Dates as text** are written by a handful of the game's functions: one for the long date that localisation words (`LONG_EU3_DATE_STRING`, the date in tooltips, events and the message log), and several that write their order in code, in English whatever the language ([research/dates.md](../research/dates.md), "The functions that write a date").

## A day, in order

The day's update (`0x758430`) moves the date on, posts the player's report of the date (`client_ping`), runs the day's body, then the monthly update when the month changed and the interface's monthly refresh, the yearly update when the year changed, and deletes the units waiting to be deleted ([research/architecture.md](../research/architecture.md), "Time"). The world keeps a flag set exactly while a day runs ([research/time.md](../research/time.md), "The daily update").

**The day's body**, in order ([research/architecture.md](../research/architecture.md), "Time"):

1. Objects waiting to be destroyed are destroyed.
2. Wars that ended; pirates; the list of active countries rebuilt.
3. The papacy, great projects, the celestial empire.
4. The game's own events due today.
5. Two passes over the countries, on the worker threads.
6. Rebels; units.
7. **Each country's daily update, one country after another** (below).
8. The Holy Roman Empire.
9. The provinces: two passes on the worker threads, then one on the main thread (constructions, spies).
10. Battles.
11. **The AI stage**, every AI country at once on the worker threads ([ai.md](ai.md)).
12. Military power; each war's day.
13. The interface's daily refresh.
14. Army missions; unit templates.

**Each country's daily update** is the country's simulation, handed to a dozen parts, none large alone: its armies' and fleets' days (moving along their routes and arriving in provinces, and the AI marking where they stand on its threat map), its knowledge of other countries refreshed when asked for, the AI's sums of the country's strength, its estates' privileges and reforms checked against their conditions, its modifiers made again, the AI's strategy and money ([research/country-daily-update.md](../research/country-daily-update.md)). The countries are in [countries.md](countries.md), the armies in [military.md](military.md).

**The monthly update** runs on a month's first day: a row of phases, four of them on the worker threads, the rest two loops on the main thread, **every province's month** and then **every country's month** ([research/monthly-update.md](../research/monthly-update.md), "What it runs, in order"). A country's month is a long row of small business: the AI's budget, its modifiers made again, cores lost after years unowned, each unit's month, mercenary companies, triggered modifiers gained and lost, messages to the player. A province's month draws for its gold, recovers from looting, moves its autonomy's bounds, recounts its modifiers, changes owner when due, and works out its trade and its effects ([research/monthly-update.md](../research/monthly-update.md), "A country's month", "A province's month"). Both loops draw random numbers and change other countries and provinces as they go, so the order they run in is part of what they compute ([research/monthly-update.md](../research/monthly-update.md), "What stops the loops from running on several threads").

## Frames inside a day

Between the day's phases the game checks the clock; once the day has run longer than an interval since the last frame (0.02 s at speeds 1 to 4, 0.05 s at speed 5), it draws a whole frame there: input, the in-game frame, drawing and Present ([research/main-thread.md](../research/main-thread.md), "Frames inside a day"). So the screen keeps to at least 20 frames a second however slow the days are. Such a frame does not feed the day: drawing random numbers is forbidden in it, the session executes no commands in it, and the player's input it takes is posted as commands for the next frame between days ([research/architecture.md](../research/architecture.md), "The application, its frame and the idlers").

The frames between days, and the frames inside them, are the in-game idler's frame, the game's top-level state while a game goes on ([research/architecture.md](../research/architecture.md), "The application, its frame and the idlers"); what they draw is in [interface.md](interface.md).

## The threads

- **The main thread does everything in the frame**: input, commands, the game state, the interface, drawing. The game pins it to one CPU ([research/architecture.md](../research/architecture.md), "Threads").
- **The worker threads** ("Short Task", one fewer than the computer's logical processors) run the day's parallel stages, about ten over countries, provinces, units and AI countries: a stage divides its range among them, merges their results under a lock and applies them one after another ([research/architecture.md](../research/architecture.md), "Time", "Threads"). Everything else of the day is serial on the main thread: the date, wars, battles, every country's and unit's daily update, the game's own events, the interface.
- **The main thread is what holds the game back.** At speed 5 it runs nearly a whole core while every other thread runs a fraction of one: late in a game the whole game used 2 to 3 of the 16 threads of the computer measured ([research/main-thread.md](../research/main-thread.md), "The shares"; [research/late-game.md](../research/late-game.md), "The whole day, late in a game").
- **A stage waits for its last part.** The main thread takes the stage's parts like the workers, then spins until the last part anyone took is done ([research/late-game.md](../research/late-game.md), "The AI stage"); late in a game that spin is most of what the AI stage costs ([ai.md](ai.md), "What it costs").

## Random numbers

- **The day draws from one generator**, a Mersenne twister kept in the game's data, with a count of the numbers drawn; multiplayer's checksum and the save keep both ([research/random.md](../research/random.md), "The generators"). A draw where drawing is forbidden (a parallel stage, a frame inside a day) logs "Calling random inside forbidden area!" and gives 0 ([research/random.md](../research/random.md)).
- **The game's parallel stages take their numbers from a table.** Before such a stage the main thread draws 1000 numbers, and each part takes the one at its object's ID plus an offset, so its numbers do not depend on the threads ([research/random.md](../research/random.md), "How the game's own parallel stages draw").
- **The AI's agents draw from the world's own generator**, from every AI country's task at once and with no lock, so which country gets which number depends on the threads' timing; with `-seed` each agent draws from a generator of its own instead ([research/random.md](../research/random.md), "The generators"; [ai.md](ai.md)).
- **A game does not repeat its days.** Loaded twice, the same save goes two ways within its first ten days, from inside the day rather than from the frames between days ([research/main-thread.md](../research/main-thread.md), "The game does not repeat itself"); the AI's threads drawing in another order is one such way, and a late save loaded twice came back with a random seed of its own each time ([research/late-game.md](../research/late-game.md), "Whether a game repeats its days"). A game started with `-seed`, loaded first in a new process, repeats its days well, its AI then running one country after another on the main thread; in two runs of five one admiral of Great Britain differed, and the rest followed it ([research/late-game.md](../research/late-game.md), "Whether a game repeats its days").

## Where the time goes

- **Early in a game the day is two thirds of the main thread**, each country's daily update the largest part of it, the frames between days the rest ([research/main-thread.md](../research/main-thread.md), "The shares").
- **Later in a game the AI grows**, and nothing else of the day: in a game the AI played alone from 1444 to 1821 the AI stage went from about 5 ms of the main thread a game day to 30, each country's daily update fell from about 21 ms to 14 as countries were annexed, the monthly update's share stayed at 9 to 11 ms, and a frame cost the same; at speed 5 the days went from about 20 a second to 12.5 ([research/late-game.md](../research/late-game.md), "The days, twenty-five years at a time"). Why the AI grows is in [ai.md](ai.md).
- **An average late day**, 64 ms of the main thread in 1780: the AI 24, each country's daily update 12, the monthly update's share 9, drawing 7.5, the rest between days 6, the provinces 3 ([research/late-game.md](../research/late-game.md), "The whole day, late in a game").
- **The month's first day stalls**: the monthly update takes about a quarter of a second on the main thread alone, every month, early and late ([research/late-game.md](../research/late-game.md), "The whole day, late in a game"; [research/monthly-update.md](../research/monthly-update.md)).
- **The autosave** runs in a frame, writing the game state into memory and then compressing it; late in a game one took nearly two seconds ([research/main-thread.md](../research/main-thread.md), "The autosave"). Saves are in [program.md](program.md).
- **Two sleeps of a millisecond every frame** in Paradox's online services' library, which sleeps when it has no socket to wait on: 5 to 6% of the main thread at speed 5 and a third of a paused frame ([research/main-thread.md](../research/main-thread.md), "POPS waits on the main thread").
- **The drawing cannot run beside the day.** A frame reads the live game state, every province each frame among it, while the day changes it, with no copy or lock between them; the game keeps them apart by running both on one thread ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "A simulation thread and a drawing thread"). The day in turn changes the interface's and the graphics' own state as it goes: it makes popups and messages, dialogs to the player, a combat result drawn into a texture, the map mode with its colors and names, the map's markers when a province changes owner, the minimap and the units' models, from a country's, a province's or a battle's own code ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the day reaches the interface and the graphics"). Leaving those aside, the drawing reads little of the game state, and the game lets its whole frame in between two countries' daily updates every 32 countries as well as between the day's phases ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Where the game lets a frame in, inside the day"); so the frames could be drawn on a thread of their own at the display's rate, the interface updated at those points ([plans/drawing-thread.md](../plans/drawing-thread.md)).

## What the framework relies on

- **Time for remote control** (`game::time`): the speed set through the speed commands, the pause through the toggle, and an advance stopped on its date by pausing as the day's update returns, so that no further day begins ([research/time.md](../research/time.md), "Stopping on a date"; [plans/remote-control.md](../plans/remote-control.md)). The commands run at once ([ADR commands-run-at-once](../adr/2026-11-03-commands-run-at-once.md)).
- **A new day for extensions**, observed at both ends of the day's update; the game is not ready to be read while the day's flag is set ([research/time.md](../research/time.md), "The daily update").
- **Dates in the player language**: the framework writes the dates the game writes in code from templates of the player language ([research/dates.md](../research/dates.md), "What the framework does"; [plans/player-language.md](../plans/player-language.md)).
- **Speed-ups** ([plans/game-speed.md](../plans/game-speed.md)): "Faster time" leaves out the frames inside a day at speed 5 (S4); Paradox's online services served without their sleeps (S1); sets of modifiers added through an index in the monthly update (S10, [research/late-game.md](../research/late-game.md), "Adding sets of modifiers"); the AI's armies' searches on idle threads (S9, [ai.md](ai.md)). The bar they meet is [ADR the-game-may-be-made-faster-with-reasoned-differences](../adr/2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md).
- **Random draws on several threads**: work the framework runs on several threads, and the AI's countries as a repair, draw from a stream for each task, seeded from one number drawn on the main thread and the object's ID ([research/random.md](../research/random.md), "A stream for each task"; [ADR random-draws-may-change-hands](../adr/2026-11-08-random-draws-may-change-hands.md)).
- **Returning to the main menu** ends the game's process and starts `eu4.exe` again, unless the framework returns in the process ([research/time.md](../research/time.md), "Returning to the main menu"; [program.md](program.md); [plans/sessions.md](../plans/sessions.md)).

## What is not known yet

- Which line of `settings.txt`, if any, sets the interval of the frames inside a day ([research/main-thread.md](../research/main-thread.md), "Open questions").
- Which windows besides the event window hold the game paused, and what the fields the multiplayer pause reads mean ([research/time.md](../research/time.md), "Pausing").
- Which of the end-of-game summary's two buttons does what ([research/time.md](../research/time.md), "The player's resuming, and the end date").
- Whether a load keeps the save's random state: read in the running game, a load of 1462 set the generator's seed and count to the save's ([research/main-thread.md](../research/main-thread.md), "The game does not repeat itself"), while two loads of a save of 1780 each wrote another seed than the save's ([research/late-game.md](../research/late-game.md), "Whether a game repeats its days"); the notes do not say why.
- What makes a day draw a different number of random numbers from one run to the next, how a load sets the generator from the save's seed and count, and what picks the admiral that varies under `-seed` ([research/main-thread.md](../research/main-thread.md), "Open questions"; [research/random.md](../research/random.md), "Not verified"; [research/late-game.md](../research/late-game.md), "Not verified").
- Whether each country's daily update, or a country's or a province's month, reads what another country's writes, which decides whether they could run apart ([research/late-game.md](../research/late-game.md), "Not verified"; [research/monthly-update.md](../research/monthly-update.md), "Not verified").
- How the worker pool behaves with two stages in flight, which a frame drawn while the AI stage is waited for would need ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Open questions").
- Which of the places where the day reaches the interface a late day reaches, how often and how soon into the day, and which the calls through pointers add; whether the day moves the units' models as it moves the units ([research/drawing-beside-the-day.md](../research/drawing-beside-the-day.md), "Open questions").
- All of it is of the game as measured with the framework installed, in one AI-only game and a few saves; a vanilla game was not profiled ([research/main-thread.md](../research/main-thread.md), "Open questions").
