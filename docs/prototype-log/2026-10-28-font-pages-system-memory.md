# 2026-10-28 Font pages written through system memory (tested by me)

The grown pages are now ordinary video-memory textures, written through a small system-memory texture with `UpdateSurface`. Chinese playset, to the lobby only, once with bitmap fonts and once with system fonts; framework settings put back afterwards.

- **Memory and time**: see [performance.md](../performance.md#since-then): 3866 MB at the main menu with bitmap fonts instead of 4353, 3778 with system fonts instead of 3860; system fonts drew their 3,143 glyphs in 28 ms instead of 3.5 s.
- **Display**: the main menu, the lobby's text and every country name on the East Asia map show as before in both looks (bitmap: the Chinese language mod's KaiTi page; system: KaiTi drawn by the framework), so the pages' own glyphs were copied in where they belong.
- **Glyphs drawn while playing**: typing "한글テスト" into the lobby's Find box showed the Korean and the kana in both looks; the Korean is not in the mod's fonts, so it was drawn into the page then, through the new path.
