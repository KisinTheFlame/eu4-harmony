# 2026-11-04 A new game after an in-process return is a session (tested by me)

The build of `claude/session-after-return`, which begins a session at each world reset with the history (an in-process return, the single player setup left). Regional combat as the extension to watch, its manifest set to api_version 0.2 for the run (as master provides) and put back after; `eu4.exe` started directly.

- **Before**: in the runs of the same night ([Extension reload](2026-11-04-extension-reload.md)), France, `menu.return`, then `menu.start FRA` began a game the framework took for the old session: no "session N started", no `on_session`, the number still 1.
- **After**: France (session 1), a return, Castile (3), France from that session (5), `menu.load autosave` from it (7, "from a loaded save: true"), France (9), a return, France again (10), France from that session (11). Each logged "session N started", each ended with the session's timing line, and regional combat's `on_session` ran each time ("session N: restored 0 task(s)").
- **The numbers**: a session begun by a load, or with another world or player, is counted twice (by the load or the world reset, and by the frame that finds the world or player changed), so the numbers rise by 2 there and by 1 for France after France. They only ever rise, which is what extensions and remote clients rely on.
- **Not verified in the game**: leaving the single player setup by its back button, then starting a game.
