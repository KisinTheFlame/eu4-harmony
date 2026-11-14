# Verify the early-release path and remaining submission P99

Status: completed 2026-11-14. Four diagnostic games proved actual early release followed by another acquisition retaining the old long lifetime; no stable performance gain was established. The normal-view pair has external-game-load limitations. Temporary runtime source was removed and the clean core rebuilt; evidence and cleanup are retained in the [research note](../research/frame-drawing-p99-release-path.md). This follows the completed [first scope experiment](frame-p99-lock-scope-validation.md).

## Question

The first A/B/B/A experiment did not establish a repeatable P99 improvement. Determine whether the proposed early-release path actually runs and shortens presentation-lock ownership, and which stages delay the remaining long intervals. Do not infer runtime activation from code or a loaded DLL alone.

## Experiment

Use one diagnostic build for both cases. A disposable process-start environment switch enables only the original bounded ownership change; it is not a supported setting. Both cases observe entries/exits of the same native getter and use the same raw performance and stage recorder. State bits distinguish preexisting ownership, daily work, ordinary crossing lifetime and scoped order-only ownership. Mark actual scoped acquisitions, early releases and promotion to crossing protection. Existing acquisition/release markers establish paired ownership durations.

Run hidden, owned games sequentially with seed 123456 and frozen settings. First compare baseline and prototype in the original view. Then compare prototype and baseline with the introduction window closed and the camera fixed on visible Ming territory. Verify screenshots and province visibility outside measurement windows. Each game has a paused control and a 120-second speed-5 window. Do not compile or analyze binaries during sampling.

Raw replay must agree with formal interval counts and P99. Reject missing events, probe overflow, failed submissions or mixed context. Keep percentile-stage decomposition separate from overlapping ownership durations. Entry/return instrumentation has a cost; this is a matched diagnostic comparison, not a player-build benchmark.

## Completion

Record each run separately, explain whether early release happened and where the remaining long intervals spent time, and retain contradictory or disturbed windows. If the evidence points to another cause, record it without silently widening the synchronization change. Remove the disposable source, debug/release test artifacts and executable copies; rebuild a clean core, scan artifacts, and release all owned games. Preserve raw evidence, patches and analysis for review. No install, commit, push or release is authorized by this experiment.
