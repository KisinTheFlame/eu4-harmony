# 2026-11-15 Reformation center reference before repair (tested by me)

This run tests the creation/removal sequence found in [the crash investigation](../research/reformation-center-crash.md). It establishes the reference omission in the running game, separately from the reported crash's unknown history.

## Setup

- EU4 1.37.5, a new observe-mode game at 1444.11.11; no player save was loaded. The claim copied the current playset: Workshop ids `253263609`, `2830362508`, `2976470733`, `1999055990`, `3253723536`, `3837775887`, `3837793510`, `3838535796`, plus the test mod. The framework report lists Regional Combat and Convenience Tools extensions. This was not a vanilla-only run.
- Framework built from `5d26a83844` with `dev-test-games`, before the center repair was registered or built. Test process 88780.
- Claim `reformation-20261115-87ec`, hidden and silent, with its own user folder, core and pipe. The player's installation and saves were untouched.
- A temporary test mod, `Harmony Reformation Center Reproduction`, supplied three hidden, triggered-only province events. The events used native game effects; no province or center field was written by a diagnostic tool.
- A read-only snapshot script opened only the process named by this checkout's claim. It read province progress (`+0x830`), converting center (`+0x838`), located center (`+0x840`), and the centers' source/target fields. It did not make allocations or call functions in the game.

## Steps

1. Run a native province event on province 50: its owner enables `protestant` and `reformed`, then the province executes `change_religion = protestant` and `add_reform_center = protestant`.
2. Run one day through `time.advance`. Center A naturally chooses province 51 as its conversion target. Pause and capture both provinces.
3. Run a native province event on province 51: `change_religion = reformed`, then `add_reform_center = reformed`. Capture both provinces without advancing a day.
4. Run a native province event on province 50: `remove_reform_center = protestant`. Capture both provinces again without advancing a day.

The console requests used `event harmony_reformation_repro.<1|2|3> <province>`, through `game console run --test-game reformation-20261115-87ec`. Every event answered `Executed`; advancing one day answered `1444.11.12`, stopped because the date was reached.

## Results

A was at `0x1596dbe1170`; the newly created B was at `0x1596e402b70`. These are this process's addresses only.

| State | A's target | Province 51's converting center | Province 51's progress | Province 51's located center |
| --- | --- | --- | --- | --- |
| After one day | 51 | A | 287 | None |
| After creating B | None | A | 287 | B |
| After removing A | A no longer located in province 50 | A's old address | 287 | B |

The progress is the stored integer, in thousandths. After removal, A's old bytes still had the center vtable; that does not make the allocation live. The native removal effect cleared province 50's located-center pointer, but province 51 retained the forgotten pointer.

A checker over the three captured snapshots required both old references and the province progress to be cleared, while preserving B. It failed on the actual creation snapshot:

```text
AssertionError: created: province 51 still refers to old center 0x1596dbe1170
```

This is an executed counterexample to complete detachment. It did not produce the reporter's access violation or a `building_prov` allocation at A's old address. There was no attempt to force allocator reuse or corrupt memory. It does not prove that the reported province 20 followed this history, and it does not exercise the trading-policy creation path.

The test process was stopped through its claim after the captures. The same claim was retained for the [repair comparison](2026-11-15-reformation-center-reference-after.md); final release and cleanup are recorded there.
