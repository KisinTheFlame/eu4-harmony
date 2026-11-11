# Institutions, ages and monuments

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with a Capstone script over the PE file, the command vtables named by the token table ([commands.md](commands.md)), and `cargo xtask disasm`. What the framework builds on it is part M20 of [mod-capabilities.md](../plans/mod-capabilities.md): the `institution`, `age` and `monument` groups. What was tried in the running game is in [the run](../prototype-log/2026-11-11-institutions-ages-monuments.md).

## Institutions

- **The database**: its getter `0x855ad0`; a vector of pointers at `+0x10`, each institution's index an i32 at `+0x10` and its key a `std::string` at `+0x18` ([provinces.md](provinces.md#prosperity-devastation-manpower-supply-separatism-and-institutions-for-m6-of-mod-capabilities)). Whether one has appeared is the world's vector at `+0x22a0`, by index.
- **Whether a country has embraced one**: a vector (begin, end) of i32 at country `+0xc68`, 1 by the institution's index once embraced; the country's save writer writes it as `institutions` (`0x2dc992`).
- **The check** `0x3f6340(country, institution, reasons, with_cost) -> bool`, which the institutions view (`0x10167aa`) and the command's check (`0x2bc481`) call, the view with a `std::string` for its reasons and `with_cost` set, the check with none:
  1. the institution has appeared, or `INSTI_NOT_ACTIVE`;
  2. the share of the country's development that has it (`0x3f5510(country, out, institution, null)`, i32 thousandths into `out`) is at least 100 (10%), or `INSTI_NOT_ENOUGH_HOLD` with the share and what is needed;
  3. with `with_cost`, the cost (`0x3f6b50(country, out, institution, null)`, i32 thousandths of a ducat) no more than the treasury (`+0xbb0`), or `INSTI_TOO_EXPENSIVE`.
  It does not ask whether the country has embraced it already; the view does not offer it then.
- **The command** `embrace_institution_command` (vtable `0x1c53528`; write `0x2bc160`, read `0x2bc250`, check `0x2bc450`, execute `0x2bb3a0`): `country` is the tag's index (`%d` of the tag's bits 32–47, read back through the country manager), `institutions` the institution's index, `message` a bool the view's button sets (`0x10193e1`). As the button writes it: `embrace_institution_command={ country=<index> institutions=<index> message=yes }`. The execute pays the cost as a single-time expense `embrace_institution` and sets the flag.

## Ages

- **The ages' database**: its getter `0x8db70` makes it on first use (0x78 bytes) and keeps it at `[0x242b9e0]`; three maps by key with a vector of pointers each: the ages (`+0x10`), the objectives (`+0x38`) and the abilities of every age (`+0x60`), in order of their index.
- **The age the world is in**: a pointer at world `+0x2420`, its key a `std::string` at `+0x10`; the world's save writer writes it as `current_age` (`0x750a43`).
- **An ability**, made by the age's reader (`0x8c65a`): its index at `+0x10` (the count of the database's vector when made), its age at `+0x18`, its `allow` triggers an array at `+0x28` (count `+0x34`), its key a `std::string` at `+0x98`.
- **A country's abilities taken**: a vector of pointers at country `+0x30`, written as `active_age_ability`; its splendor an i32 of thousandths at `+0x48`.
- **The check** `0x3f8d30(country, ability, reasons) -> bool`, the command's check calling it with no reasons: no, without a reason, for an ability taken; each `allow` trigger, whose failing lines it writes; the splendor at least the define at `[0x233e4e4]` (thousandths), or `ADD_AGE_AB_NOT_ENOUGH_SPLENDOR`. It does not ask whether the ability is of the age the world is in.
- **The command** `age_ability_command` (vtable `0x1c535d0`; write `0x2bd9f0`, read `0x2bdae0`, check `0x2bdc60`, execute `0x2bd700`): `country` the tag as a string, `which` the ability's index into the database's vector: `age_ability_command={ country="FRA" which=<index> }`. The ages view makes it at `0xde9a57`.
- `enact_golden_age` (vtable `0x1c531e0`) is the golden era's button; not offered.

## Monuments

- **The great projects' manager** at world `+0x25b0`: a map by the definition's token (`+0x18` end, `+0x28` buckets, `+0x40` mask; `0x7d3060`) to an index into an array of the game's own, its data at `+0x58` and its length (i32) at `+0x64`, each project 0x5e8 bytes (`0x92d949`, which adds the projects of a country's provinces to its modifiers).
- **A project** (vtable `0x1cb4428`, its save writer at `+0x10`, `0x7d0a50`): its definition at `+0x10`, a pointer whose key is a `std::string` at `+0x10` and whose kind, u8 at `+0x1d36`, is 1 for a canal and 2 for a monument; its province, i32 at `+0x5e0`, 0 for one the world does not have (`+0x58`, `0x7d08c0`); its tier, u8 at `+0x5e4`, from 0 to 3; when it was built and destroyed at `+0x5d8` and `+0x5dc`.
- **The upgrade check**, its vtable's `+0x60`, `0x7d1560(project, country, flag, reasons) -> bool`: no for one the world does not have; at tier 3, `CANNOT_UPGRADE_GREAT_PROJECT_ALREADY_AT_MAX`; then the definition's check for the next tier (`0x7d5f30`), which writes its requirements' failing lines (`MACRO_REQUIREMENT_NOT_FULFILLED`) and the cost against the treasury (`BUILD_BUILDING_COST_TOO_HIGH`). The command's check calls it with no flag and no reasons, after the builder owns a province.
- **The cost**, its vtable's `+0x70`, `0x7d17a0(project, out, country, explanation or null)`: i32 thousandths of a ducat into `out`, through the definition's `0x7d7430` for the next tier.
- **The command** `upgrade_great_project` (vtable `0x1c4e148`; write `0x267df0` through `0x26a7a0`, read `0x26b420`, check `0x26b690`, execute `0x267e20`), as the monuments window's button makes it (`0x139777c`): `great_projects` the definition's key, `province` the province's ID, `builder` the country's tag, written when its last byte is set: `upgrade_great_project={ great_projects="<key>" province=<id> builder="FRA" }`. `constructgreatproject` (vtable `0x1c4e1f0`) shares its writer and reader; `cancel_great_project`, `boost_great_project` and `move_great_project` are not offered.
