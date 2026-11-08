# A refusal lives with its capability, and the checks crate keeps only what the fakes share

The framework's own refusals of a capability's call, its status and words, were all in one crate of their own, `eu4-harmony-checks`, a module per group, so that the fakes standing in for the core (the fake game of `eu4-harmony test`, the Lua host's own, remote control's engine's) would refuse what the core refuses, in its words. That was its reason, and it holds for few of them: of 22 modules, 16 had the core's one capability module as their only caller, and the refusal that every panel's action makes, "the game does not let the player … now", had been written six times there, each a little differently, and twice more in the core.

A refusal now lives with its capability in the core: in the group's file, a `checks` module, or in the file several groups share (the envoys', the script content's, the military panels'), returning the core's `Fail`. Every action the game's check refuses, with its reasons or none, and every command the command layer does not post, is refused by `game::action`, in one wording. The checks crate keeps the refusals that a fake outside the core makes too (an army, a country or a province that is not there, the game speed, save data too large, a text key missing), the Lua form of answers, and the game's calendar.

## Considered Options

- **Keep every refusal in the crate**: one place for the words, but a crate boundary between a capability and its own refusals, which a reader crosses for nothing, and a second place to change each time a capability changes.
- **Move the crate into the core whole**: the same boundary inside one crate.

## Consequences

- A refusal that a fake comes to need moves into the crate when it does, and the core's capability calls it from there.
- The refusals' tests moved with them; those of the six "not allowed" refusals went, and `game::action`'s tests and the layer's over a built session take their place.
- Some refusals' words changed to the one wording of `game::action`; the change that made it says which.
