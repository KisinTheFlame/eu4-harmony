# 2026-10-28 Edit box keys and the IME (fixed after the user tested with a real keyboard)

The user tested in the "Find" box on the country selection screen and found three problems, all now fixed. After the fixes I tested again with injected keys and real Microsoft Pinyin composition:

- **Backspace**: after typing "朝鲜", two Backspaces used to leave "œ" (0x9C, the second byte of "朝"); now the box is empty.
- **Home, End**: pressed alone they used to select as well (a vanilla bug, see [research/text-input.md](../research/text-input.md)); now they only move.
- **Shift+arrow keys**: the edit box used not to handle them, so the keys went to the map and moved the view (vanilla never had this feature). Now:
  - Shift+Left selects "鲜abc", and copying gives "鲜abc";
  - Home, then Shift+Right twice, selects "朝鲜", and copying gives "朝鲜";
  - the map does not move.
- **IME window**: it used to appear at the top left of the screen (I took the caret's coordinates relative to its parent window for screen coordinates). Now the composition string appears at the start of the edit box's text, with the candidate window right below it.
  - For the retest I switched the game window to Microsoft Pinyin for a while with `WM_INPUTLANGCHANGEREQUEST`, and switched it back to English afterwards.
