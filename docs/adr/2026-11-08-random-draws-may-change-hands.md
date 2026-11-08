# Random draws may change hands

When the framework runs the game's work on several threads, a random number the game would have given one caller may go to another: the draws come in another order, and each object may get other numbers than the game's code would give it. That is a reasoned difference of the kind "when or in what order" ([ADR the-game-may-be-made-faster-with-reasoned-differences](2026-11-07-the-game-may-be-made-faster-with-reasoned-differences.md)) as long as every draw stays a fair draw: a number of the same distribution as the game's, from a generator as sound as the game's, never one drawn twice nor a generator's state torn by two threads at once. The framework draws for such work from generators it keeps safe on several threads, not from the game's generators unguarded.

The framework does not support multiplayer: a speed-up that changes the order of draws, or any other part of a day, is made for a game one machine computes, and nothing keeps several machines in step. The decision was the owner's (2026-11-08): "random numbers going to others, with the distribution the same, is fully acceptable", and "we do not support multiplayer".

The game draws in order from one generator ([research/monthly-update.md](../research/monthly-update.md)): a country's or a province's month takes the numbers after those before it, so that no loop that draws could run on several threads and give the same days. A game loaded twice already goes two ways, since a load takes a new seed and the AI's countries draw on several threads from the world's generator by their timing ([research/late-game.md](../research/late-game.md), "Whether a game repeats its days"): which number goes to whom is not something the player plays by.

## Considered Options

- **Draws only in the game's order**: every loop that draws stays on one thread, and with it every country's and province's month; the month's stall stays as it is.
- **Any draw from the game's generators, from any thread**: the game's generators have no lock, so two threads drawing at once would tear their state and could hand two callers one number.

## Consequences

- Loops that draw can be run on several threads, when what each part writes is safe to write at once; a draw is no longer what stops them. What each part reads and writes of other objects still decides it.
- A speed-up that changes the order of draws cannot be checked against the game's own answer call by call, since every number after the first moved draw differs: it is checked by its parts that do not draw, and by its differences said in its research, as the game's own threads' are.
- A thread-safe way of drawing is needed before the first such speed-up, and its own research says what it keeps of the game's generators (their seed in the save, `-seed`, the draws forbidden in a frame inside a day).
- A multiplayer game with such a speed-up on goes out of step; a player's build may leave the speed-ups that change the order of draws to the game in a multiplayer session, which is not needed for this decision.
