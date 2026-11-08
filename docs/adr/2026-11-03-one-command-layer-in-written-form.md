# Capabilities that change the game go through one internal command layer, in the commands' written form

Every command the game runs, the player's, the AI's and its own, is written out and read back before it runs, in single player too, and the written form says everything the command carries ([research/commands.md](../research/commands.md)). The core therefore has one command layer that posts a command from its written form, through the game's own reader, check and post, and hears every command the game executes, in the same form. Capabilities that change the game are built on it: each still checks its arguments and gives the game's reasons for a refusal as it does now, then writes its command as text and hands it to the layer. A capability that now calls the game's functions directly, such as ordering an army to move, moves onto the layer once the game's own command is tried in the game and found to do the same.

The layer is the core's alone. Its vocabulary is the game's: the commands' type names and keys, tags and province numbers. None of it reaches extensions or remote clients, whose capabilities stay named and shaped after what a player sees and does (ADR one-capability-catalogue); what the layer hears reaches them only as events of that kind, added as mods need them. The layer can post a command for any country; the capabilities on it act for the player alone, as remote control does (ADR remote-control-uses-capabilities).

## Considered Options

- **Make each command in memory, as recruiting and diplomacy did**: a constructor, a size and fields for each of the game's several hundred command types, each found and kept as dependency points of its own; the written form needs a handful for all of them, and is what the game itself turns every command into.
- **Offer raw commands as a capability**: a mod or a remote client could post any of the game's commands at once, but it would depend on the game's names and keys, which is what the catalogue exists to keep from them, and could act for any country.
- **A framework vocabulary for every command**: the same as a capability per command, made all at once.

## Consequences

- The game's reader takes almost any text: an unknown key is passed over, a number it cannot read becomes -1, a missing brace is not an error, and none of it is told to the caller. The layer writes every command it read out again and refuses one whose keys did not all come back as given; a key left out takes the game's default.
- A command's check answers only yes or no. The reasons a player sees come from the checks the game's interface makes before it makes a command, so the capabilities keep making those.
- The game's names and keys could change with a new baseline; EU4 is not expected to be updated again, and the layer's tests and dependency points would show such a change at once.
- A command's sender does not tell the player's commands from the AI's, so what the layer hears says what happened and to which country, not who asked for it.
