# Submission P99 probe smoke check

A temporary bounded stage recorder exported a 15-second paused window successfully, but the driver then asked for the nonexistent capability `session` instead of `session.get`. The run ended before its speed-5 windows. This was a driver error, not a game crash. No running-game percentile or optimization result is claimed from it.

- Source: `8d5358f1`, with the temporary `DEBUG-P99` changes retained as `target/scenarios/frame-p99-attribution/probe.patch`. The checkout contained the latest `origin/master` before testing.
- Core SHA-256: `94aaa8d17a4b4b6fac83f113836028cd4df66f0743ac672aa51b01eb75b56a3e`; features `dev-test-games,dev-performance`.
- Claim: `loving-wiles-p99`, created 2026-11-14 05:37:14 +08:00; process 90096. Started hidden with seed 123456, then `menu.start country=MNG`, pause, and ten seconds to settle.
- Complete raw window: 2,156 completed intervals; P50/P95/P99/max 6.944/7.330/7.817/14.429 ms. Both event and probe overflow counters were zero. The ordinary report had already been returned but was not written by the failing driver; these numbers come from the raw events.
- The runtime module base was not captured before release. Source addresses in this smoke trace therefore remain virtual addresses; later runs capture their module bases explicitly.
- Raw trace: `target/scenarios/frame-p99-attribution/traces/90096-1.txt`. Driver error and claim/start/release records: `target/scenarios/frame-p99-attribution/first-run/`.
- Released at 05:38:14. Exit 1, no crash. Kept logs: `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p99-20261114-053714/kept/`. No test game remained held. The corrected driver was used in the [attribution run](2026-11-14-frame-p99-attribution.md).
