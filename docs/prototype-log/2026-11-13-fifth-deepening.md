# 2026-11-13 Fifth deepening scenarios and ironman setup-save handoff (tested by me)

Status: completed on 2026-11-13; all three concurrent scenarios (`extensions`, `windows`, `achievements`) and the three-process ironman setup-save handoff check passed, and every claim was released.

This verifies the seven parts of the [fifth deepening](../plans/fifth-deepening.md) in hidden test games on EU4 1.37.5 while an unrelated game (`eu4.exe` 25112) ran untouched outside the claims.

## Concurrent scenario run

`cargo xtask scenario extensions windows achievements --jobs 3 --owner fifth-d3c3e1 --out target/scenarios/fifth-deepening-20261113` built the player build with `dev-test-games` only and ran three worker threads side by side:

| Scenario | Claim | Processes | Outcome | Minutes | Other games beside it |
| --- | --- | --- | --- | --- | --- |
| `extensions` | `fifth-d3c3e1-extensions-3hu9k7` | 88204 | passed | 1.8 | 3 |
| `windows` | `fifth-d3c3e1-windows-ps7w27` | 96976 | passed | 1.6 | 3 |
| `achievements` | `fifth-d3c3e1-achievements-74re6d` | 90192, 82708 | passed | 1.7 | 3 |

- **Junction-free private mod folders (G1) and process handoff (G3)**: each claim created a plain marked `user\mod\` directory of relocated `.mod` descriptors without `mklink /J` or `mod-registry\`. All four `eu4.exe` processes handed their open `Game` handles by value to `follow.exe` without a shared static slot, exited with `0x1`, and were recorded in `game-ends.tsv`. Each scenario sampled `games_beside: 3` (the outside `eu4.exe` plus the two sibling scenario games) once running, where sampling before process start previously reported `0`.
- **Bidirectional journal reading and handler-error deduplication (G2)**: `extensions` read all five extension journals through `eu4_harmony_logging::SourceMark`. `qa-steady` counted 272 days across 271 dates and recovered its save data 11 times (one autosave load and ten reloads); `qa-tasks` passed all 16 checks across both sessions and reported 1 dropped waiting task; `qa-failing` wrote its first three occurrences per error site in full (10 full error lines across sites and sessions) and summarized later repetitions through `Journal::count_repeated` (`repeated error: count=148 ... error in on_day`, `repeated error: count=2 ... error in on_month`) with zero duplicate end-of-session error lines.
- **Native windows and panel-strip entries (G4, G5, G7)**: `windows` completed all 28 steps across `new-game`, `extension-reload`, `in-game-load`, and `next-session` with `native_errors: []` and `extension_errors: []`. `qa-windows-extension-reload-2.png` and `qa-windows-next-session-2.png` were visually inspected: both rendered the native window with `"Native window: replaced rows"`, `"Replacement native row"`, and the `qa-windows` entry button on the panel button strip left of the papacy and Empire buttons with its independent number `4` centered beneath it.
- **Achievements**: tracked 178 achievements in a non-ironman Castile start and after an in-game load, and 0 with the setting off.

## Three-process ironman setup-save handoff (G6)

To exercise the main-thread ironman setup save that runs before the session's first safe frame (`on_session`), claim `fifth-ironman-d3c3e1` copied `Castilla-5.eu4` into its private `user\save games\`, mapped `mod/eu4-harmony-regional-combat.mod` inside its private `user\mod\` and `user\dlc_load.json` (leaving the player's files untouched), and added `qa-steady`:

1. **Process 82524**: loaded `Castilla-5` (`1470.3.13`), advanced 5 days to `1470.3.18` (`qa-steady day 1` to `day 6`), returned to the main menu (writing the ironman exit save), and stopped the process.
2. **Process 49068** (fresh Lua states, `qa-steady` uninitialized in memory): loaded `Castilla-5` (`1470.3.18`), advanced 3 days to `1470.3.21` (`qa-steady session: saved back 6 days`, then `day 7` to `day 10`), returned to the main menu, and stopped the process.
3. **Process 49160** (fresh Lua states): loaded `Castilla-5` (`1470.3.21`), advanced 1 day to `1470.3.22`, and returned to the main menu before `game release`.

In process 49160's `core.log`, the ironman setup save ran on the main thread (`tid=7856`) **173 ms before** `session 2` started and delivered `on_session`:

```text
23:30:57.482 ... tid=7856 ... source=core saves: loaded a save with data for 2 extension(s)
23:31:00.311 ... tid=7856 ... source=core saves: wrote the data of 2 extension(s), 1 string(s)
23:31:00.484 ... tid=7856 ... source=core session 2 started (from a loaded save: true)
```

In `extension-qa-steady.log`, `qa-steady` received its preserved state and continued unbroken:

```text
23:31:00.506 ... source=extension-qa-steady qa-steady session: saved back 10 days
23:31:00.509 ... source=extension-qa-steady qa-steady day 11 1470.3.21 reads 11
23:31:01.297 ... source=extension-qa-steady qa-steady day 12 1470.3.22 reads 12
```

Every claim (`fifth-d3c3e1-*` and `fifth-ironman-d3c3e1`) was released with no crash; `cargo xtask game status` afterwards reported `0 test games held, 0 of them running`.

## Not verified in the running game

- Fault injection during native panel-strip entry subscription (`link` failing after native `SUBSCRIBE`) and `ui::bind` mid-window rollback were verified by deterministic unit tests in `windows::entries` and `ui`, not by forcing native GUI faults inside `eu4.exe`.
- Nested `load_before`/`load_after` frames, corrupted `eu4_harmony` save blocks, fallback journal stems (`core-<run>-<serial>.log`), and `DeferredQueue` unwind recovery were verified by unit tests rather than induced in the live process.
- Only two of the eight `windows` captures were visually inspected, and hover pixels on panel-strip entries and full 1444–1820 `whole`/`release` scenario stretches were not re-run in this check.
