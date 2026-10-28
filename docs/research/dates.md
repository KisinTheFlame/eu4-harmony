# Dates as text (for `wording::dates`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe and the game's files on 2026-10-28; what was checked in the running game is marked "tested".

## How a date is kept

- **A date is one i32**: hours since 1 January 5000 BC, in a calendar of 365-day years without leap years. value = ((year + 5000) × 365 + day of the year) × 24 + hour; 1444.11.11 is `0x035D7710`.
  - The year function `0xcfd70` subtracts the epoch: `44 8b 01 41 b9 01 00 00 00 41 81 e8 c0 55 9c 02` (`sub r8d, 0x29c55c0`, which is 5000 × 365 × 24).
  - The month lengths are 12 i32 at `0x1c3a5c8`: 31 28 31 30 31 30 31 31 30 31 30 31.
- Other helpers, each taking `const date*` in rcx: `0xcfe10` day of the year (from 0), `0x14c8120` day of the month (from 0), `0x14c7fd0` hour, `0x14c8030` adds months in place, `0x14c7750` weekday name.
- **The current date** is the i32 at `[[0x233fe78] + 0x1dd0]`.
- **Month names**: `0x14c75a0`(date*, out, short) localises the keys `January`…`December`, or `Jan`…`Dec` when short. Vanilla has no `Jan`…`Dec` keys, so short month names show as the key in every language.

## The functions that write a date

Each takes `const date*` in rcx and an unconstructed `std::string` slot in rdx, constructs the text there and returns rdx. All calls are direct.

| RVA | Writes | Callers |
| --- | --- | --- |
| `0x6e5830` | the key `LONG_EU3_DATE_STRING` (`"$DD$ $MONTH$ $YYYY$"`), r8b = short month name | 226 sites: almost every tooltip date, `[GetDateText]` in events, the message log, the save list, bookmarks, ledger pages, the history window |
| `0x14c7890` | a small format language, r8 = `const std::string*` format | 5 sites, below |
| `0x6e5b40` | month and year with a comma, rounded up to the next first of the month ("December, 1446" for 15 Nov 1446), in code | 33 sites: truces, guarantees, `RESEARCH_WHEN`, war subsidies and the like |
| `0x6e6100` | month, a space, year ("November 1444"), in code | 22 sites: espionage, great power, envoys, golden era and the like |
| `0x6e5dc0` | year, month, day ("1444 November 11"), in code | ledger ruler pages (through vtables), the history window, the end-of-game dialog |
| `0x6e4c20` | the key `EU3_DATE_STRING` (`"$YYYY$.$MM$.$DD$"`) | 133 sites: `[GetDate]`, envoy texts, triggers, the top bar's tooltip, and logs |
| `0x6e5390`, `0x6e50c0` | `DASH_EU3_DATE_STRING`, and `Y.M.D` | logs, file names, saves and scripts: never to be changed |

The keys are in `localisation/tmm_l_english.yml`. Vanilla localises the first two per language (German `"$DD$. $MONTH$ $YYYY$"`, French `"$DD$/$MM$/$YYYY$"`), and the Chinese language mod sets `LONG_EU3_DATE_STRING` to year, month, day; the dates made in code stay in English order whatever the language.

**The format language** (`0x14c7890`): `y` year (absolute), `m` month number from 0 (unused), `mw`/`mws` month name, `d` day from 1, `dw`/`dws` weekday, `e` "AD"/"BC" in ASCII, `h` hour, `p` "AM"/"PM", `w` a space; spaces are skipped and anything else is copied. Its callers copy the format into a local string, so it is recognized by its text:
- the top bar's date: `"d w mw w y"` (`0x1432256` in `0x14321d0`, the element `DateText`), "11 November 1444", rebuilt when the date changes;
- the timeline: `"y, w mw"` (`0x1430821`), "1444, November";
- great projects: `"ywe"` (`0x11a554f`, `0x11a5707`, `0x11a57e0`), "2560 BC".

**Written in place**, from `0x14c75a0`, so not covered by the functions above: the province history (`0x133a503`, "Nov 11", the year in another element), the start date widget (`0x112b3cf`, "11 November"), the treasure fleet tooltip (`0xf5f646`, "November 1444"), and month names on their own (`THE_MONTH_IS_BEFORE`, `[GetMonth]`).

## What the framework does

`wording::dates` replaces `0x14c7890` (its three formats only), `0x6e6100`, `0x6e5b40` and `0x6e5dc0`. When the player language has date templates in the framework's localisation (`EU4HARMONY_DATE`, `_MONTH`, `_YEAR`, `_YEAR_BC`; Simplified Chinese: "1444年11月11日", "1444年11月", "公元前2560年"), it writes them, keeping `0x6e5b40`'s rounding; otherwise the game's function runs. The long date is left to `LONG_EU3_DATE_STRING`, which language mods word themselves. The text is constructed with the game's own `std::string` append (`0x932f0`), so the callers free it as usual.

Tested 2026-10-28, Chinese language mod, as Korea at the start: the top bar shows "1444年11月11日", and the admin technology tooltip "我们预计将在1448年10月拥有足够点数来完成它" (`0x6e5b40`, rounded). Month and year (`0x6e6100`), year, month and day (`0x6e5dc0`), the timeline and great projects were not seen in the running game.
