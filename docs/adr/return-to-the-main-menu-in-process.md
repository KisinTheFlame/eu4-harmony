# Returning to the main menu does not restart the game

Vanilla restarts `eu4.exe` every time a session is left for the main menu, and the new process loads the whole game again: about 45 s on the project's machine before the main menu answers. An in-game load, which vanilla offers outside ironman, replaces a session in the same process in a few seconds, because it only resets the world and reads the save. The framework returns to the main menu the same way, as an in-process return: the process goes on, the game's interface, graphics and scripts stay loaded, and only the session is left behind. It is a framework setting, "return without restarting", on by default; turned off, the game restarts as vanilla does.

Returning takes the session down as the in-game load does, its interface first, then with the world reset the in-game load runs, here with the game's history, then makes a main menu as startup does: the main menu, and the single player setup it makes, find the world as at startup, in the order startup has them. The continue box is read again, so it offers the save the game wrote last, as it does after a restart. The player's own ways back (the game's question, the end-of-game summary, the single player setup's back button, which restarts the game in vanilla too) and remote control's `menu.return` go through the same path.

Vanilla never takes a session down in a process that goes on, outside an in-game load, whose save fills in what the world reset leaves, so what it leaves is the framework's to put back: after every world reset, the Empire's religious status and religions and the campaign's achievements are put back as a new start has them; and as the map manager goes, the session's interface it never lets go (its minimap, its end-of-game summary and some twenty more windows) is let go first, which would otherwise stay in the game's GUI with its buttons calling into the map manager that went.

There is no list of states in which returning is known to be safe, and no fallback to a restart for them: every return takes this path, and a state in which it goes wrong is a defect to fix. The only return that does not happen is one whose ironman exit save failed: the player stays in the session.

## Considered Options

- **Keep the restart**: nothing the game never did, but 45 s per return, and remote clients lose the pipe and reconnect to a new process.
- **Leave the history for later**: reset the world without the history at the return and read the history only when the single player setup is entered, or not at all when a save is loaded next. Tried: the main menu, made before that reset, read the player's gameplay options into the world, which the reset then put back to their defaults; and a return then a new game set the country tags up twice, the slowest part of a reset, so the new game came no sooner. The return itself was about 2 s quicker.
- **Leave the world as it is until the setup is entered**: the return is quicker still, but the main menu's map then draws the session's armies and fleets, as the game did in the first tries.
- **Fall back to a restart outside the states the tests covered**: hides defects instead of fixing them, and makes some returns take 45 s for reasons the player cannot see.
- **Patch `eu4.exe`**: the framework leaves the exe untouched (ADR version-proxy-injection) and checks every place it relies on at startup (ADR framework-succeeds-or-fails-as-a-whole).

## Consequences

- A return takes about 4 to 6 s with an ironman exit save; a save loaded or a game started from the returned main menu is the same as from a new process, but for what the game chooses at random (compared section by section in their first saves).
- What a world reset leaves of a session is found by comparing a new game's first save after a return with one from a new process; what the map manager leaves, by the game's GUI still listing the session's elements. Both are defects in the game that a restart hides, and more may turn up; each is fixed where it is found.
- The game's own world reset and map manager do not let all their memory go, in vanilla's in-game loads as in returns: memory grows with each.
- A call into the game that faults in the middle of a return, a defect, restarts the game as the setting turned off would, and the log says where.
- A process now sees several sessions with the main menu between them. Extensions hear that a session ended; what the framework keeps per process (bindings, notices, its UI's resources) has to stay right across sessions.
- Remote control answers `menu.return` once the main menu is up, from the same process: no new pipe, and event numbers go on. With the setting off it answers as before, `ending`, and the process changes.
