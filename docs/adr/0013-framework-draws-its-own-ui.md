# The framework draws its own UI

The framework's own interface, that is the notice window and the settings window, is drawn by the framework with the game's D3D9 device, on top of the game's picture. Its widgets (windows, text, buttons, drop-down lists) are all the framework's own, and text uses system fonts; it borrows nothing from the game's interface system and does not live in the game's windows. This supersedes ADR 0008: notices hosted in vanilla windows break as their hosts are created and destroyed and as other mods change them; and the game's interface also lacks widgets the framework needs, for example EU4 has no native drop-down list.

The settings window can be opened only from the framework button in the corner of the main menu, and while it is open it takes the mouse and keyboard to itself. The notice window does not take the input. Neither pauses the game. Extensions' interface is not affected: elements added to vanilla panels are still spliced into vanilla windows as in ADR 0006. The framework UI is not open to extensions for now.

## Considered Options

- **Splice into always-present vanilla windows** (ADR 0008): see above.
- **Native Windows windows and dialogs**: they come with drop-down lists, but in exclusive full screen they get covered, or they make the game minimize.
- **An existing immediate-mode UI library**: it would save work on input and layout, but its default look is that of a debugging tool, and it adds a third-party dependency; and the framework needs few widgets.

## Consequences

- The framework has to manage input focus itself: while the framework UI is active, it handles the mouse and keyboard first.
- The corner button appears only in the main menu, and telling "currently in the main menu" relies on the main menu's per-frame function, which is already hooked. So framework settings can be changed only in the main menu, and take effect at the next start.
- Once the framework UI works, the code and textures of the spliced notice window are deleted. When the framework fails to start, notices use a Windows message box instead (ADR 0012).
- The widget interface does not hard-code "for the framework only", so it can later be opened to extensions as a capability.
