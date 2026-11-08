# 2026-11-09 Pop-ups placed as the game's, a long Close button, and a demo of a scene (tested by me)

Whether the pop-up's shields and buttons stand where the game's message pop-up has them, whether extension windows close by the game's long Close button at their bottom, and how the whole reads in a scene of a mod's, as the owner asked after [the run before](2026-11-09-message-pop-ups-and-window-dress.md) ([plans/extension-windows.md](../plans/extension-windows.md)).

## Setup

- **Build**: this branch, `--release --features eu4-harmony-core/dev-test-games`, in test games of their own claims, started with `--mod`.
- **The demo**, Council of the Realm, a mod kept in the scratchpad, whose two tests pass under `eu4-harmony test`: a big panel opened from its entry button (the gold crest, a red ribbon, the realm's treasury, manpower, governing capacity and prestige read from the game as bars, the six great neighbours in a scrolling list with their flags, treasuries and a button to look at their capitals, two buttons that bring petitions); a monthly ledger beside the map (a blue ribbon, no big panel); a hint with no title; pop-ups of the steward (the game's OK alone), the marshal (Go to the richest neighbour's capital beside a button of the mod's, the player's shield on the left and that neighbour's on the right) and the treasurer (two buttons of the mod's).
- **The game's pop-up to measure against**: the owner's screenshot of the game's message pop-up (a missionary's success), which the game drew at the same scale as the test game's 4K frame (its header and title lines the same distance apart).

## What came out

- **What was wrong.** The pop-up had been written 460 by 300 with the game's places for 500 by 320: the right shield stood out past the window's edge, and the buttons, at 270 and 31 high, sat on its bottom edge.
- **Placed as the game's.** At 500 wide, the shields, header, title, text and buttons stand within a few units of the game's, the band and the shields set where the screenshot has them (the band's top 5 higher, the shields' 11 lower than the game's file has them, as the game draws its pop-up); its height follows its text from 262, as the screenshot's does, its buttons 19 above its bottom.
- **The long Close button.** Each window had the game's Close, in the game's words, centered on its gold line; Esc closed the panel through it.
- **The scene.** The steward's pop-up came at the session's start beside the ledger and the hint; the entry button opened the council with the realm's figures as the game had them (treasury 60, manpower 9.8k, governing 116 of 600, prestige 27) and France, Bohemia and Hungary first, France the richest; the marshal's pop-up showed Austria's shield and France's, and Watch the border wrote its answer into the panel; the treasurer's showed Approve and Reject; Bohemia's capital button turned the camera to province 266; at December the ledger gave the month's treasury and the neighbours' treasuries changed with the game's.
- **Flags.** The flag of `GFX_shield_thin`, with the ornament above it, is 64 by 84, not 62 by 62 as first laid out: rows of 66 overlapped; with the size right and rows of 90 they did not.
- **The log.** The game's `error.log` had no line about the framework's definitions; no panic; no crash at any release.

## Not verified

- The game's own message pop-up was not shown in the test game: the settings copied from the player send the message tried (a rival set) to the side's icons. The places were measured on the owner's screenshot alone.
- A pop-up whose text runs past five lines (its height is estimated from its characters, a full-width one as two).
