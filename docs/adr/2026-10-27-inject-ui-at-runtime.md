# The framework injects interface elements at run time

Extensions declare, through a capability, the elements they want added to the vanilla interface, and the framework injects them into vanilla windows at run time; mods do not override any vanilla `.gui` file. Paradox mods usually add interface by overriding vanilla `.gui` files, but when two mods override the same file only one takes effect; once there are more native extensions, adding things to the same vanilla panel will be common.

Injection splices text into `.gui` files as the game reads them: the framework hooks the PhysFS functions eu4.exe exports by name (`PHYSFS_openRead`, `readBytes` and others), inserts element definitions into the host window's definition, and hands the result to the game's own parser. Creating the elements, their lifetime and reloading all follow the vanilla flow. The splice applies to the copy of the file that actually takes effect, so it also coexists with ordinary mods that override the same file. The evidence is in [research/ui-injection.md](../research/ui-injection.md).

## Considered Options

- **Mods ship `.gui` files that override the vanilla ones, and the framework binds behavior by name** (what regional-combat 0.3.x does): simple to build and already proven, but it cannot be used together with other mods that override the same file.
- **Merge elements into the definition tree after parsing**: this depends on the internal layout of the lexer, the prototype objects and several name tables, and has to be redone at every load or reload.
- **Call the factory at run time to create elements, then attach them to the live window**: the factory exists, but there is no separate "add child element" function, so the live window's internal lists would have to be rewritten directly.

## Consequences

- Interface declarations must be static data the framework can read directly: splicing may happen on LoadingThread, before extensions load, and the hooks cannot call extension code.
- The framework has to decide where injected elements go, and handle layout and duplicate names when several extensions add elements to the same panel.
