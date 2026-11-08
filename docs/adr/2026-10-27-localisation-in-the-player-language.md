---
status: superseded in part by built-in-unicode-module, player-language-in-the-settings and extensions-are-lua-in-a-sandbox
---

# Localisation in the player language: the framework decides it in one place and transcodes mods' texts

The "How it is decided" part and the "transcode" part have been superseded by ADR built-in-unicode-module: the player language came to depend only on the Windows display language (since ADR player-language-in-the-settings, the player chooses it in the framework settings), and mods' texts are handed to the game unchanged as UTF-8. Extensions get their texts with the Lua global `text`, not the SDK's (ADR extensions-are-lua-in-a-sandbox). The in-game language setting the consequences foresee is ADR player-language-in-the-settings, and Japanese and Korean are among its twelve languages, since nothing is written as legacy escapes any more. The file location, the splicing and the lookup rules still hold.

The framework's and every mod's texts are all shown in the same "player language", which the framework decides once at startup. The deciding logic lives only in the core's `lang` module; other code only reads the result.

**How it is decided**: from the Windows display language and the language set in the game's settings (`language` in `settings.txt`: English, French, German or Spanish). Windows is consulted as well because Chinese players' game language is in fact English, with Chinese shown by a Chinese language mod.

**Where mods' texts go**: in `eu4-harmony/localisation/<name>_l_<language>.yml`. The format is the game's own localisation file format, but the content is plain UTF-8, so authors do not have to write legacy escapes themselves.

**How the framework uses these texts**:
- At startup, the framework transcodes each mod's keys for the player language and appends them to the end of `localisation/text_l_<language>.yml` for the game's current language. Interface fragments can therefore use them as they use vanilla keys.
- At run time, extensions get texts with the SDK's `text`.
- The framework's own notice wording is compiled into the core in the same format.

The file format, the list of languages and the lookup rules (falling back to English when a key is missing) live in a separate `loc` crate, shared by the core, the scaffold and the fake game.

## Considered Options

- **Each mod handles it itself** (the earlier approach; v1 once listed "converting the encoding of mods' localisation files" as out of scope): regional-combat wrote legacy escapes on its own for this, and the localisation files it generated were Chinese only, and garbage for every other player. Every mod would repeat this work, and each could support only one language.
- **Mods put their files in the game's own `localisation/` as usual**: the game reads them directly, but Chinese has to be written as legacy escapes beforehand, and such a file is garbage in a game that does not read them. One file cannot suit both kinds of player.
- **Make a new virtual localisation file in PhysFS**: cleaner than appending to a vanilla file, but it needs directory enumeration hooked as well, which takes more reverse engineering. Appending uses only the existing splicing.

## Consequences

- Adding an in-game language setting later needs only the decision in the `lang` module changed. But the game reads localisation files only once, at startup, so the spliced keys change language only at the next start; texts at run time and notices can follow at once.
- The file appended to is often overridden by a Chinese language mod. The splice applies to the file actually read, so this does no harm. But if a mod replaces `localisation/` as a whole with `replace_path`, the target file may not exist.
- Mods' keys share one namespace with vanilla keys, so authors have to add their own prefixes. When several mods define the same key, the mod earlier in the playset wins, and this is recorded in the startup report.
- Japanese and Korean are not supported for now, because the framework writes legacy escapes only for the characters in the CJK ideograph range.
- Error details, logs and the startup report are for developers and stay in English.
