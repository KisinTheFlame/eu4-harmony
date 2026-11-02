# The manifest is TOML; the rest of a mod's layout is fixed by convention

A Harmony mod's manifest is `eu4-harmony/manifest.toml`, and it holds only what cannot be told from the files: `id`, `api_version`, `description` (a key in the mod's localisation, for the mods window) and `capabilities` (the sensitive capabilities it declares, ADR 0025). Everything else is where it is: the extension is `eu4-harmony/lua/main.lua`, a fragment for a host window is in `eu4-harmony/ui/<host window>/`, and one for its button area in `eu4-harmony/ui/<host window>/buttons/`. Name and version stay in `descriptor.mod`, which the launcher reads, and are not repeated. This supersedes ADR 0003's `manifest.json`; discovery through the playset stands.

## Considered Options

- **Keep `manifest.json`**: one more field to declare for each fragment and the extension's path, in a format that allows no comments and is unforgiving to write by hand, which every mod author now does.
- **Name and version in the manifest too**: two places to keep the same, and the launcher reads only `descriptor.mod`.
- **Declare fragments in the manifest** (what `manifest.json`'s `ui` did): a fragment's `.gui` does not say which window it goes into, so it had to be said somewhere; a folder says it as well and cannot fall out of step with the files.

## Consequences

- A file where the layout has no place for it is recorded in the startup report.
- Fragments are taken in file name order within a mod, and mods in playset order.
- Mods written for `manifest.json` have to move their files; the regional combat mod moves with its rewrite in Lua.
