# 2026-11-14 Sixth deepening (tested by me)

The [sixth deepening](../plans/sixth-deepening.md) gathers the courses of the
speed-ups that run beside the game into modules whose interfaces carry and
test the whole course: the parallel save (H1), the army searches (H2), the
background compression (H3), the performance collection's clock (H4), and the
folder listing and Windows-name rules written once (H5). Nothing changes for
players, extensions or remote clients; this run checks that the gathered
courses still answer as the game does in the running game.

## Build and setup

Branch `claude/improve-codebase-architecture-94a6d2`, merged with master at
`ad190406` (#196). Vanilla 1.37.5. Three hidden test games, claimed and
released by `cargo xtask scenario`, each with its own user folder, core and
pipe:

- `checked` — the test-switches build (`dev-test-games,dev-test-switches`),
  `[test-switches] checked = ["all"]`, a new game in observe mode from
  1444.11.11 for 3 years, an autosave written and loaded each year
  (`--checked.years 3 --checked.save_every 1`). Every checkable speed-up
  compares its answer with the game's; `army-searches` differences are counted,
  not failed (ADR the-game-may-be-made-faster-with-reasoned-differences).
- `perf` — the performance-collection build
  (`dev-test-games,dev-performance`), a new game as MNG, 30 s paused and 120 s
  at speed 5, bounded collection on.
- `late` — the player build, the developers' late saves; see "Not verified".

The offline checks (`cargo xtask offline-check`) passed on the release build
against the installed game: every point matches, every observer forwards the
arguments its function reads. The workspace tests (890 in the core with the
developer features), both clippy configurations and the locked release build
passed before the run.

## What came out

### checked: passed

Three years ran at 603–800 game days a minute, private bytes 4111–4139 MB. The
autosaves of 1445.11.11 and 1446.11.11 loaded in the game (9.8 s, 10.0 s), each
on the date it was written. The speed-ups checked against the game:

| Speed-up | Answers checked | Differing |
| --- | --- | --- |
| parallel-saves (H1) | 12 | 0 |
| army-searches (H2) | 2520 | 228 (reasoned, counted) |
| lookups (H5) | 431603 | 0 |
| listed-folders (H5) | 978542 | 0 |
| modifier-sums | 14527172 | 0 |
| trigger-profile | 2345254 | 0 |
| script-parameters | 320905 | 0 |
| localisation | 579 | 0 |
| music | 0 | 0 |

The parallel save's bytes are the game's to the bit (12 saves checked, none
differing); the folder listings answer as the game's stat does (1.4 million
checks, none differing). The army searches' 228 differences are the reasoned
ones the ADR allows, counted not failed. The core log shows the pools started:
`saves: 7 threads to write saves with`, `army searches: 8 threads to search
with`.

### perf: passed

Startup to the main menu 28.7 s; `menu.start` 5.1 s. Paused CPU 1.257 cores,
speed-5 CPU 3.303 cores; 776.5 completed game days a minute in the speed-5
window. Both collection windows are complete with no lost events: paused
30.000 s, 13252 events; running 120.000 s, 29105 events. The running window
carries the local-save markers (nine `local save begins`/`ends` pairs), and the
core log shows the parallel save's join (H1) and the background compression's
save stop (H3) on each autosave, for example:

```
saves: wrote 4942 provinces and 1380 countries on 8 threads beside the game's writing, 33160072 bytes; waited 49.3ms for them, 61.2ms after the provinces' loop
saves: a local save stopped the game for 121 ms
```

The collection's lifecycle (H4) drove these windows through its clock seam:
the deadline bounded each window exactly (30.000 s, 120.000 s retained), the
context split the timeline into segments, and the report replays them.

## Not verified

- **The late game (`late`) did not run.** The developers' observer saves were
  made with a mod the game names `Regional Combat (EU4 Harmony)`; the test
  game's playset did not have it, and `menu.load` refused the save rather than
  risk a crash. Adding the repository's Regional Combat mod
  (`D:\Workspace\eu4-regional-combat\mod`, whose descriptor names it
  `Regional Combat`) did not match the name the saves carry. This is a
  playset/mod-name matter in the test data, not a framework defect; the game
  did not crash. The late-game large-save path was therefore not exercised
  separately, but the same H1/H2/H3/H5 code paths were checked in the `checked`
  run above, and H1's join wrote a 4942-province, 1380-country game state in
  the `perf` run.
- **The background compression's completion line was not captured.** The
  course's save-stop markers appear (above), and the autosaves loaded in the
  `checked` run, which waits for the worker first; but no
  `compressed in the background` line was in the kept core logs of these two
  runs. The compaction itself is the unchanged deep module
  (`saving::background::compact`), tested by its own unit tests; whether these
  particular saves' entries were left uncompressed or the line fell outside
  the kept log window was not determined.
- Pure interface work, action response and GPU time are unmeasured by the
  collection (as `perf` notes); submission is the game's Present wrapper
  returning success, not monitor display.
