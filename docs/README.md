# Documentation

What is written down about the framework, and where. Terms are in [GLOSSARY.md](../GLOSSARY.md); how the repository is worked in is in [AGENTS.md](../AGENTS.md).

## How things are now

| | |
| --- | --- |
| [game/](game/) | What is known of the game, one page for each of its systems |
| [dev-tools.md](dev-tools.md) | The development tools, what each is for and how to use it |
| [performance.md](performance.md) | What the framework costs the game, as measured in the running game |
| [logs.md](logs.md) | Bounded framework logs, repeated-error summaries, event snapshots, cleanup and troubleshooting collection |
| [adding-a-capability.md](adding-a-capability.md) | How a capability is added, layer by layer |

What the capabilities are is `eu4-harmony wiki`'s; how the framework is built, its modules' own comments in the code.

## Records

| | |
| --- | --- |
| [adr/](adr/) | Decisions that are hard to reverse, one file each, and why they were made |
| [research/](research/) | The research into the game's code that the pages of [game/](game/) rest on, with addresses as RVAs into the 1.37.5 baseline; [addresses.toml](research/addresses.toml) indexes every address they name |
| [prototype-log/](prototype-log/) | What was tested in the running game and what came out, one file per run |
| [2026-11-13-fourth-deepening-whole.md](prototype-log/2026-11-13-fourth-deepening-whole.md) | The full scenario set, with old local-mod registration mismatches recorded separately from native behavior |
| [2026-11-13-fourth-deepening-private-registrations.md](prototype-log/2026-11-13-fourth-deepening-private-registrations.md) | The blocked save scenarios replayed with identical cores and private aliases for the actual loaded mod |
| [2026-11-13-fourth-deepening-native-windows.md](prototype-log/2026-11-13-fourth-deepening-native-windows.md) | All 28 native window steps passed through reload and session changes, with selected captures inspected |
| [2026-11-13-fourth-deepening-tooltip-handoff.md](prototype-log/2026-11-13-fourth-deepening-tooltip-handoff.md) | Native tooltip coverage on both presenting threads and stationary row replacement, with cropped evidence |
| [2026-11-13-fourth-deepening-journal-path.md](prototype-log/2026-11-13-fourth-deepening-journal-path.md) | Four native window courses completed, but the scenario read the old extension-log filename; the reader was corrected |
| [2026-11-13-fourth-deepening-probe-layout.md](prototype-log/2026-11-13-fourth-deepening-probe-layout.md) | The first native window course rejected an undersized test-mod layout; no native result claimed |
| [2026-11-13-non-ironman-achievement-tracking.md](prototype-log/2026-11-13-non-ironman-achievement-tracking.md) | Empty non-ironman achievement lists reproduced and repaired; a daily completion checked with platform submission intercepted |
| [2026-11-12-core-highlights-baseline.md](prototype-log/2026-11-12-core-highlights-baseline.md) | The missing core-hover outlines reproduced on the 0.1.4 baseline |
| [2026-11-12-core-highlights-clearing.md](prototype-log/2026-11-12-core-highlights-clearing.md) | Skipping only post-draw clearing established why asynchronous drawing lost the outlines |
| [2026-11-12-prepared-core-highlights.md](prototype-log/2026-11-12-prepared-core-highlights.md) | Prepared highlights checked across target changes, removal, loading, the country in view and a speed-5 run |
| [2026-11-12-core-highlights-final.md](prototype-log/2026-11-12-core-highlights-final.md) | The final source's core-hover smoke check and standard validation |
| [2026-11-12-core-highlight-capture-reuse.md](prototype-log/2026-11-12-core-highlight-capture-reuse.md) | Capture-buffer reuse, failed-capture invalidation and the final short game check |
| [panel-entry-text.md](research/panel-entry-text.md) | Independent native numbers and the shared final tooltip-content seam |
| [area-governing-cost.md](research/area-governing-cost.md) | Owned areas, native governing costs, and statehood without completing cores |
| [Convenience Tools run](prototype-log/2026-11-12-convenience-tools.md) | Three finite batches in a separate mod, live area costs and trade-company exclusion |
| [Convenience Tools entry](prototype-log/2026-11-12-convenience-entry.md) | Native static title, original image and button states; isolated resource-registration diagnosis |
| [Managed tooltips and panel entries](prototype-log/2026-11-13-managed-tooltips.md) | Shared tooltip adapters, direct images and independent entry numbers; native verification and its limits |
| [Convenience Tools window](prototype-log/2026-11-12-convenience-ui.md) | Grouped native controls, a single result line with a text tooltip, and four-language rendering |
| [Reviewed area and live capacity](prototype-log/2026-11-12-convenience-reviewed-capacity.md) | Separate national reads, nullable prediction failures, tooltip regressions and a focused native comparison |
| [Convenience Tools hands-on](prototype-log/2026-11-12-convenience-hands-on.md) | The isolated visible game handed to the user before publication |
| [research/mercenary-drill.md](research/mercenary-drill.md) | Mercenary drill eligibility, per-regiment damage modifiers, and their path into land-battle losses |
| [research/mercenary-discipline.md](research/mercenary-discipline.md) | The battle panel's discipline overview, the separate mercenary and company additions used by regiments, and the supplied save's sources |
| [prototype-log/2026-11-12-mercenary-drill.md](prototype-log/2026-11-12-mercenary-drill.md) | Mercenary drill growth and its army-panel display in the supplied save's reduced playset |
| [prototype-log/2026-11-12-mercenary-drill-eligibility.md](prototype-log/2026-11-12-mercenary-drill-eligibility.md) | The country's runtime mercenary drill permission, checked separately from its ideas |
| [prototype-log/2026-11-12-mercenary-discipline-panel.md](prototype-log/2026-11-12-mercenary-discipline-panel.md) | The battle overview's 105% discipline and the separate mercenary and company modifiers |
| [prototype-log/2026-11-12-event-texture-master-check.md](prototype-log/2026-11-12-event-texture-master-check.md) | A bounded check of event-option textures on the then-current master; the reported corruption was not reproduced |
| [prototype-log/2026-11-12-event-texture-published-check.md](prototype-log/2026-11-12-event-texture-published-check.md) | The 0.1.3 core-source comparison: text initialization overlaps observed, exact texture corruption not reproduced |

## Plans

Each larger piece of work has a plan of its own in [plans/](plans/), written as [plans/README.md](plans/README.md) says. A new plan gets a row here.

### In progress

| | |
| --- | --- |
| [fourth-deepening.md](plans/fourth-deepening.md) | Native window lifetimes and the tooltip's frame handoff tested through their complete processes |
| [panel-entries-and-tooltips.md](plans/panel-entries-and-tooltips.md) | Shared tooltip content and lifetime, checked entry images and independent formatted numbers |
| [game-speed.md](plans/game-speed.md) | Making the game faster without changing the game it plays; how a part is chosen, and which comes next |
| [mod-capabilities.md](plans/mod-capabilities.md) | What the framework opens to mods, what each area still lacks, and the parts taking it up: signals, reads, the script group and requests as the game, tasks, the panels' actions |
| [drawing-thread.md](plans/drawing-thread.md) | The simulation, the interface and the drawing apart: the frames drawn by a thread of their own at the display's rate, the crossings made into events, then the world double buffered |
| [workshop-release.md](plans/workshop-release.md) | Harmony and mods built on it on the Steam Workshop: the core loaded from Harmony's own item, the name players read, and what the Workshop shows |

### Built

| | |
| --- | --- |
| [bounded-logs.md](plans/bounded-logs.md) | Bounded owned logs, useful event context, safe cleanup and bounded troubleshooting files |
| [v1.md](plans/v1.md) | The design of the first version |
| [area-governing-cost.md](plans/area-governing-cost.md) | General area grouping and checked governing capacity for state commands |
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
| [address-index.md](plans/address-index.md) | Every address of the game's the notes name, by its RVA, in one index that the tools read |
| [third-deepening.md](plans/third-deepening.md) | The presentation lock's turns, extensions' windows and pop-ups, a bound game element, commands' written form and the repairs' moments each gathered into one module |

### Replaced

| | |
| --- | --- |
| [game-testing.md](plans/game-testing.md) | The claim of the whole game as commands, replaced by test games; kept as it was |
