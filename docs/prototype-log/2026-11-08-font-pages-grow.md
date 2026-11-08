# 2026-11-08 Font pages that start small and grow (tested by me)

Pages of system glyphs that start at a quarter of the size they had and double as they fill ([ADR font-pages-grow](../adr/font-pages-grow.md)). Four runs in test games of my own, one claim for the first three and another for the fourth, each started hidden from the game folder with the player's settings and playset: the Chinese language mod and the unchanged regional-combat extension, 3840x2160 at an interface scale of 1.5, body text in FZPingXianYaSongS-R-GB, headings in FZYaSongS-B-GB, the map in FZShengShiKaiShuS-EB-GB with its outline. The builds were this branch's, with the test games' pipes, after the build, the tests, both clippy runs and the offline checks against `eu4.exe` passed. Each run went to the main menu, then `menu.start MNG`, closed the start-of-game windows, turned the camera to Beijing (`camera.look_at`, province 1816) at a height of 1200, and took screenshots.

## The runs

1. **The first build**: a page grew once three quarters claimed, at the next frame the main thread presented, and the names on the map were built again only when a map font's page grew during a game.
   - The seven pages were made 768 pixels square, the map font's (`tahoma_60`, 78.2 px with a 6-pixel outline) 2560. At the main menu `standard_font`'s page, drawn from by 7 fonts, grew to 768x1536 and `tahoma_60`'s to 2560x5120 (+50.1 MB of private bytes). Starting the game grew `standard_22`'s, 7 fonts, to 768x1536.
   - The main menu, the start-of-game windows, the top bar and the outliner showed every character, no '?'.
   - **The map showed province names but no country names.** The map font's page had grown at the main menu, where nothing built the names again, and the game started from it drew the country names the main menu had built, their UVs fractions of the old size. Province names, which the map mode's refresh builds, were right.
2. **The test switch `grow-pages`**, added for this run (every page asks to grow at the first frame of a game): at that frame all seven pages grew, `tahoma_60`'s from 2560x5120 to 5120x5120 (+100.2 MB), `ToolTip_Font`'s for 22 fonts, and the names on the map were built again in 10.1 ms. Country names, province names and the window text were all right afterwards, the screenshot a few seconds later the same.
3. **The names built again at the next frame of a game** whenever a map font's page has grown since they were last built, wherever it grew. Without the test switch: the same growths as the first run, then "the names on the map built again in 8.8 ms" at the game's first frame, and the country names showed. Then `menu.return` and `menu.start FRA`: Europe's country names, an event's text and a tooltip were right; no page grew again.
4. **A glyph without room drawn once**: the framework's costs at the main menu in the first run counted 10,859 glyphs drawn in 2.1 s, where the day's other builds counted about 4,250. A glyph that found no room was drawn again at every lookup until the page grew, and the names on the map ask for thousands while the game loads, when the main thread presents no frame. Such a glyph now waits for the page to grow. This run counted 4,249 glyphs, in 1.6 s; another test game was starting beside it, and that run took 55 s to the main menu where the others took 38 s. Country and province names showed after the game started, rebuilt in 22.1 ms.

## Numbers

- Two pages growing at the main menu took 0.4 ms (run 3) to 13.3 ms (run 2) together, by the framework's costs; run 4, 6.2 ms.
- The pages at the main menu after run 3's growths: six of 768x768 or 768x1536 and the map font's 2560x5120, about 69 MB of textures, and about 71 MB once `standard_22`'s grew in the game. At the sizes before (six of 1536x1536 and 4864x4864) they came to about 150 MB. These are worked out from the sizes; the process's private bytes at the main menu were not compared with a build from before.

## Not verified

- A page growing as a font loads, when the font's Windows-1252 characters do not fit the first page: the first pages held them in every run.
- A '?' shown for a frame before a page grew, and a button or tooltip keeping it: none was seen in the screenshots, which were taken after the growths.
- A device that cannot copy from one texture to another, which keeps the old sizes: not tried.
- The map's second font (`vic_36_black`) and whether the second set of names on the map holds province names, sea names or both.
- Long games: no game ran time, so no page grew from text a game shows only as it runs.
