# Monarch points and the treasury (for `game::power` and A2)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-11-05 with `cargo xtask disasm`, the token table (u32 token then its name, 0x208 bytes an entry, around `0x207b068`) and scripts over the PE file; what was tried in the game is in [prototype-log.md](../prototype-log.md) ("Player actions A2"). The fields and the costs' first reading are in [countries.md](countries.md), "What the economy's actions cost" and "National focus"; this file is what the panels' buttons check, say and post. Part A2 of [player-actions.md](../player-actions.md) is built on it.

## The commands

Every command below was found by its type token: the command classes answer it from their vtable's `+0x50` (`mov eax, imm32; ret`), which leads to the vtable (`+0x10` write, `+0x20` read, `+0x40` check, `+0x48` execute; [commands.md](commands.md)).

| Button | Command (vtable) | Written form | Its check |
|---|---|---|---|
| Research a technology, raise stability, reduce inflation, raise mercantilism | `spend_power_command` (`0x1c554c0`) | `spend_power_command={ country="FRA" power={ data=I category=K skill=S } }` | `0x2a76d0` → `0x94f250(spending, tag)` |
| National focus | `set_national_focus_command` (`0x1c54330`) | `set_national_focus_command={ country="FRA" power=F }` | `0x2b1a20`: `F` at most 3 |
| Take a bank loan | `take_bank_loan` (`0x1c8e1e8`) | `take_bank_loan={ country="FRA" }` | `0x5d5450`: bank loans below the most, not surrendered |
| Repay a loan | `pay_loan` (`0x1c8dab0`) | `pay_loan={ country="FRA" loan={ id=N type=T } }` | `0x5d74d0`: the loan's ID names a loan |

- **`spend_power_command`** (write `0x2a7440`): `country`, then `power` (token `0x2ab4`) the power spending inline at `+0x58`, written by the spending's own writer `0x94eed0`: `data` (`+0x18`) only when it is not 0, `second` (`+0x1c`) only when it is not −1, `category` (`+0x10`, the kind of spending), `skill` (`+0x14`, the kind of monarch power), `negative_power=yes` when `+0x30` is set. Execute `0x2a7310` runs `0x94f140`.
- **`set_national_focus_command`** (write `0x2b1780`): `country`, then `power` (token `0x2ab4` again) the focus at `+0x58`: 0 administrative, 1 diplomatic, 2 military, 3 none. Execute `0x2b1740` calls the setter `0x3db150(country, focus, 1)`. **Its check does not check the focus's cooldown**: only the court view's button does (below).
- **`take_bank_loan`** (write `0x5d6300`): `country`, `interest_modifier` (`+0x58`, fixed point), `duration` (`+0x64`), `fixed_interest` (`+0x5c`), `interest` (`+0x60`), `estate_loan` (`+0x68`); the button leaves them all at their defaults.
- **`pay_loan`** (write `0x5d7290`, shared with `cancel_loan`): `country`, then `loan` (token `0x28ad`) the loan's ID (`+0x60`) as an ID object `{ id type }` (`0x5ddc30`, `0xce610`). A loan keeps its ID at `+0x10` as every ID of the game's is kept: the type (an i32) at `+0x10`, the number at `+0x14` (tested: two loans taken one after another had the same `+0x10` and the numbers 1 and 2). Execute `0x5d7760` looks the ID up (`0x14cf730`), unlinks the loan from the country's list (`+0xde8`; while the list is walked, `+0xdfc` set, it only marks the node's `+0x18`), and pays the loan's amount × 1000 plus the interest left, `0x86b4b0(loan, &out)` (thousandths of ducats; `"CPayLoanCommand::Execute loans %i"`). It does not compare the price with the treasury. `pay_all_loans` (`0x1c8db58`) was not followed.

## The power spending

- **Built by the buttons in their own memory**, 0x38 bytes: class `0x1c55480` at `+0`, `0x165` at `+8`, kind `+0x10`, skill `+0x14`, data `+0x18`, second `+0x1c` (−1), an object pointer `+0x20` (null), an ID `+0x28` copied from `[0x233fea8]`, the byte `+0x30`. The stability view at `0xf8ab35`, the technology view at `0x1009480`, the inflation and mercantilism checks from 16-byte constants (`0x1dc2750`: kind 15 skill 0; `0x1dc27c0`: kind 26 skill 1; `0x1dc2740`: kind 2 skill 0).
- **The kinds used here**: 1 `advance_tech` (skill: the category's kind of power, `+0x78`; data: the category's index, `+0x50`, which `0x9552a0` uses as the place in the categories' vector `[0x242be50]+0x18`), 2 `boost_stab` (skill 0), 15 `reduce_inflation` (skill 0), 26 `promote_merc` (skill 1).
- **The cost**: `0x9526d0(spending, tag, text, 0)`, whole points ([countries.md](countries.md)).
- **The check** `0x94f250(spending, tag)`: the cost against the country's points of `skill` (`+0x260 + 4 × skill`), unless `negative_power`; then by kind (jump table `0x94fa20`): `advance_tech` nothing more; `boost_stab` `0x382970(country, 0)` and stability (`+0xbd8`) below 3000; `reduce_inflation` `0x38d5a0(country)`; `promote_merc` `0x3f47a0(country, 0, 0)`.

## Technology

- **The technology view** (`0x1009350`, its update for one category's row; `0x1012160`, the research button's tooltip): the player's level in the category, `[[country+0x2440]+0x128]` at the category's index; the category's levels, a vector of pointers at `+0x60`–`+0x68`. **The button is shown only while the level is below the number of levels less one** (`0x10093f4`); then the cost (`0x9526d0`, kind 1) against the points: enabled (`+0x268`) when the points are enough, disabled (`+0x270`) otherwise.
- **The tooltip with too few points** (`0x1019810`): `CV_RESEARCH_TECH_NOPOWER` (`TYPE` the localised `MON_ADM_LONG`, `MON_DIP_LONG` or `MON_MIL_LONG` from `0xa491f0(out, kind)`, `POWER_COST`, `POWER_AVAILABLE`), then when the points will be there: `RESEARCH_WHEN` with `DATE`, the date `0x14c8030(&date, months)` months on from today, months being `0x380910(country, &cost × 1000, kind)`, the date written by `0x6e5b40(&date, out)`; or `RESEARCH_NEVER` when the months are 0 or fewer (no monthly gain). The framework gives both lines as the reasons. At the last level the button is not shown, and gives no reason.

## National focus

- **The court view** (`0xed2be0` shows the buttons, `0xed6d90` their tooltips, `0xed65f0`/`0xed6860`/… one per button press): a press asks `0x3db490(country, text)` first and does nothing when it answers false; otherwise it posts the command with the new focus: **the button of the focus set removes it** (`0xed666c`: 3 when the focus was already this one, else this one).
- **The cooldown's line**: `0x3db490` writes `NAT_FOCUS_COOLDOWN` (`YEARS`, `DATE`) into its text when the focus may not change yet.
- `can_change_national_focus` of `common/scripted_functions` (below, `+0xb8`) is not read at these buttons; where the game reads it was not found.

## Stability

- **The stability and expansion view** (`0xf8a4d0`, the `stability_boost` button at `0xf8aa46`): stability (`+0xbd8`) at 3000 → `STABILITY_BOOST_MAXED`; `0x382970(country, text)` false → its text (`0x382970` hands the text to `0xaccac0`, which reads two arguments and writes none, so the tooltip is empty then; the framework gives `can_increase_stability`'s failing lines); the cost above the administrative points → `STABILITY_BOOST_NOPOWER` (`POWER_COST`, `POWER_AVAILABLE`, `0xf90130`); else `STABILITY_BOOST_SPEND`.

## The economy

- **The economy view** (`0xec1aa0`):
  - **Take a loan** (`take_loan`, `0xec28ad`): `0x33e060(country, text)` false → `NO_MORE_BANKLOANS` and its text, `NO_MORE_BANKLOANS_TOO_MANY` (`LOANNUM`, `MAXNUM`) when the bank loans (those whose lender's tag is not valid, the byte `+0x27` of the loan) reach `0x33eb70(country)` (at least 3), `NO_MORE_BANKLOANS_UNCONDITIONAL` when `+0x2883` is set; then `0x3ed490(country)` true → `NO_NATION_RUINING_LOANS` (`DATE` from country `+0x2738`, `OPTION` the localised `FE_BLOCK_NATION_RUINING`), the host's rule against ruining a nation; else `TAKE_BANK_LOAN` with `AMOUNT`, `max(loan_size (+0x1105, a byte) × estimated_loan (+0xe3c), 1000) / 1000` ducats. **The command reckons the same when it runs** (`take_bank_loan`'s execute `0x5d54e0`, `0x5d558e`): the loan (`0x86aed0`) is of those ducats (`+0x2c`) and the treasury gets them × 1000 (`0x338540`, ledger 9). `estimated_loan` is written by `0x93fe30` (its callers not followed), and changes as soon as the income does: tested, developing Paris's tax raised the loan offered from 128 to 129 at once, and a loan then taken was of 129 and raised the treasury by 129; across a month's end with nothing done it stayed 128.
  - **Reduce inflation** (`combat_inflation`, `0xec3e9d`): the modifier `may_not_reduce_inflation` (`0xf0`, `0x8e7b60(0x93d750(country + 0x28c0) + 8, 0xf0)`) → `CAN_NOT_REDUCE_INFLATION_TT`; inflation at 0 or below → `NO_INFLATION` (`VAL` 0); the cost above the administrative points → `CV_COMBAT_INFLATION_NOPOWER` (`POWER_COST`, `POWER_AVAILABLE`); else `CV_COMBAT_INFLATION_DESC`. The button's own verdict is `0x38d5a0(country)`, which also asks `can_reduce_inflation`.
  - **Loans** (`repay_loan`, `0xec368d`): the button opens the loan window; `CV_ECONOMY_LOAN_NOREPAY_BUTTON_DESC` when there are none (`+0xdf8`, the list's count).
- **The loan window's repay button** (`0xeb1c70`, `pay_button`): its price is the amount × 1000 plus `0x86b4b0`; when the button is disabled its tooltip is `CV_ECONOMY_LOAN_REPAY_CANT_AFFORD_DESC` (`VAL`, two decimals). It is disabled when the treasury (`+0xbb0`) holds less than the price (tested: at −235 ducats every repay button was disabled with that tooltip); where the window sets it was not followed. A press asks to confirm (`CV_REPAY_LOAN_CONFIRM_TEXT`, the price and the interest in it).
- **Mercantilism** is the trade view's (`0x10215a0`, `promote_mercantilism` at `0x1022441`): `0x3f47a0(country, text, cost text)` writes `ALREADY_MAX_MERC` (mercantilism `+0x1098` at 100000), `NO_PROMOTE_MERC_NO_POINTS` (`COST`, `VAL`; diplomatic points, `+0x264`) or, allowed, `PROMOTE_MERC_IRO`.

## Texts with values

- The game's localisation lookup `0x14b34a0(out, key, values, count)` takes an array of 0x68-byte values (`0x8f670` makes one, `0x8ee90` frees it): the name, a C string, at `+0`; the kind at `+0x28`; for kind 1 the text, a `std::string` at `+0x30`; kind 5 an integer and kind 7 a fixed-point number further on (`+0x60` and `+0x50`, read from callers only). `0x8ecd0` and `0x8edc0` are its one- and two-value forms. The framework passes every value as text (kind 1), formatted as the tooltip does.

## `common/scripted_functions`

- The object at `[0x242bdd8]` (made on first use, `0xacd400`, 0x2e8 bytes) holds one entry for each function the game names (`0xacdf60`, `0xace290`): `can_join_hre_league` `+0x38`, `can_leave_hre_league` `+0x48`, `can_harsh_treatment` `+0x58`, `can_reduce_war_exhaustion` `+0x68`, `can_reduce_inflation` `+0x78`, `can_declare_bankruptcy` `+0x88`, `can_release_colonial_nation` `+0x98`, `can_claim_defender_of_faith` `+0xa8`, `can_change_national_focus` `+0xb8`, `can_increase_stability` `+0xc8`, `can_add_province_to_hre` `+0xd8`, `can_remove_province_from_hre` `+0xe8`, `can_increase_autonomy` `+0xf8`, `can_decrease_autonomy` `+0x108`, `can_make_core` `+0x118`, `can_make_state` `+0x128`, `can_abandon_state` `+0x138`, `can_improve_tax` `+0x148`, `can_improve_production` `+0x158`, `can_improve_manpower` `+0x168`, `can_exploit_tax` `+0x178`, `can_exploit_production` `+0x188`, `can_exploit_manpower` `+0x198`, `can_change_culture` `+0x1a8`, `can_move_capital` `+0x1b8`, `can_move_trade_port` `+0x1c8`, `can_order_assault` `+0x1d8`, `can_order_force_march` `+0x1e8`, `can_scorch_earth` `+0x1f8`, `can_garrison_sorties` `+0x208`, `can_seize_colony` `+0x218`, `can_burn_colony` `+0x228`, `can_colonize_province` `+0x238`, `can_colonize_province_siberian_frontier` `+0x248`, `can_add_seat_in_parliament` `+0x258`, `can_remove_seat_in_parliament` `+0x268`, `can_add_estate` `+0x278`, `can_expel_minority_from` `+0x288`, `can_have_colonial_nations` `+0x298`, `is_possible_hre_province` `+0x2a8` (and more after it, not read).
- **An entry** is a pointer, null when the file leaves the function out; it holds a vector of conditions, data `+0x18`, count `+0x24`, each a `condition = { tooltip potential allow }` of the file: `potential` a content trigger at `+0x30`, `allow` one at `+0xa8` (their children at `+0x38`/`+0x44` and `+0xb0`/`+0xbc`). **The verdict** `0xaccac0(entry, tag)` → `0xacd150`: for each condition whose `potential` holds, `allow` must hold.
- Where a condition's `tooltip` key is kept was not followed: the framework gives `allow`'s failing lines ([triggers.md](triggers.md)), and a mod's own `tooltip` text is not shown.

## Not known

- Where the loan window enables its repay button; `pay_all_loans`'s written form; how many bank loans `0x33eb70` allows (France at the start was allowed more than 34).
- Where a scripted function's `tooltip` is kept, and where `can_change_national_focus` is read.
