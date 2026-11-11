# Documentation

What is written down about the framework, and where. Terms are in [GLOSSARY.md](../GLOSSARY.md); how the repository is worked in is in [AGENTS.md](../AGENTS.md).

## How things are now

| | |
| --- | --- |
| [game/](game/) | What is known of the game, one page for each of its systems |
| [dev-tools.md](dev-tools.md) | The development tools, what each is for and how to use it |
| [performance.md](performance.md) | What the framework costs the game, as measured in the running game |
| [adding-a-capability.md](adding-a-capability.md) | How a capability is added, layer by layer |

What the capabilities are is `eu4-harmony wiki`'s; how the framework is built, its modules' own comments in the code.

## Records

| | |
| --- | --- |
| [adr/](adr/) | Decisions that are hard to reverse, one file each, and why they were made |
| [research/](research/) | The research into the game's code that the pages of [game/](game/) rest on, with addresses as RVAs into the 1.37.5 baseline |
| [prototype-log/](prototype-log/) | What was tested in the running game and what came out, one file per run |

## Plans

Each larger piece of work has a plan of its own in [plans/](plans/), written as [plans/README.md](plans/README.md) says. A new plan gets a row here.

### In progress

| | |
| --- | --- |
| [game-speed.md](plans/game-speed.md) | Making the game faster without changing the game it plays; how a part is chosen, and which comes next |
| [drawing-thread.md](plans/drawing-thread.md) | The simulation, the interface and the drawing apart: the frames drawn by a thread of their own at the display's rate, the crossings made into events, then the world double buffered |

### Built

| | |
| --- | --- |
| [v1.md](plans/v1.md) | The design of the first version |
| [unicode-module.md](plans/unicode-module.md) | The Unicode module and the framework UI |
| [remote-control.md](plans/remote-control.md) | Remote control: its parts and what each delivers |
| [sessions.md](plans/sessions.md) | Leaving a session without a restart |
| [lua-extensions.md](plans/lua-extensions.md) | Extensions in Lua instead of native DLLs |
| [commands.md](plans/commands.md) | Capabilities that change the game reaching it as commands, and developer builds |
| [leak-finding.md](plans/leak-finding.md) | Finding what the game's memory keeps of its sessions |
| [extension-reload.md](plans/extension-reload.md) | Reloading an extension in the running game |
| [input.md](plans/input.md) | Remote clients giving the game mouse and keyboard input |
| [dev-commands.md](plans/dev-commands.md) | The development tools as commands of `cargo xtask` and the command line tool instead of scripts |
| [hidden-start.md](plans/hidden-start.md) | Starting the game with nothing of it on the player's desktop |
| [loading.md](plans/loading.md) | A shorter startup and load, the data loaded the same |
| [startup-side-by-side.md](plans/startup-side-by-side.md) | The startup's stretches run beside each other where nothing one makes is read by the other, the data loaded the same |
| [player-actions.md](plans/player-actions.md) | Extensions and remote clients doing in the game's panels what the player does |
| [scenarios.md](plans/scenarios.md) | Scenarios: runs of the real game that one runner plays and reports on |
| [player-language.md](plans/player-language.md) | The player language chosen out of twelve, and fonts that follow it |
| [music-volume.md](plans/music-volume.md) | The volume sliders in decibels, and the music mixed by the framework |
| [observe-mode.md](plans/observe-mode.md) | Remote control and extensions in observe mode: a country in view, and acting as it |
| [test-games.md](plans/test-games.md) | Test games: several at once, each with a user folder, a core and a pipe of its own |
| [deepening.md](plans/deepening.md) | Knowledge written out in several places gathered into one module each |
| [whole-game.md](plans/whole-game.md) | The framework played through the real game from end to end before a release |
| [extension-windows.md](plans/extension-windows.md) | Pop-ups and windows of extensions' own in the game's look, laid out in Lua and opened from the panel button strip |
| [map-names-distance-fields.md](plans/map-names-distance-fields.md) | The names on the map drawn from distance fields, sharp at every zoom, with the outline drawn by the map's shader |
| [second-deepening.md](plans/second-deepening.md) | Diplomatic acts, font pages, armies, the input barrier and the catalogue's groups each gathered into one module |

### Replaced

| | |
| --- | --- |
| [game-testing.md](plans/game-testing.md) | The claim of the whole game as commands, replaced by test games; kept as it was |
