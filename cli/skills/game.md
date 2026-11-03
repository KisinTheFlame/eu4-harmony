game: trying the mod in the running game

Remote control lets you drive the running game from the command line, as
the player would: eu4-harmony game <capability> [arguments], answering in
JSON (eu4-harmony wiki remote has how a game goes and the exit codes).

steps:

1. Back up the player's saves and settings first: the game rewrites a save
   it loads, rotates its autosaves and rewrites its settings. Copy the
   folder Documents\Paradox Interactive\Europa Universalis IV (its save
   games, settings.txt, dlc_load.json) somewhere outside it. Done when the
   copy exists.
2. Register the mod: eu4-harmony link, from the mod's folder. Enable it in a
   Paradox launcher playset (or add mod/eu4-harmony-<id>.mod to
   enabled_mods in dlc_load.json). Done when the mod is in the playset.
3. Start the game and wait for the main menu: eu4-harmony game menu answers
   place main_menu.
4. Enter a game: eu4-harmony game menu start <country tag>, or
   menu.load <save> (menu.saves lists them). Let time run with
   eu4-harmony game time advance <days>; answer what stops it with
   question.list and question.answer.
5. Read what the mod did: its log, eu4-harmony/logs/<id>.log in that
   Documents folder, and the startup report beside it, report.txt, which
   says whether the extension loaded and how long its handlers ran. For
   the screen, eu4-harmony game screenshot --out shot.png. Done when each
   handler you changed has run and the log shows no error.
6. Leave nothing behind: close the game, put the copied files back, and
   remove the mod from the playset if it was there only for the test.

After a change to Lua, start the game again: a mod is loaded once, at
startup.
