# The player chooses the player language in the framework settings, out of twelve

The player language is a framework setting. The first time the framework starts, when the settings name no language, it takes the language Windows displays, if that is one of twelve, and English otherwise, and writes it into the settings; from then on the player changes it in the settings window, and the change takes effect at the next start. The twelve are Paradox's: `english`, `french`, `german`, `spanish`, `simp_chinese`, `trad_chinese`, `russian`, `polish`, `braz_por`, `japanese`, `korean` and `turkish`. Windows' Traditional Chinese (Taiwan, Hong Kong, Macao) is `trad_chinese`, every Portuguese `braz_por`. A text missing in the player language is English's, with nothing in between. The language in the game's own settings plays no part.

The system fonts the player has not chosen follow the player language: each language has, for body text, headings and the map, a list of fonts, of which the first installed draws, and the fallback for characters a font lacks tries the language's own fonts first. Latin and Cyrillic languages draw in Segoe UI, their headings in Segoe UI Semibold, the map in Georgia.

This supersedes the rule in ADR built-in-unicode-module that the player language is Simplified Chinese when Windows displays Chinese and the game's own language otherwise, and the defaults in ADR game-text-in-system-fonts (Microsoft YaHei, Microsoft YaHei and KaiTi for every player).

## Considered Options

- **Keep the old rule, Chinese by Windows and the rest by the game**: Chinese was the only language a player could read without the game having it; a Japanese or Russian player read English, and a mod's files in their language were refused as an unknown language. A player whose Windows is Chinese but who plays without a Chinese language mod read Chinese notices in an English game, and could do nothing about it.
- **Decide it at every start, the game's language for the four it has and Windows' for the rest**: follows the game when the player switches its language, but still leaves the player no way to correct a wrong guess.
- **Offer "automatic" in the setting**: one more choice whose meaning the player has to learn; once the first start has written Windows' language, choosing it again is one click.
- **An open list, any `l_<id>`**: a mod could ship a language the framework does not know, but no Windows language would ever lead to it and the settings window could not name it.
- **Falling back through a near language, Traditional to Simplified Chinese**: kinder to Traditional Chinese readers while the framework has no texts in their language, but one more rule for every text, and the near language is not always welcome.
- **The same fonts for every player (Microsoft YaHei and KaiTi)**: since every glyph is drawn from system fonts, Latin text in a Chinese face looked foreign to every other player, and characters Chinese, Japanese and Korean share took Chinese shapes for Japanese and Korean readers.

## Consequences

- Players of languages the game lacks read mods' texts in their language; the framework's own texts stay English until someone translates them (it has English, French, German, Spanish and Simplified Chinese).
- A mod's file in a language not on the list is left out and the startup report says so; the mod is loaded.
- A player who has never chosen a font sees the framework's defaults for their language, and sees them change with it. A settings file from before this decision keeps the fonts it names, except those that were the defaults then, which follow the language from then on.
- The settings window lists, for body text, headings and the map, the fonts that can write the language's own name ("Deutsch", "日本語"), named as the language names them. That cannot tell a Chinese font from a Japanese one; the defaults do.
- `eu4-harmony new` writes a new mod's texts in English and in the language Windows displays, or those `--languages` names.
