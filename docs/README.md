# Documentation

What is written down about the framework, and where. Terms are in [GLOSSARY.md](../GLOSSARY.md); how the repository is worked in is in [AGENTS.md](../AGENTS.md).

## How things are now

| | |
| --- | --- |
| [adr/](adr/) | Decisions that are hard to reverse, one file each, and why they were made |
| [research/](research/) | What is known about the game's code, with addresses as RVAs into the 1.37.5 baseline |
| [prototype-log/](prototype-log/) | What was tested in the running game and what came out, one file per run |
| [performance.md](performance.md) | What the framework costs the game, as measured in the running game |
| [dev-tools.md](dev-tools.md) | The development tools, what each is for and how to use it |
| [adding-a-capability.md](adding-a-capability.md) | How a capability is added, layer by layer |

## Plans

Each larger piece of work has a plan of its own: what it is for, its parts, and a status line at its top that says what was built and tried. A new plan gets a row here.

### In progress

| | |
| --- | --- |
| [game-speed.md](game-speed.md) | Making the game faster without changing the game it plays; how a part is chosen, and which comes next |

### Built

| | |
| --- | --- |
| [v1.md](v1.md) | The design of the first version |
| [unicode-module.md](unicode-module.md) | The Unicode module and the framework UI |
| [remote-control.md](remote-control.md) | Remote control: its parts and what each delivers |
| [sessions.md](sessions.md) | Leaving a session without a restart |
| [lua-extensions.md](lua-extensions.md) | Extensions in Lua instead of native DLLs |
| [commands.md](commands.md) | Capabilities that change the game reaching it as commands, and developer builds |
| [leak-finding.md](leak-finding.md) | Finding what the game's memory keeps of its sessions |
| [extension-reload.md](extension-reload.md) | Reloading an extension in the running game |
| [input.md](input.md) | Remote clients giving the game mouse and keyboard input |
| [dev-commands.md](dev-commands.md) | The development tools as commands of `cargo xtask` and the command line tool instead of scripts |
| [hidden-start.md](hidden-start.md) | Starting the game with nothing of it on the player's desktop |
| [loading.md](loading.md) | A shorter startup and load, the data loaded the same |
| [player-actions.md](player-actions.md) | Extensions and remote clients doing in the game's panels what the player does |
| [scenarios.md](scenarios.md) | Scenarios: runs of the real game that one runner plays and reports on |
| [player-language.md](player-language.md) | The player language chosen out of twelve, and fonts that follow it |
| [music-volume.md](music-volume.md) | The volume sliders in decibels, and the music mixed by the framework |
| [observe-mode.md](observe-mode.md) | Remote control and extensions in observe mode: a country in view, and acting as it |
| [test-games.md](test-games.md) | Test games: several at once, each with a user folder, a core and a pipe of its own |
| [deepening.md](deepening.md) | Knowledge written out in several places gathered into one module each |
| [whole-game.md](whole-game.md) | The framework played through the real game from end to end before a release |

### Replaced

| | |
| --- | --- |
| [game-testing.md](game-testing.md) | The claim of the whole game as commands, replaced by test games; kept as it was |
