# 2026-11-13 Fourth deepening with private save registrations (tested by me)

Status: completed on 2026-11-13 at 05:44; all nine scenarios passed and their claims were released.

This repeats the nine scenarios blocked by the old registration path in the [whole run](2026-11-13-fourth-deepening-whole.md): `sessions`, `late`, `long-1600`, `long-1700`, `long-1760`, `long-1780`, `long-1800`, `long-1820` and `growth`. The owner is `fourth-private-4510f0`, report folder `target/scenarios/fourth-deepening-private-retry-20261113`, with three scenario workers beside the first run's remaining game. The normal claim manager still limits the computer to four running test games.

## Private setup

A one-off debug build of xtask reused the exact player and memory-account cores copied aside by the original run; it did not rebuild or modify core code. Before starting each replay game, it made that claim's mod registry private, copied the current published descriptor to the old filename `eu4-harmony-regional-combat.mod`, and replaced the current registration in that claim's own playset with the old path. Both descriptors' contents are identical and point at the same Workshop folder, whose manifest still has id `regional-combat` and interface version `0.3`. Only one of those registrations is enabled. No save bytes, player settings, player registrations or Workshop files were changed, and the missing-mod check remains in place.

The first `long-1600` replay's startup log confirms that the current published Regional Combat content was loaded. Its private registration files were compared byte for byte. The copied core was also compared with the original run's player core: both have SHA-256 `d1cb5fdd6b997190c9779f705d029ab96aa78ef9d1d5ccd978e98c6556187f53`.

The temporary replay changes were removed from source immediately after building the debug tool and before starting the replay. They add no shipped option, special case for a mod, or permanent environment setting. After the replay, `cargo build -p xtask --locked` rebuilt the ordinary debug tool; its executable was checked to contain no temporary replay switch. Claims removed the private aliases when they were released.

## Results

The replay used the existing scenario implementations and their checks rather than an independently rewritten course. All nine passed:

| Scenario | Minutes | Coverage |
| --- | --- | --- |
| sessions | 19.3 | All ten rounds of loads, new games, observe mode, ironman and returns |
| late | 5.1 | Five late saves, time advanced and their autosaves loaded |
| long-1600 | 43.6 | 1600 to 1700, autosaves loaded at 1620, 1640, 1660 and 1680 |
| long-1700 | 29.5 | 1700 to 1760.7, three autosaves loaded |
| long-1760 | 12.8 | 1760.7 to 1780.7, an autosave loaded |
| long-1780 | 12.3 | 1780.7 to 1800.7, an autosave loaded |
| long-1800 | 11.9 | 1800.7 to 1820.7, an autosave loaded |
| long-1820 | 1.0 | 1820.7 to 1820.11.11 |
| growth | 5.2 | Ten rounds of sessions on the memory-account build |

The long-game scenarios kept private bytes below their 8,000 MB limit. The growth run measured a private-byte slope of +8.61 MB per round over rounds two to ten, of which +0.33 MB was live game-heap data, +11.74 MB unused committed heap space and -3.60 MB outside the heaps. Its passing result establishes the course completed; it does not establish zero growth or a comparison against an unchanged baseline. Other test games were running alongside, so the time figures are not isolated performance measurements.

Every claim release reported no crash and exit code `0x1`, kept its logs and removed the private user folder, registry, core and follower. A final status check found none of these nine claims held; only the original run's `long-1444` remained running. The original report still records the registration failures; this replay supplies the missing coverage without changing those outcomes retroactively.
