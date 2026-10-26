# Native automatic-order adapter contract

Scope: approved unrestricted EXE SHA256 `97dd63a811487fbe2c917f2c15b1e68f7222bef74a833889cf943c8010049d5e`, x64 Windows. Addresses below are RVAs. This investigation performed static analysis only; it did not attach, call native functions, or issue orders. The frozen file hash was rechecked. New focused disassembly/decompilation is in `work/native-order/pseudocode.txt`; existing evidence is identified below.

The low-level Move and Stop ABIs are established. They are mutators, not legality checks, and neither returns acceptance. A complete adapter must gate calls, preserve exact command ownership, and verify state after each call. This document supplies that contract; it does not enable a production order writer.

## 1. Move: exact ABI and effect

```cpp
// Windows x64: RCX, RDX, R8B, R9B. No additional stack arguments.
void submit_path(Army* army, const PathList* path,
                 uint8_t replace_existing, uint8_t mission_reason); // RVA D50340
```

Frida declaration, when implemented, must return `'void'` with arguments `['pointer','pointer','uint8','uint8']` and `abi: 'win64'`. Do not interpret AL/RAX as success. The prefix is:

```
D50340: 48 89 5c 24 20 44 88 44 24 18 48 89 54 24 10 55
```

The prologue saves R8B, copies R9B to EDI, RDX to RSI and RCX to R15. Argument 3 is tested for zero; argument 4 is tested for zero but its original byte is subsequently forwarded to `D3B8B0(army,1,mission_reason,0,0)`. **Argument 4 is a reason-like byte, not merely a boolean.** `D3B8B0` distinguishes values `0`, `1`, `0x35`, `0x36` and other values when handling the native mission object at army `+0x1F8` and notifications. Its complete enum names have not been recovered.

Observed callers:

| Source | Call/return site | Arguments/evidence |
| --- | --- | --- |
| Original rebel mission tick `1C9D30` | Existing `work/focused-pseudocode.txt` | `submit(army,&temporaryPath,1,0)`; frees the temporary nodes afterward. |
| Ordinary movement builder `D5D680` | Call `D5D973`, return `D5D978` | `submit(army,&temporaryPath,replace,0x18)`; observed in `path-trace-01.jsonl`. |
| Attached-unit propagation inside submit | Call `D50BA6`, return `D50BAB` | `submit(child,inputPath,replace,0)`; also observed in that trace. |
| Retreat command execution `D6B910` | Call `D6B9EB` | `submit(army,path,0,0)`; this is not an ordinary automatic-movement precedent. |

For an independently managed automatic task, `(1,0)` matches the existing native automatic mission caller. It does not cancel a conflicting original native mission at `+0x1F8`; mutual exclusion with that mission must be established separately. Do not copy `0x18` simply because a manual move used it.

The supplied `PathList` is 0x18 bytes: head `+0`, tail `+8`, signed count `+0x10`, mutation/deferred-delete byte `+0x14`. Each node is 0x20 bytes: int32 province ID `+0`, previous pointer `+8`, next pointer `+0x10`, tombstone byte `+0x18`. Ordinary pathfinding produces next-province through destination, excluding current province. `Options+0x40` chain-length encoding is not a province path and must never be submitted.

`D50340` does the following:

1. At `D50425`, an army with a nonnull battle pointer `+0xC0` proceeds only if that object's vslot `+0x70` returns nonzero. That exception is not permission for this feature to command armies in battle.
2. It can detach the army from its siege context `+0xC8`, clear battle state, process native missions when argument 4 is nonzero, and change attached units. It has more effects than installing a list.
3. With replace nonzero, it calls `D50EC0`, clears the old path, and writes **`army+0x202 = 0` at `D50605`**. It resets dword `+0x130` if old/new head differ or either list is empty; equal first hops preserve that value. Do not treat `+0x130` as an eligibility boolean.
4. Starting from current province (replace) or the old tail (append), the loop at `D50680..D50796` finds an adjacency whose destination ID equals the next supplied ID. It allocates and copies a node only when that adjacency exists. Missing adjacency terminates the loop and leaves a prefix installed.

**This is an adjacency prefix, not a proven legal prefix.** The copy loop does not call ordinary path-policy military-access, blocked-strait or fort/history predicates. Supplying geographically adjacent IDs alone can bypass the rules that the native pathfinder normally established. The adapter must use a fresh successful ordinary legal route from the actual army origin, with its true path/history context and no transport edges; range BFS output is not an order path.

The function copies nodes using game allocation; it does not take ownership of input nodes. The original mission and ordinary builder free their temporary nodes after submission. The safe implementation is to hold the fresh native pathfinder output until submission completes, validate its entire bounded chain, exclude all original army nodes, then free only those proven temporary nodes with the already established game allocator/free pair. Never free installed army nodes. The current routes helper returns route DTOs and frees its temporary list; those DTOs alone do not preserve a native submission buffer. A writer must requery or introduce a private, same-turn query-and-submit lifecycle.

## 2. Stop: exact ABI, caller flags and effects

```cpp
// Windows x64: RCX, DL, R8B. No fourth argument. Return is void.
void cancel_movement(Army* army, uint8_t mission_reason,
                     uint8_t detach_context_98); // RVA D4FCB0
```

Prefix:

```
D4FCB0: 48 89 5c 24 10 55 56 57 41 54 41 55 41 56 41 57
```

The prologue copies R8B into R13D, DL into R12D and RCX into RSI. It immediately writes scheduling values at army `+0x8C/+0x90`, propagates to followers from `+0x58`, resets dword `+0x130`, writes **`+0x202 = 0` at `D4FDA4`**, calls `D50EC0`, frees old path nodes and sets `+0x100/+0x108/+0x110` to zero at `D4FE16..D4FE24`. There is no root-unit early guard for ownership, movement lock, retreat, battle, exile or session.

The second argument, if nonzero and the native mission's vslot `+0x70` permits it, is forwarded as the reason byte to `D3B8B0(army,1,reason,0,0)` at `D50156`. The third argument, if nonzero with army `+0x98` set, calls `1D3C90(context98,army,1)` at `D5017A`, clears `+0x98`, and updates another path/context list at `+0x118`. Its original type name remains unknown; `detach_context_98` describes the observed effect without guessing its enum.

`D50EC0` is now independently bounded and decompiled (`D50EC0..D50F28`): it calls `D76860(army+A0,army)` when that pointer exists, clears `+0xA0`, and releases/zeros the vector at `+0xA8/+0xB0/+0xB8`. Both Stop and replacement Move perform this cleanup even with reason zero. These associated-object semantics are not fully recovered.

Concrete callers and reasons:

- Native rebel mission: `cancel(army,0,0)` before a new automatic path when its existing path is nonempty. This is the narrowly supported candidate for cancelling this feature's own outstanding path while preserving the unrelated mission flags.
- Current-province map action: `D429E0` checks target equals current province, its queue condition, and **vslot+0x118 not locked** before constructing Stop command type `0x28B2`, vtable `1D1DA88`. At `D42D61`, it writes word `command+0x60 = 0x0101`.
- Stop command execute `D5E1A0`: resolves the typed army ID at command `+0x58`, writes cached army pointer `+0x50`, then at `D5E1C1/D5E1C6` loads R8B from command `+0x60`, DL from `+0x61`, and tail-jumps at `D5E1CF` to `D4FCB0`. Thus this particular UI action executes **`cancel(army,1,1)`**, not the original mission's `(0,0)`.
- Retreat/defeat processing `21EB30`: direct calls at `21ED6F` and `21EE3E` use `(army,1,0)` under their own special transition logic. Do not reuse that reason to implement ordinary target cancellation.
- Recursive follower cancellation: `D4FD23` passes `(child,sameReason,0)` only if the child's vslot `+0x118` says unlocked. The root did not receive that guard.

Stop command validator `D5E450` returns AL false if typed army resolution fails or `+0x203` is nonzero. For one virtual-type branch (`vslot+0x88` true), it also checks the first existing route province and `vslot+0x120` with the normal neutral tag `0x2D2D2D` if that province is land. It does **not** itself reproduce all the map-source lock gates and it writes the command's cached pointer. It is not a universal pure `canAcceptOrders(Army*)` function and must not be called with a fabricated partial command.

The Stop path may also start siege-related handling at `D4FF16` when standing in a hostile-controlled province without current battle/siege/retreat; it can affect followers and naval-related branches. Checking only the root's empty path does not prove no other side effects occurred. The automatic adapter should use known ordinary land armies with explicitly handled native mission/attachment contexts; unknown contexts must block writes until verified, rather than be silently cleared.

## 3. Required pre-call eligibility and freshness

These are adapter requirements for this feature, not a claim that the game has one equivalent native predicate:

| Fact | Verified source | Required treatment |
| --- | --- | --- |
| Thread/world/day | Confirmed UI/manager callback, world global `233FE78`, day `+1DD0`, daily updating `+1E10` | Same verified game thread, no active load/daily update, unchanged session/world/day from snapshot through query and submission. Honor pause policy; manager callbacks continue while paused. |
| Army identity | Land vtable `1C42540`, exact uint64 ID `+20` | Resolve fresh against complete roster and adapter generation. No stale pointer or JS number rounding. Same pointer alone is insufficient. |
| Ownership | Full country tag `army+D0`; active player `world+1E60`, or `+1E68` only when byte `+1E66==7 && +1E6F!=0` | Full tag/country validation and equality, not just low tag letters. Ownership changes invalidate pending commands. |
| Battle | Pointer `army+C0` | Must be null. Do not exploit submit's battle-vslot exception. |
| Retreat | Bytes `+203` and `+204` | Both zero. Neither alone proves defeat; real defeat observer separately disables task. |
| Movement lock | Byte `+202`; land vslot `+118 -> D4F2A0`, exact leaf `0F B6 81 02 02 00 00 C3` | Must be zero **before** either mutator; do not let their writes unlock movement. Defer owned cancellation until unlocked. |
| Exile | Native `099CA0` semantics and byte `+207` for normal validated player country | Do not issue combat/recovery automatic orders while exiled. Preserve native special-country handling if genericizing beyond player armies. |
| Current route context | Actual origin `+E8`, history pointers `+F0/+F8`, full list `+100`, battle/siege and flags | Must match query context immediately before call. Full route must end at current authorized destination and satisfy legal-route/no-transport contract. |
| Native contexts | Parent/attachment `+50`, followers `+58`, `+98`, `+A0/+A8`, mission `+1F8` | Their effects cannot be assumed absent from the mutators. Require established compatibility or explicitly block unsupported contexts; never patch fields to manufacture eligibility. |
| Manual/task ownership | Adapter session, command sequence, task revision and manual-command epoch | Every queued decision is revocable. Check again immediately before any native call. |
| Target validity | Complete fresh visible-target certificate tied to current session/world/player/day | Enemy still visible/hostile and in allowed range, or own province still hostile-controlled. Hidden last-known target data never suffices. Return-center moves use their independent rule. |

Fresh focused analysis also establishes `D42850(Army* RCX, Province* RDX) -> uint8 AL` as the UI helper's **reject-like** predicate: `D429E0` branches to illegal-order UI when it is nonzero. It checks nonempty regiment vector, retreat bytes, vslot `+0xB0`/byte `+0x205`/battle combinations, army `+0x50 == null`, and additional virtual-type, country, province visibility/discovery and access conditions. It contains calls with incompletely named semantics (`D59FA0`, `748FB0`, `3E44C0`, `3504A0`). Its result polarity and full two-argument ABI are now established; it is not independently runtime-tested as a standalone read-only predicate. Do not call it a complete universal eligibility check or use it to omit the explicit gates above. Evidence: new `work/native-order/pseudocode.txt`, function range `D42850..D429DF`.

## 4. Minimal submit-and-verify protocol

1. Bind the core decision to `{session_id, snapshot_id, ArmyKey, command.sequence, taskRevision, manualEpoch, world, player, day}`. Queueing it is not acceptance. Process manual takeover, defeat, load and ownership changes before consuming queued decisions.
2. On the verified game-thread frame after original manager work, resolve the army again, enforce the table above, and reject stale/missing/incomplete evidence. Do not perform native calls on RPC/background threads.
3. For Move, run an ordinary legal-route query from the real current origin. Keep its temporary list privately owned. Revalidate context, destination, task/manual epochs and session after the query; submit `D50340(army,path,1,0)` only if all remain valid. A zero-edge destination equal to current province requires no Move mutation and can be acknowledged as an already satisfied request after fresh checks.
4. Observe the entire installed army list synchronously after the native call, bounded by province count and checked for cycles, tombstones, valid previous/next pointers, exact count/head/tail and valid province IDs. Require **every installed ID in order** equals the requested route, the last equals destination, identity/session/day/origin/owner stayed valid, and no relevant epoch changed. Only then report core `Accepted` for that exact sequence. Full equality matters; destination or prefix alone is insufficient.
5. On early refusal, unexpected mutation or partial prefix, report `Rejected` only if the pending sequence still belongs to this session/task. Never reissue in a busy retry loop or report success from the void return. Treat partial installed paths as an anomalous native mutation: quarantine further automatic writes and retain diagnostic state. Do not blindly issue rollback Stop, because a later/manual command may now own the army. A separately ownership-verified compensating cancellation is possible only under the Stop conditions below.
6. Release only proven temporary input nodes after query/submission/verification. On uncertain ownership or unsafe traversal, quarantine the writer and retain the script instead of freeing arbitrary memory. Do not unload an active native call or callback.

This contract verifies actual installed movement, not just network enqueue. If the eventual implementation chooses native command creation/network dispatch instead of direct automatic-mission-style submission, dispatcher `14CA2D0` AL=1 still only means enqueue/send happened. It must separately correlate execution and the installed list before acknowledging acceptance. Multiplayer synchronization and permitted direct execution mode are not proven by this task.

## 5. Cancelled target and manual-command priority

An invalidated target should cancel only the path demonstrably owned by this controller. Keep an acceptance receipt containing session/ArmyKey/sequence, native path IDs, original origin, task revision and manual epoch. Track legitimate progress as the remaining suffix and new origin; do not infer ownership merely because current destination still equals the old target.

Before automatic Stop, require the receipt still belongs to the controller, the remaining path/origin correspond to that accepted route, no external command or unresolved mutation has superseded it, and all current eligibility gates hold. A hidden/dead/out-of-range/nonhostile target can make cancellation necessary, but cannot override movement lock, battle or retreat. If locked, keep a deferred cancellation request and revalidate ownership after unlocking; never call `D4FCB0` to force unlock. If the army is already idle, acknowledge the no-op without calling the mutator, after confirming the same session/command.

For a proven owned ordinary path, the native mission precedent is `D4FCB0(army,0,0)`. Afterward require unchanged identity/session/day/owner and empty head/tail/count. This is the candidate Stop acceptance check; a real staged invocation, including post-stop behavior on subsequent frames, remains necessary before enabling it. The bare clear of `+100` in memory is never an acceptable substitute for native Stop.

For confirmed human takeover, call `core.manual_command()` and revoke queued commands, pending acknowledgements, deferred Stop and cancellation receipts **without issuing Stop**. The existing reliable source is `116E3EF -> AD0BA0 -> AD0C2C -> D429E0`, matching the selected player's exact army, followed by matching Move dispatch `14CA2D0` AL=1. A later shared `D50340` call is not itself a human-source classifier.

Ordinary right-click is observed; dedicated Stop/hotkeys, queued Shift movement, modal mission-confirmation callbacks and all alternate command types do not yet have equivalent live coverage. There is also a gap between dispatch and eventual execution. Until full source coverage exists, unknown local dispatch or non-owned native path mutation should conservatively revoke cancellation ownership/suspend this army's writer, not be classified as definitely manual or ignored. Comparing path contents cannot detect a manual command that happens to choose the same path. A dispatcher epoch or accepted-command source observation must cover that case before any subsequent automatic cancellation is safe.

## 6. Evidence and remaining acceptance work

Existing `work/native-path/pseudocode1.txt` contains full assembly/decompile of `D4FCB0` and `D50340`; `work/native-manual/pseudocode4.txt` has Stop execute/validator; `work/native-manual/pseudocode3.txt` has the UI lock/source/constructor chain; `work/native-hostility/pseudocode2.txt` has `D3B8B0`; `docs/native-scheduler-findings.txt` ties battle/lock/retreat fields to UI strings; `docs/native-targets-findings.md` describes exile and visibility proof.

Previously recorded `path-trace-01.jsonl` observed one manual Move with root flags `(1,0x18)` and attached recursion `(1,0)`, 35 input and installed nodes, with only the first 32 node IDs logged. It supports copy semantics but does not prove full-tail equality or automatic `(1,0)` behavior. `route-query-01.jsonl` proved native query and temporary-node freeing, without automatic submission. This task added no runtime evidence.

Remaining staged acceptance cases are: automatic `(1,0)` full route install and preservation across frames; native `(0,0)` Stop with no manual supersession; Stop deferred through lock; battle/retreat/exile calls prevented; disappeared target cancellation; manual takeover before submit, before Stop and during queued dispatch; path-prefix failure without unsafe retry; attached/native-mission context handling; and exact multiplayer/session restrictions. These are release gates, not reasons to guess additional fields or widen a low-level ABI.
