# Government: ideas, policies, reforms, rank, estates, parliament, decisions, missions

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-05 with `cargo xtask disasm` and scripts over the PE file (the command vtables, the token table), and from the running game's memory read from outside (a 1592 save, the Netherlands); the commands' written forms were then seen in a developer build's command log as the AI posted them. What was tried in the game through the capabilities is in [prototype-log.md](../prototype-log.md). What is marked _inferred_ was concluded from the shape of the code. Where the conditions are kept and how the game writes them is [triggers.md](triggers.md); the country's fields are [countries.md](countries.md).

## The commands

Found by scanning `.rdata` for vtables with the command layout ([commands.md](commands.md)) and naming each by the token its `+0x50` answers; each command's `+0x10` writes it, `+0x40` is its check and `+0x48` its execute. The forms below are those the AI's own commands were logged with (the stamps the session adds left out):

| What the player does | Command (vtable) | Written form |
|---|---|---|
| Take an idea group | `add_idea_group` (`0x1c556b8`) | `add_idea_group={ country="JMB" idea_group="defensive_ideas" }` |
| Abandon an idea group | `abandon_idea_group` (`0x1c55610`) | the same keys |
| Buy an idea | `spend_power_command` (`0x1c554c0`) | `spend_power_command={ country="PAL" power={ data=89 category=0 skill=2 } }`: kind 0 (`buy_idea`), `data` the idea's index in the idea database's ideas, left out when 0, `skill` the group's category |
| Enable, disable a policy | `enable_unlockable` (`0x1c66ac8`), `disable_unlockable` (`0x1c66b70`) | `enable_unlockable={ policy="global_trade_networks_act" country="JOL" }` |
| Enact a reform in the next tier | `select_government_reform` (`0x1c38a08`) | `select_government_reform={ members={ dummy="MGD" dummy="the_social_contract_reform" } }` |
| Change a reached tier's reform | `change_to_government_reform` (`0x1c38ab0`) | `members={ dummy=<tag> dummy=<reform> dummy=<int> dummy=<yes/no> }`, from its writer `0xc6e40`; not seen in the log |
| Raise the rank | `government_rank_command` (`0x1c53918`) | `government_rank_command={ country=<tag> value=<rank> prestige=yes }`, from its writer `0x2bace0` and the button's post (`0xf383ed`: the rank plus 1, prestige set) |
| Grant, revoke a privilege | `grant_privilege` (`0x1c38d50`), `revoke_privilege` (`0x1c38df8`) | `grant_privilege={ members={ dummy="VIJ" dummy=3 dummy=45 } }`: the estate's index in the estates' database, the privilege's in the privileges' |
| An interaction over crown land | `crown_land_interaction` (`0x1c38ea0`) | `crown_land_interaction={ members={ dummy="MAJ" dummy=2 } }`: the interaction's index |
| Ask an estate for an agenda | `create_new_estate_agenda` (`0x1c38f48`) | `members={ dummy=<tag> dummy=<string> dummy=<string> }` (writer `0xc72d0`); not used |
| Start a debate | `start_debate_command` (`0x1c56628`) | `start_debate_command={ country="GBR" which="restore_integration" }` |
| Give a province a seat, take it away | `add_to_parliament_command` (`0x1c55ca0`), `remove_from_parliament_command` (`0x1c55e98`) | `add_to_parliament_command={ country="C07" province=942 }` |
| Bribe in parliament | `bribe` | `bribe={ country="C07" province=913 }`; not used |
| Take a decision | `execute_decision_command` (`0x1c66c18`) | `execute_decision_command={ decision="enforce_religion_muslim" scope={ country="BNE" scope_is_valid=yes seed=343576198 random=343576198 } }` |
| Complete a mission | `complete_mission` (`0x1c3a100`) | `complete_mission={ members={ dummy="ASH" dummy="west_africa_embracing_old_traditions" } }` |

- **`members`** is how the game writes a command whose data is a tuple (`0xc15c0` and its kind): a wrapper object (vtable `0x1c384d0` and others, one per tuple type) over the command's `+0x50`, whose `+0x10` writes each element under the token `dummy` (`0xe6`), in the tuple's order: a tag as a string (`0x1707d90`), a `std::string`, an i32 (`0xcbff0`, `%d`), a bool (`0x42c5e0`). The elements lie in memory last first (`0xc8440`: the tag at `+8`, then `+4`, then `+0`).
- **Other commands** of the government's, not used: `government_interaction_command={ country="PER" parent=feudal_theocracy_mechanic index=invite_minorities }` (a government mechanic's interaction), `reset_debate_command`, `set_native_policy`, `select_trading_policy`, `enact_decree`, `ignore_decision_command`.

## Ideas

- **The database** (`[0x242bce0]`, made by its getter `0x850750`): groups at `+0x20`–`+0x28`, ideas at `+0x38`–`+0x40`, `std::vector`s of pointers.
- **An idea group** (vtable `0x1cbaad8`, constructor `0x84e4f0`, read by `0x850fb0`): `+0x10` its key; `+0x30` its ideas, a `std::vector` of pointers in order (`0x332723`); `+0x48` `trigger`, the conditions for taking it (`0x851489`); `+0xc0` `can_buy_idea` (`0x851580`); `+0x568` `free`, set for national ideas; `+0x570` its category, 0 `ADM`, 1 `DIP`, 2 `MIL` (`0x851cdf`); `+0x5b0` `important`; `+0x5b4` its index. Its vtable's `+0x10` (`0x84f1c0`, `(group, tag) -> bool`) evaluates `trigger`; `0x84f750` evaluates `can_buy_idea` the same way. An idea's key is at its `+0x10` and its group at `+0x198`.
- **The country's**: `+0x1438` the groups taken (countries.md); `+0x10a0` a byte for each group by its index, set when taken (`0x2a7eb6`). The next idea of a group is its ideas at the count bought (`0x3326e0` says a country has an idea when it is among the first count of its group's).
- **Taking a group** (`add_idea_group`'s check `0x2a7e70`): the group is not free, its `trigger` holds, it is not taken, and a slot is free (`0x32e2d0`). The interface's tooltip (`0x139d080`, `IDEA_GROUP_CANNOT_TAKE`) writes `can_buy_idea`'s lines.
- **Buying an idea** (the spend command's check `0x94f250(spending, tag)`, kind 0): enough monarch points of the spending's skill (`+0x260` + skill × 4) for the cost (`0x9526d0`), the idea not had, the group not free, `can_buy_idea` (`0x84f750`). The framework builds a spending in its own memory as `0x94f3b8` does on its stack (vtable `0x1c55480`, `+8` `0x165`, kind, skill, data, second −1, `+0x28` from `[0x233fea8]`) and asks its cost and check.

## Policies

- **The check** `0x958e10(country + 0x140, policy, reasons)` (triggers.md) writes into a `std::string` the conditions of `allow`, all of them with their marks, and its own lines (`ALREADY_HAS_POLICY`, `MAX_POLICIES_ALREADY`); the framework keeps the lines that do not hold (`conditions::refusal`).
- **The category** at policy `+0x178` is what the policies view's walk compares (`0x959882`); the active policies are three arrays at country `+0x150`, `+0x180`, `+0x1b0` (countries.md), whose items keep the policy at `+0x10`.

## Reforms

- **The government type's tiers** (`0x798030`, which finds a reform's tier by its key): a game array at type `+0x60` (data `+0x68`, count `+0x74`) of tiers, each with its key at `+0x10` and its reforms, a game array at `+0x30` (data `+0x38`, count `+0x44`). The 1592 Netherlands' monarchy has 11 tiers, from `feudalism_vs_autocracy` to `separation_of_power`.
- **Enacting in the next tier** (`select_government_reform`'s check `0x40a9d0(country, key)`): the reform found by key (`0x70f9b0`, which finds by key in the reforms' registry and the other databases alike), not a mechanic (`+0x58`), its `potential` (`+0x1b0`) and `trigger` (`+0x228`) hold, it belongs to the tier after those reached, and the reform progress (country `+0x9a68`, thousandths) is at least the tier's cost, `0x79ead0(government, out, tier)`.
- **Changing a reached tier's reform** (`change_to_government_reform`'s check `0x40ade0(country, key, free)`): the same conditions, the reform's tier among those reached, and, unless `free`, the reform progress at least the define at `0x233e4cc`. The government view posts it from two places (`0x105cec0` and after), with `free` no and yes. Its execute (`0x2c0800`) hands the i32 member to `0x7afa60(stack, place, reform, tag, …)`, which puts the reform at that place of the reform stack: the place of the reform it replaces.
- **The reform progress** is written as `government_reform_progress` (`0x3820`, `0x2e10e2`).

## Rank

- **The check** `0x3167b0(country, reasons) -> bool` writes the game's reasons into a `std::string`: `CANNOT_UPGRADE_RANK_JUNIOR_PARTNER`, `_SUBJECT`, `_HRE_ELECTOR`, `_HRE`, `_NEED_DEVELOPMENT`, `_NEED_PRESTIGE`, `_MAX_RANK`, `_FIXED_RANK`. It answers false with no reason where the next rank is not one the country may have (`0x317620`).
- **The command's check** (`0x2bafd0`) only asks that the rank be between 1 and the highest (`[0x233e35c]`); its execute (`0x2bac00`) sets the rank (`0x317110`) and, with `prestige`, gives the prestige the defines `0x233e4fc`, `0x233e4f8`, `0x233e4f4` name for ranks 2, 3 and above. Raising the rank costs nothing else.

## Estates

- **An estate** (the database's entries, vtable `0x1cad508`, read by `0x7233f0`): `+0x10` its key; `+0x520` its privileges, a game array of pointers into the privileges' database (data `+0x528`, count `+0x534`), read from its `privileges` (`0x72378c`); `+0x538` its agendas, the same from `agendas`.
- **A privilege** (the database's 0x888-byte entries): `+0x18` its key, `+0x38` its icon's name, `+0x498` `can_select`, `+0x588` `is_valid` (triggers.md). The estates view lists the privileges whose `is_valid` holds, and those granted.
- **Granting** (`grant_privilege`'s check `0x2c16e0`): the estate by its index (`0x716c30`), the privilege by its index, then `0x71edf0(entry, &tag, privilege, reasons)`, which writes `ESTATE_PRIVILEGE_ALREADY_GRANTED`, `CANNOT_AFFORD_ESTATE_PRIVILEGE`, crown land and `can_select`. **Revoking** (`0x2c1c40`): `0x71f430(entry, tag, privilege, flag, reasons)`, the tag by value, the flag 0 as the command's check passes it.
- **Influence** is `0x71a520(entry, out, modifiers)`, thousandths, with the country's modifiers `0x93d750(country + 0x28c0) + 8`, as the influence tooltip `0x718190` asks it (clamped to 0–100 there).
- **The interactions over crown land** (`common/estate_crown_land`, `sale_of_titles`, `summon_the_diet`, `seize_land`): an object answered by `0x728430()` whose game array at `+0x58` holds 0x128-byte interactions (`0xc50ac`), each with its key, an index at `+0x10`, its `trigger` at `+0x38`; the check `0x4128b0(country, interaction, reasons)` writes the cooldown (`CROWN_INTERACTIONS_COOLDOWN`) and `trigger`'s lines.

## Parliament

- **The issues' database** is an object at the fixed place `0x2329390` (its getter `0x9688e0`): a game array of 0xe8-byte issues (data `+8`, count `+0x14`; 108 in the unmodded game), each with its key at `+0x10` and a pointer to its `allow`, an `and` block (vtable `0x1cc08d8`), at `+0x48`; a map by key at `+0x18` (`0x9694d0`, FNV-1a).
- **The parliament** (country `+0x24c8`): the issues on offer, a game array of i32 indices into the database at `+0x20` (11 for Great Britain in 1592), the debate at `+0x50`, the issue enacted at `+0x58`.
- **Debating** (`start_debate_command`'s check `0x2a4c00`): the country has a parliament and no debate going on; the issue is read by key (`0x2a4a45`).

## Decisions and missions

- **Taking a decision**: the view's button (`0x1063660`) calls `0x488050(database, decision, scope)` (triggers.md). The scope written carries `seed` and `random`.
- **The country's missions** (`country_missions`, `0x3809`, inline at country `+0x99d8`, vtable `0x1c573c8`, writer `0x445540`): a game array of columns at `+0x10` (data `+0x18`, 0x18-byte items, count `+0x24`), each a game array of missions, written as `mission_slot`. The 1592 Netherlands had five columns of two. A mission's missions before it are a game array of `std::string` keys at `+0x120` (`0x8d4e6a`); whether one is completed is `0x1796c0(country, mission)`, and whether it can be completed `0x8d4fc0(mission, tag)` (triggers.md).

## Not established

- Where the interface shows the reasons for enacting a reform, and its text for too little reform progress; the framework says that in its own words.
- An idea group's slots' text in the interface; the framework says that in its own words.
- The debate object's layout; the framework takes its `+0x10` for the issue only when it points at one of the database's.
- The key's place in a crown land interaction (`+0x18` by the reader `0x72835a`, whose object may not be the interaction).
- Estate agendas, parliament bribes, government mechanics' interactions: commands found, not offered.
