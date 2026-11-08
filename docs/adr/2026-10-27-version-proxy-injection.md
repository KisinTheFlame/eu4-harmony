# Inject into the game process through a version.dll proxy

The framework now gets into the eu4 process through a `version.dll` proxy in the game's root folder, replacing `winmm.dll` (ADR winmm-proxy-injection); `eu4.exe` is still left untouched. eu4.exe imports `VERSION.dll` statically, and other native mods are loaded through it too. Now that the framework has its own Unicode module (ADR built-in-unicode-module), taking this entry keeps them from being loaded at all: only the framework changes text-related code in the process, and there is no need to handle another side by side.

A `version.dll` already in the root folder is moved to `eu4-harmony/backup/` as a backup, as the installation instructions say. The proxy does not load it, and it goes back where it was on uninstall. The switch is made before the Unicode module's first text hook is enabled.

## Considered Options

- **Keep winmm, and refuse to run when another mod that changes the game's text is detected**: players would have to remove it themselves; and both change the same text code, so loading both at once means garbage or a crash.
- **Chain-load the existing version.dll** (what the winmm proxy does with an existing `winmm.dll`): this would bring in whatever it loads, which is exactly what is to be avoided.

## Consequences

- Other plugins that are also loaded through `version.dll` are no longer loaded either.
- The winmm proxy was never distributed, so players need no migration; the development install removed the `winmm.dll` from development machines at the time.
- The conclusions of ADR winmm-proxy-injection that do not concern the entry still stand: `DllMain` runs under the loader lock and does as little as possible.
