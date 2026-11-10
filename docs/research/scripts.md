# Flags, variables and firing events (for `game::script`)

What the scripts' flags and variables are kept in, how the effects that set them and the effects that fire events do it, and what the framework calls to do the same: the `script` group (`script.flag`, `script.set_flag`, `script.clear_flag`, `script.variable`, `script.set_variable`, `script.fire_event`; [plans/mod-capabilities.md](../plans/mod-capabilities.md), M10 to M12).

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-11-11; what was checked in the running game is marked "tested" ([the run](../prototype-log/2026-11-11-script-flags-variables-events.md)).

## How an effect is found

- **Names are tokens.** The token table in `.data` holds records of `0x208` bytes, a token's number at `+8` and its name from `+0xc`: `set_country_flag` is `0x2735`, `clr_country_flag` `0x2736`, `set_province_flag` `0x2737`, `clr_province_flag` `0x2738`, `set_global_flag` `0x2739`, `clr_global_flag` `0x273a`, `country_event` `0x278f`, `province_event` `0x2790`, `set_variable` `0x27b3`, `change_variable` `0x27b4`, `set_ruler_flag` `0x2ffb`, `clr_ruler_flag` `0x2ffc`, `set_heir_flag` `0x34d4`, `clr_heir_flag` `0x34d6`, `set_consort_flag` `0x34d7`, `clr_consort_flag` `0x34d9`. The triggers `has_country_flag` (`0x29cb`), `has_province_flag`, `has_global_flag`, `has_ruler_flag`, `has_heir_flag`, `has_consort_flag`, `had_country_flag` and `check_variable` are tokens too; there is no `set_global_variable`: the game has no global variables.
- **The effect factory** `0x5dec50(container, reader, token)` ([architecture.md](architecture.md), "Effects") compares the token, then jumps through tables: for `0x2711` to `0x28cb` the byte table at `0x5e5250` and the offsets at `0x5e4f34`; for `0x2f36` to `0x3007` `0x5e546c` and `0x5e540c`; for `0x34c8` to `0x357d` `0x5e5ae8` and `0x5e5a98`. Each case allocates the effect and calls its constructor, which writes its vtable; the effect's execute is at vtable `+0x78`.

| Effect | Constructor | Vtable | Execute |
| --- | --- | --- | --- |
| `set_country_flag` | `0x5e8640` | `0x1c99138` | `0x611870` |
| `clr_country_flag` | `0x5e8680` | `0x1c996b8` | `0x611ca0` |
| `set_ruler_flag` | `0x5e86c0` | `0x1c99758` | `0x612030` |
| `clr_ruler_flag` | `0x5e8700` | `0x1c997f8` | `0x612400` |
| `set_heir_flag` | `0x5e8740` | `0x1c99898` | `0x6127d0` |
| `clr_heir_flag` | `0x5e8780` | `0x1c99438` | `0x612b80` |
| `set_consort_flag` | `0x5e87c0` | `0x1c994d8` | `0x612f20` |
| `clr_consort_flag` | `0x5e8800` | `0x1c99578` | `0x6132f0` |
| `set_province_flag` | `0x5e8840` | `0x1c99618` | `0x6136b0` |
| `clr_province_flag` | `0x5e88c0` | `0x1c98918` | `0x613a20` |
| `set_global_flag` | `0x5e8900` | `0x1c989b8` | `0x614100` |
| `clr_global_flag` | `0x5e8940` | `0x1c98a58` | `0x614130` |
| `country_event` | `0x5ea190` | `0x1c94b18` | `0x660eb0` |
| `province_event` | `0x5ea1f0` | `0x1c94bb0` | `0x661d00` |
| `set_variable` | `0x5eae90` | `0x1c92bb0` | `0x663200` |

## Flags

- **A flag effect** (`0x60` bytes) keeps the flag's name, a `std::string`, at `+0x40`, and an id it has looked up at `+0x38` (u16). Its vtable's `+0x98` (`0x611680` for `set_country_flag`) answers the id: the name, with any saved scope's name put in for it (`0x5ed880`, `0x5edad0`), looked up by `0x737ea0`.
- **A flag's id** is its name's place among every flag name the game has met: `0x737ea0(&name) -> u16` takes a lock (`0x737120`, `0x1581820`), looks the name up in a hash map by its FNV-1a hash (`0x738960`; the map's end at `[0x242bc38]`), and when the game has not met it yet gives it the next id (`[0x242bc40]`), adds it to the map (`0x738480`) and appends the name to the names, a `std::vector` of `std::string`s at `0x2357d80` (`0x1f6b20`). The id is the name's place in that vector: the logging of `set_country_flag` reads the name back as `[0x2357d80] + id × 0x20` (`0x611c12`).
- **A flag set** is one layout wherever it is kept: `+0x10` a `std::vector` of u16 ids (begin, end at `+0x18`, capacity `+0x20`) and `+0x28` a `std::vector` of i32 days beside them (end `+0x30`, capacity `+0x38`), the day a flag was set at the same place as its id.
  - **Setting** is `0x7371f0(set, id, day)`: when the id is there, its day is overwritten; otherwise the id and the day are appended. A day before the define at `0x1fc2354` (compared by `0xcfd70`, which works out its year) is replaced by today (`[[0x233fe78] + 0x1dd0]`). Every `set_*_flag` passes `0x29c77f8`, which is 1.1.1 in the game's dates (hours from 5000 years before year 0, `0x29c55c0` being year 0), so a flag is set on today's date. Tested: a flag set from remote control read back as the test game's date.
  - **Clearing** is `0x7372f0(set, id)`, a leaf: it finds the id, moves the last id and the last day into its place and shortens both vectors; nothing when it is not there.
- **Where each scope keeps its flags**, from the executes:
  - a country, `country + 0x270` (`0x611bd7`), the country found from the scope's tag (`[[0x233d8d0] + 0x118] + index × 8`);
  - a province, `[[0x233fe78] + 0x1ca8] + id × 0x2e10 + 0x1b8` (`0x6139cc`);
  - the game, `[0x233fe78] + 0xb38` (`0x61410d`);
  - a ruler, `[country + 0x1280] + 0xa0` when the ruler's vtable `+0x58` answers true, and `country + 0x2b0` when it answers false, a country without a ruler keeping its ruler's flags itself (`0x612381`–`0x6123c8`);
  - an heir, `[country + 0x1298] + 0xa0` when its `+0x58` answers true, and nowhere otherwise: the effect does nothing (`0x612b17`);
  - a consort, the ruler when the ruler's vtable `+0x60` answers true, else `[country + 0x12b0]`, then `+0xa0` of it when its `+0x58` answers true (`0x613269`–`0x6132b6`).
- **The monarchs' vtables** ([names.md](names.md)): `+0x58` is `0x8f440` (true) for a monarch made from a history (`0x1cdcdd8`), an heir (`0x1c52240`) and a consort (`0x1c520c0`), and `0x8f450` (false) for `0x1cdcd40`, the placeholder a country keeps where it has no one. `+0x60` is false but for the consort's class, where it is `0x296ca0`, `[monarch + 0x78] != 0`: the regency's estate is set, the consort ruling as regent.

## Variables

- **`set_variable`** (`0x80` bytes): its variable's name, a `std::string`, at `+0x38`, the name of another variable whose value it copies at `+0x58` (`which` and `value = <name>`), its value at `+0x78`, i32. Its execute (`0x663200`) finds the scope's variables with `0xd7c9a0(scope, &name, "CSetVariableEffect::Execute")`, takes the other variable's value when it names one (`0xd7ce90`), and sets the value with `0xd7c450(variables, &name, &value)`.
- **Where they are kept** (`0xd7c9a0`): the country's when the scope has one, `country + 0x330` (`0xd7cc81`), else its province's, `province + 0x238` (`0xd7cca8`); with neither, it looks among saved scopes (`0x1fd5fa8`) and logs "Failed to find variable" when it finds none.
- **The variables** are an `std::unordered_map` from a name to an i32 at `+0x10`: its items' list head at `+0x18`, its buckets at `+0x28`, its mask at `+0x40` (`0x1fd1a0`, the lookup by the name's FNV-1a hash). An item has the next at `+0`, the previous at `+8`, the name, a `std::string`, at `+0x10`, and the value at `+0x30` (`0xd7c3b0`, the read, which answers 0 for a name it does not find).
- **The value is in thousandths**: tested, a variable the scripts set to 2.5 read 2500, and one set to 12.25 through the framework was what `check_variable = { which = … value = 12 }` let through.
- `0xd7c450` inserts the name when it is not there (`0xd7da20`), copying it, and writes the value.

## Events

- **`country_event`** (`0x88` bytes): the event's number at `+0x38`, the days to wait at `+0x60` and a random number of days more at `+0x64`. Its execute (`0x660eb0`):
  1. refuses a scope without a valid country, as every country effect does;
  2. makes a scope of its own on the stack (`0xd0` bytes, vtable `0x1c38158`): the country from the scope it runs in, no province, a random seed from that scope's, and that scope as the new one's FROM;
  3. finds the event in the events, an `std::unordered_map` from an event's number to the event, a pointer at `0x2340308`: its items' list head at `+8`, an item's number at `+0x10` and its event at `+0x18`, found by the FNV-1a hash of the number's four bytes;
  4. without days to wait, asks the event's trigger, a content trigger at `event + 0x40` ([triggers.md](triggers.md): its conditions at `+0x48`, their count at `+0x54`), each condition's evaluate (`+0x30`) in the new scope, and fires the event only when every one holds: `0x6f4f40(0x6f2fa0(), event, scope)`;
  5. with days to wait, puts it in the country's delayed events (`0x37ef40`).
- **`province_event`** (`0x661d00`) is the same, but its scope has the province the effect runs in and the tag `---`, so that the event is the province's owner's ([questions-and-offers.md](questions-and-offers.md), "Whose it is").
- **`0x6f2fa0()`** returns the event manager (`[0x242bbf0]`), made when there is none; **`0x6f4f40(manager, event, scope)`** is "TriggerEvent" ([questions-and-offers.md](questions-and-offers.md)). It does nothing when the event's byte `+0xa5f` is set and `0x74b2d0(world)` says so, or when the world's byte `+0x7484` is clear, and logs "TriggerEvent, %u" with the event's number. It copies the scope into the pending event it makes, so the caller's scope may go once it returns.
- **The events' ids**: the event keeps its id, as its file writes it, at `+0x20` ([questions-and-offers.md](questions-and-offers.md)); the effects keep the number the id was read into, so the framework finds an event by its id by walking the events and comparing each one's.

## What the framework does

- **A flag** is read without the game's lock: the names are walked for the name, which gives its id when the game has met it, and the scope's flag set is walked for the id. A name the game has not met is no flag of any scope's, and asking for it adds nothing. Setting asks `0x737ea0` for the id, as the effects do, which adds the name the first time, and calls `0x7371f0` with the day `0x29c77f8`; clearing calls `0x7372f0` when the name has an id. The scope's flag set is found as the effects find it, monarchs' through their vtables; an heir or a consort a country has not is no flag set, so reading answers none, clearing does nothing, and setting is refused.
- **A variable** is read by walking the scope's map for the name, so that a variable set to 0 is told from none; set with `0xd7c450`, from a value rounded to thousandths.
- **An event** is found by its id among the events, its trigger asked with `game::conditions`' evaluate in a scope `game::conditions` makes (the country and no province, or the province and `---`), and fired with `0x6f4f40` through `0x6f2fa0`, as the effects do with no days to wait. The framework's scope has no FROM, and its random seed is the scope maker's.
- Every function, global and field above is a dependency point in `game::script`'s points, each with an instruction of the game's own that uses it.

## Not established

- What the define at `0x1fc2354` is (it holds 1.1.1 in the exe and is overwritten when the defines are read); the flag read back as today's date shows that 1.1.1 is before it in a game.
- Whether anything else than flag effects adds names to the flag names, and whether the game ever removes one; the framework only reads them.
- The lock `0x737ea0` takes, and whether any thread but the main one sets flags; the framework reads the names unlocked at its safe point, on the main thread.
- `change_variable` and the other variable effects were not read; the framework only reads and sets.
- What `0x74b2d0` and the event's byte `+0xa5f` stand for; the framework fires as the effects do and leaves them to `TriggerEvent`.
