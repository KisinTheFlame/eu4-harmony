# Making the game faster leaves what it computes as it is

The framework makes the game faster only where that leaves the game's rules, data, AI and decisions as they are: in work the game does that writes no game state (Paradox's online client, compressing and writing saves, drawing, the framework's own work), and, when the player chooses it, in how often the game draws while a day is worked out. It never makes the game faster by doing less of the game's work: no define, script or AI setting changes, no update skipped or thinned, no precision lowered, no feature turned off.

The game does not repeat itself ([research/main-thread.md](../research/main-thread.md)): the same save played for the same days with the same answers differs after ten days, from inside the day. So a change cannot be shown to keep the game by running it twice and comparing. A change is allowed instead by where it acts, each kind with its own proof ([game-speed.md](../game-speed.md)):

- **Outside the game state**: shown by reading the code, which writes nothing the day reads.
- **The pace of frames**: frames inside a day execute no commands and draw no random numbers, so drawing fewer of them changes when the screen is redrawn and when the player's input is read, not the day. It is the player's choice, since the player sees it, and the game's own pace is the default.
- **Inside the day**: only on a bench that makes the game repeat, so that two builds can be compared day by day. There is none, so no change is made there.

## Considered Options

- **Anything that keeps the game playable**: thinner AI, monthly work spread out, fewer defines evaluated. The largest gains, but the game played is then another game, and no one could tell how far.
- **Changes inside the day argued from the code alone**: a faster allocator, the parallel stages reworked. The code does not show whether the game orders or hashes by address, or why its runs differ, so the argument cannot be finished.
- **Fewer frames for everyone**: the gain without a setting, but the map then looks choppier to every player, and some would rather have it smooth.

## Consequences

- The gains are bounded by what is not the day: about a third of the main thread early in a game and more than half late in it, at the fastest speed.
- The framework changes a module that is not `eu4.exe` for the first time: Paradox's client, `pops_api.dll`, whose bytes become dependency points under ADR framework-succeeds-or-fails-as-a-whole.
- Saves are compressed by the framework, so their bytes differ from what the game would write, while their contents do not.
- A change inside the day waits until the game can be made to repeat on a bench.
