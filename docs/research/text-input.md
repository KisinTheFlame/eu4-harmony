# Text input and edit boxes (for the Unicode module)

Baseline 1.37.5 (`9ad3efe1…`). All addresses are RVAs. Everything here comes from static disassembly of eu4.exe; the addresses the code uses also have dependency points that check their bytes. Conclusions marked "to be tested" have not yet been verified in the game.

## 1. From SDL's text event to the edit box

The game's SDL is a version around 2.0.4. eu4.exe exports SDL's functions by name: `SDL_StartTextInput` 0x1735ae0, `SDL_GetClipboardText` 0x1734490, `SDL_SetClipboardText` 0x17357c0, `SDL_free` 0x1735e00.

**Event pump**: `0x1569c70`, slot 1 of the SDL input class (vtable 0x1d95068). Arguments: `rdx` the receiver (kept in `r14`), `r9` the recorder (kept in `r15`). The event is read into `[rbp-0x50]`; a text event's text is at `[rbp-0x44]`.

- The branch for 0x303 (SDL_TEXTINPUT) starts at `0x1569f91`. What it originally did:
  - it looked only at the first character of the text;
  - a two-byte sequence was cut to one wrong byte (`(lead<<6)|(second&0x3f)`, keeping only the low 8 bits);
  - a sequence of three bytes or more was dropped entirely.
- Then it first calls `recorder->vt[0x20](recorder, 0x303, [rbp-0x3c], 0)`, then builds a 0x58-byte "char" event, posts it through `receiver->vt[0x18]`, and goes back to `0x156a22a` for the next event.
- What the char event holds: the character at `+0x10`; 16 bytes from `+0x38`, taken from the constant at `0x1dc1b00`; type 2 at `+0x50`; zero everywhere else. The original code does not initialize `+0x11..+0x13`.
- 0x302 (SDL_TEXTEDITING, the composition string) has no branch and is simply dropped, so the game never shows the characters being composed.
- **What the framework does**: it jumps away at `0x1569f91`, calls the recorder as the original does, then posts one event per character in the game's encoding, with the character's bytes from `+0x10` on and a terminating 0.

**GUI queue**: `0x14e9a70` handles type 2 at `0x14e9c9b`: it calls `vt[8]` of the focus listener (`[[0x23494f0]+0x210]`, that is, edit box + 0x1d0), passing the address of `+0x10`.

**OnChar**: `0x15354e0`(edit box+0x1d0, const char*).

- It looks the lead byte up in two forbidden-character lists: `"§¤£@{}` (only when inner+0xe4 is set) and the edit box's own at `+0x2a0`. A UTF-8 lead byte is never 0xA3, 0xA4 or 0xA7, so both checks work as they are.
- Then at `0x15355f7..0x153560a` it builds a one-byte `std::string` (`rsp+0x20`, length at `rsp+0x30`) and calls `inner->vt[0xa0]` (InsertText).
- **What the framework does**: it replaces this stretch with code that copies the whole character (up to 4 bytes) and sets the length.

## 2. The edit box

The outer edit box's constructor is `0x1533360` (vtable 0x1d91358). The text editor is at **edit box + 0xc8** (called inner below; vtable 0x1d915c0).

| inner offset | Contents |
|---|---|
| +0x30 | Text, `std::string` |
| +0x50 | Column kept while moving up and down |
| +0x54 / +0x56 | Caret column (a **byte** offset within the line) / caret line, u16 |
| +0x60 | Line count, u16 |
| +0x68 / +0x6a | Width / height (pixels), u16 |
| +0x6c | Editable |
| +0x70 | Selected text |
| +0x90 | Has a selection (`0x1539010` reads only this byte) |
| +0x92 / +0x94 | Column / line of the selection's start |
| +0x98 | Font |
| +0xb8 | Forbidden characters |
| +0xd8 / +0xd9 | Enter inserts a line break / multi-line |
| +0xdc | Line limit (-1 for no limit) |
| +0xe0 | Byte limit (default 1,000,000; the edit box constructor sets it to 250) |
| +0xe4 | Formatting characters not allowed |

The outer edit box: `+0x1d0` keyboard listener interface, `+0x1f0` caret sprite, `+0x260` has focus, `+0x261` disabled, `+0x262` password box.

**Methods used**:

| Method | RVA |
|---|---|
| Line and column to index `(inner, line, column) → index` | 0x153aa90 |
| Set the caret by index `(inner, index)` | 0x1536f50 |
| Delete one byte `(inner, index)` | 0x153ab90 |
| Insert text `vt[0xa0]`(inner, std::string*) | 0x1536b80 |
| Set text `vt[0x80]` (clear, then insert) | 0x1536610 |
| The caret's pixel offset in the text `vt[0x58]` | 0x1536060 |
| Select a word to the left `vt[0x128]` (Ctrl+Backspace calls it first, then cuts) | 0x1538a70 |
| Cut `vt[0x178]` / delete the selection `vt[0x188]` | 0x1539090 / 0x1539320 |
| Left / Right | 0x15384d0 / 0x15385a0 |
| Up / Down | 0x15387d0 / 0x1538870 |
| Backspace / Delete | 0x1538db0 / 0x1538f30 |
| Paste / Copy | 0x1539240 / 0x15395b0 |
| Place the caret on a click | 0x15361f0 |
| Multi-line wrapping: how many bytes fit on a line | 0x1537210 |
| Multi-line wrapping: split the text into lines | 0x1537480 |
| Find the last space in the part that fits | 0x1539820 |
| Caret position update (UpdateCaret) | 0x1534250 |

**All of these worked in bytes; the framework makes them work in characters**:

- **Left**: from `0x15384d6` it subtracts 1 from the column. The framework subtracts the byte count of the character before the caret instead; when the column is 0, it takes the original path (`0x15384eb`) to the end of the previous line.
- **Right**: at `0x1538616` it adds 1 to the index, then sets the caret. The framework adds the byte count of the character at the caret instead.
- **Backspace**: at `0x1538e5f` it deletes the one byte before the index, then calls Left. The framework first deletes the bytes of this character that come before its last one and moves the column back by as many, then lets the original code delete the last byte and call Left.
- **Delete**: at `0x1538fbe` it deletes the one byte at the index. The framework first deletes the continuation bytes that follow it in this character.
- **Click**: at `0x153646f` it writes the line and column; the column comes from measuring the width of each byte prefix, so it can fall inside a character. The framework moves it back to the start of that character.
- **Up, Down**: they keep the byte column, which can fall inside a character on the new line. After they return, the framework moves the caret back to the start of that character, then calls `inner->vt[0x208]` (both of them call it at the end).
- **Length cut**: InsertAtCaret (`0x153a979`), Append (`0x153a2e4`) and Prepend (`0x153a6f5`) all use `cmp edx,ecx; cmovle ecx,edx; mov r8d,ecx` to cut the whole text to the limit, which can split a character. The framework cuts before that character instead.
- **Line cut**: after text is inserted (`0x1536b80`), when the line count exceeds `height / line height` (`0x1536e0d` reads +0x6a) or the line limit (`0x1536e37` reads +0xdc), it calls Backspace repeatedly at `0x1536e57`. Backspace already deletes whole characters, so nothing else needs changing here.
- **Multi-line wrapping**: `0x1537210` binary-searches how many bytes fit on a line. The framework moves the result back to a character boundary and keeps at least one character, so that a line never ends up unable to hold anything.
  - The line splitter `0x1537480` first looks for the first space: if there is none, or the first word does not fit, it breaks right at `0x1537210`'s result; otherwise it uses `0x1539820` to find the last space in the part that fits and breaks there. Chinese has no spaces, so a Latin-script name inside Chinese text gets cut apart.
  - The country history window (the "History" button beside the minimap) lays out its text in a hidden multi-line text component (built at `0x1044e97`, its width the window's width minus 0x28), then splits it at line breaks into separate `king_entry_text` entries, so it takes this path too.
  - While text remains to be placed, the framework moves `0x1537210`'s result further back to the last place where a line may break (Unicode rules). With spaces, the result does not change: the place after a space was already a break.

**Key handling**: `0x15366c0`(inner, event) is inner's `vt[0xa8]`. The outer edit box's `vt[0x48]` (0x1534190) hands keys to it while the box has focus; the return value says whether the key was handled. Keys that were not handled go on to other parts of the interface, the map for example.

- In the event, `+0` is the SDL key code and `+8` the modifiers: 0 none, 1 Ctrl, 4 Shift, -1 "any".
- The key table is a chain of "modifiers + key code → method" checks:
  - Left, Right, Up and Down are handled only with no modifiers (and in a single-line box, Up and Down are not handled at all);
  - Home and End, with or without Shift, both call "select to line start / line end" (`vt[0x1d8]`, `vt[0x1e8]`). The "select or not" argument passed in is ignored, so plain Home and End select too. The methods that only move are `vt[0x1e0]` and `vt[0x1f0]`.
  - A key that matches nothing goes on at `0x1536a31` to the checks for Ctrl+C, Ctrl+V, Ctrl+X and so on.
- inner has "select one character to the left" `vt[0xd0]` (0x1538560) and "select one character to the right" `vt[0xe0]` (0x1538670), but the key table never calls them, so vanilla has no Shift+arrow selection of characters.
- **What the framework does**:
  - it changes the two calls for plain Home and End to the methods that only move;
  - at `0x1536a31` it adds Shift+Left and Shift+Right, which call these two selection methods.
- Backspace calls Left after deleting the last byte, meaning to move left by 1 byte. Now that Left moves by whole characters, this one call has to be told separately to move only 1 byte; otherwise, at the end of the text, it skips one character too many (before the 2026-10-28 fix, pressing Backspace twice on "朝鲜" (Korea) left "œ").

**Forbidden-character filter**: `0xb19590`(std::string* text, std::string* forbidden) deletes, byte by byte, the bytes that appear in the forbidden list, and returns whether the length changed.

- InsertText uses it at `0x1536c32` to filter by inner+0xb8; on paste, the font's `0x15a04f0` calls it too.
- 0x15358a0 adds `" § ¤ £ @ { } / \` to 37 edit boxes; the save dialog adds `\ / : * ? < > | \r \n \t` as well.
- Comparing byte by byte deletes continuation bytes whose value is 0xA3, 0xA4 or 0xA7. The framework replaces the whole function with one that compares characters.

**Clipboard**:

- Paste calls `SDL_GetClipboardText` at `0x1539250`, inserts the UTF-8 it returns as it is, and never frees it. The framework converts it to the game's encoding and frees it with `SDL_free`.
- Copy hands the selected bytes to `SDL_SetClipboardText` at `0x1539604`, which expects UTF-8. The framework converts them to UTF-8 first.

## 3. Input method

- The game calls `SDL_StartTextInput` only once, when it creates its input object, and never calls `SDL_SetTextInputRect`. The rectangle worked out when an edit box gains focus (`0x1535250`) is passed to a function that does nothing. So SDL never knows where the caret is.
  - The only caller of `ImmSetCompositionWindow` in SDL is `0x17657c0` (SDL_SetTextInputRect's Windows implementation), so it is never called either.
  - eu4.exe does not import `ImmSetCandidateWindow`; the framework has to get it from imm32.dll itself.
- **IME_HandleMessage**: `0x1764940`(hwnd, msg, wParam, LPARAM* lParam, videodata), with a single call site, `0x173ad09`. When it returns non-zero, the window procedure skips DefWindowProc.
  - WM_IME_SETCONTEXT: `0x1764c7c` clears `*lParam`, so the system's composition and candidate windows are not shown.
  - WM_IME_STARTCOMPOSITION, WM_IME_COMPOSITION: swallowed (it returns 1). The composition result becomes a text event through `SDL_SendKeyboardText`, and the composition string becomes an editing event (which the game drops).
  - WM_IME_NOTIFY: most candidate list notifications are swallowed.
- **TSF's UI-less mode**: at `0x1764e7e`, IME_Init creates an ITfThreadMgrEx with CoCreateInstance, activates it in "UI elements only" mode, and installs UI element callbacks to hide the system's interface. If creation fails, it goes to `0x1764f82`, and the UI-less flag (videodata+0x1560) is 0.
- **What the framework does**:
  - it changes the call at `0x1764e7e` to fail straight away, so UI-less mode is never entered;
  - it wraps IME_HandleMessage: on WM_IME_SETCONTEXT it restores lParam; WM_IME_STARTCOMPOSITION and WM_IME_NOTIFY go to DefWindowProc; WM_IME_COMPOSITION is still handled by SDL as before, and then DefWindowProc is called with an lParam that has the result bits (GCS_RESULT*) removed, so that Windows does not send WM_IME_CHAR a second time.
- **Caret position**: at `0x15343bc` in UpdateCaret, `ebx` is the caret's offset in the text (x in the low 16 bits, y in the high 16 bits), in GUI units.
  - `r15d` and `r14d` are the caret sprite's base point **relative to the edit box's parent window**: the edit box's position in its parent, plus the text inset (`+0x28c`+`+0x240`; for y, `+0x290`+`+0x244`), plus the offset at `[[box+0x1e0]+0x32604]`. It is not a screen position; used as one, the input method's windows end up towards the top left of the screen (tested in the game 2026-10-28).
  - The position on screen: the edit box's `vt[0xd8]` (0x15357a0) gives the edit box's position on screen (in GUI units); its click handler (0x1534560) subtracts it from the mouse position to get coordinates within the box. The caret's screen position = this position + the inset + that offset + the caret offset.
  - Multiplying by the two floats at `[[0x23494b8]+0x4e0]` / `+0x4e4` gives window pixels; the focus function converts the same way. The font's line height is `font[+0x960] * font[+0x968]` (method 0x159ba20).
  - The framework places both windows at the caret with `ImmSetCompositionWindow` and `ImmSetCandidateWindow`, keeping the candidate window clear of the current line.
- **To be tested**: whether Backspace, Enter and the arrow keys pressed during composition also reach the edit box (no check for VK_PROCESSKEY was found in the window procedure).

## 4. Save file names

- **To ASCII**: `0x1705180`(std::string*) converts in place: þ Þ Ð ð become t T D d, and the other bytes of 0x80 and above are looked up in the table at `0x1c37af0` (accented letters lose their accents; everything else becomes `?`). So every UTF-8 continuation byte becomes `?`.
- The call sites for save paths are `0x13b8f6c` (confirming a save), `0x13b8de1`, `0x13b9567` and `0x5c9fbf`; there are about ten other callers whose purpose has not been checked.
- The path goes to PhysFS without conversion (`0x1718300` calls `PHYSFS_openWrite` directly), and PhysFS uses UTF-8.
- **What the framework does**: it replaces the whole function; UTF-8 sequences are kept as they are, and Windows-1252 single bytes are converted by the original table. For a name with only Windows-1252 characters, the result is exactly the same as before.
