# Extensions are Lua scripts in a sandbox

Extensions are Lua 5.4 scripts that the framework runs in a Lua state of their own, built into the core, instead of native DLLs exporting a C ABI. A native DLL can do anything in the game process, so a mod from the Steam Workshop could not be opened to players without trusting its author completely (the consequence ADR 0002 left open), and writing one took a Rust toolchain, the Visual Studio build tools and a build. What made native code necessary at first, hooking the game and reading its memory, has since all moved behind capabilities (ADR 0005), so an extension needs nothing a script cannot do. This supersedes ADR 0002; with the C ABI gone, ADR 0014, ADR 0018 and ADR 0019, which govern it, are superseded too.

A state holds Lua's own string, table, math, utf8 and coroutine libraries and the base library without loading files or bytecode, plus the framework's capabilities and globals; `require` reaches only the mod's own Lua files. Extensions share nothing: an error in one, or one that is stopped, leaves the others as they were.

## Considered Options

- **Keep native DLLs beside Lua**, for authors who need more: as long as any mod can load a DLL, the sandbox protects nobody, and the C ABI and its rules of growth would have to be kept as well as the Lua host.
- **The game's own Lua 5.1**, which runs `defines.lua`: it would take dependency points for a Lua state the game owns, its standard library is the game's to configure, and its numbers are doubles, which cannot hold an army's 64-bit ID.
- **LuaJIT or Luau**, which game mods often use: both follow Lua 5.1 and have no 64-bit integers; LuaJIT's speed comes with its FFI, which a sandbox has to take away.
- **Lua 5.5** (released 2025-12-22): its integer arrays take about two thirds of 5.4's memory, at about the same speed, but its editors' tooling and libraries had not caught up, and the version cannot change once mods are written for it without breaking them.
- **WebAssembly**: any language could compile to it, but every author would again need a toolchain and a build, which is half of what this change is for.
- **One Lua state for all extensions**: less memory, but globals would collide and one extension could reach into another's tables.

## Consequences

- An extension costs little to run: a handler call about 40 ns, a capability call 30 to 60 ns, a fresh state 17 KB (measured 2026-11-03). It computes 40 to 110 times slower than Rust, so a heavy search belongs in a daily event, not in every frame; where an extension needs more, the framework adds a coarser general capability, not a way back to native code.
- Mods built on native extensions have to be rewritten; the regional combat mod is rewritten as the acceptance test of this change.
- A Lua integer holds the game's identifiers exactly, and extension save data keeps its 64-bit integers.
- The version a mod targets is `api_version`, the catalogue's (ADR 0026), not an ABI's; the Lua version changes only as a breaking change.
- The SDK's Rust types and its fake game go; the fake game is rebuilt for Lua in the command line tool, on the same checks as the core.
