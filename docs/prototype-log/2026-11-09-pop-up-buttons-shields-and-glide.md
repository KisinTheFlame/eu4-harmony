# 2026-11-09 Pop-up buttons agreeing and declining, shields that open diplomacy, and a Go to that glides (tested by me)

What the owner asked after [the council demo](2026-11-09-council-demo.md): an agreeing button on the right with c, a declining one on the left with z; shields that open the diplomacy view on their country, as on the game's message pop-ups; a Go to that glides as the game's does, and `camera.look_at` able to do either; a title ribbon that is not pressed when held; flags in a list that do not overlap what stands beside them ([plans/extension-windows.md](../plans/extension-windows.md), [research/message-pop-up-actions.md](../research/message-pop-up-actions.md)).

## Setup

- **Build**: this branch, `--release --features eu4-harmony-core/dev-test-games`, in a test game of its own claim, hidden, started with `--mod` and the Council of the Realm demo (kept in the scratchpad), changed to `agree` and `decline` and rows 92 high; its two tests pass under `eu4-harmony test`.
- **Driven** through remote control: `input.click`, `input.key`, `input.mouse_down` and `mouse_up`, `popup.list`, `camera.get`, `camera.look_at`, `screenshot`. A new game as Austria at 1444.

## What came out

- **The buttons.** The treasurer's pop-up showed Reject on the left and Approve on the right; z answered `"decline"`, and the council's status line said so. The steward's pop-up, with the game's OK alone, closed on c. `popup.list` gave the buttons left to right: Go to and Watch the border, Reject and Approve.
- **The shields.** A click on the steward's left shield opened the diplomacy view on Austria; a click on the marshal's right shield opened it on France. The pop-up stayed open.
- **The big panel.** The first time, the council stayed open beside the diplomacy view opened from the shield: the game opens that view through `0x820990`, which closes its views in a loop of its own and never calls `0x80c6e0`, the function the framework observed. With an observer before `0x820990` too, the council closed as the view opened.
- **Go to glides.** The marshal's Go to answered `"goto"` and the camera went from 2995, 1428 to Paris (2841, 1490) over the next frames, read as 2971, 2926, 2898, 2879, 2865, 2856 across. `camera.look_at --province 183 --glide` answered Paris at once and moved the same way; without `--glide` it was there at the next frame, as before.
- **The ribbon.** Held down with the mouse, the council's red ribbon looked the same as before.
- **Flags.** The flag of `GFX_shield_thin` is drawn on a frame of 92 by 92, not 64 by 84; laid out at that size, the three neighbours' flags no longer reached their share bars.
- **The log.** No line about the framework's definitions in the game's `error.log`; no panic; the release found no crash.

## Not verified

- Whether the game itself opens the diplomacy view through `0x820990` from every other place (the country's shield in the top left, the map) was not tried; the observer is at the function, so any caller is heard.
- Whether the idler's own frame ever calls `0x820990` without the player asking, which would close an extension window that is a big panel unasked, was not looked at; in this run nothing closed the council unasked.
- Esc on a pop-up while a big panel is open (which of the two Esc closes) was not tried.
