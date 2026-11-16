# Parallel work during an in-game save load

## Result

The best new investigation targets are map-shape preparation, flag-source cache verification, and the private country-distance kernel. Each has an identifiable data-preparation/compute/publication boundary. None has been implemented or benchmarked as a production parallel path in this investigation. Native whole-stage calls are not interchangeable with private compute tasks.

Investigate cheaper serial improvements alongside worker prototypes: repeated map-shape searches, repeated geographic classification inside a country-distance call, and flag checksum byte normalization. Threading the existing redundant work is not necessarily the best implementation.

## Scope and evidence

Investigated 2026-11-16 on the same Windows host and inherited eight-mod playset as [the normal-mode load measurements](../performance.md#normal-mode-loads-and-file-streams-2026-11-16), following `a382d21b`. The installed EU4 1.37.5 executable was identified through REA `open_binary` as SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`. No new Ghidra pseudocode is claimed: the earlier cold query had exceeded the client deadline, and this pass used the repository's PE exception-directory disassembler after `xtask what` lookups. Whole named bodies and selected leaf tails were read; unaudited callbacks and exceptional paths remain explicit below and in the game topics.

Runtime used one claimed hidden game, seed 123456, release with `dev-test-games,dev-call-trace,dev-test-switches`, all speed-ups enabled. The test inputs were the 39,948,416-byte plain Austria save at 1449.7.1, compressed-text England at 1444.11.11, and binary ironman Austria at 1590.11.30. Temporary timing wrapped `physfs::text_snapshot` and logged every flag-setup invocation through the existing observer. These changes were restored, and an uninstrumented release was rebuilt. The game was released without a crash. No installation was changed.

Three in-game loads per format supplied coarse attribution. Another holder's game overlapped the first plain sample; it is excluded from the two plain samples below. The later coarse requests' before/after process inventories found only the owned game, although brief overlap between inventories and unrelated machine activity cannot be excluded. Fine flag/map attribution and later shape captures overlapped another holder's test game; those samples are explicitly bounded attribution, not isolated speedup measurements. No game of another holder was changed or stopped.

The coarse trace timed country setup and its direct phases, country construction/clear, the two cache roots, distance/naval/province modifier rebuilds, ZIP extraction and checksum functions. Fine tracing added map reset/pixel scan, flag atlas/validator and per-file checksum wrappers. Tracing thousands of calls adds observer overhead; nested durations must not be added as independent work. An additional 18-second, 100 Hz stack sample saw 1,716 main-thread samples, with 0.4% suspension overhead. Its window included idle/request-ready time and only part of the repeated-load sequence; it supplies call attribution, not an exact load-time partition.

The paused ordinary-map probe read width 5,632, height 2,048, distance mode 0, 1,380 countries and 4,942 entries in the world's province vector.

## Measured work and candidate boundaries

| Work | Observed elapsed work | Candidate | Assessment |
| --- | --- | --- | --- |
| Map pixel/shape scan | Seven later fine invocations 350–388 ms; one earlier invocation 28.5 ms | Private row runs, then stable per-province application; or bucket first and assign each province to one worker | High priority; investigate repeated-search removal before threading |
| Flag validation | Fine validator 278–312 ms, including existing-file reads/normalization at 243–274 ms | Independent private file preparation followed by ordered MD5, or complete private validation overlapped with earlier reset work | High priority; graphics publication remains serial |
| Country distances across both cache passes | Two coarse plain samples 364–365 ms, 2,760 calls | Target groups inside one country invocation, private minima, original double writes on main thread | Concrete arithmetic boundary; many short calls make scheduling important |
| Naval tables across both passes | Coarse plain 73–75 ms, 2,758 calls; England 75–78 ms | Target-province groups inside one substantial table build | Lower priority for this early save; historical late-game sample was 261 ms |
| Country allocation/setup and definition application | Plain 114–128 ms and 135–142 ms respectively | Private source preparation, deterministic serial publication | Smaller than the inclusive roughly one-second country-setup label suggests |
| Province modifier rebuilding within both cache passes | Plain 129–133 ms, including nested calls | Freeze shared derived inputs before private province computation | Current native functions have demonstrated shared writes; substantial prerequisite work |
| Plain-save snapshot | 8.468 and 8.567 ms for the two non-overlapping coarse plain samples | Prepare bytes beside old-world reset | Structurally plausible, but only millisecond-scale warm-input budget here |
| Compressed-text entry preparation | England's three entries totaled 73.0–76.0 ms, including 19.3–19.6 ms of inflate | One private extraction pipeline beside reset | Modest budget; independent native entry calls share a mutex/cursor |
| Plain-save checksum calculation | 6.35–6.51 ms | Byte-only precomputation, stateful application at original point | Small budget; the complete verification wrapper is not pure computation |

The coarse plain country-setup wrapper was 984–1,008 ms. Its flags were approximately 380 ms in the existing two-decimal log. Fine tracing split flags into atlas setup at 418–437 ms and validation at 278–312 ms, under overlap and additional observers. These are different measurement conditions, not values to subtract into a precise CPU/GPU split. The unrelated texture registration at the wrapper's end was only 0.005–0.006 ms in fine tracing.

For England, entry sizes were 34,259,647 bytes of gamestate, 2,493 of metadata and 116,324 of AI: roughly 99.65% of uncompressed data is one entry. Equal three-way entry jobs are not a balanced split. Extraction time minus inflate includes input reading, CRC32, copying, allocation and validation; it is not a measured CRC-only time. Repeated binary loads also run ironman exit saving and can encounter the background compressor's stored/deflated transitions, so their request times are not used as a clean parallel-input estimate.

## Homogeneous computation

### Map shapes

[Load preparation](../game/program/load-preparation.md#map-pixels-and-retained-province-shapes) records the full reset ordering and the pixel scan's reads/writes. Different rows can belong to one province, so native row workers would race on that province's vector, cached span pointer and bounding box. A private stripe worker can extract ordered province runs and local bounds. A serial merge can apply them in original row/pixel order; alternatively, serial bucketing can assign each existing shape exclusively to a province worker. Preserve old shape contents, native span ordering, negative-pixel behavior and derived presence/area/island data.

Repeated load probes sampled static entry IDs 1, 134, 183, 236, 295, 500 and 1000. Their span counts were 77, 24, 21, 18, 36, 18 and 146; span-byte digests and bounding boxes matched before and after two further loads, and all last-span pointers were null. The native existing-span hit can return without caching that span, making repeated searches a plausible serial target. **These captures were outside the reset**, not at the pixel-scan entry. They do not prove every intervening callback leaves geometry intact or establish that the whole scan may be omitted. The 28.5 ms versus 350–388 ms variation also needs explanation before choosing a performance baseline.

Validate any replacement against complete destination grids, every shape's span sequence, bounds, presence bits, area and island outputs. Start with a serial equivalent and compare at the geometry publication boundary, then vary worker count. Full saves alone do not cover these caches. Join before the native area/island consumers; whole-map-reset overlap with deserialization is not justified.

### Country distances

[The save topic](../game/program/saves.md#distance-computation-and-concurrent-readers) records the valid planar kernel, double table writes and numerical contract. Keep the native outer country order and prepare source/target lists, adjacency, coordinates and capital classifications for **one invocation**. Workers compute target groups into private pairs of minima; the main thread commits in native target order, twice per target. Unsupported map modes, invalid IDs and exceptional arithmetic remain on the native path until covered.

There are 2,760 calls per load, averaging about 130 microseconds of traced work. A 10-microsecond dispatch/join overhead per invocation would consume 27.6 ms. Keep empty/small calls serial, use persistent workers and a measured threshold, and avoid one task per province pair. On the older 0.359-second attribution, perfect four-way scaling of the *entire block* would save at most about 0.269 seconds before serial preparation/publication and overhead. That is an arithmetic ceiling from an instrumented sample, not a prediction.

Before parallel testing, compare every target's two minima against the native result **before either write**, since a later reversed pair can overwrite an earlier mistake. Preserve floating-point precision/environment, integer truncation and u16 narrowing. Geographic classification deduplication within one invocation deserves a separate measurement before scheduling the remaining kernel.

### Naval and province caches

[Naval supply](../game/provinces/trade-range-and-naval-supply.md#the-naval-supply-table) has a viable per-target compute boundary after serial preparation. Retain native source order for ties and a write mask for entries the original leaves unchanged. Dynamic range overrides need a contract or fallback. Keep each country's result published before its following triggered-modifier work; extracting all countries into a new phase requires proving additional reads and writes.

Native province modifier calls have a stronger blocker than an unaudited callee: multiple provinces can explicitly clear/rebuild the same [great-project modifier collections](../game/provinces/institutions-ages-monuments.md#shared-modifier-refresh-during-province-cache-rebuilding). Prewarming does not remove those calls. Shared project preparation must be separated and frozen, and the other callbacks audited, before province workers are justified.

## Overlap between stages

### Flag input preparation beside earlier reset work

[The native flag chain](../game/program/load-preparation.md#flag-cache-verification-is-ordered-input-work) supplies a new cross-stage opportunity with substantially more measured work than the warm plain-save snapshot. It uses a local checksum, but consumes an ordered list of file contents **and paths**. CRLF normalization also has a precise per-file/chunk contract. Parallel per-file MD5s cannot be substituted for this ordered stream.

Two possible designs are private read/normalization jobs followed by main-thread ordered feeding, or a worker-owned validation preparation launched after the flag list/configuration/source versions can be frozen and joined before the atlas decision. The complete current validator also touches native metadata and source resolution, so moving it wholesale is not proved safe. Avoid the shared atlas scratch buffer; keep graphics/texture-manager writes on their established thread. Bound buffered bytes, preserve missing-file/duplicate order and error behavior, and join all jobs before reload, abort, exit or generation destruction. Source mutation between verification and use needs an explicit policy.

A lower-complexity serial candidate is avoiding byte-by-byte copying of chunks without CR, while preserving pending CR across chunk boundaries and solitary CR at EOF. Image data can contain CR bytes; it must not be assumed absent. Measure read, normalization and MD5 costs separately before choosing between this and a worker pipeline.

### Save input beside old-world reset

[Input ordering](../game/program/load-preparation.md#input-preparation-before-the-reset) provides an actual interval: source open precedes reset, description/header reads follow it. A worker borrowing that stream must join **before the header**, not only before deserialization. Preparing a new world's objects beside destruction/recreation of its managers is not the same operation and remains ordered. Keep ironman/cloud on their original paths until source-version and exit-save semantics are established.

The ordinary warm snapshot was only about 8.5 ms; overlapping it cannot hide the roughly 0.9-second parser/world-setup interval. England's private ZIP extraction has a larger roughly 75 ms budget, but the original getters serialize on shared state. Cold storage, larger saves and different formats may change the budget and were not measured here. A bounded future prototype should separate worker read/decode, join delay, copies, parser time and full request time rather than attributing parser improvements to prefetch.

### Whole-stage overlaps not supported

- Reset and object deserialization destroy/create/publish through the same world and registries. Concrete unprotected registration and dynamic-token writes are recorded in [load preparation](../game/program/load-preparation.md#shared-registration-during-save-object-reads).
- The two cache calculations have AI/interface setup and savegame fixes between them. Those mutate calculation inputs; neither wholesale parallel execution nor changing the order is established safe.
- Whole flag/GUI functions retain graphics state and shared scratch/publication. The earlier startup mesh/database overlap actually crashed ([historical evidence](performance-evidence.md#startup-and-loading)); Direct3D's per-call locking does not establish an arbitrary stage boundary.

## Implementation priority and limits

1. Investigate map-shape search reuse and a serial run-based equivalent; then compare stripe/per-province workers if enough work remains.
2. Prototype flag-source read/normalization and ordered verification, with a bounded buffer and exact source-generation ownership. Measure serial normalization improvements too.
3. Build a checked private country-distance kernel, then measure target grouping and thresholds.
4. Revisit naval tables on heavier compatible saves; current early-save budget is smaller.
5. Keep ordinary warm-save prefetch low priority; reconsider private ZIP preparation or genuinely cold input separately.

These priorities are evidence-based research recommendations, not completed concurrency implementations. Do not sum inclusive timings, apply ideal scaling to whole wrappers, or present matching saves/non-crashing runs as proof that a concurrent program has no races. Production candidates need per-boundary differential checks, full cache/state checks, fresh-process timing without competing games/builds, and stress covering their explicitly supported inputs and lifecycle.
