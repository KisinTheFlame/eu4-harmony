# The religion view: its commands, personal deities, cults, church aspects and blessings (for `religion`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with scripts over the PE file (the `.rdata` scan for command vtables of [commands.md](commands.md), the token table at `0x1fd8660`, capstone) and the game's own files (`common/personal_deities`, `common/fetishist_cults`, `localisation/religion_l_english.yml`); what was tried in the game is in [the run](../prototype-log/2026-11-11-religion-deities-cults.md). Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md) is built on it: the `religion` group follows personal deities and cults, chooses a church's aspects and takes blessings; the other commands below are found and not yet offered.

## The religion view's commands

Each command class answers its type token from its vtable's `+0x50`; its writer is `+0x10`, its reader `+0x20`, its check `+0x40`, its execute `+0x48` ([commands.md](commands.md)). The view's button that posts each was found by the code that stores the vtable (`lea` of it): the interface's (from `0xd…` up) besides the AI's (`0xa…`).

| Command (vtable) | Writer, check, execute | The view's poster | What |
|---|---|---|---|
| `personal_deity` (`0x1ce0bd8`) | `0xaae070`, `0xaae300`, `0xaae030` | `0xfddb00` | Follow a personal deity (Hinduism, Norse); below |
| `fetishist_cult` (`0x1ce07e8`) | `0xaae550`, `0xaae6a0`, `0xaae3f0` | `0xfde840` | Embrace a cult (Fetishism, the Dreamtime); below |
| `change_church_aspect` (`0x1ce0890`) | `0xaaea10`, `0xaaee40`, `0xaae790` | `0xfdc0f0` | Protestant church aspects: `{ index=N aspect="K" country="T" operation=B }`, `index` only when not negative; its check reads the country's church at `+0xf68` |
| `change_blessing` (`0x1ce0938`) | `0xaaf0a0`, `0xaaf3d0`, `0xaaf000` | `0xe17020` | The blessings and aspects of `common/church_aspects` other than the Protestants': `{ aspect="K" country="T" }` |
| `toggle_fervor_effect` (`0x1ce0b30`) | `0xaadc90`, `0x9bfa40`, `0xaadc30` | `0xfe2f00` | Reformed fervor: `{ country="T" effect=N }`, `N` the effect's number (its `+0x60`) in the database `0x735500` answers, by the command's `+0x58` |
| `reform_religion_command` (`0x1c54d48`) | `0x2a4d40`, `0x2af4b0`, `0x2ae680` | `0x139fae0` | Religious reforms (`common/religious_reforms`); the writer is `reform_native`'s too |
| `harmonize_with_religion` (`0x1ce05f0`) | `0xab0900`, `0xab0b90`, `0xab0650` | `0xe40530` | Confucian harmonization |
| `use_high_piety`, `use_low_piety` (`0x1c3a1a8`, `0x1c3a250`) | `0xc11a0` both; `0xc1370`, `0xc1250` | `0xff7210`, `0xff7590` | Islamic piety's actions |
| `commission_icon` (`0x1ce0460`) | `0xab1500`, `0xab1790`, `0xab1410` | `0xfd9b30` | Orthodox icons |
| `consecrate_patriarch_command` (`0x1ce0698`) | `0x271330`, `0xab0ee0`, `0xab0da0` | `0x133b0e0` | Orthodox patriarch authority |
| `invite_scholar` (`0x1ce0548`) | `0xab1b30`, `0xab1e20`, `0xab1b10` | `0xfd9fb0` | Islamic schools' scholars |
| `papacy_action_command` (`0x1ce0d28`) | `0xaab770`, `0xaaba00`, `0xaab180` | `0x12c74f0` | The curia's papal actions |
| `invest_papal_influence_command` (`0x1c65fe0`) | `0x477660`, `0x4778d0`, `0x477530` | `0x12cc000` | Papal influence spent on the curia |
| `establish_holy_order` (`0x1c3a2f8`) | `0xc0f40`, `0xc0ff0`, `0xc1000` | (AI's only found: `0x9af40`, `0xc0e20`) | Holy orders in a province |
| `select_teaching`, `deselect_teaching` (`0x1c39530`, `0x1c395d8`) | `0xc15c0`/`0xc1900`, `0xc3cf0`/`0xc3ab0` | (AI's only found) | Teachings |
| `convert_to_religion` (`0x1ce0a88`) | `0xaad630`, `0xaad990`, `0xaaccf0` | `0xfe28d0` | Converting the state religion |
| `join_religion_league` (`0x1ce09e0`) | `0xab0240`, `0xab0530`, `0xaaf4d0` | | The Empire's religious leagues |
| `convert_ruler` (`0x1cbc620`) | `0x86e9f0`, `0x86ecc0`, `0x86e790` | | |

Shinto's isolationism, Tengri's syncretic faith and karma have no command of their own among the 339 command classes (inferred from the scan: no name of theirs); events, decisions and the monthly update change them.

## Personal deities

- **The database**: a singleton at `[0x242bca8]`, 0x28 bytes, made by its getter `0x7e6910` (`0x7e691a` `48 8b 05 87 53 c4 01`): a name table (count `+0`, 511 buckets `+4`, the buckets `+8`, each node the deity at `+0` and the next node at `+8`; looked up by `0x7e6830`, hash `(h + c) × 61`), then the deities in file order, a `std::vector` of pointers at `+0x10` (`0x7e6961` `48 89 7b 10`).
- **A deity** (its reader of one key, `0x7e5c10`): its key a `std::string` at `+0x10` (the command's writer, `0xaae096` `48 83 c3 10`); its modifiers `+0x70`; `effect` `+0x1c8`, `removed_effect` `+0x200`, `ai_will_do` `+0x238`, `sprite` `+0x274`, `potential` `+0x278` and `trigger` `+0x2f0`, content triggers as a decision's ([triggers.md](triggers.md)). **A trigger's reader is its part at `+0x38`**, which the deity's reader is handed: `potential` is read at `+0x2b0` (`0x7e5cd0` `48 81 c1 b0 02 00 00`) and `trigger` at `+0x328` (`0x7e5c39` `48 81 c1 28 03 00 00`), while the constructor `0x7e58a0` makes both blocks (`0xb89710`) at `+0x278` and `+0x2f0`, their conditions a small vector at the block's `+8` with its count at `+0x14`, as the verdict `0xb98200` reads them. The policies' reader shifts the same way (`potential` read at `+0x1b8`, the block at `+0x180`). Read at the reader's offsets, every `potential` held in the game (the run).
- **No deity**: the game's `Null` deity, at `[0x233fe90]`, made by `0x7e56c0` (`0x7e5766` `48 89 1d 23 a7 b5 01`), which the lookup `0x7e6830` answers for a key no deity has; it is in the database's vector, first.
- **The country's deity**: country `+0xfa0`, a pointer to the deity, the no-deity while it follows none; the setter `0x3d6bf0(country, deity)` (`0x3d6c12` `48 8b 89 a0 0f 00 00`) runs the old one's `removed_effect` and stores the new one.
- **Whether a religion has them**: `0xa7eda0(religion)`: the religion's byte `+0x117` (`personal_deity = yes`), and the expansion that brings them.
- **The command's check** `0xaae300`: the deity's own `+0x40`, the country's tag valid (`+0x57`), and its state religion (country `+0xf30`) having deities. **It reads neither the deity's `potential` nor its `trigger`.**
- **The view's button** (`0xfddb00`) asks the same and posts; whether the player may follow another deity now is `0x3d6e70(country, text)`, which the view asks for the button (`0xff1874`): false during a regency (the ruler's `+0x78` set), with `CANT_CHANGE_DIETY_REGENCY` in the text, and true otherwise, also while a deity is followed ("last until your monarch changes", `RV_NO_DEITY_DESC`).
- **What the framework offers**, as the view's list does (_inferred_ from the files' `potential`/`trigger` pairs and the decisions' and policies' views, the list itself not read): the deities whose `potential` holds, each allowed when `0x3d6e70` allows it and its `trigger` holds, with the regency's line and `trigger`'s failing lines as the reasons.

## Cults

- **The cults**: a `std::vector` of pointers of the game's own at `0x2328a78`, its end at `0x2328a80` (the getter `0x7faf20`, `0x7faf40` `48 8d 05 31 db b2 01`; the destructor `0x1b5ec60` walks it, `0x1b5ec79` `48 3b 1d 00 9e 7c 00`), in file order; a name table follows (`+0x18` count, `+0x1c` buckets, `+0x20` the buckets).
- **No cult**: an object of the game's at `0x245b190`, made on first use by `0x7fae20` (`0x7fae40` `48 8d 05 49 03 c6 01`), which a country that has cults follows while it follows none; its key is `nocult`, and the cults' vector holds it among them (seen in the game).
- **A cult** (`0x7fad10`): its key `+0x10` (`0xaae5a6` `48 83 c3 10`), `ai_will_do` `+0x78`, its modifiers `+0xb0`, `allow` `+0x200` (read at `+0x238`, `0x7fad78` `48 81 c1 38 02 00 00`, as a deity's triggers are), `effect` `+0x278`, `removed_effect` `+0x2b0`, `sprite` `+0x74`, `default` `+0x2e8`. Its `allow` holds the religion, the technology group or `has_unlocked_cult`, and the primary cult's rule (`common/fetishist_cults`).
- **The country's cult**: country `+0xfa8`, the cult or no cult; the setter `0x3d6f00(country, cult)`.
- **Whether a religion has them**: `0xa7ee00(religion)`: the religion's byte `+0x118` (`fetishist_cult = yes`), and the expansion.
- **The command's check** `0xaae6a0`: the cult not no cult, the tag valid, the state religion having cults. It does not read `allow`.
- **Whether the player may embrace one now**: `0x3d7850(country, text)`: false during a regency, with `CANT_CHANGE_CULT_REGENCY`; otherwise true only while the country follows no cult (a cult "last[s] until your monarch changes", `RV_NO_CULT_DESC`), false without a line while it follows one, and false with a null pointer. The framework gives that last refusal its own line.
- **What the framework offers**: every cult, while the religion has them, each allowed when `0x3d7850` allows it and its `allow` holds (_inferred_ as for deities).

## Church aspects and blessings

Read on 2026-11-11 from the code and the running game's memory (the 1700 save, as Sweden and Ethiopia); what the framework does with it and what the game showed is in [the run](../prototype-log/2026-11-11-religion-aspects-blessings.md).

- **The aspects' database** (`common/church_aspects`): a singleton at `[0x242bcb8]`, 0x60 bytes, made by its getter `0x7f6ef0` with a name table as the deities' (`+0` count, `+4` 511 buckets, `+8` the buckets), then three `std::vector`s of pointers: every aspect with the game's no-aspect first (`+0x10`; `noaspect`, the object at `[0x2458be0]`), the aspects a church chooses among (`+0x28`: the Protestants', the Hussites', the Jewish), and the blessings (`+0x40`, no-aspect first: `is_blessing = yes`, the Copts' and the Zoroastrians'). An aspect's key is a `std::string` at `+0x10`; its `cost`, i32 thousandths of church power, at `+0x358` (`0x7f8fe2` `8b 87 58 03 00 00`; 100000 for every Protestant aspect, the files' `cost = 100`).
- **A religion's lists**: the aspects its church offers at religion `+0x730` (what `0x7f8270(church)` answers, `0x7f82a5` `48 05 30 07 00 00`; Protestantism's 16, `anglican_aspect` among them), and its blessings at `+0x760` (`0x37adeb`; the Copts' five). Every other religion's are empty in the 1700 save.
- **A country's church**: country `+0xf68` (`0xaae7c7` `48 8b b8 68 0f 00 00`), an object with a vtable whose `+0x40` says it is valid: its church power, i32 thousandths, at `+0x10` (Sweden's 200000), its country at `+0x18`, and its slots, a `std::vector` of pointers at `+0x20`, three in the 1700 save, the no-aspect in a free one.
- **A country's blessings**: country `+0xf70`, a `std::vector` of pointers to the blessings taken (`0x37aef6`); Ethiopia's two.
- **`change_church_aspect`**: `{ index=N aspect="K" country="T" operation=B }`, `index` written only when not negative. The view's button (`0xfdc0f0`) keeps the aspect (`+0x1c0`), a slot (`+0x1c8`) and the operation (`+0x1cc`), asks the same as the command's check and posts. The check (`0xaaee40`) wants the aspect valid, the country's state religion one with church power (`0xa7e6b0`, its byte `+0x116`), the church valid, the slot below the slots' count (so `-1`, no slot, passes), and by operation:
  - **0, take into a free slot**: `0x7f8e70(church, aspect, text)`, which writes the view's lines into the `std::string`: a free slot (`CHANGE_ASPECT_TOO_MANY`) and the church power (`0x7f8fe2`: the aspect's cost against `+0x10`, with the power's name from the religion, `0xa7e910`/`0xa7e9f0`). The execute (`0xaae93f`) takes the slot given if it is free, and otherwise finds a free one (`0x7f81f0`), sets it (`0x7f7f30`) and takes the cost from the church power (`0x7f7ce0`).
  - **1, give up a chosen aspect**: the aspect among the slots, and `0x7f9610(church, text)`, whether the aspects may change now: stability at least a define (`[0x233e664]`) less 3, with `CHANGE_ASPECT_TOO_LOW_STAB`. The execute finds the aspect's slot itself and frees it.
  - **2, replace the aspect in a slot**: `0x7f8ba0(church, aspect)` (the power, the aspect not chosen, the religion's list); the execute also takes stability by the define (`0xaae816`).
- **`change_blessing`**: `{ aspect="K" country="T" }`. Its check (`0xaaf3d0`), after the multiplayer guard, is `0x37adb0(country, aspect, text)`: the aspect valid, `0x37aea0(country, text)` (the country's religion keeping its holy sites, with `LOST_HOLY_SITES` and how many of the blessings last taken lapse), and the aspect in the religion's blessings.

## What is known of the view's other buttons

- **Fervor** (the Reformed): the effects' database at `[0x242bc20]` (getter `0x735500`), its effects a `std::vector` of pointers at `+0x10`; `toggle_fervor_effect` writes the effect's number from its vtable's `+0x60` (`0xaadd50`). The command's check (`0x9bfa40`) is the multiplayer guard alone; what the player may do is the view's button's (`0xfe2f00`), which was not followed.
- **Religious reforms** (`common/religious_reforms`): the reforms are bought with the spending kind `buy_religious_reform` (31, [power-spending.md](power-spending.md)); `reform_religion_command` `{ country="T" }` reforms the religion once all are bought. Its check (`0x2af4b0`): the country not `+0x28a7`, its religion's byte `+0x58b` (it has reforms), `0x9759c0`, every one of the country's reform tiers at `+0x1450` (0x20 bytes each, a vector of i32 at `+8`) taken, and `0x3cd850`.
- **Harmonization** (Confucianism): `harmonize_with_religion` `{ country="T" religion=N }`, `N` a religion's number in the religions' vector (`[0x242bd98]` through `0x99b60`, `+0x28`). The check (`0xab0b90`) wants the religion harmonizing (`0xa7eed0`) and the religion, or its group when it is of the player's religion's group (`+0xd8`), among the candidates `0x3ec460(country, list, 0, 0)` writes (0x18 bytes each: a religion and a group). The reasons the view shows (`HARMONIZE_NOT_ENOUGH_DEVELOPMENT` and the rest) are written by a widget's tooltip (`0xe3d060`), not a function of the country.
- **Icons**: `commission_icon` `{ country="T" icon=N }`; **the patriarch**: `consecrate_patriarch_command` `{ country="T" province=N }`; **scholars**: `invite_scholar` `{ country="T" school=… }`; **piety**: `use_high_piety` and `use_low_piety` write a `members` block; the curia and papal influence write an `id` or an `index`. Their lists and checks were not read.

## Not known

- How the view's lists of deities and cults are made: whether they leave out what does not hold, or show it disabled; and whether the church's list leaves out an aspect whose `potential` fails (Protestantism's list holds `anglican_aspect`, which the framework lists as the religion does).
- The view's other buttons beyond the above: fervor's choices and costs, the religious reforms' list, harmonization's reasons, icons, the patriarch, scholars, piety, the curia, holy orders, teachings, converting.
