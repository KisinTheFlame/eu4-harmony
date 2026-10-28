# Inject into the game process through a version.dll proxy

The framework now gets into the eu4 process through a `version.dll` proxy in the game's root folder, replacing `winmm.dll` (ADR 0001); `eu4.exe` is still left untouched. eu4.exe imports `VERSION.dll` statically, and the CJK font plugin is loaded through it too. Now that the framework has its own Unicode module (ADR 0010), taking this entry keeps the font plugin from being loaded at all: only the framework changes text-related code in the process, and there is no need to handle the two side by side.

A `version.dll` already in the root folder (usually the font plugin's loader) is moved under `eu4-harmony/` as a backup, as the installation instructions say. The proxy does not load it, and it goes back where it was on uninstall. The plugin files under `plugins/` are no longer loaded and can be deleted. The switch is made before the Unicode module's first text hook is enabled.

## Considered Options

- **Keep winmm, and refuse to run when the font plugin is detected**: players would have to remove the plugin themselves; and both change the same text code, so loading both at once means garbage or a crash.
- **Chain-load the existing version.dll** (what the winmm proxy does with an existing `winmm.dll`): this would bring the font plugin in with it, which is exactly what is to be avoided.

## Consequences

- Other plugins that are also loaded through `version.dll` are no longer loaded either.
- The winmm proxy was never distributed, so players need no migration; the development install script removes the `winmm.dll` on development machines.
- ADR 0001's conclusions that do not concern the entry still stand: `DllMain` runs under the loader lock and does as little as possible.
