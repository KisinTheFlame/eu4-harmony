---
status: superseded in part by ADR-0010
---

# Localisation in the player language: the framework decides it in one place and transcodes mods' texts

The "How it is decided" part and the "transcode" part have been superseded by ADR 0010: the player language now depends only on the Windows display language, and mods' texts are handed to the game unchanged as UTF-8. The file location, the splicing and the lookup rules still hold.

The framework's and every mod's texts are all shown in the same "player language", which the framework decides once at startup. The deciding logic lives only in the core's `lang` module; other code only reads the result.

**How it is decided**: when the CJK font plugin is loaded and the Windows display language is Chinese, Simplified Chinese; otherwise whichever of English, French, German or Spanish is set in the game's settings (`language` in `settings.txt`). Windows is consulted as well because Chinese players' game language is in fact English, with Chinese characters shown by a Chinese language mod and the font plugin; and the plugin serves Chinese, Japanese and Korean alike, so the plugin alone cannot tell which language it is.

**Where mods' texts go**: in `eu4-harmony/localisation/<name>_l_<language>.yml`. The format is the game's own localisation file format, but the content is plain UTF-8, so authors do not have to write the escapes the font plugin requires themselves.

**How the framework uses these texts**:
- At startup, the framework transcodes each mod's keys for the player language and appends them to the end of `localisation/text_l_<language>.yml` for the game's current language. Interface fragments can therefore use them as they use vanilla keys.
- At run time, extensions get texts with the SDK's `text`.
- The framework's own notice wording is compiled into the core in the same format.

The file format, the list of languages and the lookup rules (falling back to English when a key is missing) live in a separate `loc` crate, shared by the core, the scaffold and the fake game.

## Considered Options

- **Each mod handles it itself** (the earlier approach; v1 once listed "converting the encoding of mods' localisation files" as out of scope): regional-combat implemented the plugin's escapes on its own for this, and the localisation files it generated were Chinese only, so players without the plugin saw garbage. Every mod would repeat this work, and each could support only one language.
- **Mods put their files in the game's own `localisation/` as usual**: the game reads them directly, but Chinese has to be escaped beforehand, and an escaped file is garbage in a game without the plugin. One file cannot suit both kinds of player.
- **Make a new virtual localisation file in PhysFS**: cleaner than appending to a vanilla file, but it needs directory enumeration hooked as well, which takes more reverse engineering. Appending uses only the existing splicing.
- **Decide on Chinese from the font plugin alone**: Japanese and Korean players install this plugin too, and would be taken for Chinese.

## Consequences

- Adding an in-game language setting later needs only the decision in the `lang` module changed. But the game reads localisation files only once, at startup, so the spliced keys change language only at the next start; texts at run time and notices can follow at once.
- The file appended to is often overridden by a Chinese language mod. The splice applies to the file actually read, so this does no harm. But if a mod replaces `localisation/` as a whole with `replace_path`, the target file may not exist.
- Mods' keys share one namespace with vanilla keys, so authors have to add their own prefixes. When several mods define the same key, the mod earlier in the playset wins, and this is recorded in the startup report.
- Japanese and Korean are not supported for now, because the framework escapes for the plugin only the characters in the CJK ideograph range.
- Error details, logs and the startup report are for developers and stay in English.
