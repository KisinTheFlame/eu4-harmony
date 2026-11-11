# The framework ships on the Steam Workshop, its core loaded from its own item

The framework is a Steam Workshop item of its own, Harmony. It holds `version.dll` and the core. The player copies `version.dll` into the game's root folder once; the proxy then loads the core from Harmony's own Workshop item, which Steam keeps up to date, after the core a command line names and the one beside `version.dll`. The proxy knows Harmony's item by its published file id, built into it, and looks into no other mod's folder. The item holds no executable: the command line tool is not part of a player's install. This supersedes the line of [plans/v1.md](../plans/v1.md) that the framework itself is not distributed or updated through the Steam Workshop.

## Considered Options

- **An archive the player copies into the game folder, as v1 had it**: every update means downloading and copying again, and a player who forgets runs an old framework against mods that need a new one. Players already get mods from the Workshop.
- **A Workshop item that only carries the files, all copied into the game folder**: as many copies as there are updates.
- **Finding the core in any subscribed item that holds a file of its name**: no published file id to build in, but any mod on the Workshop could ship a core of its own, and the proxy would run it with every right in the game, which the Lua sandbox (ADR extensions-are-lua-in-a-sandbox) exists to prevent.
- **Enabling Harmony in the playset to turn it on**: a switch players know, but the proxy would have to read the launcher's files before the core is loaded, and turning it off is already unsubscribing.
- **An installer, a script in the item that copies `version.dll`**: scripts trip SmartScreen and virus scanners, may not survive the upload, and still need the player to find the Workshop folder first.

## Consequences

- Updates to the core arrive with Steam's; `version.dll` itself is not updated, so the proxy and the core check a version of what passes between them, and a mismatch tells the player to copy `version.dll` again.
- The published file id exists only once the item is created; the proxy is built with it from then on.
- A core beside `version.dll` wins over the Workshop's, so a developer's install still runs what they built.
- Unsubscribing leaves the game unmodified, with an inert `version.dll` in the root folder.
