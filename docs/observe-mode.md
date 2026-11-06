# Observe mode plan

Status: decided with the owner (2026-11-07); not built yet. Terms are in [GLOSSARY.md](../GLOSSARY.md) (Observe mode, Country in view, Player's view, Session). The decision is [ADR observe-mode-acts-as-the-country-in-view](adr/observe-mode-acts-as-the-country-in-view.md).

## Goal and scope

- **Goal**: remote control and extensions work in an observe-mode game as in any other: they read the game, run time, move the camera and use the console with no country in view, put a country in view and take it out as the player does, and act as the country in view.
- **In scope**: the session in observe mode; a capability needing a country, said in `describe` and failing `no_country` without one; the player's view in observe mode; `session.view` and `session.leave_view`; the `country_in_view` event; acting as the country in view; loading an observe-mode save, and starting an observe-mode game.
- **Out of scope**: going into or out of observe mode in a game, which stays with the console's `observe` and `tag`; keeping the AI from playing the country in view; multiplayer, where the game has observers of its own.

## What the game does

From the game's code (RVAs into 1.37.5):

- The world keeps two country tags. `+0x1e60` is the player's own: a country's, or in observe mode the observer tag `O00`, whose kind byte (`+0x1e66`) is 7. `+0x1e68` is the country in view, valid when its valid byte (`+0x1e6f`) is set. The game's own "the player's country" (`0x811b20`) is the country in view when the player observes and one is in view, and the player's own tag otherwise.
- `0x763a20(world, tag)` puts a country in view: it writes `+0x1e68` and, in observe mode, refreshes the fog of war (`0x8275e0`) and the map. Ctrl and a click on a province puts its owner in view (`0x116e357`), as do a country in a panel (`0x139c0e9`) and a player in the outliner (`0x81036e`). The menubar's button takes the view out by passing `---` (`0x81703e`).
- `0x763840(world, tag)` sets the player's own tag: the console's `tag` and `observe` (`0x2788aa`), loading a save (`0x5d046d`) and the setup.
- With the player's own tag not a country's, the fog of war hides nothing, so with no country in view the whole map shows.
- The game refuses observe mode in an ironman game (`FE_NOT_IRONMAN_AND_OBSERVER`). A save of an observe-mode game says `player="O00"` in its header.

## Behavior

### A game with no country played (O1)

- An observe-mode game is a game: the session starts when it is loaded or started, and ends when the player's own tag changes, as it does for `tag` and `observe`. A change of the country in view does not end it.
- `session.get` answers `{number, player, observing, date}`: `player` is the country acted as (the country played, the country in view, or null), `observing` whether the player observes. `menu.load` and `menu.start` answer the same.
- A capability that acts as a country fails with `no_country` while the player observes with no country in view. `not_in_game` stays for where there is no game. `describe` says it of every capability as `country`, beside `in_game`, which is unchanged; `eu4-harmony wiki` says it as a sentence of its own.
- What needs a game but no country works in observe mode: the map and every read, time, the camera, the console, the main menu's capabilities, the interface.
- The player's view is the country in view's fog of war and discoveries, and with none in view the whole map. What the game's panels do not show of a country stays left out until the full view is on.
- Events about the player's country (its armies arriving, its battles, the provinces it discovers) are about the country in view, and with none in view none are heard; the rest are heard as in any game. After a change of view they follow the new country from the next frame on.
- An extension's handlers are called in observe-mode games as in any other.
- `menu.load` loads observe-mode saves, and the saves list says `player: null` for them.

### Changing the view (O2)

- `session.view {country}` puts a country in view, or another in its place, as the player's Ctrl and click does, and answers as `session.get`. `session.leave_view {}` takes it out, and does nothing with none in view. Both fail `rejected` when the player plays a country, `invalid_argument` for a tag that names no country, `not_found` for a country that no longer exists. Extensions have both.
- `country_in_view {country}` is heard at every change of the country in view, whoever made it: a remote client, an extension, or the player on the map, a panel or the outliner. A new session does not start with one; `session.get` says what is in view. Extensions hear it as `on_country_in_view(game, country)`.

### Acting as the country in view (O3)

- Every capability that acts as a country acts as the country in view when it runs: moving armies, recruiting, building, diplomacy, answering events and offers. The AI keeps playing that country, and may replace what a client did.
- What waits beyond the frame on a country's behalf, a diplomatic proposal waiting for its answer or a peace being negotiated, checks when it answers that its country is still the one acted as, and answers `rejected` if it is not.

### Starting a game in observe mode (O4)

- `menu.start {}` with no country starts an observe-mode game, as the setup's observe button does. With ironman on in the setup it is `rejected`, saying why; the setting is left as it is. The command line tool's `game menu start` takes no country for it.

## How it fits together

- **`game::player`**, one module that knows who the player is: `Player::{Plays(country), Observes { own, in_view: Option<country> }}`, read from the four fields, with a dependency point on `0x811b20` that uses them. It puts a country in view and takes it out through `0x763a20`, and an observe point after `0x763a20` tells `sessions` of every change, which hands on the `country_in_view` event at the frame.
- **The session**: `game::context` reads the player through `game::player`. `Session::open` fails `no_country` with no country to act as, so the capabilities taking a session stay as they are. The reads that need no country take a smaller type with the world, the date and what the player's view shows, `Eyes::of(view, player)`, a pure function that decides the fog of war in one place. `sessions` keys a session by the world and the player's own tag.
- **The catalogue and the remote crate**: `Code::NoCountry` (`no_country`); `Capability::country`, from a marker of its own in `group!` beside the place, `anywhere` with `country` failing to compile; a test that the catalogue's capabilities taking a session are those marked `country`. `remote::session::Session` takes `player: Option<CountryTag>` and `observing`.
- **Waits**: the diplomacy tickets and the peace negotiation keep the country they were made for and check it.
- **Saves**: `saves::file` lists the observer tag as no player.
- **Fakes**: the `buffers` session and `catalogue::fake` grow an observe-mode player, and the command line tool's fake world a session without one.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and both clippy runs pass, and so do the offline checks against the game's files;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md), saying as plainly what was not verified;
- a change players or mod authors notice is in README.md and README.zh-CN.md.

### O1 A game with no country played

`game::player` reading the player, the session's key, `no_country`, `country` in `describe` and the wiki, the reads and the player's view, the events about the player's country, the saves list, `menu.load`. Unit tests: `Eyes::of` for each view and player; a session lasting across a change of view and ending at `observe`; a capability needing a country failing `no_country` and a read answering. Tried with the 1600.1.1 observe-mode save: `session.get`, a read of the map, time and the camera with no country in view and with the top great power in view through the game's own Ctrl and click, and `no_country`.

### O2 Changing the view

`session.view`, `session.leave_view`, the observe point and `country_in_view`, for remote clients and extensions. Unit tests: the failures; one event per change. Tried: both capabilities, and the event for a change the game made.

### O3 Acting as the country in view

The waits checking their country. Tried: moving an army, recruiting, building and a diplomatic proposal as the country in view, and whether each held against the AI over a month; a proposal answered after the view changed.

### O4 Starting a game in observe mode

`menu.start` without a country, and the command line tool. Tried: a start, and the refusal with ironman on.

## Order

One pull request a part, in order: O1, O2, O3, O4.
