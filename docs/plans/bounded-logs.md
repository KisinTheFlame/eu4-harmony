# Bounded framework logs

Status: decided and built (2026-11-12, the owner), L1 to L4; review fixes completed 2026-11-13. Final validation: 1,045 workspace tests (six ignored), both Clippy builds and release build. Ten player/developer offline checks passed before the review fixes, which change no game dependency points. No player game was started or changed; running-game frame cost was not measured.

## Goal and scope

Keep the framework's own logs bounded while making an unexplained end of the game easier to investigate. The owner approved automatic cleanup of framework-owned logs and small event snapshots, and raised the proposed combined space target from 64 MiB to 256 MiB. Game logs, saves, crash dumps and manually kept troubleshooting zip files stay outside cleanup. There is no new process watcher, settings interface or exception interception.

The game's crash writer is already observed, with a record of the exception and its calls ([crash handling](../research/crash-handling.md)). A process can end without that writer or fail to persist a record. This work keeps that mechanism; it does not claim to catch every exit.

## Behavior

- **L1: Shared bounded logs.** Core, proxy and extension log writers use one file policy, complete run/time/source identity, four 8 MiB segments and registered-file cleanup. Inactive sources older than 30 days or beyond the 256 MiB target are removed; active sources are protected, even if that temporarily exceeds the target. Unknown files are untouched.
- **L2: Useful bounded context.** Exact repeated ordinary errors have counts and periodic summaries. A 256-event snapshot is overwritten periodically and on important errors. Session/load changes and extension versions enter that context without adding per-frame or unit-action logs. Critical records bypass counting, report actual write results and have a separately opened emergency writer when the normal log is unavailable.
- **L3: Bounded troubleshooting files.** Stream one entry at a time under a total payload limit, take marked tails of large text logs, preserve whole dumps only when they fit, redact named credentials and report omissions. Never overwrite a player's earlier zip.
- **L4: Evidence and documentation.** Temporary-directory regressions cover rotation, cleanup ownership, concurrent writers, counts, bounded snapshots, write failure, crash-lock fallback, large-file tails, packing limits and redaction. The [current behavior](../logs.md) explains limits and failure cases.

## How it fits together

`logging/` is a small shared file-policy and record implementation with no game or shell dependency. The proxy uses it without linking the folders or extension implementations under the loader lock. The core's log wrapper adds a bounded startup buffer, safe-frame maintenance and crash-lock fallback. Extension logs enter the same implementation through their host. Troubleshooting collection uses the common zip writer's incremental output, leaving save and music paths unchanged.

Every part is done when its regression tests and the applicable workspace checks pass, the behavior is documented in English and Chinese front pages, and no test has changed player files. Performance limits are reasoned from bounded data and write frequency; tests of file sizes are not a benchmark of the running game.
