# The monthly update, and what of it could run on several threads (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-08 as the third step of [game-speed.md](../game-speed.md), "How a part is chosen", for the monthly update (`0x75d690`), asked by the owner as the first of the main thread's serial work to look at for running on several threads. From the sampler's run of [the AI stage's wait, measured](../prototype-log/2026-11-08-ai-stage-wait-measured.md) (1780.7.2 to 1790.9.2, 67 ms of the main thread a day), from the game's code (a Capstone script over the image, the functions' extents from the exception directory's chained entries, a graph of the direct calls), and from the write watch in a test game ([what a province's modifiers read and write](../prototype-log/2026-11-08-province-modifiers-watched.md)).

In short: the monthly update runs on a month's first day, **about 255 ms**, 8.4 ms a day spread over the month, and the day it runs stands still a quarter of a second at the fastest speed. It is a row of phases; the game already runs four of them on its worker threads. The rest is two loops on the main thread, **every country's month** (`0x2f36c0`, about 152 ms) and **every province's month** (`0x9f35c0`, about 55 ms). Neither loop can run on several threads as it is: each country's and province's month draws from the game's one random generator, from 74 and 70 functions it reaches, and changes other countries and provinces (owners, effects, events), so its order is part of what it computes. Inside them, parts that draw nothing and work out one object's figures can: **a province's modifiers recounted** (`0x9f9500`, about 48 ms a month), the AI's budget (`0x122790`, about 25 ms) and `0x3bc640` (about 18 ms).

## What it runs, in order

`0x75d690` (`0x75d690`..`0x75e189`), its phases separated by the check for a frame inside a day (`0x769ea0`, with a code for each place: `0x10d5`, `0x10f8` and others):

| Phase | Where | ms a month (sampled) |
| --- | --- | --- |
| The Empire, imperial influence (`0x7019e0`, "SetImperialInfluence %d", `empirebase.cpp`) and others before | `0x75d6b7`..`0x75d913` | small |
| A parallel stage (`0x780880`, through the pool's `0x179e90`) | `0x75d94f` | about 4 |
| **Every province's month**: `0x9f35c0` for each province of `[world+0x1cd8]` (count `+0x1ce4`), the frame check every 50 | `0x75d9e8`..`0x75da23` | about 55 |
| A loop over `[world+0x1c90]`..`+0x1c98` with random draws (`0x14c71b0`, `0x75f630`) | `0x75da45`..`0x75dc35` | small |
| Three parallel stages (`0x77e9e0`, `0x77ee90`, `0x780d30`) | `0x75dc85`..`0x75dccd` | about 3 |
| **Every country's month**: `0x2f36c0` for each country of `[world+0x1ce8]`..`+0x1cf0` from the second, the frame check every 5 | `0x75dd30`..`0x75dd77` | about 152 |
| Loops over `0x3c2750`, `0x9c0a0` and others | `0x75de20`..`0x75e05f` | about 4 |
| `0x763470` → `0x3bc640` → `0xd54600`, sets of modifiers added (S10) | `0x75e07a` | about 18 |

## A country's month

`0x2f36c0` (18 KB, `0x2f36c0`..`0x2f7e81`), about 152 ms over the countries of the array (118 of them living in 1780, [country-knowledge.md](country-knowledge.md); how long the array is and how soon a country no longer living returns were not counted); a long row of the country's monthly business, each step small. By its sampled cost, a month a game day over the run:

| Step | ms a day | What it is |
| --- | --- | --- |
| `0x18f040` → `0x122790` | 0.83 | The AI's budget, set once a month ([ai.md](ai.md), "A day of the AI") |
| `0x327dc0` (`country.cpp:10766`) | 0.56 | The country's modifiers made again |
| `0x302ee0` | 0.38 | Cores lost: a core not owned nor fought over for years is dropped ("CORELOSS"), each province through `0x9fa230` |
| `0x2f9c50` | 0.34 | Not read; its own code |
| `0x329140` | 0.28 | Each unit's month (`0xd3efd0` for each) |
| `0x4109f0` | 0.24 | Mercenary companies no longer available ("MERCENARY_COMPANY_UNAVAILABLE") |
| `0xa6bee0`, `0x38b070`, `0x28ab40`, `0x93e760`, `0x2ec1d0`, `0xa675b0` | 0.1 to 0.2 each | Among them the triggered modifiers gained and lost (`0x2ec1d0`, "TRIGMOD", "TRIGMODLOST") |

Besides: messages to the player, a leader dismissed ("DISMISSLEADER"), the Treaty of Tordesillas, harmonized religions, war taxes stopped, the knowledge refreshed (`country.cpp:10641`).

## A province's month

`0x9f35c0` (5.5 KB), about 11 µs a province; in order: a random draw (`0x9f3756`, `province.cpp`), two small updates (`0xa12460`, `0xa12ab0`), `0x9f7a80`, **its modifiers recounted** (`0x9f9500` at `0x9f38d4`, 1.60 of its 1.80 ms a day), then a long row with more random draws (`0x9f3f4f`, `0x9f4115`), a change of owner when due (`0xa00270`, [dependency-teardown.md](dependency-teardown.md)), its trade, its tax text, its effects (`0xa255d0`).

## What stops the loops from running on several threads

- **Random draws.** `0x14c7420` and `0x14c74c0` draw from the game's one generator (`0x2325220`, a Mersenne twister `0x14c71b0`), which the game keeps the same on every machine of a multiplayer game. By direct calls a country's month reaches 2,988 functions, 74 of which draw; a province's 2,821, 70 of which draw. Each country takes the numbers after the countries before it, as many as its own state calls for: on several threads, a country would draw others' numbers.
- **Other objects changed.** The same paths change a province's owner (`0xa00270`, which goes on to the countries, `0x3663b0`, `0x364f10`), run effects and events (`0x76c1a0`), and refresh other countries' knowledge; a later country's month reads what an earlier one changed.

So the two loops cannot run on several threads with the same results; any change to their order is a change to what the game computes, of kind C, and would change every draw after it.

## The parts that could

By direct calls (calls through pointers are not in the graph, so a draw behind one would be missed):

| Part | Reaches | Draws | ms a month |
| --- | --- | --- | --- |
| A province's modifiers recounted (`0x9f9500`) | 80 functions | none | about 48 |
| The AI's budget (`0x122790`) | 638 | none | about 25 |
| `0x3bc640` | 460 | none | about 18 |
| A country's modifiers made (`0x93d130`) | 183 | none | (in `0x327dc0`) |
| A country's modifiers made again (`0x327dc0`) | 681 | one (`0x8c7790`) | about 17 |
| Cores lost (`0x302ee0`) | 1,044 | two | about 12 |

A part that draws nothing could be worked out for every object at once on the idle threads, before its loop or as the loop goes, and taken by the object's own call when what it reads has not changed since, as S9 takes an army's search; or, where nothing before it in the loop changes what it reads, worked out ahead with the same result. Which holds needs, for each part, what it reads and writes, and whether the loop's earlier steps change it.

## What a province's modifiers read and write

Watched with the write watch on 2026-11-08 ([what a province's modifiers read and write](../prototype-log/2026-11-08-province-modifiers-watched.md)): two calls of `0x9f9500`, each read and written, both made as an army arrived in a province (`0xd3fd30` → `0xd404bf`; the recount is called there too, as when a fort is mothballed, [sieges.md](sieges.md)), not by the month; the function is the same.

- **It writes only its own province**: four of the province's fields (`+0x18dc`, `+0x249c`, `+0x24a0`, `+0x24a4`), a block whose size followed the province (0x2870 bytes for one, 0x1438 for the other; _inferred_ its modifiers' values), a few small blocks (0x160, 0xe0, 0x80 bytes) and blocks it made; no static, no country.
- **It reads** about 17,700 times in 7,300 groups: 1,069 heap blocks, most of 0x20 to 0x50 bytes (_inferred_ the definitions of the modifiers it adds up), 73 places of `eu4.exe`'s data (the world `[0x233fe78]`, `0x1fd4b30`, `0x233ff59`, `0x233d850`), its own fields (`+0x20`, `+0x28`, `+0x468`, `+0x9a8`, `+0x9f0` the most, `+0x1888`, `+0x18d0`..`+0x18dc`, `+0x2490`..`+0x24a4`), another province's `+0x28` (index 146, in both calls), and a country's first word (index 201). The report lists only the 300 largest of a reading call's groups, so what else of other objects it reads is not known.

So the recounts of two provinces write nothing the other writes: run at once, they do not race on what they make. Whether a province's recount in the month reads what the provinces before it in the loop changed (an owner, a building, a country's modifiers) is not shown by one call: it decides between working the recounts out before the loop with the same result and working them out ahead and taking each only when its inputs are unchanged.

## What this means for making the game faster

- **The loops as they are cannot run on several threads**: their random draws and their changes to other objects are in order.
- **A province's modifiers recounted ahead, on the idle threads**, is the largest part that could: about 48 ms of the month's 255, a fifth of the month's stall, writing only its own province. The way to know whether the result is the game's: a prototype that works every recount out before the loop on the idle threads, keeps it, and at each province's own call, in a developer build, works it out as the game does and compares the two (`checked`), counting the differences; where there are none, it takes the result ahead in a player's build. Its bound: the 48 ms, less what the threads take, on a month's first day.
- **The AI's budget** (about 25 ms) and `0x3bc640` (about 18 ms) are the next of that kind, their reads and writes not watched yet.
- How many threads such a part should use, against the game's own workers (15 short-task threads on this computer's 16 logical processors) and the framework's (S9's 8), is to be decided with it: every pool counts logical processors, none counts cores.

## Not verified

- That the parts said to draw nothing draw nothing through calls by pointer.
- What `0x9f9500` reads beyond the 300 largest groups; a call of it from the month; whether the provinces before it in the loop change what it reads.
- What `0x2f9c50`, `0xa6bee0`, `0x38b070`, `0x28ab40` and `0x93e760` are; how long the countries' array is.
- One save, in observe mode.
