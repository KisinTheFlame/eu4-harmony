# A font's page starts small and grows as it fills

A font's page of system glyphs (ADR game-text-in-system-fonts) used to be sized once, as the font loads, because the game keeps what it lays out with glyphs' places in fractions of the page's size. A page that size wastes video memory on characters a game never shows: the seven pages of the Chinese playset at an interface scale of 1.5 came to about 150 MB of textures. Yet a long Chinese game can still need more glyphs than one holds, and those past it showed as '?'. Now a page starts at a quarter of that size and grows: once three quarters of it is claimed, or a glyph finds no room, it doubles at the next frame the main thread presents, taller first, then wider, up to the largest texture the graphics card takes. Its glyphs are copied into the new texture where they were, and what the game kept laid out from the old size is laid out again: the text each font drawing from the page keeps, and for a map font the names on the map. A font keeps one page.

## Considered Options

- **Several pages for a font, a new one added as the last fills**: nothing laid out would need doing again, and no frame would wait for a page to be copied. But the game draws a text from one texture, so all four of its text paths (main text, button text, floating text and map names) would have to split a text by page and draw each part on its own, a far larger change to the game's code than a new size for one page; and growing never runs out in practice, since a page as large as the card takes holds about 300,000 body text glyphs.
- **Sizing the page at load from the characters the localisation files use**: no growth in a game, but counting them would lengthen the startup, and text that comes only as the game runs, such as names the player gives and an extension's texts, would still fall outside it.

## Consequences

- A page grows on the main thread, just before a frame is presented, where no text is half laid out. A glyph that finds no room before then shows as '?' for that frame; a button or tooltip drawn in that frame keeps the '?' until the game draws its text again. Growing before the page is full makes that rare.
- A map font's page that grows outside a game, as at the main menu, has the names on the map built again at the next frame of a game: a game started from the main menu draws the names the main menu built.
- A page may be a render target and copied on the card (`StretchRect`); a device that cannot copy from one texture to another gives every page the size it would have kept, as before.
