# 2026-11-09 Extension windows, pop-ups and entry buttons (tested by me)

Whether extension windows and pop-ups ([docs/extension-windows.md](../extension-windows.md), W1 to W4) are built in the game's own interface, look like the game's, and do what the plan says, in the running game.

## Setup

- **Build**: this branch, `--release --features eu4-harmony-core/dev-test-games`, in a test game of its own claim, started with `cargo xtask game start --mod <folder>` (added on this branch).
- **Game**: vanilla 1.37.5, the player's playset and a test mod, Windows Demo, kept in the scratchpad: a big panel `reforms` (460 by 560, an entry button of the strip's sprite, frame 3; a text, two text buttons, an icon with a tooltip, a progress bar, a flag with a text beside it, a list of three rows of a flag, a text, a progress bar of another sprite and a text button), a small window `tracker` (not a big panel, anchored to the top right), and three pop-ups of three buttons shown at each session's start. Its two tests pass under `eu4-harmony test`.
- **Games**: new games as Austria and, after a return to the main menu, Castile, through `menu.start`; input through `eu4-harmony game input`, looked at through screenshots.

## What came out

- **Definitions read after startup.** The first build failed: the game would not build `eu4harmony_windows_demo_tracker_2`, and its `error.log` said why, a parse error at `background = { … }`: a `windowType` takes no such block, which is a `containerWindowType`'s. The window's `backGround = "<element>"` naming a button of the stretching sprite, as the game's dialogs have it, is what it takes. With that, the definitions read through the game's reread (`windows: reading 3684 bytes of definitions`), and the game's `error.log` had no line about them for the rest of the run. Before the fix a window that would not build was tried every frame, filling the game's log; it is now tried once per generation of definitions, and a pop-up that cannot be built answers `closed`.
- **Pop-ups (W1).** Three cascade at the screen's middle, the last on top, each with its title, its text with its value filled in, and its three buttons, in the game's dialog look. A click on Later answered 3; `popup.answer --popup 2 --button 1` answered 1; Esc answered the last open one with its last button, 3; `popup.answer` refused `--popup 9` (`not_found`) and `--button 7` (`invalid_argument`) in the framework's words. `popup.list` listed the open ones with their mod and words; `events --kinds popup` gave the three `popup` events.
- **Extension windows (W2, W3).** The tracker stood at the top right with the game's background, title, close button and its text, which its handler changed when a pop-up was answered. The reforms panel showed every game widget where its layout put it: the title, the text, the two text buttons (the game's 149-wide sprite), the icon, the progress bar at a quarter, Austria's flag, and the list's three rows with France's, England's and Castile's flags, their bars at 90, 50 and 20 percent, and their Go buttons. A click on the second row's Go was heard as `reforms.go` with row 2; Fill was heard and set the text, in yellow from its formatting code, and the bar to three quarters. The icon's tooltip showed in the game's tooltip frame. Dragging the title moved the window; it opened again where it was dragged.
- **The game's big panels.** The Holy Roman Empire's button closed the reforms panel (`on_window` heard it closed) and left the tracker, which is no big panel, open. The entry button then closed the Empire's panel and opened the reforms panel, with what had been set on it.
- **Esc and the close button.** Esc closed the reforms panel; the entry button opened it and, pressed again, closed it.
- **Entry buttons (W4).** The entry button stood left of the Empire's and the papacy's buttons, in the strip's round background, as Austria and as Castile. After the return to the main menu and the new game it opened the panel again: the framework bound it to the strip the new game made.
- **Extension reload.** `extension reload --id windows-demo` closed the windows, read the definitions again under new names, and the new code's tracker and pop-ups were built.
- **The session's end.** At the return to the main menu every window and pop-up went; the extension's three waiting tasks were dropped as a session's end drops them.
- **The log.** `startup: baseline 1.37.5: 1774 dependency points match`; no panic. `release` reported every start ended by the release or a stop, and no crash.

## Not verified

- A change of the screen's resolution, or the game's `reload` of an interface file, with an extension window open: the framework rebuilds a window the game kills, which was not seen to happen.
- A pop-up shown at a session's very start opens under the game's own start-of-game window, which is built later; the player sees it once that window is closed.
- A list of more rows than it holds scrolling; a window anchored to each corner and edge (only the top right and the middle were seen); a flag set at run time to a country that does not exist (refused in code, not tried).
- Right clicks on extension windows' buttons; a click on a text button that is disabled.
- More than one mod with entry buttons, and a strip of all eight vanilla buttons.
