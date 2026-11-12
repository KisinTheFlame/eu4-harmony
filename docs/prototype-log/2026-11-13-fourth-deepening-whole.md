# 2026-11-13 Fourth deepening whole scenario set (tested by me)

Status: running. The completed report and cleanup will be recorded when the remaining scenarios end.

The complete `whole` set was started with `cargo xtask scenario whole --owner fourth-deepening-4510f0 --out target/scenarios/fourth-deepening-whole-20261113` on `3ecdc4f2`, with master `e902f0a0` included. Its player, memory-account and test-switches cores were built and copied aside before any scenario played. Each scenario owns a separate hidden test game; at most four games run at once. The player core's SHA-256 is `d1cb5fdd6b997190c9779f705d029ab96aa78ef9d1d5ccd978e98c6556187f53`.

## Save registration mismatch

The inherited playset loads published Regional Combat from Workshop item 3837793510 through `mod/ugc_3837793510.mod`. The older test saves still record `mod/eu4-harmony-regional-combat.mod`, with the display name `Regional Combat (EU4 Harmony)`. The framework correctly rejects those loads: missing-mod checking compares descriptor paths, not display names. The original saves and the player's playset were left as they are.

This prevented `sessions`, `late`, `long-1600`, `long-1700`, `long-1760`, `long-1780`, `long-1800`, `long-1820` and `growth` from completing their reads. They are failed outcomes of this report, not passing coverage and not evidence of a native lifetime regression. The [private-registration replay](2026-11-13-fourth-deepening-private-registrations.md) repeats those nine scenarios with a private registration alias and the exact same core binaries.

The separate [native window courses](2026-11-13-fourth-deepening-native-windows.md) and [tooltip checks](2026-11-13-fourth-deepening-tooltip-handoff.md) cover the changed paths directly. The set's final results, measurements and remaining limits follow after completion.
