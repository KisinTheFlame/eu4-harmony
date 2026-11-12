# 2026-11-12 Temporary core highlights kept for the prepared drawing (tested by me)

After the [baseline reproduction](2026-11-12-core-highlights-baseline.md) and [single-variable clearing experiment](2026-11-12-core-highlights-clearing.md), this run tried the local border-reader correction in `core/src/drawing/marks.rs`.

## Build and setup

Branch `claude/core-highlight`, master base `daf21a47` (0.1.4), independent tooltip ownership guard `4b0c408e`, uncommitted prepared-highlight reader. Built the core with `dev-test-games,dev-test-switches` before starting. The game retained its original clear call at `0x815928`; no shared game-object array pointer was swapped. The drawing thread kept its original repeated-draw behavior, unlimited. The later source changes reused reader buffers through `Memory::read_into`, extracted capture for regression tests and added dependency checks; those did not change which inputs are retained.

Hidden, silent test game owner `core-highlight`, claim `core-highlight-20261112-185437`, own core, user folder and pipe, same playset as the baseline. Save `observer-1700.1.1`, 3840x2160. No installation into the game folder. All game input and screenshots used its named test-game pipe.

## What was tried

1. **Original trigger.** Province 151, center click, Ottoman core flag at `(118,763)` at scale 50. Eight red-pixel counts with the baseline's criterion: `[837, 702, 60, 6, 6, 6, 28, 638]`. The screenshot visibly showed the outlines; the border pulse explains the low samples. Moving away produced twelve counts of 6.
2. **Different target without accumulating marks.** Province 145 (Morea), whose cores were TUR and GRE. Hover TUR at the same flag position, then GRE at `(140,763)` at scale 50, without leaving the province panel. The Ottoman outline reached 1118 red samples, with up to 320 in an eastern Anatolia region (x=600..850, y=100..300 in the scale-25 image, stride 2). Hovering GRE reduced that region to its background count 9 in all twelve samples; the Ottoman red outline was gone. Greece's unowned cores instead had the game's purple temporary outline, up to 881 samples (R>120, B>110, G<0.7*min(R,B), x=310..860, y=80..500, stride 2). A red-only assertion initially misclassified that result; inspection showed the correct Greek tooltip and purple outline. After moving away, all twelve purple counts were 0 and red counts remained at background 12.
3. **In-game load.** Loaded the same save again. The game's pause menu remained open, so a camera request was refused as locked; closing it with Escape allowed the original trigger. Counts `[216, 821, 768, 119, 6, 6, 6, 25]` showed the outline restored after the load. Moving away gave eight counts of 6.
4. **Country in view.** Entered observe mode with the game console's `observe`, then used `session.view` for FRA and TUR. The original core hover still worked: `[22, 7, 7, 8, 219, 844, 737, 95]`. Moving away gave eight counts of 6. This tests switching the country in view, not every player-country console transition.
5. **Camera and a day running beside drawing.** A middle-button map drag `(2300,1200)` to `(2700,1200)` changed camera x from 3243 to 3195 at the same y=1349 and height=310. Then `time.advance --days 180 --speed 5` reached 1700.6.30 from 1700.1.1 in observe mode, with no missed signals: 180 day signals and 39 natural army-order signals. No forced events were used.

## Evidence and limits

The kept core log contains no crash record and no failure to capture or read temporary highlights; release reported no crash. Paused gameplay drawing samples were about 178–195 FPS, versus the baseline's last steady 194.8 FPS sample; the speed-5 interval reported 154.4 FPS and 3.5 ms average drawing. These are contextual log samples, not a controlled performance benchmark. The drag's camera displacement was checked; visual frame pacing during the drag was not measured. Observe mode produces no player questions, so this does not verify opening and answering a natural event window.

The visible map samples establish the right country outlines locally; this run did not enumerate every core across the world or test every caller of the temporary highlight producer, every map mode, or every possible day-side access. The lifetime argument and the precise reader fields are in [research](../research/map-drawing-reads.md#the-frames-temporary-highlights-read-and-tested-2026-11-12). The independent tooltip lifetime guard is separate from the cause established here.

## Cleanup

Released the claim after the last checks. The game ended by release (exit code 1), with no crash; logs were kept and the rest removed. `game status` reported 0 held and 0 running test games. No player process was stopped and no game-folder file was installed or replaced during these runs.
