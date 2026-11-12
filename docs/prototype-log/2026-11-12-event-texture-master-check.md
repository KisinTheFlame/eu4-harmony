# 2026-11-12 Checking the first event option's texture on master (tested by me)

The player reported that the first event option appeared to show other interface textures again after 0.1.3 stopped the picture freezing. This run checks the current master separately from the published build. It does not reproduce that reported corruption and does not establish its cause.

## Build and setup

- Baseline: master `482fbcd1`, then rebased to `b484480a`. The intervening change only affects what `cargo xtask install` installs; the core sources are identical between those commits. Built with `cargo build --release --features eu4-harmony-core/dev-test-games`.
- Hidden test game `event-regression`, its own user folder, core and pipe. No framework files were installed into the game folder, and no other game was controlled.
- The computer's copied playset: five Workshop entries (`253263609`, `2830362508`, `2976470733`, `1999055990`, `3253723536`) and the local Regional Combat mod. Chinese texts, 3840x2160, frame rate without a limit.
- Loaded `observer-1700.1.1`, switched the country to the Ottomans with `console.run` (`tag TUR`), and let time run at speed 5. Polled `question.list`, took three 50% screenshots for every batch of questions, answered each first available option with `question.answer`, and resumed time. These were events fired by the game's daily update, rather than events fired directly from the console.

## Result

The three-minute run listed and answered 17 questions in 16 captured batches, with 48 screenshots, from the first captured event at 1700.12.20 through the last at 1708.5.13. Every first screenshot was inspected, including a contact sheet of all 16 retained as local evidence outside the repository, and the additional screenshots were retained during the comparison. No first option showed the top bar, another panel, a missing button, or a frame stretched into its texture in the inspected images. This is a bounded negative result: the earlier broken run first corrupted the screen at its 22nd event ([the original run](2026-11-11-event-window-band.md)), so these 16 windows alone do not rule the defect out.

## Why this is not a test of the Workshop's 0.1.3

Master already includes [the event window's crossing](2026-11-12-event-option-text-left.md), merged as `482fbcd1`; the published tag `v0.1.3` is `7f694b85` and does not. Across the entire `core`, `proxy`, `cli` and `remote` source trees, the only difference between that tag and this master is the addition of the event-showing function `0x828c00` to `drawing/crossings.rs` and its associated comments and array length.

That change protects an event's options from being drawn while they are being activated. Its earlier evidence concerns a texture made by the drawing thread with left-aligned text, not another interface panel drawn into the texture. The input to that earlier investigation was already from 0.1.2, before the change for the frozen picture; its existence is therefore not proof that the change for the frozen picture introduced it.

The change for the frozen picture retains the renderer-table redirects and the presentation lock. It stops an already-held lock's renderer calls from marking the prepared frame changed, and explicitly marks a frame the main thread draws itself. The old render-target race and the incomplete option's activation are different gaps: see [the research](../research/drawing-beside-the-day.md), "A text drawn into its texture in a day", "An event's window, met half made", and "The war overview after the drawing".

## Attempt to shorten the run

A temporary test mod, passed through `game start --mod`, defined a daily chain of identical event options to allow pixel comparisons. Neither its namespaced event nor a numeric-id variant was found by the game's console after loading the old save. The namespaced variant logged an unknown namespace. No samples from this failed setup count as event coverage, and no conclusion about its cause was drawn. The same test-mod limitation was already recorded in the earlier event-option run. The temporary mod never went into the player's linked `mod` folder.

## Limits

The player's newly reported texture corruption was not captured. The run used this computer's playset and late save, rather than a captured failing event from the player's current session. A complete frame-by-frame screen recording was not made, and short flashes between screenshots are not ruled out. A question behind another window may not have been visible in its batch's screenshots. The published build is [checked separately](2026-11-12-event-texture-published-check.md); no product change is justified by this negative result alone.

The claim was released after the published-source comparison, with no crash. Its release and the removal of temporary material are recorded in that run's cleanup. Only the run notes were kept in the repository; contact sheets were retained as local evidence outside it; the installed framework and the player's files were not changed.
