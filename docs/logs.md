# Framework logs and troubleshooting files

Harmony records what it did without recording every frame, getter or unit action. The shared `logging/` module owns the file policy used by the core, proxy and extensions. It does not intercept the game's error writer. A large `error.log` is the game's file, even when a mod or an interaction with the framework caused its messages.

## Files and identity

The user folder's `eu4-harmony/logs/` holds:

- `core.log`, `core.prev.log`, `core.2.log` and `core.3.log`: the current core segment and three older segments. A start and a size rollover both advance these names.
- `proxy.log` and its older segments, opened only if the proxy needs to report a problem.
- `extension-<id>.log` and its older segments, for each extension that writes a line. Unsafe filename bytes in an id are escaped; long names keep a stable suffix. Extension names cannot collide with the core or escape the logs folder.
- `<source>.snapshot.txt`: a bounded view of recent events, pending repeated errors and log-write failures. `.snapshot.new` is a replacement in progress, not an additional history.
- `report.txt` and `report.prev.txt`: startup reports, still overwritten as before rather than treated as streaming logs.

Records include a full local date and time, the process id, thread id, source, the process's run id and a monotonic system time (`up_ms`). The run id includes the process creation time, so a reused PID is not mistaken for the same run. Proxy and core records share that identity. Source files have a marked header; `.harmony-logs/` holds their ownership receipts and separate leases.

A second writer of the same source in the same user folder uses a suffixed filename instead of rotating the first writer's file. This includes overlapping processes. Reading a live log does not require acquiring its lease. The usual test game's private user folder continues to use `core.log`. An exclusive file probe also avoids rotating open logs from an older writer without a lease. The proxy's policy requires this build of `version.dll`; a core update from the Workshop cannot replace the proxy already copied into the game folder.

## What is written

The core keeps its existing messages, including the ten-second drawing summaries. Session transitions, session-start date/player context, save-read completion and extension versions, loading, stopping and extension reloads also enter the small event snapshot. Startup identifies the actual core file's on-disk path, length and modification time as well as the framework version and supported baseline; on-disk metadata is not a hash of the loaded image. Extension log calls are events in that extension's snapshot. No stream of getter calls, frames or AI commands is added. Native message-window clicks and individual mouse/keyboard input are not recorded, so these logs cannot replay every player action.

Commands posted through the framework contribute aggregate begin/success/failure counts for at most 64 command kinds, with their last monotonic update and unmatched begins. There is no file record per command and no command arguments. A busy logger can miss an update; checkpoint status reports the missed-update count, and unmatched begins are diagnostic clues, not proof that a command remains on the stack.

Ordinary errors with exactly the same redacted text are written once and counted thereafter. A summary names the count and the first and last occurrence times, at most a minute later or at a session checkpoint. New errors remain separate. Both the set of error identities and the event ring are bounded at 256 entries. Lua handler errors continue to be counted by the Lua host; occurrences beyond the first few also reach the live journal's periodic counts, rather than becoming visible only at an orderly session end. Opening an extension log backs off for a minute after failure, records the failure in the core log and counts unconfirmed records for recovery/checkpoint reporting.

A snapshot keeps at most 256 event records of at most 2 KiB each, plus bounded pending-error summaries. A safe-frame gate maintains it about every 30 seconds, including while paused. Ordinary errors accelerate the next snapshot, with at most one such rewrite per second; their first log line is still written and flushed immediately. Critical records request a snapshot immediately. There is no new timer thread or external process watcher. No safe frame while loading or hung means no periodic refresh in that interval.

Each individual message is limited to 64 KiB and explicitly marked if truncated. This bounds an extension's single log call as well as its cumulative output. Normal crash descriptions and the existing 32-frame stack fit within this limit; this is not a promise to retain an arbitrary amount of text in one critical record.

## File bounds and cleanup

Defaults are 8 MiB per segment, four segments per source, a 256 MiB combined target and a maximum age of 30 days for archived segments and inactive source files. Age is an upper bound, not a guaranteed history: size and total-space limits can remove older evidence sooner.

Sources clean up on opening and about once a minute while writing or receiving safe-frame maintenance. Cleanup uses the ownership registry and verifies the header of each file. It ignores unregistered or unmarked files, reparse points and files whose source lease is held. A writer rotates and expires its own archived segments, but never another writer's current file. Under total-space pressure it can also discard its own oldest archives. An inactive source's current segment is eligible once no writer owns it. Cleanup is not a background task that runs when the game is closed.

The combined target includes registered logs and snapshots. If the protected current segments and snapshots alone exceed it, the target can temporarily be exceeded; cleanup does not delete them to force the figure down. Safety takes precedence over deleting a live file; a subsequent cleanup retries after those writers have ended. There is no guarantee that 30 days of logs fit into 256 MiB.

The game's logs, crash folders, dumps, player saves, manually saved troubleshooting zip files and third-party files are never deleted or rotated by this policy. Old extension logs without a new ownership marker are left in place. A source's own legacy current/previous files are adopted as bounded tails only when that source opens them; cleanup never guesses ownership from a `.log` extension. Development trace output and the test-claim tool's existing seven-day `kept` cleanup remain separate policies.

## Troubleshooting collection

The About page still saves one zip file on the desktop and never overwrites an earlier zip of the same name. Collection writes entries incrementally, holding one bounded source and its compressed form, not all collected logs in memory.

The payload limit is 128 MiB. Current core and emergency evidence and snapshots come first, then crash files, other framework logs, settings and game logs. Framework text files contribute at most 8 MiB each; game text logs contribute at most their last 2 MiB. A leading partial line is dropped. A binary dump is included whole only when it fits the remaining space and its 32 MiB per-file limit; a cut dump would be unusable, so a larger one is omitted and named in the summary. Collection has a file-count bound as well.

`about.txt` lists original byte counts, tail offsets, collected sizes, truncation and omission reasons. It also records the run id and the collection limits. Redaction removes named password, authentication, token, ticket, secret and known launcher account-identifier fields from command-line, query and text records, including the startup command line in the summary. It is best effort for named fields, not a guarantee to discover a secret embedded in arbitrary prose or a binary dump. No files are uploaded automatically.

## Crash-record limits

The existing observer of the game's crash-file writer and its exception-context stack walk are unchanged. Panic records still capture their original backtrace. These records bypass ordinary error counting. They wait at most one second for the normal log lock, then try a separately opened, bounded emergency journal without waiting on its lock. A return value of success means a write and flush succeeded; merely buffering an early-startup record is not reported as success. Snapshots include write-failure counters, and session checkpoints include lock-timeout and emergency-failure counts.

Neither the lock deadline nor `flush` gives a deadline for filesystem I/O or guarantees durability against power loss. Heap corruption can prevent allocation; both journals may be unavailable; and fail-fast termination, external termination or power loss may never reach the game's crash handling. An empty crash record does not prove which of these occurred. More useful context cannot reconstruct a missing exception address or stack from the earlier run.

The implementation is checked using temporary directories and synthetic files. It does not require touching the player's existing logs to exercise retention.
