# The framework mounts files of its own into the game's file system

The framework has files of its own that the game reads as it reads its own and the mods': framework files. They are made in memory at startup, packed as a zip archive and mounted into PhysFS, the file system the game reads every script, interface and graphics file through, as the game initializes it, after every place already there. Their names are the framework's, beginning `eu4harmony`, so they add files and never stand in for a game's or a mod's. The first are the framework fonts: for each, a definition in `interface/eu4harmony_fonts.gfx`, a `.fnt` file with its lines and no characters, and a small empty page, which the Unicode module replaces with a page of system glyphs (ADR game-text-in-system-fonts). The game console draws in the first of them, in a system font of its own.

## Considered Options

- **Edit the game's files as they are read** (`splice`, as mods' UI and texts are added, ADR inject-ui-at-runtime): a font of the framework's would be a copy of a game font put into whichever file defines that font, `interface/core.gfx` in vanilla and in the Chinese language mods. It depends on a font of that name being in a file of that name, and a copy loads the same page as its original, which draws in one system font, so the copy needs a path written otherwise (a leading `/`) to get a page of its own, a side effect of how the game names its textures. Tried in the game on 2026-11-03 ([prototype log](../prototype-log.md)), and given up for this.
- **Change the font of the console's text boxes as the game builds them**: no file changes, but the console's window and its lines would each need a hook where they are built, and the layout of their text boxes as dependency points; the lines are built again as they scroll.
- **Install the files into the game folder or a mod**: the player's folders would hold files of the framework's that must be kept in step with it and removed with it, and a mod would have to be in the playset.

## Consequences

- The game reads the framework files only if PhysFS mounts the archive. A failed mount is logged, the framework fonts are not defined, and the windows that would draw in them keep the game's fonts: a window is given a framework font only once that font has its page of system glyphs.
- Which windows draw in which framework font is one table (`unicode/framework_fonts.rs`); the `font` of each is changed in whichever `.gui` file under `interface/` defines it, the game's or a mod's.
- `PHYSFS_mountMemory` is a dependency point. The archive lives in memory as long as the game runs.
- Framework files are under `interface/` and `gfx/`, which the game does not take into its checksum of the game's files (the checksum shown in the main menu stays the same).
