# Extensions read everything the game keeps; remote control reads the player's view unless the player gives it the full view

An extension reads everything the game keeps: every country's, province's and trade node's state, armies under fog of war included. A remote client reads the player's view: the map as fog of war leaves it, and of other countries what the player's interface shows them, their panels and the ledger. A framework setting, in the player's hands, gives remote control the full view instead; it is off until the player turns it on. Remote control is mostly an AI playing the game as the player (ADR remote-control-uses-capabilities), which should know what the player would and no more, while a mod's extension is the mod's own logic inside the game (ADR mods-hold-the-logic), such as a map mode or an AI of its own, which may need what the player's interface does not show.

## Considered Options

- **The player's view for both**: the same rule everywhere, but it keeps mods from what the game's own scripts read freely, such as another country's treasury or an army under fog of war.
- **The full view for both**, as reads mostly were: the simplest, but an AI playing through remote control would know what the player cannot.
- **A remote client choosing its view in each request**: the most flexible, but the AI being judged could ask for the full view itself; the setting leaves the choice with whoever runs the game.

## Consequences

- Every read capability is described for both views; under the player's view what the player's interface does not show is left out of the answer.
- Capabilities that exist change: `army.at` gives an extension the armies under fog of war too, and remote control's `country.get` and the other reads are held to the player's view unless the setting is on.
- What an action needs to know about another country, such as whether it would accept a proposal, is what the player's interface says of it, in either view.
