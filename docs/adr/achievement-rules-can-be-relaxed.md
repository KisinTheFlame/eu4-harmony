# The player can let achievements be earned outside ironman and with a changed game checksum

Vanilla gives achievements only in an ironman game, only while the game checksum is vanilla's, and only when every other rule holds: the save checksum of the save loaded, the difficulty, the game rules, the major DLCs, single player. Two framework settings, both off by default and the player's alone (no capability or manifest reaches them), lift one rule each:

- **Achievements outside ironman**: the game decides as if every game were ironman, for achievements only. Ironman itself is unchanged: the game console and loading other saves stay what they are in each kind of game. The game has no rule of its own about the game console, which vanilla keeps from achievements only by refusing it in ironman, so a game that is not ironman can earn achievements with the console used and with saves loaded again.
- **Achievements with a changed game checksum**: the game takes its game checksum as vanilla's, for achievements and for everything else that follows it: the saves it writes are salted as an unmodified game's, and an ironman save loads in a modded game.

Every other rule stays vanilla's, the save checksum among them: a save the game marked as modified keeps achievements off, whichever setting is on. The framework leaves no mark of its own in a save; the game decides with the settings of the moment. The settings window opens only in the main menu, so a change takes effect with the next game started or loaded, without a restart.

## Considered Options

- **One setting for both rules**: a player who plays with a mod that changes the game checksum may still want vanilla's ironman, and the other way round.
- **A third setting for the game console**: the game has no such rule to lift; the framework would have to keep its own record of the console's use, in the save, which this decision keeps out.
- **Lift the game checksum only where achievements are decided**: saves written in a modded game would still be salted as modified ones, and the first load of such a save would turn achievements off for the rest of that game.
- **Let a mod ask for achievements to stay on**: which mod changed the game checksum cannot be told from it, and achievements are the player's to decide.

## Consequences

- With the game checksum setting on, a save written in a modded game cannot be told from one written in vanilla by its save checksum.
- A game played in a modded game before that setting was turned on carries saves marked as modified, and earns no achievements after it either.
- A game that is not ironman, played in vanilla before achievements outside ironman were turned on, earns achievements when it is loaded with the setting on.
- Multiplayer is unchanged: the game's own rule about multiplayer is decided before both of these.
