# 2026-11-12 Reusing the prepared highlight capture buffers (tested by me)

A review follow-up to [the final source smoke check](2026-11-12-core-highlights-final.md). The previously reused buffers were the drawing thread's TLS reader only. The preparation still set its `Option<Kept>` to `None` each frame and allocated two new vectors through `Memory::bytes`, while holding the presentation lock. The review finding was current and correct.

## Correction and focused regression

Preparation now initializes its `Kept` storage only once, retaining both vectors. `resize` and `Memory::read_into` refill them; counts that fit their capacities reuse the allocations. A separate `valid` flag is cleared before any read and set only after both arrays and the country limit pass. Failed or partial capture retains capacity but cannot supply marks to the border classifier. The highlight inputs, publication, hook, game clearing and drawing cadence are unchanged.

The capacity regression failed on the previous capture implementation: after shrinking the arrays, capacities were `(64, 32)` instead of the original `(128, 64)`. With the correction it checks the same source buffers at full size, smaller size, empty and full size again, asserting both capacities and allocation pointers remain unchanged. Another regression injects failures in the first array read, second array read and final country-limit validation: each invalidates the snapshot, rejects it even after the source is repaired, retains capacity, and recovers on the next successful capture. Both existing marks lifecycle tests still pass.

Focused checks all passed:

- `cargo fmt --all --check` and `git diff --check`.
- `cargo test -p eu4-harmony-core drawing::marks::tests`: 4 passed.
- `cargo clippy -p eu4-harmony-core --all-targets -- -D warnings`.
- The same Clippy command with `--features dev`.
- `cargo build --release -p eu4-harmony-core`.
- `cargo build --release -p eu4-harmony-core --features dev-test-games,dev-test-switches`.

No dependency point changed; the prior workspace tests and offline dependency checks were not repeated. This follow-up demonstrates buffer reuse, not a measured speed gain.

## Final short game check

Same branch `claude/core-highlight`, master base `daf21a47`, independent tooltip guard `4b0c408e` and uncommitted highlight correction. The completed final test core's SHA256 was `24e4ec4c67b873c6b140ac8dc28e99531fc424625c8f7a1930d2e9e9608989b4`.

Claim `core-highlight-20261112-193017`, owner `core-highlight`, hidden and silent, own core/user folder/pipe. Loaded `observer-1700.1.1`, then repeated only the original province-151 Ottoman-core hover, same playset and screenshot criterion as the baseline. Eight red-pixel counts: `[823, 235, 6, 6, 6, 13, 457, 855]`. The outline assertion passed. The core log had no crash record or temporary-highlight capture/read error.

Released immediately after the capture. Release reported no crash; status showed 0 held and 0 running test games. No installation, new diagnostic switch, commit, push, pull request or release. The incremental code patch and checks are `target/core-highlight-evidence/capture-reuse.patch` and `capture-reuse-checks.txt`; screenshots and raw results are in `capture-reuse-final/`, `capture-reuse-smoke-output.txt` and `capture-reuse-core.log`. The earlier lifecycle and 180-day checks were not repeated or expanded.
