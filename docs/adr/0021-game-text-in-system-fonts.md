# The game's text is drawn in system fonts

Every font the game draws text with gets a new page whose glyphs are all drawn from system fonts, for every player and in every language: body text, headings and names on the map in the fonts and weights chosen in the framework settings (Microsoft YaHei, Microsoft YaHei and KaiTi by default). The fonts' own glyphs are replaced; a font's own page is read only for the color of its ink and the outline around its letters, which names on the map keep. There is no setting to keep the fonts' own glyphs. This supersedes the glyphs part of ADR 0010, where bitmap fonts came first by default, a character a bitmap font lacked was filled in from a system font, and the "Text appearance" setting could switch everything to system fonts.

## Considered Options

- **Keep the setting, bitmap fonts first by default (ADR 0010)**: text looks as the fonts' makers drew it, but every font has two ways of being drawn to keep working and to try in the game; a Chinese language mod's compressed pages need new glyphs encoded as DXT5 blocks; and characters filled in from a system font sit among the bitmap font's own in another style. A bitmap font's page is also stretched by the interface scale and blurred, where system glyphs are drawn as dense as the screen.
- **Keep the setting, system fonts by default**: the second way of drawing would still have to be kept working and tried in the game, for players who switch back.

## Consequences

- Without a Chinese language mod the interface no longer looks like vanilla: Latin text too is drawn in the body text and heading fonts. ADR 0010 turned down redrawing the vanilla fonts for this reason; the player can choose the fonts in the settings.
- Interface text reaches the window a texel to a pixel at an interface scale above 1, for every player.
- A settings file's `text_appearance` line is ignored, and dropped the next time the settings are saved.
- The glyphs above 0xFF that a font's file brings are still read as it loads: a font the framework cannot give a page of system glyphs (on a device that is not Direct3D 9Ex, for one) draws them as before.
- Memory and the time to start stay as they were with system fonts, which the last measurements put level with bitmap fonts ([performance.md](../performance.md)).
