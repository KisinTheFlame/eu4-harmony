# Sessions plan

Status: decided (2026-11-02); S1 to S4 built and tried in the game the same day ([prototype-log.md](prototype-log.md)). Of S5, against a new process a new game and a loaded save after a return are the same as in a new process but for what the game chooses at random; the odd moments (a window open, an autosave, observing, the end-of-game summary), a failing ironman exit save and the setting off were tried; returning by the game's own question and summary crashed the game until the session's interface the map manager leaves was let go. A return after time ran crashed the game until the return took the session's interface down as the in-game load does and let the message windows' popups go, and a sprite type that sprites still show was no longer unloaded with its group (research/main-menu.md). Then 20 rounds, the odd moments, ironman and the player's own buttons ran without a crash. What the game still leaves of each session is known and left as it is in this version ([Known leaks](#known-leaks)). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Sessions). The decisions and their reasons are in [ADR return-to-the-main-menu-in-process](adr/return-to-the-main-menu-in-process.md) (returning to the main menu without a restart) and [ADR in-game-load-in-ironman](adr/in-game-load-in-ironman.md) (an in-game load in ironman, and undo). What the game does is in [research/main-menu.md](research/main-menu.md).

## Goal and scope

- **Goal**: leaving a session, for the main menu or for another save, without restarting the game, for the player and for remote control alike.
- **In scope**: the in-process return; the in-game load from remote control, in ironman too, and the player's in-game load in ironman; a new game started from a session by remote control; the "session ended" event for extensions; two framework settings and the toggle they are shown with; what the player sees meanwhile; the continue box after an in-process return.
- **Out of scope**: extensions starting any of this (the main menu stays a capability of remote control only, ADR remote-control-uses-capabilities); multiplayer; cloud saves.

## Behavior

### Returning to the main menu

- With "return without restarting" on (the default), every return is an in-process return: the game's question, the end-of-game summary and `menu.return` alike. Off, the game restarts as vanilla does.
- An ironman session is always left with its ironman exit save. When that save fails, the player stays in the session and is told so in a notice; `menu.return` is answered with the reason.
- The session is taken down as the in-game load takes it down, its interface, then the world reset, here with the game's history, and the main menu is made after it, as at startup (ADR return-to-the-main-menu-in-process). What the world reset and the map manager leave of the session is put back or let go by the framework.
- The single player setup's back button, which restarts the game in vanilla, goes back to the main menu the same way.
- The continue box offers the save the game wrote last, as after a restart.

### An in-game load

- From a session straight into the save asked for, with no main menu between: the player's load button, in ironman too, and `menu.load` from a session.
- Outside ironman nothing is saved first, as in vanilla.
- In ironman, "save an ironman game before an in-game load" (on by default) decides whether the ironman exit save is written first. Off, loading the same save again is an undo. The game's question that confirms the load keeps its words, except in ironman with the setting on, where it says that the current game is saved first.
- The session begun takes ironman from its save.

### A new game from a session

`menu.start <tag>` from a session leaves it as a return to the main menu does (with its ironman exit save) and starts the game as from the main menu.

### When something fails

- What can be found out before the world is touched is checked first, and leaves the session as it was: the save is not there, its header cannot be read or is of another version, it needs mods the playset lacks, the ironman exit save failed.
- What only shows once the world has been reset, such as a game state that cannot be read, cannot go back: the game ends up in the main menu, and the request is answered `rejected` with the reason.
- There is no fallback to a restart (ADR return-to-the-main-menu-in-process).

### What remote control answers

- `menu.load` and `menu.start` from a session are answered as from the main menu: once the new session's first frame has run, with its number, date and player; or `rejected` with the game's reason.
- `menu.return` with "return without restarting" on is answered once the main menu is up, `place` `main_menu`, from the same process, with event numbers going on; off, at once with `place` `ending`, as now. Requests made before the map manager's frame acts on the first are the same return, and each is answered with it.

### Extensions

A new event, "session ended", with its reason (a return to the main menu, an in-game load, a new game; or another country, when the player's country changes on the same world, as by the game console's `tag`, which begins a session too), comes before the next session begins; it is appended to the ABI (ADR abi-grows-by-appending). It comes for the game's own in-game load outside ironman too. It comes once leaving is certain: after the ironman exit save, when there is one, so that a session whose exit save failed, which goes on, is not told it ended. The ABI's and SDK's comments that call a session a "campaign" are reworded. With Lua extensions the ABI and SDK went: the event is the handler `on_session_ended(game, reason)`, `reason` being `return`, `load`, `new_game` or `country`.

### What the player sees

From the moment a return or a load begins until the next frame the game draws, the framework UI shows a still notice in the game's window: "Returning to the main menu…" or "Loading the save…". The main menu of an in-process return presents the loading screen's painting as it takes over, as at startup; those frames are held back, so the notice stays until the main menu's first frame.

### Framework settings

Two toggles, both on by default and both taking effect when the settings are saved, without a restart: "return without restarting" and "save an ironman game before an in-game load". The toggle is a new widget of the framework UI; the map outline, a yes-or-no drop-down list until now, becomes one too. The settings window's note that changes take effect at the next start stays next to the font settings only.

## How it fits together

One module in the core takes every way of leaving a session: the player's (the return question, the end-of-game summary, the load button) and remote control's (`menu.return`, `menu.load`, `menu.start`), so the player and remote control take one path.

```rust
pub enum Destination { MainMenu, Save, NewGame }
pub fn plan(to: Destination, ironman: bool, rules: Rules) -> Result<Vec<Step>, &'static str>;
```

`plan` (`core/src/game/menu/leave.rs`) is a pure function: it lays out the steps from where the game is, where it is going and the two settings (the checks, the ironman exit save, the session's end told, taking the session down, the world reset, the load or the start), with unit tests of its own; an executor carries them out against the game and answers. The save's name and the new game's country go with the request, not with the destination.

What those steps and the safe points see of a session, its first game frame, its end with the reason, a save beginning to be read, a world reset, the game's restart, goes to one module of the core, `sessions` (`core/src/sessions.rs`). It keeps whether a session is going on, where the safe point is and whether a save is being read, in a state machine with tests of its own, and tells each thing to the parts that keep something of a session in one order: remote control keeps the end as an event before extensions hear it, and a session ends once, before the next begins.

## Known leaks

Decided 2026-11-03: left as they are in this version. Each is the game's own: a restart hides it, and the game's own in-game load leaves its share in vanilla too. Measured 2026-11-02 with the GUI root's elements, the process's private bytes and the heap census ([prototype-log.md](prototype-log.md), "A sprite type unloaded while shown, and the stability runs"). A leak run ([leak-finding.md](leak-finding.md)) lists what is left now, site by site:

- **Each return**, with the session it left: 8 to 9 MB of the game's heap (46,000 to 47,600 blocks) and about 1,030 GUI elements; the process's private bytes grow about 15 MB. Handles do not grow. Since the AI's agents are let go too (2026-11-03): 6.5 to 6.9 MB of the heap (43,400 to 44,400 blocks); since the map modes' window and the outliner's entries are let go too, 6.1 to 6.7 MB (41,000 to 44,400 blocks) and about 1,170 to 1,360 objects in the GUI root. The private bytes, 2026-11-03, over twenty rounds as below: about 29 MB a round, against 32 MB before (prototype-log.md); since the sets of modifiers are let go too, 23 to 25 MB a round; since the heaps give back their free pages after each world reset, about 21 MB; since the textures' transparency masks keep no more than their textures need, 20.4 to 20.9 MB. Measured with the memory account (2026-11-04), rounds as a leak run plays them: 17.7 MB a round, and since the text boxes' animations are let go too, 15.5 MB over the same rounds and 14.1 MB over twenty, the game's heap holding 7.3 to 7.5 MB of it against 9.2. Since the monarchs, heirs and queens the histories leave are let go too (2026-11-06): 12.2 MB a round over ten rounds and 12.3 MB over twenty, the game's heap holding 5.8 MB of it; the census's leftovers grow 5.7 MB a round against 7.7. Since the battles a war's history leaves are let go too: the game's heap holds 5.5 MB a round over ten rounds, the census's leftovers grow 5.4 MB; the private bytes, 13.8 to 15.6 MB a round, vary with the heaps' committed pages no block uses. Since the session's panels' windows are killed too: the game's heap holds 2.6 to 2.8 MB a round over ten rounds, the census's leftovers grow 2.2 MB; the private bytes, 13.3 to 14.1 MB a round, are the heaps' committed pages no block uses, 12.2 to 13.3 MB of them. Since what a country's first history lists alone is let go too (2026-11-07): the game's heap holds 1.2 MB a round over ten rounds; the private bytes grew 10.4 MB a round, 10.7 MB of it the heaps' committed pages no block uses. Since the relations of the diplomatic histories and the trade nodes are let go too: 0.78 MB a round over fifteen rounds, the private bytes 8.2 MB, 6.9 MB of it the heaps' unused pages. With the session's panels' windows no longer killed (2026-11-07, which ended the game about once in ten rounds): 3.9 MB a round over twenty rounds, the private bytes 10.1 to 10.3 MB.
- **Each in-game load**: 5 to 7 MB of the game's heap and 300 to 400 GUI elements; since the agents are let go, 4.0 to 4.2 MB of the heap; since the map modes' window and the outliner's entries are, 3.7 to 4.1 MB.
- **Where the private bytes go** (2026-11-03): the game's heap keeps about 12 MB of blocks a round, what the census counts, and kept as many again of free pages committed around them until the framework had the heaps give them back; what lies outside the heaps does not grow. Measured again on 2026-11-04 with the memory account, without the census, ten rounds: 17.7 MB a round, of it 9.2 MB blocks the game's heap holds, 7.3 MB free pages its heap still commits after the framework had it give them back, and 1.2 MB outside the heaps ([prototype-log.md](prototype-log.md), "Where each round's growth goes"). On 2026-11-07, with what the game holds down to 2.1 to 2.4 MB a round: what the heap commits and does not use grows with the small free blocks among what the sessions leave, 6 to 7 MB a round, about three times their bytes; its free blocks of up to 1 MB, 60 to 155 MB, are taken again by the next session, and compacting the heap gave them back at the main menu without lowering the growth ([prototype-log.md](prototype-log.md), "What the heap's free pages are, after the windows").
- **What it comes to**: ten returns in one process keep about 150 MB until the game is closed. Twenty rounds of returns, loads and new games ran without a crash.
- **Where it comes from**:
  - the list entries of the interface's panels, whose holders stay after their panel goes, about 300 a round, about 50 of them with their windows (2026-11-03);
  - smaller sites, about forty in all, and buffers of vectors and strings the census cannot follow;
  - windows of the session's interface, about 75 a return, 0.2 MB, counted at the main menu once the interface is gone (the census at the world reset counts the interface still in use with them): the two plain structs the map manager's destructor leaves, which crash the game when the framework lets them go, and windows whose holders never go, among them the building interface's (`macrobuildinterface.gui`), the government reforms' choosers (`countryreformview.gui`), the papacy's, the ledger's and the tutorial's hints. From 2026-11-06 the windows the map manager's constructor and its interface's builds make were killed at a return once it was gone, 70 to 105 of them, taken out on 2026-11-07 as it ended the game about once in ten rounds (research/main-menu.md, "The session's panels' windows"); what stays of the windows, about 10 KB a generation, is held by blocks of `0x1108700` and `0x13b64e0`, and an in-game load still leaves the build before's until the return.
- **Already let go by the framework** ([research/main-menu.md](research/main-menu.md)): the countries' arrays, the mercenary companies' leaders, the rebel factions, the agents the countries' AI started (about 2.2 MB a session, the largest part before), the session's interface, the message windows' popups, the map modes' window, the bar of the units selected and the outliner's entries (each also at every interface rebuild), the sets of modifiers a country's modifiers copy (in play too, 0.6 to 1 MB each from two sites a session), what the textures' transparency masks keep beyond their textures (about 1 MB a session), the monarchs, heirs and queens a country's history leaves (about 5,300 a round, deleted at the first frame after a world reset rather than where the history goes, which crashed the game) and the battles a war's history leaves (about 770 a round, deleted the same way), what a country's first history lists alone, which its clear empties without deleting (9,413 blocks and records after each new game, deleted as the clear empties it), the relations the diplomatic histories leave (911 to 917 after each new game or load, deleted at the world's clear after the next return), the trade nodes each world clear replaces (80, deleted the same way),  the text boxes' animations (in play too, about 0.6 MB a generation with their buffers, the most of any site before; [research/interface-bindings.md](research/interface-bindings.md#text)).
- **To take them up later**: the heap census (a developer build's `dev-heap-census`) counts what each world reset's generation leaves by call site; research/main-menu.md lists the sites and how far each was followed.

## Parts

### Every part is done when

- `cargo fmt`, the build, the tests and clippy pass, and so do the offline checks against `eu4.exe`;
- what it delivers was tried in the game, with the run recorded in [prototype-log.md](prototype-log.md);
- docs/research holds what was found in the game's code, and README.md and README.zh-CN.md say what changed for players.

### S1 Toggle and settings

The toggle widget; the map outline as a toggle; the two new settings, taking effect when saved.

### S2 One way out of a session

The module above, with `Destination::Save` from a session outside ironman through the game's own in-game load; `menu.load` from a session; the player's load button going through the module; the "session ended" event; the still notice while loading.

### S3 Ironman

The load button in ironman; the ironman exit save before an in-game load and its setting; the confirming question's words; staying in the session when the ironman exit save fails.

### S4 In-process return

The in-process return, with the world reset with the history; what the world reset and the map manager leave put back or let go; the single player setup's back button; the continue box read again; `menu.return` answered from the same process; `menu.start` from a session; the still notice while returning.

### S5 Acceptance

Run by remote control; any of these failing is a defect to fix before the change is merged:

1. **Against a new process**: a new game started after an in-process return from a late save, saved at once, is the same as one started and saved in a new process, but for times and the random seed. A save loaded after an in-process return gives the same first save as one loaded in a new process.
2. **Over and over**: returning, loading, starting and loading in a session, 20 times round, without handles growing and without a crash, and memory growing by no more than the [Known leaks](#known-leaks).
3. **At odd moments**: returning with one of the game's windows open, during an autosave, while observing, and from the end-of-game summary.
4. **Ironman**: with the setting on, the ironman exit save is written before the load; with it off, loading the same save gives the session as that save holds it; when the ironman exit save fails, the player stays in the session with a notice.
