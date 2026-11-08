# Sieges and forts: barrage, assault, mothballing

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-05 with `core/examples/disasm.rs` and capstone scripts over the PE file; nothing here was tried in the game. What is marked _inferred_ was concluded from the shape of the code. Bytes are given as `disasm bytes:RVA:12`.

A battle fought is in [battles.md](battles.md).

## Objects

### A combat: battle or siege

Battles and sieges share one base class ("combat", `combat.cpp`); its writer `0x22a420` (land and naval) and the siege's `0x23c470` write the same first keys.

| Offset | Type | What | Instruction (bytes) |
|---|---|---|---|
| `+0x10` | id object `{ id type }` | `id` (`0xcd8f0` with token `0xb`); the u32 at `+0x14` is what the game logs as the combat's ID | `0x23a043` `44 8b 4f 14` |
| `+0x20` | side* | `attacker` (`0x28f9`) | `0x403224` `48 8b 42 20 4c 8b 88 08 01 00 00` |
| `+0x28` | side* | `defender` (`0x28fa`) | `0x22a58e` (writer) |
| `+0x30` | province* | `location` (the ID at province `+0x20`) | `0x22a449` `48 8b 47 30 ba 70 28 00 00 48 8b ce 8b 58 20` |
| `+0x38`, `+0x3c`, `+0x40` | i32 | `day`, `duration`, `phase` | writer |
| `+0x44` | u8 | settled (`message`; [defeat.md](defeat.md)'s `SETTLED`) | `0x23a066` `80 7f 44 00 0f 85 e1 04 00 00 c6 47 44 01` |
| method `+0x60` | | kind: **1 land** (`0xe61f0`), **2 naval** (`0xe6240`), **3 siege** (`0xe6250`) | |
| method `+0x88` | | daily update: `0x22c080` for land and naval alike | |

Vtables: land combat `0x1c4a2e0` (type `land_combat` `0x291d`), naval combat `0x1c4a0c8` (`naval_combat` `0x291e`), siege `0x1c4a168` (`siege_combat` `0x291f`); the type answer is slot `+0x58`, the writer `+0x10`.

### A side

Its writer is `0x21b3c0`.

| Offset | Type | What | Instruction (bytes) |
|---|---|---|---|
| `+0x10` | combat* | its combat (`0x21ff04` `mov rcx, [r15+0x10]`, then the combat's `+0x60`) | |
| `+0x18` | i32 | `dice` | writer |
| `+0x1c` | i32, thousandths | `losses`, total (written `%d.%03d` by `0x246bc0`; a regiment's strength 1.000 is 1000 men, so this is men; _inferred_) | `0x22b82a` `41 01 5f 1c` |
| `+0xe8`/`+0xf0`, `+0xf8` | list, i32 | its units, their count ([defeat.md](defeat.md)) | |
| `+0x100`/`+0x108` | vector of 0x20-byte entries | `participating_countries`; an entry's tag at `+0x10`; entry 0 is the side's leading country | `0x40322f` `4c 8b 80 00 01 00 00`, `0x403246` `4d 8b 50 10 49 c1 ea 20` |
| `+0x118`…`+0x130` | 7 × i32, thousandths | `infantry`, `cavalry`, `artillery`, `heavy_ship`, `light_ship`, `galley`, `transport`: the side's composition by kind (when it is set was not read) | writer `0x21b780` (`+0x118`) |
| `+0x134`…`+0x14c` | 7 × i32, thousandths | `losses_type`: losses by kind, same order (kind = regiment type `+0x10`) | `0x22b815` `41 01 9c 8f 34 01 00 00` |
| `+0x168` | u8 | `is_attacker` | `0x21b42b` `41 0f b6 9e 68 01 00 00` |
| `+0x169` | u8 | `overrun` | `0x21b453` `41 0f b6 9e 69 01 00 00` |
| method `+0x98` | | morale, thousandths (`(side, out*)`; the assault check compares with 500) | `0x38134e` `49 8b 02 48 8d 95 c8 00 00 00 49 8b ca ff 90 98 00 00` |

- **Its leading country** `0x21c7a0(side, out tag*)`: the owner of the first unit owned by entry 0's country whose method `+0x90` says so, else entry 0's tag.
- **Its leader** `0x21fe80(side)`: the best leader (unit `+0xd8`) among its units for the combat's kind, else a default leader object (`[0x233fec0]`).
- A unit's regiments (or ships) are pointers from unit `+0x30` to `+0x38`, each with its strength at `+0x28` (thousandths) and its type at `+0x20` (kind at type `+0x10`) (`0x22b7b3`–`0x22b811`). A unit's owner is the tag at `+0xd0` (`0x22b870` `49 8b 85 d0 00 00 00`).

### A siege

The siege's own fields (writer `0x23c470`, besides the combat's): `morale` `+0x50`, `last_assault` (a date) `+0x4c`, `impact` `+0x5c`, **`breach` `+0x60`** (i32), `last_impact` `+0x64`, `total` `+0x68`, `roll` `+0x6c`, `siege_status` `+0x70`, **`assault` `+0x74`** (u8, an assault going on), `active` `+0x75`. Its attacker side `+0x20` is the besiegers, with their countries at side `+0x100` and their units at side `+0xe8`.

- **A province's siege** is a pointer at province `+0x388`, null when there is none: `0x94f4c1` `48 8b 94 0a 88 03 00 00` (in the power spending's check), `0x5db7ec` `4c 8b 82 88 03 00 00`. A unit's siege is unit `+0xc8`.
- Breach "full": the breach the barrage gives and the barrage refuses at is the define at `0x233e51c` (i32): `0x4031d9` `8b 05 3d b3 f3 01 39 42 60 7c 40` (`cmp [siege+0x60], define`); both barrages' execute store it (`0x951015`, `0x9511f6`).
- Fort level: province `+0x2de0`, or `+0x2ddc` while mothballed (`+0x8f4` = 2): `0x40337e` `49 8b 47 30 80 b8 f4 08 00 00 02 75 08 8b 98 dc 2d 00`.

## A. Artillery barrage, naval barrage, assault

### They are not `perform_action`

`perform_action` (vtable `0x1d1c588`; keys `unit` `{ id type }`, `perform_action_status` yes/no at `+0x64`, `perform_action_province` `%d` at `+0x60`) is the army's map action: its check only finds the unit; its execute `0xd67580` calls the unit's method `+0x30` (the army's and the fleet's `0xd423a0` → `0xd429e0`(unit, province, queue = status, from interface = 1), the move or cancel of [army-orders.md](army-orders.md)). It is made at `0xd5a9d0` (in the code around `0xd5a986`, presumably the deferred action after a confirmation; _inferred_).

### The command: `spend_power_command`

The three siege buttons post the monarch-power spending of [countries.md](countries.md) ("What the economy's actions cost"): `spend_power_command` (vtable `0x1c554c0`, write `0x2a7440`, readkey `0x2a7530`, check `0x2a76d0`, exec `0x2a7310`) carrying a spending (vtable `0x1c55480`, writer `0x94eed0`, readkey `0x94f030`).

```
spend_power_command={ country="FRA" power={ data=183 category=37 skill=2 } }
```

| Key | Field | Value |
|---|---|---|
| `country` | command `+0x50` | the tag, quoted |
| `power` | spending, command `+0x58` | a block |
| `power.category` | spending `+0x10` | **9 assault, 37 `artillery_barrage`, 40 `naval_barrage`** |
| `power.skill` | `+0x14` | 2 (military) |
| `power.data` | `+0x18` | the besieged province's ID (written only when not 0) |
| `power.second` | `+0x1c` | -1 (not written) |
| `power.negative_power` | `+0x30` | not set (yes would let the country go below zero) |

**The buttons** (the siege view; the view's `+0x40` is the siege, the province its `+0x30`): `assault_button` `0x13af8a0` (category 9, `0x13af99c` `c7 40 68 09 00 00 00 c7 40 6c 02 00 00 00`), `artillery_barrage_button` `0x13af9e0` (37, `0x13afae0` `c7 40 68 25 00 00 00 c7 40 6c 02 00 00 00`), `naval_barrage_button` `0x13afb90` (40, `0x13afc7f`). Each makes the command for the player's country (map manager `+0x1e60`, or `+0x1e68` in observer mode 7) and posts it with `0x14ca2d0` without checking. The view's update `0x13ad9c0` enables each button from the answer below (`0x13adaf7`, `0x13adb0c`, `0x13adb21`; the button's method `+0x80`); the tooltip function `0x13b09e0` asks the same functions with a string (`0x13b177d`, `0x13b17f1`, `0x13b1865`).

**Check** `0x2a76d0` → `0x94f250(spending, tag)`: the cost `0x9526d0` against the country's power (country `+0x260 + 4 × skill`; military `+0x268`, `0x4035f2` `41 8b be 68 02 00 00`) unless `negative_power`, then by category: the province's siege (`+0x388`) must exist and the answer below must be yes (`0x94f46e` assault, `0x94f4a8` artillery, `0x94f4de` naval).

**Execute** `0x2a7310` → `0x94f140`: checks again, pays (`0x94fad0`), then `0x94fcb0` by category:
- 9: logs "Starting assault from powerspending", `0x23c960(siege, 1)`, which sets the siege's `+0x74` (`0x23c9c8`).
- 37: logs "Starting artillery barrage from powerspending", sets the siege's breach `+0x60` to the define at `0x233e51c`, gives a flag `have_berlin_wall` to … when the province is 50 (Berlin; not followed), and sends the leading country of the besieged side a notice (`0x943d00`, alert `+0xeb8`).
- 40: sets the breach the same way, notice `+0xec8`.

### Can the player press it, and why not: one function each

Each answers yes or no and, when `reason` is a `std::string*` (else null), appends the localised text (`0x14b34a0` then `0x932f0`): either the reason it cannot, or the explanation with the cost when it can. They are exactly what the GUI and the command's check call.

- **Common** `0x4030e0(tag, siege, reason)` (artillery and naval): `NOAB_NO_FORT` (fort level `+0x2de0` ≤ 0), `NOAB_YOU_ARE_DEFENDING` (the province's controller `+0x470` is the tag), `NOAB_ALREADY_ASSAULTING` (siege `+0x74`), `NOAB_ALREADY_BREACH` (siege `+0x60` ≥ define `0x233e51c`), `NOAB_NOT_LEADING` (entry 0 of the attacker side's countries is not the tag, and the tag is neither that country's overlord (`+0x18f0`, `0x403264` `48 8b 80 f0 18 00 00`) and among the participants). No vector at all answers no without a reason.
- **Artillery barrage** `0x4032f0(country, siege, reason)`, 3 arguments, returns u8. Answers no at once, without a reason, unless `[0x233ff2c]` is set and `[0x233fed6]` clear, or a thread-local byte (TLS block `+0x10`) is set (presumably a DLC or feature switch; not identified). Then `0x4030e0`; then the attacker side's artillery, `0xd72920(side+0xe8, out, kind 2, '---')` in thousandths, divided by 1000, must be at least the fort level (at least 1): `NOAB_NOT_ENOUGH_ART` ($ART$, $FORT$); then military power ≥ cost: `NOAB_NO_POINTS` ($COST$, $VAL$); yes with `ORDER_ARTILLERY_BARRAGE_IRO` ($COST$).
- **Naval barrage** `0x4038c0(country, siege, reason)`, same shape (its switch is `[0x233ff34]`). `0x4030e0`; the province is coastal (`0x4042d0(province+0x28)`, bit 6 of the answer's byte `+0x308`): `NO_NAVAL_BARRAGE_NO_COAST`; the country's fleets (unit method `+0x88`, owner `+0xd0`) in the adjacent sea provinces (province `+0x28` → adjacency vector `+0x168`/`+0x170`, 0x24-byte entries with the province ID at `+0`; a neighbor is sea when `[neighbor+0x28]+0x228` is negative): none, `NO_NAVAL_BARRAGE_NO_NAVIES` plus `NAVAL_BARRAGE_CANNON_REQ`; their cannons (`0xd54600` each, summed) at least 100 × fort level: `NO_NAVAL_BARRAGE_NOT_ENOUGH_CANNONS` ($CANNONS_CURRENT$) plus `NAVAL_BARRAGE_CANNON_REQ` ($CANNONS_NEEDED$); military power: `NO_NAVAL_BARRAGE_POINTS` ($COST$, $CURRENT$); yes with `ORDER_NAVAL_BARRAGE_COST`.
- **Assault** `0x381070(country, siege, reason)`: `NOAS_NO_FORT`, `NOAS_YOU_ARE_DEFENDING`, `NOAS_ALREADY_ASSAULTING` (`+0x74`), `NOAS_NO_BREACH` (`+0x60` ≤ 0), `NOAS_NO_INFANTRY` (the side's infantry `0xd72920(…, kind 0, …)` below 10 thousandths, `0x381230` `83 38 0a 7d 51`), `NOAS_NOT_LEADING` (as above), `NOAS_LOW_MORALE` (side morale below 0.5, `0x381361` `81 38 f4 01 00 00`), `NOAS_NO_POINTS` (power below `0x951980(tag, 9, 0, 0)`); then, when the singleton `0xacd3a0()`'s `+0x1d8` is set, `0xacccf0(that, tag, province ID, reason)` must say yes (a further rule; not followed); yes with `ORDER_ASSAULT_IRO` ($COST$) and `ASSAULT_WIDTH` (the fort's defense `0xa0e860` × define `0x233e3f4`).

So the framework can call `0x4032f0`, `0x4038c0` and `0x381070` with the country object, the province's siege (`+0x388`, not null) and its own `std::string` or null, as the view does.

### Cost

- `0x9526d0(spending, tag, text, 0)` is what the check and the payment use (whole points), by category:
  - 37: define `0x233f65c` × (1 + the country's modifier `0x1f1` + the attacker side's leader's (`0x21fe80` → `0x866900`) modifier `0x1f1`) (`0x953459`–`0x9535ec`); `0x1f1` is `artillery_barrage_cost` (_inferred_ from the modifier's place; not matched with the modifier table).
  - 40: define `0x233f678` × (1 + per adjacent own fleet its modifier through `0x913400` (`barrage_cost_in_fleet_modifier`, _inferred_) + the country's modifier `0x171` (`global_naval_barrage_cost`, _inferred_)) (`0x95362b`–`0x9537eb`).
  - 9: define `0x233f660` with factor 1 (`0x953439`); the panel's cost `0x951980(tag, 9, province, 0)` is define `0x233f660` × (1 + country modifier `0x217` + the province's modifier `0x218`) (`0x9521d7`; `assault_fort_cost_modifier` and `local_assault_fort_cost_modifier`, _inferred_). The assault check passes province 0. Whether the finish `0x9524d0` adds more was not read.
- The defines: `0x952344` `8b 05 12 d3 9e 01` (`0x233f65c`), `0x95234c` `8b 05 26 d3 9e 01` (`0x233f678`), `0x952302` `69 0d 54 d3 9e 01 e8 03 00 00` (`0x233f660`). Their NDefines names were not looked up.

## B. Mothballing and reopening a fort

- **The state**, province `+0x8f4`, u8: **0 active and locked** (never mothballed), **1 active**, **2 mothballed**. `0x5db7e4` `44 0f b6 8a f4 08 00 00`; the predicate "mothballed" is `0x144480` (`cmp byte ptr [rcx+0x8f4], 2`). The setter `0xa0fcc0(province, state)` stores it, refreshes the map (mode 0x19) and, when mothballing, cuts the garrison `+0x400` to `+0x2ddc` × define `0x233fe0c` (what a free capital fort keeps) through `0xa0fda0`.
- **`mothball_fort`** (vtable `0x1c8d810`, token `0x31b8`): `mothball_fort={ province=183 country="FRA" }` (`province` at `+0x58`, written when not 0; `country` at `+0x50`). Check `0x5db7c0`: the tag controls the province (`+0x470`), no siege (`+0x388` null), state not 0. Execute `0x5db310`: **toggles**, 1 → 2 or 2 → 1 (`0xa0fcc0(p, state == 2 ? 1 : 2)`), recounts (`0x9f9500`), and on reopening also `0xa26f20(province)` and `0x364740` for each country in the owner's list `+0x1b88` (not followed).
- **`set_fort_active_locked`** (vtable `0x1c8d8b8`, token `0x3ba0`, the same writer and reader): same keys. Check `0x5dbb50`: controller and no siege. Execute `0x5db880`: **toggles the lock**: state 0 → 1, any other → 0 (so a mothballed fort locked becomes active).
- **The buttons** (the province view, `0x136a8d0` binds; the view's `+0x20` is the province): `fort_maintenance_checkbox` → `0x1377df0`, which posts `mothball_fort` for the player when it controls the province, there is no siege and the state is not 0; `fort_maintenance_lock_checkbox` → `0x1377f40`, which posts `set_fort_active_locked` when it controls it and there is no siege (`0x136c88f`, `0x136c8d8`).
- **Why not** (one function each, `(tag, province ID, std::string* reason)`, returns u8, appends lines): `0x5db5a0` for the maintenance box, `FORT_MAIN_NONC` (not the controller), `FORT_MAIN_SIEGE`, `FORT_MAIN_LOCKED` (state 0); `0x5db9c0` for the lock, `FORT_MAINTENANCE_LOCK_NONC`, `FORT_MAINTENANCE_LOCK_SIEGE`. Callers: the province view `0x137a2b7` and `0x15882c`.
- **The maintenance**: the view (`0x137a2b7`–`0x137a461`) shows `FORT_CURRENTLY_WITH_MAIN` with `0xa1e970(province, out)` (the fort's monthly cost, fixed point) when active; when mothballed, `FORT_CURRENTLY_HALF_MAIN` with the same function's value if the owner is the controller, else `FORT_CURRENTLY_WITHOUT_MAIN`. The localisation says a mothballed fort costs half; `0xa1ea80`, which `0xa1e970` calls, does not read `+0x8f4`, so where the half is applied was not found.
- **`mothball_all`** (vtable `0x1c8d570`): `mothball_all={ tag="FRA" mothball=yes }` (`tag` `+0x50`, `mothball` `+0x58`, written only when yes). Check `0x268490`: the country has provinces (`+0x1344`) or `+0x26` is 7. Execute `0x5dbc00`: for each owned province (`+0x1338`, count `+0x1344`) with a fort (`+0x2dd8`, `0x5dbc71` `80 bb d8 2d 00 00 00`), no siege, controlled by the country and not locked: state 2 when `mothball`, else 1. Posted at `0xfaacf2` and `0xfbbb0c` (the military view; `CONFIRM_MOTHBALL_ALL_*`, `ALL_MOTHBALLED`), not read further.

## Not found or not done

- The NDefines names of `0x233e51c`, `0x233f65c`, `0x233f660`, `0x233f678`, `0x233fe0c`, `0x233e3f4`; the modifier IDs' names are from the localisation only.
- What the switches `[0x233ff2c]`/`[0x233ff34]` with `[0x233fed6]` and the thread-local byte are (without them both barrage answers say no without a reason).
- The assault's extra rule `0xacccf0`; what `0x23c960` does besides setting `+0x74`.
- Nothing was tried in the game.

## What the framework does, and what the game showed

`game::sieges` and `game::forts`; tried on 2026-11-05 ([Player actions A4](../prototype-log/2026-11-05-player-actions-a4-armies-fleets.md)).

- **The cost**: the framework asks `0x9526d0` with a spending of its own, laid out as the siege view's buttons lay theirs out inside the command (`0x13af964` to `0x13af9b8`): the vtable `0x1c55480`, `+0x08` 0x165, `+0x10` the category, `+0x14` 2, `+0x18` the province, `+0x1c` -1, the rest zero. It answered 50 for both barrages and 5 for the assault, what the payments took.
- **The answers** `0x4032f0`, `0x4038c0` and `0x381070` gave the tooltip's reasons as written above, among them the cannons' line with the naval barrage's; with no fort, each its own `…_NO_FORT`. `spend_power_command` with categories 40 and 9 made a breach and started the assault, which took the fort.
- **Forts**: `mothball_fort` toggled Northumberland's fort both ways; `0x5db5a0` answered yes for each of England's forts at peace. Its reasons (under siege, not the controller, locked) were not seen.
