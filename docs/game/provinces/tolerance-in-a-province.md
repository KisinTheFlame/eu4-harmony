# A country's tolerance of a religion in a province

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
| The AI's religion plan (`0x173180` → `0x173370` → `0x1738f0`, `0x173d30`; [ai.md](../ai/ai.md), the religion plan's think every 1 to 3 days) | for each of the AI country's provinces, before any other test, and again for the provinces it weighs for missionaries | yes |
| The same plan's `0x1741d0` | once a think | no |
| `0x3d32b0`, from `0x3839b0` and `0x9e3210` | whether the tolerance is above 0 | no |
| `0x6de5b0`, `0x6dfbf0`, `0xc0cce0`, `0xc1a010`, `0x95f050` | reached only through tables of function pointers (`0x1c9f6c0`, `0x1ced1c8`, `0x1d15f40`, `0x1cd0430`), the last two comparing the tolerance with a value: _inferred_, script triggers and values. `0x6de5b0` asks for the province's religion by its owner, with the province; the others without one | `0x6de5b0` only |

**Religious unity** (`0x93fb80`, in the knowledge refresh, [country-knowledge.md](../countries/country-knowledge.md)): for each of the country's provinces (`country+0x1338`, `+0x1344` counted) but those its flags at `+0x3f8` and `+0x958` leave out, it adds the province's weight `0xa28d70`(province) to `country+0xee8`. It adds the weight times `0x300a70`'s factor to `country+0xee4`, and stores their ratio, at most 1, at `country+0xee0`. `0x300a70`(country, out, province, religion) asks `0x2ffc60` for the tolerance of the province's religion (`province+0x448`), then `0x36e300`: for a religion the country takes as its own, the factor is 1 whatever the tolerance. Otherwise it is 1 above a tolerance of 2, 0.75 above 1, 0.5 above 0, 0.25 above −1, and 0 below. _Inferred_ from what it works out: the share the game shows as religious unity, the weight the province's development.

## What it reads and what changes it

It reads the province's modifiers (`+0x998`), its religion; the country's modifiers, its state and secondary religions, its harmonized religions; the religion's group; the modifier table's objects. It writes nothing but its stack, the heap blocks of its copy (made and freed in the call) and the counts of the shared pointers, which it puts back.

What it reads changes when a province's modifiers are recounted ([monthly-update.md](../day/monthly-update.md): a province's month, and events and buildings), when a country's modifiers are made again (its daily update, `0x327dc0` → `0x93d130`, [country-daily-update.md](../day/country-daily-update.md)), and when a country changes religion or harmonizes one. Nothing of it changes within one call. Neither way below keeps anything between calls, so nothing needs letting go.

## Not known

- What objects the modifier table holds at `+0x40`, for which modifiers, and what puts them there.
- How many of religious unity's own calls are for a religion the country takes as its own: the earlier measurements covered callers together on the main thread.
- What `0x777b80`, the other caller of religious unity's step, is, and how often it runs.
- The triggers and values behind the tables of function pointers, by name.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x2ffde8` | code | Referenced while examining ways to make it faster; complete role not established |
| `0x3004a0` | function | Referenced while examining timed in the game; complete role not established |


The native dense-set initialization at `0x2ffde8` writes an empty pair as modifier `0x286` with value zero. This is a representation fact used by the copy path, independent of a proposed optimization.
