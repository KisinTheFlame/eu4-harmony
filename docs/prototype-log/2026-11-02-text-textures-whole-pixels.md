# 2026-11-02 Text textures on whole pixels (tested by me)

The texture button and tooltip text is drawn into, shown a texel to a pixel at its own size with its edges on pixel edges (`unicode/pixels.rs`, the matrix changed at 0x1551a73). Two runs of `eu4.exe`, started directly, as in the run before: the player's playset and framework settings, 3840x2160, the main menu, `menu.start MNG`, the mouse on the top bar's treasury, screenshots through `eu4-harmony game screenshot`. Backed up with `player-state.ps1` first; the interface scale was 1.5 for the first run and 1.6 for the second.

- **Startup**: 598 dependency points match; 106 hooks, 8 writes. The tooltip showed whole and in place at both scales.
- **The line "这是你可以用于征募军队"**, each character fitted to the system font's glyph at the page's size as in the run before, free to sit between pixels:
  - at 1.5: before (that run's screenshot), all eleven at (0.75, 0.75) between pixels, root mean square difference 24.4 on average when held on whole pixels; now all on whole pixels, 4.3 on average (3.4 to 4.8);
  - at 1.6: before, at 0.30 to 0.50 across, falling along the line as the texture was shown smaller than it is, and 0.90 down, 29.5 on whole pixels; now all on whole pixels (one at 0.05), 5.6 on average (3.5 to 9.8).
  - So the matrix the game copies at 0x1551a83 is what the shader uses, and it is a plain 2D projection with GUI y growing upward, as main text's is.
- **Not seen in the running game**: buttons' text compared with the glyphs, a texture whose mesh is cut or turned, a button's text with more than one frame, an interface scale of 1, powers of two.
- **Cleanup**: each game was closed by ending its process; `player-state.ps1 -Restore` put every file back (the interface scale back to 1.5) and removed the backup. No crash folder was made. This branch's build was left installed.
