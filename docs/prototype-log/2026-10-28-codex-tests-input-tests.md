# 2026-10-28 Tests run by Codex, and input tests

Codex tested from the checklist with computer use, and I checked its results against the logs and screenshots. After that I tested input myself with Unicode key events (SendInput).

- **Passed**:
  - startup: 198 dependency points;
  - the two test notices show and close one after the other;
  - the corner button appears only on the main menu's front page;
  - the settings window's Save and Esc to cancel, with changes taking effect after a restart; the framework UI switches to the chosen font;
  - Chinese in the top bar, tooltips, the province window, the army panel and regional-combat's tooltips;
  - curved country names (Ming, Korea, Mongolia, the Timurids and others);
  - save names in legacy escapes;
  - all four game languages start up to the main menu.
- **Input (tested by me)**, in the "Find" box on the country selection screen:
  - typing "朝鲜" (Korea): whole characters are inserted, and the search finds 朝鲜, 朝鲜西部, 朝鲜湾 (Korea, western Korea, Korea Bay) and more;
  - "朝鲜abc", then Left three times and Backspace, gives "朝abc"; Home then Delete gives "abc";
  - Ctrl+V pastes "中文测试" ("Chinese test") correctly;
  - with "abc" selected, copy and cut both work.
- **Could not verify**:
  - selecting Chinese with Shift and the arrow keys, by dragging with the mouse or by double-clicking, then copying: the injected keys and mouse made no selection and the copy came out empty; this needs another try with a real keyboard;
  - where the IME candidate window appears;
  - saving and loading with Chinese save names;
  - accented letters in French, German and Spanish: the Chinese language mod keeps the menus in Chinese, so I did not see them.
- **To fix**:
  - the font drop-down list in the settings window is taller than the screen and cannot scroll;
  - line breaking in main text: paragraphs such as the start-of-game introduction have very short lines, with full stops at the start of a line; pinyin inside Chinese text is cut in the middle (Yu / anzhang). The cause is still being looked into; I checked the logic of the layout loop itself and found nothing wrong.
  - in one game, the game's `error.log` got 7951 `Could not find coloring for character` lines:
    - in 5820 of them the character is the byte 0xB0, which suggests some path takes the UTF-8 continuation byte 0xA7 for a color code;
    - 2131 are `I`, from color codes in the interface definitions themselves, unrelated to the framework.
  - the two truncating functions at vtable `+0x70` and `+0x80` (which measure the width, then add " ...") still measure byte by byte, and may cut a character in half.
- **Observation**: on "Back" from the country selection screen, the game restarts itself as a child process (vanilla behavior, unrelated to the framework).
