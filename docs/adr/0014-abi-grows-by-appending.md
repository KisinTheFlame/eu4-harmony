# The ABI grows by appending; its version changes only when something breaks

Every table between the core and an extension starts with its own `size`, and a field is only ever appended; the layout of every table and struct is checked as the `abi` crate compiles, so moving or resizing a field stops the build. Adding a capability, a callback or a field to a struct changes no version: each side uses only what the other's table has.

- **Capabilities** (`GameApi`): the core fills its whole table. Before calling a capability added after a table's first release, the SDK checks that the core's table reaches it, and otherwise returns "unavailable" for that capability alone; an extension built with a newer SDK runs on an older core and loses only the newer capabilities.
- **Callbacks** (`ExtensionVTable`): the core hands the extension its table with every callback empty, and the SDK fills the callbacks that fit in the core's `size`. An extension built with a newer SDK runs on an older core and does not hear the newer events; a callback an older SDK leaves empty is one the extension does not have.

`ABI_VERSION`, and with it `API_VERSION`, its name in manifests, change only when a field that exists moves, goes, changes type or changes meaning. The DLL says which ABI version it was built for (the export `eu4h_extension_abi`), and the core loads an extension only when both that and the manifest's `api_version` are its own; when they are not, the player is told which version the mod was built for and which the framework provides.

## Considered Options

- **Raise the version with every addition** (as the first version design read "during 0.x only an exact `api_version` is loaded"): every new capability would stop every extension until it is rebuilt, although nothing it uses has changed, and the sizes that exist for exactly this would go unused. Additions made so far (the player language and texts) already changed no version.
- **Trust the manifest's `api_version` alone**: `eu4-harmony new` writes it once, so it says what the mod was started with, not what its DLL was built against; a stale one blocks a good DLL, and a fresh one lets a stale DLL through to the handshake, which can only refuse without a reason.
- **Refuse an extension whose callback table is larger than the core's** (what the SDK did): one new event in the SDK would stop extensions on every older core, unlike a new capability, which degrades on its own.

## Consequences

- A capability added later is something an extension must expect to be unavailable, and an event added later one it may not hear.
- A breaking change still stops every extension until it is rebuilt and its manifest updated.
- The first version design's line on the interface version is superseded: an exact match is still required, but of a version that changes only with a breaking change.
