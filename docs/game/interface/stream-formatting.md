# Formatting helpers and stream release boundaries

## The second helper's actual work

Function `0x91a70` spans `[0x91a70, 0x91b79)`. It reads one argument, RCX: a locale-like object whose `+8` points to an implementation with a facet pointer array at `+0x10`, its count at `+0x18`, and a fallback flag at `+0x24`. Calling this a locale facet lookup is an inference from the data flow and surrounding character conversion, not a recovered symbol.

It constructs numbered guard zero at `0x91a86`, through `0x1a0c750`, and keeps that guard at `[rsp+0x48]`. Guard zero enters the critical section at `0x233cdd0` through `0x1a5f504`; its destructor `0x1a0c7c8` leaves the same section through `0x1a5f520` and `LeaveCriticalSection`. This is the lock already identified in the runtime traces.

Under this guard, the helper:

1. Reads cached fallback facet `0x233d878` and facet index `0x233b2a0`.
2. If the index is zero, constructs another guard zero at `[rsp+0x40]`, rechecks the index, increments global counter `0x233b288` and stores the assigned index. It destroys this inner guard at `0x91ad5` before returning to lookup. The outer guard remains held, so this is a recursive acquisition of the same critical section, not a new lock spanning the eventual return.
3. Tries the facet slot in the supplied implementation. If absent and its fallback flag is set, `0x1a0c400` reads the global implementation pointer `0x233b2e0` and lookup tries its slot. That helper is just a global-pointer load and return.
4. Uses the cached fallback if present. Otherwise it calls cold helper `0x918e0`, may throw through `0x923b0`, registers the constructed object through `0x1a0c3c8`, invokes its `+8` virtual method and publishes it in `0x233d878`.
5. Destroys the outer guard at `0x91b5e`, then returns the facet pointer in RAX at the sole normal `ret`, `0x91b72`.

Every normal branch joins at `0x91b59`, before the outer destructor. The only branch away from that common exit is the explicit failure call at `0x91b73`. No game lock acquired directly by the lookup remains held at normal return. Index initialization has already destroyed its recursive guard. Cold-construction and exceptional paths require separate coverage.


## Returned objects and the enclosing initializer

Initializer `0x1a14f0` spans `[0x1a14f0, 0x1a1648)` and reads RCX, RDX and R8b. It initializes its destination stream's fields, obtains the shared locale implementation through first getter `0x1a0c408`, stores that implementation in an allocated wrapper, and retains it again before constructing a stack-local locale argument to the second helper. The first and second calls return at `0x1a1558` and `0x1a1585` respectively.

The implementation built by `0x1a0c2e4` uses virtual table `0x1c1f070`; the fallback facet built by `0x918e0` uses `0x1c1f1d8`. In both built-in tables, `+8` is `0x92500`, a complete leaf consisting of `lock inc dword ptr [rcx+8]` and `ret`. Slot `+0x10` is `0x92510`, an atomic decrement returning the object only when the count becomes zero. These are actual table contents and instructions, not an assumption from a library name.

After the second helper returns, the initializer calls returned-facet slot `+0x40` at `0x1a1591`. In the built-in fallback table this is `0x918b0`, whose whole body returns the incoming byte in DL. It then releases the temporary implementation reference and invokes a deleting destructor only if the decrement returned a non-null object. The stream wrapper still owns the reference obtained from the first getter; the temporary reference is a separate retain/release pair. The built-in fallback also acquired its own retained reference before global publication. Thus normal return from the helper does not hand out a stack object or depend on continuing to hold its guard for the built-in object's lifetime.

This does not prove every possible facet vtable or every caller's object-lifetime discipline. The helper's existing native design already calls its guard destructor before handing the facet to its caller. The prototype changes only the framework's extra presentation ownership at that boundary; it does not drop the game's critical section sooner, alter reference counts, or hold a facet across additional game work. Calls after the observer returns remain under normal lock-order and crossing interception.

Do not extend the scope to the whole initializer on this evidence. The initializer includes subsequent virtual calls, possible deletion, stream error construction and an optional registration call `0x1a0cc48`. That last function takes numbered lock two and updates the global stream registry, independently of the observed lock-zero pair. The broader function has a much larger proof obligation than the lookup's already-balanced guard lifetime.

## Third acquisition: a second facet lookup at 0x91670

The next diagnostic identified native return address `0x9168b` as a later lock-order acquisition after the second helper's scope released. This section reads that helper independently; it does not generalize the two concrete helpers into a rule for all numbered guards. `cargo xtask what 0x91670` first located the earlier first-use-global observation in [loading.md](../program/loading.md). New source evidence is `third-scope.asm`, `third-cold.asm` and `third-identity-and-spans.json` in the original disassembly capture. The JSON retains **all bytes** of both complete function bodies as well as the selected check spans and built-in virtual-table targets.

Function `0x91670` spans `[0x91670, 0x91779)` and reads one argument in RCX. Its control flow is the same pattern as the second lookup, but its fallback cache is `0x233d870` and its facet index is `0x2459ac8`. These globals are not substituted for the second helper's globals, and no prototype read of them is needed.

- It constructs outer guard zero at `0x91686`, using `[rsp+0x48]` as the guard object.
- If its index is uninitialized, it constructs recursive guard zero at `0x916ab`, rechecks the index, assigns the next value from shared counter `0x233b288`, and destroys the inner guard at `0x916d5`.
- It tries the supplied implementation's facet array, optionally the global implementation through already-read leaf `0x1a0c400`, and then its cached fallback. Every successful lookup or cached-fallback branch joins at `0x91759`.
- Missing all three sources invokes cold helper `0x914f0` at `0x9172c`. Only after that call can the function register through `0x1a0c3c8`, invoke the facet's `+8` virtual method at `0x9174f`, and publish `0x233d870`.
- The common exit destroys the outer guard at `0x9175e`, returns the facet in RAX, and executes the sole normal return at `0x91772`. The explicit failure path at `0x91773` calls already-read throwing helper `0x923b0` instead.

The third helper has its own cold factory `0x914f0`, distinct from `0x918e0`. There is no indirect call on any third-helper path that avoids `0x914f0`, and all direct native guards on those paths have been destroyed before the normal return. The recursive index-initialization guard is fully balanced; it does not require an unguarded index precheck or a separate exclusion. This normal-return reading does not establish safety for abnormal return or custom virtual implementations.

The complete cold helper `[0x914f0, 0x91664)` was also read. It reads RCX as the output-pointer address and RDX as the locale-like argument. It allocates 16 bytes, takes guard zero, saves/sets locale through `0x1a0c57c`, initializes a reference-count field and table pointer, writes the output, restores locale through `0x1a0c5e8`, frees its temporary strings and destroys its guard at `0x91634`. Its common normal return is `0x91656`; an invalid locale-name path invokes already-read throw helper `0x1a0c214`. It installs table `0x1c1f678`, whose actual `+8` and `+0x10` slots are the same atomic retain/release leaves `0x92500` and `0x92510`. The cold path contains no additional indirect call itself; the caller's retain is the only indirect call inside the third lookup. Transitive allocator, locale manipulation and exception cleanup are still excluded rather than assumed safe.

The third lookup's PE unwind metadata is exactly the same record `0x1dca2c0` used by the second lookup. This is corroborating compiler metadata, not proof that an after callback is a normal return. The cold helper has its own record. Complete C++ unwind-map decoding and arbitrary exceptional cleanup were not established.

As with the second helper, the native function already leaves its game critical section before returning its facet pointer. Scoped presentation release does not change that native lifetime boundary or its reference counts. It does not prove the semantics of every later virtual method reached by the caller, so later calls must retain normal crossing and lock-order interception. No blanket permission to release at all guard destructors or around a larger stream operation follows from this body.

| Third-helper dependency | RVA | Arguments / check bytes |
| --- | --- | --- |
| Scope entry | `0x91670` | 1 argument; `48 89 5c 24 20 55 56 57 48 83 ec 20` |
| Cold promotion entry | `0x914f0` | 2 arguments; `48 89 5c 24 10 48 89 74 24 18 55 57 41 56` |
| Outer release and sole normal return | `0x91759` | `48 8d 4c 24 48 e8 65 b0 97 01 48 8b c3 48 8b 5c 24 58 48 83 c4 20 5f 5e 5d c3` |
| Only cold transition | `0x91724` | `48 8b d5 48 8d 4c 24 50 e8 bf fd ff ff` |
| Recursive guard destruction | `0x916d0` | `48 8d 4c 24 40 e8 ee b0 97 01` |

These are static facts about the specified built-in functions. Measured release counts and lock lifetimes belong to [performance evidence](../../research/performance-evidence.md#frame-pacing-and-lock-ownership).

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

The captured outer formatter `0x1563650` builds a stack-local string stream through `0x85afc0`, inserts category/file/line/message data, extracts the resulting buffer, appends a newline, sends it to `OutputDebugStringA` (IAT `0x1b66890`) and destroys the stream. The observed integer insertion returns at `0x1563967`; later string insertions use `0x8fa80`, and buffer extraction uses `0x1a1320`. The logging role is also recorded in [localisation-loading.md](../program/localisation-loading.md).

Constructor `0x85afc0` calls initializer `0x1a14f0` with R8 zero, excluding its optional registered-stream lock-two path. It calls buffer constructor `0x1a1460` and installs table `0x1c41c28` at `0x85b080`. Buffer construction calls the already-covered first getter `0x1a0c408` once more, at `0x1a1486`; this explains a short acquisition between the initializer lookups and later numeric insertion. It is not a fifth helper type appearing after the fourth lookup.

| Built-in buffer slot | Target | Read behavior |
| --- | --- | --- |
| `+8`, `+0x10` | `0x955a0` | Immediate `ret 0`; no lock or state access. |
| `+0x18` | `0x1a1840` | Write into available space, or allocate/copy/replace the local buffer; free the old allocation through `0x95620`. No direct numbered guard call. |
| `+0x48` | `0x1a1e90` | Query capacity through leaf `0x927c0`, copy available bytes, or call the same overflow slot; return bytes written. |
| `+0x68` | `0x8f480` | Return zero; no lock or state access. |

The new stream's tied-stream and callback-list pointers are initialized to null, and the read insertion path does not populate them. Therefore the optional tied-stream flush and callback traversal do not introduce another target on this concrete construction path. `0x8fa80` uses the same resolved buffer sentry, overflow, bulk-write and sync slots; it contains no direct facet lookup. `0x1a1320` copies the selected buffer range through `0x95110`.

Cleanup through `0x860350`/`0x1a2010` frees buffer storage using `0x1a1f60`, then releases its locale implementation. Stream-state cleanup `0x1a0ccc0` calls `0x1a0cd38` for the lists and releases its own locale wrapper. The built-in release slot is the already-read atomic decrement `0x92510`; if it reaches zero, a deleting destructor is invoked. These cleanup bodies contain no direct guard-zero acquisition. The zero-count deleting-destructor path, arbitrary callback registrations, concurrent locale replacement and custom facet implementations are not all proven by observing an ordinary freshly built stream, so this is not an unconditional transitive no-lock claim for every possible stream state.


## Other recorded addresses

Recorded sites and partially understood addresses; a listing is not a complete function account.

| RVA | Kind | Known role or scope |
| --- | --- | --- |
| `0x91a3c` | code | Referenced while examining exclude the cold path by execution, not by an unlocked cache read; complete role not established |
| `0x91ad0` | code | Referenced while examining concrete observer and byte-check inputs; complete role not established |
| `0x91b24` | code | Referenced while examining concrete observer and byte-check inputs; complete role not established |
| `0x91b2c` | code | Referenced while examining exclude the cold path by execution, not by an unlocked cache read; complete role not established |
| `0x91b4f` | code | Referenced while examining exclude the cold path by execution, not by an unlocked cache read; complete role not established |
| `0x1a0c910` | function | Referenced while examining exclude the cold path by execution, not by an unlocked cache read; complete role not established |
| `0x1a34e3c` | function | Referenced while examining exceptions and framework ownership; complete role not established |
| `0x1a36038` | function | Referenced while examining exceptions and framework ownership; complete role not established |
