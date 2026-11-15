# Reformation-center icon crash on 2026-11-14

This note examines the reported main-thread access violation in the reformation-center map icon update. It informs map-icon drawing and the repair in `core/src/repairs/reformation_centers.rs`. The dump establishes the invalid object interpretation at the fault; static reading finds a center-creation path that leaves a province referring to an old center after its normal deletion. An isolated test game reproduced that reference omission through native events. This establishes a way the pointer can become stale, but does not establish that the reported run took it. Game addresses are RVAs into the EU4 1.37.5 baseline.

## Evidence and identity

The supplied troubleshooting files recorded a crash at 2026-11-14 19:07:21.323, game date 1522.12.8, process 17604, main thread 3556. The matching original crash folder supplied separately contained the dump that the old collection limits had omitted.

- Dump: `minidump.dmp`, 76,924,260 bytes, SHA-256 `f9a742c02a3f03ebf431d8db7091eed4dd66c77d5c602ab514b3fcd3023327c7`.
- Game: EU4 v1.37.5.0 Inca, SCM commit `835bfdf8ca24c291a1b3f1b5bc72d47e7df1ae18`.
- Dump game image: timestamp `66fe6ab9`, image size `025e0000`, PE checksum `024baf19`. All three match the installed executable used for static reading. This is a metadata comparison, not a whole-file byte comparison.
- Framework: Harmony 0.2.1, capability API 0.3; Regional Combat 0.4.0 and Convenience Tools 0.1.0 were loaded. Startup accepted 2,518 dependency points, 302 hooks and 90 writes. Acceptance does not prove runtime correctness.

The crash record belongs to the earlier run's `core.prev.log`; `core.log` in the troubleshooting files belongs to a later start. Four `Ended(..., Country)` signals shortly before the fault describe country changes, not four in-game loads. Neither those signals nor the playset establishes causation.

The system's `dbgeng.dll`, through `cargo xtask crash-dump`, read the exception context, registers, captured objects and stack. The installed baseline was read with `cargo xtask disasm`. The dump and logs were treated as data and were not uploaded. No matching PDB for the reporter's core was available; framework addresses must not be named from the debugger's fallback symbol labels.

## Fault and complete local pointer chain

The exception is `c0000005`, a read of address `1114`. The exception context puts the instruction at `0x132f43b`, inside the function beginning at `0x132f1c0`.

```text
RVA       instruction
132f41b   mov r15, [rsi+838h]
132f422   test r15, r15
132f425   je ...
132f42b   mov rdx, [r15+10h]
132f42f   test rdx, rdx
132f432   je ...
132f434   add rdx, 90h
132f43b   cmp qword ptr [rdx+18h], 10h

rsi = 00000250266959a0
r15 = 00000250b776bc70
rdx = 00000000000010fc
[r15+10h] = 000000000000106c
```

Thus the nonzero value `106c` (decimal 4204) passes the pointer check, becomes `10fc`, and causes the read at `1114`. This is not evidence of a direct null pointer plus an offset. A second null check would not catch this value.

The province's captured header has vtable `0x1cd6640` and province number 20 at `+0x20`. The caller and callee pass it consistently as the province. Mods were loaded, so the province number is not assigned a vanilla place name here. The dump does not contain the province's `+0x838` page: that field cannot be reread from the dump, but `r15` retains the value already loaded by the displayed instruction chain.

The relevant game frames are:

```text
132f43b <- 132b23e <- 13229be <- 10a7f87 <- 10a6d6f
        <- 81582e <- framework observer frames <- 14c2818
        <- framework observer frames <- 769fb7
```

The return at `0x769fb7` is in the game's check for a frame inside a day, beginning at `0x769ea0`. The fault is in the main-thread interface update. Passing through framework observers is not evidence that an observer damaged the object.

## The object actually present

The captured object at `r15` has this layout:

| Offset | Captured value | Meaning established from construction and persistence |
| --- | --- | --- |
| `+0x00` | image base plus vtable `0x1c415a0` | `building_prov` record's vtable |
| `+0x08` | low 32 bits `165` | value initialized by the constructor; no further meaning assigned |
| `+0x10` | `106c` | 32-bit `id`, decimal 4204 |
| `+0x18` | inline `temple` | `key`, a string |
| `+0x28` | `6` | string length |
| `+0x30` | `f` | string capacity |
| `+0x38` | `1` | 32-bit `value` |

This establishes the wrong object interpretation at the fault. It does not establish that a center was freed and this allocation reused its address. An earlier wrong-pointer write and other memory corruption histories remain distinguishable possibilities for which this dump has no history.

## Captured-memory coverage and framework limits

An isolated Python parser scanned the dump's recorded memory ranges, without executing their contents: 1,070 ranges totaling 75,078,427 bytes. No aligned instance of the real center's vtable was found in those ranges. The fault object's address appeared twice as an aligned pointer, both on the captured stack. The relevant province fields were not captured. These are coverage observations, not evidence that the live process had no centers or no other references; this minidump did not retain the required heap history or source/target graph.

A focused source inspection of the framework before this repair found no direct center-field handling or center-object ownership in the province snapshot, map-icon cache and inspected repair paths. The map-icon cache in `core/src/drawing/icons.rs` keeps a window, copied positions and a matrix address; it does not keep a center object. It does retain raw window and matrix addresses, so this observation does not prove all lifetimes safe.

Current `core/src/drawing.rs` and `drawing/turns.rs` observe the game's application frame and check for a frame inside a day. Application-frame entry takes the presentation lock unless already held; the daily update releases it and a frame inside the day can take it again. The inspected path does not add a separate call to run a frame. These are current-source observations, not a reconstruction of the reporter's exact DLL or its lock state at the fault. They cannot exclude indirect game calls, erroneous writes elsewhere, or concurrency involving uncaptured state.

The game error log contains missing `idle` states for `reformation_protestant` and `reformation_reformed`. No timing relationship to the invalid pointer was established. Those messages are not used as a cause.

## What is established and what is still missing

Established from the dump: the main-thread icon update followed the province's converting-center pointer to a structurally consistent `building_prov` record and treated its integer id as a pointer. The exception address follows exactly. Established from static reading: creating a center in an existing conversion target discards the old center's target without clearing the province's reference, and normal later removal can free the old center while that reference remains.

Not established: that province 20 underwent this sequence, which center previously occupied the fault address, when it was freed, and how a 64-byte building record came to occupy the address read as a center. No pre-crash save with the same playset or runnable reproduction was supplied. The dump omits the relevant province pages, contains no complete center graph, has incomplete secondary thread stacks, and was written by crash handling after the original exception. It cannot prove or exclude a race or another corruption history. The baseline defect is established separately from responsibility for this reported occurrence.

The [native-event run before repair](mechanics-evidence.md) reproduces the missing detach: center A at province 50 naturally targets province 51; creating a Reformed center B in province 51 clears A's target but retains province 51's converting-center pointer and progress. Removing A leaves that pointer behind. The checker fails on the captured creation state. This is an executed reference-lifetime counterexample, not a reproduction of the reporter's final access violation or allocation reuse.

## Repair at successful center creation

`core/src/repairs/reformation_centers.rs` completes the detach at `0xdae38f` and `0xd37b17`, only when creation actually encounters an existing converting center. Each stub replays the old center's target clear and the displaced type-definition load. Between them it calls the game's setter at `0xa16a50` with the source province, zero progress and a null converting center. This clears the reverse reference and progress and marks the map colors stale. It leaves the new center's location, type, religion and later insertion to the original creation function.

The manager stub takes the source from the saved game `rbp` in the shared call frame; the policy stub takes it from `rdi`. Both preserve the flags and volatile registers around the call. Dependency points check the old-center load and conditional branch, each eight-byte replacement, each continuation, and the setter's field writes. A null old reference follows the original branch without entering the repair. The policy's earlier refusal paths are also untouched.

The [repair comparison](mechanics-evidence.md) passes the same captured-state checker: after creation and removal, the province's converting reference is null, progress is zero, and its new located center remains. Seven further daily updates leave that source detached while its new center starts converting province 59. The trading-policy creation path was checked statically and against the installed game's bytes, but not exercised through a merchant policy.

This prevents the demonstrated reference omission on future successful creation. It does not scan for arbitrary preexisting corrupt pointers, establish the original report's history, or promise recovery from unrelated memory corruption. For the reported occurrence, the affected province's writes and allocation/free history still need capture. The original supplied files remain intact.

The systematic game account is [reformation-centers](../game/provinces/reformation-centers.md).
