# Each remote control request acts as the player or as the game; extensions may act as the game too

A remote client's request acts as the player unless it asks to act as the game (`--as-game` on the command line, `"as": "game"` on the pipe). As the player, it reads the player's view and does only what the player could, as every request did until now. As the game, it reads the full view and may also change the game as the game's and the mods' own scripts do, such as setting a flag or firing an event, which no player could. Extensions read the full view as they did, and may always change the game as the game, since such a change is what the mod's own scripts could do anyway, in ironman too. The framework setting "Remote control sees everything" goes: the request says what it reads.

This supersedes in part ADR remote-control-reads-the-players-view (the setting, and the player's view as remote control's only one), ADR remote-control-uses-capabilities (a remote client acting as the player in every request) and ADR mods-hold-the-logic (a capability that changes the game doing only what the player can do).

## Considered Options

- **Keep the setting, in the player's hands, and add one for changing the game as the game**: what ADR remote-control-reads-the-players-view chose, so that an AI judged on its play could not give itself the full view. Remote control is open only to the Windows user playing, on this computer, who can already change the saves, the game's files and its memory: a setting guards nothing that user could not do, and costs every remote client that needs more a trip to the settings window.
- **Run the game console's commands for changes beyond the player**: the console refuses almost every command in ironman and the developers' commands always, and crashes the game on purpose when its ironman check is bypassed ([research/game-console.md](../research/game-console.md)).
- **Changes beyond the player for extensions only**: a remote client, such as a scenario setting up a test, would have no way to put the game in the state it needs.

## Consequences

- Every capability says whether it needs a request as the game. Changes beyond the player (the `script` group's writes, and the typed changes of ADR changes-as-the-game-are-typed-capabilities) need it, and so does reading what the player's interface never shows, such as flags. A request as the player is refused such a capability with `as_the_game`.
- Every read is still described for both views; which one a request reads is now its own choice. A remote client that wants to judge an AI fairly sends it only requests as the player.
- An extension cannot tell a request as the game from the game's own scripts' changes, as it cannot tell the player's orders from a remote client's.
- A settings file that still says `remote_full_view` is read without it.
