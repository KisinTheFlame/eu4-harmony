# Each country's daily update

Sampled with `cargo xtask profile`, the 1780.7.1 save in observe mode in the Ottomans' view at speed 5, 240 s; each country's daily update was 19.6% of the main thread, 13.2 ms a game day, of which its own code 0.19.

| What it calls | ms a game day | What it is in the game |
| --- | --- | --- |
| `0x1a2d30`, an army's method `+0xd0` | 3.42 | Each army's day: its unit's daily update, then its AI's update of the threat map |
| `0x93fe30` | 1.66 | The country's knowledge refreshed when asked for ([country-knowledge.md](../countries/country-knowledge.md)) |
| `0x90e410`, a fleet's method `+0xd0` | 1.40 | Each fleet's day: its unit's daily update and the fleet's own |
| `0x37c730` (`UpdateEstates`) | 1.39 | The estates: privileges no longer valid, reforms, the crown land |
| `0x93c790`, then `0x93ca70` twice | 1.37 | The AI's sums of the country's strength |
| `0x327dc0` | 0.84 | The country's modifiers made again |
| `0x182580` | 0.47 | The AI's strategy, when it is due ([ai.md](../ai/ai.md), "A day of the AI") |
| `0x18e7f0` | 0.34 | The AI's money set aside for the day ([ai.md](../ai/ai.md)) |
| `0x38b1f0`, `0xa855b0` | 0.34 each | Not read |

## The units' days

An army's method `+0xd0` (`0x1a2d30`, vtable `0x1c42540`) calls its unit's daily update (`0xd3e110`, `CUnit::DailyUpdate`, [time.md](time.md)) first, then, past some checks of its own state (`+0x207`, `0x1a2a50`, `0x1a8c10`), its AI's update of the threat map; a fleet's (`0x90e410`) calls the unit's daily update and the fleet's own. Of the 3.18 ms in the units' daily updates:

- **Moving** (`0xd51520`, `unit.cpp`, "CancelMovement", "CancelAllOfPathButNext"), 2.0 ms: a unit on a route steps along it, and one that reaches the next province is put there (`0xd3fd30`, "SetLocation, %u, %d": the location `+0xe8`, the interface, the provinces' lists, the rebels' messages, [armies.md](../military/armies.md)), 1.4 ms of it.
- `0x90f740` → `0x91c2f0`, 0.66 ms, in the fleets' updates: not read.

**The threat map** (`0x242ba00`, which the AI stage reads, [ai.md](../ai/ai.md)): `0x195e10(map, army)` finds the army's record in the map's table (`+0x30`, by the army's ID), counts it and stamps it with the date (`0x195e86`), gathers the provinces the army stands in and is heading for (its location `+0xe8`, its route `+0x100`), and from each looks over its neighbors (`0x195750`, whose sole caller in `eu4.exe` is `0x19680a` inside `0x195e10`; calling the route edge check `0x96f860`, `0xd4f4a0`), writing what it finds into the map (`0x1990f0`, `0x199ef0`). It is called from the army's method after its unit's day (1.08 ms sampled) and from `SetLocation` when a unit arrives in a province, while a setting of the map manager allows (`[world+0x1e00]`'s method `+0x80`, its bytes `+0x54` or `+0x55`; 1.19 ms sampled).

- **Counted** in the game: 34.8 updates a game day, about 100 µs each, all on the main thread; **1.7% were of an army already updated that day**, 0.05 ms a day, and 5% of those had moved since. So an army is not updated twice on a day it moves, as its two callers suggested: what updates cost is each update's look over its neighbors, not repeats.
- What the threat map is to the AI, and why an army's day updates it rather than the AI stage, was not followed; [ai.md](../ai/ai.md) has the stage rebuilding it before the AI's tasks (`0x1950e0`), which reads dates more than it rebuilds, and was not followed either.

## The AI's sums of the country's strength

While the AI runs (`world+0x7484`, checked at `0x2ee4cc`), the daily update calls `0x93c790` and `0x93ca70` twice (with 0 and 1) on the AI's part of the country (`country+0x28c0`), and keeps the three sums at `country+0x96c0`, `+0x96c8` and `+0x96d0` (`0x2ee4e6`..`0x2ee526`). `0x93c790` walks the country's units (`+0xd18`) and works out each army's strength with `0xd46800` (the strength the support check also uses, [ai-stage-wait.md](../ai/support-checks.md)), 1.03 ms of it in `0xae0f00` (`0xae01d0`, `0xae1330` → `0xae7600` → `0xaedf20`, `0x341820`, `0x3c77c0` → `0x3c74a0`), and scales hired mercenaries using `country+0x17a0`/`+0x17ac` (hire date `+0x20`). Its callees are read-only once `0x14d4c80` (its static logger singleton) is initialized and `country+0x28c8` is not in `EState::CALCULATING` (`0x93d750` returns `country+0x28c8` without mutating it); they read `country+0x9878` (max morale), `country+0x2440` (`+0x38..+0x5c` fire/shock/tactics), `country+0x28d0` (modifier set), `country+0x210`/`+0x21c` (special-unit modifiers), `country+0x17a0`/`+0x17ac`, and each army's regiments (`+0x20` type, `+0x28` strength, `+0x2c` morale, `+0x30` drill, `+0x64` special unit category). In `0x75b0d0`, the active-country loop begins at `0x75b1f3` (body `0x75b1fc`, end `0x75b242`), allowing `0x93c790` to be precomputed in parallel across active countries before the loop with per-country fingerprinting to catch any country mutated by an earlier country's `0x2ee280`.

## The estates

`0x37c730` (`country.cpp`, "UpdateEstates", "EstateAdd %s %d %d", " crownland changed to "), 1.39 ms: `0x723120` (0.70 ms), `0x7b2940` (0.48 ms, the strings "REFORM", "REFORM_DESELECTED") and `0x71e000` (estates.cpp, "PRIVILEGE_NO_LONGER_VALID", "_EFFECTS", "_NO_EFFECT"). Their time goes to the script's conditions: the "or" (`0xb985b0`, true when any child is) and "nor" (`0xb98910`, true when none is) blocks of [triggers.md](../scripts/triggers.md), through `0xd135c0` → `0x7a6020`. So each day every country checks its estates' privileges and its reforms against their conditions, to take back what is no longer allowed.


## Not verified

- `0x38b1f0`, `0xa855b0`, `0x90f740` → `0x91c2f0`, `0x723120`'s and `0x7b2940`'s own work beyond their strings and the conditions they evaluate.
- What reads the strength sums at `country+0x96c0`..`+0x96d0`; what the threat map is to the AI, and the setting that lets `SetLocation` update it.
- One save, in observe mode; a game a player plays; the experiment's own cost in its 100 µs.
