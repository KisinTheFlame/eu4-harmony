# Capabilities answer facts and rules; mods hold the logic

A capability answers one kind of fact about the game, such as a province's owner, or one of the game's rules, such as whether an army may step from one province to another, or does one thing the player can do. It may answer for many objects in one call, as `province.list` does for the provinces it names, since that is the same answer asked many times. It does not carry a mod's way of putting such answers together: a range of provinces around a center, a choice of target, a plan. That is the mod's, in Lua, where it can be tested with the fake game and changed without a new framework.

When a mod built this way costs too much, the cause is found by measuring first. The framework makes the capabilities it calls cheaper; a coarser capability comes only when the cost is in crossing into the framework itself, and then it is still a general one (ADR 0005).

## Considered Options

- **A coarser capability for each costly pattern** (`army.range`, tried for regional combat and taken back out): fewer calls, but the catalogue would grow one mod's algorithm at a time, with each one's choices (where a range ends, what it counts) fixed for every mod, and the cost that made it seem needed was the framework's reads, not the calls (ADR 0032).
- **Mods may ship native code for their hot paths**: what ADR 0024 left behind, with its build, safety and review costs.

## Consequences

- The catalogue stays small and general; mods compose it, and two mods can compose it differently.
- L6's cost target is met, or not, by the framework's capabilities being cheap enough for a mod to call them as often as its logic needs.
- A capability that answers for many objects is added when one call per object is the cost, measured, not as a shortcut for one mod.
