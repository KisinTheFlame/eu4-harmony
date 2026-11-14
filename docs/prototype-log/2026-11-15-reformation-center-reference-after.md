# 2026-11-15 Reformation center reference after repair (tested by me)

This run repeats the native-event sequence from [the failing run](2026-11-15-reformation-center-reference-before.md) with the repair in `core/src/repairs/reformation_centers.rs`. The [research note](../research/reformation-center-crash.md) explains the two creation paths and the limits of associating them with the supplied crash dump.

## Setup

- Branch `claude/reformation-center-crash-20261115`, based on `5d26a83844`, with the uncommitted center repair; core built with `cargo build --release -p eu4-harmony-core --features dev-test-games`.
- Actual copied core: 13,810,176 bytes, SHA-256 `27257541b494924c70fa64c955b2bc2063418582a22ccfa0e67b4e58dc700ea8`.
- Process 104528, a fresh observe-mode game at 1444.11.11. Same claim, copied playset, hidden test mod and read-only snapshot script as the failing run. No player save loaded, no installation into the game folder.
- Startup report: `baseline 1.37.5: 2543 dependency points match; 311 hooks, 90 writes`. The preceding run had 2,536 points and 309 hooks: the repair's seven checked points and two jumps are present.
- The first startup attempt had been refused because the copied core lacked the test-game pipe marker. Rebuilding with the explicit feature resolved it. A later workspace rebuild and a stop command encountered Windows executable locking while another `xtask.exe` was running; building the core package and using the already-built claim command resolved that. No repair source change was needed for either build issue.

## Sequence and results

Use the same three native province events as before: create Protestant A at province 50, advance one day and read its naturally selected target P, create Reformed B in P, remove A at province 50. No diagnostic code writes to the game's state.

In this run the natural target was province 1759, rather than province 51. The script follows the target the game actually selected; it does not manufacture a target or rely on a particular random choice. A was at `0x12815b6d5d0`, B at `0x12815fec360`.

| State | A's target | P's converting center | P's progress | P's located center |
| --- | --- | --- | --- | --- |
| After one day, 1444.11.12 | 1759 | A | 285 | None |
| After creating B | None | None | 0 | B |
| After removing A | A no longer located in province 50 | None | 0 | B |
| Seven days later, 1444.11.19 | Removed | None | 0 | B |

The same captured-state checker that failed before now answers:

```text
PASS: creation clears both old references and progress; removal preserves the new center
```

After seven more native daily updates, B had naturally selected province 59 and accumulated conversion progress 1,981 (stored thousandths). A further read checked both sides: B's target was province 59, province 59's converting-center reference was B, and their progress values agreed and were positive. The source province still had no converting-center reference and zero progress of its own. This checks continued operation beyond the immediate detach, rather than merely observing no crash. The inspected framework logs contained no detach failure, unhandled exception or panic.

```text
PASS: B targets province 59; both sides refer to B with progress 1981
```

## Repository checks and cleanup

- `cargo fmt --all --check` and `cargo build --release --locked` passed.
- `cargo test --workspace --locked`: 1,174 passed, none failed, six ignored.
- Both workspace Clippy checks passed with `-D warnings`, with default features and with `--features eu4-harmony-core/dev`.
- `cargo xtask offline-check` passed in both its normal and developer configurations, including installed-game dependency bytes and observer argument checks. The address-index tests also passed (three tests).
- A separate read-only review checked the two stubs, calling convention, continuation addresses, setter arguments, registration and dependency declarations and found no confirmed defect in that scope. It did not independently disassemble the game or execute the policy path.
- Released claim `reformation-20261115-87ec`. Both owned processes, 88780 and 104528, ended through test-game stop/release with exit code `0x1`; the release command reported no crash. A subsequent `game status` listed no claim or unregistered folder belonging to this run.
- Removed the temporary event mod, read-only snapshot script, checker and captured JSON files from `target/reformation-repro-assets/`, and the copied event mod left in the released claim's `test-mods/`. Checked these were ordinary files before removal. The released claim now contains only `kept/`, retaining framework/game logs and process-exit evidence for the tool's seven-day retention period. The supplied crash materials, player files and installed framework were untouched.

## Limits

- This executes the manager creation path, including creation with no old reference in the first event and with an old reference in the second. The trading-policy creation path uses the same repair callback at a separately checked jump, but was not driven through a merchant policy in this run.
- The reporter's final access violation and allocation reuse were not reproduced. This run verifies the demonstrated reference omission and its repair; it does not reconstruct the reported province 20's history.
- The playset was copied from the player and included Regional Combat and Convenience Tools; this was not a vanilla-only comparison. No extension source was changed for the test.
- Map invalidation is performed by the game's own setter and its instructions were read. No visual screenshot comparison of conversion colors was made.
- This is an in-game regression with native events and captured references. The snapshot reader/checker are temporary investigation tools, not a new public memory capability or a retained CI test; CI cannot launch the installed game for this sequence.
