# Advisors: the seats, the pool, hiring, firing and promoting

Baseline 1.37.5. All addresses are RVAs. Read from `eu4.exe` on 2026-11-11 with scripts over the PE file (the command vtables, the token table, the court view's functions) and from the running game's memory read from outside (a new game as the Mamluks, 1444.11.11); what the framework does with it, and what the game showed, is in the last section. What is marked _inferred_ was concluded from the shape of the code. It is what the `advisor` group of [mod-capabilities.md](../plans/mod-capabilities.md) (M8) rests on.

## The advisor object

An advisor (vtable `0x1c427f0`; its advisor type's records are read by `0x1aae50` from `history/advisors` and saves) is a person of the court: a real advisor answers yes at vtable `+0x50` (`0x8f440`); the game's empty advisor (vtable `0x1c48748`, named "no advisor" in the player language) answers no (`0x8f450`), and stands in a seat with no advisor.

| Offset | Field | An instruction that uses it (bytes) |
| --- | --- | --- |
| `+0x10` | ID, u64: type (51, `0x33`) at `+0x10`, number at `+0x14`; written `{ id=<+0x14> type=51 }` | the pool's node holds it, read by `0x14cf730` |
| `+0x20` | name, `std::string`, given name and family name joined ([names.md](names.md)) | |
| `+0x40` | skill, i32, from 1 | `0x1ac51a` `44 8b 43 40` |
| `+0x2a0` | its advisor type | `0x11ad56` `48 8b 86 a0 02 00 00` |
| `+0x2a8` | a date the hire cost's age factor counts from (_inferred_: when it appeared) | `0x1ac523` |
| `+0x2b0` | the date it was hired; a month after it (`0x14c8030`, the date a month on), it may be fired | `0x1aec37` |
| `+0x2c0` | i32 its salary is worked out with (1000 in every advisor read; _inferred_: a discount factor) | `0x1ac4e1` `44 8b 81 c0 02 00 00` |
| `+0x2c8`, `+0x2d0` | its culture and religion, pointers to the objects a province's are ([provinces.md](provinces.md)) | `0x1ac50f`, `0x1ac4f3` |
| `+0x2d8` | byte, set when it may be fired at all (_inferred_: clear for one the country may not let go) | `0x1aebdb` `80 bf d8 02 00 00 00` |

An **advisor type** (vtable `0x1c42bf0`, made by `0x1af830` for each entry of `common/advisortypes`, 0x1a0 bytes) has its key, a `std::string`, at `+0x120` (`0x1af93f` `48 8d 8b 20 01 00 00`), its token at `+0x118`, its index at `+0x160`, its modifiers at `+0x50`, and the kind of monarch power it gives, 0 administrative, 1 diplomatic, 2 military, at `+0xfc` (`0x11ad56` `48 63 90 fc 00 00 00`).

## The seats and the pool

- **The seats**: three pointers at country `+0x14e0`, one for each kind of monarch power by its index, as the AI's spending reads them (`0x11ad2e` `48 8d ba e0 14 00 00`, then each advisor's power to index its own array). A seat with no advisor holds the empty advisor, not null.
- **The pool**, the advisors the court view offers to hire: an array of pointers at world `+0x1c30` by the country's tag index (`0xbec432` `48 8b 82 30 1c 00 00`, the `has_advisor_type_in_pool` trigger), each a pool object whose `+0x10` is the first of a singly linked list of nodes (`0x1b58f0`, the pool's lookup by type): a node holds the advisor, filled in as it is found, at `+0`, its ID at `+8`, and the next node at `+0x18`. `0x14cf730`(ID) finds the advisor by its ID (the framework's `army::FIND_ID`). The Mamluks' pool in 1444.11.11 held twelve advisors, four of each power, none of them seated.

## Costs

- **Salary** `0x1ac650(out, tag by value, skill, power, culture, religion, &discount, text or null)`: the monthly cost in thousandths of a ducat, from the define `ADVISOR_COST` and modifiers (`MODIFIER_ADVISOR_COST`, the same culture's and religion's, the yearly growth), the court view's `COUVIEW_MCOST`.
- **Hiring** `0x1ac4c0(advisor, out, tag by value) -> out`: the salary times a factor of the years since `+0x2a8`, at least once; the hire command pays it (`0x1c582c`, expense `change_advisor`).
- **Promoting** `0x1ad2d0(advisor, out, tag by value) -> out`, the court view's `PROMOTE_COST` (the define `PROMOTE_COST_MONTHS_ADVISOR_SALARY`, _inferred_ from the define's name).

## The court view's checks

- **Hiring** (the hire list's row, `0xecc820`): the button is enabled when the hire cost is below the treasury (country `+0xbb0`, thousandths) and the advisor in the seat of its power, if any, may be fired (`0xecc8dd`–`0xecc955`). There is no check function with reasons: the framework words the treasury itself and gives the firing check's.
- **Firing** `0x1aebb0(advisor, reasons or null) -> bool`: an empty advisor may always go; a real one only with `+0x2d8` set (`CAN_NOT_FIRE_CURRENT_ADVISOR`) and a month after it was hired (`CANT_FIRE_ADVISOR`, with the date).
- **Promoting** `0x1ad440(advisor, tag by value, reasons or null) -> bool`: below the maximum skill (`[0x233d9f8]`, `CANT_PROMOTE_ADVISOR_MAX_SKILL`), of a culture the country accepts (`0x2fc3e0`, `CANT_PROMOTE_ADVISOR_WRONG_CULTURE`), and the treasury at least the promotion's cost (`CANT_PROMOTE_ADVISOR_NO_FUNDS`).

## The commands

Named by the token their `+0x50` answers, as [commands.md](commands.md) finds them:

| What the player does | Command (vtable) | Written form |
| --- | --- | --- |
| Hire into a seat, or empty it | `change_advisor` (`0x1c43e58`; write `0x1c5b10`, check `0x1c5df0`, execute `0x1c5690`) | `change_advisor={ country="MAM" index=1 advisor={ id=3550 type=51 } }`: `index` the seat, the power's index; with no `advisor` the seat is emptied, as the AI fires (`0x11b70b`) |
| Promote | `promote_advisor` (`0x1c39878`; write `0xc3180`, check `0xc3230`) | `promote_advisor={ members={ dummy="MAM" dummy={ id=3549 type=51 } } }`, a tuple of the tag and the ID ([government.md](government.md), "members") |
| Dismiss an advisor of the pool | `dismiss_advisor` (`0x1c43db0`; write `0x1c6200`, check `0x1c6440`) | `dismiss_advisor={ country=<tag> advisor={ id type } }`; its check asks for the treasury at least the hire cost. Not offered |

- **The hire command's check** (`0x1c5df0`) only finds the advisor by its ID among the world's (`[world+0x1c30]`'s lists); **its execute** fires the seat's advisor (`0x1b6020`, `0x356890`), takes the new one out of the pool (`0x1b6120`), seats it (`0x355f70`) and pays the hire cost.
- The court view posts the hire command from `0xec8c80` (after `CONFIRM_ADVISOR_FIRE` when the seat is taken), the promotion from `0xec9be0` and the dismissal from `0xec9440`; the AI's spending (`0x11b2e0`) hires, fires and promotes through the same commands.

## Not established

- What `+0x2a8` and `+0x2c0` are exactly; the framework passes them on to the game's own cost functions only.
- Whether the court view shows advisors of the pool the country cannot hire for some reason other than their cost; the framework lists every real advisor of the pool not seated.
- Where the hire list's tooltip words a cost above the treasury; the framework says it in its own words.

## What the framework does, and what the game showed

On 2026-11-11, in a new game as the Mamluks ([the run](../prototype-log/2026-11-11-mechanics-and-advisors.md)): `advisor.options` listed the eleven advisors of the pool not seated (after one was hired), each with the hire cost the game charged (59.2 ducats for a skill 2 philosopher, the treasury going from 70.381 to 11.181) and its salary; `advisor.hire` posted `change_advisor={ country="MAM" index=0 advisor={ id=3545 type=51 } }` and the advisor took the administrative seat; `advisor.fire` was refused until 1444.12.11, a month after the hire, with the game's `CANT_FIRE_ADVISOR` line, and then emptied the seat with `change_advisor={ country="MAM" index=0 }`; `advisor.promote` raised the skill from 2 to 3 for 240 ducats, and before that was refused with the game's `CANT_PROMOTE_ADVISOR_NO_FUNDS` line.
