# Vanilla languages default to the game's original bitmap fonts

In the four languages the game ships with (`english`, `french`, `german` and `spanish`), body text, headings and names on the map default to the game's own bitmap fonts rather than system fonts. When a role uses the game's original bitmap font, Harmony skips replacing its texture page and its glyph table altogether: the game draws its own `.fnt` and texture page (or a mod's bitmap font that replaces them), and the map's shader is left unchanged rather than drawing distance fields. Characters outside Windows-1252 are not filled in from system fonts on those pages.

This applies only to body text, headings and the map in the four vanilla languages:

- The game console keeps its framework font (`Consolas` by default) in every language.
- The framework UI draws with DirectWrite and falls back to `Segoe UI` when body text uses the game's original bitmap font.
- In the settings window, each of the three roles in those four languages offers "Game original" at the top of its font list, distinct from Simplified Chinese's bundled "Default" fonts. While "Game original" is selected, the role's weight list and live preview strip show `—` in a disabled state, and for the map the "Outline the names on the map" toggle is disabled as well; switching to any system font, switching to a non-vanilla language, or resetting defaults updates them live.
- In `settings.txt`, omitting a role's font continues to mean "follow the player language": no new sentinel value is written, and any system font a player already chose explicitly is preserved.
- If a third-party mod shares one texture wrapper across fonts whose roles mix the game's original bitmap font and a system font, whichever font loaded the wrapper first sets the wrapper's mode so later fonts never sample the wrong page layout.

This supersedes the parts of ADR game-text-in-system-fonts and ADR player-language-in-the-settings that replaced the game's own bitmap fonts with `Segoe UI` and `Georgia` by default in English, French, German and Spanish.

## Considered Options

- **Keep replacing every font with system fonts, bundling open-source Garamond and Trajan lookalikes**: keeps a single rendering path with high-DPI interface sharpness, map distance fields and full Unicode fallback, but still alters vanilla's exact appearance and overrides players' own bitmap font mods.
- **Keep bitmap fonts and fill missing Unicode glyphs into them at runtime (ADR built-in-unicode-module)**: preserves vanilla glyphs while showing arbitrary Unicode, but reintroduces DXT5 re-encoding, mismatched style and resolution between bitmap and system glyphs, and two intertwined page lifecycles.
- **Offer "Game original" in every language**: lets players of non-vanilla languages keep legacy bitmap language mods, at the cost of exposing a broken option to players without such a mod whose language needs characters outside Windows-1252.

## Consequences

- Players of English, French, German and Spanish who install Harmony see the game's original appearance and any bitmap font mods they use, without having to configure anything.
- On roles left on "Game original", characters outside Windows-1252 in game text draw as `?`, interface scaling above `1.0` stretches the game's bitmap pages as vanilla does, and map names do not use distance fields; switching a role to any system font restores all three.
- When a system font chosen in settings is later uninstalled in a vanilla language, the fallback is the game's original bitmap font and the missing-font notice names "Game original".
- Scenarios that test system font page growth and map-name rebuilds (such as `font-growth`) explicitly configure system fonts instead of relying on English's defaults.
