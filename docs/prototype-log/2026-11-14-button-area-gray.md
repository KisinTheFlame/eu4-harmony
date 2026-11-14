# 2026-11-14 The army button area's disabled background (tested by me)

This run investigated the owner's report that the army button area's background was gray. Branch `claude/epic-tharp-06bbcc`, based on `8d5358f1`, was already current with `origin/master`. Both builds used `cargo build --release --features eu4-harmony-core/dev-test-games`; both starts were hidden under claim `great-turing-gray`, with the inherited player settings and playset, including the Workshop Regional Combat and Convenience Tools extensions. Each started a new Austria game through `menu.start`, paused, dismissed the country introduction, and selected the same 10,000-man army through remote input. The screenshots were 3840 by 2160.

## Reproduction

The one-column button area beside the army panel was gray, including its edge. A screenshot check sampled the empty interior at screen rectangle `(810, 700, 50, 150)`, below the mod's button. It required the mean blue channel to exceed the mean red channel by 20 percent, distinguishing the intended blue panel from a fully desaturated one. Two separate captures on unchanged source returned the same means:

```text
background RGB mean: 52.23, 52.23, 52.23
FAIL: the button area has lost its blue background.
```

The source texture `gfx/interface/country_religion_view_allreligion_bg_large.dds` in the installed game was converted separately: it is blue with a gold edge.

## Controlled comparison

Only the final call in `ui::keep_still` changed for the second build: `SET_ENABLED.call(m, [background, 0])` became `SET_ENABLED.call(m, [background, 1])`. After a stop and a fresh hidden start, the same army id (`652835029046`) and the same screen rectangle returned:

```text
background RGB mean: 43.58, 53.95, 61.02
PASS: the button area background is blue.
```

The mod combination, texture declaration and drawing mode were unchanged. The installed game's `gfx/FX/buttonstate.shader` explains the exact symptom: its `Disable` effect uses `PixelShaderGreyed`, which computes luminance with weights `(0.212671, 0.715160, 0.072169)` and writes that value to all three color channels. The framework's background sprite declares no effect of its own. Disabling the background prevents its press behavior, but also selects this desaturated presentation.

This corrects the claim that a disabled background does not gray in [the November 10 run](2026-11-10-unmoving-windows-and-quick-right-clicks.md). That run's comparison measured the difference between held and released backgrounds, not their absolute color against an enabled background.

## Scope and cleanup

The button area's frame calls `ui::keep_still`; non-movable extension windows call the same function through their native lifecycle. Their backgrounds therefore share the suspect state and sprite, but this run measured only the army button area. It did not visually test another extension window, an alternative shader, a corrected input implementation, or a different mod combination. Simply enabling the background is diagnostic, not a retained repair: it restores the original press behavior.

The temporary source change was reverted, `git diff --exit-code -- core/src/ui.rs` passed, and the original source was rebuilt successfully. The claim was released; both processes ended through stop/release with exit code `0x1`, and release reported no crash. Framework and game logs remain in the claim's retained `kept` folder. Diagnostic screenshots and the pixel-check script were removed after preserving the cropped before/after evidence under `target/button-area-comparison.png`. No framework files were installed into the player's game folder.
