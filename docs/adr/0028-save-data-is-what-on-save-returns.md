---
status: superseded by ADR-0036
---

# Extension save data is what `on_save` returns

Superseded: the function is `save_data`, and it is also asked before a reload (ADR 0036).

Whenever the game writes its world, the ironman exit save included, the framework calls each extension's `on_save` handler, and what it returns is that extension's save data in that save; when a session begins from a save, `on_session` gets it back. Native extensions instead set their data with `set_save_data` whenever it changed, and the framework wrote the last value set; an extension had to remember to call it after every change, and a name for a store call is not how a mod thinks of keeping its state. Where and how the data is written into the save does not change (ADR 0020), so a save written with a native extension is read by its Lua rewrite when the data's shape is the same.

The value is any value JSON can hold, at most 16 MiB of JSON. One that cannot be written (a function, a table with mixed keys, a cycle) leaves that save without the extension's data, and the error is logged as any other (ADR 0027).

## Considered Options

- **Keep `set_save_data` and `save_data`**: works, but data set before a later change is what the save keeps, and the calls are the store's, not the mod's.
- **A table the framework writes by itself**: nothing to call, but a mod's state lives in its own locals and would have to be copied into the table on every change, and a value that cannot be written would only show when a save is made, far from where it was put there.

## Consequences

- `on_session_ended` no longer decides what goes into the ironman exit save; it is for finishing what the extension was doing.
- An extension builds its data when asked, so it can leave out what it can work out again (caches, routes).
- Where the game writes its world off the main thread, the framework calls `on_save` at the safe point before the write; the extension sees no difference.
