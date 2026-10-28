---
status: superseded by ADR-0011
---

# Inject into the game process through a winmm.dll proxy

The framework gets into the eu4 process through a `winmm.dll` proxy in the game's root folder; `eu4.exe` on disk stays as it is. When the Windows loader resolves eu4.exe's import table, it looks in the exe's own folder first for DLLs that are not in KnownDLLs. The proxy forwards the 15 `waveOut*` / `waveIn*` functions eu4 uses to the winmm in System32, and starts the framework inside the process.

## Considered Options

- **Replace `eu4.exe` with a launcher** (what eu4-regional-combat 0.3.x does): verifying the files in Steam restores the entry; the Paradox launcher's arguments have to be passed on; it takes the `eu4.exe` file name, so it cannot be used together with other modified exes.
- **Patch the exe statically**: of several patches only one can be used, the whole Paradox exe has to be distributed, and the work has to be redone for every game version.
- **`version.dll` / `d3d9.dll`**: eu4 imports these statically too, but the CJK font plugin already uses them.
- **`bcrypt` / `crypt32` / `d3dx9_43` / `opengl32`**: these sit on the cryptography or rendering path, every import of the same name in the process would go through the proxy, and they have far more exports.

## Consequences

- `DllMain` runs under the loader lock, so it does as little as possible; the real initialization waits for a hooked early game function.
- Someone else's `winmm.dll` already in the root folder is not overwritten: the installation instructions say to move it into `eu4-harmony/` first, and the proxy chain-loads it.
- This relies on eu4.exe importing `WINMM.dll` statically; if a game update drops that import, a different entry is needed.
