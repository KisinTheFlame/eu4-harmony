# Each country's daily update (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-08 as the third step of [game-speed.md](../plans/game-speed.md), "How a part is chosen", for its candidate each country's daily update (`0x2ee280`): the main thread's largest serial part of the day after the AI stage. From the sampler's run of [the AI stage's wait, measured](../prototype-log/2026-11-08-ai-stage-wait-measured.md) (1780.7.2 to 1790.9.2, 14.9 days a second, 67 ms of the main thread a day), from the game's code, and from an experiment build that counted one of its parts ([the threat map's updates, counted](../prototype-log/2026-11-08-threat-map-updates-counted.md)).

In short: each country's daily update is **13.2 ms of a 67 ms day**, run one country after another on the main thread, and almost none of it is its own code: it is the country's simulation, handed to a dozen functions. The largest is **its units' daily updates**, 4.8 ms: armies and fleets moving along their routes and arriving in provinces, and their AI marking where they are on its threat map. Then the knowledge's refresh ([country-knowledge.md](country-knowledge.md)), 1.7 ms; **the AI's sums of the country's strength**, each unit's strength worked out again, 1.4 ms; **the estates**, their privileges checked against their conditions, 1.4 ms; the country's modifiers made again, 0.8 ms; the AI's strategy and money, 0.8 ms. No part was found doing again what it did before: the one suspected, an army's threat map updated twice on a day it moves, happens for 1.7% of the updates. **It is no waste to cut but work to spread**: the bound left is running the countries' updates apart, which changes the order the game does them in.

## Where its time goes

Sampled with `cargo xtask profile`, the 1780.7.1 save in observe mode in the Ottomans' view at speed 5, 240 s; each country's daily update was 19.6% of the main thread, 13.2 ms a game day, of which its own code 0.19.

| What it calls | ms a game day | What it is in the game |
| --- | --- | --- |
| `0x1a2d30`, an army's method `+0xd0` | 3.42 | Each army's day: its unit's daily update, then its AI's update of the threat map |
| `0x93fe30` | 1.66 | The country's knowledge refreshed when asked for ([country-knowledge.md](country-knowledge.md)) |
| `0x90e410`, a fleet's method `+0xd0` | 1.40 | Each fleet's day: its unit's daily update and the fleet's own |
| `0x37c730` (`UpdateEstates`) | 1.39 | The estates: privileges no longer valid, reforms, the crown land |
| `0x93c790`, then `0x93ca70` twice | 1.37 | The AI's sums of the country's strength |
| `0x327dc0` | 0.84 | The country's modifiers made again |
| `0x182580` | 0.47 | The AI's strategy, when it is due ([ai.md](ai.md), "A day of the AI") |
| `0x18e7f0` | 0.34 | The AI's money set aside for the day ([ai.md](ai.md)) |
| `0x38b1f0`, `0xa855b0` | 0.34 each | Not read |
| The rest, 15 functions | 1.6 | Under 0.25 each |

## The units' days

An army's method `+0xd0` (`0x1a2d30`, vtable `0x1c42540`) calls its unit's daily update (`0xd3e110`, `CUnit::DailyUpdate`, [time.md](time.md)) first, then, past some checks of its own state (`+0x207`, `0x1a2a50`, `0x1a8c10`), its AI's update of the threat map; a fleet's (`0x90e410`) calls the unit's daily update and the fleet's own. Of the 3.18 ms in the units' daily updates:

- **Moving** (`0xd51520`, `unit.cpp`, "CancelMovement", "CancelAllOfPathButNext"), 2.0 ms: a unit on a route steps along it, and one that reaches the next province is put there (`0xd3fd30`, "SetLocation, %u, %d": the location `+0xe8`, the interface, the provinces' lists, the rebels' messages, [armies.md](armies.md)), 1.4 ms of it.
- `0x90f740` → `0x91c2f0`, 0.66 ms, in the fleets' updates: not read.

**The threat map** (`0x242ba00`, which the AI stage reads, [ai.md](ai.md)): `0x195e10(map, army)` finds the army's record in the map's table (`+0x30`, by the army's ID), counts it and stamps it with the date (`0x195e86`), gathers the provinces the army stands in and is heading for (its location `+0xe8`, its route `+0x100`), and from each looks over its neighbors (`0x195750`, the route edge check `0x96f860`, `0xd4f4a0`), writing what it finds into the map (`0x1990f0`, `0x199ef0`). It is called from the army's method after its unit's day (1.08 ms sampled) and from `SetLocation` when a unit arrives in a province, while a setting of the map manager allows (`[world+0x1e00]`'s method `+0x80`, its bytes `+0x54` or `+0x55`; 1.19 ms sampled).

- **Counted** in the game: 34.8 updates a game day, about 100 µs each, all on the main thread; **1.7% were of an army already updated that day**, 0.05 ms a day, and 5% of those had moved since. So an army is not updated twice on a day it moves, as its two callers suggested: what updates cost is each update's look over its neighbors, not repeats.
- What the threat map is to the AI, and why an army's day updates it rather than the AI stage, was not followed; [ai.md](ai.md) has the stage rebuilding it before the AI's tasks (`0x1950e0`), which reads dates more than it rebuilds, and was not followed either.

## The AI's sums of the country's strength

While the AI runs (`world+0x7484`), the daily update calls `0x93c790` and `0x93ca70` twice (with 0 and 1) on the AI's part of the country (`country+0x28c0`), and keeps the three sums at `country+0x96c0`, `+0x96c8` and `+0x96d0` (`0x2ee4e6`..`0x2ee526`). `0x93c790` walks the country's units (`+0xd18`) and works out each one's strength with `0xd46800` (the strength the support check also uses, [ai-stage-wait.md](ai-stage-wait.md)), 1.03 ms of it in `0xae0f00`. Every country, every day, every unit: a unit's strength changes as it reinforces, loses men and morale, so the sums are worked out new each day; what reads them, and whether a day-old sum would do, was not followed.

## The estates

`0x37c730` (`country.cpp`, "UpdateEstates", "EstateAdd %s %d %d", " crownland changed to "), 1.39 ms: `0x723120` (0.70 ms), `0x7b2940` (0.48 ms, the strings "REFORM", "REFORM_DESELECTED") and `0x71e000` (estates.cpp, "PRIVILEGE_NO_LONGER_VALID", "_EFFECTS", "_NO_EFFECT"). Their time goes to the script's conditions: the "or" (`0xb985b0`, true when any child is) and "nor" (`0xb98910`, true when none is) blocks of [triggers.md](triggers.md), through `0xd135c0` → `0x7a6020`. So each day every country checks its estates' privileges and its reforms against their conditions, to take back what is no longer allowed.

## What this means for making the game faster

1. **No part does again what it did before.** The knowledge's refresh is the only part with repeats on a day, and its ways are bounded at about 1 ms ([country-knowledge.md](country-knowledge.md)); the threat map's repeats are 0.05 ms.
2. **Doing a part less often** (the estates' checks, the strength sums) changes what the game does on the days between: a privilege taken back a day later, the AI deciding on an older strength. Each a difference of kind C, small: 1.4 ms each at most.
3. **The countries' updates run apart**, on the idle threads: bounded by the update itself, 13 ms a day, the most there is ([late-game.md](late-game.md), "Where the late day could be shorter"). The updates read and write each other's countries (a diplomatic action's refresh of both, a war's allies, the units of one fighting another's), so running them apart changes the order the game does its work in, and needs every such write found and made safe first. A large piece of work, of kind C.

So each country's daily update is the game's simulation of the country, spread over its parts, none of them large enough alone; what would make it shorter is running it apart, which is the size of a part of its own.

## Not verified

- `0x38b1f0`, `0xa855b0`, `0x90f740` → `0x91c2f0`, `0x723120`'s and `0x7b2940`'s own work beyond their strings and the conditions they evaluate.
- What reads the strength sums at `country+0x96c0`..`+0x96d0`; what the threat map is to the AI, and the setting that lets `SetLocation` update it.
- One save, in observe mode; a game a player plays; the experiment's own cost in its 100 µs.
