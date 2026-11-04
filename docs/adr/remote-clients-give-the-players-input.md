# Remote clients may give the game the player's mouse and keyboard input

Remote control has a capability group of its own, `input`, through which a remote client clicks, moves the mouse, drags, turns the wheel, presses keys and types text at screen positions of the frame the game draws, as the player does with a real mouse and keyboard. It is the fallback for what has no capability of its own yet, such as building in a province, choosing an idea or a mission, or the trade view: a client that looks at `screenshot` can do there what the player does. Where a capability exists, it is the way to use: its answer says what the game did and why it refused, a click does not. The catalogue's descriptions (`wiki`, `describe`) say so.

Input is a capability like the others in the sense of ADR remote-control-uses-capabilities and ADR one-capability-catalogue: it is something the player does, shaped after what the player sees, the frame, and not after how the game keeps it. It opens no way to read memory, call game functions or patch code. ADR remote-control-is-always-on already holds that a program running as the player can send the game input; the framework makes it easy and puts it where the other capabilities are.

- **Remote control only.** Extensions are in the game, with their interface elements and capabilities; input would let a mod work the game's interface past the catalogue. As with every capability of remote control's only, it moves to extensions by being added there once one needs it (ADR remote-control-uses-capabilities).
- **Given inside the process.** The framework hands the game the input where the game reads it, so it works while another program is in front or the game is minimized, does not move the Windows cursor and reaches no other program. The player's own mouse and keyboard keep working beside it; nothing locks either out.
- **On the player's path.** The input goes where the player's goes: through the framework UI first, which takes what falls on it and holds everything back while a modal window is open, and then to the game.

## Considered Options

- **Input from outside the process, as `tools/game-input.ps1` sent it** (`SendInput`): no change to the framework, but the game has to be brought to the front, the real cursor moves, and a click meant for the game lands in whatever program is in front, as happened in a run (prototype-log.md); positions are the screen's, not the frame's, and an injected Backspace deleted more than one character.
- **Only named capabilities**: every answer means something, but each part of the game a client needs waits for its reverse engineering, and a client that meets one not done yet cannot go on.
- **Extensions too**: a mod could work any panel, but then the catalogue no longer says everything a mod can do to the game, and a mod's clicks would fight the player's.

## Consequences

- What a click at a position does depends on the resolution, the interface scale, the language and the mods; the catalogue's promise that a capability keeps its meaning across game builds (ADR one-capability-catalogue) holds for `input`'s capabilities, not for what the game puts under a position.
- A remote client that uses input acts as the player as much as with any capability: the orders it gives so are the player's (ADR remote-control-uses-capabilities), and an extension cannot tell them from the player's.
- The development tools that sent input and took screenshots from outside the process, `tools/game-input.ps1` and `tools/screenshot.ps1`, go; where no framework runs (measuring vanilla with `tools/perf-run.ps1`) the tool sends the few inputs it needs itself.
