# 2026-10-27 M0 test (dependency points declared in one place, all-or-nothing startup)

Against the "M0 checklist" in [unicode-module.md](../unicode-module.md), tested by Codex with computer use, in two rounds; I checked the results against the raw logs. The playset had the Chinese language mod and regional-combat enabled; a new game as the Ottomans, running `eu4.exe` directly.

- **Startup**: every start reported `startup: baseline 1.37.5: 58 dependency points match; 34 hooks, 0 writes`. Localisation, `core.gfx`, `frontend.gui`, `topbar.gui` and `unitpanel_main.gui` were spliced through PhysFS; the main menu, the top bar and the army panel were all bound.
- **Regional combat**: the button and its Chinese tooltip are fine; a left click turns region mode on, a right click opens the radius box, and after the radius is changed to 6 the map coloring grows to match; the daily decision runs every day. After a declaration of war it issues a move order on its own (`command move to Some(151): accepted`), and the army starts moving.
- **Saves**: saved under the default name (the file name is Chinese, `奥斯曼1444_11_11.eu4`, "Ottomans 1444_11_11"); about 0.86 seconds later the extension save data was written, holding the on state and radius 6. Loading this save logged `loaded … with data for 1 extension(s)`; regional-combat restored 1 task, with the radius still 6.
- **Rebuild**: back to the main menu and into a new game; the button appears again, with no binding errors.
- **Startup failure**: started with `EU4HARMONY_FAIL_STARTUP=1` set; not a single hook was installed; a message box popped up saying EU4 Harmony could not start, and the report says `startup FAILED`; the game runs as usual, with no regional-combat button on the army panel.
- **Four languages**: with each of French, German, Spanish and English set, the game started normally, with the same startup line. The Chinese language mod keeps the menus in Chinese, so the display of these languages themselves was not checked. In the French, German and Spanish runs, the game set the language in `settings.txt` back to `l_english`; the framework did not write that.
- **Not covered**: the stop order, the defeat event.
- **Cleanup**: the test save and its extension data are deleted; the 141 existing saves and data files, `settings.txt` and `dlc_load.json` are all byte-for-byte the same as before the test.
