# Units and fleets: merging, splitting, disbanding, missions, transport (for the player actions, A4)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-05 with `core/examples/disasm.rs` and capstone scripts (scratchpad `work_units\`). Nothing here was tried in the game. What is marked _inferred_ was concluded from the shape of the code. How commands are written, read and posted is in [commands.md](commands.md); the right click and `move_order` in [army-orders.md](army-orders.md); the route query in [routes.md](routes.md).

## The unit panel's actions

- **The unit toolbar is a set of action objects**, one class per button, each with a vtable of ten slots: `+0x00` destructor `0x147a560` (or `0x116c0a0`), `+0x10` **execute** (what a click does), `+0x20` visible, `+0x30` toggled on, `+0x38` **can do** `(action, std::string* reason) → al` (writes the tooltip's reason when the pointer is not null), `+0x40` name key, `+0x48` description. The object's `+0x08` is a context whose method `+0x08` answers the unit and `+0x10` the interface (`[0x233fe78]+0x1e00`); the interface's `+0x1b80` is the selection (its list at `+0x28`), `[+0x330]+0x370` the session the command is posted to.
- **The base can-do `0x147d840`(action, reason)** refuses a unit whose owner (`+0xd0`, index) is not the player (the getter of [armies.md](armies.md)), with `CANNOT_PERFORM_ACTION_NOT_YOUR_UNIT`. Every action's `+0x38` calls it first.
- The panel posts with `0x14ca2d0` (or that post inlined: the handler at the session's `+0x58`, its `+0x30`) **without calling the command's check**; the session checks it before executing.
- The reason text is a `std::string` (MSVC: data or SSO buffer `+0x00`, size `+0x10`, capacity `+0x18`); `0x17055d0` adds a line break when it is not empty, `0x14b34a0` looks a key up. So **the can-do functions below can be called by the framework with a reason string of its own**, or with null.

| Toolbar class (vtable) | Button | Execute | Can do (`+0x38`) | What it calls |
|---|---|---|---|---|
| `0x1d88f70` | `UNIT_SPLIT_HALF` | `0x147fa70` | `0x147f8c0` | no reason text; same conditions as `0xd63fc0` |
| `0x1d88fc0` | `MERGE_UNITS` | `0x14813a0` | `0x1480d00` | inline copy of `0x342f60` |
| `0x1d89010` | `REORGANIZE_UNITS` | `0x1480c70` | `0x1480520` | opens the reorganise panel (not followed) |
| `0x1d88f20` | `DISBAND_UNIT` | `0x147fcc0` (confirm dialog) | `0x147fc50` | `0x3428c0` |
| `0x1d88d40` | `CONSOLIDATE_BUTTON` | `0x1482620` | `0x14825a0` | unit method `+0x148` (army `0x1a5610`) |
| `0x1d892e0` | `ENABLE/DISABLE_ARMY_DRILL` | `0x147edf0` | `0x147eda0` | `0xd40ce0` |
| `0x1d890b0` | `MOTHBALL_TITLE` | `0x1481710` | `0x14816b0` | `0x90e310` |
| `0x1d88850` | `CONNECT_TO_SHIP` / `DISCONNECT_TO_SHIP` | `0x1487170` | `0x1487110` | `0xd55e70` |
| `0x1d88940` | `UNIT_ATTACH` / `UNIT_DETACH` | `0x1487490` | `0x1487390` | `0xd565b0` (`NO_TARGETS_TO_ATTACH_TO`) |
| `0x1d88ca0` | `START/STOP_AUTO_TRANSFER_MODE` | `0x14864d0` (`toggle_auto_transfer`) | `0x1486350` | `TRADE_MOTHBALLED`, `AUTO_TRANSPORT_NO_COG` |
| `0x1d891f0` | `UNIT_DETACH_MERC` | `0x147e350` (`detach_merc`) | `0x147e300` | `0x3454a0` |
| `0x1d88d90`, `0x1d88e30` | detach siege, detach blockade | `detach_siege`, `detach_blockade` | | `0x345c60`, `0x346000` |

The patrol and evade buttons are not toolbar actions but handlers of the single unit panel (below). The fleet's naval missions are chosen in the `naval_missions` window and then on the map (below).

## A. Merging, splitting, disbanding, consolidating, drilling

### `merge_units` (vtable `0x1d1dbd8`, token `0x29cf`)

- **Written form**: one `unit={ id= type= }` per unit, in order (write `0xd60a80`: a list at `+0x50` (head), `+0x58` (tail), `+0x60` count; nodes of 0x28 bytes: `+0x00` the unit, `+0x08` its ID, `+0x10` previous, `+0x18` next; the reader `0xd60b70` appends one node per `unit` key). Example: `merge_units={ unit={ id=127 type=54 } unit={ id=131 type=54 } }`.
- **The button** (`0x14813a0`): every selected object whose selection kind (its method `+0x08`) is 0 or 1 (armies and fleets), in selection order, goes into the list; it posts and plays `mergearmynavy`. The first unit is the one the others merge into (inferred from the order).
- **Check** `0xd60d10`: each unit found by its ID; armies and fleets not mixed; all of the first unit's owner, in its province (`+0xe8`); none in a battle (`+0xc0`), `+0x210` set, retreating (`+0x203`, `+0x204`), on a ship at sea (army `+0x218` with the location not land), carrying troops (fleet `+0x228` > 0), movement-locked (`+0x118`), hired (`+0x90`, `+0x98`); a mercenary company (`+0x48`) only with the modifier `0x222` on the country or the company; missions that agree (the mission's `+0xb0`); at least 2 units that are not attached (`+0x48`). Execute `0xd60020`.
- **Can do with reasons**: `0x342f60`(country, const list* units, std::string* reason, u8) → `al` (the game's list: head `+0x00`, count `+0x10`; nodes with the unit at `+0`, next at `+0x10`); the siege view calls it with the player's country (`0x13b1603`). Keys: `NO_REORGANISE_SINGLE_UNIT`, `UNIT_VIEW_CREATE_NEW_TRADE`, `MERGE_MOTHBALLED`, `UNIT_EXILE_DESC`, `CANT_SPLIT_WHILE_TRANSFER`, `NO_REORGANISE_UNITS_DIFFERENT_LOCATION`, `NO_MERGE_UNITS_OF_DIFFERENT_COUNTRIES`, `CANT_MERGE_MERCENARY_COMPANY`, and `0xd45de0`'s `NO_REORGANISE_UNIT_*`. Cost: none.

### `split_unit` (vtable `0x1d1d740`, token `0x2aa2`)

- **Written form** (write `0xd63dc0`, read `0xd63e70`): `unit` (`+0x50`/`+0x58`), `other` (`+0x60`/`+0x68`, a unit to move the subunits into; the empty ID when none), `value` (`+0x70`, i32, how many regiments or ships to split off). Constructor `0xd63680`(command, unit, value).
- **"Split in half"** (`0x147fa70`, the only split the toolbar offers; shortcut `s`): `value` = ceil(n / 2) for n subunits (`+0x30`..`+0x38`), `other` empty; plays `split_army_fleet`. Example: `split_unit={ unit={ id=127 type=54 } value=5 }`.
- **Check** `0xd64140`: both units found; `0xd63fc0`(unit) for the unit and for `other` when given; more than one subunit; 0 < `value` < n; `other` of the same owner. Execute `0xd63b80`.
- **Can split**: `0xd63fc0`(unit) → `al` (reads 2 arguments; the second is only spilled), no reason text: not in a battle (`+0xc0`), `+0x210` clear, not retreating (`+0x203`, `+0x204`), an army not aboard at sea (`+0x218`), a fleet not carrying troops (`+0x228`), not locked (`+0x118`), not hired (`+0x90`, `+0x98`), a mercenary company only with modifier `0x222`. Prologue `48 89 5c 24 08 48 89 54 24 10 57 48 83 ec 20 48 8b d9 48 85 c9 0f`. The toolbar's `0x147f8c0` adds the owner test and more than one subunit. The keys `CANT_SPLIT_*` are written by the merge and detach can-dos (`0x342f60`, `0x3454a0`, `0xd64770`), not by the split button.
- **Detach by type** (`detach_subunit`, vtable `0x1d1d7e8`: `unit`, `type` (`+0x60`, i32), `select_army` (`+0x64`, bool)) is posted by the unit card's type icons (`0x1465f60` → `0x1465e20`); can do with reasons `0xd64770`(unit, type, std::string*) (`DETACH_SUBS_OF_TYPE_*`, `CANT_SPLIT_*`). `detach_unit` (`unit`) is the toolbar's detach of an attached army (`0x1487490`).

### `disband_unit` (vtable `0x1d1dd28`, token `0x29cd`)

- **Written form**: `unit`, `delete_only_empty` (`+0x60`, bool). The button opens the confirm dialog (`0x147cd00`, 0x1d0 bytes: `CONFIRM_DISBAND_TITLE`, `CONFIRM_DISBAND_DESC`); its accept `0x147d6a0` posts `delete_only_empty=no`. Example: `disband_unit={ unit={ id=127 type=54 } }`.
- **Check** `0xd5fac0`: the unit found and `0x3428c0`(owner country, unit, null); with `delete_only_empty`, only a unit without subunits. Execute `0xd5f1b0` (logs `DISBAND`, `disband_unit`).
- **Can do with reasons**: `0x3428c0`(country, unit, std::string* reason) → `al`. Keys in order: `EStopMissionReason_InCombat` (`+0xc0`), `DISBAND_UNIT_CONDOTTIERI` (`+0x90`), `UNIT_LOADED_ON_A_SHIP` (army `+0x218`), `HAS_FOREIGN_UNITS_ABOARD` (fleet), `IN_ENEMY_TERRITORY` (an army in a province whose controller `+0x470` is hostile), `DISBAND_UNIT_CANT` (hostile armies in the province or next to it). Prologue `48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8b ec 48 83 ec`.
- **Cost**: none; the description (`0x147fd60`) tells the manpower or sailors returned (`DISBAND_UNIT_RETURN_MANPOWER(_CAPPED)`, `..._SAILORS`); not followed.

### `consolidate` (vtable `0x1d1d350`, token `0x2caf`)

- **Written form** (write `0xd693f0`): `unit`, then `empty={ yes yes ... }`, 23 booleans (`+0x60`..`+0x76`), indexed by a subunit's `+0x64`. Constructor `0xd69160`(command, unit, flag) fills all 23 with the flag. The button sets the flag when Shift is held (the action's `+0x10`) or the army is a mercenary company (`+0x48`): `CONSOLIDATE_BUTTON2` says Shift leaves 0-strength regiments behind. Execute `0xd691f0`: pours the strength (`+0x28`, max `[type+0x20]+0x58`) of regiments of the same type and `+0x64` into each other and removes an emptied regiment unless its `empty` is yes. Example: `consolidate={ unit={ id=127 type=54 } empty={ no no no no no no no no no no no no no no no no no no no no no no no } }`.
- **Check** `0xd695c0`: found and not in a battle. **Can do**: the unit's method `+0x148`(unit, flag, std::string* reason) (army `0x1a5610`: `EStopMissionReason_InCombat`, `CANNOT_CONSOLIDATE_FULL_STRENGTH`); armies only (`+0x20` of the action is "not a fleet"). Cost: none.

### `drill_army_mission` (vtable `0x1d1c780`, token `0x370a`) and `stop_unit_mission` (vtable `0x1d1bda8`, token `0x3bd7`)

- **Written forms** (shared write `0xd672a0`, read `0xd67340`): `unit`, `reason` (`+0x60`, u8, written as a number). The drill button (`0x147edf0`) posts `drill_army_mission` with `reason=54` whether drilling or not: **its execute `0xd67210` toggles**: when the army's mission is drilling it stops it (`0xd3b8b0`(army, 1, reason, 0, 0)), otherwise starts drilling (`0xd6fbd0`). Example: `drill_army_mission={ unit={ id=127 type=54 } reason=54 }`.
- **Drill check** `0xd674b0`: an army and `0xd40ce0`(army, null, null). **Can drill with reasons**: `0xd40ce0`(army, std::string* reason, u8* out) → `al` (3 arguments; prologue `48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48`): `NO_DRILL_MERCENARY`, `REQUIRED_TECH_FOR_DRILLING` / `EStopMissionReason_TechDoesntAllowIt`, `NO_DRILL_NOT_OWNER`, `NO_DRILL_ATTACHED`, `NO_DRILL_NOT_CONTROLLER`, `NO_DRILL_IN_COMBAT`, `NO_DRILL_NO_LEADER`, `NO_DRILL_NO_EXILED`, `NO_DRILL_MOVING`, `NO_DRILL_CANT_MOVE`. The button is shown on (toggled, `0x147f780`) when the mission's type is 15. Cost: none up front (`ARMY_DRILL_DESC`: full maintenance, low morale while drilling; not followed).
- **`stop_unit_mission`**: execute `0xd6cd90` = `0xd3b8b0`(unit, 1, `reason`, 0, a qword at `+0x64` that is not written). Check `0xd6cec0`: found; for a fleet, it has a mission of a type other than 0, and a type-5 mission only when `0x910550`(fleet, 0) allows. The evade button posts it with `reason=54` to stop evading (below); `0x1463eed`, `0x142f982` and `0x14531d2` post it too.
- **`reason`** is `EStopMissionReason` (names by `0xd7a080`, a switch on reason − 2): 2 Exiled, 3 NoLeader, 4 Retreating, 5 Completed, 6 CannotReach, …, 11 StartingNewMission, 13 Exploring, 19 InCombat, 20 Moving, 24 OverriddenByNewCommand (`0x18`, the right click's), 31 Drilling, 54 UserAction (`0x36`, the buttons'), 55 TechDoesntAllowIt.

## B. Fleet missions

- **A unit's mission** is the object at `+0x1f8` (armies and fleets alike); its method `+0x50` answers the type. Naval (vtables, `+0x50`): 1 protect trade `0x1ccb838`, 2 privateer `0x1ccbdd8`, 3 pirate hunt `0x1ccbbf8`, 4 patrol `0x1ccba18`, 5 explore `0x1ccbb08`, 6 evade `0x1ccb658`, 7 hunt fleets `0x1ccb748`, 8 blockade `0x1ccb568`, 9 intercept `0x1ccbec8`, 10 repair `0x1ccbce8`, 11 `0x1ccb928` (its text is `EXPLORE_MISSION_CIRCUMNAVIGATE_TITLE`), 0 none (`0x1ccbfb8`, `0x1ccc0a8`, `0x1ccc198`). Army: 12 explore land `0x1c43fe0`, 13 hunt rebels `0x1c440c0`, 14 carpet siege `0x1c43f00`, 15 drill `0x1d1de78`. `0xd59fa0`(unit) answers "exploring" (5, 11, 12). A fleet's `+0x1f8` is read without a null test in several places (inferred: a fleet always has one, type 0 when idle); an army's may be null.
- **A fleet on a mission of type 5, 11 or 12** may take another order only when `0x910550`(fleet, 0) says so (checked by every mission command below).

### `naval_region_mission` (vtable `0x1d1bfa0`, token `0x33e3`): hunt, blockade, intercept

- **Written form** (write `0xd6db60`, read `0xd6dc40`; the base keys first): `type` (`+0x50`, i32), `unit` (`+0x58`/`+0x60`, the fleet), `region` (`+0x54`, i32). Example: `naval_region_mission={ type=8 unit={ id=210 type=54 } region=47 }`.
- `type`: 7 hunt enemy fleets (`0xd70fc0`), 8 blockade (`0xd71270`), 9 intercept (`0xd714a0`); others do nothing (execute `0xd6daa0`).
- `region`: an index into the vector at `[0x233fed8]+0x4f0` (the map's regions; the check wants it in range and not null). The GUI takes it from the clicked map object (`[[[obj+0x18]+0x40]+0x18]+0x7c`, inferred: province → area → region) and only when it is among `0x91db30`(fleet, std::vector<int>*) (the regions the fleet may choose; 2 arguments) (`0x1475440` → `0x1475f70`; the mission chosen in the `naval_missions` window is its `[...+0x2b0]+0x08`, type 5 excluded).
- **Check** `0xd6ddd0`: type 7, 8 or 9; a switch of the game's on (`0x233ff3c` with `0x233fed6` clear, or the thread's flag: the one that gates hired armies in [armies.md](armies.md); inferred a DLC's); the fleet found; the region valid.

### `trade_mission`, `privateer_mission`, `pirate_hunt`: protect trade, privateer, pirate hunt

- **One written form** (write `0xd6a780`, read `0xd6a850`): `unit` (`+0x50`/`+0x58`), `index` (`+0x60`, i32), `start` (`+0x68`, a province pointer written as its ID `+0x20` and read back into the province array `[0x233fe78]+0x1ca8`, 0x2e10 bytes each), `replace` (`+0x70`, bool). Example: `privateer_mission={ unit={ id=210 type=54 } index=12 start=1301 replace=yes }`.
- **The map's trade node mode** (`0x1419470`(panel, node), from the map input `0x8317c0` with `[hover+0x120]`, validated by `0x1418e70`): the panel's `+0x288` says which mission was chosen (1 protect trade `trade_mission` token `0x2dbb`, constructor `0xd6a420`(cmd, fleet, index, start, replace); 2 `privateer_mission` `0x2f70`; 3 `pirate_hunt` `0x313c`), `index` is the clicked trade node (inferred), `start` the fleet's province (`+0xe8`), `replace` yes. The panel posts only for a fleet with a ship of category 4 (protect trade, privateer) or 3 to 5 (pirate hunt) (`[sub+0x20]+0x10`), not mothballed (`+0x281`), and not exploring unless `0x910550` allows.
- **Checks**: `trade_mission` `0xd6aa10`, the fleet found and `start` set; `privateer_mission`/`pirate_hunt` `0xd6aef0`, also `0x974e60`(command) first (not followed). Executes `0xd6a4a0`, `0xd6ab40`, `0xd6b030`.

### `patrol` (vtable `0x1d1cb70`, token `0x2a66`) and `evade_mission` (vtable `0x1d1c048`, token `0x33e9`)

- **`patrol={ unit={ ... } }`** (write `0xd5ec00`, `unit` only), posted by the panel's patrol button (`0x1452ff0`, plays `navy_move`). **Execute** `0xd65df0` toggles: it stops the current mission (`0xd3b8b0` with reason 11); when that was a patrol it stops there; otherwise it starts a patrol between the fleet's province and **the last province of its route** (`+0x108`, the route's count `+0x110` above 0). **Check** `0xd65fb0`: a fleet, already patrolling or with a route, not mothballed, exploring only when `0x910550` allows, the mission's `+0x70`, the location's owner…
- **`evade_mission={ navy={ ... } }`** (write `0xd6df60`, token `navy`), posted by the evade button (`0x14531b0`) when the fleet is not evading; when it is (mission 6) the button posts `stop_unit_mission={ unit=… reason=54 }` instead (`STOP_EVADE_MISSION`). **Execute** `0xd6def0`: evade towards the route's last province (`0xd71720`). **Check** `0xd6e070`: the switch above, `0xcfc70(0x233ff3c)`, the fleet not mothballed, not exploring unless allowed, the mission's `+0x70`, a route (`+0x110` > 0): `EVADE_MISSION_REQUIRES_DESTINATION` ("first right-click on a destination").
- So **patrol and evade are given after a move order**: the fleet needs a route first.

## C. Fleets moving

- **The fleet's action is the army's**: method `+0x30` of the fleet class `0x1cca7e0` is `0xd423a0` too, so a right click with a fleet selected goes through the selection's action `0xad0ba0` and the same `0xd429e0` ([army-orders.md](army-orders.md)), which makes a `move_order` with `0xd5cd20` and posts it. **The written form is the same**: `move_order={ unit={ id=… type=… } province={ 1301 } location=1295 clear=yes cancel_autofleet=yes }` with the fleet's ID (low half of `+0x20`; the fleet's ID type was not checked, inferred 54 as every unit's constructor `0xd3b0a0` gives).
- **The route query** for the preview/order in `0xd429e0` (`0xd4392b`): normal policy `0x1c3af98`, options with `+0x08` and `+0x09` set, `+0x0c` = −1, tag `---`, the rest zero; from the unit's province, or from its route's end (`0xd511a0`/`0xd51170`) when locked or queuing. The order's execution queries again as in [routes.md](routes.md) (`0xd5d680`). No option is fleet-specific: the edge check tells land from sea by the unit.
- **Played sounds**: `navy_move` for fleets, `army_move` for armies, `illegal_moveorder` when refused.

## D. Transport as the player does it

1. **An army ordered onto a sea zone with an own fleet**: a plain `move_order` to the sea zone (the route's last step onto the sea is allowed when `0x9dd6a0` finds ships, [routes.md](routes.md); the access query's `BOARD_SHIPS_ACCESS`). The move's execution `0xd5cf90` stops the army and loads it, `0x90ada0`(fleet, army) at `0xd5d490`/`0xd5d512`; an army larger than the fleet's capacity is split first (`split_unit` built inline and executed, `0xd63b90`) and the split part loaded. **`load_unit`** (`unit`, `navy`; check `0xd69bf0`, execute `0xd699a0` = `0x90ada0` for an army not aboard) is posted only by the AI (`0xd69910` from `0xf1390`, `0xf2e90`); the framework could post it too.
2. **The link**: an army aboard has its fleet at **army `+0x218`** (null on land; `0xd43996`: `48 83 b8 18 02 00 00 00`); a fleet has **the list of armies aboard at `+0x218`** (nodes: unit `+0`, next `+0x10`; `0xd44362`: `48 8b 9b 18 02 00 00`) and **their count at `+0x228`** (i32; `0xd44342`: `83 b8 28 02 00 00 00`). Both are reached through the casts `+0x60`/`+0x68` (as army) and `+0x70`/`+0x78` (as fleet), which answer the unit itself (`0x94f20`) or null (`0x8f480`).
3. **An army aboard ordered to a coastal province**: with the army selected, a plain `move_order` for it. With the **fleet** selected and a land province clicked (`0xd44322`): when the fleet carries troops and `0x90aee0`(fleet, province) allows, `0xd429e0` posts **one `move_order` per army aboard** that `0xd45910`(army, province, army's province) accepts (clear as for a right click, `safe` and `clear_all_but_next` 0). **`unload_unit`** (`unit`; check `0xd69820`: aboard and standing on land; execute `0xd69660` = `0x90ae50`(fleet, army)) is posted only by the AI (`0xd4c70`).
4. **Automatic transport** (`start_auto_fleet_transport`, vtable `0x1d1bd00`, token `0x30e8`):
   - **Written form** (write `0xd6c7d0`, read `0xd6ca10`): `army={ id= type= }` for each army (vector of {pointer, ID} at `+0x50`..`+0x60`), `navy={ … }` for each fleet (`+0x68`..`+0x78`), `province_id` (`+0x80`, i32, the destination), `force` (`+0x84`, bool). Constructor `0xd6c1a0`(cmd, armies vector, navies vector, province, force). Example: `start_auto_fleet_transport={ army={ id=127 type=54 } navy={ id=210 type=54 } navy={ id=215 type=54 } province_id=236 force=no }`.
   - **When the game issues it**: in the right click (`0xd429e0`, from the interface only, `0xd43a8f`..`0xd441b1`), for an own army not aboard, when the switch `0xcfc70(0x233ff94)` is on and either no land route was found or the unit manager's `[+0x140]+0x68` method `+0x48` answers yes (that answer is also `force`; what it asks was not worked out, inferred a modifier key or the setting). It gathers the selected armies not movement-locked, the fleets that may carry them (`0x342170`(country, std::vector*, army)), refuses when one of them is already busy (`0xd59fd0`), queries a route **with the options `+0x04`, `+0x05` and `+0x09` set** (`+0x0c` −1, tag `---`; `0xd43c75`..`0xd43d09`), and asks `MOVE_AUTO_TRANSPORT_DIALOG` (dialog vtable `0x1d27048`: armies `+0x1b8`, fleets `+0x1d0`, province `+0x1e8`, force `+0x1ec`). Its accept `0xe15030` makes and posts the command.
   - **Check** `0xd6cc40`: every army and fleet found by its ID. Execute `0xd6c540` hands them to the transport planner `0x1d0e20`, which splits (`split_unit`) and connects (`connect_to_ship_command`) as needed.
   - A fleet's "Enable Automatic Transportation" toggle is a different command, `toggle_auto_transfer` (`0x14864d0`).
5. **`connect_to_ship_command`** (vtable `0x1d1db30`, token `0x2e22`): `unit` (an army in port), `do_connect_to_ship` (bool, `+0x60`) = the army's byte `+0x20d` inverted (constructor `0xd5e590`; `0xd5e60f`: `48 8d 88 fd 01 00 00`, `+0x1fd` from the registered `+0x10`): "Attach to Transport", the army boards when the fleet leaves. Can do with reasons `0xd55e70`(army, std::string*): `CONNECT_TO_SHIP_(PENDING_)CONDOTTIERI`, `NO_FRIENDLY_TRANSPORT_SHIPS_IN_PORT`, `CONNECT_TO_SHIP_LOW_MORALE`, `UNIT_MOVEMENT_RETREATING`, `CONNECT_TO_SHIP_LOCKED`, `UNIT_IN_BATTLE`.
6. **`attach_unit`** (`unit`, `target`): the toolbar's attach (`0x1487490`) posts `detach_unit` for an attached army, otherwise gathers the targets (`0xd56750`) and posts `attach_unit` when there is one, or opens `attach_unit_window` to pick.

## E. Unit fields

| Field | Offset | One instruction | Bytes |
|---|---|---|---|
| ID (u64: low type, high number) | `+0x20` | [armies.md](armies.md) | |
| Subunits (regiments or ships), vector of pointers | `+0x30`..`+0x38` | `0x147fabe` | `48 8b 4f 38 4c 8b c8 48 2b 4f 30` |
| Subunit's type, its category (`[sub+0x20]+0x10`; 4 light ship, 3–5 warships, 6 transport, inferred 0–2 the army's) | | `0x14194ca` | `83 79 10 04 74 06` |
| Location (a fleet's sea zone too) | `+0xe8` | `0xd5f0a1` | `48 8b 90 e8 00 00 00` |
| Route: last node, count | `+0x108`, `+0x110` | `0xd6e15f` | `39 bb 10 01 00 00 7e 57 48 8b 83 08 01 00 00` |
| Mission object | `+0x1f8` | `0x147f7b1` | `48 8b 88 f8 01 00 00 48 85 c9 74 13 48 8b 01 ff 50 50` |
| Mission type (method `+0x50`), 15 drill | | `0x147f7c0` | `ff 50 50 83 f8 0f` |
| Mercenary company | `+0x48` | `0xd6102a` (merge check) | |
| Unknown flag that refuses split and merge | `+0x210` | `0x147f913` | `80 b8 10 02 00 00 00` |
| Army: the fleet it is aboard | `+0x218` | `0xd43996` | `48 83 b8 18 02 00 00 00` |
| Army: connected to a ship (attach to transport) | `+0x20d` | `0xd5e60f` | `48 8d 88 fd 01 00 00` (from `+0x10`) |
| Fleet: armies aboard (list), count | `+0x218`, `+0x228` | `0xd44362`, `0xd44342` | above |
| Fleet: mothballed | `+0x281` | `0x1481774` | `80 b8 81 02 00 00 00` |

- **Drilling**: the mission's type is 15 (`0xd67253`: `83 f8 0f`).
- **Mothballing** (`mothball_command`, vtable `0x1d1d9e0`, token `0x306d`): `unit`, and the on/off value under the key `connect_to_ship_command` (`+0x60`, bool; the button sets it to `+0x281` inverted, `0x1481710`). Execute `0xd5ee30` writes it to `+0x281`. Check `0xd5f060`: a fleet in a province with a valid owner and `+0x550` set (a port, inferred) and no route. Can do with reasons `0x90e310`(fleet, std::string*): `NAVY_MOTHBALL_NO_PORT`. A mothballed fleet does not move, repair or take missions and costs less (`MOTHBALL_DESC`). Example: `mothball_command={ unit={ id=210 type=54 } connect_to_ship_command=yes }`.

## Not worked out

- What `+0x210` is, what `0x910550` and `0x974e60` test, what the unit manager's `[+0x140]+0x68` method `+0x48` answers (`force`), which switch `0x233ff3c` and `0x233ff94` are.
- The region index's source on the map, the trade node index's (`[hover+0x120]`), and the 23 entries of `consolidate`'s `empty`, are inferred from the code's shape only.
- The fleet's ID type; the costs, which none of these commands spends except as described in their descriptions (not followed).
- The panels' enabled-state code for patrol and evade (`0x1450660`) was not read; their conditions above are the commands' checks.

## What the framework does, and what the game showed

`game::units`, `game::orders` and `game::armies`; tried on 2026-11-05 ([prototype-log.md](../prototype-log.md), "Player actions A4").

- **The can-do functions** answered as the panel's tooltips: `0x342f60` with a list the framework builds of its own nodes (the game's list: head, tail, count; nodes with the unit, previous and next), `0x3428c0`, `0xd40ce0` (its third argument a byte of the framework's), and the army's method `+0x148`. Their text starts each line with `-`, which the framework leaves out.
- **Every command read back as written** and ran: `split_unit` with `value` (14 regiments to 7 and 7; 32 ships to 16 and 16), `merge_units` with two `unit` keys, `disband_unit`, `consolidate` without `empty` (14 regiments to 12 after a battle), `drill_army_mission` (started and stopped), `move_order` for a fleet, `patrol`, `trade_mission`, `naval_region_mission`, `stop_unit_mission`, `start_auto_fleet_transport` with the fleets `0x342170(country, std::vector*, army)` answered (a vector of pointers, freed with `0x17394f0`), which carried an army from London to Calais.
- **Not seen**: the route query answering a route for an army onto a sea zone with the player's fleet in it (none was found from London or Dumfries; whether that fleet had transports was not read), and so boarding and landing by a move order.
