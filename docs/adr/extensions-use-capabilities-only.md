# Extensions use only the capabilities the framework provides

Extensions reach the game only through the capabilities the framework provides: game objects, commands, interface, events, per-save data. Symbols, hooks, memory reads and writes, and native calls are used only inside the framework and are not open to extensions. So extensions depend on no address or data layout, and a new baseline needs only the framework updated. The framework manages all hooks in one place, so extensions cannot break each other by each patching the game on its own.

## Considered Options

- **Open up patching, as Harmony does**: mod authors would not have to wait for the framework to add capabilities, but every mod would depend on addresses and break one by one with each new baseline, and mods hooking the same function would each have to deal with the conflicts.

## Consequences

- The framework carries all the reverse engineering and is also the bottleneck: a capability an extension needs and the framework lacks can only be added to the framework.
- The framework does not grow special capabilities for one mod's particular needs; logic specific to a mod stays in its own extension.
- Extensions are native DLLs and can technically still hook on their own; that use is unsupported, and the framework takes no responsibility for the conflicts it causes.
