# A province's snapshot (for `game::provinces`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-10-29; what was checked in the running game is marked "tested" (the run is in [prototype-log.md](../prototype-log.md), remote control C3). The province object (`0x2e10` bytes, by ID from the world's `+0x1ca8`) and its map data (`+0x28`) are as `objects::campaign` names them; the map data's neighbors are in [regional-combat/native-geography-findings.md](regional-combat/native-geography-findings.md).

## Where the keys come from

The province writer in saves, `0x9d6e60`(province, writer), writes each field under a binary token: `call 0x1709280`(writer, token) with the token in `edx`. The token table in `.data` holds each token's number (u32) followed by its key, NUL-ended (`0x20ad2c0`: `0x2a03`, `has_building`; `0x21337d0`: `0x3097`, `has_terrain`); reading it gives `0x1b` name, `0x2839` base_tax, `0x31b2` base_production, `0x31bc` base_manpower, `0x36ed` buildings, `0x3b9f` mothball_status. The same table names the commands' types (`0x2921` buildunit, `0x2801` constructbuilding) and a country's unit kinds (`0x2781` infantry, `0x2780` cavalry, `0x2782` artillery, `0x2784` heavy_ship, `0x2785` light_ship, `0x2787` galley, `0x2786` transport).

## Name

- **The name is a `std::string` at `+0x10` of the map data** (`province+0x28`). The writer: `0x9d6fb0` `49 8b 5f 28 48 83 c3 10` (`mov rbx, [r15+0x28]; add rbx, 0x10`), then token `0x1b`. It is set by `0xa40700`(map data) from the localisation of `PROV%d` with the map data's ID (`+0x150`), through the lookup `0x14b34a0`, and assigned over the old text (`0xa407ac`). So it is the localised name, or what the province was renamed to, in game text: with the Chinese language mod, 君士坦丁堡 (tested).
- The Chinese language mod writes `PROV1418` (Houtman Abrolhos) as 霍特曼·阿布洛霍斯群岛: its `·` is the Windows-1252 byte `0xb7` in game text, the rest UTF-8. Reading the name with `unicode::game_text::to_utf8` gives the characters (unit test `a_mixed_name_reads_as_its_characters`; tested).

## Development

- **Base tax `+0x3e4`, production `+0x3e8`, manpower `+0x3e0`**, i32 in thousandths: the writer reads them at `0x9d7a5b`, `0x9d7ac5` and `0x9d7afa` before the tokens `base_tax`, `base_production` and `base_manpower`.
- **The game's own getters add more**: the scope values registered by `0x6a87d0` ("base_tax", "base_production", "base_manpower", "development", ...) are `std::function`s whose calls are `0x6de7a0` (tax), `0x6de830` (production) and `0x6de8c0` (manpower), each (unused, out, province): the stored value plus, when the owner's `+0x9a78` (its `native` object, token `native` in the country writer at `0x2deed6`) exists and the owner's capital (`+0x14b8`) is this province, that object's `+0x1c`/`+0x20`/`+0x24` rounded down to whole points: a native country's tribal development in its capital. The province view's getter `0xa0d6b0` (tax, with its explanation) adds the same. The framework calls the three getters. Tested: Toledo 9/7/5, Constantinople 12/11/9 and Seville 6/6/4, as the province view shows them.

## Terrain

- **The terrain is a pointer at `+0x30` of the map data**: the has_terrain trigger (vtable `0x1cf2670`, evaluated by `0xcd8e40`) compares it with its own at `0xcd915e` (`mov rdx, [rdx+rcx+0x28]; mov rax, [rdi+0x68]; cmp [rdx+0x30], rax`). `0xa42e20`(map data, terrain, ...) sets it, logging "SetMostCommonTerrain". Tested: the keys read (grasslands, farmlands, drylands, mountain, ocean, inland_ocean) are what the map's tooltip names (草原, 农田).
- **The map's terrains** are a vector of pointers at `+0x758`/`+0x760` of the map (`[0x233fed8]`); `0x88a0f0`(map, key) finds one by comparing its key, a `std::string` at `+0x118` of the terrain (`0x88a139`), with the trigger's.

## Fort

- **Fort level `+0x2de0`, i32; while mothballed, `+0x2ddc`.** The mothball state is a byte at `+0x8f4` (token `mothball_status`, written when it is not 1; 2 is mothballed). The fort's defense `0xa0e860` and the siege and occupation naming `0x23a940` both take `+0x2ddc` when the state is 2 and `+0x2de0` otherwise (`0xa0e890`); the province view shows "HAS_CAPITAL_FORT" when `+0x2ddc` is above 0, so it is what a capital keeps. "OCCUPATION_OF" is used where the level is 0. Tested: Toledo, Castile's capital, 3 (its tooltip: level 3, a free capital fort and an active one); Constantinople 3; Seville 0. A mothballed fort was not at hand.

## Buildings

- **`+0x3b0`/`+0x3b8` is a vector of 16-byte slots, one per kind of building**: the building and the tag of who built it (token `0x3763` in the writer). The slot at index *i* holds building *i*, or the database's placeholder (index 0) where there is none. Whether a province has a building (`0x26a3b0`, the destroy-building command's check): its index (`+0x14` of the building) is below the slot count and the slot at that index holds a building whose index is not 0 (`0x26a3df`). The writer (`0x9d82b3`) walks every slot and writes the token at `+0x10` of each building whose index is not 0.
- **A building's key** is a `std::string` at `+0x28`: the database (`0x1e5390`, a hash of keys) finds a building by it at `0x1e5ea6`, and answers the placeholder `0x1e52c0` ("nobuilding") when there is none. Forts are buildings too (`fort_15th` to `fort_18th`). Tested: Toledo `fort_15th`, Constantinople `temple` and `fort_15th` (shown as a mosque and the fort), Seville none, as the building panels show.

## What can be recruited there

- **A country's current unit types** are 7 pointers at `+0xd48`, by kind in the token order above: infantry, cavalry, artillery, heavy_ship, light_ship, galley, transport (the kinds' tokens: `0x2c7341` fills a table from `0x1dc2380`; the country writer `0x2e0147` walks the 7 under `sub_unit`, writing each one's key, `0x267b40` `lea rax, [rcx+0x60]`).
- **A unit type** has the class `0x1ce3830` (its constructor `0xad8c40`), its kind at `+0x10` (i32, 0 to 6) and its key at `+0x60` (`std::string`). Where a country has none of a kind it holds the placeholder `---` made by `0xadbec0`, of class `0x1ce38a0`, whose validity method (`+0x68`) answers false where a unit type's answers true.
- **The buildunit command** (vtable `0x1c4dff8`, token `0x2921`; made by `0x26ce40`/`0x26cf20`) holds the country `+0x50`, the unit type `+0x58`, the province ID `+0x60`, a special kind `+0x78` (0 regular, 1 when a mercenary company `+0x68` is set; specials, such as janissaries, from 2) and a template `+0x80`. Its check (`0x26e080`, vtable `+0x40`) asks:
  - the cost `0x348330` against the treasury (`+0xbb0` of the country);
  - for a ship, not `0x40d2b0`(country) unless `0xcfc70`(`0x233ff48`) (a rule of the game's, not worked out);
  - **`0x346570`(country, province, unit type, special kind, any province, check resources, reasons, ship, unused, flags)**, which returns whether it can be built and, when `reasons` is a `std::string`, appends each reason in the game's language, a line each: not a core (`MACRO_NOT_CORE`), not the controller or the owner, under siege, in combat, too much local autonomy, no manpower, no sailors, no money, army tradition, legitimacy, prestige, absolutism, special regiments, the rules, no ports. It reads the province's discovery and, for a ship, whether the owned province has a port (`+0x550`), without a reason. "Any province" (fifth) skips the core and ownership checks; "check resources" (sixth) adds manpower, sailors and money;
  - then `0xa1fe50`(province, unit type), its answer: a regiment anywhere, a ship only in an owned province with a port, and the same rule as above for the owner.
- Its execute (`0x26cfe0`) queues the construction (`CMilitaryConstruction`, finished by `0x251df0`, "Building Regular Unit, %s").
- The framework asks `0x346570` with special kind 0, any province 0, check resources 1 and a string for the reasons, then `0xa1fe50`, for each kind the player has a unit type of. It does not mirror the command's ship rule (`0x40d2b0`). Tested, as Castile with no manpower: regiments refused everywhere with "没有足够的£icon_manpower£。" (the recruit panel's tooltip says it needs 1000 men); artillery has no unit type yet (the panel lists none); in Seville and Gibraltar the four ships allowed (the shipbuilding panel offers them), in Toledo and Madrid refused without a reason; elsewhere "Castile is not the controller", and "not a core" for a sea zone.

## What the framework relies on

The fields are in `objects::campaign` (`province`, `geometry::NAME` and `TERRAIN_TYPE`, `map::TERRAINS`, `terrain`, `building`, `unit_type`, `country::UNIT_TYPES`, `classes::UNIT_TYPE`), each with a dependency point keeping an instruction of the game's that uses it; the functions it calls are points in `game::provinces`.
