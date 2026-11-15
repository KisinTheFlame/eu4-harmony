# Mercenary army names lose bytes while removing colors

This note follows a mercenary company's localised name into the army created when it is hired. The Unicode module replaces the game's byte-based color removal so that complete characters survive. On baseline 1.37.5, the native function changes `merc__local_grand_company` from "大佣兵团" to "夽£兵团" when it creates the army. [The reproduction](../prototype-log/2026-11-15-mercenary-company-names.md) reads the actual army's name from the test game's memory.

All game addresses below are RVAs. Read on 2026-11-15 with `cargo xtask disasm`; the company getter, hiring function, color remover and string repetition function were read to their ends. The other color-removal call sites were checked for argument and output ownership; their complete behavior was not investigated.

## A company name and an army name are separate

The existing company name function `0x8ffcb0` takes `(company, std::string* out)`:

- With a template at company `+0x28`, it copies the template's explicit name at `+0x40` when nonempty. Otherwise it asks the localisation lookup `0x14b34a0` for the template's content key at `+0x20`.
- Without a template it returns an empty string.
- It moves the resulting string into `out`, destroys the emptied temporaries and returns `out`. It neither removes colors nor changes character case.

The hiring function `0x411c60` constructs the army, then gives it its name:

1. At `0x411cfc`, call the company name function.
2. At `0x411d0c`, pass that result as the source to `0x17046a0`, with a separate output string.
3. At `0x411d37`, copy the cleaned result into army `+0x148` with the ordinary string assignment `0x95110`.
4. Destroy both temporary strings, then associate the army with its company at army `+0x48` and continue constructing its regiments and hiring state.

The defect therefore occurs when the army is made, before it is drawn. The company getter can keep returning the correct name while the army's own name is already wrong. It is not caused by the font, an ellipsis or clipping a label to its box.

## The native color remover treats every byte as a character

`0x17046a0` takes `(const std::string* source, std::string* out)` and returns `out`. It initializes an empty output, reads the source length into a signed 32-bit loop bound, and walks the string one byte at a time:

- At `0x1704704`, compare the byte with `0xa7`, the game's single-byte `§` color marker.
- If equal, advance the index twice, dropping that byte and the next byte without examining them.
- Otherwise append exactly that byte. It builds a one-byte temporary with string repetition `0x90320` (count 1), appends it with `0x932f0`, then destroys it, including its heap storage if needed.
- Return the output after the whole loop; there is no UTF-8 or legacy-escape decoding and no other formatting-code removal in this function.

In vanilla's single-byte strings this removes a color marker and its selector. In the Unicode module's game text, `0xa7` can instead be a continuation byte inside a Chinese character. The source is already decoded game text here; decoding legacy escapes again cannot repair this loss.

The exact byte transformation seen in the test game is:

```text
source: e5 a4 a7 e4 bd a3 e5 85 b5 e5 9b a2  (大佣兵团)
removed:     a7 e4
result: e5 a4       bd a3 e5 85 b5 e5 9b a2  (夽£兵团)
```

The surviving `e5 a4 bd` is U+593D, "夽". The lone `a3` is the Windows-1252 `£` in game text. No character substitution is needed to explain either glyph in the supplied image. The control "自由佣兵团" has no `a7` byte and its army name remains intact.

## Scope and the repair boundary

The native remover has eight direct callers: the calls at `0x3a8b39`, `0x411d0c`, `0x844ce0`, `0x852f9c`, `0x853482`, `0xe7c8eb`, `0xe7d3d0` and `0x151c624`. Each passes a source string in RCX and a separate output slot in RDX. Where used, RAX supplies the constructed output for a copy, move or further text operation. The calls at `0x844ce0`, `0x852f9c` and `0x853482` sit in save-name paths; the first passes the result to the existing Unicode save-name replacement. The other callers' complete effects remain unknown; only mercenary hiring was followed as a game operation.

`unicode/colors.rs` replaces the common removal entry, keeping its construction and return convention. It uses `game_text::chars` to remove a color marker only when it is a character of its own, skips its selector as a complete character, and copies every other original byte range. Normal single-byte color selectors retain their native meaning, while icons, country flags and other formatting survive untouched. `game_text::plain` is not an exact replacement because it also rewrites icons.

The replacement initializes the caller's output as an empty game string, then uses the game's own append function so both inline and allocated text have the usual ownership and destructor. Its dependency point checks the native entry bytes at startup. Regression tests first reproduced the exact damaged bytes with the old byte rule, then passed with character boundaries; they cover the grand company, real color selectors, mixed scripts, Windows-1252 characters, other formatting, empty input and a trailing marker.

[The repaired run](../prototype-log/2026-11-15-mercenary-company-names-after.md) hires the grand company and a free company with a colored, longer test name. Two reads of the army fields retain "大佣兵团" exactly and remove real colors from "自由佣兵团 test"; the former stays inline and the latter uses the game's heap storage.

The affected army field already contains damaged text. Preventing future corruption does not by itself restore existing names, which may also have been renamed by a player. Existing army restoration and save/reload behavior are not covered by this repair.
