# 2026-10-27 Third test (notice window)

Two test mods whose manifests are wrong on purpose (missing `api_version`) were added to the playset, to trigger two notices:

- `core.log` recorded both notices, and on reaching the main menu (about 32 seconds after start) bound the notice window's 4 elements to the main menu.
- The user confirmed the notice window displays correctly and shows the next notice after one is closed.
- Conclusion: item 9 passes (the texture is read from `eu4-harmony/gfx/`); the main menu part of item 10 passes.
- This run did not go into a game, so the display in the top bar and binding again after the host window is rebuilt are both not tested.
- The user pointed out the texts were English only, so localisation was added (ADR localisation-in-the-player-language).
