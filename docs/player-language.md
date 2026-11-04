# Player language plan

Status: L1 to L4 built (2026-11-05); not yet tried in the game. Terms are in [GLOSSARY.md](../GLOSSARY.md) (Player language, Framework settings, System font). The decision and its reasons are in [ADR player-language-in-the-settings](adr/player-language-in-the-settings.md).

## Goal and scope

- **Goal**: no language has a place of its own in the framework. The player reads the framework's and mods' texts in the language they choose, out of the twelve Paradox names, and the text is drawn in fonts made for that language.
- **In scope**: the list of languages; choosing the player language, the first time and in the settings window; the default system fonts and the fallback order by language; the languages `eu4-harmony new` writes; the documents.
- **Out of scope**: the framework's own texts in the new languages (they show English until translated); the legacy escapes' shifted range (below).

## Behavior

### The languages

| Paradox name | Calls itself | Windows languages that lead to it |
| --- | --- | --- |
| `english` | English | English |
| `french` | Français | French |
| `german` | Deutsch | German |
| `spanish` | Español | Spanish |
| `simp_chinese` | 简体中文 | Chinese of the mainland and Singapore |
| `trad_chinese` | 繁體中文 | Chinese of Taiwan, Hong Kong and Macao |
| `russian` | Русский | Russian |
| `polish` | Polski | Polish |
| `braz_por` | Português do Brasil | Portuguese |
| `japanese` | 日本語 | Japanese |
| `korean` | 한국어 | Korean |
| `turkish` | Türkçe | Turkish |

- A mod's localisation file in a language not on the list is left out, and the startup report says so; the rest of the mod loads.
- A text missing in the player language is English's.

### Choosing it

- The framework settings keep it as `language="<Paradox name>"`. When they name none (the first start, or a file from before), the framework takes the language Windows displays when it is on the list, English otherwise, and writes it into the file.
- The settings window lists the twelve by what each calls itself. A change takes effect at the next start, as a font's does.
- The game's own language plays no part. The mods' texts are still spliced into the localisation file of the language the game runs in.

### Fonts

The fonts for body text, headings and the map follow the player language until the player chooses one; the file keeps only those chosen.

| Player language | Body text | Headings | Map |
| --- | --- | --- | --- |
| Simplified Chinese | FZPingXianYaSongS-R-GB, else Microsoft YaHei | FZYaSongS-B-GB, else Microsoft YaHei | FZShengShiKaiShuS-EB-GB, else KaiTi |
| Traditional Chinese | Microsoft JhengHei | Microsoft JhengHei | DFKai-SB |
| Japanese | Yu Gothic UI | Yu Gothic UI | Yu Mincho |
| Korean | Malgun Gothic | Malgun Gothic | Batang |
| Every other | Segoe UI | Segoe UI Semibold | Georgia |

- The game console's font is Consolas in every language.
- When none of a list is installed, Segoe UI draws. The framework UI draws in the body text's face.
- A character the face lacks comes from the fallback families, the player language's own first: Simplified Chinese tries Microsoft YaHei and SimSun before the rest, Japanese Yu Gothic and Meiryo, Korean Malgun Gothic, every other Segoe UI.
- The settings window offers, for body text, headings and the map, the installed fonts that can write the language's own name, each named as the language names it; for the console, those with Latin letters.
- A settings file from before keeps the fonts it names, except those that were the defaults then (Microsoft YaHei for body text and headings in Regular, KaiTi on the map, Consolas in the console), which follow the language.

### New mods

`eu4-harmony new` writes the mod's texts in English and in the language Windows displays; `--languages french,simp_chinese` names others instead. English and Simplified Chinese get the template's own texts; any other gets the English ones under its own header, to translate.

## Parts

### Every part is done when

`cargo fmt`, the build, the tests and both clippy runs pass, and the documents say what the code does.

### L1 The list of languages

The twelve in the `loc` crate, with what each calls itself, its locale and the Windows languages that lead to it. Done when a mod's file in any of them is read, and one in another language is left out with a note.

### L2 The language setting

The setting, its first value from Windows, the choice in the settings window, and `lang` reading it. Done when a first start writes Windows' language, a changed one shows at the next start, and the startup report names it.

### L3 Fonts by language

The default lists, the fonts not chosen left out of the file, the fallback order, the framework UI's face and the settings window's lists. Done when a player who never chose a font sees the table's fonts for their language.

### L4 New mods and documents

`eu4-harmony new --languages`, and the documents that described the old rule.

## Known limits

- **Legacy escapes' shifted range**: the tools that write legacy escapes for language mods move U+0101 to U+09FF up by 0xE000 before escaping them, and the framework does not move them back, so such a character in a language mod's escaped text comes out as a private-use one. Greek, Cyrillic and the like reach the game as UTF-8 and are not affected ([research/unicode-text.md](research/unicode-text.md)).
