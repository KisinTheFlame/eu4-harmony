# 2026-11-15 Fixed window backgrounds (tested by me)

This run follows [the gray-background diagnosis](2026-11-14-button-area-gray.md). Branch `claude/epic-tharp-06bbcc` was brought to master `5d26a83844`; the uncommitted repair gives fixed backgrounds a separate corner-tiled sprite using the game's static button effect. The existing disabled input state is retained. The second start also includes dependency checks for native window and list wheel handling. Both starts used `cargo build --release --features eu4-harmony-core/dev-test-games` and the hidden, silent test-game claim `great-turing-fixed-bg`, in `great-turing-fixed-bg-20261115-032314`. No framework files were installed into the player's game folder.

## Setup

Both starts made a new Austria game, paused on November 11, 1444, dismissed the introduction and selected the 10,000-man `K.u.K Armee`, id `652835029046`, in province 134. Only the temporary `fixed-background-probe` mod was enabled in this claim's private playset. The first start had one army-panel button slot; the second had nine, filling eight rows and starting a second column. The inherited player mod combination was not used in this repair run.

The probe opened fixed windows at 320 by 300 and 420 by 500 GUI units, plus a movable 320 by 300 window. The small window contained an enabled button, a disabled button and a click counter. The large window contained a 360 by 170 list of thirty 30-unit rows. The background texture and 12-unit border were unchanged. The game selected 3840 by 2160 borderless output despite the requested smaller private display settings; GUI scale stayed 1. All coordinates and pixel comparisons below use the actual 3840 by 2160 frame.

## Presentation and input

The automated check passed 26 assertions on each start. Screenshots also visibly confirmed the gold corners and edges at both window sizes and at button-area sizes 70 by 392 and 116 by 392. Background samples were:

| Surface | Screen rectangle, x/y/width/height | Mean RGB |
| --- | --- | --- |
| Small fixed window | 665 / 325 / 260 / 65 | 43.583 / 53.961 / 61.005 |
| Large fixed window | 1010 / 430 / 350 / 140 | 43.613 / 54.002 / 61.044 |
| One-column button area | 540 / 410 / 35 / 250 | 43.483 / 53.844 / 60.844 |
| Two-column button area, empty second column | 590 / 410 / 35 / 250 | 43.490 / 53.854 / 60.866 |

On each sampled background, hover and held-left-button captures differed from idle by **zero pixels**. Left and right empty-background clicks kept the same selected army and its empty movement route. Five upward and five downward wheel events over each empty background kept camera height, x and y unchanged at `310 / 3061 / 1480`. These checks covered both fixed windows and each button-area width.

Four downward wheel events over the list changed the visible entries from Row 1 onward to Row 5 onward; camera height and position stayed unchanged. The text-region comparison changed 363 of 45,000 pixels, and the screenshot independently confirmed the new row numbers. Dragging within the small fixed window left all 91,104 sampled interior pixels unchanged. Dragging the movable window from `(710, 720)` to `(810, 770)` moved it by `(100, 50)`; 89,959 of 91,104 pixels at its old position changed, and the screenshot confirmed its new location.

The real enabled button retained a visible pressed state: 3,027 of 4,060 sampled pixels changed while held. Releasing it incremented the counter to `Clicks: 1` and produced exactly one `probe click small.go` log entry on each start. Clicking the disabled control produced no action. The disabled control remained gray while the surrounding fixed background retained its blue and gold.

As a positive control after the blocked wheel checks, three downward events directly over the map changed height from 310 to 487 on the first start and from 310 to 481 on the second. Thus the blocked cases did not result from a missing wheel event.

## Limits and verification

The first extra drag check started inside the two-column background at `(610, 500)` but released outside its bounds at `(710, 550)`. The army panel closed, so an assertion requiring its old rectangle to remain identical failed. This was not counted as a passing fixed-background drag check. Background input shielding is bounded by the window's rectangle; this change does not add pointer capture for a release over the map.

The corrected drag from `(610, 500)` to `(610, 600)`, wholly inside the button area, left all 43,456 sampled pixels unchanged. One hundred wheel events reached each list limit, followed by five more in the same direction; neither end let zoom through (camera stayed `488 / 3061 / 1480`). The ninth army-area button produced `probe click probe9`. Together with the two 26-assertion runs, these checks yielded 55 passing assertions.

`cargo build --release`, `cargo test` (1,174 passed, six ignored), both required clippy configurations, `cargo fmt --all -- --check` and `cargo xtask offline-check` passed on the final source. The offline check covers expected bytes and observer arguments, including the new native wheel dependency points.

Both test processes (47072 and 102248) ended through stop/release with exit code `0x1`; release reported no crash. `game status` no longer listed this owner. Release removed the private user folder, core and registrations; the remaining copied `test-mods` folder was checked and removed separately. The temporary mod source, scripts, intermediate JSON reports and raw screenshots in this worktree's `target` were removed. The final composite screenshot and 55-assertion JSON were kept as `target/fixed-window-backgrounds-verified.png` and `target/fixed-window-backgrounds-evidence.json`; the earlier diagnostic comparison remains `target/button-area-comparison.png`. Framework and game logs remain under the claim's `kept` for the normal seven-day retention. No REA analysis session or cleanup was pending. Other owners' claims were left alone.

The run did not repeat the owner's full Workshop playset, change vanilla panel presentation, test another renderer or GUI scale, or test extension reloading and session transitions. The repair keeps the same native construction, subscriptions and teardown as before.
