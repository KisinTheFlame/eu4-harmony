# The framework succeeds or fails as a whole: dependency points declared in one place, startup all or nothing

Each dependency point is declared once: its location (an RVA, or an export name of eu4.exe), the bytes that should be there, and what the framework does there (nothing, replace the function's entry, jump to the framework from the middle of a function, rewrite bytes). Checking, hooking and undoing are all driven by this one declaration; the existing interface, map, save, event and PhysFS hooks move onto these declarations as well. Before, each module registered its expected bytes and then hooked on its own, so one location was written down twice, and a missed registration raised no error.

**Startup**: first check every dependency point; only when all match are all the hooks installed in one go. If any one does not match, or any hook fails to go in, the changes already made are undone and the framework does not run this time: not a single hook is installed, a separate thread writes the log and the startup report, and a Windows message box gives the reason. The Unicode module (ADR 0010) is a foundation of the framework; its failure is the framework's failure.

**While running**: expected, recoverable cases are handled as normal flow. For example, a missing character shows a placeholder; a full atlas evicts the glyph unused for longest; a broken legacy escape is skipped, that one alone, and recorded in the report; a font chosen in the settings that is no longer there is replaced by the default font, with one notice. Unexpected cases, that is framework code panicking, an invalid memory access, or engine data not matching the framework's assumptions, are logged and shown in a message box, and then the game process is ended. Extension errors are still handled as in the first version design: that extension is stopped, and the game goes on.

## Considered Options

- **Group by feature, each group succeeding or failing on its own** (for example, extensions keep running when the Unicode module fails): once the Unicode module fails, all Chinese in the game is garbage, and extensions running as usual is of little use; groups would also make the startup logic and the various fallbacks much more complex.
- **Still hook PhysFS when the exe is unsupported, to write the report and show in-game notices** (the earlier approach): this adds a "partly hooked" state, while a message box is enough to give the reason.
- **On an error while running, switch to the vanilla code and let the game go on**: the strings in the game are already UTF-8, so the vanilla code can only draw garbage, and the faulty state may also keep spreading.

## Consequences

- When startup fails, the only way to tell the player is a Windows message box. Windows draws the message box itself, so it can show Chinese.
- Ending the game loses the progress made since the last save.
- A new dependency point needs declaring in one place only; checking, undoing and the check results in the startup report all follow from it automatically.
