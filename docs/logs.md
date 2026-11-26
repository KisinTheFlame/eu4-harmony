# Framework logs and troubleshooting files

Harmony records what it did without recording every frame, getter or unit action. The shared `logging/` module owns the file policy used by the core, proxy and extensions. It does not intercept the game's error writer. A large `error.log` is the game's file, even when a mod or an interaction with the framework caused its messages.

## Files and identity

The user folder's `eu4-harmony/logs/` holds:

- `core.log`, `core.prev.log`, `core.2.log` and `core.3.log`: the current core segment and three older segments. A start and a size rollover both advance these names.
- `proxy.log` and its older segments, opened only if the proxy needs to report a problem.
- `extension-<id>.log` and its older segments, for each extension that writes a line. Unsafe filename bytes in an id are escaped; long names keep a stable suffix. Extension names cannot collide with the core or escape the logs folder.
- `<source>.snapshot.txt`: a bounded view of recent events, pending repeated errors and log-write failures. `<source>.snapshot-<run>-<serial>.new` is an exclusively created replacement in progress, not an additional history; a preexisting staging file is never truncated.
- `report.txt` and `report.prev.txt`: startup reports, still overwritten as before rather than treated as streaming logs.

Records include a full local date and time, the process id, thread id, source, the process's run id and a monotonic system time (`up_ms`). The run id includes the process creation time, so a reused PID is not mistaken for the same run. Proxy and core records share that identity. Source files have a marked header; `.harmony-logs/` holds their ownership receipts and separate leases.

A second writer of the same source in the same user folder uses a suffixed filename instead of rotating the first writer's file. This includes overlapping processes. Reading a live log does not require acquiring its lease. The usual test game's private user folder continues to use `core.log`. An exclusive file probe also avoids rotating open logs from an older writer without a lease. The proxy's policy requires this build of `version.dll`; a core update from the Workshop cannot replace the proxy already copied into the game folder.

## What is written

The core keeps its existing messages, including the ten-second drawing summaries. Session transitions, session-start date/player context, save-read completion and extension versions, loading, stopping and extension reloads also enter the small event snapshot. Startup identifies the actual core file's on-disk path, length and modification time as well as the framework version and supported baseline; on-disk metadata is not a hash of the loaded image. Extension log calls are events in that extension's snapshot. No stream of getter calls, frames or AI commands is added. Native message-window clicks and individual mouse/keyboard input are not recorded, so these logs cannot replay every player action.

Commands posted through the framework contribute aggregate begin/success/failure counts for at most 64 command kinds, with their last monotonic update and unmatched begins. There is no file record per command and no command arguments. A busy logger can miss an update; checkpoint status reports the missed-update count, and unmatched begins are diagnostic clues, not proof that a command remains on the stack.

Ordinary errors with exactly the same redacted text are written once and counted thereafter. A summary names the count and the first and last occurrence times, at most a minute later or at a session checkpoint. New errors remain separate. Both the set of error identities and the event ring are bounded at 256 entries. Lua handler errors continue to be counted by the Lua host; occurrences beyond the first few also reach the live journal's periodic counts, rather than becoming visible only at an orderly session end. Opening an extension log backs off for a minute after failure, records the failure in the core log and counts unconfirmed records for recovery/checkpoint reporting.

A snapshot keeps at most 256 event records of at most 2 KiB each, plus bounded pending-error summaries. A safe-frame gate maintains it about every 30 seconds, including while paused. Ordinary errors accelerate the next snapshot, with at most one such rewrite per second; their first log line is still written and flushed immediately. Critical records request a snapshot immediately. There is no new timer thread or external process watcher. No safe frame while loading or hung means no periodic refresh in that interval.

Each individual message is limited to 64 KiB and explicitly marked if truncated. This bounds an extension's single log call as well as its cumulative output. If a write fails after appending only part of a record, rotation uses the actual file length before another append; if that length cannot be read, the next write rolls to a fresh segment. Normal crash descriptions and the existing 32-frame stack fit within this limit; this is not a promise to retain an arbitrary amount of text in one critical record.

## File bounds and cleanup

Defaults are 8 MiB per segment, four segments per source, a 256 MiB combined target and a maximum age of 30 days for archived segments and inactive source files. Age is an upper bound, not a guaranteed history: size and total-space limits can remove older evidence sooner.

Sources request cleanup on opening and about once a minute while writing or receiving safe-frame maintenance. Journals in the same directory share one scan deadline and the remaining space pressure; adding sources does not multiply complete directory scans. Cleanup uses the ownership registry and verifies the header of each file. It preserves unregistered or unmarked files and reparse points. A malformed or unreadable receipt is skipped and counted in `cleanup_skipped_receipts`, rather than stopping other sources' cleanup. A writer rotates and expires its own archived segments, but never another writer's current file. Under total-space pressure it can also discard its own oldest archives. An inactive source's current segment is eligible once no writer owns it. Abandoned unique snapshot staging files are eligible once an exclusive probe confirms they are closed; an active snapshot keeps its file handle through the rename. Cleanup is not a background task that runs when the game is closed.

An inactive source with no remaining source files also loses its ownership receipt and source lease. Source opening and registration removal hold the same directory lease, so a new writer cannot acquire a replacement lock file during removal. These acquisitions are nonblocking; contention is an open or maintenance failure rather than a wait under the proxy's loader path. Unknown source files keep their registration in place. The directory lease itself remains.

The combined target includes registered logs and snapshots. If the protected current segments and snapshots alone exceed it, the target can temporarily be exceeded; cleanup does not delete them to force the figure down. Safety takes precedence over deleting a live file; a subsequent cleanup retries after those writers have ended. There is no guarantee that 30 days of logs fit into 256 MiB.

The game's logs, crash folders, dumps, player saves, manually saved troubleshooting zip files and third-party files are never deleted or rotated by this policy. Old extension logs without a new ownership marker are left in place. A source's own legacy current/previous files are adopted as bounded tails only when that source opens them; cleanup never guesses ownership from a `.log` extension. Development trace output and the test-claim tool's existing seven-day `kept` cleanup remain separate policies.

## Troubleshooting collection

The About page saves one zip file on the desktop and never overwrites an earlier zip of the same name. Collection includes the framework's logs, the game's logs, all existing crash folders including their complete binary dumps, matching Windows crash evidence, the settings and the playset. Saves are excluded. Snapshot staging files still being written are excluded. There is no per-file size, total payload or file-count limit, and text logs are not reduced to tails.

Entries are compressed directly into the output file, with ZIP64 records for sizes, offsets and entry counts beyond ordinary ZIP's fields. Binary dumps are read and compressed in chunks. Text files are read one at a time for credential redaction, then compressed in chunks; the whole archive is never buffered in memory. Each source contributes up to its length when opened, so a live log's continued growth cannot keep collection running indefinitely. A file that cannot be opened is named in the summary; a read, compression or output failure aborts collection and removes the incomplete zip instead of presenting it as complete.

`about.txt` lists original and collected byte counts and files that could not be opened. It also records the run id and the complete-collection policy. Redaction removes named password, authentication, token, ticket, secret and known launcher account-identifier fields from command-line, query and text records, including the startup command line in the summary. `Authorization` and `Proxy-Authorization` redact the complete field value for every scheme, including multiword and quoted parameters. It is best effort for named fields, not a guarantee to discover a secret embedded in arbitrary prose or a binary dump. No files are uploaded automatically.

`windows-crashes.txt` includes Windows Application Error event 1000 records for this installation of `eu4.exe`. Collection reads at most 128 candidate events from the last 30 days, newest first, when the player saves the troubleshooting files. It records selected fields, including the event time in UTC, the faulting module and version, the exception code and module offset, and the process ID and creation time. The latter two form the same `run` ID as in the framework logs. Test games, other user folders and runs without Harmony can use the same executable, so compare that ID before attributing an event to the player's run. The report says if the query could not be completed or found no matching records.

## Crash-record limits

The observer of the game's crash-file writer records the exception and walks its saved registers. Panic records still capture their original backtrace. These records bypass ordinary error counting. They wait at most one second for the normal log lock, then try a separately opened, bounded emergency journal without waiting on its lock. A return value of success means a write and flush succeeded; merely buffering an early-startup record is not reported as success. Snapshots include write-failure counters, and session checkpoints include lock-timeout and emergency-failure counts.

Neither the lock deadline nor `flush` gives a deadline for filesystem I/O or guarantees durability against power loss. Heap corruption can prevent allocation, both journals may be unavailable, and fail-fast termination, external termination or power loss may never reach the game's crash handling. Windows may record a fault that the game could not record. A Windows event provides a fault location and code without a call stack. When neither source recorded the fault, the last log line only shows the most recent activity that was recorded.

The implementation is checked using temporary directories and synthetic files. It does not require touching the player's existing logs to exercise retention.

## Windows minidumps

For crashes caught by the game's own crash handler, Harmony logs the exception and a 32-frame stack in `core.log` and relies on the game's crash folder (`crashes\`) rather than shipping an executable beside the core (ADR the-framework-ships-on-the-workshop).

For faults that bypass the game, startup registers the shipped Windows Error Reporting callback DLL (`eu4_harmony_crash_dumps.dll`) for the current process and lists its path under the current user's `Software\Microsoft\Windows\Windows Error Reporting\RuntimeExceptionHelperModules`. When Windows calls it for a fatal EU4 exception, it writes a minidump in `%LOCALAPPDATA%\CrashDumps\eu4-harmony` and leaves the normal Windows reporting decision to Windows. The startup log records registration failures and flags a process error mode that suppresses Windows Error Reporting. The development uninstall and test-game release remove the corresponding DLL path. Removing a manual installation can leave an unused path in this per-user list. Registration alone cannot establish whether Windows delivered a particular fault. The callback retains up to five completed Harmony captures in `%LOCALAPPDATA%\CrashDumps\eu4-harmony`. [Windows runtime exception callbacks](https://learn.microsoft.com/windows/win32/api/werapi/nf-werapi-werregisterruntimeexceptionmodule)

In Harmony Settings, open About and select **Enable Windows crash dumps for EU4 (admin)** to add a Windows Error Reporting rule. Approve the Windows administrator prompt once. The rule applies to `eu4.exe` for all accounts on the computer. The game keeps its original permissions and the shipped WER callback works independently of this setting. Cancelling leaves the button available for another attempt.

The button sets only `DumpFolder` under `HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\eu4.exe`, to `%LOCALAPPDATA%\CrashDumps\eu4-harmony-wer` for the account running the game. Other values inherit the system's global WER settings (by default, minidumps and a retention count of ten). If an EU4 rule already exists, Harmony displays its configured status. This machine setting persists after uninstall. An administrator can remove the EU4 subkey created by the button to undo it. Windows decides which faults reach WER. [Collecting user-mode dumps](https://learn.microsoft.com/windows/win32/wer/collecting-user-mode-dumps)

Collection also checks the configured WER destination, Harmony's Windows dump folders and the Windows default `%LOCALAPPDATA%\CrashDumps`. A Windows minidump with exception registers, a process ID and a creation time matching a retained Harmony log is included as `windows-dumps/<run>/minidump.dmp`. The About page packs binary minidumps verbatim. They can contain process memory. A forced kill or power loss can leave no exception or minidump.

Use `cargo xtask crash-dump <dump> <log> '.ecxr' 'kn 30'` to inspect the exception stack.
