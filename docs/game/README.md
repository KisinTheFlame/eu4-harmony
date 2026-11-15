# The game

The known logic of EU4 1.37.5 for Windows (`9ad3efe1…`), organized by system. System pages lead to topic pages: normal flows, callers and callees, state read and written, object ownership and RVAs. A reference to an address does not mean its complete function is understood; partial readings, inferences and unverified behavior are explicit.

- [The day and its time](day.md): Dates, speed, daily and monthly work, admitted frames and the state shared between them.
- [Countries](countries.md): Country identity and knowledge, rulers, government, modifiers and shared national values.
- [Provinces and the map](provinces.md): Province state, areas, construction, religion, trade and what the map reveals.
- [Armies and fleets](military.md): Unit state, orders, paths, recruitment, leaders and the complete known battle-loss paths.
- [Diplomacy](diplomacy.md): Relations, envoys, questions, offers, peace and subject-reference lifetime.
- [The AI](ai.md): Country tasks, agent order, support checks and the limits of execution independence.
- [Scripts and commands](scripts.md): Script scope and evaluation, triggers, parameterized text, command construction and execution.
- [The interface, text and input](interface.md): Window lifetime and scene traversal, accepted input, text, fonts, the camera and rendering state.
- [The program](program.md): Program layers, startup, files, saves, process boundaries and persistent object ownership.

[addresses.toml](addresses.toml) indexes only these pages, including nested topics. `cargo xtask what <rva>` presents their explanation before code references. Investigation and measurement evidence is linked from the relevant topic and kept in [research](../research).

## Updating a topic

Write the game's normal logic in one topic, in the order it runs. Keep field meaning, callers, side effects and the limits of the reading together. Put RVAs beside their explanation; offsets belong with their object. Link shared operations instead of explaining them again. Distinguish vanilla behavior from Harmony's use or replacement of it. Experimental proposals and successful samples do not establish concurrency safety.

Update this account when evidence changes a conclusion, then run `cargo xtask addresses`. Preserve existing address names and partial leads when consolidating topics. The system entry pages are navigation, not another copy of the topic's facts.
