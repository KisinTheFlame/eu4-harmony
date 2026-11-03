localisation: texts in the player language

Texts go in eu4-harmony/localisation/<name>_l_<language>.yml, one file per
language: english, french, german, spanish, simp_chinese. The format is the
game's localisation format, as plain UTF-8:

  l_english:
   MY_MOD_TOOLTIP:0 "§YMy Mod§!\nClick to send the army home."
   MY_MOD_GREETING:0 "Welcome back, $NAME$."

- Start every key with the mod's own prefix, in capitals: keys share one
  namespace with the game's and every other mod's, and the first mod in the
  playset wins a key two define.
- The framework shows the player language: Chinese where Windows displays
  Chinese, the game's own language (English, French, German or Spanish)
  otherwise. A key the player language's file lacks falls back to English,
  so write English first.
- Interface fragments use the keys as the game's own .gui files do.
- Lua gets a text with text.get("MY_MOD_GREETING", { NAME = name }), which
  fills in its $NAME$ places; text.language() names the player language.
- The manifest's description is a key, shown in the mods window.

Done when every key the mod uses is in the English file, every file reads
(eu4-harmony package checks them), and the texts show in the game.
