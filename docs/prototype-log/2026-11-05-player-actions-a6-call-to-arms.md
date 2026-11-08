# 2026-11-05 Player actions A6 merged with A5: the call to arms as a panel action (tested by me)

The player build of this branch after merging the integration branch with A2 to A8, within a claim, started hidden; new games as France, war declared on Savoy without a casus belli at 1444.12.16 and no ally called.

- **`diplomacy.options PRO`** gave `call_to_arms` (召集盟友！), `can` true, with the acceptance the panel's entry asks: accepting, 26 in all with "承诺给予萨伏依土地" +20 (65 in the second process, "对敌人的态度" +40 among them).
- **`diplomacy.call_to_arms PRO`** was sent from the panel's list and Provence `declined`, as in A5's run; `CAS`, no ally, was refused "the diplomacy panel offers no call to arms to CAS".
- **The window's byte**: a build that cleared the call's `+0x50` as the diplomacy window does for a country the AI plays had the call answered `pending`, and Provence had not joined the war three days on; that change was taken back, and the question is in research/peace.md.
- **Cleanup**: each release put back the player's files and the game folder's framework files; the screenshots were deleted.
