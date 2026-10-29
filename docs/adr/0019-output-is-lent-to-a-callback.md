# A command's output crosses the ABI lent to a callback, for the call

A host function that does something it cannot do again to be asked a second time, such as running a game console command (`GameApi::console_run`), hands back text of unknown length through a callback the caller passes (`abi::TextSink`) with a `user` pointer of its own. The core calls it once, on the game thread, before the host function returns, with the text as UTF-8; the text is lent for that call only, and the caller copies what it keeps. The SDK's `Game::console` does that and returns a `String`, so an extension never sees the callback.

So the ABI passes text two ways: copied into the caller's buffer, by the two-call pattern, where asking again changes nothing (every list and snapshot); or lent to the caller's callback for the call, where it would. Either way nothing the core owns outlives a call into the core, and nothing the core made is freed by the caller. This supersedes the reason in ADR 0018 that the ABI passes nothing it does not copy into the caller's buffer; ADR 0018's decision stands.

## Considered Options

- **The two-call pattern**: its second call, with room for the output, would run the command again: `cash 100` would add 200 ducats.
- **The core keeps the last command's output for a second host function to read by two calls**: state held between calls, which a further command replaces, and two fields in `GameApi` for one capability.
- **A buffer the caller passes up front, the output cut at its end**: an output longer than the caller guessed, such as `help`, would be lost past it, with no way to ask for the rest.
- **A pointer to text the core owns, freed by a host function the caller calls**: what ADR 0018 turned down, a caller that forgets leaks, and one that frees twice corrupts the core's heap.

## Consequences

- A callback must not unwind across the ABI (no panic or exception may leave it), and may call host functions: the core holds nothing locked while it calls it.
- Another host function that cannot be called twice for the same answer, such as a later command that does something, takes a `TextSink` the same way, appended to `GameApi` as any capability is (ADR 0014).
