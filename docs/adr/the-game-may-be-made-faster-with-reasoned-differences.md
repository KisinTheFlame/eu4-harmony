# The game may be made faster with reasoned differences

The framework may change what the game computes to make it faster, the AI included, when each difference has a reason that can be stated and checked, and the game the player plays is the same game in kind: the same rules and data, an AI that decides by the same rules and the same information. A change of this kind is allowed when its differences are one of these, said in the change's research and its pull request:

- **When or in what order**: the same work done at another moment or in another order, of the kind the game's own threads already make. The game runs its countries on several threads and does not repeat a day from the same save; with `-seed` it runs them one after another to repeat ([research/ai.md](../research/ai.md), [research/late-game.md](../research/late-game.md), "Whether a game repeats its days"). Work that reads the world as it stood a moment earlier, where the game's own threads could have read it so, is of this kind.
- **The same answer**: a computation that gives what the game's gives, shown by comparing the two in the running game, call by call where it can be.
- **A simplification**: less of the game's work done, the AI deciding more coarsely, with what it gives up stated, and shown on a bench of runs, several seeds and decades each, not to make the AI play worse by the measures the bench compares (wars, territory, armies, money, development).

Each change can be turned off, in a developer build at least, so that its gain and its differences can be measured against the game's own code; the change's research records both.

What stays as it was: defines, scripts, the game's rules and the player's own play; a change that the player sees, such as fewer frames drawn, is still the player's choice.

## Considered Options

- **What the game computes left as it is** (ADR making-the-game-faster-leaves-what-it-computes-as-it-is, superseded): the same days to the byte. It left out every change inside the day, where most of the late game's time is (the AI, the countries' updates, the month), because the game does not repeat itself, so there was nothing to compare a change with but the game's own varying days.
- **Anything that keeps the game playable**: the largest gains, with no reason asked for each difference; no one could tell how far the game played had moved.

## Consequences

- Changes inside the day are made, beginning with the AI: the searches of a country's armies run on idle threads (research/ai.md).
- Each such change says which kind its differences are, and is measured on the game as the player has it, with the change on and off.
- A bench of runs compared by statistics is needed for simplifications; until it exists only the first two kinds are made.
