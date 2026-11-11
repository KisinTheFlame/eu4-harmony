# The religion view: its commands, personal deities and cults (for `religion`)

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with scripts over the PE file (the `.rdata` scan for command vtables of [commands.md](commands.md), the token table at `0x1fd8660`, capstone) and the game's own files (`common/personal_deities`, `common/fetishist_cults`, `localisation/religion_l_english.yml`); what was tried in the game is in [the run](../prototype-log/2026-11-11-religion-deities-cults.md). Part M18 of [mod-capabilities.md](../plans/mod-capabilities.md) is built on it: the `religion` group follows personal deities and cults; the other commands below are found and not yet offered.

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

## Not known

- How the view's lists of deities and cults are made: whether they leave out what does not hold, or show it disabled.
- The other commands' written forms beyond the writers' first reading above, their checks, and what the view's buttons check before they post: church aspects (church power, the slots), blessings, fervor (its cost, the database `0x735500`), religious reforms, harmonization, piety, icons, the patriarch, scholars, the curia, holy orders, teachings, converting.
