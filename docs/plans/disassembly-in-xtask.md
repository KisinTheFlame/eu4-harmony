# Disassembly in xtask

Status: decided 2026-11-15; D1 to D3 built and checked offline the same day. Formatting, the locked release build, workspace tests, the command-line regression test and both clippy checks passed; all five installed-game offline checks passed in both player and developer builds. Nine disassembly argument forms passed against the installed game; all 67 prepared developer-core artifact files retained their SHA-256 hashes, modification times and sizes after `cargo xtask disasm`. Dependency trees confirmed that xtask excludes core and player core excludes analysis. No game was started. No new glossary terms or ADRs.

## Goal and scope

Run `cargo xtask disasm` without building the core or replacing the DLL, library or symbols prepared for a game. The command keeps its arguments and output. This updates the implementation behind the entry point introduced by [dev-commands.md](dev-commands.md); the current tool is described in [dev-tools.md](../dev-tools.md#disassembly).

The old entry point ran the core's example with a different feature set. Recorded runs lost their intended core features after disassembly ([call tracing](../prototype-log/2026-11-07-ai-country-task-timed.md), [test-game pipes](../prototype-log/2026-11-08-databases-and-meshes.md)). No game behavior, dependency points or installation files change here.

## How it fits together

- `xtask/src/disasm.rs` owns the command, finds the game through the existing folder lookup, reads the file and formats the analysis. It calls no nested Cargo command.
- `analysis/`, the independent `eu4-harmony-analysis` crate, owns the existing PE image and x64 instruction analysis and its tests. It depends on iced-x86, with no dependency on core or xtask.
- The core's offline tests depend on analysis as a dev-dependency. Its call tracing and write watching use the same crate through the optional `analysis` feature. Player builds do not require it.
- Instruction formatting is an xtask dependency. The example target and the core's analysis module are removed. Historical research and run records keep their original paths; current documentation explains the move.

## Parts

- **D1 Shared analysis:** move the existing implementation and tests into the new workspace crate and update the core's callers and dependency declarations.
- **D2 Command ownership:** move the example into xtask, dispatch directly, and update the help, current documentation and source lookup used by `cargo xtask what`.
- **D3 Validation:** run formatting, release build, workspace tests, both clippy checks and the installed-game offline checks. Exercise disassembly against the installed file and compare the core artifacts' hashes and modification times before and after it; confirm xtask's dependency tree excludes core.

Every part is done when its existing behavior is preserved, the checks pass, and disassembly leaves a prepared core build untouched. No running-game test is required: this change reads the installed executable without changing or starting it.
