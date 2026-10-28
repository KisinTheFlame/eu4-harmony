# Currently visible enemy armies and own provinces under hostile occupation

2026-10-26, approved baseline SHA256 `97dd63a811487fbe2c917f2c15b1e68f7222bef74a833889cf943c8010049d5e`. All addresses are RVAs. This subtask read only the frozen exe and existing static and runtime records, and did not attach to the game; the helper has not yet passed in-game acceptance.

`tools/native_targets.js` defines a standalone `createTargetReader(base)`. It does not hook, offers no RPC, issues no orders, changes neither saves nor the map, and does not scan units inside the fog. It is separate from the geography reader helper; the caller is responsible for combining the data under the same stable world and army identity, and for obtaining native routes and final permission to issue orders.

## Calling and the FOW freshness contract

```js
const reader = createTargetReader(base);
const result = reader.read({
  army,                       // pointer to a currently valid main land army
  uiThreadId,                 // confirmed native game/UI thread ID
  sessionId: 'campaign/load-1',
  fow: currentCertificate,    // copy of the certificate at the start of this read
  getFowState() { return currentCertificate; }
});
```

The FOW certificate has the fields below. If any is missing or does not match, the result is `targets_complete:false` with empty provinces/enemies, and enumeration of armies never starts.

```js
{
  source: 'native-0x8275e0-onLeave',
  confirmed: true,
  inProgress: false,
  fresh: true,
  epoch: 1,                   // sequence number of the completed native refresh, a positive safe integer
  revision: 0,                // tracker invalidation/change sequence number, a non-negative safe integer
  sessionId: 'campaign/load-1',
  world: '0x...',             // matches the current NativePointer.toString()
  player: '0x10000b900534143', // full valid player Tag, lower-case hex
  day: 56679720              // raw int32 value of the current world+0x1DD0
}
```

This is the caller's statement of what it really observed; the helper does not fake "FOW hook installed". The provider must invalidate the certificate and increase revision on load/unload/country switch, at the start of a refresh, on a FOW setter not covered by the evidence of a completed refresh, and on any other relevant visibility change. After the day advances, the old certificate cannot be kept without a refresh completed on that same day. sessionId must change when a save is loaded, even if the date and world address repeat. Only a result closed by a real normal completion event of `0x8275E0`, the correct world/tag/day and a stable point may set fresh again.

At the start the helper checks both `input.fow` and `getFowState()`; at the end it reads the live provider once more and compares it with the full certificate it copied, rather than keeping a reference to the caller's mutable object as the old value. Saving one historical epoch without tracking later invalidation does not meet the contract. The observations of the earlier `native_fow_observer.js` can provide part of the evidence, but it lacks the day/session/revision/fresh contract needed here, so its old lastCompleted object cannot simply be labelled a full certificate.

Both entry and exit require that the actual thread matches and `world.byte(+0x1E10)==0`; being on the right thread does not by itself rule out nested manager frames during the daily update. The whole read also checks that the world, player, raw date, country array, map vector, army ID/owner/location and the unit list of each visible province stay stable. All objects and the readable-memory range cache live only within one synchronous read.

## Visibility before unit enumeration

This uses the cache already verified statically and by passive runtime observation:

```cpp
hidden = (province.byte(+0x959) & 0x20) &&
         (province.byte(+0x958) & 0x02);
```

Only currently eligible, visible land provinces are enumerated. hidden must be computed first, and only once it is false is the unit list head at `province+0x350` read. For a hidden province, the list head, nodes, army locations, army IDs, countries, troop strength and any other army fields are never read; there is no "last seen location" cache, and the number of hidden units is not exported. The ordinary map owner field can be used to pick out our own territory, but the controller of a hidden province is not read. If the certificate claims to be fresh while one of our own provinces is still hidden, the whole target result is rejected, rather than passing off false as "confirmed not occupied".

List nodes: payload `+0`, next `+0x10`. Each visible unit must still have the current province as `army+0xE8`; the list is bounded and loops/duplicates are detected. Only the main land-army vtable `0x1C42540` is handled as a land army; the known fleet vtable `0x1CCA7E0` is skipped, and any other visible unit type rejects the current result rather than being guessed at. Only land armies in land provinces are kept here; units on transports or in sea zones are not land combat targets.

If the FOW bits, identities, lists or certificate have changed at the final recheck, the whole result is discarded; a failure return contains none of the enemy armies or target provinces collected, nor partial counts of them. `status()` likewise keeps only the failure reason/phase and does not store the identities of the last enemy armies.

## Legal hostility of enemy armies and state filtering

The only native call is the recovered relation predicate:

```cpp
uint8_t (__fastcall *HostileArmy)(Army* own, Army* candidate); // RVA 0xD54A60
```

It takes two pointers in Win64 RCX/RDX and returns a boolean in AL. Keep the native argument order. The function covers the ordinary war table and the exceptions for special countries/rebels, related countries and leased military identities. A different owner alone does not make an army an enemy. The 16 entry bytes are:

```text
48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80
```

It may initialize native thread TLS, but it issues no move and changes no task. See `docs/native-hostility-findings.txt` and `0xD54A60` in `work/native-integration/pseudocode1.txt`. The full exe hash is a precondition; the entry prefix is only an aid to verification.

A candidate must also have a non-empty regiment pointer vector (`army+0x30 .. +0x38`, stride 8), must not be in a retreat state with `+0x203` or `+0x204` non-zero, and must not meet the native exile test. "Non-empty" is used here only to tell that the object is still an army with troops; troop numbers and strength are not compared at all.

The exile test comes from `0x99CA0`, recovered by targeted analysis this time: it first confirms land army vslot+0x80; with a valid owner Tag whose kind is not 7 and whose country index is none of the three special indices, it returns `army.byte(+0x207)`; otherwise it returns AL=0. On true, the caller `0x1A2280` takes a separate access branch, and the string at RVA `0x1C41D98`, read directly from the frozen exe, is `EXILED_ACCESS`, which closes the meaning. For a confirmed main land army the helper applies the same field logic and does not need to call another native function.

Raw evidence: `work/native-targets/pseudocode1.txt` (`0x99CA0..0x99D07`). `0xD40AE0`, analyzed at the same time, is a query of army/country modifier flags and touches more objects; it is not used as an exile or hostility predicate. For the evidence on the two retreat bytes, see `docs/native-scheduler-findings.txt`.

An ordinary player must be a valid controllable Tag with index>0 and kind!=7; enemy armies and controlling countries do not inherit this player restriction. For them a non-negative signed index, including 0, is allowed, and the current country array slot is checked against the full Tag at country+0x20. Special kinds such as kind 1 are not excluded. The three special country indices are read from the current globals `0x233D8A4 / 0x233D8D8 / 0x233D8DC` and recorded in the successful result as `evidence.specialCountryIndices`; their values are not hard-coded, and the other two special countries are not given names on a guess.

When the controlled army is retreating or exiled, `attacker_eligible:false` and no attackable enemy armies are output. Full army state, the course of a battle and path/zone-of-control limits still belong to the outer snapshot and native order validation; `legally_attackable:true` here does not mean that any path or move order will succeed.

## Hostile occupation of our own provinces only

Ownership and control are `province+0x468` / `province+0x470`, confirmed by the native save tokens. The owner's full Tag must equal the player's; provinces owned by allies, subjects or other countries are never marked as recapture targets.

Pure country-to-country hostility uses the predicate inlined in the native movement arrival logic, without running the arrival function and its side effects:

1. Resolve the current controller country from the province controller's valid Tag.
2. If the controller is the player, return false.
3. Look up the hostility array at controller country `+0x1548`, count=`int32(+0x1554)`, stride 16, the first int32 being the other country's index; if it contains the player's index, return true.
4. If the controlling country's or the player's index equals any of the native special country indices above, also true; otherwise false.

Exact basis: `work/native-path/pseudocode1.txt`, machine code `0xD4FE76..0xD4FED9` and its decompilation. It first resolves the controlling country from `+0x470` of the province the army is in, makes the comparison above with the army's owner Tag, and only then checks retreat/battle state and calls `0x247C80`. The helper only reads this relation data and never calls the movement arrival or siege functions. `0xD45250` in `work/native-path/pseudocode3.txt` has a separate, identical inlined comparison for the controlling country's relations; its function as a whole, which involves forts and paths, is not called.

So control by a foreign country does not automatically mean hostile occupation: ordinary neutral or friendly control with no hostility record returns false. If the hostility array's count/span/indices are abnormal, an owned province's controller is not legal, or a country object's Tag does not match, the whole result fails closed. The array may hold duplicate relation records, because the native code only checks for existence and assumes no uniqueness.

## Output and the limits of validation

A successful output is tied to sessionId/world/player/day/armyId/origin and contains ordinary province DTOs `id/owned_by_player/occupied_by_hostile` and visible enemy army DTOs `key{id,generation:0}/province/visible/legally_attackable`. generation=0 is a placeholder, because no separate persistent generation has been recovered for the current baseline; the outer layer must still use session/lifecycle to guard against ID reuse, and must not take this as a generation read from the game.

On success, `targets_complete:true` only means that this read of target candidates finished. It is always `orders_permitted:false`: there are no routes, geographic edges, pause/manual takeover/defeat scheduling or full order permission. It must not be wrapped directly into a complete `WorldSnapshot` that can issue orders. Limits: 50,000 provinces in total, 100,000 visible nodes, a relation count of 32,767, and 5 seconds for the synchronous read; any partial result is rejected. The actual cost is recorded in the returned count metrics.

Verification: `node --check tools/native_targets.js` passes; the 13 synthetic tests in `node tests/test_native_targets.cjs` pass. They cover: assertions that forbid access to the list before FOW is checked; separating enemy armies at war and rebels from friendly armies; excluding retreating, exiled and empty-regiment armies and fleets; separating our provinces occupied in war or by special countries from neutral control; FOW certificates that are missing, stale, from before a load, or changed during the read; rejection when the daily update is running at the start or begins during the read; failing closed on list loops, unknown vtables, bad relation tables and the wrong thread; full Tag verification for kind1/index0 targets. This helper has not been run, and no acceptance is claimed for special armies, all maps or real orders.

## Runnable passive evidence and the manager queue probe

The host appends `tools/native_targets_probe.js` to `native_targets.js`; the host verifies the approved hash and picks the isolated `/work/game-stage/eu4.exe`. The script itself does not attach to a process. It checks for a unique staged module, the x64 architecture and all entry prefixes; if the host provides `PROBE_CONFIG.path`, it also requires it to match the module's actual full path. There is no compatibility fallback for an unknown version or address.

The entries actually observed, with their first 16 bytes checked again against the frozen file:

```text
manager 0x8137C0: 48 8b c4 88 50 10 55 53 56 57 41 54 41 55 41 56
FOW     0x8275E0: 48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57
daily   0x758430: 48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48
load    0x5CDC60: 48 89 5c 24 18 55 56 57 48 8b ec 48 81 ec 80 00
setter  0xA0B150: 48 89 5c 24 10 57 48 83 ec 20 4c 8b 41 28 0f b6
```

Only `0x8137C0` is replaced, with ABI `void(manager*, uint8_t)`. It first forwards to the original function exactly, then checks that the actual world+0x1E00 equals the manager argument, the manager vtable is `0x1CB8460`, dailyUpdating=0, loadDepth=0, the native thread is the right one and unchanged, and this is the outermost manager callback. Only then does it handle one task from the RPC queue. It does not depend on the army panel and neither listens to nor replaces panelupdate. A replacement is used rather than the manager's onLeave because the latter's Interceptor listener guard suppresses nested native observation; the real FOW/daily/load callbacks must stay observable here.

FOW onEnter invalidates the old certificate; a normal onLeave establishes a real epoch only when world/player/day/FOW mode, session and revision are all the same before and after. It never calls a refresh itself, and does not set fresh to true just because it read the cache. If completion happens while dailyUpdating=1, the certificate records the real refresh, but the read must still wait for a stable manager frame and for the helper to confirm dailyUpdating=0 again; a later change to the raw day invalidates the certificate. A FOW refresh during loading issues no valid certificate; after load returns, it still waits for a new natural FOW completion.

world-daily onEnter invalidates at once; load onEnter rotates the session and cancels old tasks, and its onLeave keeps waiting for a new refresh. A setter outside a full FOW refresh invalidates at once; a setter on another thread during a refresh cannot pass as part of the current refresh either. Parallel refreshes on different threads cause the current certificate to be rejected. A change of the current player/world rotates the session; finding at read time that the global FOW switch, the date or an identity differs also invalidates. The certificate's thread must be the same as the confirmed manager thread.

RPC:

```js
await script.exports.snapshot({
  army: '0x...',      // the host gets this from a known source such as the current native selection
  armyId: '0x19d300000036',
  world: '0x...'
});
script.exports.status();        // FOW, current world, thread, session, queue and cleanup state
script.exports.cancelpending(); // only cancels queued requests; does not abort a synchronous native query already started
await script.exports.dispose();
```

snapshot references must be non-zero 64-bit hex strings; the RPC thread does not call the target helper. The queue holds at most 1 item, and a request is rejected if no stable manager frame comes within 10 seconds. When it actually runs, it compares the caller's world, the current session, the army vtable and the full army ID again, then calls the helper on the native thread. Without fresh evidence it returns the helper's `targets_complete:false` and does not fake a target list. The current-world readings in `status()` are diagnostics and cannot replace the stable-point check of a queued snapshot.

The script calls `Script.pin()` before installing. dispose first stops accepting requests, invalidates the certificate and cancels the queue, then waits for the native call depth of the manager and all observation hooks to reach zero. Only after leaving the native callbacks does it revert the manager replacement, detach all hooks and flush; finally it unpins and returns `stopped:true`. No panel needs to be open; even with no further game frames, cleanup can proceed as long as no native call is active. If cleanup fails, the pin must be kept, an error returned and the host allowed to retry explicitly; an unload timeout must not be used to pretend cleanup finished.

Verification: the probe passes node's syntax check; the synthetic lifecycle scenarios in `node tests/test_native_targets_probe.cjs` pass. They include: the original function running before the helper; nested manager frames handling the queue only once; a natural FOW refresh issuing a certificate; invalidation by standalone setters, setters on other threads and FOW mode changes; invalidation by daily/load; rejection of a refresh during load; rejection of mismatched references; waiting for a safe exit when dispose is requested inside an active hook or manager; and cleanup that can be retried after a failed detach, with the pin kept. What is tested here is the probe's scheduling and lifecycle; the actual reading of enemy armies is covered by the helper's tests. The probe has not yet run in the game; runtime results are recorded separately by the main task.
