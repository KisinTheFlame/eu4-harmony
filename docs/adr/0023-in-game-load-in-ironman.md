# An ironman session can be left by an in-game load, and its ironman exit save can be turned off

Vanilla lets the player load another save in the middle of a session only outside ironman; in ironman the load button is turned off. Only the interface refuses: the game's own in-game load writes the ironman exit save of the session it leaves before it resets the world. The framework turns the button on in ironman too, for the player and for remote control's `menu.load` alike, so remote control still does only what the player can do (ADR 0015).

A framework setting, "save an ironman game before an in-game load", on by default, decides whether that load writes the ironman exit save. Turned off, the session is left without it, and loading the same save again goes back to it: an undo. Returning to the main menu always writes the ironman exit save, so leaving an ironman game the ordinary way never loses it. Achievements are left alone: the game keeps no mark of an undo, and the framework adds none. In ironman with the setting on, the question that confirms the load says that the current game is saved first, instead of the game's warning that progress since the last save is lost.

## Considered Options

- **Keep vanilla's refusal**: remote control would leave an ironman session only through the main menu, and the player could not undo at all; an undo was wanted.
- **Always write the ironman exit save**: no undo either.
- **Turn achievements off for a session that was undone**: the framework would have to keep its own mark in the save; and in a single player game the player can already end the process to the same effect.

## Consequences

- With the setting off, an ironman game no longer means that nothing can be undone, and achievements are still given in such a game.
- An in-game load whose ironman exit save fails does not happen: the player stays in the session, and a remote client is answered why.
- Loading a save sets ironman from that save: from an ironman session into a plain save gives a plain session, and the other way round.
