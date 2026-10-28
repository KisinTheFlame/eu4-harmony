# Remote control is built into the framework and uses the same capabilities as extensions

Programs outside the game process, such as a debugging session or an AI playing the game from the command line, use the game through remote control, a part of the framework itself. It needs no mod in the playset. A remote client reaches the game only through capabilities, the same ones extensions use (ADR 0005): there is no way to read memory, call game functions or patch code from outside.

Each capability is implemented once, in a capability layer in the core that takes the caller (an extension, or remote control) and returns the framework's own snapshot types. The C ABI for extensions and the remote control engine are two adapters over that layer, and each exposes what makes sense on its side:

- **Both**: game objects and lists, pathfinding, commands, time control, the game console, and the capabilities added for remote control (country and province snapshots, recruiting, diplomacy, answering events and offers, the camera).
- **Extensions only**: what belongs to a mod: its interface elements, its extension save data, its texts, logging, event callbacks.
- **Remote control only**: what only makes sense from outside the process: waiting for time to advance, reading events after the fact, describing the capabilities, the main menu (loading a save, starting a game, returning to the main menu) and screenshots. A capability here moves to the ABI, by appending (ADR 0014), once an extension needs it.

A remote client acts as the player. Its orders count as orders the player gave by hand: they take an army back from the extension that was moving it and raise the "player gave an army an order" event, exactly as a click does. Requests are queued and run at safe points on the game's main thread, in the main menu as well as during a game.

## Considered Options

- **An extension in an ordinary mod that opens a pipe**: needs no change to the framework, but works only while that mod is enabled, cannot see what other extensions see, and could not add the capabilities remote control needs without the framework anyway.
- **A debugging back door (read memory, call functions)**: what a developer debugging the framework would like, but an AI playing the game should not see it, and the tools in `tools/` (disassembly, call tracing, crash dumps) already cover it.
- **A separate order source "remote"**: extensions could tell a remote client from the player, but each would have to decide again what to do with it, and the answer is almost always "the same as the player". Extensions built before it would read the new value as "no source" and misbehave.

## Consequences

- A capability a remote client needs is added as a general capability in the capability layer, not as something only remote control has, unless it only makes sense from outside the process.
- The host functions in `core/src/ext.rs` shrink to converting between the ABI and the capability layer.
- An extension cannot tell a remote client's orders from the player's.
