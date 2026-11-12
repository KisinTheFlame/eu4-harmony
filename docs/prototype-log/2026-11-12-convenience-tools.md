# 2026-11-12 Convenience Tools and area governing costs (tested by me)

The new `province.area` read and a separate Lua mod's three finite batches: abandon states, make states without completing cores or exceeding capacity, and raze provinces. Design: [area governing costs](../plans/area-governing-cost.md); binary evidence: [the research](../research/area-governing-cost.md).

## Setup

- Mainline baseline `daf21a475d446b7d21de6e8d2ccf949d6f84003a` (0.1.4), plus the uncommitted capability changes in this isolated worktree. Not the released core.
- Release build with `eu4-harmony-core/dev-test-games`; claim `convenience-tools`, started hidden and silent with its own pipe and `--mod D:/Workspace/eu4-convenience-tools/mod`.
- New 1444.11.11 game as Castile. Inherited player playset, including the Chinese localisation and Regional Combat; the new extension was copied into the claim. No player's save or installed framework changed.
- Console `tag GOL`, `own` and `control` set up two non-core provinces for the raze test; later `tag CAS`, ownership and native trade-company commands set up the mixed-area test. These were changes only to the disposable test game.

## Area and capacity

Castile's nine owned areas totalled 231.500 used capacity, maximum 400.000. The capital area, Toledo, was 50 development but cost 0.500. Other area's costs agreed with their development and native status.

A targeted native-command comparison in Galicia (four owned provinces, 28 development):

| State | Area cost | Live country used | Projected additional cost |
| --- | ---: | ---: | ---: |
| Initial full-core state | 28.000 | 231.500 | 0 |
| Immediately after abandoning | 7.000 | 210.500 | 7.000 |
| Immediately after making a state, without coring | 14.000 | 217.500 | 0 |

All three reads were on 1444.11.11. The live getter reflects the command without advancing a day or explicitly requesting a country-cache update; the timing of the game's own cached-field writes was not measured. The 7.000 increment matches the projection and the surviving territorial cores, rather than the confirmation window's full-core projection.

## The actual buttons

The entry button opened a single native extension window with three Chinese-labelled buttons. No preview, confirmation or stop control. Input was delivered through the claim's remote control, and screenshots verified the window and result.

- **Abandon**: `batch make_territory: 8 succeeded, 1 skipped, 0 failed`. The native reason refused the capital state. Galicia became territory, Toledo stayed state, total capacity became 58.250. Date remained 1444.11.11 and `time.get.paused=true`.
- **Make states**: `batch make_state: 8 succeeded, 1 skipped, 0 failed`; reason `CT_ALREADY_STATE: 1`. The eight areas became half-core states, total capacity became 116.000, below 400.000. Date and pause state unchanged.
- **Raze**: as the Great Horde, Moscow (295) was 6/6/5 and province 297 was 2/2/1 before the button. Afterward they were 4/4/3 and 1/1/1. `batch raze: 2 succeeded, 15 skipped, 0 failed`; all 15 refused provinces had the native non-core requirement as their reason. Date and pause state unchanged.
- **Trade-company exclusion**: as Castile, Cape (1177) was added through `trade_company.add`; another owned member (1179) stayed outside the company. `province.area(1179)` listed both members of `cape_of_good_hope_area` with `trade_company=true`. The button reported `batch make_state: 0 succeeded, 10 skipped, 0 failed`, specifically `CT_ALREADY_STATE: 9` and `CT_TRADE_COMPANY: 1`. Both members stayed territories. This is direct skip-reason evidence, not only unchanged statehood.

Screenshots: [window](images/2026-11-12-convenience-window.png), [abandonment](images/2026-11-12-convenience-abandon.png), [raze](images/2026-11-12-convenience-raze.png), [trade-company exclusion](images/2026-11-12-convenience-trade-company.png).

## Corrections during the run

An initial test start refused a core without the test pipes after a concurrent offline build had replaced the shared release output. Offline checks and the developer build/start were then run sequentially. An initial extension tried to assign text to a text button, which Harmony does not permit; this was removed and the corrected files reloaded before the batch tests. The original error is retained in the claim log; no further handler errors appeared after the correction.

One attempted console `run` for an optional building-reduction setup was refused without output and did not change the world; that extra case was not pursued. An initial input selected the native Celestial Empire panel rather than the extension's entry; it was closed and the correct entry used before measuring the raze operation.

## Checks and limits

The mod's tests cover grouping, company exclusion, development/cost/key ordering, skipping a too-large area, exact capacity fill, fresh capacity, failures, country changes, duplicate clicks, pause behavior, extension/session reset and four-language result texts. The framework's native arithmetic test includes fractional truncation, negative flat costs, overflow and negative area sums. Player and developer offline dependency checks passed. Complete Rust tests passed (1,008 passed, 6 ignored). After final dependency/arithmetic hardening, the affected arithmetic and point-collection tests, three address-index tests, both warning-free workspace clippy builds, release build, and player/developer offline checks (five each) passed again. The final mod run passed all 12 tests; all four localisation files contain the same 20 keys. Formatting and diff checks passed.

Not tested in the real game: an actual too-large state being skipped at the capacity boundary (covered by batch tests), strongly discounted province costs crossing the 1% floor, negative scripted area costs, a total-conversion map, effects that change unrelated governing modifiers, Japanese/Korean/English rendering, and a very large country's batch duration. Four-language text selection was tested in the fake game. Individual initial list/scan stretches logged about 10–11 ms, below the one-second task time limit; this was not a performance comparison.

The claim is released at the end of the run; only the claim tool's seven-day kept logs and the evidence files linked here remain. No installation, commit, push, pull request or Workshop upload was performed.
