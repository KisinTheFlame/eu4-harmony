# 2026-11-15 Corrected window ordering game for the user

This run hands the user a visible isolated game with the corrected derived-icon ordering, following the [actual-playset acceptance](2026-11-15-window-ordering-actual-playset.md). It is an experience handoff, not another acceptance claim.

## Setup

- Source tree: `13fd7643`, merging `origin/master` at `9ac51408` into the accepted ordering branch. The only manual conflict was the documentation index; both sets of records were retained. Formatting and the locked release build with `eu4-harmony-core/dev-test-games` passed before starting.
- Owner: `ordering-user-fixed-1de2b155`.
- Claim: `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-user-fixed-1de2b155-20261115-044403`.
- The copied playset retains all eight enabled Workshop descriptors unchanged: `253263609`, `2830362508`, `2976470733`, `1999055990`, `3253723536`, `3837775887`, `3837793510`, `3838535796`.
- The user requested a visible game. It is started through `game start --visible`, silent, with its own user folder, core and pipe. The player's files, installed framework and Workshop files are unchanged.

## Handoff

`game start --visible` completed successfully at the main menu, with process 51704. Only after that success was `menu.start --country FRA` issued; it also completed successfully. France is paused on 1444.11.11. `army.split --army 545460846646` created army `5205500362806`, leaving two armies in Paris for the user to select and try the native attach picker. `question.answer --question 2 --option 0` dismissed the opening event, and one Escape dismissed the separate startup screen. A screenshot confirmed the paused map and three land armies in the outliner, including the two 7,000-strong Paris armies.

The game remains visible, silent and held for the user. No army was selected or picker action performed on their behalf, and this handoff does not claim a new acceptance result for the merged build. The completed acceptance of the ordering correction is linked above. Release this owner when the user finishes; do not stop another session's game.
