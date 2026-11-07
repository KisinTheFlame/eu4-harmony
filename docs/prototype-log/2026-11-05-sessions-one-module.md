# 2026-11-05 Sessions told from one module (tested by me)

The player build of the branch that moved what begins and ends a session into `core/src/sessions.rs`, the capability catalogue's groups into `core/src/catalogue/` and each repair into a file of its own. One process, within a claim, started hidden; the player's playset, with the regional-combat extension loaded (Lua). Driven by `eu4-harmony game`.

- **Startup**: 814 dependency points matched; 147 hooks, 12 writes; the extension loaded.
- **A new game**: `menu.start FRA` answered session 1; `time.advance 5` stopped at the game's question, which `question.answer` answered; `time.advance 5` then gave five `day` events. `army.list` read the armies.
- **An in-game load**: `menu.load H-Test` answered session 3. The events said `session_ended` with `load`, then `session` 3; the log said `session 1 ending (load)` before `session 3 started (from a loaded save: true)`, with the extension's session times between.
- **A return**: `menu.return` answered at the main menu; `session_ended` with `return`; the process number stayed, an in-process return.
- **A new game after it**: `menu.start CAS` answered session 5, and its days came.
- **The extension**: heard `on_session` for sessions 1, 3 and 5 (its own log), with no error and no refusal; its handlers took under 0.5% of each session.
- **Then, in a second claim** (two processes, the same build):
  - **Extension reload**: in `Castilla-5`, an ironman game loaded from the main menu, `eu4-harmony reload` in the regional-combat mod answered "on_session got what save_data gave": the reload saw the session going on.
  - **A failing ironman exit save**: with `Castilla-5.eu4` held open by another process, `menu.return` was refused twice ("the ironman exit save faulted"), the session staying and the notice shown; once the file was free, the third wrote the save and returned in process. The log said `session 2 ending (return)` once, at the first try, and the events had one `session_ended`: the later tries were not told again.
  - **The single player setup's back button**, clicked with `input`: back to the main menu in the same process ("the single player setup was left; the world reset with the history"); `menu.start FRA` after it answered a new session, 5.
  - **A restart**: with `return_without_restart=no` in the framework settings and the game started again, `menu.return` answered `ending`; the old process logged `session 1 ending (return)`, "the game starts eu4.exe again" and remote control answering every request `not_ready` from then on; a new process answered `main_menu` on the pipe. The new process was not hidden: the game's restart builds its command line again without `-harmony_hidden`, so it showed on the desktop until it was stopped, about a minute and a half. That is the hidden start's, not this branch's.
- **Not tried**: a frame of a game that cannot be played (observing).
- **Cleanup**: each `release` put back every file the run changed (the framework's logs and settings, `pdx_login_cache.txt`, `settings.txt`, `shortcuts.txt`, the saves the loads and the ironman exit save touched) and compared each equal, and put back the game folder's framework files. The game did not crash.
