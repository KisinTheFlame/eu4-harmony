# 2026-11-06 Watching the game a claim starts (tested by me)

`cargo xtask game start --visible`, which shows the hidden start's window at the main menu through `desktop.show`, with and without `--sound`. The player build of this branch installed, the game claimed with `cargo xtask game`; three starts, each ended with `game stop`.

- **`--visible --sound`**: at the main menu after 30 s, then "the window is shown". `desktop.get` answered `hidden: false`, `hidden_start: true`, `sound: true`. The game's window, looked at from another process, was visible, not cloaked (`DWMWA_CLOAKED` 0) and not a tool window, so on the desktop and the taskbar.
- **`--visible`**: at the main menu after 32 s, shown; `desktop.get` answered `hidden: false`, `sound: false`.
- **No option**: at the main menu after 33 s; `desktop.get` answered `hidden: true`, `sound: false`, as before.
- **Not verified**: the window's cloak and style in the start without sound, which goes through the same `desktop.show`; that sound was heard, rather than answered by `desktop.get`; whether the window took the focus, which `desktop.show` does not ask for.
- **Cleanup**: releasing the game put back the player's files and the game folder's framework files, nothing reported not put back.
