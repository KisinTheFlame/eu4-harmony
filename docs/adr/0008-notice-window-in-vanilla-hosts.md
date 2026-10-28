---
status: superseded by ADR-0013
---

# The notice window lives in always-present vanilla host windows

The framework's notice window (background, title, body text, close button) is spliced, the same way as mods' interface, into two vanilla windows that always exist: `country_selection_panel` in the main menu and `topbar` in the game. Every frame the framework finds the current root window and binds to it; when the window is destroyed, it unbinds at the entry of the root window's Kill (`0x1547520`). The look uses the framework's own textures and is deliberately different from vanilla dialogs. The evidence is in [research/notice-window.md](../research/notice-window.md).

## Considered Options

- **Call CreateWindow to create a window of the framework's own** (what the final design originally said): the window belongs entirely to the framework, but the framework has to manage when it is created, its registration for per-frame updates and its destruction itself, and has to handle the two interfaces, main menu and in-game, separately.
- **A Windows message box**: the simplest, but it pops up outside the full-screen game, where the player easily misses it, and it interrupts the game. Kept as the fallback.

## Consequences

- The notice window follows its host's lifetime: it is rebuilt once per game, and the root pointer changes after a `reload` of the host's `.gui`; the binding logic is shared with the army panel.
- When `topbar` slides off the screen (the timeline map mode, the army reorganization window), the notice window slides away with it.
- When another mod renames or removes a host window, the splice fails and binding fails, and notices fall back to the message box.
