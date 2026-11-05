# The province view's actions (for `game::administration` and `game::buildings`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-05 with `cargo xtask disasm` and scripts over the PE file (a scan of `.rdata` for the commands' vtables, whose `+0x50` answers the type token, and the token table); what was tried in the running game is marked "tested" (the run is in [prototype-log.md](../prototype-log.md), 2026-11-05, A3). The fields of provinces, countries and constructions used here are in [provinces.md](provinces.md); how a command is posted from its written form is in [commands.md](commands.md).

## The commands and their written forms

Each command's writer (its vtable's `+0x10`) writes these keys, by the token table's names; the command layer reads every key back, so a key the reader does not read is refused (it refused `second`, below). Tested: every form below was posted from the framework and did what the button does.

| Action | Command (vtable, check `+0x40`) | Written form |
| --- | --- | --- |
| Develop | `spend_power_command` (`0x1c554c0`, `0x2a76d0`) | `spend_power_command={ country="CAS" power={ data=217 category=7 skill=0 } }`: `data` the province, `category` the spending's kind (7 develop), `skill` the power (0 adm tax, 1 dip production, 2 mil manpower) |
| Core | the same | `category=17 skill=0` |
| Exploit | `exploit_development` (`0x1c38810`, `0xc6250`) | `exploit_development={ members={ dummy="CAS" dummy=0 dummy=217 } }`: a tuple of the tag, the kind (0 tax, 1 production, 2 manpower; 3 refused) and the province, written by `0xc8440` in the order tag (`+0x58`), kind (`+0x54`), province (`+0x50`) |
| Raise, lower autonomy | `increase_autonomy_command` (`0x1cdfea8`, `0xaaa190`), `decrease_autonomy_command` (`0x1cdfe00`, `0xaaa560`) | `increase_autonomy_command={ country="CAS" location=217 }` |
| Make a state, a territory | `make_state_command` (`0x1c8d6c0`, `0x5dc560`), `abandon_state_command` (`0x1c8d768`, `0x5dc760`) | `make_state_command={ province=217 }`: no country; both act for the province's owner |
| Set, cancel an edict | `enact_edict` (`0x1c53678`, `0x2bd640`) | `enact_edict={ country="CAS" province=217 which=0 }`, `which` the edict's index (its `+0x2d0`); without `which` it cancels (the writer leaves it out for a null edict, `0x2bd430`, and the execute `0xabd570` then removes the state's edict) |
| Cancel coring, a culture change | `cancel_build_core` (`0x1c4dc08`), `cancel_change_culture` (`0x1c4dcb0`), both writer `0x271330`, check `0x8f440` (always yes) | `cancel_build_core={ country="CAS" province=223 }` |
| Change culture | `change_culture` (`0x1c4de00`, `0x270900`) | `change_culture={ province=4555 culture=castilian }`: no country, the culture by its token's name. The province view's button converts to the owner's primary culture (`0x137b682`, the country's `+0x1228`) and posts this from its confirmation (`0x1367a30`) |
| Build | `constructbuilding` (`0x1c4e490`, `0x2694b0`) | `constructbuilding={ building=6 province=217 builder="CAS" }`: the building by its index |
| Cancel a building | `cancel_building` (`0x1c4db60`, `0x271c30`) | `cancel_building={ country="CAS" province=217 building=6 }` |
| Destroy a building | `destroybuilding` (`0x1c4e0a0`, `0x26a3b0`) | `destroybuilding={ building=6 country="CAS" province=219 }` |

- **The power spending** written under `power` (writer `0x94eed0`) also has `second` (`+0x1c`, -1 from the constructor), which the reader does not read back (tested: the layer refused it), `negative_power=yes` when its `+0x30` is set, and the rebel faction's ID when its `+0x28` resolves to one. The AI's commands, heard by the command log, write the same: `power={ data=12 category=23 skill=2 }`.
- **Which country**: the state and culture commands carry no country and act for the province's owner, so the framework refuses them for a province the player does not own before posting. The destroy command's check asks only that the building stands (`0x26a3b0`) and its execute (`0x269f10`) destroys it for whichever country; the framework refuses a province the player does not own.

## The checks, with the reasons the tooltips give

Each is a function of the game's that answers whether the player may, and appends to a `std::string` the reasons when not; most append a description of the effect when it may, which the framework does not use. The province view calls them for its buttons' tooltips (`0x1379580`, by the buttons' names: `tax_base_increase_button`, `mp_base_exploit_button`, `change_culture_button`, `add_core_button` and so on).

- **Develop**: `0x3e6ec0`(country, province, kind, reasons, describe), the reasons `IMPPR_*` (not owned, not controlled, under siege, a colony, not a core, too much of one kind, not enough power). Tested: "你需要54 行政点数才能在马德里执行提高基础税收。", "格拉纳达还不是我们的核心领土，所以暂时无法执行提高基础税收。", "这个省份不属于你或你属国…".
- **Exploit**: `0x407350`(country, kind, province, reasons), the reasons `CAN_NOT_EXPLOIT_*`. Tested: "这里没有水手。", and after exploiting, "－最近已经剥削过蓬特韦德拉的发展度，因此 1464.11.11 之前不能再次剥削。" (its bullet `－` left out by the framework).
- **Autonomy**: the buttons' tooltips `0x3dc1c0` and `0x3dca00`(country, province, reasons) answer whether it may, with the one reason why not (`AUTONOMY_*`), or the effect. Tested: "当省份的自治度低于10.00%时，无法再手动降低其自治度。", "近期自治度刚被提高，因此在1474年11月11日前无法再次对其进行改变。", "由于自治度下限为90.00%，降低自治度不会有任何效果。", "无法改变不属于你的省份的自治度。".
- **State, territory**: `0x3f34f0` and `0x3f3bd0`(country, province, reasons), the reasons `MAKESTATE_*` and `ABANDONSTATE_*`; both append the button's own text (`MAKESTATE_MAKE`, `ABANDONSTATE_MAKE`) after the reasons, which the framework leaves in, as the tooltip shows it: "加利西亚已经是直属州了。加利西亚是自治领地", "（点击设置为直属州）". The make-state command's check is the first with the owner; the abandon-state command's check (`0x5dc760`) also refuses the capital's area and asks `0xacccf0`, which the tooltip does not.
- **Edict**: `0xabcbb0`(country state, edict or 0 to cancel, reasons), what the edict command's check asks (with no reasons). Tested: "1445.11.11之前促进进步不可以被改变或者移除。" (an edict cannot be changed for a year after it is set), "当前是专制主义时代。", "需要专制主义时代的「专制主义法令」时代能力。", "当地没有法令可供废除！".
- **Core**: `0x382930`(country, province, reasons, 0, 0, 1), as the core button asks it (`0x137b9eb`). Its reasons begin with `- `, which the framework leaves out. Tested: "这个省份已经是我们的核心。", "将这个省份核心化需要150adm，而你只有42。", "只能将属于我们的省份转换为核心省份。", "我们正将格拉纳达转化为核心省份。 于1446.05.11完成。".
- **Culture**: `0x38ba40`(country, province, reasons). It does not ask whether the player has the points: the button asks that separately (`0x137b5d9`–`0x137b652`) and says so with `NOCHA_NO_POINTS`, its `$COST$` and `$VAL$` filled in through `0x8edc0`(out, key, name, value, name, value), each value a `std::string`. The framework does the same. Nor does it ask whether the culture is the primary one already, where the province view shows no button: the framework refuses that itself. Tested: the four reasons of a province not owned, not controlled, not a core and not of the state religion; "我们正在改变蓬特韦德拉的文化。 于1450.09.11完成。".
- **Build**: `0x32acb0`(country, building, province, reasons, 0, 0, 0), as the build command's check asks it but with the reasons; the check then compares the price with the treasury, which the framework does too, giving no reason of its own when only the money is short. Tested: "集市需要外交科技达到4级。", "该省份不满足造船厂的要求。", "这个工场与马德里出产的产品不适配。", "城堡已正在马德里建造".
- **Cancel a building**: `0xa1b540`(province, building, tag, reasons): there must be a building construction (`0xa1b4a0`) of that building whose builder (`+0x88`) is the tag; otherwise `CANNOT_CANCEL_OTHERS_BUILDING`.
- **Destroy**: no check of the game's with reasons was found; the building panel's tooltip (`0x138b310`) writes `DESTROY_BUILDING` and, under siege, `MACRO_UNDER_SIEGE` (from the province's `+0x388`, not worked out). The framework refuses a province the player does not own and a building that does not stand there.

## Prices

- **Monarch power**: the power spending's constructor `0x94ee80`(spending, kind, province, power) sets the province's ID (`+0x18`) from the province object's `+0x20`; `0x9526d0`(spending, tag, explanation, 0) answers the price in whole points. The object is `0x38` bytes and needs no destructor (its vtable's `+0x28` is `ret`). Tested: developing Madrid cost 54 and then 55, coring 70 to 150 adm, as the tooltips say.
- **Culture**: `0xa14e60`(province, tag, culture, explanation or 0, primary or 0), whole diplomatic points, what the command's check compares with the country's `+0x264`. Tested: 37 to 140.
- **A building**: `0x32a420`(country, out, building, province), fixed-point ducats. Tested: 100 for a marketplace, 200 for a fort, 500 for a manufactory.

## Databases

- **Edicts**: `[0x242bdc8]` (made by `0xabaa00`), the edicts by index a vector of pointers at `+0x10`; an edict's key, a `std::string`, at `+0x10` and its index at `+0x2d0`. A state's active edict (`+0x28` of the country state) holds the edict at its `+0x10`. 30 edicts in the unmodded game with every DLC.
- **Buildings**: `[0x242ba88]` (made by `0x1e5390`), the buildings by index a vector of pointers at `+0x10` (the build command's reader takes `building=` by it, `0x26936d`), index 0 the placeholder.

## A building finished

The building construction (vtable `0x1c4b498`, made by `0x253340`) is finished by its `+0x48`, `0x253810`(construction): it reads the province (`+0x38`) and the building (`+0x80`), takes out what the building replaces (the chain at the building's `+0x20`), puts the building in its slot and tells the owner. The framework observes it and, when someone listens to `building_finished`, keeps the province's ID, the building's index and the province's owner, and names the building by its key at the next safe frame.
