# Empires: the Holy Roman Empire's and the Celestial Empire's reforms, decrees and incidents

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with `cargo xtask disasm` and scripts over the PE file (the command vtables, the token table, the strings the checks write); what the running game showed is in the last section. What is marked _inferred_ was concluded from the shape of the code. It is what the `empire` and `celestial_empire` groups of [mod-capabilities.md](../plans/mod-capabilities.md) (M19) rest on.

## The empires

- **Both are in the world itself**: the Holy Roman Empire at world `+0x15d0`, the Celestial Empire at `+0x1b40` ([architecture.md](architecture.md)). They share a layout and a base class (_inferred_ from the code that reads both alike):

| Offset | Field | An instruction that uses it (bytes) |
| --- | --- | --- |
| vtable `+0x58` | the empire's token, as a reform names its empire: `hre` `0x2749`, `celestial_empire` `0x35b7` | `0x701be5` |
| vtable `+0x60` | `(empire, tag, reform, reasons or null) -> bool`: whether the country may pass the reform now, writing the view's reasons: the Holy Roman Empire's `0x706890`, the Celestial Empire's `0x7130e0` | `0x7f3d9f` |
| `+0x10` | the emperor's tag | `0x7f3d9b` `48 8b 51 10` |
| `+0x18` | imperial authority, or the mandate, i32 thousandths | `0x706f88` `45 8b 45 18` |
| `+0x20` | the reforms passed, in order, a game array of pointers (data `+0x28`, count `+0x34`) | `0x706a47` `49 63 45 34`, `0x706a58` `49 8b 45 28` |
| `+0x504` | the Holy Roman Empire's: whether this emperor has revoked a reform | `0x7062c9` `80 b9 04 05 00 00 00` |

- **The Celestial Empire's decree in force** is at its `+0x68` (world `+0x1ba8`), a pointer to the decree, null while none is, and the day it ends at `+0x70` (world `+0x1bb0`), read by the decree check (`0x478d5e`, `0x478d7e`).

## Imperial reforms

Both empires' reforms are one kind of object, defined in `common/imperial_reforms/` (the Holy Roman Empire's in `00_hre.txt` without the Emperor DLC and `00_hre_emperor.txt` with it, the Celestial Empire's in `01_china.txt`), each naming its empire (`empire = hre`).

- **The database** is at the global `0x242bcd0`, made by `0x801440`: a hash table by key (found by `0x8e9f0(database, key)`), and at `+0x10` the reforms in load order, a `std::vector` of pointers. `0x701b50(empire, out)` lists an empire's by walking the vector and keeping those whose token is the empire's.
- **A reform's fields**:

| Offset | Field | An instruction that uses it (bytes) |
| --- | --- | --- |
| `+0x10` | key, `std::string` | the enact command's writer, `0x7f3b81` `48 83 c3 10` |
| `+0xae8` | `potential`, a content trigger | the Celestial Empire's check, `0x7135af` `48 8d 8b e8 0a 00 00` |
| `+0xb60` | `trigger`, a content trigger | the same, `0x7135c9` |
| `+0xbdc` | the empire's token | `0x701be8` `39 87 dc 0b 00 00`, `0x7f3d5d` |
| `+0xbe8` | `required_reform`, the key, `std::string` | the checks, `0x706af0` |
| `+0xc08` | the reform that disables this one, the key | the checks, `0x706b02` |
| `+0xc28` | `gui_container`, the part of the Empire's view it is in (`mainline`, `left_branch`, `right_branch`; `nodlc` for those without the Emperor DLC) | the view, `0x115ece3` `49 8d 8e 28 0c 00 00` |

- **Its name**: `0x800300(reform, out)` writes the name the view gives it.
- **The Empire's view** (`0x115cbb0`) asks `0x702140` for the parts of its view and lists, part by part, the empire's reforms whose `gui_container` is that part's (`0x115ece3`). The parts are those of the DLC the game has, so the reforms of the other set are not shown; the framework lists those whose `potential` holds, which is the same choice (`potential = { has_dlc = "Emperor" }` and its negation).

### The Holy Roman Empire's check, `0x706890(empire, tag, reform, reasons)`

With reasons null it answers no at the first condition that fails; otherwise it writes every one, a line each, and when the reform may be passed, a line of what passing it does (`ENACT_REFORM`). In order:

1. `CAN_NOT_ENACT_REFORM_NOT_EMPEROR`: the tag is not the emperor's (`+0x10`).
2. `CAN_NOT_ENACT_REFORM_LEAGUES`: a religious league war in the Empire (world `+0x241c` is 1).
3. `MUST_BE_AT_PEACE_IMP_REFORM`: the emperor is at war (country `+0x289b`).
4. `ALREADY_THIS_IMP_REFORM`: the reform is among those passed.
5. `REQUIRED_REFORM_NOT_PASSED`: its `required_reform` is not.
6. `DISABLED_BY_REFORM`: the reform that disables it is passed (the other branch).
7. `NOT_ENOUGH_BACKERS_IMP_REFORM`: fewer electors and princes for than against: each member but the emperor that is an elector, or a free city (kind 7), with the Emperor DLC's flag at country `+0x14b0`'s `+0x958`, counts for when its opinion of the reform (`0x3b4f90`) is above 0, against otherwise; the emperor counts for.
8. `NOT_ENOUGH_AUTHORITY_IMP_REFORM`: the authority (`+0x18`) below `IMPERIAL_REFORM_COST` (`0x233dad8`, an i32 of whole points, times 1000).

A byte at world `+0x23a4` set makes it answer yes before anything (_inferred_: a debug switch).

### The Celestial Empire's check, `0x7130e0(empire, tag, reform, reasons)`

Its reasons, in order: the reform's `trigger` (`+0xb60`) when it has one, its failing conditions; `CELESTIAL_DECISION_NOT_EMPEROR`; `CELESTIAL_DECISION_NONE_EXIST` when the empire has no reform left (`0x701cf0`); `CELESTIAL_DECISION_ALREADY_ENACTED`; `DISABLED_BY_REFORM`; its `potential` (`+0xae8`); `CELESTIAL_DECISION_MINIMUM_TO_ENACT` below the least mandate (`0x233fb2c`); `CELESTIAL_DECISION_NOT_ENOUGH_MANDATE` below the mandate it costs (`0x233fb30`); `CELESTIAL_DECISION_NOT_ENOUGH_STABILITY` when the stability it costs (`0x233fb08`) would take stability below -3. The three defines are i32 thousandths, compared with the mandate and stability as they are.

### Revoking, `0x7061e0(empire, reasons) -> bool`

`HRE_REVOKE_REFORM_NO_REFORMS` with none passed, `HRE_REVOKE_REFORM_ALREADY_DONE` when this emperor has revoked one (`+0x504`), `HRE_REVOKE_REFORM_NO_AUTHORITY` below its cost (`0x233fb10`, i32 thousandths, compared with the authority as it is). Revoking (`0x7f5ad0`) takes the last reform passed out, sets `+0x504` and takes the cost from the authority.

## Decrees

The Celestial Emperor's decrees, defined in `common/decrees/`.

- **The database** is at the global `0x242bb08`, made by `0x479770`: an `std::unordered_map` from key to decree, its list at `+0x08`, a node's decree at `+0x30` (_inferred_ from the node's size, `0x38`).
- **A decree's fields**: key `+0x10` (the enact command's writer, `0x7f3e68`), cost in meritocracy `+0x34`, i32 thousandths (`0x4794b9` `41 8b 7f 34`), `potential` `+0x260` (`0x478fae` reads its children at `+0x268`), `trigger` `+0x2d8`.
- **The check** `0x478c60(decree, tag, reasons or null) -> bool`: `ENACT_DECREE_NOT_EMPEROR` when the tag is not the Celestial Emperor's (world `+0x1b50`), `ENACT_DECREE_ALREADY_ENACTED` while a decree is in force, its `potential` and `trigger`, and `ENACT_DECREE_NOT_ENOUGH_MERITOCRACY`.

## The commands

The command classes, found by their type token in the command vtables (`+0x50`), and their writers (`+0x10`):

| Command | Token | Vtable | Written form | Check, execute |
| --- | --- | --- | --- | --- |
| `enact_imperial_reform` | `0x2b16` | `0x1cb5910` | `enact_imperial_reform={ which="<reform>" }` | `0x7f3d50`, `0x7f2b00` |
| `revoke_imperial_reform` | `0x3a3c` | `0x1c3a058` | `revoke_imperial_reform={ members={ dummy="<emperor>" } }` | `0xc1780`, `0xc17d0` |
| `enact_decree` | `0x35f3` | `0x1cb5a60` | `enact_decree={ decree_name="<decree>" }` | `0x7f40e0`, `0x7f3e10` |
| `pick_imperial_incident_option` | `0x3a2b` | `0x1c39fb0` | `members={ … }` of the option's number (`+0x50`) and a tag (`+0x54`) | `0xc19b0`, `0xc1b10` |

- **`enact_imperial_reform` names no country.** Its check (`0x7f3d50`) takes the empire from the reform's token (`+0xbdc`: `0x2749` the Holy Roman Empire, `0x35b7` the Celestial Empire) and asks the empire's own check (its vtable's `+0x60`) for its emperor, with no reasons: the game enacts it as the emperor, whoever sent it. So the framework checks, as the view does, that the player is the emperor first.
- **`enact_decree`** names no country either: its check (`0x7f40e0`) asks the decree check for the Celestial Emperor (world `+0x1b50`).
- **`revoke_imperial_reform`** carries a tag, which must be the Holy Roman emperor's, and the Empire must stand (world `+0x1620` is 0); then the revoke check.
- **`pick_imperial_incident_option`**: see [Imperial incidents](#imperial-incidents).

## Imperial incidents

The incidents are defined in `common/imperial_incidents/`, each with the id of an event (`event = incidents_bur_inheritance.2`) whose options are the incident's, a default option and a factor for each option, for the AI. The game console starts one with `imperial_incident <key>`.

- **The incident in progress** is at world `+0x1b18`, a pointer, null with none (the pick command's check, `0xc1a1f` `48 8b 9a 18 1b 00 00`):

| Offset | Field | An instruction that uses it (bytes) |
| --- | --- | --- |
| `+0x0` | its definition | `0xc1a2f` `48 8b 0b` |
| `+0x8` | the scope its options are asked in, a scope of the game's own, which the check copies (`0xcf6a0`) before evaluating in it | `0xc1a6e` `48 8d 53 08` |
| `+0xd8` | the option the emperor favors, i32 | the view's option row (`0x1162b30`), `0x1162bd0` `8b 8d d8 00 00 00` |
| `+0xe0` | the day the emperor's choice comes into effect, as the game keeps dates | the view's title tooltip (`IMPERIAL_INCIDENT_TITLE_TOOLTIP`, `$EXPIRY$`), `0x1161167` `49 8d 89 e0 00 00 00` |
| `+0xe8` | the members' picks, a game array (data `+0xf0`, count `+0xfc`) of 12-byte entries: the member's tag at `+0`, the option it supports, i32, at `+8` | `0x1162be9` `48 8b 8d f0 00 00 00`, `0x1162bf0` `48 63 95 fc 00 00 00`, `0x1162cd3` `39 4a 08` |

- **Its definition**: its key at `+0x08`, a `std::string`, also the localisation key of its name (`0x70d290(definition, out)` writes the name, `0x70d29e` `48 8d 51 08`); the id of its event at `+0x28`, a `std::string`, looked up by `0x6ee1d0(id)` in the events' map at `0x2340308` (`0xc1a3b` `48 83 c1 28`).
- **The view's option row** (`0x1162b30`) counts an option's votes as the members' picks of it, of the Empire's princes (world `+0x162c`, i32): `IMPERIAL_INCIDENT_OPTION_SUPPORT` writes `$VOTES$ / $PRINCES$`. The emperor's own pick is `+0xd8` and a member's its entry among the picks; the row's button is enabled for the player as the emperor or a member, and the emperor's favored option is marked `IMPERIAL_INCIDENT_SELECTED_OPTION`.
- **The pick command's check** (`0xc19b0`): the Empire must stand (world `+0x1620` is 0); the command's tag must be the emperor's (world `+0x15e0`, the Empire's `+0x10`) or a member's (the byte at the pointer world `+0x1648`, indexed by the country's `+0x14b8`, nonzero); an incident must be in progress, its event found, the option's number from 0 below the count of the event's options (`+0x790`); and the option's trigger (its `+0xa8`, a content trigger, children at `+0xb0`, count `+0xbc`) must hold in a copy of the incident's scope.
- **The pick command's written form**: its writer (`0xc1900`) writes `members` (`0x2aa9`) as a block of the tuple at `+0x50` (vtable `0x1c38510`), whose writer (`0xc88b0`) writes `dummy` (`0xe6`) twice: the tag (`+0x54`) first, then the option's number (`+0x50`, `0xcbff0`, `"%d"`): `pick_imperial_incident_option={ members={ dummy="<tag>" dummy=<option> } }`.

## What the confirm dialog shows

The reform's button in either view opens one dialog (vtable `0x1d621f0`, built by `0x1164460`), handed the command already made, which it posts (`0x14ca2d0`) when confirmed: `ARE_YOU_SURE_PASS_REFORM` with the reform's name, and for the Celestial Empire `ENACT_REFORM_COST_DESC` (`0x7f2820`), its cost in stability and mandate and its effects. The decree's button has a dialog of its own (`0xe29ee0`, `ARE_YOU_SURE_PASS_DECREE`, `ENACT_DECREE_ONLY_ONE`).

## Not known yet

- How long a decree lasts, as a field.
- What the incident does when its day comes (the emperor's favored option, or the default, fired as its event); not needed to read it or pick.
