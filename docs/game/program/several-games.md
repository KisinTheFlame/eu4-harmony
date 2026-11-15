# Several games at once

## The game's instance check never finds anything

`main` (`0x8757e0`) calls `0x14c4470` with `"Europa Universalis IV"` (`0x1c45be0`, at `0x875809`). That calls `FindWindowExA` (import slot `0x1b66d78`) at `0x14c4496` with the text in `r8`, the **class** name, and `NULL` in `r9`, the title. When a window is found it logs "An instance of this game is already running on this computer! Exiting.", shows a message box unless `-suppresserrormessages` (`MessageBoxA`, `0x1b66dc8`) and calls `FatalExit` (`0x1b66868`).

The game does register a window class of that name: `0x14c3dd0` fills a `WNDCLASSEXA` (size `0x50`, style `0x48`) with the name it is given and calls `RegisterClassExA` (`0x1b66d70`, at `0x14c3e59`). But its window is SDL's, of class `SDL_app`, titled `Europa Universalis IV`; no window of the class `Europa Universalis IV` is ever made (_inferred_ from the processes' windows, which are only `SDL_app` and `IME`). So the check finds nothing, and a second game starts. Tried: a second `eu4.exe` started while the first was at its main menu ran on, and its `system.log` has no line of the check. ([headless.md](headless.md) took the text for the title; [architecture.md](architecture.md) says the game refuses a second instance.)

## Steam, and why the game must start from its own folder

Both games ran with Steam signed in, which showed nothing. Started from another folder, the game loses DLCs:

- **A folder of junctions**: a folder holding a junction to each of the game folder's folders, copies of its files, a `userdir.txt` of its own and `steam_appid.txt` (`236850`), so that it could hold a framework of its own. The game started and played, but 5 of its 31 DLCs were disabled (`game.log`, "Setting up DLC …: Disabled": Muslim Advisor Portraits, National Monuments, National Monuments II, Dharma - Indian Sultanate Pack, Dharma - Hindu Unit Pack), and `error.log` had 17 lines "Could not find item in store backend" that a game from the game folder does not have. The same with the variable `SteamAppId` instead of `steam_appid.txt`, and with the player's `pdx_login_cache.txt` copied into its user folder.
- **The game folder, with `-userdir`**: a third game from the game folder itself, with a user folder of its own and no login cache, had all 31 enabled and no such line, while the first ran.

Why Steam leaves those 5 out for a process outside the game folder was not looked into; each has a `steam_id` in its `.dlc` file, as the others do. A game with DLCs missing is another game: saves differ, and a run on it does not test what the player plays. So every game starts from the game folder.

## The game folder, the user folder and the framework

- **The core.** The proxy loads `eu4-harmony\eu4_harmony_core.dll` beside `eu4.exe` (`proxy/src/lib.rs`), so every game from the game folder runs the same core, the one installed last. Builds of two checkouts side by side need the proxy to load a core from elsewhere, named on the command line, as `-harmony_hidden` is read: the proxy may read `GetCommandLineW` in `DllMain` (_inferred_: it touches no other module). The developer build's options, `eu4-harmony\dev.toml` in the game folder, go with the core.
The native user-folder selection is the subject here. Harmony also honors `-userdir` through `folders::user_dir_of`; earlier framework-ignore observations are historical.
- **What a user folder needs** to play as the player's: `settings.txt`, `dlc_load.json` (the playset) and the `mod` folder its descriptors are in (a junction to the player's served); the framework's `eu4-harmony\settings.txt` for the language; the saves a run loads. The login cache is not needed for the DLCs.
- **The game folder's own files.** The game writes `console_history.txt` there when the console is used, the one file two games would share (_inferred_; not tried).
