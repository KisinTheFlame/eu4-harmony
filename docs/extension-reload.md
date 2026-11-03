# Extension reload plan

Status: decided (2026-11-04); R1 to R3 built and tried in the game the same day ([prototype-log.md](prototype-log.md)).

## Goal and scope

- **Goal**: a mod author changes an extension's Lua code or its texts and sees the change in the running game, without restarting it and without losing the session.
- **In scope**: reloading one extension's Lua code (`main.lua` and the modules it requires) and its localisation, in the main menu or in a session; carrying its state across; `eu4-harmony reload`, once or on every change; renaming `on_save` to `save_data`.
- **Out of scope**: interface fragments (`ui/`), which are spliced into the game's own `.gui` files as the game reads them and take effect only when the game builds that window again; a changed manifest; mods added to or taken out of the playset; reloading from inside the game (the player never reloads by accident).

## Behavior

### What reloads

- The extension's Lua code is read again: a new Lua state runs the new `main.lua`, with the same id, permissions and data folder as before.
- Its localisation is read again, so `text.get` gives the new texts. Keys the framework spliced into the game's own localisation at startup keep the text they had then.
- A manifest whose id or permissions changed, or whose `api_version` this framework does not provide, is refused: those are what the player saw in the mods window, and they change only with a restart.

### State

- In a session, the old code's `save_data` is asked for its data, and the new code's `on_session` gets it, as when a session begins from a save. A mod whose state survives saving and loading survives a reload; nothing new is asked of it.
- When `save_data` fails or gives what JSON cannot hold, `on_session` gets nil, and the answer says so.
- Lua globals are not carried: the new code starts from its own `main.lua`.
- In the main menu there is no session: the new code runs its `main.lua`, and `on_session` comes when a session begins.

### What the old code left

- Its waiting tasks are dropped, as at the end of a session (ADR 0030).
- Its map coloring is released.
- The values it set on its interface elements are forgotten; the new code sets them again, usually in its first frame. An element it never sets keeps the last look the old code gave it.
- An extension the framework stopped (ADR 0027) runs again after a reload. Its old code is not run again, `save_data` included, so `on_session` gets nil, and the answer says why.

### When the new code fails

Nothing changes: when the new `main.lua` does not compile or fails as it runs, or the manifest is refused, the old code goes on as it was, and the answer gives the error.

### The command

- `eu4-harmony reload`, run in a mod's folder (or given one), reloads that mod's extension in the running game, through remote control, and prints what came of it.
- `eu4-harmony reload --watch` does so whenever a file under the mod's `eu4-harmony/lua/` or `eu4-harmony/localisation/` changes, until it is stopped.
- Remote control has it as `extension.reload`, in the main menu and in a session, for remote clients only (ADR 0015).

## Parts

- **R1**: `on_save` becomes `save_data` (ADR 0034), `api_version` 0.3; the type definitions, the guides, `wiki`, the tests and regional combat follow.
- **R2**: the core reloads one extension in place, keeping its number, so what belongs to it (its interface elements, its map coloring, the orders it gave) stays its own; `extension.reload` in the catalogue.
- **R3**: `eu4-harmony reload` and `--watch`.
- **Acceptance**, in the game with regional combat: a changed text shows in the tooltip; a changed decision rule takes effect with the task still on; a syntax error leaves the old code running and is printed; a reload in the main menu and one after a stop both work.
