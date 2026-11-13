# Fifth deepening plan

Status: decided on 2026-11-13 from an architecture review of the codebase after the [fourth deepening](fourth-deepening.md), all seven parts authorized together and built the same day; tried in hidden test games ([2026-11-13-fifth-deepening.md](../prototype-log/2026-11-13-fifth-deepening.md)), with all test games released. No glossary terms or new ADRs are needed.

## Goal and scope

Gather the protocol knowledge and lifetime rules still split across callers after the [fourth deepening](fourth-deepening.md) into modules whose interfaces carry and test the whole course:

- Paradox `.mod` descriptors, `dlc_load.json` playsets, and private mod folders in claims (`manifest`, `core/src/mods.rs`, `cli`, `xtask/src/claim`);
- bounded journal reading, directory marks, artifact classification, failure evidence, and handler-error deduplication (`logging`, `core`, `lua`, `client`, `xtask`);
- test-game process handoff across `claim::Machine`, the restart follower, and concurrent game counting (`xtask/src/claim`, `xtask/src/scenario.rs`);
- resolved views and pressability of extension windows, list rows, entry buttons, pop-ups, and hosted elements across the running game and the fake game (`lua`, `core`, `cli`);
- native panel-strip entry placement and host-window subscriptions behind tested seams (`core/src/windows`, `core/src/ui.rs`, `core/src/bound_element.rs`);
- extension save-data authority across nested loads, pre-session ironman setup saves, session start, and extension reload (`core/src/saves.rs`, `core/src/lua/mod.rs`);
- the drawing thread's prepared-frame handoff, crossing hold-back, deferred calls, pacing, and staged map icons (`core/src/drawing`).

It leaves capabilities, Lua layout declarations, dependency points, the game's drawing order, and the thread division of [ADR simulation-and-interface-take-turns](../adr/2026-11-10-simulation-and-interface-take-turns.md) as they are.

## What the game does

- The game reads `.mod` descriptors from `<userdir>/mod/` and the enabled playset from `<userdir>/dlc_load.json`; a descriptor's `path=` or `archive=` may be absolute, so a test game's private `user/mod/` folder needs no NTFS junction into the player's `mod/` folder.
- When loading an ironman save, the game writes a setup save on the main thread before the session's first safe frame ([game/sessions.md](../game/sessions.md), [research/saves.md](../research/saves.md)), before extensions have heard `on_session`.
- The panel button strip (`floating_buttons`) lays out `f_button_1` to `f_button_8` first; extension entry buttons stand left of the shown ones, with an independent text box for each entry's formatted number ([research/panel-entry-text.md](../research/panel-entry-text.md)).
- During a frame's GUI update, placed map icons are collected on the main thread for the drawing thread's camera follow; a frame drawn on the main thread or aborted during preparation does not publish a new prepared frame ([plans/drawing-thread.md](drawing-thread.md)).

## Behavior

- **Fake game (`eu4-harmony test`)**: pressing a button that is hidden or disabled (a window button, a button in a hidden list or disabled row cell, an entry button, or a hosted element explicitly hidden or disabled) raises a test error, as pressing a closed window does. `mod:window(name)` returns the resolved view of declared widgets, list rows, and `entry` (merging layout defaults, resolving localisation in the current player language, and giving `entry.text` beside `entry.value`). `mod:element(name)` resolves `Text` descriptions dynamically in the current player language.
- **Extension error logs**: the first three occurrences of a handler error at the same place in a session are written in full into the extension's bounded journal and event snapshot; further occurrences are counted and summarized by the bounded journal at periodic and session checkpoints without a duplicate end-of-session error line.
- **Claims and scenarios**: a claim's `user\mod\` is created directly as an ordinary private folder of relocated `.mod` descriptors; no new NTFS junction into the player's `mod\` folder is made, while release still unlinks any reparse point left by an older claim. Scenario failure and crash reports include framework log panics, fatals, and `crash:` records even when `play` stops early without a game crash folder, and `games_beside` counts games running once the scenario's own game has started.

## Parts

**G1. Descriptors, playsets, and junction-free claim mod folders.** Move `.mod` descriptor parsing, location stripping, path relocation, descriptor-path comparison, and `dlc_load.json` playset updates into `eu4-harmony-manifest`. Replace hand-rolled copies in `core::mods`, `cli`, and `xtask::claim::machine`, and populate each claim's `user\mod\` directly as a private folder without `mklink /J` or `mod-registry\`.

**G2. Bidirectional bounded journals and failure evidence.** Extend `eu4-harmony-logging` with typed sources, log-directory artifact classification, structured line parsing, directory-level marks across segments and fallback stems, and failure-evidence extraction across `core`, `core-emergency`, and `proxy`. Replace reverse-parsers in `core/src/troubleshooting.rs`, `client/src/start.rs`, and `xtask/src/scenario/`, and align `Extension` handler-error counting with `Journal`.

**G3. Value-owned test-game handoff, tested restart following, and live concurrency counts.** Carry the started game handle by value from `Machine::start_game` into `Machine::follow`, removing `static STARTED` and `Machine::process`. Put `follow_games` behind a two-adapter process/pipe seam with deterministic tests, and sample `games_beside` while the scenario's game runs.

**G4. Resolved window, entry, and hosted-element views shared with the fake game.** Deepen `eu4_harmony_lua::windows` (and shared hosted-element state) to resolve widget, row, entry, and pop-up defaults and player-language text in one place and check button pressability. Use it from `core/src/windows/`, `core/src/ui.rs`, and `cli/src/{fake,test}.rs`, dropping the stale `words` cache in `core/src/windows/mod.rs`.

**G5. Native panel-strip entries and host-window subscriptions behind tested seams.** Bring panel-strip entry binding, slot compaction, independent number placement, teardown, and `ui.rs` subscription rollback behind internal native seams with both game and recording/failure-injecting test adapters, retaining callbacks before native subscription.

**G6. Extension save-data lifecycle in one module.** Replace the five globals and cross-module mutation helpers in `core/src/saves.rs` and `core/src/lua/mod.rs` with one `SaveData` state machine covering nested load frames, pre-session ironman setup saves before `on_session`, session start, live main-thread saves, off-main-thread saves, and extension reload.

**G7. Drawing-thread frame, crossing, and staged-icon coordination.** Consolidate prepared-frame publication, main-thread fallback, crossing hold-back, lock-order tracking, deferred-call reentry guards, frame pacing, and staged `icons::PLACING` cleanup into a tested in-process coordinator alongside `turns.rs`.

## Every part is done when

Each deepened module replaces the scattered logic in its callers rather than sitting beside it, and is tested through its own interface. Formatting, the locked release build, workspace tests, both clippy configurations, and offline checks pass. Hidden claimed test games exercise the changed paths (`extensions`, `windows`, `sessions`, and concurrent scenario accounting), recorded in `docs/prototype-log/` with unverified limits stated plainly, and all parts ship together in one pull request.
