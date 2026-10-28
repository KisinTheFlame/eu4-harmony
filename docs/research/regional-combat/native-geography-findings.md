# Province adjacency, straits and occupied-province checks

2026-10-26. Static baseline SHA256: `97dd63a811487fbe2c917f2c15b1e68f7222bef74a833889cf943c8010049d5e`. All addresses are RVAs; add the actual base of the verified module when calling. This pass read only the frozen exe, existing analysis results and the map text files. It did not attach to the game process and did not change the main source code.

The adjacency types, the land and impassable province flags, and the owner/controller offsets have all been checked against native parsing or saving code. Range distance should be an equal-weight BFS over eligible adjacency edges, each edge counting 1. The native edge cost `+0x0C` and the fort and zone-of-control pathfinding rules play no part in range distance. Access rights and the current strait blockade remain separate conditions for entering a province.

## Adjacency records

Starting from a valid `Province* p` in the current world:

```text
geometry = *(p + 0x28)
begin    = *(geometry + 0x168)
end      = *(geometry + 0x170)
count    = (end - begin) / 0x24
entry[i] = begin + i * 0x24
```

| Record offset | Type | Proven meaning |
| --- | --- | --- |
| `+0x00` | int32 | Target province ID (To) |
| `+0x04` | int32 | Source province ID (From) |
| `+0x08` | int32 | Through province ID; for a strait, the sea zone crossed |
| `+0x0C` | int32 | Base native movement cost; not used by the BFS |
| `+0x18` | packed data | Value related to the CSV coordinates; this task neither interprets nor uses it |
| `+0x1C` | int32 | Adjacency type, see the table below |
| `+0x20` | byte | Zeroed on creation; full meaning not confirmed, so not given a name |

The CSV loader `0x87FA10` reads From, To, Type and Through for each row. It either updates an existing record or creates records in opposite directions for the two ends. On creation it explicitly writes `entry[0]=To`, `entry[1]=From`, `entry[2]=Through`, `entry[7]=type`, and advances the vector end by `0x24`. Updating an existing row also explicitly writes `+8` and `+0x1C`. Separately, `0x96F580` looks up the controllers of the provinces on both shores through the record's `+0` and `+4`, which matches the definition of the two endpoints.

| `+0x1C` | Native CSV type literal | Handling in the range graph |
| --- | --- | --- |
| 0 | Default; any type text that matches nothing falls into 0 | Both ends must be passable land; kept as an ordinary adjacency |
| 1 | `sea` | When both ends are land, an edge across a strait; kept once it passes the current strait check |
| 2 | `river` | Kept when both ends are passable land; counts as 1 step |
| 3 | `impassable` | Excluded |
| 4 | `canal` | Excluded from this land-army range graph |
| Other | Not confirmed in this baseline | Rejected; not guessed to be ordinary land |

The type mapping comes directly from `0x87FA10`'s comparisons against the strings `river`/`sea`/`impassable`/`canal` and the assignments that follow. It is not inferred from how the map looks. The native land-army edge filter `0x96F860` also rejects type 3 directly. Type 4 goes through a separate condition on the controlling country and must not be treated as a strait.

File evidence: `work/native-geography/pseudocode1.txt`; `0x96BD00` in `work/native-path/pseudocode1.txt`, which reads `+0x0C`; `0x96F860` in `work/native-path/pseudocode2.txt`; `0x96F580` in `work/native-geography/pseudocode2.txt`. The CSV rows `151;316;sea;1320` and `123;124;sea;1307` can serve as samples for cross-checking runtime data later. They cannot stand in for the current world's adjacency records, especially with Random New World or map mods.

## Land and impassable province flags

```cpp
bool land = (read_u8(geometry + 0x228) & 0x08) != 0;
bool impassableProvince = (read_u8(geometry + 0x229) & 0x08) != 0;
bool eligibleGeography = land && !impassableProvince;
```

The full static chain of evidence:

1. In the map configuration reader `0x87B080`, token `0x28D7` reads a set of provinces into map manager `+0x440`; token `0x2AAA` reads into `+0x470`.
2. The frozen exe's token table holds `0x28D7 + "sea_starts"` and `0x2AAA + "lakes"`. The token sits in the 4 bytes before the string; the evidence addresses are `0x2087570` and `0x20C23F0` respectively.
3. The map object initialiser `0x87B9E0` sets `+0x228 bit0x10` on each geometry that belongs to lakes; only provinces in neither lakes nor sea_starts get `+0x228 bit0x08`. So bit0x08 is the land flag, and checking only for "not a strait" is not enough.
4. In the climate reader `0x886C00`, token `0x2D23` sets `+0x229 bit0x08` on the geometry of each listed province. Token table entry `0x216B590` is `0x2D23 + "impassable"`.
5. The native access check `0xD447D0` returns false when it meets this bit, and uses the text key `IMPASSABLE_ACCESS`. `0x96F860` also checks this bit before it handles the adjacency type.

File evidence: `work/native-geography/pseudocode2.txt`, `pseudocode5.txt`, `token-evidence.json`. The impassable province flag and a type 3 impassable edge are two separate restrictions; check both.

## Access: reuse the army's HaveAccess and avoid full pathfinding

Slot `+0x120` of the main land-army vtable `0x1C42540` points to `0x1A2280`. Going by the current static call chain, this entry can be used for province access. It does not run the path validation for fort zones of control, so it suits keeping the army's legal access conditions after the geographic filter.

```cpp
using ArmyHaveAccess = uint8_t (__fastcall *)(
    void* army, void* province, uint8_t debug, uint8_t p4,
    uint8_t p5, uint64_t optionalMilitaryTag, void* tooltip,
    uint8_t p8, void* accessCache);

// Matches the observed native rebel-suppression call; AL != 0 means access.
access(army, province, 0, 0, 0, 0x2D2D2D, nullptr, 0, nullptr);
```

`0x2D2D2D` is the empty-Tag argument seen in native calls; unknown parameters must not be turned into bool switches at will. The higher-level names of `p4/p5/p8` are not fully recovered, so keep this observed combination. Pass live objects and use the real Win64 nine-parameter calling convention; a NativeFunction definition with fewer parameters will not do.

The static path:

- `0x1A2280` handles land, special movement states, discovery and similar edge cases, and delegates to `0xD447D0`. The non-land branch may ask about transport ships, so sea and lakes must first be excluded by the geographic test; this function alone does not guarantee "no transport".
- `0xD447D0`'s raw log string `HaveAccess, %s, %d, %d, %d` is at `0x1D1A6F0`. It handles discovery, impassable, the ordinary owning country, related countries and `CONDOTTIERI_ACCESS`, whose string is at `0x1D1A730`. This is direct evidence of the function's purpose.
- `0xD44E00` handles army state, discovery and country relations; its ordinary branch delegates to `0x374C50`.
- `0x374C50` queries the relation `0x28B220` based on the target province's owner/controller and its land/impassable flags. Its reads of the target province involve no forts, ZoC, adjacency path or native movement cost. `0x28B220` goes on to handle access between countries, war, subject and shared permissions and the like; it does not receive the current movement path.
- Full path edge filtering is a separate policy chain, `0x96F860` / `0x96FD60`. Their results must not be used to trim the geographic range here, which is meant to "ignore forts".

The lower-level six-parameter interface is:

```cpp
using CountryProvinceAccess = uint8_t (__fastcall *)(
    void* country, void* province, uint8_t isNavy, uint8_t debug,
    void* tooltip, void* accessCache); // RVA 0x374C50
// (country, province, 0, 0, nullptr, nullptr) queries ordinary land-army access for a country.
```

This lower-level function lacks the upper layer's exceptions for exiled, leased, exploring armies and so on, and should not replace the semantics of the army vslot. Nor can a single relation result from `0x28B220` be taken directly as province access, because the native code also compares owner with controller.

A "query" here means it issues no move and changes no task; it **does not mean zero memory writes**. With cache=null, `0x374C50` increments the diagnostic counter `base+0x2458E70`; the upper layer may initialize TLS and temporarily write back log state. A non-null cache is also written to, so pass null. Call at a confirmed safe point on the native thread, never at high frequency from a remote worker thread. Until a separate experiment confirms that the results match the native interface, the static analysis is not claimed as runtime verification.

Evidence: `0x1A2280` in `work/native-hostility/pseudocode1.txt`; `0xD447D0` in `work/native-geography/pseudocode5.txt`, `0xD44E00`/`0x28B220` in `pseudocode6.txt`, `0x374C50` in `pseudocode7.txt`. `0x1154250` in `pseudocode3.txt` is an unrelated parser ruled out during the search and is not used as geographic evidence.

## Current strait blockade check

```cpp
using IsStraitBlocked = uint8_t (__fastcall *)(
    uint64_t militaryCountryTag, // RCX, by value; do not pass Country* or Tag*
    const void* adjacencyEntry   // RDX, a live 0x24-byte record
);
// RVA 0x96F650; only AL of the return is valid. true means blockaded, false means this check does not block.
```

The observed caller of this function first writes the army's effective country Tag to a temporary buffer through army vslot `+0x138`, then passes that qword **by value**. Do not blindly substitute the player tag for the army's effective military identity, especially in special states such as mercenary or leased.

`0x96F650` enters the fleet check only for type 1 with a valid Tag and a non-zero Through. It first calls `0x96F580(tag, entry)`: when both endpoints' controllers are this tag, or both pass the relation test `0x374A70(controllerCountry, tag, 0x2D2D2D)`, the blockade is skipped. It then resolves the Through sea zone and walks the unit linked list at `province+0x350`, checking vslot `+0x88` (fleet), hostility, the exception for related countries, unit state `+0x203` and a non-zero regiment count. If it finds a qualifying fleet it returns AL=1.

Not every type 1 record can be taken as a "land strait with a non-zero Through". The geographic filter first removes sea, lake and coastal combinations; only the land-land type1 edges about to be kept need the strait check. Through=0 is an explicit not-blockaded branch of this native function: `0x96F691` reads `[entry+8]`, `0x96F695` TESTs it, and `0x96F697` jumps to `XOR AL,AL` at `0x96F7AC`. The helper still calls the native predicate in this case. For a non-zero Through it requires that it index a valid sea zone; the error raised otherwise includes From/To/type/Through and the sea zone's land/lake flags. An unknown type or a record with a bad endpoint still rejects the whole graph.

This chain reads no forts or zones of control and performs no move. Statically it is a read-only relation and unit query. Actual calls should still happen at a confirmed stable point on the game thread, after making sure of the bounds and lifetimes of the entry, the endpoints, Through and the country tag. The full return conditions of `0x96F580` / `0x374A70` cannot be reduced to "we control both shores".

Entry prefixes (16 bytes; no substitute for the full file hash):

```text
0x96F650: 48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48
0x96F580: 48 89 5c 24 08 57 48 83 ec 20 48 8b 05 e7 08 9d
```

Evidence: `work/native-path/pseudocode3.txt` has `0x96F650` instruction by instruction plus its decompilation; `0x96F580` in `work/native-geography/pseudocode2.txt`; `0x374A70` in `pseudocode6.txt`.

## Own provinces under occupation

| Province offset | Type | Static identity |
| --- | --- | --- |
| `+0x468` | CountryTag qword | owner |
| `+0x470` | CountryTag qword | controller |

Direct evidence from saving: the writer `0x9D6E60` takes a country name from `province+0x468` and writes token `0x2841` to the save; it takes a country name from `province+0x470` and writes token `0x2842`. In the frozen exe's token table, `0x20744C0` is `0x2841 + "owner"` and `0x20746C8` is `0x2842 + "controller"`. A separate UI function, `0x1229CF0`, fills the `CONTROLLER` placeholder with the name of the country at `+0x470`, which agrees.

The basic condition is that the owner is the player and the controller is valid and not the player. For "hostile occupation" the controller must also be legally hostile to the player. Control by an ally or another friendly country does not make a province a hostile recapture target just because the controller differs from the owner. Only owner==player is used, so this does not extend to subjects' or allies' territory. CountryTag comparisons in this baseline usually use the country index `int16(tag >> 32)`; first verify that the full Tag and the current country object are still valid. Looking only at the three letters, a raw pointer or an unchecked array index is not enough.

Evidence: `work/native-geography/pseudocode4.txt`, `pseudocode1.txt`, `token-evidence.json`. This report does not guess occupation state from unnamed fields such as `+0x388`. Whether a siege is under way is a separate task state and cannot replace the owner/controller test.

## Building the graph and the limits of validation

Each graph snapshot is tied to the world/session and to the army's effective identity. begin/end must be in order and differ by a whole multiple of `0x24`, and the adjacency count is limited by the total number of provinces. Target and Through are both checked against the current province-ID index table; no old pointer is kept past a load. A record's `From` should match the source province ID; on any anomaly the snapshot is rejected rather than silently pointing at province 0.

Keep edges of type 0/1/2 whose two ends are geographically eligible, then apply the confirmed legal access and the type 1 strait condition on top; the weight is always 1. Do not call the whole native pathfinder with its fort rules to trim the range, and do not use days of movement, straight-line pixel distance or `entry+0x0C` as distance. A real movement target must still be validated by native pathfinding; the range graph cannot grant actual permission to move past forts.

The native calls for an ordinary land army in the current baseline were verified by the isolated experiment below. Random New World instances, map mods and all special army identities are still not covered. The static parser falling back to 0 for unknown type text is existing behavior of this baseline; the adapter can only consume the loaded records and must not reinterpret the original unknown type out of nothing. A current record with an unconfirmed enum value is rejected. The full meaning of adjacency record `+0x20` is still unknown.

## Standalone geography reader helper

`tools/native_geography.js` defines only `createGeographyReader(base)`; it has no attach, hook or RPC. The caller first verifies the full exe hash, then calls it synchronously inside a confirmed UI/game-thread pump:

```js
const geography = createGeographyReader(Process.getModuleByName('eu4.exe').base);
const graph = geography.read({ panel, uiThreadId });
// or geography.read({ army, uiThreadId }); give only one of the two.
```

`uiThreadId` must be a thread ID the caller confirmed from a real UI callback, not the current ID of some worker thread passed off as one. The helper checks that the actual thread equals this ID. Each read fetches the world, country manager, map and army pointers afresh from the globals; no objects or native memory ranges are cached across reads. During a read, the ranges holding memory pages already confirmed readable are cached as "last range + binary search over sorted ranges", which cuts calls to `Process.findRangeByAddress`; the cache is cleared in finally. It does not protect against native lifetime races caused by the caller choosing the wrong moment.

The main task's 5th isolated experiment recorded plainly: the 8-byte request address matched outTag/returnedTag, and the full Tag the native code wrote was `0x10000B900534143`, index=185, yet `Process.findRangeByAddress` returned null. That address belongs to Frida's own `Memory.alloc(8)`, not to the game's country array. So the helper uses direct read + try/catch only for the two Tag output buffers it clearly owns (start and end); every pointer that comes from the game keeps the original readable-range check and the cross-check of a valid Tag against the country object. The evidence was recorded by the main task in `work/native-integration/geographic-preview-05.jsonl`; this subtask did not attach on its own.

More evidence on the military Tag: vslot `+0x138`, `0x1A8D20(army, outTag)`, always returns outTag. The ordinary branch copies `army+0xD0`; the special branch calls `0x1A8B70`, which copies `army+0x220` when the state allows and otherwise outputs the native invalid Tag `0x2D2D2D`. The helper rejects an invalid result and verifies the current country object before passing the Tag by value to `0x96F650`; at the end of the read it fetches the Tag again and compares. Evidence: `work/native-geography/pseudocode8.txt`.

```text
0x1A2280: 48 89 5c 24 08 48 89 6c 24 10 56 57 41 56 48 83
0x1A8D20: 48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9
0x1A8B70: 48 89 5c 24 18 56 48 83 ec 20 80 3d bb 73 19 02
0xD447D0: 48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55
0xD44E00: 48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48
0x28B220: 44 88 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55
```

The result contains armyId (a hex string, so JS does not lose precision on the 64-bit ID), origin, day, world, player, militaryTag, provinces, range_edges and count. provinces is every eligible land province; a province that access rights forbid still has a node but no connections. range_edges follow the direction of the current native records; no reverse edges are invented. Each node carries the id, owned_by_player and occupied_by_hostile fields of an ordinary core snapshot; the last is always false, because this helper does not query hostile control or enemy armies.

This is only a preview graph: `geography_complete:true`, `targets_complete:false`, `orders_permitted:false`. There is no general `complete:true`, and no enemy armies, routes or order state. The caller must not pass it off as a full combat snapshot or use it to issue orders. evidence records these scopes, the placeholder occupation value, the calling thread, and the caveat that native queries may update diagnostic counters.

count records the numbers of provinces, land provinces, accessible provinces, adjacencies and kept edges, plus nativeAccessCalls, nativeStraitCalls, nativeTagCalls, readableCalls, regionQueries and durationMs. At most 50,000 provinces and 1,000,000 raw adjacencies. It checks pointer spans, ID/index, geometry ID, every adjacency enum value and endpoint, and the non-zero Through of each land-land type1 edge about to be kept; any anomaly rejects the whole result. The current selection must be owned by the player and have the confirmed main land-army vtable. Province index 0 must exist and be structurally valid, but never becomes a selectable land node. This strict assumption needs runtime verification; on an incompatible map, analyze the evidence rather than silently ignore bad records.

The read limit stays at 5,000 ms; past it the read throws and never returns a partial graph. Every read exception carries phase and metrics; a readable failure also records the requested address/size, the failing cursor, and the actual region's base/size/protection. reader.status() also gives the statistics of the last success or failure. A graph read requires `world.byte(+0x1E10)==0` at its start and end; at the end it also confirms that the world, player, date, army ID/location/owner/military Tag, map vector, selected object and adjacency storage have not changed. Being on the right thread alone does not prove a stable boundary: `0x758430` sets this byte to 1 on entry to the daily update and clears it before a normal return, and the scheduler measurements show nested manager frames while dailyUpdating=1. Evidence: `docs/native-scheduler-findings.txt` and its runtime addendum on `scheduler-trace-01.jsonl`.

Verification done in this subtask: `node --check tools/native_geography.js`; the 15 synthetic tests in `node tests/test_native_geography.cjs` pass. They cover the geographic filter, the uniform directed graph, access rejection, the effective military Tag for straits and blockades, rejection of unknown types, bad spans and the wrong thread, rejection on a world change, timeouts that report the phase, the memory range cache not crossing reads, type1 coastal edges / Through=0 / an invalid non-zero Through, diagnostics for unreadable game memory addresses, correct reads of Frida's own buffers that the range API cannot see, and rejection when the daily update is already running at the start or begins during the read. The synthetic tests and the game experiment below are recorded separately; synthetic results are not passed off as in-game acceptance.

## Geographic preview passes the runtime check

`work/native-integration/geographic-preview-06.jsonl`, run by the main task, has been checked. The isolated process was PID 43680, its full file hash matched this report's baseline, and it read army ID `0x19D300000036` at current location 149. All preview and restore operations ran on thread 23700.

| Full geography read metric | Measured |
| --- | ---: |
| Raw provinces | 4942 |
| Eligible land nodes | 4027 |
| Raw adjacency records | 23136 |
| Native army access queries | 4027 |
| Nodes with access allowed | 467 |
| Native strait queries | 24 |
| Valid directed range edges | 2072 |
| System memory range queries | 40 |
| Time for one full read | 456 ms |

The host C++ core used this real geography graph with center province 149 to compute radii 3/5/7, giving 28/66/107 provinces respectively. The sets, radii and center in the log's `preview`, `colored` and `enabled` records agree; the main task reported screenshots showing the range in blue and the center in gold. The final record has `core-remained-disabled=true`; the original map mode 1 was restored and the native refresh completed, after which dispose, unload and detach were confirmed complete. The whole experiment never enabled automatic combat or issued a move order.

This confirms that, for an ordinary land army, the chain "native geography data → native legal access/strait queries → equal-weight range in core → preview on the game map" works end to end. The daily-update rejection checks at the start and end were added after this successful experiment, based on the scheduler evidence, and pass the synthetic tests; the new rejection branch itself has not yet been triggered in a game experiment. Full snapshots of enemy armies and occupied targets, automatic orders, special army identities and other maps are outside what this run passed.
