# Leaders and mercenary companies: recruiting, assigning, dismissing, hiring

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-05, statically (disassembly and scripts over the PE file); nothing here was tried in the game. What is marked _inferred_ was concluded from the shape of the code. Localisation texts quoted are the English files'.

## The leader object

A leader is 0xa8 bytes, class `0x1cbc1b8` (constructor `0x860fe0`; `NO_LEADER`, the null leader, is made once at `[0x233fec0]` by `0x860ed0`; a real leader answers yes at vtable `+0x50`, the null one no). Its writer (`0x861880`, slot `+0x10`) gives the fields their save keys:

| Offset | Field | Key | An instruction that uses it (bytes) |
| --- | --- | --- | --- |
| `+0x10` | ID, u64: type (49, `0x31`) at `+0x10`, number at `+0x14`; written `{ id=<+0x14> type=49 }` | `id` | `0x861cd1` `4c 8d 46 10`; the type is set at `0x8631d1` (`mov dword ptr [rsp+0x38], 0x31`, `0x771be0`(world, 2, -1) gives the number) |
| `+0x20` | name, `std::string` | `name` | `0x8618b6` `48 8d 56 20` |
| `+0x40` | kind, i32: 0 general, 1 admiral, 2 conquistador, 3 explorer (`0x8664e0` names them `GENERAL`, `ADMIRAL`, `CONQUISTADOR`, `EXPLORER`) | `type` | `0x86c40a` `89 4e 40` (create's execute) |
| `+0x44` | siege, i32 | `siege` | `0x86c8df` `8b 46 44` |
| `+0x48` | maneuver | `maneuver` | `0x86c8db` `44 8b 66 48` |
| `+0x4c` | fire | `fire` | `0x86c8d4` `44 8b 76 4c` |
| `+0x50` | shock | `shock` | `0x86c8d8` `8b 7e 50` |
| `+0x54` | female, byte | `female` | `0x861a5f` `0f b6 5e 54` |
| `+0x55` | mercenary, byte | `mercenary` | `0x861a87` |
| `+0x58` | the unit it leads (army or navy pointer, null when none) | – | `0x86e6e6` `48 83 7f 58 00` (dismiss check) |
| `+0x60`, `+0x80`, `+0x84` | death date, activation date, birth date | `death_date`, `activation`, `birth_date` | `0x861c72`, `0x861c38`, `0x861ca8` |
| `+0x68` | personality (pointer) | `personality` | `0x861bef` |
| `+0x70` | the country's tag | `country` | `0x861b45` |
| `+0x78` | monarch ID (u64; 0 when the leader is no ruler or heir) | `monarch_id` | `0x861c1a` `4c 8d 46 78` |
| `+0x88` | dynasty, `std::string` | `dynasty` | `0x8618e9` |

The four skills are four i32 from `+0x44` in the order siege, maneuver, fire, shock (the writer's loop at `0x861ab1`–`0x861b35` picks the key by index).

**A country's leaders** are an intrusive doubly linked list: head `country+0x1468`, tail `+0x1470`, count `+0x1478`; each node 0x20 bytes, the leader at `+0`, previous at `+8`, next at `+0x10` (`CCountry::AddLeader` `0x306bd0`, which walks it at `0x306c51` `49 8b 87 68 14 00 00` and appends at `0x306d3b`–`0x306d6f`). **A unit's leader** is the pointer at `unit+0xd8`, `NO_LEADER` when it has none: `0xd405b0`(unit, leader) sets it (`0xd406ef` `48 89 be d8 00 00 00`), first taking the old leader's `+0x58` back (`0x865100`) and setting the new one's. A leader is found by its ID with `0x14cf730` (the registry answers the leader itself, no `0x10` to take off, unlike units).

**Leaders without upkeep** (the military view's tooltip `MIL_COST_MAINT`, "We have $CURR$ leaders, and can have $ALLOW$ leaders without upkeep"): `0x3846f0`(country, std::string* out, kind) writes it. Land leaders (kinds 0, 2) and naval leaders (1, 3) are counted apart: the pool is `0x3841b0`(country, text) for land and `0x384450`(country, text) for naval (`text` null or a `std::string` for the explanation; naval: the modifiers `0x99` plus `0x1ca`, _inferred_ `free_leader_pool` and `free_navy_leader_pool`), and the count is the country's leaders of that side whose monarch ID (`+0x78`) is 0 (`0x384780`–`0x3847b7`). Each costs `[0x233df00]` (`LEADER_MAINTENANCE_COST`, read at `0x384767` `44 8b 05 92 97 fb 01`) a month in mil points for land, dip points for naval, when the count is not below the pool (`0x3847bc` `cmovl`). **Being over the pool is only upkeep: neither the create check nor its execute looks at it.**

## A. Recruiting a leader (`create_leader`)

- **Class** `0x1cbc4d0`, 0x88 bytes; write `0x86d400`, read a key `0x86d580`, check `0x86d7b0`, execute `0x86c1a0`. Fields: country tag `+0x50`, kind `+0x58` (i32), name `+0x60` (`std::string`), is_female `+0x80` (byte).
- **Written form**: `country` (the tag, a string), `leader` (the kind as `%d`, read back with `"%i"`), `name` (a string), `is_female` (yes/no):

  `create_leader={ country="CAS" leader=0 name="Gonzalo de Córdoba" is_female=no }`

  `leader` is the kind, not a leader block: 0 general, 1 admiral, 2 conquistador, 3 explorer. An empty `name` makes the execute pick the name and the sex itself (`0x86c4bf`: `cmp qword ptr [r13+0x70], 0`), as below.
- **The button**: the military view's four recruit buttons open the dialog `ConfirmAndNameLeaderDialog` (constructor `0x144a640`(dialog, parent, kind, mode), vtable `0x1d84880`; opened by `0x14610d0` (kind 0), `0x1461140` (kind 1) and their siblings to `0x1461370`; the kind at `+0x1c0`, is_female at `+0x1c8`, the name edit box `leader_name_editbox`, a `random_leader_name_button`). `0x144ad00` fills the box: is_female from `0x314350`(country, random), the name from `0x313d90`(country, …), the same two the execute uses. The dialog's OK (`0x144a940`, its vtable `+0x50`) takes the edit box's text and calls **`0x307490`(country, kind, const std::string& name, is_female)**, which makes the command (`0x307520`–`0x307562`) and posts it straight to the handler (the session from `[country+0x1888]`'s method `+0x80`, its handler `+0x58`, the handler's post `+0x30`; the same stamping as `0x14ca2d0`), without asking its check. So the player's command carries the name in the box (a random one unless they typed one) and the sex rolled when the dialog opened.
- **Check** (`0x86d7b0`) is **`0x307370`(country, kind)**, a bool and no reasons:
  - kind 4: no; kinds 2 and 3 (conquistador, explorer): the country's boolean modifier `0xe3` must be set and the boolean modifier `0` clear (`0x8e7b60`(modifiers, index), the modifiers `0x93d750(country+0x28c0)+8`). The tooltip names them `RECRUIT_NO_IDEA` ("You need the 'Quest for the New World' national idea…"; _inferred_ `may_explore`) and `CAN_NOT_BUILD_COLONIES` (no English text in the files);
  - kind 1 (admiral): the naval force limit `0x347dd0`(modifiers, out, …) must be above 0 (`ADMIRAL_NO_NAVAL_FORCE_LIMIT`, "The naval force limit is 0, you need to have a positive force limit…");
  - the cost `0x951980`(tag, 3 + kind, 0, null) against the points: dip (`+0x264`) for an admiral or an explorer, mil (`+0x268`) for a general or a conquistador (`0x30745e` `83 fb 01 75 07 b8 64 02 00 00 eb 05 b8 68 02 00 00 39 14 38 0f 8c`).
  So `0x307370` is the one can/cannot function; the framework can call it with the player's country and the kind. It writes no reasons; the reasons are the tooltip's, in this order: the idea, the colonies modifier, not enough points (`RECRUIT_CANT_AFFORD_M` for mil, `_D` for dip; the explorer's tooltip asks for `RECRUIT_CANT_AFFORD_A`, which has no English text), the admiral's force limit.
- **Tooltip**: `0x145fc70`(…, element name) builds each recruit button's tooltip (`recruit_explorer`, `recruit_conquistador`, `recruit_admiral`, `recruit_general`), calls `0x307370` for the general (`0x1460b4a`), and writes the cost (`RECRUIT_<KIND>_IRO`, "This will cost you $COST$") and the upkeep (`0x3846f0`). The view's update `0xb7caf0` calls `0x307370` four times (`0xb7fe48`, `0xb800fe`, `0xb80504`, `0xb8081a`) to enable the buttons.
- **Cost**: **`0x951980`(tag by value, spending kind, int, std::string* text)**, whole monarch points (finished by `0x9524d0`), kinds 3 general, 4 admiral, 5 conquistador, 6 explorer. Each starts from an int define times 1000: `PS_BUY_GENERAL` `[0x233f610]` (`0x95207d` `44 69 3d 88 d5 9e 01 e8 03 00 00`), `PS_BUY_ADMIRAL` `[0x233f60c]` (`0x951f23` `44 69 3d de d6 9e 01 e8 03 00 00`), `PS_BUY_CONQUISTADOR` `[0x233f668]` (`0x951c9d` `44 69 3d c0 d9 9e 01 e8 03 00 00`), `PS_BUY_EXPLORER` `[0x233f664]` (`0x951c90` `44 69 3d c9 d9 9e 01 e8 03 00 00`); all 50 in `defines.lua`. Modifiers: `0x14e` for all four (_inferred_ `leader_cost`), `0x133` for a general (_inferred_ `general_cost`), `0x176` for an admiral (_inferred_ `admiral_cost`); with `text` it writes them (`IMPR_FURTHER_MODIFIED_BY`).
- **Execute** (`0x86c1a0`): when the country is a human player's (`[world+0x1d48]` non-empty and its entry non-zero; the AI pays nothing here) it takes the cost (`0x37f500`(country, power, -cost, 0)) and adds it to the power's spent total (`+0xb0` of `+0x2800`/`+0x2818`/`+0x2830`); in a session with shared power (`country+0x3d0`, _inferred_ co-op) it takes it from there too. It makes the leader (`0x860fe0`, an ID by `0x8631b0`), sets the kind, the name and sex (from the command, or `0x314350` and `0x313d90` when the name is empty), rolls the skills from tradition (`0x863420`; army tradition `+0xe40` for kinds 0 and 2, navy tradition `+0xe44` for 1 and 3, times `REGULAR_LEADER_POWER` `[0x233f1f8]` or `EXPLORER_LEADER_POWER` `[0x233f1f4]` over 1000), adds it with `0x306bd0`(country, leader, date, 1, 1) and, for the player, a message `CREATELEADER`.

## B. Assigning, removing and dismissing a leader

### `assign_leader`

- **Class** `0x1cbc6c8`, 0x70 bytes; write `0x86eea0`, read a key `0x86ef60`, check `0x86f060`, execute `0x86edc0`. Fields: the unit `+0x50` (pointer, filled by the check/execute) and its ID `+0x58`; the leader `+0x60` and its ID `+0x68`.
- **Written form**: `leader` = the leader's ID block, then `unit` = the unit's ID block (`0xcda00`):

  `assign_leader={ leader={ id=12345 type=49 } unit={ id=67 type=54 } }`

  (a fleet's `type` is the navy's ID type).
- **Remove a leader**: the same command with a leader ID that finds nothing. The confirmation dialog's OK (`0xfab520`) posts it with `unit` the leader's current unit (`leader+0x58`, its ID `+0x20`) and `leader` the empty ID `[0x233fea8]` (in `.bss`, so 0, _inferred_): `assign_leader={ leader={ id=0 type=0 } unit={ id=67 type=54 } }`. The execute then gives the unit `NO_LEADER`.
- **The GUI**: the leader list's choose (`0x145e3a0`, also `0x145efd0`, `0x144a3d0`) asks `0x862fa0` and `0x862a70` first; for a leader that already leads a unit it opens a confirmation (`0x144a050`), otherwise it posts at once.
- **Check** (`0x86f060`): the unit exists (`0x14cf730` on `+0x58`), `+0x203` clear, not in combat (`+0xc0` null), not loaded or not its own transport (vtable `+0x90`/`+0x98`, then `+0xd8`), a mercenary company's unit (`unit+0x48` non-null, its company's leader `+0x48`) refuses; when the unit can have a leader (`+0x88`) and the leader is found, **`0x862fa0`(leader, unit, text)** must answer yes; when the leader is found, **`0x862a70`(leader, text)** must; then the unit's location must be its owner's or not foreign (`+0xe8`, `+0xd0`, the owner's list at `+0x1548`), and a navy needs no `+0x218`.
- **Reasons, as the GUI writes them**, both functions taking a `std::string*` or null:
  - `0x862a70`(leader, text): may the leader leave its current unit (true when it leads none): `SELECT_LEADER_EXPLORING`, `SELECT_LEADER_AT_SEA`, `SELECT_LEADER_AT_SEA_OUT_OF_RANGE`, `SELECT_LEADER_IN_COMBAT`, `NO_SELECT_LEADER_SHIP`, `SELECT_LEADER_MERCENARY_COMPANY`, `SELECT_LEADER_IN_HOSTILE_TERRITORY`, `SELECT_LEADER_CONDOTTIERI`;
  - `0x862fa0`(leader, unit, text): may the unit take this leader: `SELECT_LEADER_TARGET_AT_SEA_OUT_OF_RANGE`, `UNIT_VIEW_CHOOSE_LEADER_PORT` ("…the fleet is not in port!"), `CANNOT_ASSIGN_MONARCH_CONDOTTIERI`.
  The kind is not checked anywhere read: a general on a fleet was not looked for.
- **Execute** (`0x86edc0`): resolves both; a leader not found becomes `NO_LEADER`; takes the leader off its old unit (`0xd405b0`(old, NO_LEADER)), then `0xd405b0`(unit, leader).

### `dismiss_leader`

- **Class** `0x1cbc578`; write `0x86e420`, read a key `0x86e500`, check `0x86e660`, execute `0x86d860`. Fields: country tag `+0x50`, leader ID `+0x58`.
- **Written form**: `dismiss_leader={ country="CAS" leader={ id=12345 type=49 } }`.
- **The GUI**: a confirmation dialog's OK (`0xfab020`) posts it with the player's tag and the leader's ID (`leader+0x10`).
- **Check** (`0x86e660`), a bool, no reasons: the leader is found; it is not the ruler's (`0xa4bf80`(`country+0x1280`)) or the heir's (`0xa4bf80`(`country+0x1298`)) leader; **it leads no unit** (`leader+0x58` null). It does not compare the leader's country with `country`. So a leader must be removed from its unit (above) before it can be dismissed.
- **Execute** (`0x86d860`): removes it from the country (`0x3071b0`) and writes the message.

## C. Reading the player's leaders

Walk `country+0x1468` (node `+0`, next `+0x10`); for each leader read `+0x10`/`+0x14` (ID), `+0x20` (name), `+0x40` (kind), `+0x44`..`+0x50` (siege, maneuver, fire, shock), `+0x54` (female), `+0x58` (its unit; the unit's ID at `+0x20`), `+0x78` (monarch ID: a ruler or heir leader does not count against the pool). The unit's side: `unit+0xd8`. The upkeep line: `0x3846f0`(country, out, kind), or the pools `0x3841b0`/`0x384450`(country, null) and `LEADER_MAINTENANCE_COST` at `[0x233df00]`.

## D. Hiring a mercenary company

### The company

A company is 0x68 bytes, class `0x1cc9af8` (constructor `0x901a40`; the null company, `[0x233fef8]`, from `0x901ad0`). Its writer `0x8fdfd0`:

| Offset | Field | Key | Instruction (bytes) |
| --- | --- | --- | --- |
| `+0x10` | ID, u64; type 59 (`0x3b`, `0x8fe2f9`) | `id` | `0x8fdff0` `4c 8d 41 10` |
| `+0x20` | the country it is offered to (tag) | – | set at `0x41135e` |
| `+0x28` | the template (from `common/mercenary_companies`); its key, `std::string`, at template`+0x20`; `null_mercenary_tag` without one; template`+0x25c` the home province ID (0 for none, _inferred_) | `tag` | `0x8fdfff` `48 8b 56 28` |
| `+0x30` | progress, byte (being raised) | `progress` | `0x8fe15e` `0f b6 5e 30` |
| `+0x38`/`+0x40` | its army (pointer, ID) | `unit` | `0x8fe237` `48 8b 4e 40` |
| `+0x48` | its leader | `leader` | `0x8fe207` `48 8b 4e 48` |
| `+0x50`, `+0x54` | hiring date, disband date | `hiring_date`, `disband_date` | `0x8fe263` `48 8d 4e 50`, `0x8fe291` `48 8d 4e 54` |
| `+0x58`, `+0x5c` | manpower, starting manpower | `manpower`, `starting_manpower` | `0x8fe186` `8b 5e 58`, `0x8fe1b1` `8b 5e 5c` |
| `+0x60` | modifier (thousandths, 1000 = none) | `modifier` | `0x8fe1dc` `8b 5e 60` |

An army that is a company points at it from `army+0x48` (the assign check `0x86f0e9` `48 39 6b 48`; the leader check `0x862d4c`), _inferred_.

**The companies a country may hire** are its own copies, one per template it qualifies for (`0x8fcc00`(template, tag, 0)), made by `UpdatePossibleMercenaryCompanies` (`0x411270`) into an array at `country+0x23f0` (pointers), its count at `+0x23fc` (`0x4112ef` `49 8b 0c 24 48 8b 81 f0 23 00 00 48 63 89 fc 23 00 00`). Hired ones stay in it (the list shows them as hired). The localised name: `0x8ffcb0`(company, std::string* out). The regiments: `0x900100`(company, int out[3], country) fills infantry, cavalry, artillery counts (_inferred_ from its callers' loop over three). The size the panel shows is their sum.

### The command: `buildunit` with a mercenary company

- `condottieri_unit_command` (`0x1d1bc58`, keys `unit`, `cost`, `actor`, `recipient`) is something else: lending an army to another country as condottieri (a diplomatic action), not hiring.
- **Written form**: the recruit command `buildunit` (`0x1c4dff8`, writer `0x26d880`) with the company under `mercenary` (token `0x2a0b`; `0x2660e0` writes the company's ID block from the command's `+0x70`, only when it is not the empty ID, `0x26d9db`), `category` 1 (the special kind, `+0x78`, written always) and no `unit` (the unit type is the placeholder):

  `buildunit={ country="CAS" province=219 mercenary={ id=4321 type=59 } category=1 }`

  (the writer also writes `trade_mission` and `node` as 0 and the yes/no keys when not default; whether the reader needs them was not checked). Reading `mercenary` (`0x26dcd3`, `0xcd990` into `+0x68`/`+0x70`) and `category` (`0x2b7`) gives back what `0x26ce40`(command, tag, null type, province ID, company) sets.
- **The GUI**: the macro builder's land window has a `recruit_mercs_button` / `MACRO_RECRUIT_MERC_TAB` that shows `hire_mercenary_company_window`, a `mercenary_company_list` of `mercenary_company_entry` (update `0x1235060`, tooltips `0x1235e50`: `available_time`, `hiring_cost`, `maintenance_cost`, `manpower`, the entry's buttons `list_button1`/`list_button2` enabled by `0x8ff0f0`(company, country, 1, null)). The company chosen is held by the macro builder (its `+0x60`/`+0x68`); **the player then clicks a province on the map**: the map's click (`0x828140`) calls the macro builder's `0x11c76f0`, which in its mercenary mode (`+0x38` 0 and the mode's `+0xd8` 1) makes the command with `0x26ce40`(command, player tag, null, the clicked province's ID `+0x20`, company), **asks its check** (`+0x40`, `0x11c78b9`) and posts it (`0x14ca210`), then clears the choice (`0x901ad0`). The province's map tooltip (`0x9e3210`) shows `HIRE_MERCENARY_COMPANY` ("Hire $COMPANY$ in $PROVINCE$"), `MERCENARY_COMPANY_BUILD_COST` (cost and days) or the reasons from `0x8ff7b0`. How the entry's button sets the macro builder's choice was not followed.
- **So the province is the player's choice**; the framework must pick one: any province for which `0x8ff7b0` answers yes.
- **Check**: the buildunit check (`0x26e080`) for a command without a valid unit type asks the company's `+0x50` and **`0x8ff7b0`(company, country, province pointer, std::string* text)** and stops ([provinces.md](provinces.md)). `0x8ff7b0` is the one can/cannot function with reasons, and the framework can call it:
  1. `0x8fee30`(company, country, province, text): the province may raise troops (`0x3fb1c0`(country, province, 1, text): `MACRO_NOT_CONTROLLER`, `MACRO_UNDER_SIEGE`, `MACRO_BATTLE_IN_PROGRESS`) and lies within the company's range (from its home province or the country's; `MERCENARY_COMPANY_TOO_FAR_PROVINCE` with `$RANGE$`, `MERC_RANGE_EXPLAINED`);
  2. `0x8ff0f0`(company, country, 0, text): the hiring cost `0x900cb0` within the treasury (`+0xbb0`; `CANNOT_AFFORD_COST`), not bankrupt within `[0x233e160]` years of `country+0xe00` (`MERCENARY_COMPANY_NOT_TO_BANKRUPTED`), not hired (hiring date after disband date, or `progress` set: `MERCENARY_COMPANY_ALREADY_HIRED`), recovered (`0x8ffad0`, the date it may be hired again: `MERCENARY_COMPANY_NOT_AVAILABLE_UNTIL` with `$DATE$`); with its third argument 1 (the list's entries) also that some owned province (`country+0x1338`, count `+0x1344`, fewer than 100) passes `0x8fee30` (`MERCENARY_COMPANY_NOWHERE_TO_HIRE`);
  3. `0x3fb1c0`(country, province, 1, text) once more.
  For the list (company only, no province) the panel uses `0x8ff0f0`(company, country, 1, text).
- **Cost**: `0x900cb0`(company, int* out, country, std::string* text): the sum over the three kinds of regiments × each kind's monthly cost (`0x3c6970`), times `MERCENARY_COMPANY_HIRING_COST_MONTHS` `[0x233effc]` (48; `0x900d57` `69 05 9b e2 a3 01 e8 03 00 00`), then the modifiers (`MERC_COST_BASE`, `MERC_COST_TYPE`; `mercenary_cost` and the like, `MINIMUM_MERCENARY_HIRE_PRICE`); the result is in the treasury's units (compared directly with `+0xbb0`). Monthly upkeep: `0x900660`(company, …, text) (`MERC_MAINTENANCE_TT`, `MERC_ENTRY_FOR`). The army is raised over time (`MERCENARY_COMPANY_BUILDING_TIME`; "take $TIME$ days"), not at once.

### Disbanding

A hired company is an army: the unit view disbands it with the ordinary `disband_unit={ unit={ id= type=54 } }` (`0x1d1dd28`, keys `unit`, `delete_only_empty`); the tooltip is `DISBAND_MERC_DRO`. The company's side of a removed army (`MERCENARY_COMPANY_DISBANDED`, the disband date) is in `0x33f410`, reached from the world's unit removal (`0x7697f8`) among others (_inferred_; not followed). `detach_merc` (`0x1d1d4a0`, key `unit`) is a different action (detaching mercenary regiments; not followed).

## Not found or not done

- The names of the modifiers `0xe3`, `0`, `0x14e`, `0x133`, `0x176`, `0x99`, `0x1ca` are inferred from the tooltips, not read from the modifier table.
- How the mercenary list's button hands the company to the macro builder, and whether some other button hires without a map click.
- The exact composition rule in `0x900100` (regiments from development and the template's weights) and the range rule in `0x8fee30`.
- The empty ID's value is taken as 0 because `[0x233fea8]` lies in `.bss`; no write to it was found.

## What the framework does, and what the game showed

`game::leaders` and `game::mercenaries`; tried on 2026-11-05 ([Player actions A4](../prototype-log/2026-11-05-player-actions-a4-armies-fleets.md)).

- **Recruiting**: `create_leader={ country="ENG" leader=1 }`, no `name`: the game picked the name and the sex ("安东尼 德 克利福德"), took the cost and added the leader at the end of the list. Whether the player may is `0x307370`; the reasons are the tooltip's keys for the conditions it tests, in its order: `RECRUIT_NO_IDEA` when the country's modifier `0xe3` is not set, `CAN_NOT_BUILD_COLONIES` when `0` is (asked as the check asks them, `0x93d750(country+0x28c0)+8` then `0x8e7b60`), the points against `0x951980`'s cost, and an admiral's force limit when nothing else explains a refusal.
- **A localisation text with values**: the game's lookup `0x14b34a0(out, key, null, 0)` leaves `$COST$` out of `RECRUIT_CANT_AFFORD_M` ("你需要至少mil。"). The lookup first finds the key's text by a hash of its bytes, `h = 0x811c9dc5`, then for each byte `h = h * 0x1000193 ^ (signed byte)`, through `0x16fb030(table, h)`, the table `[[0x2357b20]]`; that answers the text as its file has it. The framework reads it so (`game_string::unformatted`) and fills `$COST$` or `$COST|Y$` itself: "你需要至少50mil。".
- **Assigning and removing**: both as written above; `assign_leader={ leader={ id=0 type=0 } unit={ … } }` took the leader off, so the empty ID is 0. The framework refuses a general for a fleet and an admiral for an army itself, which the command does not check.
- **Hiring**: `buildunit={ country="ENG" province=236 mercenary={ id=N type=59 } category=1 }` read back as written, took the hiring cost at once and raised the company as an army within 40 days.
