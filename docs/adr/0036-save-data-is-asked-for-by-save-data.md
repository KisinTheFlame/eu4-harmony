# An extension's save data is what `save_data` returns, also across a reload

The framework asks an extension for its data by calling its `save_data` function: whenever the game writes its world, as `on_save` was called before (ADR 0028), and before the extension's code is reloaded (docs/extension-reload.md), so the new code gets the old code's state in `on_session` as a loaded save gives it. What the data may hold and where it is written stay as ADR 0028 and ADR 0020 have them. This supersedes ADR 0028.

`save_data` is named for what it returns, not for an event: the other handlers are `on_` something that happened, and this one is the framework's question, which must be answered at once and cannot wait. The rename is a breaking change, so `api_version` goes to 0.3.

## Considered Options

- **Keep `on_save`**: no change for mods, but a reload is not a save, and a mod author would read `on_save` as "the game is saving" and not expect it before a reload.
- **A handler of its own for a reload** (`on_reload` handing the state to the new code): the same data asked for twice under two names, and a mod that keeps its state in one form for saves and another for reloads.
- **Keep the Lua state's globals across a reload**: nothing to ask of the mod, but the new code would start from what the old one left, which is not what its `main.lua` says, and a changed data shape would meet the old values.

## Consequences

- A mod written for 0.2 is refused until its manifest says 0.3 and its `on_save` is renamed, with the notice that says which version it was written for.
- A mod whose state survives saving and loading survives a reload; one that keeps state only in globals loses it at a reload as at a load.
