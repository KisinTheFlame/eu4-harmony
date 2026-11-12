# 2026-11-13 Non-ironman achievement tracking (tested by me)

The non-ironman achievement setting passed the overall eligibility check but did not admit any achievements through their own initial script conditions. This run follows [Achievement rules, and the save checksum of an in-game load](2026-11-04-achievement-rules-load-checksum.md), which checked `achievement_ok` and save checksums, not the tracked list or a completion. The game's separate checks are in [research/achievements.md](../research/achievements.md#each-achievements-script-conditions).

## Setup

Built from `564cf60a` with `eu4-harmony-core/dev-test-games`, first unchanged and then with the achievement-evaluation scope repair. Claimed `achievement-rules-8cc249`, started only through the claim, hidden and silent, from the installed 1.37.5 game. The claim copied the player's playset (the Chinese translation and supplementary mods, Graphical Map Improvements, Emoji Universalis, the modifier sorting mod and Regional Combat) and framework settings. Both achievement settings were enabled. New games as Castile, paused at 1444.11.11, with text autosaves. No installation into the game folder and no edits to the player's saves or settings.

## Reproduction and repair

- **Before the repair:** `menu.start CAS`, then the console's `autosave`, produced a non-ironman save with `achievement_ok=yes` and no `achievement` list. The automatic check failed with `eligible non-ironman game has no tracked achievements`.
- **Platform readiness was not the blocker:** a read of this test process's achievement manager found flags `00 01 01 00`, `+0xa0 = 0`, 373 mapped definitions and zero pending or completed achievements. Definition objects still contained the ironman script condition. All 373 baseline definitions require `ironman = yes` in `possible`.
- **An ineffective comparison, not evidence against the cause:** a temporary test mod removed the 373 ironman conditions from its copy of `common/achievements.txt`, but only the framework recognized that test mod. The game's saved `mods_enabled_names` omitted it, and its live condition objects still contained the original ironman checks. Changing the descriptor path separators and trying a relative descriptor in the test user folder did not change that. Those comparisons were discarded; no test-mod registration changes are part of this repair.
- **After the core repair, with the original playset:** the same new-game and save check passed, with `achievement_ok=yes`, no `is_ironman=yes`, and **178 tracked achievement IDs**. The scope applies only to the candidate builder and the daily achievement check, feeding a true ironman value to the game's existing script comparison when the setting permits it. The actual world flag and ordinary script contexts remain unchanged.

## A real daily completion without a platform submission

In the repaired test game, achievement ID 7 was pending and no achievements were completed. Its platform object's unlock virtual method resolved to `0x164b7f0`. For this test process alone, replaced that virtual slot temporarily with a counting return stub, so that reaching the platform boundary could be observed without submitting an achievement to Steam or setting the platform object's unlocked byte.

Set Castile's `has_won_war` country flag through `script.set_flag`, advanced one day through `time.advance`, and saved. On 1444.11.12:

- exactly **one** unlock call was intercepted;
- ID **7** was in `completed_achievements` and no longer in the pending IDs;
- the pending count went from **178 to 177**;
- the next autosave persisted completed ID 7.

The script's `finally` restored the original virtual slot, read it back to verify restoration, and freed its allocated counter and stub. No platform unlock function was called by this completion. The game was then stopped and released.

## Automated regression

`cargo xtask scenario achievements --owner achievements-regression-8cc249 --out target/achievement-scenario` checks a new non-ironman Castile game's nonempty saved list, an in-game load preserving that exact list and eligibility, and a new non-ironman game with the option disabled having neither eligibility nor a tracked list. It changes only its claim's settings and never advances the date. The run **passed** in 1.4 minutes, with no other game running: **178** tracked IDs before the load, the same **178** after the load, and **0** with the option disabled (`achievement_ok=no`). The runner released its claim and reported no crash.

Four core tests cover the achievement-only value, the setting disabled, nested scopes, another thread and cleanup on unwind. Three scenario helper tests cover complete top-level save parsing, empty lists and preserving unrelated settings. The `whole` and `release` sets now include this regression scenario; the full sets were not rerun for this focused repair.

## Validation

- `cargo fmt --all -- --check`: passed.
- `cargo build --release`: passed; rebuilt once more after the offline checks to leave the ordinary player configuration in the build folder.
- `cargo test`: **1,052 passed**, none failed; the installed-game checks remain ignored here and were run separately below.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo clippy --workspace --all-targets --features eu4-harmony-core/dev -- -D warnings`: passed.
- `cargo xtask offline-check`: **5 passed** on the ordinary build and **5 passed** with the developer tools, including every dependency point matching and every observer forwarding its arguments.
- After including the scenario in `whole`, reran the xtask tests and clippy, then its set-membership test: passed. `git diff --check`: passed.

## Limits and cleanup

- Actual Steam account unlock submission and achievement-window condition tooltips were not tested. The daily completion was observed up to the intercepted platform boundary, with its saved result.
- Ordinary script isolation was checked through the scope's unit tests and the unchanged real ironman state; an ordinary-event test mod was not used after the comparison showed its game resources had not loaded.
- Existing saves without an `achievement` list are not repaired by guessing initial conditions from current state. The player's original autosaves were read only; they remain unchanged.
- The inherited Regional Combat extension reported an existing tooltip argument error on this source baseline. It did not prevent the new game, save or completion checks; that extension is outside this change.
- Releasing `achievement-rules-8cc249` reported five processes ended by the test commands, no crash, and kept only logs under the claim's `kept` folder. `cargo xtask game status` then reported zero held or running test games. The temporary comparison copies and one-off scripts are removed after validation; the reproducible regression remains in `xtask`.
