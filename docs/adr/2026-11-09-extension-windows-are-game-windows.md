# Extension windows are the game's own windows

An extension's windows and pop-ups are windows of the game's own interface, which the framework builds from window definitions it writes out of the extension's layout, declared in Lua. Mods write no `.gui` file for them. They look like the game's panels and dialogs because they are made of the game's own sprites, fonts, tooltips and sounds, and they follow a mod that changes those; the Unicode module draws their text as it draws the game's. The framework UI stays the framework's own (ADR framework-draws-its-own-ui).

## Considered Options

- **Open the framework UI to extensions, skinned with the game's sprites**: the framework would control every pixel, but lists, tooltips, the game's text formatting and country flags would all be written again, and the windows would only ever resemble the game's.
- **Mods write the `windowType` in a `.gui` file of their own**: the most freedom, but the mod has to know the game's interface format, and a second way of making the same window would sit beside the Lua layout.
- **Pop-ups as the game's own dialog** (`eu3dialog.gui`, shown by its callback dialog): it is exactly the game's, but the game drops a new one while another is open, and its words, shields and buttons are the game's to fill. The framework's pop-ups are written in its look instead, the message pop-up's, as windows of their own.

## Consequences

- The game widgets of a window are fixed when the extension loads, and an extension reload declares them again; what they show (texts, icons, numbers, whether each shows, a list's rows) changes at any time. The framework reads the definitions it writes after startup through the game's own reread of interface files, which replaces a definition of the same name.
- The framework owns these windows' lifetime: building them, building them again after the game rebuilds its interface, and destroying them when a session ends.
- The framework defines the game widgets a layout is made of, each a set of the game's elements in the game's look; a game widget the game's panels have and the framework lacks is added to the framework.
