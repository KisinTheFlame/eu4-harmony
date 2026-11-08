# Remote control is always on

Remote control has no setting and shows nothing on screen. Whenever the framework runs, the named pipe is open to the Windows user running the game, and every request is written to the framework log. The game console is open to remote clients and extensions in every build.

A program running as that user can already do anything to the game process: read and write its memory, inject a DLL, send it input. The pipe gives such a program nothing it could not do anyway; it only makes the useful things easy. Other users and the network cannot reach it.

## Considered Options

- **A framework setting, off by default**: one more step before every debugging session or AI game, protecting against programs that could do worse without the pipe.
- **A mark on screen while a remote client is active**: each call of the command line tool is a short connection, so "connected" means nothing to the engine; a mark for "a request in the last 30 seconds" would be a guess, and the log already records every request.
- **The game console only in development builds**: the console is the quickest way to set up a situation while debugging, and an AI that should not cheat can simply be told not to use it.

## Consequences

- Anyone who can run programs as the player can play the game for them, including with console commands.
- To see what a remote client did, read the framework log.
