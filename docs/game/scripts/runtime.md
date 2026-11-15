# Script


**Reading** (`pdx_core`):

- `0x170cd10` reads a file whole.
- A reader (`0x1709570`, vtable `0x1dbf7d0`) takes statements, key, operator and value, from a tokenizer: text (`0x1dbf958`, next token `0x170e700`) or binary (`0x1dbf988`).
- Keys are token ids: every key, trigger, effect and modifier name the game knows is in the token table. A name it does not know becomes an id made at run time.
- `0x1709280(writer, id)` writes a key in either format.

**Persistent objects** (`persistent.cpp`) share one vtable layout. Commands, the world, effects, triggers, scopes and the application all use it.

| Slot | What |
| --- | --- |
| `+0x08` | Write as a block (`0xd2f410`) |
| `+0x10` | Write the fields |
| `+0x18` | Read (`0x17068b0`), calling `+0x20` for each key |
| `+0x20` | Read one key |
| `+0x30` | Fix up after reading |
| `+0x38` | Report an error with file and line |

**Triggers.**

- `0xb8ba90(out, token)` makes one by its token, a switch of 899 cases.
- When the switch has nothing, `0xb89f60` tries idea groups, religions, trade goods, institutions, areas and scripted triggers.
- A trigger's evaluate is `+0x30` and its tooltip `+0x48` (vtable of `is_rival`: `0x1cf7d10`, [countries.md](../countries/countries.md)).

**Effects.**

- `0x5dec50(container, reader, token)`, also the effect block's `+0x20`, makes one of about 617 kinds (counted by the objects it allocates).
- Execute is `+0x78`, the tooltip's key `+0x68`.
- Every effect is listed at `[0x233fe48]` and checked at the end of InitGame.

**The rest of the script engine.**

- **Scopes.** A scope is a persistent object of 0xd0 bytes (`0x1c38158`): the province `+0x28`, the country `+0x2c`.
- **Modifier types** are built into the code (`0x8d84a0`, records of 0x60 bytes at `0x2358080`); `static_modifiers` and the like give them values.
- **Scripted triggers and effects.** Scripted triggers, effects and functions are databases of their own. Parameterized script is compiled per use (`metascript.cpp`, `0xdb0990`; inferred from its log lines).
- **Scripted GUI.** It binds `common\custom_gui` to `.gui` elements by name (`scriptable_gui.cpp`).

**Localisation.**

- `pdx_localize` parses the `.yml` files and keeps the hash table.
- `localize.cpp` loads `localisation\languages.yml`, then `localisation\**\*.yml`, then `localisation\replace`. The table is `[0x2357b20]`.
- The lookup `0x14b34a0` fills in `$NAME$`. Where `[Root.GetName]` is evaluated was not found.
- How the framework changes this: [unicode-text.md](../interface/unicode-text.md).

**Saves.** One reader and one writer serve text and binary, and a zip holds the parts ([saves.md](../program/saves.md)). The world's writer `0x74ff30` writes the game state.
