# Extension save data is written into the save file

The framework writes the extensions' save data into the save itself: one more top-level entry of the save's game state, `eu4_harmony`, written by the game's own writer in the same pass as the rest of the save, after the game state's last entry, before the AI that a plain save holds (the game reads a plain save's entries only up to it), and before the checksum. When a save is loaded, the framework takes the entry out before the game's handling of top-level entries sees it. The data goes wherever the save goes: copied, renamed or moved, into the game's backup copy, through rotated autosaves and into the cloud. Nothing gathers in the user folder. This supersedes ADR 0007.

The checksum the game writes at the end of the save covers the entry, so a save keeps its ironman status and its achievement eligibility. When no extension has data, nothing is written. Data is kept only for the extensions loaded when the save is loaded.

Inside the entry is a format version, then the data: one JSON object of extension id to that extension's value, base64-encoded and cut into quoted strings of at most 500 bytes.

## Considered Options

- **Keep the external files (ADR 0007)**: the data did not follow a save that was copied, renamed or moved, nor the game's backup copy, rotated autosaves or cloud saves. The files gathered in the user folder and were never removed. Tying a file to its save took observing the whole save pipeline, down to the final move of the file.
- **Change the file after the game wrote it, or add an entry to its zip**: the checksum no longer matches what the game reads back, which turns achievements off for that save.
- **One long string, read back without the game's reader**: the game's reader returns an empty string for any string of 512 bytes or more. Reading the data back would then take a parser of our own for both save forms, and more dependency points. Chunks of 500 bytes add less than 1%.
- **JSON in quoted strings, without base64**: the game escapes `\` and `"` in what it writes, and a chunk boundary would split escapes and UTF-8 sequences. Base64 is ASCII that nothing on the way changes.

## Consequences

- A save written with the framework is not promised to load without it. By the game's code (not tried in the game), vanilla skips the unknown entry, writes one line to `error.log`, and skips a fix-up it runs only after reading a save without errors.
- Data in the external files of ADR 0007 is not carried over. Files left in `eu4-harmony/saves/` can be deleted.
- An extension's data is at most 16 MiB of JSON; `set_save_data` refuses more (`InvalidArgument`). Every save carries all of the data, about 4/3 of its size after base64, and saving, loading and the checksum take longer in proportion.
- Data a save holds for an extension that is not loaded is gone after the next save. An extension that fails to load loses its data in the saves written meanwhile.
- The data is written whenever the game writes its world, including the saves it makes for multiplayer hot-join and for debugging. The framework does not support multiplayer, and a client without the framework would skip the entry as vanilla does.
- When the data cannot be read, or carries a newer format version, every extension gets none for that save, and the framework log says why.
- Where the data is written and read are dependency points (ADR 0012), described in [research/saves.md](../research/saves.md).
