# The internals of the game's text (for the Unicode module)

Baseline 1.37.5 (`9ad3efe1…`). All addresses are RVAs. Every item has been checked against eu4.exe's disassembly; the addresses the code uses also have dependency points that check their bytes.

## Fonts and glyphs

- **Glyph record**: 16 bytes. `+0` x, `+2` y, `+4` width, `+6` height (u16); `+8` x offset, `+0xa` y offset, `+0xc` advance (i16); `+0xe` one byte: 1 when a kerning pair starting with this character exists; `+0xf` unused.
- **Font object**: from `+0x120`, 256 pointers to glyph records (the "glyph table", indexed by byte value); from `+0x920`, the kerning table; at `+0x968`, the kerning scale (float). Where a character is missing, code in several places falls back to `'?'` (index 0x3f).
- **Kerning**: `0x15943d0`(glyph table, previous byte, next byte) returns a float. The key is `(prev<<16)|next`, with both cut to 8 bits, so kerning only works for characters 0..0xFF.
- **`.fnt` loading**: function `0x15953c0`. A `char` line is parsed, in order, into id `rdi`, x `r14d`, y `r15d`, width `r12d`, height `esi` (cleared when the id is a space), x offset `r13d`, y offset `[rbp+0x1148]`, advance `[rsp+0x40]`; the font is in `[rbp+0x1130]`. At `0x1595c86`, `cmp edi, 0xff; ja 0x1595f01` drops ids above 0xFF; `0x1595c92` then excludes the line feed; a new 16-byte record is allocated with `0x1a332d4` and written to `font+0x120+id*8` (`0x1595cf2`), and then `0x1595cfa` goes back for the next line. A duplicate id goes to `0x1595d07` (which writes "Duplicate character ID" to error.log).
- **Freeing the glyph table**: `0x1594360`(glyph table) frees the 256 records (`0x17394f0`, size 0x10), then the kerning table.
- **Places that look up the glyph table directly by byte** (`[font + byte*8 + 0x120]`): a scan of the whole code section finds 13: 4 in the font loader; `0x159972c`, `0x159a7a4` (main text measuring and drawing); `0x159b68b`; `0x159c837`, `0x159cec9`; `0x159dc08`, `0x159e761`; `0xfd7342`; `0xb7f578`. Buttons and other paths address it differently and are checked separately.

## Localisation

- **Importing one entry at a time**: `0x16fd650`(key, value: `std::string*`, version u32, unused, context*). Context: `+0` line number (i32), `+4` whether to replace (u8), `+0x10` the localisation set. When the global `[0x2357b20]+0x38` is not 0, the key and value are first converted from UTF-8 to Windows-1252 by `0x17059e0` and written into two static buffers (`0x234db20`, `0x234fb20`, 8 KiB each); then the registering function is called.
- **Registering**: `0x16fa8d0`(set, key, value: NUL-terminated, line number, version, replace).
- **UTF-8→1252 conversion**: `0x17059e0`(source, source length, destination, destination length, …) writes `?` where there is no matching character (logging "Couldn't find Latin1 character"). Two more wrappers, `0x1705e00` and `0x1705ec0`, assign the converted text to a `std::string`; their callers are at `0x95dd7`, `0x95f77`, `0x156044b`, `0x1667cc8` and `0x10555ae`, and what text each of them handles has not been worked out yet.

## The font's text methods

The font's vtable is at `0x1d99120`. The methods that walk text byte by byte:

| Slot | Function | What it does | In the module |
|---|---|---|---|
| `+0x40` | `0x1596660` | Buttons, instant text boxes: lays out the text, then draws it into their own texture (15 parameters) | `buttons.rs` |
| `+0x48` | `0x159c600` | Text that floats up and fades out (12 parameters); no calls seen in single player | `floating_text.rs` |
| `+0x50` | `0x15988e0` | Main text (11 parameters; the 11th is "recognize formatting codes") | `main_text.rs` |
| `+0x58` | `0x159b470` | Measures height | `text_height.rs` |
| `+0x60` | `0x159b7c0` | Measures width | `text_width.rs` |
| `+0x70` | `0x159ec30` | Fits text into a box of a given width and height; what does not fit is cut off and `...` added | `fitted_text.rs` |
| `+0x78` | `0x159e510` | Map name measuring and wrapping | `map_names.rs` |
| `+0x80` | `0x159f510` | As above, and can also insert line breaks by width (11 parameters); every traced call comes from text box layout `0x15f440f` (loading tips, the music list, the macro builder's descriptions) | `fitted_text.rs` |

Besides these, `0x159da60` builds the meshes for map names (called from country name placement around `0xfd7200` and `0xfd63a0`), and `0xfd7342` counts the visible characters in a country name; both are taken over in `map_names.rs`. The `0xb7f578` listed earlier looks up an array on the stack, not the glyph table.

Bending a country name along its curve (`0xfd3850`):

- Each glyph has a place `t` along the curve: `i / (n - 1)` for glyph `i` of `n`, or 1.0 (`0x1dc1604`) when the name has one letter (`0xfd413e`). The curve is two cubic halves split at `t` = 0.5 (`xmm9`), evaluated by `0xfd8570`. The glyph's direction is the curve's at `t`, from a second sample 0.01 away (`0xfd4331`, `0xfd447e`), normalized by `0x8b3080` into two floats at `[rbp]`; the normal is `(-y, x)` (`0xfd46af`).
- A single glyph therefore takes the direction at the curve's very end, which can hook back: 明 (Ming), one Chinese character, lay upside down (tested in the game 2026-10-28).
- For names of one or two bytes, `0xfd3ef5` sets `ebx` = 1000, and at `0xfd459e` the direction is blended fully (`ebx` / 1000) towards another vector at `[rbp+0x20]`. That vector is built as doubles (`0xfd3e15`) but read here as floats (`0xfd45d4`, `0xfd464b`), so the result is arbitrary: counting the length in characters made 明 lie on its side instead.
- The framework leaves both rules as the game has them and sets the direction to (1, 0) at `0xfd4683` for a name of one character, which then stands upright.

- **Main text layout**: each byte is first copied into the 256-byte word buffer (`0x2433cd0`; the ordinary path does no bounds check), then measured. A glyph without height (a space) ends the word; if the word does not fit on this line, a line break is added first and the word follows it. A glyph with height continues the word; but if the line is already full and the output buffer (`0x242bf90`) is not empty, the half word gathered so far is moved to a new line and the rest starts a new word, while the new line's width is counted from the part that was moved. So a "word" longer than a line is cut into "full line, short line, full line, short line…".
- **Formatting codes**: `§` plus one byte is a color (looked up by `0x15a0390`, which logs "Could not find coloring" when it finds none); an icon name starting with `£` runs up to the first byte that is not a letter, a digit, `_` or `|`, and that byte is swallowed too (the main text at `0x1599682`: a digit, or a byte from `A` to `|` whose bit is set in the mask `0xbffffff43ffffff`; the measuring, floating text and map name methods load the same mask); `@` swallows the three bytes after it; `$` swallows the one byte after it; `¤` is followed by a number.

## The encoding of game text

Game text in the module: a character Windows-1252 has is stored as its single byte, anything else as UTF-8 (see `unicode/game_text.rs`). So punctuation used in Chinese, such as `·`, `“`, `”`, `…` and `—`, is single bytes mixed in with UTF-8.

- Line break opportunities have to be worked out character by character over this mixed text: if only the valid UTF-8 at the start is passed to ICU, there are no break points after the first 1252 byte, and everything after it becomes one "word" (what happened before the 2026-10-28 fix: short lines, full stops at the start of a line, pinyin cut apart).
- Fonts made for the CJK font plugin (such as the Chinese language mod's `zh-hans-*.fnt`) put these punctuation marks at their Unicode code points (`char id=8230` and so on), and the 1252 byte positions (0x85, 0x93, …) are empty; looking glyphs up by single byte alone cannot draw them.

## Open questions

- Which texture each draw uses, and how glyphs on several pages are split into batches.
