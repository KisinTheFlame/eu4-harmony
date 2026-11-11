# A country's tolerance of a religion in a province (for making the game faster)

Baseline 1.37.5. All addresses are RVAs into `eu4.exe`. Researched on 2026-11-11 as the third step of [game-speed.md](../plans/game-speed.md), "How a part is chosen", for a candidate of that plan, not built: `0x2ffc60`, which copies a province's whole set of modifiers to read one value of it. From the game's code (`cargo xtask disasm`, and a script over the image for the direct calls and the pointers to a function, the functions' extents from the exception directory), the modifiers' names from the place where the game makes its modifier table (`0x8d84a0`), and a sample of every thread taken on 2026-11-11 with `cargo xtask profile --threads` on the 1780.7.1 save in observe mode at speed 5: 180 s, 1,529 days, 8.5 days a second, 117.7 ms of the main thread a day. The run's conditions (the build, another test game running beside it) are in [the run](../prototype-log/2026-11-11-five-candidates-timed.md). That pace is slower than the 14.5 days a second of [game-speed.md](../plans/game-speed.md) ("Which part comes next"), so the shares below matter more than the milliseconds.

In short: **`0x2ffc60` is a country's tolerance of a religion in a province**: the country's own tolerance of the true faith when it takes the religion as its own, and otherwise its tolerance of heretics or heathens plus the province's local tolerance of them, capped by the country's capacity for it. To read the one local value it needs, it first copies the province's whole set of modifiers (a name, a vector of the modifiers it was made of, every pair of modifier and value), reads the value from the copy, and lets the copy go. Late in a game nearly all of its calls come from **religious unity**, worked out in a country's knowledge refresh once for each of the country's provinces. There **the answer is not even read for a province whose religion the country takes as its own**, and of the time inside `0x2ffc60`, 43 samples of 47 were the copy and 4 the reading. It is 0.6% of the main thread (0.7 ms a day at that run's pace). Two ways give the game's answers exactly. The first does not work out a tolerance that nothing reads. The second reads the province's set where it is instead of a copy, and is exact only while two of the modifier table's per-modifier objects are null, which the game can be asked at startup.

## What it works out

`0x2ffc60`(country, out, province, religion) (checked: 4 arguments) returns `out`, a fixed-point value (thousandths) of tolerance:

1. **The copy.** With a province, it makes a set of modifiers on its stack, the same kind of set as the province's modifiers at `province+0x998` (vtable `0x1c42770`, then `0x1c57288`). It copies the province's set into it field by field: its name (the set's method `+0x40` returns it, copied with `0x95110`), its vector of shared pointers at `+0xa8` (`0x8f4d70`, an atomic increment of each one's count), the value at `+0xc0`, and every pair of `+0x58` (`+0x64` counted) whose modifier is not `0x286`, appended one at a time into an array that starts with room for 8 and doubles on the heap as it fills (`0x1a33310`). `0x286` is one past the last of the game's 646 modifiers. A set that keeps its values by modifier (its byte `+0x10` set) fills its empty places with it, so the copy holds the same pairs in the same order without the empty ones. The copy itself keeps its pairs as pairs, its byte `+0x10` clear, so the branch that would copy a set kept by modifier (`0x2ffd8c`..`0x2ffed4`) never runs. Without a province it makes an empty set instead.
2. **The value**, `0x3003b0`(country, out, set, religion): when the religion's method `+0x40` answers no, 0. Otherwise:
   - **The religion taken as the country's own** (`0x36e300`(country, religion, 0)): the country's own tolerance of the true faith.
   - **A heretic religion**: the country's tolerance of heretics, with the province's local value added and the sum capped by the country's capacity. The religion counts as heretic when it is in the same group (`religion+0xd8`) as the country's state religion (`country+0xf30`).
   - **A heathen one**: the same with heathens.
3. **The reading**, `0x300720`(out, country's set, province's set, kind), with the country's set at `0x93d750(country+0x28c0)+8` (the country's modifiers as `country_cached_data.cpp` keeps them). Each value is read from a set kept by modifier at its place, or from a set kept as pairs by looking for the first pair with that modifier. Each value is then passed, with the set it came from, to the modifier table's object for that modifier, when there is one (below).

| Kind | Read from the country | Read from the province | Answer |
| --- | --- | --- | --- |
| 0, own | `0x7c` `MODIFIER_TOLERANCE_OWN` | nothing | the value, at most 1,000 |
| 1, heretic | `0x7d` `MODIFIER_TOLERANCE_HERETIC`; cap `0x1b8` `MODIFIER_TOLERANCE_OF_HERETICS_CAPACITY` | `0x266` `MODIFIER_LOCAL_TOLERANCE_OF_HERETICS` | the lesser of the sum and the cap |
| 2, heathen | `0x7e` `MODIFIER_TOLERANCE_HEATHEN`; cap `0x1b9` `MODIFIER_TOLERANCE_OF_HEATHENS_CAPACITY` | `0x267` `MODIFIER_LOCAL_TOLERANCE_OF_HEATHENS` | the lesser of the sum and the cap |

The names are the localisation keys the modifier table gives each modifier where it is made (`0x8d84a0`: the modifier's number at `[rbp+0x288]`, its key in `r8`, before each `0x8f0810`).

**The religion as the country's own**, `0x36e300`(country, religion, other): yes for the state religion (`country+0xf30`) and for the religion at `country+0xf38` when its method `+0x40` answers yes (_inferred_: a syncretic or secondary religion). With the expansion that harmonizes, `0xa7eed0` (the state religion's byte `+0x11a`) leads to two more checks: the religions the country has harmonized (`country+0x1060`..`+0x1068`, by `religion+0x70`, `0x3ec320`) and their groups (`0x3ec3c0`). It only reads, apart from a thread's first use of the expansion check, which initializes a thread-local value (`0x1a334c0`).

**The modifier table's objects.** The table at `[0x2358080]` holds 0x287 entries of 0x60 bytes, one for each modifier. At `+0x40` an entry holds a pointer to an object, null unless something put one there. `0x300720` passes each value it reads through that object's method `+8`, which gets the set the value came from. The test of a modifier that is on or off (`0x8e7b60`) asks the same object's method `+0x10` instead, which can map the modifier to another. Which modifiers have such an object, and what puts it there, was not found. The modifier table's maker does not, and nothing of the 605 functions that use the table stores into an entry's `+0x40` as far as a search of their stores found.

## Who asks

Nine direct calls (`0x1739e5`, `0x173dfd`, `0x17437d`, `0x300a8d`, `0x3d341c`, `0x6de5e1`, `0x6dfc06`, `0xc0cd6e`, `0xc1a34d`), and `0x3003b0` once more from `0x95f050`:

| Who | When | With a province |
| --- | --- | --- |
| `0x300a70`, **religious unity's step** (below), from the knowledge refresh `0x93fe30` → `0x93fb80` and from `0x3ec460`, `0xa28cf0` and `0xfdf5b0` | every knowledge refresh, once for each of the country's provinces | yes |
| The AI's religion plan (`0x173180` → `0x173370` → `0x1738f0`, `0x173d30`; [ai.md](ai.md), the religion plan's think every 1 to 3 days) | for each of the AI country's provinces, before any other test, and again for the provinces it weighs for missionaries | yes |
| The same plan's `0x1741d0` | once a think | no |
| `0x3d32b0`, from `0x3839b0` and `0x9e3210` | whether the tolerance is above 0 | no |
| `0x6de5b0`, `0x6dfbf0`, `0xc0cce0`, `0xc1a010`, `0x95f050` | reached only through tables of function pointers (`0x1c9f6c0`, `0x1ced1c8`, `0x1d15f40`, `0x1cd0430`), the last two comparing the tolerance with a value: _inferred_, script triggers and values. `0x6de5b0` asks for the province's religion by its owner, with the province; the others without one | `0x6de5b0` only |

**Religious unity** (`0x93fb80`, in the knowledge refresh, [country-knowledge.md](country-knowledge.md)): for each of the country's provinces (`country+0x1338`, `+0x1344` counted) but those its flags at `+0x3f8` and `+0x958` leave out, it adds the province's weight `0xa28d70`(province) to `country+0xee8`. It adds the weight times `0x300a70`'s factor to `country+0xee4`, and stores their ratio, at most 1, at `country+0xee0`. `0x300a70`(country, out, province, religion) asks `0x2ffc60` for the tolerance of the province's religion (`province+0x448`), then `0x36e300`: for a religion the country takes as its own, the factor is 1 whatever the tolerance. Otherwise it is 1 above a tolerance of 2, 0.75 above 1, 0.5 above 0, 0.25 above −1, and 0 below. _Inferred_ from what it works out: the share the game shows as religious unity, the weight the province's development.

## What it costs

In the sample of [the run](../prototype-log/2026-11-11-five-candidates-timed.md) (1780.7.1, 180 s, all threads): `0x2ffc60` 47 samples, 46 of them through religious unity on the main thread and one through the religion plan; 0.6% of the main thread's samples, 0.70 ms a day at that run's pace; 8.5% of the knowledge refresh's 541 samples (13% in the sample of [country-knowledge.md](country-knowledge.md)). Inside it: its own code (the copy's loops) 24, the heap 9, the shared pointers' copy (`0x8f4d70`) 9, the reading (`0x3003b0`) 4.

How many calls, and for which religions, the call trace counted ("Timed in the game"): about 730 calls a game day on the main thread, 68% of them through religious unity, and about 68% of the main thread's values asked for a religion the country takes as its own.

## What it reads and what changes it

It reads the province's modifiers (`+0x998`), its religion; the country's modifiers, its state and secondary religions, its harmonized religions; the religion's group; the modifier table's objects. It writes nothing but its stack, the heap blocks of its copy (made and freed in the call) and the counts of the shared pointers, which it puts back.

What it reads changes when a province's modifiers are recounted ([monthly-update.md](monthly-update.md): a province's month, and events and buildings), when a country's modifiers are made again (its daily update, `0x327dc0` → `0x93d130`, [country-daily-update.md](country-daily-update.md)), and when a country changes religion or harmonizes one. Nothing of it changes within one call. Neither way below keeps anything between calls, so nothing needs letting go.

## Ways to make it faster

1. **No tolerance where religious unity does not read it.** In `0x300a70`, ask `0x36e300` first and give 1 at once for a religion the country takes as its own, without `0x2ffc60`. The factor is the game's, since `0x300a70` gives 1 then whatever the tolerance. What is left out writes nothing of the game's state. The one thing it could otherwise do is a log line from `0x93d750`, for a country whose modifiers are being worked out at that moment. The part rests only on `0x300a70`'s code. Its gain is the share of a country's provinces whose religion it takes as its own, which is most of them for most countries but was not counted (below).
2. **The province's set read where it is.** At every call, `0x3003b0` given `province+0x998` instead of a copy. `0x300720` reads the set's `+0x10`, `+0x58` and `+0x64`, and the copy's pairs are the original's in the same order without the empty ones. So for a set kept as pairs the first pair with `0x266` or `0x267` is the same, and for a set kept by modifier its place for them is too, provided an empty place holds a value of 0, as the code that fills a set kept by modifier writes one (`0x2ffde8`, the modifier `0x286` and the value 0 together). It differs only where the modifier table's object for `0x266` or `0x267` exists, since that object is handed the set itself: the copy has another vtable, the second one, and no byte `+0x10`. So the part is exact if both objects are null, which a startup check can read once the modifier table is made, leaving the game's copy in place when either is not. Whether a province's set is ever kept by modifier was not seen; the reading handles both.
   - Its gain is the whole copy, 43 of the 47 samples, at every caller, the religion plan's threads included.
   - On those threads it also saves the atomic counts of the shared pointers. Each AI country copies only its own provinces' sets, but the triggers may copy any.
3. **Thread safety.** Both ways read less than the game's own code, and the same objects. The copy reads the province's set without a lock too, so reading it in place adds no race the game does not already have.

Of the ways, (1) is cheap and needs no check. (2) covers every caller but rests on the table's objects being null, which is to be read in the running game.

## Timed in the game

The call trace's `times` ([dev-tools.md](../dev-tools.md), "Call tracing") from 1780.7.2 to 1784.11.24, about 1,600 days ([the run](../prototype-log/2026-11-11-five-candidates-timed.md)):

| Function | Calls | On the main thread | What it tells |
| --- | --- | --- | --- |
| `0x2ffc60` (4 arguments) | 4,706,942 | 1,173,253 | the tolerance asked: about 730 a day on the main thread, the rest on the AI's threads |
| `0x300a70` (4) | 831,662 | 802,598 | religious unity's factor: 68% of the main thread's tolerances |
| `0x3003b0` (4) | 4,885,918 | 1,352,229 | the value read |
| `0x3004a0` (4) | 2,261,153 | 431,357 | read only for a religion not the country's own: 46% of all values, 32% of the main thread's |
| `0x93fb80` (1) | 19,224 | 19,224 | religious unity's step, about 12 a day |
| `0x1738f0` (1) | 75,360 | 7,667 | the religion plan's walk, mostly on the AI's threads |

Every timed call takes the tool's lock, and the functions called on many threads at once waited for it: the religion plan's 15.3 s of the AI's threads under the trace was one sample of 18,151 in the sampled run. So the milliseconds are the sample's, 0.70 ms a game day on the main thread at that run's pace, and the counts the trace's.

So way (2) could take up to the copy's 43 of 47 samples, about 0.64 ms a day of the main thread, and way (1) about two thirds of religious unity's share of it; together at most about 0.65 ms, 0.6% of the day. The AI's threads' share is not on the main thread's path: the main thread's waits held none of it.

## Not known

- What objects the modifier table holds at `+0x40`, for which modifiers, and what puts them there.
- How many of religious unity's own calls are for a religion the country takes as its own: the count above is of every caller on the main thread.
- What `0x777b80`, the other caller of religious unity's step, is, and how often it runs.
- The triggers and values behind the tables of function pointers, by name.
