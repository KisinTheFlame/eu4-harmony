# 2026-11-12 Core hover outlines on the 0.1.4 baseline (tested by me)

Follow-up to [the missing core outline](2026-11-11-core-highlight-lost.md), before testing its cause or changing its mark inputs.

- **Build:** branch `claude/core-highlight`, rebased onto master `daf21a47` (0.1.4), with only the independent tooltip ownership guard (`4b0c408e`) beyond master. The uncommitted marks prototype was not wired in. Built with `cargo build --release --features eu4-harmony-core/dev-test-games,eu4-harmony-core/dev-test-switches` before starting the game; its own startup log confirmed core 0.1.4 and the drawing thread on, unlimited.
- **Setup:** hidden, silent test game, owner `core-highlight`, claim `core-highlight-20261112-184641`, its own core, user folder and pipe. The existing playset included Regional Combat and the player's translation mod. No game-folder installation. Loaded `observer-1700.1.1`; despite the save's name, `session.get` identified the player as TUR, outside observe mode.
- **Trigger:** `camera.look_at` province 151; click `(1920,1080)` in the 3840x2160 frame; move to `(118,763)` at scale 50 to hover the Ottoman core flag. Its tooltip appeared.
- **Signal:** eight scale-25 screenshots, about 0.2 s apart after a 1.5 s hover. Count pixels every two pixels in x=32%..90%, y=45%..95%, with R>170, G<90, B<90. Counts: `[6, 6, 6, 6, 6, 6, 6, 6]`: the outline was missing. The burst accounts for the outline's pulse.
- **Not verified here:** cause, correction, other hovered countries, or a running day. This run establishes the symptom after syncing master.
- **Cleanup:** released immediately after capture. The game did not crash; release kept its logs and removed the rest. `game status` then listed no test game of this owner.
