# Unicode module and framework UI plan

Status: final (2026-10-27). Terms are in [GLOSSARY.md](../GLOSSARY.md). The decisions and their reasons are in [ADR 0010](adr/0010-built-in-unicode-module.md) (the Unicode module), [ADR 0011](adr/0011-version-proxy-injection.md) (the version.dll entry), [ADR 0012](adr/0012-framework-succeeds-or-fails-as-a-whole.md) (succeeding or failing as a whole) and [ADR 0013](adr/0013-framework-draws-its-own-ui.md) (framework UI).

## Goal and scope

- **Goal**: the framework itself takes charge of text in the game, replacing the CJK font plugin. With a Chinese language mod installed, text looks as it did before; without one, as in vanilla. Mods and extensions write only plain UTF-8.
- **In scope**: display (interface and map); compatibility with legacy escapes (localisation, GUI and scripts, old saves); Chinese input; Chinese search in the diplomacy country list and the province search; complex text layout (last); framework UI (notice window, settings window).
- **Out of scope**: the IME itself; drawing uncommitted pinyin in the edit box; undo, redo, Ctrl+A to select all; pinyin search and searching across simplified and traditional characters; Japanese, Korean; multiplayer; save compatibility in the other direction (back to the CJK font plugin); text capabilities for extensions; opening the framework UI to extensions; ordering names by the naming customs of a culture.
- **Platform**: Windows 10 1903 or later (it relies on the system's `icu.dll`); the baseline is still 1.37.5.
- **Implementation**: in Rust. A jump from the middle of a function goes to a stub written as a Rust naked function, placed next to the handler it calls. Every address used and the bytes expected there are first checked against eu4.exe or in the game, then recorded in [research/](research/).

## Framework settings

Kept in `eu4-harmony/settings.txt` in the user directory, written the way the game writes its own `settings.txt`. They are changed in the settings window and take effect at the next start.

| Setting (in the file) | Options | Default |
|---|---|---|
| Text appearance (`text_appearance`) | Keep the fonts as they are (bitmap fonts first, with characters they lack filled in from the system font) / System fonts (all text in system fonts) | Keep the fonts as they are |
| Text font (`body_font`) | System fonts installed on this machine that can show Chinese characters | Microsoft YaHei |
| Map font (`map_font`) | Same as above | KaiTi |

Characters neither font has are left to the system's font fallback. The framework UI always uses the text font, whatever "Text appearance" is set to.

## Milestones

| Milestone | Content | Done when |
|---|---|---|
| M0 Infrastructure | Dependency points declared in one place (location by RVA or export name; leave unchanged, replace the entry, jump from the middle, rewrite bytes); all-or-nothing startup, undone on failure; stubs for jumps from the middle of a function; all existing hooks moved over; bindings to ICU | The game behaves as before, the startup report lists the check result for every dependency point, and all existing in-game checks pass |
| M1a Chinese in the main interface | The entry moves to `version.dll`; localisation and GUI/scripts are read as UTF-8; glyph table (code points above 0xFF, with temporary glyphs); bitmap fonts first, system fonts for missing characters (atlas, textures, device reset); walking and line breaking of main text; remove the framework's conversion code, and decide the player language from Windows | A Chinese mod with UTF-8 localisation shows Chinese in the main interface; framework notices and mod texts show in Chinese |
| UI | Drawing on top of the game's picture; capturing input; widgets (window, text, button, drop-down list); moving the notice window over; the settings window and the button in the main menu's corner; the framework settings file; removing the spliced notice window | Notices show and close correctly in the main menu and in the game; a changed setting takes effect at the next start |
| M2 Compatibility with old Chinese language mods | Legacy escapes in localisation, GUI/scripts and old saves (text and binary) become UTF-8 as they are read | Existing Chinese language mods show without conversion; old saves played with the CJK font plugin load, with the right names |
| M1b The rest of the interface | Walking and wrapping text in buttons, tooltips, lists and so on | The whole interface shows Chinese, with no garbled text, and nothing cut off or out of place |
| M1c Map text | Laying country and province names along curves, and aligning them | Chinese names on the map look right |
| M3 Input | Committing text; moving, backspacing and selecting by character; pasting; telling the system where the caret is, so the IME window appears beside the edit box; Chinese save names | Every edit box takes Chinese; saves with Chinese names save and load, with their extension save data matched to them |
| M4 Search | The diplomacy country list and the province search match by UTF-8, ignoring case and accents in Latin letters | Searching in Chinese finds results; French names are found with unaccented letters too |
| M5 Complex layout | Laying out whole paragraphs, bidirectional text, handed to the glyph table as temporary glyphs | Arabic and Devanagari in a test mod display correctly |

### Progress (2026-10-28)

- **Seen working in the game** (details in [prototype-log.md](prototype-log.md)):
  - M1a, M1b:
    - the Chinese interface in the main menu and in the game: buttons, tooltips, the outliner, events;
    - paragraph line breaking (scenario introductions, start-of-game introductions, the country history window; fixed-size multi-line text boxes, such as the description on the macro builder's diplomacy page, break lines between Chinese characters through vtable `+0x80`);
    - names cut short with "..." are cut between characters.
  - M1c: country, province and sea names on the map (this first needed the 16 MiB texture limit raised, see [research/font-textures.md](research/font-textures.md)).
  - M2: the Chinese language mod shows without conversion; save names written by the CJK font plugin.
  - M3 ([research/text-input.md](research/text-input.md)):
    - typing, moving and deleting by character, Home/End, selecting with Shift+Left/Right, copy, cut, paste;
    - the IME composition string and candidate window appear at the caret;
    - saves with Chinese names save and load, and the extension's save data loads back with them.
  - M4: searching in Chinese finds results on the country selection screen and in the province search.
  - System fonts (`unicode/fill.rs`):
    - With "Keep the fonts as they are", a font page grows by 1024 rows downward, and characters a bitmap font lacks come from the system font; the Chinese language mod's DXT5 pages stay DXT5, with new glyphs encoded and written as blocks (the ç in a Breton ruler's name is correct).
    - With "System fonts" chosen, every page is replaced by a new one, and all text uses the system fonts (the main menu, the country selection screen, and in a game the start-of-game introduction, the outliner and the top bar are all fine; map names are in KaiTi and stay black).
  - Framework UI: the notice window, the corner button that appears only on the main menu's front page, the settings window (its drop-down lists scroll).
- **Not verified in the game**:
  - M4's "found with unaccented Latin letters too": the Chinese language mod turns all place names into Chinese, so the game has no accented place name to search for; unit tests cover it.
  - Floating text on the map (font vtable `+0x48`): on 2026-10-28 a call trace showed it was not called once in the main menu, the lobby, the start of a game or several months of game time. It is called only through two vtables; the only related name in the game is `floating_chat_text`, possibly chat messages in multiplayer, which cannot be verified in single player.
- **Not for now**: M5 complex layout (decided on 2026-10-28 to leave it for now; the glyph table still supports temporary glyphs, so it can be connected when the work is done later).

## Acceptance

- Logic that does not depend on the game (walking UTF-8, decoding legacy escapes, line breaking, editing by character, search matching, the atlas) gets unit tests.
- Each milestone is tested in the game against a checklist, and the results go into [prototype-log.md](prototype-log.md).
- For each milestone, the main interface is opened once in each of the four game languages, English, French, German and Spanish, to confirm it does not differ from vanilla.
- All dependency points can be checked against an installed eu4.exe without starting the game: `EU4_GAME_DIR=<game dir> cargo test -p eu4-harmony-core -- --ignored`.

### M0 checklist

M0 does not change how the game behaves, so the checklist is a retest of what already works. Install with `cargo build --release` and `tools/dev-install.ps1`, and enable regional-combat and a mod made by `eu4-harmony new` in the playset. Tested and passed on 2026-10-27; the results are in [prototype-log.md](prototype-log.md).

1. **Startup report**: `report.txt` has the line `startup: baseline 1.37.5: 58 dependency points match; 34 hooks, 0 writes`.
2. **Main menu**: the game reaches the main menu as usual, with no new errors in `error.log`.
3. **Army panel**: select an army and the button area appears; left and right clicks on the buttons both work (regional-combat's left click turns region mode on and off, and its right click opens the radius box; its log records the effects, not the clicks), and tooltips are fine.
4. **Regional combat**: the region button and the radius box work as before, with Chinese showing as it did; once on, it decides daily and issues move and stop orders; map coloring is correct when the radius changes.
5. **Saves**: after an autosave, a data file is written under `eu4-harmony/saves/`; load that save and regional-combat restores its earlier state.
6. **Rebuild**: go back to the main menu and into a game again; the army panel's buttons and the notice window both appear again.
7. **Startup failure**: set `EU4HARMONY_FAIL_STARTUP=1` on the command line and run `eu4.exe` directly. A message box saying "EU4 Harmony could not start" should pop up; `report.txt` says `startup FAILED`; the game runs as usual, but none of the mods' buttons appear.
8. **Four languages**: set the game language to English, French, German and Spanish in turn; the main interface looks as in vanilla.
