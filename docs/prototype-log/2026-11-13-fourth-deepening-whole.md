# 2026-11-13 Fourth deepening whole scenario set (tested by me)

Status: completed on 2026-11-13. Seven scenarios passed in this run; nine were refused because of old save registrations and all nine passed the separate private-registration replay. Every scenario in `whole` therefore has a passing result across the two reports. This original report retains its nine failures and exit code 1.

The complete `whole` set was started with `cargo xtask scenario whole --owner fourth-deepening-4510f0 --out target/scenarios/fourth-deepening-whole-20261113` on `3ecdc4f2`, with master `e902f0a0` included. Its player, memory-account and test-switches cores were built and copied aside before any scenario played. Each scenario owned a separate hidden test game; at most four games ran at once. The player core's SHA-256 is `d1cb5fdd6b997190c9779f705d029ab96aa78ef9d1d5ccd978e98c6556187f53`.

## Results

| Scenario | Outcome | Minutes | Evidence |
| --- | --- | --- | --- |
| sweep | passed | 12.5 | 393 capabilities, 5,025 requests; answers held to their declared shapes or refusals |
| achievements | passed | 1.5 | Non-ironman tracking, in-game load and the setting disabled |
| extensions | passed | 1.7 | 271 dates counted without a break, save data restored 11 times, all 16 task checks passed; only the intentional loop stopped for good |
| windows | passed | 1.3 | All 28 native-course steps, with no extension or native error |
| long-1444 | passed | 74.7 | 1444.11.11 to 1600.1.1; seven autosaves loaded; private bytes stayed below 8,000 MB |
| checked | passed | 13.1 | Twenty years with the declared speed-up checks |
| perf | passed | 3.6 | Ran alone, with zero other games; 1,058.4 game days per minute |
| sessions, late, long-1600, long-1700, long-1760, long-1780, long-1800, long-1820, growth | refused in this run | — | All nine passed the private-registration replay described below |

The first long stretch loaded autosaves at 1464.11.11, 1484.11.11, 1504.11.11, 1524.11.11, 1544.11.11, 1564.11.11 and 1584.11.11. Its largest sampled private-byte figure was 4,665 MB. Other games ran alongside much of that stretch; its elapsed time is not an isolated performance measurement.

The performance scenario began after the other games ended. The main menu was reached in 27.2 seconds and its new Ming game in 4.1 seconds. At speed 5 it ran for 120.2 seconds, from 1444.11.11 to 1450.9.3, at 1,058.4 game days per minute. No unchanged-baseline comparison was made, so this is a successful performance run rather than evidence of a speed-up from the refactoring.

## Save registration mismatch

The inherited playset loads published Regional Combat from Workshop item 3837793510 through `mod/ugc_3837793510.mod`. The older test saves still record `mod/eu4-harmony-regional-combat.mod`, with the display name `Regional Combat (EU4 Harmony)`. The framework correctly rejected those loads: missing-mod checking compares descriptor paths, not display names. The original saves and the player's playset were left as they are.

This prevented the nine save-based scenarios from completing their reads. They remain failed outcomes of this report, not passing coverage and not evidence of a native lifetime regression. The [private-registration replay](2026-11-13-fourth-deepening-private-registrations.md) passed all nine using a private registration alias and the exact same core binaries. The two reports together cover all sixteen scenarios without changing the first report retroactively.

## Cleanup and limits

Every release reported no crash, with normal quit or game-stop exit codes. The final `game status` reported zero test games held and zero running. The temporary replay source was removed, and the ordinary debug tool was rebuilt and checked after that replay. Framework logs and checked core snapshots remain with the reports for provenance; private user folders, test mods, registrations and followers were released.

The [native window courses](2026-11-13-fourth-deepening-native-windows.md) and [tooltip checks](2026-11-13-fourth-deepening-tooltip-handoff.md) cover the changed paths directly. These runs do not exhaust real device loss or arbitrary partial effects inside native functions that fault under SEH. The separate `release` set's uninterrupted full-length game and heap-census leak scenario were not played; `whole` covered the long game in its seven defined stretches.
