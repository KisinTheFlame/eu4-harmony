# Observe mode plan

Status: decided with the owner (2026-11-07); built and tried in the game (2026-11-07). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Observe mode, Country in view, Player's view, Session). The decision is [ADR observe-mode-acts-as-the-country-in-view](adr/observe-mode-acts-as-the-country-in-view.md).

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
- The game refuses observe mode in an ironman game (`FE_NOT_IRONMAN_AND_OBSERVER`). A save of an observe-mode game says `player="O00"` in its header, or, made with a country in view, that country's tag. Loading the 1600.1.1 save of `O00`, the game put Sweden in view (the run of 2026-11-07).

## Behavior

### A game with no country played (O1)

- An observe-mode game is a game: the session starts when it is loaded or started, and ends when the player's own tag changes, as it does for `tag` and `observe`. A change of the country in view does not end it. The game carries out `observe` at its next frame, and `console.run` answers then, so that a request sent after its answer runs in the session that comes ([remote-control.md](remote-control.md), C5).
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

- **`game::player`**, one module that knows who the player is: `Player::{Plays(country), Observes { own, in_view: Option<country> }}`, read from the four fields, with a dependency point on `0x811b20` that uses them. It puts a country in view and takes it out through `0x763a20`, passing `---` to take it out as the menubar's button does (`0x817036`). The frame compares the country in view with the last frame's of the session (`view_changed`) and is heard as `country_in_view` when it differs: whoever changed it, as often as the frame shows a change. Two changes between two frames are heard as the last; an observe point on `0x763a20` would hear both, but nothing a client does lands between two frames but its own requests, which answer with the session as they leave it.
- **The session**: `game::context` reads the player through `game::player`. `Session::open` fails `no_country` with no country to act as, so the capabilities taking a session stay as they are. The reads that need no country take a smaller type with the world, the date and what the player's view shows, `Eyes::of(view, player)`, a pure function that decides the fog of war in one place. `sessions` keys a session by the world and the player's own tag.
- **The catalogue and the remote crate**: `Code::NoCountry` (`no_country`); `Capability::country`, from the place `group!` declares each capability with: `game` acts as a country, `world` needs a game but no country (the markers the time group brought in on 2026-11-06, whose `observing` in `describe` `country` replaces), and `anywhere` neither; `layer!` opens a `Session` for `session` and a `WorldSession` for `world`, and a test keeps the two in agreement. `remote::session::Session` takes `player: Option<CountryTag>` and `observing`.
- **Waits**: a diplomatic action, a peace offer among them, keeps the country it was sent as; its answer, read at a later frame, is `rejected` once another country, or none, is acted as (`still_acting`).
- **Saves**: `saves::file` lists the observer tag as no player.
- **Fakes**: `catalogue::fake` takes a player to give the session, and the command line tool's fake world answers `observing`.
- **Starting**: `menu.start` with no country starts as the observer's tag (`O00`, the player's own tag in observe mode) through the launches as any start; at the main menu, as the setup's observe button (`0x11374d0`) and then its start: the observer's tag the game gives for `---` of the rebels' kind (`0x76ee40`) made the player's, and the setup's start (`0x1120280`).

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and both clippy runs pass, and so do the offline checks against the game's files;
- what it delivers was tried in the game, with the run recorded in a file of its own in [prototype-log/](prototype-log/), saying as plainly what was not verified;
- a change players or mod authors notice is in README.md and README.zh-CN.md.

### O1 A game with no country played

`game::player` reading the player, the session's key, `no_country`, `country` in `describe` and the wiki, the reads and the player's view, the events about the player's country, the saves list, `menu.load`. Unit tests: `Eyes::of` for each view and player; a session lasting across a change of view and ending at `observe`; a capability needing a country failing `no_country` and a read answering. Tried with the 1600.1.1 observe-mode save: `session.get`, a read of the map, time and the camera with no country in view and with the top great power in view through the game's own Ctrl and click, and `no_country`.

### O2 Changing the view

`session.view`, `session.leave_view`, the frame's comparison and `country_in_view`, for remote clients and extensions. Unit tests: the failures; one event per change. Tried: both capabilities, and the event for a change the game made.

### O3 Acting as the country in view

The waits checking their country. Tried: moving an army, recruiting, building and a diplomatic proposal as the country in view, and whether each held against the AI over a month; a proposal answered after the view changed.

### O4 Starting a game in observe mode

`menu.start` without a country, and the command line tool. Tried: a start, and the refusal with ironman on.

## Order

In order, O1, O2, O3, O4, in one pull request.
