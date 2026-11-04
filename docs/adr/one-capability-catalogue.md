# One capability catalogue, a stable abstraction for extensions and remote clients

Every capability is described once, in one catalogue: its group, name, arguments, result and failures, and which side has it. Extensions and remote control expose that catalogue, each in its own form (a Lua call, a JSON request), with the same names and the same meanings. The catalogue is the framework's stable abstraction: capabilities are named and shaped after what a player sees and does, not after how the game keeps it, so that when the game's code changes, the capability layer takes it up and what extensions and remote clients see stays. The Lua host's bindings, the editor's type definitions, the command line tool's reference (`wiki`) and remote control's `describe` are all made from it.

The split of which side sees which capability in ADR remote-control-uses-capabilities stands; what changes is that neither adapter mirrors the layer below it any more. The command line client of ADR remote-control-engine-and-transports takes its capabilities from the catalogue instead of describing them itself.

## Considered Options

- **Each adapter shaped after the functions under it** (what the C ABI and remote control did): every change in how the game is read reached extensions and remote clients, capabilities that came out of one piece of the game's code stayed split as the code was (an army's route, whether it can enter, whether a strait is open, whether it can attack), and each adapter needed its own description.
- **Lua shaped for mods, remote control left as it is**: two names for one capability, and two references that drift apart.

## Consequences

- Remote control's capabilities are reshaped with the rest, and the names `eu4-harmony game` takes change; 0.x promises no compatibility. `eu4-harmony game help` goes; `wiki` is the one reference.
- Adding a capability, a handler or a field of a result changes no version; changing what one that exists means or how it is shaped raises `api_version`, which remote clients read from `describe`.
- A capability a mod needs is added in the catalogue's terms, general for any mod (ADR extensions-use-capabilities-only), and reaches both sides at once when both should have it.
