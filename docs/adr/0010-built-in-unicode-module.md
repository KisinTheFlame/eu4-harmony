---
status: superseded in part by ADR-0021
---

# A built-in Unicode module replaces the CJK font plugin

The framework has a built-in Unicode module: text in the game is always kept, shown and typed as UTF-8. The module is always on, for every player, and replaces the CJK font plugin. While relying on the plugin, the framework had to encode Chinese characters by the plugin's escape rules, full-width punctuation could only be swapped for half-width, characters outside the ideograph range turned into `?`, and players without the plugin saw garbage. With the framework itself in charge of text, mods and extensions write only plain UTF-8, and any character can be shown.

**Scope**:
- Display, including interface text and the country and province names on the map.
- Legacy escape compatibility: legacy escapes in a Chinese language mod's localisation, GUI and script files and in old saves are turned into UTF-8 as they are read. This knowledge lives in one place only; once read in, the game holds only UTF-8.
- Chinese input: take the text the system IME commits; move, backspace and select by character; support pasting and Chinese save names. The IME itself is not part of it.
- Chinese search in the diplomacy country list and the province search; no pinyin, and no matching between Simplified and Traditional characters.
- Complex script layout (joined Arabic, right-to-left, Devanagari) comes last.

**Glyphs**: by default bitmap fonts come first. Characters a bitmap font has are drawn as they are; characters it lacks are filled in at run time from a system font. With a Chinese language mod installed, text looks as it did before; without one, it looks like vanilla. The "Text appearance" option in the framework settings can switch everything to system fonts. One system font is chosen for body text and one for the map, by default Microsoft YaHei and KaiTi; characters neither has are left to the system's font fallback.

## Considered Options

- **Keep relying on the CJK font plugin**: it handles only the ideograph range, punctuation and other characters are lost, and players without the plugin see garbage; the framework would also have to keep maintaining its escape rules.
- **Step aside when another text plugin is detected**: the framework would have to maintain several encodings at once, and what can be shown would depend on what the player has installed.
- **Ship pre-generated bitmap glyph sets with the framework**: simple at run time, but the character set is fixed, the files are large, and every font size needs its own set.
- **Regenerate the whole set of vanilla fonts from system fonts**: all text would share one style, but the module is always on, so English players' interface would change its look too.

## Consequences

- Line breaking, graphemes and case folding rely on the `icu.dll` that comes with Windows, so the minimum system is Windows 10 1903.
- In saves played with it, names the player typed or changed are UTF-8. After removing the framework and going back to the CJK font plugin, those names show as garbage; nothing is done for compatibility in that direction. Multiplayer was never supported anyway.
- The player language no longer looks at the font plugin: Simplified Chinese if the Windows display language is Chinese, otherwise the language in the game's settings. This supersedes ADR 0009's rule for deciding it. A player whose Windows is in Chinese but who has no Chinese language mod sees the framework's and mods' texts in Chinese inside an English interface.
- The framework no longer transcodes: mods' keys are spliced into the localisation file unchanged as UTF-8, and extensions' texts are handed to the game unchanged. This supersedes the "transcode" part of ADR 0009. Encoding no longer stands in the way of Japanese and Korean, but the language list is not extended for now.
- The Unicode module is invisible to extensions and offers them no text-related capabilities.
- The glyph table supports, from the start, temporary glyphs that are valid for one draw only, so complex layout can be added later without changing it.
- The Unicode module is a foundation of the framework: if it fails to hook in, the framework has failed to start; see ADR 0012.
