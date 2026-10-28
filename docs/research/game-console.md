# The game console (for `game::console`)

Baseline 1.37.5. All addresses are RVAs. Found by reading eu4.exe on 2026-10-29; what was checked in the running game is marked "tested".

## The console and its commands

- **The game console** is an object of 0x48 bytes at the global `0x2357b58`, made by whichever getter asks for it first (`0x6e74c0` is the plain one; nine other functions make it inline). What the framework knows of it:
  - `+0x00`: its commands, a `std::vector` of 0xa8-byte entries, each with a byte at `+0` that says whether the command is available in release builds, its name at `+8`, how many other names it has at `+0x10` and those at `+0x18`, its handler at `+0x38` and the handler's context at `+0x40`. `0x1721000` looks a name up, comparing it with each name and each other name through `0x1a48020`.
  - `+0x18` (u8): the commands only developers may run are locked.
  - `+0x19` (u8): the game is a multiplayer or an ironman game.
  - `+0x44` (u8): set by every command run but `stats`.
  - The getters clear `+0x19` and `+0x44` and leave `+0x18` as the allocation left it.
- **EU4's commands** are set up whenever the in-game idler is: `0x6e90f0(idler)` hands the idler to the world (`0x756a30`), sets the console's `+0x18` to 1, sets `+0x19` to `0x6eaa10(idler)` or the byte at `world+0x24d0` (one of them multiplayer, the other ironman; which is which was not worked out), and registers the 378 (`0x17a`) commands of the table at `0x1fc23e0` (0xa0 bytes each: the release byte at `+0`, the name at `+8`, the description at `+0x30`, the handler at `+0x38`) through `0x1720b50`. A few engine commands (`rendertype`, at `0x891a0`) are registered on their own. So in the main menu, before any game, EU4's commands are not there and `+0x18` has never been set.
- **324 of the 378 are available in release builds.** The other 54 are developer tools: `debug_*` but `debug_mode`, `build_all`, `build_forts`, `moveunit`, `deiron`, `set_achievements_ok`, `trade_edit` and the like.
- **There is no `add_treasury` command.** `add_treasury` is a script effect, which `run <file>` runs with the other effects in a file. Gold is `cash [amount]` (handler `0xe64f60`): with an amount it adds that much and prints "Gold changed by <amount>"; without one it adds 5000 and prints "Now <name> have cash!". Both succeed.

## What keeps a command from running

All three checks are in the console's run function, `0x1721320`, before it calls the handler:

- **Only for developers**: a command whose release byte is 0, while `+0x18` is set, is refused with "Command available only for developers." Every game start sets `+0x18`. Only `enable_all_commands <password>` (handler `0xe43160`) clears it, together with `+0x19`, and only when the SHA-256 of the password is the hash kept in the exe (`669de9ff…`); otherwise it prints "Cheaters are not welcome here."
- **Multiplayer or ironman**: while `+0x19` is set, every command but `enable_all_commands` and `stats` is refused with "Command not available in multiplayer or ironman mode." After the handler has run, the function writes to address 0 (`mov dword ptr [0], 0x539` at `0x172192d`) if `+0x19` is set and the command was neither of the two: it crashes the game on purpose when the check was got around.
- **Unknown**: a name no command has is answered "Command not found. Did you mean '<name>'?" when `0x1721100` finds a close one, and "Unknown command" otherwise. While `+0x18` is clear a second table is looked in first (`0x17265c0`). A command without a handler is answered "Console Error - command has missing action."

The framework runs commands with these checks, as the player's console window does, and changes none of the flags.

## Running a command, and what it printed

- **The console window** (`tweakergui/console.cpp`) runs a line when Enter is pressed, in `0x166de20`: it reads the edit box, trims spaces, tabs, CR and LF from both ends (`0x17043c0`, with the set at `0x1dbf598`), adds the line to its history (at most 512 lines, also appended to `console_history.txt` in the game folder), cuts the line at `|` (`0x17047c0`), trims the first part and cuts it at every space, so two spaces in a row make an empty word and quotes mean nothing. It hands the words to `run(console, &result, &words)`, then logs "Console command <name> succeeded:" or "failed:" with the text, and prints the text in its window line by line (cut at LF), each line between `§W` and `§!` when the command succeeded and between `§R` and `§!` when not (`0x166d260`). When the line had exactly one `|`, it also writes the text to the file named after it ("Output written to …").
- **`run(console, &result, &words)`** at `0x1721320`: the words are a `std::vector<std::string>` (begin at `+0`, end at `+8`), the command's name first. It only reads them: it copies the words after the name into a vector of its own, each from its text up to its NUL, before calling the function at `0x2357b50` when one is set (`0x8740b0`, set by `0x8757e0`, which looks at the multiplayer and ironman state again) and then `handler(&result, &words, context)`.
- **The result** is 0x28 bytes: a byte at `+0`, 1 when the command succeeded, and at `+8` a `std::string` of game text, what the window prints. The refusals above write it the same way (`0x17213d4`: `mov byte ptr [rdi], 0` … `movups [rdi+8], xmm0`).
- **Other text in the window**: assertion failures (`0x873700`), the map editor's tools (`0x1258230` and others) and the completion of a command's name (`0x166d5e0`) print to the window directly; none of them is a command's output.

## What the framework does

`game::console::run` cuts a command into words as the window cuts a line (and refuses a `|`, since the answer holds the output, a NUL and an empty command), writes each word as game text into a `std::string` in its own memory, gets the console from `0x6e74c0` and calls `0x1721320`. It reads the result, destroys its string with the game's destructor (`0x95660`), and turns the game text into Unicode. The words stay the framework's: the game reads them and never frees them.

The dependency points are "game console" (`0x6e74c0`, which keeps the global), "game console run" (`0x1721320`, from its start to the developer check) and "game console run result" (`0x17213d4`, the result's layout).
