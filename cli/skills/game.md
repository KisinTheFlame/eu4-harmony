game: trying the mod in the running game

Remote control lets you drive the running game from the command line, as
the player would: eu4-harmony game <capability> [arguments], answering in
JSON (eu4-harmony wiki remote has how a game goes and the exit codes). A
request acts as the player, reading only what the player sees; add
--as-game to read everything the game keeps, as your extension does, or to
use a capability that does what no player could (its wiki page says so).

steps:

1. Back up the player's saves and settings first: the game rewrites a save
   it loads, rotates its autosaves and rewrites its settings. With the game
   closed, eu4-harmony backup <new folder>, a folder outside the game and
   the Documents\Paradox Interactive\Europa Universalis IV folder. Done when
   it prints "backed up to".
2. Register the mod: eu4-harmony link, from the mod's folder. Enable it in a
   Paradox launcher playset (or add mod/eu4-harmony-<id>.mod to
   enabled_mods in dlc_load.json). Done when the mod is in the playset.
3. Start the game and wait for the main menu: eu4-harmony start, or
   eu4-harmony start --hidden for a game with nothing of it on the desktop
   and no sound (screenshot and input work as usual). Done when it prints
   "at the main menu"; eu4-harmony game menu then answers place main_menu.
4. Enter a game: eu4-harmony game menu start <country tag>, or
   menu.load <save> (menu.saves lists them). Let time run with
   eu4-harmony game time advance <days>; answer what stops it with
   question.list and question.answer.
5. Read what the mod did: its log, eu4-harmony/logs/extension-<id>.log in that
   Documents folder, and the startup report beside it, report.txt, which
   says whether the extension loaded and how long its handlers ran. For
   the screen, eu4-harmony game screenshot --out shot.png. Done when each
   handler you changed has run and the log shows no error.
6. Leave nothing behind: close the game, run eu4-harmony restore <that
   folder>, which lists what the run changed and puts every file back, and
   remove the mod from the playset if it was there only for the test. Done
   when restore prints "every file is as the backup has it"; a NOT RESTORED
   line names a file it could not put back, and the backup stays.

After a change to the mod's Lua or texts, run eu4-harmony reload from its
folder, or keep eu4-harmony reload --watch running: the running game takes
the new code up in place, the session going on, and the mod's state comes
through save_data to on_session as from a save. Done when it prints
"reloaded"; when the new code does not load it prints why, and the old code
goes on. A change to ui/ or to the manifest takes a restart of the game.
