# Performance and concurrency evidence

Historical experiments on Windows EU4 1.37.5, baseline SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`. These are bounded observations, not universal safety claims. [Performance](../performance.md) retains the measurement tables and endpoints. Older date-based throughput and frame counts are not completed-day and successful-submission measurements. Routine build and regression transcripts have been omitted.

## Frame pacing and lock ownership

On 2026-11-14, early Ming traces attributed much of submission-interval P99 to presentation-lock waiting and invalidation. A native stream helper returned in microseconds while the framework retained ownership during subsequent AI work. The first single-scope A/B/B/A experiment had no repeatable gain. Follow-up traces showed actual early release followed by another helper reacquiring the game lock and restoring the long framework hold. [The native helper chain](../game/interface/stream-formatting.md) is distinct from that framework lifetime.

Four checked normal-return boundaries ultimately removed the chain. Timing used base `8d5358f1`, release builds with `dev-test-games,dev-performance`, fresh hidden Ming games, seed 123456, inherited settings/playset, 3840×2160 borderless, 144 Hz vsync, multisampling 8 and an unobstructed Beijing view at height 310. Each run collected ten seconds paused and 120 seconds at speed 5 after settling, without temporary stage/stack recording.

The final A/B/B/A comparison repeated approximately 70% lower submission P99. One prototype window had higher system load and slower daily progress; the second pair had nearly unchanged progress. Interface age and maxima did not consistently improve. All eight windows completed without lost events or failed submissions; all four games ended without a crash. Five-second process samples saw no other game but cannot exclude brief overlap or unrelated work. Disturbed, interrupted and overflowing diagnostics were excluded from attribution. Cold construction, exceptions, custom virtual implementations, late-game scenes and universal daily-rate cost remain unestablished. The behavior was adopted later; these remain experimental measurements.

## Startup and loading

Music-cache state, file cache, playset, tracing and background work affected startup substantially. The 2026-11-14 large-mod comparison used cached music, all DLC, hidden starts and no other EU4. Alternating optimized/native paths shortened startup with Guixu, the player's playset and 2000 AD; detailed times remain in Performance. Guixu checks matched 76,964 localisation answers, 2,826,796 trigger-profile answers, 1,067,167 directory lookups and 268,395 missing-path lookups. A twenty-year checked scenario passed. Console `verify_loc` and `reloadloc` worked. First-ever language selection, command-line localisation import/export/purge/verify and category-5 `-debug` output were not exercised interactively.

Mesh/database overlap is a negative safety result. Eleven initial starts completed, images looked alike and corresponding saves differed only in ordinary metadata. Later busy-machine starts produced two map-logic access violations in approximately twelve starts, then a third two starts later. One dump placed the mesh thread in a file read while the LoadingThread used an invalid map-grid value. The missing shared dependency was not identified; the prototype was removed. Successful starts and identical saves did not establish safety, and neither PhysFS nor machine load was established as the cause. [Loading](../game/program/loading.md) keeps the native read/write constraints.

## Saving

Parallel province/country writing matched native output byte-for-byte on binary ironman saves from 1496, 1548 and 1724 and a plain-text England autosave. End-to-end comparisons found identical AI data and state differences limited to ordinary metadata. Optimized saves subsequently loaded and played.

Cloud tests produced readable three-entry archives and loaded through the cloud tab; Steam recorded synchronization. Another machine was not tested. Historical experiments left a named test save and cloud backup whose later removal was not verified. Background replacement sometimes returned access denied while the complete uncompressed save stayed readable. An external reader was suspected, not proved; later retries were not tested against a deliberately held file. Random-new-world saves, multiplayer, one-thread hardware and vanilla loading stored entries were not established.

On 2026-11-14, twelve further parallel-save comparisons matched and two autosaves loaded on their written dates. The separate late-game scenario refused a mismatched mod name and did not run. Save-stop markers were captured without the background-compression completion line; they do not prove compaction finished. [Save writing](../game/program/saves.md) and [transfer buffers](../game/program/save-transfer-copy.md) describe the native paths.

## Randomness and AI order

Fresh-process loads with `-seed=1` often reproduced a late-game half-year after excluding campaign/time metadata; repeated loads in one process did not have the same seed behavior. Without a seed, two runs differed in 3.4 million of 3.6 million save lines. Even seeded runs sometimes chose different Royal Navy admirals. Complete determinism was not established.

Reversing a country's AI-agent order changed approximately 14,950 diff lines after one month, versus zero or six for unchanged order. Per-agent random generators therefore do not make decisions independent of execution order. The first per-country stream implementation crashed by returning full 32-bit values where native callers expect nonnegative 31-bit draws. Restricting output allowed 120 days, with 307,678 streamed draws in 11,500 tasks and none observed outside a task. Loaded-machine alternating timings established no speed gain. See [AI scheduling](../game/ai/work-scheduling.md) and [native random generators](../game/day/random.md).

## Parallel army searches (removed)

On 2026-11-28, `army-searches` (speculative background `0x1022b0` searches on an 8-thread pool) was re-measured on base `643bf7a0` (after exact `army-checks` and `support-checks` landed) in isolated hidden muted test games (synthetic 1920×1080 English settings, full DLC, single `dev` release build toggled via `game-paths = ["army-searches"]`, 60 s speed-5 windows, all complete with zero crashes):

- **Late game (`observer-1800.7.1`, 6 alternating runs ON/OFF/OFF/ON/ON/OFF)**: ON averaged 1,197.0 completed days/min (1,216 / 1,188 / 1,187; ordinary day mean 31.69 ms, P50 28.29 ms; 3.047 cores) vs OFF 1,154.0 days/min (1,159 / 1,125 / 1,178; ordinary day mean 33.57 ms, P50 30.65 ms; 2.952 cores) — **+3.7% mean (+2.5% median)** throughput for +0.095 cores and unchanged submission P95/P99.
- **Mid-late game (`observer-1780.7.1`, 4 alternating runs ON/OFF/OFF/ON)**: ON averaged 1,182.5 days/min (1,189 / 1,176) vs OFF 1,108.5 days/min (1,066 / 1,151) — **+6.7% mean** throughput for +0.100 cores.
- **Early game (1444.11.11 Ming, 4 alternating runs ON/OFF/OFF/ON)**: ON averaged 827.5 days/min (875 / 780) vs OFF 843.5 days/min (859 / 828) — **-1.9%**, within campaign-variance noise.
- **Why the net gain stayed in low single digits**: 60 s `--threads --waiting-in 0x179e90` profiles on `observer-1800.7.1` showed `[day]: the AI` took only 27.9% (ON) vs 30.6% (OFF) of main-thread time. Although `army-searches` cut `0x1022b0` on the AI critical path by 74% (4.79 to 1.23 ms/day), other AI tail work (`0x168070` recruitment, `0x96c520` offensive route queries) became the remaining bottleneck, shrinking total AI main-thread wait by only 1.35 ms/day (12.66 to 11.31 ms/day). Per ~40,000 prepared searches, 43–46% were canceled before starting, `prepare` spent ~1.8–1.9 s serially on King threads, and taken searches ran without the country access cache.

Because `army-searches` was the sole speed-up introducing vanilla-divergent AI decisions (4–7% differing answers in checked mode) for a small late-game-only gain, it was removed while keeping `0x1022b0` wrapped by `army-checks`'s exact per-search scope. Limits: single 8-core/16-thread Windows machine, unmodded full-DLC saves, 60 s windows without player-controlled late-game wars.

## Memory and crash probes

Holding worker threads made Direct3D-dependent write watches hang. Access-group watchdogs recovered some truncated runs; later page-protection probes could themselves stop the graphics driver. Incomplete watches do not establish absence of races.

Delayed freeing of retained modifier sets survived sixteen reset rounds, then corrupted the heap in round seventeen. Pointer scans could not distinguish live ownership from stale bytes; the lifetime explanation remained unresolved. A deliberate main-thread access violation verified the framework crash record against the game's exception and stack and its collection after restart. Stack overflow, pure virtual calls, `abort`, other-thread faults and fail-fast were not covered.
