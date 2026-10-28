# Extension save data lives outside save files

The framework writes each extension's save data to external files in the user folder, tied to the save by the save's identity (full SHA-256, save path, game date), and writes nothing into the `.eu4` save. So the save format stays the same, and saves still load after going back to vanilla or disabling the mod. The cost is that when a save is copied, renamed or moved, the external files do not follow it automatically; when the identity does not match, the extension gets empty data.

## Considered Options

- **Write it into the `.eu4` save**: the data travels with the save, but the save format changes, and whether vanilla and other tools can still read it would all have to be verified again.
