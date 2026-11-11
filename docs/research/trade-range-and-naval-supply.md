# The trade range check and the naval supply table (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-11 as the third step of [game-speed.md](../plans/game-speed.md), "How a part is chosen", for two candidates of the day's own work: the check of whether a province lies within a country's trade range (`0x3520d0`), and the table of each country's nearest port for every province (`0x350b80`). Neither is built. From their code (`core/examples/disasm.rs`, and scripts over the image that find the direct calls to a function and map each to its function through `.pdata`), the defines' loader for the names of the two ranges, and the main thread's samples of two runs already made: the 1780.7.1 save observed at speed 5 for 240 s ([the AI stage's wait, measured](../prototype-log/2026-11-08-ai-stage-wait-measured.md): 11,139 samples, 3,563 days), and one game from 1444 to 1580 ([one game from 1444 to 1580](../prototype-log/2026-11-06-one-game-1444-1580.md): 258,071 samples, about 49,400 days). Nothing was run in the game for this note.

In short: **`0x3520d0` asks whether a province lies within a country's trade range**: the range is the country's technology's range times its range modifier, and the distance is the least from any of the country's cored cities, or from those of the countries whose relation lends it theirs, to the province, over land and sea. It walks the country's provinces until one is close enough, working out a distance for each, and an answer of no walks them all. **Every merchant at work in a node is checked so every day** (`0x25ad10`, in the provinces' serial pass), which is nearly all of its time: late in a game 0.63 ms of the main thread a game day (0.94% of it), earlier 0.1 to 0.3%. **The answers can be kept exactly by a witness**: the province that was close enough last time, which is checked again with the game's own distance instead of the walk. That takes at most the 0.6 ms a day. **`0x350b80` builds the naval supply table** a country's fleets are supplied by: for every province of the world the nearest of the country's ports and whether it is within the naval supply range. The game already rebuilds it only for a country that marked it changed (`+0x2887`), on its month: about 2 ms of a month late in a game, nothing worth making faster in the day. At a load it is built for every country at once, which was not timed.

## The trade range check

### What it works out

**`0x3520d0(country, province, i32* share) -> bool`**: whether `province` lies within `share` thousandths of the country's trade range (the callers pass 1000). The callers are the merchants' rules and the AI's and the interface's trade work (below), so that it is the trade range by its callers' texts: the merchant's rule TRADER_IN_RANGE, and the range's tooltip `0x352180` (MERC_RANGE_EXPLAINED, MERC_RANGE_EXPLAINED_OVERRIDE, `DISTANCE`, `RANGE`, `ORIGIN`).

1. When the country's capital (`+0x14b0`) has the same `+0xe8` as the province, the answer is yes and nothing else is read. What a province's `+0xe8` is was not worked out (_inferred_: its trade node, the node of one's capital being always in range).
2. **The range**, `0x3503d0(country, i32* out)`: the technology object's (`[country+0x2440]`) `+0x54` times one plus the country's modifier at index `0x91` (its modifiers `0x93d750(country+0x28c0)`, found by index in their pairs or, when the set is kept whole, at `+0x48c` of its values), with the modifier type's own adjustment where the type has one (`[[0x2358080]+0x36a0]`, a virtual call at its `+8`), in thousandths. The modifier's name was not found: it is the same index the naval supply range uses (below). The check then multiplies the range by `share`/1000.
3. **The distance**, `0x3526b0(country, i32* out, province, _, i32* bar) -> out`, the least distance of the country's to the province, which stops as soon as one is below `*bar` (the check passes its range). It starts from 99,999,000 (99,999 in thousandths) and walks:
   - **the country's own provinces** (`+0x1338`, their count at `+0x1344`, each found in the world's array `[world+0x1ca8]` by its index, 0x2e10 bytes apiece) that are cities (`+0x958` bit `0x10`) and have the country among their cores (`+0x490`..`+0x498`, compared by the tag's index); for each, the lesser of the distance over land and sea `0x2d2520(&d, country's tag, from, province, null, 1)` and the straight distance between the two (below), divided by one plus TRADING_CITY_TRADING_RANGE_BOOST (`[0x233e0bc]`, thousandths) when `0x3f14a0(country)` holds, a rule of the country's government (`+0x1420`, its `+0x38` asked by `0x7acec0`; _inferred_ from the define's name: the trading city's), else by one;
   - **the countries its relations lend their provinces**: the country's relations with every other country (`+0x1510`, 0x48 bytes by the other's index), where the relation's kind, found in the other's diplomacy object (`[relation]+0x128`, pairs of 16 bytes by the kind's index read from `[relation+0x10]`'s high half), leads to an object whose `+0x40` has its byte `+0x11f` set (what kinds those are was not worked out; _inferred_: subjects whose type extends their overlord's range). Each such country's cored cities are walked the same way, divided by one plus the same define when that country's government has the rule.
   - When a distance is below `*bar` it stops at once. The answer is the least distance cut down to whole units (thousands). The check answers yes when that is at most the range.
4. **The straight distance** between two provinces is the map's: `0x88a390` on the two provinces' positions in the map's data (`[0x233fed8]+0x6c0`, 0x230 bytes a province, its `+0x7c`), the map's width and height (`+0x28`, `+0x2c`) and whether it wraps (`+0x31`), its square root times 1000, rounded (`0x1dc1938` is 1000.0, `0x1d99d94` 0.5); with the map's byte `+0x31` set, the value as it comes, times 1000.
5. **`0x2d2520(out, tag, a, b, bool* reached, bool which)`**, the distance from `a` to `b` for the country with `tag`: when the two provinces share the object at `+0x338` (or `+0x340`, as `which` says; neither was worked out, _inferred_: the land mass each lies on), the straight distance. Otherwise it goes by sea: through the provinces next to each (`[province+0x28]+0x168`, 0x24-byte entries), whether each has an owner (`+0x468`'s high byte), a port (`+0x550`), the coastal provinces of a group (`[group+0x40]`, its count at `+0x4c`), and the map's table of sea distances (`[0x233fed8]+0x17c8`, an N by N table of i32 by each province's map entry `+0x1b0`, its side at `+0x17c0`), the sum rounded to whole units (`0x17128a0`); 99,999,000 and `*reached` cleared when there is no way. It writes only `out` and `*reached`.

What the check reads, besides the map's data that do not change in a game: the country's provinces, its capital, its technology's range and its modifier `0x91`, its relations and their kinds, its government; each walked province's owner, city flag and cores; and, through `0x2d2520`, the owners and ports of the provinces next to the two. It writes nothing but its answer.

### Who asks, and how often

The direct calls of `0x3520d0` (fourteen, in eleven functions) and of `0x3526b0` (five, three of them outside the check):

| Caller | What it is | When |
| --- | --- | --- |
| `0x25ad10` → `0x3532c0` | A merchant's mission's day (the construction of a merchant, [envoys.md](envoys.md)): in phase 1 (_inferred_: at work in its node; 2 is a recall, [envoys.md](envoys.md), and 3 skips the check) it asks the merchant's rules again, `0x3532c0(country, the node's province, null, 0, 0, 0)`, and recalls the merchant when they no longer hold (`0x25ba70`) | Every day, for every merchant at work, in the provinces' serial pass (`0x9f52e0`) |
| `0x1bc1e0` → `0x1be790` → `0x3532c0` | The AI's merchants: whether it may send one to a node | The AI's thinks, on its threads |
| `0x1ba150` → `0x1b96c0` | The AI's trade fleets and privateers (`ai_financial.cpp`, "Evaluating trade fleets for: ") | The AI's thinks |
| `0x2f36c0` → `0x329140` → `0x90e6e0` | Something of the country's month | Monthly |
| `0x3532c0` from the interface (`0x101ccf0`, `0x101d7c0`, the trade view's items `0x102b2c0`…`0x103a800`, `0x13ca7e6`…) and the command's check (`0x3bae50`) | The merchants' rules for the player's view and the send command | When the player looks |
| `0x9e3210`, `0x1024c40`, `0x101e2b0`, `0x101e400`, `0x1023800`, `0x11a1ef0`, `0x1417b90`, `0x1418e70`, `0x12fdcb0` | The interface's trade views, the ledger (TRADE_RANGE_IRO), the privateers' node list, the map's trade mode | When drawn |
| `0xbdbf60` | A script trigger on a province (`triggerimplementation.cpp`), reached through its table, not by a direct call | When script asks |

Of the main thread's samples:

- **Late in a game**, the 1780.7.1 save observed: the check on the stack in 105 of 11,139 samples, **0.94%**, 2.3 s of 240 s over 3,563 days, **0.63 ms a game day**; of them **95 in the merchants' day** (`0x9f52e0` → `0x25ad10` → `0x3532c0`), 3 in the AI's merchants, 3 in the privateers and 4 in the month. The merchants' rules took 109 samples in all, so their range is nearly all of them. Inside the walk (104 samples) 70 were in `0x2d2520` and its province lookups, 33 in the walk's own code. On the other threads the AI's merchants and privateers took 36 samples more, about 0.2 ms of other threads a day, and the main thread waited for them in the AI stage 18 ms in all over the 240 s: off the main thread's path.
- **Earlier**, the game played from 1444 to 1580: 470 of 258,071 samples, **0.18%**, 0.19 ms a game day; by stretches of ten minutes 0.30% at the start, 0.11 to 0.23% after.

The call trace counted the checks ("Timed in the game"): about 580 a game day on the main thread, 85% of them walking; how many answer no was not counted.

### A faster way with the same answers

The walk stops at the first province close enough, and a merchant's node, once in range, stays in range day after day; so the province that stopped it last time most likely stops it again, after a walk of the provinces listed before it. **A witness** keeps that province, by the country and the province asked about:

- On a check, the framework works out the range as the game does (`0x3503d0`, cheap) and the witness's distance with the game's own functions (`0x2d2520` and the straight distance, divided as above). If the witness is still one of the country's cored cities and its distance, cut down to whole units, is at most the range, the answer is yes. That is the game's answer: the game's least distance is no more than the witness's, so its whole units are no more either.
- Otherwise the game's walk runs as it does, and the province it stops at becomes the witness: the last province the walk worked out a distance for when it returns early, which an observer of its two calls of `0x2d2520` (`0x3527bc`, `0x352a17`) can keep on the thread. An answer of no, or a yes reached at the walk's end, keeps no witness.
- What the witness must check again is what the walk checks of a province: that the country owns it (its owner `+0x468` is the country's tag; that this always agrees with the country's list `+0x1338` was not checked, and a developer build's `checked` would compare every answer), the city flag and the core. A witness from a country whose relation lends its provinces must check that relation's kind as well; leaving those out (keeping only the country's own) loses little, since most countries' own provinces are close enough.

It rests on the distance being worked out by the game's own code each time, so that a change of owners, ports or relations along the way is seen as the game sees it, and on nothing being kept but a province's index. Its risks: the walk's order decides which province stops it, and the witness may be another than the one the game would stop at; the answer is the same, and the distance the range's tooltip shows comes from the tooltip's own walk (`0x352180`, `0x35404a`, `0x3542b7`), which the witness leaves to the game. **The most it can give is the check's time on the main thread, 0.63 ms a game day late in a game**, less the cost of one distance a check and of the answers of no, which still walk everything; how many those are decides how much of it.

## The naval supply table

**`0x350b80(country)`** builds the table at `country+0x2320`, a vector of i16, one entry for each province of the world (its size the world's provinces, `[world+0x1c90]`..`+0x1c98`). It clears the country's mark `+0x2887`, and leaves the table cleared for a country without provinces (`+0x1344`) or with nothing at `+0x1c78` (not worked out; _inferred_: its fleets). Otherwise, for each province of the world but the first and those whose map data's `+0x228` has bit 8 (not worked out):

- **The range**, `0x350a20(country, i32* out)`: NAVAL_SUPPLY_RANGE (`[0x233e0c8]`, the define's name in its loader at `0x4b655d`) times one plus the same modifier `0x91` the trade range uses, in thousandths.
- **The nearest port**: the country's own provinces that are cities, controlled by their owner (`+0x470` and `+0x468` the same tag), have a port (`+0x550`), a core of the country's, and not `+0x958` bit `0x40` (not worked out); then the provinces of the countries its relations lend theirs, by two of the relation's kind lists (`+0xe0` and `+0x128` of the other's diplomacy object; not worked out). For each, `0x2d2520(&d, country's tag, port, province, null, 0)`, the least kept.
- **The entry** is the nearest port's index when its distance is within the range, minus it when it is farther, and 0 when there is none. When the country's list at `+0x1468` has no entry of kinds 3 or 4 (not worked out), a province marked by `+0x959` bit `0x10` and whose bit for the country in `+0x5e0` is clear is left out, its entry kept from the build before (_inferred_: one the country has not discovered).

So a build works out up to the world's provinces times the country's ports and its partners' distances, each one `0x2d2520`.

**When it runs**: the country's month (`0x2f36c0`, at `0x2f76b1`) builds it only when the mark `+0x2887` is set, which about twenty places set: among them a province's change of owner (`0x327dc0`), the diplomatic relations' changes (`0x52d760`, `0x531b70`), vassals and colonies released (`countrycommands.cpp`, `0x2a0df0`, `0x2a1ef0`), dynamic countries (`0x481f90`), a diplomatic action (`0x56f8b0`) and an effect (`0x62fbc0`); some of those also build it at once. At a load, "Recalculating cached data" (`0x7760b0`, `gamestate.cpp:9548`) builds it for every country (`0x776606`), after `0x327dc0`, which marks it too.

**Its cost**: late in a game 12 samples of 11,139, all in the month: 0.26 s over about 117 months, **about 2 ms a month, 0.07 ms a game day**; from 1444 to 1580, 106 of 258,071, about 1.3 ms a month. The game's mark already keeps it from being built when nothing changed, so in the day it is no candidate. [loading.md](loading.md) found "the day's setup" after a load, `0x350b80` and `0x7760b0` → `0x2d0b30` together, at 8 to 14% of a 10 s load; timed, the tables' 2,758 builds at the 1780.7.1 save's load took 0.26 s of the 1.32 s of "Recalculating cached data" ("Timed in the game").

**At a load**, the countries' tables could be built side by side: each build writes only its own table and mark, and reads the map's data, the provinces' owners, controllers, ports and cores, and the countries' relations, which nothing changes while they are built. But `0x7760b0` builds each in its turn among other work for that country (`0x327dc0` before it, `0x2ec1d0` and `0x38b070` after), and whether any of those writes what a later country's build reads was not read: that is the research the way would need. With 0.26 s of a load at stake, at most, it was not done.

## Timed in the game

With `dev-call-trace`'s `times` on a test game: the 1780.7.1 save loaded, then observed at speed 5 to 1784.11.24, about 1,600 days ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)):

| Function | Calls | Time on the main thread | What it tells |
| --- | --- | --- | --- |
| `0x3520d0` (3 arguments) | 1,020,235, 927,064 on the main thread | 1.691 s | about 580 checks a game day on the main thread, under 2 µs each with the tool's cost |
| `0x3526b0` (5) | 789,144 on the main thread | 1.378 s | 85% of the main thread's checks walk; 15% end at the leading shortcut |
| `0x25ad10` (2) | 910,505 | 2.070 s | the merchants' missions' days, nearly every check's caller |
| `0x350b80` (1) | 56 in play, 2,758 at the load | 0.169 s, 0.261 s | 3.0 ms a build in play; 95 µs a build at the load |
| `0x7760b0` (1) | 2 at the load | 1.316 s | "Recalculating cached data", of which the tables 20% |

The times include the tool's cost of about a microsecond a call, so the sample's 0.63 ms a game day for the check stands rather than the trace's 1.1 ms. `0x2d2520` is called for every province walked, tens of times a check, and was not timed. The answers of no, which a witness cannot spare, are not in the times file (it keeps no return values). So the witness could give up to about 0.6 ms of a game day late in a game, 0.5% of the run's day; the naval supply table nothing in play and at most 0.26 s of a load.

## Not worked out

- What a province's `+0xe8`, `+0x338` and `+0x340` are, and the map data's `+0x228` bit 8; `+0x958` bit `0x40`, `+0x959` bit `0x10`, the province's bitset at `+0x5e0`.
- The name of the modifier at index `0x91`; the government's rule `0x3f14a0` asks.
- Which relation kinds lend a country's provinces to another's range or supply.
- A country's `+0x1c78` and its list at `+0x1468`.
- What `0x90e6e0` does in the country's month, and the interface callers named only by their texts.
- Whether `0x7760b0`'s other work for a country writes what another country's table reads.
