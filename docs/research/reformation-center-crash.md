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

## What the icon update reads

The function at `0x132f1c0` was read through its end. It places and projects a map icon through the existing common paths (`0x132e300` and `0x132e210`), then updates elements named `reformation_conversion_mapicon`, `reformation_conversion_progress`, `reformation_conversion_progressframe`, `religious_conversion_center_frames` and `reformation_conversion_icon`.

It reads both province pointers `+0x840` and `+0x838`. The former identifies the center located in this province; the latter identifies the center currently converting this province. For each nonzero pointer it interprets the center's `+0x10` as its type definition, then reads a string at that definition's `+0x90`. It also uses the center's religion at `+0x28` to obtain the icon frame through a field at `+0x570`. The `+0x838` branch uses the province's progress at `+0x830`. These field meanings follow from the creation, daily update and persistence paths below.

The caller at `0x132b1b0`, also read through its end, checks its display arguments and holder state. A nonzero province `+0x838`, or a bit in province `+0x959`, permits the icon update. It obtains the map position through the province's `+0x28` and passes the holder's icon to the callee. Its call returns at `0x132b23e`. This caller does not support an explanation in which the immediate argument was an unrelated non-province object.

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

The constructor at `0x194da0` installs that vtable, writes the integer at `+0x10`, constructs the string at `+0x18`, and writes the integer at `+0x38`. Its deleting destructor at `0x194d60` destroys that string and frees a block of 64 bytes. Values captured after the first 64 bytes are not assigned to this object.

The field reader at `0xd7c080` and writer at `0xd7c100`, each read through its end, map these fields to tokens `id` (11), `key` (220) and `value` (1004). The static token-name table begins at `0x1fd8660`, with 6,648 entries of 520 bytes. The parent reader at `0x17b460` allocates these records and adds them to its list at `+0x280`; the corresponding loop in the parent writer at `0x17ca00` writes each under token `building_prov` (11500). Only the relevant part of that large parent writer was read.

The update wrapper at `0x17e8e0` passes the parent's `+0x280` list to `0x17f900`. These two functions were read through their ends. The latter looks up a record by integer id and string key, adds an integer to its value, creates a record when needed, and retains it only while the resulting value is positive; otherwise it destroys the string and frees the 64-byte record.

Together the vtable, constructor, field readers/writers, parent save key and captured string identify a structurally consistent `building_prov` record with `id=4204`, `key="temple"`, `value=1`. This is a persistence record, not the province's building construction object. The icon update is interpreting an object with a different layout: its supposed pointer is this record's integer id, and its supposed pointer at `+0x28` is the string length.

This establishes the wrong object interpretation at the fault. It does not establish that a center was freed and this allocation reused its address. An earlier wrong-pointer write and other memory corruption histories remain distinguishable possibilities for which this dump has no history.

## The real center and its owner

The real center has vtable `0x1c645c0` and is allocated with size 72 bytes. Its field reader at `0x44a7b0` and writer at `0x44aae0`, both read through their ends, identify the fields using the token table:

| Center offset | Saved key | Meaning |
| --- | --- | --- |
| `+0x10` | `type` | Center type definition |
| `+0x18` | `province_id` | Province where this center is located |
| `+0x20` | `any_target_province` | Province currently being converted, or null |
| `+0x28` | `religion` | Religion being spread |
| `+0x30` | `max_speed` | Stored conversion speed |
| `+0x34` | `missionary_progress` | Stored conversion progress |
| `+0x38` | `owner` | Saved country tag |

The religion manager's per-religion record owns its centers in an array: data at `+0x20`, capacity at `+0x28`, count at `+0x2c`. The reader at `0xdad740` allocates a center for each `reformation_center` block, reads it, and retains it only if its source province, religion and type are nonnull. The writer at `0xdadad0` writes the array under that key. Both functions were read through their ends.

The post-read fixup at `0x44ac30` writes the source province's `+0x840` and sets bit 0 at `+0x959`. For a target it calls `0xa16a50`, which writes the supplied progress to province `+0x830` and the center to `+0x838`, marking the map state stale when needed. This setter does not change the center's target field or detach another center. Both short functions were read through their returns.

The daily conversion update at `0x448000`, read through its return and cold exits, keeps the target's progress and `+0x838` up to date. When the target already has the center's religion, it clears that target's progress and pointer before choosing another. Target selection at `0x449990` builds candidates and delegates their filtering and ranking to `0xa81770`; both were read through their ends. The latter rejects a candidate with a nonzero province `+0x838` at `a818f7`, so a forgotten nonzero pointer does not become available for selection merely because its center has moved on.

## A creation path leaves the old target's reference behind

Two center-creation functions have the same asymmetric detach:

- `0xdae300`, the per-religion manager's creation operation, allocates the new center with no target and records the supplied province as its source. At `dae383` it loads that province's `+0x838`; if nonnull, at `dae38f` it sets the **old center's** target `+0x20` to null. It then sets the source province's center flag and `+0x840` to the new center and adds the new center to the owning array.
- `0xd37940`, the `propagate_religion_policy` path, performs the same sequence at `d37b0b` / `d37b17` before setting the province's `+0x840` and inserting the new center.

Both were read through their ends. Neither clears the province's `+0x838` or progress when it discards the old center's target. In the manager path, `rcx` is explicitly zero before the write at `dae38f`; in the policy path, `rdx` is explicitly zero before `d37b17`.

The critical manager-path instructions are:

```text
dae383  mov rax, [rbp+838h]  ; old center converting the new source province
dae38a  test rax, rax
dae38d  je ...
dae38f  mov [rax+20h], rcx   ; rcx = 0: old center forgets its target
...
dae3cd  or byte ptr [rax+959h], 1
dae3d4  mov [rax+840h], rdi  ; source province points to the new center
```

This is not solely a field-reader artifact. An effect implementation at `0x5ff320` calls the manager operation at `5ffa8d`; its province branch tests the source-center flag and the religion's check, but does not require province `+0x838` to be null. A console handler at `0xe5fbf0` resolves a province id and center type and also calls it at `e5fd30` without that check. Both were read through their ends. The installed game's religion definitions use `change_religion` followed by `add_reform_center`, and its Bohemian events use `remove_reform_center`. The isolated reproduction used those native effects. The wider dispatch and all religion-check callees were not recovered, so this does not enumerate every allowed game scenario.

A further caller at `0xa3bef0` changes the province's religion first, at `a3bf81`, then calls center creation by a tail jump at `a3bf9f`. Changing religion is not an unconditional guard: the concluding logic of `0xa15ff0` clears an incomplete conversion only when the new province religion equals the old center's religion. A change to a different religion can retain the old pointer. The tail at `0xa16720` refreshes scripted state through virtual callbacks; those callbacks were not all resolved and may have scenario-specific effects. This caller was read through its return/tail exits, and the tail through its return.

## Normal removal cannot find the forgotten province

The cleanup at `0x449f10`, read through its return and cold exits, clears the source province's center flag and `+0x840`. It then loads the center's **current** target at `449f76`. If null, `449f7d` jumps over target cleanup. If nonnull, it clears only that target's progress and `+0x838`. It does not search all provinces for references to this center.

```text
449f76  mov rdx, [rsi+20h]
449f7a  test rdx, rdx
449f7d  je 449feeh
449f7f  mov [rdx+830h], r12d  ; r12 = 0
449f86  mov [rdx+838h], r12
```

The normal manager removal by source province at `0xdae450` calls that cleanup at `dae4ba`, then invokes the deleting destructor during array removal at `dae52e` or `dae575`. Removal by array index at `0xdae5a0` does the same at `dae5cb`, with deletion at `dae641` or `dae685`. These two removal functions were read through their ends in the parallel investigation; their critical cleanup and deletion branches were checked again against the executable. The real center's deleting destructor at `0x2dc670`, read through its return, only tests the deleting flag and frees 72 bytes. It performs no additional detach.

The daily religion update at `0x75ae70` was read through its end in the parallel investigation. It checks each center's source province, religion/type and relevant game conditions, calling the conversion update when valid and the indexed removal when invalid. Thus normal daily removal can complete the dangling-reference path; a world reset or framework intervention is not required by this path.

The state transitions established by these instructions are:

```text
A is converting P:
    A.target = P
    P.converting_center = A

create center B in P:
    A.target = null
    P.located_center = B
    P.converting_center = A       # retained

A chooses another target or has no target; later remove A:
    cleanup clears A's current target, if any
    free A
    P.converting_center = old A   # retained unless another operation cleared it
```

This is a concrete static dangling-reference path in the game baseline. Subsequent use can read freed memory. If an allocation later occupies that address, the icon can interpret its fields as a center. The actual center and captured `building_prov` record have different requested sizes, 72 and 64 bytes; no allocator history, size-class equivalence, or reuse of this particular block is established by their layouts.

Other lifecycle observations do not close the gap:

- The province reset at `0x9dcc10` clears progress and both center pointers. A later reset could remove a stale reference, but the dump does not establish that one ran for this province.
- The per-religion record's destructor at `0xdad6e0` uses the owning-array cleanup at `0x116050` to delete its centers directly. This whole-container destruction does not itself detach province references. World teardown ordering was not recovered completely, so it is not labeled an additional defect.
- The inspected manager-array growth transfers center ownership and zeroes the old slots before deleting the old records; it did not reveal a second deletion of transferred centers.
- Matching offsets in another persistent object's move operation were excluded because its vtable differs from the province's. Shared base vtables likewise do not identify a center type.

## Captured-memory coverage and framework limits

An isolated Python parser scanned the dump's recorded memory ranges, without executing their contents: 1,070 ranges totaling 75,078,427 bytes. No aligned instance of the real center's vtable was found in those ranges. The fault object's address appeared twice as an aligned pointer, both on the captured stack. The relevant province fields were not captured. These are coverage observations, not evidence that the live process had no centers or no other references; this minidump did not retain the required heap history or source/target graph.

A focused source inspection of the framework before this repair found no direct center-field handling or center-object ownership in the province snapshot, map-icon cache and inspected repair paths. The map-icon cache in `core/src/drawing/icons.rs` keeps a window, copied positions and a matrix address; it does not keep a center object. It does retain raw window and matrix addresses, so this observation does not prove all lifetimes safe.

Current `core/src/drawing.rs` and `drawing/turns.rs` observe the game's application frame and check for a frame inside a day. Application-frame entry takes the presentation lock unless already held; the daily update releases it and a frame inside the day can take it again. The inspected path does not add a separate call to run a frame. These are current-source observations, not a reconstruction of the reporter's exact DLL or its lock state at the fault. They cannot exclude indirect game calls, erroneous writes elsewhere, or concurrency involving uncaptured state.

The game error log contains missing `idle` states for `reformation_protestant` and `reformation_reformed`. No timing relationship to the invalid pointer was established. Those messages are not used as a cause.

## What is established and what is still missing

Established from the dump: the main-thread icon update followed the province's converting-center pointer to a structurally consistent `building_prov` record and treated its integer id as a pointer. The exception address follows exactly. Established from static reading: creating a center in an existing conversion target discards the old center's target without clearing the province's reference, and normal later removal can free the old center while that reference remains.

Not established: that province 20 underwent this sequence, which center previously occupied the fault address, when it was freed, and how a 64-byte building record came to occupy the address read as a center. No pre-crash save with the same playset or runnable reproduction was supplied. The dump omits the relevant province pages, contains no complete center graph, has incomplete secondary thread stacks, and was written by crash handling after the original exception. It cannot prove or exclude a race or another corruption history. The baseline defect is established separately from responsibility for this reported occurrence.

The [native-event run before repair](../prototype-log/2026-11-15-reformation-center-reference-before.md) reproduces the missing detach: center A at province 50 naturally targets province 51; creating a Reformed center B in province 51 clears A's target but retains province 51's converting-center pointer and progress. Removing A leaves that pointer behind. The checker fails on the captured creation state. This is an executed reference-lifetime counterexample, not a reproduction of the reporter's final access violation or allocation reuse.

## Repair at successful center creation

`core/src/repairs/reformation_centers.rs` completes the detach at `0xdae38f` and `0xd37b17`, only when creation actually encounters an existing converting center. Each stub replays the old center's target clear and the displaced type-definition load. Between them it calls the game's setter at `0xa16a50` with the source province, zero progress and a null converting center. This clears the reverse reference and progress and marks the map colors stale. It leaves the new center's location, type, religion and later insertion to the original creation function.

The manager stub takes the source from the saved game `rbp` in the shared call frame; the policy stub takes it from `rdi`. Both preserve the flags and volatile registers around the call. Dependency points check the old-center load and conditional branch, each eight-byte replacement, each continuation, and the setter's field writes. A null old reference follows the original branch without entering the repair. The policy's earlier refusal paths are also untouched.

The [repair comparison](../prototype-log/2026-11-15-reformation-center-reference-after.md) passes the same captured-state checker: after creation and removal, the province's converting reference is null, progress is zero, and its new located center remains. Seven further daily updates leave that source detached while its new center starts converting province 59. The trading-policy creation path was checked statically and against the installed game's bytes, but not exercised through a merchant policy.

This prevents the demonstrated reference omission on future successful creation. It does not scan for arbitrary preexisting corrupt pointers, establish the original report's history, or promise recovery from unrelated memory corruption. For the reported occurrence, the affected province's writes and allocation/free history still need capture. The original supplied files remain intact.
