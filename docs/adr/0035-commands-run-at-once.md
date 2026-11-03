# The command layer has the game execute a command at once

A command posted to the game's session waits in its queue until the session's next update, a frame later, which checks it again and executes it ([research/commands.md](../research/commands.md)). The command layer (ADR 0034) does not post its command to that queue. Once the command is read from its text, has come back as written and passed its check, the layer has the game execute it then and there, as the session's update executes one command: stamped with the session's sender and tick as the session's post stamps it, with the steps before and after it, the session's flag that a command executes and the random generator's flag set as the update sets them, and through the same function object, so the game logs it and the layer hears it as any other. So when a capability on the layer returns, the game has carried its command out: an army ordered to move has its route, a recruit is paid for and queued, the speed is set.

## Considered Options

- **Answer a frame later, once the command has run**: the meaning is the same, but every call that changes the game waits a frame, so a loop that orders fifty armies takes fifty frames, and an extension has to start tasks of its own (ADR 0030) to give them in one.
- **Answer once the session has taken the command, and say later what came of it** (a handle to wait for, or an event): calls are quick, but reading the army just after the call shows its old route, the game dropping a command can only be told afterwards, and the API has a second kind of waiting beside the functions that wait.
- **Post to the queue and have the session run its queue at once**: the session's own path from end to end, but the queue holds what the game posted before, nearly always the next day at speed 5, and a day that runs long draws a whole frame of its own. Tried in the game: inside the framework's safe point, that frame came back into the safe point, which waited on itself, and the game hung.

## Consequences

- Nothing waits: what a capability changes is changed when it returns, as when it called the game's functions itself.
- What the game posted before, the player's and the AI's commands and the next day, still runs at the next frame, after the layer's command.
- The layer executes only on the main thread, at the framework's safe point, in the states in which the session's update executes commands, and never while a command executes. A command executed so has no ID of the session's and is not handed on as the session hands on the commands it executed (written once more); the game executes some of its own commands so too, such as an army's move command in its daily update.
- Every machine of a multiplayer game must execute the same commands in the same order, which a command executed at once on one machine breaks. Multiplayer is out of scope (docs/commands.md); supporting it means posting through the session's queue and answering when the command comes back to run, which this would have to be changed for.
