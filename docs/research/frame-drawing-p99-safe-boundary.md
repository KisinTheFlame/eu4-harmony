# Bounded normal-return scopes across the stream formatting path

This investigation follows the successive stream-related acquisitions found by the [release-path diagnosis](frame-drawing-p99-release-path.md), including their normal exits, nested guards, cold paths and surrounding virtual calls. It supports a conservative experiment in the drawing thread's lock-order ownership: release only at independently checked normal-return boundaries, retaining the old ownership lifetime whenever a cold facet-construction path runs. It does not justify releasing at every game-lock exit or at arbitrary stream callbacks.

Status: static boundary investigation, complete four-scope mechanism recheck and final A/B/B/A timing completed on 2026-11-14. The prototype repeated a roughly 70% P99 reduction in this scene. The daily-rate cost is not isolated, and broader safety remains unverified. The checked behavior is now implemented in `core/src/drawing/lock_order.rs`; installation remains separate.

## Sources and coverage

The source is the installed EU4 1.37.5 PE image, SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`. Addresses below are RVAs. `cargo xtask what` was used before reading each function. Existing native instruction evidence is in `target/scenarios/frame-p99-attribution/evidence/stream-lock.asm`; the new exact-byte disassembly is in `target/scenarios/frame-p99-boundary/evidence/`:

- `second-scope.asm`: the whole PE exception-directory bodies of the second helper and enclosing initializer, with their unwind records.
- `direct-callees.asm`: the cold constructor, registration helper, failure helper, stream-state setter and optional stream registration.
- `facet-callees.asm`: locale save/set/restore, facet data capture, locale implementation construction, exception construction and the native exception-handler entry.
- `virtual-methods.asm`: built-in retain/release, character conversion and deleting destructors.
- `lock-and-getter.asm`: the complete first getter and numbered guard enter/leave paths, reread from the same image.
- `inspect.py`: the read-only extractor, using installed pefile and Capstone. Function bounds come from the PE exception directory. Leaf candidates without an entry are decoded through their first return; the output distinguishes them. It never executes the image.

The entire first and second helper bodies and the enclosing initializer were read. The cold constructor and immediate locale-related callees were read, but the CRT's allocation, locale management and C++ exception-dispatch transitive implementation were not exhausted. That is why the prototype must explicitly exclude the cold path and abnormal completion, rather than treating all of the CRT as proven safe. No REA analysis target was opened or left behind for this investigation.

## The second helper's actual work

Function `0x91a70` spans `[0x91a70, 0x91b79)`. It reads one argument, RCX: a locale-like object whose `+8` points to an implementation with a facet pointer array at `+0x10`, its count at `+0x18`, and a fallback flag at `+0x24`. Calling this a locale facet lookup is an inference from the data flow and surrounding character conversion, not a recovered symbol.

It constructs numbered guard zero at `0x91a86`, through `0x1a0c750`, and keeps that guard at `[rsp+0x48]`. Guard zero enters the critical section at `0x233cdd0` through `0x1a5f504`; its destructor `0x1a0c7c8` leaves the same section through `0x1a5f520` and `LeaveCriticalSection`. This is the lock already identified in the runtime traces.

Under this guard, the helper:

1. Reads cached fallback facet `0x233d878` and facet index `0x233b2a0`.
2. If the index is zero, constructs another guard zero at `[rsp+0x40]`, rechecks the index, increments global counter `0x233b288` and stores the assigned index. It destroys this inner guard at `0x91ad5` before returning to lookup. The outer guard remains held, so this is a recursive acquisition of the same critical section, not a new lock spanning the eventual return.
3. Tries the facet slot in the supplied implementation. If absent and its fallback flag is set, `0x1a0c400` reads the global implementation pointer `0x233b2e0` and lookup tries its slot. That helper is just a global-pointer load and return.
4. Uses the cached fallback if present. Otherwise it calls cold helper `0x918e0`, may throw through `0x923b0`, registers the constructed object through `0x1a0c3c8`, invokes its `+8` virtual method and publishes it in `0x233d878`.
5. Destroys the outer guard at `0x91b5e`, then returns the facet pointer in RAX at the sole normal `ret`, `0x91b72`.

Every normal branch joins at `0x91b59`, before the outer destructor. The only branch away from that common exit is the explicit failure call at `0x91b73`. No game lock acquired directly by the lookup remains held at normal return. Index initialization has already destroyed its recursive guard, and the cold constructor's own guard is excluded by the rule below.

## Exclude the cold path by execution, not by an unlocked cache read

The recommended second scope is entry/normal return of `0x91a70`. A before-observer on `0x918e0` revokes early-release eligibility for any enclosing temporary ownership and promotes it to the original frame-check lifetime. Its marker should distinguish cold-path promotion from a genuine graphics crossing: a cold call alone is not evidence that the prepared frame is invalid.

All of the second helper's unproven cold work goes through that one call at `0x91b2c`. The only indirect call inside `0x91a70`, at `0x91b4f`, is reachable only after `0x918e0` returns. Registration and publication likewise occur only there. Every path that avoids `0x918e0` consists of the guards, index assignment, pointer/array reads and common guard destruction; it contains no callback or virtual call.

An entry-time test that `0x233d878` or `0x233b2a0` is nonzero is not the proposed safety mechanism: it precedes the game's internal lock and could become stale without a separately established lifetime proof. Observing the actual cold-path entry avoids that assumption, adds no new unguarded game-field reads, and still lets ordinary warmed lookup calls release at their proven normal boundary. The index-zero path can remain eligible because both recursive guard acquisitions and releases are visible in the complete body.

The cold helper `0x918e0` allocates a facet, constructs another guard zero, saves and changes locale state through `0x1a0c57c`, captures character-category data through `0x1a0c910`, restores locale state through `0x1a0c5e8`, frees its temporary strings, and destroys its guard at `0x91a3c`. The saved pointer output is then registered and published by the caller. These observations explain why the cold path is not merely a pointer lookup; the proposed experiment does not need to assert complete safety of its transitive CRT work.

## Returned objects and the enclosing initializer

Initializer `0x1a14f0` spans `[0x1a14f0, 0x1a1648)` and reads RCX, RDX and R8b. It initializes its destination stream's fields, obtains the shared locale implementation through first getter `0x1a0c408`, stores that implementation in an allocated wrapper, and retains it again before constructing a stack-local locale argument to the second helper. The first and second calls return at `0x1a1558` and `0x1a1585` respectively.

The implementation built by `0x1a0c2e4` uses virtual table `0x1c1f070`; the fallback facet built by `0x918e0` uses `0x1c1f1d8`. In both built-in tables, `+8` is `0x92500`, a complete leaf consisting of `lock inc dword ptr [rcx+8]` and `ret`. Slot `+0x10` is `0x92510`, an atomic decrement returning the object only when the count becomes zero. These are actual table contents and instructions, not an assumption from a library name.

After the second helper returns, the initializer calls returned-facet slot `+0x40` at `0x1a1591`. In the built-in fallback table this is `0x918b0`, whose whole body returns the incoming byte in DL. It then releases the temporary implementation reference and invokes a deleting destructor only if the decrement returned a non-null object. The stream wrapper still owns the reference obtained from the first getter; the temporary reference is a separate retain/release pair. The built-in fallback also acquired its own retained reference before global publication. Thus normal return from the helper does not hand out a stack object or depend on continuing to hold its guard for the built-in object's lifetime.

This does not prove every possible facet vtable or every caller's object-lifetime discipline. The helper's existing native design already calls its guard destructor before handing the facet to its caller. The prototype changes only the framework's extra presentation ownership at that boundary; it does not drop the game's critical section sooner, alter reference counts, or hold a facet across additional game work. Calls after the observer returns remain under normal lock-order and crossing interception.

Do not extend the scope to the whole initializer on this evidence. The initializer includes subsequent virtual calls, possible deletion, stream error construction and an optional registration call `0x1a0cc48`. That last function takes numbered lock two and updates the global stream registry, independently of the observed lock-zero pair. The broader function has a much larger proof obligation than the lookup's already-balanced guard lifetime.

## Exceptions and framework ownership

The second helper has a native unwind record and C++ handler `0x1a34e3c`; its explicit failure helper `0x923b0` constructs an exception and calls the game's throw routine `0x1a36038`. The cold constructor has its own unwind record and throwing error branch. The compressed C++ unwind maps and all CRT exceptional cleanup were not fully decoded, and no exception was injected. None of that is used to justify early release on an abnormal exit.

The existing observer implementation in `core/native.cpp:55` invokes the after callback from `__finally` and sets the frame's fault marker when `AbnormalTermination()` is true. `Call::faulted()` in `core/src/hooks.rs` exposes that distinction. Therefore after callbacks must not be treated as normal returns unconditionally. An abnormal exit revokes temporary release eligibility and retains the original ownership lifetime, even if native unwinding probably already released the internal guard.

The second scope must preserve the first prototype's other conditions:

- Only main-thread daily work can acquire eligible temporary ownership. Entry with existing frame or crossing ownership never becomes eligible just because it entered a scope.
- Release only after the outermost observed scope normally completes, with eligibility still intact. A nested return must not release an outer native guard.
- Any cold-path marker, abnormal exit, actual crossing or frame-ownership transition permanently revokes eligibility for that ownership span.
- Graphics-table and scene crossing detection must not skip a genuine crossing merely because a temporary order-only scope holds the presentation lock. Such crossings retain the ordinary protection and frame invalidation behavior.
- Every later game-lock acquisition still passes the original lock-order gate. There is no blanket suppression of interception after an early release.

## Concrete observer and byte-check inputs

| Purpose | Entry RVA | Arguments read | Entry bytes |
| --- | --- | ---: | --- |
| Second normal-return scope | `0x91a70` | 1 | `48 89 5c 24 20 55 56 57 48 83 ec 20` |
| Cold-path promotion, before only | `0x918e0` | 2 | `48 89 5c 24 10 48 89 74 24 18 55 57 41 56` |

The normal-boundary proof additionally relies on the common exit at `0x91b59`: `48 8d 4c 24 48 e8 65 ac 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3`. The cold transition at `0x91b24` begins `48 8b d5 48 8d 4c 24 50 e8 af fd ff ff`; the nested release at `0x91ad0` begins `48 8d 4c 24 40 e8 ee ac 97 01`. Preserve checked dependency points for these native assumptions, together with the existing guard destructor/leave path, rather than relying only on function entry bytes. The complete helper bytes in `second-scope.asm` can be used when checking its whole small control-flow body is preferable to selecting partial spans.

No new cached-field offsets are needed by the recommended observer design. The native object offsets above describe the evidence; they are not proposed direct framework reads. The two short critical sections remain separate scopes, rather than keeping presentation ownership across the intervening reference-count operations.

The static result supports trying this conservative second scope and measuring actual release, cold-path promotion, subsequent acquisitions and successful submission intervals. It establishes no percentage gain, all-call-path safety, exception-injection result or late-game performance result by itself.

## Third acquisition: a second facet lookup at 0x91670

The next diagnostic identified native return address `0x9168b` as a later lock-order acquisition after the second helper's scope released. This section reads that helper independently; it does not generalize the two concrete helpers into a rule for all numbered guards. `cargo xtask what 0x91670` first located the earlier first-use-global observation in [loading.md](loading.md). New source evidence is `third-scope.asm`, `third-cold.asm` and `third-identity-and-spans.json` in the same local evidence directory. The JSON retains **all bytes** of both complete function bodies as well as the selected check spans and built-in virtual-table targets.

Function `0x91670` spans `[0x91670, 0x91779)` and reads one argument in RCX. Its control flow is the same pattern as the second lookup, but its fallback cache is `0x233d870` and its facet index is `0x2459ac8`. These globals are not substituted for the second helper's globals, and no prototype read of them is needed.

- It constructs outer guard zero at `0x91686`, using `[rsp+0x48]` as the guard object.
- If its index is uninitialized, it constructs recursive guard zero at `0x916ab`, rechecks the index, assigns the next value from shared counter `0x233b288`, and destroys the inner guard at `0x916d5`.
- It tries the supplied implementation's facet array, optionally the global implementation through already-read leaf `0x1a0c400`, and then its cached fallback. Every successful lookup or cached-fallback branch joins at `0x91759`.
- Missing all three sources invokes cold helper `0x914f0` at `0x9172c`. Only after that call can the function register through `0x1a0c3c8`, invoke the facet's `+8` virtual method at `0x9174f`, and publish `0x233d870`.
- The common exit destroys the outer guard at `0x9175e`, returns the facet in RAX, and executes the sole normal return at `0x91772`. The explicit failure path at `0x91773` calls already-read throwing helper `0x923b0` instead.

Thus the third helper supports the same conservative independent scope: observe its entry and normal return, and observe **its own** cold entry `0x914f0` before execution to revoke temporary early-release eligibility. Watching only `0x918e0` would miss this distinct factory. There is no indirect call on any third-helper path that avoids `0x914f0`, and all direct native guards on those paths have been destroyed before the normal return. The recursive index-initialization guard is fully balanced; it does not require an unguarded index precheck or a separate exclusion. Abnormal return, existing ownership, real crossings and nested-scope rules remain exactly as above.

The complete cold helper `[0x914f0, 0x91664)` was also read. It reads RCX as the output-pointer address and RDX as the locale-like argument. It allocates 16 bytes, takes guard zero, saves/sets locale through `0x1a0c57c`, initializes a reference-count field and table pointer, writes the output, restores locale through `0x1a0c5e8`, frees its temporary strings and destroys its guard at `0x91634`. Its common normal return is `0x91656`; an invalid locale-name path invokes already-read throw helper `0x1a0c214`. It installs table `0x1c1f678`, whose actual `+8` and `+0x10` slots are the same atomic retain/release leaves `0x92500` and `0x92510`. The cold path contains no additional indirect call itself; the caller's retain is the only indirect call inside the third lookup. Transitive allocator, locale manipulation and exception cleanup are still excluded rather than assumed safe.

The third lookup's PE unwind metadata is exactly the same record `0x1dca2c0` used by the second lookup. This is corroborating compiler metadata, not proof that an after callback is a normal return. The cold helper has its own record. The framework must still check `Call::faulted()` and retain the original lifetime on an abnormal exit; no new claim of complete C++ unwind-map decoding is made.

As with the second helper, the native function already leaves its game critical section before returning its facet pointer. Scoped presentation release does not change that native lifetime boundary or its reference counts. It does not prove the semantics of every later virtual method reached by the caller, so later calls must retain normal crossing and lock-order interception. No blanket permission to release at all guard destructors or around a larger stream operation follows from this body.

| Third-helper dependency | RVA | Arguments / check bytes |
| --- | --- | --- |
| Scope entry | `0x91670` | 1 argument; `48 89 5c 24 20 55 56 57 48 83 ec 20` |
| Cold promotion entry | `0x914f0` | 2 arguments; `48 89 5c 24 10 48 89 74 24 18 55 57 41 56` |
| Outer release and sole normal return | `0x91759` | `48 8d 4c 24 48 e8 65 b0 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3` |
| Only cold transition | `0x91724` | `48 8b d5 48 8d 4c 24 50 e8 bf fd ff ff` |
| Recursive guard destruction | `0x916d0` | `48 8d 4c 24 40 e8 ee b0 97 01` |

This is a static eligibility argument for a third explicitly checked scope, not a measured benefit or an expansion to all facet lookup implementations. Its actual release counts, cold promotions and remaining long acquisitions belong to the diagnostic experiment.

## First diagnostic pair: the second boundary works, another acquisition remains

The [baseline](../prototype-log/2026-11-14-frame-p99-boundary-diag-a.md) and [two-scope prototype](../prototype-log/2026-11-14-frame-p99-boundary-diag-b.md) used the same diagnostic DLL, selected at process start, in hidden new Ming games with seed 123456 and a fixed Beijing view. Both had ten seconds paused and 120 seconds at speed 5, with identical settings and no other game in the five-second process-presence samples. The exact loaded identity, settings, view approval and raw events are under `target/scenarios/frame-p99-boundary/diag-{a,b}/` and `traces/`.

Baseline running P99 was 35.686 ms at 614.5 completed days/min; the two-scope prototype was 38.587 ms at 606.5 days/min. Paused controls were 7.975 and 9.409 ms. These are matched-instrumentation diagnostics, not timing-build optimization gains. All four windows were complete with no failed submissions or dropped events, and raw replay matched formal counts and P99. Both games released with exit 1 and no crash.

The prototype observed 3,898 eligible first-getter acquisitions and 2,701 eligible second-helper acquisitions. All 6,599 released early. First-getter hold median/P99 was 4/23 microseconds; second-helper hold median/P99 was 2/12 microseconds. No cold-path, promotion or abnormal-return marker occurred in the collected windows; cold and exception retention are supported by static evidence and state tests, not an injected native exception or a captured cold call here. No scope pair was unfinished or unmatched.

On the originally identified AI chain, 888 consecutive first/second pairs reduced the second hold to median 3 microseconds, P99 14 microseconds, maximum 34 microseconds. The new boundary therefore works. It does not remove all later acquisitions. In the later formatting call on that same AI-country path, 888 second/third pairs had exactly one short getter acquisition between them, and a median 5 microseconds between second release and the third acquisition. The third hold then lasted median 18.584 ms, P99 41.674 ms, maximum 52.430 ms. Its native return chain includes `0x9168b` inside the newly read third helper. These pairs are retained in `diag-b/third-acquisition.json`; they are not falsely described as three immediately adjacent acquisitions.

Lock waits still accounted for 61.38% of the prototype's P99 neighborhood, invalidation outside those waits 24.68%, map names due 6.97%, pre-Present drawing 4.99% and Present 1.96%. No marked local save overlapped that band. The result justified reading the third helper before spending four timing games on a candidate with a known remaining long hold. The unmeasured two-scope timing build passed its ownership tests, both clippy configurations and release build; its identity and source patch are retained separately. It is not substituted for a tested performance result.

## Three-scope mechanism recheck

The third diagnostic game (`diag-c`, PID 100460) used the same view, seed and settings with all three scopes active. Its paused/running P99 was 8.733/42.376 ms and its running completed-day rate was 547.5/min. It is a mechanism recheck, not a matched speed comparison with the two-scope baseline. The two windows completed without lost events, failed submissions, unfinished scope pairs or another game in the five-second process samples.

The getter, second lookup and third lookup released early 3,526, 3,277 and 822 times respectively. Their hold median/P99 values were 4/25, 2/11 and 2/8 microseconds. No cold, promotion or fault marker appeared. All 7,625 paired eligible acquisitions released early, so the third boundary itself worked.

The same AI formatting chain then acquired guard zero through return address `0x9240b`. Its 806 holds totaled 9.124700 seconds, overlapping the P99 neighborhood by 2.518614 seconds. The 806 third-to-fourth pairs had one intervening short acquisition; the fourth hold's median/P99/max was 6.408/46.599/57.338 ms, with a median 5 microseconds from third release to fourth acquisition. Exact pairs are in `diag-c/fourth-acquisition.json`.

Lock waits still accounted for 63.68% of this run's P99 neighborhood, invalidation outside waits 21.48%, map names due 5.34%, pre-Present drawing 5.70% and Present 3.79%. No marked local save overlapped the band. The three-scope timing candidate was archived without running a timing game; its final validation was intentionally stopped when this remaining long hold was identified. The interrupted log is not a completed validation result.

## Fourth acquisition and the rest of the captured formatting path

The next captured lock-zero return address, `0x9240b`, belongs to `[0x923f0, 0x924f9)`. Its enclosing return chain is `0x929a9` in `0x92890`, `0x9310f` in `0x93010`, and `0x93c10` in `0x93af0`, then `0x1563967` in the logging formatter `0x1563650`. This section reads those complete callers and resolves the built-in facet and buffer methods they use, to avoid discovering another already-visible lookup only after another game run. It does not extend the scope to all formatting or all numbered locks.

New evidence in `target/scenarios/frame-p99-boundary/evidence/` includes:

- `formatting-chain.asm`: the complete fourth lookup and the three immediate formatting callers.
- `formatting-callees.asm` and `formatting-tail.asm`: the complete fourth cold factory, direct formatting helpers, logging formatter and initial fragments of string/output helpers.
- `formatting-vtables.json` and `streambuf-vtable.json`: the built-in table entries used to resolve indirect calls.
- `stream-buffer-lifetime.asm` and `streambuf-cleanup.asm`: the actual stack stream construction, string insertion, buffer extraction and cleanup.
- `resolved-streambuf-methods.asm`, `resolved-streambuf-tail.asm`, `resolved-streambuf-end.asm` and `streambuf-deallocation.asm`: the actual buffer methods and all their remaining normal body fragments, plus string-copy continuation and error fragments.
- `fourth-identity-and-spans.json`: binary identity, **all bytes** of the fourth lookup and cold factory, argument counts and the five selected dependency checks below.

PE exception-directory entries split several functions into chained fragments. In particular, the full buffer overflow body is `[0x1a1840, 0x1a19d9)`, bulk write is `[0x1a1e90, 0x1a1f40)`, and string copy is `[0x95110, 0x9521c)`. The first exception-directory fragment's end is not a function return. The evidence explicitly follows the continuation entries and the empty-capacity leaf branch at `0x927d2`, rather than treating the extractor's first entry or first leaf return as complete coverage.

### Fourth lookup boundary

The fourth lookup reads one RCX argument with the same locale-like wrapper layout as the previous two. It constructs outer guard zero at `0x92406`, returning at the observed `0x9240b`. Its cache is `0x233d880`, its facet index is `0x2459ac0`, and its index assignment uses shared counter `0x233b288`. The index-zero path constructs a recursive guard at `0x9242b` and destroys it at `0x92455`. It then searches the supplied implementation, optionally the global implementation returned by `0x1a0c400`, and the cached fallback.

Every successful path joins at `0x924d9`; the outer guard is destroyed at `0x924de`, followed by the sole normal return at `0x924f2`. The explicit failure branch calls `0x923b0` at `0x924f3`. Missing the facet invokes cold factory `0x92090` at `0x924ac`. Registration at `0x924c4`, the lookup's sole indirect call at `0x924cf` (facet slot `+8`), and cache publication at `0x924d2` are reachable only after that cold call. Therefore every path avoiding the cold factory has balanced native guards and no virtual call before normal return.

The complete cold factory `[0x92090, 0x922c5)` reads two arguments: RCX is an output-pointer address and RDX is the locale-like argument. It allocates a 48-byte facet, enters guard zero, saves/sets locale state and collects numeric punctuation data. It installs table `0x1c1f6d8`, stores the grouping string at `+0x10`, decimal/thousands characters at `+0x18/+0x19`, and false/true strings at `+0x20/+0x28`. It restores locale state, frees temporaries, destroys its guard at `0x92293` and normally returns at `0x922ab`. Its error branches and transitive CRT locale/string allocation are excluded by observing this factory's actual entry. They are not inferred safe from a standard-library name.

The fourth helper supports the same conservative scope as the second and third: its entry and **normal** return, with a before-observer at `0x92090` revoking temporary early-release eligibility. Do not use an unlocked cache/index test. The lookup shares unwind record `0x1dca2c0` with the preceding lookups. This corroborates the shape but supplies no permission to release on an abnormal callback: the fault check, cold promotion, nested-scope, crossing and preexisting-ownership rules above still apply. No C++ exception was injected and the compressed unwind maps were not fully decoded.

| Fourth-helper dependency | RVA | Arguments / check bytes |
| --- | --- | --- |
| Scope entry | `0x923f0` | 1 argument; `48 89 5c 24 20 55 56 57 48 83 ec 20` |
| Cold promotion entry | `0x92090` | 2 arguments; `40 55 53 56 57 41 56 48 8d 6c 24 e0 48 81 ec 20 01 00 00` |
| Outer release and sole normal return | `0x924d9` | `48 8d 4c 24 48 e8 e5 a2 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3` |
| Only cold transition | `0x924a4` | `48 8b d5 48 8d 4c 24 50 e8 df fb ff ff` |
| Recursive guard destruction | `0x92450` | `48 8d 4c 24 40 e8 6e a3 97 01` |

### What follows the fourth return

`0x92890` formats the number's sign/base prefix, grouping and padding and writes it to the stream buffer. Before the fourth lookup it calls the second lookup `0x91a70` at `0x92930`, creates a temporary string through `0x91b80`, and invokes the character facet's `+0x38` at `0x92987`. In built-in table `0x1c1f1d8`, that method is `0x91c50`, which copies the supplied range through `0x1a35750`. The fourth lookup call is `0x929a4`.

After `0x929a9`, it calls numeric-punctuation facet `+0x28` at `0x929db` and `+0x20` at `0x92a01`. Built-in table `0x1c1f6d8` resolves these to `0x92580` (copy the grouping string through `0x95110`) and `0x925d0` (load byte `+0x19` and return). It then inserts grouping separators, writes characters with buffer overflow slot `+0x18` as needed, emits remaining padding through `0x927e0`, destroys its temporary strings and returns. There is no later direct facet lookup in this body. The string helpers perform local string allocation, copying and freeing; their complete read bodies contain no numbered guard-zero call.

`0x93010` builds the integer printf format and calls `0x92fb0` before entering `0x92890` at `0x9310a`. After the observed return `0x9310f`, it has only its epilogue. The CRT formatting call behind `0x92fb0` was not exhaustively expanded; it precedes the fourth acquisition, so it is not a hidden next lookup after that return.

`0x93af0` constructs the stream sentry, optionally flushes a distinct tied stream through `0x8fd40`, obtains the third facet through `0x91670` at `0x93b9f`, and invokes facet slot `+0x48` at `0x93c0d`. Built-in table `0x1c1f678` resolves that slot to `0x93010`. After the observed return `0x93c10`, the function updates stream state, checks native exception state through `0x1a0cc40`/`0x1a365ac`, conditionally calls unit-buffer handling `0x8fee0`, calls buffer slot `+0x10`, and returns. Unit-buffer handling calls slot `+0x68` only when its state and flag tests require it. This tail has no additional direct facet lookup.

### The concrete logging stream closes the buffer targets

The captured outer formatter `0x1563650` builds a stack-local string stream through `0x85afc0`, inserts category/file/line/message data, extracts the resulting buffer, appends a newline, sends it to `OutputDebugStringA` (IAT `0x1b66890`) and destroys the stream. The observed integer insertion returns at `0x1563967`; later string insertions use `0x8fa80`, and buffer extraction uses `0x1a1320`. The logging role is also recorded in [localisation-loading.md](localisation-loading.md).

Constructor `0x85afc0` calls initializer `0x1a14f0` with R8 zero, excluding its optional registered-stream lock-two path. It calls buffer constructor `0x1a1460` and installs table `0x1c41c28` at `0x85b080`. Buffer construction calls the already-covered first getter `0x1a0c408` once more, at `0x1a1486`; this explains a short acquisition between the initializer lookups and later numeric insertion. It is not a fifth helper type appearing after the fourth lookup.

| Built-in buffer slot | Target | Read behavior |
| --- | --- | --- |
| `+8`, `+0x10` | `0x955a0` | Immediate `ret 0`; no lock or state access. |
| `+0x18` | `0x1a1840` | Write into available space, or allocate/copy/replace the local buffer; free the old allocation through `0x95620`. No direct numbered guard call. |
| `+0x48` | `0x1a1e90` | Query capacity through leaf `0x927c0`, copy available bytes, or call the same overflow slot; return bytes written. |
| `+0x68` | `0x8f480` | Return zero; no lock or state access. |

The new stream's tied-stream and callback-list pointers are initialized to null, and the read insertion path does not populate them. Therefore the optional tied-stream flush and callback traversal do not introduce another target on this concrete construction path. `0x8fa80` uses the same resolved buffer sentry, overflow, bulk-write and sync slots; it contains no direct facet lookup. `0x1a1320` copies the selected buffer range through `0x95110`.

Cleanup through `0x860350`/`0x1a2010` frees buffer storage using `0x1a1f60`, then releases its locale implementation. Stream-state cleanup `0x1a0ccc0` calls `0x1a0cd38` for the lists and releases its own locale wrapper. The built-in release slot is the already-read atomic decrement `0x92510`; if it reaches zero, a deleting destructor is invoked. These cleanup bodies contain no direct guard-zero acquisition. The zero-count deleting-destructor path, arbitrary callback registrations, concurrent locale replacement and custom facet implementations are not all proven by observing an ordinary freshly built stream, so this is not an unconditional transitive no-lock claim for every possible stream state.

### Bounded conclusion for the next experiment

On the resolved built-in normal path, the concrete lock-zero helper sequence consists of the first getter `0x1a0c408` (including its repeated buffer-construction call), second lookup `0x91a70` (including its repeated formatting call), third lookup `0x91670`, and fourth lookup `0x923f0`. The fourth is the last explicit facet lookup found in the captured integer-formatting chain; no fifth direct lookup was found in its remaining callers or the resolved string-buffer methods. Its own cold factory `0x92090` is a further lock-zero user, but every call to it is explicitly excluded from early release by cold promotion.

This narrows the next measurement to a four-scope candidate without asserting that all later code is free of the lock. Transitive CRT allocator/printf/locale behavior, exceptional dispatch, custom virtual targets and zero-reference deleting destructors were not exhaustively closed. Later lock acquisitions and graphics crossings must continue through the original interception, so any such activity acquires the required protection again. This static result justifies trying the fourth independently checked boundary and examining the next diagnostic trace; it establishes neither a measured P99 improvement nor complete native exception safety.

## Four-scope diagnostic collection failure

The first four-scope run (`diag-d`, PID 104108) completed its formal 10-second paused and 120-second running windows. Paused/running P99 was 7.954/12.726 ms; the running completed-day rate was 617.5/min. Formal events had no losses or failed submissions, and five-second samples observed no other game. However, the running temporary trace filled its 2,097,152-probe capacity and dropped 1,916,959 probes. The driver rejected it as an incomplete mechanism trace, then released the game without a crash. It did not take the post-window screenshot after that assertion failed.

The formal report and truncated raw trace are retained separately; no full interval-attribution replay, complete release count or optimization claim is made from this run. The repeat increases temporary probe capacity to 8,388,608 and uses a 60-second running window. This shorter window is for the mechanism only; final timing comparisons still use 120 seconds and omit the temporary probes entirely.

The first repeat attempt was refused before claiming because another session's test game was running. A subsequent wait/launch attempt was interrupted before collection when it overlapped a build and Windows refused replacing the active task executable. That owned claim was explicitly released, no performance window was collected, and startup/build failure records are retained. Builds and game startup are serialized for the retry.

## Complete four-scope mechanism recheck

The 60-second repeat (`diag-e`, PID 103524) retained complete formal and temporary events, with exact agreement between raw replay and formal interval count/P99. It recorded 43,375 normal scope returns, no unmatched or unfinished scopes, and 41,197 eligible acquisitions, all released early. No cold, fault or promotion event was recorded. The source still excludes those unobserved paths, and state tests cover their retention behavior.

| Helper | Early releases | Hold median / P99 / max, microseconds |
| --- | ---: | --- |
| First getter | 13,273 | 2 / 19 / 220 |
| Second lookup | 15,906 | 0 / 7 / 503 |
| Third lookup | 6,009 | 2 / 9 / 162 |
| Fourth lookup | 6,009 | 0 / 4 / 27 |

Zero-microsecond samples reflect clock quantization, not zero instruction cost. The originally dominant formatting chain no longer retains the presentation lock for the rest of the AI work at these helpers. The remaining largest lock-order source in this window was a different chain through the return address already captured in earlier runs, with 32 holds totaling 184.042 ms and 47.202 ms overlapping the P99 neighborhood. That separate path is not added to the prototype without its own safety analysis.

The P99 neighborhood contained 81 intervals spanning 12.152–67.393 ms. Present occupied 29.31%, drawing work 16.06%, waits after/before Present 18.58%/5.69%, invalidation outside those waits 23.76% and map names due 5.83%. No marked save overlapped this band. This differs from the earlier lock-dominated neighborhoods, but it is not an isolated speed comparison: another game's process appeared in both paused and running samples. Paused/running P99 was 7.957/16.209 ms; the running completed-day rate was 475/min.

The game released without a crash. Its final screenshot showed intact map labels and interface with naturally opened event windows. Only endpoint screenshots were checked, not every rendered frame. Evidence is in `diag-e/`, `traces/103524-*.txt` and the matching replay/lock-source reports. The completed mechanism check permits proceeding to the frozen timing candidate; it does not convert the disturbed diagnostic into an optimization result.

## Interrupted timing comparison

The initial timing baseline (`a1`, PID 100648) began without another game present and completed its paused control at P99 7.657 ms. Another session started a test game during the running window; running P99 was 50.765 ms and completed-day rate 501.5/min. Formal events remained complete and the game released without a crash, but this disturbed baseline cannot establish an isolated paired gain. The driver refused the subsequent `b1` attempt before claiming because that other game remained active.

Retain `a1` and the blocked preflight separately. The replacement sequence is `a1r`, `b1`, `b2`, `a2`. A three-minute quiet preflight was initially scheduled, then canceled before it started a game. The final comparison deliberately permits concurrent games, as requested by the user. Other-game presence is recorded but no longer blocks startup or collection. Results describe this shared-machine condition; they do not claim an isolated hardware comparison.

## Final timing result

The final sequence used the current framework baseline and the four-scope prototype, each with the ordinary performance collector and neither with temporary stage events or entry-stack capture. Each fresh Ming game used seed 123456, the same frozen settings/playset and the approved Beijing view. Each measured ten seconds paused and 120 seconds at speed 5 after settling. The introduction and startup question were closed; later events opened naturally and were not cleared inside the window.

| Run, in execution order | P95, ms | P99, ms | Maximum, ms | Completed days/min | System CPU busy |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline `a1r` | 14.350 | 44.338 | 187.417 | 576.5 | 45.62% |
| Prototype `b1` | 8.137 | 13.166 | 194.759 | 473.5 | 66.54% |
| Prototype `b2` | 7.805 | 13.248 | 116.029 | 579.5 | 33.88% |
| Baseline `a2` | 15.206 | 43.929 | 162.954 | 581.5 | 40.84% |

The paired P99 changes were **-70.31%** (`b1` against `a1r`) and **-69.84%** (`b2` against `a2`). Both prototype windows repeated approximately 13.2 ms, while both baseline windows repeated approximately 44 ms. This is a repeatable P99 improvement in the measured scene, consistent with the independently verified removal of the long formatting-chain ownership.

Completed-day rates changed **-17.87%** and **-0.34%**. The first prototype also had much higher system CPU load, including 85.11% in its paused control versus 26.29% in the first baseline's paused control. CPU is measured system-wide; these observations cannot assign the difference to a particular background process or to the prototype. The second pair shows that this P99 improvement can coexist with nearly unchanged daily progress in the measured scene, but the experiment does not isolate a universal daily-rate cost or guarantee no regression.

Submission rates increased from 122.167 to 134.050/s in the first pair and from 121.600 to 135.317/s in the second. Time-weighted interface-age P99 changed from 118.433 to 134.021 ms and from 109.322 to 96.685 ms. Maximum submission intervals also did not improve consistently. More frequent submissions do not necessarily mean newer interface contents, and this experiment does not eliminate every extreme interval. Paused submission P99 was 7.875, 8.617, 8.361 and 8.675 ms in execution order.

All eight formal windows reached their deadlines with one context segment, zero lost events and zero failed submissions. The four running windows contained 14,659, 16,085, 16,237 and 14,591 completed submission intervals. Their five-second process snapshots observed no other game during collection, despite other games being permitted and one being present at the replacement baseline's preflight. This does not control all background machine work or exclude processes between samples. Settings hashes matched, and each loaded DLL hash matched its frozen baseline or prototype identity. All four games released without a crash. Initial and final screenshots showed no apparent map/text corruption; final event windows were retained, and intermediate frames were not individually inspected.

The narrow question is answered: the four independently checked warm-return boundaries remove the dominant long ownership and substantially reduce submission P99 in this early-game view. Production acceptance still needs broader scene/late-game regression and a clearer daily-rate cost assessment. Native cold paths, custom virtual implementations and exceptions were not dynamically forced. The prototype is retained as a patch and removed from runtime source after validation; no installation or publication is part of this result.

## Candidate checks

The final four-scope timing candidate passed `cargo fmt --all`, `cargo test` (including 799 core tests and 87 xtask tests), both required workspace clippy configurations with warnings denied, release builds, and `cargo xtask offline-check` (five checks each for the ordinary and all-developer-feature configurations). The final test/performance DLL is frozen separately from the diagnostic DLL. `final-validation-results.json`, `final-timing-validation.txt`, `final-timing-build.json` and `final-timing.patch` retain the exact outcomes, identity and source. Temporary event recording, stack capture and the environment mode switch are absent from the timing source; the ordinary performance collector remains enabled.

## Cleanup and retained evidence

The final source patch and every measured DLL identity were saved before restoring the runtime files to HEAD. The prototype module and six disposable DLL copies were removed. The debug and release core package artifacts were cleaned, the ordinary test/performance core rebuilt, and 55 related DLL/EXE/RLIB/PDB artifacts scanned without diagnostic/prototype markers. The runtime source diff is empty. The clean core SHA-256 is `19832c4e3fca55f2535e2ae64dd6481e8a65e412bf74f59f69d8832389ae86a5`.

All eleven owned claims, including the interrupted startup, are absent from the registry; their folders are removed or contain only retained logs. Mutation/game drivers were archived as text, test-only recorder exports and Python caches removed, and offline replay scripts retained. Prior experiments retain their separate cleanup records. No other workspace, installer, service registration or persistent environment setting was created. The installed framework and player files were not changed, and other sessions' games were not stopped.

Static instruction evidence, complete and explicitly truncated timelines, reports, settings, screenshots, source patches, command failures and released-game logs remain under `target/scenarios/frame-p99-boundary/`. `cleanup.json`, `artifact-scan.json`, `clean-build.txt`, `final-game-status.txt` and `document-checks.json` record the checks. The final address-index coverage test passed, along with local-document-link and retained-script syntax checks. No commit, push or publication was performed.


## Production implementation

The measured four-scope behavior is implemented in [drawing/lock_order.rs](../../core/src/drawing/lock_order.rs), with the ownership transitions in [drawing/turns.rs](../../core/src/drawing/turns.rs). It keeps the same native entries, argument counts, checked exit/cold/guard bytes and conservative fault/cold/crossing behavior as the measured timing candidate. The disposable module name, startup labels and test prefixes were replaced with ordinary names; no temporary stage recorder, stack capture or environment switch is included. The nine ownership tests cover successive/nested scopes, existing ownership, crossings, cold construction, faults, frame reentry and day end. The earlier cleanup and run records describe the experiment before this implementation was adopted. The measured percentages remain results of that experiment, not a new all-scene or installed-build benchmark.
