# 2026-10-28 Shared glyph sets (tested by me)

Fonts that bring the same glyphs above 0xFF now share one set. Chinese playset, to the lobby only, twice with each look; framework settings put back afterwards.

- **Memory**: 994,216 glyph records kept as 6 shared sets of 115,053; the framework's heap at the main menu 3.2 MB instead of 66 MB, the process 3761 and 3763 MB instead of 3866 with bitmap fonts, 3767 and 3746 MB with system fonts.
- **Display**: in both looks the lobby's text and the East Asia map's names show as before, and "한글テスト" typed into the Find box shows, so glyphs from the shared sets and glyphs drawn later are both found.
- **Tools**: the first run clicked as soon as the log said the main menu was up, before it took clicks, and stayed on the menu; waiting 8 s first fixed it.
