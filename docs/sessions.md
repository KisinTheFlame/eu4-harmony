# Sessions plan

Status: decided (2026-11-02); S1 to S4 built and tried in the game the same day ([prototype-log.md](prototype-log.md)). Of S5, against a new process a new game and a loaded save after a return are the same as in a new process but for what the game chooses at random; the odd moments (a window open, an autosave, observing, the end-of-game summary), a failing ironman exit save and the setting off were tried; returning by the game's own question and summary crashed the game until the session's interface the map manager leaves was let go. A return after time ran crashed the game until the return took the session's interface down as the in-game load does and let the message windows' popups go, and a sprite type that sprites still show was no longer unloaded with its group (research/main-menu.md). Then 20 rounds, the odd moments, ironman and the player's own buttons ran without a crash. What the game still leaves of each session is known and left as it is in this version ([Known leaks](#known-leaks)). Terms are in [GLOSSARY.md](../GLOSSARY.md) (Sessions). The decisions and their reasons are in [ADR 0022](adr/0022-return-to-the-main-menu-in-process.md) (returning to the main menu without a restart) and [ADR 0023](adr/0023-in-game-load-in-ironman.md) (an in-game load in ironman, and undo). What the game does is in [research/main-menu.md](research/main-menu.md).

## Goal and scope

- **Goal**: leaving a session, for the main menu or for another save, without restarting the game, for the player and for remote control alike.
- **In scope**: the in-process return; the in-game load from remote control, in ironman too, and the player's in-game load in ironman; a new game started from a session by remote control; the "session ended" event for extensions; two framework settings and the toggle they are shown with; what the player sees meanwhile; the continue box after an in-process return.
- **Out of scope**: extensions starting any of this (the main menu stays a capability of remote control only, ADR 0015); multiplayer; cloud saves.

## Behavior

### Returning to the main menu

- With "return without restarting" on (the default), every return is an in-process return: the game's question, the end-of-game summary and `menu.return` alike. Off, the game restarts as vanilla does.
- An ironman session is always left with its ironman exit save. When that save fails, the player stays in the session and is told so in a notice; `menu.return` is answered with the reason.
- The session is taken down as the in-game load takes it down, its interface, then the world reset, here with the game's history, and the main menu is made after it, as at startup (ADR 0022). What the world reset and the map manager leave of the session is put back or let go by the framework.
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
- There is no fallback to a restart (ADR 0022).

### What remote control answers

- `menu.load` and `menu.start` from a session are answered as from the main menu: once the new session's first frame has run, with its number, date and player; or `rejected` with the game's reason.
- `menu.return` with "return without restarting" on is answered once the main menu is up, `place` `main_menu`, from the same process, with event numbers going on; off, at once with `place` `ending`, as now.

### Extensions

A new event, "session ended", with its reason (a return to the main menu, an in-game load, a new game), comes before the next session begins; it is appended to the ABI (ADR 0014). It comes for the game's own in-game load outside ironman too. The ABI's and SDK's comments that call a session a "campaign" are reworded.

### What the player sees

From the moment a return or a load begins until the next frame the game draws, the framework UI shows a still notice in the game's window: "Returning to the main menu…" or "Loading the save…".

### Framework settings

Two toggles, both on by default and both taking effect when the settings are saved, without a restart: "return without restarting" and "save an ironman game before an in-game load". The toggle is a new widget of the framework UI; the map outline, a yes-or-no drop-down list until now, becomes one too. The settings window's note that changes take effect at the next start stays next to the font settings only.

## How it fits together

One module in the core takes every way of leaving a session: the player's (the return question, the end-of-game summary, the load button) and remote control's (`menu.return`, `menu.load`, `menu.start`), so the player and remote control take one path.

```rust
pub enum Destination { MainMenu, Save(String), NewGame([u8; 3]) }
pub fn go(caller: Caller, to: Destination) -> GResult<Expected>;
```

Inside, a pure function lays out the steps from where the game is, where it is going and the two settings (the checks, the ironman exit save, taking the session down, the world reset, the load or the start), with unit tests of its own; an executor carries them out against the game and answers.

## Known leaks

Decided 2026-11-03: left as they are in this version. Each is the game's own: a restart hides it, and the game's own in-game load leaves its share in vanilla too. Measured 2026-11-02 with the GUI root's elements, the process's private bytes and the heap census ([prototype-log.md](prototype-log.md), "A sprite type unloaded while shown, and the stability runs"):

- **Each return**, with the session it left: 8 to 9 MB of the game's heap (46,000 to 47,600 blocks) and about 1,030 GUI elements; the process's private bytes grow about 15 MB. Handles do not grow.
- **Each in-game load**: 5 to 7 MB of the game's heap and 300 to 400 GUI elements.
- **What it comes to**: ten returns in one process keep about 150 MB until the game is closed. Twenty rounds of returns, loads and new games ran without a crash.
- **Where it comes from**:
  - what the game's AI makes for every session where `ai_king.cpp` logs "Start new agents": 747 blocks of 0xbf8 bytes, about 2.2 MB, each time, the largest part;
  - blocks of 0x178 bytes from two sites, 0.6 to 1 MB each, 1,585 blocks of 0x120 bytes, and smaller sites, about forty in all; none was followed to what owns it;
  - windows the map manager's destructor leaves: the two plain structs, which crash the game when the framework lets them go, and windows not found yet, among them those still showing the building icons (`macrobuildinterface.gui`) and the government reforms (`countryreformview.gui`).
- **Already let go by the framework** ([research/main-menu.md](research/main-menu.md)): the countries' arrays, the mercenary companies' leaders, the rebel factions, the session's interface and the message windows' popups.
- **To take them up later**: the census (the `trace` feature, `EU4HARMONY_CENSUS` set) counts what each world reset's generation leaves by call site; research/main-menu.md lists the sites and how far each was followed.

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
