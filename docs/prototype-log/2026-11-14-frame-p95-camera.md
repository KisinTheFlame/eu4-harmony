# 2026-11-14 Paused camera comparison under changing machine load

This run used the same performance core built for the [submission baseline](2026-11-14-frame-p95-baseline.md), source `8d5358f1`, in another owned hidden test game. It started a new Ming game and stayed paused. Settings and playset were inherited as in that baseline. A concurrent Ghidra import overlapped the run and was closed by 05:15:06 local time, so the changing load prevents treating this as a controlled camera-cost estimate. The [repeat](2026-11-14-frame-p95-repeat.md) followed that cleanup.

A one-off Python client used the documented newline-delimited JSON pipe capabilities. It claimed `loving-wiles-p95-camera`, started it through `cargo xtask game start`, used `menu.start MNG` and `time.pause`, and waited ten seconds. It read the initial camera, then warmed a closed horizontal route twice: 8 map pixels every 100 ms, traveling 320 pixels and returning in eight seconds. The initial query, immediately after campaign creation, returned x=2986, y=1450 and height=430. The game's startup camera then settled: every measurement's final camera reported height=310, with y=1450. Fixed windows ended at x=2986 and moving windows at x=2994, the final step of the route. No zoom request was sent. Every measurement began with a request for the same initial x/y, with three seconds settling between windows.

The order was idle, fixed, moving, moving, fixed, 24 seconds each. Fixed and moving windows both sent 240 `camera.look_at` requests with `glide=false` on the same 100 ms schedule; fixed requests repeated the initial coordinates. The idle window sent no requests during collection. This controls request count but does not prove a same-position request has zero native work. The moving route was a sequence of immediate camera positions, not hardware input or continuous dragging.

| Window | Submissions | Interval P95 / P99 / max, ms | Present-call P95, ms | Interface age P95, ms | System CPU busy, % |
| --- | --- | --- | --- | --- | --- |
| Idle | 3,404 | 7.586 / 11.377 / 22.676 | 5.460 | 20.375 | 90.90 |
| Fixed 1 | 3,456 | 7.617 / 8.114 / 14.003 | 5.881 | 20.060 | 55.64 |
| Moving 1 | 3,442 | 7.571 / 8.226 / 30.927 | 6.021 | 20.171 | 52.30 |
| Moving 2 | 3,452 | 7.579 / 8.055 / 14.444 | 6.055 | 20.102 | 52.64 |
| Fixed 2 | 3,399 | 7.754 / 8.348 / 21.176 | 6.519 | 20.367 | 27.42 |

System busy time is the `GetSystemTimes` delta of kernel plus user minus idle, divided by kernel plus user, bracketed around the window; it includes the game and other processes and is not GPU load. All windows reached their in-process deadlines and retained all records, with no failures, no gaps over 50 ms, no completed days and no main-thread submissions. The reports preserve each request's elapsed time and the camera after each window.

These results did not reproduce the large moving-camera slowdown in the older late-game investigation. They do not disprove it: this is another map region, zoom, campaign and movement path, and the machine load changed materially. No code change or performance improvement is claimed.

Reports are `target/scenarios/frame-p95-camera/{idle,fixed-1,moving-1,moving-2,fixed-2}.json`, `camera.json` and `release.txt`. Process 72504 ended through release with code 1 and no crash at 05:15:19; the claim was released. Logs were retained under `%LOCALAPPDATA%/eu4-harmony-dev/claims/loving-wiles-p95-camera-20261114-051152/kept/`. No screenshots or stack sampling ran during the windows. Physical display, GPU time, hardware-input response and the cost of individual preparation passes remain unmeasured.
