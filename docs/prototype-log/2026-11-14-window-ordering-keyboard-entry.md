# 2026-11-14 Tracing the initial Escape

This run follows the [staged native-drawing course](2026-11-14-window-ordering-native-keyboard-staged.md). It adds a temporary before/after trace at the existing keyboard event-depth seam. Branch `claude/zealous-carson-b8374d`, base `8d5358f1` with uncommitted ordering changes; release features `dev-test-games,dev-test-switches`; `[test-switches] game-paths = ["drawing-thread"]`. The fixture now also includes one army-panel button fragment. This does not mean that the host was exercised.

Claim `ordering-keytrace-1de2b155`, created 16:28:15, hidden process 94236, main thread 81612. Fresh France game paused on 1444.11.11. The original introduction event was answered, and `question.list` returned empty before keyboard input. Directory: `C:\Users\kisin\AppData\Local\eu4-harmony-dev\claims\ordering-keytrace-1de2b155-20261114-162815`.

## Observed input

At 16:29:44.469, the initial Escape reached the keyboard dispatcher with key 27 and handled word 0, and returned with handled word 65535. The sampled manager state before and after was enabled byte `+0x18 = 1`, capture byte `+0x230 = 0`, capture value `+0x248 = 1`, and retained pointer `+0x140 = 0`. No extension window closed. Thus remote delivery reached native keyboard dispatch; the key was not simply lost before it. No individual GUI recipient was traced, and this evidence does not identify why that recipient handled the event without closing a fixture window.

At 16:31:04.444, the next Escape likewise changed the handled word from 0 to 65535, and the extension reported `last closed; first reopened` at 16:31:04.466. The following Escape at 16:33:00.685 closed the reopened first window; the callback was logged at 16:33:00.707. Return at 16:33:24.063 also returned handled, and the final `popup.list` showed `Ordering keyboard passed`. These later actions reached the intended recipients, each once. The extra initial Escape remains an acceptance gap.

The temporary trace also logged numerous zero-key events, causing normal bounded-log rotation. Across the retained `core*.log` segments, there are 355 stable paired refreshes, 147 matching traversals, two verified placements, and no failed/deferred refresh or ordering panic. These are retained-segment counts, not a guarantee that every early record survived rotation. All three Escape pairs and the Return pair above were retained. No `ui: bound` record was present: the army panel was not selected, so this run does not validate its binding or picker.

## Cleanup and remaining work

Released at 16:33:58, exit code 1, no crash. Logs remain under `kept\eu4-harmony-logs`. The trace, independent-traversal probe, temporary drawing hook and checkout `dev.toml` were subsequently removed. The one-off disassembly files and diagnostic screenshots were also removed after their findings were recorded.

The specific initial-key recipient, actual pointer activation and action preservation, two events in one batch, army-picker coverage and restoration, and movement of a live text scrollbar remain unverified. No coordinate clicks, player input, or installation occurred.
