# An error does not stop an extension; the time limit does

An error raised in a Lua handler ends that call and is logged with its traceback, and the extension is called again at the next event. The first version design stopped a native extension at its first panic, because after a panic or a native exception nothing in the process could be trusted; a Lua error leaves the state and the process sound, and one bad branch, such as a province the author did not expect, should not take a whole mod away for the rest of the game. This supersedes the first version design's line on errors, for extensions.

The same error at the same place is written in full only its first three times in a session, then counted, so a broken `on_frame` does not fill the log; the player is told once per session per extension that its log has errors.

What a script cannot recover from is time. A handler call that runs for more than 1 s is stopped with an error; after three stopped calls in a row the extension is stopped as a panicking one was: not called again until the game closes, its interface elements hidden, a notice saying why.

## Considered Options

- **Stop at the first error, as before**: safe against a state left half changed, but a single unexpected case would end the mod for the session.
- **No time limit**: a script stuck in a loop would hang the game, with no way to tell which mod did it.
- **A count hook on every call**: simpler than a watcher, but measured 2026-11-03, a count hook makes Lua 5.4 50 to 80% slower in tight loops whatever its interval; the limit is watched from another thread, which reaches into the state only once it has passed.

## Consequences

- An extension must expect that a handler may have stopped half way, and keep its state so that the next event can go on from there.
- The log, not the game, is where an author sees a broken mod: one notice per session, then counts.
