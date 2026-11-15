# Mercenary army names lose bytes while removing colors

This note follows a mercenary company's localised name into the army created when it is hired. The Unicode module replaces the game's byte-based color removal so that complete characters survive. On baseline 1.37.5, the native function changes `merc__local_grand_company` from "大佣兵团" to "夽£兵团" when it creates the army. [The reproduction](mechanics-evidence.md) reads the actual army's name from the test game's memory.

All game addresses below are RVAs. Read on 2026-11-15 with `cargo xtask disasm`; the company getter, hiring function, color remover and string repetition function were read to their ends. The other color-removal call sites were checked for argument and output ownership; their complete behavior was not investigated.


The exact byte transformation seen in the test game is:

```text
source: e5 a4 a7 e4 bd a3 e5 85 b5 e5 9b a2  (大佣兵团)
removed:     a7 e4
result: e5 a4       bd a3 e5 85 b5 e5 9b a2  (夽£兵团)
```

The surviving `e5 a4 bd` is U+593D, "夽". The lone `a3` is the Windows-1252 `£` in game text. No character substitution is needed to explain either glyph in the supplied image. The control "自由佣兵团" has no `a7` byte and its army name remains intact.

`unicode/colors.rs` replaces the common removal entry, keeping its construction and return convention. It uses `game_text::chars` to remove a color marker only when it is a character of its own, skips its selector as a complete character, and copies every other original byte range. Normal single-byte color selectors retain their native meaning, while icons, country flags and other formatting survive untouched. `game_text::plain` is not an exact replacement because it also rewrites icons.

The replacement initializes the caller's output as an empty game string, then uses the game's own append function so both inline and allocated text have the usual ownership and destructor. Its dependency point checks the native entry bytes at startup. Regression tests first reproduced the exact damaged bytes with the old byte rule, then passed with character boundaries; they cover the grand company, real color selectors, mixed scripts, Windows-1252 characters, other formatting, empty input and a trailing marker.

[The repaired run](mechanics-evidence.md) hires the grand company and a free company with a colored, longer test name. Two reads of the army fields retain "大佣兵团" exactly and remove real colors from "自由佣兵团 test"; the former stays inline and the latter uses the game's heap storage.

The affected army field already contains damaged text. Preventing future corruption does not by itself restore existing names, which may also have been renamed by a player. Existing army restoration and save/reload behavior are not covered by this repair.

The systematic game account is [text-formatting](../game/interface/text-formatting.md).
