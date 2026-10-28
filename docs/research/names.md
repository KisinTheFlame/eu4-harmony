# People's names and battle names (for `wording::names` and `wording::battles`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe and the game's files on 2026-10-28; what was checked in the running game is marked "tested".

## People

- **Rulers, heirs and consorts** share one layout (log strings name it `CMonarch`, monarch.cpp); vtables `0x1cdcd40`/`0x1cdcdd8` (monarch), `0x1c52240`/`0x1cdd0a8` (heir), `0x1c520c0`/`0x1cdd270` (consort-like), `0x1c522f8` (foreign heir).
  - `+0x20` the owner's tag; `+0x28` the given name, a `std::string` **with the regnal number in it** ("Louis XI"; `0x314400` turns `#N` in a name list into the numeral); `+0x58` religion; `+0x60` the person's culture (`CCulture*`); `+0x68` the dynasty, whose name is a `std::string` at `+8`; `+0x74` female; `+0x78` the regency's estate.
  - A country's monarch is at `+0x1280`, heir `+0x1298`, consort through `0x30f660`; its primary culture at `+0x1228`.
- **The full name**: `0xa4b2c0`(person, out) puts the given name in out (`0xa4b2df` `48 8d 51 28`), then, when the dynasty is valid, a one-character `" "` (`0xa4b468` `66 c7 45 b8 20 00`) and the dynasty's name (`0xa4b44c` `48 8b 57 68 48 83 c2 08`), then " (regent)" during a regency. So the order "given name, dynasty" is fixed in code.
  - 55 call sites: the court view (`0xed2be0`; the title goes in front, "国王 …"), the ruler, heir and consort tooltips, the `FULLMONARCHNAME` message value, new-ruler and regency messages, the HRE and Celestial Empire views, the front end, and `0xa4baa0`, which copies a ruler's full name into the general he becomes.
  - `0xa4b230` gives the given name only (118 callers), as does `[Root.Monarch.GetName]` in localisation; event texts that want a full name write `[Root.Monarch.GetName] [Root.Monarch.Dynasty.GetName]` themselves.
- **The lines on what follows the ruler's death** (`MDEATH_HEIR_SUCCEEDS`, `MDEATH_REGENCY_RULE`, in the ruler's tooltip, alerts and the diplomacy tooltip) come from `0x318870`, which builds `HEIR_NAME` in place: `" " + dynasty` into `[rsp+0x68]` (`0x3190ff`–`0x31919c`), the given name from `0xa4b230`, then `call append` at `0x319257` / `0x319324` with the heir in `rbx` (`0x3191fa` `48 8b cb`).
- **Generals and advisors** keep one string (`+0x20`), made when they are created by `0x313d90` / `0x313f40`: a given name from the culture's or country's lists, then a family name. It is saved with them, so it cannot be split again reliably: names in the data already mix orders ("Di Zhu", "Yi Sun-Sin", "Wang Yangming"), and family names can hold spaces ("Nguyen Phuc", "Aisin Gioro").

## Cultures

- **`CCulture`** (constructor `0x27e810`): `+0x48` its key as in common/cultures (`0x27e8a7` `4c 8d 76 48`), `+0x68` its localised name, `+0x88` its group (stored at `0x27e8fe`, read at `0x280952` `48 8b 91 88 00 00 00`).
- **`CCultureGroup`** (constructor `0x27c9f0`): `+0x38` its key (`0x27ca3c` `48 8d 71 38`), `+0x58` its localised name, `+0x78` its index in file order (match on keys, not indices).
- A person's culture link in localisation scopes reads `+0x60` (`0x280aba` `48 8b 41 60`).
- **East Asian, family name first** (common/cultures/00_cultures.txt): the groups `east_asian` (Chinese cultures, and `vietnamese_new`, `korean_new` from the Empire of China), `korean_g`, `japanese_g`, and the culture `vietnamese`. Not family first although in `east_asian`: `tibetan_new`, `altaic_new`, `zhuang_new`, `manchu_new`.

## Battles

- `0x23a900`(combat, out) is the localised key (`BATTLE_OF`, or `SIEGE_OF` for a siege, or `OCCUPATION_OF` where the province has no fort), then `" "` and the province's name: the order is fixed in code. 10 callers: land and naval battle results (`LANDBATTLEOVER`), siege and occupation over, a leader's death in battle, the combat and siege views and their tooltips.
- The war overview's battle row (`0x148a6d0`, through a vtable) builds its own: `BATTLE_OF` + `" §Y"` + the battle's stored name + `"§!"`.
- The Chinese language mod has `BATTLE_OF` "战役", `SIEGE_OF` "围攻", `OCCUPATION_OF` "占领"; the game shows "战役 雅典".
- The localisation lookup is `0x14b34a0`(out, key, values, count), which returns the key when it is missing; `0x95660` destroys a `std::string`.

## What the framework does

- `wording::names` replaces `0xa4b2c0` and the two appends at `0x319257` / `0x319324`. For a person whose culture is East Asian (above), when the player language has the template `EU4HARMONY_NAME_FAMILY_FIRST` (Simplified Chinese: "$FAMILY$ $GIVEN$"), the name becomes family name first, and a space between two Chinese, Japanese or Korean characters is dropped. Generals, advisors and names put together in localisation keep the game's order.
  - Tested 2026-10-28: the lobby shows "皇帝 Zhu Qizhen" for Ming and "国王 Yi Sejong Do" for Korea (the data's given name is "Sejong Do"); Korea's court shows "Yi Sejong Do", "Yi Hyang" and "Sim Soheon"; the ruler's tooltip ends "Yi Hyang继承王位".
- `wording::battles` replaces `0x23a900`: with the player language's templates (`EU4HARMONY_BATTLE`, `_SIEGE`, `_OCCUPATION`, with `$KIND$` and `$PLACE$`; Chinese "雅典战役", "围攻雅典", "占领雅典") the name is reworded; the kind is told by comparing the text with the three keys as localised. Not yet seen in the running game.
