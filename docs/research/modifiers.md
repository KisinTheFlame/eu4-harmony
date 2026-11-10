# Modifiers: their kinds, sets of them, and a country's (for `game::modifiers` and `game::countries`)

What the game keeps of its modifiers and how it reads one: the kinds built into the code and their keys, a set of modifiers' values and its getter, and the sets a country's modifiers are made of, among them what legitimacy adds. The framework reads them for `country.get`'s legitimacy and named modifiers and for `country.modifiers` and `province.modifiers` ([plans/mod-capabilities.md](../plans/mod-capabilities.md), M3 and M4); `modifier_sums` speeds up the adding of sets ([late-game.md](late-game.md#adding-sets-of-modifiers)).

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with `cargo xtask disasm` and scripts over the PE file (the token table, call sites); what was read back in the game is marked "tested". What is marked _inferred_ was concluded from the shape of the code.

## The kinds of modifier

- **Built into the code**, 0x287 of them: `0x8d84a0` makes them once, at the vector at `0x2358080` (end `0x2358088`), with `0x8f46f0`(vector, 0x287), then registers each with `0x8f0810(&token, &index, localisation key, &flags)`: for example the token `0x2857` (`build_cost`), index 6, `BUILD_COST`, flags `0x95`. Mods cannot add kinds: a key the game does not know names none.
- **A record** is 0x60 bytes, at the index × 0x60 (`0x8f09ea` `48 8d 0c 5b 48 c1 e1 05 48 03 0d …`, `lea rcx, [rbx+rbx*2]; shl rcx, 5; add rcx, [0x2358080]`): at `+0` the key, a `std::string` copied from the token's name in the token names (`[0x242b888]` + token × 0x20, `0x8f08e5`); at `+0x20` its localised name; at `+0x40` an optional object whose method at `+8` turns a raw value into the one the getter answers; `+0x50` the token; `+0x54` the index (`0x8e7b83` `47 8b 54 08 54`); `+0x5c` flags (`0x40` a yes-or-no modifier, `0x8e7bab`).

## A set of modifiers

- **A set** has a vtable at `+0` and keeps its values either by modifier (the byte `+0x10` set, for a modifier below 0x286) or as pairs. Either way an array of 8-byte pairs at `+0x58`, a modifier's index (i32) then its value (i32 thousandths), and their count (i32) at `+0x64`; kept by modifier, pair *i* is modifier *i*'s, and a place holding none holds the index 0x286 (`0x92e5d9` `cmp dword ptr [r8], 0x286`). The sets a set has added are a vector at `+0x38`, whose pointers it does not own.
- **The getter of one modifier**, `0x8e6bd0`(set, out, modifier): kept by modifier, the value at the modifier's place (`0x8e6be2` `41 81 f8 86 02 00 00 7f 0e 48 8b 41 58 4d 63 c8 46 8b 54 c8 04`); otherwise the first pair with the modifier, searched from the first (`0x8e6bf9` `48 63 41 64 …`), or 0. Then, when the kind's record has its object at `+0x40`, that object's method `+8`(object, out, set, &raw) answers instead (`0x8e6c3e`). `0x8e7b60`(set, modifier), whether a yes-or-no modifier is set, calls it.
- **Copying, scaling and adding sets**: `0x8ea2d0`(set, other) copies another set's values into a set, `0x8ea920`(set, &scale) multiplies them by a thousandths scale, `0x8ea1e0`(set, other, &scale) adds a scaled other set, and `0x8ea420`(set, added) adds one set to another ([late-game.md](late-game.md#adding-sets-of-modifiers)).

## A country's modifiers

- **The country's modifiers** are inline at country `+0x28c8`; `0x93d750`(country + `0x28c0`) makes them again first when they are out of date (`country_cached_data.cpp`) and answers them. Their own set, the values the game's getters read, is at `+8`; at `+0x6440` the sets they copy from others.
- **Sets of their own for what one source adds**, each 0xc0 bytes, made by `0x8e6120` from `+0x2f08` on (`0x93a991`–`0x93a9ec`: `+0x2f08`, `+0x2fc8`, `+0x3088`, `+0x3148`, `+0x3208`, `+0x32c8`, `+0x3388`, `+0x3448`), and `neighbor_bonus` at `+0x3508`. Each is filled as the modifiers are made again and added to the own set.
- **What legitimacy adds** is the set at `+0x2f08`, which the court view's tooltip of legitimacy, republican tradition and horde unity describes (its method `+0x50`, `0xb87638`, `0xb87bbe`, `0xb8887e`). `0x92e420` fills it, the static modifiers it scales being at the static modifiers' `+0x690` to `+0x6d0`:
  - meritocracy (`0x7ac710`): the static modifier at `+0x6a0` scaled by the meritocracy × 10 / 1000 − 500 thousandths, and the one at `+0x6b0` scaled by what is left below 500, added into it with `0x8ea1e0`; the set is added in every case.
  - otherwise devotion (`0x7ac600`), a horde (`0x7ac7f0`, horde unity through `0x362990`) or a monarchy (`0x7ac1a0`, legitimacy through `0x362b70`): the static modifier at `+0x690`, `+0x6d0` or `+0x6c0` scaled by the value × 10 / 1000 − 500 (`0x92e7af` `81 c3 0c fe ff ff`), so from −50% at 0 to +50% at 100, into `+0x2f08` (`0x92e7ba` `49 8b 3e 48 81 c7 08 2f 00 00`), then added; at exactly 50 the scale is 0 and the function returns before it, so the set keeps what it last held and adds nothing.
  - `0x92e810`, a republic without meritocracy (`0x7ac230` or `0x7ac3e0`): the static modifier at `+0x8f0` scaled by the republican tradition × 10 / 1000 into `+0x2f08` (`0x92e949`), and the one at `+0x900` scaled by 1000 less that into `+0x2fc8` (`0x92e999` `48 8b 1f 48 81 c3 c8 2f 00 00`), each when its static modifier is one (its method `+0x90`), both added.
  - So what legitimacy adds now is the pairs of `+0x2f08`, and a republic's of `+0x2fc8` too; for a legitimacy, horde unity or devotion at exactly 50, nothing, whatever `+0x2f08` still holds.

## A province's modifiers

- **A province's own set** is inline at province `+0x998`: the game's getter reads it for local values (`0xa1091c` `49 8d 8d 98 09 00 00 e8 …`, one of 42 calls of `0x8e6bd0` on it), and the console's `province_modifier <id> <type>` (`0xe54ea0`) prints its value of a kind, walking its pairs (`+0x58`, `+0x64`). What the owner's modifiers add across the country is in the country's own set, not here.
- The console's `country_modifier <tag> <type>` prints a country's.

## Named modifiers

- **A named modifier** is the record of one a script put on a country or a province (`add_country_modifier`, `add_province_modifier` and the like): at `+0x30` the named set of modifiers it gives, a [modifier set](#a-set-of-modifiers) of the class `0x1c42ae0` whose key is a `std::string` at `+0x158` (with bytes `+0x150` to `+0x152` and an i32 `+0x154` beside it, copied with it, `0x1b1f00`); `+0x40` `start_date`, `+0x44` the day it ends, `date`, written unless it is `0x29c77f8`; the bytes `+0x48` `ruler_modifier`, `+0x49` `parliament_modifier`, `+0x4a` `permanent` and `+0x4b` `hidden`. Its writer is `0xb35b80`'s function (`0xb35bc3` `48 8b 5e 30`, `0xb35bd9` `48 8d 93 58 01 00 00`, `0xb35c6a` `81 7e 44 f8 77 9c 02`, `0xb35d18` `0f b6 5e 4a`, `0xb35d40` `0f b6 5e 4b`).
- **A country's** are a list of the game's kind (`objects::list`) from country `+0x1f0`, written as `modifier` (`0x2968`) by the country's writer (`0x2e0046` `49 8b 87 f0 01 00 00`); **a province's** a vector (begin, end) of pointers at province `+0x2f0` (`0x9d893e` `49 8b 8f f0 02 00 00 49 8b 87 f8 02 00 00`). Both writers leave out one whose set has its byte `+0x151` set (`0x9d896a` `80 b8 51 01 00 00 00`); what that byte means was not followed.

## Not established

- Which kind of country the other sets from `+0x3088` on are for.
- What the transform at a record's `+0x40` does for each kind it is set for; the framework reads totals through the getter, which applies it, and raw values from the sets.
- Whether `0x7ac600` and `0x7ac7f0` can both say yes for one government, where `0x92e420` would take devotion and strengthening the government horde unity.
