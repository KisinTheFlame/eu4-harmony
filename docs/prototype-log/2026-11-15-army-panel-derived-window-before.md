# 2026-11-15 Army selection exposes a shared derived-window hide

This run reproduces the reported exit after opening an army panel and advancing time, and captures the receiver rejected by the native window-ordering observer. The static dispatch evidence is in [window-visibility.md](../research/window-visibility.md#shared-display-methods-also-receive-derived-windows).

## Setup

- Source: `377ed553`, synchronized with `origin/master`, with a temporary diagnostic at the failing ordinary GUI-interface check. The diagnostic recorded root/GUI vtable RVAs and a bounded native stack; it did not change native behavior.
- Owner: `army-panel-crash-d3c3e1`; claim folder `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\army-panel-crash-d3c3e1-20261115-093215`; process 17492, hidden and silent. The claim's release is recorded in the [corrected run](2026-11-15-army-panel-derived-window-after.md).
- Release core with `eu4-harmony-core/dev-test-games`, EU4 1.37.5, and all eight copied enabled Workshop descriptors unchanged: `253263609`, `2830362508`, `2976470733`, `1999055990`, `3253723536`, `3837775887`, `3837793510`, `3838535796`. No player, installed framework or Workshop file was changed.
- The user authorized coordinate input in this isolated test game. `game start` reached the main menu; `menu.start --country TUR` began on 1444.11.11, paused. Escape dismissed the separate startup screen.

## Course and result

The reproduction script read `time.get`, clicked the first army row in the outliner at `(1845,197)` with `--scale 50` for a 3840 x 2160 frame, read `army.selected`, answered available opening questions, and requested one day at a time at speed 5. The selected army was `231928234038`.

1. At `09:43:18.415`, the core logged `ui: bound 2 element(s) in ArmyPanel`.
2. The first advance reached 1444.11.12. The next request lost the pipe as the framework ended the process.
3. At `09:43:18.902`, the diagnostic captured root class `0x1d57768` and GUI class `0x1d57510`. These exactly match the derived constructor's two vtable assignments. The bounded stack included game return address `0x123fedc`, in message-list item construction. Framework/trampoline addresses outside the game image were not treated as game RVAs.
4. At `09:43:18.903`, the core panicked with `native window ordering: ordering requires the ordinary window GUI interface`; at `09:43:18.905`, it logged `fatal error, ending the game: window root hidden` with the same reason. The script failed with exit 1.

Earlier attempts with the same claim and playset reproduced the same error. Control courses before this diagnostic run kept army selection paused on 1444.11.11 without an error, and advanced an unselected game from 1444.11.11 to 1444.11.14 without an error. They establish the selection-plus-time trigger; the vtable capture above belongs specifically to process 17492.

The root hide implementation is shared with a derived window. The registry required the ordinary GUI class before determining whether the receiver belonged to a related scope. Once army-panel binding created such a scope, a normal derived hide caused the observer to reject the receiver and end the game. This run does not identify the derived window's exact in-game name or prove every visibility path.
