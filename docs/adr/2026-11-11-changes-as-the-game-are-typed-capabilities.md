# Changes as the game are capabilities of their own, not script text

A change to the game that no player could make, such as adding to a country's treasury, reaches extensions and requests as the game (ADR requests-act-as-the-player-or-as-the-game) as a capability of its own, named and shaped as the rest of the catalogue: its arguments typed, its refusals the framework's. The framework does not run a piece of effect script text (`add_treasury = 100`) handed to it, nor evaluate trigger text: what a trigger asks, an extension works out in Lua from the reads, and the script group offers only what scripts and extensions must share, flags, variables and firing an event.

Typed changes are added as mods need them, each with its consequences in the game worked out, as every capability is; none is in the first work on the script group.

## Considered Options

- **Run effect text in a scope** (`script.run(scope, effect)`): every one of the game's some 617 effects at once, carried out by the game's own code with every consequence it has, in the script language mod authors already write. But a mod's Lua would carry strings of another language that no type, test with the fake game or `eu4-harmony check` can read; the game's reader takes almost any text and tells nothing of what it passed over ([ADR one-command-layer-in-written-form](2026-11-03-one-command-layer-in-written-form.md)); and the catalogue would hold one capability that does anything, against everything else in it being named after what it does.
- **Evaluate trigger text**: the game's triggers are well researched and safe to evaluate, but a trigger only asks about facts the reads give, which Lua combines as it likes and tests without the game.

## Consequences

- What an extension can change beyond the player is only what the catalogue has; a mod that needs more asks for a capability, or does it in its own scripts and fires an event or sets a flag for them.
- Each typed change has to be worked out with what the game changes alongside it (the ledger, a recount of modifiers), where an effect would have done so by itself.
