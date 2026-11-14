# Documentation

What is written down about the framework, and where. Terms are in [GLOSSARY.md](../GLOSSARY.md); how the repository is worked in is in [AGENTS.md](../AGENTS.md).

## How things are now

| | |
| --- | --- |
| [game/](game/) | What is known of the game, one page for each of its systems |
| [dev-tools.md](dev-tools.md) | The development tools, what each is for and how to use it |
| [performance.md](performance.md) | What the framework costs the game, as measured in the running game |
| [releases.md](releases.md) | Tag-triggered GitHub publication, separate downloads, source checks, retries and local Workshop uploads |
| [logs.md](logs.md) | Bounded framework logs, repeated-error summaries, event snapshots, cleanup and troubleshooting collection |
| [adding-a-capability.md](adding-a-capability.md) | How a capability is added, layer by layer |

What the capabilities are is `eu4-harmony wiki`'s; how the framework is built, its modules' own comments in the code.

## Records

| | |
| --- | --- |
| [2026-11-14-sixth-deepening.md](prototype-log/2026-11-14-sixth-deepening.md) | Sixth deepening checked in the running game: parallel saves byte-for-byte, army searches and folder listings checked, complete collection windows; the late-game scenario blocked by a mod name |
| [2026-11-14-frame-p99-user-game.md](prototype-log/2026-11-14-frame-p99-user-game.md) | Current implementation launched visibly at the main menu for the user; gameplay not measured |
| [2026-11-14-frame-p99-boundary-a1r.md](prototype-log/2026-11-14-frame-p99-boundary-a1r.md) | Replacement timing baseline: P99 44.338 ms, 576.5 completed days/min |
| [2026-11-14-frame-p99-boundary-b1.md](prototype-log/2026-11-14-frame-p99-boundary-b1.md) | First four-scope timing prototype: P99 13.166 ms, with slower daily progress and higher machine load |
| [2026-11-14-frame-p99-boundary-b2.md](prototype-log/2026-11-14-frame-p99-boundary-b2.md) | Second timing prototype repeats P99 13.248 ms at 579.5 completed days/min |
| [2026-11-14-frame-p99-boundary-a2.md](prototype-log/2026-11-14-frame-p99-boundary-a2.md) | Closing baseline: P99 43.929 ms, 581.5 completed days/min; all four games released |
| [2026-11-14-frame-p99-boundary-a1.md](prototype-log/2026-11-14-frame-p99-boundary-a1.md) | Initial timing baseline disturbed by another test game; retained separately from the replacement comparison |
| [2026-11-14-frame-p99-boundary-diag-e.md](prototype-log/2026-11-14-frame-p99-boundary-diag-e.md) | Complete four-scope trace confirms 41,197 early releases; external-game overlap excludes isolated speed attribution |
| [2026-11-14-frame-p99-boundary-diag-d.md](prototype-log/2026-11-14-frame-p99-boundary-diag-d.md) | Complete formal report but overflowing temporary probes; rejected as a full mechanism trace |
| [2026-11-14-frame-p99-boundary-diag-c.md](prototype-log/2026-11-14-frame-p99-boundary-diag-c.md) | Third boundary also releases in microseconds; the remaining formatting-chain acquisition is retained for further analysis |
| [2026-11-14-frame-p99-boundary-diag-a.md](prototype-log/2026-11-14-frame-p99-boundary-diag-a.md) | Matched baseline for the second-boundary diagnostic, with complete raw replay |
| [2026-11-14-frame-p99-boundary-diag-b.md](prototype-log/2026-11-14-frame-p99-boundary-diag-b.md) | Second boundary releases in microseconds; a later third acquisition retains the long hold |
| [frame-drawing-p99-safe-boundary.md](research/frame-drawing-p99-safe-boundary.md) | Four stream-lock boundaries, cold-path exclusion, the complete captured formatting path and guarded release experiments |
| [frame-drawing-p99-release-path.md](research/frame-drawing-p99-release-path.md) | Early release verified; the next stream-initialization call reacquires the same lock and retains long ownership |
| [2026-11-14-frame-p99-release-old-a.md](prototype-log/2026-11-14-frame-p99-release-old-a.md) | Original-view baseline with matched scope and interval traces |
| [2026-11-14-frame-p99-release-old-b.md](prototype-log/2026-11-14-frame-p99-release-old-b.md) | Original-view prototype: 1,268 actual early releases and the subsequent long holds |
| [2026-11-14-frame-p99-release-map-b.md](prototype-log/2026-11-14-frame-p99-release-map-b.md) | Beijing-view prototype confirms the mechanism, with external-game load disclosed |
| [2026-11-14-frame-p99-release-map-a.md](prototype-log/2026-11-14-frame-p99-release-map-a.md) | Beijing-view baseline closes the diagnostic comparison; no stable gain established |
| [2026-11-14-frame-p99-optimization-a2.md](prototype-log/2026-11-14-frame-p99-optimization-a2.md) | Final baseline closes the comparison without demonstrating a repeatable P99 gain |
| [2026-11-14-frame-p99-optimization-b2.md](prototype-log/2026-11-14-frame-p99-optimization-b2.md) | Second prototype comparison, including a disturbed paused control |
| [2026-11-14-frame-p99-optimization-b1.md](prototype-log/2026-11-14-frame-p99-optimization-b1.md) | First prototype comparison: complete windows and normal release |
| [2026-11-14-frame-p99-optimization-a1.md](prototype-log/2026-11-14-frame-p99-optimization-a1.md) | First baseline in the bounded lock-order scope comparison |
| [frame-drawing-p99.md](research/frame-drawing-p99.md) | Individual submission-gap attribution: long lock-order ownership, invalidated frames, map names due, and a measured short internal lock scope |
| [2026-11-14-frame-p99-probe-smoke.md](prototype-log/2026-11-14-frame-p99-probe-smoke.md) | Paused raw trace exported; driver stopped on a wrong capability name and released its game |
| [2026-11-14-frame-p99-attribution.md](prototype-log/2026-11-14-frame-p99-attribution.md) | Two five-speed windows partitioned around P99, including a popup returning long before the next preparation |
| [2026-11-14-frame-p99-lock-sources.md](prototype-log/2026-11-14-frame-p99-lock-sources.md) | Entry stacks identify the AI stream-related lock-order trigger; explicit map-name skip transitions explain another gate |
| [2026-11-14-frame-p99-lock-scope.md](prototype-log/2026-11-14-frame-p99-lock-scope.md) | Internal scope ends in microseconds while presentation-lock ownership persists for milliseconds; all runtime probes removed |
| [2026-11-14-frame-p95-baseline.md](prototype-log/2026-11-14-frame-p95-baseline.md) | Submission intervals, Present durations and drawing-log clues, with concurrent analysis load disclosed |
| [2026-11-14-frame-p95-camera.md](prototype-log/2026-11-14-frame-p95-camera.md) | Paused fixed/moving camera comparison under changing system load; repeated separately after analysis stopped |
| [2026-11-14-frame-p95-repeat.md](prototype-log/2026-11-14-frame-p95-repeat.md) | Five paused camera windows and a speed-5 repeat after analysis stopped, with another game's load and Present disturbances disclosed |
| [2026-11-15-fixed-window-backgrounds.md](prototype-log/2026-11-15-fixed-window-backgrounds.md) | Static blue-and-gold backgrounds verified at two window sizes and two button-area widths, with native click/wheel shielding, list scrolling and real controls |
| [2026-11-14-button-area-gray.md](prototype-log/2026-11-14-button-area-gray.md) | Army button-area desaturation reproduced and isolated to the background's disabled button state; diagnostic change reverted |
| [2026-11-14-performance-overhead-1-on.md](prototype-log/2026-11-14-performance-overhead-1-on.md) | First collection-on repeat: complete paused and running windows, with external rate and CPU |
| [2026-11-14-performance-overhead-1-off.md](prototype-log/2026-11-14-performance-overhead-1-off.md) | First collection-off repeat in the same build, with absent in-process metrics |
| [2026-11-14-performance-overhead-2-off.md](prototype-log/2026-11-14-performance-overhead-2-off.md) | Second collection-off repeat, beginning the reversed-order pair |
| [2026-11-14-performance-overhead-2-on.md](prototype-log/2026-11-14-performance-overhead-2-on.md) | Second collection-on repeat, whose rate ordering reversed the first pair |
| [2026-11-14-performance-overhead-3-on.md](prototype-log/2026-11-14-performance-overhead-3-on.md) | Third collection-on repeat with complete windows and no dropped events |
| [2026-11-14-performance-overhead-3-off.md](prototype-log/2026-11-14-performance-overhead-3-off.md) | Final collection-off repeat, matching build/setup evidence and release of all test games |
| [2026-11-14-performance-baseline.md](prototype-log/2026-11-14-performance-baseline.md) | First complete 30 s paused and 120 s speed-5 windows, with distinct day throughput, submissions, interface updates and time-weighted age |
| [2026-11-14-performance-native.md](prototype-log/2026-11-14-performance-native.md) | Main-thread submission attribution and local-save markers with drawing-thread and background-compression speed-ups disabled |
| [2026-11-14-performance-smoke.md](prototype-log/2026-11-14-performance-smoke.md) | Complete short paused and speed-5 windows with attributed submissions, interface ages, save markers and unfinished intervals |
| [2026-11-14-performance-startup-refused.md](prototype-log/2026-11-14-performance-startup-refused.md) | First performance-collection startup refused overlapping local-save observers; no performance window ran |
| [performance-boundaries.md](research/performance-boundaries.md) | Existing daily-update, interface-readiness and Present boundaries used by bounded performance collection, with their limits |
| [frame-drawing-p95.md](research/frame-drawing-p95.md) | Submission-interval P95: current measurements, preparation and drawing candidates, and the evidence needed to choose a change |
| [adr/](adr/) | Decisions that are hard to reverse, one file each, and why they were made |
| [research/](research/) | The research into the game's code that the pages of [game/](game/) rest on, with addresses as RVAs into the 1.37.5 baseline; [addresses.toml](research/addresses.toml) indexes every address they name |
| [prototype-log/](prototype-log/) | What was tested in the running game and what came out, one file per run |
| [2026-11-14-release-0.2.0-smoke.md](prototype-log/2026-11-14-release-0.2.0-smoke.md) | Version 0.2.0 local build, offline checks and hidden window and achievement scenarios before automated publication |
| [2026-11-14-large-mod-startup.md](prototype-log/2026-11-14-large-mod-startup.md) | Single-pass localisation, indexed replacements and interned trigger profiles checked and timed across Guixu, 2000 AD and the player's playset |
| [localisation-loading.md](research/localisation-loading.md) | `main` loading the whole localisation twice, the quadratic scans in `localisation/replace` and duplicate checking, and the triggers' optimization profile |
| [2026-11-13-guixu-startup-sampled.md](prototype-log/2026-11-13-guixu-startup-sampled.md) | Guixu and 2000 AD startups sampled and timed, showing 11–14 s in double localisation loading and `localisation/replace` |
| [2026-11-13-fifth-deepening.md](prototype-log/2026-11-13-fifth-deepening.md) | Concurrent extension, window and achievement runs plus three-process ironman setup-save handoff |
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
| [performance-metrics.md](plans/performance-metrics.md) | Completed-day, submission and interface collection built and checked; action response, pure interface-work timing and the full save comparison matrix remain deferred |
| [releases-after-version-prs.md](plans/releases-after-version-prs.md) | Merged version PRs automatically create tags and publish, retaining separate packages and local Workshop upload |
| [panel-entries-and-tooltips.md](plans/panel-entries-and-tooltips.md) | Shared tooltip content and lifetime, checked entry images and independent formatted numbers |
| [game-speed.md](plans/game-speed.md) | Making the game faster without changing the game it plays; how a part is chosen, and which comes next |
| [mod-capabilities.md](plans/mod-capabilities.md) | What the framework opens to mods, what each area still lacks, and the parts taking it up: signals, reads, the script group and requests as the game, tasks, the panels' actions |
| [drawing-thread.md](plans/drawing-thread.md) | The simulation, the interface and the drawing apart: the frames drawn by a thread of their own at the display's rate, the crossings made into events, then the world double buffered |
| [workshop-release.md](plans/workshop-release.md) | Harmony and mods built on it on the Steam Workshop: the core loaded from Harmony's own item, the name players read, and what the Workshop shows |

### Built

| | |
| --- | --- |
| [fixed-window-backgrounds.md](plans/fixed-window-backgrounds.md) | Fixed blue-and-gold backgrounds with static presentation, adaptive borders and native input blocking |
| [sixth-deepening.md](plans/sixth-deepening.md) | The parallel save's, the army searches' and the background compression's courses, the collection's clock and the folder-listing rules each gathered into one module |
| [frame-p99-safe-boundary-validation.md](plans/frame-p99-safe-boundary-validation.md) | Four verified release boundaries repeat about 70% lower P99; broader safety remains open, disposable runtime removed |
| [frame-p99-release-path-validation.md](plans/frame-p99-release-path-validation.md) | Four diagnostic games completed; early release followed by another long acquisition, with temporary code removed |
| [frame-p99-lock-scope-validation.md](plans/frame-p99-lock-scope-validation.md) | A/B/B/A validation completed; no repeatable P99 improvement, prototype removed and evidence retained |
| [automated-releases.md](plans/automated-releases.md) | First GitHub Release with separate framework and CLI packages; manual tag creation superseded by version PR automation |
| [large-mod-startup.md](plans/large-mod-startup.md) | Localisation loaded once with indexed replacements, the triggers' optimization profile interned, non-ASCII folder entries tolerated, and the drawing thread asleep while idle |
| [fifth-deepening.md](plans/fifth-deepening.md) | Mod descriptors, bounded journal reading, process handoff, resolved UI views, native entry bindings, save-data authority and drawing coordination |
| [fourth-deepening.md](plans/fourth-deepening.md) | Native window lifetimes and the tooltip's frame handoff tested through their complete processes |
| [bounded-logs.md](plans/bounded-logs.md) | Bounded owned logs, useful event context and safe cleanup; troubleshooting collection limits superseded by complete collection |
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
