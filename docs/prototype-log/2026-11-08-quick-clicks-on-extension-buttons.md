# 2026-11-08 Quick clicks on extensions' buttons (tested by me)

Whether every click of a quick series on an extension's button reaches the extension, now that the framework's left click sits where a double click reaches too ([interface-bindings.md](../research/interface-bindings.md), "Which pointer a click reaches" and "A press the game calls a double click").

## Setup

- **Builds**: the branch `claude/regional-battle-radius-button-6dea5b` at `56f223cd` ("Count every click of a quick pair on extensions' buttons"), and for comparison the same tree with `core/src/ui.rs` and `core/src/objects/gui.rs` from its parent, master's `0ade2650`; each built with `dev-test-games` and run in a test game of its own claim.
- **Game**: vanilla 1.37.5, the player's playset, which enables Regional Combat (EU4 Harmony) from `D:/Workspace/eu4-regional-combat/mod` (its `lua` branch at `e1afd44`).
- **Save**: a copy of `Castilla-5` (1470.3.13, Castile), paused. The army `28394028793910` (Guardia Real, province 149) selected from the outliner, then the region button right-clicked to open the radius editor, which showed radius 5.
- **Input**: `eu4-harmony game input click` on the editor's right arrow (`settings_plus`, at 1268,520 of the 3840×2160 frame) and left arrow (`settings_minus`, at 917,522), with `--count` 2 for a double click and 3 for a triple; four separate clicks were four requests sent one after another. The radius was read off screenshots after each step.

## What came out

| Step | With the change | Before it |
| --- | --- | --- |
| Editor opened | 5 | 5 |
| Double click on plus | 7 | 6 |
| Triple click on plus | 10 | 8 |
| Four separate quick clicks on plus | 14 | 10 |
| Double click on minus | 12 | 9 |

Before the change the second press of each quick pair was lost: a double click counted once, a triple twice (the first and the third), four quick clicks twice. With it every press counted.

The core's log, both runs: `startup: baseline 1.37.5: 1718 dependency points match` with the change (1711 before it, the seven points it adds) and `ui: bound 9 element(s) in ArmyPanel`, with no `cannot bind`. Neither game crashed; `release` reported both ended by `game stop` or the release.

## Not verified

- Clicks by a person's mouse: only remote control's input was used. It reaches the game as Windows' own mouse messages, which is where the game tells a double click from two presses, but the timing of a person's hand was not tried.
- Right clicks: the second of a quick pair of right clicks is still lost, read from the code (the game calls no callback for it), not tried.
- The army panel's own buttons and the peace view's gold buttons, whose behavior the research reads from the code, were not clicked.
- Extensions' buttons in host windows other than the army panel.
