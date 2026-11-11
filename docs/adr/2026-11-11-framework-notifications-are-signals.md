# What extensions hear of the game is a signal; an event is the scripts'

What the framework tells extensions and remote clients of the game, a new day, a war begun, an army ordered, is a signal, and remote control's numbered list of them is read by `signals`. "Event" is left to what it means to the mod authors the framework is for: one of the game's or a mod's events, defined in `events/` and fired by its id, which `script.fire_event` fires and which, for the player, waits as a question. Handlers keep their names (`on_day`), so no extension changes.

Until now the framework called its own notices events. That was harmless while no capability touched the scripts' events, but the capability catalogue now fires them, and a Paradox mod author reading `events`, `on_<event>` and `fire_event` side by side would take them for one thing.

## Considered Options

- **Keep "event" for the framework's, and call the scripts' "script events"**: no rename, but the word a mod author uses every day would mean something else in the framework's catalogue, its wiki and its guides, for as long as the framework lasts.
- **"Notification" or "message"**: the first is what Windows and the game's own message settings call theirs, the second what the game calls its log lines and the diplomatic messages; "signal" is free in the game and means in other frameworks what it means here, something emitted that handlers hear.

## Consequences

- Remote control's `events` capability becomes `signals` while the catalogue is 0.x; remote clients, the scenarios and the guides change with it. The signal kinds keep their names (`day`, `war_started`).
- The code, the wiki, the guides and the documents use the new words from the change on. ADRs and records written before it keep "event" in the old sense, as GLOSSARY.md says.
