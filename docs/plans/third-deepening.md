# Third deepening plan

Status: decided (2026-11-11) from an architecture review of the parts changed most since the [second deepening](second-deepening.md), and built the same day, every part; tried together in the game ([prototype-log/2026-11-11-the-third-deepening.md](../prototype-log/2026-11-11-the-third-deepening.md)). F3 left the entry buttons' placing in `windows/live.rs`, which skip a method they do not know rather than refuse it; F5 left the walks that let go of what they walk (the allies to take a province, the outliner's entries, the message windows) and the route's strict read. The development tools declared once, as the speed-ups are, was set aside: each tool's options are a type of their own, and the places a tool is called from are the moments it needs; one table of moments for them would be a seam with one adapter.

## Goal and scope

- **Goal**: as in the first two deepenings, knowledge that is written out in several places today, or spread over hooks with no interface of its own, lives in one module behind a small interface, so that a change or a fix lands once and is tested at that interface, through the same seam its callers use.
- **In scope**: the parts below, in `core`, `lua` and `cli`.
- **Out of scope**: the capability catalogue and its groups, which the second deepening left deep; the interface elements extensions splice into vanilla windows as the fake game keeps them, since the names they may set come from the core's splicing of a mod's interface fragments, which the fake game does not read; and the development tools, as the status line says.

## Behavior

Nothing changes for players or remote clients. For mod authors, the fake game of `eu4-harmony test` refuses what the game refuses of an extension's windows and pop-ups (F2): a window opened or a pop-up shown outside a session, more than 32 pop-ups open at once, a button of a pop-up that is not open; and it closes them all as a session ends, as the game does.

## Parts

Each part is one module, or one module made deeper. They change different files, but F3 builds on F2's module, so F3 comes after it.

**F1. The presentation lock's turns in one module.** Which thread holds the presentation lock when is decided across nine hooks through statics: an application frame inside another, the day letting the lock go and taking it back, a crossing taking it until the next check for a frame, the drawing thread letting it go for Present but not in a frame the write watch holds the other threads for, and the handshake as the game quits. None of the drawing's modules has a test, and its recent defects were in when these pieces are called. One module keeps the main thread's side and the rules of the turns: the hooks tell it the moments (an application frame begins and ends, the day begins and ends, a crossing, the check for a frame, the game quits), and it takes and gives the lock through a small interface, the ticket lock in the game and a recorder in the tests. The tests drive whole courses of moments and check who holds the lock after each.

**F2. What extensions want of their windows and pop-ups, shared by the core and the fake game.** The rules of an extension's windows and pop-ups (which windows it declared, a big panel closing the others, opening only in a game, at most 32 pop-ups, a button pressed twice before its action ran counting once, a pop-up closed only by its own extension, everything closed as the session ends) are kept in `core/src/windows/` and written again, looser, in the fake game's store, so that a mod's test can pass in the fake game where the game refuses. The state and its rules move into the Lua host's crate, beside the layouts they check against, with whether a session goes on told by the host; the core makes the game's windows show what it holds, and the fake game keeps the same state and reads it back for the tests.

**F3. A bound game element in one module.** How the framework sets what one of the game's elements shows (a text, hidden by writing it empty, shown, enabled, a frame, a tooltip given only where the game's left it empty), with each method checked against the one it expects, and how it hears the element's clicks, with their modifiers, into a bounded queue, is written twice, in `ui.rs` for elements spliced into vanilla windows and in `windows/live.rs` for extension windows, and the two have drifted apart (one lets go of the game string it wrote, the other does not). One module holds a bound element; both become its callers.

**F4. A command's written form written by the command layer.** The command layer reads a command's written form back and checks it, but every capability group writes its commands by hand, 54 `format!`s in 25 files, each encoding IDs, tags and nested blocks itself, and the `{ id= type= }` of a unit three times. The layer gains a small writer of the written form, sharing the tokens its reading back uses; the groups write their commands through it, and what their tests expect of the text stays as it is (ADR one-command-layer-in-written-form).

**F5. Repairs' moments declared, and lists read through `containers`.** Two of the moments repairs act on, the main menu shown and the history read after a reset, are called by hand from `safe_point` and `sessions` rather than declared in `Repair` with the others; they join the declaration. Three lists are still walked by hand (a country's leaders, a defeated side's units, the armies aboard a fleet), with bounds and loop checks of their own or none; they are read through `containers`, whose rules (bounded, a loop refused, a removed node left out) then hold for them. The walk of the allies to take a province stays as it is, since it frees each node as the game does.

## Done when

Each part has replaced what it gathers, not added beside it; its tests go through its interface; the workspace builds, tests and passes both clippy runs; the offline checks pass; and the game, tried through the scenarios, behaves as before ([prototype-log/](../prototype-log/README.md)).
