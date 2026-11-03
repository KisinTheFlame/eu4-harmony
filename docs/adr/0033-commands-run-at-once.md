# The command layer has the game run a command at once

A command posted to the game's session waits in its queue until the session's next update, a frame later, which checks it again and runs it ([research/commands.md](../research/commands.md)). The command layer (ADR 0032) does not leave its command there: once the session has taken it, the layer has the session run its queue then and there, as its update does, with the same check, the same steps around the command's execution and the same bookkeeping. What the player and the AI posted since the last update runs first, as it would have at the next. So when a capability on the layer returns, the game has carried its command out: an army ordered to move has its route, a recruit is paid for and queued, the speed is set. A command the game drops as it runs it is refused there and then.

## Considered Options

- **Answer a frame later, once the command has run**: the meaning is the same, but every call that changes the game waits a frame, so a loop that orders fifty armies takes fifty frames, and an extension has to start tasks of its own (ADR 0030) to give them in one.
- **Answer once the session has taken the command, and say later what came of it** (a handle to wait for, or an event): calls are quick, but reading the army just after the call shows its old route, the game dropping a command can only be told afterwards, and the API has a second kind of waiting beside the functions that wait.

## Consequences

- Nothing waits: what a capability changes is changed when it returns, as when it called the game's functions itself.
- The session's queue is run outside its update, at the framework's safe point and only there, on the main thread, in the states in which the update runs it, and never while a command executes. A session that cannot run it then takes no command from the layer.
- Every machine of a multiplayer game must run the same commands in the same order, which a command run at once on one machine breaks. Multiplayer is out of scope (docs/commands.md); supporting it means posting through the session's queue again and answering when the command comes back to run, which this would have to be changed for.
- Commands posted together still run in the order they were posted, and each is checked against what the ones before it left.
- The game posts each day as a command in the map manager's frame, before the safe point, so at speed 5 a day is nearly always queued there. Running the queue runs that day too, up to a frame early, inside the capability's call: an extension's stretch that orders an army can take as long as a day takes.
