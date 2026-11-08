# In observe mode, remote control and extensions act as the country in view, and a session lasts across changes of view

In observe mode the player plays no country: the AI plays every country, and the player may put one at a time in view, from the map, a panel or the outliner. A remote client or an extension acting "as the player" (ADR remote-control-uses-capabilities) then acts as the country in view: its orders, recruits, proposals and answers are given in that country's name, while the AI keeps playing it, so the two may undo each other's orders. With no country in view, what needs a country fails with `no_country`, and what does not (reading the map, time, the camera, the console) works as in any game. The player's view (ADR remote-control-reads-the-players-view) is the country in view's fog of war, and with none in view the whole map.

Remote clients and extensions put a country in view and take it out as the player does, with `session.view` and `session.leave_view`, and hear every change of view, whoever made it, as the `country_in_view` event.

A change of view does not end the session. A change of the player's own country does, as before: the console's `tag` and `observe` hand a country between the player and the AI, so the orders, tickets and waits a client holds for the country it played are for a country it no longer plays. A change of view leaves the player's own country as it is, and the player makes it often. A request acts as the country in view when it runs; what waits for a later day on a country's behalf, such as a diplomatic proposal waiting for its answer or a peace being negotiated, checks that country is still the one in view and answers `rejected` if it is not.

## Considered Options

- **Hand the country in view to the player** (the console's `tag`): the AI stops playing it and nothing fights the client's orders, but entering a view would end the session, the game would no longer be the AI-only game the player chose to watch, and taking the view back out would have to hand the country back to the AI.
- **Read only, act as no one**: the simplest, but leaves observe mode without what remote control is mostly for, an AI playing a country.
- **A new session at every change of view**: the rule a change of country follows already, but the player changing view on the map would end a client's session as often as they click, and every wait the client holds would end with it.
- **Stamp each request with the view it was sent under, and reject it if the view changed before it ran**: catches a client acting on a view it no longer has, but every request pays for what only the waits that outlive a frame need.

## Consequences

- `session.get` and the sessions `menu.load` and `menu.start` return say whether the player observes, and their `player` is the country acted as: the country played, the country in view, or none.
- `menu.load` loads saves of observe-mode games, and `menu.start` without a country starts one. Going into or out of observe mode in a game stays with the console.
- An extension's acting capabilities fail with `no_country` while the player observes with no country in view; its handlers are called in observe-mode games as in any other.
- The AI may replace a client's order to the country in view with one of its own at any later day, and nothing tells the client it did, beyond what the client reads.
